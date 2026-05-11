# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

PHC is a new compiled programming language (PHP ergonomics, C speed, Rust memory safety). Compiler and toolchain written in Rust. Currently in design and scaffolding phase.

## Build Commands

```
cargo check
cargo build
cargo test
cargo fmt
cargo clippy
```

Run single test: `cargo test -p <crate-name> <test_name>`

## Architecture

Rust workspace. Pipeline: source → lexer → parser → AST → semantic → typecheck → borrowcheck → IR → lower → opt → codegen → binary.

Crate responsibilities:
- `phc` — CLI entry point
- `phc-ast` — AST node definitions
- `phc-lexer` — tokenization (logos)
- `phc-parser` — parsing
- `phc-semantic` — name resolution, semantic analysis
- `phc-typecheck` — type inference and checking
- `phc-borrowcheck` — ownership/borrow checking
- `phc-ir` — intermediate representation
- `phc-lower` — AST → IR lowering
- `phc-opt` — optimization passes
- `phc-codegen` — LLVM codegen (inkwell)
- `phc-runtime` — async, CoW, panic support
- `phc-build` — build system (pack-level caching, parallel compilation)
- `phc-pkg` — package manager
- `phc-fmt` — formatter
- `phc-lint` — linter
- `phc-lsp` — LSP server (tower-lsp, miette for diagnostics)
- `phc-test` — testing framework

`spec/` holds the formal language specification (currently placeholder).

## Canonical Design Source

Precedence order:
1. User instructions in current conversation
2. Files in `spec/` — canonical design source as of 2026-05-11 (Phase 1 complete)
3. GitHub issue `#1` — historical context only; `spec/` overrides on any conflict
4. Issues `#2–#11` — phase implementation roadmap

## Locked Design Decisions

Do not contradict these unless the user explicitly changes them:

- **Modules**: called *packs*, acyclic only, private by default
- **OOP**: no class inheritance, no `extends` in v0; reuse via traits, interfaces, composition
- **Async**: explicit `async`/`await`, structured concurrency (task groups, parent-child cancellation), no implicit promotion
- **Memory**: ownership + borrowing (not GC); lifetimes inferred by default; CoW for strings and collections
- **Mutability**: immutable by default; `flip` keyword for mutable bindings; `:=` for reassignment; `&name` shared borrow, `&flip name` mutable borrow
- **Types**: static by default; `dyn` for explicit dynamic opt-in; non-nullable by default; `?` for nullable
- **Errors**: Result-style for domain failures; exceptions/panics only for unrecoverable faults

## Still Open — Do Not Invent

Phase 1 closed every syntax decision listed in issue #1's checklist (see `spec/design-decisions.md`, D-008…D-020). Only the following remain provisional and must not be silently expanded:

- D-021 — Test syntax. Locked enough to reserve the `test` keyword; full surface is Phase 9 work.
- D-022 — Standard library core surface. Primitive widths, collection method surfaces, `Result`/`Option`/`Display`/`From`/`Into` shapes are Phase 6 work.

Any syntax decision not yet recorded in `spec/`: ask, or offer 2–4 concrete options.

## Phase Model

| Issue | Phase |
|-------|-------|
| `#2` | Phase 1: Language Spec & Formal Grammar |
| `#3` | Phase 2: Lexer & Parser |
| `#4` | Phase 3: Type Checker & Semantic Analysis |
| `#5` | Phase 4: IR & Lowering |
| `#6` | Phase 5: Code Generation & Runtime |
| `#7` | Phase 6: Standard Library |
| `#8` | Phase 7: Package Manager & Build System |
| `#9` | Phase 8: Tooling (fmt, lint, LSP) |
| `#10` | Phase 9: Testing Framework |
| `#11` | Phase 10: Docs & Release |

Do not pull future-phase work into earlier phases unless explicitly asked.

## Rust Implementation Rules

- Keep crates single-purpose; mirror compiler phases
- Explicit types at public boundaries
- No unnecessary macros
- No `unsafe` unless required for performance/FFI — document why
- Preserve spans and source locations; diagnostics are first-class
- Design for pack-level caching, invalidation, and parallel compilation

## Commit Style

```
chore: scaffold parser crate
feat(parser): add token stream abstraction
docs(spec): add operator precedence draft
refactor(ir): split control-flow nodes from value nodes
```

## Behavioral Guidelines

**Think before coding.** State assumptions. Surface tradeoffs. If multiple interpretations exist, present them. If unclear, stop and ask.

**Simplicity first.** Minimum code. No speculative abstractions. If 200 lines could be 50, rewrite. Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

**Surgical changes.** Touch only what the task requires. Match existing style. Mention unrelated dead code, don't delete it. Every changed line should trace directly to the user's request.

**Goal-driven execution.** Define verifiable success criteria before starting. For multi-step tasks, state a brief plan:

```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

**Use model for judgment only.** Classification, drafting, summarization. Not routing, retries, or deterministic transforms.

**Token budgets.** Per-task: 4,000 tokens. Per-session: 30,000. Summarize and start fresh if approaching limit. Surface the breach.

**Surface conflicts.** If two patterns contradict, pick one (more recent/tested), explain why, flag the other.

**Read before write.** Before adding code, read exports, callers, shared utilities.

**Tests verify intent.** Tests must encode WHY behavior matters, not just WHAT it does.

**Checkpoint after significant steps.** Summarize what's done, verified, and left.

**Match codebase conventions.** Conformance over taste. Surface genuine concerns, don't fork silently.

**Fail loud.** Never silently skip. Surface uncertainty.

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.
