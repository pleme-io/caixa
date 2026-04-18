use std::path::PathBuf;

use anyhow::{Context, Result};
use caixa_fmt::{FmtConfig, format_source};
use clap::Args;

/// Format caixa.lisp (or any .lisp file) via caixa-fmt.
///
/// Behavior mirrors `cargo fmt`: in-place rewrite by default, `--check` for
/// a non-zero exit if the file isn't already formatted.
#[derive(Args)]
pub struct Fmt {
    /// Paths to format. Defaults to `./caixa.lisp` + every `.lisp` under `lib/`.
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Check only — exit 0 if already-formatted, 1 otherwise. Don't write.
    #[arg(long)]
    pub check: bool,

    /// Print the formatted output to stdout instead of writing it.
    #[arg(long)]
    pub stdout: bool,

    /// Override the line width (default 100).
    #[arg(long)]
    pub line_width: Option<usize>,
}

impl Fmt {
    pub fn run(self) -> Result<()> {
        let cfg = FmtConfig {
            line_width: self.line_width.unwrap_or(100),
            ..FmtConfig::default()
        };
        let targets = self.resolve_targets()?;
        let mut any_changed = false;
        for path in &targets {
            let src = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            let formatted = format_source(&src, &cfg)
                .with_context(|| format!("formatting {}", path.display()))?;
            if formatted == src {
                continue;
            }
            any_changed = true;
            if self.stdout {
                print!("{formatted}");
            } else if self.check {
                eprintln!("would reformat {}", path.display());
            } else {
                std::fs::write(path, &formatted)
                    .with_context(|| format!("writing {}", path.display()))?;
                eprintln!("reformatted {}", path.display());
            }
        }
        if self.check && any_changed {
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
