# Stdlib v0a cheat-sheet

The v0a stdlib slices that ship today. Names follow D-006a
(lowercase for everything supplied by the language or stdlib).
Method dispatch uses the D-023 operators: `->` for instance
methods, `::` for constructors and reserved stdlib namespaces.

See [`spec/design-decisions.md`](../spec/design-decisions.md) for
the locked decision text; this page is a programmer-facing
shortlist.

## Primitives (D-022)

| Type | Canonical width | Notes |
|------|-----------------|-------|
| `int` | i64 at boundaries | Compiler may narrow locally when provable. Literals outside i64 are a compile error. |
| `float` | f64 at boundaries | f32 locally when provable. |
| `byte` | u8 | Fixed width. |
| `bytes` | owned CoW sequence of `byte` | Indexed and iterated as `byte`. |
| `bool` | 1 bit conceptually | Two-valued. |
| `string` | owned UTF-8 | Reference semantics in v0a (CoW deferred). |
| `void` | (none) | Return type only. |

Nullable form: `T?` (e.g. `int?`, `string?`).

## Strings (D-025)

CamelCase methods on `string`. Byte-oriented; ASCII case fold;
`==` and `!=` are byte-wise.

| Call | Returns |
|------|---------|
| `$s->len()` | `int` — UTF-8 byte length. |
| `$s->contains("needle")` | `bool` |
| `$s->startsWith("prefix")` | `bool` |
| `$s->endsWith("suffix")` | `bool` |
| `$s->trim()` | `string` — strip ASCII whitespace at both ends. |
| `$s->upper()` | `string` — ASCII case fold. |
| `$s->lower()` | `string` — ASCII case fold. |
| `$s->toInt()` | `result<int, parseError>` — kept as a legacy shortcut; prefer `int::parse($s)` for new code. |

Concatenation: `+` between two `string` operands (lowers to
`phc_concat2`). String interpolation: `"hello, {$name}"` (D-017).

Example: [`../examples/string_methods.phc`](../examples/string_methods.phc).

## `list<T>` (D-027 / D-030 / D-037 v0a)

```phc
list<int> $xs = list();   // construct empty; T from binding
$xs->push(10);
$xs->push(20);
int $a = $xs->at(0);      // panics on OOB
int $b = $xs[1];          // sugar for ->at
int $n = $xs->len();
for (int $x in $xs) { /* ... */ }

list<int> $doubled = $xs->map((int $n): int => $n * 2);
int $sum = $xs->fold(0, (int $acc, int $n): int => $acc + $n);
```

| Call | Returns |
|------|---------|
| `list()` | new empty list (type from binding annotation) |
| `$xs->len()` | `int` |
| `$xs->push(v)` | `void` — appends in place |
| `$xs->at(i)` | `T` — panics on out-of-bounds |
| `$xs[i]` | `T` — sugar for `at` |
| `for (T $x in $xs) { ... }` | iterate in insertion order |
| `$xs->forEach(fn(T): void)` | `void` — invoke for each element |
| `$xs->map(fn(T): U)` | `list<U>` — transform every element |
| `$xs->filter(fn(T): bool)` | `list<T>` — keep matching elements |
| `$xs->fold(U, fn(U, T): U)` | `U` — thread accumulator (init first) |
| `$xs->any(fn(T): bool)` | `bool` — short-circuit on first true |
| `$xs->all(fn(T): bool)` | `bool` — short-circuit on first false |
| `$xs->find(fn(T): bool)` | `option<T>` — first matching element |

**Reference semantics in v0a**: cloning the handle clones the
pointer, not the storage. Two bindings see each other's
`push`es. The handle's `flip` flag governs reassignment of the
handle, not method-driven mutation — `list<int> $xs` (no
`flip`) can still receive `push`/`at` calls.

Example: [`../examples/list.phc`](../examples/list.phc).

## `map<string, V>` (D-028 v0a)

```phc
map<string, int> $score = map();
$score->set("ada", 95);
if ($score->has("ada")) { /* ... */ }
option<int> $hit = $score->get("ada");
int $n = $score->len();
```

| Call | Returns |
|------|---------|
| `map()` | new empty map (V from binding annotation) |
| `$m->len()` | `int` |
| `$m->has(key)` | `bool` |
| `$m->get(key)` | `option<V>` — `some(v)` on hit, `none` on miss |
| `$m->set(key, value)` | `void` — insert or overwrite |

**v0a restricts keys to `string`.** Linear-scan storage; hash
tables and generic keys are Phase 6 follow-ups. Same reference
semantics as `list<T>`.

Example: [`../examples/map.phc`](../examples/map.phc).

## `set<string>` (D-031 v0a)

```phc
set<string> $s = set();
$s->add("ada");      // true on first insert, false on duplicate
$s->has("ada");
$s->remove("ada");
int $n = $s->len();
```

| Call | Returns |
|------|---------|
| `set()` | new empty set |
| `$s->len()` | `int` |
| `$s->add(key)` | `bool` — true on insert, false if already present |
| `$s->has(key)` | `bool` |
| `$s->remove(key)` | `bool` — true if a key was removed |

Same v0a carve-outs as map: string keys only, linear-scan
storage, reference semantics, `set` is a reserved name.

## `result<T, E>` (D-026 + D-029)

Constructors via `::`:

```phc
result<int, string> $ok = result::ok(42);
result<int, string> $err = result::err("nope");
```

| Call | Returns |
|------|---------|
| `$r->isOk()` | `bool` |
| `$r->isErr()` | `bool` |
| `$r->unwrapOr(T)` | `T` — ok payload or fallback |
| `$r->unwrap()` | `T` — panics on err |
| `$r->map(fn(T): U)` | `result<U, E>` — transform ok branch |
| `$r->andThen(fn(T): result<U, E>)` | `result<U, E>` — chain fallible step |

## `option<T>` (D-026 + D-029)

```phc
option<int> $some = option::some(7);
option<int> $none = option::none;
```

| Call | Returns |
|------|---------|
| `$o->isSome()` | `bool` |
| `$o->isNone()` | `bool` |
| `$o->unwrapOr(T)` | `T` — some payload or fallback |
| `$o->unwrap()` | `T` — panics on none |
| `$o->orElse(option<T>)` | `option<T>` — receiver if some, else the other |
| `$o->map(fn(T): U)` | `option<U>` |
| `$o->andThen(fn(T): option<U>)` | `option<U>` |
| `$o->okOr(E)` | `result<T, E>` — promote none to err |

Postfix `?` (D-006a'):

```phc
public function caller(string $raw): result<int, parseError> {
    int $n = int::parse($raw)?;   // short-circuits on err
    return result::ok($n + 1);
}
```

`?` is legal on any `result<T, E>` or `option<T>` value; the
enclosing function's return type must match.

Examples: [`../examples/result.phc`](../examples/result.phc).

## Function types (D-024)

```phc
fn(int, int): int $add = (int $a, int $b): int => $a + $b;
int $sum = $add(20, 22);

public function pickHandler(bool $loud): fn(string): string {
    if ($loud) { return (string $s): string => $s->upper(); }
    return (string $s): string => $s->lower();
}
```

- Empty params: `fn(): R`.
- Nullable carrier: wrap the carrier with `?` (e.g. via a typed
  local) for a maybe-callback.
- Generic function types (`fn<T>(T): T`) deferred.

Example: [`../examples/fn_types.phc`](../examples/fn_types.phc).

## Numeric namespaces (D-034)

Reserved static-call namespaces on the primitive type names.

```phc
result<int, parseError> $n = int::parse("42");
int $a = int::max(int::abs($x), 10);
result<float, parseError> $f = float::parse("3.14");
if (float::isNaN($f->unwrapOr(0.0))) { /* ... */ }
```

| Call | Returns |
|------|---------|
| `int::parse(string)` | `result<int, parseError>` |
| `int::min(int, int)` | `int` |
| `int::max(int, int)` | `int` |
| `int::abs(int)` | `int` — saturates `INT64_MIN` to `INT64_MAX` |
| `float::parse(string)` | `result<float, parseError>` |
| `float::min(float, float)` | `float` |
| `float::max(float, float)` | `float` |
| `float::abs(float)` | `float` |
| `float::isNaN(float)` | `bool` |

## `io` namespace (D-032)

Real I/O primitives. Compiled binaries hit real stdout/stderr;
the interpreter writes prints into its captured stdout vec and
stubs `readLine` to `option::none` (no attached stdin under
`phc run`).

| Call | Returns |
|------|---------|
| `io::print(string)` | `void` — stdout, no trailing newline |
| `io::println(string)` | `void` — stdout, trailing newline |
| `io::eprint(string)` | `void` — stderr, no trailing newline |
| `io::eprintln(string)` | `void` — stderr, trailing newline |
| `io::readLine()` | `option<string>` — none on EOF |

`Logger::info(string)` is kept as a legacy alias for the existing
example corpus; new code should prefer `io::println`.

## `assert` namespace (D-033)

Test helpers — failure raises a runtime panic, which `phc test`
treats as the test's failure signal.

| Call | Returns |
|------|---------|
| `assert::eq(T, T)` | `void` — panics if not equal |
| `assert::neq(T, T)` | `void` — panics if equal |
| `assert::isTrue(bool)` | `void` — panics if false |
| `assert::isFalse(bool)` | `void` — panics if true |
| `assert::fail(string)` | `void` — always panics with the message |

`assert::eq` / `neq` dispatch on the args' static type: strings
lower to `phc_str_eq`; everything else uses C `==` (works for
primitives + class-instance / lambda identity).

## Builtins (legacy)

| Call | Effect |
|------|--------|
| `Logger::info(string)` | Stdout + trailing newline. Legacy alias for `io::println`. |
| `Http::get(string)` | Stub returning `result::ok("<bytes from {url}>")`. Lets the async examples run without a real HTTP client. |

## Pending surfaces

These are speced in D-022 but not implemented yet:

- Generic-key `map<K, V>`, hash-based storage, set iteration /
  set operations.
- `reduce` (no-init fold), `findIndex`, `take`/`drop`,
  `zip`/`unzip`, `partition` on `list<T>`.
- `forEach` / `keys` / `values` on `map<K, V>`.
- `display`-shaped trait driving string interpolation.
- `from<T>` / `into<T>` conversion shapes backing `as` and
  `.toX()` methods.
- `taskGroup` and the structured-concurrency surface.
- Real HTTP client (replace `Http::get` stub).
