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
}
