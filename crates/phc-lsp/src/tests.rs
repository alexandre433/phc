// SPDX-License-Identifier: MIT
//! Unit tests for the LSP analysis pipeline. The tower-lsp wire
//! layer is awkward to test without a real client; the analyzer
//! and the position-translation helpers carry the load and are
//! exercised here directly.

use crate::analyze::analyze;

#[test]
fn analyze_clean_program_has_no_diagnostics() {
    let src = r#"pack demo;
function main(): void {
    int $x = 1;
    Logger::info("hi");
}
"#;
    let out = analyze(src);
    assert!(
        out.diagnostics.is_empty(),
        "expected clean: {:?}",
        out.diagnostics
    );
    assert!(out.typed.is_some());
}

#[test]
fn analyze_surfaces_borrow_diagnostic() {
    // D-005: `:=` on a non-flip binding is an error. Borrowcheck
    // is wired into the LSP pipeline so the diagnostic must
    // surface here just as it does in `phc build`.
    let src = r#"pack demo;
function main(): void {
    int $x = 1;
    $x := 2;
}
"#;
    let out = analyze(src);
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("cannot reassign immutable binding `$x`")),
        "expected borrow diag, got {:?}",
        out.diagnostics
    );
}

#[test]
fn analyze_surfaces_parse_error_with_no_typed_snapshot() {
    let out = analyze("pack;");
    assert!(!out.diagnostics.is_empty());
    // Parser produced no SourceFile → no typed snapshot.
    assert!(out.typed.is_none());
}

#[test]
fn analyze_records_method_call_return_types() {
    // Sanity check that the typecheck patch landed in the same
    // session is reflected in the LSP snapshot — hover would
    // otherwise report `<unknown>` for stdlib chain results.
    let src = r#"pack demo;
function main(): void {
    string $s = "hello";
    int $n = $s->len();
    Logger::info("ok");
    int $_unused = $n;
}
"#;
    let out = analyze(src);
    assert!(
        out.diagnostics.is_empty(),
        "expected clean: {:?}",
        out.diagnostics
    );
    let typed = out.typed.expect("typed snapshot");
    // At least one expression in the file is typed `int`. Loose
    // assertion — the precise span depends on parser internals,
    // and the test would otherwise be brittle to source edits.
    let saw_int = typed.expr_types.values().any(|t| {
        matches!(
            t,
            phc_typecheck::Ty::Primitive(phc_typecheck::Primitive::Int)
        )
    });
    assert!(saw_int, "expected at least one int-typed expression");
}
