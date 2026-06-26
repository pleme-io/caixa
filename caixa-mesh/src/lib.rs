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
    CILIUM_API_VERSION, Caixa, CaixaKind, DEFAULT_SERVICO_PORT, GATEWAY_API_API_VERSION,
    KUBE_KEY_SPEC, LABEL_APLICACAO, LABEL_CONTRATO, M3_KEY_PLACEMENT, WitContract, WitTarget,
    aplicacao::AplicacaoSpec, kube_resource_skeleton, label_selector,
    pleme_program_in_aplicacao_selector, pleme_program_selector, single_field_overlay,
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
    caixa_core::require_kind(caixa, CaixaKind::Aplicacao)?;
    let spec = caixa
        .aplicacao_view()
        .expect("Aplicacao kind has an aplicacao_view");
    spec.validate()?;

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
    let placement_value = serde_yaml::to_value(&spec.placement)?;

    let mut out = Vec::with_capacity(spec.membros.len());
    for m in &spec.membros {
        let mut entry = serde_yaml::Mapping::new();
        entry.insert(
            serde_yaml::Value::String("name".into()),
            serde_yaml::Value::String(m.caixa.clone()),
        );
        entry.insert(
            serde_yaml::Value::String("versao".into()),
            serde_yaml::Value::String(m.versao.clone()),
        );
        // Annotate with the parent Aplicacao's nome so the operator
        // knows which graph this member belongs to.
        entry.insert(
            serde_yaml::Value::String("aplicacao".into()),
            serde_yaml::Value::String(caixa.nome.clone()),
        );
        // M3 `:placement` overlay — see the per-call rationale
        // above. Cloned per entry so each programs.yaml row is
        // self-describing for downstream filters that have no
        // Aplicacao-level context.
        entry.insert(
            serde_yaml::Value::String(M3_KEY_PLACEMENT.into()),
            placement_value.clone(),
        );
        out.push(serde_yaml::Value::Mapping(entry));
    }
    Ok(out)
}

/// Compose a single typed view of the entire Aplicacao for downstream
/// renderers (Cilium, Gateway, observability). Convenience wrapper that
/// validates first.
pub fn typed_view(caixa: &Caixa) -> Result<AplicacaoSpec, Error> {
    caixa_core::require_kind(caixa, CaixaKind::Aplicacao)?;
    let spec = caixa
        .aplicacao_view()
        .expect("Aplicacao kind has an aplicacao_view");
    spec.validate()?;
    Ok(spec)
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
    let mtls_overlay = single_field_overlay(spec.politicas.mtls_required, "mode", |required| {
        serde_yaml::Value::String(if required { "required" } else { "disabled" }.into())
    });
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
    for c in &spec.contratos {
        groups
            .entry((c.de.as_str(), c.para.as_str()))
            .or_default()
            .push(c);
    }

    let mut out = Vec::with_capacity(groups.len());
    for ((de, para), edges) in &groups {
        // Policy's own labels — `aplicacao` (which graph) and
        // `contrato` (which typed edge pair). Keys come from
        // caixa_core::render so a future label-namespace rebrand is a
        // one-line edit, not a search-and-replace across renderers.
        let mut labels = BTreeMap::new();
        labels.insert(LABEL_APLICACAO, caixa.nome.clone());
        labels.insert(LABEL_CONTRATO, format!("{de}-to-{para}"));
        // The apiVersion + kind + metadata.{name, namespace, labels}
        // skeleton comes from caixa_core::render::kube_resource_skeleton
        // — same lift as pleme_program_*_selector applied to the K8s-
        // resource axis. Caller adds spec below. The Cilium-CRD-group/
        // version string threads through the lifted
        // [`CILIUM_API_VERSION`] re-export so a future Cilium-CRD bump
        // lands on the canonical [`caixa_core::CILIUM_API_VERSION`]
        // declaration, not at this call site.
        let mut policy = kube_resource_skeleton(
            CILIUM_API_VERSION,
            "CiliumNetworkPolicy",
            &format!("{}-{}-to-{}", caixa.nome, de, para),
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
        let from_endpoint = label_selector(pleme_program_in_aplicacao_selector(de, &caixa.nome));
        let mut ingress_rule = serde_yaml::Mapping::new();
        ingress_rule.insert(
            serde_yaml::Value::String("fromEndpoints".into()),
            serde_yaml::Value::Sequence(vec![from_endpoint]),
        );

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
            // Fallback to the canonical substrate Servico port
            // ([`DEFAULT_SERVICO_PORT`], lifted in caixa-core) when the
            // typed `:entrada` block doesn't name the per-`:contratos`
            // destination Servico — the typed `:contratos` graph
            // carries no per-destination port axis (the destination
            // port is the destination Servico's `lareira-<nome>`
            // chart's `trigger.service.port`, which the Aplicacao-level
            // renderer has no visibility into without a resolver
            // round-trip). The fallback is the substrate's canonical
            // assumption — by construction the same value the
            // destination's own `pleme-computeunit` chart emits and the
            // same value the destination's own typed `:entrada :port`
            // slot defaults to (via `Entrada`'s `default_port` serde
            // hook, which now also reads from
            // [`DEFAULT_SERVICO_PORT`]). The literal `8080` previously
            // duplicated here is now load-bearing only at the lifted
            // constant's definition site.
            let port = spec
                .entrada
                .as_ref()
                .filter(|e| e.para == c.para)
                .map(|e| e.port)
                .unwrap_or(DEFAULT_SERVICO_PORT);
            port_entry.insert(
                serde_yaml::Value::String("port".into()),
                serde_yaml::Value::String(port.to_string()),
            );
            port_entry.insert(
                serde_yaml::Value::String("protocol".into()),
                serde_yaml::Value::String("TCP".into()),
            );
            to_port.insert(
                serde_yaml::Value::String("ports".into()),
                serde_yaml::Value::Sequence(vec![serde_yaml::Value::Mapping(port_entry)]),
            );

            // L7 introspection only fires for HTTP-shaped contracts; the
            // typed view (validated upstream by AplicacaoSpec::validate)
            // makes the "wit world ↔ payload field" link impossible to
            // get wrong silently. PubSub / Store / Capability edges stay
            // L4-only — Cilium can't introspect those protocols.
            if let WitTarget::Http { endpoint } = c.target().expect("validated by typed_view") {
                let mut http_rule = serde_yaml::Mapping::new();
                http_rule.insert(
                    serde_yaml::Value::String("path".into()),
                    serde_yaml::Value::String(endpoint.to_string()),
                );
                let mut rules = serde_yaml::Mapping::new();
                rules.insert(
                    serde_yaml::Value::String("http".into()),
                    serde_yaml::Value::Sequence(vec![serde_yaml::Value::Mapping(http_rule)]),
                );
                to_port.insert(
                    serde_yaml::Value::String("rules".into()),
                    serde_yaml::Value::Mapping(rules),
                );
            }
            to_ports_seq.push(serde_yaml::Value::Mapping(to_port));
        }
        ingress_rule.insert(
            serde_yaml::Value::String("toPorts".into()),
            serde_yaml::Value::Sequence(to_ports_seq),
        );
        if let Some(a) = &mtls_overlay {
            ingress_rule.insert(
                serde_yaml::Value::String("authentication".into()),
                a.clone(),
            );
        }

        let mut policy_spec = serde_yaml::Mapping::new();
        policy_spec.insert(
            serde_yaml::Value::String("endpointSelector".into()),
            endpoint_selector,
        );
        policy_spec.insert(
            serde_yaml::Value::String("ingress".into()),
            serde_yaml::Value::Sequence(vec![serde_yaml::Value::Mapping(ingress_rule)]),
        );
        policy.insert(
            serde_yaml::Value::String(KUBE_KEY_SPEC.into()),
            serde_yaml::Value::Mapping(policy_spec),
        );

        out.push(serde_yaml::Value::Mapping(policy));
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
    let entrada = match spec.entrada.as_ref() {
        Some(e) => e,
        None => return Ok(Vec::new()),
    };
    let namespace = DEFAULT_NAMESPACE;

    // Gateway — apiVersion + kind + metadata.{name, namespace} skeleton
    // comes from caixa_core::render::kube_resource_skeleton; caller adds
    // spec below. No metadata.labels on Gateway today (the gateway is
    // identified by its own name + namespace; per-Aplicacao label
    // grouping happens at the HTTPRoute / route-attached-policy axis).
    let mut gateway = kube_resource_skeleton(
        GATEWAY_API_API_VERSION,
        "Gateway",
        &caixa.nome,
        namespace,
        BTreeMap::new(),
    );
    let mut listener = serde_yaml::Mapping::new();
    listener.insert(
        serde_yaml::Value::String("name".into()),
        serde_yaml::Value::String("http".into()),
    );
    listener.insert(
        serde_yaml::Value::String("port".into()),
        serde_yaml::Value::Number(80.into()),
    );
    listener.insert(
        serde_yaml::Value::String("protocol".into()),
        serde_yaml::Value::String("HTTP".into()),
    );
    listener.insert(
        serde_yaml::Value::String("hostname".into()),
        serde_yaml::Value::String(entrada.host.clone()),
    );
    let mut g_spec = serde_yaml::Mapping::new();
    // Cilium's gatewayClassName by convention; can be overridden later.
    g_spec.insert(
        serde_yaml::Value::String("gatewayClassName".into()),
        serde_yaml::Value::String("cilium".into()),
    );
    g_spec.insert(
        serde_yaml::Value::String("listeners".into()),
        serde_yaml::Value::Sequence(vec![serde_yaml::Value::Mapping(listener)]),
    );
    gateway.insert(
        serde_yaml::Value::String(KUBE_KEY_SPEC.into()),
        serde_yaml::Value::Mapping(g_spec),
    );

    // HTTPRoute — all paths route to the entrada.para Servico. Same
    // skeleton lift as Gateway above; caller adds spec.
    let mut route = kube_resource_skeleton(
        GATEWAY_API_API_VERSION,
        "HTTPRoute",
        &format!("{}-{}", caixa.nome, entrada.para),
        namespace,
        BTreeMap::new(),
    );

    let mut parent_ref = serde_yaml::Mapping::new();
    parent_ref.insert(
        serde_yaml::Value::String("name".into()),
        serde_yaml::Value::String(caixa.nome.clone()),
    );

    let paths: Vec<&str> = if entrada.paths.is_empty() {
        vec!["/"]
    } else {
        entrada.paths.iter().map(String::as_str).collect()
    };
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
    let timeout_overlay = single_field_overlay(spec.politicas.timeout, "request", |d| {
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
    let retry_overlay = single_field_overlay(spec.politicas.retries, "attempts", |attempts| {
        serde_yaml::Value::Number(attempts.into())
    });
    let mut rules = Vec::with_capacity(paths.len());
    for path in paths {
        let mut path_match = serde_yaml::Mapping::new();
        path_match.insert(
            serde_yaml::Value::String("type".into()),
            serde_yaml::Value::String("PathPrefix".into()),
        );
        path_match.insert(
            serde_yaml::Value::String("value".into()),
            serde_yaml::Value::String(path.to_string()),
        );
        let mut match_entry = serde_yaml::Mapping::new();
        match_entry.insert(
            serde_yaml::Value::String("path".into()),
            serde_yaml::Value::Mapping(path_match),
        );
        let mut backend_ref = serde_yaml::Mapping::new();
        backend_ref.insert(
            serde_yaml::Value::String("name".into()),
            serde_yaml::Value::String(entrada.para.clone()),
        );
        backend_ref.insert(
            serde_yaml::Value::String("port".into()),
            serde_yaml::Value::Number(entrada.port.into()),
        );
        let mut rule = serde_yaml::Mapping::new();
        rule.insert(
            serde_yaml::Value::String("matches".into()),
            serde_yaml::Value::Sequence(vec![serde_yaml::Value::Mapping(match_entry)]),
        );
        rule.insert(
            serde_yaml::Value::String("backendRefs".into()),
            serde_yaml::Value::Sequence(vec![serde_yaml::Value::Mapping(backend_ref)]),
        );
        if let Some(t) = &timeout_overlay {
            rule.insert(serde_yaml::Value::String("timeouts".into()), t.clone());
        }
        if let Some(r) = &retry_overlay {
            rule.insert(serde_yaml::Value::String("retry".into()), r.clone());
        }
        rules.push(serde_yaml::Value::Mapping(rule));
    }

    let mut r_spec = serde_yaml::Mapping::new();
    r_spec.insert(
        serde_yaml::Value::String("parentRefs".into()),
        serde_yaml::Value::Sequence(vec![serde_yaml::Value::Mapping(parent_ref)]),
    );
    r_spec.insert(
        serde_yaml::Value::String("hostnames".into()),
        serde_yaml::Value::Sequence(vec![serde_yaml::Value::String(entrada.host.clone())]),
    );
    r_spec.insert(
        serde_yaml::Value::String("rules".into()),
        serde_yaml::Value::Sequence(rules),
    );
    route.insert(
        serde_yaml::Value::String(KUBE_KEY_SPEC.into()),
        serde_yaml::Value::Mapping(r_spec),
    );

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
        Caixa, CaixaKind, Entrada, LABEL_PROGRAM, Membro, MeshPolicy, Placement, PlacementStrategy,
        WitContract,
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
        assert_eq!(DEFAULT_NAMESPACE, caixa_core::DEFAULT_NAMESPACE);
        assert!(
            std::ptr::eq(
                DEFAULT_NAMESPACE.as_ptr(),
                caixa_core::DEFAULT_NAMESPACE.as_ptr(),
            ),
            "DEFAULT_NAMESPACE must be a re-export of caixa_core::DEFAULT_NAMESPACE, \
             not a sibling `pub const` that happens to carry the same string \
             — drift between the two is the canonical footgun this lift closes"
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
        assert_eq!(GATEWAY_API_API_VERSION, caixa_core::GATEWAY_API_API_VERSION);
        assert!(
            std::ptr::eq(
                GATEWAY_API_API_VERSION.as_ptr(),
                caixa_core::GATEWAY_API_API_VERSION.as_ptr(),
            ),
            "GATEWAY_API_API_VERSION must be a re-export of \
             caixa_core::GATEWAY_API_API_VERSION, not a sibling `pub const` \
             that happens to carry the same string — drift between the two \
             is the canonical footgun this lift closes"
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
        assert_eq!(CILIUM_API_VERSION, caixa_core::CILIUM_API_VERSION);
        assert!(
            std::ptr::eq(
                CILIUM_API_VERSION.as_ptr(),
                caixa_core::CILIUM_API_VERSION.as_ptr(),
            ),
            "CILIUM_API_VERSION must be a re-export of \
             caixa_core::CILIUM_API_VERSION, not a sibling `pub const` \
             that happens to carry the same string — drift between the two \
             is the canonical footgun this lift closes"
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
        assert_eq!(KUBE_KEY_SPEC, caixa_core::KUBE_KEY_SPEC);
        assert!(
            std::ptr::eq(KUBE_KEY_SPEC.as_ptr(), caixa_core::KUBE_KEY_SPEC.as_ptr()),
            "KUBE_KEY_SPEC must be a re-export of caixa_core::KUBE_KEY_SPEC, \
             not a sibling `pub const` that happens to carry the same string \
             — drift between the two is the canonical footgun this lift closes"
        );
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
            assert_eq!(
                p.get("apiVersion").and_then(|v| v.as_str()),
                Some(CILIUM_API_VERSION),
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
            .map(|e| e.get("name").and_then(|n| n.as_str()).unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["catalog", "cart", "payment"]);
    }

    #[test]
    fn programs_for_aplicacao_annotates_with_parent_nome() {
        let entries = programs_for_aplicacao(&aplicacao_caixa()).unwrap();
        for e in &entries {
            assert_eq!(
                e.get("aplicacao").and_then(|v| v.as_str()),
                Some("checkout")
            );
        }
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
                p.get(serde_yaml::Value::String("estrategia".into()))
                    .and_then(|v| v.as_str()),
                Some("Replicated"),
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
                .get(serde_yaml::Value::String("clusters".into()))
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
                p.get(serde_yaml::Value::String("affinity".into()))
                    .and_then(|v| v.as_str()),
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
                p.get(serde_yaml::Value::String("affinity".into()))
                    .is_none(),
                "placement.affinity must be absent when :affinity is None"
            );
            assert!(
                p.get(serde_yaml::Value::String("shardKey".into()))
                    .is_none(),
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
                p.get(serde_yaml::Value::String("estrategia".into()))
                    .and_then(|v| v.as_str()),
                Some("Sharded")
            );
            assert_eq!(
                p.get(serde_yaml::Value::String("shardKey".into()))
                    .and_then(|v| v.as_str()),
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
                p.get(serde_yaml::Value::String("estrategia".into())),
                first.get(serde_yaml::Value::String("estrategia".into())),
                "placement.estrategia must be identical across all members"
            );
            assert_eq!(
                p.get(serde_yaml::Value::String("clusters".into())),
                first.get(serde_yaml::Value::String("clusters".into())),
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
                m.contains_key(serde_yaml::Value::String(M3_KEY_PLACEMENT.into())),
                "entry must carry the M3_KEY_PLACEMENT key exactly"
            );
        }
    }

    #[test]
    fn typed_view_returns_validated_spec() {
        let spec = typed_view(&aplicacao_caixa()).unwrap();
        assert_eq!(spec.membros.len(), 3);
        assert_eq!(spec.contratos.len(), 2);
        assert!(spec.entrada.is_some());
        assert_eq!(spec.placement.clusters.len(), 2);
    }

    #[test]
    fn cilium_emits_one_policy_per_de_para_pair() {
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        assert_eq!(policies.len(), 2);
        let names: Vec<_> = policies
            .iter()
            .map(|p| {
                p.get("metadata")
                    .and_then(|m| m.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap()
                    .to_string()
            })
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
            .filter(|p| {
                p.get("metadata")
                    .and_then(|m| m.get("name"))
                    .and_then(|n| n.as_str())
                    == Some("checkout-cart-to-catalog")
            })
            .collect();
        assert_eq!(
            cart_to_catalog.len(),
            1,
            "two cart→catalog contratos must fan into one policy, not two \
             colliding `checkout-cart-to-catalog` objects"
        );

        let to_ports = cart_to_catalog[0]
            .get("spec")
            .and_then(|s| s.get("ingress"))
            .and_then(|i| i.as_sequence())
            .and_then(|s| s.first())
            .and_then(|i| i.get("toPorts"))
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
                tp.get("rules")
                    .and_then(|r| r.get("http"))
                    .and_then(|h| h.as_sequence())
                    .and_then(|s| s.first())
                    .and_then(|rule| rule.get("path"))
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
            let endpoint = p
                .get("spec")
                .and_then(|s| s.get("endpointSelector"))
                .and_then(|e| e.get("matchLabels"))
                .unwrap();
            assert!(endpoint.get(LABEL_PROGRAM).is_some());
            // Source endpoint must include both program + aplicacao labels
            let from = p
                .get("spec")
                .and_then(|s| s.get("ingress"))
                .and_then(|i| i.as_sequence())
                .and_then(|s| s.first())
                .and_then(|i| i.get("fromEndpoints"))
                .and_then(|e| e.as_sequence())
                .and_then(|s| s.first())
                .and_then(|e| e.get("matchLabels"))
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
            let labels = p
                .get("metadata")
                .and_then(|m| m.get("labels"))
                .and_then(|l| l.as_mapping())
                .expect("policy metadata.labels mapping");
            assert_eq!(
                labels
                    .get(serde_yaml::Value::String(LABEL_APLICACAO.into()))
                    .and_then(|v| v.as_str()),
                Some("checkout")
            );
            // The contrato label is `<de>-to-<para>`; both fixture
            // edges have :de = "cart".
            let contrato_val = labels
                .get(serde_yaml::Value::String(LABEL_CONTRATO.into()))
                .and_then(|v| v.as_str())
                .expect("contrato label present");
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
            let selector = p
                .get("spec")
                .and_then(|s| s.get("endpointSelector"))
                .and_then(|e| e.get("matchLabels"))
                .and_then(|m| m.as_mapping())
                .expect("endpointSelector.matchLabels mapping");
            assert_eq!(
                selector.len(),
                1,
                "destination endpointSelector must be the program-only selector"
            );
            assert!(
                selector
                    .get(serde_yaml::Value::String(LABEL_PROGRAM.into()))
                    .is_some()
            );
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
            let from = p
                .get("spec")
                .and_then(|s| s.get("ingress"))
                .and_then(|i| i.as_sequence())
                .and_then(|s| s.first())
                .and_then(|i| i.get("fromEndpoints"))
                .and_then(|e| e.as_sequence())
                .and_then(|s| s.first())
                .and_then(|e| e.get("matchLabels"))
                .and_then(|m| m.as_mapping())
                .expect("fromEndpoints[0].matchLabels mapping");
            assert_eq!(
                from.len(),
                2,
                "source fromEndpoints must be the program-in-aplicacao selector (2 axes)"
            );
            assert!(
                from.get(serde_yaml::Value::String(LABEL_PROGRAM.into()))
                    .is_some()
            );
            assert!(
                from.get(serde_yaml::Value::String(LABEL_APLICACAO.into()))
                    .is_some()
            );
        }
    }

    #[test]
    fn cilium_http_contracts_emit_l7_rules() {
        let policies = cilium_network_policies(&aplicacao_caixa()).unwrap();
        let cart_to_catalog = policies
            .iter()
            .find(|p| {
                p.get("metadata")
                    .and_then(|m| m.get("name"))
                    .and_then(|n| n.as_str())
                    == Some("checkout-cart-to-catalog")
            })
            .unwrap();
        let http_rules = cart_to_catalog
            .get("spec")
            .and_then(|s| s.get("ingress"))
            .and_then(|i| i.as_sequence())
            .and_then(|s| s.first())
            .and_then(|i| i.get("toPorts"))
            .and_then(|p| p.as_sequence())
            .and_then(|s| s.first())
            .and_then(|p| p.get("rules"))
            .and_then(|r| r.get("http"))
            .and_then(|h| h.as_sequence())
            .unwrap();
        assert_eq!(http_rules.len(), 1);
        assert_eq!(
            http_rules[0].get("path").and_then(|v| v.as_str()),
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
        let nats_policy = policies
            .iter()
            .find(|p| {
                p.get("metadata")
                    .and_then(|m| m.get("name"))
                    .and_then(|n| n.as_str())
                    == Some("checkout-payment-to-cart")
            })
            .unwrap();
        let to_ports = nats_policy
            .get("spec")
            .and_then(|s| s.get("ingress"))
            .and_then(|i| i.as_sequence())
            .and_then(|s| s.first())
            .and_then(|i| i.get("toPorts"))
            .and_then(|p| p.as_sequence())
            .and_then(|s| s.first())
            .unwrap();
        // L4 ports yes; L7 rules no.
        assert!(to_ports.get("ports").is_some());
        assert!(to_ports.get("rules").is_none());
    }

    #[test]
    fn gateway_emits_gateway_plus_httproute_pair() {
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        assert_eq!(docs.len(), 2);
        let kinds: Vec<_> = docs
            .iter()
            .map(|d| d.get("kind").and_then(|k| k.as_str()).unwrap().to_string())
            .collect();
        assert!(kinds.contains(&"Gateway".to_string()));
        assert!(kinds.contains(&"HTTPRoute".to_string()));
    }

    #[test]
    fn gateway_listener_carries_aplicacao_host() {
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let gateway = docs
            .iter()
            .find(|d| d.get("kind").and_then(|k| k.as_str()) == Some("Gateway"))
            .unwrap();
        let listener = gateway
            .get("spec")
            .and_then(|s| s.get("listeners"))
            .and_then(|l| l.as_sequence())
            .and_then(|s| s.first())
            .unwrap();
        assert_eq!(
            listener.get("hostname").and_then(|h| h.as_str()),
            Some("checkout.quero.cloud")
        );
        assert_eq!(
            listener.get("protocol").and_then(|p| p.as_str()),
            Some("HTTP")
        );
    }

    #[test]
    fn httproute_routes_to_entrada_para() {
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let route = docs
            .iter()
            .find(|d| d.get("kind").and_then(|k| k.as_str()) == Some("HTTPRoute"))
            .unwrap();
        let backend = route
            .get("spec")
            .and_then(|s| s.get("rules"))
            .and_then(|r| r.as_sequence())
            .and_then(|s| s.first())
            .and_then(|r| r.get("backendRefs"))
            .and_then(|b| b.as_sequence())
            .and_then(|s| s.first())
            .unwrap();
        assert_eq!(backend.get("name").and_then(|n| n.as_str()), Some("cart"));
        assert_eq!(backend.get("port").and_then(|p| p.as_u64()), Some(8080));
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
            assert_eq!(
                p.get("apiVersion").and_then(|v| v.as_str()),
                Some("cilium.io/v2")
            );
            assert_eq!(
                p.get("kind").and_then(|v| v.as_str()),
                Some("CiliumNetworkPolicy")
            );
            let metadata = p
                .get("metadata")
                .and_then(|m| m.as_mapping())
                .expect("metadata mapping");
            // metadata carries name + namespace + labels (3 keys) — no
            // accidental extras leak past the skeleton lift.
            assert_eq!(metadata.len(), 3);
            assert!(
                metadata
                    .get(serde_yaml::Value::String("name".into()))
                    .and_then(|v| v.as_str())
                    .is_some()
            );
            assert_eq!(
                metadata
                    .get(serde_yaml::Value::String("namespace".into()))
                    .and_then(|v| v.as_str()),
                Some(DEFAULT_NAMESPACE)
            );
            assert!(
                metadata
                    .get(serde_yaml::Value::String("labels".into()))
                    .is_some()
            );
        }
    }

    #[test]
    fn gateway_carries_canonical_kube_skeleton_without_labels() {
        // Pin that Gateway emits apiVersion + kind + metadata.{name,
        // namespace} — and *not* metadata.labels (the empty-labels-skip
        // semantic of kube_resource_skeleton; Gateway does not need
        // per-Aplicacao label grouping at the K8s-resource axis today).
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let gateway = docs
            .iter()
            .find(|d| d.get("kind").and_then(|k| k.as_str()) == Some("Gateway"))
            .expect("Gateway present");
        assert_eq!(
            gateway.get("apiVersion").and_then(|v| v.as_str()),
            Some("gateway.networking.k8s.io/v1")
        );
        let metadata = gateway
            .get("metadata")
            .and_then(|m| m.as_mapping())
            .expect("metadata mapping");
        // Exactly 2 metadata keys (name + namespace) — labels absent.
        assert_eq!(metadata.len(), 2);
        assert_eq!(
            metadata
                .get(serde_yaml::Value::String("name".into()))
                .and_then(|v| v.as_str()),
            Some("checkout")
        );
        assert_eq!(
            metadata
                .get(serde_yaml::Value::String("namespace".into()))
                .and_then(|v| v.as_str()),
            Some(DEFAULT_NAMESPACE)
        );
        assert!(
            metadata
                .get(serde_yaml::Value::String("labels".into()))
                .is_none(),
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
        let docs = gateway_routes(&aplicacao_caixa()).unwrap();
        let route = docs
            .iter()
            .find(|d| d.get("kind").and_then(|k| k.as_str()) == Some("HTTPRoute"))
            .expect("HTTPRoute present");
        assert_eq!(
            route.get("apiVersion").and_then(|v| v.as_str()),
            Some("gateway.networking.k8s.io/v1")
        );
        let metadata = route
            .get("metadata")
            .and_then(|m| m.as_mapping())
            .expect("metadata mapping");
        assert_eq!(metadata.len(), 2);
        assert_eq!(
            metadata
                .get(serde_yaml::Value::String("name".into()))
                .and_then(|v| v.as_str()),
            Some("checkout-cart")
        );
        assert!(
            metadata
                .get(serde_yaml::Value::String("labels".into()))
                .is_none()
        );
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
        let gateway = docs
            .iter()
            .find(|d| d.get("kind").and_then(|k| k.as_str()) == Some("Gateway"))
            .expect("Gateway present");
        assert_eq!(
            gateway.get("apiVersion").and_then(|v| v.as_str()),
            Some(caixa_core::GATEWAY_API_API_VERSION),
            "Gateway's top-level apiVersion must equal the lifted \
             caixa_core::GATEWAY_API_API_VERSION by value — drift here \
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
        let route = docs
            .iter()
            .find(|d| d.get("kind").and_then(|k| k.as_str()) == Some("HTTPRoute"))
            .expect("HTTPRoute present");
        assert_eq!(
            route.get("apiVersion").and_then(|v| v.as_str()),
            Some(caixa_core::GATEWAY_API_API_VERSION),
            "HTTPRoute's top-level apiVersion must equal the lifted \
             caixa_core::GATEWAY_API_API_VERSION by value — drift here \
             is the canonical footgun this lift closes"
        );
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
            let metadata = p
                .get("metadata")
                .and_then(|m| m.as_mapping())
                .expect("metadata mapping");
            let keys: Vec<&str> = metadata.iter().filter_map(|(k, _)| k.as_str()).collect();
            assert_eq!(
                keys,
                vec!["labels", "name", "namespace"],
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
                d.get("kind")
                    .and_then(|k| k.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        // programs entries don't carry `kind:`; cilium + gateway docs do.
        assert!(kinds.contains(&"CiliumNetworkPolicy".to_string()));
        assert!(kinds.contains(&"Gateway".to_string()));
        assert!(kinds.contains(&"HTTPRoute".to_string()));
    }

    // ── HTTPRoute :politicas :timeout overlay ────────────────────────────

    fn httproute_rules(docs: &[serde_yaml::Value]) -> Vec<serde_yaml::Value> {
        docs.iter()
            .find(|d| d.get("kind").and_then(|k| k.as_str()) == Some("HTTPRoute"))
            .and_then(|d| d.get("spec"))
            .and_then(|s| s.get("rules"))
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
                .get("timeouts")
                .and_then(|t| t.as_mapping())
                .expect("rule must carry timeouts mapping when :politicas :timeout is set");
            assert_eq!(
                timeouts
                    .get(serde_yaml::Value::String("request".into()))
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
                rule.get("timeouts").is_none(),
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
                .get("timeouts")
                .and_then(|t| t.get("request"))
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
                rule.get("timeouts")
                    .and_then(|t| t.get("request"))
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
                rule.get("timeouts")
                    .and_then(|t| t.get("request"))
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
            assert!(m.contains_key(serde_yaml::Value::String("matches".into())));
            assert!(m.contains_key(serde_yaml::Value::String("backendRefs".into())));
            assert!(m.contains_key(serde_yaml::Value::String("timeouts".into())));
            assert!(m.contains_key(serde_yaml::Value::String("retry".into())));
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
                .get("retry")
                .and_then(|r| r.as_mapping())
                .expect("rule must carry retry mapping when :politicas :retries is set");
            assert_eq!(
                retry
                    .get(serde_yaml::Value::String("attempts".into()))
                    .and_then(|v| v.as_u64()),
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
                rule.get("retry").is_none(),
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
                .get("retry")
                .and_then(|r| r.get("attempts"))
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
                rule.get("retry")
                    .and_then(|r| r.get("attempts"))
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
                .get("retry")
                .and_then(|r| r.get("attempts"))
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
                rule.get("timeouts")
                    .and_then(|t| t.get("request"))
                    .and_then(|v| v.as_str()),
                Some("15s")
            );
            assert!(
                rule.get("retry").is_none(),
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
                rule.get("timeouts").is_none(),
                "timeouts: must be absent when only :retries is set"
            );
            assert_eq!(
                rule.get("retry")
                    .and_then(|r| r.get("attempts"))
                    .and_then(|v| v.as_u64()),
                Some(2)
            );
        }
    }

    // ── CiliumNetworkPolicy :politicas :mtls-required overlay ────────────

    fn cnp_ingress_rules(docs: &[serde_yaml::Value]) -> Vec<serde_yaml::Value> {
        docs.iter()
            .filter(|d| d.get("kind").and_then(|k| k.as_str()) == Some("CiliumNetworkPolicy"))
            .filter_map(|d| {
                d.get("spec")
                    .and_then(|s| s.get("ingress"))
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
                .get("authentication")
                .and_then(|a| a.as_mapping())
                .expect("rule must carry authentication mapping when :mtls-required is set");
            assert_eq!(
                auth.get(serde_yaml::Value::String("mode".into()))
                    .and_then(|v| v.as_str()),
                Some("required")
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
                rule.get("authentication").is_none(),
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
                .get("authentication")
                .and_then(|a| a.as_mapping())
                .expect("rule must carry authentication mapping for explicit :mtls-required nil");
            assert_eq!(
                auth.get(serde_yaml::Value::String("mode".into()))
                    .and_then(|v| v.as_str()),
                Some("disabled")
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
                rule.get("authentication")
                    .and_then(|a| a.get("mode"))
                    .and_then(|v| v.as_str()),
                Some("required"),
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
            assert!(m.contains_key(serde_yaml::Value::String("fromEndpoints".into())));
            assert!(m.contains_key(serde_yaml::Value::String("toPorts".into())));
            assert!(m.contains_key(serde_yaml::Value::String("authentication".into())));
            // The auth block must not leak inside fromEndpoints[] or
            // toPorts[] — guards the Cilium-side schema contract that
            // mutual-auth is an ingress-rule-level concern.
            let from = m
                .get(serde_yaml::Value::String("fromEndpoints".into()))
                .and_then(|f| f.as_sequence())
                .expect("fromEndpoints sequence");
            for fe in from {
                assert!(
                    fe.get("authentication").is_none(),
                    "authentication must not nest inside fromEndpoints[]"
                );
            }
            let to = m
                .get(serde_yaml::Value::String("toPorts".into()))
                .and_then(|t| t.as_sequence())
                .expect("toPorts sequence");
            for tp in to {
                assert!(
                    tp.get("authentication").is_none(),
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
        let nats_policy = policies
            .iter()
            .find(|p| {
                p.get("metadata")
                    .and_then(|m| m.get("name"))
                    .and_then(|n| n.as_str())
                    == Some("checkout-payment-to-cart")
            })
            .expect("pubsub CNP present");
        let rule = nats_policy
            .get("spec")
            .and_then(|s| s.get("ingress"))
            .and_then(|i| i.as_sequence())
            .and_then(|s| s.first())
            .expect("ingress[0]");
        assert_eq!(
            rule.get("authentication")
                .and_then(|a| a.get("mode"))
                .and_then(|v| v.as_str()),
            Some("required")
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
        let cart_to_payment = policies
            .iter()
            .find(|p| {
                p.get("metadata")
                    .and_then(|m| m.get("name"))
                    .and_then(|n| n.as_str())
                    == Some("checkout-cart-to-payment")
            })
            .expect("cart→payment CNP present");
        let port_value = cart_to_payment
            .get("spec")
            .and_then(|s| s.get("ingress"))
            .and_then(|i| i.as_sequence())
            .and_then(|s| s.first())
            .and_then(|i| i.get("toPorts"))
            .and_then(|t| t.as_sequence())
            .and_then(|s| s.first())
            .and_then(|tp| tp.get("ports"))
            .and_then(|p| p.as_sequence())
            .and_then(|s| s.first())
            .and_then(|p| p.get("port"))
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
                .get("authentication")
                .and_then(|a| a.get("mode"))
                .expect("authentication.mode present");
            assert!(
                mode.is_string(),
                "authentication.mode must be a YAML string (got: {mode:?})"
            );
        }
    }
}
