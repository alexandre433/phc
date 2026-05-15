// SPDX-License-Identifier: MIT
// Wired into statements (P5) and class-member parsing (P6); the
// inline tests below already exercise every production. Drop this
// allow once parse_expr has a non-test caller.
#![allow(dead_code)]

//! Expression productions.
//!
//! Mirrors the precedence table in `spec/grammar.ebnf` /
//! `spec/operators.md`. Top-of-table (tightest) is `PostfixExpr`;
//! bottom (loosest) is `NullCoalesce`. Each precedence level is its
//! own function so the parser reads top-down and the call graph
//! mirrors the EBNF.
//!
//! Match expressions and lambdas are deferred to P7; for now
//! `parse_primary` reports an unsupported diagnostic when it sees
//! `match` or an opening lambda.

use phc_ast::{BinOp, Borrow, Expr, Ident, StrPart, UnaryOp};
use phc_lexer::{Lexer, StringPart, Token};
use phc_span::Span;

use crate::functions::parse_borrow_mod;
use crate::types::parse_type;
use crate::Cursor;

/// Top-level entry — equivalent to the `Expression` production.
pub(crate) fn parse_expr(cursor: &mut Cursor<'_>) -> Option<Expr> {
    parse_null_coalesce(cursor)
}

fn parse_null_coalesce(cursor: &mut Cursor<'_>) -> Option<Expr> {
    let mut lhs = parse_or(cursor)?;
    while cursor.eat(&Token::QQuestion).is_some() {
        let rhs = parse_or(cursor)?;
        lhs = combine(lhs, rhs, BinOp::NullCoalesce, cursor);
    }
    Some(lhs)
}

fn parse_or(cursor: &mut Cursor<'_>) -> Option<Expr> {
    let mut lhs = parse_and(cursor)?;
    while cursor.eat(&Token::PipePipe).is_some() {
        let rhs = parse_and(cursor)?;
        lhs = combine(lhs, rhs, BinOp::Or, cursor);
    }
    Some(lhs)
}

fn parse_and(cursor: &mut Cursor<'_>) -> Option<Expr> {
    let mut lhs = parse_eq(cursor)?;
    while cursor.eat(&Token::AmpAmp).is_some() {
        let rhs = parse_eq(cursor)?;
        lhs = combine(lhs, rhs, BinOp::And, cursor);
    }
    Some(lhs)
}

fn parse_eq(cursor: &mut Cursor<'_>) -> Option<Expr> {
    let lhs = parse_cmp(cursor)?;
    let op = match cursor.peek_token() {
        Some(Token::EqEq) => BinOp::Eq,
        Some(Token::NotEq) => BinOp::Neq,
        _ => return Some(lhs),
    };
    cursor.advance();
    // `==` / `!=` are non-chainable per spec/operators.md.
    let rhs = parse_cmp(cursor)?;
    Some(combine(lhs, rhs, op, cursor))
}

fn parse_cmp(cursor: &mut Cursor<'_>) -> Option<Expr> {
    let lhs = parse_add(cursor)?;
    let op = match cursor.peek_token() {
        Some(Token::Lt) => BinOp::Lt,
        Some(Token::LtEq) => BinOp::Le,
        Some(Token::Gt) => BinOp::Gt,
        Some(Token::GtEq) => BinOp::Ge,
        _ => return Some(lhs),
    };
    cursor.advance();
    // Comparison operators are non-chainable per spec/operators.md.
    let rhs = parse_add(cursor)?;
    Some(combine(lhs, rhs, op, cursor))
}

fn parse_add(cursor: &mut Cursor<'_>) -> Option<Expr> {
    let mut lhs = parse_mul(cursor)?;
    loop {
        let op = match cursor.peek_token() {
            Some(Token::Plus) => BinOp::Add,
            Some(Token::Minus) => BinOp::Sub,
            _ => break,
        };
        cursor.advance();
        let rhs = parse_mul(cursor)?;
        lhs = combine(lhs, rhs, op, cursor);
    }
    Some(lhs)
}

fn parse_mul(cursor: &mut Cursor<'_>) -> Option<Expr> {
    let mut lhs = parse_cast(cursor)?;
    loop {
        let op = match cursor.peek_token() {
            Some(Token::Star) => BinOp::Mul,
            Some(Token::Slash) => BinOp::Div,
            Some(Token::Percent) => BinOp::Rem,
            _ => break,
        };
        cursor.advance();
        let rhs = parse_cast(cursor)?;
        lhs = combine(lhs, rhs, op, cursor);
    }
    Some(lhs)
}

fn parse_cast(cursor: &mut Cursor<'_>) -> Option<Expr> {
    let mut value = parse_unary(cursor)?;
    while cursor.eat(&Token::As).is_some() {
        let ty = parse_type(cursor)?;
        let span = Span::new(cursor.file(), expr_span(&value).lo, ty.span.hi);
        value = Expr::Cast {
            value: Box::new(value),
            ty,
            span,
        };
    }
    Some(value)
}

fn parse_unary(cursor: &mut Cursor<'_>) -> Option<Expr> {
    let start = cursor.current_span();
    if cursor.eat(&Token::Bang).is_some() {
        let operand = parse_unary(cursor)?;
        let hi = expr_span(&operand).hi;
        return Some(Expr::Unary {
            op: UnaryOp::Not,
            operand: Box::new(operand),
            span: Span::new(cursor.file(), start.lo, hi),
        });
    }
    if cursor.eat(&Token::Minus).is_some() {
        let operand = parse_unary(cursor)?;
        let hi = expr_span(&operand).hi;
        return Some(Expr::Unary {
            op: UnaryOp::Neg,
            operand: Box::new(operand),
            span: Span::new(cursor.file(), start.lo, hi),
        });
    }
    if cursor.eat(&Token::Await).is_some() {
        let operand = parse_unary(cursor)?;
        let hi = expr_span(&operand).hi;
        return Some(Expr::Unary {
            op: UnaryOp::Await,
            operand: Box::new(operand),
            span: Span::new(cursor.file(), start.lo, hi),
        });
    }
    if matches!(cursor.peek_token(), Some(Token::Amp)) {
        let kind = parse_borrow_mod(cursor);
        let operand = parse_unary(cursor)?;
        let hi = expr_span(&operand).hi;
        return Some(Expr::Borrow {
            kind,
            operand: Box::new(operand),
            span: Span::new(cursor.file(), start.lo, hi),
        });
    }
    parse_postfix(cursor)
}

fn parse_postfix(cursor: &mut Cursor<'_>) -> Option<Expr> {
    let mut expr = parse_primary(cursor)?;
    loop {
        match cursor.peek_token() {
            Some(Token::Arrow) => {
                cursor.advance();
                let field = expect_ident(cursor, "field or method name after `->`")?;
                let span = Span::new(cursor.file(), expr_span(&expr).lo, field.span.hi);
                expr = Expr::Member {
                    receiver: Box::new(expr),
                    field,
                    span,
                };
            }
            Some(Token::StaticOp) => {
                cursor.advance();
                let member = expect_ident(cursor, "name after `::`")?;
                let span = Span::new(cursor.file(), expr_span(&expr).lo, member.span.hi);
                expr = Expr::Static {
                    ty: Box::new(expr),
                    member,
                    span,
                };
            }
            Some(Token::LParen) => {
                cursor.advance();
                let args = parse_arg_list(cursor)?;
                let close = cursor.expect(&Token::RParen, "`)`").ok()?;
                let span = Span::new(cursor.file(), expr_span(&expr).lo, close.hi);
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    span,
                };
            }
            Some(Token::LBracket) => {
                cursor.advance();
                let index = parse_expr(cursor)?;
                let close = cursor.expect(&Token::RBracket, "`]`").ok()?;
                let span = Span::new(cursor.file(), expr_span(&expr).lo, close.hi);
                expr = Expr::Index {
                    target: Box::new(expr),
                    index: Box::new(index),
                    span,
                };
            }
            _ => return Some(expr),
        }
    }
}

fn parse_arg_list(cursor: &mut Cursor<'_>) -> Option<Vec<Expr>> {
    if matches!(cursor.peek_token(), Some(Token::RParen)) {
        return Some(Vec::new());
    }
    let first = parse_expr(cursor)?;
    let mut args = vec![first];
    while matches!(cursor.peek_token(), Some(Token::Comma)) {
        cursor.advance();
        if matches!(cursor.peek_token(), Some(Token::RParen)) {
            break; // trailing comma
        }
        args.push(parse_expr(cursor)?);
    }
    Some(args)
}

fn parse_primary(cursor: &mut Cursor<'_>) -> Option<Expr> {
    let spanned = cursor.peek()?.clone();
    match spanned.token {
        Token::IntLit(text) => {
            cursor.advance();
            Some(Expr::IntLit {
                text,
                span: spanned.span,
            })
        }
        Token::FloatLit(text) => {
            cursor.advance();
            Some(Expr::FloatLit {
                text,
                span: spanned.span,
            })
        }
        Token::True => {
            cursor.advance();
            Some(Expr::BoolLit {
                value: true,
                span: spanned.span,
            })
        }
        Token::False => {
            cursor.advance();
            Some(Expr::BoolLit {
                value: false,
                span: spanned.span,
            })
        }
        Token::Null => {
            cursor.advance();
            Some(Expr::NullLit { span: spanned.span })
        }
        Token::StrLit(parts) => {
            cursor.advance();
            Some(parse_string_literal(parts, spanned.span, cursor))
        }
        Token::Dollar => parse_var_or_this(cursor),
        Token::LParen => {
            cursor.advance();
            let inner = parse_expr(cursor)?;
            let close = cursor.expect(&Token::RParen, "`)`").ok()?;
            Some(Expr::Paren {
                inner: Box::new(inner),
                span: Span::new(cursor.file(), spanned.span.lo, close.hi),
            })
        }
        Token::Ident(name) => {
            cursor.advance();
            Some(Expr::TypeName {
                name: Ident {
                    name,
                    span: spanned.span,
                },
                span: spanned.span,
            })
        }
        Token::Match => {
            let span = spanned.span;
            cursor.error(span, "`match` expressions are not yet parsed (P7)");
            None
        }
        _ => {
            let span = spanned.span;
            cursor.error(span, "expected an expression");
            None
        }
    }
}

fn parse_var_or_this(cursor: &mut Cursor<'_>) -> Option<Expr> {
    let dollar = cursor.expect(&Token::Dollar, "`$` sigil").ok()?;
    let next = cursor.peek()?.clone();
    let Token::Ident(name) = next.token else {
        cursor.error(next.span, "expected variable name after `$`");
        return None;
    };
    cursor.advance();
    let span = Span::new(cursor.file(), dollar.lo, next.span.hi);
    if name == "this" {
        Some(Expr::This { span })
    } else {
        Some(Expr::Var {
            name: Ident {
                name,
                span: next.span,
            },
            span,
        })
    }
}

fn parse_string_literal(parts: Vec<StringPart>, span: Span, cursor: &mut Cursor<'_>) -> Expr {
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        match part {
            StringPart::Text(text) => out.push(StrPart::Text(text)),
            StringPart::Interp(body) => match parse_interp_body(&body, span, cursor) {
                Some(expr) => out.push(StrPart::Expr(expr)),
                None => {
                    // Diagnostic already emitted; keep going so the
                    // outer literal still lands in the AST.
                    out.push(StrPart::Text(String::new()));
                }
            },
        }
    }
    Expr::StrLit { parts: out, span }
}

fn parse_interp_body(body: &str, host_span: Span, cursor: &mut Cursor<'_>) -> Option<Expr> {
    let tokens: Vec<phc_lexer::Spanned> = Lexer::new(body, cursor.file())
        .filter_map(|r| match r {
            Ok(t) => Some(t),
            Err(_) => {
                cursor.error(host_span, "lex error inside string interpolation");
                None
            }
        })
        .collect();
    let mut sub = Cursor::new(&tokens, cursor.file(), body.len() as u32);
    let expr = parse_expr(&mut sub);
    for diag in sub.into_diagnostics() {
        cursor.push_diagnostic(diag);
    }
    expr
}

fn combine(lhs: Expr, rhs: Expr, op: BinOp, cursor: &Cursor<'_>) -> Expr {
    let lo = expr_span(&lhs).lo;
    let hi = expr_span(&rhs).hi;
    Expr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        span: Span::new(cursor.file(), lo, hi),
    }
}

fn expr_span(expr: &Expr) -> Span {
    match expr {
        Expr::IntLit { span, .. }
        | Expr::FloatLit { span, .. }
        | Expr::BoolLit { span, .. }
        | Expr::NullLit { span }
        | Expr::StrLit { span, .. }
        | Expr::This { span }
        | Expr::Var { span, .. }
        | Expr::TypeName { span, .. }
        | Expr::Paren { span, .. }
        | Expr::Member { span, .. }
        | Expr::Static { span, .. }
        | Expr::Call { span, .. }
        | Expr::Index { span, .. }
        | Expr::Unary { span, .. }
        | Expr::Borrow { span, .. }
        | Expr::Cast { span, .. }
        | Expr::Binary { span, .. } => *span,
    }
}

fn expect_ident(cursor: &mut Cursor<'_>, label: &str) -> Option<Ident> {
    let spanned = cursor.peek()?;
    if let Token::Ident(name) = &spanned.token {
        let ident = Ident {
            name: name.clone(),
            span: spanned.span,
        };
        cursor.advance();
        Some(ident)
    } else {
        let span = spanned.span;
        cursor.error(span, format!("expected {label}"));
        None
    }
}

// `Borrow` is only used through `parse_borrow_mod`; the re-export
// keeps the import surface tight. (Silences `unused_imports` if the
// compiler ever complains.)
#[allow(dead_code)]
fn _unused_borrow_marker() -> Borrow {
    Borrow::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use phc_lexer::{Lexer, Spanned};
    use phc_span::FileId;

    fn tokens_of(src: &str) -> Vec<Spanned> {
        Lexer::new(src, FileId(0))
            .map(|r| r.expect("clean lex"))
            .collect()
    }

    fn parse_ok(src: &str) -> Expr {
        let toks = tokens_of(src);
        let mut cursor = Cursor::new(&toks, FileId(0), src.len() as u32);
        let expr = parse_expr(&mut cursor).expect("expected an expression");
        let diags = cursor.into_diagnostics();
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        expr
    }

    #[test]
    fn integer_literal() {
        match parse_ok("42") {
            Expr::IntLit { text, .. } => assert_eq!(text, "42"),
            other => panic!("expected IntLit, got {other:?}"),
        }
    }

    #[test]
    fn bool_and_null_literals() {
        assert!(matches!(
            parse_ok("true"),
            Expr::BoolLit { value: true, .. }
        ));
        assert!(matches!(parse_ok("null"), Expr::NullLit { .. }));
    }

    #[test]
    fn variable_ref_strips_dollar() {
        match parse_ok("$count") {
            Expr::Var { name, .. } => assert_eq!(name.name, "count"),
            other => panic!("expected Var, got {other:?}"),
        }
    }

    #[test]
    fn this_keyword_is_distinct_from_other_vars() {
        assert!(matches!(parse_ok("$this"), Expr::This { .. }));
    }

    #[test]
    fn type_name_in_expression_position() {
        match parse_ok("Status") {
            Expr::TypeName { name, .. } => assert_eq!(name.name, "Status"),
            other => panic!("expected TypeName, got {other:?}"),
        }
    }

    #[test]
    fn arrow_member_access_chains_left_to_right() {
        // ($user->profile)->name
        match parse_ok("$user->profile->name") {
            Expr::Member {
                receiver, field, ..
            } => {
                assert_eq!(field.name, "name");
                assert!(matches!(*receiver, Expr::Member { .. }));
            }
            other => panic!("expected Member, got {other:?}"),
        }
    }

    #[test]
    fn static_then_call() {
        // (Status::Ok)()  — degenerate but tests postfix chaining
        match parse_ok("Method::Get()") {
            Expr::Call { callee, args, .. } => {
                assert!(args.is_empty());
                assert!(matches!(*callee, Expr::Static { .. }));
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn call_with_arguments() {
        match parse_ok("add(1, 2, 3)") {
            Expr::Call { args, .. } => assert_eq!(args.len(), 3),
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn index_postfix() {
        match parse_ok("$names[0]") {
            Expr::Index { .. } => {}
            other => panic!("expected Index, got {other:?}"),
        }
    }

    #[test]
    fn unary_not_neg_await() {
        assert!(matches!(
            parse_ok("!$ok"),
            Expr::Unary {
                op: UnaryOp::Not,
                ..
            }
        ));
        assert!(matches!(
            parse_ok("-$x"),
            Expr::Unary {
                op: UnaryOp::Neg,
                ..
            }
        ));
        assert!(matches!(
            parse_ok("await $future"),
            Expr::Unary {
                op: UnaryOp::Await,
                ..
            }
        ));
    }

    #[test]
    fn shared_and_mutable_borrow_prefixes() {
        assert!(matches!(
            parse_ok("&$x"),
            Expr::Borrow {
                kind: Borrow::Shared,
                ..
            }
        ));
        assert!(matches!(
            parse_ok("&flip $x"),
            Expr::Borrow {
                kind: Borrow::Mutable,
                ..
            }
        ));
    }

    #[test]
    fn cast_associates_left() {
        // ($x as int) as string
        match parse_ok("$x as int as string") {
            Expr::Cast { value, ty, .. } => {
                assert_eq!(ty.path[0].name, "string");
                assert!(matches!(*value, Expr::Cast { .. }));
            }
            other => panic!("expected Cast, got {other:?}"),
        }
    }

    #[test]
    fn arithmetic_precedence_mul_over_add() {
        // 1 + (2 * 3)
        match parse_ok("1 + 2 * 3") {
            Expr::Binary {
                op: BinOp::Add,
                rhs,
                ..
            } => {
                assert!(matches!(*rhs, Expr::Binary { op: BinOp::Mul, .. }));
            }
            other => panic!("expected Binary(Add), got {other:?}"),
        }
    }

    #[test]
    fn parentheses_override_precedence() {
        // (1 + 2) * 3
        match parse_ok("(1 + 2) * 3") {
            Expr::Binary {
                op: BinOp::Mul,
                lhs,
                ..
            } => {
                assert!(matches!(*lhs, Expr::Paren { .. }));
            }
            other => panic!("expected Binary(Mul), got {other:?}"),
        }
    }

    #[test]
    fn comparison_below_arithmetic() {
        // (1 + 2) < (3 * 4)
        match parse_ok("1 + 2 < 3 * 4") {
            Expr::Binary {
                op: BinOp::Lt,
                lhs,
                rhs,
                ..
            } => {
                assert!(matches!(*lhs, Expr::Binary { op: BinOp::Add, .. }));
                assert!(matches!(*rhs, Expr::Binary { op: BinOp::Mul, .. }));
            }
            other => panic!("expected Binary(Lt), got {other:?}"),
        }
    }

    #[test]
    fn equality_below_comparison() {
        match parse_ok("$a < $b == $c > $d") {
            Expr::Binary { op: BinOp::Eq, .. } => {}
            other => panic!("expected Binary(Eq), got {other:?}"),
        }
    }

    #[test]
    fn logical_and_or_chain_left_to_right() {
        // ($a && $b) || $c
        match parse_ok("$a && $b || $c") {
            Expr::Binary {
                op: BinOp::Or, lhs, ..
            } => {
                assert!(matches!(*lhs, Expr::Binary { op: BinOp::And, .. }));
            }
            other => panic!("expected Binary(Or), got {other:?}"),
        }
    }

    #[test]
    fn null_coalesce_is_lowest() {
        // $a || $b ?? $c — `??` sits BELOW `||` per spec/operators.md,
        // so this groups as ($a || $b) ?? $c.
        match parse_ok("$a || $b ?? $c") {
            Expr::Binary {
                op: BinOp::NullCoalesce,
                lhs,
                ..
            } => {
                assert!(matches!(*lhs, Expr::Binary { op: BinOp::Or, .. }));
            }
            other => panic!("expected Binary(NullCoalesce), got {other:?}"),
        }
    }

    #[test]
    fn string_literal_with_interpolation_parses_inner_expression() {
        match parse_ok(r#""hi {$user->name}!""#) {
            Expr::StrLit { parts, .. } => {
                assert_eq!(parts.len(), 3);
                assert!(matches!(parts[0], StrPart::Text(_)));
                assert!(matches!(parts[1], StrPart::Expr(Expr::Member { .. })));
                assert!(matches!(parts[2], StrPart::Text(_)));
            }
            other => panic!("expected StrLit, got {other:?}"),
        }
    }

    #[test]
    fn match_in_expression_emits_p7_diagnostic() {
        let toks = tokens_of("match ($x) { _ => 1 }");
        let mut cursor = Cursor::new(&toks, FileId(0), 21);
        let _ = parse_expr(&mut cursor);
        let diags = cursor.into_diagnostics();
        assert!(diags
            .iter()
            .any(|d| d.message.contains("not yet parsed (P7)")));
    }
}
