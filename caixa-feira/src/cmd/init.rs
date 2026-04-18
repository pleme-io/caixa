use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use caixa_core::Caixa;
use clap::Args;

/// Scaffold a new caixa.
///
/// Creates:
///   - `./<nome>/caixa.lisp`       — generated from `Caixa::template`
///   - `./<nome>/lib/<nome>.lisp`  — empty library entry
///   - `./<nome>/.gitignore`
#[derive(Args)]
pub struct Init {
    /// The caixa's `:nome` (also the default directory name).
    pub nome: String,

    /// Scaffold into this path instead of `./<nome>`. Pass `.` to use CWD.
    #[arg(long)]
    pub path: Option<PathBuf>,
}

impl Init {
    pub fn run(self) -> Result<()> {
        let root = self
            .path
            .clone()
            .unwrap_or_else(|| PathBuf::from(&self.nome));

        if root.exists() && !is_empty_dir(&root)? {
            bail!("target path {} is not empty", root.display());
        }
        std::fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;

        let manifest_path = root.join("caixa.lisp");
        if manifest_path.exists() {
            bail!("{} already exists", manifest_path.display());
        }
        let manifest = Caixa::template(&self.nome);
        std::fs::write(&manifest_path, &manifest)
            .with_context(|| format!("writing {}", manifest_path.display()))?;

        let lib_dir = root.join("lib");
        std::fs::create_dir_all(&lib_dir)?;
        let lib_entry = lib_dir.join(format!("{}.lisp", self.nome));
        let lib_src = format!(
            ";; {nome} — library entry point.\n\
             ;;\n\
             ;; Declare your forms here. Anything imported via\n\
             ;; `(importar :caixa \"{nome}\")` starts at this file.\n",
            nome = self.nome
        );
        std::fs::write(&lib_entry, lib_src)?;

        let gi = root.join(".gitignore");
        if !gi.exists() {
            std::fs::write(&gi, "/target\n/result\n")?;
        }

        // Parse back as a sanity check that the template stays in sync with the
        // manifest schema — any schema drift surfaces here, not at build time.
        let parsed = Caixa::from_lisp(&manifest)
            .context("generated caixa.lisp failed to parse; template is out of sync")?;

        eprintln!(
            "initialized caixa {} v{} in {}",
            parsed.nome,
            parsed.versao,
            root.display()
        );
        Ok(())
    }
}

fn is_empty_dir(p: &Path) -> Result<bool> {
    Ok(p.read_dir()?.next().is_none())
}
