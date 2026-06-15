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
        // Gate the positional `<nome>` against the canonical DNS-1123
        // label shape *before* any directory creation / file write —
        // the bare arg is used verbatim as the target dir
        // (`PathBuf::from(&self.nome)`), as the lisp filename inside
        // `lib/<nome>.lisp`, and as the `:nome` slot in the scaffolded
        // manifest. A path-traversal shape (`"../escape"`,
        // `"lib/../escape"`) silently escapes the target dir at the
        // `create_dir_all` / `write` calls below; a wrong-case /
        // wrong-separator shape (`"MyCaixa"`, `"my_caixa"`) lands in
        // the manifest and surfaces at `feira build` time, far from
        // the source `feira init` invocation, with the diagnostic
        // naming `:nome` rather than the `<nome>` positional. The
        // lifted gate refuses every shape the typed-slot axes
        // ([`caixa_core::ManifestError::NomeInvalid`] on the top-level
        // `:nome`) already refuse, single-sourced through the lifted
        // [`caixa_core::is_dns_1123_label`] predicate.
        super::load::validate_nome_arg(&self.nome)?;
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
