use std::path::PathBuf;

use anyhow::{Context, Result};
use caixa_core::Caixa;
use caixa_lacre::{Lacre, LacreEntry, closure_hash, hash_bytes};
use clap::Args;

/// Resolve deps and write `lacre.lisp`.
///
/// **Phase 1 resolver**: every declared dep becomes a `LacreEntry` whose
/// `:conteudo` hash is taken over `"{nome}@{versao}"` and whose
/// `:fechamento` is the closure hash (content + zero transitive deps).
/// No cloning, no transitive walk, no network — that lands in the phase 1.B
/// `feira resolve`, which replaces this stub.
#[derive(Args)]
pub struct Lock {
    /// caixa root (defaults to CWD).
    #[arg(long)]
    pub path: Option<PathBuf>,

    /// Don't actually write lacre.lisp — print the resolved content instead.
    #[arg(long)]
    pub dry_run: bool,
}

impl Lock {
    pub fn run(self) -> Result<()> {
        let root = self.path.clone().unwrap_or_else(|| PathBuf::from("."));
        let manifest_path = root.join("caixa.lisp");
        let src = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        let caixa = Caixa::from_lisp(&src)
            .with_context(|| format!("parsing {}", manifest_path.display()))?;

        let entries: Vec<LacreEntry> = caixa.deps.iter().map(resolve_stub).collect();

        let lacre = Lacre::from_entries(entries);
        let out = lacre.to_lisp();

        if self.dry_run {
            print!("{out}");
            return Ok(());
        }

        let lacre_path = root.join("lacre.lisp");
        std::fs::write(&lacre_path, &out)
            .with_context(|| format!("writing {}", lacre_path.display()))?;
        eprintln!(
            "locked {} dep(s); raiz = {}",
            lacre.entradas.len(),
            lacre.raiz
        );
        Ok(())
    }
}

/// Stub resolver — used when caixa-resolver isn't wired in. Defaults a
/// missing `:fonte` to `github:pleme-io/<nome>`, following the Zig-style
/// git-only store model.
fn resolve_stub(dep: &caixa_core::Dep) -> LacreEntry {
    let fonte = dep
        .fonte
        .clone()
        .unwrap_or_else(|| caixa_core::DepSource::default_github("pleme-io", &dep.nome));
    let conteudo = hash_bytes(format!("{}@{}", dep.nome, dep.versao).as_bytes());
    let fechamento = closure_hash(&conteudo, &[]);
    LacreEntry {
        nome: dep.nome.clone(),
        versao: dep.versao.clone(),
        fonte,
        conteudo,
        fechamento,
        deps_diretas: Vec::new(),
    }
}
