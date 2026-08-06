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
    #[must_use]
    pub fn comment_text(&self) -> Option<&str> {
        match &self.kind {
            TriviaKind::LineComment(s) => Some(s),
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
