//! Byte-offset spans — minimal and cheap. Line/column are computed on demand.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A half-open byte range `[start, end)` into some source string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub const fn point(offset: u32) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }

    #[must_use]
    pub fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// The smallest span covering both. Useful for building a list node's
    /// span from its children.
    #[must_use]
    pub fn union(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    #[must_use]
    pub fn slice<'a>(self, src: &'a str) -> &'a str {
        let start = self.start as usize;
        let end = self.end as usize;
        if start >= src.len() {
            ""
        } else {
            let end = end.min(src.len());
            &src[start..end]
        }
    }

    #[must_use]
    pub fn contains(self, offset: u32) -> bool {
        offset >= self.start && offset < self.end
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// 1-indexed line/column pair — what humans see in editors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// Compute (line, column) for a byte offset. Line and column are 1-indexed.
/// O(offset); fine for diagnostics, not for hot paths.
#[must_use]
pub fn line_column(src: &str, offset: u32) -> Position {
    let mut line: u32 = 1;
    let mut col: u32 = 1;
    let offset = offset as usize;
    for (i, ch) in src.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    Position { line, column: col }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_span_is_empty() {
        let s = Span::point(5);
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn union_widens() {
        let a = Span::new(2, 5);
        let b = Span::new(4, 9);
        let u = a.union(b);
        assert_eq!(u.start, 2);
        assert_eq!(u.end, 9);
    }

    #[test]
    fn slice_extracts_substring() {
        let src = "hello world";
        assert_eq!(Span::new(6, 11).slice(src), "world");
    }

    #[test]
    fn line_column_handles_newlines() {
        let src = "abc\ndef\nghi";
        assert_eq!(line_column(src, 0), Position { line: 1, column: 1 });
        assert_eq!(line_column(src, 4), Position { line: 2, column: 1 });
        assert_eq!(line_column(src, 9), Position { line: 3, column: 2 });
    }
}
