use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A caixa's pinned version — a thin typed wrapper over a String that parses
/// as [`semver::Version`] on demand.
///
/// Stored as a String at rest so authoring a `caixa.lisp` stays a single
/// quoted literal. The typed form is reached through [`Self::parse`].
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct CaixaVersion(pub String);

impl CaixaVersion {
    /// Parse and validate the wrapped string as semver.
    pub fn parse(&self) -> Result<semver::Version, VersionError> {
        semver::Version::parse(&self.0)
            .map_err(|e| VersionError::Semver(self.0.clone(), e.to_string()))
    }

    /// Borrow the string form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CaixaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for CaixaVersion {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for CaixaVersion {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Parse a dep's `:versao` string as a [`semver::VersionReq`].
///
/// Treats the literal `"*"` as "any version" (semver's wildcard).
pub fn parse_requirement(s: &str) -> Result<semver::VersionReq, VersionError> {
    if s == "*" {
        return Ok(semver::VersionReq::STAR);
    }
    semver::VersionReq::parse(s)
        .map_err(|e| VersionError::Requirement(s.to_string(), e.to_string()))
}

#[derive(Debug, Error)]
pub enum VersionError {
    #[error("invalid version '{0}': {1}")]
    Semver(String, String),
    #[error("invalid version requirement '{0}': {1}")]
    Requirement(String, String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_round_trip() {
        let v: CaixaVersion = "1.2.3".into();
        assert_eq!(v.as_str(), "1.2.3");
        assert_eq!(v.parse().unwrap().to_string(), "1.2.3");
    }

    #[test]
    fn star_is_any() {
        let r = parse_requirement("*").unwrap();
        assert!(r.matches(&"0.1.0".parse().unwrap()));
        assert!(r.matches(&"99.0.0".parse().unwrap()));
    }

    #[test]
    fn caret_matches_minor_range() {
        let r = parse_requirement("^0.1").unwrap();
        assert!(r.matches(&"0.1.0".parse().unwrap()));
        assert!(r.matches(&"0.1.99".parse().unwrap()));
        assert!(!r.matches(&"0.2.0".parse().unwrap()));
    }

    #[test]
    fn invalid_version_errors() {
        let v: CaixaVersion = "not-a-version".into();
        assert!(v.parse().is_err());
    }
}
