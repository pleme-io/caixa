use std::path::PathBuf;

use anyhow::{Context, Result};
use caixa_core::{Caixa, CaixaKind};
use clap::Args;

use super::load::{caixa_root, load_caixa};

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
        let root = caixa_root(self.path.as_deref());
        let caixa = load_caixa(&root)?;

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
        .descricao()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("caixa {}", c.nome()));
    let kind_comment = match c.kind() {
        CaixaKind::Biblioteca => "library (loaded via tatara-lisp importar)",
        CaixaKind::Binario => "binary (exe/ entries)",
        CaixaKind::Servico => "service (servicos/ entries)",
        CaixaKind::Supervisor => "supervisor (typed children only; runs no code itself)",
        CaixaKind::Aplicacao => "aplicacao (typed mesh of Servicos; runs no code itself)",
        CaixaKind::Acao => "acao (typed CI run; runs no code itself)",
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
        nome = c.nome(),
        versao = c.versao(),
        description = description,
        kind_comment = kind_comment,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_flake_routes_through_caixa_nome_versao_kind_accessors() {
        // Emit-path pin: the `feira nix` per-caixa flake.nix emitter's
        // terminal `{nome}` / `{versao}` scalars and the `Kind:` comment
        // line's discriminant must derive through the typed
        // [`caixa_core::Caixa::nome`] / [`caixa_core::Caixa::versao`] /
        // [`caixa_core::Caixa::kind`] accessors byte-for-byte. Before
        // this converge the four sites (`c.nome` fallback-description
        // suffix, `match c.kind` kind-comment discriminant, `nome =
        // c.nome` + `versao = c.versao` format-args tail) carried raw
        // field-accesses into the inline `format!` templates, bypassing
        // the typed dispatch every peer `feira`-verb emit-site
        // (build.rs `build_summary_line`, deploy.rs, publish.rs) already
        // routes through.
        //
        // Byte-equal today (`Caixa::nome` is `&self.nome`, `Caixa::versao`
        // is `&self.versao`, `Caixa::kind` returns `self.kind` by copy);
        // the pin catches any future accessor extension (SemVer-2 build-
        // metadata canonicalization the CAIXA-SDLC §I SemVer-2 pin
        // acknowledges, per-edition pre-release-tag overlay dispatched
        // through the sibling universal-axis scalars) whose `feira nix`
        // flake-emit would silently split the `pname` / `version` fields
        // from the paired `feira publish` git-tag body + `feira deploy`
        // k8s-repo emit that already routes through the accessors.
        let caixa = Caixa::from_lisp(
            r#"(defcaixa
                 :nome "nix-flake-pin"
                 :kind Biblioteca
                 :versao "0.3.7"
                 :bibliotecas ())"#,
        )
        .expect("parse");
        let flake = render_flake(&caixa);

        // The `pname` / `version` fields must carry the caixa's
        // `:nome` / `:versao` verbatim, sourced through the accessors.
        assert!(
            flake.contains(&format!("pname = \"{}\";", caixa.nome())),
            "flake `pname` must route `{{nome}}` through Caixa::nome \
             (got: {flake:?})"
        );
        assert!(
            flake.contains(&format!("version = \"{}\";", caixa.versao())),
            "flake `version` must route `{{versao}}` through Caixa::versao \
             (got: {flake:?})"
        );
        // The `installPhase` interpolates `{nome}` into two `share/caixa/{nome}`
        // subdirectory paths — same accessor, sibling site to the `pname`.
        assert!(
            flake.contains(&format!("mkdir -p $out/share/caixa/{}", caixa.nome())),
            "flake `installPhase` mkdir must route `{{nome}}` through \
             Caixa::nome (got: {flake:?})"
        );
        // The `Kind:` comment carries the CaixaKind discriminant's
        // comment-string projection, routed through `Caixa::kind()`.
        assert!(
            flake.contains("# Kind: library (loaded via tatara-lisp importar)"),
            "flake Kind-comment must route `match c.kind()` through the \
             Biblioteca arm's comment string (got: {flake:?})"
        );
    }

    #[test]
    fn render_flake_kind_comment_covers_every_caixa_kind_variant() {
        // Closed-set-enum coverage pin: the `match c.kind()` inside
        // `render_flake` must resolve to a non-empty comment string for
        // every [`CaixaKind`] variant, so a future kind addition (a
        // seventh variant beyond Biblioteca/Binario/Servico/Supervisor/
        // Aplicacao/Acao — the CAIXA-SDLC §I six-kind roster the
        // CLAUDE.md ★★ table pins) that lands without extending the
        // match arm surfaces at `cargo test` here rather than at
        // `feira nix` runtime with a phantom-branch fallback.
        //
        // The pin walks every declared [`CaixaKind`] variant through a
        // minimal `defcaixa` fixture and asserts the emitted flake
        // carries a `# Kind: <comment>` line — this is the exhaustive
        // sibling to the per-variant enumeration `render_flake` already
        // carries. Any future variant addition will land as a rustc
        // `non_exhaustive_patterns` error on `match c.kind()` in
        // `render_flake` itself (the crate is not `#[non_exhaustive]`);
        // this test's `for kind in [...]` array is the fail-before-pass-
        // after companion pin that a routine adding a new variant to
        // the flake emitter must also extend the coverage array.
        for (kind, kind_symbol, nome_slug) in [
            (CaixaKind::Biblioteca, "Biblioteca", "k-biblioteca"),
            (CaixaKind::Binario, "Binario", "k-binario"),
            (CaixaKind::Servico, "Servico", "k-servico"),
            (CaixaKind::Supervisor, "Supervisor", "k-supervisor"),
            (CaixaKind::Aplicacao, "Aplicacao", "k-aplicacao"),
            (CaixaKind::Acao, "Acao", "k-acao"),
        ] {
            // Every kind can be authored at the manifest tip with an
            // empty body — the layout invariants only fire on
            // `feira build`, not at `from_lisp` parse time — so every
            // variant round-trips through the emitter. The `:nome`
            // stays lowercase to satisfy the DNS-1123-label discipline
            // the peer [`Caixa::validate_nome`] gate enforces at
            // build-time even though `from_lisp` does not run it here.
            let src = format!(
                r#"(defcaixa
                     :nome "{nome_slug}"
                     :kind {kind_symbol}
                     :versao "0.1.0")"#
            );
            let caixa =
                Caixa::from_lisp(&src).unwrap_or_else(|e| panic!("parse {kind_symbol}: {e}"));
            assert_eq!(
                caixa.kind(),
                kind,
                "parse round-trip for {kind_symbol} must land on the \
                 expected CaixaKind variant"
            );
            let flake = render_flake(&caixa);
            assert!(
                flake.contains("# Kind: "),
                "flake for {kind_symbol} must carry a `# Kind: ` comment \
                 line — the `match c.kind()` arm must resolve non-empty \
                 (got: {flake:?})"
            );
            assert!(
                flake.contains(&format!("pname = \"{nome_slug}\";")),
                "flake for {kind_symbol} must carry the caixa's `:nome` \
                 verbatim on the `pname` field, routed through Caixa::nome \
                 (got: {flake:?})"
            );
        }
    }

    #[test]
    fn render_flake_description_fallback_routes_through_caixa_nome() {
        // Fallback-arm pin: when `:descricao` is omitted the
        // `render_flake` head composes a fallback description
        // `format!("caixa {}", c.nome())`, the sole raw-field-access
        // site outside the format-args tail this converge lifted. The
        // pin catches any future re-inline (a diff that re-introduces
        // `format!("caixa {}", c.nome)`) — byte-equal today, drifts
        // apart on any future `Caixa::nome` extension.
        let caixa = Caixa::from_lisp(
            r#"(defcaixa
                 :nome "desc-fallback"
                 :kind Biblioteca
                 :versao "0.1.0")"#,
        )
        .expect("parse");
        assert_eq!(
            caixa.descricao(),
            None,
            "fixture must omit `:descricao` to exercise the fallback arm"
        );
        let flake = render_flake(&caixa);
        assert!(
            flake.contains(&format!("description = \"caixa {}\";", caixa.nome())),
            "flake description must fall back to `caixa <nome>` sourced \
             through Caixa::nome (got: {flake:?})"
        );
    }
}
