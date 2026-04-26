//! Lisp lexer — scans source bytes into tokens with spans.
//!
//! Token alphabet:
//!   - `(` `)` — list delimiters
//!   - `'` `` ` `` `,` `,@` — reader macros (quote / quasiquote / unquote / splice)
//!   - `"…"` — strings, with `\"` `\\` `\n` `\t` `\r` escapes
//!   - `#t` / `#f` — booleans
//!   - `nil` — the nil atom
//!   - integers: `[+-]?[0-9]+` — integer literal
//!   - floats: decimal with `.` or scientific with `e|E`
//!   - `:name-like` — keywords
//!   - `; …\n` — line comments
//!   - otherwise: symbols — `[^ \t\r\n()'`,"]+`

use std::num::{ParseFloatError, ParseIntError};

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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LexError {
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

/// Scan a source string into tokens. Trivia (whitespace, comments) is
/// preserved — the parser filters what it doesn't need.
pub fn tokenize(src: &str) -> Result<Vec<Token>, LexError> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        let start = u32::try_from(i).expect("source too large");
        let b = bytes[i];

        if b == b'(' {
            out.push(Token {
                kind: TokenKind::LParen,
                span: Span::new(start, start + 1),
            });
            i += 1;
        } else if b == b')' {
            out.push(Token {
                kind: TokenKind::RParen,
                span: Span::new(start, start + 1),
            });
            i += 1;
        } else if b == b'\'' {
            out.push(Token {
                kind: TokenKind::Quote,
                span: Span::new(start, start + 1),
            });
            i += 1;
        } else if b == b'`' {
            out.push(Token {
                kind: TokenKind::Quasiquote,
                span: Span::new(start, start + 1),
            });
            i += 1;
        } else if b == b',' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'@' {
                out.push(Token {
                    kind: TokenKind::UnquoteSplice,
                    span: Span::new(start, start + 2),
                });
                i += 2;
            } else {
                out.push(Token {
                    kind: TokenKind::Unquote,
                    span: Span::new(start, start + 1),
                });
                i += 1;
            }
        } else if b == b'"' {
            let (tok, new_i) = lex_string(bytes, i, start)?;
            out.push(tok);
            i = new_i;
        } else if b == b';' {
            let (tok, new_i) = lex_line_comment(bytes, i, start);
            out.push(tok);
            i = new_i;
        } else if b == b'\n' || b == b'\r' {
            let (tok, new_i) = lex_newlines(bytes, i, start);
            out.push(tok);
            i = new_i;
        } else if b.is_ascii_whitespace() {
            let (tok, new_i) = lex_whitespace(bytes, i, start);
            out.push(tok);
            i = new_i;
        } else if b == b'#' {
            // #t / #f — booleans; anything else is an error we punt on.
            if i + 1 < bytes.len() {
                match bytes[i + 1] {
                    b't' => {
                        out.push(Token {
                            kind: TokenKind::Bool(true),
                            span: Span::new(start, start + 2),
                        });
                        i += 2;
                        continue;
                    }
                    b'f' => {
                        out.push(Token {
                            kind: TokenKind::Bool(false),
                            span: Span::new(start, start + 2),
                        });
                        i += 2;
                        continue;
                    }
                    _ => {}
                }
            }
            return Err(LexError::UnexpectedChar(start, b as char));
        } else if b == b':' {
            let (tok, new_i) = lex_keyword(bytes, i, start);
            out.push(tok);
            i = new_i;
        } else if b.is_ascii_digit()
            || ((b == b'-' || b == b'+')
                && i + 1 < bytes.len()
                && (bytes[i + 1].is_ascii_digit() || bytes[i + 1] == b'.'))
        {
            let (tok, new_i) = lex_number(bytes, i, start)?;
            out.push(tok);
            i = new_i;
        } else {
            let (tok, new_i) = lex_symbol(bytes, i, start);
            out.push(tok);
            i = new_i;
        }
    }

    Ok(out)
}

fn lex_string(bytes: &[u8], start: usize, span_start: u32) -> Result<(Token, usize), LexError> {
    let mut out = String::new();
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let span = Span::new(span_start, u32::try_from(i + 1).expect("ovf"));
                return Ok((
                    Token {
                        kind: TokenKind::Str(out),
                        span,
                    },
                    i + 1,
                ));
            }
            b'\\' if i + 1 < bytes.len() => {
                match bytes[i + 1] {
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    b'r' => out.push('\r'),
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    other => {
                        return Err(LexError::BadEscape(
                            u32::try_from(i).expect("ovf"),
                            other as char,
                        ));
                    }
                }
                i += 2;
            }
            c if c < 0x80 => {
                // ASCII fast path.
                out.push(c as char);
                i += 1;
            }
            _ => {
                // UTF-8 multi-byte sequence: decode the next char.
                // The leading byte's high bits encode the length:
                //   110xxxxx → 2 bytes
                //   1110xxxx → 3 bytes
                //   11110xxx → 4 bytes
                let lead = bytes[i];
                let width = if lead & 0b1110_0000 == 0b1100_0000 {
                    2
                } else if lead & 0b1111_0000 == 0b1110_0000 {
                    3
                } else if lead & 0b1111_1000 == 0b1111_0000 {
                    4
                } else {
                    return Err(LexError::BadEscape(
                        u32::try_from(i).expect("ovf"),
                        lead as char,
                    ));
                };
                if i + width > bytes.len() {
                    return Err(LexError::BadEscape(
                        u32::try_from(i).expect("ovf"),
                        lead as char,
                    ));
                }
                let chunk = std::str::from_utf8(&bytes[i..i + width]).map_err(|_| {
                    LexError::BadEscape(u32::try_from(i).expect("ovf"), lead as char)
                })?;
                out.push_str(chunk);
                i += width;
            }
        }
    }
    Err(LexError::UnterminatedString(span_start))
}

fn lex_line_comment(bytes: &[u8], start: usize, span_start: u32) -> (Token, usize) {
    let mut i = start + 1; // past the ;
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    let text = String::from_utf8_lossy(&bytes[start + 1..i]).into_owned();
    (
        Token {
            kind: TokenKind::LineComment(text),
            span: Span::new(span_start, u32::try_from(i).expect("ovf")),
        },
        i,
    )
}

fn lex_newlines(bytes: &[u8], start: usize, span_start: u32) -> (Token, usize) {
    let mut count = 0u32;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                count += 1;
                i += 1;
            }
            b'\r' => {
                i += 1;
            }
            b' ' | b'\t' => {
                i += 1;
            }
            _ => break,
        }
    }
    (
        Token {
            kind: TokenKind::Newlines(count),
            span: Span::new(span_start, u32::try_from(i).expect("ovf")),
        },
        i,
    )
}

fn lex_whitespace(bytes: &[u8], start: usize, span_start: u32) -> (Token, usize) {
    let mut i = start;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    (
        Token {
            kind: TokenKind::Whitespace,
            span: Span::new(span_start, u32::try_from(i).expect("ovf")),
        },
        i,
    )
}

fn lex_keyword(bytes: &[u8], start: usize, span_start: u32) -> (Token, usize) {
    let mut i = start + 1;
    while i < bytes.len() && !is_atom_terminator(bytes[i]) {
        i += 1;
    }
    let text = String::from_utf8_lossy(&bytes[start + 1..i]).into_owned();
    (
        Token {
            kind: TokenKind::Keyword(text),
            span: Span::new(span_start, u32::try_from(i).expect("ovf")),
        },
        i,
    )
}

fn lex_number(bytes: &[u8], start: usize, span_start: u32) -> Result<(Token, usize), LexError> {
    let mut i = start;
    let mut saw_dot = false;
    let mut saw_exp = false;

    if bytes[i] == b'-' || bytes[i] == b'+' {
        i += 1;
    }
    while i < bytes.len() {
        match bytes[i] {
            b'0'..=b'9' => i += 1,
            b'.' if !saw_dot && !saw_exp => {
                saw_dot = true;
                i += 1;
            }
            b'e' | b'E' if !saw_exp => {
                saw_exp = true;
                saw_dot = true; // exponential is float by nature
                i += 1;
                if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                    i += 1;
                }
            }
            c if is_atom_terminator(c) => break,
            _ => break,
        }
    }
    let text = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
    let span = Span::new(span_start, u32::try_from(i).expect("ovf"));
    if saw_dot || saw_exp {
        let v = text
            .parse::<f64>()
            .map_err(|e| LexError::BadFloat(span_start, e.to_string()))?;
        Ok((
            Token {
                kind: TokenKind::Float(v),
                span,
            },
            i,
        ))
    } else {
        let v = text
            .parse::<i64>()
            .map_err(|e| LexError::BadInt(span_start, e.to_string()))?;
        Ok((
            Token {
                kind: TokenKind::Int(v),
                span,
            },
            i,
        ))
    }
}

fn lex_symbol(bytes: &[u8], start: usize, span_start: u32) -> (Token, usize) {
    let mut i = start;
    while i < bytes.len() && !is_atom_terminator(bytes[i]) {
        i += 1;
    }
    let text = String::from_utf8_lossy(&bytes[start..i]).into_owned();
    let kind = match text.as_str() {
        "nil" => TokenKind::Nil,
        _ => TokenKind::Symbol(text),
    };
    (
        Token {
            kind,
            span: Span::new(span_start, u32::try_from(i).expect("ovf")),
        },
        i,
    )
}

const fn is_atom_terminator(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\t' | b'\r' | b'\n' | b'(' | b')' | b'\'' | b'`' | b',' | b'"' | b';'
    )
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
}
