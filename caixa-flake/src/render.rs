//! Render `FlakeLisp` → `flake.nix` source. Transpile mode (always
//! available); direct-eval mode lands when sui's nixpkgs parity is closer.

use crate::flake::{FlakeLisp, FlakeOutput, FlakePackage};

#[must_use]
pub fn render_flake_nix(f: &FlakeLisp) -> String {
    let mut out = String::new();
    out.push_str("# Auto-generated from flake.lisp. Re-run `feira nix` after edits.\n\n");
    out.push_str("{\n");
    out.push_str(&format!("  description = {:?};\n\n", f.descricao));

    // Inputs.
    out.push_str("  inputs = {\n");
    for entrada in &f.entradas {
        if let Some(seg) = &entrada.segue {
            out.push_str(&format!("    {}.follows = {seg:?};\n", entrada.nome));
        } else {
            out.push_str(&format!("    {}.url = {:?};\n", entrada.nome, entrada.url));
        }
    }
    out.push_str("  };\n\n");

    // Outputs.
    out.push_str("  outputs = { self, ");
    let names: Vec<&str> = f.entradas.iter().map(|e| e.nome.as_str()).collect();
    out.push_str(&names.join(", "));
    out.push_str(", ... }: let\n");
    out.push_str("    systems = [ \"aarch64-darwin\" \"x86_64-darwin\" \"aarch64-linux\" \"x86_64-linux\" ];\n");
    out.push_str("    forAllSystems = f: builtins.listToAttrs (map (s: { name = s; value = f s; }) systems);\n");
    out.push_str("  in {\n");

    if let Some(s) = &f.saidas {
        render_outputs(s, &mut out);
    }
    out.push_str("  };\n}\n");
    out
}

fn render_outputs(o: &FlakeOutput, out: &mut String) {
    if !o.pacotes.is_empty() {
        out.push_str("    packages = forAllSystems (system:\n");
        out.push_str("      let pkgs = import nixpkgs { inherit system; }; in {\n");
        for p in &o.pacotes {
            render_package(p, out);
        }
        out.push_str("      });\n\n");
    }
    if o.dev_shells {
        out.push_str("    devShells = forAllSystems (system:\n");
        out.push_str("      let pkgs = import nixpkgs { inherit system; }; in {\n");
        out.push_str("        default = pkgs.mkShell { packages = [ ]; };\n");
        out.push_str("      });\n");
    }
}

fn render_package(p: &FlakePackage, out: &mut String) {
    out.push_str(&format!(
        "        {} = pkgs.stdenvNoCC.mkDerivation {{\n\
         \tpname = {:?};\n\
         \tversion = \"0.1.0\";\n\
         \tsrc = {};\n\
         \tinstallPhase = \"mkdir -p $out && cp -r . $out/\";\n\
         }};\n",
        p.nome,
        p.nome,
        nix_src(&p.src)
    ));
}

fn nix_src(s: &str) -> String {
    if s.starts_with('.') || s.starts_with('/') {
        s.to_string()
    } else {
        format!("{s:?}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flake::{FlakeInput, FlakeLisp, FlakeOutput, FlakePackage};

    #[test]
    fn renders_with_inputs_and_package() {
        let f = FlakeLisp {
            descricao: "demo".into(),
            entradas: vec![FlakeInput {
                nome: "nixpkgs".into(),
                url: "github:nixos/nixpkgs".into(),
                segue: None,
            }],
            saidas: Some(FlakeOutput {
                pacotes: vec![FlakePackage {
                    nome: "default".into(),
                    src: ".".into(),
                }],
                modulos: vec![],
                dev_shells: true,
            }),
        };
        let out = render_flake_nix(&f);
        assert!(out.contains("description = \"demo\""));
        assert!(out.contains("nixpkgs.url = \"github:nixos/nixpkgs\""));
        assert!(out.contains("packages = forAllSystems"));
        assert!(out.contains("devShells = forAllSystems"));
    }

    #[test]
    fn lisp_round_trip_and_render() {
        let src = r#"
(defflake
  :descricao "demo"
  :entradas ((:nome "nixpkgs" :url "github:nixos/nixpkgs")))
"#;
        let f = FlakeLisp::from_lisp(src).unwrap();
        let nix = render_flake_nix(&f);
        assert!(nix.contains("description = \"demo\""));
    }
}
