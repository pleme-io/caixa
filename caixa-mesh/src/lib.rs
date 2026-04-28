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

use caixa_core::{Caixa, CaixaKind, WitTarget, aplicacao::AplicacaoSpec};
use thiserror::Error;

/// Errors caixa-mesh can raise.
#[derive(Debug, Error)]
pub enum Error {
    #[error("caixa :kind must be Aplicacao for caixa-mesh rendering, got {0:?}")]
    NotAnAplicacao(CaixaKind),
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
    if caixa.kind != CaixaKind::Aplicacao {
        return Err(Error::NotAnAplicacao(caixa.kind));
    }
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
    if caixa.kind != CaixaKind::Aplicacao {
        return Err(Error::NotAnAplicacao(caixa.kind));
    }
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
    let mut out = Vec::with_capacity(spec.contratos.len());
    for c in &spec.contratos {
        let mut policy = serde_yaml::Mapping::new();
        policy.insert(
            serde_yaml::Value::String("apiVersion".into()),
            serde_yaml::Value::String("cilium.io/v2".into()),
        );
        policy.insert(
            serde_yaml::Value::String("kind".into()),
            serde_yaml::Value::String("CiliumNetworkPolicy".into()),
        );
        let mut metadata = serde_yaml::Mapping::new();
        metadata.insert(
            serde_yaml::Value::String("name".into()),
            serde_yaml::Value::String(format!("{}-{}-to-{}", caixa.nome, c.de, c.para)),
        );
        metadata.insert(
            serde_yaml::Value::String("namespace".into()),
            serde_yaml::Value::String(namespace.into()),
        );
        let mut labels = serde_yaml::Mapping::new();
        labels.insert(
            serde_yaml::Value::String("pleme.pleme.io/aplicacao".into()),
            serde_yaml::Value::String(caixa.nome.clone()),
        );
        labels.insert(
            serde_yaml::Value::String("pleme.pleme.io/contrato".into()),
            serde_yaml::Value::String(format!("{}-to-{}", c.de, c.para)),
        );
        metadata.insert(
            serde_yaml::Value::String("labels".into()),
            serde_yaml::Value::Mapping(labels),
        );
        policy.insert(
            serde_yaml::Value::String("metadata".into()),
            serde_yaml::Value::Mapping(metadata),
        );

        // spec.endpointSelector — match the destination Servico
        let mut endpoint_selector = serde_yaml::Mapping::new();
        let mut match_labels = serde_yaml::Mapping::new();
        match_labels.insert(
            serde_yaml::Value::String("pleme.pleme.io/program".into()),
            serde_yaml::Value::String(c.para.clone()),
        );
        endpoint_selector.insert(
            serde_yaml::Value::String("matchLabels".into()),
            serde_yaml::Value::Mapping(match_labels),
        );

        // ingress[0]: from the source Servico in the same Aplicacao
        let mut from_match = serde_yaml::Mapping::new();
        from_match.insert(
            serde_yaml::Value::String("pleme.pleme.io/program".into()),
            serde_yaml::Value::String(c.de.clone()),
        );
        from_match.insert(
            serde_yaml::Value::String("pleme.pleme.io/aplicacao".into()),
            serde_yaml::Value::String(caixa.nome.clone()),
        );
        let mut from_endpoint = serde_yaml::Mapping::new();
        from_endpoint.insert(
            serde_yaml::Value::String("matchLabels".into()),
            serde_yaml::Value::Mapping(from_match),
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

    // Gateway
    let mut gateway = serde_yaml::Mapping::new();
    gateway.insert(
        serde_yaml::Value::String("apiVersion".into()),
        serde_yaml::Value::String("gateway.networking.k8s.io/v1".into()),
    );
    gateway.insert(
        serde_yaml::Value::String("kind".into()),
        serde_yaml::Value::String("Gateway".into()),
    );
    let mut g_meta = serde_yaml::Mapping::new();
    g_meta.insert(
        serde_yaml::Value::String("name".into()),
        serde_yaml::Value::String(caixa.nome.clone()),
    );
    g_meta.insert(
        serde_yaml::Value::String("namespace".into()),
        serde_yaml::Value::String(namespace.into()),
    );
    gateway.insert(
        serde_yaml::Value::String("metadata".into()),
        serde_yaml::Value::Mapping(g_meta),
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

    // HTTPRoute — all paths route to the entrada.para Servico.
    let mut route = serde_yaml::Mapping::new();
    route.insert(
        serde_yaml::Value::String("apiVersion".into()),
        serde_yaml::Value::String("gateway.networking.k8s.io/v1".into()),
    );
    route.insert(
        serde_yaml::Value::String("kind".into()),
        serde_yaml::Value::String("HTTPRoute".into()),
    );
    let mut r_meta = serde_yaml::Mapping::new();
    r_meta.insert(
        serde_yaml::Value::String("name".into()),
        serde_yaml::Value::String(format!("{}-{}", caixa.nome, entrada.para)),
    );
    r_meta.insert(
        serde_yaml::Value::String("namespace".into()),
        serde_yaml::Value::String(namespace.into()),
    );
    route.insert(
        serde_yaml::Value::String("metadata".into()),
        serde_yaml::Value::Mapping(r_meta),
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
        Caixa, CaixaKind, Entrada, Membro, MeshPolicy, Placement, PlacementStrategy, WitContract,
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
            assert!(endpoint.get("pleme.pleme.io/program").is_some());
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
                from.get("pleme.pleme.io/aplicacao")
                    .and_then(|v| v.as_str()),
                Some("checkout")
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
}
