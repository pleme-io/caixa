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

use caixa_core::{Caixa, CaixaKind};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors caixa-flux can raise.
#[derive(Debug, Error)]
pub enum Error {
    #[error("caixa :kind must be Servico for caixa-flux rendering, got {0:?}")]
    NotAServico(CaixaKind),
    #[error("caixa :servicos must declare exactly one entry for V0 (got {0})")]
    UnsupportedServicoCount(usize),
    #[error("computeunit yaml missing required field: {0}")]
    MissingField(&'static str),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

/// Default cluster-wide namespace for caixa Servicos when the
/// computeunit doesn't pin its own.
pub const DEFAULT_NAMESPACE: &str = "tatara-system";

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
    if caixa.kind != CaixaKind::Servico {
        return Err(Error::NotAServico(caixa.kind));
    }
    if caixa.servicos.len() != 1 {
        return Err(Error::UnsupportedServicoCount(caixa.servicos.len()));
    }

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
    if let Some(limits) = &caixa.limits {
        if !limits.is_empty() {
            entry
                .entry(serde_yaml::Value::String("limits".into()))
                .or_insert_with(|| {
                    serde_yaml::to_value(limits).unwrap_or(serde_yaml::Value::Null)
                });
        }
    }
    if let Some(behavior) = &caixa.behavior {
        if !behavior.is_empty() {
            entry
                .entry(serde_yaml::Value::String("behavior".into()))
                .or_insert_with(|| {
                    serde_yaml::to_value(behavior).unwrap_or(serde_yaml::Value::Null)
                });
        }
    }
    if !caixa.upgrade_from.is_empty() {
        entry
            .entry(serde_yaml::Value::String("upgradeFrom".into()))
            .or_insert_with(|| {
                serde_yaml::to_value(&caixa.upgrade_from).unwrap_or(serde_yaml::Value::Null)
            });
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
        return Err(Error::MissingField("expected mapping at root of HelmRelease"));
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
        _ => return Err(Error::MissingField("spec.values.programs must be a sequence")),
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
        return Err(Error::MissingField("expected mapping at root of values.yaml"));
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
            git_ref: GitRefSpec::Tag(format!("v{}", caixa.versao)),
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
pub fn cluster_bundle(caixa: &Caixa, opts: &ClusterBundleOpts) -> Result<Vec<BundleFile>, Error> {
    if caixa.kind != CaixaKind::Servico {
        return Err(Error::NotAServico(caixa.kind));
    }

    let name = caixa.nome.clone();
    let chart_name = format!("lareira-{name}");

    let gitref_field = match &opts.git_ref {
        GitRefSpec::Tag(t) => format!("    tag: {t:?}"),
        GitRefSpec::Branch(b) => format!("    branch: {b:?}"),
        GitRefSpec::Commit(c) => format!("    commit: {c:?}"),
    };

    let gitrepo = format!(
        "---\n\
         # Source — pinned to {tag_human}, rendered by caixa-flux.\n\
         apiVersion: source.toolkit.fluxcd.io/v1\n\
         kind: GitRepository\n\
         metadata:\n  \
           name: {name}\n  \
           namespace: {namespace}\n\
         spec:\n  \
           interval: {interval}\n  \
           url: {url}\n  \
           ref:\n\
         {gitref_field}\n",
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

    let helmrelease = format!(
        "---\n\
         # HelmRelease consumes the chart caixa-helm renders for this\n\
         # caixa Servico. Per-cluster values are injected here.\n\
         apiVersion: helm.toolkit.fluxcd.io/v2\n\
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
             pleme-computeunit:\n      \
               enabled: true\n",
        name = name,
        namespace = opts.namespace,
        interval = opts.interval,
        chart_path = opts.chart_path,
    );

    let kustomization = format!(
        "---\n\
         # Flux Kustomization that pins the GitRepository + HelmRelease.\n\
         # Paired path: pleme-io/k8s/clusters/{cluster}/services/{name}/\n\
         apiVersion: kustomize.toolkit.fluxcd.io/v1\n\
         kind: Kustomization\n\
         metadata:\n  \
           name: {name}\n  \
           namespace: flux-system\n\
         spec:\n  \
           interval: {interval}\n  \
           prune: true\n  \
           sourceRef:\n    \
             kind: GitRepository\n    \
             name: flux-system\n  \
           path: ./clusters/{cluster}/services/{name}\n  \
           healthChecks:\n    \
             - apiVersion: helm.toolkit.fluxcd.io/v2\n      \
               kind: HelmRelease\n      \
               name: {name}\n      \
               namespace: {namespace}\n  \
           timeout: 5m\n",
        name = name,
        namespace = opts.namespace,
        interval = opts.interval,
        cluster = opts.cluster,
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
    fn programs_yaml_entry_round_trips() {
        let entry = programs_yaml_entry(&sample_caixa(), &sample_cu_yaml()).unwrap();
        assert_eq!(entry.get("name").and_then(|n| n.as_str()), Some("hello-rio"));
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
        assert_eq!(entry.get("namespace").and_then(|n| n.as_str()), Some(DEFAULT_NAMESPACE));
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
        assert_eq!(arr[0].get("name").and_then(|n| n.as_str()), Some("hello-rio"));
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
        let updated_module = arr[0].get("module").unwrap().get("source").and_then(|s| s.as_str());
        assert_eq!(updated_module, Some("oci://ghcr.io/pleme-io/hello-rio:v0.1.0"));
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
            .get("spec").unwrap()
            .get("values").unwrap()
            .get("programs").unwrap()
            .as_sequence().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[1].get("name").and_then(|n| n.as_str()), Some("hello-rio"));
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
            .get("spec").unwrap()
            .get("values").unwrap()
            .get("programs").unwrap()
            .as_sequence().unwrap();
        assert_eq!(arr.len(), 2);
        let updated = arr[0].get("module").unwrap().get("source").and_then(|s| s.as_str());
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
        let names: Vec<_> = files.iter().map(|f| f.path.to_string_lossy().to_string()).collect();
        assert!(names.contains(&"gitrepository.yaml".to_string()));
        assert!(names.contains(&"helmrelease.yaml".to_string()));
        assert!(names.contains(&"kustomization.yaml".to_string()));

        let kust = files.iter().find(|f| f.path == std::path::PathBuf::from("kustomization.yaml")).unwrap();
        assert!(kust.contents.contains("./clusters/rio/services/hello-rio"));

        let gitrepo = files.iter().find(|f| f.path == std::path::PathBuf::from("gitrepository.yaml")).unwrap();
        assert!(gitrepo.contents.contains("v0.1.0"));
    }
}
