use std::path::PathBuf;

use anyhow::{Context, Result};
use caixa_fmt::{FmtConfig, format_source};
use clap::Args;

use super::load::resolve_lisp_targets;

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
            // `--stdout` is a filter: it must emit the document every time,
            // including when it was already formatted. The unchanged-file
            // skip below used to run first, so `feira fmt --stdout` on a
            // clean file printed nothing and exited 0 — which silently
            // truncates the buffer of any editor wired to capture stdout.
            if self.stdout {
                print!("{formatted}");
                any_changed |= formatted != src;
                continue;
            }
            if formatted == src {
                continue;
            }
            any_changed = true;
            if self.check {
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
        resolve_lisp_targets(&self.paths, "fmt")
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

    fn fmt_with_paths(paths: Vec<PathBuf>) -> Fmt {
        Fmt {
            paths,
            check: false,
            stdout: false,
            line_width: None,
        }
    }

    #[test]
    fn directory_target_expands_to_manifest_plus_sorted_lib_lisps() {
        // Fail-before-pass-after pin: before the shared-helper lift
        // `feira fmt`'s `resolve_targets` returned `self.paths.clone()`
        // for any non-empty input, so a directory arg fell through to
        // `read_to_string` and surfaced `Is a directory (os error 21)`
        // — the same cryptic OS error `feira lint`'s 32242c8 lift
        // fixed on the peer verb. Directory-argument support is now
        // inherited from the shared
        // [`super::super::load::resolve_lisp_targets`] helper. The
        // sorted-`lib/*.lisp` discipline is the second load-bearing
        // property the pre-lift `feira fmt` silently drifted from:
        // its inline `for entry in dir.flatten()` walk emitted
        // `reformatted <path>` lines in filesystem-order,
        // nondeterministic across a `tmpfs` vs. an `ext4` checkout,
        // and its `--check` exit-code path could not be diff-stabilized
        // against a known-good CI baseline; the shared helper's
        // `lib_files.sort()` fixes both axes at once. Peer with the
        // `directory_target_expands_to_manifest_plus_sorted_lib_lisps`
        // pin on the sibling `feira lint` arm.
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("caixa.lisp"), "");
        write(&root.join("lib").join("beta.lisp"), "");
        write(&root.join("lib").join("alpha.lisp"), "");
        write(&root.join("lib").join("readme.md"), "");

        let cmd = fmt_with_paths(vec![root.to_path_buf()]);
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
        // Peer with the pre-lift `resolve_targets` fast-path: an
        // explicit `.lisp` file argument round-trips the resolver
        // unchanged. Load-bearing so a `feira fmt path/to/one.lisp`
        // invocation keeps its cheap single-file shape when the caller
        // already knows the concrete file.
        let tmp = tempdir().unwrap();
        let file = tmp.path().join("solo.lisp");
        write(&file, "");
        let cmd = fmt_with_paths(vec![file.clone()]);
        assert_eq!(cmd.resolve_targets().unwrap(), vec![file]);
    }

    #[test]
    fn nonexistent_target_names_offending_path_with_fmt_verb() {
        // Sharpen-a-promise pin: before this lift `feira fmt`'s
        // missing-target path fell through to `std::fs::read_to_string`
        // which surfaced the cryptic OS error `No such file or
        // directory (os error 2)` with no self-locating hint at the
        // expected caixa-root shape. Routed through the shared
        // [`super::super::load::resolve_lisp_targets`] helper the
        // diagnostic now names both the offending path verbatim (so
        // the author can grep their shell history for the bad arg) and
        // the caixa-root shape (`caixa.lisp` + `lib/*.lisp`) the walk
        // expects. The `"fmt"` verb-hint self-locates the diagnostic
        // to the verb the author invoked, distinct from the sibling
        // [`super::super::lint::tests`] arm's `"lint"` verb-hint. Peer
        // with the per-verb-arg [`super::super::load::validate_cluster_arg`] /
        // [`super::super::load::validate_namespace_arg`] /
        // [`super::super::load::validate_nome_arg`] gates on the
        // sibling per-verb arg-entry named-value axis.
        let tmp = tempdir().unwrap();
        let missing = tmp.path().join("nope.lisp");
        let cmd = fmt_with_paths(vec![missing.clone()]);
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
            err.contains("fmt"),
            "error must self-locate to `feira fmt`, got: {err}"
        );
    }
}
