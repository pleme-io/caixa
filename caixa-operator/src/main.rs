//! `caixa-operator` — Kubernetes controller for Caixa / Lacre / CaixaBuild.
//!
//! Responsibilities:
//!   - Watch `Caixa` CRs; for each, resolve its Git source + transitive
//!     deps via `caixa-resolver`, write a companion `Lacre` CR with the
//!     BLAKE3 closure root.
//!   - Watch `CaixaBuild` CRs; spawn a short-lived Job that runs `feira
//!     build`, collect the artifact digests, report success/failure in
//!     status + events.
//!   - Emit standard K8s conditions (Ready/Resolving/Failed) so downstream
//!     tooling (FluxCD, Argo, custom dashboards) picks them up cleanly.
//!
//! Transport:
//!   - Binary reads `KUBECONFIG` / in-cluster config via `kube::Client`.
//!   - stdout is a tracing subscriber (JSON in prod via `--log=json`).

use anyhow::Result;
use clap::Parser;
use kube::Client;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

mod reconciler;

#[derive(Parser)]
#[command(name = "caixa-operator", version)]
struct Args {
    /// Namespace to watch (empty = cluster-scoped).
    #[arg(long, env = "CAIXA_WATCH_NAMESPACE", default_value = "")]
    namespace: String,

    /// Log format — `text` (default, human) or `json` (production).
    #[arg(long, default_value = "text")]
    log: String,

    /// Run one reconcile loop only + exit — useful for kubectl-style testing.
    #[arg(long)]
    oneshot: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    init_tracing(&args.log);
    info!(
        version = env!("CARGO_PKG_VERSION"),
        "caixa-operator starting"
    );

    let client = Client::try_default().await?;
    reconciler::run(client, &args.namespace, args.oneshot).await?;
    Ok(())
}

fn init_tracing(fmt_mode: &str) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,kube=warn"));
    let registry = tracing_subscriber::registry().with(filter);
    if fmt_mode == "json" {
        registry.with(fmt::layer().json()).init();
    } else {
        registry.with(fmt::layer().compact()).init();
    }
}
