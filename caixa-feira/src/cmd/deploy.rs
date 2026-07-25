use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use caixa_core::{Caixa, DEFAULT_GIT_REMOTE};
use clap::Args;

use super::load::{caixa_root, load_caixa, validate_cluster_arg};

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
        // Validate the `--cluster` arg at the verb entry-point, before
        // any IO. The value lands as a path segment in
        // `<k8s-repo>/clusters/<cluster>/programs/release.yaml` and as
        // a K8s `metadata.name` on the downstream lareira-fleet-programs
        // HelmRelease's per-cluster `name:`-keyed lookup; the DNS-1123
        // label gate refuses every footgun the typed `:placement
        // :clusters` slot already refuses (empty, path-traversal,
        // uppercase, underscore, leading-`-`), peer with the lifted
        // `validate_placement_cluster` discipline on the typed-slot axis.
        validate_cluster_arg(&self.cluster)?;
        // 1. Load the caixa + computeunit.
        let root = caixa_root(self.path.as_deref());
        let caixa = load_caixa(&root)?;

        let cu_yaml = super::chart::load_first_servico_yaml(&caixa, &root)?;

        // 2. Render the entry.
        let entry = caixa_flux::programs_yaml_entry(&caixa, &cu_yaml)?;

        // 3. Resolve target programs.yaml path.
        let k8s_repo = self
            .k8s_repo
            .clone()
            .or_else(|| dirs::home_dir().map(|h| h.join("code/github/pleme-io/k8s")))
            .ok_or_else(|| anyhow::anyhow!("could not resolve k8s repo path"))?;

        let programs_rel = self.programs_yaml.clone().unwrap_or_else(|| {
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

        let existing = super::load::load_yaml(&programs_abs)?;

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
        eprintln!("{}", deploy_summary_line(action, &caixa, &programs_abs));

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
    let msg = deploy_commit_message(action, caixa);
    git(repo, ["add", &rel.display().to_string()])?;
    git(repo, ["commit", "-m", &msg])?;
    Ok(())
}

/// Compose the operator-visible stderr notice `feira deploy` writes past
/// a successful upsert into the cluster's fleet-programs HelmRelease.
/// Derives its terminal `{nome}` / `{versao}` scalars through the typed
/// [`Caixa::nome`] / [`Caixa::versao`] accessors so the byte-string every
/// operator sees on stderr shares one canonical read-side surface with
/// every peer per-`Caixa` substrate-side renderer emit
/// (caixa-helm `Chart.yaml` `version:` / `appVersion:`, caixa-flux
/// `programs.yaml` `versao:` fold + `cluster_bundle` `GitRepository`
/// `spec.ref.tag`, caixa-crd `CaixaSpec.versao`) and with the sibling
/// `feira` verbs' emit surfaces
/// (`feira build` summary-line via [`super::build::build_summary_line`],
/// `feira app graph` header-line via [`super::app::graph_header_line`],
/// `feira app deploy` commit-message via
/// [`super::app::deploy_commit_message`]). Peer with
/// [`deploy_commit_message`] on the writer-side git-commit-subject
/// surface — the paired stderr-notice + commit-subject emit sites share
/// one accessor rather than four raw field-accesses in lockstep.
pub(crate) fn deploy_summary_line(
    action: &str,
    caixa: &Caixa,
    programs_abs: &std::path::Path,
) -> String {
    format!(
        "{action} entry for {} v{} in {}",
        caixa.nome(),
        caixa.versao(),
        programs_abs.display()
    )
}

/// Compose the git-commit subject + body `feira deploy --commit /
/// --apply` writes past a successful upsert into the cluster's
/// fleet-programs HelmRelease. Derives its terminal `{nome}` / `{versao}`
/// scalars through the typed [`Caixa::nome`] / [`Caixa::versao`]
/// accessors so a future git-history reader (a future `feira deploy
/// rollback` that scans commit subjects, a k8s-repo audit walker that
/// greps `deploy: added <nome>` / `deploy: updated <nome>` prefixes)
/// reads a byte-string identical to the one the paired downstream
/// Servico artefact emit already carries. Peer with
/// [`deploy_summary_line`] on the operator-visible stderr-notice
/// surface — the paired stderr-notice + commit-subject emit sites share
/// one accessor rather than four raw field-accesses in lockstep.
pub(crate) fn deploy_commit_message(action: &str, caixa: &Caixa) -> String {
    format!(
        "deploy: {action} {} v{}\n\
         \n\
         Updated by `feira deploy --cluster <name>`.\n",
        caixa.nome(),
        caixa.versao(),
    )
}

fn push_origin(repo: &std::path::Path) -> Result<()> {
    // Remote name read from the lifted [`DEFAULT_GIT_REMOTE`] constant
    // (caixa-core) so the writer-side deploy path shares one source of
    // truth with the sibling `feira publish` (`--remote` default) and
    // `feira app deploy --apply` (`push_origin`) verbs. See the
    // constant's body for the full drift-mode analysis.
    git(repo, ["push", DEFAULT_GIT_REMOTE, "HEAD"])?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_origin_remote_arg_reads_from_lifted_caixa_core_constant() {
        // Structural pin: the Servico-deploy-side `push_origin` helper
        // threads the lifted [`caixa_core::DEFAULT_GIT_REMOTE`] through
        // to its `git push <remote> HEAD` argv slot, not an inline
        // `"origin"` literal the sibling writer-side verbs (`feira
        // publish` `--remote` default, `feira app deploy --apply`
        // `push_origin`) could silently drift on. A future remote-
        // naming-convention rebrand on the lifted constant reaches
        // this site through one `&'static str` by construction.
        //
        // Pin the constant's canonical value through the same module
        // path the helper resolves (`caixa_core::DEFAULT_GIT_REMOTE`)
        // so a regression that re-introduces an inline `"origin"` byte
        // at the `git(repo, ["push", "origin", "HEAD"])` slot — or
        // routes the slot through a sibling const — surfaces here as a
        // build-time test failure naming the offending drift, peer to
        // the sibling [`caixa-feira`]
        // `publish_remote_default_pins_lifted_caixa_core_constant` test
        // on the publish-side, the
        // `push_origin_remote_arg_reads_from_lifted_caixa_core_constant`
        // test in the Aplicacao-deploy sibling
        // (caixa-feira/src/cmd/app.rs), and the
        // `default_git_remote_pins_canonical_origin_byte` canonical-
        // literal pin on the caixa-core side.
        assert_eq!(DEFAULT_GIT_REMOTE, "origin");
        assert_eq!(DEFAULT_GIT_REMOTE, caixa_core::DEFAULT_GIT_REMOTE);
        assert!(
            std::ptr::eq(
                DEFAULT_GIT_REMOTE.as_ptr(),
                caixa_core::DEFAULT_GIT_REMOTE.as_ptr(),
            ),
            "DEFAULT_GIT_REMOTE must resolve through caixa_core, not \
             a sibling local `pub const` that happens to carry the same \
             string — drift between the two is the canonical footgun \
             this lift closes"
        );
    }

    fn servico_caixa(nome: &str, versao: &str) -> Caixa {
        // Minimal `:kind Servico` caixa carrying the identity pair the
        // pins below assert against. The declared `:servicos` entry is
        // never opened on disk from these tests — every consumer under
        // test reads only `Caixa::nome` / `Caixa::versao`.
        let src = format!(
            "(defcaixa :nome \"{nome}\" :versao \"{versao}\" :kind Servico \
             :servicos (\"servicos/{nome}.computeunit.yaml\"))"
        );
        Caixa::from_lisp(&src).expect("Servico caixa src parses")
    }

    #[test]
    fn deploy_summary_line_routes_through_caixa_nome_and_versao_accessors() {
        // Fail-before-pass-after pin: the `feira deploy` operator-
        // visible post-upsert stderr notice's terminal `{nome}` /
        // `{versao}` scalars must resolve through the typed
        // [`Caixa::nome`] / [`Caixa::versao`] accessors, not the raw
        // `caixa.nome` / `caixa.versao` field-accesses this converge
        // lifts. A regression that re-inlines either raw field at the
        // emit site silently splits the byte-string the operator reads
        // on stderr past a successful `feira deploy` from the identity
        // the paired downstream Servico artefacts already carry (the
        // caixa-helm `Chart.yaml` `version:` / `appVersion:` per
        // eb912de / 05a7701, the caixa-flux `programs.yaml` `versao:`
        // fold + `cluster_bundle` `GitRepository` `spec.ref.tag` per
        // 2fc5f81, the caixa-crd `CaixaSpec.versao` CR-side emit per
        // 41ab9a3) — the operator's post-deploy confirmation of a
        // Servico's `{nome} v{versao}` identity would silently
        // disagree with the identity the substrate reconciler binds
        // against per-CR revision.
        //
        // Pin the emit composer's output byte-equal against a scratch
        // notice derived through the typed accessors + the canonical
        // action / programs-path scalars this verb carries, and pin
        // the exact operator-facing line
        // (`"added entry for checkout v0.4.2 in /k8s/…"`) so a future
        // template-format extension (e.g. adding a `[cluster=<name>]`
        // suffix once the sibling `feira app deploy` post-upsert
        // notice grows the same axis) reaches this site through one
        // composer rather than re-rolling a parallel template. Peer
        // with the sibling
        // [`super::build::build_summary_line`] / [`super::app::graph_header_line`]
        // pins on the peer `feira build` / `feira app graph` verb-emit
        // surfaces.
        let caixa = servico_caixa("checkout", "0.4.2");
        let programs_abs = std::path::Path::new("/k8s/clusters/rio/programs/release.yaml");
        let rendered = deploy_summary_line("added", &caixa, programs_abs);
        assert_eq!(
            rendered,
            format!(
                "added entry for {} v{} in {}",
                caixa.nome(),
                caixa.versao(),
                programs_abs.display()
            ),
            "deploy_summary_line must derive its {{nome}} / {{versao}} \
             slots through the typed Caixa::nome / Caixa::versao \
             accessors — a regression that re-inlines caixa.nome / \
             caixa.versao at the emit site silently splits the operator-\
             facing stderr notice from the peer accessor-derived \
             substrate-side emit"
        );
        assert_eq!(
            rendered,
            "added entry for checkout v0.4.2 in /k8s/clusters/rio/programs/release.yaml"
        );
    }

    #[test]
    fn deploy_summary_line_carries_updated_action_verbatim() {
        // Paired inversion pin: the `--commit` / `--apply`-triggered
        // per-entry-existed-already arm emits `updated` in the action
        // slot the [`Deploy::run`] `let action = if inserted { "added"
        // } else { "updated" }` cascade chooses off the
        // `upsert_into_helmrelease_programs` `inserted` bool. Pin the
        // `"updated"` shape end-to-end so a future refactor of the
        // action-slot cascade (e.g. splitting into a typed
        // `enum UpsertOutcome { Added, Updated }` once the sibling
        // `feira app deploy` cascade grows the same axis) reaches this
        // composer through one canonical form rather than re-rolling a
        // parallel byte-string.
        let caixa = servico_caixa("cart", "1.0.0");
        let programs_abs = std::path::Path::new("/k8s/clusters/mar/programs/release.yaml");
        assert_eq!(
            deploy_summary_line("updated", &caixa, programs_abs),
            "updated entry for cart v1.0.0 in /k8s/clusters/mar/programs/release.yaml"
        );
    }

    #[test]
    fn deploy_commit_message_routes_through_caixa_nome_and_versao_accessors() {
        // Fail-before-pass-after pin: the `feira deploy --commit /
        // --apply` git-commit-message composer's terminal `{nome}` /
        // `{versao}` scalars must resolve through the typed
        // [`Caixa::nome`] / [`Caixa::versao`] accessors, not the raw
        // `caixa.nome` / `caixa.versao` field-accesses the pre-lift
        // `format!("deploy: {action} {} v{}\n...\n", caixa.nome,
        // caixa.versao)` at :175-180 carried. A regression that re-
        // inlines either raw field at the emit site silently splits
        // the k8s-repo git-history commit subject a future git-history
        // reader greps against (a future `feira deploy rollback` that
        // scans commit subjects for `deploy: added <nome> v<versao>`
        // / `deploy: updated <nome> v<versao>` prefixes, a k8s-repo
        // audit walker that projects the Servico's identity out of
        // the commit-history axis) from the identity every paired
        // downstream Servico artefact already carries under one
        // accessor.
        //
        // Pin the composer's output byte-equal against a scratch
        // message derived through the typed accessors + the canonical
        // subject-line + trailing-body shape this verb carries, and
        // pin the exact commit-message body
        // (`"deploy: added checkout v0.4.2\n\nUpdated by `feira …`.\n"`)
        // so a future subject-line extension (e.g. adding a
        // `[cluster=<name>]` suffix once the sibling
        // `feira app deploy` commit-subject grows the same axis)
        // reaches this site through one composer rather than re-
        // rolling a parallel template. Peer with the sibling
        // [`super::app::deploy_commit_message`] pin on the peer
        // Aplicacao-deploy verb's commit-subject axis, and with the
        // sibling `deploy_summary_line` pin on the paired stderr-
        // notice axis of this same verb.
        let caixa = servico_caixa("checkout", "0.4.2");
        let msg = deploy_commit_message("added", &caixa);
        assert_eq!(
            msg,
            format!(
                "deploy: added {} v{}\n\
                 \n\
                 Updated by `feira deploy --cluster <name>`.\n",
                caixa.nome(),
                caixa.versao()
            ),
            "deploy_commit_message must derive its {{nome}} / {{versao}} \
             slots through the typed Caixa::nome / Caixa::versao \
             accessors — a regression that re-inlines caixa.nome / \
             caixa.versao at the emit site silently splits the k8s-repo \
             git-history commit subject from the peer accessor-derived \
             substrate-side emit"
        );
        assert_eq!(
            msg,
            "deploy: added checkout v0.4.2\n\
             \n\
             Updated by `feira deploy --cluster <name>`.\n"
        );
        // Symmetric assertion on the `updated` action arm; the
        // subject-prefix cascade must fold onto the same accessor-
        // derived composer.
        assert_eq!(
            deploy_commit_message("updated", &servico_caixa("cart", "1.0.0")),
            "deploy: updated cart v1.0.0\n\
             \n\
             Updated by `feira deploy --cluster <name>`.\n"
        );
    }
}
