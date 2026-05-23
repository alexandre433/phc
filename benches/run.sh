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

    local tmp; tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' RETURN

    if [[ -f "$c_src" ]]; then
        cc -std=c11 -O2 -o "$tmp/c_bin" "$c_src"
    fi
    if [[ -f "$phc_src" ]]; then
        target/release/phc build "$phc_src" -o "$tmp/phc_bin" >/dev/null 2>&1
    fi

    run_label() {
        local cmd="$1"; local label="$2"
        local best=999999
        for r in 1 2 3; do
            local t0; t0="$(date +%s%N)"
            $cmd > /dev/null
            local t1; t1="$(date +%s%N)"
            local elapsed; elapsed="$(awk "BEGIN { printf \"%.3f\", ($t1-$t0)/1e9 }")"
            local cmp; cmp="$(awk -v a="$elapsed" -v b="$best" 'BEGIN { print (a<b)?1:0 }')"
            if [[ "$cmp" == "1" ]]; then best="$elapsed"; fi
        done
        printf "  %-24s best: %s s\n" "$label" "$best"
    }

    if [[ -f "$c_src"   ]]; then run_label "$tmp/c_bin"          "C   (cc -O2)";       fi
    if [[ -f "$phc_src" ]]; then run_label "$tmp/phc_bin"        "PHC (phc build)";    fi
    if [[ -f "$php_src" ]]; then run_label "php $php_src"        "PHP";                fi
    if [[ -f "$py_src"  ]]; then run_label "python3 $py_src"     "Python";             fi
}

for dir in benches/*/; do
    [[ -d "$dir" ]] || continue
    bench_dir "$dir"
done
