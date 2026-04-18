//! `feira` — CLI for the caixa tatara-lisp package system.
//!
//! The name (Portuguese for *market/fair*) mirrors Rust's `cargo`: the
//! marketplace where caixas are authored, resolved, published, and consumed.
//!
//! Phase 1 scope:
//!   - `feira init <nome>`  — scaffold a new caixa
//!   - `feira add <nome>`   — append a dep to caixa.lisp
//!   - `feira lock`         — resolve deps + write lacre.lisp
//!   - `feira build`        — validate layout + parse every `:bibliotecas`
//!   - `feira nix`          — emit a flake.nix for the caixa

use anyhow::Result;
use clap::Parser;

mod cmd;

#[derive(Parser)]
#[command(
    name = "feira",
    version,
    about = "caixa package CLI — the tatara-lisp marketplace",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: cmd::Command,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.command.run()
}
