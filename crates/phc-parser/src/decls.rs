// SPDX-License-Identifier: MIT
//! Class, enum, interface, trait, and test declarations.
//!
//! See `spec/grammar.ebnf` §"Declarations" for the productions
//! implemented here. The function-declaration entry point lives in
//! [`crate::functions`] and is reused for trait methods and class
//! methods.

use phc_ast::{
    Borrow, ClassDecl, ClassMember, ConstructDecl, ConstructParam, EnumDecl, EnumVariant, Expr,
    FieldDecl, Ident, InterfaceDecl, MethodSig, Param, PropertyHook, StrPart, TestDecl, TraitDecl,
    TraitUse, Visibility,
};
use phc_lexer::{StringPart, Token};
use phc_span::Span;

use crate::expressions::parse_expr;
use crate::functions::{expect_var_ref, parse_borrow_mod, parse_function_decl};
use crate::statements::parse_block;
use crate::types::{parse_optional_generic_params, parse_type, parse_type_path};
use crate::Cursor;

// ===== Class =====

pub(crate) fn parse_class_decl(cursor: &mut Cursor<'_>) -> Option<ClassDecl> {
    let start = cursor.current_span().lo;
    let visibility = if cursor.eat(&Token::Public).is_some() {
        Visibility::Public
    } else {
        Visibility::Default
    };
    cursor.expect(&Token::Class, "`class` keyword").ok()?;
    let name = expect_ident(cursor, "class name")?;
    let generic_params = parse_optional_generic_params(cursor)?;
    let implements = if cursor.eat(&Token::Implements).is_some() {
        parse_interface_list(cursor)?
    } else {
        Vec::new()
    };
    cursor
        .expect(&Token::LBrace, "`{` before class body")
        .ok()?;
    let mut members = Vec::new();
    while !matches!(cursor.peek_token(), Some(Token::RBrace) | None) {
        let before = cursor.pos();
        if let Some(member) = parse_class_member(cursor) {
            members.push(member);
        } else {
            recover_inside_class(cursor);
            if cursor.pos() == before {
                cursor.advance();
            }
        }
    }
    let close = cursor.expect(&Token::RBrace, "`}`").ok()?;
    Some(ClassDecl {
        visibility,
        name,
        generic_params,
        implements,
        members,
        span: Span::new(cursor.file(), start, close.hi),
    })
}

fn parse_interface_list(cursor: &mut Cursor<'_>) -> Option<Vec<Vec<Ident>>> {
    let first = parse_type_path(cursor)?;
    let mut list = vec![first];
    while matches!(cursor.peek_token(), Some(Token::Comma)) {
        cursor.advance();
        if matches!(cursor.peek_token(), Some(Token::LBrace)) {
            break;
        }
        list.push(parse_type_path(cursor)?);
    }
    Some(list)
}

fn parse_class_member(cursor: &mut Cursor<'_>) -> Option<ClassMember> {
    match cursor.peek_token() {
        Some(Token::Construct) => parse_construct_decl(cursor).map(ClassMember::Construct),
        Some(Token::Use) => parse_trait_use(cursor).map(ClassMember::TraitUse),
        _ => parse_field_or_method(cursor),
    }
}

fn parse_construct_decl(cursor: &mut Cursor<'_>) -> Option<ConstructDecl> {
    let start = cursor.expect(&Token::Construct, "`construct`").ok()?;
    cursor.expect(&Token::LParen, "`(`").ok()?;
    let params = parse_construct_params(cursor)?;
    cursor.expect(&Token::RParen, "`)`").ok()?;
    let body = parse_block(cursor)?;
    let end = body.span.hi;
    Some(ConstructDecl {
        params,
        body,
        span: Span::new(cursor.file(), start.lo, end),
    })
}

fn parse_construct_params(cursor: &mut Cursor<'_>) -> Option<Vec<ConstructParam>> {
    if matches!(cursor.peek_token(), Some(Token::RParen)) {
        return Some(Vec::new());
    }
    let first = parse_construct_param(cursor)?;
    let mut params = vec![first];
    while matches!(cursor.peek_token(), Some(Token::Comma)) {
        cursor.advance();
        if matches!(cursor.peek_token(), Some(Token::RParen)) {
            break;
        }
        params.push(parse_construct_param(cursor)?);
    }
    Some(params)
}

fn parse_construct_param(cursor: &mut Cursor<'_>) -> Option<ConstructParam> {
    let lo = cursor.current_span().lo;
    let promoted = cursor.eat(&Token::Public).is_some();
    let borrow = parse_borrow_mod(cursor);
    let is_mut = cursor.eat(&Token::Flip).is_some();
    let ty = parse_type(cursor)?;
    let name = expect_var_ref(cursor)?;
    let hi = name.span.hi;
    Some(ConstructParam {
        promoted,
        borrow,
        is_mut,
        ty,
        name,
        span: Span::new(cursor.file(), lo, hi),
    })
}

fn parse_trait_use(cursor: &mut Cursor<'_>) -> Option<TraitUse> {
    let start = cursor.expect(&Token::Use, "`use`").ok()?;
    let path = parse_type_path(cursor)?;
    let end = cursor.expect(&Token::Semicolon, "`;`").ok()?.hi;
    Some(TraitUse {
        path,
        span: Span::new(cursor.file(), start.lo, end),
    })
}

/// Disambiguate a class member that does *not* start with
/// `construct`/`use`. Field and method headers both can begin with
/// optional `public` then either `flip`/`Type` (field) or
/// `async`/`function` (method).
fn parse_field_or_method(cursor: &mut Cursor<'_>) -> Option<ClassMember> {
    let mut offset = 0;
    // Leading `@name` attributes (D-052) only attach to methods, so
    // their presence forces the method path regardless of what follows.
    let mut has_attr = false;
    while matches!(cursor.peek_at(offset).map(|s| &s.token), Some(Token::At)) {
        has_attr = true;
        offset += 2; // `@` + name
    }
    if matches!(
        cursor.peek_at(offset).map(|s| &s.token),
        Some(Token::Public)
    ) {
        offset += 1;
    }
    if matches!(cursor.peek_at(offset).map(|s| &s.token), Some(Token::Async)) {
        offset += 1;
    }
    match cursor.peek_at(offset).map(|s| &s.token) {
        Some(Token::Function) => parse_function_decl(cursor).map(ClassMember::Method),
        _ if has_attr => parse_function_decl(cursor).map(ClassMember::Method),
        _ => parse_field_decl(cursor).map(ClassMember::Field),
    }
}

fn parse_field_decl(cursor: &mut Cursor<'_>) -> Option<FieldDecl> {
    let start = cursor.current_span().lo;
    let visibility = if cursor.eat(&Token::Public).is_some() {
        Visibility::Public
    } else {
        Visibility::Default
    };
    let is_mut = cursor.eat(&Token::Flip).is_some();
    let ty = parse_type(cursor)?;
    let name = expect_var_ref(cursor)?;
    let default = if cursor.eat(&Token::Eq).is_some() {
        Some(parse_expr(cursor)?)
    } else {
        None
    };
    let hooks = if matches!(cursor.peek_token(), Some(Token::LBrace)) {
        parse_hook_block(cursor)?
    } else {
        Vec::new()
    };
    let end = cursor.expect(&Token::Semicolon, "`;`").ok()?.hi;
    Some(FieldDecl {
        visibility,
        is_mut,
        ty,
        name,
        default,
        hooks,
        span: Span::new(cursor.file(), start, end),
    })
}

fn parse_hook_block(cursor: &mut Cursor<'_>) -> Option<Vec<PropertyHook>> {
    cursor
        .expect(&Token::LBrace, "`{` opening hook block")
        .ok()?;
    let mut hooks = Vec::new();
    while !matches!(cursor.peek_token(), Some(Token::RBrace) | None) {
        let hook = parse_hook(cursor)?;
        hooks.push(hook);
    }
    cursor
        .expect(&Token::RBrace, "`}` closing hook block")
        .ok()?;
    Some(hooks)
}

fn parse_hook(cursor: &mut Cursor<'_>) -> Option<PropertyHook> {
    let spanned = cursor.peek()?.clone();
    let Token::Ident(name) = &spanned.token else {
        let span = spanned.span;
        cursor.error(span, "expected `get` or `set` hook");
        return None;
    };
    match name.as_str() {
        "get" => {
            cursor.advance();
            if cursor.eat(&Token::FatArrow).is_some() {
                let expr = parse_expr(cursor)?;
                let end = cursor.expect(&Token::Semicolon, "`;`").ok()?.hi;
                Some(PropertyHook::GetExpr {
                    expr,
                    span: Span::new(cursor.file(), spanned.span.lo, end),
                })
            } else {
                let body = parse_block(cursor)?;
                let end = body.span.hi;
                Some(PropertyHook::GetBlock {
                    body,
                    span: Span::new(cursor.file(), spanned.span.lo, end),
                })
            }
        }
        "set" => {
            cursor.advance();
            cursor.expect(&Token::LParen, "`(`").ok()?;
            let param_ty = parse_type(cursor)?;
            let param_name = expect_var_ref(cursor)?;
            cursor.expect(&Token::RParen, "`)`").ok()?;
            let body = parse_block(cursor)?;
            let end = body.span.hi;
            Some(PropertyHook::Set {
                param_ty,
                param_name,
                body,
                span: Span::new(cursor.file(), spanned.span.lo, end),
            })
        }
        _ => {
            let span = spanned.span;
            cursor.error(span, "expected `get` or `set` hook");
            None
        }
    }
}

fn recover_inside_class(cursor: &mut Cursor<'_>) {
    while let Some(spanned) = cursor.peek() {
        match &spanned.token {
            Token::Semicolon => {
                cursor.advance();
                return;
            }
            Token::RBrace
            | Token::Construct
            | Token::Use
            | Token::Function
            | Token::Public
            | Token::Async
            | Token::At => return,
            _ => {
                cursor.advance();
            }
        }
    }
}

// ===== Enum =====

pub(crate) fn parse_enum_decl(cursor: &mut Cursor<'_>) -> Option<EnumDecl> {
    let start = cursor.current_span().lo;
    let visibility = if cursor.eat(&Token::Public).is_some() {
        Visibility::Public
    } else {
        Visibility::Default
    };
    cursor.expect(&Token::Enum, "`enum`").ok()?;
    let name = expect_ident(cursor, "enum name")?;
    let backing = if cursor.eat(&Token::Colon).is_some() {
        Some(parse_type(cursor)?)
    } else {
        None
    };
    cursor.expect(&Token::LBrace, "`{` before enum body").ok()?;
    let variants = parse_enum_variants(cursor)?;
    let close = cursor.expect(&Token::RBrace, "`}`").ok()?;
    Some(EnumDecl {
        visibility,
        name,
        backing,
        variants,
        span: Span::new(cursor.file(), start, close.hi),
    })
}

fn parse_enum_variants(cursor: &mut Cursor<'_>) -> Option<Vec<EnumVariant>> {
    if matches!(cursor.peek_token(), Some(Token::RBrace)) {
        return Some(Vec::new());
    }
    let first = parse_enum_variant(cursor)?;
    let mut variants = vec![first];
    while matches!(cursor.peek_token(), Some(Token::Comma)) {
        cursor.advance();
        if matches!(cursor.peek_token(), Some(Token::RBrace)) {
            break;
        }
        variants.push(parse_enum_variant(cursor)?);
    }
    Some(variants)
}

fn parse_enum_variant(cursor: &mut Cursor<'_>) -> Option<EnumVariant> {
    let name = expect_ident(cursor, "enum variant name")?;
    let lo = name.span.lo;
    let mut hi = name.span.hi;
    let value = if cursor.eat(&Token::Eq).is_some() {
        let expr = parse_expr(cursor)?;
        hi = match &expr {
            Expr::IntLit { span, .. }
            | Expr::FloatLit { span, .. }
            | Expr::StrLit { span, .. }
            | Expr::BoolLit { span, .. }
            | Expr::NullLit { span } => span.hi,
            _ => hi,
        };
        Some(expr)
    } else {
        None
    };
    Some(EnumVariant {
        name,
        value,
        span: Span::new(cursor.file(), lo, hi),
    })
}

// ===== Interface =====

pub(crate) fn parse_interface_decl(cursor: &mut Cursor<'_>) -> Option<InterfaceDecl> {
    let start = cursor.current_span().lo;
    let visibility = if cursor.eat(&Token::Public).is_some() {
        Visibility::Public
    } else {
        Visibility::Default
    };
    cursor.expect(&Token::Interface, "`interface`").ok()?;
    let name = expect_ident(cursor, "interface name")?;
    let generic_params = parse_optional_generic_params(cursor)?;
    cursor
        .expect(&Token::LBrace, "`{` before interface body")
        .ok()?;
    let mut methods = Vec::new();
    while !matches!(cursor.peek_token(), Some(Token::RBrace) | None) {
        let before = cursor.pos();
        if let Some(sig) = parse_method_sig(cursor) {
            methods.push(sig);
        } else {
            recover_inside_class(cursor);
            if cursor.pos() == before {
                cursor.advance();
            }
        }
    }
    let close = cursor.expect(&Token::RBrace, "`}`").ok()?;
    Some(InterfaceDecl {
        visibility,
        name,
        generic_params,
        methods,
        span: Span::new(cursor.file(), start, close.hi),
    })
}

fn parse_method_sig(cursor: &mut Cursor<'_>) -> Option<MethodSig> {
    let start = cursor.expect(&Token::Function, "`function` keyword").ok()?;
    let name = expect_ident(cursor, "method name")?;
    let generic_params = parse_optional_generic_params(cursor)?;
    cursor.expect(&Token::LParen, "`(`").ok()?;
    let params = parse_param_list(cursor)?;
    cursor.expect(&Token::RParen, "`)`").ok()?;
    cursor
        .expect(&Token::Colon, "`:` before return type")
        .ok()?;
    let return_type = parse_type(cursor)?;
    let end = cursor.expect(&Token::Semicolon, "`;`").ok()?.hi;
    Some(MethodSig {
        name,
        generic_params,
        params,
        return_type,
        span: Span::new(cursor.file(), start.lo, end),
    })
}

/// A duplicate of the function-decl param-list parser, kept here
/// because the trait/interface paths need it without a body. The
/// implementation is intentionally tiny so duplicating it costs
/// less than threading a shared helper through the module graph.
fn parse_param_list(cursor: &mut Cursor<'_>) -> Option<Vec<Param>> {
    if matches!(cursor.peek_token(), Some(Token::RParen)) {
        return Some(Vec::new());
    }
    let first = parse_param(cursor)?;
    let mut params = vec![first];
    while matches!(cursor.peek_token(), Some(Token::Comma)) {
        cursor.advance();
        if matches!(cursor.peek_token(), Some(Token::RParen)) {
            break;
        }
        params.push(parse_param(cursor)?);
    }
    Some(params)
}

fn parse_param(cursor: &mut Cursor<'_>) -> Option<Param> {
    let lo = cursor.current_span().lo;
    let borrow = parse_borrow_mod(cursor);
    let _ = Borrow::None; // silence unused import
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

// ===== Trait =====

pub(crate) fn parse_trait_decl(cursor: &mut Cursor<'_>) -> Option<TraitDecl> {
    let start = cursor.current_span().lo;
    let visibility = if cursor.eat(&Token::Public).is_some() {
        Visibility::Public
    } else {
        Visibility::Default
    };
    cursor.expect(&Token::Trait, "`trait`").ok()?;
    let name = expect_ident(cursor, "trait name")?;
    let generic_params = parse_optional_generic_params(cursor)?;
    cursor
        .expect(&Token::LBrace, "`{` before trait body")
        .ok()?;
    let mut methods = Vec::new();
    while !matches!(cursor.peek_token(), Some(Token::RBrace) | None) {
        let before = cursor.pos();
        if let Some(m) = parse_function_decl(cursor) {
            methods.push(m);
        } else {
            recover_inside_class(cursor);
            if cursor.pos() == before {
                cursor.advance();
            }
        }
    }
    let close = cursor.expect(&Token::RBrace, "`}`").ok()?;
    Some(TraitDecl {
        visibility,
        name,
        generic_params,
        methods,
        span: Span::new(cursor.file(), start, close.hi),
    })
}

// ===== Test =====

pub(crate) fn parse_test_decl(cursor: &mut Cursor<'_>) -> Option<TestDecl> {
    let start = cursor.expect(&Token::Test, "`test`").ok()?;
    let spanned = cursor.peek()?.clone();
    let parts = match spanned.token {
        Token::StrLit(parts) => {
            cursor.advance();
            convert_str_parts(parts, cursor, spanned.span)
        }
        _ => {
            cursor.error(spanned.span, "expected string literal naming the test");
            return None;
        }
    };
    let body = parse_block(cursor)?;
    let end = body.span.hi;
    Some(TestDecl {
        name: parts,
        body,
        span: Span::new(cursor.file(), start.lo, end),
    })
}

fn convert_str_parts(
    parts: Vec<StringPart>,
    _cursor: &mut Cursor<'_>,
    _host_span: Span,
) -> Vec<StrPart> {
    parts
        .into_iter()
        .map(|p| match p {
            StringPart::Text(t) => StrPart::Text(t),
            // Test names with interpolation are unusual but the
            // grammar allows them; store the raw text so future
            // tooling can decide how to render.
            StringPart::Interp { body, .. } => StrPart::Text(format!("{{{body}}}")),
        })
        .collect()
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
        let toks: Vec<Spanned> = Lexer::new(src, FileId(0))
            .map(|r| r.expect("clean lex"))
            .collect();
        (toks, src.len() as u32, FileId(0))
    }

    fn parse_class(src: &str) -> ClassDecl {
        let (toks, len, file) = cursor_for(src);
        let mut cursor = Cursor::new(&toks, file, len);
        let decl = parse_class_decl(&mut cursor).expect("class");
        let diags = cursor.into_diagnostics();
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        decl
    }

    fn parse_enum(src: &str) -> EnumDecl {
        let (toks, len, file) = cursor_for(src);
        let mut cursor = Cursor::new(&toks, file, len);
        let decl = parse_enum_decl(&mut cursor).expect("enum");
        let diags = cursor.into_diagnostics();
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        decl
    }

    fn parse_interface(src: &str) -> InterfaceDecl {
        let (toks, len, file) = cursor_for(src);
        let mut cursor = Cursor::new(&toks, file, len);
        let decl = parse_interface_decl(&mut cursor).expect("interface");
        let diags = cursor.into_diagnostics();
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        decl
    }

    fn parse_trait(src: &str) -> TraitDecl {
        let (toks, len, file) = cursor_for(src);
        let mut cursor = Cursor::new(&toks, file, len);
        let decl = parse_trait_decl(&mut cursor).expect("trait");
        let diags = cursor.into_diagnostics();
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        decl
    }

    fn parse_test(src: &str) -> TestDecl {
        let (toks, len, file) = cursor_for(src);
        let mut cursor = Cursor::new(&toks, file, len);
        let decl = parse_test_decl(&mut cursor).expect("test");
        let diags = cursor.into_diagnostics();
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        decl
    }

    #[test]
    fn empty_class() {
        let c = parse_class("public class User {}");
        assert_eq!(c.visibility, Visibility::Public);
        assert_eq!(c.name.name, "User");
        assert!(c.implements.is_empty());
        assert!(c.members.is_empty());
    }

    #[test]
    fn class_with_implements_and_construct() {
        let c = parse_class(
            r#"public class User implements Greet {
                construct(public string $name, int $age) {
                    $this->createdAt := $age;
                }

                instant $createdAt;
                flip int $loginCount = 0;

                public function greet(): string {
                    return "Hi";
                }
            }"#,
        );
        assert_eq!(c.implements.len(), 1);
        assert_eq!(c.implements[0][0].name, "Greet");
        assert_eq!(c.members.len(), 4);
        assert!(matches!(c.members[0], ClassMember::Construct(_)));
        assert!(matches!(c.members[1], ClassMember::Field(_)));
        assert!(matches!(c.members[2], ClassMember::Field(_)));
        assert!(matches!(c.members[3], ClassMember::Method(_)));
        if let ClassMember::Construct(con) = &c.members[0] {
            assert_eq!(con.params.len(), 2);
            assert!(con.params[0].promoted);
            assert!(!con.params[1].promoted);
        }
    }

    #[test]
    fn class_trait_use() {
        let c = parse_class(
            r#"public class User {
                use Loggable;
            }"#,
        );
        assert!(matches!(c.members[0], ClassMember::TraitUse(_)));
    }

    #[test]
    fn field_with_default_and_hooks() {
        let c = parse_class(
            r#"public class User {
                int $age = 0 {
                    get => $age;
                    set(int $v) { $this->age := $v; }
                };
            }"#,
        );
        let ClassMember::Field(field) = &c.members[0] else {
            panic!("expected Field");
        };
        assert_eq!(field.hooks.len(), 2);
        assert!(matches!(field.hooks[0], PropertyHook::GetExpr { .. }));
        assert!(matches!(field.hooks[1], PropertyHook::Set { .. }));
        assert!(field.default.is_some());
    }

    #[test]
    fn enum_plain_variants() {
        let e = parse_enum("public enum Method { Get, Post, Put, Delete }");
        assert_eq!(e.variants.len(), 4);
        assert!(e.backing.is_none());
        assert!(e.variants.iter().all(|v| v.value.is_none()));
    }

    #[test]
    fn enum_with_backing_and_values() {
        let e = parse_enum("public enum Status: int { Ok = 200, NotFound = 404, }");
        assert_eq!(e.backing.as_ref().unwrap().path[0].name, "int");
        assert_eq!(e.variants.len(), 2);
        assert!(matches!(e.variants[0].value, Some(Expr::IntLit { .. })));
    }

    #[test]
    fn interface_with_method_sigs() {
        let i = parse_interface(
            "public interface Greet { function greet(): string; function bye(): void; }",
        );
        assert_eq!(i.methods.len(), 2);
        assert_eq!(i.methods[0].name.name, "greet");
        assert_eq!(i.methods[1].return_type.path[0].name, "void");
    }

    #[test]
    fn trait_with_function_bodies() {
        let t = parse_trait(
            r#"public trait Loggable {
                function log(): void { return; }
            }"#,
        );
        assert_eq!(t.methods.len(), 1);
        assert_eq!(t.methods[0].name.name, "log");
    }

    #[test]
    fn test_decl_records_name_and_body() {
        let t = parse_test(r#"test "addition is commutative" { return; }"#);
        assert!(matches!(t.name[0], StrPart::Text(_)));
        assert_eq!(t.body.statements.len(), 1);
    }
}
