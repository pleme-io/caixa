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
}
