use std::path::PathBuf;

use anyhow::{Context, Result};
use caixa_core::{LayoutInvariants, StandardLayout};
use clap::Args;

use super::load::load_caixa;

/// Validate caixa.lisp + layout + every `:bibliotecas` entry parses.
///
/// This is the phase-1 analog of `cargo check` — no actual compilation, but
/// every declared library source is parsed through `tatara_lisp::read` so
/// lexical / structural errors surface before an `importar` from another
/// caixa. Real compilation (`tatara-lispc`) wires in next phase.
#[derive(Args)]
pub struct Build {
    /// caixa root (defaults to CWD).
    #[arg(long)]
    pub path: Option<PathBuf>,
}

impl Build {
    pub fn run(self) -> Result<()> {
        let root = self.path.clone().unwrap_or_else(|| PathBuf::from("."));
        let caixa = load_caixa(&root)?;

        StandardLayout::new()
            .verify(&caixa, &root)
            .context("layout invariants violated")?;

        for entry in &caixa.bibliotecas {
            let path = root.join(entry);
            let src = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            tatara_lisp::read(&src).with_context(|| format!("parsing {}", path.display()))?;
            eprintln!("  ✓ {entry}");
        }

        eprintln!(
            "caixa {} v{} — layout + lisp parse clean ({} lib entry/ies)",
            caixa.nome,
            caixa.versao,
            caixa.bibliotecas.len()
        );
        Ok(())
    }
}
