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

use caixa_core::{aplicacao::AplicacaoSpec, Caixa, CaixaKind};
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
}
