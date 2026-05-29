// SPDX-License-Identifier: MIT
//! Opinionated PHC source formatter (D-035 v0a).
//!
//! Token-stream pretty-printer: lex the source, then walk the
//! tokens emitting canonical whitespace + indentation. The full
//! grammar walk that an AST-aware formatter would do is deferred;
//! the token-stream approach preserves comments naturally (the
//! lexer keeps `LineComment` / `BlockComment` tokens in the
//! stream for exactly this reason) and ships in one slice.
//!
//! Canonical style (v0a):
//! - 4-space indentation per `{` level.
//! - LF line endings.
//! - Newline after `;`, `{`, and (where syntactically appropriate)
//!   after `}`.
//! - Single space between adjacent tokens by default, with the
//!   "no space" overrides listed in [`needs_space`].
//! - Comments round-trip verbatim. Line comments terminate the
//!   current line; block comments emit inline with surrounding
//!   spaces unless they contain a newline, in which case they
//!   land on their own lines.
//!
//! Known imperfections for v0a (tracked as follow-ups):
//! - `<` / `>` are treated as plain operators. Type-args contexts
//!   format readably when there was no user-added whitespace; the
//!   formatter never inserts a space across `<` / `>`, but it also
//!   won't tighten `list < int >` further.
//! - Long lines are not re-flowed.
//! - Trailing-comma policies, alignment, and other AST-aware
//!   niceties wait for an AST-aware second pass.

#[cfg(test)]
mod tests;

use phc_errors::Diagnostic;
use phc_lexer::{Lexer, Spanned, Token};
use phc_span::{FileId, Span};

const INDENT: &str = "    ";

/// Format `source` and return the formatted string. Lex errors
/// surface as `Err(diagnostics)`; in that case the source was
/// malformed and the formatter does not attempt a partial output.
pub fn format(source: &str) -> Result<String, Vec<Diagnostic>> {
    let mut tokens: Vec<Spanned> = Vec::new();
    let mut diags: Vec<Diagnostic> = Vec::new();
    for item in Lexer::new(source, FileId(0)) {
        match item {
            Ok(s) => tokens.push(s),
            Err(span) => diags.push(Diagnostic {
                severity: phc_errors::Severity::Error,
                message: "lex error".to_string(),
                span,
            }),
        }
    }
    if !diags.is_empty() {
        return Err(diags);
    }
    Ok(emit(source, &tokens))
}

fn emit(source: &str, tokens: &[Spanned]) -> String {
    let mut out = String::new();
    let mut depth: usize = 0;
    let mut at_line_start = true;
    let mut prev: Option<&Token> = None;
    let mut prev_span: Option<Span> = None;
    for s in tokens {
        // Preserve at most one blank line between top-level items
        // when the source had at least one. Counted by the number
        // of newlines between prev's `hi` and this token's `lo` in
        // the original source slice.
        let want_blank_line = if let Some(prev_sp) = prev_span {
            let between = &source[prev_sp.hi as usize..s.span.lo as usize];
            between.matches('\n').count() >= 2 && depth == 0
        } else {
            false
        };

        // Decrease depth before emitting `}` so the closing brace
        // sits at the outer level.
        if matches!(&s.token, Token::RBrace) && depth > 0 {
            depth -= 1;
        }

        let mut newline_before = need_newline_before(prev, &s.token);

        // Line comments always start on their own line when there
        // was a newline before them in source; otherwise they
        // trail the current line.
        if let Token::LineComment(_) = &s.token {
            if let Some(prev_sp) = prev_span {
                let between = &source[prev_sp.hi as usize..s.span.lo as usize];
                if between.contains('\n') {
                    newline_before = true;
                }
            }
        }

        if newline_before && !at_line_start {
            out.push('\n');
            at_line_start = true;
        }
        if want_blank_line && out.ends_with('\n') && !out.ends_with("\n\n") {
            out.push('\n');
        }
        if at_line_start {
            for _ in 0..depth {
                out.push_str(INDENT);
            }
        } else if let Some(p) = prev {
            if needs_space(p, &s.token) {
                out.push(' ');
            }
        }
        out.push_str(token_text(source, s));
        at_line_start = false;

        // Block comments containing a newline: end the line so the
        // next token starts fresh.
        if let Token::BlockComment(text) = &s.token {
            if text.contains('\n') {
                out.push('\n');
                at_line_start = true;
            }
        }

        // After `;` / `{` / line-comment: unconditionally break
        // before the next token.
        if matches!(
            &s.token,
            Token::Semicolon | Token::LBrace | Token::LineComment(_)
        ) {
            out.push('\n');
            at_line_start = true;
        }

        if matches!(&s.token, Token::LBrace) {
            depth += 1;
        }
        prev = Some(&s.token);
        prev_span = Some(s.span);
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Return the source slice for a token. Block / line comment
/// tokens already carry their full text (matcher payload), but the
/// source slice is identical and easier to share with every other
/// token form, so we go through source uniformly.
fn token_text<'s>(source: &'s str, s: &Spanned) -> &'s str {
    &source[s.span.lo as usize..s.span.hi as usize]
}

/// Newline-before decisions that depend on the (prev, current)
/// pair. The unconditional newline-after rules for `;` / `{` /
/// line-comment are handled in the main loop after each token
/// is emitted.
fn need_newline_before(prev: Option<&Token>, current: &Token) -> bool {
    let Some(p) = prev else {
        return false;
    };
    // After `}`: usually break, but keep `} else`, `},`, `};`,
    // `})`, `}]`, `}.foo`, `}->bar`, `}::baz` on the same line.
    if matches!(p, Token::RBrace)
        && !matches!(
            current,
            Token::Else
                | Token::Comma
                | Token::Semicolon
                | Token::RParen
                | Token::RBracket
                | Token::RBrace
                | Token::Dot
                | Token::Arrow
                | Token::StaticOp
        )
    {
        return true;
    }
    false
}

/// Spacing rule for two adjacent tokens on the same line. Returns
/// `true` when a single space should separate them.
fn needs_space(prev: &Token, current: &Token) -> bool {
    use Token::*;
    // No space around member access operators.
    if matches!(prev, Dot | Arrow | StaticOp) || matches!(current, Dot | Arrow | StaticOp) {
        return false;
    }
    // No space inside `()` / `[]` openers and closers.
    if matches!(prev, LParen | LBracket) || matches!(current, RParen | RBracket) {
        return false;
    }
    // No leading space before separator / postfix punctuation.
    // `:` covered here too — `function f(...): T` and `<T: Bound>`
    // both read tighter without a leading space.
    if matches!(current, Comma | Semicolon | Question | Colon) {
        return false;
    }
    // Call / index attaches directly to the preceding callee.
    if matches!(current, LParen | LBracket) && matches!(prev, Ident(_) | RParen | RBracket) {
        return false;
    }
    // `&` hugs the operand identifier in borrow expressions.
    // `$` hugs the identifier in `$name` references (D-023 sigil).
    // `@` hugs the attribute name in `@noinline` (D-052).
    if matches!(prev, Amp | Dollar | At) {
        return false;
    }
    // After `<` / before `>`: no space inserted. Comparisons stay
    // readable when the user typed `a < b` (we never insert
    // adjacency); type-args contexts stay tight (`list<int>`).
    if matches!(prev, Lt) || matches!(current, Gt) {
        return false;
    }
    // Type-args opener tightening: `list<int>` should not gain a
    // space between the type ident and `<`. We can't parse here,
    // so use a name heuristic — known stdlib type names + any
    // PascalCase-looking identifier are treated as type heads.
    // Variables are `$name`-prefixed so `Dollar Ident Lt` keeps
    // the space (the prev token there is Ident("x") from a
    // lowercase-leading name, which fails the PascalCase check).
    if matches!(current, Lt) {
        if let Ident(name) = prev {
            if is_type_like(name) {
                return false;
            }
        }
    }
    true
}

/// Heuristic for "this identifier names a type, not a value". Used
/// only by the formatter to decide whether to tighten `Ident <`
/// (type-arg opener) vs leave it spaced (comparison). The
/// PascalCase check catches user-defined classes; the explicit set
/// catches lowercase stdlib types from D-022.
fn is_type_like(name: &str) -> bool {
    matches!(
        name,
        "list" | "map" | "set" | "option" | "result" | "fn" | "array" | "dyn"
    ) || name
        .chars()
        .next()
        .map(|c| c.is_ascii_uppercase())
        .unwrap_or(false)
}
