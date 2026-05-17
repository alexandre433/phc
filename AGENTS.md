# AGENTS.md

This file is the durable instruction sheet for coding agents working in the `phc` repository.
It should help any agent understand what PHC is, what has already been decided, what is still open, and how work in this repo should be approached.

## Project overview

PHC is a new compiled programming language focused on:

- PHP-like ergonomics and OOP feel
- C-like runtime performance
- Rust-like memory safety
- Low build times on medium and large codebases through pack-based compilation
- Explicit, readable syntax with a slightly playful identity

The compiler and toolchain are written in **Rust**.

This repository is **mid-implementation** as of 2026-05-16. Phase 1 (formal spec) is locked. Phases 2 / 3 / 5 / 6 / 8 / 9 have shipped MVP slices — see [`spec/README.md`](./spec/README.md) for the current per-phase status. The Rust workspace's crate stubs have been filled in for every phase listed there; `phc-codegen` emits portable C11 today (LLVM/inkwell deferred).

## Canonical sources

Use the following precedence when deciding what is true:

1. User instructions in the current conversation
2. Files in `spec/` — canonical design source. Phase 1 locked 2026-05-11; amendments since (D-023 sigils, D-024 fn-types, D-025 strings, D-026 + D-029 result/option, D-027 + D-030 + D-037 list, D-028 map, D-031 set, D-032 io, D-033 assert, D-034 numeric, D-035 fmt, D-036 lint, D-005 aliasing extension, D-021 v0a) are recorded in `spec/design-decisions.md`'s decision list
3. GitHub issue `#1` — historical context only; `spec/` overrides on any conflict
4. Child phase issues `#2–#11` — implementation roadmap and scope per phase
5. Issue `#12` — post-v0 roadmap

Phase 1 has been completed: the spec files in `spec/` are now the canonical source.

## What has already been decided

These decisions are already locked unless the user explicitly changes them.

### Language identity

- PHC is not a NativePHP-specific language project
- PHC is its own language project
- Compiler/toolchain implementation language: Rust

### Module system

- Modules are called **packs**
- Packs must be acyclic only
- Items are private by default
- Public items must be explicitly declared
- Build strategy should support partitioned, sectioned, and parallel pack compilation

### OOP model

- No class inheritance in v0
- No `extends` keyword in v0
- Reuse is via classes, interfaces, traits, and composition

### Async model

- Async is explicit, using `async` and `await`
- No implicit async promotion
- PHC v0 includes structured concurrency
- Structured concurrency includes task groups and parent-child cancellation
- Async is for runtime concurrency and I/O orchestration, not automatic GPU execution
- GPU compute is a possible future feature, not part of v0
- Compiler builds are CPU-first in v0

### Memory model

- Memory safety is provided by ownership and borrowing, not by tracing GC
- Lifetimes should be inferred by default and only exposed when necessary
- PHC v0 is not move-by-default
- Assignment should feel closer to PHP-style value usage than Rust-style source invalidation
- Built-in strings and built-in collections use copy-on-write semantics

### Mutability

- Variables are immutable by default
- `flip` is the chosen mutability keyword for mutable bindings
- `:=` is the chosen reassignment operator
- Shared borrow syntax is intended as `&name`
- Mutable borrow syntax is intended as `&flip name`
- Avoid bitwise assignment operators such as `^=`, `&=`, and `|=`

### Type system

- Static typing by default
- Dynamic typing is explicit opt-in using `dyn`
- Non-nullable by default
- Nullable types use `?`
- Type inference is preferred where possible
- Verbose generic syntax is undesirable

### Error handling

- Expected and domain failures use Result-style handling
- Exceptions and panics are for exceptional or unrecoverable faults only
- Normal control flow should not rely on unchecked exceptions

## What is still open

Phase 1 resolved every syntax item from issue #1's open checklist (see `spec/design-decisions.md`, D-008…D-020). Implementation since has added D-023 (sigils) and D-024…D-028 (function types + first stdlib slices). The following surfaces remain open and should not be silently expanded:

- **D-021 — Test syntax** — v0a (`phc test`, file-level discovery, interp runner) shipped 2026-05-16. Assertion helpers, cross-file project discovery, filtering, parallel execution remain Phase 9 follow-ups.
- **D-022 — Standard library core surface** — v0a slices D-025 (string), D-026 (result/option methods), D-027 (`list<T>`), D-028 (`map<string, V>`) cut concrete pieces. The broader surface (`display`, `from`/`into`, `taskGroup`, `set<T>`, generic-key maps, hash-based storage) is still Phase 6.
- **Formatter, linter** — Phase 8 (issue #9). LSP minimal shipped 2026-05-16 (`phc lsp`).
- **Lockfile, workspaces, features** — Phase 7 (issue #8).
- **Borrowcheck follow-ups** — aliasing across statements, lifetime / outlives reasoning, lambda capture-mode beyond `:=` enforcement.

When extending these, record the decision in `spec/design-decisions.md` and update `spec/grammar.ebnf` / `spec/language-reference.md` accordingly. Reflect material changes in `CLAUDE.md` and this file too.

## Repo structure

Current structure:

- `Cargo.toml` — Rust workspace root
- `README.md` — project overview and phase table
- `phc/` — CLI binary crate
- `crates/` — library crates for the compiler, tooling, and runtime
- `spec/` — formal language specification area, currently placeholder

### Existing crates

- `phc` — CLI binary
- `phc-ast`
- `phc-lexer`
- `phc-parser`
- `phc-typecheck`
- `phc-borrowcheck`
- `phc-semantic`
- `phc-ir`
- `phc-lower`
- `phc-opt`
- `phc-codegen`
- `phc-runtime`
- `phc-build`
- `phc-pkg`
- `phc-fmt`
- `phc-lint`
- `phc-lsp`
- `phc-test`

Agents should keep work aligned with this structure unless the user asks to reorganize it.

## Phase model

The repo roadmap is split into issues:

- `#2` — Phase 1: Language Specification and Formal Grammar
- `#3` — Phase 2: Lexer and Parser
- `#4` — Phase 3: Type Checker and Semantic Analysis
- `#5` — Phase 4: IR and Lowering
- `#6` — Phase 5: Code Generation and Runtime
- `#7` — Phase 6: Standard Library, Core
- `#8` — Phase 7: Package Manager and Build System
- `#9` — Phase 8: Tooling, Formatter, Linter, and LSP
- `#10` — Phase 9: Testing Framework
- `#11` — Phase 10: Documentation, Examples, and Release
- `#12` — Post-v0 Roadmap

When implementing work, always check which phase the work belongs to.
Do not pull large future-phase work into an earlier phase unless the user explicitly asks for it.

## Agent workflow rules

### 1. Stay phase-correct

- Do not implement Phase 5 codegen details while Phase 1 syntax is still unresolved unless the user explicitly asks for experimental groundwork
- Prefer finishing foundational layers before advanced layers
- If something depends on unresolved syntax or semantics, document assumptions clearly instead of hard-coding arbitrary decisions

### 2. Prefer minimal correct scaffolding

When creating initial files:

- Start with the smallest useful implementation
- Leave clear TODOs referencing the relevant phase
- Avoid fake completeness
- Avoid adding speculative abstractions too early

### 3. Keep the repo consistent with the design

Agents must not introduce features that contradict the current design, such as:

- Class inheritance or `extends`
- GC-based runtime assumptions
- Move-by-default assignment semantics
- Nullable-by-default types
- Implicit async model
- Dynamic-by-default typing

### 4. Rust implementation guidance

When writing Rust code in this repo:

- Prefer clear module boundaries that mirror compiler phases
- Keep crates focused and single-purpose
- Prefer library crates for reusable compiler stages, with the `phc` crate as the CLI entry point
- Prefer explicit types at public boundaries
- Avoid unnecessary macros until there is a real need
- Keep unsafe Rust out unless there is a compelling performance or FFI reason
- If unsafe is introduced, document why it is required

#### Comments

- Every `pub` item (fn, struct, enum, trait, type alias, const, module) gets a `///` doc comment. One sentence minimum; explain *purpose*, not signature
- Every crate `lib.rs` gets a `//!` module-level summary describing the crate's role in the pipeline
- Inline `//` comments only when *why* is non-obvious: a hidden invariant, a subtle ordering constraint, a workaround for a known bug, behaviour that would surprise the reader
- Do not write `// what the code does` — well-named identifiers cover that. Do not reference task IDs, PR numbers, or callers ("used by X", "for issue #42") — that belongs in commit messages and rots in code
- When a comment encodes a spec rule, cite the D-NNN id (e.g. `// D-005: &flip is mutable borrow`) so the link to spec/ stays explicit
- `TODO:` comments must include the phase: `// TODO(phase-3): …`

#### Visibility

- Default to `pub(crate)` for items used only within the same crate
- Promote to `pub` only when the item is part of the crate's intentional public API (re-exported for the next pipeline stage to consume)
- Do not write `pub` for "convenience"; the wider the surface, the harder later refactors become
- A new `pub` item warrants a one-line note in the crate's `lib.rs` `//!` summary if it changes the crate's role

#### Crate dependency DAG

Crates form a directed acyclic graph that mirrors the compiler pipeline. Allowed edges flow left-to-right:

```
phc-span     ← phc-ast ← phc-lexer ← phc-parser ← phc-semantic ← phc-typecheck ← phc-borrowcheck ← phc-ir ← phc-lower ← phc-opt ← phc-codegen
phc-errors   ← every crate
phc-runtime  ← phc-codegen, phc-build
phc-interp   ← phc-typecheck (interim tree-walking interpreter; runs typed AST until codegen lands)
phc-build    ← phc-pkg, phc-codegen, phc-typecheck (and earlier stages it orchestrates)
phc-fmt, phc-lint, phc-lsp, phc-test ← phc-parser (and later stages they consume)
phc          (CLI binary) ← phc-build, phc-pkg, phc-fmt, phc-lint, phc-test, phc-interp
```

- A crate must not depend on any crate to its right in the pipeline. PRs that introduce a back-edge are rejected
- Shared utilities go into `phc-span` or `phc-errors`, not into the crate that happens to need them first
- New cross-cutting crates require a one-line update to this diagram in the same PR

### 5. Diagnostics matter

PHC should aim for excellent diagnostics.
Agents should preserve spans, source locations, and actionable errors wherever possible.
Do not design parser and typechecker internals in ways that make diagnostics an afterthought.

### 6. Build-speed philosophy

PHC cares a lot about iterative build speed.
When making architectural decisions in compiler and build-system code, prefer designs that support:

- pack-level caching
- pack-level invalidation
- parallel compilation across independent packs
- deterministic dependency graphs
- clear pack DAG scheduling

### 7. Do not silently decide syntax

Syntax is still open.
If a task requires syntax that has not been decided yet, either:

- ask the user
- offer 2 to 4 concrete syntax options
- implement syntax-independent infrastructure first

Do not treat placeholder syntax as final.

### 8. Agent parallelism — confirm before fan-out

When working in this repo, an agent (including the main thread) may spawn:

- **0 or 1** subagent without asking
- **2** parallel subagents without asking, if the tasks are clearly independent
- **3 or more** parallel subagents only after asking the user "fan out to N parallel agents, or run sequentially?"

The threshold is parallelism, not total count. Three sequential investigator calls are fine; three concurrent ones need a check. The check exists because parallel fan-out multiplies token cost and makes results harder to reconcile — the user should opt in.

Read-only and write-capable agents follow the same rule. The Plan-mode 3-Explore ceiling is a separate, stricter cap that still applies inside plan mode.

### 9. Branching and PRs

- `main` is the protected trunk. It always reflects releasable state
- `development` is the integration branch. Day-to-day feature work merges here first
- Feature work happens on short-lived branches off `development`, named `feat/<scope>-<short-desc>`, `fix/<scope>-<short-desc>`, `chore/<scope>-<short-desc>`, etc.
- Merging into `main` always requires a pull request. Direct pushes to `main` are forbidden
- Merging a feature branch into `development` may go via PR or direct push, depending on scope; anything user-visible should still go through a PR
- A PR may only merge into `main` after CI is green (see §*Required gates*) and at least one review is recorded
- Rebase or squash-merge to keep history linear; avoid merge commits on `main`
- Delete feature branches once merged

### 9a. Project-pinned Claude Code automations

The repo ships `.claude/settings.json`, skills, and a subagent so collaborators inherit the same workflow without per-user install. Inventory:

- **Plugin** — `caveman@caveman` (from the `JuliusBrussee/caveman` marketplace). Enables caveman-mode communication by default. Toggle off in a session with `stop caveman` or `normal mode`; code, commits, and security text always render in normal English regardless of mode.
- **Hook** — `PostToolUse` on `Edit|Write|MultiEdit` runs `cargo fmt --check --quiet` and prints `phc-fmt-hook: drift detected — run \`cargo fmt\`` on drift. Surfaces fmt drift at edit time, not at CI. Do not silence the hook; fix the drift.
- **Skill** — `/phc-slice <D-###>` (`.claude/skills/phc-slice/SKILL.md`) encodes the validated slice cadence: feat branch → spec amendment → impl → tests → fmt+clippy → ff-merge to `development` → delete branch. Use for every new D-### slice.
- **Skill** — `/phc-d-lookup <D-###>` (`.claude/skills/phc-d-lookup/SKILL.md`) returns the locked spec entry plus cross-refs / amendments in one shot. Use instead of re-reading `spec/design-decisions.md` for a single decision.
- **Subagent** — `spec-guardian` (`.claude/agents/spec-guardian.md`) audits impl against spec for the active D-### before ff-merge. Read-only; returns a drift table only. Spawn before every merge that closes a slice.

Rules of use:

- New automations (hooks, skills, subagents) added under `.claude/` must be appended to this section in the same commit.
- The fmt hook is the only automatic write-side check in `.claude/settings.json`. Adding more PostToolUse commands needs user sign-off — they run on every Edit/Write and are easy to make slow.
- Skills and the subagent are project-pinned; do not duplicate them in user-level `~/.claude/` for this repo.

### 10. Test layout

- **Unit tests** live inline at the bottom of the file under `#[cfg(test)] mod tests { ... }`. Use these for testing crate-private helpers and small invariants
- **Integration tests** live in `crates/<crate>/tests/<feature>.rs`. They consume only the crate's `pub` API. Use these for end-to-end behaviour of a crate's public surface
- **Snapshot tests** for the lexer and parser use the `insta` crate. Snapshots live in `crates/<crate>/tests/snapshots/`. Review snapshot diffs with `cargo insta review` before committing
- Naming: integration tests use the pattern `<phase>_<construct>_<case>` (e.g. `parse_function_decl_with_generics`). Inline unit tests are free-form but must be descriptive
- Tests must encode *why* a behaviour matters, not only *what* it does — the test name should signal intent

## Build and validation commands

Until fuller implementation exists, use these expected commands where possible:

- `cargo check`
- `cargo build`
- `cargo test`
- `cargo fmt`
- `cargo clippy`

As the project evolves, update this section with the exact expected commands.

## Required gates

Every commit pushed to a branch tracked by CI must pass:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo check --workspace --all-targets`
- `cargo test --workspace`

These are non-negotiable. The CI workflow at `.github/workflows/ci.yml` enforces them on every push and pull request. Run them locally before opening a PR.

## License headers

- Every `.rs` file (source, tests, benches) starts with `// SPDX-License-Identifier: MIT` as the first line
- The line stays even when the file is otherwise empty
- New files added in any phase must include the header from the first commit
- Non-Rust files (`.toml`, `.md`, `.yml`, `.phc`) do not require a header

## Feature flags

Cargo features (optional dependencies, conditional code paths) are reserved for true optionality:

- Prefer no feature flags until a real need appears
- When a crate first needs an optional dependency, prefer a Cargo `[features]` entry over `#[cfg(...)]` ad-hoc gating
- Document each new feature in this section as it is introduced (name, what it enables, default state)
- Default features should keep the most-common build green; opt-in features must not break the default build when toggled off

## Observability

- **Logging**: use the `tracing` crate. Prefer structured fields (`tracing::info!(file = %path, "loaded")`) over interpolated strings. Spans (`tracing::info_span!`) wrap async tasks and long-running passes so timings are reconstructable
- `log` (the older crate) is not used in this repo; if a dependency forces it, bridge with `tracing-log`
- **Benchmarks**: use the `criterion` crate. Benches live in `crates/<crate>/benches/<name>.rs`. Goals: track regressions over time and produce numbers comparable against PHP, Rust, and Go for the same workload
- Benches are not part of the CI gate but are run before any release tag
- A bench that grows >10% versus the previous tagged release is a release blocker until investigated

## Deferred follow-ups

These are intentionally postponed but recorded so they are not lost:

- **Supply-chain audit** — wire `cargo-audit` (RustSec advisories) and `cargo-deny` (license + banned-crate policy) into CI before the first 1.0 release
- **Changelog** — adopt the [Keep a Changelog](https://keepachangelog.com) format starting at Phase 10 / first tagged release. No `CHANGELOG.md` exists yet by design
- **Pre-commit hooks** — local `.git/hooks/pre-commit` enforcing fmt + clippy can be added once CI churn proves it useful
- **Documentation rendering** — `#![deny(missing_docs)]` per crate once the first wave of doc comments lands

## Documentation rules

When changing project direction or locking in a decision:

- update issue `#1` if it changes the current design state
- update `spec/` once Phase 1 formal docs exist
- update `README.md` if the repo structure, scope, or onboarding materially changes
- update this `AGENTS.md` if the rule should apply to future agent sessions

## Commit guidance

Every commit must follow this shape:

```
<type>(<scope>): <subject>          # ≤50 chars, imperative, no period

<optional body — required when the "why" is non-obvious>

[phase-N]                           # N = phase number 1..10
Refs #<issue>                       # phase issue: #2 Phase 1, #3 Phase 2, etc.
                                    # use `Closes #N` only when commit completes a checklist item
```

Rules:

- `<type>` ∈ `{feat, fix, chore, refactor, docs, test, perf, build, ci}`
- `<scope>` is the crate name without the `phc-` prefix (e.g. `parser`, `ir`, `spec`) or `repo` for cross-cutting changes
- Subject: imperative mood ("add", not "added"/"adds"), no trailing period, ≤50 chars
- Body: required whenever the diff alone does not explain *why*. Wrap at 72 chars. Explain motivation, not mechanics
- `[phase-N]` tag and `Refs #N` footer are mandatory on any commit that advances a phase. `chore` and `docs` commits that touch only meta files (READMEs, this file) may omit both

Examples:

```
feat(lexer): add string interpolation token

Per D-017 string interpolation is always on; lexer must split
`"hi {name}"` into [StrLit, Interp, Ident, Interp, StrLit]
so the parser sees a real expression tree, not a post-processed
string.

[phase-2]
Refs #3
```

```
docs(spec): close Phase 1

[phase-1]
Closes #2
```

## What success looks like

A good agent contribution in this repo:

- respects existing PHC decisions
- stays within the correct phase boundary
- keeps Rust code clean and modular
- improves future implementability
- avoids inventing unresolved language syntax
- leaves the repository easier for the next agent to pick up
- conforms to every rule in this document — branching, commit format, comments, dep DAG, gates, headers

## Maintenance note

If the user corrects a recurring assumption or establishes a new permanent rule, update this file so future agents inherit the correction.
