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

    /// Byte-width of the half-open range `[start, end)`. `pub const fn` —
    /// `u32::saturating_sub` is const-stable since Rust 1.47, well before
    /// this workspace's 1.89 MSRV floor, so the promotion is a body-
    /// preserving type-signature widening. Matches the sibling
    /// [`Self::new`] / [`Self::point`] / [`Self::contains`] /
    /// [`Self::union`] `pub const fn` shape on the same [`Span`] primitive
    /// — every downstream consumer that wants a compile-time span-width
    /// fixture (a `const WIDTH: u32 = SPAN.len();` LSP hover-registry
    /// entry, a per-diagnostic const-context width oracle a future
    /// admission webhook consults, a compile-time span-partition truth
    /// table the caixa-fmt trivia-owner resolver keys off) now reads
    /// through one substrate-primitive const dispatch rather than being
    /// forced onto the runtime code path.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    /// Half-open emptiness predicate — `true` iff `self.start == self.end`.
    /// `pub const fn` — folds onto the sibling [`Self::len`] `pub const
    /// fn` promotion (integer equality is const in Rust since long before
    /// this workspace's 1.89 MSRV floor). Matches every other accessor /
    /// predicate on this [`Span`] primitive's const-eval surface; only
    /// the fundamentally-runtime-only `slice(&str)` method (string-slice
    /// indexing outside const-eval) remains `pub fn`.
    #[must_use]
    pub const fn is_empty(self) -> bool {
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

impl Position {
    /// Substrate-primitive constructor every producer of a
    /// 1-indexed line/column pair reads through — folds the two-slot
    /// `Position { line, column }` struct-literal wire-up (the sole
    /// production emitter [`line_column`]'s tail, plus every
    /// per-fixture line/column literal in the sibling test module)
    /// onto one `pub const fn` dispatch. Matches the sibling
    /// [`Span::new`] / [`Span::point`] `pub const fn` shape on the
    /// same caixa-ast source-position primitive family; every
    /// downstream LSP hover-registry / diagnostic emitter / future
    /// position-carrying trivia-registry that wants a compile-time
    /// position fixture (a `const AT: Position = Position::new(1, 4);`
    /// LSP hover-oracle default, a per-diagnostic const-context
    /// leading-position fixture, a compile-time position-partition
    /// truth table the caixa-fmt trivia-owner resolver keys off)
    /// now reaches through one substrate-primitive const dispatch
    /// rather than duplicating the two-slot struct literal at every
    /// construction site.
    #[must_use]
    pub const fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }

    /// Canonical origin — line 1, column 1, matching the
    /// [`line_column`] convention (both axes 1-indexed). `pub const
    /// fn` — the shape materialises at compile time so a future
    /// const-context consumer (a substrate-wide `const ORIGIN:
    /// Position = Position::origin();` LSP hover-oracle default, a
    /// per-diagnostic const-context leading-position fixture that
    /// today reaches for the `(1, 1)` magic pair inline) reads
    /// through one substrate-primitive const dispatch rather than
    /// duplicating the `(1, 1)` origin literal at every consumer.
    #[must_use]
    pub const fn origin() -> Self {
        Self::new(1, 1)
    }
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
    Position::new(line, col)
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
        assert_eq!(line_column(src, 0), Position::origin());
        assert_eq!(line_column(src, 4), Position::new(2, 1));
        assert_eq!(line_column(src, 9), Position::new(3, 2));
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
    fn len_and_is_empty_are_const() {
        // Pin the const-eval surface: the substrate-primitive width
        // accessor and emptiness predicate reach into `const` context, so
        // a future compile-time span-registry / LSP hover-oracle /
        // trivia-owner truth-table fixture can key off `Span::len` /
        // `Span::is_empty` without being forced onto the runtime code
        // path. Any regression that drops `pub const fn` back to `pub fn`
        // (a body edit that reaches for a non-const operation) fails this
        // test at compile time rather than at runtime, matching the
        // sibling `Span::new` / `Span::point` / `Span::contains` /
        // `Span::union` `pub const fn` shape's const-eval discipline.
        const RANGE: Span = Span::new(3, 7);
        const POINT: Span = Span::point(5);
        const RANGE_LEN: u32 = RANGE.len();
        const POINT_LEN: u32 = POINT.len();
        const RANGE_EMPTY: bool = RANGE.is_empty();
        const POINT_EMPTY: bool = POINT.is_empty();
        const _: () = assert!(RANGE_LEN == 4);
        const _: () = assert!(POINT_LEN == 0);
        const _: () = assert!(!RANGE_EMPTY);
        const _: () = assert!(POINT_EMPTY);
        // Saturating-sub floor: an inverted (end < start) fixture must
        // clamp to 0 at compile time, matching the runtime
        // `u32::saturating_sub` semantics the pre-lift body carried.
        const INVERTED: Span = Span::new(9, 2);
        const INVERTED_LEN: u32 = INVERTED.len();
        const INVERTED_EMPTY: bool = INVERTED.is_empty();
        const _: () = assert!(INVERTED_LEN == 0);
        const _: () = assert!(INVERTED_EMPTY);
    }

    #[test]
    fn position_new_and_origin_are_const() {
        // Pin the const-eval surface on the substrate-primitive
        // 1-indexed line/column pair: the constructor and canonical
        // origin reach into `const` context, so a future compile-time
        // LSP hover-registry / diagnostic-emitter / trivia-owner
        // truth-table fixture can key off `Position::new` /
        // `Position::origin` without being forced onto the runtime
        // code path. Any regression that drops `pub const fn` back to
        // `pub fn` (a body edit that reaches for a non-const
        // operation) fails this test at compile time rather than at
        // runtime, matching the sibling `Span::new` / `Span::point` /
        // `Span::contains` / `Span::union` / `Span::len` /
        // `Span::is_empty` `pub const fn` shape's const-eval
        // discipline on the same caixa-ast source-position primitive
        // family.
        //
        // Also pins `Position::origin`'s canonical (1, 1) shape at
        // compile time — a future accidental drift (an `origin` that
        // returns `Position::new(0, 0)` on a well-meaning "zero-
        // indexed origin" rewrite that forgets `line_column` emits
        // 1-indexed positions) trips at build time rather than
        // surfacing far from the origin declaration at some
        // downstream diagnostic-emitter's off-by-one row report.
        const AT: Position = Position::new(2, 4);
        const ORIGIN: Position = Position::origin();
        const AT_LINE: u32 = AT.line;
        const AT_COLUMN: u32 = AT.column;
        const ORIGIN_LINE: u32 = ORIGIN.line;
        const ORIGIN_COLUMN: u32 = ORIGIN.column;
        const _: () = assert!(AT_LINE == 2);
        const _: () = assert!(AT_COLUMN == 4);
        const _: () = assert!(ORIGIN_LINE == 1);
        const _: () = assert!(ORIGIN_COLUMN == 1);
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
