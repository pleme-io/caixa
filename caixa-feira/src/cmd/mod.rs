use anyhow::Result;
use clap::Subcommand;

/// Substrate-canonical Kubernetes server-side-apply field-manager
/// identifier every `feira` verb that writes to the cluster stamps its
/// `PatchParams::apply(…)` call with.
///
/// Kubernetes SSA tracks per-manager ownership of every field it accepts —
/// two verbs sharing one manager id let the apiserver treat their writes
/// as one logical caller (the "feira" agent), while a drift between verbs
/// (`"feira"` at one site, `"feira-pool"` at another, `"caixa-feira"` at a
/// third) would split ownership: each verb's writes would race, and every
/// re-apply through a peer would surface as a
/// `metadata.managedFields[]` conflict the operator watches for far from
/// the drift's source. The paired sibling `caixa-operator` reconciler
/// picks its own `"caixa-operator"` manager id on the operator-side
/// (see [`caixa_operator::reconciler`] `PatchParams::apply("caixa-operator")`
/// call sites at `caixa-operator/src/reconciler.rs`) — the two managers
/// deliberately stay distinct so the CLI's writes and the reconciler's
/// writes are attributable in `kubectl get <cr> -o yaml`'s
/// `managedFields` block.
///
/// Prior to this lift the bare `"feira"` byte lived inline at two writer
/// sites — [`crate::cmd::ephemeral_runtime::apply_process`]'s
/// `PatchParams::apply("feira").force()` and [`crate::cmd::pool::PoolApplyArgs::run`]'s
/// symmetric `PatchParams::apply("feira").force()` — with no compile-time
/// link back to a shared source of truth. Every future feira-side writer
/// verb (a landed `allocation apply`, an M4 `feira app apply`, any
/// future CR-emitting verb the substrate grows) that reached for its own
/// inline `"feira"` byte would either duplicate the string (three inline
/// copies to walk before a rename lands coherently) or silently drift to
/// a different manager id, splitting SSA ownership at the apiserver.
/// Lifting the identifier as a typed `pub const &'static str` on the
/// `cmd` module root means every feira-side writer routes through one
/// canonical byte-string by construction, and a future rebrand (a
/// `"feira-v2"` migration, a per-verb namespaced convention) lands as one
/// caixa-feira edit rather than a coordinated N-file rewrite. Peer with
/// the [`caixa_core::DEFAULT_GIT_REMOTE`] / [`caixa_core::DEFAULT_PUBLISH_TAG_PREFIX`]
/// substrate-canonical `&'static str` lifts on the sibling caixa-core-
/// exposed writer-side scalar-value axes — same "one typed dispatch on
/// the substrate primitive, thin projections at each consumer"
/// discipline extended onto the per-feira SSA field-manager identifier
/// axis. Pinned by
/// [`tests::feira_field_manager_pins_canonical_field_manager_string`].
pub const FEIRA_FIELD_MANAGER: &str = "feira";

pub mod add;
pub mod allocation;
pub mod app;
pub mod build;
pub mod chart;
pub mod deploy;
pub mod dialeto;
pub mod ephemeral;
pub mod ephemeral_runtime;
pub mod fmt;
pub mod init;
pub mod lint;
pub mod load;
pub mod lock;
pub mod nix;
pub mod pool;
pub mod publish;
pub mod resolve;
pub mod tofu;

#[derive(Subcommand)]
pub enum Command {
    /// Scaffold a new caixa in a new or existing (empty) directory.
    Init(init::Init),
    /// Add a dependency to the caixa.lisp in CWD.
    Add(add::Add),
    /// Resolve deps + write lacre.lisp (git-cloning sources, transitive).
    Resolve(resolve::Resolve),
    /// Legacy: stub-resolve deps (no git). Kept as an escape hatch — in
    /// phase 1.B the default `feira lock` delegates to `feira resolve`.
    Lock(lock::Lock),
    /// Validate caixa.lisp + layout + every :bibliotecas entry parses.
    Build(build::Build),
    /// Format a caixa.lisp / lacre.lisp / .lisp source in place (via caixa-fmt).
    Fmt(fmt::Fmt),
    /// Lint a caixa.lisp source (via caixa-lint, Nord-themed output).
    Lint(lint::Lint),
    /// Classify every caixa manifest in a tree by dialect, and fail on an
    /// unclassifiable or wrong-dialect manifest. Default-on, hard.
    Dialeto(dialeto::Dialeto),
    /// Emit a flake.nix for the caixa.
    Nix(nix::Nix),
    /// Render a per-program lareira-<name> Helm chart (via caixa-helm).
    Chart(chart::Chart),
    /// Deploy a caixa Servico to a target cluster (upserts the entry into
    /// k8s/clusters/<cluster>/programs.yaml; FluxCD picks it up).
    Deploy(deploy::Deploy),
    /// `feira app …` — composition verbs for `:kind Aplicacao` caixas
    /// (graph, deploy). See `theory/MESH-COMPOSITION.md`.
    App(app::App),
    /// `feira ephemeral …` — verbs for `(defephemeral …)` Lisp forms:
    /// compile + lower to a Process CR YAML for review or apply.
    Ephemeral(ephemeral::Ephemeral),
    /// `feira pool …` — manage EphemeralPool CRs (list/scale/delete/apply/status).
    Pool(pool::Pool),
    /// `feira allocation …` — manage EphemeralAllocation CRs (request/release/list/status).
    Allocation(allocation::Allocation),
    /// Tag + push the current caixa's versao to its Git origin.
    Publish(publish::Publish),
    /// End-to-end: Lisp → teia → arch proof → HCL → tofu plan/apply/destroy.
    Tofu(tofu::Tofu),
}

impl Command {
    pub fn run(self) -> Result<()> {
        match self {
            Self::Init(c) => c.run(),
            Self::Add(c) => c.run(),
            Self::Resolve(c) => c.run(),
            Self::Lock(c) => c.run(),
            Self::Build(c) => c.run(),
            Self::Fmt(c) => c.run(),
            Self::Lint(c) => c.run(),
            Self::Dialeto(c) => c.run(),
            Self::Nix(c) => c.run(),
            Self::Chart(c) => c.run(),
            Self::Deploy(c) => c.run(),
            Self::App(c) => c.run(),
            Self::Ephemeral(c) => c.run(),
            Self::Pool(c) => c.run(),
            Self::Allocation(c) => c.run(),
            Self::Publish(c) => c.run(),
            Self::Tofu(c) => c.run(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kube::api::PatchParams;

    #[test]
    fn feira_field_manager_pins_canonical_field_manager_string() {
        // Fail-before-pass-after pin: the substrate-canonical writer-side
        // SSA field-manager id must resolve to the byte-string "feira",
        // the same manager id every feira-side `PatchParams::apply(…)`
        // call stamps onto its writes today (both
        // [`crate::cmd::ephemeral_runtime::apply_process`] and
        // [`crate::cmd::pool::PoolApplyArgs::run`]). Drift to any other
        // byte-string would silently split SSA ownership at the
        // apiserver — a re-apply through a peer verb using the drifted
        // manager would surface as a `metadata.managedFields[]`
        // conflict the operator watches for far from the drift's
        // source. Peer to the sibling [`caixa_core::DEFAULT_GIT_REMOTE`] /
        // [`caixa_core::DEFAULT_PUBLISH_TAG_PREFIX`] pins on the
        // writer-side scalar-value axes (see
        // `publish_prefix_default_pins_lifted_caixa_core_constant` /
        // `publish_remote_default_pins_lifted_caixa_core_constant` for
        // the same fail-before-pass-after discipline on the peer
        // publish-verb clap-default axes).
        assert_eq!(
            FEIRA_FIELD_MANAGER, "feira",
            "FEIRA_FIELD_MANAGER must resolve to the canonical \"feira\" \
             byte-string — a rebrand must land at one caixa-feira edit \
             on the const, not by re-inlining a different byte at any \
             writer-side `PatchParams::apply(…)` call"
        );
    }

    #[test]
    fn feira_field_manager_flows_through_patchparams_apply() {
        // Cross-crate composition pin: the substrate-canonical writer-
        // side SSA field-manager `pub const` composes through
        // `kube::api::PatchParams::apply(…)`'s `field_manager` axis
        // byte-for-byte identical to the inline `PatchParams::apply(
        // "feira")` construction the peer feira-side writer sites
        // (`ephemeral_runtime::apply_process`, `pool::PoolApplyArgs::run`)
        // consume — so the const-routed construction the two consumer
        // sites now reach through resolves to the same `PatchParams`
        // shape the substrate operator's SSA-ownership tracking watches
        // for. A future `kube-rs` internal change to how
        // `PatchParams::apply` folds its `field_manager` argument
        // (e.g. a normalization pass, a per-manager alias table) that
        // silently reshaped the const-routed byte at the call site
        // would surface here as a build-time test failure rather than
        // as a runtime SSA-ownership conflict far from the caixa-feira
        // commit. Peer of the sibling
        // `feira_field_manager_pins_canonical_field_manager_string`
        // pin above on the const's byte-string axis — this pin closes
        // the cross-crate composition axis.
        let via_const = PatchParams::apply(FEIRA_FIELD_MANAGER);
        let via_literal = PatchParams::apply("feira");
        assert_eq!(
            via_const.field_manager.as_deref(),
            Some("feira"),
            "PatchParams::apply(FEIRA_FIELD_MANAGER) must produce \
             field_manager = Some(\"feira\")"
        );
        assert_eq!(
            via_const.field_manager, via_literal.field_manager,
            "PatchParams::apply(FEIRA_FIELD_MANAGER) must produce the \
             same field_manager as PatchParams::apply(\"feira\") — the \
             const-routed construction and the inline construction are \
             byte-for-byte equivalent at the SSA-ownership axis"
        );
    }
}
