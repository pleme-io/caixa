//! Lisp-native fmt config — `.caixa-fmt.lisp` at the repo root.
//!
//! ```lisp
//! (deffmt-config
//!   :line-width 100
//!   :indent 2
//!   :trailing-newline #t
//!   :preserve-comments #t)
//! ```

use serde::{Deserialize, Serialize};
use tatara_lisp::DeriveTataraDomain;

use crate::config::FmtConfig;

#[derive(DeriveTataraDomain, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "deffmt-config")]
pub struct FmtConfigLisp {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_width: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indent: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trailing_newline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preserve_comments: Option<bool>,
}

impl FmtConfigLisp {
    pub fn from_lisp(src: &str) -> Result<Self, tatara_lisp::LispError> {
        use tatara_lisp::domain::TataraDomain;
        let forms = tatara_lisp::read(src)?;
        let first = forms
            .first()
            .ok_or_else(|| tatara_lisp::LispError::Compile {
                form: "deffmt-config".into(),
                message: "empty fmt config".into(),
            })?;
        Self::compile_from_sexp(first)
    }

    pub fn register() {
        tatara_lisp::domain::register::<Self>();
    }

    #[must_use]
    pub fn into_runtime(self) -> FmtConfig {
        let mut out = FmtConfig::default();
        if let Some(w) = self.line_width {
            out.line_width = usize::try_from(w).unwrap_or(100);
        }
        if let Some(i) = self.indent {
            out.indent = usize::try_from(i).unwrap_or(2);
        }
        if let Some(t) = self.trailing_newline {
            out.trailing_newline = t;
        }
        if let Some(p) = self.preserve_comments {
            out.preserve_comments = p;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_deffmt_config() {
        let src = r#"(deffmt-config :line-width 80 :indent 4 :preserve-comments #f)"#;
        let c = FmtConfigLisp::from_lisp(src).unwrap();
        assert_eq!(c.line_width, Some(80));
        assert_eq!(c.indent, Some(4));
        assert_eq!(c.preserve_comments, Some(false));

        let r = c.into_runtime();
        assert_eq!(r.line_width, 80);
        assert_eq!(r.indent, 4);
        assert!(!r.preserve_comments);
    }

    #[test]
    fn register_populates_registry() {
        FmtConfigLisp::register();
        assert!(tatara_lisp::domain::registered_keywords().contains(&"deffmt-config"));
    }
}
