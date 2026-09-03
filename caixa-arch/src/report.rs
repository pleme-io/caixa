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
}
