//! caixa-helm — typed renderer that emits a per-program `lareira-<name>`
//! Helm chart from a [`Caixa`] manifest plus its `servicos/<name>.computeunit.yaml`.
//!
//! ## Output shape
//!
//! Every chart emitted here mirrors the canonical
//! `pleme-io/helmworks/charts/lareira-<name>/` layout, which is *thin*:
//!
//!   Chart.yaml      ; metadata + dependency on pleme-computeunit
//!   values.yaml     ; pleme-computeunit values block (the typed L2 ComputeUnit shape)
//!   README.md       ; one-line elevator pitch for the chart
//!
//! There are no `templates/` — the rendering is delegated to the
//! `pleme-computeunit` library chart in helmworks (per `theory/META-FRAMEWORK.md`
//! §I, Layer 3 → Layer 2 transformation). caixa-helm's job is to derive the
//! values block from a Caixa, not to render Kubernetes objects directly.
//!
//! ## Why a separate crate
//!
//! Same pattern as [`caixa_flake`] (renders flake.nix) and [`caixa_pangea`]
//! (renders pangea Ruby) — `caixa-<target>` crates take a typed Caixa and
//! emit the canonical source for `<target>`. Naming is uniform across the
//! workspace.
//!
//! ## V0 contract
//!
//! ```rust,ignore
//! use caixa_core::Caixa;
//! use caixa_helm::{ChartDir, render_chart_for_servico};
//!
//! let caixa: Caixa = Caixa::from_lisp(src)?;
//! let cu_yaml: serde_yaml::Value =
//!     serde_yaml::from_str(std::fs::read_to_string("servicos/hello-rio.computeunit.yaml")?)?;
//! let dir: ChartDir = render_chart_for_servico(&caixa, &cu_yaml)?;
//! dir.write_to(std::path::Path::new("/tmp/lareira-hello-rio"))?;
//! ```
//!
//! ## What this is NOT
//!
//! - Not a chart for the `caixa-operator` itself — that lives in
//!   `pleme-io/caixa/operator-chart/`.
//! - Not a Helm CLI wrapper — emitting bytes only; consumers (`feira chart`,
//!   eventually) drive the I/O.
//! - Not a renderer of K8s resources — `pleme-computeunit` library chart owns
//!   the templates that turn this values block into ComputeUnit + Service +
//!   ScaledObject + ConfigMap.

#![allow(clippy::module_name_repetitions)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use caixa_core::{Caixa, CaixaKind, lareira_chart_name};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors caixa-helm can raise.
#[derive(Debug, Error)]
pub enum Error {
    /// The caixa's `:kind` doesn't match what `caixa-helm` targets
    /// (this renderer only emits per-program `lareira-<nome>` charts
    /// for `:kind Servico`). Lifted from a prior `NotAServico(CaixaKind)`
    /// arm to wrap [`caixa_core::KindMismatch`] so the diagnostic
    /// names the offending caixa's `:nome` (not just its kind),
    /// shared verbatim with `caixa-flux` and `caixa-mesh`.
    #[error("{0}")]
    NotAServico(#[from] caixa_core::KindMismatch),
    /// The caixa's `:servicos` list doesn't carry exactly one entry —
    /// the V0 contract every Servico-kind caixa satisfies (one
    /// ComputeUnit YAML pointer per Servico, matching the one Helm
    /// chart this renderer emits). Lifted from a prior
    /// `UnsupportedServicoCount(usize)` arm to wrap
    /// [`caixa_core::ServicoCountMismatch`] so the diagnostic names
    /// the offending caixa's `:nome` (not just the count), shared
    /// verbatim with `caixa-flux` (the peer per-Servico renderer
    /// running the same V0 invariant on the programs.yaml-entry axis).
    #[error("{0}")]
    UnsupportedServicoCount(#[from] caixa_core::ServicoCountMismatch),
    #[error("computeunit yaml missing required field: {0}")]
    MissingField(&'static str),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("render: {0}")]
    Render(#[from] caixa_core::RenderError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// One file in the rendered chart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartFile {
    pub path: PathBuf,
    pub contents: String,
}

/// The rendered chart — a flat list of files, plus the chart name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartDir {
    /// Chart name — e.g. `lareira-hello-rio`. Used as the output dir name.
    pub name: String,
    pub files: Vec<ChartFile>,
}

impl ChartDir {
    /// Write every file to `<dest>/<self.name>/`. Creates parent dirs.
    pub fn write_to(&self, dest: &Path) -> Result<(), Error> {
        let root = dest.join(&self.name);
        std::fs::create_dir_all(&root)?;
        for f in &self.files {
            let target = root.join(&f.path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, &f.contents)?;
        }
        Ok(())
    }
}

/// Top-level `Chart.yaml` shape for a generated lareira-<name> chart.
///
/// Mirrors `helmworks/charts/lareira-hello-world/Chart.yaml` 1:1 in
/// structural slots — versions, deps, keywords, maintainers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChartYaml {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub chart_type: String,
    pub version: String,
    #[serde(rename = "appVersion")]
    pub app_version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub maintainers: Vec<Maintainer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<String>,
    pub dependencies: Vec<ChartDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Maintainer {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChartDependency {
    pub name: String,
    pub version: String,
    pub repository: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

/// Repository for the `pleme-computeunit` library chart. Defaults to the
/// helmworks file:// path used by lareira-* charts; consumers can override
/// via `RenderOpts::library_repo` to point at the published OCI registry.
pub const DEFAULT_LIBRARY_REPO: &str = "file://../pleme-computeunit";
pub const DEFAULT_LIBRARY_VERSION: &str = "~0.1.0";
/// Canonical Helm library-chart name every `lareira-<nome>` chart depends
/// on — re-export of the lifted [`caixa_core::DEFAULT_LIBRARY_NAME`] so
/// the load-bearing string lives in exactly one place across every
/// caixa renderer (caixa-helm's `RenderOpts::library_name` default
/// here + caixa-flux's `cluster_bundle` `helmrelease.yaml` wrap key).
/// A future per-edition library-chart fork — every entry on the
/// absorption-roadmap that names a per-cluster / per-namespace /
/// per-tenant variant of the canonical library chart — reaches both
/// consumers through one `&'static str` by construction. Same shape
/// as the [`caixa_core::DEFAULT_NAMESPACE`] (a085b26) /
/// [`caixa_core::DEFAULT_SERVICO_PORT`] (1e22add) lifts on the peer
/// canonical-K8s-axis-constant surface.
pub use caixa_core::DEFAULT_LIBRARY_NAME;

/// Canonical Helm 3 `Chart.yaml` `apiVersion` every rendered
/// `lareira-<nome>` chart declares. Re-export of the lifted
/// [`caixa_core::HELM_CHART_API_VERSION`] so the Helm-side
/// chart-schema apiVersion — the discriminator the Helm binary's
/// chart-schema parser (`helm dependency build`, `helm lint`,
/// `helm template`) consults to select the schema that reads the
/// rendered Chart.yaml — lives in exactly one place across every
/// caixa renderer. The single production-code call site consuming
/// it is [`build_chart_yaml`]'s `api_version` field assignment; a
/// drifted local `pub const HELM_CHART_API_VERSION: &str = "…"` at
/// this crate (or any sibling per-chart-schema renderer the
/// absorption roadmap acknowledges — the future per-Aplicacao
/// library chart, the future per-cluster snapshot chart) would
/// silently reroute the rendered Chart.yaml through a stale
/// chart-schema parser at `helm template` time far from the
/// rebrand commit's source, so the equality + `&'static` static-data
/// identity pin
/// (`helm_chart_api_version_re_export_points_at_caixa_core_canonical`)
/// closes the drift footgun at caixa-helm build time. Same shape as
/// the [`DEFAULT_LIBRARY_NAME`] / [`KUBE_KEY_SPEC`] re-exports on the
/// sibling canonical-Helm-load-bearing-string / canonical-K8s-CR-key
/// axes.
pub use caixa_core::HELM_CHART_API_VERSION;

/// Canonical Helm 3 `Chart.yaml` `type` field per-chart-kind
/// discriminator scalar-value every rendered `lareira-<nome>` chart
/// declares. Re-export of the lifted
/// [`caixa_core::HELM_CHART_TYPE_APPLICATION`] so the Helm chart-schema
/// per-chart-kind discriminator — the scalar Helm's per-release install-
/// shape dispatch loop keys off to select the per-chart-kind install
/// pathway — lives in exactly one place across every caixa renderer.
/// The single production-code call site consuming it is
/// [`build_chart_yaml`]'s `chart_type` field assignment (the sole
/// emitter site the prior inline `"application".into()` literal sat at);
/// a drifted local `pub const HELM_CHART_TYPE_APPLICATION: &str = "…"`
/// at this crate (or any sibling per-chart renderer the absorption
/// roadmap acknowledges — the future per-Aplicacao library chart, the
/// future per-cluster snapshot chart) would surface as one of two
/// silent failure modes at `helm install` time: a value outside the
/// schema's admitted set (`{"application", "library"}`) that Helm's
/// chart-schema parser silently treats as the default `application`
/// shape (masking the schema violation with no process-log signal), or
/// an accidental collapse onto the sibling `"library"` shape that Helm
/// refuses to install directly ("Error: library charts cannot be
/// installed") with no field naming the chart-kind-drift root cause.
/// The equality + `&'static` static-data identity pin
/// (`helm_chart_type_application_re_export_points_at_caixa_core_canonical`)
/// closes the drift footgun at caixa-helm build time. Peer to the
/// [`HELM_CHART_API_VERSION`] re-export on the sibling canonical-Helm-
/// chart-schema-axis — completes the per-Chart.yaml `(apiVersion, type)`
/// canonical-scalar-axis re-export pair every rendered `lareira-<nome>`
/// chart declares at its top-level Chart.yaml body.
pub use caixa_core::HELM_CHART_TYPE_APPLICATION;

/// Canonical Helm 3 per-chart-directory metadata-file filename every
/// rendered `lareira-<nome>` chart carries at its top-level directory —
/// re-export of the lifted [`caixa_core::HELM_CHART_YAML_FILENAME`] so
/// the fixed lookup name Helm's chart-schema parser (`helm dependency
/// build`, `helm lint`, `helm template`, `helm install`) consults at
/// chart-open time to locate the per-chart schema-body scalars
/// ([`HELM_CHART_API_VERSION`], [`HELM_CHART_TYPE_APPLICATION`], the
/// name/version/dependencies fields) lives in exactly one place across
/// every caixa renderer. The single production-code call site
/// consuming it is [`render_chart_for_servico`]'s `ChartDir` assembly
/// where the metadata file's per-`ChartFile` `path` axis is set (the
/// sole emitter site the prior inline `PathBuf::from("Chart.yaml")`
/// literal sat at); every test-side round-trip navigator that reaches
/// into the rendered `ChartDir` by the metadata filename (the
/// per-chart-metadata-field sweep tests +
/// [`ChartDir::write_to`] post-write existence pin) now consults the
/// same `&'static str`, so a rebrand of the Helm 3 metadata-file axis
/// (any per-fork `Chartfile.yaml` / Helm 4 metadata-file rename the
/// upstream packaging spec might adopt) lands at one const and reaches
/// every consumer by construction. A drifted local `pub const
/// HELM_CHART_YAML_FILENAME: &str = "…"` at this crate — the canonical
/// drift footgun where a sibling local `pub const` could happen to
/// carry the same string at the source while pointing at a different
/// `&'static` allocation — surfaces as one of two silent failure modes
/// at chart-consumption time: Helm's chart-schema parser refuses to
/// open the rendered chart-directory ("Error: Chart.yaml file is
/// missing") far from the drift commit, or the sibling
/// [`caixa_flux::cluster_bundle`]'s future per-chart-directory
/// resolver — a per-cluster snapshot bundle that re-lists the
/// chart-dir contents by filename — silently returns `None` at
/// cluster-side `feira app deploy` time. The equality + `&'static`
/// static-data identity pin
/// (`helm_chart_yaml_filename_re_export_points_at_caixa_core_canonical`)
/// closes the drift footgun at caixa-helm build time. Peer to the
/// [`HELM_CHART_API_VERSION`] / [`HELM_CHART_TYPE_APPLICATION`]
/// re-exports on the sibling canonical-Helm-chart-schema-body-axis
/// surface — completes the per-`lareira-<nome>`-chart-directory
/// `(filename, apiVersion, type)` canonical-scalar-axis re-export
/// triple every rendered chart declares at its top-level metadata file.
pub use caixa_core::HELM_CHART_YAML_FILENAME;

/// Canonical K8s CR top-level `spec` key. Re-export of the canonical
/// [`caixa_core::KUBE_KEY_SPEC`] so the per-kind body key lives in
/// exactly one place across every caixa renderer — caixa-helm's
/// `build_values_yaml` (the upstream ComputeUnit YAML's `spec.*` axis
/// the rendered `lareira-<nome>` chart's values block re-routes
/// through the library alias) now consults the same `&'static str` as
/// the peer caixa-flux / caixa-mesh renderers' `KUBE_KEY_SPEC`
/// re-exports. The prior inline `"spec"` literal at the production-
/// code call site would have let a typo (e.g. `"Spec"`, `"specs"`,
/// `"spec_"`) silently emit a values block that drops every typed
/// ComputeUnit-side field (`module`, `trigger`, `capabilities`,
/// `resources`, `serviceAccount`) at the rendered chart's landing
/// site — the `Error::MissingField("spec")` diagnostic now threads
/// the same `&'static str` through the diagnostic surface so the
/// error message stays byte-identical to the key it failed to find.
/// Same shape as the [`DEFAULT_LIBRARY_NAME`] re-export on the
/// sibling canonical-Helm-load-bearing-string axis.
pub use caixa_core::KUBE_KEY_SPEC;

/// Canonical `pleme-computeunit` library-chart values-block enable-toggle
/// key — re-export of the lifted [`caixa_core::HELM_VALUES_KEY_ENABLED`]
/// so the values-block toggle every rendered `lareira-<nome>` chart's
/// values.yaml carries under its [`DEFAULT_LIBRARY_NAME`] wrap key lives
/// in exactly one place across every caixa renderer. The single
/// production-code call site consuming it is [`build_values_yaml`]'s
/// `block.insert(HELM_VALUES_KEY_ENABLED.to_string(), …)` (formerly an
/// inline `"enabled".to_string()` literal at `caixa-helm/src/lib.rs:389`);
/// the peer test-fixture navigators pinning the default-off round-trip
/// (`values_yaml_wraps_under_pleme_computeunit_key`,
/// `values_yaml_wrap_key_follows_library_name_override`) also consult the
/// re-export so a rebrand of the library-chart's per-values enable-toggle
/// axis lands at one const and reaches every consumer by construction.
/// A drifted local `pub const HELM_VALUES_KEY_ENABLED: &str = "…"` (or
/// any sibling per-renderer variant that inlined a stale
/// `"enabled"` / `"enable"` / `"disabled"` literal) would silently emit
/// a values block whose per-values enable-toggle lands under one key
/// while [`caixa_flux::cluster_bundle`]'s `HelmRelease`
/// `spec.values.<library>.enabled` per-cluster override lands under
/// another — Helm's per-values merge treats them as sibling scalars, the
/// enable-toggle the library chart's own template consults never sees the
/// flip, and the workload silently comes up with the library chart's
/// admission-time defaults instead of the per-cluster override the
/// operator set. Same shape as the [`DEFAULT_LIBRARY_NAME`] /
/// [`KUBE_KEY_SPEC`] / [`HELM_CHART_API_VERSION`] re-exports on the
/// sibling canonical-Helm-load-bearing-string / canonical-K8s-CR-body-
/// key / canonical-Helm-chart-schema-apiVersion axes.
pub use caixa_core::HELM_VALUES_KEY_ENABLED;

/// Knobs that don't come from the Caixa manifest.
#[derive(Debug, Clone)]
pub struct RenderOpts {
    /// Where the library chart lives. Default = `file://../pleme-computeunit`.
    pub library_repo: String,
    pub library_version: String,
    pub library_name: String,
    /// Whether the rendered values block is `enabled: false` by default
    /// (matching `lareira-hello-world` so cluster operators flip it on
    /// per-cluster). Default: `false` (i.e. enabled-flag set to false).
    pub enabled_default: bool,
}

impl Default for RenderOpts {
    fn default() -> Self {
        Self {
            library_repo: DEFAULT_LIBRARY_REPO.into(),
            library_version: DEFAULT_LIBRARY_VERSION.into(),
            library_name: DEFAULT_LIBRARY_NAME.into(),
            enabled_default: false,
        }
    }
}

/// Render a per-program lareira-<name> chart from a Caixa Servico + its
/// loaded ComputeUnit YAML.
///
/// The ComputeUnit YAML is passed in as a `serde_yaml::Value` because the
/// authoritative schema lives in the wasm-operator's CRD — we don't want
/// caixa-helm to drift from that schema. It's enough that we can locate
/// `spec` and pass it through.
pub fn render_chart_for_servico(
    caixa: &Caixa,
    computeunit_yaml: &serde_yaml::Value,
) -> Result<ChartDir, Error> {
    render_chart_for_servico_with(caixa, computeunit_yaml, &RenderOpts::default())
}

/// `render_chart_for_servico` with explicit options.
pub fn render_chart_for_servico_with(
    caixa: &Caixa,
    computeunit_yaml: &serde_yaml::Value,
    opts: &RenderOpts,
) -> Result<ChartDir, Error> {
    caixa_core::require_kind(caixa, CaixaKind::Servico)?;
    caixa_core::require_single_servico(caixa)?;

    let chart_name = lareira_chart_name(&caixa.nome);
    let chart_yaml = build_chart_yaml(caixa, &chart_name, opts);
    let values_yaml = build_values_yaml(caixa, computeunit_yaml, opts)?;
    let readme = build_readme(caixa, &chart_name);

    Ok(ChartDir {
        name: chart_name,
        files: vec![
            ChartFile {
                path: PathBuf::from(HELM_CHART_YAML_FILENAME),
                contents: serde_yaml::to_string(&chart_yaml)?,
            },
            ChartFile {
                path: PathBuf::from("values.yaml"),
                contents: values_yaml,
            },
            ChartFile {
                path: PathBuf::from("README.md"),
                contents: readme,
            },
        ],
    })
}

fn build_chart_yaml(caixa: &Caixa, chart_name: &str, opts: &RenderOpts) -> ChartYaml {
    let description = caixa
        .descricao
        .clone()
        .unwrap_or_else(|| format!("Generated chart for caixa Servico {}", caixa.nome));
    let keywords: Vec<String> = caixa
        .etiquetas
        .iter()
        .cloned()
        .chain([
            "lareira".to_string(),
            "wasm".to_string(),
            "tatara-lisp".to_string(),
            "caixa-servico".to_string(),
        ])
        .collect::<Vec<_>>()
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let maintainers = caixa
        .autores
        .iter()
        .map(|a| Maintainer {
            name: a.clone(),
            email: None,
        })
        .collect();
    ChartYaml {
        api_version: HELM_CHART_API_VERSION.into(),
        name: chart_name.into(),
        description,
        chart_type: HELM_CHART_TYPE_APPLICATION.into(),
        version: caixa.versao.clone(),
        app_version: caixa.versao.clone(),
        keywords,
        maintainers,
        home: caixa.repositorio.clone(),
        dependencies: vec![ChartDependency {
            name: opts.library_name.clone(),
            version: opts.library_version.clone(),
            repository: opts.library_repo.clone(),
            alias: None,
        }],
    }
}

fn build_values_yaml(
    caixa: &Caixa,
    computeunit_yaml: &serde_yaml::Value,
    opts: &RenderOpts,
) -> Result<String, Error> {
    // The library chart consumes its values under the key matching its
    // Helm chart `dependencies[].name` (Helm's per-dep alias convention
    // — when no `alias:` is set on the dependency, values are scoped
    // under the dependency's `name`). This renderer wires both axes
    // through the same `opts.library_name`: the chart's dep `name:`
    // (build_chart_yaml at line 277) and this site's values wrap key
    // both consult one `&str`, so a future fork that overrides
    // `RenderOpts::library_name` to point at `acme-computeunit` /
    // `pleme-computeunit-mirror` / the future per-edition library name
    // reaches both axes by construction. Until this lift landed the
    // wrap key was hardcoded `"pleme-computeunit"` while the dep name
    // followed `opts.library_name`, so an override silently emitted
    // values keyed under one name (the literal) while the rendered
    // Chart.yaml's dep was declared under another (the override) —
    // Helm's per-dep values router would route nothing to the
    // configured dependency at `helm template` / `helm install` time,
    // and every typed value the values block carries (`enabled`,
    // `module`, `trigger`, the M2 overlay's `:limits`/`:behavior`/
    // `:upgrade-from`) would silently no-op at the rendered chart's
    // landing site. The wrap key now reads from the same `&str` the
    // dep name reads from, structurally closing the drift footgun
    // peer with the [`caixa_core::DEFAULT_NAMESPACE`] /
    // [`caixa_core::DEFAULT_SERVICO_PORT`] lifts on the sibling
    // canonical-K8s-axis constants (where two production-code call
    // sites of the same load-bearing value would drift apart on
    // any rebrand without a shared source of truth).
    let library_alias = opts.library_name.as_str();
    let spec = computeunit_yaml
        .get(KUBE_KEY_SPEC)
        .ok_or(Error::MissingField(KUBE_KEY_SPEC))?
        .clone();

    // Prepend a comment header so the file is human-friendly.
    let header = format!(
        "# Auto-generated by caixa-helm from caixa.lisp + servicos/{nome}.computeunit.yaml.\n\
         # Edits to this file are overwritten by `feira chart`.\n\
         #\n\
         # `{library_alias}:` is the alias under which the library chart\n\
         # in pleme-io/helmworks/charts/{library_alias} consumes its values.\n\n",
        nome = caixa.nome
    );

    let mut block = BTreeMap::new();
    block.insert(
        HELM_VALUES_KEY_ENABLED.to_string(),
        serde_yaml::Value::Bool(opts.enabled_default),
    );
    if let serde_yaml::Value::Mapping(map) = spec {
        for (k, v) in map {
            if let Some(s) = k.as_str() {
                block.insert(s.to_string(), v);
            }
        }
    }

    // M2 typed-substrate slots — propagate from caixa.lisp into the
    // rendered values block so the library chart (and the operator
    // reading the rendered ComputeUnit) sees them. Spec values from
    // computeunit.yaml win over duplicates in caixa.lisp (or_insert
    // semantics). Shared with caixa-flux::programs_yaml_entry via
    // caixa_core::render::servico_m2_overlay so both renderers agree
    // on key naming + emptiness rules + serialization-error handling.
    for (key, value) in caixa_core::servico_m2_overlay(caixa)? {
        block.entry(key.to_string()).or_insert(value);
    }

    let mut wrapped = serde_yaml::Mapping::new();
    wrapped.insert(
        serde_yaml::Value::String(library_alias.into()),
        serde_yaml::to_value(block)?,
    );
    let body = serde_yaml::to_string(&serde_yaml::Value::Mapping(wrapped))?;
    Ok(format!("{header}{body}"))
}

fn build_readme(caixa: &Caixa, chart_name: &str) -> String {
    let descricao = caixa
        .descricao
        .clone()
        .unwrap_or_else(|| format!("caixa Servico {}", caixa.nome));
    format!(
        "# {chart_name}\n\
         \n\
         {descricao}\n\
         \n\
         ## Origin\n\
         \n\
         Generated by `caixa-helm` from `{repo}/caixa.lisp` v{versao}.\n\
         Edits here are overwritten by `feira chart`.\n\
         \n\
         ## Install\n\
         \n\
         ```bash\n\
         helm dependency build\n\
         helm template {chart_name} . --values values.yaml\n\
         ```\n\
         \n\
         ## License\n\
         \n\
         {license}.\n",
        chart_name = chart_name,
        descricao = descricao,
        repo = caixa
            .repositorio
            .clone()
            .unwrap_or_else(|| caixa.nome.clone()),
        versao = caixa.versao,
        license = caixa.licenca.clone().unwrap_or_else(|| "MIT".into()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use caixa_core::{Caixa, CaixaKind, M2_KEY_BEHAVIOR, M2_KEY_LIMITS, M2_KEY_UPGRADE_FROM};

    fn sample_caixa() -> Caixa {
        Caixa {
            nome: "hello-rio".into(),
            versao: "0.1.0".into(),
            kind: CaixaKind::Servico,
            edicao: Some("2026".into()),
            descricao: Some("Canonical Rust→wasm32-wasip2 caixa Servico.".into()),
            repositorio: Some("github:pleme-io/hello-rio".into()),
            licenca: Some("MIT".into()),
            autores: vec!["pleme-io".into()],
            etiquetas: vec!["hello-world".into(), "wasm".into(), "rust".into()],
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
spec:
  module:
    source: oci://ghcr.io/pleme-io/hello-rio:v0.1.0
  trigger:
    service:
      port: 8080
      paths: ["/", "/hello", "/healthz"]
      breathability:
        enabled: true
        minReplicas: 0
        maxReplicas: 5
        cooldownPeriod: 600
  capabilities:
    - http-in:0.0.0.0:8080
    - env
"#,
        )
        .unwrap()
    }

    #[test]
    fn renders_three_files() {
        let dir = render_chart_for_servico(&sample_caixa(), &sample_cu_yaml()).unwrap();
        assert_eq!(dir.name, "lareira-hello-rio");
        let names: Vec<_> = dir
            .files
            .iter()
            .map(|f| f.path.to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&HELM_CHART_YAML_FILENAME.to_string()));
        assert!(names.contains(&"values.yaml".to_string()));
        assert!(names.contains(&"README.md".to_string()));
    }

    #[test]
    fn chart_yaml_metadata_propagates() {
        let dir = render_chart_for_servico(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let chart_file = dir
            .files
            .iter()
            .find(|f| f.path == PathBuf::from(HELM_CHART_YAML_FILENAME))
            .unwrap();
        let chart: ChartYaml = serde_yaml::from_str(&chart_file.contents).unwrap();
        assert_eq!(chart.api_version, "v2");
        assert_eq!(chart.name, "lareira-hello-rio");
        assert_eq!(chart.version, "0.1.0");
        assert_eq!(chart.app_version, "0.1.0");
        assert_eq!(chart.dependencies.len(), 1);
        assert_eq!(chart.dependencies[0].name, "pleme-computeunit");
        assert!(chart.keywords.contains(&"caixa-servico".to_string()));
        assert!(chart.keywords.contains(&"hello-world".to_string()));
        assert_eq!(chart.maintainers[0].name, "pleme-io");
    }

    #[test]
    fn values_yaml_wraps_under_pleme_computeunit_key() {
        let dir = render_chart_for_servico(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let values = dir
            .files
            .iter()
            .find(|f| f.path == PathBuf::from("values.yaml"))
            .unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&values.contents).unwrap();
        let cu_block = parsed
            .get("pleme-computeunit")
            .expect("must wrap under pleme-computeunit");
        assert_eq!(
            cu_block.get(HELM_VALUES_KEY_ENABLED),
            Some(&serde_yaml::Value::Bool(false))
        );
        assert!(cu_block.get("module").is_some());
        assert!(cu_block.get("trigger").is_some());
        assert!(cu_block.get("capabilities").is_some());
    }

    #[test]
    fn values_yaml_wrap_key_follows_library_name_override() {
        // Pinning the canonical alignment between the Helm chart's
        // `dependencies[].name` axis (build_chart_yaml at line 277) and
        // the values block's wrap key (build_values_yaml at the
        // `wrapped.insert(...)` site): both consult the same
        // `opts.library_name`, so an override on either axis reaches the
        // other by construction. Helm's per-dep alias convention — when
        // no `alias:` is set on a dependency, values are scoped under
        // its `name:` — makes wrap-key drift a silent value-routing
        // no-op at `helm template` / `helm install` time, so the
        // structural pin is load-bearing.
        let opts = RenderOpts {
            library_name: "acme-computeunit".into(),
            ..RenderOpts::default()
        };
        let dir = render_chart_for_servico_with(&sample_caixa(), &sample_cu_yaml(), &opts).unwrap();
        let values = dir
            .files
            .iter()
            .find(|f| f.path == PathBuf::from("values.yaml"))
            .unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&values.contents).unwrap();
        assert!(
            parsed.get("acme-computeunit").is_some(),
            "values wrap key must follow opts.library_name override \
             (got top-level keys: {keys:?})",
            keys = parsed
                .as_mapping()
                .map(|m| m
                    .keys()
                    .filter_map(|k| k.as_str().map(str::to_string))
                    .collect::<Vec<_>>())
                .unwrap_or_default()
        );
        assert!(
            parsed.get("pleme-computeunit").is_none(),
            "values wrap key must not retain the default `pleme-computeunit` literal \
             when opts.library_name overrides it"
        );
        let cu_block = parsed.get("acme-computeunit").unwrap();
        assert_eq!(
            cu_block.get(HELM_VALUES_KEY_ENABLED),
            Some(&serde_yaml::Value::Bool(false))
        );
        assert!(cu_block.get("module").is_some());
        assert!(cu_block.get("trigger").is_some());
        assert!(cu_block.get("capabilities").is_some());
    }

    #[test]
    fn values_yaml_wrap_key_matches_chart_dependency_name() {
        // The structural invariant the lift defends: every rendered
        // chart's values.yaml wrap key equals its Chart.yaml
        // `dependencies[0].name`. Sweeping the canonical default + a
        // typed override on the same axis pins the alignment across the
        // accepted set of `RenderOpts::library_name` values rather than
        // at a single canonical literal.
        for library_name in ["pleme-computeunit", "acme-computeunit", "fork-pleme-cu"] {
            let opts = RenderOpts {
                library_name: library_name.into(),
                ..RenderOpts::default()
            };
            let dir =
                render_chart_for_servico_with(&sample_caixa(), &sample_cu_yaml(), &opts).unwrap();
            let chart_file = dir
                .files
                .iter()
                .find(|f| f.path == PathBuf::from(HELM_CHART_YAML_FILENAME))
                .unwrap();
            let chart: ChartYaml = serde_yaml::from_str(&chart_file.contents).unwrap();
            let dep_name = &chart.dependencies[0].name;
            let values = dir
                .files
                .iter()
                .find(|f| f.path == PathBuf::from("values.yaml"))
                .unwrap();
            let parsed: serde_yaml::Value = serde_yaml::from_str(&values.contents).unwrap();
            assert!(
                parsed.get(dep_name.as_str()).is_some(),
                "values.yaml wrap key must match Chart.yaml dependencies[0].name {dep_name:?} \
                 (library_name = {library_name:?}); Helm's per-dep alias convention scopes \
                 values under the dep's `name` when no `alias:` is set, so any drift between \
                 the two axes silently routes the values block nowhere"
            );
        }
    }

    #[test]
    fn values_yaml_header_comment_follows_library_name_override() {
        // The human-facing values.yaml header's `<library_alias>:` /
        // `pleme-io/helmworks/charts/<library_alias>` references both
        // resolve through `opts.library_name`, peer with the wrap key
        // itself, so an override leaves the header self-consistent
        // with the rendered structure rather than naming a drifted
        // default literal.
        let opts = RenderOpts {
            library_name: "acme-computeunit".into(),
            ..RenderOpts::default()
        };
        let dir = render_chart_for_servico_with(&sample_caixa(), &sample_cu_yaml(), &opts).unwrap();
        let values = dir
            .files
            .iter()
            .find(|f| f.path == PathBuf::from("values.yaml"))
            .unwrap();
        assert!(
            values.contents.contains("`acme-computeunit:`"),
            "header must name the overriding library alias verbatim \
             (got: {contents:?})",
            contents = values.contents
        );
        assert!(
            values
                .contents
                .contains("pleme-io/helmworks/charts/acme-computeunit"),
            "header's helmworks path must follow the overriding library alias \
             (got: {contents:?})",
            contents = values.contents
        );
        assert!(
            !values.contents.contains("`pleme-computeunit:`"),
            "header must not retain the default library alias literal \
             when overridden (got: {contents:?})",
            contents = values.contents
        );
    }

    #[test]
    fn refuses_non_servico() {
        let mut c = sample_caixa();
        c.kind = CaixaKind::Biblioteca;
        c.servicos = vec![];
        let err = render_chart_for_servico(&c, &sample_cu_yaml()).unwrap_err();
        assert!(matches!(err, Error::NotAServico(_)));
    }

    #[test]
    fn kind_mismatch_error_names_offending_caixa_nome() {
        // Pinning the lifted [`caixa_core::KindMismatch`] view's
        // load-bearing property: a kind-mismatched caixa surfaces a
        // diagnostic that *names the offending caixa* (`hello-rio`),
        // not just the rejected kind. Before the lift the renderer
        // raised `Error::NotAServico(CaixaKind::Biblioteca)` whose
        // Display said "caixa :kind must be Servico for caixa-helm
        // rendering, got Biblioteca" — the user had to grep their
        // source tree for which caixa.lisp triggered it. After the
        // lift the wrapped KindMismatch carries the `:nome`, the
        // renderer's `#[error("{0}")]` arm prints it through, and
        // the diagnostic is self-locating.
        let mut c = sample_caixa();
        c.kind = CaixaKind::Biblioteca;
        c.servicos = vec![];
        let err = render_chart_for_servico(&c, &sample_cu_yaml()).unwrap_err();
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
    fn kind_mismatch_carries_typed_view_via_from_conversion() {
        // The renderer's `Error::NotAServico` variant wraps the typed
        // [`caixa_core::KindMismatch`] view via `#[from]`, so the `?`
        // operator at the call site converts without manual glue.
        // Pinning the typed payload (not just the variant) so a
        // future refactor can't silently switch the variant to a
        // raw-`CaixaKind` payload (which would regress the lift's
        // shared-shape contract with caixa-flux + caixa-mesh).
        let mut c = sample_caixa();
        c.kind = CaixaKind::Aplicacao;
        c.servicos = vec![];
        let err = render_chart_for_servico(&c, &sample_cu_yaml()).unwrap_err();
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
        // [`render_chart_for_servico`] with the renderer's
        // `Error::UnsupportedServicoCount` variant wrapping the typed
        // [`caixa_core::ServicoCountMismatch`] view (carrying the
        // offending caixa's `:nome` + the actual count). Before the
        // lift the variant carried only `usize` — the user had to grep
        // their source tree for which `caixa.lisp` triggered it; after
        // the lift the wrapped typed view names the offending caixa
        // verbatim. Pins both the variant routing (via `#[from]`) and
        // the typed payload so a future refactor can't silently switch
        // back to the raw-`usize` payload (which would regress the
        // shared-shape contract with caixa-flux on the peer
        // programs.yaml-entry path).
        let mut c = sample_caixa();
        c.servicos = vec![
            "servicos/hello-rio.computeunit.yaml".into(),
            "servicos/extra.computeunit.yaml".into(),
        ];
        let err = render_chart_for_servico(&c, &sample_cu_yaml()).unwrap_err();
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
        // appears in the rendered diagnostic. Pinning the
        // self-locating property end-to-end (renderer entry-point →
        // typed view's Display → final diagnostic string) so a future
        // refactor that re-wraps the variant in a Display impl that
        // drops the `:nome` surfaces here as a test failure rather
        // than as silent fragmentation of the diagnostic. Peer to the
        // `kind_mismatch_error_names_offending_caixa_nome` test above
        // on the sibling V0 Servico-shape axis.
        let mut c = sample_caixa();
        c.servicos = vec![
            "servicos/hello-rio.computeunit.yaml".into(),
            "servicos/extra.computeunit.yaml".into(),
        ];
        let err = render_chart_for_servico(&c, &sample_cu_yaml()).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("hello-rio"),
            ":servicos-count-mismatch diagnostic must name the offending caixa nome \
             (got: {msg:?})"
        );
        assert!(
            msg.contains("2"),
            "diagnostic must name the actual count (got: {msg:?})"
        );
        assert!(
            msg.contains(":servicos"),
            "diagnostic must name the offending field axis (got: {msg:?})"
        );
    }

    #[test]
    fn limits_slot_propagates_into_values_block() {
        use caixa_core::LimitsSpec;
        use std::time::Duration;
        let mut c = sample_caixa();
        c.limits = Some(LimitsSpec {
            memory: Some(64 * 1024 * 1024),
            fuel: Some(1_000_000),
            wall_clock: Some(Duration::from_secs(30)),
            cpu: Some(500),
        });
        let dir = render_chart_for_servico(&c, &sample_cu_yaml()).unwrap();
        let values = dir
            .files
            .iter()
            .find(|f| f.path == PathBuf::from("values.yaml"))
            .unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&values.contents).unwrap();
        let cu_block = parsed.get("pleme-computeunit").unwrap();
        let limits = cu_block.get(M2_KEY_LIMITS).expect("limits must propagate");
        assert_eq!(limits.get("memory").and_then(|m| m.as_str()), Some("64MiB"));
        assert_eq!(limits.get("fuel").and_then(|m| m.as_u64()), Some(1_000_000));
        assert_eq!(
            limits.get("wallClock").and_then(|m| m.as_str()),
            Some("30s")
        );
        assert_eq!(limits.get("cpu").and_then(|m| m.as_str()), Some("500m"));
    }

    #[test]
    fn behavior_slot_propagates_into_values_block() {
        use caixa_core::BehaviorSpec;
        let mut c = sample_caixa();
        c.behavior = Some(BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_call: Some(PathBuf::from("lib/handlers.lisp")),
            ..Default::default()
        });
        let dir = render_chart_for_servico(&c, &sample_cu_yaml()).unwrap();
        let values = dir
            .files
            .iter()
            .find(|f| f.path == PathBuf::from("values.yaml"))
            .unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&values.contents).unwrap();
        let cu_block = parsed.get("pleme-computeunit").unwrap();
        let behavior = cu_block
            .get(M2_KEY_BEHAVIOR)
            .expect("behavior must propagate");
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
    fn upgrade_from_slot_propagates_into_values_block() {
        use caixa_core::{UpgradeFromEntry, UpgradeInstruction};
        let mut c = sample_caixa();
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.0.9".into(),
            instructions: vec![UpgradeInstruction::LoadModule {
                module: "hello-rio".into(),
            }],
        }];
        let dir = render_chart_for_servico(&c, &sample_cu_yaml()).unwrap();
        let values = dir
            .files
            .iter()
            .find(|f| f.path == PathBuf::from("values.yaml"))
            .unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&values.contents).unwrap();
        let cu_block = parsed.get("pleme-computeunit").unwrap();
        assert!(cu_block.get(M2_KEY_UPGRADE_FROM).is_some());
    }

    #[test]
    fn empty_m2_slots_do_not_appear() {
        // Existing caixa with no M2 slots → values.yaml carries no
        // limits/behavior/upgradeFrom keys (forward-compat invariant).
        let dir = render_chart_for_servico(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let values = dir
            .files
            .iter()
            .find(|f| f.path == PathBuf::from("values.yaml"))
            .unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&values.contents).unwrap();
        let cu_block = parsed.get("pleme-computeunit").unwrap();
        assert!(cu_block.get(M2_KEY_LIMITS).is_none());
        assert!(cu_block.get(M2_KEY_BEHAVIOR).is_none());
        assert!(cu_block.get(M2_KEY_UPGRADE_FROM).is_none());
    }

    #[test]
    fn write_to_creates_files() {
        let dir = render_chart_for_servico(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        dir.write_to(tmp.path()).unwrap();
        let chart_root = tmp.path().join("lareira-hello-rio");
        assert!(chart_root.join(HELM_CHART_YAML_FILENAME).exists());
        assert!(chart_root.join("values.yaml").exists());
        assert!(chart_root.join("README.md").exists());
    }

    #[test]
    fn default_library_name_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub const DEFAULT_LIBRARY_NAME` was lifted to a
        // re-export of [`caixa_core::DEFAULT_LIBRARY_NAME`] so the Helm
        // library-chart name lives in exactly one place across every
        // caixa renderer (caixa-helm's `RenderOpts::library_name`
        // default here + caixa-flux's `cluster_bundle` `helmrelease.yaml`
        // wrap key on the sibling deploy-path crate). Pin the equality
        // here so any local re-introduction of a sibling `pub const
        // DEFAULT_LIBRARY_NAME: &str = "…"` (the canonical drift footgun
        // the prior `DEFAULT_NAMESPACE` / `DEFAULT_SERVICO_PORT` lift
        // commits' bodies acknowledged as the recurring shape) is a
        // build-time test failure naming the offending drift, not a
        // silent apply-time wrap-key mismatch routing the per-cluster
        // `enabled: true` override nowhere on `helm template` /
        // `helm install`. Peer to
        // `caixa_flux::tests::default_library_name_re_export_points_at_caixa_core_canonical`
        // on the sibling renderer crate.
        caixa_core::assert_str_reexport_identity(
            "DEFAULT_LIBRARY_NAME",
            DEFAULT_LIBRARY_NAME,
            caixa_core::DEFAULT_LIBRARY_NAME,
        );
    }

    #[test]
    fn kube_key_spec_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_SPEC` was lifted from the production-
        // code inline `"spec"` literal at `build_values_yaml`'s
        // `computeunit_yaml.get("spec")` ComputeUnit-side spec read (+
        // its matching `Error::MissingField("spec")` diagnostic) to a
        // re-export of [`caixa_core::KUBE_KEY_SPEC`] so the canonical
        // K8s-CR top-level spec-axis string lives in exactly one place
        // across every caixa renderer. Pin the equality + static-data
        // identity here so any local re-introduction of a sibling
        // `pub const KUBE_KEY_SPEC: &str = "…"` (the canonical drift
        // footgun where a sibling local `pub const` could happen to
        // carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test
        // failure naming the offending drift. Peer to
        // [`default_library_name_re_export_points_at_caixa_core_canonical`]
        // on the sibling re-export axis +
        // `caixa_flux::tests::kube_key_spec_re_export_points_at_caixa_core_canonical`
        // / `caixa_mesh::tests::kube_key_spec_re_export_points_at_caixa_core_canonical`
        // on the sibling renderer crates.
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_SPEC",
            KUBE_KEY_SPEC,
            caixa_core::KUBE_KEY_SPEC,
        );
    }

    #[test]
    fn helm_chart_api_version_re_export_points_at_caixa_core_canonical() {
        // The renderer's `HELM_CHART_API_VERSION` was lifted from the
        // production-code inline `"v2".into()` literal at
        // [`build_chart_yaml`]'s `api_version` field assignment (formerly
        // `caixa-helm/src/lib.rs:298`) to a re-export of
        // [`caixa_core::HELM_CHART_API_VERSION`] so the Helm 3
        // chart-schema apiVersion the rendered Chart.yaml declares lives
        // in exactly one place across every caixa renderer. Pin the
        // equality + `&'static` static-data identity here so any local
        // re-introduction of a sibling `pub const HELM_CHART_API_VERSION:
        // &str = "…"` at this crate — the canonical drift footgun where
        // a sibling local `pub const` could happen to carry the same
        // string at the source while pointing at a different `&'static`
        // allocation — is a build-time test failure naming the offending
        // drift, not a silent chart-schema-parser reroute at
        // `helm template` time far from the drift site. Peer to
        // [`kube_key_spec_re_export_points_at_caixa_core_canonical`] /
        // [`default_library_name_re_export_points_at_caixa_core_canonical`]
        // on the sibling re-export axes.
        caixa_core::assert_str_reexport_identity(
            "HELM_CHART_API_VERSION",
            HELM_CHART_API_VERSION,
            caixa_core::HELM_CHART_API_VERSION,
        );
    }

    #[test]
    fn chart_yaml_uses_lifted_helm_chart_api_version() {
        // Fail-before-pass-after pin on the production-code
        // substitution: [`build_chart_yaml`]'s `api_version` field
        // consults the lifted [`HELM_CHART_API_VERSION`] re-export at
        // its assignment site, so the rendered Chart.yaml's top-level
        // `apiVersion` axis is byte-identical to the canonical constant
        // by construction. Before the lift the field carried an inline
        // `"v2".into()` literal at [`build_chart_yaml`]; a future
        // refactor that accidentally reverted the substitution — or
        // any parallel per-renderer variant that inlined a stale
        // Helm 2 `"v1"` literal — would silently reroute the rendered
        // Chart.yaml through the wrong chart-schema parser at
        // `helm dependency build` / `helm template` time, so this pin
        // trips at caixa-helm build time. Peer to
        // `values_yaml_wrap_key_matches_chart_dependency_name` on the
        // sibling structural-cross-axis-invariant surface.
        let dir = render_chart_for_servico(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let chart_file = dir
            .files
            .iter()
            .find(|f| f.path == PathBuf::from(HELM_CHART_YAML_FILENAME))
            .unwrap();
        let chart: ChartYaml = serde_yaml::from_str(&chart_file.contents).unwrap();
        assert_eq!(
            chart.api_version, HELM_CHART_API_VERSION,
            "rendered Chart.yaml `apiVersion` must equal the lifted \
             HELM_CHART_API_VERSION verbatim — a drifted value silently \
             reroutes the rendered chart through the wrong Helm chart-schema \
             parser at `helm template` time"
        );
    }

    #[test]
    fn helm_chart_type_application_re_export_points_at_caixa_core_canonical() {
        // The renderer's `HELM_CHART_TYPE_APPLICATION` was lifted from
        // the production-code inline `"application".into()` literal at
        // [`build_chart_yaml`]'s `chart_type` field assignment (formerly
        // `caixa-helm/src/lib.rs:354`) to a re-export of
        // [`caixa_core::HELM_CHART_TYPE_APPLICATION`] so the Helm 3
        // chart-schema per-chart-kind discriminator scalar-value the
        // rendered `lareira-<nome>` chart declares lives in exactly one
        // place across every caixa renderer. Pin the equality +
        // `&'static` static-data identity here so any local
        // re-introduction of a sibling `pub const
        // HELM_CHART_TYPE_APPLICATION: &str = "…"` at this crate — the
        // canonical drift footgun where a sibling local `pub const`
        // could happen to carry the same string at the source while
        // pointing at a different `&'static` allocation — is a
        // build-time test failure naming the offending drift, not a
        // silent per-release install-shape dispatch reroute at
        // `helm install` time far from the drift site. Peer to
        // [`helm_chart_api_version_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-Helm-chart-schema-axis re-export
        // surface — completes the per-Chart.yaml `(apiVersion, type)`
        // canonical-scalar-axis re-export pair every rendered
        // `lareira-<nome>` chart declares at its top-level Chart.yaml
        // body.
        caixa_core::assert_str_reexport_identity(
            "HELM_CHART_TYPE_APPLICATION",
            HELM_CHART_TYPE_APPLICATION,
            caixa_core::HELM_CHART_TYPE_APPLICATION,
        );
    }

    #[test]
    fn chart_yaml_uses_lifted_helm_chart_type_application() {
        // Fail-before-pass-after pin on the production-code substitution:
        // [`build_chart_yaml`]'s `chart_type` field consults the lifted
        // [`HELM_CHART_TYPE_APPLICATION`] re-export at its assignment
        // site, so the rendered Chart.yaml's top-level `type` axis is
        // byte-identical to the canonical constant by construction.
        // Before the lift the field carried an inline `"application".into()`
        // literal at [`build_chart_yaml`]; a future refactor that
        // accidentally reverted the substitution — or any parallel
        // per-renderer variant that inlined a `"library"` literal (the
        // sibling closed-set value from the Helm chart-schema's
        // per-chart-kind enum) — would silently reroute the rendered
        // Chart.yaml through the wrong per-release install-shape
        // dispatch at `helm install` time (Helm refuses to install a
        // `library` chart directly), so this pin trips at caixa-helm
        // build time. Peer to `chart_yaml_uses_lifted_helm_chart_api_version`
        // on the sibling per-Chart.yaml top-level `(apiVersion, type)`
        // canonical-scalar-axis pin pair — extends the per-Chart.yaml
        // top-level canonical-scalar-axis production-emit-pin
        // discipline from the `apiVersion` half onto the sibling `type`
        // half.
        let dir = render_chart_for_servico(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let chart_file = dir
            .files
            .iter()
            .find(|f| f.path == PathBuf::from(HELM_CHART_YAML_FILENAME))
            .unwrap();
        let chart: ChartYaml = serde_yaml::from_str(&chart_file.contents).unwrap();
        assert_eq!(
            chart.chart_type, HELM_CHART_TYPE_APPLICATION,
            "rendered Chart.yaml `type` must equal the lifted \
             HELM_CHART_TYPE_APPLICATION verbatim — a drifted value \
             silently reroutes the rendered chart through the wrong \
             per-release install-shape dispatch at `helm install` time \
             (Helm refuses to install a `library` chart directly, or \
             silently treats an unrecognized value as the default \
             `application` shape masking the schema violation)"
        );
    }

    #[test]
    fn render_opts_default_library_name_follows_lifted_constant() {
        // [`RenderOpts::default()`] sets `library_name` from
        // [`DEFAULT_LIBRARY_NAME`]; pin that the lift preserves the
        // default-knob value bit-for-bit. A future refactor that
        // detaches `RenderOpts::default()` from the lifted constant —
        // accidentally re-introducing an inline `"pleme-computeunit"`
        // literal in the impl — would silently break the shared-shape
        // contract with caixa-flux (which uses the same constant
        // directly for its `helmrelease.yaml` wrap key); this test
        // surfaces the regression at build time rather than at
        // apply time as a silent values-routing no-op.
        let opts = RenderOpts::default();
        assert_eq!(opts.library_name, caixa_core::DEFAULT_LIBRARY_NAME);
        assert_eq!(opts.library_name, "pleme-computeunit");
    }

    #[test]
    fn helm_chart_yaml_filename_re_export_points_at_caixa_core_canonical() {
        // The renderer's `HELM_CHART_YAML_FILENAME` was lifted from the
        // seven production + test-side inline `"Chart.yaml"` /
        // `PathBuf::from("Chart.yaml")` / `chart_root.join("Chart.yaml")`
        // literals across [`render_chart_for_servico`]'s `ChartDir`
        // metadata-file `path` emit site + every test-side round-trip
        // navigator that reaches into the rendered `ChartDir` by the
        // metadata filename to a re-export of
        // [`caixa_core::HELM_CHART_YAML_FILENAME`] so the Helm 3
        // per-chart-directory metadata-file filename lives in exactly one
        // place across every caixa renderer. Pin the equality +
        // `&'static` static-data identity here so any local
        // re-introduction of a sibling `pub const
        // HELM_CHART_YAML_FILENAME: &str = "…"` at this crate — the
        // canonical drift footgun where a sibling local `pub const` could
        // happen to carry the same string at the source while pointing
        // at a different `&'static` allocation — is a build-time test
        // failure naming the offending drift, not a silent
        // Helm-chart-schema-parser "Chart.yaml file is missing" reroute
        // at `helm dependency build` / `helm lint` / `helm template` /
        // `helm install` time far from the drift site. Peer to
        // [`helm_chart_api_version_re_export_points_at_caixa_core_canonical`]
        // / [`helm_chart_type_application_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-Helm-chart-schema-body-axis re-export
        // surfaces — completes the per-`lareira-<nome>`-chart-directory
        // `(filename, apiVersion, type)` canonical-scalar-axis re-export
        // triple every rendered chart declares at its top-level metadata
        // file.
        caixa_core::assert_str_reexport_identity(
            "HELM_CHART_YAML_FILENAME",
            HELM_CHART_YAML_FILENAME,
            caixa_core::HELM_CHART_YAML_FILENAME,
        );
    }

    #[test]
    fn helm_values_key_enabled_re_export_points_at_caixa_core_canonical() {
        // The renderer's `HELM_VALUES_KEY_ENABLED` was lifted from the
        // production-code inline `"enabled".to_string()` literal at
        // [`build_values_yaml`]'s
        // `block.insert("enabled".to_string(), Value::Bool(…))` values-
        // block-toggle insert (formerly `caixa-helm/src/lib.rs:389`) plus
        // its two test-side round-trip navigators
        // (`values_yaml_wraps_under_pleme_computeunit_key`,
        // `values_yaml_wrap_key_follows_library_name_override`) to a
        // re-export of [`caixa_core::HELM_VALUES_KEY_ENABLED`] so the
        // canonical `pleme-computeunit` library-chart values-block
        // enable-toggle key lives in exactly one place across every
        // caixa renderer. Pin the equality + `&'static` static-data
        // identity here so any local re-introduction of a sibling
        // `pub const HELM_VALUES_KEY_ENABLED: &str = "…"` at this crate
        // — the canonical drift footgun where a sibling local
        // `pub const` could happen to carry the same string at the
        // source while pointing at a different `&'static` allocation —
        // is a build-time test failure naming the offending drift, not
        // a silent per-values enable-toggle reroute at `helm template` /
        // `helm install` time far from the drift site (where the
        // workload silently comes up with the library chart's
        // admission-time defaults instead of the per-cluster override
        // the operator set). Peer to
        // [`helm_chart_api_version_re_export_points_at_caixa_core_canonical`]
        // / [`kube_key_spec_re_export_points_at_caixa_core_canonical`] /
        // [`default_library_name_re_export_points_at_caixa_core_canonical`]
        // on the sibling re-export axes +
        // `caixa_flux::tests::helm_values_key_enabled_re_export_points_at_caixa_core_canonical`
        // on the peer bundle-path renderer crate.
        caixa_core::assert_str_reexport_identity(
            "HELM_VALUES_KEY_ENABLED",
            HELM_VALUES_KEY_ENABLED,
            caixa_core::HELM_VALUES_KEY_ENABLED,
        );
    }

    #[test]
    fn values_yaml_enable_toggle_key_pins_lifted_helm_values_key_enabled() {
        // Fail-before-pass-after pin on the production-code substitution:
        // [`build_values_yaml`]'s `block.insert(…, Value::Bool(…))`
        // consults the lifted [`HELM_VALUES_KEY_ENABLED`] re-export at
        // its insert site, so the rendered `values.yaml`'s per-values
        // enable-toggle axis is byte-identical to the canonical constant
        // by construction. Before the lift the field carried an inline
        // `"enabled".to_string()` literal; a future refactor that
        // accidentally reverted the substitution — or any parallel per-
        // renderer variant that inlined a stale `"enable"` /
        // `"disabled"` literal — would silently emit a values block
        // whose per-values enable-toggle lands under one key while
        // [`caixa_flux::cluster_bundle`]'s `HelmRelease`
        // `spec.values.<library>.enabled` per-cluster override lands
        // under another, so this pin trips at caixa-helm build time.
        // Peer to `chart_yaml_uses_lifted_helm_chart_api_version` on the
        // sibling structural-cross-axis-invariant surface — both close
        // the drift between a rendered-value navigator's `.get(…)` /
        // struct-field read on the constant and the production-code
        // emit site that consumes the same constant.
        let dir = render_chart_for_servico(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let values = dir
            .files
            .iter()
            .find(|f| f.path == PathBuf::from("values.yaml"))
            .unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&values.contents).unwrap();
        let cu_block = parsed
            .get(DEFAULT_LIBRARY_NAME)
            .expect("must wrap under DEFAULT_LIBRARY_NAME");
        assert_eq!(
            cu_block.get(HELM_VALUES_KEY_ENABLED),
            Some(&serde_yaml::Value::Bool(false)),
            "rendered values.yaml `{DEFAULT_LIBRARY_NAME}.{HELM_VALUES_KEY_ENABLED}` must \
             equal the default-off toggle the lifted HELM_VALUES_KEY_ENABLED axis carries — \
             a drifted enable-toggle key silently splits the per-values enable-flip across \
             two sibling scalar names on the caixa-helm / caixa-flux consumer split"
        );
    }
}
