// SPDX-License-Identifier: MIT
//! End-to-end snapshot of the lexer against every PHC example.
//!
//! Iterates `examples/*.phc` and snapshots the full token stream for
//! each. The snapshots double as a regression test (any change to the
//! lexer that shifts tokens or spans surfaces in the diff) and as a
//! readable record of how the lexer handles the spec's canonical
//! source samples.

use phc_lexer::{Lexer, Spanned, StringPart, Token};
use phc_span::FileId;

fn render_tokens(source: &str) -> String {
    let mut out = String::new();
    for item in Lexer::new(source, FileId(0)) {
        match item {
            Ok(Spanned { token, span }) => {
                out.push_str(&format!(
                    "{:>4}..{:<4}  {}\n",
                    span.lo,
                    span.hi,
                    render_token(&token)
                ));
            }
            Err(span) => {
                out.push_str(&format!("{:>4}..{:<4}  <LEX ERROR>\n", span.lo, span.hi));
            }
        }
    }
    out
}

fn render_token(token: &Token) -> String {
    match token {
        Token::Ident(name) => format!("Ident({name:?})"),
        Token::IntLit(text) => format!("IntLit({text:?})"),
        Token::FloatLit(text) => format!("FloatLit({text:?})"),
        Token::StrLit(parts) => {
            let pieces: Vec<String> = parts
                .iter()
                .map(|p| match p {
                    StringPart::Text(t) => format!("Text({t:?})"),
                    StringPart::Interp(e) => format!("Interp({e:?})"),
                })
                .collect();
            format!("StrLit[{}]", pieces.join(", "))
        }
        other => format!("{other:?}"),
    }
}

#[test]
fn lex_every_example() {
    insta::glob!("../../..", "examples/*.phc", |path| {
        let src = std::fs::read_to_string(path).expect("read example");
        let rendered = render_tokens(&src);
        insta::assert_snapshot!(rendered);
    });
}
