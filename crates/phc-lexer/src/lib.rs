// SPDX-License-Identifier: MIT
//! Tokeniser for PHC source files.
//!
//! Produces a flat stream of `(Token, Span)` pairs. Higher-level
//! constructs — string interpolation, nested block comments, raw
//! strings — are layered on top in follow-up commits. See
//! `spec/keywords.md` and `spec/operators.md` for the canonical
//! vocabulary this module implements.

use logos::{FilterResult, Logos};
use phc_span::{FileId, Span};

/// Logos callback for `/* ... */` block comments.
///
/// Block comments nest (spec §1.2). Logos has no native nesting, so
/// we open with the literal `/*` token and let this callback consume
/// the body — counting depth — until the matching `*/`. On success we
/// `Skip` so the comment never appears in the token stream; on EOF
/// before close, we `SkipErr` so the iterator surfaces an error span.
fn skip_block_comment(lex: &mut logos::Lexer<Token>) -> FilterResult<(), ()> {
    let remainder = lex.remainder();
    let bytes = remainder.as_bytes();
    let mut depth: usize = 1;
    let mut i: usize = 0;
    while i + 1 < bytes.len() {
        match (bytes[i], bytes[i + 1]) {
            (b'/', b'*') => {
                depth += 1;
                i += 2;
            }
            (b'*', b'/') => {
                depth -= 1;
                i += 2;
                if depth == 0 {
                    lex.bump(i);
                    return FilterResult::Skip;
                }
            }
            _ => i += 1,
        }
    }
    lex.bump(remainder.len());
    FilterResult::Error(())
}

/// A single lexical token in PHC source.
///
/// Variants that carry data (identifier text, literal value) keep
/// the original source slice; the parser is responsible for any
/// further interpretation (e.g. parsing integer literals into `i64`).
#[derive(Logos, Debug, Clone, PartialEq, Eq)]
#[logos(skip r"[ \t\r\n\f]+")]
#[logos(skip r"//[^\n]*")]
pub enum Token {
    // ----- Comments -----
    /// `/* ... */` block comment. Nesting allowed (spec §1.2).
    /// Always skipped via [`skip_block_comment`]; never observed by
    /// downstream consumers.
    #[token("/*", skip_block_comment)]
    BlockComment,

    // ----- Hard keywords (spec/keywords.md) -----
    #[token("flip")]
    Flip,
    #[token("async")]
    Async,
    #[token("await")]
    Await,
    #[token("dyn")]
    Dyn,
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("null")]
    Null,
    #[token("public")]
    Public,
    #[token("function")]
    Function,
    #[token("return")]
    Return,
    #[token("void")]
    Void,
    #[token("pack")]
    Pack,
    #[token("use")]
    Use,
    #[token("class")]
    Class,
    #[token("construct")]
    Construct,
    #[token("enum")]
    Enum,
    #[token("interface")]
    Interface,
    #[token("trait")]
    Trait,
    #[token("implements")]
    Implements,
    #[token("match")]
    Match,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("while")]
    While,
    #[token("for")]
    For,
    #[token("in")]
    In,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("as")]
    As,
    #[token("panic")]
    Panic,
    #[token("test")]
    Test,

    // ----- Punctuation -----
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token(",")]
    Comma,
    #[token(";")]
    Semicolon,
    #[token(":")]
    Colon,

    // ----- D-023 sigils + access operators -----
    /// Variable / parameter / field sigil; precedes every `$name`.
    #[token("$")]
    Dollar,
    /// Instance member access.
    #[token("->")]
    Arrow,
    /// Static / type-level access.
    #[token("::")]
    StaticOp,
    /// Path separator (pack paths, namespaced types only).
    #[token(".")]
    Dot,

    // ----- Operators -----
    /// `:=` reassignment to a `flip` binding (D-005). Statement-only.
    #[token(":=")]
    Reassign,
    /// `=` initialiser in declarations.
    #[token("=")]
    Eq,
    /// `=>` lambda / match-arm arrow.
    #[token("=>")]
    FatArrow,
    /// `?` nullable type suffix (D-006).
    #[token("?")]
    Question,
    /// `&` shared-borrow prefix. `&flip` is lexed as two tokens.
    #[token("&")]
    Amp,
    /// `+` plus / string concat / bound combinator.
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("<")]
    Lt,
    #[token("<=")]
    LtEq,
    #[token(">")]
    Gt,
    #[token(">=")]
    GtEq,
    #[token("==")]
    EqEq,
    #[token("!=")]
    NotEq,
    #[token("!")]
    Bang,
    #[token("&&")]
    AmpAmp,
    #[token("||")]
    PipePipe,
    /// `??` null-coalesce.
    #[token("??")]
    QQuestion,
    /// `|` OR-pattern separator inside `match` arms.
    #[token("|")]
    Pipe,

    // ----- Literals -----
    /// Integer literal text, exactly as it appears in source.
    /// Underscore digit separators are kept; the parser strips them.
    #[regex(r"[0-9][0-9_]*", |lex| lex.slice().to_string())]
    IntLit(String),
    /// Float literal text, exactly as it appears in source.
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*", |lex| lex.slice().to_string())]
    FloatLit(String),
    /// Bare identifier (no leading sigil). Reserved words match above
    /// and never hit this rule. The lone `_` is also lexed as an
    /// identifier; the parser treats `Ident("_")` as the wildcard
    /// pattern when it appears in a `match` arm (spec §8).
    #[regex(r"[A-Za-z_][A-Za-z0-9_]*", |lex| lex.slice().to_string(), priority = 2)]
    Ident(String),
}

/// A spanned token: the token plus its byte range in source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned {
    pub token: Token,
    pub span: Span,
}

/// Lexer iterator over PHC source.
///
/// Wraps `logos::Lexer` and stamps every token with a [`Span`] tied
/// to the originating [`FileId`]. Lexical errors are surfaced as
/// `Err(Span)` and the iterator continues past the offending byte
/// so the parser can collect multiple diagnostics per file.
pub struct Lexer<'src> {
    inner: logos::Lexer<'src, Token>,
    file: FileId,
}

impl<'src> Lexer<'src> {
    /// Build a new lexer for `source`, tagged with `file`.
    pub fn new(source: &'src str, file: FileId) -> Self {
        Self {
            inner: Token::lexer(source),
            file,
        }
    }
}

impl Iterator for Lexer<'_> {
    type Item = Result<Spanned, Span>;

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.inner.next()?;
        let range = self.inner.span();
        let span = Span::new(self.file, range.start as u32, range.end as u32);
        Some(match token {
            Ok(t) => Ok(Spanned { token: t, span }),
            Err(_) => Err(span),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(source: &str) -> Vec<Token> {
        Lexer::new(source, FileId(0))
            .map(|r| r.expect("unexpected lex error in test"))
            .map(|s| s.token)
            .collect()
    }

    #[test]
    fn keywords_round_trip() {
        let src = "flip async await dyn true false null public function return void \
                   pack use class construct enum interface trait implements match \
                   if else while for in break continue as panic test";
        let toks = lex(src);
        assert_eq!(
            toks,
            vec![
                Token::Flip,
                Token::Async,
                Token::Await,
                Token::Dyn,
                Token::True,
                Token::False,
                Token::Null,
                Token::Public,
                Token::Function,
                Token::Return,
                Token::Void,
                Token::Pack,
                Token::Use,
                Token::Class,
                Token::Construct,
                Token::Enum,
                Token::Interface,
                Token::Trait,
                Token::Implements,
                Token::Match,
                Token::If,
                Token::Else,
                Token::While,
                Token::For,
                Token::In,
                Token::Break,
                Token::Continue,
                Token::As,
                Token::Panic,
                Token::Test,
            ]
        );
    }

    #[test]
    fn punctuation_and_access_operators() {
        let src = "{ } ( ) [ ] , ; : $ -> :: .";
        assert_eq!(
            lex(src),
            vec![
                Token::LBrace,
                Token::RBrace,
                Token::LParen,
                Token::RParen,
                Token::LBracket,
                Token::RBracket,
                Token::Comma,
                Token::Semicolon,
                Token::Colon,
                Token::Dollar,
                Token::Arrow,
                Token::StaticOp,
                Token::Dot,
            ]
        );
    }

    #[test]
    fn operators_cover_precedence_table() {
        let src = ":= = => ? & + - * / % < <= > >= == != ! && || ?? |";
        assert_eq!(
            lex(src),
            vec![
                Token::Reassign,
                Token::Eq,
                Token::FatArrow,
                Token::Question,
                Token::Amp,
                Token::Plus,
                Token::Minus,
                Token::Star,
                Token::Slash,
                Token::Percent,
                Token::Lt,
                Token::LtEq,
                Token::Gt,
                Token::GtEq,
                Token::EqEq,
                Token::NotEq,
                Token::Bang,
                Token::AmpAmp,
                Token::PipePipe,
                Token::QQuestion,
                Token::Pipe,
            ]
        );
    }

    #[test]
    fn underscore_lexes_as_identifier() {
        // The lone `_` is a valid identifier; the parser interprets it
        // as the wildcard pattern when it appears in a match arm.
        assert_eq!(lex("_"), vec![Token::Ident("_".into())]);
    }

    #[test]
    fn mut_borrow_is_two_tokens() {
        // D-005 / spec/operators.md: `&flip` is `&` then `flip`,
        // kept distinct so the parser sees both at expression level.
        assert_eq!(lex("&flip"), vec![Token::Amp, Token::Flip]);
    }

    #[test]
    fn int_and_float_literals_keep_underscores() {
        assert_eq!(
            lex("0 42 1_000_000 3.14 1_000.000_1"),
            vec![
                Token::IntLit("0".into()),
                Token::IntLit("42".into()),
                Token::IntLit("1_000_000".into()),
                Token::FloatLit("3.14".into()),
                Token::FloatLit("1_000.000_1".into()),
            ]
        );
    }

    #[test]
    fn identifiers_do_not_clash_with_keywords() {
        // Keywords win against `Ident`; everything else falls through.
        assert_eq!(
            lex("foo Bar baz_qux _under MAX42"),
            vec![
                Token::Ident("foo".into()),
                Token::Ident("Bar".into()),
                Token::Ident("baz_qux".into()),
                Token::Ident("_under".into()),
                Token::Ident("MAX42".into()),
            ]
        );
    }

    #[test]
    fn line_comments_are_skipped() {
        let src = "function // this is ignored\nreturn";
        assert_eq!(lex(src), vec![Token::Function, Token::Return]);
    }

    #[test]
    fn block_comments_are_skipped() {
        let src = "function /* anything in here */ return";
        assert_eq!(lex(src), vec![Token::Function, Token::Return]);
    }

    #[test]
    fn block_comments_nest() {
        // Spec §1.2: block comments nest. The middle */ must NOT
        // close the outer comment.
        let src = "function /* outer /* inner */ still outer */ return";
        assert_eq!(lex(src), vec![Token::Function, Token::Return]);
    }

    #[test]
    fn block_comment_can_span_multiple_lines() {
        let src = "function /* line one\n   line two\n   line three */ return";
        assert_eq!(lex(src), vec![Token::Function, Token::Return]);
    }

    #[test]
    fn unterminated_block_comment_is_an_error() {
        let mut lexer = Lexer::new("function /* never closed", FileId(3));
        assert!(matches!(
            lexer.next(),
            Some(Ok(Spanned {
                token: Token::Function,
                ..
            }))
        ));
        let err = lexer.next().unwrap().unwrap_err();
        // Error span starts at the `/*` and runs to EOF.
        assert_eq!(err, Span::new(FileId(3), 9, 24));
        assert!(lexer.next().is_none());
    }

    #[test]
    fn dollar_then_identifier_for_variable_ref() {
        // D-023: $name is two tokens ($ + Ident). The parser fuses.
        assert_eq!(
            lex("$count"),
            vec![Token::Dollar, Token::Ident("count".into())]
        );
    }

    #[test]
    fn span_covers_token_bytes() {
        let mut lexer = Lexer::new("flip int", FileId(7));
        let first = lexer.next().unwrap().unwrap();
        assert_eq!(first.token, Token::Flip);
        assert_eq!(first.span, Span::new(FileId(7), 0, 4));
        let second = lexer.next().unwrap().unwrap();
        assert_eq!(second.token, Token::Ident("int".into()));
        assert_eq!(second.span, Span::new(FileId(7), 5, 8));
    }

    #[test]
    fn unknown_byte_yields_error_span_then_resumes() {
        // The `@` is not part of the v0 lexical vocabulary.
        let mut lexer = Lexer::new("function @ return", FileId(0));
        assert!(matches!(
            lexer.next(),
            Some(Ok(Spanned {
                token: Token::Function,
                ..
            }))
        ));
        let err = lexer.next().unwrap().unwrap_err();
        assert_eq!(err, Span::new(FileId(0), 9, 10));
        assert!(matches!(
            lexer.next(),
            Some(Ok(Spanned {
                token: Token::Return,
                ..
            }))
        ));
    }
}
