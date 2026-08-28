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
    /// span from its children — every parser code path that composes a
    /// parent span from its immediate child boundaries reads through this
    /// (`open.union(close)` on a delimited list, `head.union(target.span)`
    /// on a quote-form target, and every downstream trivia-owner /
    /// diagnostic-aggregator / fmt-region parent-span builder). `pub const
    /// fn` — the body reaches for `u32::min` / `u32::max`, both const
    /// stable since Rust 1.83 (well before this workspace's 1.89 MSRV
    /// floor), so the promotion is a body-preserving type-signature
    /// widening. Matches the sibling [`Self::new`] / [`Self::point`] /
    /// [`Self::contains`] `pub const fn` shape on the same [`Span`]
    /// primitive — every downstream consumer that wants a compile-time
    /// span-composition fixture (a `const OUTER: Span = INNER1.union(
    /// INNER2);` LSP hover-registry entry, a per-diagnostic const-context
    /// parent-span oracle a future admission webhook consults, a
    /// compile-time span-partition truth table the caixa-fmt trivia-owner
    /// resolver keys off) now reads through one substrate-primitive const
    /// dispatch rather than being forced onto the runtime code path.
    #[must_use]
    pub const fn union(self, other: Span) -> Span {
        // Body-preserving `Ord::min` / `Ord::max` open-coding: the trait
        // dispatch is not yet stable in `const` context (rust-lang/rust
        // #143874), so the pub-const-fn promotion reads through inline
        // `if`/`else` on the same `u32 < u32` / `u32 > u32` comparisons
        // the primitive-integer inherent methods lower to.
        let start = if self.start < other.start {
            self.start
        } else {
            other.start
        };
        let end = if self.end > other.end {
            self.end
        } else {
            other.end
        };
        Span { start, end }
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

    /// Byte-offset half-open containment predicate every consumer that
    /// keys off an author-authored source position (LSP hover
    /// span-lookup at the cursor, per-diagnostic span-registry probe,
    /// per-trivia leading/trailing-owner attachment gate) reads through
    /// — returns `true` iff `offset` lies inside the half-open range
    /// `[self.start, self.end)`. `pub const fn` — matches the sibling
    /// [`Self::new`] / [`Self::point`] `pub const fn` shape on the same
    /// [`Span`] primitive's construction axis, extending the const-eval
    /// surface onto the primitive's containment-predicate axis without
    /// a body change (integer comparison is const in Rust since long
    /// before this workspace's 1.89 MSRV floor). Every downstream
    /// consumer that wants a compile-time span-containment fixture —
    /// a `const IS_INSIDE: bool = SPAN.contains(OFFSET);` LSP hover-
    /// registry entry, a per-diagnostic const-context span oracle a
    /// future admission webhook consults, a compile-time span-partition
    /// truth table the caixa-fmt trivia-owner resolver keys off — now
    /// reads through one substrate-primitive const dispatch rather than
    /// being forced onto the runtime code path.
    #[must_use]
    pub const fn contains(self, offset: u32) -> bool {
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
    fn union_is_const() {
        // Pin the const-eval surface: the substrate-primitive parent-span
        // builder reaches into `const` context, so a future compile-time
        // parser-fixture / diagnostic-aggregator / fmt-region template
        // can build a `const OUTER: Span = INNER1.union(INNER2);` without
        // being forced onto the runtime code path. The four
        // `const _: () = assert!(…)` bindings resolve the composition at
        // compile time — any regression that drops `pub const fn` back
        // to `pub fn` (a body edit that reaches for a non-const
        // operation) fails this test at compile time rather than at
        // runtime, matching the sibling `Span::new` / `Span::point` /
        // `Span::contains` `pub const fn` shape's const-eval discipline.
        const A: Span = Span::new(2, 5);
        const B: Span = Span::new(4, 9);
        const U: Span = A.union(B);
        const _: () = assert!(U.start == 2);
        const _: () = assert!(U.end == 9);
        // Half-open containment on the const-composed parent span keys
        // through `Span::contains`'s own `pub const fn` promotion — so
        // both const-eval surfaces (composition and containment) resolve
        // in one compile-time expression, matching the [start, end)
        // boundary discipline the sibling `contains_is_const` fixture
        // already pins.
        const _: () = assert!(U.contains(2));
        const _: () = assert!(!U.contains(9));
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

    #[test]
    fn contains_is_half_open() {
        let s = Span::new(3, 7);
        assert!(!s.contains(2));
        assert!(s.contains(3));
        assert!(s.contains(6));
        assert!(!s.contains(7));
    }

    #[test]
    fn contains_is_const() {
        // Pin the const-eval surface: the substrate-primitive
        // containment predicate reaches into `const` context, so a
        // future compile-time span-registry / LSP hover-oracle /
        // trivia-owner truth-table fixture can key off `Span::contains`
        // without being forced onto the runtime code path. The four
        // `const _: () = assert!(…)` bindings resolve the predicate at
        // compile time — any regression that drops `pub const fn` back
        // to `pub fn` (a body edit that reaches for a non-const
        // operation) fails this test at compile time rather than at
        // runtime, matching the sibling `Span::new` / `Span::point`
        // `pub const fn` shape's const-eval discipline.
        const SPAN: Span = Span::new(3, 7);
        const _: () = assert!(!SPAN.contains(2));
        const _: () = assert!(SPAN.contains(3));
        const _: () = assert!(SPAN.contains(5));
        const _: () = assert!(!SPAN.contains(7));
    }
}
