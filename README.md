# PHC

> A modern programming language with PHP-like ergonomics, C-like speed, and Rust-like memory safety.

## Goals

- **PHP-like ergonomics** — familiar OOP, traits, interfaces, and composition
- **C-like performance** — compiled to native code via LLVM, no garbage collector
- **Rust-like memory safety** — ownership and borrowing enforced at compile time
- **Fast build times** — acyclic pack system enables parallel and incremental compilation
- **Explicit and fun** — `async/await`, immutable by default, `flip` for mutability

## Status

PHC is in early design and scaffolding phase. See [issue #1](https://github.com/alexandre433/phc/issues/1) for all locked language design decisions and the open decisions still being resolved.

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
- [LLVM](https://llvm.org/) (via `inkwell`)
- [logos](https://github.com/maciejhirsz/logos) — fast lexer
- [miette](https://github.com/zkat/miette) — rich diagnostics
- [tower-lsp](https://github.com/ebkalderon/tower-lsp) — LSP server

## License

MIT
