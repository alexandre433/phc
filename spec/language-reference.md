# PHC v0 — Language Reference

> Status: in progress (Phase 1). Sections fill in as design decisions land in `design-decisions.md`.
> Once accepted, this document supersedes GitHub issue #1.

## 1. Lexical structure

### 1.1 Source files
PHC source files use the `.phc` extension. UTF-8 encoded. Newlines are `\n` or `\r\n`; the lexer normalises to `\n` for span reporting.

### 1.2 Comments
- Line: `// ...` until end of line.
- Block: `/* ... */`. Block comments nest.

### 1.3 Identifiers
ASCII identifiers in v0: `[A-Za-z_][A-Za-z0-9_]*`. Unicode identifiers deferred.

### 1.4 Keywords
See [`keywords.md`](./keywords.md).

### 1.5 Literals
- Integer: decimal digits with optional `_` separators.
- Float: integer part + `.` + fractional part. Scientific notation TBD.
- Boolean: `true`, `false`.
- Null: `null` (used only with nullable types `T?`).
- String: `"..."`. Every double-quoted string interpolates expressions inside `{ ... }` (D-017). Literal braces: `{{` and `}}`. Backslash escapes are standard (`\"`, `\\`, `\n`, `\t`, ...). A raw-string form is deferred.

### 1.6 Operators
See [`operators.md`](./operators.md).

## 2. Types

### 2.1 Casing convention (D-006a)
- Built-in / primitive types are spelled **lowercase**: `string`, `int`, `float`, `bool`, `bin`, `long`, etc.
- User-defined types (classes, structs, enums, traits, interfaces) are spelled **PascalCase**: `User`, `Result`, `HttpError`.
- The exact list of primitive type names is deferred to the stdlib surface (D-022). Examples in this document use `int`, `string`, `float`, `bool` as working placeholders.

### 2.2 Static typing
Static typing by default (D-006). Dynamic types are opt-in via `dyn`.

### 2.3 Nullability
Types are non-nullable by default. A nullable type is written `T?`. `null` is the only valid value for a `T?` that is empty.

### 2.4 Generics (D-014)
- Type parameters are listed in angle brackets at the declaration: `function max<T: Ord>(T a, T b): T`.
- Bounds appear inline after `:`. Multiple bounds combine with `+`: `<T: Ord + Display>`.
- **Call sites never pass type arguments.** All generic arguments are inferred from call/expression context: `max(3, 7)`.
- Type names that already carry parameters (e.g. `Map<string, User>` at a binding site) are part of the type, not call-site arguments.
- If inference fails, the user must add type context (a binding annotation or a more specific return position). A turbofish escape hatch may be added later.

## 3. Mutability and borrowing

### 3.1 Local bindings (D-010)
Local bindings are declared **type-then-name** with a required initialiser:

```
<Type> <name> = <expr> ;
flip <Type> <name> = <expr> ;
```

- Immutable bindings have no leading keyword.
- Mutable bindings are prefixed with `flip`.
- Forward declaration (no initialiser) is not supported in v0.
- Type inference at the binding site is not supported in v0 (no `var`/`let` form).

### 3.2 Reassignment
A `flip` binding is reassigned with `:=`:

```phc
flip int score = 0;
score := score + 1;
```

Reassigning an immutable binding is a compile error.

### 3.3 Borrows
- `&<name>` — shared borrow (read).
- `&flip <name>` — mutable (exclusive) borrow.
- In parameter positions the borrow modifier sits before the type: `&User user`, `&flip User user`.

### 3.4 Banned operators
Bitwise assignment operators (`^=`, `&=`, `|=`, etc.) do not exist (D-005).

## 4. Ownership, lifetimes, and copy-on-write
- Memory safety via ownership and borrowing.
- Lifetimes are inferred; explicit lifetime annotations are reserved for advanced cases (syntax TBD).
- Assignment is not move-by-default; value-feel matches PHP for ergonomics.
- Built-in strings and built-in collections use copy-on-write semantics.

## 5. Expressions

Operator precedence and associativity are documented in [`operators.md`](./operators.md). Highlights:
- Member access / call / index bind tightest.
- Borrow modifiers (`&`, `&flip`), unary `!` and `-`, and `await` are unary prefixes.
- `as` is a cast operator (D-019).
- Arithmetic → comparison → equality → logical → null-coalesce, in standard C-family order.
- `==` / `!=` and the relational operators are non-chainable.

### 5.1 Lambdas (D-016)

```
(<params>) [: <ReturnType>] => <body>
```

- Parameters use the type-then-name shape from D-009.
- Return type is optional. If absent, the type is inferred from the body; a body that yields no value implies `void`.
- Body is either an expression (its value becomes the lambda's result) or a `{ ... }` block. Blocks use explicit `return`.
- **Captures are automatic.** A free variable used read-only is captured as a shared borrow. A free variable reassigned with `:=` requires the outer binding to be `flip` and is captured as a mutable borrow.

```phc
List<int> doubled = nums.map((int n): int => n * 2);
button.onClick(() => Logger.info("clicked"));

int threshold = 10;
List<int> big = nums.filter((int n): bool => n > threshold);

flip int hits = 0;
nums.forEach((int n) => { if (n > threshold) hits := hits + 1; });
```

## 6. Statements

A statement is one of:

| Form | Notes |
|------|-------|
| `<Type> <name> = <expr>;` | Local binding (D-010). Immutable. |
| `flip <Type> <name> = <expr>;` | Mutable binding (D-005, D-010). |
| `<name> := <expr>;` | Reassignment of a `flip` binding (D-005). |
| `if (<expr>) <block> { else if (<expr>) <block> } [ else <block> ]` | Conditional. Parenthesised condition. |
| `while (<expr>) <block>` | Loop. |
| `for (<Type> <name> in <expr>) <block>` | Iteration over anything implementing the iterator surface (Phase 6 / D-022). |
| `return [<expr>];` | Function return (D-009). |
| `break;` / `continue;` | Loop control. |
| `<expr>;` | Expression statement; value discarded. Useful for `match` used as a statement, calls with side effects, etc. |

Blocks (`{ ... }`) group statements. They are not expressions — except `match` and lambda bodies, which are explicitly expression-shaped (D-015, D-016).

## 7. Declarations

### 7.1 Functions (D-009)

```
[public] [async] function <Name>(<TypeName>, ...): <ReturnType> { <statements> }
```

- Keyword: `function`.
- Parameter order: **type-then-name**, no sigils. Example: `Int count`.
- Multiple parameters are comma-separated.
- Return type follows a single `:` between the parameter list and the body.
- The body is a `{}` block of statements terminated by `;`.
- `return <expr>;` yields a value. There is no implicit last-expression return.
- `async function` marks an asynchronous function; the body may use `await`.

Example:

```phc
public function add(int a, int b): int {
    return a + b;
}

async function fetch(string url): Result<Bytes, HttpError> {
    // ...
}
```

Default parameter values, variadics, named arguments, and expression-bodied functions are deferred follow-ups.

### 7.2 Classes (D-012)

```
[public] class <Name> { <member> ... }
```

Members are fields and methods, in any order.

- **Constructor**: declared with `construct(<params>) { <statements> }`. A class has at most one constructor in v0.
- **Constructor parameter promotion**: a parameter that begins with `public` is promoted to a field with that visibility. A parameter with no visibility marker is init-only — usable inside the constructor body but not stored.
- **Methods**: declared using the normal `function` syntax (D-009). The implicit receiver is the keyword `this`.
- **Fields**: declared inside the body with the local-binding form (D-010): `[flip] <type> <name> [= <expr>] [<hook-block>] ;`. Default-visibility fields are pack-scoped. The optional hook block defines `get` / `set` accessors (D-018).
- **Construction**: call the class name like a function: `User("Alex", 30)`. There is no `new` keyword.
- **Inheritance**: forbidden (D-002). Reuse goes through traits/interfaces (D-013).

```phc
public class User {
    construct(
        public string name,
        int age,
    ) {
        this.createdAt = Time.now();
    }

    Instant createdAt;
    flip int loginCount = 0;

    public function greet(): string {
        return "Hi, " + this.name;
    }
}

User u = User("Alex", 30);
u.loginCount := u.loginCount + 1;
```

### 7.3 Enums (D-012)

```
[public] enum <Name> [: <PrimitiveType>] { <Variant>, ... }
```

- Variants are bare identifiers.
- A backing type may be declared with `: <primitive>` (e.g. `: int`). When present, every variant declares an explicit literal: `Ok = 200,`.
- Variants are accessed with `.`: `Method.Get`.
- Tagged-union enums (variants carrying data) are deferred.

```phc
public enum Method { Get, Post, Put, Delete }

public enum Status: int {
    Ok = 200,
    NotFound = 404,
}
```

### 7.4 Interfaces and traits (D-013)

```
[public] interface <Name> { <method-signature>; ... }
[public] trait     <Name> { <method-definition> ... }
```

- An **interface** lists method signatures only; no bodies, no fields.
- A **trait** lists methods with bodies. Traits hold no fields in v0.
- A class declares the interfaces it satisfies on its header: `class User implements Greet, Display`.
- A class mixes in traits inside its body with `use <Trait>;` (PHP-style). Multiple `use` lines allowed.
- If two mixed-in traits define the same method name, the class must declare its own method that shadows both — otherwise the compiler errors.

```phc
public interface Greet { function greet(): string; }

public trait Loggable {
    function log(): void { Logger.info(this.toString()); }
}

public class User implements Greet {
    use Loggable;
    construct(public string name) { }
    public function greet(): string { return "Hi, " + this.name; }
}
```

Note: `use` does double duty — at the top of a file it imports from a pack (§10.2); inside a class body it mixes in a trait. Position disambiguates.

### 7.4 Visibility
See §10. The only visibility keyword in v0 is `public` (D-008).

## 8. Pattern matching (D-015)

`match` is an expression. It evaluates the scrutinee, tries each arm in order, and yields the result of the first matching arm:

```
match (<scrutinee>) {
    <pattern> [if <guard>] => <expr>,
    ...
}
```

Patterns:

| Pattern | Meaning |
|---------|---------|
| `literal` | matches if equal |
| `Identifier` | binds the scrutinee to this name |
| `Enum.Variant` | matches the named enum variant |
| `p1 \| p2` | OR-pattern, matches if either does |
| `_` | wildcard, matches anything |

Exhaustiveness:
- Enum scrutinees: compiler requires every variant be covered. No `_` arm necessary.
- Any other scrutinee: a final `_` arm is required.

All arms must produce values of the same type when `match` is used as an expression. When used as a statement, the value is discarded.

## 9. Traits and interfaces

See §7.4 for declaration. Method resolution rules:

1. A method defined directly on a class wins.
2. Otherwise, the class must mix in exactly one trait that defines that method. Two traits providing the same name without a class-level override is a compile error.
3. `interface` declarations contribute signatures, not bodies. A class must implement every method of every interface it declares.

## 10. Packs and visibility

### 10.0 Pack manifest (D-020)
Every pack ships with a `phc.json` file in its root. Required fields: `name` (must equal the dotted pack path), `version` (SemVer), `edition` (calendar-year string; `"2026"` in v0). Optional: `authors`, `license`, `repository`, `description`, `dependencies`, `dev-dependencies`.

### 10.1 Packs (D-011)
Every source file starts with a `pack <path>;` declaration. The path is dot-separated:

```phc
pack app.auth;
```

A file belongs to exactly one pack. A pack may contain many files. Pack paths are logical names; the build system computes the pack DAG and enforces acyclicity (D-001) at the pack graph level, not per file.

### 10.2 Imports
Items from other packs are imported with `use`:

```phc
use app.http.Request;
use app.db.{Connection, Pool};
use std.collections.HashMap;
```

Grouped imports use `{}`. Aliasing, wildcard imports, and re-exports are deferred.

### 10.3 Visibility (D-008)
PHC v0 has two visibility levels:

| Marker | Scope |
|--------|-------|
| (none, default) | Pack-scoped — visible to every file in the same pack, invisible outside. |
| `public` | Cross-pack — visible to any pack that imports it. |

"Private" in PHC means "private to the pack", not "private to the file". File-private and friend-pack scopes are not in v0.

## 11. Async and structured concurrency
- `async` marks asynchronous functions; `await` suspends inside one.
- No implicit async promotion.
- Structured concurrency: task groups, parent-child cancellation. Exact syntax TBD.

## 12. Errors and Result
- Domain failures return `Result<T, E>` (exact shape part of D-022).
- Panics are reserved for unrecoverable faults.
- Conversion failures follow D-019: `as` is for proven-total casts only; fallible conversions return `Result<T, E>` or `T?` via methods.

## 13. Standard prelude (provisional)
The exact prelude is owned by Phase 6. Names referenced elsewhere in this document:

- **Primitives**: `int`, `long`, `float`, `bool`, `string`, `bin`, `void`. Widths/signedness open.
- **Collections (CoW)**: `List<T>`, `Map<K, V>`, `Set<T>`.
- **Errors**: `Result<T, E>`. Nullable `T?` covers the option case (Optional alternative open).
- **Formatting**: `Display`-shaped trait drives string interpolation.
- **Conversion**: `From<T>` / `Into<T>` shape backs `as` and `.toX()` methods.
- **Async**: `TaskGroup`, structured concurrency primitives.

## 14. Tests (provisional)
Tests are declared in any source file with the `test` keyword (D-021). Phase 9 finalises the runtime contract. Example:

```phc
test "addition is commutative" {
    assert.eq(add(2, 3), add(3, 2));
}
```
