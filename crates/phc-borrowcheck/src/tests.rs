// SPDX-License-Identifier: MIT
//! Unit tests for the borrowcheck MVP. Each test drives the full
//! parse → resolve → typecheck → borrowcheck pipeline so the
//! behaviour matches what the build pass actually sees.

use phc_parser::parse as parse_source;
use phc_semantic::resolve;
use phc_span::FileId;
use phc_typecheck::typecheck;

use crate::borrowcheck;

fn diagnostics_for(src: &str) -> Vec<String> {
    let parsed = parse_source(src, FileId(0));
    assert!(
        parsed.diagnostics.is_empty(),
        "parse errors: {:?}",
        parsed.diagnostics
    );
    let file = parsed.file.expect("parsed source");
    let resolved = resolve(&file);
    assert!(
        resolved.diagnostics.is_empty(),
        "resolve errors: {:?}",
        resolved.diagnostics
    );
    let typed = typecheck(&file, &resolved);
    assert!(
        typed.diagnostics.is_empty(),
        "typecheck errors: {:?}",
        typed.diagnostics
    );
    let borrowed = borrowcheck(&file, &resolved, &typed);
    borrowed
        .diagnostics
        .into_iter()
        .map(|d| d.message)
        .collect()
}

#[test]
fn reassign_immutable_local_is_rejected() {
    // `int $x` (no flip) cannot accept `:=`; D-005.
    let src = r#"pack demo;
function main(): void {
    int $x = 1;
    $x := 2;
}
"#;
    let diags = diagnostics_for(src);
    assert!(
        diags
            .iter()
            .any(|d| d.contains("cannot reassign immutable binding `$x`")),
        "expected mutability diagnostic, got {diags:?}"
    );
}

#[test]
fn reassign_flip_local_is_allowed() {
    let src = r#"pack demo;
function main(): void {
    flip int $x = 1;
    $x := 2;
}
"#;
    assert!(diagnostics_for(src).is_empty());
}

#[test]
fn mutable_borrow_of_immutable_is_rejected() {
    // `&flip $x` requires `$x` to be flip per D-005.
    let src = r#"pack demo;
function main(): void {
    int $x = 1;
    int $y = sink(&flip $x);
}
function sink(&flip int $r): int { return 0; }
"#;
    let diags = diagnostics_for(src);
    assert!(
        diags
            .iter()
            .any(|d| d.contains("cannot mutably borrow `$x`")),
        "expected borrow diagnostic, got {diags:?}"
    );
}

#[test]
fn mutable_borrow_of_flip_is_allowed() {
    let src = r#"pack demo;
function main(): void {
    flip int $x = 1;
    int $y = sink(&flip $x);
}
function sink(&flip int $r): int { return 0; }
"#;
    assert!(diagnostics_for(src).is_empty());
}

#[test]
fn member_assign_immutable_field_is_rejected() {
    // D-005a: `$obj->field = expr;` only legal when field is `flip`.
    let src = r#"pack demo;
public class Box {
    int $value = 0;
}
function bump(Box $b): void {
    $b->value = 1;
}
function main(): void {
    Box $b = Box();
    bump($b);
}
"#;
    let diags = diagnostics_for(src);
    assert!(
        diags
            .iter()
            .any(|d| d.contains("cannot assign to immutable field `value`")),
        "expected field-mutability diagnostic, got {diags:?}"
    );
}

#[test]
fn constructor_can_initialise_immutable_this_field() {
    // D-012: inside `construct(...)`, `$this->field = expr;` is
    // initialisation, not reassignment — allowed even on a
    // non-flip field. After construction, the same write would
    // be rejected.
    let src = r#"pack demo;
public class Box {
    construct() {
        $this->value = 7;
    }
    int $value = 0;
}
function main(): void {
    Box $b = Box();
}
"#;
    assert!(diagnostics_for(src).is_empty());
}

#[test]
fn member_assign_flip_field_is_allowed() {
    let src = r#"pack demo;
public class Box {
    flip int $value = 0;
}
function bump(Box $b): void {
    $b->value = 1;
}
function main(): void {
    Box $b = Box();
    bump($b);
}
"#;
    assert!(diagnostics_for(src).is_empty());
}
