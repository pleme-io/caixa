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
    /// A verbatim `#!…` first line. See [`crate::trivia::TriviaKind::Shebang`].
    Shebang(String),
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
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

    // The brace/vector dialect. `{ :k v }` and `[ a b ]` are REAL
    // SYNTAX, not sugar — theory/TATARA-LISP-CONSOLIDATION.md D4, on the
    // evidence of 62 live caixa.lisp manifests that author nested maps
    // (`:package { :name "…" :version "…" }`) and are consumed today.
    //
    // Until now these four bytes had no token here at all: they fell
    // through to the Symbol regex below, so a map lexed as a flat run of
    // atoms with `{` and `}` as ordinary symbols. That made every real
    // manifest an odd-length list to the printer, which is why `feira
    // fmt` abandoned the key/value shape and exploded them one atom per
    // line. caixa-ts/grammar.js has had `map` and `vector` rules from the
    // start and its header says the two grammars are kept in lockstep —
    // this closes the gap on the Rust side.
    #[token("{")]
    LBrace,

    #[token("}")]
    RBrace,

    #[token("[")]
    LBracket,

    #[token("]")]
    RBracket,

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

    // Keyword: `:` followed by atom chars. `{}[]` terminate it, or
    // `:version "0.3.0"}` would lex the closing brace into the keyword.
    #[regex(":[^\\s()'`,\";\\{\\}\\[\\]]+", |lex| lex.slice()[1..].to_string())]
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
    // `{}[]` join the terminator set for the same reason `()` are in it:
    // they are structural delimiters now, so `{:name` must lex as LBrace
    // + Keyword rather than as one symbol `{:name`. caixa-ts states the
    // same set as an ALLOW-list (`[A-Za-z_+\-*/=<>?!%&~.]…`), which
    // already excluded braces — this is the Rust side catching up.
    #[regex(
        "[^\\s()'`,\";#\\{\\}\\[\\]][^\\s()'`,\";#\\{\\}\\[\\]]*",
        |lex| lex.slice().to_string()
    )]
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
                // An UNKNOWN escape yields the character itself, dropping
                // the backslash — matching the canonical reader exactly
                // (`tatara-lisp/src/reader.rs`: `other => other`).
                //
                // Rejecting these was a real divergence, not strictness:
                // `actions/db-migrate/run.tlisp` carries a grep pattern
                // written `'Applied\|migration\|up to date'`, which the
                // canonical reader accepts and this lexer refused, so the
                // formatter could not read a file the runtime runs. Two
                // readers disagreeing about what the language IS is the
                // concrete cost of the fleet's 13 independent
                // S-expression readers; here the canonical one is the
                // oracle and this one conforms.
                Some((_, other)) => out.push(other),
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

    // A leading `#!` line is a shebang, not source. Emitted as its own
    // token so it survives formatting verbatim; logos never sees it, since
    // `#` is not otherwise part of the grammar. Only at offset 0 — a `#!`
    // anywhere else is genuinely invalid and must still be an error.
    let body_start = if src.starts_with("#!") {
        let end = src.find('\n').unwrap_or(src.len());
        out.push(Token {
            kind: TokenKind::Shebang(src[..end].to_string()),
            span: Span::new(0, u32::try_from(end).unwrap_or(u32::MAX)),
        });
        end
    } else {
        0
    };

    let mut lex = LogosKind::lexer(&src[body_start..]);

    while let Some(result) = lex.next() {
        let span = lex.span();
        let span_start = u32::try_from(span.start + body_start).unwrap_or(u32::MAX);
        let span_end = u32::try_from(span.end + body_start).unwrap_or(u32::MAX);
        let span = Span::new(span_start, span_end);

        match result {
            Ok(kind) => {
                let public = match kind {
                    LogosKind::LParen => TokenKind::LParen,
                    LogosKind::RParen => TokenKind::RParen,
                    LogosKind::LBrace => TokenKind::LBrace,
                    LogosKind::RBrace => TokenKind::RBrace,
                    LogosKind::LBracket => TokenKind::LBracket,
                    LogosKind::RBracket => TokenKind::RBracket,
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

    // `3.14` below is the *expected lex output* for the input string
    // `"3.14"` — a float-literal round-trip fixture, not an approximation
    // of `f64::consts::PI` used in a computation. `clippy::approx_constant`
    // is deny-by-default (correctness group), so without this scoped allow
    // `cargo clippy` aborts this crate with a hard error and never reports
    // the rest of the workspace at all. Substituting `PI` here would break
    // the round-trip the assertion exists to prove.
    #[allow(
        clippy::approx_constant,
        reason = "float-literal lex fixture, not a PI approximation"
    )]
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
