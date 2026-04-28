use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use caixa_core::{Caixa, CaixaKind, WitTarget};
use clap::{Args, Subcommand};

/// `feira app …` — composition verbs for `:kind Aplicacao` caixas.
///
/// V0 ships two subcommands:
///
///   feira app graph              — print the typed Aplicacao spec
///                                  (membros + contratos + politicas
///                                  + placement + entrada) for review
///   feira app deploy --cluster X — render every cluster artifact
///                                  (programs.yaml entries, Cilium
///                                  NetworkPolicies, Gateway/HTTPRoute)
///                                  and write to the cluster's GitOps
///                                  tree (with optional commit + push)
///
/// Mirrors the `feira deploy` shape used for individual Servicos —
/// same flags (--cluster, --dry-run, --commit, --apply), same
/// PLEME_K8S_REPO env override, same default path layout.
#[derive(Args)]
pub struct App {
    #[command(subcommand)]
    pub command: AppCommand,
}

#[derive(Subcommand)]
pub enum AppCommand {
    /// Print the validated typed graph for the Aplicacao in CWD.
    /// Useful for code review + cse-lint integration.
    Graph(GraphArgs),

    /// Render every cluster artifact for the Aplicacao + write to the
    /// k8s GitOps repo. With --apply, also git commit + push.
    Deploy(DeployArgs),
}

impl App {
    pub fn run(self) -> Result<()> {
        match self.command {
            AppCommand::Graph(c) => c.run(),
            AppCommand::Deploy(c) => c.run(),
        }
    }
}

// ── feira app graph ────────────────────────────────────────────────

#[derive(Args)]
pub struct GraphArgs {
    /// Caixa root (defaults to CWD).
    #[arg(long)]
    pub path: Option<PathBuf>,

    /// Output as JSON instead of human-readable.
    #[arg(long)]
    pub json: bool,
}

impl GraphArgs {
    pub fn run(self) -> Result<()> {
        let caixa = load_aplicacao(self.path.as_deref())?;
        let spec = caixa_mesh::typed_view(&caixa)?;
        if self.json {
            println!("{}", serde_json::to_string_pretty(&spec)?);
        } else {
            println!("Aplicacao {} v{}", caixa.nome, caixa.versao);
            println!(
                "  placement: {:?} on clusters {:?}",
                spec.placement.estrategia, spec.placement.clusters
            );
            println!("  membros ({}):", spec.membros.len(),);
            for m in &spec.membros {
                println!("    - {} {}", m.caixa, m.versao);
            }
            println!("  contratos ({}):", spec.contratos.len());
            for c in &spec.contratos {
                // Typed view: each WIT shape has exactly one payload
                // field (validated upstream). The label tells the
                // reader *what* field they're looking at, not just
                // its value.
                let label = match c.target().expect("validated by typed_view") {
                    WitTarget::Http { endpoint } => format!("endpoint={endpoint}"),
                    WitTarget::PubSub { subject } => format!("subject={subject}"),
                    WitTarget::Store { slot } => format!("slot={slot}"),
                    WitTarget::Capability => "(capability-only)".to_string(),
                };
                println!("    - {} → {}  via {}  [{}]", c.de, c.para, c.wit, label);
            }
            if let Some(e) = &spec.entrada {
                println!(
                    "  entrada: {} → {} (paths={:?}, port={})",
                    e.host, e.para, e.paths, e.port
                );
            } else {
                println!("  entrada: (internal-only mesh)");
            }
        }
        Ok(())
    }
}

// ── feira app deploy ───────────────────────────────────────────────

#[derive(Args)]
pub struct DeployArgs {
    /// Cluster name (e.g. `rio`, `mar`, `plo`). Selects the k8s tree
    /// path: `<k8s-repo>/clusters/<cluster>/aplicacaos/<nome>/`.
    #[arg(long)]
    pub cluster: String,

    /// Path to the GitOps k8s repo. Defaults to PLEME_K8S_REPO env
    /// var or `~/code/github/pleme-io/k8s` if neither is set.
    #[arg(long, env = "PLEME_K8S_REPO")]
    pub k8s_repo: Option<PathBuf>,

    /// Print the rendered manifests to stdout instead of writing.
    #[arg(long)]
    pub dry_run: bool,

    /// Auto-commit the change after writing (no push).
    #[arg(long, conflicts_with = "apply")]
    pub commit: bool,

    /// Auto-commit AND push to origin (full automation).
    #[arg(long)]
    pub apply: bool,

    /// Caixa root (defaults to CWD).
    #[arg(long)]
    pub path: Option<PathBuf>,
}

impl DeployArgs {
    pub fn run(self) -> Result<()> {
        let caixa = load_aplicacao(self.path.as_deref())?;
        let docs = caixa_mesh::render_all(&caixa)?;

        let serialized = render_multidoc(&caixa.nome, &docs)?;

        if self.dry_run {
            print!("{serialized}");
            return Ok(());
        }

        let k8s_repo = self
            .k8s_repo
            .clone()
            .or_else(|| dirs::home_dir().map(|h| h.join("code/github/pleme-io/k8s")))
            .ok_or_else(|| anyhow::anyhow!("could not resolve k8s repo path"))?;

        let rel = PathBuf::from("clusters")
            .join(&self.cluster)
            .join("aplicacaos")
            .join(&caixa.nome)
            .join("manifests.yaml");
        let abs = k8s_repo.join(&rel);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(&abs, &serialized).with_context(|| format!("writing {}", abs.display()))?;

        eprintln!("rendered {} → {}", caixa.nome, abs.display());

        if self.commit || self.apply {
            commit_change(&k8s_repo, &rel, &caixa)?;
            eprintln!("committed change in {}", k8s_repo.display());
        }
        if self.apply {
            push_origin(&k8s_repo)?;
            eprintln!("pushed origin/main");
        } else if !self.commit {
            eprintln!(
                "review with: git -C {} diff -- {}",
                k8s_repo.display(),
                rel.display()
            );
        }
        Ok(())
    }
}

// ── helpers ─────────────────────────────────────────────────────────

fn load_aplicacao(path: Option<&std::path::Path>) -> Result<Caixa> {
    let root = path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let manifest = root.join("caixa.lisp");
    let src = std::fs::read_to_string(&manifest)
        .with_context(|| format!("reading {}", manifest.display()))?;
    let caixa =
        Caixa::from_lisp(&src).with_context(|| format!("parsing {}", manifest.display()))?;
    if caixa.kind != CaixaKind::Aplicacao {
        bail!(
            "feira app: caixa :kind must be Aplicacao for app verbs, got {:?}",
            caixa.kind
        );
    }
    Ok(caixa)
}

fn render_multidoc(nome: &str, docs: &[serde_yaml::Value]) -> Result<String> {
    let header = format!(
        "# Auto-generated by `feira app deploy` from caixa.lisp ({nome}).\n\
         # One file per Aplicacao; multi-doc YAML separated by `---`.\n\
         # Edits are overwritten on next `feira app deploy`.\n",
    );
    let mut out = String::from(&header);
    for d in docs {
        out.push_str("---\n");
        let s = serde_yaml::to_string(d)?;
        out.push_str(&s);
    }
    Ok(out)
}

fn commit_change(repo: &std::path::Path, rel: &std::path::Path, caixa: &Caixa) -> Result<()> {
    let msg = format!(
        "deploy: aplicacao {} v{}\n\nUpdated by `feira app deploy --cluster <name>`.\n",
        caixa.nome, caixa.versao
    );
    git(repo, ["add", &rel.display().to_string()])?;
    git(repo, ["commit", "-m", &msg])?;
    Ok(())
}

fn push_origin(repo: &std::path::Path) -> Result<()> {
    git(repo, ["push", "origin", "HEAD"])
}

fn git<'a, I: IntoIterator<Item = &'a str>>(cwd: &std::path::Path, args: I) -> Result<()> {
    use std::process::Command;
    let argv: Vec<&str> = args.into_iter().collect();
    let out = Command::new("git").current_dir(cwd).args(&argv).output()?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            argv.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}
