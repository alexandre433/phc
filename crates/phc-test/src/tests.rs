// SPDX-License-Identifier: MIT
//! Unit tests for the test framework MVP.

use crate::run_source;

#[test]
fn passing_test_increments_passed_count() {
    let src = r#"pack demo;
test "smoke" {
    int $x = 1;
    int $y = $x + 1;
}
"#;
    let report = run_source(src);
    assert_eq!(report.passed, 1);
    assert_eq!(report.failed, 0);
    assert!(report.ok());
}

#[test]
fn failing_test_is_recorded_with_message() {
    // `at(99)` on a 0-len list panics — the runtime error is the
    // test's failure signal.
    let src = r#"pack demo;
test "out of bounds" {
    list<int> $xs = list();
    int $_ = $xs->at(99);
}
"#;
    let report = run_source(src);
    assert_eq!(report.passed, 0);
    assert_eq!(report.failed, 1);
    assert_eq!(report.results.len(), 1);
    let r = &report.results[0];
    assert_eq!(r.name, "out of bounds");
    assert!(!r.passed);
    assert!(r.errors.iter().any(|e| e.message.contains("out of bounds")));
}

#[test]
fn one_failing_test_does_not_stop_later_tests() {
    let src = r#"pack demo;
test "fails" {
    list<int> $xs = list();
    int $_ = $xs->at(99);
}
test "passes" {
    int $x = 1 + 1;
}
"#;
    let report = run_source(src);
    assert_eq!(report.passed, 1);
    assert_eq!(report.failed, 1);
    assert_eq!(report.results.len(), 2);
    assert!(report.results[0].name == "fails" && !report.results[0].passed);
    assert!(report.results[1].name == "passes" && report.results[1].passed);
}

#[test]
fn pipeline_diagnostics_abort_before_any_test_runs() {
    // Borrowcheck rejects `:=` on a non-flip binding. Setup
    // diagnostics surface before any test executes.
    let src = r#"pack demo;
test "uses immutable rebind" {
    int $x = 1;
    $x := 2;
}
"#;
    let report = run_source(src);
    assert!(!report.setup_diagnostics.is_empty());
    assert_eq!(report.passed, 0);
    assert_eq!(report.failed, 0);
    assert!(report.results.is_empty());
}

#[test]
fn test_stdout_is_captured_per_test() {
    let src = r#"pack demo;
test "logs once" {
    Logger::info("hi");
}
"#;
    let report = run_source(src);
    assert_eq!(report.passed, 1);
    assert_eq!(report.results[0].stdout, vec!["hi".to_string()]);
}

#[test]
fn assert_helpers_drive_phc_test_pass_fail() {
    let src = r#"pack demo;
test "math works" {
    assert::eq(2 + 3, 5);
    assert::isTrue(true);
}
test "math doesn't" {
    assert::eq(2 + 2, 5);
}
"#;
    let report = run_source(src);
    assert_eq!(report.passed, 1);
    assert_eq!(report.failed, 1);
    let passing = &report.results[0];
    assert_eq!(passing.name, "math works");
    assert!(passing.passed);
    let failing = &report.results[1];
    assert_eq!(failing.name, "math doesn't");
    assert!(!failing.passed);
    assert!(failing.errors[0].message.contains("assert::eq"));
}
