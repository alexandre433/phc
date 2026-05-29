#!/usr/bin/env bash
# Build + time every benchmark across PHC, C, PHP, Python.
# Per-bench layout: benches/<name>/<name>.{phc,c,php,py}.
# PHC uses the local `target/release/phc` binary; C uses `cc -O2`.
# Each runtime gets 3 runs; the best wall-clock is reported.

set -euo pipefail
cd "$(dirname "$0")/.."

if [[ ! -x target/release/phc ]]; then
    echo "building phc release binary..." >&2
    cargo build --release --quiet
fi

bench_dir() {
    local dir="$1"
    local name; name="$(basename "$dir")"
    echo "===== $name ====="
    local phc_src="$dir/$name.phc"
    local c_src="$dir/$name.c"
    local php_src="$dir/$name.php"
    local py_src="$dir/$name.py"
    local rs_src="$dir/$name.rs"
    local go_src="$dir/$name.go"

    # A bench may read a fixed integer from stdin (e.g. class_dispatch
    # reads its loop count so the compiler can't constant-fold it).
    # When present, the same input is piped to every runtime.
    local stdin_file=""
    if [[ -f "$dir/input.txt" ]]; then stdin_file="$dir/input.txt"; fi

    local tmp; tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' RETURN

    if [[ -f "$c_src" ]]; then
        cc -std=c11 -O2 -o "$tmp/c_bin" "$c_src"
    fi
    if [[ -f "$phc_src" ]]; then
        target/release/phc build "$phc_src" -o "$tmp/phc_bin" >/dev/null 2>&1
    fi
    if [[ -f "$rs_src" ]] && command -v rustc >/dev/null 2>&1; then
        rustc -O -o "$tmp/rs_bin" "$rs_src" 2>/dev/null
    fi
    if [[ -f "$go_src" ]] && command -v go >/dev/null 2>&1; then
        go build -o "$tmp/go_bin" "$go_src" 2>/dev/null
    fi

    run_label() {
        local cmd="$1"; local label="$2"
        local best=999999
        for r in 1 2 3; do
            local t0; t0="$(date +%s%N)"
            if [[ -n "$stdin_file" ]]; then $cmd < "$stdin_file" > /dev/null; else $cmd > /dev/null; fi
            local t1; t1="$(date +%s%N)"
            local elapsed; elapsed="$(awk "BEGIN { printf \"%.3f\", ($t1-$t0)/1e9 }")"
            local cmp; cmp="$(awk -v a="$elapsed" -v b="$best" 'BEGIN { print (a<b)?1:0 }')"
            if [[ "$cmp" == "1" ]]; then best="$elapsed"; fi
        done
        printf "  %-24s best: %s s\n" "$label" "$best"
    }

    if [[ -f "$c_src"     ]]; then run_label "$tmp/c_bin"        "C    (cc -O2)";      fi
    if [[ -f "$phc_src"   ]]; then run_label "$tmp/phc_bin"      "PHC  (phc build)";   fi
    if [[ -f "$tmp/rs_bin" ]]; then run_label "$tmp/rs_bin"      "Rust (rustc -O)";    fi
    if [[ -f "$tmp/go_bin" ]]; then run_label "$tmp/go_bin"      "Go   (go build)";    fi
    if [[ -f "$php_src"   ]]; then run_label "php $php_src"      "PHP";                fi
    if [[ -f "$py_src"    ]]; then run_label "python3 $py_src"   "Python";             fi
}

for dir in benches/*/; do
    [[ -d "$dir" ]] || continue
    bench_dir "$dir"
done
