//! `feira ephemeral …` — verbs for `(defephemeral …)` Lisp forms.
//!
//! v0 ships two subcommands:
//!
//!   feira ephemeral graph <form.lisp>
//!     — Compile the form, lower to ProcessSpec, print as YAML for
//!       review. Pure-data; no cluster access. Use this as the
//!       "code-review" surface for ephemeral env declarations.
//!
//!   feira ephemeral plan <form.lisp> [--out path]
//!     — Same as graph, but write the resulting Process YAML to a file
//!       (or stdout if --out is `-`). The output is exactly what
//!       `kubectl apply -f` would consume. Stays NO SHELL — the apply
//!       step is the operator's call.
//!
//! Future scope (deferred — needs in-cluster kube-rs setup decisions):
//!   feira ephemeral up      — compile + kube-rs apply
//!   feira ephemeral down    — kube-rs delete by name
//!   feira ephemeral list    — kube-rs list Processes with lifetime.ephemeral

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::{Args, Subcommand};

use tatara_process::ephemeral::compile_ephemeral_source;
use tatara_process::prelude::{Process, ProcessSpec};

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
    /// Compile a (defephemeral …) form and write the Process YAML to a file
    /// (or stdout when --out is `-`). The output is `kubectl apply`-ready.
    Plan(PlanArgs),
}

impl Ephemeral {
    pub fn run(self) -> Result<()> {
        match self.command {
            EphemeralCommand::Graph(c) => c.run(),
            EphemeralCommand::Plan(c) => c.run(),
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

/// Pure pipeline: file → (defephemeral …) → EphemeralSpec → ProcessSpec → Process CR.
/// No cluster access. The caller decides whether to apply.
fn lower_file(path: &Path, namespace: &str) -> Result<Vec<Process>> {
    let src = fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
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
        (defephemeral akeyless-closed-loop-attest
          :aplicacao (:chart-ref "oci://ghcr.io/pleme-io/charts/lareira-akeyless-deployment"
                      :version "0.5.5"
                      :profile "gateway-with-internal-saas"
                      :values-overlay (:cluster (:name "ephemeral-test-01")
                                       :data (:mysql (:persistence (:enabled #f)))))
          :ttl "1h"
          :teardown OnAttested
          :postconditions
            ((:kind HelmReleaseReleased
              :params (:name "akeyless-saas" :namespace "akeyless-test"))
             (:kind ClosedLoopAuth
              :params (:issuer (:service "gator" :port 8080)
                       :consumer (:service "gateway" :port 8000)
                       :probeImage "ghcr.io/pleme-io/closed-loop-probe:0.1.0"))))
    "#;

    #[test]
    fn lower_file_produces_named_process() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ephemeral.lisp");
        std::fs::write(&path, SAMPLE).unwrap();
        let processes = lower_file(&path, "akeyless-test").unwrap();
        assert_eq!(processes.len(), 1);
        assert_eq!(processes[0].metadata.name.as_deref(), Some("akeyless-closed-loop-attest"));
        assert_eq!(
            processes[0].metadata.namespace.as_deref(),
            Some("akeyless-test")
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
