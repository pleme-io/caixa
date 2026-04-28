use anyhow::Result;
use clap::Subcommand;

pub mod add;
pub mod build;
pub mod chart;
pub mod deploy;
pub mod fmt;
pub mod init;
pub mod lint;
pub mod lock;
pub mod nix;
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
    /// Emit a flake.nix for the caixa.
    Nix(nix::Nix),
    /// Render a per-program lareira-<name> Helm chart (via caixa-helm).
    Chart(chart::Chart),
    /// Deploy a caixa Servico to a target cluster (upserts the entry into
    /// k8s/clusters/<cluster>/programs.yaml; FluxCD picks it up).
    Deploy(deploy::Deploy),
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
            Self::Nix(c) => c.run(),
            Self::Chart(c) => c.run(),
            Self::Deploy(c) => c.run(),
            Self::Publish(c) => c.run(),
            Self::Tofu(c) => c.run(),
        }
    }
}
