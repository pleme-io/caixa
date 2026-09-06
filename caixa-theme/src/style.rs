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

/// Trait-idiomatic reverse projection on the [`Semantic`] closed 16-arm
/// caixa-theme semantic-style axis — routes byte-for-byte through the
/// paired substrate-primitive [`Semantic::from_wire`] `Option<Self>`
/// accessor so every future consumer that binds a canonical semantic
/// tag through the standard-library `.try_into()` / [`TryFrom`] axis
/// (a future `feira lint --semantic=<kebab>` CLI arg-parse that
/// composes into `let sem: Semantic = s.try_into()?`, a future
/// `caixa-lsp` per-token re-parse hydrating a prior
/// [`Semantic::as_str`] output back to the typed enum for
/// `SemanticTokenType` dispatch, a future `caixa.nvim` per-highlight-
/// group re-loader binding a stored kebab byte-string back through the
/// typed enum, a generic `<T: TryFrom<&str>>`-bound theme-overlay
/// re-loader over any of the substrate's closed-set typed enums)
/// reaches the same 16-arm accept-set the sibling
/// [`Semantic::from_wire`] resolver parses through and the sibling
/// [`Semantic::as_str`] emits, rather than an open-coded per-arm
/// `match s { "keyword" => …, "symbol" => …, … _ => … }` cascade whose
/// arm-set has no compile-time link back to the substrate primitive.
///
/// Complements the pre-existing forward-projection triple
/// ([`std::fmt::Display`], [`AsRef<str>`], [`Semantic::as_str`]) with
/// the paired trait-idiomatic reverse-projection axis: Rust-side
/// newtype/typed-enum convention pairs [`AsRef<str>`] with either
/// [`std::str::FromStr`] or [`TryFrom<&str>`] on the same primitive so
/// a caller who can project *out to* a `&str` can also project *in
/// from* one. The [`TryFrom<&str>`] axis is deliberately chosen over
/// [`std::str::FromStr`] to sidestep the `clippy::should_implement_trait`
/// lint the sibling method-named [`Semantic::from_wire`] would trigger
/// under a `FromStr` impl (the same design tradeoff the peer
/// [`caixa_core::CaixaKind`] (3c83606),
/// [`caixa_core::CaixaDialeto`] (bf33136),
/// [`caixa_core::aplicacao::PlacementStrategy`] (6fd00cd),
/// [`caixa_core::supervisor::RestartStrategy`] (5b828ed),
/// [`caixa_core::supervisor::RestartPolicy`] (6fdd0d9),
/// [`caixa_core::aplicacao::WitShape`] (5472902),
/// [`caixa_core::aplicacao::RateLimitUnit`] (bf78400),
/// [`caixa_core::render::PathShapeViolation`] (e67e48a),
/// `caixa_arch::invariants::InvariantKind` (e21a857),
/// `caixa_arch::report::ArchVerdict` (0a4cc45),
/// `caixa_lint::diagnostic::Severity` (a7bf74c), and
/// `caixa_lint::diagnostic::FixSafety` (df86c94) blocks note) — this
/// impl closes the trait-idiomatic reverse axis without disturbing the
/// method-named `from_wire` shape every peer closed-set typed enum
/// already carries.
///
/// `type Error = ()` matches the sibling [`Semantic::from_wire`]'s
/// `Option<Self>` return-shape's deliberate deferral of error typing:
/// the caller picks the diagnostic form appropriate for its use site
/// (a future `feira lint --semantic` CLI arg-parse composes its own
/// per-verb "unknown semantic-style tag: <arg> — accepted: {…}"
/// message enumerating [`Semantic::ALL`], a future admission-webhook
/// rejection body wraps the `Err(())` outcome with the accepted-set
/// enumeration for operator diagnostics, a `Result::map_err` at the
/// call site lifts the axis-error to a per-verb error type). Same
/// shape the peer sibling reverse-projection axes carry. The return-
/// shape uses fully-qualified `<Self as TryFrom<&str>>::Error` because
/// the associated-type name would otherwise collide with the
/// [`Self::Error`] variant on the same primitive (the identical
/// ambiguous-associated-item defect the sibling
/// `impl TryFrom<&str> for Severity` (a7bf74c) already threads around
/// under `#[deny(future_incompatible)]`).
///
/// The paired [`TryFrom<&str>`] impl reaches the same 16-arm accept-
/// set the [`Semantic::from_wire`] resolver dispatches through, so any
/// future arm addition (a `Namespace` tier between [`Self::Symbol`]
/// and [`Self::KeywordArg`] for the M4 tatara-lisp module system's
/// qualified-name semantic-token dispatch, a `Deleted` tier for a
/// hard-delete-mark distinct from [`Self::Removed`] the future 3-way
/// diff surface grows — both trajectory items the sibling
/// [`Semantic::ALL`] doc block already names) grows the trait-
/// idiomatic axis by construction: one caixa-theme edit on
/// [`Semantic::from_wire`] extends both the method-named reverse
/// projection every existing consumer keys off and the trait-
/// idiomatic reverse projection this impl exposes, without a
/// coordinated rewrite across every future `TryFrom<&str>`-bound
/// consumer's arm-set.
///
/// Extends the substrate-wide closed-set-enum trait-idiomatic
/// reverse-projection family ([`caixa_core::CaixaKind`] via 3c83606,
/// [`caixa_core::CaixaDialeto`] via bf33136,
/// [`caixa_core::aplicacao::PlacementStrategy`] via 6fd00cd,
/// [`caixa_core::supervisor::RestartStrategy`] via 5b828ed,
/// [`caixa_core::supervisor::RestartPolicy`] via 6fdd0d9,
/// [`caixa_core::aplicacao::WitShape`] via 5472902,
/// [`caixa_core::aplicacao::RateLimitUnit`] via bf78400,
/// [`caixa_core::render::PathShapeViolation`] via e67e48a,
/// `caixa_arch::invariants::InvariantKind` via e21a857,
/// `caixa_arch::report::ArchVerdict` via 0a4cc45,
/// `caixa_lint::diagnostic::Severity` via a7bf74c, and
/// `caixa_lint::diagnostic::FixSafety` via df86c94) onto the first
/// closed-set fieldless typed enum on the caixa-theme surface — the
/// semantic-style 16-arm accept-set every per-Semantic paint dispatch,
/// every future `caixa-lsp` per-SemanticTokenType wire-up, and every
/// future `caixa.nvim` per-highlight-group re-loader keys off. The
/// thirteenth peer on the substrate surface, and the first inside
/// `caixa-theme`; with `caixa_provedor::FerriteRuntime` remaining as
/// the only outside-caixa-core closed-set fieldless typed enum whose
/// trait-idiomatic axis is still open.
///
/// Pinned load-bearing by
/// [`tests::semantic_try_from_str_routes_through_from_wire_accessor`]
/// (byte-parity pin against [`Semantic::from_wire`] across the 16-arm
/// accept-set) and
/// [`tests::semantic_try_from_str_rejects_unknown_byte_strings`]
/// (rejection witness against silent accept-set widening).
impl TryFrom<&str> for Semantic {
    type Error = ();

    fn try_from(s: &str) -> Result<Self, <Self as TryFrom<&str>>::Error> {
        Self::from_wire(s).ok_or(())
    }
}

/// Standard-library trait-idiomatic forward projection on the
/// [`Semantic`] closed 16-arm caixa-theme semantic-style axis. Routes
/// byte-for-byte through the paired substrate-primitive
/// [`Semantic::as_str`] `pub const fn` accessor so
/// `<&'static str>::from(sem)` / `sem.into::<&'static str>()` reaches
/// the same 16-arm canonical-lowercase kebab emit-set the sibling
/// method-named accessor dispatches through and the sibling
/// [`std::fmt::Display for Semantic`] / [`AsRef<str> for Semantic`]
/// impls also route through.
///
/// Extends the substrate-wide closed-set-enum trait-idiomatic
/// forward-projection family
/// ([`caixa_core::supervisor::RestartStrategy`] via 523157d,
/// [`caixa_core::supervisor::RestartPolicy`] via 9fb37d0,
/// [`caixa_core::CaixaKind`] via edb827b,
/// [`caixa_core::CaixaDialeto`] via c189a6f,
/// [`caixa_core::aplicacao::PlacementStrategy`] via afa3562,
/// [`caixa_core::aplicacao::WitShape`] via 56998ec,
/// [`caixa_core::aplicacao::RateLimitUnit`] via 7fdfbf4,
/// [`caixa_core::render::PathShapeViolation`] via 070a6de,
/// [`caixa_arch::invariants::InvariantKind`] via f2ca7bc,
/// [`caixa_arch::report::ArchVerdict`] via d4559cb,
/// `caixa_lint::diagnostic::Severity` via 5cc3b8b, and
/// `caixa_lint::diagnostic::FixSafety` via 2a56127) onto the first
/// closed-set fieldless typed enum on the caixa-theme surface — the
/// semantic-style 16-arm accept-set every per-Semantic paint dispatch,
/// every future `caixa-lsp` per-`SemanticTokenType` wire-up, every
/// future `caixa.nvim` per-highlight-group re-loader, and every future
/// `blackmatter-shell` per-arm `data-semantic="<kebab>"` DOM emission
/// keys off. The thirteenth peer on the substrate surface, closing the
/// caixa-theme closed-set fieldless typed-enum axis onto the trait-
/// idiomatic forward-projection axis and matching the paired trait-
/// idiomatic reverse-projection axis (already closed via bd7da69 on
/// this enum), with only `caixa_provedor::FerriteRuntime` remaining as
/// the last outside-caixa-core closed-set fieldless typed enum whose
/// trait-idiomatic forward axis is still open.
///
/// Pairs with the sibling [`TryFrom<&str> for Semantic`] impl (bd7da69)
/// to close the two-way `Self ↔ &'static str` round-trip on the
/// trait-idiomatic axis pair, mirroring the pre-existing method-named
/// [`Semantic::as_str`] + [`Semantic::from_wire`] pair on the
/// substrate-primitive axis pair.
///
/// Return type is `&'static str` by construction — every
/// [`Semantic::as_str`] arm resolves to an inline `"keyword"` /
/// `"symbol"` / `"keyword-arg"` / `"string"` / `"number"` /
/// `"literal"` / `"comment"` / `"accent"` / `"muted"` / `"error"` /
/// `"warning"` / `"info"` / `"hint"` / `"added"` / `"removed"` /
/// `"unchanged"` `&'static str` literal, so the trait's return-type
/// promise is upheld structurally without a [`String::leak`] cast or a
/// per-arm inline literal outside the paired [`Semantic::as_str`]
/// dispatch.
///
/// The paired [`Semantic::as_str`] accessor's 16-arm emit-set is the
/// single source of truth — every future arm addition (a `Namespace`
/// tier between [`Self::Symbol`] and [`Self::KeywordArg`] for the M4
/// tatara-lisp module system's qualified-name semantic-token dispatch,
/// a `Deleted` tier for a hard-delete-mark distinct from
/// [`Self::Removed`] the future 3-way diff surface grows — both
/// trajectory items the sibling [`Semantic::ALL`] doc block already
/// names) grows the trait-idiomatic forward axis by construction: one
/// caixa-theme edit on [`Semantic::as_str`] extends every one of the
/// sibling forward-projection paths ([`std::fmt::Display`],
/// [`AsRef<str>`], [`Semantic::as_str`] itself, and this
/// [`From<Self> for &'static str`]) without a coordinated rewrite
/// across every future `Into<&'static str>`-bound consumer's arm-set.
///
/// Pinned load-bearing by
/// [`tests::semantic_from_into_static_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`Semantic::as_str`] across the 16-arm
/// emit-set, plus a `const`-context materialization witness for the
/// `&'static str` lifetime promise routed through the paired
/// [`Semantic::as_str`] `pub const fn` accessor, plus a paired
/// `.into()` shape assertion covering the blanket-derived
/// `Into<&'static str>` shape) and
/// [`tests::semantic_from_into_static_str_and_as_str_partition_the_emit_set`]
/// (partition pin asserting `<&'static str as From<Semantic>>::from`
/// and [`Semantic::as_str`] agree on every arm, plus a two-way direct
/// round-trip witness through the paired trait-idiomatic
/// [`TryFrom<&str>`] axis that closes the two-way
/// `Self ↔ &'static str` round-trip on the trait-idiomatic axis pair
/// — the emit-side [`Semantic::as_str`] and the parse-side
/// [`Semantic::from_wire`] dispatch on the same 16 inline canonical-
/// lowercase kebab byte-strings by construction, so round-tripping
/// composes the two trait impls directly).
impl From<Semantic> for &'static str {
    fn from(sem: Semantic) -> &'static str {
        sem.as_str()
    }
}

/// Trait-idiomatic *borrowed-input* forward projection on the
/// [`Semantic`] closed 16-arm caixa-theme semantic-style axis onto the
/// `&'static str` axis — the borrowed-input companion to the paired
/// owned-input [`From<Semantic> for &'static str`] impl immediately
/// above. Routes byte-for-byte through the same substrate-primitive
/// [`Semantic::as_str`] `pub const fn` accessor so every consumer that
/// binds a `&Semantic` through the standard-library `.into()` /
/// [`From<&Self> for &'static str`] axis (a
/// `Semantic::ALL.iter().map(<&'static str>::from).collect::<Vec<_>>()`
/// per-arm accept-set materializer — whose iterator over
/// `&'static [Semantic]` yields `&Semantic`, not `Semantic`, so the
/// owned-input [`From<Semantic>`] axis alone forces every call site
/// through an explicit `.copied()` / dereference / [`Copy`]-bound
/// restatement rather than the direct trait-idiomatic projection; a
/// future `feira lint --list-styles` operator-facing enumeration verb
/// composed via `Semantic::ALL.iter().map(Into::into)`; a future
/// `caixa-lsp` per-`SemanticTokenType` registration walk that borrows
/// `&Semantic` off a stored per-style row; a future `caixa.nvim` per-
/// highlight-group re-loader that borrows `&Semantic` off the loaded
/// theme-overlay table; a future
/// `HashMap::<&'static str, _>::from_iter(Semantic::ALL.iter().map(
///     |sem| (<&'static str>::from(sem), 0)))` per-Semantic-paint
/// histogram seed a future `blackmatter-shell` per-arm
/// `data-semantic="<kebab>"` DOM-attribute emit path composes) reaches
/// the same 16-arm `"keyword"` / `"symbol"` / `"keyword-arg"` /
/// `"string"` / `"number"` / `"literal"` / `"comment"` / `"accent"` /
/// `"muted"` / `"error"` / `"warning"` / `"info"` / `"hint"` /
/// `"added"` / `"removed"` / `"unchanged"` canonical-lowercase kebab
/// emit-set the paired owned-input [`From<Semantic> for &'static str`],
/// the sibling [`std::fmt::Display`], [`AsRef<str>`], and
/// [`Semantic::as_str`] surfaces already return.
///
/// Fifteenth and final peer on the substrate-wide trait-idiomatic
/// *borrowed-input* `&'static str`-returning forward-projection family
/// already carried by [`caixa_core::dep::DepList`] (64aa742,
/// first-mover), [`caixa_core::CaixaKind`],
/// [`caixa_core::CaixaDialeto`],
/// [`caixa_core::supervisor::RestartStrategy`],
/// [`caixa_core::supervisor::RestartPolicy`],
/// [`caixa_core::aplicacao::PlacementStrategy`],
/// [`caixa_core::aplicacao::WitShape`],
/// [`caixa_core::aplicacao::RateLimitUnit`],
/// [`caixa_core::render::PathShapeViolation`] (cdf4e95, first render-
/// side arm), [`caixa_arch::invariants::InvariantKind`] (238d886,
/// first outside-`caixa-core` arm), [`caixa_arch::report::ArchVerdict`]
/// (73bda50), `caixa_lint::diagnostic::Severity` (2b9003f),
/// `caixa_lint::diagnostic::FixSafety` (d8769ab), and
/// `caixa_provedor::FerriteRuntime` (676d693). Rust's `From` trait does
/// not auto-derive the `From<&Self>` sibling from a `From<Self>` impl
/// (the blanket `impl<T, U> From<&T> for U where T: Copy, U: From<T>`
/// does not exist in `core`), so every closed-set typed enum that
/// carries the owned-input axis but not the borrowed-input axis forces
/// every borrowed-input call site through a `.copied()` /
/// `<&'static str>::from(*sem)` / `sem.as_str()` detour whose type
/// bounds have no compile-time link to the substrate primitive.
/// Lifting the borrowed-input axis on the caixa-theme semantic-style
/// 16-arm closed-set fieldless typed enum closes that gap on the same
/// trajectory the paired owned-input axis
/// ([`impl From<Semantic> for &'static str`] immediately above) already
/// opened, and closes the trait-idiomatic *borrowed-input*
/// `&'static str`-returning axis on the last remaining substrate-wide
/// closed-set fieldless typed enum whose borrowed-input axis was still
/// open — the substrate-wide 2×2-completion campaign now covers every
/// closed-set fieldless typed enum on the caixa surface on the
/// borrowed-input `&'static str`-returning axis.
///
/// Pinned load-bearing by
/// [`tests::semantic_from_borrowed_into_static_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`Semantic::as_str`] across the 16-arm
/// emit-set via a borrowed input, plus a `const`-context
/// materialization witness for the `&'static str` lifetime promise)
/// and
/// [`tests::semantic_from_owned_and_borrowed_into_static_str_agree_on_every_arm`]
/// (cross-axis partition pin against the paired owned-input
/// [`From<Semantic> for &'static str`] impl, plus a
/// `.iter().map(Into::into)` pipe witness over [`Semantic::ALL`] whose
/// iterator yields `&Semantic` by construction so this borrowed-input
/// axis is what routes the pipe through the substrate-primitive
/// accessor without a spurious `Copy` deref).
impl From<&Semantic> for &'static str {
    fn from(sem: &Semantic) -> &'static str {
        sem.as_str()
    }
}

/// Trait-idiomatic *owned-input, owned-`String` output* forward
/// projection on the [`Semantic`] closed 16-arm caixa-theme
/// semantic-style axis — the owned-`String` companion to the paired
/// [`From<Semantic> for &'static str`] and
/// [`From<&Semantic> for &'static str`] siblings immediately above.
/// Routes byte-for-byte through the substrate-primitive
/// [`Semantic::as_str`] `pub const fn` accessor via
/// [`str::to_owned`] so every consumer that binds a [`Semantic`]
/// through the standard-library `.into()` / [`From<Self> for String`]
/// axis (a `let key: String = sem.into();`-shaped downstream call
/// site; a future `serde_json::Value::String(sem.into())` structured-
/// payload composer where the `Value::String` arm typing demands an
/// owned [`String`] and the sibling `&'static str`-returning axes
/// force an explicit `.to_owned()` / [`String::from`] restatement at
/// every call site; a future
/// `HashMap::<String, Semantic>::from_iter` per-semantic lookup on
/// a future `caixa-lsp` per-`SemanticTokenType` registration path
/// where the map's key type is owned [`String`] rather than
/// `&'static str`; a future
/// [`std::borrow::Cow::<'static, str>::Owned(sem.into())`] composer
/// on a future `feira lint --list-styles` operator-facing enumeration
/// verb's per-arm row where an owned [`Cow`] arm typing rules;
/// a future `caixa.nvim` per-highlight-group re-loader that stores
/// the per-Semantic kebab identifier as an owned [`String`] on the
/// theme-overlay table; a future `blackmatter-shell` per-arm
/// `data-semantic="<kebab>"` DOM emission whose serializer's
/// [`Serialize`] impl on [`String`] owns the emit-path) reaches the
/// same 16-arm `"keyword"` / `"symbol"` / `"keyword-arg"` /
/// `"string"` / `"number"` / `"literal"` / `"comment"` / `"accent"` /
/// `"muted"` / `"error"` / `"warning"` / `"info"` / `"hint"` /
/// `"added"` / `"removed"` / `"unchanged"` canonical-lowercase kebab
/// emit-set the paired `&'static str`-returning axes, the sibling
/// [`std::fmt::Display`], [`AsRef<str>`], and [`Semantic::as_str`]
/// surfaces already return — no `.to_owned()` /
/// `String::from(sem.as_str())` detour whose type bounds have no
/// compile-time link to the substrate primitive.
///
/// Rust's standard library does not carry a blanket
/// `impl<T: AsRef<str>> From<T> for String` (nor an
/// `impl<T: fmt::Display> From<T> for String`), so every closed-set
/// typed enum that carries the paired [`AsRef<str>`] /
/// [`std::fmt::Display`] / [`From<Self> for &'static str`] /
/// [`From<&Self> for &'static str`] quadruple but not the owned-
/// `String` axis forces every owned-string call site through the
/// detour above. This lift closes that axis on the sole closed-set
/// fieldless typed enum on the caixa-theme surface (the semantic-
/// style 16-arm axis), extending the substrate-wide
/// `{Self, &Self} × {&'static str, String}` 2×2-completion campaign
/// onto the sixth outside-`caixa-core` closed-set fieldless typed
/// enum on the caixa surface — matching the trajectory each of the
/// twelve prior peer enums —
/// [`caixa_core::supervisor::RestartStrategy`] (7baa18a, first-mover
/// on this axis), [`caixa_core::supervisor::RestartPolicy`] (7851725),
/// [`caixa_core::CaixaKind`] (231a18c),
/// [`caixa_core::CaixaDialeto`] (88942cd),
/// [`caixa_core::dep::DepList`] (32b0ee8),
/// [`caixa_core::aplicacao::PlacementStrategy`] (1154c2f),
/// [`caixa_core::aplicacao::WitShape`] (79a8723),
/// [`caixa_core::aplicacao::RateLimitUnit`] (c7d687d),
/// [`caixa_core::render::PathShapeViolation`] (6e0479a, first render-
/// side arm), [`caixa_arch::invariants::InvariantKind`] (1afd8d5,
/// first outside-`caixa-core` arm — the paired severity-classification
/// axis on the caixa-arch invariant-kind closed-set enum),
/// [`caixa_arch::report::ArchVerdict`] (cc80a53, the paired verdict-
/// outcome axis on the sibling caixa-arch closed-set enum),
/// `caixa_lint::diagnostic::Severity` (4635d4e),
/// `caixa_lint::diagnostic::FixSafety` (e4d73c6), and
/// `caixa_provedor::FerriteRuntime` (1e14fde, the paired ferrite-
/// runtime axis on the sole caixa-provedor closed-set enum) —
/// followed on the same 2×2-completion campaign.
///
/// Pinned load-bearing by
/// [`tests::semantic_from_into_owned_string_routes_through_as_str_accessor`]
/// (byte-parity pin against [`Semantic::as_str`] across the 16-arm
/// emit-set via the owned-`String` surface, plus a blanket-derived
/// `Into<String>` shape witness) and
/// [`tests::semantic_from_into_owned_string_and_static_str_agree_on_every_arm`]
/// (cross-axis partition pin against the paired owned-input
/// `&'static str`-returning [`From<Semantic> for &'static str`] impl
/// and the [`ToString::to_string`]-through-[`std::fmt::Display`]
/// surface, plus a `.iter().copied().map(String::from)` pipe witness
/// over [`Semantic::ALL`], plus a direct `Self → String → Self`
/// round-trip witness through the paired [`TryFrom<&str>`] axis on
/// the owned-[`String`]'s [`String::as_str`] borrow).
impl From<Semantic> for String {
    fn from(sem: Semantic) -> String {
        sem.as_str().to_owned()
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

    #[test]
    fn semantic_try_from_str_routes_through_from_wire_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl TryFrom<&str> for Semantic` — asserts the standard-
        // library trait impl and the substrate-primitive
        // [`super::Semantic::from_wire`] `Option<Self>` accessor
        // resolve to the same 16-arm accept-set across every arm the
        // exhaustive [`super::Semantic::ALL`] slice enumerates. Any
        // future silent detour that routes the trait impl through a
        // divergent projection (a per-arm inline `match s { "keyword"
        // => Ok(Self::Keyword), … }` re-inlining that opens a
        // compile-time link to the un-lifted arm-literal, a silent
        // case-fold that admits `"Keyword"` / `"KEYWORD"` and would
        // collide the canonical-lowercase accept-set the emitter
        // dispatches on) trips at caixa-theme test time under
        // `assert_eq!` rather than at a downstream
        // `impl TryFrom<&str>`-bound consumer's silent split. Sweeps
        // every one of the 16 arms [`super::Semantic::ALL`] carries so
        // no arm's projection is covered only by the sibling method-
        // named `from_wire` path.
        //
        // Peer of the sibling
        // [`caixa_core::kind::tests::caixa_kind_try_from_str_routes_through_from_wire_accessor`]
        // (3c83606),
        // [`caixa_core::dialeto::tests::caixa_dialeto_try_from_str_routes_through_from_wire_accessor`]
        // (bf33136),
        // `placement_strategy_try_from_str_routes_through_from_wire_accessor`
        // (6fd00cd),
        // `rate_limit_unit_try_from_str_routes_through_from_suffix_accessor`
        // (bf78400),
        // `path_shape_violation_try_from_str_routes_through_from_wire_accessor`
        // (e67e48a),
        // `caixa_arch::invariants::tests::invariant_kind_try_from_str_routes_through_from_wire_accessor`
        // (e21a857),
        // `caixa_arch::report::tests::arch_verdict_try_from_str_routes_through_from_wire_accessor`
        // (0a4cc45),
        // `caixa_lint::diagnostic::tests::severity_try_from_str_routes_through_from_wire_accessor`
        // (a7bf74c), and
        // `caixa_lint::diagnostic::tests::fix_safety_try_from_str_routes_through_from_wire_accessor`
        // (df86c94) — extends the trait-idiomatic reverse-projection
        // axis onto the first closed-set fieldless typed enum on the
        // caixa-theme surface (the semantic-style axis).
        for &variant in Semantic::ALL {
            let wire = variant.as_str();
            assert_eq!(
                <Semantic as TryFrom<&str>>::try_from(wire),
                Ok(variant),
                "TryFrom<&str> impl on Semantic must round-trip \
                 Semantic::{variant:?}.as_str() = {wire:?} back to \
                 Ok(Semantic::{variant:?}) — divergence from \
                 Semantic::from_wire signals a silent detour off the \
                 substrate-primitive accessor",
            );
            assert_eq!(
                <Semantic as TryFrom<&str>>::try_from(wire).ok(),
                Semantic::from_wire(wire),
                "TryFrom<&str> ok()-projection on {wire:?} must \
                 byte-equal Semantic::from_wire on the same input",
            );
        }
    }

    #[test]
    fn semantic_try_from_str_rejects_unknown_byte_strings() {
        // Rejection witness on the `impl TryFrom<&str> for Semantic` —
        // sweeps a candidate set of byte-strings outside the 16-arm
        // canonical-lowercase kebab wire accept-set the sibling
        // [`super::Semantic::as_str`] emits and asserts every one
        // lands on `Err(())`, so a future accidental widening of the
        // trait impl's accept-set (a stray additional
        // `_ if s.eq_ignore_ascii_case("keyword") => Ok(…)` case-fold
        // path, a silent acceptance of the pre-lift PascalCase Debug-
        // derived shapes `"Keyword"` / `"Symbol"` / `"KeywordArg"` on
        // the wire axis, a Levenshtein-forgiving arm-lookup that
        // admits `"kwd"` / `"sym"` / `"kw"` typos — the exact form a
        // `format!("{:?}", …).to_lowercase()` round-trip on the paired
        // [`std::fmt::Debug`] derive would otherwise land on) trips at
        // caixa-theme test time. The candidate set includes the empty
        // string, whitespace-only padding, uppercase / PascalCase
        // rebrand candidates, Levenshtein-neighbor typos, sibling
        // closed-set-enum canonical tags not shared with this axis
        // (peer `caixa_lint::diagnostic::FixSafety::as_str` two-arm
        // `"safe"` / `"unsafe"`, peer `caixa_arch::InvariantKind::as_str`
        // three-arm `"safety"` / `"compliance"`, peer
        // `caixa_arch::ArchVerdict::as_str` two-arm `"proven"` /
        // `"rejected"`), the trajectory-item candidates
        // (`"namespace"`, `"deleted"`) the sibling [`Semantic::ALL`]
        // doc block already names, whitespace-padded canonical tags,
        // and CamelCase spellings of the multi-word `KeywordArg`
        // variant (`"KeywordArg"`, `"keywordarg"`, `"keyword_arg"`,
        // `"keyword.arg"`) that would silently admit
        // if the accept-set widened to a case-fold or separator-
        // normalization rule.
        //
        // Peer of the sibling
        // `caixa_kind_try_from_str_rejects_unknown_byte_strings`
        // (3c83606),
        // `caixa_dialeto_try_from_str_rejects_unknown_byte_strings`
        // (bf33136),
        // `rate_limit_unit_try_from_str_rejects_unknown_byte_strings`
        // (bf78400),
        // `path_shape_violation_try_from_str_rejects_unknown_byte_strings`
        // (e67e48a),
        // `invariant_kind_try_from_str_rejects_unknown_byte_strings`
        // (e21a857),
        // `arch_verdict_try_from_str_rejects_unknown_byte_strings`
        // (0a4cc45),
        // `severity_try_from_str_rejects_unknown_byte_strings`
        // (a7bf74c), and
        // `fix_safety_try_from_str_rejects_unknown_byte_strings`
        // (df86c94) rejection pins on the sibling closed-set typed-
        // enum trait-idiomatic reverse-projection axes.
        for bad in [
            "",
            " ",
            "Keyword",
            "KEYWORD",
            "Symbol",
            "KeywordArg",
            "keywordarg",
            "keyword_arg",
            "keyword.arg",
            "String",
            "Number",
            "Literal",
            "Comment",
            "Accent",
            "Muted",
            "Error",
            "ERROR",
            "Warning",
            "Info",
            "Hint",
            "Added",
            "Removed",
            "Unchanged",
            "kwd",
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
            assert_eq!(
                <Semantic as TryFrom<&str>>::try_from(bad),
                Err(()),
                "TryFrom<&str> for Semantic({bad:?}) must return \
                 Err(()) — the trait impl's accept-set is exactly the \
                 16 Semantic::as_str outputs; a widening would \
                 silently split the trait impl's accept-set from the \
                 emitter's arm-set",
            );
        }
    }

    #[test]
    fn semantic_try_from_str_and_from_wire_partition_the_accept_set() {
        // Cross-axis partition pin: the trait-idiomatic
        // [`TryFrom<&str>`] and the method-named
        // [`super::Semantic::from_wire`] projections must return
        // equivalent decisions on every input — the trait impl's
        // `.ok()` project-out from `Result<Self, ()>` and the method's
        // `Option<Self>` return must byte-equal each other on both
        // accepts and rejects. A future silent bifurcation (the trait
        // impl gaining a case-fold path the method does not carry, the
        // method gaining a synonym alias the trait impl does not
        // honor) trips at caixa-theme test time under a single pin
        // rather than at a downstream generic-bound consumer that
        // dispatches through one axis while a peer dispatches through
        // the other. Sweeps both the 16-arm accept-set (via
        // [`super::Semantic::ALL`] threaded through
        // [`super::Semantic::as_str`]) and a canonical rejection
        // sample so both halves of the partition are covered. Peer of
        // the sibling
        // `severity_try_from_str_and_from_wire_partition_the_accept_set`
        // (a7bf74c) and
        // `fix_safety_try_from_str_and_from_wire_partition_the_accept_set`
        // (df86c94) partition pins.
        for &variant in Semantic::ALL {
            let wire = variant.as_str();
            assert_eq!(
                <Semantic as TryFrom<&str>>::try_from(wire).ok(),
                Semantic::from_wire(wire),
                "TryFrom<&str>::ok() and from_wire must agree on \
                 Semantic::{variant:?}.as_str() = {wire:?}",
            );
        }
        for bad in [
            "",
            "Keyword",
            "unknown",
            "safety",
            "safe",
            "proven",
            "namespace",
            "keywordarg",
        ] {
            assert_eq!(
                <Semantic as TryFrom<&str>>::try_from(bad).ok(),
                Semantic::from_wire(bad),
                "TryFrom<&str>::ok() and from_wire must agree on the \
                 rejection outcome for {bad:?}",
            );
        }
    }

    #[test]
    fn semantic_from_into_static_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<Semantic> for &'static str` — asserts the standard-
        // library trait impl and the substrate-primitive
        // [`super::Semantic::as_str`] `pub const fn` accessor resolve to
        // the same 16-arm canonical-lowercase kebab emit-set across every
        // arm the exhaustive [`super::Semantic::ALL`] slice enumerates.
        // Any future silent detour that routes the trait impl through a
        // divergent projection (a per-arm inline `match sem { Keyword =>
        // "keyword", … }` re-inlining that opens a compile-time link to
        // the un-lifted arm-literal outside the paired
        // [`super::Semantic::as_str`] dispatch, a swap onto a
        // `format!("{:?}", …).to_lowercase()` round-trip through the
        // `#[derive(Debug)]` output whose stability is *not* guaranteed
        // and would silently reroute the semantic-style tag through a
        // stale byte-string with no downstream signal until an operator
        // scrolled the theme-paint terminal, a `#[serde(rename_all = "…")]`
        // attribute drift that quietly forks one axis) trips at
        // caixa-theme test time under `assert_eq!` rather than at a
        // downstream `impl Into<&'static str>`-bound consumer's silent
        // split. Sweeps every one of the 16 arms
        // [`super::Semantic::ALL`] carries so no arm's projection is
        // covered only by the sibling method-named `as_str` /
        // [`std::fmt::Display`] / [`AsRef<str>`] paths. Materializes
        // three `<&'static str as From<Semantic>>::from` outputs in
        // `const`-shape bindings against the paired
        // [`super::Semantic::as_str`] `pub const fn` accessor to make
        // the `'static` lifetime promise a build-time invariant — a
        // future accidental downgrade of any arm's inline canonical-
        // lowercase kebab byte-string to a non-`&'static str` (a
        // `String::leak()`-produced return, a `Box::leak`-cast, an
        // intermediate lifetime-erasing helper) trips at caixa-theme
        // build time rather than at a downstream `'static`-bound
        // consumer.
        //
        // Peer of the sibling
        // [`caixa_core::supervisor::tests::restart_strategy_from_into_static_str_routes_through_as_str_accessor`]
        // (523157d),
        // [`caixa_core::supervisor::tests::restart_policy_from_into_static_str_routes_through_as_str_accessor`]
        // (9fb37d0),
        // [`caixa_core::kind::tests::caixa_kind_from_into_static_str_routes_through_as_str_accessor`]
        // (edb827b),
        // [`caixa_core::dialeto::tests::caixa_dialeto_from_into_static_str_routes_through_as_str_accessor`]
        // (c189a6f),
        // [`caixa_core::aplicacao::tests::placement_strategy_from_into_static_str_routes_through_as_str_accessor`]
        // (afa3562),
        // [`caixa_core::aplicacao::tests::wit_shape_from_into_static_str_routes_through_as_str_accessor`]
        // (56998ec),
        // [`caixa_core::aplicacao::tests::rate_limit_unit_from_into_static_str_routes_through_as_suffix_accessor`]
        // (7fdfbf4),
        // [`caixa_core::render::tests::path_shape_violation_from_into_static_str_routes_through_as_str_accessor`]
        // (070a6de),
        // `caixa_arch::invariants::tests::invariant_kind_from_into_static_str_routes_through_as_str_accessor`
        // (f2ca7bc),
        // `caixa_arch::report::tests::arch_verdict_from_into_static_str_routes_through_as_str_accessor`
        // (d4559cb),
        // `caixa_lint::diagnostic::tests::severity_from_into_static_str_routes_through_as_str_accessor`
        // (5cc3b8b), and
        // `caixa_lint::diagnostic::tests::fix_safety_from_into_static_str_routes_through_as_str_accessor`
        // (2a56127) pins on the sibling closed-set typed-enum forward-
        // projection axes — extends the trait-idiomatic forward-
        // projection axis onto the first closed-set fieldless typed
        // enum on the caixa-theme surface (the semantic-style axis),
        // leaving `caixa_provedor::FerriteRuntime` as the last outside-
        // caixa-core closed-set fieldless typed enum whose trait-
        // idiomatic forward axis is still open.
        const KEYWORD: &str = Semantic::Keyword.as_str();
        const KEYWORD_ARG: &str = Semantic::KeywordArg.as_str();
        const UNCHANGED: &str = Semantic::Unchanged.as_str();
        for &variant in Semantic::ALL {
            let via_trait: &'static str = <&'static str as From<Semantic>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<Semantic> for &'static str impl must round-trip \
                 Semantic::{variant:?} to the same canonical-lowercase \
                 kebab byte-string Semantic::as_str returns — divergence \
                 signals a silent detour off the substrate-primitive \
                 accessor"
            );
            let via_into: &'static str = variant.into();
            assert_eq!(
                via_into, via_method,
                "Into<&'static str>::into on Semantic::{variant:?} must \
                 byte-equal Semantic::as_str on the same input — the \
                 blanket-derived Into shape must resolve to the same \
                 as_str dispatch as the explicit From impl"
            );
        }
        assert_eq!(
            [KEYWORD, KEYWORD_ARG, UNCHANGED],
            ["keyword", "keyword-arg", "unchanged"],
            "const-context Semantic::as_str must resolve to the \
             canonical-lowercase kebab byte-strings — a future \
             accidental downgrade of any arm to a non-const or non-\
             static byte-string breaks the `&'static str`-lifetime \
             promise the paired From<Semantic> for &'static str impl \
             carries by construction"
        );
    }

    #[test]
    fn semantic_from_into_static_str_and_as_str_partition_the_emit_set() {
        // Cross-axis partition pin: the paired trait-idiomatic
        // `From<Semantic> for &'static str` forward projection and the
        // method-named [`super::Semantic::as_str`] forward projection
        // must resolve identically on *every* arm, not just the ones
        // named in the primary byte-parity pin above. Sweeps every
        // [`super::Semantic::ALL`] arm and asserts the trait's
        // `From::from` output byte-equals the method-named accessor's
        // return-value on each, locking the two forward-projection paths
        // together by construction so any future detour (a stray `From`
        // special-case that lands on a divergent per-arm literal outside
        // the paired `as_str` dispatch, a hypothetical rebrand touching
        // one axis without the other) trips at caixa-theme test time.
        //
        // Peer of the sibling forward-projection partition pins
        // [`caixa_core::supervisor::tests::restart_strategy_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (523157d),
        // [`caixa_core::supervisor::tests::restart_policy_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (9fb37d0),
        // [`caixa_core::kind::tests::caixa_kind_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (edb827b),
        // [`caixa_core::dialeto::tests::caixa_dialeto_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (c189a6f),
        // [`caixa_core::aplicacao::tests::placement_strategy_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (afa3562),
        // [`caixa_core::aplicacao::tests::wit_shape_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (56998ec),
        // [`caixa_core::aplicacao::tests::rate_limit_unit_from_into_static_str_and_as_suffix_partition_the_emit_set`]
        // (7fdfbf4),
        // [`caixa_core::render::tests::path_shape_violation_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (070a6de),
        // `caixa_arch::invariants::tests::invariant_kind_from_into_static_str_and_as_str_partition_the_emit_set`
        // (f2ca7bc),
        // `caixa_arch::report::tests::arch_verdict_from_into_static_str_and_as_str_partition_the_emit_set`
        // (d4559cb),
        // `caixa_lint::diagnostic::tests::severity_from_into_static_str_and_as_str_partition_the_emit_set`
        // (5cc3b8b), and
        // `caixa_lint::diagnostic::tests::fix_safety_from_into_static_str_and_as_str_partition_the_emit_set`
        // (2a56127) — extends the round-trip discipline onto the first
        // closed-set fieldless typed enum on the caixa-theme surface,
        // closing the two-way `Self ↔ &'static str` round-trip on the
        // trait-idiomatic pair (`From<Self> for &'static str` +
        // `TryFrom<&str> for Self`) as well as the pre-existing method-
        // named pair (`as_str` + `from_wire`).
        for &variant in Semantic::ALL {
            let via_trait: &'static str = <&'static str as From<Semantic>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<Semantic> for &'static str and Semantic::as_str \
                 must resolve identically on Semantic::{variant:?} — \
                 divergence signals the two forward-projection paths \
                 have drifted onto different emit-sets"
            );
        }
        // Round-trip witness: every arm's forward `From` output re-parses
        // through the paired trait-idiomatic `TryFrom<&str>` back to the
        // original variant. Closes the two-way `Semantic ↔ &'static str`
        // round-trip on the trait-idiomatic axis pair directly (no wire-
        // vocab intermediate — the emit-side [`super::Semantic::as_str`]
        // and the parse-side [`super::Semantic::from_wire`] dispatch on
        // the same 16 inline canonical-lowercase kebab byte-strings by
        // construction, so round-tripping through the paired
        // `From<Self> for &'static str` + `TryFrom<&str> for Self` trait
        // impls composes to the identity on `Semantic::ALL`).
        for &variant in Semantic::ALL {
            let emitted: &'static str = <&'static str as From<Semantic>>::from(variant);
            let reparsed = <Semantic as TryFrom<&str>>::try_from(emitted).unwrap_or_else(|()| {
                panic!(
                    "TryFrom<&str> for Semantic must accept every \
                     From<Semantic> for &'static str output — got \
                     Err(()) for Semantic::{variant:?}'s emit \
                     byte-string {emitted:?}"
                )
            });
            assert_eq!(
                reparsed, variant,
                "trait-idiomatic Semantic ↔ &'static str round-trip \
                 must be the identity on Semantic::{variant:?} — the \
                 From<Self> for &'static str + TryFrom<&str> for Self \
                 pair must compose to the identity on the closed 16-arm \
                 accept-set"
            );
        }
    }

    #[test]
    fn semantic_from_borrowed_into_static_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<&Semantic> for &'static str` — asserts the
        // borrowed-input standard-library trait impl and the substrate-
        // primitive [`super::Semantic::as_str`] `pub const fn` accessor
        // resolve to the same 16-arm canonical-lowercase kebab emit-set
        // across every arm the exhaustive [`super::Semantic::ALL`]
        // slice enumerates. Rust's `From` trait does not auto-derive
        // the borrowed-input sibling from a paired owned-input impl
        // (no `impl<T, U> From<&T> for U where T: Copy, U: From<T>`
        // blanket in `core`), so the borrowed-input axis is a distinct
        // trait-idiomatic surface that a `.iter().map(Into::into)`
        // shape over [`super::Semantic::ALL`] (whose iterator yields
        // `&Semantic`, not `Semantic`) reaches through this impl and
        // no other — the paired owned-input [`From<Semantic>`] impl
        // requires an explicit `.copied()` / dereference before the
        // trait fires. Materializes three
        // `<&'static str as From<&Semantic>>::from` outputs in
        // `const`-shape bindings against the paired
        // [`super::Semantic::as_str`] `pub const fn` accessor to make
        // the `'static` lifetime promise a build-time invariant — a
        // future accidental downgrade of any arm's inline canonical-
        // lowercase kebab byte-string to a non-`&'static str` (a
        // `String::leak()`-produced return, a `Box::leak`-cast, an
        // intermediate lifetime-erasing helper) trips at caixa-theme
        // build time rather than at a downstream `'static`-bound
        // consumer.
        //
        // Peer of the sibling
        // `caixa_provedor::ferrite::tests::ferrite_runtime_from_borrowed_into_static_str_routes_through_variant_slug_accessor`
        // (676d693) pin on the outside-`caixa-core` closed-set-enum
        // borrowed-input axis — extends the trait-idiomatic borrowed-
        // input forward-projection axis onto the last remaining
        // closed-set fieldless typed enum on the substrate surface
        // (the caixa-theme semantic-style 16-arm axis), closing the
        // substrate-wide 2×2-completion campaign's borrowed-input
        // `&'static str`-returning corner.
        const KEYWORD: &str = Semantic::Keyword.as_str();
        const KEYWORD_ARG: &str = Semantic::KeywordArg.as_str();
        const UNCHANGED: &str = Semantic::Unchanged.as_str();
        for variant in Semantic::ALL {
            let via_trait: &'static str = <&'static str as From<&Semantic>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<&Semantic> for &'static str impl must round-trip \
                 &Semantic::{variant:?} to the same canonical-lowercase \
                 kebab byte-string Semantic::as_str returns — divergence \
                 signals a silent detour off the substrate-primitive \
                 accessor"
            );
            let via_into: &'static str = variant.into();
            assert_eq!(
                via_into, via_method,
                "Into<&'static str>::into on &Semantic::{variant:?} must \
                 byte-equal Semantic::as_str on the same input — the \
                 blanket-derived Into shape on the borrowed-input axis \
                 must resolve to the same as_str dispatch as the \
                 explicit From impl"
            );
        }
        assert_eq!(
            [KEYWORD, KEYWORD_ARG, UNCHANGED],
            ["keyword", "keyword-arg", "unchanged"],
            "const-context Semantic::as_str must resolve to the \
             canonical-lowercase kebab byte-strings — a future \
             accidental downgrade of any arm to a non-const or non-\
             static byte-string breaks the `&'static str`-lifetime \
             promise the paired From<&Semantic> for &'static str impl \
             carries by construction"
        );
    }

    #[test]
    fn semantic_from_owned_and_borrowed_into_static_str_agree_on_every_arm() {
        // Cross-axis partition pin: the paired trait-idiomatic
        // `From<Semantic> for &'static str` owned-input forward
        // projection and the newly lifted
        // `From<&Semantic> for &'static str` borrowed-input forward
        // projection must resolve identically on *every* arm the
        // exhaustive [`super::Semantic::ALL`] slice enumerates. Locks
        // the two trait-idiomatic forward-projection paths together by
        // construction so any future detour (a stray borrowed-input
        // `From` special-case that lands on a divergent per-arm literal
        // outside the paired `as_str` dispatch, a hypothetical rebrand
        // touching one axis without the other) trips at caixa-theme
        // test time under `assert_eq!` rather than at a downstream
        // consumer split. Witnesses the borrowed-input axis with a
        // `.iter().map(Into::into)` pipe over `Semantic::ALL` (whose
        // iterator yields `&Semantic`, not `Semantic`, so the pipe
        // fires through the borrowed-input axis alone).
        //
        // Peer of the sibling
        // `caixa_provedor::ferrite::tests::ferrite_runtime_from_owned_and_borrowed_into_static_str_agree_on_every_arm`
        // (676d693) — extends the owned/borrowed-cross-axis partition
        // discipline onto the last remaining substrate-wide closed-set
        // fieldless typed enum whose borrowed-input axis was still
        // open.
        for &variant in Semantic::ALL {
            let via_owned: &'static str = <&'static str as From<Semantic>>::from(variant);
            let via_borrowed: &'static str = <&'static str as From<&Semantic>>::from(&variant);
            assert_eq!(
                via_owned, via_borrowed,
                "From<Semantic> for &'static str and From<&Semantic> \
                 for &'static str must agree on Semantic::{variant:?} \
                 — divergence signals the owned-input and borrowed-\
                 input forward-projection paths have drifted onto \
                 different emit-sets"
            );
        }
        let via_pipe: Vec<&'static str> = Semantic::ALL.iter().map(Into::into).collect();
        let via_method: Vec<&'static str> = Semantic::ALL.iter().map(|s| s.as_str()).collect();
        assert_eq!(
            via_pipe, via_method,
            "Semantic::ALL.iter().map(Into::into) must resolve to the \
             same per-arm canonical-lowercase kebab byte-string \
             sequence Semantic::as_str returns across every arm — the \
             iterator yields &Semantic, so this pipe fires through the \
             borrowed-input From<&Semantic> for &'static str axis and \
             witnesses the newly lifted impl's arm-set matches the \
             substrate-primitive accessor without a spurious Copy deref"
        );
    }

    #[test]
    fn semantic_from_into_owned_string_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<Semantic> for String` — asserts the owned-
        // `String`-returning standard-library trait impl and the
        // substrate-primitive [`super::Semantic::as_str`] `pub const
        // fn` accessor resolve to the same 16-arm canonical-lowercase
        // kebab emit-set across every arm the exhaustive
        // [`super::Semantic::ALL`] slice enumerates. Rust's standard
        // library does not carry a blanket
        // `impl<T: AsRef<str>> From<T> for String`, so the owned-
        // `String` axis is a distinct trait-idiomatic surface that a
        // `let key: String = sem.into();`-shaped downstream call site
        // reaches through this impl and no other — the sibling
        // `&'static str`-returning axes force an explicit
        // `.to_owned()` / [`String::from`] restatement whose type
        // bounds have no compile-time link to the substrate primitive.
        // Sweeps every one of the 16 arms
        // [`super::Semantic::ALL`] carries so no arm's projection is
        // covered only by the sibling method-named `as_str` /
        // [`std::fmt::Display`] / [`AsRef<str>`] / owned-input
        // `&'static str`-returning paths.
        //
        // Peer of the sibling
        // `caixa_provedor::ferrite::tests::ferrite_runtime_from_into_owned_string_routes_through_variant_slug_accessor`
        // (1e14fde) and
        // `caixa_arch::report::tests::arch_verdict_from_into_owned_string_routes_through_as_str_accessor`
        // (cc80a53) pins on the peer outside-`caixa-core` closed-set-
        // enum owned-`String`-returning axes — extends the trait-
        // idiomatic owned-`String`-returning forward-projection family
        // onto the sole closed-set fieldless typed enum on the caixa-
        // theme surface (the semantic-style 16-arm axis), extending
        // the substrate-wide 2×2-completion campaign onto the sixth
        // outside-`caixa-core` closed-set fieldless typed enum on the
        // caixa surface.
        for &variant in Semantic::ALL {
            let via_trait: String = <String as From<Semantic>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait.as_str(),
                via_method,
                "From<Semantic> for String impl must round-trip \
                 Semantic::{variant:?} to the same canonical-lowercase \
                 kebab byte-string Semantic::as_str returns — \
                 divergence signals a silent detour off the substrate-\
                 primitive accessor"
            );
            let via_into: String = variant.into();
            assert_eq!(
                via_into.as_str(),
                via_method,
                "Into<String>::into on Semantic::{variant:?} must \
                 byte-equal Semantic::as_str on the same input — the \
                 blanket-derived Into shape must resolve to the same \
                 as_str dispatch as the explicit From impl"
            );
        }
    }

    #[test]
    fn semantic_from_into_owned_string_and_static_str_agree_on_every_arm() {
        // Cross-axis partition pin: the paired trait-idiomatic
        // owned-input `&'static str`-returning
        // `From<Semantic> for &'static str` and owned-`String`-
        // returning `From<Semantic> for String` (this lift) forward
        // projections must resolve identically on every arm, locking
        // the two output-shape paths together so any future detour (a
        // stray owned-`String` special-case that lands on a divergent
        // per-arm literal outside the paired `as_str` dispatch, a
        // hypothetical rebrand touching one axis without the other, a
        // silent swap onto a hand-rolled per-arm literal that shadows
        // the paired [`super::Semantic::as_str`] dispatch) trips at
        // caixa-theme test time. Then a witness that the
        // `ToString::to_string`-through-[`std::fmt::Display`] surface
        // (`variant.to_string()`) byte-equals the trait-idiomatic
        // owned-`String` axis (`String::from(variant)`) on every arm,
        // so a future consumer that reaches for `.to_string()` and
        // one that reaches for `.into::<String>()` land on the same
        // substrate-primitive vocabulary. Plus a
        // `.iter().copied().map(String::from)` pipe witness over
        // [`super::Semantic::ALL`] — the exact shape a future per-
        // Semantic histogram key materializer or `caixa-lsp` per-
        // `SemanticTokenType` registration walk reaches through —
        // materializes the 16-arm accept-set through the owned-
        // `String` axis alone. Plus a direct `Self → String → Self`
        // round-trip witness through the paired [`TryFrom<&str>`]
        // axis on the owned-`String`'s [`String::as_str`] borrow,
        // closing the two-way round-trip on the owned-`String` axis
        // directly (no wire-vocab intermediate — the emit-side
        // [`super::Semantic::as_str`] and the parse-side
        // [`super::Semantic::from_wire`] dispatch on the same 16
        // inline canonical-lowercase kebab byte-strings by
        // construction).
        //
        // Peer of the sibling
        // `caixa_provedor::ferrite::tests::ferrite_runtime_from_into_owned_string_and_static_str_agree_on_every_arm`
        // (1e14fde) partition pin on the peer outside-`caixa-core`
        // closed-set-enum owned-`String`-returning axis.
        for &variant in Semantic::ALL {
            let owned_string: String = <String as From<Semantic>>::from(variant);
            let owned_static: &'static str = <&'static str as From<Semantic>>::from(variant);
            assert_eq!(
                owned_string.as_str(),
                owned_static,
                "From<Semantic> for String and From<Semantic> for \
                 &'static str must resolve identically on \
                 Semantic::{variant:?} — divergence signals the two \
                 output-shape forward-projection paths have drifted \
                 onto different emit-sets"
            );
            let via_display: String = variant.to_string();
            assert_eq!(
                owned_string, via_display,
                "From<Semantic> for String and ToString::to_string \
                 via Display must resolve identically on \
                 Semantic::{variant:?} — divergence signals the \
                 trait-idiomatic owned-`String` axis and the Display-\
                 routed ToString axis have drifted onto different \
                 vocabularies"
            );
        }
        let via_iter: Vec<String> = Semantic::ALL.iter().copied().map(String::from).collect();
        let via_method: Vec<String> = Semantic::ALL
            .iter()
            .map(|s| s.as_str().to_owned())
            .collect();
        assert_eq!(
            via_iter, via_method,
            "`.iter().copied().map(String::from)` over Semantic::ALL \
             must byte-equal `.iter().map(|s| s.as_str().to_owned())` \
             on every arm — the owned-`String` `From<Semantic> for \
             String` axis is what makes the `.map(String::from)` \
             shape route through the substrate-primitive \
             Semantic::as_str accessor rather than through a per-\
             call-site `.to_owned()` / `String::from(sem.as_str())` \
             detour"
        );
        for &variant in Semantic::ALL {
            let emitted: String = variant.into();
            let re_parsed: Result<Semantic, ()> =
                <Semantic as TryFrom<&str>>::try_from(emitted.as_str());
            assert_eq!(
                re_parsed,
                Ok(variant),
                "trait-idiomatic owned-`String` axis pair must round-\
                 trip Semantic::{variant:?} through \
                 `.into::<String>()` and back through `TryFrom<&str>` \
                 on the owned-`String`'s `String::as_str` borrow — a \
                 break signals the forward-emit owned-`String` axis \
                 and the reverse-parse `TryFrom<&str>` axis have \
                 drifted onto different vocabularies"
            );
        }
    }
}
