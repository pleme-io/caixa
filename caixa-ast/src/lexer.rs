//! Lisp lexer — scans source into tokens with byte spans.
//!
//! Implementation: thin wrapper over [`logos`](https://docs.rs/logos)
//! 0.14. The hand-rolled byte-level lexer that lived here previously
//! shipped two latent bugs (UTF-8 mishandling, unterminated-string
//! detection) and was not maintainable as the syntax grew. logos
//! delegates regex/UTF-8 to its DFA engine and exposes byte spans
//! directly, so this file shrinks to atoms + a few callbacks while
//! getting strictly better correctness.
//!
//! Token alphabet (unchanged — parser.rs needs no edits):
//!   - `(` `)` — list delimiters
//!   - `'` `` ` `` `,` `,@` — reader macros
//!   - `"…"` — strings, with `\"` `\\` `\n` `\t` `\r` escapes
//!   - `#t` / `#f` — booleans
//!   - `nil` — the nil atom
//!   - integers / floats with optional sign
//!   - `:name-like` — keywords
//!   - `; …` — line comments
//!   - `\n+` (with surrounding spaces/`\r`/`\t`) — newline runs (carries
//!     the line count so the parser can decide blank-line trivia)
//!   - ` `/`\t` — whitespace (no count needed)
//!   - everything else is a symbol

use std::num::{ParseFloatError, ParseIntError};

use logos::{Lexer, Logos};
use thiserror::Error;

use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    LParen,
    RParen,
    Quote,
    Quasiquote,
    Unquote,
    UnquoteSplice,
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Nil,
    Symbol(String),
    Keyword(String),
    LineComment(String),
    Newlines(u32),
    Whitespace,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Default, Error, PartialEq, Eq, Clone)]
pub enum LexError {
    #[default]
    #[error("unrecognized token")]
    Unrecognized,
    #[error("unterminated string at offset {0}")]
    UnterminatedString(u32),
    #[error("invalid escape sequence \\{1} at offset {0}")]
    BadEscape(u32, char),
    #[error("invalid number literal at offset {0}: {1}")]
    BadInt(u32, String),
    #[error("invalid float literal at offset {0}: {1}")]
    BadFloat(u32, String),
    #[error("unexpected character {1:?} at offset {0}")]
    UnexpectedChar(u32, char),
}

impl From<(u32, ParseIntError)> for LexError {
    fn from(v: (u32, ParseIntError)) -> Self {
        Self::BadInt(v.0, v.1.to_string())
    }
}

impl From<(u32, ParseFloatError)> for LexError {
    fn from(v: (u32, ParseFloatError)) -> Self {
        Self::BadFloat(v.0, v.1.to_string())
    }
}

// ── logos token enum ──────────────────────────────────────────────
//
// Internal to the module. We translate to the public `TokenKind` /
// `Token` types in `tokenize` so the parser keeps its existing API.

#[derive(Logos, Debug, PartialEq)]
#[logos(error = LexError)]
enum LogosKind {
    #[token("(")]
    LParen,

    #[token(")")]
    RParen,

    #[token("'")]
    Quote,

    #[token("`")]
    Quasiquote,

    // `,@` MUST come before `,` so it wins on the longest-match.
    #[token(",@")]
    UnquoteSplice,

    #[token(",")]
    Unquote,

    #[token("#t", |_| true)]
    #[token("#f", |_| false)]
    Bool(bool),

    // Strings: opening `"`, then repeated non-`\`/non-`"` chars OR
    // backslash-something escapes, then closing `"`. The callback
    // unescapes the body. UTF-8 is delegated to logos / regex.
    #[regex(r#""(?:[^"\\]|\\.)*""#, lex_string_body)]
    Str(String),

    // Numbers: integer first (priority 3 so it doesn't lose to symbol).
    // Float separately — has a `.` or `e/E`.
    #[regex(r"[+-]?[0-9]+", priority = 3, callback = parse_int)]
    Int(i64),

    #[regex(
        r"[+-]?(?:[0-9]+\.[0-9]*|\.[0-9]+|[0-9]+[eE][+-]?[0-9]+|[0-9]+\.[0-9]*[eE][+-]?[0-9]+|\.[0-9]+[eE][+-]?[0-9]+)",
        priority = 3,
        callback = parse_float
    )]
    Float(f64),

    // Keyword: `:` followed by atom chars.
    #[regex(":[^\\s()'`,\";]+", |lex| lex.slice()[1..].to_string())]
    Keyword(String),

    // Line comment: `;` to end of line. The leading `;` is NOT
    // included in the captured body, matching the prior behavior.
    #[regex(r";[^\n]*", |lex| {
        let s = lex.slice();
        // strip the leading ';'
        s[1..].to_string()
    })]
    LineComment(String),

    // Newline runs: any \n followed by whitespace including more \n's.
    // The callback counts \n bytes so blank-line detection works
    // exactly as before (count >= 2 means a blank line).
    #[regex(r"[\n][ \t\r\n]*", count_newlines)]
    Newlines(u32),

    // Pure-space whitespace (no newline). Intentional and separate
    // from Newlines so the parser can skip both without losing
    // line-count info.
    #[regex(r"[ \t\r]+")]
    Whitespace,

    // Anything else is a symbol or `nil`. The atom-terminator set
    // matches the prior is_atom_terminator (space/tab/cr/lf/parens/
    // single-quote/backtick/comma/double-quote/semicolon) PLUS `#`,
    // which is the boolean / reader-macro dispatch prefix and never
    // appears inside a tatara-lisp symbol. Excluding `#` here lets
    // adjacent forms like `#t#f` tokenize as two booleans rather
    // than a single `#t#f` symbol.
    #[regex("[^\\s()'`,\";#][^\\s()'`,\";#]*", |lex| lex.slice().to_string())]
    Symbol(String),
}

// ── callbacks ─────────────────────────────────────────────────────

fn lex_string_body(lex: &mut Lexer<LogosKind>) -> Result<String, LexError> {
    let raw = lex.slice();
    debug_assert!(raw.starts_with('"') && raw.ends_with('"'));
    let inner = &raw[1..raw.len() - 1];
    let span_start = u32::try_from(lex.span().start).unwrap_or(u32::MAX);

    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.char_indices();
    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some((_, 'n')) => out.push('\n'),
                Some((_, 't')) => out.push('\t'),
                Some((_, 'r')) => out.push('\r'),
                Some((_, '"')) => out.push('"'),
                Some((_, '\\')) => out.push('\\'),
                Some((_, other)) => {
                    return Err(LexError::BadEscape(
                        span_start + 1 + u32::try_from(i).unwrap_or(0),
                        other,
                    ));
                }
                None => {
                    return Err(LexError::BadEscape(
                        span_start + 1 + u32::try_from(i).unwrap_or(0),
                        '\\',
                    ));
                }
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

fn parse_int(lex: &mut Lexer<LogosKind>) -> Result<i64, LexError> {
    let span_start = u32::try_from(lex.span().start).unwrap_or(u32::MAX);
    lex.slice()
        .parse::<i64>()
        .map_err(|e| LexError::BadInt(span_start, e.to_string()))
}

fn parse_float(lex: &mut Lexer<LogosKind>) -> Result<f64, LexError> {
    let span_start = u32::try_from(lex.span().start).unwrap_or(u32::MAX);
    lex.slice()
        .parse::<f64>()
        .map_err(|e| LexError::BadFloat(span_start, e.to_string()))
}

fn count_newlines(lex: &mut Lexer<LogosKind>) -> u32 {
    let s = lex.slice();
    let n = s.bytes().filter(|&b| b == b'\n').count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

// ── public entry point ────────────────────────────────────────────

/// Scan a source string into tokens. Trivia (whitespace, comments) is
/// preserved — the parser filters what it doesn't need.
pub fn tokenize(src: &str) -> Result<Vec<Token>, LexError> {
    let mut out = Vec::new();
    let mut lex = LogosKind::lexer(src);

    while let Some(result) = lex.next() {
        let span = lex.span();
        let span_start = u32::try_from(span.start).unwrap_or(u32::MAX);
        let span_end = u32::try_from(span.end).unwrap_or(u32::MAX);
        let span = Span::new(span_start, span_end);

        match result {
            Ok(kind) => {
                let public = match kind {
                    LogosKind::LParen => TokenKind::LParen,
                    LogosKind::RParen => TokenKind::RParen,
                    LogosKind::Quote => TokenKind::Quote,
                    LogosKind::Quasiquote => TokenKind::Quasiquote,
                    LogosKind::Unquote => TokenKind::Unquote,
                    LogosKind::UnquoteSplice => TokenKind::UnquoteSplice,
                    LogosKind::Bool(b) => TokenKind::Bool(b),
                    LogosKind::Str(s) => TokenKind::Str(s),
                    LogosKind::Int(i) => TokenKind::Int(i),
                    LogosKind::Float(f) => TokenKind::Float(f),
                    LogosKind::Keyword(s) => TokenKind::Keyword(s),
                    LogosKind::LineComment(s) => TokenKind::LineComment(s),
                    LogosKind::Newlines(n) => TokenKind::Newlines(n),
                    LogosKind::Whitespace => TokenKind::Whitespace,
                    LogosKind::Symbol(s) => {
                        if s == "nil" {
                            TokenKind::Nil
                        } else {
                            TokenKind::Symbol(s)
                        }
                    }
                };
                out.push(Token { kind: public, span });
            }
            Err(_) => {
                // Unrecognized byte — most likely an unterminated
                // string (since strings are the only multi-byte form
                // that can fail to close). Distinguish them by source
                // shape so the LexError carries the right variant.
                let slice = lex.slice();
                if slice.starts_with('"') {
                    return Err(LexError::UnterminatedString(span_start));
                }
                let ch = slice.chars().next().unwrap_or(' ');
                return Err(LexError::UnexpectedChar(span_start, ch));
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        tokenize(src)
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .filter(|k| !matches!(k, TokenKind::Whitespace | TokenKind::Newlines(_)))
            .collect()
    }

    #[test]
    fn basic_atoms() {
        assert_eq!(kinds("42"), vec![TokenKind::Int(42)]);
        assert_eq!(kinds("3.14"), vec![TokenKind::Float(3.14)]);
        assert_eq!(kinds("-7"), vec![TokenKind::Int(-7)]);
        assert_eq!(kinds("#t"), vec![TokenKind::Bool(true)]);
        assert_eq!(kinds("#f"), vec![TokenKind::Bool(false)]);
        assert_eq!(kinds("nil"), vec![TokenKind::Nil]);
        assert_eq!(kinds("\"hi\\n\""), vec![TokenKind::Str("hi\n".into())]);
        assert_eq!(
            kinds(":key-word"),
            vec![TokenKind::Keyword("key-word".into())]
        );
        assert_eq!(kinds("my-sym"), vec![TokenKind::Symbol("my-sym".into())]);
    }

    #[test]
    fn lists_and_readers() {
        assert_eq!(
            kinds("(a b)"),
            vec![
                TokenKind::LParen,
                TokenKind::Symbol("a".into()),
                TokenKind::Symbol("b".into()),
                TokenKind::RParen,
            ]
        );
        assert_eq!(
            kinds("'x"),
            vec![TokenKind::Quote, TokenKind::Symbol("x".into())]
        );
        assert_eq!(
            kinds(",@xs"),
            vec![TokenKind::UnquoteSplice, TokenKind::Symbol("xs".into())]
        );
    }

    #[test]
    fn line_comment() {
        let toks = tokenize("; hello\nworld").unwrap();
        assert!(matches!(toks[0].kind, TokenKind::LineComment(ref s) if s == " hello"));
        assert!(matches!(toks[1].kind, TokenKind::Newlines(_)));
        assert!(matches!(toks[2].kind, TokenKind::Symbol(ref s) if s == "world"));
    }

    #[test]
    fn unterminated_string_errors() {
        assert!(matches!(
            tokenize(r#""oops"#),
            Err(LexError::UnterminatedString(_))
        ));
    }

    #[test]
    fn utf8_in_string_round_trip() {
        // Multi-byte chars (Greek, emoji, accented) must come back
        // exactly — the previous byte-as-Latin-1 lexer mangled these.
        let src = r#""π — émoji 🎉""#;
        let toks = tokenize(src).unwrap();
        match &toks[0].kind {
            TokenKind::Str(s) => assert_eq!(s, "π — émoji 🎉"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn newline_run_preserves_count() {
        let toks = tokenize("a\n\n\nb").unwrap();
        // a, newlines(3), b
        assert!(matches!(toks[0].kind, TokenKind::Symbol(ref s) if s == "a"));
        match toks[1].kind {
            TokenKind::Newlines(n) => assert_eq!(n, 3),
            ref other => panic!("{other:?}"),
        }
        assert!(matches!(toks[2].kind, TokenKind::Symbol(ref s) if s == "b"));
    }

    #[test]
    fn float_with_exponent() {
        assert_eq!(kinds("1.5e10"), vec![TokenKind::Float(1.5e10)]);
        assert_eq!(kinds("1e-3"), vec![TokenKind::Float(1e-3)]);
        assert_eq!(kinds("-2.5E2"), vec![TokenKind::Float(-2.5e2)]);
    }

    #[test]
    fn bool_keyword_clash_handled() {
        // `#t#f` should tokenize as two booleans (no separator
        // required). Logos' longest-match handles this for free.
        assert_eq!(
            kinds("#t#f"),
            vec![TokenKind::Bool(true), TokenKind::Bool(false)]
        );
    }
}
