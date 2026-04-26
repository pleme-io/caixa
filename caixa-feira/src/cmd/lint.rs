use std::path::PathBuf;

use anyhow::{Context, Result};
use caixa_lint::{FixSafety, Severity, apply_fixes, lint_source};
use caixa_theme::Theme;
use clap::Args;

/// Run caixa-lint — Ruby+Rust distilled best practices. Prints Nord-themed
/// diagnostics; exits non-zero if any error-level rule fires.
///
/// `--fix` writes mechanically-safe corrections back to the source. Loops
/// until no more safe fixes apply (so cascading rules converge in one
/// invocation). `--fix-unsafe` additionally applies heuristic fixes that
/// might change behavior.
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

    /// Apply mechanically-safe autofixes back to disk.
    #[arg(long)]
    pub fix: bool,

    /// Also apply heuristic (potentially behavior-changing) autofixes.
    /// Implies `--fix`.
    #[arg(long)]
    pub fix_unsafe: bool,

    /// With `--fix`, print the diff/result instead of writing back.
    #[arg(long)]
    pub fix_dry_run: bool,
}

impl Lint {
    pub fn run(mut self) -> Result<()> {
        if self.fix_unsafe {
            self.fix = true;
        }
        let targets = self.resolve_targets()?;
        let theme = Theme::blackmatter_dark();
        let mut error_count = 0usize;
        let mut total_fixes = 0usize;

        for path in &targets {
            let mut src = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;

            // If --fix is on, loop until no more safe fixes apply.
            // Each pass re-lints since rules may produce new fixes
            // after a previous one rewrote the form.
            if self.fix {
                let safety = if self.fix_unsafe {
                    FixSafety::Unsafe
                } else {
                    FixSafety::Safe
                };
                let mut applied_in_path = 0usize;
                loop {
                    let diags = lint_source(&src)
                        .with_context(|| format!("linting {}", path.display()))?;
                    let result = apply_fixes(&src, &diags, safety);
                    if result.applied == 0 {
                        break;
                    }
                    applied_in_path += result.applied;
                    src = result.source;
                }
                if applied_in_path > 0 {
                    if self.fix_dry_run {
                        println!("=== {} ({} fix{}) ===", path.display(), applied_in_path,
                                 if applied_in_path == 1 { "" } else { "es" });
                        println!("{src}");
                    } else {
                        std::fs::write(path, &src)
                            .with_context(|| format!("writing {}", path.display()))?;
                    }
                }
                total_fixes += applied_in_path;
            }

            // Final lint pass for reporting (any leftover diagnostics
            // that weren't autofixable, or all diagnostics if --fix is off).
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

        if self.fix {
            eprintln!(
                "caixa-lint: {} file(s) checked, {total_fixes} fix(es) applied, {error_count} remaining error(s)",
                targets.len()
            );
        } else {
            eprintln!(
                "caixa-lint: {} file(s) checked, {error_count} error(s)",
                targets.len()
            );
        }
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
