use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use caixa_core::Caixa;
use clap::Args;

/// Deploy a caixa Servico to a target cluster by upserting its entry
/// into the cluster's lareira-fleet-programs HelmRelease values.
///
/// Canonical path:
///   `<k8s-repo>/clusters/<cluster>/programs/release.yaml`
/// (a HelmRelease whose `spec.values.programs[]` is the fleet manifest).
///
/// `feira deploy` is the headline operator-out-of-the-loop verb. The
/// chain in five lines:
///
///   1. parse caixa.lisp + servicos/<name>.computeunit.yaml
///   2. caixa_flux::programs_yaml_entry → typed YAML mapping
///   3. read the cluster's programs/release.yaml
///   4. upsert the entry by name into spec.values.programs[]
///      (replace if exists, append otherwise)
///   5. write back; log the diff path; optionally git commit + push
///
/// Default behaviour writes the change but does NOT auto-commit, so the
/// operator can review the diff before publishing. Use `--commit` for
/// auto-commit (single-operator workflow), or `--apply` for full
/// commit + push.
#[derive(Args)]
pub struct Deploy {
    /// Cluster name (e.g. `rio`, `mar`, `plo`). Selects the k8s tree
    /// path: `<k8s-repo>/clusters/<cluster>/programs/release.yaml`.
    #[arg(long)]
    pub cluster: String,

    /// Path to the GitOps k8s repo. Defaults to the env var
    /// `PLEME_K8S_REPO` or `~/code/github/pleme-io/k8s` if neither is set.
    #[arg(long, env = "PLEME_K8S_REPO")]
    pub k8s_repo: Option<PathBuf>,

    /// Path to the fleet-programs HelmRelease inside the k8s repo,
    /// relative to it. Default: `clusters/<cluster>/programs/release.yaml`.
    #[arg(long)]
    pub programs_yaml: Option<PathBuf>,

    /// Auto-commit the change after writing (no push).
    #[arg(long, conflicts_with = "apply")]
    pub commit: bool,

    /// Auto-commit AND push to origin (full automation).
    #[arg(long)]
    pub apply: bool,

    /// Print the resulting YAML to stdout instead of writing it.
    /// Useful for dry-run / CI verification.
    #[arg(long)]
    pub dry_run: bool,

    /// caixa root (defaults to CWD).
    #[arg(long)]
    pub path: Option<PathBuf>,
}

impl Deploy {
    pub fn run(self) -> Result<()> {
        // 1. Load the caixa + computeunit.
        let root = self.path.clone().unwrap_or_else(|| PathBuf::from("."));
        let manifest_path = root.join("caixa.lisp");
        let src = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        let caixa = Caixa::from_lisp(&src)
            .with_context(|| format!("parsing {}", manifest_path.display()))?;

        let cu_path = super::chart::first_servico_path(&caixa, &root)?;
        let cu_src = std::fs::read_to_string(&cu_path)
            .with_context(|| format!("reading {}", cu_path.display()))?;
        let cu_yaml: serde_yaml::Value = serde_yaml::from_str(&cu_src)
            .with_context(|| format!("parsing {}", cu_path.display()))?;

        // 2. Render the entry.
        let entry = caixa_flux::programs_yaml_entry(&caixa, &cu_yaml)?;

        // 3. Resolve target programs.yaml path.
        let k8s_repo = self
            .k8s_repo
            .clone()
            .or_else(|| {
                dirs::home_dir().map(|h| h.join("code/github/pleme-io/k8s"))
            })
            .ok_or_else(|| anyhow::anyhow!("could not resolve k8s repo path"))?;

        let programs_rel = self
            .programs_yaml
            .clone()
            .unwrap_or_else(|| {
                PathBuf::from("clusters")
                    .join(&self.cluster)
                    .join("programs")
                    .join("release.yaml")
            });
        let programs_abs = k8s_repo.join(&programs_rel);

        if !programs_abs.exists() {
            bail!(
                "fleet-programs HelmRelease not found at {}\n\
                 (expected the k8s GitOps repo to have clusters/{}/programs/release.yaml; \
                 set --k8s-repo / PLEME_K8S_REPO or --programs-yaml if the path is non-default)",
                programs_abs.display(),
                self.cluster,
            );
        }

        let existing_src = std::fs::read_to_string(&programs_abs)
            .with_context(|| format!("reading {}", programs_abs.display()))?;
        let existing: serde_yaml::Value = serde_yaml::from_str(&existing_src)
            .with_context(|| format!("parsing {}", programs_abs.display()))?;

        // 4. Upsert the entry into spec.values.programs[] of the HelmRelease.
        let (new_doc, inserted) = caixa_flux::upsert_into_helmrelease_programs(existing, entry)?;
        let new_src = render_yaml_with_header(&new_doc, &programs_rel.display().to_string())?;

        if self.dry_run {
            print!("{new_src}");
            return Ok(());
        }

        // 5. Write + optionally commit/push.
        std::fs::write(&programs_abs, &new_src)
            .with_context(|| format!("writing {}", programs_abs.display()))?;

        let action = if inserted { "added" } else { "updated" };
        eprintln!(
            "{action} entry for {} v{} in {}",
            caixa.nome,
            caixa.versao,
            programs_abs.display()
        );

        if self.commit || self.apply {
            commit_change(&k8s_repo, &programs_rel, &caixa, action)?;
            eprintln!("committed change in {}", k8s_repo.display());
        }
        if self.apply {
            push_origin(&k8s_repo)?;
            eprintln!("pushed origin/main");
        } else if !self.commit {
            eprintln!(
                "review with: git -C {} diff -- {}",
                k8s_repo.display(),
                programs_rel.display(),
            );
            eprintln!("commit + push when ready (or rerun with --commit / --apply).");
        }

        Ok(())
    }
}

fn render_yaml_with_header(doc: &serde_yaml::Value, rel_path: &str) -> Result<String> {
    let body = serde_yaml::to_string(doc)?;
    let header = format!(
        "# {rel_path}\n\
         # Cluster fleet manifest — HelmRelease whose spec.values.programs[]\n\
         # is consumed by lareira-fleet-programs to render one ComputeUnit\n\
         # CR per entry.\n\
         #\n\
         # Edit via `feira deploy --cluster <name>`; manual edits are\n\
         # preserved on the next upsert as long as `name`-keyed entries\n\
         # are kept (lookup is by `name`, order is preserved).\n\n",
    );
    Ok(format!("{header}{body}"))
}

fn commit_change(
    repo: &std::path::Path,
    rel: &std::path::Path,
    caixa: &Caixa,
    action: &str,
) -> Result<()> {
    let msg = format!(
        "deploy: {action} {} v{}\n\
         \n\
         Updated by `feira deploy --cluster <name>`.\n",
        caixa.nome, caixa.versao,
    );
    git(repo, ["add", &rel.display().to_string()])?;
    git(repo, ["commit", "-m", &msg])?;
    Ok(())
}

fn push_origin(repo: &std::path::Path) -> Result<()> {
    git(repo, ["push", "origin", "HEAD"])?;
    Ok(())
}

fn git<'a, I: IntoIterator<Item = &'a str>>(cwd: &std::path::Path, args: I) -> Result<()> {
    use std::process::Command;
    let argv: Vec<&str> = args.into_iter().collect();
    let out = Command::new("git").current_dir(cwd).args(&argv).output()?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            argv.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}
