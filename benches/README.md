# PHC benchmarks

CPU-bound microbenchmarks comparing `phc build` output against
handwritten C, PHP, and CPython.

## How to run

```sh
benches/run.sh
```

Each bench directory holds parallel `.phc`, `.c`, `.php`, and `.py`
sources for the same algorithm. The script builds the C and PHC
versions (both via `cc -O2`), then runs each language three times
and reports the best wall-clock per runtime.

## Methodology

- PHC pipeline: `phc build` lexes → parses → typechecks → emits C →
  invokes `cc -std=c11 -O2`. The reference C version is compiled with
  the same flags so the comparison reflects what the PHC front-end
  adds to a vanilla C build, not differences in the C compiler.
- PHP is whichever `php` the shell finds. Recent runs used 8.4.19.
- Python is `python3`. Recent runs used CPython 3.11.15.
- Three runs per language; the best wall-clock is reported. Times
  include process startup. Loops are picked large enough (≥10ms) that
  startup overhead doesn't dominate.

## Sample results

Recorded on a generic cloud-container Linux host. Absolute numbers
will vary; ratios stay roughly stable.

| Benchmark      | C    | PHC  | PHP   | Python | Notes                                              |
| -------------- | ---- | ---- | ----- | ------ | -------------------------------------------------- |
| fib(35)        | 21ms | 21ms | 589ms | 1.2s   | Recursive Fibonacci, no allocation                 |
| sum_sq 1e8     | 64ms | 58ms | 849ms | 9.3s   | Integer loop, tight arithmetic                     |
| list_ops 100K  | ??ms | ??ms | ??ms  | ??ms   | 100K element list creation + closure sort + fold   |
| class_dispatch | ??ms | ??ms | ??ms  | ??ms   | 10M method calls through OOP dispatch              |

For `fib` and `sum_sq`, `objdump -d` shows the PHC and C binaries reach
identical hot-loop machine code (same five instructions for the
`sum_sq` inner loop). The `phc` front-end is zero-cost on these
shapes — once gcc -O2 sees the emitted C, the abstraction
disappears.

`list_ops` and `class_dispatch` exercise allocation, closure calls, and
method dispatch — areas where PHC's runtime overhead will be visible
until the CoW + refcount story (D-022) lands.

## Caveats

- Microbenchmarks only. None of these exercise allocation,
  collections, or class dispatch — those will not be at C parity
  until the runtime's CoW + refcount story (D-022) lands.
- No async or I/O benches yet — async codegen is still a panic stub
  in the compiled backend.
- Signed-overflow UB in the `sum_sq` benches: 1e8 × 1e8 squared
  sums overflow i64. Both PHC and C versions overflow the same
  way and produce the same (garbage) sum, so the comparison is
  fair, but the printed number isn't a real total.
