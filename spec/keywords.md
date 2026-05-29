# PHC v0 — Reserved Keywords

Canonical list. Lexer and formatter consume this directly.

## Hard keywords

| Keyword | Purpose | Source decision |
|---------|---------|-----------------|
| `flip`  | Mark a mutable binding | D-005 |
| `async` | Asynchronous function modifier | D-003 |
| `await` | Suspend on an awaitable expression | D-003 |
| `dyn`   | Opt-in dynamic typing | D-006 |
| `true`  | Boolean literal | — |
| `false` | Boolean literal | — |
| `null`  | Null literal (only valid for `T?`) | D-006 |
| `public` | Cross-pack visibility marker | D-008 |
| `function` | Function declaration | D-009 |
| `fn` | Function-type heading (`fn(T, U): R`) | D-024 |
| `return` | Yield a value from a function | D-009 |
| `void` | Primitive type for no-value return | D-010 / D-022 |
| `pack` | Pack declaration at top of source file | D-011 |
| `use` | Pack import (top-of-file) / trait mixin (in-class) | D-011, D-013 |
| `class` | Class declaration | D-012 |
| `construct` | Constructor declaration inside a class | D-012 |
| `enum` | Enum declaration | D-012 |
| `interface` | Interface declaration | D-013 |
| `trait` | Trait declaration | D-013 |
| `implements` | Class header clause | D-013 |
| `match` | Pattern-matching expression | D-015 |
| `if` | Conditional / match-arm guard | D-015, grammar fill |
| `else` | Conditional branch | grammar fill |
| `while` | Loop | grammar fill |
| `for` | Iteration loop | grammar fill |
| `in` | `for` iterator binder | grammar fill |
| `break` | Loop control | grammar fill |
| `continue` | Loop control | grammar fill |
| `as` | Total/lossless cast | D-019 |
| `panic` | Unrecoverable failure | D-007 |
| `test` | Test block declaration | D-021 (provisional) |

## Reserved identifier with sigil

| Identifier | Purpose | Source decision |
|------------|---------|-----------------|
| `$this`    | Implicit receiver inside any method / constructor / hook body | D-012, D-023 |

`$this` is the only sigil-prefixed identifier with a reserved meaning. All other `$<name>` forms are user-defined variable / parameter / field references (D-023).

## Contextual keywords

These spell out as ordinary identifiers everywhere except in specific grammar positions.

| Word | Position | Source decision |
|------|----------|-----------------|
| `get` | Property hook block (D-018) | D-018 |
| `set` | Property hook block (D-018) | D-018 |

## Reserved sigils and operators (D-023)

| Token | Role |
|-------|------|
| `$`   | Leading sigil on every variable, parameter, and field reference. |
| `->`  | Instance member access (fields, methods, property hooks). |
| `::`  | Static access (enum variants, static methods, class constants). |
| `.`   | Path separator only (pack paths, namespaced type references). |
| `@`   | Declaration-attribute sigil; heads `@name` (D-052). v0 recognises only `@noinline`. |

## Reserved-but-pending

These will be reserved once their owning decision lands. Listed here so lexer work in Phase 2 has visibility.

| Keyword (candidate) | Pending decision |
|---------------------|------------------|
| `static` | Static methods / class constants — deferred until real code surfaces the need. |
