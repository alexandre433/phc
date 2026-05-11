# PHC v0 — Operators

Canonical precedence/associativity table. Grows as expression syntax lands.

## Locked operators

| Operator | Meaning | Notes |
|----------|---------|-------|
| `:=`     | Reassignment to a `flip` binding | D-005. Not an expression in v0 — TBD whether statement-only. |
| `&`      | Shared borrow (prefix on a name) | D-005 |
| `&flip`  | Mutable borrow (prefix on a name) | D-005. Two tokens, kept distinct by the lexer. |
| `?`      | Nullable type suffix | D-006. Type-position only; expression-position `?` (e.g. Result propagation) TBD. |

## Banned

- `^=`, `&=`, `|=` and any other bitwise-assignment compound operator — D-005.

## Precedence (highest → lowest)

C-family conventional. Locked alongside Phase 1 grammar fill.

| Lvl | Operators | Assoc | Notes |
|-----|-----------|-------|-------|
| 1   | `.`  `()`  `[]` | left | member access, call, index |
| 2   | unary `!`  unary `-`  `&`  `&flip`  `await` | right | borrow modifiers only valid in expression position next to a name |
| 3   | `as` | left | total cast only (D-019) |
| 4   | `*`  `/`  `%` | left | |
| 5   | `+`  `-` | left | also string concatenation for `+` between two strings |
| 6   | `<`  `<=`  `>`  `>=` | left | non-chainable |
| 7   | `==`  `!=` | left | non-chainable |
| 8   | `&&` | left | short-circuit |
| 9   | `\|\|` | left | short-circuit |
| 10  | `??` | right | null-coalesce on a `T?` |

`match` and lambda expressions sit at the top of the expression grammar and are not part of the precedence chain.

`:=` is a statement-level reassignment marker (not an expression operator), see D-005 / D-010.
