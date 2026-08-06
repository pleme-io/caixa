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
}
