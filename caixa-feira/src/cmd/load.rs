//! Shared per-verb IO entry-point for the `<root>/caixa.lisp` manifest.
//!
//! Closes the PRIME DIRECTIVE duplication on the verb-entry IO axis the
//! `feira` CLI carries. Before this lift the same four-line shape —
//! `root.join("caixa.lisp")` → `std::fs::read_to_string` →
//! `Caixa::from_lisp`, all with verbatim `"reading {…}"` and
//! `"parsing {…}"` `.with_context(...)` strings — appeared at every
//! `feira` verb's `Run::run` entry-point: `feira add` (cmd/add.rs),
//! `feira build` (cmd/build.rs), `feira chart` (cmd/chart.rs),
//! `feira deploy` (cmd/deploy.rs), `feira lock` (cmd/lock.rs),
//! `feira nix` (cmd/nix.rs), `feira publish` (cmd/publish.rs),
//! `feira resolve` (cmd/resolve.rs), and the `feira app` verbs'
//! shared `load_aplicacao` helper (cmd/app.rs). Nine inline copies of
//! the same load-bearing diagnostic shape, each one another place a
//! future change to the `"reading {…}"` / `"parsing {…}"` discipline
//! has to remember to touch.
//!
//! After the lift every per-verb entry-point routes through
//! [`load_caixa`]: the manifest-filename literal lives at exactly one
//! `pub(crate) const CAIXA_MANIFEST_FILENAME` definition, the
//! `read_to_string` and `Caixa::from_lisp` calls are wrapped with the
//! canonical context strings at exactly one call-site, and the
//! underlying `tatara_lisp::LispError` remains reachable on the
//! anyhow cause chain for typed-view downstream callers (peer with
//! the per-renderer typed-view discipline `KindMismatch`,
//! `ServicoCountMismatch`, etc. follow). A future per-caixa-root
//! `feira` verb — the future `feira oci publish` per-Servico OCI
//! packager, the future `feira validate --strict` per-caixa
//! admission verb, the future M4 `feira reconcile` per-cluster
//! diff verb the absorption-roadmap acknowledges — inherits the
//! canonical entry-point shape through this helper rather than
//! re-derives a skewed copy.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use caixa_core::Caixa;

/// The canonical caixa manifest filename. Every `feira` verb's
/// per-caixa-root entry-point resolves `<root>/caixa.lisp` through
/// [`caixa_manifest_path`] and reaches for the manifest through
/// [`load_caixa`]; no verb should hard-code the literal string.
pub(crate) const CAIXA_MANIFEST_FILENAME: &str = "caixa.lisp";

/// The canonical "where does `--path` default to?" answer — the
/// current working directory as a relative `PathBuf`. Every `feira`
/// verb's `Option<PathBuf>` `--path` flag resolves through
/// [`caixa_root`] with this literal as the absent-flag fallback; no
/// verb should hard-code the `"."` string.
pub(crate) const CAIXA_ROOT_DEFAULT_DIRNAME: &str = ".";

/// Resolve the per-caixa-root manifest path.
pub(crate) fn caixa_manifest_path(root: &Path) -> PathBuf {
    root.join(CAIXA_MANIFEST_FILENAME)
}

/// Resolve a `feira` verb's `--path` flag into the canonical caixa
/// root, defaulting to the current working directory when the flag
/// is absent.
///
/// Closes the PRIME DIRECTIVE duplication (THEORY.md §I.5 — "the
/// duplication budget is zero") on the per-verb `--path` fallback
/// axis. Before this lift the same one-line shape —
/// `self.path.clone().unwrap_or_else(|| PathBuf::from("."))` —
/// appeared verbatim at every `feira` verb's `Run::run` entry-point:
/// `feira add` (cmd/add.rs), `feira build` (cmd/build.rs),
/// `feira chart` (cmd/chart.rs), `feira deploy` (cmd/deploy.rs),
/// `feira lock` (cmd/lock.rs), `feira nix` (cmd/nix.rs),
/// `feira publish` (cmd/publish.rs), `feira resolve`
/// (cmd/resolve.rs), and the `feira tofu render` helper plus the
/// `feira app` verbs' shared `load_aplicacao` helper (cmd/app.rs).
/// Ten inline copies of the same load-bearing fallback decision,
/// each one another place a future change to the "where does
/// `--path` default land?" discipline has to remember to touch —
/// the future M4 `feira reconcile` per-cluster diff verb the
/// absorption-roadmap acknowledges, a future `CAIXA_ROOT` env-var
/// override, a future "walk-up-until-`caixa.lisp`" discovery —
/// every one inherits the drift-budget if the choice lives at every
/// verb.
///
/// After the lift every per-verb entry-point routes through this
/// resolver: the fallback decision lives at exactly one call-site,
/// the literal `"."` string lands at exactly one definition (the
/// canonical [`CAIXA_ROOT_DEFAULT_DIRNAME`] constant, peer with the
/// [`CAIXA_MANIFEST_FILENAME`] constant on the manifest-filename
/// axis), and a future `feira` verb inherits the canonical shape
/// through this helper rather than re-derives a skewed copy. Mirrors
/// [`caixa_manifest_path`] on the per-caixa-root path-resolution
/// surface — together the two resolvers + the [`load_caixa`] /
/// [`load_yaml`] readers form the canonical per-verb IO entry-point
/// shape (`--path` → root → manifest path → typed Caixa).
pub(crate) fn caixa_root(path: Option<&Path>) -> PathBuf {
    path.map_or_else(
        || PathBuf::from(CAIXA_ROOT_DEFAULT_DIRNAME),
        Path::to_path_buf,
    )
}

/// Read + parse the per-caixa-root `caixa.lisp` into a typed
/// [`Caixa`].
///
/// The canonical per-verb IO entry-point — every `feira` verb that
/// consumes a typed `Caixa` routes through this helper, so the
/// `"reading {…}"` / `"parsing {…}"` context strings stay verbatim
/// across every verb's rendered diagnostic. The underlying
/// `tatara_lisp::LispError` (the `Caixa::from_lisp` error type) is
/// preserved through the anyhow `.with_context(...)` wrap and remains
/// reachable on the cause chain — peer with the per-renderer
/// typed-view discipline (`KindMismatch`, `ServicoCountMismatch`,
/// etc.) the substrate's per-Servico / per-Aplicacao entry-points
/// already follow.
pub(crate) fn load_caixa(root: &Path) -> Result<Caixa> {
    let manifest = caixa_manifest_path(root);
    let src = std::fs::read_to_string(&manifest)
        .with_context(|| format!("reading {}", manifest.display()))?;
    Caixa::from_lisp(&src).with_context(|| format!("parsing {}", manifest.display()))
}

/// Read + parse a YAML file at `path` into an untyped
/// [`serde_yaml::Value`].
///
/// Closes the PRIME DIRECTIVE duplication on the per-verb YAML IO
/// axis. Before this lift the same four-line shape —
/// [`std::fs::read_to_string`] + [`serde_yaml::from_str`], both with
/// verbatim `"reading {…}"` / `"parsing {…}"` `.with_context(...)`
/// strings — appeared at every `feira` verb that consumes an
/// arbitrary YAML file: the per-Servico computeunit loader (`feira
/// chart` + `feira deploy` via [`super::chart::load_first_servico_yaml`])
/// and the cluster-side fleet-programs HelmRelease loader (`feira
/// deploy` itself). Two inline copies of the same load-bearing
/// diagnostic shape, each one another place a future change to the
/// `"reading {…}"` / `"parsing {…}"` discipline has to remember to
/// touch, and another place a future per-YAML `feira` verb (the
/// future `feira validate --strict` per-caixa admission verb, the
/// future M4 `feira reconcile` cluster-diff verb the
/// absorption-roadmap acknowledges) would re-derive a skewed copy.
///
/// After the lift every per-YAML `feira` verb routes through this
/// helper: the `"reading {…}"` / `"parsing {…}"` context strings live
/// at exactly one call-site, the underlying `serde_yaml::Error`
/// remains reachable on the anyhow cause chain (peer with the
/// per-renderer [`caixa_core::KindMismatch`] /
/// [`caixa_core::ServicoCountMismatch`] downcast discipline), and the
/// canonical entry-point shape is a single helper rather than two
/// independently-evolved copies. Mirrors [`load_caixa`] on the peer
/// `<root>/caixa.lisp` axis, completing the canonical per-verb IO
/// surface (the manifest loader + this YAML loader).
pub(crate) fn load_yaml(path: &Path) -> Result<serde_yaml::Value> {
    let src =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_yaml::from_str(&src).with_context(|| format!("parsing {}", path.display()))
}

/// Validate a `feira` verb's `--cluster <name>` flag value at the
/// per-verb entry-point against the canonical DNS-1123 label shape every
/// downstream consumer (the K8s apiserver's per-Service / per-Pod /
/// per-CR `metadata.name` axis, the `:placement :clusters` typed slot,
/// the `<k8s-repo>/clusters/<cluster>/…` GitOps path join) demands.
///
/// Closes the PRIME DIRECTIVE duplication (THEORY.md §I.3.5 — "the
/// duplication budget is zero") on the per-verb `--cluster` axis. Before
/// this lift the same `--cluster` value passed through both
/// [`super::deploy::Deploy::run`] (the per-Servico `feira deploy`
/// per-cluster `<k8s-repo>/clusters/<cluster>/programs/release.yaml`
/// path-join at `cmd/deploy.rs:83`) and
/// [`super::app::DeployArgs::run`] (the whole-Aplicacao `feira app
/// deploy` per-cluster `<k8s-repo>/clusters/<cluster>/aplicacaos/<nome>/
/// manifests.yaml` path-join at `cmd/app.rs:156`) with no shape gate
/// at all — every footgun the author surface's `:placement :clusters`
/// slot already refuses through
/// [`caixa_core::AplicacaoError::PlacementClusterInvalid`] (the 6c8c00b
/// lift on the sibling typed-slot axis) silently passed at the verb
/// arg-entry. Two unprotected call-sites of the same load-bearing
/// shape, each one another place a future change to the canonical
/// cluster-name discipline has to remember to touch — the future M4
/// `feira reconcile --cluster <name>` cluster-diff verb the
/// absorption-roadmap acknowledges, the future per-cluster
/// `feira oci publish --cluster <name>` registry-push verb, the future
/// `feira app status --cluster <name>` introspection verb — each one
/// landing on a per-verb open-coded gate is a coordinated rewrite of
/// every verb-entry-point that either gets it right at every site or
/// quietly drifts at one.
///
/// Without the gate three authoring footguns silently passed:
///
///   - `--cluster ""` — the canonical "I forgot the flag value"
///     footgun. The path join landed at
///     `<k8s-repo>/clusters//programs/release.yaml`, which file-system-
///     normalizes to `<k8s-repo>/clusters/programs/release.yaml` and
///     either silently overwrote a sibling-dir file or surfaced a
///     non-existent-path error far from the source flag.
///   - `--cluster "rio/.."` — the path-traversal footgun. The path
///     join landed at `<k8s-repo>/clusters/rio/../programs/release.yaml`,
///     which file-system-normalizes to
///     `<k8s-repo>/clusters/programs/release.yaml` and overwrote the
///     sibling location with no diagnostic naming the escape.
///   - `--cluster "Rio"` / `--cluster "rio_cluster"` /
///     `--cluster "rio.cluster"` — the wrong-case / wrong-separator /
///     wrong-shape footguns the K8s apiserver refuses at admission
///     time on every `metadata.name` consumer downstream (the
///     `lareira-fleet-programs` HelmRelease's per-cluster
///     `name:`-keyed lookup, the per-Aplicacao Gateway/HTTPRoute's
///     `name:` field). Silent passage through the verb arg-entry
///     surfaced the failure at `kubectl apply` time as a "field is
///     invalid" rejection, far from the source `--cluster` flag.
///
/// After the lift every per-verb `--cluster` consumer routes through
/// this helper: the DNS-1123 label gate fires at the verb's entry-
/// point, the diagnostic carries the offending `--cluster <value>`
/// verbatim plus a parser-shaped reason naming the specific violation,
/// and the canonical "what shape must `--cluster` carry?" decision
/// lives at exactly one call-site. Mirrors the per-slot
/// [`caixa_core::AplicacaoError::PlacementClusterInvalid`] discipline
/// on the typed-Caixa author surface — the same DNS-1123 label shape
/// both the typed `:placement :clusters` slot and the per-verb
/// `--cluster` arg refuse drift against, sharing the lifted
/// [`caixa_core::is_dns_1123_label`] predicate so the substrate-wide
/// "valid cluster name" set is single-sourced.
pub(crate) fn validate_cluster_arg(cluster: &str) -> Result<()> {
    // Empty is gated first with a self-locating diagnostic — the
    // canonical "I forgot the flag value" footgun a bare
    // `--cluster ""` shape produces. Peer with the
    // [`caixa_core::AplicacaoError::PlacementClusterEmpty`] gate on
    // the typed-slot axis (the empty-first cascade every author-
    // surface DNS-1123 label slot follows on `:membros :caixa`,
    // `:placement :clusters`, `:children :caixa`, `:contratos :de` /
    // `:para`, `:entrada :para`).
    if cluster.is_empty() {
        bail!(
            "--cluster value is empty (every `feira` verb that joins \
             `<k8s-repo>/clusters/<cluster>/…` requires a non-empty \
             cluster name — e.g. `--cluster rio`, `--cluster mar`, \
             `--cluster plo`)"
        );
    }
    caixa_core::is_dns_1123_label(cluster).map_err(|reason| {
        anyhow::anyhow!(
            "--cluster {cluster:?} is not a valid DNS-1123 label: {reason} \
             (every cluster name lands as a path segment in \
             `<k8s-repo>/clusters/<cluster>/…` and as a K8s `metadata.name` \
             on downstream HelmRelease / Gateway / HTTPRoute consumers; the \
             apiserver rejects any non-DNS-1123-label name at admission time)"
        )
    })
}

/// Validate a `feira` verb's `--namespace <name>` flag value at the
/// per-verb entry-point against the canonical DNS-1123 label shape every
/// K8s Namespace `metadata.name` consumer demands.
///
/// Closes the PRIME DIRECTIVE duplication (THEORY.md §I.3.5 — "the
/// duplication budget is zero") on the per-verb `--namespace` axis,
/// peer with the [`validate_cluster_arg`] lift (feb3103) on the
/// sibling per-verb `--cluster` axis. Before this lift the same
/// `--namespace` value passed through fourteen unprotected
/// [`kube::Api::namespaced(client, &self.namespace)`] / per-CR
/// `metadata.namespace = Some(self.namespace)` call-sites across the
/// three cluster-touching verb families — `feira pool …`
/// (`PoolListArgs::run` / `PoolScaleArgs::run` / `PoolDeleteArgs::run` /
/// `PoolStatusArgs::run`, `cmd/pool.rs`), `feira ephemeral …`
/// (`GraphArgs::run` / `PlanArgs::run` / `UpArgs::run` /
/// `DownArgs::run` / `StatusArgs::run` / `ListArgs::run`,
/// `cmd/ephemeral.rs`), `feira allocation …` (`RequestArgs::run` /
/// `ReleaseArgs::run` / `ListArgs::run` / `StatusArgs::run`,
/// `cmd/allocation.rs`) — with no shape gate at all. Fourteen
/// unprotected call-sites of the same load-bearing shape, each one
/// another place a future change to the canonical namespace-name
/// discipline has to remember to touch — the future `feira pool
/// drain --namespace <ns>` per-pool reaper, the future `feira
/// allocation request --namespace <ns>` per-PR allocation verb the
/// absorption-roadmap acknowledges, the future `feira ephemeral
/// gc --namespace <ns>` cluster-wide ephemeral reaper — each one
/// landing on a per-verb open-coded gate is a coordinated rewrite of
/// every verb-entry-point that either gets it right at every site or
/// quietly drifts at one.
///
/// Without the gate three authoring footguns silently passed:
///
///   - `--namespace ""` — the canonical "I forgot the flag value"
///     footgun (the bare `--namespace=` flag on a CI invocation with
///     an unset shell variable). The K8s apiserver rejects empty
///     namespace values at admission time with a "field is required"
///     diagnostic far from the source flag; lifting the gate to the
///     verb entry-point surfaces the empty-value axis verbatim
///     ahead of the kube-rs round-trip.
///   - `--namespace "MyTeam"` / `--namespace "team_a"` /
///     `--namespace "team.a"` — the wrong-case / wrong-separator /
///     wrong-shape footguns the K8s apiserver refuses at admission
///     time on every namespaced `metadata.namespace` consumer
///     downstream (every EphemeralPool / EphemeralAllocation /
///     Process CR's apiserver-side admission rule). Silent passage
///     through the verb arg-entry surfaced the failure at kube-rs
///     RTT as a "Namespace MyTeam is invalid: metadata.namespace:
///     Invalid value" rejection, far from the source `--namespace`
///     flag.
///   - `--namespace "default/.."` — the path-traversal-shaped
///     footgun. The kube-rs Api builder URL-joins the namespace into
///     the API path (`/api/v1/namespaces/<ns>/…`); the bare `..`
///     segment either reaches the cluster-scope `/api/v1/<resource>`
///     accidentally on URL-normalize or fails kube-rs's per-request
///     `ApiResource` builder with no diagnostic naming the escape.
///     The DNS-1123 label gate refuses `/` and `.` outright with the
///     offending value named verbatim.
///
/// After the lift every per-verb `--namespace` consumer routes
/// through this helper: the DNS-1123 label gate fires at the verb's
/// entry-point, the diagnostic carries the offending `--namespace
/// <value>` verbatim plus a parser-shaped reason naming the specific
/// violation, and the canonical "what shape must `--namespace`
/// carry?" decision lives at exactly one call-site. Mirrors
/// [`validate_cluster_arg`] on the peer per-verb-arg axis — together
/// the two helpers form the canonical DNS-1123-label CLI-arg surface
/// every per-verb `metadata.name` / `metadata.namespace` entry-point
/// composes from, sharing the lifted [`caixa_core::is_dns_1123_label`]
/// predicate so the substrate-wide "valid K8s label-shaped CLI arg"
/// set is single-sourced across the typed-slot and CLI-arg surfaces.
///
/// The `"default"` literal — the canonical fallback every
/// `--namespace`-taking verb's clap `default_value = "default"`
/// attribute carries — passes the DNS-1123 label gate cleanly (six
/// lowercase letters), so a verb invoked without the flag inherits
/// the validated canonical default through this helper without
/// special-casing the absent-flag arm.
pub(crate) fn validate_namespace_arg(namespace: &str) -> Result<()> {
    // Empty is gated first with a self-locating diagnostic — the
    // canonical "I forgot the flag value" footgun a bare
    // `--namespace=` shape produces (a CI invocation with an unset
    // shell variable interpolated into the flag). Peer with the
    // [`validate_cluster_arg`] empty-arm on the sibling per-verb
    // axis.
    if namespace.is_empty() {
        bail!(
            "--namespace value is empty (every `feira` verb that scopes a \
             `kube::Api` to a namespaced resource requires a non-empty \
             namespace name — e.g. `--namespace default`, `--namespace \
             pleme-system`, or omit the flag to inherit the canonical \
             `\"default\"` fallback)"
        );
    }
    caixa_core::is_dns_1123_label(namespace).map_err(|reason| {
        anyhow::anyhow!(
            "--namespace {namespace:?} is not a valid DNS-1123 label: {reason} \
             (every K8s Namespace `metadata.name` is bounded by the DNS-1123 \
             label grammar; the apiserver rejects any non-DNS-1123-label name \
             at admission time on every namespaced CR `metadata.namespace` \
             axis downstream — EphemeralPool, EphemeralAllocation, Process)"
        )
    })
}

/// Validate a `feira` verb's positional `<nome>` argument value at the
/// per-verb entry-point against the canonical DNS-1123 label shape every
/// downstream consumer (the typed [`caixa_core::Caixa::nome`] /
/// [`caixa_core::Dep::nome`] axes, the rendered
/// `lareira-<nome>` Helm chart name, every K8s `metadata.name` materializer
/// the substrate emits) demands.
///
/// Closes the PRIME DIRECTIVE duplication (THEORY.md §I.3.5 — "the
/// duplication budget is zero") on the per-verb `<nome>` positional axis,
/// peer with the [`validate_cluster_arg`] / [`validate_namespace_arg`]
/// lifts on the sibling `--cluster` / `--namespace` flag axes. Before
/// this lift the same `<nome>` value passed through both
/// [`super::init::Init::run`] (the per-caixa scaffold's
/// `<nome>/lib/<nome>.lisp` path join) and [`super::add::Add::run`] (the
/// per-dep `:nome` slot landing in the rendered `caixa.lisp`) with no
/// shape gate at all — every footgun the author surface's `:nome` /
/// `:deps :nome` typed slots already refuse through
/// [`caixa_core::ManifestError::NomeInvalid`] (the 6c992f8 lift on the
/// top-level `:nome` typed axis) and
/// [`caixa_core::DepError::NomeInvalid`] (the 2420c44-trajectory lift on
/// the per-dep `:nome` axis) silently passed at the verb arg-entry. Two
/// unprotected call-sites of the same load-bearing shape, each one
/// another place a future change to the canonical caixa-name discipline
/// has to remember to touch — a future `feira fork <nome>` verb that
/// scaffolds a derivative caixa, a future `feira rename <nome>` verb
/// that rewrites the manifest's `:nome` slot, a future `feira oci pull
/// <nome>` verb that fetches a caixa by name from the future OCI
/// registry — each one landing on a per-verb open-coded gate is a
/// coordinated rewrite of every verb-entry-point that either gets it
/// right at every site or quietly drifts at one.
///
/// Without the gate four authoring footguns silently passed:
///
///   - `feira init ""` / `feira add ""` — the canonical "I forgot the
///     positional arg" footgun (a CI invocation with an unset shell
///     variable interpolated into the positional). On `feira init` the
///     bare empty `<nome>` joined to `./` as the target dir (the current
///     working directory) and to `./lib/.lisp` as the lisp entry file;
///     the scaffolded `caixa.lisp` carried `:nome ""` which the
///     downstream `caixa_core::Caixa::validate_nome` arm rejected at
///     `feira build` time, far from the source `feira init` invocation,
///     with the bad-shape diagnostic naming `:nome` rather than the
///     `<nome>` positional arg the author typed. On `feira add` the
///     bare empty `<nome>` landed verbatim as `:deps :nome ""` and the
///     same downstream cascade fired — both verbs surface the failure
///     after the manifest has already been mutated on disk, rather than
///     before any I/O is observable.
///   - `feira init "../escape"` / `feira init "lib/../escape"` — the
///     canonical path-traversal footgun. On `feira init` the bare
///     `<nome>` arg is used verbatim as the target directory
///     (`PathBuf::from(&nome)`) and as the lisp filename inside the
///     scaffolded `lib/<nome>.lisp` path; the bare `..` segment either
///     creates a sibling directory the operator didn't intend, or
///     scaffolds a `lib/../escape.lisp` whose `caixa.lisp` references a
///     library file at the parent of `lib/` — escaping the target dir
///     before any other I/O is observable. The DNS-1123 label gate
///     refuses `/`, `.`, and the bare `..` outright with the offending
///     value named verbatim, closing the canonical "I pasted a relative
///     path into the `<nome>` slot" footgun ahead of any directory
///     creation.
///   - `feira init "MyCaixa"` / `feira add "Caixa-Teia"` — the
///     wrong-case footgun (the canonical "I copied the README header"
///     typo). The K8s `metadata.name` axes downstream
///     (`lareira-<nome>` Chart.yaml `name:`, the `HelmRelease`
///     `release_name`, every per-Servico / per-Aplicacao CR
///     `metadata.name`) all enforce the DNS-1123-label rule at admission
///     time on a lowercase floor; silent passage through the verb
///     arg-entry surfaced the failure at `feira chart` /
///     `feira deploy` / `kubectl apply` time as a "field is invalid"
///     rejection, far from the source positional arg. The lifted
///     [`caixa_core::is_dns_1123_label`] predicate carries a
///     canonical-lowercase remediation hint in its reason wording ("use
///     {lower:?}") so the diagnostic names the canonical fix verbatim.
///   - `feira init "caixa_teia"` / `feira add "my_dep"` — the
///     wrong-separator footgun (the Python / Go module-name leak — both
///     use underscores; K8s `metadata.name` consumers accept only
///     hyphens). The lifted predicate's reason wording names `-` as the
///     canonical separator so the diagnostic surfaces the fix verbatim.
///
/// After the lift every per-verb `<nome>` positional consumer routes
/// through this helper: the DNS-1123 label gate fires at the verb's
/// entry-point before any directory creation / manifest mutation, the
/// diagnostic carries the offending `<nome>` verbatim plus a parser-
/// shaped reason naming the specific violation, and the canonical "what
/// shape must `<nome>` carry?" decision lives at exactly one call-site.
/// Mirrors the per-slot
/// [`caixa_core::ManifestError::NomeInvalid`] /
/// [`caixa_core::DepError::NomeInvalid`] discipline on the typed-Caixa
/// author surface — the same DNS-1123 label shape both the typed
/// `:nome` / `:deps :nome` slots and the per-verb `<nome>` positional
/// arg refuse drift against, sharing the lifted
/// [`caixa_core::is_dns_1123_label`] predicate so the substrate-wide
/// "valid caixa name" set is single-sourced across the typed-slot and
/// CLI-arg surfaces.
pub(crate) fn validate_nome_arg(nome: &str) -> Result<()> {
    // Empty is gated first with a self-locating diagnostic — the
    // canonical "I forgot the positional arg" footgun a bare
    // `feira init ""` / `feira add ""` shape produces (a CI invocation
    // with an unset shell variable interpolated into the positional).
    // Peer with the [`validate_cluster_arg`] / [`validate_namespace_arg`]
    // empty-arms on the sibling per-verb flag axes and with the
    // [`caixa_core::ManifestError::NomeEmpty`] /
    // [`caixa_core::DepError::NomeEmpty`] arms on the typed-slot axes.
    if nome.is_empty() {
        bail!(
            "<nome> value is empty (every `feira` verb that consumes a positional \
             caixa-name argument requires a non-empty DNS-1123 label — e.g. \
             `feira init hello-rio`, `feira add caixa-teia`)"
        );
    }
    caixa_core::is_dns_1123_label(nome).map_err(|reason| {
        anyhow::anyhow!(
            "<nome> {nome:?} is not a valid DNS-1123 label: {reason} \
             (a caixa's `:nome` lands as the `lareira-<nome>` K8s `metadata.name` \
             on every downstream chart/release/CR consumer; the apiserver rejects \
             any non-DNS-1123-label name at admission time, and `feira init` joins \
             the value verbatim into the `<nome>/lib/<nome>.lisp` scaffold path so \
             a path-traversal shape escapes the target dir before any other I/O is \
             observable)"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use caixa_core::CaixaKind;
    use tempfile::tempdir;

    /// Assert every value in `bads` triggers `validator` to reject with
    /// a diagnostic that surfaces both `arg_hint` (the offending
    /// per-verb flag or positional — `"--cluster"`, `"--namespace"`,
    /// `"<nome>"`) and the value verbatim.
    ///
    /// Lifts the six-site per-`validate_<arg>_arg` reject-sweep shape
    /// on the per-verb arg-entry axis (PRIME DIRECTIVE — theory/
    /// THEORY.md §I.5, "the duplication budget is zero") into a single
    /// canonical helper. Before this lift the `--cluster` /
    /// `--namespace` / `<nome>` path-traversal and boundary-hyphen
    /// arms each open-coded the same for-loop + `match … { Err(e) =>
    /// e, Ok(()) => panic!(...) }` + `format!("{err:?}")` + two
    /// `contains(...)` asserts — six sites, one drift budget per
    /// future edit to the diagnostic-shape contract every
    /// `validate_<arg>_arg` gate carries (the "diagnostic must name
    /// the offending flag/positional verbatim" pin and the
    /// "diagnostic must name the offending value verbatim" pin, both
    /// load-bearing so the operator can grep their shell history for
    /// the bad flag ahead of the K8s apiserver's admission-time
    /// rejection). A future per-verb arg-entry validator (the M4
    /// `feira reconcile --to <cluster>` target-cluster arg the
    /// absorption-roadmap acknowledges, any future DNS-1123-shaped
    /// per-verb positional) inherits the canonical reject-sweep shape
    /// by construction — one helper call, not another six-line copy.
    fn assert_arg_rejects_each(
        validator: impl Fn(&str) -> Result<()>,
        arg_hint: &str,
        scenario: &str,
        bads: &[&str],
    ) {
        for bad in bads {
            let Err(err) = validator(bad) else {
                panic!("{scenario} {bad:?} must reject");
            };
            let rendered = format!("{err:?}");
            assert!(
                rendered.contains(arg_hint),
                "diagnostic must name {arg_hint:?} for {bad:?} (got: {rendered:?})"
            );
            assert!(
                rendered.contains(bad),
                "diagnostic must name the offending value verbatim for {bad:?} (got: {rendered:?})"
            );
        }
    }

    #[test]
    fn caixa_manifest_path_resolves_root_join_canonical_filename() {
        // The shape every per-verb call-site relied on (verbatim
        // `root.join("caixa.lisp")`) is now a single canonical
        // resolver. Pins that the path lands at `<root>/caixa.lisp`
        // and that the canonical manifest-filename constant stays
        // load-bearing for any future `feira` verb whose entry-point
        // joins the manifest by path.
        let root = PathBuf::from("/tmp/some-caixa");
        let p = caixa_manifest_path(&root);
        assert!(p.starts_with(&root), "manifest path must live under root");
        assert!(
            p.ends_with(CAIXA_MANIFEST_FILENAME),
            "manifest path must end with the canonical filename"
        );
        assert_eq!(p.file_name().and_then(|s| s.to_str()), Some("caixa.lisp"));
    }

    #[test]
    fn load_caixa_accepts_well_formed_manifest() {
        // The canonical happy-path — a well-formed `caixa.lisp` at
        // `<root>/caixa.lisp` loads cleanly and the typed Caixa
        // round-trips its `:nome` + `:kind` + `:versao` universal-axis
        // slots. Peer with every `feira` verb's `Run::run` entry-point:
        // each one used to open-code this four-line load before the
        // lift, and now routes through the canonical helper. Each of
        // the three round-trip assertions projects the parsed manifest
        // through the substrate-canonical [`Caixa::nome`] /
        // [`Caixa::kind`] / [`Caixa::versao`] accessors — the same
        // typed-dispatch discipline every peer per-verb call-site
        // reads the loaded manifest through (see e.g.
        // `cmd/build.rs`:63-65, `cmd/nix.rs`, `cmd/publish.rs`,
        // `cmd/deploy.rs`). Sibling of the peer caixa-flux
        // `sample_caixa_nome_accessor_byte_equals_raw_field` pin
        // (2ffdb44) and the caixa-crd `round_trip_preserves_core_fields`
        // accessor convergence (1a160cd) on the sibling test-fixture-
        // navigation surfaces.
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join(CAIXA_MANIFEST_FILENAME),
            r#"(defcaixa
                 :nome "hello"
                 :kind Biblioteca
                 :versao "0.1.0"
                 :bibliotecas ())"#,
        )
        .expect("write manifest");
        let caixa = load_caixa(dir.path()).expect("well-formed manifest must load");
        assert_eq!(caixa.nome(), "hello");
        assert_eq!(caixa.kind(), CaixaKind::Biblioteca);
        assert_eq!(caixa.versao(), "0.1.0");
    }

    #[test]
    fn load_caixa_scalar_accessors_byte_equal_raw_fields() {
        // The byte-parity pin the just-landed accessor route the
        // sibling `load_caixa_accepts_well_formed_manifest` case reads
        // the parsed manifest's `:nome` / `:versao` universal-axis
        // scalars through relies on. A future implementation of
        // [`Caixa::nome`] / [`Caixa::versao`] that returned a
        // differently-bytewidth projection (a cached `CaixaName`-newtype
        // `Display` value, an operator-side normalized ASCII-lowered
        // projection the caixa-operator reconciles ahead of dispatch)
        // would silently split the test-fixture-navigation input from
        // the storage-side field the peer `feira` verb-entry emit paths
        // (`cmd/deploy.rs`, `cmd/publish.rs`, `cmd/nix.rs`) still read
        // verbatim; this pin surfaces the drift at build-time rather
        // than at a downstream `kubectl get helmrelease` / annotated-
        // tag audit on the fleet. Same byte-parity discipline the peer
        // caixa-crd `round_trip_preserves_core_fields` accessor
        // convergence (1a160cd) added to lock its own per-`Caixa`
        // scalar-accessor family, and the caixa-flux
        // `sample_caixa_nome_accessor_byte_equals_raw_field` pin
        // (2ffdb44) added on the sibling `sample_caixa()` fixture-
        // navigation axis, extended here onto the caixa-feira
        // per-verb `load_caixa` entry-point's parsed-manifest surface.
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join(CAIXA_MANIFEST_FILENAME),
            r#"(defcaixa
                 :nome "hello"
                 :kind Biblioteca
                 :versao "0.1.0"
                 :bibliotecas ())"#,
        )
        .expect("write manifest");
        let caixa = load_caixa(dir.path()).expect("well-formed manifest must load");
        assert_eq!(
            caixa.nome(),
            caixa.nome.as_str(),
            "Caixa::nome accessor must project the underlying :nome storage \
             byte-for-byte — a future implementation that returned a \
             differently-bytewidth projection would silently split the \
             load_caixa_accepts_well_formed_manifest fixture-navigation \
             input from the storage-side field the peer per-verb emit \
             paths still read verbatim"
        );
        assert_eq!(
            caixa.versao(),
            caixa.versao.as_str(),
            "Caixa::versao accessor must project the underlying :versao \
             storage byte-for-byte — the peer arm on the top-level \
             `:versao` universal-axis SemVer-2 scalar surface"
        );
        assert_eq!(
            caixa.kind(),
            caixa.kind,
            "Caixa::kind accessor must return the underlying :kind storage \
             discriminant identically — the peer arm on the top-level \
             `:kind` universal-axis typed-discriminant surface"
        );
    }

    #[test]
    fn load_caixa_missing_manifest_diagnostic_names_path_with_reading_context() {
        // The diagnostic shape every per-verb consumer pins: the
        // `.with_context(|| format!("reading {}", manifest.display()))`
        // wrap renders the verbatim `"reading <path>"` string with the
        // resolved `caixa.lisp` path on missing-file failures. Before
        // this lift the same string appeared verbatim at every verb's
        // call-site — a future refactor of the helper can't silently
        // regress to a different context-string shape without this
        // pin firing.
        let dir = tempdir().expect("tempdir");
        let err = load_caixa(dir.path()).expect_err("missing manifest must error");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("reading"),
            "diagnostic must name the reading axis (got: {rendered:?})"
        );
        assert!(
            rendered.contains("caixa.lisp"),
            "diagnostic must name the canonical manifest filename (got: {rendered:?})"
        );
    }

    #[test]
    fn load_caixa_parse_error_diagnostic_names_path_with_parsing_context() {
        // The peer arm on the parse-failure axis: a malformed manifest
        // surfaces a diagnostic whose `.with_context` wrap names the
        // `"parsing <path>"` axis verbatim. Pins the canonical
        // context-string discipline on the parse-failure path so a
        // future refactor can't silently regress it independent of
        // the read-failure path.
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join(CAIXA_MANIFEST_FILENAME),
            "this is not a defcaixa form at all",
        )
        .expect("write malformed manifest");
        let err = load_caixa(dir.path()).expect_err("malformed manifest must error");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("parsing"),
            "diagnostic must name the parsing axis (got: {rendered:?})"
        );
        assert!(
            rendered.contains("caixa.lisp"),
            "diagnostic must name the canonical manifest filename (got: {rendered:?})"
        );
    }

    #[test]
    fn load_yaml_accepts_well_formed_document() {
        // The canonical happy-path — a well-formed YAML document loads
        // cleanly and the parsed [`serde_yaml::Value`] round-trips its
        // canonical top-level scalar. Peer with every per-verb
        // YAML-consuming entry-point: the per-Servico computeunit
        // loader (`super::chart::load_first_servico_yaml`) and the
        // per-cluster fleet-programs HelmRelease loader (`feira
        // deploy`'s `Run::run`) used to open-code the same four-line
        // read+parse shape before this lift, and now route through
        // this canonical helper.
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("hello.yaml");
        std::fs::write(&p, "name: hello\nport: 8080\n").expect("write yaml");
        let value = load_yaml(&p).expect("well-formed yaml must load");
        assert_eq!(
            value.get("name").and_then(serde_yaml::Value::as_str),
            Some("hello"),
            "parsed yaml must round-trip the canonical top-level scalar"
        );
    }

    #[test]
    fn load_yaml_missing_file_diagnostic_names_path_with_reading_context() {
        // The diagnostic shape every per-verb consumer pins: the
        // `.with_context(|| format!("reading {}", path.display()))`
        // wrap renders the verbatim `"reading <path>"` string with
        // the resolved path on missing-file failures. Before this
        // lift the same string appeared verbatim at every per-verb
        // YAML call-site — a future refactor of the helper can't
        // silently regress to a different context-string shape
        // without this pin firing. Peer with the
        // `load_caixa_missing_manifest_diagnostic_names_path_with_
        // reading_context` pin on the sibling manifest-IO axis.
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("absent.yaml");
        let err = load_yaml(&p).expect_err("missing yaml must error");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("reading"),
            "diagnostic must name the reading axis (got: {rendered:?})"
        );
        assert!(
            rendered.contains("absent.yaml"),
            "diagnostic must name the offending path (got: {rendered:?})"
        );
    }

    #[test]
    fn load_yaml_parse_error_diagnostic_names_path_with_parsing_context() {
        // The peer arm on the parse-failure axis: a malformed YAML
        // surfaces a diagnostic whose `.with_context` wrap names the
        // `"parsing <path>"` axis verbatim. Pins the canonical
        // context-string discipline on the parse-failure path so a
        // future refactor can't silently regress it independent of
        // the read-failure path. Peer with the
        // `load_caixa_parse_error_diagnostic_names_path_with_parsing_
        // context` pin on the sibling manifest-IO axis.
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("torn.yaml");
        std::fs::write(&p, "name: torn\n  bad-indent: [unclosed\n").expect("write malformed yaml");
        let err = load_yaml(&p).expect_err("malformed yaml must error");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("parsing"),
            "diagnostic must name the parsing axis (got: {rendered:?})"
        );
        assert!(
            rendered.contains("torn.yaml"),
            "diagnostic must name the offending path (got: {rendered:?})"
        );
    }

    #[test]
    fn load_yaml_parse_error_preserves_underlying_serde_yaml_error_on_chain() {
        // Peer to the parse-context pin above: the anyhow
        // `.with_context(...)` wrap preserves the underlying
        // `serde_yaml::Error` on the cause chain — a future
        // structured-output mode (`feira chart --json`, the future
        // `feira validate --strict` per-caixa admission verb, the M4
        // `feira reconcile --json` cluster-diff verb) can downcast
        // through the chain and read the typed payload directly, peer
        // with the per-renderer [`caixa_core::KindMismatch`] /
        // [`caixa_core::ServicoCountMismatch`] typed-view downcast
        // discipline and with the
        // `load_caixa_parse_error_preserves_underlying_lisp_error_on_
        // chain` pin on the sibling manifest-IO axis.
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("torn.yaml");
        std::fs::write(&p, "name: torn\n  bad-indent: [unclosed\n").expect("write malformed yaml");
        let err = load_yaml(&p).expect_err("malformed yaml must error");
        let typed_reachable = err
            .chain()
            .any(|e| e.downcast_ref::<serde_yaml::Error>().is_some());
        assert!(
            typed_reachable,
            "underlying serde_yaml::Error must remain reachable on the anyhow chain"
        );
    }

    #[test]
    fn caixa_root_with_none_resolves_to_canonical_cwd_dirname() {
        // The canonical fallback arm — an absent `--path` flag
        // resolves to the canonical CWD-default dirname literal
        // ([`CAIXA_ROOT_DEFAULT_DIRNAME`]). Pins the load-bearing
        // fallback shape every per-verb call-site relied on before
        // this lift (`self.path.clone().unwrap_or_else(||
        // PathBuf::from("."))`); a future refactor of the helper
        // can't silently regress to a different fallback (an
        // absolute-path default, a CWD-via-env-var default, a
        // panic-on-absent) without this pin firing.
        let p = caixa_root(None);
        assert_eq!(
            p,
            PathBuf::from(CAIXA_ROOT_DEFAULT_DIRNAME),
            "absent --path must fall back to the canonical CWD-default dirname literal"
        );
        assert_eq!(
            p.as_os_str(),
            std::ffi::OsStr::new("."),
            "the canonical CWD-default dirname literal must remain `.` verbatim"
        );
    }

    #[test]
    fn caixa_root_with_some_returns_caller_path_clone() {
        // The peer arm — when `--path` is present the resolver
        // returns a clone of the borrowed [`Path`], not the
        // canonical fallback. Pins that a caller-provided
        // `--path /some/absolute/caixa` (the common CI shape) round-
        // trips through the helper unchanged — peer with the
        // `:nome` round-trip pin on `load_caixa_accepts_well_formed_
        // manifest`.
        let caller = PathBuf::from("/tmp/some/explicit/caixa-root");
        let resolved = caixa_root(Some(&caller));
        assert_eq!(
            resolved, caller,
            "explicit --path must pass through the resolver unchanged"
        );
    }

    #[test]
    fn caixa_root_with_some_relative_path_preserves_shape() {
        // Defensive coverage: relative `--path` shapes (the common
        // shell-invocation form, e.g. `feira chart --path
        // ./subcaixa`) round-trip the resolver verbatim — a future
        // refactor can't silently canonicalize or absolutize the
        // path without this pin firing. The canonical layout
        // discipline (`StandardLayout::verify`) joins this root with
        // `:bibliotecas` / `:exe` / `:servicos` entries downstream,
        // so any silent canonicalization would break the
        // per-caixa-root layout invariant pins peer-with
        // `caixa_manifest_path_resolves_root_join_canonical_filename`
        // on the manifest axis.
        let rel = PathBuf::from("./subcaixa");
        let resolved = caixa_root(Some(&rel));
        assert_eq!(
            resolved, rel,
            "relative --path must pass through the resolver verbatim, no canonicalize"
        );
    }

    #[test]
    fn caixa_root_canonical_dirname_constant_pins_dot_literal() {
        // The canonical-constant arm — pins
        // [`CAIXA_ROOT_DEFAULT_DIRNAME`] at the verbatim `"."`
        // literal. Peer with the
        // [`CAIXA_MANIFEST_FILENAME`]-pins-`"caixa.lisp"` discipline
        // on the manifest-filename axis: a future refactor of the
        // helper can't silently regress the canonical fallback
        // dirname to a different literal (`""`, `"./"`,
        // `std::env::current_dir().unwrap()`) independent of the
        // per-verb call-sites that already route through it.
        assert_eq!(
            CAIXA_ROOT_DEFAULT_DIRNAME, ".",
            "canonical CWD-default dirname literal must remain `.` verbatim"
        );
    }

    #[test]
    fn validate_cluster_arg_accepts_canonical_dns_1123_label_names() {
        // The canonical happy-path arm — the three canonical pleme-io
        // cluster names (`rio`, `mar`, `plo`) the `feira deploy` /
        // `feira app deploy` help text already documents pass the lifted
        // DNS-1123 label gate cleanly. Peer with every per-slot
        // DNS-1123-label call-site's happy-path pin
        // (`validate_placement_cluster` on the typed `:placement
        // :clusters` slot, `validate_membro_caixa` on `:membros :caixa`,
        // `validate_child_caixa` on `:children :caixa`).
        for cluster in ["rio", "mar", "plo", "us-east-1", "eu-west-2", "a", "a0"] {
            validate_cluster_arg(cluster)
                .unwrap_or_else(|_| panic!("canonical cluster name {cluster:?} must accept"));
        }
    }

    #[test]
    fn validate_cluster_arg_rejects_empty_with_self_locating_diagnostic() {
        // The canonical "I forgot the flag value" footgun a bare
        // `--cluster ""` shape produces — silently passing the empty
        // string would join to `<k8s-repo>/clusters//programs/release.yaml`,
        // which file-system-normalizes to a sibling-dir path. The gate
        // refuses with a self-locating diagnostic naming the `--cluster`
        // flag and the canonical cluster-name shape, peer with the
        // [`caixa_core::AplicacaoError::PlacementClusterEmpty`] arm on
        // the typed-slot axis.
        let err = validate_cluster_arg("").expect_err("empty --cluster must reject");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("--cluster"),
            "diagnostic must name the offending flag (got: {rendered:?})"
        );
        assert!(
            rendered.contains("empty"),
            "diagnostic must name the empty-value axis (got: {rendered:?})"
        );
    }

    #[test]
    fn validate_cluster_arg_rejects_path_traversal_with_named_value() {
        // The path-traversal footgun: `--cluster "rio/.."` joined at
        // `<k8s-repo>/clusters/rio/../programs/release.yaml` normalizes
        // to a sibling-dir overwrite with no diagnostic naming the
        // escape. The DNS-1123 label gate refuses `/` and `.` outright,
        // so the parent-escape attempt surfaces as a
        // "contains `/` (DNS-1123 labels allow only `[a-z0-9-]`)" or
        // "contains `.` (a single DNS-1123 label is not a subdomain)"
        // shape — peer with the `validate_placement_cluster` rejection
        // on the typed-slot axis. The diagnostic names the offending
        // `--cluster` value verbatim so the operator can grep their
        // shell history for the bad flag.
        assert_arg_rejects_each(
            validate_cluster_arg,
            "--cluster",
            "path-traversal",
            &["rio/..", "../rio", "rio/sub", "."],
        );
    }

    #[test]
    fn validate_cluster_arg_rejects_uppercase_with_named_value_and_canonical_lowercase_hint() {
        // The wrong-case footgun: `--cluster "Rio"` lands as a K8s
        // `metadata.name` on the downstream `lareira-fleet-programs`
        // HelmRelease's per-cluster `name:`-keyed lookup, which the
        // apiserver rejects at admission time with a "field is invalid"
        // diagnostic far from the source flag. The lifted
        // `is_dns_1123_label` predicate carries a canonical-lowercase
        // hint in its reason wording ("use {lower:?}") so the
        // diagnostic names the canonical fix verbatim — peer with the
        // `validate_placement_cluster` rejection on the typed-slot
        // axis. Pins both the offending value and the canonical
        // lowercase remediation in the rendered diagnostic.
        let err = validate_cluster_arg("Rio").expect_err("uppercase --cluster must reject");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("--cluster"),
            "diagnostic must name the offending flag (got: {rendered:?})"
        );
        assert!(
            rendered.contains("Rio"),
            "diagnostic must name the offending value verbatim (got: {rendered:?})"
        );
        assert!(
            rendered.contains("rio"),
            "diagnostic must surface the canonical lowercase remediation (got: {rendered:?})"
        );
    }

    #[test]
    fn validate_cluster_arg_rejects_underscore_with_named_value_and_hyphen_hint() {
        // The wrong-separator footgun: `--cluster "rio_cluster"` is a
        // common author error (the K8s `metadata.name` axis allows
        // hyphen-separated tokens, not snake_case). The lifted
        // `is_dns_1123_label` predicate's reason wording names `-` as
        // the canonical separator ("use `-` instead") so the
        // diagnostic surfaces the fix verbatim — peer with the
        // `validate_placement_cluster` rejection on the typed-slot
        // axis.
        let err =
            validate_cluster_arg("rio_cluster").expect_err("underscore --cluster must reject");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("--cluster"),
            "diagnostic must name the offending flag (got: {rendered:?})"
        );
        assert!(
            rendered.contains("rio_cluster"),
            "diagnostic must name the offending value verbatim (got: {rendered:?})"
        );
        assert!(
            rendered.contains('`'),
            "diagnostic must surface the hyphen-separator remediation (got: {rendered:?})"
        );
    }

    #[test]
    fn validate_cluster_arg_rejects_leading_hyphen_with_named_value() {
        // The boundary-char footgun: `--cluster "-rio"` violates the
        // DNS-1123 label rule that names must start and end with an
        // alphanumeric (the K8s apiserver rejects leading / trailing
        // `-` outright at admission time). The lifted predicate's
        // reason wording names the boundary rule verbatim — peer with
        // the `validate_placement_cluster` rejection on the typed-slot
        // axis.
        assert_arg_rejects_each(
            validate_cluster_arg,
            "--cluster",
            "boundary-`-`",
            &["-rio", "rio-"],
        );
    }

    #[test]
    fn validate_namespace_arg_accepts_canonical_dns_1123_label_names() {
        // The canonical happy-path arm — the K8s-canonical
        // `"default"` namespace literal every `--namespace`-taking
        // verb's `default_value = "default"` clap attribute carries
        // passes the lifted DNS-1123 label gate cleanly, alongside
        // the realistic operator-side shapes (`kube-system`,
        // `pleme-system`, `cert-manager`, `argocd`) and DNS-1123 edge
        // shapes (`a`, `a0`). Peer with the
        // [`validate_cluster_arg`] happy-path pin on the sibling
        // per-verb axis — the same DNS-1123 label discipline both
        // helpers gate against, single-sourced through the lifted
        // [`caixa_core::is_dns_1123_label`] predicate.
        for namespace in [
            "default",
            "kube-system",
            "pleme-system",
            "cert-manager",
            "argocd",
            "a",
            "a0",
        ] {
            validate_namespace_arg(namespace)
                .unwrap_or_else(|_| panic!("canonical namespace {namespace:?} must accept"));
        }
    }

    #[test]
    fn validate_namespace_arg_rejects_empty_with_self_locating_diagnostic() {
        // The canonical "I forgot the flag value" footgun a bare
        // `--namespace=` shape produces (a CI invocation with an
        // unset shell variable interpolated into the flag). Silently
        // passing the empty string would surface at the kube-rs API
        // round-trip as a "Namespace is required" rejection far
        // from the source flag. The gate refuses with a self-
        // locating diagnostic naming the `--namespace` flag and the
        // empty-value axis, peer with the
        // [`validate_cluster_arg`] empty-arm.
        let err = validate_namespace_arg("").expect_err("empty --namespace must reject");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("--namespace"),
            "diagnostic must name the offending flag (got: {rendered:?})"
        );
        assert!(
            rendered.contains("empty"),
            "diagnostic must name the empty-value axis (got: {rendered:?})"
        );
    }

    #[test]
    fn validate_namespace_arg_rejects_path_traversal_with_named_value() {
        // The path-traversal-shaped footgun: `--namespace
        // "default/.."` URL-joined into the kube-rs API path
        // (`/api/v1/namespaces/default/../<resource>`) either
        // reaches the cluster-scope `/api/v1/<resource>` on
        // URL-normalize or surfaces a per-request `ApiResource`
        // builder error with no diagnostic naming the escape. The
        // DNS-1123 label gate refuses `/` and `.` outright, so the
        // parent-escape attempt surfaces with the offending value
        // named verbatim — peer with the [`validate_cluster_arg`]
        // path-traversal arm on the sibling per-verb axis.
        assert_arg_rejects_each(
            validate_namespace_arg,
            "--namespace",
            "path-traversal",
            &["default/..", "../default", "default/sub", "."],
        );
    }

    #[test]
    fn validate_namespace_arg_rejects_uppercase_with_named_value_and_canonical_lowercase_hint() {
        // The wrong-case footgun: `--namespace "MyTeam"` lands as
        // the K8s `metadata.namespace` axis on every namespaced CR
        // (EphemeralPool / EphemeralAllocation / Process) the kube-rs
        // round-trip targets, which the apiserver rejects at
        // admission time with a "Namespace MyTeam is invalid" /
        // "metadata.namespace: Invalid value" diagnostic far from
        // the source flag. The lifted `is_dns_1123_label` predicate
        // carries a canonical-lowercase hint in its reason wording
        // ("use {lower:?}") so the diagnostic names the canonical
        // fix verbatim — peer with the
        // [`validate_cluster_arg`] uppercase arm.
        let err = validate_namespace_arg("MyTeam").expect_err("uppercase --namespace must reject");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("--namespace"),
            "diagnostic must name the offending flag (got: {rendered:?})"
        );
        assert!(
            rendered.contains("MyTeam"),
            "diagnostic must name the offending value verbatim (got: {rendered:?})"
        );
        assert!(
            rendered.contains("myteam"),
            "diagnostic must surface the canonical lowercase remediation (got: {rendered:?})"
        );
    }

    #[test]
    fn validate_namespace_arg_rejects_underscore_with_named_value_and_hyphen_hint() {
        // The wrong-separator footgun: `--namespace "team_a"` is a
        // common author error (the K8s `metadata.namespace` axis
        // allows hyphen-separated tokens, not snake_case). The lifted
        // `is_dns_1123_label` predicate's reason wording names `-` as
        // the canonical separator ("use `-` instead") so the
        // diagnostic surfaces the fix verbatim — peer with the
        // [`validate_cluster_arg`] underscore arm.
        let err = validate_namespace_arg("team_a").expect_err("underscore --namespace must reject");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("--namespace"),
            "diagnostic must name the offending flag (got: {rendered:?})"
        );
        assert!(
            rendered.contains("team_a"),
            "diagnostic must name the offending value verbatim (got: {rendered:?})"
        );
        assert!(
            rendered.contains('`'),
            "diagnostic must surface the hyphen-separator remediation (got: {rendered:?})"
        );
    }

    #[test]
    fn validate_namespace_arg_rejects_leading_hyphen_with_named_value() {
        // The boundary-char footgun: `--namespace "-default"`
        // violates the DNS-1123 label rule that names must start and
        // end with an alphanumeric (the K8s apiserver rejects
        // leading / trailing `-` outright at admission time). The
        // lifted predicate's reason wording names the boundary rule
        // verbatim — peer with the [`validate_cluster_arg`]
        // boundary-hyphen arm.
        assert_arg_rejects_each(
            validate_namespace_arg,
            "--namespace",
            "boundary-`-`",
            &["-default", "default-"],
        );
    }

    #[test]
    fn validate_namespace_arg_accepts_canonical_default_fallback_literal() {
        // Pins that the canonical fallback literal every
        // `--namespace`-taking verb's clap `default_value =
        // "default"` attribute carries (six lowercase letters)
        // passes the gate cleanly. A future refactor of either the
        // clap default or the gate that drifts the two out of
        // alignment — say, lowercasing the gate's reject set or
        // renaming the canonical fallback — surfaces as a test
        // regression here, ahead of every per-verb's first
        // namespace-less invocation. Peer with the [`load_caixa`]
        // parse-context pin on the sibling per-verb IO axis.
        validate_namespace_arg("default").expect("canonical default literal must accept");
    }

    #[test]
    fn validate_nome_arg_accepts_canonical_dns_1123_label_names() {
        // The canonical happy-path arm — the in-tree fixture
        // names (`hello-rio`, `caixa-teia`, `iac-forge-ir`),
        // every example caixa name (`checkout`, `cart`,
        // `catalog`, `payment`), and the DNS-1123 edge shapes
        // (`a`, `a0`) all pass the lifted gate cleanly. Peer
        // with [`validate_cluster_arg`] /
        // [`validate_namespace_arg`] happy-path pins on the
        // sibling per-verb axes — the same DNS-1123 label
        // discipline all three helpers gate against,
        // single-sourced through the lifted
        // [`caixa_core::is_dns_1123_label`] predicate.
        for nome in [
            "hello-rio",
            "caixa-teia",
            "iac-forge-ir",
            "checkout",
            "cart",
            "catalog",
            "payment",
            "a",
            "a0",
        ] {
            validate_nome_arg(nome)
                .unwrap_or_else(|_| panic!("canonical caixa name {nome:?} must accept"));
        }
    }

    #[test]
    fn validate_nome_arg_rejects_empty_with_self_locating_diagnostic() {
        // The canonical "I forgot the positional arg" footgun a
        // bare `feira init ""` / `feira add ""` shape produces
        // (a CI invocation with an unset shell variable
        // interpolated into the positional). Silently passing
        // the empty string would scaffold `./` as the target
        // dir and `./lib/.lisp` as the lisp entry on
        // `feira init`, and would land `:deps :nome ""` in the
        // manifest on `feira add` — both surface their failure
        // far from the source positional. The gate refuses
        // with a self-locating diagnostic naming the `<nome>`
        // positional and the empty-value axis, peer with the
        // [`validate_cluster_arg`] / [`validate_namespace_arg`]
        // empty-arms on the sibling per-verb flag axes.
        let err = validate_nome_arg("").expect_err("empty <nome> must reject");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("<nome>"),
            "diagnostic must name the offending positional (got: {rendered:?})"
        );
        assert!(
            rendered.contains("empty"),
            "diagnostic must name the empty-value axis (got: {rendered:?})"
        );
    }

    #[test]
    fn validate_nome_arg_rejects_path_traversal_with_named_value() {
        // The canonical path-traversal footgun: `feira init
        // "../escape"` would use the bare `<nome>` arg verbatim
        // as the target dir (`PathBuf::from(&nome)`) and as
        // the lisp filename inside `lib/<nome>.lisp` — the
        // bare `..` segment escapes the target dir before any
        // other I/O is observable. The DNS-1123 label gate
        // refuses `/`, `.`, and the bare `..` outright, so the
        // parent-escape attempt surfaces with the offending
        // value named verbatim — peer with the
        // [`validate_cluster_arg`] / [`validate_namespace_arg`]
        // path-traversal arms on the sibling per-verb axes.
        assert_arg_rejects_each(
            validate_nome_arg,
            "<nome>",
            "path-traversal",
            &["../escape", "lib/../escape", "foo/bar", ".", ".."],
        );
    }

    #[test]
    fn validate_nome_arg_rejects_uppercase_with_named_value_and_canonical_lowercase_hint() {
        // The wrong-case footgun: `feira init "MyCaixa"` /
        // `feira add "Caixa-Teia"` (the canonical "I copied
        // the README header" typo). The K8s `metadata.name`
        // axes downstream (`lareira-<nome>` Chart.yaml `name:`,
        // every per-Servico / per-Aplicacao CR `metadata.name`)
        // all enforce the DNS-1123-label rule at admission
        // time on a lowercase floor; silent passage through
        // the verb arg-entry surfaced the failure at
        // `feira chart` / `feira deploy` / `kubectl apply`
        // time as a "field is invalid" rejection, far from
        // the source positional. The lifted
        // [`caixa_core::is_dns_1123_label`] predicate carries
        // a canonical-lowercase remediation hint in its reason
        // wording — peer with the
        // [`validate_cluster_arg`] uppercase arm.
        let err = validate_nome_arg("MyCaixa").expect_err("uppercase <nome> must reject");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("<nome>"),
            "diagnostic must name the offending positional (got: {rendered:?})"
        );
        assert!(
            rendered.contains("MyCaixa"),
            "diagnostic must name the offending value verbatim (got: {rendered:?})"
        );
        assert!(
            rendered.contains("mycaixa"),
            "diagnostic must surface the canonical lowercase remediation (got: {rendered:?})"
        );
    }

    #[test]
    fn validate_nome_arg_rejects_underscore_with_named_value_and_hyphen_hint() {
        // The wrong-separator footgun: `feira init
        // "caixa_teia"` / `feira add "my_dep"` (the Python /
        // Go module-name leak — both use underscores; K8s
        // `metadata.name` consumers accept only hyphens). The
        // lifted predicate's reason wording names `-` as the
        // canonical separator so the diagnostic surfaces the
        // fix verbatim — peer with the [`validate_cluster_arg`]
        // underscore arm.
        let err = validate_nome_arg("caixa_teia").expect_err("underscore <nome> must reject");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("<nome>"),
            "diagnostic must name the offending positional (got: {rendered:?})"
        );
        assert!(
            rendered.contains("caixa_teia"),
            "diagnostic must name the offending value verbatim (got: {rendered:?})"
        );
        assert!(
            rendered.contains('`'),
            "diagnostic must surface the hyphen-separator remediation (got: {rendered:?})"
        );
    }

    #[test]
    fn validate_nome_arg_rejects_leading_hyphen_with_named_value() {
        // The boundary-char footgun: `feira init "-hello"`
        // violates the DNS-1123 label rule that names must
        // start and end with an alphanumeric (the K8s
        // apiserver rejects leading / trailing `-` outright
        // at admission time, and on `feira init` a leading
        // `-` is structurally indistinguishable from a clap
        // flag prefix without `--` separator discipline). The
        // lifted predicate's reason wording names the boundary
        // rule verbatim — peer with the
        // [`validate_cluster_arg`] / [`validate_namespace_arg`]
        // boundary-hyphen arms.
        assert_arg_rejects_each(
            validate_nome_arg,
            "<nome>",
            "boundary-`-`",
            &["-hello", "hello-"],
        );
    }

    #[test]
    fn validate_nome_arg_rejects_space_with_named_value() {
        // The paste-from-doc footgun: `feira init "my caixa"`
        // (an embedded space from a paste-from-aligned-doc).
        // The K8s `metadata.name` axes enforce the DNS-1123
        // label rule that names contain only `[a-z0-9-]`; the
        // embedded space surfaces at the apiserver-side
        // admission rule as a "field is invalid" rejection
        // far from the source positional. The lifted gate
        // refuses the embedded space with the offending value
        // named verbatim.
        let err = validate_nome_arg("my caixa").expect_err("space <nome> must reject");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("<nome>"),
            "diagnostic must name the offending positional (got: {rendered:?})"
        );
        assert!(
            rendered.contains("my caixa"),
            "diagnostic must name the offending value verbatim (got: {rendered:?})"
        );
    }

    #[test]
    fn load_caixa_parse_error_preserves_underlying_lisp_error_on_chain() {
        // Peer to the parse-context pin above: the anyhow
        // `.with_context(...)` wrap preserves the underlying
        // `tatara_lisp::LispError` on the cause chain — a future
        // structured-output mode (`feira app graph --json`, the M4
        // `feira reconcile --json` diff verb) can downcast through
        // the chain and read the typed payload directly, peer with
        // the per-renderer `KindMismatch` / `ServicoCountMismatch`
        // typed-view downcast discipline.
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join(CAIXA_MANIFEST_FILENAME), "(((((")
            .expect("write malformed manifest");
        let err = load_caixa(dir.path()).expect_err("malformed manifest must error");
        let typed_reachable = err
            .chain()
            .any(|e| e.downcast_ref::<tatara_lisp::LispError>().is_some());
        assert!(
            typed_reachable,
            "underlying tatara_lisp::LispError must remain reachable on the anyhow chain"
        );
    }
}
