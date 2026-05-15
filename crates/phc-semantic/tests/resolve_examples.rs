// SPDX-License-Identifier: MIT
//! End-to-end snapshot of the resolver against every PHC example.
//!
//! Lex + parse + resolve every `examples/*.phc` and snapshot the
//! top-level symbol table, every resolved use-site, and any
//! diagnostics. Mirrors the lexer / parser harnesses so a single
//! `cargo insta review` walks all three layers.

use phc_parser::parse;
use phc_semantic::{resolve, Resolved};
use phc_span::FileId;

fn render(r: &Resolved) -> String {
    let mut out = String::new();

    out.push_str("=== top level ===\n");
    let mut top: Vec<_> = r.top_level.iter().collect();
    top.sort_by(|a, b| a.0.cmp(b.0));
    for (name, id) in top {
        let sym = r.symbol(*id);
        out.push_str(&format!(
            "{:?}  {:?}  {}  ({}..{})\n",
            id, sym.kind, name, sym.def_span.lo, sym.def_span.hi
        ));
    }

    out.push_str("\n=== uses ===\n");
    let mut uses: Vec<_> = r.uses.iter().collect();
    uses.sort_by_key(|(span, _)| (span.lo, span.hi));
    for (span, id) in uses {
        let sym = r.symbol(*id);
        out.push_str(&format!(
            "{:>4}..{:<4} -> {:?}  {:?}  {}\n",
            span.lo, span.hi, id, sym.kind, sym.name
        ));
    }

    if !r.diagnostics.is_empty() {
        out.push_str("\n=== diagnostics ===\n");
        for d in &r.diagnostics {
            out.push_str(&format!(
                "{}..{}  {:?}: {}\n",
                d.span.lo, d.span.hi, d.severity, d.message
            ));
        }
    }

    out
}

#[test]
fn resolve_every_example() {
    insta::glob!("../../..", "examples/*.phc", |path| {
        let src = std::fs::read_to_string(path).expect("read example");
        let parsed = parse(&src, FileId(0));
        assert!(
            parsed.diagnostics.is_empty(),
            "parser diagnostics in {}: {:?}",
            path.display(),
            parsed.diagnostics
        );
        let file = parsed.file.expect("expected SourceFile");
        let resolved = resolve(&file);
        let rendered = render(&resolved);
        insta::assert_snapshot!(rendered);
    });
}
