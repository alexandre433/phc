# PHC examples

Canonical PHC snippets. They serve three purposes:

1. Developer-facing examples of the implemented v0a syntax surface.
   Linked from [`../docs/tutorial.md`](../docs/tutorial.md) and
   [`../docs/stdlib-v0a.md`](../docs/stdlib-v0a.md).
2. A corpus of parser/typecheck/interp snapshot-test fixtures —
   every file here must lex, parse, resolve, typecheck, and
   borrow-check cleanly. The harnesses are wired into
   `crates/phc-{parser,semantic,typecheck,interp}/tests/`.
3. End-to-end smoke for the codegen path on selected files
   (see `crates/phc-build/src/tests.rs`).

## Files

Pre-Phase-2 corpus (lifted from `spec/language-reference.md`):

| File | Concept |
|------|---------|
| `hello.phc` | Minimal `main()` + `Logger::info`. |
| `bindings.phc` | Immutable + `flip` locals, `:=` reassignment. |
| `borrows.phc` | `&name` shared + `&flip name` mutable borrows. |
| `class.phc` | Class + constructor + traits + property hooks. |
| `match.phc` | `match` expression with enum exhaustiveness. |
| `result.phc` | `result<T, E>` with postfix `?` propagation. |
| `async.phc` | `async function` + `await` skeleton. |
| `generics.phc` | Generic function with bound + call-site inference. |
| `pack.phc` | `pack` declaration + cross-pack `use`. |

Phase 6 + 9 v0a surfaces (added 2026-05-16):

| File | Decision |
|------|----------|
| `string_methods.phc` | D-025 — string method surface. |
| `list.phc` | D-027 — `list<T>` v0a. |
| `map.phc` | D-028 — `map<string, V>` v0a. |
| `fn_types.phc` | D-024 — stored / passed / returned lambdas. |
| `test_block.phc` | D-021 v0a — `test "name" { ... }` blocks. |

## Adding a new example

When you add a file here, also link it from the matching section
of [`../docs/`](../docs/) (tutorial or stdlib cheat-sheet) and
from the relevant section of `spec/language-reference.md` so the
spec, the user docs, and the corpus stay in lock-step.
