// SPDX-License-Identifier: MIT
//! Tests for the D-036 v0a lint ruleset.

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
