use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use caixa_core::{Dep, DepSource};
use clap::Args;

use super::load::{caixa_manifest_path, caixa_root, load_caixa};

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
        // Gate the positional `<nome>` against the canonical DNS-1123
        // label shape *before* any manifest mutation — the bare arg
        // is used verbatim as the new dep's `:nome` slot in the
        // rendered `caixa.lisp`. A wrong-case / wrong-separator /
        // path-traversal shape would silently land in the manifest
        // and the failure surfaces only at `feira build` /
        // `feira resolve` time as a downstream
        // [`caixa_core::DepError::NomeInvalid`] rejection, far from
        // the source `feira add` invocation, with the diagnostic
        // naming `:deps :nome` rather than the `<nome>` positional.
        // The lifted gate refuses every shape the typed-slot axis
        // already refuses, single-sourced through the lifted
        // [`caixa_core::is_dns_1123_label`] predicate.
        super::load::validate_nome_arg(&self.nome)?;
        let root = caixa_root(self.path.as_deref());
        let manifest_path = caixa_manifest_path(&root);
        let mut caixa = load_caixa(&root)?;

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
