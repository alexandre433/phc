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

/// One piece of a tokenised string literal.
///
/// A [`Token::StrLit`] holds an ordered sequence of these. `Text`
/// chunks have already had escape sequences and `{{` / `}}` brace
/// escapes resolved, so the parser sees plain UTF-8. `Interp` keeps
/// the raw expression text between matched `{` and `}`; the parser
/// re-enters the lexer on that text when it consumes the chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringPart {
    /// Literal text after escape processing.
    Text(String),
    /// Raw expression text between a matched `{` and `}` inside the
    /// string (D-017). Re-lexed by the parser when consumed; the
    /// `body_start` field carries the absolute source offset of the
    /// first body byte (just past the opening `{`) so the parser
    /// can shift the resulting AST spans back into the host file's
    /// coordinate system.
    Interp { body: String, body_start: u32 },
}

/// Lex a string literal body after the opening `"` has matched.
///
/// Per D-017 every double-quoted string interpolates. The callback
/// walks the remainder, processing standard escapes (`\"`, `\\`,
/// `\n`, `\t`, `\r`, `\0`), the brace escapes `{{` / `}}` (literal
/// `{` / `}`), and `{ ... }` interpolation expressions. Inside an
/// interpolation we count brace depth and step past any nested
/// double-quoted strings so the matching `}` is found correctly.
///
/// Supports `\u{HHHH}` unicode escapes via [`parse_unicode_escape`]
/// (1–6 hex digits, rejects surrogates and out-of-range scalars).
///
/// Returns the assembled [`StringPart`] sequence on success. On
/// error (unterminated string, unterminated interpolation, unknown
/// escape, malformed unicode escape, unmatched `}`), returns
/// `Err(())` so the lexer surfaces an error span and resumes after
/// the offending byte.
fn lex_string(lex: &mut logos::Lexer<Token>) -> Result<Vec<StringPart>, ()> {
    let remainder = lex.remainder();
    let bytes = remainder.as_bytes();
    let mut parts: Vec<StringPart> = Vec::new();
    let mut buf = String::new();
    let mut i: usize = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                if !buf.is_empty() {
                    parts.push(StringPart::Text(std::mem::take(&mut buf)));
                }
                lex.bump(i + 1);
                return Ok(parts);
            }
            b'\\' => {
                if i + 1 >= bytes.len() {
                    lex.bump(remainder.len());
                    return Err(());
                }
                let esc = match bytes[i + 1] {
                    b'"' => '"',
                    b'\\' => '\\',
                    b'n' => '\n',
                    b't' => '\t',
                    b'r' => '\r',
                    b'0' => '\0',
                    b'u' => match parse_unicode_escape(&bytes[i + 2..]) {
                        Some((ch, consumed)) => {
                            buf.push(ch);
                            i += 2 + consumed;
                            continue;
                        }
                        None => {
                            lex.bump(i + 2);
                            return Err(());
                        }
                    },
                    _ => {
                        lex.bump(i + 2);
                        return Err(());
                    }
                };
                buf.push(esc);
                i += 2;
            }
            b'{' if bytes.get(i + 1) == Some(&b'{') => {
                buf.push('{');
                i += 2;
            }
            b'}' if bytes.get(i + 1) == Some(&b'}') => {
                buf.push('}');
                i += 2;
            }
            b'{' => {
                if !buf.is_empty() {
                    parts.push(StringPart::Text(std::mem::take(&mut buf)));
                }
                let body_offset_in_remainder = i + 1;
                let consumed = match scan_interp_body(&remainder[body_offset_in_remainder..]) {
                    Some(n) => n,
                    None => {
                        lex.bump(remainder.len());
                        return Err(());
                    }
                };
                let body =
                    &remainder[body_offset_in_remainder..body_offset_in_remainder + consumed - 1];
                // host_start is offset of the opening `"` token; +1
                // skips past it into the body of the string literal.
                let host_start = lex.span().start as u32;
                let body_start = host_start + 1 + body_offset_in_remainder as u32;
                parts.push(StringPart::Interp {
                    body: body.to_string(),
                    body_start,
                });
                i = body_offset_in_remainder + consumed;
            }
            b'}' => {
                lex.bump(i + 1);
                return Err(());
            }
            _ => {
                let ch = remainder[i..]
                    .chars()
                    .next()
                    .expect("non-empty remainder yields a char");
                buf.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    lex.bump(remainder.len());
    Err(())
}

/// Parse a `\u{HHHH}` unicode escape starting just after the `\u`.
///
/// Accepts 1 to 6 hex digits, mirroring the Rust string-escape rules
/// (the only well-defined Unicode scalar values fit in 6 hex digits).
/// Returns the decoded `char` and the number of bytes consumed —
/// `{`, the digits, and the closing `}` — on success. Rejects empty
/// digit sequences, missing braces, non-hex characters, and code
/// points that are not valid Unicode scalars (surrogates and values
/// above `U+10FFFF`).
fn parse_unicode_escape(rest: &[u8]) -> Option<(char, usize)> {
    if rest.first() != Some(&b'{') {
        return None;
    }
    let mut value: u32 = 0;
    let mut digits: usize = 0;
    let mut i: usize = 1;
    while i < rest.len() {
        let b = rest[i];
        if b == b'}' {
            if digits == 0 {
                return None;
            }
            let ch = char::from_u32(value)?;
            return Some((ch, i + 1));
        }
        let d = match b {
            b'0'..=b'9' => (b - b'0') as u32,
            b'a'..=b'f' => (b - b'a' + 10) as u32,
            b'A'..=b'F' => (b - b'A' + 10) as u32,
            _ => return None,
        };
        digits += 1;
        if digits > 6 {
            return None;
        }
        value = (value << 4) | d;
        i += 1;
    }
    None
}

/// Step past the body of one `{ ... }` interpolation, returning the
/// number of bytes consumed including the closing `}`.
///
/// Counts nested `{` `}` pairs so an interpolation can itself wrap
/// a block-form expression, and skips past any nested double-quoted
/// strings so a `}` inside such a string is not treated as the
/// closer. Returns `None` on EOF before the matching `}`.
fn scan_interp_body(remainder: &str) -> Option<usize> {
    let bytes = remainder.as_bytes();
    let mut depth: usize = 1;
    let mut i: usize = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i += 1;
                while i < bytes.len() {
                    match bytes[i] {
                        b'\\' if i + 1 < bytes.len() => i += 2,
                        b'"' => {
                            i += 1;
                            break;
                        }
                        _ => {
                            let ch = remainder[i..].chars().next()?;
                            i += ch.len_utf8();
                        }
                    }
                }
            }
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {
                let ch = remainder[i..].chars().next()?;
                i += ch.len_utf8();
            }
        }
    }
    None
}

/// Logos callback for `/* ... */` block comments.
///
/// Block comments nest (spec §1.2). Logos has no native nesting, so
/// we open with the literal `/*` token and let this callback consume
/// the body — counting depth — until the matching `*/`. On success we
/// emit `Token::BlockComment(text)` so the formatter (D-035) can
/// round-trip the comment; the parser cursor filters comment
/// tokens before they reach grammar productions. The returned
/// string includes the surrounding `/* ... */` delimiters.
fn lex_block_comment(lex: &mut logos::Lexer<Token>) -> FilterResult<String, ()> {
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
                    let body = &remainder[..i];
                    // The leading "/*" was consumed by the token
                    // matcher itself; prepend it back so the
                    // returned text round-trips verbatim.
                    let mut text = String::with_capacity(body.len() + 2);
                    text.push_str("/*");
                    text.push_str(body);
                    return FilterResult::Emit(text);
                }
            }
            _ => i += 1,
        }
    }
    lex.bump(remainder.len());
    FilterResult::Error(())
}

/// Read a `// ...` line comment body. The `//` itself was consumed
/// by the token matcher; we slurp the remainder of the line and
/// emit the full text (including `//`) as the token payload.
fn lex_line_comment(lex: &mut logos::Lexer<Token>) -> String {
    let remainder = lex.remainder();
    let end = remainder.find('\n').unwrap_or(remainder.len());
    lex.bump(end);
    let mut text = String::with_capacity(end + 2);
    text.push_str("//");
    text.push_str(&remainder[..end]);
    text
}

/// A single lexical token in PHC source.
///
/// Variants that carry data (identifier text, literal value) keep
/// the original source slice; the parser is responsible for any
/// further interpretation (e.g. parsing integer literals into `i64`).
#[derive(Logos, Debug, Clone, PartialEq, Eq)]
#[logos(skip r"[ \t\r\n\f]+")]
pub enum Token {
    // ----- Comments -----
    /// `/* ... */` block comment. Nesting allowed (spec §1.2). The
    /// formatter (D-035) round-trips comments verbatim; the parser
    /// cursor filters comment tokens before grammar productions
    /// see them, so adding them to the stream is transparent for
    /// every other consumer.
    #[token("/*", lex_block_comment)]
    BlockComment(String),
    /// `// ...` line comment (excludes the trailing newline). Same
    /// round-trip / filtering rules as `BlockComment`.
    #[token("//", lex_line_comment)]
    LineComment(String),

    // ----- String literal -----
    /// Double-quoted string literal, fully tokenised per D-017.
    ///
    /// The body is split into a sequence of [`StringPart`]s with
    /// escapes resolved and `{ ... }` interpolation expressions
    /// captured as raw text. The parser re-lexes each interpolation
    /// body when it consumes the chunk.
    #[token("\"", lex_string)]
    StrLit(Vec<StringPart>),

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
    /// `fn` keyword — heads a function-type written `fn(T, U): R`
    /// (D-024). Distinct from the `function` declaration keyword
    /// to keep declarations and type references easy to skim apart.
    #[token("fn")]
    Fn,
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
    fn line_comments_become_line_comment_tokens() {
        // Updated 2026-05-16 for D-035: comments are no longer
        // dropped — they ride in the token stream so the formatter
        // can round-trip them. The parser filters them out at the
        // boundary; downstream grammar productions still don't see
        // them.
        let src = "function // this is preserved\nreturn";
        let toks = lex(src);
        assert_eq!(toks.len(), 3);
        assert_eq!(toks[0], Token::Function);
        assert!(matches!(&toks[1], Token::LineComment(t) if t == "// this is preserved"));
        assert_eq!(toks[2], Token::Return);
    }

    #[test]
    fn block_comments_become_block_comment_tokens() {
        let src = "function /* anything in here */ return";
        let toks = lex(src);
        assert_eq!(toks.len(), 3);
        assert_eq!(toks[0], Token::Function);
        assert!(matches!(&toks[1], Token::BlockComment(t) if t == "/* anything in here */"));
        assert_eq!(toks[2], Token::Return);
    }

    #[test]
    fn block_comments_nest_and_preserve_inner_text() {
        // Spec §1.2: block comments nest. The middle */ must NOT
        // close the outer comment; the captured text includes the
        // nested chunk verbatim.
        let src = "function /* outer /* inner */ still outer */ return";
        let toks = lex(src);
        assert_eq!(toks.len(), 3);
        assert_eq!(toks[0], Token::Function);
        assert!(matches!(
            &toks[1],
            Token::BlockComment(t) if t == "/* outer /* inner */ still outer */"
        ));
        assert_eq!(toks[2], Token::Return);
    }

    #[test]
    fn block_comment_can_span_multiple_lines() {
        let src = "function /* line one\n   line two\n   line three */ return";
        let toks = lex(src);
        assert_eq!(toks.len(), 3);
        assert_eq!(toks[0], Token::Function);
        assert!(matches!(
            &toks[1],
            Token::BlockComment(t) if t.contains("line one") && t.contains("line three")
        ));
        assert_eq!(toks[2], Token::Return);
    }

    fn parts_of(token: Token) -> Vec<StringPart> {
        match token {
            Token::StrLit(parts) => parts,
            other => panic!("expected StrLit, got {other:?}"),
        }
    }

    #[test]
    fn empty_string_yields_no_parts() {
        let toks = lex(r#""""#);
        assert_eq!(toks.len(), 1);
        assert!(parts_of(toks.into_iter().next().unwrap()).is_empty());
    }

    #[test]
    fn plain_string_is_one_text_chunk() {
        let toks = lex(r#""hello""#);
        assert_eq!(
            parts_of(toks.into_iter().next().unwrap()),
            vec![StringPart::Text("hello".into())]
        );
    }

    #[test]
    fn escape_sequences_are_resolved() {
        let toks = lex(r#""a\"b\\c\nd\te\rf\0g""#);
        assert_eq!(
            parts_of(toks.into_iter().next().unwrap()),
            vec![StringPart::Text("a\"b\\c\nd\te\rf\0g".into())]
        );
    }

    #[test]
    fn unicode_escape_decodes_short_form() {
        // U+0041 = 'A'
        let toks = lex(r#""\u{41}""#);
        assert_eq!(
            parts_of(toks.into_iter().next().unwrap()),
            vec![StringPart::Text("A".into())]
        );
    }

    #[test]
    fn unicode_escape_decodes_full_six_hex() {
        // U+1F600 = 😀 (six hex digits, outside the BMP)
        let toks = lex(r#""hi \u{01F600}""#);
        assert_eq!(
            parts_of(toks.into_iter().next().unwrap()),
            vec![StringPart::Text("hi 😀".into())]
        );
    }

    #[test]
    fn unicode_escape_accepts_mixed_case_hex() {
        let toks = lex(r#""\u{aB}""#);
        assert_eq!(
            parts_of(toks.into_iter().next().unwrap()),
            vec![StringPart::Text("\u{ab}".into())]
        );
    }

    #[test]
    fn unicode_escape_with_no_digits_is_an_error() {
        let mut lexer = Lexer::new(r#""\u{}""#, FileId(0));
        assert!(lexer.next().unwrap().is_err());
    }

    #[test]
    fn unicode_escape_with_surrogate_is_an_error() {
        // U+D800 is a surrogate code point — not a valid scalar value.
        let mut lexer = Lexer::new(r#""\u{D800}""#, FileId(0));
        assert!(lexer.next().unwrap().is_err());
    }

    #[test]
    fn unicode_escape_above_max_codepoint_is_an_error() {
        // U+110000 is one past the maximum valid scalar.
        let mut lexer = Lexer::new(r#""\u{110000}""#, FileId(0));
        assert!(lexer.next().unwrap().is_err());
    }

    #[test]
    fn unicode_escape_missing_closing_brace_is_an_error() {
        let mut lexer = Lexer::new(r#""\u{41"#, FileId(0));
        assert!(lexer.next().unwrap().is_err());
    }

    #[test]
    fn double_brace_is_a_literal_brace() {
        let toks = lex(r#""open {{ close }}""#);
        assert_eq!(
            parts_of(toks.into_iter().next().unwrap()),
            vec![StringPart::Text("open { close }".into())]
        );
    }

    #[test]
    fn single_interpolation_splits_into_three_parts() {
        // Source: `"hi {$user->name}!"` — outer `"` at byte 0, body
        // of the only interp starts at byte 5 (just past the `{`).
        let toks = lex(r#""hi {$user->name}!""#);
        assert_eq!(
            parts_of(toks.into_iter().next().unwrap()),
            vec![
                StringPart::Text("hi ".into()),
                StringPart::Interp {
                    body: "$user->name".into(),
                    body_start: 5,
                },
                StringPart::Text("!".into()),
            ]
        );
    }

    #[test]
    fn interpolation_at_string_start_has_no_leading_text() {
        let toks = lex(r#""{$x} trailing""#);
        assert_eq!(
            parts_of(toks.into_iter().next().unwrap()),
            vec![
                StringPart::Interp {
                    body: "$x".into(),
                    body_start: 2,
                },
                StringPart::Text(" trailing".into()),
            ]
        );
    }

    #[test]
    fn interpolation_can_contain_nested_braces() {
        let toks = lex(r#""{ match($x) { _ => 1 } }""#);
        assert_eq!(
            parts_of(toks.into_iter().next().unwrap()),
            vec![StringPart::Interp {
                body: " match($x) { _ => 1 } ".into(),
                body_start: 2,
            }]
        );
    }

    #[test]
    fn interpolation_can_contain_a_nested_string() {
        let toks = lex(r#""{$name ?? "anon"}""#);
        assert_eq!(
            parts_of(toks.into_iter().next().unwrap()),
            vec![StringPart::Interp {
                body: r#"$name ?? "anon""#.into(),
                body_start: 2,
            }]
        );
    }

    #[test]
    fn unterminated_string_is_an_error() {
        let mut lexer = Lexer::new(r#""never closed"#, FileId(1));
        let err = lexer.next().unwrap().unwrap_err();
        assert_eq!(err, Span::new(FileId(1), 0, 13));
        assert!(lexer.next().is_none());
    }

    #[test]
    fn unterminated_interpolation_is_an_error() {
        let mut lexer = Lexer::new(r#""hello {$x"#, FileId(1));
        let err = lexer.next().unwrap().unwrap_err();
        assert_eq!(err, Span::new(FileId(1), 0, 10));
    }

    #[test]
    fn unmatched_close_brace_in_string_is_an_error() {
        let mut lexer = Lexer::new(r#""oops } here""#, FileId(1));
        assert!(lexer.next().unwrap().is_err());
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
