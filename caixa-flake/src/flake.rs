use serde::{Deserialize, Serialize};
use tatara_lisp::DeriveTataraDomain;

/// A whole `flake.lisp` — parsed as a TataraDomain via `defflake`.
#[derive(DeriveTataraDomain, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defflake")]
pub struct FlakeLisp {
    pub descricao: String,
    #[serde(default)]
    pub entradas: Vec<FlakeInput>,
    #[serde(default)]
    pub saidas: Option<FlakeOutput>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FlakeInput {
    pub nome: String,
    pub url: String,
    /// When true, emit `inputs.<nome>.follows = "<segue>"` instead of `.url`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segue: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct FlakeOutput {
    #[serde(default)]
    pub pacotes: Vec<FlakePackage>,
    #[serde(default)]
    pub modulos: Vec<FlakeModule>,
    #[serde(default)]
    pub dev_shells: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FlakePackage {
    pub nome: String,
    pub src: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FlakeModule {
    pub nome: String,
    pub caminho: String,
}

impl FlakeLisp {
    pub fn from_lisp(src: &str) -> Result<Self, tatara_lisp::LispError> {
        use tatara_lisp::domain::TataraDomain;
        let forms = tatara_lisp::read(src)?;
        let first = forms
            .first()
            .ok_or_else(|| tatara_lisp::LispError::Compile {
                form: "defflake".into(),
                message: "empty flake.lisp".into(),
            })?;
        Self::compile_from_sexp(first)
    }

    pub fn register() {
        tatara_lisp::domain::register::<Self>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_flake() {
        let src = r#"
(defflake
  :descricao "my caixa"
  :entradas ((:nome "nixpkgs" :url "github:nixos/nixpkgs?ref=nixos-unstable")
             (:nome "substrate" :url "github:pleme-io/substrate")))
"#;
        let f = FlakeLisp::from_lisp(src).unwrap();
        assert_eq!(f.descricao, "my caixa");
        assert_eq!(f.entradas.len(), 2);
        assert_eq!(f.entradas[0].nome, "nixpkgs");
    }

    #[test]
    fn register_populates_registry() {
        FlakeLisp::register();
        assert!(tatara_lisp::domain::registered_keywords().contains(&"defflake"));
    }
}
