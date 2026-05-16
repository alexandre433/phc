# PHC

> A modern programming language with PHP-like ergonomics, C-like speed, and Rust-like memory safety.

## Goals

- **PHP-like ergonomics** — familiar OOP, traits, interfaces, and composition
- **C-like performance** — compiled to native code via LLVM, no garbage collector
- **Rust-like memory safety** — ownership and borrowing enforced at compile time
- **Fast build times** — acyclic pack system enables parallel and incremental compilation
- **Explicit and fun** — `async/await`, immutable by default, `flip` for mutability

## Status

Mid-implementation. Phase 1 (formal spec) is locked; Phases 2 / 3 / 5 / 6 / 8 / 9 have shipped MVP slices since.

Today you can:
- Compile a `.phc` source to a native binary (`phc build <file>` → `cc` → ELF/PE).
- Run a `.phc` source through a tree-walking interpreter (`phc run <file>`).
- Run tests in a `.phc` source (`phc test <file>` discovers `test "name" { ... }` blocks).
- Get diagnostics + hover types in any LSP-speaking editor (`phc lsp` over stdio).

Implemented language surface (compiled and interpreted in lockstep):
- Functions, classes (with `construct`, methods, fields, hooks), enums, interfaces, traits.
- Pattern `match` with exhaustiveness; lambdas (inline + stored via `fn(...): R` types per D-024).
- Mutability + borrows enforced (D-005 / D-005a / D-018, plus per-call aliasing).
- Stdlib v0a: string methods (D-025), `list<T>` with indexing + `for` (D-027), `map<string, V>` (D-028), Result/Option ergonomic methods (D-026), postfix `?` propagation.

See [`spec/`](./spec/) for canonical decisions and [`spec/design-decisions.md`](./spec/design-decisions.md) for the full list (D-001…D-028 plus amendments).

## Roadmap

| Phase | Description | Issue |
|-------|-------------|-------|
| 1 | Language Specification & Formal Grammar | [#2](https://github.com/alexandre433/phc/issues/2) |
| 2 | Lexer & Parser | [#3](https://github.com/alexandre433/phc/issues/3) |
| 3 | Type Checker & Semantic Analysis | [#4](https://github.com/alexandre433/phc/issues/4) |
| 4 | Intermediate Representation (IR) & Lowering | [#5](https://github.com/alexandre433/phc/issues/5) |
| 5 | Code Generation & Runtime | [#6](https://github.com/alexandre433/phc/issues/6) |
| 6 | Standard Library (Core) | [#7](https://github.com/alexandre433/phc/issues/7) |
| 7 | Package Manager & Build System | [#8](https://github.com/alexandre433/phc/issues/8) |
| 8 | Tooling: Formatter, Linter & LSP | [#9](https://github.com/alexandre433/phc/issues/9) |
| 9 | Testing Framework | [#10](https://github.com/alexandre433/phc/issues/10) |
| 10 | Documentation, Examples & Release | [#11](https://github.com/alexandre433/phc/issues/11) |

## Repository Structure

```
phc/
├── Cargo.toml          # Rust workspace root
├── phc/                # CLI binary
├── crates/
│   ├── phc-ast/        # AST node definitions
│   ├── phc-lexer/      # Lexer
│   ├── phc-parser/     # Parser
│   ├── phc-typecheck/  # Type checker
│   ├── phc-borrowcheck/# Borrow checker
│   ├── phc-semantic/   # Semantic analysis
│   ├── phc-ir/         # Intermediate representation
│   ├── phc-lower/      # AST → IR lowering
│   ├── phc-opt/        # Optimisation passes
│   ├── phc-codegen/    # LLVM code generation
│   ├── phc-runtime/    # Runtime (async, CoW, panic)
│   ├── phc-fmt/        # Formatter
│   ├── phc-lint/       # Linter
│   ├── phc-lsp/        # Language Server Protocol
│   ├── phc-build/      # Build system
│   ├── phc-pkg/        # Package manager
│   └── phc-test/       # Testing framework
└── spec/               # Language specification and formal grammar
```

## Built With

- [Rust](https://www.rust-lang.org/)
- A C compiler (`cc` / `gcc` / `clang`) — codegen emits portable C11 today; LLVM/inkwell path reserved for later.
- [logos](https://github.com/maciejhirsz/logos) — fast lexer
- [miette](https://github.com/zkat/miette) — rich diagnostics
- [tower-lsp](https://github.com/ebkalderon/tower-lsp) — LSP server

## License

MIT
