// SPDX-License-Identifier: MIT
//! Pipeline driver: source string → diagnostics + Typed snapshot.
//!
//! Used by the LSP backend to power `publishDiagnostics` after
//! every document edit and `hover` lookups against the latest
//! typed snapshot.

use phc_borrowcheck::borrowcheck;
use phc_errors::{Diagnostic, Severity};
use phc_parser::parse;
use phc_semantic::resolve;
use phc_span::FileId;
use phc_typecheck::{typecheck, Typed};

/// Output of analysing a single document. `typed` is `None` only
/// when parsing failed badly enough that no `SourceFile` was
/// produced; otherwise the later passes run even if earlier ones
/// emitted diagnostics, so the snapshot reflects best-effort
/// information for hover / future features.
pub struct AnalysisOutput {
    pub diagnostics: Vec<Diagnostic>,
    pub typed: Option<Typed>,
}

pub fn analyze(source: &str) -> AnalysisOutput {
    let mut all = Vec::new();
    let parsed = parse(source, FileId(0));
    all.extend(parsed.diagnostics.iter().cloned());
    let Some(file) = parsed.file else {
        return AnalysisOutput {
            diagnostics: promote_to_errors(all),
            typed: None,
        };
    };
    let resolved = resolve(&file);
    all.extend(resolved.diagnostics.iter().cloned());
    let typed = typecheck(&file, &resolved);
    all.extend(typed.diagnostics.iter().cloned());
    let borrowed = borrowcheck(&file, &resolved, &typed);
    all.extend(borrowed.diagnostics.iter().cloned());
    AnalysisOutput {
        diagnostics: all,
        typed: Some(typed),
    }
}

/// Parser diagnostics already carry severity; keep this helper for
/// the no-SourceFile branch in case future parser changes lower
/// some warnings to a non-error severity that would silently hide
/// the failure when no AST was produced.
fn promote_to_errors(mut diags: Vec<Diagnostic>) -> Vec<Diagnostic> {
    for d in &mut diags {
        if !matches!(d.severity, Severity::Error) {
            d.severity = Severity::Error;
        }
    }
    diags
}
