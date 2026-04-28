use std::path::PathBuf;

use anyhow::{Context, Result};
use caixa_core::{Caixa, CaixaKind};
use clap::Args;

/// Emit a `flake.nix` that builds the caixa via substrate.
#[derive(Args)]
pub struct Nix {
    /// caixa root (defaults to CWD).
    #[arg(long)]
    pub path: Option<PathBuf>,

    /// Print to stdout instead of writing flake.nix.
    #[arg(long)]
    pub stdout: bool,
}

impl Nix {
    pub fn run(self) -> Result<()> {
        let root = self.path.clone().unwrap_or_else(|| PathBuf::from("."));
        let manifest_path = root.join("caixa.lisp");
        let src = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        let caixa = Caixa::from_lisp(&src)
            .with_context(|| format!("parsing {}", manifest_path.display()))?;

        let flake = render_flake(&caixa);
        if self.stdout {
            print!("{flake}");
            return Ok(());
        }
        let flake_path = root.join("flake.nix");
        std::fs::write(&flake_path, &flake)
            .with_context(|| format!("writing {}", flake_path.display()))?;
        eprintln!("wrote {}", flake_path.display());
        Ok(())
    }
}

fn render_flake(c: &Caixa) -> String {
    let description = c
        .descricao
        .clone()
        .unwrap_or_else(|| format!("caixa {}", c.nome));
    let kind_comment = match c.kind {
        CaixaKind::Biblioteca => "library (loaded via tatara-lisp importar)",
        CaixaKind::Binario => "binary (exe/ entries)",
        CaixaKind::Servico => "service (servicos/ entries)",
        CaixaKind::Supervisor => "supervisor (typed children only; runs no code itself)",
    };
    format!(
        r##"{{
  description = "{description}";

  # Auto-generated from caixa.lisp by `feira nix`. Edit caixa.lisp, rerun.
  # Kind: {kind_comment}

  inputs = {{
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    substrate = {{
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
    }};
    tatara = {{
      url = "github:pleme-io/tatara";
      inputs.nixpkgs.follows = "nixpkgs";
    }};
  }};

  outputs = {{ self, nixpkgs, flake-utils, substrate, tatara, ... }}:
    flake-utils.lib.eachDefaultSystem (system:
      let pkgs = import nixpkgs {{ inherit system; }};
      in {{
        packages.default = pkgs.stdenvNoCC.mkDerivation {{
          pname = "{nome}";
          version = "{versao}";
          src = ./.;
          installPhase = ''
            mkdir -p $out/share/caixa/{nome}
            cp -r . $out/share/caixa/{nome}/
          '';
          meta = {{
            description = "{description}";
          }};
        }};

        devShells.default = pkgs.mkShell {{
          packages = [ ];
        }};
      }});
}}
"##,
        nome = c.nome,
        versao = c.versao,
        description = description,
        kind_comment = kind_comment,
    )
}
