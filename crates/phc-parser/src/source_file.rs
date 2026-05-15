// SPDX-License-Identifier: MIT
//! Top-level source file production.
//!
//! Grammar (`spec/grammar.ebnf`):
//!
//! ```text
//! SourceFile = PackDecl { UseDecl } { Item }
//! PackDecl   = "pack" PackPath ";"
//! UseDecl    = "use" PackPath [ "." "{" ImportList "}" ] ";"
//! PackPath   = Identifier { "." Identifier }
//! ImportList = Identifier { "," Identifier }
//! ```
//!
//! `Item` is parsed by sibling modules; this commit accepts zero
//! items and reports a TODO if any are present.

use phc_ast::{Ident, Item, PackDecl, PackPath, SourceFile, UseDecl};
use phc_lexer::Token;
use phc_span::Span;

use crate::functions::parse_function_decl;
use crate::Cursor;

/// Parse a complete `SourceFile`. Returns `None` only when the pack
/// declaration is missing or malformed; partial recovery on later
/// constructs still yields a `Some` value with diagnostics attached
/// to the cursor.
pub(crate) fn parse_source_file(cursor: &mut Cursor<'_>) -> Option<SourceFile> {
    let pack = parse_pack_decl(cursor)?;
    let mut uses = Vec::new();
    while matches!(cursor.peek_token(), Some(Token::Use)) {
        if let Some(decl) = parse_use_decl(cursor) {
            uses.push(decl);
        } else {
            recover_to_next_top_level(cursor);
        }
    }
    let mut items: Vec<Item> = Vec::new();
    while cursor.peek().is_some() {
        let before = cursor.pos();
        if let Some(item) = parse_item(cursor) {
            items.push(item);
        } else {
            recover_to_next_top_level(cursor);
            // The recovery sync set includes the offending keyword
            // itself (e.g. `class`), so guarantee forward progress
            // by skipping one token whenever recovery did not move.
            if cursor.pos() == before {
                cursor.advance();
            }
        }
    }
    let end = cursor.current_span().hi;
    let span = Span::new(cursor.file(), pack.span.lo, end);
    Some(SourceFile {
        pack,
        uses,
        items,
        span,
    })
}

/// Dispatch on the next token to one of the item parsers.
///
/// Skips optional `public` and `async` modifiers without consuming
/// to find the keyword that determines the item kind. Today only
/// `FunctionDecl` is wired; class/enum/interface/trait/test land
/// in P6.
fn parse_item(cursor: &mut Cursor<'_>) -> Option<Item> {
    let mut offset = 0;
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
        Some(Token::Function) => parse_function_decl(cursor).map(Item::Function),
        _ => {
            let span = cursor.current_span();
            cursor.error(
                span,
                "expected `function`, `class`, `enum`, `interface`, `trait`, or `test`",
            );
            None
        }
    }
}

fn parse_pack_decl(cursor: &mut Cursor<'_>) -> Option<PackDecl> {
    let start = cursor.expect(&Token::Pack, "`pack` keyword").ok()?;
    let path = parse_pack_path(cursor)?;
    let end = cursor.expect(&Token::Semicolon, "`;`").ok()?;
    Some(PackDecl {
        path,
        span: Span::new(cursor.file(), start.lo, end.hi),
    })
}

fn parse_use_decl(cursor: &mut Cursor<'_>) -> Option<UseDecl> {
    let start = cursor.expect(&Token::Use, "`use` keyword").ok()?;
    let path = parse_pack_path(cursor)?;
    let group = if matches!(cursor.peek_token(), Some(Token::Dot)) {
        // Look one ahead: `.{` opens a grouped import; `.<ident>` is
        // a continuation of the path and should never reach here
        // because parse_pack_path already consumed all `Dot Ident`
        // sequences. So an isolated `.` here implies grouping.
        let dot_span = cursor.advance().expect("dot was peeked").span;
        if !matches!(cursor.peek_token(), Some(Token::LBrace)) {
            cursor.error(dot_span, "expected `{` after `.` in grouped import");
            return None;
        }
        cursor.advance(); // consume `{`
        let names = parse_import_list(cursor)?;
        cursor.expect(&Token::RBrace, "`}`").ok()?;
        Some(names)
    } else {
        None
    };
    let end = cursor.expect(&Token::Semicolon, "`;`").ok()?;
    Some(UseDecl {
        path,
        group,
        span: Span::new(cursor.file(), start.lo, end.hi),
    })
}

fn parse_pack_path(cursor: &mut Cursor<'_>) -> Option<PackPath> {
    let first = expect_ident(cursor, "pack-path identifier")?;
    let mut segments = vec![first];
    while matches!(cursor.peek_token(), Some(Token::Dot)) {
        // `Dot Ident` continues a path. `Dot LBrace` opens a grouped
        // import in `parse_use_decl`, so peek two tokens before
        // committing to the dot.
        let next = cursor.peek_at(1).map(|s| &s.token);
        if matches!(next, Some(Token::LBrace)) {
            break;
        }
        cursor.advance(); // consume `.`
        let seg = expect_ident(cursor, "identifier after `.`")?;
        segments.push(seg);
    }
    let lo = segments.first().map(|s| s.span.lo).expect("non-empty path");
    let hi = segments.last().map(|s| s.span.hi).expect("non-empty path");
    Some(PackPath {
        segments,
        span: Span::new(cursor.file(), lo, hi),
    })
}

fn parse_import_list(cursor: &mut Cursor<'_>) -> Option<Vec<Ident>> {
    let first = expect_ident(cursor, "import name")?;
    let mut names = vec![first];
    while matches!(cursor.peek_token(), Some(Token::Comma)) {
        cursor.advance();
        // Allow trailing comma before the closing brace.
        if matches!(cursor.peek_token(), Some(Token::RBrace)) {
            break;
        }
        let name = expect_ident(cursor, "import name")?;
        names.push(name);
    }
    Some(names)
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

/// Advance past one malformed declaration. Stops *before* the next
/// plausible top-level start (`use`, a declaration keyword) or EOF
/// so the outer loop can re-enter on the next valid construct.
/// Also stops just after an intervening `;` so a missing-keyword
/// case does not consume the rest of the file.
fn recover_to_next_top_level(cursor: &mut Cursor<'_>) {
    while let Some(spanned) = cursor.peek() {
        match &spanned.token {
            Token::Use
            | Token::Function
            | Token::Class
            | Token::Enum
            | Token::Interface
            | Token::Trait
            | Token::Test
            | Token::Public
            | Token::Async => return,
            Token::Semicolon => {
                cursor.advance();
                return;
            }
            _ => {
                cursor.advance();
            }
        }
    }
}
