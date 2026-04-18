use std::path::PathBuf;

use anyhow::{Context, Result};
use caixa_core::Caixa;
use caixa_resolver::{CacheDir, ResolverConfig, resolve_lacre};
use clap::Args;

/// Resolve deps via git + write `lacre.lisp`.
///
/// This replaces `feira lock` in phase 1.B onwards. The older stub resolver
/// still lives under `feira lock` for testing without network access.
#[derive(Args)]
pub struct Resolve {
    /// caixa root (defaults to CWD).
    #[arg(long)]
    pub path: Option<PathBuf>,

    /// Include :deps-dev in the resolution graph.
    #[arg(long)]
    pub dev: bool,

    /// Don't write lacre.lisp — print the resolved content instead.
    #[arg(long)]
    pub dry_run: bool,

    /// Override the default org (e.g. `github:pleme-io`). Can also be set
    /// via `~/.config/caixa/config.yaml`.
    #[arg(long)]
    pub default_host: Option<String>,
}

impl Resolve {
    pub fn run(self) -> Result<()> {
        let root = self.path.clone().unwrap_or_else(|| PathBuf::from("."));
        let manifest_path = root.join("caixa.lisp");
        let src = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        let caixa = Caixa::from_lisp(&src)
            .with_context(|| format!("parsing {}", manifest_path.display()))?;

        let mut cfg = ResolverConfig::load_or_default();
        cfg.include_dev = self.dev;
        if let Some(h) = &self.default_host {
            cfg.default_host = h.clone();
        }
        let cache = CacheDir::discover().context("discovering cache dir")?;

        let lacre = resolve_lacre(&caixa, &cfg, &cache).context("resolution failed")?;
        let out = lacre.to_lisp();

        if self.dry_run {
            print!("{out}");
            return Ok(());
        }
        let lacre_path = root.join("lacre.lisp");
        std::fs::write(&lacre_path, &out)
            .with_context(|| format!("writing {}", lacre_path.display()))?;
        eprintln!(
            "resolved {} dep(s); raiz = {}",
            lacre.entradas.len(),
            lacre.raiz
        );
        Ok(())
    }
}
