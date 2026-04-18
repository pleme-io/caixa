use std::path::PathBuf;

use anyhow::{Context, Result};
use caixa_lint::{Severity, lint_source};
use caixa_theme::Theme;
use clap::Args;

/// Run caixa-lint — Ruby+Rust distilled best practices. Prints Nord-themed
/// diagnostics; exits non-zero if any error-level rule fires.
#[derive(Args)]
pub struct Lint {
    /// Paths to lint. Defaults to `./caixa.lisp` + every `.lisp` under `lib/`.
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Max severity to report (errors-only if true).
    #[arg(long)]
    pub errors_only: bool,

    /// Disable color even on a TTY.
    #[arg(long)]
    pub no_color: bool,
}

impl Lint {
    pub fn run(self) -> Result<()> {
        let targets = self.resolve_targets()?;
        let theme = Theme::blackmatter_dark();
        let mut error_count = 0usize;

        for path in &targets {
            let src = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            let mut diags =
                lint_source(&src).with_context(|| format!("linting {}", path.display()))?;
            if self.errors_only {
                diags.retain(|d| d.severity == Severity::Error);
            }
            for d in &diags {
                if d.severity == Severity::Error {
                    error_count += 1;
                }
                let rendered = if self.no_color {
                    let plain = Theme::blackmatter_light();
                    d.render(&src, &plain)
                } else {
                    d.render(&src, &theme)
                };
                eprintln!("{}: {rendered}", path.display());
            }
        }

        eprintln!(
            "caixa-lint: {} file(s) checked, {error_count} error(s)",
            targets.len()
        );
        if error_count > 0 {
            std::process::exit(1);
        }
        Ok(())
    }

    fn resolve_targets(&self) -> Result<Vec<PathBuf>> {
        if !self.paths.is_empty() {
            return Ok(self.paths.clone());
        }
        let mut out = Vec::new();
        let root = PathBuf::from(".");
        let manifest = root.join("caixa.lisp");
        if manifest.exists() {
            out.push(manifest);
        }
        if let Ok(dir) = std::fs::read_dir(root.join("lib")) {
            for entry in dir.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|e| e == "lisp") {
                    out.push(p);
                }
            }
        }
        Ok(out)
    }
}
