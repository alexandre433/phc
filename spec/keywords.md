# PHC v0 — Reserved Keywords

Canonical list. Lexer and formatter consume this directly. Grows as design decisions land.

## Locked

| Keyword | Purpose | Source decision |
|---------|---------|-----------------|
| `flip`  | Mark a mutable binding | D-005 |
| `async` | Asynchronous function modifier | D-003 |
| `await` | Suspend on an awaitable expression | D-003 |
| `dyn`   | Opt-in dynamic typing | D-006 |
| `true`  | Boolean literal | — |
| `false` | Boolean literal | — |
| `null`  | Null literal (only valid for `T?`) | D-006 |
| `public`| Marks an item as visible outside its pack | D-008 |
| `function` | Function declaration | D-009 |
| `return` | Yield a value from a function | D-009 |
| `void` | Primitive return type for no-value functions | D-010 |
| `pack` | Pack declaration at top of source file | D-011 |
| `use` | Import items from another pack | D-011 |
| `class` | Class declaration | D-012 |
| `construct` | Constructor declaration inside a class | D-012 |
| `this` | Receiver inside a method or constructor | D-012 |
| `enum` | Enum declaration | D-012 |
| `interface` | Interface declaration (signatures only) | D-013 |
| `trait` | Trait declaration (methods with bodies) | D-013 |
| `implements` | Class header clause listing implemented interfaces | D-013 |
| `match` | Pattern-matching expression | D-015 |
| `if` | Conditional + match-arm guard | D-015 (and C-family conditionals, finalised in Phase 2) |
| `as` | Total/lossless cast | D-019 |
| `get` | Property read hook (contextual) | D-018 |
| `set` | Property write hook (contextual) | D-018 |
| `panic` | Unrecoverable failure (carried from D-007) | D-007 |
| `test` | Test block declaration | D-021 (provisional) |
| `else` | Branch keyword for `if` | Phase 1 grammar fill |
| `while` | Loop | Phase 1 grammar fill |
| `for` | Iteration loop | Phase 1 grammar fill |
| `in` | `for` iterator binder | Phase 1 grammar fill |
| `break` | Loop control | Phase 1 grammar fill |
| `continue` | Loop control | Phase 1 grammar fill |

## Reserved-but-pending

These will be reserved once their owning decision lands. Listed here so lexer work in Phase 2 has visibility.

| Keyword (candidate) | Pending decision |
|---------------------|------------------|
| class / struct / enum keywords | D-012 |
| trait / interface keywords | D-013 |
| import / pack keywords | D-011 |
| match / pattern keywords | D-015 |
| Result-related (`Result`, `Ok`, `Err`, `?` propagation) | D-007 follow-up |
