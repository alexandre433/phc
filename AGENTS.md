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

This repository is currently in the design and scaffolding stage.
The repo already contains a Rust workspace skeleton with crate placeholders for each major compiler/toolchain subsystem.

## Canonical sources

Use the following precedence when deciding what is true:

1. User instructions in the current conversation
2. Files in `spec/` — canonical design source as of 2026-05-11 (Phase 1 complete)
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

Phase 1 resolved every syntax item from issue #1's open checklist (see `spec/design-decisions.md`, D-008…D-020). The remaining provisional items are:

- **D-021 — Test syntax** (provisional; finalised in Phase 9 / issue #10)
- **D-022 — Standard library core surface** (provisional; finalised in Phase 6 / issue #7)
- **Formatter, linter, LSP details** — Phase 8 (issue #9)
- **Lockfile, workspaces, features** — Phase 7 (issue #8)

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

## Build and validation commands

Until fuller implementation exists, use these expected commands where possible:

- `cargo check`
- `cargo build`
- `cargo test`
- `cargo fmt`
- `cargo clippy`

As the project evolves, update this section with the exact expected commands.

## Documentation rules

When changing project direction or locking in a decision:

- update issue `#1` if it changes the current design state
- update `spec/` once Phase 1 formal docs exist
- update `README.md` if the repo structure, scope, or onboarding materially changes
- update this `AGENTS.md` if the rule should apply to future agent sessions

## Commit guidance

Prefer small commits with clear intent, for example:

- `chore: scaffold parser crate`
- `feat(parser): add token stream abstraction`
- `docs(spec): add operator precedence draft`
- `refactor(ir): split control-flow nodes from value nodes`

## What success looks like

A good agent contribution in this repo:

- respects existing PHC decisions
- stays within the correct phase boundary
- keeps Rust code clean and modular
- improves future implementability
- avoids inventing unresolved language syntax
- leaves the repository easier for the next agent to pick up

## Maintenance note

If the user corrects a recurring assumption or establishes a new permanent rule, update this file so future agents inherit the correction.
