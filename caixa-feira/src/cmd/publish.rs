use std::path::PathBuf;
use std::process::Command;

use anyhow::{Result, bail};
use caixa_core::{Caixa, DEFAULT_GIT_REMOTE, DEFAULT_PUBLISH_TAG_PREFIX};
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

    /// The Git remote to push to. Defaults to the lifted
    /// [`caixa_core::DEFAULT_GIT_REMOTE`] (`"origin"`) so the writer-side
    /// publish verb shares one source of truth with the sibling
    /// `feira deploy --apply` / `feira app deploy --apply` verbs
    /// (caixa-feira/src/cmd/deploy.rs, caixa-feira/src/cmd/app.rs)
    /// whose `push_origin` helpers run `git push <remote> HEAD` against
    /// the k8s GitOps repo. A future remote-naming-convention rebrand
    /// (the substrate moving to `upstream` for forge-mirror clusters,
    /// to a per-tenant remote-naming convention, or to the canonical
    /// multi-remote `release` + `mirror` split) reaches all three
    /// consumers through one `&'static str` by construction; drift
    /// would silently emit a `git push` against a remote that doesn't
    /// exist on the operator's clone on one writer verb while the
    /// other two still pushed to the old remote, with the operator-
    /// observed symptom (the publish landed but the deploy didn't, or
    /// vice-versa) surfacing as a partial-state rollout far from the
    /// rebrand commit's source. See the lifted constant's body for the
    /// full drift-mode analysis.
    #[arg(long, default_value = DEFAULT_GIT_REMOTE)]
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

        let versao = publish_effective_versao(self.versao.as_deref(), &caixa);
        let tag = format!("{}{versao}", self.prefix);

        // Refuse to publish if the working tree is dirty.
        let status = run_git(&root, ["status", "--porcelain"])?;
        if !status.trim().is_empty() {
            bail!("working tree is dirty — commit or stash first:\n{status}");
        }

        // Create tag at HEAD.
        let msg = publish_tag_message(&caixa, &tag);
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

/// Resolve the effective `:versao` the `feira publish` verb tags HEAD
/// against. When the operator passes `--versao <v>` the CLI override
/// wins verbatim; otherwise the fallback derives through the typed
/// [`Caixa::versao`] `&str`-return universal-axis accessor so the
/// writer-side per-`Caixa` `:versao` byte-string every peer per-kind
/// renderer already emits (caixa-helm `Chart.yaml` `version:` /
/// `appVersion:` per eb912de / 05a7701, caixa-flux `programs.yaml`
/// `versao:` fold + `cluster_bundle` `GitRepository` `spec.ref.tag`
/// per 2fc5f81, caixa-crd `CaixaSpec.versao` per 41ab9a3, caixa-tatara
/// `Process` CR `AplicacaoIntent.version` per e73b19f, caixa-resolver
/// `FetchedDep::concrete_versao` per 0556249) shares one canonical
/// read-side surface with the tag this verb writes to `origin`. Peer
/// with [`publish_tag_message`] on the paired annotated-tag body
/// surface — the two writer-side emit sites (`tag`, `-m <message>`)
/// share one accessor rather than two raw field-accesses in lockstep.
pub(crate) fn publish_effective_versao(cli_versao: Option<&str>, caixa: &Caixa) -> String {
    cli_versao.map_or_else(|| caixa.versao().to_string(), str::to_string)
}

/// Compose the `git tag -a -m <message>` annotation body `feira publish`
/// stamps onto HEAD past the working-tree-clean gate. Derives its
/// terminal `{nome}` scalar through the typed [`Caixa::nome`] accessor
/// so a future tag-history reader (a future `feira publish rollback`
/// that scans annotation bodies for the `caixa <nome> v<versao>`
/// shape, a Git-tag-history audit walker projecting the caixa identity
/// out of the annotation axis, a future FluxCD `GitRepository.ref.tag`-
/// side reconciler that validates the annotation against the paired
/// `HelmRelease` `chart.spec.chart` name) reads a byte-string identical
/// to the one the paired downstream caixa-flux
/// `cluster_bundle::for_caixa` `GitRefSpec::Tag(v<versao>)` +
/// caixa-helm `Chart.yaml` `name: lareira-<nome>` emits already carry
/// under one accessor. Peer with [`publish_effective_versao`] on the
/// paired tag-value surface — the two writer-side emit sites share one
/// accessor rather than two raw field-accesses in lockstep.
pub(crate) fn publish_tag_message(caixa: &Caixa, tag: &str) -> String {
    format!("caixa {} {tag}", caixa.nome())
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

    #[test]
    fn publish_remote_default_pins_lifted_caixa_core_constant() {
        // Fail-before-pass-after pin: the `--remote` clap default must
        // resolve to the lifted [`caixa_core::DEFAULT_GIT_REMOTE`], not
        // an inline `"origin"` literal the sibling writer-side verbs
        // (`feira deploy --apply` / `feira app deploy --apply`)
        // `push_origin` helpers could silently drift on.
        //
        // Until this lift landed all three writer-side `feira` verbs
        // carried the bare `"origin"` byte inline — this verb's clap
        // `default_value = "origin"`, deploy.rs:187's `git(repo,
        // ["push", "origin", "HEAD"])`, and app.rs:249's symmetric
        // `git(repo, ["push", "origin", "HEAD"])`. A future remote-
        // naming-convention rebrand on one side (the substrate moving
        // to `upstream` for forge-mirror clusters, or to a per-tenant
        // naming convention, or to the canonical multi-remote
        // `release` + `mirror` split every Erlang/OTP relup shop
        // converges on once their git surface grows past one upstream)
        // without a coordinated edit on the other two would silently
        // emit a `git push` against a remote that doesn't exist on the
        // operator's clone on one writer verb while the other two
        // still pushed to the old remote — the operator-observed
        // symptom (the publish landed but the deploy didn't, or vice-
        // versa) surfacing as a partial-state rollout far from the
        // rebrand commit's source.
        //
        // Pin the parsed default through clap's `augment_args` +
        // `FromArgMatches` so a regression that re-inlines the
        // `"origin"` literal at this site surfaces here as a build-
        // time test failure, peer to the sibling
        // `publish_prefix_default_pins_lifted_caixa_core_constant`
        // test closing the same drift on the tag-prefix axis.
        let cmd = Publish::augment_args(clap::Command::new("publish"));
        let matches = cmd
            .try_get_matches_from(["publish"])
            .expect("parsing `publish` with no args must succeed");
        let parsed = Publish::from_arg_matches(&matches)
            .expect("from_arg_matches must succeed for defaults");
        assert_eq!(
            parsed.remote, DEFAULT_GIT_REMOTE,
            "Publish::remote default must equal the lifted \
             caixa_core::DEFAULT_GIT_REMOTE — drift between this \
             writer-side default and the peer `feira deploy --apply` / \
             `feira app deploy --apply` `push_origin` helpers silently \
             emits a `git push` against the wrong remote on one verb \
             while the others still target the canonical one"
        );
    }

    fn fixture_caixa(nome: &str, versao: &str) -> Caixa {
        let src = format!(
            "(defcaixa :nome \"{nome}\" :versao \"{versao}\" \
             :kind Biblioteca :bibliotecas ())"
        );
        Caixa::from_lisp(&src).expect("fixture caixa parses")
    }

    #[test]
    fn publish_effective_versao_routes_through_caixa_versao_accessor() {
        // Fail-before-pass-after pin: the `feira publish` no-`--versao`
        // fallback must resolve the tagged version through the typed
        // [`Caixa::versao`] accessor, not the raw `caixa.versao.clone()`
        // field-access this converge lifts. A regression that re-inlines
        // the raw field at the resolution site silently splits the
        // `origin`-pushed git tag from the per-`Caixa` `:versao`
        // byte-string every paired downstream substrate-side renderer
        // already emits (the caixa-flux `cluster_bundle`
        // `GitRepository.spec.ref.tag` per 2fc5f81, the caixa-helm
        // `Chart.yaml` `version:` / `appVersion:` per eb912de / 05a7701,
        // the caixa-crd `CaixaSpec.versao` per 41ab9a3, the caixa-tatara
        // `Process` CR `AplicacaoIntent.version` per e73b19f, the
        // caixa-resolver `FetchedDep::concrete_versao` per 0556249) —
        // every FluxCD `GitRepository` reconciler pinned on `v<versao>`
        // would silently miss the newly-published release.
        //
        // Peer with the sibling
        // [`super::deploy::deploy_summary_line_routes_through_caixa_nome_and_versao_accessors`]
        // pin on the peer `feira deploy` verb's operator-facing stderr-
        // notice surface, and with the sibling
        // [`super::app::deploy_commit_message_routes_through_caixa_nome_and_versao_accessors`]
        // pin on the peer `feira app deploy` verb's k8s-repo git-commit-
        // message surface.
        let caixa = fixture_caixa("checkout", "0.4.2");
        assert_eq!(
            publish_effective_versao(None, &caixa),
            caixa.versao(),
            "publish_effective_versao with no CLI override must \
             byte-equal the typed Caixa::versao accessor — a regression \
             that re-inlines the raw `caixa.versao.clone()` field-access \
             at this site silently splits the origin-pushed git tag \
             from the paired substrate-side renderer emit"
        );
        assert_eq!(publish_effective_versao(None, &caixa), "0.4.2");
    }

    #[test]
    fn publish_effective_versao_carries_cli_override_verbatim() {
        // Paired inversion pin: when the operator passes `--versao <v>`
        // the CLI override must win verbatim over the per-`Caixa`
        // `:versao` byte-string on disk. Pin the `Some(_)` arm so a
        // future refactor of the resolution cascade (e.g. promoting the
        // override to a typed `enum EffectiveVersao { CliOverride,
        // Manifest }` discriminant once the sibling `feira app
        // deploy` grows the same axis) reaches this resolver through
        // one canonical form rather than re-rolling a parallel cascade.
        let caixa = fixture_caixa("cart", "1.0.0");
        assert_eq!(
            publish_effective_versao(Some("2.0.0-rc.1"), &caixa),
            "2.0.0-rc.1",
            "publish_effective_versao with --versao <v> must carry the \
             CLI override verbatim, ignoring the manifest's :versao"
        );
    }

    #[test]
    fn publish_tag_message_routes_through_caixa_nome_accessor() {
        // Fail-before-pass-after pin: the `feira publish` annotated-tag
        // body's terminal `{nome}` scalar must resolve through the
        // typed [`Caixa::nome`] accessor, not the raw `caixa.nome`
        // field-access this converge lifts. A regression that re-inlines
        // the raw field at the emit site silently splits the annotated-
        // tag body a future Git-tag-history reader greps against (a
        // future `feira publish rollback` scanning annotation bodies
        // for the `caixa <nome> v<versao>` shape, a FluxCD
        // `GitRepository`-side audit walker projecting the caixa identity
        // out of the annotation axis, a future `feira publish --verify`
        // that keys off the annotation body against the paired
        // `caixa-flux` `cluster_bundle` bundle name) from the identity
        // every paired downstream substrate-side artefact already
        // carries under one accessor (the caixa-helm `Chart.yaml`
        // `name: lareira-<nome>` per eb912de, the caixa-flux
        // `programs.yaml` `name:` fold + `cluster_bundle`
        // `HelmRelease.metadata.name` per 4a363bf, the caixa-mesh
        // per-Aplicacao `programs.yaml` fan-out per 54bf2f3, the
        // caixa-crd `CaixaSpec.nome` per 61d3429).
        //
        // Peer with the sibling
        // [`super::deploy::deploy_summary_line_routes_through_caixa_nome_and_versao_accessors`]
        // pin on the peer `feira deploy` verb's stderr-notice surface,
        // and with the sibling
        // [`super::app::deploy_commit_message_routes_through_caixa_nome_and_versao_accessors`]
        // pin on the peer `feira app deploy` verb's commit-message
        // surface — the three writer-side emit surfaces (`git push`,
        // `git commit`, `git tag -a -m`) now share one accessor axis.
        let caixa = fixture_caixa("checkout", "0.4.2");
        let tag = "v0.4.2";
        let rendered = publish_tag_message(&caixa, tag);
        assert_eq!(
            rendered,
            format!("caixa {} {tag}", caixa.nome()),
            "publish_tag_message must derive its {{nome}} slot through \
             the typed Caixa::nome accessor — a regression that re-\
             inlines caixa.nome at the emit site silently splits the \
             annotated-tag body from the paired substrate-side renderer \
             emit"
        );
        assert_eq!(rendered, "caixa checkout v0.4.2");
    }

    #[test]
    fn publish_tag_message_carries_arbitrary_tag_verbatim() {
        // Paired pin on the `tag` slot: the composer must carry the
        // resolved tag byte-string verbatim into the annotation body,
        // regardless of the prefix / versao axes' independent resolution
        // upstream. Pin a non-default prefix + CLI-override versao
        // composition so a future refactor that folds the prefix into
        // the composer (e.g. promoting `prefix` from a `&str` arg to a
        // typed `TagShape` newtype once the CAIXA-SDLC §I SemVer-2 pin
        // grows a `release/<versao>` slash-namespaced peer) reaches
        // this site through one canonical form.
        let caixa = fixture_caixa("payment", "0.1.0");
        assert_eq!(
            publish_tag_message(&caixa, "release/2.0.0-rc.1"),
            "caixa payment release/2.0.0-rc.1"
        );
    }
}
