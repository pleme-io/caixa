//! `feira ephemeral …` — verbs for `(defephemeral …)` Lisp forms.
//!
//! Five subcommands:
//!
//!   feira ephemeral graph <form.lisp>
//!     — Compile the form, lower to ProcessSpec, print as YAML for
//!       review. Pure-data; no cluster access.
//!
//!   feira ephemeral plan <form.lisp> [--out path]
//!     — Same as graph, but write the resulting Process YAML to a file.
//!       The output is `kubectl apply -f`-ready.
//!
//!   feira ephemeral up <form.lisp> [--wait] [--timeout]
//!     — Compile + apply via kube-rs (server-side apply). Optionally
//!       block until the Process reaches `Attested`. CI-friendly:
//!       returns 0 on attestation, non-zero on timeout/failure.
//!
//!   feira ephemeral down <name> [--namespace]
//!     — Delete the Process by name via kube-rs. Owner-ref cascade
//!       reaps every emitted Flux resource.
//!
//!   feira ephemeral status <name> [--namespace]
//!     — Print phase + boundary conditions + attestation root for the
//!       named Process.
//!
//!   feira ephemeral list [--namespace] [--all]
//!     — Table of ephemeral Processes (or every Process when --all).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::{Args, Subcommand};

use tatara_process::ephemeral::compile_ephemeral_source;
use tatara_process::phase::ProcessPhase;
use tatara_process::prelude::{Process, ProcessSpec};

use super::ephemeral_runtime as runtime;

/// `feira ephemeral …` — verbs for `(defephemeral …)` forms.
#[derive(Args)]
pub struct Ephemeral {
    #[command(subcommand)]
    pub command: EphemeralCommand,
}

#[derive(Subcommand)]
pub enum EphemeralCommand {
    /// Compile a (defephemeral …) form and print the lowered Process YAML.
    Graph(GraphArgs),
    /// Compile a (defephemeral …) form and write the Process YAML to a file.
    Plan(PlanArgs),
    /// Compile + apply (server-side) the Process; optionally wait for Attested.
    Up(UpArgs),
    /// Delete a named Process; owner-refs cascade-reap the chart.
    Down(DownArgs),
    /// Print phase + conditions + attestation root for the named Process.
    Status(StatusArgs),
    /// List ephemeral Processes in a namespace (or every Process when --all).
    List(ListArgs),
}

impl Ephemeral {
    pub fn run(self) -> Result<()> {
        match self.command {
            EphemeralCommand::Graph(c) => c.run(),
            EphemeralCommand::Plan(c) => c.run(),
            EphemeralCommand::Up(c) => c.run(),
            EphemeralCommand::Down(c) => c.run(),
            EphemeralCommand::Status(c) => c.run(),
            EphemeralCommand::List(c) => c.run(),
        }
    }
}

#[derive(Args)]
pub struct GraphArgs {
    /// Path to a `.lisp` file containing one or more `(defephemeral …)` forms.
    pub path: PathBuf,
    /// Namespace stamped on the rendered Process. Defaults to "default".
    #[arg(long, default_value = "default")]
    pub namespace: String,
}

impl GraphArgs {
    pub fn run(self) -> Result<()> {
        super::load::validate_namespace_arg(&self.namespace)?;
        let processes = lower_file(&self.path, &self.namespace)?;
        for p in &processes {
            println!("---");
            print!("{}", serde_yaml::to_string(p).context("serialize Process")?);
        }
        Ok(())
    }
}

#[derive(Args)]
pub struct PlanArgs {
    /// Path to a `.lisp` file containing one or more `(defephemeral …)` forms.
    pub path: PathBuf,
    /// Output file path. Use `-` for stdout.
    #[arg(long, default_value = "-")]
    pub out: String,
    /// Namespace stamped on the rendered Process. Defaults to "default".
    #[arg(long, default_value = "default")]
    pub namespace: String,
}

impl PlanArgs {
    pub fn run(self) -> Result<()> {
        super::load::validate_namespace_arg(&self.namespace)?;
        let processes = lower_file(&self.path, &self.namespace)?;
        let mut buf = String::new();
        for p in &processes {
            buf.push_str("---\n");
            buf.push_str(&serde_yaml::to_string(p).context("serialize Process")?);
        }
        if self.out == "-" {
            print!("{buf}");
        } else {
            fs::write(&self.out, &buf)
                .with_context(|| format!("write Process YAML to {}", self.out))?;
            eprintln!(
                "wrote {} Process manifest(s) to {}",
                processes.len(),
                self.out
            );
        }
        Ok(())
    }
}

// ── cluster-touching subcommands ──────────────────────────────────────

#[derive(Args)]
pub struct UpArgs {
    /// Path to a `.lisp` file containing one or more `(defephemeral …)` forms.
    pub path: PathBuf,
    /// Namespace to apply into. Defaults to "default".
    #[arg(long, default_value = "default")]
    pub namespace: String,
    /// Block until the Process reaches `Attested`.
    #[arg(long)]
    pub wait: bool,
    /// Max wait duration (humantime: "10m", "1h"). Default 10m.
    #[arg(long, default_value = "10m")]
    pub timeout: String,
    /// Poll interval while waiting (humantime). Default 5s.
    #[arg(long, default_value = "5s")]
    pub poll: String,
}

impl UpArgs {
    pub fn run(self) -> Result<()> {
        super::load::validate_namespace_arg(&self.namespace)?;
        let processes = lower_file(&self.path, &self.namespace)?;
        let timeout = humantime::parse_duration(&self.timeout)
            .with_context(|| format!("--timeout {} is not a humantime duration", self.timeout))?;
        let poll = humantime::parse_duration(&self.poll)
            .with_context(|| format!("--poll {} is not a humantime duration", self.poll))?;
        run_async(async move {
            let client = runtime::client().await?;
            for p in &processes {
                let applied = runtime::apply_process(client.clone(), p).await?;
                let name = applied.metadata.name.as_deref().unwrap_or("?");
                eprintln!("applied Process {}/{name}", self.namespace);
            }
            if self.wait {
                for p in &processes {
                    let name = p
                        .metadata
                        .name
                        .clone()
                        .ok_or_else(|| anyhow!("Process has no metadata.name"))?;
                    eprintln!(
                        "waiting for {}/{name} → Attested (timeout {:?}, poll {:?})",
                        self.namespace, timeout, poll
                    );
                    let final_proc = runtime::wait_for_phase(
                        client.clone(),
                        &self.namespace,
                        &name,
                        ProcessPhase::Attested,
                        timeout,
                        poll,
                    )
                    .await?;
                    print_status_summary(&final_proc);
                }
            }
            Ok::<_, anyhow::Error>(())
        })
    }
}

#[derive(Args)]
pub struct DownArgs {
    /// Process name to delete.
    pub name: String,
    /// Namespace. Defaults to "default".
    #[arg(long, default_value = "default")]
    pub namespace: String,
}

impl DownArgs {
    pub fn run(self) -> Result<()> {
        super::load::validate_namespace_arg(&self.namespace)?;
        run_async(async move {
            let client = runtime::client().await?;
            let deleted = runtime::delete_process(client, &self.namespace, &self.name).await?;
            if deleted {
                eprintln!(
                    "deleted Process {}/{} (owner refs cascade-reap the chart)",
                    self.namespace, self.name
                );
            } else {
                eprintln!(
                    "Process {}/{} not found (already deleted)",
                    self.namespace, self.name
                );
            }
            Ok::<_, anyhow::Error>(())
        })
    }
}

#[derive(Args)]
pub struct StatusArgs {
    /// Process name.
    pub name: String,
    /// Namespace. Defaults to "default".
    #[arg(long, default_value = "default")]
    pub namespace: String,
}

impl StatusArgs {
    pub fn run(self) -> Result<()> {
        super::load::validate_namespace_arg(&self.namespace)?;
        run_async(async move {
            let client = runtime::client().await?;
            let process = runtime::get_process(client, &self.namespace, &self.name)
                .await?
                .ok_or_else(|| anyhow!("Process {}/{} not found", self.namespace, self.name))?;
            print_status_summary(&process);
            Ok::<_, anyhow::Error>(())
        })
    }
}

#[derive(Args)]
pub struct ListArgs {
    /// Namespace. Defaults to "default".
    #[arg(long, default_value = "default")]
    pub namespace: String,
    /// Show every Process (default: only ephemeral-lifetime).
    #[arg(long)]
    pub all: bool,
}

impl ListArgs {
    pub fn run(self) -> Result<()> {
        super::load::validate_namespace_arg(&self.namespace)?;
        run_async(async move {
            let client = runtime::client().await?;
            let processes = runtime::list_processes(client, &self.namespace, !self.all).await?;
            if processes.is_empty() {
                eprintln!(
                    "no {}Processes in {}",
                    if self.all { "" } else { "ephemeral " },
                    self.namespace
                );
                return Ok::<_, anyhow::Error>(());
            }
            println!(
                "{:<40} {:<14} {:<10} {}",
                "NAME", "PHASE", "LIFETIME", "AGE"
            );
            for p in processes {
                let name = p.metadata.name.as_deref().unwrap_or("?");
                let phase = p
                    .status
                    .as_ref()
                    .map(|s| format!("{:?}", s.phase))
                    .unwrap_or_else(|| "Pending".into());
                let lifetime = if p.spec.lifetime.is_ephemeral() {
                    "ephemeral"
                } else {
                    "permanent"
                };
                let age = p
                    .metadata
                    .creation_timestamp
                    .as_ref()
                    .map(|t| short_age(t.0))
                    .unwrap_or_else(|| "?".into());
                println!("{:<40} {:<14} {:<10} {}", name, phase, lifetime, age);
            }
            Ok::<_, anyhow::Error>(())
        })
    }
}

// ── helpers ───────────────────────────────────────────────────────────

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

fn short_age(creation: chrono::DateTime<chrono::Utc>) -> String {
    let elapsed = chrono::Utc::now().signed_duration_since(creation);
    let secs = elapsed.num_seconds().max(0) as u64;
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d{}h", secs / 86400, (secs % 86400) / 3600)
    }
}

fn print_status_summary(p: &Process) {
    let ns = p.metadata.namespace.as_deref().unwrap_or("?");
    let name = p.metadata.name.as_deref().unwrap_or("?");
    let status = p.status.as_ref();
    let phase = status
        .map(|s| format!("{:?}", s.phase))
        .unwrap_or_else(|| "Pending".into());
    let pid = status
        .and_then(|s| s.pid.clone())
        .unwrap_or_else(|| "(unassigned)".into());
    println!("Process {ns}/{name}");
    println!("  phase: {phase}");
    println!("  pid:   {pid}");
    if let Some(att) = status.and_then(|s| s.attestation.as_ref()) {
        println!("  attestation generation: {}", att.generation);
        println!("  composed_root: {}", att.composed_root);
    }
    if let Some(s) = status
        && !s.boundary.postconditions.is_empty()
    {
        println!("  postconditions:");
        for c in &s.boundary.postconditions {
            let mark = if c.satisfied { "✓" } else { "✗" };
            let msg = c.message.as_deref().unwrap_or("");
            println!("    {mark} {:?}  {msg}", c.condition.kind);
        }
    }
}

/// Pure pipeline: file → (defephemeral …) → EphemeralSpec → ProcessSpec → Process CR.
/// No cluster access. The caller decides whether to apply.
fn lower_file(path: &Path, namespace: &str) -> Result<Vec<Process>> {
    let src = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let defs = compile_ephemeral_source(&src)
        .map_err(|e| anyhow!("compile (defephemeral …) form: {e}"))?;
    if defs.is_empty() {
        return Err(anyhow!(
            "no (defephemeral …) forms found in {}",
            path.display()
        ));
    }
    let mut processes = Vec::with_capacity(defs.len());
    for d in defs {
        let spec: ProcessSpec = d.spec.into();
        let mut process = Process::new(&d.name, spec);
        process.metadata.namespace = Some(namespace.to_string());
        processes.push(process);
    }
    Ok(processes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const SAMPLE: &str = r#"
        (defephemeral example-closed-loop-attest
          :aplicacao (:chart-ref "oci://ghcr.io/pleme-io/charts/lareira-example-deployment"
                      :version "0.5.5"
                      :profile "gateway-with-internal-db"
                      :values-overlay (:cluster (:name "ephemeral-test-01")
                                       :data (:mysql (:persistence (:enabled #f)))))
          :ttl "1h"
          :teardown OnAttested
          :postconditions
            ((:kind HelmReleaseReleased
              :params (:name "example-release" :namespace "example-ephemeral"))
             (:kind ClosedLoopAuth
              :params (:issuer (:service "service-a" :port 8080)
                       :consumer (:service "service-b" :port 8000)
                       :probeImage "ghcr.io/pleme-io/closed-loop-probe:0.1.0"))))
    "#;

    #[test]
    fn lower_file_produces_named_process() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ephemeral.lisp");
        std::fs::write(&path, SAMPLE).unwrap();
        let processes = lower_file(&path, "example-ephemeral").unwrap();
        assert_eq!(processes.len(), 1);
        assert_eq!(
            processes[0].metadata.name.as_deref(),
            Some("example-closed-loop-attest")
        );
        assert_eq!(
            processes[0].metadata.namespace.as_deref(),
            Some("example-ephemeral")
        );
        // Sanity: intent.aplicacao landed.
        assert!(processes[0].spec.intent.aplicacao.is_some());
        // Sanity: lifetime is ephemeral.
        assert!(processes[0].spec.lifetime.is_ephemeral());
        // Sanity: postconditions landed.
        assert_eq!(processes[0].spec.boundary.postconditions.len(), 2);
    }

    #[test]
    fn empty_file_errors() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("empty.lisp");
        std::fs::write(&path, ";; just a comment").unwrap();
        let err = lower_file(&path, "default").unwrap_err();
        assert!(err.to_string().contains("no (defephemeral"));
    }
}
