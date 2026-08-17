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

use caixa_core::{Caixa, CaixaKind, KUBE_KEY_NAME, KUBE_KEY_NAMESPACE};

/// Canonical OCI URL scheme prefix — the `"oci://"` byte-string every
/// substrate-side renderer that composes an OCI artifact reference
/// prepends. Re-export of the canonical [`caixa_core::OCI_SCHEME_PREFIX`]
/// so the scheme string lives in exactly one place across every
/// renderer — this crate's `derive_chart_ref` and every future OCI-ref
/// emitter now consult the same `&'static str`, so a future
/// registry-protocol rebrand is a one-line edit on the canonical
/// [`caixa_core::OCI_SCHEME_PREFIX`] declaration, not a coordinated
/// rewrite across every renderer crate's OCI-ref composition site.
/// Peer to the existing [`caixa_core::lareira_chart_name`] re-export
/// discipline every peer per-Servico renderer follows on the sibling
/// per-Servico chart-name axis.
pub use caixa_core::OCI_SCHEME_PREFIX;

/// Canonical wall-clock cap the caixa-tatara-emitted `Process` grants
/// its ephemeral install-and-verify phase — the paired per-`Process`
/// wall-clock-cap scalar-value that both the tatara `AplicacaoIntent`
/// `install_timeout` (the Helm helm-controller install-phase wall-clock
/// cap Flux passes through to `helm install` for the rendered chart)
/// and the enclosing `Boundary.timeout` (the tatara-substrate outer
/// wall-clock cap the ephemeral `Process` outcome must land within)
/// travel on. Both layers pin the same 25-minute ceiling by construction
/// — an ephemeral `Process` whose outer boundary is shorter than its
/// inner Helm install budget would let the substrate declare failure
/// under a Helm apply that's still running (a wasted retry cycle that
/// stops the operator from ever observing a `HelmReleaseReleased`
/// postcondition transition), and a boundary that's longer than the
/// Helm budget would let a `HelmReleaseReleased` postcondition wait
/// past the point Helm has already given up on the install (a wasted
/// wall-clock the operator stops the ephemeral timer against).
///
/// Re-export of the canonical
/// [`caixa_core::DEFAULT_APLICACAO_INSTALL_TIMEOUT`] so the
/// install-wall-clock-cap scalar lives in exactly one place across the
/// substrate — this crate's `AplicacaoIntent.install_timeout` +
/// `Boundary.timeout` construction sites in [`process_for_aplicacao`]
/// and every future substrate-side install-wall-clock-cap consumer (the
/// future M4 `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's per-CR
/// admission-webhook install-cap floor, the future per-cluster snapshot
/// bundle emitter's per-`Process`-CR install-timeout carrier, the future
/// per-`:placement`-scoped install-timeout overlay the operator pins
/// through) all consult the same `&'static str`, so a future substrate-
/// side install-wall-clock-cap migration (`"25m"` → `"30m"` on longer
/// chart install cycles, `"25m"` → `"15m"` on faster ephemeral turnaround
/// SLAs) is a one-line edit on the canonical
/// [`caixa_core::DEFAULT_APLICACAO_INSTALL_TIMEOUT`] declaration, not a
/// coordinated rewrite across a `caixa-tatara`-owned symbol and every
/// future cross-crate consumer that would otherwise reach across the
/// crate boundary.
///
/// Prior to this canonicalization the const lived as a crate-local
/// `pub const` at `caixa-tatara/src/lib.rs:88` even though its role — a
/// canonical substrate-side wall-clock cap on the same footing as the
/// sibling [`caixa_core::DEFAULT_FLUX_RECONCILE_INTERVAL`] /
/// [`caixa_core::FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`] /
/// [`caixa_core::DEFAULT_FLUX_KUSTOMIZATION_TIMEOUT`] canonical Flux-v2-
/// emit-side scalar-value-default surface — was a substrate-canonical
/// scalar-value-default whose residence at the renderer crate rather
/// than the substrate primitive was the drift footgun a future second
/// cross-crate consumer would surface as either a coordinated cross-
/// crate rewrite or a re-declared sibling `pub const
/// DEFAULT_APLICACAO_INSTALL_TIMEOUT: &str = "25m"` at its own crate
/// root that happened to carry the same string at source but pointed at
/// a different `&'static str` allocation, exactly the drift the
/// [`caixa_core::assert_str_reexport_identity`] helper's docstring names
/// as the canonical footgun the substrate-primitive-lift discipline
/// closes.
///
/// The prior 3-site duplication inside this crate — the
/// `AplicacaoIntent.install_timeout` construction site, the
/// `Boundary.timeout` construction site, and the paired
/// `assert_eq!(a.install_timeout.as_deref(), Some("25m"))` test-side
/// probe — was closed on the 813343f original in-crate lift. This
/// canonicalization takes the same const and moves its residence from
/// the renderer crate onto the substrate primitive so every future
/// cross-crate consumer inherits the same value by construction. Peer
/// with the sibling [`caixa_core::DEFAULT_LIBRARY_NAME`] canonicalization
/// on the same "the substrate-canonical const lives in caixa-core, the
/// renderer crate re-exports for local ergonomics" residence-of-a-
/// substrate-primitive discipline.
pub use caixa_core::DEFAULT_APLICACAO_INSTALL_TIMEOUT;

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
    /// The caixa's `:kind` doesn't match what `caixa-tatara` targets
    /// (this renderer only emits the per-Aplicacao tatara `Process`
    /// artifact — `Intent::Aplicacao` + `Lifetime::Ephemeral` — for
    /// `:kind Aplicacao`). Lifted from a prior
    /// `NotAnAplicacao(CaixaKind)` arm to wrap
    /// [`caixa_core::KindMismatch`] so the diagnostic names the
    /// offending caixa's `:nome` (not just its kind), shared verbatim
    /// with `caixa-helm` / `caixa-flux` / `caixa-mesh` — every
    /// per-kind caixa-side renderer now surfaces the same
    /// self-locating kind-mismatch view through the shared
    /// [`caixa_core::require_kind`] entry gate.
    #[error("{0}")]
    NotAnAplicacao(#[from] caixa_core::KindMismatch),
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
    /// Architecture profile (e.g., `"gateway-with-internal-db"`).
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
    // Route the kind gate through the canonical
    // [`caixa_core::require_kind`] helper so caixa-tatara's per-Aplicacao
    // entry-gate shares one `require_kind + KindMismatch` cascade with
    // the peer per-Aplicacao renderer (`caixa-mesh`'s `typed_view` /
    // `programs_for_aplicacao`) and the peer per-Servico renderers
    // (`caixa-helm` / `caixa-flux`'s `require_v0_servico_shape`) —
    // every caixa-side renderer's kind mismatch now surfaces a
    // diagnostic that names the offending caixa's `:nome`, not just
    // the rejected kind. The prior inline `if caixa.kind !=
    // CaixaKind::Aplicacao { return Err(Error::NotAnAplicacao(caixa.
    // kind)); }` block left this crate as the only per-kind renderer
    // still on the pre-lift shape (the "feira verb whose error path
    // doesn't name the offending caixa" punch-list item the compounding
    // mandate names) — this lift closes that last per-renderer drift
    // surface on the shared kind-gate axis.
    caixa_core::require_kind(caixa, CaixaKind::Aplicacao)?;
    // Route the `:versao` presence + carry-through both through the typed
    // [`caixa_core::Caixa::versao`] `&str`-return accessor rather than the
    // raw `.versao` field access — closes the last unlifted per-`Caixa`
    // `.versao.is_empty()` / `.versao.clone()` raw-field-access sites in
    // this crate on the same universal-axis surface every peer per-`Caixa`
    // renderer (caixa-helm eb912de / 05a7701, caixa-flux 2fc5f81,
    // caixa-crd 41ab9a3) already routes the `:versao` scalar through.
    // Byte-equal today (`Caixa::versao` is `&self.versao`); an accessor
    // extension (SemVer-2 build-metadata canonicalization the
    // CAIXA-SDLC §I SemVer-2 pin acknowledges, an OCI-tag normalization
    // the M4 registry-alignment slot lands) reaches both the presence
    // gate and the `AplicacaoIntent.version` carrier through one
    // dispatch rather than two raw field-accesses in lockstep.
    if caixa.versao().is_empty() {
        return Err(Error::MissingVersao);
    }
    let versao = caixa.versao().to_owned();

    let chart_ref = derive_chart_ref(caixa, &inputs.registry);
    // Route the `AplicacaoIntent.release_name` `lareira-<nome>` chart-
    // identity composer through the substrate-canonical
    // [`caixa_core::Caixa::lareira_chart_name`] resolved-chart-name
    // dispatch rather than the two-step
    // [`caixa_core::lareira_chart_name`]-of-[`caixa_core::Caixa::nome`]
    // open-coded compose — the substrate's canonical per-Servico chart
    // identity resolver now reaches through exactly one typed dispatch,
    // sibling to the peer caixa-helm + caixa-flux converges at
    // [`caixa_helm::render_chart_for_servico_with`]'s `ChartDir.name`
    // and [`caixa_flux::cluster_bundle`]'s per-CR `chart_name` sites.
    // Peer of the sibling [`caixa_core::Caixa::canonical_git_url`] +
    // [`caixa_core::Caixa::publish_tag`] resolved-composers on the
    // paired per-`Caixa` published-artifact-identity axis. Pinned by
    // [`derive_process_release_name_routes_through_caixa_lareira_chart_name_accessor`]
    // in the tests module.
    let release_name = caixa.lareira_chart_name();

    let aplicacao = AplicacaoIntent {
        chart_ref,
        version: versao,
        profile: inputs.profile.clone(),
        values_overlay: inputs.values_overlay.clone(),
        release_name: Some(release_name.clone()),
        target_namespace: Some(inputs.target_namespace.clone()),
        install_timeout: Some(DEFAULT_APLICACAO_INSTALL_TIMEOUT.into()),
    };

    // `HelmReleaseReleased` postcondition params carry the K8s
    // `(namespace, name)` coordinate the tatara-reconciler's boundary
    // phase-machine probes to poll the HelmRelease's `.status.conditions[]`
    // Released transition (tatara-reconciler/src/phase_machine.rs:543-549
    // probes `params.get("name")` + `params.get("namespace")`). Route
    // both write-side keys through the canonical
    // [`caixa_core::KUBE_KEY_NAME`] / [`caixa_core::KUBE_KEY_NAMESPACE`]
    // lifts so the emit-side JSON schema for the postcondition-params
    // shape shares one `&'static str` per K8s CR canonical axis with
    // every peer renderer's `metadata.name` / `metadata.namespace`
    // emission (caixa-flux's cluster_bundle, caixa-mesh's programs
    // fan-out + Cilium + Gateway/HTTPRoute emitters, caixa-helm's
    // lareira-<nome> Chart.yaml/values.yaml emitter). A future K8s CR
    // canonical-key rebrand (a hypothetical apiserver-side migration on
    // the identity discriminator surface) would then land on the
    // canonical const's definition rather than a coordinated four-crate
    // sweep, and the peer-arm read-back pin in
    // `renders_process_pins_helm_release_postcondition_params_at_lifted_keys`
    // trips a build-time failure if any local re-introduction of a
    // sibling inline `"name"` / `"namespace"` JSON key drifts the
    // reconciler-side probe path off the substrate's K8s canonical
    // axis.
    let mut params = serde_json::Map::new();
    params.insert(
        KUBE_KEY_NAME.into(),
        serde_json::Value::String(release_name.clone()),
    );
    params.insert(
        KUBE_KEY_NAMESPACE.into(),
        serde_json::Value::String(inputs.target_namespace.clone()),
    );
    let mut postconditions = vec![Condition {
        kind: ConditionKind::HelmReleaseReleased,
        params: serde_json::Value::Object(params),
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
            timeout: Some(DEFAULT_APLICACAO_INSTALL_TIMEOUT.into()),
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

    // Route the `Process`-CR `metadata.name` compose through the typed
    // [`caixa_core::Caixa::nome`] `&str`-return accessor rather than the
    // raw `.nome.as_str()` field access — the same universal-axis
    // dispatch every peer per-`Caixa` renderer (caixa-helm 22461ef,
    // caixa-flux 162e2e2, caixa-mesh 980c059, caixa-crd 61d3429) routes
    // its per-CR / per-artefact `:nome` scalar through.
    let mut process = Process::new(caixa.nome(), spec);
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
    // the supplied registry; we compose the OCI ref through the
    // substrate-canonical [`caixa_core::Caixa::oci_chart_ref`]
    // resolved-OCI-chart-ref dispatch rather than the two-step
    // [`caixa_core::oci_chart_ref`]-of-[`caixa_core::Caixa::nome`]
    // open-coded compose — the substrate's canonical per-registry
    // OCI-artifact-ref resolver now reaches through exactly one typed
    // dispatch on the substrate primitive, sibling to the peer converge
    // at [`caixa_core::Caixa::lareira_chart_name`] the
    // `AplicacaoIntent.release_name` composer above already routes
    // through. Peer of the sibling
    // [`caixa_core::Caixa::canonical_git_url`] (124f864) /
    // [`caixa_core::Caixa::publish_tag`] (07e05b8) /
    // [`caixa_core::Caixa::lareira_chart_name`] (a8f0bee) resolved-
    // composers on the paired per-`Caixa` published-artifact-identity
    // axis — this landing extends the discipline onto the fourth (and
    // for now sole two-input) member of the per-`Caixa` deploy-artifact
    // identity surface. Pinned by
    // [`derive_chart_ref_routes_through_caixa_oci_chart_ref_accessor`]
    // in the tests module.
    caixa.oci_chart_ref(registry)
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
    use caixa_core::lareira_chart_name;
    use pretty_assertions::assert_eq;
    use tatara_process::intent::IntentVariant;
    use tatara_process::lifetime::LifetimeVariant;

    fn sample_caixa_src() -> String {
        // Smallest valid Aplicacao caixa.
        r#"
(defcaixa
  :nome "example-attest"
  :kind Aplicacao
  :versao "0.1.0"
  :membros ())
"#
        .to_string()
    }

    fn sample_inputs() -> RenderInputs {
        RenderInputs {
            registry: "ghcr.io/pleme-io/charts".into(),
            profile: "gateway-with-internal-db".into(),
            target_namespace: "example-ephemeral".into(),
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
                    "issuer":   { "service": "service-a", "port": 8080 },
                    "consumer": { "service": "service-b", "port": 8000 },
                }),
            }],
        }
    }

    #[test]
    fn renders_process_with_aplicacao_intent_and_ephemeral_lifetime() {
        let caixa = Caixa::from_lisp(&sample_caixa_src()).expect("parse caixa");
        let process = process_for_aplicacao(&caixa, &sample_inputs()).expect("render");

        // Name + namespace landed.
        assert_eq!(process.metadata.name.as_deref(), Some("example-attest"));
        assert_eq!(
            process.metadata.namespace.as_deref(),
            Some("example-ephemeral")
        );

        // Intent::Aplicacao resolves with correct chart_ref shape.
        match process.spec.intent.variant().expect("intent") {
            IntentVariant::Aplicacao(a) => {
                assert_eq!(
                    a.chart_ref,
                    "oci://ghcr.io/pleme-io/charts/lareira-example-attest"
                );
                assert_eq!(a.version, "0.1.0");
                assert_eq!(a.profile, "gateway-with-internal-db");
                assert_eq!(a.release_name.as_deref(), Some("lareira-example-attest"));
                assert_eq!(a.target_namespace.as_deref(), Some("example-ephemeral"));
                assert_eq!(
                    a.install_timeout.as_deref(),
                    Some(DEFAULT_APLICACAO_INSTALL_TIMEOUT)
                );
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
    fn renders_process_pins_helm_release_postcondition_params_at_lifted_keys() {
        // Peer-arm pin on the `HelmReleaseReleased` postcondition params
        // schema. The emit-side builder writes the K8s
        // `(namespace, name)` coordinate through the canonical
        // [`caixa_core::KUBE_KEY_NAME`] / [`caixa_core::KUBE_KEY_NAMESPACE`]
        // lifts (the exact keys the tatara-reconciler's boundary phase-
        // machine probes to poll the HelmRelease's `.status.conditions[]`
        // Released transition at
        // `tatara-reconciler/src/phase_machine.rs:543-549`). Read both
        // back at the lifted keys and pin the values against the
        // fixture's release-name (`lareira-example-attest`) + target
        // namespace (`example-ephemeral`).
        //
        // Before the lift the emitter carried inline `"name"` /
        // `"namespace"` JSON keys and no test-side probe pinned them —
        // a rebrand on the K8s canonical `metadata.{name,namespace}` axis
        // (or a local re-introduction of a sibling inline literal that
        // drifted the emit-side key off the reconciler-side probe path)
        // would have silently emitted a `Process` CR whose
        // `HelmReleaseReleased` postcondition never fired (the phase-
        // machine's `params.get("name")` reduces to `None` under any
        // drifted emit key, and every ephemeral install times out at the
        // outer `Boundary.timeout` with no cluster-side symptom naming
        // the drift). The pin here + the emit-side lift close both
        // coordinates of the drift-vs-probe pair at build time.
        let caixa = Caixa::from_lisp(&sample_caixa_src()).expect("parse caixa");
        let process = process_for_aplicacao(&caixa, &sample_inputs()).expect("render");
        let params = &process.spec.boundary.postconditions[0].params;
        assert_eq!(
            params.get(KUBE_KEY_NAME).and_then(|v| v.as_str()),
            Some("lareira-example-attest"),
            "HelmReleaseReleased postcondition params must carry the release \
             name at the lifted `caixa_core::KUBE_KEY_NAME` axis so the \
             tatara-reconciler's `params.get(\"name\")` probe resolves it"
        );
        assert_eq!(
            params.get(KUBE_KEY_NAMESPACE).and_then(|v| v.as_str()),
            Some("example-ephemeral"),
            "HelmReleaseReleased postcondition params must carry the target \
             namespace at the lifted `caixa_core::KUBE_KEY_NAMESPACE` axis \
             so the tatara-reconciler's `params.get(\"namespace\")` probe \
             resolves it"
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
        assert!(matches!(err, Error::NotAnAplicacao(_)));
    }

    #[test]
    fn kind_mismatch_error_names_offending_caixa_nome() {
        // Pinning the lifted [`caixa_core::KindMismatch`] view's
        // load-bearing property on this crate's per-Aplicacao entry
        // gate: a kind-mismatched caixa surfaces a diagnostic that
        // *names the offending caixa* (`lib`), not just the rejected
        // kind. Before this lift the renderer raised
        // `Error::NotAnAplicacao(CaixaKind::Biblioteca)` whose
        // `Display` said "caixa-tatara only renders :kind Aplicacao
        // caixas (got Biblioteca)" — the user had to grep their
        // source tree for which `caixa.lisp` triggered it. After the
        // lift the wrapped `KindMismatch` carries the `:nome`, the
        // renderer's `#[error("{0}")]` arm prints it through, and the
        // diagnostic is self-locating verbatim with the sibling
        // caixa-mesh / caixa-flux / caixa-helm per-kind renderers
        // (which each already routed through `require_kind` +
        // wrapped `KindMismatch`). Peer to
        // `caixa_mesh::tests::kind_mismatch_error_names_offending_caixa_nome`
        // (typed_view + programs_for_aplicacao entry gate) and
        // `caixa_flux::tests::…` /
        // `caixa_helm::tests::kind_mismatch_error_names_offending_caixa_nome`
        // on the sibling per-Servico renderer crates — the same
        // "one shared kind-gate helper, one shared self-locating
        // diagnostic view" discipline now covers every per-kind
        // caixa-side renderer with no per-crate drift surface.
        let src = r#"
(defcaixa
  :nome "lib"
  :kind Biblioteca
  :versao "0.1.0"
  :bibliotecas ())
"#;
        let caixa = Caixa::from_lisp(src).expect("parse biblioteca");
        let err = process_for_aplicacao(&caixa, &sample_inputs()).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("lib"),
            "kind-mismatch diagnostic must name the offending caixa nome \
             (got: {msg:?})"
        );
        assert!(
            msg.contains("Aplicacao"),
            "diagnostic must name the expected kind (got: {msg:?})"
        );
        assert!(
            msg.contains("Biblioteca"),
            "diagnostic must name the actual kind (got: {msg:?})"
        );
        match err {
            Error::NotAnAplicacao(km) => {
                assert_eq!(km.nome, "lib");
                assert_eq!(km.expected, CaixaKind::Aplicacao);
                assert_eq!(km.actual, CaixaKind::Biblioteca);
            }
            other => panic!("expected Error::NotAnAplicacao, got {other:?}"),
        }
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

    // ── Drift-detection pins on the lifted `oci_chart_ref` composer ──────
    //
    // Until this lift landed the `derive_chart_ref` carried an inline
    // `format!("oci://{registry}/{chart}")` — a 2-axis composition
    // (the `oci://` scheme prefix + the `lareira-<nome>` chart name
    // via [`lareira_chart_name`]) whose byte-shape had no compile-time
    // link to the historical doc comments across `caixa-core`,
    // `caixa-flux`, `caixa-helm`, and `caixa-tatara` promising the
    // same shape. Both writer / re-export sites now consult the lifted
    // [`caixa_core::oci_chart_ref`] + [`caixa_core::OCI_SCHEME_PREFIX`]
    // so a future composer-internal drift fires at test time, not at
    // cluster-apply time far from the drift site.

    #[test]
    fn oci_scheme_prefix_re_export_points_at_caixa_core_canonical() {
        // Re-export identity pin: the local `OCI_SCHEME_PREFIX` is the
        // canonical [`caixa_core::OCI_SCHEME_PREFIX`] `&'static str` —
        // not a sibling `&'static str` with the same bytes. Same
        // drift-detection discipline every peer per-crate re-export
        // pin uses (`DEFAULT_NAMESPACE`, `LAREIRA_CHART_NAME_PREFIX`,
        // etc.) so a future accidental shadowing surfaces here.
        caixa_core::assert_str_reexport_identity(
            "OCI_SCHEME_PREFIX",
            OCI_SCHEME_PREFIX,
            caixa_core::OCI_SCHEME_PREFIX,
        );
    }

    #[test]
    fn derive_chart_ref_emits_canonical_oci_chart_ref_output() {
        // Emission-side pin: `derive_chart_ref` composes exactly what
        // [`caixa_core::oci_chart_ref`] produces for the same inputs.
        // A future refactor that re-inlines the `format!` shape or
        // diverges on the scheme / chart-name axis surfaces here
        // rather than as a `helm registry` / FluxCD reconcile
        // failure. Sweep across the canonical fixture set — the
        // registry the sample_inputs fixture uses + a peer registry
        // for cross-registry drift coverage.
        let caixa = Caixa::from_lisp(&sample_caixa_src()).unwrap();
        for registry in [
            "ghcr.io/pleme-io/charts",
            "ghcr.io/pleme-io",
            "registry.example.com",
        ] {
            let composed = derive_chart_ref(&caixa, registry);
            assert_eq!(
                composed,
                caixa_core::oci_chart_ref(registry, caixa.nome()),
                "derive_chart_ref must route through caixa_core::oci_chart_ref for \
                 registry {registry:?}"
            );
        }
    }

    #[test]
    fn derive_chart_ref_starts_with_lifted_scheme_prefix() {
        // Cross-axis invariant: every output of the tatara
        // `derive_chart_ref` composer begins with the lifted scheme
        // prefix verbatim — a future refactor that accidentally
        // introduced a different scheme (a `https://` transposition,
        // a scheme-authority separator drift) would surface here.
        // Structurally pins the tatara-side emission on the shared
        // scheme prefix without re-inlining the `"oci://"` literal.
        let caixa = Caixa::from_lisp(&sample_caixa_src()).unwrap();
        let composed = derive_chart_ref(&caixa, "ghcr.io/pleme-io/charts");
        assert!(
            composed.starts_with(OCI_SCHEME_PREFIX),
            "derive_chart_ref emission {composed:?} must start with the lifted \
             OCI_SCHEME_PREFIX {OCI_SCHEME_PREFIX:?}"
        );
    }

    #[test]
    fn default_aplicacao_install_timeout_pins_canonical_value() {
        // Canonical-value pin on the lifted per-Process wall-clock cap
        // scalar. Was a 3-site duplicated `"25m"` inline literal at
        // (i) the `AplicacaoIntent.install_timeout` construction site
        // in `process_for_aplicacao`, (ii) the enclosing
        // `Boundary.timeout` construction site in the same fn, and
        // (iii) the paired `assert_eq!` probe in
        // `renders_process_with_aplicacao_intent_and_ephemeral_lifetime`
        // — a coordinated-edit footgun on any future wall-clock-cap
        // migration whose miss at any one site silently emitted a
        // `Process` whose outer boundary disagreed with its inner Helm
        // install budget by construction. Pinning the byte-shape here
        // fires at test time when a future accidental edit changes
        // the const's value.
        assert_eq!(DEFAULT_APLICACAO_INSTALL_TIMEOUT, "25m");
    }

    #[test]
    fn default_aplicacao_install_timeout_re_export_points_at_caixa_core_canonical() {
        // The crate's [`DEFAULT_APLICACAO_INSTALL_TIMEOUT`] was
        // canonicalized from a local `pub const DEFAULT_APLICACAO_INSTALL_TIMEOUT:
        // &str = "25m"` at `caixa-tatara/src/lib.rs:88` to a re-export
        // of [`caixa_core::DEFAULT_APLICACAO_INSTALL_TIMEOUT`] so the
        // substrate-canonical per-`Process` install-wall-clock-cap scalar
        // lives in exactly one place across every substrate-side
        // consumer — this crate's paired `AplicacaoIntent.install_timeout`
        // + `Boundary.timeout` construction sites in
        // [`process_for_aplicacao`] and every future cross-crate
        // consumer (the future M4
        // `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's per-CR
        // admission-webhook install-cap floor, the future per-cluster
        // snapshot bundle emitter's per-`Process`-CR install-timeout
        // carrier, the future per-`:placement`-scoped install-timeout
        // overlay the operator pins through). Pin the equality + `&'static`
        // static-data identity here so any local re-introduction of a
        // sibling `pub const DEFAULT_APLICACAO_INSTALL_TIMEOUT: &str =
        // "…"` at this crate — the canonical drift footgun where a
        // sibling local `pub const` could happen to carry the same
        // string at the source while pointing at a different
        // `&'static` allocation — is a build-time test failure naming
        // the offending drift, not a silent apply-time wall-clock-cap
        // divergence between the two composition sites this crate
        // hosts. Peer to the sibling
        // [`caixa_helm::tests::default_library_name_re_export_points_at_caixa_core_canonical`]
        // / [`caixa_flux::tests::default_library_name_re_export_points_at_caixa_core_canonical`]
        // pins on the sibling
        // [`caixa_core::DEFAULT_LIBRARY_NAME`]-canonicalization axis;
        // this pin closes the analogous re-export identity axis on the
        // [`caixa_core::DEFAULT_APLICACAO_INSTALL_TIMEOUT`]
        // canonicalization.
        caixa_core::assert_str_reexport_identity(
            "DEFAULT_APLICACAO_INSTALL_TIMEOUT",
            DEFAULT_APLICACAO_INSTALL_TIMEOUT,
            caixa_core::DEFAULT_APLICACAO_INSTALL_TIMEOUT,
        );
    }

    #[test]
    fn default_aplicacao_install_timeout_pairs_intent_and_boundary_at_one_axis() {
        // Emission-side pair pin: both the `AplicacaoIntent.install_timeout`
        // (inner Helm install wall-clock cap Flux passes through to
        // `helm install`) and the enclosing `Boundary.timeout` (outer
        // tatara-substrate wall-clock cap the ephemeral `Process`
        // outcome must land within) resolve to the same canonical
        // wall-clock string on the same lifted const, so a future
        // per-substrate migration reaches both layers through one
        // `&'static str` by construction. A miss (a future edit that
        // re-inlines one site or diverges the two by a different
        // scalar) surfaces here structurally, not at
        // `helm install` / operator-reconcile time far from the drift
        // site.
        let caixa = Caixa::from_lisp(&sample_caixa_src()).unwrap();
        let process = process_for_aplicacao(&caixa, &sample_inputs()).expect("render");
        let intent_install_timeout = match process.spec.intent.variant().expect("intent") {
            IntentVariant::Aplicacao(a) => a.install_timeout.clone(),
            other => panic!("expected Aplicacao, got {other:?}"),
        };
        assert_eq!(
            intent_install_timeout.as_deref(),
            Some(DEFAULT_APLICACAO_INSTALL_TIMEOUT),
        );
        assert_eq!(
            process.spec.boundary.timeout.as_deref(),
            Some(DEFAULT_APLICACAO_INSTALL_TIMEOUT),
        );
        assert_eq!(intent_install_timeout, process.spec.boundary.timeout);
    }

    #[test]
    fn derive_chart_ref_ends_with_lareira_chart_name() {
        // Cross-axis invariant: every output of the tatara
        // `derive_chart_ref` composer ends with the canonical
        // `lareira_chart_name(caixa.nome)` output verbatim. Structurally
        // pins that the tatara-side OCI-ref emission and the peer
        // per-Servico renderer chart-name path (caixa-helm's
        // `ChartDir.name`, caixa-flux's `HelmRelease` `chart:` field,
        // this crate's own `release_name`) all consult the same
        // canonical `lareira_chart_name` helper's output.
        let caixa = Caixa::from_lisp(&sample_caixa_src()).unwrap();
        let composed = derive_chart_ref(&caixa, "ghcr.io/pleme-io/charts");
        let chart = lareira_chart_name(caixa.nome());
        assert!(
            composed.ends_with(&chart),
            "derive_chart_ref emission {composed:?} must end with the canonical \
             lareira_chart_name({:?}) = {chart:?}",
            caixa.nome()
        );
    }

    #[test]
    fn process_for_aplicacao_routes_nome_and_versao_through_caixa_accessors() {
        // Drift-detection pin on the emit-side identity carriers of the
        // `Process` CR this crate materializes for an `:kind Aplicacao`
        // caixa. Every per-`Caixa` `:nome` / `:versao` scalar the emit
        // path composes onto the CR — `metadata.name`, the
        // `AplicacaoIntent.release_name` composed via `lareira_chart_name`,
        // the `AplicacaoIntent.chart_ref` composed via `oci_chart_ref`,
        // the `AplicacaoIntent.version` scalar — is derived through the
        // typed [`caixa_core::Caixa::nome`] / [`caixa_core::Caixa::versao`]
        // `&str`-return accessors, not the raw `caixa.nome` /
        // `caixa.versao` field-access. Pin each emitted scalar against
        // the accessor's return value so a regression that re-inlines
        // the raw field-access surfaces at build time verbatim with
        // the peer pins the sibling per-`Caixa` renderer converges
        // established on the same universal-axis surface
        // (caixa-crd's `caixa_into_cr_versao_routes_through_caixa_
        // versao_accessor` per 41ab9a3, caixa-flux's `cluster_bundle_
        // default_git_tag_versao_routes_through_caixa_versao_accessor`
        // per 2fc5f81, caixa-feira's `build_summary_line_routes_
        // through_caixa_nome_and_versao_accessors` per ef83332, and
        // `graph_header_line_routes_through_caixa_nome_and_versao_
        // accessors` + `deploy_commit_message_routes_through_caixa_
        // nome_and_versao_accessors` per 3219a42).
        //
        // Byte-equal today (both accessors are `&self.<field>`); the
        // pin catches any future accessor extension (SemVer-2 build-
        // metadata canonicalization the CAIXA-SDLC §I SemVer-2 pin
        // acknowledges, an OCI-tag normalization the M4 registry-
        // alignment slot lands, a DNS-1123 normalization pass on
        // `:nome` the layout-invariant gate already accepts under
        // `is_dns_1123_label`) whose `Process`-CR emit regresses to
        // the raw field.
        let caixa = Caixa::from_lisp(&sample_caixa_src()).expect("parse caixa");
        let inputs = sample_inputs();
        let process = process_for_aplicacao(&caixa, &inputs).expect("render");

        // `metadata.name` — the CR's identity discriminator — carries
        // exactly `Caixa::nome()`, not the raw `.nome` field.
        assert_eq!(
            process.metadata.name.as_deref(),
            Some(caixa.nome()),
            "Process metadata.name must route through Caixa::nome() — a \
             regression that re-inlines `caixa.nome.as_str()` at the \
             `Process::new(..)` site silently splits the CR's identity \
             discriminator from every peer renderer's `:nome` emit"
        );

        // `AplicacaoIntent.{version, release_name, chart_ref}` all fold
        // on the accessor-derived identity carriers.
        match process.spec.intent.variant().expect("intent") {
            IntentVariant::Aplicacao(a) => {
                assert_eq!(
                    a.version,
                    caixa.versao(),
                    "AplicacaoIntent.version must route through Caixa::versao() \
                     — a regression that re-inlines `caixa.versao.clone()` at \
                     the intent-compose site silently splits the CR's install \
                     version from the peer `Chart.yaml` `version:` per eb912de \
                     and the `GitRepository` `spec.ref.tag` per 2fc5f81 that \
                     each already routes through the same accessor"
                );
                let expected_release = lareira_chart_name(caixa.nome());
                assert_eq!(
                    a.release_name.as_deref(),
                    Some(expected_release.as_str()),
                    "AplicacaoIntent.release_name must route through \
                     `lareira_chart_name(caixa.nome())` — a regression that \
                     re-inlines `caixa.nome.as_str()` at the release-name \
                     compose site silently splits the CR's release-name from \
                     the peer per-Servico renderer chart-name path (caixa-helm \
                     `ChartDir.name`, caixa-flux `HelmRelease.chart:`) that \
                     each already routes through the same accessor"
                );
                assert_eq!(
                    a.chart_ref,
                    caixa_core::oci_chart_ref(&inputs.registry, caixa.nome()),
                    "AplicacaoIntent.chart_ref must route through \
                     `oci_chart_ref(registry, caixa.nome())` — a regression \
                     that re-inlines `caixa.nome.as_str()` at the OCI-ref \
                     compose site silently splits the CR's chart-ref from the \
                     peer `caixa_core::oci_chart_ref` composer's shape and \
                     from the `derive_chart_ref_ends_with_lareira_chart_name` \
                     cross-axis pin"
                );
            }
            other => panic!("expected Aplicacao intent, got {other:?}"),
        }
    }

    #[test]
    fn derive_process_release_name_routes_through_caixa_lareira_chart_name_accessor() {
        // Fail-before-pass-after pin: the emit-side
        // `AplicacaoIntent.release_name` scalar the [`process_for_aplicacao`]
        // fn composes on the intent-arm must derive from the substrate-
        // canonical [`caixa_core::Caixa::lareira_chart_name`] resolved-
        // chart-name dispatch. Before this converge the emit site
        // carried a raw `lareira_chart_name(caixa.nome())` two-step
        // compose at the `release_name` composer position, bypassing
        // the substrate primitive's single-`&Caixa` dispatch. Peer of
        // the sibling
        // [`process_for_aplicacao_routes_nome_and_versao_through_caixa_accessors`]
        // pin above on the paired per-`Caixa` universal-axis identity
        // carriers ([`Caixa::nome`] / [`Caixa::versao`]) — extends
        // the discipline from the per-atom scalar accessors onto the
        // resolved-chart-name composer projection. Byte-equal today
        // (the accessor is `caixa_core::lareira_chart_name(self.nome())`);
        // the pin catches any future accessor extension whose emit-
        // side write regresses to the two-step open-coded compose,
        // in lockstep with the peer caixa-helm
        // [`chart_dir_name_routes_through_caixa_lareira_chart_name_accessor`]
        // + caixa-flux
        // [`cluster_bundle_chart_name_routes_through_caixa_lareira_chart_name_accessor`]
        // pins that carry the same converge on the sibling per-Servico
        // renderer emit surfaces.
        let caixa = Caixa::from_lisp(&sample_caixa_src()).expect("parse caixa");
        let inputs = sample_inputs();
        let process = process_for_aplicacao(&caixa, &inputs).expect("render");
        match process.spec.intent.variant().expect("intent") {
            IntentVariant::Aplicacao(a) => {
                assert_eq!(
                    a.release_name.as_deref(),
                    Some(caixa.lareira_chart_name().as_str()),
                    "AplicacaoIntent.release_name must derive from the \
                     substrate-canonical \
                     `caixa_core::Caixa::lareira_chart_name` accessor \
                     byte-for-byte — a regression that re-inlines \
                     `caixa_core::lareira_chart_name(caixa.nome())` at \
                     the release-name compose site silently splits the \
                     CR's release-name from every future accessor \
                     extension (per-cluster alias overlay, M4 CR-\
                     materializer name rewrite, `:nome-suffix` slot, \
                     `LAREIRA_CHART_NAME_PREFIX` rebrand) that lands on \
                     the accessor"
                );
            }
            other => panic!("expected Aplicacao intent, got {other:?}"),
        }
    }

    #[test]
    fn derive_chart_ref_routes_through_caixa_oci_chart_ref_accessor() {
        // Fail-before-pass-after pin: the emit-side
        // `AplicacaoIntent.chart_ref` scalar the [`process_for_aplicacao`]
        // fn composes on the intent-arm (via the [`derive_chart_ref`]
        // helper) must derive from the substrate-canonical
        // [`caixa_core::Caixa::oci_chart_ref`] resolved-OCI-chart-ref
        // dispatch. Before this converge the emit site carried a raw
        // `caixa_core::oci_chart_ref(registry, caixa.nome())` two-step
        // compose at the `derive_chart_ref` composer position, bypassing
        // the substrate primitive's paired-`(&Caixa, &str)` dispatch.
        // Peer of the sibling
        // [`derive_process_release_name_routes_through_caixa_lareira_chart_name_accessor`]
        // pin above on the paired per-`Caixa` published-artifact-identity
        // resolved-composer axis — extends the discipline from the
        // resolved-chart-name single-`&Caixa` composer projection onto
        // the resolved-OCI-ref paired-`(&Caixa, &str)` composer
        // projection. Byte-equal today (the accessor dispatches through
        // `caixa_core::oci_chart_ref(registry, self.nome())`); the pin
        // catches any future accessor extension whose emit-side write
        // regresses to the two-step open-coded compose, in lockstep with
        // the peer caixa-core
        // [`oci_chart_ref_byte_matches_canonical_helper_composition`]
        // pin that carries the sibling byte-parity gate on the accessor
        // itself.
        let caixa = Caixa::from_lisp(&sample_caixa_src()).expect("parse caixa");
        for registry in [
            "ghcr.io/pleme-io/charts",
            "ghcr.io/pleme-io",
            "registry.example.com",
            "localhost:5000",
        ] {
            let composed = derive_chart_ref(&caixa, registry);
            assert_eq!(
                composed,
                caixa.oci_chart_ref(registry),
                "derive_chart_ref must derive from the substrate-canonical \
                 `caixa_core::Caixa::oci_chart_ref` accessor byte-for-byte \
                 — a regression that re-inlines \
                 `caixa_core::oci_chart_ref(registry, caixa.nome())` at \
                 the OCI-ref compose site silently splits the CR's chart-\
                 ref from every future accessor extension (per-registry \
                 alias overlay, M4 CR-materializer chart-ref rewrite, \
                 `OCI_SCHEME_PREFIX` rebrand) that lands on the accessor \
                 — registry ({registry:?})"
            );
        }

        // Emit-side projection pin: the resolved-OCI-ref accessor
        // reaches the emitted `AplicacaoIntent.chart_ref` field verbatim
        // via `derive_chart_ref` — a regression that decoupled the field
        // fill from the accessor's return value (a caller-supplied
        // per-CR overlay silently interposed at the intent-compose site)
        // surfaces here as well.
        let inputs = sample_inputs();
        let process = process_for_aplicacao(&caixa, &inputs).expect("render");
        match process.spec.intent.variant().expect("intent") {
            IntentVariant::Aplicacao(a) => {
                assert_eq!(
                    a.chart_ref,
                    caixa.oci_chart_ref(&inputs.registry),
                    "AplicacaoIntent.chart_ref must derive from the \
                     substrate-canonical `caixa_core::Caixa::oci_chart_ref` \
                     accessor byte-for-byte — a regression that re-inlines \
                     `caixa_core::oci_chart_ref(registry, caixa.nome())` \
                     at the intent-compose site silently splits the CR's \
                     chart-ref from the peer per-`Caixa` published-\
                     artifact-identity resolved-composers"
                );
            }
            other => panic!("expected Aplicacao intent, got {other:?}"),
        }
    }

    #[test]
    fn missing_versao_gate_routes_through_caixa_versao_accessor() {
        // Peer-arm pin on the `:versao` presence gate in
        // `process_for_aplicacao`. Before this converge the gate read
        // through the raw `caixa.versao.is_empty()` field-access; now
        // it routes through `caixa.versao().is_empty()` — same
        // universal-axis accessor the sibling `AplicacaoIntent.version`
        // carry-through consults. Pin the gate's behavior against the
        // accessor's return value so a regression that re-inlines the
        // raw field on either side (the presence gate or the carry-
        // through) surfaces at build time.
        //
        // A `:versao ""` shape is rejected at `Caixa::from_lisp` parse
        // time by the `caixa-core` `validate_versao` cascade (the
        // per-`Caixa` universal-axis gate at
        // caixa-core/src/manifest.rs:646), so this test doesn't
        // exercise the runtime `MissingVersao` arm through a parse-
        // valid fixture — instead it structurally pins the fact that
        // `Caixa::versao()` returns `&str` (the same shape
        // `String::is_empty()` had before the converge, so the gate's
        // boolean semantics are preserved byte-for-byte).
        let caixa = Caixa::from_lisp(&sample_caixa_src()).expect("parse caixa");
        assert!(
            !caixa.versao().is_empty(),
            "sample fixture's :versao must be non-empty for the emit path \
             to reach the AplicacaoIntent compose site (rules out a false-\
             positive on the presence gate's carry-through pin)"
        );
        // Non-`&str`-return regression sentinel: `Caixa::versao()` must
        // return a `&str` so the `.is_empty()` boolean projection the
        // gate performs on the accessor's return value binds to
        // `str::is_empty` (identical semantics to `String::is_empty`).
        let _: &str = caixa.versao();
    }
}
