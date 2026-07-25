use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use caixa_core::{Caixa, CaixaKind, DEFAULT_GIT_REMOTE, WitTarget};
use clap::{Args, Subcommand};

use super::load::{caixa_root, load_caixa, validate_cluster_arg};

/// `feira app …` — composition verbs for `:kind Aplicacao` caixas.
///
/// V0 ships two subcommands:
///
///   feira app graph              — print the typed Aplicacao spec
///                                  (membros + contratos + politicas
///                                  + placement + entrada) for review
///   feira app deploy --cluster X — render every cluster artifact
///                                  (programs.yaml entries, Cilium
///                                  NetworkPolicies, Gateway/HTTPRoute)
///                                  and write to the cluster's GitOps
///                                  tree (with optional commit + push)
///
/// Mirrors the `feira deploy` shape used for individual Servicos —
/// same flags (--cluster, --dry-run, --commit, --apply), same
/// PLEME_K8S_REPO env override, same default path layout.
#[derive(Args)]
pub struct App {
    #[command(subcommand)]
    pub command: AppCommand,
}

#[derive(Subcommand)]
pub enum AppCommand {
    /// Print the validated typed graph for the Aplicacao in CWD.
    /// Useful for code review + cse-lint integration.
    Graph(GraphArgs),

    /// Render every cluster artifact for the Aplicacao + write to the
    /// k8s GitOps repo. With --apply, also git commit + push.
    Deploy(DeployArgs),
}

impl App {
    pub fn run(self) -> Result<()> {
        match self.command {
            AppCommand::Graph(c) => c.run(),
            AppCommand::Deploy(c) => c.run(),
        }
    }
}

// ── feira app graph ────────────────────────────────────────────────

#[derive(Args)]
pub struct GraphArgs {
    /// Caixa root (defaults to CWD).
    #[arg(long)]
    pub path: Option<PathBuf>,

    /// Output as JSON instead of human-readable.
    #[arg(long)]
    pub json: bool,
}

impl GraphArgs {
    pub fn run(self) -> Result<()> {
        let caixa = load_aplicacao(self.path.as_deref())?;
        let spec = caixa_mesh::typed_view(&caixa)?;
        if self.json {
            println!("{}", serde_json::to_string_pretty(&spec)?);
        } else {
            println!("{}", graph_header_line(&caixa));
            // Route the per-Aplicacao `:placement` printer's paired
            // `.estrategia()` + `.clusters()` reads through the lifted
            // [`caixa_core::AplicacaoSpec::placement`] outer accessor
            // rather than the raw `spec.placement` field access — the
            // `feira app graph` per-Aplicacao print line now reaches
            // exactly one typed dispatch on the substrate primitive at
            // the outer composition altitude before dispatching onto
            // the per-axis accessors, sibling to the peer per-`:membros`
            // / per-`:contratos` slice-return accessor consumers below.
            let placement = spec.placement();
            println!(
                "  placement: {:?} on clusters {:?}",
                placement.estrategia(),
                placement.clusters()
            );
            println!("  membros ({}):", spec.membros().len(),);
            for m in spec.membros() {
                println!("    - {} {}", m.nome(), m.versao_requirement());
            }
            println!("  contratos ({}):", spec.contratos().len());
            for c in spec.contratos() {
                // Typed view: each WIT shape has exactly one payload
                // field (validated upstream). The label tells the
                // reader *what* field they're looking at, not just
                // its value. The per-arm field-name prefix routes
                // through the peer `WitTarget::{HTTP,PUBSUB,STORE}_
                // FIELD_NAME` consts so a rename of the author-
                // surface field (`:endpoint` → `:path`, say) reaches
                // this printer by construction, not by manual sync.
                let label = match c.target().expect("validated by typed_view") {
                    WitTarget::Http { endpoint } => {
                        format!("{}={endpoint}", WitTarget::HTTP_FIELD_NAME)
                    }
                    WitTarget::PubSub { subject } => {
                        format!("{}={subject}", WitTarget::PUBSUB_FIELD_NAME)
                    }
                    WitTarget::Store { slot } => {
                        format!("{}={slot}", WitTarget::STORE_FIELD_NAME)
                    }
                    WitTarget::Capability => "(capability-only)".to_string(),
                };
                println!(
                    "    - {} → {}  via {}  [{}]",
                    c.source(),
                    c.destination(),
                    c.world_ref(),
                    label,
                );
            }
            // Route the per-`:entrada` composite-reference read
            // through the lifted [`caixa_core::AplicacaoSpec::entrada`]
            // accessor rather than the raw `&spec.entrada` field
            // access — the `feira app graph` per-Aplicacao external-
            // gateway summary line now keys off the canonical read-
            // side surface every per-Aplicacao entrada consumer
            // routes through.
            //
            // Every per-`:entrada` scalar-value + list-value axis this
            // summary line reads now routes through the typed accessor
            // on the substrate primitive (`hostname()` on the `:host`
            // DNS-hostname singular axis, `destination()` on the
            // `:para` member-Servico scalar axis, `paths()` on the
            // `:paths` `&[String]` slice axis, `port()` on the `:port`
            // `u16` L4 scalar axis) rather than the raw `e.host` /
            // `e.para` / `e.paths` / `e.port` field accesses — the
            // graph-verb print now shares the same read-side surface
            // every peer per-`:entrada` consumer (the
            // [`caixa_core::AplicacaoSpec::validate`] per-`:paths`
            // gate + `:port` structural-floor gate; the
            // `caixa-mesh` Gateway / HTTPRoute renderers'
            // per-listener / per-rule projections) routes through.
            if let Some(e) = spec.entrada() {
                println!(
                    "  entrada: {} → {} (paths={:?}, port={})",
                    e.hostname(),
                    e.destination(),
                    e.paths(),
                    e.port(),
                );
            } else {
                println!("  entrada: (internal-only mesh)");
            }
        }
        Ok(())
    }
}

// ── feira app deploy ───────────────────────────────────────────────

#[derive(Args)]
pub struct DeployArgs {
    /// Cluster name (e.g. `rio`, `mar`, `plo`). Selects the k8s tree
    /// path: `<k8s-repo>/clusters/<cluster>/aplicacaos/<nome>/`.
    #[arg(long)]
    pub cluster: String,

    /// Path to the GitOps k8s repo. Defaults to PLEME_K8S_REPO env
    /// var or `~/code/github/pleme-io/k8s` if neither is set.
    #[arg(long, env = "PLEME_K8S_REPO")]
    pub k8s_repo: Option<PathBuf>,

    /// Print the rendered manifests to stdout instead of writing.
    #[arg(long)]
    pub dry_run: bool,

    /// Auto-commit the change after writing (no push).
    #[arg(long, conflicts_with = "apply")]
    pub commit: bool,

    /// Auto-commit AND push to origin (full automation).
    #[arg(long)]
    pub apply: bool,

    /// Caixa root (defaults to CWD).
    #[arg(long)]
    pub path: Option<PathBuf>,
}

impl DeployArgs {
    pub fn run(self) -> Result<()> {
        // Validate the `--cluster` arg at the verb entry-point, before
        // any IO. The value lands as a path segment in
        // `<k8s-repo>/clusters/<cluster>/aplicacaos/<nome>/manifests.yaml`
        // and as a K8s `metadata.name` on the downstream per-Aplicacao
        // Gateway / HTTPRoute / CiliumNetworkPolicy resources; the
        // DNS-1123 label gate refuses every footgun the typed
        // `:placement :clusters` slot already refuses (empty,
        // path-traversal, uppercase, underscore, leading-`-`), peer with
        // the lifted `validate_placement_cluster` discipline on the
        // typed-slot axis and with the `feira deploy` per-Servico
        // verb's matching gate.
        validate_cluster_arg(&self.cluster)?;
        let caixa = load_aplicacao(self.path.as_deref())?;
        let docs = caixa_mesh::render_all(&caixa)?;

        let serialized = render_multidoc(caixa.nome(), &docs)?;

        if self.dry_run {
            print!("{serialized}");
            return Ok(());
        }

        let k8s_repo = self
            .k8s_repo
            .clone()
            .or_else(|| dirs::home_dir().map(|h| h.join("code/github/pleme-io/k8s")))
            .ok_or_else(|| anyhow::anyhow!("could not resolve k8s repo path"))?;

        let rel = PathBuf::from("clusters")
            .join(&self.cluster)
            .join("aplicacaos")
            .join(caixa.nome())
            .join("manifests.yaml");
        let abs = k8s_repo.join(&rel);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(&abs, &serialized).with_context(|| format!("writing {}", abs.display()))?;

        eprintln!("rendered {} → {}", caixa.nome(), abs.display());

        if self.commit || self.apply {
            commit_change(&k8s_repo, &rel, &caixa)?;
            eprintln!("committed change in {}", k8s_repo.display());
        }
        if self.apply {
            push_origin(&k8s_repo)?;
            eprintln!("pushed origin/main");
        } else if !self.commit {
            eprintln!(
                "review with: git -C {} diff -- {}",
                k8s_repo.display(),
                rel.display()
            );
        }
        Ok(())
    }
}

// ── helpers ─────────────────────────────────────────────────────────

/// Substrate-canonical `feira app graph` per-Aplicacao header line
/// composer — derives the `"Aplicacao {nome} v{versao}"` byte-string
/// every operator sees on stdout past a clean `feira app graph`
/// through the typed [`caixa_core::Caixa::nome`] /
/// [`caixa_core::Caixa::versao`] accessors rather than the raw
/// `.nome` / `.versao` field-accesses the inline emit at
/// [`GraphArgs::run`] used before this lift. Peer with every prior
/// per-substrate-side renderer converge on the outer-`Caixa` `:nome`
/// / `:versao` universal-axis accessors (caixa-helm 22461ef /
/// eb912de / 05a7701, caixa-flux 4a363bf / 162e2e2 / 2fc5f81,
/// caixa-mesh 54bf2f3 / 980c059, caixa-crd 61d3429 / 41ab9a3) and
/// with the sibling [`crate::cmd::build::build_summary_line`] (ef83332)
/// on the peer `feira build` verb — closes the analogous last
/// unlifted `feira app graph` header-line raw-field-access site in
/// caixa-feira on both axes at once (the emit site composes `:nome`
/// and `:versao` in the same `println!` template, so converging one
/// without the other would leave a mixed-mode emit).
pub(crate) fn graph_header_line(caixa: &Caixa) -> String {
    format!("Aplicacao {} v{}", caixa.nome(), caixa.versao())
}

/// Substrate-canonical `feira app deploy --commit` / `--apply`
/// per-Aplicacao git-commit-message composer — derives the
/// `"deploy: aplicacao {nome} v{versao}\n\nUpdated by `feira app
/// deploy --cluster <name>`.\n"` byte-string every downstream git
/// history reader sees on the writer-side deploy path through the
/// typed [`caixa_core::Caixa::nome`] / [`caixa_core::Caixa::versao`]
/// accessors rather than the raw `.nome` / `.versao` field-accesses
/// the inline `format!` at [`commit_change`] used before this lift.
/// Peer with the sibling [`graph_header_line`] above (same lift, same
/// verb, sibling emit surface) and with the analogous `feira deploy`
/// per-Servico writer-side commit-message emit at
/// [`crate::cmd::deploy::commit_change`]'s `caixa.nome, caixa.versao`
/// field-accesses (a future peer converge folds on this template's
/// shape).
pub(crate) fn deploy_commit_message(caixa: &Caixa) -> String {
    format!(
        "deploy: aplicacao {} v{}\n\nUpdated by `feira app deploy --cluster <name>`.\n",
        caixa.nome(),
        caixa.versao(),
    )
}

fn load_aplicacao(path: Option<&std::path::Path>) -> Result<Caixa> {
    let root = caixa_root(path);
    let caixa = load_caixa(&root)?;
    // Route the `:kind` gate through the lifted typed-view predicate so
    // the diagnostic names the offending `caixa.lisp` verbatim through
    // the cause chain — peer with the canonical entry-point shape every
    // per-kind renderer (`caixa-helm`, `caixa-flux`, `caixa-mesh`)
    // already uses (c4213a4). Before the lift this feira verb raised
    // `feira app: caixa :kind must be Aplicacao for app verbs, got
    // Biblioteca` — the operator had to grep their source tree for
    // which caixa.lisp triggered it; after the lift the wrapped
    // [`caixa_core::KindMismatch`] view carries the `:nome` so the
    // anyhow Debug renderer (the `Result<()>` main contract uses) prints
    // it in the cause chain. Exactly the "feira verb whose error path
    // doesn't name the offending caixa" punch-list item the compounding
    // mandate calls out.
    caixa_core::require_kind(&caixa, CaixaKind::Aplicacao)
        .with_context(|| "feira app verbs require :kind Aplicacao")?;
    Ok(caixa)
}

fn render_multidoc(nome: &str, docs: &[serde_yaml::Value]) -> Result<String> {
    let header = format!(
        "# Auto-generated by `feira app deploy` from caixa.lisp ({nome}).\n\
         # One file per Aplicacao; multi-doc YAML separated by `---`.\n\
         # Edits are overwritten on next `feira app deploy`.\n",
    );
    let mut out = String::from(&header);
    for d in docs {
        out.push_str("---\n");
        let s = serde_yaml::to_string(d)?;
        out.push_str(&s);
    }
    Ok(out)
}

fn commit_change(repo: &std::path::Path, rel: &std::path::Path, caixa: &Caixa) -> Result<()> {
    let msg = deploy_commit_message(caixa);
    git(repo, ["add", &rel.display().to_string()])?;
    git(repo, ["commit", "-m", &msg])?;
    Ok(())
}

fn push_origin(repo: &std::path::Path) -> Result<()> {
    // Remote name read from the lifted [`DEFAULT_GIT_REMOTE`] constant
    // (caixa-core) so the writer-side Aplicacao-deploy path shares one
    // source of truth with the sibling `feira publish` (`--remote`
    // default) and `feira deploy --apply` (`push_origin`) verbs. See
    // the constant's body for the full drift-mode analysis.
    git(repo, ["push", DEFAULT_GIT_REMOTE, "HEAD"])
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
    use tempfile::tempdir;

    fn write_caixa(dir: &std::path::Path, src: &str) {
        std::fs::write(dir.join("caixa.lisp"), src).expect("write caixa.lisp");
    }

    #[test]
    fn load_aplicacao_accepts_aplicacao_kind() {
        // The canonical accept case — an Aplicacao-kind caixa parses
        // and the typed-view kind gate routes through with no
        // diagnostic. Peer with the canonical entry-point
        // `require_kind` happy-path the per-kind renderers
        // (`caixa-helm` / `caixa-flux` / `caixa-mesh`) already pin.
        let dir = tempdir().expect("tempdir");
        write_caixa(
            dir.path(),
            r#"(defcaixa
                 :nome "checkout"
                 :kind Aplicacao
                 :versao "0.1.0"
                 :membros ())"#,
        );
        let caixa = load_aplicacao(Some(dir.path())).expect("Aplicacao must load");
        assert_eq!(caixa.nome(), "checkout");
        assert_eq!(caixa.kind(), CaixaKind::Aplicacao);
    }

    #[test]
    fn load_aplicacao_rejects_non_aplicacao_kind_with_named_caixa() {
        // The load-bearing property the lift closes: a wrong-kind
        // caixa surfaces a diagnostic whose anyhow cause chain *names
        // the offending caixa* (`mis-kinded`), not just the rejected
        // kind. Before the lift this verb raised `feira app: caixa
        // :kind must be Aplicacao for app verbs, got Biblioteca` —
        // the operator had to grep their source tree for which
        // caixa.lisp triggered it. After the lift the wrapped
        // [`caixa_core::KindMismatch`] view carries the `:nome` and
        // the anyhow Debug renderer (the `Result<()>` main contract
        // uses) prints it in the cause chain. Pins the rendered
        // diagnostic shape so a future refactor can't silently
        // regress the `:nome`-naming property.
        let dir = tempdir().expect("tempdir");
        write_caixa(
            dir.path(),
            r#"(defcaixa
                 :nome "mis-kinded"
                 :kind Biblioteca
                 :versao "0.1.0"
                 :bibliotecas ())"#,
        );
        let err = load_aplicacao(Some(dir.path())).expect_err("Biblioteca must reject");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("mis-kinded"),
            "diagnostic must name the offending caixa nome (got: {rendered:?})"
        );
        assert!(
            rendered.contains("Aplicacao"),
            "diagnostic must name the expected kind (got: {rendered:?})"
        );
        assert!(
            rendered.contains("Biblioteca"),
            "diagnostic must name the actual kind (got: {rendered:?})"
        );
        assert!(
            rendered.contains("feira app"),
            "diagnostic must keep the feira-app context the prior bail \
             prefix carried (got: {rendered:?})"
        );
    }

    #[test]
    fn push_origin_remote_arg_reads_from_lifted_caixa_core_constant() {
        // Structural pin: the deploy-side `push_origin` helper threads
        // the lifted [`caixa_core::DEFAULT_GIT_REMOTE`] through to its
        // `git push <remote> HEAD` argv slot, not an inline `"origin"`
        // literal the sibling writer-side verbs (`feira publish` `--remote`
        // default, `feira deploy --apply` `push_origin`) could silently
        // drift on. A future remote-naming-convention rebrand on the
        // lifted constant reaches this site through one `&'static str`
        // by construction.
        //
        // Pin the constant's canonical value through the same module
        // path the helper resolves (`caixa_core::DEFAULT_GIT_REMOTE`)
        // so a regression that re-introduces an inline `"origin"` byte
        // at the `git(repo, ["push", "origin", "HEAD"])` slot — or
        // routes the slot through a sibling const — surfaces here as a
        // build-time test failure naming the offending drift, peer to
        // the sibling [`caixa-feira`]
        // `publish_remote_default_pins_lifted_caixa_core_constant` test
        // on the publish-side and to the
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

    #[test]
    fn graph_header_line_routes_through_caixa_nome_and_versao_accessors() {
        // Emit-path pin: the `feira app graph` per-Aplicacao header
        // line composer's terminal `{nome}` / `{versao}` scalars must
        // derive through the typed [`caixa_core::Caixa::nome`] /
        // [`caixa_core::Caixa::versao`] accessors byte-for-byte.
        // Before this converge the emit site at [`GraphArgs::run`]
        // carried raw `caixa.nome` / `caixa.versao` field-accesses
        // into the inline `println!("Aplicacao {} v{}", ...)`
        // template, bypassing the typed dispatch every peer
        // substrate-side renderer's `:nome` / `:versao` emit-site
        // already routes through.
        //
        // Byte-equal today (both accessors are `&self.<field>`); the
        // pin catches any future accessor extension (SemVer-2 build-
        // metadata canonicalization the CAIXA-SDLC §I SemVer-2 pin
        // acknowledges, per-edition pre-release-tag overlay dispatched
        // through the sibling [`caixa_core::Caixa::edicao`]
        // universal-axis scalar) whose `feira app graph` header line
        // regresses to the raw field and silently splits the byte-
        // string the operator reads on stdout from the paired
        // downstream Aplicacao artefact emit (the [`caixa_mesh`]
        // programs.yaml fan-out `versao:` fold + the
        // `CiliumNetworkPolicy` `metadata.name` `:nome` projection)
        // that already routes through the accessor. Peer with the
        // sibling [`crate::cmd::build::build_summary_line`] (ef83332)
        // drift pin on the `feira build` verb-emit surface.
        let caixa = Caixa::from_lisp(
            r#"(defcaixa
                 :nome "checkout"
                 :kind Aplicacao
                 :versao "0.2.3"
                 :membros ())"#,
        )
        .expect("parse");
        let line = graph_header_line(&caixa);
        assert_eq!(
            line,
            format!("Aplicacao {} v{}", caixa.nome(), caixa.versao()),
            "graph header line must route the `{{nome}}` / `{{versao}}` \
             scalars through the typed Caixa::nome / Caixa::versao \
             accessors — any regression to the raw `caixa.nome` / \
             `caixa.versao` field-accesses would silently pass today \
             (accessors are `&self.<field>`) but drift on the first \
             accessor extension"
        );
        assert_eq!(
            line, "Aplicacao checkout v0.2.3",
            "graph header line must carry the Aplicacao's `:nome` / \
             `:versao` verbatim (got: {line:?})"
        );
    }

    #[test]
    fn deploy_commit_message_routes_through_caixa_nome_and_versao_accessors() {
        // Emit-path pin: the `feira app deploy --commit / --apply`
        // per-Aplicacao git-commit-message composer's terminal
        // `{nome}` / `{versao}` scalars must derive through the typed
        // [`caixa_core::Caixa::nome`] / [`caixa_core::Caixa::versao`]
        // accessors byte-for-byte. Before this converge the emit site
        // at [`commit_change`] carried raw `caixa.nome` /
        // `caixa.versao` field-accesses into the inline `format!(
        // "deploy: aplicacao {} v{}\n...", ...)` template, bypassing
        // the typed dispatch every peer substrate-side renderer's
        // `:nome` / `:versao` emit-site already routes through.
        //
        // Byte-equal today (both accessors are `&self.<field>`); the
        // pin catches any future accessor extension whose `feira app
        // deploy` writer-side commit-message regresses to the raw
        // field and silently splits the git-history byte-string every
        // downstream reader (a future `feira app rollback` verb that
        // scans commit subjects to locate the last deploy per
        // Aplicacao, a k8s-repo audit walker that greps
        // `deploy: aplicacao <nome>` prefixes) sees from the paired
        // downstream Aplicacao artefact emit that already routes
        // through the accessor. Peer with the sibling
        // [`graph_header_line`] drift pin above on the same
        // `feira app` verb and with the sibling
        // [`crate::cmd::build::build_summary_line`] (ef83332) drift
        // pin on the peer `feira build` verb-emit surface.
        let caixa = Caixa::from_lisp(
            r#"(defcaixa
                 :nome "checkout"
                 :kind Aplicacao
                 :versao "1.4.7"
                 :membros ())"#,
        )
        .expect("parse");
        let msg = deploy_commit_message(&caixa);
        assert_eq!(
            msg,
            format!(
                "deploy: aplicacao {} v{}\n\nUpdated by `feira app deploy --cluster <name>`.\n",
                caixa.nome(),
                caixa.versao(),
            ),
            "commit message must route the `{{nome}}` / `{{versao}}` \
             scalars through the typed Caixa::nome / Caixa::versao \
             accessors — any regression to the raw `caixa.nome` / \
             `caixa.versao` field-accesses would silently pass today \
             (accessors are `&self.<field>`) but drift on the first \
             accessor extension"
        );
        assert!(
            msg.starts_with("deploy: aplicacao checkout v1.4.7\n"),
            "commit message subject line must carry the Aplicacao's \
             `:nome` / `:versao` verbatim (got: {msg:?})"
        );
        assert!(
            msg.ends_with("Updated by `feira app deploy --cluster <name>`.\n"),
            "commit message body must carry the canonical trailing \
             sentence naming the verb that produced it (got: {msg:?})"
        );
    }

    #[test]
    fn load_aplicacao_rejection_carries_typed_kind_mismatch_view() {
        // Peer to the [`KindMismatch`]-named-caixa pin above: the
        // anyhow error chain's underlying source downcasts to
        // [`caixa_core::KindMismatch`], so a future caller that
        // inspects the chain (e.g. a structured-output mode for
        // `feira app graph --json`) reads the typed view's
        // `nome` / `expected` / `actual` slots verbatim, peer with
        // the `caixa-helm` / `caixa-flux` / `caixa-mesh`
        // `#[from] KindMismatch` test families. Pins that the
        // `.with_context(...)` wrap preserves the typed source —
        // anyhow keeps the original `KindMismatch` reachable via
        // `Error::chain`, so the typed payload isn't lost behind
        // the string-only context message.
        let dir = tempdir().expect("tempdir");
        write_caixa(
            dir.path(),
            r#"(defcaixa
                 :nome "wrong-shape"
                 :kind Servico
                 :versao "0.1.0"
                 :servicos ("servicos/wrong-shape.computeunit.yaml"))"#,
        );
        let err = load_aplicacao(Some(dir.path())).expect_err("Servico must reject");
        let km = err
            .chain()
            .find_map(|e| e.downcast_ref::<caixa_core::KindMismatch>())
            .expect("KindMismatch must be reachable through the anyhow chain");
        assert_eq!(km.nome, "wrong-shape");
        assert_eq!(km.expected, CaixaKind::Aplicacao);
        assert_eq!(km.actual, CaixaKind::Servico);
    }
}
