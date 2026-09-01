//! Trivia — whitespace, blank lines, and comments attached to nodes.

use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trivia {
    pub kind: TriviaKind,
    pub span: Span,
}

/// The typed variant discriminator on the caixa-ast trivia surface — every
/// [`Trivia`]'s carrying-shape (line comment, blank line, shebang) projects
/// through this closed three-arm partition.
///
/// The [`gen_platform::IsVariant`] derive emits per-arm arm-discriminator
/// predicates — [`Self::is_line_comment`], [`Self::is_blank_line`],
/// [`Self::is_shebang`] — so every downstream consumer that only needs the
/// arm-discriminator projection (not the borrowed field value) reaches for
/// one typed dispatch on the substrate primitive rather than a hand-rolled
/// `matches!(t.kind, TriviaKind::X(_))` literal. Peer of the sibling
/// [`crate::NodeKind`] `IsVariant` lift already on the caixa-ast surface
/// (7f6aa98) — extends the same discipline onto the trivia axis every
/// downstream authoring consumer (`caixa-fmt` blank-line skip + line-comment
/// detection, a future `caixa-lint` no-shebang-below-line-1 rule) partitions
/// on.
#[derive(Debug, Clone, PartialEq, Eq, gen_platform::IsVariant)]
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
    /// Project the wrapped [`TriviaKind::LineComment`] body as `&str`, or
    /// `None` on the sibling two arms ([`TriviaKind::BlankLine`] +
    /// [`TriviaKind::Shebang`]) — the trivia-envelope-scoped surface every
    /// downstream authoring consumer that only needs the line-comment body
    /// (a `caixa-fmt` block-comment paragraph-fill pass over the collected
    /// trivia list, a `caixa-lint` doc-comment / rustdoc-shape probe, a
    /// deferred `caixa-lsp` hover pop-up that renders the comment body on
    /// mouse-over) partitions on.
    ///
    /// `pub const fn` — closes const-eval discipline on the [`Trivia`]
    /// envelope-scoped projection axis, peer with the sibling
    /// [`crate::TriviaKind`] `IsVariant`-derived per-arm predicate
    /// discriminators ([`TriviaKind::is_line_comment`] +
    /// [`TriviaKind::is_blank_line`] + [`TriviaKind::is_shebang`]) on the
    /// same trivia-arm-discriminator surface. The body reaches for
    /// [`String::as_str`] on the [`TriviaKind::LineComment`] borrowed-
    /// `String` slot — const-stable since Rust 1.87, well before this
    /// workspace's 1.89 MSRV floor — so the promotion is a body-preserving
    /// type-signature widening. Matches the sibling per-source-position-
    /// primitive const-eval-surface family on the caixa-ast surface
    /// ([`crate::Span::new`] / [`crate::Span::point`] / [`crate::Span::len`]
    /// / [`crate::Span::is_empty`] / [`crate::Span::contains`] /
    /// [`crate::Span::union`] on the byte-offset axis,
    /// [`crate::Position::new`] / [`crate::Position::origin`] on the 1-
    /// indexed line/column axis, [`crate::Position::line_column`] on the
    /// `Position` projection axis, [`crate::NodeKind::seq_delims`] /
    /// [`crate::NodeKind::reader_macro_prefix`] / [`crate::NodeKind::as_keyword`]
    /// / [`crate::NodeKind::as_symbol`] / [`crate::NodeKind::as_str`] on
    /// the outer-NodeKind writer-half projection axis) — every downstream
    /// consumer that wants a compile-time line-comment-body fixture (a
    /// `const HEADER: Option<&str> = TRIVIA.comment_text();` compile-time
    /// oracle a caixa-fmt paragraph-fill pass keys off, a per-lint const-
    /// context doc-comment shape probe a future admission webhook
    /// consults) now reads through one substrate-primitive const dispatch
    /// rather than being forced onto the runtime code path.
    #[must_use]
    pub const fn comment_text(&self) -> Option<&str> {
        match &self.kind {
            TriviaKind::LineComment(s) => Some(s.as_str()),
            TriviaKind::BlankLine | TriviaKind::Shebang(_) => None,
        }
    }
}

#[cfg(test)]
mod is_variant_tests {
    use super::*;

    fn all_variants() -> Vec<(TriviaKind, &'static str)> {
        vec![
            (TriviaKind::LineComment("hello".into()), "LineComment"),
            (TriviaKind::BlankLine, "BlankLine"),
            (
                TriviaKind::Shebang("#!/usr/bin/env tatara-script".into()),
                "Shebang",
            ),
        ]
    }

    fn predicate_row(k: &TriviaKind) -> [bool; 3] {
        [k.is_line_comment(), k.is_blank_line(), k.is_shebang()]
    }

    // Fail-before-pass-after pin on the [`gen_platform::IsVariant`]
    // derive-generated per-arm predicate partition — for every variant in
    // `all_variants()`, the observed 3-slot predicate row must equal a
    // one-hot row with the `true` at exactly the same index as the
    // variant's declaration order. Expected rows are generated live from
    // the enumeration rather than transcribed by hand, so a copy-paste
    // flip that reroutes one arm through the wrong predicate lane trips
    // at the identity-diagonal assertion the way every peer sibling
    // [`crate::NodeKind`] / `CaixaKind` / `CaixaDialeto` /
    // `PathShapeViolation` / `RestartStrategy` partition pin already does.
    #[test]
    fn trivia_kind_is_variant_predicates_partition_the_arm_set() {
        let variants = all_variants();
        for (idx, (variant, name)) in variants.iter().enumerate() {
            let observed = predicate_row(variant);
            let mut expected = [false; 3];
            expected[idx] = true;
            assert_eq!(
                observed, expected,
                "TriviaKind::{name} at declaration-order slot {idx} must \
                 satisfy exactly one is_* predicate (its own); observed \
                 row must equal the one-hot expected row"
            );
        }
    }

    // Fail-before-pass-after pin on [`Trivia::comment_text`]'s
    // `const`-eval-surface posture. The projection routes the wrapped
    // [`TriviaKind::LineComment`] borrowed-`String` slot through the
    // `pub const fn` [`String::as_str`] (const-stable since Rust 1.87,
    // well within the workspace's 1.89 MSRV floor) — any future
    // accidental downgrade to non-`const` fails
    // `comment_text_via_const_fn` at caixa-ast build time with E0015
    // (`cannot call non-const method`), strictly stronger than a runtime
    // `assert!`. Sibling of the peer per-source-position-primitive
    // `const`-eval-surface passes on the caixa-ast surface
    // ([`crate::Span::new`] / [`crate::Span::point`] / [`crate::Span::len`]
    // / [`crate::Span::is_empty`] / [`crate::Span::contains`] /
    // [`crate::Span::union`] on the byte-offset axis,
    // [`crate::Position::new`] / [`crate::Position::origin`] /
    // [`crate::Position::line_column`] on the 1-indexed line/column
    // axis, [`crate::NodeKind::seq_delims`] /
    // [`crate::NodeKind::reader_macro_prefix`] /
    // [`crate::NodeKind::as_keyword`] / [`crate::NodeKind::as_symbol`]
    // / [`crate::NodeKind::as_str`] on the outer-NodeKind writer-half
    // projection axis). The sweep exercises all three
    // [`TriviaKind`] arms (a populated `LineComment` body, the field-
    // less `BlankLine`, a populated `Shebang` body) so a copy-paste flip
    // that reroutes one arm's return through the wrong projection lane
    // trips at caixa-ast test time under `PartialEq` on the
    // `Option<&str>` return shape rather than at a downstream
    // caixa-fmt / caixa-lint / caixa-lsp consumer-observable drift.
    #[test]
    fn trivia_comment_text_projection_is_const_fn() {
        const fn comment_text_via_const_fn(t: &Trivia) -> Option<&str> {
            t.comment_text()
        }
        for (variant, expected) in [
            (TriviaKind::LineComment("hello".into()), Some("hello")),
            (TriviaKind::BlankLine, None),
            (
                TriviaKind::Shebang("#!/usr/bin/env tatara-script".into()),
                None,
            ),
        ] {
            let trivia = Trivia {
                kind: variant,
                span: Span::default(),
            };
            assert_eq!(
                comment_text_via_const_fn(&trivia),
                expected,
                "Trivia::comment_text must project through the pub const \
                 fn body byte-equal to the pre-lift open-coded match on \
                 the same fixture",
            );
            assert_eq!(comment_text_via_const_fn(&trivia), trivia.comment_text());
        }
    }

    // Byte-parity pin on the two field-agnostic `matches!` shapes this
    // lift replaces at production call sites: the `TriviaKind::BlankLine`
    // gate (caixa-fmt/src/printer.rs `trim_leading_blanks` take-while)
    // and the `TriviaKind::LineComment(_)` gate (caixa-fmt/src/printer.rs
    // `contains_comment_trivia` any). Refuses a future accidental split
    // between the derived predicate and its pre-lift `matches!` shape
    // (a hand-rolled shadow `impl` that overrides one path, an accidental
    // rebrand of one converged call site back to the `matches!` form) on
    // the two load-bearing trivia-arm-discriminator axes every downstream
    // authoring consumer (caixa-fmt today, caixa-lint tomorrow) keys off.
    #[test]
    fn trivia_kind_is_blank_line_and_is_line_comment_byte_equal_pre_lift_matches_shape() {
        for (variant, name) in all_variants() {
            let via_matches_blank = matches!(variant, TriviaKind::BlankLine);
            let via_predicate_blank = variant.is_blank_line();
            assert_eq!(
                via_predicate_blank, via_matches_blank,
                "TriviaKind::{name}.is_blank_line() must byte-equal \
                 matches!(_, TriviaKind::BlankLine) — otherwise the \
                 converged trim_leading_blanks call site in caixa-fmt \
                 would silently disagree with its pre-lift shape"
            );
            let via_matches_line = matches!(variant, TriviaKind::LineComment(_));
            let via_predicate_line = variant.is_line_comment();
            assert_eq!(
                via_predicate_line, via_matches_line,
                "TriviaKind::{name}.is_line_comment() must byte-equal \
                 matches!(_, TriviaKind::LineComment(_)) — otherwise the \
                 converged contains_comment_trivia call site in caixa-fmt \
                 would silently disagree with its pre-lift shape"
            );
        }
    }
}
