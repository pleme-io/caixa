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

use caixa_core::{Caixa, KUBE_KEY_NAME, KUBE_KEY_NAMESPACE};

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
    /// The caixa's `:versao` slot failed the substrate's
    /// [`caixa_core::manifest::Caixa::validate_versao`] two-arm cascade —
    /// either empty ([`caixa_core::ManifestError::VersaoEmpty`], the
    /// canonical "author omitted or blanked out the slot" arm) or not a
    /// valid SemVer-2 version
    /// ([`caixa_core::ManifestError::VersaoInvalid`], the canonical
    /// "author supplied a git-tag-shape / docker-tag-shape / requirement-
    /// shape / four-part-shape byte-string" arm). Wraps
    /// [`caixa_core::ManifestError`] via `#[from]` so the diagnostic
    /// naming the offending byte-string + a parser-shaped reason (in the
    /// invalid arm) shares one typed view with every peer per-`Caixa`
    /// consumer that routes through
    /// [`caixa_core::require_valid_versao`], matching the peer
    /// [`Self::NotAnAplicacao`] `#[from]` shape on the
    /// [`caixa_core::KindMismatch`] view above. Pre-lift this crate
    /// carried an inline `if caixa.versao().is_empty() { return
    /// Err(Error::MissingVersao); }` gate that (a) checked only the
    /// empty arm, silently accepting a SemVer-2-invalid byte-string past
    /// the gate that then landed at the `AplicacaoIntent.version`
    /// carrier and surfaced as a Helm chart-install rejection far from
    /// the source `caixa.lisp`, and (b) surfaced a context-free
    /// `"caixa is missing :versao — required to materialize chart_ref"`
    /// diagnostic with no field naming the offending value — both
    /// closed by routing through the substrate-canonical
    /// [`caixa_core::require_valid_versao`] compound gate + the shared
    /// [`caixa_core::ManifestError`] typed view.
    #[error("{0}")]
    InvalidVersao(#[from] caixa_core::ManifestError),
    /// The caixa's `:kind Aplicacao` typed view failed the substrate's
    /// [`caixa_core::AplicacaoSpec::validate`] cascade or the paired
    /// cross-slot [`caixa_core::aplicacao::validate_no_self_membership`]
    /// self-edge gate — i.e. empty `:membros`, malformed / duplicate
    /// per-`:membros` entry, `:contratos` naming an unknown member or
    /// forming a synchronous cycle, `:placement` missing `:clusters` or
    /// duplicating a cluster entry, `:entrada :para` naming an unknown
    /// member, `:politicas` carrying an operationally-meaningless value
    /// (zero timeout, zero retries, zero breaker thresholds, zero rate
    /// limit — MESH-COMPOSITION §V), or a self-referential Aplicacao
    /// that lists its own `:nome` as a `:membros` entry
    /// ([`caixa_core::AplicacaoError::MembroIsSelfAplicacao`], the
    /// one-node lacre-closure recursion the layout pipeline's
    /// `validate_no_self_membership` cross-slot gate refuses). Wraps
    /// [`caixa_core::AplicacaoError`] via `#[from]` so the diagnostic —
    /// which names the offending Aplicacao / `:membros` entry /
    /// `:contratos` edge / `:placement` cluster / `:entrada` field
    /// verbatim per the substrate's typed-view error surface — shares
    /// one typed view with every peer per-Aplicacao consumer that routes
    /// through [`caixa_core::require_aplicacao_view`] (`caixa-mesh`'s
    /// `typed_view` feeding `programs_for_aplicacao` /
    /// `cilium_network_policies` / `gateway_routes`), matching the peer
    /// [`Self::NotAnAplicacao`] `#[from]` shape on the
    /// [`caixa_core::KindMismatch`] view above and the peer
    /// [`caixa_mesh::Error::InvalidAplicacao`] `#[from]` shape on the
    /// [`caixa_core::AplicacaoError`] view its sibling per-Aplicacao
    /// renderer already carries. Pre-lift this crate's
    /// `process_for_aplicacao` gate was strictly *weaker* than the
    /// substrate's own author-time gate on every axis
    /// [`caixa_core::AplicacaoSpec::validate`] +
    /// [`caixa_core::aplicacao::validate_no_self_membership`] cover: a
    /// `:kind Aplicacao` caixa with empty `:membros`, an unknown
    /// `:contratos` member, a duplicate cluster entry, or a
    /// self-referential `:membros` entry silently rendered a `Process`
    /// CR whose `AplicacaoIntent.chart_ref` pointed at a chart the
    /// resolver could not fully materialize (the empty-`:membros` case
    /// produced a `Process` with no downstream member entries; the
    /// self-referential case produced a one-node lacre-closure recursion
    /// the resolver's traversal either refused far from the source
    /// `caixa.lisp` or exhausted its stack on). Post-lift the tatara
    /// renderer routes the compound four-arm cascade through
    /// [`caixa_core::require_aplicacao_view`] — the same substrate
    /// helper `caixa-mesh`'s `typed_view` already routes through — so
    /// every per-Aplicacao renderer in the substrate accepts the same
    /// set of Aplicacaos at emit time by construction, matching the
    /// author-time [`caixa_core::StandardLayout::verify`] four-step
    /// cascade byte-for-byte.
    #[error("aplicacao typed shape violation: {0}")]
    InvalidAplicacao(#[from] caixa_core::AplicacaoError),
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
    // Route the per-Aplicacao entry gate through the substrate-canonical
    // [`caixa_core::require_aplicacao_view`] compound helper — the same
    // four-arm cascade `caixa-mesh`'s `typed_view` routes through
    // (`require_kind(Aplicacao)` + `Caixa::aplicacao_view` +
    // [`caixa_core::AplicacaoSpec::validate`] +
    // [`caixa_core::aplicacao::validate_no_self_membership`], matching
    // the byte-for-byte four-step cascade
    // [`caixa_core::StandardLayout::verify`] runs on the `feira build`
    // author-time path). Pre-lift this site carried only the two-step
    // `require_kind + require_valid_versao` prelude — strictly *weaker*
    // than the substrate's own author-time gate on every axis
    // [`caixa_core::AplicacaoSpec::validate`] +
    // [`caixa_core::aplicacao::validate_no_self_membership`] cover: a
    // `:kind Aplicacao` caixa with empty `:membros`, an unknown
    // `:contratos` member, a duplicate `:placement` cluster, or a
    // self-referential `:membros` entry rendered a `Process` CR whose
    // `AplicacaoIntent.chart_ref` pointed at a chart the resolver could
    // not fully materialize. Every prior per-Aplicacao renderer's
    // three-arm compound-entry-gate docstring (the 3aefefb "every
    // per-Aplicacao `caixa-<target>` renderer routes through
    // ... caixa-tatara's `process_for_aplicacao`" claim) named this
    // renderer as one of the callers already routed through the
    // compound helper; the code carried the two-step prelude below the
    // claim. This lift closes that comment-vs-code drift structurally
    // and lands the same posture the peer per-Servico
    // [`caixa_core::require_v0_servico_shape`] compound gate
    // (`caixa-helm`, `caixa-flux`) and the sibling per-Aplicacao
    // [`caixa_core::require_aplicacao_view`] compound gate
    // (`caixa-mesh`) already take: one substrate helper per compound
    // gate, one `#[from]` arm on each shared typed view. The returned
    // [`caixa_core::AplicacaoSpec`] is discarded here because the
    // downstream `Process`-CR emit body derives every scalar from
    // per-`Caixa` accessors (`caixa.nome()`, `caixa.lareira_chart_name()`,
    // `caixa.oci_chart_ref(...)`) rather than from the typed view — the
    // helper's return value is a compound-gate posture-marker, not a
    // structural dependency of the emit path.
    caixa_core::require_aplicacao_view::<Error>(caixa)?;
    // Route the `:versao` presence + carry-through both through the
    // substrate-canonical [`caixa_core::require_valid_versao`] compound
    // gate — one substrate helper folds the two-arm
    // [`caixa_core::manifest::Caixa::validate_versao`] cascade
    // (empty-first → [`caixa_core::ManifestError::VersaoEmpty`],
    // SemVer-2-shape-invalid →
    // [`caixa_core::ManifestError::VersaoInvalid { versao, reason }`])
    // and hands back the validated `&str` for the downstream
    // [`AplicacaoIntent::version`] carrier the tatara reconciler passes
    // to Helm's SemVer-2-strict `Chart.yaml::version` field. Pre-lift
    // this site carried an inline `if caixa.versao().is_empty()` gate
    // that checked only the empty arm; a struct-literal
    // `Caixa { versao: "0.1".into(), .. }` through the public field or
    // a fixture that mutates `caixa.versao` past
    // [`caixa_core::Caixa::from_lisp`] silently landed a
    // SemVer-2-invalid byte-string at the emit boundary and surfaced
    // far from the source `caixa.lisp` as a Helm chart-install
    // rejection with no field naming the offending `:versao`. The
    // compound gate closes both drift arms structurally — every
    // `:versao` past this call site is guaranteed-round-trippable
    // through `semver::Version::parse`, so the future substrate
    // consumers the compound-gate docstring names
    // (`sui-supercacheci::canteiro::emit_gha`,
    // the M4 admission webhook, `feira publish`'s `v<versao>` git-tag
    // materializer) can rely on the value's shape without re-validating
    // at the renderer layer. Same trajectory as the peer per-Servico
    // [`caixa_core::require_v0_servico_shape`] compound entry gate
    // (`caixa-helm`, `caixa-flux`) and the peer per-Aplicacao
    // [`caixa_core::require_aplicacao_view`] compound entry gate
    // (`caixa-mesh`) already carry — one substrate helper per compound
    // gate, one `#[from]` arm on the shared typed view.
    let versao = caixa_core::require_valid_versao::<Error>(caixa)?.to_owned();

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
    use caixa_core::{CaixaKind, lareira_chart_name};
    use pretty_assertions::assert_eq;
    use tatara_process::intent::IntentVariant;
    use tatara_process::lifetime::LifetimeVariant;

    fn sample_caixa_src() -> String {
        // Smallest valid Aplicacao caixa past the compound
        // [`caixa_core::require_aplicacao_view`] entry gate
        // (`require_kind(Aplicacao)` + `Caixa::aplicacao_view` +
        // [`caixa_core::AplicacaoSpec::validate`] +
        // [`caixa_core::aplicacao::validate_no_self_membership`]) this
        // crate's `process_for_aplicacao` now routes through. Pre-lift
        // this fixture carried an empty `:membros ()` stanza — the
        // tatara renderer's pre-lift kind-only gate accepted it and
        // materialized a `Process` CR whose `AplicacaoIntent` pointed
        // at a chart the resolver had no member entries to fan out on.
        // Post-lift the substrate's own author-time cascade refuses
        // empty `:membros` (`AplicacaoError::NoMembros`) and empty
        // `:placement :clusters` (`AplicacaoError::PlacementWithoutClusters`),
        // so this fixture ships one member entry (`"worker"`) and one
        // hosting cluster (`"ephemeral-test-01"`) — the minimal shape
        // every peer per-Aplicacao renderer's happy-path fixture
        // (`caixa-mesh::tests::aplicacao_caixa`) already satisfies.
        r#"
(defcaixa
  :nome "example-attest"
  :kind Aplicacao
  :versao "0.1.0"
  :membros ((:caixa "worker" :versao "^0.1"))
  :placement (:estrategia Replicated :clusters ("ephemeral-test-01")))
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
    fn versao_gate_routes_through_require_valid_versao() {
        // Peer-arm pin on the `:versao` gate in
        // `process_for_aplicacao`. Pre-lift the gate carried an inline
        // `if caixa.versao().is_empty() { return
        // Err(Error::MissingVersao); }` two-liner that (a) checked only
        // the empty arm — a SemVer-2-invalid `:versao` past
        // [`caixa_core::manifest::Caixa::from_lisp`] (a fixture that
        // mutates the public `versao` field, a struct-literal caller
        // that bypasses [`Caixa::from_lisp`]) silently passed through
        // to the `AplicacaoIntent.version` carrier the tatara
        // reconciler hands to Helm's SemVer-2-strict `Chart.yaml::
        // version` field — and (b) surfaced a context-free
        // `Error::MissingVersao` diagnostic with no field naming the
        // offending byte-string.
        //
        // Post-lift the gate routes through the substrate-canonical
        // [`caixa_core::require_valid_versao`] compound helper —
        // structural mirror of the sibling per-Servico
        // [`caixa_core::require_v0_servico_shape`] and per-Aplicacao
        // [`caixa_core::require_aplicacao_view`] compound entry gates —
        // which folds the two-arm
        // [`caixa_core::manifest::Caixa::validate_versao`] cascade
        // (empty-first → [`caixa_core::ManifestError::VersaoEmpty`],
        // SemVer-2-shape-invalid →
        // [`caixa_core::ManifestError::VersaoInvalid { versao, reason }`])
        // and hands back the validated `&str`. Sample fixture holds a
        // non-empty, SemVer-2-valid `:versao` so this pin exercises
        // the happy-path arm — the empty + invalid arms are pinned
        // separately on the sibling
        // `versao_gate_rejects_empty_versao_through_manifest_error`
        // / `versao_gate_rejects_semver_invalid_versao_through_manifest_error`
        // pin pair.
        let caixa = Caixa::from_lisp(&sample_caixa_src()).expect("parse caixa");
        assert!(
            !caixa.versao().is_empty(),
            "sample fixture's :versao must be non-empty for the emit path \
             to reach the AplicacaoIntent compose site (rules out a false-\
             positive on the versao gate's carry-through pin)"
        );
        // The happy path emits a `Process` — no gate rejection past
        // the substrate's `validate_versao` cascade.
        process_for_aplicacao(&caixa, &sample_inputs()).expect("valid caixa emits");
        // Non-`&str`-return regression sentinel: `Caixa::versao()`
        // must return a `&str` so the substrate-side
        // `caixa_core::require_valid_versao` helper's `Ok(&str)`
        // return-type binds to the caller's `.to_owned()` `&str →
        // String` promotion at the emit site.
        let _: &str = caixa.versao();
    }

    #[test]
    fn versao_gate_rejects_empty_versao_through_manifest_error() {
        // Fail-before-pass-after pin on the empty arm of the compound
        // [`caixa_core::require_valid_versao`] gate this crate's
        // `process_for_aplicacao` routes `:versao` through. Pre-lift
        // the inline `if caixa.versao().is_empty()` gate surfaced a
        // context-free `Error::MissingVersao` diagnostic with no
        // field naming the offending caixa or its (empty) `:versao`
        // byte-string. Post-lift the gate routes through the
        // substrate-canonical
        // [`caixa_core::manifest::Caixa::validate_versao`] two-arm
        // cascade, so the empty arm surfaces the substrate's own
        // self-locating [`caixa_core::ManifestError::VersaoEmpty`]
        // diagnostic (whose Display body names the load-bearing
        // downstream consumers the empty `:versao` breaks — the
        // `lareira-<nome>` Helm chart's `Chart.yaml` version +
        // appVersion, the `feira publish` `v<versao>` git tag, the
        // OCI image's `:v<versao>` / `:latest` tags, the lacre
        // closure's `concrete_versao`, and the `:upgrade-from :from`
        // peers). [`caixa_core::Caixa::from_lisp`] does not gate the
        // top-level `:versao` value-shape at parse time (that gate
        // lives in [`caixa_core::manifest::Caixa::validate_versao`],
        // called by the `feira build` cascade), so the fixture
        // constructs a parse-valid caixa and clears its `:versao`
        // through the public `versao` field — the same shape a
        // caller that bypasses the `feira build` gate (a struct-
        // literal, a fixture that mutates past `from_lisp`, the
        // deferred M4 admission webhook running its own compound
        // gate) would reach.
        let mut caixa = Caixa::from_lisp(&sample_caixa_src()).expect("parse caixa");
        caixa.versao.clear();
        let err = process_for_aplicacao(&caixa, &sample_inputs()).unwrap_err();
        match err {
            Error::InvalidVersao(caixa_core::ManifestError::VersaoEmpty) => {}
            other => panic!(
                "expected Error::InvalidVersao(ManifestError::VersaoEmpty) on an empty :versao, \
                 got {other:?}"
            ),
        }
    }

    #[test]
    fn versao_gate_rejects_semver_invalid_versao_through_manifest_error() {
        // Fail-before-pass-after pin on the SemVer-2-shape-invalid
        // arm of the compound [`caixa_core::require_valid_versao`]
        // gate this crate's `process_for_aplicacao` routes `:versao`
        // through. This arm is the load-bearing widening: pre-lift
        // the inline `if caixa.versao().is_empty()` gate accepted
        // every non-empty byte-string — including canonical
        // paste-from-doc footguns the substrate's
        // [`caixa_core::manifest::Caixa::validate_versao`] cascade
        // documents (`"0.1"` — the two-part-shape drift missing the
        // patch component; `"v0.1.0"` — the git-tag-shape leaking
        // into the manifest slot; `"latest"` — the docker-tag-shape
        // leaking; `"^0.1"` — the requirement-shape leaking; a
        // four-part `"0.1.0.0"`) — and the `AplicacaoIntent.version`
        // carrier the tatara reconciler hands to Helm's SemVer-2-
        // strict `Chart.yaml::version` field surfaced the failure
        // far from the source caixa.lisp, with no field naming the
        // offending byte-string. Post-lift the gate routes through
        // the substrate cascade so every invalid arm surfaces the
        // substrate's own self-locating
        // [`caixa_core::ManifestError::VersaoInvalid`] diagnostic
        // carrying the offending value + a parser-shaped reason
        // — the author can grep their `caixa.lisp` for
        // `:versao "<value>"` and fix it in one edit.
        let mut caixa = Caixa::from_lisp(&sample_caixa_src()).expect("parse caixa");
        caixa.versao = "0.1".to_string();
        let err = process_for_aplicacao(&caixa, &sample_inputs()).unwrap_err();
        match err {
            Error::InvalidVersao(caixa_core::ManifestError::VersaoInvalid { versao, reason }) => {
                assert_eq!(
                    versao, "0.1",
                    "VersaoInvalid must carry the offending byte-string verbatim"
                );
                assert!(
                    !reason.is_empty(),
                    "VersaoInvalid must thread the parser's non-empty `reason` for the author's \
                     remediation prose"
                );
            }
            other => panic!(
                "expected Error::InvalidVersao(ManifestError::VersaoInvalid {{ .. }}) on a \
                 SemVer-2-shape-invalid :versao, got {other:?}"
            ),
        }
    }

    #[test]
    fn aplicacao_view_gate_rejects_empty_membros_through_aplicacao_error() {
        // Fail-before-pass-after pin on the empty-`:membros` arm of the
        // compound [`caixa_core::require_aplicacao_view`] gate this
        // crate's `process_for_aplicacao` now routes through. Pre-lift
        // the tatara entry gate carried only the two-step
        // `require_kind + require_valid_versao` prelude — the substrate's
        // own [`caixa_core::AplicacaoSpec::validate_membros`] arm
        // ([`caixa_core::AplicacaoError::NoMembros`]) fired at the
        // author-time `feira build` cascade but every rendering path
        // that bypassed `feira build` (a struct-literal `Caixa`, a
        // fixture that mutates the public `membros` field past
        // [`caixa_core::Caixa::from_lisp`], the deferred M4 admission
        // webhook) silently materialized a `Process` CR whose
        // `AplicacaoIntent.chart_ref` pointed at a chart the resolver
        // had no member entries to fan out on. Post-lift the tatara
        // renderer routes the substrate's compound four-arm cascade
        // (`require_kind` + `Caixa::aplicacao_view` +
        // `AplicacaoSpec::validate` + `validate_no_self_membership`),
        // so the empty-`:membros` arm surfaces the substrate's own
        // self-locating [`caixa_core::AplicacaoError::NoMembros`]
        // diagnostic verbatim — matching the peer per-Aplicacao
        // renderer `caixa-mesh`'s `typed_view` refusal.
        let mut caixa = Caixa::from_lisp(&sample_caixa_src()).expect("parse caixa");
        caixa.membros.clear();
        let err = process_for_aplicacao(&caixa, &sample_inputs()).unwrap_err();
        match err {
            Error::InvalidAplicacao(caixa_core::AplicacaoError::NoMembros) => {}
            other => panic!(
                "expected Error::InvalidAplicacao(AplicacaoError::NoMembros) on an empty :membros, \
                 got {other:?}"
            ),
        }
    }

    #[test]
    fn aplicacao_view_gate_rejects_self_membership_through_aplicacao_error() {
        // Fail-before-pass-after pin on the cross-slot self-edge arm
        // of the compound [`caixa_core::require_aplicacao_view`] gate
        // this crate's `process_for_aplicacao` now routes through — the
        // [`caixa_core::aplicacao::validate_no_self_membership`] gate
        // 3aefefb folded into the compound helper as its fourth step
        // (`require_kind` + `Caixa::aplicacao_view` +
        // `AplicacaoSpec::validate` + `validate_no_self_membership`).
        // Pre-lift the tatara entry gate's two-step
        // `require_kind + require_valid_versao` prelude accepted a
        // self-referential Aplicacao (an author who declared `:membros`
        // naming the Aplicacao's own `:nome`) and materialized a
        // `Process` CR whose downstream lacre closure was a one-node
        // recursion the resolver's traversal either refused far from
        // the source `caixa.lisp` or exhausted its stack on. Post-lift
        // the tatara renderer inherits the compound self-edge gate by
        // routing through [`caixa_core::require_aplicacao_view`], so
        // the fixture's `:membros` naming the Aplicacao's own `:nome`
        // (`"example-attest"`) surfaces
        // [`caixa_core::AplicacaoError::MembroIsSelfAplicacao`]
        // verbatim — self-locating in the offending caixa's `:nome`.
        // Peer to
        // [`caixa_mesh::tests::typed_view_rejects_self_referential_membros`]
        // on the sibling per-Aplicacao renderer's self-edge gate axis;
        // this pin closes the analogous refusal on caixa-tatara's
        // per-Aplicacao entry gate.
        let mut caixa = Caixa::from_lisp(&sample_caixa_src()).expect("parse caixa");
        caixa.membros = vec![caixa_core::Membro {
            caixa: caixa.nome().to_string(),
            versao: "^0.1".to_string(),
        }];
        let err = process_for_aplicacao(&caixa, &sample_inputs()).unwrap_err();
        match err {
            Error::InvalidAplicacao(caixa_core::AplicacaoError::MembroIsSelfAplicacao {
                caixa: nome,
            }) => {
                assert_eq!(
                    nome, "example-attest",
                    "MembroIsSelfAplicacao must carry the parent Aplicacao's :nome verbatim so the \
                     diagnostic is self-locating in the source caixa.lisp"
                );
            }
            other => panic!(
                "expected Error::InvalidAplicacao(AplicacaoError::MembroIsSelfAplicacao {{ .. }}) \
                 on a self-referential :membros entry, got {other:?}"
            ),
        }
    }

    #[test]
    fn aplicacao_view_gate_routes_through_require_aplicacao_view_happy_path() {
        // Happy-path pin on the compound
        // [`caixa_core::require_aplicacao_view`] gate this crate's
        // `process_for_aplicacao` now routes through. The sample
        // fixture ships a well-formed `:membros` + `:placement`
        // (the minimal shape the substrate's four-arm
        // `require_kind(Aplicacao)` + `Caixa::aplicacao_view` +
        // `AplicacaoSpec::validate` + `validate_no_self_membership`
        // cascade accepts), so the render must succeed and surface an
        // `Intent::Aplicacao` variant. Structurally pins that the
        // compound gate does not gratuitously reject the substrate's
        // own canonical happy-path shape — a regression that swapped
        // the helper's `E: From<KindMismatch> + From<AplicacaoError>`
        // bound for a stricter one, or that introduced a compound-gate
        // arm the fixture cannot satisfy, would surface here as a
        // structural render-side rejection rather than a silent
        // cluster-apply-time symptom.
        let caixa = Caixa::from_lisp(&sample_caixa_src()).expect("parse caixa");
        let process = process_for_aplicacao(&caixa, &sample_inputs()).expect("valid caixa emits");
        assert!(
            matches!(
                process.spec.intent.variant().expect("intent"),
                IntentVariant::Aplicacao(_)
            ),
            "the compound require_aplicacao_view happy-path must materialize an \
             Intent::Aplicacao variant"
        );
    }
}
