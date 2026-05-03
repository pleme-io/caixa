//! Render-side helpers shared by every per-Servico renderer
//! ([`caixa-helm`], [`caixa-flux`]) — the canonical place for "if the
//! M2 typed slot is non-empty, emit its camelCase YAML fragment under
//! the agreed key" patterns to live exactly once.
//!
//! Until this module landed both renderers carried an inline ~20-line
//! block per render entry-point that:
//!
//! 1. Checked `caixa.limits.is_some() && !limits.is_empty()`.
//! 2. Called `serde_yaml::to_value(limits).unwrap_or(Value::Null)` —
//!    silently swallowing every serialization error as a `null`-shaped
//!    fragment that would render as `limits: null` in the values block,
//!    indistinguishable from "the author omitted the slot" downstream.
//! 3. Inserted under the camelCase key `"limits"` with `or_insert`
//!    semantics so explicit `spec.*` fields from the ComputeUnit YAML
//!    take precedence over the manifest-derived overlay.
//! 4. Repeated the same shape for `:behavior` → `"behavior"` and
//!    `:upgrade-from` → `"upgradeFrom"`.
//!
//! That's the duplication budget violated three ways: same emptiness
//! check, same camelCase key, same precedence rule, written twice
//! verbatim. THEORY.md §I.3.5 ("Generation first, composition second,
//! hand-authoring last; the duplication budget is zero") promotes that
//! to a build-time concern: every recurring shape lives in a typed
//! helper before its third occurrence — and PRIME DIRECTIVE work is
//! exactly that lift.
//!
//! [`servico_m2_overlay`] is that helper. Renderers iterate the map it
//! returns and merge each `(key, value)` pair into their target with
//! their own map type's `entry().or_insert()` (so `spec.*` precedence
//! is preserved by construction).

use std::collections::BTreeMap;
use thiserror::Error;

use crate::Caixa;

/// Errors the render helpers can raise.
#[derive(Debug, Error)]
pub enum RenderError {
    /// `serde_yaml::to_value` failed for one of the M2 typed slots —
    /// theoretically impossible for the canonical
    /// [`crate::LimitsSpec`] / [`crate::BehaviorSpec`] /
    /// [`crate::UpgradeFromEntry`] types (all derive Serialize without
    /// fallible custom impls), but surfaced rather than swallowed so a
    /// future slot whose Serialize impl gains a fallible branch
    /// surfaces the failure to the caller instead of silently rendering
    /// as `null` (the prior inline block's behavior).
    #[error("yaml serialization of M2 slot {slot}: {source}")]
    Yaml {
        slot: &'static str,
        #[source]
        source: serde_yaml::Error,
    },
}

/// Canonical camelCase YAML key for the `:limits` slot's overlay.
pub const M2_KEY_LIMITS: &str = "limits";
/// Canonical camelCase YAML key for the `:behavior` slot's overlay.
pub const M2_KEY_BEHAVIOR: &str = "behavior";
/// Canonical camelCase YAML key for the `:upgrade-from` slot's overlay.
pub const M2_KEY_UPGRADE_FROM: &str = "upgradeFrom";

/// Render the M2 typed-slot YAML overlay for a Caixa: the camelCase
/// `(key, value)` fragments every per-Servico renderer
/// ([`caixa-helm`]'s values block, [`caixa-flux`]'s programs.yaml
/// entry) merges into its target with `or_insert` semantics so explicit
/// `spec.*` fields from the ComputeUnit YAML take precedence over the
/// manifest-derived overlay.
///
/// Keys (alphabetically ordered, since the return type is
/// [`BTreeMap`]) match the ComputeUnit / pleme-computeunit values
/// schema:
///
///   * [`M2_KEY_BEHAVIOR`] — present iff `caixa.behavior` is `Some`
///     and `BehaviorSpec::is_empty` returns `false`.
///   * [`M2_KEY_LIMITS`] — present iff `caixa.limits` is `Some` and
///     `LimitsSpec::is_empty` returns `false`.
///   * [`M2_KEY_UPGRADE_FROM`] — present iff `caixa.upgrade_from` is
///     non-empty.
///
/// An entirely empty M2 surface returns an empty map; the renderer
/// merges zero fragments and emits no extra keys (the per-renderer
/// "empty M2 slots do not appear" tests pin this invariant —
/// `caixa_helm::tests::empty_m2_slots_do_not_appear` and
/// `caixa_flux::tests::empty_m2_slots_do_not_appear_in_programs_yaml_entry`).
///
/// # Errors
///
/// Returns [`RenderError::Yaml`] if `serde_yaml::to_value` fails for
/// any of the typed M2 slot values. The prior inline block silently
/// substituted [`serde_yaml::Value::Null`] in this case, which renders
/// as e.g. `limits: null` — indistinguishable from "the author omitted
/// the slot" once it leaves the typed surface.
pub fn servico_m2_overlay(
    caixa: &Caixa,
) -> Result<BTreeMap<&'static str, serde_yaml::Value>, RenderError> {
    let mut out = BTreeMap::new();
    if let Some(limits) = &caixa.limits {
        if !limits.is_empty() {
            let v = serde_yaml::to_value(limits).map_err(|source| RenderError::Yaml {
                slot: M2_KEY_LIMITS,
                source,
            })?;
            out.insert(M2_KEY_LIMITS, v);
        }
    }
    if let Some(behavior) = &caixa.behavior {
        if !behavior.is_empty() {
            let v = serde_yaml::to_value(behavior).map_err(|source| RenderError::Yaml {
                slot: M2_KEY_BEHAVIOR,
                source,
            })?;
            out.insert(M2_KEY_BEHAVIOR, v);
        }
    }
    if !caixa.upgrade_from.is_empty() {
        let v = serde_yaml::to_value(&caixa.upgrade_from).map_err(|source| RenderError::Yaml {
            slot: M2_KEY_UPGRADE_FROM,
            source,
        })?;
        out.insert(M2_KEY_UPGRADE_FROM, v);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BehaviorSpec, CaixaKind, LimitsSpec, UpgradeFromEntry, UpgradeInstruction};
    use std::path::PathBuf;
    use std::time::Duration;

    fn bare_servico() -> Caixa {
        Caixa {
            nome: "hello-rio".into(),
            versao: "0.1.0".into(),
            kind: CaixaKind::Servico,
            edicao: Some("2026".into()),
            descricao: None,
            repositorio: None,
            licenca: None,
            autores: vec![],
            etiquetas: vec![],
            deps: vec![],
            deps_dev: vec![],
            exe: vec![],
            bibliotecas: vec![],
            servicos: vec!["servicos/hello-rio.computeunit.yaml".into()],
            limits: None,
            behavior: None,
            upgrade_from: vec![],
            estrategia: None,
            max_restarts: None,
            restart_window: None,
            children: vec![],
            membros: vec![],
            contratos: vec![],
            politicas: None,
            placement: None,
            entrada: None,
        }
    }

    #[test]
    fn empty_caixa_returns_empty_overlay() {
        let overlay = servico_m2_overlay(&bare_servico()).unwrap();
        assert!(
            overlay.is_empty(),
            "a Caixa with no M2 slots emits zero overlay fragments"
        );
    }

    #[test]
    fn empty_typed_specs_are_skipped_like_unset_ones() {
        // `Some(LimitsSpec::default())` (every axis None) and
        // `Some(BehaviorSpec::default())` (every callback None) must
        // round-trip identical to `None` — the is_empty()-skip
        // invariant the renderers' "empty M2 slots do not appear"
        // tests pinned inline before this lift.
        let mut c = bare_servico();
        c.limits = Some(LimitsSpec::default());
        c.behavior = Some(BehaviorSpec::default());
        let overlay = servico_m2_overlay(&c).unwrap();
        assert!(overlay.is_empty());
    }

    #[test]
    fn limits_slot_appears_under_camelcase_key() {
        let mut c = bare_servico();
        c.limits = Some(LimitsSpec {
            memory: Some(64 * 1024 * 1024),
            fuel: Some(1_000_000),
            wall_clock: Some(Duration::from_secs(30)),
            cpu: Some(500),
        });
        let overlay = servico_m2_overlay(&c).unwrap();
        assert_eq!(overlay.len(), 1);
        let limits = overlay.get(M2_KEY_LIMITS).expect("limits key present");
        assert_eq!(limits.get("memory").and_then(|m| m.as_str()), Some("64MiB"));
        assert_eq!(
            limits.get("wallClock").and_then(|m| m.as_str()),
            Some("30s")
        );
    }

    #[test]
    fn behavior_slot_appears_under_camelcase_key() {
        let mut c = bare_servico();
        c.behavior = Some(BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_call: Some(PathBuf::from("lib/handlers.lisp")),
            ..Default::default()
        });
        let overlay = servico_m2_overlay(&c).unwrap();
        let behavior = overlay.get(M2_KEY_BEHAVIOR).expect("behavior key present");
        assert_eq!(
            behavior.get("onInit").and_then(|v| v.as_str()),
            Some("lib/init.lisp")
        );
        assert_eq!(
            behavior.get("onCall").and_then(|v| v.as_str()),
            Some("lib/handlers.lisp")
        );
    }

    #[test]
    fn upgrade_from_slot_appears_under_camelcase_key() {
        let mut c = bare_servico();
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.0.9".into(),
            instructions: vec![UpgradeInstruction::LoadModule {
                module: "hello-rio".into(),
            }],
        }];
        let overlay = servico_m2_overlay(&c).unwrap();
        let upgrade = overlay
            .get(M2_KEY_UPGRADE_FROM)
            .expect("upgradeFrom key present");
        let arr = upgrade.as_sequence().expect("sequence");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0].get("from").and_then(|v| v.as_str()), Some("0.0.9"));
    }

    #[test]
    fn all_three_slots_appear_in_alphabetical_iteration_order() {
        // BTreeMap iteration is sorted by key — pin that the renderers
        // can rely on a deterministic iteration order, which feeds
        // into deterministic YAML output (the value-as-proof property
        // THEORY.md §V.2.7 "render determinism" requires).
        let mut c = bare_servico();
        c.limits = Some(LimitsSpec {
            memory: Some(64 * 1024 * 1024),
            ..Default::default()
        });
        c.behavior = Some(BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            ..Default::default()
        });
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.0.9".into(),
            instructions: vec![UpgradeInstruction::LoadModule {
                module: "hello-rio".into(),
            }],
        }];
        let overlay = servico_m2_overlay(&c).unwrap();
        let keys: Vec<_> = overlay.keys().copied().collect();
        assert_eq!(
            keys,
            vec![M2_KEY_BEHAVIOR, M2_KEY_LIMITS, M2_KEY_UPGRADE_FROM]
        );
    }

    #[test]
    fn overlay_kind_agnostic_for_field_projection() {
        // The helper projects fields, not kind — every Caixa carries
        // the M2 slot fields by construction. Renderer-level kind
        // gates (NotAServico in caixa-helm / caixa-flux) are the
        // shape filter; this helper is the field projector. Keeping
        // them separate means the same overlay can apply to any
        // future per-kind renderer (e.g. when M2.4 supervisor
        // rendering acquires its own M2-shaped overlay path).
        let mut c = bare_servico();
        c.kind = CaixaKind::Biblioteca;
        c.servicos = vec![];
        c.limits = Some(LimitsSpec {
            memory: Some(1024),
            ..Default::default()
        });
        let overlay = servico_m2_overlay(&c).unwrap();
        assert!(overlay.contains_key(M2_KEY_LIMITS));
    }
}
