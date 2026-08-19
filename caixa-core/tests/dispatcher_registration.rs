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

use caixa_core::{
    M2_UPGRADE_INSTRUCTION_FIELD_KEY_MODULE, M2_UPGRADE_INSTRUCTION_FIELD_KEY_SCRIPT,
    M2_UPGRADE_INSTRUCTION_KEY_KIND, RestartPolicy, RestartStrategy, UpgradeInstruction,
};
use gen_platform::{TypedDispatcherTrait, catalog};

#[test]
fn upgrade_instruction_registers_into_fleet_catalog() {
    let entry = catalog::by_label("caixa.upgrade-instruction")
        .expect("caixa-core must register the UpgradeInstruction dispatcher");
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
        vec![
            "load-module",
            "state-change",
            "soft-purge",
            "purge",
            "restart"
        ]
    );
}

#[test]
fn variant_fields_match_otp_appup_arity() {
    let entry = catalog::by_label("caixa.upgrade-instruction").unwrap();
    let fields = (entry.variant_fields)();
    assert_eq!(
        fields,
        vec![
            ("load-module", vec![M2_UPGRADE_INSTRUCTION_FIELD_KEY_MODULE]),
            (
                "state-change",
                vec![M2_UPGRADE_INSTRUCTION_FIELD_KEY_SCRIPT]
            ),
            ("soft-purge", vec![M2_UPGRADE_INSTRUCTION_FIELD_KEY_MODULE]),
            ("purge", vec![M2_UPGRADE_INSTRUCTION_FIELD_KEY_MODULE]),
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
        (UpgradeInstruction::Purge { module: "z".into() }, "purge"),
        (UpgradeInstruction::Restart, "restart"),
    ];
    for (sample, expected_kind) in &samples {
        let v: serde_json::Value = serde_json::to_value(sample).unwrap();
        assert_eq!(
            v.get(M2_UPGRADE_INSTRUCTION_KEY_KIND)
                .and_then(|k| k.as_str()),
            Some(*expected_kind),
            "variant {sample:?} should serialize with tag {M2_UPGRADE_INSTRUCTION_KEY_KIND}={expected_kind}"
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

#[test]
fn restart_strategy_registers_into_catalog() {
    let entry = catalog::by_label("caixa.restart-strategy").expect("RestartStrategy must register");
    assert_eq!((entry.variant_count)(), 4);
    let kinds = (entry.variant_kinds)();
    assert_eq!(
        kinds,
        vec![
            "one-for-one",
            "one-for-all",
            "rest-for-one",
            "simple-one-for-one"
        ]
    );
}

#[test]
fn restart_policy_registers_into_catalog() {
    let entry = catalog::by_label("caixa.restart-policy").expect("RestartPolicy must register");
    assert_eq!((entry.variant_count)(), 3);
    let kinds = (entry.variant_kinds)();
    assert_eq!(kinds, vec!["permanent", "temporary", "transient"]);
}

#[test]
fn restart_strategy_kinds_via_trait() {
    assert_eq!(RestartStrategy::variant_count(), 4);
    assert_eq!(
        RestartStrategy::variant_kinds(),
        vec![
            "one-for-one",
            "one-for-all",
            "rest-for-one",
            "simple-one-for-one"
        ]
    );
}

#[test]
fn restart_policy_kinds_via_trait() {
    assert_eq!(RestartPolicy::variant_count(), 3);
    assert_eq!(
        RestartPolicy::variant_kinds(),
        vec!["permanent", "temporary", "transient"]
    );
}

#[test]
fn upgrade_instruction_discriminant_returns_kebab_kind() {
    let load = UpgradeInstruction::LoadModule { module: "x".into() };
    let restart = UpgradeInstruction::Restart;
    assert_eq!(load.discriminant(), "load-module");
    assert_eq!(restart.discriminant(), "restart");
}

#[test]
fn upgrade_instruction_is_variant_predicates() {
    let load = UpgradeInstruction::LoadModule { module: "x".into() };
    let restart = UpgradeInstruction::Restart;
    let purge = UpgradeInstruction::Purge { module: "y".into() };

    assert!(load.is_load_module());
    assert!(!load.is_restart());
    assert!(!load.is_purge());

    assert!(restart.is_restart());
    assert!(!restart.is_load_module());

    assert!(purge.is_purge());
    assert!(!purge.is_soft_purge());
}

#[test]
fn upgrade_instruction_const_fn_in_const_context() {
    // Unit variant Restart works in const context — proves
    // Discriminant + IsVariant emit real `const fn`.
    //
    // The `is_restart` pin lives inside `const { assert!(..) }` so
    // the compiler enforces both halves (arm predicate is `const`-
    // callable AND returns `true` for the matching arm) at caixa-core
    // compile time — peer to the sibling
    // [`crate::CaixaKind::is_*`] + [`WitTarget::is_*`] +
    // [`PlacementStrategy::is_*`] const-block pins on the closed-set
    // typed enum arm-predicate const-callability axis.
    const RESTART_KIND: &str = UpgradeInstruction::Restart.discriminant();
    const {
        assert!(UpgradeInstruction::Restart.is_restart());
    }
    assert_eq!(RESTART_KIND, "restart");
}

#[test]
fn restart_strategy_quintet_round_trip() {
    use std::str::FromStr;
    for variant in [
        RestartStrategy::OneForOne,
        RestartStrategy::OneForAll,
        RestartStrategy::RestForOne,
        RestartStrategy::SimpleOneForOne,
    ] {
        let kind = variant.discriminant();
        let parsed = RestartStrategy::from_str(kind).unwrap_or_else(|_| {
            panic!("RestartStrategy::from_str must accept own discriminant {kind}")
        });
        assert_eq!(parsed.discriminant(), variant.discriminant());
    }
}

#[test]
fn restart_policy_quintet_round_trip() {
    use std::str::FromStr;
    for variant in [
        RestartPolicy::Permanent,
        RestartPolicy::Temporary,
        RestartPolicy::Transient,
    ] {
        let kind = variant.discriminant();
        let parsed = RestartPolicy::from_str(kind).unwrap_or_else(|_| {
            panic!("RestartPolicy::from_str must accept own discriminant {kind}")
        });
        assert_eq!(parsed.discriminant(), variant.discriminant());
    }
}

#[test]
fn restart_strategy_display_routes_through_pascalcase_wire_scalar() {
    // Post-convergence: [`std::fmt::Display`] on [`RestartStrategy`] no
    // longer delegates through the gen-platform discriminant catalog
    // (which returns kebab-case identifiers used by the fleet-wide
    // dispatcher catalog + `FromStr`); it routes through
    // [`RestartStrategy::as_str`] instead, which returns the same
    // lifted [`caixa_core::SUPERVISOR_ESTRATEGIA_*`] PascalCase
    // byte-string the `Serialize` derive emits under
    // [`caixa_core::SUPERVISOR_KEY_ESTRATEGIA`]. The two identifier
    // worlds now live on separate typed methods:
    //   - `.discriminant()` returns the kebab-case fleet-catalog
    //     identity (`"one-for-one"` / `"simple-one-for-one"`); the
    //     [`variant_kinds_via_trait`] / [`registers_into_catalog`] /
    //     [`quintet_round_trip`] pin tests above still probe this
    //     surface.
    //   - `.to_string()` / [`std::fmt::Display`] returns the
    //     PascalCase wire byte-string (`"OneForOne"` /
    //     `"SimpleOneForOne"`) so every consumer that formats the
    //     strategy for a diagnostic line, a graph, or a rejection
    //     body lands on the same byte-string the operator's
    //     per-strategy dispatch keys off.
    // The prior `_delegates_to_discriminant` name was accurate under
    // the pre-convergence route; the renamed pin here documents the
    // converged posture verbatim so a future reader can locate the
    // Display/wire agreement and the Display/discriminant separation
    // at exactly this test. Peer of the sibling
    // [`caixa_core::supervisor::tests::restart_strategy_display_routes_through_as_str_helper`]
    // and
    // [`…_display_matches_serialized_wire_byte_string`] which pin the
    // converged Display route from inside the caixa-core crate; this
    // test pins the same posture from the fleet-catalog test harness
    // so the two views agree end-to-end.
    assert_eq!(RestartStrategy::OneForOne.to_string(), "OneForOne");
    assert_eq!(
        RestartStrategy::SimpleOneForOne.to_string(),
        "SimpleOneForOne"
    );
}

#[test]
fn restart_strategy_predicates() {
    let one_for_one = RestartStrategy::OneForOne;
    assert!(one_for_one.is_one_for_one());
    assert!(!one_for_one.is_one_for_all());
    assert!(!one_for_one.is_rest_for_one());
    assert!(!one_for_one.is_simple_one_for_one());
}

#[test]
fn restart_strategy_from_str_rejects_unknown() {
    use std::str::FromStr;
    assert!(RestartStrategy::from_str("does-not-exist").is_err());
}
