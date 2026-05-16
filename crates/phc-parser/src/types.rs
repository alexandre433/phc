// SPDX-License-Identifier: MIT
//! Type and generic-parameter productions.
//!
//! Grammar (`spec/grammar.ebnf`):
//!
//! ```text
//! Type          = TypePath [ TypeArgs ] [ Nullable ]
//! TypePath      = Identifier { "." Identifier }
//! TypeArgs      = "<" Type { "," Type } ">"
//! GenericParams = "<" GenericParam { "," GenericParam } ">"
//! GenericParam  = Identifier [ ":" BoundList ]
//! BoundList     = TypePath { "+" TypePath }
//! ```

use phc_ast::{GenericParam, Ident, TypeRef};
use phc_lexer::Token;
use phc_span::Span;

use crate::Cursor;

/// Parse a `Type`. Returns `None` and leaves a diagnostic on a
/// missing leading identifier; partial recovery on the inner
/// `TypeArgs` still surfaces a usable `TypeRef`.
pub(crate) fn parse_type(cursor: &mut Cursor<'_>) -> Option<TypeRef> {
    // D-024 function type: `fn(T, U): R`.
    if matches!(cursor.peek_token(), Some(Token::Fn)) {
        return parse_fn_type(cursor);
    }
    let path = parse_type_path(cursor)?;
    let lo = path.first().map(|s| s.span.lo).expect("non-empty path");
    let mut hi = path.last().map(|s| s.span.hi).expect("non-empty path");

    let args = if matches!(cursor.peek_token(), Some(Token::Lt)) {
        cursor.advance();
        let inner = parse_type_arg_list(cursor)?;
        let close = cursor.expect(&Token::Gt, "`>`").ok()?;
        hi = close.hi;
        inner
    } else {
        Vec::new()
    };

    let nullable = if matches!(cursor.peek_token(), Some(Token::Question)) {
        let q = cursor.advance().expect("question was peeked").span;
        hi = q.hi;
        true
    } else {
        false
    };

    Some(TypeRef {
        path,
        args,
        nullable,
        fn_return: None,
        span: Span::new(cursor.file(), lo, hi),
    })
}

/// Parse a function type starting at the `fn` keyword (D-024).
/// Shape: `fn(T1, T2, ...): R`. Empty param list is allowed
/// (`fn(): R`). Borrow modifiers in params land alongside the
/// borrowcheck capture-mode work (task #62); v0 accepts any
/// bare `Type` per parameter slot.
fn parse_fn_type(cursor: &mut Cursor<'_>) -> Option<TypeRef> {
    let fn_kw = cursor.expect(&Token::Fn, "`fn`").ok()?;
    cursor.expect(&Token::LParen, "`(` after `fn`").ok()?;
    let mut params = Vec::new();
    if !matches!(cursor.peek_token(), Some(Token::RParen)) {
        params.push(parse_type(cursor)?);
        while matches!(cursor.peek_token(), Some(Token::Comma)) {
            cursor.advance();
            if matches!(cursor.peek_token(), Some(Token::RParen)) {
                break;
            }
            params.push(parse_type(cursor)?);
        }
    }
    cursor.expect(&Token::RParen, "`)`").ok()?;
    cursor.expect(&Token::Colon, "`:` after `fn(...)`").ok()?;
    let ret = parse_type(cursor)?;
    let hi = ret.span.hi;
    let nullable = if matches!(cursor.peek_token(), Some(Token::Question)) {
        cursor.advance();
        true
    } else {
        false
    };
    Some(TypeRef {
        path: vec![Ident {
            name: "fn".to_string(),
            span: fn_kw,
        }],
        args: params,
        nullable,
        fn_return: Some(Box::new(ret)),
        span: Span::new(cursor.file(), fn_kw.lo, hi),
    })
}

fn parse_type_arg_list(cursor: &mut Cursor<'_>) -> Option<Vec<TypeRef>> {
    let first = parse_type(cursor)?;
    let mut args = vec![first];
    while matches!(cursor.peek_token(), Some(Token::Comma)) {
        cursor.advance();
        if matches!(cursor.peek_token(), Some(Token::Gt)) {
            break; // allow trailing comma
        }
        let next = parse_type(cursor)?;
        args.push(next);
    }
    Some(args)
}

/// Parse a `TypePath` (dot-separated identifier sequence).
///
/// Accepts the `void` keyword as a leading segment since it is the
/// only reserved word that doubles as a stdlib type name (spec
/// D-006a / keywords.md). All other primitives (`int`, `string`,
/// `bool`, `list`, ...) lex as plain identifiers.
pub(crate) fn parse_type_path(cursor: &mut Cursor<'_>) -> Option<Vec<Ident>> {
    let first = expect_type_segment(cursor, "type identifier")?;
    let mut segments = vec![first];
    while matches!(cursor.peek_token(), Some(Token::Dot)) {
        cursor.advance();
        let next = expect_type_segment(cursor, "identifier after `.`")?;
        segments.push(next);
    }
    Some(segments)
}

fn expect_type_segment(cursor: &mut Cursor<'_>, label: &str) -> Option<Ident> {
    let spanned = cursor.peek()?;
    let name_opt = match &spanned.token {
        Token::Ident(name) => Some(name.clone()),
        Token::Void => Some("void".to_string()),
        _ => None,
    };
    if let Some(name) = name_opt {
        let ident = Ident {
            name,
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

/// Parse `GenericParams` after the leading `<` has *not* been
/// consumed yet. Returns `None` and leaves the cursor untouched if
/// the next token is not `<`. On `<`, consumes through the closing
/// `>` (and reports a diagnostic on malformed bounds).
pub(crate) fn parse_optional_generic_params(cursor: &mut Cursor<'_>) -> Option<Vec<GenericParam>> {
    if !matches!(cursor.peek_token(), Some(Token::Lt)) {
        return Some(Vec::new());
    }
    cursor.advance();
    let mut params = Vec::new();
    let first = parse_generic_param(cursor)?;
    params.push(first);
    while matches!(cursor.peek_token(), Some(Token::Comma)) {
        cursor.advance();
        if matches!(cursor.peek_token(), Some(Token::Gt)) {
            break;
        }
        params.push(parse_generic_param(cursor)?);
    }
    cursor.expect(&Token::Gt, "`>`").ok()?;
    Some(params)
}

fn parse_generic_param(cursor: &mut Cursor<'_>) -> Option<GenericParam> {
    let name = expect_ident(cursor, "generic parameter name")?;
    let lo = name.span.lo;
    let mut hi = name.span.hi;
    let mut bounds = Vec::new();
    if matches!(cursor.peek_token(), Some(Token::Colon)) {
        cursor.advance();
        let first = parse_type_path(cursor)?;
        hi = first.last().map(|s| s.span.hi).expect("non-empty path");
        bounds.push(first);
        while matches!(cursor.peek_token(), Some(Token::Plus)) {
            cursor.advance();
            let next = parse_type_path(cursor)?;
            hi = next.last().map(|s| s.span.hi).expect("non-empty path");
            bounds.push(next);
        }
    }
    Some(GenericParam {
        name,
        bounds,
        span: Span::new(cursor.file(), lo, hi),
    })
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

    fn cursor_for(src: &str) -> (Vec<Spanned>, u32, FileId) {
        let tokens: Vec<Spanned> = Lexer::new(src, FileId(0))
            .map(|r| r.expect("clean lex"))
            .collect();
        (tokens, src.len() as u32, FileId(0))
    }

    fn type_of(src: &str) -> TypeRef {
        let (tokens, len, file) = cursor_for(src);
        let mut cursor = Cursor::new(&tokens, file, len);
        let ty = parse_type(&mut cursor).expect("parse_type returned None");
        let diags = cursor.into_diagnostics();
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        ty
    }

    fn names(path: &[Ident]) -> Vec<&str> {
        path.iter().map(|i| i.name.as_str()).collect()
    }

    #[test]
    fn primitive_type_parses() {
        let ty = type_of("int");
        assert_eq!(names(&ty.path), vec!["int"]);
        assert!(ty.args.is_empty());
        assert!(!ty.nullable);
    }

    #[test]
    fn dotted_path_type_parses() {
        let ty = type_of("app.User");
        assert_eq!(names(&ty.path), vec!["app", "User"]);
    }

    #[test]
    fn nullable_suffix_is_recorded() {
        let ty = type_of("string?");
        assert_eq!(names(&ty.path), vec!["string"]);
        assert!(ty.nullable);
    }

    #[test]
    fn generic_argument_parses() {
        let ty = type_of("list<int>");
        assert_eq!(names(&ty.path), vec!["list"]);
        assert_eq!(ty.args.len(), 1);
        assert_eq!(names(&ty.args[0].path), vec!["int"]);
    }

    #[test]
    fn nested_generic_parses() {
        let ty = type_of("map<string, list<User>>");
        assert_eq!(ty.args.len(), 2);
        assert_eq!(names(&ty.args[0].path), vec!["string"]);
        assert_eq!(names(&ty.args[1].path), vec!["list"]);
        assert_eq!(names(&ty.args[1].args[0].path), vec!["User"]);
    }

    #[test]
    fn nullable_after_generics_is_outermost() {
        let ty = type_of("list<int>?");
        assert!(ty.nullable);
        assert_eq!(ty.args.len(), 1);
        assert!(!ty.args[0].nullable);
    }

    #[test]
    fn generic_params_with_bounds_parse() {
        let (tokens, len, file) = cursor_for("<T: Ord, U: display + from>");
        let mut cursor = Cursor::new(&tokens, file, len);
        let params = parse_optional_generic_params(&mut cursor)
            .expect("parse_optional_generic_params returned None");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name.name, "T");
        assert_eq!(params[0].bounds.len(), 1);
        assert_eq!(names(&params[0].bounds[0]), vec!["Ord"]);
        assert_eq!(params[1].name.name, "U");
        assert_eq!(params[1].bounds.len(), 2);
        assert_eq!(names(&params[1].bounds[1]), vec!["from"]);
    }

    #[test]
    fn no_generic_params_returns_empty_vec() {
        let (tokens, len, file) = cursor_for("X");
        let mut cursor = Cursor::new(&tokens, file, len);
        let params = parse_optional_generic_params(&mut cursor).unwrap();
        assert!(params.is_empty());
        // Cursor untouched — `X` is still the next token.
        assert!(matches!(cursor.peek_token(), Some(Token::Ident(_))));
    }

    #[test]
    fn missing_type_after_lt_is_an_error() {
        let (tokens, len, file) = cursor_for("list<>");
        let mut cursor = Cursor::new(&tokens, file, len);
        let result = parse_type(&mut cursor);
        assert!(result.is_none());
        let diags = cursor.into_diagnostics();
        assert!(!diags.is_empty());
    }
}
