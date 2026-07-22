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
use std::path::Path;

use caixa_core::{Caixa, MappingExt, lareira_chart_name};
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

/// One file in the rendered chart — `(path, contents)` pair every
/// [`render_chart_for_servico`]-rendered `lareira-<nome>` chart-tree
/// leaf lands at (`Chart.yaml`, `values.yaml`, `README.md`).
///
/// Type-aliased to the canonical [`caixa_core::RenderedFile`] so the
/// substrate-side "one rendered leaf artifact" shape lives at one
/// struct definition across every per-target renderer — the peer
/// [`caixa_flux::BundleFile`] alias resolves to the same canonical, so
/// a future rebrand on either axis (a per-artifact hash / provenance
/// field addition, a per-artifact write-mode discriminator once
/// per-cluster-writer sandboxing lands) lands at one caixa-core `pub
/// struct RenderedFile` edit and reaches both crates by construction.
/// Prior to this lift both crates carried an inline `pub struct
/// <Xxx>File { pub path: PathBuf, pub contents: String }` with
/// identical `#[derive(Debug, Clone, PartialEq, Eq)]` shapes and no
/// per-type impls; a future per-target renderer (`caixa-otel`, the
/// future `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer, the
/// future per-Supervisor reconciler renderer) would have carried a
/// third and fourth clone of the same record. Type aliases preserve
/// every existing struct-literal construction site
/// (`ChartFile { path, contents }`), every field-access site (`f.path`,
/// `f.contents`), and every derive-fed navigator by construction —
/// Rust type aliases inherit the aliased type's `#[derive]`-generated
/// `Debug`/`Clone`/`PartialEq`/`Eq` impls with no per-alias glue.
pub type ChartFile = caixa_core::RenderedFile;

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

/// Canonical Helm 3 `Chart.yaml` `type` field per-chart-kind discriminator
/// scalar-value the sibling library-chart shape lands on — re-export of
/// the lifted [`caixa_core::HELM_CHART_TYPE_LIBRARY`] so the second and
/// only other arm of the Helm chart-schema's closed set `{"application",
/// "library"}` lives in exactly one place across every caixa renderer.
/// No production emitter here consumes it today — the caixa-helm
/// renderer emits per-Servico `application`-typed `lareira-<nome>`
/// charts, and the sibling [`DEFAULT_LIBRARY_NAME`] `pleme-computeunit`
/// library chart is out-of-tree at `pleme-io/helmworks` (not this
/// crate's authority) — but every future substrate-side per-chart-kind
/// classifier and every emitter for the future per-Aplicacao library
/// chart the [`HELM_CHART_TYPE_APPLICATION`] docstring names as a
/// trajectory item reads the same `&'static str` by construction.
/// Peer to the [`HELM_CHART_TYPE_APPLICATION`] re-export on the sibling
/// closed-set arm — completes the two-arm re-export pair of the Helm
/// chart-schema's per-chart-kind axis at the caixa-helm surface so
/// consumers reaching for either shape read from one canonical source
/// per arm. The paired identity pins
/// (`helm_chart_type_library_re_export_points_at_caixa_core_canonical`)
/// close the drift footgun at caixa-helm build time.
pub use caixa_core::HELM_CHART_TYPE_LIBRARY;

/// Canonical Helm 3 `Chart.yaml` top-level YAML axis-key naming the
/// per-chart-kind discriminator field — re-export of the lifted
/// [`caixa_core::HELM_CHART_KEY_TYPE`] so the top-level per-chart-kind
/// discriminator YAML key the sibling [`HELM_CHART_TYPE_APPLICATION`] /
/// [`HELM_CHART_TYPE_LIBRARY`] axis-value re-exports carry the closed-
/// set admitted scalars for lives in exactly one place across every
/// caixa renderer. The production emitter today is [`ChartYaml`]'s
/// `chart_type` field `#[serde(rename = "type")]` attribute — Rust's
/// attribute grammar admits only string literals so the const cannot
/// substitute for the literal syntactically, but the drift-detection
/// pin at
/// [`tests::chart_yaml_serializes_type_axis_under_lifted_helm_chart_key_type`]
/// round-trips a rendered `Chart.yaml` through
/// `serde_yaml::from_str::<serde_yaml::Value>` and asserts the top-
/// level `Mapping::get(HELM_CHART_KEY_TYPE)` resolves, closing the
/// drift the attribute-literal-only grammar leaves silent (a future
/// refactor that dropped the `#[serde(rename = "type")]` attribute
/// would silently serialize the field as Rust's default snake_case
/// `chart_type:`, which Helm's chart-schema parser silently ignores
/// as an unknown top-level key, defaulting the per-chart-kind axis
/// to `application` with no process-log drift-signal). Peer to the
/// [`HELM_CHART_TYPE_APPLICATION`] / [`HELM_CHART_TYPE_LIBRARY`]
/// re-exports on the sibling per-chart-kind axis-value canonical
/// surface — completes the per-Chart.yaml per-chart-kind
/// discriminator axis's `(key, value-set)` canonical re-export trio
/// at the caixa-helm surface.
pub use caixa_core::HELM_CHART_KEY_TYPE;

/// Canonical Helm 3 `Chart.yaml` top-level YAML axis-key naming the
/// per-chart underlying-application-version field — re-export of the
/// lifted [`caixa_core::HELM_CHART_KEY_APP_VERSION`] so the top-level
/// per-chart-app-version YAML key the [`ChartYaml`] `app_version`
/// field's `#[serde(rename = "appVersion")]` attribute encodes lives
/// in exactly one place across every caixa renderer. Rust's attribute
/// grammar admits only string literals so the const cannot substitute
/// for the literal syntactically, but the drift-detection pin at
/// [`tests::chart_yaml_serializes_app_version_axis_under_lifted_helm_chart_key_app_version`]
/// round-trips a rendered `Chart.yaml` and asserts the top-level
/// `Mapping::get(HELM_CHART_KEY_APP_VERSION)` resolves, closing the
/// drift the attribute-literal-only grammar leaves silent (a future
/// refactor that dropped the `#[serde(rename = "appVersion")]`
/// attribute would silently serialize the field as Rust's default
/// snake_case `app_version:`, which Helm's chart-schema parser
/// silently drops from the parsed chart-metadata shape, and every
/// downstream Artifact Hub / `helm search` per-chart index falls back
/// to "no application version" for the rendered chart far from the
/// drift site). Peer to [`HELM_CHART_KEY_TYPE`] on the sibling
/// per-Chart.yaml top-level YAML axis-key re-export surface —
/// completes the per-Chart.yaml top-level YAML axis-key re-export
/// pair at the caixa-helm surface for the two serde-rename-literal-
/// only axes on this crate's [`ChartYaml`] struct that Rust's
/// attribute-argument grammar leaves un-substitutable syntactically
/// (the third top-level axis-key `apiVersion` re-exports through
/// [`HELM_CHART_KEY_API_VERSION`] below, whose byte-shape coincides
/// with the sibling [`caixa_core::KUBE_KEY_API_VERSION`] by Helm's
/// design decision to inherit the K8s CR top-level shape verbatim —
/// the paired substrate-side
/// [`caixa_core`-side
/// `helm_chart_key_api_version_matches_kube_key_api_version`] pin
/// makes the byte-shape coincidence load-bearing rather than
/// accidental).
pub use caixa_core::HELM_CHART_KEY_APP_VERSION;

/// Canonical Helm 3 `Chart.yaml` top-level YAML axis-key naming the
/// per-chart chart-schema-apiVersion field — re-export of the lifted
/// [`caixa_core::HELM_CHART_KEY_API_VERSION`] so the top-level chart-
/// schema-apiVersion YAML key the [`ChartYaml`] `api_version` field's
/// `#[serde(rename = "apiVersion")]` attribute encodes lives in
/// exactly one place across every caixa renderer. Rust's attribute
/// grammar admits only string literals so the const cannot substitute
/// for the literal syntactically, but the drift-detection pin at
/// [`tests::chart_yaml_serializes_api_version_axis_under_lifted_helm_chart_key_api_version`]
/// round-trips a rendered `Chart.yaml` and asserts the top-level
/// `Mapping::get(HELM_CHART_KEY_API_VERSION)` resolves, closing the
/// drift the attribute-literal-only grammar leaves silent (a future
/// refactor that dropped the `#[serde(rename = "apiVersion")]`
/// attribute would silently serialize the field as Rust's default
/// snake_case `api_version:`, which Helm's chart-schema parser
/// rejects at `helm lint` / `helm dependency build` / `helm template`
/// time with an "apiVersion is required" error far from the drift
/// site). Peer to [`HELM_CHART_KEY_TYPE`] / [`HELM_CHART_KEY_APP_VERSION`]
/// on the sibling per-Chart.yaml top-level YAML axis-key re-export
/// surface — completes the per-Chart.yaml top-level YAML axis-key
/// re-export trio at the caixa-helm surface for the three serde-
/// rename-literal-only axes on this crate's [`ChartYaml`] struct.
pub use caixa_core::HELM_CHART_KEY_API_VERSION;

/// Canonical Helm 3 `Chart.yaml` top-level YAML axis-key naming the
/// per-chart dependency-list field — re-export of the lifted
/// [`caixa_core::HELM_CHART_KEY_DEPENDENCIES`] so the load-bearing serde
/// field-name at [`ChartYaml`]'s `dependencies` field (the parent
/// list-container the already-re-exported per-`dependencies[]`-entry
/// sub-mapping tetrad [`HELM_CHART_DEPENDENCY_KEY_NAME`] /
/// [`HELM_CHART_DEPENDENCY_KEY_VERSION`] /
/// [`HELM_CHART_DEPENDENCY_KEY_REPOSITORY`] /
/// [`HELM_CHART_DEPENDENCY_KEY_ALIAS`] mounts one level down under)
/// lives in exactly one place across every caixa renderer. The Rust
/// field name and the wire key coincide by default (no
/// `#[serde(rename)]` attribute today), so the const doesn't substitute
/// for the field-name syntactically at the struct definition, but the
/// drift-detection pin at
/// [`tests::chart_yaml_serializes_dependencies_axis_under_lifted_helm_chart_key_dependencies`]
/// round-trips a rendered `Chart.yaml` through
/// `serde_yaml::from_str::<serde_yaml::Value>` and asserts the top-
/// level `Mapping::get(HELM_CHART_KEY_DEPENDENCIES)` resolves — closing
/// the drift a future field rename (`dependencies` → `deps` /
/// `chartDependencies`) or a `#[serde(rename_all = "camelCase")]`
/// attribute addition on [`ChartYaml`] would otherwise leave silent
/// (Helm's chart-schema parser silently drops the entire dep list from
/// the parsed chart-metadata, `helm dependency build` finds no chart to
/// vendor, and every rendered `lareira-<nome>` chart's install fails at
/// apply time far from the drift site). Peer to
/// [`HELM_CHART_KEY_TYPE`] / [`HELM_CHART_KEY_APP_VERSION`] /
/// [`HELM_CHART_KEY_API_VERSION`] on the sibling per-Chart.yaml
/// top-level YAML axis-key re-export surface — extends the per-
/// Chart.yaml top-level YAML axis-key re-export trio at the caixa-helm
/// surface onto the fourth top-level axis-key, the parent list-
/// container whose per-entry sub-mapping tetrad is already re-exported
/// under [`HELM_CHART_DEPENDENCY_KEY_*`].
pub use caixa_core::HELM_CHART_KEY_DEPENDENCIES;

/// Canonical Helm 3 `Chart.yaml` per-`dependencies[]`-entry sub-mapping
/// YAML axis-key naming the per-dep chart-name field — re-export of the
/// lifted [`caixa_core::HELM_CHART_DEPENDENCY_KEY_NAME`] so the load-
/// bearing serde field-name at [`ChartDependency`]'s `name` field lives
/// in exactly one place across every caixa renderer. Peer to
/// [`HELM_CHART_DEPENDENCY_KEY_VERSION`] /
/// [`HELM_CHART_DEPENDENCY_KEY_REPOSITORY`] /
/// [`HELM_CHART_DEPENDENCY_KEY_ALIAS`] on the sibling per-dep sub-key
/// axes. The drift-detection round-trip pin at
/// [`tests::chart_dependency_serializes_tetrad_under_lifted_helm_chart_dependency_keys`]
/// serializes a fully-populated [`ChartDependency`] and asserts each of
/// the four per-dep sub-mapping wire keys resolves — closing the drift
/// a rename of the Rust field or a `#[serde(rename_all)]` attribute
/// addition would otherwise leave silent.
pub use caixa_core::HELM_CHART_DEPENDENCY_KEY_NAME;

/// Canonical Helm 3 `Chart.yaml` per-`dependencies[]`-entry sub-mapping
/// YAML axis-key naming the per-dep chart-version-constraint field —
/// re-export of the lifted [`caixa_core::HELM_CHART_DEPENDENCY_KEY_VERSION`]
/// so the load-bearing serde field-name at [`ChartDependency`]'s
/// `version` field lives in exactly one place across every caixa
/// renderer. Peer to [`HELM_CHART_DEPENDENCY_KEY_NAME`] on the sibling
/// per-dep sub-key axes. See [`HELM_CHART_DEPENDENCY_KEY_NAME`] for the
/// shared per-entry-sub-mapping lift rationale.
pub use caixa_core::HELM_CHART_DEPENDENCY_KEY_VERSION;

/// Canonical Helm 3 `Chart.yaml` per-`dependencies[]`-entry sub-mapping
/// YAML axis-key naming the per-dep chart-registry URL field — re-export
/// of the lifted [`caixa_core::HELM_CHART_DEPENDENCY_KEY_REPOSITORY`]
/// so the load-bearing serde field-name at [`ChartDependency`]'s
/// `repository` field lives in exactly one place across every caixa
/// renderer. Peer to [`HELM_CHART_DEPENDENCY_KEY_NAME`] on the sibling
/// per-dep sub-key axes.
pub use caixa_core::HELM_CHART_DEPENDENCY_KEY_REPOSITORY;

/// Canonical Helm 3 `Chart.yaml` per-`dependencies[]`-entry sub-mapping
/// YAML axis-key naming the per-dep chart-alias override field —
/// re-export of the lifted [`caixa_core::HELM_CHART_DEPENDENCY_KEY_ALIAS`]
/// so the load-bearing serde field-name at [`ChartDependency`]'s
/// `alias` field lives in exactly one place across every caixa
/// renderer. Peer to [`HELM_CHART_DEPENDENCY_KEY_NAME`] on the sibling
/// per-dep sub-key axes.
pub use caixa_core::HELM_CHART_DEPENDENCY_KEY_ALIAS;

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

/// Canonical Helm 3 per-chart-directory values-file filename every
/// rendered `lareira-<nome>` chart carries at its top-level directory —
/// re-export of the lifted [`caixa_core::HELM_VALUES_YAML_FILENAME`] so
/// the fixed lookup name Helm's chart-schema parser (`helm dependency
/// build`, `helm lint`, `helm template`, `helm install`) consults at
/// chart-open time to locate the per-chart values block that
/// [`HELM_VALUES_KEY_ENABLED`] toggles under its
/// [`DEFAULT_LIBRARY_NAME`] wrap key lives in exactly one place across
/// every caixa renderer. The single production-code call site
/// consuming it is [`render_chart_for_servico`]'s `ChartDir` assembly
/// where the values file's per-`ChartFile` `path` axis is set (the
/// sole emitter site the prior inline `PathBuf::from("values.yaml")`
/// literal sat at); every test-side round-trip navigator that reaches
/// into the rendered `ChartDir` by the values filename (the
/// per-chart-values-field sweep tests +
/// [`ChartDir::write_to`] post-write existence pin) now consults the
/// same `&'static str`, so a rebrand of the Helm 3 values-file axis
/// (any per-fork `defaults.yaml` / Helm 4 values-file rename the
/// upstream packaging spec might adopt) lands at one const and reaches
/// every consumer by construction. A drifted local `pub const
/// HELM_VALUES_YAML_FILENAME: &str = "…"` at this crate — the canonical
/// drift footgun where a sibling local `pub const` could happen to
/// carry the same string at the source while pointing at a different
/// `&'static` allocation — surfaces as one of two silent failure modes
/// at chart-consumption time: Helm's per-chart values-loader silently
/// falls back to the empty values block (`helm template` emits the
/// library chart under its admission-time defaults, the workload comes
/// up disabled or without any per-Servico M2 overlay applied) far from
/// the drift commit, or the sibling
/// [`caixa_flux::cluster_bundle`]'s future per-chart-directory
/// resolver — a per-cluster snapshot bundle that re-lists the
/// chart-dir contents by filename to route per-cluster values overlays
/// through the canonical values file — silently returns `None` at
/// cluster-side `feira app deploy` time. The equality + `&'static`
/// static-data identity pin
/// (`helm_values_yaml_filename_re_export_points_at_caixa_core_canonical`)
/// closes the drift footgun at caixa-helm build time. Peer to the
/// [`HELM_CHART_YAML_FILENAME`] re-export on the sibling
/// canonical-Helm-per-chart-directory-metadata-file-axis surface —
/// completes the per-`lareira-<nome>`-chart-directory
/// `(Chart.yaml, values.yaml)` canonical-per-chart-directory-filename-
/// axis re-export pair every rendered chart declares as its two
/// schema-load-bearing `ChartDir::files` entries.
pub use caixa_core::HELM_VALUES_YAML_FILENAME;

/// Canonical `lareira-<nome>` chart-directory human-facing readme
/// filename every rendered chart carries at its top-level directory —
/// re-export of the lifted [`caixa_core::HELM_CHART_README_FILENAME`] so
/// the third leg of the canonical `{Chart.yaml, values.yaml, README.md}`
/// per-`lareira-<nome>` chart-directory `ChartFile` triple lives in
/// exactly one place across every caixa renderer. The single
/// production-code call site consuming it is
/// [`render_chart_for_servico`]'s `ChartDir` assembly where the readme
/// file's per-`ChartFile` `path` axis is set (the sole emitter site the
/// prior inline `"README.md"` string literal sat at); every test-side
/// round-trip navigator that reaches into the rendered `ChartDir` by
/// the readme filename (the [`render_chart_for_servico`] files-vec-
/// membership pin + the [`ChartDir::write_to`] post-write existence
/// pin — two sites) now consults the same `&'static str`. A drifted
/// local `pub const HELM_CHART_README_FILENAME: &str = "…"` at this
/// crate — the canonical drift footgun where a sibling local
/// `pub const` could happen to carry the same string at the source
/// while pointing at a different `&'static` allocation — surfaces as
/// GitHub / Artifact Hub / any downstream per-chart README-surfacing
/// UI silently falling back to "no README available" for the rendered
/// `lareira-<nome>` chart far from the drift commit's source, with no
/// field naming the readme-filename-drift root cause. The equality +
/// `&'static` static-data identity pin
/// (`helm_chart_readme_filename_re_export_points_at_caixa_core_canonical`)
/// closes the drift footgun at caixa-helm build time. Peer to the
/// [`HELM_CHART_YAML_FILENAME`] / [`HELM_VALUES_YAML_FILENAME`]
/// re-exports on the sibling canonical-Helm-per-chart-directory-
/// filename axes — completes the per-`lareira-<nome>`-chart-directory
/// `(Chart.yaml, values.yaml, README.md)` canonical-per-chart-directory-
/// filename-axis re-export triple every rendered chart declares as its
/// three `ChartDir::files` entries.
pub use caixa_core::HELM_CHART_README_FILENAME;

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

/// Canonical substrate-side default for the
/// `values.<library>.enabled` scalar-value toggle every
/// [`render_chart_for_servico`]-emitted standalone `lareira-<nome>`
/// chart's `values.yaml` document seeds inside its per-caixa
/// [`DEFAULT_LIBRARY_NAME`] wrap block to leave the paired
/// [`DEFAULT_LIBRARY_NAME`] child chart opted-out at the per-cluster
/// `helm template` / `helm install` apply step. Re-export of the
/// canonical [`caixa_core::STANDALONE_LAREIRA_ENABLED_DEFAULT`] so the
/// substrate-side default the standalone per-chart path seeds under
/// the sibling [`HELM_VALUES_KEY_ENABLED`] leaf-scalar-key lives in
/// exactly one place across every caixa renderer. Consumed by
/// [`RenderOpts::default`]'s `enabled_default` field seed (formerly
/// an inline `false` scalar-value literal at
/// `caixa-helm/src/lib.rs:700`); the peer test-fixture navigators
/// pinning the default-off round-trip also consult the re-export so a
/// rebrand of the per-values-block child-chart-enablement-toggle scalar
/// on the standalone per-chart path lands at one const and reaches
/// every consumer by construction. Semantically distinct from — and
/// inverse of — the peer
/// [`caixa_flux::CLUSTER_BUNDLE_LAREIRA_ENABLED_DEFAULT`] on the
/// composition per-cluster-`HelmRelease` values-overlay path (which
/// force-ons the child chart under the substrate-side composition
/// path); the two peer scalar-value defaults name mirror-symmetric
/// per-path child-chart-enablement-toggle-scalar-value defaults at the
/// exact same `values.<library>.enabled` sub-block position on the
/// standalone per-chart-`values.yaml` path (this re-export) and the
/// composition per-cluster-`HelmRelease` values-overlay path (the peer
/// re-export). Same shape as the [`DEFAULT_LIBRARY_NAME`] /
/// [`KUBE_KEY_SPEC`] / [`HELM_VALUES_KEY_ENABLED`] re-exports on the
/// sibling canonical-Helm-load-bearing-string / canonical-K8s-CR-body-
/// key / canonical-Helm-per-values-block-enable-toggle-key axes. See
/// [`caixa_core::STANDALONE_LAREIRA_ENABLED_DEFAULT`] for the full lift
/// rationale.
pub use caixa_core::STANDALONE_LAREIRA_ENABLED_DEFAULT;

/// Local re-export of the canonical
/// [`caixa_core::COMPUTEUNIT_SPEC_KEY_MODULE`] — the
/// `wasm.pleme.io/v1alpha1/ComputeUnit` CRD per-CR wasm-module-reference
/// `spec.module` sub-block key every rendered `values.yaml`'s
/// [`DEFAULT_LIBRARY_NAME`]-wrapped block carries so the
/// `pleme-computeunit` library chart's per-Servico module-source axis
/// binds to the exact source the caixa.lisp's `:servicos` fixture
/// pins. Two per-values drift-detection navigators in this crate's
/// test module (the canonical-wrap-key round-trip + the
/// `library-name`-override wrap-key round-trip) now consult the same
/// `&'static str` as the peer caixa-flux writer's per-Servico
/// `programs[]`-entry module-source navigators. Same re-export shape
/// as the peer [`HELM_VALUES_KEY_ENABLED`] / [`KUBE_KEY_SPEC`]
/// surfaces on the sibling canonical-Helm-load-bearing-string /
/// canonical-K8s-CR-body-key axes — extends the discipline the
/// M2-typed-slot / K8s-CR key re-export families establish onto the
/// substrate-side ComputeUnit-CRD per-`spec.*` sub-block axis. See
/// [`caixa_core::COMPUTEUNIT_SPEC_KEY_MODULE`] for the full lift
/// rationale.
pub use caixa_core::COMPUTEUNIT_SPEC_KEY_MODULE;

/// Local re-export of the canonical
/// [`caixa_core::COMPUTEUNIT_SPEC_KEY_TRIGGER`] — the
/// `wasm.pleme.io/v1alpha1/ComputeUnit` CRD per-CR invocation-shape
/// `spec.trigger` sub-block key every rendered `values.yaml`'s
/// [`DEFAULT_LIBRARY_NAME`]-wrapped block carries so the
/// `pleme-computeunit` library chart's per-Servico
/// `trigger.service.{port, paths, breathability}` routing binds to
/// the exact axis the caixa.lisp's `:servicos` fixture pins. Peer of
/// [`COMPUTEUNIT_SPEC_KEY_MODULE`] on the same ComputeUnit CRD
/// per-`spec.*` sub-block axis — see
/// [`caixa_core::COMPUTEUNIT_SPEC_KEY_TRIGGER`] for the full lift
/// rationale.
pub use caixa_core::COMPUTEUNIT_SPEC_KEY_TRIGGER;

/// Local re-export of the canonical
/// [`caixa_core::COMPUTEUNIT_SPEC_KEY_CAPABILITIES`] — the
/// `wasm.pleme.io/v1alpha1/ComputeUnit` CRD per-CR WASI-capability-list
/// `spec.capabilities` sub-block key every rendered `values.yaml`'s
/// [`DEFAULT_LIBRARY_NAME`]-wrapped block carries so the
/// `pleme-computeunit` library chart's per-Servico WASI-preview-2
/// capability-token binding fires exactly against the axis the
/// caixa.lisp's `:servicos` fixture pins. Peer of
/// [`COMPUTEUNIT_SPEC_KEY_MODULE`] and [`COMPUTEUNIT_SPEC_KEY_TRIGGER`]
/// on the same ComputeUnit CRD per-`spec.*` sub-block axis — completes
/// the substrate-side ComputeUnit-CRD per-`spec.*` sub-block re-export
/// triple in this crate. See
/// [`caixa_core::COMPUTEUNIT_SPEC_KEY_CAPABILITIES`] for the full lift
/// rationale.
pub use caixa_core::COMPUTEUNIT_SPEC_KEY_CAPABILITIES;

/// Local re-export of the canonical
/// [`caixa_core::servico_spec_and_m2_overlay_entries`] — the composed
/// per-Servico value-block splice helper this crate's
/// [`build_values_yaml`] and the peer
/// [`caixa_flux::programs_yaml_entry`] both now route their two-step
/// `spec.*` field-splice + M2 typed-slot overlay through. The single
/// production-code call site consuming it is [`build_values_yaml`]'s
/// inner splice loop (formerly two hand-written for-loops chained
/// around `string_keyed_entries` + `servico_m2_overlay`); re-exported
/// so the shared composition contract lives in exactly one place
/// across both per-Servico renderers — a future author reading
/// `caixa_helm::build_values_yaml` finds the composition helper
/// immediately without an extra `use caixa_core::…` line, and a
/// rebrand of the composition axis (e.g. a swap of the `or_insert`
/// precedence rule once per-Aplicacao operator overrides land)
/// reaches both renderers through one canonical `&'static` function
/// pointer. Same shape as the peer [`COMPUTEUNIT_SPEC_KEY_MODULE`] /
/// [`COMPUTEUNIT_SPEC_KEY_TRIGGER`] / [`COMPUTEUNIT_SPEC_KEY_CAPABILITIES`]
/// re-exports on the sibling canonical-ComputeUnit-CRD `spec.*` axis
/// — extends the shared-composition discipline the per-`spec.*`
/// sub-block re-export triple establishes onto the composed
/// spec.*+M2 splice axis every per-Servico renderer navigates.
pub use caixa_core::servico_spec_and_m2_overlay_entries;

/// Knobs that don't come from the Caixa manifest.
#[derive(Debug, Clone)]
pub struct RenderOpts {
    /// Where the library chart lives. Default = `file://../pleme-computeunit`.
    pub library_repo: String,
    pub library_version: String,
    pub library_name: String,
    /// Whether the rendered values block is `enabled: false` by default
    /// (matching `lareira-hello-world` so cluster operators flip it on
    /// per-cluster). Default: [`STANDALONE_LAREIRA_ENABLED_DEFAULT`]
    /// (`false` — the substrate's chosen standalone per-chart
    /// opt-out seed, inverse of the composition per-cluster-`HelmRelease`
    /// values-overlay path's [`caixa_flux::CLUSTER_BUNDLE_LAREIRA_ENABLED_DEFAULT`]
    /// force-on).
    pub enabled_default: bool,
}

impl Default for RenderOpts {
    fn default() -> Self {
        Self {
            library_repo: DEFAULT_LIBRARY_REPO.into(),
            library_version: DEFAULT_LIBRARY_VERSION.into(),
            library_name: DEFAULT_LIBRARY_NAME.into(),
            enabled_default: STANDALONE_LAREIRA_ENABLED_DEFAULT,
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
    caixa_core::require_v0_servico_shape::<Error>(caixa)?;

    let chart_name = lareira_chart_name(&caixa.nome);
    let chart_yaml = build_chart_yaml(caixa, &chart_name, opts);
    let values_yaml = build_values_yaml(caixa, computeunit_yaml, opts)?;
    let readme = build_readme(caixa, &chart_name);

    // Each per-artifact leaf routes through the canonical
    // [`caixa_core::RenderedFile::new`] `impl Into<PathBuf>` /
    // `impl Into<String>` constructor (re-exported by the peer
    // [`ChartFile`] alias since Rust inherent methods travel through
    // type aliases to the aliased type at name resolution). The prior
    // three inline `ChartFile { path: PathBuf::from(FILENAME_CONST),
    // contents: <body> }` blocks each re-derived the same
    // `PathBuf::from(&str)` wrap + the same two-field assembly — a
    // byte-identical duplicate of the peer [`caixa_flux::cluster_bundle`]
    // Flux v2 CR trio's three per-CR emit sites. Sweeping both trios
    // onto [`RenderedFile::new`] collapses the six substrate-side
    // per-artifact-construction sites onto one canonical constructor,
    // so a future rebrand on the record shape (a per-artifact hash /
    // provenance field addition, a per-artifact write-mode discriminator
    // once per-cluster-writer sandboxing lands, the
    // [`caixa_core::is_sandboxed_relative_path`] discipline the
    // [`RenderedFile`] docstring acknowledges is not yet run at emit
    // time) reaches every per-target renderer through one caixa-core
    // edit instead of a coordinated six-site rewrite.
    Ok(ChartDir {
        name: chart_name,
        files: vec![
            ChartFile::new(
                HELM_CHART_YAML_FILENAME,
                serde_yaml::to_string(&chart_yaml)?,
            ),
            ChartFile::new(HELM_VALUES_YAML_FILENAME, values_yaml),
            ChartFile::new(HELM_CHART_README_FILENAME, readme),
        ],
    })
}

fn build_chart_yaml(caixa: &Caixa, chart_name: &str, opts: &RenderOpts) -> ChartYaml {
    let description = caixa
        .descricao()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Generated chart for caixa Servico {}", caixa.nome));
    let keywords: Vec<String> = caixa
        .etiquetas
        .iter()
        .cloned()
        .chain(
            caixa_core::LAREIRA_CHART_KEYWORDS
                .iter()
                .copied()
                .map(String::from),
        )
        .collect::<Vec<_>>()
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let maintainers = caixa
        .autores()
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
        home: caixa.repositorio().map(str::to_owned),
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
        .ok_or(Error::MissingField(KUBE_KEY_SPEC))?;

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
    // Two-step per-Servico value-block splice — the `spec.*` field
    // splice (module / trigger / capabilities / config / resources /
    // serviceAccount) and the M2 typed-slot overlay (limits / behavior
    // / upgradeFrom, `or_insert` semantics so `spec.*` wins on
    // collision) now route through the canonical
    // [`caixa_core::servico_spec_and_m2_overlay_entries`] composition
    // helper — the two prior inline for-loops chained around
    // `string_keyed_entries` + `servico_m2_overlay` this call site
    // (and the peer [`caixa_flux::programs_yaml_entry`] site) each
    // re-derived collapse onto one canonical composition, so a future
    // change to the per-Servico splice / overlay shape (the M4 typed
    // per-edge policy overlay slot addition MESH-COMPOSITION §III.2 #3
    // acknowledges, a change to the precedence rule once per-Aplicacao
    // operator overrides land, a canonicalization pass on the merged
    // key set) reaches both renderers by construction instead of a
    // coordinated two-file rewrite. See the helper's docstring for the
    // full lift rationale. The target `BTreeMap` re-sorts by key on
    // insert, so the final rendered values block stays byte-identical
    // to the prior inline block's alphabetical shape.
    for (k, v) in caixa_core::servico_spec_and_m2_overlay_entries(caixa, spec)? {
        block.entry(k).or_insert(v);
    }

    let mut wrapped = serde_yaml::Mapping::new();
    wrapped.insert_str_key(library_alias, serde_yaml::to_value(block)?);
    let body = serde_yaml::to_string(&serde_yaml::Value::Mapping(wrapped))?;
    Ok(format!("{header}{body}"))
}

fn build_readme(caixa: &Caixa, chart_name: &str) -> String {
    let descricao = caixa
        .descricao()
        .map(str::to_owned)
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
        repo = caixa.repositorio().unwrap_or(caixa.nome.as_str()),
        versao = caixa.versao,
        license = caixa.licenca().unwrap_or("MIT"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use caixa_core::{
        Caixa, CaixaKind, M2_BEHAVIOR_KEY_ON_CALL, M2_BEHAVIOR_KEY_ON_INIT, M2_KEY_BEHAVIOR,
        M2_KEY_LIMITS, M2_KEY_UPGRADE_FROM, M2_LIMITS_KEY_CPU, M2_LIMITS_KEY_FUEL,
        M2_LIMITS_KEY_MEMORY, M2_LIMITS_KEY_WALL_CLOCK,
    };
    use std::path::PathBuf;

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
        assert!(names.contains(&HELM_VALUES_YAML_FILENAME.to_string()));
        assert!(names.contains(&HELM_CHART_README_FILENAME.to_string()));
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
        assert_eq!(chart.dependencies[0].name, DEFAULT_LIBRARY_NAME);
        assert!(chart.keywords.contains(&"caixa-servico".to_string()));
        assert!(chart.keywords.contains(&"hello-world".to_string()));
        assert_eq!(chart.maintainers[0].name, "pleme-io");
    }

    #[test]
    fn chart_yaml_keywords_union_pins_every_lareira_chart_keywords_entry() {
        // Structural pin: `build_chart_yaml`'s substrate-fixed
        // chart-keyword union routes through the canonical
        // `caixa_core::LAREIRA_CHART_KEYWORDS` array — every rendered
        // `lareira-<nome>` chart's emitted `Chart.yaml` `keywords:`
        // sequence carries every substrate-fixed entry the array
        // declares. A future substrate-fixed keyword addition
        // (an `"opentelemetry"` entry once the caixa-otel collector-
        // pipeline chart lands, a `"lunatic"` entry once the wasm-
        // process-runtime marker lands, a `"gen_server"` entry once
        // the OTP-shape callback marker lands per the
        // [`caixa_core::behavior`] surface) that lands in the array
        // reaches this crate's production emit site by construction
        // through the shared `&[&str]` reference — a drift where the
        // production emit at `build_chart_yaml` re-inlines the pre-
        // lift `["lareira", "wasm", "tatara-lisp", "caixa-servico"]`
        // literal set (or drops a per-entry axis on a rebrand
        // sweep) fails this pin at caixa-helm build time rather than
        // surfacing as a `helm search hub caixa-servico` miss on the
        // Artifact Hub keyword-search index at chart-publish time
        // downstream. Peer to
        // [`chart_yaml_metadata_propagates`] on the
        // per-`Chart.yaml`-body-field propagation surface.
        let dir = render_chart_for_servico(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let chart_file = dir
            .files
            .iter()
            .find(|f| f.path == PathBuf::from(HELM_CHART_YAML_FILENAME))
            .unwrap();
        let chart: ChartYaml = serde_yaml::from_str(&chart_file.contents).unwrap();
        for keyword in caixa_core::LAREIRA_CHART_KEYWORDS {
            assert!(
                chart.keywords.contains(&(*keyword).to_string()),
                "rendered Chart.yaml keywords {:?} must contain the \
                 substrate-fixed LAREIRA_CHART_KEYWORDS entry {keyword:?}",
                chart.keywords,
            );
        }
    }

    #[test]
    fn values_yaml_wraps_under_pleme_computeunit_key() {
        let dir = render_chart_for_servico(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let values = dir
            .files
            .iter()
            .find(|f| f.path == PathBuf::from(HELM_VALUES_YAML_FILENAME))
            .unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&values.contents).unwrap();
        let cu_block = parsed
            .get(DEFAULT_LIBRARY_NAME)
            .expect("must wrap under DEFAULT_LIBRARY_NAME");
        assert_eq!(
            cu_block.get(HELM_VALUES_KEY_ENABLED),
            Some(&serde_yaml::Value::Bool(false))
        );
        assert!(cu_block.get(COMPUTEUNIT_SPEC_KEY_MODULE).is_some());
        assert!(cu_block.get(COMPUTEUNIT_SPEC_KEY_TRIGGER).is_some());
        assert!(cu_block.get(COMPUTEUNIT_SPEC_KEY_CAPABILITIES).is_some());
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
            .find(|f| f.path == PathBuf::from(HELM_VALUES_YAML_FILENAME))
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
            parsed.get(DEFAULT_LIBRARY_NAME).is_none(),
            "values wrap key must not retain the default `{DEFAULT_LIBRARY_NAME}` literal \
             when opts.library_name overrides it"
        );
        let cu_block = parsed.get("acme-computeunit").unwrap();
        assert_eq!(
            cu_block.get(HELM_VALUES_KEY_ENABLED),
            Some(&serde_yaml::Value::Bool(false))
        );
        assert!(cu_block.get(COMPUTEUNIT_SPEC_KEY_MODULE).is_some());
        assert!(cu_block.get(COMPUTEUNIT_SPEC_KEY_TRIGGER).is_some());
        assert!(cu_block.get(COMPUTEUNIT_SPEC_KEY_CAPABILITIES).is_some());
    }

    #[test]
    fn values_yaml_wrap_key_matches_chart_dependency_name() {
        // The structural invariant the lift defends: every rendered
        // chart's values.yaml wrap key equals its Chart.yaml
        // `dependencies[0].name`. Sweeping the canonical default + a
        // typed override on the same axis pins the alignment across the
        // accepted set of `RenderOpts::library_name` values rather than
        // at a single canonical literal.
        for library_name in [DEFAULT_LIBRARY_NAME, "acme-computeunit", "fork-pleme-cu"] {
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
                .find(|f| f.path == PathBuf::from(HELM_VALUES_YAML_FILENAME))
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
            .find(|f| f.path == PathBuf::from(HELM_VALUES_YAML_FILENAME))
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
            .find(|f| f.path == PathBuf::from(HELM_VALUES_YAML_FILENAME))
            .unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&values.contents).unwrap();
        let cu_block = parsed.get(DEFAULT_LIBRARY_NAME).unwrap();
        let limits = cu_block.get(M2_KEY_LIMITS).expect("limits must propagate");
        assert_eq!(
            limits.get(M2_LIMITS_KEY_MEMORY).and_then(|m| m.as_str()),
            Some("64MiB")
        );
        assert_eq!(
            limits.get(M2_LIMITS_KEY_FUEL).and_then(|m| m.as_u64()),
            Some(1_000_000)
        );
        assert_eq!(
            limits
                .get(M2_LIMITS_KEY_WALL_CLOCK)
                .and_then(|m| m.as_str()),
            Some("30s")
        );
        assert_eq!(
            limits.get(M2_LIMITS_KEY_CPU).and_then(|m| m.as_str()),
            Some("500m")
        );
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
            .find(|f| f.path == PathBuf::from(HELM_VALUES_YAML_FILENAME))
            .unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&values.contents).unwrap();
        let cu_block = parsed.get(DEFAULT_LIBRARY_NAME).unwrap();
        let behavior = cu_block
            .get(M2_KEY_BEHAVIOR)
            .expect("behavior must propagate");
        assert_eq!(
            behavior
                .get(M2_BEHAVIOR_KEY_ON_INIT)
                .and_then(|v| v.as_str()),
            Some("lib/init.lisp")
        );
        assert_eq!(
            behavior
                .get(M2_BEHAVIOR_KEY_ON_CALL)
                .and_then(|v| v.as_str()),
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
            .find(|f| f.path == PathBuf::from(HELM_VALUES_YAML_FILENAME))
            .unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&values.contents).unwrap();
        let cu_block = parsed.get(DEFAULT_LIBRARY_NAME).unwrap();
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
            .find(|f| f.path == PathBuf::from(HELM_VALUES_YAML_FILENAME))
            .unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&values.contents).unwrap();
        let cu_block = parsed.get(DEFAULT_LIBRARY_NAME).unwrap();
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
        assert!(chart_root.join(HELM_VALUES_YAML_FILENAME).exists());
        assert!(chart_root.join(HELM_CHART_README_FILENAME).exists());
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
    fn helm_chart_type_library_re_export_points_at_caixa_core_canonical() {
        // Re-export identity pin on the peer closed-set arm the
        // renderer's `HELM_CHART_TYPE_LIBRARY` alias resolves to. Peer
        // of `helm_chart_type_application_re_export_points_at_caixa_core_canonical`
        // on the sibling closed-set arm — the two pins together enshrine
        // the two-arm `{"application", "library"}` closed set at the
        // caixa-helm re-export surface as byte-identical `&'static`
        // static-data views onto the canonical caixa-core lifts, so any
        // local re-introduction of a sibling `pub const
        // HELM_CHART_TYPE_LIBRARY: &str = "…"` at this crate (the same
        // drift footgun the peer pin closes on the sibling arm) is a
        // build-time test failure naming the offending drift. The pin
        // also structurally forbids the two arms from converging on the
        // same `&'static` allocation — a future rebrand that
        // accidentally aliased `HELM_CHART_TYPE_LIBRARY` at the
        // [`caixa_core::HELM_CHART_TYPE_APPLICATION`] canonical would
        // pass this identity check but trip the caixa-core-side
        // `helm_chart_type_application_and_library_are_distinct` pin
        // paired to the two arms' distinctness contract.
        caixa_core::assert_str_reexport_identity(
            "HELM_CHART_TYPE_LIBRARY",
            HELM_CHART_TYPE_LIBRARY,
            caixa_core::HELM_CHART_TYPE_LIBRARY,
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
    fn helm_chart_key_type_re_export_points_at_caixa_core_canonical() {
        // The renderer's `HELM_CHART_KEY_TYPE` was lifted from the
        // sole production-side inline `"type"` literal at [`ChartYaml`]'s
        // `chart_type` field `#[serde(rename = "type")]` attribute
        // (formerly `caixa-helm/src/lib.rs:149`) to a re-export of
        // [`caixa_core::HELM_CHART_KEY_TYPE`] so the Helm 3 top-level
        // per-chart-kind discriminator YAML axis-key lives in exactly
        // one place across every caixa renderer. Pin the equality +
        // `&'static` static-data identity here so any local
        // re-introduction of a sibling `pub const HELM_CHART_KEY_TYPE:
        // &str = "…"` at this crate — the canonical drift footgun
        // where a sibling local `pub const` could happen to carry the
        // same string at the source while pointing at a different
        // `&'static` allocation — is a build-time test failure naming
        // the offending drift, not a silent Helm-chart-schema-parser
        // per-chart-kind-defaulting reroute at `helm dependency build`
        // / `helm lint` / `helm template` / `helm install` time far
        // from the drift site. Peer to
        // [`helm_chart_type_application_re_export_points_at_caixa_core_canonical`]
        // / [`helm_chart_type_library_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-chart-kind axis-value re-export surface —
        // completes the per-Chart.yaml per-chart-kind discriminator
        // axis's `(key, value-set)` canonical re-export trio at the
        // caixa-helm surface.
        caixa_core::assert_str_reexport_identity(
            "HELM_CHART_KEY_TYPE",
            HELM_CHART_KEY_TYPE,
            caixa_core::HELM_CHART_KEY_TYPE,
        );
    }

    #[test]
    fn helm_chart_key_app_version_re_export_points_at_caixa_core_canonical() {
        // The renderer's `HELM_CHART_KEY_APP_VERSION` was lifted from
        // the sole production-side inline `"appVersion"` literal at
        // [`ChartYaml`]'s `app_version` field
        // `#[serde(rename = "appVersion")]` attribute (formerly
        // `caixa-helm/src/lib.rs:152`) to a re-export of
        // [`caixa_core::HELM_CHART_KEY_APP_VERSION`] so the Helm 3
        // top-level per-chart-app-version YAML axis-key lives in
        // exactly one place across every caixa renderer. Pin the
        // equality + `&'static` static-data identity here so any
        // local re-introduction of a sibling `pub const
        // HELM_CHART_KEY_APP_VERSION: &str = "…"` at this crate — the
        // canonical drift footgun where a sibling local `pub const`
        // could happen to carry the same string at the source while
        // pointing at a different `&'static` allocation — is a
        // build-time test failure naming the offending drift, not a
        // silent Helm-chart-schema-parser field-drop at every
        // downstream Artifact Hub / `helm search` chart-consumer far
        // from the drift site. Peer to
        // [`helm_chart_key_type_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-Chart.yaml top-level YAML axis-key
        // re-export surface — completes the per-Chart.yaml top-level
        // YAML axis-key re-export pair at the caixa-helm surface for
        // the two serde-rename-literal-only axes.
        caixa_core::assert_str_reexport_identity(
            "HELM_CHART_KEY_APP_VERSION",
            HELM_CHART_KEY_APP_VERSION,
            caixa_core::HELM_CHART_KEY_APP_VERSION,
        );
    }

    #[test]
    fn chart_yaml_serializes_type_axis_under_lifted_helm_chart_key_type() {
        // Fail-before-pass-after drift-detection pin on the
        // `#[serde(rename = "type")]` attribute at [`ChartYaml`]'s
        // `chart_type` field. Rust's attribute grammar admits only
        // string literals so the lifted [`HELM_CHART_KEY_TYPE`]
        // constant cannot substitute for the literal syntactically at
        // the attribute-argument site — a future refactor that
        // dropped the `#[serde(rename = "type")]` attribute (or
        // changed the target key to `"Type"` / `"kind"` /
        // `"chartType"`) would silently serialize the field under
        // Rust's default snake_case `chart_type:` key, which Helm's
        // chart-schema parser silently ignores as an unknown top-
        // level key, defaulting the per-chart-kind axis to
        // `application` with no process-log signal. This pin closes
        // the drift by round-tripping a rendered `Chart.yaml` through
        // `serde_yaml::from_str::<serde_yaml::Value>` and asserting
        // the top-level `Mapping::get(HELM_CHART_KEY_TYPE)` resolves
        // (rather than serializing through the [`ChartYaml`]-typed
        // deserializer that would silently absorb the rename drift
        // via `#[serde(default)]` fall-through at the struct-side).
        // Peer to
        // [`chart_yaml_uses_lifted_helm_chart_type_application`] on
        // the sibling per-Chart.yaml per-chart-kind axis-value
        // production-emit pin — the two pins together enforce the
        // full `(key, value)` production-emit pair at the caixa-helm
        // surface for the per-Chart.yaml per-chart-kind discriminator
        // axis.
        let dir = render_chart_for_servico(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let chart_file = dir
            .files
            .iter()
            .find(|f| f.path == PathBuf::from(HELM_CHART_YAML_FILENAME))
            .unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&chart_file.contents).unwrap();
        let mapping = doc.as_mapping().expect(
            "rendered Chart.yaml must be a top-level YAML mapping per \
             the Helm 3 chart-schema shape",
        );
        assert!(
            mapping.contains_key(serde_yaml::Value::String(HELM_CHART_KEY_TYPE.to_string())),
            "rendered Chart.yaml must carry a top-level {HELM_CHART_KEY_TYPE:?} \
             axis-key — a drift on the `#[serde(rename = {HELM_CHART_KEY_TYPE:?})]` \
             attribute at ChartYaml's `chart_type` field silently reroutes the \
             per-chart-kind discriminator axis through Rust's default snake_case \
             serialization (`chart_type:`), which Helm's chart-schema parser \
             silently ignores as an unknown top-level key (defaulting the \
             per-chart-kind axis to `application` with no process-log signal)"
        );
    }

    #[test]
    fn chart_yaml_serializes_app_version_axis_under_lifted_helm_chart_key_app_version() {
        // Fail-before-pass-after drift-detection pin on the
        // `#[serde(rename = "appVersion")]` attribute at [`ChartYaml`]'s
        // `app_version` field. Same attribute-literal-only-grammar
        // constraint the peer
        // [`chart_yaml_serializes_type_axis_under_lifted_helm_chart_key_type`]
        // pin closes on the sibling per-Chart.yaml top-level YAML
        // axis-key applies here: a future refactor that dropped the
        // `#[serde(rename = "appVersion")]` attribute (or changed the
        // target key to `"AppVersion"` / `"applicationVersion"` /
        // `"appversion"`) would silently serialize the field under
        // Rust's default snake_case `app_version:` key, which Helm's
        // chart-schema parser silently drops from the parsed
        // chart-metadata shape, and every downstream Artifact Hub /
        // `helm search` per-chart index falls back to "no application
        // version" for the rendered chart. This pin closes the drift
        // by round-tripping a rendered `Chart.yaml` through
        // `serde_yaml::from_str::<serde_yaml::Value>` (rather than
        // through the [`ChartYaml`]-typed deserializer that would
        // silently absorb the rename drift). Peer to
        // [`chart_yaml_serializes_type_axis_under_lifted_helm_chart_key_type`]
        // on the sibling per-Chart.yaml top-level YAML axis-key
        // serialization pin surface — completes the per-Chart.yaml
        // top-level YAML axis-key production-emit pin pair at the
        // caixa-helm surface for the two serde-rename-literal-only
        // axes.
        let dir = render_chart_for_servico(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let chart_file = dir
            .files
            .iter()
            .find(|f| f.path == PathBuf::from(HELM_CHART_YAML_FILENAME))
            .unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&chart_file.contents).unwrap();
        let mapping = doc.as_mapping().expect(
            "rendered Chart.yaml must be a top-level YAML mapping per \
             the Helm 3 chart-schema shape",
        );
        assert!(
            mapping.contains_key(serde_yaml::Value::String(
                HELM_CHART_KEY_APP_VERSION.to_string()
            )),
            "rendered Chart.yaml must carry a top-level \
             {HELM_CHART_KEY_APP_VERSION:?} axis-key — a drift on the \
             `#[serde(rename = {HELM_CHART_KEY_APP_VERSION:?})]` attribute at \
             ChartYaml's `app_version` field silently reroutes the per-chart \
             underlying-application-version axis through Rust's default \
             snake_case serialization (`app_version:`), which Helm's \
             chart-schema parser silently drops from the parsed chart-metadata \
             shape (every downstream Artifact Hub / `helm search` per-chart \
             index falls back to \"no application version\" for the rendered \
             chart with no process-log signal at the substrate-side emitter site)"
        );
    }

    #[test]
    fn chart_yaml_serializes_api_version_axis_under_lifted_helm_chart_key_api_version() {
        // Fail-before-pass-after drift-detection pin on the
        // `#[serde(rename = "apiVersion")]` attribute at [`ChartYaml`]'s
        // `api_version` field. Same attribute-literal-only-grammar
        // constraint the peer
        // [`chart_yaml_serializes_type_axis_under_lifted_helm_chart_key_type`]
        // / [`chart_yaml_serializes_app_version_axis_under_lifted_helm_chart_key_app_version`]
        // pins close on the sibling per-Chart.yaml top-level YAML
        // axis-keys applies here: a future refactor that dropped the
        // `#[serde(rename = "apiVersion")]` attribute (or changed the
        // target key to `"ApiVersion"` / `"apiversion"` /
        // `"schemaVersion"`) would silently serialize the field under
        // Rust's default snake_case `api_version:` key, which Helm's
        // chart-schema parser rejects at `helm lint` / `helm
        // dependency build` / `helm template` time with an "apiVersion
        // is required" error far from the drift site — every downstream
        // `lareira-<nome>` chart consumer drops with no field naming
        // the serde-rename-drift root cause. This pin closes the drift
        // by round-tripping a rendered `Chart.yaml` through
        // `serde_yaml::from_str::<serde_yaml::Value>` (rather than
        // through the [`ChartYaml`]-typed deserializer that would
        // silently absorb the rename drift via `#[serde(default)]`
        // fall-through at the struct-side). Peer to
        // [`chart_yaml_serializes_type_axis_under_lifted_helm_chart_key_type`]
        // / [`chart_yaml_serializes_app_version_axis_under_lifted_helm_chart_key_app_version`]
        // on the sibling per-Chart.yaml top-level YAML axis-key
        // serialization pin surface — completes the per-Chart.yaml
        // top-level YAML axis-key production-emit pin trio at the
        // caixa-helm surface for the three serde-rename-literal-only
        // axes.
        let dir = render_chart_for_servico(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let chart_file = dir
            .files
            .iter()
            .find(|f| f.path == PathBuf::from(HELM_CHART_YAML_FILENAME))
            .unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&chart_file.contents).unwrap();
        let mapping = doc.as_mapping().expect(
            "rendered Chart.yaml must be a top-level YAML mapping per \
             the Helm 3 chart-schema shape",
        );
        assert!(
            mapping.contains_key(serde_yaml::Value::String(
                HELM_CHART_KEY_API_VERSION.to_string()
            )),
            "rendered Chart.yaml must carry a top-level \
             {HELM_CHART_KEY_API_VERSION:?} axis-key — a drift on the \
             `#[serde(rename = {HELM_CHART_KEY_API_VERSION:?})]` attribute at \
             ChartYaml's `api_version` field silently reroutes the per-chart \
             chart-schema-apiVersion axis through Rust's default snake_case \
             serialization (`api_version:`), which Helm's chart-schema parser \
             rejects at `helm lint` / `helm template` time with an \"apiVersion \
             is required\" error far from the drift site"
        );
    }

    #[test]
    fn chart_dependency_serializes_tetrad_under_lifted_helm_chart_dependency_keys() {
        // Fail-before-pass-after drift-detection pin on the per-
        // `dependencies[]`-entry sub-mapping serde field-name tetrad
        // at [`ChartDependency`]. The four fields are identity-mapped
        // to their target wire keys today (no `#[serde(rename)]` or
        // `#[serde(rename_all)]` attribute on the struct), so a drift
        // would surface as one of two shapes: a rename of the Rust
        // field (`pub name` → `pub nome`) that silently rebrands the
        // wire key, or a `#[serde(rename_all = "camelCase")]` attribute
        // addition that stays a no-op on the four lowercase-identity
        // fields today but silently activates on a future field
        // addition (e.g. an `import_values: Option<Vec<String>>` axis
        // matching Helm 3's per-dep `import-values` sub-key). Either
        // shape silently reroutes the per-dep sub-mapping through a
        // Helm-per-dep-resolver drop at `helm dependency build` time
        // far from the drift site (Helm silently drops the drifted
        // per-dep sub-mapping field and the per-dep resolver falls
        // back to the parsed-shape defaults). This pin closes the
        // drift by serializing a fully-populated [`ChartDependency`]
        // (`alias` set to a non-`None` value so the
        // `#[serde(skip_serializing_if = "Option::is_none")]`
        // attribute doesn't elide the axis from the emitted YAML)
        // through `serde_yaml::to_value` and asserting each of the
        // four per-dep sub-mapping wire keys resolves via
        // `Mapping::get(HELM_CHART_DEPENDENCY_KEY_*)`. Peer to
        // [`chart_yaml_serializes_type_axis_under_lifted_helm_chart_key_type`]
        // / [`chart_yaml_serializes_app_version_axis_under_lifted_helm_chart_key_app_version`]
        // / [`chart_yaml_serializes_api_version_axis_under_lifted_helm_chart_key_api_version`]
        // on the sibling per-Chart.yaml top-level serde-rename-literal-
        // only axis-key drift-detection pin trio (cc44e4b / d29bc23) —
        // extends the drift-detection discipline from the per-Chart.yaml
        // top-level serde-rename-literal axes onto the per-
        // `dependencies[]`-entry sub-mapping serde-field-name tetrad.
        let dep = ChartDependency {
            name: "pleme-computeunit".into(),
            version: "~0.1.0".into(),
            repository: "file://../pleme-computeunit".into(),
            alias: Some("acme-alias".into()),
        };
        let doc = serde_yaml::to_value(&dep).unwrap();
        let mapping = doc.as_mapping().expect(
            "ChartDependency must serialize to a top-level YAML mapping per \
             the Helm 3 per-dep sub-mapping shape",
        );
        for key in [
            HELM_CHART_DEPENDENCY_KEY_NAME,
            HELM_CHART_DEPENDENCY_KEY_VERSION,
            HELM_CHART_DEPENDENCY_KEY_REPOSITORY,
            HELM_CHART_DEPENDENCY_KEY_ALIAS,
        ] {
            assert!(
                mapping.contains_key(serde_yaml::Value::String(key.to_string())),
                "serialized ChartDependency must carry a top-level {key:?} \
                 axis-key — a drift on the `ChartDependency` struct's serde \
                 field-name (a Rust-side rename, an added \
                 `#[serde(rename_all)]` attribute, an added `#[serde(rename)]` \
                 per-field override) silently reroutes the per-dep sub-mapping \
                 through a Helm-per-dep-resolver drop at `helm dependency \
                 build` time far from the drift site (Helm silently drops the \
                 drifted per-dep sub-mapping field and the per-dep resolver \
                 falls back to the parsed-shape defaults); the emitted mapping \
                 keys are {keys:?}",
                keys = mapping
                    .keys()
                    .filter_map(|k| k.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn chart_yaml_serializes_dependencies_axis_under_lifted_helm_chart_key_dependencies() {
        // Fail-before-pass-after drift-detection pin on the top-level
        // per-chart dependency-list YAML axis-key at [`ChartYaml`]'s
        // `dependencies` field. The Rust field name and the emitted
        // wire key coincide by default today (no `#[serde(rename)]`
        // attribute on the field, no `#[serde(rename_all = "…")]`
        // attribute on the struct — so serde emits `dependencies:`
        // verbatim as the top-level list-container YAML key). A future
        // hostile refactor could silently rebrand the wire key in
        // three shapes:
        //
        //   - a rename of the Rust field itself (`pub dependencies:
        //     Vec<ChartDependency>` → `pub deps: Vec<ChartDependency>`
        //     / `pub chart_dependencies: …`), which serde would then
        //     serialize as `deps:` / `chart_dependencies:` verbatim;
        //   - an added `#[serde(rename_all = "camelCase")]` /
        //     `"snake_case"` / `"kebab-case"` attribute on the struct
        //     itself — a no-op on the four identity-mapped top-level
        //     lowercase keys (`name` / `description` / `version` /
        //     `dependencies`) today but silently activates on a future
        //     multi-word field addition (e.g. an `icon_url` axis
        //     matching Helm 3's per-chart `icon:` future-schema slot);
        //   - an added `#[serde(rename = "deps")]` per-field override
        //     at the site of the `dependencies` field.
        //
        // Under any of the three shapes Helm's chart-schema parser
        // silently drops the entire per-chart dep list from the
        // parsed chart-metadata (unknown top-level YAML keys silently
        // ignored per the Helm 3 chart-schema fallthrough), `helm
        // dependency build` finds no chart to vendor, and every
        // rendered `lareira-<nome>` chart's install fails with
        // `template: no template ... associated with template ...`
        // far from the drift site with no field naming the top-level-
        // list-key-drift root cause. This pin closes the drift by
        // round-tripping a rendered `Chart.yaml` through
        // `serde_yaml::from_str::<serde_yaml::Value>` and asserting
        // the top-level `Mapping::get(HELM_CHART_KEY_DEPENDENCIES)`
        // resolves — rather than through the [`ChartYaml`]-typed
        // deserializer that would silently absorb any of the three
        // drift shapes via `#[serde(default)]` fall-through at the
        // struct-side. Peer to
        // [`chart_yaml_serializes_type_axis_under_lifted_helm_chart_key_type`]
        // / [`chart_yaml_serializes_app_version_axis_under_lifted_helm_chart_key_app_version`]
        // / [`chart_yaml_serializes_api_version_axis_under_lifted_helm_chart_key_api_version`]
        // on the sibling per-Chart.yaml top-level YAML axis-key
        // serialization pin surface (d29bc23, cc44e4b) — extends the
        // per-Chart.yaml top-level YAML axis-key production-emit pin
        // trio those closed onto the fourth top-level axis-key, the
        // parent list-container the already-lifted per-
        // `dependencies[]`-entry sub-mapping tetrad
        // [`HELM_CHART_DEPENDENCY_KEY_NAME`] /
        // [`HELM_CHART_DEPENDENCY_KEY_VERSION`] /
        // [`HELM_CHART_DEPENDENCY_KEY_REPOSITORY`] /
        // [`HELM_CHART_DEPENDENCY_KEY_ALIAS`] mounts one level down.
        let dir = render_chart_for_servico(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let chart_file = dir
            .files
            .iter()
            .find(|f| f.path == PathBuf::from(HELM_CHART_YAML_FILENAME))
            .unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&chart_file.contents).unwrap();
        let mapping = doc.as_mapping().expect(
            "rendered Chart.yaml must be a top-level YAML mapping per \
             the Helm 3 chart-schema shape",
        );
        assert!(
            mapping.contains_key(serde_yaml::Value::String(
                HELM_CHART_KEY_DEPENDENCIES.to_string()
            )),
            "rendered Chart.yaml must carry a top-level \
             {HELM_CHART_KEY_DEPENDENCIES:?} axis-key — a drift on the \
             `ChartYaml.dependencies` field's serde field-name (a Rust-side \
             rename to `deps` / `chart_dependencies`, an added \
             `#[serde(rename_all)]` attribute on the enclosing struct, an \
             added `#[serde(rename)]` per-field override) silently reroutes \
             the per-chart dependency-list axis through an unrecognized \
             top-level YAML key (`deps:` / `chartDependencies:` / \
             `chart_dependencies:`), which Helm's chart-schema parser \
             silently drops from the parsed chart-metadata shape (every \
             rendered `lareira-<nome>` chart's install fails with \
             `template: no template ... associated with template ...` at \
             `helm dependency build` / `helm template` time far from the \
             drift site with no field naming the top-level-list-key-drift \
             root cause); the emitted top-level mapping keys are {keys:?}",
            keys = mapping
                .keys()
                .filter_map(|k| k.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn helm_chart_key_dependencies_re_export_points_at_caixa_core_canonical() {
        // The renderer's [`HELM_CHART_KEY_DEPENDENCIES`] was lifted onto
        // a re-export of [`caixa_core::HELM_CHART_KEY_DEPENDENCIES`] so
        // the Helm 3 per-Chart.yaml top-level dependency-list-container
        // YAML axis-key — the parent whose per-`dependencies[]`-entry
        // sub-mapping tetrad [`HELM_CHART_DEPENDENCY_KEY_NAME`] /
        // [`HELM_CHART_DEPENDENCY_KEY_VERSION`] /
        // [`HELM_CHART_DEPENDENCY_KEY_REPOSITORY`] /
        // [`HELM_CHART_DEPENDENCY_KEY_ALIAS`] mounts one level down —
        // lives in exactly one place across every caixa renderer. Pin
        // the equality + `&'static` static-data identity here so any
        // local re-introduction of a sibling `pub const
        // HELM_CHART_KEY_DEPENDENCIES: &str = "…"` at this crate — the
        // canonical drift footgun where a sibling local `pub const`
        // could happen to carry the same string at the source while
        // pointing at a different `&'static` allocation — is a build-
        // time test failure naming the offending drift, not a silent
        // per-Chart.yaml top-level-list-key reroute at `helm dependency
        // build` / `helm lint` / `helm template` time far from the
        // drift site. Peer to
        // [`helm_chart_yaml_filename_re_export_points_at_caixa_core_canonical`]
        // and every other `helm_chart_*_re_export_points_at_caixa_core_canonical`
        // pin on the sibling canonical-Helm-chart-schema-body-axis
        // re-export surface — extends the per-Chart.yaml top-level
        // YAML axis-key re-export identity discipline onto the fourth
        // top-level axis-key at the caixa-helm surface.
        caixa_core::assert_str_reexport_identity(
            "HELM_CHART_KEY_DEPENDENCIES",
            HELM_CHART_KEY_DEPENDENCIES,
            caixa_core::HELM_CHART_KEY_DEPENDENCIES,
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
    fn helm_values_yaml_filename_re_export_points_at_caixa_core_canonical() {
        // The renderer's `HELM_VALUES_YAML_FILENAME` was lifted from the
        // twelve production + test-side inline `"values.yaml"` /
        // `PathBuf::from("values.yaml")` / `chart_root.join("values.yaml")`
        // literals across [`render_chart_for_servico`]'s `ChartDir`
        // values-file `path` emit site + every test-side round-trip
        // navigator that reaches into the rendered `ChartDir` by the
        // values filename to a re-export of
        // [`caixa_core::HELM_VALUES_YAML_FILENAME`] so the Helm 3
        // per-chart-directory values-file filename lives in exactly one
        // place across every caixa renderer. Pin the equality +
        // `&'static` static-data identity here so any local
        // re-introduction of a sibling `pub const
        // HELM_VALUES_YAML_FILENAME: &str = "…"` at this crate — the
        // canonical drift footgun where a sibling local `pub const` could
        // happen to carry the same string at the source while pointing
        // at a different `&'static` allocation — is a build-time test
        // failure naming the offending drift, not a silent
        // Helm-per-chart-values-loader fall-through to the empty values
        // block at `helm template` / `helm install` time far from the
        // drift site (where the workload silently comes up under the
        // library chart's admission-time defaults with no per-Servico
        // M2 overlay applied). Peer to
        // [`helm_chart_yaml_filename_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-Helm-per-chart-directory-metadata-file-
        // axis re-export surface — completes the
        // per-`lareira-<nome>`-chart-directory `(Chart.yaml, values.yaml)`
        // canonical-per-chart-directory-filename-axis re-export pair
        // every rendered chart declares as its two schema-load-bearing
        // `ChartDir::files` entries.
        caixa_core::assert_str_reexport_identity(
            "HELM_VALUES_YAML_FILENAME",
            HELM_VALUES_YAML_FILENAME,
            caixa_core::HELM_VALUES_YAML_FILENAME,
        );
    }

    #[test]
    fn helm_chart_readme_filename_re_export_points_at_caixa_core_canonical() {
        // The renderer's `HELM_CHART_README_FILENAME` was lifted from the
        // three production + test-side inline `"README.md"` literals
        // across [`render_chart_for_servico`]'s `ChartDir` readme-file
        // `path` emit site + every test-side round-trip navigator that
        // reaches into the rendered `ChartDir` by the readme filename
        // (the [`renders_three_files`] files-vec-membership pin + the
        // [`ChartDir::write_to`] post-write existence pin) to a
        // re-export of [`caixa_core::HELM_CHART_README_FILENAME`] so the
        // per-`lareira-<nome>` chart-directory human-facing readme
        // filename lives in exactly one place across every caixa
        // renderer. Pin the equality + `&'static` static-data identity
        // here so any local re-introduction of a sibling `pub const
        // HELM_CHART_README_FILENAME: &str = "…"` at this crate — the
        // canonical drift footgun where a sibling local `pub const`
        // could happen to carry the same string at the source while
        // pointing at a different `&'static` allocation — is a build-
        // time test failure naming the offending drift, not a silent
        // GitHub / Artifact Hub / any per-chart README-surfacing UI
        // fall-through to "no README available" at chart-consumption
        // time far from the drift site. Peer to
        // [`helm_chart_yaml_filename_re_export_points_at_caixa_core_canonical`]
        // / [`helm_values_yaml_filename_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-Helm-per-chart-directory-filename
        // axis re-export surfaces — completes the
        // per-`lareira-<nome>`-chart-directory `(Chart.yaml,
        // values.yaml, README.md)` canonical-per-chart-directory-
        // filename-axis re-export triple every rendered chart declares
        // as its three `ChartDir::files` entries.
        caixa_core::assert_str_reexport_identity(
            "HELM_CHART_README_FILENAME",
            HELM_CHART_README_FILENAME,
            caixa_core::HELM_CHART_README_FILENAME,
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
            .find(|f| f.path == PathBuf::from(HELM_VALUES_YAML_FILENAME))
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

    #[test]
    fn computeunit_spec_key_module_re_export_points_at_caixa_core_canonical() {
        // The renderer's `COMPUTEUNIT_SPEC_KEY_MODULE` was lifted from
        // the two inline `"module"` test-side call sites in this crate
        // (`values_yaml_wraps_under_pleme_computeunit_key`'s per-values
        // module-block present-check + the peer navigator on the
        // `library_name`-override wrap-key axis
        // `values_yaml_wrap_key_follows_library_name_override`) — every
        // per-Servico ComputeUnit CRD `spec.module` sub-block readback
        // in this crate now navigates through the same `&'static str`
        // re-exported to a re-export of
        // [`caixa_core::COMPUTEUNIT_SPEC_KEY_MODULE`] so the canonical
        // ComputeUnit-CRD per-`spec.*` wasm-module-reference axis lives
        // in exactly one place across every caixa renderer. Pin the
        // equality + static-data identity here so any local re-
        // introduction of a sibling `pub const COMPUTEUNIT_SPEC_KEY_MODULE:
        // &str = "…"` at this crate is a build-time test failure naming
        // the offending drift, not a silent per-Servico wasm-runtime-
        // binding drop at cluster-apply time. Peer to
        // [`helm_values_key_enabled_re_export_points_at_caixa_core_canonical`]
        // /
        // [`kube_key_spec_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-Helm-load-bearing-string /
        // canonical-K8s-CR-body-key re-export axes +
        // `caixa_flux::tests::computeunit_spec_key_module_re_export_points_at_caixa_core_canonical`
        // on the peer per-Servico renderer crate.
        caixa_core::assert_str_reexport_identity(
            "COMPUTEUNIT_SPEC_KEY_MODULE",
            COMPUTEUNIT_SPEC_KEY_MODULE,
            caixa_core::COMPUTEUNIT_SPEC_KEY_MODULE,
        );
    }

    #[test]
    fn computeunit_spec_key_trigger_re_export_points_at_caixa_core_canonical() {
        // Peer to
        // [`computeunit_spec_key_module_re_export_points_at_caixa_core_canonical`]
        // on the same ComputeUnit-CRD per-`spec.*` sub-block re-export
        // surface — pins the per-Servico invocation-shape sub-block
        // key's identity on the same trajectory.
        caixa_core::assert_str_reexport_identity(
            "COMPUTEUNIT_SPEC_KEY_TRIGGER",
            COMPUTEUNIT_SPEC_KEY_TRIGGER,
            caixa_core::COMPUTEUNIT_SPEC_KEY_TRIGGER,
        );
    }

    #[test]
    fn computeunit_spec_key_capabilities_re_export_points_at_caixa_core_canonical() {
        // Peer to
        // [`computeunit_spec_key_module_re_export_points_at_caixa_core_canonical`]
        // and
        // [`computeunit_spec_key_trigger_re_export_points_at_caixa_core_canonical`]
        // on the same ComputeUnit-CRD per-`spec.*` sub-block re-export
        // surface — completes the substrate-side ComputeUnit-CRD
        // per-`spec.*` sub-block re-export triple in this crate on the
        // WASI-capability-token-list axis.
        caixa_core::assert_str_reexport_identity(
            "COMPUTEUNIT_SPEC_KEY_CAPABILITIES",
            COMPUTEUNIT_SPEC_KEY_CAPABILITIES,
            caixa_core::COMPUTEUNIT_SPEC_KEY_CAPABILITIES,
        );
    }

    #[test]
    fn chart_file_alias_resolves_to_caixa_core_rendered_file() {
        // Type-alias identity pin: the [`ChartFile`] alias at this
        // crate's boundary resolves to the canonical
        // [`caixa_core::RenderedFile`] the substrate-side "one rendered
        // leaf artifact" shape lives at. `let _: ChartFile = <a
        // RenderedFile>` type-checks *iff* [`ChartFile`] is the aliased
        // canonical (not a sibling pub-struct re-declaration that
        // happens to carry the same field pair — that would compile
        // past the struct-literal navigators below but fail this
        // assignment). A drifted local `pub struct ChartFile { pub
        // path: PathBuf, pub contents: String }` at this crate — the
        // canonical drift footgun that would carry the same field pair
        // at the source while pointing at a different struct
        // definition — trips this pin at caixa-helm build time rather
        // than surfacing as a downstream `caixa_core::RenderedFile`
        // consumer refusing a `ChartFile`-shaped value at type-check
        // time far from the drift commit. Peer to the sibling
        // [`caixa_flux::BundleFile`]-alias-identity pin on the same
        // per-target-renderer canonical [`caixa_core::RenderedFile`]
        // re-export surface — both crates' per-artifact leaf type now
        // resolves through the same canonical struct definition, so a
        // future rebrand on the record shape lands at one caixa-core
        // edit and reaches both consumers by construction.
        let canonical: caixa_core::RenderedFile = caixa_core::RenderedFile {
            path: PathBuf::from(HELM_CHART_YAML_FILENAME),
            contents: String::new(),
        };
        let aliased: ChartFile = canonical.clone();
        assert_eq!(aliased, canonical);
        // Struct-literal construction still resolves through the alias
        // — the pre-lift `ChartFile { path, contents }` shape at every
        // production emit site (three sites in
        // `render_chart_for_servico_with`'s `ChartDir::files` assembly)
        // continues to compile, and the derive tuple travels through
        // the alias so downstream `ChartDir::files.iter().find(|f|
        // f.path == PathBuf::from(HELM_VALUES_YAML_FILENAME))`
        // navigators keep matching by `PartialEq` on `PathBuf`.
        let via_alias = ChartFile {
            path: PathBuf::from(HELM_VALUES_YAML_FILENAME),
            contents: format!("{DEFAULT_LIBRARY_NAME}:\n  enabled: false\n"),
        };
        assert_eq!(via_alias.path.to_string_lossy(), HELM_VALUES_YAML_FILENAME);
    }

    #[test]
    fn chart_file_new_constructor_travels_through_alias_to_canonical() {
        // Inherent-method-through-alias pin: the canonical
        // [`caixa_core::RenderedFile::new`] `impl Into<PathBuf>` /
        // `impl Into<String>` constructor every per-artifact leaf in
        // [`render_chart_for_servico_with`] now routes through
        // resolves at `ChartFile::new(…)` — Rust inherent methods
        // travel through a `pub type ChartFile = caixa_core::RenderedFile`
        // alias to the aliased canonical at name resolution, so a
        // drifted local `pub struct ChartFile { pub path: PathBuf, pub
        // contents: String }` at this crate would carry the field
        // pair the sibling type-alias-identity pin above still
        // accepts (both records share `pub path` / `pub contents`
        // shape) while dropping the constructor — the six sweep sites
        // in [`render_chart_for_servico_with`] would stop compiling
        // and the failing calls would name `ChartFile` directly,
        // making the drift-source unambiguous. This test pins the
        // constructor's per-alias reachability + the byte-identical
        // record shape against a `HELM_CHART_YAML_FILENAME`-keyed
        // probe so the pin fires at caixa-helm build time.
        let via_alias_new: ChartFile = ChartFile::new(HELM_CHART_YAML_FILENAME, "apiVersion: v2\n");
        let via_canonical_new = caixa_core::RenderedFile::new(
            HELM_CHART_YAML_FILENAME,
            String::from("apiVersion: v2\n"),
        );
        assert_eq!(via_alias_new, via_canonical_new);
        assert_eq!(via_alias_new.path, PathBuf::from(HELM_CHART_YAML_FILENAME));
        assert_eq!(via_alias_new.contents, "apiVersion: v2\n");
    }

    #[test]
    fn render_opts_default_library_version_follows_lifted_constant() {
        // Peer of [`render_opts_default_library_name_follows_lifted_constant`]
        // (which pins the same alignment on the sibling
        // [`RenderOpts::library_name`] / [`DEFAULT_LIBRARY_NAME`] axis). The
        // [`RenderOpts::default()`] impl sets `library_version` from
        // [`DEFAULT_LIBRARY_VERSION`]; a future refactor that detached the
        // default-knob from the lifted constant — accidentally re-inlining
        // `"~0.1.0"` in the impl body — would silently split the value the
        // default knob threads into every rendered `Chart.yaml`
        // `dependencies[0].version` axis from the const the const's callers
        // (and this crate's future per-`DEFAULT_LIBRARY_VERSION` drift pins)
        // read. The two `Chart.yaml`-dep `(name, version)` scalar-axes now
        // share the same "default-knob follows lifted constant, byte for
        // byte" pin discipline the peer library-name axis carries.
        let opts = RenderOpts::default();
        assert_eq!(opts.library_version, DEFAULT_LIBRARY_VERSION);
        assert_eq!(opts.library_version, "~0.1.0");
    }

    #[test]
    fn render_opts_default_enabled_default_follows_lifted_constant() {
        // Peer of [`render_opts_default_library_name_follows_lifted_constant`]
        // /
        // [`render_opts_default_library_version_follows_lifted_constant`]
        // / [`render_opts_default_library_repo_follows_lifted_constant`] —
        // the fourth leg of the [`RenderOpts::default()`]-body
        // default-knob-follows-lifted-constant quartet. The
        // [`RenderOpts::default()`] impl seeds `enabled_default` from
        // [`STANDALONE_LAREIRA_ENABLED_DEFAULT`]; every rendered
        // `lareira-<nome>` chart's `values.yaml` under-`<library>.enabled`
        // scalar reads through this knob, so a future refactor that
        // detached the default-knob from the lifted constant —
        // accidentally re-inlining `false` in the impl body — would
        // silently split the `bool` the default seed writes from the
        // const the drift-detection pin
        // [`standalone_lareira_enabled_default_pins_canonical_value`]
        // (in caixa-core) reads. The rendered values block would then
        // carry one `bool` at the emit site while the const-consuming
        // sibling test-fixture navigators (this crate's future per-
        // `STANDALONE_LAREIRA_ENABLED_DEFAULT` drift pins) read another,
        // and the substrate's chosen mirror-symmetric standalone /
        // composition per-values-block child-chart-enablement-toggle-
        // scalar-value default pair would silently disagree on the
        // standalone-path half. Peer with the sibling
        // `caixa_flux::tests::cluster_bundle_lareira_enabled_default_re_export_matches_caixa_core_canonical_value`
        // pin on the composition-path half of the same
        // [`HELM_VALUES_KEY_ENABLED`] scalar-axis pair.
        let opts = RenderOpts::default();
        assert_eq!(opts.enabled_default, STANDALONE_LAREIRA_ENABLED_DEFAULT);
        assert!(!opts.enabled_default);
    }

    #[test]
    fn standalone_lareira_enabled_default_re_export_matches_caixa_core_canonical_value() {
        // The renderer's `STANDALONE_LAREIRA_ENABLED_DEFAULT` was lifted
        // from the [`RenderOpts::default()`] impl-body inline `false`
        // scalar-value literal at `caixa-helm/src/lib.rs:700` to a
        // re-export of [`caixa_core::STANDALONE_LAREIRA_ENABLED_DEFAULT`]
        // so the substrate-side default the standalone per-chart path
        // seeds under the sibling [`HELM_VALUES_KEY_ENABLED`]
        // leaf-scalar-key lives in exactly one place across every caixa
        // renderer (this crate's standalone per-chart path + the peer
        // `caixa_flux::cluster_bundle`'s composition per-cluster-
        // `HelmRelease` values-overlay path, which reads through the
        // inverse [`caixa_flux::CLUSTER_BUNDLE_LAREIRA_ENABLED_DEFAULT`]
        // re-export). Pin the equality here so any local re-introduction
        // of a sibling `pub const STANDALONE_LAREIRA_ENABLED_DEFAULT:
        // bool = …` at this crate (the canonical drift footgun the peer
        // `CLUSTER_BUNDLE_LAREIRA_ENABLED_DEFAULT` re-export identity
        // pin's rationale names as the recurring shape) is a build-time
        // test failure naming the offending drift, not a silent
        // apply-time toggle-mismatch routing the standalone per-chart
        // `values.<library>.enabled` seed onto one substrate-side
        // opt-out convention while the peer composition-path override
        // routes onto another. Peer to the sibling
        // `caixa_flux::tests::cluster_bundle_lareira_enabled_default_re_export_matches_caixa_core_canonical_value`
        // on the composition-path half of the same
        // [`HELM_VALUES_KEY_ENABLED`] scalar-axis pair — the two
        // per-path re-export identity pins together lock the two peer
        // scalar-value defaults' per-crate re-exports onto their shared
        // caixa-core canonical.
        assert_eq!(
            STANDALONE_LAREIRA_ENABLED_DEFAULT,
            caixa_core::STANDALONE_LAREIRA_ENABLED_DEFAULT,
            "STANDALONE_LAREIRA_ENABLED_DEFAULT re-export must remain the \
             same `bool` as its caixa-core canonical — a drifted local \
             `pub const STANDALONE_LAREIRA_ENABLED_DEFAULT: bool = …` at \
             caixa-helm would silently split the substrate's chosen \
             standalone per-chart opt-out seed from the peer \
             composition-path force-on inversion the caixa-core canonical \
             encodes."
        );
        assert!(
            !STANDALONE_LAREIRA_ENABLED_DEFAULT,
            "STANDALONE_LAREIRA_ENABLED_DEFAULT must remain `false` — the \
             standalone per-chart path is the substrate-side opt-out path \
             where cluster operators must opt each caixa in per-cluster, \
             inverse of the composition per-cluster-HelmRelease values-\
             overlay path's opt-in force-on."
        );
    }

    #[test]
    fn render_opts_default_library_repo_follows_lifted_constant() {
        // Peer of [`render_opts_default_library_name_follows_lifted_constant`]
        // /
        // [`render_opts_default_library_version_follows_lifted_constant`] —
        // the third leg of the per-`Chart.yaml`-dep
        // `(repository, name, version)` default-knob triple. The
        // [`RenderOpts::default()`] impl seeds `library_repo` from
        // [`DEFAULT_LIBRARY_REPO`]; every rendered `lareira-<nome>` chart's
        // `Chart.yaml` `dependencies[0].repository` field reads through
        // this knob, so a future refactor that detached the default-knob
        // from the lifted constant — re-inlining
        // `"file://../pleme-computeunit"` in the impl body — would silently
        // split the URL the default seed writes from the const the
        // drift-detection pin below
        // ([`default_library_repo_ends_with_lifted_default_library_name`])
        // reads.
        let opts = RenderOpts::default();
        assert_eq!(opts.library_repo, DEFAULT_LIBRARY_REPO);
        assert_eq!(opts.library_repo, "file://../pleme-computeunit");
    }

    #[test]
    fn default_library_version_parses_as_valid_semver_requirement() {
        // Structural pin: [`DEFAULT_LIBRARY_VERSION`] carries a Cargo-shaped
        // semver-requirement string that lands verbatim in every rendered
        // `lareira-<nome>` chart's `Chart.yaml` `dependencies[0].version`
        // field. Helm 3's chart-schema parser (`helm dependency build`,
        // `helm lint`, `helm template`, `helm install`) validates the
        // scalar against the same `semver::VersionReq` grammar
        // [`caixa_core::parse_requirement`] wraps, and rejects a malformed
        // shape (`"~0.1.,0"` — paste-from-typography stray comma;
        // `"v0.1.0"` — accidental Zig-style publish-tag prefix leaking back
        // from [`caixa_core::DEFAULT_PUBLISH_TAG_PREFIX`] into the
        // requirement axis; `"0.1"` with a trailing sigil dropped by a
        // fat-fingered edit) with the load-bearing `Error: found operator
        // …, expected version` diagnostic surfacing at chart-consumption
        // time — far from the constant-drift commit's source, with no
        // field naming the offending caixa or the drifted default. Routing
        // through [`caixa_core::parse_requirement`] here — the same
        // requirement-parser entry-point every peer typed `:versao`
        // requirement slot (`:deps`, `:deps-dev`, `:membros`, `:children`)
        // routes through via
        // [`caixa_core::require_valid_versao_requirement`] — closes the
        // drift structurally at caixa-helm build time and pins the const's
        // accepted set to exactly the set the peer author-facing
        // requirement axes accept: any shape a caixa author cannot write
        // in `:deps :versao` is a shape the substrate cannot seed as the
        // library-chart-dep default. Peer of the sibling
        // [`default_library_repo_ends_with_lifted_default_library_name`]
        // structural pin on the co-resident `(name, version)` per-Chart.yaml
        // dep-scalar pair.
        caixa_core::parse_requirement(DEFAULT_LIBRARY_VERSION).unwrap_or_else(|e| {
            panic!(
                "DEFAULT_LIBRARY_VERSION {DEFAULT_LIBRARY_VERSION:?} must parse as a valid \
                 semver::VersionReq — every rendered lareira-<nome> chart's Chart.yaml \
                 dependencies[0].version axis lands this scalar verbatim, and Helm 3's \
                 chart-schema parser rejects a malformed shape at chart-consumption time \
                 far from the constant-drift commit's source: {e}",
            )
        });
    }

    #[test]
    fn default_library_repo_ends_with_lifted_default_library_name() {
        // Structural cross-const coherence pin: [`DEFAULT_LIBRARY_REPO`]
        // embeds the [`DEFAULT_LIBRARY_NAME`] byte-string verbatim as its
        // trailing directory-name component (the canonical
        // `file://../<library-chart-name>` shape every sibling
        // `lareira-<nome>` chart's `Chart.yaml` `dependencies[0]` entry
        // consults for a two-axis `(name, repository)` per-dep tuple that
        // Helm's per-chart-dep resolver `(chart-source-scheme + chart-name)`
        // navigator round-trips). The two axes must stay coupled: the
        // library-chart-directory on disk (the repo's trailing component)
        // and the library-chart's declared `name:` in its own
        // [`DEFAULT_LIBRARY_NAME`]-published `Chart.yaml` are the same
        // load-bearing chart-name identity. Prior to this pin the two
        // consts were independently authored — a future substrate-side
        // library-chart rebrand (`pleme-computeunit` → `pleme-cu` on a
        // shorter-form migration, `pleme-computeunit` →
        // `caixa-computeunit` on a substrate-alignment migration, a
        // per-edition library-chart fork the [`DEFAULT_LIBRARY_NAME`]
        // docstring names as a trajectory item) on the
        // [`caixa_core::DEFAULT_LIBRARY_NAME`] canonical without a
        // coordinated edit on this crate's [`DEFAULT_LIBRARY_REPO`] would
        // silently emit rendered `Chart.yaml` documents whose
        // `dependencies[0].name` names the new chart while
        // `dependencies[0].repository` points at the old directory —
        // `helm dependency build` would refuse to resolve the dep ("chart
        // <new-name> not found in file://../<old-name>") at chart-
        // consumption time, far from the constant-rebrand commit's source,
        // with no field naming the two-axis coherence drift root cause.
        // Pinning the structural `ends_with(DEFAULT_LIBRARY_NAME)` invariant
        // here surfaces the drift as a caixa-helm build-time test failure
        // and forces the coordinated `(REPO, NAME)` edit to move together.
        // Peer of the sibling
        // [`default_library_version_parses_as_valid_semver_requirement`]
        // structural pin on the co-resident `(name, version)` per-Chart.yaml
        // dep-scalar pair — completes the `(repository, name, version)`
        // per-Chart.yaml-dep default-triple's structural pin surface.
        assert!(
            DEFAULT_LIBRARY_REPO.ends_with(DEFAULT_LIBRARY_NAME),
            "DEFAULT_LIBRARY_REPO {DEFAULT_LIBRARY_REPO:?} must terminate with the lifted \
             DEFAULT_LIBRARY_NAME {DEFAULT_LIBRARY_NAME:?} — the two-axis (repository, name) \
             per-Chart.yaml-dep tuple must resolve to the same library-chart identity on \
             disk, so a rebrand on either axis must move both",
        );
    }
}
