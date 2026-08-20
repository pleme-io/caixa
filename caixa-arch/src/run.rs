//! Execute the full invariant set over a manifest.

use caixa_teia::TeiaManifest;

use crate::invariants::{Invariant, builtin_invariants};
use crate::report::{ArchReport, ArchVerdict};

/// Run every built-in invariant + any extras; return a verdict.
#[must_use]
pub fn check_manifest(manifest: &TeiaManifest, extras: &[Invariant]) -> ArchReport {
    let mut all: Vec<Invariant> = builtin_invariants();
    all.extend_from_slice(extras);
    let mut violations = Vec::new();
    for inv in &all {
        violations.extend((inv.check)(manifest));
    }
    // The three per-arm severity counts route through the
    // [`gen_platform::IsVariant`]-derive-generated per-arm predicates
    // ([`InvariantKind::is_safety`] / [`InvariantKind::is_compliance`] /
    // [`InvariantKind::is_hint`]) rather than open-coded
    // `matches!(v.kind, InvariantKind::…)` sites, so a future arm
    // rebrand or arm addition flows through one derive-generated body
    // per predicate rather than a coordinated rewrite across every
    // filter site. Peer of the sibling closed-set-enum-arm-discrimination
    // discipline every caixa-core closed-set enum
    // ([`caixa_core::UpgradeInstruction::is_load_module`] via
    // `gen_platform::IsVariant`,
    // [`caixa_core::supervisor::RestartStrategy::is_simple_one_for_one`],
    // [`caixa_core::WitContract::is_pubsub`]) already carries.
    let safety_count = violations.iter().filter(|v| v.kind.is_safety()).count();
    let verdict = if safety_count > 0 {
        ArchVerdict::Rejected
    } else {
        ArchVerdict::Proven
    };
    let summary = format!(
        "{} instance(s); {} violations ({} safety, {} compliance, {} hint)",
        manifest.instances.len(),
        violations.len(),
        safety_count,
        violations.iter().filter(|v| v.kind.is_compliance()).count(),
        violations.iter().filter(|v| v.kind.is_hint()).count(),
    );
    ArchReport {
        verdict,
        violations,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use caixa_teia::parse_teia_source;

    #[test]
    fn clean_manifest_proves_cleanly() {
        let src = r#"
(defteia :tipo aws/vpc :nome main :atributos (:cidr-block "10.0.0.0/16"))
(defteia :tipo aws/igw :nome main :atributos (:vpc-id (ref aws/vpc main id)))
"#;
        let m = parse_teia_source(src).unwrap();
        let r = check_manifest(&m, &[]);
        assert!(
            r.passed(),
            "expected Proven; got violations: {:?}",
            r.violations
        );
    }

    #[test]
    fn duplicate_instance_is_rejected() {
        let src = r#"
(defteia :tipo aws/vpc :nome main :atributos (:cidr-block "10.0.0.0/16"))
(defteia :tipo aws/vpc :nome main :atributos (:cidr-block "10.1.0.0/16"))
"#;
        let m = parse_teia_source(src).unwrap();
        let r = check_manifest(&m, &[]);
        assert_eq!(r.verdict, ArchVerdict::Rejected);
        assert!(
            r.violations
                .iter()
                .any(|v| v.invariant_id == "unique-resource-names")
        );
    }

    #[test]
    fn unresolved_ref_is_rejected() {
        let src = r"
(defteia :tipo aws/igw :nome main :atributos (:vpc-id (ref aws/vpc missing id)))
";
        let m = parse_teia_source(src).unwrap();
        let r = check_manifest(&m, &[]);
        assert_eq!(r.verdict, ArchVerdict::Rejected);
        assert!(
            r.violations
                .iter()
                .any(|v| v.invariant_id == "no-unresolved-refs")
        );
    }

    #[test]
    fn bad_cidr_is_hint_not_reject() {
        let src = r#"
(defteia :tipo aws/vpc :nome main :atributos (:cidr-block "not-a-cidr"))
"#;
        let m = parse_teia_source(src).unwrap();
        let r = check_manifest(&m, &[]);
        assert!(r.passed(), "hints should not reject");
        assert!(
            r.violations
                .iter()
                .any(|v| v.invariant_id == "cidr-block-looks-valid")
        );
    }
}
