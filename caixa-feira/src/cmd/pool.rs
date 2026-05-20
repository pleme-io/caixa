//! `feira pool …` — operator UX for `EphemeralPool` CRs.

use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use clap::{Args, Subcommand};
use kube::api::{Api, DeleteParams, ListParams, Patch, PatchParams};
use kube::Client;
use serde::Deserialize;
use serde_json::json;

use tatara_process::pool::EphemeralPool;

/// `feira pool …` — manage EphemeralPool CRs.
#[derive(Args)]
pub struct Pool {
    #[command(subcommand)]
    pub command: PoolCommand,
}

#[derive(Subcommand)]
pub enum PoolCommand {
    /// List pools in a namespace.
    List(PoolListArgs),
    /// Scale a pool's desiredSize.
    Scale(PoolScaleArgs),
    /// Delete a pool by name (cascade-deletes all members via ownerRefs).
    Delete(PoolDeleteArgs),
    /// Apply a Pool manifest YAML.
    Apply(PoolApplyArgs),
    /// Show a pool's status (phase + member counts + per-slot ledger).
    Status(PoolStatusArgs),
}

impl Pool {
    pub fn run(self) -> Result<()> {
        match self.command {
            PoolCommand::List(c) => c.run(),
            PoolCommand::Scale(c) => c.run(),
            PoolCommand::Delete(c) => c.run(),
            PoolCommand::Apply(c) => c.run(),
            PoolCommand::Status(c) => c.run(),
        }
    }
}

#[derive(Args)]
pub struct PoolListArgs {
    #[arg(long, default_value = "default")]
    pub namespace: String,
}

impl PoolListArgs {
    pub fn run(self) -> Result<()> {
        run_async(async move {
            let client = client().await?;
            let api: Api<EphemeralPool> = Api::namespaced(client, &self.namespace);
            let list = api.list(&ListParams::default()).await?;
            if list.items.is_empty() {
                eprintln!("no pools in {}", self.namespace);
                return Ok::<_, anyhow::Error>(());
            }
            println!(
                "{:<32} {:<10} {:<10} {:<10} {:<14}",
                "NAME", "DESIRED", "READY", "ALLOC", "PHASE"
            );
            for p in list.items {
                let name = p.metadata.name.as_deref().unwrap_or("?");
                let desired = p.spec.desired_size;
                let (ready, allocated, phase) = if let Some(s) = &p.status {
                    (s.ready_count, s.allocated_count, format!("{:?}", s.phase))
                } else {
                    (0, 0, "Initializing".into())
                };
                println!("{name:<32} {desired:<10} {ready:<10} {allocated:<10} {phase:<14}");
            }
            Ok(())
        })
    }
}

#[derive(Args)]
pub struct PoolScaleArgs {
    pub name: String,
    pub desired_size: u32,
    #[arg(long, default_value = "default")]
    pub namespace: String,
}

impl PoolScaleArgs {
    pub fn run(self) -> Result<()> {
        run_async(async move {
            let client = client().await?;
            let api: Api<EphemeralPool> = Api::namespaced(client, &self.namespace);
            let patch = json!({ "spec": { "desiredSize": self.desired_size } });
            api.patch(
                &self.name,
                &PatchParams::default(),
                &Patch::Merge(&patch),
            )
            .await
            .with_context(|| format!("scale Pool {}/{}", self.namespace, self.name))?;
            eprintln!(
                "scaled Pool {}/{} → desiredSize={}",
                self.namespace, self.name, self.desired_size
            );
            Ok::<_, anyhow::Error>(())
        })
    }
}

#[derive(Args)]
pub struct PoolDeleteArgs {
    pub name: String,
    #[arg(long, default_value = "default")]
    pub namespace: String,
}

impl PoolDeleteArgs {
    pub fn run(self) -> Result<()> {
        run_async(async move {
            let client = client().await?;
            let api: Api<EphemeralPool> = Api::namespaced(client, &self.namespace);
            match api.delete(&self.name, &DeleteParams::default()).await {
                Ok(_) => {
                    eprintln!(
                        "deleted Pool {}/{} (members cascade-reap via ownerRefs)",
                        self.namespace, self.name
                    );
                }
                Err(kube::Error::Api(e)) if e.code == 404 => {
                    eprintln!("Pool {}/{} not found", self.namespace, self.name);
                }
                Err(e) => return Err(anyhow!("delete failed: {e}")),
            }
            Ok::<_, anyhow::Error>(())
        })
    }
}

#[derive(Args)]
pub struct PoolApplyArgs {
    /// YAML file containing one or more EphemeralPool docs.
    pub path: PathBuf,
}

impl PoolApplyArgs {
    pub fn run(self) -> Result<()> {
        let yaml = std::fs::read_to_string(&self.path)
            .with_context(|| format!("read {}", self.path.display()))?;
        let docs = parse_pool_docs(&yaml)?;
        if docs.is_empty() {
            return Err(anyhow!("no EphemeralPool docs in {}", self.path.display()));
        }
        run_async(async move {
            let client = client().await?;
            for pool in docs {
                let ns = pool
                    .metadata
                    .namespace
                    .clone()
                    .unwrap_or_else(|| "default".into());
                let name = pool
                    .metadata
                    .name
                    .clone()
                    .ok_or_else(|| anyhow!("Pool has no metadata.name"))?;
                let api: Api<EphemeralPool> = Api::namespaced(client.clone(), &ns);
                api.patch(
                    &name,
                    &PatchParams::apply("feira").force(),
                    &Patch::Apply(&pool),
                )
                .await
                .with_context(|| format!("apply Pool {ns}/{name}"))?;
                eprintln!("applied Pool {ns}/{name}");
            }
            Ok::<_, anyhow::Error>(())
        })
    }
}

#[derive(Args)]
pub struct PoolStatusArgs {
    pub name: String,
    #[arg(long, default_value = "default")]
    pub namespace: String,
}

impl PoolStatusArgs {
    pub fn run(self) -> Result<()> {
        run_async(async move {
            let client = client().await?;
            let api: Api<EphemeralPool> = Api::namespaced(client, &self.namespace);
            let pool = api
                .get_opt(&self.name)
                .await?
                .ok_or_else(|| anyhow!("Pool {}/{} not found", self.namespace, self.name))?;
            println!("Pool {}/{}", self.namespace, self.name);
            println!("  desiredSize: {}", pool.spec.desired_size);
            println!("  minSize:     {}", pool.spec.min_size);
            println!("  maxSize:     {}", pool.spec.max_size);
            println!("  returnPolicy: {:?}", pool.spec.return_policy);
            if let Some(s) = pool.status {
                println!("  phase:       {:?}", s.phase);
                println!("  ready:       {}", s.ready_count);
                println!("  allocated:   {}", s.allocated_count);
                println!("  spawning:    {}", s.spawning_count);
                println!("  returning:   {}", s.returning_count);
                if !s.members.is_empty() {
                    println!("  members:");
                    for m in &s.members {
                        println!("    {:<36} {:?}", m.process_name, m.state);
                    }
                }
            }
            Ok::<_, anyhow::Error>(())
        })
    }
}

fn parse_pool_docs(yaml: &str) -> Result<Vec<EphemeralPool>> {
    let mut out = Vec::new();
    for doc in serde_yaml::Deserializer::from_str(yaml) {
        match EphemeralPool::deserialize(doc) {
            Ok(p) => out.push(p),
            Err(e) => {
                if e.to_string().contains("missing field") || e.to_string().contains("EOF") {
                    continue;
                }
                return Err(anyhow!("parse EphemeralPool: {e}"));
            }
        }
    }
    Ok(out)
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

