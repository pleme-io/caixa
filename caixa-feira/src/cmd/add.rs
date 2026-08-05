use std::path::PathBuf;

use anyhow::{Context, Result};
use caixa_core::{Dep, DepList, DepSource};
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

        // Route the per-list dispatch + within-list dedup + push
        // cascade through the substrate primitive [`Caixa::push_dep`]
        // rather than the prior open-coded `&mut caixa.deps` / `&mut
        // caixa.deps_dev` inline field-access + open-coded
        // `.iter().any(|d| d.nome == …)` dup-check + open-coded
        // `bail!("dep '{}' already declared", …)` string-diagnostic
        // path — the two-arm [`DepList`] enum + the typed
        // [`caixa_core::DepError::DuplicateNome`] carrier are the
        // substrate's canonical closed-set-typed carrier for the
        // "runtime-closure `:deps` vs dev-only-closure `:deps-dev`" +
        // "within-list duplicate `:nome` refusal" axis pair every
        // dep-list consumer already routes through. Same substrate-
        // primitive-owns-the-resolver discipline the sibling
        // [`Caixa::deps`] / [`Caixa::deps_dev`] read accessors carry
        // on the peer per-list read axis — extended onto the
        // outer-`Caixa` typed-mutation axis, the substrate's first
        // typed-mutation dispatch on the top-level manifest surface.
        let list = if self.dev {
            DepList::Dev
        } else {
            DepList::Prod
        };
        caixa.push_dep(list, dep)?;

        let emitted = caixa.to_lisp();
        std::fs::write(&manifest_path, &emitted)
            .with_context(|| format!("writing {}", manifest_path.display()))?;

        eprintln!(
            "added {} {} to {} in {}",
            self.nome,
            self.versao,
            list,
            manifest_path.display()
        );
        Ok(())
    }
}
