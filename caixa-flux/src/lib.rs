//! caixa-flux — typed renderer that emits the FluxCD-side fragments a
//! caixa Servico needs in the cluster's GitOps tree.
//!
//! Same naming convention as [`caixa_helm`] (renders per-program Helm
//! charts) and [`caixa_flake`] (renders flake.nix): `caixa-<target>` =
//! "Rust crate that takes a typed [`Caixa`] and emits the canonical
//! source for `<target>`".
//!
//! ## Two paths, two surfaces
//!
//! Per `theory/META-FRAMEWORK.md` §I, two equally-canonical ways exist
//! to deploy a caixa Servico:
//!
//! 1. **Aggregator path** ([`programs_yaml_entry`]) — the cluster has
//!    exactly one `lareira-fleet-programs` HelmRelease whose values
//!    contain a `programs:` array. Adding a Servico = adding one entry
//!    to that array. **Higher leverage** — one HelmRelease handles the
//!    whole fleet's worth of caixas, fewer reconciler events, simpler
//!    cluster surface. This is what `feira deploy` uses by default.
//!
//! 2. **Bundle path** ([`cluster_bundle`]) — emit a fresh `GitRepository`
//!    + `HelmRelease` + `Kustomization` trio for the caixa's own per-
//!    program chart (rendered by `caixa-helm`). Used for one-off /
//!    isolated services where the aggregator overhead is undesirable
//!    (e.g. alpha workloads with non-standard images, breakglass tooling).
//!
//! ## V0 contract
//!
//! ```rust,ignore
//! use caixa_core::Caixa;
//! use caixa_flux::programs_yaml_entry;
//!
//! let caixa = Caixa::from_lisp(src)?;
//! let cu_yaml: serde_yaml::Value =
//!     serde_yaml::from_str(std::fs::read_to_string("servicos/hello-rio.computeunit.yaml")?)?;
//! let entry: serde_yaml::Value = programs_yaml_entry(&caixa, &cu_yaml)?;
//! // → { name: hello-rio, namespace: tatara-system, module: { source: ... }, ... }
//! ```
//!
//! ## What this is NOT
//!
//! - Not a Flux CLI wrapper — bytes only.
//! - Not the operator deploy bundle — that lives in `pleme-io/caixa/operator-flux/`.
//! - Not an installer — `feira deploy` orchestrates the I/O of writing
//!   to a GitOps repo + opening a PR.

#![allow(clippy::module_name_repetitions)]

use caixa_core::{Caixa, CaixaKind, lareira_chart_name};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors caixa-flux can raise.
#[derive(Debug, Error)]
pub enum Error {
    /// The caixa's `:kind` doesn't match what `caixa-flux` targets
    /// (this renderer only emits `programs.yaml` entries +
    /// `GitRepository`/`HelmRelease`/`Kustomization` bundles for
    /// `:kind Servico`). Lifted from a prior `NotAServico(CaixaKind)`
    /// arm to wrap [`caixa_core::KindMismatch`] so the diagnostic
    /// names the offending caixa's `:nome` (not just its kind),
    /// shared verbatim with `caixa-helm` and `caixa-mesh`.
    #[error("{0}")]
    NotAServico(#[from] caixa_core::KindMismatch),
    /// The caixa's `:servicos` list doesn't carry exactly one entry —
    /// the V0 contract every Servico-kind caixa satisfies (one
    /// ComputeUnit YAML pointer per Servico, matching the one
    /// programs.yaml entry / cluster bundle this renderer emits).
    /// Lifted from a prior `UnsupportedServicoCount(usize)` arm to
    /// wrap [`caixa_core::ServicoCountMismatch`] so the diagnostic
    /// names the offending caixa's `:nome` (not just the count),
    /// shared verbatim with `caixa-helm` (the peer per-Servico
    /// renderer running the same V0 invariant on the
    /// `lareira-<nome>` chart-dir axis).
    #[error("{0}")]
    UnsupportedServicoCount(#[from] caixa_core::ServicoCountMismatch),
    #[error("computeunit yaml missing required field: {0}")]
    MissingField(&'static str),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("render: {0}")]
    Render(#[from] caixa_core::RenderError),
}

/// Default cluster-wide namespace for caixa Servicos when the
/// computeunit doesn't pin its own. Re-export of the canonical
/// [`caixa_core::DEFAULT_NAMESPACE`] so the namespace string lives in
/// exactly one place across every renderer — caixa-flux's
/// programs.yaml / GitRepository / HelmRelease / Kustomization
/// emitters and caixa-mesh's programs fan-out / CiliumNetworkPolicy /
/// Gateway / HTTPRoute emitters now consult the same `&'static str`,
/// so a future per-cluster-namespace rebrand is a one-line edit on
/// the canonical [`caixa_core::DEFAULT_NAMESPACE`] declaration, not a
/// coordinated rewrite across this crate, caixa-mesh, and every
/// future per-target renderer the substrate adds.
pub use caixa_core::DEFAULT_NAMESPACE;

/// Canonical FluxCD installation namespace — re-export of the lifted
/// [`caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE`] so the load-bearing
/// string lives in exactly one place across every consumer of the
/// rendered `kustomization.yaml`'s `metadata.namespace` and
/// `spec.sourceRef.name` axes. Both axes are the same conceptual "Flux
/// installation namespace" the `flux bootstrap` pipeline names; until
/// this lift landed they sat as two inline `flux-system` literals inside
/// [`cluster_bundle`]'s `kustomization.yaml` format-string template, and
/// any future per-edition Flux-installation-namespace rebrand on one
/// without a coordinated edit on the other would have silently emitted a
/// `Kustomization` outside the bootstrap controller's watch window or a
/// dangling `sourceRef`. Same shape as the
/// [`caixa_core::DEFAULT_NAMESPACE`] (a085b26) /
/// [`caixa_core::DEFAULT_LIBRARY_NAME`] (41438dc) /
/// [`caixa_core::DEFAULT_PUBLISH_TAG_PREFIX`] (0a6a602) lifts on the peer
/// canonical-load-bearing-string surface.
pub use caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE;

/// Canonical FluxCD `HelmRelease` CRD `apiVersion` — re-export of the
/// lifted [`caixa_core::FLUX_HELMRELEASE_API_VERSION`] so the load-bearing
/// string lives in exactly one place across the rendered Flux bundle's
/// `helmrelease.yaml` document `apiVersion` axis + the rendered
/// `kustomization.yaml` document's `spec.healthChecks[].apiVersion` axis.
/// Both axes are the same conceptual "Flux v2 `HelmRelease` CRD group/
/// version" load-bearing string and must move together on any future
/// upstream Flux v3 migration; until this lift landed they sat as four
/// inline `helm.toolkit.fluxcd.io/v2` literals (two render-side at lines
/// 455, 504 + two test-fixture-side at lines 928, 970), and any future
/// per-Flux-v3-migration version bump on one without a coordinated edit
/// on the other would have silently routed the rendered `HelmRelease`
/// outside the controller's `Watches` (controller-side: never reconciled,
/// every dependent chart frozen at last-applied state) or made the
/// `Kustomization`'s health-check dangle (apply-side: the per-resource
/// health-gate never resolves, the parent Kustomization sits perpetually
/// in `Reconciling`). Same shape as the
/// [`caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE`] (7197d38) lift on the
/// sibling Flux-installation-namespace axis.
pub use caixa_core::FLUX_HELMRELEASE_API_VERSION;

/// Canonical FluxCD `GitRepository` CRD `apiVersion` — re-export of the
/// lifted [`caixa_core::FLUX_GITREPOSITORY_API_VERSION`] so the load-bearing
/// string lives in exactly one place across the rendered Flux bundle's
/// `gitrepository.yaml` document `apiVersion` axis. Until this lift landed
/// the axis sat as an inline `source.toolkit.fluxcd.io/v1` literal at line
/// 436 of this crate's [`cluster_bundle`] format-string template, and any
/// future per-Flux-v3-migration version bump on this axis without a
/// coordinated edit on the sibling [`FLUX_HELMRELEASE_API_VERSION`] axis
/// would have silently routed the rendered `GitRepository` outside the
/// Flux v2 `source-controller`'s `Watches` (controller-side: never
/// reconciled, the dependent HelmRelease's `chart: sourceRef` dangles,
/// every per-Servico apply silently comes up with the prior reconciled
/// state). Same shape as the [`FLUX_HELMRELEASE_API_VERSION`] (55f0fd9) /
/// [`caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE`] (7197d38) lifts on the
/// sibling Flux-v2-load-bearing-string axis.
pub use caixa_core::FLUX_GITREPOSITORY_API_VERSION;

/// Canonical FluxCD `Kustomization` CRD `apiVersion` — re-export of the
/// lifted [`caixa_core::FLUX_KUSTOMIZATION_API_VERSION`] so the load-
/// bearing string lives in exactly one place across the rendered Flux
/// bundle's `kustomization.yaml` document `apiVersion` axis. Completes
/// the Flux v2 controller-triplet (source-controller +
/// helm-controller + kustomize-controller) lift alongside the sibling
/// [`FLUX_GITREPOSITORY_API_VERSION`] (8a6c8a3) and
/// [`FLUX_HELMRELEASE_API_VERSION`] (55f0fd9) re-exports — every
/// per-controller CRD-group/version is now a typed substrate-side
/// `&'static str` consumed through one `pub use caixa_core::FLUX_*`
/// re-export.
///
/// Until this lift landed the axis sat as an inline
/// `kustomize.toolkit.fluxcd.io/v1` literal in this crate's
/// [`cluster_bundle`] `kustomization.yaml` format-string template,
/// and any future per-Flux-v3-migration version bump on this axis
/// without a coordinated edit on the sibling
/// [`FLUX_HELMRELEASE_API_VERSION`] / [`FLUX_GITREPOSITORY_API_VERSION`]
/// axes (the Flux v2 controller triplet's CRD group/versions move
/// together upstream) would have silently routed the rendered
/// `Kustomization` outside the Flux v2 `kustomize-controller`'s
/// `Watches` (apply-side: the parent Kustomization is never
/// reconciled, every dependent `HelmRelease` / `GitRepository` it
/// keys off sits perpetually un-applied at the cluster). Same shape
/// as the [`FLUX_GITREPOSITORY_API_VERSION`] (8a6c8a3) /
/// [`FLUX_HELMRELEASE_API_VERSION`] (55f0fd9) /
/// [`caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE`] (7197d38) lifts on
/// the sibling Flux-v2-load-bearing-string axis.
pub use caixa_core::FLUX_KUSTOMIZATION_API_VERSION;

/// Canonical Helm library-chart name every `lareira-<nome>` chart
/// depends on — re-export of the lifted [`caixa_core::DEFAULT_LIBRARY_NAME`]
/// so the load-bearing string lives in exactly one place across every
/// caixa renderer. The wrap key the `cluster_bundle` `helmrelease.yaml`
/// template uses under `spec.values.<library>:` to thread the per-cluster
/// overrides through to the rendered chart's dep block must match the
/// peer `caixa-helm` chart's `dependencies[0].name` axis exactly
/// (Helm's per-dep alias convention scopes values under the dep's
/// `name:` when no `alias:` is set), and both axes now consult the same
/// `&'static str`. Same shape as the [`caixa_core::DEFAULT_NAMESPACE`]
/// (a085b26) / [`caixa_core::DEFAULT_SERVICO_PORT`] (1e22add) lifts on
/// the peer canonical-K8s-axis-constant surface.
pub use caixa_core::DEFAULT_LIBRARY_NAME;

/// Render a single `programs:[]` array entry for the cluster's
/// `lareira-fleet-programs` HelmRelease values.
///
/// The output is `serde_yaml::Value::Mapping`, so callers can splice
/// it into an existing `programs:` array without re-parsing the
/// containing structure. Schema is enforced by
/// `lareira-fleet-programs/values.schema.json` (`#/definitions/program`).
///
/// Pulls:
/// - `name` from `caixa.nome`
/// - `namespace` from `computeunit.metadata.namespace` (or `DEFAULT_NAMESPACE`)
/// - `module` / `trigger` / `capabilities` / `config` / `resources`
///   from `computeunit.spec.*` (verbatim — schemas already match)
pub fn programs_yaml_entry(
    caixa: &Caixa,
    computeunit_yaml: &serde_yaml::Value,
) -> Result<serde_yaml::Value, Error> {
    caixa_core::require_kind(caixa, CaixaKind::Servico)?;
    caixa_core::require_single_servico(caixa)?;

    let spec = computeunit_yaml
        .get("spec")
        .ok_or(Error::MissingField("spec"))?;

    let namespace = computeunit_yaml
        .get("metadata")
        .and_then(|m| m.get("namespace"))
        .and_then(|n| n.as_str())
        .unwrap_or(DEFAULT_NAMESPACE)
        .to_string();

    let mut entry = serde_yaml::Mapping::new();
    entry.insert(
        serde_yaml::Value::String("name".into()),
        serde_yaml::Value::String(caixa.nome.clone()),
    );
    entry.insert(
        serde_yaml::Value::String("namespace".into()),
        serde_yaml::Value::String(namespace),
    );

    // Splice every spec.* field through (module, trigger, capabilities,
    // config, resources, serviceAccount). Operator + chart schemas are
    // already authoritative; we don't re-validate here.
    if let serde_yaml::Value::Mapping(spec_map) = spec {
        for (k, v) in spec_map {
            if let Some(s) = k.as_str() {
                entry.insert(serde_yaml::Value::String(s.to_string()), v.clone());
            }
        }
    }

    // M2 typed-substrate slots — propagate from caixa.lisp into the
    // programs.yaml entry so lareira-fleet-programs renders a
    // ComputeUnit that carries the typed `:limits`, `:behavior`, and
    // `:upgrade-from` fields all the way to the cluster operator.
    // Spec values from computeunit.yaml take precedence (entry already
    // populated above); slots only on the Caixa flow through here.
    // Shared with caixa-helm::build_values_yaml via
    // caixa_core::render::servico_m2_overlay so both renderers agree
    // on key naming + emptiness rules + serialization-error handling.
    for (key, value) in caixa_core::servico_m2_overlay(caixa)? {
        entry
            .entry(serde_yaml::Value::String(key.to_string()))
            .or_insert(value);
    }

    Ok(serde_yaml::Value::Mapping(entry))
}

/// Insert/upsert an entry into a `programs:` array nested under
/// the canonical fleet-manifest path: `spec.values.programs[]` in a
/// `HelmRelease` document. The pleme-io convention puts the fleet's
/// program list inside a HelmRelease (consumed by `lareira-fleet-programs`),
/// not at the top level. Same upsert semantics as
/// [`upsert_into_programs_yaml`] — match by `name`, replace in place,
/// otherwise append.
pub fn upsert_into_helmrelease_programs(
    helmrelease: serde_yaml::Value,
    new_entry: serde_yaml::Value,
) -> Result<(serde_yaml::Value, bool), Error> {
    let new_name = new_entry
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or(Error::MissingField("name"))?
        .to_string();

    let serde_yaml::Value::Mapping(mut root) = helmrelease else {
        return Err(Error::MissingField(
            "expected mapping at root of HelmRelease",
        ));
    };

    let spec = root
        .get_mut(serde_yaml::Value::String("spec".into()))
        .ok_or(Error::MissingField("spec"))?;
    let serde_yaml::Value::Mapping(spec_map) = spec else {
        return Err(Error::MissingField("spec must be a mapping"));
    };
    let values = spec_map
        .entry(serde_yaml::Value::String("values".into()))
        .or_insert(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    let serde_yaml::Value::Mapping(values_map) = values else {
        return Err(Error::MissingField("spec.values must be a mapping"));
    };
    let programs_val = values_map
        .entry(serde_yaml::Value::String("programs".into()))
        .or_insert(serde_yaml::Value::Sequence(Vec::new()));
    let arr = match programs_val {
        serde_yaml::Value::Sequence(seq) => seq,
        _ => {
            return Err(Error::MissingField(
                "spec.values.programs must be a sequence",
            ));
        }
    };

    let mut inserted = true;
    for slot in arr.iter_mut() {
        if slot.get("name").and_then(|n| n.as_str()) == Some(&new_name) {
            *slot = new_entry.clone();
            inserted = false;
            break;
        }
    }
    if inserted {
        arr.push(new_entry);
    }

    Ok((serde_yaml::Value::Mapping(root), inserted))
}

/// Insert/upsert an entry into a `programs:` array of an existing
/// values.yaml structure.
///
/// Idempotent: if an entry with the same `name` exists, replaces it
/// in-place (preserving order). If not, appends. Returns the modified
/// document. Operates on `Value` so callers can round-trip via
/// `serde_yaml::from_str` / `to_string` without losing structure.
///
/// Returns the modified `programs_yaml` plus a `bool` indicating
/// whether the entry was a new insert (`true`) or a replacement (`false`).
pub fn upsert_into_programs_yaml(
    programs_yaml: serde_yaml::Value,
    new_entry: serde_yaml::Value,
) -> Result<(serde_yaml::Value, bool), Error> {
    let new_name = new_entry
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or(Error::MissingField("name"))?
        .to_string();

    let serde_yaml::Value::Mapping(mut root) = programs_yaml else {
        return Err(Error::MissingField(
            "expected mapping at root of values.yaml",
        ));
    };

    let programs_key = serde_yaml::Value::String("programs".into());
    let programs_val = root
        .entry(programs_key.clone())
        .or_insert(serde_yaml::Value::Sequence(Vec::new()));

    let arr = match programs_val {
        serde_yaml::Value::Sequence(seq) => seq,
        _ => return Err(Error::MissingField("programs must be a sequence")),
    };

    let mut inserted = true;
    for slot in arr.iter_mut() {
        if slot.get("name").and_then(|n| n.as_str()) == Some(&new_name) {
            *slot = new_entry.clone();
            inserted = false;
            break;
        }
    }
    if inserted {
        arr.push(new_entry);
    }

    Ok((serde_yaml::Value::Mapping(root), inserted))
}

// ── Cluster bundle (one-off / standalone path) ──────────────────────────

/// Inputs for [`cluster_bundle`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterBundleOpts {
    /// Cluster name — drives output paths (e.g. `rio`, `mar`).
    pub cluster: String,
    /// Namespace for the rendered HelmRelease.
    pub namespace: String,
    /// Reconcile interval string (Helm/Flux duration like `"10m"`).
    pub interval: String,
    /// Path to the chart inside the source repo (default: `chart/`).
    pub chart_path: String,
    /// Source git URL.
    pub git_url: String,
    /// Source git ref (branch or tag).
    pub git_ref: GitRefSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GitRefSpec {
    Tag(String),
    Branch(String),
    Commit(String),
}

impl ClusterBundleOpts {
    /// Sensible defaults for a per-program standalone bundle.
    #[must_use]
    pub fn for_caixa(caixa: &Caixa, cluster: impl Into<String>) -> Self {
        Self {
            cluster: cluster.into(),
            namespace: DEFAULT_NAMESPACE.into(),
            interval: "10m".into(),
            chart_path: "chart".into(),
            git_url: caixa
                .repositorio
                .clone()
                .unwrap_or_else(|| format!("https://github.com/pleme-io/{}", caixa.nome)),
            git_ref: GitRefSpec::Tag(format!(
                "{prefix}{versao}",
                prefix = caixa_core::DEFAULT_PUBLISH_TAG_PREFIX,
                versao = caixa.versao,
            )),
        }
    }
}

/// One file of the cluster bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleFile {
    pub path: std::path::PathBuf,
    pub contents: String,
}

/// Cluster bundle: the FluxCD trio for a standalone caixa deploy.
///
/// Three YAMLs:
///   gitrepository.yaml — points at the caixa's source repo at a tag
///   helmrelease.yaml   — uses the per-program chart from caixa-helm
///   kustomization.yaml — the Flux Kustomization that staples them
///
/// Written under `<cluster>/services/<caixa-name>/` by `feira deploy`.
///
/// V0 contract: every `:kind Servico` caixa carries exactly one
/// `:servicos` entry (one ComputeUnit YAML pointer per Servico,
/// matching the one `lareira-<nome>` Helm chart `caixa-helm` renders
/// and the one HelmRelease this bundle's `helmrelease.yaml` points at).
/// The pair [`caixa_core::require_kind`] + [`caixa_core::require_single_servico`]
/// is the canonical per-Servico-renderer entry-point gate axis the peer
/// [`programs_yaml_entry`] (the aggregator path) and
/// [`caixa_helm::render_chart_for_servico`] (the per-program chart path)
/// both run; until this gate landed `cluster_bundle` ran only the kind
/// half of the pair, so a Servico-kind caixa with a non-singleton
/// `:servicos` list silently passed the bundle render and the failure
/// surfaced at the chart-render layer (`caixa-helm` refused the input
/// with `UnsupportedServicoCount`) far from the source `caixa.lisp` and
/// far from the deploy-path entry point — the canonical "the V0
/// invariant is enforced at every per-Servico renderer entry except
/// this one" footgun the prior 06b2981 lift commit's body named when
/// it called the pair "the canonical V0-shape gate pair for the Servico
/// kind, in one place". Same shape every peer per-Servico-renderer
/// entry-point uses ([`programs_yaml_entry`] at the aggregator path,
/// [`caixa_helm::render_chart_for_servico`] at the per-program chart
/// path).
pub fn cluster_bundle(caixa: &Caixa, opts: &ClusterBundleOpts) -> Result<Vec<BundleFile>, Error> {
    caixa_core::require_kind(caixa, CaixaKind::Servico)?;
    caixa_core::require_single_servico(caixa)?;

    let name = caixa.nome.clone();
    let chart_name = lareira_chart_name(&name);

    let gitref_field = match &opts.git_ref {
        GitRefSpec::Tag(t) => format!("    tag: {t:?}"),
        GitRefSpec::Branch(b) => format!("    branch: {b:?}"),
        GitRefSpec::Commit(c) => format!("    commit: {c:?}"),
    };

    let gitrepo = format!(
        "---\n\
         # Source — pinned to {tag_human}, rendered by caixa-flux.\n\
         apiVersion: {api_version}\n\
         kind: GitRepository\n\
         metadata:\n  \
           name: {name}\n  \
           namespace: {namespace}\n\
         spec:\n  \
           interval: {interval}\n  \
           url: {url}\n  \
           ref:\n\
         {gitref_field}\n",
        api_version = FLUX_GITREPOSITORY_API_VERSION,
        tag_human = match &opts.git_ref {
            GitRefSpec::Tag(t) => format!("tag {t}"),
            GitRefSpec::Branch(b) => format!("branch {b}"),
            GitRefSpec::Commit(c) => format!("commit {c}"),
        },
        name = name,
        namespace = opts.namespace,
        interval = opts.interval,
        url = opts.git_url,
        gitref_field = gitref_field,
    );

    // The values wrap key under `spec.values.<library>:` must match the
    // peer caixa-helm chart's `dependencies[0].name` axis exactly —
    // Helm's per-dep alias convention scopes values under the
    // dependency's `name:` when no `alias:` is set, so a wrap-key drift
    // silently routes the per-cluster `enabled: true` override nowhere
    // at `helm template` / `helm install` time. Both axes now consult
    // the same lifted [`caixa_core::DEFAULT_LIBRARY_NAME`] constant
    // (`caixa-helm`'s `RenderOpts::library_name` defaults to the same
    // re-export), so a future per-edition library-chart rebrand reaches
    // both consumers through one `&'static str` by construction. Peer
    // with the [`DEFAULT_NAMESPACE`] (a085b26) /
    // [`DEFAULT_SERVICO_PORT`] (1e22add) lifts on the sibling
    // canonical-K8s-axis-constant surface — duplicated load-bearing
    // string axes lifted to one source of truth.
    let helmrelease = format!(
        "---\n\
         # HelmRelease consumes the chart caixa-helm renders for this\n\
         # caixa Servico. Per-cluster values are injected here.\n\
         apiVersion: {api_version}\n\
         kind: HelmRelease\n\
         metadata:\n  \
           name: {name}\n  \
           namespace: {namespace}\n\
         spec:\n  \
           interval: {interval}\n  \
           chart:\n    \
             spec:\n      \
               chart: {chart_path}\n      \
               sourceRef:\n        \
                 kind: GitRepository\n        \
                 name: {name}\n        \
                 namespace: {namespace}\n  \
           install:\n    \
             createNamespace: true\n    \
             remediation:\n      \
               retries: 3\n  \
           upgrade:\n    \
             remediation:\n      \
               retries: 3\n      \
               remediateLastFailure: true\n  \
           values:\n    \
             {library_name}:\n      \
               enabled: true\n",
        api_version = FLUX_HELMRELEASE_API_VERSION,
        name = name,
        namespace = opts.namespace,
        interval = opts.interval,
        chart_path = opts.chart_path,
        library_name = DEFAULT_LIBRARY_NAME,
    );

    let kustomization = format!(
        "---\n\
         # Flux Kustomization that pins the GitRepository + HelmRelease.\n\
         # Paired path: pleme-io/k8s/clusters/{cluster}/services/{name}/\n\
         apiVersion: {kustomization_api_version}\n\
         kind: Kustomization\n\
         metadata:\n  \
           name: {name}\n  \
           namespace: {flux_system}\n\
         spec:\n  \
           interval: {interval}\n  \
           prune: true\n  \
           sourceRef:\n    \
             kind: GitRepository\n    \
             name: {flux_system}\n  \
           path: ./clusters/{cluster}/services/{name}\n  \
           healthChecks:\n    \
             - apiVersion: {api_version}\n      \
               kind: HelmRelease\n      \
               name: {name}\n      \
               namespace: {namespace}\n  \
           timeout: 5m\n",
        kustomization_api_version = FLUX_KUSTOMIZATION_API_VERSION,
        api_version = FLUX_HELMRELEASE_API_VERSION,
        name = name,
        namespace = opts.namespace,
        interval = opts.interval,
        cluster = opts.cluster,
        flux_system = DEFAULT_FLUX_SYSTEM_NAMESPACE,
    );
    // chart_name is reserved for a future kustomization.yaml `resources:`
    // entry pointing at the rendered Chart.yaml; not yet wired.
    let _ = chart_name;

    Ok(vec![
        BundleFile {
            path: std::path::PathBuf::from("gitrepository.yaml"),
            contents: gitrepo,
        },
        BundleFile {
            path: std::path::PathBuf::from("helmrelease.yaml"),
            contents: helmrelease,
        },
        BundleFile {
            path: std::path::PathBuf::from("kustomization.yaml"),
            contents: kustomization,
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use caixa_core::{Caixa, CaixaKind};

    fn sample_caixa() -> Caixa {
        Caixa {
            nome: "hello-rio".into(),
            versao: "0.1.0".into(),
            kind: CaixaKind::Servico,
            edicao: Some("2026".into()),
            descricao: Some("Canonical Rust→wasm32-wasip2 caixa Servico.".into()),
            repositorio: Some("https://github.com/pleme-io/hello-rio".into()),
            licenca: Some("MIT".into()),
            autores: vec!["pleme-io".into()],
            etiquetas: vec!["hello-world".into()],
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

    fn sample_cu_yaml() -> serde_yaml::Value {
        serde_yaml::from_str(
            r#"
apiVersion: wasm.pleme.io/v1alpha1
kind: ComputeUnit
metadata:
  name: hello-rio
  namespace: tatara-system
spec:
  module:
    source: oci://ghcr.io/pleme-io/hello-rio:v0.1.0
  trigger:
    service:
      port: 8080
      paths: ["/", "/hello", "/healthz"]
  capabilities:
    - http-in:0.0.0.0:8080
    - env
"#,
        )
        .unwrap()
    }

    #[test]
    fn default_namespace_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub const DEFAULT_NAMESPACE` was lifted to a
        // re-export of [`caixa_core::DEFAULT_NAMESPACE`] so the
        // namespace string lives in exactly one place across every
        // caixa renderer (caixa-flux + caixa-mesh today, every future
        // per-target renderer the substrate adds). Pin the equality
        // here so any local re-introduction of a sibling `pub const
        // DEFAULT_NAMESPACE: &str = "…"` (the canonical drift footgun
        // that motivated this lift, with the prior caixa-mesh doc-
        // comment explicitly acknowledging the duplication) is a
        // build-time test failure naming the offending drift, not a
        // silent apply-time CiliumNetworkPolicy / Gateway / HTTPRoute
        // `endpointSelector` namespace mismatch dropping every L7
        // contrato flow far from the source rebrand commit. Peer to
        // `caixa_mesh::tests::default_namespace_re_export_points_at_caixa_core_canonical`
        // on the sibling renderer crate.
        assert_eq!(DEFAULT_NAMESPACE, caixa_core::DEFAULT_NAMESPACE);
        assert!(
            std::ptr::eq(
                DEFAULT_NAMESPACE.as_ptr(),
                caixa_core::DEFAULT_NAMESPACE.as_ptr(),
            ),
            "DEFAULT_NAMESPACE must be a re-export of caixa_core::DEFAULT_NAMESPACE, \
             not a sibling `pub const` that happens to carry the same string \
             — drift between the two is the canonical footgun this lift closes"
        );
    }

    #[test]
    fn programs_yaml_entry_round_trips() {
        let entry = programs_yaml_entry(&sample_caixa(), &sample_cu_yaml()).unwrap();
        assert_eq!(
            entry.get("name").and_then(|n| n.as_str()),
            Some("hello-rio")
        );
        assert_eq!(
            entry.get("namespace").and_then(|n| n.as_str()),
            Some("tatara-system")
        );
        assert!(entry.get("module").is_some());
        assert!(entry.get("trigger").is_some());
        assert!(entry.get("capabilities").is_some());
        assert!(
            entry.get("module").and_then(|m| m.get("source")).is_some(),
            "module.source must propagate verbatim"
        );
    }

    #[test]
    fn programs_yaml_entry_falls_back_to_default_namespace() {
        // A computeunit without metadata.namespace should default.
        let cu: serde_yaml::Value = serde_yaml::from_str(
            r#"
apiVersion: wasm.pleme.io/v1alpha1
kind: ComputeUnit
metadata:
  name: hello-rio
spec:
  module:
    source: oci://ghcr.io/pleme-io/hello-rio:v0.1.0
"#,
        )
        .unwrap();
        let entry = programs_yaml_entry(&sample_caixa(), &cu).unwrap();
        assert_eq!(
            entry.get("namespace").and_then(|n| n.as_str()),
            Some(DEFAULT_NAMESPACE)
        );
    }

    #[test]
    fn programs_yaml_entry_refuses_non_servico() {
        let mut c = sample_caixa();
        c.kind = CaixaKind::Biblioteca;
        c.servicos = vec![];
        let err = programs_yaml_entry(&c, &sample_cu_yaml()).unwrap_err();
        assert!(matches!(err, Error::NotAServico(_)));
    }

    #[test]
    fn kind_mismatch_error_names_offending_caixa_nome() {
        // Pinning the lifted [`caixa_core::KindMismatch`] view's
        // load-bearing property: a kind-mismatched caixa surfaces a
        // diagnostic that *names the offending caixa* (`hello-rio`),
        // not just the rejected kind. Before the lift the renderer
        // raised `Error::NotAServico(CaixaKind::Biblioteca)` whose
        // Display said "caixa :kind must be Servico for caixa-flux
        // rendering, got Biblioteca" — the user had to grep their
        // source tree for which caixa.lisp triggered it. After the
        // lift the wrapped KindMismatch carries the `:nome`, the
        // renderer's `#[error("{0}")]` arm prints it through, and
        // the diagnostic is self-locating.
        let mut c = sample_caixa();
        c.kind = CaixaKind::Biblioteca;
        c.servicos = vec![];
        let err = programs_yaml_entry(&c, &sample_cu_yaml()).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("hello-rio"),
            "kind-mismatch diagnostic must name the offending caixa nome \
             (got: {msg:?})"
        );
        assert!(
            msg.contains("Servico"),
            "diagnostic must name the expected kind (got: {msg:?})"
        );
        assert!(
            msg.contains("Biblioteca"),
            "diagnostic must name the actual kind (got: {msg:?})"
        );
    }

    #[test]
    fn cluster_bundle_kind_mismatch_names_offending_caixa_nome() {
        // The second kind-checking call site in caixa-flux —
        // [`cluster_bundle`] — must surface the same lifted diagnostic
        // shape. Pinning so a future divergence between
        // `programs_yaml_entry` and `cluster_bundle` (e.g. one
        // re-inlines the kind check, the other uses `require_kind`)
        // surfaces here as a test failure rather than as a silent
        // diagnostic regression on the deploy path.
        let mut c = sample_caixa();
        c.kind = CaixaKind::Aplicacao;
        c.servicos = vec![];
        let opts = ClusterBundleOpts::for_caixa(&c, "rio");
        let err = cluster_bundle(&c, &opts).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("hello-rio"),
            "cluster_bundle's kind-mismatch must also name the caixa \
             nome (got: {msg:?})"
        );
        match err {
            Error::NotAServico(km) => {
                assert_eq!(km.nome, "hello-rio");
                assert_eq!(km.expected, CaixaKind::Servico);
                assert_eq!(km.actual, CaixaKind::Aplicacao);
            }
            other => panic!("expected Error::NotAServico, got {other:?}"),
        }
    }

    #[test]
    fn servico_count_mismatch_carries_typed_view_with_nome() {
        // Peer to the [`KindMismatch`]-lift pin above on the V0
        // `:servicos`-singularity axis: a Servico-kind caixa whose
        // `:servicos` list is non-singleton fails
        // [`programs_yaml_entry`] with the renderer's
        // `Error::UnsupportedServicoCount` variant wrapping the typed
        // [`caixa_core::ServicoCountMismatch`] view (carrying the
        // offending caixa's `:nome` + the actual count). Before the
        // lift the variant carried only `usize` — the user had to grep
        // their source tree for which `caixa.lisp` triggered it; after
        // the lift the wrapped typed view names the offending caixa
        // verbatim. Pins both the variant routing (via `#[from]`) and
        // the typed payload so a future refactor can't silently switch
        // back to the raw-`usize` payload (which would regress the
        // shared-shape contract with caixa-helm on the peer
        // `lareira-<nome>` chart-dir path).
        let mut c = sample_caixa();
        c.servicos = vec![
            "servicos/hello-rio.computeunit.yaml".into(),
            "servicos/extra.computeunit.yaml".into(),
        ];
        let err = programs_yaml_entry(&c, &sample_cu_yaml()).unwrap_err();
        match err {
            Error::UnsupportedServicoCount(scm) => {
                assert_eq!(scm.nome, "hello-rio");
                assert_eq!(scm.count, 2);
            }
            other => panic!("expected Error::UnsupportedServicoCount, got {other:?}"),
        }
    }

    #[test]
    fn servico_count_mismatch_diagnostic_names_offending_caixa_nome() {
        // The renderer's `#[error("{0}")] UnsupportedServicoCount(
        // #[from] ServicoCountMismatch)` arm prints the typed view's
        // Display through verbatim, so the offending caixa's `:nome`
        // appears in the rendered diagnostic on both the
        // `programs_yaml_entry` and `cluster_bundle` paths. Pinning the
        // self-locating property end-to-end so a future refactor that
        // re-wraps the variant in a Display impl that drops the
        // `:nome` surfaces here as a test failure rather than as
        // silent fragmentation across the two flux deploy paths. Peer
        // to `cluster_bundle_kind_mismatch_names_offending_caixa_nome`
        // on the sibling V0 Servico-shape axis.
        let mut c = sample_caixa();
        c.servicos = vec![];
        let err = programs_yaml_entry(&c, &sample_cu_yaml()).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("hello-rio"),
            ":servicos-count-mismatch diagnostic must name the offending caixa nome \
             (got: {msg:?})"
        );
        assert!(
            msg.contains("0"),
            "diagnostic must name the actual count (got: {msg:?})"
        );
        assert!(
            msg.contains(":servicos"),
            "diagnostic must name the offending field axis (got: {msg:?})"
        );
    }

    #[test]
    fn cluster_bundle_servico_count_mismatch_carries_typed_view_with_nome() {
        // Peer to `cluster_bundle_kind_mismatch_names_offending_caixa_nome`
        // on the sibling V0 `:servicos`-singularity axis. The second
        // per-Servico renderer entry-point in caixa-flux —
        // [`cluster_bundle`] — must surface the same lifted
        // [`caixa_core::ServicoCountMismatch`] view the peer
        // [`programs_yaml_entry`] path already pins on the sibling V0
        // gate axis. Until this gate landed `cluster_bundle` ran only
        // the [`require_kind`] half of the V0-shape gate pair, so a
        // Servico-kind caixa whose `:servicos` list is non-singleton
        // silently passed the bundle render and the failure surfaced at
        // the chart-render layer (`caixa-helm` refused the same input
        // with `UnsupportedServicoCount`) far from the deploy-path
        // entry-point — the canonical "the V0 invariant is enforced at
        // every per-Servico renderer entry except this one" footgun.
        // Pins both the variant routing (via `#[from]`) and the typed
        // payload so a future refactor that re-inlines a raw count
        // check, or strips the `require_single_servico` call from this
        // path, surfaces here as a test failure rather than as silent
        // fragmentation across the two flux deploy paths.
        let mut c = sample_caixa();
        c.servicos = vec![
            "servicos/hello-rio.computeunit.yaml".into(),
            "servicos/extra.computeunit.yaml".into(),
        ];
        let opts = ClusterBundleOpts::for_caixa(&c, "rio");
        let err = cluster_bundle(&c, &opts).unwrap_err();
        match err {
            Error::UnsupportedServicoCount(scm) => {
                assert_eq!(scm.nome, "hello-rio");
                assert_eq!(scm.count, 2);
            }
            other => panic!("expected Error::UnsupportedServicoCount, got {other:?}"),
        }
    }

    #[test]
    fn cluster_bundle_servico_count_mismatch_diagnostic_names_offending_caixa_nome() {
        // End-to-end Display pin on the [`cluster_bundle`] path —
        // peer of `servico_count_mismatch_diagnostic_names_offending_caixa_nome`
        // on the sibling [`programs_yaml_entry`] path. The rendered
        // diagnostic must name the offending caixa's `:nome`, the
        // actual count, and the `:servicos` field axis verbatim on
        // both per-Servico renderer entry-points in caixa-flux. Peer
        // to `cluster_bundle_kind_mismatch_names_offending_caixa_nome`
        // on the sibling V0 kind-shape axis — both V0 gate arms now
        // pin their self-locating diagnostic shape end-to-end through
        // the bundle path.
        let mut c = sample_caixa();
        c.servicos = vec![];
        let opts = ClusterBundleOpts::for_caixa(&c, "rio");
        let err = cluster_bundle(&c, &opts).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("hello-rio"),
            "cluster_bundle's :servicos-count-mismatch must name the offending caixa nome \
             (got: {msg:?})"
        );
        assert!(
            msg.contains("0"),
            "diagnostic must name the actual count (got: {msg:?})"
        );
        assert!(
            msg.contains(":servicos"),
            "diagnostic must name the offending field axis (got: {msg:?})"
        );
    }

    #[test]
    fn upsert_inserts_new_entry() {
        let initial: serde_yaml::Value = serde_yaml::from_str(
            r#"
enabled: true
defaultNamespace: tatara-system
programs: []
"#,
        )
        .unwrap();
        let entry = programs_yaml_entry(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let (modified, inserted) = upsert_into_programs_yaml(initial, entry).unwrap();
        assert!(inserted, "first time should be insert");
        let arr = modified.get("programs").unwrap().as_sequence().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(
            arr[0].get("name").and_then(|n| n.as_str()),
            Some("hello-rio")
        );
    }

    #[test]
    fn upsert_replaces_existing_entry() {
        let initial: serde_yaml::Value = serde_yaml::from_str(
            r#"
enabled: true
defaultNamespace: tatara-system
programs:
  - name: hello-rio
    namespace: tatara-system
    module:
      source: oci://ghcr.io/pleme-io/hello-rio:v0.0.1
  - name: other
    namespace: tatara-system
    module: { source: github:foo/bar }
"#,
        )
        .unwrap();
        let entry = programs_yaml_entry(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let (modified, inserted) = upsert_into_programs_yaml(initial, entry).unwrap();
        assert!(!inserted, "second time should be replace");
        let arr = modified.get("programs").unwrap().as_sequence().unwrap();
        assert_eq!(arr.len(), 2, "no new entry added");
        let updated_module = arr[0]
            .get("module")
            .unwrap()
            .get("source")
            .and_then(|s| s.as_str());
        assert_eq!(
            updated_module,
            Some("oci://ghcr.io/pleme-io/hello-rio:v0.1.0")
        );
    }

    #[test]
    fn upsert_helmrelease_inserts_under_spec_values_programs() {
        let initial: serde_yaml::Value = serde_yaml::from_str(
            r#"
apiVersion: helm.toolkit.fluxcd.io/v2
kind: HelmRelease
metadata:
  name: rio-fleet-programs
  namespace: tatara-system
spec:
  interval: 30m
  chart:
    spec:
      chart: lareira-fleet-programs
  values:
    enabled: true
    defaultNamespace: tatara-system
    programs:
      - name: existing
        module: { source: github:foo/bar }
"#,
        )
        .unwrap();
        let entry = programs_yaml_entry(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let (modified, inserted) = upsert_into_helmrelease_programs(initial, entry).unwrap();
        assert!(inserted);
        let arr = modified
            .get("spec")
            .unwrap()
            .get("values")
            .unwrap()
            .get("programs")
            .unwrap()
            .as_sequence()
            .unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(
            arr[1].get("name").and_then(|n| n.as_str()),
            Some("hello-rio")
        );
    }

    #[test]
    fn upsert_helmrelease_replaces_existing() {
        let initial: serde_yaml::Value = serde_yaml::from_str(
            r#"
apiVersion: helm.toolkit.fluxcd.io/v2
kind: HelmRelease
metadata: { name: rio-fleet-programs }
spec:
  values:
    programs:
      - name: hello-rio
        module: { source: oci://ghcr.io/pleme-io/hello-rio:v0.0.1 }
      - name: other
        module: { source: github:foo/bar }
"#,
        )
        .unwrap();
        let entry = programs_yaml_entry(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let (modified, inserted) = upsert_into_helmrelease_programs(initial, entry).unwrap();
        assert!(!inserted);
        let arr = modified
            .get("spec")
            .unwrap()
            .get("values")
            .unwrap()
            .get("programs")
            .unwrap()
            .as_sequence()
            .unwrap();
        assert_eq!(arr.len(), 2);
        let updated = arr[0]
            .get("module")
            .unwrap()
            .get("source")
            .and_then(|s| s.as_str());
        assert_eq!(updated, Some("oci://ghcr.io/pleme-io/hello-rio:v0.1.0"));
    }

    #[test]
    fn limits_slot_propagates_into_programs_yaml_entry() {
        use caixa_core::LimitsSpec;
        use std::time::Duration;
        let mut c = sample_caixa();
        c.limits = Some(LimitsSpec {
            memory: Some(64 * 1024 * 1024),
            fuel: Some(1_000_000),
            wall_clock: Some(Duration::from_secs(30)),
            cpu: Some(500),
        });
        let entry = programs_yaml_entry(&c, &sample_cu_yaml()).unwrap();
        let limits = entry.get("limits").expect("limits propagates");
        assert_eq!(limits.get("memory").and_then(|m| m.as_str()), Some("64MiB"));
        assert_eq!(limits.get("cpu").and_then(|m| m.as_str()), Some("500m"));
    }

    #[test]
    fn behavior_slot_propagates_into_programs_yaml_entry() {
        use caixa_core::BehaviorSpec;
        use std::path::PathBuf;
        let mut c = sample_caixa();
        c.behavior = Some(BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_state_change: Some(PathBuf::from("lib/migrations.lisp")),
            ..Default::default()
        });
        let entry = programs_yaml_entry(&c, &sample_cu_yaml()).unwrap();
        let behavior = entry.get("behavior").expect("behavior propagates");
        assert_eq!(
            behavior.get("onInit").and_then(|v| v.as_str()),
            Some("lib/init.lisp")
        );
    }

    #[test]
    fn upgrade_from_slot_propagates_into_programs_yaml_entry() {
        use caixa_core::{UpgradeFromEntry, UpgradeInstruction};
        let mut c = sample_caixa();
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.0.9".into(),
            instructions: vec![UpgradeInstruction::SoftPurge {
                module: "hello-rio-old".into(),
            }],
        }];
        let entry = programs_yaml_entry(&c, &sample_cu_yaml()).unwrap();
        let upgrade_from = entry
            .get("upgradeFrom")
            .and_then(|u| u.as_sequence())
            .expect("upgradeFrom propagates as a sequence");
        assert_eq!(upgrade_from.len(), 1);
        assert_eq!(
            upgrade_from[0].get("from").and_then(|f| f.as_str()),
            Some("0.0.9")
        );
    }

    #[test]
    fn empty_m2_slots_do_not_appear_in_programs_yaml_entry() {
        // Forward-compat invariant: a Servico with no M2 slots emits a
        // programs.yaml entry that's structurally identical to V0
        // (no extra keys).
        let entry = programs_yaml_entry(&sample_caixa(), &sample_cu_yaml()).unwrap();
        assert!(entry.get("limits").is_none());
        assert!(entry.get("behavior").is_none());
        assert!(entry.get("upgradeFrom").is_none());
    }

    #[test]
    fn cluster_bundle_three_files() {
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        assert_eq!(files.len(), 3);
        let names: Vec<_> = files
            .iter()
            .map(|f| f.path.to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"gitrepository.yaml".to_string()));
        assert!(names.contains(&"helmrelease.yaml".to_string()));
        assert!(names.contains(&"kustomization.yaml".to_string()));

        let kust = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from("kustomization.yaml"))
            .unwrap();
        assert!(kust.contents.contains("./clusters/rio/services/hello-rio"));

        let gitrepo = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from("gitrepository.yaml"))
            .unwrap();
        assert!(gitrepo.contents.contains("v0.1.0"));
    }

    #[test]
    fn default_library_name_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::DEFAULT_LIBRARY_NAME` is
        // the single source of truth for the Helm library-chart wrap
        // key the `cluster_bundle` `helmrelease.yaml` template scopes
        // the per-cluster `enabled: true` override under. Pin the
        // equality (and the static-data identity, peer with the
        // sibling `default_namespace_re_export_points_at_caixa_core_canonical`
        // pin) so any local re-introduction of a sibling `pub const
        // DEFAULT_LIBRARY_NAME: &str = "…"` (the canonical drift
        // footgun this lift closes — two production-code consumers of
        // the same load-bearing Helm library-chart name across
        // caixa-helm + caixa-flux, lifted to one re-export at the
        // caixa-core boundary) is a build-time test failure naming
        // the offending drift, not a silent apply-time wrap-key
        // mismatch routing the per-cluster override nowhere. Peer to
        // `caixa_helm::tests::default_library_name_re_export_points_at_caixa_core_canonical`
        // on the sibling renderer crate.
        assert_eq!(DEFAULT_LIBRARY_NAME, caixa_core::DEFAULT_LIBRARY_NAME);
        assert!(
            std::ptr::eq(
                DEFAULT_LIBRARY_NAME.as_ptr(),
                caixa_core::DEFAULT_LIBRARY_NAME.as_ptr(),
            ),
            "DEFAULT_LIBRARY_NAME must be a re-export of caixa_core::DEFAULT_LIBRARY_NAME, \
             not a sibling `pub const` that happens to carry the same string \
             — drift between the two is the canonical footgun this lift closes"
        );
    }

    #[test]
    fn cluster_bundle_helmrelease_values_wrap_key_uses_lifted_constant() {
        // Fail-before-pass-after pin: the rendered `helmrelease.yaml`'s
        // `spec.values.<library>:` wrap key — the scope under which
        // per-cluster overrides like `enabled: true` reach the
        // dependent library chart at `helm template` / `helm install`
        // time — must spell out the lifted [`DEFAULT_LIBRARY_NAME`]
        // verbatim. Before the lift this site carried an inline
        // `pleme-computeunit:` literal in the format string; a future
        // per-edition library-chart rebrand (the substrate forking
        // `pleme-computeunit` to `<registry>-computeunit` for a per-
        // cluster image-registry mirror, or to `aplicacao-computeunit`
        // for the M4 typed-Aplicacao renderer's sibling library chart,
        // or any per-tenant variant the absorption-roadmap names) on
        // the sibling caixa-helm `RenderOpts::library_name` axis
        // without a coordinated edit here would have silently routed
        // the per-cluster override nowhere — the cluster's apply would
        // come up with the library chart's defaults, far from the
        // rebrand commit's source.
        //
        // The pin is structural: parse the rendered YAML and assert
        // the wrap key under `spec.values` equals the lifted constant.
        // A regression that re-introduces an inline literal surfaces
        // as a key mismatch (the inline literal would survive, but
        // the lifted-constant-keyed assertion would fail).
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from("helmrelease.yaml"))
            .expect("helmrelease.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&hr.contents).expect("helmrelease.yaml parses as YAML");
        let values = parsed
            .get("spec")
            .and_then(|s| s.get("values"))
            .and_then(|v| v.as_mapping())
            .expect("spec.values mapping present");
        assert!(
            values
                .get(serde_yaml::Value::String(DEFAULT_LIBRARY_NAME.into()))
                .is_some(),
            "spec.values must wrap under the lifted DEFAULT_LIBRARY_NAME \
             ({DEFAULT_LIBRARY_NAME:?}); a drifted literal here silently \
             routes per-cluster overrides nowhere at helm template time"
        );
        // The wrapped block must carry the canonical `enabled: true`
        // overlay — the per-cluster override the bundle path threads
        // through. Pin the round-trip so a refactor that hoists the
        // overlay out of the wrap can't silently drop it.
        let wrapped = values
            .get(serde_yaml::Value::String(DEFAULT_LIBRARY_NAME.into()))
            .and_then(|v| v.as_mapping())
            .expect("wrapped library mapping");
        assert_eq!(
            wrapped
                .get(serde_yaml::Value::String("enabled".into()))
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn cluster_bundle_helmrelease_wrap_key_pins_canonical_pleme_computeunit_string() {
        // Bridge-arm pin: the lifted [`DEFAULT_LIBRARY_NAME`] constant
        // resolves to the canonical `"pleme-computeunit"` string today,
        // and the rendered `helmrelease.yaml`'s wrap key must spell
        // it out verbatim. Pin the literal here (peer with the
        // [`caixa_helm::tests::values_yaml_wraps_under_pleme_computeunit_key`]
        // canonical-default arm on the chart-render side) so a future
        // rebrand of the lifted constant surfaces here as a coordinated
        // edit-point — same trajectory as the
        // `default_servico_port_constant_pins_canonical_8080_literal`
        // bridge-arm pin in caixa-core.
        assert_eq!(DEFAULT_LIBRARY_NAME, "pleme-computeunit");
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from("helmrelease.yaml"))
            .unwrap();
        assert!(
            hr.contents.contains("pleme-computeunit:"),
            "helmrelease.yaml must spell the canonical library-chart wrap \
             key under spec.values (got: {contents:?})",
            contents = hr.contents
        );
    }

    #[test]
    fn default_flux_system_namespace_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE`
        // is the single source of truth for the FluxCD installation
        // namespace both axes of the rendered `kustomization.yaml`
        // document (`metadata.namespace` and `spec.sourceRef.name`)
        // consume. Pin the equality (and the static-data identity, peer
        // with the sibling
        // `default_namespace_re_export_points_at_caixa_core_canonical` /
        // `default_library_name_re_export_points_at_caixa_core_canonical`
        // pins) so any local re-introduction of a sibling `pub const
        // DEFAULT_FLUX_SYSTEM_NAMESPACE: &str = "…"` (the canonical
        // drift footgun this lift closes — two production-code
        // consumers of the same load-bearing FluxCD-installation-
        // namespace inside the kustomization template, lifted to one
        // re-export at the caixa-core boundary) is a build-time test
        // failure naming the offending drift, not a silent apply-time
        // `Kustomization`-outside-controller-watch-window / dangling-
        // `sourceRef` reconciliation freeze.
        assert_eq!(
            DEFAULT_FLUX_SYSTEM_NAMESPACE,
            caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE
        );
        assert!(
            std::ptr::eq(
                DEFAULT_FLUX_SYSTEM_NAMESPACE.as_ptr(),
                caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE.as_ptr(),
            ),
            "DEFAULT_FLUX_SYSTEM_NAMESPACE must be a re-export of \
             caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE, not a sibling `pub const` \
             that happens to carry the same string — drift between the two is \
             the canonical footgun this lift closes"
        );
    }

    #[test]
    fn cluster_bundle_kustomization_uses_lifted_flux_system_namespace() {
        // Fail-before-pass-after pin: the rendered `kustomization.yaml`'s
        // `metadata.namespace` and `spec.sourceRef.name` axes — the two
        // physical sites the inline `flux-system` literal previously
        // sat at (caixa-flux/src/lib.rs:477, 483 in the prior shape) —
        // must both resolve to the lifted
        // [`DEFAULT_FLUX_SYSTEM_NAMESPACE`] verbatim. Before this lift
        // the kustomization template carried two inline `flux-system`
        // literals; a future per-cluster Flux installation rebrand on
        // either axis without a coordinated edit on the other would have
        // silently emitted a `Kustomization` outside the bootstrap
        // controller's watch window (the `kustomize-controller` watches
        // the installation namespace by default) or a dangling
        // `spec.sourceRef.name` pointing at a `GitRepository` that
        // doesn't exist in the rebranded namespace.
        //
        // The pin is structural: parse the rendered YAML and assert
        // each of the two axes equals the lifted constant by value. A
        // regression that re-introduces an inline literal at either
        // axis surfaces here as a key mismatch (the inline literal
        // would survive, but the lifted-constant-keyed assertion would
        // fail when the lift's value changes).
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let kust = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from("kustomization.yaml"))
            .expect("kustomization.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&kust.contents).expect("kustomization.yaml parses as YAML");
        assert_eq!(
            parsed
                .get("metadata")
                .and_then(|m| m.get("namespace"))
                .and_then(|n| n.as_str()),
            Some(DEFAULT_FLUX_SYSTEM_NAMESPACE),
            "kustomization.yaml metadata.namespace must spell the lifted \
             DEFAULT_FLUX_SYSTEM_NAMESPACE ({DEFAULT_FLUX_SYSTEM_NAMESPACE:?}); \
             a drifted literal here silently places the Kustomization outside \
             the bootstrap kustomize-controller's watch window"
        );
        assert_eq!(
            parsed
                .get("spec")
                .and_then(|s| s.get("sourceRef"))
                .and_then(|r| r.get("name"))
                .and_then(|n| n.as_str()),
            Some(DEFAULT_FLUX_SYSTEM_NAMESPACE),
            "kustomization.yaml spec.sourceRef.name must spell the lifted \
             DEFAULT_FLUX_SYSTEM_NAMESPACE ({DEFAULT_FLUX_SYSTEM_NAMESPACE:?}); \
             a drifted literal here dangles the reference at a GitRepository \
             that doesn't exist in the rebranded installation namespace"
        );
    }

    #[test]
    fn cluster_bundle_kustomization_pins_canonical_flux_system_string() {
        // Bridge-arm pin: the lifted [`DEFAULT_FLUX_SYSTEM_NAMESPACE`]
        // constant resolves to the canonical `"flux-system"` string
        // today, and both rendered `kustomization.yaml` axes must spell
        // it out verbatim. Pin the literal here (peer with the
        // [`default_flux_system_namespace_pins_canonical_value`]
        // canonical-default arm in caixa-core, and with
        // [`cluster_bundle_helmrelease_wrap_key_pins_canonical_pleme_computeunit_string`]
        // on the sibling `DEFAULT_LIBRARY_NAME` axis) so a future
        // rebrand of the lifted constant surfaces here as a coordinated
        // edit-point, same trajectory as the
        // `default_servico_port_constant_pins_canonical_8080_literal`
        // bridge-arm pin in caixa-core.
        assert_eq!(DEFAULT_FLUX_SYSTEM_NAMESPACE, "flux-system");
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let kust = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from("kustomization.yaml"))
            .unwrap();
        assert!(
            kust.contents.contains("namespace: flux-system\n"),
            "kustomization.yaml must spell the canonical FluxCD \
             installation namespace at metadata.namespace (got: {contents:?})",
            contents = kust.contents
        );
        assert!(
            kust.contents.contains("name: flux-system\n"),
            "kustomization.yaml must spell the canonical FluxCD \
             installation namespace at spec.sourceRef.name (got: {contents:?})",
            contents = kust.contents
        );
    }

    #[test]
    fn cluster_bundle_helmrelease_uses_lifted_flux_api_version() {
        // Fail-before-pass-after pin: the rendered `helmrelease.yaml`
        // `apiVersion` axis — the load-bearing Flux v2 CRD-group/version
        // declaration the `helm-controller` watches — must resolve to the
        // lifted [`FLUX_HELMRELEASE_API_VERSION`] verbatim. Before this
        // lift the helmrelease template carried an inline
        // `helm.toolkit.fluxcd.io/v2` literal; a future upstream Flux v3
        // migration on this axis without a coordinated edit on the
        // sibling kustomization `healthChecks[].apiVersion` (the second
        // render-side occurrence the same lift threads through) would
        // have silently routed the rendered `HelmRelease` outside the
        // controller's `Watches` and broken at apply time with a non-
        // self-locating "no kind 'HelmRelease' is registered for version
        // 'helm.toolkit.fluxcd.io/v2beta2'" error. Peer with
        // [`cluster_bundle_kustomization_uses_lifted_flux_system_namespace`]
        // on the sibling [`DEFAULT_FLUX_SYSTEM_NAMESPACE`] lift.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from("helmrelease.yaml"))
            .expect("helmrelease.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&hr.contents).expect("helmrelease.yaml parses as YAML");
        assert_eq!(
            parsed.get("apiVersion").and_then(|n| n.as_str()),
            Some(FLUX_HELMRELEASE_API_VERSION),
            "helmrelease.yaml apiVersion must spell the lifted \
             FLUX_HELMRELEASE_API_VERSION ({FLUX_HELMRELEASE_API_VERSION:?}); \
             a drifted literal here routes the HelmRelease outside the Flux v2 \
             helm-controller's Watches",
        );
    }

    #[test]
    fn cluster_bundle_kustomization_health_check_uses_lifted_flux_api_version() {
        // Sibling-axis pin to
        // `cluster_bundle_helmrelease_uses_lifted_flux_api_version`: the
        // rendered `kustomization.yaml`'s `spec.healthChecks[].apiVersion`
        // axis is the Flux v2 contract pairing the parent Kustomization's
        // per-resource health-gate to the sibling HelmRelease's CRD
        // group/version. Both axes must resolve to the same lifted
        // constant by value — a future upstream Flux v3 migration on
        // either axis without a coordinated edit on the other would
        // have silently dangled the health-check (apply-side: the per-
        // resource health-gate never resolves, the parent Kustomization
        // sits perpetually in `Reconciling`).
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let kust = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from("kustomization.yaml"))
            .expect("kustomization.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&kust.contents).expect("kustomization.yaml parses as YAML");
        let health_checks = parsed
            .get("spec")
            .and_then(|s| s.get("healthChecks"))
            .and_then(|h| h.as_sequence())
            .expect("kustomization.yaml spec.healthChecks present");
        assert!(
            !health_checks.is_empty(),
            "kustomization.yaml spec.healthChecks must carry at least one \
             entry — the rendered Kustomization gates on its sibling \
             HelmRelease's health by construction",
        );
        for (i, entry) in health_checks.iter().enumerate() {
            assert_eq!(
                entry.get("apiVersion").and_then(|n| n.as_str()),
                Some(FLUX_HELMRELEASE_API_VERSION),
                "kustomization.yaml spec.healthChecks[{i}].apiVersion must \
                 spell the lifted FLUX_HELMRELEASE_API_VERSION \
                 ({FLUX_HELMRELEASE_API_VERSION:?}); a drifted literal here \
                 dangles the per-resource health-gate at apply time",
            );
        }
    }

    #[test]
    fn cluster_bundle_helmrelease_pins_canonical_flux_v2_api_version_string() {
        // Bridge-arm pin: the lifted [`FLUX_HELMRELEASE_API_VERSION`]
        // constant resolves to the canonical
        // `"helm.toolkit.fluxcd.io/v2"` string today, and both rendered
        // axes (helmrelease.yaml apiVersion + kustomization.yaml
        // healthChecks[].apiVersion) must spell it out verbatim. Pin the
        // literal here (peer with the
        // [`flux_helmrelease_api_version_pins_canonical_value`] canonical-
        // default arm in caixa-core, and with
        // [`cluster_bundle_kustomization_pins_canonical_flux_system_string`]
        // on the sibling `DEFAULT_FLUX_SYSTEM_NAMESPACE` axis) so a
        // future Flux v3 migration of the lifted constant surfaces here
        // as a coordinated edit-point.
        assert_eq!(FLUX_HELMRELEASE_API_VERSION, "helm.toolkit.fluxcd.io/v2");
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from("helmrelease.yaml"))
            .unwrap();
        assert!(
            hr.contents.contains("apiVersion: helm.toolkit.fluxcd.io/v2\n"),
            "helmrelease.yaml must spell the canonical Flux v2 HelmRelease \
             apiVersion at the top-level apiVersion axis (got: {contents:?})",
            contents = hr.contents,
        );
        let kust = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from("kustomization.yaml"))
            .unwrap();
        assert!(
            kust.contents
                .contains("apiVersion: helm.toolkit.fluxcd.io/v2\n"),
            "kustomization.yaml must spell the canonical Flux v2 HelmRelease \
             apiVersion at spec.healthChecks[].apiVersion (got: {contents:?})",
            contents = kust.contents,
        );
    }

    #[test]
    fn cluster_bundle_default_git_tag_uses_lifted_caixa_core_prefix() {
        // Fail-before-pass-after pin: the [`ClusterBundleOpts::for_caixa`]
        // constructor's default `git_ref: GitRefSpec::Tag(...)` must
        // compose the lifted [`caixa_core::DEFAULT_PUBLISH_TAG_PREFIX`]
        // against the caixa's `:versao` — not an inline `"v"` byte the
        // peer `feira publish` `--prefix` default and this deploy-side
        // default could silently drift on.
        //
        // Until this lift landed the deploy-side carried a `format!("v{}",
        // caixa.versao)` literal while the writer-side (caixa-feira/src/cmd/publish.rs:22)
        // carried a clap `default_value = "v"` literal — two production-code
        // consumers of the same git-tag-naming convention on the same
        // git remote axis, drift-prone by construction. A future
        // Zig-style-tag rebrand on one side (e.g. moving the publisher to
        // `release/<versao>` once a sibling forge convention lands)
        // without a coordinated edit here would silently emit a tag the
        // FluxCD `GitRepository` reconciler can't resolve — the
        // dependent `HelmRelease`'s `chart: sourceRef` would never
        // converge and every per-Servico apply would silently come up
        // with the prior reconciled state, with the failure surfacing
        // far from the rebrand commit's source at
        // `kubectl describe gitrepository` time.
        //
        // Pin the equality on the constructed `GitRefSpec::Tag` body and
        // on the rendered `gitrepository.yaml`'s `ref: { tag: ... }`
        // field so a regression that re-inlines the `"v"` literal at
        // either layer (this constructor or the format-string in
        // [`cluster_bundle`]) surfaces here as a build-time test failure
        // rather than as a silent deploy-time `GitRepository` reconcile
        // loop. Peer to the sibling [`caixa-feira`]
        // `publish_prefix_default_pins_lifted_caixa_core_constant` test
        // closing the same drift on the writer-side.
        let caixa = sample_caixa();
        let opts = ClusterBundleOpts::for_caixa(&caixa, "rio");
        match &opts.git_ref {
            GitRefSpec::Tag(tag) => {
                assert!(
                    tag.starts_with(caixa_core::DEFAULT_PUBLISH_TAG_PREFIX),
                    "default git_ref tag must start with the lifted \
                     caixa_core::DEFAULT_PUBLISH_TAG_PREFIX (got: {tag:?})"
                );
                assert_eq!(
                    tag,
                    &format!(
                        "{prefix}{versao}",
                        prefix = caixa_core::DEFAULT_PUBLISH_TAG_PREFIX,
                        versao = caixa.versao,
                    ),
                    "default git_ref tag must compose the lifted prefix \
                     against the caixa's :versao verbatim"
                );
            }
            other => panic!("expected GitRefSpec::Tag, got {other:?}"),
        }
        let files = cluster_bundle(&caixa, &opts).unwrap();
        let gr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from("gitrepository.yaml"))
            .expect("gitrepository.yaml present");
        let expected_tag = format!(
            "{prefix}{versao}",
            prefix = caixa_core::DEFAULT_PUBLISH_TAG_PREFIX,
            versao = caixa.versao,
        );
        assert!(
            gr.contents.contains(&format!("tag: {expected_tag:?}")),
            "gitrepository.yaml must spell the lifted-prefix-composed tag \
             at ref.tag (expected: {expected_tag:?}, got: {contents:?})",
            contents = gr.contents
        );
    }

    #[test]
    fn flux_gitrepository_api_version_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_GITREPOSITORY_API_VERSION`
        // is the single source of truth for the Flux v2 `GitRepository`
        // CRD-group/version the rendered `gitrepository.yaml` document
        // declares at its `apiVersion` axis. Pin the equality (and the
        // static-data identity, peer with the sibling
        // `default_flux_system_namespace_re_export_points_at_caixa_core_canonical` /
        // `default_library_name_re_export_points_at_caixa_core_canonical`
        // pins) so any local re-introduction of a sibling `pub const
        // FLUX_GITREPOSITORY_API_VERSION: &str = "…"` (the canonical drift
        // footgun this lift closes — one production-code consumer of the
        // load-bearing Flux v2 `GitRepository` CRD-group/version inside
        // the gitrepository template, lifted to one re-export at the
        // caixa-core boundary) is a build-time test failure naming the
        // offending drift, not a silent apply-time `GitRepository`-
        // outside-controller-watch-window reconciliation freeze.
        assert_eq!(
            FLUX_GITREPOSITORY_API_VERSION,
            caixa_core::FLUX_GITREPOSITORY_API_VERSION
        );
        assert!(
            std::ptr::eq(
                FLUX_GITREPOSITORY_API_VERSION.as_ptr(),
                caixa_core::FLUX_GITREPOSITORY_API_VERSION.as_ptr(),
            ),
            "FLUX_GITREPOSITORY_API_VERSION must be a re-export of \
             caixa_core::FLUX_GITREPOSITORY_API_VERSION, not a sibling `pub const` \
             that happens to carry the same string — drift between the two is \
             the canonical footgun this lift closes"
        );
    }

    #[test]
    fn cluster_bundle_gitrepository_uses_lifted_flux_api_version() {
        // Fail-before-pass-after pin: the rendered `gitrepository.yaml`
        // `apiVersion` axis — the load-bearing Flux v2 CRD-group/version
        // declaration the `source-controller` watches — must resolve to the
        // lifted [`FLUX_GITREPOSITORY_API_VERSION`] verbatim. Before this
        // lift the gitrepository template carried an inline
        // `source.toolkit.fluxcd.io/v1` literal; a future upstream Flux v3
        // migration on this axis without a coordinated edit on the sibling
        // [`FLUX_HELMRELEASE_API_VERSION`] axis (the Flux v2 controller-
        // triple shares the `.toolkit.fluxcd.io` root and promotes together
        // on each major bump) would have silently routed the rendered
        // `GitRepository` outside the controller's `Watches` and broken at
        // apply time with a non-self-locating "no kind 'GitRepository' is
        // registered for version 'source.toolkit.fluxcd.io/v1beta2'"
        // error. Peer with
        // [`cluster_bundle_helmrelease_uses_lifted_flux_api_version`] on
        // the sibling [`FLUX_HELMRELEASE_API_VERSION`] lift.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let gr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from("gitrepository.yaml"))
            .expect("gitrepository.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&gr.contents).expect("gitrepository.yaml parses as YAML");
        assert_eq!(
            parsed.get("apiVersion").and_then(|n| n.as_str()),
            Some(FLUX_GITREPOSITORY_API_VERSION),
            "gitrepository.yaml apiVersion must spell the lifted \
             FLUX_GITREPOSITORY_API_VERSION ({FLUX_GITREPOSITORY_API_VERSION:?}); \
             a drifted literal here routes the GitRepository outside the Flux v2 \
             source-controller's Watches",
        );
    }

    #[test]
    fn cluster_bundle_gitrepository_pins_canonical_flux_v1_api_version_string() {
        // Bridge-arm pin: the lifted [`FLUX_GITREPOSITORY_API_VERSION`]
        // constant resolves to the canonical
        // `"source.toolkit.fluxcd.io/v1"` string today, and the rendered
        // `gitrepository.yaml`'s `apiVersion` axis must spell it out
        // verbatim. Pin the literal here (peer with the
        // [`flux_gitrepository_api_version_pins_canonical_value`]
        // canonical-default arm in caixa-core, and with
        // [`cluster_bundle_helmrelease_pins_canonical_flux_v2_api_version_string`]
        // on the sibling `FLUX_HELMRELEASE_API_VERSION` axis) so a future
        // Flux v3 migration of the lifted constant surfaces here as a
        // coordinated edit-point.
        assert_eq!(
            FLUX_GITREPOSITORY_API_VERSION,
            "source.toolkit.fluxcd.io/v1"
        );
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let gr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from("gitrepository.yaml"))
            .unwrap();
        assert!(
            gr.contents
                .contains("apiVersion: source.toolkit.fluxcd.io/v1\n"),
            "gitrepository.yaml must spell the canonical Flux v2 GitRepository \
             apiVersion at the top-level apiVersion axis (got: {contents:?})",
            contents = gr.contents,
        );
    }

    #[test]
    fn flux_kustomization_api_version_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_KUSTOMIZATION_API_VERSION`
        // is the single source of truth for the Flux v2 `Kustomization`
        // CRD-group/version the rendered `kustomization.yaml` document
        // declares at its `apiVersion` axis. Pin the equality (and the
        // static-data identity, peer with the sibling
        // [`flux_gitrepository_api_version_re_export_points_at_caixa_core_canonical`]
        // / [`default_flux_system_namespace_re_export_points_at_caixa_core_canonical`]
        // pins) so any local re-introduction of a sibling `pub const
        // FLUX_KUSTOMIZATION_API_VERSION: &str = "…"` (the canonical drift
        // footgun this lift closes — one production-code consumer of the
        // load-bearing Flux v2 `Kustomization` CRD-group/version inside
        // the kustomization template, lifted to one re-export at the
        // caixa-core boundary) is a build-time test failure naming the
        // offending drift, not a silent apply-time `Kustomization`-
        // outside-controller-watch-window reconciliation freeze.
        assert_eq!(
            FLUX_KUSTOMIZATION_API_VERSION,
            caixa_core::FLUX_KUSTOMIZATION_API_VERSION
        );
        assert!(
            std::ptr::eq(
                FLUX_KUSTOMIZATION_API_VERSION.as_ptr(),
                caixa_core::FLUX_KUSTOMIZATION_API_VERSION.as_ptr(),
            ),
            "FLUX_KUSTOMIZATION_API_VERSION must be a re-export of \
             caixa_core::FLUX_KUSTOMIZATION_API_VERSION, not a sibling `pub const` \
             that happens to carry the same string — drift between the two is \
             the canonical footgun this lift closes"
        );
    }

    #[test]
    fn cluster_bundle_kustomization_uses_lifted_flux_api_version() {
        // Fail-before-pass-after pin: the rendered `kustomization.yaml`
        // top-level `apiVersion` axis — the load-bearing Flux v2
        // CRD-group/version declaration the `kustomize-controller`
        // watches — must resolve to the lifted
        // [`FLUX_KUSTOMIZATION_API_VERSION`] verbatim. Before this lift
        // the kustomization template carried an inline
        // `kustomize.toolkit.fluxcd.io/v1` literal; a future upstream
        // Flux v3 migration on this axis without a coordinated edit on
        // the sibling [`FLUX_HELMRELEASE_API_VERSION`] /
        // [`FLUX_GITREPOSITORY_API_VERSION`] axes (the Flux v2
        // controller triplet shares the `.toolkit.fluxcd.io` root and
        // promotes together on each major bump) would have silently
        // routed the rendered `Kustomization` outside the controller's
        // `Watches` and broken at apply time with a non-self-locating
        // "no kind 'Kustomization' is registered for version
        // 'kustomize.toolkit.fluxcd.io/v1beta2'" error. Peer with
        // [`cluster_bundle_helmrelease_uses_lifted_flux_api_version`] /
        // [`cluster_bundle_gitrepository_uses_lifted_flux_api_version`]
        // on the sibling Flux-CRD-axis lifts.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let kz = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from("kustomization.yaml"))
            .expect("kustomization.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&kz.contents).expect("kustomization.yaml parses as YAML");
        assert_eq!(
            parsed.get("apiVersion").and_then(|n| n.as_str()),
            Some(FLUX_KUSTOMIZATION_API_VERSION),
            "kustomization.yaml apiVersion must spell the lifted \
             FLUX_KUSTOMIZATION_API_VERSION ({FLUX_KUSTOMIZATION_API_VERSION:?}); \
             a drifted literal here routes the Kustomization outside the Flux v2 \
             kustomize-controller's Watches",
        );
    }

    #[test]
    fn cluster_bundle_kustomization_pins_canonical_flux_v1_api_version_string() {
        // Bridge-arm pin: the lifted [`FLUX_KUSTOMIZATION_API_VERSION`]
        // constant resolves to the canonical
        // `"kustomize.toolkit.fluxcd.io/v1"` string today, and the
        // rendered `kustomization.yaml`'s top-level `apiVersion` axis
        // must spell it out verbatim. Pin the literal here (peer with
        // the [`flux_kustomization_api_version_pins_canonical_value`]
        // canonical-default arm in caixa-core, and with
        // [`cluster_bundle_helmrelease_pins_canonical_flux_v2_api_version_string`]
        // / [`cluster_bundle_gitrepository_pins_canonical_flux_v1_api_version_string`]
        // on the sibling `FLUX_HELMRELEASE_API_VERSION` /
        // `FLUX_GITREPOSITORY_API_VERSION` axes) so a future Flux v3
        // migration of the lifted constant surfaces here as a
        // coordinated edit-point.
        assert_eq!(
            FLUX_KUSTOMIZATION_API_VERSION,
            "kustomize.toolkit.fluxcd.io/v1"
        );
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let kz = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from("kustomization.yaml"))
            .unwrap();
        assert!(
            kz.contents
                .contains("apiVersion: kustomize.toolkit.fluxcd.io/v1\n"),
            "kustomization.yaml must spell the canonical Flux v2 Kustomization \
             apiVersion at the top-level apiVersion axis (got: {contents:?})",
            contents = kz.contents,
        );
    }
}
