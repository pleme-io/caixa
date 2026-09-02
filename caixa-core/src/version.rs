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
            .map_err(|e| VersionError::semver(self.0.clone(), e.to_string()))
    }

    /// Borrow the string form.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
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

/// Substrate-canonical [`AsRef<str>`] projection on the [`CaixaVersion`]
/// typed newtype — routes through the same [`CaixaVersion::as_str`]
/// `pub const fn` scalar accessor the sibling [`fmt::Display`] impl
/// and every downstream `&str`-shaped consumer already keys off, so
/// any future consumer that binds a [`CaixaVersion`] through the
/// standard-library `impl AsRef<str>` bound (a `Path`-shaped file-
/// system reader on the operator side that accepts the version body
/// as one segment of a per-caixa `versao/<v>/...` on-disk cache path,
/// a builder-shaped API on the future `feira publish` writer verb
/// that composes `<prefix><versao>` through a git-tag builder crate's
/// `impl AsRef<str>` join step, a `HashMap<CaixaVersion, _>` lookup
/// through the `map.get::<str>(v.as_ref())` shape a future
/// version-keyed dispatch table lands on) reaches the wrapped
/// [`String`] through one substrate-primitive dispatch rather than
/// through the pre-lift `.as_str()` open-coded projection at every
/// wire-up.
///
/// Peer of the sibling [`fmt::Display`] impl on the same primitive —
/// both delegate to the shared [`CaixaVersion::as_str`] `pub const
/// fn` accessor, so [`format!("{v}")`], `v.as_str()`, and
/// `<CaixaVersion as AsRef<str>>::as_ref(&v)` resolve to the same
/// byte-string per instance by construction. A future rebrand of the
/// wrapped storage (a hypothetical widening to a typed [`semver::Version`]
/// slot the roadmap acknowledges once eager parse-on-construct
/// discipline lands, an internal normalization step that trims
/// leading zeroes off pre-release identifiers, a per-cluster overlay
/// the operator pins through a future `:versao-overrides` slot) that
/// changes what [`CaixaVersion::as_str`] returns migrates every
/// consumer of every one of the three paths in lockstep.
///
/// Same "route the trait impl through the substrate-primitive
/// accessor" discipline the sibling [`fmt::Display`] impl on this
/// type already carries — extends it onto the standard-library
/// [`AsRef<str>`] projection axis every third-party API that takes
/// `impl AsRef<str>` (the [`std::path::Path::new`] / [`std::fs`]
/// interop surface, [`std::process::Command::arg`], the peer
/// `tracing::field::Value` recorder's `Str`-arm, every `clap`-side
/// `value_parser!` fold that accepts an owned newtype through
/// `impl AsRef<str>`) already binds through. Rust-side newtype
/// convention pairs `AsRef<str>` and [`fmt::Display`] on the same
/// primitive so a caller who has one has both; before this lift,
/// [`CaixaVersion`] carried [`fmt::Display`] but not the paired
/// [`AsRef<str>`] impl the convention names.
///
/// The first standard-library trait added to [`CaixaVersion`] beyond
/// the pre-existing [`serde::Serialize`] / [`serde::Deserialize`] /
/// [`Debug`] / [`Clone`] / [`PartialEq`] / [`Eq`] / [`Hash`] derives
/// and the paired [`fmt::Display`] / [`From<String>`] / [`From<&str>`]
/// hand-written impls. Pinned load-bearing by
/// [`tests::caixa_version_as_ref_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaVersion::as_str`]) — any future
/// silent detour that routes the impl through a divergent projection
/// (a `Cow<'_, str>` intermediate, a stray `.to_lowercase()`
/// normalization, a swap onto a per-arm inline `&self.0.as_str()`
/// re-inlining) trips at caixa-core test time under `assert_eq!`
/// rather than at a downstream `impl AsRef<str>`-bound consumer's
/// silent split.
impl AsRef<str> for CaixaVersion {
    fn as_ref(&self) -> &str {
        self.as_str()
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

/// Canonical git remote name every `feira` writer-side verb pushes to —
/// the destination handle the operator-out-of-the-loop publish + deploy
/// chain (`feira publish`, `feira deploy --apply`, `feira app deploy
/// --apply`) names when it invokes `git push <remote> <ref>` against
/// the local clone of the source / k8s GitOps repo.
///
/// Three production-code consumers carry this remote name on the same
/// `git push` axis:
///
/// 1. [`caixa-feira`]'s `feira publish` verb (caixa-feira/src/cmd/publish.rs)
///    — the writer-side publish path. Its `--remote` clap flag defaults
///    to this string and the verb runs `git push <remote> <tag>` to push
///    the freshly written `v<versao>` tag upstream.
/// 2. [`caixa-feira`]'s `feira deploy --apply` verb
///    (caixa-feira/src/cmd/deploy.rs) — the writer-side Servico cluster-
///    deploy path. Its `push_origin` helper runs `git push origin HEAD`
///    against the k8s GitOps repo's working tree after upserting the
///    Servico's entry into the cluster's lareira-fleet-programs
///    HelmRelease values.
/// 3. [`caixa-feira`]'s `feira app deploy --apply` verb
///    (caixa-feira/src/cmd/app.rs) — the writer-side Aplicacao
///    cluster-deploy path. Its `push_origin` helper runs the same
///    `git push origin HEAD` against the k8s GitOps repo after writing
///    the rendered multi-doc YAML (programs.yaml entries + Cilium
///    NetworkPolicies + Gateway/HTTPRoute) to the cluster's tree.
///
/// Until this lift landed all three consumers carried the bare
/// `"origin"` byte inline — `publish.rs`'s clap `default_value = "origin"`,
/// `deploy.rs`'s `git(repo, ["push", "origin", "HEAD"])`, and
/// `app.rs`'s `git(repo, ["push", "origin", "HEAD"])`. A future
/// remote-naming-convention rebrand on any one side (the substrate
/// moving to `upstream` for forge-mirror clusters, to a per-tenant
/// remote naming convention once the operator-flux pipeline grows the
/// `:placement :remote` slot, or to the canonical multi-remote
/// `release` + `mirror` split every Erlang/OTP `release_handler` /
/// `relup` shop converges on once their git surface grows past one
/// upstream) without a coordinated edit on the other two would have
/// silently emitted a `git push` against a remote that doesn't exist
/// on the operator's clone (`fatal: '<remote>' does not appear to be
/// a git repository`) on one writer verb while the other two still
/// pushed to the old remote — operator-observed symptom: the publish
/// landed but the deploy didn't, or vice-versa, with the failure
/// surfacing as a partial-state rollout far from the rebrand commit's
/// source.
///
/// Lifting the literal to one `&'static str` constant closes the drift
/// footgun structurally — all three consumers read from the same
/// memory, so any future remote-naming rebrand reaches every writer
/// verb by construction and a CI build that re-introduces a sibling
/// inline `"origin"` literal trips the peer pinning tests
/// ([`caixa-feira`]'s `publish_remote_default_pins_lifted_caixa_core_constant`
/// on the clap-default axis, the sibling structural pins on the two
/// `push_origin` helpers) at the build-time fail-before-deploy
/// posture every prior load-bearing-string lift on this surface
/// ([`crate::DEFAULT_NAMESPACE`] a085b26, [`crate::DEFAULT_LIBRARY_NAME`]
/// 41438dc, [`crate::DEFAULT_SERVICO_PORT`] 1e22add,
/// [`crate::DEFAULT_PUBLISH_TAG_PREFIX`] 0a6a602,
/// [`crate::DEFAULT_FLUX_SYSTEM_NAMESPACE`] 7197d38) establishes.
///
/// Pairs with [`DEFAULT_PUBLISH_TAG_PREFIX`] on the same git remote
/// axis — `feira publish` runs `git push <DEFAULT_GIT_REMOTE>
/// <DEFAULT_PUBLISH_TAG_PREFIX><versao>` to push the typed `:versao`
/// body composed under the canonical prefix to the canonical remote.
/// Both halves of the publish-side convention now live in one place.
pub const DEFAULT_GIT_REMOTE: &str = "origin";

/// Canonical GitHub org name the pleme-io substrate defaults every un-
/// pinned caixa's source repo to — the org handle the two substrate-side
/// "no `:repositorio` / no `:fonte` declared, fall back to the canonical
/// org" paths compose their `github:<org>/<nome>` shorthand + full
/// `https://github.com/<org>/<nome>` URL under.
///
/// Two production-code consumers carry this org name on the same
/// canonical-substrate-default-git-org axis:
///
/// 1. [`caixa-feira`]'s `feira lock` verb's `resolve_stub` (caixa-feira/src/cmd/lock.rs)
///    — the resolver-side default. When a declared dep has no
///    `:fonte` block the stub resolver composes
///    `caixa_core::DepSource::default_github(<org>, &dep.nome)` to fill
///    the shorthand `github:<org>/<nome>` fallback the phase 1.B
///    `feira resolve` walker will resolve against upstream.
/// 2. [`caixa-flux`]'s [`caixa-flux::cluster_bundle`] renderer
///    (caixa-flux/src/lib.rs) — the renderer-side default. Its
///    `ClusterBundleOpts::for_caixa` constructor defaults
///    `git_url` to `format!("https://github.com/{org}/{}", caixa.nome)`
///    when the caixa carries no `:repositorio`, so the rendered
///    `gitrepository.yaml` points `FluxCD`'s `GitRepository`
///    reconciler at the substrate's canonical git host for un-pinned
///    caixas.
///
/// Until this lift landed both consumers carried the bare `"pleme-io"`
/// byte inline — `caixa-feira/src/cmd/lock.rs:61`'s
/// `default_github("pleme-io", …)` call and `caixa-flux/src/lib.rs`'s
/// `format!("https://github.com/pleme-io/{}", …)` literal. A future
/// substrate-side git-org migration (the pleme-io org renaming to a
/// short form, forking to a per-tenant `<org>-<tenant>` shape once the
/// operator-flux pipeline grows a `:placement :org` slot, or moving to
/// a self-hosted forge under a wholly-owned org name once the
/// substrate's forge-gen roadmap graduates past GitHub) without a
/// coordinated edit on both sides would silently emit a `feira lock`-
/// side `github:<old-org>/<nome>` fallback shorthand while the
/// `cluster_bundle`-side `gitrepository.yaml` pointed at the new org's
/// `<nome>` — the phase 1.B `feira resolve` walker would probe the
/// prior org's git host for a repo that migrated with the org, or vice-
/// versa: Flux's `GitRepository` reconciler would loop forever looking
/// for an upstream repo the old org handle no longer maps to, the
/// dependent `HelmRelease`'s `chart: sourceRef` would never resolve,
/// every per-Servico apply would silently come up with the prior
/// reconciled state, and the failure would surface at `kubectl describe
/// gitrepository` time (the `Status: Stalled` / `Reason: Failed` arm)
/// far from the org-migration commit's source.
///
/// Lifting the literal to one `&'static str` constant closes the drift
/// footgun structurally — both consumers read from the same memory, so
/// any future org migration reaches both sites by construction and a CI
/// build that re-introduces a sibling inline `"pleme-io"` literal trips
/// the peer pinning tests at the build-time fail-before-deploy posture
/// every prior load-bearing-string lift on this surface
/// ([`crate::DEFAULT_NAMESPACE`] a085b26,
/// [`crate::DEFAULT_LIBRARY_NAME`] 41438dc,
/// [`crate::DEFAULT_SERVICO_PORT`] 1e22add,
/// [`DEFAULT_PUBLISH_TAG_PREFIX`] 0a6a602,
/// [`DEFAULT_GIT_REMOTE`],
/// [`crate::DEFAULT_FLUX_SYSTEM_NAMESPACE`] 7197d38) establishes.
///
/// Distinct from the [`crate::PLEME_LABEL_PREFIX`] canonical pleme-io
/// label-namespace prefix (`"pleme.pleme.io"`, the K8s label-namespace
/// axis every substrate-emitted cluster object's `LABEL_APLICACAO` /
/// `LABEL_PROGRAM` / `LABEL_CONTRATO` axis shares) — these constants
/// sit on separate schema-contract surfaces (the git-host org handle
/// vs. the K8s label-namespace prefix) governed by independent rebrand
/// cycles, so a git-org rename must not couple the K8s label-namespace
/// axis to the git-host axis (or vice-versa). Splitting the two lets
/// each schema's future rebrand land independently at its canonical
/// const definition without silently coupling the surfaces — same
/// "byte-distinct, semantically distinct" discipline the
/// [`crate::PLEME_LABEL_PREFIX`] / [`crate::LABEL_APLICACAO`] /
/// [`crate::LABEL_PROGRAM`] / [`crate::LABEL_CONTRATO`] set establishes
/// on the peer per-K8s-label-namespace canonical-string surface.
pub const DEFAULT_PLEME_GIT_ORG: &str = "pleme-io";

/// Parse a dep's `:versao` string as a [`semver::VersionReq`].
///
/// Treats the literal `"*"` as "any version" (semver's wildcard).
pub fn parse_requirement(s: &str) -> Result<semver::VersionReq, VersionError> {
    if s == "*" {
        return Ok(semver::VersionReq::STAR);
    }
    semver::VersionReq::parse(s).map_err(|e| VersionError::requirement(s, e.to_string()))
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VersionError {
    #[error("invalid version '{0}': {1}")]
    Semver(String, String),
    #[error("invalid version requirement '{0}': {1}")]
    Requirement(String, String),
}

// Fold the sole `VersionError::Semver(<into-String-expr>, <into-String-expr>)`
// wire-up site on [`CaixaVersion::parse`]'s [`semver::Version::parse`]
// `map_err` arm onto one substrate primitive — the paired
// `(String, String)` two-slot tuple-newtype [`VersionError::Semver`] on
// the [`CaixaVersion`] parser surface, the first of the two variants on
// the [`VersionError`] envelope's paired `(String, String)` tuple-newtype
// codec-magnitude family (its peer is [`VersionError::Requirement`] on
// the sibling [`parse_requirement`] surface). Same discipline the peer
// per-variant lifts on [`AplicacaoError`] / [`SupervisorError`] /
// [`UpgradeError`] / [`LayoutError`] / [`DepError`] / [`ManifestError`]
// / [`LimitsError`] / [`BehaviorError`] / [`DialetoError`] have
// converged through the "one substrate primitive per emit-site variant"
// ratchet: the sole wire-up site opens the identical
// `VersionError::Semver(<into-String-expr>, <into-String-expr>)` block
// against the parser-scoped `String` binding (`self.0.clone()`) and the
// derived `String` binding (`e.to_string()`) on the failing
// [`semver::Version::parse`] arm, so the fold routes the site through
// one dispatch on a uniform pair of `impl Into<String>` params,
// byte-equal to the pre-lift tuple-newtype construction on the same
// arguments. The `impl Into<String>` bound covers both the pre-lift
// `String` bindings and any future `&str` binding a downstream consumer
// might carry without forcing the caller to spell the `.into()`
// conversion at the wire-up site — the same shape the peer
// [`LimitsError::empty_byte_size`] / [`LimitsError::empty_duration`] /
// [`DialetoError::leitura`] folds carry on the single-slot `(String)`
// tuple-newtype cousins of the same tuple-newtype error-envelope family
// on the sibling parser surfaces. `#[must_use]` fires a compile warning
// at any wire-up that mistakenly discards the constructed error. The
// added [`PartialEq`] / [`Eq`] derives on the envelope (peer with the
// sibling [`LimitsError`] / [`DialetoError`] / [`DepError`] envelopes
// on the same axis) let the fail-before-pass-after byte-equality pins
// below trip a de-lift regression at caixa-core test time under
// `PartialEq` rather than at a downstream diagnostic shape drift.
//
// Every future consumer that wants to construct this variant outside
// [`CaixaVersion::parse`] (a deferred `feira lint --canonical-versao`
// per-caixa admission verb probing each authored top-level `:versao`
// value against the same [`semver::Version::parse`] gate, an M4 typed
// `mesh.pleme.io/v1alpha1/Servico` CR materializer's per-manifest
// admission validator re-checking one edited `:versao` slot against
// the [`CaixaVersion::parse`] semver floor, a per-`caixa.lisp` value-
// shape pre-emitter probing each declared `:versao` magnitude ahead of
// the operator's admit-cycle) now reaches the variant through one call
// rather than re-inlining the two-slot tuple-newtype block in lockstep.
impl VersionError {
    /// Construct a [`VersionError::Semver`] carrying the offending
    /// authoring string `value` and the underlying [`semver::Version::parse`]
    /// `reason` verbatim in the variant's two-slot tuple-newtype payload.
    /// Folds the uniform `Self::Semver(value.into(), reason.into())`
    /// tuple-newtype construction onto one substrate primitive so every
    /// wire-up on the variant reads through one dispatch rather than the
    /// pre-lift open-coded
    /// `VersionError::Semver(<into-String-expr>, <into-String-expr>)`
    /// block. The paired `impl Into<String>` bounds cover the pre-lift
    /// `String` wire-up shape on [`CaixaVersion::parse`]
    /// (`self.0.clone()` on the parser-scoped `String` field, `e.to_string()`
    /// on the derived `String` from the failing
    /// [`semver::Version::parse`] arm) without forcing the caller to
    /// spell the conversion at the wire-up site. Peer to the sibling
    /// [`VersionError::Requirement`] variant on the [`parse_requirement`]
    /// surface — the same `(String, String)` two-slot tuple-newtype axis
    /// of the paired [`VersionError`] envelope, but on the `SemVer`
    /// version-body parser surface rather than the version-requirement
    /// parser surface.
    #[must_use]
    pub fn semver(value: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Semver(value.into(), reason.into())
    }

    /// Construct a [`VersionError::Requirement`] carrying the offending
    /// authoring string `value` and the underlying
    /// [`semver::VersionReq::parse`] `reason` verbatim in the variant's
    /// two-slot tuple-newtype payload. Folds the uniform
    /// `Self::Requirement(value.into(), reason.into())` tuple-newtype
    /// construction onto one substrate primitive so every wire-up on the
    /// variant reads through one dispatch rather than the pre-lift open-
    /// coded `VersionError::Requirement(<into-String-expr>,
    /// <into-String-expr>)` block. Peer to the sibling
    /// [`VersionError::semver`] ctor on the [`CaixaVersion::parse`]
    /// surface — the same `(String, String)` two-slot tuple-newtype axis
    /// of the paired [`VersionError`] envelope, but on the version-
    /// requirement parser surface rather than the semver-version-body
    /// parser surface. Closes the last un-lifted variant on the
    /// [`VersionError`] envelope: every arm now reaches its emit site
    /// through one substrate-primitive dispatch, matching the "one
    /// substrate primitive per emit-site variant" ratchet the peer per-
    /// variant lifts on [`crate::AplicacaoError`] /
    /// [`crate::SupervisorError`] / [`crate::UpgradeError`] /
    /// [`crate::LayoutError`] / [`crate::DepError`] /
    /// [`crate::ManifestError`] / [`crate::LimitsError`] /
    /// [`crate::BehaviorError`] / [`crate::DialetoError`] have converged
    /// onto.
    #[must_use]
    pub fn requirement(value: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Requirement(value.into(), reason.into())
    }
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
    fn caixa_version_as_str_accessor_is_const_fn() {
        // Fail-before-pass-after pin on [`CaixaVersion::as_str`]'s
        // `const`-eval-surface posture. The accessor projects the typed
        // newtype's inner [`String`] through the `pub const fn`
        // [`String::as_str`] (const-stable since Rust 1.87, well within
        // the workspace MSRV) — any future accidental downgrade to
        // non-`const` fails `as_str_via_const_fn` at caixa-core build
        // time with E0015 (`cannot call non-const method`), strictly
        // stronger than a runtime `assert!`. Sibling of the peer
        // per-M2/M3/universal-axis `String → &str` scalar-accessor
        // family pins on the sibling `const`-eval-surface passes
        // ([`crate::Caixa::nome`] / [`crate::Caixa::versao`] at the
        // top-level manifest, [`crate::aplicacao::Membro::nome`] /
        // [`crate::aplicacao::Membro::versao_requirement`] at the M3
        // membership axis, [`crate::aplicacao::Entrada::hostname`] /
        // [`crate::aplicacao::Entrada::destination`] at the M3 ingress
        // axis, [`crate::supervisor::ChildSpec::nome`] /
        // [`crate::supervisor::ChildSpec::versao_requirement`] at the
        // M2 supervisor-tree axis,
        // [`crate::upgrade::UpgradeFromEntry::prior_versao`] at the M2
        // upgrade axis, [`crate::dep::Dep::nome`] /
        // [`crate::dep::Dep::versao_requirement`] at the dep-graph
        // axis, and the peer per-`:contratos` [`crate::aplicacao::WitContract::source`] /
        // [`crate::aplicacao::WitContract::destination`] /
        // [`crate::aplicacao::WitContract::world_ref`] trio the
        // sibling pin at 279823b already anchors).
        const fn as_str_via_const_fn(v: &CaixaVersion) -> &str {
            v.as_str()
        }
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            assert_eq!(as_str_via_const_fn(&v), v.as_str());
            assert_eq!(v.as_str(), versao);
        }
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
    fn semver_ctor_matches_tuple_literal_wrap_on_str_binding() {
        // Fail-before-pass-after byte-equality pin: the lifted
        // [`VersionError::semver`] inherent ctor projects a `&str`
        // binding pair through the paired `impl Into<String>` bounds
        // byte-equal to the pre-lift open-coded
        // `VersionError::Semver(<into-String-expr>, <into-String-expr>)`
        // tuple-literal on the same fixture, so any future silent
        // regression that swaps `.into()` for a divergent conversion
        // (a stray `String::from(str::trim(v))` normalization, a
        // parity-lossy `.to_lowercase()` fold, a `Cow<'_, str>` detour)
        // trips at caixa-core test time under `PartialEq` rather than
        // at a downstream diagnostic-shape drift on a consumer surface.
        // Same shape the peer
        // [`crate::LimitsError::empty_byte_size_ctor_matches_tuple_literal_wrap_on_str_binding`]
        // / [`crate::DialetoError::leitura_ctor_matches_tuple_literal_wrap_on_str_binding`]
        // pins carry on the sibling single-slot `(String)` tuple-newtype
        // cousins of the same tuple-newtype error-envelope family on the
        // sibling parser surfaces.
        let value: &str = "not-a-version";
        let reason: &str = "unexpected character 'n' while parsing major version number";
        assert_eq!(
            VersionError::semver(value, reason),
            VersionError::Semver(value.to_string(), reason.to_string()),
            "generated semver ctor over `&str` bindings must match \
             the pre-lift tuple-literal wrap on the same fixture",
        );
    }

    #[test]
    fn semver_ctor_matches_tuple_literal_wrap_on_string_binding() {
        // Fail-before-pass-after byte-equality pin on the paired owned-
        // `String` shape — the actual wire-up shape on
        // [`CaixaVersion::parse`] (`self.0.clone()` +
        // `e.to_string()`). Peer to the `&str` variant above; refuses
        // any future de-lift that inlines a divergent construction on
        // the owned-`String` path (a stray `.trim().to_string()`
        // normalization on either slot, a swap that routes the ctor
        // through the sibling [`VersionError::Requirement`] variant on
        // the paired parser surface).
        let value: String = String::from("1.2");
        let reason: String =
            String::from("unexpected end of input while parsing minor version number");
        assert_eq!(
            VersionError::semver(value.clone(), reason.clone()),
            VersionError::Semver(value, reason),
            "generated semver ctor over owned-`String` bindings must \
             match the pre-lift tuple-literal wrap on the same fixture",
        );
    }

    #[test]
    fn parse_semver_error_routes_through_semver_ctor() {
        // Fail-before-pass-after routes-through pin: refuses any future
        // de-lift of [`CaixaVersion::parse`]'s
        // [`semver::Version::parse`] `map_err` arm off the substrate
        // primitive. Sweeps three malformed authoring shapes (a bare
        // non-numeric, a partial `major.minor` shape, a stray leading
        // `v`-prefix that the [`DEFAULT_PUBLISH_TAG_PREFIX`] git-tag
        // convention rejects at the version-body slot) through the
        // parser and asserts the emitted [`VersionError`] equals the
        // ctor-built error verbatim under `PartialEq`, so any future
        // swap of the wire-up (an inline `Self::Semver(...)`
        // re-inlining, a routing detour through the sibling
        // [`VersionError::Requirement`] variant on the paired parser
        // surface, a swap of the ordering on the paired arguments)
        // trips at caixa-core test time rather than at a downstream
        // diagnostic drift on a `feira lint` / operator admission
        // callsite.
        for bad in ["not-a-version", "1.2", "v0.1.0"] {
            let v: CaixaVersion = bad.into();
            let err = v
                .parse()
                .expect_err("malformed versao fixture must fail semver parsing");
            let semver_reason = match semver::Version::parse(bad) {
                Err(e) => e.to_string(),
                Ok(_) => unreachable!(
                    "fixture `{bad}` is documented as a `SemVer` \
                     rejection but parsed cleanly — the pin's oracle \
                     drifted from `semver`'s current shape",
                ),
            };
            assert_eq!(
                err,
                VersionError::semver(bad, semver_reason),
                "CaixaVersion::parse must route its semver `map_err` \
                 arm through the lifted VersionError::semver ctor on \
                 the same offending value and semver reason",
            );
        }
    }

    #[test]
    fn default_git_remote_pins_canonical_origin_byte() {
        // Bridge-arm pin: [`DEFAULT_GIT_REMOTE`] resolves to the
        // canonical `"origin"` byte today, the same remote-handle every
        // `git clone <url>` invocation populates by default and every
        // peer `feira` writer-side verb (`feira publish`, `feira deploy
        // --apply`, `feira app deploy --apply`) names when it invokes
        // `git push <remote> <ref>` against the local clone. Pin the
        // literal here (peer with the
        // [`DEFAULT_PUBLISH_TAG_PREFIX`] / [`crate::DEFAULT_SERVICO_PORT`]
        // / [`crate::DEFAULT_NAMESPACE`] / [`crate::DEFAULT_LIBRARY_NAME`]
        // / [`crate::DEFAULT_FLUX_SYSTEM_NAMESPACE`] canonical-literal
        // pins on the sibling lifted-constant surfaces) so a future
        // remote-naming rebrand surfaces here as a coordinated edit-
        // point: the sibling [`caixa-feira`]
        // `publish_remote_default_pins_lifted_caixa_core_constant`
        // pinning test already pins the equality at the clap-default
        // axis; this pin closes the second coordinate of the
        // triangle by anchoring the lifted constant's current byte
        // to the canonical git-default-remote convention's documented
        // shape.
        assert_eq!(DEFAULT_GIT_REMOTE, "origin");
    }

    #[test]
    fn default_pleme_git_org_pins_canonical_pleme_io_byte() {
        // Bridge-arm pin: [`DEFAULT_PLEME_GIT_ORG`] resolves to the
        // canonical `"pleme-io"` GitHub-org-handle today, the same org
        // name every peer substrate-side default-git-source consumer
        // ([`caixa-feira`]'s `feira lock` `resolve_stub` for the
        // per-dep `:fonte`-elided `github:<org>/<nome>` fallback,
        // [`caixa-flux`]'s `ClusterBundleOpts::for_caixa` constructor
        // for the per-caixa `:repositorio`-elided
        // `https://github.com/<org>/<nome>` fallback) fills into its
        // per-consumer render/resolve compose site. Pin the literal
        // here (peer with the [`DEFAULT_PUBLISH_TAG_PREFIX`] /
        // [`DEFAULT_GIT_REMOTE`] canonical-literal pins on the sibling
        // lifted-constant surfaces) so a future substrate-side git-org
        // migration surfaces here as a coordinated edit-point: both
        // sibling consumer sites already thread through the same
        // `&'static str`, this pin anchors the lifted constant's
        // current byte to the canonical substrate-git-org convention's
        // documented shape.
        assert_eq!(DEFAULT_PLEME_GIT_ORG, "pleme-io");
    }

    #[test]
    fn requirement_ctor_matches_tuple_literal_wrap_on_str_binding() {
        // Fail-before-pass-after byte-equality pin: the lifted
        // [`VersionError::requirement`] inherent ctor projects a `&str`
        // binding pair through the paired `impl Into<String>` bounds
        // byte-equal to the pre-lift open-coded
        // `VersionError::Requirement(<into-String-expr>, <into-String-expr>)`
        // tuple-literal on the same fixture. Same shape the peer
        // [`VersionError::semver_ctor_matches_tuple_literal_wrap_on_str_binding`]
        // pin carries on the sibling [`VersionError::Semver`] variant of
        // the same `(String, String)` two-slot tuple-newtype envelope.
        let value: &str = "not-a-req";
        let reason: &str = "unexpected character 'n' while parsing major version number";
        assert_eq!(
            VersionError::requirement(value, reason),
            VersionError::Requirement(value.to_string(), reason.to_string()),
            "generated requirement ctor over `&str` bindings must match \
             the pre-lift tuple-literal wrap on the same fixture",
        );
    }

    #[test]
    fn requirement_ctor_matches_tuple_literal_wrap_on_string_binding() {
        // Fail-before-pass-after byte-equality pin on the paired owned-
        // `String` shape. Peer to the `&str` variant above; refuses any
        // future de-lift that inlines a divergent construction on the
        // owned-`String` path (a stray `.trim().to_string()` normalization
        // on either slot, a swap that routes the ctor through the sibling
        // [`VersionError::Semver`] variant on the paired parser surface,
        // an argument-ordering swap on the paired slots).
        let value: String = String::from("^bogus");
        let reason: String = String::from("unexpected character while parsing requirement");
        assert_eq!(
            VersionError::requirement(value.clone(), reason.clone()),
            VersionError::Requirement(value, reason),
            "generated requirement ctor over owned-`String` bindings must \
             match the pre-lift tuple-literal wrap on the same fixture",
        );
    }

    #[test]
    fn parse_requirement_error_routes_through_requirement_ctor() {
        // Fail-before-pass-after routes-through pin: refuses any future
        // de-lift of [`parse_requirement`]'s
        // [`semver::VersionReq::parse`] `map_err` arm off the substrate
        // primitive. Sweeps three malformed authoring shapes (a bare
        // non-numeric, a stray operator with no version body, a
        // caret-prefixed non-numeric that the [`semver::VersionReq`]
        // grammar rejects at the operator-body slot) through the parser
        // and asserts the emitted [`VersionError`] equals the ctor-built
        // error verbatim under `PartialEq`, so any future swap of the
        // wire-up (an inline `Self::Requirement(...)` re-inlining, a
        // routing detour through the sibling [`VersionError::Semver`]
        // variant on the paired parser surface, an argument-ordering
        // swap on the paired slots) trips at caixa-core test time rather
        // than at a downstream diagnostic drift on a `feira lock` /
        // resolver admission callsite. The `"*"` wildcard short-circuit
        // is deliberately excluded from the sweep — it returns
        // [`semver::VersionReq::STAR`] before reaching the parser arm.
        for bad in ["not-a-req", "^", "^bogus"] {
            let err = parse_requirement(bad)
                .expect_err("malformed requirement fixture must fail parsing");
            let semver_reason = match semver::VersionReq::parse(bad) {
                Err(e) => e.to_string(),
                Ok(_) => unreachable!(
                    "fixture `{bad}` is documented as a `VersionReq` \
                     rejection but parsed cleanly — the pin's oracle \
                     drifted from `semver`'s current shape",
                ),
            };
            assert_eq!(
                err,
                VersionError::requirement(bad, semver_reason),
                "parse_requirement must route its `map_err` arm through \
                 the lifted VersionError::requirement ctor on the same \
                 offending value and semver reason",
            );
        }
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

    #[test]
    fn caixa_version_as_ref_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl AsRef<str> for CaixaVersion` — asserts the standard-
        // library trait impl and the substrate-primitive
        // [`CaixaVersion::as_str`] `pub const fn` accessor resolve to
        // the same `&str` per instance, so any future silent detour
        // that routes the impl through a divergent projection (a
        // `Cow<'_, str>` intermediate, a stray `.to_lowercase()`
        // normalization, a swap onto a per-arm inline `&self.0.as_str()`
        // re-inlining, a swap onto a divergent [`String::trim`]
        // fold) trips at caixa-core test time under `PartialEq`
        // rather than at a downstream `impl AsRef<str>`-bound
        // consumer's silent split. Sweeps four authoring shapes (a
        // canonical release version, a pre-release build-metadata
        // version, the zero-version canonical unset baseline, and
        // the empty-string byte the caller-side default-construct
        // path composes) so every non-degenerate arm of the wrapped
        // `String` storage is covered. Peer of the sibling
        // [`caixa_version_as_str_accessor_is_const_fn`] const-eval
        // pin on the same [`CaixaVersion::as_str`] primitive — the
        // two pins together cover the const-eval axis (the pin above)
        // and the trait-projection axis (this pin) of the same
        // substrate-primitive scalar accessor.
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            assert_eq!(
                <CaixaVersion as AsRef<str>>::as_ref(&v),
                v.as_str(),
                "AsRef<str> impl must byte-equal CaixaVersion::as_str \
                 on the same instance — divergence signals a silent \
                 detour off the substrate-primitive accessor",
            );
            assert_eq!(
                <CaixaVersion as AsRef<str>>::as_ref(&v),
                versao,
                "AsRef<str> impl must byte-equal the pre-lift wrapped \
                 String storage on round-trip through the From<&str> \
                 constructor — divergence signals a normalization \
                 detour on either the constructor or the accessor",
            );
        }
    }

    #[test]
    fn caixa_version_as_ref_str_routes_through_display_via_shared_accessor() {
        // Fail-before-pass-after byte-parity pin on the three-path
        // convergence discipline the substrate primitive now carries
        // on the `&str`-projection axis: `<CaixaVersion as
        // AsRef<str>>::as_ref(&v)` (the newly lifted impl),
        // `format!("{v}")` (the pre-existing [`fmt::Display`] impl),
        // and `v.as_str()` (the substrate-primitive `pub const fn`
        // accessor both trait impls delegate through) must resolve to
        // the same byte-string on every instance. Refuses any future
        // divergence between the two trait impls (a stray
        // [`fmt::Display::fmt`] rewrite that inlines
        // `f.write_str(&self.0)` on the wrapped `String` directly,
        // bypassing the shared accessor; a hypothetical `AsRef<str>`
        // rewrite that inlines the same `&self.0` field-access) that
        // would silently split the two projection paths of the same
        // typed newtype. Mirrors the sibling three-path-convergence
        // discipline the peer [`RestartStrategy`] typed enum carries
        // on its `Display` / `as_str` / `Serialize` triple (aplicacao.rs
        // pin `restart_strategy_display_matches_serialized_wire_byte_string`).
        for versao in ["0.1.0", "1.2.3-alpha.1", ""] {
            let v: CaixaVersion = versao.into();
            let via_as_ref: &str = <CaixaVersion as AsRef<str>>::as_ref(&v);
            let via_display: String = format!("{v}");
            let via_accessor: &str = v.as_str();
            assert_eq!(via_as_ref, via_accessor);
            assert_eq!(via_display, via_accessor);
            assert_eq!(via_as_ref, via_display.as_str());
        }
    }
}
