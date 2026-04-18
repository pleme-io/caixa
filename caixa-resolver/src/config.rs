use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Resolver configuration — lives at `~/.config/caixa/config.yaml`.
///
/// The whole file is optional; defaults work out of the box. When a user
/// wants to point `:nome` shorthand at a non-default org, they edit this.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolverConfig {
    /// How to expand a bare `:nome "x"` when `:fonte` is omitted.
    /// Default: `github:pleme-io`.
    #[serde(default = "default_host")]
    pub default_host: String,

    /// Where to cache cloned repos. Default: `$XDG_CACHE_HOME/caixa` or
    /// `~/.cache/caixa`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<PathBuf>,

    /// Include `:deps-dev` when resolving.
    #[serde(default)]
    pub include_dev: bool,

    /// Extra hosts the resolver recognizes as shorthand prefixes.
    /// E.g. `["codeberg:my-org"]` lets users write `(:nome "x" :fonte
    /// (:tipo git :repo "codeberg:my-org/x"))`.
    #[serde(default)]
    pub additional_hosts: Vec<String>,
}

impl Default for ResolverConfig {
    fn default() -> Self {
        Self {
            default_host: default_host(),
            cache_dir: None,
            include_dev: false,
            additional_hosts: Vec::new(),
        }
    }
}

fn default_host() -> String {
    "github:pleme-io".to_string()
}

impl ResolverConfig {
    /// Load from `~/.config/caixa/config.yaml`, falling back to defaults.
    pub fn load_or_default() -> Self {
        let Some(base) = dirs::config_dir() else {
            return Self::default();
        };
        let path = base.join("caixa").join("config.yaml");
        match std::fs::read_to_string(&path) {
            Ok(src) => serde_yaml::from_str(&src).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }
}
