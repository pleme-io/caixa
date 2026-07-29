//! OTP-shaped supervisor trees, encoded as a typed `:kind Supervisor`
//! caixa with a strategy + restart-policy children list.
//!
//! See `theory/INSPIRATIONS.md` §II.2 + §III.2 for the prior-art frame
//! (Erlang OTP supervisor + Lunatic supervisor strategies as Rust types).
//!
//! ```lisp
//! (defcaixa
//!   :nome           "my-app-root"
//!   :versao         "0.1.0"
//!   :kind           Supervisor
//!   :estrategia     OneForOne
//!   :max-restarts   5
//!   :restart-window "60s"
//!   :children       ((:caixa "worker"       :versao "^0.1" :restart Permanent)
//!                    (:caixa "cache-server" :versao "^0.1" :restart Transient)
//!                    (:caixa "scratch-job"  :versao "^0.1" :restart Temporary)))
//! ```
//!
//! wasm-operator (M3) walks the tree, materializes one ComputeUnit per
//! child, and applies the strategy on child failure. The Rust types
//! here are the typed contract; the runtime owns lifecycle.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One of the four canonical Erlang/OTP restart strategies.
///
/// The strategy decides what happens to *sibling* children when one
/// child dies. Per-child behaviour is governed by [`RestartPolicy`].
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    gen_platform::TypedDispatcher,
    gen_platform::Discriminant,
    gen_platform::IsVariant,
    gen_platform::FromStrKind,
)]
pub enum RestartStrategy {
    /// On child failure, restart only that child. Default; matches
    /// most "tree of independent workers" use cases.
    OneForOne,
    /// On child failure, restart every child. Used when children
    /// share state and must be in sync.
    OneForAll,
    /// On child failure, restart the failed child and every child
    /// started *after* it (preserving startup order). Used when later
    /// children depend on earlier ones.
    RestForOne,
    /// Dynamic children of the same shape, started on demand. The
    /// supervisor doesn't know its children at boot; they're added as
    /// they're needed (e.g. one child per session).
    SimpleOneForOne,
}

impl Default for RestartStrategy {
    fn default() -> Self {
        Self::OneForOne
    }
}

impl RestartStrategy {
    /// Canonical PascalCase discriminator scalar this variant serializes
    /// as under [`crate::render::SUPERVISOR_KEY_ESTRATEGIA`]. The four arms
    /// return the paired [`crate::render::SUPERVISOR_ESTRATEGIA_ONE_FOR_ONE`]
    /// / [`crate::render::SUPERVISOR_ESTRATEGIA_ONE_FOR_ALL`] /
    /// [`crate::render::SUPERVISOR_ESTRATEGIA_REST_FOR_ONE`] /
    /// [`crate::render::SUPERVISOR_ESTRATEGIA_SIMPLE_ONE_FOR_ONE`] lifted
    /// constants so every substrate consumer that dispatches on the
    /// per-supervisor sibling-restart strategy (the future
    /// wasm-operator's per-supervisor sibling-restart branch, the future
    /// M4 `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's
    /// admission-time enum-arm bind, the `caixa-operator`'s hierarchical
    /// reconciliation scheduler's per-strategy fan-out) reads the same
    /// byte-string the `Serialize` derive emits — the pin test in
    /// [`tests::restart_strategy_variants_serialize_to_lifted_scalar_values`]
    /// asserts the two paths agree, peer of the M3
    /// `PlacementStrategy::as_str` (cc8f749) on the sibling per-Aplicacao
    /// distribution-strategy axis.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OneForOne => crate::render::SUPERVISOR_ESTRATEGIA_ONE_FOR_ONE,
            Self::OneForAll => crate::render::SUPERVISOR_ESTRATEGIA_ONE_FOR_ALL,
            Self::RestForOne => crate::render::SUPERVISOR_ESTRATEGIA_REST_FOR_ONE,
            Self::SimpleOneForOne => crate::render::SUPERVISOR_ESTRATEGIA_SIMPLE_ONE_FOR_ONE,
        }
    }
}

/// [`std::fmt::Display`] routed through [`RestartStrategy::as_str`], so the
/// pretty-printed byte-string every consumer that formats the strategy as
/// user-facing text lands on (the future wasm-operator's per-supervisor
/// sibling-restart-strategy diagnostic line, the future `feira app graph`
/// per-supervisor strategy line, the future M4
/// `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's admission-webhook
/// rejection body) reaches for the same lifted
/// [`crate::render::SUPERVISOR_ESTRATEGIA_ONE_FOR_ONE`] /
/// [`crate::render::SUPERVISOR_ESTRATEGIA_ONE_FOR_ALL`] /
/// [`crate::render::SUPERVISOR_ESTRATEGIA_REST_FOR_ONE`] /
/// [`crate::render::SUPERVISOR_ESTRATEGIA_SIMPLE_ONE_FOR_ONE`] const the
/// wire-format `Serialize` derive already emits under
/// [`crate::render::SUPERVISOR_KEY_ESTRATEGIA`] and the
/// [`RestartStrategy::as_str`] helper already returns.
///
/// Pre-convergence the two paths structurally disagreed — the
/// `#[derive(gen_platform::Discriminant)]` + `#[discriminant(also_display)]`
/// route (now retired here) sent [`std::fmt::Display`] through the
/// gen-platform discriminant catalog string, which arrives kebab-case as
/// `"one-for-one"` / `"one-for-all"` / `"rest-for-one"` /
/// `"simple-one-for-one"`, while the wire format ran as `PascalCase`
/// `"OneForOne"` / `"OneForAll"` / `"RestForOne"` / `"SimpleOneForOne"`
/// through the un-`rename`d serde derive. Every consumer that formatted
/// the strategy for a diagnostic line, a graph, or a rejection body under
/// `format!("{v}")` therefore landed under a different byte-string than
/// the wire format the operator's per-strategy dispatch keyed off — a
/// silent split whose apply-time symptom (a `format!("{v}")`-carrying
/// diagnostic quoting `"one-for-one"` while the wire scalar the operator
/// probed was `"OneForOne"`) surfaced as a confused correlate at
/// operator-log time far from the two-declaration site.
///
/// Routing `Display` through [`RestartStrategy::as_str`] closes the third
/// path: every `format!("{v}")` call reaches the same lifted
/// [`crate::render::SUPERVISOR_ESTRATEGIA_*`] const the wire format and
/// the [`RestartStrategy::as_str`] helper route through — `Debug` (the
/// compiler-derived variant name), `Display` (via `as_str`), and `Serialize`
/// (via the un-`rename`d derive) all resolve to the same `PascalCase`
/// byte-string per variant. A future variant rename or
/// `#[serde(rename_all = "kebab-case")]` attribute reaches every path at
/// exactly one place, structurally.
///
/// The dispatcher-catalog identity remains kebab-case — [`Self::discriminant`]
/// (from `#[derive(gen_platform::Discriminant)]`) still returns
/// `"one-for-one"` / etc., and the fleet-wide
/// [`gen_platform::register_dispatcher!("caixa.restart-strategy", …)`]
/// registration keys the catalog off the same kebab identity. The two
/// naming worlds now live on separate typed methods (`Display` /
/// `as_str` for the wire byte-string, `discriminant` for the catalog
/// identity) rather than sharing one `Display` route that structurally
/// disagrees with the wire format.
///
/// Pin tests
/// [`tests::restart_strategy_display_routes_through_as_str_helper`]
/// and
/// [`tests::restart_strategy_display_matches_serialized_wire_byte_string`]
/// assert the three paths agree byte-for-byte on every variant, so a
/// future variant rename or per-arm serde attribute drift is a build
/// error visible at caixa-core test time, not a silent per-consumer
/// dispatch miss at apply / reconcile time.
///
/// Mirrors the M3 [`crate::aplicacao::PlacementStrategy`] `Display` impl
/// (aplicacao.rs:2306) on the sibling per-Aplicacao distribution-strategy
/// axis — same three-path-convergence discipline, extended to close the
/// second of three OTP-shaped closed-enum discriminator axes on the
/// caixa typed surface.
impl std::fmt::Display for RestartStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Per-child restart policy.
///
/// Permanent / Temporary / Transient match Erlang/OTP semantics 1:1.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    gen_platform::TypedDispatcher,
    gen_platform::Discriminant,
    gen_platform::IsVariant,
    gen_platform::FromStrKind,
)]
pub enum RestartPolicy {
    /// Always restart the child, regardless of how it died. Used for
    /// long-running services that must always be up.
    Permanent,
    /// Never restart. Used for one-shot work whose completion is
    /// itself the success signal (`oneShot` triggers map here).
    Temporary,
    /// Restart only when the child died *abnormally* (non-zero exit
    /// or unhandled exception). A clean exit completes the child.
    Transient,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self::Permanent
    }
}

impl RestartPolicy {
    /// Canonical PascalCase discriminator scalar this variant serializes
    /// as under [`crate::render::SUPERVISOR_CHILD_KEY_RESTART`]. The three
    /// arms return the paired
    /// [`crate::render::SUPERVISOR_CHILD_RESTART_PERMANENT`] /
    /// [`crate::render::SUPERVISOR_CHILD_RESTART_TEMPORARY`] /
    /// [`crate::render::SUPERVISOR_CHILD_RESTART_TRANSIENT`] lifted
    /// constants so every substrate consumer that dispatches on the
    /// per-child restart-decision policy (the future wasm-operator's
    /// per-child post-exit restart-decision branch, the future M4
    /// `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's
    /// admission-time enum-arm bind, the `caixa-operator`'s hierarchical
    /// reconciliation scheduler's per-child-policy fan-out) reads the
    /// same byte-string the `Serialize` derive emits — the pin test in
    /// [`tests::restart_policy_variants_serialize_to_lifted_scalar_values`]
    /// asserts the two paths agree, peer of the M2
    /// [`RestartStrategy::as_str`] (09ffb2d) on the sibling per-supervisor
    /// sibling-restart-strategy axis and the M3
    /// [`crate::aplicacao::PlacementStrategy::as_str`] (cc8f749) on the
    /// per-Aplicacao distribution-strategy axis — the third of three
    /// OTP-shaped closed-enum discriminator axes on the caixa typed
    /// surface to converge onto the same three-path-convergence
    /// (`Serialize` derive → `as_str` helper → lifted constant)
    /// drift-detection posture.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Permanent => crate::render::SUPERVISOR_CHILD_RESTART_PERMANENT,
            Self::Temporary => crate::render::SUPERVISOR_CHILD_RESTART_TEMPORARY,
            Self::Transient => crate::render::SUPERVISOR_CHILD_RESTART_TRANSIENT,
        }
    }
}

/// [`std::fmt::Display`] routed through [`RestartPolicy::as_str`], so the
/// pretty-printed byte-string every consumer that formats the policy as
/// user-facing text lands on (the future wasm-operator's per-child
/// post-exit restart-decision diagnostic line, the future `feira app
/// graph` per-child restart column, the future M4
/// `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's per-child
/// admission-webhook rejection body) reaches for the same lifted
/// [`crate::render::SUPERVISOR_CHILD_RESTART_PERMANENT`] /
/// [`crate::render::SUPERVISOR_CHILD_RESTART_TEMPORARY`] /
/// [`crate::render::SUPERVISOR_CHILD_RESTART_TRANSIENT`] const the
/// wire-format `Serialize` derive already emits under
/// [`crate::render::SUPERVISOR_CHILD_KEY_RESTART`] and the
/// [`RestartPolicy::as_str`] helper already returns.
///
/// Pre-convergence the two paths structurally disagreed — the
/// `#[derive(gen_platform::Discriminant)]` + `#[discriminant(also_display)]`
/// route (now retired here) sent [`std::fmt::Display`] through the
/// gen-platform discriminant catalog string, which arrives kebab-case as
/// `"permanent"` / `"temporary"` / `"transient"` on this three-arm enum
/// (whose variant names each collapse to their own lowercase form under
/// the kebab-case transform), while the wire format ran as `PascalCase`
/// `"Permanent"` / `"Temporary"` / `"Transient"` through the un-`rename`d
/// serde derive. Every consumer that formatted the policy for a
/// diagnostic line, a graph column, or a rejection body under
/// `format!("{v}")` therefore landed under a different byte-string than
/// the wire format the operator's per-child-policy dispatch keyed off —
/// a silent split whose apply-time symptom (a `format!("{v}")`-carrying
/// diagnostic quoting `"permanent"` while the wire scalar the operator
/// probed was `"Permanent"`) surfaced as a confused correlate at
/// operator-log time far from the two-declaration site.
///
/// Routing `Display` through [`RestartPolicy::as_str`] closes the third
/// path: every `format!("{v}")` call reaches the same lifted
/// [`crate::render::SUPERVISOR_CHILD_RESTART_*`] const the wire format
/// and the [`RestartPolicy::as_str`] helper route through — `Debug` (the
/// compiler-derived variant name), `Display` (via `as_str`), and `Serialize`
/// (via the un-`rename`d derive) all resolve to the same `PascalCase`
/// byte-string per variant. A future variant rename or
/// `#[serde(rename_all = "kebab-case")]` attribute reaches every path at
/// exactly one place, structurally.
///
/// The dispatcher-catalog identity remains kebab-case — [`Self::discriminant`]
/// (from `#[derive(gen_platform::Discriminant)]`) still returns
/// `"permanent"` / `"temporary"` / `"transient"`, and the fleet-wide
/// [`gen_platform::register_dispatcher!("caixa.restart-policy", …)`]
/// registration keys the catalog off the same kebab identity. The two
/// naming worlds now live on separate typed methods (`Display` /
/// `as_str` for the wire byte-string, `discriminant` for the catalog
/// identity) rather than sharing one `Display` route that structurally
/// disagrees with the wire format.
///
/// Pin tests
/// [`tests::restart_policy_display_routes_through_as_str_helper`]
/// and
/// [`tests::restart_policy_display_matches_serialized_wire_byte_string`]
/// assert the three paths agree byte-for-byte on every variant, so a
/// future variant rename or per-arm serde attribute drift is a build
/// error visible at caixa-core test time, not a silent per-consumer
/// dispatch miss at apply / reconcile time.
///
/// Mirrors the M3 [`crate::aplicacao::PlacementStrategy`] `Display` impl
/// (aplicacao.rs:2306) on the per-Aplicacao distribution-strategy axis
/// and the sibling [`RestartStrategy`] `Display` impl on the
/// per-supervisor sibling-restart-strategy axis — same three-path-
/// convergence discipline, extended to close the third and final of
/// three OTP-shaped closed-enum discriminator axes on the caixa typed
/// surface.
impl std::fmt::Display for RestartPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// Fleet-wide dispatcher-catalog registrations for caixa's OTP
// supervisor surface — two more typed shadows over Erlang/OTP
// primitives the substrate now mechanically tracks (see
// theory/UNIFIED-COMPUTING-MODEL.md §VI for the roadmap +
// theory/TYPED-ABSORPTION.md for the absorption arc).
gen_platform::register_dispatcher!("caixa.restart-strategy", RestartStrategy);
gen_platform::register_dispatcher!("caixa.restart-policy", RestartPolicy);

/// One child entry in the supervisor's `:children` list.
///
/// Every child references another caixa by `:caixa <nome>` + version
/// constraint. The supervisor materializes one ComputeUnit per entry.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChildSpec {
    /// The child caixa's `:nome`. Must resolve via the same dependency
    /// resolution path as `:deps` (caixa-resolver).
    pub caixa: String,

    /// Semver constraint (`"^0.1"`, `"~0.1.2"`, etc.) — same shape as
    /// [`crate::dep::Dep::versao`].
    pub versao: String,

    /// Restart policy — defaults to [`RestartPolicy::Permanent`].
    #[serde(default)]
    pub restart: RestartPolicy,
}

impl ChildSpec {
    /// Substrate-canonical per-`:children` child-caixa `:nome` scalar
    /// accessor every consumer that reads the OTP-shape supervised
    /// child's identity keys off — returns the author-declared
    /// `:children :caixa` byte-string verbatim as a `&str`, borrowed
    /// from the typed slot's own [`String`] storage.
    ///
    /// The `:children :caixa` slot carries the DNS-1123 label — the
    /// child caixa's `:nome` — that every emitted cluster artifact
    /// derives its `metadata.name` from verbatim: the rendered
    /// `wasm.pleme.io/v1alpha1/ComputeUnit.metadata.name` per child, the
    /// [`crate::LABEL_PROGRAM`] label value on every child's pod
    /// identity, and the per-child K8s Service `metadata.name` the
    /// future wasm-operator (M3) provisions for inter-child supervision-
    /// tree wiring. Every downstream consumer that fans on the child's
    /// caixa-name keys off this scalar (the [`SupervisorSpec::validate`]
    /// per-child DNS-1123 gate at
    /// `require_valid_dns_1123_label(child.nome(), …)`, the per-child
    /// duplicate-detection [`crate::render::insert_first_seen`] key, the
    /// [`validate_no_self_supervision`] cross-slot equality check
    /// against the parent's `:nome`, every `SupervisorError` variant
    /// carrying the offending child caixa verbatim for `feira lint`
    /// rendering, the future wasm-operator's hierarchical reconciliation
    /// scheduler's per-child ComputeUnit-name projection, the future M4
    /// `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's per-child
    /// admission webhook).
    ///
    /// Prior to this lift the `.caixa` byte-string was accessed inline
    /// at seven sites in `supervisor.rs` — the DNS-1123 gate's
    /// `&child.caixa`, the four `SupervisorError::{ChildCaixaInvalid,
    /// EmptyChildVersion, ChildVersaoInvalid, DuplicateChildCaixa}`
    /// carriers' `child.caixa.clone()`, the dedup key's
    /// `child.caixa.as_str()`, and the [`validate_no_self_supervision`]
    /// `child.caixa == parent_nome` cross-slot check — seven open-coded
    /// field-accesses that expressed no compile-time link back to the
    /// typed slot. A future extension of the `:children :caixa` axis to
    /// a richer author surface (a per-cluster alias table the operator
    /// pins through a future `:placement`-scoped slot on the supervisor
    /// tree, a namespace-qualified rewrite the M4 CR materializer
    /// applies per-CR, a per-child overlay from the future `:children
    /// :nome-suffix` slot the MESH-COMPOSITION §III.2 roadmap
    /// acknowledges) would have had to be threaded through every
    /// open-coded copy in lockstep or one consumer would silently
    /// disagree with the peers on which caixa a given child resolves to
    /// — a child-set lookup that treated the name as `"cart-worker"`
    /// while the peer duplicate-detector treated it as
    /// `"tenant-a/cart-worker"` would silently split the
    /// `DuplicateChildCaixa` membership-lookup diagnostic from the
    /// self-supervision detector's parent-equality check, a two-consumer
    /// split at the validator far from the source `caixa.lisp` with no
    /// field naming the identity-drift root cause. Lifting the resolution
    /// rule to a typed method on the substrate primitive means every
    /// downstream consumer of the Supervisor's per-`:children` identity
    /// surface reaches for exactly one typed dispatch — the resolver's
    /// accept-set migrates as a unit on any future axis addition.
    ///
    /// Sibling of the peer per-`:membros` [`crate::Membro::nome`]
    /// (4a32abf) member-caixa `:nome` scalar accessor on the M3
    /// mesh-slot surface — same "one typed dispatch on the substrate
    /// primitive, thin projections at each consumer" discipline extended
    /// onto the M2 supervisor-tree per-`:children` child-identity axis.
    /// The two typed axes (`Membro::nome` on the M3 Aplicacao side,
    /// `ChildSpec::nome` on the M2 Supervisor side) now share one
    /// accessor discipline for the shared substrate concept "another
    /// caixa referenced by `:nome`". Peer of the second M2 slot scalar
    /// accessor [`crate::UpgradeFromEntry::prior_versao`] (75d27a8) on
    /// the sibling per-`:upgrade-from :from` OTP-appup axis — the M2
    /// slot family's typed-accessor discipline now spans both the
    /// upgrade axis (`:upgrade-from`) and the supervision axis
    /// (`:children`), matching the closed M3 mesh-slot accessor family's
    /// shape. Named `nome()` to match the tatara-lisp author-surface
    /// term the field's docstring already reaches for ("The child
    /// caixa's `:nome`") and the peer [`crate::Membro::nome`] /
    /// [`crate::Caixa::nome`] / [`crate::dep::Dep::nome`] field-name
    /// discipline the substrate already carries — the accessor's name
    /// maps directly onto the canonical caixa-identity vocabulary rather
    /// than shadowing the field's storage-side `caixa` label.
    #[must_use]
    pub fn nome(&self) -> &str {
        self.caixa.as_str()
    }

    /// Substrate-canonical per-`:children` child-caixa `:versao` semver-
    /// requirement scalar accessor every consumer that reads the OTP-shape
    /// supervised child's version pin keys off — returns the author-declared
    /// `:children :versao` byte-string verbatim as a `&str`, borrowed from
    /// the typed slot's own [`String`] storage.
    ///
    /// The `:children :versao` slot carries the Cargo-shaped semver
    /// requirement string (`"^0.1"`, `"~0.1.2"`, `"0.1.0"`, `"*"`) that pins
    /// which release of the supervised child caixa the OTP-shape supervisor
    /// tree materializes against — the same requirement grammar the peer
    /// `:deps :versao` / `:membros :versao` axes carry, resolved through the
    /// shared [`crate::render::require_valid_versao_requirement`] cascade
    /// and the shared [`crate::version::parse_requirement`] parser. Every
    /// downstream consumer that fans on the child's version pin keys off
    /// this scalar (the [`SupervisorSpec::validate`] per-child requirement
    /// gate at `require_valid_versao_requirement(child.versao_requirement(),
    /// …)`, the [`SupervisorError::ChildVersaoInvalid`] variant's carrier
    /// for `feira lint` rendering, every future per-cluster version-lock
    /// overlay the caixa-operator's hierarchical reconciliation scheduler
    /// pins through a future `:placement`-scoped supervisor-tree slot, the
    /// future M4 `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's
    /// per-child version resolver, the future wasm-operator's per-child
    /// lacre BLAKE3-closure lookup at `ComputeUnit` materialization time).
    ///
    /// Prior to this lift the `.versao` byte-string was accessed inline at
    /// two `&str`-shaped sites in `caixa-core/src/supervisor.rs` — the
    /// [`SupervisorSpec::validate`] requirement-gate call
    /// `require_valid_versao_requirement(&child.versao, …)` and the
    /// [`SupervisorError::ChildVersaoInvalid`] carrier at
    /// `versao: child.versao.clone()` — two open-coded field-accesses that
    /// expressed no compile-time link back to the typed slot. A future
    /// extension of the `:children :versao` axis to a richer author surface
    /// (a per-cluster version-pin overlay per MESH-COMPOSITION §III.2 canary
    /// flow, a lacre-projected concrete-version rewrite the operator
    /// materializes at CR-admission time, a future `:children :versao-lock`
    /// per-cluster override slot the wasm-operator's hierarchical
    /// reconciliation scheduler authors per-CR) would have had to be
    /// threaded through both open-coded copies in lockstep or one consumer
    /// would silently disagree with the peer on which release constraint a
    /// given child resolves to — the requirement-gate call reading
    /// `"^0.1"` while the error-body carrier read `"tenant-a-pin/^0.1"`
    /// would silently split the `ChildVersaoInvalid` diagnostic quote from
    /// the actual gate rejection input, a two-consumer split at the
    /// validator far from the source `caixa.lisp` with no field naming the
    /// version-pin drift root cause. Lifting the resolution rule to a typed
    /// method on the substrate primitive means every downstream
    /// requirement-facing consumer of the Supervisor's per-`:children`
    /// version-pin surface reaches for exactly one typed dispatch — the
    /// resolver's accept-set migrates as a unit on any future axis addition.
    ///
    /// Sibling of the peer per-`:membros` [`crate::Membro::versao_requirement`]
    /// (a40b0e3) member-caixa `:versao` scalar accessor on the M3 mesh-slot
    /// surface — same "one typed dispatch on the substrate primitive, thin
    /// projections at each consumer" discipline extended onto the M2
    /// supervisor-tree per-`:children` child-version-pin axis. The two typed
    /// axes (`Membro::versao_requirement` on the M3 Aplicacao side,
    /// `ChildSpec::versao_requirement` on the M2 Supervisor side) now share
    /// one accessor discipline for the shared substrate concept "another
    /// caixa referenced by a Cargo-shaped semver requirement". Peer of the
    /// sibling per-`:children` [`ChildSpec::nome`] (57c61d0) child-caixa
    /// `:nome` scalar accessor — the pair
    /// `(nome(), versao_requirement())` jointly projects the
    /// `(caixa, versao)` field pair every OTP-shape supervisor-tree consumer
    /// that fans on per-child identity + version pin keys off, closing the
    /// last unlifted per-`:children` `String`-carry axis so every downstream
    /// per-`:children` reader now routes through a typed dispatch on the
    /// substrate primitive. Named `versao_requirement()` rather than
    /// `versao()` because the field's storage-side `.versao` label is
    /// already the author-surface term (`:versao`); the accessor's name
    /// carries the semantic role — the semver *requirement* string the
    /// shared [`crate::version::parse_requirement`] entry-point consumes —
    /// so a raw field access and a typed dispatch read differently at every
    /// consumer site. Matches the peer [`crate::Membro::versao_requirement`]
    /// naming discipline verbatim.
    #[must_use]
    pub fn versao_requirement(&self) -> &str {
        self.versao.as_str()
    }

    /// Substrate-canonical per-`:children` `:restart` OTP-shaped
    /// per-child post-exit restart-decision policy scalar accessor every
    /// consumer that dispatches on the supervised child's post-exit
    /// reconcile posture keys off — returns the author-declared
    /// `:children :restart` variant verbatim as a [`RestartPolicy`],
    /// `Copy`-projected from the typed slot's own [`RestartPolicy`]
    /// storage.
    ///
    /// The `:children :restart` slot carries the closed-set OTP-shaped
    /// per-child restart-decision policy discriminator
    /// ([`RestartPolicy::Permanent`] — always restart, the OTP `permanent`
    /// worker-child default; [`RestartPolicy::Transient`] — restart only
    /// on abnormal exit, the OTP `transient` clean-completion-aware
    /// default; [`RestartPolicy::Temporary`] — never restart, the OTP
    /// `temporary` one-shot default) that every downstream consumer of
    /// the Supervisor's per-child post-exit reconcile branch keys off.
    /// Every future downstream consumer that fans on the per-child
    /// restart-decision keys off this scalar (the future `feira app
    /// graph` per-child restart column, the future wasm-operator's
    /// per-child post-exit restart-decision branch, the future M4
    /// `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's per-child
    /// admission webhook, the `caixa-operator`'s hierarchical
    /// reconciliation scheduler's per-child post-exit reconcile branch,
    /// the [`RestartPolicy::as_str`] `Serialize`-derive-pinning path the
    /// [`tests::restart_policy_variants_serialize_to_lifted_scalar_values`]
    /// pin threads through).
    ///
    /// Peer of the sibling per-`:supervisor` [`SupervisorSpec::estrategia`]
    /// (eafb619) `Copy`-return [`RestartStrategy`] sibling-restart-strategy
    /// scalar accessor and the M3 mesh-slot
    /// [`crate::Placement::estrategia`] (921fe1b) `Copy`-return
    /// [`crate::PlacementStrategy`] distribution-strategy scalar accessor
    /// — same "one typed dispatch on the substrate primitive,
    /// `Copy`-projected closed-set enum-arm discriminator that partitions
    /// the downstream renderer's per-arm fan-out" discipline extended
    /// onto the M2 supervisor-slot per-`:children` restart-decision-policy
    /// `Copy`-composite-enum scalar axis. Third axis on the per-`:children`
    /// [`ChildSpec`] type — companion to the sibling per-`:children`
    /// [`ChildSpec::nome`] (57c61d0) child-caixa `:nome` scalar accessor
    /// and the per-`:children` [`ChildSpec::versao_requirement`]
    /// (2c053c8) child-caixa `:versao` semver-requirement scalar accessor
    /// on the sibling `String`-carry axes. The triple
    /// `(nome(), versao_requirement(), restart())` jointly projects the
    /// `(caixa, versao, restart)` field trio every OTP-shape supervisor-
    /// tree consumer that fans on per-child identity + version pin +
    /// restart-decision keys off, closing the last unlifted per-`:children`
    /// axis so every downstream per-`:children` reader now routes through
    /// a typed dispatch on the substrate primitive. Named `restart()` to
    /// match the storage field's name and the author-surface
    /// `:children :restart` slot term verbatim; the accessor's identity
    /// name maps onto the canonical OTP-shape per-child restart-decision-
    /// policy vocabulary the [`RestartPolicy`] enum's docstring already
    /// carries.
    #[must_use]
    pub fn restart(&self) -> RestartPolicy {
        self.restart
    }
}

/// Supervisor-typed slots that live alongside the standard Caixa
/// fields when `:kind Supervisor`. Held flat in [`crate::Caixa`] so
/// the manifest stays a single typed form; this struct exists for
/// validation + conversion.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorSpec {
    /// Restart strategy. Defaults to [`RestartStrategy::OneForOne`].
    #[serde(default)]
    pub estrategia: RestartStrategy,

    /// Max restarts within [`Self::restart_window`] before the
    /// supervisor itself terminates (and its parent supervisor decides
    /// what to do). Default 5.
    #[serde(default = "default_max_restarts")]
    pub max_restarts: u32,

    /// Sliding window for `max_restarts`. Authored as a duration
    /// string (`"60s"`, `"5m"`); absent = "never reset". A `Some(0s)`
    /// is rejected by [`Self::validate`] — Erlang/OTP's
    /// `MaxIntensity / Period` invariant requires a positive window
    /// (a zero-period supervisor either trips on the first failure or
    /// never trips, depending on operator interpretation, neither of
    /// which is the author's intent). Omit the slot to express "no
    /// reset"; carry a positive duration to express the sliding window.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "duration_codec"
    )]
    pub restart_window: Option<Duration>,

    /// Static children. Empty for `SimpleOneForOne` (children added
    /// dynamically); required for the other three strategies.
    #[serde(default)]
    pub children: Vec<ChildSpec>,
}

const fn default_max_restarts() -> u32 {
    5
}

/// Upper-bound ceiling on the `:supervisor :max-restarts` axis — every
/// validated [`SupervisorSpec::max_restarts`] past
/// [`SupervisorSpec::validate`] lies in `1..=SUPERVISOR_MAX_RESTARTS_MAX`.
///
/// The typed field is `u32` (the zero-floor arm
/// [`SupervisorError::ZeroMaxRestarts`] already brackets the bottom edge),
/// so a programmatic struct literal
/// (`SupervisorSpec { max_restarts: u32::MAX, .. }`) and the equivalent
/// author-surface form (`:max-restarts 4294967295` or any
/// `:max-restarts 100000`-shape typo landing in the slot) both round-trip
/// cleanly through serde — a structurally unbounded `u32` ceiling. The
/// runtime substrate consuming the value (Erlang/OTP's
/// `MaxIntensity / Period` ratio, the future wasm-operator's
/// per-supervisor restart-intensity counter, the M4
/// `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's admission webhook)
/// then turned a typed `:max-restarts` policy into a no-op supervisor: the
/// escalation threshold is structurally so high that no realistic
/// restarts-per-`:restart-window` traffic shape can reach it, the
/// supervisor never escalates to its parent, and a bad child can loop
/// inside the window indefinitely with the parent supervisor structurally
/// never receiving the "this subtree has exceeded its restart budget"
/// signal the typed slot is meant to express — the canonical
/// "supervisor intensity declared, no escalation" footgun, exactly the
/// peer of the [`crate::aplicacao::POLICY_BREAKER_MAX_FAILURES_MAX`] cap
/// on the `:politicas :circuit-breaker :max-failures` axis (both are
/// "trip the next-higher protection layer after N events in a rolling
/// window" counters with identical degenerate-at-the-high-end shape).
///
/// The `1000` ceiling matches the sibling
/// [`crate::aplicacao::POLICY_BREAKER_MAX_FAILURES_MAX`] (the closest
/// peer — same "events-per-window trip threshold" semantics, same `u32`
/// type, same no-op-at-the-high-end failure mode) so the M4
/// `mesh.pleme.io/v1alpha1/Supervisor` / `.../Aplicacao` CR materializers
/// and the future wasm-operator's per-supervisor restart-intensity
/// counter reach for either field knowing the value is in `1..=1000`
/// without re-validating at the reconciler layer. The cap sits two
/// orders of magnitude above every documented Erlang/OTP production
/// playbook recommendation (Learn You Some Erlang's
/// `{intensity, 5, 60}` worker-supervisor default, Elixir's `Supervisor`
/// `max_restarts: 3` default, OTP's `supervisor` callback module
/// `MaxR = 1` / `MaxT = 5` "minimal-restart" default, Riak Core's
/// typical `MaxR ∈ 5..=100`, RabbitMQ's broker-supervisor `MaxR = 5`
/// default) and below the clearly-pathological "effectively no
/// escalation" floor (`10_000`, `100_000`, `u32::MAX`): a value the
/// author can plausibly want at hyperscale (a long-running supervisor
/// over a very-flaky pool tolerating thousands of transient restarts
/// before escalating), but a hard wall above which the typed policy is
/// structurally a no-op carried verbatim on every emitted child-restart
/// reconciliation contract.
///
/// Lifted as a typed `pub const` so the bound has exactly one source of
/// truth — the future M4 `mesh.pleme.io/v1alpha1/Supervisor` CR
/// materializer's admission webhook and the wasm-operator-side
/// per-supervisor restart-intensity reconciler read from one place. Same
/// shape every other typed upper bound in this crate carries
/// ([`crate::aplicacao::POLICY_BREAKER_MAX_FAILURES_MAX`],
/// [`crate::aplicacao::POLICY_RETRIES_MAX`],
/// [`crate::aplicacao::POLICY_RATE_LIMIT_MAX`],
/// [`crate::LIMITS_MEMORY_WASM32_MAX_BYTES`],
/// [`crate::render::DNS_1123_LABEL_MAX_LEN`],
/// [`crate::render::NATS_SUBJECT_MAX_LEN`]).
pub const SUPERVISOR_MAX_RESTARTS_MAX: u32 = 1000;

/// Upper-bound ceiling on the `:supervisor :restart-window` axis —
/// every validated `Some(`[`SupervisorSpec::restart_window`]`)` past
/// [`SupervisorSpec::validate`] lies in `1ms..=SUPERVISOR_RESTART_WINDOW_MAX`
/// (inclusive on both ends, integer-millisecond magnitudes by the
/// canonical-form gate immediately preceding).
///
/// The typed field is `Option<Duration>` (the zero-floor arm
/// [`SupervisorError::RestartWindowZero`] already rejects
/// `Some(Duration::ZERO)`, and the canonical-form arm
/// [`SupervisorError::RestartWindowNotCanonical`] already rejects
/// sub-millisecond residue), so a programmatic struct literal
/// (`SupervisorSpec { restart_window: Some(Duration::from_secs(86_400)),
/// .. }` — 24h) and the equivalent author-surface form
/// (`(:supervisor (:restart-window "24h"))` — the shared duration codec
/// emits `"<n>h"` for any integer-hour magnitude) both round-trip
/// cleanly through serde — a structurally unbounded `Duration` ceiling.
/// A `:restart-window` value far above the documented Erlang/OTP
/// `MaxIntensity / Period` production-playbook band (Learn You Some
/// Erlang's `{intensity, 5, 60}` worker-supervisor `Period = 60s`
/// default, Elixir's `Supervisor` `max_seconds: 5` default, OTP's
/// `supervisor` callback module `MaxT = 5..=60` typical, Riak Core's
/// `MaxT ∈ 10s..=300s`, RabbitMQ broker-supervisor `MaxT = 5s` default)
/// degenerates the supervisor's restart-intensity counter into a
/// lifetime counter: the rolling failure-counting window is structurally
/// so long that transient restarts are never forgotten, so the
/// `MaxIntensity / Period` ratio degenerates from "trip the parent
/// supervisor when the child has exceeded its restart budget *within
/// the recent window*" to "trip the parent when the child has exceeded
/// its restart budget *over its lifetime*" — every transient restart
/// counts against the budget forever, the supervisor's reset semantic
/// never reaches the child, and the typed `:restart-window` slot
/// becomes a no-op rolling window carried on every emitted hierarchical
/// reconciliation contract. The canonical
/// rolling-window-degenerates-to-lifetime-counter footgun the sibling
/// [`crate::POLICY_BREAKER_WINDOW_MAX`] cap closes on the peer
/// `:politicas :circuit-breaker :window` axis with identical shape (both
/// are "rolling failure-counting window with a per-`Period` reset" Duration
/// axes whose lifetime-counter degenerate at the high end is the same
/// "the reset semantic never fires" CSE invariant violation).
///
/// The `1h` (3600s = `3_600_000` ms) ceiling matches the largest unit
/// the shared duration codec emits (`"<n>h"` for any integer-hour
/// magnitude) — every value in the canonical authoring form's
/// `<integer><unit>` grammar at or below this cap renders to a clean
/// canonical string — and matches the three sibling typed-`Duration`
/// caps already lifted to this surface
/// ([`crate::LIMITS_WALL_CLOCK_MAX`], [`crate::POLICY_TIMEOUT_MAX`],
/// [`crate::POLICY_BREAKER_WINDOW_MAX`]). All four typed-`Duration`
/// axes — per-process `:limits :wall-clock`, per-edge `:politicas
/// :timeout`, per-breaker `:politicas :circuit-breaker :window`, and
/// per-supervisor `:supervisor :restart-window` — now share a single
/// uniform top edge at the codec's largest emitted unit so the next
/// typed-slot wiring (the future wasm-operator's per-supervisor
/// `MaxIntensity / Period` reconciler, the M4
/// `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's admission
/// webhook, the `caixa-operator`'s hierarchical reconciliation
/// scheduler) reaches for any of the four knowing the value is in
/// `1ms..=1h` without re-validating at the renderer layer. The cap sits
/// two orders of magnitude above every documented Erlang/OTP / Elixir /
/// Riak Core / RabbitMQ production-playbook recommendation band
/// (`5s..=300s`) and below the clearly-pathological "rolling window
/// degenerates to lifetime counter" floor (`24h`, `7d`, `Duration::MAX`):
/// a value the author can plausibly want for a very-low-traffic
/// long-tail failure-restart window over a hyperscale-flaky child pool,
/// but a hard wall above which the rolling-window contract is
/// structurally a lifetime-counter contract.
///
/// Lifted as a typed `pub const` so the bound has exactly one source
/// of truth — the future M4 `mesh.pleme.io/v1alpha1/Supervisor` CR
/// materializer's admission webhook, the wasm-operator-side
/// per-supervisor `MaxIntensity / Period` reconciler, and the
/// `caixa-operator`'s hierarchical reconciliation scheduler all read
/// from one place. Same shape every other typed upper bound in this
/// crate carries ([`SUPERVISOR_MAX_RESTARTS_MAX`],
/// [`crate::aplicacao::POLICY_BREAKER_MAX_FAILURES_MAX`],
/// [`crate::aplicacao::POLICY_RETRIES_MAX`],
/// [`crate::aplicacao::POLICY_RATE_LIMIT_MAX`],
/// [`crate::LIMITS_MEMORY_WASM32_MAX_BYTES`],
/// [`crate::LIMITS_WALL_CLOCK_MAX`], [`crate::POLICY_TIMEOUT_MAX`],
/// [`crate::POLICY_BREAKER_WINDOW_MAX`],
/// [`crate::render::DNS_1123_LABEL_MAX_LEN`],
/// [`crate::render::NATS_SUBJECT_MAX_LEN`]).
pub const SUPERVISOR_RESTART_WINDOW_MAX: Duration = Duration::from_secs(3600);

impl Default for SupervisorSpec {
    fn default() -> Self {
        Self {
            estrategia: RestartStrategy::default(),
            max_restarts: default_max_restarts(),
            restart_window: Some(Duration::from_secs(60)),
            children: Vec::new(),
        }
    }
}

impl SupervisorSpec {
    /// Substrate-canonical per-`:supervisor` `:estrategia` OTP-shaped
    /// sibling-restart-strategy scalar accessor every consumer that
    /// dispatches on the supervisor's per-sibling restart-decision shape
    /// keys off — returns the author-declared `:supervisor :estrategia`
    /// variant verbatim as a [`RestartStrategy`], `Copy`-projected from
    /// the typed slot's own [`RestartStrategy`] storage.
    ///
    /// The `:supervisor :estrategia` slot carries the closed-set
    /// OTP-shaped sibling-restart-strategy discriminator ([`RestartStrategy::OneForOne`]
    /// — restart only the failed child, the Erlang/OTP `one_for_one` default;
    /// [`RestartStrategy::OneForAll`] — restart every child on any child
    /// failure, the Erlang/OTP `one_for_all` shared-state cohort default;
    /// [`RestartStrategy::RestForOne`] — restart the failed child and
    /// every child started after it, the Erlang/OTP `rest_for_one`
    /// startup-order default; [`RestartStrategy::SimpleOneForOne`] —
    /// dynamic children of the same shape, the Erlang/OTP
    /// `simple_one_for_one` per-session default) that every downstream
    /// consumer of the Supervisor's per-sibling restart-decision fan-out
    /// shape keys off. Validated by [`SupervisorSpec::validate`] to be
    /// paired coherently with the sibling `:children` axis
    /// (`SimpleOneForOne ↔ children.is_empty()` — the cross-slot
    /// partition the strategy-arm's [`SupervisorError::SimpleOneForOneWithStaticChildren`]
    /// / [`SupervisorError::NoChildren`] refusal cascade pins), and every
    /// downstream consumer that reads the strategy keys off this scalar
    /// (the [`SupervisorSpec::validate`] `SimpleOneForOne ↔ non-SimpleOneForOne`
    /// partition-dispatch `match` arm, the non-`SimpleOneForOne`-arm
    /// declared-but-empty [`SupervisorError::NoChildren`] error carrier's
    /// `estrategia:` field, the future `feira app graph` per-Supervisor
    /// strategy print line, the future wasm-operator's per-supervisor
    /// sibling-restart-strategy branch, the future M4
    /// `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's per-strategy
    /// admission-webhook resolver, the `caixa-operator`'s hierarchical
    /// reconciliation scheduler's per-strategy fan-out).
    ///
    /// Prior to this lift the `.estrategia` field was accessed inline at
    /// two production sites in `caixa-core/src/supervisor.rs` — the
    /// [`SupervisorSpec::validate`] `SimpleOneForOne ↔ non-SimpleOneForOne`
    /// `match self.estrategia { … }` partition dispatch, and the
    /// non-`SimpleOneForOne`-arm [`SupervisorError::NoChildren`] error
    /// carrier at `estrategia: self.estrategia` — two open-coded
    /// field-accesses that expressed no compile-time link back to the
    /// typed slot. A future extension of the `:supervisor :estrategia`
    /// axis to a richer author surface (a per-cluster strategy override
    /// the operator pins through a future `:supervisor :estrategia-overrides`
    /// slot the MESH-COMPOSITION §III.2 supervision-canary roadmap
    /// acknowledges, a per-tenant strategy-alias table the M4 CR
    /// materializer resolves per-CR, a per-Supervisor dynamic strategy
    /// derivation the future adaptive-supervision engine computes from
    /// child-failure-history topology, a per-child-cohort strategy split
    /// the future `RestForCohort` extension acknowledged by the
    /// INSPIRATIONS.md §II.2 Erlang/OTP absorption roadmap acknowledges)
    /// would have had to be threaded through every open-coded copy in
    /// lockstep — one consumer reading the raw variant while a peer read
    /// the operator-resolved variant would silently split the
    /// [`SupervisorError::NoChildren`] diagnostic's quoted strategy from
    /// the actual partition-dispatch input the empty-children refusal
    /// arm reached under, a two-consumer split at the validator far from
    /// the source `caixa.lisp` with no field naming the strategy-drift
    /// root cause. Lifting the resolution rule to a typed method on the
    /// substrate primitive means every downstream consumer of the
    /// Supervisor's per-`:supervisor` sibling-restart-strategy surface
    /// reaches for exactly one typed dispatch — the resolver's accept-set
    /// migrates as a unit on any future axis addition.
    ///
    /// Peer of the sibling M3 mesh-slot [`crate::Placement::estrategia`]
    /// (921fe1b) `Copy`-return `PlacementStrategy` scalar accessor on the
    /// per-`:placement` distribution-strategy axis — same "one typed
    /// dispatch on the substrate primitive, thin projections at each
    /// consumer" discipline extended onto the M2 supervisor-slot
    /// per-`:supervisor` sibling-restart-strategy `Copy`-composite-enum
    /// scalar axis. The two typed axes (`Placement::estrategia` on the
    /// M3 Aplicacao side, `SupervisorSpec::estrategia` on the M2
    /// Supervisor side) now share one accessor discipline for the shared
    /// substrate concept "a `Copy`-projected closed-set enum-arm
    /// discriminator that partitions the downstream renderer's per-arm
    /// fan-out". First `Copy`-return accessor on the M2 supervisor-slot
    /// `SupervisorSpec` type — companion to the sibling per-`:children`
    /// [`crate::ChildSpec::nome`] (57c61d0) /
    /// [`crate::ChildSpec::versao_requirement`] (2c053c8) child-caixa
    /// scalar accessors on the sibling per-`:children` `String`-carry
    /// axes. Named `estrategia()` to match the storage field's name and
    /// the peer [`crate::Placement::estrategia`] method-name discipline
    /// verbatim; the accessor's identity name maps onto the canonical
    /// OTP-shape supervision vocabulary the [`RestartStrategy`] enum's
    /// docstring already carries.
    #[must_use]
    pub fn estrategia(&self) -> RestartStrategy {
        self.estrategia
    }

    /// Substrate-canonical per-`:supervisor` `:max-restarts` OTP-shaped
    /// `MaxIntensity` restart-budget scalar accessor every consumer that
    /// reads the supervisor's per-`:restart-window` restart-budget count
    /// keys off — returns the author-declared `:supervisor :max-restarts`
    /// typed `u32` verbatim, `Copy`-projected from the typed slot's own
    /// `u32` storage (`u32` is `Copy`, so the accessor returns by value; no
    /// borrow of `&self` past the call). Non-optional (the `u32` field
    /// carries the restart-budget count as a required axis with a
    /// [`default_max_restarts`]-supplied default; the zero-floor arm
    /// [`SupervisorError::ZeroMaxRestarts`] and the cap arm
    /// [`SupervisorError::MaxRestartsExceedsCap`] jointly bracket the
    /// accept-set to `1..=SUPERVISOR_MAX_RESTARTS_MAX`).
    ///
    /// The `:supervisor :max-restarts` slot carries the Erlang/OTP
    /// `MaxIntensity` restart-budget count that pairs with the sibling
    /// `:restart-window` `Period` to form the `MaxIntensity / Period`
    /// restart-intensity ratio the supervisor trips its own escalation on
    /// (`theory/RUNTIME-PATTERNS.md` §II.2, Learn You Some Erlang's
    /// `{intensity, 5, 60}` worker-supervisor default). Every downstream
    /// consumer of the Supervisor's per-`:supervisor` restart-budget count
    /// keys off this scalar (the [`SupervisorSpec::validate`] zero-floor +
    /// upper-cap bracket at
    /// `require_positive_bounded_u32(self.max_restarts(), …)`, the future
    /// wasm-operator's per-supervisor restart-intensity counter's
    /// budget-vs-count comparator, the future M4
    /// `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's admission
    /// webhook, the `caixa-operator`'s hierarchical reconciliation
    /// scheduler's per-supervisor escalation-decision branch, every
    /// `SupervisorError::MaxRestartsExceedsCap` variant carrying the
    /// offending count verbatim for `feira lint` rendering).
    ///
    /// Prior to this lift the `.max_restarts` field was accessed inline at
    /// one production site in `caixa-core/src/supervisor.rs` — the
    /// [`SupervisorSpec::validate`] `require_positive_bounded_u32(self
    /// .max_restarts, …)` bracket-gate call — one open-coded field-access
    /// that expressed no compile-time link back to the typed slot. A
    /// future extension of the `:max-restarts` axis to a richer author
    /// surface (a per-cluster restart-budget override the operator pins
    /// through a future `:supervisor :max-restarts-overrides` slot the
    /// MESH-COMPOSITION §III.2 supervision-canary roadmap acknowledges,
    /// a per-tenant restart-budget-alias table the M4 CR materializer
    /// resolves per-CR, a per-supervisor dynamic restart-budget derivation
    /// the future adaptive-supervision engine computes from child-failure-
    /// history topology, a promotion of the plain `u32` count to a richer
    /// `{MaxR, MaxT}` tuple once Erlang/OTP's per-child-cohort restart-
    /// budget-partition slot comes into scope) would have had to be
    /// threaded through every open-coded copy in lockstep or the validate
    /// gate and the future M4 emit path would silently disagree on which
    /// restart-budget count a given supervisor resolves to — an author's
    /// `:max-restarts 5` would satisfy validate while the emit path
    /// silently read a drifted other value (a `:max-restarts 10000`
    /// no-op supervisor at the emit boundary would carry the author's
    /// declared `5` verbatim in `feira lint` output while the future
    /// wasm-operator's restart-intensity counter operated under the
    /// drifted count), a two-consumer split at the validator far from the
    /// source `caixa.lisp` with no field naming the restart-budget-drift
    /// root cause. Lifting the resolution rule to a typed method on the
    /// substrate primitive means every downstream consumer of the
    /// Supervisor's per-`:supervisor` restart-budget-count surface reaches
    /// for exactly one typed dispatch — the resolver's accept-set migrates
    /// as a unit on any future axis addition.
    ///
    /// Peer of the sibling M3 mesh-slot [`crate::CircuitBreaker::max_failures`]
    /// (3a74062) `Copy`-return `u32` sub-struct required-scalar accessor
    /// on the per-`:politicas :circuit-breaker :max-failures` Envoy-
    /// outlier-detection trip-threshold axis — same "one typed dispatch on
    /// the substrate primitive, thin projections at each consumer"
    /// discipline extended onto the M2 supervisor-slot per-`:supervisor`
    /// restart-budget-count `Copy`-`u32` scalar axis. The two typed axes
    /// (`CircuitBreaker::max_failures` on the M3 Aplicacao side,
    /// `SupervisorSpec::max_restarts` on the M2 Supervisor side) now share
    /// one accessor discipline for the shared substrate concept "a
    /// `Copy`-projected required `u32` count that trips the next-higher
    /// protection layer after N events in a rolling window" — both are
    /// counters with identical degenerate-at-the-high-end shape and share
    /// the paired [`crate::POLICY_BREAKER_MAX_FAILURES_MAX`] /
    /// [`SUPERVISOR_MAX_RESTARTS_MAX`] `1000` cap. Second `Copy`-return
    /// accessor on the M2 supervisor-slot `SupervisorSpec` type, sibling
    /// to the [`SupervisorSpec::estrategia`] (eafb619) `Copy`-composite-
    /// enum `RestartStrategy` accessor. Named `max_restarts()` to match
    /// the storage field's name verbatim and the peer
    /// [`crate::CircuitBreaker::max_failures`] method-name discipline; the
    /// accessor's identity maps onto the canonical OTP-shape supervision
    /// vocabulary the [`SupervisorSpec::max_restarts`] field's docstring
    /// already carries.
    #[must_use]
    pub const fn max_restarts(&self) -> u32 {
        self.max_restarts
    }

    /// Substrate-canonical per-`:supervisor` `:restart-window` OTP-shaped
    /// `Period` sliding-window scalar accessor every consumer of the
    /// supervisor's `MaxIntensity / Period` restart-intensity denominator
    /// keys off — returns the author-declared `:supervisor :restart-window`
    /// typed [`Duration`] verbatim as an `Option<Duration>`, copied out of
    /// the typed slot's own `Option<Duration>` storage (`Duration` is
    /// `Copy`, so `Option<Duration>` is `Copy` and the accessor returns by
    /// value; no borrow of `&self` past the call). `None` when the slot is
    /// absent (the canonical "never reset — every restart across the
    /// supervisor's lifetime counts against the sibling `:max-restarts`
    /// budget" sentinel the field's own docstring names and the peer
    /// `validate_accepts_none_restart_window` pin locks in on the
    /// [`SupervisorSpec::validate`] entry-side).
    ///
    /// The `:supervisor :restart-window` slot carries the Erlang/OTP
    /// `Period` sliding-observation-interval that pairs with the sibling
    /// `:max-restarts` `MaxIntensity` restart-budget count to form the
    /// `MaxIntensity / Period` restart-intensity ratio the supervisor
    /// trips its own escalation on (`theory/RUNTIME-PATTERNS.md` §II.2,
    /// Learn You Some Erlang's `{intensity, 5, 60}` worker-supervisor
    /// default). The typed slot's `Option<Duration>` accept-set —
    /// zero-floor rejected through [`SupervisorError::RestartWindowZero`]
    /// (Erlang/OTP's `MaxIntensity / Period` invariant requires
    /// `Period > 0`; a zero period either trips on the first failure or
    /// never trips depending on operator interpretation, neither of which
    /// is the author's intent — omit the slot to express "no reset";
    /// carry a positive duration to express the sliding window),
    /// integer-millisecond canonical form enforced through
    /// [`SupervisorError::RestartWindowNotCanonical`] (the duration
    /// codec's canonical form emits `"1500ms"` not `"1.5s"` and the
    /// future wasm-operator's per-supervisor restart-intensity counter
    /// quantizes at milliseconds), upper-bounded by
    /// [`SUPERVISOR_RESTART_WINDOW_MAX`] (1h — the coarsest per-
    /// supervisor rolling window any operationally-reachable supervisor
    /// can honor without spanning multiple scheduler epochs the
    /// hierarchical-reconciliation scheduler treats as independent) —
    /// maps onto the future wasm-operator (M3) per-supervisor
    /// restart-intensity counter's rolling-observation-interval, the
    /// future M4 `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's
    /// per-`spec.restartWindow` admission webhook, and the sibling
    /// `duration_codec`-serialized wire scalar every downstream consumer
    /// of the supervisor's per-`:supervisor` restart-intensity denominator
    /// keys off.
    ///
    /// Prior to this lift the `.restart_window` field was accessed inline
    /// at one production site in `caixa-core/src/supervisor.rs` — the
    /// [`SupervisorSpec::validate`] `if let Some(w) = self.restart_window {
    /// … }` zero-floor + canonical-form + upper-cap bracket arm — one
    /// open-coded field-access that expressed no compile-time link back to
    /// the typed slot. A future extension of the `:restart-window` axis to
    /// a richer author surface (a per-cluster restart-window override the
    /// operator pins through a future `:supervisor :restart-window-overrides`
    /// slot the MESH-COMPOSITION §III.2 supervision-canary roadmap
    /// acknowledges, a per-tenant restart-window-alias table the M4 CR
    /// materializer resolves per-CR, a per-supervisor dynamic
    /// restart-window derivation the future adaptive-supervision engine
    /// computes from child-failure-history topology, a promotion of the
    /// plain `Option<Duration>` window to a richer `{observation, cooldown}`
    /// pair once Erlang/OTP's per-child-cohort observation-interval-
    /// partition slot comes into scope) would have had to be threaded
    /// through every open-coded copy in lockstep or the validate gate and
    /// the future M4 emit path would silently disagree on which
    /// restart-window a given supervisor resolves to — an author's
    /// `:restart-window "60s"` would satisfy validate while the emit path
    /// silently read a drifted other value (a `Some(Duration::from_secs(60))`
    /// authored slot at the emit boundary would carry the author's
    /// declared window verbatim in `feira lint` output while the future
    /// wasm-operator's restart-intensity counter operated under a
    /// drifted window, or vice versa: an author's `:restart-window ()`
    /// would carry the "never reset" sentinel through validate while the
    /// emit path silently substituted a default sliding window), a
    /// two-consumer split at the validator far from the source
    /// `caixa.lisp` with no field naming the restart-window-drift root
    /// cause. Lifting the resolution rule to a typed method on the
    /// substrate primitive means every downstream consumer of the
    /// Supervisor's per-`:supervisor` restart-intensity-denominator
    /// surface reaches for exactly one typed dispatch — the resolver's
    /// accept-set migrates as a unit on any future axis addition.
    ///
    /// Third `Copy`-return accessor on the M2 supervisor-slot
    /// `SupervisorSpec` type, closing the last unlifted per-`:supervisor`
    /// scalar-value axis (`children: Vec<ChildSpec>` carries a `Vec`
    /// payload rather than a `Copy`-scalar, and the per-`:children`
    /// [`crate::ChildSpec::nome`] (57c61d0) /
    /// [`crate::ChildSpec::versao_requirement`] (2c053c8) child-caixa
    /// scalar accessors already close the per-element `String`-carry
    /// axes). Sibling to the peer M2 [`crate::LimitsSpec::wall_clock`]
    /// (8cb717b) `Option<Duration>` accessor on the `:limits` slot's
    /// per-outermost-call wall-clock-deadline axis and the peer M3
    /// [`crate::MeshPolicy::timeout`] (7073d0f) `Option<Duration>`
    /// accessor on the `:politicas` slot's per-call-deadline axis — all
    /// three share the shared substrate concept "a `Copy`-projected
    /// optional `Duration` that carries a positive integer-millisecond
    /// canonical value with a `1ms..=<axis-specific>_MAX` accept-set and
    /// the paired zero-floor / non-canonical / above-cap refusal cascade"
    /// through the same [`crate::render::require_positive_canonical_bounded_duration`]
    /// bracket-helper the three axes each route through. Named
    /// `restart_window()` to match the storage field's name verbatim and
    /// the peer [`crate::LimitsSpec::wall_clock`] /
    /// [`crate::MeshPolicy::timeout`] method-name discipline; the
    /// accessor's identity maps onto the canonical OTP-shape supervision
    /// vocabulary the [`SupervisorSpec::restart_window`] field's docstring
    /// already carries.
    #[must_use]
    pub const fn restart_window(&self) -> Option<Duration> {
        self.restart_window
    }

    /// Substrate-canonical per-`:supervisor` `:children` OTP-shaped
    /// static-child-list slice accessor every consumer that walks the
    /// supervisor's declared child set keys off — returns the author-
    /// declared `:supervisor :children` `Vec<ChildSpec>` verbatim as a
    /// `&[ChildSpec]` slice-view, borrowed from the typed slot's own
    /// `Vec<ChildSpec>` storage (a zero-copy slice-view over the same
    /// backing buffer the `Serialize`/`Deserialize` derives round-trip
    /// through). Non-optional: an empty slice is the load-bearing
    /// "author declared `:children ()`" sentinel every consumer of the
    /// cross-slot `SimpleOneForOne ↔ children.is_empty()` partition
    /// keys off (`SimpleOneForOne` requires the empty slice; the peer
    /// three strategies require a non-empty slice — the paired
    /// [`SupervisorError::SimpleOneForOneWithStaticChildren`] /
    /// [`SupervisorError::NoChildren`] refusal cascade pins the
    /// partition on both arms).
    ///
    /// The `:supervisor :children` slot carries the OTP-shaped static
    /// child list the supervisor materializes one ComputeUnit per
    /// entry from — the Erlang/OTP `supervisor:init/1`'s
    /// `{ok, {SupFlags, ChildSpecs}}` `ChildSpecs` list, projected
    /// through the tatara-lisp `:children` author surface onto a typed
    /// `Vec<ChildSpec>` whose per-element `(nome(),
    /// versao_requirement(), restart)` triple the per-child
    /// [`SupervisorSpec::validate`] loop already gates through the
    /// lifted [`ChildSpec::nome`] (57c61d0) /
    /// [`ChildSpec::versao_requirement`] (2c053c8) scalar accessors.
    /// Every downstream consumer that fans on the static child list
    /// keys off this slice (the [`SupervisorSpec::validate`]
    /// `SimpleOneForOne ↔ non-SimpleOneForOne` partition dispatch's
    /// `.is_empty()` probe on both arms, the [`SupervisorSpec::validate`]
    /// per-child DNS-1123 / semver-requirement / duplicate-detection
    /// fan-out loop, every future wasm-operator (M3) per-supervisor
    /// hierarchical-reconciliation scheduler's per-child ComputeUnit
    /// materialization loop, the future M4
    /// `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's per-child
    /// admission-webhook fan-out, the future `feira app graph`
    /// per-supervisor tree-print traversal).
    ///
    /// Prior to this lift the `.children` `Vec<ChildSpec>` was accessed
    /// inline at three production sites in `caixa-core/src/supervisor.rs`
    /// — the [`SupervisorSpec::validate`] `SimpleOneForOne`-arm
    /// `!self.children.is_empty()` cross-slot refusal probe, the peer
    /// non-`SimpleOneForOne`-arm `self.children.is_empty()`
    /// [`SupervisorError::NoChildren`] refusal probe, and the per-child
    /// validate loop's `for child in &self.children` traversal head —
    /// three open-coded field-accesses that expressed no compile-time
    /// link back to the typed slot. A future extension of the
    /// `:supervisor :children` axis to a richer author surface (a
    /// per-cluster child-set overlay the operator pins through a future
    /// `:supervisor :children-overrides` slot the MESH-COMPOSITION §III.2
    /// supervision-canary roadmap acknowledges, a per-tenant
    /// child-set-alias table the M4 CR materializer resolves per-CR,
    /// a per-supervisor dynamic-child derivation the future adaptive-
    /// supervision engine computes from child-failure-history topology,
    /// a promotion of the plain `Vec<ChildSpec>` to a richer
    /// `{static, dynamic}` partition once Erlang/OTP's
    /// `simple_one_for_one` dynamic-child slot comes into typed scope)
    /// would have had to be threaded through all three open-coded copies
    /// in lockstep or one consumer would silently disagree with the
    /// peers on which child-set a given supervisor resolves to — the
    /// `SimpleOneForOne`-arm probe reading the raw slot while the peer
    /// non-`SimpleOneForOne`-arm probe read an operator-resolved slot
    /// would silently split the partition-dispatch's two-arm coherence
    /// (a supervisor that satisfies neither arm's precondition, or that
    /// satisfies both, at the cost of the paired
    /// `SimpleOneForOneWithStaticChildren`/`NoChildren` refusal cascade
    /// silently drifting from the per-child validate loop's actual
    /// traversal input), a three-consumer split at the validator far
    /// from the source `caixa.lisp` with no field naming the
    /// child-set-drift root cause. Lifting the resolution rule to a
    /// typed method on the substrate primitive means every downstream
    /// consumer of the Supervisor's per-`:supervisor` static-child-list
    /// surface reaches for exactly one typed dispatch — the resolver's
    /// accept-set migrates as a unit on any future axis addition.
    ///
    /// First slice-return (`&[T]`) accessor on any M2 or M3 typed slot
    /// — the seed for the same "one typed dispatch on the substrate
    /// primitive, thin projections at each consumer" discipline the
    /// closed [`crate::LimitsSpec`] / [`BehaviorSpec`] /
    /// [`crate::UpgradeFromEntry`] scalar-accessor families each carry
    /// on their `Copy` / `Option<Copy>` / `Option<&str>` axes, extended
    /// onto the first `Vec`-carry axis on the substrate. The four peer
    /// `Vec`-carry axes still unlifted at the time of this seed —
    /// [`crate::Placement::clusters`] (`Vec<String>` per-cluster
    /// distribution-target list), [`crate::AplicacaoSpec::membros`]
    /// (`Vec<Membro>` per-Aplicacao member list),
    /// [`crate::AplicacaoSpec::contratos`] (`Vec<WitContract>`
    /// per-Aplicacao WIT-typed edge list),
    /// [`crate::UpgradeFromEntry::instructions`]
    /// (`Vec<UpgradeInstruction>` per-appup migration-instruction list)
    /// — inherit this accessor's discipline as future compounding runs
    /// migrate their consumers onto the shared slice-return shape.
    /// Fourth (and final) accessor on the M2 supervisor-slot
    /// `SupervisorSpec` type, sibling to the three `Copy`-return
    /// [`SupervisorSpec::estrategia`] (eafb619) /
    /// [`SupervisorSpec::max_restarts`] (7844f4e) /
    /// [`SupervisorSpec::restart_window`] (7e7b32f) accessors — closes
    /// the last unlifted per-`:supervisor` field axis (the
    /// `Vec<ChildSpec>` static-child-list carrier) so every downstream
    /// per-`:supervisor` reader now routes through a typed dispatch on
    /// the substrate primitive. Named `children()` to match the storage
    /// field's name verbatim and the tatara-lisp author-surface term
    /// (`:children`) the field's own docstring already carries; the
    /// accessor's identity maps onto the canonical OTP-shape
    /// supervision vocabulary the [`SupervisorSpec::children`] field's
    /// docstring already reaches for ("Static children ..."). Returns
    /// `&[ChildSpec]` (not `&Vec<ChildSpec>`) because every downstream
    /// consumer of the child list treats it as a read-only sequence —
    /// the slice-view is the narrowest borrow that supports every
    /// present + roadmapped consumer (`.is_empty()`, `.iter()`,
    /// index, `.len()`) without leaking the backing `Vec`'s
    /// grow/push/reserve surface that no consumer of the typed view
    /// reaches for (the storage-side `Vec` remains reachable through
    /// the `pub children` field for the mutation-carrying
    /// `Caixa::supervisor_view` fold-in path in
    /// `manifest.rs:supervisor_view`).
    #[must_use]
    pub fn children(&self) -> &[ChildSpec] {
        self.children.as_slice()
    }

    /// Validate the supervisor's typed shape — strategy ↔ children
    /// invariants, max_restarts > 0, restart_window > 0 when set,
    /// per-child non-empty + duplicate-free names.
    ///
    /// Mirrors the value-shape discipline applied to every other
    /// typed slot:
    ///
    ///   - `Some(Duration::ZERO)` on a Duration-bearing axis is the
    ///     same "0 means the opposite of what you think" footgun
    ///     closed for `:politicas :timeout` (Envoy interprets a zero
    ///     timeout as `infinite`), `:politicas :circuit-breaker
    ///     :window`, and `:limits :wall-clock`. The
    ///     `MaxIntensity / Period` ratio in Erlang/OTP's
    ///     `supervisor` requires `Period > 0`; a zero period either
    ///     trips on the first failure or never trips depending on
    ///     operator interpretation, neither of which is the
    ///     author's intent. Omit `:restart-window` to express "no
    ///     reset"; carry a positive duration to express the window.
    ///   - duplicate `:children` `:caixa` names are the same
    ///     graph-node-set / multiset distinction closed for
    ///     `:membros` (4bb3f3d), `:placement :clusters` (c7c7799),
    ///     and `:entrada :paths` (eb3456d). Two children with the
    ///     same `:caixa` materialize as two ComputeUnits with the
    ///     same name in the cluster's HelmRelease values, one
    ///     silently overwriting the other. Erlang/OTP's
    ///     `child_spec.id` is required-unique per supervisor;
    ///     pleme-io enforces the same set-not-multiset shape on
    ///     `:caixa` (the load-bearing identity in our renderer).
    pub fn validate(&self) -> Result<(), SupervisorError> {
        // Route the `SimpleOneForOne ↔ non-SimpleOneForOne` partition
        // dispatch and the non-`SimpleOneForOne`-arm [`SupervisorError::NoChildren`]
        // error carrier's `estrategia:` field through the lifted
        // [`SupervisorSpec::estrategia`] accessor rather than the raw
        // `self.estrategia` field access — the two production consumers
        // of the per-`:supervisor` sibling-restart-strategy scalar now
        // key off exactly one typed dispatch on the substrate primitive,
        // so any future rebrand on the axis (a per-cluster strategy
        // override the operator pins through a future `:supervisor
        // :estrategia-overrides` slot, a per-tenant strategy-alias table
        // the M4 CR materializer resolves per-CR) migrates as a single
        // caixa-core edit rather than a coordinated rewrite of the two
        // call sites — sibling of the peer M3 [`crate::Placement::estrategia`]
        // (921fe1b) four-consumer migration on the per-`:placement`
        // distribution-strategy axis.
        // Route the `SimpleOneForOne ↔ non-SimpleOneForOne` partition-
        // dispatch's paired `.is_empty()` cross-slot refusal probes
        // (the `SimpleOneForOne`-arm
        // [`SupervisorError::SimpleOneForOneWithStaticChildren`] refusal
        // and the non-`SimpleOneForOne`-arm [`SupervisorError::NoChildren`]
        // refusal) through the lifted [`SupervisorSpec::children`]
        // slice-return accessor rather than the raw `self.children`
        // field access — the two paired production consumers of the
        // per-`:supervisor` static-child-list scalar-shape now key off
        // exactly one typed dispatch on the substrate primitive, so any
        // future rebrand on the axis (a per-cluster child-set overlay
        // the operator pins through a future `:supervisor
        // :children-overrides` slot, a per-tenant child-set-alias table
        // the M4 CR materializer resolves per-CR) migrates as a single
        // caixa-core edit rather than a coordinated rewrite of the
        // paired arms — first slice-return migration on any typed slot,
        // seed for the peer per-`:placement :clusters`,
        // per-`:membros`, per-`:contratos`, and per-`:upgrade-from
        // :instructions` `Vec`-carry axes.
        match self.estrategia() {
            RestartStrategy::SimpleOneForOne => {
                // SimpleOneForOne: children added at runtime. Static
                // list must be empty (one shape declared elsewhere).
                if !self.children().is_empty() {
                    return Err(SupervisorError::SimpleOneForOneWithStaticChildren);
                }
            }
            _ => {
                if self.children().is_empty() {
                    return Err(SupervisorError::NoChildren {
                        estrategia: self.estrategia(),
                    });
                }
            }
        }
        // Zero-floor + upper-cap bracket on the typed `:max-restarts`
        // axis. See [`crate::render::require_positive_bounded_u32`] for
        // the ordering discipline (zero-floor arm strictly precedes cap
        // arm so `0` surfaces the self-locating `ZeroMaxRestarts`
        // diagnostic with its counter-axis remediation directly named,
        // not the misleading `0 > SUPERVISOR_MAX_RESTARTS_MAX == false`
        // cap-arm miss). Until this bracket landed the top edge ran all
        // the way to `u32::MAX` and a struct-literal
        // `SupervisorSpec { max_restarts: 100_000, .. }` (or the
        // equivalent author-surface `:max-restarts 100000` /
        // `:max-restarts 4294967295` typo landing in the slot) silently
        // passed validate. The runtime substrate consuming the value
        // (Erlang/OTP's `MaxIntensity / Period` ratio, the future
        // wasm-operator's per-supervisor restart-intensity counter, the
        // M4 `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's
        // admission webhook) then turned a typed `:max-restarts`
        // policy into a no-op supervisor: the escalation threshold is
        // structurally so high that no realistic
        // restarts-per-`:restart-window` traffic shape can reach it,
        // the supervisor never escalates to its parent, and a bad
        // child can loop inside the window indefinitely with the
        // parent supervisor structurally never receiving the "this
        // subtree has exceeded its restart budget" signal the typed
        // slot is meant to express. The bracket set is
        // `1..=SUPERVISOR_MAX_RESTARTS_MAX`, peer with the
        // [`crate::aplicacao::POLICY_BREAKER_MAX_FAILURES_MAX`] cap on
        // the sibling `:politicas :circuit-breaker :max-failures` axis:
        // both are "trip the next-higher protection layer after N
        // events in a rolling window" counters with identical
        // degenerate-at-the-high-end shape and now share one canonical
        // bracket helper. The bracket precedes the sibling
        // `:restart-window` zero-floor / canonical-millisecond arms so
        // an over-cap `max_restarts` paired with a structurally invalid
        // window surfaces the bracket diagnostic first, mirroring the
        // `PolicyBreakerMaxFailuresExceedsCap` / window-axis cross-arm
        // ordering on the peer `:politicas :circuit-breaker` slot.
        // Route the [`SupervisorSpec::validate`] `:max-restarts` zero-floor +
        // upper-cap bracket-gate through the lifted [`SupervisorSpec::max_restarts`]
        // accessor rather than the raw `self.max_restarts` field access —
        // the one production consumer of the per-`:supervisor`
        // restart-budget-count scalar now keys off exactly one typed
        // dispatch on the substrate primitive, so any future rebrand on
        // the axis (a per-cluster restart-budget override the operator
        // pins through a future `:supervisor :max-restarts-overrides`
        // slot, a per-tenant restart-budget-alias table the M4 CR
        // materializer resolves per-CR) migrates as a single caixa-core
        // edit rather than a coordinated rewrite — sibling of the peer M3
        // [`crate::CircuitBreaker::max_failures`] (3a74062) migration on
        // the per-`:politicas :circuit-breaker :max-failures` axis.
        crate::render::require_positive_bounded_u32(
            self.max_restarts(),
            SUPERVISOR_MAX_RESTARTS_MAX,
            || SupervisorError::ZeroMaxRestarts,
            |max_restarts| SupervisorError::MaxRestartsExceedsCap { max_restarts },
        )?;
        // Route the [`SupervisorSpec::validate`] `:restart-window`
        // zero-floor + integer-millisecond canonical-form + upper-cap
        // bracket-gate through the lifted [`SupervisorSpec::restart_window`]
        // accessor rather than the raw `self.restart_window` field access —
        // the one production consumer of the per-`:supervisor`
        // restart-intensity-denominator scalar now keys off exactly one
        // typed dispatch on the substrate primitive, so any future rebrand
        // on the axis (a per-cluster restart-window override the operator
        // pins through a future `:supervisor :restart-window-overrides`
        // slot, a per-tenant restart-window-alias table the M4 CR
        // materializer resolves per-CR) migrates as a single caixa-core
        // edit rather than a coordinated rewrite — sibling of the peer M2
        // [`crate::LimitsSpec::wall_clock`] (8cb717b) validate-arm-route
        // on the per-`:limits :wall-clock` axis and the peer M3
        // [`crate::MeshPolicy::timeout`] (7073d0f) accessor-route on the
        // per-`:politicas :timeout` axis.
        if let Some(w) = self.restart_window() {
            // Zero-floor + integer-millisecond canonical-form +
            // upper-cap bracket on the typed `:restart-window` axis.
            // See
            // [`crate::render::require_positive_canonical_bounded_duration`]
            // for the full three-arm ordering discipline (zero-floor
            // strictly precedes canonical-form so `Duration::ZERO`
            // surfaces the self-locating `RestartWindowZero`
            // diagnostic; canonical-form strictly precedes the cap arm
            // so a sub-millisecond above-cap value surfaces the more
            // fundamental round-trip-shape diagnostic first) and the
            // three peer typed-`Duration` sites that share this
            // canonical bracket ([`crate::MeshPolicy::timeout`],
            // [`crate::CircuitBreaker::window`],
            // [`crate::LimitsSpec::wall_clock`]). Every validated
            // value lies in `1ms..=SUPERVISOR_RESTART_WINDOW_MAX`
            // (1ms..=1h), integer-millisecond granularity.
            crate::render::require_positive_canonical_bounded_duration(
                w,
                SUPERVISOR_RESTART_WINDOW_MAX,
                || SupervisorError::RestartWindowZero,
                |window| SupervisorError::RestartWindowNotCanonical { window },
                |window| SupervisorError::RestartWindowExceedsCap { window },
            )?;
        }
        // Route the per-child DNS-1123 / semver-requirement / duplicate-
        // detection fan-out loop's traversal head through the lifted
        // [`SupervisorSpec::children`] slice-return accessor rather than
        // the raw `self.children` field access — the third production
        // consumer of the per-`:supervisor` static-child-list surface
        // now keys off exactly one typed dispatch on the substrate
        // primitive. Paired with the sibling `SimpleOneForOne ↔
        // non-SimpleOneForOne` partition-dispatch two-arm probe above
        // to close the third and final open-coded `.children` field
        // access in [`SupervisorSpec::validate`], so a future extension
        // of the axis migrates as a single caixa-core edit rather than
        // a coordinated rewrite of three call sites.
        let mut seen = std::collections::HashSet::new();
        for child in self.children() {
            // Every emitted cluster artifact's `metadata.name` for a
            // supervised child derives from this `:children :caixa` value
            // verbatim — the rendered `wasm.pleme.io/v1alpha1/ComputeUnit
            // .metadata.name` per child, the [`crate::LABEL_PROGRAM`]
            // label value on every child's pod identity, and the per-
            // child K8s [`Service`][svc] `metadata.name` the future
            // wasm-operator (M3) provisions for inter-child supervision
            // tree wiring. Each apiserver-side schema on each landing
            // site enforces the DNS-1123 label rule on admission; a
            // structurally invalid child name (`"Worker"`, `"my_worker"`,
            // `"team.worker"`, `"-worker"`, `"worker-"`, the >63-byte
            // UUID-shaped mistaken-identity slug) silently passes the
            // prior empty-/duplicate-only gate and the failure surfaces
            // at `kubectl apply` time as a `metadata.name: Invalid value`
            // rejection, far from the source caixa.lisp, with no field
            // naming the offending `:children` entry. Lifting the gate
            // to caixa-build time mirrors the `:membros :caixa` value-
            // shape trajectory (3f9d7a0) and the `:placement :clusters`
            // trajectory (6cbb900) onto the third DNS-1123-label-shaped
            // identifier axis — the supervisor tree's child names —
            // through the lifted
            // [`crate::render::require_valid_dns_1123_label`] gate the
            // seven peer name axes (`:membros :caixa`, `:placement
            // :clusters`, `:placement :affinity`, `:contratos :de`/`:para`,
            // `:entrada :para`, `:nome`, `:upgrade-from :module`) each
            // route through, so drift between the eight axes' accepted
            // DNS-1123-label sets is structurally impossible.
            //
            // [svc]: https://kubernetes.io/docs/concepts/services-networking/service/
            crate::render::require_valid_dns_1123_label(
                child.nome(),
                || SupervisorError::EmptyChildName,
                |reason| SupervisorError::ChildCaixaInvalid {
                    caixa: child.nome().to_string(),
                    reason,
                },
            )?;
            // The author surface for `:children :versao` is the same
            // Cargo-shaped semver requirement string `:deps :versao` and
            // `:membros :versao` carry — and the lacre pipeline resolves
            // all three axes through the same
            // [`crate::version::parse_requirement`] entry-point. The
            // shared [`crate::render::require_valid_versao_requirement`]
            // helper brackets the empty-first + parse cascade both peer
            // axes ([`crate::dep::Dep::validate`] on `:deps :versao`,
            // [`crate::AplicacaoSpec::validate_membros`] on `:membros
            // :versao`) route through, so drift between the three axes'
            // accepted requirement sets is structurally impossible and
            // the parse-side no-op the empty-first arm closes (semver's
            // empty parse yields an implicit `*`) lives in exactly one
            // predicate. Every `ChildSpec::versao` past validate is
            // round-trippable through [`crate::parse_requirement`]
            // without re-checking at the resolver layer, and the three
            // `:versao` typed surfaces (`:deps`, `:membros`, `:children`)
            // are now structurally equivalent by construction.
            crate::render::require_valid_versao_requirement(
                child.versao_requirement(),
                || SupervisorError::EmptyChildVersion {
                    caixa: child.nome().to_string(),
                },
                |reason| SupervisorError::ChildVersaoInvalid {
                    caixa: child.nome().to_string(),
                    versao: child.versao_requirement().to_string(),
                    reason,
                },
            )?;
            crate::render::insert_first_seen(&mut seen, child.nome(), || {
                SupervisorError::DuplicateChildCaixa {
                    caixa: child.nome().to_string(),
                }
            })?;
        }
        Ok(())
    }
}

/// Cross-slot coherence gate on the supervision tree: no
/// `:children :caixa` entry may name the supervisor's own `:nome`.
///
/// A supervisor that lists itself as a child is a degenerate self-parent
/// — the supervision tree is a DAG rooted at the supervisor (OTP child
/// specs reference *distinct* child processes; a supervisor is never its
/// own child), and the wasm-operator's hierarchical reconciliation would
/// otherwise be handed a node that is its own parent: a one-node cycle it
/// either rejects far from the source `caixa.lisp` or recurses on. Because
/// every `:nome` is a globally-unique substrate identity (DNS-1123 label +
/// lacre closure root), a child whose `:caixa` equals the supervisor's
/// `:nome` *is* the supervisor itself, not a coincidentally-named peer.
///
/// Lives outside [`SupervisorSpec::validate`] because the typed view
/// carries the children but not the parent `:nome`; mirrors the
/// cross-slot precedence gate `validate_upgrade_from_against_versao`
/// (which likewise reads one slot against another at the
/// [`crate::layout`] wire-up site) and the mesh self-edge gate
/// `AplicacaoSpec`'s `ContratoSelfLoop` — the same "an edge from a graph
/// node to itself is structurally not a tree/mesh edge" discipline, here
/// on the supervision-tree axis.
pub fn validate_no_self_supervision(
    children: &[ChildSpec],
    parent_nome: &str,
) -> Result<(), SupervisorError> {
    for child in children {
        if child.nome() == parent_nome {
            return Err(SupervisorError::ChildSupervisesSelf {
                caixa: parent_nome.to_string(),
            });
        }
    }
    Ok(())
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SupervisorError {
    #[error("supervisor :estrategia {estrategia:?} requires at least one :children entry")]
    NoChildren { estrategia: RestartStrategy },
    #[error(
        "SimpleOneForOne supervisors must declare zero static children (children spawn dynamically)"
    )]
    SimpleOneForOneWithStaticChildren,
    #[error(":max-restarts must be > 0")]
    ZeroMaxRestarts,
    #[error(
        ":supervisor :max-restarts ({max_restarts}) exceeds the supervisor-policy ceiling \
         (SUPERVISOR_MAX_RESTARTS_MAX = 1000) — a value above this cap turns the typed \
         restart-intensity policy into a no-op supervisor: the escalation threshold is \
         structurally so high that no realistic restarts-per-:restart-window traffic shape \
         can reach it, so the supervisor never escalates to its parent and a bad child can \
         loop inside the window indefinitely. Every typed-slot consumer (Erlang/OTP's \
         MaxIntensity/Period ratio, the future wasm-operator's per-supervisor \
         restart-intensity counter, the M4 mesh.pleme.io/v1alpha1/Supervisor CR \
         materializer's admission webhook) emits a `:max-restarts` declaration that is \
         structurally never reached. Pin a value in 1..=1000 (Erlang/OTP / Elixir / Riak \
         Core / RabbitMQ production playbooks recommend 3..=100; the OTP `supervisor` \
         callback module's `MaxR = 1` minimal-restart default sits at the bottom of the \
         band) or restructure the supervision tree (split the flaky child into its own \
         sub-supervisor with a tighter budget) if you need a higher restart tolerance."
    )]
    MaxRestartsExceedsCap { max_restarts: u32 },
    #[error(
        ":restart-window must be > 0 when set — Erlang/OTP's MaxIntensity/Period \
         requires Period > 0; a zero window either trips on the first failure or \
         never trips depending on operator interpretation. Omit :restart-window to \
         express `never reset`; carry a positive duration to express the window."
    )]
    RestartWindowZero,
    #[error(
        ":supervisor :restart-window ({window:?}) carries a sub-millisecond residue the shared `duration_codec` cannot round-trip — \
         the codec truncates to `as_millis()` before picking the canonical unit, so a value with `subsec_nanos() % 1_000_000 != 0` either \
         truncates on first serialize (e.g. `Duration::from_micros(1500)` → \"1ms\" → `Duration::from_millis(1)` ≠ original) or renders \
         as \"0s\" the `RestartWindowZero` arm then rejects on re-validate. Pin an integer-millisecond magnitude in the canonical authoring form \
         (`<integer><unit>` for unit ∈ {{ms, s, m, h}}, e.g. `\"500ms\"`, `\"30s\"`, `\"2m\"`, `\"1h\"`) or omit the field for `never reset`"
    )]
    RestartWindowNotCanonical { window: Duration },
    #[error(
        ":supervisor :restart-window ({window:?}) exceeds the supervisor-policy ceiling \
         (SUPERVISOR_RESTART_WINDOW_MAX = 1h = 3600s) — a value above this cap turns the typed \
         per-supervisor rolling-window restart-intensity counter into a lifetime counter: the \
         failure-counting window is structurally so long that transient restarts are never \
         forgotten, the MaxIntensity/Period ratio degenerates from `trip the parent supervisor \
         when the child has exceeded its restart budget within the recent window` to `trip the \
         parent when the child has exceeded its restart budget over its lifetime`, and the \
         supervisor's reset semantic never reaches the child — every typed-slot consumer \
         (Erlang/OTP's MaxIntensity/Period reconciler, the future wasm-operator's \
         per-supervisor restart-intensity counter, the M4 mesh.pleme.io/v1alpha1/Supervisor CR \
         materializer's admission webhook, the caixa-operator's hierarchical reconciliation \
         scheduler) emits a `:restart-window` declaration that is structurally a no-op rolling \
         window. Pin a value in 1ms..=1h (Learn You Some Erlang's `{{intensity, 5, 60}}` \
         worker-supervisor `Period = 60s` default, Elixir's `Supervisor` `max_seconds: 5` \
         default, OTP's `supervisor` callback module `MaxT = 5..=60` typical, Riak Core's \
         `MaxT ∈ 10s..=300s`, RabbitMQ broker-supervisor `MaxT = 5s` default — every Erlang/OTP \
         / Elixir production playbook sits in the 5s..=300s band; the longest documented \
         per-supervisor restart-window any pleme-io substrate playbook recommends maxes at \
         ~30m) or omit :restart-window to express `never reset` (the supervisor's restart \
         budget then becomes a strict lifetime counter by design, not a degenerate one — the \
         author surfaces the lifetime-counter semantic explicitly at the slot, rather than \
         hiding it behind a rolling-window declaration the cap arm rejects)"
    )]
    RestartWindowExceedsCap { window: Duration },
    #[error("child entry has empty :caixa name")]
    EmptyChildName,
    #[error(
        "child :caixa {caixa:?} is not a valid DNS-1123 label: {reason} \
         (the K8s apiserver enforces this rule on every `metadata.name` / Service \
         name / label value the child name lands in — the per-child \
         `wasm.pleme.io/v1alpha1/ComputeUnit.metadata.name`, the `LABEL_PROGRAM` \
         label value, and the future wasm-operator per-child Service `metadata.name` \
         — each apiserver-side schema rejects names that don't match; use a \
         lowercase alphanumeric + hyphen identifier like `\"worker\"` or `\"cache-v2\"`)"
    )]
    ChildCaixaInvalid { caixa: String, reason: String },
    #[error("child {caixa:?} has empty :versao constraint")]
    EmptyChildVersion { caixa: String },
    #[error(
        "child {caixa:?} :versao {versao:?} is not a valid semver requirement: \
         {reason} (use Cargo-shaped forms like `\"^0.1\"`, `\"~0.1.2\"`, \
         `\"0.1.0\"`, or `\"*\"` — the same shape `:deps :versao` and \
         `:membros :versao` carry; the lacre pipeline resolves all three \
         through the same parser)"
    )]
    ChildVersaoInvalid {
        caixa: String,
        versao: String,
        reason: String,
    },
    #[error(
        "child {caixa:?} appears more than once (Erlang/OTP requires unique \
         child_spec.id per supervisor; duplicate children materialize as duplicate \
         ComputeUnits in the rendered chart, one silently overwriting the other)"
    )]
    DuplicateChildCaixa { caixa: String },
    #[error(
        "supervisor {caixa:?} lists itself as a :children entry — a supervisor is \
         never its own child (the supervision tree is a DAG rooted at the supervisor; \
         OTP child specs reference distinct child processes). Since every :nome is a \
         globally-unique substrate identity, a child naming the supervisor's own :nome \
         is a one-node reconciliation cycle, not a coincidentally-named peer; drop the \
         self-referential :children entry or rename it to the actual child caixa."
    )]
    ChildSupervisesSelf { caixa: String },
}

/// Shared duration string codec for the typed slots that take a
/// duration (`restart_window`, `MeshPolicy::timeout`,
/// `CircuitBreaker::window`, …). Public so [`crate::aplicacao`] can
/// reuse it without duplicating the parser.
pub mod duration_codec {
    use super::Duration;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(d) => s.serialize_str(&render(*d)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        let opt: Option<String> = Option::deserialize(d)?;
        match opt {
            None => Ok(None),
            Some(s) => parse(&s).map(Some).map_err(serde::de::Error::custom),
        }
    }

    pub(crate) fn parse(s: &str) -> Result<Duration, String> {
        // Whitespace-rejection arm — peer with the leading-`+` /
        // fractional-magnitude arm below (`"+30s"`, `"1.5s"`) and the
        // leading-zero arm below (`"030s"`) on the same canonical-form
        // render-determinism axis. Until this gate landed the parser
        // silently tolerated leading / trailing / internal whitespace
        // via the top-level `s.trim()` at parse entry and the per-part
        // `num_part.trim()` / `unit.trim()` calls below, so every
        // whitespace-carrying shape (`" 30s"` — paste-from-aligned-doc
        // / YAML-quoted-plain-scalar leading-space; `"30s "` —
        // paste-from-shell-history trailing-space; `"30 s"` —
        // paste-from-typography whitespace-between-magnitude-and-unit;
        // `"30\ts"` — peer tab byte between magnitude and unit;
        // `"30s\n"` — trailing newline from a multi-line paste;
        // `"\t30s"` — paste-from-indented-doc / YAML-block-scalar tab
        // byte) parsed to the same `Duration::from_secs(30)` and serde
        // silently round-tripped to `"30s"` on the next emit (a
        // *different* canonical string) — breaking the THEORY.md Part V
        // render-determinism contract on three typed-duration slots at
        // once (`:supervisor :restart-window`, `:politicas :timeout`,
        // `:politicas :circuit-breaker :window`) via the shared codec.
        //
        // The canonical author shape is `<integer><unit>` (or
        // `<integer>` for the bare-integer-as-seconds shorthand) with
        // no whitespace bytes anywhere — every string [`render`] emits
        // carries none, so the parser's accepted set must match for
        // serialize / deserialize to round-trip losslessly. This gate
        // makes the pre-existing `s.trim()` / `num_part.trim()` /
        // `unit.trim()` calls below strict no-ops on the accepted set
        // (every byte-position match they would perform is now already
        // trimmed away by the accepted set itself), while the arm
        // surfaces every rejected whitespace-carrying shape with a
        // self-locating diagnostic naming the offending byte and the
        // canonical form the author intended, peer with every prior
        // canonical-form-drift arm on this codec.
        //
        // Routed through the lifted
        // [`crate::render::find_ascii_whitespace_byte`] predicate —
        // the same source of truth the four peer typed-magnitude
        // codec sites (`limits::parse_byte_size`,
        // `limits::parse_duration`, `limits::parse_millicores`,
        // `rate_limit_codec`) share. `u8::is_ascii_whitespace()` at
        // the predicate covers the five WhatWG-conformant ASCII
        // whitespace bytes (space, tab, LF, FF, CR); the "single
        // lifted predicate" discipline the peer non-ASCII arm below
        // carries on the strictly-complementary Unicode `White_Space`
        // class extends here to the ASCII byte set as well. Covers
        // three typed-duration slots at once through the shared
        // codec: `:supervisor :restart-window`, `:politicas
        // :timeout`, and `:politicas :circuit-breaker :window`.
        if let Some(b) = crate::render::find_ascii_whitespace_byte(s) {
            return Err(format!(
                "duration: value {s:?} contains whitespace byte 0x{b:02x} — the canonical \
                 authoring form for the typed duration slots routed through this shared codec \
                 (`:supervisor :restart-window`, `:politicas :timeout`, \
                 `:politicas :circuit-breaker :window`) is `<integer><unit>` (e.g. \
                 `\"30s\"`, `\"500ms\"`, `\"2m\"`, `\"1h\"`) with no whitespace bytes \
                 anywhere. A whitespace-carrying shape (`\" 30s\"`, `\"30s \"`, `\"30 s\"`, \
                 `\"\\t30s\"`, `\"30s\\n\"`) round-trips through `render` to a *different* \
                 canonical form (`\"30s\"`) on first serialize — breaking the THEORY.md \
                 Part V render-determinism contract every typed slot carries. Strip every \
                 whitespace byte (write `\"30s\"` verbatim)"
            ));
        }
        // Non-ASCII Unicode `White_Space` arm — the strictly-
        // complementary class the ASCII arm above cannot see.
        // `str::trim` at the top of every peer codec uses
        // `char::is_whitespace` (Unicode `White_Space`, strictly wider
        // than the ASCII byte set), so an NBSP (`\u{00A0}`) / LINE
        // SEPARATOR (`\u{2028}`) / EM-SPACE (`\u{2003}`) survives the
        // byte-scan (its UTF-8 bytes are not in `is_ascii_whitespace`),
        // gets silently stripped by the top-level `s.trim()` below,
        // and the value round-trips through `render` to a *different*
        // canonical form (`\"30s\"`) on next emit — breaking the
        // THEORY.md Part V render-determinism contract on three typed
        // duration slots at once (`:supervisor :restart-window`,
        // `:politicas :timeout`, `:politicas :circuit-breaker
        // :window`) via the shared codec. Closed here and at the
        // three peer codec sites (`limits::parse_byte_size`,
        // `limits::parse_duration`, `rate_limit_codec`) through the
        // shared [`crate::render::find_non_ascii_whitespace_char`]
        // predicate — the "single lifted predicate across all four
        // codec sites in one follow-up run" the 24a8ad4 commit body's
        // `Forward compounding` bullet named as the next compounding
        // step.
        if let Some(ch) = crate::render::find_non_ascii_whitespace_char(s) {
            return Err(format!(
                "duration: value {s:?} contains non-ASCII Unicode whitespace character \
                 {ch:?} (U+{cp:04X}) — the canonical authoring form for the typed \
                 duration slots routed through this shared codec (`:supervisor \
                 :restart-window`, `:politicas :timeout`, `:politicas :circuit-breaker \
                 :window`) is `<integer><unit>` (e.g. `\"30s\"`, `\"500ms\"`, `\"2m\"`, \
                 `\"1h\"`) with no whitespace characters anywhere (ASCII or Unicode). A \
                 non-ASCII-whitespace-carrying shape (`\"\\u{{00A0}}30s\"`, \
                 `\"30s\\u{{2028}}\"`, `\"30\\u{{2003}}s\"`) survives the ASCII byte-scan \
                 but `str::trim` (which uses `char::is_whitespace` — the Unicode \
                 `White_Space` property, strictly wider than the ASCII byte set) silently \
                 strips it at parse entry, and the value round-trips through `render` to \
                 a *different* canonical form (`\"30s\"`) on first serialize — breaking \
                 the THEORY.md Part V render-determinism contract every typed slot \
                 carries. Strip every non-ASCII whitespace character (write `\"30s\"` \
                 verbatim with only ASCII bytes)",
                cp = ch as u32
            ));
        }
        let s = s.trim();
        let split = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
        let (num_part, unit) = s.split_at(split);
        let num_trim = num_part.trim();
        // The canonical authoring form for every typed slot routed
        // through this shared codec — `:supervisor :restart-window`,
        // `:politicas :timeout`, `:politicas :circuit-breaker :window`
        // — is `<integer><unit>`. Every magnitude [`render`] emits is a
        // non-negative integer with no decimal point and no leading
        // sign, so the parser's accepted set must match for
        // serialize/deserialize to round-trip without canonical-form
        // drift. Until this gate landed the parser accepted any
        // `f64`-shaped magnitude (`"1.5s"` → 1500ms, `"1.0s"` → 1s,
        // `"0.5m"` → 30s, `"+30s"` → 30s) and serde silently round-
        // tripped the value to a *different* canonical string on the
        // next emit (`"1.5s"` → 1500ms → `"1500ms"`, `"1.0s"` → 1s →
        // `"1s"`, `"0.5m"` → 30s → `"30s"`, `"+30s"` → 30s → `"30s"`)
        // — breaking the THEORY.md Part V render-determinism contract
        // on three typed slots at once. Same canonical-form discipline
        // `crate::limits::parse_duration` (818dd38, the immediate
        // predecessor on the peer `:limits :wall-clock` codec) applies;
        // this gate lifts the discipline onto the shared codec that
        // backs the remaining three typed-duration slots in caixa-core.
        //
        // Strict canonical form: every byte of the magnitude is an
        // ASCII digit (no `.`, no `+`, no `-`). On non-digit-only
        // inputs the gate distinguishes "non-canonical-but-numeric"
        // (parses as f64 or i64 — surfaced with a self-locating
        // diagnostic naming the canonical authoring form, the
        // round-trip drift each rejected shape would produce on first
        // serialize, and the canonical-form remediation) from
        // "garbage" (parses as neither — surfaced with the existing
        // narrower "bad duration magnitude" wording so its diagnostic
        // shape remains stable for the parser-shape footgun case).
        // The pre-existing `num < 0.0` arm is now unreachable — the
        // digit-only gate strictly precedes magnitude parsing, and a
        // leading `-` is not an ASCII digit, so `"-30s"` lands on the
        // non-canonical-but-numeric branch with the `-30` named
        // verbatim in the diagnostic rather than the prior
        // value-laundered "negative duration in \"-30s\"" wording.
        //
        // Routed through the lifted
        // [`crate::render::is_digit_only_magnitude`] predicate — the
        // same source of truth the four peer typed-magnitude codec
        // sites share.
        let digit_only = crate::render::is_digit_only_magnitude(num_trim);
        if !digit_only {
            let numeric = num_trim.parse::<f64>().is_ok() || num_trim.parse::<i64>().is_ok();
            if numeric {
                return Err(format!(
                    "duration: magnitude {num_trim:?} is not a non-negative integer — the \
                     canonical authoring form for the typed duration slots routed through \
                     this shared codec (`:supervisor :restart-window`, `:politicas :timeout`, \
                     `:politicas :circuit-breaker :window`) is `<integer><unit>` (e.g. \
                     `\"30s\"`, `\"500ms\"`, `\"2m\"`, `\"1h\"`) with no decimal point and \
                     no leading `+` / `-` sign. A fractional / decimal-shaped magnitude \
                     (`\"1.5s\"`, `\"1.0s\"`, `\"0.5m\"`, `\"+30s\"`, `\"-30s\"`) round-trips \
                     through `render` to a *different* canonical form (`\"1500ms\"`, `\"1s\"`, \
                     `\"30s\"`, `\"30s\"`, `\"30s\"`) on first serialize — breaking the \
                     THEORY.md Part V render-determinism contract every typed slot carries. \
                     Pick an integer magnitude in the unit that divides cleanly (write \
                     `\"1500ms\"` instead of `\"1.5s\"`; `\"30s\"` instead of `\"0.5m\"`)"
                ));
            }
            return Err(format!("bad duration magnitude in {s:?}"));
        }
        // Leading-zero arm — peer with the `rate_limit_codec` leading-
        // zero arm (4f46830) on the same canonical-form render-
        // determinism axis. The digit-only gate accepts `"030s"`,
        // `"00s"`, `"01h"`, `"0500ms"` as `u64::from_str` parses them
        // losslessly (= 30, 0, 1, 500), but `render` emits the leading-
        // zero-stripped form (`"30s"`, `"0s"`, `"1h"`, `"500ms"`) — a
        // *different* canonical string on the next emit, breaking the
        // THEORY.md Part V render-determinism contract the same way
        // `"+30s"` did before the leading-`+` arm landed. The single-
        // byte magnitude `"0"` (or `"0s"` / `"0ms"`) round-trips
        // losslessly through `render` (`render(Duration::ZERO)` emits
        // `"0s"`) — the downstream semantic-zero gates (e.g.
        // `SupervisorError::ZeroRestartWindow` on
        // `:supervisor :restart-window`,
        // `AplicacaoError::PolicyTimeoutZero` /
        // `PolicyCircuitBreakerWindowZero` on the typed `:politicas`
        // duration slots) refuse zero-magnitude authoring at the typed-
        // validate layer above, so the single-byte `"0"` stays in the
        // accepted set at this codec layer and the diagnostic
        // partitioning between canonical-form drift (this arm) and
        // semantic-zero (the downstream gates) remains stable.
        // Peer with the future leading-zero arms on the two remaining
        // typed-magnitude codecs the trajectory acknowledges:
        // `limits::parse_duration` backing `:limits :wall-clock`,
        // `limits::parse_byte_size` backing `:limits :memory` — each
        // carries the same canonical-form-drift class today; this
        // gate lands the discipline on the shared duration codec
        // first because the `rate_limit_codec` predecessor on the
        // same canonical-form-drift axis is the closest peer on the
        // trajectory.
        //
        // Routed through the lifted
        // [`crate::render::is_leading_zero_padded_magnitude`]
        // predicate — the same source of truth the four peer
        // typed-magnitude codec sites share.
        if crate::render::is_leading_zero_padded_magnitude(num_trim) {
            return Err(format!(
                "duration: magnitude {num_trim:?} has a non-canonical leading zero — the \
                 canonical authoring form for the typed duration slots routed through \
                 this shared codec (`:supervisor :restart-window`, `:politicas :timeout`, \
                 `:politicas :circuit-breaker :window`) is `<integer><unit>` (e.g. \
                 `\"30s\"`, `\"500ms\"`, `\"2m\"`, `\"1h\"`) with no leading-zero padding \
                 on the magnitude. A leading-zero magnitude (`\"030s\"`, `\"00s\"`, \
                 `\"01h\"`, `\"0500ms\"`) round-trips through `render` to a *different* \
                 canonical form (`\"30s\"`, `\"0s\"`, `\"1h\"`, `\"500ms\"`) on first \
                 serialize — breaking the THEORY.md Part V render-determinism contract \
                 every typed slot carries. Strip the leading zeros (write \
                 `\"30s\"` instead of `\"030s\"`)"
            ));
        }
        // The digit-only gate guarantees every byte is `[0-9]`, and
        // the leading-zero arm above guarantees the magnitude is
        // either the single byte `"0"` or starts with `[1-9]`, so
        // the only way `u64::from_str` can fail here is overflow (the
        // magnitude exceeds `u64::MAX`). Surface that with an
        // overflow-shaped wording so the diagnostic names the offending
        // magnitude verbatim rather than collapsing onto the
        // non-canonical arm. The codec now operates on `u64` end-to-end
        // — every accepted magnitude is integer-exact; no f64 mantissa
        // drift between author-supplied magnitude and the consumer's
        // `Duration` value. Same shape `crate::limits::parse_duration`
        // (818dd38) carries on the peer `:limits :wall-clock` axis.
        let num: u64 = num_trim.parse::<u64>().map_err(|_| {
            format!("bad duration magnitude in {s:?} (digit-only magnitude overflows u64)")
        })?;
        let unit_trim = unit.trim();
        let dur = match unit_trim {
            "ms" => Duration::from_millis(num),
            "s" | "" => Duration::from_secs(num),
            "m" => Duration::from_secs(num.checked_mul(60).ok_or_else(|| {
                format!("duration {num}{unit_trim} overflows u64 (magnitude × 60 > 2^64-1)")
            })?),
            "h" => Duration::from_secs(num.checked_mul(3600).ok_or_else(|| {
                format!("duration {num}{unit_trim} overflows u64 (magnitude × 3600 > 2^64-1)")
            })?),
            other => return Err(format!("unknown duration unit {other:?}")),
        };
        Ok(dur)
    }

    /// Render a [`Duration`] in the canonical pleme-io duration string
    /// form (`"30s"`, `"1m"`, `"1h"`, `"500ms"`). The same form every
    /// caixa typed-duration slot serializes to and the same form K8s
    /// Gateway API HTTPRoute `timeouts` / `backendRequest` and Cilium
    /// EnvoyConfig per-route timeouts both expect (an integer
    /// followed by `s`/`m`/`h`/`ms`, no fractional values, no leading
    /// `+`). Lifted to `pub` so caixa-side renderers
    /// (`caixa-mesh::gateway_routes`'s :politicas :timeout overlay,
    /// the future per-:politicas `CiliumClusterwideEnvoyConfig`
    /// emitter, the future caixa-otel collector pipeline emitter) can
    /// consume the same canonical formatter without re-inlining the
    /// magnitude/unit decision tree (and inheriting the same drift
    /// footguns: a subtly different `300ms` vs `0.3s` rendering breaks
    /// downstream apply-time parsing in non-obvious ways).
    pub fn render(d: Duration) -> String {
        let total_ms = d.as_millis();
        if total_ms == 0 {
            return "0s".into();
        }
        if total_ms % (3600 * 1000) == 0 {
            return format!("{}h", total_ms / (3600 * 1000));
        }
        if total_ms % (60 * 1000) == 0 {
            return format!("{}m", total_ms / (60 * 1000));
        }
        if total_ms % 1000 == 0 {
            return format!("{}s", total_ms / 1000);
        }
        format!("{total_ms}ms")
    }

    /// True iff `d` round-trips losslessly through [`render`] + [`parse`].
    ///
    /// [`render`] truncates a `Duration` to `as_millis()` before picking the
    /// largest divisor unit, so any sub-millisecond residue
    /// (`d.subsec_nanos() % 1_000_000 != 0`) silently breaks the THEORY.md
    /// §V.2.7 render-determinism contract:
    ///
    ///   - `Duration::from_micros(1500)` (= `1_500_000` ns) → `as_millis() == 1`
    ///     → renders `"1ms"` → parses back to `Duration::from_millis(1)` =
    ///     `1_000_000` ns ≠ original `1_500_000` ns;
    ///   - `Duration::from_nanos(1)` (= 1 ns) → `as_millis() == 0` →
    ///     renders the literal `"0s"`, which the per-axis zero-floor gate
    ///     on every typed-`Duration` slot then rejects on re-validate.
    ///
    /// Lifted to a `pub` predicate next to the [`render`] / [`parse`] pair so
    /// the codec's round-trippable accepted set lives in exactly one place —
    /// every typed-`Duration` slot that routes through this shared codec
    /// (`SupervisorSpec::restart_window` via [`super::duration_codec`],
    /// [`crate::MeshPolicy::timeout`] / [`crate::CircuitBreaker::window`] via
    /// `supervisor::duration_codec` + [`super::duration_codec_required`]) and
    /// every typed-`Duration` slot whose own codec shares the same
    /// `as_millis()`-truncation shape ([`crate::LimitsSpec::wall_clock`] via
    /// [`crate::limits`]'s in-module `parse_duration` / `render_duration`
    /// pair) calls this predicate from its `validate()` to bracket the
    /// accepted set against the codec's accepted set, structurally. Drift
    /// between the codec's granularity and any typed slot's accepted set is
    /// then a single-source-of-truth edit at this predicate rather than a
    /// silent round-trip break the next consumer discovers at apply time.
    ///
    /// Peer of [`crate::aplicacao::POLICY_RETRIES_MAX`] /
    /// [`crate::LIMITS_MEMORY_WASM32_MAX_BYTES`] and the
    /// `is_dns_1123_label` / `is_canonical_rate_limit_window` predicate
    /// family — same "typed-slot's valid set matches its codec's accepted
    /// set, structurally" discipline carried at the codec layer.
    #[must_use]
    pub fn is_integer_millisecond_duration(d: Duration) -> bool {
        d.subsec_nanos().is_multiple_of(1_000_000)
    }
}

/// Required-Duration variant for fields that aren't Option<Duration>.
pub mod duration_codec_required {
    use super::Duration;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::duration_codec::render(*v))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let s = String::deserialize(d)?;
        super::duration_codec::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn child(name: &str, ver: &str, restart: RestartPolicy) -> ChildSpec {
        ChildSpec {
            caixa: name.into(),
            versao: ver.into(),
            restart,
        }
    }

    #[test]
    fn default_has_one_for_one_and_5_restarts_in_60s() {
        let s = SupervisorSpec::default();
        assert_eq!(s.estrategia, RestartStrategy::OneForOne);
        assert_eq!(s.max_restarts, 5);
        assert_eq!(s.restart_window, Some(Duration::from_secs(60)));
        assert!(s.children.is_empty());
    }

    #[test]
    fn validate_one_for_one_requires_children() {
        let mut s = SupervisorSpec::default();
        s.children = vec![];
        assert!(matches!(
            s.validate().unwrap_err(),
            SupervisorError::NoChildren { .. }
        ));
        s.children = vec![child("worker", "^0.1", RestartPolicy::Permanent)];
        s.validate().unwrap();
    }

    #[test]
    fn validate_simple_one_for_one_forbids_static_children() {
        let mut s = SupervisorSpec {
            estrategia: RestartStrategy::SimpleOneForOne,
            ..SupervisorSpec::default()
        };
        s.children
            .push(child("w", "^0.1", RestartPolicy::Permanent));
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::SimpleOneForOneWithStaticChildren
        );
        s.children.clear();
        s.validate().unwrap();
    }

    #[test]
    fn validate_rejects_zero_max_restarts() {
        let s = SupervisorSpec {
            max_restarts: 0,
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(s.validate().unwrap_err(), SupervisorError::ZeroMaxRestarts);
    }

    // ── upper-cap: SUPERVISOR_MAX_RESTARTS_MAX brackets the typed slot ─────
    //
    // The cap arm lifts the `:politicas :circuit-breaker :max-failures` /
    // `POLICY_BREAKER_MAX_FAILURES_MAX` (2b51ace) discipline onto the peer
    // `:supervisor :max-restarts` axis — both fields are "trip the
    // next-higher protection layer after N events in a rolling window"
    // counters with identical degenerate-at-the-high-end shape, so the
    // typed-slot's accepted set lies in `1..=1000` on the supervisor side
    // exactly as it lies in `1..=1000` on the breaker side.

    #[test]
    fn validate_rejects_max_restarts_above_cap() {
        // The fail-before-pass-after pin: `SUPERVISOR_MAX_RESTARTS_MAX +
        // 1` is structurally one past the cap and silently passed
        // validate on every pre-gate codebase because the typed slot's
        // only check was the zero-floor arm. The no-op-supervisor vector
        // only surfaced at the runtime substrate (Erlang/OTP
        // MaxIntensity/Period ratio, the future wasm-operator's
        // per-supervisor restart-intensity counter) far from the source
        // caixa.lisp with no field naming the offending supervisor.
        let s = SupervisorSpec {
            max_restarts: SUPERVISOR_MAX_RESTARTS_MAX + 1,
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::MaxRestartsExceedsCap {
                max_restarts: SUPERVISOR_MAX_RESTARTS_MAX + 1,
            }
        );
    }

    #[test]
    fn validate_rejects_max_restarts_far_above_cap() {
        // The `u32::MAX` worst case — the four-billion-restart
        // threshold a typo (`:max-restarts 4294967295`) or a
        // struct-literal copy-paste lands in the slot. Pin the cap
        // arm's coverage explicitly across the full `u32` overflow so
        // a future relaxation that drops the upper bound surfaces
        // here. Same shape every other typed-cap arm on this surface
        // carries (POLICY_BREAKER_MAX_FAILURES_MAX,
        // POLICY_RETRIES_MAX, POLICY_RATE_LIMIT_MAX).
        let s = SupervisorSpec {
            max_restarts: u32::MAX,
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::MaxRestartsExceedsCap {
                max_restarts: u32::MAX,
            }
        );
    }

    #[test]
    fn validate_accepts_max_restarts_at_cap() {
        // The boundary value — exactly SUPERVISOR_MAX_RESTARTS_MAX —
        // must validate. The cap is inclusive on the top edge,
        // matching the POLICY_BREAKER_MAX_FAILURES_MAX /
        // POLICY_RETRIES_MAX / LIMITS_MEMORY_WASM32_MAX_BYTES
        // discipline on the sibling capped axes. Pin the boundary
        // explicitly so a future off-by-one tightening
        // (`>= SUPERVISOR_MAX_RESTARTS_MAX` instead of `>`) surfaces
        // here as a test failure rather than a silent contract
        // narrowing.
        let s = SupervisorSpec {
            max_restarts: SUPERVISOR_MAX_RESTARTS_MAX,
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        s.validate()
            .expect("max_restarts == SUPERVISOR_MAX_RESTARTS_MAX must validate");
    }

    #[test]
    fn validate_accepts_max_restarts_typical_values() {
        // The documented production-playbook band positive-control
        // sweep — every value Erlang/OTP / Elixir / Riak Core /
        // RabbitMQ recommend (1..=100) must pass, plus a sweep
        // through the hyperscale band (200, 500, 1000) the cap
        // accepts. Pin the inclusive validated set explicitly so a
        // future tightening of the ceiling surfaces here.
        for n in [1u32, 3, 5, 10, 20, 50, 100, 200, 500, 1000] {
            let s = SupervisorSpec {
                max_restarts: n,
                children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
                ..SupervisorSpec::default()
            };
            s.validate()
                .unwrap_or_else(|e| panic!("max_restarts={n} must validate; got {e:?}"));
        }
    }

    #[test]
    fn zero_max_restarts_takes_precedence_over_cap() {
        // The cross-arm ordering pin: `0` is structurally outside
        // both `1..` (zero-floor) and `..=SUPERVISOR_MAX_RESTARTS_MAX`
        // (cap), but the zero-floor diagnostic is the more
        // self-locating one (it directly names the counter-axis
        // remediation), so the validate gate must fire on zero first.
        // Same shape every other zero-then-shape ordering on this
        // surface uses (PolicyRetriesZero then
        // PolicyRetriesExceedsCap; PolicyBreakerZeroFailures then
        // PolicyBreakerMaxFailuresExceedsCap).
        let s = SupervisorSpec {
            max_restarts: 0,
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::ZeroMaxRestarts,
            "max_restarts == 0 must surface the zero-floor diagnostic, not the cap diagnostic"
        );
    }

    #[test]
    fn max_restarts_cap_takes_precedence_over_restart_window_gates() {
        // The cross-arm ordering pin between the cap and the sibling
        // `:restart-window` gates (zero-window, canonical-window). A
        // supervisor carrying both an over-cap `max_restarts` AND a
        // structurally invalid window (zero, sub-ms) must surface the
        // cap diagnostic first — the cap arm is wired immediately
        // after the zero-restart arm and strictly before the window
        // arms, so the offending value the diagnostic names matches
        // the order the author would discover the gates by reading
        // top-to-bottom through `SupervisorSpec::validate`. Pin the
        // order so a future refactor that reorders the arms surfaces
        // here as a test failure rather than a silent diagnostic
        // regression. Peer of
        // `circuit_breaker_max_failures_cap_takes_precedence_over_window_gates`
        // on the sibling `:politicas :circuit-breaker` slot.
        let s = SupervisorSpec {
            max_restarts: SUPERVISOR_MAX_RESTARTS_MAX + 1,
            restart_window: Some(Duration::ZERO),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::MaxRestartsExceedsCap {
                max_restarts: SUPERVISOR_MAX_RESTARTS_MAX + 1,
            },
            "over-cap max_restarts must surface the cap diagnostic before any window-axis diagnostic"
        );
    }

    #[test]
    fn max_restarts_cap_diagnostic_carries_offending_value() {
        // The diagnostic-shape pin: the offending `u32` is carried
        // verbatim into the `SupervisorError::MaxRestartsExceedsCap`
        // variant so the surfaced error message names the value the
        // author wrote (`":supervisor :max-restarts (50000) exceeds the
        // supervisor-policy ceiling …"`), not just the cap. Same
        // self-locating diagnostic shape every other typed-cap arm on
        // this surface carries
        // (`AplicacaoError::PolicyBreakerMaxFailuresExceedsCap` carries
        // the offending failure count verbatim,
        // `AplicacaoError::PolicyRetriesExceedsCap` carries the offending
        // retries count verbatim).
        let s = SupervisorSpec {
            max_restarts: 50_000,
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::MaxRestartsExceedsCap {
                    max_restarts: 50_000
                }
            ),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("50000"),
            ":supervisor :max-restarts cap diagnostic must carry the offending value verbatim (got: {msg})"
        );
    }

    #[test]
    fn supervisor_max_restarts_cap_pins_canonical_value() {
        // The SUPERVISOR_MAX_RESTARTS_MAX constant pins the value at
        // 1000 — the same ceiling the peer
        // POLICY_BREAKER_MAX_FAILURES_MAX cap carries on the
        // `:politicas :circuit-breaker :max-failures` axis (both are
        // "trip the next-higher protection layer after N events in a
        // rolling window" counters with identical
        // degenerate-at-the-high-end shape; uniform top edge so the
        // M4 CR materializers and the wasm-operator reconciler reach
        // for either field knowing the value is in `1..=1000`). Two
        // orders of magnitude above every documented Erlang/OTP /
        // Elixir / Riak Core / RabbitMQ production-playbook
        // recommendation band and below the clearly-pathological
        // "effectively no escalation" floor (10_000, 100_000,
        // u32::MAX). Pinning the literal value here surfaces a future
        // drift (a relaxation to 10_000, a tightening to 100) as a
        // deliberate test edit, not a silent contract narrowing.
        assert_eq!(SUPERVISOR_MAX_RESTARTS_MAX, 1000);
    }

    #[test]
    fn validate_rejects_empty_child_name() {
        let s = SupervisorSpec {
            children: vec![child("", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(s.validate().unwrap_err(), SupervisorError::EmptyChildName);
    }

    #[test]
    fn validate_rejects_empty_child_version() {
        let s = SupervisorSpec {
            children: vec![child("w", "", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert!(matches!(
            s.validate().unwrap_err(),
            SupervisorError::EmptyChildVersion { .. }
        ));
    }

    // ── value-shape: parse-as-VersionReq on :children :versao ─────────────

    #[test]
    fn validate_rejects_invalid_child_versao_requirement() {
        // The fail-before-pass-after pin: a non-empty but malformed
        // semver requirement (`"^bad-version"`) silently passed
        // `validate()` on every pre-gate codebase because the prior
        // shape only refused the empty string. The parse failure
        // surfaced far downstream at lacre-resolve time with a
        // `semver::Error` that didn't name which `:children` entry
        // carried the typo. The new gate moves the check to caixa-build
        // time at the source caixa.lisp — the third `:versao` typed
        // axis (`:children`) joins `:deps` and `:membros` (9888b13) at
        // structural parity.
        let s = SupervisorSpec {
            children: vec![
                child("worker", "^0.1", RestartPolicy::Permanent),
                child("cache", "^bad-version", RestartPolicy::Transient),
            ],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildVersaoInvalid { ref caixa, ref versao, .. }
                    if caixa == "cache" && versao == "^bad-version"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_child_versao_with_double_caret_typo() {
        // `"^^0.1"` is the canonical doubled-caret typo — looks
        // Cargo-shaped on first glance but fails the parser because
        // semver doesn't accept stacked operators. Pin this
        // adjacent-shape footgun explicitly so a future relaxation that
        // accepts "looks-canonical-but-isn't" forms surfaces here.
        let s = SupervisorSpec {
            children: vec![child("worker", "^^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildVersaoInvalid { ref caixa, ref versao, .. }
                    if caixa == "worker" && versao == "^^0.1"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_child_versao_with_v_prefixed_tag() {
        // `"v0.1"` is the canonical "git-tag-shape leaking into the
        // semver requirement slot" typo — an author copies the
        // publish-side git-tag string verbatim into `:versao`, but
        // Cargo's semver parser rejects the leading `v`. Same
        // adjacent-shape footgun pinned for `:membros :versao`
        // (9888b13).
        let s = SupervisorSpec {
            children: vec![child("worker", "v0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildVersaoInvalid { ref caixa, ref versao, .. }
                    if caixa == "worker" && versao == "v0.1"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_accepts_canonical_child_versao_forms() {
        // The Cargo-shaped requirement forms `:deps :versao` and
        // `:membros :versao` already accept via
        // `crate::parse_requirement` must pass the children gate
        // without re-validating at the resolver layer. Pin every leg so
        // a future tightening of the canonical set surfaces here as a
        // test failure.
        for form in [
            "^0.1",      // caret — minor-range pin (the most common shape)
            "~0.1.2",    // tilde — patch-range pin
            "0.1.0",     // exact — single-version pin
            "*",         // wildcard — any version (semver::VersionReq::STAR)
            ">=0.1, <2", // multi-range — comma-separated comparators
        ] {
            let s = SupervisorSpec {
                children: vec![child("worker", form, RestartPolicy::Permanent)],
                ..SupervisorSpec::default()
            };
            s.validate()
                .unwrap_or_else(|e| panic!("canonical form {form:?} must validate, got {e:?}"));
        }
    }

    #[test]
    fn child_versao_empty_takes_precedence_over_invalid() {
        // Order pin: the existing `EmptyChildVersion` diagnostic (which
        // doesn't try to parse) fires before the new
        // `ChildVersaoInvalid` parse-side diagnostic, so an empty
        // `:versao` keeps its narrower error message —
        // `parse_requirement` would also reject `""`, but the
        // empty-string arm is the more self-locating diagnostic for the
        // author. Same ordering discipline as
        // `membro_versao_empty_takes_precedence_over_invalid` in
        // aplicacao.rs.
        let s = SupervisorSpec {
            children: vec![child("worker", "", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, SupervisorError::EmptyChildVersion { ref caixa } if caixa == "worker"),
            "got {err:?}"
        );
    }

    #[test]
    fn child_versao_invalid_fires_before_duplicate_check() {
        // Order pin: a malformed requirement on a non-duplicate entry
        // surfaces *its own* diagnostic (which names the offending
        // `:versao` string), even when a later entry would otherwise
        // collapse onto an earlier name. The per-entry shape gate runs
        // inline before the duplicate-key insert — parallel to
        // `membro_versao_invalid_fires_before_duplicate_check` in
        // aplicacao.rs and the b0c8389 / c4213a4 ordering discipline.
        let s = SupervisorSpec {
            children: vec![
                child("worker", "^bad", RestartPolicy::Permanent),
                child("cache", "^0.1", RestartPolicy::Transient),
                child("worker", "^0.2", RestartPolicy::Permanent), // would otherwise raise DuplicateChildCaixa
            ],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildVersaoInvalid { ref caixa, .. } if caixa == "worker"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn child_versao_invalid_diagnostic_carries_offending_versao() {
        // The diagnostic-shape pin: the error names the offending
        // `:versao` value verbatim so the author can grep their
        // caixa.lisp without re-running the build, and carries a
        // non-empty `reason` from `semver::VersionReq::parse` so the
        // parser's own wording flows through to the diagnostic.
        let s = SupervisorSpec {
            children: vec![child("worker", "not-a-req", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        let SupervisorError::ChildVersaoInvalid {
            caixa,
            versao,
            reason,
        } = err
        else {
            panic!("expected ChildVersaoInvalid, got other variant");
        };
        assert_eq!(caixa, "worker");
        assert_eq!(versao, "not-a-req");
        assert!(
            !reason.is_empty(),
            "ChildVersaoInvalid `reason` must carry the parser's wording verbatim"
        );
    }

    // ── value-shape: DNS-1123 label rule on :children :caixa ──────────────

    #[test]
    fn validate_rejects_child_caixa_with_uppercase() {
        // The canonical "I copied the Servico's display name verbatim"
        // typo — child caixa names are lowercase per K8s DNS-1123 label
        // rule. The diagnostic names the offending name and suggests the
        // lower-cased fix in one edit, mirroring the
        // `rejects_membro_caixa_with_uppercase` gate's shape (3f9d7a0).
        let s = SupervisorSpec {
            children: vec![child("Worker", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        let SupervisorError::ChildCaixaInvalid { caixa, reason } = err else {
            panic!("expected ChildCaixaInvalid, got other variant");
        };
        assert_eq!(caixa, "Worker");
        assert!(
            reason.contains("uppercase"),
            "diagnostic must name the violation as `uppercase` (got: {reason:?})"
        );
        assert!(
            reason.contains("\"worker\""),
            "diagnostic must suggest the lower-cased fix verbatim (got: {reason:?})"
        );
    }

    #[test]
    fn validate_rejects_child_caixa_with_underscore() {
        // The canonical "I'm thinking of a Python module / Postgres
        // table" leak — `_` is forbidden by every DNS-1123 / DNS-1035
        // label schema. K8s rejects `metadata.name: my_worker` at
        // admission time with an opaque `field is invalid` (no source-
        // citing diagnostic). The gate moves it to caixa-build time.
        let s = SupervisorSpec {
            children: vec![child("my_worker", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildCaixaInvalid { ref caixa, ref reason }
                    if caixa == "my_worker" && reason.contains('_')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_child_caixa_with_dot() {
        // A `:children :caixa` entry is a single DNS-1123 label, not a
        // subdomain. The K8s Service / ComputeUnit `metadata.name` rules
        // forbid dots. Same shape as `rejects_membro_caixa_with_dot`
        // (3f9d7a0) on the peer name axis.
        let s = SupervisorSpec {
            children: vec![child("team.worker", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildCaixaInvalid { ref caixa, ref reason }
                    if caixa == "team.worker" && reason.contains('.')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_child_caixa_with_leading_hyphen() {
        // DNS-1123 / DNS-1035 boundary rule: labels must start and end
        // with an alphanumeric. The K8s apiserver rejects `-worker`
        // outright; the renderer would emit a `metadata.name: "-worker"`
        // that fails admission far from the source caixa.lisp.
        let s = SupervisorSpec {
            children: vec![child("-worker", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildCaixaInvalid { ref caixa, ref reason }
                    if caixa == "-worker" && reason.contains("start and end")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_child_caixa_with_trailing_hyphen() {
        // The symmetric arm of the boundary rule. Pin separately so
        // both ends of the label are covered against a future relaxation
        // that only checks one boundary.
        let s = SupervisorSpec {
            children: vec![child("worker-", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildCaixaInvalid { ref caixa, .. }
                    if caixa == "worker-"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_child_caixa_with_unicode() {
        // DNS-1123 is ASCII-only; IDN must be pre-encoded as Punycode
        // (`xn--…`) by the author before it reaches K8s. The byte-by-
        // byte ASCII validity check rejects multi-byte UTF-8 sequences
        // by the first byte that fails the `[a-z0-9-]` predicate.
        let s = SupervisorSpec {
            children: vec![child("café", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildCaixaInvalid { ref caixa, .. }
                    if caixa == "café"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_child_caixa_with_whitespace() {
        // Whitespace is the canonical "I pasted from a sketch / doc"
        // footgun. The apiserver rejects every `metadata.name` value
        // carrying whitespace; pin the gate fires at the right boundary.
        let s = SupervisorSpec {
            children: vec![child("my worker", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildCaixaInvalid { ref caixa, .. }
                    if caixa == "my worker"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_child_caixa_too_long() {
        // The 64-byte boundary pin. DNS-1123 / DNS-1035 cap labels at
        // 63 bytes; the K8s apiserver rejects every `metadata.name`
        // axis over the limit at admission time. The diagnostic names
        // both the cap and the actual length so the author can shorten
        // in one edit, mirroring `rejects_membro_caixa_too_long`
        // (3f9d7a0) and `rejects_placement_cluster_too_long` (6cbb900).
        let too_long = "a".repeat(64);
        let s = SupervisorSpec {
            children: vec![child(&too_long, "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        let SupervisorError::ChildCaixaInvalid { caixa, reason } = err else {
            panic!("expected ChildCaixaInvalid, got other variant");
        };
        assert_eq!(caixa, too_long);
        assert!(
            reason.contains("63"),
            "diagnostic must name the 63-byte cap (got: {reason:?})"
        );
        assert!(
            reason.contains("64"),
            "diagnostic must name the actual length (got: {reason:?})"
        );
    }

    #[test]
    fn child_caixa_max_length_validates() {
        // The 63-byte boundary control pin — exactly-at-the-cap is
        // accepted, mirroring `membro_caixa_max_length_validates`
        // (3f9d7a0) and `placement_cluster_max_length_validates`
        // (6cbb900). Pinned separately so a future off-by-one tightening
        // surfaces here.
        let max_label = "a".repeat(63);
        let s = SupervisorSpec {
            children: vec![child(&max_label, "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        s.validate().unwrap();
    }

    #[test]
    fn validate_accepts_canonical_child_caixa_forms() {
        // The realistic shapes a supervised child's `:caixa` carries —
        // single-word `worker`, version-suffixed `cache-v2`, single-char
        // `a`, two-char `db`, digit-start `2-pool`, longer hyphen-joined
        // `payment-retry`, all-digit `0`. Pin every leg so a future
        // tightening (e.g. requiring a leading lowercase letter) surfaces
        // here as a test failure. Mirrors `accepts_canonical_membro_caixa_forms`
        // (3f9d7a0) and `accepts_canonical_placement_cluster_forms`
        // (6cbb900).
        for form in [
            "worker",
            "cache-v2",
            "a",
            "db",
            "2-pool",
            "payment-retry",
            "0",
        ] {
            let s = SupervisorSpec {
                children: vec![child(form, "^0.1", RestartPolicy::Permanent)],
                ..SupervisorSpec::default()
            };
            s.validate()
                .unwrap_or_else(|e| panic!("canonical form {form:?} must validate, got {e:?}"));
        }
    }

    #[test]
    fn child_caixa_empty_takes_precedence_over_invalid() {
        // Order pin: the existing `EmptyChildName` diagnostic (which
        // doesn't try to parse the DNS-1123 shape) fires before the new
        // `ChildCaixaInvalid` per-axis gate, so an empty `:caixa` keeps
        // its narrower error message — `is_dns_1123_label` would reject
        // the empty string too (boundary check on the first byte), but
        // the empty-string arm is the more self-locating diagnostic for
        // the author. Same ordering discipline as
        // `membro_caixa_empty_takes_precedence_over_invalid` in
        // aplicacao.rs.
        let s = SupervisorSpec {
            children: vec![child("", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert_eq!(err, SupervisorError::EmptyChildName);
    }

    #[test]
    fn child_caixa_invalid_fires_before_versao_check() {
        // Order pin: the per-axis shape gate runs inline before the
        // per-entry versao check, so a malformed `:caixa` on an entry
        // whose `:versao` would also fail surfaces the more self-
        // locating name-axis diagnostic first. Parallel to
        // `membro_versao_invalid_fires_before_duplicate_check` (9888b13)
        // and `placement_cluster_invalid_fires_before_duplicate_check`
        // (6cbb900).
        let s = SupervisorSpec {
            children: vec![child("My_Worker", "", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildCaixaInvalid { ref caixa, .. } if caixa == "My_Worker"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn child_caixa_invalid_fires_before_duplicate_check() {
        // Order pin: a malformed name on a non-duplicate entry surfaces
        // its own diagnostic, even when a later entry would otherwise
        // collapse onto an earlier name. The per-entry shape gate runs
        // inline before the duplicate-key HashSet insert, mirroring
        // `placement_cluster_invalid_fires_before_duplicate_check`
        // (6cbb900).
        let s = SupervisorSpec {
            children: vec![
                child("Worker", "^0.1", RestartPolicy::Permanent),
                child("cache", "^0.1", RestartPolicy::Transient),
                child("worker", "^0.2", RestartPolicy::Permanent), // would otherwise raise DuplicateChildCaixa
            ],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::ChildCaixaInvalid { ref caixa, .. } if caixa == "Worker"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn child_caixa_invalid_diagnostic_carries_offending_caixa() {
        // The diagnostic-shape pin: the error names the offending
        // `:caixa` verbatim plus a non-empty parser-shaped `reason` so
        // the author can grep their caixa.lisp without re-running the
        // build. Mirrors the diagnostic-shape sweep on every prior
        // value-shape gate (3f9d7a0, 6cbb900, c7d05ec).
        let s = SupervisorSpec {
            children: vec![child("My_Worker", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        let SupervisorError::ChildCaixaInvalid { caixa, reason } = err else {
            panic!("expected ChildCaixaInvalid, got other variant");
        };
        assert_eq!(caixa, "My_Worker");
        assert!(
            !reason.is_empty(),
            "ChildCaixaInvalid `reason` must carry the parser's wording verbatim"
        );
    }

    // ── value-shape: zero restart_window + duplicate child names ──────────

    #[test]
    fn validate_accepts_none_restart_window() {
        // Omitted `:restart-window` is the "never reset" sentinel —
        // valid by design. Mirrors :limits axes where None = unbounded.
        let s = SupervisorSpec {
            restart_window: None,
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        s.validate().unwrap();
    }

    #[test]
    fn validate_rejects_zero_restart_window() {
        // Same "0 means the opposite of what you think" footgun closed
        // for :politicas :timeout (Envoy treats 0s as infinite) and
        // :limits :wall-clock (wasmtime traps before the call starts).
        // Erlang/OTP's MaxIntensity/Period requires Period > 0.
        let s = SupervisorSpec {
            restart_window: Some(Duration::ZERO),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::RestartWindowZero
        );
    }

    // ── value-shape: integer-ms canonical-form on :restart-window ─────────
    //
    // The fourth (and last) typed-`Duration` axis in caixa-core to get
    // the integer-millisecond canonical-form gate — peer with
    // `:limits :wall-clock` (82fc3ef), `:politicas :timeout` (a4ae535),
    // and `:politicas :circuit-breaker :window` (a4ae535). The serde
    // path is already gated at the shared codec layer (see
    // `restart_window_serde_rejects_fractional_seconds`); this arm
    // closes the programmatic-struct-literal path the codec gate can't
    // see.

    #[test]
    fn validate_rejects_sub_millisecond_restart_window() {
        // The fail-before-pass-after pin: a programmatic
        // `Duration::from_micros(1500)` (= 1_500_000 ns) silently passed
        // `validate` on every pre-gate codebase, then truncated to
        // `as_millis() == 1` on first serialize — the shared codec
        // emits `"1ms"`, parses it back to `Duration::from_millis(1)` =
        // 1_000_000 ns, the typed `restart_window` no longer matches
        // its rendered form.
        let s = SupervisorSpec {
            restart_window: Some(Duration::from_micros(1500)),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        match s.validate().unwrap_err() {
            SupervisorError::RestartWindowNotCanonical { window } => {
                assert_eq!(window, Duration::from_micros(1500));
            }
            other => panic!("expected RestartWindowNotCanonical, got {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_one_nanosecond_restart_window() {
        // The far-sub-ms case: `Duration::from_nanos(1)` is non-zero
        // (so `RestartWindowZero` doesn't fire) but `as_millis() == 0`,
        // so the shared codec emits the literal `"0s"` — the next
        // serde round-trip would parse back to `Duration::ZERO`, which
        // the `RestartWindowZero` arm then rejects on re-validate. The
        // canonical-form gate at this layer surfaces a self-locating
        // diagnostic naming the offending Duration verbatim rather
        // than a downstream `RestartWindowZero` whose remediation
        // points at omitting the slot.
        let s = SupervisorSpec {
            restart_window: Some(Duration::from_nanos(1)),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        match s.validate().unwrap_err() {
            SupervisorError::RestartWindowNotCanonical { window } => {
                assert_eq!(window, Duration::from_nanos(1));
            }
            other => panic!("expected RestartWindowNotCanonical, got {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_nanosecond_past_canonical_boundary_restart_window() {
        // The 1-ns-past-1ms boundary case: a `Duration` carrying
        // 1_000_001 ns is structurally past the integer-ms granularity
        // floor — `subsec_nanos() % 1_000_000 == 1`. The codec round-
        // trip would truncate to `1ms` and the consumer would observe
        // a 1-ns drift on every emit. Same boundary the peer
        // `validate_rejects_nanosecond_past_canonical_boundary` test
        // in limits.rs pins for the `:limits :wall-clock` axis.
        let w = Duration::from_nanos(1_000_001);
        let s = SupervisorSpec {
            restart_window: Some(w),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::RestartWindowNotCanonical { window: w }
        );
    }

    #[test]
    fn validate_accepts_integer_millisecond_restart_window_values() {
        // The positive-control sweep: every `Duration` the shared
        // codec can round-trip losslessly — the canonical
        // `<integer>{ms,s,m,h}` set the codec's `render` / `parse`
        // pair emits and accepts — passes `validate` without
        // surfacing the new canonical-form arm. Mirrors
        // `validate_accepts_integer_millisecond_wall_clock_values` on
        // the sibling `:limits :wall-clock` axis.
        for w in [
            Duration::from_millis(1),
            Duration::from_millis(500),
            Duration::from_millis(1500),
            Duration::from_secs(1),
            Duration::from_secs(30),
            Duration::from_secs(60),
            Duration::from_secs(120),
            Duration::from_secs(3600),
        ] {
            let s = SupervisorSpec {
                restart_window: Some(w),
                children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
                ..SupervisorSpec::default()
            };
            s.validate()
                .unwrap_or_else(|e| panic!("integer-ms {w:?} must validate, got {e:?}"));
        }
    }

    #[test]
    fn validate_restart_window_zero_takes_precedence_over_canonical_gate() {
        // Cross-arm ordering pin: `Duration::ZERO` has
        // `subsec_nanos() == 0` and would otherwise pass the
        // canonical-form arm — the zero-floor arm must fire first so
        // the more self-locating `RestartWindowZero` diagnostic (with
        // its omit-axis remediation directly named) leads. Same
        // posture every peer zero-then-shape gate uses
        // (`WallClockZero` → `WallClockNotCanonical`,
        // `PolicyTimeoutZero` → `PolicyTimeoutNotCanonical`,
        // `PolicyBreakerZeroWindow` → `PolicyBreakerWindowNotCanonical`).
        let s = SupervisorSpec {
            restart_window: Some(Duration::ZERO),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::RestartWindowZero
        );
    }

    #[test]
    fn restart_window_canonical_diagnostic_carries_offending_duration() {
        // Diagnostic-shape pin: the canonical-form arm names the
        // offending `Duration` verbatim so the author's grep lands on
        // the field's value, not a generic "duration not canonical"
        // message. Same shape every other typed-canonical-form arm
        // on this surface carries (`WallClockNotCanonical` carries
        // the offending `Duration` verbatim,
        // `PolicyTimeoutNotCanonical` carries the offending
        // `Duration` verbatim).
        let w = Duration::from_micros(500);
        let s = SupervisorSpec {
            restart_window: Some(w),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("500"),
            "diagnostic must carry the offending magnitude verbatim (got {msg:?})"
        );
        assert!(
            msg.contains("sub-millisecond"),
            "diagnostic must name the sub-millisecond residue class (got {msg:?})"
        );
    }

    #[test]
    fn restart_window_validated_value_round_trips_through_codec() {
        // The structural property the canonical-ms gate enforces:
        // every `SupervisorSpec::restart_window` past
        // `SupervisorSpec::validate` round-trips losslessly through
        // the shared duration codec (serialize → string →
        // deserialize → equal value). Pin this end-to-end so a future
        // change to either side (the validate gate's accepted
        // granularity, the codec's parse/render unit set) that breaks
        // the alignment surfaces here. Peer of
        // `wall_clock_validated_value_round_trips_through_codec` on
        // the sibling `:limits :wall-clock` axis.
        for w in [
            Duration::from_millis(1),
            Duration::from_millis(1500),
            Duration::from_secs(30),
            Duration::from_secs(3600),
        ] {
            let s = SupervisorSpec {
                restart_window: Some(w),
                children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
                ..SupervisorSpec::default()
            };
            s.validate().unwrap();
            let json = serde_json::to_string(&s).unwrap();
            let back: SupervisorSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(back.restart_window, Some(w));
        }
    }

    // ── value-shape: upper cap on :restart-window ─────────────────────────
    //
    // The fourth (and last) typed-`Duration` axis in caixa-core to get
    // the 1h upper cap — peer with `:limits :wall-clock` (51e0dbd),
    // `:politicas :timeout` (2e8ee7e), and `:politicas
    // :circuit-breaker :window` (379a814). Brackets the typed
    // `:restart-window` axis structurally: every validated value lies
    // in `1ms..=SUPERVISOR_RESTART_WINDOW_MAX`, integer-millisecond
    // granularity, closing the
    // rolling-window-degenerates-to-lifetime-counter footgun the prior
    // zero-floor-and-canonical-form-only checks left open.

    #[test]
    fn validate_rejects_restart_window_above_cap() {
        // The fail-before-pass-after pin: 3601s = 1h + 1s is
        // structurally one canonical-tick past the
        // [`SUPERVISOR_RESTART_WINDOW_MAX`] ceiling (1h = 3600s) — an
        // integer-millisecond magnitude the canonical-form arm above
        // accepts cleanly, that the shared duration codec round-trips
        // losslessly as `"3601s"`, and that silently passed validate on
        // every pre-gate codebase because the typed slot's only checks
        // were the zero-floor and canonical-form arms. The runtime
        // substrate consuming the value (Erlang/OTP's MaxIntensity/
        // Period reconciler, the future wasm-operator's per-supervisor
        // restart-intensity counter) reaches for a `Duration` so long
        // no realistic restart-recovery pattern resets the counter,
        // far from the source caixa.lisp.
        let w = SUPERVISOR_RESTART_WINDOW_MAX + Duration::from_secs(1);
        let s = SupervisorSpec {
            restart_window: Some(w),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::RestartWindowExceedsCap { window: w }
        );
    }

    #[test]
    fn validate_rejects_restart_window_one_millisecond_above_cap() {
        // Boundary case: exactly 1ms past the cap (the granularity the
        // canonical-form gate enforces). Catches a future "strictly
        // less than" half-measure and pins the diagnostic to name the
        // offending `Duration` verbatim. Peer of
        // `validate_rejects_wall_clock_one_millisecond_above_cap` /
        // `rejects_policy_timeout_one_millisecond_above_cap` /
        // `rejects_circuit_breaker_window_one_millisecond_above_cap`
        // on the sibling typed-`Duration` axes' top edges.
        let w = SUPERVISOR_RESTART_WINDOW_MAX + Duration::from_millis(1);
        let s = SupervisorSpec {
            restart_window: Some(w),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::RestartWindowExceedsCap { window: w }
        );
    }

    #[test]
    fn validate_rejects_restart_window_far_above_cap() {
        // The "obvious authoring footgun" case: a `(:restart-window "24h")`,
        // `(:restart-window "7d")`, or any "I want a lifetime counter
        // but wrote a `<integer>h` magnitude anyway" typo — values the
        // canonical-form arm accepts as integer-millisecond magnitudes,
        // the codec round-trips losslessly through serde, but the
        // operator's `MaxIntensity / Period` reconciler cannot honor
        // as a meaningful rolling window. Until this gate landed
        // validate accepted them. Pin the common above-cap values (24h,
        // 7d, ~11.5d) so a future relaxation that drops the upper bound
        // surfaces here.
        for w in [
            Duration::from_secs(86_400),    // 24h
            Duration::from_secs(604_800),   // 7d
            Duration::from_secs(1_000_000), // ~11.5 days
        ] {
            let s = SupervisorSpec {
                restart_window: Some(w),
                children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
                ..SupervisorSpec::default()
            };
            assert_eq!(
                s.validate().unwrap_err(),
                SupervisorError::RestartWindowExceedsCap { window: w }
            );
        }
    }

    #[test]
    fn validate_accepts_restart_window_at_cap() {
        // The boundary value — exactly [`SUPERVISOR_RESTART_WINDOW_MAX`]
        // (1h) — must validate. The cap is inclusive on the top edge,
        // matching the [`crate::LIMITS_WALL_CLOCK_MAX`] /
        // [`crate::POLICY_TIMEOUT_MAX`] /
        // [`crate::POLICY_BREAKER_WINDOW_MAX`] discipline on the sibling
        // capped axes. Pin the boundary explicitly so a future
        // off-by-one tightening (`>= SUPERVISOR_RESTART_WINDOW_MAX`
        // instead of `>`) surfaces here as a test failure rather than a
        // silent contract narrowing.
        let s = SupervisorSpec {
            restart_window: Some(SUPERVISOR_RESTART_WINDOW_MAX),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        s.validate()
            .expect("restart_window == SUPERVISOR_RESTART_WINDOW_MAX must validate");
    }

    #[test]
    fn validate_accepts_restart_window_typical_values() {
        // The documented Erlang/OTP / Elixir / Riak Core / RabbitMQ
        // per-supervisor production-playbook band positive-control
        // sweep — every value Learn You Some Erlang's `{intensity, 5,
        // 60}` worker-supervisor `Period = 60s` default, Elixir's
        // `Supervisor` `max_seconds: 5` default, OTP's `supervisor`
        // callback module `MaxT = 5..=60` typical, Riak Core's `MaxT ∈
        // 10s..=300s`, and RabbitMQ broker-supervisor `MaxT = 5s`
        // default recommend (5s..=300s) must pass, plus a sweep
        // through the long-tail-flaky-pool band (5m, 15m, 30m, 1h) the
        // cap accepts. Mirrors `validate_accepts_wall_clock_typical_values`
        // on the sibling `:limits :wall-clock` axis.
        for w in [
            Duration::from_millis(1),
            Duration::from_millis(500),
            Duration::from_secs(1),
            Duration::from_secs(5),  // RabbitMQ broker-supervisor default
            Duration::from_secs(10), // Riak Core lower
            Duration::from_secs(30),
            Duration::from_secs(60),  // Learn You Some Erlang default
            Duration::from_secs(120), // OTP supervisor MaxT typical
            Duration::from_secs(300), // Riak Core upper
            Duration::from_secs(900), // 15m
            Duration::from_secs(1800),
            Duration::from_secs(3600), // exactly 1h, the cap
        ] {
            let s = SupervisorSpec {
                restart_window: Some(w),
                children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
                ..SupervisorSpec::default()
            };
            s.validate()
                .unwrap_or_else(|e| panic!("restart_window={w:?} must validate; got {e:?}"));
        }
    }

    #[test]
    fn restart_window_zero_takes_precedence_over_cap() {
        // The cross-arm ordering pin: `Duration::ZERO` is structurally
        // outside both `>= 1ms` (zero-floor) and `<=
        // SUPERVISOR_RESTART_WINDOW_MAX` (cap), but the zero-floor
        // diagnostic is the more self-locating one (it directly names
        // the omit-axis remediation), so the validate gate must fire
        // on zero first. Same shape every other zero-then-cap ordering
        // on this surface uses (`WallClockZero` then
        // `WallClockExceedsCap`, `PolicyTimeoutZero` then
        // `PolicyTimeoutExceedsCap`, `PolicyBreakerZeroWindow` then
        // `PolicyBreakerWindowExceedsCap`).
        let s = SupervisorSpec {
            restart_window: Some(Duration::ZERO),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::RestartWindowZero,
            "Duration::ZERO must surface the zero-floor diagnostic, not the cap diagnostic"
        );
    }

    #[test]
    fn restart_window_canonical_takes_precedence_over_cap() {
        // The cross-arm ordering pin: a `Duration` that is *both*
        // sub-millisecond (non-canonical-form) and structurally above
        // the cap surfaces the canonical-form diagnostic first,
        // because the round-trip-shape break is the more fundamental
        // issue (the value can't even round-trip through the codec,
        // so the cap diagnostic naming `1ms..=1h` would be misleading
        // — there's no integer-ms form of the offending value). Pin
        // the order so a future refactor that reorders the arms
        // surfaces here as a test failure rather than a silent
        // diagnostic regression. Peer of
        // `wall_clock_canonical_takes_precedence_over_cap` /
        // `policy_timeout_canonical_takes_precedence_over_cap`.
        let w = SUPERVISOR_RESTART_WINDOW_MAX + Duration::from_nanos(1);
        let s = SupervisorSpec {
            restart_window: Some(w),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::RestartWindowNotCanonical { window: w },
            "sub-ms above-cap value must surface the canonical-form diagnostic, not the cap diagnostic"
        );
    }

    #[test]
    fn max_restarts_cap_takes_precedence_over_restart_window_cap() {
        // The cross-arm ordering pin between the `:max-restarts` cap
        // and the sibling `:restart-window` cap. A supervisor carrying
        // both an over-cap `max_restarts` AND an over-cap window must
        // surface the `MaxRestartsExceedsCap` diagnostic first — the
        // cap arm is wired immediately after the zero-restart arm and
        // strictly before every window-axis arm (zero / canonical /
        // cap), so the offending value the diagnostic names matches
        // the order the author would discover the gates by reading
        // top-to-bottom through `SupervisorSpec::validate`. Pin the
        // order so a future refactor that reorders the arms surfaces
        // here as a test failure rather than a silent diagnostic
        // regression. Peer of
        // `max_restarts_cap_takes_precedence_over_restart_window_gates`
        // on the sibling zero / canonical window arms.
        let w = SUPERVISOR_RESTART_WINDOW_MAX + Duration::from_secs(1);
        let s = SupervisorSpec {
            max_restarts: SUPERVISOR_MAX_RESTARTS_MAX + 1,
            restart_window: Some(w),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::MaxRestartsExceedsCap {
                max_restarts: SUPERVISOR_MAX_RESTARTS_MAX + 1,
            },
            "over-cap max_restarts must surface the cap diagnostic before any window-axis diagnostic"
        );
    }

    #[test]
    fn restart_window_cap_diagnostic_carries_offending_value() {
        // The diagnostic-shape pin: the offending `Duration` is
        // carried verbatim into the
        // [`SupervisorError::RestartWindowExceedsCap`] variant so the
        // surfaced error message names the value the author wrote,
        // not just the cap. Same self-locating diagnostic shape every
        // other typed-cap arm on this surface carries
        // (`WallClockExceedsCap` carries the offending `Duration`
        // verbatim, `PolicyTimeoutExceedsCap` carries the offending
        // `Duration` verbatim, `PolicyBreakerWindowExceedsCap` carries
        // the offending `Duration` verbatim).
        let w = Duration::from_secs(7200); // 2h
        let s = SupervisorSpec {
            restart_window: Some(w),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, SupervisorError::RestartWindowExceedsCap { window } if window == w),
            "got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("7200"),
            ":supervisor :restart-window cap diagnostic must carry the offending value verbatim (got: {msg})"
        );
    }

    #[test]
    fn supervisor_restart_window_cap_pins_canonical_value() {
        // The SUPERVISOR_RESTART_WINDOW_MAX constant pins the value at
        // exactly 1 hour (3600s = 3_600_000ms) — the largest unit the
        // shared duration codec emits as a clean canonical string
        // (`"<n>h"`). Pinning the literal value here surfaces a future
        // drift (a relaxation to 24h, a tightening to 5m) as a
        // deliberate test edit, not a silent contract narrowing.
        //
        // The four typed-`Duration` caps on the validation surface
        // (`LIMITS_WALL_CLOCK_MAX` per-process, `POLICY_TIMEOUT_MAX`
        // per-edge, `POLICY_BREAKER_WINDOW_MAX` per-breaker,
        // `SUPERVISOR_RESTART_WINDOW_MAX` per-supervisor) share a
        // single uniform top edge at the codec's largest emitted unit
        // — a structural-property invariant the equality assertions
        // here enshrine, so a future drift on any of the four
        // surfaces as a deliberate test edit. Same shape every other
        // typed-cap value pin uses
        // (`wall_clock_cap_pins_canonical_value`,
        // `policy_timeout_cap_pins_canonical_value`,
        // `circuit_breaker_window_cap_pins_canonical_value`).
        assert_eq!(SUPERVISOR_RESTART_WINDOW_MAX, Duration::from_secs(3600));
        assert_eq!(SUPERVISOR_RESTART_WINDOW_MAX.as_millis(), 3_600_000);
        assert_eq!(SUPERVISOR_RESTART_WINDOW_MAX, crate::LIMITS_WALL_CLOCK_MAX);
        assert_eq!(SUPERVISOR_RESTART_WINDOW_MAX, crate::POLICY_TIMEOUT_MAX);
        assert_eq!(
            SUPERVISOR_RESTART_WINDOW_MAX,
            crate::POLICY_BREAKER_WINDOW_MAX
        );
    }

    #[test]
    fn restart_window_cap_value_round_trips_through_codec() {
        // The codec round-trip property the cap arm preserves: the
        // [`SUPERVISOR_RESTART_WINDOW_MAX`] constant itself round-trips
        // through the shared duration codec — every value at the cap
        // serializes to the canonical `"1h"` form and parses back
        // identically. Pin the round-trip so a future change to the
        // codec's unit set or to the cap's magnitude that breaks the
        // round-trip property surfaces here. Peer of
        // `wall_clock_cap_value_round_trips_through_codec` on the
        // sibling `:limits :wall-clock` axis.
        let s = SupervisorSpec {
            restart_window: Some(SUPERVISOR_RESTART_WINDOW_MAX),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        s.validate().unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            json.contains("\"1h\""),
            "SUPERVISOR_RESTART_WINDOW_MAX must serialize to the canonical `\"1h\"` form (got {json})"
        );
        let back: SupervisorSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.restart_window, Some(SUPERVISOR_RESTART_WINDOW_MAX));
    }

    #[test]
    fn validate_rejects_duplicate_child_caixa() {
        // Two children with the same :caixa render to two ComputeUnits
        // with the same name in the cluster's HelmRelease values —
        // one silently overwrites the other. Erlang/OTP's child_spec.id
        // is required-unique per supervisor; same set-not-multiset
        // discipline applied here as for :membros / :placement
        // :clusters / :entrada :paths.
        let s = SupervisorSpec {
            children: vec![
                child("worker", "^0.1", RestartPolicy::Permanent),
                child("cache", "^0.1", RestartPolicy::Transient),
                child("worker", "^0.2", RestartPolicy::Permanent),
            ],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, SupervisorError::DuplicateChildCaixa { ref caixa } if caixa == "worker"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_duplicate_child_diagnostic_names_first_collision() {
        // Iteration walks the :children list in declaration order —
        // the diagnostic names the first repeat, deterministically,
        // even when multiple names duplicate.
        let s = SupervisorSpec {
            children: vec![
                child("a", "^0.1", RestartPolicy::Permanent),
                child("b", "^0.1", RestartPolicy::Permanent),
                child("a", "^0.1", RestartPolicy::Permanent),
                child("b", "^0.1", RestartPolicy::Permanent),
            ],
            ..SupervisorSpec::default()
        };
        let err = s.validate().unwrap_err();
        assert!(
            matches!(err, SupervisorError::DuplicateChildCaixa { ref caixa } if caixa == "a"),
            "got {err:?}"
        );
    }

    // ── self-supervision cross-slot gate ──────────────────────────

    #[test]
    fn validate_no_self_supervision_rejects_self_referential_child() {
        // A supervisor whose `:children` lists its own `:nome` is a
        // one-node reconciliation cycle — rejected, naming the parent.
        let children = vec![
            child("worker", "^0.1", RestartPolicy::Permanent),
            child("orquestra", "^0.1", RestartPolicy::Permanent),
        ];
        let err = validate_no_self_supervision(&children, "orquestra").unwrap_err();
        assert!(
            matches!(err, SupervisorError::ChildSupervisesSelf { ref caixa } if caixa == "orquestra"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_no_self_supervision_accepts_distinct_children() {
        // Positive control: distinct child names (including a child that
        // is itself a supervisor — nested trees are valid OTP) pass.
        let children = vec![
            child("worker", "^0.1", RestartPolicy::Permanent),
            child("sub-tree", "^0.1", RestartPolicy::Permanent),
        ];
        validate_no_self_supervision(&children, "orquestra").unwrap();
    }

    #[test]
    fn validate_no_self_supervision_empty_children_is_ok() {
        // SimpleOneForOne / no-static-children supervisors have nothing
        // to self-reference — the gate is vacuously satisfied.
        validate_no_self_supervision(&[], "orquestra").unwrap();
    }

    #[test]
    fn validate_simple_one_for_one_skips_uniqueness_check() {
        // SimpleOneForOne supervisors carry no static children — the
        // duplicate-child loop never runs. A zero-window declaration
        // on a SimpleOneForOne supervisor still trips the window check
        // (window applies to dynamic children too).
        let s = SupervisorSpec {
            estrategia: RestartStrategy::SimpleOneForOne,
            restart_window: None,
            children: vec![],
            ..SupervisorSpec::default()
        };
        s.validate().unwrap();
        let s_zero = SupervisorSpec {
            estrategia: RestartStrategy::SimpleOneForOne,
            restart_window: Some(Duration::ZERO),
            children: vec![],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s_zero.validate().unwrap_err(),
            SupervisorError::RestartWindowZero
        );
    }

    #[test]
    fn validate_zero_window_runs_after_max_restarts_check() {
        // Pin the order: max_restarts == 0 fires before
        // restart_window == 0s, so an author with both wrong sees the
        // counter-axis diagnostic first (matches the order in the
        // struct and in the doc comment).
        let s = SupervisorSpec {
            max_restarts: 0,
            restart_window: Some(Duration::ZERO),
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(s.validate().unwrap_err(), SupervisorError::ZeroMaxRestarts);
    }

    #[test]
    fn round_trip_all_strategies() {
        for strat in [
            RestartStrategy::OneForOne,
            RestartStrategy::OneForAll,
            RestartStrategy::RestForOne,
            RestartStrategy::SimpleOneForOne,
        ] {
            // Route the `SimpleOneForOne ↔ non-SimpleOneForOne` fixture-
            // shape partition through the [`gen_platform::IsVariant`]
            // derive-generated [`RestartStrategy::is_simple_one_for_one`]
            // predicate rather than the raw
            // `matches!(strat, RestartStrategy::SimpleOneForOne)`
            // open-coded pattern-match — same closed-set-typed-enum
            // arm-discriminator dispatch discipline the sibling
            // [`crate::upgrade::UpgradeInstruction::is_restart`] convergence
            // (915a934) extended onto its two paired positive / negated
            // `matches!` filter sites, and the sibling
            // [`crate::aplicacao::PlacementStrategy`] `IsVariant`-derived
            // predicate convergence (766ec63) extended onto the M3 mesh-
            // slot per-`:placement` distribution-strategy `matches!`
            // discriminator axis. See the sibling
            // `supervisor_spec_estrategia_returns_estrategia_verbatim_across_permutations`
            // fixture and the peer `manifest::tests::
            // caixa_estrategia_and_supervisor_view_reads_through_lifted_estrategia_accessor`
            // fixture — all three sites (the last unlifted
            // `matches!`-based arm-discriminator axis on the OTP-shape
            // supervisor sibling-restart-strategy closed-set typed enum,
            // acknowledged in 915a934's Prior-commits footnote as the
            // outstanding follow-up) now consult one typed dispatch on
            // the substrate primitive.
            let s = SupervisorSpec {
                estrategia: strat,
                children: if strat.is_simple_one_for_one() {
                    vec![]
                } else {
                    vec![child("w", "^0.1", RestartPolicy::Permanent)]
                },
                ..SupervisorSpec::default()
            };
            let json = serde_json::to_string(&s).unwrap();
            let back: SupervisorSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(s, back);
        }
    }

    #[test]
    fn round_trip_all_restart_policies() {
        for policy in [
            RestartPolicy::Permanent,
            RestartPolicy::Temporary,
            RestartPolicy::Transient,
        ] {
            let c = child("w", "^0.1", policy);
            let json = serde_json::to_string(&c).unwrap();
            let back: ChildSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(c, back);
        }
    }

    #[test]
    fn restart_strategy_is_simple_one_for_one_predicate_partitions_the_arm_set() {
        // The fail-before-pass-after pin on the `gen_platform::IsVariant`
        // derive's [`RestartStrategy::is_simple_one_for_one`] arm-
        // discriminator predicate: [`RestartStrategy::SimpleOneForOne`]
        // is the only variant that satisfies `.is_simple_one_for_one()`;
        // every static-children-bearing arm (`OneForOne` / `OneForAll`
        // / `RestForOne`) returns `false`. This pin makes the partition
        // invariant load-bearing at caixa-core test time so a future
        // derive regression (a hole that returns `false` for
        // `SimpleOneForOne` too, or a byte-collision that flips a second
        // variant to `true`) trips here rather than laundering the arm
        // at the three test-fixture builder sites (a hole flips the
        // `SimpleOneForOne` fixture to carry a non-empty children list
        // and the subsequent `SupervisorSpec::validate` would refuse the
        // fixture with [`SupervisorError::SimpleOneForOneWithStaticChildren`];
        // a collision flips a peer strategy's fixture to carry an empty
        // children list and the subsequent `validate` would refuse with
        // [`SupervisorError::NoChildren`] — either way, the pin fires
        // here, at the derive site, rather than at the fixture-refusal
        // site far away). Peer of the sibling
        // [`crate::upgrade::tests::upgrade_instruction_is_restart_predicate_partitions_the_arm_set`]
        // (915a934) pin on the M2 OTP-appup axis and the sibling
        // [`crate::kind::tests::caixa_kind_is_variant_predicates_partition_the_arm_set`]
        // pin on the M0 `:kind` axis.
        let cases: &[(RestartStrategy, bool)] = &[
            (RestartStrategy::OneForOne, false),
            (RestartStrategy::OneForAll, false),
            (RestartStrategy::RestForOne, false),
            (RestartStrategy::SimpleOneForOne, true),
        ];
        for (variant, expected) in cases {
            assert_eq!(
                variant.is_simple_one_for_one(),
                *expected,
                "RestartStrategy::{variant:?}.is_simple_one_for_one() must \
                 return {expected} (partition invariant on the \
                 IsVariant-derived arm-discriminator predicate — every \
                 test-fixture site that partitions the `:children` slot \
                 shape on `SimpleOneForOne ↔ non-SimpleOneForOne` keys \
                 off this typed dispatch, so a derive regression must \
                 surface here rather than at the fixture-refusal site)"
            );
        }
    }

    #[test]
    fn restart_strategy_fixture_partition_routes_through_is_simple_one_for_one_predicate() {
        // Byte-identity pin on the `SimpleOneForOne ↔ non-SimpleOneForOne`
        // fixture-shape partition against the pre-lift
        // `matches!(strat, RestartStrategy::SimpleOneForOne)` open-coded
        // pattern-match every test-fixture builder site previously
        // coupled to inline. Asserts the two projections agree byte-for-
        // byte on every arm of the enum, so a future derive regression
        // that flipped either predicate's arm-set would surface here at
        // caixa-core test time rather than at the three fixture-builder
        // sites (`supervisor::tests::round_trip_all_strategies`,
        // `supervisor::tests::supervisor_spec_estrategia_returns_estrategia_verbatim_across_permutations`,
        // `manifest::tests::caixa_estrategia_and_supervisor_view_reads_through_lifted_estrategia_accessor`)
        // far from the derive site. Same peer-shape byte-identity pin
        // every sibling `IsVariant`-derive-routed convergence carries on
        // the substrate's closed-set typed-enum surface (peer of
        // [`crate::upgrade::tests::validate_restart_exclusive_routes_through_is_restart_predicate`]
        // on the M2 OTP-appup axis).
        let cases = [
            RestartStrategy::OneForOne,
            RestartStrategy::OneForAll,
            RestartStrategy::RestForOne,
            RestartStrategy::SimpleOneForOne,
        ];
        for strat in cases {
            let via_predicate = strat.is_simple_one_for_one();
            let via_matches = matches!(strat, RestartStrategy::SimpleOneForOne);
            assert_eq!(
                via_predicate, via_matches,
                "RestartStrategy::{strat:?}: is_simple_one_for_one() must \
                 byte-equal matches!(_, RestartStrategy::SimpleOneForOne) — \
                 the pre-lift open-coded pattern and the \
                 IsVariant-derived predicate are the same axis, \
                 one typed dispatch"
            );
        }
    }

    #[test]
    fn duration_codec_round_trip_canonical_units() {
        // Note the canonical-form rule: durations serialize to the
        // *largest* unit that divides cleanly, so 60s ↔ "1m" and not
        // "60s" — but the round-trip preserves the underlying Duration.
        let cases = [
            ("30s", Duration::from_secs(30)),
            ("5m", Duration::from_secs(300)),
            ("1h", Duration::from_secs(3600)),
            ("500ms", Duration::from_millis(500)),
        ];
        for (lit, dur) in cases {
            let s = SupervisorSpec {
                children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
                restart_window: Some(dur),
                ..SupervisorSpec::default()
            };
            let json = serde_json::to_string(&s).unwrap();
            assert!(
                json.contains(&format!("\"{lit}\"")),
                "expected \"{lit}\" in {json}"
            );
            let back: SupervisorSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(back.restart_window, Some(dur));
        }
    }

    #[test]
    fn duration_canonicalizes_to_largest_unit() {
        // 60 seconds → "1m" (largest cleanly-divisible unit), but the
        // typed Duration still equals 60s on the way back.
        let s = SupervisorSpec {
            children: vec![child("w", "^0.1", RestartPolicy::Permanent)],
            restart_window: Some(Duration::from_secs(60)),
            ..SupervisorSpec::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"1m\""), "{json}");
        let back: SupervisorSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.restart_window, Some(Duration::from_secs(60)));
    }

    #[test]
    fn three_child_one_for_one_validates() {
        let s = SupervisorSpec {
            estrategia: RestartStrategy::OneForOne,
            max_restarts: 5,
            restart_window: Some(Duration::from_secs(60)),
            children: vec![
                child("worker", "^0.1", RestartPolicy::Permanent),
                child("cache", "^0.1", RestartPolicy::Transient),
                child("scratch", "^0.1", RestartPolicy::Temporary),
            ],
        };
        s.validate().unwrap();
    }

    #[test]
    fn json_uses_pascal_case_for_strategy_and_policy() {
        // Variant names are PascalCase by default in serde, matching
        // tatara-lisp's enum convention (`:estrategia OneForOne`).
        let c = child("w", "^0.1", RestartPolicy::Permanent);
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"Permanent\""));
        assert!(!json.contains("\"permanent\""));

        let s = SupervisorSpec {
            estrategia: RestartStrategy::OneForOne,
            children: vec![c],
            ..SupervisorSpec::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"estrategia\":\"OneForOne\""));
    }

    // ── shared duration codec: integer-magnitude canonical-form gate ──
    //
    // The gate lifts the discipline `crate::limits::parse_duration`
    // (818dd38) carries on the peer `:limits :wall-clock` codec onto
    // the shared codec backing the remaining three typed-duration
    // slots: `:supervisor :restart-window`, `:politicas :timeout`, and
    // `:politicas :circuit-breaker :window`. Every magnitude `render`
    // emits is a non-negative integer with no decimal point and no
    // leading sign, so the codec's accepted set must match for
    // serialize/deserialize to round-trip without canonical-form
    // drift.

    #[test]
    fn parse_accepts_integer_canonical_units() {
        // Pin the happy-path: every canonical author shape `render`
        // ever emits parses to the same `Duration` value, so the
        // codec's accepted set is at least a superset of its emitted
        // set on the canonical-unit axis.
        for (lit, dur) in [
            ("30s", Duration::from_secs(30)),
            ("500ms", Duration::from_millis(500)),
            ("2m", Duration::from_secs(120)),
            ("1h", Duration::from_secs(3600)),
            ("0s", Duration::ZERO),
        ] {
            assert_eq!(
                duration_codec::parse(lit).unwrap(),
                dur,
                "parse({lit:?}) should be {dur:?}"
            );
        }
    }

    #[test]
    fn parse_accepts_bare_integer_as_seconds() {
        // The `"s" | ""` arm: a bare integer with no unit is read as
        // seconds. Pin this so the unit-empty form keeps parsing (it
        // renders to `"<n>s"` on serialize — that's a unit-choice
        // drift the integer-magnitude gate does NOT close, matching
        // the `parse_byte_size` `"1024"` → `"1KiB"` scope decision in
        // the peer `:limits :memory` codec).
        assert_eq!(
            duration_codec::parse("30").unwrap(),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn parse_rejects_fractional_seconds_with_canonical_form_diagnostic() {
        // `"1.5s"` parses as f64 to 1.5 → renders back as `"1500ms"`
        // on first serialize — DRIFT. The integer-magnitude gate names
        // the offending `"1.5"` verbatim and points at the canonical
        // remediation `"1500ms"`.
        let err = duration_codec::parse("1.5s").unwrap_err();
        assert!(err.contains("\"1.5\""), "missing magnitude in {err:?}");
        assert!(
            err.contains("not a non-negative integer"),
            "missing canonical-form reason in {err:?}"
        );
        assert!(
            err.contains("\"1500ms\""),
            "missing canonical-form remediation in {err:?}"
        );
    }

    #[test]
    fn parse_rejects_decimal_shaped_integer_seconds() {
        // `"1.0s"` is the trickiest drift class: numerically `1.0s` is
        // `1s` exactly, so the round-trip looks correct — but the
        // emitted canonical form is `"1s"`, not `"1.0s"`. Gate the
        // decimal-shape-with-integer-value form so author intent is
        // never silently rewritten.
        let err = duration_codec::parse("1.0s").unwrap_err();
        assert!(err.contains("\"1.0\""), "missing magnitude in {err:?}");
        assert!(
            err.contains("not a non-negative integer"),
            "missing canonical-form reason in {err:?}"
        );
    }

    #[test]
    fn parse_rejects_half_unit_minute() {
        // `"0.5m"` is the unit-fraction footgun — author writes a
        // human-readable half-minute, serde silently rewrites to
        // `"30s"` on next emit. The gate names the offending
        // magnitude `"0.5"` and points at the integer-in-smaller-unit
        // form.
        let err = duration_codec::parse("0.5m").unwrap_err();
        assert!(err.contains("\"0.5\""), "missing magnitude in {err:?}");
        assert!(
            err.contains("\"30s\""),
            "missing canonical-form remediation in {err:?}"
        );
    }

    #[test]
    fn parse_rejects_leading_plus_sign() {
        // `u64::from_str` rejects `"+30"` but `f64::from_str` accepts
        // it as `30.0` — the prior parser used f64 so `"+30s"` parsed
        // cleanly to 30s and round-tripped to `"30s"` on next emit
        // (DRIFT). The digit-only gate closes the leading-sign class
        // first; the diagnostic names `"+30"` verbatim.
        let err = duration_codec::parse("+30s").unwrap_err();
        assert!(err.contains("\"+30\""), "missing magnitude in {err:?}");
        assert!(
            err.contains("not a non-negative integer"),
            "missing canonical-form reason in {err:?}"
        );
    }

    #[test]
    fn parse_rejects_leading_minus_sign() {
        // The former `num < 0.0` arm: `"-30s"` parsed as f64 to -30,
        // rejected with `"negative duration in \"-30s\""`. Under the
        // integer-magnitude gate the diagnostic is unified — `-30` is
        // non-digit-only, f64-numeric, and surfaces with the canonical-
        // form reason (no leading `+` / `-` sign) naming the offending
        // `"-30"` verbatim. Same diagnostic shape as every other
        // rejected non-integer magnitude.
        let err = duration_codec::parse("-30s").unwrap_err();
        assert!(err.contains("\"-30\""), "missing magnitude in {err:?}");
        assert!(
            err.contains("not a non-negative integer"),
            "missing canonical-form reason in {err:?}"
        );
    }

    #[test]
    fn parse_garbage_still_falls_through_to_bad_magnitude() {
        // Non-digit-only AND non-numeric (`"--1s"`, `"abc"`) falls
        // through to the narrower "bad duration magnitude" arm — the
        // canonical-form diagnostic is reserved for the parser-shape
        // footgun case, not the "not a number at all" case. Same
        // shape `parse_byte_size`'s `BadByteMagnitude` arm carries on
        // the peer `:limits :memory` codec.
        let err = duration_codec::parse("--1s").unwrap_err();
        assert!(
            err.contains("bad duration magnitude"),
            "expected bad-magnitude wording in {err:?}"
        );
    }

    #[test]
    fn parse_digit_only_magnitude_carries_zero_f64_drift() {
        // The accepted set is now closed under `u64`-exact integer
        // arithmetic: `"500ms"` → `Duration::from_millis(500)` exactly,
        // `"3600s"` → `Duration::from_secs(3600)` exactly, `"1h"` →
        // `Duration::from_secs(3600)` exactly, no f64 mantissa drift
        // possible. Pin the integer-exact arms across the four unit
        // suffixes so a future refactor that reaches back for f64
        // (`from_secs_f64`, `mul_f64`) surfaces here.
        assert_eq!(
            duration_codec::parse("3600s").unwrap(),
            Duration::from_secs(3600)
        );
        assert_eq!(
            duration_codec::parse("60m").unwrap(),
            Duration::from_secs(3600)
        );
        assert_eq!(
            duration_codec::parse("1h").unwrap(),
            Duration::from_secs(3600)
        );
        assert_eq!(
            duration_codec::parse("999ms").unwrap(),
            Duration::from_millis(999)
        );
    }

    #[test]
    fn restart_window_serde_rejects_fractional_seconds() {
        // The shared codec backs `SupervisorSpec::restart_window`
        // (`with = "duration_codec"`) — so the gate applies on serde
        // deserialize for the typed Supervisor slot. A
        // `{"restartWindow":"1.5s"}` payload that previously round-
        // tripped to a different canonical string on next serialize
        // is now refused at deserialize with the integer-magnitude
        // diagnostic.
        let payload = r#"{"estrategia":"OneForOne","maxRestarts":5,
            "restartWindow":"1.5s",
            "children":[{"caixa":"w","versao":"^0.1","restart":"Permanent"}]}"#;
        let err = serde_json::from_str::<SupervisorSpec>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not a non-negative integer"),
            "expected integer-magnitude diagnostic in {msg:?}"
        );
        assert!(msg.contains("\"1.5\""), "missing magnitude in {msg:?}");
    }

    #[test]
    fn restart_window_serde_rejects_leading_plus() {
        // The `u64::from_str` leading-`+` permissiveness gap that
        // motivated the digit-only gate (the `f64`-side accepted
        // `"+30"`, the prior parser silently round-tripped to `"30s"`)
        // is now closed on the shared codec — surfaces as a structured
        // diagnostic at the serde layer for every typed-duration slot.
        let payload = r#"{"estrategia":"OneForOne","maxRestarts":5,
            "restartWindow":"+30s",
            "children":[{"caixa":"w","versao":"^0.1","restart":"Permanent"}]}"#;
        let err = serde_json::from_str::<SupervisorSpec>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("\"+30\""), "missing magnitude in {msg:?}");
        assert!(
            msg.contains("not a non-negative integer"),
            "missing canonical-form reason in {msg:?}"
        );
    }

    #[test]
    fn parse_rejects_leading_zero_magnitude() {
        // `"030s"` is digit-only, so the existing non-digit-only / sign
        // / fractional arm doesn't catch it — `u64::from_str("030")`
        // returns `Ok(30)`, so before this gate `"030s"` parsed to
        // `Duration::from_secs(30)` and round-tripped through `render`
        // to `"30s"` — a *different* canonical string on the next emit,
        // breaking the THEORY.md Part V render-determinism contract
        // exactly the way `"+30s"` did before the leading-`+` arm
        // landed. Peer with the `rate_limit_codec` leading-zero arm
        // (4f46830) on the same canonical-form-drift axis.
        let err = duration_codec::parse("030s").unwrap_err();
        assert!(
            err.contains("non-canonical leading zero"),
            "expected leading-zero diagnostic in {err:?}"
        );
        assert!(err.contains("\"030\""), "missing magnitude in {err:?}");
        assert!(
            err.contains("\"30s\""),
            "missing canonical-form remediation in {err:?}"
        );
        assert!(
            err.contains("THEORY.md"),
            "missing render-determinism citation in {err:?}"
        );
    }

    #[test]
    fn parse_rejects_multi_digit_zero_magnitude() {
        // `"00s"` and `"00ms"` are the all-zero leading-zero footgun —
        // digit-only, parse losslessly to `Duration::ZERO`, but render
        // back to `"0s"` (the single-byte canonical form) on the next
        // emit. The leading-zero arm refuses the drift class at the
        // codec layer; the semantic-zero gate downstream
        // (`SupervisorError::ZeroRestartWindow`, etc.) would refuse
        // the single-byte canonical form `"0s"` separately on the
        // typed-validate layer.
        let err = duration_codec::parse("00s").unwrap_err();
        assert!(
            err.contains("non-canonical leading zero"),
            "expected leading-zero diagnostic in {err:?}"
        );
        assert!(err.contains("\"00\""), "missing magnitude in {err:?}");
    }

    #[test]
    fn parse_rejects_leading_zero_per_hour_window() {
        // `"01h"` is the per-hour-window footgun — multi-byte magnitude
        // starting with `0`, parses losslessly to `Duration::from_secs(3600)`,
        // renders to `"1h"` (DRIFT). The arm is unit-agnostic: every
        // canonical unit suffix the codec accepts (`ms` / `s` / `m` /
        // `h` / bare-integer-as-seconds) inherits the same gate.
        let err = duration_codec::parse("01h").unwrap_err();
        assert!(
            err.contains("non-canonical leading zero"),
            "expected leading-zero diagnostic in {err:?}"
        );
        assert!(err.contains("\"01\""), "missing magnitude in {err:?}");
    }

    #[test]
    fn parse_rejects_leading_zero_bare_integer_as_seconds() {
        // The `parse_accepts_bare_integer_as_seconds` happy-path
        // (`"30"` → 30s) inherits the leading-zero arm: `"030"` is
        // multi-byte starts-with-`0`, parses losslessly to
        // `Duration::from_secs(30)`, renders to `"30s"` (DRIFT). The
        // bare-integer surface accepts permissive unit-empty
        // shorthand but still must reject leading-zero padding.
        let err = duration_codec::parse("030").unwrap_err();
        assert!(
            err.contains("non-canonical leading zero"),
            "expected leading-zero diagnostic in {err:?}"
        );
        assert!(err.contains("\"030\""), "missing magnitude in {err:?}");
    }

    #[test]
    fn parse_accepts_single_zero_magnitude_at_codec_layer() {
        // The codec-layer / typed-validate-layer boundary: `"0s"` /
        // `"0ms"` / `"0"` are the single-byte canonical-zero forms —
        // each round-trips losslessly through `render`
        // (`render(Duration::ZERO)` → `"0s"`), so the codec layer
        // accepts them. The downstream semantic-zero gates
        // (`SupervisorError::ZeroRestartWindow`,
        // `AplicacaoError::PolicyTimeoutZero`,
        // `AplicacaoError::PolicyCircuitBreakerWindowZero`) refuse
        // zero-magnitude authoring at the typed-validate layer above,
        // peer with the `rate_limit_codec` codec-layer / typed-
        // validate-layer partition for `"0/s"`.
        assert_eq!(duration_codec::parse("0s").unwrap(), Duration::ZERO);
        assert_eq!(duration_codec::parse("0ms").unwrap(), Duration::ZERO);
        assert_eq!(duration_codec::parse("0").unwrap(), Duration::ZERO);
    }

    #[test]
    fn parse_accepts_canonical_magnitude_with_leading_one() {
        // The complementary boundary: a future tightening cannot
        // drift into rejecting valid canonical magnitudes that
        // happen to start with `1` (or any digit `[1-9]`). Pin
        // every canonical-unit suffix so the leading-zero arm
        // remains strictly narrower than the digit-only arm.
        assert_eq!(
            duration_codec::parse("100ms").unwrap(),
            Duration::from_millis(100)
        );
        assert_eq!(
            duration_codec::parse("100s").unwrap(),
            Duration::from_secs(100)
        );
        assert_eq!(
            duration_codec::parse("10m").unwrap(),
            Duration::from_secs(600)
        );
        assert_eq!(
            duration_codec::parse("10h").unwrap(),
            Duration::from_secs(36_000)
        );
    }

    #[test]
    fn restart_window_serde_rejects_leading_zero() {
        // The shared codec backs `SupervisorSpec::restart_window`
        // (`with = "duration_codec"`) — so the leading-zero arm
        // applies on serde deserialize for the typed Supervisor slot.
        // A `{"restartWindow":"030s"}` payload that previously round-
        // tripped to a different canonical string on next serialize
        // is now refused at deserialize with the leading-zero
        // diagnostic. Peer with `restart_window_serde_rejects_leading_plus`
        // / `restart_window_serde_rejects_fractional_seconds` on the
        // same canonical-form-drift axis.
        let payload = r#"{"estrategia":"OneForOne","maxRestarts":5,
            "restartWindow":"030s",
            "children":[{"caixa":"w","versao":"^0.1","restart":"Permanent"}]}"#;
        let err = serde_json::from_str::<SupervisorSpec>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("non-canonical leading zero"),
            "expected leading-zero diagnostic in {msg:?}"
        );
        assert!(msg.contains("\"030\""), "missing magnitude in {msg:?}");
    }

    #[test]
    fn parse_rejects_leading_whitespace() {
        // `" 30s"` — the canonical paste-from-aligned-doc /
        // paste-from-YAML-quoted-plain-scalar footgun. Before this
        // gate the top-level `s.trim()` at parse entry silently ate
        // the leading space and parsed the value to
        // `Duration::from_secs(30)`, which then round-tripped through
        // `render` to `"30s"` (a *different* canonical string on the
        // next emit) — the exact canonical-form-drift class the
        // leading-`+` / leading-zero arms already close, extended
        // to the whitespace-byte class. Peer with the sibling
        // `rate_limit_codec` whitespace-rejection arm (1ad7755) on
        // the M3 `:politicas` axis.
        let err = duration_codec::parse(" 30s").unwrap_err();
        assert!(
            err.contains("contains whitespace byte"),
            "expected whitespace diagnostic in {err:?}"
        );
        assert!(err.contains("0x20"), "missing offending byte in {err:?}");
        assert!(
            err.contains("THEORY.md"),
            "missing render-determinism contract citation in {err:?}"
        );
    }

    #[test]
    fn parse_rejects_trailing_whitespace() {
        // `"30s "` — the canonical shell-history / trailing-space
        // paste footgun. Before this gate the top-level `s.trim()`
        // silently ate the trailing space and parsed to
        // `Duration::from_secs(30)`, round-tripping to `"30s"` on the
        // next emit — same canonical-form drift as the leading-space
        // sibling, closed on the same whitespace-byte arm.
        let err = duration_codec::parse("30s ").unwrap_err();
        assert!(
            err.contains("contains whitespace byte"),
            "expected whitespace diagnostic in {err:?}"
        );
        assert!(err.contains("0x20"), "missing offending byte in {err:?}");
    }

    #[test]
    fn parse_rejects_internal_whitespace_between_magnitude_and_unit() {
        // `"30 s"` — the canonical typographically-spaced author
        // shape (the same idiom every prose reference to a duration
        // renders as, mistakenly retained when the value is pasted
        // into a codec-shaped slot). Before this gate the per-part
        // `num_part.trim()` / `unit.trim()` calls silently ate the
        // whitespace between the magnitude and the unit and parsed
        // the value to `Duration::from_secs(30)`, round-tripping to
        // `"30s"` — the codec's *internal* whitespace-tolerance
        // vector, orthogonal to the leading / trailing surface but
        // the same canonical-form-drift class. Pins the arm as
        // strictly stronger than the pre-existing top-level
        // `s.trim()` behavior: it fires on whitespace anywhere in
        // the value, not just at the string boundary.
        let err = duration_codec::parse("30 s").unwrap_err();
        assert!(
            err.contains("contains whitespace byte"),
            "expected whitespace diagnostic in {err:?}"
        );
        assert!(err.contains("0x20"), "missing offending byte in {err:?}");
    }

    #[test]
    fn parse_rejects_tab_byte() {
        // `"\t30s"` — the canonical paste-from-indented-doc /
        // paste-from-YAML-block-scalar footgun where a tab byte leads
        // the magnitude. Pins that the gate covers tab (`0x09`) as
        // well as space (`0x20`) — both are `u8::is_ascii_whitespace`
        // members and both would be silently swallowed by `s.trim()`
        // pre-gate. The `is_ascii_whitespace` coverage extends beyond
        // space alone to the full ASCII-whitespace set (space `0x20`,
        // tab `0x09`, LF `0x0A`, FF `0x0C`, CR `0x0D`); this test pins
        // the tab arm as a representative of the non-space members.
        let err = duration_codec::parse("\t30s").unwrap_err();
        assert!(
            err.contains("contains whitespace byte"),
            "expected whitespace diagnostic in {err:?}"
        );
        assert!(
            err.contains("0x09"),
            "missing offending tab byte in {err:?}"
        );
    }

    #[test]
    fn restart_window_serde_rejects_whitespace() {
        // The shared codec backs `SupervisorSpec::restart_window`
        // (`with = "duration_codec"`) — so the whitespace arm
        // applies on serde deserialize for the typed Supervisor slot.
        // A `{"restartWindow":" 30s"}` payload that previously round-
        // tripped to a different canonical string on next serialize
        // is now refused at deserialize with the whitespace-byte
        // diagnostic. Peer with `restart_window_serde_rejects_leading_zero`
        // / `restart_window_serde_rejects_leading_plus` /
        // `restart_window_serde_rejects_fractional_seconds` on the
        // same canonical-form-drift axis.
        let payload = r#"{"estrategia":"OneForOne","maxRestarts":5,
            "restartWindow":" 30s",
            "children":[{"caixa":"w","versao":"^0.1","restart":"Permanent"}]}"#;
        let err = serde_json::from_str::<SupervisorSpec>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("contains whitespace byte"),
            "expected whitespace diagnostic in {msg:?}"
        );
        assert!(msg.contains("0x20"), "missing offending byte in {msg:?}");
    }

    // ── canonical-form: non-ASCII Unicode `White_Space` duration gate ─────
    //
    // Successor to the ASCII-whitespace arm (a7ae622) on the shared
    // duration codec — closes the strictly-complementary class the
    // byte-scan cannot see, through the lifted
    // [`crate::render::find_non_ascii_whitespace_char`] predicate.
    // Applies to `:supervisor :restart-window`, `:politicas :timeout`,
    // and `:politicas :circuit-breaker :window` simultaneously via
    // this shared codec.

    #[test]
    fn duration_codec_parse_rejects_leading_nbsp() {
        // NBSP prefix — the strictly-complementary drift class the
        // ASCII byte-scan cannot see. `str::trim` strips it silently
        // and the value drifts to `"30s"` on next serialize.
        let err = duration_codec::parse("\u{00A0}30s").unwrap_err();
        assert!(
            err.contains("non-ASCII Unicode whitespace character"),
            "expected non-ASCII whitespace diagnostic in {err:?}"
        );
        assert!(err.contains("U+00A0"), "missing codepoint in {err:?}");
    }

    #[test]
    fn duration_codec_parse_rejects_trailing_line_separator() {
        // LINE SEPARATOR (`\u{2028}`) trailing — paste-from-web-doc
        // footgun.
        let err = duration_codec::parse("30s\u{2028}").unwrap_err();
        assert!(
            err.contains("non-ASCII Unicode whitespace character"),
            "expected non-ASCII whitespace diagnostic in {err:?}"
        );
        assert!(err.contains("U+2028"), "missing codepoint in {err:?}");
    }

    #[test]
    fn duration_codec_parse_accepts_ascii_only_forms_after_unicode_arm() {
        // Positive-control pin: every ASCII-only canonical form the
        // renderer emits stays accepted through the new arm.
        assert_eq!(
            duration_codec::parse("30s").unwrap(),
            Duration::from_secs(30)
        );
        assert_eq!(
            duration_codec::parse("500ms").unwrap(),
            Duration::from_millis(500)
        );
        assert_eq!(
            duration_codec::parse("1h").unwrap(),
            Duration::from_secs(3600)
        );
    }

    #[test]
    fn restart_window_serde_rejects_non_ascii_whitespace() {
        // The shared codec backs `SupervisorSpec::restart_window` — so
        // the new non-ASCII Unicode whitespace arm applies on serde
        // deserialize for the typed Supervisor slot. A
        // `{"restartWindow":" 30s"}` payload that previously
        // survived the ASCII byte-scan (only ASCII whitespace was
        // refused) is now refused at deserialize with the
        // non-ASCII-whitespace-and-codepoint diagnostic.
        let payload = "{\"estrategia\":\"OneForOne\",\"maxRestarts\":5,\
            \"restartWindow\":\"\u{00A0}30s\",\
            \"children\":[{\"caixa\":\"w\",\"versao\":\"^0.1\",\"restart\":\"Permanent\"}]}";
        let err = serde_json::from_str::<SupervisorSpec>(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("non-ASCII Unicode whitespace character"),
            "expected non-ASCII whitespace diagnostic in {msg:?}"
        );
        assert!(msg.contains("U+00A0"), "missing codepoint in {msg:?}");
    }

    // ── drift-detection: serde-derive-to-SUPERVISOR_KEY_* identity ────────

    #[test]
    fn supervisor_spec_serde_keys_match_lifted_supervisor_key_consts() {
        // Load-bearing invariant: the four `SUPERVISOR_KEY_*` consts
        // (`SUPERVISOR_KEY_ESTRATEGIA` / `SUPERVISOR_KEY_MAX_RESTARTS` /
        // `SUPERVISOR_KEY_RESTART_WINDOW` / `SUPERVISOR_KEY_CHILDREN`)
        // name the exact camelCase JSON keys the
        // `#[serde(rename_all = "camelCase")]` attribute on
        // `SupervisorSpec` emits. Serialize a fully-populated spec (each
        // field carries `Some(_)` / non-empty) and pin that each canonical
        // byte-sequence appears verbatim in the JSON — a future accidental
        // `rename_all = "snake_case"` / `"kebab-case"` / verbatim-field-
        // name flip at the derive attribute (any of which would silently
        // break every downstream JSON consumer that reaches for one of the
        // four consts via `Value::get(...)`) surfaces here as a build-time
        // test failure at `supervisor.rs`, not as an apply-time
        // `.get(<stale-canonical-const>)` returning `None` far from the
        // derive-attr drift's commit. Peer with the sibling
        // `limits_spec_serde_keys_match_lifted_m2_limits_key_consts`
        // (d8b8b4f) pin on the M2 `:limits` axis — same discipline the
        // M2 typed-slot family established, extended here to close the
        // top-level Supervisor axis.
        let spec = SupervisorSpec {
            estrategia: RestartStrategy::OneForOne,
            max_restarts: 5,
            restart_window: Some(Duration::from_secs(60)),
            children: vec![ChildSpec {
                caixa: "w".into(),
                versao: "^0.1".into(),
                restart: RestartPolicy::Permanent,
            }],
        };
        let json = serde_json::to_string(&spec).unwrap();
        for key in [
            crate::render::SUPERVISOR_KEY_ESTRATEGIA,
            crate::render::SUPERVISOR_KEY_MAX_RESTARTS,
            crate::render::SUPERVISOR_KEY_RESTART_WINDOW,
            crate::render::SUPERVISOR_KEY_CHILDREN,
        ] {
            let quoted = format!("\"{key}\"");
            assert!(
                json.contains(&quoted),
                "serialized SupervisorSpec must carry the lifted \
                 SUPERVISOR_KEY_* byte-sequence {quoted} verbatim in \
                 the JSON emission (got: {json})",
            );
        }
    }

    #[test]
    fn supervisor_key_consts_are_pairwise_distinct() {
        // Cross-axis drift-detection pin: a future collapse of two
        // canonical top-level byte-strings onto the same value (e.g. an
        // accidental copy-paste flip of `SUPERVISOR_KEY_CHILDREN` to
        // also read `"estrategia"`) would silently reroute every
        // downstream probe on one axis onto the sibling axis's overlay
        // entry and pass every propagation-probe test that expected only
        // the stale axis's value. Peer of the sibling four-way distinct
        // pin on the `M2_LIMITS_KEY_*` tetrad (d8b8b4f).
        let all = [
            crate::render::SUPERVISOR_KEY_ESTRATEGIA,
            crate::render::SUPERVISOR_KEY_MAX_RESTARTS,
            crate::render::SUPERVISOR_KEY_RESTART_WINDOW,
            crate::render::SUPERVISOR_KEY_CHILDREN,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(
                    a, b,
                    "SUPERVISOR_KEY_* consts must be pairwise-distinct \
                     canonical byte-sequences — got `{a}` == `{b}`",
                );
            }
        }
    }

    #[test]
    fn supervisor_key_consts_are_lower_camel_case_shape() {
        // Shape-pin: every `SUPERVISOR_KEY_*` const must be a
        // lowerCamelCase byte-sequence (no `snake_case` underscores, no
        // `kebab-case` hyphens, no leading colon, no `PascalCase` leading
        // capital, no whitespace / dots) — the canonical shape the
        // `#[serde(rename_all = "camelCase")]` derive produces on
        // `SupervisorSpec`. A future flip to a non-camelCase attribute
        // at the derive surfaces both here (this test fails on the
        // stale-constant shape) and at
        // `supervisor_spec_serde_keys_match_lifted_supervisor_key_consts`
        // (that test fails on the mismatch between const and derive).
        // Peer with `m2_limits_key_consts_are_lower_camel_case_shape`
        // (d8b8b4f) on the sibling M2 `:limits` axis.
        for key in [
            crate::render::SUPERVISOR_KEY_ESTRATEGIA,
            crate::render::SUPERVISOR_KEY_MAX_RESTARTS,
            crate::render::SUPERVISOR_KEY_RESTART_WINDOW,
            crate::render::SUPERVISOR_KEY_CHILDREN,
        ] {
            assert!(
                !key.is_empty(),
                "SUPERVISOR_KEY_* must be non-empty (got {key:?})"
            );
            let first = key.chars().next().unwrap();
            assert!(
                first.is_ascii_lowercase(),
                "SUPERVISOR_KEY_* must lead with an ASCII-lowercase byte \
                 (got {key:?}, leads with {first:?})",
            );
            assert!(
                key.chars().all(|c| c.is_ascii_alphanumeric()),
                "SUPERVISOR_KEY_* must be ASCII-alphanumeric only \
                 — no `_` / `-` / `:` / `.` / whitespace (got {key:?})",
            );
        }
    }

    #[test]
    fn supervisor_key_consts_are_byte_distinct_from_supervisor_author_key_peers() {
        // Cross-axis drift pin: the four `SUPERVISOR_KEY_*` consts
        // (camelCase JSON keys, no leading colon) must never collide
        // byte-for-byte with the four peer `SUPERVISOR_AUTHOR_KEY_*`
        // consts (kebab-case author-facing labels with leading colon)
        // that sit next to them at `caixa_core::render`. Both families
        // cover the same four typed Supervisor slots on two distinct
        // axes (author-side kebab vs renderer-side camelCase);
        // collapsing either family onto the other's byte-shape would
        // silently reroute the render-side probe onto the author-facing
        // surface, or vice versa. Peer of the byte-distinctness
        // discipline the `M3_PLACEMENT_KEY_ESTRATEGIA` docstring names
        // against the peer `M3_AUTHOR_KEY_PLACEMENT`.
        let pairs = [
            (
                crate::render::SUPERVISOR_KEY_ESTRATEGIA,
                crate::render::SUPERVISOR_AUTHOR_KEY_ESTRATEGIA,
            ),
            (
                crate::render::SUPERVISOR_KEY_MAX_RESTARTS,
                crate::render::SUPERVISOR_AUTHOR_KEY_MAX_RESTARTS,
            ),
            (
                crate::render::SUPERVISOR_KEY_RESTART_WINDOW,
                crate::render::SUPERVISOR_AUTHOR_KEY_RESTART_WINDOW,
            ),
            (
                crate::render::SUPERVISOR_KEY_CHILDREN,
                crate::render::SUPERVISOR_AUTHOR_KEY_CHILDREN,
            ),
        ];
        for (json_key, author_key) in pairs {
            assert_ne!(
                json_key, author_key,
                "SUPERVISOR_KEY_* (JSON side) must differ byte-for-byte \
                 from the peer SUPERVISOR_AUTHOR_KEY_* (author side); \
                 got JSON `{json_key}` == author `{author_key}`",
            );
        }
    }

    // ── drift-detection: serde-derive-to-SUPERVISOR_CHILD_KEY_* identity ──

    #[test]
    fn child_spec_serde_keys_match_lifted_supervisor_child_key_consts() {
        // Load-bearing invariant: the three `SUPERVISOR_CHILD_KEY_*` consts
        // (`SUPERVISOR_CHILD_KEY_CAIXA` / `SUPERVISOR_CHILD_KEY_VERSAO` /
        // `SUPERVISOR_CHILD_KEY_RESTART`) name the exact camelCase JSON
        // keys the `#[serde(rename_all = "camelCase")]` attribute on
        // `ChildSpec` emits. Serialize a fully-populated `ChildSpec` and
        // pin that each canonical byte-sequence appears verbatim in the
        // JSON — a future accidental `rename_all = "snake_case"` /
        // `"kebab-case"` / verbatim-field-name flip at the derive
        // attribute (any of which would silently break every downstream
        // JSON consumer that reaches for one of the three consts via
        // `Value::get(...)`) surfaces here as a build-time test failure at
        // `supervisor.rs`, not as an apply-time
        // `.get(<stale-canonical-const>)` returning `None` far from the
        // derive-attr drift's commit. Peer with the enclosing
        // `supervisor_spec_serde_keys_match_lifted_supervisor_key_consts`
        // (40cc4e5) pin on the M2 supervision-tree top-level axis — same
        // discipline the SupervisorSpec top-level lift established,
        // extended here to the sibling per-`:children` entry `ChildSpec`
        // derive so the last M2 typed-struct sub-block
        // `#[serde(rename_all = "camelCase")]` axis on the Supervisor
        // surface without a lifted serde-key peer joins the substrate's
        // "one canonical byte-string per typed serialized-key axis"
        // discipline.
        let c = ChildSpec {
            caixa: "worker".into(),
            versao: "^0.1".into(),
            restart: RestartPolicy::Permanent,
        };
        let json = serde_json::to_string(&c).unwrap();
        for key in [
            crate::render::SUPERVISOR_CHILD_KEY_CAIXA,
            crate::render::SUPERVISOR_CHILD_KEY_VERSAO,
            crate::render::SUPERVISOR_CHILD_KEY_RESTART,
        ] {
            let quoted = format!("\"{key}\"");
            assert!(
                json.contains(&quoted),
                "serialized ChildSpec must carry the lifted \
                 SUPERVISOR_CHILD_KEY_* byte-sequence {quoted} verbatim \
                 in the JSON emission (got: {json})",
            );
        }
    }

    #[test]
    fn supervisor_child_key_consts_are_pairwise_distinct() {
        // Cross-axis drift-detection pin: a future collapse of two
        // canonical `ChildSpec` per-entry byte-strings onto the same
        // value (e.g. an accidental copy-paste flip of
        // `SUPERVISOR_CHILD_KEY_RESTART` to also read `"caixa"`) would
        // silently reroute every downstream probe on one axis onto the
        // sibling axis's overlay entry and pass every propagation-probe
        // test that expected only the stale axis's value. Peer of the
        // sibling three-way distinct pin on the `CONTRATO_KEY_*` triad
        // (ca463a4) and the two-way distinct pin on the `MEMBRO_KEY_*`
        // pair (ce80ca0).
        let all = [
            crate::render::SUPERVISOR_CHILD_KEY_CAIXA,
            crate::render::SUPERVISOR_CHILD_KEY_VERSAO,
            crate::render::SUPERVISOR_CHILD_KEY_RESTART,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(
                    a, b,
                    "SUPERVISOR_CHILD_KEY_* consts must be pairwise-\
                     distinct canonical byte-sequences — got `{a}` == `{b}`",
                );
            }
        }
    }

    #[test]
    fn supervisor_child_key_consts_are_lower_camel_case_shape() {
        // Shape-pin: every `SUPERVISOR_CHILD_KEY_*` const must be a
        // lowerCamelCase byte-sequence (no `snake_case` underscores, no
        // `kebab-case` hyphens, no leading colon, no `PascalCase` leading
        // capital, no whitespace / dots) — the canonical shape the
        // `#[serde(rename_all = "camelCase")]` derive produces on
        // `ChildSpec`. A future flip to a non-camelCase attribute at the
        // derive surfaces both here (this test fails on the
        // stale-constant shape) and at
        // `child_spec_serde_keys_match_lifted_supervisor_child_key_consts`
        // (that test fails on the mismatch between const and derive).
        // Peer with `supervisor_key_consts_are_lower_camel_case_shape`
        // (40cc4e5) on the sibling `SupervisorSpec` top-level axis.
        for key in [
            crate::render::SUPERVISOR_CHILD_KEY_CAIXA,
            crate::render::SUPERVISOR_CHILD_KEY_VERSAO,
            crate::render::SUPERVISOR_CHILD_KEY_RESTART,
        ] {
            assert!(
                !key.is_empty(),
                "SUPERVISOR_CHILD_KEY_* must be non-empty (got {key:?})"
            );
            let first = key.chars().next().unwrap();
            assert!(
                first.is_ascii_lowercase(),
                "SUPERVISOR_CHILD_KEY_* must lead with an ASCII-lowercase \
                 byte (got {key:?}, leads with {first:?})",
            );
            assert!(
                key.chars().all(|c| c.is_ascii_alphanumeric()),
                "SUPERVISOR_CHILD_KEY_* must be ASCII-alphanumeric only \
                 — no `_` / `-` / `:` / `.` / whitespace (got {key:?})",
            );
        }
    }

    // ── drift-detection: serde-derive-to-SUPERVISOR_ESTRATEGIA_* identity ────

    #[test]
    fn restart_strategy_variants_serialize_to_lifted_scalar_values() {
        // The fail-before-pass-after pin: pre-lift there was no
        // single-source binding between the [`RestartStrategy`] variant
        // name the un-`rename`d `Serialize` derive emits under
        // [`crate::render::SUPERVISOR_KEY_ESTRATEGIA`] and the byte-string
        // every downstream cluster-side dispatcher (the future
        // wasm-operator's per-supervisor sibling-restart branch, the
        // future M4 `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's
        // admission-time enum-arm bind, the `caixa-operator`'s
        // hierarchical reconciliation scheduler's per-strategy fan-out)
        // probes verbatim. A future `#[serde(rename_all = "kebab-case")]`
        // attribute on the enum — or a per-variant `#[serde(rename = "…")]`
        // override, or a variant rename in the source — would silently
        // rebrand the emitted scalar under one spelling while every
        // downstream dispatcher still probed the other, with the failure
        // surfacing at the operator's reconcile posture (subtrees coming
        // up under the `default()` `OneForOne` arm rather than the typed
        // slot's declared strategy — a bad child would then only take
        // itself down instead of the sibling set the author intended, so
        // shared-state children fall out of sync) far from the source
        // rebrand commit and with no field naming the drift. Pinning the
        // two paths (the `Serialize` derive's serialized string AND the
        // [`RestartStrategy::as_str`] helper) to the same four lifted
        // [`crate::render::SUPERVISOR_ESTRATEGIA_ONE_FOR_ONE`] /
        // [`crate::render::SUPERVISOR_ESTRATEGIA_ONE_FOR_ALL`] /
        // [`crate::render::SUPERVISOR_ESTRATEGIA_REST_FOR_ONE`] /
        // [`crate::render::SUPERVISOR_ESTRATEGIA_SIMPLE_ONE_FOR_ONE`]
        // byte-strings makes any future drift on either endpoint fail
        // here at caixa-core build time. Peer of the M3
        // `placement_strategy_variants_serialize_to_lifted_scalar_values`
        // (3f0e21c) on the sibling `PlacementStrategy` axis — same
        // three-path-convergence discipline, extended to close the
        // OTP-shaped per-supervisor sibling-restart axis.
        for (variant, expected) in [
            (
                RestartStrategy::OneForOne,
                crate::render::SUPERVISOR_ESTRATEGIA_ONE_FOR_ONE,
            ),
            (
                RestartStrategy::OneForAll,
                crate::render::SUPERVISOR_ESTRATEGIA_ONE_FOR_ALL,
            ),
            (
                RestartStrategy::RestForOne,
                crate::render::SUPERVISOR_ESTRATEGIA_REST_FOR_ONE,
            ),
            (
                RestartStrategy::SimpleOneForOne,
                crate::render::SUPERVISOR_ESTRATEGIA_SIMPLE_ONE_FOR_ONE,
            ),
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(
                json,
                format!("\"{expected}\""),
                "RestartStrategy::{variant:?} must serialize to {expected:?}"
            );
            assert_eq!(
                variant.as_str(),
                expected,
                "RestartStrategy::{variant:?}.as_str() must return the lifted \
                 SUPERVISOR_ESTRATEGIA_* constant"
            );
        }
    }

    #[test]
    fn supervisor_estrategia_consts_are_pairwise_distinct() {
        // Cross-arm drift-detection pin: a future collapse of two
        // canonical variant byte-strings onto the same value (e.g. an
        // accidental copy-paste flip of `SUPERVISOR_ESTRATEGIA_REST_FOR_ONE`
        // to also read `"OneForOne"`) would silently reroute every
        // downstream operator's per-strategy dispatch onto the sibling
        // arm's reconcile branch and pass every propagation-probe test
        // that expected only the stale arm's value — the mis-strategied
        // subtree would come up with the wrong sibling-restart posture
        // on every subsequent failure. Peer of the sibling four-way
        // distinct pin `supervisor_key_consts_are_pairwise_distinct`
        // (40cc4e5) on the top-level `SUPERVISOR_KEY_*` axis.
        let all = [
            crate::render::SUPERVISOR_ESTRATEGIA_ONE_FOR_ONE,
            crate::render::SUPERVISOR_ESTRATEGIA_ONE_FOR_ALL,
            crate::render::SUPERVISOR_ESTRATEGIA_REST_FOR_ONE,
            crate::render::SUPERVISOR_ESTRATEGIA_SIMPLE_ONE_FOR_ONE,
        ];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                if i != j {
                    assert_ne!(
                        a, b,
                        "SUPERVISOR_ESTRATEGIA_* consts must be pairwise distinct \
                         — got duplicate {a:?} at indices {i} and {j}",
                    );
                }
            }
        }
    }

    #[test]
    fn restart_strategy_display_routes_through_as_str_helper() {
        // The fail-before-pass-after pin on the first half of the
        // three-path convergence: pre-convergence the sibling
        // OTP-shape typed enum [`RestartStrategy`] carried a
        // [`std::fmt::Display`] surface via its
        // `#[discriminant(also_display)]` gen-platform derive route,
        // which arrived kebab-case as `"one-for-one"` /
        // `"one-for-all"` / `"rest-for-one"` /
        // `"simple-one-for-one"` while the wire format ran as
        // PascalCase `"OneForOne"` / `"OneForAll"` / `"RestForOne"` /
        // `"SimpleOneForOne"` through the un-`rename`d serde derive.
        // Every consumer reaching for a strategy byte-string past the
        // wire format had to pick between three paths
        // ([`RestartStrategy::as_str`], the `Serialize` derive's
        // serialized string, or `format!("{v}")` on the
        // discriminant-Display route), any two of which a future
        // variant rename or `#[serde(rename_all = "kebab-case")]`
        // attribute would silently desynchronize. Wiring
        // [`std::fmt::Display`] through [`RestartStrategy::as_str`]
        // closes the third path: every `format!("{v}")` call reaches
        // the same lifted [`crate::render::SUPERVISOR_ESTRATEGIA_*`]
        // const the wire format and the [`RestartStrategy::as_str`]
        // helper already route through, so a future variant rename
        // lands at exactly one place. Pin the routing here so a future
        // `impl std::fmt::Display for RestartStrategy`
        // reimplementation that hand-rolls the arms instead of
        // delegating to [`RestartStrategy::as_str`] fails at
        // caixa-core build time. Peer of the M3
        // `placement_strategy_display_routes_through_as_str_helper`
        // (cc8f749) which the M3 axis converged first.
        for variant in [
            RestartStrategy::OneForOne,
            RestartStrategy::OneForAll,
            RestartStrategy::RestForOne,
            RestartStrategy::SimpleOneForOne,
        ] {
            assert_eq!(
                variant.to_string(),
                variant.as_str(),
                "RestartStrategy::{variant:?} Display must route through \
                 RestartStrategy::as_str (single source of truth: the lifted \
                 SUPERVISOR_ESTRATEGIA_* const the wire format also emits)"
            );
        }
    }

    #[test]
    fn restart_strategy_display_matches_serialized_wire_byte_string() {
        // The fail-before-pass-after pin on the second half of the
        // three-path convergence: `Display` (user-facing text) agrees
        // byte-for-byte with the `Serialize` derive's wire format
        // (canonical camelCase-schema `SUPERVISOR_KEY_ESTRATEGIA`
        // scalar) on every variant. Pre-convergence the two paths
        // were structurally independent — a future
        // `#[serde(rename_all = "kebab-case")]` attribute on the
        // enum would silently rebrand the emitted wire scalar
        // (`one-for-one`, `one-for-all`, `rest-for-one`,
        // `simple-one-for-one`) while every consumer that
        // pretty-prints the strategy (the future wasm-operator's
        // per-supervisor sibling-restart-strategy diagnostic line,
        // the future `feira app graph` per-supervisor strategy line,
        // the future M4 `mesh.pleme.io/v1alpha1/Supervisor` CR
        // materializer's admission-webhook rejection body) would
        // still emit the PascalCase form the `as_str` / `Display`
        // route returns, with the mismatch surfacing at consumer
        // parse time / operator dispatch time far from the source
        // rebrand commit. Pin the two paths byte-for-byte here so any
        // future serde-attribute or variant-rename drift is a
        // caixa-core-build-time test failure at this call, not a
        // silent per-consumer dispatch miss. Peer of the M3
        // `placement_strategy_display_matches_serialized_wire_byte_string`
        // (cc8f749) which the M3 axis converged first.
        for variant in [
            RestartStrategy::OneForOne,
            RestartStrategy::OneForAll,
            RestartStrategy::RestForOne,
            RestartStrategy::SimpleOneForOne,
        ] {
            let wire = serde_json::to_string(&variant).unwrap();
            let unquoted = wire
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .expect("serialized RestartStrategy is a JSON string");
            assert_eq!(
                variant.to_string(),
                unquoted,
                "RestartStrategy::{variant:?} Display byte-string must match the \
                 Serialize derive's wire byte-string (three-path convergence: \
                 Display + as_str + Serialize all resolve to the same \
                 SUPERVISOR_ESTRATEGIA_* const)"
            );
        }
    }

    // ── drift-detection: serde-derive-to-SUPERVISOR_CHILD_RESTART_* identity ─

    #[test]
    fn restart_policy_variants_serialize_to_lifted_scalar_values() {
        // The fail-before-pass-after pin: pre-lift there was no
        // single-source binding between the [`RestartPolicy`] variant
        // name the un-`rename`d `Serialize` derive emits under
        // [`crate::render::SUPERVISOR_CHILD_KEY_RESTART`] and the
        // byte-string every downstream cluster-side dispatcher (the
        // future wasm-operator's per-child post-exit restart-decision
        // branch, the future M4 `mesh.pleme.io/v1alpha1/Supervisor` CR
        // materializer's admission-time enum-arm bind, the
        // `caixa-operator`'s hierarchical reconciliation scheduler's
        // per-child-policy fan-out) probes verbatim. A future
        // `#[serde(rename_all = "kebab-case")]` attribute on the enum —
        // or a per-variant `#[serde(rename = "…")]` override, or a
        // variant rename in the source — would silently rebrand the
        // emitted scalar under one spelling while every downstream
        // dispatcher still probed the other, with the failure surfacing
        // at the operator's reconcile posture (children coming up under
        // the `default()` `Permanent` arm rather than the typed slot's
        // declared policy — a `:temporary` `oneShot` child would be
        // restarted on clean exit, treating the successful-completion
        // signal as failure and re-running the completion-terminal
        // one-shot indefinitely; a `:transient` child that clean-exited
        // would be restarted, masking the clean-completion contract)
        // far from the source rebrand commit and with no field naming
        // the drift. Pinning the two paths (the `Serialize` derive's
        // serialized string AND the [`RestartPolicy::as_str`] helper)
        // to the same three lifted
        // [`crate::render::SUPERVISOR_CHILD_RESTART_PERMANENT`] /
        // [`crate::render::SUPERVISOR_CHILD_RESTART_TEMPORARY`] /
        // [`crate::render::SUPERVISOR_CHILD_RESTART_TRANSIENT`]
        // byte-strings makes any future drift on either endpoint fail
        // here at caixa-core build time. Peer of the sibling
        // [`restart_strategy_variants_serialize_to_lifted_scalar_values`]
        // (09ffb2d) on the per-supervisor sibling-restart-strategy axis
        // and the M3
        // `placement_strategy_variants_serialize_to_lifted_scalar_values`
        // (3f0e21c) on the per-Aplicacao distribution-strategy axis —
        // same three-path-convergence discipline, extended to close the
        // third OTP-shaped closed-enum discriminator axis on the caixa
        // typed surface (per-child restart-decision policy).
        for (variant, expected) in [
            (
                RestartPolicy::Permanent,
                crate::render::SUPERVISOR_CHILD_RESTART_PERMANENT,
            ),
            (
                RestartPolicy::Temporary,
                crate::render::SUPERVISOR_CHILD_RESTART_TEMPORARY,
            ),
            (
                RestartPolicy::Transient,
                crate::render::SUPERVISOR_CHILD_RESTART_TRANSIENT,
            ),
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(
                json,
                format!("\"{expected}\""),
                "RestartPolicy::{variant:?} must serialize to {expected:?}"
            );
            assert_eq!(
                variant.as_str(),
                expected,
                "RestartPolicy::{variant:?}.as_str() must return the lifted \
                 SUPERVISOR_CHILD_RESTART_* constant"
            );
        }
    }

    #[test]
    fn supervisor_child_restart_consts_are_pairwise_distinct() {
        // Cross-arm drift-detection pin: a future collapse of two
        // canonical variant byte-strings onto the same value (e.g. an
        // accidental copy-paste flip of `SUPERVISOR_CHILD_RESTART_TRANSIENT`
        // to also read `"Permanent"`) would silently reroute every
        // downstream operator's per-child-policy dispatch onto the
        // sibling arm's reconcile branch and pass every propagation-probe
        // test that expected only the stale arm's value — a `:transient`
        // child would come up under the `:permanent` restart-decision
        // posture on every subsequent clean exit, so a completion-terminal
        // child would be restarted indefinitely against its declared
        // policy. Peer of the sibling
        // [`supervisor_estrategia_consts_are_pairwise_distinct`]
        // (09ffb2d) on the per-supervisor sibling-restart-strategy axis
        // and the four-way distinct pin
        // `supervisor_key_consts_are_pairwise_distinct` (40cc4e5) on the
        // top-level `SUPERVISOR_KEY_*` axis.
        let all = [
            crate::render::SUPERVISOR_CHILD_RESTART_PERMANENT,
            crate::render::SUPERVISOR_CHILD_RESTART_TEMPORARY,
            crate::render::SUPERVISOR_CHILD_RESTART_TRANSIENT,
        ];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                if i != j {
                    assert_ne!(
                        a, b,
                        "SUPERVISOR_CHILD_RESTART_* consts must be pairwise distinct \
                         — got duplicate {a:?} at indices {i} and {j}",
                    );
                }
            }
        }
    }

    #[test]
    fn restart_policy_display_routes_through_as_str_helper() {
        // The fail-before-pass-after pin on the first half of the
        // three-path convergence: pre-convergence [`RestartPolicy`]
        // carried a [`std::fmt::Display`] surface via its
        // `#[discriminant(also_display)]` gen-platform derive route,
        // which arrived kebab-case as `"permanent"` / `"temporary"`
        // / `"transient"` on this three-arm enum (whose variant
        // names each collapse to their own lowercase form under the
        // kebab-case transform) while the wire format ran as
        // PascalCase `"Permanent"` / `"Temporary"` / `"Transient"`
        // through the un-`rename`d serde derive. Every consumer
        // reaching for a policy byte-string past the wire format had
        // to pick between three paths ([`RestartPolicy::as_str`],
        // the `Serialize` derive's serialized string, or
        // `format!("{v}")` on the discriminant-Display route), any
        // two of which a future variant rename or
        // `#[serde(rename_all = "kebab-case")]` attribute would
        // silently desynchronize. Wiring [`std::fmt::Display`]
        // through [`RestartPolicy::as_str`] closes the third path:
        // every `format!("{v}")` call reaches the same lifted
        // [`crate::render::SUPERVISOR_CHILD_RESTART_*`] const the
        // wire format and the [`RestartPolicy::as_str`] helper
        // already route through, so a future variant rename lands at
        // exactly one place. Pin the routing here so a future
        // `impl std::fmt::Display for RestartPolicy`
        // reimplementation that hand-rolls the arms instead of
        // delegating to [`RestartPolicy::as_str`] fails at
        // caixa-core build time. Peer of the sibling
        // [`restart_strategy_display_routes_through_as_str_helper`]
        // on the per-supervisor sibling-restart-strategy axis and
        // the M3
        // `placement_strategy_display_routes_through_as_str_helper`
        // (cc8f749) — the third of three OTP-shape closed-enum
        // discriminator axes on the caixa typed surface now
        // converged onto the same three-path
        // (Display → as_str → lifted const) discipline.
        for variant in [
            RestartPolicy::Permanent,
            RestartPolicy::Temporary,
            RestartPolicy::Transient,
        ] {
            assert_eq!(
                variant.to_string(),
                variant.as_str(),
                "RestartPolicy::{variant:?} Display must route through \
                 RestartPolicy::as_str (single source of truth: the lifted \
                 SUPERVISOR_CHILD_RESTART_* const the wire format also emits)"
            );
        }
    }

    #[test]
    fn restart_policy_display_matches_serialized_wire_byte_string() {
        // The fail-before-pass-after pin on the second half of the
        // three-path convergence: `Display` (user-facing text) agrees
        // byte-for-byte with the `Serialize` derive's wire format
        // (canonical camelCase-schema `SUPERVISOR_CHILD_KEY_RESTART`
        // scalar) on every variant. Pre-convergence the two paths
        // were structurally independent — a future
        // `#[serde(rename_all = "kebab-case")]` attribute on the
        // enum would silently rebrand the emitted wire scalar
        // (`permanent`, `temporary`, `transient`) while every
        // consumer that pretty-prints the policy (the future
        // wasm-operator's per-child post-exit restart-decision
        // diagnostic line, the future `feira app graph` per-child
        // restart column, the future M4
        // `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's
        // per-child admission-webhook rejection body) would still
        // emit the PascalCase form the `as_str` / `Display` route
        // returns, with the mismatch surfacing at consumer parse
        // time / operator dispatch time far from the source rebrand
        // commit. Pin the two paths byte-for-byte here so any future
        // serde-attribute or variant-rename drift is a
        // caixa-core-build-time test failure at this call, not a
        // silent per-consumer dispatch miss. Peer of the sibling
        // [`restart_strategy_display_matches_serialized_wire_byte_string`]
        // on the per-supervisor sibling-restart-strategy axis and
        // the M3
        // `placement_strategy_display_matches_serialized_wire_byte_string`
        // (cc8f749).
        for variant in [
            RestartPolicy::Permanent,
            RestartPolicy::Temporary,
            RestartPolicy::Transient,
        ] {
            let wire = serde_json::to_string(&variant).unwrap();
            let unquoted = wire
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .expect("serialized RestartPolicy is a JSON string");
            assert_eq!(
                variant.to_string(),
                unquoted,
                "RestartPolicy::{variant:?} Display byte-string must match the \
                 Serialize derive's wire byte-string (three-path convergence: \
                 Display + as_str + Serialize all resolve to the same \
                 SUPERVISOR_CHILD_RESTART_* const)"
            );
        }
    }

    // ── drift-detection: ChildSpec::nome accessor pins ────────────────────
    //
    // The M2 supervisor-tree sibling of the M3 `Membro::nome` (4a32abf) pin
    // pair (`membro_nome_returns_caixa_byte_equal_across_permutations` +
    // `membro_nome_borrows_from_caixa_storage`) — extended here to the M2
    // per-`:children` child-caixa `:nome` axis, sibling to the first M2
    // slot scalar accessor `UpgradeFromEntry::prior_versao` (75d27a8) on
    // the peer per-`:upgrade-from :from` axis. The three pins jointly
    // brace the accessor against every future silent detour that would
    // desynchronize it from the raw `.caixa` field access every consumer
    // previously open-coded.

    #[test]
    fn child_spec_nome_returns_caixa_byte_equal_across_permutations() {
        // The canonical per-`:children` child-caixa `:nome`-scalar pin:
        // [`ChildSpec::nome`] must return the `:children :caixa` field
        // byte-for-byte across every DNS-1123-label value the upstream
        // [`crate::render::require_valid_dns_1123_label`] gate at
        // `SupervisorSpec::validate` admits. Peer of the sibling
        // `membro_nome_returns_caixa_byte_equal_across_permutations`
        // (4a32abf) pin on the M3 per-`:membros` axis — same "the
        // substrate-primitive accessor must byte-equal the raw field
        // access verbatim across every author-declared value" discipline
        // extended to the M2 supervisor-tree per-`:children` arm. Pins
        // against a future silent detour that re-normalized the child
        // identity (an accidental `.to_lowercase()` — every `:children
        // :caixa` is validated as a DNS-1123 label upstream, so any
        // re-normalization is redundant + a drift surface between the
        // validator and the accessor), a namespace-prefix rewrite (an
        // accidental `format!("{namespace}/{caixa}")` per-CR
        // fully-qualified rewrite that didn't land on the peer axes), or
        // a per-cluster alias stamp the future wasm-operator's
        // hierarchical reconciliation scheduler authors on one consumer
        // without the others. Five values sweep the accept-set the
        // DNS-1123 gate upstream admits (short single-word / dashed /
        // v-suffixed / mixed-digit child names).
        for name in [
            "worker",
            "cache-server",
            "scratch-job",
            "orders-v2",
            "session-8080",
        ] {
            let c = ChildSpec {
                caixa: name.into(),
                versao: "^0.1".into(),
                restart: RestartPolicy::Permanent,
            };
            assert_eq!(
                c.nome(),
                name,
                "ChildSpec::nome must return :children :caixa verbatim \
                 (got {:?}, expected {name:?})",
                c.nome(),
            );
            assert_eq!(
                c.nome(),
                c.caixa.as_str(),
                "ChildSpec::nome must byte-equal the .caixa field access",
            );
        }
    }

    #[test]
    fn child_spec_nome_borrows_from_caixa_storage() {
        // The borrow-not-copy pin: [`ChildSpec::nome`] must return a
        // `&str` slice that borrows from the typed slot's own [`String`]
        // storage — same-address invariant with `c.caixa.as_str()`. Pins
        // against a future silent detour that allocated a fresh `String`
        // (`self.caixa.clone()` in the body would type-check but silently
        // drop the borrow, and every downstream consumer that assumed
        // the returned slice outlives `&self` would break on a stale-
        // reference use-after-free — the [`crate::render::insert_first_seen`]
        // dedup key at [`SupervisorSpec::validate`], the
        // [`validate_no_self_supervision`] equality check against the
        // parent's `:nome` string slice, the DNS-1123 gate's `&str`
        // borrow — each would silently misbehave if this accessor
        // produced a detached copy). Peer of the sibling
        // `membro_nome_borrows_from_caixa_storage` (4a32abf) pin on the
        // M3 per-`:membros` axis and the
        // `prior_versao_borrows_from_from_storage` (75d27a8) pin on the
        // first M2 slot scalar accessor.
        let c = ChildSpec {
            caixa: "worker".into(),
            versao: "^0.1".into(),
            restart: RestartPolicy::Permanent,
        };
        let name = c.nome();
        let caixa_slice = c.caixa.as_str();
        assert_eq!(
            name.as_ptr(),
            caixa_slice.as_ptr(),
            "ChildSpec::nome must borrow from the .caixa String's backing \
             storage — a fresh allocation here means the accessor no \
             longer names the substrate-primitive typed dispatch and \
             every downstream consumer would silently carry a detached \
             copy",
        );
        assert_eq!(
            name.len(),
            caixa_slice.len(),
            "ChildSpec::nome and .caixa.as_str() must byte-equal in length \
             as well as in address",
        );
    }

    #[test]
    fn validate_gates_child_nome_through_lifted_accessor() {
        // Bilateral coherence pin: every `:children :caixa` that
        // [`SupervisorSpec::validate`] accepts is one
        // [`crate::render::require_valid_dns_1123_label`] accepts on the
        // accessor-projected value, and vice versa on the reject side.
        // This closes the "the validator reads through the accessor"
        // contract structurally — a future silent detour that made the
        // accessor return a different byte-string than the validator
        // gates against would surface here as a coverage mismatch, not
        // as an apply-time DNS-1123 rejection at
        // `metadata.name: Invalid value` far from the caixa.lisp source.
        // Peer of the M2 sibling
        // `validate_parses_prior_versao_through_lifted_accessor`
        // (75d27a8) on the per-`:upgrade-from :from` axis and the M3
        // `validate_membros` peer discipline.
        //
        // Accept-set sweep: five DNS-1123-label values the upstream gate
        // admits.
        for ok_name in ["a", "worker", "cache-server", "orders-v2", "svc-8080"] {
            let s = SupervisorSpec {
                children: vec![ChildSpec {
                    caixa: ok_name.into(),
                    versao: "^0.1".into(),
                    restart: RestartPolicy::Permanent,
                }],
                ..SupervisorSpec::default()
            };
            s.validate().unwrap_or_else(|e| {
                panic!(
                    "SupervisorSpec::validate must accept :children :caixa {ok_name:?} \
                     (upstream DNS-1123 gate accepts it): got {e:?}",
                );
            });
            let c = ChildSpec {
                caixa: ok_name.into(),
                versao: "^0.1".into(),
                restart: RestartPolicy::Permanent,
            };
            crate::render::require_valid_dns_1123_label(c.nome(), || (), |_reason| ())
                .unwrap_or_else(|()| {
                    panic!(
                        "require_valid_dns_1123_label must accept the accessor-projected \
                     :children :caixa {ok_name:?}",
                    );
                });
        }
        // Reject-set sweep: five DNS-1123-label-violating shapes the
        // upstream gate refuses (empty / uppercase / underscore / dot /
        // leading-hyphen). Every rejection at the validator must
        // correspond to a rejection when the accessor's projected value
        // is fed back through the shared gate.
        for bad_name in ["", "Worker", "my_worker", "team.worker", "-worker"] {
            let s = SupervisorSpec {
                children: vec![ChildSpec {
                    caixa: bad_name.into(),
                    versao: "^0.1".into(),
                    restart: RestartPolicy::Permanent,
                }],
                ..SupervisorSpec::default()
            };
            let err = s.validate().unwrap_err();
            assert!(
                matches!(
                    err,
                    SupervisorError::EmptyChildName | SupervisorError::ChildCaixaInvalid { .. }
                ),
                "SupervisorSpec::validate must reject :children :caixa {bad_name:?} \
                 via the DNS-1123 gate: got {err:?}",
            );
            let c = ChildSpec {
                caixa: bad_name.into(),
                versao: "^0.1".into(),
                restart: RestartPolicy::Permanent,
            };
            assert!(
                crate::render::require_valid_dns_1123_label(c.nome(), || (), |_reason| (),)
                    .is_err(),
                "require_valid_dns_1123_label must reject the accessor-projected \
                 :children :caixa {bad_name:?}",
            );
        }
    }

    // ── drift-detection: ChildSpec::versao_requirement accessor pins ──────
    //
    // Sibling of the peer per-`:membros` `membro_versao_requirement_*`
    // (a40b0e3) pin pair on the M3 mesh-slot surface — extended here to the
    // M2 supervisor-tree per-`:children` child-`:versao` axis, sibling to
    // the just-landed [`ChildSpec::nome`] (57c61d0) child-`:nome` pin
    // trio on the peer per-`:children` `String`-carry axis. The three pins
    // jointly brace the accessor against every future silent detour that
    // would desynchronize it from the raw `.versao` field access the
    // requirement gate + error carrier previously open-coded.
    //
    // Closes the last unlifted per-`:children` `String`-carry axis: the
    // pair (`nome`, `versao_requirement`) now jointly projects the
    // (`.caixa`, `.versao`) field pair every OTP-shape supervisor-tree
    // consumer that fans on per-child identity + version pin reads,
    // matching the peer M3 (`Membro::nome`, `Membro::versao_requirement`)
    // pair discipline verbatim.
    #[test]
    fn child_spec_versao_requirement_returns_versao_byte_equal_across_permutations() {
        // The canonical per-`:children` child-`:versao`-scalar pin:
        // [`ChildSpec::versao_requirement`] must return the `:children
        // :versao` field byte-for-byte across every Cargo-shaped semver
        // requirement value the upstream
        // [`crate::render::require_valid_versao_requirement`] gate admits.
        // Peer of the sibling
        // `membro_versao_requirement_returns_versao_byte_equal_across_permutations`
        // (a40b0e3) pin on the M3 per-`:membros` axis — same "the
        // substrate-primitive accessor must byte-equal the raw field
        // access verbatim across every author-declared value" discipline
        // extended to the M2 supervisor-tree per-`:children` arm. Pins
        // against a future silent detour that re-canonicalized the
        // requirement (an accidental `.to_string()` via
        // [`crate::version::parse_requirement`] → [`std::fmt::Display`]
        // round-trip that collapsed `"^0.1"` to `">=0.1, <0.2"` and
        // silently drifted the error carrier's quoted requirement away
        // from the source `caixa.lisp`, an accidental whitespace trim on
        // `"^ 0.1"` that no consumer ever produced from the field-access
        // side, an accidental per-cluster lacre-projected concrete-version
        // rewrite that didn't land on the peer requirement-gate call).
        // Five values sweep the accept-set the shared
        // [`crate::render::require_valid_versao_requirement`] gate admits
        // (caret / tilde / exact / wildcard / bare-major).
        for req in ["^0.1", "~0.1.2", "0.1.0", "*", "^1"] {
            let c = ChildSpec {
                caixa: "worker".into(),
                versao: req.into(),
                restart: RestartPolicy::Permanent,
            };
            assert_eq!(
                c.versao_requirement(),
                req,
                "ChildSpec::versao_requirement must return :children :versao \
                 verbatim (got {:?}, expected {req:?})",
                c.versao_requirement(),
            );
            assert_eq!(
                c.versao_requirement(),
                c.versao.as_str(),
                "ChildSpec::versao_requirement must byte-equal the .versao \
                 field access",
            );
        }
    }

    #[test]
    fn child_spec_versao_requirement_borrows_from_versao_storage() {
        // The borrow-not-copy pin: [`ChildSpec::versao_requirement`] must
        // return a `&str` slice that borrows from the typed slot's own
        // [`String`] storage — same-address invariant with
        // `c.versao.as_str()`. Pins against a future silent detour that
        // allocated a fresh `String` (`self.versao.clone()` in the body
        // would type-check but silently drop the borrow, and every
        // downstream consumer that assumed the returned slice outlives
        // `&self` — the [`crate::render::require_valid_versao_requirement`]
        // gate's `&str` borrow, the [`SupervisorError::ChildVersaoInvalid`]
        // `.to_string()` carrier's byte-length assumption — would silently
        // misbehave if this accessor produced a detached copy). Peer of
        // the sibling `child_spec_nome_borrows_from_caixa_storage`
        // (57c61d0) pin on the per-`:children` `:nome` axis and the M3
        // `membro_versao_requirement_borrows_from_versao_storage` (a40b0e3)
        // pin on the peer per-`:membros` `:versao` axis.
        let c = ChildSpec {
            caixa: "worker".into(),
            versao: "^0.1".into(),
            restart: RestartPolicy::Permanent,
        };
        let req = c.versao_requirement();
        let versao_slice = c.versao.as_str();
        assert_eq!(
            req.as_ptr(),
            versao_slice.as_ptr(),
            "ChildSpec::versao_requirement must borrow from the .versao \
             String's backing storage — a fresh allocation here means the \
             accessor no longer names the substrate-primitive typed \
             dispatch and every downstream consumer would silently carry \
             a detached copy",
        );
        assert_eq!(
            req.len(),
            versao_slice.len(),
            "ChildSpec::versao_requirement and .versao.as_str() must \
             byte-equal in length as well as in address",
        );
    }

    #[test]
    fn validate_gates_child_versao_through_lifted_accessor() {
        // Bilateral coherence pin: every `:children :versao` that
        // [`SupervisorSpec::validate`] accepts is one
        // [`crate::render::require_valid_versao_requirement`] accepts on
        // the accessor-projected value, and vice versa on the reject side.
        // This closes the "the validator reads through the accessor"
        // contract structurally — a future silent detour that made the
        // accessor return a different byte-string than the validator gates
        // against would surface here as a coverage mismatch, not as a
        // resolver-time semver-parse rejection at lacre-closure time far
        // from the caixa.lisp source. Peer of the sibling
        // `validate_gates_child_nome_through_lifted_accessor` (57c61d0) on
        // the per-`:children :caixa` axis and the M2
        // `validate_parses_prior_versao_through_lifted_accessor` (75d27a8)
        // on the peer per-`:upgrade-from :from` axis.
        //
        // Accept-set sweep: five Cargo-shaped semver requirement values
        // the upstream gate admits (caret / tilde / exact / wildcard /
        // bare-major).
        for ok_req in ["^0.1", "~0.1.2", "0.1.0", "*", "^1"] {
            let s = SupervisorSpec {
                children: vec![ChildSpec {
                    caixa: "worker".into(),
                    versao: ok_req.into(),
                    restart: RestartPolicy::Permanent,
                }],
                ..SupervisorSpec::default()
            };
            s.validate().unwrap_or_else(|e| {
                panic!(
                    "SupervisorSpec::validate must accept :children :versao {ok_req:?} \
                     (upstream versao-requirement gate accepts it): got {e:?}",
                );
            });
            let c = ChildSpec {
                caixa: "worker".into(),
                versao: ok_req.into(),
                restart: RestartPolicy::Permanent,
            };
            crate::render::require_valid_versao_requirement(
                c.versao_requirement(),
                || (),
                |_reason| (),
            )
            .unwrap_or_else(|()| {
                panic!(
                    "require_valid_versao_requirement must accept the accessor-projected \
                     :children :versao {ok_req:?}",
                );
            });
        }
        // Reject-set sweep: five requirement-violating shapes the upstream
        // gate refuses. The empty string closes the empty-first arm of the
        // shared [`crate::render::require_valid_versao_requirement`]
        // cascade; the four non-empty arms exercise distinct semver-parse
        // failure modes the M3 peer per-`:membros` reject-set already pins
        // (`rejects_invalid_membro_versao_requirement` on `^bad-version`,
        // `rejects_membro_versao_with_double_caret_typo` on `^^0.1`,
        // `rejects_membro_versao_with_v_prefixed_tag` on `v0.1`) — the
        // shared parser routing means the same reject-set must fail
        // identically at the M2 supervisor-tree per-`:children` accessor
        // arm here. Every rejection at the validator must correspond to a
        // rejection when the accessor's projected value is fed back
        // through the shared gate.
        //
        // (Bare partial magnitudes like `"0.1"` and bare identifiers like
        // `"not-a-semver"` are intentionally *not* in the reject-set: the
        // semver crate accepts `"0.1"` as an implicit `^0.1` requirement,
        // and the identifier-tail arm's grammar admits some non-canonical
        // shapes — matching what the M3 peer test suite already documents
        // as the shared parser's accept-set edges.)
        for bad_req in ["", "v0.1.0", "^bad-version", "^^0.1", "v0.1"] {
            let s = SupervisorSpec {
                children: vec![ChildSpec {
                    caixa: "worker".into(),
                    versao: bad_req.into(),
                    restart: RestartPolicy::Permanent,
                }],
                ..SupervisorSpec::default()
            };
            let err = s.validate().unwrap_err();
            assert!(
                matches!(
                    err,
                    SupervisorError::EmptyChildVersion { .. }
                        | SupervisorError::ChildVersaoInvalid { .. }
                ),
                "SupervisorSpec::validate must reject :children :versao {bad_req:?} \
                 via the versao-requirement gate: got {err:?}",
            );
            let c = ChildSpec {
                caixa: "worker".into(),
                versao: bad_req.into(),
                restart: RestartPolicy::Permanent,
            };
            assert!(
                crate::render::require_valid_versao_requirement(
                    c.versao_requirement(),
                    || (),
                    |_reason| (),
                )
                .is_err(),
                "require_valid_versao_requirement must reject the accessor-projected \
                 :children :versao {bad_req:?}",
            );
        }
    }

    // ── per-`:children` `:restart` typed-accessor coherence pins ──────────
    //
    // The [`ChildSpec::restart`] accessor lift closes the last unlifted
    // per-`:children` axis (the pair `nome()` + `versao_requirement()`
    // already project the `String`-carry `(caixa, versao)` fields; the
    // `Copy`-composite-enum `restart` field is the third and final axis).
    // Peer of the sibling per-`:supervisor` [`SupervisorSpec::estrategia`]
    // (eafb619) `Copy`-return [`RestartStrategy`] sibling-restart-strategy
    // scalar accessor and the M3 mesh-slot [`crate::Placement::estrategia`]
    // (921fe1b) `Copy`-return [`crate::PlacementStrategy`] distribution-
    // strategy scalar accessor — same "one typed dispatch on the substrate
    // primitive, `Copy`-projected closed-set enum-arm discriminator" shape
    // extended onto the M2 supervisor-slot per-`:children` restart-decision
    // axis. The pin below covers the accessor's byte-equal projection
    // against the raw field access across every variant in the closed
    // accept-set (`Permanent`, `Transient`, `Temporary`).

    #[test]
    fn child_spec_restart_returns_restart_verbatim_across_permutations() {
        // The canonical per-`:children` restart-decision-policy-scalar
        // pin: [`ChildSpec::restart`] must return the `:children :restart`
        // field verbatim as a [`RestartPolicy`], `Copy`-projected from the
        // typed slot's own [`RestartPolicy`] storage across every variant
        // in the closed accept-set (`Permanent`, `Transient`, `Temporary`).
        // Pins against a future silent detour that re-derived the policy
        // from a peer axis (an accidental fallback to
        // `if is_supervisor_child { Permanent } else { Temporary }` that
        // collapsed the child's kind axis into the restart discriminator),
        // a variant remap the operator authors on one consumer without the
        // other, or a stale-derive detour that substituted
        // [`RestartPolicy::default`] when the field held any explicit
        // variant (which would silently collapse the distinction between
        // "author explicitly declared `:restart Permanent`" and "author
        // omitted the slot and inherited the default" the future
        // per-cluster restart-decision override slot depends on).
        //
        // Peer of the sibling per-`:supervisor`
        // `supervisor_spec_estrategia_returns_estrategia_verbatim_across_permutations`
        // (eafb619) pin on the M2 supervisor-slot sibling-restart-strategy
        // axis and the M3
        // `placement_estrategia_returns_estrategia_verbatim_across_permutations`
        // (921fe1b) pin on the per-`:placement` distribution-strategy axis
        // — same "the substrate-primitive accessor must byte-equal the raw
        // field access verbatim across every author-declared value"
        // discipline extended onto the M2 supervisor-slot per-`:children`
        // restart-decision-policy axis, closing the last unlifted axis on
        // the per-`:children` [`ChildSpec`] type.
        for restart in [
            RestartPolicy::Permanent,
            RestartPolicy::Transient,
            RestartPolicy::Temporary,
        ] {
            let c = ChildSpec {
                caixa: "worker".into(),
                versao: "^0.1".into(),
                restart,
            };
            assert_eq!(
                c.restart(),
                restart,
                "ChildSpec::restart must return :children :restart \
                 verbatim (got {:?}, expected {restart:?})",
                c.restart(),
            );
            assert_eq!(
                c.restart(),
                c.restart,
                "ChildSpec::restart accessor and .restart field access \
                 must byte-equal — the accessor is the substrate-primitive \
                 typed dispatch every downstream per-child restart-\
                 decision consumer must route through",
            );
        }
    }

    // ── per-`:supervisor` `:estrategia` typed-accessor coherence pins ─────
    //
    // The [`SupervisorSpec::estrategia`] accessor lift extends the peer M3
    // [`crate::Placement::estrategia`] (921fe1b) `Copy`-return
    // distribution-strategy accessor discipline onto the M2 supervisor-slot
    // per-`:supervisor` sibling-restart-strategy `Copy`-composite-enum
    // scalar axis. The two pins below cover (1) the accessor's byte-equal
    // projection against the raw field access across every variant in the
    // closed accept-set, and (2) the two-consumer coherence between the
    // [`SupervisorSpec::validate`] partition-dispatch `match` arm and the
    // non-`SimpleOneForOne`-arm [`SupervisorError::NoChildren`] error
    // carrier's `estrategia:` field — peer of the sibling M3
    // `placement_estrategia_returns_estrategia_verbatim_across_permutations`
    // / `validate_placement_reads_through_lifted_estrategia_accessor` pin
    // pair on the per-`:placement` distribution-strategy axis.

    #[test]
    fn supervisor_spec_estrategia_returns_estrategia_verbatim_across_permutations() {
        // The canonical per-`:supervisor` sibling-restart-strategy-scalar
        // pin: [`SupervisorSpec::estrategia`] must return the
        // `:supervisor :estrategia` field verbatim as a
        // [`RestartStrategy`], `Copy`-projected from the typed slot's own
        // [`RestartStrategy`] storage across every variant in the closed
        // accept-set (`OneForOne`, `OneForAll`, `RestForOne`,
        // `SimpleOneForOne`). Pins against a future silent detour that
        // re-derived the strategy from a peer axis (an accidental
        // fallback to `if children.is_empty() { SimpleOneForOne } else {
        // OneForOne }` collapse that read the children-count axis into
        // the strategy discriminator), a variant remap the operator
        // authors on one consumer without the other, or a stale-derive
        // detour that substituted [`RestartStrategy::default`] when the
        // field held any explicit variant (which would silently collapse
        // the distinction between "author explicitly declared
        // `:estrategia OneForOne`" and "author omitted the slot and
        // inherited the default" the future per-cluster strategy override
        // slot depends on). Peer of the sibling M3
        // `placement_estrategia_returns_estrategia_verbatim_across_permutations`
        // (921fe1b) pin on the M3 mesh-slot `Copy`-composite-enum scalar
        // axis — same "the substrate-primitive accessor must byte-equal
        // the raw field access verbatim across every author-declared
        // value" discipline extended onto the M2 supervisor-slot
        // per-`:supervisor` sibling-restart-strategy axis.
        for estrategia in [
            RestartStrategy::OneForOne,
            RestartStrategy::OneForAll,
            RestartStrategy::RestForOne,
            RestartStrategy::SimpleOneForOne,
        ] {
            // `SimpleOneForOne` requires `children.is_empty()`; the peer
            // three strategies require a non-empty static children list.
            // Build each shape coherently so the pin's fixture would
            // itself pass [`SupervisorSpec::validate`] once fed through
            // the sibling coherence pin below — the byte-equal projection
            // asserted here is a strictly weaker property (a `Copy` field
            // read) that does not depend on `validate` running, but
            // keeping the fixture validate-clean means a future extension
            // of the pin to exercise `validate` end-to-end does not have
            // to re-author the children shape.
            //
            // Route the `SimpleOneForOne ↔ non-SimpleOneForOne` fixture-
            // shape partition through the [`gen_platform::IsVariant`]
            // derive-generated
            // [`RestartStrategy::is_simple_one_for_one`] predicate rather
            // than the raw `matches!(estrategia, RestartStrategy::
            // SimpleOneForOne)` open-coded pattern-match — same closed-
            // set-typed-enum arm-discriminator dispatch discipline the
            // sibling [`crate::upgrade::UpgradeInstruction::is_restart`]
            // convergence (915a934) extended onto its two paired positive
            // / negated `matches!` sites and the peer
            // [`crate::aplicacao::PlacementStrategy`] `IsVariant`-derived
            // predicate convergence (766ec63) extended onto the M3 mesh-
            // slot per-`:placement` distribution-strategy discriminator
            // axis. See the sibling `round_trip_all_strategies` and the
            // peer `manifest::tests::
            // caixa_estrategia_and_supervisor_view_reads_through_lifted_estrategia_accessor`
            // fixture for the two peer sites the same lift closes on.
            let children = if estrategia.is_simple_one_for_one() {
                Vec::new()
            } else {
                vec![ChildSpec {
                    caixa: "worker".into(),
                    versao: "^0.1".into(),
                    restart: RestartPolicy::Permanent,
                }]
            };
            let s = SupervisorSpec {
                estrategia,
                children,
                ..SupervisorSpec::default()
            };
            assert_eq!(
                s.estrategia(),
                estrategia,
                "SupervisorSpec::estrategia must return :supervisor :estrategia \
                 verbatim (got {:?}, expected {estrategia:?})",
                s.estrategia(),
            );
            assert_eq!(
                s.estrategia(),
                s.estrategia,
                "SupervisorSpec::estrategia accessor and .estrategia field \
                 access must byte-equal — the accessor is the substrate-\
                 primitive typed dispatch every downstream sibling-restart-\
                 strategy consumer must route through",
            );
        }
    }

    #[test]
    fn validate_reads_through_lifted_estrategia_accessor() {
        // Two-consumer coherence pin: the [`SupervisorSpec::validate`]
        // `SimpleOneForOne ↔ non-SimpleOneForOne` `match` partition
        // dispatch (which reads through [`SupervisorSpec::estrategia`]
        // to fan across the strategy-arm shape-gate cascades) and the
        // non-`SimpleOneForOne`-arm [`SupervisorError::NoChildren`]
        // error carrier's `estrategia:` field (which reads through
        // [`SupervisorSpec::estrategia`] to name the strategy the empty
        // `:children` list was declared against) must both key off the
        // lifted accessor, so any future rebrand on the typed slot's
        // reader shape lands at exactly one place. Pins the two-site
        // coherence by exercising the `NoChildren` error surface end-to-
        // end across every non-`SimpleOneForOne` variant and asserting
        // the surfaced `estrategia:` field byte-equals the accessor's
        // return. Peer of the sibling M3
        // `validate_placement_reads_through_lifted_estrategia_accessor`
        // (921fe1b) three-consumer coherence pin on the per-`:placement`
        // distribution-strategy axis.
        for estrategia in [
            RestartStrategy::OneForOne,
            RestartStrategy::OneForAll,
            RestartStrategy::RestForOne,
        ] {
            let s = SupervisorSpec {
                estrategia,
                children: Vec::new(),
                ..SupervisorSpec::default()
            };
            let err = s.validate().unwrap_err();
            match err {
                SupervisorError::NoChildren { estrategia: e } => {
                    assert_eq!(
                        e,
                        s.estrategia(),
                        "NoChildren.estrategia must byte-equal \
                         SupervisorSpec::estrategia() — the empty-`:children` \
                         refusal reads through the lifted accessor",
                    );
                    assert_eq!(
                        e, estrategia,
                        "NoChildren.estrategia must carry the author-declared \
                         :supervisor :estrategia variant verbatim (got {e:?}, \
                         expected {estrategia:?})",
                    );
                }
                other => panic!("expected NoChildren, got {other:?} for estrategia={estrategia:?}"),
            }
        }
    }

    // ── per-`:supervisor` `:max-restarts` typed-accessor coherence pins ────
    //
    // The [`SupervisorSpec::max_restarts`] accessor lift extends the peer M3
    // [`crate::CircuitBreaker::max_failures`] (3a74062) `Copy`-return
    // required-`u32` scalar accessor discipline onto the M2 supervisor-slot
    // per-`:supervisor` restart-budget-count `Copy`-`u32` scalar axis.
    // The two pins below cover (1) the accessor's byte-equal projection
    // against the raw field access across every representative value in
    // the `u32` accept-set (`1` lower boundary, `SUPERVISOR_MAX_RESTARTS_MAX`
    // upper boundary, `0` past-the-guard zero sentinel, `u32::MAX`
    // past-the-guard cap sentinel), and (2) the [`SupervisorSpec::validate`]
    // zero-floor / cap composition — the validate gate and the accessor
    // must route through the same substrate-primitive typed dispatch, so
    // any future silent detour that had the accessor perform a
    // bounds-collapsing clamp would fail here at caixa-core build time.
    // Peer of the sibling M3
    // `circuit_breaker_max_failures_returns_max_failures_u32_byte_equal_across_permutations`
    // (3a74062) pin on the per-`CircuitBreaker :max-failures` axis.

    #[test]
    fn supervisor_spec_max_restarts_returns_max_restarts_u32_byte_equal_across_permutations() {
        // The canonical per-`:supervisor` restart-budget-count scalar pin:
        // [`SupervisorSpec::max_restarts`] must return the `:supervisor
        // :max-restarts` typed `u32` verbatim, `Copy`-projected from the
        // typed slot's own `u32` storage, byte-equal to the raw field
        // access across every representative value in the accept-set —
        // `1` (the lower boundary of the `1..=SUPERVISOR_MAX_RESTARTS_MAX`
        // accept-set the surrounding [`SupervisorSpec::validate`] gate
        // carves out on the sibling `ZeroMaxRestarts` refusal),
        // `SUPERVISOR_MAX_RESTARTS_MAX` (the upper boundary the same gate
        // carves out on the sibling `MaxRestartsExceedsCap` refusal), `0`
        // (a past-the-guard sentinel that pins the accessor doesn't
        // perform a silent bounds-collapse into `1` on the zero arm —
        // validate rejects zero but the accessor must ship the raw slot
        // verbatim so a validate-time gate regression surfaces at the
        // emit boundary rather than being silently absorbed), `u32::MAX`
        // (a past-the-guard sentinel that pins the accessor doesn't
        // perform a silent bounds-collapse through
        // `SUPERVISOR_MAX_RESTARTS_MAX` at the return path).
        //
        // Peer of the sibling M3
        // `circuit_breaker_max_failures_returns_max_failures_u32_byte_equal_across_permutations`
        // (3a74062) pin on the M3 mesh-slot `Copy`-`u32` sub-struct
        // required-scalar axis — same "the substrate-primitive accessor
        // must byte-equal the raw field access verbatim across every
        // value in the `u32` accept-set" discipline extended onto the M2
        // supervisor-slot per-`:supervisor` restart-budget-count axis.
        for max_restarts in [1u32, SUPERVISOR_MAX_RESTARTS_MAX, 0, u32::MAX] {
            let s = SupervisorSpec {
                max_restarts,
                ..SupervisorSpec::default()
            };
            assert_eq!(
                s.max_restarts(),
                max_restarts,
                "SupervisorSpec::max_restarts must return :supervisor \
                 :max-restarts verbatim (got {}, expected {max_restarts})",
                s.max_restarts(),
            );
            assert_eq!(
                s.max_restarts(),
                s.max_restarts,
                "SupervisorSpec::max_restarts accessor and .max_restarts \
                 field access must byte-equal — the accessor is the \
                 substrate-primitive typed dispatch every downstream \
                 restart-budget-count consumer must route through",
            );
        }
    }

    #[test]
    fn validate_max_restarts_zero_floor_and_cap_arms_route_through_accessor() {
        // Composition pin: [`SupervisorSpec::validate`]'s `:max-restarts`
        // zero-floor + upper-cap bracket must key off
        // [`SupervisorSpec::max_restarts`], not the raw `.max_restarts`
        // field access. Structurally: a `SupervisorSpec { max_restarts:
        // 0, .. }` must surface the `ZeroMaxRestarts` refusal exactly, a
        // `SupervisorSpec { max_restarts: SUPERVISOR_MAX_RESTARTS_MAX + 1,
        // .. }` must surface the `MaxRestartsExceedsCap` refusal exactly
        // (with the offending count carried verbatim from the accessor
        // return), and a `SupervisorSpec { max_restarts: 1, .. }` (the
        // lower boundary of the accept-set) plus a `SupervisorSpec {
        // max_restarts: SUPERVISOR_MAX_RESTARTS_MAX, .. }` (the upper
        // boundary) must pass validate. The four together jointly pin the
        // accessor + validate-gate composition: any future silent detour
        // that had the accessor return a fresh `1` on the zero arm (a
        // `.max_restarts().max(1)` collapse) would silently absorb the
        // `ZeroMaxRestarts` refusal at the accessor boundary and the
        // validate gate would accept a struct-literal `SupervisorSpec {
        // max_restarts: 0, .. }` — the composition pin catches that at
        // caixa-core build time.
        //
        // Peer of the sibling M3
        // `validate_politicas_max_failures_zero_floor_arm_routes_through_accessor`
        // (3a74062) pin on the sibling per-`CircuitBreaker :max-failures`
        // composition axis — same "the validate / shape-gate predicate
        // must route through the substrate-primitive typed dispatch"
        // discipline extended onto the peer M2 supervisor-slot
        // required-`u32` composition axis.
        let child = ChildSpec {
            caixa: "worker".into(),
            versao: "^0.1".into(),
            restart: RestartPolicy::Permanent,
        };
        // Zero-floor arm.
        let s = SupervisorSpec {
            max_restarts: 0,
            children: vec![child.clone()],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::ZeroMaxRestarts,
            "validate must reject max_restarts == 0 with ZeroMaxRestarts \
             — the accessor and the validate gate must route through the \
             same substrate-primitive typed dispatch on the zero-floor arm",
        );
        // Cap arm — the surfaced `max_restarts:` field must byte-equal
        // the accessor's return so a future rebrand on the accessor
        // lands in the diagnostic without a coordinated rewrite.
        let over_cap = SUPERVISOR_MAX_RESTARTS_MAX + 1;
        let s = SupervisorSpec {
            max_restarts: over_cap,
            children: vec![child.clone()],
            ..SupervisorSpec::default()
        };
        match s.validate().unwrap_err() {
            SupervisorError::MaxRestartsExceedsCap { max_restarts } => {
                assert_eq!(
                    max_restarts,
                    s.max_restarts(),
                    "MaxRestartsExceedsCap.max_restarts must byte-equal \
                     SupervisorSpec::max_restarts() — the cap-arm refusal \
                     reads through the lifted accessor",
                );
                assert_eq!(
                    max_restarts, over_cap,
                    "MaxRestartsExceedsCap.max_restarts must carry the \
                     author-declared :supervisor :max-restarts value \
                     verbatim (got {max_restarts}, expected {over_cap})",
                );
            }
            other => panic!("expected MaxRestartsExceedsCap, got {other:?}"),
        }
        // Lower + upper accept-set boundaries.
        for max_restarts in [1u32, SUPERVISOR_MAX_RESTARTS_MAX] {
            let s = SupervisorSpec {
                max_restarts,
                children: vec![child.clone()],
                ..SupervisorSpec::default()
            };
            assert!(
                s.validate().is_ok(),
                "validate must accept max_restarts == {max_restarts} \
                 (an accept-set boundary of \
                 1..=SUPERVISOR_MAX_RESTARTS_MAX)",
            );
        }
    }

    // ── per-`:supervisor` `:restart-window` typed-accessor coherence pins ─
    //
    // The [`SupervisorSpec::restart_window`] accessor lift extends the peer
    // M2 [`crate::LimitsSpec::wall_clock`] (8cb717b) `Option<Duration>`
    // accessor discipline and the peer M3 [`crate::MeshPolicy::timeout`]
    // (7073d0f) `Option<Duration>` accessor discipline onto the M2
    // supervisor-slot per-`:supervisor` restart-intensity-denominator
    // `Option<Duration>` scalar axis — third `Copy`-return accessor on the
    // M2 supervisor-slot `SupervisorSpec` type, closing the last unlifted
    // per-`:supervisor` scalar-value axis. The three pins below cover
    // (1) the accessor's byte-equal projection against the raw field
    // access across every representative value in the `Option<Duration>`
    // accept-set (`None` never-reset sentinel, `Some(Duration::from_millis(1))`
    // lower boundary, `Some(SUPERVISOR_RESTART_WINDOW_MAX)` upper boundary,
    // `Some(Duration::ZERO)` past-the-guard zero sentinel, `Some(Duration::MAX)`
    // past-the-guard above-cap sentinel), (2) the [`SupervisorSpec::validate`]
    // `if let Some(w) = self.restart_window() { … }` bracket-arm
    // composition — the validate gate and the accessor must route through
    // the same substrate-primitive typed dispatch, so any future silent
    // detour that had the accessor perform a bounds-collapsing clamp
    // would fail here at caixa-core build time, and (3) the accessor's
    // by-copy idempotence pin — the returned `Option<Duration>` must
    // outlive `&self` and two successive calls must return byte-equal
    // values. Peer of the sibling M2
    // `limits_wall_clock_returns_option_duration_byte_equal_across_permutations`
    // (8cb717b) pin on the per-`:limits :wall-clock` axis and the sibling
    // M3 `mesh_policy_timeout_returns_timeout_option_byte_equal_across_permutations`
    // (7073d0f) pin on the per-`:politicas :timeout` axis.

    #[test]
    fn supervisor_spec_restart_window_returns_option_duration_byte_equal_across_permutations() {
        // The canonical per-`:supervisor` restart-intensity-denominator
        // scalar pin: [`SupervisorSpec::restart_window`] must return the
        // `:supervisor :restart-window` typed [`Duration`] verbatim as an
        // `Option<Duration>`, `Copy`-projected from the typed slot's own
        // `Option<Duration>` storage, byte-equal to the raw field access
        // across every representative value in the accept-set — `None`
        // (the "never reset — every restart across the supervisor's
        // lifetime counts against the sibling `:max-restarts` budget"
        // sentinel the field's own docstring names and the peer
        // `validate_accepts_none_restart_window` pin locks in on the
        // [`SupervisorSpec::validate`] entry-side),
        // `Some(Duration::from_millis(1))` (the structural minimum a
        // validated `:restart-window` may carry, the integer-millisecond
        // floor [`SupervisorError::RestartWindowNotCanonical`] rejects
        // everything sub-ms; `Duration::ZERO` is separately rejected by
        // [`SupervisorError::RestartWindowZero`]),
        // `Some(SUPERVISOR_RESTART_WINDOW_MAX)` (the upper boundary the
        // surrounding [`SupervisorSpec::validate`] gate carves out on the
        // sibling [`SupervisorError::RestartWindowExceedsCap`] refusal),
        // `Some(Duration::ZERO)` (a past-the-guard sentinel that pins the
        // accessor doesn't perform a silent bounds-collapse into `None` on
        // the zero-Duration arm — validate rejects zero but the accessor
        // must ship the raw slot verbatim so a validate-time gate
        // regression surfaces at the emit boundary rather than being
        // silently absorbed), and `Some(Duration::MAX)` (a past-the-guard
        // sentinel that pins the accessor doesn't perform a silent
        // bounds-collapse through [`SUPERVISOR_RESTART_WINDOW_MAX`] at the
        // return path).
        //
        // Peer of the sibling M2
        // `limits_wall_clock_returns_option_duration_byte_equal_across_permutations`
        // (8cb717b) pin on the per-`:limits :wall-clock` axis and the
        // sibling M3
        // `mesh_policy_timeout_returns_timeout_option_byte_equal_across_permutations`
        // (7073d0f) pin on the per-`:politicas :timeout` axis — same "the
        // substrate-primitive accessor must byte-equal the raw field
        // access verbatim across every value in the `Option<Duration>`
        // accept-set" discipline extended onto the M2 supervisor-slot
        // per-`:supervisor` `Option<Duration>` axis. Pins against a future
        // silent detour that re-derived the restart-window from a peer
        // axis (an accidental `.max_restarts.into()` collapse that read
        // the restart-budget-count as a duration — the two axes serve
        // different halves of the `MaxIntensity / Period` restart-
        // intensity ratio, and confusing them silently inverts the
        // ratio's numerator and denominator), a `None → Some(Duration::ZERO)`
        // "zero means never reset" collapse (the canonical
        // `Option<Duration>` → `Duration` collapse footgun the
        // [`SupervisorError::RestartWindowZero`] validate arm guards on
        // the peer zero-floor axis; a zero period either trips on the
        // first failure or never trips depending on operator
        // interpretation, neither of which is the author's "never reset"
        // intent that `None` expresses structurally), or a per-arm
        // variant swap that landed on one consumer without the other.
        for restart_window in [
            None,
            Some(Duration::from_millis(1)),
            Some(SUPERVISOR_RESTART_WINDOW_MAX),
            Some(Duration::ZERO),
            Some(Duration::MAX),
        ] {
            let s = SupervisorSpec {
                restart_window,
                ..SupervisorSpec::default()
            };
            assert_eq!(
                s.restart_window(),
                restart_window,
                "SupervisorSpec::restart_window must return :supervisor \
                 :restart-window verbatim (got {:?}, expected {restart_window:?})",
                s.restart_window(),
            );
            assert_eq!(
                s.restart_window(),
                s.restart_window,
                "SupervisorSpec::restart_window accessor and \
                 .restart_window field access must byte-equal — the \
                 accessor is the substrate-primitive typed dispatch every \
                 downstream restart-intensity-denominator consumer must \
                 route through",
            );
        }
    }

    #[test]
    fn validate_restart_window_bracket_arm_routes_through_accessor() {
        // Composition pin: [`SupervisorSpec::validate`]'s
        // `:restart-window` `if let Some(w) = self.restart_window() { … }`
        // zero-floor + integer-millisecond canonical-form + upper-cap
        // bracket-arm must key off [`SupervisorSpec::restart_window`], not
        // the raw `.restart_window` field access. Structurally: a
        // `SupervisorSpec { restart_window: None, .. }` must pass the
        // arm gate structurally (the `if let Some(_)` shape returns
        // early on the `None` arm — the accessor and the validate gate
        // must agree on `None → skip the bracket cascade` so an authored
        // `:restart-window ()` structurally routes through the "never
        // reset" sentinel path), a `SupervisorSpec { restart_window:
        // Some(Duration::ZERO), .. }` must surface the `RestartWindowZero`
        // refusal exactly, a `SupervisorSpec { restart_window:
        // Some(Duration::from_micros(1500)), .. }` must surface the
        // `RestartWindowNotCanonical` refusal exactly (with the offending
        // duration carried verbatim from the accessor return), a
        // `SupervisorSpec { restart_window: Some(SUPERVISOR_RESTART_WINDOW_MAX
        // + Duration::from_millis(1)), .. }` must surface the
        // `RestartWindowExceedsCap` refusal exactly (with the offending
        // duration carried verbatim from the accessor return), and a
        // `SupervisorSpec { restart_window: Some(Duration::from_millis(1)),
        // .. }` (the lower boundary of the accept-set) plus a
        // `SupervisorSpec { restart_window: Some(SUPERVISOR_RESTART_WINDOW_MAX),
        // .. }` (the upper boundary) must pass validate. The six together
        // jointly pin the accessor + validate-gate composition: any future
        // silent detour that had the accessor return a fresh `None` on any
        // `Some` arm (a `.restart_window().filter(|w| !w.is_zero())`
        // collapse) would silently absorb the `RestartWindowZero` refusal
        // at the accessor boundary and the validate gate would accept a
        // struct-literal `SupervisorSpec { restart_window:
        // Some(Duration::ZERO), .. }` — the composition pin catches that
        // at caixa-core build time.
        //
        // Peer of the sibling M2 [`crate::LimitsSpec::wall_clock`]
        // (8cb717b) validate-arm-route pin on the per-`:limits :wall-clock`
        // axis and the peer M3 [`crate::MeshPolicy::timeout`] (7073d0f)
        // accessor-composition pin on the per-`:politicas :timeout` axis —
        // same "the validate / shape-gate predicate must route through
        // the substrate-primitive typed dispatch" discipline extended
        // onto the peer M2 supervisor-slot optional-`Duration` axis.
        let child = ChildSpec {
            caixa: "worker".into(),
            versao: "^0.1".into(),
            restart: RestartPolicy::Permanent,
        };
        // None arm — must not surface any :restart-window-shaped refusal;
        // the `if let Some(_)` bracket returns early on `None` structurally.
        let s = SupervisorSpec {
            restart_window: None,
            children: vec![child.clone()],
            ..SupervisorSpec::default()
        };
        assert!(
            s.validate().is_ok(),
            "validate must accept restart_window: None (the never-reset \
             sentinel) — the `if let Some(_)` bracket returns early on \
             the None arm and the accessor must agree",
        );
        // Zero-floor arm.
        let s = SupervisorSpec {
            restart_window: Some(Duration::ZERO),
            children: vec![child.clone()],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::RestartWindowZero,
            "validate must reject restart_window == Some(Duration::ZERO) \
             with RestartWindowZero — the accessor and the validate gate \
             must route through the same substrate-primitive typed \
             dispatch on the zero-floor arm",
        );
        // Non-canonical (sub-ms) arm — the surfaced `window:` field must
        // byte-equal the accessor's return so a future rebrand on the
        // accessor lands in the diagnostic without a coordinated rewrite.
        let sub_ms = Duration::from_micros(1500);
        let s = SupervisorSpec {
            restart_window: Some(sub_ms),
            children: vec![child.clone()],
            ..SupervisorSpec::default()
        };
        match s.validate().unwrap_err() {
            SupervisorError::RestartWindowNotCanonical { window } => {
                assert_eq!(
                    Some(window),
                    s.restart_window(),
                    "RestartWindowNotCanonical.window must byte-equal \
                     SupervisorSpec::restart_window().unwrap() — the \
                     non-canonical-arm refusal reads through the lifted \
                     accessor",
                );
                assert_eq!(
                    window, sub_ms,
                    "RestartWindowNotCanonical.window must carry the \
                     author-declared :supervisor :restart-window value \
                     verbatim (got {window:?}, expected {sub_ms:?})",
                );
            }
            other => panic!("expected RestartWindowNotCanonical, got {other:?}"),
        }
        // Cap arm — the surfaced `window:` field must byte-equal the
        // accessor's return.
        let over_cap = SUPERVISOR_RESTART_WINDOW_MAX + Duration::from_millis(1);
        let s = SupervisorSpec {
            restart_window: Some(over_cap),
            children: vec![child.clone()],
            ..SupervisorSpec::default()
        };
        match s.validate().unwrap_err() {
            SupervisorError::RestartWindowExceedsCap { window } => {
                assert_eq!(
                    Some(window),
                    s.restart_window(),
                    "RestartWindowExceedsCap.window must byte-equal \
                     SupervisorSpec::restart_window().unwrap() — the \
                     cap-arm refusal reads through the lifted accessor",
                );
                assert_eq!(
                    window, over_cap,
                    "RestartWindowExceedsCap.window must carry the \
                     author-declared :supervisor :restart-window value \
                     verbatim (got {window:?}, expected {over_cap:?})",
                );
            }
            other => panic!("expected RestartWindowExceedsCap, got {other:?}"),
        }
        // Lower + upper accept-set boundaries.
        for restart_window in [Duration::from_millis(1), SUPERVISOR_RESTART_WINDOW_MAX] {
            let s = SupervisorSpec {
                restart_window: Some(restart_window),
                children: vec![child.clone()],
                ..SupervisorSpec::default()
            };
            assert!(
                s.validate().is_ok(),
                "validate must accept restart_window == Some({restart_window:?}) \
                 (an accept-set boundary of \
                 1ms..=SUPERVISOR_RESTART_WINDOW_MAX)",
            );
        }
    }

    #[test]
    fn supervisor_spec_restart_window_projects_option_duration_by_copy() {
        // The by-copy pin: [`SupervisorSpec::restart_window`] returns
        // `Option<Duration>` by copy — `Duration` is `Copy` (so
        // `Option<Duration>` is `Copy`) and the accessor must return by
        // value, not by reference. Peer of the sibling M2
        // [`crate::LimitsSpec::wall_clock`] (8cb717b) by-copy pin on the
        // per-`:limits :wall-clock` axis and the sibling M3
        // [`crate::MeshPolicy::timeout`] (7073d0f) by-copy pin on the
        // per-`:politicas :timeout` axis, extended onto the peer M2
        // supervisor-slot `Option<Duration>` copy-invariant shape — the
        // accessor's returned `Option<Duration>` must outlive `&self`
        // (multiple calls must return equal values from a dropped-`&self`
        // copy, since the returned Option carries no borrow), and calling
        // the accessor twice on the same SupervisorSpec must yield the
        // same `Option<Duration>` verbatim (idempotent, no side effects
        // on `&self`).
        //
        // Pins against a future silent detour that returned
        // `Option<&Duration>` (which would type-check but silently break
        // every downstream caller — the future wasm-operator's
        // per-supervisor restart-intensity counter consumes `Duration` by
        // value and `&Duration` would fold to a detached copy at the call
        // site), an accidental `Option::as_ref()` projection
        // (`self.restart_window.as_ref()` would also type-check but
        // return `Option<&Duration>`), or a one-arm-only accessor that
        // reads `Some(*w)` in the Some arm but reads a fresh
        // `Default::default()` (which would collapse to `Duration::ZERO`,
        // not `None`) in the None arm — a footgun the
        // [`SupervisorError::RestartWindowZero`] validate arm explicitly
        // closes since Erlang/OTP's `MaxIntensity / Period` invariant
        // requires `Period > 0` and `None` structurally expresses "never
        // reset" instead.
        for restart_window in [
            None,
            Some(Duration::from_millis(1)),
            Some(Duration::from_secs(60)),
            Some(SUPERVISOR_RESTART_WINDOW_MAX),
        ] {
            let s = SupervisorSpec {
                restart_window,
                ..SupervisorSpec::default()
            };
            let first = s.restart_window();
            let second = s.restart_window();
            assert_eq!(
                first, second,
                "SupervisorSpec::restart_window must be idempotent — two \
                 successive calls on the same &self must return the \
                 same Option<Duration>",
            );
            assert_eq!(
                first, restart_window,
                "SupervisorSpec::restart_window must return :supervisor \
                 :restart-window verbatim by copy — got {first:?}, \
                 expected {restart_window:?}",
            );
        }
    }

    // ── per-`:supervisor` `:children` typed-accessor coherence pins ─────────
    //
    // The [`SupervisorSpec::children`] accessor lift is the seed of the
    // slice-return (`&[T]`) accessor discipline on the substrate — the four
    // peer `Vec`-carry axes ([`crate::Placement::clusters`],
    // [`crate::AplicacaoSpec::membros`], [`crate::AplicacaoSpec::contratos`],
    // [`crate::UpgradeFromEntry::instructions`]) still key off the raw field
    // access at the time of this seed, and inherit this pin family's
    // discipline as future compounding runs migrate their consumers. The
    // three pins below cover (1) the accessor's byte-equal projection
    // against the raw field access across the empty / singleton / cohort
    // fixtures the [`SupervisorSpec::validate`] partition-dispatch fans
    // between, (2) the [`SupervisorSpec::validate`] `SimpleOneForOne ↔
    // non-SimpleOneForOne` partition dispatch's paired `.is_empty()`
    // consumer routing through the accessor on both arms, and (3) the
    // per-child validate loop's traversal reading the same slice-view the
    // accessor projects. Peer of the sibling M2
    // [`validate_reads_through_lifted_estrategia_accessor`] (eafb619)
    // two-consumer coherence pin on the per-`:supervisor`
    // sibling-restart-strategy `Copy`-composite-enum scalar axis, extended
    // onto the per-`:supervisor` static-child-list `Vec`-carry axis.

    #[test]
    fn supervisor_spec_children_returns_children_slice_byte_equal_across_permutations() {
        // The canonical per-`:supervisor` static-child-list scalar-shape
        // pin: [`SupervisorSpec::children`] must return the `:supervisor
        // :children` typed `Vec<ChildSpec>` verbatim as a `&[ChildSpec]`
        // slice-view over the same backing buffer the raw
        // `self.children.as_slice()` field access borrows from, byte-
        // equal across every representative fixture in the accept-set —
        // the empty slice (the `SimpleOneForOne`-arm sentinel),
        // the singleton slice (the minimal non-`SimpleOneForOne` shape),
        // and a two-child cohort (a peer non-`SimpleOneForOne` shape
        // with the peer three restart-policy variants in play).
        //
        // Pins against a future silent detour that returned
        // `&Vec<ChildSpec>` (which would type-check but leak the
        // storage-side `Vec`'s grow/push/reserve surface no consumer of
        // the typed view reaches for), a fresh-allocated
        // `Vec<ChildSpec>` copy (which would type-check via a coercion
        // but silently break every downstream caller that relied on the
        // slice sharing the backing buffer's identity), or an
        // out-of-order or length-drifted projection (which would silently
        // split the per-child validate loop's traversal input from the
        // paired partition-dispatch `.is_empty()` probe's input).
        //
        // Peer of the sibling
        // `supervisor_spec_estrategia_returns_estrategia_verbatim_across_permutations`
        // (eafb619) `Copy`-composite-enum byte-equal pin on the
        // per-`:supervisor` sibling-restart-strategy axis, extended onto
        // the per-`:supervisor` static-child-list `Vec`-carry axis.
        let fixtures: Vec<Vec<ChildSpec>> = vec![
            Vec::new(),
            vec![child("worker", "^0.1", RestartPolicy::Permanent)],
            vec![
                child("worker", "^0.1", RestartPolicy::Permanent),
                child("cache-server", "^0.1", RestartPolicy::Transient),
            ],
            vec![
                child("worker", "^0.1", RestartPolicy::Permanent),
                child("cache-server", "^0.1", RestartPolicy::Transient),
                child("scratch-job", "^0.1", RestartPolicy::Temporary),
            ],
        ];
        for children in fixtures {
            let s = SupervisorSpec {
                children: children.clone(),
                ..SupervisorSpec::default()
            };
            assert_eq!(
                s.children(),
                children.as_slice(),
                "SupervisorSpec::children must return :supervisor \
                 :children verbatim (got {:?}, expected {:?})",
                s.children(),
                children.as_slice(),
            );
            assert_eq!(
                s.children(),
                s.children.as_slice(),
                "SupervisorSpec::children accessor and \
                 .children.as_slice() field access must byte-equal — \
                 the accessor is the substrate-primitive typed \
                 dispatch every downstream static-child-list consumer \
                 must route through",
            );
            assert_eq!(
                s.children().len(),
                s.children.len(),
                "SupervisorSpec::children().len() must byte-equal \
                 self.children.len() — a length-drift would silently \
                 split the paired partition-dispatch `.is_empty()` \
                 probe input from the per-child validate loop's \
                 traversal input",
            );
        }
    }

    #[test]
    fn validate_reads_through_lifted_children_accessor() {
        // Three-consumer coherence pin: the [`SupervisorSpec::validate`]
        // `SimpleOneForOne`-arm `!self.children().is_empty()` refusal
        // probe (which must trip [`SupervisorError::SimpleOneForOneWithStaticChildren`]
        // when the accessor projects a non-empty slice under a
        // `SimpleOneForOne` estrategia), the peer non-`SimpleOneForOne`-arm
        // `self.children().is_empty()` refusal probe (which must trip
        // [`SupervisorError::NoChildren`] when the accessor projects the
        // empty slice under any peer estrategia), and the per-child
        // validate loop's `for child in self.children()` traversal
        // (which must reach every entry in the same order the accessor
        // projects) must all key off the lifted accessor, so any future
        // rebrand on the typed slot's reader shape lands at exactly one
        // place. Pins the three-site coherence by exercising each
        // production consumer end-to-end: (1) the
        // `SimpleOneForOneWithStaticChildren` refusal under a non-empty
        // slice + `SimpleOneForOne` estrategia, (2) the `NoChildren`
        // refusal under the empty slice + non-`SimpleOneForOne`
        // estrategia across every peer variant, and (3) the per-child
        // duplicate-detection surface fires on the second entry of a
        // two-child cohort that shares a `:caixa` name (which requires
        // the loop to reach both entries — a first-entry-only projection
        // would silently pass since the dedup HashSet has room for the
        // first insert).
        //
        // Peer of the sibling M2
        // [`validate_reads_through_lifted_estrategia_accessor`] (eafb619)
        // two-consumer coherence pin on the per-`:supervisor`
        // sibling-restart-strategy axis, extended onto the
        // per-`:supervisor` static-child-list `Vec`-carry axis.

        // (1) `SimpleOneForOne`-arm probe: a non-empty slice under a
        // `SimpleOneForOne` estrategia must trip
        // `SimpleOneForOneWithStaticChildren`.
        let s = SupervisorSpec {
            estrategia: RestartStrategy::SimpleOneForOne,
            children: vec![child("worker", "^0.1", RestartPolicy::Permanent)],
            ..SupervisorSpec::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            SupervisorError::SimpleOneForOneWithStaticChildren,
            "SimpleOneForOne + non-empty children must trip \
             SimpleOneForOneWithStaticChildren — the accessor projects \
             a non-empty slice, and the SimpleOneForOne-arm refusal \
             probe reads through the lifted accessor",
        );
        assert!(
            !s.children().is_empty(),
            "the SimpleOneForOne-arm refusal input must be a non-empty \
             slice per the accessor's projection",
        );

        // (2) Peer non-`SimpleOneForOne`-arm probe: the empty slice
        // under any peer estrategia must trip `NoChildren`.
        for estrategia in [
            RestartStrategy::OneForOne,
            RestartStrategy::OneForAll,
            RestartStrategy::RestForOne,
        ] {
            let s = SupervisorSpec {
                estrategia,
                children: Vec::new(),
                ..SupervisorSpec::default()
            };
            match s.validate().unwrap_err() {
                SupervisorError::NoChildren { estrategia: e } => {
                    assert_eq!(
                        e, estrategia,
                        "NoChildren.estrategia must carry the author-\
                         declared :supervisor :estrategia variant \
                         verbatim (got {e:?}, expected {estrategia:?})",
                    );
                }
                other => panic!(
                    "expected NoChildren, got {other:?} for \
                     estrategia={estrategia:?}"
                ),
            }
            assert!(
                s.children().is_empty(),
                "the non-SimpleOneForOne-arm refusal input must be the \
                 empty slice per the accessor's projection",
            );
        }

        // (3) Per-child validate loop: a two-child cohort that shares a
        // `:caixa` name must trip `DuplicateChildCaixa` — the loop must
        // reach both entries through the accessor.
        let s = SupervisorSpec {
            estrategia: RestartStrategy::OneForOne,
            children: vec![
                child("worker", "^0.1", RestartPolicy::Permanent),
                child("worker", "^0.2", RestartPolicy::Transient),
            ],
            ..SupervisorSpec::default()
        };
        match s.validate().unwrap_err() {
            SupervisorError::DuplicateChildCaixa { caixa } => {
                assert_eq!(
                    caixa, "worker",
                    "DuplicateChildCaixa.caixa must carry the shared \
                     child `:caixa` name verbatim",
                );
            }
            other => panic!("expected DuplicateChildCaixa, got {other:?}"),
        }
        assert_eq!(
            s.children().len(),
            2,
            "the per-child validate loop's traversal input must be a \
             two-element slice per the accessor's projection",
        );
    }
}
