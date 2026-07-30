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
    /// `#!/usr/bin/env tatara-script` on the first line of an executable
    /// script, held VERBATIM.
    ///
    /// Not a comment: it carries no `;` and re-emitting it as one would
    /// stop the kernel recognising the file, so the script would no longer
    /// run. Five corpus files are executable scripts the canonical
    /// interpreter runs happily and this reader refused outright — the
    /// formatter could not read them at all.
    Shebang(String),
}

impl Trivia {
    #[must_use]
    pub fn comment_text(&self) -> Option<&str> {
        match &self.kind {
            TriviaKind::LineComment(s) => Some(s),
            TriviaKind::BlankLine | TriviaKind::Shebang(_) => None,
        }
    }
}
