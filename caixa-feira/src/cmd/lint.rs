use std::path::PathBuf;

use anyhow::{Context, Result};
use caixa_fmt::{FmtConfig, format_source};
use caixa_lint::{FixSafety, apply_fixes, lint_source};
use caixa_theme::Theme;
use clap::Args;

use super::load::resolve_lisp_targets;

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
                    let diags =
                        lint_source(&src).with_context(|| format!("linting {}", path.display()))?;
                    let result = apply_fixes(&src, &diags, safety);
                    if result.applied == 0 {
                        break;
                    }
                    applied_in_path += result.applied;
                    src = result.source;
                }
                if applied_in_path > 0 {
                    // Re-fmt after autofix: edits might shift keywords
                    // around in ways the previous fmt didn't anticipate.
                    // Running fmt again converges on the canonical layout.
                    let cfg = FmtConfig::default();
                    if let Ok(reformatted) = format_source(&src, &cfg) {
                        src = reformatted;
                    }
                    if self.fix_dry_run {
                        println!(
                            "=== {} ({} fix{}) ===",
                            path.display(),
                            applied_in_path,
                            if applied_in_path == 1 { "" } else { "es" }
                        );
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
                // Route the errors-only retain through the substrate-side
                // [`caixa_lint::Severity::is_error`] predicate the
                // [`gen_platform::IsVariant`] derive on
                // [`caixa_lint::Severity`] emits, so the two-arm
                // error-family gate in this verb (this
                // retain + the paired per-diagnostic error-count bump
                // below) shares one convention with every peer downstream
                // severity-family gate — the same closed-set arm-
                // discriminator discipline the sibling
                // `caixa_arch::ArchReport::passed` /
                // `caixa_feira::cmd::tofu::render` converge onto
                // `ArchVerdict::is_proven` / `is_rejected` (94dafa6) and
                // the peer `caixa_arch::run::check_manifest` +
                // `ArchReport::safety_count` converge onto
                // `InvariantKind::is_safety` / `is_compliance` /
                // `is_hint` (5226ad5) already carry.
                diags.retain(|d| d.severity.is_error());
            }
            for d in &diags {
                if d.severity.is_error() {
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
        resolve_lisp_targets(&self.paths, "lint")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::tempdir;

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, body).unwrap();
    }

    fn lint_with_paths(paths: Vec<PathBuf>) -> Lint {
        Lint {
            paths,
            errors_only: false,
            no_color: true,
            fix: false,
            fix_unsafe: false,
            fix_dry_run: false,
        }
    }

    #[test]
    fn directory_target_expands_to_manifest_plus_sorted_lib_lisps() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("caixa.lisp"), "");
        write(&root.join("lib").join("beta.lisp"), "");
        write(&root.join("lib").join("alpha.lisp"), "");
        write(&root.join("lib").join("readme.md"), "");

        let cmd = lint_with_paths(vec![root.to_path_buf()]);
        let targets = cmd.resolve_targets().unwrap();

        assert_eq!(
            targets,
            vec![
                root.join("caixa.lisp"),
                root.join("lib").join("alpha.lisp"),
                root.join("lib").join("beta.lisp"),
            ],
        );
    }

    #[test]
    fn file_target_passes_through_untouched() {
        let tmp = tempdir().unwrap();
        let file = tmp.path().join("solo.lisp");
        write(&file, "");
        let cmd = lint_with_paths(vec![file.clone()]);
        assert_eq!(cmd.resolve_targets().unwrap(), vec![file]);
    }

    #[test]
    fn nonexistent_target_names_offending_path_with_lint_verb() {
        // Pins that the missing-target diagnostic routed through the
        // shared [`super::super::load::resolve_lisp_targets`] helper
        // self-locates to `feira lint` (verb = "lint") — a future
        // refactor that dropped the per-verb `verb` argument, or
        // routed `feira fmt`'s call-site through the same helper with
        // `verb = "fmt"` by mistake, would silently split the "no
        // such <verb> target: <path>" byte-string the author greps
        // for. The named-value discipline mirrors the peer
        // [`super::super::load::validate_cluster_arg`] /
        // [`super::super::load::validate_namespace_arg`] /
        // [`super::super::load::validate_nome_arg`] gates on the
        // sibling per-verb arg-entry axes.
        let tmp = tempdir().unwrap();
        let missing = tmp.path().join("nope.lisp");
        let cmd = lint_with_paths(vec![missing.clone()]);
        let err = cmd.resolve_targets().unwrap_err().to_string();
        assert!(
            err.contains(&missing.display().to_string()),
            "error must name the offending path, got: {err}"
        );
        assert!(
            err.contains("caixa.lisp"),
            "error must point at the expected caixa-root shape, got: {err}"
        );
        assert!(
            err.contains("lint"),
            "error must self-locate to `feira lint`, got: {err}"
        );
    }
}
