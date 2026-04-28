use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use caixa_core::Caixa;
use clap::Args;

/// Render the per-program lareira-<name> Helm chart for the caixa Servico
/// in CWD.
///
/// Reads:
///   ./caixa.lisp
///   ./servicos/<name>.computeunit.yaml   (first servicos[] entry)
///
/// Writes:
///   <out>/lareira-<name>/Chart.yaml
///   <out>/lareira-<name>/values.yaml
///   <out>/lareira-<name>/README.md
///
/// The chart is "thin" by design — it depends on the
/// `pleme-computeunit` library chart in helmworks, which owns the K8s
/// templates. caixa-helm only wires the metadata + values block.
#[derive(Args)]
pub struct Chart {
    /// Where to write the chart directory. Default: `./.caixa/chart`.
    #[arg(long, default_value = ".caixa/chart")]
    pub out: PathBuf,

    /// caixa root (defaults to CWD).
    #[arg(long)]
    pub path: Option<PathBuf>,
}

impl Chart {
    pub fn run(self) -> Result<()> {
        let root = self.path.clone().unwrap_or_else(|| PathBuf::from("."));
        let manifest_path = root.join("caixa.lisp");
        let src = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        let caixa = Caixa::from_lisp(&src)
            .with_context(|| format!("parsing {}", manifest_path.display()))?;

        let cu_path = first_servico_path(&caixa, &root)?;
        let cu_src = std::fs::read_to_string(&cu_path)
            .with_context(|| format!("reading {}", cu_path.display()))?;
        let cu_yaml: serde_yaml::Value = serde_yaml::from_str(&cu_src)
            .with_context(|| format!("parsing {}", cu_path.display()))?;

        let dir = caixa_helm::render_chart_for_servico(&caixa, &cu_yaml)?;
        dir.write_to(&self.out)
            .with_context(|| format!("writing chart to {}", self.out.display()))?;

        eprintln!(
            "rendered {} → {}",
            dir.name,
            self.out.join(&dir.name).display()
        );
        Ok(())
    }
}

pub(crate) fn first_servico_path(caixa: &Caixa, root: &std::path::Path) -> Result<PathBuf> {
    let s = caixa
        .servicos
        .first()
        .ok_or_else(|| anyhow::anyhow!("caixa.lisp has no :servicos entry"))?;
    let p = root.join(s);
    if !p.exists() {
        bail!("declared servicos entry not found: {}", p.display());
    }
    Ok(p)
}
