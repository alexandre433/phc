// SPDX-License-Identifier: MIT
//! Function-declaration production.
//!
//! Grammar (`spec/grammar.ebnf`):
//!
//! ```text
//! FunctionDecl = Visibility [ "async" ] "function" Identifier
//!                [ GenericParams ] "(" [ ParamList ] ")"
//!                ":" Type Block
//! ParamList    = Param { "," Param } [ "," ]
//! Param        = [ BorrowMod ] Type VarRef
//! BorrowMod    = "&" | "&" "flip"
//! Block        = "{" { Statement } "}"
//! ```
//!
//! Statements inside `Block` are not parsed yet — only the empty
//! `{}` body is accepted in this commit. Anything else surfaces a
//! diagnostic and the parser recovers to the closing `}`.

use phc_ast::{Block, Borrow, FunctionDecl, Ident, Param, Stmt, Visibility};
use phc_lexer::Token;
use phc_span::Span;

use crate::types::{parse_optional_generic_params, parse_type};
use crate::Cursor;

/// Parse a function declaration. Caller positions the cursor at the
/// first relevant token (`public`, `async`, or `function`).
pub(crate) fn parse_function_decl(cursor: &mut Cursor<'_>) -> Option<FunctionDecl> {
    let start_span = cursor.current_span();
    let visibility = if cursor.eat(&Token::Public).is_some() {
        Visibility::Public
    } else {
        Visibility::Default
    };
    let is_async = cursor.eat(&Token::Async).is_some();
    cursor.expect(&Token::Function, "`function` keyword").ok()?;
    let name = expect_ident(cursor, "function name")?;
    let generic_params = parse_optional_generic_params(cursor)?;
    cursor.expect(&Token::LParen, "`(`").ok()?;
    let params = parse_param_list(cursor)?;
    cursor.expect(&Token::RParen, "`)`").ok()?;
    cursor
        .expect(&Token::Colon, "`:` before return type")
        .ok()?;
    let return_type = parse_type(cursor)?;
    let body = parse_block(cursor)?;
    let end = body.span.hi;
    Some(FunctionDecl {
        visibility,
        is_async,
        name,
        generic_params,
        params,
        return_type,
        body,
        span: Span::new(cursor.file(), start_span.lo, end),
    })
}

fn parse_param_list(cursor: &mut Cursor<'_>) -> Option<Vec<Param>> {
    if matches!(cursor.peek_token(), Some(Token::RParen)) {
        return Some(Vec::new());
    }
    let first = parse_param(cursor)?;
    let mut params = vec![first];
    while matches!(cursor.peek_token(), Some(Token::Comma)) {
        cursor.advance();
        if matches!(cursor.peek_token(), Some(Token::RParen)) {
            break; // trailing comma
        }
        params.push(parse_param(cursor)?);
    }
    Some(params)
}

fn parse_param(cursor: &mut Cursor<'_>) -> Option<Param> {
    let lo = cursor.current_span().lo;
    let borrow = parse_borrow_mod(cursor);
    let ty = parse_type(cursor)?;
    let name = expect_var_ref(cursor)?;
    let hi = name.span.hi;
    Some(Param {
        borrow,
        ty,
        name,
        span: Span::new(cursor.file(), lo, hi),
    })
}

/// Borrow prefix on a parameter or expression operand. Returns
/// [`Borrow::None`] without consuming when the next token is not
/// `&`.
pub(crate) fn parse_borrow_mod(cursor: &mut Cursor<'_>) -> Borrow {
    if cursor.eat(&Token::Amp).is_none() {
        return Borrow::None;
    }
    if cursor.eat(&Token::Flip).is_some() {
        Borrow::Mutable
    } else {
        Borrow::Shared
    }
}

/// Consume a `$name` VarRef and return the bare identifier. The
/// leading `$` is required (D-023).
pub(crate) fn expect_var_ref(cursor: &mut Cursor<'_>) -> Option<Ident> {
    let dollar = cursor.expect(&Token::Dollar, "`$` sigil").ok()?;
    let name = expect_ident(cursor, "variable name after `$`")?;
    Some(Ident {
        name: name.name,
        span: Span::new(cursor.file(), dollar.lo, name.span.hi),
    })
}

fn parse_block(cursor: &mut Cursor<'_>) -> Option<Block> {
    let open = cursor.expect(&Token::LBrace, "`{`").ok()?;
    if !matches!(cursor.peek_token(), Some(Token::RBrace)) {
        let span = cursor.current_span();
        cursor.error(
            span,
            "function body statements are not yet parsed (P5 wires them)",
        );
        recover_to_block_close(cursor);
    }
    let close = cursor.expect(&Token::RBrace, "`}`").ok()?;
    Some(Block {
        statements: Vec::<Stmt>::new(),
        span: Span::new(cursor.file(), open.lo, close.hi),
    })
}

fn recover_to_block_close(cursor: &mut Cursor<'_>) {
    let mut depth: usize = 1;
    while let Some(spanned) = cursor.peek() {
        match &spanned.token {
            Token::LBrace => {
                depth += 1;
                cursor.advance();
            }
            Token::RBrace => {
                depth -= 1;
                if depth == 0 {
                    return;
                }
                cursor.advance();
            }
            _ => {
                cursor.advance();
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use phc_lexer::{Lexer, Spanned};
    use phc_span::FileId;

    fn parse(src: &str) -> (Option<FunctionDecl>, Vec<phc_errors::Diagnostic>) {
        let tokens: Vec<Spanned> = Lexer::new(src, FileId(0))
            .map(|r| r.expect("clean lex"))
            .collect();
        let mut cursor = Cursor::new(&tokens, FileId(0), src.len() as u32);
        let decl = parse_function_decl(&mut cursor);
        (decl, cursor.into_diagnostics())
    }

    fn parse_ok(src: &str) -> FunctionDecl {
        let (decl, diags) = parse(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        decl.expect("expected a FunctionDecl")
    }

    #[test]
    fn minimal_no_args_void_return() {
        let f = parse_ok("function main(): void {}");
        assert_eq!(f.name.name, "main");
        assert_eq!(f.visibility, Visibility::Default);
        assert!(!f.is_async);
        assert!(f.generic_params.is_empty());
        assert!(f.params.is_empty());
        assert_eq!(f.return_type.path[0].name, "void");
    }

    #[test]
    fn public_async_function() {
        let f = parse_ok("public async function fetch(): void {}");
        assert_eq!(f.visibility, Visibility::Public);
        assert!(f.is_async);
    }

    #[test]
    fn parameters_carry_borrow_modifier_and_dollar_name() {
        let f = parse_ok("function update(&flip User $u, &int $count): void {}");
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0].borrow, Borrow::Mutable);
        assert_eq!(f.params[0].ty.path[0].name, "User");
        assert_eq!(f.params[0].name.name, "u");
        assert_eq!(f.params[1].borrow, Borrow::Shared);
        assert_eq!(f.params[1].name.name, "count");
    }

    #[test]
    fn generic_function_with_bound() {
        let f = parse_ok("function max<T: Ord>(T $a, T $b): T {}");
        assert_eq!(f.generic_params.len(), 1);
        assert_eq!(f.generic_params[0].name.name, "T");
        assert_eq!(f.generic_params[0].bounds[0][0].name, "Ord");
        assert_eq!(f.return_type.path[0].name, "T");
    }

    #[test]
    fn trailing_comma_in_param_list() {
        let f = parse_ok("function f(int $a, int $b,): void {}");
        assert_eq!(f.params.len(), 2);
    }

    #[test]
    fn missing_function_keyword_is_an_error() {
        let (_decl, diags) = parse("public main(): void {}");
        assert!(diags
            .iter()
            .any(|d| d.message.contains("`function` keyword")));
    }

    #[test]
    fn missing_dollar_on_param_is_an_error() {
        let (_decl, diags) = parse("function f(int x): void {}");
        assert!(diags.iter().any(|d| d.message.contains("`$` sigil")));
    }

    #[test]
    fn statements_in_body_emit_todo_diagnostic() {
        let (_decl, diags) = parse("function f(): void { return; }");
        assert!(diags.iter().any(|d| d.message.contains("not yet parsed")));
    }
}
