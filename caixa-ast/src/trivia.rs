//! Trivia — whitespace, blank lines, and comments attached to nodes.

use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trivia {
    pub kind: TriviaKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriviaKind {
    /// `; comment` — to end of line.
    LineComment(String),
    /// A run of ≥ 2 newlines — significant for preserving paragraph breaks.
    BlankLine,
}

impl Trivia {
    #[must_use]
    pub fn comment_text(&self) -> Option<&str> {
        match &self.kind {
            TriviaKind::LineComment(s) => Some(s),
            TriviaKind::BlankLine => None,
        }
    }
}
