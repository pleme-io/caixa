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

/// Canonical Zig-style git-tag prefix every `feira publish` run writes
/// and every downstream consumer of a published caixa reads. A caixa
/// published at `:versao "0.1.0"` lands as a git tag `v0.1.0` on the
/// source repo's `origin` remote — the [`crate::CaixaVersion`] value
/// gates the version body, this constant gates the prefix the body
/// composes under.
///
/// Two production-code consumers carry this prefix on the same git
/// remote axis:
///
/// 1. [`caixa-feira`]'s `feira publish` verb (caixa-feira/src/cmd/publish.rs)
///    — the writer. Its `--prefix` clap flag defaults to this string
///    and the verb computes the tag as `format!("{prefix}{versao}")`
///    before `git tag -a <tag>` + `git push origin <tag>`.
/// 2. [`caixa-flux`]'s [`caixa-flux::cluster_bundle`] renderer
///    (caixa-flux/src/lib.rs) — the reader. Its
///    `ClusterBundleOpts::for_caixa` constructor defaults
///    `git_ref: GitRefSpec::Tag(...)` to `<prefix><versao>` so the
///    rendered `gitrepository.yaml` carries `ref: { tag: v<versao> }`
///    pointing `FluxCD`'s `GitRepository` reconciler at the exact tag
///    the publisher just wrote.
///
/// Until this lift landed both consumers carried the bare `"v"` byte
/// inline — `caixa-feira/src/cmd/publish.rs:22`'s clap
/// `default_value = "v"` and `caixa-flux/src/lib.rs:335`'s
/// `format!("v{}", caixa.versao)` literal. A future Zig-style-tag
/// convention rebrand (the substrate moving to plain `<versao>` tags
/// once the GitHub releases UI normalizes around the bare form, to
/// `release/<versao>` once a sibling forge convention adopts the
/// `<type>/<value>` slash-namespaced shape, or to a per-edition
/// override the operator pins through a future `:placement
/// :tag-prefix` slot) without a coordinated edit on both sides would
/// silently emit a `feira publish`-side tag at one shape (e.g.
/// `release/0.1.0`) and a `cluster_bundle`-side `ref: { tag: v0.1.0 }`
/// pointing at the prior shape — Flux's `GitRepository` reconciler
/// would loop forever looking for an upstream `v0.1.0` ref the publish
/// remote no longer carries, the dependent `HelmRelease`'s `chart:
/// sourceRef` would never resolve, every per-Servico apply would
/// silently come up with the prior reconciled state, and the failure
/// would surface at `kubectl describe gitrepository` time (the
/// `Status: Stalled` / `Reason: Failed` arm) far from the rebrand
/// commit's source.
///
/// Lifting the literal to one `&'static str` constant closes the drift
/// footgun structurally — both consumers read from the same memory,
/// so any future rebrand reaches both sites by construction and a CI
/// build that re-introduces a sibling inline `"v"` literal trips the
/// peer pinning tests
/// ([`caixa-feira`]'s `publish_prefix_default_pins_lifted_caixa_core_constant`,
/// [`caixa-flux`]'s `cluster_bundle_default_git_tag_uses_lifted_caixa_core_prefix`)
/// at the build-time fail-before-deploy posture every prior
/// load-bearing-string lift on this surface
/// ([`crate::DEFAULT_NAMESPACE`] a085b26,
/// [`crate::DEFAULT_LIBRARY_NAME`] 41438dc,
/// [`crate::DEFAULT_SERVICO_PORT`] 1e22add) establishes.
///
/// Authoring-side `:versao` gates already refuse the `"v"`-prefixed
/// publish tag shape leaking back into a version body — every typed
/// `:versao` surface (top-level `:versao`, `:upgrade-from :from`,
/// `:deps :versao`, `:deps-dev :versao`, `:membros :versao`,
/// `:children :versao`) routes through `semver::Version::parse` /
/// [`parse_requirement`], both of which reject the `v`-prefix as
/// invalid `SemVer`. The split — bare `SemVer` at the `:versao` slot,
/// `v<versao>` at the published git-tag axis — is the convention this
/// constant pins.
pub const DEFAULT_PUBLISH_TAG_PREFIX: &str = "v";

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

    #[test]
    fn default_publish_tag_prefix_pins_canonical_v_byte() {
        // Bridge-arm pin: [`DEFAULT_PUBLISH_TAG_PREFIX`] resolves to the
        // canonical Zig-style `"v"` byte today, the same prefix every
        // peer doc-comment on the typed `:versao` surfaces (the
        // top-level `:versao` `validate_versao` cascade at
        // caixa-core/src/manifest.rs:646, the four sibling per-axis
        // `:versao` requirement gates that name the publish-side
        // `v<versao>` tag inline in their bodies) cites as the
        // canonical convention. Pin the literal here (peer with the
        // [`crate::DEFAULT_SERVICO_PORT`] / [`crate::DEFAULT_NAMESPACE`]
        // / [`crate::DEFAULT_LIBRARY_NAME`] canonical-literal pins on
        // the sibling lifted-constant surfaces) so a future rebrand of
        // the constant surfaces here as a coordinated edit-point: both
        // sibling pinning tests on the two consumer crates
        // ([`caixa-feira`] `publish_prefix_default_pins_lifted_caixa_core_constant`,
        // [`caixa-flux`] `cluster_bundle_default_git_tag_uses_lifted_caixa_core_prefix`)
        // already pin the equality at the consumer-default axis; this
        // pin closes the third coordinate of the triangle by anchoring
        // the lifted constant's current byte to the canonical Zig-style
        // convention's documented shape.
        assert_eq!(DEFAULT_PUBLISH_TAG_PREFIX, "v");
    }
}
