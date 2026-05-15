// SPDX-License-Identifier: MIT
//! End-to-end snapshot of the typechecker against every PHC example.
//!
//! Lex + parse + resolve + typecheck every `examples/*.phc` and
//! snapshot the function-signature table, the per-expression type
//! map (sorted by span), and any diagnostics. Mirrors the lexer /
//! parser / semantic harnesses so a single `cargo insta review`
//! walks all four layers.

use phc_parser::parse;
use phc_semantic::resolve;
use phc_span::FileId;
use phc_typecheck::{typecheck, Typed};

fn render(typed: &Typed) -> String {
    let mut out = String::new();

    out.push_str("=== function sigs ===\n");
    let mut sigs: Vec<_> = typed.function_sigs.iter().collect();
    sigs.sort_by_key(|(id, _)| id.0);
    for (id, sig) in sigs {
        out.push_str(&format!("{:?}", id));
        if !sig.generic_params.is_empty() {
            out.push('<');
            out.push_str(&sig.generic_params.join(", "));
            out.push('>');
        }
        out.push('(');
        let params: Vec<String> = sig
            .params
            .iter()
            .map(|p| {
                let prefix = match p.borrow {
                    phc_ast::Borrow::None => "",
                    phc_ast::Borrow::Shared => "&",
                    phc_ast::Borrow::Mutable => "&flip ",
                };
                format!("{}{} ${}", prefix, p.ty.display(), p.name)
            })
            .collect();
        out.push_str(&params.join(", "));
        out.push_str(") -> ");
        out.push_str(&sig.return_ty.display());
        out.push('\n');
    }

    out.push_str("\n=== expr types ===\n");
    let mut entries: Vec<_> = typed.expr_types.iter().collect();
    entries.sort_by_key(|(span, _)| (span.lo, span.hi));
    for (span, ty) in entries {
        out.push_str(&format!(
            "{:>4}..{:<4} : {}\n",
            span.lo,
            span.hi,
            ty.display()
        ));
    }

    if !typed.diagnostics.is_empty() {
        out.push_str("\n=== diagnostics ===\n");
        for d in &typed.diagnostics {
            out.push_str(&format!(
                "{}..{}  {:?}: {}\n",
                d.span.lo, d.span.hi, d.severity, d.message
            ));
        }
    }

    out
}

#[test]
fn typecheck_every_example() {
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
        let typed = typecheck(&file, &resolved);
        let rendered = render(&typed);
        insta::assert_snapshot!(rendered);
    });
}
