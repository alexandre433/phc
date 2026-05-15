// SPDX-License-Identifier: MIT
//! Recursive-descent parser producing a typed PHC AST.
//!
//! Layered on top of [`phc_lexer`]: the parser consumes
//! `(Token, Span)` pairs and builds nodes from [`phc_ast`].
//! Errors are surfaced as [`phc_errors::Diagnostic`]; the parser
//! recovers when it can so a single source file can yield multiple
//! diagnostics in one pass.
//!
//! This file holds the shared parser machinery and the top-level
//! [`parse_source_file`] entry point. Productions are split into
//! sibling modules as they grow.

mod source_file;

#[cfg(test)]
mod tests;

use phc_ast::SourceFile;
use phc_errors::Diagnostic;
use phc_lexer::{Lexer, Spanned, Token};
use phc_span::{FileId, Span};

use source_file::parse_source_file;

/// Outcome of parsing a single source file.
///
/// Even when [`Self::file`] is `Some`, [`Self::diagnostics`] may be
/// non-empty: the parser recovers past local errors so consumers
/// (LSP, CLI) can show every problem in one pass instead of
/// stopping at the first one.
#[derive(Debug)]
pub struct ParseResult {
    pub file: Option<SourceFile>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parse a complete source file from raw text.
///
/// Convenience wrapper that drives the lexer and then
/// [`parse_source_file`]. Lex errors are added to the diagnostic
/// list before parsing begins.
pub fn parse(source: &str, file: FileId) -> ParseResult {
    let mut tokens: Vec<Spanned> = Vec::new();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    for item in Lexer::new(source, file) {
        match item {
            Ok(spanned) => tokens.push(spanned),
            Err(span) => diagnostics.push(lex_error(span)),
        }
    }
    let mut cursor = Cursor::new(&tokens, file, source.len() as u32);
    let parsed = parse_source_file(&mut cursor);
    diagnostics.extend(cursor.into_diagnostics());
    ParseResult {
        file: parsed,
        diagnostics,
    }
}

fn lex_error(span: Span) -> Diagnostic {
    Diagnostic {
        severity: phc_errors::Severity::Error,
        message: "lex error".to_string(),
        span,
    }
}

/// Cursor over a slice of [`Spanned`] tokens shared by every parser
/// production. Tracks position, accumulates diagnostics, and offers
/// the small set of look-ahead and consume primitives the recursive
/// productions actually need.
pub(crate) struct Cursor<'tok> {
    tokens: &'tok [Spanned],
    pos: usize,
    file: FileId,
    eof_offset: u32,
    diagnostics: Vec<Diagnostic>,
}

impl<'tok> Cursor<'tok> {
    pub(crate) fn new(tokens: &'tok [Spanned], file: FileId, source_len: u32) -> Self {
        Self {
            tokens,
            pos: 0,
            file,
            eof_offset: source_len,
            diagnostics: Vec::new(),
        }
    }

    pub(crate) fn peek(&self) -> Option<&'tok Spanned> {
        self.tokens.get(self.pos)
    }

    /// Peek `offset` tokens past the cursor. `peek_at(0) == peek()`.
    pub(crate) fn peek_at(&self, offset: usize) -> Option<&'tok Spanned> {
        self.tokens.get(self.pos + offset)
    }

    pub(crate) fn peek_token(&self) -> Option<&'tok Token> {
        self.peek().map(|s| &s.token)
    }

    pub(crate) fn advance(&mut self) -> Option<&'tok Spanned> {
        let item = self.tokens.get(self.pos)?;
        self.pos += 1;
        Some(item)
    }

    /// If the next token equals `expected`, consume it and return
    /// the span; otherwise leave the cursor untouched.
    pub(crate) fn eat(&mut self, expected: &Token) -> Option<Span> {
        match self.peek() {
            Some(s) if &s.token == expected => {
                let span = s.span;
                self.pos += 1;
                Some(span)
            }
            _ => None,
        }
    }

    /// Consume the next token if it matches `expected`. On mismatch,
    /// emit a diagnostic and leave the cursor where it is.
    pub(crate) fn expect(&mut self, expected: &Token, label: &str) -> Result<Span, ()> {
        match self.eat(expected) {
            Some(span) => Ok(span),
            None => {
                let span = self.current_span();
                self.error(span, format!("expected {label}"));
                Err(())
            }
        }
    }

    /// Span of the next token, or a zero-width span at EOF.
    pub(crate) fn current_span(&self) -> Span {
        match self.peek() {
            Some(s) => s.span,
            None => Span::new(self.file, self.eof_offset, self.eof_offset),
        }
    }

    pub(crate) fn error(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            severity: phc_errors::Severity::Error,
            message: message.into(),
            span,
        });
    }

    pub(crate) fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    pub(crate) fn file(&self) -> FileId {
        self.file
    }
}
