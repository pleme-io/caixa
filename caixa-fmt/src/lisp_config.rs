//! Lisp-native fmt config — `.caixa-fmt.lisp` at the repo root.
//!
//! ```lisp
//! (deffmt-config
//!   :line-width 80
//!   :indent 2
//!   :max-inline-items 3
//!   :trailing-newline #t
//!   :preserve-comments #t)
//! ```
//!
//! Every field is optional and falls back to [`FmtConfig::default`], which
//! is the house style: 80 columns, stack down rather than across.

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
    /// Max children an s-expression may hold and still render flat.
    /// See [`FmtConfig::max_inline_items`] — bounds sideways sprawl by
    /// arity, which width alone cannot do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_inline_items: Option<i64>,
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

    /// Register `FmtConfigLisp` with the global tatara-lisp domain
    /// registry so `deffmt-config` is dispatchable from any tatara-lisp
    /// binary that seeds the registry.
    ///
    /// # Errors
    ///
    /// [`tatara_lisp::KeywordCollision`] when a peer type has already
    /// claimed the `deffmt-config` keyword in this process. Peer of
    /// [`caixa_core::Caixa::register`] and the other per-crate entry
    /// points documented at
    /// `caixa-core/src/manifest.rs::Caixa::register` — every substrate
    /// crate that owns a tatara-lisp keyword now propagates the same
    /// typed error verbatim.
    pub fn register() -> Result<(), tatara_lisp::KeywordCollision> {
        tatara_lisp::domain::register::<Self>()
    }

    #[must_use]
    pub fn into_runtime(self) -> FmtConfig {
        let mut out = FmtConfig::default();
        if let Some(w) = self.line_width {
            out.line_width = usize::try_from(w).unwrap_or(80);
        }
        if let Some(n) = self.max_inline_items {
            out.max_inline_items = usize::try_from(n).unwrap_or(3);
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
        let src = r"(deffmt-config :line-width 80 :indent 4 :preserve-comments #f)";
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
        FmtConfigLisp::register().expect("first register call in this test process must succeed");
        assert!(tatara_lisp::domain::registered_keywords().contains(&"deffmt-config"));
    }
}
