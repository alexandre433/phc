# PHC Language Specification

This directory holds the formal language specification for PHC v0.

## Status

> ✅ Phase 1 complete (2026-05-11). This directory is the **canonical** design source for PHC v0. It supersedes GitHub issue #1.

## Files

| File | Description |
|------|-------------|
| [`grammar.ebnf`](./grammar.ebnf) | Formal EBNF grammar |
| [`language-reference.md`](./language-reference.md) | Developer-facing language reference |
| [`design-decisions.md`](./design-decisions.md) | Rationale for every locked and provisional decision (D-001…D-022) |
| [`keywords.md`](./keywords.md) | Reserved keyword table |
| [`operators.md`](./operators.md) | Operator precedence / associativity table |

## Phase status

- **Phase 1 (#2)** — closed. All locked decisions and the formal grammar live here.
- **Phase 2 (#3)** lexer/parser work consumes this directory as input.
- D-021 (test syntax) and D-022 (stdlib core surface) are intentionally provisional and will be finalised in Phase 9 and Phase 6 respectively.
