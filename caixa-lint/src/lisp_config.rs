//! Lisp-native lint config — `.caixa-lint.lisp` at the repo root.
//!
//! Users enable/disable rules, override severities, and (phase 2) author
//! pattern-based rules as data rather than Rust code.
//!
//! ```lisp
//! (deflint-config
//!   :severidade-padrao Warning
//!   :regras ((:id "small-forms"        :habilitada #f)
//!            (:id "no-fixme-descricao" :severidade Error))
//!   :regras-customizadas
//!     ((defregra :id "no-legacy-provider"
//!                :descricao "the 'legacy' provider is banned"
//!                :severidade Error
//!                :padrao (keyword-value :provider "legacy"))))
//! ```

use serde::{Deserialize, Serialize};
use tatara_lisp::DeriveTataraDomain;

use crate::diagnostic::Severity;

#[derive(DeriveTataraDomain, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "deflint-config")]
pub struct LintConfigLisp {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severidade_padrao: Option<Severity>,

    #[serde(default)]
    pub regras: Vec<RuleOverride>,

    #[serde(default)]
    pub regras_customizadas: Vec<CustomRule>,
}

/// Override an existing built-in rule's severity or enabled state.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuleOverride {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub habilitada: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severidade: Option<Severity>,
}

/// A user-authored rule expressed as data. Phase 2 — the Rust runner
/// interprets a small pattern DSL (`keyword-value`, `has-head`, etc.).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CustomRule {
    pub id: String,
    pub descricao: String,
    pub severidade: Severity,
    /// A pattern value — parsed by [`crate::pattern::Pattern::from_value`]
    /// when the linter loads.
    pub padrao: serde_json::Value,
}

impl LintConfigLisp {
    pub fn from_lisp(src: &str) -> Result<Self, tatara_lisp::LispError> {
        use tatara_lisp::domain::TataraDomain;
        let forms = tatara_lisp::read(src)?;
        let first = forms
            .first()
            .ok_or_else(|| tatara_lisp::LispError::Compile {
                form: "deflint-config".into(),
                message: "empty lint config".into(),
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
    fn parses_deflint_config() {
        let src = r#"
(deflint-config
  :severidade-padrao Warning
  :regras ((:id "small-forms"        :habilitada #f)
           (:id "no-fixme-descricao" :severidade Error)))
"#;
        let c = LintConfigLisp::from_lisp(src).unwrap();
        assert_eq!(c.severidade_padrao, Some(Severity::Warning));
        assert_eq!(c.regras.len(), 2);
        assert_eq!(c.regras[0].id, "small-forms");
        assert_eq!(c.regras[0].habilitada, Some(false));
        assert_eq!(c.regras[1].severidade, Some(Severity::Error));
    }

    #[test]
    fn register_populates_registry() {
        LintConfigLisp::register();
        assert!(tatara_lisp::domain::registered_keywords().contains(&"deflint-config"));
    }
}
