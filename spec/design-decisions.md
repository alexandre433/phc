# PHC v0 — Design Decisions

Canonical record of every design decision. Each entry: **Decision**, **Alternatives considered**, **Rationale**, **Date**, **Status** (`locked` or `provisional`).

Once Phase 1 is accepted this file supersedes GitHub issue #1 as the source of truth.

---

## Locked decisions (carried forward from issue #1)

### D-001 — Module system: packs
- **Decision**: Modules are called *packs*. Packs are acyclic. Items are private by default; public items must be explicitly declared.
- **Alternatives considered**: Module / namespace / crate naming; cyclic deps allowed.
- **Rationale**: Acyclic packs enable pack-level caching and parallel compilation (a hard project goal). Private-by-default reduces accidental API surface.
- **Date**: pre-2026-05-11 (issue #1).
- **Status**: locked.

### D-002 — No class inheritance in v0
- **Decision**: No `extends`. Reuse via classes + interfaces + traits + composition.
- **Alternatives considered**: Single inheritance (Java/PHP); multiple inheritance (C++).
- **Rationale**: Avoids fragile-base-class problems; traits + interfaces cover the reuse cases inheritance is typically (mis)used for.
- **Date**: pre-2026-05-11 (issue #1).
- **Status**: locked.

### D-003 — Explicit async
- **Decision**: `async` / `await` are explicit. No implicit promotion. Structured concurrency with task groups and parent-child cancellation.
- **Alternatives considered**: Implicit async (Go-style goroutines); coloured-function avoidance.
- **Rationale**: Explicitness aids reasoning about cancellation and I/O scheduling; structured concurrency prevents orphan tasks.
- **Date**: pre-2026-05-11 (issue #1).
- **Status**: locked.

### D-004 — Memory model: ownership + borrowing, no GC
- **Decision**: Ownership + borrowing. Lifetimes inferred by default. Not move-by-default — assignment is PHP-like value-feel. Built-in strings and collections use copy-on-write.
- **Alternatives considered**: Tracing GC; reference counting only; Rust-style move-by-default.
- **Rationale**: Matches the "C-like speed, Rust-like safety, PHP-like ergonomics" project goal.
- **Date**: pre-2026-05-11 (issue #1).
- **Status**: locked.

### D-005 — Mutability syntax
- **Decision**: Immutable by default. `flip` keyword for mutable bindings. `:=` for reassignment. `&name` for shared borrow. `&flip name` for mutable borrow. No bitwise assignment operators (`^=`, `&=`, `|=`).
- **Alternatives considered**: `mut`, `var`, `let mut`, `=` for reassignment.
- **Rationale**: `flip` is distinctive and matches the playful identity; `:=` visually separates initialisation from reassignment.
- **Date**: pre-2026-05-11 (issue #1).
- **Status**: locked.
- **Amended by D-005a (2026-05-15)**: see below.

### D-005a — Member assignment with `=`
- **Decision**: `<lhs> = <expr>;` is a real Statement when `<lhs>` is a `->` chain rooted at a `$name` or `$this` (e.g. `$this->createdAt = instant::now();`, `$user->profile->bio = "...";`). The form writes the target field directly without recursing through any setter hook, matching the D-018 hook-setter exception text and extending it to every block.
- **Out of scope**: bare `$x = expr;` (no `->`) stays illegal. To overwrite a variable use `:=` after declaring `$x` as `flip` per D-005.
- **Alternatives considered**: keep `:=` everywhere (forces `flip` on write-once fields, awkward for constructors); add an `init` block (extra syntax for one-time init); restrict `=` to constructor body only (special case).
- **Rationale**: PHP-like ergonomics for object initialisation in constructors and arbitrary write paths, while D-005's `:=` discipline still applies to plain variables.
- **Date**: 2026-05-15.
- **Status**: locked.
- **Subsumes**: the D-018 hook-setter sentence about `$this->field = expr;`.

### D-006 — Type system: static by default, non-nullable by default
- **Decision**: Static typing by default. `dyn` keyword for dynamic opt-in. All types non-nullable unless suffixed `?`.
- **Alternatives considered**: Gradual typing; dynamic by default; nullable by default.
- **Rationale**: Compile-time safety; explicit nullability prevents null-pointer bugs.
- **Date**: pre-2026-05-11 (issue #1).
- **Status**: locked.

### D-006a' — Postfix `?` for Result/Option propagation
- **Decision**: A trailing `?` after any expression of type `result<T, E>` or `option<T>` is a postfix operator at the tightest precedence level (alongside `->`, `::`, `()`, `[]`). On `result::ok(v)` / `option::some(v)` it yields `v`; on `result::err(e)` / `option::none` it short-circuits the enclosing function with the equivalent failure value (`return result::err(e);` or `return option::none;`). The enclosing function's return type must be compatible.
- **Alternatives considered**: keep deferred until Phase 6 stdlib lock; require explicit `match`; macro-style `try!`.
- **Rationale**: Result-style error handling (D-007) is heavily used; without `?` even simple chained calls require nested matches. Lifting the deferral now keeps Phase 2 examples readable and matches Rust's well-trodden ergonomics.
- **Date**: 2026-05-15 (lifts the deferral originally noted in D-006 / spec/operators.md).
- **Status**: locked.
- **Follow-ups**: exact `result` / `option` shapes still owned by D-022 (Phase 6); the `?` token is reused in type-position for `T?` nullable, with no ambiguity because postfix `?` only appears in expression context.

### D-006a — Type name casing convention
- **Decision**: **Anything supplied by the language or its standard library is spelled lowercase**: primitives (`int`, `float`, `bool`, `string`, `byte`, `bytes`, `void`), containers (`list`, `map`, `set`, `array`), stdlib traits (`display`, `from`, `into`), stdlib error/result types (`result`, `option`, `parseError`, `overflowError`). **User-defined types stay PascalCase** (e.g. `User`, `HttpError`, `Greet`, `Loggable`). The visual distinction is the rule: "is this name shipping with the compiler?" → lowercase. "Did I author this in my own pack?" → PascalCase.
- **Rationale**: At a glance, every type name in source tells the reader whether it is part of the language surface or part of the user's code. Matches the PHP feel for primitives, keeps user-authored classes capitalised in the Java/PHP tradition.
- **Date**: 2026-05-11 (amended after initial Phase 1 lock).
- **Status**: locked.
- **Follow-ups**: the canonical primitive widths and stdlib name surface live in D-022.

### D-007 — Error handling: Result + panics
- **Decision**: Result-style for expected/domain failures. Exceptions/panics only for unrecoverable faults. No unchecked exceptions for normal control flow.
- **Alternatives considered**: Pervasive checked exceptions; pervasive unchecked exceptions.
- **Rationale**: Result is composable and explicit; panics reserved for true invariant violations.
- **Date**: pre-2026-05-11 (issue #1).
- **Status**: locked.

---

## Open decisions (in resolution order)

Each item below will be resolved in turn, with options offered to the user before locking.

All items below are resolved. See "Resolved this phase" for the entries.

1. D-008 — Visibility keyword (locked: `public`)
2. D-009 — Function declaration syntax (locked: `function`, type-then-name params)
3. D-010 — Variable declaration syntax (locked: `type name = expr;`)
4. D-011 — Pack declaration and imports (locked: `pack` + `use`, `.` separator)
5. D-012 — Class and enum declarations (locked: PHP 8-style, `construct`, plain enums)
6. D-013 — Interfaces and traits (locked: `interface` + `trait`, `implements`, conflict = error)
7. D-014 — Generic parameters and bounds (locked: `<T: Bound + Bound>`, call-site inference)
8. D-015 — Pattern matching (locked: PHP 8 `match` expression, enum exhaustiveness)
9. D-016 — Lambdas (locked: `(params)[: T] => body`, auto-capture)
10. D-017 — String interpolation (locked: always-on, `{expr}`, `{{` escape)
11. D-018 — Property hooks (locked: PHP 8.4 hooks)
12. D-019 — Casting / conversion (locked: total-only `as`, fallible via methods)
13. D-020 — Manifest format (locked: `phc.json`, composer-shaped)
14. D-021 — Test syntax (provisional; finalised in Phase 9)
15. D-022 — Standard library core surface (provisional; finalised in Phase 6)

Amendment added after the first-pass Phase 1 commit `3df7de5`:

16. D-023 — Identifier sigils and member access (`$`, `->`, `::`, `.`) — amends D-009/D-010/D-012/D-013/D-016/D-017/D-018

Amendments surfaced during Phase 2 parser work (locked 2026-05-15):

17. D-005a — Member assignment `$obj->field = expr;` is a real Statement on any `->` chain rooted at a `$name` or `$this`, not only inside SetHook bodies. Subsumes the D-018 hook-setter exception and resolves a Phase 2 grammar gap. Bare `$x = expr;` remains illegal — `:=` (with `flip`) is still required for variable reassignment.
18. D-006a' — Expression-position `?` is no longer deferred. Postfix `?` on a `result<T, E>` or `option<T>` value short-circuits the enclosing function with the failure / `null` case (Rust-style propagation). Type-position `T?` is unchanged.

Locked during Phase 6 stdlib build-out:

19. D-025 — String stdlib v0 method surface (locked 2026-05-16): camelCase methods `len/contains/startsWith/endsWith/trim/upper/lower` plus carry-over `toInt`. Byte-oriented; ASCII case fold; `==` / `!=` on `string` lower to `phc_str_eq`. Multi-byte / Unicode-aware variants deferred.
20. D-027 — `list<T>` collection v0a (locked 2026-05-16): `list()` ctor (type from binding annotation), `push/len/at` methods, `$xs[i]` indexing, `for (T $x in $xs)` iteration. **Reference semantics** (handle-shared mutation), explicitly diverges from D-022's CoW pending refcounts. OOB aborts; fallible variants deferred. `list` is a reserved name.
21. D-026 — Result/Option ergonomic methods (locked 2026-05-16): `result` gets `isOk/isErr/unwrapOr`; `option` gets `isSome/isNone/unwrapOr/orElse`. Inline statement-expression lowering for `unwrapOr/orElse`; `map/andThen/unwrap/okOr` deferred (the closure forms wait on D-024).
22. D-024 — Function type syntax (locked 2026-05-16): `fn(T1, T2, ...): R` heads a function type. `fn` reserved keyword. Additive AST (`fn_return: Option<Box<TypeRef>>`) lowered to `Ty::Path { path:["fn"], args:[R, P1, ...] }`; codegen maps to `phc_lambda`. Stored lambdas now legal: bind, pass, return. Generic fn-types and the `Ty::Function` enum refactor deferred.
23. D-005 aliasing extension (locked 2026-05-16): within a single call's arg list, no two borrows of the same root binding may both be mutable, and a mutable borrow cannot coexist with any other borrow of the same root. Broader aliasing (across statements, through intermediate bindings) needs a full liveness pass and stays out of scope for v0.
24. D-028 — `map<string, V>` collection v0a (locked 2026-05-16): `map()` ctor (V from binding annotation), `set/get/has/len` methods. String keys only; linear-scan storage. Reference semantics like list. `map` is a reserved name. Generic keys, hash storage, `remove`/iteration deferred.
25. D-021 v0a (Phase 9 MVP, 2026-05-16): `test "name" { ... }` blocks at file top-level are discovered by `phc-test` and run sequentially through the interpreter. Pass = body completes without runtime error; fail = any panic or `?` propagation. `phc test <file>` reports pass/fail counts and exits non-zero on any failure. Assertion helpers (`assertEq`, etc.), cross-file project discovery, filtering, and parallel execution deferred.
26. D-029 — Result/Option closure methods (locked 2026-05-16): `result<T,E>` gets `map(fn(T):U)→result<U,E>`, `andThen(fn(T):result<U,E>)→result<U,E>`, `unwrap()→T` (panics on err). `option<T>` gets `map`, `andThen` (option-shaped), `okOr(E)→result<T,E>`, `unwrap()` (panics on none). Codegen lowers each to a stmt-expr that invokes the stored `phc_lambda` on the unwrapped payload and packs the result. Closure return type recovered from the arg's `fn(T):U` static type; typecheck lambda inference now also seeds lambda params and records `fn(...):R` on every lambda expression.
27. D-030 — `list<T>` closure methods (locked 2026-05-16): `forEach(fn(T):void)→void`, `map(fn(T):U)→list<U>`, `filter(fn(T):bool)→list<T>`. Same stmt-expr + `phc_lambda` invocation pattern as D-029, looped over `phc_list_at`/`phc_list_push`. No new runtime functions.
28. D-031 — `set<string>` collection v0a (locked 2026-05-16): `set()` ctor, `add(string)→bool` (true on insert, false if dup), `has(string)→bool`, `remove(string)→bool`, `len()→int`. String keys only; linear-scan storage parallel to map. Distinct `phc_set` C type. Generic keys / hash storage / iteration deferred.
29. D-032 — `io` stdlib namespace (locked 2026-05-16): `io::print/println/eprint/eprintln(string)→void` plus `io::readLine()→option<string>` (none on EOF). Reserved namespace; routed via the existing static-call dispatch (parallel to `Logger::info`). Compiled binaries hit real stdin/stdout/stderr via runtime helpers; interp routes prints into its captured stdout vec and stubs `readLine` to none. Logger::info retained as legacy alias for the existing example corpus.

---

## Resolved this phase

### D-008 — Visibility: two levels, `public` keyword
- **Decision**: PHC v0 has exactly two visibility levels:
  - **Pack-scoped (default, no keyword)** — the item is visible to every file in the same pack, and invisible outside the pack. In PHC, "private" means "private to the pack", not "private to the file".
  - **Cross-pack (`public` keyword)** — the item is visible to any pack that imports it.
  ```phc
  function helper(): void { }        // visible across files in this pack, hidden outside
  public function login(): void { }  // visible to importing packs
  ```
- **Alternatives considered**: `pub` (Rust-style); `export` (JS-style); `open` (Swift/Kotlin overload); a three-level model with a separate `internal` keyword for pack-scoped and file-private as the default.
- **Rationale**: PHP-familiar spelling for the explicit form. The two-level model keeps the surface tiny; file-private adds friction without clear value because packs are already small composition units. Scoped forms like `public(pack)` are unnecessary when "default" already means pack-scoped.
- **Date**: 2026-05-11.
- **Status**: locked.
- **Follow-ups**: file-private scope, friend-pack visibility, and finer scopes can be revisited post-v0 if real code demands them.

### D-009 — Function declaration syntax: `function` (PHP-style)
- **Decision**: Functions are declared with the `function` keyword. Parameter list uses **type-then-name** order with no sigils. Return type follows a `:`. The `return` keyword is required to yield a value. Statements terminate with `;`.
  ```phc
  function greet(string $name): string {
      return "Hello, {$name}";
  }

  public function add(int $a, int $b): int {
      return $a + $b;
  }

  async function fetch(string $url): result<bytes, HttpError> {
      // ...
  }
  ```
  (`result` and `bytes` are stdlib and thus lowercase; `HttpError` is user-defined and PascalCase. Sigils and arrows follow D-023.)
- **Alternatives considered**: `fn` (Rust/Swift, terse, arrow return), `fun` (Kotlin, mid-length), `func` (Go/Swift, word-like).
- **Rationale**: PHC's target audience is PHP-familiar; `function` is the term they already type every day. Verbosity is mitigated by PHC having no `$` sigils, so PHP devs get the familiarity without the noise.
- **Date**: 2026-05-11.
- **Status**: locked.
- **Implied sub-decisions** (locked together with D-009):
  - **Parameter order**: type-then-name. This is a hard constraint on D-010 (variable declaration) — local bindings should stay consistent (e.g. `int x = 5` rather than `x: int = 5`).
  - **Return-type position**: after `: Type` immediately before the body block. No `->` arrow.
  - **Explicit `return`**: no implicit last-expression return. Block bodies are statement-shaped, not expression-shaped.
  - **Statement terminator**: `;`. This implies PHC is line-insensitive (no significant newlines).
- **Follow-ups**:
  - Default parameter values and variadics: deferred (not in #1's checklist; revisit during parser work).
  - Named/labelled arguments at call sites: deferred.
  - Expression-bodied functions (`function add(int a, int b): int = a + b;` Kotlin-style single-expression form): provisional, may be added later if ergonomics demand it.

### D-010 — Variable declaration: `type name = expr;`
- **Decision**: Local bindings are declared type-then-name, matching D-009 parameter order. Immutable bindings have no leading keyword. Mutable bindings are prefixed with `flip` (D-005). Reassignment of a `flip` binding uses `:=`. Borrow modifiers `&` / `&flip` appear before the type in parameter positions.
  ```phc
  // immutable
  int $count = 42;
  string $name = "Alex";
  User $u = User("Ada", 30);    // construction is a call (D-012)

  // mutable
  flip int $score = 0;
  $score := $score + 1;

  // borrows in parameter positions
  function render(&User $user): void { /* ... */ }
  function mutate(&flip User $user): void { /* ... */ }
  ```
- **Alternatives considered**: `let type name = expr;` (Rust-like explicit binding keyword); `var name = expr;` with type inference at the binding site.
- **Rationale**: Symmetry with D-009 parameter syntax. PHP/C/Java developers read `int count = 42;` at a glance. Avoids introducing yet another keyword (`let`) when the type already signals "this is a binding".
- **Date**: 2026-05-11.
- **Status**: locked.
- **Implied sub-decisions**:
  - Explicit types required on local bindings in v0 — no `var`/inference form. Function return-type inference and generic inference are separate questions (revisit during D-014).
  - `void` is a primitive type (lowercase per D-006a) used as the return type of functions with no meaningful return value.
  - Borrow modifier placement: `&` / `&flip` appears before the type in parameter positions.
  - Initialiser required in v0: every binding has `= expr`. Forward declaration is not supported in v0.
- **Follow-ups**: constructor/factory call form (`User::new()` vs `new User(...)` vs `User { ... }`) is part of D-012. Resolved by D-012 + D-023: construction is a plain call `User(...)`; static factories use `::`.

### D-011 — Pack declaration and imports
- **Decision**: Each source file begins with `pack <path>;`. Items from other packs are brought in with `use`. Pack paths are dot-separated. Grouped imports are written with `{}`.
  ```phc
  // file: app/auth/login.phc
  pack app.auth;

  use app.http.Request;
  use app.db.{Connection, Pool};

  public function login(&Request $req): result<Session, AuthError> {
      // ...
  }
  ```
- **Alternatives considered**: `namespace` + `use` (PHP-style, maximum familiarity); implicit packs derived from directory layout with no `pack` declaration (Go/Rust-mod-style, less boilerplate but strict layout).
- **Rationale**: An explicit `pack` line states intent unambiguously and decouples logical pack structure from physical filesystem layout. `use` is shorter than `import` and matches PHP `use` muscle memory. Dot separators avoid PHP's `\` and Rust's `::`.
- **Date**: 2026-05-11.
- **Status**: locked.
- **Implied sub-decisions**:
  - Pack path separator: `.` (e.g. `app.auth.login`).
  - Grouped imports: `use a.b.{X, Y, Z};` is the canonical multi-import form.
  - One pack declaration per file: a file belongs to exactly one pack.
  - Acyclicity (D-001) is enforced at the pack graph level, not the file level. The build system computes the pack DAG.
- **Follow-ups**: aliasing (`use a.b.X as Y;`), wildcard imports (`use a.b.*;`), and re-exports (`public use a.b.X;`) deferred; revisit during Phase 2 parser work or Phase 7 build-system work.

### D-012 — Class and enum declarations (PHP 8-style)
- **Decision**: Classes are declared with `class`. Bodies contain fields and methods. A single constructor is declared with the `construct` keyword. Constructor parameters with a visibility marker are **promoted** to fields with that visibility (PHP 8.x semantics); parameters with no marker are init-only. Methods are declared with the regular `function` syntax (D-009). The implicit receiver is `this`. Construction uses call syntax with no `new` keyword: `User(args)`. `struct` does not exist in v0.

  Enums are declared with `enum`. Variants are bare names. An enum may declare a backing type via `: <primitive>`, in which case each variant has an explicit literal value.

  Field initialisation rules (D-005, D-010 carry over): a field with no `flip` is immutable; a `flip` field is mutable and reassigned with `:=`. Initial assignment inside `construct` uses `this.<field> = <expr>;` (binding/init), not `:=`.

  ```phc
  public class User {
      construct(
          public string $name,   // promoted: public field `$name`
          int $age,              // init-only param, not a field
      ) {
          $this->createdAt = instant::now();
      }

      // explicit, non-promoted field
      instant $createdAt;

      // explicit mutable field
      flip int $loginCount = 0;

      public function greet(): string {
          return "Hi, {$this->name}";
      }
  }

  User $u = User("Ada", 30);
  $u->loginCount := $u->loginCount + 1;
  ```

  ```phc
  public enum Method {
      Get,
      Post,
      Put,
      Delete,
  }

  public enum Status: int {
      Ok = 200,
      NotFound = 404,
  }

  Method $m = Method::Get;
  Status $s = Status::Ok;
  ```
- **Alternatives considered**: Java/C#-style `new User(...)`; Kotlin-style primary constructor on the class header; tagged-union enums; allowing both plain and tagged-union enums.
- **Rationale**: PHP 8 muscle memory for the target audience. Promotion-via-visibility-marker keeps the cheap path cheap and still allows init-only params. Plain enums plus future traits/interfaces cover the Result/Option use case without burdening v0 with full algebraic data types.
- **Date**: 2026-05-11.
- **Status**: locked.
- **Implied sub-decisions**:
  - **`class`, `construct`, `enum`, `this`** are reserved keywords.
  - **Variant access** uses `::` (D-023): `Method::Get`, `Status::Ok`.
  - **Field declarations** inside a class body use the local-binding form from D-010 (`type name [= expr];`, with optional `flip` prefix).
  - **Field initialisation in `construct`** uses `$this->field = expr;` (D-023). After construction, immutable fields cannot be reassigned; `flip` fields can be reassigned with `:=`.
  - **Single constructor in v0.** Multiple/overloaded constructors are deferred (named factory methods cover the gap).
  - **No `new` keyword.** Construction is a call: `User(args)`.
  - **No inheritance** (carried from D-002). Reuse via interfaces/traits (D-013).
- **Follow-ups**:
  - Tagged-union enums (variants carrying data): deferred; revisit post-v0 once Result/Option/pattern-match patterns clarify need.
  - Static methods, class constants, abstract classes: deferred.
  - Destructors / `Drop`: deferred to runtime work (Phase 5).
  - Trailing commas in constructor / enum lists: allowed by the grammar by default (Phase 2 parser work confirms).

### D-013 — Interfaces and traits (PHP 8-style)
- **Decision**: Two reuse keywords.
  - **`interface`** declares method signatures only. No bodies, no fields. A class implements one or more interfaces via `implements I1, I2` on the class header.
  - **`trait`** declares methods with bodies (no fields in v0) that get mixed into a class via `use Trait;` inside the class body. Traits can mix multiple together.
  - **Conflict resolution** when two traits define the same method name: **compile error**. The class must declare its own method to disambiguate. No `insteadof`, no last-wins.
  ```phc
  public interface Greet {
      function greet(): string;
  }

  public trait Loggable {
      function log(): void {
          Logger::info($this->toString());
      }
  }

  public class User implements Greet {
      use Loggable;

      construct(public string $name) { }

      public function greet(): string {
          return "Hi, {$this->name}";
      }
  }
  ```
- **Alternatives considered**: Single `trait` concept with `impl Trait for Class` blocks (Rust/Swift); hybrid form listing both at the class header (`class User : Greet, Loggable`); PHP `insteadof` conflict resolution; last-wins silent override.
- **Rationale**: PHP 8 split between interface (shape) and trait (mixin) is well-understood by the target audience. Compile-error-on-conflict avoids silent ordering footguns and the verbose `insteadof` grammar.
- **Date**: 2026-05-11.
- **Status**: locked.
- **Implied sub-decisions**:
  - **`interface`, `trait`, `implements`** become reserved keywords.
  - **`use` is context-overloaded**: at the top of a file it imports from a pack (D-011); inside a class body it mixes in a trait. The parser disambiguates by position.
  - **Traits hold no fields in v0** — fields are a class concern. This avoids trait/class field-conflict resolution rules.
  - **Multiple `implements`** are comma-separated: `class C implements I1, I2`.
  - **Interface methods have no bodies**; trait methods always have bodies (in v0 there is no "abstract trait method").
- **Follow-ups**:
  - Trait fields (PHP allows them) — deferred.
  - Interface default methods — deferred; can be modelled today by combining `interface` + `trait`.
  - Constants on interfaces — deferred.
  - Renaming a trait method on the class side (`use T { foo as bar; }`) — deferred.

### D-014 — Generic parameters and bounds
- **Decision**: Generic parameters use **angle brackets** with **inline bounds**. Multiple bounds combine with `+`. **Call sites never carry explicit type arguments** — generic arguments are always inferred from the call.
  ```phc
  function max<T: Ord>(T $a, T $b): T { ... }
  public class Pair<A, B: Hash> { ... }
  public interface Container<T> {
      function add(T $item): void;
      function get(int $idx): T?;
  }

  // call sites — no <T> needed:
  int $m = max(3, 7);
  map<string, User> $users = map();   // type still required at the binding site (D-010)
  ```
- **Alternatives considered**: trailing `where T: Bound` clause; `T extends Bound` (TS/Java); explicit turbofish at call sites.
- **Rationale**: Inline `<T: Bound>` keeps signatures self-contained for the common case. Mandatory call-site inference matches the D-006 preference for terse generics and aligns with modern languages (Rust, Kotlin, Swift, TS).
- **Date**: 2026-05-11.
- **Status**: locked.
- **Implied sub-decisions**:
  - **Multiple bounds**: `T: A + B` joins bounds with `+`.
  - **`<>` at types still required where the type is named** (e.g. `map<string, User>` at the binding site is part of the type, not a call-site argument).
  - **No explicit type-argument syntax at call sites in v0.** If inference fails, the user must rewrite to provide more type context (e.g. annotate the result binding). A turbofish (`::<T>`) escape hatch may be added if real code shows it's needed.
- **Follow-ups**:
  - Variance annotations (`in` / `out`, covariant / contravariant): deferred.
  - Higher-kinded types: out of scope for v0.
  - Const generics: out of scope for v0.

### D-015 — Pattern matching (`match` expression)
- **Decision**: PHC adopts a **PHP 8-style `match` expression**. The scrutinee is in parentheses. Arms are comma-separated and use `=>` between the pattern and the result expression. Patterns support: literals, identifier captures (e.g. `$s`), enum variants (`Method::Get` per D-023), wildcard `_`, OR-patterns joined with `|`, and optional `if <guard>` guards. `match` is **expression-shaped** — it returns a value, even though regular function bodies are statement-shaped (D-009). A `match` used as a statement simply discards the value.

  Exhaustiveness:
  - When the scrutinee is an enum, the compiler verifies that every variant is covered. A `_` arm is unnecessary in that case.
  - For any other scrutinee (int, string, class instance, etc.), a final `_` arm is required.

  ```phc
  int $code = match ($status) {
      Status::Ok => 0,
      Status::NotFound | Status::Gone => 404,
      $s if $s->isServerError() => 500,
      _ => -1,
  };

  // statement form (value discarded)
  match ($method) {
      Method::Get => handleGet(),
      Method::Post => handlePost(),
      _ => respond(405),
  };
  ```
- **Alternatives considered**: Rust-style match with block arms; C/PHP-style `switch` statement; always-`_` exhaustiveness; no exhaustiveness check at all.
- **Rationale**: PHP 8 `match` is the most familiar starting point for the target audience and is already expression-shaped. Strict enum exhaustiveness catches missed cases when a variant is added later.
- **Date**: 2026-05-11.
- **Status**: locked.
- **Implied sub-decisions**:
  - **`match` is a reserved keyword.** `_` is a reserved pattern token.
  - **`if` is reserved** (used here for guards; also for ordinary conditionals — exact `if`/`else` statement shape is conventional C-family, finalised during Phase 2 parser work).
  - **Arms return values of the same type.** Type inference unifies arm result types.
  - **Variable patterns** (an unbound identifier in a pattern position binds the scrutinee to that name in the arm body — e.g. `s if s.isServerError()`).
  - **No fall-through.** Each arm matches at most once.
- **Follow-ups**:
  - Structural patterns (destructuring class fields, tuple patterns): deferred; useful but adds significant grammar weight.
  - Range patterns (`1..=10`): deferred.
  - Binding inside OR-patterns: deferred.

### D-016 — Lambdas / closures: keyword-less arrow
- **Decision**: A lambda is `(<params>) [: <ReturnType>] => <body>`. Parameters follow D-009's type-then-name shape. The return type after `:` is **optional**: if omitted, the compiler infers it from the body, and a body that produces no value implies `void`. The body is either a single expression (its value is the lambda's result) or a `{ ... }` block (statements terminated by `;`, value yielded via `return`).

  **Captures are automatic.** Every free variable referenced by the lambda body is captured. The capture mode is inferred from how the body uses it:
  - read-only use → captured by **shared borrow** (the outer binding must outlive the lambda).
  - reassignment via `:=` inside the body → captured by **mutable borrow**, which is only legal when the outer binding is `flip`.

  ```phc
  list<int> $doubled = $nums->map((int $n): int => $n * 2);

  // return type elided (void)
  $button->onClick(() => Logger::info("clicked"));

  // block body
  $users->forEach((User $u) => {
      Logger::info($u->name);
      $u->loginCount := $u->loginCount + 1;
  });

  // captures
  int $threshold = 10;
  list<int> $big = $nums->filter((int $n): bool => $n > $threshold);   // shared borrow

  flip int $hits = 0;
  $nums->forEach((int $n) => { if ($n > $threshold) $hits := $hits + 1; });  // $hits is flip → mutable capture OK
  ```
- **Alternatives considered**: PHP 8 dual form (`fn ... =>` + `function ... { }`); a single `fn (params) => body` form; PHP `use (...)` explicit capture lists; no-capture lambdas.
- **Rationale**: A single keyword-less arrow is the smallest possible lambda surface. Auto-capture with inferred borrow mode matches Rust/Swift ergonomics while respecting D-005 mutability rules — mutating captures fall out as a compile error unless the binding is explicitly `flip`.
- **Date**: 2026-05-11.
- **Status**: locked.
- **Implied sub-decisions**:
  - **`=>` is a reserved punctuation token**, used in match arms (D-015) and lambda bodies.
  - **Lambdas vs functions**: function bodies follow D-009 (statement-shaped, explicit `return`). Lambdas with an **expression body implicitly return** the expression. Lambdas with a **block body** use explicit `return`. This asymmetry is intentional — top-level functions stay verbose and uniform, lambdas stay concise.
  - **Move/owned captures** are not in v0; if a closure must outlive the outer scope, the user must rebuild the captured state explicitly.
- **Follow-ups**:
  - Owned/move captures (e.g. `move |...| ...` in Rust): deferred until concurrency/async usage proves the need.
  - Variadic lambda params: deferred.
  - Inferred parameter types (`(n) => n * 2` without the `int` annotation): deferred; v0 requires explicit param types to match D-010's no-inference rule.

### D-017 — String interpolation: always-on with `{expr}`
- **Decision**: All double-quoted strings are interpolated. An embedded expression sits inside `{ ... }`. To write a literal `{` or `}` in a string, double it: `{{` and `}}`. Standard backslash escapes (`\"`, `\\`, `\n`, `\t`, etc.) work as expected. Any expression is permitted inside `{ ... }`; the result is converted to `string` via the standard formatting trait (TBD in D-022).
  ```phc
  string $g = "Hello, {$user->name}! You have {$inbox->count} messages.";

  // literal braces in output
  string $json = "{{\"key\": \"value\"}}";

  // arbitrary expressions
  string $log = "User {$user->name} scored {$score * 2}";
  ```
- **Alternatives considered**: opt-in `f"..."` prefix (Python/Hack); opt-in `$"..."` prefix (C#); JS-style backticks with `${ ... }`.
- **Rationale**: PHP devs already expect interpolation inside `"..."`. Without `$` sigils, the cleanest port is "every double-quoted string interpolates". Doubling for literal braces is a tiny tax for the cleanest call-site syntax.
- **Date**: 2026-05-11.
- **Status**: locked.
- **Implied sub-decisions**:
  - **The single-quoted `'...'` form is reserved for the raw-string variant** (deferred — needed to support strings full of `{` without escaping). For v0, only double-quoted strings exist.
  - **Embedded expressions are full expressions**, not just identifiers; precedence cuts at the closing `}`.
  - **Conversion to string** for the interpolated expression goes through the stdlib `display` trait (D-022), the exact shape of which is owned by Phase 6.
- **Follow-ups**:
  - Format specifiers (`{value:.2f}`, padding, hex, etc.): deferred.
  - Raw-string form (probably `'...'` or `r"..."`): deferred.
  - Multi-line strings: deferred.

### D-018 — Property hooks (PHP 8.4-style)
- **Decision**: A field may optionally carry a **hook block** with `get` and/or `set` hooks. Without a hook block the field is a plain backing storage location accessed directly. With a hook block:
  - `get` defines how the property is read. Can be a short form (`get => expr;`) or a block (`get { ... return ...; }`).
  - `set(<Type> value)` defines how the property is written. Inside the body, `this.<name> = ...` assigns the backing field (not recursive).
  - A property with only a `get` hook is read-only from outside the class. A property that should be writable from outside must have a `set` hook *and* the backing field must be `flip`. Writes use `:=` per D-005.
  ```phc
  public class User {
      construct(public string $firstName, public string $lastName) { }

      // computed, read-only from anywhere
      public string $fullName {
          get => "{$this->firstName} {$this->lastName}";
      }

      // backing field with validated setter
      flip int $age = 0 {
          set(int $value) {
              if ($value < 0) panic("age must be non-negative");
              $this->age = $value;       // direct assign to backing field, no recursion
          }
      }
  }

  User $u = User("Ada", "Lovelace");
  string $f = $u->fullName;   // calls the get hook
  $u->age := 25;              // runs the set hook
  ```
- **Alternatives considered**: method-based `getX()` / `setX()` only; C#-style `{ get; set; }` auto-properties; Kotlin computed properties.
- **Rationale**: PHP 8.4 just shipped this exact feature; PHC's target audience will recognise it immediately. The plain-field path stays plain; hook syntax only appears when behaviour is needed.
- **Date**: 2026-05-11.
- **Status**: locked.
- **Implied sub-decisions**:
  - **`get` and `set` are contextual keywords** inside hook blocks (still usable as identifiers elsewhere, parser disambiguates by position).
  - **No private/internal hooks in v0** — hooks share the field's outward visibility.
  - **`$this->field = expr;` inside a `set` hook bypasses the setter and writes the backing field directly** (preventing infinite recursion). This is a *use* of the general member-assignment form formalised by D-005a (2026-05-15); it is no longer a hook-local exception. Anywhere else in code, `$obj->field = expr;` is also a direct field write — the hook-call semantics in v0 only apply when the user explicitly writes `:=` for variable reassignment, never as an implicit setter trampoline.
  - **Pure data fields stay zero-cost** — only fields with hooks generate accessor calls.
- **Follow-ups**:
  - Asymmetric visibility (e.g. `public get`, `private set`): deferred.
  - Hook on a non-backed (purely computed) property without declaring `flip`/value: today expressed by writing `get` only and omitting `= value`.
  - `init` hook (runs only during construct): deferred.

### D-019 — Casting and conversion: `as` is total, methods are fallible
- **Decision**: The `value as <Type>` expression is permitted **only** when the conversion is total and lossless. The typechecker rejects any lossy or fallible `as`. Lossless cases include:
  - Widening numeric conversions where every value of the source type is representable in the target (`byte → int`, `int → float` for the canonical widths in D-022, etc.). Note: with the polymorphic-storage `int`/`float` rules in D-022, most "widening" cases between same-named types disappear — they happen at the storage layer, not at the type layer.
  - Upcast to a trait/interface the source type implements (`$user as display`).
  - Upcast to `dyn T` (D-006).

  Anything fallible — narrowing, parsing, lossy float→int, downcast — is exposed as a method that returns `result<T, E>` (with a domain error) or `T?` (when the only failure mode is "not representable").
  ```phc
  // Widening / upcast cases (now mostly handled by the polymorphic int rules in D-022).
  display $d = $user as display;          // OK: upcast to a stdlib trait
  dyn $any = $user as dyn;                // OK: upcast to dyn

  // Fallible / parse / narrowing — through methods that return result<...> or T?.
  string $s = "42";
  result<int, parseError> $p = $s->toInt();
  int? $maybe = $s->tryParseInt();
  ```
- **Alternatives considered**: `as` for everything with panic on failure (violates D-007); `as` for safe + `as?` returning `T?` for fallible (compresses two distinct failure shapes into one).
- **Rationale**: Anchors the type system in D-007: failures that the compiler can prove are impossible cost nothing; failures the compiler cannot prove must surface in the type. The user picks `result` vs `T?` based on whether the cause matters.
- **Date**: 2026-05-11.
- **Status**: locked.
- **Implied sub-decisions**:
  - **`as` is a reserved keyword.**
  - **Conversion trait** (`from`/`into` shape, lowercase per D-006a) is the canonical extension point for user types and is part of D-022 (stdlib core).
  - **No reinterpret/bit-cast** in v0 (that would require `unsafe` and is out of v0 scope).
- **Follow-ups**:
  - Pattern downcasting (`match` on a `dyn T` value to recover concrete type): deferred.
  - Conversion-method naming convention (`toX` vs `intoX` vs both): part of D-022.

### D-020 — Manifest format: `phc.json`
- **Decision**: A pack declares its metadata in a sibling `phc.json` file. The shape is composer-influenced JSON. The `name` field must equal the dotted pack path used in `pack <path>;` declarations (D-011). Dependency entries are either a version string or an object with `version`, `path`, or future `git` keys.
  ```json
  {
    "name": "app.auth",
    "version": "0.1.0",
    "edition": "2026",
    "authors": ["Alex <alex@example.com>"],
    "license": "MIT",
    "dependencies": {
      "std": "1",
      "app.http": { "path": "../http" }
    },
    "dev-dependencies": {
      "phc-test": "0.1"
    }
  }
  ```
- **Alternatives considered**: TOML (`phc.toml`, Cargo-shaped, comments + cleaner readability); YAML (whitespace-sensitive, footgun-prone).
- **Rationale**: PHP developers recognise composer.json on sight. JSON is universally tooled. Trading away comments is acceptable for v0; the loss can be mitigated with a sidecar `phc.lock` or schema documentation.
- **Date**: 2026-05-11.
- **Status**: locked.
- **Implied sub-decisions**:
  - **Filename**: exactly `phc.json` in the pack root directory.
  - **`name` matches `pack` declarations** — the build system enforces this.
  - **Version field**: SemVer string. Lock file conventions are deferred to Phase 7.
  - **`edition`**: a calendar-year string tying source files in the pack to a language edition. v0 uses `"2026"` as the only valid value.
  - **Dependency entry shape**: string for the version-only common case; object form for path / git / extras.
- **Follow-ups**:
  - Workspaces (multi-pack projects, similar to Cargo workspaces) — Phase 7.
  - Features / conditional compilation — deferred.
  - Lockfile format (`phc.lock`) — Phase 7.
  - JSON schema / `$schema` field — Phase 8 (tooling).

### D-021 — Test syntax (provisional)
- **Decision (provisional)**: PHC tests are declared with the `test` keyword inside any source file. A test is a named block that runs as part of `phc test`. Assertions are ordinary function calls returning `result`. The test fails if it returns an `err` variant or if the body panics.
  ```phc
  test "addition is commutative" {
      assert::eq(add(2, 3), add(3, 2));
  }

  test "login rejects empty password" {
      result<Session, AuthError> $r = login("alex", "");
      assert::isErr($r);
  }
  ```
- **Status**: provisional. Locked just enough that other Phase 1 docs and lexer work can know `test` is a reserved keyword.
- **Date**: 2026-05-11.
- **Open in Phase 9 (#10)**:
  - Per-test setup/teardown, table-driven tests, parametrised tests.
  - Snapshot/property-test syntax.
  - Filtering and tagging.
  - Async tests and timeout policy.
- **Implied reservations**: `test` is a reserved keyword (visible to the lexer in Phase 2).

### D-022 — Standard library core surface (provisional)
- **Decision (provisional)**: All stdlib names are lowercase (D-006a). The locked primitive set is **`int`**, **`float`**, **`byte`**, **`bytes`**, **`bool`**, **`string`**, **`void`**. There is no separate `long`, `double`, `decimal`, or `short` keyword.
  - **`int`** is a polymorphic signed integer. Its **canonical width is 64-bit signed** at every type boundary that has to commit to a layout (function parameters and returns, fields, generic instantiations, FFI). For local bindings whose value range the compiler can prove, narrower storage (i8 / i16 / i32) may be picked silently as an optimisation; the user-visible type is still `int`. Integer literals outside the canonical i64 range are a compile-time error.
  - **`float`** mirrors `int`: canonical width is **64-bit IEEE-754** at boundaries; narrower storage (f32) may be picked locally when the compiler can prove the value is representable.
  - **`byte`** is an 8-bit unsigned scalar (`0..255`). Fixed width.
  - **`bytes`** is an owned, CoW sequence of `byte`. The container for raw binary data; indexed and iterated as `byte`.
  - **`bool`** is the two-valued boolean. Fixed width (1 bit conceptually; one register slot in practice).
  - **`string`** is an owned, CoW UTF-8 string (D-004). No separate `char` type in v0; iterating a `string` yields code points by method, not by a primitive type.
  - **`void`** is the absence-of-value return type (D-010).

  **Built-in collections** (all lowercase, CoW per D-004): `list<T>`, `map<K, V>`, `set<T>`. **`array<T>` is an alias for `list<T>`** for PHP muscle-memory at the call site.

  **Stdlib traits and types** (provisional names, lowercase per D-006a): `result<T, E>`, `option<T>`, `display`, `from<T>`, `into<T>`, `taskGroup`, plus stdlib error types like `parseError`, `overflowError`. User-defined errors stay PascalCase (e.g. `AuthError`, `HttpError`).
- **Status**: provisional. The primitive set itself is locked; the trait/error/method surfaces are owned by Phase 6 (#7).
- **Date**: 2026-05-11 (amended after initial Phase 1 lock).
- **Open in Phase 6 (#7)**: complete name list, method surfaces, panic API, arbitrary-precision integer fallback (if any), encoding-conversion API, iterator protocol.

### D-023 — Identifier sigils and member access (`$`, `->`, `::`, `.`)
- **Decision**: PHC adopts PHP-style sigils and three distinct access operators. This amends D-009 (function parameters), D-010 (local bindings), D-012 (classes), D-013 (traits/interfaces), D-016 (lambdas), D-017 (string interpolation), and D-018 (property hooks). Every example in this document and `spec/language-reference.md` uses the post-amendment form.

  | Form | Used for | Example |
  |------|----------|---------|
  | `$name` | Every variable, parameter, and field reference at the use site. The declaration site of a parameter or field also carries `$`. | `$user`, `$count`, `$this` |
  | `->` | Member access on an **instance** of a class (instance fields, instance methods, property hooks). | `$user->name`, `$this->loginCount`, `$conn->open()` |
  | `::` | **Static** access: enum variants, static methods, class constants. | `method::Get`, `status::Ok`, `User::new()` (factory style) |
  | `.` | **Path** separator only — pack paths, namespaced type references in declarations. | `pack app.auth;`, `use app.http.Request;`, `app.http.Request` |

  PHC keeps **no `new` keyword** (D-012). Construction is still a call: `User("Ada", 30)`. Factory methods (when introduced) use `::`: `User::fromJson(...)`.

  `$this` is the implicit receiver inside any method or constructor body and is the only sigil-prefixed identifier with a reserved meaning.

  ```phc
  pack app.auth;

  use app.http.Request;

  public class User {
      construct(public string $name, int $age) {
          $this->createdAt = instant::now();
      }

      instant $createdAt;
      flip int $loginCount = 0;

      public function greet(): string {
          return "Hi, {$this->name}";
      }
  }

  public function login(&Request $req): result<session, AuthError> {
      User $u = User("Ada", 30);
      $u->loginCount := $u->loginCount + 1;
      method $m = method::Get;
      return ok(session::open($u));
  }
  ```
- **Alternatives considered**: keep the sigil-less / single-`.` form locked in the first pass of Phase 1; partial adoption (sigils only on `$this`); JS-shaped `obj.member` plus `Class.staticMember` (loses static/instance distinction at the call site).
- **Rationale**: PHC's target audience reads `$user->name` instantly. The three-operator split makes static vs instance vs path unambiguous at every use site — `Status.Ok` (was) is now `status::Ok` and cannot be confused with a member access on a value named `status`.
- **Date**: 2026-05-11 (introduced after the first-pass Phase 1 commit `3df7de5`).
- **Status**: locked.
- **Implied sub-decisions**:
  - **Sigil characters**: `$` is reserved as a leading-identifier sigil. `$$` (variable-variable) and PHP heredoc forms are **not** supported in v0.
  - **`->`, `::`, `.` are three distinct tokens.** The lexer produces them as separate tokens; the parser uses them to switch member-access semantics.
  - **String interpolation (D-017)** still uses `{expr}`. Inside the braces, sigils and arrows are required as elsewhere: `"Hello, {$user->name}"`.
  - **Pattern matching (D-015)** scrutinees and arm patterns use the same sigils. `match ($status) { status::Ok => 0, _ => -1 }`.
  - **Local-binding LHS** carries `$`: `int $count = 42;`, `flip int $score = 0;`, `$score := $score + 1;`.
  - **Borrows** stay as in D-005: `&$user`, `&flip $user`. The sigil sits on the identifier; `&` / `&flip` is a prefix on the borrow expression.
  - **Trait `use` inside a class body** (D-013) is unaffected — the trait name is a type, not a variable.
  - **Pack `use` declarations** (D-011) are unaffected — they reference type paths, not values.
- **Follow-ups**:
  - `static` keyword for static methods / class constants: deferred to a later edition once user code exposes the need.
  - Optional chaining (`?->`): deferred.

### D-025 — String stdlib method surface (v0)
- **Decision**: `string` carries a fixed v0 method surface, dispatched
  via `->` like any instance method (D-023). All names are
  **camelCase** to match the existing `$str->toInt()` builtin.
  **Byte-oriented**: `len()` and the substring scan methods operate
  on UTF-8 bytes, not code points; `upper()`/`lower()` apply ASCII
  case fold only. Multi-byte-aware variants (`charLen`, full
  Unicode case fold) land when the runtime grows real Unicode
  tables and are explicitly out of scope for v0.

  | Method | Signature | Notes |
  |--------|-----------|-------|
  | `len` | `(): int` | Length in bytes. |
  | `contains` | `(string): bool` | Substring containment. |
  | `startsWith` | `(string): bool` | Prefix check. |
  | `endsWith` | `(string): bool` | Suffix check. |
  | `trim` | `(): string` | Strip ASCII whitespace from both ends; returns a fresh owned string. |
  | `upper` | `(): string` | ASCII case fold. |
  | `lower` | `(): string` | ASCII case fold. |
  | `toInt` | `(): result<int, parseError>` | Existing builtin (carried over). |

  String equality (`==`, `!=`) is **byte-wise** and added in the
  same slice; the codegen lowers both operands typed `string` to
  the runtime's `phc_str_eq` helper.
- **Alternatives considered**: snake_case method names (`starts_with`,
  consistent with the C runtime layer but inconsistent with `toInt`);
  free-function form (`string::len($s)`) instead of methods; PCRE-
  style chained predicates; deferring strings until full Unicode
  support is in.
- **Rationale**: A small fixed surface unblocks every realistic v0
  program (validation, simple parsing, log message construction)
  without committing to a Unicode model. CamelCase matches the
  existing `toInt` and the broader `$obj->method()` convention.
  ASCII-only fold keeps the runtime self-contained — the entire
  set lives in ~80 lines of C.
- **Date**: 2026-05-16.
- **Status**: locked for v0 surface; UTF-8 / Unicode follow-ups
  tracked separately.
- **Implied sub-decisions**:
  - `string == string` is byte-wise. Locale-aware or
    case-insensitive comparison is **not** in scope; it would be a
    later method (e.g. `equalsIgnoreCase`) or a full collator.
  - All methods that return a `string` allocate a fresh buffer
    (the runtime does not reuse the input). This matches D-004's
    CoW intent today; reference-counted sharing lands when the
    runtime grows it.
  - Concatenation (`+`) is unchanged (D-019 / D-022): both
    operands typed `string` already lower to `phc_concat2`.
- **Open follow-ups**:
  - `split(string)` — produces `list<string>`. Blocked on the
    `list<T>` collection type.
  - `replace`, `indexOf`, `slice`, code-point-iteration methods.
  - Full Unicode case fold and grapheme-cluster `len`.

### D-027 — `list<T>` collection (v0a)
- **Decision**: Locks the v0a surface for `list<T>` (D-022 left this
  Phase 6). Reserved name; user code may not declare a top-level
  `list` function or class.
  - **Construction**: `list()` — bare zero-arg call returns an empty
    list. The element type is taken from the binding's type
    annotation (`list<int> $xs = list();`). Generic-call ctor syntax
    (`list<int>()`) is **not** in v0a — it parses today as a
    comparison; explicit generic instantiation lands later.
  - **Methods (D-023 dispatch)**:

    | Method | Signature | Notes |
    |--------|-----------|-------|
    | `len` | `(): int` | Element count. |
    | `push` | `(T): void` | Append; mutates the heap-allocated list. |
    | `at` | `(int): T` | Bounds-checked element read. **Out of bounds aborts the program** (`phc_panic`); fallible variants (`option<T>` / `result<T, IndexError>`) deferred. |

  - **Indexing**: `$xs[i]` is sugar for `$xs->at(i)`; same panic-on-OOB
    behaviour. Indexing is supported only on `list<T>` in v0a; other
    receivers are a codegen error.
  - **Iteration**: `for (T $x in $xs) { ... }` walks every element in
    insertion order. Only `list<T>` is iterable in v0a; iterating
    other types is a runtime error in the interpreter and a
    codegen error in compiled binaries.
  - **Reference semantics**: `list<T>` is heap-allocated and the
    local binding holds a handle. Two handles to the same list see
    each other's `push`es. This **diverges from D-022's "CoW per
    D-004"** — refcounted CoW lands when the runtime grows it; the
    v0a behaviour is documented here so the divergence is explicit.
  - **Mutation vs `flip`**: `$xs->push(x)` mutates the heap-allocated
    backing storage, **not** the local handle. The handle's `flip`
    flag governs only handle reassignment (`$xs := other_list;`).
    A `list<T> $xs` (no `flip`) can therefore receive `push`/`at`
    calls without violating D-005. Stated explicitly so D-005 +
    D-027 do not appear contradictory.
- **Alternatives considered**: CoW today (needs refcounts before any
  sharing edge case is sound); `array<T>` literal syntax `[1, 2, 3]`
  in v0 (new lexer token, deferrable); fallible `at` returning
  `option<T>` (PHP/Rust idiom, but adds `?` ceremony at every read);
  PHP-style by-value array semantics (forces deep clones on
  assignment, conflicts with D-022's CoW intent).
- **Rationale**: Smallest coherent surface that lets a real program
  collect-and-iterate values without a class scaffolding. Reference
  semantics is honest about today's runtime; documenting the gap
  beats silently picking either CoW or copy.
- **Date**: 2026-05-16.
- **Status**: locked for v0a surface; CoW migration, fallible
  accessors, `pop`/`slice`/`insert`/`remove`, and generic-call ctor
  tracked separately.
- **Memory note (runtime)**: `phc_list_push` of a `phc_string`
  shallow-copies the struct (same `data` pointer). Safe today
  because v0 never frees; will need a real ownership story before
  drops are introduced.

### D-026 — Result/Option ergonomic methods (v0)
- **Decision**: Adds the smallest method surface needed to consume
  `result<T, E>` and `option<T>` without falling back to `match` for
  every check. CamelCase per D-025.

  **Result methods**:

  | Method | Signature | Returns |
  |--------|-----------|---------|
  | `isOk` | `(): bool` | true if the receiver is `result::ok(_)`. |
  | `isErr` | `(): bool` | true if the receiver is `result::err(_)`. |
  | `unwrapOr` | `(T): T` | The ok payload, or the fallback when err. |

  **Option methods**:

  | Method | Signature | Returns |
  |--------|-----------|---------|
  | `isSome` | `(): bool` | true if the receiver is `option::some(_)`. |
  | `isNone` | `(): bool` | true if the receiver is `option::none`. |
  | `unwrapOr` | `(T): T` | The some payload, or the fallback when none. |
  | `orElse` | `(option<T>): option<T>` | The receiver if some, else the alternative option. |

  Predicates lower to runtime helpers (`phc_result_is_ok`, etc.).
  `unwrapOr` and `orElse` are emitted inline as GCC
  statement-expressions that bind the receiver once and select the
  payload union member at the call site from the static T — same
  payload-dispatch pattern D-025 / D-027 use.
- **Alternatives considered**: `unwrap()` (panic on err/none — kept
  for a later slice; `unwrapOr` covers the common case without
  inviting panics in well-formed code); `okOr(E)` to convert option
  to result (deferred — simpler to add when conversion patterns
  surface in real code); `map`/`andThen` (require lambdas as values,
  blocked on D-024).
- **Rationale**: Closes the readability gap between "match every
  result by hand" and "ignore the failure variant entirely". Picks
  the methods that have a known type-inference path through codegen
  today (no closure needed, payload type comes from the static
  receiver type), so the slice doesn't sit on D-024.
- **Date**: 2026-05-16.
- **Status**: locked for v0 surface; `map` / `andThen` / `unwrap` /
  `okOr` tracked separately.
- **Known limitation**: typecheck does not yet infer return types
  for method calls (only free functions). Chains like
  `$o->orElse(other)->unwrapOr(0)` build through the interpreter
  but not through codegen; split through a typed local for now.
  Lifting this is a typecheck improvement, not a D-026 change.

### D-024 — Function type syntax (`fn(T, U): R`)
- **Decision**: A function type is written `fn(T1, T2, ...): R`.
  - `fn` is a reserved keyword; uses the same name as the runtime
    `phc_lambda` value but stays visually distinct from the
    `function` declaration keyword so a reader can tell at a glance
    whether they are looking at a declaration or a type.
  - The colon-then-return-type shape matches function declarations
    (D-009) and lambdas (D-016), so the syntactic family is
    consistent: a thing that produces a value of type `R` is
    written `... : R`.
  - Empty parameter list is allowed: `fn(): R`.
  - Nullable suffix is allowed at the outermost position:
    `fn(int): int?` (the return is nullable) and `fn(int): int?` 
    bound to a `?`-suffixed local is two distinct concepts; the
    function-type carrier itself can be wrapped in `?` for
    null-allowed callbacks. v0 keeps that to the outermost.
  - Borrow modifiers in parameter slots (`fn(&User): bool`) parse
    in v0 only at the bare-type level — the parser accepts any
    `Type` per slot. Lambda capture-mode + parameter borrow
    enforcement lands with the borrowcheck aliasing slice (task
    #62), not here.
  - Generic function types (`fn<T>(T): T`) are **deferred**;
    parsing them needs disambiguation work that overlaps with
    D-014 call-site inference and is best landed there.
- **AST representation**: additive. `TypeRef` gains a single
  optional field `fn_return: Option<Box<TypeRef>>`. When `Some`,
  `path == ["fn"]`, `args` is the parameter type list, and
  `fn_return` is `R`. Lowered to `Ty::Path { path: ["fn"],
  args: [R, P1, ..., Pn], nullable }` so consumers can recover the
  return type as `args[0]`. A dedicated `Ty::Function` is a future
  refactor; the additive shape ships D-024 in one slice without
  touching every AST consumer.
- **Codegen**: `fn(...): R` lowers to `phc_lambda` at the C
  boundary. An `Expr::Lambda` that escapes the inline-invoke
  shortcut (i.e. used as a value) materialises as a `phc_lambda`
  compound literal pairing the lifted body's address with its
  capture env. Calling a stored lambda casts `__l.fn` to its
  precise function-pointer signature (recovered from the callee's
  static `fn(...): R` type) and invokes through `__l.env`.
- **Alternatives considered**: `(T, U) -> R` (Rust shape — needs
  lexer disambiguation since `->` already means D-023 instance
  member access); `fn<R, T, U>` (reuses generics surface, no new
  tokens, but the first arg is the return type which is awkward to
  read); `callable` opaque type (no static arity / signature info,
  loses the value of having types at all).
- **Rationale**: The colon-then-return-type form is the smallest
  surface that delivers stored / passed / returned lambdas without
  inventing a new return-type punctuation. `fn` keyword is short,
  obviously distinct from `function`, and matches the visual rhyme
  in `(params) =>` lambdas where the type-position version reads as
  "the type of one of those". Additive AST keeps the slice small —
  the enum refactor is honest follow-up work, not a prerequisite.
- **Date**: 2026-05-16.
- **Status**: locked for v0 surface; enum-based AST refactor,
  generic function types, and borrow-aware parameter parsing
  tracked separately.

### D-028 — `map<string, V>` collection (v0a)
- **Decision**: First map. **v0a restricts keys to `string`** —
  the generic-key form `map<K, V>` parses fine, but only `K =
  string` is wired through codegen and runtime in this slice.
  Same reference-semantics carve-out as `list<T>` (D-027): the
  local handle is immutable by default, mutation through methods
  is allowed regardless of the handle's `flip` flag.
  - **Construction**: `map()` zero-arg call; value type taken
    from the binding annotation (`map<string, int> $m = map();`).
    `map` is a reserved name.
  - **Methods (D-023 dispatch)**:

    | Method | Signature | Notes |
    |--------|-----------|-------|
    | `len` | `(): int` | Number of entries. |
    | `has` | `(string): bool` | Key presence. |
    | `get` | `(string): option<V>` | Returns `option::some(v)` on hit, `option::none` on miss. |
    | `set` | `(string, V): void` | Insert or overwrite. |

  - **Storage in v0a is linear-scan** — every method walks the
    entries Vec. Acceptable for the v0 workloads this unblocks
    (small lookup tables, key→config maps); hash-based storage
    lands when generic-key hashing is speced.
- **Alternatives considered**: hash-from-day-one (couples to a
  hasher decision before user code has surfaced what they need);
  generic keys via a `hashable` trait dispatch (depends on D-014
  trait-bound resolution that hasn't shipped); separate
  `dict<V>` for the string-only case (forks the type surface).
- **Rationale**: Picks the smallest surface that lets a real
  program build and read a string-keyed lookup table. The linear
  scan plus the reserved-name pattern keeps the slice tight and
  consistent with D-027.
- **Date**: 2026-05-16.
- **Status**: locked for v0a; generic `K`, hash storage,
  `remove`, iteration / `forEach`, and `entries / keys / values`
  tracked separately.

---

### D-029 — Result/Option closure methods (v0)
- **Decision**: Adds the closure forms that were deferred from
  D-026 pending D-024 (function-type syntax, now locked).

  **Result methods**:

  | Method | Signature |
  |--------|-----------|
  | `map` | `(fn(T): U): result<U, E>` |
  | `andThen` | `(fn(T): result<U, E>): result<U, E>` |
  | `unwrap` | `(): T` — panics on `result::err(_)` |

  **Option methods**:

  | Method | Signature |
  |--------|-----------|
  | `map` | `(fn(T): U): option<U>` |
  | `andThen` | `(fn(T): option<U>): option<U>` |
  | `okOr` | `(E): result<T, E>` |
  | `unwrap` | `(): T` — panics on `option::none` |

  Codegen lowers each closure form to a GCC statement-expression
  that binds the receiver once, branches on the tagged-union
  `kind`, invokes the stored `phc_lambda` (`map`/`andThen`) on
  the unwrapped payload, and packs the returned value back into
  the appropriate carrier with the right `phc_payload` union
  member. `okOr` does not need the callback machinery — it just
  re-tags the receiver, picking the err-side member from the
  argument's static type. `unwrap` is the simplest: a `kind != 0`
  check that calls `phc_panic` with a clear message.

  Closure return type recovery: the callback argument's static
  type is `fn(T): U`, which lowers (D-024) to
  `Ty::Path { path: ["fn"], args: [U, T] }`. The new
  `lambda_return_ty` helper plucks `U = args[0]` for `map` /
  `andThen` payload-member selection.

  Typecheck improvement bundled with this slice: the lambda
  inferer now (a) seeds the lambda's own parameters into the
  binding-type table before walking the body, and (b) records
  the lambda expression as `fn(P1, ..., Pn): R` (return type
  from explicit `: T` annotation, falling back to the
  Expr-body's recorded type). Without (a) a `result::ok($n + 1)`
  inside a lambda body would record the binary as `Unknown` and
  the codegen would pick the wrong payload member. Without (b)
  `$xs->map($double)` couldn't infer U because the lambda's
  static type would be `Ty::Unknown`.
- **Alternatives considered**: keep these deferred until a
  full generics + bound-resolution pass (delays every realistic
  result-pipeline program); ship without `unwrap` (matches
  Rust's "panics are loud" spirit but blocks the common
  "I checked, give me the value" pattern); fold into D-026
  (rewrites a locked decision instead of layering).
- **Rationale**: small surface; orthogonal to the rest of D-022;
  same dispatch + payload-member pattern D-025 / D-027 / D-028
  already use, plus the stored-lambda invocation pattern from
  D-024. Unblocks every realistic result-pipeline program.
- **Date**: 2026-05-16.
- **Status**: locked for v0 surface; collection closure methods
  (`forEach` / `map` / `filter` on `list<T>`) tracked separately.

### D-030 — `list<T>` closure methods (v0)
- **Decision**: Adds the closure forms to `list<T>` now that
  D-024 (function types) and D-029 (Result/Option closure
  pattern) are locked.

  | Method | Signature |
  |--------|-----------|
  | `forEach` | `(fn(T): void): void` |
  | `map` | `(fn(T): U): list<U>` |
  | `filter` | `(fn(T): bool): list<T>` |

  Codegen uses the same stmt-expr + `phc_lambda` cast pattern
  D-029 introduced. The loop walks `phc_list_at(__xs, __i)` for
  `__i in [0, phc_list_len(__xs))`. `map` and `filter` allocate
  a fresh `phc_list_new()` and `phc_list_push` per kept element;
  `forEach` invokes the callback and trails `(void)0;` so the
  stmt-expr value is `void`-typed (legal in ExprStmt position).

  Interp mirrors the surface via the existing
  `eval_lambda_arg` / `invoke_lambda_with` helpers from D-029,
  snapshotting the list before iteration so a callback that
  mutates the underlying list does not invalidate the loop.
- **Alternatives considered**: defer to a generic-iterator
  trait (depends on D-014 bounds work that hasn't shipped);
  add `reduce` / `fold` in the same slice (Bigger; payload-
  member dispatch for the accumulator needs more design and
  is a clean follow-up).
- **Rationale**: smallest closure-friendly surface that
  unblocks "transform a list" programs; same codegen +
  typecheck plumbing D-029 already validated.
- **Date**: 2026-05-16.
- **Status**: locked for v0 surface; `reduce` / `fold` /
  `find` / `any` / `all` tracked separately.

### D-031 — `set<string>` collection (v0a)
- **Decision**: Adds the third collection per D-022 in the same
  v0a shape as `list<T>` (D-027) and `map<string, V>` (D-028):
  string-only keys, linear-scan storage, reference semantics
  (handle shared between bindings; mutation through methods
  updates the shared backing storage).

  | Method | Signature | Notes |
  |--------|-----------|-------|
  | `set()` | `(): set<string>` | Reserved-name ctor; T from binding annotation. |
  | `add` | `(string): bool` | True on insert, false if key already present. |
  | `has` | `(string): bool` | Membership test. |
  | `remove` | `(string): bool` | True if a key was removed, false otherwise. |
  | `len` | `(): int` | Number of distinct keys. |

  Runtime is a `phc_set` opaque pointer wrapping a `phc_string[]`
  with len + cap doubling, identical in structure to `phc_map`
  but without the value slot. Removal compacts by swapping the
  last entry into the hole (insertion-order is not preserved,
  matching most v0 expectations for sets).

  `set` is a reserved name: a user-defined free function or
  class named `set` is shadowed by the built-in constructor.
- **Alternatives considered**: piggyback on `map<string, void>`
  (saves a runtime type but conflates two concepts at the user
  level); hash-based storage from day one (couples to a hasher
  decision before user code surfaces what they need).
- **Rationale**: rounds out the D-022 collection trio with the
  smallest surface that lets a real program model membership.
  Same pattern as `list` and `map` makes the codegen + interp
  + typecheck wiring almost mechanical.
- **Date**: 2026-05-16.
- **Status**: locked for v0a surface; generic-key sets, hash
  storage, iteration (`forEach`/`toList`), and set operations
  (`union`/`intersect`/`difference`) tracked separately.

### D-032 — `io` stdlib namespace (v0)
- **Decision**: Adds a reserved `io` static-call namespace that
  surfaces the real I/O primitives the runtime already supports,
  replacing the long-standing `Logger::info` placeholder as the
  recommended way to write to stdout/stderr.

  | Call | Signature | Behaviour |
  |------|-----------|-----------|
  | `io::print` | `(string): void` | stdout, no trailing newline. |
  | `io::println` | `(string): void` | stdout, trailing newline. |
  | `io::eprint` | `(string): void` | stderr, no trailing newline. |
  | `io::eprintln` | `(string): void` | stderr, trailing newline. |
  | `io::readLine` | `(): option<string>` | One line from stdin (CRLF trimmed); `option::none` on EOF. |

  Codegen lowers each call to a `phc_io_*` runtime helper (added
  in `phc_runtime.c`). The interpreter routes both stdout and
  stderr prints into its single captured stdout vector (the
  tree-walking interp doesn't own a separate stream); `readLine`
  returns `option::none` in interp because there's no real stdin
  attached. Compiled binaries via `phc build` hit the real
  streams through `fwrite` / `fputc` / `fgetc`.

  `Logger::info` is kept as a legacy alias for the existing
  examples corpus; new code should prefer `io::println`.
- **Alternatives considered**: `io::print`/`io::println`/etc. on
  a `stdout` static (more Java-shaped, more typing); fold print
  family under a generic `display`-trait-driven `print` free
  function (depends on D-022's `display` trait which is still
  pending); make `readLine` return `result<string, ioError>`
  (forces an unused E type today; option<string> matches the
  EOF-is-not-an-error reading every other modern lang adopts).
- **Rationale**: Smallest reserved namespace that ships real I/O
  without waiting on D-022's `display` trait. The static-call
  dispatch path is already used by `Logger::info` / `result::ok` /
  `option::some`, so wiring is mechanical. Honest split between
  what's real in compiled binaries vs stubbed in the interp.
- **Date**: 2026-05-16.
- **Status**: locked for v0 surface; binary I/O, file APIs,
  formatted print (`printf`-style), and a real `display` trait
  tracked separately.
