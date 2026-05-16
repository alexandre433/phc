# Stdlib v0a cheat-sheet

The v0a stdlib slices that ship today. Names follow D-006a
(lowercase for everything supplied by the language or stdlib).
Method dispatch uses the D-023 operators: `->` for instance
methods, `::` for constructors and enum-style statics.

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
| `$s->toInt()` | `result<int, parseError>` |

Concatenation: `+` between two `string` operands (lowers to
`phc_concat2`). String interpolation: `"hello, {$name}"` (D-017).

Example: [`../examples/string_methods.phc`](../examples/string_methods.phc).

## `list<T>` (D-027 v0a)

```phc
list<int> $xs = list();   // construct empty; T from binding
$xs->push(10);
$xs->push(20);
int $a = $xs->at(0);      // panics on OOB
int $b = $xs[1];          // sugar for ->at
int $n = $xs->len();
for (int $x in $xs) {
    // ...
}
```

| Call | Returns |
|------|---------|
| `list()` | new empty list (type from binding annotation) |
| `$xs->len()` | `int` |
| `$xs->push(v)` | `void` — appends in place |
| `$xs->at(i)` | `T` — panics on out-of-bounds |
| `$xs[i]` | `T` — sugar for `at` |
| `for (T $x in $xs) { ... }` | iterate in insertion order |

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

## `result<T, E>` and `option<T>` (D-026)

Constructors via `::`:

```phc
result<int, string> $ok = result::ok(42);
result<int, string> $err = result::err("nope");
option<int> $some = option::some(7);
option<int> $none = option::none;
```

Methods:

| Call | Returns |
|------|---------|
| `$r->isOk()` | `bool` |
| `$r->isErr()` | `bool` |
| `$r->unwrapOr(T)` | `T` — ok payload or fallback |
| `$o->isSome()` | `bool` |
| `$o->isNone()` | `bool` |
| `$o->unwrapOr(T)` | `T` — some payload or fallback |
| `$o->orElse(option<T>)` | `option<T>` — receiver if some, else the other |

Postfix `?` (D-006a'):

```phc
public function caller(string $raw): result<int, parseError> {
    int $n = $raw->toInt()?;   // short-circuits on err
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
- Nullable carrier: `fn(int): int?` means the function returns
  `int?`. Wrap the carrier with `?` (e.g. via a typed local)
  for a maybe-callback.
- Generic function types (`fn<T>(T): T`) are deferred.

Example: [`../examples/fn_types.phc`](../examples/fn_types.phc).

## Builtins

| Call | Effect |
|------|--------|
| `Logger::info(string)` | Write to stdout with a trailing newline. The placeholder until the `display`-shaped trait + real `print` land. |
| `Http::get(string)` | Stub returning `result::ok("<bytes from {url}>")`. Lets the async examples run without a real client; replaced when the runtime grows real HTTP. |

## Pending surfaces

These are speced in D-022 but not implemented yet:

- `set<T>`, generic-key `map<K, V>`, hash-based storage.
- Result/Option closure methods (`map`, `andThen`, `okOr`,
  `unwrap`). Unblocked by D-024; a follow-up slice will add them.
- `display`-shaped trait driving string interpolation.
- `from<T>` / `into<T>` conversion shapes backing `as` and
  `.toX()` methods.
- `taskGroup` and the structured-concurrency surface.
- Real I/O primitives (`print`, `eprint`, `readLine`).
