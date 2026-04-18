use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
use caixa_core::Caixa;
use clap::Args;

/// Publish the current caixa by tagging its Git HEAD and pushing the tag
/// to `origin`.
///
/// Store model = Git, Zig-style. There is no central registry — publishing a
/// caixa is the same mechanism Nix flakes use: a tag on a Git repo. Consumers
/// of this caixa pin `:tag "v<versao>"`.
#[derive(Args)]
pub struct Publish {
    /// Optional semver override. Defaults to the caixa's `:versao`.
    #[arg(long)]
    pub versao: Option<String>,

    /// Tag prefix (default `v`). `feira publish` tags `v<versao>`.
    #[arg(long, default_value = "v")]
    pub prefix: String,

    /// The Git remote to push to.
    #[arg(long, default_value = "origin")]
    pub remote: String,

    /// Skip the push — create the tag locally only.
    #[arg(long)]
    pub no_push: bool,

    /// caixa root (defaults to CWD).
    #[arg(long)]
    pub path: Option<PathBuf>,
}

impl Publish {
    pub fn run(self) -> Result<()> {
        let root = self.path.clone().unwrap_or_else(|| PathBuf::from("."));
        let manifest = root.join("caixa.lisp");
        let src = std::fs::read_to_string(&manifest)
            .with_context(|| format!("reading {}", manifest.display()))?;
        let caixa =
            Caixa::from_lisp(&src).with_context(|| format!("parsing {}", manifest.display()))?;

        let versao = self.versao.clone().unwrap_or(caixa.versao.clone());
        let tag = format!("{}{versao}", self.prefix);

        // Refuse to publish if the working tree is dirty.
        let status = run_git(&root, ["status", "--porcelain"])?;
        if !status.trim().is_empty() {
            bail!("working tree is dirty — commit or stash first:\n{status}");
        }

        // Create tag at HEAD.
        let msg = format!("caixa {} {tag}", caixa.nome);
        exec_git(&root, ["tag", "-a", &tag, "-m", &msg])?;

        if !self.no_push {
            exec_git(&root, ["push", &self.remote, &tag])?;
            eprintln!("published {tag} to {}", self.remote);
        } else {
            eprintln!("created tag {tag} locally (not pushed)");
        }
        Ok(())
    }
}

fn exec_git<'a, I: IntoIterator<Item = &'a str>>(cwd: &std::path::Path, args: I) -> Result<()> {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(args.into_iter().collect::<Vec<_>>())
        .output()?;
    if !out.status.success() {
        bail!("git failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

fn run_git<'a, I: IntoIterator<Item = &'a str>>(cwd: &std::path::Path, args: I) -> Result<String> {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(args.into_iter().collect::<Vec<_>>())
        .output()?;
    if !out.status.success() {
        bail!("git failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
