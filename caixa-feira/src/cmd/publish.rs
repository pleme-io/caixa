use std::path::PathBuf;
use std::process::Command;

use anyhow::{Result, bail};
use caixa_core::DEFAULT_PUBLISH_TAG_PREFIX;
use clap::Args;

use super::load::{caixa_root, load_caixa};

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

    /// Tag prefix. `feira publish` tags `<prefix><versao>` — defaults to
    /// the lifted [`caixa_core::DEFAULT_PUBLISH_TAG_PREFIX`] (`"v"`) so
    /// the writer-side default shares one source of truth with the
    /// reader-side [`caixa_flux::cluster_bundle`]
    /// `ClusterBundleOpts::for_caixa` constructor (caixa-flux/src/lib.rs).
    /// A future Zig-style-tag-convention rebrand reaches both consumers
    /// through one `&'static str` by construction; drift would silently
    /// emit a publish-side tag at one shape and a FluxCD-side
    /// `GitRepository.ref.tag` pointing at the prior shape, dropping
    /// every dependent `HelmRelease`'s `chart: sourceRef` resolution at
    /// reconcile time far from the rebrand commit's source. See the
    /// lifted constant's body for the full drift-mode analysis.
    #[arg(long, default_value = DEFAULT_PUBLISH_TAG_PREFIX)]
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
        let root = caixa_root(self.path.as_deref());
        let caixa = load_caixa(&root)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Args as _, FromArgMatches};

    #[test]
    fn publish_prefix_default_pins_lifted_caixa_core_constant() {
        // Fail-before-pass-after pin: the `--prefix` clap default must
        // resolve to the lifted [`caixa_core::DEFAULT_PUBLISH_TAG_PREFIX`],
        // not an inline `"v"` literal the peer
        // [`caixa_flux::cluster_bundle`] `ClusterBundleOpts::for_caixa`
        // constructor's default `GitRefSpec::Tag(...)` could silently
        // drift on.
        //
        // Until this lift landed both consumers carried the bare `"v"`
        // byte inline — this verb's clap `default_value = "v"` and
        // caixa-flux/src/lib.rs:335's `format!("v{}", caixa.versao)`. A
        // future Zig-style-tag rebrand on one side (e.g. moving the
        // publisher to `release/<versao>` once a sibling forge
        // convention adopts the `<type>/<value>` slash-namespaced shape)
        // without a coordinated edit on the deploy-side would silently
        // emit a tag the deploy-side `GitRepository` reconciler can't
        // resolve — every per-Servico apply would silently come up with
        // the prior reconciled state, far from the rebrand commit's
        // source.
        //
        // Pin the parsed default through clap's `augment_args` +
        // `FromArgMatches` so a regression that re-inlines the `"v"`
        // literal at this site (or routes `default_value` through a
        // sibling const) surfaces here as a build-time test failure,
        // peer to the sibling [`caixa_flux::tests`]
        // `cluster_bundle_default_git_tag_uses_lifted_caixa_core_prefix`
        // test closing the same drift on the reader-side.
        let cmd = Publish::augment_args(clap::Command::new("publish"));
        let matches = cmd
            .try_get_matches_from(["publish"])
            .expect("parsing `publish` with no args must succeed");
        let parsed = Publish::from_arg_matches(&matches)
            .expect("from_arg_matches must succeed for defaults");
        assert_eq!(
            parsed.prefix, DEFAULT_PUBLISH_TAG_PREFIX,
            "Publish::prefix default must equal the lifted \
             caixa_core::DEFAULT_PUBLISH_TAG_PREFIX — drift between this \
             writer-side default and the peer caixa-flux deploy-side \
             default silently breaks FluxCD GitRepository tag resolution"
        );
    }
}
