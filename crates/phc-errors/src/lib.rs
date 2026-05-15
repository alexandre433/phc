// SPDX-License-Identifier: MIT
//! Shared diagnostic and error types for the PHC compiler.
//!
//! Crate-specific error enums live in their owning crate; this crate
//! provides only the cross-cutting building blocks (severity levels,
//! the common diagnostic shape) so every layer reports errors the
//! same way and the CLI can render them with `miette`.

use phc_span::Span;

/// Severity of a compiler diagnostic.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Severity {
    /// Compilation cannot continue; produce no artefact.
    Error,
    /// Compilation continues but the user should fix the issue.
    Warning,
    /// Informational note attached to a primary diagnostic.
    Note,
}

/// Minimal diagnostic shape carried between compiler stages.
///
/// Concrete error enums in each crate convert into this type via
/// `Into<Diagnostic>` so the CLI and LSP can render them uniformly.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Span,
}

// TODO(phase-2): introduce the first concrete error enum once the
// lexer produces real diagnostics; wire it through `Into<Diagnostic>`.
