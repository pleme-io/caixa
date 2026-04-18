//! Top-down parser — consumes the lexer's token stream, emits [`Node`]s with
//! leading trivia attached.

use thiserror::Error;

use crate::lexer::{LexError, Token, TokenKind, tokenize};
use crate::node::{Node, NodeKind};
use crate::span::Span;
use crate::trivia::{Trivia, TriviaKind};

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("lexer: {0}")]
    Lex(#[from] LexError),
    #[error("unexpected token {kind:?} at {span}")]
    Unexpected { kind: TokenKind, span: Span },
    #[error("unexpected end of input")]
    Eof,
    #[error("unmatched ')' at {0}")]
    UnmatchedClose(Span),
    #[error("reader macro ({0}) without a following form at {1}")]
    DanglingReader(&'static str, Span),
}

pub fn parse(src: &str) -> Result<Vec<Node>, ParseError> {
    let tokens = tokenize(src)?;
    let mut p = Parser {
        tokens: &tokens,
        pos: 0,
    };
    let mut out = Vec::new();
    loop {
        let leading = p.consume_trivia();
        if p.peek().is_none() {
            break;
        }
        let mut node = p.node()?;
        if node.leading.is_empty() {
            node.leading = leading;
        } else {
            // uncommon, but merge
            let mut combined = leading;
            combined.extend(node.leading.drain(..));
            node.leading = combined;
        }
        out.push(node);
    }
    Ok(out)
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.pos)
    }

    fn bump(&mut self) -> Option<&'a Token> {
        let t = self.tokens.get(self.pos)?;
        self.pos += 1;
        Some(t)
    }

    /// Collect leading comments / blank-line markers. Whitespace is dropped.
    fn consume_trivia(&mut self) -> Vec<Trivia> {
        let mut out = Vec::new();
        while let Some(tok) = self.peek() {
            match &tok.kind {
                TokenKind::LineComment(s) => {
                    out.push(Trivia {
                        kind: TriviaKind::LineComment(s.clone()),
                        span: tok.span,
                    });
                    self.pos += 1;
                }
                TokenKind::Newlines(n) if *n >= 2 => {
                    out.push(Trivia {
                        kind: TriviaKind::BlankLine,
                        span: tok.span,
                    });
                    self.pos += 1;
                }
                TokenKind::Newlines(_) | TokenKind::Whitespace => {
                    self.pos += 1;
                }
                _ => break,
            }
        }
        out
    }

    fn node(&mut self) -> Result<Node, ParseError> {
        let tok = self.peek().ok_or(ParseError::Eof)?;
        let span = tok.span;
        match &tok.kind {
            TokenKind::LParen => self.list(),
            TokenKind::RParen => Err(ParseError::UnmatchedClose(span)),
            TokenKind::Quote => self.reader_macro("quote", |n| NodeKind::Quote(Box::new(n))),
            TokenKind::Quasiquote => {
                self.reader_macro("quasiquote", |n| NodeKind::Quasiquote(Box::new(n)))
            }
            TokenKind::Unquote => self.reader_macro("unquote", |n| NodeKind::Unquote(Box::new(n))),
            TokenKind::UnquoteSplice => {
                self.reader_macro("unquote-splicing", |n| NodeKind::UnquoteSplice(Box::new(n)))
            }
            TokenKind::Str(s) => {
                let s = s.clone();
                self.pos += 1;
                Ok(Node::new(NodeKind::Str(s), span))
            }
            TokenKind::Int(i) => {
                let i = *i;
                self.pos += 1;
                Ok(Node::new(NodeKind::Int(i), span))
            }
            TokenKind::Float(f) => {
                let f = *f;
                self.pos += 1;
                Ok(Node::new(NodeKind::Float(f), span))
            }
            TokenKind::Bool(b) => {
                let b = *b;
                self.pos += 1;
                Ok(Node::new(NodeKind::Bool(b), span))
            }
            TokenKind::Nil => {
                self.pos += 1;
                Ok(Node::new(NodeKind::Nil, span))
            }
            TokenKind::Symbol(s) => {
                let s = s.clone();
                self.pos += 1;
                Ok(Node::new(NodeKind::Symbol(s), span))
            }
            TokenKind::Keyword(s) => {
                let s = s.clone();
                self.pos += 1;
                Ok(Node::new(NodeKind::Keyword(s), span))
            }
            kind => Err(ParseError::Unexpected {
                kind: kind.clone(),
                span,
            }),
        }
    }

    fn list(&mut self) -> Result<Node, ParseError> {
        let open = self.bump().expect("lparen").span;
        let mut items = Vec::new();
        loop {
            let leading = self.consume_trivia();
            let next = self.peek();
            match next {
                None => return Err(ParseError::Eof),
                Some(tok) if matches!(tok.kind, TokenKind::RParen) => {
                    let close = self.bump().expect("rparen").span;
                    let span = open.union(close);
                    let mut node = Node::new(NodeKind::List(items), span);
                    // list's leading trivia handled at caller
                    node.leading = Vec::new();
                    // discard the leading we just collected (could be trailing of last item)
                    let _ = leading;
                    return Ok(node);
                }
                Some(_) => {
                    let mut child = self.node()?;
                    if child.leading.is_empty() {
                        child.leading = leading;
                    }
                    items.push(child);
                }
            }
        }
    }

    fn reader_macro(
        &mut self,
        name: &'static str,
        wrap: impl FnOnce(Node) -> NodeKind,
    ) -> Result<Node, ParseError> {
        let head = self.bump().expect("reader macro token").span;
        self.consume_trivia();
        let inner = self.peek().ok_or(ParseError::DanglingReader(name, head))?;
        let _ = inner;
        let target = self.node()?;
        let span = head.union(target.span);
        Ok(Node::new(wrap(target), span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_atom() {
        let nodes = parse("42").unwrap();
        assert_eq!(nodes.len(), 1);
        assert!(matches!(nodes[0].kind, NodeKind::Int(42)));
        assert_eq!(nodes[0].span, Span::new(0, 2));
    }

    #[test]
    fn parse_list() {
        let nodes = parse("(a b c)").unwrap();
        assert_eq!(nodes.len(), 1);
        let NodeKind::List(items) = &nodes[0].kind else {
            panic!("expected list");
        };
        assert_eq!(items.len(), 3);
        assert!(matches!(items[0].kind, NodeKind::Symbol(ref s) if s == "a"));
    }

    #[test]
    fn parse_kwargs() {
        let nodes = parse(r#"(defcaixa :nome "demo" :versao "0.1.0")"#).unwrap();
        assert_eq!(nodes[0].head_symbol(), Some("defcaixa"));
        assert!(matches!(
            nodes[0].kwarg("nome").map(|n| &n.kind),
            Some(NodeKind::Str(s)) if s == "demo"
        ));
    }

    #[test]
    fn parse_nested_with_comments() {
        let src = r#"
;; leading doc
(defcaixa
  :nome "demo"
  ;; inline note
  :versao "0.1.0")
"#;
        let nodes = parse(src).unwrap();
        assert_eq!(nodes.len(), 1);
        assert!(!nodes[0].leading.is_empty());
        // inline comment is trivia attached to the next kwarg
    }

    #[test]
    fn parse_reader_macros() {
        let nodes = parse("`(a ,b ,@cs)").unwrap();
        let NodeKind::Quasiquote(inner) = &nodes[0].kind else {
            panic!("expected quasiquote");
        };
        let NodeKind::List(items) = &inner.kind else {
            panic!("expected list inside quasiquote");
        };
        assert_eq!(items.len(), 3);
        assert!(matches!(items[1].kind, NodeKind::Unquote(_)));
        assert!(matches!(items[2].kind, NodeKind::UnquoteSplice(_)));
    }

    #[test]
    fn to_tatara_sexp_equivalence() {
        use tatara_lisp::{Atom, Sexp};
        let src = r#"(defcaixa :nome "demo" :kind Biblioteca)"#;
        let nodes = parse(src).unwrap();
        let lowered = nodes[0].to_tatara_sexp();
        match lowered {
            Sexp::List(items) => {
                assert_eq!(items.len(), 5);
                assert!(matches!(items[0], Sexp::Atom(Atom::Symbol(ref s)) if s == "defcaixa"));
                assert!(matches!(items[1], Sexp::Atom(Atom::Keyword(ref s)) if s == "nome"));
                assert!(matches!(items[2], Sexp::Atom(Atom::Str(ref s)) if s == "demo"));
                assert!(matches!(items[3], Sexp::Atom(Atom::Keyword(ref s)) if s == "kind"));
                assert!(matches!(items[4], Sexp::Atom(Atom::Symbol(ref s)) if s == "Biblioteca"));
            }
            other => panic!("expected List, got {other:?}"),
        }
    }
}
