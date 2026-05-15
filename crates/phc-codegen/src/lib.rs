// SPDX-License-Identifier: MIT
//! Code generation. Today: emit portable C from a typed PHC
//! [`SourceFile`](phc_ast::SourceFile). The compile pipeline
//! (`phc-build`) writes the C, invokes `cc` / `gcc`, and produces a
//! native binary.
//!
//! The LLVM-via-IR backend stays planned for v1. The C-emit path is
//! interim and lets the language ship binaries while IR + codegen
//! mature in parallel.
//!
//! Scope today: enough to build `examples/hello.phc`. Each follow-up
//! commit adds one feature axis (control flow, classes, enums, ...)
//! mirroring the interpreter's I1..I7 progression.

mod c_emit;

#[cfg(test)]
mod tests;

pub use c_emit::emit_c;

use phc_errors::Diagnostic;

/// Result of running the C-emit pass on one source file.
#[derive(Debug, Default)]
pub struct CodegenOutput {
    /// The full C source (one translation unit), ready to be passed
    /// to `cc` / `gcc` alongside the runtime sources.
    pub c_source: String,
    /// Codegen-time diagnostics. Construct emission is best-effort:
    /// unsupported constructs surface as a diagnostic and the
    /// emitted source compiles to a runtime trap (`phc_panic`) at
    /// the matching position rather than aborting the whole build.
    pub diagnostics: Vec<Diagnostic>,
}
