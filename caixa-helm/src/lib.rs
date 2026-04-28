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

use caixa_core::{Caixa, CaixaKind};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors caixa-helm can raise.
#[derive(Debug, Error)]
pub enum Error {
    #[error("caixa :kind must be Servico for caixa-helm rendering, got {0:?}")]
    NotAServico(CaixaKind),
    #[error("caixa :servicos must declare exactly one entry for V0 (got {0})")]
    UnsupportedServicoCount(usize),
    #[error("computeunit yaml missing required field: {0}")]
    MissingField(&'static str),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
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
pub const DEFAULT_LIBRARY_NAME: &str = "pleme-computeunit";

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
    if caixa.kind != CaixaKind::Servico {
        return Err(Error::NotAServico(caixa.kind));
    }
    if caixa.servicos.len() != 1 {
        return Err(Error::UnsupportedServicoCount(caixa.servicos.len()));
    }

    let chart_name = format!("lareira-{}", caixa.nome);
    let chart_yaml = build_chart_yaml(caixa, &chart_name, opts);
    let values_yaml = build_values_yaml(caixa, computeunit_yaml, opts)?;
    let readme = build_readme(caixa, &chart_name);

    Ok(ChartDir {
        name: chart_name,
        files: vec![
            ChartFile {
                path: PathBuf::from("Chart.yaml"),
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
        api_version: "v2".into(),
        name: chart_name.into(),
        description,
        chart_type: "application".into(),
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
    // The `pleme-computeunit` library chart expects the values to live
    // under the `pleme-computeunit:` key (alias-friendly Helm pattern).
    // We emit `enabled: <opts.enabled_default>` plus the entire `spec:`
    // block from the ComputeUnit YAML.
    let spec = computeunit_yaml
        .get("spec")
        .ok_or(Error::MissingField("spec"))?
        .clone();

    // Prepend a comment header so the file is human-friendly.
    let header = format!(
        "# Auto-generated by caixa-helm from caixa.lisp + servicos/{}.computeunit.yaml.\n\
         # Edits to this file are overwritten by `feira chart`.\n\
         #\n\
         # `pleme-computeunit:` is the alias under which the library chart\n\
         # in pleme-io/helmworks/charts/pleme-computeunit consumes its values.\n\n",
        caixa.nome
    );

    let mut block = BTreeMap::new();
    block.insert("enabled".to_string(), serde_yaml::Value::Bool(opts.enabled_default));
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
    // computeunit.yaml win over duplicates in caixa.lisp.
    if let Some(limits) = &caixa.limits {
        if !limits.is_empty() {
            block
                .entry("limits".to_string())
                .or_insert_with(|| serde_yaml::to_value(limits).unwrap_or(serde_yaml::Value::Null));
        }
    }
    if let Some(behavior) = &caixa.behavior {
        if !behavior.is_empty() {
            block.entry("behavior".to_string()).or_insert_with(|| {
                serde_yaml::to_value(behavior).unwrap_or(serde_yaml::Value::Null)
            });
        }
    }
    if !caixa.upgrade_from.is_empty() {
        block
            .entry("upgradeFrom".to_string())
            .or_insert_with(|| {
                serde_yaml::to_value(&caixa.upgrade_from).unwrap_or(serde_yaml::Value::Null)
            });
    }

    let mut wrapped = serde_yaml::Mapping::new();
    wrapped.insert(
        serde_yaml::Value::String("pleme-computeunit".into()),
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
        repo = caixa.repositorio.clone().unwrap_or_else(|| caixa.nome.clone()),
        versao = caixa.versao,
        license = caixa.licenca.clone().unwrap_or_else(|| "MIT".into()),
    )
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
        let names: Vec<_> = dir.files.iter().map(|f| f.path.to_string_lossy().to_string()).collect();
        assert!(names.contains(&"Chart.yaml".to_string()));
        assert!(names.contains(&"values.yaml".to_string()));
        assert!(names.contains(&"README.md".to_string()));
    }

    #[test]
    fn chart_yaml_metadata_propagates() {
        let dir = render_chart_for_servico(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let chart_file = dir.files.iter().find(|f| f.path == PathBuf::from("Chart.yaml")).unwrap();
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
        let values = dir.files.iter().find(|f| f.path == PathBuf::from("values.yaml")).unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&values.contents).unwrap();
        let cu_block = parsed.get("pleme-computeunit").expect("must wrap under pleme-computeunit");
        assert_eq!(cu_block.get("enabled"), Some(&serde_yaml::Value::Bool(false)));
        assert!(cu_block.get("module").is_some());
        assert!(cu_block.get("trigger").is_some());
        assert!(cu_block.get("capabilities").is_some());
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
        let limits = cu_block.get("limits").expect("limits must propagate");
        assert_eq!(limits.get("memory").and_then(|m| m.as_str()), Some("64MiB"));
        assert_eq!(limits.get("fuel").and_then(|m| m.as_u64()), Some(1_000_000));
        assert_eq!(limits.get("wallClock").and_then(|m| m.as_str()), Some("30s"));
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
        let behavior = cu_block.get("behavior").expect("behavior must propagate");
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
        assert!(cu_block.get("upgradeFrom").is_some());
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
        assert!(cu_block.get("limits").is_none());
        assert!(cu_block.get("behavior").is_none());
        assert!(cu_block.get("upgradeFrom").is_none());
    }

    #[test]
    fn write_to_creates_files() {
        let dir = render_chart_for_servico(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        dir.write_to(tmp.path()).unwrap();
        let chart_root = tmp.path().join("lareira-hello-rio");
        assert!(chart_root.join("Chart.yaml").exists());
        assert!(chart_root.join("values.yaml").exists());
        assert!(chart_root.join("README.md").exists());
    }
}
