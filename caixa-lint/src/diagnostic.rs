use caixa_ast::Span;
use caixa_theme::{Semantic, Theme};
use serde::{Deserialize, Serialize};

/// The four-arm diagnostic-severity discriminator every rule attaches
/// to its emitted [`Diagnostic`] via [`Diagnostic::severity`] — the
/// closed-set typed enum every consumer of the linter's severity axis
/// keys off (the render-side per-arm Nord palette dispatch through
/// [`Self::as_semantic`], the errors-only filter at
/// `caixa-feira/src/cmd/lint.rs`, the per-rule severity annotation at
/// `caixa-lint/src/rules.rs`, the LSP-side per-severity
/// `DiagnosticSeverity` mapping at `caixa-lsp/src/main.rs`).
///
/// The [`gen_platform::IsVariant`] derive emits per-arm
/// [`Severity::is_error`] / [`Severity::is_warning`] / [`Severity::is_info`]
/// / [`Severity::is_hint`] predicates the runner-side error-count gate
/// (`caixa-feira lint`'s `--errors-only` retain, the same verb's
/// per-diagnostic `error_count` bump) and every peer downstream
/// severity-family gate route through — the same closed-set
/// arm-discriminator discipline the sibling `caixa_core::CaixaKind` /
/// `caixa_core::CaixaDialeto` / `caixa_core::PlacementStrategy` /
/// `caixa_core::RestartStrategy` / `caixa_core::RestartPolicy` /
/// `caixa_core::DepList` / `caixa_core::RateLimitUnit` /
/// `caixa_core::render::PathShapeViolation` /
/// `caixa_arch::InvariantKind` / `caixa_lint::FixSafety` /
/// `caixa_arch::ArchVerdict` closed-set fieldless typed enums already
/// carry. `Hash` (already on the derive set) keeps the enum usable in
/// arm-keyed sets (a future `feira lint --severity-policy=<tier>` verb
/// keying arm-specific counters, a future admission-webhook rejection
/// body enumerating the accepted severity tiers).
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, gen_platform::IsVariant,
)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl Severity {
    /// Exhaustive iteration surface for every consumer that walks the
    /// closed four-arm [`Severity`] discriminator set (the byte-parity
    /// witness against the paired [`gen_platform::IsVariant`]-derived
    /// [`Self::is_error`] / [`Self::is_warning`] / [`Self::is_info`] /
    /// [`Self::is_hint`] predicate family, a future
    /// `feira lint --list-severities` CLI enumeration of the accepted
    /// severity tags, a future admission-webhook rejection body naming
    /// the accepted severity set, any future round-trip fuzz harness
    /// that sweeps every arm).
    ///
    /// A future variant addition (a `Debug` tier below [`Self::Hint`]
    /// once verbose per-node lint traces enter scope, a `Critical` tier
    /// above [`Self::Error`] the M3-and-later LSP surfaces for
    /// build-halting failures the rulebook grows) extends this slice as
    /// a single edit and every consumer picks up the new entry by
    /// construction; the compiler-checked exhaustiveness on the paired
    /// [`gen_platform::IsVariant`]-derived per-arm predicates covers
    /// the projection axis so both halves of the closed-set discipline
    /// migrate as one edit.
    ///
    /// Peer of the sibling closed-set fieldless typed enums'
    /// [`caixa_core::CaixaKind::ALL`] (6b1f4fb) /
    /// [`caixa_core::CaixaDialeto::ALL`] (dd4f541) /
    /// [`caixa_core::aplicacao::PlacementStrategy::ALL`] (18c7342) /
    /// [`caixa_core::supervisor::RestartStrategy::ALL`] (4eec29c) /
    /// [`caixa_core::supervisor::RestartPolicy::ALL`] (dd32ccf) /
    /// [`caixa_core::dep::DepList::ALL`] (45ee563) /
    /// [`caixa_core::aplicacao::RateLimitUnit::ALL`] (6bce03d) /
    /// [`caixa_core::render::PathShapeViolation::ALL`] (efc0326) /
    /// [`caixa_arch::invariants::InvariantKind::ALL`] (5226ad5) /
    /// [`crate::FixSafety::ALL`] (732a791) /
    /// [`caixa_arch::report::ArchVerdict::ALL`] (94dafa6)
    /// exhaustive-iteration surfaces — the twelfth closed-set typed
    /// enum on the caixa surface (and the second inside `caixa-lint`,
    /// after [`crate::FixSafety`] at 732a791) to converge onto the
    /// same one-canonical-arm-list-per-enum discipline. Order matches
    /// variant declaration order verbatim (`Error` → `Warning` →
    /// `Info` → `Hint`) so the slice is the canonical severity
    /// ordering every listing / rendering consumer defers to.
    pub const ALL: &'static [Self] = &[Self::Error, Self::Warning, Self::Info, Self::Hint];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Hint => "hint",
        }
    }

    #[must_use]
    pub const fn as_semantic(self) -> Semantic {
        match self {
            Self::Error => Semantic::Error,
            Self::Warning => Semantic::Warning,
            Self::Info => Semantic::Info,
            Self::Hint => Semantic::Hint,
        }
    }

    /// Reverse projection on the [`Severity`] closed-set enum's
    /// canonical-tag axis — parses a `"error"` / `"warning"` / `"info"`
    /// / `"hint"` wire byte-string back to the typed enum, or returns
    /// `None` when `s` lies outside the four-arm accept-set
    /// [`Self::as_str`] emits. The single `&str → Self` projection every
    /// future re-entry point on the diagnostic-severity axis dispatches
    /// through (a future `feira lint --severity <error|warning|info|hint>`
    /// CLI arg-parse that binds the wire byte-string into the typed enum
    /// before dispatching to the per-arm filter, a future
    /// `caixa-lsp`-side per-diagnostic re-parse that hydrates a prior
    /// [`Self::as_str`] output back to the typed enum for
    /// `DiagnosticSeverity` mapping, a future M4
    /// `mesh.pleme.io/v1alpha1/LintReport` CR materializer's admission-
    /// time re-parse of the per-diagnostic severity column, a
    /// `tracing::field::Value::Str`-arm structured-log re-loader
    /// binding a prior emission's [`Self::as_str`] output back to the
    /// typed enum for cross-run severity-histogram diff) would have had
    /// to re-inline a four-arm `match s` cascade that expressed no
    /// compile-time link back to the substrate primitive.
    ///
    /// Same closed-set-reverse-projection discipline the sibling
    /// [`caixa_core::CaixaKind::from_wire`] (2aa6d23) /
    /// [`caixa_core::CaixaDialeto::from_wire`] (d0e65ea) /
    /// [`caixa_core::supervisor::RestartStrategy::from_wire`] (4eec29c) /
    /// [`caixa_core::supervisor::RestartPolicy::from_wire`] (dd32ccf) /
    /// [`caixa_core::aplicacao::PlacementStrategy::from_wire`] (18c7342) /
    /// [`caixa_core::dep::DepList::from_wire`] (45ee563) /
    /// [`caixa_core::render::PathShapeViolation::from_wire`] (aebd9c6) /
    /// `caixa_arch::invariants::InvariantKind::from_wire` (b9e4e61) /
    /// `caixa_arch::report::ArchVerdict::from_wire` (6afe564) typed
    /// enums carry on the peer wire-side `str → Self` axes — extends the
    /// substrate-wide `(as_str, from_wire)` round-trip family onto the
    /// first closed-set fieldless typed enum on the `caixa-lint` surface
    /// (the diagnostic-severity axis; the paired [`FixSafety`]
    /// fix-safety-tier axis remains open, an available follow-up),
    /// matching the same two-way `str ↔ Self` round-trip every sibling
    /// closed-set enum already carries. Method-named `from_wire` (not
    /// `from_str`) to match the peer shapes verbatim and side-step a
    /// `clippy::should_implement_trait` lint that a plain `from_str`
    /// name would otherwise trigger without paired
    /// [`std::str::FromStr`] impl scaffolding this axis does not carry
    /// today. Returns `Option<Self>` (rather than `Result<Self, _>`) to
    /// match the peer shapes: the caller picks the diagnostic form
    /// appropriate for its use site (a `feira lint --severity` CLI
    /// arg-parse renders its own per-verb error message; an admission-
    /// webhook rejection body wraps the `None` outcome with the
    /// accepted-set enumeration `Severity::ALL.iter().map(…)` for
    /// operator diagnostics).
    ///
    /// Pinned load-bearing at the substrate-primitive level by
    /// [`tests::severity_from_wire_accepts_every_as_str_output`]
    /// (round-trip witness against the peer [`Self::as_str`] axis) and
    /// [`tests::severity_from_wire_rejects_unknown_byte_strings`]
    /// (rejection witness against silent accept-set widening).
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "error" => Some(Self::Error),
            "warning" => Some(Self::Warning),
            "info" => Some(Self::Info),
            "hint" => Some(Self::Hint),
            _ => None,
        }
    }
}

/// Route the standard-library `&str`-projection trait through the
/// existing [`Severity::as_str`] `pub const fn` scalar accessor so
/// every downstream consumer that binds a [`Severity`] through the
/// trait-idiomatic `.as_ref()` (a `Command::arg` shell-out composing
/// the canonical severity tag into a `feira lint --severity=<tag>`
/// diagnostic overlay, a `tracing::field::Value::Str`-arm structured-
/// log recorder on the runner's per-diagnostic emission path, a
/// `HashMap::get::<str>(sev.as_ref())` lookup on a future
/// per-severity policy table) reaches the canonical byte-string
/// through one substrate-primitive dispatch rather than an open-
/// coded `.as_str()` re-inlining at every wire-up. Follows the
/// same closed-set-typed-enum `AsRef<str>`-through-`as_str`
/// convention the caixa-core siblings ([`caixa_core::CaixaKind`]
/// cd2091f, [`caixa_core::CaixaDialeto`] 1723611,
/// [`caixa_core::PlacementStrategy`] d86edd2,
/// [`caixa_core::RateLimitUnit`] d8136db,
/// [`caixa_core::RestartStrategy`] 63eb1a4,
/// [`caixa_core::RestartPolicy`] 419ea81,
/// [`caixa_core::DepList`] df4592e, [`caixa_core::CaixaVersion`]
/// 16d5c7e) already carry — extending the axis onto the caixa-lint
/// severity-classification closed-set typed enum, the ninth peer
/// on the caixa surface and the first outside caixa-core.
impl AsRef<str> for Severity {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Route the derived-style [`std::fmt::Display`] impl on [`Severity`]
/// through the substrate-canonical [`Severity::as_str`] `pub const fn`
/// accessor so every consumer that binds a [`Severity`] through the
/// standard-library `{}` formatting axis (a future
/// `caixa-lsp`-side hover-line composer projecting the canonical tag
/// into a rendered diagnostic, a `tracing::field::Value::from(sev)`
/// structured-log recorder on the runner's per-diagnostic emission
/// path, any `format!("{sev}")` interpolation in a future
/// audit-report surface) reaches the canonical byte-string through
/// one substrate-primitive dispatch rather than an open-coded per-arm
/// match at every wire-up.
///
/// Follows the same closed-set-typed-enum `Display`-through-`as_str`
/// convention the substrate-wide siblings [`caixa_core::CaixaKind`],
/// [`caixa_core::aplicacao::PlacementStrategy`],
/// [`caixa_core::supervisor::RestartStrategy`],
/// [`caixa_core::supervisor::RestartPolicy`],
/// [`caixa_core::dep::DepList`], and
/// [`caixa_arch::invariants::InvariantKind`] (87c875a) already carry
/// — closes the [`Severity`] closed-set enum's
/// `(as_str, Display, AsRef<str>)` canonical-projection triple.
impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Trait-idiomatic reverse projection on the [`Severity`] closed-set
/// caixa-lint diagnostic-severity axis — routes byte-for-byte through
/// the paired substrate-primitive [`Severity::from_wire`] `Option<Self>`
/// accessor so every future consumer that binds a canonical severity tag
/// through the standard-library `.try_into()` / [`TryFrom`] axis (a
/// future `feira lint --severity=<error|warning|info|hint>` CLI arg-parse
/// that composes into `let sev: Severity = s.try_into()?`, a future M4
/// `mesh.pleme.io/v1alpha1/LintReport` CR admission-webhook rejection-
/// body parser that folds a prior report's `spec.severity: String`
/// through `Severity::try_from(&s)?`, a `caixa-lsp`-side per-diagnostic
/// re-parse hydrating a prior [`Severity::as_str`] output back to the
/// typed enum for `DiagnosticSeverity` policy dispatch, a generic
/// `<T: TryFrom<&str>>`-bound lint-report re-loader over any of the
/// substrate's closed-set typed enums) reaches the same four-arm accept-
/// set the sibling [`Severity::from_wire`] resolver parses through and
/// the sibling [`Severity::as_str`] emits, rather than an open-coded
/// per-arm `match s { "error" => …, "warning" => …, "info" => …,
/// "hint" => …, _ => … }` cascade whose arm-set has no compile-time link
/// back to the substrate primitive.
///
/// Complements the pre-existing forward-projection triple
/// ([`std::fmt::Display`], [`AsRef<str>`], [`Severity::as_str`]) with the
/// paired trait-idiomatic reverse-projection axis: Rust-side newtype/
/// typed-enum convention pairs [`AsRef<str>`] with either
/// [`std::str::FromStr`] or [`TryFrom<&str>`] on the same primitive so a
/// caller who can project *out to* a `&str` can also project *in from*
/// one. The [`TryFrom<&str>`] axis is deliberately chosen over
/// [`std::str::FromStr`] to sidestep the `clippy::should_implement_trait`
/// lint the sibling method-named [`Severity::from_wire`] would trigger
/// under a `FromStr` impl (the same design tradeoff the peer
/// [`caixa_core::CaixaKind`] (3c83606),
/// [`caixa_core::CaixaDialeto`] (bf33136),
/// [`caixa_core::aplicacao::PlacementStrategy`] (6fd00cd),
/// [`caixa_core::supervisor::RestartStrategy`] (5b828ed),
/// [`caixa_core::supervisor::RestartPolicy`] (6fdd0d9),
/// [`caixa_core::aplicacao::WitShape`] (5472902),
/// [`caixa_core::aplicacao::RateLimitUnit`] (bf78400),
/// [`caixa_core::render::PathShapeViolation`] (e67e48a),
/// [`caixa_arch::invariants::InvariantKind`] (e21a857), and
/// [`caixa_arch::report::ArchVerdict`] (0a4cc45) blocks note) — this
/// impl closes the trait-idiomatic reverse axis without disturbing the
/// method-named `from_wire` shape every peer closed-set typed enum
/// already carries.
///
/// `type Error = ()` matches the sibling [`Severity::from_wire`]'s
/// `Option<Self>` return-shape's deliberate deferral of error typing: the
/// caller picks the diagnostic form appropriate for its use site (a
/// future `feira lint --severity` CLI arg-parse composes its own per-verb
/// "unknown lint-severity tier: <arg> — accepted: {…}" message
/// enumerating [`Severity::ALL`], a future M4 admission-webhook rejection
/// body wraps the `Err(())` outcome with the accepted-set enumeration for
/// operator diagnostics, a `Result::map_err` at the call site lifts the
/// axis-error to a per-verb error type). Same shape the peer sibling
/// reverse-projection axes carry.
///
/// The paired [`TryFrom<&str>`] impl reaches the same four-arm accept-
/// set the [`Severity::from_wire`] resolver dispatches through, so any
/// future arm addition (a `Debug` tier below [`Self::Hint`] once verbose
/// per-node lint traces enter scope, a `Critical` tier above
/// [`Self::Error`] the M3-and-later LSP surfaces for build-halting
/// failures — both trajectory items the sibling [`Severity::ALL`] doc
/// block already names) grows the trait-idiomatic axis by construction:
/// one caixa-lint edit on [`Severity::from_wire`] extends both the
/// method-named reverse projection every existing consumer keys off and
/// the trait-idiomatic reverse projection this impl exposes, without a
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
/// [`caixa_arch::invariants::InvariantKind`] via e21a857, and
/// [`caixa_arch::report::ArchVerdict`] via 0a4cc45) onto the first
/// closed-set fieldless typed enum on the caixa-lint surface — the
/// diagnostic-severity four-arm accept-set every `feira lint` per-
/// diagnostic render site, every `caixa-lsp`-side per-severity
/// `DiagnosticSeverity` mapping, and every future M4 admission-webhook /
/// lint-report re-loader dispatches through. The eleventh peer on the
/// substrate surface (and the first inside `caixa-lint`, with the paired
/// [`FixSafety`] fix-safety-tier axis remaining open as an available
/// follow-up).
///
/// Pinned load-bearing by
/// [`tests::severity_try_from_str_routes_through_from_wire_accessor`]
/// (byte-parity pin against [`Severity::from_wire`] across the four-arm
/// accept-set) and
/// [`tests::severity_try_from_str_rejects_unknown_byte_strings`]
/// (rejection witness against silent accept-set widening).
impl TryFrom<&str> for Severity {
    type Error = ();

    fn try_from(s: &str) -> Result<Self, <Self as TryFrom<&str>>::Error> {
        Self::from_wire(s).ok_or(())
    }
}

/// Standard-library trait-idiomatic forward projection on the
/// [`Severity`] closed-set caixa-lint diagnostic-severity axis.
/// Routes byte-for-byte through the paired substrate-primitive
/// [`Severity::as_str`] `pub const fn` accessor so
/// `<&'static str>::from(sev)` / `sev.into::<&'static str>()`
/// reaches the same four-arm `"error"` / `"warning"` / `"info"` /
/// `"hint"` canonical-lowercase emit-set the sibling method-named
/// accessor dispatches through and the sibling
/// [`std::fmt::Display for Severity`] / [`AsRef<str> for Severity`]
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
/// [`caixa_arch::invariants::InvariantKind`] via f2ca7bc, and
/// [`caixa_arch::report::ArchVerdict`] via d4559cb) onto the first
/// closed-set fieldless typed enum on the caixa-lint surface — the
/// diagnostic-severity four-arm accept-set every `feira lint`
/// per-diagnostic render site, every `caixa-lsp`-side per-severity
/// `DiagnosticSeverity` mapping, and every future M4 admission-webhook
/// / lint-report re-loader dispatches through. Extends the
/// trait-idiomatic forward-projection family onto the third
/// outside-caixa-core closed-set fieldless typed enum on the caixa
/// surface (after the two peer caixa-arch axes at f2ca7bc / d4559cb),
/// so a downstream `impl From<T> for &'static str`-bound generic
/// consumer reaches the caixa-lint diagnostic-severity axis through
/// the same uniform trait dispatch every caixa-core sibling and the
/// two peer caixa-arch axes already carry.
///
/// Pairs with the sibling [`TryFrom<&str> for Severity`] impl
/// (a7bf74c) to close the two-way `Self ↔ &'static str` round-trip on
/// the trait-idiomatic axis pair, mirroring the pre-existing
/// method-named [`Severity::as_str`] + [`Severity::from_wire`] pair
/// on the substrate-primitive axis pair.
///
/// Return type is `&'static str` by construction — every
/// [`Severity::as_str`] arm resolves to an inline `"error"` /
/// `"warning"` / `"info"` / `"hint"` `&'static str` literal, so the
/// trait's return-type promise is upheld structurally without a
/// [`String::leak`] cast or a per-arm inline literal outside the
/// paired [`Severity::as_str`] dispatch.
///
/// The paired [`Severity::as_str`] accessor's four-arm emit-set is
/// the single source of truth — every future arm addition (a `Debug`
/// tier below [`Self::Hint`] once verbose per-node lint traces enter
/// scope, a `Critical` tier above [`Self::Error`] the M3-and-later
/// LSP surfaces for build-halting failures — both trajectory items
/// the sibling [`Severity::ALL`] doc block already names) grows the
/// trait-idiomatic forward axis by construction: one caixa-lint edit
/// on [`Severity::as_str`] extends every one of the sibling
/// forward-projection paths ([`std::fmt::Display`], [`AsRef<str>`],
/// [`Severity::as_str`] itself, and this [`From<Self> for &'static
/// str`]) without a coordinated rewrite across every future
/// `Into<&'static str>`-bound consumer's arm-set.
///
/// Pinned load-bearing by
/// [`tests::severity_from_into_static_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`Severity::as_str`] across the four-arm
/// emit-set, plus a `const`-context materialization witness for the
/// `&'static str` lifetime promise routed through the paired
/// [`Severity::as_str`] `pub const fn` accessor, plus a paired
/// `.into()` shape assertion covering the blanket-derived
/// `Into<&'static str>` shape) and
/// [`tests::severity_from_into_static_str_and_as_str_partition_the_emit_set`]
/// (partition pin asserting `<&'static str as
/// From<Severity>>::from` and [`Severity::as_str`] agree on every
/// arm, plus a two-way direct round-trip witness through the paired
/// trait-idiomatic [`TryFrom<&str>`] axis that closes the two-way
/// `Self ↔ &'static str` round-trip on the trait-idiomatic axis pair
/// — the emit-side [`Severity::as_str`] and the parse-side
/// [`Severity::from_wire`] dispatch on the same four inline
/// canonical-lowercase byte-strings by construction, so round-tripping
/// composes the two trait impls directly).
impl From<Severity> for &'static str {
    fn from(severity: Severity) -> &'static str {
        severity.as_str()
    }
}

/// A textual edit — replace `span` with `replacement` in the source.
/// Edits never overlap; the autofix driver sorts them by `span.start`
/// descending and applies in reverse order so earlier offsets stay
/// stable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edit {
    pub span: Span,
    pub replacement: String,
}

/// Auto-applicable correction for a [`Diagnostic`]. A single fix may
/// involve multiple edits (e.g. rename a name + every reference);
/// all edits apply atomically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fix {
    pub description: String,
    pub edits: Vec<Edit>,
    pub safety: FixSafety,
}

/// How safe is a fix to apply automatically?
///
/// * `Safe` — mechanical, semantics-preserving for pure round-trips.
///   `feira lint --fix` applies these by default.
/// * `Unsafe` — heuristic; may change runtime behavior in edge cases.
///   Requires explicit `--fix-unsafe` opt-in.
///
/// The `gen_platform::IsVariant` derive emits per-arm
/// [`FixSafety::is_safe`] / [`FixSafety::is_unsafe`] predicates the
/// runner's fix-safety gate keys off — the same closed-set
/// arm-discriminator discipline the sibling `caixa_core::CaixaKind`
/// / `caixa_core::CaixaDialeto` / `caixa_core::PlacementStrategy` /
/// `caixa_core::RestartStrategy` / `caixa_core::RestartPolicy` /
/// `caixa_core::DepList` / `caixa_core::RateLimitUnit` /
/// `caixa_arch::InvariantKind` closed-set fieldless typed enums already
/// carry. `Hash` joins the derive set so the enum can live in
/// arm-keyed sets (a future `feira lint --fix-safety=<policy>` verb
/// keying arm-specific counters, a future admission-webhook rejection
/// body naming the accepted safety-tier list).
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, gen_platform::IsVariant,
)]
pub enum FixSafety {
    Safe,
    Unsafe,
}

impl FixSafety {
    /// Exhaustive iteration surface for every consumer that walks the
    /// closed two-arm [`FixSafety`] discriminator set (a future
    /// `feira lint --fix-safety=<policy>` verb enumerating the accepted
    /// safety-tier list, a future admission-webhook rejection body
    /// naming the accepted tiers, any future safety-tier fuzz harness
    /// that sweeps every arm).
    ///
    /// A future variant addition (an `Experimental` tier between
    /// [`Self::Safe`] and [`Self::Unsafe`] the M3-and-later lint
    /// runner grows for AI-suggested rewrites that need explicit
    /// review-and-accept) extends this slice as a single edit and
    /// every consumer picks up the new entry by construction; the
    /// compiler-checked exhaustiveness on the paired
    /// `gen_platform::IsVariant`-derived per-arm predicates
    /// ([`Self::is_safe`] / [`Self::is_unsafe`]) covers the projection
    /// axis so both halves of the closed-set discipline migrate as
    /// one edit.
    ///
    /// Peer of the sibling closed-set typed enums'
    /// [`caixa_core::CaixaKind::ALL`] (6b1f4fb) /
    /// [`caixa_core::CaixaDialeto::ALL`] (dd4f541) /
    /// [`caixa_core::PlacementStrategy::ALL`] (18c7342) /
    /// [`caixa_core::RestartStrategy::ALL`] (4eec29c) /
    /// [`caixa_core::RestartPolicy::ALL`] (dd32ccf) /
    /// [`caixa_core::DepList::ALL`] (45ee563) /
    /// [`caixa_core::RateLimitUnit::ALL`] (6bce03d) /
    /// [`caixa_arch::InvariantKind::ALL`] (5226ad5)
    /// exhaustive-iteration surfaces — the ninth closed-set typed
    /// enum on the caixa surface (and the second outside `caixa-core`,
    /// after `caixa_arch::InvariantKind`) to converge onto the same
    /// one-canonical-arm-list-per-enum discipline.
    pub const ALL: &'static [Self] = &[Self::Safe, Self::Unsafe];

    /// Substrate-canonical per-[`FixSafety`] lowercase-tag scalar
    /// accessor every consumer that renders the fix-safety-tier axis
    /// as user-facing text keys off — returns the per-arm byte-string
    /// (`"safe"` / `"unsafe"`) as a `&'static str`, matching the paired
    /// [`gen_platform::IsVariant`]-derive-generated per-arm predicate
    /// names ([`Self::is_safe`] / [`Self::is_unsafe`]) verbatim.
    ///
    /// Named `as_str` (not `label` / `tag`) to match the sibling
    /// closed-set-enum `as_str` convention the substrate already
    /// carries across every peer typed enum — [`crate::Severity::as_str`]
    /// on the paired caixa-lint severity axis, [`caixa_core::CaixaKind::as_str`]
    /// on the caixa-core top-level `:kind` axis, and the M2/M3
    /// [`caixa_core::supervisor::RestartStrategy::as_str`] /
    /// [`caixa_core::supervisor::RestartPolicy::as_str`] /
    /// [`caixa_core::aplicacao::PlacementStrategy::as_str`] siblings.
    /// Every future consumer that reaches the fix-safety tier as a
    /// canonical byte-string (a future `feira lint --fix-safety=<tag>`
    /// verb enumerating the accepted tier list into a Nord-themed
    /// help line, a `tracing::field::Value::Str`-arm structured-log
    /// recorder on the runner's per-diagnostic fix-safety-classification
    /// emission path, a future admission-webhook rejection body
    /// naming the accepted fix-safety tiers) reaches the paired
    /// byte-string through one substrate-primitive dispatch rather
    /// than an open-coded `format!("{:?}", ...)`-allocation re-inlining
    /// at every wire-up.
    ///
    /// `pub const fn` — matches the sibling
    /// [`gen_platform::IsVariant`]-derive-generated per-arm `is_*`
    /// predicates' `const fn` posture, so every future substrate-side
    /// `const`-context consumer (a `const _: () = assert!(…)` module-
    /// scope pin on a per-fixture typed [`FixSafety`], a compile-time
    /// `HashMap<&'static str, _>`-shaped per-tier policy table) reaches
    /// the paired byte-string through one substrate-primitive dispatch
    /// at compile time as at runtime.
    ///
    /// A future variant addition (an `Experimental` tier between
    /// [`Self::Safe`] and [`Self::Unsafe`] the M3-and-later lint
    /// runner grows for AI-suggested rewrites that need explicit
    /// review-and-accept) reaches the paired [`std::fmt::Display`] +
    /// [`AsRef<str>`] impls and every downstream `.as_str()` consumer
    /// through one match-arm edit here.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Unsafe => "unsafe",
        }
    }

    /// Reverse projection on the [`FixSafety`] closed-set enum's
    /// canonical-tag axis — parses a `"safe"` / `"unsafe"` wire byte-
    /// string back to the typed enum, or returns `None` when `s` lies
    /// outside the two-arm accept-set [`Self::as_str`] emits. The single
    /// `&str → Self` projection every future re-entry point on the
    /// fix-safety-tier axis dispatches through (a future
    /// `feira lint --fix-safety=<safe|unsafe>` CLI arg-parse binding the
    /// wire byte-string into the typed enum before dispatching to the
    /// per-arm gate at [`crate::runner::apply_fixes`], a future
    /// `caixa-lsp`-side per-fix re-parse hydrating a prior
    /// [`Self::as_str`] output back to the typed enum for
    /// `CodeActionKind::QuickFix` policy dispatch, a future M4
    /// `mesh.pleme.io/v1alpha1/LintReport` CR materializer's admission-
    /// time re-parse of the per-fix `safety` column, a
    /// `tracing::field::Value::Str`-arm structured-log re-loader
    /// binding a prior emission's [`Self::as_str`] output back to the
    /// typed enum for cross-run fix-application-histogram diff) would
    /// have had to re-inline a two-arm `match s` cascade that expressed
    /// no compile-time link back to the substrate primitive.
    ///
    /// Same closed-set-reverse-projection discipline the sibling
    /// [`crate::Severity::from_wire`] (5afff0e) on the paired caixa-lint
    /// severity axis, [`caixa_core::CaixaKind::from_wire`] (2aa6d23),
    /// [`caixa_core::CaixaDialeto::from_wire`] (d0e65ea),
    /// [`caixa_core::supervisor::RestartStrategy::from_wire`] (4eec29c),
    /// [`caixa_core::supervisor::RestartPolicy::from_wire`] (dd32ccf),
    /// [`caixa_core::aplicacao::PlacementStrategy::from_wire`] (18c7342),
    /// [`caixa_core::dep::DepList::from_wire`] (45ee563),
    /// [`caixa_core::render::PathShapeViolation::from_wire`] (aebd9c6),
    /// `caixa_arch::invariants::InvariantKind::from_wire` (b9e4e61), and
    /// `caixa_arch::report::ArchVerdict::from_wire` (6afe564) typed enums
    /// carry on the peer wire-side `str → Self` axes — closes the
    /// substrate-wide `(as_str, from_wire)` round-trip family on the
    /// caixa-lint surface (the second and last closed-set fieldless
    /// typed enum on `caixa-lint`, after [`crate::Severity`]), matching
    /// the same two-way `str ↔ Self` round-trip every sibling closed-set
    /// enum already carries. Method-named `from_wire` (not `from_str`)
    /// to match the peer shapes verbatim and side-step a
    /// `clippy::should_implement_trait` lint that a plain `from_str`
    /// name would otherwise trigger without paired [`std::str::FromStr`]
    /// impl scaffolding this axis does not carry today. Returns
    /// `Option<Self>` (rather than `Result<Self, _>`) to match the peer
    /// shapes: the caller picks the diagnostic form appropriate for its
    /// use site (a `feira lint --fix-safety` CLI arg-parse renders its
    /// own per-verb error message; an admission-webhook rejection body
    /// wraps the `None` outcome with the accepted-set enumeration
    /// `FixSafety::ALL.iter().map(…)` for operator diagnostics).
    ///
    /// Pinned load-bearing at the substrate-primitive level by
    /// [`tests::fix_safety_from_wire_accepts_every_as_str_output`]
    /// (round-trip witness against the peer [`Self::as_str`] axis) and
    /// [`tests::fix_safety_from_wire_rejects_unknown_byte_strings`]
    /// (rejection witness against silent accept-set widening).
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "safe" => Some(Self::Safe),
            "unsafe" => Some(Self::Unsafe),
            _ => None,
        }
    }
}

/// Route the derived-style [`std::fmt::Display`] impl on [`FixSafety`]
/// through the substrate-canonical [`FixSafety::as_str`] `pub const fn`
/// accessor so every consumer that binds a [`FixSafety`] through the
/// standard-library `{}` formatting axis (a future `feira lint --fix`
/// per-tier summary line, a `tracing::field::Value::from(fix.safety)`
/// structured-log recorder on the runner's per-fix emission path, any
/// `format!("{safety}")` interpolation in a future audit surface)
/// reaches the canonical byte-string through one substrate-primitive
/// dispatch rather than an open-coded per-arm match at every wire-up.
///
/// Follows the same closed-set-typed-enum `Display`-through-`as_str`
/// convention the substrate-wide siblings [`crate::Severity`] (6ad94f3),
/// [`caixa_core::CaixaKind`], [`caixa_core::aplicacao::PlacementStrategy`],
/// [`caixa_core::supervisor::RestartStrategy`],
/// [`caixa_core::supervisor::RestartPolicy`],
/// [`caixa_core::dep::DepList`], [`caixa_arch::invariants::InvariantKind`]
/// (87c875a), and [`caixa_arch::report::ArchVerdict`] (f3da79b) already
/// carry — closes the [`FixSafety`] closed-set enum's
/// `(as_str, Display, AsRef<str>)` canonical-projection triple.
impl std::fmt::Display for FixSafety {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Route the standard-library [`AsRef<str>`] projection on [`FixSafety`]
/// through the substrate-canonical [`FixSafety::as_str`] `pub const fn`
/// accessor so every consumer that binds a [`FixSafety`] through the
/// trait-idiomatic `.as_ref()` (a future
/// `HashMap::get::<str>(safety.as_ref())` per-tier policy-table lookup,
/// a `Command::arg` shell-out composing the canonical fix-safety tag
/// into a `feira lint --fix-safety=<tag>` verb, any
/// `impl AsRef<str>`-bound generic function) reaches the canonical
/// byte-string through one substrate-primitive dispatch rather than an
/// open-coded `.as_str()` re-inlining at every wire-up.
///
/// Peer of the substrate-wide sibling closed-set-enum
/// `AsRef<str>`-through-`as_str` family already carried by
/// [`crate::Severity`] (ce9d1e3), [`caixa_core::CaixaKind`],
/// [`caixa_core::CaixaDialeto`],
/// [`caixa_core::aplicacao::PlacementStrategy`],
/// [`caixa_core::aplicacao::RateLimitUnit`],
/// [`caixa_core::supervisor::RestartStrategy`],
/// [`caixa_core::supervisor::RestartPolicy`],
/// [`caixa_core::dep::DepList`], [`caixa_core::CaixaVersion`],
/// [`caixa_arch::invariants::InvariantKind`], and
/// [`caixa_arch::report::ArchVerdict`] — extends the axis onto the
/// caixa-lint fix-safety-tier closed-set enum, closing the
/// `(as_str, Display, AsRef<str>)` canonical-projection triple.
impl AsRef<str> for FixSafety {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Route the standard-library [`TryFrom<&str>`] projection on
/// [`FixSafety`] through the substrate-canonical
/// [`FixSafety::from_wire`] `Option<Self>` accessor so every future
/// consumer that binds a fix-safety-tier byte-string through the
/// trait-idiomatic `.try_into()` / [`TryFrom`] axis (a future
/// `feira lint --fix-safety=<safe|unsafe>` CLI arg-parse composing into
/// `let sev: FixSafety = s.try_into()?`, a `caixa-lsp`-side per-fix
/// re-parse hydrating a prior [`FixSafety::as_str`] output back for
/// `CodeActionKind::QuickFix` policy dispatch, a future M4
/// `mesh.pleme.io/v1alpha1/LintReport` CR admission-webhook body parser
/// folding `spec.fixes[*].safety: String` through
/// `FixSafety::try_from(&s)?`, any generic `<T: TryFrom<&str>>`-bound
/// lint-report re-loader) reaches the same two-arm accept-set the
/// sibling [`FixSafety::from_wire`] resolver dispatches through, without
/// an open-coded per-arm cascade with no compile-time link back to the
/// typed enum.
///
/// `type Error = ()` matches the peer reverse-projection axes'
/// deliberate deferral of error typing — the caller picks the diagnostic
/// form appropriate for its use site (a CLI arg-parse renders its own
/// per-verb error message; an admission-webhook rejection body wraps
/// the `Err(())` outcome with the accepted-set enumeration
/// `FixSafety::ALL.iter().map(…)` for operator diagnostics). Chosen
/// over [`std::str::FromStr`] to sidestep the
/// `clippy::should_implement_trait` lint the sibling method-named
/// [`FixSafety::from_wire`] would trigger under a plain `FromStr` impl
/// — same design tradeoff every prior sibling reverse-projection lift
/// already carries.
///
/// The paired [`TryFrom<&str>`] impl reaches the same two-arm accept-
/// set the [`FixSafety::from_wire`] resolver dispatches through, so any
/// future arm addition (an `Experimental` tier between [`Self::Safe`]
/// and [`Self::Unsafe`] the M3-and-later lint runner grows for
/// AI-suggested rewrites that need explicit review-and-accept — the
/// trajectory item the sibling [`FixSafety::ALL`] doc block already
/// names) grows the trait-idiomatic axis by construction: one caixa-
/// lint edit on [`FixSafety::from_wire`] extends both the method-named
/// reverse projection every existing consumer keys off and the
/// trait-idiomatic reverse projection this impl exposes, without a
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
/// [`caixa_arch::invariants::InvariantKind`] via e21a857,
/// [`caixa_arch::report::ArchVerdict`] via 0a4cc45, and the paired
/// sibling [`Severity`] via a7bf74c) onto the second (and last)
/// closed-set fieldless typed enum on the caixa-lint surface — the
/// fix-safety-tier two-arm accept-set every `feira lint --fix` per-fix
/// dispatch, every `caixa-lsp`-side per-fix `CodeActionKind::QuickFix`
/// policy dispatch, and every future M4 admission-webhook / lint-report
/// re-loader dispatches through. The twelfth peer on the substrate
/// surface, closing the caixa-lint crate's two closed-set fieldless
/// typed enums onto the two-way `str ↔ Self` round-trip.
///
/// Pinned load-bearing by
/// [`tests::fix_safety_try_from_str_routes_through_from_wire_accessor`]
/// (byte-parity pin against [`FixSafety::from_wire`] across the two-arm
/// accept-set),
/// [`tests::fix_safety_try_from_str_rejects_unknown_byte_strings`]
/// (rejection witness against silent accept-set widening), and
/// [`tests::fix_safety_try_from_str_and_from_wire_partition_the_accept_set`]
/// (cross-axis partition pin locking trait and method-named projections
/// to the same `Option<Self>` output on every input).
impl TryFrom<&str> for FixSafety {
    type Error = ();

    fn try_from(s: &str) -> Result<Self, <Self as TryFrom<&str>>::Error> {
        Self::from_wire(s).ok_or(())
    }
}

/// Standard-library trait-idiomatic forward projection on the
/// [`FixSafety`] closed-set caixa-lint fix-safety-tier axis. Routes
/// byte-for-byte through the paired substrate-primitive
/// [`FixSafety::as_str`] `pub const fn` accessor so
/// `<&'static str>::from(safety)` / `safety.into::<&'static str>()`
/// reaches the same two-arm `"safe"` / `"unsafe"` canonical-lowercase
/// emit-set the sibling method-named accessor dispatches through and
/// the sibling [`std::fmt::Display for FixSafety`] /
/// [`AsRef<str> for FixSafety`] impls also route through.
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
/// [`caixa_arch::report::ArchVerdict`] via d4559cb, and the paired
/// sibling [`Severity`] via 5cc3b8b) onto the second (and last)
/// closed-set fieldless typed enum on the caixa-lint surface — the
/// fix-safety-tier two-arm accept-set every `feira lint --fix` per-fix
/// dispatch, every `caixa-lsp`-side per-fix
/// `CodeActionKind::QuickFix` policy dispatch, and every future M4
/// admission-webhook / lint-report re-loader dispatches through.
/// Closes the caixa-lint crate's two closed-set fieldless typed enums
/// onto the trait-idiomatic forward-projection axis, matching the
/// paired trait-idiomatic reverse-projection axis (already closed via
/// df86c94 on this enum and a7bf74c on the sibling [`Severity`]).
///
/// Pairs with the sibling [`TryFrom<&str> for FixSafety`] impl
/// (df86c94) to close the two-way `Self ↔ &'static str` round-trip on
/// the trait-idiomatic axis pair, mirroring the pre-existing
/// method-named [`FixSafety::as_str`] + [`FixSafety::from_wire`] pair
/// on the substrate-primitive axis pair.
///
/// Return type is `&'static str` by construction — every
/// [`FixSafety::as_str`] arm resolves to an inline `"safe"` /
/// `"unsafe"` `&'static str` literal, so the trait's return-type
/// promise is upheld structurally without a [`String::leak`] cast or
/// a per-arm inline literal outside the paired [`FixSafety::as_str`]
/// dispatch.
///
/// The paired [`FixSafety::as_str`] accessor's two-arm emit-set is
/// the single source of truth — every future arm addition (an
/// `Experimental` tier between [`Self::Safe`] and [`Self::Unsafe`] the
/// M3-and-later lint runner grows for AI-suggested rewrites that need
/// explicit review-and-accept — the trajectory item the sibling
/// [`FixSafety::ALL`] doc block already names) grows the
/// trait-idiomatic forward axis by construction: one caixa-lint edit
/// on [`FixSafety::as_str`] extends every one of the sibling
/// forward-projection paths ([`std::fmt::Display`], [`AsRef<str>`],
/// [`FixSafety::as_str`] itself, and this
/// [`From<Self> for &'static str`]) without a coordinated rewrite
/// across every future `Into<&'static str>`-bound consumer's arm-set.
///
/// Pinned load-bearing by
/// [`tests::fix_safety_from_into_static_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`FixSafety::as_str`] across the two-arm
/// emit-set, plus a `const`-context materialization witness for the
/// `&'static str` lifetime promise routed through the paired
/// [`FixSafety::as_str`] `pub const fn` accessor, plus a paired
/// `.into()` shape assertion covering the blanket-derived
/// `Into<&'static str>` shape) and
/// [`tests::fix_safety_from_into_static_str_and_as_str_partition_the_emit_set`]
/// (partition pin asserting `<&'static str as
/// From<FixSafety>>::from` and [`FixSafety::as_str`] agree on every
/// arm, plus a two-way direct round-trip witness through the paired
/// trait-idiomatic [`TryFrom<&str>`] axis that closes the two-way
/// `Self ↔ &'static str` round-trip on the trait-idiomatic axis pair
/// — the emit-side [`FixSafety::as_str`] and the parse-side
/// [`FixSafety::from_wire`] dispatch on the same two inline
/// canonical-lowercase byte-strings by construction, so round-tripping
/// composes the two trait impls directly).
impl From<FixSafety> for &'static str {
    fn from(safety: FixSafety) -> &'static str {
        safety.as_str()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub rule_id: &'static str,
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    pub hint: Option<String>,
    /// Optional autofix. When present, `feira lint --fix` applies it
    /// (subject to the requested safety threshold).
    pub fix: Option<Fix>,
}

impl Diagnostic {
    #[must_use]
    pub fn new(
        rule_id: &'static str,
        severity: Severity,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        Self {
            rule_id,
            severity,
            span,
            message: message.into(),
            hint: None,
            fix: None,
        }
    }

    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Attach an auto-applicable correction.
    #[must_use]
    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }

    /// Convenience: attach a single-edit safe fix that replaces this
    /// diagnostic's own span with `replacement`.
    #[must_use]
    pub fn with_fix_replace(
        self,
        description: impl Into<String>,
        replacement: impl Into<String>,
    ) -> Self {
        let span = self.span;
        self.with_fix(Fix {
            description: description.into(),
            edits: vec![Edit {
                span,
                replacement: replacement.into(),
            }],
            safety: FixSafety::Safe,
        })
    }

    /// Render this diagnostic against a source string, Nord-themed.
    #[must_use]
    pub fn render(&self, src: &str, theme: &Theme) -> String {
        let pos = caixa_ast::line_column(src, self.span.start);
        let sev = theme.paint(self.severity.as_semantic(), self.severity.as_str());
        let id = theme.paint(Semantic::Muted, &format!("[{}]", self.rule_id));
        let at = theme.paint(Semantic::Muted, &format!("{pos}"));
        let mut out = format!("{sev} {id} {at}: {}", self.message);
        if let Some(h) = &self.hint {
            out.push('\n');
            out.push_str("  ");
            out.push_str(&theme.paint(Semantic::Hint, "hint"));
            out.push_str(": ");
            out.push_str(h);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fix_safety_all_lists_every_variant_in_declaration_order() {
        // Fail-before-pass-after pin on the closed two-arm
        // [`FixSafety`] discriminator set: any future variant
        // addition (an `Experimental` tier between [`FixSafety::Safe`]
        // and [`FixSafety::Unsafe`] the M3-and-later lint runner grows
        // for AI-suggested rewrites that need explicit review-and-
        // accept) that lands the new variant on the enum without
        // extending [`FixSafety::ALL`] trips this test — the
        // compiler-checked exhaustiveness on the
        // `gen_platform::IsVariant`-derived per-arm predicates
        // ([`FixSafety::is_safe`] / [`FixSafety::is_unsafe`]) covers
        // the projection axis; this pin covers the exhaustive-
        // iteration axis so both halves of the closed-set discipline
        // migrate as one edit.
        //
        // Peer of the sibling
        // `caixa_arch::invariants::tests::invariant_kind_all_lists_every_variant_in_declaration_order`
        // pin on the peer closed-set-enum `ALL` axis (5226ad5).
        assert_eq!(FixSafety::ALL, &[FixSafety::Safe, FixSafety::Unsafe]);
        // Every arm satisfies exactly one of the paired per-arm
        // predicates — the byte-parity pin between the
        // [`FixSafety::ALL`] iteration axis and the per-arm
        // `gen_platform::IsVariant`-derived predicate axis.
        for arm in FixSafety::ALL {
            assert_eq!(
                usize::from(arm.is_safe()) + usize::from(arm.is_unsafe()),
                1,
                "FixSafety::{arm:?} must satisfy exactly one of \
                 is_safe / is_unsafe",
            );
        }
    }

    #[test]
    fn fix_safety_predicates_are_byte_equal_to_matches_family() {
        // Fail-before-pass-after pin on the runner-side converge of
        // the pre-lift `match (max_safety, fix.safety) { (Safe, Unsafe)
        // => skip, _ => {} }` tuple-match at
        // [`crate::runner::apply_fixes`] onto the
        // `gen_platform::IsVariant`-derive-generated per-arm predicate
        // family: for every arm, each predicate agrees byte-for-byte
        // with the pre-lift `matches!(_, FixSafety::…)` shape.
        //
        // A future rebrand touching either endpoint (a
        // `#[is_variant(name = "…")]` attribute drift on the derive,
        // an arm rename, an accidental peer predicate that shadows the
        // derive-generated one) would silently split the two paths and
        // trip this pin. Peer of the sibling
        // `caixa_arch::invariants::tests::invariant_kind_predicates_are_byte_equal_to_matches_family`
        // pin on the peer closed-set-enum `IsVariant` convergence axis
        // (5226ad5) and the workspace
        // `caixa_core::supervisor::tests::restart_policy_predicates_are_byte_equal_to_matches`
        // sibling pin.
        for arm in FixSafety::ALL {
            assert_eq!(
                arm.is_safe(),
                matches!(arm, FixSafety::Safe),
                "FixSafety::{arm:?}.is_safe() must agree with \
                 matches!(_, FixSafety::Safe) byte-for-byte",
            );
            assert_eq!(
                arm.is_unsafe(),
                matches!(arm, FixSafety::Unsafe),
                "FixSafety::{arm:?}.is_unsafe() must agree with \
                 matches!(_, FixSafety::Unsafe) byte-for-byte",
            );
        }
    }

    #[test]
    fn fix_safety_gate_agrees_with_tuple_match_across_full_2x2_grid() {
        // Byte-parity pin on the [`crate::runner::apply_fixes`] gate:
        // the post-lift `max_safety.is_safe() && fix.safety.is_unsafe()`
        // predicate must agree with the pre-lift `matches!((max_safety,
        // fix.safety), (FixSafety::Safe, FixSafety::Unsafe))` tuple-
        // match across every point of the 2×2 (max_safety × fix.safety)
        // grid — the four-point closure of the runner's fix-safety
        // gate over the closed-set discriminator's full accept-set.
        // A future arm addition to [`FixSafety`] shifts this from a
        // 2×2 to an NxN grid; the [`FixSafety::ALL`]-driven iteration
        // picks up the new arm by construction.
        for a in FixSafety::ALL {
            for b in FixSafety::ALL {
                let post_lift = a.is_safe() && b.is_unsafe();
                let pre_lift = matches!((a, b), (FixSafety::Safe, FixSafety::Unsafe));
                assert_eq!(
                    post_lift, pre_lift,
                    "runner fix-safety gate diverges at (max={a:?}, fix={b:?}): \
                     post-lift={post_lift}, pre-lift={pre_lift}",
                );
            }
        }
    }

    #[test]
    fn severity_all_enumerates_every_variant_once() {
        // Fail-before-pass-after pin on the [`gen_platform::IsVariant`]
        // derive's per-arm-list-per-enum discipline for [`Severity`]:
        // any future variant addition (a `Debug` tier below
        // [`Severity::Hint`] for verbose per-node lint traces, a
        // `Critical` tier above [`Severity::Error`] for build-halting
        // failures the rulebook grows) that lands the new variant on
        // the enum without extending [`Severity::ALL`] trips this test
        // — the compiler-checked exhaustiveness on the paired
        // [`gen_platform::IsVariant`]-derived per-arm predicates
        // ([`Severity::is_error`] / [`Severity::is_warning`] /
        // [`Severity::is_info`] / [`Severity::is_hint`]) covers the
        // projection axis; this pin covers the exhaustive-iteration
        // axis so both halves of the closed-set discipline migrate as
        // one edit.
        //
        // Peer of the sibling
        // [`fix_safety_all_lists_every_variant_in_declaration_order`]
        // pin on the peer closed-set-enum `ALL` axis (732a791) and the
        // workspace `caixa_arch::report::tests::arch_verdict_all_enumerates_every_variant_once`
        // sibling pin (94dafa6).
        assert_eq!(
            Severity::ALL,
            &[
                Severity::Error,
                Severity::Warning,
                Severity::Info,
                Severity::Hint,
            ],
        );
        // Every arm satisfies exactly one of the paired per-arm
        // predicates — the byte-parity pin between the
        // [`Severity::ALL`] iteration axis and the per-arm
        // [`gen_platform::IsVariant`]-derived predicate axis: for
        // every arm, exactly one of `is_error` / `is_warning` /
        // `is_info` / `is_hint` returns `true`.
        for arm in Severity::ALL {
            let (err, warn, info, hint) = (
                arm.is_error(),
                arm.is_warning(),
                arm.is_info(),
                arm.is_hint(),
            );
            assert_eq!(
                usize::from(err) + usize::from(warn) + usize::from(info) + usize::from(hint),
                1,
                "Severity::{arm:?} must satisfy exactly one of \
                 is_error / is_warning / is_info / is_hint",
            );
        }
    }

    #[test]
    fn severity_predicates_are_byte_equal_to_matches_family() {
        // Fail-before-pass-after pin on the two-axis convergence of
        // the four pre-lift `d.severity == Severity::Error` sites at
        // `caixa-lint/src/rules.rs` (the `aplicacao-completeness`
        // rule-firing assertion + the `clean_manifest_only_info_level`
        // error-filter) and `caixa-feira/src/cmd/lint.rs` (the
        // `--errors-only` retain + the per-diagnostic `error_count`
        // bump) onto the [`gen_platform::IsVariant`]-derive-generated
        // per-arm predicate family: for every arm, each predicate
        // agrees byte-for-byte with the pre-lift `matches!(_,
        // Severity::…)` shape.
        //
        // A future rebrand touching either endpoint (a
        // `#[is_variant(name = "…")]` attribute drift on the derive,
        // an arm rename, an accidental peer predicate that shadows
        // the derive-generated one) would silently split the two
        // paths and trip this pin. Peer of the sibling
        // [`fix_safety_predicates_are_byte_equal_to_matches_family`]
        // pin on the peer `caixa-lint` closed-set-enum `IsVariant`
        // convergence axis (732a791) and the workspace
        // `caixa_arch::report::tests::arch_verdict_predicates_are_byte_equal_to_matches_family`
        // sibling pin (94dafa6).
        for arm in Severity::ALL {
            assert_eq!(
                arm.is_error(),
                matches!(arm, Severity::Error),
                "Severity::{arm:?}.is_error() must agree with \
                 matches!(_, Severity::Error) byte-for-byte",
            );
            assert_eq!(
                arm.is_warning(),
                matches!(arm, Severity::Warning),
                "Severity::{arm:?}.is_warning() must agree with \
                 matches!(_, Severity::Warning) byte-for-byte",
            );
            assert_eq!(
                arm.is_info(),
                matches!(arm, Severity::Info),
                "Severity::{arm:?}.is_info() must agree with \
                 matches!(_, Severity::Info) byte-for-byte",
            );
            assert_eq!(
                arm.is_hint(),
                matches!(arm, Severity::Hint),
                "Severity::{arm:?}.is_hint() must agree with \
                 matches!(_, Severity::Hint) byte-for-byte",
            );
        }
    }

    #[test]
    fn severity_error_family_dispatches_through_is_error_across_full_arm_set() {
        // Byte-parity pin on the four converged `d.severity.is_error()`
        // sites (`caixa-lint/src/rules.rs` :674 + :738,
        // `caixa-feira/src/cmd/lint.rs` :108 + :111): for every arm in
        // [`Severity::ALL`], a [`Diagnostic`] carrying that severity
        // reports `is_error()` iff the arm is `Error`. Guards against a
        // future silent split between the four call sites and the
        // derived predicate (a hand-rolled `== Severity::Error`
        // reintroduction, an arm rename that touches one site but not
        // the others, an accidental peer predicate that shadows
        // `is_error`) by asserting the two paths return the same
        // `bool` on every arm.
        //
        // The paired dummy [`Diagnostic`] materializes the "runner-
        // facing" projection the four production sites actually
        // dispatch on (each is `d.severity == Severity::Error` on a
        // borrowed [`Diagnostic`]), not just the free-standing
        // arm-discriminator the peer
        // [`severity_predicates_are_byte_equal_to_matches_family`] pin
        // covers — so a hypothetical future accessor-shim that widens
        // the runner-side severity view (a per-cluster override, an
        // operator-derived severity remap) that failed to route
        // through [`Severity::is_error`] would surface here rather
        // than at apply time far from the shim commit.
        for &severity in Severity::ALL {
            let d = Diagnostic {
                rule_id: "test-rule",
                severity,
                message: String::new(),
                span: Span::default(),
                hint: None,
                fix: None,
            };
            assert_eq!(
                d.severity.is_error(),
                matches!(severity, Severity::Error),
                "Diagnostic::severity.is_error() must dispatch through \
                 Severity::is_error() byte-for-byte on {severity:?}",
            );
        }
    }

    #[test]
    fn severity_as_ref_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after pin on the `impl AsRef<str> for
        // Severity` trait-idiomatic `&str`-projection axis: for every
        // arm in [`Severity::ALL`], the standard-library `.as_ref()`
        // dispatch must resolve to the same byte-string the existing
        // [`Severity::as_str`] `pub const fn` scalar accessor returns.
        // Guards against a future silent split between the trait impl
        // and the substrate accessor (a hand-rolled `match self`
        // reintroduction inside the impl, an arm rename that touches
        // one path but not the other, an accidental peer accessor that
        // shadows [`Severity::as_str`]) by asserting the two paths
        // converge byte-for-byte across the full closed set.
        //
        // Peer of the sibling caixa-core `AsRef<str>`-through-`as_str`
        // pins ([`caixa_core::kind::tests::caixa_kind_as_ref_str_routes_through_as_str_accessor`]
        // cd2091f, [`caixa_core::dialeto::tests::caixa_dialeto_as_ref_str_routes_through_as_str_accessor`]
        // 1723611, [`caixa_core::aplicacao::tests::placement_strategy_as_ref_str_routes_through_as_str_accessor`]
        // d86edd2, [`caixa_core::supervisor::tests::restart_strategy_as_ref_str_routes_through_as_str_accessor`]
        // 63eb1a4, [`caixa_core::supervisor::tests::restart_policy_as_ref_str_routes_through_as_str_accessor`]
        // 419ea81, [`caixa_core::dep::tests::dep_list_as_ref_str_routes_through_as_str_accessor`]
        // df4592e).
        for &arm in Severity::ALL {
            let via_trait: &str = arm.as_ref();
            let via_accessor: &str = arm.as_str();
            assert_eq!(
                via_trait, via_accessor,
                "Severity::{arm:?}.as_ref() must resolve to the same \
                 byte-string as Severity::{arm:?}.as_str() — the \
                 AsRef<str> impl must dispatch through the substrate \
                 accessor",
            );
        }
    }

    #[test]
    fn severity_as_ref_str_returns_canonical_lowercase_tag_per_arm() {
        // Byte-parity pin on the [`Severity`] four-arm canonical
        // lowercase tag alphabet — the byte-string every downstream
        // `.as_ref()`-bound consumer (a `Command::arg` shell-out
        // composing `--severity=<tag>` on the runner's diagnostic-
        // overlay CLI, a `tracing::field::Value::Str`-arm structured-
        // log recorder on the per-diagnostic emission path, a
        // `HashMap::get::<str>(sev.as_ref())` lookup on a future
        // per-severity policy table) receives through one substrate-
        // primitive dispatch.
        //
        // A future rename of the canonical tag on any arm (a `"warn"`
        // shortening of [`Severity::Warning`], a `"err"` shortening
        // of [`Severity::Error`], a capitalisation drift on any of
        // the four) touches the [`Severity::as_str`] scalar accessor
        // and both this pin and the paired
        // [`severity_as_ref_str_routes_through_as_str_accessor`]
        // convergence pin catch it in one caixa-lint test run rather
        // than at a downstream consumer's silent misclassification.
        //
        // Peer of the sibling caixa-core canonical-byte-string arm-
        // family pins ([`caixa_core::kind::tests::caixa_kind_as_str_returns_lifted_peer_const`],
        // [`caixa_core::dialeto::tests::caixa_dialeto_as_str_returns_canonical_pascal_case_tag`]).
        assert_eq!(Severity::Error.as_ref(), "error");
        assert_eq!(Severity::Warning.as_ref(), "warning");
        assert_eq!(Severity::Info.as_ref(), "info");
        assert_eq!(Severity::Hint.as_ref(), "hint");
    }

    #[test]
    fn severity_display_and_as_ref_str_route_through_as_str_accessor() {
        // Three-path convergence pin: the paired [`std::fmt::Display`]
        // impl, the paired [`AsRef<str>`] impl, and the substrate-
        // canonical [`Severity::as_str`] `pub const fn` accessor must
        // resolve to the same `&'static str` per arm.
        //
        // Guards against any future silent detour that routes one impl
        // through a divergent projection (a hand-rolled per-arm match
        // in the `fmt` body, an `impl AsRef<str>` swap onto a
        // hypothetical wire_name axis, a rename that touches one
        // endpoint but not the paired sibling) — the pin trips at
        // caixa-lint test time rather than at a downstream consumer's
        // silent tag split. Peer of the sibling
        // [`caixa_arch::invariants::tests::invariant_kind_display_and_as_ref_str_route_through_as_str_accessor`]
        // (87c875a) three-path convergence pin on the caixa-arch
        // [`InvariantKind`] axis.
        for &arm in Severity::ALL {
            let via_as_str: &str = arm.as_str();
            let via_as_ref: &str = arm.as_ref();
            let via_display = arm.to_string();
            assert_eq!(
                via_as_ref, via_as_str,
                "Severity::{arm:?} AsRef<str>::as_ref() must byte-equal \
                 as_str()",
            );
            assert_eq!(
                via_display, via_as_str,
                "Severity::{arm:?} Display::fmt() must byte-equal \
                 as_str()",
            );
        }
    }

    #[test]
    fn severity_display_byte_equals_canonical_lowercase_tag_per_arm() {
        // Byte-parity pin on the [`Severity`] four-arm canonical
        // lowercase tag alphabet through the standard-library `{}`
        // Display axis — pinned separately from the paired
        // [`severity_as_ref_str_returns_canonical_lowercase_tag_per_arm`]
        // pin so a future silent swap of the `Display` impl onto a
        // divergent projection (a hand-rolled per-arm match in the
        // `fmt` body that shifts one arm, a routing detour through the
        // derived `Debug` output shape, an `impl Display` swap that
        // reads from a hypothetical peer accessor) trips at caixa-lint
        // test time rather than at the downstream `format!("{sev}")`
        // consumer's silent tag drift.
        assert_eq!(format!("{}", Severity::Error), "error");
        assert_eq!(format!("{}", Severity::Warning), "warning");
        assert_eq!(format!("{}", Severity::Info), "info");
        assert_eq!(format!("{}", Severity::Hint), "hint");
    }

    #[test]
    fn fix_safety_as_str_returns_canonical_lowercase_tag_per_arm() {
        // Fail-before-pass-after pin on the [`FixSafety`] two-arm
        // canonical lowercase tag alphabet — the byte-string every
        // downstream `.as_str()`-bound consumer (a future
        // `feira lint --fix-safety=<tag>` verb enumerating the
        // accepted tier list, a `tracing::field::Value::Str`-arm
        // structured-log recorder on the runner's per-fix emission
        // path, a `HashMap::get::<str>(safety.as_str())` lookup on a
        // future per-tier policy table) receives through one
        // substrate-primitive dispatch. Matches the paired
        // [`gen_platform::IsVariant`]-derive-generated per-arm
        // predicate names ([`FixSafety::is_safe`] /
        // [`FixSafety::is_unsafe`]) verbatim.
        //
        // A future rename of the canonical tag on any arm (a `"soft"`
        // shortening of [`FixSafety::Safe`], a `"risky"` shortening
        // of [`FixSafety::Unsafe`], a capitalisation drift on either
        // of the two) touches the [`FixSafety::as_str`] scalar
        // accessor and both this pin and the paired
        // [`fix_safety_as_ref_str_routes_through_as_str_accessor`]
        // convergence pin catch it in one caixa-lint test run rather
        // than at a downstream consumer's silent misclassification.
        //
        // Peer of the sibling
        // [`severity_as_ref_str_returns_canonical_lowercase_tag_per_arm`]
        // pin on the paired caixa-lint severity axis (ce9d1e3 / 6ad94f3).
        assert_eq!(FixSafety::Safe.as_str(), "safe");
        assert_eq!(FixSafety::Unsafe.as_str(), "unsafe");
    }

    #[test]
    fn fix_safety_as_ref_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after pin on the `impl AsRef<str> for
        // FixSafety` trait-idiomatic `&str`-projection axis: for every
        // arm in [`FixSafety::ALL`], the standard-library `.as_ref()`
        // dispatch must resolve to the same byte-string the paired
        // [`FixSafety::as_str`] `pub const fn` scalar accessor returns.
        // Guards against a future silent split between the trait impl
        // and the substrate accessor (a hand-rolled `match self`
        // reintroduction inside the impl, an arm rename that touches
        // one path but not the other, an accidental peer accessor that
        // shadows [`FixSafety::as_str`]) by asserting the two paths
        // converge byte-for-byte across the full closed set.
        //
        // Peer of the sibling caixa-lint
        // [`severity_as_ref_str_routes_through_as_str_accessor`]
        // convergence pin (ce9d1e3) and the caixa-arch
        // `invariant_kind_as_ref_str_routes_through_as_str_accessor`
        // (87c875a) / `arch_verdict_display_and_as_ref_str_route_through_as_str_accessor`
        // (f3da79b) peers.
        for &arm in FixSafety::ALL {
            let via_trait: &str = arm.as_ref();
            let via_accessor: &str = arm.as_str();
            assert_eq!(
                via_trait, via_accessor,
                "FixSafety::{arm:?}.as_ref() must resolve to the same \
                 byte-string as FixSafety::{arm:?}.as_str() — the \
                 AsRef<str> impl must dispatch through the substrate \
                 accessor",
            );
        }
    }

    #[test]
    fn fix_safety_display_and_as_ref_str_route_through_as_str_accessor() {
        // Three-path convergence pin: the paired [`std::fmt::Display`]
        // impl, the paired [`AsRef<str>`] impl, and the substrate-
        // canonical [`FixSafety::as_str`] `pub const fn` accessor must
        // resolve to the same `&'static str` per arm.
        //
        // Guards against any future silent detour that routes one impl
        // through a divergent projection (a hand-rolled per-arm match
        // in the `fmt` body, an `impl AsRef<str>` swap onto a
        // hypothetical peer accessor, a rename that touches one
        // endpoint but not the paired sibling) — the pin trips at
        // caixa-lint test time rather than at a downstream consumer's
        // silent tag split. Peer of the sibling caixa-lint
        // [`severity_display_and_as_ref_str_route_through_as_str_accessor`]
        // (6ad94f3), caixa-arch
        // `invariant_kind_display_and_as_ref_str_route_through_as_str_accessor`
        // (87c875a), and
        // `arch_verdict_display_and_as_ref_str_route_through_as_str_accessor`
        // (f3da79b) three-path convergence pins — closes the
        // `(as_str, Display, AsRef<str>)` canonical-projection triple
        // on the last remaining closed-set fieldless typed enum on the
        // caixa-lint surface without it.
        for &arm in FixSafety::ALL {
            let via_as_str: &str = arm.as_str();
            let via_as_ref: &str = arm.as_ref();
            let via_display = arm.to_string();
            assert_eq!(
                via_as_ref, via_as_str,
                "FixSafety::{arm:?} AsRef<str>::as_ref() must byte-equal \
                 as_str()",
            );
            assert_eq!(
                via_display, via_as_str,
                "FixSafety::{arm:?} Display::fmt() must byte-equal \
                 as_str()",
            );
        }
    }

    #[test]
    fn severity_from_wire_accepts_every_as_str_output() {
        // Fail-before-pass-after per-arm accept pin on the newly lifted
        // [`Severity::from_wire`] reverse projection: every arm in
        // [`Severity::ALL`] must parse back through `from_wire` when
        // fed its own [`Severity::as_str`] output, landing on
        // `Some(same_variant)`. A regression that hand-rolled either
        // side's per-arm match without threading through the shared
        // four-string closed set would silently disagree on any future
        // arm rename (or a new arm the rulebook grows — a `Debug` tier
        // below [`Severity::Hint`] once verbose per-node lint traces
        // enter scope, a `Critical` tier above [`Severity::Error`] the
        // M3-and-later LSP surfaces for build-halting failures) and
        // this pin flags it at caixa-lint build time rather than at a
        // downstream `feira lint --severity` consumer's silent tag
        // misclassification.
        //
        // Peer of the sibling
        // `caixa_arch::report::tests::arch_verdict_from_wire_accepts_every_as_str_output`
        // (6afe564) /
        // `caixa_arch::invariants::tests::invariant_kind_from_wire_accepts_every_as_str_output`
        // (b9e4e61) round-trip pins on the peer caixa-arch closed-set-
        // enum reverse-projection axes, and of the sibling
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
        for &variant in Severity::ALL {
            let wire = variant.as_str();
            let parsed = Severity::from_wire(wire).unwrap_or_else(|| {
                panic!(
                    "Severity::from_wire({wire:?}) must accept every \
                     Severity::as_str output — got None for the wire \
                     byte-string of {variant:?}"
                )
            });
            assert_eq!(
                parsed, variant,
                "Severity::from_wire(Severity::{variant:?}.as_str()) \
                 must return Severity::{variant:?} — the (as_str, \
                 from_wire) pair must form a total round-trip on the \
                 closed four-arm Severity arm-set",
            );
        }
    }

    #[test]
    fn severity_from_wire_rejects_unknown_byte_strings() {
        // Rejection pin on the [`Severity::from_wire`] parser's
        // accept-set: any string outside the four-arm
        // [`Severity::as_str`] output set must return `None`. A future
        // accidental widening of the accept-set (a case-insensitive
        // match that accepts `"ERROR"` / `"Error"`, a silent acceptance
        // of the pre-lift PascalCase Debug-derived shapes `"Error"` /
        // `"Warning"` / `"Info"` / `"Hint"` on the wire axis, a
        // Levenshtein-forgiving arm-lookup that admits `"eror"` /
        // `"warn"` typos, a silent absorption of the sibling
        // [`FixSafety::as_str`] two-arm accept-set — the two axes share
        // no byte-strings but a widened parser could still misclassify
        // a peer's arm-tag as a severity, a silent absorption of the
        // sibling `caixa_arch::invariants::InvariantKind::as_str` set
        // where the two axes DO share `"hint"` — the parser must not
        // widen the accept-set beyond the four-arm severity tag alphabet
        // even when a peer axis emits an overlapping byte-string) would
        // silently drift the parser's accept-set from the emitter's — a
        // downstream lint-report re-loader that bound a prior report's
        // [`Self::as_str`] output back to the typed enum through this
        // parser would then bind a malformed byte-string to a plausibly-
        // wrong typed arm the caller does not route through any
        // fallback, silently misclassifying the reloaded row.
        //
        // Peer of the sibling
        // `caixa_arch::report::tests::arch_verdict_from_wire_rejects_unknown_byte_strings`
        // (6afe564) /
        // `caixa_arch::invariants::tests::invariant_kind_from_wire_rejects_unknown_byte_strings`
        // (b9e4e61) rejection pins on the peer caixa-arch axes, and of
        // the sibling
        // `caixa_kind_from_wire_rejects_unknown_byte_strings` (2aa6d23),
        // `caixa_dialeto_from_wire_rejects_unknown_byte_strings`
        // (d0e65ea),
        // `placement_strategy_from_wire_rejects_unknown_byte_strings`
        // (18c7342),
        // `dep_list_from_wire_returns_none_on_unknown_wire_scalar`
        // (45ee563), and
        // `path_shape_violation_from_wire_rejects_unknown_byte_strings`
        // (aebd9c6) rejection pins on the sibling caixa-core axes.
        for bad in [
            "",
            " ",
            "Error",
            "ERROR",
            "Warning",
            "WARNING",
            "Info",
            "INFO",
            "Hint",
            "HINT",
            "eror",
            "warn",
            "informational",
            "hnt",
            "safe",
            "unsafe",
            "safety",
            "compliance",
            "proven",
            "rejected",
            "fatal",
            "debug",
            "critical",
            "error ",
            " error",
            "error\n",
            "error\t",
            "warning ",
            " warning",
            "info ",
            " info",
            "hint ",
            " hint",
        ] {
            assert!(
                Severity::from_wire(bad).is_none(),
                "Severity::from_wire({bad:?}) must return None — the \
                 parser's accept-set is exactly the four Severity::as_str \
                 outputs; a widening would silently split the parser's \
                 accept-set from the emitter's arm-set",
            );
        }
    }

    #[test]
    fn severity_try_from_str_routes_through_from_wire_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl TryFrom<&str> for Severity` — asserts the standard-
        // library trait impl and the substrate-primitive
        // [`super::Severity::from_wire`] `Option<Self>` accessor resolve
        // to the same four-arm accept-set across every arm the
        // exhaustive [`super::Severity::ALL`] slice enumerates. Any
        // future silent detour that routes the trait impl through a
        // divergent projection (a per-arm inline `match s { "error" =>
        // Ok(Self::Error), … }` re-inlining that opens a compile-time
        // link to the un-lifted arm-literal, a silent case-fold that
        // admits `"Error"` / `"Warning"` / `"Info"` / `"Hint"` and would
        // collide the canonical-lowercase accept-set the emitter
        // dispatches on) trips at caixa-lint test time under
        // `assert_eq!` rather than at a downstream
        // `impl TryFrom<&str>`-bound consumer's silent split. Sweeps
        // every one of the four arms [`super::Severity::ALL`] carries so
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
        // (e21a857), and
        // `caixa_arch::report::tests::arch_verdict_try_from_str_routes_through_from_wire_accessor`
        // (0a4cc45) — extends the trait-idiomatic reverse-projection
        // axis onto the first closed-set fieldless typed enum on the
        // caixa-lint surface (the diagnostic-severity axis).
        for &variant in Severity::ALL {
            let wire = variant.as_str();
            assert_eq!(
                <Severity as TryFrom<&str>>::try_from(wire),
                Ok(variant),
                "TryFrom<&str> impl on Severity must round-trip \
                 Severity::{variant:?}.as_str() = {wire:?} back to \
                 Ok(Severity::{variant:?}) — divergence from \
                 Severity::from_wire signals a silent detour off the \
                 substrate-primitive accessor",
            );
            assert_eq!(
                <Severity as TryFrom<&str>>::try_from(wire).ok(),
                Severity::from_wire(wire),
                "TryFrom<&str> ok()-projection on {wire:?} must byte-equal \
                 Severity::from_wire on the same input",
            );
        }
    }

    #[test]
    fn severity_try_from_str_rejects_unknown_byte_strings() {
        // Rejection witness on the `impl TryFrom<&str> for Severity` —
        // sweeps a candidate set of byte-strings outside the four-arm
        // canonical-lowercase wire accept-set the sibling
        // [`super::Severity::as_str`] emits and asserts every one lands
        // on `Err(())`, so a future accidental widening of the trait
        // impl's accept-set (a stray additional
        // `_ if s.eq_ignore_ascii_case("error") => Ok(…)` case-fold
        // path, a silent acceptance of the pre-lift PascalCase Debug-
        // derived shapes `"Error"` / `"Warning"` / `"Info"` / `"Hint"`
        // on the wire axis, a Levenshtein-forgiving arm-lookup that
        // admits `"eror"` / `"warn"` typos — the exact form a
        // `format!("{:?}", …).to_lowercase()` round-trip on the paired
        // [`std::fmt::Debug`] derive would otherwise land on, the drift
        // footgun the emitter's documentation explicitly names as the
        // reason the substrate-canonical lowercase `"error"` /
        // `"warning"` / `"info"` / `"hint"` slug set exists) trips at
        // caixa-lint test time. The candidate set includes the empty
        // string, whitespace-only padding, uppercase rebrand candidates,
        // Levenshtein-neighbor typos, sibling closed-set-enum canonical
        // tags on the peer [`super::FixSafety::as_str`] two-arm accept-
        // set (`"safe"` / `"unsafe"`) and the peer
        // `caixa_arch::invariants::InvariantKind::as_str` three-arm
        // arch-severity set (`"safety"` / `"compliance"` — non-shared
        // with this axis's four-arm severity set; the shared `"hint"`
        // arm between this axis and the arch-severity axis is a
        // coincidence of lowercase-tag choice, not a typed cross-axis
        // promise, but the two axes' `"hint"` arm DOES belong to this
        // axis, so `"hint"` is deliberately excluded from the
        // rejection set), and trailing/leading-whitespace-padded
        // canonical tags.
        //
        // Peer of the sibling
        // [`caixa_core::kind::tests::caixa_kind_try_from_str_rejects_unknown_byte_strings`]
        // (3c83606),
        // [`caixa_core::dialeto::tests::caixa_dialeto_try_from_str_rejects_unknown_byte_strings`]
        // (bf33136),
        // `rate_limit_unit_try_from_str_rejects_unknown_byte_strings`
        // (bf78400),
        // `path_shape_violation_try_from_str_rejects_unknown_byte_strings`
        // (e67e48a),
        // `invariant_kind_try_from_str_rejects_unknown_byte_strings`
        // (e21a857), and
        // `arch_verdict_try_from_str_rejects_unknown_byte_strings`
        // (0a4cc45) rejection pins on the sibling closed-set typed-enum
        // trait-idiomatic reverse-projection axes.
        for bad in [
            "",
            " ",
            "Error",
            "ERROR",
            "Warning",
            "WARNING",
            "Info",
            "INFO",
            "Hint",
            "HINT",
            "eror",
            "warn",
            "informational",
            "hnt",
            "safe",
            "unsafe",
            "safety",
            "compliance",
            "proven",
            "rejected",
            "fatal",
            "debug",
            "critical",
            "error ",
            " error",
            "error\n",
            "error\t",
            "warning ",
            " warning",
            "info ",
            " info",
            "hint ",
            " hint",
        ] {
            assert_eq!(
                <Severity as TryFrom<&str>>::try_from(bad),
                Err(()),
                "TryFrom<&str> for Severity({bad:?}) must return \
                 Err(()) — the trait impl's accept-set is exactly the \
                 four Severity::as_str outputs; a widening would \
                 silently split the trait impl's accept-set from the \
                 emitter's arm-set",
            );
        }
    }

    #[test]
    fn severity_try_from_str_and_from_wire_partition_the_accept_set() {
        // Cross-axis partition pin: the trait-idiomatic
        // [`TryFrom<&str>`] and the method-named
        // [`super::Severity::from_wire`] projections must return
        // equivalent decisions on every input — the trait impl's `.ok()`
        // project-out from `Result<Self, ()>` and the method's
        // `Option<Self>` return must byte-equal each other on both
        // accepts and rejects. A future silent bifurcation (the trait
        // impl gaining a case-fold path the method does not carry, the
        // method gaining a synonym alias the trait impl does not honor)
        // trips at caixa-lint test time under a single pin rather than
        // at a downstream generic-bound consumer that dispatches through
        // one axis while a peer dispatches through the other. Sweeps
        // both the four-arm accept-set (via [`super::Severity::ALL`]
        // threaded through [`super::Severity::as_str`]) and a canonical
        // rejection sample so both halves of the partition are covered.
        for &variant in Severity::ALL {
            let wire = variant.as_str();
            assert_eq!(
                <Severity as TryFrom<&str>>::try_from(wire).ok(),
                Severity::from_wire(wire),
                "TryFrom<&str>::ok() and from_wire must agree on \
                 Severity::{variant:?}.as_str() = {wire:?}",
            );
        }
        for bad in ["", "Error", "unknown", "safety", "safe"] {
            assert_eq!(
                <Severity as TryFrom<&str>>::try_from(bad).ok(),
                Severity::from_wire(bad),
                "TryFrom<&str>::ok() and from_wire must agree on the \
                 rejection outcome for {bad:?}",
            );
        }
    }

    #[test]
    fn severity_from_into_static_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<Severity> for &'static str` — asserts the
        // standard-library trait impl and the substrate-primitive
        // [`super::Severity::as_str`] `pub const fn` accessor resolve
        // to the same four-arm canonical-lowercase emit-set across
        // every arm the exhaustive [`super::Severity::ALL`] slice
        // enumerates. Any future silent detour that routes the trait
        // impl through a divergent projection (a per-arm inline
        // `match sev { Error => "error", … }` re-inlining that opens
        // a compile-time link to the un-lifted arm-literal outside
        // the paired [`super::Severity::as_str`] dispatch, a swap
        // onto a `format!("{:?}", …).to_lowercase()` round-trip
        // through the `#[derive(Debug)]` output whose stability is
        // *not* guaranteed and would silently reroute the diagnostic
        // tag through a stale byte-string with no downstream signal
        // until an operator scrolled the `feira lint` terminal —
        // the exact drift footgun the sibling
        // [`super::Severity::as_str`] documentation explicitly
        // names) trips at caixa-lint test time under `assert_eq!`
        // rather than at a downstream `impl Into<&'static
        // str>`-bound consumer's silent split. Sweeps every one of
        // the four arms [`super::Severity::ALL`] carries so no arm's
        // projection is covered only by the sibling method-named
        // `as_str` / [`std::fmt::Display`] / [`AsRef<str>`] paths.
        // Materializes the `<&'static str as
        // From<Severity>>::from` output in four `const`-shape
        // bindings against the paired [`super::Severity::as_str`]
        // `pub const fn` accessor to make the `'static` lifetime
        // promise a build-time invariant — a future accidental
        // downgrade of any arm's inline canonical-lowercase
        // byte-string to a non-`&'static str` (a `String::leak()`-
        // produced return, a `Box::leak`-cast, an intermediate
        // lifetime-erasing helper) trips at caixa-lint build time
        // rather than at a downstream `'static`-bound consumer.
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
        // (f2ca7bc), and
        // `caixa_arch::report::tests::arch_verdict_from_into_static_str_routes_through_as_str_accessor`
        // (d4559cb) pins on the sibling closed-set typed-enum forward-
        // projection axes — extends the trait-idiomatic forward-
        // projection axis onto the first closed-set fieldless typed
        // enum on the caixa-lint surface (the diagnostic-severity
        // axis), the third outside-caixa-core closed-set fieldless
        // typed enum on the caixa surface (after the two peer
        // caixa-arch axes at f2ca7bc / d4559cb).
        const ERROR: &str = Severity::Error.as_str();
        const WARNING: &str = Severity::Warning.as_str();
        const INFO: &str = Severity::Info.as_str();
        const HINT: &str = Severity::Hint.as_str();
        for &variant in Severity::ALL {
            let via_trait: &'static str = <&'static str as From<Severity>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<Severity> for &'static str impl must round-trip \
                 Severity::{variant:?} to the same canonical-lowercase \
                 byte-string Severity::as_str returns — divergence \
                 signals a silent detour off the substrate-primitive \
                 accessor"
            );
            let via_into: &'static str = variant.into();
            assert_eq!(
                via_into, via_method,
                "Into<&'static str>::into on Severity::{variant:?} \
                 must byte-equal Severity::as_str on the same input \
                 — the blanket-derived Into shape must resolve to the \
                 same as_str dispatch as the explicit From impl"
            );
        }
        assert_eq!(
            [ERROR, WARNING, INFO, HINT],
            ["error", "warning", "info", "hint"],
            "const-context Severity::as_str must resolve to the four \
             canonical-lowercase byte-strings — a future accidental \
             downgrade of any arm to a non-const or non-static \
             byte-string breaks the `&'static str`-lifetime promise \
             the paired From<Severity> for &'static str impl carries \
             by construction"
        );
    }

    #[test]
    fn severity_from_into_static_str_and_as_str_partition_the_emit_set() {
        // Cross-axis partition pin: the paired trait-idiomatic
        // `From<Severity> for &'static str` forward projection and
        // the method-named [`super::Severity::as_str`] forward
        // projection must resolve identically on *every* arm, not
        // just the ones named in the primary byte-parity pin above.
        // Sweeps every [`super::Severity::ALL`] arm and asserts the
        // trait's `From::from` output byte-equals the method-named
        // accessor's return-value on each, locking the two forward-
        // projection paths together by construction so any future
        // detour (a stray `From` special-case that lands on a
        // divergent per-arm literal outside the paired `as_str`
        // dispatch, a hypothetical rebrand touching one axis without
        // the other) trips at caixa-lint test time.
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
        // (f2ca7bc), and
        // `caixa_arch::report::tests::arch_verdict_from_into_static_str_and_as_str_partition_the_emit_set`
        // (d4559cb) — extends the round-trip discipline onto the
        // first closed-set fieldless typed enum on the caixa-lint
        // surface, closing the two-way `Self ↔ &'static str`
        // round-trip on the trait-idiomatic pair (`From<Self> for
        // &'static str` + `TryFrom<&str> for Self`) as well as the
        // pre-existing method-named pair (`as_str` + `from_wire`).
        for &variant in Severity::ALL {
            let via_trait: &'static str = <&'static str as From<Severity>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<Severity> for &'static str and Severity::as_str \
                 must resolve identically on Severity::{variant:?} — \
                 divergence signals the two forward-projection paths \
                 have drifted onto different emit-sets"
            );
        }
        // Round-trip witness: every arm's forward `From` output
        // re-parses through the paired trait-idiomatic reverse
        // `TryFrom<&str>` back to the original variant. Closes the
        // two-way `Severity ↔ &'static str` round-trip on the
        // trait-idiomatic axis pair directly (no wire-vocab
        // intermediate — the emit-side [`super::Severity::as_str`]
        // and the parse-side [`super::Severity::from_wire`] dispatch
        // on the same four inline canonical-lowercase byte-strings
        // by construction, so round-tripping through the paired
        // `From<Self> for &'static str` + `TryFrom<&str> for Self`
        // trait impls composes to the identity on `Severity::ALL`).
        for &variant in Severity::ALL {
            let emitted: &'static str = <&'static str as From<Severity>>::from(variant);
            let reparsed = <Severity as TryFrom<&str>>::try_from(emitted).unwrap_or_else(|()| {
                panic!(
                    "TryFrom<&str> for Severity must accept every \
                     From<Severity> for &'static str output — got \
                     Err(()) for Severity::{variant:?}'s emit \
                     byte-string {emitted:?}"
                )
            });
            assert_eq!(
                reparsed, variant,
                "trait-idiomatic Severity ↔ &'static str round-trip \
                 must be the identity on Severity::{variant:?} — the \
                 From<Self> for &'static str + TryFrom<&str> for Self \
                 pair must compose to the identity on the closed \
                 four-arm accept-set"
            );
        }
    }

    #[test]
    fn fix_safety_from_wire_accepts_every_as_str_output() {
        // Fail-before-pass-after per-arm accept pin on the newly lifted
        // [`FixSafety::from_wire`] reverse projection: every arm in
        // [`FixSafety::ALL`] must parse back through `from_wire` when
        // fed its own [`FixSafety::as_str`] output, landing on
        // `Some(same_variant)`. A regression that hand-rolled either
        // side's per-arm match without threading through the shared
        // two-string closed set would silently disagree on any future
        // arm rename (or a new arm the runner grows — an `Experimental`
        // tier between [`FixSafety::Safe`] and [`FixSafety::Unsafe`] the
        // M3-and-later lint runner grows for AI-suggested rewrites that
        // need explicit review-and-accept) and this pin flags it at
        // caixa-lint build time rather than at a downstream
        // `feira lint --fix-safety` consumer's silent tag
        // misclassification.
        //
        // Peer of the sibling
        // [`severity_from_wire_accepts_every_as_str_output`] (5afff0e)
        // pin on the paired caixa-lint diagnostic-severity axis, and of
        // `caixa_arch::report::tests::arch_verdict_from_wire_accepts_every_as_str_output`
        // (6afe564) /
        // `caixa_arch::invariants::tests::invariant_kind_from_wire_accepts_every_as_str_output`
        // (b9e4e61) round-trip pins on the peer caixa-arch closed-set-
        // enum reverse-projection axes, and of the sibling
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
        for &variant in FixSafety::ALL {
            let wire = variant.as_str();
            let parsed = FixSafety::from_wire(wire).unwrap_or_else(|| {
                panic!(
                    "FixSafety::from_wire({wire:?}) must accept every \
                     FixSafety::as_str output — got None for the wire \
                     byte-string of {variant:?}"
                )
            });
            assert_eq!(
                parsed, variant,
                "FixSafety::from_wire(FixSafety::{variant:?}.as_str()) \
                 must return FixSafety::{variant:?} — the (as_str, \
                 from_wire) pair must form a total round-trip on the \
                 closed two-arm FixSafety arm-set",
            );
        }
    }

    #[test]
    fn fix_safety_from_wire_rejects_unknown_byte_strings() {
        // Rejection pin on the [`FixSafety::from_wire`] parser's
        // accept-set: any string outside the two-arm
        // [`FixSafety::as_str`] output set must return `None`. A future
        // accidental widening of the accept-set (a case-insensitive
        // match that accepts `"SAFE"` / `"Safe"`, a silent acceptance
        // of the pre-lift PascalCase Debug-derived shapes `"Safe"` /
        // `"Unsafe"` on the wire axis, a Levenshtein-forgiving arm-
        // lookup that admits `"saf"` / `"unsaf"` typos, a silent
        // absorption of the sibling [`crate::Severity::as_str`] four-arm
        // accept-set — the two axes share no byte-strings but a widened
        // parser could still misclassify a peer's arm-tag as a fix-
        // safety tier, a silent absorption of the sibling
        // `caixa_arch::invariants::InvariantKind::as_str` set where the
        // two axes share the byte-string `"safety"` — the parser must
        // NOT widen the accept-set to admit `"safety"` even though a
        // peer axis emits it, since that byte-string is the caixa-arch
        // invariant-kind tag rather than a fix-safety tier) would
        // silently drift the parser's accept-set from the emitter's —
        // a downstream lint-report re-loader that bound a prior
        // report's [`Self::as_str`] output back to the typed enum
        // through this parser would then bind a malformed byte-string
        // to a plausibly-wrong typed arm the caller does not route
        // through any fallback, silently misclassifying the reloaded
        // row.
        //
        // Peer of the sibling
        // [`severity_from_wire_rejects_unknown_byte_strings`] (5afff0e)
        // pin on the paired caixa-lint diagnostic-severity axis, and of
        // `caixa_arch::report::tests::arch_verdict_from_wire_rejects_unknown_byte_strings`
        // (6afe564) /
        // `caixa_arch::invariants::tests::invariant_kind_from_wire_rejects_unknown_byte_strings`
        // (b9e4e61) rejection pins on the peer caixa-arch axes, and of
        // the sibling
        // `caixa_kind_from_wire_rejects_unknown_byte_strings` (2aa6d23),
        // `caixa_dialeto_from_wire_rejects_unknown_byte_strings`
        // (d0e65ea),
        // `placement_strategy_from_wire_rejects_unknown_byte_strings`
        // (18c7342),
        // `dep_list_from_wire_returns_none_on_unknown_wire_scalar`
        // (45ee563), and
        // `path_shape_violation_from_wire_rejects_unknown_byte_strings`
        // (aebd9c6) rejection pins on the sibling caixa-core axes.
        for bad in [
            "",
            " ",
            "Safe",
            "SAFE",
            "Unsafe",
            "UNSAFE",
            "safe ",
            " safe",
            "safe\n",
            "safe\t",
            "unsafe ",
            " unsafe",
            "saf",
            "unsaf",
            "un-safe",
            "un_safe",
            "safer",
            "unsafer",
            "experimental",
            "error",
            "warning",
            "info",
            "hint",
            "safety",
            "compliance",
            "proven",
            "rejected",
            "biblioteca",
            "servico",
            "one-for-one",
        ] {
            assert!(
                FixSafety::from_wire(bad).is_none(),
                "FixSafety::from_wire({bad:?}) must return None — the \
                 parser's accept-set is exactly the two FixSafety::as_str \
                 outputs; a widening would silently split the parser's \
                 accept-set from the emitter's arm-set",
            );
        }
    }

    #[test]
    fn fix_safety_display_byte_equals_canonical_lowercase_tag_per_arm() {
        // Byte-parity pin on the [`FixSafety`] two-arm canonical
        // lowercase tag alphabet through the standard-library `{}`
        // Display axis — pinned separately from the paired
        // [`fix_safety_as_str_returns_canonical_lowercase_tag_per_arm`]
        // pin so a future silent swap of the `Display` impl onto a
        // divergent projection (a hand-rolled per-arm match in the
        // `fmt` body that shifts one arm, a routing detour through
        // the derived `Debug` output shape, an `impl Display` swap
        // that reads from a hypothetical peer accessor) trips at
        // caixa-lint test time rather than at the downstream
        // `format!("{safety}")` consumer's silent tag drift. Peer of
        // the sibling caixa-lint
        // [`severity_display_byte_equals_canonical_lowercase_tag_per_arm`]
        // (6ad94f3) pin on the paired severity axis.
        assert_eq!(format!("{}", FixSafety::Safe), "safe");
        assert_eq!(format!("{}", FixSafety::Unsafe), "unsafe");
    }

    #[test]
    fn fix_safety_try_from_str_routes_through_from_wire_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl TryFrom<&str> for FixSafety` — asserts the standard-
        // library trait impl and the substrate-primitive
        // [`super::FixSafety::from_wire`] `Option<Self>` accessor resolve
        // to the same two-arm accept-set across every arm the exhaustive
        // [`super::FixSafety::ALL`] slice enumerates. Any future silent
        // detour that routes the trait impl through a divergent
        // projection (a per-arm inline `match s { "safe" =>
        // Ok(Self::Safe), … }` re-inlining that opens a compile-time
        // link to the un-lifted arm-literal, a silent case-fold that
        // admits `"Safe"` / `"Unsafe"` and would collide the canonical-
        // lowercase accept-set the emitter dispatches on) trips at
        // caixa-lint test time under `assert_eq!` rather than at a
        // downstream `impl TryFrom<&str>`-bound consumer's silent split.
        // Sweeps every one of the two arms [`super::FixSafety::ALL`]
        // carries so no arm's projection is covered only by the sibling
        // method-named `from_wire` path.
        //
        // Peer of the sibling
        // [`severity_try_from_str_routes_through_from_wire_accessor`]
        // (a7bf74c) on the paired caixa-lint diagnostic-severity axis,
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
        // (e21a857), and
        // `caixa_arch::report::tests::arch_verdict_try_from_str_routes_through_from_wire_accessor`
        // (0a4cc45) — extends the trait-idiomatic reverse-projection
        // axis onto the second (and last) closed-set fieldless typed
        // enum on the caixa-lint surface (the fix-safety-tier axis),
        // closing the caixa-lint crate onto the substrate-wide two-way
        // `str ↔ Self` round-trip family.
        for &variant in FixSafety::ALL {
            let wire = variant.as_str();
            assert_eq!(
                <FixSafety as TryFrom<&str>>::try_from(wire),
                Ok(variant),
                "TryFrom<&str> impl on FixSafety must round-trip \
                 FixSafety::{variant:?}.as_str() = {wire:?} back to \
                 Ok(FixSafety::{variant:?}) — divergence from \
                 FixSafety::from_wire signals a silent detour off the \
                 substrate-primitive accessor",
            );
            assert_eq!(
                <FixSafety as TryFrom<&str>>::try_from(wire).ok(),
                FixSafety::from_wire(wire),
                "TryFrom<&str> ok()-projection on {wire:?} must byte-equal \
                 FixSafety::from_wire on the same input",
            );
        }
    }

    #[test]
    fn fix_safety_try_from_str_rejects_unknown_byte_strings() {
        // Rejection witness on the `impl TryFrom<&str> for FixSafety` —
        // sweeps a candidate set of byte-strings outside the two-arm
        // canonical-lowercase wire accept-set the sibling
        // [`super::FixSafety::as_str`] emits and asserts every one lands
        // on `Err(())`, so a future accidental widening of the trait
        // impl's accept-set (a stray additional
        // `_ if s.eq_ignore_ascii_case("safe") => Ok(…)` case-fold path,
        // a silent acceptance of the pre-lift PascalCase Debug-derived
        // shapes `"Safe"` / `"Unsafe"` on the wire axis, a Levenshtein-
        // forgiving arm-lookup that admits `"saf"` / `"unsaf"` typos —
        // the exact form a `format!("{:?}", …).to_lowercase()` round-
        // trip on the paired [`std::fmt::Debug`] derive would otherwise
        // land on, the drift footgun the emitter's documentation
        // explicitly names as the reason the substrate-canonical
        // lowercase `"safe"` / `"unsafe"` slug set exists) trips at
        // caixa-lint test time. The candidate set includes the empty
        // string, whitespace-only padding, uppercase / PascalCase
        // rebrand candidates, Levenshtein-neighbor typos, the M3-and-
        // later trajectory-item candidate `"experimental"` the sibling
        // [`super::FixSafety::ALL`] doc block names (which must reject
        // today and pass by construction when the arm lands), sibling
        // closed-set-enum canonical tags on the peer
        // [`super::Severity::as_str`] four-arm severity set (`"error"` /
        // `"warning"` / `"info"` / `"hint"`), the
        // `caixa_arch::invariants::InvariantKind::as_str` three-arm
        // arch-severity set (`"safety"` / `"compliance"` — the `"hint"`
        // arm shared with the caixa-lint severity axis is a coincidence
        // of lowercase-tag choice, but neither belongs to the
        // fix-safety-tier axis), the
        // `caixa_arch::report::ArchVerdict::as_str` two-arm outcome set
        // (`"proven"` / `"rejected"`), cross-crate kind / strategy
        // tags, and trailing/leading-whitespace-padded canonical tags.
        //
        // Peer of the sibling
        // [`severity_try_from_str_rejects_unknown_byte_strings`]
        // (a7bf74c) on the paired caixa-lint diagnostic-severity axis,
        // [`caixa_core::kind::tests::caixa_kind_try_from_str_rejects_unknown_byte_strings`]
        // (3c83606),
        // [`caixa_core::dialeto::tests::caixa_dialeto_try_from_str_rejects_unknown_byte_strings`]
        // (bf33136),
        // `rate_limit_unit_try_from_str_rejects_unknown_byte_strings`
        // (bf78400),
        // `path_shape_violation_try_from_str_rejects_unknown_byte_strings`
        // (e67e48a),
        // `invariant_kind_try_from_str_rejects_unknown_byte_strings`
        // (e21a857), and
        // `arch_verdict_try_from_str_rejects_unknown_byte_strings`
        // (0a4cc45) rejection pins on the sibling closed-set typed-enum
        // trait-idiomatic reverse-projection axes.
        for bad in [
            "",
            " ",
            "\t",
            "Safe",
            "SAFE",
            "Unsafe",
            "UNSAFE",
            "saf",
            "unsaf",
            "safer",
            "unsafer",
            "un-safe",
            "un_safe",
            "experimental",
            "error",
            "warning",
            "info",
            "hint",
            "safety",
            "compliance",
            "proven",
            "rejected",
            "biblioteca",
            "servico",
            "one-for-one",
            "safe ",
            " safe",
            "safe\n",
            "safe\t",
            "unsafe ",
            " unsafe",
        ] {
            assert_eq!(
                <FixSafety as TryFrom<&str>>::try_from(bad),
                Err(()),
                "TryFrom<&str> for FixSafety({bad:?}) must return \
                 Err(()) — the trait impl's accept-set is exactly the \
                 two FixSafety::as_str outputs; a widening would \
                 silently split the trait impl's accept-set from the \
                 emitter's arm-set",
            );
        }
    }

    #[test]
    fn fix_safety_try_from_str_and_from_wire_partition_the_accept_set() {
        // Cross-axis partition pin: the trait-idiomatic
        // [`TryFrom<&str>`] and the method-named
        // [`super::FixSafety::from_wire`] projections must return
        // equivalent decisions on every input — the trait impl's `.ok()`
        // project-out from `Result<Self, ()>` and the method's
        // `Option<Self>` return must byte-equal each other on both
        // accepts and rejects. A future silent bifurcation (the trait
        // impl gaining a case-fold path the method does not carry, the
        // method gaining a synonym alias the trait impl does not honor)
        // trips at caixa-lint test time under a single pin rather than
        // at a downstream generic-bound consumer that dispatches through
        // one axis while a peer dispatches through the other. Sweeps
        // both the two-arm accept-set (via [`super::FixSafety::ALL`]
        // threaded through [`super::FixSafety::as_str`]) and a canonical
        // rejection sample so both halves of the partition are covered.
        //
        // Peer of the sibling
        // [`severity_try_from_str_and_from_wire_partition_the_accept_set`]
        // (a7bf74c) partition pin on the paired caixa-lint severity
        // axis.
        for &variant in FixSafety::ALL {
            let wire = variant.as_str();
            assert_eq!(
                <FixSafety as TryFrom<&str>>::try_from(wire).ok(),
                FixSafety::from_wire(wire),
                "TryFrom<&str>::ok() and from_wire must agree on \
                 FixSafety::{variant:?}.as_str() = {wire:?}",
            );
        }
        for bad in ["", "Safe", "unknown", "error", "hint", "experimental"] {
            assert_eq!(
                <FixSafety as TryFrom<&str>>::try_from(bad).ok(),
                FixSafety::from_wire(bad),
                "TryFrom<&str>::ok() and from_wire must agree on the \
                 rejection outcome for {bad:?}",
            );
        }
    }

    #[test]
    fn fix_safety_from_into_static_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<FixSafety> for &'static str` — asserts the
        // standard-library trait impl and the substrate-primitive
        // [`super::FixSafety::as_str`] `pub const fn` accessor resolve
        // to the same two-arm canonical-lowercase emit-set across
        // every arm the exhaustive [`super::FixSafety::ALL`] slice
        // enumerates. Any future silent detour that routes the trait
        // impl through a divergent projection (a per-arm inline
        // `match safety { Safe => "safe", Unsafe => "unsafe" }`
        // re-inlining that opens a compile-time link to the un-lifted
        // arm-literal outside the paired [`super::FixSafety::as_str`]
        // dispatch, a swap onto a `format!("{:?}", …).to_lowercase()`
        // round-trip through the `#[derive(Debug)]` output whose
        // stability is *not* guaranteed and would silently reroute
        // the fix-safety tag through a stale byte-string with no
        // downstream signal until an operator scrolled the
        // `feira lint --fix` terminal — the exact drift footgun the
        // sibling [`super::FixSafety::as_str`] documentation
        // explicitly names) trips at caixa-lint test time under
        // `assert_eq!` rather than at a downstream `impl
        // Into<&'static str>`-bound consumer's silent split. Sweeps
        // every one of the two arms [`super::FixSafety::ALL`] carries
        // so no arm's projection is covered only by the sibling
        // method-named `as_str` / [`std::fmt::Display`] /
        // [`AsRef<str>`] paths. Materializes the `<&'static str as
        // From<FixSafety>>::from` output in two `const`-shape bindings
        // against the paired [`super::FixSafety::as_str`]
        // `pub const fn` accessor to make the `'static` lifetime
        // promise a build-time invariant — a future accidental
        // downgrade of any arm's inline canonical-lowercase
        // byte-string to a non-`&'static str` (a `String::leak()`-
        // produced return, a `Box::leak`-cast, an intermediate
        // lifetime-erasing helper) trips at caixa-lint build time
        // rather than at a downstream `'static`-bound consumer.
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
        // (d4559cb), and the paired sibling
        // [`severity_from_into_static_str_routes_through_as_str_accessor`]
        // (5cc3b8b) pins on the sibling closed-set typed-enum
        // forward-projection axes — extends the trait-idiomatic
        // forward-projection axis onto the second (and last) closed-
        // set fieldless typed enum on the caixa-lint surface (the
        // fix-safety-tier axis), closing the caixa-lint crate's two
        // closed-set fieldless typed enums onto the trait-idiomatic
        // forward-projection family.
        const SAFE: &str = FixSafety::Safe.as_str();
        const UNSAFE: &str = FixSafety::Unsafe.as_str();
        for &variant in FixSafety::ALL {
            let via_trait: &'static str = <&'static str as From<FixSafety>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<FixSafety> for &'static str impl must round-trip \
                 FixSafety::{variant:?} to the same canonical-lowercase \
                 byte-string FixSafety::as_str returns — divergence \
                 signals a silent detour off the substrate-primitive \
                 accessor"
            );
            let via_into: &'static str = variant.into();
            assert_eq!(
                via_into, via_method,
                "Into<&'static str>::into on FixSafety::{variant:?} \
                 must byte-equal FixSafety::as_str on the same input \
                 — the blanket-derived Into shape must resolve to the \
                 same as_str dispatch as the explicit From impl"
            );
        }
        assert_eq!(
            [SAFE, UNSAFE],
            ["safe", "unsafe"],
            "const-context FixSafety::as_str must resolve to the two \
             canonical-lowercase byte-strings — a future accidental \
             downgrade of any arm to a non-const or non-static \
             byte-string breaks the `&'static str`-lifetime promise \
             the paired From<FixSafety> for &'static str impl carries \
             by construction"
        );
    }

    #[test]
    fn fix_safety_from_into_static_str_and_as_str_partition_the_emit_set() {
        // Cross-axis partition pin: the paired trait-idiomatic
        // `From<FixSafety> for &'static str` forward projection and
        // the method-named [`super::FixSafety::as_str`] forward
        // projection must resolve identically on *every* arm, not
        // just the ones named in the primary byte-parity pin above.
        // Sweeps every [`super::FixSafety::ALL`] arm and asserts the
        // trait's `From::from` output byte-equals the method-named
        // accessor's return-value on each, locking the two forward-
        // projection paths together by construction so any future
        // detour (a stray `From` special-case that lands on a
        // divergent per-arm literal outside the paired `as_str`
        // dispatch, a hypothetical rebrand touching one axis without
        // the other) trips at caixa-lint test time.
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
        // (d4559cb), and the paired sibling
        // [`severity_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (5cc3b8b) — extends the round-trip discipline onto the
        // second (and last) closed-set fieldless typed enum on the
        // caixa-lint surface, closing the two-way `Self ↔ &'static str`
        // round-trip on the trait-idiomatic pair (`From<Self> for
        // &'static str` + `TryFrom<&str> for Self`) as well as the
        // pre-existing method-named pair (`as_str` + `from_wire`).
        for &variant in FixSafety::ALL {
            let via_trait: &'static str = <&'static str as From<FixSafety>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<FixSafety> for &'static str and FixSafety::as_str \
                 must resolve identically on FixSafety::{variant:?} — \
                 divergence signals the two forward-projection paths \
                 have drifted onto different emit-sets"
            );
        }
        // Round-trip witness: every arm's forward `From` output
        // re-parses through the paired trait-idiomatic reverse
        // `TryFrom<&str>` back to the original variant. Closes the
        // two-way `FixSafety ↔ &'static str` round-trip on the
        // trait-idiomatic axis pair directly (no wire-vocab
        // intermediate — the emit-side [`super::FixSafety::as_str`]
        // and the parse-side [`super::FixSafety::from_wire`] dispatch
        // on the same two inline canonical-lowercase byte-strings
        // by construction, so round-tripping through the paired
        // `From<Self> for &'static str` + `TryFrom<&str> for Self`
        // trait impls composes to the identity on `FixSafety::ALL`).
        for &variant in FixSafety::ALL {
            let emitted: &'static str = <&'static str as From<FixSafety>>::from(variant);
            let reparsed = <FixSafety as TryFrom<&str>>::try_from(emitted).unwrap_or_else(|()| {
                panic!(
                    "TryFrom<&str> for FixSafety must accept every \
                     From<FixSafety> for &'static str output — got \
                     Err(()) for FixSafety::{variant:?}'s emit \
                     byte-string {emitted:?}"
                )
            });
            assert_eq!(
                reparsed, variant,
                "trait-idiomatic FixSafety ↔ &'static str round-trip \
                 must be the identity on FixSafety::{variant:?} — the \
                 From<Self> for &'static str + TryFrom<&str> for Self \
                 pair must compose to the identity on the closed \
                 two-arm accept-set"
            );
        }
    }
}
