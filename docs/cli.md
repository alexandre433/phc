# CLI reference

The `phc` binary is the entry point for every PHC workflow. Every
subcommand exits zero on success and non-zero on any failure
(parse / resolve / typecheck / borrowcheck error, runtime panic in
the interpreter, non-zero `cc` exit in the compiler, failing test).

## Global

```sh
phc --help
phc --version
```

Run without a subcommand prints a one-line hint and exits zero.

## `phc build`

Compile a PHC source to a native binary via the C-emit codegen and
the system C compiler.

```sh
phc build [<file>] [-o <output>]
```

Forms:

- **Single file**: `phc build app.phc` writes `app` (or `app.exe`
  on Windows) into the cwd.
- **Project root**: `phc build` with no `<file>` argument looks
  for `phc.json` in the cwd, loads every `.phc` file under the
  root, runs the multi-file pipeline (load → cross-pack `use`
  resolution → pack acyclicity), and emits one binary for the
  whole project. Output stem defaults to the manifest's `name`.

Flags:

| Flag | Meaning |
|------|---------|
| `-o <path>`, `--output <path>` | Override the binary output path. |

Environment:

| Variable | Effect |
|----------|--------|
| `CC` | Picks the C compiler (default `cc`). |

Exit codes:

- `0` — binary produced.
- `1` — pipeline diagnostic, codegen warning that escalated, or
  `cc` failed.

## `phc run`

Run a `.phc` source through the tree-walking interpreter. Faster
iteration than `phc build` because it skips C emission + linking.

```sh
phc run <file>
```

Looks for a `function main(): void` (or `result<...>`) in the file
and evaluates its body. The interpreter walks every implemented
language surface — the same set the codegen supports today, minus
LLVM-specific features (none yet).

Exit codes:

- `0` — `main` returned normally (or `result::ok(_)` / `option::some(_)`).
- `1` — pipeline diagnostic, runtime error, or `main` returned a
  failure variant (`result::err(_)` / `option::none`).
- `2` — file could not be read.

## `phc check`

Run the multi-file pipeline (load + cross-pack `use` resolution +
pack acyclicity) and report diagnostics. No binary output.

```sh
phc check [<root>]
```

`<root>` defaults to the cwd. Useful in CI for a fast "does this
project parse and resolve" gate without paying for codegen.

Exit codes:

- `0` — clean (no errors; warnings still print).
- `1` — at least one error-severity diagnostic.

## `phc test`

Discover every `test "name" { ... }` block in a file and run each
through the interpreter. Sequential, in declaration order. One
test's runtime error does not stop later tests.

```sh
phc test <file>
```

A test **passes** when its body completes without a runtime error.
A test **fails** on any panic (out-of-bounds, divide-by-zero) or
`?` propagation that escapes the body. Dedicated assertion helpers
(`assert::eq`, etc.) are a Phase 9 follow-up; in the meantime,
trigger a panic from any `if (failed) { ... }` branch.

Output:

```
PASS  addition is commutative
FAIL  list at oob
        list index 0 out of bounds (len 0)

phc test: 1 passed, 1 failed
```

Pipeline diagnostics (parse / resolve / typecheck / borrowcheck)
surface before any test runs. When they do, no tests execute:

```
error at 27..29: cannot reassign immutable binding `$x` ...
phc test: setup diagnostics present, no tests run
```

Exit codes:

- `0` — every test passed and setup was clean.
- `1` — at least one test failed, or setup had errors.
- `2` — file could not be read.

## `phc lsp`

Start a Language Server Protocol server over stdio.

```sh
phc lsp
```

Capabilities (v0):

- Text-document sync (full): every `didOpen` / `didChange`
  re-runs the full pipeline and pushes diagnostics.
- Hover: returns the typechecker's recovered `Ty` for the
  smallest expression span containing the cursor.

Out of scope today: completion, signature help, goto-definition,
references, code actions, multi-file project analysis.

Editors spawn this as a long-lived child process and speak LSP
over its stdio. The server runs until the client closes the
connection.

## Stubbed subcommands

The following subcommands exist on the CLI but print "not yet
implemented" and exit non-zero:

- `phc fmt` — formatter (Phase 8 follow-up).
- `phc lint` — linter (Phase 8 follow-up).
- `phc new <name>` — project scaffolder (Phase 7 follow-up).
