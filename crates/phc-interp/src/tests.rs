// SPDX-License-Identifier: MIT
//! Inline tests for the tree-walking interpreter.

use crate::{run, RunOutput, Value};
use phc_parser::parse;
use phc_span::FileId;

fn run_src(src: &str) -> RunOutput {
    let parsed = parse(src, FileId(0));
    assert!(
        parsed.diagnostics.is_empty(),
        "parser diagnostics: {:?}",
        parsed.diagnostics
    );
    let file = parsed.file.expect("expected SourceFile");
    let resolved = phc_semantic::resolve(&file);
    let typed = phc_typecheck::typecheck(&file, &resolved);
    run(&file, &resolved, &typed)
}

#[test]
fn missing_main_is_a_runtime_error() {
    let out = run_src("pack a;");
    assert!(
        out.errors.iter().any(|e| e.message.contains("`main`")),
        "expected missing-main error, got {:?}",
        out.errors
    );
}

#[test]
fn empty_main_returns_void() {
    let out = run_src("pack a;\nfunction main(): void {}");
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert!(matches!(out.result, Some(Value::Void)));
    assert!(out.stdout.is_empty());
}

#[test]
fn main_can_print_a_plain_string() {
    let out = run_src(
        r#"pack a;
           function main(): void {
               Logger::info("hello");
           }"#,
    );
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["hello"]);
}

#[test]
fn string_interpolation_renders_local_variable() {
    let out = run_src(
        r#"pack a;
           function main(): void {
               string $name = "PHC";
               Logger::info("Hello, {$name}!");
           }"#,
    );
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["Hello, PHC!"]);
}

#[test]
fn multiple_logger_calls_each_emit_a_line() {
    let out = run_src(
        r#"pack a;
           function main(): void {
               Logger::info("one");
               Logger::info("two");
               Logger::info("three");
           }"#,
    );
    assert_eq!(out.stdout, vec!["one", "two", "three"]);
}

#[test]
fn return_int_propagates_to_run_output() {
    let out = run_src(
        r#"pack a;
           function main(): int { return 42; }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(42))));
}

#[test]
fn hello_phc_example_runs_end_to_end() {
    let src = std::fs::read_to_string("../../examples/hello.phc").expect("read hello.phc");
    let out = run_src(&src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["Hello, PHC!"]);
}
