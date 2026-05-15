// SPDX-License-Identifier: MIT
//! Inline tests for the C emitter — string match against the
//! emitted source. End-to-end "compile + run + assert stdout" lives
//! in `phc-build`'s integration tests.

use crate::emit_c;
use phc_parser::parse;
use phc_span::FileId;

fn emit_for(src: &str) -> String {
    let parsed = parse(src, FileId(0));
    assert!(
        parsed.diagnostics.is_empty(),
        "parser diagnostics: {:?}",
        parsed.diagnostics
    );
    let file = parsed.file.expect("expected SourceFile");
    let resolved = phc_semantic::resolve(&file);
    let typed = phc_typecheck::typecheck(&file, &resolved);
    let out = emit_c(&file, &resolved, &typed);
    out.c_source
}

#[test]
fn empty_main_emits_void_function_and_entry() {
    let src = "pack a;\nfunction main(): void {}";
    let c = emit_for(src);
    assert!(c.contains("static void phc_main(void)"));
    assert!(c.contains("int main(int argc, char** argv)"));
    assert!(c.contains("phc_main();"));
}

#[test]
fn string_local_emits_runtime_call() {
    let src = r#"pack a;
        function main(): void {
            string $name = "PHC";
        }"#;
    let c = emit_for(src);
    assert!(c.contains("phc_string phc_var_name = "));
    assert!(c.contains("phc_string_lit(\"PHC\")"));
}

#[test]
fn logger_info_emits_phc_print() {
    let src = r#"pack a;
        function main(): void {
            Logger::info("hello");
        }"#;
    let c = emit_for(src);
    assert!(c.contains("phc_print("));
    assert!(c.contains("phc_string_lit(\"hello\")"));
}

#[test]
fn string_interpolation_emits_concat_chain() {
    let src = r#"pack a;
        function main(): void {
            string $name = "PHC";
            Logger::info("Hello, {$name}!");
        }"#;
    let c = emit_for(src);
    // Three pieces concat-chained: "Hello, " + name + "!".
    assert!(c.contains("phc_concat2"));
    assert!(c.contains("phc_to_string(phc_var_name)"));
    assert!(c.contains("phc_string_lit(\"Hello, \")"));
    assert!(c.contains("phc_string_lit(\"!\")"));
}

#[test]
fn integer_literal_uses_int64() {
    let src = "pack a;\nfunction main(): void { int $x = 42; }";
    let c = emit_for(src);
    assert!(c.contains("int64_t phc_var_x = (int64_t)42"));
}

#[test]
fn user_function_call_dispatches_by_name() {
    let src = r#"pack a;
        function helper(): void {}
        function main(): void { helper(); }"#;
    let c = emit_for(src);
    assert!(c.contains("static void phc_helper(void)"));
    assert!(c.contains("phc_helper()"));
}
