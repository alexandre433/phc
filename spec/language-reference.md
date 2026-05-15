# PHC v0 — Language Reference

> Status: canonical (Phase 1 complete, amended with D-023 after the initial Phase 1 commit `3df7de5`).
> Supersedes GitHub issue #1.

## 1. Lexical structure

### 1.1 Source files
PHC source files use the `.phc` extension. UTF-8 encoded. Newlines are `\n` or `\r\n`; the lexer normalises to `\n` for span reporting.

### 1.2 Comments
- Line: `// ...` until end of line.
- Block: `/* ... */`. Block comments nest.

### 1.3 Identifiers and sigils (D-023)
- Bare identifiers in ASCII: `[A-Za-z_][A-Za-z0-9_]*`. Unicode identifiers deferred.
- Every variable, parameter, and field reference at the use site carries the **`$` sigil**: `$count`, `$user`, `$this`.
- The declaration site of a parameter or field also carries `$`: `function add(int $a, int $b): int`.
- Top-level type names (`User`, `int`, `result`) do **not** take a sigil.

### 1.4 Keywords
See [`keywords.md`](./keywords.md).

### 1.5 Literals
- Integer: decimal digits with optional `_` separators.
- Float: integer part + `.` + fractional part. Scientific notation deferred.
- Boolean: `true`, `false`.
- Null: `null` (used only with nullable types `T?`).
- String: `"..."`. Every double-quoted string interpolates expressions inside `{ ... }` (D-017). Literal braces: `{{` and `}}`. Backslash escapes are standard (`\"`, `\\`, `\n`, `\t`, ...). Inside `{ ... }` the embedded expressions follow the normal D-023 rules: `"Hello, {$user->name}"`. A raw-string form is deferred.

### 1.6 Operators
See [`operators.md`](./operators.md).

## 2. Types

### 2.1 Casing convention (D-006a)
- **Anything supplied by the language or stdlib is lowercase**: primitives (`int`, `float`, `byte`, `bytes`, `bool`, `string`, `void`), containers (`list`, `map`, `set`, `array`), stdlib traits/types (`result`, `option`, `display`, `from`, `into`, `parseError`, `overflowError`, `taskGroup`).
- **User-defined types stay PascalCase**: `User`, `HttpError`, `Greet`, `Loggable`.

### 2.2 Primitive types (D-022)
| Name | Description |
|------|-------------|
| `int` | Signed integer. Canonical width is 64-bit at boundaries (params, fields, generics, FFI). The compiler may pick narrower storage for local bindings when it can prove the value range. Literals outside `i64` are a compile error. |
| `float` | IEEE-754 binary floating point. Canonical width 64-bit at boundaries; locally narrowed to 32-bit when provable. |
| `byte` | 8-bit unsigned scalar (`0..255`). Fixed width. |
| `bytes` | Owned CoW sequence of `byte`. Container for raw binary data. |
| `bool` | Two-valued boolean. |
| `string` | Owned CoW UTF-8 string (D-004). |
| `void` | Absence of value, only valid as a return type. |

There is **no** `long`, `double`, `decimal`, `short`, `char` keyword in v0.

### 2.3 Static typing
Static typing by default (D-006). Dynamic types are opt-in via `dyn`.

### 2.4 Nullability
Types are non-nullable by default. A nullable type is written `T?`. `null` is the only valid value for a `T?` that is empty.

### 2.5 Generics (D-014)
- Type parameters are listed in angle brackets at the declaration: `function max<T: Ord>(T $a, T $b): T`.
- Bounds appear inline after `:`. Multiple bounds combine with `+`: `<T: Ord + display>`.
- **Call sites never pass type arguments.** All generic arguments are inferred from call/expression context: `max(3, 7)`.
- Type names that already carry parameters (e.g. `map<string, User>` at a binding site) are part of the type, not call-site arguments.
- If inference fails, the user must add type context (a binding annotation or a more specific return position). A turbofish escape hatch may be added later.

## 3. Mutability and borrowing

### 3.1 Local bindings (D-010 + D-023)
Local bindings are declared **type-then-name**; the name carries the `$` sigil. The initialiser is required:

```
<Type> $<name> = <expr> ;
flip <Type> $<name> = <expr> ;
```

- Immutable bindings have no leading keyword.
- Mutable bindings are prefixed with `flip`.
- Forward declaration (no initialiser) is not supported in v0.
- Type inference at the binding site is not supported in v0 (no `var`/`let` form).

```phc
int $count = 42;
flip int $score = 0;
$score := $score + 1;
```

### 3.2 Reassignment
A `flip` binding is reassigned with `:=`. Reassigning an immutable binding is a compile error.

### 3.2a Member assignment (D-005a)
Object-field write uses `=`: `$this->createdAt = instant::now();`, `$user->profile->bio = "...";`. The LHS must be a `->` chain rooted at a `$name` or `$this`. The form writes the target field directly without recursing through any setter hook (which is what makes `$this->field = expr;` inside a `set` hook safe). Bare variable reassignment (`$x = expr;` with no `->`) is **not** a member assignment and remains a compile error — use `:=` per §3.2.

### 3.3 Borrows
- `&$name` — shared borrow (read).
- `&flip $name` — mutable (exclusive) borrow.
- In parameter positions the borrow modifier sits before the type: `&User $user`, `&flip User $user`.

### 3.4 Banned operators
Bitwise assignment operators (`^=`, `&=`, `|=`, etc.) do not exist (D-005).

## 4. Ownership, lifetimes, and copy-on-write
- Memory safety via ownership and borrowing.
- Lifetimes are inferred; explicit lifetime annotations are reserved for advanced cases (syntax deferred).
- Assignment is not move-by-default; value-feel matches PHP for ergonomics.
- Built-in strings and built-in collections use copy-on-write semantics.

## 5. Expressions

### 5.1 Member access (D-023)
PHC has three distinct access operators:

| Operator | Use | Example |
|----------|-----|---------|
| `->` | Instance member access (fields, methods, property hooks). | `$user->name`, `$this->loginCount` |
| `::` | Static access — enum variants, static methods, class constants, stdlib type-level calls. | `Method::Get`, `status::Ok`, `instant::now()` |
| `.` | Path separator only — pack paths and namespaced type references in declarations. | `pack app.auth;`, `use app.http.Request;` |

Construction has **no `new` keyword** — `User("Ada", 30)` is a call.

### 5.2 Precedence
Operator precedence and associativity are documented in [`operators.md`](./operators.md). Highlights:

- Member access (`->`, `::`, `.`), call, and index bind tightest.
- Borrow modifiers (`&`, `&flip`), unary `!` and `-`, and `await` are unary prefixes.
- `as` is a cast operator (D-019).
- Arithmetic → comparison → equality → logical → null-coalesce, in standard C-family order.
- `==` / `!=` and the relational operators are non-chainable.

### 5.3 Lambdas (D-016 + D-023)

```
(<params>) [: <ReturnType>] => <body>
```

- Parameters use the type-then-name shape from D-009 with `$` on names.
- Return type is optional. If absent, the type is inferred from the body; a body that yields no value implies `void`.
- Body is either an expression (its value becomes the lambda's result) or a `{ ... }` block. Blocks use explicit `return`.
- **Captures are automatic.** A free variable used read-only is captured as a shared borrow. A free variable reassigned with `:=` requires the outer binding to be `flip` and is captured as a mutable borrow.

```phc
list<int> $doubled = $nums->map((int $n): int => $n * 2);
$button->onClick(() => Logger::info("clicked"));

int $threshold = 10;
list<int> $big = $nums->filter((int $n): bool => $n > $threshold);

flip int $hits = 0;
$nums->forEach((int $n) => { if ($n > $threshold) $hits := $hits + 1; });
```

## 6. Statements

A statement is one of:

| Form | Notes |
|------|-------|
| `<Type> $<name> = <expr>;` | Local binding (D-010). Immutable. |
| `flip <Type> $<name> = <expr>;` | Mutable binding (D-005, D-010). |
| `$<name> := <expr>;` | Reassignment of a `flip` binding (D-005). |
| `if (<expr>) <block> { else if (<expr>) <block> } [ else <block> ]` | Conditional. Parenthesised condition. |
| `while (<expr>) <block>` | Loop. |
| `for (<Type> $<name> in <expr>) <block>` | Iteration over anything implementing the iterator surface (Phase 6 / D-022). |
| `return [<expr>];` | Function return (D-009). |
| `break;` / `continue;` | Loop control. |
| `<expr>;` | Expression statement; value discarded. Useful for `match` used as a statement, calls with side effects, etc. |

Blocks (`{ ... }`) group statements. They are not expressions — except `match` and lambda bodies, which are explicitly expression-shaped (D-015, D-016).

## 7. Declarations

### 7.1 Functions (D-009 + D-023)

```
[public] [async] function <Name>(<Type> $<name>, ...): <ReturnType> { <statements> }
```

- Keyword: `function`.
- Parameter order: **type-then-name**. The name carries `$`.
- Multiple parameters are comma-separated.
- Return type follows a single `:` between the parameter list and the body.
- The body is a `{}` block of statements terminated by `;`.
- `return <expr>;` yields a value. There is no implicit last-expression return.
- `async function` marks an asynchronous function; the body may use `await`.

```phc
public function add(int $a, int $b): int {
    return $a + $b;
}

async function fetch(string $url): result<bytes, HttpError> {
    // ...
}
```

Default parameter values, variadics, named arguments, and expression-bodied functions are deferred follow-ups.

### 7.2 Classes (D-012 + D-023)

```
[public] class <Name> { <member> ... }
```

Members are fields and methods, in any order.

- **Constructor**: declared with `construct(<params>) { <statements> }`. A class has at most one constructor in v0.
- **Constructor parameter promotion**: a parameter that begins with `public` is promoted to a field with that visibility. A parameter with no visibility marker is init-only — usable inside the constructor body but not stored.
- **Methods**: declared using the normal `function` syntax (D-009). The implicit receiver is `$this`.
- **Fields**: declared inside the body with the local-binding form (D-010): `[<visibility>] [flip] <type> $<name> [= <expr>] [<hook-block>] ;`. Default-visibility fields are pack-scoped. The optional hook block defines `get` / `set` accessors (D-018).
- **Construction**: call the class name like a function: `User("Ada", 30)`. There is no `new` keyword.
- **Inheritance**: forbidden (D-002). Reuse goes through traits/interfaces (D-013).

```phc
public class User {
    construct(
        public string $name,
        int $age,
    ) {
        $this->createdAt = instant::now();
    }

    instant $createdAt;
    flip int $loginCount = 0;

    public function greet(): string {
        return "Hi, {$this->name}";
    }
}

User $u = User("Ada", 30);
$u->loginCount := $u->loginCount + 1;
```

### 7.3 Enums (D-012 + D-023)

```
[public] enum <Name> [: <PrimitiveType>] { <Variant>, ... }
```

- Variants are bare identifiers (no `$` sigil — variants are type-level, not value-level).
- A backing type may be declared with `: <primitive>` (e.g. `: int`). When present, every variant declares an explicit literal: `Ok = 200,`.
- Variants are accessed with `::`: `Method::Get`.
- Tagged-union enums (variants carrying data) are deferred.

```phc
public enum Method { Get, Post, Put, Delete }

public enum Status: int {
    Ok = 200,
    NotFound = 404,
}

Method $m = Method::Get;
Status $s = Status::Ok;
```

### 7.4 Interfaces and traits (D-013 + D-023)

```
[public] interface <Name> { <method-signature>; ... }
[public] trait     <Name> { <method-definition> ... }
```

- An **interface** lists method signatures only; no bodies, no fields.
- A **trait** lists methods with bodies. Traits hold no fields in v0.
- A class declares the interfaces it satisfies on its header: `class User implements Greet, display`.
- A class mixes in traits inside its body with `use <Trait>;` (PHP-style). Multiple `use` lines allowed.
- If two mixed-in traits define the same method name, the class must declare its own method that shadows both — otherwise the compiler errors.

```phc
public interface Greet { function greet(): string; }

public trait Loggable {
    function log(): void { Logger::info($this->toString()); }
}

public class User implements Greet {
    use Loggable;
    construct(public string $name) { }
    public function greet(): string { return "Hi, {$this->name}"; }
}
```

Note: `use` does double duty — at the top of a file it imports from a pack (§10.2); inside a class body it mixes in a trait. Position disambiguates.

### 7.5 Visibility
See §10. The only visibility keyword in v0 is `public` (D-008).

## 8. Pattern matching (D-015 + D-023)

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
| `$<name>` | binds the scrutinee to this name |
| `<EnumName>::<Variant>` | matches the named enum variant |
| `p1 \| p2` | OR-pattern, matches if either does |
| `_` | wildcard, matches anything |

Exhaustiveness:
- Enum scrutinees: compiler requires every variant be covered. No `_` arm necessary.
- Any other scrutinee: a final `_` arm is required.

```phc
int $code = match ($status) {
    Status::Ok => 0,
    Status::NotFound | Status::Gone => 404,
    $s if $s->isServerError() => 500,
    _ => -1,
};
```

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
- Structured concurrency: task groups, parent-child cancellation. Exact syntax in Phase 5 / Phase 6.

## 12. Errors and Result
- Domain failures return `result<T, E>` (exact shape part of D-022).
- Panics are reserved for unrecoverable faults.
- Conversion failures follow D-019: `as` is for proven-total casts only; fallible conversions return `result<T, E>` or `T?` via methods.
- **Postfix `?` propagation (D-006a', 2026-05-15)**: a trailing `?` after any expression of type `result<T, E>` or `option<T>` short-circuits the enclosing function with the failure case (`return result::err(e);` / `return option::none;`) and otherwise yields the success payload. Sits at the tightest precedence level alongside `->`, `::`, `()`, `[]`. The enclosing function's return type must be compatible.

## 13. Standard prelude (provisional, D-022)
The complete prelude is owned by Phase 6. Names referenced elsewhere in this document:

- **Primitives**: `int`, `float`, `byte`, `bytes`, `bool`, `string`, `void`. Polymorphic-storage rules in §2.2.
- **Collections** (CoW): `list<T>`, `map<K, V>`, `set<T>`. `array<T>` is an alias for `list<T>`.
- **Errors**: `result<T, E>`. `T?` covers the optional case; an explicit `option<T>` remains open.
- **Formatting**: `display`-shaped trait drives string interpolation.
- **Conversion**: `from<T>` / `into<T>` shape backs `as` and `.toX()` methods.
- **Async**: `taskGroup`, structured concurrency primitives.

## 14. Tests (provisional, D-021)
Tests are declared in any source file with the `test` keyword. Phase 9 finalises the runtime contract. Example:

```phc
test "addition is commutative" {
    assert::eq(add(2, 3), add(3, 2));
}
```
