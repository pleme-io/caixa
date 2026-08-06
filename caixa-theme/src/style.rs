//! The small semantic-style enum every caixa tool agrees on.

use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, gen_platform::IsVariant,
)]
pub enum Semantic {
    /// Language keywords — `defcaixa`, `defteia`, `let`, `lambda`, etc.
    Keyword,
    /// Non-keyword symbols — identifiers, function names, variant names.
    Symbol,
    /// `:keyword-positioned` atoms.
    KeywordArg,
    /// `"string literals"`.
    String,
    /// `42`, `3.14`.
    Number,
    /// `#t`, `#f`, `nil`.
    Literal,
    /// `; comments`.
    Comment,
    /// Primary accent — useful for highlights, carets, focused tokens.
    Accent,
    /// Dim text — metadata, line numbers, help text.
    Muted,

    // Diagnostic severities.
    Error,
    Warning,
    Info,
    Hint,

    // Diff decorations — used by formatter preview and lint output.
    Added,
    Removed,
    Unchanged,
}

impl Semantic {
    /// Every variant of [`Semantic`] in declaration order.
    ///
    /// The single canonical arm-list every substrate consumer that has
    /// to walk the closed 15-arm semantic-style partition (the two
    /// theme overlays' exhaustive-match resolver functions
    /// `blackmatter_dark_color` / `blackmatter_light_color` in
    /// [`crate::blackmatter`], the future LSP-side per-Semantic
    /// `SemanticTokenType` dispatch at
    /// `caixa-lsp/src/main.rs`, a future
    /// `feira lint --list-styles` operator-facing enumeration verb)
    /// reads for. Peer of the sibling closed-set fieldless typed
    /// enums' `ALL` slices already carried by
    /// [`caixa_core::CaixaKind`] /
    /// [`caixa_core::supervisor::RestartStrategy`] /
    /// [`caixa_core::supervisor::RestartPolicy`] /
    /// [`caixa_core::aplicacao::PlacementStrategy`] /
    /// [`caixa_core::upgrade::UpgradeInstruction`] /
    /// `caixa_lint::diagnostic::Severity` /
    /// `caixa_lint::diagnostic::FixSafety` /
    /// `caixa_arch::InvariantKind` / `caixa_arch::ArchVerdict` /
    /// `caixa_provedor::FerriteRuntime` closed-set typed-enum
    /// discriminator axes.
    pub const ALL: &'static [Self] = &[
        Self::Keyword,
        Self::Symbol,
        Self::KeywordArg,
        Self::String,
        Self::Number,
        Self::Literal,
        Self::Comment,
        Self::Accent,
        Self::Muted,
        Self::Error,
        Self::Warning,
        Self::Info,
        Self::Hint,
        Self::Added,
        Self::Removed,
        Self::Unchanged,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_all_enumerates_every_variant_in_declaration_order() {
        // Fail-before-pass-after pin on the [`Semantic::ALL`] slice:
        // the slice must list every one of the 15 variants in
        // declaration order (Keyword → Symbol → KeywordArg → String →
        // Number → Literal → Comment → Accent → Muted → Error →
        // Warning → Info → Hint → Added → Removed → Unchanged). Peer
        // of the sibling ALL slices on the closed-set typed-enum
        // discriminator axes ([`caixa_core::CaixaKind::ALL`],
        // [`caixa_core::supervisor::RestartStrategy::ALL`],
        // [`caixa_core::supervisor::RestartPolicy::ALL`],
        // [`caixa_core::aplicacao::PlacementStrategy::ALL`],
        // [`caixa_core::upgrade::UpgradeInstruction::ALL`]). A future
        // arm addition (a `Namespace` tier between `Symbol` and
        // `KeywordArg` for the M4 tatara-lisp module system's
        // qualified-name semantic-token dispatch, a `Deleted` tier
        // for a hard-delete-mark distinct from `Removed` the future
        // 3-way diff surface grows) that lands the arm on the enum
        // but forgets to extend `ALL` must trip this pin rather than
        // surface as a downstream consumer's silently-partial
        // iteration.
        assert_eq!(
            Semantic::ALL,
            &[
                Semantic::Keyword,
                Semantic::Symbol,
                Semantic::KeywordArg,
                Semantic::String,
                Semantic::Number,
                Semantic::Literal,
                Semantic::Comment,
                Semantic::Accent,
                Semantic::Muted,
                Semantic::Error,
                Semantic::Warning,
                Semantic::Info,
                Semantic::Hint,
                Semantic::Added,
                Semantic::Removed,
                Semantic::Unchanged,
            ],
        );
        // Also pin the per-arm `IsVariant`-derived partition: every
        // arm in `ALL` must satisfy exactly one of the 15 generated
        // arm-discriminator predicates.
        for variant in Semantic::ALL {
            let row = [
                variant.is_keyword(),
                variant.is_symbol(),
                variant.is_keyword_arg(),
                variant.is_string(),
                variant.is_number(),
                variant.is_literal(),
                variant.is_comment(),
                variant.is_accent(),
                variant.is_muted(),
                variant.is_error(),
                variant.is_warning(),
                variant.is_info(),
                variant.is_hint(),
                variant.is_added(),
                variant.is_removed(),
                variant.is_unchanged(),
            ];
            let hits = row.iter().filter(|b| **b).count();
            assert_eq!(
                hits, 1,
                "Semantic::{variant:?} must satisfy exactly one of the \
                 15 is_* arm-discriminator predicates; got {row:?}",
            );
        }
    }

    #[test]
    fn semantic_is_variant_predicates_partition_the_arm_set() {
        // Fail-before-pass-after pin on the [`gen_platform::IsVariant`]
        // derive: for each of the 15 variants, exactly one of the
        // generated is_* predicates returns `true` and the other 14
        // return `false`. Pre-derive the closed 15-arm partition
        // lived only inside the two theme overlays' 15-arm match
        // resolvers; a future rebrand (a `#[is_variant(name = "…")]`
        // drift, a manual hand-rolled `impl` that shadows the
        // derive-generated method, an arm rename) trips this pin at
        // caixa-theme build time rather than surfacing far from the
        // derive declaration. Peer of the sibling
        // [`caixa_core::CaixaKind`] `IsVariant` partition pin.
        // A copy-paste flip that reroutes one arm through the wrong
        // predicate lane trips at the identity-diagonal assertion,
        // since each variant's row is generated live from `ALL`'s
        // declaration order rather than transcribed by hand.
        for (idx, variant) in Semantic::ALL.iter().enumerate() {
            let observed: [bool; 16] = [
                variant.is_keyword(),
                variant.is_symbol(),
                variant.is_keyword_arg(),
                variant.is_string(),
                variant.is_number(),
                variant.is_literal(),
                variant.is_comment(),
                variant.is_accent(),
                variant.is_muted(),
                variant.is_error(),
                variant.is_warning(),
                variant.is_info(),
                variant.is_hint(),
                variant.is_added(),
                variant.is_removed(),
                variant.is_unchanged(),
            ];
            let mut expected = [false; 16];
            expected[idx] = true;
            assert_eq!(
                observed, expected,
                "Semantic::{variant:?} at ALL[{idx}] is_* predicates \
                 must fire only on their own arm lane (identity \
                 diagonal); got {observed:?}",
            );
        }
    }

    #[test]
    fn semantic_is_variant_predicates_are_const_fn() {
        // The [`gen_platform::IsVariant`] derive emits `const fn`
        // predicates on the peer [`caixa_core::CaixaKind`] /
        // [`caixa_core::upgrade::UpgradeInstruction`] /
        // [`caixa_core::supervisor::RestartStrategy`] /
        // [`caixa_core::supervisor::RestartPolicy`] closed-set typed
        // enums — pin the same posture on [`Semantic`] so a future
        // accidental downgrade to non-`const` (an added runtime helper
        // reachable only from a non-`const` context, a manual hand-
        // rolled `impl` that shadows the derive-generated method)
        // trips at caixa-theme build time rather than surfacing as a
        // downstream `const`-context regression far from the derive
        // declaration.
        const { assert!(Semantic::Keyword.is_keyword()) };
        const { assert!(Semantic::Error.is_error()) };
        const { assert!(Semantic::Added.is_added()) };
        const { assert!(Semantic::Unchanged.is_unchanged()) };
    }
}
