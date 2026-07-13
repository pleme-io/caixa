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

use caixa_core::{Caixa, CaixaKind, lareira_chart_name, oci_chart_ref};

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
/// Prior to this lift the same `"25m"` byte-string sat inline at
/// both the `AplicacaoIntent.install_timeout` construction site and
/// the `Boundary.timeout` construction site in [`process_for_aplicacao`],
/// plus the paired `assert_eq!(a.install_timeout.as_deref(), Some("25m"))`
/// test-side probe — a 3-site duplication of the load-bearing wall-clock
/// scalar-value whose byte-shape had no compile-time link. A future
/// per-substrate wall-clock-cap migration (`"25m"` → `"30m"` on longer
/// chart install cycles, `"25m"` → `"15m"` on faster ephemeral turnaround
/// SLAs) would have required a coordinated three-way edit whose miss
/// at any one site silently emitted a `Process` whose outer boundary
/// disagreed with its inner Helm budget by construction. Every consumer
/// now routes through this `&'static str`, so the same migration is a
/// one-line edit on the const.
///
/// Peer to the sibling [`caixa_core::DEFAULT_FLUX_RECONCILE_INTERVAL`]
/// / [`caixa_core::FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`] lifts
/// on the canonical Flux-v2-emit-side scalar-value-default surface —
/// each pinned by a single canonical const the tatara / Flux emitters
/// jointly consult.
pub const DEFAULT_APLICACAO_INSTALL_TIMEOUT: &str = "25m";

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
        install_timeout: Some(DEFAULT_APLICACAO_INSTALL_TIMEOUT.into()),
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
    // canonical [`caixa_core::oci_chart_ref`] composer (which itself
    // reads through [`caixa_core::OCI_SCHEME_PREFIX`] +
    // [`caixa_core::lareira_chart_name`]) so the Process resolves the
    // same chart name the publisher pushed under, with no inline
    // prefix-format drift on either the scheme or the chart-name axis.
    oci_chart_ref(registry, caixa.nome.as_str())
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
                caixa_core::oci_chart_ref(registry, caixa.nome.as_str()),
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
        let chart = lareira_chart_name(caixa.nome.as_str());
        assert!(
            composed.ends_with(&chart),
            "derive_chart_ref emission {composed:?} must end with the canonical \
             lareira_chart_name({:?}) = {chart:?}",
            caixa.nome
        );
    }
}
