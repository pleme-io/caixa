//! caixa-mesh — typed renderer that emits cluster mesh primitives
//! from an `:kind Aplicacao` caixa.
//!
//! See `theory/MESH-COMPOSITION.md` for the design frame: a typed
//! Aplicacao composes Servicos into a graph with WIT-typed contracts,
//! mesh policies, and explicit placement. caixa-mesh is the renderer
//! that turns that typed graph into the cluster-side primitives:
//!
//!   1. **programs.yaml fan-out** — one entry per `:membros`,
//!      consumed by lareira-fleet-programs (V0; this crate)
//!   2. **Cilium NetworkPolicy** — one per distinct `:contratos`
//!      `(:de, :para)` pair, identity-based L7 allow-list (M3.x next)
//!   3. **Gateway + HTTPRoute** — one per `:entrada`, K8s Gateway API
//!      external ingress (M3.x next)
//!
//! Same `caixa-<target>` naming convention as [`caixa_helm`] +
//! [`caixa_flux`]: a typed renderer that takes a typed Caixa and emits
//! the canonical source for `<target>`.
//!
//! V0 contract:
//!
//! ```rust,ignore
//! use caixa_core::Caixa;
//! use caixa_mesh::programs_for_aplicacao;
//!
//! let aplicacao: Caixa = Caixa::from_lisp(src)?;
//! let entries: Vec<serde_yaml::Value> = programs_for_aplicacao(&aplicacao)?;
//! // → one entry per :membros, suitable for fan-out into the
//! //   cluster's lareira-fleet-programs HelmRelease.
//! ```

#![allow(clippy::module_name_repetitions)]

use std::collections::BTreeMap;

use caixa_core::{
    Caixa, CaixaKind, FLEET_PROGRAMS_KEY_APLICACAO, FLEET_PROGRAMS_KEY_NAME,
    FLEET_PROGRAMS_KEY_VERSAO, GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME,
    GATEWAY_API_DEFAULT_HTTP_LISTENER_PORT, GATEWAY_API_KEY_NAME, LABEL_APLICACAO, LABEL_CONTRATO,
    M3_KEY_PLACEMENT, MappingExt, SequenceExt, WitContract, aplicacao::AplicacaoSpec,
    kube_resource_skeleton, label_selector, pleme_program_in_aplicacao_selector,
    pleme_program_selector, single_field_overlay,
};
use thiserror::Error;

/// Errors caixa-mesh can raise.
#[derive(Debug, Error)]
pub enum Error {
    /// The caixa's `:kind` doesn't match what `caixa-mesh` targets
    /// (this renderer only emits the per-Aplicacao mesh artifact set
    /// — programs.yaml fan-out + Cilium NetworkPolicies + Gateway/
    /// HTTPRoute — for `:kind Aplicacao`). Lifted from a prior
    /// `NotAnAplicacao(CaixaKind)` arm to wrap [`caixa_core::KindMismatch`]
    /// so the diagnostic names the offending caixa's `:nome` (not
    /// just its kind), shared verbatim with `caixa-helm` and
    /// `caixa-flux`.
    #[error("{0}")]
    NotAnAplicacao(#[from] caixa_core::KindMismatch),
    #[error("aplicacao typed shape violation: {0}")]
    InvalidAplicacao(#[from] caixa_core::AplicacaoError),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

/// Render one `programs.yaml` entry per `:membros` in the Aplicacao.
///
/// Each entry is a typed [`serde_yaml::Value::Mapping`] suitable for
/// upserting into a `lareira-fleet-programs` HelmRelease's
/// `spec.values.programs[]` (the same shape `caixa-flux::programs_yaml_entry`
/// emits for individual Servico caixas).
///
/// V0 caveats:
///   - The member entry has only `name` + `versao` + a passthrough
///     `aplicacao` annotation linking it back to the parent Aplicacao.
///     Resolving each member's full ComputeUnit (module.source,
///     trigger, capabilities) is the resolver's job at deploy time —
///     the resolver fetches each member's caixa.lisp from git, calls
///     `caixa-flux::programs_yaml_entry` on it, then merges with the
///     Aplicacao-level `politicas` overrides.
///   - Mesh-level concerns (Cilium NetworkPolicy, Gateway) are
///     deferred to follow-up rendering verbs in this crate (M3.x).
pub fn programs_for_aplicacao(caixa: &Caixa) -> Result<Vec<serde_yaml::Value>, Error> {
    // Route the entry gate through the canonical [`typed_view`] entry
    // point so every per-Aplicacao renderer in this crate
    // (`programs_for_aplicacao`, `cilium_network_policies`,
    // `gateway_routes`) shares one `require_kind + aplicacao_view +
    // AplicacaoSpec::validate` cascade. Prior to this lift
    // `programs_for_aplicacao` re-inlined the three-arm gate while its
    // two sibling renderers reached for `typed_view`; the drift risk
    // was structural — a future entry-gate widening (e.g. a per-
    // Aplicacao capability-audit prelude, a `:placement`-aware
    // pre-render normalization, the M4 CR materializer's admission-
    // webhook floor) would have to be threaded through both call sites
    // in lockstep or one renderer would silently diverge from the
    // other on which shapes it accepted at emit time. Peer with the
    // sibling `typed_view` consumers on the same one-entry-gate
    // discipline (a4ba8ec `require_v0_servico_shape` lifted the
    // `require_kind(Servico) + require_single_servico` compound entry-
    // gate across `caixa-helm` + `caixa-flux`; this lift closes the
    // matching two-caller drift surface on `caixa-mesh`'s per-
    // Aplicacao entry-gate).
    let spec = typed_view(caixa)?;

    // `:placement` overlay — surfaces the typed Aplicacao-level
    // distribution strategy + cluster list (validated upstream by
    // [`AplicacaoSpec::validate_placement`]: non-empty `:clusters`,
    // unique entries, `Sharded` carries `:shard-key`) onto every
    // emitted programs.yaml entry under the canonical
    // [`M3_KEY_PLACEMENT`] key. Until this wiring landed the typed
    // `:placement` slot was inert past validate() — `AplicacaoSpec`
    // refused empty/duplicate clusters, missing shard-keys, and
    // empty affinity hints (the c7c7799 + 4bb3f3d + 2d71a9a +
    // c4213a4 typed-shape lifts), but the rendered programs.yaml
    // entries carried only `name + versao + aplicacao`, so the
    // lareira-fleet-programs aggregator and the future M4
    // cross-cluster fanout / `app-operator` reconciler had no way
    // to scope each entry by its parent Aplicacao's distribution
    // strategy.
    //
    // Wiring it through turns MESH-COMPOSITION §III.4 ("the
    // application graph is a compile-time typed value … rendered
    // through to whatever runtime layer makes sense") + §V
    // ("cross-cluster federation: `:placement :replicated
    // :clusters (\"rio\" \"mar\")` deploys the Aplicacao to every
    // named cluster") from a typed-author-side promise into a
    // typed-renderer-side artifact: each cluster's local
    // lareira-fleet-programs HelmRelease can filter
    // `programs[]` by `placement.clusters.contains(<self>)`,
    // dispatch on `placement.estrategia`, and (for `Sharded`)
    // route by `placement.shardKey`. Same trajectory as the
    // 5f477a6 / 23b7f00 / 878bf81 `:politicas` axis overlays:
    // typed slot → cluster artifact in one wiring step, no new
    // primitive needed.
    //
    // The serialized fragment uses the [`Placement`] struct's
    // serde shape verbatim — `estrategia` + `clusters` always
    // present (validated non-empty), `affinity` + `shardKey`
    // present iff `Some` (skip_serializing_if). One entry per
    // member carries the same placement block; redundant in
    // bytes, but each programs.yaml entry is self-describing for
    // the aggregator (which has no Aplicacao-level context),
    // mirroring the existing `aplicacao:` annotation's per-entry
    // emission.
    // Route the per-Aplicacao `:placement` composite-serialization seed
    // through the lifted [`caixa_core::AplicacaoSpec::placement`] outer
    // accessor rather than the raw `&spec.placement` field access — every
    // downstream per-`programs[]` entry's placement-block annotation now
    // keys off the substrate-primitive typed dispatch on the outer
    // composition altitude, sibling to the paired peer
    // [`caixa_core::AplicacaoSpec::politicas`] (534dc21) outer-composite-
    // reference accessor the per-CNP mTLS-overlay + HTTPRoute
    // timeout/retry-overlay emitters below already route through. The
    // accessor's `&Placement` return borrows the same backing composite
    // the raw field access borrows from, so the serialization byte-string
    // is byte-for-byte identical (validated by the peer
    // `aplicacao_spec_placement_returns_placement_ref_byte_equal_across_permutations`
    // reference-identity pin).
    let placement_value = serde_yaml::to_value(spec.placement())?;

    let mut out = Vec::with_capacity(spec.membros().len());
    for m in spec.membros() {
        let mut entry = serde_yaml::Mapping::new();
        // Route the per-`:membros` entry-`name:` byte-string through the
        // typed [`Membro::nome`] accessor rather than the raw `.caixa`
        // field access — the last un-lifted `.caixa.clone()` copy of the
        // read path on the per-`:membros` member-caixa `:nome` axis, and
        // the sibling `String`-carry site to the five `&str`-read sites
        // the peer 4a32abf lift already routed through the accessor.
        // Prior to this lift the emit-side `String`-carry path was the
        // solitary consumer bypassing the typed dispatch, so a future
        // extension of the `:membros :caixa` axis to a richer author
        // surface (a per-cluster alias table, an M4 namespace-qualified
        // rewrite, a `:membros :nome-suffix` overlay) that lands on the
        // accessor would silently disagree with the emitted programs.yaml
        // `name:` — the drift-detection pins below key off this equality
        // to catch the regression at caixa-mesh build time.
        entry.insert_string(FLEET_PROGRAMS_KEY_NAME, m.nome().to_string());
        // Per-`:membros` version-constraint annotation — flows the
        // Membro's `:versao` (the M3 Aplicacao's per-member semver /
        // range constraint) through the canonical
        // [`caixa_core::FLEET_PROGRAMS_KEY_VERSAO`] axis-key the
        // substrate operator's per-`:membros` resolver reads to fetch
        // each member's caixa.lisp release. See the const's doc-comment
        // for the fourth-of-four per-entry fleet-programs values-schema
        // axis-key single-sourcing arc — with this lift landed every
        // per-entry axis (name + versao + aplicacao + placement) lives
        // in exactly one `&'static str` across the emitter here + every
        // downstream aggregator/resolver call site. Routes through the
        // typed [`Membro::versao_requirement`] accessor (a40b0e3) rather
        // than the raw `.versao` field — same converging-`String`-carry-
        // path discipline the sibling [`FLEET_PROGRAMS_KEY_NAME`] emit
        // above applies on the peer per-`:membros` member-caixa `:nome`
        // axis.
        entry.insert_string(
            FLEET_PROGRAMS_KEY_VERSAO,
            m.versao_requirement().to_string(),
        );
        // Annotate with the parent Aplicacao's nome so the operator
        // knows which graph this member belongs to. Consumes the
        // lifted [`caixa_core::FLEET_PROGRAMS_KEY_APLICACAO`] axis-key
        // — see its doc-comment for why the per-entry parent-graph-
        // annotation-key axis lives in one canonical const across
        // the emitter here + the readback probe below.
        entry.insert_string(FLEET_PROGRAMS_KEY_APLICACAO, caixa.nome().to_string());
        // M3 `:placement` overlay — see the per-call rationale
        // above. Cloned per entry so each programs.yaml row is
        // self-describing for downstream filters that have no
        // Aplicacao-level context.
        entry.insert_str_key(M3_KEY_PLACEMENT, placement_value.clone());
        out.push_mapping(entry);
    }
    Ok(out)
}

/// Compose a single typed view of the entire Aplicacao for downstream
/// renderers (Cilium, Gateway, observability). Convenience wrapper that
/// routes the compound `require_kind + aplicacao_view + validate`
/// cascade through the canonical substrate primitive
/// [`caixa_core::require_aplicacao_view`], sibling to the
/// per-Servico [`caixa_core::require_v0_servico_shape`] compound entry
/// gate every `caixa-helm` / `caixa-flux` renderer already routes
/// through. The wrapper stays for turbofish elision at this crate's
/// three call sites (`programs_for_aplicacao` /
/// `cilium_network_policies` / `gateway_routes`), matching the shape
/// the sibling `caixa-flux` / `caixa-helm` renderers read the compound
/// V0-Servico gate as, and every future per-Aplicacao consumer
/// (`caixa-tatara`'s spec-consuming validate arm when it lands, the
/// deferred `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's
/// admission webhook) gets the compound three-arm gate for free with
/// one call rather than re-inlining the cascade — same discipline the
/// peer [`caixa_core::require_v0_servico_shape`] lift closed on the
/// per-Servico renderer axis.
pub fn typed_view(caixa: &Caixa) -> Result<AplicacaoSpec, Error> {
    caixa_core::require_aplicacao_view::<Error>(caixa)
}

/// Default namespace for emitted cluster objects when the Aplicacao
/// doesn't pin one. Re-export of the canonical
/// [`caixa_core::DEFAULT_NAMESPACE`] so the namespace string lives in
/// exactly one place across every renderer — caixa-mesh's programs
/// fan-out / CiliumNetworkPolicy / Gateway / HTTPRoute emitters and
/// caixa-flux's programs.yaml / GitRepository / HelmRelease /
/// Kustomization emitters now consult the same `&'static str`, so a
/// future per-cluster-namespace rebrand is a one-line edit on the
/// canonical [`caixa_core::DEFAULT_NAMESPACE`] declaration, not a
/// coordinated rewrite across this crate, caixa-flux, and every
/// future per-target renderer the substrate adds. The prior local
/// `pub const` declaration explicitly acknowledged the duplication
/// ("Mirrors `caixa_flux::DEFAULT_NAMESPACE`"); this re-export
/// closes the drift footgun structurally — a future rebrand on one
/// side without a coordinated edit on the other would otherwise have
/// silently emitted Servicos into one namespace and their Aplicacao's
/// NetworkPolicies / Gateways / HTTPRoutes into a drifted one, with
/// the apply-time symptom (CiliumNetworkPolicy `endpointSelector`
/// matches no pods, every L7 contrato flow silently drops) far from
/// the rebrand commit's source.
pub use caixa_core::DEFAULT_NAMESPACE;

/// Canonical K8s Gateway API CRD `apiVersion` every `gateway_routes`-
/// emitted `Gateway` / `HTTPRoute` document declares. Re-export of the
/// canonical [`caixa_core::GATEWAY_API_API_VERSION`] so the
/// Gateway-API-conformant CRD-group/version string lives in exactly
/// one place across every caixa renderer — caixa-mesh's
/// `gateway_routes` Gateway + HTTPRoute emitters (the two production-
/// code sites the prior inline literal sat at,
/// caixa-mesh/src/lib.rs:455, 496) and every future per-edge
/// `TCPRoute` / `TLSRoute` / `GRPCRoute` emitter the M3.x absorption-
/// roadmap acknowledges now consult the same `&'static str`, so a
/// future K8s Gateway API GA promotion (the upstream SIG-Network
/// roadmap names per-CRD-group / per-version migration once the v1
/// GA branch matures) is a one-line edit on the canonical
/// [`caixa_core::GATEWAY_API_API_VERSION`] declaration, not a
/// coordinated rewrite across this crate's two `kube_resource_skeleton`
/// call sites + every future per-target renderer the substrate adds.
/// The prior inline literals would have let a Gateway-API GA bump on
/// one axis without a coordinated edit on the other silently emit a
/// `Gateway` / `HTTPRoute` pair pointing at distinct CRD versions —
/// apply-side: the `Gateway` and `HTTPRoute` land in two distinct
/// apiserver-side CRD registrations, the per-route attached-policy
/// resolution pipeline never binds, every external `:entrada` flow
/// drops at the gateway with no field naming the version-drift root
/// cause. Peer to the [`DEFAULT_NAMESPACE`] re-export on the sibling
/// canonical-load-bearing-string axis — extends the discipline onto
/// the canonical-K8s-Gateway-API-CRD-axis surface.
pub use caixa_core::GATEWAY_API_API_VERSION;

/// Canonical Cilium CRD `apiVersion` every `cilium_network_policies`-
/// emitted `CiliumNetworkPolicy` document declares. Re-export of the
/// canonical [`caixa_core::CILIUM_API_VERSION`] so the Cilium-CRD-
/// group/version string lives in exactly one place across every caixa
/// renderer — caixa-mesh's `cilium_network_policies` per-`(:de, :para)`
/// CiliumNetworkPolicy emitter (the single production-code site the
/// prior inline literal sat at, caixa-mesh/src/lib.rs:326) and every
/// future per-policy `CiliumClusterwideNetworkPolicy` /
/// `CiliumLocalRedirectPolicy` emitter the M3.x absorption roadmap
/// acknowledges now consult the same `&'static str`, so a future
/// Cilium-CRD-group/version promotion (the upstream Cilium roadmap
/// names per-CRD-group / per-version migration once the
/// `cilium.io/v3` branch lands) is a one-line edit on the canonical
/// [`caixa_core::CILIUM_API_VERSION`] declaration, not a coordinated
/// rewrite across this crate's `kube_resource_skeleton` call site +
/// every future per-target renderer the substrate adds. The prior
/// inline literal would have let a Cilium-CRD bump on one axis without
/// a coordinated edit on the matching in-file
/// `cilium_policy_carries_canonical_kube_skeleton` test fixture pin
/// (caixa-mesh/src/lib.rs:1560) silently emit a `CiliumNetworkPolicy`
/// whose top-level apiVersion drifts off the lifted-test-fixture pin —
/// apply-side: the policy lands in a stale apiserver-side CRD-version
/// registration the Cilium operator no longer watches, every
/// `(:de, :para)` intra-mesh L4 contract drops at the eBPF data plane
/// with no field naming the version-drift root cause. Peer to the
/// [`GATEWAY_API_API_VERSION`] re-export on the sibling
/// canonical-K8s-Gateway-API-CRD-axis — extends the discipline onto
/// the canonical-Cilium-CRD-axis surface.
pub use caixa_core::CILIUM_API_VERSION;

/// Canonical Cilium CRD `kind` discriminator every
/// `cilium_network_policies`-emitted `CiliumNetworkPolicy` document
/// declares at its top-level [`caixa_core::KUBE_KEY_KIND`] axis.
/// Re-export of the canonical [`caixa_core::CILIUM_KIND_NETWORK_POLICY`]
/// so the Cilium-operator-side CRD `kind` discriminator string lives in
/// exactly one place across every caixa renderer — caixa-mesh's
/// `cilium_network_policies` per-`(:de, :para)` CiliumNetworkPolicy
/// emitter (the single production-code site the prior inline
/// `"CiliumNetworkPolicy"` literal sat at,
/// caixa-mesh/src/lib.rs:382 — the `kube_resource_skeleton` kind
/// argument) and every future per-Cilium-side renderer the M3.x
/// absorption roadmap acknowledges now consult the same `&'static
/// str`, so a future Cilium-CRD rebrand (e.g. an upstream rename to
/// `CiliumNetworkPolicyV2`) is a one-line edit on the canonical
/// [`caixa_core::CILIUM_KIND_NETWORK_POLICY`] declaration, not a
/// coordinated rewrite across this crate's `kube_resource_skeleton`
/// call site + every future per-target renderer the substrate adds.
/// The prior inline literal would have let a Cilium-CRD bump on the
/// kind axis without a coordinated edit on the matching in-file
/// `cilium_policy_carries_canonical_kube_skeleton` test fixture pin
/// silently emit a `CiliumNetworkPolicy` whose top-level kind drifts
/// off the lifted-test-fixture pin — apply-side: the policy lands
/// outside the Cilium-operator-side CRD registration, every
/// `(:de, :para)` intra-mesh L4/L7 contract drops at the eBPF data
/// plane with no field naming the kind-drift root cause. Peer to the
/// [`CILIUM_API_VERSION`] re-export on the sibling
/// canonical-Cilium-CRD-apiVersion-axis — extends the discipline from
/// the apiVersion half of the `(apiVersion, kind)` CRD-lookup tuple
/// onto the kind half, completing the per-Cilium-CRD
/// kind+apiVersion re-export pair this crate's `cilium_network_policies`
/// renderer's eBPF data-plane contract rests on.
pub use caixa_core::CILIUM_KIND_NETWORK_POLICY;

/// Canonical Cilium `CiliumNetworkPolicy` per-ingress-rule port-set
/// container-axis key every `cilium_network_policies`-emitted CNP
/// document mounts its per-ingress-rule `[{ports: […], rules: {…}}]`
/// list under (`spec.ingress[].toPorts[]`). Re-export of the canonical
/// [`caixa_core::CILIUM_KEY_TO_PORTS`] so the Cilium-operator-side
/// per-CNP L4/L7-dispatch container-key string lives in exactly one
/// place across every caixa renderer — caixa-mesh's
/// `cilium_network_policies` per-`(:de, :para)` `CiliumNetworkPolicy`
/// emitter (the `ingress_rule.insert("toPorts", …)` call the prior
/// inline `"toPorts"` literal sat at) and every future per-Cilium-side
/// renderer the M3.x absorption roadmap acknowledges now consult the
/// same `&'static str`, so a future Cilium-CRD rebrand on the port-set
/// container axis (unlikely on the CRD's stable `cilium.io/v2` slot,
/// but the coordination point the prior [`KUBE_KEY_RULES`] +
/// [`CILIUM_KIND_NETWORK_POLICY`] + [`CILIUM_API_VERSION`] re-exports
/// anchor on the sibling per-CNP-dispatch-axis surface) lands in one
/// place. The prior inline literal split across the one production
/// emitter and six test-fixture navigation sites (2 CNP presence /
/// absence pins, 1 fan-in-per-pair invariant pin, 1 mTLS-overlay
/// nesting pin — via the pair of `contains_key` + `.get` navigations,
/// 1 L4-fallback port pin) would have let a Cilium-CRD port-set-
/// container rebrand or a per-emitter typo (`"toport"` / `"toPort"` /
/// `"targetPorts"`) at any one site silently emit a per-ingress-rule
/// entry whose port-set container the Cilium CRD schema validator
/// drops as unknown; every intra-mesh `:contratos` flow the affected
/// CNP was authored to allow drops at the eBPF data plane's default-
/// deny gate with no field naming the container-drift root cause, and
/// on the test-fixture side the drift silently masks the emission-side
/// pin (`.get("toPorts")` returns `None` under both the drifted
/// emitter and the drifted probe — the `cilium_pubsub_contracts_skip_\
/// l7_rules` absence pin's downstream `to_ports.get("rules").is_none()`
/// assertion succeeds vacuously because `to_ports` is itself `None`).
/// Peer to the [`caixa_core::KUBE_KEY_RULES`] re-export on the sibling
/// canonical-per-CNP-dispatch-axis surface — completes the per-CNP
/// L4/L7-dispatch-container `(toPorts, rules)` re-export pair this
/// crate's `cilium_network_policies` renderer's eBPF data-plane
/// contract rests on.
pub use caixa_core::CILIUM_KEY_TO_PORTS;

/// Canonical Cilium `CiliumNetworkPolicy` per-CNP-body destination-
/// identity selector-axis key every `cilium_network_policies`-emitted
/// CNP document mounts its L3-target `LabelSelector` under
/// (`spec.endpointSelector`). Re-export of the canonical
/// [`caixa_core::CILIUM_KEY_ENDPOINT_SELECTOR`] so the Cilium-operator-
/// side per-CNP destination-identity-axis string lives in exactly one
/// place across every caixa renderer — caixa-mesh's
/// `cilium_network_policies` per-`(:de, :para)` `CiliumNetworkPolicy`
/// emitter (the `policy_spec.insert("endpointSelector", …)` call the
/// prior inline `"endpointSelector"` literal sat at) and every future
/// per-Cilium-side renderer the M3.x absorption roadmap acknowledges
/// now consult the same `&'static str`, so a future Cilium-CRD rebrand
/// on the destination-identity axis (unlikely on the CRD's stable
/// `cilium.io/v2` slot, but the coordination point the prior
/// [`CILIUM_KEY_TO_PORTS`] + [`caixa_core::KUBE_KEY_RULES`] +
/// [`CILIUM_KIND_NETWORK_POLICY`] + [`CILIUM_API_VERSION`] re-exports
/// anchor on the sibling per-CNP-body axis surface) lands in one place.
/// The prior inline literal split across the one production emitter and
/// two test-fixture navigation sites (destination-`endpointSelector`
/// retrieval whose downstream navigation chains ride through the same
/// axis-key) would have let a Cilium-CRD destination-identity axis
/// rebrand or a per-emitter typo (`"endpointselector"` /
/// `"endpointSelectors"` / `"endpoints"`) at any one site silently emit
/// a CNP whose destination-identity axis the Cilium CRD schema
/// validator drops as unknown; the policy binds against no destination
/// pods and every intra-mesh `:contratos` flow the affected CNP was
/// authored to allow drops at the eBPF data plane's default-deny gate
/// with no field naming the destination-identity-drift root cause, and
/// on the test-fixture side the drift silently masks the emission-side
/// pin (`.get("endpointSelector")` returns `None` under both the
/// drifted emitter and the drifted probe — the downstream
/// `.and_then(|s| s.get("matchLabels"))` chain short-circuits vacuously
/// because the outer selector-lookup is itself `None`). Peer to the
/// [`CILIUM_KEY_TO_PORTS`] re-export on the sibling canonical-per-CNP-
/// body-axis surface — extends the per-CNP-body re-export set from the
/// per-ingress-rule port-set container axis (the L4 dispatch container
/// half of the `(endpointSelector, ingress → toPorts → rules)` L3/L4/
/// L7-triad) onto the destination-identity axis half, completing the
/// per-CNP L3-target-selector re-export the M3 Aplicacao mesh
/// renderer's eBPF data-plane contract rests on.
pub use caixa_core::CILIUM_KEY_ENDPOINT_SELECTOR;

/// Canonical Cilium `CiliumNetworkPolicy` per-CNP-body traffic-direction
/// container-axis key every `cilium_network_policies`-emitted CNP
/// document mounts its permitted-inbound-per-`(:de, :para)` ingress-rule
/// list under (`spec.ingress[]`). Re-export of the canonical
/// [`caixa_core::CILIUM_KEY_INGRESS`] so the Cilium-operator-side per-
/// CNP inbound-traffic-dispatch container-key string lives in exactly
/// one place across every caixa renderer — caixa-mesh's
/// `cilium_network_policies` per-`(:de, :para)` `CiliumNetworkPolicy`
/// emitter (the `policy_spec.insert("ingress", …)` call the prior
/// inline `"ingress"` literal sat at) and every future per-Cilium-side
/// renderer the M3.x absorption roadmap acknowledges now consult the
/// same `&'static str`, so a future Cilium-CRD rebrand on the traffic-
/// direction axis (unlikely on the CRD's stable `cilium.io/v2` slot,
/// but the coordination point the prior [`CILIUM_KEY_ENDPOINT_SELECTOR`]
/// + [`CILIUM_KEY_TO_PORTS`] + [`caixa_core::KUBE_KEY_RULES`] +
/// [`CILIUM_KIND_NETWORK_POLICY`] + [`CILIUM_API_VERSION`] re-exports
/// anchor on the sibling per-CNP-body axis surface) lands in one place.
/// The prior inline literal split across the one production emitter and
/// eight test-fixture navigation sites (whose downstream navigation
/// chains — `fromEndpoints`, `toPorts`, `authentication` — ride through
/// the same axis-key) would have let a Cilium-CRD traffic-direction
/// axis rebrand or a per-emitter typo (`"Ingress"` / `"ingressRules"` /
/// `"inbound"`) at any one site silently emit a CNP whose ingress-rule
/// list the Cilium CRD schema validator drops as unknown; the policy
/// binds against the destination workload but admits no ingress
/// traffic, and every intra-mesh `:contratos` flow the affected CNP was
/// authored to allow drops at the eBPF data plane's default-deny gate
/// with no field naming the traffic-direction-drift root cause, and on
/// the test-fixture side the drift silently masks the emission-side
/// pin (`.get("ingress")` returns `None` under both the drifted emitter
/// and the drifted probe — every downstream `.and_then(|i|
/// i.as_sequence())` chain short-circuits vacuously because the outer
/// traffic-direction-lookup is itself `None`, and every per-CNP
/// downstream navigation — `fromEndpoints`, `toPorts`, `authentication`
/// — rides through the same short-circuited outer axis-lookup with no
/// field naming the drift root cause). Peer to the
/// [`CILIUM_KEY_ENDPOINT_SELECTOR`] + [`CILIUM_KEY_TO_PORTS`] re-exports
/// on the sibling canonical-per-CNP-body-axis surface — completes the
/// per-CNP L3/L4/L7-triad
/// `(endpointSelector, ingress → toPorts → rules)` re-export this
/// crate's `cilium_network_policies` renderer's eBPF data-plane
/// contract rests on by lifting the traffic-direction axis that
/// structurally separates the destination-identity axis from the port-
/// set-container axis nested beneath it.
pub use caixa_core::CILIUM_KEY_INGRESS;

/// Canonical Cilium `CiliumNetworkPolicy` per-ingress-rule identity-
/// source selector-list axis key every `cilium_network_policies`-emitted
/// CNP document mounts its permitted-source `LabelSelector` list under
/// (`spec.ingress[].fromEndpoints[]`). Re-export of the canonical
/// [`caixa_core::CILIUM_KEY_FROM_ENDPOINTS`] so the Cilium-operator-side
/// per-ingress-rule identity-source axis-key string lives in exactly one
/// place across every caixa renderer — caixa-mesh's
/// `cilium_network_policies` per-`(:de, :para)` `CiliumNetworkPolicy`
/// emitter (the `ingress_rule.insert("fromEndpoints", …)` call the prior
/// inline `"fromEndpoints"` literal sat at) and every future per-
/// Cilium-side renderer the M3.x absorption roadmap acknowledges now
/// consult the same `&'static str`, so a future Cilium-CRD rebrand on
/// the identity-source axis (unlikely on the CRD's stable `cilium.io/v2`
/// slot, but the coordination point the prior [`CILIUM_KEY_ENDPOINT_SELECTOR`]
/// + [`CILIUM_KEY_INGRESS`] + [`CILIUM_KEY_TO_PORTS`] +
/// [`caixa_core::KUBE_KEY_RULES`] + [`CILIUM_KIND_NETWORK_POLICY`] +
/// [`CILIUM_API_VERSION`] re-exports anchor on the sibling per-CNP-body
/// axis surface) lands in one place. The prior inline literal split
/// across the one production emitter and four test-fixture navigation
/// sites (whose downstream navigation chains — `.as_sequence()`,
/// `.first()`, `.get(KUBE_KEY_MATCH_LABELS)` — ride through the same
/// axis-key) would have let a Cilium-CRD identity-source axis rebrand
/// or a per-emitter typo (`"fromendpoints"` / `"fromEndPoint"` /
/// `"sourceEndpoints"`) at any one site silently emit a CNP whose per-
/// ingress-rule identity-source selector list the Cilium CRD schema
/// validator drops as unknown; the ingress rule admits no source pods,
/// and every intra-mesh `:contratos` flow the affected CNP was authored
/// to allow drops at the eBPF data plane's default-deny gate with no
/// field naming the identity-source-drift root cause, and on the test-
/// fixture side the drift silently masks the emission-side pin
/// (`.get("fromEndpoints")` returns `None` under both the drifted
/// emitter and the drifted probe — every downstream navigation short-
/// circuits vacuously because the outer identity-source-lookup is
/// itself `None`). Peer to the [`CILIUM_KEY_ENDPOINT_SELECTOR`] +
/// [`CILIUM_KEY_INGRESS`] + [`CILIUM_KEY_TO_PORTS`] re-exports on the
/// sibling canonical-per-CNP-body-axis surface — completes the per-CNP
/// identity-pair `(endpointSelector, fromEndpoints)` re-export this
/// crate's `cilium_network_policies` renderer's eBPF data-plane
/// contract rests on by lifting the identity-source axis structurally
/// paired with the destination-identity axis under the Cilium-operator-
/// side per-CNP SPIFFE-identity-bound access-control contract.
pub use caixa_core::CILIUM_KEY_FROM_ENDPOINTS;

/// Canonical Cilium `CiliumNetworkPolicy` per-`toPorts[]`-entry L4
/// port-tuple-list-container axis key every `cilium_network_policies`-
/// emitted CNP document mounts its per-port-set `[{port, protocol}]`
/// list under (`spec.ingress[].toPorts[].ports[]`). Re-export of the
/// canonical [`caixa_core::CILIUM_KEY_PORTS`] so the Cilium-operator-
/// side per-`toPorts[]`-entry L4-port-tuple-list-container-axis-key
/// string lives in exactly one place across every caixa renderer —
/// caixa-mesh's `cilium_network_policies` per-`(:de, :para)`
/// `CiliumNetworkPolicy` emitter (the `to_port.insert("ports", …)` call
/// the prior inline `"ports"` literal sat at) and every future per-
/// Cilium-side renderer the M3.x absorption roadmap acknowledges now
/// consult the same `&'static str`, so a future Cilium-CRD rebrand on
/// the L4 port-tuple-list-container axis (unlikely on the CRD's stable
/// `cilium.io/v2` slot, but the coordination point the prior
/// [`CILIUM_KEY_FROM_ENDPOINTS`] + [`CILIUM_KEY_ENDPOINT_SELECTOR`] +
/// [`CILIUM_KEY_INGRESS`] + [`CILIUM_KEY_TO_PORTS`] +
/// [`caixa_core::KUBE_KEY_RULES`] + [`CILIUM_KIND_NETWORK_POLICY`] +
/// [`CILIUM_API_VERSION`] re-exports anchor on the sibling per-CNP-body
/// axis surface) lands in one place. The prior inline literal split
/// across the one production emitter and two test-fixture navigation
/// sites (`cilium_pubsub_contracts_skip_l7_rules` — the
/// `to_ports.get("ports").is_some()` presence pin the L4-yes-L7-no
/// separation invariant hinges on;
/// `cnp_l4_fallback_port_reflects_default_servico_port` — the
/// `.and_then(|tp| tp.get("ports"))` navigation whose downstream
/// `.and_then(|s| s.first()).and_then(|p| p.get("port"))` chain reads
/// the per-port-set L4 port-tuple value the `DEFAULT_SERVICO_PORT`
/// fallback pins) would have let a Cilium-CRD L4 port-tuple-list-
/// container axis rebrand or a per-emitter typo (`"port"` /
/// `"portList"` / `"L4Ports"`) at any one site silently emit a per-
/// `toPorts[]` entry whose L4 port-tuple-list-container axis the Cilium
/// CRD schema validator drops as unknown; the port-set admits no
/// `(port, protocol)` tuple, and every intra-mesh `:contratos` flow the
/// affected CNP was authored to allow drops at the eBPF data plane's
/// default-deny gate with no field naming the L4-port-tuple-list-
/// container-drift root cause, and on the test-fixture side the drift
/// silently masks the emission-side pin (`.get("ports")` returns `None`
/// under both the drifted emitter and the drifted probe — every
/// downstream navigation short-circuits vacuously because the outer L4-
/// port-tuple-list-container-lookup is itself `None`). Peer to the
/// [`CILIUM_KEY_TO_PORTS`] re-export on the sibling canonical-per-CNP-
/// dispatch-axis surface — nests the per-port-set L4 port-tuple-list-
/// container axis structurally beneath the sibling
/// [`CILIUM_KEY_TO_PORTS`] port-set-container axis, extending the per-
/// CNP L3/L4/L7-triad
/// `(endpointSelector, ingress → toPorts → ports / rules)` re-export
/// with the L4-half's port-tuple-list-container axis this crate's
/// `cilium_network_policies` renderer's eBPF data-plane L4-allow
/// contract rests on.
pub use caixa_core::CILIUM_KEY_PORTS;

/// Canonical Cilium `CiliumNetworkPolicy` per-ingress-rule mutual-auth
/// policy body-axis key every `cilium_network_policies`-emitted CNP
/// document mounts its per-rule mTLS enforcement block under
/// (`spec.ingress[].authentication`). Re-export of the canonical
/// [`caixa_core::CILIUM_KEY_AUTHENTICATION`] so the Cilium-operator-
/// side per-ingress-rule mutual-auth-axis-key string lives in exactly
/// one place across every caixa renderer — caixa-mesh's
/// `cilium_network_policies` per-`(:de, :para)` `CiliumNetworkPolicy`
/// emitter (the `ingress_rule.insert("authentication", …)` call in
/// the `:politicas :mtls-required` overlay emit gate the prior inline
/// `"authentication"` literal sat at) and every future per-Cilium-
/// side renderer the M3.x absorption roadmap acknowledges now consult
/// the same `&'static str`, so a future Cilium-CRD rebrand on the
/// per-ingress-rule mutual-auth axis (unlikely on the CRD's stable
/// `cilium.io/v2` slot, but the coordination point the prior
/// [`CILIUM_KEY_PORTS`] + [`CILIUM_KEY_FROM_ENDPOINTS`] +
/// [`CILIUM_KEY_ENDPOINT_SELECTOR`] + [`CILIUM_KEY_INGRESS`] +
/// [`CILIUM_KEY_TO_PORTS`] + [`caixa_core::KUBE_KEY_RULES`] +
/// [`CILIUM_KIND_NETWORK_POLICY`] + [`CILIUM_API_VERSION`] re-exports
/// anchor on the sibling per-CNP-body axis surface) lands in one
/// place. The prior inline literal split across the one production
/// emitter and nine test-fixture navigation sites (the presence pin
/// under the `:mtls-required t` overlay, the absence pin under the
/// `:mtls-required` unset semantic, the explicit-`false`-emits-
/// disabled-mode pin under the `Some(false)` arm, the fan-out pin
/// across multiple contratos, the rule-level-not-nested position pin
/// with two nested-under-`fromEndpoints[]` and nested-under-
/// `toPorts[]` negative-navigation guards, the pubsub-carry-overlay-
/// too shape pin, and the yaml-string-scalar `mode`-value pin) would
/// have let a Cilium-CRD mutual-auth-axis rebrand or a per-emitter
/// typo (`"auth"` / `"mutualAuth"` / `"mtls"` / `"authPolicy"`) at
/// any one site silently emit a per-`ingress[]` entry whose mutual-
/// auth-axis the Cilium CRD schema validator drops as unknown; the
/// ingress rule falls back to the cluster-default authentication
/// mode and every intra-mesh `:contratos` flow the CNP was authored
/// to protect with per-edge SPIFFE-identity-bound mutual-auth
/// silently bypasses the mTLS handshake at the Cilium data-plane's
/// default-authentication mode with no field naming the mutual-
/// auth-axis-drift root cause. On the test-fixture side the drift
/// silently masks the emission-side pin
/// (`.get("authentication")` returns `None` under both the drifted
/// emitter and the drifted probe — every downstream
/// `.and_then(|a| a.get("mode"))` chain short-circuits vacuously
/// because the outer mutual-auth-body-lookup is itself `None`). Peer
/// to the [`CILIUM_KEY_FROM_ENDPOINTS`] + [`CILIUM_KEY_TO_PORTS`]
/// re-exports on the sibling per-ingress-rule-body-axis surfaces —
/// completes the per-ingress-rule-body triple
/// `(fromEndpoints, toPorts, authentication)` this crate's
/// `cilium_network_policies` renderer's SPIFFE-identity-bound per-
/// edge mTLS contract rests on.
pub use caixa_core::CILIUM_KEY_AUTHENTICATION;

/// Canonical Cilium `CiliumNetworkPolicy` per-`ingress[].authentication`
/// block mTLS-mode-discriminator leaf-scalar-axis key every
/// `cilium_network_policies`-emitted CNP document mounts its per-rule
/// mutual-auth mode leaf under (`spec.ingress[].authentication.mode`).
/// Re-export of the canonical [`caixa_core::CILIUM_KEY_MODE`] so the
/// Cilium-operator-side per-ingress-rule mutual-auth-mode-discriminator
/// leaf-axis key string lives in exactly one place across every caixa
/// renderer — caixa-mesh's `cilium_network_policies` per-`(:de, :para)`
/// `CiliumNetworkPolicy` emitter (the single-field-overlay call in the
/// `:politicas :mtls-required` overlay emit gate the prior inline
/// `"mode"` literal sat at) and every future per-Cilium-side renderer
/// the M3.x absorption roadmap acknowledges now consult the same
/// `&'static str`, so a future Cilium-CRD rebrand on the per-
/// authentication-block mode-discriminator leaf-axis (unlikely on the
/// CRD's stable `cilium.io/v2` slot, but the coordination point the
/// prior [`CILIUM_KEY_AUTHENTICATION`] re-export anchors on the parent
/// per-ingress-rule mutual-auth-body-axis) lands in one place. The
/// prior inline literal split across the one production emitter site
/// and five test-fixture navigation sites (the presence pin under the
/// `:mtls-required t` overlay, the explicit-`false`-emits-disabled-
/// mode pin under the `Some(false)` arm, the fan-out pin across
/// multiple contratos, the pubsub-carry-overlay-too shape pin, and the
/// yaml-string-scalar `mode`-value pin) would have let a Cilium-CRD
/// mutual-auth-mode-leaf rebrand or a per-emitter typo (`"policy"` /
/// `"authMode"` / `"handshakeMode"`) at any one site silently emit a
/// per-`ingress[]` entry whose mutual-auth-block mode-discriminator-
/// leaf-axis the Cilium CRD schema validator drops as unknown; the
/// ingress rule falls back to the cluster-default authentication mode
/// and every intra-mesh `:contratos` flow the CNP was authored to
/// protect with per-edge SPIFFE-identity-bound mutual-auth silently
/// bypasses the mTLS handshake at the Cilium data-plane's default-
/// authentication mode with no field naming the mutual-auth-mode-
/// leaf-axis-drift root cause. On the test-fixture side the drift
/// silently masks the emission-side pin (`.get("mode")` returns `None`
/// under both the drifted-key emitter and the drifted-key probe —
/// every downstream `.and_then(|v| v.as_str())` chain short-circuits
/// vacuously because the outer mode-leaf-lookup is itself `None`).
/// Peer to the [`CILIUM_KEY_AUTHENTICATION`] re-export on the parent
/// per-ingress-rule mutual-auth-body-axis surface — completes the
/// per-rule mutual-auth `(authentication → mode)` body/leaf axis
/// re-export pair this crate's `cilium_network_policies` renderer's
/// SPIFFE-identity-bound per-edge mTLS enforcement contract rests on.
pub use caixa_core::CILIUM_KEY_MODE;

/// Canonical Cilium `CiliumNetworkPolicy` `MutualAuthenticationMode` OpenAPI
/// schema enum's `required` mTLS-mandatory per-`ingress[].authentication.mode`
/// scalar-value every `cilium_network_policies`-emitted CNP document declares
/// under the `:mtls-required t` affirmative arm of the typed `:politicas
/// :mtls-required` tristate. Re-export of the canonical
/// [`caixa_core::CILIUM_AUTH_MODE_REQUIRED`] so the Cilium-agent-side per-rule
/// mutual-auth-mandatory scalar-value string lives in exactly one place across
/// every caixa renderer — caixa-mesh's `cilium_network_policies` per-`(:de,
/// :para)` `CiliumNetworkPolicy` emitter (the single-field-overlay closure's
/// `if required { … }` affirmative arm the prior inline `"required"` literal
/// sat at, plus the presence / fan-out / pubsub-carry-overlay-too test-fixture
/// probes that pin the emitted value under the `:mtls-required t` shape) and
/// every future per-Cilium-side renderer the M3.x absorption roadmap
/// acknowledges now consult the same `&'static str`. Peer to the sibling
/// [`CILIUM_AUTH_MODE_DISABLED`] re-export on the explicit-opt-out arm of the
/// same tristate — completes the per-authn-block `(mode → {required,
/// disabled})` author-reachable-scalar-value-pair re-export pair this crate's
/// `cilium_network_policies` renderer's SPIFFE-identity-bound per-edge mTLS
/// enforcement + explicit-opt-out contract rests on.
pub use caixa_core::CILIUM_AUTH_MODE_REQUIRED;

/// Canonical Cilium `CiliumNetworkPolicy` `MutualAuthenticationMode` OpenAPI
/// schema enum's `disabled` mTLS-skipped per-`ingress[].authentication.mode`
/// scalar-value every `cilium_network_policies`-emitted CNP document declares
/// under the explicit `Some(false)` opt-out arm of the typed `:politicas
/// :mtls-required` tristate (distinct from the `None` slot-absent arm the
/// renderer maps to omit-the-block-entirely). Re-export of the canonical
/// [`caixa_core::CILIUM_AUTH_MODE_DISABLED`] so the Cilium-agent-side per-rule
/// mutual-auth-skipped scalar-value string lives in exactly one place across
/// every caixa renderer — caixa-mesh's `cilium_network_policies` per-`(:de,
/// :para)` `CiliumNetworkPolicy` emitter (the single-field-overlay closure's
/// `else { … }` opt-out arm the prior inline `"disabled"` literal sat at,
/// plus the `cnp_explicit_mtls_required_false_emits_disabled_mode` test-
/// fixture probe that pins the emitted value under the explicit-opt-out
/// shape) and every future per-Cilium-side renderer the M3.x absorption
/// roadmap acknowledges now consult the same `&'static str`. Peer to the
/// sibling [`CILIUM_AUTH_MODE_REQUIRED`] re-export on the affirmative arm of
/// the same tristate.
pub use caixa_core::CILIUM_AUTH_MODE_DISABLED;

/// Canonical `bool → &'static str` bijection projection every consumer of the
/// Cilium `CiliumNetworkPolicy` `MutualAuthenticationMode` OpenAPI schema
/// enum's closed-set author-reachable scalar-value pair
/// ([`CILIUM_AUTH_MODE_REQUIRED`] / [`CILIUM_AUTH_MODE_DISABLED`]) consults
/// so the per-tristate-arm dispatch — `Some(true)` (mTLS handshake
/// mandatory) → [`CILIUM_AUTH_MODE_REQUIRED`], `Some(false)` (mTLS handshake
/// skipped, explicit opt-out) → [`CILIUM_AUTH_MODE_DISABLED`] — lives in
/// exactly one place. Re-export of the canonical
/// [`caixa_core::cilium_auth_mode`] so a future Cilium CNP
/// `MutualAuthenticationMode` enum rebrand (either arm's scalar-value or
/// the per-arm dispatch shape) lands at the two consts + one projection
/// body rather than at scattered per-emitter inline closure bodies.
/// Consumed by the `cilium_network_policies` per-`(:de, :para)` emitter's
/// `single_field_overlay(spec.politicas.mtls_required, CILIUM_KEY_MODE,
/// |required| serde_yaml::Value::String(cilium_auth_mode(required).into()))`
/// closure body the prior inline `if required { CILIUM_AUTH_MODE_REQUIRED }
/// else { CILIUM_AUTH_MODE_DISABLED }` per-arm dispatch sat at (plus the
/// caixa-core in-file `single_field_overlay_threads_typed_value_through_
/// closure` generic-helper pin that mirrors the production overlay's
/// shape letter-for-letter and now threads through the same projection).
/// Peer to the [`CILIUM_AUTH_MODE_REQUIRED`] / [`CILIUM_AUTH_MODE_DISABLED`]
/// re-export pair the two arms of the same enum land on — completes the
/// canonical `(closed-set-CRD-schema-enum-value pair, per-typed-arm
/// dispatch projection)` compound re-export triple this crate's
/// `cilium_network_policies` renderer's SPIFFE-identity-bound per-edge
/// mTLS enforcement + explicit-opt-out contract rests on.
pub use caixa_core::cilium_auth_mode;

/// Canonical M3 `:contratos` edge-direction separator byte-string every
/// caixa-mesh emitter that encodes a typed edge as a K8s-name-shaped
/// scalar reads from — the per-`(:de, :para)`
/// [`LABEL_CONTRATO`] value threaded through
/// [`contrato_edge_label`] and the per-`(:de, :para)`
/// `CiliumNetworkPolicy` `metadata.name` threaded through
/// [`cilium_network_policy_name`]. Re-export of the canonical
/// [`caixa_core::CONTRATO_EDGE_LABEL_SEPARATOR`] so the load-bearing
/// `-to-` byte-string lives in exactly one place across every caixa
/// renderer — caixa-mesh's `cilium_network_policies` per-`(:de, :para)`
/// group (the two writer sites the prior inline `format!` literals
/// sat at) and every future per-target renderer that encodes a typed
/// M3 edge as a K8s-name-shaped scalar. A future edge-encoding rebrand
/// (`-to-` → `->` for compactness, `-to-` → `_to_` to reserve `-` for
/// embedded DNS-1123-label boundaries, an edge-direction-arrow
/// migration to UTF-8 shapes) lands at the canonical
/// [`caixa_core::CONTRATO_EDGE_LABEL_SEPARATOR`] declaration, not at
/// this crate's per-group writer sites. Peer with the
/// [`contrato_edge_label`] / [`cilium_network_policy_name`] composer
/// re-exports that consume this const — together the three items
/// close the canonical per-`(:de, :para)` CNP identity pair
/// `(metadata.labels.pleme.pleme.io/contrato, metadata.name)` onto
/// one shared edge-encoding source of truth.
pub use caixa_core::CONTRATO_EDGE_LABEL_SEPARATOR;

/// Canonical M3 `:contratos` edge label value composer — the
/// `<de>-to-<para>` K8s-name-shaped scalar every per-`(:de, :para)`
/// `CiliumNetworkPolicy` document carries at its
/// `metadata.labels.pleme.pleme.io/contrato` axis. Re-export of the
/// canonical [`caixa_core::contrato_edge_label`] composer so the
/// per-CNP `LABEL_CONTRATO`-value construction lives in exactly one
/// place across every caixa renderer. Reads from the lifted
/// [`CONTRATO_EDGE_LABEL_SEPARATOR`] byte-string so a future
/// edge-encoding rebrand lands at one canonical composition. Peer of
/// [`cilium_network_policy_name`] on the sibling per-CNP
/// `metadata.name` composition axis — the two composers close the
/// canonical `(LABEL_CONTRATO-value, metadata.name)` per-CNP identity
/// pair on one shared edge-encoding source of truth
/// ([`CONTRATO_EDGE_LABEL_SEPARATOR`]).
pub use caixa_core::contrato_edge_label;

/// Canonical per-`(:de, :para)` `CiliumNetworkPolicy` `metadata.name`
/// composer — the `<aplicacao>-<de>-to-<para>` K8s-name-shaped
/// scalar every caixa-mesh `cilium_network_policies` emitter mounts
/// its per-edge CNP under. Re-export of the canonical
/// [`caixa_core::cilium_network_policy_name`] composer so the per-CNP
/// name construction lives in exactly one place across every caixa
/// renderer. Composes on the lifted [`contrato_edge_label`] helper so
/// the two writer-side axes — the CNP
/// `metadata.labels.pleme.pleme.io/contrato` value and the CNP
/// `metadata.name` — share one canonical edge-encoding source of
/// truth ([`CONTRATO_EDGE_LABEL_SEPARATOR`]). Peer of
/// [`contrato_edge_label`] on the parent-composition axis — the two
/// writer-side composers close the canonical
/// `(LABEL_CONTRATO-value, metadata.name)` per-CNP identity pair so a
/// future edge-encoding rebrand or a per-emitter typo can't silently
/// split the two axes at emit time and orphan every operator-side
/// grep-by-label query at apply time far from the source caixa.lisp.
pub use caixa_core::cilium_network_policy_name;

/// Canonical per-`:entrada` `HTTPRoute` `metadata.name` composer —
/// the `<aplicacao>-<para>` K8s-name-shaped scalar every caixa-mesh
/// `gateway_routes` emitter mounts its per-`:entrada` HTTPRoute
/// under. Re-export of the canonical
/// [`caixa_core::gateway_api_http_route_name`] composer so the
/// per-HTTPRoute name construction lives in exactly one place across
/// every caixa renderer. Peer of the sibling
/// [`cilium_network_policy_name`] composer on the per-Aplicacao
/// per-CR K8s-name-shaped-identity-scalar axis: the CNP-name composer
/// carries the per-`(:de, :para)` L4/L7 policy CR name and this
/// composer carries the per-`:entrada` L7 route CR name — both share
/// the same "aplicacao-prefixed sub-identity" discipline (an
/// aplicacao-prefix joined to a per-CR sub-axis by a canonical `-`
/// separator) so a future substrate-side per-Aplicacao Gateway API
/// axis extension (`GRPCRoute` on grpc-shaped `:contratos` payloads,
/// `TCPRoute` on l4-only tcp payloads, per-`:entrada` `HTTPRouteFilter`
/// / `BackendTLSPolicy` overlays) reaches the shared naming
/// discipline through this composer's peer-shape by construction.
///
/// Until this lift landed the HTTPRoute `metadata.name` axis sat as a
/// verbatim inline `format!("{}-{}", caixa.nome, entrada.para)` at
/// the [`gateway_routes`] emitter (with an in-file test-side probe
/// pinning the expected `checkout-cart` byte-shape by verbatim
/// literal), and any future name-encoding rebrand on this axis would
/// have had to be threaded through both sites in lockstep or the
/// HTTPRoute `metadata.name` silently split from the operator-side
/// grep-by-name / `kubectl get httproute -n tatara-system
/// <aplicacao>-<para>` lookup encoding at apply time far from the
/// source caixa.lisp. See
/// [`caixa_core::gateway_api_http_route_name`] for the full lift
/// rationale.
pub use caixa_core::gateway_api_http_route_name;

/// Canonical M3 [`caixa_core::aplicacao::PlacementStrategy::SingleNode`]
/// variant discriminator scalar-value the `Serialize` derive on the
/// un-`rename`d enum emits under [`caixa_core::M3_PLACEMENT_KEY_ESTRATEGIA`] on every
/// `programs_for_aplicacao`-emitted programs.yaml entry authored with
/// `:placement (:estrategia SingleNode …)`. Re-export of the canonical
/// [`caixa_core::M3_PLACEMENT_ESTRATEGIA_SINGLE_NODE`] so the OTP-style
/// single-cluster-takeover distribution-strategy scalar lives in exactly
/// one place across every caixa renderer and every caixa-mesh test-fixture
/// probe that dispatches on the strategy string. Peer to the sibling
/// [`M3_PLACEMENT_ESTRATEGIA_REPLICATED`] / [`M3_PLACEMENT_ESTRATEGIA_SHARDED`]
/// re-exports on the other two arms of the same closed enum surface —
/// together the three constants name every author-reachable arm of the
/// M3 distribution-strategy discriminator.
pub use caixa_core::M3_PLACEMENT_ESTRATEGIA_SINGLE_NODE;

/// Canonical M3 [`caixa_core::aplicacao::PlacementStrategy::Replicated`]
/// variant discriminator scalar-value the `Serialize` derive on the
/// un-`rename`d enum emits under [`caixa_core::M3_PLACEMENT_KEY_ESTRATEGIA`] on every
/// `programs_for_aplicacao`-emitted programs.yaml entry authored with
/// `:placement (:estrategia Replicated …)` (and — because the enum's
/// `default()` is `Replicated` — every programs.yaml entry authored
/// without an explicit `:estrategia` slot). Re-export of the canonical
/// [`caixa_core::M3_PLACEMENT_ESTRATEGIA_REPLICATED`]. The
/// `programs_entry_placement_carries_strategy` test-fixture probe pins
/// the emitted value against this re-export so a future variant rename
/// or `rename_all` attribute at the aplicacao module reaches the caixa-
/// mesh probe by construction rather than silently rebranding the
/// substrate's default distribution posture. Peer to the sibling
/// [`M3_PLACEMENT_ESTRATEGIA_SINGLE_NODE`] / [`M3_PLACEMENT_ESTRATEGIA_SHARDED`]
/// re-exports on the other two arms of the same closed enum surface.
pub use caixa_core::M3_PLACEMENT_ESTRATEGIA_REPLICATED;

/// Canonical M3 [`caixa_core::aplicacao::PlacementStrategy::Sharded`]
/// variant discriminator scalar-value the `Serialize` derive on the
/// un-`rename`d enum emits under [`caixa_core::M3_PLACEMENT_KEY_ESTRATEGIA`] on every
/// `programs_for_aplicacao`-emitted programs.yaml entry authored with
/// `:placement (:estrategia Sharded :shard-key …)` — the one arm on
/// which the sibling [`caixa_core::M3_PLACEMENT_KEY_SHARD_KEY`] sub-block is
/// required (`AplicacaoSpec::validate_placement` gates
/// `shard_key.is_some() == matches!(estrategia, Sharded)` as a
/// structural partition of every validated Placement). Re-export of the
/// canonical [`caixa_core::M3_PLACEMENT_ESTRATEGIA_SHARDED`]. The
/// `programs_entry_placement_carries_shard_key_when_sharded` test-fixture
/// probe pins the emitted value against this re-export so a future
/// variant rename or `rename_all` attribute at the aplicacao module
/// reaches the caixa-mesh probe by construction rather than silently
/// collapsing the hash-keyed distribution back onto the aggregator's
/// default. Peer to the sibling [`M3_PLACEMENT_ESTRATEGIA_SINGLE_NODE`] /
/// [`M3_PLACEMENT_ESTRATEGIA_REPLICATED`] re-exports on the other two
/// arms of the same closed enum surface.
pub use caixa_core::M3_PLACEMENT_ESTRATEGIA_SHARDED;

/// Canonical Cilium `CiliumNetworkPolicy` per-`ingress[].toPorts[].rules`
/// L7-HTTP-rule-list-discriminator container-axis key every
/// `cilium_network_policies`-emitted CNP document mounts its per-
/// `toPorts[]` entry L7 URL-path-prefix predicate list under
/// (`spec.ingress[].toPorts[].rules.http`). Re-export of the canonical
/// [`caixa_core::CILIUM_KEY_HTTP`] so the Cilium-CRD per-`toPorts[]` L7-
/// HTTP-rule-list-discriminator container-axis key string lives in exactly
/// one place across every caixa renderer — caixa-mesh's
/// `cilium_network_policies` per-`(:de, :para)` `CiliumNetworkPolicy`
/// emitter (the single production-code site the prior inline `"http"`
/// literal sat at, the `rules.insert("http", …)` call in the
/// `WitTarget::Http` L7 introspection emit branch) and every future
/// per-Cilium-side renderer the M3.x absorption roadmap acknowledges now
/// consult the same `&'static str`, so a future Cilium-CRD rebrand on the
/// per-`toPorts[]` L7-HTTP-rule-list-discriminator axis (unlikely on the
/// CRD's stable `cilium.io/v2` slot, but the coordination point the
/// sibling [`CILIUM_KEY_AUTHENTICATION`] / [`CILIUM_KEY_MODE`] re-exports
/// anchor on the parent per-ingress-rule mutual-auth body/leaf axis pair)
/// lands in one place. The prior inline literal split across the one
/// production emitter site and two test-fixture navigation sites (the L7
/// fan-in path-capture pin across the multi-edge group, the per-HTTP-
/// contract L7-path presence pin) would have let a Cilium-CRD L7-HTTP-
/// rule-list-discriminator rebrand or a per-emitter typo (`"HTTP"` /
/// `"Http"` / `"httpRules"` / `"httpMatch"`) at any one site silently
/// emit a per-`toPorts[]` entry whose L7-HTTP-rule-list-discriminator
/// key the Cilium CRD schema validator drops as unknown; the per-
/// `toPorts[]` entry falls back to L4-only enforcement — no L7 URL-
/// path predicate is applied — silently admitting every HTTP-method /
/// URL-path combination the ingress rule was authored to filter to the
/// exact path prefix set the typed `:contratos` graph names at the L7
/// introspection axis, with no field naming the L7-HTTP-rule-list-
/// discriminator-drift root cause. On the test-fixture side the drift
/// silently masks the emission-side pin (`.get("http")` returns `None`
/// under both the drifted-key emitter and the drifted-key probe —
/// every downstream `.and_then(|h| h.as_sequence())` chain short-
/// circuits vacuously because the outer L7-HTTP-rule-list-lookup is
/// itself `None`). Peer to the [`CILIUM_KEY_MODE`] /
/// [`CILIUM_KEY_AUTHENTICATION`] re-exports on the sibling per-
/// ingress-rule mutual-auth body/leaf axis pair — completes the per-
/// `toPorts[]` L7-introspection `(rules → http)` container/protocol-
/// discriminator axis re-export pair this crate's
/// `cilium_network_policies` renderer's HTTP-shaped-`:contratos` URL-
/// path-prefix-filtering L7-enforcement contract rests on.
pub use caixa_core::CILIUM_KEY_HTTP;

/// Canonical Cilium `CiliumNetworkPolicy` per-`ingress[].toPorts[].rules.http[]`
/// per-HTTP-rule URL-path-predicate leaf-scalar-axis key every
/// `cilium_network_policies`-emitted CNP document mounts its per-HTTP-rule
/// URL-path-prefix predicate scalar under
/// (`spec.ingress[].toPorts[].rules.http[].path`). Re-export of the canonical
/// [`caixa_core::CILIUM_KEY_PATH`] so the Cilium-CRD per-`rules.http[]`
/// URL-path-predicate leaf-axis key string lives in exactly one place across
/// every caixa renderer — caixa-mesh's `cilium_network_policies` per-`(:de,
/// :para)` `CiliumNetworkPolicy` emitter (the single production-code site
/// the prior inline `"path"` literal sat at, the `http_rule.insert("path", …)`
/// call in the `WitTarget::Http` L7 introspection emit branch) and every
/// future per-Cilium-side renderer the M3.x absorption roadmap acknowledges
/// now consult the same `&'static str`, so a future Cilium-CRD rebrand on
/// the per-`rules.http[]` URL-path-predicate leaf-axis (unlikely on the
/// CRD's stable `cilium.io/v2` slot, but the coordination point the sibling
/// [`CILIUM_KEY_HTTP`] re-export anchors on the parent per-`toPorts[]`
/// L7-HTTP-rule-list-discriminator container-axis it nests inside) lands in
/// one place. The prior inline literal split across the one production
/// emitter site and one test-fixture navigation site (the per-HTTP-rule
/// URL-path-predicate presence-and-value pin on the aplicacao fixture's
/// cart→catalog HTTP-shaped `:contratos` edge) would have let a Cilium-CRD
/// per-HTTP-rule URL-path-predicate rebrand or a per-emitter typo (`"Path"`
/// / `"pathPrefix"` / `"regex"` / `"urlPath"` / `"pathMatch"`) at any one
/// site silently emit a per-`rules.http[]` entry whose URL-path-predicate
/// leaf-axis key the Cilium CRD schema validator drops as unknown; the
/// per-`rules.http[]` entry falls back to a match-any-URL-path predicate —
/// the per-`toPorts[]` L7 rule admits every URL path on the destination
/// port silently, bypassing the URL-path-prefix predicate the typed
/// `:contratos` HTTP-shaped edge's `:endpoint` slot names at the L7
/// introspection axis, with no field naming the URL-path-predicate-leaf-
/// axis-drift root cause. On the test-fixture side the drift silently
/// masks the emission-side pin (`.get("path")` returns `None` under both
/// the drifted-key emitter and the drifted-key probe — every downstream
/// `.and_then(|v| v.as_str())` chain short-circuits vacuously because the
/// outer per-HTTP-rule URL-path-lookup is itself `None`). Peer to the
/// [`CILIUM_KEY_HTTP`] re-export on the parent per-`toPorts[]` L7-HTTP-
/// rule-list-discriminator container-axis it nests inside — completes the
/// per-`toPorts[]` L7-introspection `(rules → http → path)` container /
/// protocol-discriminator / URL-path-predicate axis triple re-export chain
/// this crate's `cilium_network_policies` renderer's HTTP-shaped-
/// `:contratos` URL-path-prefix-filtering L7-enforcement contract rests on.
/// Distinct from the sibling K8s-Gateway-API-side [`GATEWAY_API_KEY_PATH`]
/// per-`HTTPRouteMatch` path-matcher container-axis re-export: both re-
/// exports carry the same underlying `"path"` string but name distinct
/// schema axes on distinct CRD groups (the Cilium-side leaf on the
/// `cilium.io/v2` `CiliumNetworkPolicy` CRD's per-`rules.http[]` entry,
/// the Gateway-API-side container on the `gateway.networking.k8s.io/v1`
/// `HTTPRoute` CRD's `spec.rules[].matches[]` entry), so the sibling
/// `pub use` declarations stay independent for the same axis-independence
/// reason the sibling [`CILIUM_KIND_NETWORK_POLICY`] /
/// [`GATEWAY_API_KIND_GATEWAY`] / [`GATEWAY_API_KIND_HTTP_ROUTE`] kind-
/// discriminator re-exports stay independent across the two CRD groups.
/// The axis-independence discipline lives at the rustc symbol-name
/// axis (the two `pub use caixa_core::CILIUM_KEY_PATH` /
/// `pub use caixa_core::GATEWAY_API_KEY_PATH` symbol re-exports a
/// future rebrand of one leaves the other structurally untouched under)
/// rather than the runtime-address axis — Rust's `&'static str` interner
/// coalesces identical byte-sequences onto one storage allocation at
/// codegen time, so the per-axis re-export identity pin against the
/// canonical caixa-core declaration on each axis is what actually
/// forbids a sibling local `pub const` from drifting, not a cross-axis
/// pointer-inequality assertion.
pub use caixa_core::CILIUM_KEY_PATH;

/// Canonical K8s Gateway API CRD `kind` discriminator every
/// `gateway_routes`-emitted `Gateway` document declares at its top-level
/// [`caixa_core::KUBE_KEY_KIND`] axis. Re-export of the canonical
/// [`caixa_core::GATEWAY_API_KIND_GATEWAY`] so the Gateway-API-conformant
/// CRD `kind` discriminator string lives in exactly one place across
/// every caixa renderer — caixa-mesh's `gateway_routes` per-Aplicacao
/// `Gateway` emitter (the single production-code site the prior inline
/// `"Gateway"` literal sat at, caixa-mesh/src/lib.rs:578 — the
/// `kube_resource_skeleton` kind argument) and every future per-
/// Gateway-API-side renderer the M3.x absorption roadmap acknowledges
/// now consult the same `&'static str`, so a future Gateway-API rebrand
/// (e.g. an upstream rename to `GatewayV1` post-GA) is a one-line edit
/// on the canonical [`caixa_core::GATEWAY_API_KIND_GATEWAY`] declaration,
/// not a coordinated rewrite across this crate's `kube_resource_skeleton`
/// call site + every future per-target renderer the substrate adds.
/// The prior inline literal would have let a Gateway-API kind rebrand
/// on the caixa-mesh side without a coordinated edit on the matching
/// in-file `gateway_carries_canonical_kube_skeleton_without_labels` /
/// `render_all_includes_every_artifact_kind` test fixture pins silently
/// emit a `Gateway` whose top-level kind drifts off the lifted-test-
/// fixture pins — apply-side: the Gateway lands outside the apiserver-
/// side CRD registration, every external `:entrada` flow drops at the
/// gateway-class-controller's reconcile loop with no field naming the
/// kind-drift root cause. Peer to the [`GATEWAY_API_API_VERSION`]
/// re-export on the sibling canonical-Gateway-API-CRD-apiVersion-axis —
/// extends the discipline from the apiVersion half of the
/// `(apiVersion, kind)` CRD-lookup tuple onto the kind half on the
/// same Gateway-API-CRD-axis, beginning the per-Gateway-API-CRD
/// kind+apiVersion re-export pair this crate's `gateway_routes`
/// renderer's external `:entrada` ingress contract rests on. Peer to
/// the [`CILIUM_KIND_NETWORK_POLICY`] re-export on the sibling
/// canonical-Cilium-CRD-kind-discriminator surface.
pub use caixa_core::GATEWAY_API_KIND_GATEWAY;

/// Canonical K8s Gateway API CRD `kind` discriminator every
/// `gateway_routes`-emitted `HTTPRoute` document declares at its
/// top-level [`caixa_core::KUBE_KEY_KIND`] axis. Re-export of the
/// canonical [`caixa_core::GATEWAY_API_KIND_HTTP_ROUTE`] so the
/// Gateway-API-conformant CRD `kind` discriminator string lives in
/// exactly one place across every caixa renderer — caixa-mesh's
/// `gateway_routes` per-Aplicacao `HTTPRoute` emitter (the single
/// production-code site the prior inline `"HTTPRoute"` literal sat at,
/// caixa-mesh/src/lib.rs:663 — the `kube_resource_skeleton` kind
/// argument) and every future per-Gateway-API-side renderer the M3.x
/// absorption roadmap acknowledges now consult the same `&'static str`,
/// so a future Gateway-API rebrand (e.g. an upstream rename to
/// `HTTPRouteV1` post-GA) is a one-line edit on the canonical
/// [`caixa_core::GATEWAY_API_KIND_HTTP_ROUTE`] declaration, not a
/// coordinated rewrite across this crate's `kube_resource_skeleton`
/// call site + every future per-target renderer the substrate adds.
/// The prior inline literal would have let a Gateway-API kind rebrand
/// on the caixa-mesh side without a coordinated edit on the matching
/// in-file `httproute_carries_canonical_kube_skeleton_without_labels`
/// / `render_all_includes_every_artifact_kind` test fixture pins
/// silently emit an `HTTPRoute` whose top-level kind drifts off the
/// lifted-test-fixture pins — apply-side: the HTTPRoute lands outside
/// the apiserver-side CRD registration, every external `:entrada` flow
/// drops at the gateway-class-controller's reconcile loop with no
/// field naming the kind-drift root cause. Peer to the
/// [`GATEWAY_API_KIND_GATEWAY`] re-export on the sibling canonical-
/// Gateway-API-CRD-`kind`-discriminator surface — completes the
/// per-Gateway-API-CRD `kind`-axis re-export pair this crate's
/// `gateway_routes` renderer's external `:entrada` ingress contract
/// rests on across the `(Gateway, HTTPRoute)` pair the renderer emits
/// together.
pub use caixa_core::GATEWAY_API_KIND_HTTP_ROUTE;

/// Canonical K8s Gateway API v1 `ProtocolType` OpenAPI schema enum's
/// `HTTP` listener-protocol scalar value every `gateway_routes`-emitted
/// `Gateway` document's first (and V0-only) listener declares under its
/// [`caixa_core::KUBE_KEY_PROTOCOL`] axis. Re-export of the canonical
/// [`caixa_core::GATEWAY_API_PROTOCOL_HTTP`] so the Gateway-API-
/// implementation-side per-listener L7-parser-selection scalar value
/// lives in exactly one place across every caixa renderer — caixa-mesh's
/// `gateway_routes` per-`:entrada` `Gateway` emitter (the single
/// production-code site the prior inline `"HTTP".into()` literal sat at,
/// caixa-mesh/src/lib.rs:2123 — the per-listener `KUBE_KEY_PROTOCOL`
/// scalar-value emit) and every future per-Gateway-API-side renderer the
/// M3.x absorption roadmap acknowledges now consult the same
/// `&'static str`, so a future Gateway API `ProtocolType` enum rebrand
/// (e.g. an upstream rename to `HTTP/1.1` / `HTTP/2` per the SIG-Network
/// per-version-scope proposal) is a one-line edit on the canonical
/// [`caixa_core::GATEWAY_API_PROTOCOL_HTTP`] declaration, not a
/// coordinated rewrite across this crate's `gateway_routes` renderer's
/// per-listener `KUBE_KEY_PROTOCOL`-scalar-value emit + the matching
/// in-file `gateway_listener_carries_aplicacao_host` test's
/// `assert_eq!(…, Some("HTTP"))` listener-protocol-value pin + every
/// future per-Gateway-API-side renderer the substrate adds. The prior
/// inline literal would have let a Gateway-API `ProtocolType` rebrand
/// on the caixa-mesh side without a coordinated edit on the matching
/// in-file test pin silently emit a `Gateway` whose listener-protocol
/// scalar drifts off the lifted-test-fixture pin — apply-side: the
/// gateway-class-controller's per-listener bind loop rejects the
/// `Gateway` at admission (the K8s Gateway API v1 `ProtocolType` OpenAPI
/// schema enum admits the closed set `{"HTTP", "HTTPS", "TCP", "TLS",
/// "UDP"}` verbatim), and every external `:entrada` HTTP flow drops at
/// the gateway-class-controller's admission gate with no field naming
/// the listener-protocol-drift root cause. Peer to the
/// [`GATEWAY_API_KIND_GATEWAY`] + [`GATEWAY_API_KIND_HTTP_ROUTE`]
/// re-exports on the sibling canonical-Gateway-API-CRD-`kind`-
/// discriminator surface + the [`DEFAULT_GATEWAY_CLASS_NAME`] re-export
/// on the sibling Gateway-controller-binding-scalar-value axis —
/// extends the Gateway-API-CRD-`kind`-value + Gateway-controller-
/// binding-value re-export set onto the sibling per-Gateway
/// `spec.listeners[].protocol` listener-protocol-scalar-value axis the
/// same `gateway_routes` renderer's external `:entrada` ingress contract
/// carries under the shared `Gateway` body.
pub use caixa_core::GATEWAY_API_PROTOCOL_HTTP;

/// Canonical K8s Gateway API v1 `PathMatchType` OpenAPI schema enum's
/// `PathPrefix` per-`HTTPRouteMatch` path-selection-predicate discriminator
/// value every `gateway_routes`-emitted `HTTPRoute` per-rule `matches[]`
/// entry declares under its per-match `spec.rules[].matches[].path.type`
/// scalar axis. Re-export of the canonical
/// [`caixa_core::GATEWAY_API_PATH_MATCH_TYPE_PATH_PREFIX`] so the Gateway-
/// API-implementation-side per-`HTTPRouteMatch` request-path-selection-
/// predicate discriminator scalar value lives in exactly one place across
/// every caixa renderer — caixa-mesh's `gateway_routes` per-Aplicacao
/// `HTTPRoute` emitter (the single production-code site the prior inline
/// `"PathPrefix".into()` literal sat at, caixa-mesh/src/lib.rs — the
/// per-match `path_match.insert("type", "PathPrefix")` scalar-value emit)
/// and every future per-Gateway-API-side renderer the M3.x absorption
/// roadmap acknowledges now consult the same `&'static str`, so a future
/// Gateway API `PathMatchType` enum rebrand (e.g. an upstream rename to
/// `Prefix` / `PathPrefixMatch` per the SIG-Network per-version-scope
/// proposal) is a one-line edit on the canonical
/// [`caixa_core::GATEWAY_API_PATH_MATCH_TYPE_PATH_PREFIX`] declaration,
/// not a coordinated rewrite across this crate's `gateway_routes`
/// renderer's per-match `path_match.insert` scalar-value emit + every
/// future per-Gateway-API-side renderer the substrate adds. The prior
/// inline literal at the one production emitter site would have let a
/// Gateway-API `PathMatchType` rebrand on the caixa-mesh side without a
/// coordinated caixa-core edit silently emit an `HTTPRoute` whose per-
/// match path-selection-predicate scalar drifts off the canonical
/// [`caixa_core::GATEWAY_API_PATH_MATCH_TYPE_PATH_PREFIX`] value —
/// apply-side: the K8s apiserver-side Gateway API v1 `PathMatchType`
/// OpenAPI schema enum admits the closed set
/// `{"Exact", "PathPrefix", "RegularExpression"}` verbatim, so any
/// drifted value lands the emitted `HTTPRoute` outside the enum's
/// admitted set and every external `:entrada` path-filtered flow drops
/// at the gateway-class-controller's admission gate with no field naming
/// the path-match-type-drift root cause. Peer to the
/// [`GATEWAY_API_PROTOCOL_HTTP`] re-export on the sibling per-Gateway-
/// listener L7-parser-selection scalar-value axis — extends the
/// canonical-Gateway-API-v1-OpenAPI-schema-enum-value single-sourcing
/// re-export discipline the `ProtocolType.HTTP` re-export established
/// onto the sibling `PathMatchType.PathPrefix` per-`HTTPRouteMatch`
/// path-selection-predicate discriminator the same `gateway_routes`
/// external `:entrada` ingress emitter carries under the shared
/// `HTTPRoute` body.
pub use caixa_core::GATEWAY_API_PATH_MATCH_TYPE_PATH_PREFIX;

/// Canonical K8s Gateway API `HTTPRoute` parent-Gateway-binding container-
/// axis key every `gateway_routes`-emitted `HTTPRoute` document mounts
/// its per-route parent-Gateway `[{name}]` attachment list under
/// (`spec.parentRefs[]`). Re-export of the canonical
/// [`caixa_core::GATEWAY_API_KEY_PARENT_REFS`] so the Gateway-API-
/// implementation-side per-HTTPRoute parent-Gateway-binding-container-
/// axis-key string lives in exactly one place across every caixa
/// renderer — caixa-mesh's `gateway_routes` per-Aplicacao `HTTPRoute`
/// emitter (the `r_spec.insert("parentRefs", …)` call the prior inline
/// `"parentRefs"` literal sat at, caixa-mesh/src/lib.rs:1389) and every
/// future per-Gateway-API-side renderer the M3.x absorption roadmap
/// acknowledges now consult the same `&'static str`, so a future
/// Gateway API rebrand on the parent-Gateway-binding axis (an upstream
/// Gateway API v2 rename to `parents` / `parentGateways` / `attachedTo`,
/// coordinated with the upstream SIG-Network Gateway API deprecation
/// cycle) is a one-line edit on the canonical
/// [`caixa_core::GATEWAY_API_KEY_PARENT_REFS`] declaration, not a
/// coordinated rewrite across this crate's `gateway_routes` renderer +
/// every future per-target renderer the substrate adds. The prior
/// inline literal at the one production emitter site would have let a
/// Gateway-API-CRD parent-Gateway-binding-axis rebrand or a per-
/// emitter typo (`"parentRef"` / `"parents"` / `"parentGateways"`)
/// silently emit an `HTTPRoute` whose parent-Gateway-binding axis the
/// Gateway API CRD schema validator drops as unknown — the route lands
/// unattached to any Gateway, and every external `:entrada` flow the
/// HTTPRoute was authored to accept drops at the Gateway API
/// implementation's per-Gateway HTTP-listener fan-in with no field
/// naming the parent-Gateway-binding-drift root cause. Peer to the
/// [`GATEWAY_API_KIND_HTTP_ROUTE`] + [`GATEWAY_API_KIND_GATEWAY`]
/// re-exports on the sibling canonical-Gateway-API-CRD-`kind`-
/// discriminator surface — pivots this crate's per-CNP-body-axis
/// re-export discipline onto the sibling per-HTTPRoute-body-axis
/// surface, beginning the per-Gateway-API-HTTPRoute-body-axis
/// canonical-string re-export set (`parentRefs`, future `hostnames`)
/// this crate's `gateway_routes` renderer's external `:entrada`
/// ingress contract rests on across the Gateway API HTTPRoute-side
/// per-route body-shape.
pub use caixa_core::GATEWAY_API_KEY_PARENT_REFS;

/// Canonical K8s Gateway API `HTTPRoute` per-`spec.parentRefs[]` entry
/// listener-selector sub-axis key every `gateway_routes`-emitted
/// `HTTPRoute` document mounts under each parent-Gateway attachment
/// (`spec.parentRefs[].sectionName`). Re-export of the canonical
/// [`caixa_core::GATEWAY_API_KEY_SECTION_NAME`] so the Gateway-API-
/// implementation-side per-parentRef listener-selector-sub-axis-key
/// string lives in exactly one place across every caixa renderer —
/// caixa-mesh's `gateway_routes` per-Aplicacao `HTTPRoute` emitter
/// (the per-parentRef `parent_ref.insert(<KEY>, …)` call whose paired
/// [`GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME`] `&'static str` value
/// binds the emitted route to the same listener the parent Gateway's
/// sole `listener.insert(GATEWAY_API_KEY_NAME, …)` call names) and
/// every future per-Gateway-API-side renderer the M3.x absorption
/// roadmap acknowledges now consult the same `&'static str`, so a
/// future Gateway API rebrand on the per-parentRef listener-selector
/// sub-axis (an upstream Gateway API v2 rename to `listenerName` /
/// `listener` / `attachTo`, coordinated with the upstream SIG-Network
/// Gateway API deprecation cycle) is a one-line edit on the canonical
/// [`caixa_core::GATEWAY_API_KEY_SECTION_NAME`] declaration, not a
/// coordinated rewrite across this crate's `gateway_routes` renderer
/// + every future per-target renderer the substrate adds.
///
/// The per-parentRef listener-selector sub-axis binds the emitted
/// `HTTPRoute` to exactly one listener on its parent Gateway (the
/// paired [`GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME`] byte-string names
/// the sole HTTP listener the substrate emits today). Omitting the
/// selector attaches the route to *every* listener on the parent
/// Gateway — the Gateway API v1 default fan-out that silently doubles
/// route emission once the substrate ships a second listener under
/// the HTTPS-by-default trajectory the peer
/// [`GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME`] docstring forecasts.
/// Pinning the selector by construction closes that drift footgun
/// structurally: the listener-name emitter and the sectionName
/// selector move as a single unit through one lifted `&'static str`,
/// so a substrate-side rebrand of the canonical listener-name
/// identifier reaches both sites at construction time.
///
/// Peer to the [`GATEWAY_API_KEY_PARENT_REFS`] re-export on the
/// sibling canonical-Gateway-API-HTTPRoute-body-axis surface — nests
/// the per-Gateway-API-HTTPRoute-body-axis canonical-string re-export
/// set (`parentRefs`, `backendRefs`, future `hostnames`) one level
/// deeper onto the per-parentRef listener-selector sub-axis this
/// crate's `gateway_routes` renderer's external `:entrada` ingress
/// contract now rests on across the Gateway API HTTPRoute-side per-
/// parentRef body-shape.
pub use caixa_core::GATEWAY_API_KEY_SECTION_NAME;

/// Canonical K8s Gateway API `HTTPRoute` per-rule backend-destination
/// container-axis key every `gateway_routes`-emitted `HTTPRoute` per-
/// rule block mounts its `[{name, port}]` backend fan-out list under
/// (`spec.rules[].backendRefs[]`). Re-export of the canonical
/// [`caixa_core::GATEWAY_API_KEY_BACKEND_REFS`] so the Gateway-API-
/// implementation-side per-rule backend-destination-container-axis-key
/// string lives in exactly one place across every caixa renderer —
/// caixa-mesh's `gateway_routes` per-Aplicacao `HTTPRoute` emitter
/// (the `rule.insert("backendRefs", …)` call the prior inline
/// `"backendRefs"` literal sat at, caixa-mesh/src/lib.rs:1414) and
/// every future per-Gateway-API-side renderer the M3.x absorption
/// roadmap acknowledges now consult the same `&'static str`, so a
/// future Gateway API rebrand on the per-rule backend-destination axis
/// (an upstream Gateway API v2 rename to `backends` / `forwardTo` /
/// `to`, coordinated with the upstream SIG-Network Gateway API
/// deprecation cycle) is a one-line edit on the canonical
/// [`caixa_core::GATEWAY_API_KEY_BACKEND_REFS`] declaration, not a
/// coordinated rewrite across this crate's `gateway_routes` renderer +
/// every future per-target renderer the substrate adds. The prior
/// inline literal at the one production emitter site + two test-side
/// fixture pins (`httproute_routes_to_entrada_para`'s
/// `.get("backendRefs")` navigation, `httproute_rule_keys_pin_overlay_position`'s
/// `contains_key("backendRefs")` presence pin) would have let a
/// Gateway-API-CRD per-rule backend-destination-axis rebrand or a per-
/// emitter typo (`"backendRef"` / `"backends"` / `"forwardTo"`)
/// silently emit an `HTTPRoute` whose per-rule backend-destination axis
/// the Gateway API CRD schema validator drops as unknown — no backend
/// is picked at the per-rule L7 dispatch, and every external `:entrada`
/// request the rule was authored to route drops at the gateway-class-
/// controller's per-rule reconcile with no field naming the backend-
/// destination-drift root cause. A drift on the test-fixture side
/// silently masks the emission-side pin (`.get("backendRefs")` returns
/// `None` under both the drifted-key emitter and the drifted-key probe
/// — the downstream `.and_then(|b| b.as_sequence())` /
/// `.and_then(|s| s.first())` chain short-circuits vacuously because
/// the outer per-rule backend-destination lookup is itself `None`).
/// Peer to the [`GATEWAY_API_KEY_PARENT_REFS`] re-export on the
/// sibling canonical-Gateway-API-HTTPRoute-body-axis surface — extends
/// the per-Gateway-API-HTTPRoute-body-axis canonical-string re-export
/// set (`parentRefs`, `backendRefs`, future `hostnames`) this crate's
/// `gateway_routes` renderer's external `:entrada` ingress contract
/// rests on across the Gateway API HTTPRoute-side per-route body-
/// shape.
pub use caixa_core::GATEWAY_API_KEY_BACKEND_REFS;

/// Canonical K8s Gateway API `HTTPRoute` per-rule route-match container-
/// axis key every `gateway_routes`-emitted `HTTPRoute` per-rule block
/// mounts its per-rule `[{path: {type, value}}]` route-match fan-out
/// list under (`spec.rules[].matches[]`). Re-export of the canonical
/// [`caixa_core::GATEWAY_API_KEY_MATCHES`] so the Gateway-API-
/// implementation-side per-rule route-match-container-axis-key string
/// lives in exactly one place across every caixa renderer —
/// caixa-mesh's `gateway_routes` per-Aplicacao `HTTPRoute` emitter
/// (the `rule.insert("matches", …)` call the prior inline `"matches"`
/// literal sat at, seeded from the Aplicacao's `:entrada :paths`
/// slot) and every future per-Gateway-API-side renderer the M3.x
/// absorption roadmap acknowledges now consult the same `&'static
/// str`, so a future Gateway API rebrand on the per-rule route-match
/// axis (an upstream Gateway API v2 rename to `match` /
/// `routeMatches` / `predicates`, coordinated with the upstream SIG-
/// Network Gateway API deprecation cycle) is a one-line edit on the
/// canonical [`caixa_core::GATEWAY_API_KEY_MATCHES`] declaration, not
/// a coordinated rewrite across this crate's `gateway_routes`
/// renderer + every future per-target renderer the substrate adds.
/// The prior inline literal at the one production emitter site + one
/// test-side fixture pin (`httproute_rule_keys_pin_overlay_position`'s
/// `contains_key("matches")` presence pin) would have let a Gateway-
/// API-CRD per-rule route-match-axis rebrand or a per-emitter typo
/// (`"match"` / `"routeMatches"` / `"predicates"`) silently emit an
/// `HTTPRoute` whose per-rule request-selection axis the Gateway API
/// CRD schema validator drops as unknown — the per-rule predicate
/// degrades to the wildcard match at the gateway-class-controller's
/// per-rule reconcile, the rule matches every request unconditionally,
/// and every external `:entrada` path filter the rule was authored
/// to enforce drops with no field naming the route-match-drift root
/// cause. A drift on the test-fixture side silently masks the
/// emission-side pin (`contains_key("matches")` returns `false` under
/// both the drifted-key emitter and the drifted-key probe). Peer to
/// the [`GATEWAY_API_KEY_BACKEND_REFS`] / [`GATEWAY_API_KEY_PARENT_REFS`]
/// re-exports on the sibling canonical-Gateway-API-HTTPRoute-body-
/// axis surface — completes the per-rule top-level-axis re-export
/// set (`matches`, `backendRefs`, `timeouts`, `retry`) the
/// `httproute_rule_keys_pin_overlay_position` pin binds against, so
/// every one of the four per-rule top-level axes now threads a
/// lifted `&'static str` apiece.
pub use caixa_core::GATEWAY_API_KEY_MATCHES;

/// Canonical K8s Gateway API `HTTPRoute` per-`HTTPRouteMatch` path-matcher
/// container-axis key every `gateway_routes`-emitted `HTTPRoute` per-rule
/// `matches[]` entry mounts its per-match `{type, value}` path-selection
/// predicate under (`spec.rules[].matches[].path`). Re-export of the
/// canonical [`caixa_core::GATEWAY_API_KEY_PATH`] so the Gateway-API-
/// implementation-side per-`HTTPRouteMatch` path-matcher-container-axis-
/// key string lives in exactly one place across every caixa renderer —
/// caixa-mesh's `gateway_routes` per-Aplicacao `HTTPRoute` emitter (the
/// per-match `match_entry.insert("path", …)` call the prior inline
/// `"path"` literal sat at, seeded from the Aplicacao's `:entrada :paths`
/// slot) and every future per-Gateway-API-side renderer the M3.x
/// absorption roadmap acknowledges now consult the same `&'static str`,
/// so a future Gateway API rebrand on the per-`HTTPRouteMatch` path-
/// matcher axis (an upstream Gateway API v2 rename to `pathMatch` /
/// `prefix` / `url`, coordinated with the upstream SIG-Network Gateway
/// API deprecation cycle) is a one-line edit on the canonical
/// [`caixa_core::GATEWAY_API_KEY_PATH`] declaration, not a coordinated
/// rewrite across this crate's `gateway_routes` renderer + every future
/// per-target renderer the substrate adds. The prior inline literal at
/// the one production emitter site would have let a Gateway-API-CRD
/// per-`HTTPRouteMatch` path-matcher-axis rebrand or a per-emitter typo
/// (`"pathMatch"` / `"prefix"` / `"url"`) silently emit an `HTTPRoute`
/// whose per-match path-selection axis the Gateway API CRD schema
/// validator drops as unknown — the per-match path predicate degrades
/// to the wildcard match at the gateway-class-controller's per-rule
/// reconcile, the rule matches every request path unconditionally, and
/// every external `:entrada` path filter the rule was authored to
/// enforce drops with no field naming the path-matcher-drift root
/// cause. Peer to the [`GATEWAY_API_KEY_MATCHES`] /
/// [`GATEWAY_API_KEY_BACKEND_REFS`] / [`GATEWAY_API_KEY_PARENT_REFS`]
/// re-exports on the sibling canonical-Gateway-API-HTTPRoute-body-
/// axis surface — nests the per-Gateway-API-HTTPRoute-per-rule-body-
/// axis canonical-string re-export set (`matches`, `backendRefs`,
/// `timeouts`, `retry`) one level deeper onto the per-`HTTPRouteMatch`
/// body-axis surface this crate's `gateway_routes` renderer's external
/// `:entrada` ingress contract rests on across the Gateway API
/// HTTPRoute-side per-match body-shape.
pub use caixa_core::GATEWAY_API_KEY_PATH;

/// Canonical K8s Gateway API v1 `HTTPPathMatch` scalar-payload axis key
/// every `gateway_routes`-emitted `HTTPRoute` per-match `path` block
/// mounts its per-match request-path-selection scalar payload under
/// (`spec.rules[].matches[].path.value`). Re-export of the canonical
/// [`caixa_core::GATEWAY_API_KEY_VALUE`] so the Gateway-API-
/// implementation-side per-`HTTPPathMatch` scalar-payload-axis-key
/// string lives in exactly one place across every caixa renderer —
/// caixa-mesh's `gateway_routes` per-Aplicacao `HTTPRoute` emitter
/// (the per-match `path_match.insert("value", …)` call the prior
/// inline `"value"` literal sat at, seeded from the Aplicacao's
/// `:entrada :paths` slot) and every future per-Gateway-API-side
/// renderer the M3.x absorption roadmap acknowledges now consult the
/// same `&'static str`, so a future Gateway API rebrand on the
/// per-`HTTPPathMatch` scalar-payload axis (an upstream Gateway API
/// v2 rename to `path` / `pattern` / `expression`, coordinated with
/// the upstream SIG-Network Gateway API deprecation cycle) is a
/// one-line edit on the canonical
/// [`caixa_core::GATEWAY_API_KEY_VALUE`] declaration, not a
/// coordinated rewrite across this crate's `gateway_routes` renderer
/// + every future per-target renderer the substrate adds. The prior
/// inline literal at the one production emitter site would have let
/// a Gateway-API-CRD per-`HTTPPathMatch`-`value`-axis rebrand or a
/// per-emitter typo (`"path"` / `"pattern"` / `"expression"`)
/// silently emit an `HTTPRoute` whose per-match request-path scalar
/// the Gateway API CRD schema validator drops as unknown — the
/// per-match path predicate degrades to the wildcard match at the
/// gateway-class-controller's per-rule reconcile, the rule matches
/// every request path unconditionally, and every external `:entrada`
/// path filter the rule was authored to enforce drops with no field
/// naming the `HTTPPathMatch`-scalar-payload-drift root cause. Peer
/// to the [`GATEWAY_API_KEY_PATH`] re-export on the sibling
/// canonical-Gateway-API-HTTPRoute-per-`HTTPRouteMatch`-body-axis
/// surface — nests the per-Gateway-API-HTTPRoute-per-match-body-axis
/// canonical-string re-export set (`path` container-axis, `value`
/// scalar-payload key) one level deeper onto the per-`HTTPPathMatch`
/// body-axis surface this crate's `gateway_routes` renderer's
/// external `:entrada` ingress contract rests on across the Gateway
/// API HTTPRoute-side per-match body-shape.
pub use caixa_core::GATEWAY_API_KEY_VALUE;

/// Canonical K8s Gateway API `Gateway` per-listener-set container-axis
/// key every `gateway_routes`-emitted `Gateway` document mounts its per-
/// Gateway `[{name, port, protocol, hostname}]` L7-listener fan-out
/// list under (`spec.listeners[]`). Re-export of the canonical
/// [`caixa_core::GATEWAY_API_KEY_LISTENERS`] so the Gateway-API-
/// implementation-side per-Gateway L7-listener-set-container-axis-key
/// string lives in exactly one place across every caixa renderer —
/// caixa-mesh's `gateway_routes` per-Aplicacao `Gateway` emitter (the
/// `g_spec.insert("listeners", …)` call the prior inline `"listeners"`
/// literal sat at) and every future per-Gateway-API-side renderer the
/// M3.x absorption roadmap acknowledges now consult the same
/// `&'static str`, so a future Gateway API rebrand on the per-Gateway
/// L7-listener-set axis (an upstream Gateway API v2 rename to
/// `servers` / `endpoints` / `bindings`, coordinated with the upstream
/// SIG-Network Gateway API deprecation cycle) is a one-line edit on
/// the canonical [`caixa_core::GATEWAY_API_KEY_LISTENERS`] declaration,
/// not a coordinated rewrite across this crate's `gateway_routes`
/// renderer + every future per-target renderer the substrate adds. The
/// prior inline literal at the one production emitter site + one test-
/// side fixture pin (`gateway_listener_carries_aplicacao_host`'s
/// `.get("listeners")` navigation) would have let a Gateway-API-CRD
/// per-Gateway L7-listener-set-axis rebrand or a per-emitter typo
/// (`"listener"` / `"listen"` / `"servers"`) silently emit a `Gateway`
/// whose L7-listener-set axis the Gateway API CRD schema validator
/// drops as unknown — no listener is opened, and every external
/// `:entrada` flow the Gateway was authored to accept drops at the
/// gateway-class-controller's per-Gateway reconcile with no field
/// naming the L7-listener-set-drift root cause. A drift on the test-
/// fixture side silently masks the emission-side pin (`.get("listeners")`
/// returns `None` under both the drifted-key emitter and the drifted-
/// key probe — the downstream `.and_then(|l| l.as_sequence())` /
/// `.and_then(|s| s.first())` chain short-circuits vacuously because
/// the outer per-Gateway L7-listener-set lookup is itself `None`).
/// Peer to the [`GATEWAY_API_KEY_PARENT_REFS`] +
/// [`GATEWAY_API_KEY_BACKEND_REFS`] re-exports on the sibling
/// canonical-Gateway-API-HTTPRoute-body-axis surface — extends the
/// per-Gateway-API-CRD-body-axis canonical-string re-export set
/// (`parentRefs`, `backendRefs`, `listeners`, future `hostnames`) this
/// crate's `gateway_routes` renderer's external `:entrada` ingress
/// contract rests on across the Gateway API CRD-side body-shape.
pub use caixa_core::GATEWAY_API_KEY_LISTENERS;

/// Canonical K8s Gateway API `Gateway` per-listener DNS-host-discriminator
/// axis key every `gateway_routes`-emitted `Gateway` document mounts each
/// listener's virtual-host filter under (`spec.listeners[].hostname`). Re-
/// export of the canonical [`caixa_core::GATEWAY_API_KEY_HOSTNAME`] so the
/// Gateway-API-implementation-side per-listener DNS-host-discriminator-
/// axis-key string lives in exactly one place across every caixa renderer
/// — caixa-mesh's `gateway_routes` per-Aplicacao `Gateway` emitter (the
/// per-listener `listener.insert("hostname", …)` call the prior inline
/// `"hostname"` literal sat at, seeded from the Aplicacao's `:entrada
/// :host` slot) and every future per-Gateway-API-side renderer the M3.x
/// absorption roadmap acknowledges now consult the same `&'static str`,
/// so a future Gateway API rebrand on the per-listener DNS-host
/// discriminator axis (an upstream Gateway API v2 rename to `host` /
/// `vhost` / `serverName`, coordinated with the upstream SIG-Network
/// Gateway API deprecation cycle) is a one-line edit on the canonical
/// [`caixa_core::GATEWAY_API_KEY_HOSTNAME`] declaration, not a
/// coordinated rewrite across this crate's `gateway_routes` renderer +
/// every future per-target renderer the substrate adds. The prior inline
/// literal at the one production emitter site + one test-side fixture
/// pin (`gateway_listener_carries_aplicacao_host`'s `.get("hostname")`
/// navigation) would have let a Gateway-API-CRD per-listener DNS-host-
/// discriminator-axis rebrand or a per-emitter typo (`"host"` /
/// `"vhost"` / `"serverName"`) silently emit a `Gateway` whose per-
/// listener virtual-host filter axis the Gateway API CRD schema
/// validator drops as unknown — the listener accepts traffic on the
/// wildcard host rather than the typed `:entrada :host` the Aplicacao
/// author declared, and every external `:entrada` flow the listener was
/// authored to accept lands on the wrong virtual-host filter with no
/// field naming the DNS-host-discriminator-drift root cause. A drift on
/// the test-fixture side silently masks the emission-side pin
/// (`.get("hostname")` returns `None` under both the drifted-key emitter
/// and the drifted-key probe — the downstream `.and_then(|h| h.as_str())`
/// chain short-circuits vacuously because the outer per-listener DNS-
/// host discriminator lookup is itself `None`). Peer to the
/// [`GATEWAY_API_KEY_LISTENERS`] +
/// [`GATEWAY_API_KEY_PARENT_REFS`] +
/// [`GATEWAY_API_KEY_BACKEND_REFS`] re-exports on the sibling
/// canonical-Gateway-API-CRD-body-axis surface — nests the per-Gateway-
/// API-CRD-body-axis canonical-string re-export set one level deeper
/// onto the per-listener body-axis surface (`parentRefs`, `backendRefs`,
/// `listeners`, `hostname`, future `hostnames`) this crate's
/// `gateway_routes` renderer's external `:entrada` ingress contract
/// rests on across the Gateway API CRD-side body-shape.
pub use caixa_core::GATEWAY_API_KEY_HOSTNAME;

/// Canonical K8s Gateway API `HTTPRoute` spec-level DNS-host-filter axis
/// key every `gateway_routes`-emitted `HTTPRoute` document mounts the
/// route's per-route virtual-host filter list under (`spec.hostnames[]`).
/// The plural sibling of [`GATEWAY_API_KEY_HOSTNAME`] — same
/// Gateway-API-CRD DNS-host-discriminator convention nested one level up
/// on the sibling `HTTPRoute` per-route body-axis surface, distinct
/// spelling (`hostnames` — plural — is the `HTTPRoute` spec-level filter
/// list; the singular `hostname` axis it pairs against is the per-
/// `Gateway`-listener virtual-host discriminator). Re-export of the
/// canonical [`caixa_core::GATEWAY_API_KEY_HOSTNAMES`] so the Gateway-
/// API-implementation-side per-route DNS-host-filter-axis-key string
/// lives in exactly one place across every caixa renderer — caixa-mesh's
/// `gateway_routes` per-Aplicacao `HTTPRoute` emitter (the spec-level
/// `r_spec.insert("hostnames", …)` call the prior inline `"hostnames"`
/// literal sat at, seeded from the Aplicacao's `:entrada :host` slot as
/// a single-element sequence) and every future per-Gateway-API-side
/// renderer the M3.x absorption roadmap acknowledges now consult the
/// same `&'static str`, so a future Gateway API rebrand on the per-route
/// DNS-host filter axis (an upstream Gateway API v2 rename to `hosts` /
/// `vhosts` / `serverNames`, coordinated with the upstream SIG-Network
/// Gateway API deprecation cycle) is a one-line edit on the canonical
/// [`caixa_core::GATEWAY_API_KEY_HOSTNAMES`] declaration, not a
/// coordinated rewrite across this crate's `gateway_routes` renderer +
/// every future per-target renderer the substrate adds. The prior inline
/// literal at the one production emitter site would have let a Gateway-
/// API-CRD per-route DNS-host-filter-axis rebrand or a per-emitter typo
/// (`"hosts"` / `"vhosts"` / `"serverNames"`) silently emit an
/// `HTTPRoute` whose per-route virtual-host filter axis the Gateway API
/// CRD schema validator drops as unknown — the route accepts traffic on
/// every host the parent Gateway's listener accepts rather than the
/// typed `:entrada :host` the Aplicacao author declared, and every
/// external `:entrada` flow the route was authored to accept lands on
/// the wildcard virtual-host filter with no field naming the DNS-host-
/// filter-drift root cause. Peer to the
/// [`GATEWAY_API_KEY_HOSTNAME`] +
/// [`GATEWAY_API_KEY_LISTENERS`] +
/// [`GATEWAY_API_KEY_PARENT_REFS`] +
/// [`GATEWAY_API_KEY_BACKEND_REFS`] re-exports on the sibling
/// canonical-Gateway-API-CRD-body-axis surface — closes the per-Gateway-
/// API-CRD `HTTPRoute` per-route body-axis re-export pair across the
/// singular / plural DNS-host discriminator surface (`hostname` at the
/// parent-Gateway per-listener discriminator + `hostnames` at the child
/// HTTPRoute per-route filter list), so both halves of the DNS-host-
/// discriminator convention across the `(Gateway, HTTPRoute)` pair this
/// crate's `gateway_routes` renderer's external `:entrada` ingress
/// contract emits together now carry one lifted canonical `&'static str`
/// re-export apiece.
pub use caixa_core::GATEWAY_API_KEY_HOSTNAMES;

/// Canonical K8s CR top-level `spec` key. Re-export of the canonical
/// [`caixa_core::KUBE_KEY_SPEC`] so the per-kind body key lives in
/// exactly one place across every caixa renderer — caixa-mesh's
/// `cilium_network_policies` per-`(:de, :para)` `CiliumNetworkPolicy`
/// emitter (the `endpointSelector` + `ingress` block under spec),
/// caixa-mesh's `gateway_routes` `Gateway` + `HTTPRoute` emitter (the
/// `listeners` / `rules` / `parentRefs` / `hostnames` block under
/// spec), and every future per-target renderer that materializes a CR
/// (the M4 `mesh.pleme.io/v1alpha1/Aplicacao` materializer's
/// per-policy spec block, the future per-Servico `ComputeUnit` schema
/// reroute) consults the same `&'static str`. The prior inline
/// `"spec".into()` literals at the three production-code call sites
/// in this crate would have let a typo / camelCase drift on any one
/// of the three sites silently emit a CR with no recognizable spec
/// (the apiserver-side CRD schema validator drops the malformed
/// document at apply time, naming the unrecognized key but not the
/// source-side renderer call site). Peer to the
/// [`GATEWAY_API_API_VERSION`] / [`CILIUM_API_VERSION`] re-exports on
/// the sibling canonical-K8s-API-axis surfaces.
pub use caixa_core::KUBE_KEY_SPEC;

/// Canonical K8s Gateway API `GatewayClass` name every
/// `gateway_routes`-emitted `Gateway` document declares at its
/// `spec.gatewayClassName` axis. Re-export of the canonical
/// [`caixa_core::DEFAULT_GATEWAY_CLASS_NAME`] so the substrate's chosen
/// Gateway API controller-discriminator lives in exactly one place
/// across every caixa renderer — caixa-mesh's `gateway_routes`
/// per-`:entrada` `Gateway` emitter (the single production-code site
/// the prior inline `"cilium".into()` literal sat at — the
/// `spec.gatewayClassName` field of the emitted `Gateway`'s `spec`
/// block) and every future per-Aplicacao materializer the M3.x + M4
/// absorption roadmap acknowledges (the future
/// `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's `Gateway`
/// synthesis, a future per-cluster / per-edge `Gateway` renderer for
/// non-HTTP `:entrada` shapes) now consult the same `&'static str`,
/// so a future substrate-side Gateway API controller migration
/// (Cilium → Envoy Gateway / Istio Gateway or any per-edition
/// Gateway API v1.x GA controller variant the SIG-Network roadmap
/// names) is a one-line edit on the canonical
/// [`caixa_core::DEFAULT_GATEWAY_CLASS_NAME`] declaration, not a
/// coordinated rewrite across this crate's `gateway_routes` call site
/// + every future per-target renderer the substrate adds.
///
/// The prior inline literal would have let a substrate-side controller
/// migration on the caixa-mesh side without a coordinated edit on the
/// matching in-file `gateway_gateway_class_name_uses_lifted_default_gateway_class_name`
/// test fixture pin silently emit a `Gateway` whose
/// `spec.gatewayClassName` referenced a `GatewayClass` no controller
/// reconciles — apply-side: the `Gateway` sits at `Programmed: False`
/// with every attached `HTTPRoute` unbound, every external `:entrada`
/// flow drops at the ingress with no field naming the controller-
/// drift root cause. And splitting the controller across renderers
/// would silently reintroduce the two-data-planes drift the mesh
/// composition "one identity layer, one data plane" invariant
/// (MESH-COMPOSITION.md §V) closes — the emitted `Gateway`'s
/// controller and the sibling `CiliumNetworkPolicy`'s controller
/// would land in distinct reconcilers, and the intra-mesh
/// identity-aware policy stops matching the ingress-side traffic at
/// the eBPF data plane. Peer to the [`DEFAULT_NAMESPACE`] re-export
/// on the sibling canonical-substrate-default-resource-name axis —
/// extends the discipline onto the canonical-Gateway-API-controller-
/// choice axis surface.
pub use caixa_core::DEFAULT_GATEWAY_CLASS_NAME;

/// Canonical K8s Gateway API `Gateway` per-Gateway controller-binding
/// scalar-axis key every `gateway_routes`-emitted `Gateway` document
/// mounts its per-Gateway `GatewayClass.metadata.name` reference
/// under (`spec.gatewayClassName`). Re-export of the canonical
/// [`caixa_core::GATEWAY_API_KEY_GATEWAY_CLASS_NAME`] so the
/// Gateway-API-implementation-side per-Gateway controller-binding
/// scalar-axis-key string lives in exactly one place across every
/// caixa renderer — caixa-mesh's `gateway_routes` per-Aplicacao
/// `Gateway` emitter (the `g_spec.insert("gatewayClassName", …)` call
/// the prior inline `"gatewayClassName"` literal sat at) and every
/// future per-Gateway-API-side renderer the M3.x absorption roadmap
/// acknowledges now consult the same `&'static str`, so a future
/// Gateway API rebrand on the per-Gateway controller-binding scalar
/// axis (an upstream Gateway API v2 rename to `className` /
/// `gatewayClassRef`, coordinated with the upstream SIG-Network
/// Gateway API deprecation cycle) is a one-line edit on the canonical
/// [`caixa_core::GATEWAY_API_KEY_GATEWAY_CLASS_NAME`] declaration,
/// not a coordinated rewrite across this crate's `gateway_routes`
/// renderer + every future per-target renderer the substrate adds.
/// The prior inline literal at the one production emitter site + one
/// test-side fixture pin
/// (`gateway_gateway_class_name_uses_lifted_default_gateway_class_name`'s
/// `.get("gatewayClassName")` navigation) would have let a Gateway-
/// API-CRD per-Gateway controller-binding-axis rebrand or a per-
/// emitter typo (`"gatewayClass"` / `"className"` /
/// `"gatewayClassRef"`) silently emit a `Gateway` whose controller-
/// binding scalar-axis the Gateway API CRD schema validator drops as
/// unknown — no `GatewayClass` is resolved, no `controllerName` is
/// looked up, and every external `:entrada` flow the Gateway was
/// authored to accept drops at the gateway-class-controller's per-
/// Gateway reconcile with no field naming the controller-binding-
/// drift root cause. A drift on the test-fixture side silently masks
/// the emission-side pin (`.get("gatewayClassName")` returns `None`
/// under both the drifted-key emitter and the drifted-key probe —
/// the downstream `.and_then(|c| c.as_str())` chain short-circuits
/// vacuously because the outer per-Gateway controller-binding lookup
/// is itself `None`). Peer to the [`GATEWAY_API_KEY_LISTENERS`] +
/// [`GATEWAY_API_KEY_HOSTNAME`] re-exports on the sibling canonical-
/// Gateway-API-CRD-per-Gateway-body-axis surface. Sibling of the
/// peer [`DEFAULT_GATEWAY_CLASS_NAME`] re-export on the canonical-
/// Gateway-API-`(key, value)`-pair-lift surface this re-export
/// closes the KEY half of.
pub use caixa_core::GATEWAY_API_KEY_GATEWAY_CLASS_NAME;

/// Canonical K8s CR top-level `metadata` key. Re-export of the canonical
/// [`caixa_core::KUBE_KEY_METADATA`] so the per-kind metadata block key
/// lives in exactly one place across every caixa renderer — caixa-mesh's
/// `cilium_network_policies` per-`(:de, :para)` `CiliumNetworkPolicy`
/// emitter (the `metadata.{name, namespace, labels}` block every policy
/// carries) and `gateway_routes` `Gateway` + `HTTPRoute` emitter (the
/// `metadata.{name, namespace}` block each doc carries) now consult the
/// same `&'static str` as the peer caixa-flux renderer's
/// `KUBE_KEY_METADATA` re-export. The prior inline `"metadata"`
/// literals at every drift-detection / policy-traversal test-side site
/// in this crate would have let a typo on any one site (e.g. `"Metadata"`,
/// `"meta-data"`, `"medadata"`) silently miss the per-CNP / per-Gateway
/// / per-HTTPRoute metadata retrieval — the equality assertion would
/// then compare `None` against `Some("checkout")` rather than the
/// expected label value; the lift routes every K8s-CR-top-level-
/// metadata-axis retrieval through the same `&'static str` so drift
/// between any two sites becomes a single-edit fix at the caixa-core
/// const definition. Same shape as the [`KUBE_KEY_SPEC`] re-export on
/// the sibling K8s-CR top-level-spec-axis.
pub use caixa_core::KUBE_KEY_METADATA;

/// Canonical K8s CR top-level `kind` discriminator key. Re-export of the
/// canonical [`caixa_core::KUBE_KEY_KIND`] so the per-CR-kind-axis
/// retrieval key lives in exactly one place across every caixa renderer
/// — caixa-mesh's `cilium_network_policies` + `gateway_routes` +
/// `render_all` test-side `(:kind, :apiVersion)` CRD-lookup-tuple
/// traversal predicates (every `docs.iter().find(|d| d.get("kind")…)` +
/// `for p in &policies { p.get("kind")… }` filter that separates the
/// rendered `Gateway` / `HTTPRoute` / `CiliumNetworkPolicy` documents
/// inside the multi-doc sequence the `gateway_routes` / `render_all`
/// emitters return) now consult the same `&'static str` as the peer
/// caixa-core-side `kube_resource_skeleton` production emitter (which
/// already inserts [`KUBE_KEY_KIND`] under caixa-core/src/render.rs:7181
/// on the [`caixa_core::KUBE_KEY_API_VERSION`] + [`KUBE_KEY_KIND`]
/// axis pair every rendered CR carries). The prior inline `"kind"`
/// literals at every drift-detection / policy-traversal / render-
/// determinism test-side site in this crate would have let a typo on
/// any one site (e.g. `"Kind"`, `"kinds"`, `"knid"`) silently miss the
/// per-CR kind-axis retrieval — the equality assertion would then
/// compare `None` against `Some("CiliumNetworkPolicy")` / `Some("Gateway")`
/// / `Some("HTTPRoute")` rather than the expected kind discriminator,
/// and the `docs.iter().find(|d| d.get(…) == Some(…))` predicate would
/// silently miss the per-kind document inside the multi-doc sequence
/// (the `.expect("Gateway present")` unwrap that names the offending
/// axis would fire instead of the intended assertion, masking the true
/// drift). The lift routes every K8s-CR-top-level-kind-axis retrieval
/// through the same `&'static str` so drift between any two sites
/// becomes a single-edit fix at the caixa-core const definition. Same
/// shape as the [`KUBE_KEY_SPEC`] + [`KUBE_KEY_METADATA`] re-exports on
/// the sibling K8s-CR top-level-spec / top-level-metadata axes —
/// completes the per-K8s-CR top-level `(apiVersion, kind, metadata,
/// spec)` axis re-export set on the `kind` half, which every downstream
/// `docs.iter().find(|d| d.get(KUBE_KEY_KIND)…)` predicate the multi-doc
/// `render_all` sequence-consumer needs to distinguish the emitted
/// `Cilium` / `Gateway` / `HTTPRoute` documents by rests on.
pub use caixa_core::KUBE_KEY_KIND;

/// Canonical K8s CR top-level `apiVersion` key. Re-export of the
/// canonical [`caixa_core::KUBE_KEY_API_VERSION`] so the per-CR-
/// apiVersion-axis retrieval key lives in exactly one place across
/// every caixa renderer — caixa-mesh's `cilium_network_policies` +
/// `gateway_routes` test-side `(:kind, :apiVersion)` CRD-lookup-tuple
/// pins (every `p.get("apiVersion")` / `gateway.get("apiVersion")` /
/// `route.get("apiVersion")` retrieval that traverses the multi-doc
/// sequence the `gateway_routes` / `cilium_network_policies` emitters
/// return to assert the top-level `apiVersion` axis on each per-CNP /
/// per-`Gateway` / per-`HTTPRoute` document binds to the lifted
/// [`CILIUM_API_VERSION`] / [`GATEWAY_API_API_VERSION`] CRD-group/
/// version) now consult the same `&'static str` as the peer
/// caixa-core-side `kube_resource_skeleton` production emitter (which
/// already inserts [`KUBE_KEY_API_VERSION`] under
/// caixa-core/src/render.rs:7177 on the [`KUBE_KEY_API_VERSION`] +
/// [`KUBE_KEY_KIND`] axis pair every rendered CR carries). The prior
/// inline `"apiVersion"` literals at every drift-detection / CRD-
/// group-version pin test-side site in this crate would have let a
/// typo on any one site (e.g. `"ApiVersion"`, `"api-version"`,
/// `"apiVerison"`) silently miss the per-CR apiVersion retrieval —
/// the equality assertion would then compare `None` against
/// `Some("cilium.io/v2")` / `Some("gateway.networking.k8s.io/v1")`
/// rather than the expected CRD-group/version string, masking the
/// true sibling [`CILIUM_API_VERSION`] / [`GATEWAY_API_API_VERSION`]
/// axis drift. The lift routes every K8s-CR-top-level-apiVersion-
/// axis retrieval through the same `&'static str` so drift between
/// any two sites becomes a single-edit fix at the caixa-core const
/// definition. Same shape as the [`KUBE_KEY_SPEC`] + [`KUBE_KEY_METADATA`]
/// + [`KUBE_KEY_KIND`] re-exports on the sibling K8s-CR top-level-
/// spec / top-level-metadata / top-level-kind axes — completes the
/// per-K8s-CR top-level `(apiVersion, kind, metadata, spec)` axis
/// re-export quartet on the `apiVersion` half, which every drift-
/// detection pin on the sibling controller-triplet-CRD-group/
/// version axis (`CILIUM_API_VERSION` for Cilium, `GATEWAY_API_API_VERSION`
/// for Gateway API `Gateway` + `HTTPRoute`) rests on. Peer to
/// `caixa_flux::KUBE_KEY_API_VERSION` (e0555d6) on the sibling
/// renderer crate — extends the discipline from the Flux v2
/// controller-triplet drift-detection pins onto the Cilium + Gateway
/// API controller-pair drift-detection pins in this crate.
pub use caixa_core::KUBE_KEY_API_VERSION;

/// Canonical K8s CR `metadata.namespace` nested-axis key. Re-export of
/// the canonical [`caixa_core::KUBE_KEY_NAMESPACE`] so the per-CR
/// namespace-axis retrieval key lives in exactly one place across
/// every caixa renderer — caixa-mesh's `cilium_policy_carries_canonical_kube_skeleton`
/// / `gateway_carries_canonical_kube_skeleton_without_labels` /
/// `cilium_policy_metadata_block_iterates_alphabetically` test-side
/// `metadata.namespace` retrievals + alphabetical-iteration determinism
/// pin (the three inline `"namespace"` sites this crate's rendered
/// multi-doc mesh bundle's per-CR `metadata.{name, namespace, labels}`
/// / `metadata.{name, namespace}` block traversal navigates) now
/// consult the same `&'static str` as the peer caixa-core-side
/// `kube_resource_skeleton` production emitter (which already inserts
/// [`KUBE_KEY_NAMESPACE`] under caixa-core/src/render.rs:9019 on the
/// per-CR metadata block every rendered mesh bundle document carries).
/// The prior inline `"namespace"` literals at every drift-detection
/// / render-determinism test-side site in this crate would have let a
/// typo on any one site (e.g. `"Namespace"`, `"name space"`, the
/// canonical transposition `"namesapce"`) silently miss the per-CR
/// metadata.namespace retrieval — the equality assertion would then
/// compare `None` against `Some(DEFAULT_NAMESPACE)` rather than the
/// expected namespace value, masking the true sibling
/// [`DEFAULT_NAMESPACE`] axis drift; the alphabetical-iteration
/// determinism pin's `vec!["labels", "name", "namespace"]` fixture
/// would compare against the actually-iterated key sequence and fire
/// on the drifted-fixture rather than the true render-determinism
/// property. The lift routes every K8s-CR-metadata-namespace-axis
/// retrieval + fixture through the same `&'static str` so drift
/// between any two sites becomes a single-edit fix at the caixa-core
/// const definition. Peer to `caixa_flux::KUBE_KEY_NAMESPACE`
/// (44bebfe) on the sibling renderer crate — extends the discipline
/// from the Flux v2 controller-triplet + ComputeUnit-side
/// metadata.namespace drift-detection pins onto the Cilium + Gateway
/// API controller-pair metadata.namespace drift-detection pins in
/// this crate. Extends the per-K8s-CR top-level `(apiVersion, kind,
/// metadata, spec)` axis re-export quartet onto the load-bearing
/// nested `metadata.namespace` axis — the axis every rendered
/// `CiliumNetworkPolicy` / `Gateway` / `HTTPRoute` document binds to
/// on the deploy path (the Cilium operator's per-CNP
/// `endpointSelector` matches pods in this namespace; the
/// gateway-class-controller's per-`Gateway` listener attaches only to
/// HTTPRoutes in this namespace; every apiserver-side CR admission-
/// time schema validates against it) so exactly one canonical
/// byte-sequence must reach every rendered artifact.
pub use caixa_core::KUBE_KEY_NAMESPACE;

/// Canonical K8s CR `metadata.labels` nested-axis key. Re-export of the
/// canonical [`caixa_core::KUBE_KEY_LABELS`] so the per-CR labels-axis
/// retrieval key lives in exactly one place across every caixa renderer
/// — caixa-mesh's `cilium_policy_metadata_labels_use_lifted_consts`
/// test-side retrieval of the per-CNP `metadata.labels` mapping (the
/// LABEL_APLICACAO + LABEL_CONTRATO drift-detection pin's entry point),
/// caixa-mesh's `cilium_policy_carries_canonical_kube_skeleton` +
/// `gateway_carries_canonical_kube_skeleton_without_labels` +
/// `httproute_carries_canonical_kube_skeleton_without_labels` per-CR
/// metadata-block `.get("labels")` probes (the presence-of-labels /
/// empty-labels-skip semantic pins on `CiliumNetworkPolicy` /
/// `Gateway` / `HTTPRoute`), and the
/// `cilium_policy_metadata_block_iterates_alphabetically` render-
/// determinism-contract fixture (the alphabetical-iteration `vec!["labels",
/// "name", KUBE_KEY_NAMESPACE]` fixture whose first entry the alphabetical-
/// key-ordering `metadata:` block emission pins). The prior five inline
/// `"labels"` literals at every drift-detection / render-determinism
/// test-side site in this crate would have let a typo on any one site
/// (e.g. `"Labels"`, `"lables"`, the canonical transposition `"lablels"`)
/// silently miss the per-CR metadata.labels retrieval — the
/// `.get("labels")` chain would then return `None` and the trailing
/// `.expect("policy metadata.labels mapping")` would panic with the
/// mapping-shape message, masking the true label-key drift, or the
/// presence-of-labels / empty-labels-skip semantic pins would compare
/// `Some(...)`/`None` under the wrong retrieval so the empty-labels-skip
/// contract's true drift never surfaces, or the alphabetical-iteration
/// render-determinism fixture would fire on the drifted-fixture rather
/// than the true render-determinism property. The lift routes every K8s-
/// CR-metadata-labels-axis retrieval + fixture through the same
/// `&'static str` so drift between any two sites becomes a single-edit
/// fix at the caixa-core const definition. Extends the per-K8s-CR
/// top-level `(apiVersion, kind, metadata, spec)` axis re-export
/// quartet + the load-bearing nested `metadata.namespace` axis onto
/// the load-bearing nested `metadata.labels` axis — the axis every
/// rendered `CiliumNetworkPolicy` document carries at the `pleme.pleme.io/
/// aplicacao` + `pleme.pleme.io/contrato` grouping key (the Hubble flow-
/// grouping / operator-policy-filter selection axis every consumer of
/// the rendered mesh bundle keys off) so exactly one canonical byte-
/// sequence must reach every rendered artifact.
pub use caixa_core::KUBE_KEY_LABELS;

/// Canonical K8s CR `metadata.name` nested-axis key. Re-export of the
/// canonical [`caixa_core::KUBE_KEY_NAME`] so the per-CR name-axis
/// retrieval key lives in exactly one place across every caixa renderer
/// — caixa-mesh's `cilium_policy_carries_canonical_kube_skeleton` +
/// `gateway_carries_canonical_kube_skeleton_without_labels` +
/// `httproute_carries_canonical_kube_skeleton_without_labels` per-CR
/// metadata-block `.get("name")` retrievals (the presence + equality
/// pins on `CiliumNetworkPolicy` / `Gateway` / `HTTPRoute`), the six
/// per-CNP `metadata.name`-axis lookup navigations across the
/// `cilium_policy_metadata_names_span_all_edges` /
/// `cilium_fans_same_de_para_edges_into_one_policy` /
/// `cilium_http_contracts_emit_l7_rules` /
/// `cilium_pubsub_contracts_skip_l7_rules` /
/// `cnp_l4_fallback_port_routes_through_lifted_default_servico_port` /
/// `cilium_mtls_required_contract_emits_authentication_required`
/// test-side `policies.iter().find(|p| p.get(KUBE_KEY_METADATA)
/// .and_then(|m| m.get(KUBE_KEY_NAME)))` filters (the per-CNP
/// `<aplicacao>-<de>-to-<para>` metadata.name binding that names every
/// `CiliumNetworkPolicy` document the per-`(:de, :para)` fan-out emits),
/// and the `cilium_policy_metadata_block_iterates_alphabetically`
/// render-determinism-contract fixture (the alphabetical-iteration
/// `vec![KUBE_KEY_LABELS, "name", KUBE_KEY_NAMESPACE]` fixture whose
/// middle entry the alphabetical-key-ordering `metadata:` block
/// emission pins). The prior ten inline `"name"` literals at every
/// drift-detection / per-CNP-lookup / render-determinism test-side site
/// in this crate would have let a typo on any one site (e.g. `"Name"`,
/// `"nmae"`, the canonical transposition `"naem"`) silently miss the
/// per-CR metadata.name retrieval — the `.get("name")` chain would then
/// return `None` under the presence pin so the true metadata-name-axis
/// drift never surfaces, or compare `Some(<other>)` against the
/// expected caixa name/route name under the equality pins so the
/// caixa-nome → metadata-name binding's true drift is masked, or slip
/// past the per-CNP metadata.name filter under the six per-`(:de, :para)`
/// lookup navigations so the true policy-identity → edge-shape binding
/// under fan-in / L7-emission / L4-fallback / mTLS-authentication drift
/// never surfaces (each `.find(|p| p.get(KUBE_KEY_METADATA)
/// .and_then(|m| m.get("name")))` chain would silently return
/// `.unwrap()`-panicking `None` on the first per-CNP lookup or match
/// the wrong policy under the equality-comparison filter, masking the
/// true fan-in / L7-rule / L4-port / mTLS-authentication mode
/// property), or trip the alphabetical-iteration determinism fixture
/// against the drifted-fixture rather than the true render-determinism
/// property. The lift routes every K8s-CR-metadata-name-axis retrieval
/// + fixture through the same `&'static str` so drift between any two
/// sites becomes a single-edit fix at the caixa-core const definition.
/// Completes the K8s-CR metadata-block axis triplet `(name, namespace, labels)`
/// under a single canonical `caixa-core::KUBE_KEY_*` re-export shape
/// in this crate — the peer `KUBE_KEY_NAMESPACE` (ae34889) and
/// `KUBE_KEY_LABELS` (aa2d105) sweeps established the discipline; this
/// lift extends it onto the last remaining metadata-nested axis. The
/// per-K8s-CR top-level `(apiVersion, kind, metadata, spec)` axis
/// re-export quartet + the load-bearing nested `metadata.namespace` +
/// `metadata.labels` axes now extend onto the load-bearing nested
/// `metadata.name` axis — the axis every rendered `CiliumNetworkPolicy`
/// / `Gateway` / `HTTPRoute` document binds to on the deploy path (the
/// apiserver's CR admission-time schema keys the object identity off
/// it; every `kubectl get`/GC/finalizer navigates the same key; the
/// Cilium operator's per-CNP status-update path and the gateway-class-
/// controller's per-`Gateway` listener-attach navigate the same axis)
/// so exactly one canonical byte-sequence must reach every rendered
/// artifact.
pub use caixa_core::KUBE_KEY_NAME;

/// Canonical K8s `LabelSelector.matchLabels` nested-axis key. Re-export
/// of the canonical [`caixa_core::KUBE_KEY_MATCH_LABELS`] so the per-CR
/// selector-axis retrieval key lives in exactly one place across every
/// caixa renderer — caixa-mesh's `cilium_policies_are_identity_based`
/// (the `endpointSelector.matchLabels` presence pin + the
/// `ingress[0].fromEndpoints[0].matchLabels` two-axis-selector pin
/// that check the `pleme.pleme.io/program` + `pleme.pleme.io/aplicacao`
/// identity keys the Cilium data plane matches on), the
/// `cilium_endpoint_selector_is_program_only` destination-selector-axis
/// pin (single-axis `LABEL_PROGRAM`-only selector — the
/// destination-`endpointSelector.matchLabels` retrieval whose
/// `selector.len() == 1` assertion pins the program-only semantic the
/// canonical `pleme_program_selector` helper emits), and the
/// `cilium_from_endpoints_carries_aplicacao_scoped_selector` source-
/// selector-axis pin (two-axis `LABEL_PROGRAM` + `LABEL_APLICACAO`
/// selector — the source-`fromEndpoints[0].matchLabels` retrieval
/// whose `from.len() == 2` assertion pins the
/// program-in-Aplicacao-scoped semantic the canonical
/// `pleme_program_in_aplicacao_selector` helper emits, guarding the
/// safety property that a same-named program in a different
/// Aplicacao cannot satisfy the policy's ingress rule) now consult
/// the same `&'static str` as the peer caixa-core-side
/// `label_selector` production emitter (which already inserts
/// [`KUBE_KEY_MATCH_LABELS`] under caixa-core/src/render.rs:7112 on
/// every `{matchLabels: <mapping>}` envelope the typed selector
/// helpers emit). The prior four inline `"matchLabels"` literals at
/// every drift-detection / selector-axis test-side site in this
/// crate would have let a typo on any one site (e.g. `"MatchLabels"`,
/// `"match_labels"`, `"match-labels"`, the canonical camelCase-drift
/// `"matchlabels"` — the K8s apiserver's OpenAPI v3 schema property
/// name is strict camelCase `matchLabels`) silently miss the per-CR
/// selector-mapping retrieval — the `.get("matchLabels")` chain
/// would then return `None` under the presence pin so the true
/// selector-axis drift never surfaces, or the surrounding
/// `.expect("endpointSelector.matchLabels mapping")` /
/// `.expect("fromEndpoints[0].matchLabels mapping")` panic-message
/// tag would fire with the mapping-shape message rather than the
/// true selector-key drift, or the `selector.len() == 1` /
/// `from.len() == 2` axis-count assertion would compare against the
/// wrong retrieval so the destination-program-only / source-program-
/// in-Aplicacao selector-shape contract's true drift is masked. The
/// lift routes every K8s-`LabelSelector.matchLabels`-axis retrieval
/// through the same `&'static str` so drift between any two sites
/// becomes a single-edit fix at the caixa-core const definition.
/// Extends the per-K8s-CR top-level `(apiVersion, kind, metadata,
/// spec)` axis re-export quartet + the load-bearing nested
/// `metadata.{name, namespace, labels}` triplet onto the load-bearing
/// nested `LabelSelector.matchLabels` axis — the equality-projection
/// axis every rendered `CiliumNetworkPolicy` document carries at both
/// `spec.endpointSelector.matchLabels` (the destination-identity
/// selector the Cilium data plane matches pod-identity keys against)
/// and `spec.ingress[*].fromEndpoints[*].matchLabels` (the source-
/// identity selector the same data plane checks on the admitted-
/// source side). Peer to the sibling load-bearing nested
/// `LabelSelector.matchLabels` axis re-exports every downstream
/// consumer of the rendered mesh bundle keys off (the Cilium
/// operator's per-CNP `endpointSelector` and per-ingress-rule
/// `fromEndpoints` navigate the same K8s-`LabelSelector`-schema
/// projection).
pub use caixa_core::KUBE_KEY_MATCH_LABELS;

/// Canonical K8s CR `rules` collection-axis key. Re-export of the
/// canonical [`caixa_core::KUBE_KEY_RULES`] so the per-CR rule-list
/// container key lives in exactly one place across every caixa
/// renderer — this crate's two production-code emitters
/// (`cilium_network_policies`'s per-`toPorts[]` `rules:` L7 rule-list
/// mapping the Cilium data plane dispatches HTTP / Kafka / DNS L7
/// rules under, `gateway_routes`'s `HTTPRoute` `spec.rules[]` sequence
/// the gateway-class-controller dispatches per-rule `matches[]` +
/// `backendRefs[]` + timeouts / retries overlay under) and this crate's
/// five test-side rule-list traversal sites (the
/// `httproute_carries_paths_from_http_endpoints` `.get("rules")` under
/// `toPorts[]` L7-path-content pin, the `cilium_l7_rules_are_http_only`
/// `.get("rules")` under `toPorts[]` L7-http-only-shape pin, the
/// `cilium_pubsub_contracts_skip_l7_rules` `to_ports.get("rules").is_none()`
/// pubsub-contracts-carry-no-L7-rules absence pin, the
/// `gateway_emits_gateway_plus_httproute_pair` `.get("rules")` under
/// `spec` HTTPRoute-backendRefs-shape pin, and the `httproute_rules`
/// test-fixture helper `.get("rules")` under `spec` HTTPRoute-rule-
/// sequence retrieval every downstream policy-timeout / retries /
/// mtls / rate-limit determinism pin reaches through) now consult the
/// same `&'static str` as the peer caixa-core-side const definition.
///
/// The prior inline `"rules"` literals at the two production emitter
/// sites + five test-side retrieval sites in this crate would have let
/// a typo on any one site (e.g. `"Rules"`, `"rule"`, `"ruleset"`, the
/// canonical `HTTPRoute.spec.rules` vs Cilium `toPorts[].rules` cross-
/// context transposition where a maintainer replaces one axis's key
/// with the other's spelling mid-edit) silently miss the per-CR rule-
/// list retrieval — the presence pin's `.expect("HTTPRoute spec.rules
/// sequence")` panic-message tag would fire against the mapping-shape
/// message rather than the true rule-list-key drift, or the
/// `is_none()` absence pin (`cilium_pubsub_contracts_skip_l7_rules`)
/// would fire on the drifted retrieval so the L7-rules-absent-on-
/// pubsub-contracts contract's true drift is masked. On the production
/// side, a drift to `"Rules"` at either emitter site would silently
/// emit a CR whose rule-list container the apiserver-side CRD schema
/// validator drops as unrecognized at apply time (the Cilium operator's
/// per-CNP L7 dispatch pass would silently no-op every rule on the
/// affected `toPorts[]`; the gateway-class-controller's per-HTTPRoute
/// rule-dispatch pass would silently no-op every match/backend rule on
/// the affected route) with no field naming the rule-list-key-drift
/// root cause. The lift routes every K8s-CR-rules-axis retrieval +
/// emission through the same `&'static str` so drift between any two
/// sites becomes a single-edit fix at the caixa-core const definition.
///
/// Same shape as the [`KUBE_KEY_MATCH_LABELS`] re-export on the
/// sibling nested-selector-projection axis — extends the per-K8s-CR
/// top-level `(apiVersion, kind, metadata, spec)` axis re-export
/// quartet + the load-bearing nested `metadata.{name, namespace,
/// labels}` triplet + the load-bearing nested
/// `LabelSelector.matchLabels` selector-projection axis onto the
/// load-bearing nested `spec.rules[]` / `toPorts[].rules`
/// rule-list-container axis every downstream L7-policy /
/// HTTPRoute-rule-dispatch consumer of the rendered mesh bundle keys
/// off. Peer to the sibling load-bearing K8s-CR-schema-axis re-exports
/// every downstream apiserver-side CRD-schema-validator navigates the
/// same rule-list container axis on (the Cilium operator's per-CNP
/// L7 dispatch pass under `spec.ingress[].toPorts[].rules.http[]`, the
/// gateway-class-controller's per-HTTPRoute rule-dispatch pass under
/// `spec.rules[].matches[]` + `spec.rules[].backendRefs[]`).
pub use caixa_core::KUBE_KEY_RULES;

/// Canonical K8s CR L4-port scalar-axis key. Re-export of the
/// canonical [`caixa_core::KUBE_KEY_PORT`] so the per-CR L4-port
/// scalar field name lives in exactly one place across every caixa
/// renderer — this crate's three production-code emission sites
/// (`cilium_network_policies`'s per-`toPorts[].ports[]` port-tuple
/// `port:` scalar the Cilium data plane's per-tuple bpf policy
/// dispatch loop compares against the observed TCP/UDP L4 header
/// port value, `gateway_routes`'s per-`Gateway` per-listener
/// `spec.listeners[].port` scalar the gateway-class-controller's
/// per-listener bind loop opens the listener socket on,
/// `gateway_routes`'s per-`HTTPRoute` per-rule
/// `spec.rules[].backendRefs[].port` scalar the gateway-class-
/// controller's per-rule backend-dispatch loop forwards the matched
/// request to on the resolved Service / ExternalName backend) and
/// this crate's two test-side L4-port traversal sites (the
/// `cilium_l4_ports_default_to_servico_port` `.get("port")` under
/// `toPorts[].ports[]` L7-fallback-port-content pin threading through
/// [`DEFAULT_SERVICO_PORT`], the `gateway_emits_gateway_plus_httproute_pair`
/// `.get("port")` under `backendRefs[]` HTTPRoute-backend-port-content
/// pin) now consult the same `&'static str` as the peer caixa-core-side
/// const definition.
///
/// The prior inline `"port"` literals at the three production
/// emitter sites + two test-side retrieval sites in this crate would
/// have let a typo on any one site (e.g. `"Port"`, `"portNumber"`,
/// `"portValue"`, the canonical K8s port-value axis vs the K8s
/// Service `targetPort` L4-forwarding-destination axis cross-context
/// transposition where a maintainer replaces the port-value axis's
/// key with the forwarding-destination axis's spelling mid-edit)
/// silently miss the per-CR L4-port retrieval or emit a malformed CR
/// whose port field the apiserver-side CRD schema validator drops as
/// unrecognized at apply time — the Cilium operator's per-CNP L4
/// per-tuple bpf policy dispatch loop would silently accept every
/// L4 packet on the drifted `toPorts[].ports[]` entry regardless of
/// port match (bpf policy no-op on unrecognized port field), the
/// gateway-class-controller's per-listener bind loop would silently
/// fall back to a null listener socket (no bind, no L7 traffic
/// admitted), and the gateway-class-controller's per-rule backend-
/// dispatch loop would silently fall back to the K8s Service's
/// default target port (which may bind a different Servico's L4
/// port, silently routing traffic to the wrong backend) with no
/// field naming the L4-port-key-drift root cause. The lift routes
/// every K8s-CR-L4-port-scalar-axis retrieval + emission through the
/// same `&'static str` so drift between any two sites becomes a
/// single-edit fix at the caixa-core const definition.
///
/// Same shape as the [`KUBE_KEY_RULES`] re-export on the sibling
/// nested-rule-list-container axis — extends the per-K8s-CR top-
/// level `(apiVersion, kind, metadata, spec)` axis re-export quartet
/// + the load-bearing nested `metadata.{name, namespace, labels}`
/// triplet + the load-bearing nested `LabelSelector.matchLabels`
/// selector-projection axis + the load-bearing nested `spec.rules[]`
/// / `toPorts[].rules` rule-list-container axis onto the load-
/// bearing nested L4-port-scalar axis every downstream bpf-policy-
/// dispatch / gateway-listener-bind / gateway-backend-dispatch
/// consumer of the rendered mesh bundle keys off. Peer to the
/// sibling load-bearing K8s-CR-schema-axis re-exports every
/// downstream apiserver-side CRD-schema-validator navigates the same
/// L4-port scalar axis on (the Cilium operator's per-CNP L4 dispatch
/// pass under `spec.ingress[].toPorts[].ports[].port`, the gateway-
/// class-controller's per-Gateway per-listener bind pass under
/// `spec.listeners[].port`, the gateway-class-controller's per-
/// HTTPRoute per-rule backend-dispatch pass under
/// `spec.rules[].backendRefs[].port`).
pub use caixa_core::KUBE_KEY_PORT;

/// Canonical K8s CR L4/L7 protocol scalar-discriminator-axis key.
/// Re-export of the canonical [`caixa_core::KUBE_KEY_PROTOCOL`] so
/// the per-CR protocol scalar-discriminator field name lives in
/// exactly one place across every caixa renderer — this crate's two
/// production-code emission sites (`cilium_network_policies`'s
/// per-`toPorts[].ports[]` port-tuple `protocol:` scalar the Cilium
/// data plane's per-tuple bpf policy dispatch loop compares against
/// the observed L4 header protocol before applying the port match,
/// `gateway_routes`'s per-`Gateway` per-listener
/// `spec.listeners[].protocol` scalar the gateway-class-controller's
/// per-listener bind loop selects the L7 parser + TLS termination
/// strategy from) and this crate's one test-side protocol-scalar
/// traversal site (the `gateway_emits_gateway_plus_httproute_pair`
/// `.get("protocol")` retrieval on the emitted `Gateway`'s first
/// listener pinning the canonical `HTTP` listener-protocol content)
/// now consult the same `&'static str` as the peer caixa-core-side
/// const definition.
///
/// The prior inline `"protocol"` literals at the two production
/// emitter sites + one test-side retrieval site in this crate would
/// have let a typo on any one site (e.g. `"Protocol"`, `"proto"`,
/// `"transportProtocol"`) silently miss the per-CR protocol
/// discrimination or emit a malformed CR whose protocol field the
/// apiserver-side CRD schema validator drops as unrecognized at
/// apply time — the Cilium data plane's per-tuple bpf policy
/// dispatch loop would silently fall back to the CRD default
/// protocol `ANY` (admitting UDP traffic through a TCP-only rule
/// with no diagnostic), the gateway-class-controller's per-listener
/// bind loop would silently fail listener validation on a required
/// protocol field (rejecting the entire `Gateway` object at
/// admission time, no L7 traffic admitted, with the error message
/// naming the missing field rather than the drifted key that caused
/// the omission), and the test-side pin's `assert_eq!(…, Some("HTTP"))`
/// would silently unwrap to `None` under the drifted retrieval. The
/// lift routes every K8s-CR-protocol-scalar-axis retrieval + emission
/// through the same `&'static str` so drift between any two sites
/// becomes a single-edit fix at the caixa-core const definition.
///
/// Same shape as the [`KUBE_KEY_PORT`] re-export on the sibling
/// L4-port-scalar axis — extends the per-K8s-CR top-level
/// `(apiVersion, kind, metadata, spec)` axis re-export quartet +
/// the load-bearing nested `metadata.{name, namespace, labels}`
/// triplet + the load-bearing nested `LabelSelector.matchLabels`
/// selector-projection axis + the load-bearing nested `spec.rules[]`
/// / `toPorts[].rules` rule-list-container axis + the load-bearing
/// nested L4-port-scalar axis onto the load-bearing nested
/// L4/L7-protocol-scalar-discriminator axis every downstream bpf-
/// policy-dispatch / gateway-listener-bind consumer of the rendered
/// mesh bundle keys off before it can commit to a port match or a
/// listener parser. Peer to the sibling load-bearing K8s-CR-schema-
/// axis re-exports every downstream apiserver-side CRD-schema-
/// validator navigates the same protocol scalar axis on (the Cilium
/// operator's per-CNP L4 dispatch pass under
/// `spec.ingress[].toPorts[].ports[].protocol`, the gateway-class-
/// controller's per-Gateway per-listener bind pass under
/// `spec.listeners[].protocol`).
pub use caixa_core::KUBE_KEY_PROTOCOL;

/// Canonical K8s CR discriminated-union `type` scalar-discriminator-
/// axis key. Re-export of the canonical [`caixa_core::KUBE_KEY_TYPE`]
/// so the per-CR discriminated-union type scalar-discriminator field
/// name lives in exactly one place across every caixa renderer — this
/// crate's one production-code emission site (`gateway_routes`'s per-
/// rule per-`HTTPRouteMatch` `spec.rules[].matches[].path.type` scalar
/// the gateway-class-controller's per-rule L7 dispatch pass selects
/// the path-match strategy from) and this crate's test-side traversal
/// sites navigating the rendered `HTTPRoute`'s per-match path-selection-
/// predicate discriminator now consult the same `&'static str` as the
/// peer caixa-core-side const definition. Pairs with the sibling
/// [`GATEWAY_API_PATH_MATCH_TYPE_PATH_PREFIX`] re-export on the per-
/// `HTTPRouteMatch` path-selection-predicate discriminator scalar-VALUE
/// axis the discriminator scalar-KEY here holds under, closing the
/// per-`HTTPRouteMatch` path-selection-predicate `(type key →
/// PathPrefix value)` scalar-key/scalar-value discriminator axis pair
/// this crate's `gateway_routes` renderer's external `:entrada` per-
/// path L7-filtering ingress contract rests on — the same shape the
/// sibling [`KUBE_KEY_PROTOCOL`] key + [`KUBE_PROTOCOL_TCP`] /
/// [`GATEWAY_API_PROTOCOL_HTTP`] value pair already carries on the
/// L4/L7-protocol scalar-discriminator surface.
///
/// The prior inline `"type"` literal at the one production emitter
/// site (`path_match.insert("type", …)` in `gateway_routes`) would
/// have let a typo (`"Type"` / `"kind"` / `"discriminator"` /
/// `"predicate"`) silently emit an `HTTPRoute` whose per-match path-
/// selection-predicate discriminator scalar-key the Gateway API v1
/// `HTTPPathMatch` OpenAPI schema validator drops as unknown at
/// apply time — the per-match entry falls back to the schema-side
/// default path-match-strategy, silently admitting every URL-path
/// prefix the ingress rule was authored to filter to the exact
/// predicate the typed `:entrada :paths` slot names at the request-
/// path-selection axis, and every external `:entrada` path-filtered
/// flow drops at the gateway-class-controller's admission gate with
/// no field naming the discriminator-scalar-key-drift root cause.
/// The lift routes every K8s-CR-discriminated-union-type-scalar-key
/// retrieval + emission through the same `&'static str` so drift
/// between any two sites becomes a single-edit fix at the caixa-core
/// const definition.
///
/// Same shape as the [`KUBE_KEY_PROTOCOL`] / [`KUBE_KEY_PORT`] re-
/// exports on the sibling L4/L7-protocol + L4-port scalar-axis
/// surfaces — extends the per-K8s-CR top-level
/// `(apiVersion, kind, metadata, spec)` axis re-export quartet + the
/// load-bearing nested `metadata.{name, namespace, labels}` triplet
/// + the load-bearing nested `LabelSelector.matchLabels` selector-
/// projection axis + the load-bearing nested `spec.rules[]` /
/// `toPorts[].rules` rule-list-container axis + the load-bearing
/// nested L4-port-scalar axis + the load-bearing nested L4/L7-
/// protocol-scalar-discriminator axis onto the load-bearing nested
/// K8s-discriminated-union-type-scalar-discriminator axis every
/// downstream apiserver-side OpenAPI-schema-validator / gateway-
/// class-controller consumer of the rendered mesh bundle keys off
/// before it can commit to a per-match request-path-selection
/// predicate.
pub use caixa_core::KUBE_KEY_TYPE;

/// Canonical K8s core `Protocol` OpenAPI schema enum's `TCP` L4-transport-
/// protocol scalar value every `cilium_network_policies`-emitted
/// `CiliumNetworkPolicy` document's per-`spec.ingress[].toPorts[].ports[]`
/// port-tuple declares under its per-tuple [`caixa_core::KUBE_KEY_PROTOCOL`]
/// axis. Re-export of the canonical [`caixa_core::KUBE_PROTOCOL_TCP`] so
/// the K8s-core-`Protocol`-enum-side per-port-tuple L4-transport-selection
/// scalar value lives in exactly one place across every caixa renderer —
/// caixa-mesh's `cilium_network_policies` per-`(:de, :para)` CNP emitter
/// (the single production-code site the prior inline `"TCP".into()`
/// literal sat at, caixa-mesh/src/lib.rs — the per-`toPorts[].ports[]`
/// port-tuple `KUBE_KEY_PROTOCOL` scalar-value emit) and every future
/// per-Cilium-CNP-side / K8s-core-`Protocol`-side renderer the M3.x
/// absorption roadmap acknowledges now consult the same `&'static str`,
/// so a future K8s core `Protocol` enum rebrand (e.g. the `KEP-3675 QUIC
/// transport` proposal's `"QUIC"` addition to the enum, coordinated with
/// the upstream SIG-Network per-version deprecation cycle) is a one-line
/// edit on the canonical [`caixa_core::KUBE_PROTOCOL_TCP`] declaration,
/// not a coordinated rewrite across this crate's `cilium_network_policies`
/// renderer's per-port-tuple `KUBE_KEY_PROTOCOL`-scalar-value emit + every
/// future per-Cilium-CNP-side renderer the substrate adds. The prior
/// inline literal would have let a K8s core `Protocol` rebrand on the
/// caixa-mesh side without a coordinated edit silently emit a
/// `CiliumNetworkPolicy` whose per-`toPorts[].ports[]` port-tuple
/// L4-transport-protocol scalar drifts off the K8s core `Protocol` enum's
/// admitted closed set — apply-side: the Cilium operator's per-CNP L4
/// dispatch pass rejects the object at admission (the K8s core `Protocol`
/// OpenAPI schema enum admits the closed set `{"TCP", "UDP", "SCTP"}`
/// verbatim), and every intra-mesh `:contratos` L4-tuple-gated flow drops
/// at the Cilium operator's admission gate with no field naming the L4-
/// transport-protocol-drift root cause; worse — because the `protocol`
/// scalar carries a schema-side default of `TCP` on the K8s core
/// `Protocol` enum, a silently-elided drift on the emit lands a
/// `CiliumNetworkPolicy` whose ingress rule falls back to the default
/// L4-transport-protocol and every port-match on a non-default transport
/// silently misses at the eBPF data plane's per-tuple dispatch. Peer to
/// the [`GATEWAY_API_PROTOCOL_HTTP`] +
/// [`GATEWAY_API_PATH_MATCH_TYPE_PATH_PREFIX`] re-exports on the sibling
/// canonical-Gateway-API-v1-OpenAPI-schema-enum-value surface — extends
/// the Gateway-API-v1-OpenAPI-schema-enum-value re-export pair onto the
/// sibling K8s-core-`Protocol`-OpenAPI-schema-enum-value axis the same
/// `cilium_network_policies` renderer's intra-mesh L4-tuple-gating
/// contract carries under the shared `CiliumNetworkPolicy` body.
pub use caixa_core::KUBE_PROTOCOL_TCP;

/// Canonical K8s Gateway API `HTTPRoute` per-rule request-timeout-policy
/// body-axis key. Re-export of the canonical
/// [`caixa_core::GATEWAY_API_KEY_TIMEOUTS`] so the per-rule request-
/// timeout-policy field name lives in exactly one place across every
/// caixa renderer — this crate's one production emitter site
/// (`gateway_routes`'s per-`HTTPRoute` per-rule
/// `spec.rules[].timeouts` insert the Aplicacao's typed
/// `:politicas :timeout` overlay lands under, the sub-shape the
/// Gateway API v1 CRD schema pins as `HTTPRouteTimeouts` and whose
/// `request` scalar the Gateway-API-implementation-side per-rule
/// request-dispatch loop compares each accepted request's wall-clock
/// elapsed time against before cancelling the in-flight backend call)
/// and this crate's eight test-side per-rule timeout-policy traversal
/// sites (the `httproute_carries_politicas_timeout_on_every_rule` /
/// `httproute_omits_timeouts_when_politicas_timeout_unset` /
/// `httproute_timeout_renders_every_rule_independently` /
/// `httproute_timeout_uses_canonical_kube_duration_format` /
/// `httproute_timeout_renders_minute_window_canonically` /
/// `httproute_rule_keys_pin_overlay_position` /
/// `httproute_timeouts_and_retry_coexist_independently`
/// pins asserting the overlay's presence, absence, canonical-duration-
/// format contract, per-rule fan-out under multi-`:entrada :paths`,
/// and independent-axis coexistence with the sibling `retry` per-
/// rule retry-policy axis) now consult the same `&'static str` as the
/// peer caixa-core-side const definition.
///
/// The prior inline `"timeouts"` literals at the one production
/// emitter site + eight test-side retrieval sites in this crate would
/// have let a typo on any one site (e.g. `"timeout"` (singular) /
/// `"timeoutPolicy"` / `"deadlines"`) silently miss the per-rule
/// request-timeout policy or emit a malformed `HTTPRoute` whose per-
/// rule request-timeout-policy field the apiserver-side Gateway API
/// CRD schema validator drops as unrecognized at apply time — the
/// Gateway API implementation's per-rule request-dispatch loop would
/// silently no-op the per-rule wall-clock deadline (the "no infinite
/// blocking" guarantee MESH-COMPOSITION.md §V mandates for every
/// rendered per-`:politicas` mesh-composition edge silently regresses
/// to the pre-overlay unbounded-request semantic, and every external
/// `:entrada` flow the route was authored to bound by the typed
/// `:politicas :timeout` slot runs to whatever backend deadline the
/// resolved backend's downstream infrastructure picks with no field
/// naming the per-rule-timeout-policy-drift root cause), and the test-
/// side pins' `.expect("rule must carry timeouts mapping when
/// :politicas :timeout is set")` panic-message tags would fire against
/// the presence-shape message rather than naming the true per-rule-
/// timeout-policy-key drift, the `.get("timeouts").and_then(|t|
/// t.get("request"))` navigators would silently unwrap to `None`
/// under the drifted retrieval. The lift routes every per-rule
/// timeout-policy-axis retrieval + emission through the same
/// `&'static str` so drift between any two sites becomes a single-
/// edit fix at the caixa-core const definition.
///
/// Same shape as the [`GATEWAY_API_KEY_HOSTNAMES`] /
/// [`GATEWAY_API_KEY_HOSTNAME`] / [`GATEWAY_API_KEY_LISTENERS`] /
/// [`GATEWAY_API_KEY_PARENT_REFS`] / [`GATEWAY_API_KEY_BACKEND_REFS`]
/// re-exports on the sibling per-Gateway-API-CRD-body-axis surface —
/// extends the per-Gateway-API-`HTTPRoute` per-rule body-axis re-export
/// set (`backendRefs`) onto the load-bearing per-rule request-timeout-
/// policy axis every downstream Gateway-API-implementation-side per-
/// rule request-dispatch loop keys off before it can commit to a per-
/// request wall-clock deadline. Peer to the sibling load-bearing K8s-
/// CR-schema-axis re-exports every downstream apiserver-side CRD-
/// schema-validator navigates the same per-rule request-timeout-policy
/// axis on (the gateway-class-controller's per-HTTPRoute per-rule
/// request-dispatch pass under `spec.rules[].timeouts.request`).
pub use caixa_core::GATEWAY_API_KEY_TIMEOUTS;

/// Canonical K8s Gateway API `HTTPRoute` per-rule retry-policy body-axis
/// key. Re-export of the canonical [`caixa_core::GATEWAY_API_KEY_RETRY`]
/// so the per-rule retry-policy field name lives in exactly one place
/// across every caixa renderer — this crate's one production emitter
/// site (`gateway_routes`'s per-`HTTPRoute` per-rule
/// `spec.rules[].retry` insert the Aplicacao's typed `:politicas
/// :retries` overlay lands under, the sub-shape the Gateway API v1 CRD
/// schema pins as `HTTPRouteRetry` and whose `attempts` scalar the
/// Gateway-API-implementation-side per-rule request-dispatch loop
/// compares each failed attempt count against before giving up on the
/// in-flight backend call) and this crate's eight test-side per-rule
/// retry-policy traversal sites (the
/// `httproute_rule_keys_pin_overlay_position` rule-level top-key-set
/// pin, the `httproute_carries_politicas_retries_on_every_rule`
/// presence pin, the `httproute_omits_retry_when_politicas_retries_unset`
/// absence pin, the `httproute_retry_renders_every_rule_independently`
/// per-rule fan-out pin under multi-`:entrada :paths`, the
/// `httproute_retry_round_trips_typed_attempt_count` typed-`u32`-round-
/// trip pin, the `httproute_retry_attempts_serialized_as_yaml_number`
/// YAML integer-scalar-kind pin, and two
/// `httproute_timeouts_and_retry_coexist_independently` presence-only +
/// absence-only pins pinning independent-axis coexistence with the
/// sibling `timeouts` per-rule request-timeout-policy axis) now consult
/// the same `&'static str` as the peer caixa-core-side const definition.
///
/// The prior inline `"retry"` literals at the one production emitter
/// site + eight test-side retrieval sites in this crate would have let a
/// typo on any one site (e.g. `"retries"` (plural) / `"retryPolicy"` /
/// `"budget"`) silently miss the per-rule retry policy or emit a
/// malformed `HTTPRoute` whose per-rule retry-policy field the
/// apiserver-side Gateway API CRD schema validator drops as unrecognized
/// at apply time — the Gateway API implementation's per-rule request-
/// dispatch loop would silently no-op the per-rule retry budget (the
/// "no infinite retrying without bound" guarantee MESH-COMPOSITION.md §V
/// mandates for every rendered per-`:politicas` mesh-composition edge
/// silently regresses to the pre-overlay unbounded-retry semantic, and
/// every external `:entrada` flow the route was authored to cap by the
/// typed `:politicas :retries` slot runs to whatever retry policy the
/// resolved backend's downstream infrastructure picks with no field
/// naming the per-rule-retry-policy-drift root cause), and the test-
/// side pins' `.expect("rule must carry retry mapping when :politicas
/// :retries is set")` panic-message tags would fire against the
/// presence-shape message rather than naming the true per-rule-retry-
/// policy-key drift, the `.get("retry").and_then(|r| r.get("attempts"))`
/// navigators would silently unwrap to `None` under the drifted
/// retrieval. The lift routes every per-rule retry-policy-axis
/// retrieval + emission through the same `&'static str` so drift
/// between any two sites becomes a single-edit fix at the caixa-core
/// const definition.
///
/// Same shape as the [`GATEWAY_API_KEY_TIMEOUTS`] /
/// [`GATEWAY_API_KEY_HOSTNAMES`] / [`GATEWAY_API_KEY_HOSTNAME`] /
/// [`GATEWAY_API_KEY_LISTENERS`] / [`GATEWAY_API_KEY_PARENT_REFS`] /
/// [`GATEWAY_API_KEY_BACKEND_REFS`] re-exports on the sibling per-
/// Gateway-API-CRD-body-axis surface — closes the per-Gateway-API-
/// `HTTPRoute`-per-rule `:politicas` overlay axis re-export pair
/// (`timeouts` for `:politicas :timeout`, `retry` for `:politicas
/// :retries`) both MESH-COMPOSITION.md §V "no infinite blocking / no
/// infinite retrying" guarantees rest on. Peer to the sibling load-
/// bearing K8s-CR-schema-axis re-exports every downstream apiserver-
/// side CRD-schema-validator navigates the same per-rule retry-policy
/// axis on (the gateway-class-controller's per-HTTPRoute per-rule
/// request-dispatch pass under `spec.rules[].retry.attempts`).
pub use caixa_core::GATEWAY_API_KEY_RETRY;

/// Canonical K8s Gateway API `HTTPRoute` per-rule retry-policy
/// `attempts` leaf scalar-key. Re-export of the canonical
/// [`caixa_core::GATEWAY_API_KEY_ATTEMPTS`] so the per-rule retry-
/// attempts leaf key lives in exactly one place across every caixa
/// renderer — this crate's one production emitter site
/// (`gateway_routes`'s per-`HTTPRoute` per-rule
/// `single_field_overlay(spec.politicas.retries, …)` call that seeds
/// the typed `u32` attempt count into the sibling
/// [`GATEWAY_API_KEY_RETRY`] container axis under
/// `spec.rules[].retry.attempts`, the leaf the Gateway API v1 CRD
/// schema pins as `HTTPRouteRetry.attempts` and whose scalar value
/// the Gateway-API-implementation-side per-rule request-dispatch
/// loop compares each failed backend attempt count against before
/// giving up on the in-flight backend call) and this crate's five
/// test-side per-rule retry-attempts traversal sites (the
/// `httproute_carries_politicas_retries_on_every_rule` typed-`u64`-
/// value pin, the `httproute_retry_renders_every_rule_independently`
/// per-rule fan-out attempt-count pin under multi-`:entrada :paths`,
/// the `httproute_retry_round_trips_typed_attempt_count` typed-`u32`-
/// round-trip pin, the `httproute_retry_attempts_serialized_as_yaml_number`
/// YAML integer-scalar-kind pin, and the retries-only arm of
/// `httproute_timeouts_and_retry_coexist_independently` pinning the
/// leaf attempt count survives when only the sibling `:retries` slot
/// is set) now consult the same `&'static str` as the peer caixa-core-
/// side const definition.
///
/// The prior inline `"attempts"` literals at the one production
/// emitter site + five test-side retrieval sites in this crate would
/// have let a typo on any one site (e.g. `"attempt"` (singular) /
/// `"count"` / `"tries"` / `"maxAttempts"`) silently drop the per-rule
/// retry attempt count or emit a malformed `HTTPRoute` whose per-rule
/// retry-attempts leaf the apiserver-side Gateway API CRD schema
/// validator drops as unrecognized at apply time — the Gateway API
/// implementation's per-rule request-dispatch loop would silently
/// parse the retry sub-shape as an empty `HTTPRouteRetry` with the
/// typed `u32` attempt count discarded (the "no infinite retrying
/// without bound" guarantee MESH-COMPOSITION.md §V mandates for
/// every rendered per-`:politicas` mesh-composition edge silently
/// regresses to the pre-overlay unbounded-retry semantic, and every
/// external `:entrada` flow the route was authored to cap by the
/// typed `:politicas :retries` slot runs to whatever retry policy
/// the resolved backend's downstream infrastructure picks with no
/// field naming the per-rule-retry-attempts-leaf-key-drift root
/// cause), and the test-side navigators' `.and_then(|r|
/// r.get("attempts"))` chains would silently unwrap to `None` under
/// the drifted retrieval. The lift routes every per-rule retry-
/// attempts-leaf retrieval + emission through the same `&'static str`
/// so drift between any two sites becomes a single-edit fix at the
/// caixa-core const definition.
///
/// Same shape as the [`GATEWAY_API_KEY_RETRY`] /
/// [`GATEWAY_API_KEY_TIMEOUTS`] / [`GATEWAY_API_KEY_HOSTNAMES`] /
/// [`GATEWAY_API_KEY_HOSTNAME`] / [`GATEWAY_API_KEY_LISTENERS`] /
/// [`GATEWAY_API_KEY_PARENT_REFS`] / [`GATEWAY_API_KEY_BACKEND_REFS`]
/// re-exports on the sibling per-Gateway-API-CRD-body-axis surface —
/// closes the parent-leaf axis pair (`retry` container + `attempts`
/// leaf) the K8s Gateway API v1 `HTTPRouteRetry` sub-shape pins under
/// `HTTPRoute.spec.rules[].retry.attempts`, one nesting level deeper
/// than the parent per-rule retry-policy container axis (`retry`).
/// Peer to the sibling load-bearing K8s-CR-schema-axis re-exports
/// every downstream apiserver-side CRD-schema-validator navigates the
/// same per-rule retry-attempts leaf on (the gateway-class-
/// controller's per-HTTPRoute per-rule request-dispatch pass under
/// `spec.rules[].retry.attempts`).
pub use caixa_core::GATEWAY_API_KEY_ATTEMPTS;

/// Canonical K8s Gateway API `HTTPRoute` per-rule request-timeout-policy
/// `request` leaf scalar-key. Re-export of the canonical
/// [`caixa_core::GATEWAY_API_KEY_REQUEST`] so the per-rule request-
/// deadline leaf key lives in exactly one place across every caixa
/// renderer — this crate's one production emitter site
/// (`gateway_routes`'s per-`HTTPRoute` per-rule
/// `single_field_overlay(spec.politicas.timeout, …)` call that seeds
/// the typed `Duration` request-deadline string into the sibling
/// [`GATEWAY_API_KEY_TIMEOUTS`] container axis under
/// `spec.rules[].timeouts.request`, the leaf the Gateway API v1 CRD
/// schema pins as `HTTPRouteTimeouts.request` and whose scalar value
/// the Gateway-API-implementation-side per-rule request-dispatch loop
/// commits to as the per-request wall-clock deadline every inbound
/// request is bounded against before the resolved backend even sees
/// the call) and this crate's five test-side per-rule request-
/// deadline traversal sites (the
/// `httproute_carries_politicas_timeout_on_every_rule` typed-`&str`-
/// value pin, the `httproute_timeout_renders_every_rule_independently`
/// per-rule fan-out request-deadline pin under multi-`:entrada :paths`,
/// the `httproute_timeout_uses_canonical_kube_duration_format` typed-
/// `Duration`-round-trip pin, the
/// `httproute_timeout_renders_minute_window_canonically` canonical-
/// minute-form pin, and the timeout-only arm of
/// `httproute_timeouts_and_retry_coexist_independently` pinning the
/// leaf request-deadline survives when only the sibling `:timeout`
/// slot is set) now consult the same `&'static str` as the peer
/// caixa-core-side const definition.
///
/// The prior inline `"request"` literals at the one production emitter
/// site + five test-side retrieval sites in this crate would have let a
/// typo on any one site (e.g. `"deadline"` / `"requestTimeout"` /
/// `"timeout"` / `"upstreamRequest"`) silently drop the per-rule
/// request deadline or emit a malformed `HTTPRoute` whose per-rule
/// request-deadline leaf the apiserver-side Gateway API CRD schema
/// validator drops as unrecognized at apply time — the Gateway API
/// implementation's per-rule request-dispatch loop would silently parse
/// the timeouts sub-shape as an empty `HTTPRouteTimeouts` with the
/// typed `Duration` request-deadline discarded (the "no infinite
/// blocking" guarantee MESH-COMPOSITION.md §V mandates for every
/// rendered per-`:politicas` mesh-composition edge silently regresses
/// to the pre-overlay unbounded-blocking semantic, and every external
/// `:entrada` flow the route was authored to cap by the typed
/// `:politicas :timeout` slot runs to whatever request-deadline the
/// resolved backend's downstream infrastructure picks with no field
/// naming the per-rule-request-deadline-leaf-key-drift root cause),
/// and the test-side navigators' `.and_then(|t| t.get("request"))`
/// chains would silently unwrap to `None` under the drifted retrieval.
/// The lift routes every per-rule request-deadline-leaf retrieval +
/// emission through the same `&'static str` so drift between any two
/// sites becomes a single-edit fix at the caixa-core const definition.
///
/// Same shape as the [`GATEWAY_API_KEY_ATTEMPTS`] /
/// [`GATEWAY_API_KEY_RETRY`] / [`GATEWAY_API_KEY_TIMEOUTS`] /
/// [`GATEWAY_API_KEY_HOSTNAMES`] / [`GATEWAY_API_KEY_HOSTNAME`] /
/// [`GATEWAY_API_KEY_LISTENERS`] / [`GATEWAY_API_KEY_PARENT_REFS`] /
/// [`GATEWAY_API_KEY_BACKEND_REFS`] re-exports on the sibling per-
/// Gateway-API-CRD-body-axis surface — closes the second parent-leaf
/// axis pair (`timeouts` container + `request` leaf) the K8s Gateway
/// API v1 `HTTPRouteTimeouts` sub-shape pins under
/// `HTTPRoute.spec.rules[].timeouts.request`, sibling to the parent-
/// leaf pair (`retry` container + `attempts` leaf) closed in the
/// immediately-preceding [`GATEWAY_API_KEY_ATTEMPTS`] lift (e2e136b).
/// Both MESH-COMPOSITION.md §V "no infinite blocking / no infinite
/// retrying" guarantees now rest on typed lifts at both container-axis
/// and leaf-scalar-axis nesting levels. Peer to the sibling load-
/// bearing K8s-CR-schema-axis re-exports every downstream apiserver-
/// side CRD-schema-validator navigates the same per-rule request-
/// deadline leaf on (the gateway-class-controller's per-HTTPRoute
/// per-rule request-dispatch pass under
/// `spec.rules[].timeouts.request`).
pub use caixa_core::GATEWAY_API_KEY_REQUEST;

// ── Cilium NetworkPolicy emission ──────────────────────────────────────

/// Render one [`CiliumNetworkPolicy`-shaped][cnp] YAML per distinct
/// `(:de, :para)` pair across `:contratos`. The policy whitelists the
/// `:de → :para` flow at L4 (every contract); HTTP contracts add L7
/// rules (path) keyed by the `:wit` shape.
///
/// A CiliumNetworkPolicy's identity is its destination (`endpointSelector`)
/// plus its admitted source (`fromEndpoints`), so the `(:de, :para)`
/// pair is the policy's `metadata.name` axis — `<aplicacao>-<de>-to-<para>`.
/// [`AplicacaoSpec::validate`] deliberately permits multiple typed edges
/// between the same ordered pair (cart→catalog at `/products` *and*
/// `/search`, an HTTP edge alongside a NATS edge — distinct identity keys
/// via differing payloads, see `caixa_core::aplicacao` validate), so the
/// renderer fans those in: each edge in a `(:de, :para)` group contributes
/// one `ingress[0].toPorts[]` entry to the *single* policy for that pair.
/// Emitting one policy per raw contrato instead would name two objects
/// `<aplicacao>-<de>-to-<para>` identically and collide at `kubectl apply`
/// time, far from the source caixa.lisp.
///
/// Every emitted policy is identity-based — `endpointSelector` matches
/// pleme labels (`pleme.pleme.io/program: <:para>`) injected by the
/// fleet-programs aggregator, and `fromEndpoints` requires the same
/// label on the source. Identity = caixa nome + Aplicacao annotation
/// (no IP-based reasoning required).
///
/// V0 emits a typed YAML mapping; the operator (Cilium control plane)
/// validates against the official schema.
///
/// [cnp]: https://docs.cilium.io/en/stable/security/policy/index.html
pub fn cilium_network_policies(caixa: &Caixa) -> Result<Vec<serde_yaml::Value>, Error> {
    let spec = typed_view(caixa)?;
    let namespace = DEFAULT_NAMESPACE; // operators scope per-cluster manifests
    // `:politicas :mtls-required` overlay — when the typed slot
    // carries a value it surfaces as a per-ingress-rule
    // `authentication: { mode: <mode> }` block on every emitted
    // CiliumNetworkPolicy, the canonical Cilium per-rule mutual-
    // authentication shape:
    // https://docs.cilium.io/en/stable/network/servicemesh/mutual-authentication/
    //
    // Same trajectory as the `:timeout`/`:retries` overlays in
    // [`gateway_routes`]: until this landed the typed
    // `:mtls-required` slot was inert past
    // [`AplicacaoSpec::validate`] — the slot round-tripped through
    // serde and read non-empty for `MeshPolicy::is_empty`, but no
    // caixa-side renderer surfaced it as a cluster artifact. Wiring
    // it through the CiliumNetworkPolicy renderer turns the
    // MESH-COMPOSITION §V CSE invariant ("every Aplicacao declares
    // `:politicas :mtls-required t` — no plaintext intra-mesh") from
    // a validate-time gate into a runtime-enforced contract: Cilium's
    // identity-aware data plane refuses every ingress edge that
    // doesn't carry a peer SPIFFE-identity-bound mTLS handshake, so
    // a same-namespace pod that doesn't satisfy the typed identity
    // contract can't satisfy the rule even from inside the cluster.
    //
    // The author-facing `:mtls-required` slot is a `Option<bool>`
    // tristate (Some(true) | Some(false) | None) — the explicit
    // Some(false) opt-out reads non-empty for `MeshPolicy::is_empty`
    // (the author *named* the axis, the renderer needs to honor that
    // vs. fall back to the cluster default). The two non-None arms
    // map to the two valid Cilium authentication modes:
    //   - Some(true)  → `mode: "required"`  (mTLS handshake mandatory)
    //   - Some(false) → `mode: "disabled"`  (mTLS handshake skipped —
    //                    explicit opt-out, e.g. for a debug edge)
    //   - None        → omit the block entirely (cluster default
    //                    applies — typically "disabled" cluster-wide).
    // Single-axis overlay built once per renderer call and cloned into
    // each ingress rule. Same lifted-typed-primitive shape the
    // gateway_routes overlays (timeout, retry) consume — see
    // [`caixa_core::render::single_field_overlay`].
    // Route the `:politicas :mtls-required` mTLS-enforcement-toggle
    // read through the typed [`caixa_core::MeshPolicy::mtls_required`]
    // accessor rather than the raw `spec.politicas.mtls_required` field
    // access — one of the two open-coded field-access sites on the
    // per-`:politicas` `:mtls-required` axis the accessor lift now
    // owns (peer of the [`caixa_core::MeshPolicy::is_empty`] arm that
    // already routes through the same dispatch). The accessor returns
    // `Option<bool>` by copy (bool is `Copy`), so
    // [`single_field_overlay`]'s first parameter accepts the narrower
    // owned Option verbatim without a re-allocation or a `.clone()`.
    // Route the outer `:politicas` composite-reference read through
    // the lifted [`caixa_core::AplicacaoSpec::politicas`] outer accessor
    // rather than the raw `spec.politicas` field access — the outer
    // composite-reference axis now dispatches on the substrate
    // primitive, and the per-axis `mtls_required()` inner-accessor
    // dispatch chains onto the returned `&MeshPolicy` reference verbatim.
    let mtls_overlay = single_field_overlay(
        spec.politicas().mtls_required(),
        CILIUM_KEY_MODE,
        |required| serde_yaml::Value::String(cilium_auth_mode(required).into()),
    );
    // Fan typed edges into per-`(:de, :para)` groups — the policy
    // identity axis. A `BTreeMap` keyed by the pair gives deterministic
    // policy order independent of `:contratos` declaration order
    // (THEORY.md §V.2.7 render determinism), and collapses the
    // validate-permitted "same caller-callee pair, different payload"
    // contratos (cart→catalog at `/products` and `/search`) onto the
    // single CiliumNetworkPolicy whose `metadata.name` they'd otherwise
    // collide on. Insertion preserves source order within each group, so
    // the per-edge `toPorts[]` entries appear in the author's declared
    // order.
    let mut groups: BTreeMap<(&str, &str), Vec<&WitContract>> = BTreeMap::new();
    for c in spec.contratos() {
        groups
            .entry((c.source(), c.destination()))
            .or_default()
            .push(c);
    }

    let mut out = Vec::with_capacity(groups.len());
    for ((de, para), edges) in &groups {
        // Policy's own labels — `aplicacao` (which graph) and
        // `contrato` (which typed edge pair). Keys come from
        // caixa_core::render so a future label-namespace rebrand is a
        // one-line edit, not a search-and-replace across renderers.
        // The per-`(:de, :para)` [`LABEL_CONTRATO`] value + the
        // per-CNP `metadata.name` now compose on the lifted
        // [`contrato_edge_label`] / [`cilium_network_policy_name`]
        // helpers (which route through the canonical
        // [`caixa_core::CONTRATO_EDGE_LABEL_SEPARATOR`] `"-to-"`
        // byte-string) so a future edge-encoding rebrand lands at one
        // const-edit and both writer sites pick up the new encoding
        // by construction. Prior to this lift the two sites carried
        // verbatim inline `format!("{de}-to-{para}")` +
        // `format!("{}-{}-to-{}", caixa.nome, de, para)` with no
        // compile-time link between them; a rebrand on either half
        // would have silently split the CNP `metadata.name` from its
        // own `metadata.labels.pleme.pleme.io/contrato` value,
        // orphaning every operator-side `kubectl get cnp -l
        // pleme.pleme.io/contrato=<de>-to-<para>` grep-by-label
        // query at apply time far from the source caixa.lisp.
        let mut labels = BTreeMap::new();
        labels.insert(LABEL_APLICACAO, caixa.nome().to_string());
        labels.insert(LABEL_CONTRATO, contrato_edge_label(de, para));
        // The apiVersion + kind + metadata.{name, namespace, labels}
        // skeleton comes from caixa_core::render::kube_resource_skeleton
        // — same lift as pleme_program_*_selector applied to the K8s-
        // resource axis. Caller adds spec below. The Cilium-CRD-group/
        // version string threads through the lifted
        // [`CILIUM_API_VERSION`] re-export so a future Cilium-CRD bump
        // lands on the canonical [`caixa_core::CILIUM_API_VERSION`]
        // declaration, not at this call site. The kind axis of the
        // `(apiVersion, kind)` CRD-lookup tuple now threads through the
        // matching [`CILIUM_KIND_NETWORK_POLICY`] re-export so a future
        // Cilium-CRD kind rebrand lands on the canonical
        // [`caixa_core::CILIUM_KIND_NETWORK_POLICY`] declaration too —
        // both halves of the tuple move as a unit through one lifted
        // const each, no per-renderer drift surface. The CNP
        // `metadata.name` axis now threads through the lifted
        // [`cilium_network_policy_name`] composer (peer with the
        // [`LABEL_CONTRATO`] value composer above) so the
        // per-CNP identity pair — `(metadata.labels.pleme.pleme.io/
        // contrato, metadata.name)` — shares one canonical
        // edge-encoding source of truth
        // ([`caixa_core::CONTRATO_EDGE_LABEL_SEPARATOR`]).
        // The per-CNP `metadata.name` identity byte-string derives from
        // the parent-Caixa's `:nome` verbatim through the substrate-
        // canonical [`caixa_core::cilium_network_policy_name`] composer.
        // Routing the aplicacao-name arg through the typed
        // [`caixa_core::Caixa::nome`] accessor (`caixa.nome()`) rather
        // than the raw `&caixa.nome` `&String`-borrow of the underlying
        // field extends the "one typed dispatch on the substrate
        // primitive, thin projections at each consumer" discipline the
        // e6b7d97 [`caixa_core::Caixa::nome`] accessor lift opened onto
        // this per-CNP `metadata.name` axis — peer of the sibling 22461ef
        // caixa-helm non-`.clone()` raw-field-access converge on the
        // per-`lareira-<nome>` chart-directory identity composer.
        let mut policy = kube_resource_skeleton(
            CILIUM_API_VERSION,
            CILIUM_KIND_NETWORK_POLICY,
            &cilium_network_policy_name(caixa.nome(), de, para),
            namespace,
            labels,
        );

        // spec.endpointSelector — match the destination Servico's
        // identity. Single-axis (program-only) selector; see
        // caixa_core::render::pleme_program_selector for the deliberate
        // intent / safety tradeoff vs. the in-aplicacao variant. The
        // `{matchLabels: <selector>}` envelope comes from
        // caixa_core::render::label_selector — same lift as
        // yaml_string_mapping / kube_resource_skeleton applied to the
        // K8s LabelSelector axis.
        let endpoint_selector = label_selector(pleme_program_selector(para));

        // ingress[0]: from the source Servico, scoped to this
        // Aplicacao (so a same-named program in a different Aplicacao
        // can't satisfy the rule). Two-axis selector via the
        // canonical helper — call-site reads as intent, not as five
        // hand-written insert() calls. Wrapped in label_selector so
        // the `matchLabels` envelope is the typed primitive's
        // responsibility, not this site's.
        // The per-CNP ingress[0]-from-endpoint two-axis selector's
        // aplicacao-scope arg (the `LABEL_APLICACAO` value the emitted
        // selector matches against) reads the parent-Caixa's `:nome`
        // through the typed [`caixa_core::Caixa::nome`] accessor rather
        // than the raw `&caixa.nome` `&String`-borrow of the underlying
        // field — same converge as the peer per-CNP `metadata.name`
        // composer above, extended onto the sibling per-CNP ingress
        // selector's aplicacao-scope axis so the pair
        // `(metadata.name, ingress[0].from[].matchLabels
        // .pleme.pleme.io/aplicacao)` shares one typed dispatch on the
        // substrate primitive.
        let from_endpoint = label_selector(pleme_program_in_aplicacao_selector(de, caixa.nome()));
        let mut ingress_rule = serde_yaml::Mapping::new();
        ingress_rule.insert_sequence(CILIUM_KEY_FROM_ENDPOINTS, vec![from_endpoint]);

        // One `toPorts[]` entry per typed edge in the group — Cilium
        // unions the L4/L7 rules across entries, so each edge keeps its
        // own L7 shape (an HTTP edge's path stays scoped to the HTTP
        // edge; a NATS/store edge stays L4-only) instead of leaking
        // across the shared destination port.
        let mut to_ports_seq = Vec::with_capacity(edges.len());
        for c in edges {
            // toPorts — wit-shape-aware. HTTP gets L7 rules; pubsub +
            // store get L4-only (Cilium can't introspect those protocols).
            let mut to_port = serde_yaml::Mapping::new();
            let mut port_entry = serde_yaml::Mapping::new();
            // The per-destination Servico TCP port every emitted CNP's
            // ingress[].toPorts[].ports[0].port axis reads now routes
            // through the canonical [`AplicacaoSpec::port_for_destination`]
            // typed dispatch (re-exported through the peer `typed_view`
            // path this call reaches through). Prior to this lift the
            // "if the typed `:entrada` block names this destination use
            // its author-declared `:port`, else fall back to the lifted
            // [`DEFAULT_SERVICO_PORT`] substrate-canonical port floor"
            // cascade lived inline here — the sole per-Aplicacao L4
            // fallback site with no typed method on the substrate
            // primitive that named the rule, so a future per-destination
            // port axis addition (a per-`:contratos` explicit `:port`
            // slot the M4 typed-edge registry adds, a per-`:membros`
            // `:port` overlay once heterogeneous per-Servico listener
            // ports land, a per-cluster override the operator pins
            // through a future `:placement :default-port` slot) would
            // have had to be threaded through this inline cascade and
            // every future per-Aplicacao renderer's inline copy in
            // lockstep or one consumer would silently disagree on which
            // port a given destination Servico's ingress lands at.
            // Lifting the resolution rule to a typed method on
            // `AplicacaoSpec` — peer with the sibling
            // [`AplicacaoSpec::validate`] / `AplicacaoSpec::detect_sync_cycles`
            // typed dispatches — means the M4
            // `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's
            // per-CNP L4 port resolver, the future per-edge policy
            // resolver's per-destination probe axis, and every
            // downstream test-fixture navigator asserting the L4
            // port floor read from one place.
            let port = spec.port_for_destination(c.destination());
            port_entry.insert_string(KUBE_KEY_PORT, port.to_string());
            port_entry.insert_string(KUBE_KEY_PROTOCOL, KUBE_PROTOCOL_TCP);
            to_port.insert_singleton_mapping_sequence(CILIUM_KEY_PORTS, port_entry);

            // L7 introspection only fires for HTTP-shaped contracts; the
            // typed view (validated upstream by AplicacaoSpec::validate)
            // makes the "wit world ↔ payload field" link impossible to
            // get wrong silently. PubSub / Store / Capability edges stay
            // L4-only — Cilium can't introspect those protocols.
            //
            // Route the per-arm HTTP-endpoint read through the lifted
            // substrate-primitive [`caixa_core::WitTarget::http_endpoint`]
            // typed accessor rather than the raw `if let
            // WitTarget::Http { endpoint } = …` pattern-match — the
            // per-`(:de, :para)` CNP L7 introspection branch's sole
            // production consumer of the projected-target HTTP endpoint
            // now dispatches on the substrate primitive's canonical
            // post-projection HTTP-endpoint accessor, sibling to the
            // WitContract pre-projection [`caixa_core::WitContract::endpoint`]
            // (7020470) `Option<&str>` accessor on the peer raw-field
            // axis. A future [`WitTarget`] variant addition (a
            // `Rest`/`Grpc` split of [`WitTarget::Http`] once the WIT
            // registry stabilizes gRPC-shaped worlds per the enum's own
            // docstring) reaches this L7 emit site through one
            // caixa-core edit at the accessor's match arms — either
            // coalescing the two peers under a shared `path:` emit, or
            // splitting the emit path per-arm — rather than a
            // coordinated rewrite of every renderer's raw `if let`
            // pattern-match on the [`WitTarget::Http`] arm's payload
            // field.
            if let Some(endpoint) = c.target().expect("validated by typed_view").http_endpoint() {
                let mut http_rule = serde_yaml::Mapping::new();
                http_rule.insert_string(CILIUM_KEY_PATH, endpoint.to_string());
                let mut rules = serde_yaml::Mapping::new();
                rules.insert_singleton_mapping_sequence(CILIUM_KEY_HTTP, http_rule);
                to_port.insert_mapping(KUBE_KEY_RULES, rules);
            }
            to_ports_seq.push_mapping(to_port);
        }
        ingress_rule.insert_sequence(CILIUM_KEY_TO_PORTS, to_ports_seq);
        ingress_rule.insert_str_key_if_some(CILIUM_KEY_AUTHENTICATION, mtls_overlay.as_ref());

        let mut policy_spec = serde_yaml::Mapping::new();
        policy_spec.insert_str_key(CILIUM_KEY_ENDPOINT_SELECTOR, endpoint_selector);
        policy_spec.insert_singleton_mapping_sequence(CILIUM_KEY_INGRESS, ingress_rule);
        policy.insert_mapping(KUBE_KEY_SPEC, policy_spec);

        out.push_mapping(policy);
    }
    Ok(out)
}

// ── K8s Gateway API emission ───────────────────────────────────────────

/// Render the Gateway + HTTPRoute pair for `:entrada`, when set.
/// Returns an empty Vec when the Aplicacao has no external entry
/// point (internal-only meshes).
///
/// Output is two YAML documents:
///
///   - one `gateway.networking.k8s.io/v1 Gateway` named after the
///     Aplicacao, listening on the Aplicacao's host
///   - one `gateway.networking.k8s.io/v1 HTTPRoute` per declared
///     `:entrada :paths` entry (or one catch-all when paths is empty),
///     pointing at the destination Servico.
pub fn gateway_routes(caixa: &Caixa) -> Result<Vec<serde_yaml::Value>, Error> {
    let spec = typed_view(caixa)?;
    // Route the per-`:entrada` composite-reference read through the
    // lifted [`caixa_core::AplicacaoSpec::entrada`] accessor rather
    // than the raw `spec.entrada.as_ref()` field access — the
    // per-Aplicacao K8s Gateway API v1 Gateway + HTTPRoute emitter's
    // early-return partition now keys off the canonical read-side
    // surface every per-Aplicacao entrada consumer routes through,
    // peer of the sibling per-`:politicas` and per-`:placement`
    // outer-composite-reference migrations already routed through
    // [`caixa_core::AplicacaoSpec::politicas`] and
    // [`caixa_core::AplicacaoSpec::placement`].
    let entrada = match spec.entrada() {
        Some(e) => e,
        None => return Ok(Vec::new()),
    };
    let namespace = DEFAULT_NAMESPACE;

    // Gateway — apiVersion + kind + metadata.{name, namespace} skeleton
    // comes from caixa_core::render::kube_resource_skeleton; caller adds
    // spec below. No metadata.labels on Gateway today (the gateway is
    // identified by its own name + namespace; per-Aplicacao label
    // grouping happens at the HTTPRoute / route-attached-policy axis).
    // The Gateway-API-CRD-group/version string threads through the
    // lifted [`GATEWAY_API_API_VERSION`] re-export so a future
    // Gateway-API bump lands on the canonical
    // [`caixa_core::GATEWAY_API_API_VERSION`] declaration, not at this
    // call site. The kind axis of the `(apiVersion, kind)` CRD-lookup
    // tuple now threads through the matching [`GATEWAY_API_KIND_GATEWAY`]
    // re-export so a future Gateway-API kind rebrand lands on the
    // canonical [`caixa_core::GATEWAY_API_KIND_GATEWAY`] declaration too
    // — both halves of the tuple move as a unit through one lifted const
    // each, no per-renderer drift surface.
    // The Gateway `metadata.name` identity byte-string derives from the
    // parent-Aplicacao Caixa's `:nome` verbatim (Gateway API v1 keys per-
    // Gateway resolution off this scalar, and the sibling HTTPRoute
    // `spec.parentRefs[0].name` below binds through it). Routing the
    // name arg through the typed [`caixa_core::Caixa::nome`] accessor
    // (`caixa.nome()`) rather than the raw `&caixa.nome` `&String`-borrow
    // of the underlying field pins the pair `(Gateway metadata.name,
    // HTTPRoute spec.parentRefs[0].name)` onto one typed dispatch — the
    // parentRefs projection already read through [`caixa_core::Caixa::nome`]
    // ahead of this converge; this call closes the peer skeleton-name
    // arm so both halves of the parent-Aplicacao's Gateway identity
    // pair move as a unit on any future accessor extension.
    let mut gateway = kube_resource_skeleton(
        GATEWAY_API_API_VERSION,
        GATEWAY_API_KIND_GATEWAY,
        caixa.nome(),
        namespace,
        BTreeMap::new(),
    );
    let mut listener = serde_yaml::Mapping::new();
    // Per-listener name-discriminator scalar — reads from the lifted
    // [`GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME`] `&'static str` const so
    // a future substrate-side listener-name migration (`"http"` →
    // `"http-v1"` once multi-listener Gateways ship under the HTTPS-by-
    // default trajectory the sibling
    // [`GATEWAY_API_DEFAULT_HTTP_LISTENER_PORT`] docstring names, an
    // operator-pinned override the future `:entrada :listener-name`
    // slot promotes) lands at the canonical
    // [`caixa_core::GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME`] declaration,
    // not at this call site. Peer with the sibling
    // [`GATEWAY_API_DEFAULT_HTTP_LISTENER_PORT`] port-scalar consumer on
    // the immediately-following `listener.insert(KUBE_KEY_PORT, …)`
    // line — both per-listener substrate-canonical scalar-value axes
    // now route through their own lifted typed const, so a future
    // rebrand on either axis reaches its consumer by construction.
    // Downstream `HTTPRoute.spec.parentRefs[].sectionName` selectors
    // that attach to this Gateway's HTTP listener bind by the same
    // lifted byte-string, so a listener-name drift can't silently
    // orphan the route at attachment time.
    listener.insert_string(GATEWAY_API_KEY_NAME, GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME);
    // Per-listener HTTP-listener-port scalar — reads from the lifted
    // [`GATEWAY_API_DEFAULT_HTTP_LISTENER_PORT`] `u16` const so a future
    // substrate-side external-Gateway port migration (`:80` → `:443`
    // once cert-manager-issued per-`:entrada :host` certificates land
    // and the external listener becomes HTTPS-by-default, matching the
    // mTLS-by-default trajectory [`DEFAULT_SERVICO_PORT`]'s docstring
    // names) lands at the canonical
    // [`caixa_core::GATEWAY_API_DEFAULT_HTTP_LISTENER_PORT`] declaration,
    // not at this call site. Peer with the sibling
    // `DEFAULT_SERVICO_PORT` fallback in the `cilium_network_policies`
    // per-`(:de, :para)` L4 port resolver — both consumers of a
    // K8s-CRD-side `port:` axis now route through their own lifted
    // typed `u16` const, so a future rebrand on either axis reaches
    // its consumer by construction.
    listener.insert_number(KUBE_KEY_PORT, GATEWAY_API_DEFAULT_HTTP_LISTENER_PORT);
    listener.insert_string(KUBE_KEY_PROTOCOL, GATEWAY_API_PROTOCOL_HTTP);
    // Substrate-canonical per-`:entrada` DNS-hostname singular
    // resolver — routes through the lifted
    // [`caixa_core::Entrada::hostname`] typed accessor on the
    // substrate primitive so the parent-Gateway per-listener
    // `hostname:` filter and the peer per-HTTPRoute plural
    // `spec.hostnames[]` filter list (see the sibling
    // [`caixa_core::Entrada::hostnames`] consumer below in this
    // same `gateway_routes` emitter) both key off exactly one
    // typed dispatch on the substrate primitive. Every future
    // Gateway-API-aware consumer (the M4 `mesh.pleme.io/v1alpha1/
    // Aplicacao` CR materializer's per-listener SNI fan-out, a
    // future per-cluster `:entrada :alt-hosts` overlay resolver
    // the operator pins through a `:placement`-scoped slot,
    // every future per-Aplicacao snapshot renderer) reads the
    // same typed dispatch, so a rebrand of the singular-plural
    // resolution shape lands at exactly one caixa-core edit and
    // reaches every consumer by construction. Peer of the
    // sibling [`caixa_core::Entrada::resolved_paths`] (1449891)
    // path-list resolver on the per-HTTPRoute per-rule path axis.
    listener.insert_string(GATEWAY_API_KEY_HOSTNAME, entrada.hostname().to_string());
    let mut g_spec = serde_yaml::Mapping::new();
    // `spec.gatewayClassName` binds the emitted `Gateway` to the
    // substrate's chosen K8s Gateway API controller — the same Cilium
    // eBPF-identity data plane the sibling `cilium_network_policies`
    // renderer emits `CiliumNetworkPolicy` documents against, closing
    // the mesh-composition "one identity layer, one data plane"
    // invariant (MESH-COMPOSITION.md §V). The controller-choice value
    // threads through the lifted [`DEFAULT_GATEWAY_CLASS_NAME`]
    // re-export so a future substrate-side controller migration
    // (Cilium → Envoy Gateway / Istio Gateway / any per-edition
    // Gateway API v1.x GA controller variant) lands at the canonical
    // [`caixa_core::DEFAULT_GATEWAY_CLASS_NAME`] declaration, not at
    // this call site — same discipline the [`DEFAULT_NAMESPACE`] /
    // [`GATEWAY_API_API_VERSION`] / [`GATEWAY_API_KIND_GATEWAY`]
    // lifts apply on the peer canonical-K8s-Gateway-API-axis surfaces.
    g_spec.insert_string(
        GATEWAY_API_KEY_GATEWAY_CLASS_NAME,
        DEFAULT_GATEWAY_CLASS_NAME,
    );
    g_spec.insert_singleton_mapping_sequence(GATEWAY_API_KEY_LISTENERS, listener);
    gateway.insert_mapping(KUBE_KEY_SPEC, g_spec);

    // HTTPRoute — all paths route to the entrada.destination() Servico.
    // Same skeleton lift as Gateway above; caller adds spec. Both halves
    // of the `(apiVersion, kind)` CRD-lookup tuple now thread through
    // their matching lifted [`GATEWAY_API_API_VERSION`] +
    // [`GATEWAY_API_KIND_HTTP_ROUTE`] re-exports so a future Gateway-API
    // rebrand on either axis lands on the canonical
    // [`caixa_core::GATEWAY_API_KIND_HTTP_ROUTE`] declaration, not at
    // this call site — peer with the sibling Gateway skeleton above on
    // the canonical-Gateway-API-CRD-`kind`-discriminator surface. The
    // HTTPRoute `metadata.name` axis now threads through the lifted
    // [`gateway_api_http_route_name`] composer (peer of the
    // [`cilium_network_policy_name`] composer that carries the
    // sibling per-`(:de, :para)` CNP name on the shared
    // "aplicacao-prefixed sub-identity" discipline) so a future
    // per-Aplicacao Gateway API per-CR name-encoding rebrand lands at
    // one lifted composer and reaches this call site by construction.
    // The destination-Servico discriminator arg now routes through
    // the lifted [`caixa_core::Entrada::destination`] typed accessor
    // on the substrate primitive so the HTTPRoute `metadata.name`
    // discriminator and the peer per-rule `backendRefs[0].name` (see
    // the sibling consumer below in this same emitter) both key off
    // exactly one typed dispatch — a future extension of the `:entrada`
    // slot to a multi-destination author surface (weighted canary
    // backends, per-path override, an M4 `mesh.pleme.io/v1alpha1/
    // Aplicacao` CR materializer's admission-webhook that promotes the
    // scalar to a weighted list) reaches both consumers by construction
    // rather than by a coordinated inline-copy rewrite. Prior to this
    // lift the site inlined a verbatim `format!("{}-{}", caixa.nome,
    // entrada.para)` with no compile-time link to the peer CNP-name
    // composer's naming discipline; a rebrand would have had to be
    // threaded through this site and the in-file test-side
    // `httproute_carries_canonical_kube_skeleton_without_labels`
    // probe's `Some("checkout-cart")` byte-shape pin in lockstep or
    // the HTTPRoute `metadata.name` would have silently split from the
    // operator-side `kubectl get httproute -n tatara-system
    // <aplicacao>-<destination>` grep-by-name lookup encoding.
    // The HTTPRoute `metadata.name` identity byte-string derives from
    // the parent-Aplicacao Caixa's `:nome` verbatim through the
    // substrate-canonical [`caixa_core::gateway_api_http_route_name`]
    // composer. Routing the aplicacao-name arg through the typed
    // [`caixa_core::Caixa::nome`] accessor (`caixa.nome()`) rather than
    // the raw `&caixa.nome` `&String`-borrow of the underlying field
    // extends the same "one typed dispatch on the substrate primitive"
    // discipline onto the HTTPRoute-name axis every operator-side
    // `kubectl -n tatara-system get httproute <aplicacao>-<destination>`
    // grep-by-name lookup consults — peer of the sibling Gateway
    // `metadata.name` converge above and the co-resident CNP
    // `metadata.name` composer converge in [`cilium_network_policies`].
    let mut route = kube_resource_skeleton(
        GATEWAY_API_API_VERSION,
        GATEWAY_API_KIND_HTTP_ROUTE,
        &gateway_api_http_route_name(caixa.nome(), entrada.destination()),
        namespace,
        BTreeMap::new(),
    );

    let mut parent_ref = serde_yaml::Mapping::new();
    parent_ref.insert_string(GATEWAY_API_KEY_NAME, caixa.nome().to_string());
    // Per-parentRef listener-selector sub-axis — pins the emitted
    // `HTTPRoute` to the parent Gateway's sole HTTP listener by name,
    // rather than accepting the Gateway API v1 default
    // attach-to-every-listener fan-out. Both halves of the substrate's
    // canonical per-listener identity pair — the Gateway's
    // `spec.listeners[].name` (emitted a few lines above through the
    // `listener.insert(GATEWAY_API_KEY_NAME, GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME)`
    // call) and the HTTPRoute's `spec.parentRefs[].sectionName` (this
    // call) — now thread through the same lifted
    // [`GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME`] `&'static str`
    // constant, so a substrate-side rebrand of the canonical listener-
    // name identifier (`"http" → "http-v1"` on the multi-listener
    // HTTPS-by-default trajectory the paired
    // [`GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME`] docstring forecasts,
    // a per-cluster override the future `:entrada :listener-name`
    // slot promotes) reaches both sites by construction.
    //
    // Until this line landed the emitter omitted the selector
    // entirely, silently accepting the Gateway API v1
    // attach-to-every-listener default: a future substrate-side
    // second listener under the same parent Gateway (the
    // cert-manager-issued per-`:entrada :host` HTTPS listener the
    // sibling `GATEWAY_API_DEFAULT_HTTP_LISTENER_PORT` docstring
    // forecasts) would have silently doubled every route's dispatch
    // surface — every external `:entrada` request the route was
    // authored to accept on the substrate's canonical HTTP listener
    // would have accepted a matching request on the paired HTTPS
    // listener too, with the second-listener leak surfacing only in
    // per-request access logs (never in `kubectl describe httproute`
    // — the implicit fan-out reads as intended per the Gateway API v1
    // spec). Peer to the sibling `parent_ref.insert(GATEWAY_API_KEY_NAME,
    // …)` call on the same parent-Gateway attachment sub-container —
    // both per-parentRef sub-axes now name their target through the
    // canonical byte-string sourced from `caixa-core`, so a rebrand on
    // either axis reaches this consumer by construction.
    parent_ref.insert_string(
        GATEWAY_API_KEY_SECTION_NAME,
        GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME,
    );

    // Substrate-canonical per-`:entrada` URL-path resolver — routes
    // through the lifted [`caixa_core::Entrada::resolved_paths`] typed
    // method on the substrate primitive so the "empty `:entrada :paths`
    // → single [`GATEWAY_API_DEFAULT_HTTP_ROUTE_PATH`] catch-all;
    // non-empty → each declared path verbatim" cascade lives at one
    // typed dispatch on `Entrada` rather than inline here. Every
    // future HTTPRoute-aware consumer (the M4 `mesh.pleme.io/v1alpha1/
    // Aplicacao` CR materializer's per-rule path-list emit site, a
    // future per-cluster `:entrada :default-path` overlay resolver
    // the operator pins through a `:placement`-scoped slot, every
    // future per-Aplicacao snapshot renderer) reads from the same
    // typed dispatch, so a rebrand of the catch-all shape (a
    // hypothetical Gateway API v2 `Exact ""` migration, an operator-
    // pinned override, a per-controller variant that treats `"/"` as
    // a literal prefix rather than the catch-all) lands at exactly
    // one caixa-core edit and reaches every consumer by construction.
    // Peer of the sibling [`GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME`] /
    // [`GATEWAY_API_DEFAULT_HTTP_LISTENER_PORT`] lifts on the
    // per-Gateway per-listener substrate-canonical scalar-value axes.
    let paths: Vec<&str> = entrada.resolved_paths();
    // `:politicas :timeout` overlay — when the typed slot carries a
    // value it surfaces as a per-rule `timeouts: { request: <K8s
    // duration> }` block on every HTTPRoute rule, the canonical
    // Gateway API v1.x request-deadline shape:
    // https://gateway-api.sigs.k8s.io/api-types/httproute/#timeouts
    //
    // Until this overlay landed the typed `:politicas :timeout` slot
    // was inert at the cluster boundary — `AplicacaoSpec::validate`
    // refused zero values (PolicyTimeoutZero), but a non-zero timeout
    // never reached an emitted artifact. Wiring it through the
    // HTTPRoute renderer turns the MESH-COMPOSITION §V CSE invariant
    // ("every Aplicacao declares :politicas :timeout — no infinite
    // blocking") from a validate-time gate into a runtime-enforced
    // contract: the cluster's apiserver-side Gateway API parser will
    // refuse a malformed timeouts block, and the data plane (Envoy /
    // Cilium L7) will trip the per-call deadline at exactly the
    // configured duration.
    //
    // Duration → string formatting comes from the canonical
    // caixa_core::supervisor::duration_codec::render so a `30s`
    // typed-slot value renders to the same `"30s"` string K8s
    // tooling parses — no per-renderer ad-hoc duration formatting.
    let timeout_overlay =
        single_field_overlay(spec.politicas().timeout(), GATEWAY_API_KEY_REQUEST, |d| {
            serde_yaml::Value::String(caixa_core::supervisor::duration_codec::render(d))
        });
    // `:politicas :retries` overlay — when the typed slot carries a
    // value it surfaces as a per-rule `retry: { attempts: <N> }` block
    // on every HTTPRoute rule, the canonical Gateway API v1.2+
    // per-rule retry-policy shape:
    // https://gateway-api.sigs.k8s.io/api-types/httproute/#retry
    //
    // Same trajectory as the `:politicas :timeout` overlay above:
    // until this landed the typed `:retries` slot was inert past
    // [`AplicacaoSpec::validate`] (which refuses zero via
    // [`AplicacaoError::PolicyRetriesZero`]) — a non-zero attempt
    // count never reached an emitted artifact. Wiring it through the
    // HTTPRoute renderer turns the MESH-COMPOSITION §V CSE invariant
    // ("no infinite retrying without bound") into a runtime-enforced
    // contract: the cluster's data plane (Envoy / Cilium L7) caps
    // the retry budget at exactly the typed slot's value, so a
    // transient failure can't loop unbounded against a downstream.
    //
    // The overlay carries `attempts:` only — the typed slot is a
    // single-axis `Option<u32>`. Future axes the Gateway API exposes
    // (`codes:` for retryable status codes, `backoff:` for the
    // backoff window) are future `MeshPolicy` field additions + a
    // future `&& self.<axis>.is_none()` arm in
    // [`MeshPolicy::is_empty`] + a parallel arm here, not a
    // coordinated rewrite of this site.
    //
    // The retry cap projection routes through the substrate-canonical
    // per-`:politicas` [`caixa_core::MeshPolicy::retries`] typed
    // accessor (sibling of the peer per-`:politicas`
    // [`caixa_core::MeshPolicy::mtls_required`] accessor the CNP
    // `mtls_overlay` builder above keys off) — every downstream
    // consumer of the per-Aplicacao `:retries` axis reaches for
    // exactly one typed dispatch on the substrate primitive rather
    // than an open-coded `.retries` field access, so a future
    // extension of the axis (a per-`:contratos`-edge override overlay
    // the operator pins through a `:contratos :retries` slot the
    // MESH-COMPOSITION §III.2 #2 roadmap acknowledges, a per-cluster
    // retry-default overlay the M4 CR materializer resolves per-CR,
    // a promotion of the plain `u32` attempt-count to a richer
    // `{attempts, codes, backoff}` sub-block once the Gateway API
    // grows the peer `retry.codes` / `retry.backoff` axes) reaches
    // this consumer by construction.
    let retry_overlay = single_field_overlay(
        spec.politicas().retries(),
        GATEWAY_API_KEY_ATTEMPTS,
        |attempts| serde_yaml::Value::Number(attempts.into()),
    );
    let mut rules = Vec::with_capacity(paths.len());
    for path in paths {
        let mut path_match = serde_yaml::Mapping::new();
        path_match.insert_string(KUBE_KEY_TYPE, GATEWAY_API_PATH_MATCH_TYPE_PATH_PREFIX);
        path_match.insert_string(GATEWAY_API_KEY_VALUE, path.to_string());
        let mut match_entry = serde_yaml::Mapping::new();
        match_entry.insert_mapping(GATEWAY_API_KEY_PATH, path_match);
        let mut backend_ref = serde_yaml::Mapping::new();
        // Substrate-canonical per-`:entrada` destination-Servico
        // scalar — routes through the lifted
        // [`caixa_core::Entrada::destination`] typed accessor on the
        // substrate primitive so the per-HTTPRoute per-rule
        // `backendRefs[0].name` axis and the peer HTTPRoute
        // `metadata.name` discriminator arg (see the sibling consumer
        // in the [`kube_resource_skeleton`] call above in this same
        // emitter) both key off exactly one typed dispatch. A future
        // extension of the `:entrada` slot to a multi-destination
        // author surface (weighted canary backends, per-path override,
        // an M4 `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's
        // admission-webhook that promotes the scalar to a weighted list)
        // reaches both consumers by construction rather than by a
        // coordinated inline-copy rewrite — Gateway API v1.x
        // conformance requires the HTTPRoute's `backendRefs[]` to
        // reach a K8s Service the parent Gateway has permission to
        // route to; drift between the `metadata.name` grep-by-name
        // encoding and the `backendRefs[]` service-name reach breaks
        // the operator-side `kubectl get httproute` lookup encoding
        // far from any single-site commit.
        backend_ref.insert_string(GATEWAY_API_KEY_NAME, entrada.destination().to_string());
        // Substrate-canonical per-destination L4 port scalar — routes
        // through the lifted [`caixa_core::AplicacaoSpec::port_for_destination`]
        // typed dispatch on the substrate primitive so the per-HTTPRoute
        // per-rule `backendRefs[0].port` axis and the peer per-`(:de, :para)`
        // `CiliumNetworkPolicy` `toPorts[0].ports[0].port` axis (see the
        // sibling consumer at `cilium_network_policies` earlier in this
        // module — the sole other per-Aplicacao renderer that reaches for
        // a per-destination Servico TCP port scalar) both key off exactly
        // one typed dispatch. Prior to this lift the site inlined a
        // verbatim `entrada.port` field access, with no compile-time link
        // to the peer CNP-side resolver dispatch's naming discipline; a
        // future per-destination port axis extension (a per-`:contratos`
        // explicit `:port` slot the M4 typed-edge registry adds, a per-
        // `:membros` `:port` overlay once heterogeneous per-Servico
        // listener ports land, a per-cluster override the operator pins
        // through a future `:placement :default-port` slot, the M4
        // `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's per-CR
        // admission-webhook floor) would have had to be threaded through
        // this inline field access and every future per-Aplicacao
        // renderer's inline copy in lockstep, or the HTTPRoute
        // `backendRefs[].port` would have silently split from the peer
        // CNP L4 whitelist port — Gateway API v1.x forwards the request
        // to the destination Servico's Service on the emitted `port:`,
        // while Cilium's per-L4 policy filter drops any flow whose
        // destination port doesn't match the CNP whitelist, so a two-
        // consumer split silently blackholes every external `:entrada`
        // flow at the eBPF data plane far from the source `caixa.lisp`
        // with no field naming the port-drift root cause in the
        // emitted YAML. Peer with the sibling `entrada.destination()`
        // consumer on the immediately-preceding `backend_ref.insert_
        // string(GATEWAY_API_KEY_NAME, …)` call — both per-`:entrada`
        // backendRef scalar axes now route through their own typed
        // dispatch on the substrate primitive, so a future rebrand on
        // either axis reaches this consumer by construction. Same
        // discipline the peer `cilium_network_policies` L4 port resolver
        // at caixa-mesh/src/lib.rs:2663 established (9ca4896) on the
        // sibling per-`(:de, :para)` CNP consumer.
        backend_ref.insert_number(
            KUBE_KEY_PORT,
            spec.port_for_destination(entrada.destination()),
        );
        let mut rule = serde_yaml::Mapping::new();
        rule.insert_singleton_mapping_sequence(GATEWAY_API_KEY_MATCHES, match_entry);
        rule.insert_singleton_mapping_sequence(GATEWAY_API_KEY_BACKEND_REFS, backend_ref);
        rule.insert_str_key_if_some(GATEWAY_API_KEY_TIMEOUTS, timeout_overlay.as_ref());
        rule.insert_str_key_if_some(GATEWAY_API_KEY_RETRY, retry_overlay.as_ref());
        rules.push_mapping(rule);
    }

    let mut r_spec = serde_yaml::Mapping::new();
    r_spec.insert_singleton_mapping_sequence(GATEWAY_API_KEY_PARENT_REFS, parent_ref);
    // Substrate-canonical per-`:entrada` DNS-hostname plural
    // resolver — routes through the lifted
    // [`caixa_core::Entrada::hostnames`] typed accessor on the
    // substrate primitive so the per-HTTPRoute plural
    // `spec.hostnames[]` filter list and the peer parent-Gateway
    // per-listener singular `hostname:` filter (see the sibling
    // [`caixa_core::Entrada::hostname`] consumer above in this
    // same `gateway_routes` emitter) both key off exactly one
    // typed dispatch on the substrate primitive. The pair-
    // invariant `hostnames() == vec![hostname()]` pinned in
    // [`caixa_core::aplicacao::tests::hostnames_returns_singleton_of_hostname_accessor`]
    // keeps the two axes in lockstep by construction, so a
    // Gateway API v1.x `Accepted:False/NoMatchingParent` reject
    // at HTTPRoute-attach time (the parent Gateway's listener
    // hostname doesn't intersect the route's hostname filter
    // list) is a caixa-core-build-time failure rather than a
    // cluster-apply-time surprise. Peer of the sibling
    // [`caixa_core::Entrada::resolved_paths`] (1449891) path-list
    // resolver on the per-HTTPRoute per-rule path axis.
    r_spec.insert_sequence(
        GATEWAY_API_KEY_HOSTNAMES,
        entrada
            .hostnames()
            .into_iter()
            .map(|h| serde_yaml::Value::String(h.to_string()))
            .collect(),
    );
    r_spec.insert_sequence(KUBE_KEY_RULES, rules);
    route.insert_mapping(KUBE_KEY_SPEC, r_spec);

    Ok(vec![
        serde_yaml::Value::Mapping(gateway),
        serde_yaml::Value::Mapping(route),
    ])
}

/// One-shot bundle that renders every cluster artifact for an Aplicacao:
///
///   - programs.yaml entries (one per `:membros`)
///   - Cilium NetworkPolicies (one per `:contratos`)
///   - Gateway + HTTPRoute (when `:entrada` is set)
///
/// Returned as a flat `Vec<Value>` of YAML documents, suitable for
/// concatenation into a single multi-doc YAML file (the canonical
/// `feira app deploy` write target).
pub fn render_all(caixa: &Caixa) -> Result<Vec<serde_yaml::Value>, Error> {
    let mut out = Vec::new();
    out.extend(programs_for_aplicacao(caixa)?);
    out.extend(cilium_network_policies(caixa)?);
    out.extend(gateway_routes(caixa)?);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use caixa_core::{
        Caixa, CaixaKind, DEFAULT_SERVICO_PORT, Entrada, GATEWAY_API_DEFAULT_HTTP_ROUTE_PATH,
        LABEL_PROGRAM, M3_PLACEMENT_KEY_AFFINITY, M3_PLACEMENT_KEY_CLUSTERS,
        M3_PLACEMENT_KEY_ESTRATEGIA, M3_PLACEMENT_KEY_SHARD_KEY, Membro, MeshPolicy, Placement,
        PlacementStrategy, WitContract, find_by_kind, find_by_name, kube_api_version_is, kube_kind,
        kube_kind_is, kube_metadata, kube_metadata_label, kube_metadata_label_is,
        kube_metadata_labels, kube_name, kube_name_is, kube_namespace, kube_spec, kube_spec_field,
        kube_spec_str_field,
    };
    use std::time::Duration;

    fn aplicacao_caixa() -> Caixa {
        Caixa {
            nome: "checkout".into(),
            versao: "0.1.0".into(),
            kind: CaixaKind::Aplicacao,
            edicao: Some("2026".into()),
            descricao: Some("Checkout flow.".into()),
            repositorio: Some("github:pleme-io/checkout".into()),
            licenca: Some("MIT".into()),
            autores: vec!["pleme-io".into()],
            etiquetas: vec!["checkout".into()],
            deps: vec![],
            deps_dev: vec![],
            exe: vec![],
            bibliotecas: vec![],
            servicos: vec![],
            limits: None,
            behavior: None,
            upgrade_from: vec![],
            estrategia: None,
            max_restarts: None,
            restart_window: None,
            children: vec![],
            membros: vec![
                Membro {
                    caixa: "catalog".into(),
                    versao: "^0.1".into(),
                },
                Membro {
                    caixa: "cart".into(),
                    versao: "^0.1".into(),
                },
                Membro {
                    caixa: "payment".into(),
                    versao: "^0.2".into(),
                },
            ],
            contratos: vec![
                WitContract {
                    de: "cart".into(),
                    para: "catalog".into(),
                    wit: "wasi:http/proxy".into(),
                    endpoint: Some("/products/:id".into()),
                    subject: None,
                    slot: None,
                },
                WitContract {
                    de: "cart".into(),
                    para: "payment".into(),
                    wit: "wasi:http/proxy".into(),
                    endpoint: Some("/charge".into()),
                    subject: None,
                    slot: None,
                },
            ],
            politicas: Some(MeshPolicy {
                timeout: Some(Duration::from_secs(30)),
                retries: Some(3),
                mtls_required: Some(true),
                ..Default::default()
            }),
            placement: Some(Placement {
                estrategia: PlacementStrategy::Replicated,
                clusters: vec!["rio".into(), "mar".into()],
                affinity: Some("data-locality".into()),
                shard_key: None,
            }),
            entrada: Some(Entrada {
                host: "checkout.quero.cloud".into(),
                para: "cart".into(),
                paths: vec!["/api/cart".into()],
                port: 8080,
            }),
            ci: None,
        }
    }

    #[test]
    fn default_namespace_re_export_points_at_caixa_core_canonical() {
        // The renderer's `pub const DEFAULT_NAMESPACE` (with its prior
        // doc-comment explicitly acknowledging the duplication —
        // "Mirrors `caixa_flux::DEFAULT_NAMESPACE`") was lifted to a
        // re-export of [`caixa_core::DEFAULT_NAMESPACE`] so the
        // namespace string lives in exactly one place across every
        // caixa renderer. Pin the equality here so any local re-
        // introduction of a sibling `pub const DEFAULT_NAMESPACE: &str
        // = "…"` is a build-time test failure naming the offending
        // drift, not a silent apply-time symptom — the prior shape
        // would have let a rebrand on the caixa-flux side without a
        // coordinated caixa-mesh edit silently land Servicos at one
        // namespace and their Aplicacao's CiliumNetworkPolicy /
        // Gateway / HTTPRoute objects at the drifted other, with
        // every L7 contrato flow dropping at apply time because the
        // policy's `endpointSelector` matched no pods in its emit
        // namespace. Peer to
        // `caixa_flux::tests::default_namespace_re_export_points_at_caixa_core_canonical`
        // on the sibling renderer crate.
        caixa_core::assert_str_reexport_identity(
            "DEFAULT_NAMESPACE",
            DEFAULT_NAMESPACE,
            caixa_core::DEFAULT_NAMESPACE,
        );
    }

    #[test]
    fn contrato_edge_label_separator_re_export_points_at_caixa_core_canonical() {
        // The renderer's `CONTRATO_EDGE_LABEL_SEPARATOR` was lifted from
        // two verbatim inline `format!("{de}-to-{para}")` +
        // `format!("{}-{}-to-{}", caixa.nome, de, para)` sites at the
        // `cilium_network_policies` per-`(:de, :para)` group's
        // [`LABEL_CONTRATO`] `labels.insert(...)` call and the peer
        // `kube_resource_skeleton` `name:` argument to a re-export of
        // [`caixa_core::CONTRATO_EDGE_LABEL_SEPARATOR`] so the load-
        // bearing `-to-` byte-string lives in exactly one place across
        // every caixa renderer. Pin the equality + static-data identity
        // here so any local re-introduction of a sibling `pub const
        // CONTRATO_EDGE_LABEL_SEPARATOR: &str = "…"` (the canonical
        // drift footgun where a sibling local `pub const` could happen
        // to carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test failure
        // naming the offending drift, not a silent apply-time symptom —
        // the prior shape would have let an edge-encoding rebrand on
        // the caixa-mesh side without a coordinated caixa-core edit
        // silently split the CNP `metadata.name` from its own
        // `metadata.labels.pleme.pleme.io/contrato` value, orphaning
        // every operator-side `kubectl get cnp -l pleme.pleme.io/
        // contrato=<de>-to-<para>` grep-by-label query at apply time.
        // Peer to
        // [`default_namespace_re_export_points_at_caixa_core_canonical`]
        // on the sibling re-export axis.
        caixa_core::assert_str_reexport_identity(
            "CONTRATO_EDGE_LABEL_SEPARATOR",
            CONTRATO_EDGE_LABEL_SEPARATOR,
            caixa_core::CONTRATO_EDGE_LABEL_SEPARATOR,
        );
    }

    #[test]
    fn contrato_edge_label_re_export_matches_caixa_core_canonical_output() {
        // The renderer's `contrato_edge_label` was lifted from the
        // verbatim inline `format!("{de}-to-{para}")` at the
        // `cilium_network_policies` per-`(:de, :para)` group's
        // [`LABEL_CONTRATO`] `labels.insert(...)` call to a re-export
        // of [`caixa_core::contrato_edge_label`]. Pin the output-shape
        // equality here on a representative fixture so any local re-
        // introduction of a sibling `pub fn contrato_edge_label(...)`
        // shadow at this crate is a build-time test failure. Both call
        // paths must resolve to the same canonical function through
        // `pub use`, so their outputs agree by construction.
        assert_eq!(
            contrato_edge_label("cart", "catalog"),
            caixa_core::contrato_edge_label("cart", "catalog"),
        );
        assert_eq!(contrato_edge_label("cart", "catalog"), "cart-to-catalog");
    }

    #[test]
    fn cilium_network_policy_name_re_export_matches_caixa_core_canonical_output() {
        // The renderer's `cilium_network_policy_name` was lifted from
        // the verbatim inline `format!("{}-{}-to-{}", caixa.nome, de,
        // para)` at the `cilium_network_policies` per-`(:de, :para)`
        // group's `kube_resource_skeleton` `name:` argument to a re-
        // export of [`caixa_core::cilium_network_policy_name`]. Pin the
        // output-shape equality here on a representative fixture so any
        // local re-introduction of a sibling `pub fn
        // cilium_network_policy_name(...)` shadow at this crate is a
        // build-time test failure. Peer with
        // [`contrato_edge_label_re_export_matches_caixa_core_canonical_output`]
        // on the sibling composer axis — the CNP metadata.name
        // composes on [`contrato_edge_label`], so a drift on either
        // half would silently split the per-CNP identity pair
        // `(metadata.labels.pleme.pleme.io/contrato, metadata.name)`
        // at emit time.
        assert_eq!(
            cilium_network_policy_name("checkout", "cart", "catalog"),
            caixa_core::cilium_network_policy_name("checkout", "cart", "catalog"),
        );
        assert_eq!(
            cilium_network_policy_name("checkout", "cart", "catalog"),
            "checkout-cart-to-catalog",
        );
    }

    #[test]
    fn cilium_network_policy_metadata_name_uses_lifted_composer() {
        // Composition pin: the CNP `metadata.name` emitted by
        // `cilium_network_policies` per `(:de, :para)` group must
        // byte-equal the output of the lifted
        // [`cilium_network_policy_name`] composer with the same
        // arguments — so a future refactor of the composer's internals
        // (edge-encoding rebrand, aplicacao-prefix reshape) reaches
        // the renderer through the one function-pointer edit, and any
        // rewrite of the inline `format!` at the emit site that
        // desynchronizes from the composer fires here at build-time
        // rather than silently splitting the two writer-side axes at
        // emit time. The fixture's two `:contratos` edges (cart→catalog
        // and cart→payment) exercise both edges of the per-`(:de,
        // :para)` groups the CNP emitter produces.
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        let names: Vec<String> = policies
            .iter()
            .map(|p| kube_name(p).expect("policy metadata.name").to_string())
            .collect();
        assert!(
            names.contains(&cilium_network_policy_name("checkout", "cart", "catalog")),
            "CNP metadata.name for (cart, catalog) must match lifted composer output; \
             got names {names:?}",
        );
        assert!(
            names.contains(&cilium_network_policy_name("checkout", "cart", "payment")),
            "CNP metadata.name for (cart, payment) must match lifted composer output; \
             got names {names:?}",
        );
    }

    #[test]
    fn cilium_network_policy_label_contrato_value_uses_lifted_composer() {
        // Composition pin: the CNP `metadata.labels.pleme.pleme.io/
        // contrato` value emitted by `cilium_network_policies` per
        // `(:de, :para)` group must byte-equal the output of the
        // lifted [`contrato_edge_label`] composer with the same
        // arguments — so a future refactor of the composer's internals
        // (edge-encoding rebrand) reaches the label emission through
        // one function-pointer edit, and any rewrite of the inline
        // `format!` at the emit site that desynchronizes from the
        // composer fires here at build-time rather than silently
        // orphaning every operator-side grep-by-label query at apply
        // time. Peer with
        // [`cilium_network_policy_metadata_name_uses_lifted_composer`]
        // on the sibling per-CNP identity-pair axis — the two pins
        // together close the drift surface between
        // `metadata.labels.pleme.pleme.io/contrato` and
        // `metadata.name` on the shared
        // [`CONTRATO_EDGE_LABEL_SEPARATOR`] byte-string.
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        let contrato_values: Vec<String> = policies
            .iter()
            .filter_map(|p| kube_metadata_label(p, LABEL_CONTRATO).map(String::from))
            .collect();
        assert!(
            contrato_values.contains(&contrato_edge_label("cart", "catalog")),
            "CNP LABEL_CONTRATO value for (cart, catalog) must match lifted composer output; \
             got values {contrato_values:?}",
        );
        assert!(
            contrato_values.contains(&contrato_edge_label("cart", "payment")),
            "CNP LABEL_CONTRATO value for (cart, payment) must match lifted composer output; \
             got values {contrato_values:?}",
        );
    }

    #[test]
    fn gateway_api_http_route_name_re_export_matches_caixa_core_canonical_output() {
        // The renderer's `gateway_api_http_route_name` was lifted from
        // the verbatim inline `format!("{}-{}", caixa.nome,
        // entrada.para)` at the `gateway_routes`
        // `kube_resource_skeleton` `name:` argument to a re-export of
        // [`caixa_core::gateway_api_http_route_name`]. Pin the
        // output-shape equality here on a representative fixture so
        // any local re-introduction of a sibling `pub fn
        // gateway_api_http_route_name(...)` shadow at this crate is a
        // build-time test failure. Peer with
        // [`cilium_network_policy_name_re_export_matches_caixa_core_canonical_output`]
        // on the sibling per-Aplicacao per-CR K8s-name-shaped-identity-
        // scalar composer axis — the CNP-name composer carries the
        // per-`(:de, :para)` policy CR name and this composer carries
        // the per-`:entrada` route CR name.
        assert_eq!(
            gateway_api_http_route_name("checkout", "cart"),
            caixa_core::gateway_api_http_route_name("checkout", "cart"),
        );
        assert_eq!(
            gateway_api_http_route_name("checkout", "cart"),
            "checkout-cart",
        );
    }

    #[test]
    fn gateway_api_http_route_metadata_name_uses_lifted_composer() {
        // Composition pin: the HTTPRoute `metadata.name` emitted by
        // `gateway_routes` must byte-equal the output of the lifted
        // [`gateway_api_http_route_name`] composer with the same
        // arguments — so a future refactor of the composer's internals
        // (per-Aplicacao Gateway API per-CR name-encoding rebrand)
        // reaches the renderer through one function-pointer edit, and
        // any rewrite of the inline `format!` at the emit site that
        // desynchronizes from the composer fires here at build-time
        // rather than silently splitting the emitted HTTPRoute
        // `metadata.name` from the operator-side `kubectl get
        // httproute -n tatara-system <aplicacao>-<para>` grep-by-name
        // lookup encoding. Peer to
        // [`cilium_network_policy_metadata_name_uses_lifted_composer`]
        // on the sibling per-CR K8s-name-shaped-identity-scalar
        // composer axis.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let route = find_by_kind(&docs, GATEWAY_API_KIND_HTTP_ROUTE).expect("HTTPRoute present");
        assert_eq!(
            kube_name(route),
            Some(gateway_api_http_route_name("checkout", "cart").as_str()),
            "HTTPRoute metadata.name must match lifted composer output",
        );
    }

    #[test]
    fn gateway_api_api_version_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_API_VERSION` was lifted from two
        // inline `"gateway.networking.k8s.io/v1"` literals at the two
        // `gateway_routes` `kube_resource_skeleton` call sites
        // (caixa-mesh/src/lib.rs:455, 496 — the `Gateway` + `HTTPRoute`
        // CRD-group/version axis pair) to a re-export of
        // [`caixa_core::GATEWAY_API_API_VERSION`] so the Gateway-API-
        // conformant CRD-group/version string lives in exactly one
        // place across every caixa renderer. Pin the equality + static-
        // data identity here so any local re-introduction of a sibling
        // `pub const GATEWAY_API_API_VERSION: &str = "…"` (the canonical
        // drift footgun where a sibling local `pub const` could happen
        // to carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test failure
        // naming the offending drift, not a silent apply-time symptom —
        // the prior shape would have let a Gateway-API GA bump on the
        // caixa-mesh side without a coordinated caixa-core edit silently
        // land Gateway / HTTPRoute objects at one CRD version and every
        // future per-target renderer's emitted `Gateway` / `HTTPRoute` /
        // `TCPRoute` / `TLSRoute` / `GRPCRoute` at the drifted other,
        // with every external `:entrada` flow dropping at apply time
        // because the per-route attached-policy pipeline never binds
        // across the version-drifted CRD-group/version pair. Peer to
        // [`default_namespace_re_export_points_at_caixa_core_canonical`]
        // on the sibling re-export axis.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_API_VERSION",
            GATEWAY_API_API_VERSION,
            caixa_core::GATEWAY_API_API_VERSION,
        );
    }

    #[test]
    fn cilium_api_version_re_export_points_at_caixa_core_canonical() {
        // The renderer's `CILIUM_API_VERSION` was lifted from the inline
        // `"cilium.io/v2"` literal at the `cilium_network_policies`
        // `kube_resource_skeleton` call site (caixa-mesh/src/lib.rs:326 —
        // the per-`(:de, :para)` CiliumNetworkPolicy emit site) to a
        // re-export of [`caixa_core::CILIUM_API_VERSION`] so the
        // Cilium-CRD-group/version string lives in exactly one place
        // across every caixa renderer. Pin the equality + static-data
        // identity here so any local re-introduction of a sibling
        // `pub const CILIUM_API_VERSION: &str = "…"` (the canonical
        // drift footgun where a sibling local `pub const` could happen
        // to carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test failure
        // naming the offending drift, not a silent apply-time symptom —
        // the prior shape would have let a Cilium-CRD bump on the
        // caixa-mesh side without a coordinated caixa-core edit silently
        // land per-`(:de, :para)` CiliumNetworkPolicy objects at one CRD
        // version and every future per-target Cilium-side renderer's
        // emitted `CiliumClusterwideNetworkPolicy` /
        // `CiliumLocalRedirectPolicy` at the drifted other, with every
        // intra-mesh L4/L7 contrato flow dropping at apply time because
        // the per-policy attached-identity pipeline never binds across
        // the version-drifted CRD-group/version pair. Peer to
        // [`gateway_api_api_version_re_export_points_at_caixa_core_canonical`]
        // / [`default_namespace_re_export_points_at_caixa_core_canonical`]
        // on the sibling re-export axes.
        caixa_core::assert_str_reexport_identity(
            "CILIUM_API_VERSION",
            CILIUM_API_VERSION,
            caixa_core::CILIUM_API_VERSION,
        );
    }

    #[test]
    fn kube_key_spec_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_SPEC` was lifted from three inline
        // `"spec".into()` literals at the three K8s-CR top-level-spec
        // insertion sites (the `cilium_network_policies` per-`(:de,
        // :para)` `CiliumNetworkPolicy` skeleton, the `gateway_routes`
        // `Gateway` skeleton, the `gateway_routes` `HTTPRoute`
        // skeleton) to a re-export of [`caixa_core::KUBE_KEY_SPEC`] so
        // the canonical K8s-CR top-level spec-axis string lives in
        // exactly one place across every caixa renderer. Pin the
        // equality + static-data identity here so any local
        // re-introduction of a sibling `pub const KUBE_KEY_SPEC: &str
        // = "…"` (the canonical drift footgun where a sibling local
        // `pub const` could happen to carry the same string at the
        // source while pointing at a different `&'static` allocation)
        // is a build-time test failure naming the offending drift,
        // not a silent apply-time symptom. Peer to
        // [`gateway_api_api_version_re_export_points_at_caixa_core_canonical`]
        // / [`cilium_api_version_re_export_points_at_caixa_core_canonical`]
        // / [`default_namespace_re_export_points_at_caixa_core_canonical`]
        // on the sibling re-export axes.
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_SPEC",
            KUBE_KEY_SPEC,
            caixa_core::KUBE_KEY_SPEC,
        );
    }

    #[test]
    fn kube_key_metadata_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_METADATA` was lifted from eleven
        // inline `"metadata"` literals at the test-side K8s-CR
        // top-level-metadata-axis retrieval calls that navigate into
        // the metadata block of each rendered `CiliumNetworkPolicy` /
        // `Gateway` / `HTTPRoute` doc (per-policy name / namespace /
        // labels / mapping-shape / alphabetical-iteration axes) to a
        // re-export of [`caixa_core::KUBE_KEY_METADATA`] so the
        // canonical K8s-CR top-level metadata-axis string lives in
        // exactly one place across every caixa renderer. Pin the
        // equality + static-data identity here so any local
        // re-introduction of a sibling `pub const KUBE_KEY_METADATA:
        // &str = "…"` (the canonical drift footgun where a sibling
        // local `pub const` could happen to carry the same string at
        // the source while pointing at a different `&'static`
        // allocation) is a build-time test failure naming the
        // offending drift. Peer to
        // [`kube_key_spec_re_export_points_at_caixa_core_canonical`]
        // on the sibling K8s-CR top-level-spec-axis re-export +
        // `caixa_flux::tests::kube_key_metadata_re_export_points_at_caixa_core_canonical`
        // on the sibling renderer crate.
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_METADATA",
            KUBE_KEY_METADATA,
            caixa_core::KUBE_KEY_METADATA,
        );
    }

    #[test]
    fn kube_key_kind_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_KIND` was lifted from sixteen inline
        // `"kind"` literals at the test-side K8s-CR top-level-kind-axis
        // retrieval calls that navigate the multi-doc sequence the
        // `cilium_network_policies` / `gateway_routes` / `render_all`
        // emitters return to isolate the per-`(Gateway, HTTPRoute,
        // CiliumNetworkPolicy)` document (the `docs.iter().find(|d|
        // d.get("kind")…)` filter predicate + `for p in &policies {
        // p.get("kind")… }` iteration axes) to a re-export of
        // [`caixa_core::KUBE_KEY_KIND`] so the canonical K8s-CR
        // top-level kind-discriminator axis string lives in exactly one
        // place across every caixa renderer. Pin the equality +
        // static-data identity here so any local re-introduction of a
        // sibling `pub const KUBE_KEY_KIND: &str = "…"` (the canonical
        // drift footgun where a sibling local `pub const` could happen
        // to carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test
        // failure naming the offending drift, not a silent apply-time
        // symptom — the prior shape would have let a typo on any one
        // sibling `pub const` declaration silently miss the per-CR
        // kind retrieval so the multi-doc `.find(|d|
        // d.get(KUBE_KEY_KIND)…) == Some(…)` predicate the render-
        // determinism / kind-axis pins rest on would compare against
        // `None` and mask the true kind-axis drift under the trailing
        // `.expect("Gateway present")` panic. Peer to
        // [`kube_key_spec_re_export_points_at_caixa_core_canonical`] +
        // [`kube_key_metadata_re_export_points_at_caixa_core_canonical`]
        // on the sibling K8s-CR top-level-spec / top-level-metadata
        // axis re-exports — completes the per-K8s-CR top-level axis
        // re-export triple `(spec, metadata, kind)` the multi-doc
        // consumer patterns across this crate's test suite rest on.
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_KIND",
            KUBE_KEY_KIND,
            caixa_core::KUBE_KEY_KIND,
        );
    }

    #[test]
    fn kube_key_api_version_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_API_VERSION` was lifted from six
        // inline `"apiVersion"` literals at the test-side K8s-CR
        // top-level-apiVersion-axis retrieval calls that navigate the
        // rendered multi-doc sequence the `cilium_network_policies` /
        // `gateway_routes` emitters return to isolate each per-`(CNP,
        // Gateway, HTTPRoute)` document's top-level-apiVersion axis and
        // pin it to the lifted [`CILIUM_API_VERSION`] /
        // [`GATEWAY_API_API_VERSION`] controller-pair CRD-group/version
        // constants (the two `cilium_network_policies_use_lifted_cilium_api_version`
        // + `gateway_routes_gateway_uses_lifted_gateway_api_api_version`
        // + `gateway_routes_httproute_uses_lifted_gateway_api_api_version`
        // lifted-uses pins plus the three sibling
        // `cilium_policy_carries_canonical_kube_skeleton` /
        // `gateway_carries_canonical_kube_skeleton_without_labels` /
        // `httproute_carries_canonical_kube_skeleton_without_labels`
        // canonical-string bridge-arm pins), to a re-export of
        // [`caixa_core::KUBE_KEY_API_VERSION`] so the canonical K8s-CR
        // top-level apiVersion-axis string lives in exactly one place
        // across every caixa renderer. Pin the equality + static-data
        // identity here so any local re-introduction of a sibling `pub
        // const KUBE_KEY_API_VERSION: &str = "…"` (the canonical drift
        // footgun where a sibling local `pub const` could happen to
        // carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test failure
        // naming the offending drift, not a silent apply-time symptom —
        // the prior shape would have let a typo on any one sibling
        // `pub const` declaration silently miss the per-CR apiVersion
        // retrieval so the drift-detection
        // `.get(KUBE_KEY_API_VERSION).and_then(|n| n.as_str()) == Some(…)`
        // predicate the sibling [`CILIUM_API_VERSION`] /
        // [`GATEWAY_API_API_VERSION`] re-export pins rest on would
        // compare against `None` under the trailing
        // `.expect("Gateway present")` / `.expect("HTTPRoute present")`
        // panic and mask the true sibling controller-pair
        // CRD-group/version axis drift. Peer to
        // [`kube_key_spec_re_export_points_at_caixa_core_canonical`] +
        // [`kube_key_metadata_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_kind_re_export_points_at_caixa_core_canonical`]
        // on the sibling K8s-CR top-level-spec / top-level-metadata /
        // top-level-kind axis re-exports — completes the per-K8s-CR
        // top-level `(apiVersion, kind, metadata, spec)` axis re-export
        // quartet every rendered multi-doc mesh bundle document
        // navigates. Peer to
        // `caixa_flux::tests::kube_key_api_version_re_export_points_at_caixa_core_canonical`
        // (e0555d6) on the sibling renderer crate — extends the
        // discipline from the Flux v2 controller-triplet drift-
        // detection pins onto the Cilium + Gateway API controller-pair
        // drift-detection pins in this crate.
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_API_VERSION",
            KUBE_KEY_API_VERSION,
            caixa_core::KUBE_KEY_API_VERSION,
        );
    }

    #[test]
    fn kube_key_namespace_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_NAMESPACE` was lifted from three
        // inline `"namespace"` literals at the test-side K8s-CR
        // metadata.namespace-axis retrieval sites (the
        // `cilium_policy_carries_canonical_kube_skeleton` /
        // `gateway_carries_canonical_kube_skeleton_without_labels`
        // per-CR metadata.namespace equality pins and the
        // `cilium_policy_metadata_block_iterates_alphabetically`
        // render-determinism-contract fixture) to a re-export of
        // [`caixa_core::KUBE_KEY_NAMESPACE`] so the canonical K8s-CR
        // metadata.namespace-axis string lives in exactly one place
        // across every caixa renderer. Pin the equality + static-data
        // identity here so any local re-introduction of a sibling
        // `pub const KUBE_KEY_NAMESPACE: &str = "…"` (the canonical
        // drift footgun where a sibling local `pub const` could
        // happen to carry the same string at the source while
        // pointing at a different `&'static` allocation) is a
        // build-time test failure naming the offending drift, not a
        // silent apply-time symptom — the prior shape would have let
        // a typo on any one sibling `pub const` declaration silently
        // miss the per-CR metadata.namespace retrieval so the
        // `.get(KUBE_KEY_NAMESPACE).and_then(|n| n.as_str()) ==
        // Some(DEFAULT_NAMESPACE)` predicate the sibling
        // [`DEFAULT_NAMESPACE`] re-export pin rests on would compare
        // against `None` under the trailing `.expect("metadata
        // mapping")` panic and mask the true sibling default-
        // namespace-axis drift. Peer to
        // [`kube_key_spec_re_export_points_at_caixa_core_canonical`] +
        // [`kube_key_metadata_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_kind_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_api_version_re_export_points_at_caixa_core_canonical`]
        // on the sibling K8s-CR top-level `(apiVersion, kind, metadata,
        // spec)` axis re-export quartet — extends the discipline onto
        // the load-bearing nested `metadata.namespace` axis every
        // rendered `CiliumNetworkPolicy` / `Gateway` / `HTTPRoute`
        // document binds to on the deploy path. Peer to
        // `caixa_flux::tests::kube_key_namespace_re_export_points_at_caixa_core_canonical`
        // (44bebfe) on the sibling renderer crate — extends the
        // discipline from the Flux v2 controller-triplet + ComputeUnit-
        // side metadata.namespace drift-detection pins onto the
        // Cilium + Gateway API controller-pair metadata.namespace
        // drift-detection pins in this crate.
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_NAMESPACE",
            KUBE_KEY_NAMESPACE,
            caixa_core::KUBE_KEY_NAMESPACE,
        );
    }

    #[test]
    fn kube_key_labels_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_LABELS` was lifted from five inline
        // `"labels"` literals at the test-side K8s-CR metadata.labels-
        // axis retrieval sites (the `cilium_policy_metadata_labels_use_lifted_consts`
        // per-CNP metadata.labels retrieval entry point, the
        // `cilium_policy_carries_canonical_kube_skeleton` +
        // `gateway_carries_canonical_kube_skeleton_without_labels` +
        // `httproute_carries_canonical_kube_skeleton_without_labels`
        // presence-of-labels / empty-labels-skip semantic pins, and the
        // `cilium_policy_metadata_block_iterates_alphabetically`
        // render-determinism-contract fixture) to a re-export of
        // [`caixa_core::KUBE_KEY_LABELS`] so the canonical K8s-CR
        // metadata.labels-axis string lives in exactly one place
        // across every caixa renderer. Pin the equality + static-data
        // identity here so any local re-introduction of a sibling
        // `pub const KUBE_KEY_LABELS: &str = "…"` (the canonical drift
        // footgun where a sibling local `pub const` could happen to
        // carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test
        // failure naming the offending drift, not a silent apply-time
        // symptom — the prior shape would have let a typo on any one
        // sibling `pub const` declaration silently miss the per-CR
        // metadata.labels retrieval so the `.get(KUBE_KEY_LABELS)`
        // chain the LABEL_APLICACAO + LABEL_CONTRATO drift-detection
        // pin rests on would return `None` under the trailing
        // `.expect("policy metadata.labels mapping")` panic and mask
        // the true sibling label-key-axis drift, or the presence-of-
        // labels / empty-labels-skip semantic pins would compare
        // `Some(...)`/`None` against the wrong retrieval so the
        // empty-labels-skip contract's true drift never surfaces, or
        // the alphabetical-iteration render-determinism fixture would
        // fire on the drifted-fixture rather than the true render-
        // determinism property. Peer to
        // [`kube_key_spec_re_export_points_at_caixa_core_canonical`] +
        // [`kube_key_metadata_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_kind_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_api_version_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_namespace_re_export_points_at_caixa_core_canonical`]
        // on the sibling K8s-CR top-level `(apiVersion, kind,
        // metadata, spec)` axis re-export quartet + the load-bearing
        // nested `metadata.namespace` axis re-export — extends the
        // discipline onto the load-bearing nested `metadata.labels`
        // axis every rendered `CiliumNetworkPolicy` document carries
        // at the `pleme.pleme.io/aplicacao` + `pleme.pleme.io/contrato`
        // grouping key.
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_LABELS",
            KUBE_KEY_LABELS,
            caixa_core::KUBE_KEY_LABELS,
        );
    }

    #[test]
    fn kube_key_name_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_NAME` was lifted from four inline
        // `"name"` literals at the test-side K8s-CR metadata.name-axis
        // retrieval sites (the `cilium_policy_carries_canonical_kube_skeleton`
        // + `gateway_carries_canonical_kube_skeleton_without_labels` +
        // `httproute_carries_canonical_kube_skeleton_without_labels`
        // per-CR metadata.name presence + equality pins, and the
        // `cilium_policy_metadata_block_iterates_alphabetically`
        // render-determinism-contract fixture) to a re-export of
        // [`caixa_core::KUBE_KEY_NAME`] so the canonical K8s-CR
        // metadata.name-axis string lives in exactly one place across
        // every caixa renderer. Pin the equality + static-data identity
        // here so any local re-introduction of a sibling
        // `pub const KUBE_KEY_NAME: &str = "…"` (the canonical drift
        // footgun where a sibling local `pub const` could happen to
        // carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test failure
        // naming the offending drift, not a silent apply-time symptom
        // — the prior shape would have let a typo on any one sibling
        // `pub const` declaration silently miss the per-CR
        // metadata.name retrieval so the caixa-nome → metadata-name
        // binding's true drift is masked, or trip the alphabetical-
        // iteration determinism fixture against the drifted-fixture
        // rather than the true render-determinism property. Bridge-arm
        // peer to [`kube_key_spec_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_metadata_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_kind_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_api_version_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_namespace_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_labels_re_export_points_at_caixa_core_canonical`]
        // — completes the K8s-CR metadata-block axis triplet `(name,
        // namespace, labels)` bridge-arm pin under a single canonical
        // `caixa-core::KUBE_KEY_*` re-export shape in this crate.
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_NAME",
            KUBE_KEY_NAME,
            caixa_core::KUBE_KEY_NAME,
        );
    }

    #[test]
    fn gateway_api_key_name_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_KEY_NAME` was lifted from four
        // inline `"name"` literals at the Gateway API v1 per-child-
        // object name-reference-axis emission + retrieval sites (the
        // per-listener `listener.insert("name", …)`, the per-parentRef
        // `parent_ref.insert("name", …)`, and the per-backendRef
        // `backend_ref.insert("name", …)` calls in `gateway_routes`,
        // plus the in-file `httproute_routes_to_entrada_para` fixture's
        // per-backendRef `.get("name")` retrieval) to a re-export of
        // [`caixa_core::GATEWAY_API_KEY_NAME`] so the canonical
        // Gateway-API-v1-per-child-object name-reference-axis string
        // lives in exactly one place across every caixa renderer.
        //
        // Pin the equality + static-data identity here so any local
        // re-introduction of a sibling
        // `pub const GATEWAY_API_KEY_NAME: &str = "…"` (the canonical
        // drift footgun where a sibling local `pub const` could happen
        // to carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test
        // failure naming the offending drift, not a silent apply-time
        // symptom — the prior shape would have let a typo on any one
        // sibling `pub const` declaration silently miss the per-
        // listener / per-parentRef / per-backendRef name-reference
        // retrieval so the Aplicacao gateway bundle's true drift is
        // masked (the Gateway API implementation's per-listener
        // section identity resolves to nothing, the per-HTTPRoute
        // parent-Gateway attachment reconciles as unbound, or the
        // per-rule backend fan-out resolves no Service — all at
        // apply time, far from the source caixa.lisp with no field
        // naming the name-reference-axis drift).
        //
        // Byte-identical to [`KUBE_KEY_NAME`] today — both resolve to
        // the same three-byte `"name"` literal — but semantically
        // distinct: `KUBE_KEY_NAME` names the K8s CR canonical
        // `metadata.name` outer-level identity axis (every rendered
        // CR's own name), while `GATEWAY_API_KEY_NAME` names the
        // Gateway API v1 CRD schema's per-child-object name-reference
        // axis on `Listener` / `ParentReference` / `BackendObjectReference`
        // sub-schemas. Splitting the two lets each schema's future
        // rebrand land independently at its canonical const definition
        // — a future Gateway API v2 rename of the name-reference axis
        // to `target` / `ref` / `objectName` cannot coincidentally
        // rebrand the K8s CR canonical `metadata.name` axis, and vice
        // versa — the same discipline
        // [`caixa_core::FLEET_PROGRAMS_KEY_NAME`] establishes vs.
        // [`caixa_core::KUBE_KEY_NAME`] on the `lareira-fleet-programs`
        // values-schema per-entry name-axis.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_KEY_NAME",
            GATEWAY_API_KEY_NAME,
            caixa_core::GATEWAY_API_KEY_NAME,
        );
        // The re-export is byte-identical to `KUBE_KEY_NAME` today; pin
        // the value-equality so a future rebrand on either axis (the
        // K8s CR canonical `metadata.name` axis moving to a namespaced
        // key, or the Gateway API v1 per-child-object name-reference
        // axis moving to `target` / `ref` / `objectName`) surfaces here
        // as an explicit split rather than a silent coupling.
        assert_eq!(GATEWAY_API_KEY_NAME, "name");
    }

    #[test]
    fn kube_key_match_labels_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_MATCH_LABELS` was lifted from four
        // inline `"matchLabels"` literals at the test-side
        // K8s-`LabelSelector.matchLabels` retrieval sites (the
        // `cilium_policies_are_identity_based` destination-
        // `endpointSelector.matchLabels` presence pin +
        // source-`ingress[0].fromEndpoints[0].matchLabels` two-axis
        // pin, the `cilium_endpoint_selector_is_program_only`
        // destination-`endpointSelector.matchLabels` retrieval whose
        // `selector.len() == 1` assertion pins the program-only
        // semantic the canonical `pleme_program_selector` helper
        // emits, and the
        // `cilium_from_endpoints_carries_aplicacao_scoped_selector`
        // source-`fromEndpoints[0].matchLabels` retrieval whose
        // `from.len() == 2` assertion pins the program-in-Aplicacao-
        // scoped semantic the canonical
        // `pleme_program_in_aplicacao_selector` helper emits — the
        // safety property that a same-named program in a different
        // Aplicacao cannot satisfy the policy's ingress rule) to a
        // re-export of [`caixa_core::KUBE_KEY_MATCH_LABELS`] so the
        // canonical K8s-`LabelSelector.matchLabels`-axis string lives
        // in exactly one place across every caixa renderer. Pin the
        // equality + static-data identity here so any local
        // re-introduction of a sibling `pub const KUBE_KEY_MATCH_LABELS:
        // &str = "…"` (the canonical drift footgun where a sibling
        // local `pub const` could happen to carry the same string at
        // the source while pointing at a different `&'static`
        // allocation) is a build-time test failure naming the
        // offending drift, not a silent apply-time symptom — the
        // prior shape would have let a typo on any one sibling `pub
        // const` declaration silently miss the per-CR selector-
        // mapping retrieval so the destination-program-only /
        // source-program-in-Aplicacao selector-shape contract's true
        // drift is masked, or fire the trailing
        // `.expect("endpointSelector.matchLabels mapping")` /
        // `.expect("fromEndpoints[0].matchLabels mapping")` panic-
        // message tag with the mapping-shape message rather than the
        // true selector-key drift. Bridge-arm peer to
        // [`kube_key_spec_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_metadata_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_kind_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_api_version_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_namespace_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_labels_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_name_re_export_points_at_caixa_core_canonical`]
        // — extends the K8s-CR top-level `(apiVersion, kind,
        // metadata, spec)` axis re-export quartet + the load-bearing
        // nested `metadata.{name, namespace, labels}` triplet bridge-
        // arm pin under a single canonical `caixa-core::KUBE_KEY_*`
        // re-export shape in this crate onto the load-bearing nested
        // `LabelSelector.matchLabels` axis every rendered
        // `CiliumNetworkPolicy` document carries at both
        // `spec.endpointSelector.matchLabels` (the destination-
        // identity selector the Cilium data plane matches pod-
        // identity keys against) and
        // `spec.ingress[*].fromEndpoints[*].matchLabels` (the source-
        // identity selector the same data plane checks on the
        // admitted-source side).
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_MATCH_LABELS",
            KUBE_KEY_MATCH_LABELS,
            caixa_core::KUBE_KEY_MATCH_LABELS,
        );
    }

    #[test]
    fn kube_key_rules_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_RULES` was lifted from seven inline
        // `"rules"` literals — two production emitter sites
        // (`cilium_network_policies`'s per-`toPorts[]` L7 `rules:`
        // mapping the Cilium data plane dispatches HTTP / Kafka / DNS
        // L7 rules under, `gateway_routes`'s `HTTPRoute` `spec.rules[]`
        // sequence the gateway-class-controller dispatches per-rule
        // `matches[]` + `backendRefs[]` + timeouts / retries overlay
        // under) and five test-side rule-list traversal sites (the
        // `httproute_carries_paths_from_http_endpoints` /
        // `cilium_l7_rules_are_http_only` L7-rule-content pins under
        // `toPorts[]`, the `cilium_pubsub_contracts_skip_l7_rules`
        // absence pin whose `.is_none()` guards the pubsub-contracts-
        // carry-no-L7-rules contract, the
        // `gateway_emits_gateway_plus_httproute_pair` HTTPRoute-
        // backendRefs-shape pin under `spec`, and the `httproute_rules`
        // test-fixture helper the downstream policy-timeout / retries
        // / mtls / rate-limit determinism pins reach through) — to a
        // re-export of [`caixa_core::KUBE_KEY_RULES`] so the canonical
        // K8s-CR-`rules`-collection-axis string lives in exactly one
        // place across every caixa renderer. Pin the equality + static-
        // data identity here so any local re-introduction of a sibling
        // `pub const KUBE_KEY_RULES: &str = "…"` (the canonical drift
        // footgun where a sibling local `pub const` could happen to
        // carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test failure
        // naming the offending drift, not a silent apply-time symptom
        // — the prior shape would have let a typo on any one sibling
        // `pub const` declaration silently miss the per-CR rule-list
        // retrieval so the L7-rules-absent-on-pubsub-contracts contract
        // (`cilium_pubsub_contracts_skip_l7_rules`), the
        // HTTPRoute-rule-sequence-under-spec contract (`httproute_rules`
        // fixture + every determinism pin downstream), and the L7-rule-
        // path-content contract
        // (`httproute_carries_paths_from_http_endpoints` /
        // `cilium_l7_rules_are_http_only`) true drift is masked, or
        // fire the trailing `.expect("HTTPRoute spec.rules sequence")`
        // panic-message tag with the sequence-shape message rather than
        // the true rule-list-key drift. Bridge-arm peer to
        // [`kube_key_match_labels_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_spec_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_metadata_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_kind_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_api_version_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_namespace_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_labels_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_name_re_export_points_at_caixa_core_canonical`]
        // — extends the K8s-CR top-level `(apiVersion, kind, metadata,
        // spec)` axis re-export quartet + the load-bearing nested
        // `metadata.{name, namespace, labels}` triplet + the load-
        // bearing nested `LabelSelector.matchLabels` selector-projection
        // axis under a single canonical `caixa-core::KUBE_KEY_*`
        // re-export shape in this crate onto the load-bearing nested
        // `spec.rules[]` / `toPorts[].rules` rule-list-container axis
        // every rendered `CiliumNetworkPolicy` L7 rule-list + every
        // rendered `HTTPRoute` rule-list carries.
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_RULES",
            KUBE_KEY_RULES,
            caixa_core::KUBE_KEY_RULES,
        );
    }

    #[test]
    fn kube_key_port_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_PORT` was lifted from five inline
        // `"port"` literals — three production emitter sites
        // (`cilium_network_policies`'s per-`toPorts[].ports[]` port-tuple
        // `port:` scalar the Cilium data plane's per-tuple bpf policy
        // dispatch loop compares against the observed TCP/UDP L4 header
        // port value, `gateway_routes`'s per-`Gateway` per-listener
        // `spec.listeners[].port` scalar the gateway-class-controller's
        // per-listener bind loop opens the listener socket on,
        // `gateway_routes`'s per-`HTTPRoute` per-rule
        // `spec.rules[].backendRefs[].port` scalar the gateway-class-
        // controller's per-rule backend-dispatch loop forwards the
        // matched request to on the resolved Service / ExternalName
        // backend) and two test-side L4-port traversal sites (the
        // `gateway_emits_gateway_plus_httproute_pair` `.get("port")`
        // under `backendRefs[]` HTTPRoute-backend-port-content pin, the
        // `cilium_l4_ports_default_to_servico_port` `.get("port")` under
        // `toPorts[].ports[]` L7-fallback-port-content pin threading
        // through [`DEFAULT_SERVICO_PORT`]) — to a re-export of
        // [`caixa_core::KUBE_KEY_PORT`] so the canonical
        // K8s-CR-`port`-L4-scalar-axis string lives in exactly one place
        // across every caixa renderer. Pin the equality + static-data
        // identity here so any local re-introduction of a sibling `pub
        // const KUBE_KEY_PORT: &str = "…"` (the canonical drift footgun
        // where a sibling local `pub const` could happen to carry the
        // same string at the source while pointing at a different
        // `&'static` allocation) is a build-time test failure naming
        // the offending drift, not a silent apply-time symptom — the
        // prior shape would have let a typo on any one sibling `pub
        // const` declaration silently miss the per-CR L4-port
        // retrieval (the L7-fallback-port-content pin's
        // `.expect("toPorts[0].ports[0].port present")` panic-message
        // tag would fire against the sequence-shape message rather than
        // the true L4-port-key drift, the HTTPRoute-backend-port-content
        // pin's `assert_eq!(…, Some(8080))` would silently mask the
        // drift under the `None` unwrap-default), or silently emit a
        // malformed CR whose port field the apiserver-side CRD schema
        // validator drops as unrecognized at apply time. Bridge-arm
        // peer to
        // [`kube_key_rules_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_match_labels_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_spec_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_metadata_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_kind_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_api_version_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_namespace_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_labels_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_name_re_export_points_at_caixa_core_canonical`]
        // — extends the K8s-CR top-level `(apiVersion, kind, metadata,
        // spec)` axis re-export quartet + the load-bearing nested
        // `metadata.{name, namespace, labels}` triplet + the load-
        // bearing nested `LabelSelector.matchLabels` selector-projection
        // axis + the load-bearing nested `spec.rules[]` /
        // `toPorts[].rules` rule-list-container axis under a single
        // canonical `caixa-core::KUBE_KEY_*` re-export shape in this
        // crate onto the load-bearing nested L4-port-scalar axis every
        // rendered `CiliumNetworkPolicy` per-`toPorts[].ports[]` port-
        // tuple + every rendered `Gateway` per-listener + every
        // rendered `HTTPRoute` per-`backendRefs[]` per-rule per-backend
        // carries.
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_PORT",
            KUBE_KEY_PORT,
            caixa_core::KUBE_KEY_PORT,
        );
    }

    #[test]
    fn kube_key_protocol_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_PROTOCOL` was lifted from three
        // inline `"protocol"` literals — two production emitter sites
        // (`cilium_network_policies`'s per-`toPorts[].ports[]` port-
        // tuple `protocol:` scalar the Cilium data plane's per-tuple
        // bpf policy dispatch loop compares against the observed L4
        // header protocol before applying the port match,
        // `gateway_routes`'s per-`Gateway` per-listener
        // `spec.listeners[].protocol` scalar the gateway-class-
        // controller's per-listener bind loop selects the L7 parser
        // + TLS termination strategy from) and one test-side
        // protocol-scalar traversal site (the
        // `gateway_emits_gateway_plus_httproute_pair` `.get("protocol")`
        // retrieval on the emitted `Gateway`'s first listener pinning
        // the canonical `HTTP` listener-protocol content) — to a
        // re-export of [`caixa_core::KUBE_KEY_PROTOCOL`] so the
        // canonical K8s-CR-`protocol`-scalar-discriminator-axis
        // string lives in exactly one place across every caixa
        // renderer. Pin the equality + static-data identity here so
        // any local re-introduction of a sibling `pub const
        // KUBE_KEY_PROTOCOL: &str = "…"` (the canonical drift
        // footgun where a sibling local `pub const` could happen to
        // carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test
        // failure naming the offending drift, not a silent apply-time
        // symptom — the prior shape would have let a typo on any one
        // sibling `pub const` declaration silently miss the per-CR
        // protocol retrieval (the listener-protocol-content pin's
        // `assert_eq!(…, Some("HTTP"))` would silently mask the
        // drift under the `None` unwrap-default), or silently emit a
        // malformed CR whose protocol field the apiserver-side CRD
        // schema validator drops as unrecognized at apply time (the
        // Cilium data plane's per-tuple bpf policy dispatch loop
        // silently fall back to the CRD default protocol `ANY`,
        // admitting UDP traffic through a TCP-only rule; the
        // gateway-class-controller's per-listener bind loop silently
        // fail listener validation on a required protocol field,
        // rejecting the entire `Gateway` object at admission time,
        // no L7 traffic admitted). Bridge-arm peer to
        // [`kube_key_port_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_rules_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_match_labels_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_spec_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_metadata_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_kind_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_api_version_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_namespace_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_labels_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_name_re_export_points_at_caixa_core_canonical`]
        // — extends the K8s-CR top-level `(apiVersion, kind,
        // metadata, spec)` axis re-export quartet + the load-bearing
        // nested `metadata.{name, namespace, labels}` triplet + the
        // load-bearing nested `LabelSelector.matchLabels` selector-
        // projection axis + the load-bearing nested `spec.rules[]` /
        // `toPorts[].rules` rule-list-container axis + the load-
        // bearing nested L4-port-scalar axis under a single canonical
        // `caixa-core::KUBE_KEY_*` re-export shape in this crate onto
        // the load-bearing nested L4/L7-protocol-scalar-discriminator
        // axis every rendered `CiliumNetworkPolicy` per-
        // `toPorts[].ports[]` port-tuple + every rendered `Gateway`
        // per-listener carries.
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_PROTOCOL",
            KUBE_KEY_PROTOCOL,
            caixa_core::KUBE_KEY_PROTOCOL,
        );
    }

    #[test]
    fn kube_protocol_tcp_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_PROTOCOL_TCP` was lifted from the single
        // inline `"TCP".into()` literal at the `cilium_network_policies`
        // per-`(:de, :para)` CNP `port_entry.insert(KUBE_KEY_PROTOCOL,
        // …)` call site (the per-`toPorts[].ports[]` port-tuple L4-
        // transport-protocol scalar-value emit the Cilium data plane's
        // per-tuple bpf policy dispatch loop compares against the
        // observed L4 header protocol before applying the port match)
        // to a re-export of [`caixa_core::KUBE_PROTOCOL_TCP`] so the
        // canonical K8s-core-`Protocol`-enum-value string lives in
        // exactly one place across every caixa renderer. Pin the
        // equality + static-data identity here so any local
        // re-introduction of a sibling `pub const KUBE_PROTOCOL_TCP:
        // &str = "…"` (the canonical drift footgun where a sibling
        // local `pub const` could happen to carry the same string at
        // the source while pointing at a different `&'static`
        // allocation) is a build-time test failure naming the offending
        // drift, not a silent apply-time symptom — the prior shape
        // would have let a K8s core `Protocol` rebrand on the caixa-
        // mesh side without a coordinated caixa-core edit silently
        // land per-`(:de, :para)` CiliumNetworkPolicy documents whose
        // per-`toPorts[].ports[]` port-tuple L4-transport-protocol
        // scalar the K8s core `Protocol` OpenAPI schema enum's
        // `{"TCP", "UDP", "SCTP"}` closed set rejects at apply time
        // (the Cilium operator's per-CNP L4 dispatch pass drops the
        // CNP under a non-self-locating
        // "spec.ingress[0].toPorts[0].ports[0].protocol: Unsupported
        // value" apiserver admission rejection); worse — because the
        // schema-side default is `TCP`, a silently-elided drift on
        // the value lands a CNP whose ingress rule falls back to the
        // default L4-transport-protocol and every port-match on a
        // non-default transport silently misses at the eBPF data
        // plane's per-tuple dispatch. Peer to
        // [`gateway_api_protocol_http_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-Gateway-API-v1-OpenAPI-schema-enum-
        // value re-export surface — extends the Gateway-API-v1-
        // OpenAPI-schema-enum-value single-sourcing discipline onto
        // the sibling K8s-core `Protocol.TCP` per-port-tuple L4-
        // transport-protocol-discriminator the `cilium_network_policies`
        // intra-mesh L4-tuple-gating emitter carries under the shared
        // `CiliumNetworkPolicy` body.
        caixa_core::assert_str_reexport_identity(
            "KUBE_PROTOCOL_TCP",
            KUBE_PROTOCOL_TCP,
            caixa_core::KUBE_PROTOCOL_TCP,
        );
    }

    #[test]
    fn cilium_port_tuple_carries_lifted_kube_protocol_tcp() {
        // Production-emit pin: traverse a rendered CNP's first
        // `spec.ingress[0].toPorts[0].ports[0]` port-tuple and assert
        // the `protocol:` scalar is the lifted `KUBE_PROTOCOL_TCP`
        // (`"TCP"`) verbatim — the load-bearing per-tuple L4-transport-
        // protocol discriminator the Cilium data plane's per-tuple
        // bpf policy dispatch loop compares against the observed L4
        // header protocol before applying the port match. Before the
        // lift the emitter carried an inline `"TCP".into()` literal
        // at the sole `port_entry.insert(KUBE_KEY_PROTOCOL, …)` call
        // site; a typo there (`"tcp"` / `"Tcp"` / `"TCP/IP"`) would
        // have silently landed the CNP outside the K8s core `Protocol`
        // OpenAPI schema enum's `{"TCP", "UDP", "SCTP"}` admitted set,
        // and worse — because the schema-side default is `TCP` — a
        // silently-elided drift would have fallen back to the default
        // L4-transport-protocol at admission, letting non-default-
        // transport port-matches silently miss at the eBPF data plane
        // with no field naming the drift root cause. Peer to
        // `gateway_listener_carries_aplicacao_host`'s
        // `assert_eq!(listener.get(KUBE_KEY_PROTOCOL)…, Some(GATEWAY_API_PROTOCOL_HTTP))`
        // per-listener L7-parser-selection scalar pin on the sibling
        // `Gateway.spec.listeners[].protocol` surface — extends the
        // per-listener L7-parser-selection scalar pin discipline onto
        // the sibling per-`toPorts[].ports[]` port-tuple L4-transport-
        // protocol scalar pin surface every `cilium_network_policies`
        // intra-mesh L4-tuple-gating emit carries under the shared
        // `CiliumNetworkPolicy` body.
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        let port_tuple = policies
            .first()
            .and_then(kube_spec)
            .and_then(|s| s.get(CILIUM_KEY_INGRESS))
            .and_then(|i| i.as_sequence())
            .and_then(|s| s.first())
            .and_then(|i| i.get(CILIUM_KEY_TO_PORTS))
            .and_then(|p| p.as_sequence())
            .and_then(|s| s.first())
            .and_then(|tp| tp.get(CILIUM_KEY_PORTS))
            .and_then(|p| p.as_sequence())
            .and_then(|s| s.first())
            .expect("spec.ingress[0].toPorts[0].ports[0] port-tuple");
        assert_eq!(
            port_tuple.get(KUBE_KEY_PROTOCOL).and_then(|v| v.as_str()),
            Some(KUBE_PROTOCOL_TCP),
            "per-`toPorts[].ports[]` port-tuple `protocol:` scalar must be \
             the lifted `KUBE_PROTOCOL_TCP` (`\"TCP\"`) verbatim — the \
             load-bearing K8s core `Protocol` OpenAPI schema enum value \
             the Cilium data plane's per-tuple bpf policy dispatch loop \
             compares against the observed L4 header protocol"
        );
    }

    #[test]
    fn gateway_api_key_timeouts_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_KEY_TIMEOUTS` was lifted from nine
        // inline `"timeouts"` literals — one production emitter site
        // (`gateway_routes`'s per-`HTTPRoute` per-rule
        // `spec.rules[].timeouts` insert the Aplicacao's typed
        // `:politicas :timeout` overlay lands under, the sub-shape the
        // Gateway API v1 CRD schema pins as `HTTPRouteTimeouts` and
        // whose `request` scalar the Gateway-API-implementation-side
        // per-rule request-dispatch loop compares each accepted
        // request's wall-clock elapsed time against before cancelling
        // the in-flight backend call) and eight test-side per-rule
        // timeout-policy traversal sites (the
        // `httproute_carries_politicas_timeout_on_every_rule` presence
        // pin, the `httproute_omits_timeouts_when_politicas_timeout_unset`
        // absence pin, the `httproute_timeout_renders_every_rule_independently`
        // per-rule fan-out pin under multi-`:entrada :paths`, the
        // `httproute_timeout_uses_canonical_kube_duration_format`
        // `Duration::from_secs(90)`-round-trip canonical-form pin, the
        // `httproute_timeout_renders_minute_window_canonically`
        // 1-minute canonical-form pin, the
        // `httproute_rule_keys_pin_overlay_position` rule-level
        // top-key-set pin, and two `httproute_timeouts_and_retry_coexist_independently`
        // presence-only + absence-only pins pinning independent-axis
        // coexistence with the sibling `retry` per-rule retry-policy
        // axis) — to a re-export of
        // [`caixa_core::GATEWAY_API_KEY_TIMEOUTS`] so the canonical
        // K8s-Gateway-API-`HTTPRoute`-per-rule-request-timeout-policy-
        // body-axis string lives in exactly one place across every
        // caixa renderer. Pin the equality + static-data identity here
        // so any local re-introduction of a sibling `pub const
        // GATEWAY_API_KEY_TIMEOUTS: &str = "…"` (the canonical drift
        // footgun where a sibling local `pub const` could happen to
        // carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test failure
        // naming the offending drift, not a silent apply-time symptom —
        // the prior shape would have let a typo on any one sibling
        // `pub const` declaration silently miss the per-rule request-
        // timeout retrieval (the presence pin's `.expect("rule must
        // carry timeouts mapping when :politicas :timeout is set")`
        // panic-message tag would fire against the presence-shape
        // message rather than the true per-rule-timeout-policy-key
        // drift, the `.get("timeouts").and_then(|t| t.get("request"))`
        // navigators would silently unwrap to `None` under the drifted
        // retrieval), or silently emit a malformed `HTTPRoute` whose
        // per-rule request-timeout-policy field the apiserver-side
        // Gateway API CRD schema validator drops as unrecognized at
        // apply time (the Gateway API implementation's per-rule
        // request-dispatch loop silently no-ops the per-rule wall-
        // clock deadline, the "no infinite blocking" guarantee
        // MESH-COMPOSITION.md §V mandates for every rendered per-
        // `:politicas` mesh-composition edge silently regresses to the
        // pre-overlay unbounded-request semantic). Bridge-arm peer to
        // [`kube_key_protocol_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_port_re_export_points_at_caixa_core_canonical`]
        // + [`kube_key_rules_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_hostnames_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_hostname_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_listeners_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_parent_refs_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_backend_refs_re_export_points_at_caixa_core_canonical`]
        // — extends the per-Gateway-API-CRD-body-axis re-export set
        // onto the load-bearing per-rule request-timeout-policy axis
        // the M3 Aplicacao mesh renderer's per-`:politicas :timeout`
        // overlay lands under.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_KEY_TIMEOUTS",
            GATEWAY_API_KEY_TIMEOUTS,
            caixa_core::GATEWAY_API_KEY_TIMEOUTS,
        );
    }

    #[test]
    fn gateway_api_key_retry_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_KEY_RETRY` was lifted from nine
        // inline `"retry"` literals — one production emitter site
        // (`gateway_routes`'s per-`HTTPRoute` per-rule
        // `spec.rules[].retry` insert the Aplicacao's typed
        // `:politicas :retries` overlay lands under, the sub-shape the
        // Gateway API v1 CRD schema pins as `HTTPRouteRetry` and whose
        // `attempts` scalar the Gateway-API-implementation-side per-
        // rule request-dispatch loop compares each failed attempt count
        // against before giving up on the in-flight backend call) and
        // eight test-side per-rule retry-policy traversal sites (the
        // `httproute_rule_keys_pin_overlay_position` rule-level top-key-
        // set pin, the `httproute_carries_politicas_retries_on_every_rule`
        // presence pin, the `httproute_omits_retry_when_politicas_retries_unset`
        // absence pin, the `httproute_retry_renders_every_rule_independently`
        // per-rule fan-out pin under multi-`:entrada :paths`, the
        // `httproute_retry_round_trips_typed_attempt_count` typed-`u32`-
        // round-trip pin, the `httproute_retry_attempts_serialized_as_yaml_number`
        // YAML integer-scalar-kind pin, and two
        // `httproute_timeouts_and_retry_coexist_independently` presence-
        // only + absence-only pins pinning independent-axis coexistence
        // with the sibling `timeouts` per-rule request-timeout-policy
        // axis) — to a re-export of
        // [`caixa_core::GATEWAY_API_KEY_RETRY`] so the canonical
        // K8s-Gateway-API-`HTTPRoute`-per-rule-retry-policy-body-axis
        // string lives in exactly one place across every caixa
        // renderer. Pin the equality + static-data identity here so any
        // local re-introduction of a sibling `pub const
        // GATEWAY_API_KEY_RETRY: &str = "…"` (the canonical drift
        // footgun where a sibling local `pub const` could happen to
        // carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test failure
        // naming the offending drift, not a silent apply-time symptom —
        // the prior shape would have let a typo on any one sibling
        // `pub const` declaration silently miss the per-rule retry
        // retrieval (the presence pin's `.expect("rule must carry retry
        // mapping when :politicas :retries is set")` panic-message tag
        // would fire against the presence-shape message rather than the
        // true per-rule-retry-policy-key drift, the
        // `.get("retry").and_then(|r| r.get("attempts"))` navigators
        // would silently unwrap to `None` under the drifted retrieval),
        // or silently emit a malformed `HTTPRoute` whose per-rule
        // retry-policy field the apiserver-side Gateway API CRD schema
        // validator drops as unrecognized at apply time (the Gateway
        // API implementation's per-rule request-dispatch loop silently
        // no-ops the per-rule retry budget, the "no infinite retrying
        // without bound" guarantee MESH-COMPOSITION.md §V mandates for
        // every rendered per-`:politicas` mesh-composition edge
        // silently regresses to the pre-overlay unbounded-retry
        // semantic). Bridge-arm peer to
        // [`gateway_api_key_timeouts_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_hostnames_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_hostname_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_listeners_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_parent_refs_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_backend_refs_re_export_points_at_caixa_core_canonical`]
        // — closes the per-Gateway-API-`HTTPRoute`-per-rule `:politicas`
        // overlay axis re-export pair (`timeouts` for `:politicas
        // :timeout`, `retry` for `:politicas :retries`) both
        // MESH-COMPOSITION.md §V "no infinite blocking / no infinite
        // retrying" guarantees rest on.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_KEY_RETRY",
            GATEWAY_API_KEY_RETRY,
            caixa_core::GATEWAY_API_KEY_RETRY,
        );
    }

    #[test]
    fn gateway_api_key_attempts_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_KEY_ATTEMPTS` was lifted from six
        // inline `"attempts"` literals — one production emitter site
        // (`gateway_routes`'s per-`HTTPRoute` per-rule
        // `single_field_overlay(spec.politicas.retries, …)` call that
        // seeds the typed `u32` attempt count into the sibling
        // [`GATEWAY_API_KEY_RETRY`] container axis under
        // `spec.rules[].retry.attempts`, the leaf the Gateway API v1
        // CRD schema pins as `HTTPRouteRetry.attempts` and whose scalar
        // value the Gateway-API-implementation-side per-rule request-
        // dispatch loop compares each failed backend attempt count
        // against before giving up on the in-flight backend call) and
        // five test-side per-rule retry-attempts traversal sites (the
        // `httproute_carries_politicas_retries_on_every_rule` typed-
        // `u64`-value pin, the
        // `httproute_retry_renders_every_rule_independently` per-rule
        // fan-out attempt-count pin under multi-`:entrada :paths`, the
        // `httproute_retry_round_trips_typed_attempt_count` typed-`u32`-
        // round-trip pin, the
        // `httproute_retry_attempts_serialized_as_yaml_number` YAML
        // integer-scalar-kind pin, and the retries-only arm of
        // `httproute_timeouts_and_retry_coexist_independently` pinning
        // the leaf attempt count survives when only the sibling
        // `:retries` slot is set) — to a re-export of
        // [`caixa_core::GATEWAY_API_KEY_ATTEMPTS`] so the canonical K8s-
        // Gateway-API-`HTTPRoute`-per-rule-retry-policy-`attempts`-
        // leaf-scalar-key string lives in exactly one place across
        // every caixa renderer. Pin the equality + static-data identity
        // here so any local re-introduction of a sibling `pub const
        // GATEWAY_API_KEY_ATTEMPTS: &str = "…"` (the canonical drift
        // footgun where a sibling local `pub const` could happen to
        // carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test failure
        // naming the offending drift, not a silent apply-time symptom
        // — the prior shape would have let a typo on any one sibling
        // `pub const` declaration silently miss the per-rule retry-
        // attempts retrieval (the round-trip pins' `Some(3)` /
        // `Some(5)` / `Some(2)` equality tags would fire against the
        // `None` retrieval, silently masking the true per-rule-retry-
        // attempts-leaf-key drift), or silently emit a malformed
        // `HTTPRoute` whose per-rule retry sub-shape the apiserver-side
        // Gateway API CRD schema validator drops the leaf attempt count
        // from as unrecognized at apply time (the Gateway API
        // implementation's per-rule request-dispatch loop parses the
        // retry sub-shape as an empty `HTTPRouteRetry`, the "no
        // infinite retrying without bound" guarantee MESH-COMPOSITION.md
        // §V mandates for every rendered per-`:politicas` mesh-
        // composition edge silently regresses to the pre-overlay
        // unbounded-retry semantic). Bridge-arm peer to
        // [`gateway_api_key_retry_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_timeouts_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_hostnames_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_hostname_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_listeners_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_parent_refs_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_backend_refs_re_export_points_at_caixa_core_canonical`]
        // — closes the parent-leaf axis pair (`retry` container +
        // `attempts` leaf) both MESH-COMPOSITION.md §V "no infinite
        // retrying" guarantees rest on, one nesting level deeper than
        // the parent per-rule retry-policy container axis (`retry`).
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_KEY_ATTEMPTS",
            GATEWAY_API_KEY_ATTEMPTS,
            caixa_core::GATEWAY_API_KEY_ATTEMPTS,
        );
    }

    #[test]
    fn gateway_api_key_request_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_KEY_REQUEST` was lifted from six
        // inline `"request"` literals — one production emitter site
        // (`gateway_routes`'s per-`HTTPRoute` per-rule
        // `single_field_overlay(spec.politicas.timeout, …)` call that
        // seeds the typed `Duration` request-deadline string into the
        // sibling [`GATEWAY_API_KEY_TIMEOUTS`] container axis under
        // `spec.rules[].timeouts.request`, the leaf the Gateway API v1
        // CRD schema pins as `HTTPRouteTimeouts.request` and whose
        // scalar value the Gateway-API-implementation-side per-rule
        // request-dispatch loop commits to as the per-request wall-
        // clock deadline every inbound request is bounded against
        // before the resolved backend even sees the call) and five
        // test-side per-rule request-deadline traversal sites (the
        // `httproute_carries_politicas_timeout_on_every_rule` typed-
        // `&str`-value pin, the
        // `httproute_timeout_renders_every_rule_independently` per-rule
        // fan-out request-deadline pin under multi-`:entrada :paths`,
        // the `httproute_timeout_uses_canonical_kube_duration_format`
        // typed-`Duration`-round-trip pin, the
        // `httproute_timeout_renders_minute_window_canonically`
        // canonical-minute-form pin, and the timeout-only arm of
        // `httproute_timeouts_and_retry_coexist_independently` pinning
        // the leaf request-deadline survives when only the sibling
        // `:timeout` slot is set) — to a re-export of
        // [`caixa_core::GATEWAY_API_KEY_REQUEST`] so the canonical K8s-
        // Gateway-API-`HTTPRoute`-per-rule-request-timeout-policy-
        // `request`-leaf-scalar-key string lives in exactly one place
        // across every caixa renderer. Pin the equality + static-data
        // identity here so any local re-introduction of a sibling `pub
        // const GATEWAY_API_KEY_REQUEST: &str = "…"` (the canonical
        // drift footgun where a sibling local `pub const` could happen
        // to carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test failure
        // naming the offending drift, not a silent apply-time symptom
        // — the prior shape would have let a typo on any one sibling
        // `pub const` declaration silently miss the per-rule request-
        // deadline retrieval (the round-trip pins' `Some("30s")` /
        // `Some("90s")` / `Some("1m")` / `Some("15s")` equality tags
        // would fire against the `None` retrieval, silently masking
        // the true per-rule-request-deadline-leaf-key drift), or
        // silently emit a malformed `HTTPRoute` whose per-rule
        // timeouts sub-shape the apiserver-side Gateway API CRD schema
        // validator drops the leaf request-deadline from as
        // unrecognized at apply time (the Gateway API implementation's
        // per-rule request-dispatch loop parses the timeouts sub-shape
        // as an empty `HTTPRouteTimeouts`, the "no infinite blocking"
        // guarantee MESH-COMPOSITION.md §V mandates for every rendered
        // per-`:politicas` mesh-composition edge silently regresses to
        // the pre-overlay unbounded-blocking semantic). Bridge-arm peer
        // to
        // [`gateway_api_key_attempts_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_retry_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_timeouts_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_hostnames_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_hostname_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_listeners_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_parent_refs_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_key_backend_refs_re_export_points_at_caixa_core_canonical`]
        // — closes the second parent-leaf axis pair (`timeouts`
        // container + `request` leaf) both MESH-COMPOSITION.md §V "no
        // infinite blocking / no infinite retrying" guarantees rest
        // on, sibling to the parent-leaf pair (`retry` container +
        // `attempts` leaf) closed in the immediately-preceding
        // [`GATEWAY_API_KEY_ATTEMPTS`] bridge-arm.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_KEY_REQUEST",
            GATEWAY_API_KEY_REQUEST,
            caixa_core::GATEWAY_API_KEY_REQUEST,
        );
    }

    #[test]
    fn cilium_kind_network_policy_re_export_points_at_caixa_core_canonical() {
        // The renderer's `CILIUM_KIND_NETWORK_POLICY` was lifted from
        // the inline `"CiliumNetworkPolicy"` literal at the
        // `cilium_network_policies` `kube_resource_skeleton` kind
        // argument (caixa-mesh/src/lib.rs:382 — the per-`(:de, :para)`
        // CiliumNetworkPolicy emit site) to a re-export of
        // [`caixa_core::CILIUM_KIND_NETWORK_POLICY`] so the
        // Cilium-CRD-`kind` discriminator string lives in exactly one
        // place across every caixa renderer. Pin the equality +
        // static-data identity here so any local re-introduction of a
        // sibling `pub const CILIUM_KIND_NETWORK_POLICY: &str = "…"`
        // (the canonical drift footgun where a sibling local `pub
        // const` could happen to carry the same string at the source
        // while pointing at a different `&'static` allocation) is a
        // build-time test failure naming the offending drift, not a
        // silent apply-time symptom — the prior shape would have let a
        // Cilium-CRD kind rebrand on the caixa-mesh side without a
        // coordinated caixa-core edit silently land per-`(:de, :para)`
        // CiliumNetworkPolicy objects at one CRD kind and every
        // future per-target Cilium-side renderer's emitted
        // `CiliumClusterwideNetworkPolicy` / `CiliumLocalRedirectPolicy`
        // at the drifted other, with every intra-mesh L4/L7 contrato
        // flow dropping at apply time because the per-policy
        // attached-identity pipeline never binds across the
        // kind-drifted CRD-discriminator pair. Peer to
        // [`cilium_api_version_re_export_points_at_caixa_core_canonical`]
        // on the sibling Cilium-CRD-apiVersion-re-export axis —
        // completes the per-Cilium-CRD kind+apiVersion re-export pair
        // this crate's `cilium_network_policies` renderer's eBPF
        // data-plane contract rests on.
        caixa_core::assert_str_reexport_identity(
            "CILIUM_KIND_NETWORK_POLICY",
            CILIUM_KIND_NETWORK_POLICY,
            caixa_core::CILIUM_KIND_NETWORK_POLICY,
        );
    }

    #[test]
    fn cilium_key_to_ports_re_export_points_at_caixa_core_canonical() {
        // The renderer's `CILIUM_KEY_TO_PORTS` was lifted from the
        // inline `"toPorts"` literal at the `cilium_network_policies`
        // per-`(:de, :para)` `ingress_rule.insert("toPorts", …)` call
        // site + six test-side navigations
        // (`cilium_http_contracts_emit_l7_rules`,
        // `cilium_pubsub_contracts_skip_l7_rules`,
        // `cilium_multiple_edges_same_pair_fold_into_one_policy`,
        // `cnp_authentication_carries_mtls_overlay_at_ingress_rule_level`
        // — via the `contains_key` presence check + the paired
        // `.get(…).and_then(|t| t.as_sequence())` navigation both
        // consulting the same axis,
        // `cnp_l4_fallback_port_routes_through_lifted_default_servico_port`)
        // to a re-export of [`caixa_core::CILIUM_KEY_TO_PORTS`] so the
        // Cilium-CRD per-ingress-rule port-set container-key string
        // lives in exactly one place across every caixa renderer. Pin
        // the equality + static-data identity here so any local
        // re-introduction of a sibling `pub const CILIUM_KEY_TO_PORTS:
        // &str = "…"` (the canonical drift footgun where a sibling
        // local `pub const` could happen to carry the same string at
        // the source while pointing at a different `&'static`
        // allocation) is a build-time test failure naming the offending
        // drift, not a silent apply-time symptom — the prior shape
        // would have let a Cilium-CRD schema rebrand on the port-set
        // container axis without a coordinated caixa-core edit silently
        // land per-`(:de, :para)` CiliumNetworkPolicy documents whose
        // `spec.ingress[].toPorts[]` container the Cilium CRD schema
        // validator drops as unrecognized at apply time, with every
        // intra-mesh L4/L7 `:contratos` flow dropping at the eBPF data
        // plane's default-deny gate because the per-CNP L4-allow /
        // L7-dispatch pass never binds through the container-drifted
        // port-set-container axis. Peer to
        // [`kube_key_rules_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-CNP-dispatch-container re-export axis —
        // completes the per-CNP L4/L7-dispatch-container
        // `(toPorts, rules)` re-export pair this crate's
        // `cilium_network_policies` renderer's eBPF data-plane contract
        // rests on. Peer to
        // [`cilium_kind_network_policy_re_export_points_at_caixa_core_canonical`]
        // + [`cilium_api_version_re_export_points_at_caixa_core_canonical`]
        // on the outer `(apiVersion, kind)` shell of the same per-CNP
        // CRD — extends the per-Cilium-CRD re-export set from the outer
        // shell down through the load-bearing
        // `spec.ingress[].toPorts[].rules` L4/L7-dispatch axis.
        caixa_core::assert_str_reexport_identity(
            "CILIUM_KEY_TO_PORTS",
            CILIUM_KEY_TO_PORTS,
            caixa_core::CILIUM_KEY_TO_PORTS,
        );
    }

    #[test]
    fn cilium_key_endpoint_selector_re_export_points_at_caixa_core_canonical() {
        // The renderer's `CILIUM_KEY_ENDPOINT_SELECTOR` was lifted from
        // the inline `"endpointSelector"` literal at the
        // `cilium_network_policies` per-`(:de, :para)` CNP
        // `policy_spec.insert("endpointSelector", …)` call site plus
        // two test-side navigations
        // (`cilium_policies_are_identity_based`,
        // `cnp_endpoint_selector_carries_program_only_single_axis_shape`
        // — the pair of destination-`endpointSelector` retrievals whose
        // downstream `.and_then(|e| e.get(KUBE_KEY_MATCH_LABELS))`
        // chains consult the same axis-key) to a re-export of
        // [`caixa_core::CILIUM_KEY_ENDPOINT_SELECTOR`] so the Cilium-CRD
        // per-CNP-body destination-identity axis-key string lives in
        // exactly one place across every caixa renderer. Pin the
        // equality + static-data identity here so any local
        // re-introduction of a sibling `pub const CILIUM_KEY_ENDPOINT_\
        // SELECTOR: &str = "…"` (the canonical drift footgun where a
        // sibling local `pub const` could happen to carry the same
        // string at the source while pointing at a different `&'static`
        // allocation) is a build-time test failure naming the offending
        // drift, not a silent apply-time symptom — the prior shape
        // would have let a Cilium-CRD schema rebrand on the
        // destination-identity axis without a coordinated caixa-core
        // edit silently land per-`(:de, :para)` CiliumNetworkPolicy
        // documents whose `spec.endpointSelector` axis the Cilium CRD
        // schema validator drops as unrecognized at apply time, with
        // the emitted policy binding against no destination pods and
        // every intra-mesh `:contratos` flow the affected CNP was
        // authored to allow dropping at the eBPF data plane's default-
        // deny gate because the per-CNP L3-target-selector pass never
        // resolves through the axis-drifted destination-identity key.
        // Peer to
        // [`cilium_key_to_ports_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-CNP-body-axis re-export set — completes the
        // per-CNP L3/L4/L7-triad
        // `(endpointSelector, ingress → toPorts → rules)` re-export
        // this crate's `cilium_network_policies` renderer's eBPF data-
        // plane contract rests on. Peer to
        // [`cilium_kind_network_policy_re_export_points_at_caixa_core_canonical`]
        // + [`cilium_api_version_re_export_points_at_caixa_core_canonical`]
        // on the outer `(apiVersion, kind)` shell of the same per-CNP
        // CRD — extends the per-Cilium-CRD re-export set from the outer
        // shell down through the load-bearing `spec.endpointSelector`
        // L3-target-selector axis.
        caixa_core::assert_str_reexport_identity(
            "CILIUM_KEY_ENDPOINT_SELECTOR",
            CILIUM_KEY_ENDPOINT_SELECTOR,
            caixa_core::CILIUM_KEY_ENDPOINT_SELECTOR,
        );
    }

    #[test]
    fn cilium_key_ingress_re_export_points_at_caixa_core_canonical() {
        // The renderer's `CILIUM_KEY_INGRESS` was lifted from the inline
        // `"ingress"` literal at the `cilium_network_policies`
        // per-`(:de, :para)` CNP `policy_spec.insert("ingress", …)`
        // call site plus eight test-side navigations
        // (`cilium_http_contracts_emit_l7_rules`,
        // `cilium_policies_are_identity_based`,
        // `cnp_from_endpoints_carries_program_plus_aplicacao_labels_two_axis_shape`,
        // `cilium_multiple_edges_same_pair_fold_into_one_policy`,
        // `cilium_pubsub_contracts_skip_l7_rules`,
        // `render_multi_doc_contains_expected_kinds`,
        // `cnp_authentication_carries_mtls_overlay_at_ingress_rule_level`,
        // `cnp_l4_fallback_port_routes_through_lifted_default_servico_port`
        // — the per-CNP navigation each of these tests consults to
        // reach the ingress-rule list before descending into the
        // `fromEndpoints` / `toPorts` / `authentication` axes) to a
        // re-export of [`caixa_core::CILIUM_KEY_INGRESS`] so the
        // Cilium-CRD per-CNP-body traffic-direction axis-key string
        // lives in exactly one place across every caixa renderer. Pin
        // the equality + static-data identity here so any local
        // re-introduction of a sibling `pub const CILIUM_KEY_INGRESS:
        // &str = "…"` (the canonical drift footgun where a sibling
        // local `pub const` could happen to carry the same string at
        // the source while pointing at a different `&'static`
        // allocation) is a build-time test failure naming the offending
        // drift, not a silent apply-time symptom — the prior shape
        // would have let a Cilium-CRD schema rebrand on the traffic-
        // direction axis without a coordinated caixa-core edit silently
        // land per-`(:de, :para)` CiliumNetworkPolicy documents whose
        // `spec.ingress[]` list the Cilium CRD schema validator drops
        // as unrecognized at apply time, with the emitted policy
        // binding against the destination workload but admitting no
        // ingress traffic and every intra-mesh `:contratos` flow the
        // affected CNP was authored to allow dropping at the eBPF data
        // plane's default-deny gate because the per-CNP L4/L7-dispatch
        // pass never resolves through the axis-drifted traffic-
        // direction key. Peer to
        // [`cilium_key_endpoint_selector_re_export_points_at_caixa_core_canonical`]
        // + [`cilium_key_to_ports_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-CNP-body-axis re-export set — completes
        // the per-CNP L3/L4/L7-triad
        // `(endpointSelector, ingress → toPorts → rules)` re-export
        // this crate's `cilium_network_policies` renderer's eBPF data-
        // plane contract rests on by lifting the traffic-direction
        // axis that structurally separates the destination-identity
        // axis from the port-set-container axis nested beneath it.
        // Peer to
        // [`cilium_kind_network_policy_re_export_points_at_caixa_core_canonical`]
        // + [`cilium_api_version_re_export_points_at_caixa_core_canonical`]
        // on the outer `(apiVersion, kind)` shell of the same per-CNP
        // CRD — extends the per-Cilium-CRD re-export set from the outer
        // shell down through the load-bearing `spec.ingress[]` traffic-
        // direction axis.
        caixa_core::assert_str_reexport_identity(
            "CILIUM_KEY_INGRESS",
            CILIUM_KEY_INGRESS,
            caixa_core::CILIUM_KEY_INGRESS,
        );
    }

    #[test]
    fn cilium_key_from_endpoints_re_export_points_at_caixa_core_canonical() {
        // The renderer's `CILIUM_KEY_FROM_ENDPOINTS` was lifted from the
        // inline `"fromEndpoints"` literal at the `cilium_network_policies`
        // per-`(:de, :para)` CNP `ingress_rule.insert("fromEndpoints", …)`
        // call site plus four test-side navigations
        // (`cnp_from_endpoints_carries_program_plus_aplicacao_labels_two_axis_shape`
        // — the `.and_then(|i| i.get("fromEndpoints"))` navigation whose
        // downstream `.and_then(|e| e.get(KUBE_KEY_MATCH_LABELS))` chain
        // reaches the source `LabelSelector`,
        // `cilium_policies_are_identity_based` — the paired
        // `.and_then(|i| i.get("fromEndpoints"))` navigation whose
        // `.expect("fromEndpoints[0].matchLabels mapping")` pins the
        // two-axis-selector shape,
        // `cnp_authentication_carries_mtls_overlay_at_ingress_rule_level`
        // — via both the `contains_key` presence check + the paired
        // `.get(…).and_then(|f| f.as_sequence())` navigation consulting
        // the same axis) to a re-export of
        // [`caixa_core::CILIUM_KEY_FROM_ENDPOINTS`] so the Cilium-CRD
        // per-ingress-rule identity-source axis-key string lives in
        // exactly one place across every caixa renderer. Pin the
        // equality + static-data identity here so any local
        // re-introduction of a sibling `pub const CILIUM_KEY_FROM_\
        // ENDPOINTS: &str = "…"` (the canonical drift footgun where a
        // sibling local `pub const` could happen to carry the same
        // string at the source while pointing at a different `&'static`
        // allocation) is a build-time test failure naming the offending
        // drift, not a silent apply-time symptom — the prior shape
        // would have let a Cilium-CRD schema rebrand on the identity-
        // source axis without a coordinated caixa-core edit silently
        // land per-`(:de, :para)` CiliumNetworkPolicy documents whose
        // `spec.ingress[].fromEndpoints[]` list the Cilium CRD schema
        // validator drops as unrecognized at apply time, with the
        // emitted ingress rule admitting no source pods and every
        // intra-mesh `:contratos` flow the affected CNP was authored to
        // allow dropping at the eBPF data plane's default-deny gate
        // because the per-CNP identity-resolution pass never binds
        // through the axis-drifted identity-source key. Peer to
        // [`cilium_key_endpoint_selector_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-CNP-body-axis re-export set — completes
        // the per-CNP identity-pair
        // `(endpointSelector, fromEndpoints)` re-export this crate's
        // `cilium_network_policies` renderer's eBPF data-plane contract
        // rests on by lifting the identity-source axis structurally
        // paired with the destination-identity axis under the Cilium-
        // operator-side per-CNP SPIFFE-identity-bound access-control
        // contract. Peer to
        // [`cilium_kind_network_policy_re_export_points_at_caixa_core_canonical`]
        // + [`cilium_api_version_re_export_points_at_caixa_core_canonical`]
        // on the outer `(apiVersion, kind)` shell of the same per-CNP
        // CRD — extends the per-Cilium-CRD re-export set from the outer
        // shell down through the load-bearing
        // `spec.ingress[].fromEndpoints[]` identity-source axis.
        caixa_core::assert_str_reexport_identity(
            "CILIUM_KEY_FROM_ENDPOINTS",
            CILIUM_KEY_FROM_ENDPOINTS,
            caixa_core::CILIUM_KEY_FROM_ENDPOINTS,
        );
    }

    #[test]
    fn cilium_key_ports_re_export_points_at_caixa_core_canonical() {
        // The renderer's `CILIUM_KEY_PORTS` was lifted from the inline
        // `"ports"` literal at the `cilium_network_policies` per-
        // `(:de, :para)` CNP `to_port.insert("ports", …)` call site
        // (caixa-mesh/src/lib.rs:1081 — the per-`toPorts[]`-entry L4
        // port-tuple-list emitter) plus two test-side navigations
        // (`cilium_pubsub_contracts_skip_l7_rules` — the
        // `to_ports.get("ports").is_some()` presence pin the L4-yes-L7-
        // no separation invariant hinges on;
        // `cnp_l4_fallback_port_reflects_default_servico_port` — the
        // `.and_then(|tp| tp.get("ports"))` navigation whose downstream
        // `.and_then(|s| s.first()).and_then(|p| p.get("port"))` chain
        // reads the per-port-set L4 port-tuple value the
        // `DEFAULT_SERVICO_PORT` fallback pins) to a re-export of
        // [`caixa_core::CILIUM_KEY_PORTS`] so the Cilium-CRD per-
        // `toPorts[]`-entry L4-port-tuple-list-container-axis-key
        // string lives in exactly one place across every caixa
        // renderer. Pin the equality + static-data identity here so any
        // local re-introduction of a sibling `pub const CILIUM_KEY_\
        // PORTS: &str = "…"` (the canonical drift footgun where a
        // sibling local `pub const` could happen to carry the same
        // string at the source while pointing at a different `&'static`
        // allocation) is a build-time test failure naming the offending
        // drift, not a silent apply-time symptom — the prior shape
        // would have let a Cilium-CRD schema rebrand on the L4 port-
        // tuple-list-container axis without a coordinated caixa-core
        // edit silently land per-`(:de, :para)` CiliumNetworkPolicy
        // documents whose `spec.ingress[].toPorts[].ports[]` list the
        // Cilium CRD schema validator drops as unrecognized at apply
        // time, with the emitted per-port-set entry admitting no
        // `(port, protocol)` tuple and every intra-mesh `:contratos`
        // flow the affected CNP was authored to allow dropping at the
        // eBPF data plane's default-deny gate because the per-CNP L4-
        // allow eBPF-program-generation pass never sources through the
        // axis-drifted L4-port-tuple-list-container key. Peer to
        // [`cilium_key_to_ports_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-CNP-dispatch-axis re-export set — nests
        // the per-port-set L4 port-tuple-list-container axis
        // structurally beneath the sibling per-ingress-rule port-set-
        // container axis it lives inside, extending the per-CNP
        // L3/L4/L7-triad
        // `(endpointSelector, ingress → toPorts → ports / rules)`
        // re-export with the L4-half's port-tuple-list-container axis
        // this crate's `cilium_network_policies` renderer's eBPF data-
        // plane L4-allow contract rests on.
        caixa_core::assert_str_reexport_identity(
            "CILIUM_KEY_PORTS",
            CILIUM_KEY_PORTS,
            caixa_core::CILIUM_KEY_PORTS,
        );
    }

    #[test]
    fn cilium_key_authentication_re_export_points_at_caixa_core_canonical() {
        // The renderer's `CILIUM_KEY_AUTHENTICATION` was lifted from
        // the inline `"authentication"` literal at the
        // `cilium_network_policies` per-`(:de, :para)` CNP
        // `ingress_rule.insert("authentication", …)` call site (the
        // per-ingress-rule mutual-auth emit gate the `:politicas
        // :mtls-required` overlay lands under) plus nine test-side
        // navigations: the presence pin under `:mtls-required t`, the
        // absence pin under `:mtls-required` unset (default),
        // the explicit-`Some(false)`-emits-`"disabled"`-mode pin, the
        // per-policy-fan-out pin across multiple `:contratos`, the
        // rule-level-position pin (with two nested-inside-negative
        // guards under `fromEndpoints[]` and `toPorts[]`), the pubsub-
        // contracts-carry-overlay-too shape pin, and the yaml-string-
        // scalar `mode`-value pin — to a re-export of
        // [`caixa_core::CILIUM_KEY_AUTHENTICATION`] so the Cilium-CRD
        // per-ingress-rule mutual-auth-body-axis-key string lives in
        // exactly one place across every caixa renderer. Pin the
        // equality + static-data identity here so any local
        // re-introduction of a sibling `pub const CILIUM_KEY_\
        // AUTHENTICATION: &str = "…"` (the canonical drift footgun
        // where a sibling local `pub const` could happen to carry the
        // same string at the source while pointing at a different
        // `&'static` allocation) is a build-time test failure naming
        // the offending drift, not a silent apply-time symptom — the
        // prior shape would have let a Cilium-CRD schema rebrand on
        // the per-ingress-rule mutual-auth axis without a coordinated
        // caixa-core edit silently land per-`(:de, :para)`
        // CiliumNetworkPolicy documents whose
        // `spec.ingress[].authentication` block the Cilium CRD schema
        // validator drops as unrecognized at apply time; the emitted
        // per-ingress-rule mutual-auth block falls back to the
        // cluster-default authentication mode and every intra-mesh
        // `:contratos` flow the CNP was authored to protect with
        // per-edge SPIFFE-identity-bound mutual-auth silently
        // bypasses the mTLS handshake at the Cilium data-plane's
        // default-authentication mode. Peer to
        // [`cilium_key_from_endpoints_re_export_points_at_caixa_core_canonical`]
        // + [`cilium_key_to_ports_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-ingress-rule-body-axis re-export set —
        // completes the per-ingress-rule-body triple
        // `(fromEndpoints, toPorts, authentication)` this crate's
        // `cilium_network_policies` renderer's SPIFFE-identity-bound
        // per-edge mTLS contract rests on.
        caixa_core::assert_str_reexport_identity(
            "CILIUM_KEY_AUTHENTICATION",
            CILIUM_KEY_AUTHENTICATION,
            caixa_core::CILIUM_KEY_AUTHENTICATION,
        );
    }

    #[test]
    fn cilium_key_mode_re_export_points_at_caixa_core_canonical() {
        // The renderer's `CILIUM_KEY_MODE` was lifted from the inline
        // `"mode"` literal at the `cilium_network_policies` per-`(:de,
        // :para)` CNP `single_field_overlay(spec.politicas.mtls_required,
        // "mode", …)` call site (the per-rule mutual-auth-mode-leaf
        // emit gate the `:politicas :mtls-required` overlay lands
        // under, which the helper writes at the single leaf axis of
        // the per-rule authentication block) plus five test-side
        // navigations: the presence pin under `:mtls-required t`, the
        // explicit-`Some(false)`-emits-`"disabled"`-mode pin, the
        // per-policy-fan-out pin across multiple `:contratos`, the
        // pubsub-contracts-carry-overlay-too shape pin, and the
        // yaml-string-scalar `mode`-value pin — to a re-export of
        // [`caixa_core::CILIUM_KEY_MODE`] so the Cilium-CRD per-
        // authentication-block mode-discriminator leaf-axis key string
        // lives in exactly one place across every caixa renderer. Pin
        // the equality + static-data identity here so any local
        // re-introduction of a sibling `pub const CILIUM_KEY_MODE:
        // &str = "…"` (the canonical drift footgun where a sibling
        // local `pub const` could happen to carry the same string at
        // the source while pointing at a different `&'static`
        // allocation) is a build-time test failure naming the
        // offending drift, not a silent apply-time symptom — the prior
        // shape would have let a Cilium-CRD schema rebrand on the per-
        // authentication-block mode-discriminator leaf axis without a
        // coordinated caixa-core edit silently land per-`(:de, :para)`
        // CiliumNetworkPolicy documents whose per-`ingress[]` entry
        // mutual-auth block's mode-leaf-key the Cilium CRD schema
        // validator drops as unrecognized at apply time; the ingress
        // rule falls back to the cluster-default authentication mode
        // and every intra-mesh `:contratos` flow the CNP was authored
        // to protect with per-edge SPIFFE-identity-bound mutual-auth
        // silently bypasses the mTLS handshake at the Cilium data-
        // plane's default-authentication mode. Peer to
        // [`cilium_key_authentication_re_export_points_at_caixa_core_canonical`]
        // on the parent per-ingress-rule mutual-auth-body-axis re-
        // export surface — completes the per-rule mutual-auth
        // `(authentication → mode)` body/leaf axis re-export pair this
        // crate's `cilium_network_policies` renderer's SPIFFE-identity-
        // bound per-edge mTLS enforcement contract rests on.
        caixa_core::assert_str_reexport_identity(
            "CILIUM_KEY_MODE",
            CILIUM_KEY_MODE,
            caixa_core::CILIUM_KEY_MODE,
        );
    }

    #[test]
    fn cilium_auth_mode_required_re_export_points_at_caixa_core_canonical() {
        // The renderer's `CILIUM_AUTH_MODE_REQUIRED` was lifted from the
        // inline `"required"` scalar-value at the `cilium_network_policies`
        // per-`(:de, :para)` CNP `single_field_overlay(spec.politicas.
        // mtls_required, CILIUM_KEY_MODE, |required| …)` closure's
        // `if required { … }` affirmative arm (the per-rule mutual-auth-
        // mode-discriminator leaf value the Cilium agent's per-rule
        // dispatch loop keys off to select the SPIFFE-identity-handshake-
        // mandatory enforcement policy) plus three test-side navigations —
        // the presence pin under `:mtls-required t`, the per-policy fan-
        // out pin across multiple `:contratos`, and the pubsub-carry-
        // overlay-too shape pin — to a re-export of
        // [`caixa_core::CILIUM_AUTH_MODE_REQUIRED`] so the Cilium-CRD per-
        // rule mutual-auth-mandatory scalar-value string lives in exactly
        // one place across every caixa renderer. Pin the equality +
        // static-data identity here so any local re-introduction of a
        // sibling `pub const CILIUM_AUTH_MODE_REQUIRED: &str = "…"` (the
        // canonical drift footgun where a sibling local `pub const` could
        // happen to carry the same string at the source while pointing at
        // a different `&'static` allocation) is a build-time test failure
        // naming the offending drift, not a silent apply-time symptom —
        // the prior shape would have let a Cilium CNP
        // `MutualAuthenticationMode` OpenAPI schema enum rebrand on the
        // mTLS-mandatory scalar value without a coordinated caixa-core
        // edit silently land per-`(:de, :para)` CiliumNetworkPolicy
        // documents whose per-`ingress[]` entry mutual-auth block's mode
        // value the Cilium-agent-side schema validator drops as
        // unrecognized at apply time; the author's mTLS-mandatory intent
        // silently collapses onto the cluster-default authentication mode
        // and every intra-mesh `:contratos` flow the CNP was authored to
        // protect with per-edge SPIFFE-identity-bound mutual-auth silently
        // bypasses the mTLS handshake at the Cilium data-plane's default-
        // authentication mode. Peer to
        // [`cilium_auth_mode_disabled_re_export_points_at_caixa_core_canonical`]
        // on the explicit-opt-out arm of the same
        // `MutualAuthenticationMode` enum + to
        // [`cilium_key_mode_re_export_points_at_caixa_core_canonical`] on
        // the parent per-authn-block mode-discriminator leaf-axis-key re-
        // export surface — completes the per-authn-block `(mode →
        // {required, disabled})` author-reachable-scalar-value-pair re-
        // export pair this crate's `cilium_network_policies` renderer's
        // SPIFFE-identity-bound per-edge mTLS enforcement contract rests
        // on across the affirmative arm of the `:politicas :mtls-required`
        // tristate.
        caixa_core::assert_str_reexport_identity(
            "CILIUM_AUTH_MODE_REQUIRED",
            CILIUM_AUTH_MODE_REQUIRED,
            caixa_core::CILIUM_AUTH_MODE_REQUIRED,
        );
    }

    #[test]
    fn cilium_auth_mode_disabled_re_export_points_at_caixa_core_canonical() {
        // Peer to `cilium_auth_mode_required_re_export_points_at_caixa_
        // core_canonical` on the `Some(false)` explicit-opt-out arm of
        // the same `:politicas :mtls-required` tristate: the renderer's
        // `CILIUM_AUTH_MODE_DISABLED` was lifted from the inline
        // `"disabled"` scalar-value at the `cilium_network_policies` per-
        // `(:de, :para)` CNP `single_field_overlay(...)` closure's `else
        // { … }` opt-out arm plus one test-side navigation (the
        // `cnp_explicit_mtls_required_false_emits_disabled_mode` explicit-
        // opt-out probe) to a re-export of
        // [`caixa_core::CILIUM_AUTH_MODE_DISABLED`] so the Cilium-CRD per-
        // rule mutual-auth-skipped scalar-value string lives in exactly
        // one place across every caixa renderer. Pin the equality +
        // static-data identity here so any local re-introduction of a
        // sibling `pub const CILIUM_AUTH_MODE_DISABLED: &str = "…"` is a
        // build-time test failure naming the offending drift, not a
        // silent apply-time symptom — the prior shape would have let a
        // rebrand silently erase the author's explicit-opt-out intent at
        // the emit boundary. Peer to
        // [`cilium_auth_mode_required_re_export_points_at_caixa_core_canonical`]
        // — the two per-arm re-export pins together complete the per-
        // authn-block `(mode → {required, disabled})` author-reachable-
        // scalar-value-pair single-sourcing.
        caixa_core::assert_str_reexport_identity(
            "CILIUM_AUTH_MODE_DISABLED",
            CILIUM_AUTH_MODE_DISABLED,
            caixa_core::CILIUM_AUTH_MODE_DISABLED,
        );
    }

    #[test]
    fn m3_placement_estrategia_single_node_re_export_points_at_caixa_core_canonical() {
        // The `M3_PLACEMENT_ESTRATEGIA_SINGLE_NODE` re-export was lifted
        // from the (author-facing but not yet emit-referenced by this
        // crate) inline `"SingleNode"` scalar the
        // [`caixa_core::aplicacao::PlacementStrategy::SingleNode`] variant
        // serializes as under [`caixa_core::M3_PLACEMENT_KEY_ESTRATEGIA`].
        // Pin the equality + static-data identity here so any local re-
        // introduction of a sibling `pub const M3_PLACEMENT_ESTRATEGIA_SINGLE_NODE:
        // &str = "…"` is a build-time test failure naming the offending
        // drift, not a silent apply-time symptom at the aggregator's
        // per-entry strategy dispatch or the operator's reconcile posture
        // — the prior shape would have let an aplicacao-side variant
        // rename or `#[serde(rename_all = …)]` attribute silently rebrand
        // the emitted scalar under one spelling while every caixa-mesh
        // probe still checked another. Peer to
        // [`m3_placement_estrategia_replicated_re_export_points_at_caixa_core_canonical`]
        // and
        // [`m3_placement_estrategia_sharded_re_export_points_at_caixa_core_canonical`]
        // on the other two arms of the same closed
        // [`caixa_core::aplicacao::PlacementStrategy`] enum surface — the
        // three per-arm pins together complete the per-strategy
        // discriminator scalar-value single-sourcing across the M3
        // distribution-strategy dispatch axis.
        caixa_core::assert_str_reexport_identity(
            "M3_PLACEMENT_ESTRATEGIA_SINGLE_NODE",
            M3_PLACEMENT_ESTRATEGIA_SINGLE_NODE,
            caixa_core::M3_PLACEMENT_ESTRATEGIA_SINGLE_NODE,
        );
    }

    #[test]
    fn m3_placement_estrategia_replicated_re_export_points_at_caixa_core_canonical() {
        // Peer of `m3_placement_estrategia_single_node_re_export_points_at_caixa_core_canonical`
        // on the every-cluster-active-active arm of the same
        // [`caixa_core::aplicacao::PlacementStrategy`] enum — the arm the
        // enum's `default()` maps to, so drift here silently rebrands the
        // substrate's default distribution posture across every Aplicacao
        // that never declares the slot explicitly. This is the same
        // constant the `programs_entry_placement_carries_strategy` probe
        // dispatches on (the sweep lands here at the sole author-facing
        // consumption site the M3.x roadmap has today).
        caixa_core::assert_str_reexport_identity(
            "M3_PLACEMENT_ESTRATEGIA_REPLICATED",
            M3_PLACEMENT_ESTRATEGIA_REPLICATED,
            caixa_core::M3_PLACEMENT_ESTRATEGIA_REPLICATED,
        );
    }

    #[test]
    fn m3_placement_estrategia_sharded_re_export_points_at_caixa_core_canonical() {
        // Peer of the SingleNode / Replicated pins on the hash-keyed-
        // across-clusters arm of the same
        // [`caixa_core::aplicacao::PlacementStrategy`] enum — the one arm
        // on which the sibling [`caixa_core::M3_PLACEMENT_KEY_SHARD_KEY`]
        // sub-block is required (`AplicacaoSpec::validate_placement`
        // gates `shard_key.is_some() == matches!(estrategia, Sharded)` as
        // a structural partition of every validated Placement). Drift
        // here silently collapses the hash-keyed distribution back onto
        // the aggregator's default (Replicated) and every sharded
        // workload's per-entity routing invariant vanishes at the data
        // plane. This is the same constant the
        // `programs_entry_placement_carries_shard_key_when_sharded` probe
        // dispatches on.
        caixa_core::assert_str_reexport_identity(
            "M3_PLACEMENT_ESTRATEGIA_SHARDED",
            M3_PLACEMENT_ESTRATEGIA_SHARDED,
            caixa_core::M3_PLACEMENT_ESTRATEGIA_SHARDED,
        );
    }

    #[test]
    fn cilium_key_http_re_export_points_at_caixa_core_canonical() {
        // The renderer's `CILIUM_KEY_HTTP` was lifted from the inline
        // `"http"` literal at the `cilium_network_policies` per-`(:de,
        // :para)` CNP `rules.insert("http", …)` call site in the
        // `WitTarget::Http` L7 introspection emit branch (the per-
        // `toPorts[]` L7-HTTP-rule-list-discriminator container-axis
        // emit the Cilium data plane's per-`toPorts[]` L7 dispatch pass
        // reads to source the per-`toPorts[]` L7 URL-path-prefix
        // predicate list the ingress rule was authored to filter each
        // HTTP-shaped `:contratos` flow through) plus two test-side
        // navigations: the L7-fan-in path-capture pin across the multi-
        // edge group and the per-HTTP-contract L7-path presence pin —
        // to a re-export of [`caixa_core::CILIUM_KEY_HTTP`] so the
        // Cilium-CRD per-`toPorts[]` L7-HTTP-rule-list-discriminator
        // container-axis key string lives in exactly one place across
        // every caixa renderer. Pin the equality + static-data identity
        // here so any local re-introduction of a sibling `pub const
        // CILIUM_KEY_HTTP: &str = "…"` (the canonical drift footgun
        // where a sibling local `pub const` could happen to carry the
        // same string at the source while pointing at a different
        // `&'static` allocation) is a build-time test failure naming
        // the offending drift, not a silent apply-time symptom — the
        // prior shape would have let a Cilium-CRD schema rebrand on
        // the per-`toPorts[]` L7-HTTP-rule-list-discriminator axis
        // without a coordinated caixa-core edit silently land per-
        // `(:de, :para)` CiliumNetworkPolicy documents whose per-
        // `toPorts[]` entry L7-HTTP-rule-list-discriminator container-
        // axis key the Cilium CRD schema validator drops as
        // unrecognized at apply time; the per-`toPorts[]` entry falls
        // back to L4-only enforcement — no L7 URL-path predicate is
        // applied — silently admitting every HTTP-method / URL-path
        // combination the ingress rule was authored to filter to the
        // exact path prefix set the typed `:contratos` graph names at
        // the L7 introspection axis. Peer to
        // [`cilium_key_mode_re_export_points_at_caixa_core_canonical`] /
        // [`cilium_key_authentication_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-ingress-rule mutual-auth body/leaf axis
        // re-export pair — completes the per-`toPorts[]` L7-
        // introspection `(rules → http)` container/protocol-
        // discriminator axis re-export pair this crate's
        // `cilium_network_policies` renderer's HTTP-shaped-`:contratos`
        // URL-path-prefix-filtering L7-enforcement contract rests on.
        caixa_core::assert_str_reexport_identity(
            "CILIUM_KEY_HTTP",
            CILIUM_KEY_HTTP,
            caixa_core::CILIUM_KEY_HTTP,
        );
    }

    #[test]
    fn cilium_key_path_re_export_points_at_caixa_core_canonical() {
        // The renderer's `CILIUM_KEY_PATH` was lifted from the inline
        // `"path"` literal at the `cilium_network_policies` per-`(:de,
        // :para)` CNP `http_rule.insert("path", …)` call site in the
        // `WitTarget::Http` L7 introspection emit branch (the per-
        // `rules.http[]` URL-path-predicate leaf-scalar-axis emit the
        // Cilium data plane's per-HTTP-rule L7 dispatch pass reads to
        // source the per-HTTP-rule URL-path predicate scalar the ingress
        // rule was authored to filter each HTTP-shaped `:contratos`
        // flow through) plus one test-side navigation: the per-HTTP-
        // rule URL-path-predicate presence-and-value pin on the
        // aplicacao fixture's cart→catalog HTTP-shaped `:contratos`
        // edge — to a re-export of [`caixa_core::CILIUM_KEY_PATH`] so
        // the Cilium-CRD per-`rules.http[]` URL-path-predicate leaf-
        // axis key string lives in exactly one place across every
        // caixa renderer. Pin the equality + static-data identity here
        // so any local re-introduction of a sibling `pub const
        // CILIUM_KEY_PATH: &str = "…"` (the canonical drift footgun
        // where a sibling local `pub const` could happen to carry the
        // same string at the source while pointing at a different
        // `&'static` allocation) is a build-time test failure naming
        // the offending drift, not a silent apply-time symptom — the
        // prior shape would have let a Cilium-CRD schema rebrand on
        // the per-`rules.http[]` URL-path-predicate leaf-axis without
        // a coordinated caixa-core edit silently land per-`(:de, :para)`
        // CiliumNetworkPolicy documents whose per-`rules.http[]` entry
        // URL-path-predicate leaf-axis key the Cilium CRD schema
        // validator drops as unrecognized at apply time; the per-
        // `rules.http[]` entry falls back to a match-any-URL-path
        // predicate — the per-`toPorts[]` L7 rule admits every URL
        // path on the destination port silently, bypassing the URL-
        // path-prefix predicate the typed `:contratos` HTTP-shaped
        // edge's `:endpoint` slot names at the L7 introspection axis,
        // with no field naming the URL-path-predicate-leaf-axis-drift
        // root cause. Peer to
        // [`cilium_key_http_re_export_points_at_caixa_core_canonical`]
        // on the parent per-`toPorts[]` L7-HTTP-rule-list-
        // discriminator container-axis re-export — descends the per-
        // `toPorts[]` L7-introspection `(rules → http → path)`
        // container / protocol-discriminator / URL-path-predicate axis
        // re-export chain one leaf level beneath the parent
        // `CILIUM_KEY_HTTP` per-`toPorts[]` L7-HTTP-rule-list-
        // discriminator re-export it nests inside, completing the per-
        // `toPorts[]` L7-introspection `(rules → http → path)`
        // container / protocol-discriminator / URL-path-predicate axis
        // re-export triple this crate's `cilium_network_policies`
        // renderer's HTTP-shaped-`:contratos` URL-path-prefix-
        // filtering L7-enforcement contract rests on. Distinct from
        // the sibling K8s-Gateway-API-side
        // [`gateway_api_key_path_re_export_points_at_caixa_core_canonical`]
        // — both re-exports carry the same underlying `"path"` string
        // but name distinct schema axes on distinct CRD groups (the
        // Cilium-side leaf on the `cilium.io/v2` `CiliumNetworkPolicy`
        // CRD's per-`rules.http[]` entry, the Gateway-API-side
        // container on the `gateway.networking.k8s.io/v1` `HTTPRoute`
        // CRD's `spec.rules[].matches[]` entry), so this test asserts
        // pointer-identity against the Cilium-side canonical
        // declaration (not the Gateway-API-side canonical declaration
        // the sibling re-export test asserts against), pinning the
        // axis-independence discipline the two sibling re-exports rest
        // on against a future coalescing edit that would erase the
        // per-CRD-group distinction.
        caixa_core::assert_str_reexport_identity(
            "CILIUM_KEY_PATH",
            CILIUM_KEY_PATH,
            caixa_core::CILIUM_KEY_PATH,
        );
    }

    #[test]
    fn cilium_key_path_and_gateway_api_key_path_stay_independent_axes() {
        // Axis-independence pin: the Cilium-CRD per-`rules.http[]`
        // URL-path-predicate leaf-axis (`CILIUM_KEY_PATH`) and the K8s
        // Gateway API v1 `HTTPRoute` per-`HTTPRouteMatch` path-matcher
        // container-axis (`GATEWAY_API_KEY_PATH`) spell the same
        // underlying `"path"` string but name distinct schema axes on
        // distinct CRD groups (the Cilium-side leaf on the
        // `cilium.io/v2` `CiliumNetworkPolicy` CRD, the Gateway-API-
        // side container on the `gateway.networking.k8s.io/v1`
        // `HTTPRoute` CRD). Rust's `&'static str` interner coalesces
        // identical byte-sequences onto one storage allocation at
        // codegen time, so a pointer-identity assertion against
        // `.as_ptr()` between the two constants can't distinguish
        // "sibling `pub const` declarations carrying identical bytes"
        // from "coalesced canonical declaration" at runtime — the
        // axis-independence discipline this test names lives at the
        // rustc symbol-name axis (two separate `pub const` symbols
        // whose bindings a future rebrand of one leaves the other
        // structurally untouched) rather than the runtime-address
        // axis. Pin equality of the string bytes (so a downstream
        // consumer that expects `"path"` at either axis gets it),
        // pin equality of each half against its own caixa-core
        // canonical declaration (so the sibling
        // [`cilium_key_path_re_export_points_at_caixa_core_canonical`]
        // / [`gateway_api_key_path_re_export_points_at_caixa_core_canonical`]
        // re-export identity pins remain the load-bearing per-axis
        // "no sibling local `pub const` drift" gate this test rests
        // on, not this test itself), and let the two distinct
        // `pub const CILIUM_KEY_PATH` / `pub const GATEWAY_API_KEY_PATH`
        // symbol declarations carry the per-CRD-group axis-
        // independence at the type-symbol level any future rustc
        // codegen the substrate consumes preserves by construction.
        // A future coalescing edit collapsing the two symbols onto
        // one canonical declaration in caixa-core (the axis-
        // coalescence regression this pin names) would surface at
        // the sibling per-axis re-export identity pins — the local
        // `CILIUM_KEY_PATH` re-export would begin pointing at the
        // Gateway-API-side canonical declaration (or vice-versa),
        // fingering the offending axis on the sibling test's failure
        // message — rather than at this cross-axis pointer-identity
        // pin. So this test asserts the weaker string-equality
        // property that any future rustc string-interner behavior
        // preserves, and defers the load-bearing per-axis
        // "no sibling local `pub const` drift" gate to the sibling
        // per-axis re-export identity pins.
        assert_eq!(CILIUM_KEY_PATH, GATEWAY_API_KEY_PATH);
        assert_eq!(CILIUM_KEY_PATH, caixa_core::CILIUM_KEY_PATH);
        assert_eq!(GATEWAY_API_KEY_PATH, caixa_core::GATEWAY_API_KEY_PATH);
    }

    #[test]
    fn cilium_l7_rule_list_carries_lifted_cilium_key_http() {
        // Production-emit pin: traverse a rendered CNP's
        // `spec.ingress[0].toPorts[0].rules` L7-rule-list-container
        // block and assert the L7-HTTP-rule-list-discriminator entry
        // is keyed by the lifted `CILIUM_KEY_HTTP` (`"http"`) verbatim
        // — the load-bearing per-`toPorts[]` L7-HTTP-rule-list-
        // discriminator container-axis key the Cilium data plane's
        // per-`toPorts[]` L7 dispatch pass reads to source the per-
        // `toPorts[]` L7 URL-path-prefix predicate list before applying
        // the per-request URL-path predicate against the observed
        // HTTP request line. Before the lift the emitter carried an
        // inline `"http".into()` literal at the sole `rules.insert(…)`
        // call site in the `WitTarget::Http` L7 introspection emit
        // branch; a typo there (`"HTTP"` / `"Http"` / `"httpRules"`)
        // would have silently landed a per-`toPorts[]` entry whose
        // L7-HTTP-rule-list-discriminator key the Cilium CRD schema
        // validator drops as unknown at admission, and the per-
        // `toPorts[]` entry would have fallen back to L4-only
        // enforcement — no L7 URL-path predicate applied — silently
        // admitting every HTTP-method / URL-path combination the
        // ingress rule was authored to filter to the exact path prefix
        // set the typed `:contratos` graph names at the L7
        // introspection axis, with no field naming the L7-HTTP-rule-
        // list-discriminator-drift root cause. Peer to
        // `cilium_port_tuple_carries_lifted_kube_protocol_tcp`'s per-
        // `toPorts[].ports[]` L4-transport-protocol scalar pin on the
        // sibling per-`toPorts[]` L4-tuple surface — extends the per-
        // `toPorts[]` L4-tuple-scalar pin discipline onto the sibling
        // per-`toPorts[]` L7-rule-list-container-axis pin surface
        // every `cilium_network_policies` intra-mesh L7-URL-path-
        // predicate-gating emit carries under the shared
        // `CiliumNetworkPolicy` body.
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        let rules = policies
            .iter()
            .find_map(|p| {
                kube_spec_field(p, CILIUM_KEY_INGRESS)
                    .and_then(|i| i.as_sequence())
                    .and_then(|s| s.first())
                    .and_then(|i| i.get(CILIUM_KEY_TO_PORTS))
                    .and_then(|p| p.as_sequence())
                    .and_then(|s| s.iter().find(|tp| tp.get(KUBE_KEY_RULES).is_some()))
                    .and_then(|tp| tp.get(KUBE_KEY_RULES))
            })
            .expect(
                "at least one per-`toPorts[]` entry emits a `rules` \
                 L7-rule-list-container block on the aplicacao fixture's \
                 HTTP-shaped `:contratos` edges",
            );
        assert!(
            rules.get(CILIUM_KEY_HTTP).is_some(),
            "per-`toPorts[]` `rules` L7-rule-list-container must carry \
             the lifted `CILIUM_KEY_HTTP` (`\"http\"`) L7-HTTP-rule-list-\
             discriminator key verbatim — the load-bearing Cilium CRD \
             per-`toPorts[]` L7-HTTP-rule-list-discriminator container-\
             axis key the Cilium data plane's per-`toPorts[]` L7 dispatch \
             pass reads to source the per-`toPorts[]` L7 URL-path-prefix \
             predicate list, got rules = {rules:?}"
        );
    }

    #[test]
    fn kube_key_type_re_export_points_at_caixa_core_canonical() {
        // The renderer's `KUBE_KEY_TYPE` was lifted from the inline
        // `"type"` literal at the `gateway_routes` per-`HTTPRouteMatch`
        // `path_match.insert("type", …)` call site (the sole production-
        // code site the prior literal sat at — the per-`HTTPRouteMatch`
        // path-selection-predicate discriminator scalar-key the
        // gateway-class-controller's per-rule L7 dispatch pass reads to
        // source the path-match strategy from the closed
        // `PathMatchType` OpenAPI schema enum's
        // `{Exact, PathPrefix, RegularExpression}` set) to a re-export
        // of [`caixa_core::KUBE_KEY_TYPE`] so the K8s-CR discriminated-
        // union type scalar-discriminator key string lives in exactly
        // one place across every caixa renderer. Pin the equality +
        // static-data identity here so any local re-introduction of a
        // sibling `pub const KUBE_KEY_TYPE: &str = "…"` (the canonical
        // drift footgun where a sibling local `pub const` could happen
        // to carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test failure
        // naming the offending drift, not a silent apply-time symptom —
        // the prior shape would have let a K8s API conventions rebrand
        // on the caixa-mesh side without a coordinated caixa-core edit
        // silently land per-Aplicacao `HTTPRoute` documents whose per-
        // `HTTPRouteMatch` path-selection-predicate discriminator
        // scalar-key the Gateway API v1 `HTTPPathMatch` OpenAPI schema
        // validator drops as unknown at apply time; the per-match entry
        // falls back to the schema-side default path-match-strategy,
        // silently admitting every URL-path prefix the ingress rule was
        // authored to filter to the exact predicate the typed `:entrada
        // :paths` slot names at the request-path-selection axis. Peer to
        // [`gateway_api_path_match_type_path_prefix_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-`HTTPRouteMatch` path-selection-predicate
        // discriminator scalar-VALUE axis re-export — closes the per-
        // `HTTPRouteMatch` path-selection-predicate `(type key →
        // PathPrefix value)` scalar-key/scalar-value discriminator axis
        // pair this crate's `gateway_routes` renderer's external
        // `:entrada` per-path L7-filtering ingress contract rests on.
        caixa_core::assert_str_reexport_identity(
            "KUBE_KEY_TYPE",
            KUBE_KEY_TYPE,
            caixa_core::KUBE_KEY_TYPE,
        );
    }

    #[test]
    fn httproute_path_match_carries_lifted_kube_key_type() {
        // Production-emit pin: traverse a rendered `HTTPRoute`'s per-
        // `HTTPRouteMatch` `spec.rules[].matches[].path` mapping and
        // assert the path-selection-predicate discriminator entry is
        // keyed by the lifted `KUBE_KEY_TYPE` (`"type"`) verbatim — the
        // load-bearing per-`HTTPRouteMatch` path-selection-predicate
        // discriminator scalar-key the gateway-class-controller's per-
        // rule L7 dispatch pass reads to source the path-match strategy
        // (the closed `PathMatchType` OpenAPI schema enum's `{Exact,
        // PathPrefix, RegularExpression}` set) before applying the per-
        // match request-path predicate against the observed HTTP request
        // line's `:path` pseudo-header. Before the lift the emitter
        // carried an inline `"type".into()` literal at the sole
        // `path_match.insert(…)` call site in the `gateway_routes` per-
        // path iteration; a typo there (`"Type"` / `"kind"` /
        // `"discriminator"` / `"predicate"`) would have silently landed
        // a per-match entry whose path-selection-predicate discriminator
        // scalar-key the Gateway API v1 `HTTPPathMatch` OpenAPI schema
        // validator drops as unknown at admission, and the per-match
        // entry would have fallen back to the schema-side default path-
        // match-strategy — silently admitting every URL-path prefix the
        // ingress rule was authored to filter to the exact predicate the
        // typed `:entrada :paths` slot names at the request-path-
        // selection axis, with no field naming the discriminator-scalar-
        // key-drift root cause. Peer to
        // `cilium_l7_rule_list_carries_lifted_cilium_key_http`'s per-
        // `toPorts[]` L7-HTTP-rule-list-discriminator container-axis
        // pin on the sibling per-`toPorts[]` L7-rule-list-container
        // surface — extends the per-CRD-body-axis lifted-uses pin
        // discipline onto the sibling per-`HTTPRouteMatch` path-
        // selection-predicate discriminator scalar-key axis every
        // `gateway_routes` external `:entrada` per-path L7-filtering
        // emit carries under the shared `HTTPRoute` body.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let rules = httproute_rules(&docs);
        assert!(
            !rules.is_empty(),
            "HTTPRoute must carry at least one rule the per-match path-\
             selection-predicate discriminator scalar-key nests under"
        );
        for rule in &rules {
            let matches = rule
                .get(caixa_core::GATEWAY_API_KEY_MATCHES)
                .and_then(|m| m.as_sequence())
                .expect("HTTPRoute per-rule spec.rules[].matches sequence");
            for m in matches {
                let path = m
                    .get(caixa_core::GATEWAY_API_KEY_PATH)
                    .and_then(|p| p.as_mapping())
                    .expect(
                        "HTTPRoute per-match spec.rules[].matches[].path must be \
                         navigable through the lifted GATEWAY_API_KEY_PATH constant",
                    );
                assert!(
                    path.get(KUBE_KEY_TYPE).is_some(),
                    "per-`HTTPRouteMatch` `path` block must carry \
                     the lifted `KUBE_KEY_TYPE` (`\"type\"`) path-\
                     selection-predicate discriminator scalar-key \
                     verbatim — the load-bearing Gateway API v1 \
                     HTTPPathMatch canonical discriminator-key axis \
                     the gateway-class-controller's per-rule L7 \
                     dispatch pass reads to source the path-match \
                     strategy, got path = {path:?}"
                );
            }
        }
    }

    #[test]
    fn gateway_api_kind_gateway_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_KIND_GATEWAY` was lifted from the
        // inline `"Gateway"` literal at the `gateway_routes`
        // `kube_resource_skeleton` kind argument
        // (caixa-mesh/src/lib.rs:578 — the per-Aplicacao Gateway emit
        // site) to a re-export of [`caixa_core::GATEWAY_API_KIND_GATEWAY`]
        // so the Gateway-API-conformant CRD `kind` discriminator string
        // lives in exactly one place across every caixa renderer. Pin
        // the equality + static-data identity here so any local
        // re-introduction of a sibling
        // `pub const GATEWAY_API_KIND_GATEWAY: &str = "…"` (the canonical
        // drift footgun where a sibling local `pub const` could happen
        // to carry the same string at the source while pointing at a
        // different `&'static` allocation) is a build-time test failure
        // naming the offending drift, not a silent apply-time symptom —
        // the prior shape would have let a Gateway-API kind rebrand on
        // the caixa-mesh side without a coordinated caixa-core edit
        // silently land per-Aplicacao Gateway objects at one CRD kind
        // and every future per-target Gateway-API-side renderer's
        // emitted `GatewayClass` / `TCPRoute` / `TLSRoute` / `GRPCRoute`
        // at the drifted other, with every external `:entrada` flow
        // dropping at the gateway-class-controller's reconcile loop
        // because the per-route attached-policy pipeline never binds
        // across the kind-drifted CRD-discriminator pair. Peer to
        // [`gateway_api_api_version_re_export_points_at_caixa_core_canonical`]
        // on the sibling Gateway-API-CRD-apiVersion-re-export axis —
        // begins the per-Gateway-API-CRD kind+apiVersion re-export pair
        // this crate's `gateway_routes` renderer's external `:entrada`
        // ingress contract rests on. Peer to
        // [`cilium_kind_network_policy_re_export_points_at_caixa_core_canonical`]
        // on the sibling Cilium-CRD-kind-discriminator re-export axis.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_KIND_GATEWAY",
            GATEWAY_API_KIND_GATEWAY,
            caixa_core::GATEWAY_API_KIND_GATEWAY,
        );
    }

    #[test]
    fn gateway_api_kind_http_route_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_KIND_HTTP_ROUTE` was lifted from
        // the inline `"HTTPRoute"` literal at the `gateway_routes`
        // `kube_resource_skeleton` kind argument
        // (caixa-mesh/src/lib.rs:663 — the per-Aplicacao HTTPRoute emit
        // site) to a re-export of
        // [`caixa_core::GATEWAY_API_KIND_HTTP_ROUTE`] so the
        // Gateway-API-conformant CRD `kind` discriminator string lives
        // in exactly one place across every caixa renderer. Pin the
        // equality + static-data identity here so any local
        // re-introduction of a sibling
        // `pub const GATEWAY_API_KIND_HTTP_ROUTE: &str = "…"` (the
        // canonical drift footgun where a sibling local `pub const`
        // could happen to carry the same string at the source while
        // pointing at a different `&'static` allocation) is a
        // build-time test failure naming the offending drift, not a
        // silent apply-time symptom — the prior shape would have let a
        // Gateway-API kind rebrand on the caixa-mesh side without a
        // coordinated caixa-core edit silently land per-Aplicacao
        // HTTPRoute objects at one CRD kind and every future
        // per-target Gateway-API-side renderer's emitted `TCPRoute` /
        // `TLSRoute` / `GRPCRoute` at the drifted other, with every
        // external `:entrada` flow dropping at the
        // gateway-class-controller's reconcile loop because the
        // per-route attached-policy pipeline never binds across the
        // kind-drifted CRD-discriminator pair. Peer to
        // [`gateway_api_kind_gateway_re_export_points_at_caixa_core_canonical`]
        // on the sibling parent-Gateway-`kind`-discriminator re-export
        // axis — completes the per-Gateway-API-CRD `kind`-axis
        // re-export pair this crate's `gateway_routes` renderer's
        // external `:entrada` ingress contract rests on across the
        // `(Gateway, HTTPRoute)` pair the renderer emits together.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_KIND_HTTP_ROUTE",
            GATEWAY_API_KIND_HTTP_ROUTE,
            caixa_core::GATEWAY_API_KIND_HTTP_ROUTE,
        );
    }

    #[test]
    fn gateway_api_protocol_http_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_PROTOCOL_HTTP` was lifted from the
        // inline `"HTTP".into()` literal at the `gateway_routes`
        // per-`:entrada` `Gateway`'s per-listener `KUBE_KEY_PROTOCOL`
        // scalar-value emit (caixa-mesh/src/lib.rs:2165 — the sole
        // production-code call site the prior literal sat at) to a
        // re-export of [`caixa_core::GATEWAY_API_PROTOCOL_HTTP`] so the
        // Gateway-API-v1-`ProtocolType`-conformant HTTP listener-protocol
        // scalar value string lives in exactly one place across every
        // caixa renderer. Pin the equality + static-data identity here
        // so any local re-introduction of a sibling
        // `pub const GATEWAY_API_PROTOCOL_HTTP: &str = "…"` (the
        // canonical drift footgun where a sibling local `pub const`
        // could happen to carry the same string at the source while
        // pointing at a different `&'static` allocation) is a
        // build-time test failure naming the offending drift, not a
        // silent apply-time symptom — the prior shape would have let a
        // Gateway-API `ProtocolType` rebrand on the caixa-mesh side
        // without a coordinated caixa-core edit silently land per-
        // `:entrada` `Gateway` objects at one listener-protocol scalar
        // and every future per-Aplicacao multi-listener fan-out
        // renderer's emitted `Gateway` at the drifted other, with every
        // external `:entrada` HTTP flow dropping at the gateway-class-
        // controller's admission gate because the K8s Gateway API v1
        // `ProtocolType` OpenAPI schema enum only admits the closed set
        // `{"HTTP", "HTTPS", "TCP", "TLS", "UDP"}` verbatim. Peer to
        // [`gateway_api_kind_gateway_re_export_points_at_caixa_core_canonical`]
        // + [`gateway_api_kind_http_route_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-Gateway-API-CRD-`kind`-discriminator
        // re-export pair + [`default_gateway_class_name_re_export_points_at_caixa_core_canonical`]
        // on the sibling Gateway-controller-binding-scalar-value axis —
        // extends the pair of `kind`-value + controller-binding-value
        // re-export drift pins across the `(Gateway, HTTPRoute)` pair
        // onto the sibling per-Gateway `spec.listeners[].protocol`
        // listener-protocol-scalar-value axis this crate's
        // `gateway_routes` renderer's external `:entrada` ingress
        // contract rests on.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_PROTOCOL_HTTP",
            GATEWAY_API_PROTOCOL_HTTP,
            caixa_core::GATEWAY_API_PROTOCOL_HTTP,
        );
    }

    #[test]
    fn gateway_api_path_match_type_path_prefix_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_PATH_MATCH_TYPE_PATH_PREFIX` was
        // lifted from the inline `"PathPrefix".into()` literal at the
        // `gateway_routes` per-match `path_match.insert("type", …)`
        // per-`HTTPRouteMatch` path-selection-predicate scalar-value
        // emit (the sole production-code call site the prior literal
        // sat at) to a re-export of
        // [`caixa_core::GATEWAY_API_PATH_MATCH_TYPE_PATH_PREFIX`] so
        // the Gateway-API-v1-`PathMatchType`-conformant per-match
        // request-path-selection-predicate discriminator scalar value
        // string lives in exactly one place across every caixa
        // renderer. Pin the equality + static-data identity here so any
        // local re-introduction of a sibling
        // `pub const GATEWAY_API_PATH_MATCH_TYPE_PATH_PREFIX: &str =
        // "…"` (the canonical drift footgun where a sibling local
        // `pub const` could happen to carry the same string at the
        // source while pointing at a different `&'static` allocation)
        // is a build-time test failure naming the offending drift, not
        // a silent apply-time symptom — the prior shape would have let
        // a Gateway-API `PathMatchType` rebrand on the caixa-mesh side
        // without a coordinated caixa-core edit silently land per-
        // `:entrada` `HTTPRoute` objects at one path-selection-
        // predicate scalar and every future per-Aplicacao multi-
        // predicate fan-out renderer's emitted `HTTPRoute` at the
        // drifted other, with every external `:entrada` path-filtered
        // flow dropping at the gateway-class-controller's admission
        // gate because the K8s Gateway API v1 `PathMatchType` OpenAPI
        // schema enum only admits the closed set
        // `{"Exact", "PathPrefix", "RegularExpression"}` verbatim. Peer
        // to
        // [`gateway_api_protocol_http_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-Gateway-listener L7-parser-selection
        // scalar-value re-export axis — extends the canonical-Gateway-
        // API-v1-OpenAPI-schema-enum-value re-export drift pin the
        // `ProtocolType.HTTP` re-export pin established onto the
        // sibling `PathMatchType.PathPrefix` per-`HTTPRouteMatch`
        // path-selection-predicate discriminator this crate's
        // `gateway_routes` renderer's external `:entrada` ingress
        // contract rests on under the shared `HTTPRoute` body.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_PATH_MATCH_TYPE_PATH_PREFIX",
            GATEWAY_API_PATH_MATCH_TYPE_PATH_PREFIX,
            caixa_core::GATEWAY_API_PATH_MATCH_TYPE_PATH_PREFIX,
        );
    }

    #[test]
    fn default_gateway_class_name_re_export_points_at_caixa_core_canonical() {
        // The renderer's `DEFAULT_GATEWAY_CLASS_NAME` was lifted from the
        // inline `"cilium".into()` literal at the `gateway_routes`
        // per-`:entrada` `Gateway` `spec.gatewayClassName` field to a
        // re-export of [`caixa_core::DEFAULT_GATEWAY_CLASS_NAME`] so the
        // substrate's chosen K8s Gateway API controller-discriminator
        // lives in exactly one place across every caixa renderer. Pin
        // the equality + static-data identity here so any local
        // re-introduction of a sibling
        // `pub const DEFAULT_GATEWAY_CLASS_NAME: &str = "…"` (the
        // canonical drift footgun where a sibling local `pub const`
        // could happen to carry the same string at the source while
        // pointing at a different `&'static` allocation) is a
        // build-time test failure naming the offending drift, not a
        // silent apply-time symptom — the prior shape would have let a
        // substrate-side Gateway API controller migration on the
        // caixa-mesh side without a coordinated caixa-core edit
        // silently land per-`:entrada` `Gateway` objects at one
        // `gatewayClassName` and every future per-Aplicacao
        // materializer's emitted `Gateway` at the drifted other, with
        // every external `:entrada` flow dropping at the
        // gateway-class-controller's reconcile loop because the
        // per-route attached-policy pipeline never binds across the
        // controller-drifted `spec.gatewayClassName` pair. And
        // splitting the Gateway controller across renderers would
        // silently reintroduce the two-data-planes drift the mesh
        // composition "one identity layer, one data plane" invariant
        // (MESH-COMPOSITION.md §V) closes — the emitted `Gateway`'s
        // controller and the sibling `CiliumNetworkPolicy`'s eBPF
        // reconciler would land in distinct data planes, and the
        // intra-mesh identity-aware policy would stop matching the
        // ingress-side traffic at the eBPF data plane. Peer to
        // [`default_namespace_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-substrate-default-resource-name
        // re-export axis.
        caixa_core::assert_str_reexport_identity(
            "DEFAULT_GATEWAY_CLASS_NAME",
            DEFAULT_GATEWAY_CLASS_NAME,
            caixa_core::DEFAULT_GATEWAY_CLASS_NAME,
        );
    }

    #[test]
    fn gateway_api_key_gateway_class_name_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_KEY_GATEWAY_CLASS_NAME` was
        // lifted from the inline `"gatewayClassName"` literal at the
        // `gateway_routes` per-Aplicacao Gateway's
        // `g_spec.insert("gatewayClassName", …)` call site
        // (caixa-mesh/src/lib.rs:2060 — the sole per-production-code
        // controller-binding-scalar-axis-KEY emitter) to a re-export
        // of [`caixa_core::GATEWAY_API_KEY_GATEWAY_CLASS_NAME`] so
        // the Gateway-API-CRD per-Gateway controller-binding-scalar-
        // axis-key string lives in exactly one place across every
        // caixa renderer. Pin the equality + static-data identity
        // here so any local re-introduction of a sibling
        // `pub const GATEWAY_API_KEY_GATEWAY_CLASS_NAME: &str = "…"`
        // (the canonical drift footgun where a sibling local
        // `pub const` could happen to carry the same string at the
        // source while pointing at a different `&'static` allocation)
        // is a build-time test failure naming the offending drift,
        // not a silent apply-time symptom — the prior shape would
        // have let a Gateway-API-CRD controller-binding-axis rebrand
        // on the caixa-mesh side without a coordinated caixa-core
        // edit silently land per-Aplicacao Gateway objects at one
        // controller-binding key and every future per-Aplicacao
        // materializer's emitted `Gateway` at the drifted other,
        // with every external `:entrada` flow dropping at the
        // gateway-class-controller's reconcile loop because the
        // per-Gateway `GatewayClass` lookup never binds across the
        // KEY-drifted controller-binding-axis pair. Peer to
        // [`default_gateway_class_name_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-Gateway-API-`(key, value)`-pair-
        // lift re-export axis this pin closes the KEY half of, and
        // to
        // [`gateway_api_key_listeners_re_export_points_at_caixa_core_canonical`]
        // on the sibling per-Gateway-body-axis re-export surface.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_KEY_GATEWAY_CLASS_NAME",
            GATEWAY_API_KEY_GATEWAY_CLASS_NAME,
            caixa_core::GATEWAY_API_KEY_GATEWAY_CLASS_NAME,
        );
    }

    #[test]
    fn gateway_api_key_parent_refs_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_KEY_PARENT_REFS` was lifted from
        // the inline `"parentRefs"` literal at the `gateway_routes`
        // per-Aplicacao HTTPRoute's `r_spec.insert("parentRefs", …)`
        // call site (caixa-mesh/src/lib.rs:1389 — the sole per-
        // production-code parent-Gateway-binding-container-axis
        // emitter) to a re-export of
        // [`caixa_core::GATEWAY_API_KEY_PARENT_REFS`] so the
        // Gateway-API-CRD per-HTTPRoute parent-Gateway-binding-
        // container-axis-key string lives in exactly one place across
        // every caixa renderer. Pin the equality + static-data
        // identity here so any local re-introduction of a sibling
        // `pub const GATEWAY_API_KEY_PARENT_REFS: &str = "…"` (the
        // canonical drift footgun where a sibling local `pub const`
        // could happen to carry the same string at the source while
        // pointing at a different `&'static` allocation) is a
        // build-time test failure naming the offending drift, not a
        // silent apply-time symptom — the prior shape would have let a
        // Gateway-API-CRD parent-Gateway-binding-axis rebrand on the
        // caixa-mesh side without a coordinated caixa-core edit
        // silently land per-HTTPRoute parent-Gateway attachments at
        // the drifted axis; the route lands unattached, and every
        // external `:entrada` flow drops at the Gateway API
        // implementation's per-Gateway HTTP-listener fan-in with no
        // field naming the parent-Gateway-binding-drift root cause.
        // Peer to
        // [`cilium_key_ports_re_export_points_at_caixa_core_canonical`]
        // /
        // [`gateway_api_kind_http_route_re_export_points_at_caixa_core_canonical`]
        // /
        // [`gateway_api_kind_gateway_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-K8s-CRD-body-axis re-export set —
        // begins the per-Gateway-API-HTTPRoute-body-axis re-export
        // identity-pin set (`parentRefs`, future `hostnames`) this
        // crate's `gateway_routes` renderer's external `:entrada`
        // ingress contract rests on across the Gateway API HTTPRoute-
        // side per-route body-shape.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_KEY_PARENT_REFS",
            GATEWAY_API_KEY_PARENT_REFS,
            caixa_core::GATEWAY_API_KEY_PARENT_REFS,
        );
    }

    #[test]
    fn gateway_api_key_section_name_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_KEY_SECTION_NAME` was lifted from
        // the (previously omitted) per-parentRef listener-selector sub-
        // axis at the `gateway_routes` per-Aplicacao HTTPRoute's
        // `parent_ref.insert(GATEWAY_API_KEY_SECTION_NAME,
        // GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME)` call site (the sole
        // per-production-code per-parentRef listener-selector-sub-axis
        // emitter) to a re-export of
        // [`caixa_core::GATEWAY_API_KEY_SECTION_NAME`] so the Gateway-
        // API-CRD per-HTTPRoute per-parentRef listener-selector-sub-
        // axis-key string lives in exactly one place across every caixa
        // renderer. Pin the equality + static-data identity here so any
        // local re-introduction of a sibling
        // `pub const GATEWAY_API_KEY_SECTION_NAME: &str = "…"` (the
        // canonical drift footgun where a sibling local `pub const`
        // could happen to carry the same string at the source while
        // pointing at a different `&'static` allocation) is a build-
        // time test failure naming the offending drift, not a silent
        // apply-time symptom — the prior shape (the selector omitted
        // entirely) would have let a Gateway-API-CRD per-parentRef
        // listener-selector-axis rebrand on the caixa-mesh side without
        // a coordinated caixa-core edit silently land per-HTTPRoute
        // parent-Gateway attachments at the drifted axis; the route
        // reverts to the Gateway API v1 attach-to-every-listener
        // default fan-out, silently doubling per-request dispatch
        // surface once a second listener lands on the parent Gateway
        // (the HTTPS-by-default trajectory the paired
        // [`GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME`] docstring
        // forecasts). Peer to
        // [`gateway_api_key_parent_refs_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-Gateway-API-HTTPRoute-body-axis re-
        // export identity-pin set — nests the per-Gateway-API-
        // HTTPRoute-body-axis re-export identity-pin set (`parentRefs`,
        // `backendRefs`, future `hostnames`) one level deeper onto the
        // per-parentRef listener-selector sub-axis this crate's
        // `gateway_routes` renderer's external `:entrada` ingress
        // contract now rests on across the Gateway API HTTPRoute-side
        // per-parentRef body-shape.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_KEY_SECTION_NAME",
            GATEWAY_API_KEY_SECTION_NAME,
            caixa_core::GATEWAY_API_KEY_SECTION_NAME,
        );
        // Pin the byte-shape too so a future Gateway API v2 rebrand of
        // the per-parentRef listener-selector sub-axis (an upstream
        // SIG-Network Gateway API v2 rename to `listenerName` /
        // `listener` / `attachTo`) surfaces here as an explicit
        // byte-shape drift rather than a silent per-consumer dispatch
        // miss at the K8s apiserver-side CRD schema validator.
        assert_eq!(GATEWAY_API_KEY_SECTION_NAME, "sectionName");
    }

    #[test]
    fn gateway_api_key_backend_refs_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_KEY_BACKEND_REFS` was lifted from
        // the inline `"backendRefs"` literal at the `gateway_routes`
        // per-Aplicacao HTTPRoute's per-rule
        // `rule.insert("backendRefs", …)` call site (the sole per-
        // production-code per-rule backend-destination-container-axis
        // emitter) plus the matching test-side fixture sites
        // (`httproute_routes_to_entrada_para`'s `.get("backendRefs")`
        // navigation + `httproute_rule_keys_pin_overlay_position`'s
        // `contains_key("backendRefs")` presence pin) to a re-export of
        // [`caixa_core::GATEWAY_API_KEY_BACKEND_REFS`] so the Gateway-
        // API-CRD per-HTTPRoute per-rule backend-destination-container-
        // axis-key string lives in exactly one place across every caixa
        // renderer. Pin the equality + static-data identity here so any
        // local re-introduction of a sibling
        // `pub const GATEWAY_API_KEY_BACKEND_REFS: &str = "…"` (the
        // canonical drift footgun where a sibling local `pub const`
        // could happen to carry the same string at the source while
        // pointing at a different `&'static` allocation) is a
        // build-time test failure naming the offending drift, not a
        // silent apply-time symptom — the prior shape would have let a
        // Gateway-API-CRD per-rule backend-destination-axis rebrand on
        // the caixa-mesh side without a coordinated caixa-core edit
        // silently land per-rule backend fan-outs at the drifted axis;
        // no backend is picked at the per-rule L7 dispatch, and every
        // external `:entrada` request drops at the gateway-class-
        // controller's per-rule reconcile with no field naming the
        // backend-destination-drift root cause. Peer to
        // [`gateway_api_key_parent_refs_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-Gateway-API-HTTPRoute-body-axis re-
        // export set — extends the per-Gateway-API-HTTPRoute-body-axis
        // re-export identity-pin set (`parentRefs`, `backendRefs`,
        // future `hostnames`) this crate's `gateway_routes` renderer's
        // external `:entrada` ingress contract rests on across the
        // Gateway API HTTPRoute-side per-route body-shape.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_KEY_BACKEND_REFS",
            GATEWAY_API_KEY_BACKEND_REFS,
            caixa_core::GATEWAY_API_KEY_BACKEND_REFS,
        );
    }

    #[test]
    fn gateway_api_key_matches_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_KEY_MATCHES` was lifted from the
        // inline `"matches"` literal at the `gateway_routes` per-
        // Aplicacao HTTPRoute's per-rule
        // `rule.insert("matches", …)` call site (the sole per-
        // production-code per-rule route-match-container-axis
        // emitter) plus the matching test-side fixture site
        // (`httproute_rule_keys_pin_overlay_position`'s
        // `contains_key("matches")` presence pin) to a re-export of
        // [`caixa_core::GATEWAY_API_KEY_MATCHES`] so the Gateway-API-
        // CRD per-HTTPRoute per-rule route-match-container-axis-key
        // string lives in exactly one place across every caixa
        // renderer. Pin the equality + static-data identity here so
        // any local re-introduction of a sibling
        // `pub const GATEWAY_API_KEY_MATCHES: &str = "…"` (the
        // canonical drift footgun where a sibling local `pub const`
        // could happen to carry the same string at the source while
        // pointing at a different `&'static` allocation) is a
        // build-time test failure naming the offending drift, not a
        // silent apply-time symptom — the prior shape would have let
        // a Gateway-API-CRD per-rule route-match-axis rebrand on the
        // caixa-mesh side without a coordinated caixa-core edit
        // silently land per-rule request-selection predicates at the
        // drifted axis; the per-rule predicate degrades to the
        // wildcard match, the rule matches every request
        // unconditionally, and every external `:entrada` path filter
        // drops with no field naming the route-match-drift root
        // cause. Peer to
        // [`gateway_api_key_backend_refs_re_export_points_at_caixa_core_canonical`]
        // /
        // [`gateway_api_key_parent_refs_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-Gateway-API-HTTPRoute-body-axis
        // re-export identity-pin set — completes the per-rule top-
        // level-axis re-export identity-pin set (`matches`,
        // `backendRefs`, `timeouts`, `retry`) this crate's
        // `gateway_routes` renderer's external `:entrada` ingress
        // contract rests on across the Gateway API HTTPRoute per-
        // rule body-shape.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_KEY_MATCHES",
            GATEWAY_API_KEY_MATCHES,
            caixa_core::GATEWAY_API_KEY_MATCHES,
        );
    }

    #[test]
    fn gateway_api_key_path_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_KEY_PATH` was lifted from the
        // inline `"path"` literal at the `gateway_routes` per-
        // Aplicacao HTTPRoute's per-match
        // `match_entry.insert("path", …)` call site (the sole per-
        // production-code per-`HTTPRouteMatch` path-matcher-
        // container-axis emitter) to a re-export of
        // [`caixa_core::GATEWAY_API_KEY_PATH`] so the Gateway-API-CRD
        // per-`HTTPRouteMatch` path-matcher-container-axis-key string
        // lives in exactly one place across every caixa renderer.
        // Pin the equality + static-data identity here so any local
        // re-introduction of a sibling
        // `pub const GATEWAY_API_KEY_PATH: &str = "…"` (the canonical
        // drift footgun where a sibling local `pub const` could
        // happen to carry the same string at the source while pointing
        // at a different `&'static` allocation) is a build-time test
        // failure naming the offending drift, not a silent apply-time
        // symptom — the prior shape would have let a Gateway-API-CRD
        // per-`HTTPRouteMatch` path-matcher-axis rebrand on the
        // caixa-mesh side without a coordinated caixa-core edit
        // silently land per-match path-selection predicates at the
        // drifted axis; the per-match path predicate degrades to the
        // wildcard match, the rule matches every request path
        // unconditionally, and every external `:entrada` path filter
        // drops with no field naming the path-matcher-drift root
        // cause. Peer to
        // [`gateway_api_key_matches_re_export_points_at_caixa_core_canonical`]
        // /
        // [`gateway_api_key_backend_refs_re_export_points_at_caixa_core_canonical`]
        // /
        // [`gateway_api_key_parent_refs_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-Gateway-API-HTTPRoute-body-axis
        // re-export identity-pin set — nests the per-Gateway-API-
        // HTTPRoute-per-rule-body-axis re-export identity-pin set
        // (`matches`, `backendRefs`, `timeouts`, `retry`) one level
        // deeper onto the per-`HTTPRouteMatch` body-axis surface this
        // crate's `gateway_routes` renderer's external `:entrada`
        // ingress contract rests on across the Gateway API HTTPRoute
        // per-match body-shape.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_KEY_PATH",
            GATEWAY_API_KEY_PATH,
            caixa_core::GATEWAY_API_KEY_PATH,
        );
    }

    #[test]
    fn gateway_api_key_listeners_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_KEY_LISTENERS` was lifted from the
        // inline `"listeners"` literal at the `gateway_routes` per-
        // Aplicacao Gateway's `g_spec.insert("listeners", …)` call site
        // (the sole per-production-code per-Gateway L7-listener-set-
        // container-axis emitter) plus the matching test-side fixture
        // site (`gateway_listener_carries_aplicacao_host`'s
        // `.get("listeners")` navigation) to a re-export of
        // [`caixa_core::GATEWAY_API_KEY_LISTENERS`] so the Gateway-API-
        // CRD per-Gateway L7-listener-set-container-axis-key string
        // lives in exactly one place across every caixa renderer. Pin
        // the equality + static-data identity here so any local re-
        // introduction of a sibling
        // `pub const GATEWAY_API_KEY_LISTENERS: &str = "…"` (the
        // canonical drift footgun where a sibling local `pub const`
        // could happen to carry the same string at the source while
        // pointing at a different `&'static` allocation) is a build-
        // time test failure naming the offending drift, not a silent
        // apply-time symptom — the prior shape would have let a
        // Gateway-API-CRD per-Gateway L7-listener-set-axis rebrand on
        // the caixa-mesh side without a coordinated caixa-core edit
        // silently land per-Gateway L7-listener fan-outs at the
        // drifted axis; no listener is opened, and every external
        // `:entrada` flow drops at the gateway-class-controller's per-
        // Gateway reconcile with no field naming the L7-listener-set-
        // drift root cause. Peer to
        // [`gateway_api_key_parent_refs_re_export_points_at_caixa_core_canonical`]
        // /
        // [`gateway_api_key_backend_refs_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-Gateway-API-HTTPRoute-body-axis re-
        // export identity-pin set — extends the per-Gateway-API-CRD-
        // body-axis re-export identity-pin set (`parentRefs`,
        // `backendRefs`, `listeners`, future `hostnames`) this crate's
        // `gateway_routes` renderer's external `:entrada` ingress
        // contract rests on across the Gateway API CRD-side body-
        // shape.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_KEY_LISTENERS",
            GATEWAY_API_KEY_LISTENERS,
            caixa_core::GATEWAY_API_KEY_LISTENERS,
        );
    }

    #[test]
    fn gateway_api_key_hostname_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_KEY_HOSTNAME` was lifted from the
        // inline `"hostname"` literal at the `gateway_routes` per-
        // Aplicacao Gateway's per-listener
        // `listener.insert("hostname", …)` call site (the sole per-
        // production-code per-listener DNS-host-discriminator-axis
        // emitter) plus the matching test-side fixture site
        // (`gateway_listener_carries_aplicacao_host`'s `.get("hostname")`
        // navigation) to a re-export of
        // [`caixa_core::GATEWAY_API_KEY_HOSTNAME`] so the Gateway-API-
        // CRD per-listener DNS-host-discriminator-axis-key string lives
        // in exactly one place across every caixa renderer. Pin the
        // equality + static-data identity here so any local re-
        // introduction of a sibling
        // `pub const GATEWAY_API_KEY_HOSTNAME: &str = "…"` (the
        // canonical drift footgun where a sibling local `pub const`
        // could happen to carry the same string at the source while
        // pointing at a different `&'static` allocation) is a build-
        // time test failure naming the offending drift, not a silent
        // apply-time symptom — the prior shape would have let a
        // Gateway-API-CRD per-listener DNS-host-discriminator-axis
        // rebrand on the caixa-mesh side without a coordinated caixa-
        // core edit silently land per-listener virtual-host filters at
        // the drifted axis; the listener accepts traffic on the
        // wildcard host rather than the typed `:entrada :host`, and
        // every external `:entrada` flow drops at the gateway-class-
        // controller's per-listener dispatch with no field naming the
        // DNS-host-discriminator-drift root cause. Peer to
        // [`gateway_api_key_listeners_re_export_points_at_caixa_core_canonical`]
        // /
        // [`gateway_api_key_parent_refs_re_export_points_at_caixa_core_canonical`]
        // /
        // [`gateway_api_key_backend_refs_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-Gateway-API-CRD-body-axis re-
        // export identity-pin set — nests the per-Gateway-API-CRD-
        // body-axis re-export identity-pin set (`parentRefs`,
        // `backendRefs`, `listeners`, `hostname`, future `hostnames`)
        // one level deeper onto the per-listener body-axis surface this
        // crate's `gateway_routes` renderer's external `:entrada`
        // ingress contract rests on across the Gateway API CRD-side
        // body-shape.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_KEY_HOSTNAME",
            GATEWAY_API_KEY_HOSTNAME,
            caixa_core::GATEWAY_API_KEY_HOSTNAME,
        );
    }

    #[test]
    fn gateway_api_key_hostnames_re_export_points_at_caixa_core_canonical() {
        // The renderer's `GATEWAY_API_KEY_HOSTNAMES` was lifted from the
        // inline `"hostnames"` literal at the `gateway_routes` per-
        // Aplicacao HTTPRoute's spec-level `r_spec.insert("hostnames",
        // …)` call site (the sole production-code per-route DNS-host-
        // filter-axis emitter) to a re-export of
        // [`caixa_core::GATEWAY_API_KEY_HOSTNAMES`] so the Gateway-API-
        // CRD per-route DNS-host-filter-axis-key string lives in exactly
        // one place across every caixa renderer. Pin the equality +
        // static-data identity here so any local re-introduction of a
        // sibling `pub const GATEWAY_API_KEY_HOSTNAMES: &str = "…"` (the
        // canonical drift footgun where a sibling local `pub const` could
        // happen to carry the same string at the source while pointing
        // at a different `&'static` allocation) is a build-time test
        // failure naming the offending drift, not a silent apply-time
        // symptom — the prior shape would have let a Gateway-API-CRD
        // per-route DNS-host-filter-axis rebrand on the caixa-mesh side
        // without a coordinated caixa-core edit silently land per-route
        // virtual-host filters at the drifted axis; the route accepts
        // traffic on every host the parent Gateway's listener accepts
        // rather than the typed `:entrada :host`, and every external
        // `:entrada` flow drops at the gateway-class-controller's per-
        // route dispatch with no field naming the DNS-host-filter-drift
        // root cause. Peer to
        // [`gateway_api_key_hostname_re_export_points_at_caixa_core_canonical`]
        // /
        // [`gateway_api_key_listeners_re_export_points_at_caixa_core_canonical`]
        // /
        // [`gateway_api_key_parent_refs_re_export_points_at_caixa_core_canonical`]
        // /
        // [`gateway_api_key_backend_refs_re_export_points_at_caixa_core_canonical`]
        // on the sibling canonical-Gateway-API-CRD-body-axis re-export
        // identity-pin set — closes the per-Gateway-API-CRD `HTTPRoute`
        // per-route body-axis re-export identity-pin pair across the
        // singular / plural DNS-host discriminator surface (`hostname`
        // at the parent-Gateway per-listener discriminator + `hostnames`
        // at the child HTTPRoute per-route filter list), so both halves
        // of the DNS-host-discriminator convention across the
        // `(Gateway, HTTPRoute)` pair this crate's `gateway_routes`
        // renderer's external `:entrada` ingress contract emits together
        // now carry one lifted `&'static str` re-export identity-pin
        // apiece.
        caixa_core::assert_str_reexport_identity(
            "GATEWAY_API_KEY_HOSTNAMES",
            GATEWAY_API_KEY_HOSTNAMES,
            caixa_core::GATEWAY_API_KEY_HOSTNAMES,
        );
    }

    #[test]
    fn cilium_network_policies_use_lifted_cilium_kind_network_policy() {
        // Fail-before-pass-after pin parsing every rendered
        // `CiliumNetworkPolicy` document and asserting its top-level
        // `kind` axis equals the lifted constant by value. Peer to the
        // canonical-string pin (`cilium_policy_carries_canonical_kube_skeleton`
        // — still present below as the bridge-arm pin asserting the
        // inline canonical string) and the re-export-identity pin
        // (`cilium_kind_network_policy_re_export_points_at_caixa_core_canonical`)
        // — together the three arms (canonical-string pin, lifted-uses
        // pin, re-export-identity pin) close the three-arm drift
        // footgun the inline-literal-pair-across-the-production-
        // skeleton-call-plus-test-fixture shape carried by
        // construction. Peer to
        // [`cilium_network_policies_use_lifted_cilium_api_version`] on
        // the sibling Cilium-CRD-apiVersion-axis lift trajectory —
        // completes the per-Cilium-CRD kind+apiVersion lifted-uses
        // pin pair the renderer's exit threading through the lifted
        // [`CILIUM_API_VERSION`] + [`CILIUM_KIND_NETWORK_POLICY`] pair
        // demands.
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        assert!(
            !policies.is_empty(),
            "the aplicacao fixture must emit at least one CiliumNetworkPolicy \
             — drift here masks the lifted-uses assertion below"
        );
        for p in &policies {
            // Readback through the substrate-primitive [`kube_kind`]
            // pinned accessor — the top-level `kind:` scalar-key axis
            // is pinned at the substrate level so this per-CNP
            // discriminator readback stays aligned with the sibling
            // per-CR `metadata.name` / `metadata.namespace` accessor
            // trio the peer test bodies reach through.
            assert_eq!(
                kube_kind(p),
                Some(CILIUM_KIND_NETWORK_POLICY),
                "every rendered CiliumNetworkPolicy must declare the lifted \
                 [`CILIUM_KIND_NETWORK_POLICY`] constant on its top-level kind \
                 axis — drift here means the per-policy skeleton call no \
                 longer threads the lifted constant through"
            );
        }
    }

    #[test]
    fn cilium_network_policies_use_lifted_cilium_api_version() {
        // Fail-before-pass-after pin parsing every rendered
        // `CiliumNetworkPolicy` document and asserting its top-level
        // `apiVersion` axis equals the lifted constant by value. Peer to
        // the canonical-string pin
        // (`cilium_policy_carries_canonical_kube_skeleton` — still
        // present below as the bridge-arm pin asserting the inline
        // canonical string) and the re-export-identity pin
        // (`cilium_api_version_re_export_points_at_caixa_core_canonical`)
        // — together the three arms (canonical-string pin,
        // lifted-uses pin, re-export-identity pin) close the
        // three-arm drift footgun the inline-literal-pair-across-the-
        // production-skeleton-call-plus-test-fixture shape carried by
        // construction. Peer to
        // [`gateway_routes_gateway_uses_lifted_gateway_api_api_version`]
        // / [`gateway_routes_httproute_uses_lifted_gateway_api_api_version`]
        // on the sibling K8s Gateway API CRD-axis lift trajectory.
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        assert!(
            !policies.is_empty(),
            "the aplicacao fixture must emit at least one CiliumNetworkPolicy \
             — drift here masks the lifted-uses assertion below"
        );
        for p in &policies {
            assert!(
                kube_api_version_is(p, CILIUM_API_VERSION),
                "every rendered CiliumNetworkPolicy must declare the lifted \
                 [`CILIUM_API_VERSION`] constant on its top-level apiVersion \
                 axis — drift here means the per-policy skeleton call no \
                 longer threads the lifted constant through"
            );
        }
    }

    #[test]
    fn programs_for_aplicacao_emits_one_entry_per_member() {
        let entries = programs_for_aplicacao(&aplicacao_caixa()).unwrap();
        assert_eq!(entries.len(), 3);
        let names: Vec<_> = entries
            .iter()
            .map(|e| {
                e.get(FLEET_PROGRAMS_KEY_NAME)
                    .and_then(|n| n.as_str())
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(names, vec!["catalog", "cart", "payment"]);
    }

    #[test]
    fn programs_for_aplicacao_annotates_with_parent_nome() {
        let entries = programs_for_aplicacao(&aplicacao_caixa()).unwrap();
        for e in &entries {
            assert_eq!(
                e.get(FLEET_PROGRAMS_KEY_APLICACAO).and_then(|v| v.as_str()),
                Some("checkout")
            );
        }
    }

    #[test]
    fn fleet_programs_key_aplicacao_pins_canonical_value() {
        // Bridge-arm pin on the emit-side/probe-side coordinate — the
        // [`caixa_core::FLEET_PROGRAMS_KEY_APLICACAO`] constant this
        // crate now consumes at the `entry.insert(…)` per-`:membros`
        // parent-graph-annotation emit site + at the peer readback
        // probe above must resolve to the canonical `"aplicacao"` byte
        // the substrate operator's fleet-aggregator reads to group
        // each `programs[]` entry back onto its parent Aplicacao. A
        // future rebrand on the per-entry parent-graph-annotation
        // axis surfaces here as a coordinated edit-point rather than
        // as a silent apply-time split between the emitter's write
        // and the aggregator's per-graph reduce step. Peer of the
        // in-file [`fleet_programs_key_aplicacao_re_export_static_identity`]
        // static-data identity pin below and of the sibling
        // [`caixa_core::render::tests::fleet_programs_key_aplicacao_pins_canonical_value`]
        // canonical-value pin on the definition-site coordinate.
        assert_eq!(FLEET_PROGRAMS_KEY_APLICACAO, "aplicacao");
    }

    #[test]
    fn fleet_programs_key_aplicacao_re_export_static_identity() {
        // Second coordinate of the re-export pin triangle: the
        // symbol this crate imports as `FLEET_PROGRAMS_KEY_APLICACAO`
        // must be *the same* `&'static str` as the canonical
        // [`caixa_core::FLEET_PROGRAMS_KEY_APLICACAO`] definition (a
        // sibling `pub const` at this crate's use-site would trip the
        // equality above without tripping this identity guard — the
        // exact drift shape the peer `caixa_flux`
        // [`fleet_programs_key_name_re_export_static_identity`] guard
        // catches on the per-entry-name-axis surface). Pinned via
        // `std::ptr::eq` on the two `.as_ptr()` addresses so a future
        // refactor that re-inlines the const here instead of importing
        // it from `caixa_core` fails at the fail-before-deploy posture.
        assert!(
            std::ptr::eq(
                FLEET_PROGRAMS_KEY_APLICACAO.as_ptr(),
                caixa_core::FLEET_PROGRAMS_KEY_APLICACAO.as_ptr(),
            ),
            "FLEET_PROGRAMS_KEY_APLICACAO must resolve to the canonical \
             caixa_core::FLEET_PROGRAMS_KEY_APLICACAO static, not a sibling \
             `pub const` — the aggregator/emitter drift footgun the lift closes."
        );
    }

    #[test]
    fn fleet_programs_key_versao_pins_canonical_value() {
        // Bridge-arm pin on the emit-side coordinate — the
        // [`caixa_core::FLEET_PROGRAMS_KEY_VERSAO`] constant this crate
        // now consumes at the `entry.insert(…)` per-`:membros` version-
        // constraint emit site must resolve to the canonical `"versao"`
        // byte the substrate operator's per-`:membros` resolver reads
        // to project each entry back onto its M3 Aplicacao's declared
        // version-constraint. Peer of the in-file
        // [`fleet_programs_key_versao_re_export_static_identity`]
        // static-data identity pin below and of the sibling
        // [`caixa_core::render::tests::fleet_programs_key_versao_pins_canonical_value`]
        // canonical-value pin on the definition-site coordinate.
        assert_eq!(FLEET_PROGRAMS_KEY_VERSAO, "versao");
    }

    #[test]
    fn fleet_programs_key_versao_re_export_static_identity() {
        // Second coordinate of the re-export pin triangle: the
        // symbol this crate imports as `FLEET_PROGRAMS_KEY_VERSAO`
        // must be *the same* `&'static str` as the canonical
        // [`caixa_core::FLEET_PROGRAMS_KEY_VERSAO`] definition (a
        // sibling `pub const` at this crate's use-site would trip the
        // equality above without tripping this identity guard). Peer
        // of the sibling
        // [`fleet_programs_key_aplicacao_re_export_static_identity`]
        // guard on the per-entry parent-graph-annotation axis surface.
        // Pinned via `std::ptr::eq` on the two `.as_ptr()` addresses
        // so a future refactor that re-inlines the const here instead
        // of importing it from `caixa_core` fails at the fail-before-
        // deploy posture.
        assert!(
            std::ptr::eq(
                FLEET_PROGRAMS_KEY_VERSAO.as_ptr(),
                caixa_core::FLEET_PROGRAMS_KEY_VERSAO.as_ptr(),
            ),
            "FLEET_PROGRAMS_KEY_VERSAO must resolve to the canonical \
             caixa_core::FLEET_PROGRAMS_KEY_VERSAO static, not a sibling \
             `pub const` — the resolver/emitter drift footgun the lift closes."
        );
    }

    #[test]
    fn programs_for_aplicacao_carries_lifted_fleet_programs_key_versao() {
        // Production-emit pin: each `programs[]` entry the per-
        // `:membros` fan-out writes must carry the source Membro's
        // `:versao` constraint under the lifted
        // [`caixa_core::FLEET_PROGRAMS_KEY_VERSAO`] axis-key, in
        // declaration order. Peer of the sibling
        // [`programs_for_aplicacao_annotates_with_parent_nome`]
        // per-`:membros` parent-graph-annotation-axis emit pin;
        // together the two pins pin every per-entry axis the
        // caixa-mesh fan-out writes (name / versao / aplicacao) at the
        // production-emit coordinate.
        let entries = programs_for_aplicacao(&aplicacao_caixa()).unwrap();
        let versoes: Vec<_> = entries
            .iter()
            .map(|e| {
                e.get(FLEET_PROGRAMS_KEY_VERSAO)
                    .and_then(|v| v.as_str())
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(versoes, vec!["^0.1", "^0.1", "^0.2"]);
    }

    #[test]
    fn programs_for_aplicacao_entry_name_routes_through_membro_nome_accessor() {
        // Emit-path pin: each `programs[]` entry's `name:` byte-string
        // must resolve through the typed [`caixa_core::Membro::nome`]
        // accessor, not the raw `.caixa` field. Pins the
        // last-`.clone()`-site lift the `m.nome().to_string()` edit at
        // the per-`:membros` emit call site above landed against a
        // future silent detour that re-inlined `m.caixa.clone()` — the
        // regression would round-trip past the sibling
        // [`programs_for_aplicacao_emits_one_entry_per_member`] fixture-
        // literal pin (which spells the emitted `name:` byte-string
        // verbatim) but would silently split the emit-side write from
        // any future substrate-side rewrite the accessor grows (a
        // per-cluster alias table, an M4 namespace-qualified rewrite,
        // the `:membros :nome-suffix` overlay MESH-COMPOSITION §III.2
        // acknowledges). Asserted against the accessor byte-for-byte,
        // per-membro, in declaration order — a mutation of the emit
        // site back to `.caixa.clone()` still passes this pin *today*
        // (nome() is byte-equal to .caixa on the sibling
        // [`caixa_core::aplicacao::tests::membro_nome_returns_caixa_byte_equal_across_permutations`]
        // pin), but any future accessor extension immediately fires the
        // regression here — the pair `(emit path, accessor path)` moves
        // as a unit on the substrate primitive.
        let c = aplicacao_caixa();
        let membros = &c
            .aplicacao_view()
            .expect("Aplicacao view for fixture")
            .membros;
        let entries = programs_for_aplicacao(&c).unwrap();
        assert_eq!(entries.len(), membros.len());
        for (m, entry) in membros.iter().zip(entries.iter()) {
            let emitted = entry
                .get(FLEET_PROGRAMS_KEY_NAME)
                .and_then(|v| v.as_str())
                .expect("programs.yaml entry carries name: as a string");
            assert_eq!(
                emitted,
                m.nome(),
                "programs.yaml entry `name:` must byte-equal Membro::nome() — \
                 emit path must route through the typed accessor, not the \
                 raw `.caixa` field"
            );
        }
    }

    #[test]
    fn programs_for_aplicacao_entry_versao_routes_through_membro_versao_requirement_accessor() {
        // Emit-path pin: each `programs[]` entry's `versao:` byte-string
        // must resolve through the typed
        // [`caixa_core::Membro::versao_requirement`] accessor, not the
        // raw `.versao` field. Peer of the sibling
        // [`programs_for_aplicacao_entry_name_routes_through_membro_nome_accessor`]
        // pin on the per-`:membros` version-constraint axis; together
        // the two pins pin the last two `.clone()` sites the a40b0e3 /
        // 4a32abf sibling per-`:membros` accessor lifts left carrying
        // raw-field-access `String`-carry copies (the sibling `&str`-
        // read sites already route through the accessors). A future
        // extension of the version-constraint accessor (a per-cluster
        // version-pin overlay the operator pins through a future
        // `:placement`-scoped slot, a lacre-projected concrete-version
        // rewrite, an M4 canary version-pinning slot) that lands on the
        // accessor now flows through the emitted programs.yaml `versao:`
        // by construction.
        let c = aplicacao_caixa();
        let membros = &c
            .aplicacao_view()
            .expect("Aplicacao view for fixture")
            .membros;
        let entries = programs_for_aplicacao(&c).unwrap();
        assert_eq!(entries.len(), membros.len());
        for (m, entry) in membros.iter().zip(entries.iter()) {
            let emitted = entry
                .get(FLEET_PROGRAMS_KEY_VERSAO)
                .and_then(|v| v.as_str())
                .expect("programs.yaml entry carries versao: as a string");
            assert_eq!(
                emitted,
                m.versao_requirement(),
                "programs.yaml entry `versao:` must byte-equal \
                 Membro::versao_requirement() — emit path must route \
                 through the typed accessor, not the raw `.versao` field"
            );
        }
    }

    #[test]
    fn programs_for_aplicacao_entry_aplicacao_routes_through_caixa_nome_accessor() {
        // Emit-path pin: each `programs[]` entry's `aplicacao:`
        // byte-string must resolve through the typed
        // [`caixa_core::Caixa::nome`] accessor, not the raw `.nome`
        // field. Pins the `caixa.nome().to_string()` edit at the
        // per-`:membros` emit call site above against a future silent
        // detour that re-inlined `caixa.nome.clone()` — the regression
        // would round-trip past every fixture-literal aplicacao-name
        // pin (which spells the emitted `aplicacao:` byte-string
        // verbatim as `"checkout"`) but would silently split the emit-
        // side write from any future substrate-side rewrite the
        // accessor grows (a per-cluster alias table the operator pins
        // through a future `:placement`-scoped slot, an M4 namespace-
        // qualified rewrite the CR materializer applies per-CR, the
        // future `:nome-suffix` overlay the MESH-COMPOSITION §III.2
        // roadmap acknowledges). Asserted against the accessor byte-
        // for-byte, per-entry — a mutation of the emit site back to
        // `.nome.clone()` still passes this pin *today* (nome() is
        // byte-equal to .nome on the sibling
        // [`caixa_core::manifest::tests`] accessor pins), but any
        // future accessor extension immediately fires the regression
        // here — the pair `(emit path, accessor path)` moves as a unit
        // on the substrate primitive. Peer of the sibling
        // [`programs_for_aplicacao_entry_name_routes_through_membro_nome_accessor`]
        // pin on the per-`:membros` `name:` axis (4127bb6) extended
        // onto the parent-`Aplicacao` `aplicacao:` annotation axis;
        // opens the "converge every remaining `caixa.nome.clone()`
        // raw-field-access `String`-carry site in caixa-mesh onto
        // Caixa::nome" sweep the sibling
        // [`cilium_network_policies_label_aplicacao_routes_through_caixa_nome_accessor`]
        // and
        // [`gateway_routes_parent_ref_name_routes_through_caixa_nome_accessor`]
        // pins fold on.
        let c = aplicacao_caixa();
        let entries = programs_for_aplicacao(&c).unwrap();
        assert!(!entries.is_empty());
        for entry in &entries {
            let emitted = entry
                .get(FLEET_PROGRAMS_KEY_APLICACAO)
                .and_then(|v| v.as_str())
                .expect("programs.yaml entry carries aplicacao: as a string");
            assert_eq!(
                emitted,
                c.nome(),
                "programs.yaml entry `aplicacao:` must byte-equal \
                 Caixa::nome() — emit path must route through the typed \
                 accessor, not the raw `.nome` field"
            );
        }
    }

    #[test]
    fn cilium_network_policies_label_aplicacao_routes_through_caixa_nome_accessor() {
        // Emit-path pin: each CNP's
        // `metadata.labels.pleme.pleme.io/aplicacao` byte-string must
        // resolve through the typed [`caixa_core::Caixa::nome`]
        // accessor, not the raw `.nome` field. Sibling of the peer
        // [`programs_for_aplicacao_entry_aplicacao_routes_through_caixa_nome_accessor`]
        // pin on the fleet-programs `aplicacao:` annotation axis,
        // extended onto the per-`(:de, :para)` CNP `LABEL_APLICACAO`
        // label-axis — same "the emit path must route through the
        // substrate-primitive typed dispatch" discipline extended onto
        // the peer per-CNP `String`-carry site. A future extension of
        // the accessor to a richer author surface (a per-cluster alias
        // table, an M4 namespace-qualified rewrite, the future
        // `:nome-suffix` overlay MESH-COMPOSITION §III.2 acknowledges)
        // that landed on the accessor but not on this label would have
        // silently split the parent-Aplicacao identity between two
        // consumers — the operator's `kubectl -n tatara-system get cnp
        // -l pleme.pleme.io/aplicacao=<name>` grep-by-label would land
        // on a policy whose parent-Aplicacao annotation drifted from
        // the accessor's projection.
        let c = aplicacao_caixa();
        let policies = cilium_network_policies(&c).unwrap();
        assert!(!policies.is_empty());
        for policy in &policies {
            let emitted = kube_metadata_label(policy, LABEL_APLICACAO)
                .expect("CNP metadata.labels carries LABEL_APLICACAO as a string");
            assert_eq!(
                emitted,
                c.nome(),
                "CNP `metadata.labels.{LABEL_APLICACAO}` must byte-equal \
                 Caixa::nome() — emit path must route through the typed \
                 accessor, not the raw `.nome` field"
            );
        }
    }

    #[test]
    fn gateway_routes_parent_ref_name_routes_through_caixa_nome_accessor() {
        // Emit-path pin: the HTTPRoute's
        // `spec.parentRefs[0].name` byte-string must resolve through
        // the typed [`caixa_core::Caixa::nome`] accessor, not the raw
        // `.nome` field. Third and final sibling of the peer
        // [`programs_for_aplicacao_entry_aplicacao_routes_through_caixa_nome_accessor`]
        // + [`cilium_network_policies_label_aplicacao_routes_through_caixa_nome_accessor`]
        // pins — closes the last unlifted `caixa.nome.clone()` raw-
        // field-access `String`-carry site in caixa-mesh. The
        // parentRefs[].name binds the emitted HTTPRoute to the peer
        // Gateway whose `metadata.name` is derived from the same
        // Caixa::nome earlier in this same emitter (via
        // `kube_resource_skeleton(..., &caixa.nome, ...)` at the
        // Gateway skeleton call above — a peer `&str`-read site out of
        // scope for this `String`-carry sweep); a future accessor
        // extension that split those two projections would orphan the
        // route from its parent Gateway at every apply-time Gateway
        // API v1.x per-parentRef resolution step. Pinning the emit
        // path against the accessor byte-for-byte here fires that
        // regression at caixa-mesh build time rather than at K8s API
        // server admission.
        let c = aplicacao_caixa();
        let routes = gateway_routes(&c).unwrap();
        let route = routes
            .iter()
            .find(|r| {
                r.get(KUBE_KEY_KIND)
                    .and_then(|k| k.as_str())
                    .is_some_and(|k| k == GATEWAY_API_KIND_HTTP_ROUTE)
            })
            .expect("gateway_routes emits at least one HTTPRoute for the fixture Aplicacao");
        let emitted = kube_spec_field(route, GATEWAY_API_KEY_PARENT_REFS)
            .and_then(|p| p.as_sequence())
            .and_then(|s| s.first())
            .and_then(|p| p.get(GATEWAY_API_KEY_NAME))
            .and_then(|v| v.as_str())
            .expect("HTTPRoute spec.parentRefs[0].name is a string");
        assert_eq!(
            emitted,
            c.nome(),
            "HTTPRoute `spec.parentRefs[0].{GATEWAY_API_KEY_NAME}` must \
             byte-equal Caixa::nome() — emit path must route through the \
             typed accessor, not the raw `.nome` field"
        );
    }

    #[test]
    fn cilium_network_policy_metadata_name_routes_through_caixa_nome_accessor() {
        // Emit-path pin: each CNP's `metadata.name` byte-string must
        // derive from the typed [`caixa_core::Caixa::nome`] accessor
        // byte-for-byte through the substrate-canonical
        // [`caixa_core::cilium_network_policy_name`] composer. Before
        // this converge the outer
        // `cilium_network_policy_name(&caixa.nome, de, para)` call at
        // [`cilium_network_policies`] carried a raw `&caixa.nome`
        // `&String`-borrow of the underlying field, bypassing the typed
        // accessor. Peer of the sibling
        // [`cilium_network_policies_label_aplicacao_routes_through_caixa_nome_accessor`]
        // pin on the co-resident per-CNP `metadata.labels
        // .pleme.pleme.io/aplicacao` `String`-carry site — extends the
        // "one typed dispatch on the substrate primitive, thin
        // projections at each consumer" discipline onto the
        // non-`.clone()` raw-field-access axis of `Caixa::nome` in
        // caixa-mesh (sibling of the 22461ef caixa-helm converge on the
        // per-`lareira-<nome>` chart-directory identity composer).
        // Byte-equal today (the accessor is `&self.nome`); the pin
        // catches any future accessor extension (a per-cluster alias
        // overlay, an M4 CR-materializer name rewrite, a future
        // `:nome-suffix` slot) whose emit-side write regresses to the
        // raw `&caixa.nome` field access.
        let c = aplicacao_caixa();
        let policies = cilium_network_policies(&c).unwrap();
        assert!(!policies.is_empty());
        for policy in &policies {
            let emitted = kube_name(policy).expect("CNP metadata.name scalar present");
            // Extract the (de, para) pair back out of
            // `<aplicacao>-<de>-to-<para>` by stripping the accessor-
            // canonical `<aplicacao>-` prefix and splitting on the
            // canonical `-to-` separator — pins the composer's byte
            // shape (aplicacao-name arg first, `-to-` separator,
            // destination-name arg last) against the emitted encoding.
            let stripped = emitted
                .strip_prefix(&format!("{}-", c.nome()))
                .expect("CNP metadata.name carries the accessor-derived aplicacao prefix");
            let (de, para) = stripped
                .split_once(CONTRATO_EDGE_LABEL_SEPARATOR)
                .expect("CNP metadata.name carries the canonical `-to-` edge separator");
            assert_eq!(
                emitted,
                cilium_network_policy_name(c.nome(), de, para),
                "CNP `metadata.name` must derive from the typed \
                 `caixa_core::Caixa::nome` accessor through \
                 `caixa_core::cilium_network_policy_name` byte-for-byte \
                 — a regression that re-inlines \
                 `cilium_network_policy_name(&caixa.nome, de, para)` at \
                 the emit site silently splits the per-CNP \
                 `metadata.name` axis (the operator-side `kubectl -n \
                 tatara-system get cnp <aplicacao>-<de>-to-<para>` \
                 grep-by-name lookup key) from every future accessor \
                 extension that lands on the accessor"
            );
        }
    }

    #[test]
    fn cilium_network_policy_from_endpoints_aplicacao_scope_routes_through_caixa_nome_accessor() {
        // Emit-path pin: each CNP's `spec.ingress[0].fromEndpoints[0]
        // .matchLabels.pleme.pleme.io/aplicacao` byte-string (the
        // aplicacao-scope axis of the two-axis source selector
        // [`caixa_core::pleme_program_in_aplicacao_selector`] emits)
        // must resolve through the typed [`caixa_core::Caixa::nome`]
        // accessor. Before this converge the outer
        // `pleme_program_in_aplicacao_selector(de, &caixa.nome)` call
        // at [`cilium_network_policies`] carried a raw `&caixa.nome`
        // `&String`-borrow of the underlying field, bypassing the typed
        // accessor. Load-bearing safety property: a same-named program
        // in a *different* Aplicacao cannot satisfy the CNP's ingress
        // rule — if the emit-side aplicacao-scope value ever drifted
        // from the accessor-canonical projection of `Caixa::nome`, a
        // future accessor extension (per-cluster alias overlay, M4 CR-
        // materializer name rewrite, `:nome-suffix` slot) that rewrote
        // the parent-Aplicacao identity on the accessor but not on this
        // selector site would silently break the aplicacao-scoping
        // guarantee at every Cilium data-plane admission decision.
        let c = aplicacao_caixa();
        let policies = cilium_network_policies(&c).unwrap();
        assert!(!policies.is_empty());
        for policy in &policies {
            let selector = kube_spec_field(policy, CILIUM_KEY_INGRESS)
                .and_then(|i| i.as_sequence())
                .and_then(|s| s.first())
                .and_then(|i| i.get(CILIUM_KEY_FROM_ENDPOINTS))
                .and_then(|e| e.as_sequence())
                .and_then(|s| s.first())
                .and_then(|e| e.get(KUBE_KEY_MATCH_LABELS))
                .and_then(|m| m.as_mapping())
                .expect("CNP spec.ingress[0].fromEndpoints[0].matchLabels mapping present");
            let emitted = selector
                .get(LABEL_APLICACAO)
                .and_then(|v| v.as_str())
                .expect("fromEndpoints selector carries LABEL_APLICACAO as a string");
            assert_eq!(
                emitted,
                c.nome(),
                "CNP `spec.ingress[0].fromEndpoints[0].matchLabels.{LABEL_APLICACAO}` \
                 must byte-equal Caixa::nome() — emit path must route \
                 through the typed accessor, not the raw `.nome` field, \
                 so a future accessor extension that rewrites the \
                 parent-Aplicacao identity reaches this aplicacao-scope \
                 axis by construction and preserves the load-bearing \
                 safety property that a same-named program in a \
                 different Aplicacao cannot satisfy the ingress rule"
            );
        }
    }

    #[test]
    fn gateway_routes_gateway_metadata_name_routes_through_caixa_nome_accessor() {
        // Emit-path pin: the Gateway's `metadata.name` byte-string must
        // resolve through the typed [`caixa_core::Caixa::nome`]
        // accessor byte-for-byte. Before this converge the outer
        // `kube_resource_skeleton(..., &caixa.nome, ...)` call at
        // [`gateway_routes`]'s Gateway skeleton site carried a raw
        // `&caixa.nome` `&String`-borrow of the underlying field,
        // bypassing the typed accessor. Companion to the sibling
        // [`gateway_routes_parent_ref_name_routes_through_caixa_nome_accessor`]
        // pin on the HTTPRoute `spec.parentRefs[0].name` axis: the pair
        // `(Gateway metadata.name, HTTPRoute spec.parentRefs[0].name)`
        // — the two halves of the Gateway API v1 parent-binding contract
        // Envoy's per-listener attachment resolver keys off — must
        // share exactly one typed dispatch on the substrate primitive.
        // A future accessor extension that rewrote the parent-
        // Aplicacao identity on the accessor but not on this skeleton-
        // name site would orphan every emitted HTTPRoute from its
        // parent Gateway at K8s API-server admission time.
        let c = aplicacao_caixa();
        let docs = gateway_routes(&c).unwrap();
        let gateway = find_by_kind(&docs, GATEWAY_API_KIND_GATEWAY)
            .expect("Gateway present under a `:entrada`-carrying fixture Aplicacao");
        let emitted = kube_name(gateway).expect("Gateway metadata.name scalar present");
        assert_eq!(
            emitted,
            c.nome(),
            "Gateway `metadata.name` must derive from the typed \
             `caixa_core::Caixa::nome` accessor byte-for-byte — a \
             regression that re-inlines \
             `kube_resource_skeleton(..., &caixa.nome, ...)` at the emit \
             site silently splits the Gateway `metadata.name` axis (the \
             operator-side `kubectl -n tatara-system get gateway \
             <aplicacao>` grep-by-name lookup key and the peer sibling \
             HTTPRoute `spec.parentRefs[0].name` binding) from every \
             future accessor extension that lands on the accessor"
        );
    }

    #[test]
    fn gateway_routes_httproute_metadata_name_routes_through_caixa_nome_accessor() {
        // Emit-path pin: the HTTPRoute's `metadata.name` byte-string
        // must derive from the typed [`caixa_core::Caixa::nome`]
        // accessor byte-for-byte through the substrate-canonical
        // [`caixa_core::gateway_api_http_route_name`] composer. Before
        // this converge the outer
        // `gateway_api_http_route_name(&caixa.nome, entrada.destination())`
        // call at [`gateway_routes`]'s HTTPRoute skeleton site carried
        // a raw `&caixa.nome` `&String`-borrow of the underlying field,
        // bypassing the typed accessor. Peer of the sibling per-CNP
        // `metadata.name` composer converge above and the co-resident
        // Gateway `metadata.name` skeleton-arg converge — closes the
        // last unlifted non-`.clone()` raw-field-access `Caixa::nome`
        // site in caixa-mesh. A future accessor extension that
        // rewrote the parent-Aplicacao identity on the accessor but
        // not on this HTTPRoute-name site would silently split the
        // per-HTTPRoute `metadata.name` axis (the operator-side
        // `kubectl -n tatara-system get httproute
        // <aplicacao>-<destination>` grep-by-name lookup key) from
        // every future accessor extension.
        let c = aplicacao_caixa();
        let docs = gateway_routes(&c).unwrap();
        let route = find_by_kind(&docs, GATEWAY_API_KIND_HTTP_ROUTE)
            .expect("HTTPRoute present under a `:entrada`-carrying fixture Aplicacao");
        let emitted = kube_name(route).expect("HTTPRoute metadata.name scalar present");
        let entrada = c
            .entrada()
            .expect("aplicacao_caixa carries a typed `:entrada` block");
        assert_eq!(
            emitted,
            gateway_api_http_route_name(c.nome(), entrada.destination()),
            "HTTPRoute `metadata.name` must derive from the typed \
             `caixa_core::Caixa::nome` accessor through \
             `caixa_core::gateway_api_http_route_name` byte-for-byte — \
             a regression that re-inlines \
             `gateway_api_http_route_name(&caixa.nome, \
             entrada.destination())` at the emit site silently splits \
             the per-HTTPRoute `metadata.name` axis (the operator-side \
             `kubectl -n tatara-system get httproute \
             <aplicacao>-<destination>` grep-by-name lookup key) from \
             every future accessor extension that lands on the accessor"
        );
    }

    #[test]
    fn programs_for_aplicacao_rejects_non_aplicacao_kinds() {
        let mut c = aplicacao_caixa();
        c.kind = CaixaKind::Servico;
        c.servicos = vec!["servicos/x.computeunit.yaml".into()];
        let err = programs_for_aplicacao(&c).unwrap_err();
        assert!(matches!(err, Error::NotAnAplicacao(_)));
    }

    #[test]
    fn kind_mismatch_error_names_offending_caixa_nome() {
        // Pinning the lifted [`caixa_core::KindMismatch`] view's
        // load-bearing property: a kind-mismatched caixa surfaces a
        // diagnostic that *names the offending caixa* (`checkout`),
        // not just the rejected kind. Before the lift the renderer
        // raised `Error::NotAnAplicacao(CaixaKind::Servico)` whose
        // Display said "caixa :kind must be Aplicacao for caixa-mesh
        // rendering, got Servico" — the user had to grep their
        // source tree for which caixa.lisp triggered it. After the
        // lift the wrapped KindMismatch carries the `:nome`, the
        // renderer's `#[error("{0}")]` arm prints it through, and
        // the diagnostic is self-locating.
        let mut c = aplicacao_caixa();
        c.kind = CaixaKind::Servico;
        c.servicos = vec!["servicos/x.computeunit.yaml".into()];
        let err = programs_for_aplicacao(&c).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("checkout"),
            "kind-mismatch diagnostic must name the offending caixa nome \
             (got: {msg:?})"
        );
        assert!(
            msg.contains("Aplicacao"),
            "diagnostic must name the expected kind (got: {msg:?})"
        );
        assert!(
            msg.contains("Servico"),
            "diagnostic must name the actual kind (got: {msg:?})"
        );
    }

    #[test]
    fn typed_view_kind_mismatch_names_offending_caixa_nome() {
        // The second kind-checking call site in caixa-mesh —
        // [`typed_view`] (consumed by every downstream renderer:
        // `cilium_network_policies`, `gateway_routes`, the future
        // per-:politicas `CiliumClusterwideEnvoyConfig` emitter) —
        // must surface the same lifted diagnostic shape. Pinning so
        // a future divergence between `programs_for_aplicacao` and
        // `typed_view` (e.g. one re-inlines the kind check, the other
        // uses `require_kind`) surfaces here as a test failure rather
        // than as a silent diagnostic regression on the per-Aplicacao
        // mesh-emission path.
        let mut c = aplicacao_caixa();
        c.kind = CaixaKind::Supervisor;
        c.servicos = vec![];
        c.children = vec![];
        let err = typed_view(&c).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("checkout"),
            "typed_view's kind-mismatch must also name the caixa nome \
             (got: {msg:?})"
        );
        match err {
            Error::NotAnAplicacao(km) => {
                assert_eq!(km.nome, "checkout");
                assert_eq!(km.expected, CaixaKind::Aplicacao);
                assert_eq!(km.actual, CaixaKind::Supervisor);
            }
            other => panic!("expected Error::NotAnAplicacao, got {other:?}"),
        }
    }

    #[test]
    fn programs_for_aplicacao_validates_typed_shape() {
        let mut c = aplicacao_caixa();
        // Add an invalid contrato pointing at a non-member.
        c.contratos.push(WitContract {
            de: "cart".into(),
            para: "phantom".into(),
            wit: "wasi:http/proxy".into(),
            endpoint: Some("/x".into()),
            subject: None,
            slot: None,
        });
        let err = programs_for_aplicacao(&c).unwrap_err();
        assert!(matches!(err, Error::InvalidAplicacao(_)));
    }

    #[test]
    fn programs_for_aplicacao_routes_entry_gate_through_typed_view() {
        // Drift-detection pin on the lifted per-Aplicacao entry-gate
        // cascade: `programs_for_aplicacao` and its sibling
        // per-Aplicacao renderers (`cilium_network_policies`,
        // `gateway_routes`) all funnel through [`typed_view`]'s
        // `require_kind + aplicacao_view + AplicacaoSpec::validate`
        // three-arm gate before touching any renderer-specific
        // emission. Feeding the same offending caixa into both paths
        // must therefore surface byte-identical `Error` diagnostics —
        // the same variant, the same self-locating fields, the same
        // `Display` prose.
        //
        // Until `programs_for_aplicacao` was refactored onto
        // `typed_view` it re-inlined the three-arm cascade by hand.
        // Both paths happened to agree today only because the
        // hand-written scaffold was the same three lines, but a
        // future entry-gate widening on one side without a matching
        // edit on the other would have surfaced only at the sibling
        // renderer that took the drifted path — silently on the one
        // that stayed on the pre-widening cascade. Pinning both
        // paths' error surface on the same input structurally
        // eliminates that drift: a future entry-gate change threads
        // through both call sites together, or this test fires.
        //
        // Peer to the sibling `typed_view_kind_mismatch_names_
        // offending_caixa_nome` test on the kind-check arm; this test
        // covers the `AplicacaoSpec::validate` arm (via a
        // non-member `:contratos :para` reference — the same fixture
        // the pre-existing `programs_for_aplicacao_validates_typed_
        // shape` test uses).
        let mut c = aplicacao_caixa();
        c.contratos.push(WitContract {
            de: "cart".into(),
            para: "phantom".into(),
            wit: "wasi:http/proxy".into(),
            endpoint: Some("/x".into()),
            subject: None,
            slot: None,
        });
        let programs_err = programs_for_aplicacao(&c).unwrap_err();
        let typed_view_err = typed_view(&c).unwrap_err();
        assert_eq!(
            format!("{programs_err}"),
            format!("{typed_view_err}"),
            "programs_for_aplicacao must surface the same entry-gate \
             diagnostic as typed_view — a divergence here means the \
             renderer skipped the shared cascade"
        );
        assert!(
            matches!(programs_err, Error::InvalidAplicacao(_)),
            "programs_for_aplicacao must surface the AplicacaoSpec::validate \
             failure through the same Error::InvalidAplicacao variant \
             typed_view raises"
        );
        assert!(
            matches!(typed_view_err, Error::InvalidAplicacao(_)),
            "typed_view must raise the same variant so a future divergence \
             on either path is a compile-time signal, not a silent \
             renderer-side drift"
        );
    }

    #[test]
    fn programs_for_aplicacao_kind_mismatch_matches_typed_view() {
        // Companion of the validate-arm drift-detection pin
        // immediately above: on the kind-check arm (Supervisor caixa
        // fed into a per-Aplicacao renderer), both `typed_view` and
        // `programs_for_aplicacao` must surface byte-identical
        // diagnostics — the same lifted [`caixa_core::KindMismatch`]
        // view wrapped in the same `Error::NotAnAplicacao` variant.
        // Pinning both arms of the shared entry-gate cascade closes
        // the drift surface structurally; a future edit that widens
        // the kind-check on one path without the other would have
        // silently regressed on the sibling renderer.
        let mut c = aplicacao_caixa();
        c.kind = CaixaKind::Supervisor;
        c.servicos = vec![];
        c.children = vec![];
        let programs_err = programs_for_aplicacao(&c).unwrap_err();
        let typed_view_err = typed_view(&c).unwrap_err();
        assert_eq!(
            format!("{programs_err}"),
            format!("{typed_view_err}"),
            "programs_for_aplicacao and typed_view must agree on the \
             kind-mismatch diagnostic — divergence indicates one path \
             skipped the shared `require_kind` gate"
        );
    }

    #[test]
    fn typed_view_routes_through_caixa_core_require_aplicacao_view_helper() {
        // Fail-before-pass-after pin on the [`typed_view`] delegation
        // to the lifted [`caixa_core::require_aplicacao_view`]
        // primitive: pre-lift the wrapper carried the three-line
        // `require_kind + aplicacao_view + AplicacaoSpec::validate`
        // cascade inline at this crate's own [`typed_view`] body with
        // no compile-time link to any substrate-canonical compound
        // entry-gate the sibling per-Servico renderers already route
        // through ([`caixa_core::require_v0_servico_shape`]).
        // Converging the wrapper on the substrate-canonical
        // [`caixa_core::require_aplicacao_view`] compound gate closes
        // the drift potential structurally: every future per-Aplicacao
        // consumer (the deferred `mesh.pleme.io/v1alpha1/Aplicacao`
        // CR materializer's admission webhook the M4 roadmap names,
        // `caixa-tatara`'s spec-consuming validate arm when it grows
        // beyond the `require_kind`-only entry gate it carries today
        // at caixa-tatara/src/lib.rs:203, a future `feira validate
        // --aplicacao` per-caixa admission verb) reaches for one
        // `caixa_core::require_aplicacao_view::<Error>(caixa)?`
        // one-liner and gets the compound three-arm gate for free —
        // matching the peer [`caixa_core::require_v0_servico_shape`]
        // discipline every per-Servico renderer already routes
        // through.
        //
        // Byte-for-byte parity assertion: [`typed_view`]'s
        // Ok/Err discrimination on every fixture must equal
        // [`caixa_core::require_aplicacao_view`]'s discrimination on
        // the same fixture (Ok arm — same serialized `AplicacaoSpec`;
        // Err arm — same Display bytes). Trips at the caller's build
        // time, not silently at the diagnostic-emission site. Peer to
        // the sibling `programs_for_aplicacao_routes_entry_gate_
        // through_typed_view` drift-detection pin already in place on
        // this crate's per-renderer entry-gate axis.
        let ok_cases: Vec<Caixa> = vec![aplicacao_caixa()];
        for c in ok_cases {
            let via_wrapper = typed_view(&c).expect("valid aplicacao passes typed_view");
            let via_primitive = caixa_core::require_aplicacao_view::<Error>(&c)
                .expect("valid aplicacao passes require_aplicacao_view");
            assert_eq!(
                serde_yaml::to_string(&via_wrapper).expect("typed_view spec serializes"),
                serde_yaml::to_string(&via_primitive)
                    .expect("require_aplicacao_view spec serializes"),
                "typed_view's Ok-arm AplicacaoSpec must equal \
                 caixa_core::require_aplicacao_view's Ok-arm AplicacaoSpec \
                 byte-for-byte on the same fixture — otherwise typed_view \
                 has drifted from the substrate primitive"
            );
        }

        // Kind-mismatch axis: mis-kinded input surfaces the same
        // [`KindMismatch`]-carrying diagnostic through both paths.
        let mut c = aplicacao_caixa();
        c.kind = CaixaKind::Supervisor;
        c.servicos = vec![];
        c.children = vec![];
        let wrapper_err = typed_view(&c).unwrap_err();
        let primitive_err = caixa_core::require_aplicacao_view::<Error>(&c).unwrap_err();
        assert_eq!(
            format!("{wrapper_err}"),
            format!("{primitive_err}"),
            "typed_view's kind-mismatch Display bytes must equal \
             caixa_core::require_aplicacao_view's kind-mismatch Display \
             bytes — a future format edit lands in exactly one place \
             (caixa-core::render), not duplicated across every \
             per-Aplicacao renderer"
        );
        assert!(
            matches!(wrapper_err, Error::NotAnAplicacao(_)),
            "typed_view must forward the KindMismatch through the \
             Error::NotAnAplicacao #[from] arm"
        );
        assert!(
            matches!(primitive_err, Error::NotAnAplicacao(_)),
            "caixa_core::require_aplicacao_view must forward the \
             KindMismatch through the Error::NotAnAplicacao #[from] arm \
             — same discipline as the sibling require_v0_servico_shape \
             `E: From<KindMismatch>` bound"
        );

        // Invalid-aplicacao axis: a valid-kind but spec-invalid caixa
        // (a `:contratos` entry referencing a non-member) surfaces
        // the same [`caixa_core::AplicacaoError`]-carrying diagnostic
        // through both paths — the peer `programs_for_aplicacao_
        // routes_entry_gate_through_typed_view` pin already covers
        // this axis on the outer renderer surface; extending it onto
        // the wrapper-vs-primitive surface here closes the drift
        // potential at both altitudes.
        let mut c = aplicacao_caixa();
        c.contratos.push(WitContract {
            de: "cart".into(),
            para: "phantom".into(),
            wit: "wasi:http/proxy".into(),
            endpoint: Some("/x".into()),
            subject: None,
            slot: None,
        });
        let wrapper_err = typed_view(&c).unwrap_err();
        let primitive_err = caixa_core::require_aplicacao_view::<Error>(&c).unwrap_err();
        assert_eq!(
            format!("{wrapper_err}"),
            format!("{primitive_err}"),
            "typed_view's invalid-aplicacao Display bytes must equal \
             caixa_core::require_aplicacao_view's invalid-aplicacao \
             Display bytes on the same non-member `:contratos :para` \
             fixture"
        );
        assert!(
            matches!(wrapper_err, Error::InvalidAplicacao(_)),
            "typed_view must forward the AplicacaoError through the \
             Error::InvalidAplicacao #[from] arm"
        );
        assert!(
            matches!(primitive_err, Error::InvalidAplicacao(_)),
            "caixa_core::require_aplicacao_view must forward the \
             AplicacaoError through the Error::InvalidAplicacao \
             #[from] arm"
        );
    }

    // ── programs.yaml :placement overlay ─────────────────────────────────

    fn placement_blocks(entries: &[serde_yaml::Value]) -> Vec<&serde_yaml::Mapping> {
        entries
            .iter()
            .map(|e| {
                e.get(M3_KEY_PLACEMENT)
                    .and_then(|p| p.as_mapping())
                    .expect("every member entry must carry a placement mapping")
            })
            .collect()
    }

    #[test]
    fn programs_entry_carries_placement_block() {
        // The fixture sets `:placement :estrategia Replicated
        // :clusters ("rio" "mar") :affinity "data-locality"`. Every
        // emitted programs.yaml entry must carry a `placement:` block
        // wiring the typed `:placement` slot through to the rendered
        // artifact under the canonical [`M3_KEY_PLACEMENT`] key. Before
        // this overlay landed the typed slot was inert past
        // `AplicacaoSpec::validate_placement` — the rendered entry
        // carried only `name + versao + aplicacao`, so the
        // lareira-fleet-programs aggregator and the future
        // `app-operator` had no way to scope each entry by its parent
        // Aplicacao's distribution strategy. This test is the pinned
        // proof that the slot now reaches the cluster artifact (the
        // fail-before-pass-after pin: the assertion below fails on any
        // pre-overlay codebase, since the entry had no `placement:`
        // key at all).
        let entries = programs_for_aplicacao(&aplicacao_caixa()).unwrap();
        assert!(!entries.is_empty());
        for e in &entries {
            assert!(
                e.get(M3_KEY_PLACEMENT).is_some(),
                "every member entry must carry a `placement:` block"
            );
        }
    }

    #[test]
    fn programs_entry_placement_carries_strategy() {
        // Pin that the `placement.estrategia` axis round-trips the
        // typed [`PlacementStrategy`] enum verbatim — the fixture sets
        // `Replicated`, the serde Serialize impl emits the variant name
        // exactly. A future refactor that adds a `#[serde(rename_all =
        // …)]` attribute on the enum (e.g. shifting to lowercase to
        // match the lisp authoring spelling) is an intentional break
        // this test surfaces — coordinated with the consumer-side
        // (lareira-fleet-programs aggregator's strategy dispatcher,
        // future `app-operator` reconciler) to keep the contract
        // round-tripping end-to-end.
        let entries = programs_for_aplicacao(&aplicacao_caixa()).unwrap();
        for p in placement_blocks(&entries) {
            assert_eq!(
                p.get(M3_PLACEMENT_KEY_ESTRATEGIA).and_then(|v| v.as_str()),
                Some(M3_PLACEMENT_ESTRATEGIA_REPLICATED),
                "placement.estrategia must round-trip the typed PlacementStrategy variant"
            );
        }
    }

    #[test]
    fn programs_entry_placement_carries_clusters_list() {
        // Pin that the `placement.clusters` list round-trips the
        // validated cluster-pool list (non-empty + duplicate-free per
        // [`AplicacaoSpec::validate_placement`]) verbatim. The
        // downstream aggregator's per-cluster filter
        // (`programs.filter(|p| p.placement.clusters.contains(<self>))`)
        // depends on this round-tripping bit-for-bit — drift here
        // silently drops workloads from clusters that should run them.
        let entries = programs_for_aplicacao(&aplicacao_caixa()).unwrap();
        for p in placement_blocks(&entries) {
            let clusters = p
                .get(M3_PLACEMENT_KEY_CLUSTERS)
                .and_then(|c| c.as_sequence())
                .expect("placement.clusters sequence");
            let names: Vec<&str> = clusters.iter().filter_map(|v| v.as_str()).collect();
            assert_eq!(names, vec!["rio", "mar"]);
        }
    }

    #[test]
    fn programs_entry_placement_carries_affinity_when_set() {
        // The fixture's `:affinity "data-locality"` (Some) round-trips
        // through. Pin both the key spelling and the value to guard
        // against future rename / placement-engine semantic drift —
        // the `affinity:` value flows into the M3 Adaptive compression
        // weighting (MESH-COMPOSITION §V) and the wasm-operator's pod
        // affinity overlay.
        let entries = programs_for_aplicacao(&aplicacao_caixa()).unwrap();
        for p in placement_blocks(&entries) {
            assert_eq!(
                p.get(M3_PLACEMENT_KEY_AFFINITY).and_then(|v| v.as_str()),
                Some("data-locality")
            );
        }
    }

    #[test]
    fn programs_entry_placement_omits_affinity_and_shard_key_when_unset() {
        // Empty-axis-skip semantic (mirrors the `:politicas`
        // `:timeout`/`:retries`/`:mtls-required` overlays' omit-when-
        // unset contract): an Aplicacao that doesn't declare
        // `:placement :affinity` and uses a non-Sharded strategy
        // (`shardKey` always None) emits a `placement:` block with
        // exactly `estrategia` + `clusters` and no `affinity:` /
        // `shardKey:` keys. The Placement struct's
        // `skip_serializing_if = "Option::is_none"` semantic is what
        // delivers this; pin it at the renderer's exit so a future
        // refactor that drops the attribute (e.g. forcing every axis
        // to round-trip) silently bloating downstream programs.yaml
        // surfaces here.
        let mut c = aplicacao_caixa();
        if let Some(p) = c.placement.as_mut() {
            p.affinity = None;
            p.shard_key = None;
        }
        let entries = programs_for_aplicacao(&c).unwrap();
        for p in placement_blocks(&entries) {
            assert!(
                p.get(M3_PLACEMENT_KEY_AFFINITY).is_none(),
                "placement.affinity must be absent when :affinity is None"
            );
            assert!(
                p.get(M3_PLACEMENT_KEY_SHARD_KEY).is_none(),
                "placement.shardKey must be absent when :shard-key is None"
            );
            // Exactly 2 keys remain — estrategia + clusters.
            assert_eq!(p.len(), 2);
        }
    }

    #[test]
    fn programs_entry_placement_carries_shard_key_when_sharded() {
        // The `Sharded` strategy carries a `:shard-key` (validated
        // non-empty by [`AplicacaoSpec::validate_placement`] — the
        // ShardedKeyEmpty arm). Pin that the typed slot's value
        // round-trips through to `placement.shardKey` under the
        // canonical camelCase key — the future Akka-style cluster-
        // sharding reconciler (MESH-COMPOSITION §II.4) keys off this
        // exact spelling to compute hash-based entity placement.
        let mut c = aplicacao_caixa();
        if let Some(p) = c.placement.as_mut() {
            p.estrategia = PlacementStrategy::Sharded;
            p.shard_key = Some("$tenantId".into());
        }
        let entries = programs_for_aplicacao(&c).unwrap();
        for p in placement_blocks(&entries) {
            assert_eq!(
                p.get(M3_PLACEMENT_KEY_ESTRATEGIA).and_then(|v| v.as_str()),
                Some(M3_PLACEMENT_ESTRATEGIA_SHARDED)
            );
            assert_eq!(
                p.get(M3_PLACEMENT_KEY_SHARD_KEY).and_then(|v| v.as_str()),
                Some("$tenantId")
            );
        }
    }

    #[test]
    fn programs_entry_placement_appears_on_every_member() {
        // Multiple `:membros` entries → multiple programs.yaml rows.
        // The placement overlay must apply to every entry, not just
        // the first one — pin so a future refactor that hoists the
        // overlay out of the loop without re-cloning into each row
        // can't accidentally drop the placement from the tail
        // entries. Same hoist-out-of-loop guard the `:politicas`
        // overlay tests enshrine for HTTPRoute / CNP rules. The
        // fixture has 3 members; the assertion catches any regression
        // that drops the block from the 2nd or 3rd entry.
        let entries = programs_for_aplicacao(&aplicacao_caixa()).unwrap();
        assert_eq!(entries.len(), 3);
        let placements = placement_blocks(&entries);
        assert_eq!(placements.len(), 3);
        // Every placement block must carry the same estrategia +
        // clusters — the placement is graph-level (one per
        // Aplicacao), so it's identical across members by
        // construction.
        let first = placements[0];
        for p in &placements[1..] {
            assert_eq!(
                p.get(M3_PLACEMENT_KEY_ESTRATEGIA),
                first.get(M3_PLACEMENT_KEY_ESTRATEGIA),
                "placement.estrategia must be identical across all members"
            );
            assert_eq!(
                p.get(M3_PLACEMENT_KEY_CLUSTERS),
                first.get(M3_PLACEMENT_KEY_CLUSTERS),
                "placement.clusters must be identical across all members"
            );
        }
    }

    #[test]
    fn programs_entry_placement_uses_lifted_canonical_key() {
        // Pin the key spelling via the lifted [`M3_KEY_PLACEMENT`]
        // const (instead of an inline `"placement"` literal). Drift
        // between the renderer-side emission and the consumer-side
        // (lareira-fleet-programs aggregator's filter, future
        // `app-operator` dispatcher) is a programs.yaml entry whose
        // placement is silently dropped at the consumer's filter
        // step. Lifting the key to a const + pinning the const here
        // makes a future top-level rename a one-line edit + this
        // test's verification, not a search-and-replace across every
        // consumer crate.
        assert_eq!(M3_KEY_PLACEMENT, "placement");
        let entries = programs_for_aplicacao(&aplicacao_caixa()).unwrap();
        for e in &entries {
            let m = e.as_mapping().expect("entry mapping");
            assert!(
                m.contains_key(M3_KEY_PLACEMENT),
                "entry must carry the M3_KEY_PLACEMENT key exactly"
            );
        }
    }

    #[test]
    fn m3_placement_key_estrategia_pins_canonical_value() {
        // Bridge-arm pin: [`M3_PLACEMENT_KEY_ESTRATEGIA`] resolves to
        // the canonical `"estrategia"` byte today — the exact YAML
        // sub-key the M3 [`caixa_core::aplicacao::Placement`] struct's
        // `#[serde(rename_all = "camelCase")]` derive emits for its
        // `estrategia` field, and the exact scalar every downstream
        // dispatch consults (the lareira-fleet-programs aggregator's
        // per-entry `placement.estrategia` strategy branch, the future
        // `app-operator` reconciler's per-Aplicacao takeover dispatch,
        // the future `mesh.pleme.io/v1alpha1/Aplicacao` CR
        // materializer's admission-time typed-enum bind, the M3
        // Adaptive compression weighting per MESH-COMPOSITION.md §V).
        // Peer with the [`programs_entry_placement_uses_lifted_canonical_key`]
        // canonical-literal pin on the sibling per-entry overlay-key
        // surface — that pin anchors the top-level `placement:` byte,
        // this pin anchors the per-sub-block `estrategia:` byte both
        // consumers dispatch on.
        assert_eq!(M3_PLACEMENT_KEY_ESTRATEGIA, "estrategia");
    }

    #[test]
    fn m3_placement_key_estrategia_matches_placement_serde_derive() {
        // Structural pin: the lifted [`M3_PLACEMENT_KEY_ESTRATEGIA`]
        // byte equals the exact key
        // [`caixa_core::aplicacao::Placement`]'s
        // `#[serde(rename_all = "camelCase")]` derive emits for its
        // `estrategia` field. A future refactor that (a) renames the
        // Rust field to `strategy` / `distribution` for English-
        // uniformity, or (b) retains the field name but adds a
        // per-field `#[serde(rename = "…")]` override, or (c) drops
        // the `rename_all = "camelCase"` attribute entirely, would
        // silently emit a `placement:` block whose distribution-
        // strategy discriminator lands under one key while every
        // downstream consumer (the lareira-fleet-programs aggregator's
        // dispatch, the future `app-operator` reconciler, the future
        // CR materializer's admission bind) still probes another. The
        // structural bind between the derive-time output and the
        // consumer-side navigation const is what this pin enforces —
        // any derive-side rebrand must be a coordinated edit at the
        // lifted const's definition site + here, not a silent apply-
        // time no-op at the aggregator's filter step. Peer with the
        // [`FLEET_PROGRAMS_KEY_APLICACAO`] / [`FLEET_PROGRAMS_KEY_VERSAO`]
        // / [`FLEET_PROGRAMS_KEY_NAME`] canonical-literal pins on the
        // sibling per-entry fleet-programs schema-key surfaces on the
        // same "one const, structurally bound to the derive-emitted
        // shape, tested at both endpoints" discipline every prior
        // canonical-schema-key lift on this surface established.
        let placement = Placement {
            estrategia: PlacementStrategy::Replicated,
            clusters: vec!["rio".to_string(), "mar".to_string()],
            affinity: None,
            shard_key: None,
        };
        let value = serde_yaml::to_value(&placement).expect("serialize Placement");
        let mapping = value
            .as_mapping()
            .expect("Placement serializes to a mapping");
        assert!(
            mapping.contains_key(M3_PLACEMENT_KEY_ESTRATEGIA),
            "Placement's serde derive must emit the estrategia axis under the exact key \
             the lifted M3_PLACEMENT_KEY_ESTRATEGIA const carries; got mapping keys: {keys:?}",
            keys = mapping
                .keys()
                .filter_map(|k| k.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn m3_placement_key_clusters_pins_canonical_value() {
        // Bridge-arm pin: [`M3_PLACEMENT_KEY_CLUSTERS`] resolves to the
        // canonical `"clusters"` byte today — the exact YAML sub-key the
        // M3 [`caixa_core::aplicacao::Placement`] struct's
        // `#[serde(rename_all = "camelCase")]` derive emits for its
        // `clusters` field, and the exact scalar every downstream
        // per-cluster fanout consumer scopes off (the lareira-fleet-
        // programs aggregator's per-cluster `.placement.clusters | contains
        // .Values.cluster` filter, the future `app-operator` reconciler's
        // per-Aplicacao cluster-set dispatch, the future
        // `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's admission-
        // time typed-list bind, the M3 Adaptive per-cluster weighting per
        // MESH-COMPOSITION.md §V). Peer with the
        // [`m3_placement_key_estrategia_pins_canonical_value`] canonical-
        // literal pin on the sibling per-sub-block strategy-discriminator
        // surface — that pin anchors the per-`placement:` `estrategia:`
        // byte every dispatch consumer branches on, this pin anchors the
        // per-`placement:` `clusters:` byte every per-cluster fanout
        // consumer filters by.
        assert_eq!(M3_PLACEMENT_KEY_CLUSTERS, "clusters");
    }

    #[test]
    fn m3_placement_key_clusters_matches_placement_serde_derive() {
        // Structural pin: the lifted [`M3_PLACEMENT_KEY_CLUSTERS`] byte
        // equals the exact key [`caixa_core::aplicacao::Placement`]'s
        // `#[serde(rename_all = "camelCase")]` derive emits for its
        // `clusters` field. A future refactor that (a) renames the Rust
        // field to `clusterPool` / `sites` for schema-clarity or eventual
        // multi-substrate reach, or (b) retains the field name but adds a
        // per-field `#[serde(rename = "…")]` override, or (c) drops the
        // `rename_all = "camelCase"` attribute entirely, would silently
        // emit a `placement:` block whose cluster-list lands under one
        // key while every downstream per-cluster fanout consumer (the
        // lareira-fleet-programs aggregator's filter, the future
        // `app-operator` reconciler, the future CR materializer's
        // admission bind) still probes another. The structural bind
        // between the derive-time output and the consumer-side navigation
        // const is what this pin enforces — any derive-side rebrand must
        // be a coordinated edit at the lifted const's definition site +
        // here, not a silent apply-time no-op at the aggregator's fanout
        // step. Peer with the
        // [`m3_placement_key_estrategia_matches_placement_serde_derive`]
        // structural pin on the sibling per-sub-block strategy-
        // discriminator axis on the same "one const, structurally bound
        // to the derive-emitted shape, tested at both endpoints"
        // discipline every prior canonical-schema-key lift on this
        // surface established.
        let placement = Placement {
            estrategia: PlacementStrategy::Replicated,
            clusters: vec!["rio".to_string(), "mar".to_string()],
            affinity: None,
            shard_key: None,
        };
        let value = serde_yaml::to_value(&placement).expect("serialize Placement");
        let mapping = value
            .as_mapping()
            .expect("Placement serializes to a mapping");
        assert!(
            mapping.contains_key(M3_PLACEMENT_KEY_CLUSTERS),
            "Placement's serde derive must emit the clusters axis under the exact key \
             the lifted M3_PLACEMENT_KEY_CLUSTERS const carries; got mapping keys: {keys:?}",
            keys = mapping
                .keys()
                .filter_map(|k| k.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn m3_placement_key_affinity_pins_canonical_value() {
        // Bridge-arm pin: [`M3_PLACEMENT_KEY_AFFINITY`] resolves to the
        // canonical `"affinity"` byte today — the exact YAML sub-key the
        // M3 [`caixa_core::aplicacao::Placement`] struct's
        // `#[serde(rename_all = "camelCase")]` derive emits for its
        // `affinity` field, and the exact scalar every downstream
        // placement-hint consumer weights off (the lareira-fleet-programs
        // aggregator's per-entry M3 Adaptive compression pass per
        // MESH-COMPOSITION.md §V, the future `app-operator` reconciler's
        // per-Aplicacao pod-affinity / node-affinity K8s-primitive
        // materializer, the future `mesh.pleme.io/v1alpha1/Aplicacao` CR
        // materializer's admission-time typed-string bind, the M4 cross-
        // cluster placement engine's per-hint takeover-priority dispatch).
        // Peer with the [`m3_placement_key_estrategia_pins_canonical_value`]
        // + [`m3_placement_key_clusters_pins_canonical_value`] canonical-
        // literal pins on the sibling per-sub-block always-emitted
        // strategy-discriminator / cluster-pool surfaces — those pins
        // anchor the always-emitted `estrategia:` / `clusters:` bytes
        // every dispatch / fanout consumer branches on, this pin anchors
        // the optional-emitted `affinity:` byte every weighting consumer
        // reads off when the typed slot resolves to `Some(_)`.
        assert_eq!(M3_PLACEMENT_KEY_AFFINITY, "affinity");
    }

    #[test]
    fn m3_placement_key_affinity_matches_placement_serde_derive() {
        // Structural pin: the lifted [`M3_PLACEMENT_KEY_AFFINITY`] byte
        // equals the exact key [`caixa_core::aplicacao::Placement`]'s
        // `#[serde(rename_all = "camelCase")]` derive emits for its
        // `affinity` field when the typed slot resolves to `Some(_)`. A
        // future refactor that (a) renames the Rust field to
        // `affinityHint` / `placementHint` for schema-clarity, or (b)
        // retains the field name but adds a per-field
        // `#[serde(rename = "…")]` override, or (c) drops the
        // `rename_all = "camelCase"` attribute entirely, or (d) drops the
        // `skip_serializing_if = "Option::is_none"` attribute (letting a
        // `None` slot emit `affinity: null` and thereby breaking the
        // omit-when-unset contract every peer typed slot carries), would
        // silently emit a `placement:` block whose affinity hint lands
        // under one key while every downstream weighting consumer (the M3
        // Adaptive compression pass, the future `app-operator`
        // reconciler's pod-affinity / node-affinity materializer, the
        // future CR materializer's admission bind, the M4 cross-cluster
        // placement engine's per-hint dispatch) still probes another. The
        // structural bind between the derive-time output and the
        // consumer-side navigation const is what this pin enforces — any
        // derive-side rebrand must be a coordinated edit at the lifted
        // const's definition site + here, not a silent apply-time no-op
        // at the aggregator's weighting step. Peer with the
        // [`m3_placement_key_estrategia_matches_placement_serde_derive`]
        // + [`m3_placement_key_clusters_matches_placement_serde_derive`]
        // structural pins on the sibling per-sub-block always-emitted
        // axes on the same "one const, structurally bound to the derive-
        // emitted shape, tested at both endpoints" discipline every prior
        // canonical-schema-key lift on this surface established. Unlike
        // the peer pins (which construct a `Placement` with the axis
        // always present and simply probe for the key), this pin
        // constructs a `Placement` with `affinity: Some(_)` to force the
        // `skip_serializing_if` gate open so the derive-emitted key
        // actually appears in the serialized mapping.
        let placement = Placement {
            estrategia: PlacementStrategy::Replicated,
            clusters: vec!["rio".to_string(), "mar".to_string()],
            affinity: Some("data-locality".to_string()),
            shard_key: None,
        };
        let value = serde_yaml::to_value(&placement).expect("serialize Placement");
        let mapping = value
            .as_mapping()
            .expect("Placement serializes to a mapping");
        assert!(
            mapping.contains_key(M3_PLACEMENT_KEY_AFFINITY),
            "Placement's serde derive must emit the affinity axis under the exact key \
             the lifted M3_PLACEMENT_KEY_AFFINITY const carries when the typed slot \
             resolves to `Some(_)`; got mapping keys: {keys:?}",
            keys = mapping
                .keys()
                .filter_map(|k| k.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn m3_placement_key_shard_key_pins_canonical_value() {
        // Bridge-arm pin: [`M3_PLACEMENT_KEY_SHARD_KEY`] resolves to the
        // canonical `"shardKey"` byte today — the exact YAML sub-key the
        // M3 [`caixa_core::aplicacao::Placement`] struct's
        // `#[serde(rename_all = "camelCase")]` derive emits for its
        // `shard_key` field, and the exact scalar every downstream shard-
        // dispatch consumer materializes off (the lareira-fleet-programs
        // aggregator's per-entry M3 shard-pool dispatch materializer per
        // MESH-COMPOSITION.md §II.4, the future `app-operator`
        // reconciler's per-Aplicacao `ShardedResource` CR emitter, the
        // future `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's
        // admission-time typed-string bind, the M4 Orleans-style virtual-
        // actor runtime's per-grain placement dispatch per
        // RUNTIME-PATTERNS.md). Peer with the
        // [`m3_placement_key_estrategia_pins_canonical_value`] +
        // [`m3_placement_key_clusters_pins_canonical_value`] +
        // [`m3_placement_key_affinity_pins_canonical_value`] canonical-
        // literal pins on the sibling per-sub-block strategy-discriminator
        // / cluster-pool / placement-hint surfaces — those pins anchor
        // the peer axes' bytes, this pin anchors the optional-emitted
        // `shardKey:` byte every shard-dispatch consumer keys off when
        // the typed slot resolves to `Some(_)` under the `Sharded`
        // strategy.
        assert_eq!(M3_PLACEMENT_KEY_SHARD_KEY, "shardKey");
    }

    #[test]
    fn m3_placement_key_shard_key_matches_placement_serde_derive() {
        // Structural pin: the lifted [`M3_PLACEMENT_KEY_SHARD_KEY`] byte
        // equals the exact key [`caixa_core::aplicacao::Placement`]'s
        // `#[serde(rename_all = "camelCase")]` derive emits for its
        // `shard_key` field when the typed slot resolves to `Some(_)`. A
        // future refactor that (a) renames the Rust field to
        // `partition_key` for Kafka-symmetric naming, `entity_key` for
        // Akka/Orleans-symmetric naming, `hash_key` for schema-clarity,
        // etc., or (b) retains the field name but adds a per-field
        // `#[serde(rename = "…")]` override, or (c) drops the
        // `rename_all = "camelCase"` attribute entirely, or (d) drops the
        // `skip_serializing_if = "Option::is_none"` attribute (letting a
        // `None` slot emit `shardKey: null` and thereby breaking the
        // omit-when-unset contract every peer typed slot carries), would
        // silently emit a `placement:` block whose shard-selection
        // template lands under one key while every downstream shard-
        // dispatch consumer (the M3 shard-pool dispatch materializer, the
        // future `app-operator` reconciler's `ShardedResource` CR
        // emitter, the future CR materializer's admission bind, the M4
        // Orleans-style virtual-actor runtime's per-grain placement
        // dispatch) still probes another. The structural bind between the
        // derive-time output and the consumer-side navigation const is
        // what this pin enforces — any derive-side rebrand must be a
        // coordinated edit at the lifted const's definition site + here,
        // not a silent apply-time no-op at the aggregator's shard-
        // dispatch step. Peer with the sibling per-sub-block
        // `matches_placement_serde_derive` pins on the same "one const,
        // structurally bound to the derive-emitted shape, tested at both
        // endpoints" discipline every prior canonical-schema-key lift on
        // this surface established. Like the peer
        // [`m3_placement_key_affinity_matches_placement_serde_derive`]
        // pin (and unlike the always-emitted `estrategia` / `clusters`
        // pins), this pin constructs a `Placement` with
        // `shard_key: Some(_)` to force the `skip_serializing_if` gate
        // open so the derive-emitted key actually appears in the
        // serialized mapping. Uniquely on this axis (relative to every
        // sibling `Placement` sub-key pin), the underlying serde
        // transform is *not* a no-op — the source-side field name
        // `shard_key` carries a `_` the `rename_all = "camelCase"`
        // derive actively transforms to `shardKey`, so any rebrand that
        // touches either endpoint of the transform (the field name OR
        // the `rename_all` attribute OR a per-field `rename` override)
        // reaches this assertion by construction.
        let placement = Placement {
            estrategia: PlacementStrategy::Sharded,
            clusters: vec!["rio".to_string(), "mar".to_string()],
            affinity: None,
            shard_key: Some("$tenantId".to_string()),
        };
        let value = serde_yaml::to_value(&placement).expect("serialize Placement");
        let mapping = value
            .as_mapping()
            .expect("Placement serializes to a mapping");
        assert!(
            mapping.contains_key(M3_PLACEMENT_KEY_SHARD_KEY),
            "Placement's serde derive must emit the shard_key axis under the exact key \
             the lifted M3_PLACEMENT_KEY_SHARD_KEY const carries when the typed slot \
             resolves to `Some(_)`; got mapping keys: {keys:?}",
            keys = mapping
                .keys()
                .filter_map(|k| k.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn typed_view_returns_validated_spec() {
        let spec = typed_view(&aplicacao_caixa()).unwrap();
        // Route the per-`:membros` / per-`:contratos` list-length probes and
        // the per-`:placement` distribution-target-list length probe through
        // the lifted [`caixa_core::AplicacaoSpec::membros`] /
        // [`caixa_core::AplicacaoSpec::contratos`] slice-return accessors and
        // the paired [`caixa_core::AplicacaoSpec::placement`] +
        // [`caixa_core::Placement::clusters`] outer-then-inner accessors
        // rather than the raw `spec.<field>` / `spec.placement.clusters`
        // field accesses — sibling to the peer per-`:entrada` presence-bit
        // probe below that already routes through
        // [`caixa_core::AplicacaoSpec::entrada`] (9e8630e). Every accessor
        // projects its backing slot verbatim (byte-equal to the raw field
        // access), pinned in caixa-core at
        // `aplicacao_spec_membros_returns_membros_slice_byte_equal_across_permutations`,
        // `aplicacao_spec_contratos_returns_contratos_slice_byte_equal_across_permutations`,
        // `aplicacao_spec_placement_returns_placement_ref_byte_equal_across_permutations`,
        // and
        // `placement_clusters_returns_clusters_slice_byte_equal_across_permutations`.
        // Closes the last unlifted per-`AplicacaoSpec` raw-field-access
        // sites in the caixa-mesh `typed_view_returns_validated_spec` test.
        assert_eq!(spec.membros().len(), 3);
        assert_eq!(spec.contratos().len(), 2);
        // Route the per-`:entrada` presence-bit probe through the lifted
        // [`caixa_core::AplicacaoSpec::entrada`] accessor rather than the
        // raw `spec.entrada.is_some()` field access — sibling to the
        // production `gateway_routes` reader at :2806 that already routes
        // its `Some(_)` / `None` early-return partition through the same
        // accessor. The accessor projects the raw `Option<Entrada>` slot's
        // presence bit through the reference-return unchanged (`entrada()
        // -> Option<&Entrada>` = `self.entrada.as_ref()`), so `is_some()`
        // on both sides is byte-equal — pinned in caixa-core at
        // `aplicacao_spec_entrada_returns_entrada_option_ref_byte_equal_across_permutations`.
        assert!(spec.entrada().is_some());
        assert_eq!(spec.placement().clusters().len(), 2);
    }

    #[test]
    fn cilium_emits_one_policy_per_de_para_pair() {
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        assert_eq!(policies.len(), 2);
        let names: Vec<_> = policies
            .iter()
            .map(|p| kube_name(p).unwrap().to_string())
            .collect();
        assert!(names.contains(&"checkout-cart-to-catalog".to_string()));
        assert!(names.contains(&"checkout-cart-to-payment".to_string()));
    }

    #[test]
    fn cilium_fans_same_de_para_edges_into_one_policy() {
        // Fail-before / pass-after: AplicacaoSpec::validate permits two
        // contratos sharing a `(:de, :para)` pair with distinct payloads
        // (cart→catalog at `/products/:id` *and* `/search`). The renderer
        // names every CiliumNetworkPolicy `<aplicacao>-<de>-to-<para>`, so
        // before the per-pair fan-in those two contratos rendered two
        // objects both named `checkout-cart-to-catalog` — a `kubectl apply`
        // collision far from the source caixa.lisp. They must now fan into
        // exactly one policy whose `ingress[0].toPorts[]` carries both
        // edges' L7 paths.
        let mut c = aplicacao_caixa();
        c.contratos.push(WitContract {
            de: "cart".into(),
            para: "catalog".into(),
            wit: "wasi:http/proxy".into(),
            endpoint: Some("/search".into()),
            subject: None,
            slot: None,
        });
        let policies = cilium_network_policies(&c).unwrap();

        let cart_to_catalog: Vec<_> = policies
            .iter()
            .filter(|p| kube_name_is(p, "checkout-cart-to-catalog"))
            .collect();
        assert_eq!(
            cart_to_catalog.len(),
            1,
            "two cart→catalog contratos must fan into one policy, not two \
             colliding `checkout-cart-to-catalog` objects"
        );

        let to_ports = kube_spec_field(cart_to_catalog[0], CILIUM_KEY_INGRESS)
            .and_then(|i| i.as_sequence())
            .and_then(|s| s.first())
            .and_then(|i| i.get(CILIUM_KEY_TO_PORTS))
            .and_then(|p| p.as_sequence())
            .expect("ingress[0].toPorts sequence");
        assert_eq!(
            to_ports.len(),
            2,
            "each typed edge in the (cart, catalog) group contributes one toPorts entry"
        );
        let paths: Vec<&str> = to_ports
            .iter()
            .filter_map(|tp| {
                tp.get(KUBE_KEY_RULES)
                    .and_then(|r| r.get(CILIUM_KEY_HTTP))
                    .and_then(|h| h.as_sequence())
                    .and_then(|s| s.first())
                    .and_then(|rule| rule.get(CILIUM_KEY_PATH))
                    .and_then(|v| v.as_str())
            })
            .collect();
        assert!(
            paths.contains(&"/products/:id") && paths.contains(&"/search"),
            "both edges' L7 paths must survive the fan-in, got {paths:?}"
        );
    }

    #[test]
    fn cilium_policies_are_identity_based() {
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        for p in &policies {
            let endpoint = kube_spec_field(p, CILIUM_KEY_ENDPOINT_SELECTOR)
                .and_then(|e| e.get(KUBE_KEY_MATCH_LABELS))
                .unwrap();
            assert!(endpoint.get(LABEL_PROGRAM).is_some());
            // Source endpoint must include both program + aplicacao labels
            let from = kube_spec_field(p, CILIUM_KEY_INGRESS)
                .and_then(|i| i.as_sequence())
                .and_then(|s| s.first())
                .and_then(|i| i.get(CILIUM_KEY_FROM_ENDPOINTS))
                .and_then(|e| e.as_sequence())
                .and_then(|s| s.first())
                .and_then(|e| e.get(KUBE_KEY_MATCH_LABELS))
                .unwrap();
            assert_eq!(
                from.get(LABEL_APLICACAO).and_then(|v| v.as_str()),
                Some("checkout")
            );
            let from_program = from.get(LABEL_PROGRAM).and_then(|v| v.as_str()).unwrap();
            assert!(
                from_program == "cart" || from_program == "payment",
                "fromEndpoints.matchLabels.{LABEL_PROGRAM} = {from_program:?} \
                 must name the source caixa of one of the fixture's two contratos"
            );
        }
    }

    #[test]
    fn cilium_policy_metadata_labels_use_lifted_consts() {
        // The policy's own labels (carried at metadata.labels, not on
        // workload pods) must come through caixa_core::render's typed
        // constants. Pinning the keys via the lifted consts (instead
        // of inline `"pleme.pleme.io/aplicacao"` strings) makes drift
        // between render-side emission and consumer-side selection
        // (Hubble flow grouping, operator policy filters) a build
        // error: a future label-namespace rename is one PLEME_LABEL_PREFIX
        // edit, and this test pins that the rename actually flows
        // through to the policy metadata.
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        for p in &policies {
            let labels = kube_metadata_labels(p).expect("policy metadata.labels mapping");
            assert!(
                kube_metadata_label_is(p, LABEL_APLICACAO, "checkout"),
                "policy metadata.labels.LABEL_APLICACAO must byte-equal \
                 the parent Aplicacao's `:nome` — routes through the \
                 lifted predicate arity that closes the three-arity \
                 closure on the metadata.labels.<label> selector axis"
            );
            // The contrato label is `<de>-to-<para>`; both fixture
            // edges have :de = "cart".
            let contrato_val =
                kube_metadata_label(p, LABEL_CONTRATO).expect("contrato label present");
            assert!(
                contrato_val.starts_with("cart-to-"),
                "contrato label {contrato_val:?} must follow `<de>-to-<para>` shape"
            );
            // No leaked stale labels — every pleme-prefixed key on the
            // policy's own metadata must come from the lifted const set.
            // (Workload-identity labels live elsewhere — this only
            // checks the policy's *own* metadata.labels block.)
            for (k, _) in labels {
                if let Some(s) = k.as_str() {
                    if s.starts_with(caixa_core::PLEME_LABEL_PREFIX) {
                        assert!(
                            s == LABEL_APLICACAO || s == LABEL_CONTRATO,
                            "policy metadata.labels carries unexpected pleme-prefixed key {s:?} \
                             (only LABEL_APLICACAO + LABEL_CONTRATO are canonical here)"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn cilium_endpoint_selector_is_program_only() {
        // The destination matchLabels must be the single-axis
        // (program-only) selector — pinning that the lift to the
        // typed helper preserves the existing one-key semantic
        // (caixa-mesh deliberately matches every pod with the
        // destination program name in this cluster, regardless of
        // Aplicacao). If a future change wants to scope the
        // destination by Aplicacao too, that's an intentional
        // semantic shift, not an accidental drift.
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        for p in &policies {
            let selector = kube_spec_field(p, CILIUM_KEY_ENDPOINT_SELECTOR)
                .and_then(|e| e.get(KUBE_KEY_MATCH_LABELS))
                .and_then(|m| m.as_mapping())
                .expect("endpointSelector.matchLabels mapping");
            assert_eq!(
                selector.len(),
                1,
                "destination endpointSelector must be the program-only selector"
            );
            assert!(selector.get(LABEL_PROGRAM).is_some());
        }
    }

    #[test]
    fn cilium_from_endpoints_carries_aplicacao_scoped_selector() {
        // The source fromEndpoints.matchLabels must be the two-axis
        // selector (program + aplicacao) — pinning that the lift to
        // pleme_program_in_aplicacao_selector preserves the safety
        // property that a same-named program in a different Aplicacao
        // cannot satisfy the rule.
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        for p in &policies {
            let from = kube_spec_field(p, CILIUM_KEY_INGRESS)
                .and_then(|i| i.as_sequence())
                .and_then(|s| s.first())
                .and_then(|i| i.get(CILIUM_KEY_FROM_ENDPOINTS))
                .and_then(|e| e.as_sequence())
                .and_then(|s| s.first())
                .and_then(|e| e.get(KUBE_KEY_MATCH_LABELS))
                .and_then(|m| m.as_mapping())
                .expect("fromEndpoints[0].matchLabels mapping");
            assert_eq!(
                from.len(),
                2,
                "source fromEndpoints must be the program-in-aplicacao selector (2 axes)"
            );
            assert!(from.get(LABEL_PROGRAM).is_some());
            assert!(from.get(LABEL_APLICACAO).is_some());
        }
    }

    #[test]
    fn cilium_http_contracts_emit_l7_rules() {
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        let cart_to_catalog = find_by_name(&policies, "checkout-cart-to-catalog").unwrap();
        let http_rules = kube_spec_field(cart_to_catalog, CILIUM_KEY_INGRESS)
            .and_then(|i| i.as_sequence())
            .and_then(|s| s.first())
            .and_then(|i| i.get(CILIUM_KEY_TO_PORTS))
            .and_then(|p| p.as_sequence())
            .and_then(|s| s.first())
            .and_then(|p| p.get(KUBE_KEY_RULES))
            .and_then(|r| r.get(CILIUM_KEY_HTTP))
            .and_then(|h| h.as_sequence())
            .unwrap();
        assert_eq!(http_rules.len(), 1);
        assert_eq!(
            http_rules[0].get(CILIUM_KEY_PATH).and_then(|v| v.as_str()),
            Some("/products/:id")
        );
    }

    #[test]
    fn cilium_pubsub_contracts_skip_l7_rules() {
        let mut c = aplicacao_caixa();
        c.contratos.push(WitContract {
            de: "payment".into(),
            para: "cart".into(), // back-edge for testing only
            wit: "nats:pub-sub".into(),
            endpoint: None,
            subject: Some("checkout.events.charge.failed".into()),
            slot: None,
        });
        let policies = cilium_network_policies(&c).unwrap();
        let nats_policy = find_by_name(&policies, "checkout-payment-to-cart").unwrap();
        let to_ports = kube_spec_field(nats_policy, CILIUM_KEY_INGRESS)
            .and_then(|i| i.as_sequence())
            .and_then(|s| s.first())
            .and_then(|i| i.get(CILIUM_KEY_TO_PORTS))
            .and_then(|p| p.as_sequence())
            .and_then(|s| s.first())
            .unwrap();
        // L4 ports yes; L7 rules no.
        assert!(to_ports.get(CILIUM_KEY_PORTS).is_some());
        assert!(to_ports.get(KUBE_KEY_RULES).is_none());
    }

    #[test]
    fn gateway_emits_gateway_plus_httproute_pair() {
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        assert_eq!(docs.len(), 2);
        let kinds: Vec<_> = docs
            .iter()
            .map(|d| {
                d.get(KUBE_KEY_KIND)
                    .and_then(|k| k.as_str())
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert!(kinds.contains(&GATEWAY_API_KIND_GATEWAY.to_string()));
        assert!(kinds.contains(&GATEWAY_API_KIND_HTTP_ROUTE.to_string()));
    }

    #[test]
    fn gateway_listener_carries_aplicacao_host() {
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let gateway = find_by_kind(&docs, GATEWAY_API_KIND_GATEWAY).unwrap();
        let listener = kube_spec_field(gateway, GATEWAY_API_KEY_LISTENERS)
            .and_then(|l| l.as_sequence())
            .and_then(|s| s.first())
            .unwrap();
        assert_eq!(
            listener
                .get(GATEWAY_API_KEY_HOSTNAME)
                .and_then(|h| h.as_str()),
            Some("checkout.quero.cloud")
        );
        assert_eq!(
            listener.get(KUBE_KEY_PROTOCOL).and_then(|p| p.as_str()),
            Some(GATEWAY_API_PROTOCOL_HTTP)
        );
    }

    #[test]
    fn gateway_listener_name_routes_through_lifted_default_http_listener_name() {
        // The per-Aplicacao `Gateway`'s sole per-listener name-
        // discriminator axis (the `listener.insert(GATEWAY_API_KEY_NAME,
        // …)` call site in [`gateway_routes`]) must read from the lifted
        // [`caixa_core::GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME`]
        // `&'static str` constant — not from an open-coded `"http"`
        // literal that could drift if the substrate's canonical
        // author-chosen short listener-name ever moved (`"http" →
        // "http-v1"` on the multi-listener HTTPS-by-default trajectory,
        // a per-cluster override the operator pins through a future
        // `:entrada :listener-name` slot). A future rebrand of the
        // constant must reach this consumer by construction so
        // downstream `HTTPRoute.spec.parentRefs[].sectionName`
        // selectors that bind by the canonical byte-string can't
        // silently orphan the route at attachment time. Peer with the
        // sibling
        // [`gateway_listener_port_routes_through_lifted_default_http_listener_port`]
        // pin on the [`GATEWAY_API_DEFAULT_HTTP_LISTENER_PORT`] port-
        // scalar consumer at the same emitter — the two per-listener
        // substrate-canonical scalar-value axes name distinct scalars
        // (listener name identifier vs listener port), both now routed
        // through their own lifted const, so a substrate-side rebrand
        // on either axis lands at exactly one consumer per axis
        // without coupling the two rebrand cycles.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let gateway = find_by_kind(&docs, GATEWAY_API_KIND_GATEWAY).expect("Gateway present");
        let listener = kube_spec_field(gateway, GATEWAY_API_KEY_LISTENERS)
            .and_then(|l| l.as_sequence())
            .and_then(|s| s.first())
            .expect("first listener present");
        assert_eq!(
            listener.get(GATEWAY_API_KEY_NAME).and_then(|n| n.as_str()),
            Some(GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME),
            "the Gateway per-listener name-discriminator scalar must render \
             the lifted GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME constant \
             verbatim — drift here means the constant lift no longer reaches \
             this consumer and every downstream HTTPRoute `sectionName` \
             selector authored against the substrate's canonical name would \
             miss its listener at attachment time"
        );
    }

    #[test]
    fn httproute_parent_ref_pins_section_name_to_lifted_default_http_listener_name() {
        // The per-Aplicacao `HTTPRoute`'s sole per-parentRef listener-
        // selector sub-axis (the `parent_ref.insert(GATEWAY_API_KEY_SECTION_NAME,
        // GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME)` call site in
        // [`gateway_routes`]) must render the same lifted
        // [`caixa_core::GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME`]
        // `&'static str` constant the sibling Gateway listener-name
        // emitter reaches for — the paired
        // [`gateway_listener_name_routes_through_lifted_default_http_listener_name`]
        // pin fires the same byte-string on the sibling
        // `listener.insert(GATEWAY_API_KEY_NAME, …)` call, and this
        // pin closes the sectionName half so the substrate's canonical
        // per-listener identity pair (`Gateway.spec.listeners[].name`
        // + `HTTPRoute.spec.parentRefs[].sectionName`) moves as a
        // single unit through one lifted `&'static str`.
        //
        // Until this line landed the emitter omitted the selector
        // entirely, silently accepting the Gateway API v1 default
        // attach-to-every-listener fan-out. A future substrate-side
        // second listener under the same parent Gateway (the
        // cert-manager-issued per-`:entrada :host` HTTPS listener the
        // sibling [`GATEWAY_API_DEFAULT_HTTP_LISTENER_PORT`] docstring
        // forecasts) would have silently doubled every route's per-
        // request dispatch surface — every external `:entrada`
        // request the route was authored to accept on the HTTP
        // listener would have accepted a matching request on the
        // paired HTTPS listener too, with the second-listener leak
        // surfacing only in per-request access logs (never in
        // `kubectl describe httproute` — the implicit fan-out reads
        // as intended per the Gateway API v1 spec). Pinning the
        // selector by construction closes that drift footgun
        // structurally: a substrate-side rebrand of the canonical
        // listener-name identifier reaches both the listener-name
        // emitter and the sectionName selector at construction time.
        //
        // Peer with the sibling
        // [`gateway_listener_name_routes_through_lifted_default_http_listener_name`]
        // pin on the same lifted const — the two per-listener
        // substrate-canonical byte-string axes (`Gateway.spec.
        // listeners[].name` vs `HTTPRoute.spec.parentRefs[].sectionName`)
        // now bind by construction to the same lifted `&'static str`,
        // so a rebrand on either axis reaches its consumer through
        // one canonical caixa-core declaration.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let route = find_by_kind(&docs, GATEWAY_API_KIND_HTTP_ROUTE).expect("HTTPRoute present");
        let parent = kube_spec_field(route, GATEWAY_API_KEY_PARENT_REFS)
            .and_then(|p| p.as_sequence())
            .and_then(|s| s.first())
            .expect("first parentRef present");
        assert_eq!(
            parent
                .get(GATEWAY_API_KEY_SECTION_NAME)
                .and_then(|n| n.as_str()),
            Some(GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME),
            "the HTTPRoute per-parentRef listener-selector scalar must \
             render the lifted GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME \
             constant verbatim — the Gateway listener-name emitter and \
             this sectionName selector must move as a unit through one \
             canonical caixa-core `&'static str`, else a future \
             listener-name rebrand silently splits the per-listener \
             identity pair and the emitted route reverts to the Gateway \
             API v1 attach-to-every-listener default fan-out"
        );
    }

    #[test]
    fn gateway_listener_port_routes_through_lifted_default_http_listener_port() {
        // The per-Aplicacao `Gateway`'s sole per-listener HTTP-listener-
        // port axis (the `listener.insert(KUBE_KEY_PORT, …)` call site
        // in [`gateway_routes`]) must read from the lifted
        // [`caixa_core::GATEWAY_API_DEFAULT_HTTP_LISTENER_PORT`] `u16`
        // constant — not from an open-coded `80` literal that could
        // drift if the substrate's canonical external-Gateway HTTP
        // listener port ever moved (`:80 → :443` on the HTTPS-by-
        // default trajectory, a per-cluster override the operator
        // pins). A future rebrand of the constant must reach this
        // consumer by construction. Peer with the sibling
        // [`cnp_l4_fallback_port_routes_through_lifted_default_servico_port`]
        // pin on the [`DEFAULT_SERVICO_PORT`] fallback in the
        // `cilium_network_policies` per-`(:de, :para)` L4 port
        // resolver — the two axes name distinct scalars (external
        // Gateway listener port vs in-cluster Servico port), both now
        // routed through their own lifted `u16` const, so a substrate-
        // side port migration on either axis lands at exactly one
        // consumer per axis without coupling the two rebrand cycles.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let gateway = find_by_kind(&docs, GATEWAY_API_KIND_GATEWAY).expect("Gateway present");
        let listener = kube_spec_field(gateway, GATEWAY_API_KEY_LISTENERS)
            .and_then(|l| l.as_sequence())
            .and_then(|s| s.first())
            .expect("first listener present");
        assert_eq!(
            listener.get(KUBE_KEY_PORT).and_then(|p| p.as_u64()),
            Some(u64::from(GATEWAY_API_DEFAULT_HTTP_LISTENER_PORT)),
            "the Gateway per-listener HTTP-listener-port scalar must render \
             the lifted GATEWAY_API_DEFAULT_HTTP_LISTENER_PORT constant \
             verbatim — drift here means the constant lift no longer reaches \
             this consumer"
        );
    }

    #[test]
    fn httproute_catch_all_path_routes_through_lifted_default_http_route_path() {
        // The per-Aplicacao `HTTPRoute`'s empty-`:entrada :paths`
        // catch-all resolver (the `let paths: Vec<&str> = if
        // entrada.paths.is_empty() { vec![…] } else { … }` branch in
        // [`gateway_routes`]) must read from the lifted
        // [`caixa_core::GATEWAY_API_DEFAULT_HTTP_ROUTE_PATH`]
        // `&'static str` constant — not from an open-coded `"/"`
        // literal that could drift if the substrate's canonical
        // catch-all URL-path shape ever moved (`"/"` → the Gateway API
        // v2 `Exact ""` idiom on a per-controller variant that treats
        // `"/"` as a literal prefix rather than the catch-all, an
        // operator-pinned override the future `:entrada :default-path`
        // slot promotes). A future rebrand of the constant must reach
        // this consumer by construction so an author who declared an
        // external `:entrada` but no per-path rule surface still gets
        // a route whose sole `HTTPPathMatch` matches every incoming
        // request under the paired
        // [`GATEWAY_API_PATH_MATCH_TYPE_PATH_PREFIX`] discriminator.
        // Peer with the sibling
        // [`gateway_listener_name_routes_through_lifted_default_http_listener_name`]
        // and
        // [`gateway_listener_port_routes_through_lifted_default_http_listener_port`]
        // pins on the [`GATEWAY_API_DEFAULT_HTTP_LISTENER_NAME`] /
        // [`GATEWAY_API_DEFAULT_HTTP_LISTENER_PORT`] consumers at the
        // same emitter — all three per-Gateway-API-CRD substrate-
        // canonical scalar-value axes now routed through their own
        // lifted const, so a substrate-side rebrand on any one axis
        // lands at exactly one consumer per axis without coupling the
        // rebrand cycles.
        let mut caixa = aplicacao_caixa();
        caixa.entrada.as_mut().unwrap().paths = Vec::new();
        let docs = gateway_routes(&caixa).unwrap();
        let route = find_by_kind(&docs, GATEWAY_API_KIND_HTTP_ROUTE).expect("HTTPRoute present");
        let match_path_value = kube_spec_field(route, KUBE_KEY_RULES)
            .and_then(|r| r.as_sequence())
            .and_then(|s| s.first())
            .and_then(|r| r.get(GATEWAY_API_KEY_MATCHES))
            .and_then(|m| m.as_sequence())
            .and_then(|s| s.first())
            .and_then(|m| m.get(GATEWAY_API_KEY_PATH))
            .and_then(|p| p.get(GATEWAY_API_KEY_VALUE))
            .and_then(|v| v.as_str())
            .expect("HTTPRoute rules[0].matches[0].path.value present");
        assert_eq!(
            match_path_value, GATEWAY_API_DEFAULT_HTTP_ROUTE_PATH,
            "the HTTPRoute empty-`:entrada :paths` catch-all URL-path scalar \
             must render the lifted GATEWAY_API_DEFAULT_HTTP_ROUTE_PATH \
             constant verbatim — drift here means the constant lift no longer \
             reaches this consumer and every external `:entrada` HTTP flow \
             authored against a Servico with no per-path rule surface would \
             drop at the first hop with no diagnostic naming the catch-all-path \
             drift root cause"
        );
    }

    #[test]
    fn httproute_path_list_routes_through_lifted_entrada_resolved_paths() {
        // Cross-crate pin: the per-Aplicacao HTTPRoute rules[] path list
        // must render exactly [`caixa_core::Entrada::resolved_paths`]'s
        // typed dispatch on the substrate primitive — one rule per
        // resolved path, in the resolver's authored order. Pins that a
        // future renderer-side detour that re-inlined the `paths.is_empty()`
        // cascade (or reordered / deduped / dropped author-declared
        // paths) surfaces at caixa-mesh build time rather than at
        // cluster-apply time as a silently-dropped-route HTTP flow.
        //
        // Exercises BOTH arms of the resolver's accept-set at one
        // emitter call site: the empty-`:entrada :paths` catch-all
        // arm (fixture cleared to `Vec::new()`; resolver returns the
        // lifted `[GATEWAY_API_DEFAULT_HTTP_ROUTE_PATH]` singleton)
        // and the author-declared non-empty arm (fixture's baseline
        // two-path `["/api/cart", "/api/products"]` list; resolver
        // returns each entry verbatim in order). Peer discipline with
        // the sibling
        // [`httproute_catch_all_path_routes_through_lifted_default_http_route_path`]
        // pin on the empty-arm arm-scalar axis — that pin nails the
        // per-rule fallback scalar; this pin nails the per-rule
        // dispatch shape the typed method drives.
        for paths in [
            vec![],
            vec!["/api/cart".to_string(), "/api/products".to_string()],
            vec!["/only".to_string()],
        ] {
            let mut caixa = aplicacao_caixa();
            let expected: Vec<String> = caixa
                .entrada
                .as_mut()
                .map(|e| {
                    e.paths.clone_from(&paths);
                    e.resolved_paths().iter().map(|&s| s.to_string()).collect()
                })
                .expect("aplicacao_caixa carries a typed `:entrada` block");
            let docs = gateway_routes(&caixa).unwrap();
            let route = find_by_kind(&docs, GATEWAY_API_KIND_HTTP_ROUTE)
                .expect("HTTPRoute present under every :entrada permutation");
            let rules = kube_spec_field(route, KUBE_KEY_RULES)
                .and_then(|r| r.as_sequence())
                .expect("HTTPRoute.spec.rules[] present");
            let emitted: Vec<String> = rules
                .iter()
                .map(|r| {
                    r.get(GATEWAY_API_KEY_MATCHES)
                        .and_then(|m| m.as_sequence())
                        .and_then(|s| s.first())
                        .and_then(|m| m.get(GATEWAY_API_KEY_PATH))
                        .and_then(|p| p.get(GATEWAY_API_KEY_VALUE))
                        .and_then(|v| v.as_str())
                        .expect("each HTTPRoute rule carries a matches[0].path.value scalar")
                        .to_string()
                })
                .collect();
            assert_eq!(
                emitted, expected,
                "HTTPRoute per-rule path list must render \
                 `Entrada::resolved_paths()` verbatim (in author-\
                 declared order for the non-empty arm, as the lifted \
                 catch-all singleton for the empty arm) — drift here \
                 means the emitter no longer routes through the \
                 substrate-primitive typed dispatch and a future \
                 resolver axis (:default-path override, per-cluster \
                 overlay) would silently disagree between caixa-core \
                 and caixa-mesh on which paths a given `:entrada` \
                 block resolves to. Input paths: {paths:?}"
            );
        }
    }

    #[test]
    fn gateway_listener_hostname_routes_through_lifted_entrada_hostname() {
        // Cross-crate pin: the per-Aplicacao `Gateway`'s sole per-
        // listener singular `hostname:` filter must render exactly
        // [`caixa_core::Entrada::hostname`]'s typed dispatch on the
        // substrate primitive — not from an open-coded `entrada.host.
        // clone()` field access that could silently disagree with the
        // peer per-HTTPRoute plural `spec.hostnames[]` filter list on
        // future extensions of the `:entrada` slot to a multi-hostname
        // author surface. A drift here would surface at cluster-apply
        // time as an `Accepted:False/NoMatchingParent` reject on the
        // HTTPRoute (the parent Gateway's listener hostname doesn't
        // intersect the route's hostname filter list) — far from any
        // single-site commit and never surfacing in the emitted YAML.
        // Peer with the sibling
        // [`httproute_hostnames_routes_through_lifted_entrada_hostnames`]
        // pin on the plural-axis half of the DNS-hostname resolver
        // pair — the two pin tests together nail the two-consumer
        // coherence discipline the pair-invariant `hostnames() ==
        // vec![hostname()]` (pinned in
        // [`caixa_core::aplicacao::tests::hostnames_returns_singleton_of_hostname_accessor`])
        // encodes. Peer discipline with the sibling
        // [`httproute_path_list_routes_through_lifted_entrada_resolved_paths`]
        // pin on the sibling per-`:entrada` path-list resolver axis.
        for host in ["checkout.quero.cloud", "shop.pleme.dev", "app.example.io"] {
            let mut caixa = aplicacao_caixa();
            let expected = caixa
                .entrada
                .as_mut()
                .map(|e| {
                    e.host = host.into();
                    e.hostname().to_string()
                })
                .expect("aplicacao_caixa carries a typed `:entrada` block");
            let docs = gateway_routes(&caixa).unwrap();
            let gateway = find_by_kind(&docs, GATEWAY_API_KIND_GATEWAY)
                .expect("Gateway present under every :entrada permutation");
            let listener = kube_spec_field(gateway, GATEWAY_API_KEY_LISTENERS)
                .and_then(|l| l.as_sequence())
                .and_then(|s| s.first())
                .expect("first listener present");
            let emitted = listener
                .get(GATEWAY_API_KEY_HOSTNAME)
                .and_then(|h| h.as_str())
                .expect("Gateway listener carries a hostname scalar");
            assert_eq!(
                emitted, expected,
                "Gateway per-listener singular `hostname:` scalar must \
                 render `Entrada::hostname()` verbatim — drift here \
                 means the emitter no longer routes through the \
                 substrate-primitive typed dispatch and a future \
                 hostname-resolution axis (per-cluster :alt-hosts \
                 overlay, SNI fan-out) would silently disagree with \
                 the plural sibling. Input host: {host:?}"
            );
        }
    }

    #[test]
    fn httproute_hostnames_routes_through_lifted_entrada_hostnames() {
        // Cross-crate pin: the per-Aplicacao HTTPRoute's plural
        // `spec.hostnames[]` filter list must render exactly
        // [`caixa_core::Entrada::hostnames`]'s typed dispatch on the
        // substrate primitive — one entry per resolved hostname, in
        // the resolver's authored order. Pins that a future renderer-
        // side detour that re-inlined the `vec![entrada.host.
        // clone()]` construction (or reordered / deduped / dropped
        // resolver-declared hostnames) surfaces at caixa-mesh build
        // time rather than at cluster-apply time as an
        // `Accepted:False/NoMatchingParent` reject.
        //
        // Peer with the sibling
        // [`gateway_listener_hostname_routes_through_lifted_entrada_hostname`]
        // pin on the singular-axis half — the two pin tests together
        // nail the two-consumer coherence discipline the pair-invariant
        // `hostnames() == vec![hostname()]` (pinned in
        // [`caixa_core::aplicacao::tests::hostnames_returns_singleton_of_hostname_accessor`])
        // encodes.
        for host in ["checkout.quero.cloud", "shop.pleme.dev", "app.example.io"] {
            let mut caixa = aplicacao_caixa();
            let expected: Vec<String> = caixa
                .entrada
                .as_mut()
                .map(|e| {
                    e.host = host.into();
                    e.hostnames().into_iter().map(String::from).collect()
                })
                .expect("aplicacao_caixa carries a typed `:entrada` block");
            let docs = gateway_routes(&caixa).unwrap();
            let route = find_by_kind(&docs, GATEWAY_API_KIND_HTTP_ROUTE)
                .expect("HTTPRoute present under every :entrada permutation");
            let hostnames = kube_spec_field(route, GATEWAY_API_KEY_HOSTNAMES)
                .and_then(|h| h.as_sequence())
                .expect("HTTPRoute.spec.hostnames[] present");
            let emitted: Vec<String> = hostnames
                .iter()
                .map(|v| {
                    v.as_str()
                        .expect("each HTTPRoute.spec.hostnames[] entry is a scalar string")
                        .to_string()
                })
                .collect();
            assert_eq!(
                emitted, expected,
                "HTTPRoute per-route plural `spec.hostnames[]` list \
                 must render `Entrada::hostnames()` verbatim — drift \
                 here means the emitter no longer routes through the \
                 substrate-primitive typed dispatch and a future \
                 hostname-resolution axis (per-cluster :alt-hosts \
                 overlay, SNI fan-out) would silently disagree \
                 between caixa-core and caixa-mesh on which hostname \
                 set a given `:entrada` block resolves to. Input \
                 host: {host:?}"
            );
        }
    }

    #[test]
    fn gateway_listener_hostname_and_httproute_hostnames_pair_invariant_at_emit_site() {
        // The pair-invariant cross-crate pin: the singular Gateway
        // listener `hostname:` filter and the plural HTTPRoute
        // `spec.hostnames[]` filter list at the same [`gateway_routes`]
        // emit site must project as the substrate-canonical pair
        // `hostnames == vec![hostname]` — the invariant
        // [`caixa_core::Entrada`] pins at the typed-primitive level
        // (see
        // [`caixa_core::aplicacao::tests::hostnames_returns_singleton_of_hostname_accessor`])
        // must reach every per-Aplicacao emit site by construction.
        // Pins that any future renderer-side detour that broke the
        // singular-plural coherence (an accidental prefix substitution
        // on one axis, a trailing-`.` FQDN normalization on the peer
        // that didn't land on the peer axis, a wildcard-prefix SNI
        // fan-out overlay that authored only one axis) surfaces at
        // caixa-mesh build time rather than at cluster-apply time as
        // an `Accepted:False/NoMatchingParent` reject far from any
        // single-site commit. Peer discipline with the two singular /
        // plural pin tests immediately above.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let gateway = find_by_kind(&docs, GATEWAY_API_KIND_GATEWAY).expect("Gateway present");
        let listener_hostname = kube_spec_field(gateway, GATEWAY_API_KEY_LISTENERS)
            .and_then(|l| l.as_sequence())
            .and_then(|s| s.first())
            .and_then(|l| l.get(GATEWAY_API_KEY_HOSTNAME))
            .and_then(|h| h.as_str())
            .expect("Gateway listener carries a hostname scalar")
            .to_string();
        let route = find_by_kind(&docs, GATEWAY_API_KIND_HTTP_ROUTE).expect("HTTPRoute present");
        let route_hostnames: Vec<String> = kube_spec_field(route, GATEWAY_API_KEY_HOSTNAMES)
            .and_then(|h| h.as_sequence())
            .expect("HTTPRoute.spec.hostnames[] present")
            .iter()
            .map(|v| {
                v.as_str()
                    .expect("each HTTPRoute.spec.hostnames[] entry is a scalar string")
                    .to_string()
            })
            .collect();
        assert_eq!(
            route_hostnames,
            vec![listener_hostname.clone()],
            "The Gateway listener singular `hostname:` filter and the \
             HTTPRoute plural `spec.hostnames[]` filter list must \
             project as the pair `hostnames == vec![hostname]` at the \
             gateway_routes emit site — Gateway API v1.x conformance \
             requires the HTTPRoute's hostname filter to intersect \
             the parent listener's hostname; drift breaks that at \
             cluster-apply time. Emitted listener_hostname: {:?}, \
             route_hostnames: {:?}",
            listener_hostname,
            route_hostnames,
        );
    }

    #[test]
    fn httproute_name_composer_destination_arg_routes_through_lifted_entrada_destination() {
        // Cross-crate pin: the per-Aplicacao HTTPRoute's `metadata.name`
        // discriminator arg must render exactly
        // [`caixa_core::Entrada::destination`]'s typed dispatch on the
        // substrate primitive — not from an open-coded `entrada.para`
        // field access that could silently disagree with the peer per-
        // rule `backendRefs[0].name` axis on future extensions of the
        // `:entrada` slot to a multi-destination author surface. Drift
        // here would break the operator-side
        // `kubectl get httproute -n tatara-system <aplicacao>-<destination>`
        // grep-by-name lookup encoding — a route whose `metadata.name`
        // names one destination but whose `backendRefs[]` reach a
        // sibling Servico would silently drop every external
        // `:entrada` flow at the gateway. Peer with the sibling
        // [`httproute_backend_ref_name_routes_through_lifted_entrada_destination`]
        // pin on the per-rule backend-name half of the two-consumer
        // coherence axis.
        for para in ["cart", "catalog", "payment"] {
            let mut caixa = aplicacao_caixa();
            // Hoist the parent-Caixa's `:nome` off the typed
            // [`caixa_core::Caixa::nome`] accessor into a local before
            // the `&mut caixa.entrada` mutation below so both projections
            // (the peer `caixa.entrada.as_mut()` borrow inside the block
            // and the aplicacao-name arg of the substrate-canonical
            // [`caixa_core::gateway_api_http_route_name`] composer) reach
            // through disjoint borrows — the accessor's `&self` shape
            // can't coexist with the same-scope `&mut caixa.entrada`
            // borrow directly, but a hoisted `String` clone of its
            // return keeps the emit-side pin routed through the typed
            // dispatch on the substrate primitive.
            let nome = caixa.nome().to_string();
            let (expected_composed_name, expected_destination) = {
                let entrada = caixa
                    .entrada
                    .as_mut()
                    .expect("aplicacao_caixa carries a typed `:entrada` block");
                entrada.para = para.into();
                (
                    gateway_api_http_route_name(&nome, entrada.destination()),
                    entrada.destination().to_string(),
                )
            };
            let docs = gateway_routes(&caixa).unwrap();
            let route = find_by_kind(&docs, GATEWAY_API_KIND_HTTP_ROUTE)
                .expect("HTTPRoute present under every :entrada :para permutation");
            let emitted = kube_name(route).expect("HTTPRoute metadata.name scalar present");
            assert_eq!(
                emitted, expected_composed_name,
                "HTTPRoute `metadata.name` must equal \
                 `gateway_api_http_route_name(caixa.nome, \
                 entrada.destination())` verbatim — drift means the \
                 composer no longer routes through the substrate-\
                 primitive typed dispatch. Input :entrada :para: \
                 {para:?}, expected destination: {expected_destination:?}"
            );
        }
    }

    #[test]
    fn httproute_backend_ref_name_routes_through_lifted_entrada_destination() {
        // Cross-crate pin: the per-Aplicacao HTTPRoute's per-rule
        // `backendRefs[0].name` axis must render exactly
        // [`caixa_core::Entrada::destination`]'s typed dispatch on the
        // substrate primitive — not from an open-coded
        // `entrada.para.clone()` field access. Drift here would break
        // Gateway API v1.x conformance: the `backendRefs[].name` must
        // name a K8s Service in the same namespace as the parent
        // Gateway; a `backendRef` that silently references a peer
        // Servico's Service (because the emitter re-inlined the field
        // access) drops every external `:entrada` flow with the
        // destination-drift root cause invisible in the emitted YAML.
        // Peer with the sibling
        // [`httproute_name_composer_destination_arg_routes_through_lifted_entrada_destination`]
        // pin on the `metadata.name` discriminator half of the
        // two-consumer coherence axis — the two pin tests together
        // nail the two-consumer coherence discipline the pair-invariant
        // `metadata.name == "<caixa.nome>-<destination>"` /
        // `backendRefs[0].name == destination` encodes.
        for para in ["cart", "catalog", "payment"] {
            let mut caixa = aplicacao_caixa();
            let expected = {
                let entrada = caixa
                    .entrada
                    .as_mut()
                    .expect("aplicacao_caixa carries a typed `:entrada` block");
                entrada.para = para.into();
                entrada.destination().to_string()
            };
            let docs = gateway_routes(&caixa).unwrap();
            let route = find_by_kind(&docs, GATEWAY_API_KIND_HTTP_ROUTE)
                .expect("HTTPRoute present under every :entrada :para permutation");
            let backend = kube_spec_field(route, KUBE_KEY_RULES)
                .and_then(|r| r.as_sequence())
                .and_then(|s| s.first())
                .and_then(|r| r.get(GATEWAY_API_KEY_BACKEND_REFS))
                .and_then(|b| b.as_sequence())
                .and_then(|s| s.first())
                .expect("first HTTPRoute rule's first backendRef present");
            let emitted = backend
                .get(GATEWAY_API_KEY_NAME)
                .and_then(|n| n.as_str())
                .expect("backendRef name scalar present");
            assert_eq!(
                emitted, expected,
                "HTTPRoute per-rule `backendRefs[0].name` must render \
                 `Entrada::destination()` verbatim — drift here means \
                 the emitter no longer routes through the substrate-\
                 primitive typed dispatch and the per-rule backend \
                 would silently disagree with the `metadata.name` \
                 discriminator on which destination Servico the \
                 ingress fronts. Input :entrada :para: {para:?}"
            );
        }
    }

    #[test]
    fn httproute_name_and_backend_ref_name_destination_pair_invariant_at_emit_site() {
        // The pair-invariant cross-crate pin: the HTTPRoute's
        // `metadata.name` discriminator and the per-rule
        // `backendRefs[0].name` axis at the same [`gateway_routes`]
        // emit site must project as the substrate-canonical pair
        // `metadata.name == gateway_api_http_route_name(caixa.nome,
        // backendRefs[0].name)` — the invariant the lifted
        // [`caixa_core::Entrada::destination`] typed accessor pins at
        // the substrate-primitive level. Pins that any future
        // renderer-side detour that broke the two-consumer coherence
        // (an accidental namespace-prefix rewrite on one axis, a
        // per-cluster suffix stamp on the peer, a weighted-canary
        // overlay that authored only one axis) surfaces at caixa-mesh
        // build time rather than at cluster-apply time — an HTTPRoute
        // whose `metadata.name` names one Servico but whose
        // `backendRefs[]` reach a peer silently drops external
        // `:entrada` flows and the destination-drift root cause is
        // invisible in the emitted YAML. Peer discipline with the two
        // singular / plural pin tests immediately above and the
        // sibling `gateway_listener_hostname_and_httproute_hostnames_
        // pair_invariant_at_emit_site` pin on the DNS-hostname axis.
        let caixa = aplicacao_caixa();
        let docs = gateway_routes(&caixa).unwrap();
        let route = find_by_kind(&docs, GATEWAY_API_KIND_HTTP_ROUTE).expect("HTTPRoute present");
        let route_name = kube_name(route)
            .expect("HTTPRoute metadata.name scalar present")
            .to_string();
        let backend_name = kube_spec_field(route, KUBE_KEY_RULES)
            .and_then(|r| r.as_sequence())
            .and_then(|s| s.first())
            .and_then(|r| r.get(GATEWAY_API_KEY_BACKEND_REFS))
            .and_then(|b| b.as_sequence())
            .and_then(|s| s.first())
            .and_then(|b| b.get(GATEWAY_API_KEY_NAME))
            .and_then(|n| n.as_str())
            .expect("first HTTPRoute rule's first backendRef.name scalar present")
            .to_string();
        assert_eq!(
            route_name,
            gateway_api_http_route_name(caixa.nome(), &backend_name),
            "The HTTPRoute `metadata.name` discriminator and the per-\
             rule `backendRefs[0].name` axis must project as the pair \
             `metadata.name == gateway_api_http_route_name(caixa.nome, \
             backendRefs[0].name)` at the gateway_routes emit site — \
             Gateway API v1.x conformance requires the operator-side \
             `kubectl get httproute -n <namespace> <aplicacao>-<destination>` \
             grep-by-name lookup and the per-rule backend service reach \
             to name the same destination Servico; drift breaks that \
             lookup encoding at cluster-apply time. Emitted route_name: \
             {route_name:?}, backend_name: {backend_name:?}"
        );
    }

    #[test]
    fn httproute_backend_ref_port_routes_through_lifted_port_for_destination_resolver() {
        // Cross-crate drift-detection pin: the per-Aplicacao HTTPRoute's
        // per-rule `backendRefs[0].port` axis in [`gateway_routes`] now
        // routes through the lifted
        // [`caixa_core::AplicacaoSpec::port_for_destination`] typed
        // dispatch (peer with the sibling
        // `cnp_l4_port_routes_through_lifted_port_for_destination_resolver`
        // pin on the per-`(:de, :para)` CNP L4-port axis — the two per-
        // Aplicacao renderers that reach for a per-destination Servico
        // TCP port scalar both key off exactly one typed dispatch on the
        // substrate primitive now). Pin the resolver-shaped rule at the
        // emit-side path across a non-default `:entrada :port` scalar
        // (`8443`, exercising a hypothetical HTTPS-by-default trajectory
        // for the destination Servico's listener port) and a `:para`
        // permutation, so a future renderer-side detour that re-inlined
        // the `entrada.port` field access at this call site would surface
        // as a caixa-mesh build-time test failure — the two consumer
        // paths on the per-Aplicacao L4 port axis (the resolver's own
        // typed dispatch at caixa-core, this renderer's emit-side reach
        // for it) must agree at every point on the port scalar the
        // emitted HTTPRoute renders, per the MESH-COMPOSITION §V "one
        // identity layer, one data plane" invariant that the CNP L4
        // whitelist port and the HTTPRoute `backendRefs[].port` name the
        // same destination Servico's listener.
        for (para, port) in [
            ("cart", 8080u16),
            ("catalog", 8443u16),
            ("payment", 9090u16),
        ] {
            let mut caixa = aplicacao_caixa();
            let expected_port = {
                let entrada = caixa
                    .entrada
                    .as_mut()
                    .expect("aplicacao_caixa carries a typed `:entrada` block");
                entrada.para = para.into();
                entrada.port = port;
                let spec =
                    typed_view(&caixa).expect("aplicacao_caixa fixture must be a valid Aplicacao");
                let entrada_ref = spec.entrada().expect("entrada present in spec");
                spec.port_for_destination(entrada_ref.destination())
            };
            let docs = gateway_routes(&caixa).unwrap();
            let route = find_by_kind(&docs, GATEWAY_API_KIND_HTTP_ROUTE)
                .expect("HTTPRoute present under every :entrada :para permutation");
            let emitted_port = kube_spec_field(route, KUBE_KEY_RULES)
                .and_then(|r| r.as_sequence())
                .and_then(|s| s.first())
                .and_then(|r| r.get(GATEWAY_API_KEY_BACKEND_REFS))
                .and_then(|b| b.as_sequence())
                .and_then(|s| s.first())
                .and_then(|b| b.get(KUBE_KEY_PORT))
                .and_then(|p| p.as_u64())
                .expect("first HTTPRoute rule's first backendRef.port scalar present");
            assert_eq!(
                emitted_port,
                u64::from(expected_port),
                "HTTPRoute per-rule `backendRefs[0].port` must render \
                 `AplicacaoSpec::port_for_destination(entrada.destination())` \
                 verbatim — drift here means the emitter no longer routes \
                 through the substrate-primitive typed dispatch and the \
                 per-rule backend port would silently disagree with the \
                 peer CNP L4 whitelist port on which destination Servico's \
                 listener the ingress fronts. Input :entrada :para: \
                 {para:?}, :entrada :port: {port}"
            );
        }
    }

    #[test]
    fn httproute_backend_ref_port_and_cnp_l4_port_share_port_for_destination_resolver_at_emit_site()
    {
        // The two-renderer pair-invariant cross-crate pin: the per-
        // Aplicacao HTTPRoute's per-rule `backendRefs[0].port` axis (this
        // module's [`gateway_routes`] emit-side path) and the peer
        // per-`(:de, :para)` `CiliumNetworkPolicy` `toPorts[0].ports[0]
        // .port` axis (this module's [`cilium_network_policies`] emit-
        // side path) at the same fixture must both project to
        // [`caixa_core::AplicacaoSpec::port_for_destination`]'s scalar
        // for the entrada apex destination — the substrate-canonical
        // invariant the lifted resolver pins at the substrate-primitive
        // level and both consumers now reach for by construction. A
        // future renderer-side detour that broke the two-consumer
        // coherence (an accidental per-cluster port stamp on one
        // renderer, a hardcoded `DEFAULT_SERVICO_PORT` re-inline on the
        // peer, an mTLS-by-default overlay that authored only one axis)
        // surfaces at caixa-mesh build time rather than at cluster-apply
        // time — a two-renderer split silently blackholes every external
        // `:entrada` flow at the eBPF data plane far from the source
        // `caixa.lisp` with no field naming the port-drift root cause in
        // the emitted YAML. Peer discipline with the sibling
        // `httproute_name_and_backend_ref_name_destination_pair_invariant_at_emit_site`
        // pin on the per-`:entrada` destination-Servico scalar axis and
        // the `gateway_listener_hostname_and_httproute_hostnames_pair_invariant_at_emit_site`
        // pin on the DNS-hostname axis — same two-consumer coherence
        // discipline the M3 mesh contract lifts encode.
        let caixa = aplicacao_caixa();
        let spec = typed_view(&caixa).expect("aplicacao_caixa fixture must be a valid Aplicacao");
        let apex_destination = spec
            .entrada()
            .expect("aplicacao_caixa carries a typed `:entrada` block")
            .destination()
            .to_string();
        let expected_port = spec.port_for_destination(&apex_destination);

        let gateway_docs = gateway_routes(&caixa).unwrap();
        let route =
            find_by_kind(&gateway_docs, GATEWAY_API_KIND_HTTP_ROUTE).expect("HTTPRoute present");
        let httproute_port = kube_spec_field(route, KUBE_KEY_RULES)
            .and_then(|r| r.as_sequence())
            .and_then(|s| s.first())
            .and_then(|r| r.get(GATEWAY_API_KEY_BACKEND_REFS))
            .and_then(|b| b.as_sequence())
            .and_then(|s| s.first())
            .and_then(|b| b.get(KUBE_KEY_PORT))
            .and_then(|p| p.as_u64())
            .expect("HTTPRoute backendRef.port scalar present");
        assert_eq!(
            httproute_port,
            u64::from(expected_port),
            "HTTPRoute `backendRefs[0].port` must equal \
             `spec.port_for_destination(entrada.destination())` at the \
             gateway_routes emit site — this is one half of the two-\
             renderer pair-invariant on the per-destination Servico L4 \
             port axis."
        );

        // The peer CNP `toPorts[0].ports[0].port` axis must render the
        // same resolver's answer for each destination. The per-`(:de, :para)`
        // CNP naming scheme is `<caixa.nome>-<de>-to-<para>`; extract the
        // `<para>` and confirm every emitted CNP's L4 port scalar equals
        // `spec.port_for_destination(<destination>)`.
        let policies = cilium_network_policies(&caixa).unwrap();
        for policy in &policies {
            let cnp_name = kube_name(policy)
                .expect("every CNP has a metadata.name")
                .to_string();
            let Some(destination) = cnp_name.split("-to-").nth(1) else {
                continue;
            };
            let cnp_port = kube_spec_field(policy, CILIUM_KEY_INGRESS)
                .and_then(|i| i.as_sequence())
                .and_then(|s| s.first())
                .and_then(|i| i.get(CILIUM_KEY_TO_PORTS))
                .and_then(|t| t.as_sequence())
                .and_then(|s| s.first())
                .and_then(|tp| tp.get(CILIUM_KEY_PORTS))
                .and_then(|p| p.as_sequence())
                .and_then(|s| s.first())
                .and_then(|p| p.get(KUBE_KEY_PORT))
                .and_then(|v| v.as_str())
                .expect("CNP toPorts[0].ports[0].port present");
            assert_eq!(
                cnp_port,
                spec.port_for_destination(destination).to_string(),
                "CNP {cnp_name:?} toPorts[0].ports[0].port must equal \
                 `spec.port_for_destination({destination:?})` — drift here \
                 means the CNP emit-side path re-inlined the port \
                 resolution rule and would silently disagree with the \
                 HTTPRoute peer on the shared destination Servico's \
                 listener port."
            );
        }
    }

    #[test]
    fn httproute_routes_to_entrada_para() {
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let route = find_by_kind(&docs, GATEWAY_API_KIND_HTTP_ROUTE).unwrap();
        let backend = kube_spec_field(route, KUBE_KEY_RULES)
            .and_then(|r| r.as_sequence())
            .and_then(|s| s.first())
            .and_then(|r| r.get(GATEWAY_API_KEY_BACKEND_REFS))
            .and_then(|b| b.as_sequence())
            .and_then(|s| s.first())
            .unwrap();
        assert_eq!(
            backend.get(GATEWAY_API_KEY_NAME).and_then(|n| n.as_str()),
            Some("cart")
        );
        assert_eq!(
            backend.get(KUBE_KEY_PORT).and_then(|p| p.as_u64()),
            Some(8080)
        );
    }

    #[test]
    fn gateway_skips_when_no_entrada() {
        let mut c = aplicacao_caixa();
        c.entrada = None;
        let docs = gateway_routes(&c).unwrap();
        assert!(docs.is_empty());
    }

    #[test]
    fn cilium_policy_carries_canonical_kube_skeleton() {
        // Pin that the kube_resource_skeleton lift preserves the exact
        // apiVersion + kind + metadata.{name, namespace, labels} shape
        // every CNP carried before the lift. Drift here is invisible at
        // runtime (Cilium tolerates extra/missing keys quietly), so
        // structural pinning is the only signal a refactor would
        // accidentally drop apiVersion or shift the metadata block.
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        for p in &policies {
            // Per-CNP CRD-group/version equality-wrap through the
            // substrate-primitive [`kube_api_version_is`] pinned
            // predicate — structural peer to the sibling
            // [`kube_kind_is`] equality-wrap below on the top-level
            // `(apiVersion, kind)` canonical discriminator-pair the K8s
            // API-machinery threads through every controller /
            // API-server admission decision.
            assert!(kube_api_version_is(p, "cilium.io/v2"));
            // Per-CNP discriminator readback through the
            // substrate-primitive [`kube_kind`] pinned accessor.
            assert_eq!(kube_kind(p), Some(CILIUM_KIND_NETWORK_POLICY));
            // Route the per-CNP `metadata:` sub-mapping readback
            // through the substrate-primitive [`kube_metadata`] pinned
            // accessor rather than the raw two-hop
            // `.get(KUBE_KEY_METADATA).and_then(|m| m.as_mapping())`
            // navigation — sibling convergence to the peer
            // `gateway_carries_canonical_kube_skeleton_without_labels`
            // + `httproute_carries_canonical_kube_skeleton_without_labels`
            // + `cilium_policy_metadata_iterates_alphabetically`
            // per-CR metadata-sub-mapping-readback sites that fold
            // onto the same substrate accessor. The extracted
            // `metadata` sub-mapping stays bound for the sibling
            // `.len()` / `KUBE_KEY_NAME` / `KUBE_KEY_LABELS` per-sub-
            // key probes below that need the sub-view for shape
            // assertions the per-axis accessor does not close.
            let metadata = kube_metadata(p).expect("metadata mapping");
            // metadata carries name + namespace + labels (3 keys) — no
            // accidental extras leak past the skeleton lift.
            assert_eq!(metadata.len(), 3);
            assert!(
                metadata
                    .get(KUBE_KEY_NAME)
                    .and_then(|v| v.as_str())
                    .is_some()
            );
            // Route the per-CNP `metadata.namespace` readback through
            // the substrate-primitive [`kube_namespace`] pinned
            // accessor rather than the raw two-hop
            // `metadata.get(KUBE_KEY_NAMESPACE).and_then(|v| v.as_str())`
            // navigation on the already-extracted `metadata` sub-view
            // — sibling convergence to the caixa-flux
            // `programs_yaml_entry` production readback + the
            // `cluster_bundle` kustomization.yaml pin that fold onto
            // the same substrate accessor. The extracted `metadata`
            // sub-mapping stays for the sibling `.len()` /
            // `KUBE_KEY_LABELS` probes above / below that need the
            // sub-view for shape assertions the per-axis accessor does
            // not close.
            assert_eq!(kube_namespace(p), Some(DEFAULT_NAMESPACE));
            assert!(metadata.get(KUBE_KEY_LABELS).is_some());
        }
    }

    #[test]
    fn gateway_carries_canonical_kube_skeleton_without_labels() {
        // Pin that Gateway emits apiVersion + kind + metadata.{name,
        // namespace} — and *not* metadata.labels (the empty-labels-skip
        // semantic of kube_resource_skeleton; Gateway does not need
        // per-Aplicacao label grouping at the K8s-resource axis today).
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let gateway = find_by_kind(&docs, GATEWAY_API_KIND_GATEWAY).expect("Gateway present");
        assert!(kube_api_version_is(gateway, "gateway.networking.k8s.io/v1",));
        // Route the per-Gateway `metadata:` sub-mapping readback
        // through the substrate-primitive [`kube_metadata`] pinned
        // accessor — sibling convergence to the peer
        // `cilium_policy_carries_canonical_kube_skeleton` +
        // `httproute_carries_canonical_kube_skeleton_without_labels` +
        // `cilium_policy_metadata_iterates_alphabetically` per-CR
        // metadata-sub-mapping-readback sites that fold onto the same
        // substrate accessor.
        let metadata = kube_metadata(gateway).expect("metadata mapping");
        // Exactly 2 metadata keys (name + namespace) — labels absent.
        assert_eq!(metadata.len(), 2);
        assert_eq!(
            metadata.get(KUBE_KEY_NAME).and_then(|v| v.as_str()),
            Some("checkout")
        );
        // Route the per-Gateway `metadata.namespace` readback through
        // the substrate-primitive [`kube_namespace`] pinned accessor
        // rather than the raw two-hop
        // `metadata.get(KUBE_KEY_NAMESPACE).and_then(|v| v.as_str())`
        // navigation on the already-extracted `metadata` sub-view —
        // peer convergence to the sibling
        // `cilium_policy_carries_canonical_kube_skeleton` per-CNP
        // readback that migrates through the same accessor.
        assert_eq!(kube_namespace(gateway), Some(DEFAULT_NAMESPACE));
        assert!(
            metadata.get(KUBE_KEY_LABELS).is_none(),
            "Gateway must not carry metadata.labels (empty-labels-skip \
             contract from kube_resource_skeleton)"
        );
    }

    #[test]
    fn httproute_carries_canonical_kube_skeleton_without_labels() {
        // Same shape pin for HTTPRoute (the second lift site in
        // gateway_routes). Same empty-labels-skip semantic — the
        // route's parent-Gateway-association lives at spec.parentRefs,
        // not at metadata.labels.
        //
        // The `metadata.name` byte-shape probe now consults the lifted
        // [`gateway_api_http_route_name`] composer rather than a
        // verbatim `Some("checkout-cart")` literal so a future
        // per-Aplicacao Gateway API per-CR name-encoding rebrand
        // (which lands at the composer's caixa-core definition site)
        // reaches this probe by construction — pinning the composer's
        // output prevents the emitter and this probe from silently
        // splitting on any rebrand. Peer to the sibling
        // `cilium_fans_same_de_para_edges_into_one_policy` probe
        // pinning the CNP `metadata.name` via
        // [`cilium_network_policy_name`] on the same shared
        // "aplicacao-prefixed sub-identity" discipline.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let route = find_by_kind(&docs, GATEWAY_API_KIND_HTTP_ROUTE).expect("HTTPRoute present");
        assert!(kube_api_version_is(route, "gateway.networking.k8s.io/v1",));
        // Route the per-HTTPRoute `metadata:` sub-mapping readback
        // through the substrate-primitive [`kube_metadata`] pinned
        // accessor — sibling convergence to the peer per-CR
        // metadata-sub-mapping-readback sites that fold onto the same
        // substrate accessor.
        let metadata = kube_metadata(route).expect("metadata mapping");
        assert_eq!(metadata.len(), 2);
        assert_eq!(
            metadata.get(KUBE_KEY_NAME).and_then(|v| v.as_str()),
            Some(gateway_api_http_route_name("checkout", "cart").as_str())
        );
        assert!(metadata.get(KUBE_KEY_LABELS).is_none());
    }

    #[test]
    fn gateway_routes_gateway_uses_lifted_gateway_api_api_version() {
        // Fail-before-pass-after pin parsing the rendered `Gateway`
        // document and asserting its top-level `apiVersion` axis
        // equals the lifted [`caixa_core::GATEWAY_API_API_VERSION`]
        // constant by value (not just by the canonical-literal
        // string, which the sibling
        // `gateway_carries_canonical_kube_skeleton_without_labels`
        // pin already enforces). The two pins form the bridge-arm
        // pair: this pin trips on drift between the renderer-side
        // threading and the lifted const, the sibling pin trips on
        // drift between the lifted const and the canonical literal,
        // and the
        // [`gateway_api_api_version_re_export_points_at_caixa_core_canonical`]
        // pin trips on drift between this crate's re-export and the
        // caixa-core canonical declaration — together they close the
        // three-arm drift footgun the inline-literal-pair-across-two-
        // skeleton-calls shape carried by construction. Peer to
        // `caixa_flux::tests::cluster_bundle_gitrepository_uses_lifted_flux_api_version`
        // / `cluster_bundle_helmrelease_uses_lifted_flux_api_version`
        // / `cluster_bundle_kustomization_uses_lifted_flux_api_version`
        // on the sibling Flux v2 controller-triplet lift trajectory.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let gateway = find_by_kind(&docs, GATEWAY_API_KIND_GATEWAY).expect("Gateway present");
        assert!(
            kube_api_version_is(gateway, caixa_core::GATEWAY_API_API_VERSION),
            "Gateway's top-level apiVersion must equal the lifted \
             caixa_core::GATEWAY_API_API_VERSION by value — drift here \
             is the canonical footgun this lift closes"
        );
    }

    #[test]
    fn gateway_routes_gateway_uses_lifted_gateway_api_kind_gateway() {
        // Fail-before-pass-after pin parsing the rendered `Gateway`
        // document and asserting its top-level `kind` axis equals the
        // lifted [`caixa_core::GATEWAY_API_KIND_GATEWAY`] constant by
        // value (not just by the canonical-literal string, which the
        // sibling `gateway_carries_canonical_kube_skeleton_without_labels`
        // pin already enforces). The two pins form the bridge-arm pair:
        // this pin trips on drift between the renderer-side threading
        // and the lifted const, the sibling pin trips on drift between
        // the lifted const and the canonical literal, and the
        // [`gateway_api_kind_gateway_re_export_points_at_caixa_core_canonical`]
        // pin trips on drift between this crate's re-export and the
        // caixa-core canonical declaration — together the three arms
        // (canonical-string pin, lifted-uses pin, re-export-identity
        // pin) close the three-arm drift footgun the inline-literal-
        // across-the-production-skeleton-call-plus-test-fixture shape
        // carried by construction. Peer to
        // [`gateway_routes_gateway_uses_lifted_gateway_api_api_version`]
        // on the sibling Gateway-API-CRD-apiVersion-axis lift trajectory
        // — begins the per-Gateway-API-CRD kind+apiVersion lifted-uses
        // pin pair the renderer's exit threading through the lifted
        // [`GATEWAY_API_API_VERSION`] + [`GATEWAY_API_KIND_GATEWAY`]
        // pair demands. Peer to
        // [`cilium_network_policies_use_lifted_cilium_kind_network_policy`]
        // on the sibling Cilium-CRD-kind-axis lift trajectory.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let gateway = find_by_kind(&docs, GATEWAY_API_KIND_GATEWAY).expect("Gateway present");
        // Readback through the substrate-primitive [`kube_kind`] pinned
        // accessor — same three-arity closure the sibling
        // [`find_by_kind`] navigator + [`kube_kind_is`] predicate reach
        // through on the top-level `kind:` discriminator axis.
        assert_eq!(
            kube_kind(gateway),
            Some(caixa_core::GATEWAY_API_KIND_GATEWAY),
            "Gateway's top-level kind must equal the lifted \
             caixa_core::GATEWAY_API_KIND_GATEWAY by value — drift here \
             is the canonical footgun this lift closes"
        );
    }

    #[test]
    fn gateway_routes_httproute_uses_lifted_gateway_api_kind_http_route() {
        // Fail-before-pass-after pin parsing the rendered `HTTPRoute`
        // document and asserting its top-level `kind` axis equals the
        // lifted [`caixa_core::GATEWAY_API_KIND_HTTP_ROUTE`] constant
        // by value (not just by the canonical-literal string, which the
        // sibling `httproute_carries_canonical_kube_skeleton_without_labels`
        // pin already enforces). The two pins form the bridge-arm pair:
        // this pin trips on drift between the renderer-side threading
        // and the lifted const, the sibling pin trips on drift between
        // the lifted const and the canonical literal, and the
        // [`gateway_api_kind_http_route_re_export_points_at_caixa_core_canonical`]
        // pin trips on drift between this crate's re-export and the
        // caixa-core canonical declaration — together the three arms
        // (canonical-string pin, lifted-uses pin, re-export-identity
        // pin) close the three-arm drift footgun the inline-literal-
        // across-the-production-skeleton-call-plus-test-fixture shape
        // carried by construction. Peer to
        // [`gateway_routes_httproute_uses_lifted_gateway_api_api_version`]
        // on the sibling Gateway-API-CRD-apiVersion-axis lift trajectory
        // — completes the per-Gateway-API-CRD kind+apiVersion lifted-
        // uses pin pair the renderer's exit threading through the
        // lifted [`GATEWAY_API_API_VERSION`] + [`GATEWAY_API_KIND_HTTP_ROUTE`]
        // pair demands. Peer to
        // [`gateway_routes_gateway_uses_lifted_gateway_api_kind_gateway`]
        // on the sibling parent-Gateway-`kind`-axis lift trajectory —
        // completes the per-Gateway-API-CRD `kind`-axis lifted-uses
        // pin pair across the `(Gateway, HTTPRoute)` pair the renderer
        // emits together.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let route = find_by_kind(&docs, GATEWAY_API_KIND_HTTP_ROUTE).expect("HTTPRoute present");
        // Readback through the substrate-primitive [`kube_kind`] pinned
        // accessor — the sibling per-Gateway pin
        // (`gateway_routes_gateway_uses_lifted_gateway_api_kind_gateway`)
        // reaches through the same accessor on the paired-CR axis.
        assert_eq!(
            kube_kind(route),
            Some(caixa_core::GATEWAY_API_KIND_HTTP_ROUTE),
            "HTTPRoute's top-level kind must equal the lifted \
             caixa_core::GATEWAY_API_KIND_HTTP_ROUTE by value — drift here \
             is the canonical footgun this lift closes"
        );
    }

    #[test]
    fn gateway_routes_httproute_uses_lifted_gateway_api_api_version() {
        // Sibling-axis pin to
        // [`gateway_routes_gateway_uses_lifted_gateway_api_api_version`]
        // on the HTTPRoute CRD-group/version axis (the second
        // `kube_resource_skeleton` call site at
        // caixa-mesh/src/lib.rs:496). The K8s SIG-Network Gateway API
        // contract bumps `Gateway`, `HTTPRoute`, `GatewayClass`, and
        // the rest of the per-conformance CRD set as a unit; a future
        // Gateway-API GA promotion on one axis without a coordinated
        // edit on the other would land the rendered `Gateway` /
        // `HTTPRoute` pair pointing at distinct CRD versions, with
        // the per-route attached-policy resolution pipeline never
        // binding at apply time. Peer to the sibling Gateway-axis
        // pin above — together they enforce the per-CRD-axis
        // movement-as-a-unit invariant at the renderer's exit.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let route = find_by_kind(&docs, GATEWAY_API_KIND_HTTP_ROUTE).expect("HTTPRoute present");
        assert!(
            kube_api_version_is(route, caixa_core::GATEWAY_API_API_VERSION),
            "HTTPRoute's top-level apiVersion must equal the lifted \
             caixa_core::GATEWAY_API_API_VERSION by value — drift here \
             is the canonical footgun this lift closes"
        );
    }

    #[test]
    fn gateway_gateway_class_name_uses_lifted_default_gateway_class_name() {
        // Fail-before-pass-after pin parsing the rendered `Gateway`
        // document and asserting its `spec.gatewayClassName` axis equals
        // the lifted [`caixa_core::DEFAULT_GATEWAY_CLASS_NAME`] constant
        // by value — the third arm of the three-arm drift footgun close
        // pattern the prior lifts (`GATEWAY_API_KIND_GATEWAY`,
        // `GATEWAY_API_API_VERSION`) established on the peer
        // Gateway-API-CRD-discriminator axes. The three arms:
        // this pin trips on drift between the renderer-side threading
        // and the lifted const, the sibling
        // `default_gateway_class_name_pins_canonical_value` in caixa-core
        // trips on drift between the lifted const and the canonical
        // literal value, and the
        // [`default_gateway_class_name_re_export_points_at_caixa_core_canonical`]
        // pin trips on drift between this crate's re-export and the
        // caixa-core canonical declaration — together they close the
        // three-arm drift footgun the inline-literal-across-the-
        // production-spec-map-plus-implicit-test-fixture shape carried
        // by construction. Peer to
        // [`gateway_routes_gateway_uses_lifted_gateway_api_kind_gateway`]
        // on the sibling parent-Gateway-`kind`-axis lifted-uses pin —
        // extends the discipline from the CRD-discriminator half of the
        // per-Gateway typed contract onto the controller-choice half.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let gateway = find_by_kind(&docs, GATEWAY_API_KIND_GATEWAY).expect("Gateway present");
        let class_name = kube_spec_str_field(gateway, GATEWAY_API_KEY_GATEWAY_CLASS_NAME)
            .expect("Gateway spec.gatewayClassName present");
        assert_eq!(
            class_name,
            caixa_core::DEFAULT_GATEWAY_CLASS_NAME,
            "Gateway's spec.gatewayClassName must equal the lifted \
             caixa_core::DEFAULT_GATEWAY_CLASS_NAME by value — drift here \
             is the canonical footgun this lift closes"
        );
    }

    #[test]
    fn gateway_routes_gateway_uses_lifted_gateway_api_key_gateway_class_name() {
        // Fail-before-pass-after pin parsing the rendered `Gateway`
        // document via the raw canonical `"gatewayClassName"` KEY
        // literal (not the lifted const, to trip on drift between the
        // renderer-side emitter and the lifted const), then asserting
        // the emitted key IS byte-identical to
        // [`caixa_core::GATEWAY_API_KEY_GATEWAY_CLASS_NAME`]. The three
        // arms: this pin trips on drift between the renderer-side
        // emitter and the lifted const, the sibling
        // `gateway_api_key_gateway_class_name_pins_canonical_value` in
        // caixa-core trips on drift between the lifted const and the
        // canonical literal, and the
        // [`gateway_api_key_gateway_class_name_re_export_points_at_caixa_core_canonical`]
        // pin trips on drift between this crate's re-export and the
        // caixa-core canonical declaration — together they close the
        // three-arm drift footgun the inline-literal-across-the-
        // production-emit-plus-navigation shape carried by construction.
        // Sibling of the peer
        // `gateway_gateway_class_name_uses_lifted_default_gateway_class_name`
        // on the canonical-Gateway-API-`(key, value)`-pair-lifted-uses
        // pin surface this pin closes the KEY half of.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let gateway = find_by_kind(&docs, GATEWAY_API_KIND_GATEWAY).expect("Gateway present");
        let spec = kube_spec(gateway).expect("Gateway spec is a mapping");
        assert!(
            spec.contains_key(caixa_core::GATEWAY_API_KEY_GATEWAY_CLASS_NAME),
            "Gateway spec must carry a key byte-identical to the lifted \
             caixa_core::GATEWAY_API_KEY_GATEWAY_CLASS_NAME — drift here \
             is the canonical footgun this lift closes"
        );
    }

    #[test]
    fn gateway_routes_httproute_uses_lifted_gateway_api_key_hostnames() {
        // Fail-before-pass-after pin parsing the rendered `HTTPRoute`
        // document and asserting its `spec.hostnames[0]` axis is
        // navigable through the lifted
        // [`caixa_core::GATEWAY_API_KEY_HOSTNAMES`] constant by value
        // (and carries the Aplicacao's `:entrada :host` slot as its
        // single element, the same seed the sibling per-`Gateway`
        // per-listener `hostname` axis threads through
        // [`gateway_listener_carries_aplicacao_host`]). Peer to
        // [`gateway_routes_gateway_uses_lifted_gateway_api_kind_gateway`]
        // /
        // [`gateway_routes_httproute_uses_lifted_gateway_api_kind_http_route`]
        // on the sibling Gateway-API-CRD-kind-axis lift trajectory and
        // to
        // [`gateway_routes_gateway_uses_lifted_gateway_api_api_version`]
        // /
        // [`gateway_routes_httproute_uses_lifted_gateway_api_api_version`]
        // on the sibling Gateway-API-CRD-apiVersion-axis lift
        // trajectory — closes the per-Gateway-API-CRD `HTTPRoute` per-
        // route body-axis lifted-uses pin pair across the singular /
        // plural DNS-host discriminator surface (`hostname` at the
        // parent-Gateway per-listener discriminator + `hostnames` at
        // the child HTTPRoute per-route filter list), so both halves of
        // the DNS-host-discriminator convention across the
        // `(Gateway, HTTPRoute)` pair the M3 Aplicacao mesh renderer's
        // external `:entrada` ingress contract emits together now carry
        // one lifted-uses pin apiece. The
        // [`gateway_api_key_hostnames_re_export_points_at_caixa_core_canonical`]
        // pin trips on drift between this crate's re-export and the
        // caixa-core canonical declaration — together the two arms
        // (lifted-uses pin here, re-export-identity pin above) close
        // the drift footgun the inline-literal-at-the-production-
        // skeleton-call shape carried by construction.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let route = find_by_kind(&docs, GATEWAY_API_KIND_HTTP_ROUTE).expect("HTTPRoute present");
        let hostnames = kube_spec_field(route, caixa_core::GATEWAY_API_KEY_HOSTNAMES)
            .and_then(|h| h.as_sequence())
            .expect("HTTPRoute spec.hostnames must be navigable through the lifted constant");
        assert_eq!(
            hostnames.len(),
            1,
            "HTTPRoute spec.hostnames must carry exactly one entry — the \
             typed `:entrada :host` seed"
        );
        assert_eq!(
            hostnames[0].as_str(),
            Some("checkout.quero.cloud"),
            "HTTPRoute spec.hostnames[0] must carry the Aplicacao's \
             `:entrada :host` slot — the same seed the sibling per-`Gateway` \
             per-listener `hostname` axis threads through"
        );
    }

    #[test]
    fn gateway_routes_httproute_uses_lifted_gateway_api_key_matches() {
        // Fail-before-pass-after pin parsing the rendered `HTTPRoute`
        // document and asserting every per-rule route-match container-
        // axis is navigable through the lifted
        // [`caixa_core::GATEWAY_API_KEY_MATCHES`] constant by value
        // (and carries a non-empty per-rule request-selection predicate
        // sequence, one entry per typed `:entrada :paths` path). Peer
        // to
        // [`gateway_routes_httproute_uses_lifted_gateway_api_key_hostnames`]
        // on the sibling Gateway-API-HTTPRoute-body-axis lift
        // trajectory — completes the per-rule top-level-axis lifted-
        // uses pin set (`matches`, `backendRefs`, `timeouts`,
        // `retry`) the `httproute_rule_keys_pin_overlay_position` pin
        // binds against, so every one of the four per-rule top-level
        // axes now carries a lifted-uses pin apiece alongside its
        // sibling re-export-identity pin. The
        // [`gateway_api_key_matches_re_export_points_at_caixa_core_canonical`]
        // pin trips on drift between this crate's re-export and the
        // caixa-core canonical declaration — together the two arms
        // (lifted-uses pin here, re-export-identity pin above) close
        // the drift footgun the inline-literal-at-the-production-
        // per-rule-emitter shape carried by construction.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let rules = httproute_rules(&docs);
        assert!(
            !rules.is_empty(),
            "HTTPRoute must carry at least one rule the per-rule route-match \
             axis nests under"
        );
        for rule in &rules {
            let matches = rule
                .get(caixa_core::GATEWAY_API_KEY_MATCHES)
                .and_then(|m| m.as_sequence())
                .expect(
                    "HTTPRoute per-rule spec.rules[].matches must be navigable \
                     through the lifted constant",
                );
            assert!(
                !matches.is_empty(),
                "per-rule matches sequence must carry at least one entry — the \
                 typed `:entrada :paths` seed",
            );
        }
    }

    #[test]
    fn gateway_routes_httproute_uses_lifted_gateway_api_key_path() {
        // Fail-before-pass-after pin parsing the rendered `HTTPRoute`
        // document and asserting every per-`HTTPRouteMatch` path-matcher
        // container-axis is navigable through the lifted
        // [`caixa_core::GATEWAY_API_KEY_PATH`] constant by value (and
        // carries a non-empty per-match `{type, value}` path-selection
        // predicate mapping, one entry per typed `:entrada :paths`
        // path). Peer to
        // [`gateway_routes_httproute_uses_lifted_gateway_api_key_matches`]
        // on the sibling per-rule route-match container-axis lift
        // trajectory — nests the per-Gateway-API-HTTPRoute-per-rule-
        // body-axis lifted-uses pin set (`matches`, `backendRefs`,
        // `timeouts`, `retry`) one level deeper onto the per-
        // `HTTPRouteMatch` body-axis surface, so the container-axis
        // key beneath the sibling `matches[]` axis now carries a
        // lifted-uses pin alongside its parent-container-axis
        // lifted-uses pin. The
        // [`gateway_api_key_path_re_export_points_at_caixa_core_canonical`]
        // pin trips on drift between this crate's re-export and the
        // caixa-core canonical declaration — together the two arms
        // (lifted-uses pin here, re-export-identity pin above) close
        // the drift footgun the inline-literal-at-the-production-
        // per-match-emitter shape carried by construction.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let rules = httproute_rules(&docs);
        assert!(
            !rules.is_empty(),
            "HTTPRoute must carry at least one rule the per-match path-matcher \
             axis nests under"
        );
        for rule in &rules {
            let matches = rule
                .get(caixa_core::GATEWAY_API_KEY_MATCHES)
                .and_then(|m| m.as_sequence())
                .expect("HTTPRoute per-rule spec.rules[].matches sequence");
            assert!(
                !matches.is_empty(),
                "per-rule matches sequence must carry at least one entry — the \
                 typed `:entrada :paths` seed the per-match path-matcher axis \
                 nests under",
            );
            for m in matches {
                let path = m
                    .get(caixa_core::GATEWAY_API_KEY_PATH)
                    .and_then(|p| p.as_mapping())
                    .expect(
                        "HTTPRoute per-match spec.rules[].matches[].path must be \
                         navigable through the lifted constant",
                    );
                assert!(
                    !path.is_empty(),
                    "per-match path-matcher mapping must carry the typed \
                     `{{type, value}}` path-selection predicate — the Gateway \
                     API v1 HTTPRouteMatch canonical path shape",
                );
            }
        }
    }

    #[test]
    fn cilium_policy_metadata_block_iterates_alphabetically() {
        // The kube_resource_skeleton's render-determinism contract:
        // metadata: block keys appear in alphabetical order (labels,
        // name, namespace) regardless of source-code declaration order.
        // Pinning this at the renderer's exit so a future
        // pretty-printer / round-trip / diff-friendly format depends
        // on the determinism property (mirrors the M2 overlay helper's
        // alphabetical-iteration determinism property — THEORY.md
        // §V.2.7).
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        for p in &policies {
            // Route the per-CNP `metadata:` sub-mapping readback
            // through the substrate-primitive [`kube_metadata`] pinned
            // accessor — sibling convergence to the peer per-CR
            // metadata-sub-mapping-readback sites that fold onto the
            // same substrate accessor. The extracted `metadata` sub-
            // mapping stays bound for the sibling `.iter()` walk that
            // pins the alphabetical-iteration determinism contract
            // this test protects (THEORY.md §V.2.7).
            let metadata = kube_metadata(p).expect("metadata mapping");
            let keys: Vec<&str> = metadata.iter().filter_map(|(k, _)| k.as_str()).collect();
            assert_eq!(
                keys,
                vec![KUBE_KEY_LABELS, KUBE_KEY_NAME, KUBE_KEY_NAMESPACE],
                "metadata block must iterate alphabetically (the kube \
                 skeleton's render-determinism contract)"
            );
        }
    }

    #[test]
    fn render_all_includes_every_artifact_kind() {
        let docs = render_all(&aplicacao_caixa()).unwrap();
        // 3 programs + 2 cilium policies + 1 gateway + 1 httproute = 7
        assert_eq!(docs.len(), 7);
        let kinds: Vec<_> = docs
            .iter()
            .filter_map(|d| {
                d.get(KUBE_KEY_KIND)
                    .and_then(|k| k.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        // programs entries don't carry `kind:`; cilium + gateway docs do.
        assert!(kinds.contains(&CILIUM_KIND_NETWORK_POLICY.to_string()));
        assert!(kinds.contains(&GATEWAY_API_KIND_GATEWAY.to_string()));
        assert!(kinds.contains(&GATEWAY_API_KIND_HTTP_ROUTE.to_string()));
    }

    // ── HTTPRoute :politicas :timeout overlay ────────────────────────────

    fn httproute_rules(docs: &[serde_yaml::Value]) -> Vec<serde_yaml::Value> {
        find_by_kind(docs, GATEWAY_API_KIND_HTTP_ROUTE)
            .and_then(kube_spec)
            .and_then(|s| s.get(KUBE_KEY_RULES))
            .and_then(|r| r.as_sequence())
            .cloned()
            .expect("HTTPRoute spec.rules sequence")
    }

    #[test]
    fn httproute_carries_politicas_timeout_on_every_rule() {
        // The fixture sets `:politicas :timeout 30s`. Every emitted
        // HTTPRoute rule must carry `timeouts: { request: "30s" }`,
        // wiring the typed `:politicas :timeout` slot through to the
        // canonical Gateway API per-rule request-deadline shape:
        // https://gateway-api.sigs.k8s.io/api-types/httproute/#timeouts
        // Before this overlay landed the typed slot was inert past
        // validate() — the rendered HTTPRoute carried no timeouts:
        // block, so MESH-COMPOSITION §V "no infinite blocking" was
        // a build-time gate without runtime teeth. This test is the
        // pinned proof that the slot now reaches the cluster artifact.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let rules = httproute_rules(&docs);
        assert!(!rules.is_empty(), "HTTPRoute must carry at least one rule");
        for rule in &rules {
            let timeouts = rule
                .get(GATEWAY_API_KEY_TIMEOUTS)
                .and_then(|t| t.as_mapping())
                .expect("rule must carry timeouts mapping when :politicas :timeout is set");
            assert_eq!(
                timeouts
                    .get(GATEWAY_API_KEY_REQUEST)
                    .and_then(|v| v.as_str()),
                Some("30s")
            );
        }
    }

    #[test]
    fn httproute_omits_timeouts_when_politicas_timeout_unset() {
        // Empty-axis-skip semantic: an Aplicacao that doesn't declare
        // `:politicas :timeout` (politicas = MeshPolicy::default()
        // here) emits an HTTPRoute with no timeouts: key on any rule
        // — the K8s "no per-rule deadline declared" semantic, which
        // matches the prior pre-overlay behavior bit-for-bit. Pinning
        // so a future refactor of the overlay can't accidentally
        // emit `timeouts: {}` (the empty-mapping form, which some
        // Gateway API conformance suites reject as malformed).
        let mut c = aplicacao_caixa();
        c.politicas = Some(MeshPolicy::default());
        let docs = gateway_routes(&c).unwrap();
        let rules = httproute_rules(&docs);
        assert!(!rules.is_empty());
        for rule in &rules {
            assert!(
                rule.get(GATEWAY_API_KEY_TIMEOUTS).is_none(),
                "rule must omit `timeouts:` when :politicas :timeout is None"
            );
        }
    }

    #[test]
    fn httproute_timeout_renders_every_rule_independently() {
        // Multiple `:entrada :paths` entries → multiple HTTPRoute
        // rules. The overlay must apply to every rule, not just the
        // first one — pin so a future refactor that hoists the
        // overlay out of the loop without re-cloning into each rule
        // can't accidentally drop the policy from the tail rules.
        let mut c = aplicacao_caixa();
        if let Some(e) = c.entrada.as_mut() {
            e.paths = vec![
                "/api/cart".into(),
                "/api/products".into(),
                "/healthz".into(),
            ];
        }
        let docs = gateway_routes(&c).unwrap();
        let rules = httproute_rules(&docs);
        assert_eq!(rules.len(), 3);
        for rule in &rules {
            let req = rule
                .get(GATEWAY_API_KEY_TIMEOUTS)
                .and_then(|t| t.get(GATEWAY_API_KEY_REQUEST))
                .and_then(|v| v.as_str())
                .expect("each of the 3 rules carries timeouts.request");
            assert_eq!(req, "30s");
        }
    }

    #[test]
    fn httproute_timeout_uses_canonical_kube_duration_format() {
        // The duration formatter is shared with every other
        // typed-duration slot (caixa_core::supervisor::duration_codec
        // ::render). Pin that a 90-second timeout renders as `"90s"`
        // (the canonical form K8s tooling parses), not `"1m30s"`
        // (the ad-hoc multi-unit form some Go time.Duration formatters
        // produce, which the Gateway API parser rejects).
        let mut c = aplicacao_caixa();
        c.politicas = Some(MeshPolicy {
            timeout: Some(Duration::from_secs(90)),
            ..Default::default()
        });
        let docs = gateway_routes(&c).unwrap();
        let rules = httproute_rules(&docs);
        for rule in &rules {
            assert_eq!(
                rule.get(GATEWAY_API_KEY_TIMEOUTS)
                    .and_then(|t| t.get(GATEWAY_API_KEY_REQUEST))
                    .and_then(|v| v.as_str()),
                Some("90s")
            );
        }
    }

    #[test]
    fn httproute_timeout_renders_minute_window_canonically() {
        // A 1-minute timeout must render as `"1m"` (canonical), not
        // `"60s"` (numerically equivalent but not the canonical form
        // duration_codec::render emits). Pinning the formatter's
        // round-trip contract through the renderer end-to-end so a
        // future change to the formatter that picks a non-canonical
        // unit surfaces here.
        let mut c = aplicacao_caixa();
        c.politicas = Some(MeshPolicy {
            timeout: Some(Duration::from_secs(60)),
            ..Default::default()
        });
        let docs = gateway_routes(&c).unwrap();
        let rules = httproute_rules(&docs);
        for rule in &rules {
            assert_eq!(
                rule.get(GATEWAY_API_KEY_TIMEOUTS)
                    .and_then(|t| t.get(GATEWAY_API_KEY_REQUEST))
                    .and_then(|v| v.as_str()),
                Some("1m")
            );
        }
    }

    #[test]
    fn httproute_rule_keys_pin_overlay_position() {
        // Pin that timeouts: + retry: appear alongside matches: and
        // backendRefs: at the rule level, not nested inside either.
        // Gateway API v1.x defines both as top-level rule fields — a
        // misplaced `matches[].timeouts` or `matches[].retry` would
        // silently be ignored by the apiserver, which matches no
        // traffic visibly but disables the per-call deadline / retry
        // budget. The fixture sets both `:politicas :timeout` and
        // `:politicas :retries`, so every rule carries all 4 keys.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let rules = httproute_rules(&docs);
        for rule in &rules {
            let m = rule.as_mapping().expect("rule mapping");
            // matches + backendRefs + timeouts + retry (4 top-level keys).
            assert_eq!(m.len(), 4);
            assert!(m.contains_key(GATEWAY_API_KEY_MATCHES));
            assert!(m.contains_key(GATEWAY_API_KEY_BACKEND_REFS));
            assert!(m.contains_key(GATEWAY_API_KEY_TIMEOUTS));
            assert!(m.contains_key(GATEWAY_API_KEY_RETRY));
        }
    }

    // ── HTTPRoute :politicas :retries overlay ────────────────────────────

    #[test]
    fn httproute_carries_politicas_retries_on_every_rule() {
        // The fixture sets `:politicas :retries 3`. Every emitted
        // HTTPRoute rule must carry `retry: { attempts: 3 }`, wiring
        // the typed `:politicas :retries` slot through to the
        // canonical Gateway API per-rule retry-policy shape:
        // https://gateway-api.sigs.k8s.io/api-types/httproute/#retry
        // Before this overlay landed the typed slot was inert past
        // validate() — `AplicacaoSpec::validate` refused zero via
        // PolicyRetriesZero, but a non-zero attempt count never
        // reached an emitted artifact. This test is the pinned proof
        // that the slot now reaches the cluster artifact (the
        // fail-before-pass-after pin: the assertion below fails on
        // any pre-overlay codebase, since the rule had no `retry:` key
        // at all).
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let rules = httproute_rules(&docs);
        assert!(!rules.is_empty(), "HTTPRoute must carry at least one rule");
        for rule in &rules {
            let retry = rule
                .get(GATEWAY_API_KEY_RETRY)
                .and_then(|r| r.as_mapping())
                .expect("rule must carry retry mapping when :politicas :retries is set");
            assert_eq!(
                retry.get(GATEWAY_API_KEY_ATTEMPTS).and_then(|v| v.as_u64()),
                Some(3),
                "retry.attempts must round-trip the typed :retries value"
            );
        }
    }

    #[test]
    fn httproute_omits_retry_when_politicas_retries_unset() {
        // Empty-axis-skip semantic (mirrors the `:timeout` overlay's
        // omit-when-unset contract): an Aplicacao that doesn't declare
        // `:politicas :retries` (politicas = MeshPolicy::default()
        // here) emits an HTTPRoute with no `retry:` key on any rule
        // — the K8s "no per-rule retry policy declared" semantic,
        // which lets the cluster default policy take effect. Pinning
        // so a future refactor of the overlay can't accidentally emit
        // `retry: {}` (the empty-mapping form, which is structurally
        // distinct from "no retry policy" and may be rejected by the
        // Gateway API parser).
        let mut c = aplicacao_caixa();
        c.politicas = Some(MeshPolicy::default());
        let docs = gateway_routes(&c).unwrap();
        let rules = httproute_rules(&docs);
        assert!(!rules.is_empty());
        for rule in &rules {
            assert!(
                rule.get(GATEWAY_API_KEY_RETRY).is_none(),
                "rule must omit `retry:` when :politicas :retries is None"
            );
        }
    }

    #[test]
    fn httproute_retry_renders_every_rule_independently() {
        // Multiple `:entrada :paths` entries → multiple HTTPRoute
        // rules. The retry overlay must apply to every rule, not just
        // the first one — pin so a future refactor that hoists the
        // overlay out of the loop without re-cloning into each rule
        // can't accidentally drop the policy from the tail rules.
        // Same hoist-out-of-loop guard the parallel `:timeout`
        // overlay test enshrines.
        let mut c = aplicacao_caixa();
        if let Some(e) = c.entrada.as_mut() {
            e.paths = vec![
                "/api/cart".into(),
                "/api/products".into(),
                "/healthz".into(),
            ];
        }
        let docs = gateway_routes(&c).unwrap();
        let rules = httproute_rules(&docs);
        assert_eq!(rules.len(), 3);
        for rule in &rules {
            let attempts = rule
                .get(GATEWAY_API_KEY_RETRY)
                .and_then(|r| r.get(GATEWAY_API_KEY_ATTEMPTS))
                .and_then(|v| v.as_u64())
                .expect("each of the 3 rules carries retry.attempts");
            assert_eq!(attempts, 3);
        }
    }

    #[test]
    fn httproute_retry_round_trips_typed_attempt_count() {
        // The overlay must round-trip whatever value the typed
        // `:retries` slot carries — pin a non-fixture value (5) so a
        // future change to the formatter / overlay shape (e.g.
        // accidentally clamping attempts to a constant, mis-mapping
        // the typed `u32` to a string) surfaces here.
        let mut c = aplicacao_caixa();
        c.politicas = Some(MeshPolicy {
            retries: Some(5),
            ..Default::default()
        });
        let docs = gateway_routes(&c).unwrap();
        let rules = httproute_rules(&docs);
        for rule in &rules {
            assert_eq!(
                rule.get(GATEWAY_API_KEY_RETRY)
                    .and_then(|r| r.get(GATEWAY_API_KEY_ATTEMPTS))
                    .and_then(|v| v.as_u64()),
                Some(5),
                "retry.attempts must round-trip the typed :retries value verbatim"
            );
        }
    }

    #[test]
    fn httproute_retry_attempts_serialized_as_yaml_number() {
        // Pin the YAML scalar shape: `attempts:` is an *integer* in
        // the Gateway API schema (HTTPRouteRetry.attempts: integer),
        // not a string. A renderer that accidentally emits
        // `attempts: "3"` would round-trip past serde_yaml but be
        // rejected by the apiserver-side OpenAPI schema validation
        // — pin the scalar kind here so a regression surfaces at
        // build time, not at apply time.
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let rules = httproute_rules(&docs);
        for rule in &rules {
            let attempts = rule
                .get(GATEWAY_API_KEY_RETRY)
                .and_then(|r| r.get(GATEWAY_API_KEY_ATTEMPTS))
                .expect("retry.attempts present");
            assert!(
                attempts.is_u64() || attempts.is_i64(),
                "retry.attempts must be a YAML integer (got: {attempts:?})"
            );
        }
    }

    #[test]
    fn httproute_timeouts_and_retry_coexist_independently() {
        // The two `:politicas` axes (`:timeout` + `:retries`) emit
        // independently — one set, the other unset, must surface
        // exactly the expected single overlay. Pin both directions
        // (timeout-only and retries-only) so a future refactor can't
        // accidentally couple the two emission gates.
        let mut c = aplicacao_caixa();
        c.politicas = Some(MeshPolicy {
            timeout: Some(Duration::from_secs(15)),
            retries: None,
            ..Default::default()
        });
        let docs = gateway_routes(&c).unwrap();
        let rules = httproute_rules(&docs);
        for rule in &rules {
            assert_eq!(
                rule.get(GATEWAY_API_KEY_TIMEOUTS)
                    .and_then(|t| t.get(GATEWAY_API_KEY_REQUEST))
                    .and_then(|v| v.as_str()),
                Some("15s")
            );
            assert!(
                rule.get(GATEWAY_API_KEY_RETRY).is_none(),
                "retry: must be absent when only :timeout is set"
            );
        }

        let mut c2 = aplicacao_caixa();
        c2.politicas = Some(MeshPolicy {
            timeout: None,
            retries: Some(2),
            ..Default::default()
        });
        let docs = gateway_routes(&c2).unwrap();
        let rules = httproute_rules(&docs);
        for rule in &rules {
            assert!(
                rule.get(GATEWAY_API_KEY_TIMEOUTS).is_none(),
                "timeouts: must be absent when only :retries is set"
            );
            assert_eq!(
                rule.get(GATEWAY_API_KEY_RETRY)
                    .and_then(|r| r.get(GATEWAY_API_KEY_ATTEMPTS))
                    .and_then(|v| v.as_u64()),
                Some(2)
            );
        }
    }

    // ── CiliumNetworkPolicy :politicas :mtls-required overlay ────────────

    fn cnp_ingress_rules(docs: &[serde_yaml::Value]) -> Vec<serde_yaml::Value> {
        docs.iter()
            .filter(|d| kube_kind_is(d, CILIUM_KIND_NETWORK_POLICY))
            .filter_map(|d| {
                kube_spec_field(d, CILIUM_KEY_INGRESS)
                    .and_then(|i| i.as_sequence())
                    .and_then(|s| s.first())
                    .cloned()
            })
            .collect()
    }

    #[test]
    fn cnp_carries_politicas_mtls_required_on_every_rule() {
        // The fixture sets `:politicas :mtls-required t`. Every
        // emitted CiliumNetworkPolicy ingress rule must carry
        // `authentication: { mode: "required" }`, wiring the typed
        // `:politicas :mtls-required` slot through to the canonical
        // Cilium per-rule mutual-authentication shape:
        // https://docs.cilium.io/en/stable/network/servicemesh/mutual-authentication/
        // Before this overlay landed the typed slot was inert past
        // validate() — the rendered CNP carried no authentication:
        // block, so MESH-COMPOSITION §V "no plaintext intra-mesh"
        // was a build-time gate without runtime teeth. This test is
        // the pinned proof that the slot now reaches the cluster
        // artifact (the fail-before-pass-after pin: the assertion
        // below fails on any pre-overlay codebase, since the rule
        // had no `authentication:` key at all).
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        let rules = cnp_ingress_rules(&policies);
        assert!(
            !rules.is_empty(),
            "CNPs must carry at least one ingress rule"
        );
        for rule in &rules {
            let auth = rule
                .get(CILIUM_KEY_AUTHENTICATION)
                .and_then(|a| a.as_mapping())
                .expect("rule must carry authentication mapping when :mtls-required is set");
            assert_eq!(
                auth.get(CILIUM_KEY_MODE).and_then(|v| v.as_str()),
                Some(CILIUM_AUTH_MODE_REQUIRED)
            );
        }
    }

    #[test]
    fn cnp_omits_authentication_when_mtls_required_unset() {
        // Empty-axis-skip semantic (mirrors the `:timeout`/`:retries`
        // overlays' omit-when-unset contract): an Aplicacao that
        // doesn't declare `:politicas :mtls-required` (politicas =
        // MeshPolicy::default() here) emits CNPs with no
        // `authentication:` key on any ingress rule — the Cilium
        // "no per-rule auth mode declared" semantic, which lets the
        // cluster default policy take effect (typically "disabled").
        // Pinning so a future refactor of the overlay can't
        // accidentally emit `authentication: {}` (the empty-mapping
        // form, which the Cilium agent rejects as malformed since
        // the `mode:` field is required when the block is present).
        let mut c = aplicacao_caixa();
        c.politicas = Some(MeshPolicy::default());
        let policies = cilium_network_policies(&c).unwrap();
        let rules = cnp_ingress_rules(&policies);
        assert!(!rules.is_empty());
        for rule in &rules {
            assert!(
                rule.get(CILIUM_KEY_AUTHENTICATION).is_none(),
                "rule must omit `authentication:` when :mtls-required is None"
            );
        }
    }

    #[test]
    fn cnp_explicit_mtls_required_false_emits_disabled_mode() {
        // The author-facing `:mtls-required` slot is a tristate. The
        // `Some(false)` arm is *not* the same as `None` — the author
        // explicitly named the axis and asked for the mTLS handshake
        // to be skipped on this Aplicacao's edges (e.g. a debug or
        // legacy-bridge Aplicacao that needs to talk to non-mesh
        // peers). The renderer must surface that explicit opt-out as
        // `mode: "disabled"`, *not* fall back to omitting the block
        // (which would let the cluster default — typically also
        // "disabled" today, but environment-divergent — take effect).
        // Pinning so a future refactor that collapses the tristate
        // into a bool can't silently lose the authored intent.
        let mut c = aplicacao_caixa();
        c.politicas = Some(MeshPolicy {
            mtls_required: Some(false),
            ..Default::default()
        });
        let policies = cilium_network_policies(&c).unwrap();
        let rules = cnp_ingress_rules(&policies);
        assert!(!rules.is_empty());
        for rule in &rules {
            let auth = rule
                .get(CILIUM_KEY_AUTHENTICATION)
                .and_then(|a| a.as_mapping())
                .expect("rule must carry authentication mapping for explicit :mtls-required nil");
            assert_eq!(
                auth.get(CILIUM_KEY_MODE).and_then(|v| v.as_str()),
                Some(CILIUM_AUTH_MODE_DISABLED)
            );
        }
    }

    #[test]
    fn cnp_authentication_renders_every_policy_independently() {
        // Multiple `:contratos` → multiple CiliumNetworkPolicies.
        // The auth overlay must apply to every policy's ingress
        // rule, not just the first one — pin so a future refactor
        // that hoists the overlay out of the loop without re-cloning
        // into each rule can't accidentally drop the policy from the
        // tail CNPs. Same hoist-out-of-loop guard the parallel
        // `:timeout`/`:retries` overlay tests enshrine for HTTPRoute.
        let mut c = aplicacao_caixa();
        // Three `:contratos` → three CNPs.
        c.contratos.push(WitContract {
            de: "payment".into(),
            para: "catalog".into(),
            wit: "wasi:http/proxy".into(),
            endpoint: Some("/inventory".into()),
            subject: None,
            slot: None,
        });
        let policies = cilium_network_policies(&c).unwrap();
        assert_eq!(policies.len(), 3);
        let rules = cnp_ingress_rules(&policies);
        assert_eq!(rules.len(), 3);
        for rule in &rules {
            assert_eq!(
                rule.get(CILIUM_KEY_AUTHENTICATION)
                    .and_then(|a| a.get(CILIUM_KEY_MODE))
                    .and_then(|v| v.as_str()),
                Some(CILIUM_AUTH_MODE_REQUIRED),
                "every CNP's ingress rule must carry the authentication overlay"
            );
        }
    }

    #[test]
    fn cnp_authentication_position_is_rule_level_not_nested() {
        // Pin that `authentication:` appears at the ingress-rule
        // level (alongside `fromEndpoints` + `toPorts`), not nested
        // inside either. Cilium's per-rule mutual-auth schema places
        // the field at the IngressRule axis — a misplaced
        // `fromEndpoints[].authentication` or
        // `toPorts[].authentication` would silently be ignored by
        // the Cilium agent (matches no traffic visibly but disables
        // the per-edge mTLS contract). The fixture sets
        // `:mtls-required t`, so every rule carries fromEndpoints +
        // toPorts + authentication (3 top-level rule keys).
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        let rules = cnp_ingress_rules(&policies);
        for rule in &rules {
            let m = rule.as_mapping().expect("rule mapping");
            assert_eq!(m.len(), 3);
            assert!(m.contains_key(CILIUM_KEY_FROM_ENDPOINTS));
            assert!(m.contains_key(CILIUM_KEY_TO_PORTS));
            assert!(m.contains_key(CILIUM_KEY_AUTHENTICATION));
            // The auth block must not leak inside fromEndpoints[] or
            // toPorts[] — guards the Cilium-side schema contract that
            // mutual-auth is an ingress-rule-level concern.
            let from = m
                .get(CILIUM_KEY_FROM_ENDPOINTS)
                .and_then(|f| f.as_sequence())
                .expect("fromEndpoints sequence");
            for fe in from {
                assert!(
                    fe.get(CILIUM_KEY_AUTHENTICATION).is_none(),
                    "authentication must not nest inside fromEndpoints[]"
                );
            }
            let to = m
                .get(CILIUM_KEY_TO_PORTS)
                .and_then(|t| t.as_sequence())
                .expect("toPorts sequence");
            for tp in to {
                assert!(
                    tp.get(CILIUM_KEY_AUTHENTICATION).is_none(),
                    "authentication must not nest inside toPorts[]"
                );
            }
        }
    }

    #[test]
    fn cnp_authentication_pubsub_contracts_carry_overlay_too() {
        // The auth overlay applies to every `:contratos` edge,
        // regardless of WIT shape. Cilium's mutual-auth happens at
        // L4 (per the SPIFFE-identity handshake) — same as the
        // identity-bound fromEndpoints selector — so a `nats:pub-sub`
        // contrato (which the existing `cilium_pubsub_contracts_skip_l7_rules`
        // test pins as L4-only) still carries the auth block. Pin
        // here so a future overlay refactor that mistakenly couples
        // the auth overlay to L7-shape (e.g. only adds it when
        // `target() == WitTarget::Http`) surfaces.
        let mut c = aplicacao_caixa();
        c.contratos.push(WitContract {
            de: "payment".into(),
            para: "cart".into(), // back-edge for testing only
            wit: "nats:pub-sub".into(),
            endpoint: None,
            subject: Some("checkout.events.charge.failed".into()),
            slot: None,
        });
        let policies = cilium_network_policies(&c).unwrap();
        let nats_policy =
            find_by_name(&policies, "checkout-payment-to-cart").expect("pubsub CNP present");
        let rule = kube_spec_field(nats_policy, CILIUM_KEY_INGRESS)
            .and_then(|i| i.as_sequence())
            .and_then(|s| s.first())
            .expect("ingress[0]");
        assert_eq!(
            rule.get(CILIUM_KEY_AUTHENTICATION)
                .and_then(|a| a.get(CILIUM_KEY_MODE))
                .and_then(|v| v.as_str()),
            Some(CILIUM_AUTH_MODE_REQUIRED)
        );
    }

    #[test]
    fn cnp_l4_fallback_port_routes_through_lifted_default_servico_port() {
        // The L4-fallback in [`cilium_network_policies`] (the
        // `.unwrap_or(DEFAULT_SERVICO_PORT)` branch fired when the
        // typed `:entrada` block doesn't name the per-`:contratos`
        // destination Servico) must read from the lifted
        // [`caixa_core::DEFAULT_SERVICO_PORT`] constant — not from an
        // open-coded `8080` literal that could drift if the
        // substrate's canonical Servico port ever moved. The fixture's
        // `cart → payment` HTTP contrato exercises this arm: the
        // fixture's `:entrada` block names `:para "cart"`, so the
        // sibling `cart → catalog` and `cart → payment` contratos
        // don't match the entrada's `:para` axis (the entrada is the
        // ingress *to* cart, not *to* payment / catalog), and the
        // renderer falls back to DEFAULT_SERVICO_PORT on the per-CNP
        // L4 port. Pin the rendered L4 port at `DEFAULT_SERVICO_PORT`
        // verbatim so a future rebrand of the constant reaches this
        // consumer by construction, peer with the
        // `default_namespace_re_export_points_at_caixa_core_canonical`
        // pin on the namespace-axis lifted-constant.
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        let cart_to_payment =
            find_by_name(&policies, "checkout-cart-to-payment").expect("cart→payment CNP present");
        let port_value = kube_spec_field(cart_to_payment, CILIUM_KEY_INGRESS)
            .and_then(|i| i.as_sequence())
            .and_then(|s| s.first())
            .and_then(|i| i.get(CILIUM_KEY_TO_PORTS))
            .and_then(|t| t.as_sequence())
            .and_then(|s| s.first())
            .and_then(|tp| tp.get(CILIUM_KEY_PORTS))
            .and_then(|p| p.as_sequence())
            .and_then(|s| s.first())
            .and_then(|p| p.get(KUBE_KEY_PORT))
            .and_then(|v| v.as_str())
            .expect("toPorts[0].ports[0].port present");
        assert_eq!(
            port_value,
            DEFAULT_SERVICO_PORT.to_string(),
            "the L4 fallback must render the lifted DEFAULT_SERVICO_PORT \
             constant verbatim — drift here means the constant lift no \
             longer reaches this consumer"
        );
    }

    #[test]
    fn cnp_l4_port_routes_through_lifted_port_for_destination_resolver() {
        // Cross-crate drift-detection pin: the CNP L4-fallback port axis
        // in [`cilium_network_policies`] now routes through the lifted
        // [`caixa_core::AplicacaoSpec::port_for_destination`] typed
        // dispatch (peer with the sibling
        // `cnp_l4_fallback_port_routes_through_lifted_default_servico_port`
        // pin on the DEFAULT_SERVICO_PORT canonical-constant axis). Pin
        // the resolver-shaped rule at the emit-side path so a future
        // renderer-side detour that re-inlined the "if entrada matches
        // this destination use its :port else fall back to
        // DEFAULT_SERVICO_PORT" cascade would surface as a caixa-mesh
        // build-time test failure — the two consumer paths on the
        // per-Aplicacao L4 port axis (the resolver's own typed dispatch
        // at caixa-core, this renderer's emit-side reach for it) must
        // agree at every point on the port scalar the emitted CNP
        // renders, per the CNP identity rule that the destination
        // Servico's L4 port is a substrate-canonical scalar the M4 CR
        // materializer + the future per-edge policy resolver inherit
        // by construction.
        let caixa = aplicacao_caixa();
        let spec = typed_view(&caixa).expect("aplicacao_caixa fixture must be a valid Aplicacao");
        let policies = cilium_network_policies(&caixa).unwrap();

        // The `checkout-cart-to-catalog` CNP's toPorts[0].ports[0].port
        // scalar must match the resolver's scalar for the `catalog`
        // destination (a non-apex destination — the entrada names
        // `:para "cart"`, so the resolver falls back to the substrate
        // floor).
        let cart_to_catalog =
            find_by_name(&policies, "checkout-cart-to-catalog").expect("cart→catalog CNP present");
        let port_value = kube_spec_field(cart_to_catalog, CILIUM_KEY_INGRESS)
            .and_then(|i| i.as_sequence())
            .and_then(|s| s.first())
            .and_then(|i| i.get(CILIUM_KEY_TO_PORTS))
            .and_then(|t| t.as_sequence())
            .and_then(|s| s.first())
            .and_then(|tp| tp.get(CILIUM_KEY_PORTS))
            .and_then(|p| p.as_sequence())
            .and_then(|s| s.first())
            .and_then(|p| p.get(KUBE_KEY_PORT))
            .and_then(|v| v.as_str())
            .expect("toPorts[0].ports[0].port present");
        assert_eq!(
            port_value,
            spec.port_for_destination("catalog").to_string(),
            "the per-CNP L4 port must match the port_for_destination \
             resolver's scalar for the same destination — drift here \
             means the emit-side path re-inlined the resolution rule"
        );
    }

    #[test]
    fn cnp_authentication_mode_serialized_as_yaml_string() {
        // Pin the YAML scalar shape: `mode:` is a *string* in the
        // Cilium CRD schema (CiliumNetworkPolicy.spec.ingress[]
        // .authentication.mode: string enum {required, disabled,
        // test-always-fail}), not a bool. A renderer that
        // accidentally emits `mode: true` (the raw typed slot's
        // bool) would round-trip past serde_yaml but be rejected by
        // the apiserver-side OpenAPI schema validation — pin the
        // scalar kind here so a regression surfaces at build time,
        // not at apply time.
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        let rules = cnp_ingress_rules(&policies);
        for rule in &rules {
            let mode = rule
                .get(CILIUM_KEY_AUTHENTICATION)
                .and_then(|a| a.get(CILIUM_KEY_MODE))
                .expect("authentication.mode present");
            assert!(
                mode.is_string(),
                "authentication.mode must be a YAML string (got: {mode:?})"
            );
        }
    }

    /// Byte-parity converge pin on the per-`:entrada` outer-composite
    /// `Option<&Entrada>` accessor axis: [`AplicacaoSpec::entrada`] must
    /// agree with the raw `spec.entrada.as_ref()` field access on the
    /// `aplicacao_caixa` fixture across both the `Some(_)` presence-bit
    /// arm and the projected `Entrada` composite's per-axis `:host`,
    /// `:para`, `:paths`, `:port` scalar reads, and on the mutated
    /// `entrada = None` `None` arm the sibling `gateway_skips_when_no_entrada`
    /// test exercises. Peer of the caixa-core-side
    /// `aplicacao_spec_entrada_returns_entrada_option_ref_byte_equal_across_permutations`
    /// pin (the substrate-primitive accessor definition — `entrada(&self)
    /// -> Option<&Entrada>` = `self.entrada.as_ref()`) — this pin lands
    /// the sibling drift-detection gate at the caixa-mesh boundary so a
    /// future extension of the `:entrada` slot (a per-cluster ingress
    /// alias table pinned through a future `:entrada-overrides` overlay
    /// the MESH-COMPOSITION §V federation roadmap acknowledges, an M4
    /// `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's admission-
    /// webhook per-tenant `:host` rewrite, a canonicalization pass that
    /// lowercases the DNS-1123 label post-parse) that landed on the
    /// accessor without a lockstep edit on the raw field access — or vice
    /// versa — surfaces at caixa-mesh build time rather than at cluster-
    /// apply time. The three prior caixa-mesh test-side raw
    /// `spec.entrada.<field>` sites (the `typed_view_returns_validated_spec`
    /// presence-bit probe, the
    /// `httproute_backend_ref_port_routes_through_lifted_port_for_destination_resolver`
    /// per-permutation `entrada_ref` bind, and the
    /// `httproute_backend_ref_port_and_cnp_l4_port_share_port_for_destination_resolver_at_emit_site`
    /// apex-destination bind) are the consumers this pin protects — the
    /// converge routed each onto the substrate primitive; the pin here
    /// guards the primitive's byte-parity contract against silent
    /// divergence.
    #[test]
    fn spec_entrada_accessor_byte_equal_to_raw_field_access() {
        // `Some(_)` arm — the aplicacao_caixa fixture carries a typed
        // `:entrada` block, so the accessor projects `Some(&Entrada)`
        // byte-equal to the raw `self.entrada.as_ref()`.
        let spec = typed_view(&aplicacao_caixa()).expect("fixture is a valid Aplicacao");
        let via_accessor: Option<&Entrada> = spec.entrada();
        let via_raw: Option<&Entrada> = spec.entrada.as_ref();
        assert_eq!(
            via_accessor.is_some(),
            via_raw.is_some(),
            "AplicacaoSpec::entrada() must project the raw \
             Option<Entrada> slot's presence bit byte-equal to \
             self.entrada.as_ref() — drift would let the accessor's \
             Some/None partition disagree with the raw field's on a \
             fixture the substrate contract pins as Some(_)"
        );
        let acc = via_accessor.expect("accessor Some arm");
        let raw = via_raw.expect("raw Some arm");
        assert_eq!(
            acc.hostname(),
            raw.hostname(),
            "accessor and raw must agree on Entrada::hostname()"
        );
        assert_eq!(
            acc.destination(),
            raw.destination(),
            "accessor and raw must agree on Entrada::destination()"
        );
        assert_eq!(
            acc.port(),
            raw.port(),
            "accessor and raw must agree on Entrada::port()"
        );
        assert_eq!(
            acc.paths(),
            raw.paths(),
            "accessor and raw must agree on Entrada::paths()"
        );

        // `None` arm — mutate the fixture to drop `:entrada`, matching
        // the sibling `gateway_skips_when_no_entrada` early-return
        // partition. The accessor and the raw field must both project
        // `None`.
        let mut no_entrada = aplicacao_caixa();
        no_entrada.entrada = None;
        let spec_none = typed_view(&no_entrada).expect("fixture without :entrada is still valid");
        assert!(
            spec_none.entrada().is_none(),
            "accessor must project None on a fixture with no :entrada"
        );
        assert!(
            spec_none.entrada.is_none(),
            "raw field must project None on a fixture with no :entrada"
        );
    }

    /// Byte-parity converge pin on the per-`Caixa` `:entrada` outer-composite
    /// `Option<&Entrada>` accessor axis at the caixa-mesh boundary:
    /// [`caixa_core::Caixa::entrada`] must agree with the raw
    /// `caixa.entrada.as_ref()` field access on the `aplicacao_caixa`
    /// fixture across both the `Some(_)` presence-bit arm and the projected
    /// `Entrada` composite's per-axis `:host`, `:para`, `:paths`, `:port`
    /// scalar reads, and on the mutated `entrada = None` `None` arm the
    /// sibling `gateway_skips_when_no_entrada` test exercises. Peer of the
    /// caixa-core-side
    /// [`caixa_core::manifest::tests::entrada_returns_entrada_option_ref_verbatim_across_permutations`]
    /// pin (the substrate-primitive accessor definition —
    /// `entrada(&self) -> Option<&Entrada>` = `self.entrada.as_ref()`) —
    /// this pin lands the sibling drift-detection gate at the caixa-mesh
    /// boundary so a future extension of the top-level `Caixa`'s
    /// `Option<Entrada>` `:entrada` slot (a per-cluster ingress-alias table
    /// pinned through a future `:entrada-overrides` overlay, an M4
    /// `mesh.pleme.io/v1alpha1/Caixa` CR materializer's admission-webhook
    /// per-tenant `:host` rewrite) that landed on the accessor without a
    /// lockstep edit on the raw field access — or vice versa — surfaces at
    /// caixa-mesh build time rather than at cluster-apply time. The
    /// caixa-mesh test-side raw `caixa.entrada.as_ref()` site the
    /// `httproute_name_derives_from_caixa_nome_and_entrada_destination` pin
    /// carried before this converge is the consumer this pin protects — the
    /// converge routed the site onto the substrate primitive; the pin here
    /// guards the primitive's byte-parity contract against silent divergence.
    /// Peer of the sibling
    /// [`spec_entrada_accessor_byte_equal_to_raw_field_access`] pin on the
    /// paired [`caixa_core::AplicacaoSpec::entrada`] view-side axis — this
    /// pin extends the same "byte-equal, borrow-shared, presence-bit-
    /// preserved" outer-accessor discipline onto the top-level `Caixa`
    /// axis at the caixa-mesh crate boundary.
    #[test]
    fn caixa_entrada_accessor_byte_equal_to_raw_field_access() {
        // `Some(_)` arm — the aplicacao_caixa fixture carries a typed
        // `:entrada` block, so the accessor projects `Some(&Entrada)`
        // byte-equal to the raw `self.entrada.as_ref()`.
        let c = aplicacao_caixa();
        let via_accessor: Option<&Entrada> = c.entrada();
        let via_raw: Option<&Entrada> = c.entrada.as_ref();
        assert_eq!(
            via_accessor.is_some(),
            via_raw.is_some(),
            "Caixa::entrada() must project the raw Option<Entrada> slot's \
             presence bit byte-equal to self.entrada.as_ref() — drift would \
             let the accessor's Some/None partition disagree with the raw \
             field's on a fixture the substrate contract pins as Some(_)"
        );
        let acc = via_accessor.expect("accessor Some arm");
        let raw = via_raw.expect("raw Some arm");
        assert_eq!(
            acc.hostname(),
            raw.hostname(),
            "accessor and raw must agree on Entrada::hostname()"
        );
        assert_eq!(
            acc.destination(),
            raw.destination(),
            "accessor and raw must agree on Entrada::destination()"
        );
        assert_eq!(
            acc.port(),
            raw.port(),
            "accessor and raw must agree on Entrada::port()"
        );
        assert_eq!(
            acc.paths(),
            raw.paths(),
            "accessor and raw must agree on Entrada::paths()"
        );

        // `None` arm — mutate the fixture to drop `:entrada`, matching
        // the sibling `gateway_skips_when_no_entrada` early-return
        // partition. The accessor and the raw field must both project
        // `None`.
        let mut no_entrada = aplicacao_caixa();
        no_entrada.entrada = None;
        assert!(
            no_entrada.entrada().is_none(),
            "Caixa::entrada() must project None on a fixture with no :entrada"
        );
        assert!(
            no_entrada.entrada.is_none(),
            "Caixa::entrada raw field must project None on a fixture with no :entrada"
        );
    }
}
