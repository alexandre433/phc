# PHC user docs

> Audience: programmers who want to **write PHC**, not compiler hackers.
> For the formal language reference, grammar, and design rationale,
> see [`spec/`](../spec/) at the repo root.

PHC is mid-implementation as of 2026-05-16; this guide covers the v0a
surface that the compiler + interpreter + test runner currently
support. Anything described here works in `phc build`, `phc run`, and
`phc test`.

## Where to start

| Doc | What it covers |
|-----|----------------|
| [getting-started.md](./getting-started.md) | Install prereqs, build the toolchain, run your first program, run tests, point an editor at the LSP. |
| [tutorial.md](./tutorial.md) | Linear walkthrough: bindings → control flow → classes & traits → result/option → collections → tests. Each section ends with a runnable `examples/*.phc`. |
| [stdlib-v0a.md](./stdlib-v0a.md) | Cheat-sheet for the stdlib that ships today: strings, `list<T>`, `map<string, V>`, `result<T, E>`, `option<T>`, function types. |
| [cli.md](./cli.md) | Per-subcommand reference: `phc build`, `phc run`, `phc check`, `phc test`, `phc lsp`. Flags, exit codes, examples. |

## Examples corpus

Every `.phc` file in [`../examples/`](../examples/) parses, type-checks,
and borrow-checks cleanly. Each one that defines a `main` also runs
through the interpreter and compiles natively via the codegen path
(both CI-gated); the main-less files are feature snapshots exercised
via `phc check` and the interpreter/typecheck harnesses. Reach for them
when the docs link to a concrete artefact:

| File | Concept |
|------|---------|
| `hello.phc` | Minimal `main()` + `Logger::info`. |
| `bindings.phc` | Immutable + `flip` mutable locals, `:=` reassignment. |
| `borrows.phc` | `&name` shared + `&flip name` mutable borrows. |
| `class.phc` | Class with constructor + interface + trait mixin + `flip` field. |
| `match.phc` | `match` expression with enum exhaustiveness. |
| `result.phc` | `result<T, E>` with postfix `?` propagation. |
| `async.phc` | `async function` + `await` skeleton. |
| `generics.phc` | Generic function with bound + call-site inference. |
| `pack.phc` | `pack` declaration + cross-pack `use`. |
| `string_methods.phc` | D-025 string surface (`len`, `contains`, `trim`, etc.). |
| `list.phc` | D-027 `list<T>` with `push`/`at`/indexing/`for`. |
| `map.phc` | D-028 `map<string, V>` with `set`/`get`/`has`/`len`. |
| `fn_types.phc` | D-024 stored / passed / returned lambdas via `fn(T,U): R`. |
| `test_block.phc` | D-021 v0a `test "name" { ... }` blocks. |

## What is not here yet

These docs cover the **v0a surface**. The following are tracked
follow-ups; do not assume they work today:

- LLVM/inkwell codegen backend (today emits portable C11).
- `set<T>`, generic-key `map<K, V>`, hash-based storage.
- Dedicated assertion helpers in `phc test` (`assert::eq`, etc.).
- Result/Option closure methods (`map`, `andThen`, `okOr`).
- Lambda capture-mode borrowchecks beyond the `:=`/`flip` rule.
- LSP completion / goto-definition / multi-file project analysis.
- Package manager publish flow.

See [`spec/design-decisions.md`](../spec/design-decisions.md) for the
authoritative decision list.
