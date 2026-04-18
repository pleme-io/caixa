//! Cache directory discovery — XDG-respecting.

use std::path::{Path, PathBuf};

/// A cache root — `~/.cache/caixa` (or `$XDG_CACHE_HOME/caixa`).
#[derive(Debug, Clone)]
pub struct CacheDir {
    root: PathBuf,
}

impl CacheDir {
    /// Discover the default cache directory and ensure it exists.
    pub fn discover() -> std::io::Result<Self> {
        let root = dirs::cache_dir()
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join(".cache")
            })
            .join("caixa");
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Use an explicit directory. Caller is responsible for its existence.
    #[must_use]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { root: path.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Per-source directory, keyed by a BLAKE3 hash of the canonical URL + ref.
    #[must_use]
    pub fn source_dir(&self, key: &str) -> PathBuf {
        self.root.join("sources").join(key)
    }
}
