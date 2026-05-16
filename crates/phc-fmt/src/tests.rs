// SPDX-License-Identifier: MIT
//! Tests for the D-035 token-stream formatter.

use crate::format;

fn fmt(src: &str) -> String {
    format(src).expect("clean source")
}

#[test]
fn hello_world_round_trip_canonicalises() {
    let src = "pack hello;function main():void{Logger::info(\"Hello, PHC!\");}";
    let out = fmt(src);
    assert_eq!(
        out,
        "pack hello;\n\
         function main(): void {\n\
         \x20\x20\x20\x20Logger::info(\"Hello, PHC!\");\n\
         }\n"
    );
}

#[test]
fn fmt_is_idempotent_on_clean_source() {
    let src = "pack demo;\n\
         function main(): void {\n\
         \x20\x20\x20\x20int $x = 1 + 2;\n\
         \x20\x20\x20\x20Logger::info(\"hi\");\n\
         }\n";
    let once = fmt(src);
    let twice = fmt(&once);
    assert_eq!(once, twice, "fmt(fmt(x)) must equal fmt(x)");
}

#[test]
fn line_comments_round_trip() {
    let src = "pack demo;\n\
        // top-level note\n\
        function main(): void {\n\
        \x20\x20\x20\x20// inside\n\
        \x20\x20\x20\x20int $x = 1;\n\
        }\n";
    let out = fmt(src);
    assert!(
        out.contains("// top-level note"),
        "missing top comment in {out}"
    );
    assert!(out.contains("// inside"), "missing inner comment in {out}");
}

#[test]
fn block_comments_round_trip() {
    let src = "pack demo;\n\
        /* block\n   spans lines */\n\
        function main(): void {}\n";
    let out = fmt(src);
    assert!(out.contains("/* block"), "missing block comment: {out}");
    assert!(out.contains("spans lines */"), "missing block tail: {out}");
}

#[test]
fn fmt_collapses_redundant_whitespace_inside_calls() {
    let src = "pack d;function f():void{Logger::info ( \"hi\" ) ;}";
    let out = fmt(src);
    assert!(out.contains("Logger::info(\"hi\");"), "unexpected: {out:?}");
}

#[test]
fn fmt_preserves_blank_lines_between_top_level_items() {
    let src = "pack d;\n\
        function a(): void {}\n\
        \n\
        function b(): void {}\n";
    let out = fmt(src);
    // Two consecutive newlines separate the two function decls.
    assert!(
        out.contains("}\n\nfunction b"),
        "expected blank line between top-level items, got:\n{out}"
    );
}
