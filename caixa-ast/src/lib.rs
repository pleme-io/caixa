//! `caixa-ast` — span-aware Lisp AST, shared by caixa-fmt, caixa-lint,
//! and caixa-lsp.
//!
//! Why a sibling of `tatara-lisp::Sexp`? Because tatara-lisp's reader
//! throws away byte positions (it's a *compiler* — once the Sexp is
//! typed, positions are slack). Tools care about positions: the
//! formatter must re-emit at the original column; the linter must
//! highlight the exact region; the LSP must compute ranges. This crate
//! adds span + trivia (comments, blank lines) to every node while
//! keeping a free conversion to the plain `tatara_lisp::Sexp` for the
//! downstream compile pipeline.
//!
//! The surface grammar matches tatara-lisp exactly (list, symbol,
//! keyword, string, int, float, bool, nil, quote/quasiquote/unquote/
//! splice) — parse the same source, get equivalent trees.

extern crate self as caixa_ast;

pub mod lexer;
pub mod node;
pub mod parser;
pub mod span;
pub mod trivia;
pub mod visit;

pub use lexer::{Token, TokenKind, tokenize};
pub use node::{Node, NodeKind};
pub use parser::{ParseError, parse};
pub use span::{Position, Span, line_column};
pub use trivia::{Trivia, TriviaKind};
pub use visit::{Visitor, walk};
