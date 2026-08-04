//! `feira dialeto` — classify every `caixa.lisp` / `*.caixa.lisp` in a tree,
//! and refuse the states that let two declarations share one keyword.
//!
//! # The gate, and why it is shaped this way
//!
//! `defcaixa` was spoken by two unrelated declarations (see
//! [`caixa_core::dialeto`]). The migration to `defmolde` for the repo-surface
//! dialect cannot be a big bang — the legacy spelling is live in manifests
//! across the org, in repos this change does not touch. So the gate enforces
//! the properties that CAN hold today, hard and by default:
//!
//! * **every manifest classifies.** A `(defcaixa …)` matching neither schema
//!   is a failure, not a shrug. This is what stops a third dialect appearing.
//! * **no manifest is a wrong-dialect `caixa.lisp`.** A plain `caixa.lisp`
//!   (the file `feira` itself reads) that turns out to be a repo-surface
//!   declaration is a failure — that is the collision actually biting.
//! * **`--strict-palavra` refuses the legacy spelling.** Off by default *only*
//!   because the legacy corpus exists; ON in every scaffolder-facing path, so
//!   a newly generated file can never be born in the ambiguous spelling.
//!
//! What it deliberately does NOT do is fail a `*.caixa.lisp` that speaks the
//! Molde dialect under the legacy keyword. Those are ~93% of the corpus, they
//! are correct inputs to their own consumer, and failing them would make the
//! gate un-adoptable — which is how the previous check ended up
//! `|| echo "::warning"`. A gate nobody can turn on is not a gate.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use caixa_core::{CaixaDialeto, DialetoError, classify_dialeto};
use clap::Args;

/// The manifest filenames this command recognises.
const NOME_SIMPLES: &str = "caixa.lisp";
const SUFIXO_PONTUADO: &str = ".caixa.lisp";

/// Classify caixa manifests by dialect; fail on an unclassifiable or
/// wrong-dialect manifest.
#[derive(Args)]
pub struct Dialeto {
    /// Tree to walk. Defaults to the current directory.
    #[arg(value_name = "PATH", default_value = ".")]
    pub path: PathBuf,

    /// Print one row per manifest instead of only the summary + failures.
    #[arg(long)]
    pub list: bool,

    /// Also refuse a repo-surface declaration still written as `(defcaixa …)`.
    ///
    /// The regrowth seal. A scaffolder emitting a new manifest runs with this
    /// on, so the ambiguous spelling cannot be re-introduced even while the
    /// legacy corpus is being migrated.
    #[arg(long)]
    pub strict_palavra: bool,
}

/// One classified manifest.
struct Linha {
    path: PathBuf,
    verdict: Result<CaixaDialeto, DialetoError>,
    /// True for a plain `caixa.lisp` — the filename `feira` itself loads.
    manifesto_de_pacote: bool,
    /// True when the head symbol read `defcaixa` rather than `defmolde`.
    palavra_legada: bool,
}

impl Dialeto {
    pub fn run(self) -> Result<()> {
        let files = collect(&self.path)?;

        // Calibration. Every count below is over `files`; an empty walk would
        // report "0 failures" and exit 0 having examined nothing. That green
        // is indistinguishable from a real pass, and it is exactly how a
        // mis-pointed path or an ignore rule turns a gate into decoration.
        if files.is_empty() {
            bail!(
                "no caixa manifests found under {} — refusing to report a clean \
                 dialect census over a tree the walk never read. Expected at \
                 least one `{NOME_SIMPLES}` or `*{SUFIXO_PONTUADO}`.",
                self.path.display()
            );
        }

        let mut linhas = Vec::with_capacity(files.len());
        for path in files {
            let src = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let palavra_legada = !src.lines().any(|l| l.trim_start().starts_with("(defmolde"));
            linhas.push(Linha {
                manifesto_de_pacote: path
                    .file_name()
                    .is_some_and(|n| n == std::ffi::OsStr::new(NOME_SIMPLES)),
                verdict: classify_dialeto(&src),
                palavra_legada,
                path,
            });
        }

        let mut falhas: Vec<String> = Vec::new();
        let mut pacote = 0usize;
        let mut molde = 0usize;
        let mut posicional = 0usize;
        let mut desconhecido = 0usize;
        let mut ilegivel = 0usize;
        let mut legado = 0usize;

        for l in &linhas {
            let shown = l.path.display();
            match &l.verdict {
                Ok(CaixaDialeto::Pacote) => pacote += 1,
                Ok(CaixaDialeto::Molde) => molde += 1,
                Ok(CaixaDialeto::MoldePosicional) => posicional += 1,
                Ok(CaixaDialeto::Desconhecido) => {
                    desconhecido += 1;
                    falhas.push(format!(
                        "{shown}: a `(defcaixa …)` form matching no known schema. \
                         Either it is a package manifest missing `:nome`, or it is \
                         a third dialect — and a third dialect is the thing this \
                         gate exists to prevent."
                    ));
                }
                Err(e) => {
                    ilegivel += 1;
                    falhas.push(format!("{shown}: {e}"));
                }
            }

            if let Ok(d) = &l.verdict {
                if l.palavra_legada
                    && matches!(d, CaixaDialeto::Molde | CaixaDialeto::MoldePosicional)
                {
                    legado += 1;
                    if self.strict_palavra {
                        falhas.push(format!(
                            "{shown}: repo-surface declaration written as `(defcaixa …)`. \
                             Write `(defmolde …)` — `defcaixa` is the tatara-lisp \
                             package manifest and the two are different declarations."
                        ));
                    }
                }
                // The collision actually biting: `feira` loads `caixa.lisp` by
                // that exact name, so a plain `caixa.lisp` holding the other
                // declaration is a file `feira` will try to read and cannot.
                if l.manifesto_de_pacote
                    && matches!(d, CaixaDialeto::Molde | CaixaDialeto::MoldePosicional)
                    && !l.palavra_legada
                {
                    falhas.push(format!(
                        "{shown}: a `defmolde` declaration under the filename \
                         `{NOME_SIMPLES}`, which `feira` loads as a package \
                         manifest. Rename the file or the declaration."
                    ));
                }
            }

            if self.list {
                let d = match &l.verdict {
                    Ok(d) => d.to_string(),
                    Err(_) => "ILEGIVEL".to_string(),
                };
                println!("{d}\t{shown}");
            }
        }

        let total = linhas.len();
        eprintln!(
            "feira dialeto: {total} manifest(s) under {} — \
             Pacote {pacote} · Molde {molde} · MoldePosicional {posicional} · \
             Desconhecido {desconhecido} · ilegivel {ilegivel} \
             ({legado} repo-surface manifest(s) still written as `defcaixa`)",
            self.path.display()
        );

        if falhas.is_empty() {
            return Ok(());
        }
        for f in &falhas {
            eprintln!("feira dialeto: {f}");
        }
        bail!("{} dialect violation(s)", falhas.len())
    }
}

/// Walk `root` for caixa manifests. Skips build output and VCS metadata; a
/// scan that descended into `target/` would classify vendored fixtures as
/// this repo's manifests.
fn collect(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if root.is_file() {
        out.push(root.to_path_buf());
        return Ok(out);
    }
    walk(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries =
        std::fs::read_dir(dir).with_context(|| format!("reading dir {}", dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if entry.file_type()?.is_dir() {
            if matches!(name.as_ref(), "target" | ".git" | "node_modules" | "vendor") {
                continue;
            }
            walk(&path, out)?;
        } else if name == NOME_SIMPLES || name.ends_with(SUFIXO_PONTUADO) {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(dir: &Path, rel: &str, body: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(p, body).expect("write");
    }

    const PACOTE: &str = r#"(defcaixa :nome "demo" :versao "0.1.0" :kind Biblioteca)"#;
    const MOLDE_LEGADO: &str =
        r#"(defcaixa :name "demo" :kind :Biblioteca :ecosystem :rust-single-crate)"#;
    const MOLDE_NOVO: &str =
        r#"(defmolde :name "demo" :kind :Biblioteca :ecosystem :rust-single-crate)"#;

    #[test]
    fn an_empty_tree_fails_rather_than_reporting_a_clean_census() {
        // The calibration. Without it every other assertion here is satisfied
        // by a walk that read nothing.
        let dir = tempdir().expect("tempdir");
        let cmd = Dialeto {
            path: dir.path().to_path_buf(),
            list: false,
            strict_palavra: false,
        };
        let err = cmd.run().expect_err("an empty tree must not pass");
        assert!(
            err.to_string().contains("no caixa manifests found"),
            "got: {err}"
        );
    }

    #[test]
    fn a_mixed_but_classifiable_tree_passes_by_default() {
        // The corpus as it actually is: package manifests and legacy-spelled
        // repo-surface manifests side by side. The gate must be adoptable on
        // this, or it gets turned off.
        let dir = tempdir().expect("tempdir");
        write(dir.path(), "a/caixa.lisp", PACOTE);
        write(dir.path(), "b/base64.caixa.lisp", MOLDE_LEGADO);
        write(dir.path(), "c/demo.caixa.lisp", MOLDE_NOVO);
        let cmd = Dialeto {
            path: dir.path().to_path_buf(),
            list: false,
            strict_palavra: false,
        };
        cmd.run().expect("a classifiable tree must pass");
    }

    #[test]
    fn an_unclassifiable_defcaixa_fails() {
        let dir = tempdir().expect("tempdir");
        write(dir.path(), "a/caixa.lisp", PACOTE);
        write(
            dir.path(),
            "b/weird.caixa.lisp",
            r#"(defcaixa :licenca "MIT")"#,
        );
        let cmd = Dialeto {
            path: dir.path().to_path_buf(),
            list: false,
            strict_palavra: false,
        };
        let err = cmd.run().expect_err("a third dialect must fail");
        assert!(
            err.to_string().contains("1 dialect violation"),
            "got: {err}"
        );
    }

    #[test]
    fn strict_palavra_refuses_the_legacy_spelling() {
        let dir = tempdir().expect("tempdir");
        write(dir.path(), "b/base64.caixa.lisp", MOLDE_LEGADO);
        let lax = Dialeto {
            path: dir.path().to_path_buf(),
            list: false,
            strict_palavra: false,
        };
        lax.run().expect("legacy spelling is tolerated by default");
        let strict = Dialeto {
            path: dir.path().to_path_buf(),
            list: false,
            strict_palavra: true,
        };
        let err = strict.run().expect_err("--strict-palavra must refuse it");
        assert!(
            err.to_string().contains("1 dialect violation"),
            "got: {err}"
        );
    }

    #[test]
    fn a_defmolde_named_caixa_lisp_fails_because_feira_loads_that_filename() {
        let dir = tempdir().expect("tempdir");
        write(dir.path(), "a/caixa.lisp", MOLDE_NOVO);
        let cmd = Dialeto {
            path: dir.path().to_path_buf(),
            list: false,
            strict_palavra: false,
        };
        let err = cmd
            .run()
            .expect_err("wrong declaration under caixa.lisp must fail");
        assert!(
            err.to_string().contains("1 dialect violation"),
            "got: {err}"
        );
    }

    #[test]
    fn build_output_is_not_walked() {
        // A fixture under target/ classified as this repo's manifest would
        // make the gate fail on somebody else's vendored file.
        let dir = tempdir().expect("tempdir");
        write(dir.path(), "caixa.lisp", PACOTE);
        write(dir.path(), "target/junk/x.caixa.lisp", "(((((");
        let cmd = Dialeto {
            path: dir.path().to_path_buf(),
            list: false,
            strict_palavra: false,
        };
        cmd.run().expect("target/ must be skipped");
    }
}
