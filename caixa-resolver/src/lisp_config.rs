//! Lisp-native resolver config — `~/.config/caixa/config.lisp`.
//!
//! Shape mirrors [`crate::ResolverConfig`] but is a TataraDomain, so authoring
//! is the same homoiconic surface as every other caixa form.
//!
//! ```lisp
//! (defresolver-config
//!   :default-host "github:pleme-io"
//!   :include-dev  #f
//!   :additional-hosts ("codeberg:my-org"))
//! ```

use serde::{Deserialize, Serialize};
use tatara_lisp::DeriveTataraDomain;

use crate::config::ResolverConfig;

#[derive(DeriveTataraDomain, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defresolver-config")]
pub struct ResolverConfigLisp {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_host: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_dev: Option<bool>,

    #[serde(default)]
    pub additional_hosts: Vec<String>,
}

impl ResolverConfigLisp {
    /// Parse a config.lisp source string.
    pub fn from_lisp(src: &str) -> Result<Self, tatara_lisp::LispError> {
        use tatara_lisp::domain::TataraDomain;
        let forms = tatara_lisp::read(src)?;
        let first = forms
            .first()
            .ok_or_else(|| tatara_lisp::LispError::Compile {
                form: "defresolver-config".into(),
                message: "empty config.lisp".into(),
            })?;
        Self::compile_from_sexp(first)
    }

    /// Register the keyword so `tatara-check` / LSP can dispatch on it.
    pub fn register() {
        tatara_lisp::domain::register::<Self>();
    }

    /// Lower into the runtime [`ResolverConfig`].
    #[must_use]
    pub fn into_runtime(self) -> ResolverConfig {
        let mut out = ResolverConfig::default();
        if let Some(h) = self.default_host {
            out.default_host = h;
        }
        if let Some(d) = self.cache_dir {
            out.cache_dir = Some(std::path::PathBuf::from(d));
        }
        if let Some(dev) = self.include_dev {
            out.include_dev = dev;
        }
        out.additional_hosts = self.additional_hosts;
        out
    }
}

impl ResolverConfig {
    /// Load `~/.config/caixa/config.lisp`; fall back to `config.yaml`; else
    /// default.
    pub fn load_lisp_or_yaml() -> Self {
        let Some(base) = dirs::config_dir() else {
            return Self::default();
        };
        let dir = base.join("caixa");
        let lisp = dir.join("config.lisp");
        if lisp.exists() {
            if let Ok(src) = std::fs::read_to_string(&lisp) {
                if let Ok(parsed) = ResolverConfigLisp::from_lisp(&src) {
                    return parsed.into_runtime();
                }
            }
        }
        let yaml = dir.join("config.yaml");
        if yaml.exists() {
            if let Ok(src) = std::fs::read_to_string(&yaml) {
                if let Ok(cfg) = serde_yaml::from_str::<Self>(&src) {
                    return cfg;
                }
            }
        }
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_defresolver_config() {
        let src = r#"
(defresolver-config
  :default-host "codeberg:my-org"
  :include-dev #t
  :additional-hosts ("sourcehut:zig-org"))
"#;
        let c = ResolverConfigLisp::from_lisp(src).unwrap();
        assert_eq!(c.default_host.as_deref(), Some("codeberg:my-org"));
        assert_eq!(c.include_dev, Some(true));
        assert_eq!(c.additional_hosts, vec!["sourcehut:zig-org".to_string()]);
    }

    #[test]
    fn lowers_into_runtime_config() {
        let c = ResolverConfigLisp {
            default_host: Some("codeberg:org".into()),
            cache_dir: None,
            include_dev: Some(true),
            additional_hosts: vec![],
        };
        let r = c.into_runtime();
        assert_eq!(r.default_host, "codeberg:org");
        assert!(r.include_dev);
    }

    #[test]
    fn register_populates_registry() {
        ResolverConfigLisp::register();
        assert!(tatara_lisp::domain::registered_keywords().contains(&"defresolver-config"));
    }
}
