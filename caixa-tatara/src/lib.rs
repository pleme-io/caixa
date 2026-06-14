//! `caixa-tatara` — typed renderer that emits a tatara `Process` CR with
//! `Intent::Aplicacao` + `Lifetime::Ephemeral` from an `:kind Aplicacao`
//! caixa.
//!
//! Same `caixa-<target>` naming as `caixa-helm` / `caixa-flux` /
//! `caixa-mesh`: a typed renderer that takes a typed `Caixa` and emits
//! the canonical source artifact for `<target>` (here: tatara's
//! `Process` CRD in the `tatara.pleme.io/v1alpha1` API group).
//!
//! Bridge contract:
//!
//! ```text
//! (defaplicacao name :kind Aplicacao :membros […] :versao "0.1.0" …)
//!   │
//!   │  caixa-helm renders → OCI chart "lareira-<name>:0.1.0"
//!   │  caixa-tatara renders → Process CR with intent.aplicacao
//!   │                          pointing at that OCI chart
//!   ▼
//! Process CR
//!   intent.aplicacao.chart_ref  = "oci://<registry>/lareira-<name>"
//!   intent.aplicacao.version    = caixa.versao
//!   intent.aplicacao.profile    = caller-supplied
//!   intent.aplicacao.values     = derived from membros
//!   lifetime.ephemeral.ttl       = caller-supplied
//!   lifetime.ephemeral.teardown  = caller-supplied
//!   boundary.postconditions      = derived from membros + contratos
//! ```
//!
//! Today the `:lifetime` slot does not yet live on `AplicacaoSpec`
//! itself in caixa-core; this renderer takes lifetime + chart context
//! as explicit `RenderInputs`. When caixa-core's `AplicacaoSpec`
//! grows a `:lifetime` slot, this surface accepts the embedded form.

#![allow(clippy::module_name_repetitions)]

use caixa_core::{Caixa, CaixaKind, lareira_chart_name};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use tatara_process::boundary::{Boundary, Condition, ConditionKind};
use tatara_process::classification::{
    Classification, ConvergencePointType, DataClassification, Horizon, SubstrateType,
};
use tatara_process::intent::{AplicacaoIntent, Intent};
use tatara_process::lifetime::{EphemeralLifetime, Lifetime, TeardownPolicy};
use tatara_process::prelude::{Process, ProcessSpec};

/// Errors caixa-tatara can raise.
#[derive(Debug, Error)]
pub enum Error {
    /// The caixa's `:kind` isn't Aplicacao.
    #[error("caixa-tatara only renders :kind Aplicacao caixas (got {0:?})")]
    NotAnAplicacao(CaixaKind),
    /// The caixa is missing its `:versao` — required to materialize a chart ref.
    #[error("caixa is missing :versao — required to materialize chart_ref")]
    MissingVersao,
    /// Serialization to YAML/JSON failed.
    #[error("serialization: {0}")]
    Serialize(String),
}

/// Result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// Inputs caixa-tatara takes alongside the caixa itself. Today these
/// carry the operator's lifetime knobs since `AplicacaoSpec` doesn't
/// yet embed them. The shape exists so the typed surface is stable
/// even once caixa-core grows a `:lifetime` slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderInputs {
    /// OCI registry the rendered chart lives in.
    pub registry: String,
    /// Architecture profile (e.g., `"gateway-with-internal-saas"`).
    /// Empty = chart default.
    pub profile: String,
    /// Target namespace the Process + chart deploy into.
    pub target_namespace: String,
    /// Ephemeral lifetime knobs.
    pub lifetime: RenderEphemeralLifetime,
    /// Free-form values overlay merged on top of chart defaults +
    /// caixa-derived values. Operator-supplied (e.g.,
    /// `{ "compliance": { "overlays": [] } }`).
    #[serde(default)]
    pub values_overlay: serde_json::Value,
    /// Optional postconditions to include alongside the auto-derived
    /// `HelmReleaseReleased` (e.g., a ClosedLoopAuth probe).
    #[serde(default)]
    pub extra_postconditions: Vec<Condition>,
}

/// Lifetime knobs in the typed surface — `From` bridge to
/// `tatara_process::lifetime::EphemeralLifetime`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderEphemeralLifetime {
    /// `humantime` TTL string (e.g., `"1h"`).
    pub ttl: String,
    /// Teardown policy.
    pub teardown_policy: TeardownPolicy,
    /// Cluster-wide concurrency cap (`0` = uncapped).
    pub max_concurrent: u32,
}

impl From<RenderEphemeralLifetime> for EphemeralLifetime {
    fn from(v: RenderEphemeralLifetime) -> Self {
        // `..Default::default()` fills `exports` (added upstream in
        // tatara-process @ c99fdb36, the typed export-trigger axis on
        // ephemeral lifetimes) with its documented `Vec::new()` default
        // — the existing typed surface here has no `:exports` slot, so
        // the canonical "no exports declared" shape carries forward
        // unchanged. Same posture every peer "future-typed-axis default-
        // forward" lift uses on the tatara-process bridge.
        Self {
            ttl: v.ttl,
            teardown_policy: v.teardown_policy,
            max_concurrent: v.max_concurrent,
            ..Default::default()
        }
    }
}

/// Render a `Caixa` (kind = Aplicacao) + `RenderInputs` to a `Process`.
pub fn process_for_aplicacao(caixa: &Caixa, inputs: &RenderInputs) -> Result<Process> {
    if caixa.kind != CaixaKind::Aplicacao {
        return Err(Error::NotAnAplicacao(caixa.kind));
    }
    if caixa.versao.is_empty() {
        return Err(Error::MissingVersao);
    }
    let versao = caixa.versao.clone();

    let chart_ref = derive_chart_ref(caixa, &inputs.registry);
    let release_name = lareira_chart_name(caixa.nome.as_str());

    let aplicacao = AplicacaoIntent {
        chart_ref,
        version: versao,
        profile: inputs.profile.clone(),
        values_overlay: inputs.values_overlay.clone(),
        release_name: Some(release_name.clone()),
        target_namespace: Some(inputs.target_namespace.clone()),
        install_timeout: Some("25m".into()),
    };

    let mut postconditions = vec![Condition {
        kind: ConditionKind::HelmReleaseReleased,
        params: serde_json::json!({
            "name": release_name,
            "namespace": inputs.target_namespace,
        }),
    }];
    postconditions.extend(inputs.extra_postconditions.iter().cloned());

    let spec = ProcessSpec {
        identity: Default::default(),
        classification: default_class(),
        intent: Intent {
            aplicacao: Some(aplicacao),
            ..Intent::default()
        },
        boundary: Boundary {
            preconditions: vec![],
            postconditions,
            timeout: Some("25m".into()),
        },
        compliance: Default::default(),
        depends_on: vec![],
        signals: Default::default(),
        lifetime: Lifetime {
            ephemeral: Some(inputs.lifetime.clone().into()),
            ..Lifetime::default()
        },
        // Added upstream in tatara-process @ c99fdb36 alongside the
        // ephemeral-lifetime `:exports` axis (handled in the
        // `RenderEphemeralLifetime → EphemeralLifetime` bridge). Both
        // are `Option<...>` and the existing typed `caixa-tatara`
        // surface declares no `:routing` / `:encapsulates` slot — the
        // canonical "no routing/encapsulation declared" shape carries
        // forward as `None`, the apiserver-side documented default.
        routing: None,
        encapsulates: None,
        suspended: false,
    };

    let mut process = Process::new(caixa.nome.as_str(), spec);
    process.metadata.namespace = Some(inputs.target_namespace.clone());
    Ok(process)
}

/// Render a `Caixa` to YAML bytes (Process wire format).
pub fn process_yaml(caixa: &Caixa, inputs: &RenderInputs) -> Result<String> {
    let process = process_for_aplicacao(caixa, inputs)?;
    serde_yaml::to_string(&process).map_err(|e| Error::Serialize(e.to_string()))
}

fn derive_chart_ref(caixa: &Caixa, registry: &str) -> String {
    // caixa-helm publishes the rendered chart as `lareira-<name>` to
    // the supplied registry; we compose the OCI ref through the same
    // canonical `lareira_chart_name` helper every per-Servico
    // renderer consults so the Process resolves the same chart name
    // the publisher pushed under, with no inline prefix-format drift.
    let chart = lareira_chart_name(caixa.nome.as_str());
    format!("oci://{registry}/{chart}")
}

fn default_class() -> Classification {
    Classification {
        point_type: ConvergencePointType::Gate,
        substrate: SubstrateType::Compute,
        horizon: Horizon::default(),
        calm: Default::default(),
        data_classification: DataClassification::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tatara_process::intent::IntentVariant;
    use tatara_process::lifetime::LifetimeVariant;

    fn sample_caixa_src() -> String {
        // Smallest valid Aplicacao caixa.
        r#"
(defcaixa
  :nome "akeyless-attest"
  :kind Aplicacao
  :versao "0.1.0"
  :membros ())
"#
        .to_string()
    }

    fn sample_inputs() -> RenderInputs {
        RenderInputs {
            registry: "ghcr.io/pleme-io/charts".into(),
            profile: "gateway-with-internal-saas".into(),
            target_namespace: "akeyless-test".into(),
            lifetime: RenderEphemeralLifetime {
                ttl: "1h".into(),
                teardown_policy: TeardownPolicy::OnAttested,
                max_concurrent: 1,
            },
            values_overlay: serde_json::json!({
                "cluster": { "name": "ephemeral-test-01" },
                "compliance": { "overlays": [] }
            }),
            extra_postconditions: vec![Condition {
                kind: ConditionKind::ClosedLoopAuth,
                params: serde_json::json!({
                    "issuer":   { "service": "gator",   "port": 8080 },
                    "consumer": { "service": "gateway", "port": 8000 },
                }),
            }],
        }
    }

    #[test]
    fn renders_process_with_aplicacao_intent_and_ephemeral_lifetime() {
        let caixa = Caixa::from_lisp(&sample_caixa_src()).expect("parse caixa");
        let process = process_for_aplicacao(&caixa, &sample_inputs()).expect("render");

        // Name + namespace landed.
        assert_eq!(process.metadata.name.as_deref(), Some("akeyless-attest"));
        assert_eq!(process.metadata.namespace.as_deref(), Some("akeyless-test"));

        // Intent::Aplicacao resolves with correct chart_ref shape.
        match process.spec.intent.variant().expect("intent") {
            IntentVariant::Aplicacao(a) => {
                assert_eq!(
                    a.chart_ref,
                    "oci://ghcr.io/pleme-io/charts/lareira-akeyless-attest"
                );
                assert_eq!(a.version, "0.1.0");
                assert_eq!(a.profile, "gateway-with-internal-saas");
                assert_eq!(a.release_name.as_deref(), Some("lareira-akeyless-attest"));
                assert_eq!(a.target_namespace.as_deref(), Some("akeyless-test"));
                assert_eq!(a.install_timeout.as_deref(), Some("25m"));
                // Values overlay preserved.
                assert_eq!(a.values_overlay["cluster"]["name"], "ephemeral-test-01");
            }
            other => panic!("expected Aplicacao, got {other:?}"),
        }

        // Lifetime::Ephemeral resolves with operator-supplied knobs.
        match process.spec.lifetime.variant().expect("lifetime") {
            LifetimeVariant::Ephemeral(e) => {
                assert_eq!(e.ttl, "1h");
                assert_eq!(e.teardown_policy, TeardownPolicy::OnAttested);
                assert_eq!(e.max_concurrent, 1);
            }
            other => panic!("expected Ephemeral, got {other:?}"),
        }

        // Postconditions: HelmReleaseReleased auto + ClosedLoopAuth extra.
        assert_eq!(process.spec.boundary.postconditions.len(), 2);
        assert_eq!(
            process.spec.boundary.postconditions[0].kind,
            ConditionKind::HelmReleaseReleased
        );
        assert_eq!(
            process.spec.boundary.postconditions[1].kind,
            ConditionKind::ClosedLoopAuth
        );
    }

    #[test]
    fn yaml_serialization_round_trip() {
        let caixa = Caixa::from_lisp(&sample_caixa_src()).unwrap();
        let yaml = process_yaml(&caixa, &sample_inputs()).expect("render yaml");
        // Sanity: typed-emitted YAML carries the canonical fields.
        assert!(yaml.contains("apiVersion: tatara.pleme.io/v1alpha1"));
        assert!(yaml.contains("kind: Process"));
        assert!(yaml.contains("aplicacao:"));
        assert!(yaml.contains("ephemeral:"));
        assert!(yaml.contains("ClosedLoopAuth"));
    }

    #[test]
    fn rejects_non_aplicacao_kind() {
        // A Biblioteca caixa is the simplest non-Aplicacao kind to
        // construct.
        let src = r#"
(defcaixa
  :nome "lib"
  :kind Biblioteca
  :versao "0.1.0"
  :bibliotecas ())
"#;
        let caixa = Caixa::from_lisp(src).expect("parse biblioteca");
        let err = process_for_aplicacao(&caixa, &sample_inputs()).unwrap_err();
        assert!(matches!(err, Error::NotAnAplicacao(CaixaKind::Biblioteca)));
    }

    #[test]
    fn lifetime_from_impl_preserves_knobs() {
        let r = RenderEphemeralLifetime {
            ttl: "30m".into(),
            teardown_policy: TeardownPolicy::Always,
            max_concurrent: 4,
        };
        let e: EphemeralLifetime = r.into();
        assert_eq!(e.ttl, "30m");
        assert_eq!(e.teardown_policy, TeardownPolicy::Always);
        assert_eq!(e.max_concurrent, 4);
    }

    #[test]
    fn deterministic_rendering() {
        let caixa = Caixa::from_lisp(&sample_caixa_src()).unwrap();
        let a = process_yaml(&caixa, &sample_inputs()).unwrap();
        let b = process_yaml(&caixa, &sample_inputs()).unwrap();
        assert_eq!(a, b, "renderer must be deterministic");
    }
}
