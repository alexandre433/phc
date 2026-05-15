// SPDX-License-Identifier: MIT
//! Statement productions and the block parser that consumes them.
//!
//! Grammar (`spec/grammar.ebnf`):
//!
//! ```text
//! Statement    = LocalBinding | Reassign | IfStmt | WhileStmt
//!              | ForStmt | ReturnStmt | BreakStmt | ContinueStmt
//!              | ExprStmt
//! LocalBinding = [ "flip" ] Type VarRef "=" Expression ";"
//! Reassign     = ReassignLhs ":=" Expression ";"
//! ReassignLhs  = VarRef { Arrow Identifier }
//! ```
//!
//! `LocalBinding` shares its leading `Type` with `ExprStmt` (both
//! can start with a bare identifier). The disambiguator uses a
//! cursor checkpoint: it tries to parse `[flip] Type VarRef =` and
//! restores on failure, falling through to the expression-based
//! path that handles both `Reassign` and `ExprStmt`.

use phc_ast::{
    Block, Expr, ExprStmt, ForStmt, Ident, IfStmt, LocalBinding, MemberAssignStmt, ReassignStmt,
    ReturnStmt, Stmt, WhileStmt,
};
use phc_lexer::Token;
use phc_span::Span;

use crate::expressions::parse_expr;
use crate::functions::expect_var_ref;
use crate::types::parse_type;
use crate::Cursor;

/// Parse a `{ ... }` block of statements.
pub(crate) fn parse_block(cursor: &mut Cursor<'_>) -> Option<Block> {
    let open = cursor.expect(&Token::LBrace, "`{`").ok()?;
    let mut stmts = Vec::new();
    while !matches!(cursor.peek_token(), Some(Token::RBrace) | None) {
        let before = cursor.pos();
        if let Some(stmt) = parse_statement(cursor) {
            stmts.push(stmt);
        } else {
            recover_inside_block(cursor);
            if cursor.pos() == before {
                cursor.advance();
            }
        }
    }
    let close = cursor.expect(&Token::RBrace, "`}`").ok()?;
    Some(Block {
        statements: stmts,
        span: Span::new(cursor.file(), open.lo, close.hi),
    })
}

fn parse_statement(cursor: &mut Cursor<'_>) -> Option<Stmt> {
    match cursor.peek_token() {
        Some(Token::Flip) => parse_local_binding(cursor, true),
        Some(Token::If) => parse_if_stmt(cursor).map(Stmt::If),
        Some(Token::While) => parse_while_stmt(cursor).map(Stmt::While),
        Some(Token::For) => parse_for_stmt(cursor).map(Stmt::For),
        Some(Token::Return) => parse_return_stmt(cursor).map(Stmt::Return),
        Some(Token::Break) => {
            let span = cursor.advance().expect("break was peeked").span;
            let semi = cursor.expect(&Token::Semicolon, "`;`").ok()?;
            Some(Stmt::Break {
                span: Span::new(cursor.file(), span.lo, semi.hi),
            })
        }
        Some(Token::Continue) => {
            let span = cursor.advance().expect("continue was peeked").span;
            let semi = cursor.expect(&Token::Semicolon, "`;`").ok()?;
            Some(Stmt::Continue {
                span: Span::new(cursor.file(), span.lo, semi.hi),
            })
        }
        Some(_) => parse_local_or_expr_or_reassign(cursor),
        None => None,
    }
}

fn parse_local_binding(cursor: &mut Cursor<'_>, has_flip_keyword: bool) -> Option<Stmt> {
    let start = cursor.current_span().lo;
    if has_flip_keyword {
        cursor.expect(&Token::Flip, "`flip`").ok()?;
    }
    let ty = parse_type(cursor)?;
    let name = expect_var_ref(cursor)?;
    cursor.expect(&Token::Eq, "`=` in local binding").ok()?;
    let value = parse_expr(cursor)?;
    let end = cursor.expect(&Token::Semicolon, "`;`").ok()?.hi;
    Some(Stmt::Local(LocalBinding {
        is_mut: has_flip_keyword,
        ty,
        name,
        value,
        span: Span::new(cursor.file(), start, end),
    }))
}

/// Disambiguate the three statement forms that share an Ident or `$`
/// leading token: LocalBinding, Reassign, ExprStmt. Tries the local
/// binding shape first via a cursor checkpoint; on rollback parses
/// an expression and decides between Reassign and ExprStmt by the
/// next token.
fn parse_local_or_expr_or_reassign(cursor: &mut Cursor<'_>) -> Option<Stmt> {
    let cp = cursor.checkpoint();
    let lo = cursor.current_span().lo;
    if let Some(local) = try_local_binding(cursor) {
        return Some(local);
    }
    cursor.restore(cp);

    let lhs = parse_expr(cursor)?;
    if cursor.eat(&Token::Reassign).is_some() {
        validate_reassign_lhs(cursor, &lhs);
        let value = parse_expr(cursor)?;
        let end = cursor.expect(&Token::Semicolon, "`;`").ok()?.hi;
        return Some(Stmt::Reassign(ReassignStmt {
            lhs,
            value,
            span: Span::new(cursor.file(), lo, end),
        }));
    }
    if cursor.eat(&Token::Eq).is_some() {
        validate_member_assign_lhs(cursor, &lhs);
        let value = parse_expr(cursor)?;
        let end = cursor.expect(&Token::Semicolon, "`;`").ok()?.hi;
        return Some(Stmt::MemberAssign(MemberAssignStmt {
            lhs,
            value,
            span: Span::new(cursor.file(), lo, end),
        }));
    }
    let end = cursor.expect(&Token::Semicolon, "`;`").ok()?.hi;
    Some(Stmt::Expr(ExprStmt {
        expr: lhs,
        span: Span::new(cursor.file(), lo, end),
    }))
}

/// Try to parse a non-`flip` local binding (`Type VarRef = expr;`).
/// Returns `Some` only on full success; on any failure the cursor
/// state is intact-by-virtue-of-checkpoint at the call site.
fn try_local_binding(cursor: &mut Cursor<'_>) -> Option<Stmt> {
    let start = cursor.current_span().lo;
    let ty = parse_type(cursor)?;
    if !matches!(cursor.peek_token(), Some(Token::Dollar)) {
        return None;
    }
    let name = expect_var_ref(cursor)?;
    if !matches!(cursor.peek_token(), Some(Token::Eq)) {
        return None;
    }
    cursor.advance();
    let value = parse_expr(cursor)?;
    let end = cursor.expect(&Token::Semicolon, "`;`").ok()?.hi;
    Some(Stmt::Local(LocalBinding {
        is_mut: false,
        ty,
        name,
        value,
        span: Span::new(cursor.file(), start, end),
    }))
}

fn parse_if_stmt(cursor: &mut Cursor<'_>) -> Option<IfStmt> {
    let start = cursor.expect(&Token::If, "`if`").ok()?;
    cursor.expect(&Token::LParen, "`(`").ok()?;
    let cond = parse_expr(cursor)?;
    cursor.expect(&Token::RParen, "`)`").ok()?;
    let block = parse_block(cursor)?;
    let mut branches = vec![(cond, block)];
    let mut else_block: Option<Block> = None;
    let mut end = branches.last().unwrap().1.span.hi;
    while cursor.eat(&Token::Else).is_some() {
        if cursor.eat(&Token::If).is_some() {
            cursor.expect(&Token::LParen, "`(` after `else if`").ok()?;
            let cond = parse_expr(cursor)?;
            cursor.expect(&Token::RParen, "`)`").ok()?;
            let blk = parse_block(cursor)?;
            end = blk.span.hi;
            branches.push((cond, blk));
        } else {
            let blk = parse_block(cursor)?;
            end = blk.span.hi;
            else_block = Some(blk);
            break;
        }
    }
    Some(IfStmt {
        branches,
        else_block,
        span: Span::new(cursor.file(), start.lo, end),
    })
}

fn parse_while_stmt(cursor: &mut Cursor<'_>) -> Option<WhileStmt> {
    let start = cursor.expect(&Token::While, "`while`").ok()?;
    cursor.expect(&Token::LParen, "`(`").ok()?;
    let cond = parse_expr(cursor)?;
    cursor.expect(&Token::RParen, "`)`").ok()?;
    let body = parse_block(cursor)?;
    let end = body.span.hi;
    Some(WhileStmt {
        cond,
        body,
        span: Span::new(cursor.file(), start.lo, end),
    })
}

fn parse_for_stmt(cursor: &mut Cursor<'_>) -> Option<ForStmt> {
    let start = cursor.expect(&Token::For, "`for`").ok()?;
    cursor.expect(&Token::LParen, "`(`").ok()?;
    let elem_ty = parse_type(cursor)?;
    let elem_name = expect_var_ref(cursor)?;
    cursor.expect(&Token::In, "`in`").ok()?;
    let iter = parse_expr(cursor)?;
    cursor.expect(&Token::RParen, "`)`").ok()?;
    let body = parse_block(cursor)?;
    let end = body.span.hi;
    Some(ForStmt {
        elem_ty,
        elem_name,
        iter,
        body,
        span: Span::new(cursor.file(), start.lo, end),
    })
}

fn parse_return_stmt(cursor: &mut Cursor<'_>) -> Option<ReturnStmt> {
    let start = cursor.expect(&Token::Return, "`return`").ok()?;
    let value = if matches!(cursor.peek_token(), Some(Token::Semicolon)) {
        None
    } else {
        Some(parse_expr(cursor)?)
    };
    let end = cursor.expect(&Token::Semicolon, "`;`").ok()?.hi;
    Some(ReturnStmt {
        value,
        span: Span::new(cursor.file(), start.lo, end),
    })
}

/// Walk a parsed expression and emit a diagnostic if it is not a
/// valid LHS for `:=`. Per the EBNF, only `VarRef { Arrow Ident }`
/// is permitted (`$x`, `$u->loginCount`, ...).
fn validate_reassign_lhs(cursor: &mut Cursor<'_>, lhs: &Expr) {
    fn is_lhs(expr: &Expr) -> bool {
        match expr {
            Expr::Var { .. } | Expr::This { .. } => true,
            Expr::Member { receiver, .. } => is_lhs(receiver),
            _ => false,
        }
    }
    if !is_lhs(lhs) {
        let span = expr_span(lhs);
        cursor.error(
            span,
            "left-hand side of `:=` must be `$name` or a `->` chain rooted at `$name`",
        );
    }
}

/// D-005a: the LHS of a member-assignment `=` must be an
/// [`Expr::Member`] chain rooted at a `$name` or `$this`. A bare
/// variable LHS is rejected so `$x = expr;` keeps requiring `:=`
/// with a `flip` declaration.
fn validate_member_assign_lhs(cursor: &mut Cursor<'_>, lhs: &Expr) {
    fn rooted_at_var(expr: &Expr) -> bool {
        match expr {
            Expr::Var { .. } | Expr::This { .. } => true,
            Expr::Member { receiver, .. } => rooted_at_var(receiver),
            _ => false,
        }
    }
    let ok = matches!(lhs, Expr::Member { .. }) && rooted_at_var(lhs);
    if !ok {
        let span = expr_span(lhs);
        cursor.error(
            span,
            "left-hand side of `=` must be a `->` chain rooted at `$name` or `$this`; \
             use `:=` (with `flip`) to reassign a variable",
        );
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
        | Expr::Try { span, .. }
        | Expr::Unary { span, .. }
        | Expr::Borrow { span, .. }
        | Expr::Cast { span, .. }
        | Expr::Binary { span, .. }
        | Expr::Match { span, .. }
        | Expr::Lambda { span, .. } => *span,
    }
}

/// Skip past a malformed statement, stopping at `;` (consumed) or at
/// `}` (left in place so the enclosing block close still matches).
fn recover_inside_block(cursor: &mut Cursor<'_>) {
    while let Some(spanned) = cursor.peek() {
        match &spanned.token {
            Token::Semicolon => {
                cursor.advance();
                return;
            }
            Token::RBrace => return,
            _ => {
                cursor.advance();
            }
        }
    }
}

// `Ident` import is used by sub-modules of the test suite even when
// the production parser never instantiates one directly here.
#[allow(dead_code)]
fn _unused_ident_marker() -> Option<Ident> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use phc_ast::{BinOp, Borrow, FunctionDecl, Visibility};
    use phc_lexer::{Lexer, Spanned};
    use phc_span::FileId;

    fn parse_function(src: &str) -> (Option<FunctionDecl>, Vec<phc_errors::Diagnostic>) {
        let toks: Vec<Spanned> = Lexer::new(src, FileId(0))
            .map(|r| r.expect("clean lex"))
            .collect();
        let mut cursor = Cursor::new(&toks, FileId(0), src.len() as u32);
        let decl = crate::functions::parse_function_decl(&mut cursor);
        (decl, cursor.into_diagnostics())
    }

    fn body_of(src: &str) -> Block {
        let (decl, diags) = parse_function(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        decl.expect("expected a FunctionDecl").body
    }

    #[test]
    fn empty_block_still_parses() {
        let body = body_of("function f(): void {}");
        assert!(body.statements.is_empty());
    }

    #[test]
    fn local_binding_immutable() {
        let body = body_of("function f(): void { int $x = 1; }");
        match &body.statements[0] {
            Stmt::Local(b) => {
                assert!(!b.is_mut);
                assert_eq!(b.ty.path[0].name, "int");
                assert_eq!(b.name.name, "x");
                assert!(matches!(b.value, Expr::IntLit { .. }));
            }
            other => panic!("expected Local, got {other:?}"),
        }
    }

    #[test]
    fn local_binding_with_flip() {
        let body = body_of("function f(): void { flip int $score = 0; }");
        match &body.statements[0] {
            Stmt::Local(b) => assert!(b.is_mut),
            other => panic!("expected Local, got {other:?}"),
        }
    }

    #[test]
    fn reassign_to_var() {
        let body = body_of("function f(): void { flip int $x = 0; $x := $x + 1; }");
        assert_eq!(body.statements.len(), 2);
        match &body.statements[1] {
            Stmt::Reassign(r) => {
                assert!(matches!(r.lhs, Expr::Var { .. }));
                assert!(matches!(r.value, Expr::Binary { op: BinOp::Add, .. }));
            }
            other => panic!("expected Reassign, got {other:?}"),
        }
    }

    #[test]
    fn reassign_to_member_chain() {
        let body =
            body_of("function f(&flip User $u): void { $u->loginCount := $u->loginCount + 1; }");
        match &body.statements[0] {
            Stmt::Reassign(r) => assert!(matches!(r.lhs, Expr::Member { .. })),
            other => panic!("expected Reassign, got {other:?}"),
        }
    }

    #[test]
    fn reassign_to_literal_is_an_error() {
        let (_decl, diags) = parse_function("function f(): void { 1 := 2; }");
        assert!(diags
            .iter()
            .any(|d| d.message.contains("left-hand side of `:=`")));
    }

    #[test]
    fn expression_statement() {
        let body = body_of(r#"function f(): void { Logger::info("hi"); }"#);
        match &body.statements[0] {
            Stmt::Expr(e) => assert!(matches!(e.expr, Expr::Call { .. })),
            other => panic!("expected Expr, got {other:?}"),
        }
    }

    #[test]
    fn return_with_value() {
        let body = body_of("function f(): int { return $x + 1; }");
        match &body.statements[0] {
            Stmt::Return(r) => assert!(matches!(r.value, Some(Expr::Binary { .. }))),
            other => panic!("expected Return, got {other:?}"),
        }
    }

    #[test]
    fn bare_return() {
        let body = body_of("function f(): void { return; }");
        match &body.statements[0] {
            Stmt::Return(r) => assert!(r.value.is_none()),
            other => panic!("expected Return, got {other:?}"),
        }
    }

    #[test]
    fn if_else_chain() {
        let body =
            body_of("function f(): void { if ($a) {} else if ($b) {} else if ($c) {} else {} }");
        match &body.statements[0] {
            Stmt::If(i) => {
                assert_eq!(i.branches.len(), 3);
                assert!(i.else_block.is_some());
            }
            other => panic!("expected If, got {other:?}"),
        }
    }

    #[test]
    fn while_loop() {
        let body = body_of("function f(): void { while ($x < 10) { $x := $x + 1; } }");
        match &body.statements[0] {
            Stmt::While(w) => {
                assert!(matches!(w.cond, Expr::Binary { .. }));
                assert_eq!(w.body.statements.len(), 1);
            }
            other => panic!("expected While, got {other:?}"),
        }
    }

    #[test]
    fn for_loop() {
        let body = body_of("function f(): void { for (int $n in $nums) { $sum := $sum + $n; } }");
        match &body.statements[0] {
            Stmt::For(f) => {
                assert_eq!(f.elem_ty.path[0].name, "int");
                assert_eq!(f.elem_name.name, "n");
                assert!(matches!(f.iter, Expr::Var { .. }));
            }
            other => panic!("expected For, got {other:?}"),
        }
    }

    #[test]
    fn break_and_continue() {
        let body = body_of(
            "function f(): void { while (true) { if ($x) { break; } else { continue; } } }",
        );
        if let Stmt::While(w) = &body.statements[0] {
            if let Stmt::If(i) = &w.body.statements[0] {
                assert!(matches!(i.branches[0].1.statements[0], Stmt::Break { .. }));
                assert!(matches!(
                    i.else_block.as_ref().unwrap().statements[0],
                    Stmt::Continue { .. }
                ));
            } else {
                panic!("expected nested If");
            }
        } else {
            panic!("expected While");
        }
    }

    #[test]
    fn missing_semicolon_after_local_recovers_to_next_statement() {
        let (_decl, diags) = parse_function("function f(): void { int $x = 1\n int $y = 2; }");
        // First binding triggers a `;` diagnostic; recovery still
        // emits the second binding.
        assert!(diags.iter().any(|d| d.message.contains("`;`")));
    }

    #[test]
    fn nested_blocks_compose() {
        let body = body_of("function f(): void { if (true) { if (false) { return; } else { } } }");
        assert!(matches!(&body.statements[0], Stmt::If(_)));
    }

    #[test]
    fn member_assign_via_equals() {
        // D-005a: `$this->createdAt = ...;` is a real Statement.
        let body = body_of("function f(): void { $this->createdAt := 0; $this->createdAt = 7; }");
        assert_eq!(body.statements.len(), 2);
        assert!(matches!(body.statements[0], Stmt::Reassign(_)));
        match &body.statements[1] {
            Stmt::MemberAssign(m) => {
                assert!(matches!(m.lhs, Expr::Member { .. }));
                assert!(matches!(m.value, Expr::IntLit { .. }));
            }
            other => panic!("expected MemberAssign, got {other:?}"),
        }
    }

    #[test]
    fn member_assign_through_a_long_chain() {
        let body = body_of("function f(): void { $user->profile->bio = $value; }");
        match &body.statements[0] {
            Stmt::MemberAssign(m) => {
                if let Expr::Member { receiver, .. } = &m.lhs {
                    assert!(matches!(**receiver, Expr::Member { .. }));
                } else {
                    panic!("expected Member root");
                }
            }
            other => panic!("expected MemberAssign, got {other:?}"),
        }
    }

    #[test]
    fn bare_var_with_equals_is_rejected() {
        // `$x = 1;` with no `->` is NOT a member assignment; D-005
        // still requires `:=` for variable reassignment.
        let (_decl, diags) = parse_function("function f(): void { flip int $x = 0; $x = 1; }");
        assert!(diags
            .iter()
            .any(|d| d.message.contains("left-hand side of `=`")));
    }

    #[test]
    fn public_function_with_borrow_param_and_body() {
        let body = body_of("public function bump(&flip int $c): void { $c := $c + 1; return; }");
        assert_eq!(body.statements.len(), 2);
        let _ = Visibility::Public; // silence unused import
        let _ = Borrow::None;
    }
}
