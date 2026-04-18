//! `feira tofu` — the end-to-end pipeline.
//!
//! ```
//!   caixa.lisp + lib/*.lisp
//!       │  tatara-lisp parse
//!       ▼
//!   TeiaManifest     ← (defteia …) forms collected
//!       │  caixa-arch invariants
//!       ▼
//!   ArchReport       ← refuses to emit HCL on any Safety violation
//!       │  caixa-pangea render
//!       ▼
//!   main.tf.json     ← Terraform JSON config
//!       │  subprocess
//!       ▼
//!   tofu init / plan / apply / destroy
//! ```

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
use caixa_arch::{ArchVerdict, check_manifest};
use caixa_pangea::{ProviderBlock, RequiredProvider, TofuConfig, emit_tf_json};
use caixa_teia::{TeiaManifest, parse_teia_source};
use caixa_theme::{Semantic, Theme};
use clap::{Args, Subcommand};

/// End-to-end Lisp → teia → arch proof → HCL → tofu.
#[derive(Args)]
pub struct Tofu {
    #[command(subcommand)]
    pub command: TofuCmd,
}

#[derive(Subcommand)]
pub enum TofuCmd {
    /// Run everything and emit `main.tf.json` — does not invoke `tofu`.
    Render(RenderOpts),
    /// Emit `main.tf.json` and run `tofu init && tofu plan`.
    Plan(RunOpts),
    /// Emit + `tofu apply` (interactive).
    Apply(RunOpts),
    /// Emit + `tofu destroy`.
    Destroy(RunOpts),
}

#[derive(Args)]
pub struct RenderOpts {
    /// caixa root (defaults to CWD).
    #[arg(long)]
    pub path: Option<PathBuf>,
    /// Out directory for generated Terraform files. Defaults to `.caixa/tofu`.
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Provider block — `name=source@version` (repeatable).
    #[arg(long = "provider")]
    pub providers: Vec<String>,
    /// Provider config JSON (one per `--provider`, matched by order).
    #[arg(long = "provider-config")]
    pub provider_configs: Vec<String>,
    /// Refuse to emit if *any* invariant (even hints) fails — strict mode.
    #[arg(long)]
    pub strict: bool,
}

#[derive(Args)]
pub struct RunOpts {
    #[command(flatten)]
    pub render: RenderOpts,
    /// Path to the `tofu` binary (defaults to `tofu` on PATH, falls back to `terraform`).
    #[arg(long)]
    pub bin: Option<String>,
    /// Extra args to pass through to tofu.
    #[arg(last = true)]
    pub passthrough: Vec<String>,
}

impl Tofu {
    pub fn run(self) -> Result<()> {
        match self.command {
            TofuCmd::Render(o) => render(&o).map(|_| ()),
            TofuCmd::Plan(o) => run_subcmd("plan", &o),
            TofuCmd::Apply(o) => run_subcmd("apply", &o),
            TofuCmd::Destroy(o) => run_subcmd("destroy", &o),
        }
    }
}

fn render(opts: &RenderOpts) -> Result<(PathBuf, TeiaManifest)> {
    let root = opts.path.clone().unwrap_or_else(|| PathBuf::from("."));
    let out_dir = opts
        .out
        .clone()
        .unwrap_or_else(|| root.join(".caixa").join("tofu"));
    let theme = Theme::blackmatter_dark();

    // 1) Parse every .lisp source under lib/ (plus lib/*.lisp + caixa.lisp itself).
    let manifest = collect_manifest(&root)?;
    eprintln!(
        "{} parsed {} (defteia …) instance(s) across {}",
        theme.paint(Semantic::Info, "feira tofu:"),
        manifest.instances.len(),
        root.display()
    );

    // 2) Prove via caixa-arch.
    let report = check_manifest(&manifest, &[]);
    for v in &report.violations {
        let sev = match v.kind {
            caixa_arch::InvariantKind::Safety => Semantic::Error,
            caixa_arch::InvariantKind::Compliance => Semantic::Warning,
            caixa_arch::InvariantKind::Hint => Semantic::Hint,
        };
        eprintln!(
            "{}  [{}] {}/{}: {}",
            theme.paint(sev, &format!("{:?}", v.kind).to_lowercase()),
            v.invariant_id,
            v.instance_tipo,
            v.instance_nome,
            v.message
        );
    }
    eprintln!(
        "{} {}",
        theme.paint(Semantic::Muted, "arch:"),
        report.summary
    );

    let strict_blocks = opts.strict && !report.violations.is_empty();
    if report.verdict == ArchVerdict::Rejected || strict_blocks {
        bail!(
            "arch verdict: Rejected ({} safety violation(s))",
            report.safety_count()
        );
    }

    // 3) Build TofuConfig from --provider flags.
    let mut cfg = TofuConfig::default();
    for (i, p) in opts.providers.iter().enumerate() {
        let (name, source_version) = p
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("--provider must be name=source@version"))?;
        let (source, version) = source_version
            .split_once('@')
            .ok_or_else(|| anyhow::anyhow!("--provider must be name=source@version"))?;
        cfg.required_providers.push(RequiredProvider {
            name: name.to_string(),
            source: source.to_string(),
            version: version.to_string(),
        });
        let raw_cfg = opts.provider_configs.get(i).cloned().unwrap_or("{}".into());
        let parsed: serde_json::Value = serde_json::from_str(&raw_cfg)
            .with_context(|| format!("--provider-config #{i} is not JSON"))?;
        cfg.providers.push(ProviderBlock {
            name: name.to_string(),
            config: parsed,
        });
    }

    // 4) Emit main.tf.json.
    let json = emit_tf_json(&manifest, &cfg);
    std::fs::create_dir_all(&out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let out_file = out_dir.join("main.tf.json");
    let pretty = serde_json::to_string_pretty(&json)? + "\n";
    std::fs::write(&out_file, &pretty)
        .with_context(|| format!("writing {}", out_file.display()))?;
    eprintln!(
        "{} {}",
        theme.paint(Semantic::Info, "wrote"),
        out_file.display()
    );

    Ok((out_dir, manifest))
}

fn run_subcmd(sub: &str, opts: &RunOpts) -> Result<()> {
    let (out_dir, _) = render(&opts.render)?;
    let bin = opts.bin.clone().unwrap_or_else(|| "tofu".to_string());
    let theme = Theme::blackmatter_dark();

    if sub == "plan" || !already_initialized(&out_dir) {
        eprintln!("{} {} init", theme.paint(Semantic::Info, "▶"), bin);
        let init = Command::new(&bin)
            .arg("init")
            .arg("-input=false")
            .current_dir(&out_dir)
            .status();
        match init {
            Ok(s) if s.success() => {}
            Ok(s) => bail!("{} init exited with {}", bin, s),
            Err(e) => {
                if opts.bin.is_none() {
                    eprintln!(
                        "{} tofu not found; falling back to terraform",
                        theme.paint(Semantic::Warning, "!")
                    );
                    return retry_with_terraform(sub, opts, &out_dir);
                }
                bail!("invoking {}: {e}", bin);
            }
        }
    }

    let mut args = vec![sub.to_string()];
    args.extend(opts.passthrough.iter().cloned());
    let status = Command::new(&bin)
        .args(&args)
        .current_dir(&out_dir)
        .status()
        .with_context(|| format!("invoking {bin} {}", args.join(" ")))?;
    if !status.success() {
        bail!("{} {} exited with {status}", bin, sub);
    }
    Ok(())
}

fn retry_with_terraform(sub: &str, opts: &RunOpts, out_dir: &std::path::Path) -> Result<()> {
    let status = Command::new("terraform")
        .args(["init", "-input=false"])
        .current_dir(out_dir)
        .status()?;
    if !status.success() {
        bail!("terraform init failed");
    }
    let mut args = vec![sub.to_string()];
    args.extend(opts.passthrough.iter().cloned());
    let s = Command::new("terraform")
        .args(&args)
        .current_dir(out_dir)
        .status()?;
    if !s.success() {
        bail!("terraform {sub} exited with {s}");
    }
    Ok(())
}

fn already_initialized(dir: &std::path::Path) -> bool {
    dir.join(".terraform").exists() || dir.join(".terraform.lock.hcl").exists()
}

fn collect_manifest(root: &std::path::Path) -> Result<TeiaManifest> {
    let mut combined = String::new();
    let lib = root.join("lib");
    if lib.is_dir() {
        for entry in std::fs::read_dir(&lib)? {
            let p = entry?.path();
            if p.extension().is_some_and(|e| e == "lisp") {
                combined.push_str(&std::fs::read_to_string(&p)?);
                combined.push('\n');
            }
        }
    }
    let infra = root.join("infra");
    if infra.is_dir() {
        for entry in std::fs::read_dir(&infra)? {
            let p = entry?.path();
            if p.extension().is_some_and(|e| e == "lisp") {
                combined.push_str(&std::fs::read_to_string(&p)?);
                combined.push('\n');
            }
        }
    }
    if combined.is_empty() {
        bail!("no .lisp sources found under lib/ or infra/ — nothing to compile");
    }
    let manifest = parse_teia_source(&combined).context("parsing teia sources")?;
    Ok(manifest)
}
