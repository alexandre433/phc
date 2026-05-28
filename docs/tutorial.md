# PHC tutorial

A linear walkthrough of the v0a language surface. Each section
ends with a runnable example file under
[`../examples/`](../examples/). Read through with the file open
next to you and run `phc run examples/<file>.phc` after each one.

Every snippet here is real PHC and compiles today. Things that
don't yet work are called out explicitly.

## 1. Files and packs

Every source file starts with a `pack` declaration:

```phc
pack hello;

public function main(): void {
    Logger::info("Hello, PHC!");
}
```

A pack is the smallest unit of visibility. Items default to
**pack-scoped** (visible to every file in the same pack, hidden
outside). Cross-pack visibility is opt-in with `public` (D-008).

Run it:

```sh
phc run hello.phc
phc build hello.phc && ./hello
```

Example: [`hello.phc`](../examples/hello.phc).

## 2. Bindings and mutability

Bindings are immutable by default. The `flip` keyword marks a
mutable binding; `:=` reassigns it. Bare `$x = ...` (no `->`) is
a compile error — that's reserved for `flip` reassignment so a
reader can spot mutation at a glance.

```phc
int $count = 42;          // immutable
flip int $score = 0;      // mutable
$score := $score + 1;
$score := $score + $count;
```

Trying to `$count := 0` is a borrowcheck error:

```
cannot reassign immutable binding `$count` —
declare it with `flip` to allow `:=`
```

Example: [`bindings.phc`](../examples/bindings.phc).

## 3. Borrows

`&$x` is a shared (read) borrow; `&flip $x` is a mutable
(exclusive) borrow. Borrow modifiers sit before the type in
parameter positions:

```phc
function inspect(&int $r): int {
    return $r + 1;
}

function bump(&flip int $w): void {
    // mutate through $w (lambda capture rules apply; see §10)
}
```

A `&flip $x` requires `$x` itself to be `flip`. Aliasing within
a single call is restricted: no two `&flip` of the same root, no
mix of `&` and `&flip` of the same root.

Example: [`borrows.phc`](../examples/borrows.phc).

## 4. Control flow

Standard C-family shapes. Conditions go in parens.

```phc
if ($n < 0) {
    Logger::info("negative");
} else if ($n == 0) {
    Logger::info("zero");
} else {
    Logger::info("positive");
}

flip int $i = 0;
while ($i < 5) {
    $i := $i + 1;
}
```

`for` iterates a `list<T>` (the only iterable surface in v0a):

```phc
list<int> $xs = list();
$xs->push(10);
flip int $sum = 0;
for (int $x in $xs) {
    $sum := $sum + $x;
}
```

`break` and `continue` work inside `while` / `for`.

## 5. Functions

```phc
public function add(int $a, int $b): int {
    return $a + $b;
}

async function fetch(string $url): result<bytes, HttpError> {
    bytes $body = await Http::get($url)?;
    return result::ok($body);
}
```

- Parameter order is **type-then-name** with `$` on the name
  (D-009 + D-023).
- Return type follows a single `:` between the parameter list
  and the body.
- `return <expr>;` yields. No implicit last-expression return.
- `async` marks an asynchronous function; the body may use
  `await`.

Default values, variadics, named arguments, expression-bodied
functions: deferred.

## 6. Lambdas and function types

A lambda is `(params) [: ReturnType] => body`:

```phc
(int $n): int => $n * 2
(int $a, int $b): int => $a + $b
(int $n) => { if ($n > 0) { Logger::info("positive"); } }
```

Captures are automatic: read-only use → captured by shared
borrow; reassignment via `:=` inside the body requires the outer
binding be `flip` (and is captured mutably).

Function types are written `fn(T1, T2, ...): R` (D-024). Lambdas
can be bound, passed, and returned through this type:

```phc
fn(int, int): int $add = (int $a, int $b): int => $a + $b;
int $sum = $add(20, 22);

public function pickHandler(bool $loud): fn(string): string {
    if ($loud) {
        return (string $s): string => $s->upper();
    }
    return (string $s): string => $s->lower();
}
```

Example: [`fn_types.phc`](../examples/fn_types.phc).

## 7. Classes, traits, interfaces

No class inheritance in v0; reuse goes through traits +
interfaces + composition.

```phc
public interface Greet { function greet(): string; }

public trait Loggable {
    function log(): void { Logger::info($this->greet()); }
}

public class User implements Greet {
    use Loggable;
    construct(
        public string $name,
        int $age,
    ) {
        $this->ageNextYear = $age + 1;
    }

    int $ageNextYear;
    flip int $loginCount = 0;

    public function greet(): string {
        return "Hi, {$this->name}";
    }
}

User $u = User("Ada", 30);
$u->loginCount := $u->loginCount + 1;
```

Highlights:

- `construct(...)` is the constructor. A `public` parameter is
  promoted to a field with that visibility; a bare parameter is
  init-only.
- Inside `construct(...)`, `$this->field = expr;` is
  initialisation — allowed even on a non-`flip` field. Outside
  the constructor, `$this->field = expr;` requires the field
  itself to be `flip`.
- Methods use the normal `function` syntax. Receiver is `$this`.
- Traits supply method bodies; `use TraitName;` inside the class
  body mixes them in.

Example: [`class.phc`](../examples/class.phc).

## 8. Enums and `match`

```phc
public enum Method { Get, Post, Put, Delete }

public enum Status: int {
    Ok = 200,
    NotFound = 404,
}

int $code = match ($status) {
    Status::Ok => 0,
    Status::NotFound => 404,
    _ => -1,
};
```

`match` is an expression. Enum scrutinees require every variant
be covered (no `_` necessary). Non-enum scrutinees require a
final `_` arm.

Patterns: literal, `$name` (bind), `Type::Variant`, `p1 | p2`
(OR), `_` (wildcard). Optional `if <guard>`:

```phc
match ($status) {
    Status::Ok => 0,
    $s if $s > 500 => -1,
    _ => -2,
}
```

Example: [`match.phc`](../examples/match.phc).

## 9. `result<T, E>` and `option<T>`

PHC has no exceptions for domain failures. Use
`result<T, E>` to carry the failure type, `option<T>` for the
maybe-empty case.

Constructors via `::`:

```phc
result<int, string> $ok = result::ok(42);
result<int, string> $err = result::err("nope");
option<int> $some = option::some(7);
option<int> $none = option::none;
```

Ergonomic methods (D-026):

```phc
if ($ok->isOk()) { /* ... */ }
int $a = $ok->unwrapOr(0);            // 42
int $b = $err->unwrapOr(99);          // 99
option<int> $alt = $none->orElse(option::some(5));
```

**Postfix `?` (D-006a')** short-circuits the enclosing function
on the failure variant:

```phc
public function caller(string $raw): result<int, parseError> {
    int $n = $raw->toInt()?;   // on err: returns the err verbatim
    return result::ok($n + 1);
}
```

The enclosing function's return type must match the operand's
result/option shape.

Example: [`result.phc`](../examples/result.phc).

## 10. Collections

### Strings

Byte-oriented, ASCII case fold (D-025):

```phc
string $raw = "  Hello, PHC!  ";
string $clean = $raw->trim();
int $n = $clean->len();          // 11
bool $has = $clean->contains("PHC");
string $up = $clean->upper();    // "HELLO, PHC!"
```

`==` and `!=` between two `string` operands are byte-wise.

Example: [`string_methods.phc`](../examples/string_methods.phc).

### `list<T>`

```phc
list<int> $xs = list();
$xs->push(10);
$xs->push(20);
int $a = $xs->at(0);            // panics on OOB
int $b = $xs[1];                // sugar for ->at
int $n = $xs->len();
for (int $x in $xs) { /* ... */ }
```

**Reference semantics in v0a**: two bindings to the same list
see each other's `push`es. The handle's `flip` flag governs
reassignment of the handle, not method-driven mutation — a
non-`flip` `list<int> $xs` can still receive `push`/`at` calls.

Example: [`list.phc`](../examples/list.phc).

### `map<string, V>`

String keys only in v0a (D-028):

```phc
map<string, int> $score = map();
$score->set("ada", 95);
option<int> $hit = $score->get("ada");
int $n = $hit->unwrapOr(0);
```

Example: [`map.phc`](../examples/map.phc).

## 11. Tests

Top-level `test "name" { ... }` blocks. A test passes when its
body completes without a runtime error.

```phc
public function add(int $a, int $b): int { return $a + $b; }

test "addition is commutative" {
    if (add(2, 3) != add(3, 2)) {
        list<int> $oops = list();
        int $_ = $oops->at(99);  // OOB → test fails
    }
}
```

Run via `phc test <file>`:

```
PASS  addition is commutative

phc test: 1 passed, 0 failed
```

Dedicated `assert::eq` helpers are a Phase 9 follow-up; in the
meantime, any panic (out-of-bounds, divide-by-zero, `?`
escaping the body) is the failure signal.

Example: [`test_block.phc`](../examples/test_block.phc).

## 12. Multi-file projects

Files belong to a pack (one pack may span many files). Cross-pack
items are imported with `use`:

```phc
pack app.auth;

use app.http.Request;
use app.db.{Connection, Pool};
```

Pack-scoped items are invisible outside the pack — make them
`public` to export.

Project root carries a `phc.json` manifest (D-020); `phc build`
inside a project root walks the whole tree, resolves cross-pack
`use`s, and emits one binary. `phc check <root>` runs the
pipeline without producing a binary.

Example: [`pack.phc`](../examples/pack.phc).

## 13. What is not in this tutorial

Tracked follow-ups; surfaces here today as a placeholder so you
know not to rely on them:

- Result/Option closure methods (`map`, `andThen`, `okOr`).
  Unblocked by D-024; will land in a later slice.
- `set<T>`, generic-key `map<K, V>`, hash-based storage.
- Lambda capture-mode borrowcheck (only `:=` is enforced today).
- Assertion helpers in `phc test`.
- LSP completion / goto-definition / multi-file project analysis.
- LLVM/inkwell codegen backend (today emits portable C11).

See [`docs/stdlib-v0a.md`](./stdlib-v0a.md) for what does ship,
[`docs/cli.md`](./cli.md) for every subcommand, and
[`spec/`](../spec/) for the formal reference.
