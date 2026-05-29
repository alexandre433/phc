# PHC Language Specification

This directory holds the formal language specification for PHC v0.

## Status

> ✅ Phase 1 complete (2026-05-11). This directory is the **canonical** design source for PHC v0. It supersedes GitHub issue #1.
>
> Implementation slices since Phase 1 lock have introduced amendments and Phase-6 / Phase-9 surfaces (D-024 through D-028, D-021 v0a). Every change is recorded in `design-decisions.md`'s decision list.

## Files

| File | Description |
|------|-------------|
| [`grammar.ebnf`](./grammar.ebnf) | Formal EBNF grammar |
| [`language-reference.md`](./language-reference.md) | Developer-facing language reference |
| [`design-decisions.md`](./design-decisions.md) | Rationale for every locked and provisional decision (D-001…D-052 as of 2026-05-29) |
| [`keywords.md`](./keywords.md) | Reserved keyword table |
| [`operators.md`](./operators.md) | Operator precedence / associativity table |

## Phase status

- **Phase 1 (#2)** — closed. All locked decisions and the formal grammar live here.
- **Phase 2 (#3)** lexer/parser — implemented. Consumes this directory as input.
- **Phase 3 (#4)** typecheck + borrowcheck — MVP shipped (D-005 / D-005a / D-012 enforcement, plus D-005 aliasing extension and method-call return-type inference).
- **Phase 5 (#6)** codegen + runtime — C-emit path live; Result/Option/`?` (C6), inline lambdas (C5a), stored lambdas via D-024, shared + `&flip` borrow codegen with caller write-back (D-005), `??` null-coalesce for nullable references (D-051), `@noinline` declaration attribute (D-052). Every main-bearing `examples/*.phc` is native-build CI-gated.
- **Phase 6 (#7)** stdlib — D-025 strings, D-026 result/option methods + D-029 closure forms, D-027 list + D-030 closure methods + D-037 fold/any/all/find, D-028 map, D-031 set, D-032 io namespace, D-034 numeric namespaces.
- **Phase 8 (#9)** tooling — LSP minimal via `phc lsp`, formatter via `phc fmt` (D-035), linter starter ruleset via `phc lint` (D-036).
- **Phase 9 (#10)** test framework — MVP via D-021 v0a; `phc test <file>` discovers and runs `test "name" { ... }` blocks, with assert helpers via D-033.
- D-021 (test syntax) and D-022 (stdlib core surface) remain provisional; the v0a slices above are concrete cuts of those decisions, not their final form.
