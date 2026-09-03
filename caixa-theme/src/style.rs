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

    /// Canonical kebab-case discriminator scalar for this variant — the
    /// single substrate-primitive `&'static str` projection every
    /// downstream consumer of the closed 16-arm [`Semantic`] partition
    /// (a future LSP-side per-Semantic `SemanticTokenType` name-mapping
    /// dispatch at `caixa-lsp/src/main.rs`, a future
    /// `feira lint --list-styles` operator-facing enumeration verb, a
    /// future `caixa.nvim` per-Semantic highlight-group name resolver
    /// that reaches for a stable kebab identifier per arm, a future
    /// `blackmatter-shell` per-arm classname the terminal emitter
    /// composes into a `data-semantic="<kebab>"` attribute) reaches
    /// through. Kebab-case matches the peer
    /// [`gen_platform::IsVariant`]-derived kebab discriminant convention
    /// the sibling closed-set typed enums
    /// ([`caixa_core::CaixaKind::as_str`],
    /// [`caixa_core::supervisor::RestartStrategy::as_str`],
    /// [`caixa_core::supervisor::RestartPolicy::as_str`],
    /// [`caixa_core::aplicacao::PlacementStrategy::as_str`],
    /// [`caixa_core::upgrade::UpgradeInstruction::as_str`],
    /// `caixa_lint::diagnostic::Severity::as_str`,
    /// `caixa_lint::diagnostic::FixSafety::as_str`,
    /// `caixa_arch::InvariantKind::as_str`,
    /// `caixa_arch::ArchVerdict::as_str`,
    /// `caixa_provedor::FerriteRuntime::variant_slug`) already emit
    /// on their canonical `&'static str` projection axis.
    ///
    /// The 16 arms return the kebab-case forms of their `PascalCase`
    /// variant names (`Keyword` → `"keyword"`, `KeywordArg` →
    /// `"keyword-arg"`, `Unchanged` → `"unchanged"`, etc.), matching
    /// the peer closed-set typed-enum canonical byte-string conventions.
    ///
    /// Peer of the [`std::fmt::Display`] and [`AsRef<str>`] impls on
    /// this enum, which both route through this accessor so
    /// `format!("{s}")`, `s.as_str()`, and
    /// `<Semantic as AsRef<str>>::as_ref(&s)` resolve to the same
    /// per-arm byte-string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Keyword => "keyword",
            Self::Symbol => "symbol",
            Self::KeywordArg => "keyword-arg",
            Self::String => "string",
            Self::Number => "number",
            Self::Literal => "literal",
            Self::Comment => "comment",
            Self::Accent => "accent",
            Self::Muted => "muted",
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Hint => "hint",
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Unchanged => "unchanged",
        }
    }

    /// Reverse projection on the [`Semantic`] closed 16-arm enum's
    /// canonical kebab-tag axis — parses a `"keyword"` / `"symbol"` /
    /// `"keyword-arg"` / `"string"` / `"number"` / `"literal"` /
    /// `"comment"` / `"accent"` / `"muted"` / `"error"` / `"warning"` /
    /// `"info"` / `"hint"` / `"added"` / `"removed"` / `"unchanged"` wire
    /// byte-string back to the typed enum, or returns `None` when `s`
    /// lies outside the 16-arm accept-set [`Self::as_str`] emits. The
    /// single `&str → Self` projection every future re-entry point on
    /// the caixa-theme semantic-style axis dispatches through (a future
    /// `feira lint --list-styles` operator-facing enumeration verb
    /// hydrating a per-arm row from a stored kebab identifier back to
    /// the typed enum before rendering, a future `caixa-lsp`-side
    /// per-`SemanticTokenType` re-parse binding a prior
    /// [`Self::as_str`] output back to the typed enum for
    /// per-Semantic-highlight dispatch, a future `caixa.nvim` per-
    /// Semantic highlight-group resolver re-loading a stored kebab
    /// identifier back to the typed enum, a future `blackmatter-shell`
    /// per-arm classname reverse-lookup binding a
    /// `data-semantic="<kebab>"` DOM attribute back to the typed enum
    /// for per-arm style dispatch, a `tracing::field::Value::Str`-arm
    /// structured-log re-loader binding a prior emission's
    /// [`Self::as_str`] output back to the typed enum for cross-run
    /// per-Semantic-paint-histogram diff) would have had to re-inline a
    /// 16-arm `match s` cascade that expressed no compile-time link back
    /// to the substrate primitive.
    ///
    /// Same closed-set-reverse-projection discipline the sibling
    /// [`caixa_core::CaixaKind::from_wire`] (2aa6d23),
    /// [`caixa_core::CaixaDialeto::from_wire`] (d0e65ea),
    /// [`caixa_core::supervisor::RestartStrategy::from_wire`] (4eec29c),
    /// [`caixa_core::supervisor::RestartPolicy::from_wire`] (dd32ccf),
    /// [`caixa_core::aplicacao::PlacementStrategy::from_wire`] (18c7342),
    /// [`caixa_core::dep::DepList::from_wire`] (45ee563),
    /// [`caixa_core::render::PathShapeViolation::from_wire`] (aebd9c6),
    /// `caixa_arch::invariants::InvariantKind::from_wire` (b9e4e61),
    /// `caixa_arch::report::ArchVerdict::from_wire` (6afe564),
    /// `caixa_lint::diagnostic::Severity::from_wire` (5afff0e), and
    /// `caixa_lint::diagnostic::FixSafety::from_wire` (bd505a1) typed
    /// enums carry on the peer wire-side `str → Self` axes — extends
    /// the substrate-wide `(as_str, from_wire)` round-trip family onto
    /// the caixa-theme closed-set fieldless typed-enum axis (the first
    /// closed-set fieldless typed enum on `caixa-theme` to converge on
    /// the reverse-projection discipline), matching the same two-way
    /// `str ↔ Self` round-trip every sibling closed-set enum already
    /// carries. Method-named `from_wire` (not `from_str`) to match the
    /// peer shapes verbatim and side-step a
    /// `clippy::should_implement_trait` lint that a plain `from_str`
    /// name would otherwise trigger without paired
    /// [`std::str::FromStr`] impl scaffolding this axis does not carry
    /// today. Returns `Option<Self>` (rather than `Result<Self, _>`) to
    /// match the peer shapes: the caller picks the diagnostic form
    /// appropriate for its use site (a `feira lint --list-styles` CLI
    /// arg-parse renders its own per-verb error message; a future
    /// admission-webhook rejection body wraps the `None` outcome with
    /// the accepted-set enumeration `Semantic::ALL.iter().map(…)` for
    /// operator diagnostics).
    ///
    /// Pinned load-bearing at the substrate-primitive level by
    /// [`tests::semantic_from_wire_accepts_every_as_str_output`]
    /// (round-trip witness against the peer [`Self::as_str`] axis) and
    /// [`tests::semantic_from_wire_rejects_unknown_byte_strings`]
    /// (rejection witness against silent accept-set widening).
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "keyword" => Some(Self::Keyword),
            "symbol" => Some(Self::Symbol),
            "keyword-arg" => Some(Self::KeywordArg),
            "string" => Some(Self::String),
            "number" => Some(Self::Number),
            "literal" => Some(Self::Literal),
            "comment" => Some(Self::Comment),
            "accent" => Some(Self::Accent),
            "muted" => Some(Self::Muted),
            "error" => Some(Self::Error),
            "warning" => Some(Self::Warning),
            "info" => Some(Self::Info),
            "hint" => Some(Self::Hint),
            "added" => Some(Self::Added),
            "removed" => Some(Self::Removed),
            "unchanged" => Some(Self::Unchanged),
            _ => None,
        }
    }
}

/// [`std::fmt::Display`] routed through [`Semantic::as_str`], so the
/// pretty-printed byte-string every consumer that formats the semantic
/// style as user-facing text lands on (a future `feira lint --list-styles`
/// operator-facing enumeration verb's per-arm line, a future
/// `caixa-lsp` diagnostic-source line naming the offending semantic,
/// a future `caixa.nvim` per-Semantic highlight-group name emitter,
/// a `tracing::field::display(&sem)` structured-log recorder on the
/// paint-side emit path) reaches for the same lifted kebab-case
/// per-arm byte-string [`Semantic::as_str`] returns.
///
/// Peer of the sibling [`std::fmt::Display`] impls on the closed-set
/// typed enums the substrate carries — [`caixa_core::CaixaKind`],
/// [`caixa_core::supervisor::RestartStrategy`],
/// [`caixa_core::supervisor::RestartPolicy`],
/// [`caixa_core::aplicacao::PlacementStrategy`],
/// [`caixa_core::upgrade::UpgradeInstruction`],
/// `caixa_lint::diagnostic::Severity`,
/// `caixa_lint::diagnostic::FixSafety`,
/// `caixa_arch::InvariantKind`, `caixa_arch::ArchVerdict`,
/// `caixa_provedor::FerriteRuntime` — extended to the second-to-last
/// un-lifted `caixa-theme` closed-set fieldless typed enum on the
/// substrate-wide `(as_str, AsRef<str>, Display)` canonical-projection
/// triple ratchet the prior lifts converged onto.
impl std::fmt::Display for Semantic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Substrate-canonical [`AsRef<str>`] projection on the caixa-theme
/// [`Semantic`] closed-set fieldless typed enum — routes through the
/// same [`Semantic::as_str`] `pub const fn` scalar accessor the paired
/// [`std::fmt::Display`] impl already reaches for.
///
/// Peer of the sibling [`AsRef<str>`] impls the substrate carries on
/// [`caixa_core::CaixaKind`], [`caixa_core::CaixaVersion`],
/// [`caixa_core::CaixaDialeto`], [`caixa_core::dep::DepList`],
/// [`caixa_core::supervisor::RestartStrategy`],
/// [`caixa_core::supervisor::RestartPolicy`],
/// [`caixa_core::aplicacao::PlacementStrategy`],
/// [`caixa_core::aplicacao::RateLimitUnit`],
/// `caixa_lint::diagnostic::Severity`,
/// `caixa_arch::InvariantKind`, `caixa_arch::ArchVerdict`,
/// `caixa_provedor::FerriteRuntime` — extends the substrate-wide
/// `(as_str, AsRef<str>, Display)` canonical-projection triple onto
/// the caixa-theme closed-set typed-enum axis, so a future consumer
/// bound through the trait-idiomatic `.as_ref()` (a
/// `HashMap::get::<str>(sem.as_ref())` per-Semantic style-lookup, a
/// future `caixa-lsp` `SemanticTokenType::new(sem.as_ref())`
/// registration site, any `impl AsRef<str>`-bound generic function)
/// reaches the same kebab byte-string [`Semantic::as_str`] returns
/// rather than an open-coded `.as_str()` projection at every
/// wire-up.
impl AsRef<str> for Semantic {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
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
    fn semantic_as_str_returns_canonical_kebab_case_per_arm() {
        // Fail-before-pass-after per-arm byte-string pin on
        // [`Semantic::as_str`] — the substrate-canonical `&'static str`
        // projection every downstream consumer of the closed 16-arm
        // partition reaches through. A future arm rename (a `Symbol` →
        // `Identifier` rebrand tracking a hypothetical LSP-side
        // `SemanticTokenType` reshuffle, an `Accent` → `Highlight`
        // rebrand tracking a `blackmatter-shell` classname rework) that
        // touches the enum arm but forgets to update the paired
        // `as_str` arm — or vice versa — trips this pin at caixa-theme
        // build time rather than surfacing as a downstream
        // `feira lint --list-styles` operator-facing enumeration verb's
        // silently-renamed row far from the two-declaration site.
        //
        // Kebab-case matches the peer [`gen_platform::IsVariant`]-
        // derived kebab discriminant convention the sibling closed-set
        // typed enums already emit on their canonical byte-string
        // projection axis.
        assert_eq!(Semantic::Keyword.as_str(), "keyword");
        assert_eq!(Semantic::Symbol.as_str(), "symbol");
        assert_eq!(Semantic::KeywordArg.as_str(), "keyword-arg");
        assert_eq!(Semantic::String.as_str(), "string");
        assert_eq!(Semantic::Number.as_str(), "number");
        assert_eq!(Semantic::Literal.as_str(), "literal");
        assert_eq!(Semantic::Comment.as_str(), "comment");
        assert_eq!(Semantic::Accent.as_str(), "accent");
        assert_eq!(Semantic::Muted.as_str(), "muted");
        assert_eq!(Semantic::Error.as_str(), "error");
        assert_eq!(Semantic::Warning.as_str(), "warning");
        assert_eq!(Semantic::Info.as_str(), "info");
        assert_eq!(Semantic::Hint.as_str(), "hint");
        assert_eq!(Semantic::Added.as_str(), "added");
        assert_eq!(Semantic::Removed.as_str(), "removed");
        assert_eq!(Semantic::Unchanged.as_str(), "unchanged");
    }

    #[test]
    fn semantic_as_str_projections_are_all_distinct_across_arms() {
        // Fail-before-pass-after pin on the injectivity of
        // [`Semantic::as_str`]'s projection — no two arms may share
        // their canonical kebab byte-string, since a future consumer
        // that keys a per-arm dispatch table off the projection (a
        // `HashMap::<&str, _>::from_iter(Semantic::ALL.iter().map(|s|
        // (s.as_str(), …)))` style-lookup, a future `caixa-lsp`
        // `SemanticTokenType::new(sem.as_ref())` registration table
        // keyed by kebab identifier, a future `feira lint --list-styles`
        // one-row-per-arm enumeration table) would silently collapse
        // the two colliding arms onto one entry, dropping the second
        // insertion. A future arm addition (a `Namespace` tier between
        // `Symbol` and `KeywordArg` for the M4 tatara-lisp module
        // system's qualified-name semantic-token dispatch, a `Deleted`
        // tier for a hard-delete-mark distinct from `Removed` a future
        // 3-way diff surface grows) that lands the arm on the enum and
        // reuses a peer arm's kebab identifier (a copy-paste-derived
        // `"removed"` on the new `Deleted` arm) trips this pin rather
        // than surfacing far from the arm addition site.
        let mut projections: Vec<&'static str> = Semantic::ALL.iter().map(|s| s.as_str()).collect();
        let before = projections.len();
        projections.sort_unstable();
        projections.dedup();
        assert_eq!(
            projections.len(),
            before,
            "Semantic::as_str must be injective across ALL — collisions: \
             {projections:?}",
        );
    }

    #[test]
    fn semantic_display_and_as_ref_str_route_through_as_str_accessor() {
        // Fail-before-pass-after three-path convergence pin on the
        // substrate-wide `(as_str, AsRef<str>, Display)` canonical-
        // projection triple for the caixa-theme [`Semantic`] closed-set
        // fieldless typed enum. For every arm in [`Semantic::ALL`], the
        // paired [`std::fmt::Display`] impl + [`AsRef<str>`] impl + the
        // substrate-canonical [`Semantic::as_str`] `pub const fn`
        // scalar accessor must resolve to the same `&'static str`
        // per arm. Peer of the sibling three-path-convergence pins the
        // substrate carries on the closed-set typed enums the prior
        // lifts converged onto
        // (`restart_strategy_display_routes_through_as_str_helper` /
        // `restart_strategy_as_ref_str_routes_through_as_str_accessor`
        // on [`caixa_core::supervisor::RestartStrategy`],
        // `ferrite_runtime_display_and_as_ref_str_route_through_variant_slug_accessor`
        // on `caixa_provedor::FerriteRuntime`, and the analogous pins
        // on `caixa_lint::Severity` / `caixa_lint::FixSafety` /
        // `caixa_arch::InvariantKind` / `caixa_arch::ArchVerdict`).
        //
        // A future accidental split (a hand-rolled `impl fmt::Display`
        // that shadows this route through a divergent per-arm match, an
        // `impl AsRef<str>` that returns the compiler-derived `Debug`
        // string via `format!("{:?}", self)` — allocating and diverging
        // on every arm — or a `#[serde(rename_all = "…")]` attribute
        // drift that quietly forks the projection) trips this pin at
        // caixa-theme build time rather than surfacing as a downstream
        // consumer's silently-forked per-Semantic dispatch far from the
        // trait-impl declaration site.
        for &sem in Semantic::ALL {
            let via_as_str: &str = sem.as_str();
            let via_display: String = format!("{sem}");
            let via_as_ref: &str = <Semantic as AsRef<str>>::as_ref(&sem);
            assert_eq!(
                via_display, via_as_str,
                "Semantic::{sem:?} — Display routes off `as_str`; got \
                 Display={via_display:?} vs as_str={via_as_str:?}",
            );
            assert_eq!(
                via_as_ref, via_as_str,
                "Semantic::{sem:?} — AsRef<str> routes off `as_str`; got \
                 AsRef={via_as_ref:?} vs as_str={via_as_str:?}",
            );
        }
    }

    #[test]
    fn semantic_as_str_is_usable_in_const_context() {
        // The [`Semantic::as_str`] accessor is declared `pub const fn`,
        // matching the peer closed-set typed enums' canonical
        // `&'static str` projection accessors
        // ([`caixa_core::CaixaKind::as_str`],
        // [`caixa_core::supervisor::RestartStrategy::as_str`],
        // [`caixa_core::aplicacao::PlacementStrategy::as_str`],
        // `caixa_provedor::FerriteRuntime::variant_slug`). Pin the
        // same posture with a `const {}` assertion block so a future
        // accidental downgrade to non-`const` (an added runtime helper
        // reachable only from a non-`const` context) trips at
        // caixa-theme build time rather than surfacing as a downstream
        // `const`-context regression far from the accessor
        // declaration.
        const KEYWORD: &str = Semantic::Keyword.as_str();
        const ERROR: &str = Semantic::Error.as_str();
        const UNCHANGED: &str = Semantic::Unchanged.as_str();
        const { assert!(KEYWORD.as_bytes()[0] == b'k') };
        const { assert!(ERROR.as_bytes()[0] == b'e') };
        const { assert!(UNCHANGED.as_bytes()[0] == b'u') };
    }

    #[test]
    fn semantic_from_wire_accepts_every_as_str_output() {
        // Fail-before-pass-after per-arm accept pin on the newly lifted
        // [`Semantic::from_wire`] reverse projection: every arm in
        // [`Semantic::ALL`] must parse back through `from_wire` when fed
        // its own [`Semantic::as_str`] output, landing on
        // `Some(same_variant)`. A regression that hand-rolled either
        // side's per-arm match without threading through the shared
        // 16-string closed set would silently disagree on any future
        // arm rename (a `Symbol` → `Identifier` rebrand tracking a
        // hypothetical LSP-side `SemanticTokenType` reshuffle, an
        // `Accent` → `Highlight` rebrand tracking a `blackmatter-shell`
        // classname rework) or new arm the theme grows (a `Namespace`
        // tier between `Symbol` and `KeywordArg` for the M4 tatara-lisp
        // module system's qualified-name semantic-token dispatch, a
        // `Deleted` tier for a hard-delete-mark distinct from `Removed`
        // the future 3-way diff surface grows) and this pin flags it at
        // caixa-theme build time rather than at a downstream
        // `feira lint --list-styles` operator-facing enumeration verb's
        // silent tag misclassification.
        //
        // Peer of the sibling
        // `caixa_lint::diagnostic::tests::severity_from_wire_accepts_every_as_str_output`
        // (5afff0e) /
        // `caixa_lint::diagnostic::tests::fix_safety_from_wire_accepts_every_as_str_output`
        // (bd505a1) /
        // `caixa_arch::report::tests::arch_verdict_from_wire_accepts_every_as_str_output`
        // (6afe564) /
        // `caixa_arch::invariants::tests::invariant_kind_from_wire_accepts_every_as_str_output`
        // (b9e4e61) round-trip pins on the peer caixa-lint / caixa-arch
        // closed-set-enum reverse-projection axes, and of the sibling
        // `caixa_core::kind::tests::caixa_kind_wire_round_trips_through_from_wire`
        // (2aa6d23) /
        // `caixa_core::dialeto::tests::caixa_dialeto_from_wire_accepts_every_as_str_output`
        // (d0e65ea) /
        // `caixa_core::aplicacao::tests::placement_strategy_from_wire_accepts_every_lifted_constant`
        // (18c7342) /
        // `caixa_core::dep::tests::dep_list_round_trips_through_as_str_and_from_wire`
        // (45ee563) /
        // `caixa_core::render::tests::path_shape_violation_from_wire_accepts_every_as_str_output`
        // (aebd9c6) round-trip pins on the sibling caixa-core closed-
        // set typed-enum reverse-projection axes.
        for &variant in Semantic::ALL {
            let wire = variant.as_str();
            let parsed = Semantic::from_wire(wire).unwrap_or_else(|| {
                panic!(
                    "Semantic::from_wire({wire:?}) must accept every \
                     Semantic::as_str output — got None for the wire \
                     byte-string of {variant:?}"
                )
            });
            assert_eq!(
                parsed, variant,
                "Semantic::from_wire(Semantic::{variant:?}.as_str()) \
                 must return Semantic::{variant:?} — the (as_str, \
                 from_wire) pair must form a total round-trip on the \
                 closed 16-arm Semantic arm-set",
            );
        }
    }

    #[test]
    fn semantic_from_wire_rejects_unknown_byte_strings() {
        // Rejection pin on the [`Semantic::from_wire`] parser's accept-
        // set: any string outside the 16-arm [`Semantic::as_str`] output
        // set must return `None`. A future accidental widening of the
        // accept-set (a case-insensitive match that accepts `"KEYWORD"`
        // / `"Keyword"`, a silent acceptance of the pre-lift PascalCase
        // Debug-derived shapes `"Keyword"` / `"KeywordArg"` /
        // `"Unchanged"` on the wire axis, a snake_case drift accepting
        // `"keyword_arg"` beside the canonical kebab-case
        // `"keyword-arg"`, a Levenshtein-forgiving arm-lookup that
        // admits `"kewyord"` typos, a silent absorption of a hypothetical
        // future `Namespace` / `Deleted` arm before it lands on the enum
        // and its paired [`Semantic::as_str`] emitter arm) would
        // silently drift the parser's accept-set from the emitter's — a
        // downstream style-report re-loader that bound a prior report's
        // [`Self::as_str`] output back to the typed enum through this
        // parser would then bind a malformed byte-string to a plausibly-
        // wrong typed arm the caller does not route through any
        // fallback, silently misclassifying the reloaded row.
        //
        // Peer of the sibling
        // `caixa_lint::diagnostic::tests::severity_from_wire_rejects_unknown_byte_strings`
        // (5afff0e) /
        // `caixa_lint::diagnostic::tests::fix_safety_from_wire_rejects_unknown_byte_strings`
        // (bd505a1) /
        // `caixa_arch::report::tests::arch_verdict_from_wire_rejects_unknown_byte_strings`
        // (6afe564) /
        // `caixa_arch::invariants::tests::invariant_kind_from_wire_rejects_unknown_byte_strings`
        // (b9e4e61) rejection pins on the peer caixa-lint / caixa-arch
        // axes, and of the sibling
        // `caixa_kind_from_wire_rejects_unknown_byte_strings` (2aa6d23),
        // `caixa_dialeto_from_wire_rejects_unknown_byte_strings`
        // (d0e65ea),
        // `placement_strategy_from_wire_rejects_unknown_byte_strings`
        // (18c7342),
        // `dep_list_from_wire_returns_none_on_unknown_wire_scalar`
        // (45ee563), and
        // `path_shape_violation_from_wire_rejects_unknown_byte_strings`
        // (aebd9c6) rejection pins on the sibling caixa-core axes.
        //
        // The rejection set also covers overlapping-byte-string tags
        // from peer axes: caixa-lint `Severity::as_str` outputs
        // `"error"`/`"warning"`/`"info"`/`"hint"` and caixa-arch
        // `InvariantKind::as_str` outputs `"safety"`/`"compliance"`
        // share zero canonical byte-strings with the widened 16-arm
        // Semantic set here — a widened parser that admitted the peer's
        // arm on the sibling axis would still not admit an arm foreign
        // to the caixa-theme semantic-style discriminator's own accept-
        // set. Yet four peer-axis strings DO overlap with the
        // caixa-theme set here (`Severity`'s
        // `"error"`/`"warning"`/`"info"`/`"hint"` map identically onto
        // the caixa-theme diagnostic-severity sub-region
        // `Semantic::Error`/`Warning`/`Info`/`Hint`) — a widened parser
        // that admitted them under a different arm would collapse the
        // two axes and silently mislabel; the pin excludes those four
        // from the rejection set precisely because they must accept.
        for bad in [
            "",
            " ",
            "Keyword",
            "KEYWORD",
            "Symbol",
            "SYMBOL",
            "KeywordArg",
            "keyword_arg",
            "keywordarg",
            "String",
            "STRING",
            "Number",
            "Literal",
            "Comment",
            "Accent",
            "Muted",
            "Error",
            "ERROR",
            "Warning",
            "WARNING",
            "Info",
            "INFO",
            "Hint",
            "HINT",
            "Added",
            "ADDED",
            "Removed",
            "REMOVED",
            "Unchanged",
            "UNCHANGED",
            "kewyord",
            "sym",
            "kw",
            "str",
            "num",
            "lit",
            "cmt",
            "safe",
            "unsafe",
            "safety",
            "compliance",
            "proven",
            "rejected",
            "namespace",
            "deleted",
            "highlight",
            "identifier",
            "keyword ",
            " keyword",
            "keyword\n",
            "keyword\t",
            "keyword-arg ",
            " keyword-arg",
            "added ",
            " added",
            "unchanged ",
            " unchanged",
        ] {
            assert!(
                Semantic::from_wire(bad).is_none(),
                "Semantic::from_wire({bad:?}) must return None — the \
                 parser's accept-set is exactly the 16 Semantic::as_str \
                 outputs; a widening would silently split the parser's \
                 accept-set from the emitter's arm-set",
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
