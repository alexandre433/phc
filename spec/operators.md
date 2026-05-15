# PHC v0 — Operators

Canonical precedence/associativity table. Grows as expression syntax lands.

## Locked operators

| Operator | Meaning | Notes |
|----------|---------|-------|
| `:=`     | Reassignment to a `flip` binding | D-005. Statement-only in v0. |
| `&`      | Shared borrow (prefix on a `$name`) | D-005, D-023 |
| `&flip`  | Mutable borrow (prefix on a `$name`) | D-005. Two tokens, kept distinct by the lexer. |
| `?`      | Nullable type suffix; postfix Result/Option propagation in expression position | D-006 (amended 2026-05-15). Type-position: marks `T?`. Expression-position: postfix unwrap-or-early-return for `result<T, E>` and `option<T>`. |
| `->`     | Instance member access | D-023. `$obj->field`, `$obj->method()`. |
| `::`     | Static / type-level access | D-023. `Status::Ok`, `User::new()`. |
| `.`      | Path separator only — pack paths, namespaced types | D-023. Never a member-access operator. |
| `$`      | Leading sigil on every variable, parameter, and field reference | D-023. |

## Banned

- `^=`, `&=`, `|=` and any other bitwise-assignment compound operator — D-005.

## Precedence (highest → lowest)

C-family conventional. Locked alongside Phase 1 grammar fill.

| Lvl | Operators | Assoc | Notes |
|-----|-----------|-------|-------|
| 1   | `->`  `::`  `()`  `[]`  postfix `?` | left | instance member, static access, call, index, Result/Option propagation |
| 2   | unary `!`  unary `-`  `&`  `&flip`  `await` | right | borrow modifiers only valid in expression position next to a `$name` |
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
