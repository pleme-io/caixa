//! `feira allocation …` — operator UX for `EphemeralAllocation` CRs.

use anyhow::{anyhow, Context, Result};
use clap::{Args, Subcommand};
use kube::api::{Api, DeleteParams, ListParams, PostParams};
use kube::{Client, Resource};
use tatara_process::allocation::{AllocationSpec, EphemeralAllocation, Requestor};
use tatara_process::pool::AllocationRef;

/// `feira allocation …` — manage EphemeralAllocation CRs.
#[derive(Args)]
pub struct Allocation {
    #[command(subcommand)]
    pub command: AllocationCommand,
}

#[derive(Subcommand)]
pub enum AllocationCommand {
    /// Request an allocation (typed request → reconciler binds to a Pool member).
    Request(RequestArgs),
    /// Release (delete) an allocation; the pool member is returned per policy.
    Release(ReleaseArgs),
    /// List allocations in a namespace.
    List(ListArgs),
    /// Show one allocation's status (phase, bound pool, assigned Process).
    Status(StatusArgs),
}

impl Allocation {
    pub fn run(self) -> Result<()> {
        match self.command {
            AllocationCommand::Request(c) => c.run(),
            AllocationCommand::Release(c) => c.run(),
            AllocationCommand::List(c) => c.run(),
            AllocationCommand::Status(c) => c.run(),
        }
    }
}

#[derive(Args)]
pub struct RequestArgs {
    /// Allocation name (used as the CR's metadata.name).
    pub name: String,
    /// Namespace.
    #[arg(long, default_value = "default")]
    pub namespace: String,
    /// Requestor kind discriminator (e.g., "manual", "github-pr").
    #[arg(long, default_value = "manual")]
    pub kind: String,
    /// Optional repo identifier.
    #[arg(long)]
    pub repo: Option<String>,
    /// Optional branch identifier.
    #[arg(long)]
    pub branch: Option<String>,
    /// Optional PR number (for kind=github-pr).
    #[arg(long)]
    pub pr_number: Option<u64>,
    /// Optional commit SHA.
    #[arg(long)]
    pub sha: Option<String>,
    /// PR labels (comma-separated).
    #[arg(long, value_delimiter = ',')]
    pub labels: Vec<String>,
    /// Free-form actor.
    #[arg(long)]
    pub actor: Option<String>,
    /// Pin to a specific pool (skips selector routing).
    #[arg(long)]
    pub pool: Option<String>,
    /// TTL override (humantime — `30m`, `2h`).
    #[arg(long)]
    pub ttl: Option<String>,
    /// Operator note.
    #[arg(long)]
    pub note: Option<String>,
}

impl RequestArgs {
    pub fn run(self) -> Result<()> {
        super::load::validate_namespace_arg(&self.namespace)?;
        let pool_ref = self.pool.clone().map(|name| AllocationRef {
            name,
            namespace: self.namespace.clone(),
        });
        let spec = AllocationSpec {
            pool_ref,
            requestor: Requestor {
                kind: self.kind,
                repo: self.repo,
                branch: self.branch,
                pr_number: self.pr_number,
                sha: self.sha,
                pr_labels: self.labels,
                actor: self.actor,
            },
            ttl: self.ttl,
            note: self.note,
        };
        let mut alloc = EphemeralAllocation::new(&self.name, spec);
        alloc.meta_mut().namespace = Some(self.namespace.clone());
        let name = self.name.clone();
        let namespace = self.namespace.clone();
        run_async(async move {
            let client = client().await?;
            let api: Api<EphemeralAllocation> = Api::namespaced(client, &namespace);
            api.create(&PostParams::default(), &alloc)
                .await
                .with_context(|| format!("create Allocation {namespace}/{name}"))?;
            eprintln!(
                "requested Allocation {namespace}/{name} \
                 — reconciler will bind to a Pool member"
            );
            Ok::<_, anyhow::Error>(())
        })
    }
}

#[derive(Args)]
pub struct ReleaseArgs {
    pub name: String,
    #[arg(long, default_value = "default")]
    pub namespace: String,
}

impl ReleaseArgs {
    pub fn run(self) -> Result<()> {
        super::load::validate_namespace_arg(&self.namespace)?;
        run_async(async move {
            let client = client().await?;
            let api: Api<EphemeralAllocation> = Api::namespaced(client, &self.namespace);
            match api.delete(&self.name, &DeleteParams::default()).await {
                Ok(_) => eprintln!(
                    "released Allocation {}/{} — pool reconciler returns the member per pool's policy",
                    self.namespace, self.name
                ),
                Err(kube::Error::Api(e)) if e.code == 404 => {
                    eprintln!("Allocation {}/{} not found", self.namespace, self.name);
                }
                Err(e) => return Err(anyhow!("delete: {e}")),
            }
            Ok::<_, anyhow::Error>(())
        })
    }
}

#[derive(Args)]
pub struct ListArgs {
    #[arg(long, default_value = "default")]
    pub namespace: String,
}

impl ListArgs {
    pub fn run(self) -> Result<()> {
        super::load::validate_namespace_arg(&self.namespace)?;
        run_async(async move {
            let client = client().await?;
            let api: Api<EphemeralAllocation> = Api::namespaced(client, &self.namespace);
            let list = api.list(&ListParams::default()).await?;
            if list.items.is_empty() {
                eprintln!("no allocations in {}", self.namespace);
                return Ok::<_, anyhow::Error>(());
            }
            println!(
                "{:<28} {:<14} {:<24} {:<14} {}",
                "NAME", "PHASE", "POOL", "KIND", "REQUESTOR"
            );
            for a in list.items {
                let name = a.metadata.name.as_deref().unwrap_or("?");
                let phase = a
                    .status
                    .as_ref()
                    .map(|s| format!("{:?}", s.phase))
                    .unwrap_or_else(|| "Pending".into());
                let pool = a
                    .status
                    .as_ref()
                    .and_then(|s| s.bound_pool.as_ref())
                    .map(|p| p.name.as_str())
                    .unwrap_or("-");
                let kind = a.spec.requestor.kind.as_str();
                let actor = a
                    .spec
                    .requestor
                    .actor
                    .as_deref()
                    .or(a.spec.requestor.repo.as_deref())
                    .unwrap_or("-");
                println!("{name:<28} {phase:<14} {pool:<24} {kind:<14} {actor}");
            }
            Ok(())
        })
    }
}

#[derive(Args)]
pub struct StatusArgs {
    pub name: String,
    #[arg(long, default_value = "default")]
    pub namespace: String,
}

impl StatusArgs {
    pub fn run(self) -> Result<()> {
        super::load::validate_namespace_arg(&self.namespace)?;
        run_async(async move {
            let client = client().await?;
            let api: Api<EphemeralAllocation> = Api::namespaced(client, &self.namespace);
            let a = api
                .get_opt(&self.name)
                .await?
                .ok_or_else(|| anyhow!("Allocation {}/{} not found", self.namespace, self.name))?;
            println!("Allocation {}/{}", self.namespace, self.name);
            println!("  requestor.kind: {}", a.spec.requestor.kind);
            if let Some(repo) = &a.spec.requestor.repo {
                println!("  requestor.repo: {repo}");
            }
            if let Some(branch) = &a.spec.requestor.branch {
                println!("  requestor.branch: {branch}");
            }
            if let Some(pr) = a.spec.requestor.pr_number {
                println!("  requestor.prNumber: {pr}");
            }
            if let Some(s) = a.status {
                println!("  phase: {:?}", s.phase);
                if let Some(p) = &s.bound_pool {
                    println!("  boundPool: {}/{}", p.namespace, p.name);
                }
                if let Some(p) = &s.assigned_process {
                    println!("  assignedProcess: {}/{}", p.namespace, p.name);
                }
                if let Some(at) = s.allocated_at {
                    println!("  allocatedAt: {at}");
                }
                if let Some(at) = s.expires_at {
                    println!("  expiresAt:   {at}");
                }
                if let Some(m) = s.message {
                    println!("  message: {m}");
                }
            }
            Ok::<_, anyhow::Error>(())
        })
    }
}

async fn client() -> Result<Client> {
    Client::try_default()
        .await
        .context("kube client (need KUBECONFIG or in-cluster auth)")
}

fn run_async<F, T>(fut: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    runtime.block_on(fut)
}

