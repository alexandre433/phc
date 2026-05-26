// SPDX-License-Identifier: MIT
//! Tests for the D-036 v0a + D-036b lint ruleset.

use crate::lint_source;

#[test]
fn clean_program_has_no_warnings() {
    let src = r#"pack demo;
public function add(int $a, int $b): int { return $a + $b; }
"#;
    let report = lint_source(src);
    assert!(
        report.setup_errors.is_empty(),
        "setup errors: {:?}",
        report.setup_errors
    );
    assert!(
        report.warnings.is_empty(),
        "unexpected warnings: {:?}",
        report.warnings
    );
}

#[test]
fn unused_local_is_warned() {
    let src = r#"pack demo;
function main(): void {
    int $unused = 42;
    int $used = 1;
    int $also_unused = $used + 1;
    Logger::info("{$also_unused}");
}
"#;
    let report = lint_source(src);
    assert!(
        report
            .warnings
            .iter()
            .any(|d| d.message.contains("unused local binding `$unused`")),
        "expected unused warning, got {:?}",
        report.warnings
    );
    // `$also_unused` is referenced inside the string interp, so it
    // shouldn't fire.
    assert!(
        !report
            .warnings
            .iter()
            .any(|d| d.message.contains("$also_unused")),
        "unexpected $also_unused warning"
    );
}

#[test]
fn underscore_prefixed_locals_are_ignored() {
    let src = r#"pack demo;
function main(): void {
    int $_skip = 7;
}
"#;
    let report = lint_source(src);
    assert!(
        report.warnings.is_empty(),
        "expected no warnings, got {:?}",
        report.warnings
    );
}

#[test]
fn unreachable_statement_after_return_is_warned() {
    let src = r#"pack demo;
function f(): int {
    return 1;
    int $x = 2;
}
"#;
    let report = lint_source(src);
    assert!(
        report
            .warnings
            .iter()
            .any(|d| d.message.contains("unreachable statement after `return`")),
        "expected unreachable warning, got {:?}",
        report.warnings
    );
}

#[test]
fn non_pascal_class_name_is_warned() {
    let src = r#"pack demo;
public class user_account { }
"#;
    let report = lint_source(src);
    assert!(
        report.warnings.iter().any(|d| d
            .message
            .contains("class name `user_account` should be PascalCase")),
        "expected class-naming warning, got {:?}",
        report.warnings
    );
}

#[test]
fn pascal_class_name_is_clean() {
    let src = r#"pack demo;
public class User { construct() {} }
"#;
    let report = lint_source(src);
    assert!(
        report.warnings.is_empty(),
        "expected no warnings, got {:?}",
        report.warnings
    );
}

// ===== shadow_local tests =====

#[test]
fn shadow_local_is_warned() {
    let src = r#"pack demo;
function main(): void {
    int $x = 1;
    if (true) {
        int $x = 2;
    }
}
"#;
    let report = lint_source(src);
    assert!(
        report.setup_errors.is_empty(),
        "setup errors: {:?}",
        report.setup_errors
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|d| d.message.contains("local '$x' shadows an outer binding")),
        "expected shadow warning, got {:?}",
        report.warnings
    );
}

#[test]
fn shadow_local_different_names_are_clean() {
    let src = r#"pack demo;
function main(): void {
    int $x = 1;
    if (true) {
        int $y = 2;
    }
}
"#;
    let report = lint_source(src);
    assert!(
        report.setup_errors.is_empty(),
        "setup errors: {:?}",
        report.setup_errors
    );
    // shadow_local should not fire; dead_branch may fire for literal `true`
    assert!(
        !report
            .warnings
            .iter()
            .any(|d| d.message.contains("shadows an outer binding")),
        "unexpected shadow warning, got {:?}",
        report.warnings
    );
}

// ===== dead_branch tests =====

#[test]
fn dead_branch_true_condition_is_warned() {
    let src = r#"pack demo;
function main(): void {
    if (true) {
        int $x = 1;
    }
}
"#;
    let report = lint_source(src);
    assert!(
        report.setup_errors.is_empty(),
        "setup errors: {:?}",
        report.setup_errors
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|d| d.message.contains("if condition is always true")),
        "expected dead_branch warning, got {:?}",
        report.warnings
    );
}

#[test]
fn dead_branch_false_condition_is_warned() {
    let src = r#"pack demo;
function main(): void {
    if (false) {
        int $x = 1;
    }
}
"#;
    let report = lint_source(src);
    assert!(
        report.setup_errors.is_empty(),
        "setup errors: {:?}",
        report.setup_errors
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|d| d.message.contains("if condition is always false")),
        "expected dead_branch warning, got {:?}",
        report.warnings
    );
}

#[test]
fn dead_branch_dynamic_condition_is_clean() {
    let src = r#"pack demo;
function main(): void {
    int $x = 0;
    if ($x > 0) {
        int $y = 1;
    }
}
"#;
    let report = lint_source(src);
    assert!(
        report.setup_errors.is_empty(),
        "setup errors: {:?}",
        report.setup_errors
    );
    assert!(
        !report
            .warnings
            .iter()
            .any(|d| d.message.contains("always true") || d.message.contains("always false")),
        "unexpected dead_branch warning, got {:?}",
        report.warnings
    );
}

// ===== Walker-arm coverage for the underexercised paths in
// `walk_block_for_unreachable` and `collect_locals` — class methods,
// trait methods, test blocks, lambdas, loops, reassigns. =====

#[test]
fn pascal_naming_fires_on_enum_interface_trait() {
    let src = r#"pack demo;
public enum status { Ok }
public interface drawable { function draw(): void; }
public trait loggable {
    function log(): void { Logger::info("hi"); }
}
"#;
    let report = lint_source(src);
    let names_with_warning: Vec<&str> = report
        .warnings
        .iter()
        .filter(|d| d.message.contains("PascalCase"))
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        names_with_warning.iter().any(|m| m.contains("enum")),
        "expected enum PascalCase warning: {:?}",
        report.warnings
    );
    assert!(
        names_with_warning.iter().any(|m| m.contains("interface")),
        "expected interface PascalCase warning: {:?}",
        report.warnings
    );
    assert!(
        names_with_warning.iter().any(|m| m.contains("trait")),
        "expected trait PascalCase warning: {:?}",
        report.warnings
    );
}

#[test]
fn unused_local_in_class_method_is_warned() {
    let src = r#"pack demo;
public class Box {
    construct(public int $value) {}
    public function noop(): void {
        int $unused = 5;
    }
}
"#;
    let report = lint_source(src);
    assert!(
        report.warnings.iter().any(|d| d.message.contains("unused")),
        "expected unused-local warning inside class method: {:?}",
        report.warnings
    );
}

#[test]
fn unused_local_in_trait_method_is_warned() {
    let src = r#"pack demo;
public trait Doubler {
    function doubled(): int {
        int $stash = 7;
        return 0;
    }
}
"#;
    let report = lint_source(src);
    assert!(
        report.warnings.iter().any(|d| d.message.contains("unused")),
        "expected unused-local warning inside trait method: {:?}",
        report.warnings
    );
}

#[test]
fn unused_local_in_test_block_is_warned() {
    let src = r#"pack demo;
test "scratch" {
    int $unused = 1;
    assert::isTrue(true);
}
"#;
    let report = lint_source(src);
    assert!(
        report.warnings.iter().any(|d| d.message.contains("unused")),
        "expected unused-local warning inside test block: {:?}",
        report.warnings
    );
}

#[test]
fn unreachable_after_return_in_class_method_is_warned() {
    let src = r#"pack demo;
public class Box {
    construct(public int $value) {}
    public function get(): int {
        return $this->value;
        Logger::info("unreachable");
    }
}
"#;
    let report = lint_source(src);
    assert!(
        report
            .warnings
            .iter()
            .any(|d| d.message.contains("unreachable")),
        "expected unreachable warning inside class method: {:?}",
        report.warnings
    );
}

#[test]
fn unreachable_after_return_in_while_body_is_warned() {
    let src = r#"pack demo;
public function loop(): void {
    while (true) {
        return;
        Logger::info("unreachable");
    }
}
"#;
    let report = lint_source(src);
    assert!(
        report
            .warnings
            .iter()
            .any(|d| d.message.contains("unreachable")),
        "expected unreachable warning inside while body: {:?}",
        report.warnings
    );
}

#[test]
fn unused_local_in_for_loop_body_is_warned() {
    let src = r#"pack demo;
public function each(): void {
    list<int> $xs = list();
    $xs->push(1);
    for (int $x in $xs) {
        int $unused = $x + 1;
    }
}
"#;
    let report = lint_source(src);
    assert!(
        report.warnings.iter().any(|d| d.message.contains("unused")),
        "expected unused-local warning inside for-loop body: {:?}",
        report.warnings
    );
}

#[test]
fn report_ok_method_reflects_warning_state() {
    // Smoke-test the `Report::ok()` helper so it gets coverage in
    // both states.
    let clean = lint_source("pack a;\npublic function f(): int { return 1; }\n");
    assert!(clean.ok(), "clean report should be ok");
    let dirty = lint_source(
        "pack a;\npublic function f(): void { int $unused = 1; Logger::info(\"hi\"); }\n",
    );
    assert!(!dirty.ok(), "dirty report should not be ok");
}
