# Getting started with PHC

## 1. Prerequisites

To build the compiler:

- **Rust** stable toolchain (`rustup` recommended). PHC pins via
  [`rust-toolchain.toml`](../rust-toolchain.toml); `cargo` picks it
  up automatically.

To build PHC source into a native binary:

- A **C compiler** on `PATH` (`cc`, `gcc`, or `clang`). The codegen
  emits portable C11 and shells out to the system compiler. On
  Windows, MSYS2's `gcc` or the Visual Studio C++ toolchain both
  work. The `CC` environment variable overrides the default
  (`cc`).

The interpreter (`phc run`, `phc test`) does **not** need a C
compiler — it walks the AST directly.

## 2. Build the toolchain

```sh
git clone https://github.com/alexandre433/phc.git
cd phc
cargo build --release
```

The CLI binary lands at `target/release/phc` (or `phc.exe` on
Windows). Either add `target/release/` to your `PATH` or invoke
it via `cargo run --release --bin phc -- <args>` while iterating.

To run the project's own test suite:

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

All three must pass before merging into `development`.

## 3. Your first program

Create `hello.phc`:

```phc
pack hello;

public function main(): void {
    Logger::info("Hello, PHC!");
}
```

Run it through the interpreter:

```sh
phc run hello.phc
```

Compile it to a native binary:

```sh
phc build hello.phc
./hello       # or .\hello.exe on Windows
```

`phc build` writes the binary to `<stem>` (or `<stem>.exe` on
Windows) in the current directory. Pass `-o <path>` to override
the output location.

## 4. A program with tests

Create `math.phc`:

```phc
pack math;

public function add(int $a, int $b): int {
    return $a + $b;
}

test "addition is commutative" {
    if (add(2, 3) != add(3, 2)) {
        list<int> $oops = list();
        int $_ = $oops->at(99);  // OOB → test fails
    }
}
```

Run the tests:

```sh
phc test math.phc
```

Output:

```
PASS  addition is commutative

phc test: 1 passed, 0 failed
```

The runner exits non-zero on any failure or setup diagnostic, so
CI scripts can branch on outcome. Dedicated `assert::eq` helpers
are a Phase 9 follow-up — for now any panic (out-of-bounds, `?`
propagation that escapes the test body) is the failure signal.

## 5. Editor support

PHC ships with a minimal LSP server. Start it manually with:

```sh
phc lsp
```

It speaks LSP over stdio: diagnostics from parse / resolve /
typecheck / borrowcheck, plus hover types from the typechecker's
recovered `Ty`. Wire your editor to spawn `phc lsp` for `.phc`
buffers. Concrete editor configs are not in the repo yet — see
the README for current status.

## 6. Multi-file projects

A project root contains a `phc.json` manifest (D-020). Minimal
example:

```json
{
    "name": "my_pack",
    "version": "0.1.0",
    "edition": "2026"
}
```

With `phc.json` in the cwd, `phc build` (no file argument) builds
the whole project. `phc check <root>` runs the multi-file pipeline
(load → cross-pack `use` resolution → pack acyclicity) and reports
diagnostics without producing a binary.

## 7. Next

- [tutorial.md](./tutorial.md) — language walkthrough.
- [stdlib-v0a.md](./stdlib-v0a.md) — what the stdlib looks like today.
- [cli.md](./cli.md) — full subcommand reference.
- [`../examples/`](../examples/) — every concept above as a runnable
  `.phc` file.
