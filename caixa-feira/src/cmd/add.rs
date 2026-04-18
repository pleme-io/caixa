use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use caixa_core::{Caixa, Dep, DepSource};
use clap::Args;

/// Add a dep to the caixa.lisp in CWD (or `--path`).
///
/// Shape mirrors `cargo add`: name + semver, optional git source pin. Writes
/// back through `Caixa::to_lisp`, so the manifest round-trips.
#[derive(Args)]
pub struct Add {
    /// Caixa name to add.
    pub nome: String,

    /// Semver constraint (`^0.1`, `~0.1.2`, `0.1.0`, `*`). Defaults to `*`.
    #[arg(long, default_value = "*")]
    pub versao: String,

    /// Add under :deps-dev instead of :deps.
    #[arg(long)]
    pub dev: bool,

    /// Git source URL — e.g. `github:pleme-io/caixa-teia`. When set, the dep's
    /// `:fonte` becomes a git source; otherwise defaults to the feira registry.
    #[arg(long)]
    pub git: Option<String>,

    /// Tag pin for `--git`.
    #[arg(long)]
    pub tag: Option<String>,

    /// Revision pin for `--git`.
    #[arg(long)]
    pub rev: Option<String>,

    /// Branch pin for `--git`.
    #[arg(long)]
    pub branch: Option<String>,

    /// Feature flags to enable on the target caixa.
    #[arg(long = "caracteristica", value_name = "NAME")]
    pub caracteristicas: Vec<String>,

    /// caixa root (defaults to CWD).
    #[arg(long)]
    pub path: Option<PathBuf>,
}

impl Add {
    pub fn run(self) -> Result<()> {
        let root = self.path.clone().unwrap_or_else(|| PathBuf::from("."));
        let manifest_path = root.join("caixa.lisp");
        let src = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        let mut caixa = Caixa::from_lisp(&src)
            .with_context(|| format!("parsing {}", manifest_path.display()))?;

        let fonte = self.git.as_ref().map(|repo| DepSource::Git {
            repo: repo.clone(),
            tag: self.tag.clone(),
            rev: self.rev.clone(),
            branch: self.branch.clone(),
        });

        let dep = Dep {
            nome: self.nome.clone(),
            versao: self.versao.clone(),
            fonte,
            opcional: false,
            caracteristicas: self.caracteristicas.clone(),
        };

        let target = if self.dev {
            &mut caixa.deps_dev
        } else {
            &mut caixa.deps
        };
        if target.iter().any(|d| d.nome == self.nome) {
            bail!("dep '{}' already declared", self.nome);
        }
        target.push(dep);

        let emitted = caixa.to_lisp();
        std::fs::write(&manifest_path, &emitted)
            .with_context(|| format!("writing {}", manifest_path.display()))?;

        let section = if self.dev { "deps-dev" } else { "deps" };
        eprintln!(
            "added {} {} to :{} in {}",
            self.nome,
            self.versao,
            section,
            manifest_path.display()
        );
        Ok(())
    }
}
