//! caixa-flux — typed renderer that emits the FluxCD-side fragments a
//! caixa Servico needs in the cluster's GitOps tree.
//!
//! Same naming convention as [`caixa_helm`] (renders per-program Helm
//! charts) and [`caixa_flake`] (renders flake.nix): `caixa-<target>` =
//! "Rust crate that takes a typed [`Caixa`] and emits the canonical
//! source for `<target>`".
//!
//! ## Two paths, two surfaces
//!
//! Per `theory/META-FRAMEWORK.md` §I, two equally-canonical ways exist
//! to deploy a caixa Servico:
//!
//! 1. **Aggregator path** ([`programs_yaml_entry`]) — the cluster has
//!    exactly one `lareira-fleet-programs` HelmRelease whose values
//!    contain a `programs:` array. Adding a Servico = adding one entry
//!    to that array. **Higher leverage** — one HelmRelease handles the
//!    whole fleet's worth of caixas, fewer reconciler events, simpler
//!    cluster surface. This is what `feira deploy` uses by default.
//!
//! 2. **Bundle path** ([`cluster_bundle`]) — emit a fresh `GitRepository`
//!    + `HelmRelease` + `Kustomization` trio for the caixa's own per-
//!    program chart (rendered by `caixa-helm`). Used for one-off /
//!    isolated services where the aggregator overhead is undesirable
//!    (e.g. alpha workloads with non-standard images, breakglass tooling).
//!
//! ## V0 contract
//!
//! ```rust,ignore
//! use caixa_core::Caixa;
//! use caixa_flux::programs_yaml_entry;
//!
//! let caixa = Caixa::from_lisp(src)?;
//! let cu_yaml: serde_yaml::Value =
//!     serde_yaml::from_str(std::fs::read_to_string("servicos/hello-rio.computeunit.yaml")?)?;
//! let entry: serde_yaml::Value = programs_yaml_entry(&caixa, &cu_yaml)?;
//! // → { name: hello-rio, namespace: tatara-system, module: { source: ... }, ... }
//! ```
//!
//! ## What this is NOT
//!
//! - Not a Flux CLI wrapper — bytes only.
//! - Not the operator deploy bundle — that lives in `pleme-io/caixa/operator-flux/`.
//! - Not an installer — `feira deploy` orchestrates the I/O of writing
//!   to a GitOps repo + opening a PR.

#![allow(clippy::module_name_repetitions)]

use caixa_core::{Caixa, MappingExt, kube_metadata_str_field, lareira_chart_name};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors caixa-flux can raise.
#[derive(Debug, Error)]
pub enum Error {
    /// The caixa's `:kind` doesn't match what `caixa-flux` targets
    /// (this renderer only emits `programs.yaml` entries +
    /// `GitRepository`/`HelmRelease`/`Kustomization` bundles for
    /// `:kind Servico`). Lifted from a prior `NotAServico(CaixaKind)`
    /// arm to wrap [`caixa_core::KindMismatch`] so the diagnostic
    /// names the offending caixa's `:nome` (not just its kind),
    /// shared verbatim with `caixa-helm` and `caixa-mesh`.
    #[error("{0}")]
    NotAServico(#[from] caixa_core::KindMismatch),
    /// The caixa's `:servicos` list doesn't carry exactly one entry —
    /// the V0 contract every Servico-kind caixa satisfies (one
    /// ComputeUnit YAML pointer per Servico, matching the one
    /// programs.yaml entry / cluster bundle this renderer emits).
    /// Lifted from a prior `UnsupportedServicoCount(usize)` arm to
    /// wrap [`caixa_core::ServicoCountMismatch`] so the diagnostic
    /// names the offending caixa's `:nome` (not just the count),
    /// shared verbatim with `caixa-helm` (the peer per-Servico
    /// renderer running the same V0 invariant on the
    /// `lareira-<nome>` chart-dir axis).
    #[error("{0}")]
    UnsupportedServicoCount(#[from] caixa_core::ServicoCountMismatch),
    #[error("computeunit yaml missing required field: {0}")]
    MissingField(&'static str),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("render: {0}")]
    Render(#[from] caixa_core::RenderError),
}

/// Default cluster-wide namespace for caixa Servicos when the
/// computeunit doesn't pin its own. Re-export of the canonical
/// [`caixa_core::DEFAULT_NAMESPACE`] so the namespace string lives in
/// exactly one place across every renderer — caixa-flux's
/// programs.yaml / GitRepository / HelmRelease / Kustomization
/// emitters and caixa-mesh's programs fan-out / CiliumNetworkPolicy /
/// Gateway / HTTPRoute emitters now consult the same `&'static str`,
/// so a future per-cluster-namespace rebrand is a one-line edit on
/// the canonical [`caixa_core::DEFAULT_NAMESPACE`] declaration, not a
/// coordinated rewrite across this crate, caixa-mesh, and every
/// future per-target renderer the substrate adds.
pub use caixa_core::DEFAULT_NAMESPACE;

/// Canonical Flux v2 `spec.interval` reconcile-poll cadence default the
/// substrate seeds into every per-caixa `cluster_bundle` CR triplet.
/// Re-export of the canonical [`caixa_core::DEFAULT_FLUX_RECONCILE_INTERVAL`]
/// so the Flux v2 controller-side reconcile-cadence default scalar-value
/// string lives in exactly one place across every caixa renderer —
/// [`ClusterBundleOpts::for_caixa`]'s per-caixa default seed (the sole
/// production-code site the prior inline `"10m"` scalar-value literal sat
/// at, seeding the [`ClusterBundleOpts::interval`] field that
/// [`cluster_bundle`]'s three per-CR format-string templates thread
/// through their [`FLUX_KEY_INTERVAL`]-keyed `spec.interval` axis
/// verbatim) now consults the same `&'static str`, so a future substrate-
/// side reconcile-cadence migration (`"10m"` → `"5m"` on lower-latency-
/// poll optimizations, `"10m"` → `"15m"` on cost-optimized clusters where
/// per-CR source-controller poll cost outweighs reconcile-freshness
/// gains — coordinated with the upstream Flux v2 project's per-controller
/// tuning cycle) is a one-line edit on the canonical
/// [`caixa_core::DEFAULT_FLUX_RECONCILE_INTERVAL`] declaration, not a
/// coordinated rewrite across the [`ClusterBundleOpts`] default seed and
/// every future per-target renderer the substrate adds. Pairs with the
/// sibling [`FLUX_KEY_INTERVAL`] re-export on the same per-CR
/// `spec.interval` scalar-axis — the key half of the per-CR scalar-key/
/// scalar-value pair lives at [`FLUX_KEY_INTERVAL`], the value half's
/// substrate-side default seed lives here. Same shape as the sibling
/// [`caixa_core::DEFAULT_NAMESPACE`] / [`caixa_core::DEFAULT_LIBRARY_NAME`]
/// / [`caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE`] /
/// [`caixa_core::DEFAULT_PUBLISH_TAG_PREFIX`] re-exports on the peer
/// canonical-substrate-default-load-bearing-scalar surface.
pub use caixa_core::DEFAULT_FLUX_RECONCILE_INTERVAL;

/// Canonical Flux v2 `HelmRelease.spec.chart.spec.chart` per-CR chart-
/// directory-in-GitRepository-source sub-path default the substrate seeds
/// into every per-caixa `helmrelease.yaml` document. Re-export of the
/// canonical [`caixa_core::DEFAULT_FLUX_CHART_SOURCE_SUBPATH`] so the
/// Flux v2 helm-controller-side per-CR chart-directory-in-git-source
/// default scalar-value lives in exactly one place across every caixa
/// renderer — [`ClusterBundleOpts::for_caixa`]'s per-caixa default seed
/// (the sole production-code site the prior inline `"chart".into()`
/// scalar-value literal sat at, seeding the [`ClusterBundleOpts::chart_path`]
/// field that [`cluster_bundle`]'s `helmrelease.yaml` format-string
/// template threads through its [`FLUX_HELMCHART_TEMPLATE_KEY_CHART`]-
/// keyed `spec.chart.spec.chart` axis verbatim) now consults the same
/// `&'static str`, so a future substrate-side chart-directory-in-git-
/// source rebrand (`"chart"` → `"charts"` once a per-caixa multi-chart
/// layout lands and the substrate publishes N sibling `lareira-<nome>/`
/// charts under one git repository, `"chart"` → `"helm"` on a cross-
/// language convention alignment with sibling wasm-runtime substrates,
/// `"chart"` → `"deploy"` on a per-caixa-deploy-directory naming
/// migration) is a one-line edit on the canonical
/// [`caixa_core::DEFAULT_FLUX_CHART_SOURCE_SUBPATH`] declaration, not a
/// coordinated rewrite across the [`ClusterBundleOpts`] default seed and
/// every future per-target renderer the substrate adds. Pairs with the
/// sibling [`FLUX_HELMCHART_TEMPLATE_KEY_CHART`] re-export on the same
/// per-CR `spec.chart.spec.chart` scalar-axis — the key half of the per-
/// CR scalar-key/scalar-value pair lives at
/// [`FLUX_HELMCHART_TEMPLATE_KEY_CHART`], the value half's substrate-side
/// default seed lives here. Same shape as the sibling
/// [`caixa_core::DEFAULT_NAMESPACE`] / [`caixa_core::DEFAULT_LIBRARY_NAME`]
/// / [`caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE`] /
/// [`caixa_core::DEFAULT_FLUX_RECONCILE_INTERVAL`] /
/// [`caixa_core::DEFAULT_FLUX_KUSTOMIZATION_TIMEOUT`] /
/// [`caixa_core::DEFAULT_PUBLISH_TAG_PREFIX`] re-exports on the peer
/// canonical-substrate-default-load-bearing-scalar surface.
pub use caixa_core::DEFAULT_FLUX_CHART_SOURCE_SUBPATH;

/// Canonical Flux v2 `HelmRelease.spec.{install,upgrade}.remediation.retries`
/// bounded retry-count default the substrate seeds into every per-caixa
/// `helmrelease.yaml` document. Re-export of the canonical
/// [`caixa_core::FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`] so the
/// Flux v2 helm-controller-side remediation-retries default scalar-value
/// lives in exactly one place across every caixa renderer —
/// [`cluster_bundle`]'s `helmrelease.yaml` format-string template's two
/// production-code retry-cap sites (the prior inline `retries: 3`
/// scalar-value literal under the `install.remediation` sub-block + the
/// second inline `retries: 3` scalar-value literal under the
/// `upgrade.remediation` sub-block) now both consume the same `u32` at
/// emit time through one `{retries_default}` named-arg interpolation, so
/// a future substrate-side retry-ceiling migration (`3` → `5` once per-
/// caixa idempotency invariants tighten, `3` → `1` on hardened per-caixa
/// pipelines where a failed apply should escalate to operator-attention
/// rather than mask under further retries — coordinated with the
/// upstream Flux v2 project's per-controller tuning cycle) is a one-line
/// edit on the canonical
/// [`caixa_core::FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`]
/// declaration, not a coordinated rewrite across the two per-CR
/// remediation-retries sites. Before this lift the two axes carried
/// independently-inlined `retries: 3` literals in the sibling
/// `install.remediation.retries` / `upgrade.remediation.retries`
/// positions of the [`cluster_bundle`] `helmrelease.yaml` format-string
/// template — any future retry-ceiling migration on one axis without a
/// coordinated edit on the other would have silently split the
/// substrate's canonical retry-ceiling between the install-path (first-
/// time chart applies) and the upgrade-path (every subsequent per-
/// caixa-version re-apply the same `HelmRelease` gates), with no field
/// naming the ceiling-drift root cause. Pairs with the sibling
/// [`DEFAULT_FLUX_RECONCILE_INTERVAL`] re-export on the same canonical-
/// Flux-v2-per-CR-substrate-default surface — the reconcile-poll cadence
/// default names how often the helm-controller re-evaluates per-CR
/// desired state, and this retry-cap names how many times a per-
/// evaluation Helm action is allowed to fail-and-retry before it stops.
/// Same shape as the sibling [`caixa_core::DEFAULT_NAMESPACE`] /
/// [`caixa_core::DEFAULT_LIBRARY_NAME`] /
/// [`caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE`] /
/// [`caixa_core::DEFAULT_FLUX_RECONCILE_INTERVAL`] /
/// [`caixa_core::DEFAULT_PUBLISH_TAG_PREFIX`] re-exports on the peer
/// canonical-substrate-default-load-bearing-scalar surface.
pub use caixa_core::FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT;

/// Canonical Flux v2 `HelmRelease.spec.{install,upgrade}.remediation.retries`
/// leaf scalar-key — re-export of the canonical
/// [`caixa_core::FLUX_HELMRELEASE_KEY_RETRIES`] so the Flux v2
/// helm-controller-side per-CR remediation-retries leaf-scalar-key lives
/// in exactly one place across every caixa renderer. Peer to the sibling
/// [`FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`] (30dcdae) scalar-
/// value half of the same `(leaf-key, scalar-value)` per-path retry-cap
/// declaration pair — extends the drift-closing discipline the scalar-
/// value lift established from the value the leaf holds onto the leaf-
/// key itself. Two production emit sites (this crate's [`cluster_bundle`]
/// `helmrelease.yaml` format-string template's install-path retry-cap
/// leaf under the [`FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`]-valued
/// sub-block + the sibling upgrade-path retry-cap leaf under the same
/// scalar-value, both threading the same `&'static str` through a
/// `{retries_key}` named-arg interpolation) plus two test-fixture
/// navigation sites in `mod tests` (the install-path
/// [`cluster_bundle_helmrelease_install_remediation_retries_pins_lifted_default`]
/// pin's `.get("retries")` probe + the sibling upgrade-path
/// [`cluster_bundle_helmrelease_upgrade_remediation_retries_pins_lifted_default`]
/// pin's peer probe) now consult the same `&'static str`. Until this
/// lift landed the axis carried four inline `retries` literals across
/// the two production emit sites and the two test-fixture navigation
/// sites; a future hypothetical Flux v3 rename (`attempts` /
/// `maxRetries` / `retryCount`) on any production-emit site without a
/// coordinated edit on the sibling navigation sites would have silently
/// stripped the retry-cap declaration from the emitted `remediation:`
/// sub-block, letting the helm-controller fall back to the Flux v2
/// upstream default rather than the substrate's chosen ceiling with no
/// diagnostic naming the leaf-key-drift root cause. Same shape as the
/// sibling [`FLUX_KEY_SOURCE_REF`] (236ef01) / [`FLUX_KEY_CHART`] /
/// [`FLUX_KEY_VALUES`] / [`FLUX_KEY_INTERVAL`] /
/// [`FLUX_KEY_HEALTH_CHECKS`] lifts on the peer per-CR container-axis-
/// key surface — extends the discipline from the container-axis keys
/// that nest per-CR sub-blocks onto a leaf-scalar-key at the bottom of
/// a two-level nested per-path retry-cap declaration.
pub use caixa_core::FLUX_HELMRELEASE_KEY_RETRIES;

/// Canonical Flux v2 `HelmRelease.spec.{install,upgrade}.remediation`
/// sub-container-axis-key — re-export of the canonical
/// [`caixa_core::FLUX_HELMRELEASE_KEY_REMEDIATION`] so the Flux v2
/// helm-controller-side per-CR remediation sub-container-axis-key lives
/// in exactly one place across every caixa renderer. Peer to the sibling
/// [`FLUX_HELMRELEASE_KEY_RETRIES`] (a12f9fc) leaf-scalar-key half + the
/// sibling [`FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`] (30dcdae)
/// scalar-value half of the same
/// `(container-axis-key, leaf-scalar-key, scalar-value)` per-path retry-
/// cap declaration triple — closes the parent-container-axis-key axis on
/// the same per-path retry-cap declaration, so all three halves now live
/// in one place. Two production emit sites (this crate's
/// [`cluster_bundle`] `helmrelease.yaml` format-string template's
/// install-path remediation sub-block-header + the sibling upgrade-path
/// remediation sub-block-header, both threading the same `&'static str`
/// through a `{remediation_key}` named-arg interpolation) plus two test-
/// fixture navigation sites in `mod tests` (the install-path
/// [`cluster_bundle_helmrelease_install_remediation_retries_pins_lifted_default`]
/// pin's `.get(FLUX_HELMRELEASE_KEY_REMEDIATION)` probe + the sibling
/// upgrade-path
/// [`cluster_bundle_helmrelease_upgrade_remediation_retries_pins_lifted_default`]
/// pin's peer probe) now consult the same `&'static str`. Until this
/// lift landed the axis carried four inline `remediation` literals across
/// the two production emit sites and the two test-fixture navigation
/// sites; a future hypothetical Flux v3 rename (`recovery` /
/// `retryPolicy` / `errorHandling`) on any production-emit site without
/// a coordinated edit on the sibling navigation sites would have
/// silently stripped the whole per-path remediation sub-block from the
/// emitted `HelmRelease` CR, letting the helm-controller fall back to
/// the Flux v2 upstream defaults for the whole remediation surface
/// rather than the substrate's chosen ceiling with no diagnostic naming
/// the sub-container-key-drift root cause. Same shape as the sibling
/// [`FLUX_HELMRELEASE_KEY_RETRIES`] (a12f9fc) leaf-scalar-key half plus
/// the peer [`FLUX_KEY_SOURCE_REF`] (236ef01) / [`FLUX_KEY_CHART`] /
/// [`FLUX_KEY_VALUES`] / [`FLUX_KEY_INTERVAL`] /
/// [`FLUX_KEY_HEALTH_CHECKS`] container-axis-key lifts on the peer per-
/// CR container-axis-key surface — extends the discipline from the
/// leaf-scalar-key at the bottom of the two-level nested per-path
/// retry-cap declaration onto the sub-container-axis-key one level up.
pub use caixa_core::FLUX_HELMRELEASE_KEY_REMEDIATION;

/// Canonical Flux v2 `HelmRelease.spec.install` per-CR helm-action-phase
/// discriminator parent-container-axis-key — re-export of the canonical
/// [`caixa_core::FLUX_HELMRELEASE_KEY_INSTALL`] so the Flux v2 helm-
/// controller-side per-CR install-path phase-discriminator parent-
/// container-axis-key lives in exactly one place across every caixa
/// renderer. Pairs with the sibling [`FLUX_HELMRELEASE_KEY_UPGRADE`]
/// per-CR helm-action-phase discriminator parent-container-axis-key on
/// the peer per-CR upgrade-path phase. Peer to the sibling
/// [`FLUX_HELMRELEASE_KEY_REMEDIATION`] (6fe4e7e) sub-container-axis-key
/// hosted beneath both parent-container-axis-keys +
/// [`FLUX_HELMRELEASE_KEY_RETRIES`] (a12f9fc) leaf-scalar-key +
/// [`FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`] (30dcdae) scalar-
/// value halves of the same `(parent-container-key, sub-container-key,
/// leaf-key, scalar-value)` per-path retry-cap declaration quartet —
/// closes the parent-container-axis-key axis on the same per-path retry-
/// cap declaration quartet, so all four halves now live in one place.
/// One production emit site (this crate's [`cluster_bundle`]
/// `helmrelease.yaml` format-string template's install-path sub-block-
/// header, threading the same `&'static str` through a new
/// `{install_key}` named-arg interpolation) plus one test-fixture
/// navigation site in `mod tests` (the install-path
/// [`cluster_bundle_helmrelease_install_remediation_retries_pins_lifted_default`]
/// pin's `.get(FLUX_HELMRELEASE_KEY_INSTALL)` probe) now consult the
/// same `&'static str`. Until this lift landed the axis carried two
/// inline `install` literals across the one production emit site and
/// the one test-fixture navigation site; a future hypothetical Flux v3
/// rename (`initialize` / `apply` / `create` / `first-run`) on the
/// production-emit site without a coordinated edit on the sibling
/// navigation site would have silently stripped the whole install-path
/// per-CR phase block from the emitted `HelmRelease` CR, letting the
/// helm-controller fall back to the Flux v2 upstream defaults for the
/// whole install-path phase surface (the `createNamespace: true` seeder
/// never fires, the per-CR retry-cap ceiling silently drops off the
/// emitted document) rather than the substrate's chosen per-CR install-
/// path knob-set with no diagnostic naming the phase-discriminator-drift
/// root cause. Same shape as the sibling
/// [`FLUX_HELMRELEASE_KEY_REMEDIATION`] (6fe4e7e) sub-container-axis-key +
/// [`FLUX_HELMRELEASE_KEY_RETRIES`] (a12f9fc) leaf-scalar-key + the peer
/// [`FLUX_KEY_SOURCE_REF`] (236ef01) / [`FLUX_KEY_CHART`] /
/// [`FLUX_KEY_VALUES`] / [`FLUX_KEY_INTERVAL`] /
/// [`FLUX_KEY_HEALTH_CHECKS`] container-axis-key lifts on the peer per-
/// CR container-axis-key surface — extends the discipline from the sub-
/// container-axis-key one level up onto the parent-container-axis-key
/// hosting it, so the four-level nested `spec.install.remediation
/// .retries` declaration now resolves through four lifted `&'static str`
/// / `u32` values.
pub use caixa_core::FLUX_HELMRELEASE_KEY_INSTALL;

/// Canonical Flux v2 `HelmRelease.spec.upgrade` per-CR helm-action-phase
/// discriminator parent-container-axis-key — re-export of the canonical
/// [`caixa_core::FLUX_HELMRELEASE_KEY_UPGRADE`] so the Flux v2 helm-
/// controller-side per-CR upgrade-path phase-discriminator parent-
/// container-axis-key lives in exactly one place across every caixa
/// renderer. Pairs with the sibling [`FLUX_HELMRELEASE_KEY_INSTALL`]
/// per-CR helm-action-phase discriminator parent-container-axis-key on
/// the peer per-CR install-path phase. Peer to the sibling
/// [`FLUX_HELMRELEASE_KEY_REMEDIATION`] (6fe4e7e) sub-container-axis-key
/// hosted beneath both parent-container-axis-keys. One production emit
/// site (this crate's [`cluster_bundle`] `helmrelease.yaml` format-
/// string template's upgrade-path sub-block-header, threading the same
/// `&'static str` through a new `{upgrade_key}` named-arg interpolation)
/// plus one test-fixture navigation site in `mod tests` (the upgrade-
/// path
/// [`cluster_bundle_helmrelease_upgrade_remediation_retries_pins_lifted_default`]
/// pin's `.get(FLUX_HELMRELEASE_KEY_UPGRADE)` probe) now consult the
/// same `&'static str`. Until this lift landed the axis carried two
/// inline `upgrade` literals across the one production emit site and
/// the one test-fixture navigation site; a future hypothetical Flux v3
/// rename (`reapply` / `reconcile` / `update` / `promote`) on the
/// production-emit site without a coordinated edit on the sibling
/// navigation site would have silently stripped the whole upgrade-path
/// per-CR phase block from the emitted `HelmRelease` CR, letting the
/// helm-controller fall back to the Flux v2 upstream defaults for the
/// whole upgrade-path phase surface (the substrate's
/// `remediateLastFailure: true` toggle never fires, the per-CR retry-
/// cap ceiling silently drops off the emitted document) rather than the
/// substrate's chosen per-CR upgrade-path knob-set with no diagnostic
/// naming the phase-discriminator-drift root cause. Same shape as the
/// sibling [`FLUX_HELMRELEASE_KEY_INSTALL`] parent-container-axis-key
/// on the peer per-CR install-path phase — pairs with the install-path
/// re-export to close the per-CR helm-action-phase discriminator
/// parent-container-axis-key pair across both per-CR phases the helm-
/// controller reconciles between.
pub use caixa_core::FLUX_HELMRELEASE_KEY_UPGRADE;

/// Canonical Flux v2 `HelmRelease.spec.upgrade.remediation.remediateLastFailure`
/// upgrade-path-only per-CR remediation-toggle leaf-scalar-key — re-export
/// of the canonical [`caixa_core::FLUX_HELMRELEASE_KEY_REMEDIATE_LAST_FAILURE`]
/// so the Flux v2 helm-controller-side upgrade-path per-CR remediation-
/// toggle leaf-scalar-key lives in exactly one place across every caixa
/// renderer. Sibling to the peer [`FLUX_HELMRELEASE_KEY_RETRIES`] per-CR
/// retry-cap leaf-scalar-key at the same per-CR upgrade-path per-CR
/// remediation sub-container position — closes the
/// `spec.upgrade.remediation.{retries, remediateLastFailure}` per-path
/// remediation-block leaf-scalar-key pair the substrate seeds into every
/// emitted per-caixa `HelmRelease` CR on the upgrade-path per-CR
/// remediation block. One production emit site (this crate's
/// [`cluster_bundle`] `helmrelease.yaml` format-string template's
/// upgrade-path remediation-toggle leaf under the sibling
/// [`FLUX_HELMRELEASE_KEY_REMEDIATION`]-container-keyed sub-block,
/// threading the same `&'static str` through a new
/// `{remediate_last_failure_key}` named-arg interpolation) plus one test-
/// fixture navigation site in `mod tests` (the upgrade-path
/// [`cluster_bundle_helmrelease_upgrade_remediation_remediate_last_failure_pins_lifted_true`]
/// pin's `.get(FLUX_HELMRELEASE_KEY_REMEDIATE_LAST_FAILURE)` probe) now
/// consult the same `&'static str`. Until this lift landed the axis
/// carried the load-bearing `remediateLastFailure` bytes inline at the
/// one production emit site; a future hypothetical Flux v3 rename
/// (`rollbackOnFailure` / `remediateOnFailure` / `recoverLastFailure`)
/// on the production-emit site without a coordinated edit on every
/// per-renderer consumer the absorption roadmap surfaces (the M4
/// `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's per-Aplicacao
/// `HelmRelease` synthesis) would have silently dropped the substrate's
/// chosen post-retry-exhaustion rollback semantic from every emitted
/// per-caixa `HelmRelease` document — the helm-controller would then
/// leave every terminally-failed upgrade in the failed state without
/// rolling back to the prior last-known-good release the substrate's
/// "no chart apply leaves a per-caixa CR in a stalled, unremediated
/// state" MESH-COMPOSITION.md §V guarantee mandates, with no diagnostic
/// naming the remediation-toggle-drift root cause far from the source
/// caixa.lisp / the renderer's format-string template. Same shape as
/// the sibling [`FLUX_HELMRELEASE_KEY_RETRIES`] (a12f9fc) leaf-scalar-
/// key + the peer [`FLUX_HELMRELEASE_KEY_REMEDIATION`] (6fe4e7e) sub-
/// container-axis-key + [`FLUX_HELMRELEASE_KEY_INSTALL`] /
/// [`FLUX_HELMRELEASE_KEY_UPGRADE`] (7767c26) parent-container-axis-key
/// pair lifts on the peer per-CR remediation-surface leaf-scalar-key /
/// container-axis-key surface — extends the discipline from the sibling
/// retry-cap leaf-scalar-key onto the co-resident remediation-toggle
/// leaf-scalar-key at the same `spec.upgrade.remediation.*` position.
pub use caixa_core::FLUX_HELMRELEASE_KEY_REMEDIATE_LAST_FAILURE;

/// Canonical Flux v2 `HelmRelease.spec.install.createNamespace` install-path-
/// only per-CR namespace-seeder-toggle leaf-scalar-key — re-export of the
/// canonical [`caixa_core::FLUX_HELMRELEASE_KEY_CREATE_NAMESPACE`] so the
/// Flux v2 helm-controller-side install-path per-CR namespace-seeder-toggle
/// leaf-scalar-key lives in exactly one place across every caixa renderer.
/// Peer to the sibling [`FLUX_HELMRELEASE_KEY_REMEDIATE_LAST_FAILURE`]
/// (96581b7) upgrade-path-only per-CR remediation-toggle leaf-scalar-key at
/// the mirror-symmetric parent-container-axis-key position — closes the
/// `spec.{install.createNamespace, upgrade.remediation.remediateLastFailure}`
/// per-path per-CR phase-specific toggle leaf-scalar-key pair the substrate
/// seeds into every emitted per-caixa `HelmRelease` CR. One production emit
/// site (this crate's [`cluster_bundle`] `helmrelease.yaml` format-string
/// template's install-path namespace-seeder-toggle leaf under the sibling
/// [`FLUX_HELMRELEASE_KEY_INSTALL`]-container-keyed sub-block, threading
/// the same `&'static str` through a new `{create_namespace_key}` named-
/// arg interpolation) plus one test-fixture navigation site in `mod tests`
/// (the install-path
/// [`cluster_bundle_helmrelease_install_create_namespace_pins_lifted_true`]
/// pin's `.get(FLUX_HELMRELEASE_KEY_CREATE_NAMESPACE)` probe) now consult
/// the same `&'static str`. Until this lift landed the axis carried the
/// load-bearing `createNamespace` bytes inline at the one production emit
/// site; a future hypothetical Flux v3 rename (`createTargetNamespace` /
/// `seedNamespace` / `provisionNamespace`) on the production-emit site
/// without a coordinated edit on every per-renderer consumer the
/// absorption roadmap surfaces (the M4 `mesh.pleme.io/v1alpha1/Aplicacao`
/// CR materializer's per-Aplicacao `HelmRelease` synthesis) would have
/// silently dropped the substrate's chosen first-apply namespace-seeder
/// semantic from every emitted per-caixa `HelmRelease` document — the
/// helm-controller would then refuse every first-time per-caixa chart
/// apply against a fresh cluster whose target namespace has not been
/// pre-provisioned by an out-of-band pipeline the substrate's "no per-
/// caixa Servico apply is blocked on manual namespace preprovisioning"
/// MESH-COMPOSITION.md §V install-path-fluency guarantee mandates, with no
/// diagnostic naming the seeder-toggle-drift root cause far from the
/// source caixa.lisp / the renderer's format-string template. Same shape
/// as the sibling [`FLUX_HELMRELEASE_KEY_REMEDIATE_LAST_FAILURE`] (96581b7)
/// upgrade-path-only per-CR remediation-toggle leaf-scalar-key +
/// [`FLUX_HELMRELEASE_KEY_RETRIES`] (a12f9fc) leaf-scalar-key + peer
/// [`FLUX_HELMRELEASE_KEY_REMEDIATION`] (6fe4e7e) sub-container-axis-key +
/// [`FLUX_HELMRELEASE_KEY_INSTALL`] / [`FLUX_HELMRELEASE_KEY_UPGRADE`]
/// (7767c26) parent-container-axis-key pair lifts on the peer per-CR
/// HelmRelease-spec surface — extends the discipline from the sibling
/// upgrade-path-only per-CR remediation-toggle onto the mirror install-
/// path-only per-CR namespace-seeder-toggle at the `spec.install.*`
/// position mirroring the peer's `spec.upgrade.remediation.*` position.
pub use caixa_core::FLUX_HELMRELEASE_KEY_CREATE_NAMESPACE;

/// Canonical Flux v2 `Kustomization.spec.prune` per-CR garbage-collection-
/// toggle leaf-scalar-key — re-export of the canonical
/// [`caixa_core::FLUX_KUSTOMIZATION_KEY_PRUNE`] so the Flux v2 kustomize-
/// controller-side per-CR garbage-collection-toggle leaf-scalar-key lives
/// in exactly one place across every caixa renderer. Peer to the sibling
/// [`FLUX_HELMRELEASE_KEY_CREATE_NAMESPACE`] (ba9ab8b) install-path-only
/// per-CR namespace-seeder-toggle leaf-scalar-key on the co-resident
/// per-caixa `HelmRelease` CR — extends the per-CR-toggle leaf-scalar-key
/// discipline from the co-resident per-caixa `HelmRelease` CR spec surface
/// onto the co-resident per-caixa `Kustomization` CR spec surface at the
/// mirror-symmetric top-level `spec.prune` position. One production emit
/// site (this crate's [`cluster_bundle`] `kustomization.yaml` format-
/// string template's per-CR garbage-collection-toggle leaf under the
/// top-level `spec` position, threading the same `&'static str` through a
/// new `{prune_key}` named-arg interpolation) plus one test-fixture
/// navigation site in `mod tests` (the per-CR
/// [`cluster_bundle_kustomization_prune_pins_lifted_true`] pin's
/// `.get(FLUX_KUSTOMIZATION_KEY_PRUNE)` probe) now consult the same
/// `&'static str`. Until this lift landed the axis carried the load-
/// bearing `prune` bytes inline at the one production emit site; a
/// future hypothetical Flux v3 rename (`garbageCollect` / `sweep` /
/// `pruneOrphaned` / `deleteOrphans`) on the production-emit site
/// without a coordinated edit on every per-renderer consumer the
/// absorption roadmap surfaces (the M4 `mesh.pleme.io/v1alpha1/Aplicacao`
/// CR materializer's per-Aplicacao `Kustomization` synthesis) would have
/// silently dropped the substrate's chosen sweep-what-you-removed
/// semantic from every emitted per-caixa `Kustomization` document — the
/// kustomize-controller would then leave every per-caixa resource the
/// source manifest set previously reconciled but no longer carries
/// dangling in the cluster the substrate's "the cluster's per-caixa live
/// state converges to the caixa's tatara-lisp source-of-truth on every
/// reconcile — resources the source no longer carries are swept by the
/// kustomize-controller, not left dangling" CAIXA-SDLC.md §V author-to-
/// live-convergence guarantee mandates, with no diagnostic naming the
/// toggle-drift root cause far from the source caixa.lisp / the
/// renderer's format-string template. Same shape as the sibling
/// [`FLUX_HELMRELEASE_KEY_CREATE_NAMESPACE`] (ba9ab8b) install-path-only
/// per-CR namespace-seeder-toggle leaf-scalar-key +
/// [`FLUX_HELMRELEASE_KEY_REMEDIATE_LAST_FAILURE`] (96581b7) upgrade-
/// path-only per-CR remediation-toggle leaf-scalar-key +
/// [`FLUX_HELMRELEASE_KEY_RETRIES`] (a12f9fc) leaf-scalar-key + peer
/// [`FLUX_HELMRELEASE_KEY_REMEDIATION`] (6fe4e7e) sub-container-axis-key
/// + [`FLUX_HELMRELEASE_KEY_INSTALL`] / [`FLUX_HELMRELEASE_KEY_UPGRADE`]
/// (7767c26) parent-container-axis-key pair lifts on the peer per-CR
/// HelmRelease-spec surface — extends the discipline from the co-
/// resident per-`HelmRelease`-CR spec surface onto the co-resident per-
/// `Kustomization`-CR spec surface at the mirror-symmetric top-level
/// `spec.prune` position.
pub use caixa_core::FLUX_KUSTOMIZATION_KEY_PRUNE;

/// Canonical Flux v2 `Kustomization.spec.path` per-CR source-sub-tree
/// leaf-scalar-key — re-export of the canonical
/// [`caixa_core::FLUX_KUSTOMIZATION_KEY_PATH`] so the Flux v2 kustomize-
/// controller-side per-CR source-sub-tree leaf-scalar-key lives in
/// exactly one place across every caixa renderer. Peer to the sibling
/// co-resident [`FLUX_KUSTOMIZATION_KEY_PRUNE`] (8ec7917) per-CR
/// garbage-collection-toggle leaf-scalar-key on the same top-level
/// `spec` position of the emitted per-caixa `Kustomization` CR — extends
/// the per-`Kustomization`-CR-spec leaf-scalar-key discipline from the
/// co-resident garbage-collection-toggle axis onto the co-resident
/// source-sub-tree axis at the mirror-symmetric top-level `spec.path`
/// position. One production emit site (this crate's [`cluster_bundle`]
/// `kustomization.yaml` format-string template's per-CR source-sub-tree
/// leaf under the top-level `spec` position, threading the same
/// `&'static str` through a new `{path_key}` named-arg interpolation)
/// plus one test-fixture navigation site in `mod tests` (the per-CR
/// [`cluster_bundle_kustomization_path_pins_lifted_sub_tree`] pin's
/// `.get(FLUX_KUSTOMIZATION_KEY_PATH)` probe) now consult the same
/// `&'static str`. Until this lift landed the axis carried the load-
/// bearing `path` bytes inline at the one production emit site; a
/// future hypothetical Flux v3 rename (`sourcePath` / `manifestsPath` /
/// `sourceRoot`) on the production-emit site without a coordinated edit
/// on every per-renderer consumer the absorption roadmap surfaces (the
/// M4 `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's per-
/// Aplicacao `Kustomization` synthesis) would have silently unbound
/// every emitted per-caixa `Kustomization` from its paired per-caixa
/// sub-tree of the pleme-io k8s repository — the Flux v2 kustomize-
/// controller would then either reconcile the whole GitRepository root
/// (defaulting to `./` when the CR omits the leaf, pulling every
/// unrelated cluster's manifests through the wrong per-caixa
/// `Kustomization`) or refuse to reconcile at all (parking the CR at
/// `BuildFailed` naming the missing sub-tree far from the source
/// `caixa.lisp` / the renderer's format-string template).
pub use caixa_core::FLUX_KUSTOMIZATION_KEY_PATH;

/// Canonical substrate-side per-cluster / per-caixa
/// `Kustomization.spec.path` source-sub-tree scalar composer — re-export
/// of the canonical [`caixa_core::flux_kustomization_source_subtree`]
/// so the `./clusters/<cluster>/services/<nome>` GitRepository-relative
/// directory-tree seed every emitted per-caixa `kustomization.yaml`
/// document mounts under its lifted [`FLUX_KUSTOMIZATION_KEY_PATH`]
/// leaf-scalar-key lives at one composer across every caixa renderer.
///
/// Peer to [`FLUX_KUSTOMIZATION_KEY_PATH`] (613d7ed) — the leaf-scalar-
/// key half of the `spec.path` `(key, value)` per-CR pair — on the
/// paired value-half axis. Until this lift landed the two-axis
/// composition (the `./clusters/` per-cluster prefix + the `/services/`
/// per-caixa infix) sat as a verbatim inline
/// `format!("./clusters/{cluster}/services/{name}")` at the sole
/// [`cluster_bundle`] `kustomization.yaml` format-string production
/// emit site plus a mirror-symmetric verbatim inline
/// `format!("./clusters/{cluster}/services/{name}", …)` at its paired
/// `cluster_bundle_kustomization_path_pins_lifted_sub_tree` test-fixture
/// navigation site, with no compile-time link between the two sites and
/// no compile-time link ahead of the second production-emit occurrence
/// the M4 `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's per-
/// Aplicacao `Kustomization` synthesis will surface. Both sites now
/// consult the same canonical composer, so a future substrate-side
/// directory-tree axis rebrand (`clusters/` → `environments/` for a
/// multi-env-per-cluster axis extension, `services/` → `servicos/` for
/// a portuguese-canonical directory-name migration matching the sibling
/// `:servicos` slot spelling, a per-tenant scoping prefix for multi-
/// tenant Aplicacao hosting) is one edit at the composer, not a
/// coordinated sweep across every renderer's `spec.path` emit site.
/// Same shape as the sibling [`caixa_core::oci_chart_ref`] /
/// [`caixa_core::cilium_network_policy_name`] /
/// [`caixa_core::gateway_api_http_route_name`] composer re-exports on
/// the peer substrate-side canonical-load-bearing-scalar-that-consumers-
/// key-off axis.
pub use caixa_core::flux_kustomization_source_subtree;

/// Canonical Flux v2 `Kustomization.spec.timeout` per-CR reconcile
/// wall-clock cap leaf-scalar-key — re-export of the canonical
/// [`caixa_core::FLUX_KUSTOMIZATION_KEY_TIMEOUT`] so the Flux v2
/// kustomize-controller-side per-CR reconcile wall-clock cap leaf-
/// scalar-key lives in exactly one place across every caixa
/// renderer. Peer to the co-resident
/// [`FLUX_KUSTOMIZATION_KEY_PATH`] (613d7ed) per-CR source-sub-tree
/// leaf-scalar-key and [`FLUX_KUSTOMIZATION_KEY_PRUNE`] (8ec7917)
/// per-CR garbage-collection-toggle leaf-scalar-key on the same top-
/// level `spec` position of the emitted per-caixa `Kustomization` CR
/// — extends the per-`Kustomization`-CR-spec leaf-scalar-key
/// discipline from the co-resident source-sub-tree and garbage-
/// collection-toggle axes onto the co-resident reconcile wall-clock
/// cap axis at the mirror-symmetric top-level `spec.timeout`
/// position. One production emit site (this crate's
/// [`cluster_bundle`] `kustomization.yaml` format-string template's
/// per-CR reconcile wall-clock cap leaf under the top-level `spec`
/// position, threading the same `&'static str` through a new
/// `{timeout_key}` named-arg interpolation) plus one test-fixture
/// navigation site in `mod tests` (the per-CR
/// [`cluster_bundle_kustomization_timeout_pins_lifted_default`] pin's
/// `.get(FLUX_KUSTOMIZATION_KEY_TIMEOUT)` probe) now consult the same
/// `&'static str`. Until this lift landed the axis carried the load-
/// bearing `timeout` bytes inline at the one production emit site; a
/// future hypothetical Flux v3 rename (`deadline` / `reconcileTimeout`
/// / `maxDuration`) on the production-emit site without a coordinated
/// edit on every per-renderer consumer the absorption roadmap
/// surfaces (the M4 `mesh.pleme.io/v1alpha1/Aplicacao` CR
/// materializer's per-Aplicacao `Kustomization` synthesis) would have
/// silently stripped the substrate's chosen reconcile-ceiling
/// declaration from every emitted per-caixa `Kustomization` document
/// — the Flux v2 kustomize-controller would then fall back to the
/// upstream Flux v2 controller-side default cap, letting a
/// persistently-failing per-caixa manifest apply consume kustomize-
/// controller reconcile-loop cycles past the substrate's chosen
/// ceiling with no field naming the leaf-key-drift root cause far
/// from the source `caixa.lisp` / the renderer's format-string
/// template. Pairs with the sibling
/// [`DEFAULT_FLUX_KUSTOMIZATION_TIMEOUT`] scalar-value half of the
/// same `(leaf-key, scalar-value)` per-path reconcile-ceiling-
/// declaration pair. Same shape as the sibling
/// [`FLUX_KUSTOMIZATION_KEY_PATH`] (613d7ed) /
/// [`FLUX_KUSTOMIZATION_KEY_PRUNE`] (8ec7917) /
/// [`FLUX_HELMRELEASE_KEY_REMEDIATE_LAST_FAILURE`] (96581b7) /
/// [`FLUX_HELMRELEASE_KEY_CREATE_NAMESPACE`] (ba9ab8b) leaf-scalar-
/// key re-exports on the peer per-Flux-v2-CR-spec-leaf-scalar-key
/// surface.
pub use caixa_core::FLUX_KUSTOMIZATION_KEY_TIMEOUT;

/// Canonical Flux v2 `Kustomization.spec.timeout` per-CR reconcile
/// wall-clock cap default the substrate seeds into every per-caixa
/// `kustomization.yaml` document. Re-export of the canonical
/// [`caixa_core::DEFAULT_FLUX_KUSTOMIZATION_TIMEOUT`] so the Flux v2
/// kustomize-controller-side per-CR reconcile-ceiling default scalar-
/// value lives in exactly one place across every caixa renderer —
/// [`cluster_bundle`]'s `kustomization.yaml` format-string template's
/// sole production-code emit site (the prior inline `timeout: 5m`
/// scalar-value literal under the top-level `spec` position, keyed by
/// the sibling [`FLUX_KUSTOMIZATION_KEY_TIMEOUT`] leaf-scalar-key)
/// now consumes the same `&'static str` at emit time through one
/// `{timeout_default}` named-arg interpolation, so a future
/// substrate-side reconcile-ceiling migration (`"5m"` → `"3m"` on
/// faster per-caixa idempotency-checkpoint cadence, `"5m"` → `"10m"`
/// on larger per-caixa manifest sets — coordinated with the sibling
/// [`DEFAULT_FLUX_RECONCILE_INTERVAL`] reconcile-poll cadence tuning
/// cycle) is a one-line edit on the canonical
/// [`caixa_core::DEFAULT_FLUX_KUSTOMIZATION_TIMEOUT`] declaration, not
/// a coordinated rewrite across the emit site + every future per-CR
/// reconcile-cap consumer the substrate adds. Pairs with the sibling
/// [`FLUX_KUSTOMIZATION_KEY_TIMEOUT`] re-export on the same per-CR
/// `spec.timeout` scalar-axis — the key half of the per-CR scalar-
/// key/scalar-value pair lives at [`FLUX_KUSTOMIZATION_KEY_TIMEOUT`],
/// the value half's substrate-side default seed lives here. Same
/// shape as the sibling [`DEFAULT_FLUX_RECONCILE_INTERVAL`] (908180f)
/// / [`FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`] (30dcdae) re-
/// exports on the peer canonical-substrate-default-load-bearing-
/// scalar surface.
pub use caixa_core::DEFAULT_FLUX_KUSTOMIZATION_TIMEOUT;

/// Canonical FluxCD installation namespace — re-export of the lifted
/// [`caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE`] so the load-bearing
/// string lives in exactly one place across every consumer of the
/// rendered `kustomization.yaml`'s `metadata.namespace` and
/// `spec.sourceRef.name` axes. Both axes are the same conceptual "Flux
/// installation namespace" the `flux bootstrap` pipeline names; until
/// this lift landed they sat as two inline `flux-system` literals inside
/// [`cluster_bundle`]'s `kustomization.yaml` format-string template, and
/// any future per-edition Flux-installation-namespace rebrand on one
/// without a coordinated edit on the other would have silently emitted a
/// `Kustomization` outside the bootstrap controller's watch window or a
/// dangling `sourceRef`. Same shape as the
/// [`caixa_core::DEFAULT_NAMESPACE`] (a085b26) /
/// [`caixa_core::DEFAULT_LIBRARY_NAME`] (41438dc) /
/// [`caixa_core::DEFAULT_PUBLISH_TAG_PREFIX`] (0a6a602) lifts on the peer
/// canonical-load-bearing-string surface.
pub use caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE;

/// Canonical FluxCD `HelmRelease` CRD `apiVersion` — re-export of the
/// lifted [`caixa_core::FLUX_HELMRELEASE_API_VERSION`] so the load-bearing
/// string lives in exactly one place across the rendered Flux bundle's
/// `helmrelease.yaml` document `apiVersion` axis + the rendered
/// `kustomization.yaml` document's `spec.healthChecks[].apiVersion` axis.
/// Both axes are the same conceptual "Flux v2 `HelmRelease` CRD group/
/// version" load-bearing string and must move together on any future
/// upstream Flux v3 migration; until this lift landed they sat as four
/// inline `helm.toolkit.fluxcd.io/v2` literals (two render-side at lines
/// 455, 504 + two test-fixture-side at lines 928, 970), and any future
/// per-Flux-v3-migration version bump on one without a coordinated edit
/// on the other would have silently routed the rendered `HelmRelease`
/// outside the controller's `Watches` (controller-side: never reconciled,
/// every dependent chart frozen at last-applied state) or made the
/// `Kustomization`'s health-check dangle (apply-side: the per-resource
/// health-gate never resolves, the parent Kustomization sits perpetually
/// in `Reconciling`). Same shape as the
/// [`caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE`] (7197d38) lift on the
/// sibling Flux-installation-namespace axis.
pub use caixa_core::FLUX_HELMRELEASE_API_VERSION;

/// Canonical FluxCD `GitRepository` CRD `apiVersion` — re-export of the
/// lifted [`caixa_core::FLUX_GITREPOSITORY_API_VERSION`] so the load-bearing
/// string lives in exactly one place across the rendered Flux bundle's
/// `gitrepository.yaml` document `apiVersion` axis. Until this lift landed
/// the axis sat as an inline `source.toolkit.fluxcd.io/v1` literal at line
/// 436 of this crate's [`cluster_bundle`] format-string template, and any
/// future per-Flux-v3-migration version bump on this axis without a
/// coordinated edit on the sibling [`FLUX_HELMRELEASE_API_VERSION`] axis
/// would have silently routed the rendered `GitRepository` outside the
/// Flux v2 `source-controller`'s `Watches` (controller-side: never
/// reconciled, the dependent HelmRelease's `chart: sourceRef` dangles,
/// every per-Servico apply silently comes up with the prior reconciled
/// state). Same shape as the [`FLUX_HELMRELEASE_API_VERSION`] (55f0fd9) /
/// [`caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE`] (7197d38) lifts on the
/// sibling Flux-v2-load-bearing-string axis.
pub use caixa_core::FLUX_GITREPOSITORY_API_VERSION;

/// Canonical FluxCD `Kustomization` CRD `apiVersion` — re-export of the
/// lifted [`caixa_core::FLUX_KUSTOMIZATION_API_VERSION`] so the load-
/// bearing string lives in exactly one place across the rendered Flux
/// bundle's `kustomization.yaml` document `apiVersion` axis. Completes
/// the Flux v2 controller-triplet (source-controller +
/// helm-controller + kustomize-controller) lift alongside the sibling
/// [`FLUX_GITREPOSITORY_API_VERSION`] (8a6c8a3) and
/// [`FLUX_HELMRELEASE_API_VERSION`] (55f0fd9) re-exports — every
/// per-controller CRD-group/version is now a typed substrate-side
/// `&'static str` consumed through one `pub use caixa_core::FLUX_*`
/// re-export.
///
/// Until this lift landed the axis sat as an inline
/// `kustomize.toolkit.fluxcd.io/v1` literal in this crate's
/// [`cluster_bundle`] `kustomization.yaml` format-string template,
/// and any future per-Flux-v3-migration version bump on this axis
/// without a coordinated edit on the sibling
/// [`FLUX_HELMRELEASE_API_VERSION`] / [`FLUX_GITREPOSITORY_API_VERSION`]
/// axes (the Flux v2 controller triplet's CRD group/versions move
/// together upstream) would have silently routed the rendered
/// `Kustomization` outside the Flux v2 `kustomize-controller`'s
/// `Watches` (apply-side: the parent Kustomization is never
/// reconciled, every dependent `HelmRelease` / `GitRepository` it
/// keys off sits perpetually un-applied at the cluster). Same shape
/// as the [`FLUX_GITREPOSITORY_API_VERSION`] (8a6c8a3) /
/// [`FLUX_HELMRELEASE_API_VERSION`] (55f0fd9) /
/// [`caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE`] (7197d38) lifts on
/// the sibling Flux-v2-load-bearing-string axis.
pub use caixa_core::FLUX_KUSTOMIZATION_API_VERSION;

/// Canonical FluxCD `GitRepository` CRD `kind` discriminator — re-export
/// of the lifted [`caixa_core::FLUX_KIND_GIT_REPOSITORY`] so the load-
/// bearing string lives in exactly one place across the three rendered
/// Flux bundle axes that name the same K8s CRD discriminator:
///
///   - the rendered `gitrepository.yaml` document's top-level `kind`
///     axis (the GitRepository CR's own discriminator);
///   - the rendered `helmrelease.yaml` document's
///     `spec.chart.spec.sourceRef.kind` axis (pointing back at the
///     sibling GitRepository this chart sources from);
///   - the rendered `kustomization.yaml` document's `spec.sourceRef.kind`
///     axis (pointing back at the cluster's bootstrap GitRepository).
///
/// Until this lift landed the three axes sat as three inline
/// `GitRepository` literals across the [`cluster_bundle`] `gitrepo` +
/// `helmrelease` + `kustomization` format-string templates (caixa-flux
/// /src/lib.rs:505, 556, 591). The apiserver-side CRD resolution
/// contract is the `(apiVersion, kind)` tuple keyed against the
/// registered `CustomResourceDefinition`; a typo at any one of the three
/// call sites would have silently dangled the corresponding `sourceRef`
/// at apply time (the `helm-controller` never resolves a chart for the
/// HelmRelease, the `kustomize-controller` never reconciles the parent
/// Kustomization, every per-Servico apply silently comes up with the
/// prior reconciled state) with no diagnostic naming the kind-drift root
/// cause far from the source caixa.lisp. Same shape as the
/// [`FLUX_GITREPOSITORY_API_VERSION`] (8a6c8a3) /
/// [`FLUX_HELMRELEASE_API_VERSION`] (55f0fd9) /
/// [`FLUX_KUSTOMIZATION_API_VERSION`] (d2dd1b1) lifts on the sibling
/// apiVersion half of the same `(apiVersion, kind)` CRD-lookup tuple —
/// extends the discipline from the apiVersion axis of the Flux v2
/// source-controller CRD onto its kind axis.
pub use caixa_core::FLUX_KIND_GIT_REPOSITORY;

/// Canonical FluxCD `HelmRelease` CRD `kind` discriminator — re-export
/// of the lifted [`caixa_core::FLUX_KIND_HELM_RELEASE`] so the load-
/// bearing string lives in exactly one place across the two rendered
/// Flux bundle axes that name the same K8s CRD discriminator:
///
///   - the rendered `helmrelease.yaml` document's top-level `kind`
///     axis (the HelmRelease CR's own discriminator);
///   - the rendered `kustomization.yaml` document's
///     `spec.healthChecks[].kind` axis (pointing back at the sibling
///     HelmRelease the Kustomization health-gates on before declaring
///     its own reconcile complete).
///
/// Until this lift landed the two axes sat as two inline `HelmRelease`
/// literals across the [`cluster_bundle`] `helmrelease` + `kustomization`
/// format-string templates (caixa-flux/src/lib.rs:580, 631). The
/// apiserver-side CRD resolution contract is the `(apiVersion, kind)`
/// tuple keyed against the registered `CustomResourceDefinition`; a
/// typo at either of the two call sites would have silently lost the
/// apply-side resolution (top-level `kind` typos surface as "no kind
/// 'X' is registered" at apply parse time; the nested
/// `healthChecks[].kind` typo silently dangles the parent
/// Kustomization at `Reconciling` forever) with no diagnostic naming
/// the kind-drift root cause far from the source caixa.lisp. Same
/// shape as the [`FLUX_KIND_GIT_REPOSITORY`] (dbbcf29) /
/// [`FLUX_HELMRELEASE_API_VERSION`] (55f0fd9) /
/// [`FLUX_GITREPOSITORY_API_VERSION`] (8a6c8a3) /
/// [`FLUX_KUSTOMIZATION_API_VERSION`] (d2dd1b1) lifts on the sibling
/// Flux-v2-load-bearing-string axes — extends the kind-axis discipline
/// from the Flux v2 source-controller CRD onto the sibling Flux v2
/// helm-controller CRD.
pub use caixa_core::FLUX_KIND_HELM_RELEASE;

/// Canonical FluxCD `Kustomization` CRD `kind` discriminator — re-export
/// of the lifted [`caixa_core::FLUX_KIND_KUSTOMIZATION`] so the load-
/// bearing string lives in exactly one place across the rendered Flux
/// bundle's `Kustomization`-naming axis:
///
///   - the rendered `kustomization.yaml` document's top-level `kind`
///     axis (the Kustomization CR's own discriminator).
///
/// Until this lift landed the axis sat as an inline `Kustomization`
/// literal inside the [`cluster_bundle`] `kustomization` format-string
/// template (caixa-flux/src/lib.rs:651). The apiserver-side CRD
/// resolution contract is the `(apiVersion, kind)` tuple keyed against
/// the registered `CustomResourceDefinition`; a typo at this call site
/// would have surfaced at apply parse time as a non-self-locating "no
/// kind 'Kustomizaton' is registered for version
/// 'kustomize.toolkit.fluxcd.io/v1'" error, with the rendered parent
/// Kustomization never reconciling and every downstream per-Servico
/// `dependsOn` chain freezing at the kustomize-controller's CRD-lookup
/// boundary. Same shape as the [`FLUX_KIND_GIT_REPOSITORY`] (dbbcf29) /
/// [`FLUX_KIND_HELM_RELEASE`] (e24ea3c) /
/// [`FLUX_KUSTOMIZATION_API_VERSION`] (d2dd1b1) /
/// [`FLUX_GITREPOSITORY_API_VERSION`] (8a6c8a3) /
/// [`FLUX_HELMRELEASE_API_VERSION`] (55f0fd9) lifts on the sibling
/// Flux-v2-load-bearing-string axes — extends the kind-axis discipline
/// from the Flux v2 source-controller + helm-controller CRDs onto the
/// sibling Flux v2 kustomize-controller CRD, completing the
/// canonical-Flux-v2-CRD-kind-discriminator lift across the
/// source/helm/kustomize controller triplet.
pub use caixa_core::FLUX_KIND_KUSTOMIZATION;

/// Canonical Flux v2 per-`HelmRelease` inline-chart-template container-
/// axis key every `caixa-flux`-emitted `HelmRelease` document nests its
/// per-CR chart-template block under (`spec.chart` on `HelmRelease`). Re-
/// export of the canonical [`caixa_core::FLUX_KEY_CHART`] so the load-
/// bearing Flux-v2-helm-controller-side per-`HelmRelease` chart-template
/// container-axis key lives in exactly one place across every caixa
/// renderer — the sweep converts this crate's one production-code call
/// site (the [`cluster_bundle`] `helmrelease.yaml` format-string
/// template's baked `chart:\n` container-axis key nesting the
/// `HelmChartTemplate` sub-document whose peer sibling lifted
/// [`FLUX_KEY_SOURCE_REF`] source-reference container-axis + inner
/// chart-name-scalar reach through) plus the two test-fixture
/// `.get("chart")` navigation sites in `mod tests` that probe the
/// emitted `spec.chart.spec.sourceRef.kind` pin onto the re-export. A
/// future Flux v3 rebrand of the per-`HelmRelease` chart-template
/// container-axis key (a hypothetical upstream fluxcd/flux2 rename from
/// `chart` to `Chart` / `chartTemplate` / `helmChart` / `chartRef`,
/// coordinated with the upstream project's per-version deprecation
/// cycle) now lands at one const rather than scattered across the one
/// emit-side format-string template + two test-fixture probe sites.
/// Same "the typed constant lives in one place" discipline the peer
/// [`FLUX_KEY_SOURCE_REF`] + [`FLUX_KEY_VALUES`] re-exports enforce on
/// the sibling Flux v2 per-`HelmRelease` body-key surfaces — completes
/// the triplet of Flux v2 per-`HelmRelease` `spec.*` body-key constants
/// (`spec.chart` + `spec.chart.spec.sourceRef` + `spec.values`) the
/// `cluster_bundle` renderer threads through its `helmrelease.yaml`
/// format-string template.
pub use caixa_core::FLUX_KEY_CHART;

/// Canonical Flux v2 `HelmChartTemplate.spec.chart` per-CR chart-NAME-
/// reference leaf-scalar-axis key every `caixa-flux`-emitted
/// `HelmRelease` document nests inside the parent [`FLUX_KEY_CHART`]
/// container-axis (a nested [`KUBE_KEY_SPEC`] axis inside that
/// container hosts this leaf plus its sibling [`FLUX_KEY_SOURCE_REF`]
/// per-CR source-reference triple). Re-export of the canonical
/// [`caixa_core::FLUX_HELMCHART_TEMPLATE_KEY_CHART`] so the load-
/// bearing Flux-v2-helm-controller-side per-`HelmChartTemplate` chart-
/// NAME reference leaf-scalar-axis key lives in exactly one place
/// across every caixa renderer — the sweep converts this crate's one
/// production-code call site (the [`cluster_bundle`] `helmrelease.yaml`
/// format-string template's baked `chart: {chart_path}` leaf-scalar-
/// axis interpolation the peer sibling lifted [`FLUX_KEY_SOURCE_REF`]
/// source-reference triple's per-CR source-artifact publishes) onto
/// the re-export.
///
/// A future Flux v3 rebrand of the per-`HelmChartTemplate` chart-NAME
/// reference leaf-scalar-axis key (a hypothetical upstream fluxcd/flux2
/// rename from `chart` to `Chart` / `chartRef` / `chartName`,
/// coordinated with the upstream project's per-version deprecation
/// cycle) now lands at one const rather than scattered across the one
/// emit-side format-string template site. Same "the typed constant
/// lives in one place" discipline the peer [`FLUX_KEY_CHART`] +
/// [`FLUX_KEY_SOURCE_REF`] + [`FLUX_KEY_VALUES`] re-exports enforce
/// on the sibling Flux v2 per-`HelmRelease` body-key surfaces —
/// completes the per-`HelmRelease` chart-template `(spec.chart →
/// spec.chart.spec.chart + spec.chart.spec.sourceRef)` axis chain by
/// descending one level beneath the parent container-axis re-export
/// the sibling [`FLUX_KEY_CHART`] anchors.
///
/// Deliberate axis-independence discipline with the parent
/// [`FLUX_KEY_CHART`] container-axis re-export: both re-exports carry
/// the same underlying `"chart"` string today but name distinct schema
/// axes on the same CRD group (a container-axis parent vs a leaf-
/// scalar grandchild inside it), so the two `pub use` re-exports stay
/// sibling constants at the rustc symbol-name axis rather than
/// coalescing onto one canonical declaration. Peer to the deliberate
/// [`CILIUM_KEY_PATH`] / [`caixa_core::GATEWAY_API_KEY_PATH`] axis-
/// independence discipline the two-CRD-groups-sharing-a-string
/// sibling `"path"` re-exports established on the peer canonical-
/// axis-independence surface.
pub use caixa_core::FLUX_HELMCHART_TEMPLATE_KEY_CHART;

/// Canonical Flux v2 per-`HelmRelease`/`Kustomization` source-reference
/// container-axis key every `caixa-flux`-emitted bundle document mounts
/// its per-CR source-of-truth `(kind, name, namespace)` reference triple
/// under (`spec.chart.spec.sourceRef` on `HelmRelease`, `spec.sourceRef`
/// on `Kustomization`). Re-export of the canonical
/// [`caixa_core::FLUX_KEY_SOURCE_REF`] so the load-bearing Flux-v2-source-
/// controller-side per-CR source-reference container-axis key lives in
/// exactly one place across every caixa renderer — the sweep converts
/// this crate's two production emit sites (the [`cluster_bundle`]
/// `helmrelease.yaml` format-string template's baked
/// `spec.chart.spec.sourceRef:\n` sub-block header + the sibling
/// `kustomization.yaml` format-string template's baked
/// `spec.sourceRef:\n` sub-block header, both now threaded through a
/// `{source_ref_key}` named-arg interpolation on the lifted const, closing
/// the last two open production sites for this axis) plus the five
/// test-fixture `.get("sourceRef")` navigation sites in `mod tests` (the
/// `helmrelease.yaml` `spec.chart.spec.sourceRef.kind` pin, the
/// `kustomization.yaml` `spec.sourceRef.name` + `spec.sourceRef.kind`
/// pins under the paired [`DEFAULT_FLUX_SYSTEM_NAMESPACE`] +
/// [`FLUX_KIND_GIT_REPOSITORY`] canonical-string axes, and the two
/// cross-axis triplet pins that traverse both bundle documents to
/// pin the sibling `GitRepository`-kind axis triplet against one
/// canonical string) onto the re-export. A future Flux v3 rebrand of
/// the per-CR source-reference container-axis key (a hypothetical
/// upstream fluxcd/flux2 rename from `sourceRef` to `source` /
/// `sourceReference` / `sourceOf`, coordinated with the upstream
/// project's per-version deprecation cycle) now lands at one const
/// rather than scattered across the two per-CR format-string templates
/// + five test-fixture probe sites. Same "the typed constant lives in
/// one place" discipline the peer [`FLUX_KIND_GIT_REPOSITORY`] /
/// [`FLUX_KIND_HELM_RELEASE`] / [`FLUX_KIND_KUSTOMIZATION`] +
/// [`FLUX_HELMRELEASE_API_VERSION`] / [`FLUX_GITREPOSITORY_API_VERSION`]
/// / [`FLUX_KUSTOMIZATION_API_VERSION`] re-exports enforce on the
/// sibling canonical-Flux-v2-load-bearing-string surfaces.
pub use caixa_core::FLUX_KEY_SOURCE_REF;

/// Canonical Flux v2 per-`HelmRelease` values-override block-body-axis
/// key every `caixa-flux`-emitted `HelmRelease` document nests its per-
/// cluster value overrides under (`spec.values` on `HelmRelease`). Re-
/// export of the canonical [`caixa_core::FLUX_KEY_VALUES`] so the load-
/// bearing Flux-v2-helm-controller-side per-`HelmRelease` values-
/// override block-body-axis key lives in exactly one place across every
/// caixa renderer — the sweep converts this crate's one production-code
/// call site (the [`cluster_bundle`] `helmrelease.yaml` format-string
/// template's baked `values:\n` key beside the sibling lifted
/// [`DEFAULT_LIBRARY_NAME`] wrap key + [`HELM_VALUES_KEY_ENABLED`]
/// enable-toggle) plus the [`upsert_into_helmrelease_programs`]
/// upsert-path's `spec.values.programs[]` write-side navigation onto the
/// re-export, and threads three test-fixture `.get("values")` block-
/// body-axis probe sites in `mod tests` through the same const. A
/// future Flux v3 rebrand of the per-`HelmRelease` values-override
/// block-body-axis key (a hypothetical upstream fluxcd/flux2 rename
/// from `values` to `Values` / `chartValues` / `overrides`, coordinated
/// with the upstream project's per-version deprecation cycle) now lands
/// at one const rather than scattered across the one emit-side format-
/// string template + one upsert-side write-side navigation + three
/// test-fixture probe sites. Same "the typed constant lives in one
/// place" discipline the peer [`FLUX_KEY_SOURCE_REF`] +
/// [`FLUX_KIND_GIT_REPOSITORY`] / [`FLUX_KIND_HELM_RELEASE`] /
/// [`FLUX_KIND_KUSTOMIZATION`] + [`FLUX_HELMRELEASE_API_VERSION`] /
/// [`FLUX_GITREPOSITORY_API_VERSION`] / [`FLUX_KUSTOMIZATION_API_VERSION`]
/// re-exports enforce on the sibling canonical-Flux-v2-load-bearing-
/// string surfaces.
pub use caixa_core::FLUX_KEY_VALUES;

/// Canonical Flux v2 per-`Kustomization` health-gate reference-list
/// container-axis key every `caixa-flux`-emitted `kustomization.yaml`
/// document mounts its per-sibling-`HelmRelease` health-probe list under
/// (`spec.healthChecks` on `Kustomization`). Re-export of the canonical
/// [`caixa_core::FLUX_KEY_HEALTH_CHECKS`] so the load-bearing Flux-v2-
/// kustomize-controller-side per-`Kustomization` health-gate reference-
/// list container-axis key lives in exactly one place across every caixa
/// renderer — the sweep converts this crate's one production-code call
/// site (the [`cluster_bundle`] `kustomization.yaml` format-string
/// template's baked `healthChecks:\n` container-axis key nesting the
/// per-entry `[]NamespacedObjectKindReference` list whose peer sibling
/// lifted [`FLUX_HELMRELEASE_API_VERSION`] per-entry `apiVersion` axis +
/// [`FLUX_KIND_HELM_RELEASE`] per-entry `kind` axis the health-gate
/// references) plus the three test-fixture `.get("healthChecks")`
/// navigation sites in `mod tests` that probe the emitted per-entry
/// `apiVersion` + `kind` pin onto the re-export. A future Flux v3 rebrand
/// of the per-`Kustomization` health-gate reference-list container-axis
/// key (a hypothetical upstream fluxcd/flux2 rename from `healthChecks`
/// to `HealthChecks` / `healthchecks` / `healthcheck` / `health_checks`
/// / `probes`, coordinated with the upstream project's per-version
/// deprecation cycle) now lands at one const rather than scattered across
/// the one emit-side format-string template + three test-fixture probe
/// sites. Same "the typed constant lives in one place" discipline the
/// peer [`FLUX_KEY_SOURCE_REF`] + [`FLUX_KEY_CHART`] + [`FLUX_KEY_VALUES`]
/// re-exports enforce on the sibling Flux v2 per-`HelmRelease` +
/// per-`Kustomization` body-key surfaces — completes the quartet of Flux
/// v2 `spec.*` body-key constants (`spec.chart` + `spec.chart.spec.sourceRef`
/// + `spec.values` + `spec.healthChecks`) the `cluster_bundle` renderer
/// threads through its two format-string templates.
pub use caixa_core::FLUX_KEY_HEALTH_CHECKS;

/// Canonical Flux v2 per-CR reconcile-poll cadence scalar-axis key every
/// `caixa-flux`-emitted Flux document (`GitRepository`, `HelmRelease`,
/// `Kustomization`) declares its per-CR `spec.interval` reconcile cadence
/// under. Re-export of the canonical [`caixa_core::FLUX_KEY_INTERVAL`] so
/// the load-bearing Flux-v2-controller-triplet-side per-CR reconcile-poll
/// cadence scalar-axis key lives in exactly one place across every caixa
/// renderer — the sweep converts this crate's three production-code call
/// sites (the [`cluster_bundle`] `gitrepository.yaml` + `helmrelease.yaml`
/// + `kustomization.yaml` format-string templates' baked `interval:`
/// scalar-axis keys, one per Flux v2 CRD kind, nested alongside the peer
/// sibling lifted per-CR `apiVersion` + `kind` axis re-exports on this
/// crate — [`FLUX_GITREPOSITORY_API_VERSION`] + [`FLUX_KIND_GIT_REPOSITORY`]
/// on the source-controller CRD, [`FLUX_HELMRELEASE_API_VERSION`] +
/// [`FLUX_KIND_HELM_RELEASE`] on the helm-controller CRD, and
/// [`FLUX_KUSTOMIZATION_API_VERSION`] + [`FLUX_KIND_KUSTOMIZATION`] on the
/// kustomize-controller CRD) onto the re-export. A future Flux v3 rebrand
/// of the per-CR reconcile-poll cadence scalar-axis key (a hypothetical
/// upstream fluxcd/flux2 rename from `interval` to `Interval` / `period`
/// / `cadence` / `pollInterval` / `reconcileInterval`, coordinated with
/// the upstream project's per-version deprecation cycle) now lands at one
/// const rather than scattered across three per-CR emit-side format-string
/// template sites. Same "the typed constant lives in one place" discipline
/// the peer [`FLUX_KEY_SOURCE_REF`] + [`FLUX_KEY_CHART`] + [`FLUX_KEY_VALUES`]
/// + [`FLUX_KEY_HEALTH_CHECKS`] re-exports enforce on the sibling Flux v2
/// per-CR body-key surfaces — extends the per-CR body-key lift trajectory
/// onto the sibling *cross-CR-shared* reconcile-poll cadence scalar-axis
/// every Flux v2 controller reads to bind its per-CR poll cycle.
pub use caixa_core::FLUX_KEY_INTERVAL;

/// Canonical Flux v2 per-`GitRepository` `spec.ref.tag` git-tag-
/// selector scalar-axis key every [`cluster_bundle`]-rendered
/// `gitrepository.yaml` document declares on the tag-arm of the
/// [`GitRefSpec`] discriminated-union. Re-export of the canonical
/// [`caixa_core::FLUX_GITREPOSITORY_REF_KEY_TAG`] so the sub-selector
/// byte-string lives in exactly one place: the two consumer sites
/// (the YAML emit-side `gitref_field` composer and the human-readable
/// `tag_human` narrator prose) both now read through the lifted
/// [`GitRefSpec::ref_field_name`] dispatch that maps the tag-arm of
/// the discriminated-union onto this canonical scalar. Pairs with
/// [`FLUX_GITREPOSITORY_REF_KEY_BRANCH`] +
/// [`FLUX_GITREPOSITORY_REF_KEY_COMMIT`] on the sibling per-shape arms
/// of the same FluxCD source-controller `GitRepository.spec.ref`
/// discriminated-union axis — three consts, one per arm, one canonical
/// dispatch. See the caixa-core docstring for the full lift rationale.
pub use caixa_core::FLUX_GITREPOSITORY_REF_KEY_TAG;

/// Canonical Flux v2 per-`GitRepository` `spec.ref.branch` git-branch-
/// selector scalar-axis key — peer of [`FLUX_GITREPOSITORY_REF_KEY_TAG`]
/// on the branch-arm of the [`GitRefSpec`] discriminated-union.
/// Re-export of [`caixa_core::FLUX_GITREPOSITORY_REF_KEY_BRANCH`]; see
/// [`FLUX_GITREPOSITORY_REF_KEY_TAG`] for the full lift rationale.
pub use caixa_core::FLUX_GITREPOSITORY_REF_KEY_BRANCH;

/// Canonical Flux v2 per-`GitRepository` `spec.ref.commit` git-commit-
/// selector scalar-axis key — peer of [`FLUX_GITREPOSITORY_REF_KEY_TAG`]
/// on the commit-arm of the [`GitRefSpec`] discriminated-union.
/// Re-export of [`caixa_core::FLUX_GITREPOSITORY_REF_KEY_COMMIT`]; see
/// [`FLUX_GITREPOSITORY_REF_KEY_TAG`] for the full lift rationale.
pub use caixa_core::FLUX_GITREPOSITORY_REF_KEY_COMMIT;

/// Canonical Flux v2 per-`GitRepository` `spec.ref` ref-selection
/// discriminated-union parent container-axis key every
/// [`cluster_bundle`]-rendered `gitrepository.yaml` document mounts
/// its per-shape `{tag, branch, commit}` sub-selector arm under.
/// Re-export of the canonical
/// [`caixa_core::FLUX_GITREPOSITORY_KEY_REF`] so the container-axis
/// byte-string lives in exactly one place across every consumer:
/// the writer-side `cluster_bundle` `gitrepo` template composer's
/// `ref:` sub-block header (the sole production emission site the
/// prior inline `"ref:"` literal sat at) + the peer test-fixture
/// `.get("ref")` sub-selector traversal (the sole test-side reader
/// site the prior inline `"ref"` literal sat at) both now navigate
/// through the same `&'static str`. Nests one level above the
/// sibling [`FLUX_GITREPOSITORY_REF_KEY_TAG`] /
/// [`FLUX_GITREPOSITORY_REF_KEY_BRANCH`] /
/// [`FLUX_GITREPOSITORY_REF_KEY_COMMIT`] per-shape arm sub-selector
/// triple it wraps — the parent container-axis KEY now moves
/// through one lifted const alongside its already-lifted per-shape
/// arm sub-selector-KEY triple, so a future Flux v3 sub-schema
/// rebrand (an upstream `fluxcd/flux2` rename of the ref-selection
/// container-axis from `spec.ref` to `spec.gitRef` /
/// `spec.source.ref`) lands as one const edit coordinated with the
/// sibling per-shape arm lifts. See the caixa-core docstring for the
/// full lift rationale.
pub use caixa_core::FLUX_GITREPOSITORY_KEY_REF;

/// Canonical Flux v2 per-`GitRepository` `spec.url` remote-repo-URL
/// leaf-scalar-axis key every [`cluster_bundle`]-rendered
/// `gitrepository.yaml` document declares. Re-export of the canonical
/// [`caixa_core::FLUX_GITREPOSITORY_KEY_URL`] so the byte-string lives
/// in exactly one place: the writer-side `cluster_bundle` `gitrepo`
/// template composer's `url:` sub-key (the sole production emission
/// site the prior inline `"url:"` literal sat at). Sibling to the
/// already-lifted [`FLUX_GITREPOSITORY_KEY_REF`] on the peer per-CR
/// `spec.ref` container-axis surface — these two constants together
/// with the reconcile-cadence [`FLUX_KEY_INTERVAL`] enumerate the
/// canonical `GitRepository.spec.*` per-CR sub-block key surface
/// caixa-flux emits today. See the caixa-core docstring for the full
/// lift rationale.
pub use caixa_core::FLUX_GITREPOSITORY_KEY_URL;

/// Canonical Flux v2 per-cluster-bundle `HelmRelease` document filename
/// every [`cluster_bundle`]-rendered `BundleFile` carries at its per-file
/// `path` axis — re-export of the lifted
/// [`caixa_core::FLUX_HELMRELEASE_YAML_FILENAME`] so the fixed filename
/// the cluster-side FluxCD `kustomize-controller` looks up when it opens
/// the per-Servico bundle directory lives in exactly one place across
/// every caixa renderer. The single source of truth all thirteen
/// consumers reach for — one production [`cluster_bundle`] `BundleFile`
/// assembly's `HelmRelease` document `path` axis plus a dozen test-side
/// round-trip navigators that reach into the rendered bundle by the
/// same filename to pin per-CR body-axis emission — now consult the
/// same `&'static str`. Pin the equality + `&'static` static-data
/// identity so any local re-introduction of a sibling `pub const
/// FLUX_HELMRELEASE_YAML_FILENAME: &str = "…"` at this crate is a
/// build-time test failure naming the offending drift, not a silent
/// FluxCD `kustomize-controller` "no `HelmRelease` document found under
/// this bundle" reroute at cluster-side reconcile time far from the
/// drift site. Peer to the [`HELM_CHART_YAML_FILENAME`]
/// (`caixa_core::HELM_CHART_YAML_FILENAME`, c2c99b0) /
/// [`HELM_VALUES_YAML_FILENAME`] (`caixa_core::HELM_VALUES_YAML_FILENAME`,
/// 9a980ba) re-exports on the sibling Helm-chart-directory filename
/// surfaces — pivots the canonical-filename single-sourcing discipline
/// from the per-Helm-chart-directory metadata / values file axes onto
/// the sibling per-Flux-v2-bundle `HelmRelease` document filename axis
/// this crate's [`cluster_bundle`] renders.
pub use caixa_core::FLUX_HELMRELEASE_YAML_FILENAME;

/// Canonical Flux v2 per-cluster-bundle `GitRepository` document
/// filename every [`cluster_bundle`]-rendered `BundleFile` carries at
/// its per-file `path` axis — re-export of the lifted
/// [`caixa_core::FLUX_GITREPOSITORY_YAML_FILENAME`] so the fixed
/// filename the cluster-side FluxCD `source-controller` looks up when
/// it opens the per-Servico bundle directory lives in exactly one
/// place across every caixa renderer. Pairs with the sibling
/// [`FLUX_HELMRELEASE_YAML_FILENAME`] +
/// [`FLUX_KUSTOMIZATION_YAML_FILENAME`] re-exports to close the
/// per-bundle `(gitrepository, helmrelease, kustomization)` filename
/// axis triple every rendered cluster bundle carries — the single
/// source of truth all nine consumers reach for (one production
/// [`cluster_bundle`] `BundleFile` assembly's `GitRepository`
/// document `path` axis plus eight test-side round-trip navigators
/// that reach into the rendered bundle by the same filename to pin
/// per-CR body-axis emission) now consult the same `&'static str`.
/// Pin the equality + `&'static` static-data identity so any local
/// re-introduction of a sibling `pub const
/// FLUX_GITREPOSITORY_YAML_FILENAME: &str = "…"` at this crate is a
/// build-time test failure naming the offending drift, not a silent
/// FluxCD `source-controller` "no `GitRepository` document found
/// under this bundle" reroute at cluster-side reconcile time far
/// from the drift site. Peer to the sibling
/// [`FLUX_HELMRELEASE_YAML_FILENAME`] (ba7b0b2) re-export — extends
/// the canonical-Flux-v2-bundle-filename lifted-const discipline from
/// the middle coordinate of the filename triple onto its first
/// coordinate.
pub use caixa_core::FLUX_GITREPOSITORY_YAML_FILENAME;

/// Canonical Flux v2 per-cluster-bundle `Kustomization` document
/// filename every [`cluster_bundle`]-rendered `BundleFile` carries at
/// its per-file `path` axis — re-export of the lifted
/// [`caixa_core::FLUX_KUSTOMIZATION_YAML_FILENAME`] so the fixed
/// filename the cluster-side FluxCD `kustomize-controller` looks up
/// when it opens the per-Servico bundle directory lives in exactly
/// one place across every caixa renderer. Pairs with the sibling
/// [`FLUX_GITREPOSITORY_YAML_FILENAME`] +
/// [`FLUX_HELMRELEASE_YAML_FILENAME`] re-exports to close the
/// per-bundle `(gitrepository, helmrelease, kustomization)` filename
/// axis triple every rendered cluster bundle carries — the single
/// source of truth all sixteen consumers reach for (one production
/// [`cluster_bundle`] `BundleFile` assembly's `Kustomization`
/// document `path` axis plus fifteen test-side round-trip navigators
/// that reach into the rendered bundle by the same filename to pin
/// per-CR body-axis emission) now consult the same `&'static str`.
/// Pin the equality + `&'static` static-data identity so any local
/// re-introduction of a sibling `pub const
/// FLUX_KUSTOMIZATION_YAML_FILENAME: &str = "…"` at this crate is a
/// build-time test failure naming the offending drift, not a silent
/// FluxCD `kustomize-controller` "no `Kustomization` document found
/// under this bundle" reroute at cluster-side reconcile time far
/// from the drift site. Peer to the sibling
/// [`FLUX_HELMRELEASE_YAML_FILENAME`] (ba7b0b2) +
/// [`FLUX_GITREPOSITORY_YAML_FILENAME`] (this commit) re-exports —
/// closes the canonical-Flux-v2-bundle-filename lifted-const
/// discipline on the third coordinate of the filename triple.
pub use caixa_core::FLUX_KUSTOMIZATION_YAML_FILENAME;

/// Canonical Helm library-chart name every `lareira-<nome>` chart
/// depends on — re-export of the lifted [`caixa_core::DEFAULT_LIBRARY_NAME`]
/// so the load-bearing string lives in exactly one place across every
/// caixa renderer. The wrap key the `cluster_bundle` `helmrelease.yaml`
/// template uses under `spec.values.<library>:` to thread the per-cluster
/// overrides through to the rendered chart's dep block must match the
/// peer `caixa-helm` chart's `dependencies[0].name` axis exactly
/// (Helm's per-dep alias convention scopes values under the dep's
/// `name:` when no `alias:` is set), and both axes now consult the same
/// `&'static str`. Same shape as the [`caixa_core::DEFAULT_NAMESPACE`]
/// (a085b26) / [`caixa_core::DEFAULT_SERVICO_PORT`] (1e22add) lifts on
/// the peer canonical-K8s-axis-constant surface.
pub use caixa_core::DEFAULT_LIBRARY_NAME;

/// Canonical `pleme-computeunit` library-chart values-block enable-toggle
/// key — re-export of the lifted [`caixa_core::HELM_VALUES_KEY_ENABLED`]
/// so the values-block enable-toggle every rendered `HelmRelease` /
/// upstream `values.yaml` carries under its [`DEFAULT_LIBRARY_NAME`] wrap
/// key lives in exactly one place across every caixa renderer. The single
/// production-code call site consuming it is [`cluster_bundle`]'s
/// `helmrelease.yaml` format-string template (formerly an inline
/// `enabled: true\n` literal at `caixa-flux/src/lib.rs:844`); the peer
/// test-side round-trip navigator
/// (`cluster_bundle_helmrelease_wraps_library_values_under_lifted_default_library_name`)
/// also consults the re-export so a rebrand of the library-chart's per-
/// values enable-toggle axis lands at one const and reaches every
/// consumer by construction. A drifted local
/// `pub const HELM_VALUES_KEY_ENABLED: &str = "…"` (or any sibling per-
/// renderer variant that inlined a stale `"enabled"` / `"enable"` /
/// `"disabled"` literal) would silently emit a `HelmRelease` whose per-
/// cluster override lands under one key while
/// [`caixa_helm::build_values_yaml`]'s default-off toggle inside the
/// rendered `values.yaml` lands under another — Helm's per-values merge
/// treats them as sibling scalars, the enable-toggle the library chart's
/// own template consults never sees the flip, and the workload silently
/// comes up with the library chart's admission-time defaults instead of
/// the per-cluster override the operator set. Same shape as the
/// [`DEFAULT_LIBRARY_NAME`] / [`KUBE_KEY_SPEC`] / [`FLUX_HELMRELEASE_API_VERSION`]
/// re-exports on the sibling canonical-Helm-load-bearing-string /
/// canonical-K8s-CR-body-key / canonical-Flux-CRD-apiVersion axes.
pub use caixa_core::HELM_VALUES_KEY_ENABLED;

/// Canonical K8s CR top-level `spec` key. Re-export of the canonical
/// [`caixa_core::KUBE_KEY_SPEC`] so the per-kind body key lives in
/// exactly one place across every caixa renderer — caixa-flux's
/// `programs_yaml_entry` (the upstream ComputeUnit YAML's
/// `spec.*` axis the rendered programs.yaml entry splices from),
/// `upsert_into_helmrelease_programs` (the canonical
/// `spec.values.programs[]` path the `lareira-fleet-programs`
/// HelmRelease keys the per-Servico entry list under), and each of the
/// three [`cluster_bundle`] format-string templates' four `spec:`
/// YAML label positions (`gitrepository.yaml` top-level +
/// `helmrelease.yaml` top-level + `helmrelease.yaml` `spec.chart.spec`
/// nested + `kustomization.yaml` top-level) all consult the same
/// `&'static str` as the peer caixa-mesh / caixa-helm renderers'
/// `KUBE_KEY_SPEC` re-exports. The prior inline `"spec"` literals at
/// the production-code call sites would have let a typo on one site
/// (e.g. `"Spec"`, `"specs"`, `"spec_"`) silently emit a
/// programs.yaml entry that no `lareira-fleet-programs` schema
/// validator recognizes; the `Error::MissingField("spec")` paths now
/// thread the same `&'static str` through the diagnostic surface so
/// the error message stays byte-identical to the key it failed to
/// find. Same shape as the [`FLUX_HELMRELEASE_API_VERSION`] /
/// [`DEFAULT_LIBRARY_NAME`] re-exports on the sibling
/// canonical-Flux-load-bearing-string axes. Peer with the sibling
/// [`KUBE_KEY_METADATA`] / [`KUBE_KEY_KIND`] /
/// [`KUBE_KEY_API_VERSION`] re-exports on the other three canonical
/// K8s-CR top-level block-scope label axes the `cluster_bundle`
/// format-string templates thread through named-arg interpolation.
pub use caixa_core::KUBE_KEY_SPEC;

/// Canonical K8s CR top-level `metadata` key. Re-export of the canonical
/// [`caixa_core::KUBE_KEY_METADATA`] so the per-kind metadata block key
/// lives in exactly one place across every caixa renderer — caixa-flux's
/// `programs_yaml_entry` (the upstream ComputeUnit YAML's `metadata.namespace`
/// axis the rendered programs.yaml entry's `namespace` field reads from)
/// now consults the same `&'static str` as the peer caixa-mesh renderer's
/// `KUBE_KEY_METADATA` re-export. The prior inline `"metadata"` literals
/// at the production-code + drift-detection call sites would have let a
/// typo on one site (e.g. `"Metadata"`, `"meta-data"`, `"medadata"`)
/// silently miss the ComputeUnit's `metadata.namespace` lookup and fall
/// back to `DEFAULT_NAMESPACE` even when the ComputeUnit YAML pinned a
/// distinct target namespace; the lift routes every K8s-CR-top-level-
/// metadata-axis retrieval through the same `&'static str` so drift
/// between any two sites becomes a single-edit fix at the caixa-core
/// const definition. Same shape as the [`KUBE_KEY_SPEC`] re-export on
/// the sibling K8s-CR top-level-spec-axis.
pub use caixa_core::KUBE_KEY_METADATA;

/// Canonical K8s CR top-level `kind` key. Re-export of the canonical
/// [`caixa_core::KUBE_KEY_KIND`] so the per-CR-kind-axis discriminator
/// key lives in exactly one place across every caixa renderer —
/// caixa-flux's `cluster_bundle` drift-detection pins that traverse the
/// rendered `gitrepository.yaml` / `helmrelease.yaml` / `kustomization.yaml`
/// documents to assert the top-level `kind` + nested `sourceRef.kind`
/// / `healthChecks[].kind` axes bind to the lifted
/// [`FLUX_KIND_GIT_REPOSITORY`] / [`FLUX_KIND_HELM_RELEASE`] /
/// [`FLUX_KIND_KUSTOMIZATION`] discriminators now consult the same
/// `&'static str` as the peer caixa-mesh renderer's `KUBE_KEY_KIND`
/// re-export (615a13d). The prior inline `"kind"` literals at every
/// drift-detection cross-axis-pin call site in this crate would have let
/// a typo on any one site (e.g. `"Kind"`, `"kinds"`, `"knid"`) silently
/// miss the per-CR kind-axis retrieval — the equality assertion would
/// then compare `None` against `Some("GitRepository")` /
/// `Some("HelmRelease")` / `Some("Kustomization")` rather than the
/// expected discriminator, masking the sibling `FLUX_KIND_*` re-export
/// drift the pin was meant to catch under a `.expect("… present")` panic
/// on the missing `.and_then` chain. The lift routes every K8s-CR-
/// top-level-kind-axis retrieval through the same `&'static str` so
/// drift between any two sites becomes a single-edit fix at the
/// caixa-core const definition. Same shape as the [`KUBE_KEY_SPEC`] +
/// [`KUBE_KEY_METADATA`] re-exports on the sibling K8s-CR top-level-spec
/// / top-level-metadata axes — completes the per-K8s-CR top-level
/// `(spec, metadata, kind)` axis re-export triple every rendered Flux
/// bundle document navigates.
pub use caixa_core::KUBE_KEY_KIND;

/// Canonical K8s CR top-level `apiVersion` key. Re-export of the
/// canonical [`caixa_core::KUBE_KEY_API_VERSION`] so the per-CR-
/// group/version-axis discriminator key lives in exactly one place
/// across every caixa renderer — caixa-flux's [`cluster_bundle`]
/// drift-detection pins that traverse the rendered
/// `gitrepository.yaml` / `helmrelease.yaml` / `kustomization.yaml`
/// documents to assert the top-level `apiVersion` axis + the
/// `kustomization.yaml`'s nested `spec.healthChecks[].apiVersion`
/// axis bind to the lifted [`FLUX_GITREPOSITORY_API_VERSION`] /
/// [`FLUX_HELMRELEASE_API_VERSION`] / [`FLUX_KUSTOMIZATION_API_VERSION`]
/// controller-triplet CRD-group/versions now consult the same
/// `&'static str` as the sibling [`KUBE_KEY_SPEC`] / [`KUBE_KEY_METADATA`]
/// / [`KUBE_KEY_KIND`] re-exports on the peer K8s-CR top-level axes.
/// The prior inline `"apiVersion"` literals at every drift-detection
/// cross-axis-pin call site in this crate would have let a typo on
/// any one site (e.g. `"ApiVersion"`, `"api_version"`, `"apiversion"`,
/// `"apiVer"`) silently miss the per-CR apiVersion-axis retrieval —
/// the equality assertion would then compare `None` against
/// `Some("source.toolkit.fluxcd.io/v1")` /
/// `Some("helm.toolkit.fluxcd.io/v2")` /
/// `Some("kustomize.toolkit.fluxcd.io/v1")` rather than the expected
/// controller-triplet CRD-group/version, masking the sibling
/// `FLUX_*_API_VERSION` re-export drift the pin was meant to catch
/// under a `.expect("… present")` panic on the missing `.and_then`
/// chain. The lift routes every K8s-CR top-level-apiVersion-axis
/// retrieval through the same `&'static str` so drift between any
/// two sites becomes a single-edit fix at the caixa-core const
/// definition, extending the discipline the sibling [`KUBE_KEY_SPEC`]
/// / [`KUBE_KEY_METADATA`] / [`KUBE_KEY_KIND`] re-exports establish
/// onto the last of the four K8s-CR top-level axes (`apiVersion`,
/// `kind`, `metadata`, `spec`) every rendered Flux v2 bundle
/// document declares — completes the per-K8s-CR top-level
/// `(apiVersion, kind, metadata, spec)` axis re-export quartet
/// across this crate.
pub use caixa_core::KUBE_KEY_API_VERSION;

/// Canonical K8s CR `metadata.namespace` key. Re-export of the canonical
/// [`caixa_core::KUBE_KEY_NAMESPACE`] so the per-CR namespace-axis key
/// lives in exactly one place across every caixa renderer — caixa-flux's
/// [`programs_yaml_entry`] threads the ComputeUnit YAML's
/// `metadata.namespace` retrieval and the emitted `programs:[]` entry's
/// isomorphic `namespace:` field (the `lareira-fleet-programs` chart's
/// per-Servico namespace axis, populated verbatim from the ComputeUnit's
/// `metadata.namespace` per the docstring on `programs_yaml_entry` above)
/// through this key, [`cluster_bundle`]'s rendered `kustomization.yaml`
/// drift-detection pin traverses the emitted document's
/// `metadata.namespace` axis to assert it binds to the lifted
/// [`DEFAULT_FLUX_SYSTEM_NAMESPACE`] (the bootstrap kustomize-
/// controller's watch window) through this same key, and each of the
/// three [`cluster_bundle`] format-string templates' five `namespace:`
/// YAML label positions — `gitrepository.yaml` top-level
/// `metadata.namespace`, `helmrelease.yaml` top-level
/// `metadata.namespace` + nested `spec.chart.spec.sourceRef.namespace`,
/// `kustomization.yaml` top-level `metadata.namespace` + nested
/// `spec.healthChecks[].namespace` — thread this `&'static str`
/// through named-arg interpolation instead of the prior five inline
/// `namespace:` label literals.
///
/// The prior inline `"namespace"` literals at the production-code
/// (`programs_yaml_entry` read + write) and drift-detection call sites
/// would have let a typo on any one site (e.g. `"Namespace"`,
/// `"name space"`, `"namesapce"`, the canonical transposition) silently
/// miss the ComputeUnit's `metadata.namespace` lookup and fall back to
/// [`DEFAULT_NAMESPACE`] even when the ComputeUnit YAML pinned a
/// distinct target namespace, or write a `namesapce:` key that no
/// `lareira-fleet-programs` schema validator recognizes, or mask a
/// drifted [`DEFAULT_FLUX_SYSTEM_NAMESPACE`] regression by returning
/// `None` from the `.get("namesapce")` retrieval so the equality
/// assertion never sees the true axis value under the `.expect(…)`
/// panic on the missing `.and_then` chain. The lift routes every
/// K8s-CR-`metadata.namespace`-axis retrieval / emission through the
/// same `&'static str` so drift between any two sites becomes a
/// single-edit fix at the caixa-core const definition. Same shape as
/// the [`KUBE_KEY_METADATA`] / [`KUBE_KEY_SPEC`] / [`KUBE_KEY_KIND`] /
/// [`KUBE_KEY_API_VERSION`] re-exports on the sibling K8s-CR top-level
/// axes — extends the discipline the top-level `(apiVersion, kind,
/// metadata, spec)` axis re-export quartet establishes onto the
/// canonical `metadata.namespace` nested axis every rendered Flux v2
/// bundle document navigates.
pub use caixa_core::KUBE_KEY_NAMESPACE;

/// Canonical K8s CR `metadata.name` key. Re-export of the canonical
/// [`caixa_core::KUBE_KEY_NAME`] so the per-CR name-axis key lives in
/// exactly one place across every caixa renderer. Each of the three
/// [`cluster_bundle`] format-string templates' six `name:` YAML label
/// positions — `gitrepository.yaml` top-level `metadata.name`,
/// `helmrelease.yaml` top-level `metadata.name` + nested
/// `spec.chart.spec.sourceRef.name`, `kustomization.yaml` top-level
/// `metadata.name` + nested `spec.sourceRef.name` + nested
/// `spec.healthChecks[].name` — thread this `&'static str` through
/// named-arg interpolation instead of the prior six inline `name:`
/// label literals.
///
/// The prior inline `"name"` literals at the production-side call
/// sites would have let a typo on any one site (e.g. `"Name"`,
/// `"nane"`, `"nam"`, the canonical transposition) silently emit a
/// document whose per-CR `metadata.name` axis the apiserver-side
/// ObjectMeta parser cannot key on — the `helm-controller` /
/// `source-controller` / `kustomize-controller` would then treat the
/// rendered document as an anonymous CR (or the apiserver would
/// reject the apply with a schema-validation error naming the wrong
/// key), with no field naming the label-drift root cause far from
/// the source caixa.lisp. Now the emit-side and retrieval-side both
/// consult one substrate-owned `&'static str`, and the raw-byte-
/// label pin structurally forbids the emit format-string from
/// drifting away from the canonical key without failing at test
/// time. Same shape as the [`KUBE_KEY_NAMESPACE`] /
/// [`KUBE_KEY_METADATA`] / [`KUBE_KEY_SPEC`] / [`KUBE_KEY_KIND`] /
/// [`KUBE_KEY_API_VERSION`] re-exports on the sibling K8s-CR
/// canonical-key axes — extends the discipline the top-level
/// `(apiVersion, kind, metadata, spec)` axis re-export quartet +
/// the sibling `metadata.namespace` nested-axis re-export establish
/// onto the paired `metadata.name` nested axis every rendered Flux
/// v2 bundle document navigates as the other half of the `(name,
/// namespace)` ObjectMeta / sourceRef / healthCheck identity-pair.
pub use caixa_core::KUBE_KEY_NAME;

/// Local re-export of [`caixa_core::FLEET_PROGRAMS_KEY_PROGRAMS`] —
/// the canonical `lareira-fleet-programs` values-schema array key
/// (`programs:` — the exact YAML key the fleet-programs library chart
/// reads under `.Values.programs[]` to iterate one `ComputeUnit` CR per
/// entry). This crate's two writer-side upsert paths
/// ([`upsert_into_helmrelease_programs`] on the aggregator-HelmRelease
/// shape, [`upsert_into_programs_yaml`] on the bare-values.yaml shape)
/// both anchor on this key when walking the entry sequence to
/// match-by-name-and-replace-or-append. Re-exported here so the two
/// production sites' `values_map.entry(Value::String(<key>.into()))` /
/// `programs_yaml.get(<key>)` navigation reads from the same
/// `&'static str` as every peer consumer (and every future fleet-
/// programs schema-key consumer — the M4 `app-operator` per-Aplicacao
/// reconciler, the future `feira app deploy --apply` writer-side
/// aggregator merge). Same re-export shape as the peer
/// [`KUBE_KEY_NAMESPACE`] / [`KUBE_KEY_METADATA`] / [`KUBE_KEY_SPEC`]
/// / [`KUBE_KEY_KIND`] / [`KUBE_KEY_API_VERSION`] surfaces on the
/// sibling K8s-CR canonical-key axes — extends the discipline the
/// K8s-CR key re-export quintet establishes onto the canonical
/// fleet-programs schema top-level axis.
pub use caixa_core::FLEET_PROGRAMS_KEY_PROGRAMS;

/// Local re-export of [`caixa_core::FLEET_PROGRAMS_KEY_NAME`] — the
/// canonical `lareira-fleet-programs` values-schema per-entry name
/// discriminator key (`name:` — the exact YAML key the fleet-programs
/// library chart's `range .Values.programs` step reads per-entry to
/// key each rendered `ComputeUnit` CR's `metadata.name` off). Peer of
/// the sibling [`FLEET_PROGRAMS_KEY_PROGRAMS`] top-level array-key
/// re-export on the same fleet-programs schema — that one carries the
/// `programs:` array key both writer verbs upsert into, this one
/// carries the per-entry name-axis both writer verbs walk that array
/// by (and both emit-side entry builders — this crate's
/// [`programs_yaml_entry`] and the peer [`caixa_mesh::programs_for_aplicacao`]
/// — write the per-entry name-axis at).
///
/// This crate's three writer-side sites anchor on this key:
/// [`programs_yaml_entry`]'s emit-side `entry.insert(<key>.into(), …)`
/// call (seeding the per-entry name-axis from the Caixa's `nome`),
/// [`upsert_into_helmrelease_programs`]'s two `new_entry.get(<key>)`
/// / `slot.get(<key>)` navigations + `Error::MissingField(<key>)`
/// diagnostic on the aggregator-HelmRelease shape, and
/// [`upsert_into_programs_yaml`]'s peer three-site (extract + match +
/// `MissingField`) shape on the bare-values.yaml shape. All three
/// verbs now read from the same `&'static str` as every peer
/// consumer (and every future fleet-programs schema-key consumer —
/// the M4 `app-operator` per-Aplicacao reconciler, the future
/// `feira app deploy --apply` writer-side aggregator merge, the peer
/// [`caixa_mesh::programs_for_aplicacao`] per-`:membros` emit-side
/// name-axis). Same re-export shape as the peer
/// [`FLEET_PROGRAMS_KEY_PROGRAMS`] / [`KUBE_KEY_NAMESPACE`] /
/// [`KUBE_KEY_METADATA`] / [`KUBE_KEY_SPEC`] / [`KUBE_KEY_KIND`] /
/// [`KUBE_KEY_API_VERSION`] surfaces on the sibling fleet-programs /
/// K8s-CR canonical-key axes — extends the discipline the K8s-CR key
/// re-export quintet + the sibling fleet-programs top-level array-
/// key re-export establish onto the canonical fleet-programs schema
/// per-entry name-discriminator axis.
pub use caixa_core::FLEET_PROGRAMS_KEY_NAME;

/// Local re-export of the canonical
/// [`caixa_core::COMPUTEUNIT_SPEC_KEY_MODULE`] — the
/// `wasm.pleme.io/v1alpha1/ComputeUnit` CRD per-CR wasm-module-reference
/// `spec.module` sub-block key every rendered `programs[]` entry
/// splices verbatim from the upstream ComputeUnit YAML's `spec.module`
/// (per the docstring on [`programs_yaml_entry`] above), so the
/// `lareira-fleet-programs` library chart's per-Servico module-source
/// axis binds to the exact source the caixa.lisp's `:servicos`
/// fixture pins. Four per-entry drift-detection navigators in this
/// crate's test module (the `programs_yaml_entry_round_trips` per-key
/// pair + the `upsert_helmrelease_replaces_existing` /
/// `upsert_into_programs_yaml` per-`module.source` navigators)
/// now consult the same `&'static str` as the peer caixa-helm
/// per-values navigators' module-source axis. Same re-export shape
/// as the peer [`FLEET_PROGRAMS_KEY_NAME`] / [`KUBE_KEY_NAMESPACE`]
/// surfaces on the sibling canonical-fleet-programs-schema-key /
/// canonical-K8s-CR-key axes — extends the discipline the M2-typed-
/// slot / fleet-programs schema / K8s-CR key re-export families
/// establish onto the substrate-side ComputeUnit-CRD per-`spec.*`
/// sub-block axis. See [`caixa_core::COMPUTEUNIT_SPEC_KEY_MODULE`]
/// for the full lift rationale.
pub use caixa_core::COMPUTEUNIT_SPEC_KEY_MODULE;

/// Local re-export of the canonical
/// [`caixa_core::COMPUTEUNIT_SPEC_KEY_TRIGGER`] — the
/// `wasm.pleme.io/v1alpha1/ComputeUnit` CRD per-CR invocation-shape
/// `spec.trigger` sub-block key every rendered `programs[]` entry
/// splices verbatim from the upstream ComputeUnit YAML's
/// `spec.trigger`. Peer of [`COMPUTEUNIT_SPEC_KEY_MODULE`] on the same
/// ComputeUnit CRD per-`spec.*` sub-block axis. See
/// [`caixa_core::COMPUTEUNIT_SPEC_KEY_TRIGGER`] for the full lift
/// rationale.
pub use caixa_core::COMPUTEUNIT_SPEC_KEY_TRIGGER;

/// Local re-export of the canonical
/// [`caixa_core::COMPUTEUNIT_SPEC_KEY_CAPABILITIES`] — the
/// `wasm.pleme.io/v1alpha1/ComputeUnit` CRD per-CR WASI-capability-list
/// `spec.capabilities` sub-block key every rendered `programs[]` entry
/// splices verbatim from the upstream ComputeUnit YAML's
/// `spec.capabilities`. Peer of [`COMPUTEUNIT_SPEC_KEY_MODULE`] and
/// [`COMPUTEUNIT_SPEC_KEY_TRIGGER`] on the same ComputeUnit CRD
/// per-`spec.*` sub-block axis — completes the substrate-side
/// ComputeUnit-CRD per-`spec.*` sub-block re-export triple in this
/// crate. See [`caixa_core::COMPUTEUNIT_SPEC_KEY_CAPABILITIES`] for
/// the full lift rationale.
pub use caixa_core::COMPUTEUNIT_SPEC_KEY_CAPABILITIES;

/// Local re-export of the canonical
/// [`caixa_core::COMPUTEUNIT_MODULE_KEY_SOURCE`] — the
/// `wasm.pleme.io/v1alpha1/ComputeUnit` CRD nested
/// `spec.module.source` per-CR wasm-component-reference leaf-scalar
/// sub-block key every rendered `programs[]` entry carries under the
/// parent [`COMPUTEUNIT_SPEC_KEY_MODULE`] block to name the exact
/// OCI / git / file wasm-component artifact the M2.5 wasm-engine
/// instantiator loads at Servico bring-up. Three per-`module.source`
/// drift-detection navigators in this crate's test module
/// (`programs_yaml_entry_round_trips`'s per-entry
/// `.get(COMPUTEUNIT_SPEC_KEY_MODULE).and_then(|m| m.get(…))`
/// present-check + `upsert_into_programs_yaml`'s cross-upsert
/// readback + `upsert_into_helmrelease_programs`'s peer
/// `HelmRelease`-wrapped `spec.values.programs[]` cross-upsert
/// readback) now consult the same `&'static str` — extends the
/// discipline the peer [`COMPUTEUNIT_SPEC_KEY_MODULE`] /
/// [`COMPUTEUNIT_SPEC_KEY_TRIGGER`] / [`COMPUTEUNIT_SPEC_KEY_CAPABILITIES`]
/// top-level-`spec.*` re-exports establish one level deeper onto the
/// nested `spec.module.*` leaf-scalar-axis. See
/// [`caixa_core::COMPUTEUNIT_MODULE_KEY_SOURCE`] for the full lift
/// rationale.
pub use caixa_core::COMPUTEUNIT_MODULE_KEY_SOURCE;

/// Local re-export of the canonical
/// [`caixa_core::servico_spec_and_m2_overlay_entries`] — the composed
/// per-Servico value-block splice helper this crate's
/// [`programs_yaml_entry`] and the peer
/// [`caixa_helm::build_values_yaml`] both now route their two-step
/// `spec.*` field-splice + M2 typed-slot overlay through. The single
/// production-code call site consuming it is
/// [`programs_yaml_entry`]'s inner splice loop (formerly two hand-
/// written for-loops chained around `string_keyed_entries` +
/// `servico_m2_overlay`); re-exported so the shared composition
/// contract lives in exactly one place across both per-Servico
/// renderers — a future author reading `caixa_flux::programs_yaml_entry`
/// finds the composition helper immediately without an extra `use
/// caixa_core::…` line, and a rebrand of the composition axis (e.g. a
/// swap of the `or_insert` precedence rule) reaches both renderers
/// through one canonical `&'static` function pointer. Same shape as
/// the peer render-side helper re-exports on the sibling
/// canonical-composed-primitive axes.
pub use caixa_core::servico_spec_and_m2_overlay_entries;

/// Render a single `programs:[]` array entry for the cluster's
/// `lareira-fleet-programs` HelmRelease values.
///
/// The output is `serde_yaml::Value::Mapping`, so callers can splice
/// it into an existing `programs:` array without re-parsing the
/// containing structure. Schema is enforced by
/// `lareira-fleet-programs/values.schema.json` (`#/definitions/program`).
///
/// Pulls:
/// - `name` from `caixa.nome`
/// - `namespace` from `computeunit.metadata.namespace` (or `DEFAULT_NAMESPACE`)
/// - [`COMPUTEUNIT_SPEC_KEY_MODULE`] / [`COMPUTEUNIT_SPEC_KEY_TRIGGER`] /
///   [`COMPUTEUNIT_SPEC_KEY_CAPABILITIES`] / `config` / `resources`
///   from `computeunit.spec.*` (verbatim — schemas already match)
pub fn programs_yaml_entry(
    caixa: &Caixa,
    computeunit_yaml: &serde_yaml::Value,
) -> Result<serde_yaml::Value, Error> {
    caixa_core::require_v0_servico_shape::<Error>(caixa)?;

    let spec = computeunit_yaml
        .get(KUBE_KEY_SPEC)
        .ok_or(Error::MissingField(KUBE_KEY_SPEC))?;

    let namespace = kube_metadata_str_field(computeunit_yaml, KUBE_KEY_NAMESPACE)
        .unwrap_or(DEFAULT_NAMESPACE)
        .to_string();

    let mut entry = serde_yaml::Mapping::new();
    entry.insert_string(FLEET_PROGRAMS_KEY_NAME, caixa.nome.clone());
    entry.insert_string(KUBE_KEY_NAMESPACE, namespace);

    // Two-step per-Servico value-block splice — the `spec.*` field
    // splice (module / trigger / capabilities / config / resources /
    // serviceAccount) and the M2 typed-slot overlay (limits / behavior
    // / upgradeFrom, `or_insert` semantics so `spec.*` wins on
    // collision) now route through the canonical
    // [`caixa_core::servico_spec_and_m2_overlay_entries`] composition
    // helper — the two prior inline for-loops chained around
    // `string_keyed_entries` + `servico_m2_overlay` this call site
    // (and the peer [`caixa_helm::build_values_yaml`] site) each
    // re-derived collapse onto one canonical composition, so a future
    // change to the per-Servico splice / overlay shape (the M4 typed
    // per-edge policy overlay slot addition MESH-COMPOSITION §III.2
    // #3 acknowledges, a change to the precedence rule once per-
    // Aplicacao operator overrides land, a canonicalization pass on
    // the merged key set) reaches both renderers by construction
    // instead of a coordinated two-file rewrite. See the helper's
    // docstring for the full lift rationale and the byte-shape
    // guarantee (spec.* keys preserved in source-Mapping insertion
    // order; M2 slots appended in canonical BTreeMap-key ordering at
    // every M2 key not already claimed by spec.*).
    for (k, v) in caixa_core::servico_spec_and_m2_overlay_entries(caixa, spec)? {
        entry.entry_str_key(&k).or_insert(v);
    }

    Ok(serde_yaml::Value::Mapping(entry))
}

/// Insert/upsert an entry into a `programs:` array nested under
/// the canonical fleet-manifest path: `spec.values.programs[]` in a
/// `HelmRelease` document. The pleme-io convention puts the fleet's
/// program list inside a HelmRelease (consumed by `lareira-fleet-programs`),
/// not at the top level. Same upsert semantics as
/// [`upsert_into_programs_yaml`] — match by `name`, replace in place,
/// otherwise append.
pub fn upsert_into_helmrelease_programs(
    helmrelease: serde_yaml::Value,
    new_entry: serde_yaml::Value,
) -> Result<(serde_yaml::Value, bool), Error> {
    let serde_yaml::Value::Mapping(mut root) = helmrelease else {
        return Err(Error::MissingField(
            "expected mapping at root of HelmRelease",
        ));
    };

    let spec = root
        .get_mut(KUBE_KEY_SPEC)
        .ok_or(Error::MissingField(KUBE_KEY_SPEC))?;
    let serde_yaml::Value::Mapping(spec_map) = spec else {
        return Err(Error::MissingField("spec must be a mapping"));
    };
    let values_map = spec_map
        .entry_or_default_mapping(FLUX_KEY_VALUES)
        .ok_or(Error::MissingField("spec.values must be a mapping"))?;
    let arr = values_map
        .entry_or_default_sequence(FLEET_PROGRAMS_KEY_PROGRAMS)
        .ok_or(Error::MissingField(
            "spec.values.programs must be a sequence",
        ))?;

    let inserted = caixa_core::upsert_named_entry(arr, new_entry, FLEET_PROGRAMS_KEY_NAME, || {
        Error::MissingField(FLEET_PROGRAMS_KEY_NAME)
    })?;

    Ok((serde_yaml::Value::Mapping(root), inserted))
}

/// Insert/upsert an entry into a `programs:` array of an existing
/// values.yaml structure.
///
/// Idempotent: if an entry with the same `name` exists, replaces it
/// in-place (preserving order). If not, appends. Returns the modified
/// document. Operates on `Value` so callers can round-trip via
/// `serde_yaml::from_str` / `to_string` without losing structure.
///
/// Returns the modified `programs_yaml` plus a `bool` indicating
/// whether the entry was a new insert (`true`) or a replacement (`false`).
pub fn upsert_into_programs_yaml(
    programs_yaml: serde_yaml::Value,
    new_entry: serde_yaml::Value,
) -> Result<(serde_yaml::Value, bool), Error> {
    let serde_yaml::Value::Mapping(mut root) = programs_yaml else {
        return Err(Error::MissingField(
            "expected mapping at root of values.yaml",
        ));
    };

    let arr = root
        .entry_or_default_sequence(FLEET_PROGRAMS_KEY_PROGRAMS)
        .ok_or(Error::MissingField("programs must be a sequence"))?;

    let inserted = caixa_core::upsert_named_entry(arr, new_entry, FLEET_PROGRAMS_KEY_NAME, || {
        Error::MissingField(FLEET_PROGRAMS_KEY_NAME)
    })?;

    Ok((serde_yaml::Value::Mapping(root), inserted))
}

// ── Cluster bundle (one-off / standalone path) ──────────────────────────

/// Inputs for [`cluster_bundle`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterBundleOpts {
    /// Cluster name — drives output paths (e.g. `rio`, `mar`).
    pub cluster: String,
    /// Namespace for the rendered HelmRelease.
    pub namespace: String,
    /// Reconcile interval string (Helm/Flux duration like `"10m"`).
    pub interval: String,
    /// Path to the chart inside the source repo (default: `chart/`).
    pub chart_path: String,
    /// Source git URL.
    pub git_url: String,
    /// Source git ref (branch or tag).
    pub git_ref: GitRefSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GitRefSpec {
    Tag(String),
    Branch(String),
    Commit(String),
}

impl GitRefSpec {
    /// The FluxCD source-controller `GitRepository.spec.ref.<field>`
    /// sub-selector scalar-axis key this variant renders under — the
    /// per-arm dispatch onto the canonical lifted
    /// [`FLUX_GITREPOSITORY_REF_KEY_TAG`] /
    /// [`FLUX_GITREPOSITORY_REF_KEY_BRANCH`] /
    /// [`FLUX_GITREPOSITORY_REF_KEY_COMMIT`] byte-string trio the
    /// caixa-core substrate owns.
    ///
    /// Both consumer sites in [`cluster_bundle`] (the `gitref_field`
    /// YAML emit-side composer and the sibling `tag_human`
    /// human-readable narrator prose) now route through this dispatch,
    /// closing the 2-site duplication of the prior inline
    /// `format!("    {arm}: {v:?}")` + `format!("{arm} {v}")` match
    /// blocks that each open-coded the three-way arm-shape mapping
    /// side-by-side. Same "one canonical dispatch per typed axis"
    /// discipline the peer [`caixa_core::WitTarget::payload_pair`]
    /// (6788ed6) established on the sibling `:contratos` payload-arm
    /// dispatch surface — a future `GitRefSpec` variant addition
    /// (FluxCD's source-controller `spec.ref` schema exposes further
    /// `semver` / `name` sub-selectors this V0 shape stops short of)
    /// becomes exactly one new match-arm here (a compile-time
    /// exhaustiveness error otherwise), not a coordinated three-way
    /// rewrite of both format-string templates + every downstream
    /// consumer that reaches for the per-arm sub-selector key.
    #[must_use]
    pub const fn ref_field_name(&self) -> &'static str {
        match self {
            GitRefSpec::Tag(_) => FLUX_GITREPOSITORY_REF_KEY_TAG,
            GitRefSpec::Branch(_) => FLUX_GITREPOSITORY_REF_KEY_BRANCH,
            GitRefSpec::Commit(_) => FLUX_GITREPOSITORY_REF_KEY_COMMIT,
        }
    }

    /// The underlying scalar the variant carries — the tag / branch /
    /// commit value the FluxCD source-controller feeds into its
    /// per-CR git-source clone refspec. Peer of [`Self::ref_field_name`]
    /// on the same per-variant dispatch: both consumer sites in
    /// [`cluster_bundle`] pair the sub-selector key with the paired
    /// scalar to compose the rendered `spec.ref.<field>: <value>`
    /// YAML sub-block + the sibling `<field> <value>` narrator prose,
    /// so the pair moves together on any future variant addition.
    #[must_use]
    pub fn ref_value(&self) -> &str {
        match self {
            GitRefSpec::Tag(t) | GitRefSpec::Branch(t) | GitRefSpec::Commit(t) => t.as_str(),
        }
    }
}

impl ClusterBundleOpts {
    /// Sensible defaults for a per-program standalone bundle.
    #[must_use]
    pub fn for_caixa(caixa: &Caixa, cluster: impl Into<String>) -> Self {
        Self {
            cluster: cluster.into(),
            namespace: DEFAULT_NAMESPACE.into(),
            interval: DEFAULT_FLUX_RECONCILE_INTERVAL.into(),
            chart_path: DEFAULT_FLUX_CHART_SOURCE_SUBPATH.into(),
            git_url: caixa.repositorio.clone().unwrap_or_else(|| {
                format!(
                    "https://github.com/{org}/{nome}",
                    org = caixa_core::DEFAULT_PLEME_GIT_ORG,
                    nome = caixa.nome,
                )
            }),
            git_ref: GitRefSpec::Tag(format!(
                "{prefix}{versao}",
                prefix = caixa_core::DEFAULT_PUBLISH_TAG_PREFIX,
                versao = caixa.versao,
            )),
        }
    }
}

/// One file of the cluster bundle — `(path, contents)` pair every
/// [`cluster_bundle`]-rendered Flux v2 CR YAML document lands at.
///
/// Type-aliased to the canonical [`caixa_core::RenderedFile`] so the
/// substrate-side "one rendered leaf artifact" shape lives at one
/// struct definition across every per-target renderer — the peer
/// [`caixa_helm::ChartFile`] alias resolves to the same canonical, so
/// a future rebrand on either axis (a per-artifact hash / provenance
/// field addition, a per-artifact write-mode discriminator once
/// per-cluster-writer sandboxing lands) lands at one caixa-core `pub
/// struct RenderedFile` edit and reaches both crates by construction.
/// Prior to this lift both crates carried an inline `pub struct
/// <Xxx>File { pub path: PathBuf, pub contents: String }` with
/// identical `#[derive(Debug, Clone, PartialEq, Eq)]` shapes and no
/// per-type impls; a future per-target renderer (`caixa-otel`, the
/// future `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer, the
/// future per-Supervisor reconciler renderer) would have carried a
/// third and fourth clone of the same record. Type aliases preserve
/// every existing struct-literal construction site
/// (`BundleFile { path, contents }`), every field-access site (`f.path`,
/// `f.contents`), and every derive-fed navigator by construction —
/// Rust type aliases inherit the aliased type's `#[derive]`-generated
/// `Debug`/`Clone`/`PartialEq`/`Eq` impls with no per-alias glue.
pub type BundleFile = caixa_core::RenderedFile;

/// Cluster bundle: the FluxCD trio for a standalone caixa deploy.
///
/// Three YAMLs:
///   gitrepository.yaml — points at the caixa's source repo at a tag
///   helmrelease.yaml   — uses the per-program chart from caixa-helm
///   kustomization.yaml — the Flux Kustomization that staples them
///
/// Written under `<cluster>/services/<caixa-name>/` by `feira deploy`.
///
/// V0 contract: every `:kind Servico` caixa carries exactly one
/// `:servicos` entry (one ComputeUnit YAML pointer per Servico,
/// matching the one `lareira-<nome>` Helm chart `caixa-helm` renders
/// and the one HelmRelease this bundle's `helmrelease.yaml` points at).
/// The pair [`caixa_core::require_kind`] + [`caixa_core::require_single_servico`]
/// is the canonical per-Servico-renderer entry-point gate axis the peer
/// [`programs_yaml_entry`] (the aggregator path) and
/// [`caixa_helm::render_chart_for_servico`] (the per-program chart path)
/// both run; until this gate landed `cluster_bundle` ran only the kind
/// half of the pair, so a Servico-kind caixa with a non-singleton
/// `:servicos` list silently passed the bundle render and the failure
/// surfaced at the chart-render layer (`caixa-helm` refused the input
/// with `UnsupportedServicoCount`) far from the source `caixa.lisp` and
/// far from the deploy-path entry point — the canonical "the V0
/// invariant is enforced at every per-Servico renderer entry except
/// this one" footgun the prior 06b2981 lift commit's body named when
/// it called the pair "the canonical V0-shape gate pair for the Servico
/// kind, in one place". Same shape every peer per-Servico-renderer
/// entry-point uses ([`programs_yaml_entry`] at the aggregator path,
/// [`caixa_helm::render_chart_for_servico`] at the per-program chart
/// path).
pub fn cluster_bundle(caixa: &Caixa, opts: &ClusterBundleOpts) -> Result<Vec<BundleFile>, Error> {
    caixa_core::require_v0_servico_shape::<Error>(caixa)?;

    let name = caixa.nome.clone();
    let chart_name = lareira_chart_name(&name);

    // The per-variant sub-selector key (`tag` / `branch` / `commit`)
    // + its paired scalar (the tag / branch / commit value) now route
    // through the canonical [`GitRefSpec::ref_field_name`] +
    // [`GitRefSpec::ref_value`] dispatch, closing the 2-site
    // duplication the prior inline per-variant `format!("    <arm>:
    // {v:?}")` match block open-coded side-by-side with the sibling
    // `tag_human` narrator prose block. Byte-identical output to the
    // prior 3-arm inline `format!`s: `{value:?}` on `&str` renders
    // the same shape as `{v:?}` on `String` (both call the same
    // `Debug` impl at the identical stack position).
    let gitref_field = format!(
        "    {field}: {value:?}",
        field = opts.git_ref.ref_field_name(),
        value = opts.git_ref.ref_value(),
    );

    let gitrepo = format!(
        "---\n\
         # Source — pinned to {tag_human}, rendered by caixa-flux.\n\
         {api_version_key}: {api_version}\n\
         {kind_key}: {kind}\n\
         {metadata_key}:\n  \
           {name_key}: {name}\n  \
           {namespace_key}: {namespace}\n\
         {spec_key}:\n  \
           {interval_key}: {interval}\n  \
           {url_key}: {url}\n  \
           {ref_key}:\n\
         {gitref_field}\n",
        api_version_key = KUBE_KEY_API_VERSION,
        api_version = FLUX_GITREPOSITORY_API_VERSION,
        kind_key = KUBE_KEY_KIND,
        kind = FLUX_KIND_GIT_REPOSITORY,
        metadata_key = KUBE_KEY_METADATA,
        name_key = KUBE_KEY_NAME,
        namespace_key = KUBE_KEY_NAMESPACE,
        spec_key = KUBE_KEY_SPEC,
        tag_human = format!(
            "{field} {value}",
            field = opts.git_ref.ref_field_name(),
            value = opts.git_ref.ref_value(),
        ),
        name = name,
        namespace = opts.namespace,
        interval_key = FLUX_KEY_INTERVAL,
        interval = opts.interval,
        url_key = FLUX_GITREPOSITORY_KEY_URL,
        url = opts.git_url,
        ref_key = FLUX_GITREPOSITORY_KEY_REF,
        gitref_field = gitref_field,
    );

    // The values wrap key under `spec.values.<library>:` must match the
    // peer caixa-helm chart's `dependencies[0].name` axis exactly —
    // Helm's per-dep alias convention scopes values under the
    // dependency's `name:` when no `alias:` is set, so a wrap-key drift
    // silently routes the per-cluster `enabled: true` override nowhere
    // at `helm template` / `helm install` time. Both axes now consult
    // the same lifted [`caixa_core::DEFAULT_LIBRARY_NAME`] constant
    // (`caixa-helm`'s `RenderOpts::library_name` defaults to the same
    // re-export), so a future per-edition library-chart rebrand reaches
    // both consumers through one `&'static str` by construction. Peer
    // with the [`DEFAULT_NAMESPACE`] (a085b26) /
    // [`DEFAULT_SERVICO_PORT`] (1e22add) lifts on the sibling
    // canonical-K8s-axis-constant surface — duplicated load-bearing
    // string axes lifted to one source of truth.
    let helmrelease = format!(
        "---\n\
         # HelmRelease consumes the chart caixa-helm renders for this\n\
         # caixa Servico. Per-cluster values are injected here.\n\
         {api_version_key}: {api_version}\n\
         {kind_key}: {kind}\n\
         {metadata_key}:\n  \
           {name_key}: {name}\n  \
           {namespace_key}: {namespace}\n\
         {spec_key}:\n  \
           {interval_key}: {interval}\n  \
           {chart_key}:\n    \
             {spec_key}:\n      \
               {chart_name_key}: {chart_path}\n      \
               {source_ref_key}:\n        \
                 {kind_key}: {source_kind}\n        \
                 {name_key}: {name}\n        \
                 {namespace_key}: {namespace}\n  \
           {install_key}:\n    \
             {create_namespace_key}: true\n    \
             {remediation_key}:\n      \
               {retries_key}: {retries_default}\n  \
           {upgrade_key}:\n    \
             {remediation_key}:\n      \
               {retries_key}: {retries_default}\n      \
               {remediate_last_failure_key}: true\n  \
           {values_key}:\n    \
             {library_name}:\n      \
               {enabled_key}: true\n",
        api_version_key = KUBE_KEY_API_VERSION,
        api_version = FLUX_HELMRELEASE_API_VERSION,
        kind_key = KUBE_KEY_KIND,
        kind = FLUX_KIND_HELM_RELEASE,
        source_kind = FLUX_KIND_GIT_REPOSITORY,
        metadata_key = KUBE_KEY_METADATA,
        name_key = KUBE_KEY_NAME,
        namespace_key = KUBE_KEY_NAMESPACE,
        spec_key = KUBE_KEY_SPEC,
        name = name,
        namespace = opts.namespace,
        interval_key = FLUX_KEY_INTERVAL,
        interval = opts.interval,
        chart_key = FLUX_KEY_CHART,
        chart_name_key = FLUX_HELMCHART_TEMPLATE_KEY_CHART,
        chart_path = opts.chart_path,
        source_ref_key = FLUX_KEY_SOURCE_REF,
        values_key = FLUX_KEY_VALUES,
        library_name = DEFAULT_LIBRARY_NAME,
        enabled_key = HELM_VALUES_KEY_ENABLED,
        retries_default = FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT,
        retries_key = FLUX_HELMRELEASE_KEY_RETRIES,
        remediation_key = FLUX_HELMRELEASE_KEY_REMEDIATION,
        install_key = FLUX_HELMRELEASE_KEY_INSTALL,
        upgrade_key = FLUX_HELMRELEASE_KEY_UPGRADE,
        remediate_last_failure_key = FLUX_HELMRELEASE_KEY_REMEDIATE_LAST_FAILURE,
        create_namespace_key = FLUX_HELMRELEASE_KEY_CREATE_NAMESPACE,
    );

    // The `spec.path` per-CR source-sub-tree scalar composes through
    // the canonical [`flux_kustomization_source_subtree`] helper
    // (re-exported from [`caixa_core::flux_kustomization_source_subtree`]),
    // so the substrate's canonical per-cluster / per-caixa GitRepository-
    // relative directory-tree seed (`./clusters/<cluster>/services/<nome>`)
    // lives at one composer instead of a verbatim inline
    // `format!("./clusters/{cluster}/services/{name}")` at this emit site.
    // Threaded through a `{source_subtree}` named-arg interpolation on
    // the paired `{path_key}` leaf-scalar-key emit. Peer to the sibling
    // [`caixa_core::oci_chart_ref`] / [`caixa_core::cilium_network_policy_name`]
    // / [`caixa_core::gateway_api_http_route_name`] canonical-composer
    // re-exports on the substrate-side canonical-load-bearing-scalar
    // axis.
    let source_subtree = flux_kustomization_source_subtree(&opts.cluster, &name);

    let kustomization = format!(
        "---\n\
         # Flux Kustomization that pins the GitRepository + HelmRelease.\n\
         # Paired path: pleme-io/k8s/clusters/{cluster}/services/{name}/\n\
         {api_version_key}: {kustomization_api_version}\n\
         {kind_key}: {kind}\n\
         {metadata_key}:\n  \
           {name_key}: {name}\n  \
           {namespace_key}: {flux_system}\n\
         {spec_key}:\n  \
           {interval_key}: {interval}\n  \
           {prune_key}: true\n  \
           {source_ref_key}:\n    \
             {kind_key}: {source_kind}\n    \
             {name_key}: {flux_system}\n  \
           {path_key}: {source_subtree}\n  \
           {health_checks_key}:\n    \
             - {api_version_key}: {api_version}\n      \
               {kind_key}: {health_kind}\n      \
               {name_key}: {name}\n      \
               {namespace_key}: {namespace}\n  \
           {timeout_key}: {timeout_default}\n",
        api_version_key = KUBE_KEY_API_VERSION,
        kustomization_api_version = FLUX_KUSTOMIZATION_API_VERSION,
        kind_key = KUBE_KEY_KIND,
        kind = FLUX_KIND_KUSTOMIZATION,
        source_kind = FLUX_KIND_GIT_REPOSITORY,
        health_kind = FLUX_KIND_HELM_RELEASE,
        metadata_key = KUBE_KEY_METADATA,
        name_key = KUBE_KEY_NAME,
        namespace_key = KUBE_KEY_NAMESPACE,
        spec_key = KUBE_KEY_SPEC,
        api_version = FLUX_HELMRELEASE_API_VERSION,
        name = name,
        namespace = opts.namespace,
        interval_key = FLUX_KEY_INTERVAL,
        interval = opts.interval,
        cluster = opts.cluster,
        flux_system = DEFAULT_FLUX_SYSTEM_NAMESPACE,
        source_ref_key = FLUX_KEY_SOURCE_REF,
        health_checks_key = FLUX_KEY_HEALTH_CHECKS,
        prune_key = FLUX_KUSTOMIZATION_KEY_PRUNE,
        path_key = FLUX_KUSTOMIZATION_KEY_PATH,
        timeout_key = FLUX_KUSTOMIZATION_KEY_TIMEOUT,
        timeout_default = DEFAULT_FLUX_KUSTOMIZATION_TIMEOUT,
        source_subtree = source_subtree,
    );
    // chart_name is reserved for a future kustomization.yaml `resources:`
    // entry pointing at the rendered Chart.yaml; not yet wired.
    let _ = chart_name;

    Ok(vec![
        BundleFile {
            path: std::path::PathBuf::from(FLUX_GITREPOSITORY_YAML_FILENAME),
            contents: gitrepo,
        },
        BundleFile {
            path: std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME),
            contents: helmrelease,
        },
        BundleFile {
            path: std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME),
            contents: kustomization,
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use caixa_core::{
        Caixa, CaixaKind, M2_BEHAVIOR_KEY_ON_INIT, M2_KEY_BEHAVIOR, M2_KEY_LIMITS,
        M2_KEY_UPGRADE_FROM, M2_LIMITS_KEY_CPU, M2_LIMITS_KEY_MEMORY, M2_UPGRADE_FROM_KEY_FROM,
        kube_root_str_field,
    };

    fn sample_caixa() -> Caixa {
        Caixa {
            nome: "hello-rio".into(),
            versao: "0.1.0".into(),
            kind: CaixaKind::Servico,
            edicao: Some("2026".into()),
            descricao: Some("Canonical Rust→wasm32-wasip2 caixa Servico.".into()),
            repositorio: Some("https://github.com/pleme-io/hello-rio".into()),
            licenca: Some("MIT".into()),
            autores: vec!["pleme-io".into()],
            etiquetas: vec!["hello-world".into()],
            deps: vec![],
            deps_dev: vec![],
            exe: vec![],
            bibliotecas: vec![],
            servicos: vec!["servicos/hello-rio.computeunit.yaml".into()],
            limits: None,
            behavior: None,
            upgrade_from: vec![],
            estrategia: None,
            max_restarts: None,
            restart_window: None,
            children: vec![],
            membros: vec![],
            contratos: vec![],
            politicas: None,
            placement: None,
            entrada: None,
        }
    }

    fn sample_cu_yaml() -> serde_yaml::Value {
        serde_yaml::from_str(
            r#"
apiVersion: wasm.pleme.io/v1alpha1
kind: ComputeUnit
metadata:
  name: hello-rio
  namespace: tatara-system
spec:
  module:
    source: oci://ghcr.io/pleme-io/hello-rio:v0.1.0
  trigger:
    service:
      port: 8080
      paths: ["/", "/hello", "/healthz"]
  capabilities:
    - http-in:0.0.0.0:8080
    - env
"#,
        )
        .unwrap()
    }

    #[test]
    fn default_namespace_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub const DEFAULT_NAMESPACE` was lifted to a
        // re-export of [`caixa_core::DEFAULT_NAMESPACE`] so the
        // namespace string lives in exactly one place across every
        // caixa renderer (caixa-flux + caixa-mesh today, every future
        // per-target renderer the substrate adds). Pin the equality
        // here so any local re-introduction of a sibling `pub const
        // DEFAULT_NAMESPACE: &str = "…"` (the canonical drift footgun
        // that motivated this lift, with the prior caixa-mesh doc-
        // comment explicitly acknowledging the duplication) is a
        // build-time test failure naming the offending drift, not a
        // silent apply-time CiliumNetworkPolicy / Gateway / HTTPRoute
        // `endpointSelector` namespace mismatch dropping every L7
        // contrato flow far from the source rebrand commit. Peer to
        // `caixa_mesh::tests::default_namespace_re_export_points_at_caixa_core_canonical`
        // on the sibling renderer crate.
        caixa_core::assert_str_reexport_identity(
            "DEFAULT_NAMESPACE",
            DEFAULT_NAMESPACE,
            caixa_core::DEFAULT_NAMESPACE,
        );
    }

    #[test]
    fn default_flux_reconcile_interval_re_export_points_at_caixa_core_canonical() {
        // The renderer's `DEFAULT_FLUX_RECONCILE_INTERVAL` was lifted
        // from the inline `"10m"` scalar-value literal at
        // [`ClusterBundleOpts::for_caixa`]'s per-caixa default seed (the
        // sole production-code site the substrate seeds into the
        // [`ClusterBundleOpts::interval`] field that [`cluster_bundle`]'s
        // three per-CR format-string templates thread through their
        // [`FLUX_KEY_INTERVAL`]-keyed `spec.interval` axis verbatim) to a
        // re-export of [`caixa_core::DEFAULT_FLUX_RECONCILE_INTERVAL`] so
        // the Flux v2 controller-side reconcile-cadence default scalar-
        // value string lives in exactly one place across every caixa
        // renderer. Pin the equality + static-data identity here so any
        // local re-introduction of a sibling `pub const
        // DEFAULT_FLUX_RECONCILE_INTERVAL: &str = "…"` (the canonical
        // drift footgun where a sibling local `pub const` could happen
        // to carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test failure
        // naming the offending drift, not a silent apply-time symptom —
        // the prior inline shape would have let a substrate-side
        // reconcile-cadence migration without a coordinated caixa-core
        // edit silently seed per-caixa Flux v2 CRs at a drifted per-CR
        // reconcile-schedule, splitting the substrate's per-caixa
        // convergence-freshness contract across renderer versions with
        // no diagnostic naming the cadence-drift root cause. Peer to
        // [`default_namespace_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-substrate-default-load-bearing-
        // scalar re-export surface.
        caixa_core::assert_str_reexport_identity(
            "DEFAULT_FLUX_RECONCILE_INTERVAL",
            DEFAULT_FLUX_RECONCILE_INTERVAL,
            caixa_core::DEFAULT_FLUX_RECONCILE_INTERVAL,
        );
    }

    #[test]
    fn cluster_bundle_opts_for_caixa_seeds_interval_from_lifted_default() {
        // Fail-before-pass-after pin: the substrate's per-caixa default
        // seed for [`ClusterBundleOpts::interval`] must resolve to the
        // lifted [`DEFAULT_FLUX_RECONCILE_INTERVAL`] verbatim. Before the
        // lift the field carried an inline `"10m"` literal at the sole
        // production-code call site (the [`ClusterBundleOpts::for_caixa`]
        // per-caixa default builder); a future substrate-side
        // reconcile-cadence migration on the canonical
        // [`caixa_core::DEFAULT_FLUX_RECONCILE_INTERVAL`] declaration
        // that failed to reach this seed site would silently split the
        // substrate's per-caixa Flux v2 convergence-freshness contract
        // between the operator-facing canonical default and the per-
        // caixa `cluster_bundle` renderer's seeded reconcile-schedule,
        // freezing every per-caixa Flux v2 CR at the drifted cadence
        // far from the rebrand commit's source. Pin the identity here
        // so a regression that re-introduces an inline literal at the
        // seed site surfaces at build time on this test's failure. Peer
        // to the sibling
        // [`cluster_bundle_every_flux_cr_carries_lifted_flux_key_interval_scalar`]
        // pin on the per-CR `spec.interval` axis — that test pins the
        // rendered scalar agrees with `opts.interval`, this test pins
        // `opts.interval`'s substrate-side seed agrees with the lifted
        // canonical default.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        assert_eq!(
            opts.interval, DEFAULT_FLUX_RECONCILE_INTERVAL,
            "the substrate's per-caixa Flux v2 reconcile-cadence default \
             seed must resolve to the lifted \
             `DEFAULT_FLUX_RECONCILE_INTERVAL` scalar — drift here \
             silently splits the substrate's per-caixa convergence-\
             freshness contract between the operator-facing canonical \
             default and the per-caixa seeded reconcile-schedule"
        );
    }

    #[test]
    fn default_flux_chart_source_subpath_re_export_points_at_caixa_core_canonical() {
        // The renderer's `DEFAULT_FLUX_CHART_SOURCE_SUBPATH` was lifted
        // from the inline `"chart".into()` scalar-value literal at
        // [`ClusterBundleOpts::for_caixa`]'s per-caixa default seed (the
        // sole production-code site the substrate seeds into the
        // [`ClusterBundleOpts::chart_path`] field that [`cluster_bundle`]'s
        // `helmrelease.yaml` format-string template threads through its
        // [`FLUX_HELMCHART_TEMPLATE_KEY_CHART`]-keyed `spec.chart.spec.chart`
        // axis verbatim) to a re-export of
        // [`caixa_core::DEFAULT_FLUX_CHART_SOURCE_SUBPATH`] so the Flux v2
        // helm-controller-side chart-directory-in-GitRepository-source
        // default scalar-value string lives in exactly one place across
        // every caixa renderer. Pin the equality + static-data identity
        // here so any local re-introduction of a sibling `pub const
        // DEFAULT_FLUX_CHART_SOURCE_SUBPATH: &str = "…"` (the canonical
        // drift footgun where a sibling local `pub const` could happen to
        // carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test failure
        // naming the offending drift, not a silent apply-time symptom —
        // the prior inline shape would have let a substrate-side chart-
        // directory-in-git-source migration without a coordinated caixa-
        // core edit silently seed per-caixa `HelmRelease` CRs at a
        // drifted per-CR chart-directory pointer, splitting the
        // substrate's per-caixa chart-open contract with the Flux v2
        // helm-controller across renderer versions with no diagnostic
        // naming the sub-path-drift root cause (the source-controller
        // would then either fail to open the paired per-caixa chart
        // directory or, worse, silently open a stale sibling directory
        // the drifted scalar happens to name). Peer to
        // [`default_flux_reconcile_interval_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-substrate-default-load-bearing-
        // scalar re-export surface.
        caixa_core::assert_str_reexport_identity(
            "DEFAULT_FLUX_CHART_SOURCE_SUBPATH",
            DEFAULT_FLUX_CHART_SOURCE_SUBPATH,
            caixa_core::DEFAULT_FLUX_CHART_SOURCE_SUBPATH,
        );
    }

    #[test]
    fn cluster_bundle_opts_for_caixa_seeds_chart_path_from_lifted_default() {
        // Fail-before-pass-after pin: the substrate's per-caixa default
        // seed for [`ClusterBundleOpts::chart_path`] must resolve to the
        // lifted [`DEFAULT_FLUX_CHART_SOURCE_SUBPATH`] verbatim. Before
        // the lift the field carried an inline `"chart".into()` literal
        // at the sole production-code call site (the
        // [`ClusterBundleOpts::for_caixa`] per-caixa default builder); a
        // future substrate-side chart-directory-in-git-source migration
        // on the canonical
        // [`caixa_core::DEFAULT_FLUX_CHART_SOURCE_SUBPATH`] declaration
        // that failed to reach this seed site would silently split the
        // substrate's per-caixa Flux v2 chart-open contract between the
        // operator-facing canonical default and the per-caixa
        // `cluster_bundle` renderer's seeded chart-directory pointer,
        // freezing every per-caixa `HelmRelease` CR at the drifted sub-
        // path far from the rebrand commit's source. Pin the identity
        // here so a regression that re-introduces an inline literal at
        // the seed site surfaces at build time on this test's failure.
        // Peer to the sibling
        // [`cluster_bundle_opts_for_caixa_seeds_interval_from_lifted_default`]
        // pin on the co-resident per-caixa Flux v2 CR substrate-default
        // seed surface — that test pins `opts.interval`'s substrate-side
        // seed agrees with the lifted canonical default, this test pins
        // `opts.chart_path`'s substrate-side seed agrees with the peer
        // lifted canonical default.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        assert_eq!(
            opts.chart_path, DEFAULT_FLUX_CHART_SOURCE_SUBPATH,
            "the substrate's per-caixa Flux v2 chart-directory-in-git-\
             source default seed must resolve to the lifted \
             `DEFAULT_FLUX_CHART_SOURCE_SUBPATH` scalar — drift here \
             silently splits the substrate's per-caixa chart-open \
             contract between the operator-facing canonical default \
             and the per-caixa seeded chart-directory pointer"
        );
    }

    #[test]
    fn flux_helmrelease_remediation_retries_default_re_export_points_at_caixa_core_canonical() {
        // The renderer's `FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`
        // was lifted from the two production-code duplicated inline
        // `retries: 3` scalar-value literals inside [`cluster_bundle`]'s
        // `helmrelease.yaml` format-string template (the install-path
        // `install.remediation.retries` site + the upgrade-path
        // `upgrade.remediation.retries` site) to a re-export of
        // [`caixa_core::FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`] so
        // the Flux v2 helm-controller-side per-CR remediation-retries
        // default scalar-value lives in exactly one place across every
        // caixa renderer. Pin the equality here so any local
        // re-introduction of a sibling `pub const
        // FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT: u32 = <n>` (the
        // canonical drift footgun where a sibling local `pub const`
        // could happen to carry a different value while the rest of the
        // codebase keeps consuming the caixa-core canonical) is a
        // build-time test failure naming the offending drift, not a
        // silent apply-time symptom — the prior duplicated inline shape
        // would have let a substrate-side retry-ceiling migration on the
        // canonical caixa-core declaration without a coordinated caixa-
        // flux edit silently seed per-caixa Flux v2 `HelmRelease` CRs at
        // a drifted per-path remediation-retries schedule, splitting the
        // substrate's canonical retry-ceiling between the install-path
        // and the upgrade-path with no diagnostic naming the ceiling-
        // drift root cause. Peer to
        // [`default_flux_reconcile_interval_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-substrate-default-load-bearing-scalar
        // re-export surface.
        assert_eq!(
            FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT,
            caixa_core::FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT,
            "`caixa_flux::FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT` \
             must be the value-identical re-export of the canonical \
             `caixa_core::FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT` — \
             drift here silently splits the substrate's per-caixa Flux \
             v2 `HelmRelease` remediation-retries ceiling between the \
             canonical caixa-core declaration and the renderer's threaded \
             seed"
        );
    }

    #[test]
    fn flux_helmrelease_key_retries_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_HELMRELEASE_KEY_RETRIES`
        // is the single source of truth for the Flux v2 per-CR
        // remediation-retries leaf-scalar-key baked into both the
        // install-path + upgrade-path `retries:` sub-block leaf-headers
        // of the [`cluster_bundle`] `helmrelease.yaml` format-string
        // template (both threading the same `&'static str` through the
        // new `{retries_key}` named-arg interpolation) plus the two
        // test-fixture navigation sites in `mod tests` that probe the
        // rendered document at `.get(FLUX_HELMRELEASE_KEY_RETRIES)`. Pin
        // the equality (and the static-data identity, peer with the
        // sibling
        // [`flux_helmrelease_remediation_retries_default_re_export_points_at_caixa_core_canonical`]
        // pin on the scalar-value half of the same
        // `(leaf-key, scalar-value)` per-path retry-cap declaration
        // pair) so any local re-introduction of a sibling
        // `pub const FLUX_HELMRELEASE_KEY_RETRIES: &str = "…"` (the
        // canonical drift footgun this lift closes — the load-bearing
        // Flux-v2-helm-controller-side per-CR remediation-retries leaf-
        // scalar-key across four prior inlined occurrences — two
        // production emit sites in the `cluster_bundle` `helmrelease.yaml`
        // format-string template plus two test-fixture navigation sites,
        // lifted to one re-export at the caixa-core boundary) is a
        // build-time test failure naming the offending drift, not a
        // silent apply-time symptom (a stripped `retries:` leaf letting
        // the helm-controller fall back to the Flux v2 upstream default).
        assert_eq!(
            FLUX_HELMRELEASE_KEY_RETRIES,
            caixa_core::FLUX_HELMRELEASE_KEY_RETRIES
        );
        assert!(
            std::ptr::eq(
                FLUX_HELMRELEASE_KEY_RETRIES.as_ptr(),
                caixa_core::FLUX_HELMRELEASE_KEY_RETRIES.as_ptr(),
            ),
            "FLUX_HELMRELEASE_KEY_RETRIES must be a re-export of \
             caixa_core::FLUX_HELMRELEASE_KEY_RETRIES, not a sibling `pub const` \
             that happens to carry the same string — drift between the two is \
             the canonical footgun this lift closes"
        );
    }

    #[test]
    fn flux_helmrelease_key_remediation_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_HELMRELEASE_KEY_REMEDIATION`
        // is the single source of truth for the Flux v2 per-CR
        // remediation sub-container-axis-key baked into both the
        // install-path + upgrade-path `remediation:` sub-block-header
        // lines of the [`cluster_bundle`] `helmrelease.yaml` format-
        // string template (both threading the same `&'static str`
        // through the new `{remediation_key}` named-arg interpolation)
        // plus the two test-fixture navigation sites in `mod tests` that
        // probe the rendered document at
        // `.get(FLUX_HELMRELEASE_KEY_REMEDIATION)`. Pin the equality
        // (and the static-data identity, peer with the sibling
        // [`flux_helmrelease_key_retries_re_export_points_at_caixa_core_canonical`]
        // pin on the leaf-scalar-key half of the same
        // `(container-axis-key, leaf-scalar-key, scalar-value)` per-path
        // retry-cap declaration triple) so any local re-introduction of
        // a sibling `pub const FLUX_HELMRELEASE_KEY_REMEDIATION: &str =
        // "…"` (the canonical drift footgun this lift closes — the
        // load-bearing Flux-v2-helm-controller-side per-CR remediation
        // sub-container-axis-key across four prior inlined occurrences —
        // two production emit sites in the `cluster_bundle`
        // `helmrelease.yaml` format-string template plus two test-
        // fixture navigation sites, lifted to one re-export at the
        // caixa-core boundary) is a build-time test failure naming the
        // offending drift, not a silent apply-time symptom (a stripped
        // whole-remediation sub-block letting the helm-controller fall
        // back to the Flux v2 upstream defaults for the whole per-path
        // remediation surface).
        caixa_core::assert_str_reexport_identity(
            "FLUX_HELMRELEASE_KEY_REMEDIATION",
            FLUX_HELMRELEASE_KEY_REMEDIATION,
            caixa_core::FLUX_HELMRELEASE_KEY_REMEDIATION,
        );
    }

    #[test]
    fn flux_helmrelease_key_remediation_pins_canonical_remediation_string() {
        // Bridge-arm pin: the lifted [`FLUX_HELMRELEASE_KEY_REMEDIATION`]
        // constant resolves to the canonical `"remediation"` string
        // today, and both rendered Flux v2 `HelmRelease` per-path per-CR
        // sub-block-headers must spell it out verbatim. Pin the literal
        // here (peer with the sibling
        // [`flux_helmrelease_key_retries_pins_canonical_retries_string`]-shape
        // canonical-default arm on the sibling per-CR leaf-scalar-key
        // surface + the peer
        // [`flux_key_source_ref_pins_canonical_source_ref_string`]-shape
        // canonical-default arm on the peer per-CR container-axis-key
        // surface) so a future rebrand of the lifted constant (Flux v3
        // rename like `recovery` / `retryPolicy` / `errorHandling` —
        // upstream Flux v3 roadmap candidates that would land the same
        // per-CR retry-cap semantics under a different sub-container-
        // axis-key) surfaces here as a coordinated edit-point.
        assert_eq!(FLUX_HELMRELEASE_KEY_REMEDIATION, "remediation");
    }

    #[test]
    fn flux_helmrelease_key_install_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_HELMRELEASE_KEY_INSTALL`
        // is the single source of truth for the Flux v2 per-CR install-
        // path helm-action-phase discriminator parent-container-axis-key
        // baked into the `install:` sub-block-header line of the
        // [`cluster_bundle`] `helmrelease.yaml` format-string template
        // (threading the same `&'static str` through the new
        // `{install_key}` named-arg interpolation) plus the one test-
        // fixture navigation site in `mod tests` that probes the
        // rendered document at `.get(FLUX_HELMRELEASE_KEY_INSTALL)`. Pin
        // the equality (and the static-data identity, peer with the
        // sibling
        // [`flux_helmrelease_key_remediation_re_export_points_at_caixa_core_canonical`]
        // pin on the sub-container-axis-key half of the same
        // `(parent-container-key, sub-container-key, leaf-key, scalar-
        // value)` per-path retry-cap declaration quartet) so any local
        // re-introduction of a sibling `pub const
        // FLUX_HELMRELEASE_KEY_INSTALL: &str = "…"` (the canonical drift
        // footgun this lift closes — the load-bearing Flux-v2-helm-
        // controller-side per-CR install-path phase-discriminator
        // parent-container-axis-key across two prior inlined occurrences —
        // one production emit site in the `cluster_bundle`
        // `helmrelease.yaml` format-string template plus one test-
        // fixture navigation site, lifted to one re-export at the caixa-
        // core boundary) is a build-time test failure naming the
        // offending drift, not a silent apply-time symptom (a stripped
        // whole-install-path per-CR phase block letting the helm-
        // controller fall back to the Flux v2 upstream defaults for the
        // whole install-path phase surface).
        caixa_core::assert_str_reexport_identity(
            "FLUX_HELMRELEASE_KEY_INSTALL",
            FLUX_HELMRELEASE_KEY_INSTALL,
            caixa_core::FLUX_HELMRELEASE_KEY_INSTALL,
        );
    }

    #[test]
    fn flux_helmrelease_key_install_pins_canonical_install_string() {
        // Bridge-arm pin: the lifted [`FLUX_HELMRELEASE_KEY_INSTALL`]
        // constant resolves to the canonical `"install"` string today,
        // and the rendered Flux v2 `HelmRelease` per-CR install-path
        // phase sub-block-header must spell it out verbatim. Pin the
        // literal here (peer with the sibling
        // [`flux_helmrelease_key_remediation_pins_canonical_remediation_string`]-shape
        // canonical-default arm on the sibling per-CR sub-container-
        // axis-key surface + the sibling
        // [`flux_helmrelease_key_retries_pins_canonical_retries_string`]-shape
        // canonical-default arm on the sibling per-CR leaf-scalar-key
        // surface) so a future rebrand of the lifted constant (Flux v3
        // rename like `initialize` / `apply` / `create` / `first-run` —
        // upstream Flux v3 roadmap candidates that would land the same
        // per-CR first-time chart apply phase semantics under a
        // different parent-container-axis-key) surfaces here as a
        // coordinated edit-point.
        assert_eq!(FLUX_HELMRELEASE_KEY_INSTALL, "install");
    }

    #[test]
    fn flux_helmrelease_key_upgrade_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_HELMRELEASE_KEY_UPGRADE`
        // is the single source of truth for the Flux v2 per-CR upgrade-
        // path helm-action-phase discriminator parent-container-axis-key
        // baked into the `upgrade:` sub-block-header line of the
        // [`cluster_bundle`] `helmrelease.yaml` format-string template
        // (threading the same `&'static str` through the new
        // `{upgrade_key}` named-arg interpolation) plus the one test-
        // fixture navigation site in `mod tests` that probes the
        // rendered document at `.get(FLUX_HELMRELEASE_KEY_UPGRADE)`. Pin
        // the equality (and the static-data identity, peer with the
        // sibling
        // [`flux_helmrelease_key_install_re_export_points_at_caixa_core_canonical`]
        // pin on the peer install-path phase-discriminator half of the
        // same per-CR helm-action-phase discriminator parent-container-
        // axis-key pair) so any local re-introduction of a sibling
        // `pub const FLUX_HELMRELEASE_KEY_UPGRADE: &str = "…"` is a
        // build-time test failure naming the offending drift, not a
        // silent apply-time symptom (a stripped whole-upgrade-path per-
        // CR phase block letting the helm-controller fall back to the
        // Flux v2 upstream defaults for the whole upgrade-path phase
        // surface, silently dropping the substrate's
        // `remediateLastFailure: true` toggle + the per-CR retry-cap
        // ceiling from every per-version chart re-apply).
        caixa_core::assert_str_reexport_identity(
            "FLUX_HELMRELEASE_KEY_UPGRADE",
            FLUX_HELMRELEASE_KEY_UPGRADE,
            caixa_core::FLUX_HELMRELEASE_KEY_UPGRADE,
        );
    }

    #[test]
    fn flux_helmrelease_key_upgrade_pins_canonical_upgrade_string() {
        // Bridge-arm pin: the lifted [`FLUX_HELMRELEASE_KEY_UPGRADE`]
        // constant resolves to the canonical `"upgrade"` string today,
        // and the rendered Flux v2 `HelmRelease` per-CR upgrade-path
        // phase sub-block-header must spell it out verbatim. Pin the
        // literal here (peer with the sibling
        // [`flux_helmrelease_key_install_pins_canonical_install_string`]-shape
        // canonical-default arm on the peer per-CR install-path phase-
        // discriminator surface) so a future rebrand of the lifted
        // constant (Flux v3 rename like `reapply` / `reconcile` /
        // `update` / `promote` — upstream Flux v3 roadmap candidates
        // that would land the same per-CR per-version chart re-apply
        // phase semantics under a different parent-container-axis-key)
        // surfaces here as a coordinated edit-point.
        assert_eq!(FLUX_HELMRELEASE_KEY_UPGRADE, "upgrade");
    }

    #[test]
    fn flux_helmrelease_key_retries_pins_canonical_retries_string() {
        // Bridge-arm pin: the lifted [`FLUX_HELMRELEASE_KEY_RETRIES`]
        // constant resolves to the canonical `"retries"` string today,
        // and both rendered Flux v2 `HelmRelease` per-path retry-cap
        // leaf sub-block headers must spell it out verbatim. Pin the
        // literal here (peer with the sibling
        // [`flux_key_source_ref_pins_canonical_source_ref_string`]-shape
        // canonical-default arm on the sibling per-CR container-axis-key
        // surface) so a future rebrand of the lifted constant (Flux v3
        // rename like `attempts` / `maxRetries` / `retryCount` — the
        // upstream Gateway-API-side `spec.rules[].retry.attempts` leaf
        // already uses `attempts` on the sibling `GATEWAY_API_KEY_ATTEMPTS`
        // axis, an independent CRD group's evolution the two `pub const`
        // declarations stay sibling constants against) surfaces here as
        // a coordinated edit-point.
        assert_eq!(FLUX_HELMRELEASE_KEY_RETRIES, "retries");
    }

    #[test]
    fn cluster_bundle_helmrelease_install_remediation_retries_pins_lifted_default() {
        // Fail-before-pass-after pin: the rendered `helmrelease.yaml`
        // document's `spec.install.remediation.retries` scalar must
        // resolve to the lifted
        // [`FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`] verbatim.
        // Before the lift the axis carried an inline `retries: 3`
        // scalar-value literal at the sole production-code call site
        // (the install-path `install.remediation.retries` position of
        // the [`cluster_bundle`] `helmrelease.yaml` format-string
        // template); a future substrate-side retry-ceiling migration on
        // the canonical
        // [`caixa_core::FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`]
        // declaration that failed to reach this emit site would
        // silently split the substrate's canonical retry-ceiling
        // between the operator-facing canonical default and the per-
        // caixa `helmrelease.yaml` install-path retry cap the helm-
        // controller consumes at first-time chart apply time. Pin the
        // identity here so a regression that re-introduces an inline
        // literal at the emit site surfaces at build time on this
        // test's failure. Peer to the sibling
        // [`cluster_bundle_helmrelease_upgrade_remediation_retries_pins_lifted_default`]
        // pin on the upgrade-path retry-cap sibling axis — that test
        // pins the rendered upgrade-path scalar agrees with the lifted
        // default, this test pins the rendered install-path scalar
        // agrees with the same.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME))
            .expect("helmrelease.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&hr.contents).expect("helmrelease.yaml parses as YAML");
        let install_retries = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_HELMRELEASE_KEY_INSTALL))
            .and_then(|i| i.get(FLUX_HELMRELEASE_KEY_REMEDIATION))
            .and_then(|r| r.get(FLUX_HELMRELEASE_KEY_RETRIES))
            .and_then(|v| v.as_u64())
            .expect(
                "spec.install.remediation.retries scalar present; drift on \
                 this axis silently splits the substrate's canonical retry-\
                 ceiling between the install-path first-time chart apply and \
                 the canonical `FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`",
            );
        assert_eq!(
            install_retries,
            u64::from(FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT),
            "spec.install.remediation.retries must carry the lifted \
             `FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT` scalar — \
             drift here silently splits the substrate's canonical retry-\
             ceiling between the operator-facing canonical default and \
             the install-path retry cap the Flux v2 helm-controller \
             consumes at first-time chart apply time"
        );
    }

    #[test]
    fn cluster_bundle_helmrelease_upgrade_remediation_retries_pins_lifted_default() {
        // Fail-before-pass-after pin: the rendered `helmrelease.yaml`
        // document's `spec.upgrade.remediation.retries` scalar must
        // resolve to the lifted
        // [`FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`] verbatim.
        // Before the lift the axis carried a second inline `retries: 3`
        // scalar-value literal at the sole production-code call site
        // (the upgrade-path `upgrade.remediation.retries` position of
        // the [`cluster_bundle`] `helmrelease.yaml` format-string
        // template, sibling to the install-path retry-cap the peer
        // [`cluster_bundle_helmrelease_install_remediation_retries_pins_lifted_default`]
        // pin covers); a future substrate-side retry-ceiling migration
        // on the canonical
        // [`caixa_core::FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`]
        // declaration that failed to reach this emit site would
        // silently split the substrate's canonical retry-ceiling
        // between the operator-facing canonical default and the per-
        // caixa `helmrelease.yaml` upgrade-path retry cap the helm-
        // controller consumes on every subsequent per-version chart re-
        // apply the same `HelmRelease` CR gates. Pin the identity here
        // so a regression that re-introduces an inline literal at the
        // emit site surfaces at build time on this test's failure. Peer
        // to the sibling
        // [`cluster_bundle_helmrelease_install_remediation_retries_pins_lifted_default`]
        // pin on the install-path retry-cap sibling axis — closing the
        // per-path retry-cap sweep the caixa-core lift established.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME))
            .expect("helmrelease.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&hr.contents).expect("helmrelease.yaml parses as YAML");
        let upgrade_retries = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_HELMRELEASE_KEY_UPGRADE))
            .and_then(|u| u.get(FLUX_HELMRELEASE_KEY_REMEDIATION))
            .and_then(|r| r.get(FLUX_HELMRELEASE_KEY_RETRIES))
            .and_then(|v| v.as_u64())
            .expect(
                "spec.upgrade.remediation.retries scalar present; drift on \
                 this axis silently splits the substrate's canonical retry-\
                 ceiling between the upgrade-path per-version chart re-apply \
                 and the canonical `FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`",
            );
        assert_eq!(
            upgrade_retries,
            u64::from(FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT),
            "spec.upgrade.remediation.retries must carry the lifted \
             `FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT` scalar — \
             drift here silently splits the substrate's canonical retry-\
             ceiling between the operator-facing canonical default and \
             the upgrade-path retry cap the Flux v2 helm-controller \
             consumes on every subsequent per-caixa-version chart re-apply"
        );
    }

    #[test]
    fn cluster_bundle_helmrelease_upgrade_remediation_remediate_last_failure_pins_lifted_true() {
        // Fail-before-pass-after pin: the rendered `helmrelease.yaml`
        // document's `spec.upgrade.remediation.remediateLastFailure`
        // upgrade-path-only per-CR remediation-toggle leaf-scalar-key must
        // resolve to `true` verbatim under the lifted
        // [`FLUX_HELMRELEASE_KEY_REMEDIATE_LAST_FAILURE`] leaf-key. Before
        // the lift the axis carried an inline `remediateLastFailure: true`
        // leaf-scalar-key literal at the sole production-code call site
        // (the upgrade-path `upgrade.remediation.remediateLastFailure`
        // position of the [`cluster_bundle`] `helmrelease.yaml` format-
        // string template, sibling to the upgrade-path retry-cap the peer
        // [`cluster_bundle_helmrelease_upgrade_remediation_retries_pins_lifted_default`]
        // pin covers); a future substrate-side rebrand on the canonical
        // [`caixa_core::FLUX_HELMRELEASE_KEY_REMEDIATE_LAST_FAILURE`]
        // declaration that failed to reach this emit site would silently
        // strip the substrate's chosen post-retry-exhaustion rollback
        // semantic from every emitted per-caixa `HelmRelease` document —
        // the Flux v2 helm-controller would then leave every terminally-
        // failed upgrade in the failed state without rolling back to the
        // prior last-known-good release the substrate's "no chart apply
        // leaves a per-caixa CR in a stalled, unremediated state"
        // MESH-COMPOSITION.md §V guarantee mandates. Pin the identity
        // here so a regression that re-introduces an inline literal at
        // the emit site — or a rebrand on the canonical const that fails
        // to reach the emit site — surfaces at build time on this test's
        // failure. Peer to the sibling
        // [`cluster_bundle_helmrelease_upgrade_remediation_retries_pins_lifted_default`]
        // pin on the upgrade-path retry-cap sibling axis — closes the
        // `spec.upgrade.remediation.{retries, remediateLastFailure}` leaf-
        // scalar-key pair the substrate seeds under the upgrade-path per-
        // CR remediation sub-container.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME))
            .expect("helmrelease.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&hr.contents).expect("helmrelease.yaml parses as YAML");
        let remediate_last_failure = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_HELMRELEASE_KEY_UPGRADE))
            .and_then(|u| u.get(FLUX_HELMRELEASE_KEY_REMEDIATION))
            .and_then(|r| r.get(FLUX_HELMRELEASE_KEY_REMEDIATE_LAST_FAILURE))
            .and_then(|v| v.as_bool())
            .expect(
                "spec.upgrade.remediation.remediateLastFailure boolean scalar \
                 present; drift on this axis silently drops the substrate's \
                 chosen post-retry-exhaustion rollback semantic from every \
                 emitted per-caixa `HelmRelease` document, leaving every \
                 terminally-failed upgrade in the failed state without \
                 rolling back to the prior last-known-good release",
            );
        assert!(
            remediate_last_failure,
            "spec.upgrade.remediation.remediateLastFailure must carry the \
             substrate's canonical `true` seed — drift to `false` silently \
             drops the post-retry-exhaustion rollback semantic the Flux v2 \
             helm-controller's per-CR upgrade-path remediation loop keys \
             off to trigger the prior-release rollback pipeline, and every \
             terminally-failed per-caixa upgrade sits in the failed state \
             indefinitely with no diagnostic naming the remediation-toggle-\
             drift root cause"
        );
    }

    #[test]
    fn flux_helmrelease_key_remediate_last_failure_re_export_points_at_caixa_core_canonical() {
        // The renderer's `FLUX_HELMRELEASE_KEY_REMEDIATE_LAST_FAILURE` was
        // lifted from the production-code inline `remediateLastFailure`
        // literal at the sole `cluster_bundle` `helmrelease.yaml` format-
        // string template's upgrade-path per-CR remediation-toggle leaf-
        // scalar-key emit site + the test-fixture navigation site the
        // sibling
        // [`cluster_bundle_helmrelease_upgrade_remediation_remediate_last_failure_pins_lifted_true`]
        // pin opens onto the rendered document, to a re-export of
        // [`caixa_core::FLUX_HELMRELEASE_KEY_REMEDIATE_LAST_FAILURE`] so
        // the canonical Flux v2 per-CR upgrade-path per-CR remediation-
        // toggle leaf-scalar-key lives in exactly one place across every
        // caixa renderer. Pin the equality + static-data identity here
        // so any local re-introduction of a sibling
        // `pub const FLUX_HELMRELEASE_KEY_REMEDIATE_LAST_FAILURE: &str = "…"`
        // at this crate (the canonical drift footgun where a sibling
        // local `pub const` could happen to carry the same string at the
        // source while pointing at a different `&'static` allocation) is
        // a build-time test failure naming the offending drift. Peer to
        // [`flux_helmrelease_key_retries_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-CR retry-cap leaf-scalar-key re-export axis.
        caixa_core::assert_str_reexport_identity(
            "FLUX_HELMRELEASE_KEY_REMEDIATE_LAST_FAILURE",
            FLUX_HELMRELEASE_KEY_REMEDIATE_LAST_FAILURE,
            caixa_core::FLUX_HELMRELEASE_KEY_REMEDIATE_LAST_FAILURE,
        );
    }

    #[test]
    fn cluster_bundle_helmrelease_install_create_namespace_pins_lifted_true() {
        // Fail-before-pass-after pin: the rendered `helmrelease.yaml`
        // document's `spec.install.createNamespace` install-path-only
        // per-CR namespace-seeder-toggle leaf-scalar-key must resolve to
        // `true` verbatim under the lifted
        // [`FLUX_HELMRELEASE_KEY_CREATE_NAMESPACE`] leaf-key. Before the
        // lift the axis carried an inline `createNamespace: true` leaf-
        // scalar-key literal at the sole production-code call site (the
        // install-path `install.createNamespace` position of the
        // [`cluster_bundle`] `helmrelease.yaml` format-string template,
        // mirror-symmetric to the upgrade-path remediation-toggle the peer
        // [`cluster_bundle_helmrelease_upgrade_remediation_remediate_last_failure_pins_lifted_true`]
        // pin covers); a future substrate-side rebrand on the canonical
        // [`caixa_core::FLUX_HELMRELEASE_KEY_CREATE_NAMESPACE`]
        // declaration that failed to reach this emit site would silently
        // strip the substrate's chosen first-apply namespace-seeder
        // semantic from every emitted per-caixa `HelmRelease` document —
        // the Flux v2 helm-controller would then refuse every first-time
        // per-caixa chart apply against a fresh cluster whose target
        // namespace has not been pre-provisioned by an out-of-band
        // pipeline the substrate's "no per-caixa Servico apply is blocked
        // on manual namespace preprovisioning" MESH-COMPOSITION.md §V
        // install-path-fluency guarantee mandates. Pin the identity here
        // so a regression that re-introduces an inline literal at the
        // emit site — or a rebrand on the canonical const that fails to
        // reach the emit site — surfaces at build time on this test's
        // failure. Peer to the sibling
        // [`cluster_bundle_helmrelease_upgrade_remediation_remediate_last_failure_pins_lifted_true`]
        // pin on the mirror-symmetric upgrade-path per-CR remediation-
        // toggle axis — closes the
        // `spec.{install.createNamespace, upgrade.remediation.remediateLastFailure}`
        // per-CR phase-specific toggle leaf-scalar-key pair the substrate
        // seeds under mirror-symmetric parent-container-axis-keys.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME))
            .expect("helmrelease.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&hr.contents).expect("helmrelease.yaml parses as YAML");
        let create_namespace = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_HELMRELEASE_KEY_INSTALL))
            .and_then(|i| i.get(FLUX_HELMRELEASE_KEY_CREATE_NAMESPACE))
            .and_then(|v| v.as_bool())
            .expect(
                "spec.install.createNamespace boolean scalar present; drift \
                 on this axis silently drops the substrate's chosen first-\
                 apply namespace-seeder semantic from every emitted per-\
                 caixa `HelmRelease` document, leaving every first-time \
                 per-caixa chart apply against a fresh cluster refused by \
                 the helm-controller because the target namespace was not \
                 pre-provisioned by an out-of-band pipeline",
            );
        assert!(
            create_namespace,
            "spec.install.createNamespace must carry the substrate's \
             canonical `true` seed — drift to `false` silently drops the \
             first-apply namespace-seeder semantic the Flux v2 helm-\
             controller's per-CR install-path pre-apply loop keys off to \
             materialize the target namespace before the first chart \
             apply, and every first-time per-caixa Servico chart apply \
             against a fresh cluster is refused with no diagnostic naming \
             the seeder-toggle-drift root cause"
        );
    }

    #[test]
    fn flux_helmrelease_key_create_namespace_re_export_points_at_caixa_core_canonical() {
        // The renderer's `FLUX_HELMRELEASE_KEY_CREATE_NAMESPACE` was
        // lifted from the production-code inline `createNamespace`
        // literal at the sole `cluster_bundle` `helmrelease.yaml` format-
        // string template's install-path per-CR namespace-seeder-toggle
        // leaf-scalar-key emit site + the test-fixture navigation site
        // the sibling
        // [`cluster_bundle_helmrelease_install_create_namespace_pins_lifted_true`]
        // pin opens onto the rendered document, to a re-export of
        // [`caixa_core::FLUX_HELMRELEASE_KEY_CREATE_NAMESPACE`] so the
        // canonical Flux v2 per-CR install-path per-CR namespace-seeder-
        // toggle leaf-scalar-key lives in exactly one place across every
        // caixa renderer. Pin the equality + static-data identity here so
        // any local re-introduction of a sibling
        // `pub const FLUX_HELMRELEASE_KEY_CREATE_NAMESPACE: &str = "…"`
        // at this crate (the canonical drift footgun where a sibling
        // local `pub const` could happen to carry the same string at the
        // source while pointing at a different `&'static` allocation) is
        // a build-time test failure naming the offending drift. Peer to
        // [`flux_helmrelease_key_remediate_last_failure_re_export_points_at_caixa_core_canonical`]
        // on the sibling mirror-symmetric upgrade-path-only per-CR
        // remediation-toggle leaf-scalar-key re-export axis.
        caixa_core::assert_str_reexport_identity(
            "FLUX_HELMRELEASE_KEY_CREATE_NAMESPACE",
            FLUX_HELMRELEASE_KEY_CREATE_NAMESPACE,
            caixa_core::FLUX_HELMRELEASE_KEY_CREATE_NAMESPACE,
        );
    }

    #[test]
    fn cluster_bundle_kustomization_prune_pins_lifted_true() {
        // Fail-before-pass-after pin: the rendered `kustomization.yaml`
        // document's `spec.prune` per-CR garbage-collection-toggle leaf-
        // scalar-key must resolve to `true` verbatim under the lifted
        // [`FLUX_KUSTOMIZATION_KEY_PRUNE`] leaf-key. Before the lift the
        // axis carried an inline `prune: true` leaf-scalar-key literal
        // at the sole production-code call site (the top-level `spec`
        // position of the [`cluster_bundle`] `kustomization.yaml`
        // format-string template, mirror-symmetric to the per-caixa
        // `HelmRelease` CR install-path-only namespace-seeder-toggle the
        // peer
        // [`cluster_bundle_helmrelease_install_create_namespace_pins_lifted_true`]
        // pin covers on the co-resident per-caixa `HelmRelease` CR); a
        // future substrate-side rebrand on the canonical
        // [`caixa_core::FLUX_KUSTOMIZATION_KEY_PRUNE`] declaration that
        // failed to reach this emit site would silently strip the
        // substrate's chosen sweep-what-you-removed semantic from every
        // emitted per-caixa `Kustomization` document — the Flux v2
        // kustomize-controller would then leave every per-caixa
        // resource the source manifest set previously reconciled but no
        // longer carries dangling in the cluster the substrate's "the
        // cluster's per-caixa live state converges to the caixa's
        // tatara-lisp source-of-truth on every reconcile — resources
        // the source no longer carries are swept by the kustomize-
        // controller, not left dangling" CAIXA-SDLC.md §V author-to-
        // live-convergence guarantee mandates. Pin the identity here so
        // a regression that re-introduces an inline literal at the emit
        // site — or a rebrand on the canonical const that fails to
        // reach the emit site — surfaces at build time on this test's
        // failure. Peer to the sibling
        // [`cluster_bundle_helmrelease_install_create_namespace_pins_lifted_true`]
        // pin on the co-resident per-`HelmRelease`-CR install-path per-
        // CR namespace-seeder-toggle axis — extends the per-CR-toggle
        // leaf-scalar-key discipline from the co-resident per-
        // `HelmRelease`-CR spec surface onto the co-resident per-
        // `Kustomization`-CR spec surface at the mirror-symmetric top-
        // level `spec.prune` position.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let ks = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
            .expect("kustomization.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&ks.contents).expect("kustomization.yaml parses as YAML");
        let prune = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_KUSTOMIZATION_KEY_PRUNE))
            .and_then(|v| v.as_bool())
            .expect(
                "spec.prune boolean scalar present; drift on this axis \
                 silently drops the substrate's chosen sweep-what-you-\
                 removed semantic from every emitted per-caixa \
                 `Kustomization` document, leaving per-caixa resources \
                 the source manifest set previously reconciled but no \
                 longer carries dangling in the cluster",
            );
        assert!(
            prune,
            "spec.prune must carry the substrate's canonical `true` seed \
             — drift to `false` silently drops the sweep-what-you-removed \
             semantic the Flux v2 kustomize-controller's per-CR reconcile \
             loop keys off to garbage-collect per-caixa resources removed \
             from the source manifest set between reconciles, and every \
             per-caixa `Kustomization` reconcile leaves orphaned resources \
             dangling in the cluster with no diagnostic naming the toggle-\
             drift root cause"
        );
    }

    #[test]
    fn flux_kustomization_key_prune_re_export_points_at_caixa_core_canonical() {
        // The renderer's `FLUX_KUSTOMIZATION_KEY_PRUNE` was lifted from
        // the production-code inline `prune` literal at the sole
        // `cluster_bundle` `kustomization.yaml` format-string template's
        // per-CR garbage-collection-toggle leaf-scalar-key emit site +
        // the test-fixture navigation site the sibling
        // [`cluster_bundle_kustomization_prune_pins_lifted_true`] pin
        // opens onto the rendered document, to a re-export of
        // [`caixa_core::FLUX_KUSTOMIZATION_KEY_PRUNE`] so the canonical
        // Flux v2 per-CR garbage-collection-toggle leaf-scalar-key lives
        // in exactly one place across every caixa renderer. Pin the
        // equality + static-data identity here so any local re-
        // introduction of a sibling
        // `pub const FLUX_KUSTOMIZATION_KEY_PRUNE: &str = "…"` at this
        // crate (the canonical drift footgun where a sibling local
        // `pub const` could happen to carry the same string at the
        // source while pointing at a different `&'static` allocation)
        // is a build-time test failure naming the offending drift. Peer
        // to
        // [`flux_helmrelease_key_create_namespace_re_export_points_at_caixa_core_canonical`]
        // on the sibling co-resident per-`HelmRelease`-CR install-path-
        // only per-CR namespace-seeder-toggle leaf-scalar-key re-export
        // axis.
        caixa_core::assert_str_reexport_identity(
            "FLUX_KUSTOMIZATION_KEY_PRUNE",
            FLUX_KUSTOMIZATION_KEY_PRUNE,
            caixa_core::FLUX_KUSTOMIZATION_KEY_PRUNE,
        );
    }

    #[test]
    fn cluster_bundle_kustomization_path_pins_lifted_sub_tree() {
        // Fail-before-pass-after pin: the rendered `kustomization.yaml`
        // document's `spec.path` per-CR source-sub-tree leaf-scalar-key
        // must resolve to the canonical per-cluster / per-caixa sub-
        // tree path seed verbatim under the lifted
        // [`FLUX_KUSTOMIZATION_KEY_PATH`] leaf-key. Before the lift the
        // axis carried an inline `path: ./clusters/<cluster>/services/<name>`
        // leaf-scalar-key literal at the sole production-code call
        // site (the top-level `spec` position of the [`cluster_bundle`]
        // `kustomization.yaml` format-string template, mirror-symmetric
        // to the co-resident per-`Kustomization`-CR garbage-collection-
        // toggle the sibling
        // [`cluster_bundle_kustomization_prune_pins_lifted_true`] pin
        // covers). A future substrate-side rebrand on the canonical
        // [`caixa_core::FLUX_KUSTOMIZATION_KEY_PATH`] declaration that
        // failed to reach this emit site would silently unbind every
        // emitted per-caixa `Kustomization` from its paired per-caixa
        // sub-tree of the pleme-io k8s repository — the Flux v2
        // kustomize-controller would then either reconcile the whole
        // GitRepository root (when the CR omits the leaf, the
        // controller defaults to `./`, pulling every unrelated cluster's
        // manifests through the wrong per-caixa `Kustomization`) or
        // refuse to reconcile at all (when the leaf points at a path
        // the GitRepository doesn't carry, the CR sits perpetually at
        // `BuildFailed`). Pin the identity here so a regression that
        // re-introduces an inline literal at the emit site — or a
        // rebrand on the canonical const that fails to reach the emit
        // site — surfaces at build time on this test's failure. Peer
        // to the sibling
        // [`cluster_bundle_kustomization_prune_pins_lifted_true`] pin
        // on the co-resident per-`Kustomization`-CR garbage-collection-
        // toggle axis — extends the per-`Kustomization`-CR-spec leaf-
        // scalar-key discipline from the co-resident garbage-
        // collection-toggle onto the co-resident source-sub-tree
        // pointer at the mirror-symmetric top-level `spec.path`
        // position.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let ks = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
            .expect("kustomization.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&ks.contents).expect("kustomization.yaml parses as YAML");
        let path = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_KUSTOMIZATION_KEY_PATH))
            .and_then(|v| v.as_str())
            .expect(
                "spec.path string scalar present; drift on this axis \
                 silently unbinds every per-caixa Kustomization from \
                 its paired per-caixa sub-tree of the pleme-io k8s \
                 repository, and the kustomize-controller either \
                 defaults to reconciling the GitRepository root or \
                 refuses to reconcile at all",
            );
        let expected = flux_kustomization_source_subtree(&opts.cluster, &sample_caixa().nome);
        assert_eq!(
            path, expected,
            "spec.path must carry the substrate's canonical per-cluster \
             / per-caixa sub-tree seed — drift silently unbinds every \
             per-caixa Kustomization from its paired sub-tree of the \
             pleme-io k8s repository"
        );
    }

    #[test]
    fn flux_kustomization_key_path_re_export_points_at_caixa_core_canonical() {
        // The renderer's `FLUX_KUSTOMIZATION_KEY_PATH` was lifted from
        // the production-code inline `path` literal at the sole
        // `cluster_bundle` `kustomization.yaml` format-string template's
        // per-CR source-sub-tree leaf-scalar-key emit site + the test-
        // fixture navigation site the sibling
        // [`cluster_bundle_kustomization_path_pins_lifted_sub_tree`] pin
        // opens onto the rendered document, to a re-export of
        // [`caixa_core::FLUX_KUSTOMIZATION_KEY_PATH`] so the canonical
        // Flux v2 per-CR source-sub-tree leaf-scalar-key lives in
        // exactly one place across every caixa renderer. Pin the
        // equality + static-data identity here so any local re-
        // introduction of a sibling
        // `pub const FLUX_KUSTOMIZATION_KEY_PATH: &str = "…"` at this
        // crate (the canonical drift footgun where a sibling local
        // `pub const` could happen to carry the same string at the
        // source while pointing at a different `&'static` allocation)
        // is a build-time test failure naming the offending drift.
        // Peer to
        // [`flux_kustomization_key_prune_re_export_points_at_caixa_core_canonical`]
        // on the sibling co-resident per-`Kustomization`-CR garbage-
        // collection-toggle leaf-scalar-key re-export axis.
        caixa_core::assert_str_reexport_identity(
            "FLUX_KUSTOMIZATION_KEY_PATH",
            FLUX_KUSTOMIZATION_KEY_PATH,
            caixa_core::FLUX_KUSTOMIZATION_KEY_PATH,
        );
    }

    #[test]
    fn flux_kustomization_source_subtree_re_export_matches_caixa_core_canonical_output() {
        // The renderer's `flux_kustomization_source_subtree` was lifted
        // from the verbatim inline
        // `format!("./clusters/{cluster}/services/{name}")` at the sole
        // `cluster_bundle` `kustomization.yaml` format-string production
        // emit site + a mirror-symmetric verbatim inline
        // `format!("./clusters/{cluster}/services/{name}", …)` at the
        // paired `cluster_bundle_kustomization_path_pins_lifted_sub_tree`
        // test-fixture navigation site, to a re-export of
        // [`caixa_core::flux_kustomization_source_subtree`]. Pin the
        // output-shape equality here on representative fixtures so any
        // local re-introduction of a sibling
        // `pub fn flux_kustomization_source_subtree(...)` shadow at this
        // crate (the canonical drift footgun where a sibling local
        // `pub fn` could happen to produce the same byte-shape at the
        // source while diverging on either axis of the composition) is
        // a build-time test failure naming the offending drift. Peer to
        // [`flux_kustomization_key_path_re_export_points_at_caixa_core_canonical`]
        // on the paired leaf-scalar-key half of the same `(key, value)`
        // per-CR `spec.path` pair — the key half's re-export identity
        // lives at the sibling `assert_str_reexport_identity` pin, the
        // value half's canonical composition lives here. Same shape as
        // the sibling composer re-export identity pins the
        // [`caixa_mesh::cilium_network_policy_name`] and
        // [`caixa_mesh::gateway_api_http_route_name`] re-exports carry
        // at their crate's `mod tests` — every composer re-export in
        // the substrate reaches the canonical `caixa-core` function
        // through one `pub use` and the test-side output-shape identity
        // pin closes the "sibling `pub fn` shadow" footgun by
        // construction.
        assert_eq!(
            flux_kustomization_source_subtree("rio", "hello-rio"),
            caixa_core::flux_kustomization_source_subtree("rio", "hello-rio"),
        );
        assert_eq!(
            flux_kustomization_source_subtree("rio", "hello-rio"),
            "./clusters/rio/services/hello-rio",
        );
        assert_eq!(
            flux_kustomization_source_subtree("paris", "cart"),
            caixa_core::flux_kustomization_source_subtree("paris", "cart"),
        );
    }

    #[test]
    fn cluster_bundle_kustomization_spec_path_uses_lifted_composer() {
        // Composition pin: the `spec.path` scalar emitted by
        // `cluster_bundle` for every per-caixa `kustomization.yaml`
        // document must byte-equal the output of the lifted
        // [`flux_kustomization_source_subtree`] composer with the same
        // `(cluster, nome)` arguments — so a future refactor of the
        // composer's internals (per-cluster prefix rebrand, per-caixa
        // infix rebrand, multi-tenant scoping prefix) reaches the
        // renderer through the one function-pointer edit at the
        // canonical `caixa-core` declaration, and any rewrite of the
        // format-string template's `{path_key}: {source_subtree}` emit
        // site that desynchronizes from the composer fires here at
        // build-time rather than silently splitting the writer-side
        // sub-tree seed at emit time. Peer to
        // [`cluster_bundle_kustomization_path_pins_lifted_sub_tree`]
        // above on the paired output-shape assertion — that pin locks
        // the emitted `spec.path` scalar against the composer's output;
        // this pin locks the equivalence to the composer with the
        // ClusterBundleOpts's `cluster` axis threaded through, so a
        // regression that pinned one axis at the emit site while
        // leaving the peer axis inline surfaces on either half.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let ks = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
            .expect("kustomization.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&ks.contents).expect("kustomization.yaml parses as YAML");
        let emitted = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_KUSTOMIZATION_KEY_PATH))
            .and_then(|v| v.as_str())
            .expect("spec.path string scalar present")
            .to_owned();
        let composed = flux_kustomization_source_subtree(&opts.cluster, &sample_caixa().nome);
        assert_eq!(
            emitted, composed,
            "spec.path emit must byte-equal the lifted \
             flux_kustomization_source_subtree composer's output — drift \
             at either half silently splits the per-cluster / per-caixa \
             sub-tree seed axis at emit time from the canonical composer"
        );
    }

    #[test]
    fn cluster_bundle_kustomization_timeout_pins_lifted_default() {
        // Fail-before-pass-after pin: the rendered `kustomization.yaml`
        // document's `spec.timeout` per-CR reconcile wall-clock cap
        // leaf-scalar-key must resolve to the canonical
        // [`DEFAULT_FLUX_KUSTOMIZATION_TIMEOUT`] scalar-value verbatim
        // under the lifted [`FLUX_KUSTOMIZATION_KEY_TIMEOUT`] leaf-key.
        // Before the lift the axis carried an inline `timeout: 5m` leaf-
        // scalar literal at the sole production-code call site (the
        // top-level `spec` position of the [`cluster_bundle`]
        // `kustomization.yaml` format-string template, sibling to the
        // co-resident per-`Kustomization`-CR source-sub-tree pointer
        // the peer [`cluster_bundle_kustomization_path_pins_lifted_sub_tree`]
        // pin covers). A future substrate-side rebrand on either half
        // of the canonical `(FLUX_KUSTOMIZATION_KEY_TIMEOUT,
        // DEFAULT_FLUX_KUSTOMIZATION_TIMEOUT)` pair that failed to
        // reach this emit site would silently strip the substrate's
        // chosen reconcile-ceiling declaration from every emitted per-
        // caixa `Kustomization` document — the Flux v2 kustomize-
        // controller would then fall back to the upstream Flux v2
        // controller-side default cap (which the upstream project
        // ships tuned for the average upstream Flux-managed manifest
        // set, not the substrate's per-caixa idempotency-checkpoint
        // cadence the peer
        // [`FLUX_HELMRELEASE_REMEDIATION_RETRIES_DEFAULT`] retry-
        // ceiling and [`DEFAULT_FLUX_RECONCILE_INTERVAL`] reconcile-
        // poll cadence are jointly tuned against), letting a
        // persistently-failing per-caixa manifest apply consume
        // kustomize-controller reconcile-loop cycles past the
        // substrate's chosen ceiling with no field naming the timeout-
        // drift root cause. Pin the identity here so a regression that
        // re-introduces an inline literal at the emit site — or a
        // rebrand on either canonical const that fails to reach the
        // emit site — surfaces at build time on this test's failure.
        // Peer to the sibling
        // [`cluster_bundle_kustomization_path_pins_lifted_sub_tree`]
        // and [`cluster_bundle_kustomization_prune_pins_lifted_true`]
        // pins on the co-resident per-`Kustomization`-CR spec surface
        // axes — extends the per-`Kustomization`-CR-spec leaf-scalar-
        // key/scalar-value discipline from the co-resident source-
        // sub-tree pointer and garbage-collection-toggle onto the co-
        // resident reconcile wall-clock cap at the mirror-symmetric
        // top-level `spec.timeout` position.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let ks = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
            .expect("kustomization.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&ks.contents).expect("kustomization.yaml parses as YAML");
        let timeout = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_KUSTOMIZATION_KEY_TIMEOUT))
            .and_then(|v| v.as_str())
            .expect(
                "spec.timeout string scalar present; drift on this axis \
                 silently strips the substrate's chosen reconcile-ceiling \
                 declaration from every emitted per-caixa `Kustomization` \
                 document, letting the kustomize-controller fall back to \
                 the upstream Flux v2 controller-side default cap",
            );
        assert_eq!(
            timeout, DEFAULT_FLUX_KUSTOMIZATION_TIMEOUT,
            "spec.timeout must carry the substrate's canonical Flux v2 \
             per-`Kustomization`-CR reconcile wall-clock cap seed — drift \
             silently strips the substrate's chosen reconcile-ceiling \
             declaration from every emitted per-caixa `Kustomization` \
             document"
        );
    }

    #[test]
    fn flux_kustomization_key_timeout_re_export_points_at_caixa_core_canonical() {
        // The renderer's `FLUX_KUSTOMIZATION_KEY_TIMEOUT` was lifted
        // from the production-code inline `timeout` literal at the
        // sole `cluster_bundle` `kustomization.yaml` format-string
        // template's per-CR reconcile wall-clock cap leaf-scalar-key
        // emit site + the test-fixture navigation site the sibling
        // [`cluster_bundle_kustomization_timeout_pins_lifted_default`]
        // pin opens onto the rendered document, to a re-export of
        // [`caixa_core::FLUX_KUSTOMIZATION_KEY_TIMEOUT`] so the
        // canonical Flux v2 per-CR reconcile wall-clock cap leaf-
        // scalar-key lives in exactly one place across every caixa
        // renderer. Pin the equality + static-data identity here so
        // any local re-introduction of a sibling
        // `pub const FLUX_KUSTOMIZATION_KEY_TIMEOUT: &str = "…"` at
        // this crate (the canonical drift footgun where a sibling
        // local `pub const` could happen to carry the same string at
        // the source while pointing at a different `&'static`
        // allocation) is a build-time test failure naming the
        // offending drift. Peer to
        // [`flux_kustomization_key_path_re_export_points_at_caixa_core_canonical`]
        // and
        // [`flux_kustomization_key_prune_re_export_points_at_caixa_core_canonical`]
        // on the sibling co-resident per-`Kustomization`-CR spec
        // surface re-export axes.
        caixa_core::assert_str_reexport_identity(
            "FLUX_KUSTOMIZATION_KEY_TIMEOUT",
            FLUX_KUSTOMIZATION_KEY_TIMEOUT,
            caixa_core::FLUX_KUSTOMIZATION_KEY_TIMEOUT,
        );
    }

    #[test]
    fn default_flux_kustomization_timeout_re_export_points_at_caixa_core_canonical() {
        // The renderer's `DEFAULT_FLUX_KUSTOMIZATION_TIMEOUT` was
        // lifted from the production-code inline `5m` scalar-value
        // literal at the sole `cluster_bundle` `kustomization.yaml`
        // format-string template's per-CR reconcile wall-clock cap
        // emit site, to a re-export of
        // [`caixa_core::DEFAULT_FLUX_KUSTOMIZATION_TIMEOUT`] so the
        // canonical Flux v2 per-CR reconcile wall-clock cap default
        // scalar-value lives in exactly one place across every caixa
        // renderer. Pin the equality + static-data identity here so
        // any local re-introduction of a sibling
        // `pub const DEFAULT_FLUX_KUSTOMIZATION_TIMEOUT: &str = "…"`
        // at this crate (the canonical drift footgun where a sibling
        // local `pub const` could happen to carry the same string at
        // the source while pointing at a different `&'static`
        // allocation) is a build-time test failure naming the
        // offending drift. Peer to
        // [`default_flux_reconcile_interval_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-Flux-v2-per-CR-substrate-default-
        // scalar re-export axis.
        caixa_core::assert_str_reexport_identity(
            "DEFAULT_FLUX_KUSTOMIZATION_TIMEOUT",
            DEFAULT_FLUX_KUSTOMIZATION_TIMEOUT,
            caixa_core::DEFAULT_FLUX_KUSTOMIZATION_TIMEOUT,
        );
    }

    #[test]
    fn kube_key_spec_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_SPEC` was lifted from the
        // production-code inline `"spec"` literals at the two K8s-CR
        // top-level-spec-axis call sites (`programs_yaml_entry`'s
        // `computeunit_yaml.get("spec")` ComputeUnit-side spec read +
        // its matching `Error::MissingField("spec")` diagnostic;
        // `upsert_into_helmrelease_programs`'s `root.get_mut("spec")`
        // HelmRelease-side spec mutate + its matching
        // `Error::MissingField("spec")` diagnostic) to a re-export of
        // [`caixa_core::KUBE_KEY_SPEC`] so the canonical K8s-CR
        // top-level spec-axis string lives in exactly one place across
        // every caixa renderer. Pin the equality + static-data
        // identity here so any local re-introduction of a sibling
        // `pub const KUBE_KEY_SPEC: &str = "…"` (the canonical drift
        // footgun where a sibling local `pub const` could happen to
        // carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test
        // failure naming the offending drift. Peer to
        // [`default_namespace_re_export_points_at_caixa_core_canonical`]
        // on the sibling re-export axis +
        // `caixa_mesh::tests::kube_key_spec_re_export_points_at_caixa_core_canonical`
        // on the sibling renderer crate.
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_SPEC",
            KUBE_KEY_SPEC,
            caixa_core::KUBE_KEY_SPEC,
        );
    }

    #[test]
    fn kube_key_metadata_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_METADATA` was lifted from the
        // production-code inline `"metadata"` literal at
        // `programs_yaml_entry`'s `computeunit_yaml.get("metadata")`
        // ComputeUnit-side metadata read + the drift-detection pin at
        // `cluster_bundle_kustomization_carries_flux_system_namespace_axes`'s
        // `parsed.get("metadata")` rendered-kustomization traversal, to a
        // re-export of [`caixa_core::KUBE_KEY_METADATA`] so the canonical
        // K8s-CR top-level metadata-axis string lives in exactly one
        // place across every caixa renderer. Pin the equality +
        // static-data identity here so any local re-introduction of a
        // sibling `pub const KUBE_KEY_METADATA: &str = "…"` (the
        // canonical drift footgun where a sibling local `pub const`
        // could happen to carry the same string at the source while
        // pointing at a different `&'static` allocation) is a build-time
        // test failure naming the offending drift. Peer to
        // [`kube_key_spec_re_export_points_at_caixa_core_canonical`] on
        // the sibling K8s-CR top-level-spec-axis re-export +
        // `caixa_mesh::tests::kube_key_metadata_re_export_points_at_caixa_core_canonical`
        // on the sibling renderer crate.
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_METADATA",
            KUBE_KEY_METADATA,
            caixa_core::KUBE_KEY_METADATA,
        );
    }

    #[test]
    fn kube_key_kind_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_KIND` was lifted from eleven inline
        // `"kind"` literals at the test-side K8s-CR top-level-kind-axis
        // retrieval calls that navigate the rendered `cluster_bundle`
        // multi-file sequence to isolate each per-`(GitRepository,
        // HelmRelease, Kustomization)` document's top-level-kind
        // discriminator plus its nested `spec.chart.spec.sourceRef.kind`
        // (helmrelease) / `spec.sourceRef.kind` (kustomization) /
        // `spec.healthChecks[].kind` (kustomization) drift-detection
        // pins, to a re-export of [`caixa_core::KUBE_KEY_KIND`] so the
        // canonical K8s-CR top-level kind-discriminator axis string
        // lives in exactly one place across every caixa renderer. Pin
        // the equality + static-data identity here so any local
        // re-introduction of a sibling `pub const KUBE_KEY_KIND: &str
        // = "…"` (the canonical drift footgun where a sibling local
        // `pub const` could happen to carry the same string at the
        // source while pointing at a different `&'static` allocation)
        // is a build-time test failure naming the offending drift, not
        // a silent apply-time symptom — the prior shape would have let
        // a typo on any one sibling `pub const` declaration silently
        // miss the per-CR kind retrieval so the drift-detection
        // `.get(KUBE_KEY_KIND).and_then(|n| n.as_str()) == Some(…)`
        // predicate the sibling `FLUX_KIND_*` re-export pins rest on
        // would compare against `None` under the trailing
        // `.expect("… present")` panic and mask the true sibling
        // `FLUX_KIND_*` axis drift. Peer to
        // [`kube_key_spec_re_export_points_at_caixa_core_canonical`] +
        // [`kube_key_metadata_re_export_points_at_caixa_core_canonical`]
        // on the sibling K8s-CR top-level-spec / top-level-metadata
        // axis re-exports + `caixa_mesh::tests::kube_key_kind_re_export_points_at_caixa_core_canonical`
        // (615a13d) on the sibling renderer crate — completes the
        // per-K8s-CR top-level `(spec, metadata, kind)` axis re-export
        // triple every rendered Flux bundle document navigates.
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_KIND",
            KUBE_KEY_KIND,
            caixa_core::KUBE_KEY_KIND,
        );
    }

    #[test]
    fn kube_key_api_version_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_API_VERSION` was lifted from four
        // inline `"apiVersion"` literals at the test-side K8s-CR
        // top-level-apiVersion-axis retrieval calls that navigate the
        // rendered `cluster_bundle` multi-file sequence to isolate each
        // per-`(GitRepository, HelmRelease, Kustomization)` document's
        // top-level-apiVersion axis plus the `kustomization.yaml`'s
        // nested `spec.healthChecks[].apiVersion` drift-detection pin,
        // to a re-export of [`caixa_core::KUBE_KEY_API_VERSION`] so the
        // canonical K8s-CR top-level apiVersion-axis string lives in
        // exactly one place across every caixa renderer. Pin the
        // equality + static-data identity here so any local
        // re-introduction of a sibling `pub const KUBE_KEY_API_VERSION:
        // &str = "…"` (the canonical drift footgun where a sibling
        // local `pub const` could happen to carry the same string at
        // the source while pointing at a different `&'static`
        // allocation) is a build-time test failure naming the offending
        // drift, not a silent apply-time symptom — the prior shape
        // would have let a typo on any one sibling `pub const`
        // declaration silently miss the per-CR apiVersion retrieval so
        // the drift-detection `.get(KUBE_KEY_API_VERSION).and_then(|n|
        // n.as_str()) == Some(…)` predicate the sibling
        // `FLUX_*_API_VERSION` re-export pins rest on would compare
        // against `None` under the trailing `.expect("… present")`
        // panic and mask the true sibling `FLUX_*_API_VERSION` axis
        // drift. Peer to
        // [`kube_key_spec_re_export_points_at_caixa_core_canonical`] +
        // [`kube_key_metadata_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_kind_re_export_points_at_caixa_core_canonical`]
        // on the sibling K8s-CR top-level-spec / top-level-metadata /
        // top-level-kind axis re-exports — completes the per-K8s-CR
        // top-level `(apiVersion, kind, metadata, spec)` axis
        // re-export quartet every rendered Flux v2 bundle document
        // navigates.
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_API_VERSION",
            KUBE_KEY_API_VERSION,
            caixa_core::KUBE_KEY_API_VERSION,
        );
    }

    #[test]
    fn kube_key_namespace_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_NAMESPACE` was lifted from five inline
        // `"namespace"` literals — the two production-code call sites in
        // `programs_yaml_entry` (the ComputeUnit YAML's
        // `metadata.namespace` retrieval that feeds the emitted
        // `programs:[]` entry's isomorphic `namespace:` field, and the
        // write-side entry-key emission the `lareira-fleet-programs`
        // schema keys the per-Servico namespace off) plus the three
        // test-side drift-detection call sites (the two
        // `programs_yaml_entry` round-trip pins asserting the emitted
        // entry's `namespace:` field spells `tatara-system` on the
        // metadata-carried path and [`DEFAULT_NAMESPACE`] on the
        // fallback path, and the `cluster_bundle_kustomization_carries_\
        // flux_system_namespace_axes` pin asserting the rendered
        // `kustomization.yaml` document's `metadata.namespace` axis
        // binds to the lifted [`DEFAULT_FLUX_SYSTEM_NAMESPACE`]) — to
        // a re-export of [`caixa_core::KUBE_KEY_NAMESPACE`] so the
        // canonical K8s-CR `metadata.namespace` axis string lives in
        // exactly one place across every caixa renderer. Pin the
        // equality + static-data identity here so any local
        // re-introduction of a sibling `pub const KUBE_KEY_NAMESPACE:
        // &str = "…"` (the canonical drift footgun where a sibling
        // local `pub const` could happen to carry the same string at
        // the source while pointing at a different `&'static`
        // allocation) is a build-time test failure naming the
        // offending drift, not a silent apply-time symptom — the prior
        // shape would have let a typo on any one sibling `pub const`
        // declaration silently miss the ComputeUnit's
        // `metadata.namespace` lookup and fall back to
        // [`DEFAULT_NAMESPACE`] even when the ComputeUnit YAML pinned
        // a distinct target namespace, or mask the sibling
        // [`DEFAULT_FLUX_SYSTEM_NAMESPACE`] drift the
        // `kustomization.yaml`-side pin was meant to catch under the
        // `.expect("kustomization.yaml present")` panic on the missing
        // `.and_then` chain. Peer to
        // [`kube_key_spec_re_export_points_at_caixa_core_canonical`] /
        // [`kube_key_metadata_re_export_points_at_caixa_core_canonical`]
        // / [`kube_key_kind_re_export_points_at_caixa_core_canonical`]
        // / [`kube_key_api_version_re_export_points_at_caixa_core_canonical`]
        // on the sibling K8s-CR top-level `(spec, metadata, kind,
        // apiVersion)` axis re-exports — extends the discipline the
        // top-level quartet establishes onto the canonical
        // `metadata.namespace` nested axis every rendered Flux v2
        // bundle document navigates.
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_NAMESPACE",
            KUBE_KEY_NAMESPACE,
            caixa_core::KUBE_KEY_NAMESPACE,
        );
    }

    #[test]
    fn fleet_programs_key_programs_re_export_points_at_caixa_core_canonical() {
        // The renderer's `FLEET_PROGRAMS_KEY_PROGRAMS` was lifted from
        // the two inline `"programs"` production-code call sites
        // (`upsert_into_helmrelease_programs` at
        // `values_map.entry(Value::String("programs".into()))` on the
        // aggregator-HelmRelease shape, `upsert_into_programs_yaml` at
        // `let programs_key = Value::String("programs".into())` on the
        // bare-values.yaml shape) — the two writer-side upsert paths
        // that walk `HelmRelease.spec.values.programs[]` /
        // top-level `programs[]` to match-by-name-and-replace-or-append.
        // Both now navigate through the same `&'static str` as every
        // peer consumer, re-exported to a re-export of
        // [`caixa_core::FLEET_PROGRAMS_KEY_PROGRAMS`] so the canonical
        // `lareira-fleet-programs` values-schema array key lives in
        // exactly one place across every caixa renderer + consumer.
        // Pin the equality + static-data identity here so any local
        // re-introduction of a sibling `pub const FLEET_PROGRAMS_KEY_PROGRAMS:
        // &str = "…"` (the canonical drift footgun where a sibling
        // local `pub const` could happen to carry the same string at
        // the source while pointing at a different `&'static`
        // allocation) is a build-time test failure naming the offending
        // drift, not a silent apply-time symptom — the prior shape
        // would have let a typo on any one sibling `pub const`
        // declaration silently emit an entry under one key while the
        // peer-side upsert probed a different key, and the aggregator's
        // `range .Values.programs` would then iterate an empty sequence
        // (every `ComputeUnit` CR silently vanishing from the cluster's
        // fleet). Peer to
        // [`kube_key_namespace_re_export_points_at_caixa_core_canonical`]
        // /
        // [`kube_key_spec_re_export_points_at_caixa_core_canonical`] /
        // [`kube_key_metadata_re_export_points_at_caixa_core_canonical`]
        // /
        // [`kube_key_kind_re_export_points_at_caixa_core_canonical`] /
        // [`kube_key_api_version_re_export_points_at_caixa_core_canonical`]
        // on the sibling K8s-CR top-level axis re-exports — extends
        // the discipline the K8s-CR key re-export quintet establishes
        // onto the canonical fleet-programs schema top-level axis
        // (`programs:`) every rendered aggregator-HelmRelease /
        // bare-values.yaml document navigates.
        caixa_core::assert_str_reexport_identity(
            "FLEET_PROGRAMS_KEY_PROGRAMS",
            FLEET_PROGRAMS_KEY_PROGRAMS,
            caixa_core::FLEET_PROGRAMS_KEY_PROGRAMS,
        );
    }

    #[test]
    fn fleet_programs_key_name_re_export_points_at_caixa_core_canonical() {
        // The renderer's `FLEET_PROGRAMS_KEY_NAME` was lifted from the
        // three inline `"name"` production-code call sites in
        // [`programs_yaml_entry`] (emit-side per-Servico entry.insert
        // seeded from `caixa.nome`), [`upsert_into_helmrelease_programs`]
        // (writer-side `new_entry.get("name")` / `slot.get("name")` +
        // `Error::MissingField("name")` triplet on the aggregator-
        // HelmRelease shape), and [`upsert_into_programs_yaml`] (writer-
        // side peer triplet on the bare-values.yaml shape) — every
        // fleet-programs per-entry name-axis read + write + missing-
        // field diagnostic across the two writer-side upsert paths and
        // the one emit-side entry builder now navigates through the
        // same `&'static str` re-exported to a re-export of
        // [`caixa_core::FLEET_PROGRAMS_KEY_NAME`] so the canonical
        // `lareira-fleet-programs` values-schema per-entry name-
        // discriminator key lives in exactly one place across every
        // caixa renderer + consumer. Pin the equality + static-data
        // identity here so any local re-introduction of a sibling
        // `pub const FLEET_PROGRAMS_KEY_NAME: &str = "…"` (the canonical
        // drift footgun where a sibling local `pub const` could happen
        // to carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test failure
        // naming the offending drift, not a silent apply-time symptom —
        // the prior shape would have let a typo on any one sibling
        // `pub const` declaration silently emit an entry under one key
        // while the peer-side upsert probed a different key, and the
        // aggregator's `range .Values.programs` would then iterate
        // entries whose per-entry name-axis the library chart's
        // `metadata.name` templating reads as empty (or match against
        // the wrong entry on upsert), collapsing every rendered
        // `ComputeUnit` CR at the aggregator's name-keyed reduce step.
        // Peer to
        // [`fleet_programs_key_programs_re_export_points_at_caixa_core_canonical`]
        // on the sibling fleet-programs top-level array-key re-export
        // + [`kube_key_namespace_re_export_points_at_caixa_core_canonical`]
        // / [`kube_key_spec_re_export_points_at_caixa_core_canonical`]
        // on the peer K8s-CR canonical-key re-export surfaces —
        // extends the discipline the K8s-CR key re-export quintet +
        // the sibling fleet-programs top-level array-key re-export
        // establish onto the canonical fleet-programs schema per-entry
        // name-discriminator axis.
        caixa_core::assert_str_reexport_identity(
            "FLEET_PROGRAMS_KEY_NAME",
            FLEET_PROGRAMS_KEY_NAME,
            caixa_core::FLEET_PROGRAMS_KEY_NAME,
        );
    }

    #[test]
    fn computeunit_spec_key_module_re_export_points_at_caixa_core_canonical() {
        // The renderer's `COMPUTEUNIT_SPEC_KEY_MODULE` was lifted from
        // the four inline `"module"` test-side call sites in this
        // crate (`programs_yaml_entry_round_trips`'s per-entry
        // `entry.get("module")` present-check + its per-
        // `module.source` nested navigator, plus
        // `upsert_helmrelease_replaces_existing` /
        // `upsert_into_programs_yaml`'s per-`module.source` cross-
        // upsert readback navigators) — every per-Servico ComputeUnit
        // CRD `spec.module` sub-block readback across the four
        // drift-detection navigators now navigates through the same
        // `&'static str` re-exported to a re-export of
        // [`caixa_core::COMPUTEUNIT_SPEC_KEY_MODULE`]. Pin the
        // equality + static-data identity here so any local re-
        // introduction of a sibling `pub const COMPUTEUNIT_SPEC_KEY_MODULE:
        // &str = "…"` is a build-time test failure naming the
        // offending drift, not a silent apply-time symptom. Peer to
        // [`fleet_programs_key_programs_re_export_points_at_caixa_core_canonical`]
        // /
        // [`kube_key_spec_re_export_points_at_caixa_core_canonical`]
        // on the sibling fleet-programs / K8s-CR-key re-export
        // surfaces — extends the discipline the K8s-CR / fleet-
        // programs key re-export families establish onto the
        // substrate-side ComputeUnit-CRD per-`spec.*` sub-block axis.
        caixa_core::assert_str_reexport_identity(
            "COMPUTEUNIT_SPEC_KEY_MODULE",
            COMPUTEUNIT_SPEC_KEY_MODULE,
            caixa_core::COMPUTEUNIT_SPEC_KEY_MODULE,
        );
    }

    #[test]
    fn computeunit_spec_key_trigger_re_export_points_at_caixa_core_canonical() {
        // Peer to
        // [`computeunit_spec_key_module_re_export_points_at_caixa_core_canonical`]
        // on the same ComputeUnit-CRD per-`spec.*` sub-block re-export
        // surface — pins the per-Servico invocation-shape sub-block
        // key's identity on the same trajectory.
        caixa_core::assert_str_reexport_identity(
            "COMPUTEUNIT_SPEC_KEY_TRIGGER",
            COMPUTEUNIT_SPEC_KEY_TRIGGER,
            caixa_core::COMPUTEUNIT_SPEC_KEY_TRIGGER,
        );
    }

    #[test]
    fn computeunit_spec_key_capabilities_re_export_points_at_caixa_core_canonical() {
        // Peer to
        // [`computeunit_spec_key_module_re_export_points_at_caixa_core_canonical`]
        // and
        // [`computeunit_spec_key_trigger_re_export_points_at_caixa_core_canonical`]
        // on the same ComputeUnit-CRD per-`spec.*` sub-block re-export
        // surface — completes the substrate-side ComputeUnit-CRD
        // per-`spec.*` sub-block re-export triple in this crate on the
        // WASI-capability-token-list axis.
        caixa_core::assert_str_reexport_identity(
            "COMPUTEUNIT_SPEC_KEY_CAPABILITIES",
            COMPUTEUNIT_SPEC_KEY_CAPABILITIES,
            caixa_core::COMPUTEUNIT_SPEC_KEY_CAPABILITIES,
        );
    }

    #[test]
    fn computeunit_module_key_source_re_export_points_at_caixa_core_canonical() {
        // The renderer's `COMPUTEUNIT_MODULE_KEY_SOURCE` was lifted
        // from the three inline `"source"` test-side call sites in
        // this crate — `programs_yaml_entry_round_trips`'s per-entry
        // `.get(COMPUTEUNIT_SPEC_KEY_MODULE).and_then(|m| m.get("source"))`
        // present-check + `upsert_replaces_existing_entry`'s per-
        // `arr[0].get(COMPUTEUNIT_SPEC_KEY_MODULE).get("source")`
        // cross-upsert readback + `upsert_helmrelease_inserts_under_spec_values_programs`'s
        // peer `HelmRelease`-wrapped `spec.values.programs[]` cross-
        // upsert readback. Every per-Servico ComputeUnit-CRD
        // `spec.module.source` leaf-scalar readback across the three
        // drift-detection navigators now navigates through the same
        // `&'static str` re-exported to a re-export of
        // [`caixa_core::COMPUTEUNIT_MODULE_KEY_SOURCE`]. Pin the
        // equality + static-data identity here so any local re-
        // introduction of a sibling `pub const COMPUTEUNIT_MODULE_KEY_SOURCE:
        // &str = "…"` is a build-time test failure naming the
        // offending drift, not a silent apply-time symptom. Peer to
        // [`computeunit_spec_key_module_re_export_points_at_caixa_core_canonical`]
        // on the same ComputeUnit-CRD family — extends the re-export-
        // identity gate one level deeper from the top-level `spec.*`
        // container-axis surface onto the nested `spec.module.*`
        // leaf-scalar-axis.
        caixa_core::assert_str_reexport_identity(
            "COMPUTEUNIT_MODULE_KEY_SOURCE",
            COMPUTEUNIT_MODULE_KEY_SOURCE,
            caixa_core::COMPUTEUNIT_MODULE_KEY_SOURCE,
        );
    }

    #[test]
    fn programs_yaml_entry_round_trips() {
        let entry = programs_yaml_entry(&sample_caixa(), &sample_cu_yaml()).unwrap();
        assert_eq!(
            entry.get(FLEET_PROGRAMS_KEY_NAME).and_then(|n| n.as_str()),
            Some("hello-rio")
        );
        assert_eq!(
            entry.get(KUBE_KEY_NAMESPACE).and_then(|n| n.as_str()),
            Some(DEFAULT_NAMESPACE)
        );
        assert!(entry.get(COMPUTEUNIT_SPEC_KEY_MODULE).is_some());
        assert!(entry.get(COMPUTEUNIT_SPEC_KEY_TRIGGER).is_some());
        assert!(entry.get(COMPUTEUNIT_SPEC_KEY_CAPABILITIES).is_some());
        assert!(
            entry
                .get(COMPUTEUNIT_SPEC_KEY_MODULE)
                .and_then(|m| m.get(COMPUTEUNIT_MODULE_KEY_SOURCE))
                .is_some(),
            "module.source must propagate verbatim"
        );
    }

    #[test]
    fn programs_yaml_entry_falls_back_to_default_namespace() {
        // A computeunit without metadata.namespace should default.
        let cu: serde_yaml::Value = serde_yaml::from_str(
            r#"
apiVersion: wasm.pleme.io/v1alpha1
kind: ComputeUnit
metadata:
  name: hello-rio
spec:
  module:
    source: oci://ghcr.io/pleme-io/hello-rio:v0.1.0
"#,
        )
        .unwrap();
        let entry = programs_yaml_entry(&sample_caixa(), &cu).unwrap();
        assert_eq!(
            entry.get(KUBE_KEY_NAMESPACE).and_then(|n| n.as_str()),
            Some(DEFAULT_NAMESPACE)
        );
    }

    #[test]
    fn programs_yaml_entry_refuses_non_servico() {
        let mut c = sample_caixa();
        c.kind = CaixaKind::Biblioteca;
        c.servicos = vec![];
        let err = programs_yaml_entry(&c, &sample_cu_yaml()).unwrap_err();
        assert!(matches!(err, Error::NotAServico(_)));
    }

    #[test]
    fn kind_mismatch_error_names_offending_caixa_nome() {
        // Pinning the lifted [`caixa_core::KindMismatch`] view's
        // load-bearing property: a kind-mismatched caixa surfaces a
        // diagnostic that *names the offending caixa* (`hello-rio`),
        // not just the rejected kind. Before the lift the renderer
        // raised `Error::NotAServico(CaixaKind::Biblioteca)` whose
        // Display said "caixa :kind must be Servico for caixa-flux
        // rendering, got Biblioteca" — the user had to grep their
        // source tree for which caixa.lisp triggered it. After the
        // lift the wrapped KindMismatch carries the `:nome`, the
        // renderer's `#[error("{0}")]` arm prints it through, and
        // the diagnostic is self-locating.
        let mut c = sample_caixa();
        c.kind = CaixaKind::Biblioteca;
        c.servicos = vec![];
        let err = programs_yaml_entry(&c, &sample_cu_yaml()).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("hello-rio"),
            "kind-mismatch diagnostic must name the offending caixa nome \
             (got: {msg:?})"
        );
        assert!(
            msg.contains("Servico"),
            "diagnostic must name the expected kind (got: {msg:?})"
        );
        assert!(
            msg.contains("Biblioteca"),
            "diagnostic must name the actual kind (got: {msg:?})"
        );
    }

    #[test]
    fn cluster_bundle_kind_mismatch_names_offending_caixa_nome() {
        // The second kind-checking call site in caixa-flux —
        // [`cluster_bundle`] — must surface the same lifted diagnostic
        // shape. Pinning so a future divergence between
        // `programs_yaml_entry` and `cluster_bundle` (e.g. one
        // re-inlines the kind check, the other uses `require_kind`)
        // surfaces here as a test failure rather than as a silent
        // diagnostic regression on the deploy path.
        let mut c = sample_caixa();
        c.kind = CaixaKind::Aplicacao;
        c.servicos = vec![];
        let opts = ClusterBundleOpts::for_caixa(&c, "rio");
        let err = cluster_bundle(&c, &opts).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("hello-rio"),
            "cluster_bundle's kind-mismatch must also name the caixa \
             nome (got: {msg:?})"
        );
        match err {
            Error::NotAServico(km) => {
                assert_eq!(km.nome, "hello-rio");
                assert_eq!(km.expected, CaixaKind::Servico);
                assert_eq!(km.actual, CaixaKind::Aplicacao);
            }
            other => panic!("expected Error::NotAServico, got {other:?}"),
        }
    }

    #[test]
    fn servico_count_mismatch_carries_typed_view_with_nome() {
        // Peer to the [`KindMismatch`]-lift pin above on the V0
        // `:servicos`-singularity axis: a Servico-kind caixa whose
        // `:servicos` list is non-singleton fails
        // [`programs_yaml_entry`] with the renderer's
        // `Error::UnsupportedServicoCount` variant wrapping the typed
        // [`caixa_core::ServicoCountMismatch`] view (carrying the
        // offending caixa's `:nome` + the actual count). Before the
        // lift the variant carried only `usize` — the user had to grep
        // their source tree for which `caixa.lisp` triggered it; after
        // the lift the wrapped typed view names the offending caixa
        // verbatim. Pins both the variant routing (via `#[from]`) and
        // the typed payload so a future refactor can't silently switch
        // back to the raw-`usize` payload (which would regress the
        // shared-shape contract with caixa-helm on the peer
        // `lareira-<nome>` chart-dir path).
        let mut c = sample_caixa();
        c.servicos = vec![
            "servicos/hello-rio.computeunit.yaml".into(),
            "servicos/extra.computeunit.yaml".into(),
        ];
        let err = programs_yaml_entry(&c, &sample_cu_yaml()).unwrap_err();
        match err {
            Error::UnsupportedServicoCount(scm) => {
                assert_eq!(scm.nome, "hello-rio");
                assert_eq!(scm.count, 2);
            }
            other => panic!("expected Error::UnsupportedServicoCount, got {other:?}"),
        }
    }

    #[test]
    fn servico_count_mismatch_diagnostic_names_offending_caixa_nome() {
        // The renderer's `#[error("{0}")] UnsupportedServicoCount(
        // #[from] ServicoCountMismatch)` arm prints the typed view's
        // Display through verbatim, so the offending caixa's `:nome`
        // appears in the rendered diagnostic on both the
        // `programs_yaml_entry` and `cluster_bundle` paths. Pinning the
        // self-locating property end-to-end so a future refactor that
        // re-wraps the variant in a Display impl that drops the
        // `:nome` surfaces here as a test failure rather than as
        // silent fragmentation across the two flux deploy paths. Peer
        // to `cluster_bundle_kind_mismatch_names_offending_caixa_nome`
        // on the sibling V0 Servico-shape axis.
        let mut c = sample_caixa();
        c.servicos = vec![];
        let err = programs_yaml_entry(&c, &sample_cu_yaml()).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("hello-rio"),
            ":servicos-count-mismatch diagnostic must name the offending caixa nome \
             (got: {msg:?})"
        );
        assert!(
            msg.contains("0"),
            "diagnostic must name the actual count (got: {msg:?})"
        );
        assert!(
            msg.contains(":servicos"),
            "diagnostic must name the offending field axis (got: {msg:?})"
        );
    }

    #[test]
    fn cluster_bundle_servico_count_mismatch_carries_typed_view_with_nome() {
        // Peer to `cluster_bundle_kind_mismatch_names_offending_caixa_nome`
        // on the sibling V0 `:servicos`-singularity axis. The second
        // per-Servico renderer entry-point in caixa-flux —
        // [`cluster_bundle`] — must surface the same lifted
        // [`caixa_core::ServicoCountMismatch`] view the peer
        // [`programs_yaml_entry`] path already pins on the sibling V0
        // gate axis. Until this gate landed `cluster_bundle` ran only
        // the [`require_kind`] half of the V0-shape gate pair, so a
        // Servico-kind caixa whose `:servicos` list is non-singleton
        // silently passed the bundle render and the failure surfaced at
        // the chart-render layer (`caixa-helm` refused the same input
        // with `UnsupportedServicoCount`) far from the deploy-path
        // entry-point — the canonical "the V0 invariant is enforced at
        // every per-Servico renderer entry except this one" footgun.
        // Pins both the variant routing (via `#[from]`) and the typed
        // payload so a future refactor that re-inlines a raw count
        // check, or strips the `require_single_servico` call from this
        // path, surfaces here as a test failure rather than as silent
        // fragmentation across the two flux deploy paths.
        let mut c = sample_caixa();
        c.servicos = vec![
            "servicos/hello-rio.computeunit.yaml".into(),
            "servicos/extra.computeunit.yaml".into(),
        ];
        let opts = ClusterBundleOpts::for_caixa(&c, "rio");
        let err = cluster_bundle(&c, &opts).unwrap_err();
        match err {
            Error::UnsupportedServicoCount(scm) => {
                assert_eq!(scm.nome, "hello-rio");
                assert_eq!(scm.count, 2);
            }
            other => panic!("expected Error::UnsupportedServicoCount, got {other:?}"),
        }
    }

    #[test]
    fn cluster_bundle_servico_count_mismatch_diagnostic_names_offending_caixa_nome() {
        // End-to-end Display pin on the [`cluster_bundle`] path —
        // peer of `servico_count_mismatch_diagnostic_names_offending_caixa_nome`
        // on the sibling [`programs_yaml_entry`] path. The rendered
        // diagnostic must name the offending caixa's `:nome`, the
        // actual count, and the `:servicos` field axis verbatim on
        // both per-Servico renderer entry-points in caixa-flux. Peer
        // to `cluster_bundle_kind_mismatch_names_offending_caixa_nome`
        // on the sibling V0 kind-shape axis — both V0 gate arms now
        // pin their self-locating diagnostic shape end-to-end through
        // the bundle path.
        let mut c = sample_caixa();
        c.servicos = vec![];
        let opts = ClusterBundleOpts::for_caixa(&c, "rio");
        let err = cluster_bundle(&c, &opts).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("hello-rio"),
            "cluster_bundle's :servicos-count-mismatch must name the offending caixa nome \
             (got: {msg:?})"
        );
        assert!(
            msg.contains("0"),
            "diagnostic must name the actual count (got: {msg:?})"
        );
        assert!(
            msg.contains(":servicos"),
            "diagnostic must name the offending field axis (got: {msg:?})"
        );
    }

    #[test]
    fn upsert_inserts_new_entry() {
        let initial: serde_yaml::Value = serde_yaml::from_str(
            r#"
enabled: true
defaultNamespace: tatara-system
programs: []
"#,
        )
        .unwrap();
        let entry = programs_yaml_entry(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let (modified, inserted) = upsert_into_programs_yaml(initial, entry).unwrap();
        assert!(inserted, "first time should be insert");
        let arr = modified
            .get(FLEET_PROGRAMS_KEY_PROGRAMS)
            .unwrap()
            .as_sequence()
            .unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(
            arr[0].get(FLEET_PROGRAMS_KEY_NAME).and_then(|n| n.as_str()),
            Some("hello-rio")
        );
    }

    #[test]
    fn upsert_replaces_existing_entry() {
        let initial: serde_yaml::Value = serde_yaml::from_str(
            r#"
enabled: true
defaultNamespace: tatara-system
programs:
  - name: hello-rio
    namespace: tatara-system
    module:
      source: oci://ghcr.io/pleme-io/hello-rio:v0.0.1
  - name: other
    namespace: tatara-system
    module: { source: github:foo/bar }
"#,
        )
        .unwrap();
        let entry = programs_yaml_entry(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let (modified, inserted) = upsert_into_programs_yaml(initial, entry).unwrap();
        assert!(!inserted, "second time should be replace");
        let arr = modified
            .get(FLEET_PROGRAMS_KEY_PROGRAMS)
            .unwrap()
            .as_sequence()
            .unwrap();
        assert_eq!(arr.len(), 2, "no new entry added");
        let updated_module = arr[0]
            .get(COMPUTEUNIT_SPEC_KEY_MODULE)
            .unwrap()
            .get(COMPUTEUNIT_MODULE_KEY_SOURCE)
            .and_then(|s| s.as_str());
        assert_eq!(
            updated_module,
            Some("oci://ghcr.io/pleme-io/hello-rio:v0.1.0")
        );
    }

    #[test]
    fn upsert_helmrelease_inserts_under_spec_values_programs() {
        let initial: serde_yaml::Value = serde_yaml::from_str(
            r#"
apiVersion: helm.toolkit.fluxcd.io/v2
kind: HelmRelease
metadata:
  name: rio-fleet-programs
  namespace: tatara-system
spec:
  interval: 30m
  chart:
    spec:
      chart: lareira-fleet-programs
  values:
    enabled: true
    defaultNamespace: tatara-system
    programs:
      - name: existing
        module: { source: github:foo/bar }
"#,
        )
        .unwrap();
        let entry = programs_yaml_entry(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let (modified, inserted) = upsert_into_helmrelease_programs(initial, entry).unwrap();
        assert!(inserted);
        let arr = modified
            .get(KUBE_KEY_SPEC)
            .unwrap()
            .get(FLUX_KEY_VALUES)
            .unwrap()
            .get(FLEET_PROGRAMS_KEY_PROGRAMS)
            .unwrap()
            .as_sequence()
            .unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(
            arr[1].get(FLEET_PROGRAMS_KEY_NAME).and_then(|n| n.as_str()),
            Some("hello-rio")
        );
    }

    #[test]
    fn upsert_helmrelease_replaces_existing() {
        let initial: serde_yaml::Value = serde_yaml::from_str(
            r#"
apiVersion: helm.toolkit.fluxcd.io/v2
kind: HelmRelease
metadata: { name: rio-fleet-programs }
spec:
  values:
    programs:
      - name: hello-rio
        module: { source: oci://ghcr.io/pleme-io/hello-rio:v0.0.1 }
      - name: other
        module: { source: github:foo/bar }
"#,
        )
        .unwrap();
        let entry = programs_yaml_entry(&sample_caixa(), &sample_cu_yaml()).unwrap();
        let (modified, inserted) = upsert_into_helmrelease_programs(initial, entry).unwrap();
        assert!(!inserted);
        let arr = modified
            .get(KUBE_KEY_SPEC)
            .unwrap()
            .get(FLUX_KEY_VALUES)
            .unwrap()
            .get(FLEET_PROGRAMS_KEY_PROGRAMS)
            .unwrap()
            .as_sequence()
            .unwrap();
        assert_eq!(arr.len(), 2);
        let updated = arr[0]
            .get(COMPUTEUNIT_SPEC_KEY_MODULE)
            .unwrap()
            .get(COMPUTEUNIT_MODULE_KEY_SOURCE)
            .and_then(|s| s.as_str());
        assert_eq!(updated, Some("oci://ghcr.io/pleme-io/hello-rio:v0.1.0"));
    }

    #[test]
    fn limits_slot_propagates_into_programs_yaml_entry() {
        use caixa_core::LimitsSpec;
        use std::time::Duration;
        let mut c = sample_caixa();
        c.limits = Some(LimitsSpec {
            memory: Some(64 * 1024 * 1024),
            fuel: Some(1_000_000),
            wall_clock: Some(Duration::from_secs(30)),
            cpu: Some(500),
        });
        let entry = programs_yaml_entry(&c, &sample_cu_yaml()).unwrap();
        let limits = entry.get(M2_KEY_LIMITS).expect("limits propagates");
        assert_eq!(
            limits.get(M2_LIMITS_KEY_MEMORY).and_then(|m| m.as_str()),
            Some("64MiB")
        );
        assert_eq!(
            limits.get(M2_LIMITS_KEY_CPU).and_then(|m| m.as_str()),
            Some("500m")
        );
    }

    #[test]
    fn behavior_slot_propagates_into_programs_yaml_entry() {
        use caixa_core::BehaviorSpec;
        use std::path::PathBuf;
        let mut c = sample_caixa();
        c.behavior = Some(BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_state_change: Some(PathBuf::from("lib/migrations.lisp")),
            ..Default::default()
        });
        let entry = programs_yaml_entry(&c, &sample_cu_yaml()).unwrap();
        let behavior = entry.get(M2_KEY_BEHAVIOR).expect("behavior propagates");
        assert_eq!(
            behavior
                .get(M2_BEHAVIOR_KEY_ON_INIT)
                .and_then(|v| v.as_str()),
            Some("lib/init.lisp")
        );
    }

    #[test]
    fn upgrade_from_slot_propagates_into_programs_yaml_entry() {
        use caixa_core::{UpgradeFromEntry, UpgradeInstruction};
        let mut c = sample_caixa();
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.0.9".into(),
            instructions: vec![UpgradeInstruction::SoftPurge {
                module: "hello-rio-old".into(),
            }],
        }];
        let entry = programs_yaml_entry(&c, &sample_cu_yaml()).unwrap();
        let upgrade_from = entry
            .get(M2_KEY_UPGRADE_FROM)
            .and_then(|u| u.as_sequence())
            .expect("upgradeFrom propagates as a sequence");
        assert_eq!(upgrade_from.len(), 1);
        assert_eq!(
            upgrade_from[0]
                .get(M2_UPGRADE_FROM_KEY_FROM)
                .and_then(|f| f.as_str()),
            Some("0.0.9")
        );
    }

    #[test]
    fn empty_m2_slots_do_not_appear_in_programs_yaml_entry() {
        // Forward-compat invariant: a Servico with no M2 slots emits a
        // programs.yaml entry that's structurally identical to V0
        // (no extra keys).
        let entry = programs_yaml_entry(&sample_caixa(), &sample_cu_yaml()).unwrap();
        assert!(entry.get(M2_KEY_LIMITS).is_none());
        assert!(entry.get(M2_KEY_BEHAVIOR).is_none());
        assert!(entry.get(M2_KEY_UPGRADE_FROM).is_none());
    }

    #[test]
    fn cluster_bundle_three_files() {
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        assert_eq!(files.len(), 3);
        let names: Vec<_> = files
            .iter()
            .map(|f| f.path.to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&FLUX_GITREPOSITORY_YAML_FILENAME.to_string()));
        assert!(names.contains(&FLUX_HELMRELEASE_YAML_FILENAME.to_string()));
        assert!(names.contains(&FLUX_KUSTOMIZATION_YAML_FILENAME.to_string()));

        let kust = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
            .unwrap();
        assert!(kust.contents.contains("./clusters/rio/services/hello-rio"));

        let gitrepo = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_GITREPOSITORY_YAML_FILENAME))
            .unwrap();
        assert!(gitrepo.contents.contains("v0.1.0"));
    }

    #[test]
    fn default_library_name_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::DEFAULT_LIBRARY_NAME` is
        // the single source of truth for the Helm library-chart wrap
        // key the `cluster_bundle` `helmrelease.yaml` template scopes
        // the per-cluster `enabled: true` override under. Pin the
        // equality (and the static-data identity, peer with the
        // sibling `default_namespace_re_export_points_at_caixa_core_canonical`
        // pin) so any local re-introduction of a sibling `pub const
        // DEFAULT_LIBRARY_NAME: &str = "…"` (the canonical drift
        // footgun this lift closes — two production-code consumers of
        // the same load-bearing Helm library-chart name across
        // caixa-helm + caixa-flux, lifted to one re-export at the
        // caixa-core boundary) is a build-time test failure naming
        // the offending drift, not a silent apply-time wrap-key
        // mismatch routing the per-cluster override nowhere. Peer to
        // `caixa_helm::tests::default_library_name_re_export_points_at_caixa_core_canonical`
        // on the sibling renderer crate.
        caixa_core::assert_str_reexport_identity(
            "DEFAULT_LIBRARY_NAME",
            DEFAULT_LIBRARY_NAME,
            caixa_core::DEFAULT_LIBRARY_NAME,
        );
    }

    #[test]
    fn cluster_bundle_helmrelease_values_wrap_key_uses_lifted_constant() {
        // Fail-before-pass-after pin: the rendered `helmrelease.yaml`'s
        // `spec.values.<library>:` wrap key — the scope under which
        // per-cluster overrides like `enabled: true` reach the
        // dependent library chart at `helm template` / `helm install`
        // time — must spell out the lifted [`DEFAULT_LIBRARY_NAME`]
        // verbatim. Before the lift this site carried an inline
        // `pleme-computeunit:` literal in the format string; a future
        // per-edition library-chart rebrand (the substrate forking
        // `pleme-computeunit` to `<registry>-computeunit` for a per-
        // cluster image-registry mirror, or to `aplicacao-computeunit`
        // for the M4 typed-Aplicacao renderer's sibling library chart,
        // or any per-tenant variant the absorption-roadmap names) on
        // the sibling caixa-helm `RenderOpts::library_name` axis
        // without a coordinated edit here would have silently routed
        // the per-cluster override nowhere — the cluster's apply would
        // come up with the library chart's defaults, far from the
        // rebrand commit's source.
        //
        // The pin is structural: parse the rendered YAML and assert
        // the wrap key under `spec.values` equals the lifted constant.
        // A regression that re-introduces an inline literal surfaces
        // as a key mismatch (the inline literal would survive, but
        // the lifted-constant-keyed assertion would fail).
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME))
            .expect("helmrelease.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&hr.contents).expect("helmrelease.yaml parses as YAML");
        let values = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_KEY_VALUES))
            .and_then(|v| v.as_mapping())
            .expect("spec.values mapping present");
        assert!(
            values.get(DEFAULT_LIBRARY_NAME).is_some(),
            "spec.values must wrap under the lifted DEFAULT_LIBRARY_NAME \
             ({DEFAULT_LIBRARY_NAME:?}); a drifted literal here silently \
             routes per-cluster overrides nowhere at helm template time"
        );
        // The wrapped block must carry the canonical `enabled: true`
        // overlay — the per-cluster override the bundle path threads
        // through. Pin the round-trip so a refactor that hoists the
        // overlay out of the wrap can't silently drop it.
        let wrapped = values
            .get(DEFAULT_LIBRARY_NAME)
            .and_then(|v| v.as_mapping())
            .expect("wrapped library mapping");
        assert_eq!(
            wrapped
                .get(HELM_VALUES_KEY_ENABLED)
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn cluster_bundle_helmrelease_wrap_key_pins_canonical_pleme_computeunit_string() {
        // Bridge-arm pin: the lifted [`DEFAULT_LIBRARY_NAME`] constant
        // resolves to the canonical `"pleme-computeunit"` string today,
        // and the rendered `helmrelease.yaml`'s wrap key must spell
        // it out verbatim. Pin the literal here (peer with the
        // [`caixa_helm::tests::values_yaml_wraps_under_pleme_computeunit_key`]
        // canonical-default arm on the chart-render side) so a future
        // rebrand of the lifted constant surfaces here as a coordinated
        // edit-point — same trajectory as the
        // `default_servico_port_constant_pins_canonical_8080_literal`
        // bridge-arm pin in caixa-core.
        assert_eq!(DEFAULT_LIBRARY_NAME, "pleme-computeunit");
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME))
            .unwrap();
        assert!(
            hr.contents.contains("pleme-computeunit:"),
            "helmrelease.yaml must spell the canonical library-chart wrap \
             key under spec.values (got: {contents:?})",
            contents = hr.contents
        );
    }

    #[test]
    fn helm_values_key_enabled_re_export_points_at_caixa_core_canonical() {
        // The renderer's `HELM_VALUES_KEY_ENABLED` was lifted from the
        // production-code inline `enabled: true\n` fragment inside
        // [`cluster_bundle`]'s `helmrelease.yaml` format-string template
        // (formerly `caixa-flux/src/lib.rs:844`) plus its test-side
        // round-trip navigator
        // (`cluster_bundle_helmrelease_values_wrap_key_uses_lifted_constant`,
        // where `.get("enabled")`
        // isolated the per-cluster override the bundle path threads
        // through) to a re-export of
        // [`caixa_core::HELM_VALUES_KEY_ENABLED`] so the canonical
        // `pleme-computeunit` library-chart values-block enable-toggle
        // key lives in exactly one place across every caixa renderer.
        // Pin the equality + `&'static` static-data identity here so any
        // local re-introduction of a sibling
        // `pub const HELM_VALUES_KEY_ENABLED: &str = "…"` at this crate
        // — the canonical drift footgun where a sibling local
        // `pub const` could happen to carry the same string at the
        // source while pointing at a different `&'static` allocation —
        // is a build-time test failure naming the offending drift, not
        // a silent per-values enable-toggle reroute at `helm template` /
        // `helm install` time far from the drift site. Peer to
        // [`default_library_name_re_export_points_at_caixa_core_canonical`]
        // / [`kube_key_spec_re_export_points_at_caixa_core_canonical`] on
        // the sibling re-export axes +
        // `caixa_helm::tests::helm_values_key_enabled_re_export_points_at_caixa_core_canonical`
        // on the peer per-Servico-chart renderer crate.
        caixa_core::assert_str_reexport_identity(
            "HELM_VALUES_KEY_ENABLED",
            HELM_VALUES_KEY_ENABLED,
            caixa_core::HELM_VALUES_KEY_ENABLED,
        );
    }

    #[test]
    fn default_flux_system_namespace_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE`
        // is the single source of truth for the FluxCD installation
        // namespace both axes of the rendered `kustomization.yaml`
        // document (`metadata.namespace` and `spec.sourceRef.name`)
        // consume. Pin the equality (and the static-data identity, peer
        // with the sibling
        // `default_namespace_re_export_points_at_caixa_core_canonical` /
        // `default_library_name_re_export_points_at_caixa_core_canonical`
        // pins) so any local re-introduction of a sibling `pub const
        // DEFAULT_FLUX_SYSTEM_NAMESPACE: &str = "…"` (the canonical
        // drift footgun this lift closes — two production-code
        // consumers of the same load-bearing FluxCD-installation-
        // namespace inside the kustomization template, lifted to one
        // re-export at the caixa-core boundary) is a build-time test
        // failure naming the offending drift, not a silent apply-time
        // `Kustomization`-outside-controller-watch-window / dangling-
        // `sourceRef` reconciliation freeze.
        caixa_core::assert_str_reexport_identity(
            "DEFAULT_FLUX_SYSTEM_NAMESPACE",
            DEFAULT_FLUX_SYSTEM_NAMESPACE,
            caixa_core::DEFAULT_FLUX_SYSTEM_NAMESPACE,
        );
    }

    #[test]
    fn cluster_bundle_kustomization_uses_lifted_flux_system_namespace() {
        // Fail-before-pass-after pin: the rendered `kustomization.yaml`'s
        // `metadata.namespace` and `spec.sourceRef.name` axes — the two
        // physical sites the inline `flux-system` literal previously
        // sat at (caixa-flux/src/lib.rs:477, 483 in the prior shape) —
        // must both resolve to the lifted
        // [`DEFAULT_FLUX_SYSTEM_NAMESPACE`] verbatim. Before this lift
        // the kustomization template carried two inline `flux-system`
        // literals; a future per-cluster Flux installation rebrand on
        // either axis without a coordinated edit on the other would have
        // silently emitted a `Kustomization` outside the bootstrap
        // controller's watch window (the `kustomize-controller` watches
        // the installation namespace by default) or a dangling
        // `spec.sourceRef.name` pointing at a `GitRepository` that
        // doesn't exist in the rebranded namespace.
        //
        // The pin is structural: parse the rendered YAML and assert
        // each of the two axes equals the lifted constant by value. A
        // regression that re-introduces an inline literal at either
        // axis surfaces here as a key mismatch (the inline literal
        // would survive, but the lifted-constant-keyed assertion would
        // fail when the lift's value changes).
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let kust = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
            .expect("kustomization.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&kust.contents).expect("kustomization.yaml parses as YAML");
        assert_eq!(
            kube_metadata_str_field(&parsed, KUBE_KEY_NAMESPACE),
            Some(DEFAULT_FLUX_SYSTEM_NAMESPACE),
            "kustomization.yaml metadata.namespace must spell the lifted \
             DEFAULT_FLUX_SYSTEM_NAMESPACE ({DEFAULT_FLUX_SYSTEM_NAMESPACE:?}); \
             a drifted literal here silently places the Kustomization outside \
             the bootstrap kustomize-controller's watch window"
        );
        assert_eq!(
            parsed
                .get(KUBE_KEY_SPEC)
                .and_then(|s| s.get(FLUX_KEY_SOURCE_REF))
                .and_then(|r| r.get("name"))
                .and_then(|n| n.as_str()),
            Some(DEFAULT_FLUX_SYSTEM_NAMESPACE),
            "kustomization.yaml spec.sourceRef.name must spell the lifted \
             DEFAULT_FLUX_SYSTEM_NAMESPACE ({DEFAULT_FLUX_SYSTEM_NAMESPACE:?}); \
             a drifted literal here dangles the reference at a GitRepository \
             that doesn't exist in the rebranded installation namespace"
        );
    }

    #[test]
    fn cluster_bundle_kustomization_pins_canonical_flux_system_string() {
        // Bridge-arm pin: the lifted [`DEFAULT_FLUX_SYSTEM_NAMESPACE`]
        // constant resolves to the canonical `"flux-system"` string
        // today, and both rendered `kustomization.yaml` axes must spell
        // it out verbatim. Pin the literal here (peer with the
        // [`default_flux_system_namespace_pins_canonical_value`]
        // canonical-default arm in caixa-core, and with
        // [`cluster_bundle_helmrelease_wrap_key_pins_canonical_pleme_computeunit_string`]
        // on the sibling `DEFAULT_LIBRARY_NAME` axis) so a future
        // rebrand of the lifted constant surfaces here as a coordinated
        // edit-point, same trajectory as the
        // `default_servico_port_constant_pins_canonical_8080_literal`
        // bridge-arm pin in caixa-core.
        assert_eq!(DEFAULT_FLUX_SYSTEM_NAMESPACE, "flux-system");
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let kust = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
            .unwrap();
        assert!(
            kust.contents.contains("namespace: flux-system\n"),
            "kustomization.yaml must spell the canonical FluxCD \
             installation namespace at metadata.namespace (got: {contents:?})",
            contents = kust.contents
        );
        assert!(
            kust.contents.contains("name: flux-system\n"),
            "kustomization.yaml must spell the canonical FluxCD \
             installation namespace at spec.sourceRef.name (got: {contents:?})",
            contents = kust.contents
        );
    }

    #[test]
    fn cluster_bundle_helmrelease_uses_lifted_flux_api_version() {
        // Fail-before-pass-after pin: the rendered `helmrelease.yaml`
        // `apiVersion` axis — the load-bearing Flux v2 CRD-group/version
        // declaration the `helm-controller` watches — must resolve to the
        // lifted [`FLUX_HELMRELEASE_API_VERSION`] verbatim. Before this
        // lift the helmrelease template carried an inline
        // `helm.toolkit.fluxcd.io/v2` literal; a future upstream Flux v3
        // migration on this axis without a coordinated edit on the
        // sibling kustomization `healthChecks[].apiVersion` (the second
        // render-side occurrence the same lift threads through) would
        // have silently routed the rendered `HelmRelease` outside the
        // controller's `Watches` and broken at apply time with a non-
        // self-locating "no kind 'HelmRelease' is registered for version
        // 'helm.toolkit.fluxcd.io/v2beta2'" error. Peer with
        // [`cluster_bundle_kustomization_uses_lifted_flux_system_namespace`]
        // on the sibling [`DEFAULT_FLUX_SYSTEM_NAMESPACE`] lift.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME))
            .expect("helmrelease.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&hr.contents).expect("helmrelease.yaml parses as YAML");
        assert_eq!(
            kube_root_str_field(&parsed, KUBE_KEY_API_VERSION),
            Some(FLUX_HELMRELEASE_API_VERSION),
            "helmrelease.yaml apiVersion must spell the lifted \
             FLUX_HELMRELEASE_API_VERSION ({FLUX_HELMRELEASE_API_VERSION:?}); \
             a drifted literal here routes the HelmRelease outside the Flux v2 \
             helm-controller's Watches",
        );
    }

    #[test]
    fn cluster_bundle_kustomization_health_check_uses_lifted_flux_api_version() {
        // Sibling-axis pin to
        // `cluster_bundle_helmrelease_uses_lifted_flux_api_version`: the
        // rendered `kustomization.yaml`'s `spec.healthChecks[].apiVersion`
        // axis is the Flux v2 contract pairing the parent Kustomization's
        // per-resource health-gate to the sibling HelmRelease's CRD
        // group/version. Both axes must resolve to the same lifted
        // constant by value — a future upstream Flux v3 migration on
        // either axis without a coordinated edit on the other would
        // have silently dangled the health-check (apply-side: the per-
        // resource health-gate never resolves, the parent Kustomization
        // sits perpetually in `Reconciling`).
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let kust = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
            .expect("kustomization.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&kust.contents).expect("kustomization.yaml parses as YAML");
        let health_checks = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_KEY_HEALTH_CHECKS))
            .and_then(|h| h.as_sequence())
            .expect("kustomization.yaml spec.healthChecks present");
        assert!(
            !health_checks.is_empty(),
            "kustomization.yaml spec.healthChecks must carry at least one \
             entry — the rendered Kustomization gates on its sibling \
             HelmRelease's health by construction",
        );
        for (i, entry) in health_checks.iter().enumerate() {
            assert_eq!(
                kube_root_str_field(entry, KUBE_KEY_API_VERSION),
                Some(FLUX_HELMRELEASE_API_VERSION),
                "kustomization.yaml spec.healthChecks[{i}].apiVersion must \
                 spell the lifted FLUX_HELMRELEASE_API_VERSION \
                 ({FLUX_HELMRELEASE_API_VERSION:?}); a drifted literal here \
                 dangles the per-resource health-gate at apply time",
            );
        }
    }

    #[test]
    fn cluster_bundle_helmrelease_pins_canonical_flux_v2_api_version_string() {
        // Bridge-arm pin: the lifted [`FLUX_HELMRELEASE_API_VERSION`]
        // constant resolves to the canonical
        // `"helm.toolkit.fluxcd.io/v2"` string today, and both rendered
        // axes (helmrelease.yaml apiVersion + kustomization.yaml
        // healthChecks[].apiVersion) must spell it out verbatim. Pin the
        // literal here (peer with the
        // [`flux_helmrelease_api_version_pins_canonical_value`] canonical-
        // default arm in caixa-core, and with
        // [`cluster_bundle_kustomization_pins_canonical_flux_system_string`]
        // on the sibling `DEFAULT_FLUX_SYSTEM_NAMESPACE` axis) so a
        // future Flux v3 migration of the lifted constant surfaces here
        // as a coordinated edit-point.
        assert_eq!(FLUX_HELMRELEASE_API_VERSION, "helm.toolkit.fluxcd.io/v2");
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME))
            .unwrap();
        assert!(
            hr.contents
                .contains("apiVersion: helm.toolkit.fluxcd.io/v2\n"),
            "helmrelease.yaml must spell the canonical Flux v2 HelmRelease \
             apiVersion at the top-level apiVersion axis (got: {contents:?})",
            contents = hr.contents,
        );
        let kust = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
            .unwrap();
        assert!(
            kust.contents
                .contains("apiVersion: helm.toolkit.fluxcd.io/v2\n"),
            "kustomization.yaml must spell the canonical Flux v2 HelmRelease \
             apiVersion at spec.healthChecks[].apiVersion (got: {contents:?})",
            contents = kust.contents,
        );
    }

    #[test]
    fn cluster_bundle_default_git_tag_uses_lifted_caixa_core_prefix() {
        // Fail-before-pass-after pin: the [`ClusterBundleOpts::for_caixa`]
        // constructor's default `git_ref: GitRefSpec::Tag(...)` must
        // compose the lifted [`caixa_core::DEFAULT_PUBLISH_TAG_PREFIX`]
        // against the caixa's `:versao` — not an inline `"v"` byte the
        // peer `feira publish` `--prefix` default and this deploy-side
        // default could silently drift on.
        //
        // Until this lift landed the deploy-side carried a `format!("v{}",
        // caixa.versao)` literal while the writer-side (caixa-feira/src/cmd/publish.rs:22)
        // carried a clap `default_value = "v"` literal — two production-code
        // consumers of the same git-tag-naming convention on the same
        // git remote axis, drift-prone by construction. A future
        // Zig-style-tag rebrand on one side (e.g. moving the publisher to
        // `release/<versao>` once a sibling forge convention lands)
        // without a coordinated edit here would silently emit a tag the
        // FluxCD `GitRepository` reconciler can't resolve — the
        // dependent `HelmRelease`'s `chart: sourceRef` would never
        // converge and every per-Servico apply would silently come up
        // with the prior reconciled state, with the failure surfacing
        // far from the rebrand commit's source at
        // `kubectl describe gitrepository` time.
        //
        // Pin the equality on the constructed `GitRefSpec::Tag` body and
        // on the rendered `gitrepository.yaml`'s `ref: { tag: ... }`
        // field so a regression that re-inlines the `"v"` literal at
        // either layer (this constructor or the format-string in
        // [`cluster_bundle`]) surfaces here as a build-time test failure
        // rather than as a silent deploy-time `GitRepository` reconcile
        // loop. Peer to the sibling [`caixa-feira`]
        // `publish_prefix_default_pins_lifted_caixa_core_constant` test
        // closing the same drift on the writer-side.
        let caixa = sample_caixa();
        let opts = ClusterBundleOpts::for_caixa(&caixa, "rio");
        match &opts.git_ref {
            GitRefSpec::Tag(tag) => {
                assert!(
                    tag.starts_with(caixa_core::DEFAULT_PUBLISH_TAG_PREFIX),
                    "default git_ref tag must start with the lifted \
                     caixa_core::DEFAULT_PUBLISH_TAG_PREFIX (got: {tag:?})"
                );
                assert_eq!(
                    tag,
                    &format!(
                        "{prefix}{versao}",
                        prefix = caixa_core::DEFAULT_PUBLISH_TAG_PREFIX,
                        versao = caixa.versao,
                    ),
                    "default git_ref tag must compose the lifted prefix \
                     against the caixa's :versao verbatim"
                );
            }
            other => panic!("expected GitRefSpec::Tag, got {other:?}"),
        }
        let files = cluster_bundle(&caixa, &opts).unwrap();
        let gr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_GITREPOSITORY_YAML_FILENAME))
            .expect("gitrepository.yaml present");
        let expected_tag = format!(
            "{prefix}{versao}",
            prefix = caixa_core::DEFAULT_PUBLISH_TAG_PREFIX,
            versao = caixa.versao,
        );
        assert!(
            gr.contents.contains(&format!("tag: {expected_tag:?}")),
            "gitrepository.yaml must spell the lifted-prefix-composed tag \
             at ref.tag (expected: {expected_tag:?}, got: {contents:?})",
            contents = gr.contents
        );
    }

    #[test]
    fn flux_gitrepository_api_version_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_GITREPOSITORY_API_VERSION`
        // is the single source of truth for the Flux v2 `GitRepository`
        // CRD-group/version the rendered `gitrepository.yaml` document
        // declares at its `apiVersion` axis. Pin the equality (and the
        // static-data identity, peer with the sibling
        // `default_flux_system_namespace_re_export_points_at_caixa_core_canonical` /
        // `default_library_name_re_export_points_at_caixa_core_canonical`
        // pins) so any local re-introduction of a sibling `pub const
        // FLUX_GITREPOSITORY_API_VERSION: &str = "…"` (the canonical drift
        // footgun this lift closes — one production-code consumer of the
        // load-bearing Flux v2 `GitRepository` CRD-group/version inside
        // the gitrepository template, lifted to one re-export at the
        // caixa-core boundary) is a build-time test failure naming the
        // offending drift, not a silent apply-time `GitRepository`-
        // outside-controller-watch-window reconciliation freeze.
        caixa_core::assert_str_reexport_identity(
            "FLUX_GITREPOSITORY_API_VERSION",
            FLUX_GITREPOSITORY_API_VERSION,
            caixa_core::FLUX_GITREPOSITORY_API_VERSION,
        );
    }

    #[test]
    fn cluster_bundle_gitrepository_uses_lifted_flux_api_version() {
        // Fail-before-pass-after pin: the rendered `gitrepository.yaml`
        // `apiVersion` axis — the load-bearing Flux v2 CRD-group/version
        // declaration the `source-controller` watches — must resolve to the
        // lifted [`FLUX_GITREPOSITORY_API_VERSION`] verbatim. Before this
        // lift the gitrepository template carried an inline
        // `source.toolkit.fluxcd.io/v1` literal; a future upstream Flux v3
        // migration on this axis without a coordinated edit on the sibling
        // [`FLUX_HELMRELEASE_API_VERSION`] axis (the Flux v2 controller-
        // triple shares the `.toolkit.fluxcd.io` root and promotes together
        // on each major bump) would have silently routed the rendered
        // `GitRepository` outside the controller's `Watches` and broken at
        // apply time with a non-self-locating "no kind 'GitRepository' is
        // registered for version 'source.toolkit.fluxcd.io/v1beta2'"
        // error. Peer with
        // [`cluster_bundle_helmrelease_uses_lifted_flux_api_version`] on
        // the sibling [`FLUX_HELMRELEASE_API_VERSION`] lift.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let gr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_GITREPOSITORY_YAML_FILENAME))
            .expect("gitrepository.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&gr.contents).expect("gitrepository.yaml parses as YAML");
        assert_eq!(
            kube_root_str_field(&parsed, KUBE_KEY_API_VERSION),
            Some(FLUX_GITREPOSITORY_API_VERSION),
            "gitrepository.yaml apiVersion must spell the lifted \
             FLUX_GITREPOSITORY_API_VERSION ({FLUX_GITREPOSITORY_API_VERSION:?}); \
             a drifted literal here routes the GitRepository outside the Flux v2 \
             source-controller's Watches",
        );
    }

    #[test]
    fn cluster_bundle_gitrepository_pins_canonical_flux_v1_api_version_string() {
        // Bridge-arm pin: the lifted [`FLUX_GITREPOSITORY_API_VERSION`]
        // constant resolves to the canonical
        // `"source.toolkit.fluxcd.io/v1"` string today, and the rendered
        // `gitrepository.yaml`'s `apiVersion` axis must spell it out
        // verbatim. Pin the literal here (peer with the
        // [`flux_gitrepository_api_version_pins_canonical_value`]
        // canonical-default arm in caixa-core, and with
        // [`cluster_bundle_helmrelease_pins_canonical_flux_v2_api_version_string`]
        // on the sibling `FLUX_HELMRELEASE_API_VERSION` axis) so a future
        // Flux v3 migration of the lifted constant surfaces here as a
        // coordinated edit-point.
        assert_eq!(
            FLUX_GITREPOSITORY_API_VERSION,
            "source.toolkit.fluxcd.io/v1"
        );
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let gr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_GITREPOSITORY_YAML_FILENAME))
            .unwrap();
        assert!(
            gr.contents
                .contains("apiVersion: source.toolkit.fluxcd.io/v1\n"),
            "gitrepository.yaml must spell the canonical Flux v2 GitRepository \
             apiVersion at the top-level apiVersion axis (got: {contents:?})",
            contents = gr.contents,
        );
    }

    #[test]
    fn flux_kustomization_api_version_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_KUSTOMIZATION_API_VERSION`
        // is the single source of truth for the Flux v2 `Kustomization`
        // CRD-group/version the rendered `kustomization.yaml` document
        // declares at its `apiVersion` axis. Pin the equality (and the
        // static-data identity, peer with the sibling
        // [`flux_gitrepository_api_version_re_export_points_at_caixa_core_canonical`]
        // / [`default_flux_system_namespace_re_export_points_at_caixa_core_canonical`]
        // pins) so any local re-introduction of a sibling `pub const
        // FLUX_KUSTOMIZATION_API_VERSION: &str = "…"` (the canonical drift
        // footgun this lift closes — one production-code consumer of the
        // load-bearing Flux v2 `Kustomization` CRD-group/version inside
        // the kustomization template, lifted to one re-export at the
        // caixa-core boundary) is a build-time test failure naming the
        // offending drift, not a silent apply-time `Kustomization`-
        // outside-controller-watch-window reconciliation freeze.
        caixa_core::assert_str_reexport_identity(
            "FLUX_KUSTOMIZATION_API_VERSION",
            FLUX_KUSTOMIZATION_API_VERSION,
            caixa_core::FLUX_KUSTOMIZATION_API_VERSION,
        );
    }

    #[test]
    fn cluster_bundle_kustomization_uses_lifted_flux_api_version() {
        // Fail-before-pass-after pin: the rendered `kustomization.yaml`
        // top-level `apiVersion` axis — the load-bearing Flux v2
        // CRD-group/version declaration the `kustomize-controller`
        // watches — must resolve to the lifted
        // [`FLUX_KUSTOMIZATION_API_VERSION`] verbatim. Before this lift
        // the kustomization template carried an inline
        // `kustomize.toolkit.fluxcd.io/v1` literal; a future upstream
        // Flux v3 migration on this axis without a coordinated edit on
        // the sibling [`FLUX_HELMRELEASE_API_VERSION`] /
        // [`FLUX_GITREPOSITORY_API_VERSION`] axes (the Flux v2
        // controller triplet shares the `.toolkit.fluxcd.io` root and
        // promotes together on each major bump) would have silently
        // routed the rendered `Kustomization` outside the controller's
        // `Watches` and broken at apply time with a non-self-locating
        // "no kind 'Kustomization' is registered for version
        // 'kustomize.toolkit.fluxcd.io/v1beta2'" error. Peer with
        // [`cluster_bundle_helmrelease_uses_lifted_flux_api_version`] /
        // [`cluster_bundle_gitrepository_uses_lifted_flux_api_version`]
        // on the sibling Flux-CRD-axis lifts.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let kz = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
            .expect("kustomization.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&kz.contents).expect("kustomization.yaml parses as YAML");
        assert_eq!(
            kube_root_str_field(&parsed, KUBE_KEY_API_VERSION),
            Some(FLUX_KUSTOMIZATION_API_VERSION),
            "kustomization.yaml apiVersion must spell the lifted \
             FLUX_KUSTOMIZATION_API_VERSION ({FLUX_KUSTOMIZATION_API_VERSION:?}); \
             a drifted literal here routes the Kustomization outside the Flux v2 \
             kustomize-controller's Watches",
        );
    }

    #[test]
    fn cluster_bundle_kustomization_pins_canonical_flux_v1_api_version_string() {
        // Bridge-arm pin: the lifted [`FLUX_KUSTOMIZATION_API_VERSION`]
        // constant resolves to the canonical
        // `"kustomize.toolkit.fluxcd.io/v1"` string today, and the
        // rendered `kustomization.yaml`'s top-level `apiVersion` axis
        // must spell it out verbatim. Pin the literal here (peer with
        // the [`flux_kustomization_api_version_pins_canonical_value`]
        // canonical-default arm in caixa-core, and with
        // [`cluster_bundle_helmrelease_pins_canonical_flux_v2_api_version_string`]
        // / [`cluster_bundle_gitrepository_pins_canonical_flux_v1_api_version_string`]
        // on the sibling `FLUX_HELMRELEASE_API_VERSION` /
        // `FLUX_GITREPOSITORY_API_VERSION` axes) so a future Flux v3
        // migration of the lifted constant surfaces here as a
        // coordinated edit-point.
        assert_eq!(
            FLUX_KUSTOMIZATION_API_VERSION,
            "kustomize.toolkit.fluxcd.io/v1"
        );
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let kz = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
            .unwrap();
        assert!(
            kz.contents
                .contains("apiVersion: kustomize.toolkit.fluxcd.io/v1\n"),
            "kustomization.yaml must spell the canonical Flux v2 Kustomization \
             apiVersion at the top-level apiVersion axis (got: {contents:?})",
            contents = kz.contents,
        );
    }

    #[test]
    fn cluster_bundle_every_flux_cr_carries_top_level_api_version_label_from_lifted_key() {
        // Fail-before-pass-after production-side sweep pin: every one of
        // the three rendered Flux bundle files' top-level `apiVersion:`
        // YAML label — the load-bearing per-CR CRD-group/version-axis
        // label naming the exact key the apiserver's `RESTMapper` reads
        // to resolve each CR's registered `CustomResourceDefinition` —
        // must byte-compose the lifted [`KUBE_KEY_API_VERSION`] verbatim
        // as its label prefix. Before this sweep the three
        // [`cluster_bundle`] format-string templates carried four inline
        // `apiVersion:` YAML label literals (gitrepository.yaml top-level
        // + helmrelease.yaml top-level + kustomization.yaml top-level +
        // kustomization.yaml `spec.healthChecks[].apiVersion`) side-by-
        // side with their `{api_version}`-interpolated value axes; a
        // future rebrand of the lifted [`KUBE_KEY_API_VERSION`] const
        // (or a coordinated K8s-API-conventions per-major-version
        // discriminator promotion) had to reach every inline label site
        // in lockstep, or the emit-side silently kept the pre-rebrand
        // label byte while the per-CR body-key retrieval sites (already
        // routed through [`KUBE_KEY_API_VERSION`]) rebranded — the two
        // sides would then disagree on the label byte, and every
        // downstream apiserver-side `RESTMapper` lookup on the rendered
        // CR would silently miss its per-CR CRD-group/version
        // registration with no field naming the label-drift root cause
        // far from the source caixa.lisp. Peer to
        // [`cluster_bundle_helmrelease_uses_lifted_flux_api_version`] /
        // [`cluster_bundle_gitrepository_uses_lifted_flux_api_version`] /
        // [`cluster_bundle_kustomization_uses_lifted_flux_api_version`]
        // on the sibling per-CR `.get(KUBE_KEY_API_VERSION)` retrieval-
        // side pins one level below the raw-byte label-axis this pin
        // gates.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let label_prefix = format!("{KUBE_KEY_API_VERSION}: ");
        for filename in [
            FLUX_GITREPOSITORY_YAML_FILENAME,
            FLUX_HELMRELEASE_YAML_FILENAME,
            FLUX_KUSTOMIZATION_YAML_FILENAME,
        ] {
            let f = files
                .iter()
                .find(|f| f.path == std::path::PathBuf::from(filename))
                .unwrap_or_else(|| panic!("{filename} present"));
            assert!(
                f.contents.contains(&label_prefix),
                "{filename} must carry the top-level {label_prefix:?} YAML label \
                 composed from the lifted KUBE_KEY_API_VERSION ({KUBE_KEY_API_VERSION:?}); \
                 a drifted inline label here silently rebrands the per-CR \
                 CRD-group/version-axis discriminator away from the lifted key \
                 (got: {contents:?})",
                contents = f.contents,
            );
        }
    }

    #[test]
    fn cluster_bundle_every_flux_cr_carries_top_level_kind_label_from_lifted_key() {
        // Fail-before-pass-after production-side sweep pin: every one of
        // the three rendered Flux bundle files' top-level `kind:` YAML
        // label — the load-bearing per-CR CRD-`kind`-discriminator-axis
        // label naming the exact key the apiserver's `RESTMapper` reads
        // to resolve each CR's registered `CustomResourceDefinition`
        // against the sibling `apiVersion:` half of the `(apiVersion,
        // kind)` CRD-lookup tuple — must byte-compose the lifted
        // [`KUBE_KEY_KIND`] verbatim as its label prefix. Before this
        // sweep the three [`cluster_bundle`] format-string templates
        // carried six inline `kind:` YAML label literals
        // (gitrepository.yaml top-level + helmrelease.yaml top-level +
        // helmrelease.yaml `spec.chart.spec.sourceRef.kind` +
        // kustomization.yaml top-level + kustomization.yaml
        // `spec.sourceRef.kind` + kustomization.yaml
        // `spec.healthChecks[].kind`) side-by-side with their
        // `{kind}` / `{source_kind}` / `{health_kind}`-interpolated
        // value axes; a future rebrand of the lifted [`KUBE_KEY_KIND`]
        // const (or a coordinated K8s-API-conventions per-major-version
        // discriminator promotion) had to reach every inline label site
        // in lockstep, or the emit-side silently kept the pre-rebrand
        // label byte while the per-CR body-key retrieval sites (already
        // routed through [`KUBE_KEY_KIND`] via `kube_root_str_field` /
        // `kube_kind_is`) rebranded — the two sides would then disagree
        // on the label byte, and every downstream apiserver-side
        // `RESTMapper` / Flux-controller-side `Watches` predicate on the
        // rendered CR would silently miss its per-CR CRD-`kind` match
        // with no field naming the label-drift root cause far from the
        // source caixa.lisp. Peer to
        // [`cluster_bundle_every_flux_cr_carries_top_level_api_version_label_from_lifted_key`]
        // on the sibling apiVersion half of the same `(apiVersion, kind)`
        // CRD-lookup tuple — extends the production-side raw-byte-label-
        // sweep discipline established there onto the sibling
        // discriminator-half's raw-byte label position, so the two
        // halves of the tuple's label axes both consume the same
        // substrate-owned `&'static str` by construction.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let label_prefix = format!("{KUBE_KEY_KIND}: ");
        for filename in [
            FLUX_GITREPOSITORY_YAML_FILENAME,
            FLUX_HELMRELEASE_YAML_FILENAME,
            FLUX_KUSTOMIZATION_YAML_FILENAME,
        ] {
            let f = files
                .iter()
                .find(|f| f.path == std::path::PathBuf::from(filename))
                .unwrap_or_else(|| panic!("{filename} present"));
            assert!(
                f.contents.contains(&label_prefix),
                "{filename} must carry the top-level {label_prefix:?} YAML label \
                 composed from the lifted KUBE_KEY_KIND ({KUBE_KEY_KIND:?}); \
                 a drifted inline label here silently rebrands the per-CR \
                 CRD-`kind`-discriminator-axis away from the lifted key \
                 (got: {contents:?})",
                contents = f.contents,
            );
        }
    }

    #[test]
    fn cluster_bundle_every_flux_cr_carries_top_level_metadata_label_from_lifted_key() {
        // Fail-before-pass-after production-side sweep pin: every one of
        // the three rendered Flux bundle files' top-level `metadata:`
        // YAML label — the load-bearing per-CR block-scope key naming
        // the exact axis the apiserver reads to resolve each CR's
        // ObjectMeta (`.metadata.name` / `.metadata.namespace` /
        // `.metadata.labels` / `.metadata.annotations`) against the
        // sibling `spec:` half of the top-level `(metadata, spec)`
        // K8s-CR-shape pair — must byte-compose the lifted
        // [`KUBE_KEY_METADATA`] verbatim as its label prefix. Before
        // this sweep the three [`cluster_bundle`] format-string
        // templates carried three inline `metadata:` YAML label
        // literals (gitrepository.yaml top-level +
        // helmrelease.yaml top-level + kustomization.yaml top-level)
        // side-by-side with their `name: {name}` / `namespace:
        // {namespace}` children; a future rebrand of the lifted
        // [`KUBE_KEY_METADATA`] const (or a coordinated
        // K8s-API-conventions per-major-version ObjectMeta-block-key
        // promotion) had to reach every inline label site in lockstep,
        // or the emit-side silently kept the pre-rebrand label byte
        // while the per-CR body-key retrieval sites (already routed
        // through [`KUBE_KEY_METADATA`] via `kube_metadata_str_field`)
        // rebranded — the two sides would then disagree on the label
        // byte, and every downstream apiserver-side ObjectMeta parser
        // on the rendered CR would silently miss its per-CR
        // `metadata.name` / `metadata.namespace` lookup with no field
        // naming the label-drift root cause far from the source
        // caixa.lisp. Peer to
        // [`cluster_bundle_every_flux_cr_carries_top_level_kind_label_from_lifted_key`]
        // (ef2c7ef) /
        // [`cluster_bundle_every_flux_cr_carries_top_level_api_version_label_from_lifted_key`]
        // (6d3cbf0) on the sibling `kind:` / `apiVersion:` halves of
        // the same top-level K8s-CR shape — extends the production-
        // side raw-byte-label-sweep discipline established there onto
        // the sibling ObjectMeta-block-scope label position, so the
        // three top-level K8s-CR body-block label axes all consume
        // the same substrate-owned `&'static str`s by construction.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let label_prefix = format!("{KUBE_KEY_METADATA}:");
        for filename in [
            FLUX_GITREPOSITORY_YAML_FILENAME,
            FLUX_HELMRELEASE_YAML_FILENAME,
            FLUX_KUSTOMIZATION_YAML_FILENAME,
        ] {
            let f = files
                .iter()
                .find(|f| f.path == std::path::PathBuf::from(filename))
                .unwrap_or_else(|| panic!("{filename} present"));
            assert!(
                f.contents.contains(&label_prefix),
                "{filename} must carry the top-level {label_prefix:?} YAML label \
                 composed from the lifted KUBE_KEY_METADATA ({KUBE_KEY_METADATA:?}); \
                 a drifted inline label here silently rebrands the per-CR \
                 ObjectMeta-block-scope axis away from the lifted key \
                 (got: {contents:?})",
                contents = f.contents,
            );
        }
    }

    #[test]
    fn cluster_bundle_every_flux_cr_carries_top_level_spec_label_from_lifted_key() {
        // Fail-before-pass-after production-side sweep pin: every one of
        // the three rendered Flux bundle files' top-level `spec:` YAML
        // label — the load-bearing per-CR block-scope key naming the
        // exact axis the apiserver reads to resolve each CR's payload
        // (`.spec.interval` / `.spec.url` / `.spec.chart.spec.*` /
        // `.spec.sourceRef` / `.spec.path` / `.spec.healthChecks` /
        // `.spec.timeout` / `.spec.install` / `.spec.upgrade` /
        // `.spec.values` / `.spec.prune`) against the sibling
        // `metadata:` half of the top-level `(metadata, spec)` K8s-CR-
        // shape pair — must byte-compose the lifted [`KUBE_KEY_SPEC`]
        // verbatim as its label prefix. Before this sweep the three
        // [`cluster_bundle`] format-string templates carried four inline
        // `spec:` YAML label literals (gitrepository.yaml top-level +
        // helmrelease.yaml top-level + helmrelease.yaml
        // `spec.chart.spec` nested + kustomization.yaml top-level)
        // side-by-side with their per-CR `interval:` / `url:` /
        // `chart:` / `sourceRef:` / `install:` / `upgrade:` / `values:`
        // / `path:` / `healthChecks:` / `timeout:` children; a future
        // rebrand of the lifted [`KUBE_KEY_SPEC`] const (or a
        // coordinated K8s-API-conventions per-major-version body-block-
        // key promotion) had to reach every inline label site in
        // lockstep, or the emit-side silently kept the pre-rebrand
        // label byte while the retrieval-side per-CR body-key readers
        // (already routed through [`KUBE_KEY_SPEC`] via caixa-core's
        // `servico_spec_and_m2_overlay_entries` / caixa-flux's
        // `programs_yaml_entry` / caixa-mesh's per-CNP/HTTPRoute walks)
        // rebranded — the two sides would then disagree on the label
        // byte, and every downstream apiserver-side spec-block parser
        // on the rendered CR would silently miss its per-CR
        // `spec.interval` / `spec.url` / `spec.chart` / `spec.sourceRef`
        // / `spec.install` / `spec.upgrade` / `spec.values` /
        // `spec.path` / `spec.healthChecks` / `spec.timeout` lookup
        // with no field naming the label-drift root cause far from the
        // source caixa.lisp. Peer to
        // [`cluster_bundle_every_flux_cr_carries_top_level_metadata_label_from_lifted_key`]
        // (83ce571) /
        // [`cluster_bundle_every_flux_cr_carries_top_level_kind_label_from_lifted_key`]
        // (ef2c7ef) /
        // [`cluster_bundle_every_flux_cr_carries_top_level_api_version_label_from_lifted_key`]
        // (6d3cbf0) on the sibling `metadata:` / `kind:` / `apiVersion:`
        // halves of the same top-level K8s-CR shape — closes the
        // fourth (and final) canonical K8s-CR top-level block-scope
        // label axis under the same production-side raw-byte-label-
        // sweep discipline the prior three commits established, so
        // every top-level K8s-CR body-block label position across the
        // whole `cluster_bundle` render surface consumes the same
        // substrate-owned `&'static str`s by construction.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let label_prefix = format!("{KUBE_KEY_SPEC}:");
        for filename in [
            FLUX_GITREPOSITORY_YAML_FILENAME,
            FLUX_HELMRELEASE_YAML_FILENAME,
            FLUX_KUSTOMIZATION_YAML_FILENAME,
        ] {
            let f = files
                .iter()
                .find(|f| f.path == std::path::PathBuf::from(filename))
                .unwrap_or_else(|| panic!("{filename} present"));
            assert!(
                f.contents.contains(&label_prefix),
                "{filename} must carry the top-level {label_prefix:?} YAML label \
                 composed from the lifted KUBE_KEY_SPEC ({KUBE_KEY_SPEC:?}); \
                 a drifted inline label here silently rebrands the per-CR \
                 spec-block-scope axis away from the lifted key \
                 (got: {contents:?})",
                contents = f.contents,
            );
        }
    }

    #[test]
    fn cluster_bundle_every_flux_cr_carries_metadata_namespace_label_from_lifted_key() {
        // Fail-before-pass-after production-side sweep pin: every one
        // of the three rendered Flux bundle files' `namespace:` YAML
        // label positions — the load-bearing per-CR-`metadata.namespace`
        // axis the apiserver reads to route each CR into its target
        // namespace (`gitrepository.yaml` top-level +
        // `helmrelease.yaml` top-level + `helmrelease.yaml`
        // `spec.chart.spec.sourceRef.namespace` nested + `kustomization.yaml`
        // top-level + `kustomization.yaml`
        // `spec.healthChecks[].namespace` nested) against the sibling
        // `name:` half of the ObjectMeta / sourceRef / healthCheck
        // `(name, namespace)` identity-pair — must byte-compose the
        // lifted [`KUBE_KEY_NAMESPACE`] verbatim as its label prefix.
        // Before this sweep the three [`cluster_bundle`] format-string
        // templates carried five inline `namespace:` YAML label
        // literals side-by-side with the sibling per-CR `name:` half
        // of every `(name, namespace)` identity-pair emission; a
        // future rebrand of the lifted [`KUBE_KEY_NAMESPACE`] const
        // (or a coordinated K8s-API-conventions per-major-version
        // ObjectMeta-key promotion) had to reach every inline label
        // site in lockstep, or the emit-side silently kept the pre-
        // rebrand label byte while the retrieval-side per-CR
        // `metadata.namespace` readers (already routed through
        // [`KUBE_KEY_NAMESPACE`] via caixa-core's
        // `kube_metadata_str_field` walks + this crate's
        // `programs_yaml_entry` upstream ComputeUnit YAML retrieval +
        // `cluster_bundle`'s rendered `kustomization.yaml` drift-
        // detection pin) rebranded — the two sides would then
        // disagree on the label byte, and every downstream apiserver-
        // side ObjectMeta parser on the rendered CR would silently
        // miss its per-CR `metadata.namespace` lookup and route the
        // resource into `default` (or whatever fallback the calling
        // controller supplies) with no field naming the label-drift
        // root cause far from the source caixa.lisp. Peer to
        // [`cluster_bundle_every_flux_cr_carries_top_level_metadata_label_from_lifted_key`]
        // (83ce571) /
        // [`cluster_bundle_every_flux_cr_carries_top_level_kind_label_from_lifted_key`]
        // (ef2c7ef) /
        // [`cluster_bundle_every_flux_cr_carries_top_level_api_version_label_from_lifted_key`]
        // (6d3cbf0) /
        // [`cluster_bundle_every_flux_cr_carries_top_level_spec_label_from_lifted_key`]
        // (ed23a31) on the sibling `metadata:` / `kind:` /
        // `apiVersion:` / `spec:` halves of the top-level K8s-CR
        // shape — extends the same production-side raw-byte-label-
        // sweep discipline onto the canonical `metadata.namespace`
        // nested axis every rendered Flux v2 bundle document
        // navigates, so every `namespace:` label position across the
        // whole `cluster_bundle` render surface consumes the same
        // substrate-owned `&'static str` by construction.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let label_prefix = format!("{KUBE_KEY_NAMESPACE}:");
        for filename in [
            FLUX_GITREPOSITORY_YAML_FILENAME,
            FLUX_HELMRELEASE_YAML_FILENAME,
            FLUX_KUSTOMIZATION_YAML_FILENAME,
        ] {
            let f = files
                .iter()
                .find(|f| f.path == std::path::PathBuf::from(filename))
                .unwrap_or_else(|| panic!("{filename} present"));
            assert!(
                f.contents.contains(&label_prefix),
                "{filename} must carry the {label_prefix:?} YAML label \
                 composed from the lifted KUBE_KEY_NAMESPACE ({KUBE_KEY_NAMESPACE:?}); \
                 a drifted inline label here silently rebrands the per-CR \
                 metadata.namespace axis away from the lifted key \
                 (got: {contents:?})",
                contents = f.contents,
            );
        }
    }

    #[test]
    fn cluster_bundle_every_flux_cr_carries_metadata_name_label_from_lifted_key() {
        // Fail-before-pass-after production-side sweep pin: every one
        // of the three rendered Flux bundle files' `name:` YAML label
        // positions — the load-bearing per-CR-`metadata.name` axis the
        // apiserver-side ObjectMeta parser keys each rendered CR off
        // (`gitrepository.yaml` top-level +
        // `helmrelease.yaml` top-level +
        // `helmrelease.yaml` `spec.chart.spec.sourceRef.name` nested +
        // `kustomization.yaml` top-level +
        // `kustomization.yaml` `spec.sourceRef.name` nested +
        // `kustomization.yaml` `spec.healthChecks[].name` nested)
        // against the sibling `namespace:` half of the ObjectMeta /
        // sourceRef / healthCheck `(name, namespace)` identity-pair —
        // must byte-compose the lifted [`KUBE_KEY_NAME`] verbatim as
        // its label prefix. Before this sweep the three
        // [`cluster_bundle`] format-string templates carried six
        // inline `name:` YAML label literals side-by-side with the
        // sibling per-CR `namespace:` half of every `(name,
        // namespace)` identity-pair emission; a future rebrand of the
        // lifted [`KUBE_KEY_NAME`] const (or a coordinated K8s-API-
        // conventions per-major-version ObjectMeta-key promotion) had
        // to reach every inline label site in lockstep, or the emit-
        // side silently kept the pre-rebrand label byte while the
        // retrieval-side per-CR `metadata.name` readers (already
        // routed through [`KUBE_KEY_NAME`] via caixa-core's
        // `kube_metadata_str_field` walks + this crate's
        // `programs_yaml_entry` upstream ComputeUnit YAML retrieval)
        // rebranded — the two sides would then disagree on the label
        // byte, and every downstream apiserver-side ObjectMeta parser
        // on the rendered CR would treat the document as an anonymous
        // CR (or the apiserver would reject the apply with a schema-
        // validation error naming the wrong key) with no field naming
        // the label-drift root cause far from the source caixa.lisp.
        // Peer to
        // [`cluster_bundle_every_flux_cr_carries_metadata_namespace_label_from_lifted_key`]
        // (743e5cd) on the sibling `namespace:` half of the
        // ObjectMeta / sourceRef / healthCheck `(name, namespace)`
        // identity-pair — extends the same production-side raw-byte-
        // label-sweep discipline onto the paired `metadata.name`
        // nested axis every rendered Flux v2 bundle document
        // navigates, so every `name:` label position across the whole
        // `cluster_bundle` render surface consumes the same
        // substrate-owned `&'static str` by construction.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let label_prefix = format!("{KUBE_KEY_NAME}:");
        for filename in [
            FLUX_GITREPOSITORY_YAML_FILENAME,
            FLUX_HELMRELEASE_YAML_FILENAME,
            FLUX_KUSTOMIZATION_YAML_FILENAME,
        ] {
            let f = files
                .iter()
                .find(|f| f.path == std::path::PathBuf::from(filename))
                .unwrap_or_else(|| panic!("{filename} present"));
            assert!(
                f.contents.contains(&label_prefix),
                "{filename} must carry the {label_prefix:?} YAML label \
                 composed from the lifted KUBE_KEY_NAME ({KUBE_KEY_NAME:?}); \
                 a drifted inline label here silently rebrands the per-CR \
                 metadata.name axis away from the lifted key \
                 (got: {contents:?})",
                contents = f.contents,
            );
        }
    }

    #[test]
    fn flux_kind_git_repository_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_KIND_GIT_REPOSITORY` is
        // the single source of truth for the Flux v2 `GitRepository` CRD
        // `kind` discriminator the rendered Flux bundle's three
        // `GitRepository`-naming axes declare (the `gitrepository.yaml`
        // top-level `kind`, the `helmrelease.yaml`
        // `spec.chart.spec.sourceRef.kind`, and the `kustomization.yaml`
        // `spec.sourceRef.kind`). Pin the equality (and the static-data
        // identity, peer with the sibling
        // [`flux_gitrepository_api_version_re_export_points_at_caixa_core_canonical`]
        // / [`flux_kustomization_api_version_re_export_points_at_caixa_core_canonical`]
        // pins) so any local re-introduction of a sibling `pub const
        // FLUX_KIND_GIT_REPOSITORY: &str = "…"` (the canonical drift
        // footgun this lift closes — three production-code consumers of
        // the load-bearing Flux v2 `GitRepository` CRD `kind`
        // discriminator inside the cluster_bundle templates, lifted to one
        // re-export at the caixa-core boundary) is a build-time test
        // failure naming the offending drift, not a silent apply-time
        // `helm-controller` chart-resolution dangle.
        caixa_core::assert_str_reexport_identity(
            "FLUX_KIND_GIT_REPOSITORY",
            FLUX_KIND_GIT_REPOSITORY,
            caixa_core::FLUX_KIND_GIT_REPOSITORY,
        );
    }

    #[test]
    fn cluster_bundle_gitrepository_kind_uses_lifted_flux_kind_git_repository() {
        // Fail-before-pass-after pin: the rendered `gitrepository.yaml`
        // top-level `kind` axis — the load-bearing K8s CRD discriminator
        // the Flux v2 `source-controller` resolves the rendered document
        // against — must resolve to the lifted
        // [`FLUX_KIND_GIT_REPOSITORY`] verbatim. Before this lift the
        // gitrepository template carried an inline `GitRepository`
        // literal at one of three sibling production-code call sites
        // across the cluster_bundle templates; the apiserver-side CRD
        // resolution contract is the `(apiVersion, kind)` tuple keyed
        // against the registered `CustomResourceDefinition`, so drift on
        // the kind axis is exactly as load-bearing as drift on the
        // sibling [`FLUX_GITREPOSITORY_API_VERSION`] axis (a future Flux
        // v3 rebrand on this axis without a coordinated edit on the
        // sibling sourceRef.kind axes silently lands the rendered
        // `GitRepository` outside the source-controller's `Watches`).
        // Peer with [`cluster_bundle_gitrepository_uses_lifted_flux_api_version`]
        // on the sibling apiVersion half of the same CRD-lookup tuple.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let gr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_GITREPOSITORY_YAML_FILENAME))
            .expect("gitrepository.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&gr.contents).expect("gitrepository.yaml parses as YAML");
        assert_eq!(
            kube_root_str_field(&parsed, KUBE_KEY_KIND),
            Some(FLUX_KIND_GIT_REPOSITORY),
            "gitrepository.yaml top-level kind must spell the lifted \
             FLUX_KIND_GIT_REPOSITORY ({FLUX_KIND_GIT_REPOSITORY:?}); a drifted \
             literal here routes the GitRepository outside the Flux v2 \
             source-controller's CRD registration",
        );
    }

    #[test]
    fn cluster_bundle_helmrelease_source_ref_kind_uses_lifted_flux_kind_git_repository() {
        // Sibling-axis pin to
        // `cluster_bundle_gitrepository_kind_uses_lifted_flux_kind_git_repository`:
        // the rendered `helmrelease.yaml`'s `spec.chart.spec.sourceRef.kind`
        // axis is the Flux v2 contract pairing the HelmRelease's chart-
        // source resolution to the sibling GitRepository's CRD
        // discriminator. Both axes must resolve to the same lifted
        // constant by value — a future Flux v3 rebrand on either axis
        // without a coordinated edit on the other would have silently
        // dangled the HelmRelease's chart sourceRef (apply-side: the
        // `helm-controller` never resolves a chart for the HelmRelease,
        // the rendered Servico chart never reconciles), with the failure
        // surfacing far from the rebrand commit's source at
        // `kubectl describe helmrelease` time. This is the one of the
        // three call sites whose drift the apiserver can't self-locate
        // (top-level kind typos surface as "no kind 'X' is registered"
        // at apply parse time; a nested sourceRef.kind typo silently
        // dangles a controller-side reference).
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME))
            .expect("helmrelease.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&hr.contents).expect("helmrelease.yaml parses as YAML");
        let source_ref_kind = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_KEY_CHART))
            .and_then(|c| c.get(KUBE_KEY_SPEC))
            .and_then(|s| s.get(FLUX_KEY_SOURCE_REF))
            .and_then(|r| r.get(KUBE_KEY_KIND))
            .and_then(|k| k.as_str())
            .expect("helmrelease.yaml spec.chart.spec.sourceRef.kind present");
        assert_eq!(
            source_ref_kind, FLUX_KIND_GIT_REPOSITORY,
            "helmrelease.yaml spec.chart.spec.sourceRef.kind must spell the \
             lifted FLUX_KIND_GIT_REPOSITORY ({FLUX_KIND_GIT_REPOSITORY:?}); a \
             drifted literal here dangles the HelmRelease's chart sourceRef \
             at the Flux v2 source-controller's CRD registration",
        );
    }

    #[test]
    fn cluster_bundle_kustomization_source_ref_kind_uses_lifted_flux_kind_git_repository() {
        // Sibling-axis pin to
        // `cluster_bundle_gitrepository_kind_uses_lifted_flux_kind_git_repository`
        // / `cluster_bundle_helmrelease_source_ref_kind_uses_lifted_flux_kind_git_repository`:
        // the rendered `kustomization.yaml`'s `spec.sourceRef.kind` axis is
        // the Flux v2 contract pairing the parent Kustomization's source
        // resolution to the cluster's bootstrap GitRepository's CRD
        // discriminator (paired with [`DEFAULT_FLUX_SYSTEM_NAMESPACE`] on
        // the namespace axis). Completes the three-axis pin set so any
        // local re-introduction of an inline `GitRepository` literal at
        // any one of the three rendered Flux bundle axes is a build-time
        // test failure, not a silent apply-time dangling-sourceRef
        // reconciliation freeze.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let kz = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
            .expect("kustomization.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&kz.contents).expect("kustomization.yaml parses as YAML");
        let source_ref_kind = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_KEY_SOURCE_REF))
            .and_then(|r| r.get(KUBE_KEY_KIND))
            .and_then(|k| k.as_str())
            .expect("kustomization.yaml spec.sourceRef.kind present");
        assert_eq!(
            source_ref_kind, FLUX_KIND_GIT_REPOSITORY,
            "kustomization.yaml spec.sourceRef.kind must spell the lifted \
             FLUX_KIND_GIT_REPOSITORY ({FLUX_KIND_GIT_REPOSITORY:?}); a drifted \
             literal here dangles the parent Kustomization's sourceRef at \
             the Flux v2 source-controller's CRD registration",
        );
    }

    #[test]
    fn cluster_bundle_three_git_repository_kind_axes_share_one_lifted_constant() {
        // Cross-axis triplet invariant: the three rendered Flux bundle
        // axes that name the Flux v2 `GitRepository` CRD discriminator
        // (gitrepository.yaml top-level kind, helmrelease.yaml
        // spec.chart.spec.sourceRef.kind, kustomization.yaml
        // spec.sourceRef.kind) all consult one lifted `&'static str`.
        // The apiserver-side CRD resolution contract is the
        // `(apiVersion, kind)` tuple keyed against the registered
        // `CustomResourceDefinition`; the three axes must move
        // together on any future Flux v3 CRD rename (e.g. `GitSource`)
        // for the rendered HelmRelease's chart sourceRef + the parent
        // Kustomization's sourceRef to bind to the sibling
        // GitRepository's renamed kind discriminator. Peer to the
        // sibling [`flux_controller_triplet_api_versions_share_toolkit_fluxcd_io_root`]
        // cross-axis pin on the apiVersion half of the same CRD-lookup
        // tuple.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();

        let gr_kind = serde_yaml::from_str::<serde_yaml::Value>(
            &files
                .iter()
                .find(|f| f.path == std::path::PathBuf::from(FLUX_GITREPOSITORY_YAML_FILENAME))
                .unwrap()
                .contents,
        )
        .unwrap()
        .get(KUBE_KEY_KIND)
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap();

        let hr_source_kind = serde_yaml::from_str::<serde_yaml::Value>(
            &files
                .iter()
                .find(|f| f.path == std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME))
                .unwrap()
                .contents,
        )
        .unwrap()
        .get(KUBE_KEY_SPEC)
        .and_then(|s| s.get(FLUX_KEY_CHART))
        .and_then(|c| c.get(KUBE_KEY_SPEC))
        .and_then(|s| s.get(FLUX_KEY_SOURCE_REF))
        .and_then(|r| r.get(KUBE_KEY_KIND))
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap();

        let kz_source_kind = serde_yaml::from_str::<serde_yaml::Value>(
            &files
                .iter()
                .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
                .unwrap()
                .contents,
        )
        .unwrap()
        .get(KUBE_KEY_SPEC)
        .and_then(|s| s.get(FLUX_KEY_SOURCE_REF))
        .and_then(|r| r.get(KUBE_KEY_KIND))
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap();

        assert_eq!(gr_kind, FLUX_KIND_GIT_REPOSITORY);
        assert_eq!(hr_source_kind, FLUX_KIND_GIT_REPOSITORY);
        assert_eq!(kz_source_kind, FLUX_KIND_GIT_REPOSITORY);
        assert_eq!(
            gr_kind, hr_source_kind,
            "gitrepository.yaml top-level kind and helmrelease.yaml \
             spec.chart.spec.sourceRef.kind must spell the same lifted \
             constant — drift here dangles the HelmRelease's chart sourceRef"
        );
        assert_eq!(
            gr_kind, kz_source_kind,
            "gitrepository.yaml top-level kind and kustomization.yaml \
             spec.sourceRef.kind must spell the same lifted constant — \
             drift here dangles the parent Kustomization's sourceRef"
        );
    }

    #[test]
    fn flux_kind_helm_release_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_KIND_HELM_RELEASE` is
        // the single source of truth for the Flux v2 `HelmRelease` CRD
        // `kind` discriminator the rendered Flux bundle's two
        // `HelmRelease`-naming axes declare (the `helmrelease.yaml`
        // top-level `kind`, the `kustomization.yaml`
        // `spec.healthChecks[].kind`). Pin the equality (and the
        // static-data identity, peer with the sibling
        // [`flux_kind_git_repository_re_export_points_at_caixa_core_canonical`]
        // pin) so any local re-introduction of a sibling `pub const
        // FLUX_KIND_HELM_RELEASE: &str = "…"` (the canonical drift
        // footgun this lift closes — two production-code consumers of
        // the load-bearing Flux v2 `HelmRelease` CRD `kind`
        // discriminator inside the cluster_bundle templates, lifted to
        // one re-export at the caixa-core boundary) is a build-time
        // test failure naming the offending drift, not a silent
        // apply-time `helm-controller` resolution dangle or a
        // perpetually-`Reconciling` parent Kustomization.
        caixa_core::assert_str_reexport_identity(
            "FLUX_KIND_HELM_RELEASE",
            FLUX_KIND_HELM_RELEASE,
            caixa_core::FLUX_KIND_HELM_RELEASE,
        );
    }

    #[test]
    fn cluster_bundle_helmrelease_kind_uses_lifted_flux_kind_helm_release() {
        // Fail-before-pass-after pin: the rendered `helmrelease.yaml`
        // top-level `kind` axis — the load-bearing K8s CRD discriminator
        // the Flux v2 `helm-controller` resolves the rendered document
        // against — must resolve to the lifted
        // [`FLUX_KIND_HELM_RELEASE`] verbatim. Before this lift the
        // helmrelease template carried an inline `HelmRelease` literal
        // at one of two sibling production-code call sites across the
        // cluster_bundle templates; the apiserver-side CRD resolution
        // contract is the `(apiVersion, kind)` tuple keyed against the
        // registered `CustomResourceDefinition`, so drift on the kind
        // axis is exactly as load-bearing as drift on the sibling
        // [`FLUX_HELMRELEASE_API_VERSION`] axis (a future Flux v3
        // rebrand on this axis without a coordinated edit on the
        // sibling healthChecks[].kind axis silently lands the rendered
        // `HelmRelease` outside the helm-controller's `Watches`).
        // Peer with [`cluster_bundle_helmrelease_uses_lifted_flux_api_version`]
        // on the sibling apiVersion half of the same CRD-lookup tuple,
        // and with
        // [`cluster_bundle_gitrepository_kind_uses_lifted_flux_kind_git_repository`]
        // on the sibling Flux-v2 source-controller CRD kind axis.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME))
            .expect("helmrelease.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&hr.contents).expect("helmrelease.yaml parses as YAML");
        assert_eq!(
            kube_root_str_field(&parsed, KUBE_KEY_KIND),
            Some(FLUX_KIND_HELM_RELEASE),
            "helmrelease.yaml top-level kind must spell the lifted \
             FLUX_KIND_HELM_RELEASE ({FLUX_KIND_HELM_RELEASE:?}); a drifted \
             literal here routes the HelmRelease outside the Flux v2 \
             helm-controller's CRD registration",
        );
    }

    #[test]
    fn cluster_bundle_kustomization_health_check_kind_uses_lifted_flux_kind_helm_release() {
        // Sibling-axis pin to
        // `cluster_bundle_helmrelease_kind_uses_lifted_flux_kind_helm_release`:
        // the rendered `kustomization.yaml`'s
        // `spec.healthChecks[].kind` axis is the Flux v2 contract
        // pairing the parent Kustomization's per-resource health gate
        // to the sibling HelmRelease's CRD discriminator. Both axes
        // must resolve to the same lifted constant by value — a
        // future Flux v3 rebrand on either axis without a coordinated
        // edit on the other would have silently pinned the parent
        // Kustomization at `Reconciling` forever (apply-side: the
        // `kustomize-controller` perpetually re-evaluates an
        // unmatched health gate, the Kustomization never declares
        // its reconcile complete, every downstream `dependsOn` chain
        // freezes), with the failure surfacing far from the rebrand
        // commit's source at `kubectl describe kustomization` time.
        // This is one of the two call sites whose drift the
        // apiserver can't self-locate (top-level kind typos surface
        // as "no kind 'X' is registered" at apply parse time; a
        // healthChecks[].kind typo silently dangles a controller-side
        // health gate). Completes the two-axis pin set so any local
        // re-introduction of an inline `HelmRelease` literal at
        // either of the two rendered Flux bundle axes is a
        // build-time test failure, not a silent apply-time stuck-
        // Reconciling reconciliation freeze.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let kz = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
            .expect("kustomization.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&kz.contents).expect("kustomization.yaml parses as YAML");
        let health_checks = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_KEY_HEALTH_CHECKS))
            .and_then(|h| h.as_sequence())
            .expect("kustomization.yaml spec.healthChecks present");
        assert!(
            !health_checks.is_empty(),
            "kustomization.yaml spec.healthChecks must carry at least one \
             entry — the HelmRelease health gate is the canonical pleme-io \
             Flux bundle invariant"
        );
        let health_kind = health_checks[0]
            .get(KUBE_KEY_KIND)
            .and_then(|k| k.as_str())
            .expect("kustomization.yaml spec.healthChecks[0].kind present");
        assert_eq!(
            health_kind, FLUX_KIND_HELM_RELEASE,
            "kustomization.yaml spec.healthChecks[0].kind must spell the \
             lifted FLUX_KIND_HELM_RELEASE ({FLUX_KIND_HELM_RELEASE:?}); a \
             drifted literal here dangles the parent Kustomization at \
             `Reconciling` forever at the Flux v2 kustomize-controller's \
             health-gate evaluation",
        );
    }

    #[test]
    fn cluster_bundle_two_helm_release_kind_axes_share_one_lifted_constant() {
        // Cross-axis pair invariant: the two rendered Flux bundle axes
        // that name the Flux v2 `HelmRelease` CRD discriminator
        // (helmrelease.yaml top-level kind, kustomization.yaml
        // spec.healthChecks[].kind) both consult one lifted
        // `&'static str`. The apiserver-side CRD resolution contract
        // is the `(apiVersion, kind)` tuple keyed against the
        // registered `CustomResourceDefinition`; the two axes must
        // move together on any future Flux v3 CRD rename (e.g.
        // `ChartRelease`) for the parent Kustomization's health gate
        // to bind to the sibling HelmRelease's renamed kind
        // discriminator. Peer to the sibling
        // [`cluster_bundle_three_git_repository_kind_axes_share_one_lifted_constant`]
        // cross-axis pin on the Flux v2 source-controller CRD kind
        // axis.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();

        let hr_kind = serde_yaml::from_str::<serde_yaml::Value>(
            &files
                .iter()
                .find(|f| f.path == std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME))
                .unwrap()
                .contents,
        )
        .unwrap()
        .get(KUBE_KEY_KIND)
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap();

        let kz_health_kind = serde_yaml::from_str::<serde_yaml::Value>(
            &files
                .iter()
                .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
                .unwrap()
                .contents,
        )
        .unwrap()
        .get(KUBE_KEY_SPEC)
        .and_then(|s| s.get(FLUX_KEY_HEALTH_CHECKS))
        .and_then(|h| h.as_sequence())
        .and_then(|seq| seq.first())
        .and_then(|e| e.get(KUBE_KEY_KIND))
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap();

        assert_eq!(hr_kind, FLUX_KIND_HELM_RELEASE);
        assert_eq!(kz_health_kind, FLUX_KIND_HELM_RELEASE);
        assert_eq!(
            hr_kind, kz_health_kind,
            "helmrelease.yaml top-level kind and kustomization.yaml \
             spec.healthChecks[].kind must spell the same lifted constant \
             — drift here dangles the parent Kustomization's health gate \
             at the Flux v2 kustomize-controller"
        );
    }

    #[test]
    fn flux_kind_kustomization_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_KIND_KUSTOMIZATION` is
        // the single source of truth for the Flux v2 `Kustomization` CRD
        // `kind` discriminator the rendered Flux bundle's
        // `Kustomization`-naming axis declares (the `kustomization.yaml`
        // top-level `kind`). Pin the equality (and the static-data
        // identity, peer with the sibling
        // [`flux_kind_git_repository_re_export_points_at_caixa_core_canonical`]
        // /
        // [`flux_kind_helm_release_re_export_points_at_caixa_core_canonical`]
        // pins) so any local re-introduction of a sibling
        // `pub const FLUX_KIND_KUSTOMIZATION: &str = "…"` (the canonical
        // drift footgun this lift closes — the load-bearing Flux v2
        // `Kustomization` CRD `kind` discriminator inside the
        // cluster_bundle template, lifted to one re-export at the
        // caixa-core boundary) is a build-time test failure naming the
        // offending drift, not a silent apply-time
        // `kustomize-controller` CRD-lookup miss that perpetually
        // freezes the rendered parent Kustomization and every
        // downstream per-Servico `dependsOn` chain.
        caixa_core::assert_str_reexport_identity(
            "FLUX_KIND_KUSTOMIZATION",
            FLUX_KIND_KUSTOMIZATION,
            caixa_core::FLUX_KIND_KUSTOMIZATION,
        );
    }

    #[test]
    fn flux_key_source_ref_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_KEY_SOURCE_REF` is the
        // single source of truth for the Flux v2 per-`HelmRelease` /
        // `Kustomization` source-reference container-axis key the
        // rendered bundle documents mount their per-CR `(kind, name,
        // namespace)` reference triple under. Pin the equality (and the
        // static-data identity, peer with the sibling
        // [`flux_kind_git_repository_re_export_points_at_caixa_core_canonical`]
        // / [`flux_kind_helm_release_re_export_points_at_caixa_core_canonical`]
        // / [`flux_kind_kustomization_re_export_points_at_caixa_core_canonical`]
        // pins on the sibling per-CRD kind-discriminator surface) so any
        // local re-introduction of a sibling `pub const
        // FLUX_KEY_SOURCE_REF: &str = "…"` (the canonical drift footgun
        // where a sibling local `pub const` could happen to carry the
        // same string at the source while pointing at a different
        // `&'static` allocation) is a build-time test failure naming
        // the offending drift, not a silent apply-time dangling-
        // sourceRef reconciliation freeze (a rebrand on this axis
        // without a coordinated caixa-core edit silently dangles both
        // the `HelmRelease.spec.chart.spec.sourceRef` chart resolution
        // + the parent `Kustomization.spec.sourceRef` source resolution
        // at the Flux v2 source-controller's CRD registration; the
        // dependent per-Servico `dependsOn` chain freezes at apply time
        // with no field naming the container-axis-drift root cause).
        // Closes the sibling re-export identity axis on the same
        // trajectory the peer per-CRD-`kind`-discriminator pins carry.
        caixa_core::assert_str_reexport_identity(
            "FLUX_KEY_SOURCE_REF",
            FLUX_KEY_SOURCE_REF,
            caixa_core::FLUX_KEY_SOURCE_REF,
        );
    }

    #[test]
    fn cluster_bundle_helmrelease_spec_chart_spec_source_ref_key_pins_lifted_flux_key_source_ref() {
        // Production-emit pin traversing a rendered `helmrelease.yaml`
        // document via parsed YAML: the `spec.chart.spec.sourceRef:\n`
        // sub-block header key baked into the [`cluster_bundle`]
        // `helmrelease.yaml` format-string template — the container-axis
        // key nesting the Flux v2 `HelmChartTemplate`'s per-CR
        // source-of-truth `(kind, name, namespace)` reference triple —
        // must resolve at the lifted [`FLUX_KEY_SOURCE_REF`] verbatim
        // byte-value. Before this sweep the site inlined `sourceRef:\n`
        // as a literal beside its sibling lifted `{chart_key}:` /
        // `{values_key}:` axes; a caixa-core rebrand of the const would
        // silently drift the probe path (`.get(FLUX_KEY_SOURCE_REF)`)
        // away from the emit path (baked `sourceRef:\n`), and a future
        // Flux v3 rename would land in the const while the emit-side
        // format-string template silently kept the old byte sequence.
        // The sweep threads the const through a `{source_ref_key}`
        // named-arg interpolation so both paths consult one
        // `&'static str`; this pin traverses the rendered document at
        // the lifted-const-keyed navigation and asserts a populated
        // sub-mapping (the `(kind, name, namespace)` triple) resolves
        // there, verifying that the emit path spells the exact const
        // byte-value. A regression that re-introduces an inline literal
        // in the format-string template surfaces as a `None` at the
        // lifted-const-keyed lookup. Peer to the sibling
        // [`cluster_bundle_kustomization_spec_source_ref_key_pins_lifted_flux_key_source_ref`]
        // pin closing the second production emit site on the same
        // container-axis, and to the sibling
        // [`cluster_bundle_helmrelease_values_block_uses_lifted_flux_key_values`]
        // pin on the sibling per-`HelmRelease` body-key surface.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME))
            .expect("helmrelease.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&hr.contents).expect("helmrelease.yaml parses as YAML");
        let source_ref = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_KEY_CHART))
            .and_then(|c| c.get(KUBE_KEY_SPEC))
            .and_then(|s| s.get(FLUX_KEY_SOURCE_REF))
            .and_then(|r| r.as_mapping())
            .expect("spec.chart.spec.<FLUX_KEY_SOURCE_REF> mapping present");
        assert!(
            !source_ref.is_empty(),
            "spec.chart.spec.<FLUX_KEY_SOURCE_REF> ({FLUX_KEY_SOURCE_REF:?}) \
             must carry the `(kind, name, namespace)` reference triple; drift \
             on this container-axis key silently dangles the HelmRelease's \
             chart resolution at the Flux v2 source-controller's CRD \
             registration"
        );
    }

    #[test]
    fn cluster_bundle_kustomization_spec_source_ref_key_pins_lifted_flux_key_source_ref() {
        // Production-emit pin traversing a rendered `kustomization.yaml`
        // document via parsed YAML: the `spec.sourceRef:\n` sub-block
        // header key baked into the [`cluster_bundle`]
        // `kustomization.yaml` format-string template — the
        // container-axis key nesting the parent `Kustomization`'s
        // source-of-truth `(kind, name)` reference pair pointing back at
        // the cluster's bootstrap `GitRepository` — must resolve at the
        // lifted [`FLUX_KEY_SOURCE_REF`] verbatim byte-value. Before
        // this sweep the site inlined `sourceRef:\n` as a literal
        // beside its sibling lifted `{interval_key}:` /
        // `{health_checks_key}:` axes; a caixa-core rebrand of the
        // const would silently drift the probe path away from the emit
        // path, and a future Flux v3 rename would land in the const
        // while this format-string template silently kept the old byte
        // sequence. The sweep threads the const through a
        // `{source_ref_key}` named-arg interpolation so both paths
        // consult one `&'static str`; this pin traverses the rendered
        // document at the lifted-const-keyed navigation and asserts a
        // populated sub-mapping resolves there. A regression that
        // re-introduces an inline literal surfaces as a `None` at the
        // lifted-const-keyed lookup. Peer to the sibling
        // [`cluster_bundle_helmrelease_spec_chart_spec_source_ref_key_pins_lifted_flux_key_source_ref`]
        // pin closing the first production emit site on the same
        // container-axis, together closing the two-site production
        // emit sweep the peer [`FLUX_KEY_SOURCE_REF`] doc block calls
        // out.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let kz = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
            .expect("kustomization.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&kz.contents).expect("kustomization.yaml parses as YAML");
        let source_ref = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_KEY_SOURCE_REF))
            .and_then(|r| r.as_mapping())
            .expect("spec.<FLUX_KEY_SOURCE_REF> mapping present");
        assert!(
            !source_ref.is_empty(),
            "spec.<FLUX_KEY_SOURCE_REF> ({FLUX_KEY_SOURCE_REF:?}) must \
             carry the `(kind, name)` reference pair pointing at the \
             cluster's bootstrap GitRepository; drift on this \
             container-axis key silently dangles the parent \
             Kustomization's source resolution at the Flux v2 \
             source-controller's CRD registration"
        );
    }

    #[test]
    fn flux_key_values_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_KEY_VALUES` is the
        // single source of truth for the Flux v2 per-`HelmRelease`
        // values-override block-body-axis key the rendered
        // `helmrelease.yaml` nests its per-cluster override YAML under.
        // Pin the equality (and the static-data identity, peer with the
        // sibling
        // [`flux_key_source_ref_re_export_points_at_caixa_core_canonical`]
        // pin on the sibling Flux v2 per-CR container-axis-key surface)
        // so any local re-introduction of a sibling `pub const
        // FLUX_KEY_VALUES: &str = "…"` (the canonical drift footgun
        // where a sibling local `pub const` could happen to carry the
        // same string at the source while pointing at a different
        // `&'static` allocation) is a build-time test failure naming
        // the offending drift, not a silent apply-time per-cluster-
        // override-routed-nowhere reconciliation (a rebrand on this
        // axis without a coordinated caixa-core edit silently routes
        // the per-cluster override YAML nowhere at Helm-render time;
        // the workload silently comes up with the referenced chart's
        // admission-time defaults, far from the source `caixa.lisp` /
        // the renderer's format-string template). Closes the sibling
        // re-export identity axis on the same trajectory the peer
        // [`flux_key_source_ref_re_export_points_at_caixa_core_canonical`]
        // pin carries.
        caixa_core::assert_str_reexport_identity(
            "FLUX_KEY_VALUES",
            FLUX_KEY_VALUES,
            caixa_core::FLUX_KEY_VALUES,
        );
    }

    #[test]
    fn cluster_bundle_helmrelease_values_block_uses_lifted_flux_key_values() {
        // Fail-before-pass-after pin: the rendered `helmrelease.yaml`'s
        // top-level `spec.values` block-body-axis key — the scope under
        // which per-cluster overrides reach the referenced chart at
        // Flux v2 `helm-controller` reconcile time — must resolve to the
        // lifted [`FLUX_KEY_VALUES`] verbatim. Before the lift this
        // site carried an inline `values:\n` literal in the format
        // string; a future Flux v3 rebrand on this axis (a hypothetical
        // upstream fluxcd/flux2 rename from `values` to `Values` /
        // `chartValues` / `overrides`, coordinated with the upstream
        // project's per-version deprecation cycle) without a
        // coordinated edit here would have silently routed the per-
        // cluster override YAML nowhere at `helm-controller` reconcile
        // time — the workload's per-cluster overrides never reach the
        // referenced chart, and the apply comes up with the chart's
        // admission-time defaults far from the rebrand commit's source.
        //
        // The pin is structural: parse the rendered YAML and assert the
        // per-`HelmRelease` values-override block-body-axis key resolves
        // under the lifted constant (not `.get("values")` — that is the
        // sibling literal-shape probe the peer test at line 1955 pins;
        // this test asserts the lifted-const-keyed navigation carries a
        // populated mapping under it). A regression that re-introduces
        // an inline literal in the format-string template surfaces as a
        // `None` at the lifted-const-keyed lookup (the inline literal
        // would survive, but the lifted-const-keyed assertion would
        // fail). Peer to the sibling
        // [`cluster_bundle_helmrelease_values_wrap_key_uses_lifted_constant`]
        // pin on the sibling per-`HelmRelease` inner-wrap-key surface
        // — extends the canonical-Flux-v2-per-`HelmRelease`-values-
        // navigation lift from the wrap-key axis onto the outer block-
        // body-axis this test targets.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME))
            .expect("helmrelease.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&hr.contents).expect("helmrelease.yaml parses as YAML");
        let values = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_KEY_VALUES))
            .and_then(|v| v.as_mapping())
            .expect("spec.<FLUX_KEY_VALUES> mapping present");
        assert!(
            !values.is_empty(),
            "spec.<FLUX_KEY_VALUES> ({FLUX_KEY_VALUES:?}) must carry \
             at least the lifted-[`DEFAULT_LIBRARY_NAME`]-wrapped per-\
             cluster override block; drift on this block-body-axis key \
             silently routes overrides nowhere at helm-controller \
             reconcile time"
        );
    }

    #[test]
    fn cluster_bundle_kustomization_kind_uses_lifted_flux_kind_kustomization() {
        // Fail-before-pass-after pin: the rendered `kustomization.yaml`
        // top-level `kind` axis — the load-bearing K8s CRD discriminator
        // the Flux v2 `kustomize-controller` resolves the rendered
        // document against — must resolve to the lifted
        // [`FLUX_KIND_KUSTOMIZATION`] verbatim. Before this lift the
        // kustomization template carried an inline `Kustomization`
        // literal; the apiserver-side CRD resolution contract is the
        // `(apiVersion, kind)` tuple keyed against the registered
        // `CustomResourceDefinition`, so drift on the kind axis is
        // exactly as load-bearing as drift on the sibling
        // [`FLUX_KUSTOMIZATION_API_VERSION`] axis (a future Flux v3
        // rebrand on this axis without a coordinated edit on the
        // sibling apiVersion axis silently lands the rendered
        // `Kustomization` outside the kustomize-controller's `Watches`
        // and surfaces at apply parse time as a non-self-locating
        // "no kind 'Kustomization' is registered" error far from the
        // rebrand commit's source). Peer with
        // [`cluster_bundle_kustomization_uses_lifted_flux_kustomization_api_version`]
        // on the sibling apiVersion half of the same CRD-lookup tuple,
        // and with
        // [`cluster_bundle_helmrelease_kind_uses_lifted_flux_kind_helm_release`]
        // /
        // [`cluster_bundle_gitrepository_kind_uses_lifted_flux_kind_git_repository`]
        // on the sibling Flux v2 controller-triplet CRD kind axes.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let kz = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
            .expect("kustomization.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&kz.contents).expect("kustomization.yaml parses as YAML");
        assert_eq!(
            kube_root_str_field(&parsed, KUBE_KEY_KIND),
            Some(FLUX_KIND_KUSTOMIZATION),
            "kustomization.yaml top-level kind must spell the lifted \
             FLUX_KIND_KUSTOMIZATION ({FLUX_KIND_KUSTOMIZATION:?}); a drifted \
             literal here routes the Kustomization outside the Flux v2 \
             kustomize-controller's CRD registration",
        );
    }

    #[test]
    fn flux_key_chart_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_KEY_CHART` is the
        // single source of truth for the Flux v2 per-`HelmRelease`
        // chart-template container-axis key the rendered
        // `helmrelease.yaml` nests its per-CR `HelmChartTemplate`
        // sub-document under. Pin the equality (and the static-data
        // identity, peer with the sibling
        // [`flux_key_source_ref_re_export_points_at_caixa_core_canonical`]
        // / [`flux_key_values_re_export_points_at_caixa_core_canonical`]
        // pins on the sibling Flux v2 per-`HelmRelease` body-key
        // surfaces) so any local re-introduction of a sibling `pub
        // const FLUX_KEY_CHART: &str = "…"` (the canonical drift
        // footgun where a sibling local `pub const` could happen to
        // carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test
        // failure naming the offending drift, not a silent apply-
        // time dangling-chart-template reconciliation freeze (a
        // rebrand on this axis without a coordinated caixa-core edit
        // silently dangles the `HelmRelease.spec.chart` chart-
        // template resolution at the Flux v2 helm-controller's CRD
        // registration; the referenced chart never resolves and the
        // per-Servico workload freezes at apply time with no field
        // naming the container-axis-drift root cause). Closes the
        // sibling re-export identity axis on the same trajectory the
        // peer per-CR body-key pins carry.
        caixa_core::assert_str_reexport_identity(
            "FLUX_KEY_CHART",
            FLUX_KEY_CHART,
            caixa_core::FLUX_KEY_CHART,
        );
    }

    #[test]
    fn cluster_bundle_helmrelease_chart_block_uses_lifted_flux_key_chart() {
        // Fail-before-pass-after pin: the rendered `helmrelease.yaml`'s
        // top-level `spec.chart` container-axis key — the scope under
        // which the Flux v2 `HelmChartTemplate` sub-document lives (the
        // `HelmChartTemplate.spec` block that carries the chart-name
        // leaf, source-of-truth reference triple, and per-CR reconcile
        // cadence the Flux v2 `helm-controller`'s per-CR reconcile loop
        // reads to source the referenced chart at Helm-render time) —
        // must resolve to the lifted [`FLUX_KEY_CHART`] verbatim.
        // Before the lift this site carried an inline `chart:\n`
        // literal in the format string; a future Flux v3 rebrand on
        // this axis (a hypothetical upstream fluxcd/flux2 rename from
        // `chart` to `Chart` / `chartTemplate` / `helmChart` /
        // `chartRef`, coordinated with the upstream project's per-
        // version deprecation cycle) without a coordinated edit here
        // would have silently dangled the whole chart-template block
        // at `helm-controller` reconcile time — the workload's
        // referenced chart never resolves, and the apply freezes at
        // the CRD-registration boundary far from the rebrand commit's
        // source.
        //
        // The pin is structural: parse the rendered YAML and assert
        // the per-`HelmRelease` chart-template container-axis key
        // resolves under the lifted constant with a populated
        // `HelmChartTemplate.spec` sub-mapping under it. A regression
        // that re-introduces an inline literal in the format-string
        // template surfaces as a `None` at the lifted-const-keyed
        // lookup. Peer to the sibling
        // [`cluster_bundle_helmrelease_values_block_uses_lifted_flux_key_values`]
        // pin on the sibling per-`HelmRelease` values-override block-
        // body-axis surface — extends the canonical-Flux-v2-per-
        // `HelmRelease`-body-key lifted-const-keyed pin discipline
        // from the values-override block-body-axis onto the sibling
        // chart-template container-axis this test targets.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME))
            .expect("helmrelease.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&hr.contents).expect("helmrelease.yaml parses as YAML");
        let chart = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_KEY_CHART))
            .and_then(|v| v.as_mapping())
            .expect("spec.<FLUX_KEY_CHART> mapping present");
        assert!(
            !chart.is_empty(),
            "spec.<FLUX_KEY_CHART> ({FLUX_KEY_CHART:?}) must carry \
             at least the nested `HelmChartTemplate.spec` sub-document; \
             drift on this container-axis key silently dangles the \
             whole chart-template block at helm-controller reconcile time"
        );
    }

    #[test]
    fn flux_helmchart_template_key_chart_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_HELMCHART_TEMPLATE_KEY_CHART`
        // is the single source of truth for the Flux v2 per-`HelmChartTemplate`
        // chart-NAME reference leaf-scalar-axis key the rendered
        // `helmrelease.yaml`'s `spec.chart.spec.chart` leaf-scalar-
        // valued field carries the chart-artifact name at the
        // helm-controller's per-CR reconcile-time chart-lookup axis.
        // Pin the equality (and the static-data identity, peer with
        // the sibling [`flux_key_chart_re_export_points_at_caixa_core_canonical`]
        // pin on the parent container-axis re-export the leaf-scalar-
        // axis nests inside) so any local re-introduction of a sibling
        // `pub const FLUX_HELMCHART_TEMPLATE_KEY_CHART: &str = "…"`
        // (the canonical drift footgun this lift closes — the one
        // production-code call site the `cluster_bundle`
        // `helmrelease.yaml` format-string template threaded the
        // chart-NAME leaf-scalar-key through, lifted to one re-export
        // at the caixa-core boundary) is a build-time test failure
        // naming the offending drift, not a silent apply-time
        // "chart 'unknown' not found in <source>" reconciliation
        // dangle the CRD's OpenAPI extra-property schema permits at
        // the apiserver.
        caixa_core::assert_str_reexport_identity(
            "FLUX_HELMCHART_TEMPLATE_KEY_CHART",
            FLUX_HELMCHART_TEMPLATE_KEY_CHART,
            caixa_core::FLUX_HELMCHART_TEMPLATE_KEY_CHART,
        );
    }

    #[test]
    fn cluster_bundle_helmrelease_chart_name_leaf_uses_lifted_flux_helmchart_template_key_chart() {
        // Fail-before-pass-after pin: the rendered `helmrelease.yaml`'s
        // per-`HelmChartTemplate.spec.chart` chart-NAME reference
        // leaf-scalar-valued field — the load-bearing chart-lookup
        // scalar the Flux v2 helm-controller resolves the referenced
        // chart artifact through the sibling `sourceRef` triple's
        // source at reconcile time by — must resolve under the lifted
        // [`FLUX_HELMCHART_TEMPLATE_KEY_CHART`] leaf-scalar-axis key
        // and carry the `ClusterBundleOpts.chart_path` value verbatim.
        // Before this lift the leaf site carried an inline `chart:`
        // literal at the `cluster_bundle` `helmrelease.yaml` format-
        // string template's chart-name-leaf interpolation site — a
        // future Flux v3 rebrand on this axis (a hypothetical upstream
        // fluxcd/flux2 rename from `chart` to `Chart` / `chartRef` /
        // `chartName`) without a coordinated edit on the canonical
        // caixa-core const would have silently dangled the chart-
        // artifact resolution at the helm-controller's per-CR
        // reconcile-time chart-lookup with a non-self-locating
        // "chart 'unknown' not found in <source>" error far from the
        // rebrand commit's source. Peer to
        // [`cluster_bundle_helmrelease_chart_block_uses_lifted_flux_key_chart`]
        // on the parent container-axis surface — extends the pin
        // discipline one level beneath by asserting the chart-NAME
        // leaf-scalar under the container-axis carries the lifted
        // constant.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let hr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME))
            .expect("helmrelease.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&hr.contents).expect("helmrelease.yaml parses as YAML");
        let chart_name = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_KEY_CHART))
            .and_then(|c| c.get(KUBE_KEY_SPEC))
            .and_then(|s| s.get(FLUX_HELMCHART_TEMPLATE_KEY_CHART))
            .and_then(|n| n.as_str())
            .expect("spec.chart.spec.<FLUX_HELMCHART_TEMPLATE_KEY_CHART> scalar present");
        assert_eq!(
            chart_name,
            opts.chart_path,
            "spec.chart.spec.<FLUX_HELMCHART_TEMPLATE_KEY_CHART> \
             ({FLUX_HELMCHART_TEMPLATE_KEY_CHART:?}) leaf-scalar must carry the \
             ClusterBundleOpts.chart_path verbatim ({expected:?}); a drifted \
             key here silently dangles the chart-artifact resolution at the \
             Flux v2 helm-controller's per-CR reconcile-time chart-lookup axis",
            expected = opts.chart_path,
        );
    }

    #[test]
    fn flux_key_health_checks_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_KEY_HEALTH_CHECKS` is
        // the single source of truth for the Flux v2 per-`Kustomization`
        // health-gate reference-list container-axis key the rendered
        // `kustomization.yaml` nests its per-entry
        // `[]NamespacedObjectKindReference` list under. Pin the
        // equality (and the static-data identity, peer with the sibling
        // [`flux_key_source_ref_re_export_points_at_caixa_core_canonical`]
        // / [`flux_key_chart_re_export_points_at_caixa_core_canonical`]
        // / [`flux_key_values_re_export_points_at_caixa_core_canonical`]
        // pins on the sibling Flux v2 body-key surfaces) so any local
        // re-introduction of a sibling `pub const FLUX_KEY_HEALTH_CHECKS:
        // &str = "…"` (the canonical drift footgun where a sibling local
        // `pub const` could happen to carry the same string at the
        // source while pointing at a different `&'static` allocation) is
        // a build-time test failure naming the offending drift, not a
        // silent apply-time dangling-health-gate reconciliation freeze
        // (a rebrand on this axis without a coordinated caixa-core edit
        // silently dangles the `Kustomization.spec.healthChecks` health-
        // gate at the Flux v2 kustomize-controller's per-CR reconcile
        // loop; the parent Kustomization stays at `Reconciling` forever
        // and the dependent per-cluster fleet-programs upsert chain
        // never sees `Ready=True` at apply time with no field naming
        // the container-axis-drift root cause). Closes the sibling re-
        // export identity axis on the same trajectory the peer per-CR
        // body-key pins carry — completes the quartet.
        caixa_core::assert_str_reexport_identity(
            "FLUX_KEY_HEALTH_CHECKS",
            FLUX_KEY_HEALTH_CHECKS,
            caixa_core::FLUX_KEY_HEALTH_CHECKS,
        );
    }

    #[test]
    fn cluster_bundle_kustomization_health_checks_block_uses_lifted_flux_key_health_checks() {
        // Fail-before-pass-after pin: the rendered `kustomization.yaml`'s
        // top-level `spec.healthChecks` container-axis key — the scope
        // under which the Flux v2 `[]NamespacedObjectKindReference` list
        // lives (the per-entry `(apiVersion, kind, name, namespace)`
        // triples the Flux v2 `kustomize-controller`'s per-CR reconcile
        // loop reads to gate the parent Kustomization's `Ready=True`
        // transition on the referenced sibling `HelmRelease` reaching its
        // `HelmReleaseReady=True` condition) — must resolve to the
        // lifted [`FLUX_KEY_HEALTH_CHECKS`] verbatim. Before the lift
        // this site carried an inline `healthChecks:\n` literal in the
        // `kustomization` format string; a future Flux v3 rebrand on
        // this axis (a hypothetical upstream fluxcd/flux2 rename from
        // `healthChecks` to `HealthChecks` / `healthchecks` /
        // `healthcheck` / `health_checks` / `probes`, coordinated with
        // the upstream project's per-version deprecation cycle) without
        // a coordinated edit here would have silently dangled the whole
        // health-gate reference-list at `kustomize-controller` reconcile
        // time — the parent Kustomization stays at `Reconciling`
        // forever, and the dependent per-cluster fleet-programs upsert
        // chain never sees `Ready=True` far from the rebrand commit's
        // source.
        //
        // The pin is structural: parse the rendered YAML and assert the
        // per-`Kustomization` health-gate reference-list container-axis
        // key resolves under the lifted constant with a non-empty
        // sequence under it. A regression that re-introduces an inline
        // literal in the format-string template surfaces as a `None` at
        // the lifted-const-keyed lookup. Peer to the sibling
        // [`cluster_bundle_helmrelease_chart_block_uses_lifted_flux_key_chart`]
        // / [`cluster_bundle_helmrelease_values_block_uses_lifted_flux_key_values`]
        // pins on the sibling per-`HelmRelease` body-key surfaces —
        // extends the canonical-Flux-v2-body-key lifted-const-keyed pin
        // discipline from the per-`HelmRelease` triplet onto the sibling
        // per-`Kustomization` `spec.healthChecks` reference-list
        // container-axis this test targets, completing the quartet.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        let kz = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_KUSTOMIZATION_YAML_FILENAME))
            .expect("kustomization.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&kz.contents).expect("kustomization.yaml parses as YAML");
        let health_checks = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_KEY_HEALTH_CHECKS))
            .and_then(|v| v.as_sequence())
            .expect("spec.<FLUX_KEY_HEALTH_CHECKS> sequence present");
        assert!(
            !health_checks.is_empty(),
            "spec.<FLUX_KEY_HEALTH_CHECKS> ({FLUX_KEY_HEALTH_CHECKS:?}) must \
             carry at least one `[]NamespacedObjectKindReference` entry; \
             drift on this container-axis key silently dangles the whole \
             health-gate at kustomize-controller reconcile time and freezes \
             the parent Kustomization at `Reconciling`"
        );
    }

    #[test]
    fn flux_key_interval_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_KEY_INTERVAL` is the
        // single source of truth for the Flux v2 per-CR reconcile-poll
        // cadence scalar-axis key the three rendered Flux documents
        // (`gitrepository.yaml`, `helmrelease.yaml`, `kustomization.yaml`)
        // each carry as their `spec.interval` scalar. Pin the equality
        // (and the static-data identity, peer with the sibling
        // [`flux_key_source_ref_re_export_points_at_caixa_core_canonical`]
        // / [`flux_key_chart_re_export_points_at_caixa_core_canonical`]
        // / [`flux_key_values_re_export_points_at_caixa_core_canonical`]
        // / [`flux_key_health_checks_re_export_points_at_caixa_core_canonical`]
        // pins on the sibling Flux v2 per-CR body-key surfaces) so any
        // local re-introduction of a sibling `pub const FLUX_KEY_INTERVAL:
        // &str = "…"` (the canonical drift footgun where a sibling local
        // `pub const` could happen to carry the same string at the
        // source while pointing at a different `&'static` allocation) is
        // a build-time test failure naming the offending drift, not a
        // silent apply-time three-way reconcile freeze (a rebrand on
        // this axis without a coordinated caixa-core edit silently drops
        // the per-CR reconcile schedule from all three Flux v2
        // controllers' per-CR watch registrations simultaneously — the
        // referenced Git source never re-polls / the referenced chart
        // never re-templates / the parent Kustomization never re-applies
        // at upstream drift, freezing the whole cluster's per-`caixa`
        // per-cluster bundle at the last-applied snapshot with no field
        // naming the axis-drift root cause). Closes the re-export
        // identity axis on the same trajectory the peer per-CR body-key
        // pins carry — extends the discipline from the per-CR body-key
        // quartet onto the sibling cross-CR-shared reconcile-poll cadence
        // scalar-axis every Flux v2 controller reads.
        caixa_core::assert_str_reexport_identity(
            "FLUX_KEY_INTERVAL",
            FLUX_KEY_INTERVAL,
            caixa_core::FLUX_KEY_INTERVAL,
        );
    }

    #[test]
    fn flux_gitrepository_ref_key_tag_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub use caixa_core::FLUX_GITREPOSITORY_REF_KEY_TAG`
        // is the single source of truth for the FluxCD source-controller
        // `GitRepository.spec.ref.tag` sub-selector scalar-axis key the
        // rendered `gitrepository.yaml` document declares on the tag-arm
        // of the [`GitRefSpec`] discriminated-union. Pin the equality +
        // `&'static` static-data identity so any local re-introduction
        // of a sibling `pub const FLUX_GITREPOSITORY_REF_KEY_TAG: &str
        // = "…"` at this crate is a build-time test failure naming the
        // offending drift, not a silent apply-time `GitRepository`
        // sub-selector reroute at cluster-side reconcile time. Peer to
        // [`flux_key_interval_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-CR body-key surface — pivots the
        // canonical-lifted-const single-sourcing discipline onto the
        // per-`GitRepository`-`spec.ref`-sub-selector axis.
        caixa_core::assert_str_reexport_identity(
            "FLUX_GITREPOSITORY_REF_KEY_TAG",
            FLUX_GITREPOSITORY_REF_KEY_TAG,
            caixa_core::FLUX_GITREPOSITORY_REF_KEY_TAG,
        );
    }

    #[test]
    fn flux_gitrepository_ref_key_branch_re_export_points_at_caixa_core_canonical() {
        // Peer of
        // [`flux_gitrepository_ref_key_tag_re_export_points_at_caixa_core_canonical`]
        // on the branch-arm of the FluxCD source-controller
        // `GitRepository.spec.ref` discriminated-union axis.
        caixa_core::assert_str_reexport_identity(
            "FLUX_GITREPOSITORY_REF_KEY_BRANCH",
            FLUX_GITREPOSITORY_REF_KEY_BRANCH,
            caixa_core::FLUX_GITREPOSITORY_REF_KEY_BRANCH,
        );
    }

    #[test]
    fn flux_gitrepository_ref_key_commit_re_export_points_at_caixa_core_canonical() {
        // Peer of
        // [`flux_gitrepository_ref_key_tag_re_export_points_at_caixa_core_canonical`]
        // on the commit-arm of the FluxCD source-controller
        // `GitRepository.spec.ref` discriminated-union axis.
        caixa_core::assert_str_reexport_identity(
            "FLUX_GITREPOSITORY_REF_KEY_COMMIT",
            FLUX_GITREPOSITORY_REF_KEY_COMMIT,
            caixa_core::FLUX_GITREPOSITORY_REF_KEY_COMMIT,
        );
    }

    #[test]
    fn flux_gitrepository_key_ref_re_export_points_at_caixa_core_canonical() {
        // Bridge-arm pin: the re-exported FLUX_GITREPOSITORY_KEY_REF
        // resolves to the canonical `"ref"` byte + `&'static`
        // allocation from caixa-core, closing the local-`pub const`-
        // shadow footgun where a sibling `pub const
        // FLUX_GITREPOSITORY_KEY_REF: &str = "…"` in this crate could
        // silently carry the same string at the source while pointing
        // at a different `&'static` allocation. Peer of the sibling
        // [`flux_gitrepository_ref_key_tag_re_export_points_at_caixa_core_canonical`]
        // / [`flux_gitrepository_ref_key_branch_re_export_points_at_caixa_core_canonical`]
        // / [`flux_gitrepository_ref_key_commit_re_export_points_at_caixa_core_canonical`]
        // pins on the sibling per-shape arm axes of the FluxCD source-
        // controller `GitRepository.spec.ref` discriminated-union — closes
        // the parent-container-axis KEY pin above the already-pinned
        // per-shape arm sub-selector-KEY triple, so the whole per-
        // `GitRepository` `spec.ref` sub-schema (parent container-axis
        // KEY + per-shape arm sub-selector-KEY triple) now navigates
        // through four caixa-core `&'static str`s pinned in coordination.
        caixa_core::assert_str_reexport_identity(
            "FLUX_GITREPOSITORY_KEY_REF",
            FLUX_GITREPOSITORY_KEY_REF,
            caixa_core::FLUX_GITREPOSITORY_KEY_REF,
        );
    }

    #[test]
    fn cluster_bundle_gitrepository_spec_ref_key_pins_lifted_flux_gitrepository_key_ref() {
        // Production-emit pin: traverse a rendered `gitrepository.yaml`
        // document's `spec` block and assert the ref-selection
        // discriminated-union parent container-axis is keyed by the
        // *lifted* FLUX_GITREPOSITORY_KEY_REF (`"ref"`) verbatim — the
        // load-bearing per-`GitRepository` `spec.ref` container-axis
        // KEY the FluxCD source-controller reads to source the per-CR
        // git-clone refspec. Before the lift the writer template
        // carried an inline `ref:\n` literal at the sole
        // `format!("… spec:\n  ref:\n{gitref_field}\n", …)` call in
        // `cluster_bundle`; a typo there (`"gitRef:"` / `"Ref:"` /
        // `"revision:"` / `"source:"`) would have silently landed a
        // `GitRepository` whose ref-selection container-axis the CRD
        // schema validator drops as unknown at admission, and every
        // downstream `HelmRelease` / `Kustomization` bundle document
        // that resolves through the sibling
        // `HelmRelease.spec.chart.spec.sourceRef` reference would have
        // silently dangled at the FluxCD apply chain with no field
        // naming the container-axis-drift root cause. Peer of the
        // sibling per-shape arm sub-selector-KEY test
        // [`cluster_bundle_gitrepository_ref_key_dispatches_per_variant_onto_lifted_consts`]
        // on the peer per-arm scalar-axis surface — closes the parent
        // container-axis pin above the already-pinned per-shape arm
        // sub-selector-KEY triple.
        let caixa = sample_caixa();
        let opts = ClusterBundleOpts::for_caixa(&caixa, "rio");
        let files = cluster_bundle(&caixa, &opts).expect("bundle renders");
        let gr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_GITREPOSITORY_YAML_FILENAME))
            .expect("gitrepository.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&gr.contents).expect("gitrepository.yaml parses as YAML");
        assert!(
            parsed
                .get(KUBE_KEY_SPEC)
                .and_then(|s| s.get(FLUX_GITREPOSITORY_KEY_REF))
                .is_some(),
            "rendered gitrepository.yaml must carry its ref-selection \
             container-axis at the lifted FLUX_GITREPOSITORY_KEY_REF key \
             verbatim (got: {:?})",
            gr.contents
        );
    }

    #[test]
    fn flux_gitrepository_key_url_re_export_points_at_caixa_core_canonical() {
        // Bridge-arm pin: the re-exported FLUX_GITREPOSITORY_KEY_URL
        // resolves to the canonical `"url"` byte + `&'static`
        // allocation from caixa-core, closing the local-`pub const`-
        // shadow footgun where a sibling `pub const
        // FLUX_GITREPOSITORY_KEY_URL: &str = "…"` in this crate could
        // silently carry the same string at the source while pointing
        // at a different `&'static` allocation. Peer of the sibling
        // [`flux_gitrepository_key_ref_re_export_points_at_caixa_core_canonical`]
        // pin on the sibling per-CR `spec.ref` container-axis re-export
        // surface — closes the second per-`GitRepository` sub-block
        // key re-export identity pin, extending the discipline from
        // the container-axis surface onto the leaf-scalar remote-URL
        // axis.
        caixa_core::assert_str_reexport_identity(
            "FLUX_GITREPOSITORY_KEY_URL",
            FLUX_GITREPOSITORY_KEY_URL,
            caixa_core::FLUX_GITREPOSITORY_KEY_URL,
        );
    }

    #[test]
    fn cluster_bundle_gitrepository_spec_url_key_pins_lifted_flux_gitrepository_key_url() {
        // Production-emit pin: traverse a rendered `gitrepository.yaml`
        // document's `spec` block and assert the remote-repo-URL leaf-
        // scalar-axis is keyed by the *lifted*
        // FLUX_GITREPOSITORY_KEY_URL (`"url"`) verbatim — the load-
        // bearing per-`GitRepository` `spec.url` leaf-scalar-axis KEY
        // the FluxCD source-controller reads to source the per-CR
        // git-remote clone target. Before the lift the writer template
        // carried an inline `url: {url}\n` literal at the sole
        // `format!(…)` call in `cluster_bundle`'s gitrepo composer;
        // a typo there (`"URL:"` / `"gitUrl:"` / `"repository:"` /
        // `"repo:"`) would have silently landed a `GitRepository`
        // whose CRD schema validator drops the URL field as unknown
        // at admission, the per-Servico artifact would never populate,
        // and every downstream `HelmRelease` / `Kustomization` bundle
        // document would silently no-op at reconcile time with an
        // empty artifact. Peer of the sibling per-container-axis KEY
        // test
        // [`cluster_bundle_gitrepository_spec_ref_key_pins_lifted_flux_gitrepository_key_ref`]
        // on the peer `spec.ref` container-axis surface — extends the
        // per-CR sub-block key pin discipline from the ref-selection
        // container-axis onto the remote-URL leaf-scalar-axis.
        let caixa = sample_caixa();
        let opts = ClusterBundleOpts::for_caixa(&caixa, "rio");
        let files = cluster_bundle(&caixa, &opts).expect("bundle renders");
        let gr = files
            .iter()
            .find(|f| f.path == std::path::PathBuf::from(FLUX_GITREPOSITORY_YAML_FILENAME))
            .expect("gitrepository.yaml present");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&gr.contents).expect("gitrepository.yaml parses as YAML");
        let url = parsed
            .get(KUBE_KEY_SPEC)
            .and_then(|s| s.get(FLUX_GITREPOSITORY_KEY_URL))
            .and_then(|u| u.as_str())
            .expect(
                "rendered gitrepository.yaml must carry its remote-URL leaf-scalar \
                 axis at the lifted FLUX_GITREPOSITORY_KEY_URL key verbatim",
            );
        assert!(
            !url.is_empty(),
            "spec.url leaf-scalar must resolve to a non-empty git-remote clone \
             target — a drifted key would collapse the readback to None, an \
             empty string would break the source-controller's per-CR clone step"
        );
    }

    #[test]
    fn gitrefspec_ref_field_name_dispatches_per_variant_onto_lifted_consts() {
        // The [`GitRefSpec::ref_field_name`] method routes each variant
        // onto the paired canonical
        // [`FLUX_GITREPOSITORY_REF_KEY_TAG`] /
        // [`FLUX_GITREPOSITORY_REF_KEY_BRANCH`] /
        // [`FLUX_GITREPOSITORY_REF_KEY_COMMIT`] const via a compile-
        // time-exhaustive match — closes the drift surface where a
        // future variant addition or per-arm rebrand could silently
        // desynchronize the YAML emit-side + human-readable narrator
        // consumer sites from the sub-selector-key axis. Pin the
        // per-variant dispatch identity so a refactor that swaps two
        // arms' const references silently at the method-impl site
        // fires as a build-time test failure, not as a
        // `spec.ref.<wrong-key>` reroute at cluster-apply time.
        assert_eq!(
            GitRefSpec::Tag("v0.1.0".into()).ref_field_name(),
            FLUX_GITREPOSITORY_REF_KEY_TAG,
        );
        assert_eq!(
            GitRefSpec::Branch("main".into()).ref_field_name(),
            FLUX_GITREPOSITORY_REF_KEY_BRANCH,
        );
        assert_eq!(
            GitRefSpec::Commit("deadbeef".into()).ref_field_name(),
            FLUX_GITREPOSITORY_REF_KEY_COMMIT,
        );
    }

    #[test]
    fn gitrefspec_ref_value_extracts_underlying_scalar_per_variant() {
        // Peer of
        // [`gitrefspec_ref_field_name_dispatches_per_variant_onto_lifted_consts`]:
        // the [`GitRefSpec::ref_value`] method extracts the underlying
        // scalar the variant carries (tag / branch / commit value)
        // through a single collapsed match — the byte-string the
        // FluxCD source-controller feeds into its per-CR git-source
        // clone refspec. Both consumer sites in [`cluster_bundle`]
        // pair the sub-selector key with this scalar; pin the
        // per-variant extraction identity so a refactor that mixes
        // the arms (a spurious `.to_ascii_lowercase()` in one arm,
        // a lifetime-inversion that clones the payload as a scratch
        // `String`) surfaces as a build-time test failure.
        assert_eq!(GitRefSpec::Tag("v0.1.0".into()).ref_value(), "v0.1.0");
        assert_eq!(GitRefSpec::Branch("main".into()).ref_value(), "main");
        assert_eq!(
            GitRefSpec::Commit("deadbeef".into()).ref_value(),
            "deadbeef",
        );
    }

    #[test]
    fn cluster_bundle_gitref_field_composes_lifted_dispatch_byte_shape() {
        // Fail-before-pass-after emission-side pin: the rendered
        // `gitrepository.yaml` document's `spec.ref` sub-block byte-
        // shape matches the composition of the canonical lifted
        // [`GitRefSpec::ref_field_name`] +
        // [`GitRefSpec::ref_value`] dispatch pair against the prior
        // inline `format!("    {arm}: {v:?}")` per-arm match block,
        // for every arm of the [`GitRefSpec`] discriminated-union.
        // Byte-identical output to the prior 3-arm inline match
        // (`{v:?}` on `&str` renders the same shape as `{v:?}` on
        // `String`) — the composition equation
        // `format!("    {field}: {value:?}", ...)` reduces to
        // `format!("    tag: {t:?}")` / `format!("    branch: {b:?}")`
        // / `format!("    commit: {c:?}")` per arm by construction.
        // Peer to the sibling
        // [`cluster_bundle_default_git_tag_uses_lifted_caixa_core_prefix`]
        // emission-side pin on the tag-arm — pivots the discipline
        // from the tag-value axis (the prior lift closed) onto the
        // sub-selector-key axis (this lift closes) so the pair of
        // pins closes both coordinates of the `spec.ref.<key>:
        // <value>` sub-block.
        let cases = [
            (GitRefSpec::Tag("v0.1.0".into()), "    tag: \"v0.1.0\""),
            (GitRefSpec::Branch("main".into()), "    branch: \"main\""),
            (
                GitRefSpec::Commit("deadbeef".into()),
                "    commit: \"deadbeef\"",
            ),
        ];
        for (git_ref, expected) in cases {
            let composed = format!(
                "    {field}: {value:?}",
                field = git_ref.ref_field_name(),
                value = git_ref.ref_value(),
            );
            assert_eq!(
                composed, expected,
                "GitRefSpec::{git_ref:?} must compose to the prior \
                 inline byte-shape via the lifted dispatch",
            );
        }
    }

    #[test]
    fn cluster_bundle_gitref_narrator_composes_lifted_dispatch_byte_shape() {
        // Peer of
        // [`cluster_bundle_gitref_field_composes_lifted_dispatch_byte_shape`]
        // on the sibling `tag_human` narrator-prose axis in
        // [`cluster_bundle`] — pins the byte-shape of the `<arm>
        // <value>` operator-facing narrator prose the rendered
        // `gitrepository.yaml` document's leading `# Source — pinned
        // to <tag_human>` comment quotes. Byte-identical output to
        // the prior 3-arm inline `format!("{arm} {v}")` per-variant
        // match block by construction.
        let cases = [
            (GitRefSpec::Tag("v0.1.0".into()), "tag v0.1.0"),
            (GitRefSpec::Branch("main".into()), "branch main"),
            (GitRefSpec::Commit("deadbeef".into()), "commit deadbeef"),
        ];
        for (git_ref, expected) in cases {
            let composed = format!(
                "{field} {value}",
                field = git_ref.ref_field_name(),
                value = git_ref.ref_value(),
            );
            assert_eq!(
                composed, expected,
                "GitRefSpec::{git_ref:?} narrator prose must compose \
                 to the prior inline byte-shape via the lifted dispatch",
            );
        }
    }

    #[test]
    fn cluster_bundle_gitrepo_yaml_carries_lifted_sub_selector_keys() {
        // End-to-end emission-side pin: for every arm of
        // [`GitRefSpec`], the rendered `gitrepository.yaml` document's
        // `spec.ref` sub-block declares the sub-selector under the
        // canonical lifted [`FLUX_GITREPOSITORY_REF_KEY_TAG`] /
        // [`FLUX_GITREPOSITORY_REF_KEY_BRANCH`] /
        // [`FLUX_GITREPOSITORY_REF_KEY_COMMIT`] key — pin the presence
        // + value round-trip through the rendered YAML so a regression
        // that re-inlines the sub-selector key at the emit site
        // surfaces here as a test failure rather than as a silent
        // FluxCD source-controller sub-block reroute at reconcile
        // time. Peer to the sibling
        // [`cluster_bundle_default_git_tag_uses_lifted_caixa_core_prefix`]
        // pin that closes the sibling per-arm value axis on the
        // tag-arm.
        let caixa = sample_caixa();
        let cases: [(GitRefSpec, &str, &str); 3] = [
            (
                GitRefSpec::Tag("v0.1.0".into()),
                FLUX_GITREPOSITORY_REF_KEY_TAG,
                "v0.1.0",
            ),
            (
                GitRefSpec::Branch("main".into()),
                FLUX_GITREPOSITORY_REF_KEY_BRANCH,
                "main",
            ),
            (
                GitRefSpec::Commit("deadbeef".into()),
                FLUX_GITREPOSITORY_REF_KEY_COMMIT,
                "deadbeef",
            ),
        ];
        for (git_ref, expected_key, expected_value) in cases {
            let mut opts = ClusterBundleOpts::for_caixa(&caixa, "rio");
            opts.git_ref = git_ref.clone();
            let files = cluster_bundle(&caixa, &opts).expect("bundle renders");
            let gr = files
                .iter()
                .find(|f| f.path == std::path::PathBuf::from(FLUX_GITREPOSITORY_YAML_FILENAME))
                .expect("gitrepository.yaml present");
            let parsed: serde_yaml::Value =
                serde_yaml::from_str(&gr.contents).expect("gitrepository.yaml parses as YAML");
            let sub_selector = parsed
                .get(KUBE_KEY_SPEC)
                .and_then(|s| s.get(FLUX_GITREPOSITORY_KEY_REF))
                .and_then(|r| r.get(expected_key))
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    panic!(
                        "spec.ref.{expected_key:?} missing or non-string \
                         for GitRefSpec::{git_ref:?}: {contents:?}",
                        contents = gr.contents,
                    )
                });
            assert_eq!(
                sub_selector, expected_value,
                "spec.ref.{expected_key:?} must carry the paired \
                 scalar for GitRefSpec::{git_ref:?}",
            );
        }
    }

    #[test]
    fn flux_helmrelease_yaml_filename_re_export_points_at_caixa_core_canonical() {
        // The renderer's `FLUX_HELMRELEASE_YAML_FILENAME` was lifted from
        // the thirteen production + test-side inline `"helmrelease.yaml"`
        // / `PathBuf::from("helmrelease.yaml")` /
        // `names.contains(&"helmrelease.yaml".to_string())` literals
        // across [`cluster_bundle`]'s per-`BundleFile` `HelmRelease`
        // document `path` emit site + every test-side round-trip
        // navigator that reaches into the rendered bundle by the
        // `HelmRelease` document filename to a re-export of
        // [`caixa_core::FLUX_HELMRELEASE_YAML_FILENAME`] so the Flux v2
        // per-cluster-bundle `HelmRelease` document filename lives in
        // exactly one place across every caixa renderer. Pin the equality
        // + `&'static` static-data identity here so any local
        // re-introduction of a sibling `pub const
        // FLUX_HELMRELEASE_YAML_FILENAME: &str = "…"` at this crate — the
        // canonical drift footgun where a sibling local `pub const` could
        // happen to carry the same string at the source while pointing
        // at a different `&'static` allocation — is a build-time test
        // failure naming the offending drift, not a silent FluxCD
        // `kustomize-controller` "no `HelmRelease` document found under
        // this bundle" reroute at cluster-side reconcile time far from
        // the drift site. Peer to
        // [`flux_key_interval_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-CR body-key surface, and to the
        // [`caixa_helm::HELM_CHART_YAML_FILENAME`] (c2c99b0) /
        // [`caixa_helm::HELM_VALUES_YAML_FILENAME`] (9a980ba) re-exports
        // on the sibling Helm-chart-directory filename surfaces — pivots
        // the canonical-filename single-sourcing discipline from the
        // per-Helm-chart-directory metadata / values file axes onto the
        // sibling per-Flux-v2-bundle `HelmRelease` document filename
        // axis this crate's [`cluster_bundle`] renders.
        caixa_core::assert_str_reexport_identity(
            "FLUX_HELMRELEASE_YAML_FILENAME",
            FLUX_HELMRELEASE_YAML_FILENAME,
            caixa_core::FLUX_HELMRELEASE_YAML_FILENAME,
        );
    }

    #[test]
    fn flux_gitrepository_yaml_filename_re_export_points_at_caixa_core_canonical() {
        // The renderer's `FLUX_GITREPOSITORY_YAML_FILENAME` was lifted
        // from the nine production + test-side inline
        // `"gitrepository.yaml"` / `PathBuf::from("gitrepository.yaml")`
        // / `names.contains(&"gitrepository.yaml".to_string())` literals
        // across [`cluster_bundle`]'s per-`BundleFile` `GitRepository`
        // document `path` emit site + every test-side round-trip
        // navigator that reaches into the rendered bundle by the
        // `GitRepository` document filename to a re-export of
        // [`caixa_core::FLUX_GITREPOSITORY_YAML_FILENAME`] so the Flux v2
        // per-cluster-bundle `GitRepository` document filename lives in
        // exactly one place across every caixa renderer. Pin the
        // equality + `&'static` static-data identity here so any local
        // re-introduction of a sibling `pub const
        // FLUX_GITREPOSITORY_YAML_FILENAME: &str = "…"` at this crate —
        // the canonical drift footgun where a sibling local `pub const`
        // could happen to carry the same string at the source while
        // pointing at a different `&'static` allocation — is a
        // build-time test failure naming the offending drift, not a
        // silent FluxCD `source-controller` "no `GitRepository`
        // document found under this bundle" reroute at cluster-side
        // reconcile time far from the drift site. Peer to
        // [`flux_helmrelease_yaml_filename_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-`HelmRelease`-document filename surface —
        // extends the canonical-Flux-v2-bundle-filename lifted-const
        // discipline from the middle coordinate of the filename triple
        // onto its first coordinate.
        caixa_core::assert_str_reexport_identity(
            "FLUX_GITREPOSITORY_YAML_FILENAME",
            FLUX_GITREPOSITORY_YAML_FILENAME,
            caixa_core::FLUX_GITREPOSITORY_YAML_FILENAME,
        );
    }

    #[test]
    fn flux_kustomization_yaml_filename_re_export_points_at_caixa_core_canonical() {
        // The renderer's `FLUX_KUSTOMIZATION_YAML_FILENAME` was lifted
        // from the sixteen production + test-side inline
        // `"kustomization.yaml"` /
        // `PathBuf::from("kustomization.yaml")` /
        // `names.contains(&"kustomization.yaml".to_string())` literals
        // across [`cluster_bundle`]'s per-`BundleFile` `Kustomization`
        // document `path` emit site + every test-side round-trip
        // navigator that reaches into the rendered bundle by the
        // `Kustomization` document filename to a re-export of
        // [`caixa_core::FLUX_KUSTOMIZATION_YAML_FILENAME`] so the
        // Flux v2 per-cluster-bundle `Kustomization` document filename
        // lives in exactly one place across every caixa renderer. Pin
        // the equality + `&'static` static-data identity here so any
        // local re-introduction of a sibling `pub const
        // FLUX_KUSTOMIZATION_YAML_FILENAME: &str = "…"` at this crate —
        // the canonical drift footgun where a sibling local `pub const`
        // could happen to carry the same string at the source while
        // pointing at a different `&'static` allocation — is a
        // build-time test failure naming the offending drift, not a
        // silent FluxCD `kustomize-controller` "no `Kustomization`
        // document found under this bundle" reroute at cluster-side
        // reconcile time far from the drift site. Peer to
        // [`flux_helmrelease_yaml_filename_re_export_points_at_caixa_core_canonical`]
        // + [`flux_gitrepository_yaml_filename_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-`HelmRelease` / per-`GitRepository`
        // document filename surfaces — closes the canonical-Flux-v2-
        // bundle-filename lifted-const discipline on the third
        // coordinate of the filename triple.
        caixa_core::assert_str_reexport_identity(
            "FLUX_KUSTOMIZATION_YAML_FILENAME",
            FLUX_KUSTOMIZATION_YAML_FILENAME,
            caixa_core::FLUX_KUSTOMIZATION_YAML_FILENAME,
        );
    }

    #[test]
    fn cluster_bundle_every_flux_cr_carries_lifted_flux_key_interval_scalar() {
        // Fail-before-pass-after pin: every rendered Flux v2 document in
        // the `cluster_bundle` triplet (`gitrepository.yaml`,
        // `helmrelease.yaml`, `kustomization.yaml`) must resolve its
        // `spec.interval` reconcile-poll cadence scalar under the lifted
        // [`FLUX_KEY_INTERVAL`] verbatim. Before the lift each of the
        // three sites carried an inline `interval:` literal in its
        // per-CR format string; a future Flux v3 rebrand on this axis (a
        // hypothetical upstream fluxcd/flux2 rename from `interval` to
        // `Interval` / `period` / `cadence` / `pollInterval` /
        // `reconcileInterval`, coordinated with the upstream project's
        // per-version deprecation cycle) without a coordinated edit at
        // any one of the three emit sites would have silently dropped
        // the per-CR reconcile schedule from the affected Flux v2
        // controller's per-CR watch registration — the referenced Git
        // source never re-polls / the referenced chart never re-templates
        // / the parent Kustomization never re-applies at upstream drift,
        // freezing the whole cluster's per-`caixa` per-cluster bundle at
        // the last-applied snapshot far from the rebrand commit's source.
        //
        // The pin is structural: parse each rendered YAML document in
        // the triplet and assert the per-CR reconcile-poll cadence
        // scalar-axis key resolves under the lifted constant with a
        // non-empty string scalar under it. A regression that re-introduces
        // an inline literal at any of the three emit sites surfaces as a
        // `None` at the lifted-const-keyed lookup on that document. Peer
        // to the sibling
        // [`cluster_bundle_helmrelease_chart_block_uses_lifted_flux_key_chart`]
        // /
        // [`cluster_bundle_helmrelease_values_block_uses_lifted_flux_key_values`]
        // /
        // [`cluster_bundle_kustomization_health_checks_block_uses_lifted_flux_key_health_checks`]
        // pins on the sibling per-CR body-key surfaces — extends the
        // canonical-Flux-v2-body-key lifted-const-keyed pin discipline
        // from the per-CR body-key quartet onto the sibling cross-CR-
        // shared reconcile-poll cadence scalar-axis this test targets.
        let opts = ClusterBundleOpts::for_caixa(&sample_caixa(), "rio");
        let files = cluster_bundle(&sample_caixa(), &opts).unwrap();
        for filename in [
            FLUX_GITREPOSITORY_YAML_FILENAME,
            FLUX_HELMRELEASE_YAML_FILENAME,
            FLUX_KUSTOMIZATION_YAML_FILENAME,
        ] {
            let doc = files
                .iter()
                .find(|f| f.path == std::path::PathBuf::from(filename))
                .unwrap_or_else(|| panic!("{filename} present"));
            let parsed: serde_yaml::Value = serde_yaml::from_str(&doc.contents)
                .unwrap_or_else(|_| panic!("{filename} parses as YAML"));
            let interval = parsed
                .get(KUBE_KEY_SPEC)
                .and_then(|s| s.get(FLUX_KEY_INTERVAL))
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    panic!(
                        "{filename} spec.<FLUX_KEY_INTERVAL> ({FLUX_KEY_INTERVAL:?}) \
                         scalar present; drift on this axis silently drops the \
                         per-CR reconcile schedule from the Flux v2 controller's \
                         per-CR watch registration",
                    )
                });
            assert!(
                !interval.is_empty(),
                "{filename} spec.<FLUX_KEY_INTERVAL> ({FLUX_KEY_INTERVAL:?}) must \
                 carry a non-empty duration scalar; the Flux v2 controller's \
                 per-CR reconcile loop rejects an empty cadence at admission",
            );
            // The renderer threads opts.interval through every CR
            // verbatim (the same [`ClusterBundleOpts::interval`] field
            // for the whole per-cluster bundle); pin that the emitted
            // scalar agrees with the opts-side input so a future
            // refactor that per-CR-overrides the cadence surfaces here
            // rather than as a silent per-CR reconcile-schedule split.
            assert_eq!(
                interval, opts.interval,
                "{filename} spec.<FLUX_KEY_INTERVAL> ({FLUX_KEY_INTERVAL:?}) \
                 must carry the same duration scalar the [`ClusterBundleOpts`] \
                 seeded — drift here silently splits the per-CR reconcile \
                 schedule across the three Flux v2 controllers",
            );
        }
    }

    #[test]
    fn bundle_file_alias_resolves_to_caixa_core_rendered_file() {
        // Type-alias identity pin: the [`BundleFile`] alias at this
        // crate's boundary resolves to the canonical
        // [`caixa_core::RenderedFile`] the substrate-side "one rendered
        // leaf artifact" shape lives at. `let _: BundleFile = <a
        // RenderedFile>` type-checks *iff* [`BundleFile`] is the
        // aliased canonical (not a sibling pub-struct re-declaration
        // that happens to carry the same field pair — that would
        // compile past the struct-literal navigators below but fail
        // this assignment). A drifted local `pub struct BundleFile
        // { pub path: PathBuf, pub contents: String }` at this crate —
        // the canonical drift footgun that would carry the same
        // field pair at the source while pointing at a different
        // struct definition — trips this pin at caixa-flux build time
        // rather than surfacing as a downstream `caixa_core::RenderedFile`
        // consumer refusing a `BundleFile`-shaped value at type-check
        // time far from the drift commit.
        let canonical: caixa_core::RenderedFile = caixa_core::RenderedFile {
            path: std::path::PathBuf::from(FLUX_GITREPOSITORY_YAML_FILENAME),
            contents: String::new(),
        };
        let aliased: BundleFile = canonical.clone();
        assert_eq!(aliased, canonical);
        // Struct-literal construction still resolves through the alias
        // — the pre-lift `BundleFile { path, contents }` shape at every
        // production emit site (three sites in [`cluster_bundle`])
        // continues to compile, and the derive tuple travels through
        // the alias.
        let via_alias = BundleFile {
            path: std::path::PathBuf::from(FLUX_HELMRELEASE_YAML_FILENAME),
            contents: "kind: HelmRelease\n".to_string(),
        };
        assert_eq!(
            via_alias.path.to_string_lossy(),
            FLUX_HELMRELEASE_YAML_FILENAME
        );
    }
}
