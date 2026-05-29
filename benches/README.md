# PHC benchmarks

CPU-bound microbenchmarks comparing `phc build` output against
handwritten C, Rust, Go, PHP, and CPython.

## How to run

```sh
benches/run.sh
```

Each bench directory holds parallel `.phc`, `.c`, `.rs`, `.go`, `.php`,
and `.py` sources for the same algorithm. The script builds the C
(`cc -O2`), PHC (`phc build`), Rust (`rustc -O`), and Go (`go build`)
versions, then runs each available runtime three times and reports the
best wall-clock per runtime. Rust and Go are skipped automatically if
`rustc` / `go` are not on `PATH`. A bench that needs a runtime-supplied
input ships an `input.txt`, which is piped to every runtime's stdin.

## Methodology

- PHC pipeline: `phc build` lexes → parses → typechecks → emits C →
  invokes `cc -std=c11 -O2`. The reference C version is compiled with
  the same flags, so the comparison reflects what the PHC front-end
  adds to a vanilla C build, not differences in the C compiler.
- Rust is `rustc -O`; Go is `go build`. Both are native AOT peers and
  are the credible comparison for the "C speed + Rust safety" pitch —
  beating only the interpreters (PHP/Python) proves little.
- PHP is whichever `php` the shell finds; Python is `python3`.
- Three runs per language; the best wall-clock is reported. Times
  include process startup.
- **No loop elision.** Every bench observes its result (prints the
  computed value), so an optimizing compiler can't dead-code-eliminate
  the work. `class_dispatch` additionally reads its loop count from
  stdin and marks the dispatched method non-inlinable in every language
  (`@noinline` in PHC per D-052, `__attribute__((noinline))` in C,
  `#[inline(never)]` in Rust, `//go:noinline` in Go) — otherwise gcc's
  induction-variable pass collapses the whole loop to `value = count`
  and the benchmark measures nothing. See "Caveats" below.

## Sample results (Windows, release, GCC/rustc/go)

Best-of-5 wall-clock on a Windows 11 host, measured 2026-05-29 in a
single pass so the columns are mutually consistent. These include
~60 ms of process-startup overhead, which is a large fraction of the
faster benches on Windows — run on Linux (startup ≈ 1 ms) for clean
small-bench numbers.

| Benchmark          | C    | PHC  | Rust | Go   | PHP    | Python  | Notes                                     |
| ------------------ | ---- | ---- | ---- | ---- | ------ | ------- | ----------------------------------------- |
| fib(35)            | 68ms | 67ms | 71ms | 88ms | 1360ms | 1447ms  | Recursive Fibonacci, no allocation        |
| sum_sq 1e8         | 86ms | 85ms | 54ms | 90ms | 1790ms | 10511ms | Integer loop, tight arithmetic            |
| list_ops 100K      | 53ms | 55ms | 50ms | 53ms | 111ms  | 171ms   | 100K list build + qsort + fold-sum        |
| class_dispatch 10M | 69ms | 61ms | 63ms | 66ms | 518ms  | 977ms   | 10M genuine `@noinline` method dispatches |

PHC tracks C within noise on every bench; the compiled peers (C, PHC,
Rust, Go) cluster together while the interpreters trail by 8–120×. For
`fib` and `sum_sq`, `objdump -d` shows the PHC and C binaries reach
identical hot-loop machine code — the `phc` front-end is zero-cost on
these shapes once gcc -O2 sees the emitted C. For `class_dispatch`,
`objdump` confirms both the C and PHC binaries emit a real `call` to
the (noinline) increment method inside the loop, so all four compiled
languages are measured doing the same 10M genuine dispatches.

## Caveats

- Microbenchmarks only — four CPU-bound shapes, not a representative
  suite. They do not exercise maps/sets, string-heavy work, async, or
  I/O. Collection-mutation throughput will not be at C parity until the
  runtime's CoW + refcount story (D-022) lands.
- No async or I/O benches yet — async codegen is still a panic stub in
  the compiled backend.
- **Signed-overflow in `sum_sq`**: 1e8 i*i squared sums overflow i64.
  The compiled languages (C, PHC, Rust, Go) all wrap two's-complement
  and print the *same* value (`662921401752298880`), so their
  comparison is fair. PHP and Python promote to arbitrary precision and
  print the true sum — a different number by design; their `sum_sq`
  output is not expected to match the compiled set.
- **`class_dispatch` requires `@noinline` to be honest.** With a trivial
  inlinable `value++`, gcc's induction-variable pass collapses the loop
  to `value = count` and constant-folds it — for both handwritten C and
  PHC-emitted C — making the benchmark measure process startup, not
  dispatch. A runtime-sourced loop count does not defeat this; only
  hiding the callee body does. The non-inline pragmas (D-052 for PHC)
  force a genuine call per iteration in all four compiled languages.
- `list_ops` sort uses `phc_list_sort_i64` backed by stdlib `qsort`.
  The thread-local lambda trick is safe until async/parallel lands.
- **Rust `sum_sq` auto-vectorizes**: `objdump` shows LLVM emits an
  SSE2 `paddq` accumulation loop, so Rust's number reflects a
  SIMD-vectorized loop, not the scalar one C/PHC run. It's a real loop
  (verified it scales with iteration count), just not like-for-like
  scalar. `class_dispatch` was verified honest for every compiled
  language (C, PHC, Rust, Go) by differential timing — the loop scales
  linearly with the stdin count (10M→2000M ≈ 200×) — and, for C and
  PHC, by `objdump` showing a surviving `call` to the increment method.
- `run.sh` skips Rust / Go when `rustc` / `go` are absent. The table
  above was measured with all six (go1.26.3, rustc 1.95).
