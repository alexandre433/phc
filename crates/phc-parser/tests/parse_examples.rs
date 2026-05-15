// SPDX-License-Identifier: MIT
//! End-to-end snapshot of the parser against every PHC example.
//!
//! Iterates `examples/*.phc`, parses each one with the full
//! lexer + parser pipeline, and snapshots the AST debug-format
//! plus any diagnostics. Mirrors the lexer's snapshot harness so
//! lexer + parser regressions surface in the same `cargo insta
//! review` flow.

use phc_parser::{parse, ParseResult};
use phc_span::FileId;

fn render(result: ParseResult) -> String {
    let mut out = String::new();
    if !result.diagnostics.is_empty() {
        out.push_str("=== diagnostics ===\n");
        for d in &result.diagnostics {
            out.push_str(&format!(
                "{}..{}  {:?}: {}\n",
                d.span.lo, d.span.hi, d.severity, d.message
            ));
        }
        out.push('\n');
    }
    match result.file {
        Some(file) => {
            out.push_str("=== ast ===\n");
            out.push_str(&format!("{file:#?}\n"));
        }
        None => out.push_str("=== ast ===\n<no SourceFile produced>\n"),
    }
    out
}

#[test]
fn parse_every_example() {
    insta::glob!("../../..", "examples/*.phc", |path| {
        let src = std::fs::read_to_string(path).expect("read example");
        let result = parse(&src, FileId(0));
        let rendered = render(result);
        insta::assert_snapshot!(rendered);
    });
}
