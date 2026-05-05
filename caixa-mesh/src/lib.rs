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
//!   2. **Cilium NetworkPolicy** — one per `:contratos`, identity-based
//!      L7 allow-list (M3.x next)
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
    Caixa, CaixaKind, LABEL_APLICACAO, LABEL_CONTRATO, WitTarget, aplicacao::AplicacaoSpec,
    kube_resource_skeleton, pleme_program_in_aplicacao_selector, pleme_program_selector,
    yaml_string_mapping,
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
/// doesn't pin one. Mirrors `caixa_flux::DEFAULT_NAMESPACE`.
pub const DEFAULT_NAMESPACE: &str = "tatara-system";

// ── Cilium NetworkPolicy emission ──────────────────────────────────────

/// Render one [`CiliumNetworkPolicy`-shaped][cnp] YAML per `:contratos`
/// edge. The policy whitelists the `:de → :para` flow at L4 (every
/// contract); HTTP contracts add L7 rules (method + path) keyed by the
/// `:wit` shape.
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
    let mtls_overlay: Option<serde_yaml::Value> = spec.politicas.mtls_required.map(|required| {
        let mut a = serde_yaml::Mapping::new();
        a.insert(
            serde_yaml::Value::String("mode".into()),
            serde_yaml::Value::String(if required { "required" } else { "disabled" }.into()),
        );
        serde_yaml::Value::Mapping(a)
    });
    let mut out = Vec::with_capacity(spec.contratos.len());
    for c in &spec.contratos {
        // Policy's own labels — `aplicacao` (which graph) and
        // `contrato` (which typed edge). Keys come from
        // caixa_core::render so a future label-namespace rebrand is a
        // one-line edit, not a search-and-replace across renderers.
        let mut labels = BTreeMap::new();
        labels.insert(LABEL_APLICACAO, caixa.nome.clone());
        labels.insert(LABEL_CONTRATO, format!("{}-to-{}", c.de, c.para));
        // The apiVersion + kind + metadata.{name, namespace, labels}
        // skeleton comes from caixa_core::render::kube_resource_skeleton
        // — same lift as pleme_program_*_selector applied to the K8s-
        // resource axis. Caller adds spec below.
        let mut policy = kube_resource_skeleton(
            "cilium.io/v2",
            "CiliumNetworkPolicy",
            &format!("{}-{}-to-{}", caixa.nome, c.de, c.para),
            namespace,
            labels,
        );

        // spec.endpointSelector — match the destination Servico's
        // identity. Single-axis (program-only) selector; see
        // caixa_core::render::pleme_program_selector for the deliberate
        // intent / safety tradeoff vs. the in-aplicacao variant.
        let mut endpoint_selector = serde_yaml::Mapping::new();
        endpoint_selector.insert(
            serde_yaml::Value::String("matchLabels".into()),
            yaml_string_mapping(pleme_program_selector(&c.para)),
        );

        // ingress[0]: from the source Servico, scoped to this
        // Aplicacao (so a same-named program in a different Aplicacao
        // can't satisfy the rule). Two-axis selector via the
        // canonical helper — call-site reads as intent, not as five
        // hand-written insert() calls.
        let mut from_endpoint = serde_yaml::Mapping::new();
        from_endpoint.insert(
            serde_yaml::Value::String("matchLabels".into()),
            yaml_string_mapping(pleme_program_in_aplicacao_selector(&c.de, &caixa.nome)),
        );
        let mut ingress_rule = serde_yaml::Mapping::new();
        ingress_rule.insert(
            serde_yaml::Value::String("fromEndpoints".into()),
            serde_yaml::Value::Sequence(vec![serde_yaml::Value::Mapping(from_endpoint)]),
        );

        // toPorts — wit-shape-aware. HTTP gets L7 rules; pubsub +
        // store get L4-only (Cilium can't introspect those protocols).
        let mut to_port = serde_yaml::Mapping::new();
        let mut port_entry = serde_yaml::Mapping::new();
        let port = spec
            .entrada
            .as_ref()
            .filter(|e| e.para == c.para)
            .map(|e| e.port)
            .unwrap_or(8080);
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
        ingress_rule.insert(
            serde_yaml::Value::String("toPorts".into()),
            serde_yaml::Value::Sequence(vec![serde_yaml::Value::Mapping(to_port)]),
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
            serde_yaml::Value::Mapping(endpoint_selector),
        );
        policy_spec.insert(
            serde_yaml::Value::String("ingress".into()),
            serde_yaml::Value::Sequence(vec![serde_yaml::Value::Mapping(ingress_rule)]),
        );
        policy.insert(
            serde_yaml::Value::String("spec".into()),
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
        "gateway.networking.k8s.io/v1",
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
        serde_yaml::Value::String("spec".into()),
        serde_yaml::Value::Mapping(g_spec),
    );

    // HTTPRoute — all paths route to the entrada.para Servico. Same
    // skeleton lift as Gateway above; caller adds spec.
    let mut route = kube_resource_skeleton(
        "gateway.networking.k8s.io/v1",
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
    let timeout_overlay: Option<serde_yaml::Value> = spec.politicas.timeout.map(|d| {
        let mut t = serde_yaml::Mapping::new();
        t.insert(
            serde_yaml::Value::String("request".into()),
            serde_yaml::Value::String(caixa_core::supervisor::duration_codec::render(d)),
        );
        serde_yaml::Value::Mapping(t)
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
    let retry_overlay: Option<serde_yaml::Value> = spec.politicas.retries.map(|attempts| {
        let mut r = serde_yaml::Mapping::new();
        r.insert(
            serde_yaml::Value::String("attempts".into()),
            serde_yaml::Value::Number(attempts.into()),
        );
        serde_yaml::Value::Mapping(r)
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
        serde_yaml::Value::String("spec".into()),
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

    #[test]
    fn typed_view_returns_validated_spec() {
        let spec = typed_view(&aplicacao_caixa()).unwrap();
        assert_eq!(spec.membros.len(), 3);
        assert_eq!(spec.contratos.len(), 2);
        assert!(spec.entrada.is_some());
        assert_eq!(spec.placement.clusters.len(), 2);
    }

    #[test]
    fn cilium_emits_one_policy_per_contrato() {
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
