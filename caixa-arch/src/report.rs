use serde::{Deserialize, Serialize};

use crate::invariants::Violation;

/// Two-arm proof outcome the `caixa-arch` invariant sweep summarizes into
/// — the closed-set typed enum every consumer of the `check_manifest`
/// verdict keys off (the render-refusal gate in [`ArchReport::passed`],
/// the `feira tofu` HCL-emission block in
/// `caixa-feira/src/cmd/tofu.rs`).
///
/// The [`gen_platform::IsVariant`] derive emits per-arm
/// [`ArchVerdict::is_proven`] / [`ArchVerdict::is_rejected`] predicates
/// every consumer routes through — the same closed-set arm-discriminator
/// discipline the sibling `caixa_core::CaixaKind` /
/// `caixa_core::CaixaDialeto` / `caixa_core::PlacementStrategy` /
/// `caixa_core::RestartStrategy` / `caixa_core::RestartPolicy` /
/// `caixa_core::DepList` / `caixa_core::RateLimitUnit` /
/// `caixa_core::PathShapeViolation` / `caixa_arch::InvariantKind` /
/// `caixa_lint::FixSafety` closed-set fieldless typed enums already
/// carry. `Copy` joins the derive set so the predicate family can be
/// called by value on a shared borrow's field-read (`self.verdict.is_proven()`);
/// `Hash` joins so the enum can live in arm-keyed sets (a future
/// `feira arch --verdict-policy=<tier>` verb keying arm-specific
/// exit codes, a future admission-webhook rejection body enumerating
/// the accepted verdict tags).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, gen_platform::IsVariant,
)]
pub enum ArchVerdict {
    /// No safety violations — HCL emission is safe.
    Proven,
    /// Safety violations found — HCL emission must be refused.
    Rejected,
}

impl ArchVerdict {
    /// Exhaustive iteration surface for every consumer that walks the
    /// closed two-arm [`ArchVerdict`] discriminator set (the byte-parity
    /// witness against the paired [`gen_platform::IsVariant`]-derived
    /// [`Self::is_proven`] / [`Self::is_rejected`] predicate family, a
    /// future `feira arch --list-verdicts` CLI enumeration of the
    /// accepted outcome tags, a future admission-webhook rejection
    /// body naming the accepted verdict set).
    ///
    /// A future variant addition (a `PartiallyProven` tier the
    /// `iac-forge` policy-engine grows for compliance-only violation
    /// sets, an `Unknown` tier for the M4 admission-webhook's
    /// timeout-during-check outcome) extends this slice as a single
    /// edit and every consumer picks up the new entry by construction;
    /// the compiler-checked exhaustiveness on the paired
    /// [`gen_platform::IsVariant`]-derived per-arm predicates
    /// ([`Self::is_proven`] / [`Self::is_rejected`]) covers the
    /// projection axis so both halves of the closed-set discipline
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
    /// [`crate::invariants::InvariantKind::ALL`] (5226ad5) /
    /// [`caixa_lint::FixSafety::ALL`] (732a791) /
    /// [`caixa_core::render::PathShapeViolation::ALL`] (efc0326)
    /// exhaustive-iteration surfaces — the eleventh closed-set typed
    /// enum on the caixa surface (and the second inside `caixa-arch`,
    /// after `InvariantKind`) to converge onto the same
    /// one-canonical-arm-list-per-enum discipline. Order matches
    /// variant declaration order verbatim (`Proven` → `Rejected`) so
    /// the slice is the canonical verdict ordering every listing /
    /// rendering consumer defers to.
    pub const ALL: &'static [Self] = &[Self::Proven, Self::Rejected];

    /// Substrate-canonical per-[`ArchVerdict`] lowercase-tag scalar
    /// accessor every consumer that renders the arch two-arm proof-
    /// outcome axis as user-facing text keys off — returns the per-arm
    /// byte-string (`"proven"` / `"rejected"`) as a `&'static str`, the
    /// same lowercase tags a future `feira arch` summary line, a future
    /// admission-webhook rejection body naming the accepted-verdict
    /// set, or a `tracing::field::Value::Str`-arm structured-log
    /// recorder on the `caixa-arch` per-manifest emission path would
    /// have otherwise reached via a `format!("{:?}", verdict).to_lowercase()`
    /// round-trip through the [`std::fmt::Debug`] derive — with two
    /// silent drift footguns the substrate-canonical accessor closes at
    /// build time:
    ///
    ///   - the `Debug` derive's per-arm output is *not* a stability
    ///     guarantee (Rust's own convention gives it as *no guarantee
    ///     at all*), so a `#[derive(Debug)]` swap for a hand-rolled
    ///     `impl Debug` that pretty-prints the arm with per-arm context
    ///     (`"Proven(clean)"`, `"Rejected(safety violations)"`) would
    ///     silently reroute every diagnostic tag through a stale byte-
    ///     string with no downstream signal until an operator scrolled
    ///     the `feira arch` / `feira tofu` terminal output;
    ///   - `format!("{:?}", verdict).to_lowercase()` allocates a fresh
    ///     `String` per verdict on every render pass — a per-arm
    ///     `&'static str` return eliminates the allocation at every
    ///     substrate-side per-[`ArchVerdict`] render consumer.
    ///
    /// Peer of the sibling substrate-wide closed-set fieldless typed-
    /// enum canonical-lowercase-tag scalar accessors
    /// [`crate::invariants::InvariantKind::as_str`] (87c875a — the
    /// paired severity-classification axis on the sibling `caixa-arch`
    /// invariant-kind closed-set enum), [`caixa_lint::Severity::as_str`]
    /// (per the caixa-lint four-arm severity axis returning
    /// `"error"` / `"warning"` / `"info"` / `"hint"`),
    /// [`caixa_core::CaixaKind::as_str`], and the sibling M2/M3
    /// `caixa-core` closed-set typed-enum `as_str` family
    /// ([`caixa_core::supervisor::RestartStrategy::as_str`] /
    /// [`caixa_core::supervisor::RestartPolicy::as_str`] /
    /// [`caixa_core::aplicacao::PlacementStrategy::as_str`]) —
    /// extends the substrate-wide "one canonical lowercase-tag
    /// accessor per closed-set fieldless typed enum" discipline onto
    /// the caixa-arch verdict-outcome axis, closing the last remaining
    /// closed-set fieldless typed enum on the caixa surface without
    /// this accessor.
    ///
    /// `pub const fn` — matches the sibling
    /// [`gen_platform::IsVariant`]-derive-generated per-arm `is_*`
    /// predicates' `const fn` posture, so every future substrate-side
    /// `const`-context consumer (a `const _: () = assert!(…)` module-
    /// scope pin on a per-fixture typed [`ArchVerdict`], a future M4
    /// admission-webhook `const fn` per-verdict rejection-body
    /// composer, a compile-time `HashMap<&'static str, _>`-shaped
    /// per-verdict policy table) reaches the paired byte-string
    /// through one substrate-primitive dispatch at compile time as at
    /// runtime.
    ///
    /// A future variant addition (a `PartiallyProven` tier the
    /// `iac-forge` policy-engine grows for compliance-only violation
    /// sets, an `Unknown` tier for the M4 admission-webhook's
    /// timeout-during-check outcome) reaches the paired
    /// [`std::fmt::Display`] impl + [`AsRef<str>`] impl + every
    /// downstream `.as_str()` consumer through one match-arm edit
    /// here, not a coordinated rewrite of every open-coded
    /// `format!("{:?}", …)` re-inlining. Named `as_str` (not `label`
    /// / `tag`) to match the sibling closed-set-enum `as_str`
    /// convention the substrate already carries verbatim across every
    /// peer typed enum.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proven => "proven",
            Self::Rejected => "rejected",
        }
    }

    /// Reverse projection on the [`ArchVerdict`] closed-set enum's
    /// canonical-tag axis — parses a `"proven"` / `"rejected"` wire
    /// byte-string back to the typed enum, or returns `None` when `s`
    /// lies outside the two-arm accept-set [`Self::as_str`] emits. The
    /// single `&str → Self` projection every future re-entry point on
    /// the verdict-outcome axis dispatches through (a future
    /// `feira arch --verdict <proven|rejected>` CLI arg-parse that
    /// binds the wire byte-string into the typed enum before
    /// dispatching to a per-arm filter, a future M4
    /// `mesh.pleme.io/v1alpha1/ArchAudit` CR materializer's admission-
    /// time re-parse of the per-manifest verdict axis, an `iac-forge`
    /// audit-report re-loader that binds a prior [`Self::as_str`]
    /// output back to the typed enum for cross-run outcome-histogram
    /// diff) would have had to re-inline a two-arm `match s` cascade
    /// that expressed no compile-time link back to the substrate
    /// primitive.
    ///
    /// Same closed-set-reverse-projection discipline the sibling
    /// [`caixa_core::CaixaKind::from_wire`] (2aa6d23) /
    /// [`caixa_core::CaixaDialeto::from_wire`] (d0e65ea) /
    /// [`caixa_core::supervisor::RestartStrategy::from_wire`] (4eec29c) /
    /// [`caixa_core::supervisor::RestartPolicy::from_wire`] (dd32ccf) /
    /// [`caixa_core::aplicacao::PlacementStrategy::from_wire`] (18c7342) /
    /// [`caixa_core::dep::DepList::from_wire`] (45ee563) /
    /// [`crate::invariants::InvariantKind::from_wire`] (b9e4e61) /
    /// [`caixa_core::render::PathShapeViolation::from_wire`] (aebd9c6)
    /// typed enums carry on the peer wire-side `str → Self` axes —
    /// extends the substrate-wide `(as_str, from_wire)` round-trip
    /// family onto the second closed-set fieldless typed enum on the
    /// caixa-arch surface (the verdict-outcome axis, after the peer
    /// severity-classification axis on [`InvariantKind`]), matching the
    /// same two-way `str ↔ Self` round-trip every sibling closed-set
    /// enum already carries. Method-named `from_wire` (not `from_str`)
    /// to match the peer shapes verbatim and side-step a
    /// `clippy::should_implement_trait` lint that a plain `from_str`
    /// name would otherwise trigger without paired
    /// [`std::str::FromStr`] impl scaffolding this axis does not carry
    /// today. Returns `Option<Self>` (rather than `Result<Self, _>`)
    /// to match the peer shapes: the caller picks the diagnostic form
    /// appropriate for its use site (a `feira arch --verdict` CLI
    /// arg-parse renders its own per-verb error message; an admission-
    /// webhook rejection body wraps the `None` outcome with the
    /// accepted-set enumeration `ArchVerdict::ALL.iter().map(…)` for
    /// operator diagnostics).
    ///
    /// Pinned load-bearing at the substrate-primitive level by
    /// [`tests::arch_verdict_from_wire_accepts_every_as_str_output`]
    /// (round-trip witness against the peer [`Self::as_str`] axis) and
    /// [`tests::arch_verdict_from_wire_rejects_unknown_byte_strings`]
    /// (rejection witness against silent accept-set widening).
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "proven" => Some(Self::Proven),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }
}

/// Route the derived-style [`std::fmt::Display`] impl on
/// [`ArchVerdict`] through the substrate-canonical
/// [`ArchVerdict::as_str`] `pub const fn` accessor so every consumer
/// that binds an [`ArchVerdict`] through the standard-library `{}`
/// formatting axis (a future `feira arch` per-manifest summary line
/// naming the outcome, a `tracing::field::Value::from(verdict)`
/// structured-log recorder on the operator's per-check emission
/// path, any `format!("{verdict}")` interpolation in a future audit
/// surface) reaches the canonical byte-string through one substrate-
/// primitive dispatch rather than an open-coded per-arm match at
/// every wire-up.
///
/// Follows the same closed-set-typed-enum `Display`-through-`as_str`
/// convention the substrate-wide siblings
/// [`crate::invariants::InvariantKind`] (87c875a),
/// [`caixa_lint::Severity`] (6ad94f3), [`caixa_core::CaixaKind`],
/// [`caixa_core::aplicacao::PlacementStrategy`],
/// [`caixa_core::supervisor::RestartStrategy`],
/// [`caixa_core::supervisor::RestartPolicy`], and
/// [`caixa_core::dep::DepList`] already carry — closes the
/// [`ArchVerdict`] closed-set enum's
/// `(as_str, Display, AsRef<str>)` canonical-projection triple.
impl std::fmt::Display for ArchVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Route the standard-library [`AsRef<str>`] projection on
/// [`ArchVerdict`] through the substrate-canonical
/// [`ArchVerdict::as_str`] `pub const fn` accessor so every consumer
/// that binds an [`ArchVerdict`] through the trait-idiomatic
/// `.as_ref()` (a future `HashMap::get::<str>(v.as_ref())` per-verdict
/// policy-table lookup, a `Command::arg` shell-out composing the
/// canonical outcome tag into a `feira arch --verdict=<tag>` filter,
/// any `impl AsRef<str>`-bound generic function) reaches the
/// canonical byte-string through one substrate-primitive dispatch
/// rather than an open-coded `.as_str()` re-inlining at every wire-up.
///
/// Peer of the substrate-wide sibling closed-set-enum
/// `AsRef<str>`-through-`as_str` family already carried by
/// [`crate::invariants::InvariantKind`] (87c875a),
/// [`caixa_lint::Severity`] (ce9d1e3), [`caixa_core::CaixaKind`],
/// [`caixa_core::CaixaDialeto`],
/// [`caixa_core::aplicacao::PlacementStrategy`],
/// [`caixa_core::aplicacao::RateLimitUnit`],
/// [`caixa_core::supervisor::RestartStrategy`],
/// [`caixa_core::supervisor::RestartPolicy`],
/// [`caixa_core::dep::DepList`], and [`caixa_core::CaixaVersion`] —
/// extends the axis onto the caixa-arch verdict-outcome closed-set
/// enum, closing the substrate-wide
/// `(as_str, Display, AsRef<str>)` canonical-projection triple on
/// the last remaining closed-set fieldless typed enum on the caixa
/// surface without it.
impl AsRef<str> for ArchVerdict {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Trait-idiomatic reverse projection on the [`ArchVerdict`] closed-
/// set enum: routes byte-for-byte through the paired
/// [`ArchVerdict::from_wire`] substrate-primitive `Option<Self>`
/// accessor, so every future substrate-side consumer that binds a
/// canonical verdict-outcome tag through the standard-library
/// `.try_into()` / [`TryFrom`] axis (a `feira arch --verdict
/// <proven|rejected>` CLI arg-parse that folds the operator's `String`
/// through `ArchVerdict::try_from(&s)?`, a future M4
/// `mesh.pleme.io/v1alpha1/ArchAudit` CR materializer's admission-
/// time re-parse of the per-manifest verdict axis, an `iac-forge`
/// audit-report re-loader binding a prior [`ArchVerdict::as_str`]
/// output back to the typed enum through the trait-bound axis, a
/// generic `<T: TryFrom<&str>>`-bound audit-report re-loader over any
/// of the substrate's closed-set typed enums) reaches the same two-arm
/// accept-set the sibling [`ArchVerdict::from_wire`] resolver
/// dispatches through — without an open-coded per-arm cascade with no
/// compile-time link back to the typed enum.
///
/// `type Error = ()` matches the peer sibling
/// [`crate::invariants::InvariantKind`] (e21a857),
/// [`caixa_core::CaixaKind`] (3c83606),
/// [`caixa_core::CaixaDialeto`] (bf33136),
/// [`caixa_core::aplicacao::PlacementStrategy`] (6fd00cd),
/// [`caixa_core::supervisor::RestartStrategy`] (5b828ed),
/// [`caixa_core::supervisor::RestartPolicy`] (6fdd0d9),
/// [`caixa_core::aplicacao::WitShape`] (5472902),
/// [`caixa_core::aplicacao::RateLimitUnit`] (bf78400), and
/// [`caixa_core::render::PathShapeViolation`] (e67e48a)
/// [`TryFrom<&str>`] impls: the axis-error carries no payload because
/// the paired [`ArchVerdict::from_wire`] accessor already returns
/// `None` on rejection, and the caller picks the diagnostic form
/// appropriate for its use site (a `feira arch --verdict` arg-parse
/// wraps the `Err(())` outcome with an "unknown verdict axis: <arg>
/// — accepted: {…}" message enumerating [`ArchVerdict::ALL`], a
/// future M4 admission-webhook rejection body wraps the same
/// `Err(())` outcome for operator diagnostics, a `Result::map_err`
/// at the call site lifts the axis-error to a per-verb error type).
/// Same shape the peer sibling reverse-projection axes carry.
///
/// A future arm addition (a `PartiallyProven` tier the `iac-forge`
/// policy-engine grows for compliance-only violation sets, an
/// `Unknown` tier for the M4 admission-webhook's timeout-during-check
/// outcome — both trajectory items the sibling [`ArchVerdict::ALL`]
/// doc block already names) grows the trait-idiomatic axis by
/// construction through one caixa-arch edit on
/// [`ArchVerdict::from_wire`], not a coordinated rewrite across every
/// future `TryFrom<&str>`-bound consumer's arm-set.
///
/// Extends the substrate-wide closed-set-enum trait-idiomatic
/// reverse-projection family onto the second closed-set fieldless
/// typed enum on the caixa-arch surface — the verdict-outcome axis,
/// after the peer severity-classification axis on
/// [`crate::invariants::InvariantKind`]. Method-named `from_wire` (not
/// `from_str`) is preserved on the paired accessor to side-step the
/// `clippy::should_implement_trait` lint a plain `from_str` name would
/// otherwise trigger without paired [`std::str::FromStr`] scaffolding
/// this axis does not carry today — same design tradeoff every prior
/// sibling reverse-projection lift already carries.
///
/// Pinned load-bearing by
/// [`tests::arch_verdict_try_from_str_routes_through_from_wire_accessor`]
/// (byte-parity pin against [`ArchVerdict::from_wire`] across the two-
/// arm accept-set),
/// [`tests::arch_verdict_try_from_str_rejects_unknown_byte_strings`]
/// (rejection witness against silent accept-set widening), and
/// [`tests::arch_verdict_try_from_str_and_from_wire_partition_the_accept_set`]
/// (cross-axis partition pin locking trait and method-named
/// projections to the same `Option<Self>` output on every input).
impl TryFrom<&str> for ArchVerdict {
    type Error = ();

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::from_wire(s).ok_or(())
    }
}

/// Standard-library trait-idiomatic forward projection on the
/// [`ArchVerdict`] closed-set caixa-arch verdict-outcome axis.
/// Routes byte-for-byte through the paired substrate-primitive
/// [`ArchVerdict::as_str`] `pub const fn` accessor so
/// `<&'static str>::from(verdict)` / `verdict.into::<&'static str>()`
/// reaches the same two-arm `"proven"` / `"rejected"` canonical-
/// lowercase emit-set the sibling method-named accessor dispatches
/// through and the sibling [`std::fmt::Display for ArchVerdict`] /
/// [`AsRef<str> for ArchVerdict`] impls also route through.
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
/// [`crate::invariants::InvariantKind`] via f2ca7bc) onto the second
/// closed-set fieldless typed enum on the caixa-arch surface — the
/// verdict-outcome two-arm accept-set every `feira arch` render site,
/// every `feira tofu` HCL-emission gate, and every future M4
/// admission-webhook / audit-report re-loader dispatches through.
/// Extends the trait-idiomatic forward-projection family onto the
/// second outside-caixa-core closed-set fieldless typed enum on the
/// caixa surface so a downstream `impl From<T> for &'static str`-bound
/// generic consumer reaches the caixa-arch verdict-outcome axis
/// through the same uniform trait dispatch every caixa-core sibling
/// and the peer caixa-arch severity-classification axis already carry.
///
/// Pairs with the sibling [`TryFrom<&str> for ArchVerdict`] impl
/// (0a4cc45) to close the two-way `Self ↔ &'static str` round-trip on
/// the trait-idiomatic axis pair, mirroring the pre-existing
/// method-named [`ArchVerdict::as_str`] + [`ArchVerdict::from_wire`]
/// pair on the substrate-primitive axis pair.
///
/// Return type is `&'static str` by construction — every
/// [`ArchVerdict::as_str`] arm resolves to an inline `"proven"` /
/// `"rejected"` `&'static str` literal, so the trait's return-type
/// promise is upheld structurally without a [`String::leak`] cast or
/// a per-arm inline literal outside the paired
/// [`ArchVerdict::as_str`] dispatch.
///
/// The paired [`ArchVerdict::as_str`] accessor's two-arm emit-set is
/// the single source of truth — every future arm addition (a
/// `PartiallyProven` tier the `iac-forge` policy-engine grows for
/// compliance-only violation sets, an `Unknown` tier for the M4
/// admission-webhook's timeout-during-check outcome — both
/// trajectory items the sibling [`ArchVerdict::ALL`] doc block
/// already names) grows the trait-idiomatic forward axis by
/// construction: one caixa-arch edit on [`ArchVerdict::as_str`]
/// extends every one of the sibling forward-projection paths
/// ([`std::fmt::Display`], [`AsRef<str>`], [`ArchVerdict::as_str`]
/// itself, and this [`From<Self> for &'static str`]) without a
/// coordinated rewrite across every future `Into<&'static str>`-bound
/// consumer's arm-set.
///
/// Pinned load-bearing by
/// [`tests::arch_verdict_from_into_static_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`ArchVerdict::as_str`] across the two-
/// arm emit-set, plus a `const`-context materialization witness for
/// the `&'static str` lifetime promise routed through the paired
/// [`ArchVerdict::as_str`] `pub const fn` accessor, plus a paired
/// `.into()` shape assertion covering the blanket-derived
/// `Into<&'static str>` shape) and
/// [`tests::arch_verdict_from_into_static_str_and_as_str_partition_the_emit_set`]
/// (partition pin asserting `<&'static str as
/// From<ArchVerdict>>::from` and [`ArchVerdict::as_str`] agree on
/// every arm, plus a two-way direct round-trip witness through the
/// paired trait-idiomatic [`TryFrom<&str>`] axis that closes the
/// two-way `Self ↔ &'static str` round-trip on the trait-idiomatic
/// axis pair — the emit-side [`ArchVerdict::as_str`] and the
/// parse-side [`ArchVerdict::from_wire`] dispatch on the same two
/// inline canonical-lowercase byte-strings by construction, so
/// round-tripping composes the two trait impls directly).
impl From<ArchVerdict> for &'static str {
    fn from(verdict: ArchVerdict) -> &'static str {
        verdict.as_str()
    }
}

/// Trait-idiomatic *borrowed-input* forward projection on
/// [`ArchVerdict`] onto the `&'static str` axis — the borrowed-input
/// companion to the paired owned-input [`From<ArchVerdict> for
/// &'static str`] impl immediately above. Routes byte-for-byte through
/// the same substrate-primitive [`ArchVerdict::as_str`] `pub const fn`
/// accessor so every consumer that binds a `&ArchVerdict` through the
/// standard-library `.into()` / [`From<&Self> for &'static str`] axis (a
/// `ArchVerdict::ALL.iter().map(<&'static str>::from).collect::<Vec<_>>()`
/// per-arm accept-set materializer — whose iterator over
/// `&'static [ArchVerdict]` yields `&ArchVerdict`, not `ArchVerdict`,
/// so the owned-input [`From<ArchVerdict>`] axis alone forces every
/// call site through an explicit `.copied()` / dereference /
/// [`Copy`]-bound restatement rather than the direct trait-idiomatic
/// projection; a future `feira arch --list-verdicts` CLI enumeration
/// composed via `ArchVerdict::ALL.iter().map(Into::into)`; a future M4
/// admission-webhook rejection body whose accepted-set enumeration
/// walks the same iterator shape; a future
/// `HashMap::<&'static str, usize>::from_iter(reports.iter().map(|r|
/// (<&'static str>::from(&r.verdict), 0)))` per-verdict histogram seed
/// on the operator's audit path — whose borrowed access off
/// `&ArchReport.verdict` avoids a `.copied()` / [`Copy`]-bound
/// dereference on the arch-verdict field) reaches the same two-arm
/// `"proven"` / `"rejected"` canonical-lowercase emit-set the paired
/// owned-input [`From<ArchVerdict> for &'static str`], the sibling
/// [`std::fmt::Display`], [`AsRef<str>`], and [`ArchVerdict::as_str`]
/// surfaces already return.
///
/// Second outside-`caixa-core` peer (and first on the caixa-arch
/// verdict-outcome axis) on the substrate-wide trait-idiomatic
/// *borrowed-input* `&'static str`-returning forward-projection family
/// already carried by [`caixa_core::dep::DepList`] (64aa742, first-
/// mover), [`caixa_core::CaixaKind`], [`caixa_core::CaixaDialeto`],
/// [`caixa_core::supervisor::RestartStrategy`],
/// [`caixa_core::supervisor::RestartPolicy`],
/// [`caixa_core::aplicacao::PlacementStrategy`],
/// [`caixa_core::aplicacao::WitShape`],
/// [`caixa_core::aplicacao::RateLimitUnit`],
/// [`caixa_core::render::PathShapeViolation`] (cdf4e95, first render-
/// side arm), and [`crate::invariants::InvariantKind`] (238d886, first
/// outside-`caixa-core` arm — the paired severity-classification axis
/// on the sibling `caixa-arch` invariant-kind closed-set enum). Rust's
/// `From` trait does not auto-derive the `From<&Self>` sibling from a
/// `From<Self>` impl (the blanket `impl<T, U> From<&T> for U where
/// T: Copy, U: From<T>` does not exist in `core`), so every closed-set
/// typed enum that carries the owned-input axis but not the borrowed-
/// input axis forces every borrowed-input call site through a
/// `.copied()` / `<&'static str>::from(*verdict)` / `verdict.as_str()`
/// detour whose type bounds have no compile-time link to the substrate
/// primitive. Lifting the borrowed-input axis on the caixa-arch
/// verdict-outcome closed-set fieldless typed enum closes that gap on
/// the same trajectory the paired owned-input axis
/// ([`impl From<ArchVerdict> for &'static str`] immediately above)
/// already opened.
///
/// Pinned load-bearing by
/// [`tests::arch_verdict_from_borrowed_into_static_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`ArchVerdict::as_str`] across the two-arm
/// emit-set via a borrowed input, plus a `const`-context materialization
/// witness for the `&'static str` lifetime promise) and
/// [`tests::arch_verdict_from_owned_and_borrowed_into_static_str_agree_on_every_arm`]
/// (cross-axis partition pin against the paired owned-input
/// [`From<ArchVerdict> for &'static str`] impl, plus a
/// `.iter().map(Into::into)` pipe witness over [`ArchVerdict::ALL`]
/// whose iterator yields `&ArchVerdict` by construction so this
/// borrowed-input axis is what routes the pipe through the substrate-
/// primitive accessor without a spurious `Copy` deref).
impl From<&ArchVerdict> for &'static str {
    fn from(verdict: &ArchVerdict) -> &'static str {
        verdict.as_str()
    }
}

/// Trait-idiomatic *owned-input, owned-`String` output* forward
/// projection on [`ArchVerdict`] onto the owned-`String` axis — the
/// owned-`String` companion to the paired [`From<ArchVerdict> for
/// &'static str`] and [`From<&ArchVerdict> for &'static str`]
/// siblings immediately above. Routes byte-for-byte through the
/// substrate-primitive [`ArchVerdict::as_str`] `pub const fn` accessor
/// via [`str::to_owned`] so every consumer that binds an
/// [`ArchVerdict`] through the standard-library `.into()` /
/// [`From<Self> for String`] axis (a `let key: String =
/// verdict.into();`-shaped downstream call site; a future
/// `serde_json::Value::String(verdict.into())` structured-payload
/// composer where the `Value::String` arm typing demands an owned
/// [`String`] and the sibling `&'static str`-returning axes force an
/// explicit `.to_owned()` / [`String::from`] restatement at every call
/// site; a future `HashMap::<String, ArchVerdict>::from_iter` per-
/// verdict lookup on the operator's audit path where the map's key
/// type is owned [`String`] rather than `&'static str`; a future
/// [`std::borrow::Cow::<'static, str>::Owned(verdict.into())`]
/// composer on a future M4 admission-webhook rejection body's owned-
/// arm; a future caixa-arch pipeline's per-verdict structured-log
/// emit where the JSON serializer's [`Serialize`] impl on [`String`]
/// owns the emit-path) reaches the same two-arm `"proven"` /
/// `"rejected"` canonical-lowercase emit-set the paired
/// `&'static str`-returning axes, the sibling [`std::fmt::Display`],
/// [`AsRef<str>`], and [`ArchVerdict::as_str`] surfaces already return
/// — no `.to_owned()` / `String::from(verdict.as_str())` detour whose
/// type bounds have no compile-time link to the substrate primitive.
///
/// Rust's standard library does not carry a blanket
/// `impl<T: AsRef<str>> From<T> for String` (nor an
/// `impl<T: fmt::Display> From<T> for String`), so every closed-set
/// typed enum that carries the paired [`AsRef<str>`] /
/// [`std::fmt::Display`] / [`From<Self> for &'static str`] /
/// [`From<&Self> for &'static str`] quadruple but not the owned-
/// `String` axis forces every owned-string call site through the
/// detour above. This lift closes that axis on the second outside-
/// `caixa-core` closed-set fieldless typed enum on the caixa surface
/// (the caixa-arch verdict-outcome two-arm axis), matching the
/// trajectory each of the ten prior peer enums —
/// [`caixa_core::supervisor::RestartStrategy`] (7baa18a, first-mover
/// on this axis), [`caixa_core::supervisor::RestartPolicy`] (7851725),
/// [`caixa_core::CaixaKind`] (231a18c),
/// [`caixa_core::CaixaDialeto`] (88942cd),
/// [`caixa_core::dep::DepList`] (32b0ee8),
/// [`caixa_core::aplicacao::PlacementStrategy`] (1154c2f),
/// [`caixa_core::aplicacao::WitShape`] (79a8723),
/// [`caixa_core::aplicacao::RateLimitUnit`] (c7d687d),
/// [`caixa_core::render::PathShapeViolation`] (6e0479a, first render-
/// side arm), and [`crate::invariants::InvariantKind`] (1afd8d5,
/// first outside-`caixa-core` arm — the paired severity-classification
/// axis on the sibling `caixa-arch` invariant-kind closed-set enum) —
/// followed on the same 2×2-completion campaign.
///
/// Pinned load-bearing by
/// [`tests::arch_verdict_from_into_owned_string_routes_through_as_str_accessor`]
/// (byte-parity pin against [`ArchVerdict::as_str`] across the two-
/// arm emit-set via the owned-`String` surface) and
/// [`tests::arch_verdict_from_into_owned_string_and_static_str_agree_on_every_arm`]
/// (cross-axis partition pin against the paired owned-input
/// `&'static str`-returning [`From<ArchVerdict> for &'static str`]
/// impl and the [`ToString::to_string`]-through-[`std::fmt::Display`]
/// surface, plus a `.iter().copied().map(String::from)` pipe witness
/// over [`ArchVerdict::ALL`], plus a direct `Self → String → Self`
/// round-trip witness through the paired [`TryFrom<&str>`] axis on
/// the owned-[`String`]'s [`String::as_str`] borrow).
impl From<ArchVerdict> for String {
    fn from(verdict: ArchVerdict) -> String {
        verdict.as_str().to_owned()
    }
}

/// Trait-idiomatic *borrowed-input, owned-`String` output* forward
/// projection on [`ArchVerdict`] — the borrowed-input companion to the
/// paired owned-input [`From<ArchVerdict> for String`] (cc80a53), the
/// paired borrowed-input [`From<&ArchVerdict> for &'static str`]
/// (73bda50), and the paired owned-input
/// [`From<ArchVerdict> for &'static str`] siblings above. Routes byte-
/// for-byte through the substrate-primitive [`ArchVerdict::as_str`]
/// `pub const fn` accessor via [`str::to_owned`] so every consumer
/// that binds an [`ArchVerdict`] through the standard-library `.into()`
/// / [`From<&Self> for String`] axis reaches the same two-arm
/// `"proven"` / `"rejected"` canonical-lowercase emit-set the paired
/// [`std::fmt::Display`], [`AsRef<str>`], [`ArchVerdict::as_str`], and
/// the three other trait-idiomatic forward-projection impls already
/// return — no `verdict.as_str().to_owned()` / `String::from(*verdict)`
/// (with a spurious [`Copy`]) / `verdict.to_string()` (through
/// [`std::fmt::Display`]) detour whose type bounds have no compile-
/// time link to the substrate primitive.
///
/// Fills the *last* remaining corner of the substrate-wide
/// `{Self, &Self} × {&'static str, String}` 2×2 trait-idiomatic
/// projection family on the caixa-arch verdict-outcome two-arm
/// closed-set fieldless typed enum. Rust's standard library carries
/// no blanket `impl<T: AsRef<str>> From<&T> for String` (nor an
/// `impl<T: fmt::Display> From<&T> for String`), so every borrowed-
/// input owned-string call site — a future
/// `serde_json::Value::String(String::from(&report.verdict))`
/// structured-payload composer over a borrowed [`ArchReport::verdict`]
/// field where the [`serde_json::Value::String`] arm typing demands an
/// owned [`String`] and the sibling `&'static str`-returning axes
/// force an explicit `.to_owned()` / [`String::from`] restatement, a
/// future `.iter().map(|r| String::from(&r.verdict)).collect()` per-
/// verdict fan-out over `&[ArchReport]` in an M4 admission-webhook
/// rejection-body composer whose borrowed access off `&ArchReport.verdict`
/// avoids a spurious `.copied()` / [`Copy`]-bound dereference on the
/// arch-verdict field, a future
/// `HashMap::<String, usize>::from_iter(reports.iter().map(|r| (String::from(&r.verdict), 0)))`
/// per-verdict histogram seed on the operator's audit path whose
/// borrowed-iteration axis over `&ArchReport.verdict` avoids a
/// spurious [`Copy`] on the arch-verdict field, a future
/// `ArchVerdict::ALL.iter().map(String::from).collect::<Vec<_>>()`
/// per-arm accept-set materializer on a future `feira arch
/// --list-verdicts` CLI enumeration — otherwise resolves through the
/// detour above.
///
/// Twelfth peer on the substrate-wide trait-idiomatic *borrowed-
/// input, owned-`String` output* forward-projection family opened on
/// [`caixa_core::supervisor::RestartStrategy`] (579385f), closed on
/// the M2 OTP-shape sibling axis pair by
/// [`caixa_core::supervisor::RestartPolicy`] (8465740), extended onto
/// the two-list dep-graph peer by [`caixa_core::dep::DepList`]
/// (e0cb617), onto the top-level [`caixa_core::CaixaKind`] peer by
/// (e76436d), the dialect-classification peer
/// [`caixa_core::CaixaDialeto`] (d3c0d1d), the M3 mesh-primitive
/// [`caixa_core::aplicacao::PlacementStrategy`] (d3dc000),
/// [`caixa_core::aplicacao::WitShape`] (d638fd3),
/// [`caixa_core::aplicacao::RateLimitUnit`] (6424e45 — closing the
/// whole M3 triple's 2×2 corner),
/// [`caixa_core::render::PathShapeViolation`] (b90e193 — first
/// outside-manifest-surface arm on this axis), and
/// [`crate::invariants::InvariantKind`] (3c3f66f — first outside-
/// `caixa-core` arm on this axis, the paired severity-classification
/// axis on the sibling `caixa-arch` closed-set enum). *Second
/// outside-`caixa-core` peer* on this axis, and the corner that
/// closes the whole 2×2 trait-idiomatic projection family on this
/// enum — the caixa-arch verdict-outcome two-arm axis every
/// `feira arch` / `feira tofu` per-manifest emission path dispatches
/// through — on the same trajectory the paired owned-input owned-
/// [`String`] axis lift (cc80a53), the paired borrowed-input owned-
/// [`&'static str`] axis lift (73bda50), and the paired owned-input
/// owned-[`&'static str`] axis lift already took onto the same enum.
///
/// Same three-path convergence discipline as the paired owned-input
/// impl (this borrowed-input axis, the paired owned-input
/// [`From<ArchVerdict> for String`], and [`ArchVerdict::as_str`] all
/// route through the same two-arm inline canonical-lowercase byte-
/// strings), so a future variant addition (a `Skipped` third arm the
/// operator's audit path grows for manifests the arch-gate refuses to
/// evaluate) reaches every one of the paired forward-projection paths
/// through exactly one caixa-arch edit on the [`ArchVerdict::as_str`]
/// `pub const fn` accessor.
///
/// The [`ArchVerdict::as_str`] emit and [`ArchVerdict::from_wire`]
/// parse share the same two inline canonical-lowercase byte-strings
/// by construction — so the borrowed-input owned-[`String`] forward
/// axis and the reverse [`TryFrom<&str>`] axis compose directly (via
/// the owned-[`String`]'s [`String::as_str`] borrow) without the
/// intermediate wire-vocab hop the peer [`caixa_core::CaixaKind`]
/// axis pair requires. The round-trip witness pin below locks this
/// direct composition on the caixa-arch verdict-outcome enum's
/// borrowed-input owned-[`String`] axis pair.
///
/// Pinned load-bearing by
/// [`tests::arch_verdict_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
/// (byte-parity pin against [`ArchVerdict::as_str`] across the two-
/// arm emit-set through the borrowed-input surface) and
/// [`tests::arch_verdict_from_into_borrowed_owned_string_agrees_with_paired_axes_on_every_arm`]
/// (cross-axis partition pin against every one of the four 2×2
/// corners — the paired owned-input owned-[`String`]
/// [`From<ArchVerdict> for String`] impl (cc80a53), the paired
/// borrowed-input owned-[`&'static str`]
/// [`From<&ArchVerdict> for &'static str`] impl (73bda50), and the
/// paired owned-input owned-[`&'static str`]
/// [`From<ArchVerdict> for &'static str`] impl — plus a
/// [`ToString::to_string`]-through-[`std::fmt::Display`] byte-parity
/// witness, plus a `.iter().map(String::from)` pipe witness over
/// [`ArchVerdict::ALL`] (whose iterator yields `&ArchVerdict` by
/// construction, so the borrowed-input owned-[`String`] axis is what
/// routes the pipe through the substrate-primitive
/// [`ArchVerdict::as_str`] accessor without a spurious [`Copy`]
/// deref), plus a direct round-trip witness through
/// [`TryFrom<&str>`] on the owned-[`String`]'s [`String::as_str`]
/// borrow that closes the two-way `&Self → String → Self` round-
/// trip on the trait-idiomatic borrowed-input owned-[`String`]
/// forward + reverse axis pair).
impl From<&ArchVerdict> for String {
    fn from(verdict: &ArchVerdict) -> String {
        verdict.as_str().to_owned()
    }
}

/// Trait-idiomatic *owned-input, [`std::borrow::Cow<'static, str>`]
/// output* forward projection on the caixa-arch verdict-outcome two-arm
/// closed-set [`ArchVerdict`] typed enum — routes byte-for-byte through
/// the substrate-primitive [`ArchVerdict::as_str`] `pub const fn`
/// accessor (via [`std::borrow::Cow::Borrowed`]) so every consumer that
/// binds an [`ArchVerdict`] through the standard-library `.into()` /
/// [`From<Self> for std::borrow::Cow<'static, str>`] (equivalently
/// [`Into<std::borrow::Cow<'static, str>>`]) axis — a future
/// `axum::response::IntoResponse` per-verdict rejection-body composer
/// whose typing folds a per-arm verdict tag into a
/// [`std::borrow::Cow<'static, str>`] boundary, a future M4
/// `mesh.pleme.io/v1alpha1/ArchAudit` CR admission-webhook rejection-
/// reason emitter whose typing rules out the sibling [`AsRef<str>`]
/// borrowed return and the sibling [`From<Self> for &'static str`]
/// axis's non-[`std::borrow::Cow`]-parameterized shape, a future
/// substrate-wide per-verdict diagnostic surface that folds either the
/// zero-alloc [`std::borrow::Cow::Borrowed`] arm (for the closed-set
/// arms whose byte-string is inline-lifted) or the
/// [`std::borrow::Cow::Owned`] arm (for a caller that mutates the
/// projection) through one uniform trait dispatch, a future generic
/// `<T: Into<std::borrow::Cow<'static, str>>>`-bound emitter on a per-
/// verdict structured-log or admission-webhook rejection body — reaches
/// the same two-arm `"proven"` / `"rejected"` canonical-lowercase byte-
/// strings the paired [`std::fmt::Display`], [`AsRef<str>`],
/// [`ArchVerdict::as_str`], and the four `{Self, &Self} × {&'static
/// str, String}` 2×2 trait-idiomatic forward-projection corners
/// ([`From<ArchVerdict> for &'static str`],
/// [`From<&ArchVerdict> for &'static str`],
/// [`From<ArchVerdict> for String`],
/// [`From<&ArchVerdict> for String`]) already return, rather than an
/// open-coded per-call-site
/// `std::borrow::Cow::Borrowed(verdict.as_str())` /
/// `std::borrow::Cow::Owned(verdict.to_string())` /
/// `String::from(verdict).into()` composition whose type bounds have no
/// compile-time link back to the substrate primitive.
///
/// Deliberately returns [`std::borrow::Cow::Borrowed`] rather than
/// [`std::borrow::Cow::Owned`] — the substrate-primitive
/// [`ArchVerdict::as_str`] accessor's return carries the `&'static str`
/// lifetime by construction (each `match` arm resolves to an inline
/// `"proven"` / `"rejected"` `&'static str` literal with static
/// lifetime), so the zero-alloc borrowed arm is the type-correct
/// projection with no runtime allocation. The paired
/// [`std::borrow::Cow::Owned`] arm stays reachable at the call site
/// through the existing [`From<ArchVerdict> for String`] axis composed
/// with [`std::borrow::Cow::from`] on the resulting owned [`String`] —
/// a caller who chose to mutate the projection lands on the owned arm
/// by their own composition, not by the substrate-primitive projection
/// silently allocating on their behalf.
///
/// Eleventh peer on the substrate-wide trait-idiomatic
/// [`std::borrow::Cow<'static, str>`] forward-projection family opened
/// on the top-level [`caixa_core::CaixaKind`] by 99c1735 (closed on the
/// `{Self, &Self}` input-shape corner by d45c409), extended onto the
/// M2 OTP-shape tier by 7dd28b3 / 9b3e4b3
/// ([`caixa_core::supervisor::RestartStrategy`]) and 0612398 / ee577fd
/// ([`caixa_core::supervisor::RestartPolicy`], closing the M2 OTP-shape
/// tier), extended onto the M3 mesh-shape tier by 8634dec / 25690ef
/// ([`caixa_core::aplicacao::WitShape`]), eee504d / afdf0f4
/// ([`caixa_core::aplicacao::PlacementStrategy`]), and 1d59925 /
/// 53346fb ([`caixa_core::aplicacao::RateLimitUnit`], closing the M3
/// mesh-shape tier), extended onto the outside-M3 caixa-core tier by
/// 6858bac / 702cdf4 ([`caixa_core::dep::DepList`]) and 8322511 /
/// ebeb9e0 ([`caixa_core::CaixaDialeto`]), extended onto the outside-
/// manifest-surface / render-side tier by 7342c32 / f80fbd6
/// ([`caixa_core::render::PathShapeViolation`], closing the caixa-core
/// arm of the campaign), and extended onto the outside-`caixa-core`
/// tier by 9361e96 / d7f3039
/// ([`crate::invariants::InvariantKind`], first outside-`caixa-core`
/// peer — the paired severity-classification axis on the sibling
/// caixa-arch invariant-kind closed-set enum) — this lift extends the
/// outside-`caixa-core` tier onto the *second* peer (the caixa-arch
/// verdict-outcome two-arm axis every `feira arch` render site, every
/// `feira tofu` HCL-emission gate, and every future M4 admission-
/// webhook / audit-report re-loader dispatches through), on the same
/// trajectory the paired [`&'static str`]-returning owned-input axis
/// ([`impl From<ArchVerdict> for &'static str`] above) and the paired
/// owned-input owned-[`String`] axis ([`impl From<ArchVerdict> for
/// String`] above) already took onto the same enum.
///
/// Rust's standard library does not carry a blanket
/// `impl<T: AsRef<str>> From<T> for std::borrow::Cow<'static, str>` (nor
/// an `impl<T: fmt::Display> From<T> for std::borrow::Cow<'static, str>`),
/// so every closed-set fieldless typed enum peer on the substrate that
/// carries the paired [`AsRef<str>`] / [`std::fmt::Display`] /
/// [`From<Self> for &'static str`] / [`From<&Self> for &'static str`] /
/// [`From<Self> for String`] / [`From<&Self> for String`] sextet but not
/// the [`std::borrow::Cow<'static, str>`] axis forces every
/// [`std::borrow::Cow<'static, str>`]-parameterized call site through a
/// `std::borrow::Cow::Borrowed(verdict.as_str())` /
/// `std::borrow::Cow::Owned(verdict.to_string())` /
/// `String::from(verdict).into()` detour whose type bounds have no
/// compile-time link to the substrate primitive.
///
/// The remaining outside-`caixa-core` closed-set fieldless typed enum
/// peers on the substrate surface (`Severity`, `FixSafety`, `Semantic`,
/// `FerriteRuntime`) are the future targets of this campaign — each
/// carries the same paired sextet that this axis extends onto.
///
/// Unlike the peer [`caixa_core::CaixaKind`] pair (whose forward emit
/// lands on the lowercase Portuguese diagnostic vocabulary while the
/// reverse parse lands on the `PascalCase` wire vocabulary, forcing the
/// round-trip through an intermediate [`caixa_core::CaixaKind::wire_name`]
/// hop), [`ArchVerdict`] is a caixa-arch verdict-outcome axis with no
/// wire/diagnostic vocabulary split — the [`ArchVerdict::as_str`] emit
/// and [`ArchVerdict::from_wire`] parse share the same two inline
/// canonical-lowercase byte-strings by construction, so the
/// [`std::borrow::Cow<'static, str>`] projection this impl exposes
/// composes directly with the paired trait-idiomatic reverse
/// [`TryFrom<&str>`] axis on the projection's
/// [`std::borrow::Cow::as_ref`] borrow — no intermediate wire-vocab hop
/// required.
///
/// The paired `{Self, &Self}` borrowed-input closer on
/// `&ArchVerdict` is the next commit on this axis, matching the
/// closure discipline every prior peer landed one commit after its
/// opener.
///
/// Pinned load-bearing by
/// [`tests::arch_verdict_from_into_static_cow_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`ArchVerdict::as_str`] across the two-arm
/// emit-set through the [`std::borrow::Cow<'static, str>`] surface,
/// plus a [`std::borrow::Cow::Borrowed`] discriminator witness that the
/// projection lands on the zero-alloc arm rather than silently
/// allocating through [`std::borrow::Cow::Owned`], plus a blanket-
/// derived [`Into<std::borrow::Cow<'static, str>>`] shape witness that
/// also lands on [`std::borrow::Cow::Borrowed`]) and
/// [`tests::arch_verdict_from_into_static_cow_str_agrees_with_paired_axes_on_every_arm`]
/// (cross-axis partition pin against the paired owned-input
/// [`From<ArchVerdict> for &'static str`] and
/// [`From<ArchVerdict> for String`] forward-projection corners plus
/// the sibling [`ToString::to_string`]-through-[`std::fmt::Display`]
/// surface, plus a `.iter().copied().map(std::borrow::Cow::from)` pipe
/// witness over [`ArchVerdict::ALL`] whose zero-alloc
/// [`std::borrow::Cow::Borrowed`] outcome is load-bearing on every arm,
/// plus a direct round-trip witness through [`TryFrom<&str>`] on the
/// projection's [`std::borrow::Cow::as_ref`] borrow that closes the
/// two-way `Self → Cow<'static, str> → Self` round-trip on the trait-
/// idiomatic [`std::borrow::Cow<'static, str>`] forward + reverse axis
/// pair without the wire-vocab intermediate hop the peer
/// [`caixa_core::CaixaKind`] axis pair requires).
impl From<ArchVerdict> for std::borrow::Cow<'static, str> {
    fn from(verdict: ArchVerdict) -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed(verdict.as_str())
    }
}

/// Trait-idiomatic *borrowed-input* forward projection on
/// [`ArchVerdict`] onto the [`std::borrow::Cow<'static, str>`] axis —
/// the borrowed-input companion to the paired owned-input
/// [`From<ArchVerdict> for std::borrow::Cow<'static, str>`] impl
/// (b492d5f) immediately above, and the corner that closes the whole
/// substrate-wide `{Self, &Self} × {&'static str, String, Cow<'static,
/// str>}` 2×3 trait-idiomatic forward-projection family on the caixa-
/// arch verdict-outcome two-arm closed-set fieldless typed enum.
/// Routes byte-for-byte through the substrate-primitive
/// [`ArchVerdict::as_str`] `pub const fn` accessor via
/// [`std::borrow::Cow::Borrowed`] so every consumer that binds a
/// `&ArchVerdict` through the standard-library `.into()` /
/// [`From<&Self> for std::borrow::Cow<'static, str>`] axis — the exact
/// `ArchVerdict::ALL.iter().map(std::borrow::Cow::from).collect()`
/// pipe shape a future substrate-wide per-verdict diagnostic surface,
/// a future M4 `mesh.pleme.io/v1alpha1/ArchAudit` CR materializer's
/// admission-webhook per-verdict rejection-reason emitter whose
/// typing rules out the sibling [`AsRef<str>`] borrowed return, or a
/// future `axum::response::IntoResponse` per-verdict rejection-body
/// composer over an iterator-yielded `&ArchVerdict` reaches — resolves
/// to the same two-arm `"proven"` / `"rejected"` canonical-lowercase
/// byte-strings the paired [`std::fmt::Display`], [`AsRef<str>`],
/// [`ArchVerdict::as_str`], the four `{Self, &Self} × {&'static str,
/// String}` 2×2 trait-idiomatic forward-projection corners, and the
/// paired owned-input [`From<ArchVerdict> for std::borrow::Cow<'static,
/// str>`] impl already return.
///
/// Deliberately returns [`std::borrow::Cow::Borrowed`] rather than
/// [`std::borrow::Cow::Owned`] — the substrate-primitive
/// [`ArchVerdict::as_str`] accessor's return carries the
/// `&'static str` lifetime by construction (each `match` arm resolves
/// to an inline `&'static str` literal), so the zero-alloc borrowed
/// arm is the type-correct projection with no runtime allocation on
/// the borrowed-input surface just as on the paired owned-input
/// surface.
///
/// Closes the `{Self, &Self}` input-shape corner on the second
/// outside-`caixa-core` peer of the substrate-wide
/// [`std::borrow::Cow<'static, str>`] forward-projection campaign,
/// opened one commit prior (b492d5f) on the paired owned-input impl.
/// Rust's standard library does not carry a blanket
/// `impl<T: AsRef<str>> From<&T> for std::borrow::Cow<'static, str>`
/// (nor an `impl<T: fmt::Display> From<&T> for
/// std::borrow::Cow<'static, str>`, nor a [`Copy`]-based
/// `impl<T: Copy, U: From<T>> From<&T> for U`), so every closed-set
/// fieldless typed enum peer on the substrate that carries the paired
/// owned-input [`std::borrow::Cow<'static, str>`] axis but not the
/// borrowed-input axis forces every borrowed-input
/// [`std::borrow::Cow<'static, str>`]-parameterized call site through
/// a spurious [`Copy`] deref (`std::borrow::Cow::from(*verdict)`) or
/// a `std::borrow::Cow::Borrowed(verdict.as_str())` open-code whose
/// type bounds have no compile-time link to the substrate primitive.
///
/// Matches the closure discipline d7f3039 landed on the sibling
/// caixa-arch [`crate::invariants::InvariantKind`] one commit after
/// (9361e96), f80fbd6 landed on the outside-manifest-surface / render-
/// side [`caixa_core::render::PathShapeViolation`] one commit after
/// (7342c32), ebeb9e0 on the outside-M3 [`caixa_core::CaixaDialeto`]
/// one commit after 8322511, 702cdf4 on the two-list dep-graph
/// [`caixa_core::dep::DepList`] one commit after 6858bac, afdf0f4 on
/// the M3-mesh-shape [`caixa_core::aplicacao::PlacementStrategy`] one
/// commit after eee504d, 25690ef on
/// [`caixa_core::aplicacao::WitShape`] one commit after 8634dec,
/// 53346fb on [`caixa_core::aplicacao::RateLimitUnit`] one commit
/// after 1d59925 (closing the whole M3 mesh-shape tier), d45c409 on
/// the top-level [`caixa_core::CaixaKind`] one commit after 99c1735,
/// and 9b3e4b3 / ee577fd on the M2 OTP-shape
/// [`caixa_core::supervisor::RestartStrategy`] /
/// [`caixa_core::supervisor::RestartPolicy`] sibling peers one commit
/// after 7dd28b3 / 0612398. The remaining outside-`caixa-core` peers
/// (`Severity`, `FixSafety`, `Semantic`, `FerriteRuntime`) are the
/// remaining future targets on the axis.
///
/// Unlike the peer [`caixa_core::CaixaKind`] pair (whose forward emit
/// lands on the lowercase Portuguese diagnostic vocabulary while the
/// reverse parse lands on the `PascalCase` wire vocabulary, forcing
/// the round-trip through an intermediate
/// [`caixa_core::CaixaKind::wire_name`] hop), [`ArchVerdict`] is a
/// caixa-arch verdict-outcome axis with no wire/diagnostic vocabulary
/// split — the [`ArchVerdict::as_str`] emit and
/// [`ArchVerdict::from_wire`] parse share the same two inline
/// canonical-lowercase byte-strings by construction, so the borrowed-
/// input [`std::borrow::Cow<'static, str>`] projection this impl
/// exposes composes directly with the paired trait-idiomatic reverse
/// [`TryFrom<&str>`] axis on the projection's
/// [`std::borrow::Cow::as_ref`] borrow — no intermediate wire-vocab
/// hop required.
///
/// Pinned load-bearing by
/// [`tests::arch_verdict_from_borrowed_into_static_cow_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`ArchVerdict::as_str`] across the two-arm
/// emit-set through the borrowed-input
/// [`std::borrow::Cow<'static, str>`] surface, plus a
/// [`std::borrow::Cow::Borrowed`] discriminator witness that the
/// borrowed-input projection lands on the zero-alloc arm rather than
/// silently allocating through [`std::borrow::Cow::Owned`], plus a
/// blanket-derived [`Into<std::borrow::Cow<'static, str>>`] shape
/// witness on the borrowed-input surface that also lands on
/// [`std::borrow::Cow::Borrowed`]) and
/// [`tests::arch_verdict_from_borrowed_into_static_cow_str_agrees_with_paired_axes_on_every_arm`]
/// (cross-axis partition pin against the paired owned-input
/// [`From<ArchVerdict> for std::borrow::Cow<'static, str>`] impl
/// (b492d5f), the paired borrowed-input
/// [`From<&ArchVerdict> for &'static str`], and
/// [`From<&ArchVerdict> for String`] impls plus
/// [`ToString::to_string`]-through-[`std::fmt::Display`], plus a
/// `.iter().map(std::borrow::Cow::from)` pipe witness over
/// [`ArchVerdict::ALL`] whose iterator yields `&ArchVerdict` by
/// construction — so the borrowed-input axis is what routes the pipe
/// without a spurious [`Copy`] deref — pinning zero-alloc
/// [`std::borrow::Cow::Borrowed`] on every element, plus a direct
/// round-trip witness through [`TryFrom<&str>`] on the projection's
/// [`std::borrow::Cow::as_ref`] borrow that closes the two-way
/// `&Self → Cow<'static, str> → Self` round-trip on the borrowed-input
/// axis).
impl From<&ArchVerdict> for std::borrow::Cow<'static, str> {
    fn from(verdict: &ArchVerdict) -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed(verdict.as_str())
    }
}

/// Trait-idiomatic *owned-input, [`Box<str>`] output* forward projection
/// on the caixa-arch verdict-outcome two-arm closed-set fieldless
/// typed enum [`ArchVerdict`]. Routes byte-for-byte through the
/// substrate-primitive [`ArchVerdict::as_str`] `pub const fn` accessor
/// via [`Box::<str>::from`] on the returned `&'static str`, so every
/// consumer that binds a `let key: Box<str> = verdict.into();`-shaped
/// call site — a per-verdict census-key materializer that stashes the
/// verdict-outcome discriminator in a [`Box<str>`]-typed heap-owned
/// scalar for cheap clone (the shared-nothing per-verdict accept-set a
/// future caixa-arch policy-fanout materializer keys off), a future M4
/// `mesh.pleme.io/v1alpha1/ArchAudit` CR reconciliation scheduler's
/// admission-webhook rejection body whose per-arm [`Box<str>`] field
/// composes from an owned [`ArchVerdict`] handle, a future `feira arch
/// --by-verdict` histogram-column emitter that stashes each arm as an
/// owned [`Box<str>`] label — reaches the same two
/// `"proven"` / `"rejected"` canonical-lowercase byte-strings the
/// sibling `{Self, &Self} × {&'static str, String, Cow<'static, str>}`
/// forward-projection corner already returns.
///
/// Rust's standard library carries `impl From<&str> for Box<str>` and
/// `impl From<String> for Box<str>` but no blanket
/// `impl<T: AsRef<str>> From<T> for Box<str>`, so this axis is a
/// distinct trait-idiomatic surface that a downstream
/// `ArchVerdict → Box<str>` `.into()` reaches through this impl and no
/// other — without a `Box::from(verdict.as_str())` open-code whose type
/// bounds have no compile-time link back to the substrate primitive.
///
/// Extends the outside-`caixa-core` tier of the substrate-wide trait-
/// idiomatic [`Box<str>`] forward-projection campaign onto the second
/// peer — the caixa-arch verdict-outcome two-arm closed-set fieldless
/// typed enum — following the first-mover
/// [`crate::invariants::InvariantKind`] pair (10613a7 owned +
/// 5901887 borrowed) that opened the tier one commit prior. Same
/// discipline as the paired
/// [`caixa_core::supervisor::RestartStrategy`] /
/// [`caixa_core::supervisor::RestartPolicy`] M2-OTP-shape and
/// [`caixa_core::aplicacao::PlacementStrategy`] /
/// [`caixa_core::aplicacao::WitShape`] /
/// [`caixa_core::aplicacao::RateLimitUnit`] M3-mesh-shape [`Box<str>`]
/// axes: forward emit (this impl, the sibling
/// `{&'static str, String, Cow<'static, str>}` forward-projection
/// corner, [`std::fmt::Display`], [`AsRef<str>`],
/// [`ArchVerdict::as_str`]) and reverse parse
/// ([`ArchVerdict::from_wire`], [`TryFrom<&str>`]) route through the
/// same two inline `"proven"` / `"rejected"` canonical-lowercase byte-
/// strings [`ArchVerdict::as_str`] returns by construction, so the
/// round-trip composes directly without the wire-vocab intermediate
/// hop the peer [`caixa_core::CaixaKind`] axis pair requires.
///
/// The sibling outside-`caixa-core` peers ([`caixa_lint::Severity`],
/// [`caixa_lint::FixSafety`], [`caixa_theme::Semantic`], and
/// [`caixa_provedor::FerriteRuntime`]) whose [`Box<str>`] axis
/// closures remain future targets of this campaign. Leaves the paired
/// borrowed-input [`From<&ArchVerdict> for Box<str>`]
/// `{Self, &Self}`-closer as the direct next target on this axis.
///
/// Pinned load-bearing by
/// [`tests::arch_verdict_from_into_box_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`ArchVerdict::as_str`] across the two-arm
/// [`ArchVerdict::ALL`] emit-set on the owned-input surface, plus a
/// blanket-derived [`Into`] shape witness).
impl From<ArchVerdict> for Box<str> {
    fn from(verdict: ArchVerdict) -> Box<str> {
        Box::<str>::from(verdict.as_str())
    }
}

/// Trait-idiomatic *borrowed-input, [`Box<str>`] output* forward
/// projection on the caixa-arch verdict-outcome two-arm closed-set
/// fieldless typed enum [`ArchVerdict`]. Routes byte-for-byte through
/// the substrate-primitive [`ArchVerdict::as_str`] `pub const fn`
/// accessor via [`Box::<str>::from`] on the returned `&'static str`, so
/// every consumer that binds a `let key: Box<str> = (&verdict).into();`-
/// shaped call site or an `ArchVerdict::ALL.iter().map(Box::<str>::from)`-
/// shaped pipe (whose iterator over `&'static [ArchVerdict]` yields
/// `&ArchVerdict` by construction) — a per-verdict census-key
/// materializer that stashes the verdict-outcome discriminator in a
/// [`Box<str>`]-typed heap-owned scalar for cheap clone off a borrowed
/// handle, a future M4 `mesh.pleme.io/v1alpha1/ArchAudit` CR reconciler's
/// admission-webhook rejection body whose per-arm [`Box<str>`] field
/// composes from a borrowed [`ArchVerdict`] handle, a future
/// `feira arch --by-verdict` histogram-column emitter that iterates
/// [`ArchVerdict::ALL`] into per-arm owned [`Box<str>`] labels — reaches
/// the same two `"proven"` / `"rejected"` canonical-lowercase byte-
/// strings the sibling `{Self, &Self} × {&'static str, String,
/// Cow<'static, str>}` forward-projection corner already returns.
///
/// Rust's standard library carries `impl From<&str> for Box<str>` and
/// `impl From<String> for Box<str>` but no blanket
/// `impl<T: AsRef<str>> From<&T> for Box<str>` (nor a `Copy`-based
/// `impl<T: Copy, U: From<T>> From<&T> for U`), so this borrowed-input
/// axis is a distinct trait-idiomatic surface that the pipe shape
/// [`ArchVerdict::ALL`]`.iter().map(Box::<str>::from)` reaches through
/// this impl and no other — without it, the same pipe would force an
/// explicit `.copied()` restatement (`.iter().copied().map(Box::<str>
/// ::from)`) whose type bounds have no compile-time link back to the
/// substrate primitive, and a `let key: Box<str> = (&verdict).into();`-
/// shaped call site would force an explicit `Copy` deref
/// (`Box::<str>::from(*verdict)`) or a `Box::<str>::from(verdict
/// .as_str())` open-code with the same defect.
///
/// Closes the `{Self, &Self}` input-shape corner on the outside-
/// `caixa-core` tier of the substrate-wide trait-idiomatic
/// [`Box<str>`] forward-projection campaign, on its second peer — the
/// caixa-arch verdict-outcome two-arm closed-set fieldless typed enum —
/// opened by the paired owned-input [`From<ArchVerdict> for Box<str>`]
/// impl (3e08f5a) one commit prior, following the first-mover
/// [`crate::invariants::InvariantKind`] pair (10613a7 owned + 5901887
/// borrowed) that opened the tier one axis prior. Same discipline as
/// the paired [`caixa_core::supervisor::RestartStrategy`] /
/// [`caixa_core::supervisor::RestartPolicy`] M2-OTP-shape and
/// [`caixa_core::aplicacao::PlacementStrategy`] /
/// [`caixa_core::aplicacao::WitShape`] M3-mesh-shape [`Box<str>`]
/// `{Self, &Self}`-closers: forward emit (this impl, the paired owned-
/// input [`From<ArchVerdict> for Box<str>`] impl, the sibling
/// `{&'static str, String, Cow<'static, str>}` forward-projection
/// corner, [`std::fmt::Display`], [`AsRef<str>`],
/// [`ArchVerdict::as_str`]) and reverse parse
/// ([`ArchVerdict::from_wire`], [`TryFrom<&str>`]) route through the
/// same two inline `"proven"` / `"rejected"` canonical-lowercase byte-
/// strings [`ArchVerdict::as_str`] returns by construction, so the
/// round-trip composes directly without the wire-vocab intermediate
/// hop the peer [`caixa_core::CaixaKind`] axis pair requires.
///
/// Pinned load-bearing by
/// [`tests::arch_verdict_from_borrowed_into_box_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`ArchVerdict::as_str`] across the two-arm
/// [`ArchVerdict::ALL`] emit-set on the borrowed-input surface, plus a
/// blanket-derived [`Into`] shape witness, plus a
/// `.iter().map(Box::<str>::from)` pipe witness over
/// [`ArchVerdict::ALL`] — whose iterator yields `&ArchVerdict` by
/// construction, so the borrowed-input [`Box<str>`] axis is what
/// routes the pipe through the substrate-primitive
/// [`ArchVerdict::as_str`] accessor without a spurious [`Copy`] deref).
impl From<&ArchVerdict> for Box<str> {
    fn from(verdict: &ArchVerdict) -> Box<str> {
        Box::<str>::from(verdict.as_str())
    }
}

/// Trait-idiomatic *owned-input, [`std::sync::Arc<str>`] output* forward
/// projection on the caixa-arch verdict-outcome two-arm closed-set
/// fieldless typed enum [`ArchVerdict`] — extends the outside-`caixa-core`
/// tier of the substrate-wide trait-idiomatic [`std::sync::Arc<str>`]
/// forward-projection campaign onto its second peer, one commit after
/// 03c043f closed the paired [`crate::invariants::InvariantKind`]
/// `{Self, &Self}` corner (4e923c1 owned + 03c043f borrowed) as the
/// first-mover on this tier. Routes byte-for-byte through the substrate-
/// primitive [`ArchVerdict::as_str`] `pub const fn` accessor via
/// [`std::sync::Arc::<str>::from`] on the returned `&'static str`.
///
/// Every consumer that binds an [`ArchVerdict`] through the standard-
/// library `.into()` / [`From<Self> for std::sync::Arc<str>`]
/// (equivalently [`Into<std::sync::Arc<str>>`]) axis — a future caixa-arch
/// admission-webhook whose per-request rejection body composes from a
/// moved [`ArchVerdict`] handle across an `.await` boundary through a
/// `<T: Into<std::sync::Arc<str>>>`-bound diagnostic-column dispatch, a
/// future per-Aplicacao arch-audit metric-key materializer holding a
/// shared-ownership per-arm verdict-outcome label across concurrent
/// tokio-scheduled reconcile loops, a future
/// `<T: Into<std::sync::Arc<str>>>`-bound `tracing`-span attributes
/// collector recording an owned [`ArchVerdict`] per-arm field onto the
/// parent span's shared-ownership context — reaches the same two
/// `"proven"` / `"rejected"` canonical-lowercase byte-strings the sibling
/// `{&'static str, String, Cow<'static, str>, Box<str>}` forward-
/// projection corner already returns.
///
/// Rust's standard library carries `impl From<&str> for
/// std::sync::Arc<str>` and `impl From<String> for std::sync::Arc<str>`
/// but no blanket `impl<T: AsRef<str>> From<T> for std::sync::Arc<str>`
/// (nor an `impl<T: fmt::Display> From<T> for std::sync::Arc<str>`), so
/// this axis is a distinct trait-idiomatic surface that a
/// `let key: std::sync::Arc<str> = verdict.into();`-shaped call site
/// reaches through this impl and no other — a paired
/// `std::sync::Arc::<str>::from(verdict.as_str())` open-code has no
/// compile-time link back to the substrate primitive, and a two-step
/// `std::sync::Arc::<str>::from(String::from(verdict))` composition
/// through the owned-`String` axis allocates twice (once into the
/// intermediate `String`, once into the [`std::sync::Arc<str>`] on the
/// `From<String>` conversion) where the single-step trait impl allocates
/// once. The shared-ownership + [`Sync`] + [`Send`] contract
/// [`std::sync::Arc<str>`] provides is the distinct value the sibling
/// [`Box<str>`] axis's owned-move return-shape cannot provide — a per-
/// arch-audit verdict-outcome label reachable from multiple concurrent
/// per-cluster reconcile tasks through the same two canonical-lowercase
/// byte-strings, without a `.clone()`-per-task materialization the
/// owned-move [`Box<str>`] axis would force.
///
/// Second peer on the outside-`caixa-core` tier of the substrate-wide
/// trait-idiomatic [`std::sync::Arc<str>`] forward-projection campaign,
/// following the first-mover [`crate::invariants::InvariantKind`] pair
/// (4e923c1 owned + 03c043f borrowed) — every remaining outside-
/// `caixa-core` closed-set fieldless typed enum peer
/// ([`caixa_lint::Severity`], [`caixa_lint::FixSafety`],
/// [`caixa_theme::Semantic`], [`caixa_provedor::FerriteRuntime`]) whose
/// [`std::sync::Arc<str>`] pair remains a future target of this campaign,
/// tracking the same 2-corner `{Self, &Self}` × 5-tier
/// `{&'static str, String, Cow<'static, str>, Box<str>,
/// std::sync::Arc<str>}` emit-set the M2 OTP-shape and M3 mesh-shape
/// tiers already converged onto. Leaves the paired borrowed-input
/// [`From<&ArchVerdict> for std::sync::Arc<str>`] `{Self, &Self}`-closer
/// as the direct next target on the same enum, mirroring the shape
/// 03c043f used to close the first-mover [`crate::invariants::InvariantKind`]
/// pair one commit after 4e923c1 opened its owned-input half.
///
/// Pinned load-bearing by
/// [`tests::arch_verdict_from_into_arc_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`ArchVerdict::as_str`] across the two-arm
/// [`ArchVerdict::ALL`] emit-set on the owned-input surface, plus a
/// blanket-derived [`Into`] shape witness and cross-axis byte-parity
/// pins against the sibling owned-input
/// `{&'static str, String, Cow<'static, str>, Box<str>}` return-shape
/// axes).
impl From<ArchVerdict> for std::sync::Arc<str> {
    fn from(verdict: ArchVerdict) -> std::sync::Arc<str> {
        std::sync::Arc::<str>::from(verdict.as_str())
    }
}

/// Trait-idiomatic *borrowed-input, [`std::sync::Arc<str>`] output* forward
/// projection on the caixa-arch verdict-outcome two-arm closed-set
/// fieldless typed enum [`ArchVerdict`] — the borrowed-input companion to
/// the paired owned-input [`From<ArchVerdict> for std::sync::Arc<str>`]
/// impl (1682f8b, one commit prior) that closes the `{Self, &Self}` input-
/// shape corner of the outside-`caixa-core` tier of the substrate-wide
/// trait-idiomatic [`std::sync::Arc<str>`] forward-projection campaign on
/// its second peer, mirroring the shape 03c043f used to close the paired
/// first-mover [`crate::invariants::InvariantKind`] `{Self, &Self}` corner
/// on this tier one commit after 4e923c1 opened its owned-input half.
/// Routes byte-for-byte through the substrate-primitive
/// [`ArchVerdict::as_str`] `pub const fn` accessor via
/// [`std::sync::Arc::<str>::from`] on the returned `&'static str`.
///
/// Every consumer that holds a `&ArchVerdict` and needs a
/// [`std::sync::Arc<str>`] — a
/// `ArchVerdict::ALL.iter().map(std::sync::Arc::<str>::from).collect::<Vec<_>>()`
/// per-arm verdict-outcome census-key materializer (whose iterator over
/// `&'static [ArchVerdict]` yields `&ArchVerdict`, not `ArchVerdict`, so
/// the paired owned-input [`From<ArchVerdict> for std::sync::Arc<str>`]
/// axis alone forces every call site through an explicit [`Copy`] deref
/// or a `.copied()` restatement rather than the direct trait-idiomatic
/// projection), a future caixa-arch admission-webhook whose per-request
/// rejection body composes from a borrowed `&ArchVerdict` handle across
/// an `.await` boundary through a `<T: Into<std::sync::Arc<str>>>`-bound
/// diagnostic-column dispatch, a future per-Aplicacao arch-audit metric-
/// key materializer holding a shared-ownership per-arm verdict-outcome
/// label from a `&ArchVerdict` borrow through a per-Aplicacao lifetime
/// and cloning the shared-ownership label into concurrent per-cluster
/// reconcile tasks through [`std::sync::Arc::clone`] rather than a
/// per-task allocation, a future
/// `<T: Into<std::sync::Arc<str>>>`-bound `tracing`-span attributes
/// collector recording a borrowed [`ArchVerdict`] per-arm field onto the
/// parent span's shared-ownership context — reaches the substrate-
/// primitive [`ArchVerdict::as_str`] accessor through this impl and no
/// other, without a `std::sync::Arc::<str>::from(verdict.as_str())`
/// open-code whose type bounds have no compile-time link back to the
/// substrate primitive.
///
/// Rust's standard library carries `impl From<&str> for
/// std::sync::Arc<str>` and `impl From<String> for std::sync::Arc<str>`
/// but no blanket `impl<T: AsRef<str>> From<&T> for std::sync::Arc<str>`
/// (nor a `Copy`-based `impl<T: Copy, U: From<T>> From<&T> for U`), so
/// every closed-set fieldless typed enum peer on the substrate that
/// carries the paired owned-input [`std::sync::Arc<str>`] axis but not
/// the borrowed-input axis forces every borrowed-input
/// [`std::sync::Arc<str>`]-parameterized call site through a spurious
/// [`Copy`] deref (`std::sync::Arc::<str>::from((*verdict).as_str())`)
/// or a `std::sync::Arc::<str>::from(verdict.as_str())` open-code whose
/// type bounds have no compile-time link back to the substrate primitive.
/// The shared-ownership + [`Sync`] + [`Send`] contract
/// [`std::sync::Arc<str>`] provides is the distinct value the sibling
/// [`Box<str>`] axis's owned-move return-shape cannot provide from a
/// borrowed-input axis without a `.clone()`-per-task materialization.
///
/// Closes the `{Self, &Self}` input-shape corner on the second peer of
/// the outside-`caixa-core` tier of the substrate-wide trait-idiomatic
/// [`std::sync::Arc<str>`] forward-projection campaign, following the
/// first-mover [`crate::invariants::InvariantKind`] `{Self, &Self}`
/// corner (4e923c1 owned + 03c043f borrowed). Same discipline as
/// c4319a8 used to close the paired [`Box<str>`] tier's `{Self, &Self}`
/// corner on this same enum one commit after 3e08f5a opened its owned-
/// input half, cc87908 closed the first M3 mesh peer
/// ([`caixa_core::aplicacao::PlacementStrategy`]) one commit after
/// 977d577 opened its owned-input half, 941748c closed the second M3
/// mesh peer ([`caixa_core::aplicacao::WitShape`]) one commit after
/// 9a59b77, and dae722f closed the third M3 mesh peer
/// ([`caixa_core::aplicacao::RateLimitUnit`]) one commit after c481bfe.
/// Leaves the remaining outside-`caixa-core` closed-set fieldless typed
/// enum peers ([`caixa_lint::Severity`], [`caixa_lint::FixSafety`],
/// [`caixa_theme::Semantic`], [`caixa_provedor::FerriteRuntime`]) as the
/// campaign's next multi-peer targets — this commit gives them a two-
/// peer `{Self, &Self}` corner template to converge onto on the outside-
/// `caixa-core` tier of the Arc<str> axis.
///
/// Pinned load-bearing by
/// [`tests::arch_verdict_from_borrowed_into_arc_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`ArchVerdict::as_str`] across the two-arm
/// [`ArchVerdict::ALL`] emit-set on the borrowed-input surface, plus a
/// blanket-derived [`Into`] shape witness, a cross-axis partition pin
/// against the paired owned-input
/// [`From<ArchVerdict> for std::sync::Arc<str>`] and the sibling
/// borrowed-input `{&'static str, String, Cow<'static, str>, Box<str>}`
/// return-shape axes, and a `.iter().map(std::sync::Arc::<str>::from)`
/// pipe witness over [`ArchVerdict::ALL`] — whose iterator yields
/// `&ArchVerdict` by construction, so the borrowed-input
/// [`std::sync::Arc<str>`] axis is what routes the pipe through the
/// substrate-primitive [`ArchVerdict::as_str`] accessor without a
/// spurious [`Copy`] deref).
impl From<&ArchVerdict> for std::sync::Arc<str> {
    fn from(verdict: &ArchVerdict) -> std::sync::Arc<str> {
        std::sync::Arc::<str>::from(verdict.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchReport {
    pub verdict: ArchVerdict,
    pub violations: Vec<Violation>,
    pub summary: String,
}

impl ArchReport {
    /// Route the verdict-family gate through the
    /// [`gen_platform::IsVariant`]-derive-generated
    /// [`ArchVerdict::is_proven`] predicate on the substrate primitive
    /// rather than the pre-lift open-coded `matches!(self.verdict,
    /// ArchVerdict::Proven)` site, so this accessor and every peer
    /// consumer of the two-arm verdict partition (the `feira tofu`
    /// HCL-emission block converged onto [`ArchVerdict::is_rejected`]
    /// in the same run) share one convention on the same closed-set-
    /// enum arm-discriminator axis. Sibling of the peer
    /// [`Self::safety_count`] site on the paired
    /// [`crate::invariants::InvariantKind`] discriminator family.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.verdict.is_proven()
    }

    #[must_use]
    pub fn safety_count(&self) -> usize {
        // Route the per-`Violation` severity-family gate through the
        // [`gen_platform::IsVariant`]-derive-generated
        // [`crate::invariants::InvariantKind::is_safety`] predicate on
        // the substrate primitive rather than the pre-lift open-coded
        // `matches!(v.kind, InvariantKind::Safety)` site, so the pair
        // of per-`ArchReport` safety-population aggregators (this
        // accessor + [`crate::run::check_manifest`]'s `safety_count`
        // local) share one convention on the same closed-set-enum
        // arm-discriminator axis. Sibling of the peer
        // [`crate::run::check_manifest`] `is_compliance()` +
        // `is_hint()` sites on the same [`InvariantKind`] discriminator
        // family — every consumer of the three-arm severity partition
        // now reaches for one typed dispatch per arm.
        self.violations
            .iter()
            .filter(|v| v.kind.is_safety())
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arch_verdict_all_enumerates_every_variant_once() {
        // Fail-before-pass-after pin on the [`gen_platform::IsVariant`]
        // derive's per-arm-list-per-enum discipline: any future variant
        // addition (a `PartiallyProven` tier the `iac-forge` policy-
        // engine grows for compliance-only violation sets, an `Unknown`
        // tier for the M4 admission-webhook's timeout-during-check
        // outcome) that lands the new variant on the enum without
        // extending [`ArchVerdict::ALL`] trips this test — the
        // compiler-checked exhaustiveness on the sibling
        // [`gen_platform::IsVariant`]-derived per-arm predicates
        // ([`ArchVerdict::is_proven`] / [`ArchVerdict::is_rejected`])
        // covers the projection axis; this pin covers the exhaustive-
        // iteration axis so both halves of the closed-set discipline
        // migrate as one edit.
        assert_eq!(
            ArchVerdict::ALL,
            &[ArchVerdict::Proven, ArchVerdict::Rejected],
        );
        // Every arm satisfies exactly the paired per-arm predicate — the
        // byte-parity pin between the [`ArchVerdict::ALL`] iteration axis
        // and the per-arm [`gen_platform::IsVariant`]-derived predicate
        // axis: for every arm, the paired predicate returns `true` and
        // every other predicate returns `false`. Same discipline the
        // sibling [`crate::invariants::InvariantKind::ALL`] +
        // per-arm `is_*` peer pins carry.
        for arm in ArchVerdict::ALL {
            let (proven, rejected) = (arm.is_proven(), arm.is_rejected());
            assert_eq!(
                usize::from(proven) + usize::from(rejected),
                1,
                "ArchVerdict::{arm:?} must satisfy exactly one of \
                 is_proven / is_rejected",
            );
        }
    }

    #[test]
    fn arch_verdict_predicates_are_byte_equal_to_matches_family() {
        // The fail-before-pass-after pin on the two-axis convergence of
        // the two pre-lift `matches!(_, ArchVerdict::…)` /
        // `_ == ArchVerdict::…` sites at [`ArchReport::passed`] (the
        // verdict-family gate — `matches!(self.verdict,
        // ArchVerdict::Proven)`) and `caixa-feira/src/cmd/tofu.rs`'s
        // HCL-emission block (`report.verdict == ArchVerdict::Rejected`)
        // onto the [`gen_platform::IsVariant`]-derive-generated per-arm
        // predicate family: for every arm, each predicate agrees
        // byte-for-byte with the pre-lift `matches!` / `==` shape.
        //
        // A future rebrand touching either endpoint (a
        // `#[is_variant(name = "…")]` attribute drift on the derive, an
        // arm rename, an accidental peer predicate that shadows the
        // derive-generated one) would silently split the two paths and
        // trip this pin. Peer of the sibling
        // [`crate::invariants::tests::invariant_kind_predicates_are_byte_equal_to_matches_family`]
        // pin on the peer closed-set-enum IsVariant convergence axis.
        for arm in ArchVerdict::ALL {
            assert_eq!(
                arm.is_proven(),
                matches!(arm, ArchVerdict::Proven),
                "ArchVerdict::{arm:?}.is_proven() must agree with \
                 matches!(_, ArchVerdict::Proven) byte-for-byte",
            );
            assert_eq!(
                arm.is_rejected(),
                matches!(arm, ArchVerdict::Rejected),
                "ArchVerdict::{arm:?}.is_rejected() must agree with \
                 matches!(_, ArchVerdict::Rejected) byte-for-byte",
            );
        }
    }

    #[test]
    fn arch_report_passed_dispatches_through_arch_verdict_is_proven() {
        // Byte-parity pin on the [`ArchReport::passed`] accessor's
        // convergence onto [`ArchVerdict::is_proven`]: for every arm in
        // [`ArchVerdict::ALL`], an [`ArchReport`] carrying that verdict
        // reports `passed()` iff the arm is `Proven`. Guards against a
        // future silent split between the accessor and the derived
        // predicate (a hand-rolled `passed()` reintroduction that
        // matches on `Rejected` instead of `Proven`, an arm rename that
        // touches the accessor but not the predicate) by asserting the
        // two paths return the same bool on every arm.
        for &verdict in ArchVerdict::ALL {
            let report = ArchReport {
                verdict,
                violations: Vec::new(),
                summary: String::new(),
            };
            assert_eq!(
                report.passed(),
                verdict.is_proven(),
                "ArchReport::passed() must dispatch through \
                 ArchVerdict::is_proven() byte-for-byte on {verdict:?}",
            );
        }
    }

    #[test]
    fn arch_verdict_as_str_byte_equals_pre_lift_debug_lowercase_form() {
        // Fail-before-pass-after byte-parity pin on the substrate-
        // canonical [`ArchVerdict::as_str`] `pub const fn` accessor vs
        // the pre-lift `format!("{:?}", verdict).to_lowercase()` shape
        // — the round-trip through the [`std::fmt::Debug`] derive plus
        // [`str::to_lowercase`] that any prospective future consumer
        // (a `feira arch` per-manifest summary line, a
        // `tracing::field::Value::Str`-arm structured-log recorder, a
        // future admission-webhook rejection body naming the accepted-
        // verdict set) would have otherwise reached — for every arm in
        // [`ArchVerdict::ALL`].
        //
        // The pre-lift shape depended on the [`std::fmt::Debug`]
        // derive's per-arm byte-string output (Rust convention gives
        // no stability guarantee) plus a per-render `String`
        // allocation from [`str::to_lowercase`]; the post-lift
        // `.as_str()` return is a `&'static str` reached in one
        // substrate-primitive dispatch. This pin makes the two paths'
        // byte-agreement load-bearing so a future silent drift between
        // them (a hand-rolled `impl Debug` that pretty-prints the arm
        // with per-arm context, an arm rename that touches `Debug` but
        // not `as_str`, or the reverse) trips at caixa-arch build time
        // rather than at a downstream consumer's silent tag drift.
        // Peer of the sibling
        // [`crate::invariants::tests::invariant_kind_as_str_byte_equals_pre_lift_debug_lowercase_form`]
        // (87c875a) pin on the peer closed-set-enum canonical-tag
        // convergence axis.
        for arm in ArchVerdict::ALL {
            let pre_lift = format!("{arm:?}").to_lowercase();
            assert_eq!(
                arm.as_str(),
                pre_lift,
                "ArchVerdict::{arm:?}.as_str() must byte-equal the \
                 pre-lift format!(\"{{:?}}\", verdict).to_lowercase() \
                 shape",
            );
        }
    }

    #[test]
    fn arch_verdict_display_and_as_ref_str_route_through_as_str_accessor() {
        // Three-path convergence pin: the paired [`std::fmt::Display`]
        // impl, the paired [`AsRef<str>`] impl, and the substrate-
        // canonical [`ArchVerdict::as_str`] `pub const fn` accessor
        // must resolve to the same `&'static str` per arm.
        //
        // Guards against any future silent detour that routes one impl
        // through a divergent projection (a hand-rolled per-arm match
        // in the `fmt` body, an `impl AsRef<str>` swap onto a
        // hypothetical wire_name axis, a rename that touches one
        // endpoint but not the paired sibling) — the pin trips at
        // caixa-arch test time rather than at a downstream consumer's
        // silent tag split. Peer of the sibling
        // [`crate::invariants::tests::invariant_kind_display_and_as_ref_str_route_through_as_str_accessor`]
        // (87c875a) three-path convergence pin on the peer caixa-arch
        // `InvariantKind` axis, and of the workspace-wide
        // [`caixa_lint::diagnostic::tests::severity_as_ref_str_routes_through_as_str_accessor`]
        // (ce9d1e3) sibling pin on the caixa-lint `Severity` axis.
        for &arm in ArchVerdict::ALL {
            let via_as_str: &str = arm.as_str();
            let via_as_ref: &str = arm.as_ref();
            let via_display = arm.to_string();
            assert_eq!(
                via_as_ref, via_as_str,
                "ArchVerdict::{arm:?} AsRef<str>::as_ref() must \
                 byte-equal as_str()",
            );
            assert_eq!(
                via_display, via_as_str,
                "ArchVerdict::{arm:?} Display::fmt() must byte-equal \
                 as_str()",
            );
        }
    }

    #[test]
    fn arch_verdict_from_wire_accepts_every_as_str_output() {
        // Fail-before-pass-after per-arm accept pin on the newly lifted
        // [`ArchVerdict::from_wire`] reverse projection: every arm in
        // [`ArchVerdict::ALL`] must parse back through `from_wire` when
        // fed its own [`ArchVerdict::as_str`] output, landing on
        // `Some(same_variant)`. A regression that hand-rolled either
        // side's per-arm match without threading through the shared
        // two-string closed set would silently disagree on any future
        // arm rename (or a new arm the `iac-forge` policy-engine grows
        // — a `PartiallyProven` tier for compliance-only violation
        // sets, an `Unknown` tier for the M4 admission-webhook's
        // timeout-during-check outcome) and this pin flags it at
        // caixa-arch build time rather than at a downstream `feira
        // arch --verdict` consumer's silent tag misclassification.
        // Peer of the sibling
        // [`crate::invariants::tests::invariant_kind_from_wire_accepts_every_as_str_output`]
        // (b9e4e61) round-trip pin on the peer caixa-arch
        // `InvariantKind` reverse-projection axis, and of the sibling
        // `caixa_core::kind::tests::caixa_kind_wire_round_trips_through_from_wire`
        // (2aa6d23) / `caixa_dialeto_from_wire_accepts_every_as_str_output`
        // (d0e65ea) / `placement_strategy_from_wire_accepts_every_lifted_constant`
        // (18c7342) / `dep_list_round_trips_through_as_str_and_from_wire`
        // (45ee563) round-trip pins on the sibling closed-set typed-enum
        // reverse-projection axes.
        for &variant in ArchVerdict::ALL {
            let wire = variant.as_str();
            let parsed = ArchVerdict::from_wire(wire).unwrap_or_else(|| {
                panic!(
                    "ArchVerdict::from_wire({wire:?}) must accept every \
                     ArchVerdict::as_str output — got None for the wire \
                     byte-string of {variant:?}"
                )
            });
            assert_eq!(
                parsed, variant,
                "ArchVerdict::from_wire(ArchVerdict::{variant:?}.as_str()) \
                 must return ArchVerdict::{variant:?} — the (as_str, \
                 from_wire) pair must form a total round-trip on the \
                 closed two-arm ArchVerdict arm-set",
            );
        }
    }

    #[test]
    fn arch_verdict_from_wire_rejects_unknown_byte_strings() {
        // Rejection pin on the [`ArchVerdict::from_wire`] parser's
        // accept-set: any string outside the two-arm
        // [`ArchVerdict::as_str`] output set must return `None`. A
        // future accidental widening of the accept-set (a case-
        // insensitive match that accepts `"PROVEN"` / `"Proven"`, a
        // silent acceptance of the pre-lift PascalCase Debug-derived
        // shapes `"Proven"` / `"Rejected"` on the wire axis, a
        // Levenshtein-forgiving arm-lookup that admits `"provn"`
        // typos, a silent absorption of the sibling
        // [`crate::invariants::InvariantKind::as_str`] three-arm
        // accept-set — the two axes share no byte-strings but a
        // widened parser could still misclassify a peer's arm-tag as
        // a verdict) would silently drift the parser's accept-set
        // from the emitter's — a downstream audit-report re-loader
        // that bound a prior audit's [`Self::as_str`] output back to
        // the typed enum through this parser would then bind a
        // malformed byte-string to a plausibly-wrong typed arm the
        // caller does not route through any fallback, silently
        // misclassifying the reloaded row.
        //
        // Peer of the sibling
        // [`crate::invariants::tests::invariant_kind_from_wire_rejects_unknown_byte_strings`]
        // (b9e4e61) rejection pin on the peer caixa-arch
        // `InvariantKind` axis, and of the sibling
        // `caixa_kind_from_wire_rejects_unknown_byte_strings` (2aa6d23),
        // `caixa_dialeto_from_wire_rejects_unknown_byte_strings`
        // (d0e65ea), `placement_strategy_from_wire_rejects_unknown_byte_strings`
        // (18c7342), and `dep_list_from_wire_returns_none_on_unknown_wire_scalar`
        // (45ee563) rejection pins on the sibling closed-set typed-enum
        // reverse-projection axes.
        for bad in [
            "",
            " ",
            "Proven",
            "PROVEN",
            "Rejected",
            "REJECTED",
            "provn",
            "rejcted",
            "safety",
            "compliance",
            "hint",
            "warning",
            "error",
            "info",
            "fatal",
            "proven ",
            " proven",
            "proven\n",
            "proven\t",
            "rejected ",
            " rejected",
        ] {
            assert!(
                ArchVerdict::from_wire(bad).is_none(),
                "ArchVerdict::from_wire({bad:?}) must return None — \
                 the parser's accept-set is exactly the two \
                 ArchVerdict::as_str outputs; a widening would \
                 silently split the parser's accept-set from the \
                 emitter's arm-set",
            );
        }
    }

    #[test]
    fn arch_verdict_try_from_str_routes_through_from_wire_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl TryFrom<&str> for ArchVerdict` — asserts the standard-
        // library trait impl and the substrate-primitive
        // [`super::ArchVerdict::from_wire`] `Option<Self>` accessor
        // resolve to the same two-arm accept-set across every arm the
        // exhaustive [`super::ArchVerdict::ALL`] slice enumerates. Any
        // future silent detour that routes the trait impl through a
        // divergent projection (a per-arm inline `match s { "proven"
        // => Ok(Self::Proven), … }` re-inlining that opens a compile-
        // time link to the un-lifted arm-literal, a silent case-fold
        // that admits `"Proven"` / `"Rejected"` and would collide the
        // canonical-lowercase accept-set the emitter dispatches on)
        // trips at caixa-arch test time under `assert_eq!` rather
        // than at a downstream `impl TryFrom<&str>`-bound consumer's
        // silent split. Sweeps every one of the two arms
        // [`super::ArchVerdict::ALL`] carries so no arm's projection
        // is covered only by the sibling method-named `from_wire`
        // path.
        //
        // Peer of the sibling
        // [`crate::invariants::tests::invariant_kind_try_from_str_routes_through_from_wire_accessor`]
        // (e21a857) on the peer caixa-arch severity-classification
        // axis, and of
        // [`caixa_core::kind::tests::caixa_kind_try_from_str_routes_through_from_wire_accessor`]
        // (3c83606) / `caixa_dialeto_try_from_str_routes_through_from_wire_accessor`
        // (bf33136) / `placement_strategy_try_from_str_routes_through_from_wire_accessor`
        // (6fd00cd) / `rate_limit_unit_try_from_str_routes_through_from_suffix_accessor`
        // (bf78400) / `path_shape_violation_try_from_str_routes_through_from_wire_accessor`
        // (e67e48a) — extends the trait-idiomatic reverse-projection
        // axis onto the second closed-set fieldless typed enum on the
        // caixa-arch surface (the verdict-outcome axis).
        for &variant in ArchVerdict::ALL {
            let wire = variant.as_str();
            assert_eq!(
                <ArchVerdict as TryFrom<&str>>::try_from(wire),
                Ok(variant),
                "TryFrom<&str> impl on ArchVerdict must round-trip \
                 ArchVerdict::{variant:?}.as_str() = {wire:?} back \
                 to Ok(ArchVerdict::{variant:?}) — divergence from \
                 ArchVerdict::from_wire signals a silent detour off \
                 the substrate-primitive accessor",
            );
            assert_eq!(
                <ArchVerdict as TryFrom<&str>>::try_from(wire).ok(),
                ArchVerdict::from_wire(wire),
                "TryFrom<&str> ok()-projection on {wire:?} must \
                 byte-equal ArchVerdict::from_wire on the same input",
            );
        }
    }

    #[test]
    fn arch_verdict_try_from_str_rejects_unknown_byte_strings() {
        // Rejection witness on the `impl TryFrom<&str> for
        // ArchVerdict` — sweeps a candidate set of byte-strings
        // outside the two-arm canonical-lowercase wire accept-set the
        // sibling [`super::ArchVerdict::as_str`] emits and asserts
        // every one lands on `Err(())`, so a future accidental
        // widening of the trait impl's accept-set (a stray additional
        // `_ if s.eq_ignore_ascii_case("proven") => Ok(…)` case-fold
        // path, a silent acceptance of the pre-lift PascalCase Debug-
        // derived shapes `"Proven"` / `"Rejected"` on the wire axis,
        // a Levenshtein-forgiving arm-lookup that admits `"provn"`
        // typos — the exact form a `format!("{:?}", …).to_lowercase()`
        // round-trip on the paired [`std::fmt::Debug`] derive would
        // otherwise land on, the drift footgun the emitter's
        // documentation explicitly names as the reason the substrate-
        // canonical lowercase `"proven"` / `"rejected"` slug set
        // exists) trips at caixa-arch test time. The candidate set
        // includes the empty string, whitespace-only padding,
        // uppercase rebrand candidates, Levenshtein-neighbor typos,
        // sibling closed-set-enum canonical tags on the peer
        // [`crate::invariants::InvariantKind`] three-arm severity axis
        // (`"safety"` / `"compliance"` / `"hint"`) — non-shared with
        // this axis's two-arm verdict-outcome set (accepting them
        // here would silently split the parser's accept-set from the
        // emitter's arm-set and misclassify a severity-shaped byte-
        // string as a verdict), sibling `caixa_lint::Severity`
        // four-arm severity tags (`"error"` / `"warning"` / `"info"`)
        // that the verdict axis must not absorb, and
        // trailing/leading-whitespace-padded canonical tags.
        //
        // Peer of the sibling
        // [`crate::invariants::tests::invariant_kind_try_from_str_rejects_unknown_byte_strings`]
        // (e21a857) rejection pin on the peer caixa-arch severity-
        // classification axis.
        for bad in [
            "",
            " ",
            "Proven",
            "PROVEN",
            "Rejected",
            "REJECTED",
            "provn",
            "rejcted",
            "safety",
            "compliance",
            "hint",
            "warning",
            "error",
            "info",
            "fatal",
            "biblioteca",
            "servico",
            "one-for-one",
            "empty",
            "proven ",
            " proven",
            "proven\n",
            "proven\t",
            "rejected ",
            " rejected",
        ] {
            assert_eq!(
                <ArchVerdict as TryFrom<&str>>::try_from(bad),
                Err(()),
                "TryFrom<&str> for ArchVerdict({bad:?}) must return \
                 Err(()) — the trait impl's accept-set is exactly \
                 the two ArchVerdict::as_str outputs; a widening \
                 would silently split the trait impl's accept-set \
                 from the emitter's arm-set",
            );
        }
    }

    #[test]
    fn arch_verdict_try_from_str_and_from_wire_partition_the_accept_set() {
        // Cross-axis partition pin: the trait-idiomatic
        // [`TryFrom<&str>`] and the method-named
        // [`super::ArchVerdict::from_wire`] projections must return
        // equivalent decisions on every input — the trait impl's
        // `.ok()` project-out from `Result<Self, ()>` and the
        // method's `Option<Self>` return must byte-equal each other
        // on both accepts and rejects. A future silent bifurcation
        // (the trait impl gaining a case-fold path the method does
        // not carry, the method gaining a synonym alias the trait
        // impl does not honor) trips at caixa-arch test time under
        // a single pin rather than at a downstream generic-bound
        // consumer that dispatches through one axis while a peer
        // dispatches through the other. Sweeps both the two-arm
        // accept-set (via [`super::ArchVerdict::ALL`] threaded
        // through [`super::ArchVerdict::as_str`]) and a canonical
        // rejection sample so both halves of the partition are
        // covered.
        for &variant in ArchVerdict::ALL {
            let wire = variant.as_str();
            assert_eq!(
                <ArchVerdict as TryFrom<&str>>::try_from(wire).ok(),
                ArchVerdict::from_wire(wire),
                "TryFrom<&str>::ok() and from_wire must agree on \
                 ArchVerdict::{variant:?}.as_str() = {wire:?}",
            );
        }
        for bad in ["", "Proven", "unknown", "safety", "warning"] {
            assert_eq!(
                <ArchVerdict as TryFrom<&str>>::try_from(bad).ok(),
                ArchVerdict::from_wire(bad),
                "TryFrom<&str>::ok() and from_wire must agree on the \
                 rejection outcome for {bad:?}",
            );
        }
    }

    #[test]
    fn arch_verdict_from_into_static_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<ArchVerdict> for &'static str` — asserts the
        // standard-library trait impl and the substrate-primitive
        // [`super::ArchVerdict::as_str`] `pub const fn` accessor
        // resolve to the same two-arm canonical-lowercase emit-set
        // across every arm the exhaustive [`super::ArchVerdict::ALL`]
        // slice enumerates. Any future silent detour that routes the
        // trait impl through a divergent projection (a per-arm inline
        // `match verdict { Proven => "proven", … }` re-inlining that
        // opens a compile-time link to the un-lifted arm-literal
        // outside the paired [`super::ArchVerdict::as_str`] dispatch, a
        // swap onto a `format!("{:?}", …).to_lowercase()` round-trip
        // through the `#[derive(Debug)]` output whose stability is
        // *not* guaranteed and would silently reroute the diagnostic
        // tag through a stale byte-string with no downstream signal
        // until an operator scrolled the `feira tofu` terminal — the
        // exact drift footgun the sibling
        // [`super::ArchVerdict::as_str`] documentation explicitly
        // names) trips at caixa-arch test time under `assert_eq!`
        // rather than at a downstream `impl Into<&'static
        // str>`-bound consumer's silent split. Sweeps every one of the
        // two arms [`super::ArchVerdict::ALL`] carries so no arm's
        // projection is covered only by the sibling method-named
        // `as_str` / [`std::fmt::Display`] / [`AsRef<str>`] paths.
        // Materializes the `<&'static str as
        // From<ArchVerdict>>::from` output in two `const`-shape
        // bindings against the paired [`super::ArchVerdict::as_str`]
        // `pub const fn` accessor to make the `'static` lifetime
        // promise a build-time invariant — a future accidental
        // downgrade of either arm's inline canonical-lowercase
        // byte-string to a non-`&'static str` (a `String::leak()`-
        // produced return, a `Box::leak`-cast, an intermediate
        // lifetime-erasing helper) trips at caixa-arch build time
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
        // (070a6de), and
        // [`crate::invariants::tests::invariant_kind_from_into_static_str_routes_through_as_str_accessor`]
        // (f2ca7bc) pins on the sibling closed-set typed-enum forward-
        // projection axes — extends the trait-idiomatic forward-
        // projection axis onto the second closed-set fieldless typed
        // enum on the caixa-arch surface (the verdict-outcome axis),
        // extending the trait-idiomatic forward-projection family
        // onto the second outside-caixa-core closed-set fieldless
        // typed enum on the caixa surface.
        const PROVEN: &str = ArchVerdict::Proven.as_str();
        const REJECTED: &str = ArchVerdict::Rejected.as_str();
        for &variant in ArchVerdict::ALL {
            let via_trait: &'static str = <&'static str as From<ArchVerdict>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<ArchVerdict> for &'static str impl must round-trip \
                 ArchVerdict::{variant:?} to the same canonical-lowercase \
                 byte-string ArchVerdict::as_str returns — divergence \
                 signals a silent detour off the substrate-primitive \
                 accessor"
            );
            let via_into: &'static str = variant.into();
            assert_eq!(
                via_into, via_method,
                "Into<&'static str>::into on ArchVerdict::{variant:?} \
                 must byte-equal ArchVerdict::as_str on the same input \
                 — the blanket-derived Into shape must resolve to the \
                 same as_str dispatch as the explicit From impl"
            );
        }
        assert_eq!(
            [PROVEN, REJECTED],
            ["proven", "rejected"],
            "const-context ArchVerdict::as_str must resolve to the \
             two canonical-lowercase byte-strings — a future accidental \
             downgrade of either arm to a non-const or non-static \
             byte-string breaks the `&'static str`-lifetime promise the \
             paired From<ArchVerdict> for &'static str impl carries by \
             construction"
        );
    }

    #[test]
    fn arch_verdict_from_into_static_str_and_as_str_partition_the_emit_set() {
        // Cross-axis partition pin: the paired trait-idiomatic
        // `From<ArchVerdict> for &'static str` forward projection and
        // the method-named [`super::ArchVerdict::as_str`] forward
        // projection must resolve identically on *every* arm, not just
        // the ones named in the primary byte-parity pin above. Sweeps
        // every [`super::ArchVerdict::ALL`] arm and asserts the
        // trait's `From::from` output byte-equals the method-named
        // accessor's return-value on each, locking the two forward-
        // projection paths together by construction so any future
        // detour (a stray `From` special-case that lands on a divergent
        // per-arm literal outside the paired `as_str` dispatch, a
        // hypothetical rebrand touching one axis without the other)
        // trips at caixa-arch test time.
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
        // (070a6de), and
        // [`crate::invariants::tests::invariant_kind_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (f2ca7bc) — extends the round-trip discipline onto the
        // second closed-set fieldless typed enum on the caixa-arch
        // surface, closing the two-way `Self ↔ &'static str`
        // round-trip on the trait-idiomatic pair (`From<Self> for
        // &'static str` + `TryFrom<&str> for Self`) as well as the
        // pre-existing method-named pair (`as_str` + `from_wire`).
        for &variant in ArchVerdict::ALL {
            let via_trait: &'static str = <&'static str as From<ArchVerdict>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<ArchVerdict> for &'static str and \
                 ArchVerdict::as_str must resolve identically on \
                 ArchVerdict::{variant:?} — divergence signals the two \
                 forward-projection paths have drifted onto different \
                 emit-sets"
            );
        }
        // Round-trip witness: every arm's forward `From` output
        // re-parses through the paired trait-idiomatic reverse
        // `TryFrom<&str>` back to the original variant. Closes the
        // two-way `ArchVerdict ↔ &'static str` round-trip on the
        // trait-idiomatic axis pair directly (no wire-vocab
        // intermediate — the emit-side [`super::ArchVerdict::as_str`]
        // and the parse-side [`super::ArchVerdict::from_wire`]
        // dispatch on the same two inline canonical-lowercase
        // byte-strings by construction), mirroring the pre-existing
        // method-named `as_str` + `from_wire` round-trip on the
        // substrate-primitive axis pair and the peer
        // [`super::ArchVerdict`] two-halves lock pin
        // [`tests::arch_verdict_try_from_str_and_from_wire_partition_the_accept_set`]
        // on the sibling parse axis.
        for &variant in ArchVerdict::ALL {
            let emitted: &'static str = variant.into();
            let re_parsed: Result<ArchVerdict, ()> =
                <ArchVerdict as TryFrom<&str>>::try_from(emitted);
            assert_eq!(
                re_parsed,
                Ok(variant),
                "trait-idiomatic axis pair must round-trip \
                 ArchVerdict::{variant:?} through `.into::<&'static \
                 str>()` and back through `TryFrom<&str>` — a break \
                 signals the forward-emit and reverse-parse axes have \
                 drifted onto different vocabularies"
            );
        }
    }

    #[test]
    fn arch_verdict_from_borrowed_into_static_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<&ArchVerdict> for &'static str` — asserts the
        // borrowed-input standard-library trait impl and the substrate-
        // primitive [`super::ArchVerdict::as_str`] `pub const fn`
        // accessor resolve to the same two-arm canonical-lowercase
        // emit-set across every arm the exhaustive
        // [`super::ArchVerdict::ALL`] slice enumerates. Rust's `From`
        // trait does not auto-derive the borrowed-input sibling from a
        // paired owned-input impl (no `impl<T, U> From<&T> for U where
        // T: Copy, U: From<T>` blanket in `core`), so the borrowed-
        // input axis is a distinct trait-idiomatic surface that a
        // `.iter().map(Into::into)` shape over
        // [`super::ArchVerdict::ALL`] (whose iterator yields
        // `&ArchVerdict`, not `ArchVerdict`) reaches through this impl
        // and no other — the paired owned-input
        // [`From<ArchVerdict>`] impl requires an explicit `.copied()`
        // / dereference before the trait fires. Materializes the
        // `<&'static str as From<&ArchVerdict>>::from` output in two
        // `const`-shape bindings against the paired
        // [`super::ArchVerdict::as_str`] `pub const fn` accessor to
        // make the `'static` lifetime promise a build-time invariant
        // — a future accidental downgrade of either arm's inline
        // canonical-lowercase byte-string to a non-`&'static str` (a
        // `String::leak()`-produced return, a `Box::leak`-cast, an
        // intermediate lifetime-erasing helper) trips at caixa-arch
        // build time rather than at a downstream `'static`-bound
        // consumer.
        const PROVEN: &str = ArchVerdict::Proven.as_str();
        const REJECTED: &str = ArchVerdict::Rejected.as_str();
        for variant in ArchVerdict::ALL {
            let via_trait: &'static str = <&'static str as From<&ArchVerdict>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<&ArchVerdict> for &'static str impl must \
                 round-trip &ArchVerdict::{variant:?} to the same \
                 canonical-lowercase byte-string ArchVerdict::as_str \
                 returns — divergence signals a silent detour off the \
                 substrate-primitive accessor"
            );
            let via_into: &'static str = variant.into();
            assert_eq!(
                via_into, via_method,
                "Into<&'static str>::into on &ArchVerdict::{variant:?} \
                 must byte-equal ArchVerdict::as_str on the same \
                 input — the blanket-derived Into shape must resolve \
                 to the same as_str dispatch as the explicit From impl"
            );
        }
        assert_eq!(
            [PROVEN, REJECTED],
            ["proven", "rejected"],
            "const-context ArchVerdict::as_str must resolve to the \
             two canonical-lowercase byte-strings — the borrowed-\
             input From<&ArchVerdict> for &'static str impl inherits \
             its `'static` lifetime promise from the same accessor \
             the owned-input sibling routes through"
        );
    }

    #[test]
    fn arch_verdict_from_owned_and_borrowed_into_static_str_agree_on_every_arm() {
        // Cross-axis partition pin: the paired trait-idiomatic
        // owned-input `From<ArchVerdict> for &'static str` and
        // borrowed-input `From<&ArchVerdict> for &'static str` (this
        // lift) forward projections must resolve identically on every
        // arm, locking the two input-shape paths together so any
        // future detour (a stray borrowed-input special-case that
        // lands on a divergent per-arm literal outside the paired
        // `as_str` dispatch, a hypothetical rebrand touching one axis
        // without the other) trips at caixa-arch test time. Then a
        // witness that a `.iter().map(Into::into)` pipe over
        // [`super::ArchVerdict::ALL`] (whose iterator yields
        // `&ArchVerdict`) materializes the two-arm accept-set through
        // the borrowed-input axis alone — the exact shape a future M4
        // admission-webhook rejection body composer, a future
        // substrate-wide per-arm diagnostic column, or a
        // `HashMap::<&'static str, super::ArchVerdict>::from_iter(
        //     super::ArchVerdict::ALL.iter().map(|v| (v.into(), *v)))`-
        // style per-verdict lookup reaches through — closing the
        // two-way owned/borrowed input-shape symmetry on the forward-
        // projection trait-idiomatic axis. Peer of the sibling
        // [`crate::invariants::tests::invariant_kind_from_owned_and_borrowed_into_static_str_agree_on_every_arm`]
        // partition pin on the paired severity-classification axis on
        // the sibling `caixa-arch` closed-set enum — extends the
        // borrowed-input axis discipline onto the second closed-set
        // fieldless typed enum on the caixa-arch surface, the
        // verdict-outcome axis.
        for &variant in ArchVerdict::ALL {
            let owned: &'static str = <&'static str as From<ArchVerdict>>::from(variant);
            let borrowed: &'static str = <&'static str as From<&ArchVerdict>>::from(&variant);
            assert_eq!(
                owned, borrowed,
                "From<ArchVerdict> and From<&ArchVerdict> for \
                 &'static str must resolve identically on \
                 ArchVerdict::{variant:?} — divergence signals the \
                 owned-input and borrowed-input forward-projection \
                 paths have drifted onto different emit-sets"
            );
        }
        let via_iter: Vec<&'static str> = ArchVerdict::ALL.iter().map(Into::into).collect();
        let via_method: Vec<&'static str> = ArchVerdict::ALL.iter().map(|v| v.as_str()).collect();
        assert_eq!(
            via_iter, via_method,
            "`.iter().map(Into::into)` over ArchVerdict::ALL must \
             byte-equal `.iter().map(|v| v.as_str())` on every arm — \
             the borrowed-input `From<&ArchVerdict> for &'static str` \
             axis is what makes the `.iter().map(Into::into)` shape \
             route through the substrate-primitive \
             `ArchVerdict::as_str` accessor rather than through a \
             per-call-site `.copied()` / dereference detour"
        );
    }

    #[test]
    fn arch_verdict_from_into_owned_string_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<ArchVerdict> for String` — asserts the owned-
        // `String`-returning standard-library trait impl and the
        // substrate-primitive [`super::ArchVerdict::as_str`] `pub const
        // fn` accessor resolve to the same two-arm canonical-lowercase
        // emit-set across every arm the exhaustive
        // [`super::ArchVerdict::ALL`] slice enumerates. Rust's standard
        // library does not carry a blanket
        // `impl<T: AsRef<str>> From<T> for String`, so the owned-
        // `String` axis is a distinct trait-idiomatic surface that a
        // `let key: String = verdict.into();`-shaped downstream call
        // site reaches through this impl and no other — the sibling
        // `&'static str`-returning axes force an explicit
        // `.to_owned()` / [`String::from`] restatement whose type
        // bounds have no compile-time link to the substrate primitive.
        // Sweeps every one of the two arms
        // [`super::ArchVerdict::ALL`] carries so no arm's projection is
        // covered only by the sibling method-named `as_str` /
        // [`std::fmt::Display`] / [`AsRef<str>`] / owned-input
        // `&'static str`-returning paths.
        //
        // Peer of the sibling
        // [`crate::invariants::tests::invariant_kind_from_into_owned_string_routes_through_as_str_accessor`]
        // (1afd8d5 — first outside-`caixa-core` arm on the owned-
        // `String` axis, the paired severity-classification axis on
        // the sibling caixa-arch closed-set enum) — extends the trait-
        // idiomatic owned-`String`-returning forward-projection family
        // onto the second outside-`caixa-core` closed-set fieldless
        // typed enum on the caixa surface, the caixa-arch verdict-
        // outcome two-arm axis.
        for &variant in ArchVerdict::ALL {
            let via_trait: String = <String as From<ArchVerdict>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait.as_str(),
                via_method,
                "From<ArchVerdict> for String impl must round-trip \
                 ArchVerdict::{variant:?} to the same canonical-\
                 lowercase byte-string ArchVerdict::as_str returns — \
                 divergence signals a silent detour off the substrate-\
                 primitive accessor"
            );
            let via_into: String = variant.into();
            assert_eq!(
                via_into.as_str(),
                via_method,
                "Into<String>::into on ArchVerdict::{variant:?} must \
                 byte-equal ArchVerdict::as_str on the same input — \
                 the blanket-derived Into shape must resolve to the \
                 same as_str dispatch as the explicit From impl"
            );
        }
    }

    #[test]
    fn arch_verdict_from_into_owned_string_and_static_str_agree_on_every_arm() {
        // Cross-axis partition pin: the paired trait-idiomatic owned-
        // input `&'static str`-returning `From<ArchVerdict> for
        // &'static str` and owned-`String`-returning
        // `From<ArchVerdict> for String` (this lift) forward
        // projections must resolve identically on every arm, locking
        // the two output-shape paths together so any future detour (a
        // stray owned-`String` special-case that lands on a divergent
        // per-arm literal outside the paired `as_str` dispatch, a
        // hypothetical rebrand touching one axis without the other)
        // trips at caixa-arch test time. Then a witness that the
        // `ToString::to_string`-through-[`std::fmt::Display`] surface
        // (`variant.to_string()`) byte-equals the trait-idiomatic
        // owned-`String` axis (`String::from(variant)`) on every arm,
        // so a future consumer that reaches for `.to_string()` and
        // one that reaches for `.into::<String>()` land on the same
        // substrate-primitive vocabulary. Plus a
        // `.iter().copied().map(String::from)` pipe witness over
        // [`super::ArchVerdict::ALL`] — the exact shape a future per-
        // verdict histogram key materializer or admission-webhook
        // rejection body composer reaches through — materializes the
        // two-arm accept-set through the owned-`String` axis alone.
        // Plus a direct `Self → String → Self` round-trip witness
        // through the paired [`TryFrom<&str>`] axis on the owned-
        // `String`'s [`String::as_str`] borrow, closing the two-way
        // round-trip on the owned-`String` axis directly (no wire-
        // vocab intermediate — [`super::ArchVerdict::as_str`] and
        // [`super::ArchVerdict::from_wire`] dispatch on the same two
        // inline canonical-lowercase byte-strings by construction).
        for &variant in ArchVerdict::ALL {
            let owned_string: String = <String as From<ArchVerdict>>::from(variant);
            let owned_static: &'static str = <&'static str as From<ArchVerdict>>::from(variant);
            assert_eq!(
                owned_string.as_str(),
                owned_static,
                "From<ArchVerdict> for String and From<ArchVerdict> \
                 for &'static str must resolve identically on \
                 ArchVerdict::{variant:?} — divergence signals the \
                 two output-shape forward-projection paths have \
                 drifted onto different emit-sets"
            );
            let via_display: String = variant.to_string();
            assert_eq!(
                owned_string, via_display,
                "From<ArchVerdict> for String and ToString::to_string \
                 via Display must resolve identically on \
                 ArchVerdict::{variant:?} — divergence signals the \
                 trait-idiomatic owned-`String` axis and the Display-\
                 routed ToString axis have drifted onto different \
                 vocabularies"
            );
        }
        let via_iter: Vec<String> = ArchVerdict::ALL.iter().copied().map(String::from).collect();
        let via_method: Vec<String> = ArchVerdict::ALL
            .iter()
            .map(|v| v.as_str().to_owned())
            .collect();
        assert_eq!(
            via_iter, via_method,
            "`.iter().copied().map(String::from)` over \
             ArchVerdict::ALL must byte-equal \
             `.iter().map(|v| v.as_str().to_owned())` on every arm — \
             the owned-`String` `From<ArchVerdict> for String` axis \
             is what makes the `.map(String::from)` shape route \
             through the substrate-primitive `ArchVerdict::as_str` \
             accessor rather than through a per-call-site `.to_owned()` \
             / `String::from(verdict.as_str())` detour"
        );
        for &variant in ArchVerdict::ALL {
            let emitted: String = variant.into();
            let re_parsed: Result<ArchVerdict, ()> =
                <ArchVerdict as TryFrom<&str>>::try_from(emitted.as_str());
            assert_eq!(
                re_parsed,
                Ok(variant),
                "trait-idiomatic owned-`String` axis pair must round-\
                 trip ArchVerdict::{variant:?} through \
                 `.into::<String>()` and back through \
                 `TryFrom<&str>` on the owned-`String`'s \
                 `String::as_str` borrow — a break signals the \
                 forward-emit owned-`String` axis and the reverse-\
                 parse `TryFrom<&str>` axis have drifted onto \
                 different vocabularies"
            );
        }
    }

    #[test]
    fn arch_verdict_from_into_borrowed_owned_string_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<&ArchVerdict> for String` — asserts the borrowed-
        // input owned-`String`-returning standard-library trait impl
        // and the substrate-primitive [`super::ArchVerdict::as_str`]
        // `pub const fn` accessor resolve to the same two-arm
        // canonical-lowercase emit-set across every arm the exhaustive
        // [`super::ArchVerdict::ALL`] slice enumerates. Rust's
        // standard library does not carry a blanket
        // `impl<T: AsRef<str>> From<&T> for String` (nor an
        // `impl<T: fmt::Display> From<&T> for String`), so the
        // borrowed-input owned-`String` forward-projection axis is a
        // distinct trait-idiomatic surface that a
        // `let key: String = (&verdict).into();`-shaped call site
        // reaches through this impl and no other — the paired sibling
        // `From<ArchVerdict> for String` impl (cc80a53) forces every
        // borrowed-input call site through an explicit `Copy` deref
        // (`String::from(*verdict)`) or an `.as_str().to_owned()` /
        // `.to_string()` detour whose type bounds have no compile-
        // time link to the substrate primitive.
        //
        // Peer of the first-mover
        // [`caixa_core::supervisor::tests::restart_strategy_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (579385f), the second-peer
        // [`caixa_core::supervisor::tests::restart_policy_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (8465740), the third-peer
        // [`caixa_core::dep::tests::dep_list_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (e0cb617), the fourth-peer
        // [`caixa_core::kind::tests::caixa_kind_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (e76436d), the fifth-peer
        // [`caixa_core::dialeto::tests::caixa_dialeto_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (d3c0d1d), the sixth-peer
        // [`caixa_core::aplicacao::tests::placement_strategy_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (d3dc000), the seventh-peer
        // [`caixa_core::aplicacao::tests::wit_shape_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (d638fd3), the eighth-peer
        // [`caixa_core::aplicacao::tests::rate_limit_unit_from_into_borrowed_owned_string_routes_through_as_suffix_accessor`]
        // (6424e45), the ninth-peer
        // [`caixa_core::render::tests::path_shape_violation_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (b90e193 — first outside-manifest-surface arm), and the
        // tenth-peer
        // [`crate::invariants::tests::invariant_kind_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (3c3f66f — first outside-`caixa-core` arm, the paired
        // severity-classification axis on the sibling caixa-arch
        // closed-set enum) pins on the sibling closed-set typed-enum
        // borrowed-input owned-`String` forward-projection axes —
        // closes the whole `{Self, &Self} × {&'static str, String}`
        // 2×2 trait-idiomatic projection corner on the second outside-
        // `caixa-core` closed-set fieldless typed enum on the caixa
        // surface (the caixa-arch verdict-outcome two-arm axis every
        // `feira arch` / `feira tofu` per-manifest emission path
        // dispatches through), on the same trajectory the paired
        // owned-input owned-`String` axis lift (cc80a53) and the
        // paired borrowed-input owned-`&'static str` axis lift
        // (73bda50) already took onto the same enum.
        for &variant in ArchVerdict::ALL {
            let via_trait: String = <String as From<&ArchVerdict>>::from(&variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait.as_str(),
                via_method,
                "From<&ArchVerdict> for String impl must round-trip \
                 &ArchVerdict::{variant:?} to the same canonical-\
                 lowercase byte-string ArchVerdict::as_str returns — \
                 divergence signals a silent detour off the substrate-\
                 primitive accessor"
            );
            let via_into: String = (&variant).into();
            assert_eq!(
                via_into.as_str(),
                via_method,
                "Into<String>::into on &ArchVerdict::{variant:?} must \
                 byte-equal ArchVerdict::as_str on the same input — \
                 the blanket-derived Into shape must resolve to the \
                 same as_str dispatch as the explicit From impl"
            );
        }
    }

    #[test]
    fn arch_verdict_from_into_borrowed_owned_string_agrees_with_paired_axes_on_every_arm() {
        // Cross-axis partition pin: the newly lifted trait-idiomatic
        // borrowed-input owned-`String`
        // `From<&ArchVerdict> for String` (this lift), the paired
        // owned-input owned-`String`
        // `From<ArchVerdict> for String` (cc80a53), the paired
        // borrowed-input owned-`&'static str`
        // `From<&ArchVerdict> for &'static str` (73bda50), and the
        // paired owned-input owned-`&'static str`
        // `From<ArchVerdict> for &'static str` — every corner of the
        // `{Self, &Self} × {&'static str, String}` 2×2 trait-
        // idiomatic projection family — must resolve identically on
        // every arm, locking the four return-shape × input-shape
        // paths together so any future detour trips at caixa-arch
        // test time. Also byte-parity witness against the sibling
        // [`ToString::to_string`] surface routed through
        // [`std::fmt::Display`] and a direct round-trip witness
        // through the paired trait-idiomatic reverse [`TryFrom<&str>`]
        // axis on the owned-`String`'s [`String::as_str`] borrow that
        // closes the two-way `&Self → String → Self` round-trip on
        // the trait-idiomatic borrowed-input owned-`String` forward +
        // reverse axis pair.
        for &variant in ArchVerdict::ALL {
            let borrowed_string: String = <String as From<&ArchVerdict>>::from(&variant);
            let owned_string: String = <String as From<ArchVerdict>>::from(variant);
            let borrowed_static: &'static str =
                <&'static str as From<&ArchVerdict>>::from(&variant);
            let owned_static: &'static str = <&'static str as From<ArchVerdict>>::from(variant);
            assert_eq!(
                borrowed_string, owned_string,
                "From<&ArchVerdict> for String and From<ArchVerdict> \
                 for String must resolve identically on \
                 ArchVerdict::{variant:?} — divergence signals the \
                 owned-`String` axis pair's borrowed-input and owned-\
                 input arms have drifted onto different emit-sets"
            );
            assert_eq!(
                borrowed_string.as_str(),
                borrowed_static,
                "From<&ArchVerdict> for String and From<&ArchVerdict> \
                 for &'static str must resolve identically on \
                 ArchVerdict::{variant:?} — divergence signals the \
                 borrowed-input axis pair's two output-shape arms \
                 have drifted onto different emit-sets"
            );
            assert_eq!(
                borrowed_string.as_str(),
                owned_static,
                "From<&ArchVerdict> for String and From<ArchVerdict> \
                 for &'static str must resolve identically on \
                 ArchVerdict::{variant:?} — cross-diagonal of the \
                 2×2 must agree, locking the four corners onto a \
                 single substrate-primitive emit-set"
            );
            let via_display: String = variant.to_string();
            assert_eq!(
                borrowed_string, via_display,
                "From<&ArchVerdict> for String and ToString::to_string \
                 via Display must resolve identically on \
                 ArchVerdict::{variant:?} — divergence signals the \
                 trait-idiomatic borrowed-input owned-`String` axis \
                 and the Display-routed ToString axis have drifted \
                 onto different vocabularies"
            );
        }
        let via_iter: Vec<String> = ArchVerdict::ALL.iter().map(String::from).collect();
        let via_method: Vec<String> = ArchVerdict::ALL
            .iter()
            .map(|v| v.as_str().to_owned())
            .collect();
        assert_eq!(
            via_iter, via_method,
            "`.iter().map(String::from)` over ArchVerdict::ALL must \
             byte-equal `.iter().map(|v| v.as_str().to_owned())` on \
             every arm — the borrowed-input owned-`String` \
             `From<&ArchVerdict> for String` axis is what makes the \
             `.iter().map(String::from)` shape route through the \
             substrate-primitive `ArchVerdict::as_str` accessor \
             (whose iterator yields `&ArchVerdict` by construction) \
             rather than through a per-call-site `.copied()` / \
             spurious `Copy` deref detour"
        );
        for &variant in ArchVerdict::ALL {
            let emitted: String = (&variant).into();
            let re_parsed: Result<ArchVerdict, ()> =
                <ArchVerdict as TryFrom<&str>>::try_from(emitted.as_str());
            assert_eq!(
                re_parsed,
                Ok(variant),
                "trait-idiomatic borrowed-`String` axis pair must \
                 round-trip ArchVerdict::{variant:?} through \
                 `(&variant).into::<String>()` and back through \
                 `TryFrom<&str>` on the owned-`String`'s \
                 `String::as_str` borrow — a break signals the \
                 forward-emit borrowed-input owned-`String` axis and \
                 the reverse-parse `TryFrom<&str>` axis have drifted \
                 onto different vocabularies"
            );
        }
    }

    #[test]
    fn arch_verdict_from_into_static_cow_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<ArchVerdict> for std::borrow::Cow<'static, str>`
        // — asserts the standard-library trait impl and the substrate-
        // primitive [`ArchVerdict::as_str`] `pub const fn` accessor
        // resolve to the same two-arm canonical-lowercase emit-set
        // across every arm the exhaustive [`ArchVerdict::ALL`] slice
        // enumerates. Rust's standard library does not carry a
        // blanket `impl<T: AsRef<str>> From<T> for
        // std::borrow::Cow<'static, str>` (nor an
        // `impl<T: fmt::Display> From<T> for std::borrow::Cow<'static,
        // str>`), so the [`std::borrow::Cow<'static, str>`] forward-
        // projection axis is a distinct trait-idiomatic surface that
        // a `let key: std::borrow::Cow<'static, str> =
        // verdict.into();`-shaped call site reaches through this impl
        // and no other — the paired sibling `From<ArchVerdict> for
        // &'static str` and `From<ArchVerdict> for String` impls
        // force every [`std::borrow::Cow<'static, str>`]-parameterized
        // call site through a
        // `std::borrow::Cow::Borrowed(verdict.as_str())` /
        // `std::borrow::Cow::Owned(verdict.to_string())` /
        // `String::from(verdict).into()` composition whose type bounds
        // have no compile-time link back to the substrate primitive.
        //
        // Also asserts the projection lands on the zero-alloc
        // [`std::borrow::Cow::Borrowed`] arm (not the
        // [`std::borrow::Cow::Owned`] arm) — the substrate-primitive
        // [`ArchVerdict::as_str`] accessor's `&'static str` return
        // lifetime by construction makes the borrowed arm the type-
        // correct projection with no runtime allocation. Any future
        // silent detour that routes the impl through the owned arm
        // (an accidental
        // `std::borrow::Cow::Owned(verdict.to_string())` rewrite that
        // would allocate on every call site where the `&'static str`
        // return of [`ArchVerdict::as_str`] makes the zero-alloc
        // borrowed projection type-correct) trips at caixa-arch test
        // time under the [`std::borrow::Cow::Borrowed`] discriminator
        // witness rather than at a downstream
        // [`std::borrow::Cow<'static, str>`]-bound consumer's silent
        // allocation.
        //
        // Second outside-`caixa-core` peer on the substrate-wide
        // trait-idiomatic [`std::borrow::Cow<'static, str>`] forward-
        // projection family — extends the axis off the paired
        // severity-classification axis on the sibling `caixa-arch`
        // invariant-kind closed-set enum
        // ([`crate::invariants::InvariantKind`], 9361e96 / d7f3039 —
        // first outside-`caixa-core` peer) onto the verdict-outcome
        // two-arm axis, continuing the outside-`caixa-core` tier of
        // the campaign.
        for &variant in ArchVerdict::ALL {
            let via_trait: std::borrow::Cow<'static, str> =
                <std::borrow::Cow<'static, str> as From<ArchVerdict>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait.as_ref(),
                via_method,
                "From<ArchVerdict> for Cow<'static, str> impl must \
                 round-trip ArchVerdict::{variant:?} to the same \
                 canonical-lowercase byte-string \
                 ArchVerdict::as_str returns — divergence signals a \
                 silent detour off the substrate-primitive accessor"
            );
            assert!(
                matches!(via_trait, std::borrow::Cow::Borrowed(_)),
                "From<ArchVerdict> for Cow<'static, str> impl must \
                 land on the zero-alloc Cow::Borrowed arm on \
                 ArchVerdict::{variant:?} — a Cow::Owned outcome \
                 signals the projection has silently allocated where \
                 the substrate-primitive ArchVerdict::as_str \
                 `&'static str` return makes the borrowed arm the \
                 type-correct projection"
            );
            let via_into: std::borrow::Cow<'static, str> = variant.into();
            assert_eq!(
                via_into.as_ref(),
                via_method,
                "Into<Cow<'static, str>>::into on \
                 ArchVerdict::{variant:?} must byte-equal \
                 ArchVerdict::as_str on the same input — the \
                 blanket-derived Into shape must resolve to the same \
                 as_str dispatch as the explicit From impl"
            );
            assert!(
                matches!(via_into, std::borrow::Cow::Borrowed(_)),
                "Into<Cow<'static, str>>::into on \
                 ArchVerdict::{variant:?} must land on the zero- \
                 alloc Cow::Borrowed arm — the blanket-derived Into \
                 shape must resolve to the same Cow::Borrowed \
                 dispatch as the explicit From impl"
            );
        }
    }

    #[test]
    fn arch_verdict_from_into_static_cow_str_agrees_with_paired_axes_on_every_arm() {
        // Cross-axis partition pin: the newly lifted trait-idiomatic
        // `From<ArchVerdict> for std::borrow::Cow<'static, str>`
        // (this lift), the paired owned-input
        // `From<ArchVerdict> for &'static str`, and the paired
        // owned-input `From<ArchVerdict> for String` forward
        // projections must resolve identically on every arm, locking
        // the three return-shape paths together by construction so
        // any future detour trips at caixa-arch test time. Also
        // byte-parity witness against the sibling
        // [`ToString::to_string`] surface routed through
        // [`std::fmt::Display`] — every owned-heap-string path (the
        // [`std::borrow::Cow::Owned`] promotion of this axis's
        // `.into_owned()`, `From<ArchVerdict> for String`, and
        // `.to_string()`) resolves to the same canonical-lowercase
        // byte-string per arm.
        //
        // Then a `.iter().copied().map(std::borrow::Cow::from)` pipe
        // witness over [`ArchVerdict::ALL`] that materializes the
        // two-arm accept-set through the
        // [`std::borrow::Cow<'static, str>`] axis alone — the exact
        // shape a future `axum::response::IntoResponse` per-verdict
        // rejection-body composer, a future M4
        // `mesh.pleme.io/v1alpha1/ArchAudit` CR materializer's
        // admission-webhook per-verdict rejection-reason emitter
        // whose typing rules out the sibling [`AsRef<str>`] borrowed
        // return, or a future substrate-wide per-verdict diagnostic
        // surface that binds through a
        // [`std::borrow::Cow<'static, str>`] boundary reaches through
        // — closing the composable-projection axis on the caixa-arch
        // verdict-outcome two-arm closed-set fieldless typed enum
        // peer. The pipe witness also pins the zero-alloc discipline:
        // every element in the collected vector satisfies the
        // [`std::borrow::Cow::Borrowed`] arm predicate, so a future
        // accidental silent-allocation regression on the pipe's
        // iteration axis is a caixa-arch-test-time failure.
        //
        // Then a direct round-trip witness through [`TryFrom<&str>`]
        // on the projection's [`std::borrow::Cow::as_ref`] borrow —
        // unlike the peer [`caixa_core::CaixaKind`] axis pair (whose
        // forward emit lands on the lowercase Portuguese diagnostic
        // vocabulary while the reverse parse lands on the
        // `PascalCase` wire vocabulary, forcing the round-trip
        // through an intermediate
        // [`caixa_core::CaixaKind::wire_name`] hop), [`ArchVerdict`]'s
        // forward emit and reverse parse share the same two inline
        // canonical-lowercase byte-strings by construction, so the
        // [`std::borrow::Cow<'static, str>`] projection composes
        // directly with the trait-idiomatic reverse [`TryFrom<&str>`]
        // axis without the wire-vocab intermediate hop.
        for &variant in ArchVerdict::ALL {
            let via_cow: std::borrow::Cow<'static, str> =
                <std::borrow::Cow<'static, str> as From<ArchVerdict>>::from(variant);
            let via_static: &'static str = <&'static str as From<ArchVerdict>>::from(variant);
            let via_string: String = <String as From<ArchVerdict>>::from(variant);
            assert_eq!(
                via_cow.as_ref(),
                via_static,
                "From<ArchVerdict> for Cow<'static, str> and \
                 From<ArchVerdict> for &'static str must resolve \
                 identically on ArchVerdict::{variant:?} — \
                 divergence signals the Cow<'static, str> and \
                 &'static str return-shape paths have drifted onto \
                 different emit-sets"
            );
            assert_eq!(
                via_cow.as_ref(),
                via_string.as_str(),
                "From<ArchVerdict> for Cow<'static, str> and \
                 From<ArchVerdict> for String must resolve \
                 identically on ArchVerdict::{variant:?} — \
                 divergence signals the Cow<'static, str> and String \
                 return-shape paths have drifted onto different \
                 emit-sets"
            );
            let via_to_string: String = variant.to_string();
            assert_eq!(
                via_cow.as_ref(),
                via_to_string.as_str(),
                "From<ArchVerdict> for Cow<'static, str> must byte- \
                 equal ArchVerdict::to_string on \
                 ArchVerdict::{variant:?} — divergence signals the \
                 trait-idiomatic Cow<'static, str> forward- \
                 projection axis and the ToString-through-Display \
                 axis have drifted onto different emit-sets"
            );
        }
        let via_iter: Vec<std::borrow::Cow<'static, str>> = ArchVerdict::ALL
            .iter()
            .copied()
            .map(std::borrow::Cow::from)
            .collect();
        let via_method: Vec<std::borrow::Cow<'static, str>> = ArchVerdict::ALL
            .iter()
            .map(|v| std::borrow::Cow::Borrowed(v.as_str()))
            .collect();
        assert_eq!(
            via_iter, via_method,
            "`.iter().copied().map(Cow::from)` over \
             ArchVerdict::ALL must byte-equal `.iter().map(|v| \
             Cow::Borrowed(v.as_str()))` on every arm — the trait- \
             idiomatic `From<ArchVerdict> for Cow<'static, str>` \
             axis is what makes the `Cow::from` composition route \
             through the substrate-primitive ArchVerdict::as_str \
             accessor rather than a per-call-site open-code"
        );
        for cow in &via_iter {
            assert!(
                matches!(cow, std::borrow::Cow::Borrowed(_)),
                "`.iter().copied().map(Cow::from)` over \
                 ArchVerdict::ALL must land on the zero-alloc \
                 Cow::Borrowed arm on every element — a Cow::Owned \
                 outcome signals the pipe has silently allocated \
                 where the substrate-primitive \
                 ArchVerdict::as_str `&'static str` return makes \
                 the borrowed arm the type-correct projection"
            );
        }
        for &variant in ArchVerdict::ALL {
            let via_cow: std::borrow::Cow<'static, str> =
                <std::borrow::Cow<'static, str> as From<ArchVerdict>>::from(variant);
            let re_parsed: Result<ArchVerdict, ()> =
                <ArchVerdict as TryFrom<&str>>::try_from(via_cow.as_ref());
            assert_eq!(
                re_parsed,
                Ok(variant),
                "trait-idiomatic Cow<'static, str> forward-projection \
                 + reverse-projection axis pair must round-trip \
                 ArchVerdict::{variant:?} through \
                 `.into::<Cow<'static, str>>()` on the owned-input \
                 surface and back through `TryFrom<&str>` on the \
                 projection's Cow::as_ref borrow — a break signals \
                 the Cow<'static, str> forward-emit and reverse- \
                 parse axes have drifted onto different vocabularies \
                 (unlike the peer CaixaKind axis pair, \
                 ArchVerdict's forward emit and reverse parse \
                 share the same two inline canonical-lowercase \
                 byte-strings by construction, so the round-trip \
                 composes directly)"
            );
        }
    }

    #[test]
    fn arch_verdict_from_borrowed_into_static_cow_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<&ArchVerdict> for std::borrow::Cow<'static,
        // str>` — asserts the borrowed-input standard-library trait
        // impl and the substrate-primitive [`ArchVerdict::as_str`]
        // `pub const fn` accessor resolve to the same two-arm
        // canonical-lowercase emit-set across every arm the
        // exhaustive [`ArchVerdict::ALL`] slice enumerates. Rust's
        // standard library does not carry a blanket
        // `impl<T: AsRef<str>> From<&T> for Cow<'static, str>` (nor a
        // `Copy`-based `impl<T: Copy, U: From<T>> From<&T> for U`),
        // so the borrowed-input `Cow<'static, str>` forward-
        // projection axis is a distinct trait-idiomatic surface that
        // a `let key: Cow<'static, str> = (&verdict).into();`-shaped
        // call site or a
        // `ArchVerdict::ALL.iter().map(Cow::from)`-shaped pipe
        // reaches through this impl and no other — the paired owned-
        // input `From<ArchVerdict> for Cow<'static, str>` impl
        // (b492d5f) forces every borrowed-input call site through an
        // explicit `Copy` deref (`Cow::from(*verdict)`) or a
        // `Cow::Borrowed(verdict.as_str())` open-code whose type
        // bounds have no compile-time link back to the substrate
        // primitive.
        //
        // Also asserts the projection lands on the zero-alloc
        // [`std::borrow::Cow::Borrowed`] arm (not the
        // [`std::borrow::Cow::Owned`] arm) — the substrate-primitive
        // [`ArchVerdict::as_str`] accessor's `&'static str` return
        // lifetime by construction makes the borrowed arm the type-
        // correct projection with no runtime allocation on the
        // borrowed-input surface just as on the paired owned-input
        // surface.
        //
        // Closes the `{Self, &Self}` input-shape corner on the
        // second outside-`caixa-core` closed-set fieldless typed
        // enum peer of the substrate-wide
        // [`std::borrow::Cow<'static, str>`] forward-projection
        // campaign, exactly as d7f3039 closed it on the sibling
        // caixa-arch [`crate::invariants::InvariantKind`] one commit
        // after (9361e96), f80fbd6 on the render-side
        // `PathShapeViolation` one commit after (7342c32), ebeb9e0
        // on `CaixaDialeto` one commit after (8322511), 702cdf4 on
        // `DepList` one commit after (6858bac), 53346fb on
        // `RateLimitUnit` one commit after (1d59925) closing the
        // whole M3 mesh-shape tier, afdf0f4 on `PlacementStrategy`
        // one commit after (eee504d), 25690ef on `WitShape` one
        // commit after (8634dec), d45c409 on `CaixaKind` one commit
        // after (99c1735), and 9b3e4b3 / ee577fd on the M2 OTP-shape
        // `RestartStrategy` / `RestartPolicy` sibling peers one
        // commit after (7dd28b3 / 0612398) landed.
        for &variant in ArchVerdict::ALL {
            let via_trait: std::borrow::Cow<'static, str> =
                <std::borrow::Cow<'static, str> as From<&ArchVerdict>>::from(&variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait.as_ref(),
                via_method,
                "From<&ArchVerdict> for Cow<'static, str> impl must \
                 round-trip &ArchVerdict::{variant:?} to the same \
                 canonical-lowercase byte-string \
                 ArchVerdict::as_str returns — divergence signals a \
                 silent detour off the substrate-primitive accessor"
            );
            assert!(
                matches!(via_trait, std::borrow::Cow::Borrowed(_)),
                "From<&ArchVerdict> for Cow<'static, str> impl must \
                 land on the zero-alloc Cow::Borrowed arm on \
                 &ArchVerdict::{variant:?} — a Cow::Owned outcome \
                 signals the projection has silently allocated \
                 where the substrate-primitive ArchVerdict::as_str \
                 `&'static str` return makes the borrowed arm the \
                 type-correct projection"
            );
            let via_into: std::borrow::Cow<'static, str> = (&variant).into();
            assert_eq!(
                via_into.as_ref(),
                via_method,
                "Into<Cow<'static, str>>::into on \
                 &ArchVerdict::{variant:?} must byte-equal \
                 ArchVerdict::as_str on the same input — the \
                 blanket-derived Into shape on the borrowed-input \
                 surface must resolve to the same as_str dispatch \
                 as the explicit From impl"
            );
            assert!(
                matches!(via_into, std::borrow::Cow::Borrowed(_)),
                "Into<Cow<'static, str>>::into on \
                 &ArchVerdict::{variant:?} must land on the zero-\
                 alloc Cow::Borrowed arm — the blanket-derived Into \
                 shape on the borrowed-input surface must resolve \
                 to the same Cow::Borrowed dispatch as the explicit \
                 From impl"
            );
        }
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "cross-axis partition pin folds four return-shape paths \
                  (borrowed-input Cow<'static, str>, owned-input Cow<'static, str>, \
                  borrowed-input &'static str, borrowed-input String) plus the \
                  ToString-through-Display witness plus a `.iter().map(Cow::from)` \
                  pipe witness with zero-alloc discriminator plus a direct \
                  round-trip witness through TryFrom<&str> over two typed \
                  variants; the linear per-axis repetition is exactly what the \
                  fold is pinning — a helper would hide the shape it locks"
    )]
    fn arch_verdict_from_borrowed_into_static_cow_str_agrees_with_paired_axes_on_every_arm() {
        // Cross-axis partition pin: the newly lifted trait-idiomatic
        // borrowed-input `From<&ArchVerdict> for
        // std::borrow::Cow<'static, str>` (this lift), the paired
        // owned-input `From<ArchVerdict> for
        // std::borrow::Cow<'static, str>` (b492d5f), the paired
        // borrowed-input `From<&ArchVerdict> for &'static str`, and
        // the paired borrowed-input `From<&ArchVerdict> for String`
        // forward projections must resolve identically on every arm,
        // locking the four return-shape paths together by
        // construction so any future detour trips at caixa-arch
        // test time. Also byte-parity witness against the sibling
        // [`ToString::to_string`] surface routed through
        // [`std::fmt::Display`].
        //
        // Then a `.iter().map(std::borrow::Cow::from)` pipe witness
        // over [`ArchVerdict::ALL`] — whose iterator yields
        // `&ArchVerdict` by construction, so the borrowed-input
        // [`std::borrow::Cow<'static, str>`] axis is what routes the
        // pipe through the substrate-primitive
        // [`ArchVerdict::as_str`] accessor with the zero-alloc
        // [`std::borrow::Cow::Borrowed`] arm and without a spurious
        // [`Copy`] deref. Every collected element satisfies the
        // [`std::borrow::Cow::Borrowed`]-arm predicate so a future
        // accidental silent-allocation regression on the pipe's
        // iteration axis is a caixa-arch-test-time failure.
        //
        // Then a direct round-trip witness through [`TryFrom<&str>`]
        // on the projection's [`std::borrow::Cow::as_ref`] borrow —
        // unlike the peer [`caixa_core::CaixaKind`] axis pair (whose
        // forward emit lands on the lowercase Portuguese diagnostic
        // vocabulary while the reverse parse lands on the
        // `PascalCase` wire vocabulary, forcing the round-trip
        // through an intermediate
        // [`caixa_core::CaixaKind::wire_name`] hop),
        // [`ArchVerdict`]'s forward emit and reverse parse share the
        // same two inline canonical-lowercase byte-strings by
        // construction, so the borrowed-input
        // [`std::borrow::Cow<'static, str>`] projection composes
        // directly with the trait-idiomatic reverse
        // [`TryFrom<&str>`] axis without the wire-vocab intermediate
        // hop.
        for &variant in ArchVerdict::ALL {
            let via_borrowed_cow: std::borrow::Cow<'static, str> =
                <std::borrow::Cow<'static, str> as From<&ArchVerdict>>::from(&variant);
            let via_owned_cow: std::borrow::Cow<'static, str> =
                <std::borrow::Cow<'static, str> as From<ArchVerdict>>::from(variant);
            let via_borrowed_static: &'static str =
                <&'static str as From<&ArchVerdict>>::from(&variant);
            let via_borrowed_string: String = <String as From<&ArchVerdict>>::from(&variant);
            assert_eq!(
                via_borrowed_cow.as_ref(),
                via_owned_cow.as_ref(),
                "From<&ArchVerdict> for Cow<'static, str> and \
                 From<ArchVerdict> for Cow<'static, str> must \
                 resolve identically on ArchVerdict::{variant:?} — \
                 divergence signals the borrowed-input and owned-\
                 input Cow<'static, str> forward-projection input-\
                 shape paths have drifted onto different emit-sets"
            );
            assert_eq!(
                via_borrowed_cow.as_ref(),
                via_borrowed_static,
                "From<&ArchVerdict> for Cow<'static, str> and \
                 From<&ArchVerdict> for &'static str must resolve \
                 identically on ArchVerdict::{variant:?} — \
                 divergence signals the borrowed-input Cow<'static, \
                 str> and borrowed-input `&'static str` return-\
                 shape paths have drifted onto different emit-sets"
            );
            assert_eq!(
                via_borrowed_cow.as_ref(),
                via_borrowed_string.as_str(),
                "From<&ArchVerdict> for Cow<'static, str> and \
                 From<&ArchVerdict> for String must resolve \
                 identically on ArchVerdict::{variant:?} — \
                 divergence signals the borrowed-input Cow<'static, \
                 str> and borrowed-input owned-`String` return-\
                 shape paths have drifted onto different emit-sets"
            );
            let via_to_string: String = variant.to_string();
            assert_eq!(
                via_borrowed_cow.as_ref(),
                via_to_string.as_str(),
                "From<&ArchVerdict> for Cow<'static, str> must \
                 byte-equal ArchVerdict::to_string on \
                 ArchVerdict::{variant:?} — divergence signals the \
                 trait-idiomatic borrowed-input Cow<'static, str> \
                 forward-projection axis and the ToString-through-\
                 Display axis have drifted onto different emit-sets"
            );
            assert!(
                matches!(via_borrowed_cow, std::borrow::Cow::Borrowed(_)),
                "From<&ArchVerdict> for Cow<'static, str> must \
                 land on the zero-alloc Cow::Borrowed arm on \
                 &ArchVerdict::{variant:?} — a Cow::Owned outcome \
                 signals the borrowed-input surface has silently \
                 allocated where the substrate-primitive \
                 ArchVerdict::as_str `&'static str` return makes \
                 the borrowed arm the type-correct projection"
            );
        }
        let via_iter: Vec<std::borrow::Cow<'static, str>> = ArchVerdict::ALL
            .iter()
            .map(std::borrow::Cow::from)
            .collect();
        let via_method: Vec<std::borrow::Cow<'static, str>> = ArchVerdict::ALL
            .iter()
            .map(|v| std::borrow::Cow::Borrowed(v.as_str()))
            .collect();
        assert_eq!(
            via_iter, via_method,
            "`.iter().map(Cow::from)` over ArchVerdict::ALL — a \
             call site whose iteration axis holds `&ArchVerdict` by \
             construction — must byte-equal `.iter().map(|v| \
             Cow::Borrowed(v.as_str()))` on every arm — the \
             borrowed-input `From<&ArchVerdict> for Cow<'static, \
             str>` axis is what makes the `Cow::from` composition \
             route through the substrate-primitive \
             `ArchVerdict::as_str` accessor without a spurious \
             `Copy` deref (which would only be reachable through \
             the owned-input `From<ArchVerdict> for Cow<'static, \
             str>` axis by first calling `.copied()` on the \
             iterator)"
        );
        for cow in &via_iter {
            assert!(
                matches!(cow, std::borrow::Cow::Borrowed(_)),
                "`.iter().map(Cow::from)` over ArchVerdict::ALL \
                 must land on the zero-alloc Cow::Borrowed arm on \
                 every element — a Cow::Owned outcome signals the \
                 pipe has silently allocated through the borrowed-\
                 input axis where the substrate-primitive \
                 ArchVerdict::as_str `&'static str` return makes \
                 the borrowed arm the type-correct projection"
            );
        }
        for &variant in ArchVerdict::ALL {
            let via_cow: std::borrow::Cow<'static, str> =
                <std::borrow::Cow<'static, str> as From<&ArchVerdict>>::from(&variant);
            let re_parsed: Result<ArchVerdict, ()> =
                <ArchVerdict as TryFrom<&str>>::try_from(via_cow.as_ref());
            assert_eq!(
                re_parsed,
                Ok(variant),
                "trait-idiomatic borrowed-input Cow<'static, str> \
                 forward-projection + reverse-projection axis pair \
                 must round-trip &ArchVerdict::{variant:?} through \
                 `(&variant).into::<Cow<'static, str>>()` on the \
                 borrowed-input surface and back through \
                 `TryFrom<&str>` on the projection's Cow::as_ref \
                 borrow — a break signals the borrowed-input \
                 Cow<'static, str> forward-emit and reverse-parse \
                 axes have drifted onto different vocabularies \
                 (unlike the peer CaixaKind axis pair, \
                 ArchVerdict's forward emit and reverse parse share \
                 the same two inline canonical-lowercase byte-\
                 strings by construction, so the round-trip \
                 composes directly)"
            );
        }
    }

    #[test]
    fn arch_verdict_from_into_box_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<ArchVerdict> for Box<str>` — asserts the owned-
        // input standard-library trait impl and the substrate-primitive
        // [`super::ArchVerdict::as_str`] `pub const fn` accessor
        // resolve to the same two-arm canonical-lowercase emit-set
        // across every arm the exhaustive [`super::ArchVerdict::ALL`]
        // slice enumerates. Extends the outside-`caixa-core` tier of
        // the substrate-wide [`Box<str>`] forward-projection campaign
        // onto the second peer — the caixa-arch verdict-outcome two-arm
        // closed-set fieldless typed enum — following the first-mover
        // [`super::super::invariants::InvariantKind`] pair (10613a7
        // owned + 5901887 borrowed) that opened the tier one commit
        // prior. Rust's standard library carries `impl From<&str> for
        // Box<str>` and `impl From<String> for Box<str>` but no
        // blanket `impl<T: AsRef<str>> From<T> for Box<str>`, so this
        // axis is a distinct trait-idiomatic surface that a
        // `let key: Box<str> = verdict.into();`-shaped call site
        // reaches through this impl and no other — a paired
        // `Box::from(verdict.as_str())` open-code has no compile-time
        // link back to the substrate primitive.
        for &variant in super::ArchVerdict::ALL {
            let via_trait: Box<str> = <Box<str> as From<super::ArchVerdict>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait.as_ref(),
                via_method,
                "From<ArchVerdict> for Box<str> impl must round-trip \
                 ArchVerdict::{variant:?} to the same canonical- \
                 lowercase byte-string ArchVerdict::as_str returns — \
                 divergence signals a silent detour off the substrate- \
                 primitive accessor"
            );
            let via_into: Box<str> = variant.into();
            assert_eq!(
                via_into.as_ref(),
                via_method,
                "Into<Box<str>>::into on ArchVerdict::{variant:?} \
                 must byte-equal ArchVerdict::as_str on the same \
                 input — the blanket-derived Into shape must resolve \
                 to the same as_str dispatch as the explicit From impl"
            );
        }
    }

    #[test]
    fn arch_verdict_from_borrowed_into_box_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<&ArchVerdict> for Box<str>` — asserts the
        // borrowed-input standard-library trait impl and the substrate-
        // primitive [`super::ArchVerdict::as_str`] `pub const fn`
        // accessor resolve to the same two-arm canonical-lowercase emit-
        // set across every arm the exhaustive
        // [`super::ArchVerdict::ALL`] slice enumerates. Rust's standard
        // library carries `impl From<&str> for Box<str>` and `impl
        // From<String> for Box<str>` but no blanket
        // `impl<T: AsRef<str>> From<&T> for Box<str>` (nor a `Copy`-based
        // `impl<T: Copy, U: From<T>> From<&T> for U`), so the borrowed-
        // input [`Box<str>`] forward-projection axis is a distinct
        // trait-idiomatic surface that a
        // `ArchVerdict::ALL.iter().map(Box::<str>::from)`-shaped pipe
        // (whose iterator over `&'static [ArchVerdict]` yields
        // `&ArchVerdict` by construction) or a
        // `let key: Box<str> = (&verdict).into();`-shaped call site
        // reaches through this impl and no other — the paired owned-
        // input `From<ArchVerdict> for Box<str>` impl (3e08f5a) alone
        // would force every borrowed-input call site through an
        // explicit `Copy` deref (`Box::<str>::from(*verdict)`) or a
        // `Box::<str>::from(verdict.as_str())` open-code whose type
        // bounds have no compile-time link back to the substrate
        // primitive.
        //
        // Closes the `{Self, &Self}` input-shape corner on the second
        // outside-`caixa-core` closed-set fieldless typed enum peer of
        // the substrate-wide [`Box<str>`] forward-projection campaign,
        // exactly as 5901887 closed the paired first-mover
        // `InvariantKind` axis one commit after (10613a7) landed,
        // cb1d068 closed the paired M2-OTP-shape `RestartPolicy` axis
        // one commit after (0a1b313) landed, and 3c971b2 closed the
        // paired M3-mesh-shape `PlacementStrategy` axis one commit
        // after (6d73e84) landed.
        for &variant in super::ArchVerdict::ALL {
            let via_trait: Box<str> = <Box<str> as From<&super::ArchVerdict>>::from(&variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait.as_ref(),
                via_method,
                "From<&ArchVerdict> for Box<str> impl must round-trip \
                 &ArchVerdict::{variant:?} to the same canonical- \
                 lowercase byte-string ArchVerdict::as_str returns — \
                 divergence signals a silent detour off the substrate- \
                 primitive accessor"
            );
            let via_into: Box<str> = (&variant).into();
            assert_eq!(
                via_into.as_ref(),
                via_method,
                "Into<Box<str>>::into on &ArchVerdict::{variant:?} \
                 must byte-equal ArchVerdict::as_str on the same \
                 input — the blanket-derived Into shape on the \
                 borrowed-input surface must resolve to the same \
                 as_str dispatch as the explicit From impl"
            );
        }

        // Pipe witness — the distinguishing shape that forces the
        // borrowed-input axis to be independent of the owned-input
        // peer. `ArchVerdict::ALL.iter()` yields `&ArchVerdict` by
        // construction, so `.map(Box::<str>::from)` resolves through
        // the borrowed-input `From<&ArchVerdict> for Box<str>` impl
        // and no other — without this axis, the same pipe would force
        // an explicit `.copied()` restatement whose type bounds bypass
        // the substrate primitive.
        let via_pipe: Vec<Box<str>> = super::ArchVerdict::ALL
            .iter()
            .map(Box::<str>::from)
            .collect();
        let via_accessor: Vec<&'static str> =
            super::ArchVerdict::ALL.iter().map(|v| v.as_str()).collect();
        assert_eq!(
            via_pipe.len(),
            via_accessor.len(),
            "ArchVerdict::ALL.iter().map(Box::<str>::from) pipe must \
             preserve arity against the paired ArchVerdict::as_str \
             accessor — a length divergence signals the borrowed-input \
             axis has silently rejected an arm"
        );
        for (pipe_arm, accessor_arm) in via_pipe.iter().zip(via_accessor.iter()) {
            assert_eq!(
                pipe_arm.as_ref(),
                *accessor_arm,
                "ArchVerdict::ALL.iter().map(Box::<str>::from) pipe \
                 must byte-equal the paired \
                 ArchVerdict::ALL.iter().map(|v| v.as_str()) pipe on \
                 every arm — divergence signals the borrowed-input \
                 `From<&ArchVerdict> for Box<str>` axis has silently \
                 detoured off the substrate-primitive accessor"
            );
        }
    }

    #[test]
    fn arch_verdict_from_into_arc_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<ArchVerdict> for std::sync::Arc<str>` — asserts
        // the owned-input standard-library trait impl and the
        // substrate-primitive [`super::ArchVerdict::as_str`]
        // `pub const fn` accessor resolve to the same two-arm
        // canonical-lowercase emit-set across every arm the exhaustive
        // [`super::ArchVerdict::ALL`] slice enumerates. Extends the
        // outside-`caixa-core` tier of the substrate-wide
        // [`std::sync::Arc<str>`] forward-projection campaign onto the
        // second peer — the caixa-arch verdict-outcome two-arm closed-
        // set fieldless typed enum — one commit after 03c043f closed
        // the paired first-mover
        // [`super::super::invariants::InvariantKind`] `{Self, &Self}`
        // corner (4e923c1 owned + 03c043f borrowed) on this tier.
        // Rust's standard library carries `impl From<&str> for
        // std::sync::Arc<str>` and `impl From<String> for
        // std::sync::Arc<str>` but no blanket
        // `impl<T: AsRef<str>> From<T> for std::sync::Arc<str>` (nor
        // an `impl<T: fmt::Display> From<T> for std::sync::Arc<str>`),
        // so this axis is a distinct trait-idiomatic surface that a
        // `let key: std::sync::Arc<str> = verdict.into();`-shaped call
        // site reaches through this impl and no other — a paired
        // `std::sync::Arc::<str>::from(verdict.as_str())` open-code
        // has no compile-time link back to the substrate primitive,
        // and a two-step
        // `std::sync::Arc::<str>::from(String::from(verdict))`
        // composition through the owned-`String` axis allocates twice
        // (once into the intermediate `String`, once into the
        // [`Arc<str>`] on the `From<String>` conversion) where the
        // single-step trait impl allocates once.
        //
        // Cross-axis byte-parity witness against the sibling owned-
        // input `{&'static str, String, Cow<'static, str>, Box<str>}`
        // return-shape axes — locking the five return-shape paths on
        // the owned-input surface together by construction so any
        // future detour off the substrate-primitive
        // [`super::ArchVerdict::as_str`] accessor trips at caixa-
        // arch test time.
        for &variant in super::ArchVerdict::ALL {
            let via_trait: std::sync::Arc<str> =
                <std::sync::Arc<str> as From<super::ArchVerdict>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait.as_ref(),
                via_method,
                "From<ArchVerdict> for std::sync::Arc<str> impl must \
                 round-trip ArchVerdict::{variant:?} to the same \
                 canonical-lowercase byte-string ArchVerdict::as_str \
                 returns — divergence signals a silent detour off the \
                 substrate-primitive accessor"
            );
            let via_into: std::sync::Arc<str> = variant.into();
            assert_eq!(
                via_into.as_ref(),
                via_method,
                "Into<std::sync::Arc<str>>::into on ArchVerdict::\
                 {variant:?} must byte-equal ArchVerdict::as_str on \
                 the same input — the blanket-derived Into shape must \
                 resolve to the same as_str dispatch as the explicit \
                 From impl"
            );
            let owned_static: &'static str =
                <&'static str as From<super::ArchVerdict>>::from(variant);
            assert_eq!(
                via_trait.as_ref(),
                owned_static,
                "From<ArchVerdict> for std::sync::Arc<str> and \
                 From<ArchVerdict> for &'static str must resolve \
                 identically on ArchVerdict::{variant:?} — \
                 divergence signals the owned-input \
                 std::sync::Arc<str> and &'static str return-shape \
                 paths have drifted onto different emit-sets"
            );
            let owned_string: String = <String as From<super::ArchVerdict>>::from(variant);
            assert_eq!(
                via_trait.as_ref(),
                owned_string.as_str(),
                "From<ArchVerdict> for std::sync::Arc<str> and \
                 From<ArchVerdict> for String must resolve \
                 identically on ArchVerdict::{variant:?} — \
                 divergence signals the owned-input \
                 std::sync::Arc<str> and owned-`String` return-shape \
                 paths have drifted onto different emit-sets"
            );
            let owned_cow: std::borrow::Cow<'static, str> =
                <std::borrow::Cow<'static, str> as From<super::ArchVerdict>>::from(variant);
            assert_eq!(
                via_trait.as_ref(),
                owned_cow.as_ref(),
                "From<ArchVerdict> for std::sync::Arc<str> and \
                 From<ArchVerdict> for Cow<'static, str> must \
                 resolve identically on ArchVerdict::{variant:?} — \
                 divergence signals the owned-input \
                 std::sync::Arc<str> and Cow<'static, str> return- \
                 shape paths have drifted onto different emit-sets"
            );
            let owned_box: Box<str> = <Box<str> as From<super::ArchVerdict>>::from(variant);
            assert_eq!(
                via_trait.as_ref(),
                owned_box.as_ref(),
                "From<ArchVerdict> for std::sync::Arc<str> and \
                 From<ArchVerdict> for Box<str> must resolve \
                 identically on ArchVerdict::{variant:?} — \
                 divergence signals the owned-input \
                 std::sync::Arc<str> and Box<str> return-shape paths \
                 have drifted onto different emit-sets"
            );
        }
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "cross-axis partition pin folds four return-shape paths \
                  (borrowed-input &'static str, String, Cow<'static, str>, \
                  Box<str>) plus the paired owned-input Arc<str> witness \
                  and the .iter().map(std::sync::Arc::<str>::from) pipe \
                  witness into one exhaustive round-trip over \
                  ArchVerdict::ALL — the accepted line-count cost of \
                  keying the whole borrowed-input Arc<str> corner to the \
                  substrate-primitive as_str accessor at the same test-site"
    )]
    fn arch_verdict_from_borrowed_into_arc_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<&ArchVerdict> for std::sync::Arc<str>` — asserts
        // the borrowed-input standard-library trait impl and the
        // substrate-primitive [`super::ArchVerdict::as_str`]
        // `pub const fn` accessor resolve to the same two-arm
        // canonical-lowercase emit-set across every arm the exhaustive
        // [`super::ArchVerdict::ALL`] slice enumerates. Rust's standard
        // library does not carry a blanket
        // `impl<T: AsRef<str>> From<&T> for std::sync::Arc<str>` (nor
        // a `Copy`-based `impl<T: Copy, U: From<T>> From<&T> for U`), so
        // the borrowed-input `std::sync::Arc<str>` forward-projection
        // axis is a distinct trait-idiomatic surface that a
        // `let key: std::sync::Arc<str> = (&verdict).into();`-shaped
        // call site or a
        // `ArchVerdict::ALL.iter().map(std::sync::Arc::<str>::from)`-shaped
        // pipe reaches through this impl and no other — the paired
        // owned-input `From<ArchVerdict> for std::sync::Arc<str>` impl
        // (1682f8b) forces every borrowed-input call site through an
        // explicit `Copy` deref
        // (`std::sync::Arc::<str>::from((*verdict).as_str())`) or a
        // `std::sync::Arc::<str>::from(verdict.as_str())` open-code
        // whose type bounds have no compile-time link back to the
        // substrate primitive.
        //
        // Closes the `{Self, &Self}` input-shape corner on the second
        // peer of the outside-`caixa-core` tier of the substrate-wide
        // trait-idiomatic [`std::sync::Arc<str>`] forward-projection
        // campaign, exactly as 03c043f closed the paired first-mover
        // [`super::super::invariants::InvariantKind`] `{Self, &Self}`
        // corner (4e923c1 owned + 03c043f borrowed) on this tier, as
        // c4319a8 closed the paired [`Box<str>`] tier's `{Self, &Self}`
        // corner on this same enum one commit after 3e08f5a opened its
        // owned-input half, and as cc87908 / 941748c / dae722f closed
        // the M3 mesh peers
        // ([`caixa_core::aplicacao::PlacementStrategy`],
        // [`caixa_core::aplicacao::WitShape`],
        // [`caixa_core::aplicacao::RateLimitUnit`]) one commit after
        // their respective owning halves (977d577 / 9a59b77 / c481bfe).
        //
        // Also byte-parity witness against the paired owned-input
        // [`From<ArchVerdict> for std::sync::Arc<str>`] and the sibling
        // borrowed-input [`From<&ArchVerdict> for &'static str`],
        // [`From<&ArchVerdict> for String`],
        // [`From<&ArchVerdict> for Cow<'static, str>`], and
        // [`From<&ArchVerdict> for Box<str>`] return-shape axes —
        // locking the five return-shape × input-shape paths together
        // by construction so any future detour off the substrate-
        // primitive accessor trips at caixa-arch test time. Then a
        // `.iter().map(std::sync::Arc::<str>::from)` pipe witness over
        // [`super::ArchVerdict::ALL`] — whose iterator yields
        // `&ArchVerdict` by construction, so the borrowed-input
        // [`std::sync::Arc<str>`] axis is what routes the pipe through
        // the substrate-primitive [`super::ArchVerdict::as_str`]
        // accessor without a spurious [`Copy`] deref (which would only
        // be reachable through the owned-input
        // [`From<ArchVerdict> for std::sync::Arc<str>`] axis by first
        // calling `.copied()` on the iterator).
        for &variant in super::ArchVerdict::ALL {
            let via_trait: std::sync::Arc<str> =
                <std::sync::Arc<str> as From<&super::ArchVerdict>>::from(&variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait.as_ref(),
                via_method,
                "From<&ArchVerdict> for std::sync::Arc<str> impl must \
                 round-trip &ArchVerdict::{variant:?} to the same \
                 canonical-lowercase byte-string ArchVerdict::as_str \
                 returns — divergence signals a silent detour off the \
                 substrate-primitive accessor"
            );
            let via_into: std::sync::Arc<str> = (&variant).into();
            assert_eq!(
                via_into.as_ref(),
                via_method,
                "Into<std::sync::Arc<str>>::into on &ArchVerdict::\
                 {variant:?} must byte-equal ArchVerdict::as_str on \
                 the same input — the blanket-derived Into shape must \
                 resolve to the same as_str dispatch as the explicit \
                 From impl"
            );
            let owned_arc: std::sync::Arc<str> =
                <std::sync::Arc<str> as From<super::ArchVerdict>>::from(variant);
            assert_eq!(
                via_trait, owned_arc,
                "From<&ArchVerdict> for std::sync::Arc<str> and \
                 From<ArchVerdict> for std::sync::Arc<str> must \
                 resolve identically on ArchVerdict::{variant:?} — \
                 divergence signals the borrowed-input and owned-input \
                 std::sync::Arc<str> forward-projection input-shape \
                 paths have drifted onto different emit-sets"
            );
            let borrowed_static: &'static str =
                <&'static str as From<&super::ArchVerdict>>::from(&variant);
            assert_eq!(
                via_trait.as_ref(),
                borrowed_static,
                "From<&ArchVerdict> for std::sync::Arc<str> and \
                 From<&ArchVerdict> for &'static str must resolve \
                 identically on ArchVerdict::{variant:?} — divergence \
                 signals the borrowed-input std::sync::Arc<str> and \
                 &'static str return-shape paths have drifted onto \
                 different emit-sets"
            );
            let borrowed_string: String = <String as From<&super::ArchVerdict>>::from(&variant);
            assert_eq!(
                via_trait.as_ref(),
                borrowed_string.as_str(),
                "From<&ArchVerdict> for std::sync::Arc<str> and \
                 From<&ArchVerdict> for String must resolve \
                 identically on ArchVerdict::{variant:?} — divergence \
                 signals the borrowed-input std::sync::Arc<str> and \
                 owned-`String` return-shape paths have drifted onto \
                 different emit-sets"
            );
            let borrowed_cow: std::borrow::Cow<'static, str> =
                <std::borrow::Cow<'static, str> as From<&super::ArchVerdict>>::from(&variant);
            assert_eq!(
                via_trait.as_ref(),
                borrowed_cow.as_ref(),
                "From<&ArchVerdict> for std::sync::Arc<str> and \
                 From<&ArchVerdict> for Cow<'static, str> must \
                 resolve identically on ArchVerdict::{variant:?} — \
                 divergence signals the borrowed-input \
                 std::sync::Arc<str> and Cow<'static, str> return-shape \
                 paths have drifted onto different emit-sets"
            );
            let borrowed_box: Box<str> = <Box<str> as From<&super::ArchVerdict>>::from(&variant);
            assert_eq!(
                via_trait.as_ref(),
                borrowed_box.as_ref(),
                "From<&ArchVerdict> for std::sync::Arc<str> and \
                 From<&ArchVerdict> for Box<str> must resolve \
                 identically on ArchVerdict::{variant:?} — divergence \
                 signals the borrowed-input std::sync::Arc<str> and \
                 Box<str> return-shape paths have drifted onto different \
                 emit-sets"
            );
        }
        let via_iter: Vec<std::sync::Arc<str>> = super::ArchVerdict::ALL
            .iter()
            .map(std::sync::Arc::<str>::from)
            .collect();
        let via_method: Vec<std::sync::Arc<str>> = super::ArchVerdict::ALL
            .iter()
            .map(|v| std::sync::Arc::<str>::from(v.as_str()))
            .collect();
        assert_eq!(
            via_iter, via_method,
            "`.iter().map(std::sync::Arc::<str>::from)` over \
             ArchVerdict::ALL — a call site whose iteration axis holds \
             `&ArchVerdict` by construction — must byte-equal \
             `.iter().map(|v| std::sync::Arc::<str>::from(v.as_str()))` \
             on every arm — the borrowed-input std::sync::Arc<str> \
             `From<&ArchVerdict> for std::sync::Arc<str>` axis is what \
             makes the `std::sync::Arc::<str>::from` composition route \
             through the substrate-primitive `ArchVerdict::as_str` \
             accessor without a spurious `Copy` deref (which would only \
             be reachable through the owned-input \
             `From<ArchVerdict> for std::sync::Arc<str>` axis by first \
             calling `.copied()` on the iterator)"
        );
    }
}
