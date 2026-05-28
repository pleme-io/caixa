//! Verify caixa-core's UpgradeInstruction enum registers into
//! gen-platform's fleet-wide DispatcherCatalog. caixa is the
//! first NON-ADAPTER consumer of the dispatcher catamorphism —
//! demonstrates the substrate's ★★ "second class of consumer"
//! adoption criterion (theory/QUIRK-APPLIER.md §V.1).
//!
//! The variants directly mirror Erlang/OTP's appup low-level
//! instructions (load_module / code_change / soft_purge /
//! purge / restart) — the substrate's typed shadow of an OTP
//! ecosystem primitive.

use caixa_core::UpgradeInstruction;
use gen_platform::{catalog, TypedDispatcherTrait};

#[test]
fn upgrade_instruction_registers_into_fleet_catalog() {
    let entry =
        catalog::by_label("caixa.upgrade-instruction").expect(
            "caixa-core must register the UpgradeInstruction dispatcher",
        );
    assert_eq!(entry.label, "caixa.upgrade-instruction");
    assert_eq!((entry.variant_count)(), 5);
}

#[test]
fn variant_kinds_match_otp_appup_kebab() {
    let entry = catalog::by_label("caixa.upgrade-instruction").unwrap();
    let kinds = (entry.variant_kinds)();
    // Mirrors Erlang/OTP `appup`'s low-level instructions
    // (`code:load_module`, `code_change`, `code:soft_purge`,
    // `code:purge`) plus the typed fallback `Restart`.
    assert_eq!(
        kinds,
        vec!["load-module", "state-change", "soft-purge", "purge", "restart"]
    );
}

#[test]
fn variant_fields_match_otp_appup_arity() {
    let entry = catalog::by_label("caixa.upgrade-instruction").unwrap();
    let fields = (entry.variant_fields)();
    assert_eq!(
        fields,
        vec![
            ("load-module", vec!["module"]),
            ("state-change", vec!["script"]),
            ("soft-purge", vec!["module"]),
            ("purge", vec!["module"]),
            ("restart", vec![]),
        ]
    );
}

#[test]
fn reflection_round_trips_through_serde_tags() {
    let entry = catalog::by_label("caixa.upgrade-instruction").unwrap();
    let reflected_kinds = (entry.variant_kinds)();

    // Build one instance of each variant and assert the serde
    // tag matches the reflected kind.
    let samples = [
        (
            UpgradeInstruction::LoadModule { module: "x".into() },
            "load-module",
        ),
        (
            UpgradeInstruction::StateChange {
                script: std::path::PathBuf::from("a.lisp"),
            },
            "state-change",
        ),
        (
            UpgradeInstruction::SoftPurge { module: "y".into() },
            "soft-purge",
        ),
        (
            UpgradeInstruction::Purge { module: "z".into() },
            "purge",
        ),
        (UpgradeInstruction::Restart, "restart"),
    ];
    for (sample, expected_kind) in &samples {
        let v: serde_json::Value = serde_json::to_value(sample).unwrap();
        assert_eq!(
            v.get("kind").and_then(|k| k.as_str()),
            Some(*expected_kind),
            "variant {sample:?} should serialize with kind {expected_kind}"
        );
        // And the reflected kind list must contain it.
        assert!(reflected_kinds.contains(expected_kind));
    }
}

#[test]
fn variant_count_matches_reflected_kinds_len() {
    assert_eq!(UpgradeInstruction::variant_count(), 5);
    assert_eq!(
        UpgradeInstruction::variant_count(),
        UpgradeInstruction::variant_kinds().len()
    );
}
