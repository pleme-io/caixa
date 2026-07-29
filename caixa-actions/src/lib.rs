//! caixa-actions — typed renderer for `:kind Acao` caixas.
//!
//! An `Acao` caixa's sole payload is its `:ci` slot — a
//! [`canteiro_types::CiRun`] (CANTEIRO §7.1-C, `pleme-io/sui`'s
//! `canteiro-types` crate): a repo's CI run as a flat list of typed
//! [`canteiro_types::CiNode`]s plus their `deps` edges.
//!
//! ## M0 contract — validate-only
//!
//! [`validate`] is this crate's *only* entry point today. It gates the
//! input on `:kind Acao`, then hands the declared `:ci` to
//! [`canteiro_types::decompose`] — the same DAG-construction morphism
//! canteiro's own execution engine runs — and turns a successful
//! decomposition into a small typed [`RenderedAcao`] artifact (the
//! topological node order + the edge count). An illegal `CiRun` shape
//! (a duplicate node name, a dependency on an undeclared node, a
//! dependency cycle) surfaces as a typed [`Error::Decompose`], never a
//! silently-accepted bad DAG.
//!
//! This crate deliberately does **not** emit a GitHub Actions workflow
//! (`emit_gha` in canteiro-speak). That emitter lives in
//! `sui-supercacheci::canteiro`'s heavier execution tree (tokio,
//! shigoto-scheduler, axum, sea-orm) — pulling it in here would defeat
//! the entire point of the `canteiro-types` lean-crate fork (CANTEIRO
//! §7.1-C, option B): a caixa consumer that wants only the pure,
//! wire-serializable types + DAG algebra, not canteiro's execution
//! runtime. Emitting a real `.github/workflows/*.yml` from a validated
//! [`RenderedAcao`] is the named next step for this crate (a peer of
//! [`caixa_flux`]/[`caixa_helm`]'s `render_*` functions), tracked but
//! **not built** in this pass.

use caixa_core::{Caixa, CaixaKind, CiDecomposeFailure};
use canteiro_types::DecomposeError;
use thiserror::Error;

/// Errors `caixa-actions` can raise.
#[derive(Debug, Error)]
pub enum Error {
    /// The caixa's `:kind` isn't `Acao` — this renderer only validates
    /// `:kind Acao` caixas' `:ci` slot. Wraps [`caixa_core::KindMismatch`]
    /// so the diagnostic names the offending caixa's `:nome`, shared
    /// verbatim with `caixa-helm`/`caixa-flux`/`caixa-mesh`.
    #[error("{0}")]
    NotAnAcao(#[from] caixa_core::KindMismatch),
    /// The caixa's `:kind` is `Acao` but it declares no `:ci` slot —
    /// the layout invariant [`caixa_core::LayoutError::MissingCi`]
    /// catches this at `feira build` time too; this is the renderer's
    /// own independent gate for callers that invoke [`validate`]
    /// without having run [`caixa_core::LayoutInvariants::verify`]
    /// first. Wraps [`caixa_core::MissingCiSlot`] via `#[from]` so the
    /// diagnostic naming the offending caixa's `:nome` shares one
    /// typed view with every other per-`Acao` consumer that reaches
    /// for the sibling [`caixa_core::require_ci`] predicate — matching
    /// the peer [`Self::NotAnAcao`] `#[from]` shape on the
    /// [`caixa_core::KindMismatch`] view above.
    #[error("{0}")]
    MissingCi(#[from] caixa_core::MissingCiSlot),
    /// The declared `:ci` run is structurally illegal — a duplicate
    /// node name, a dependency on an undeclared node, or a dependency
    /// cycle. Wraps [`caixa_core::CiDecomposeFailure`] via `#[from]`
    /// so the diagnostic naming the offending caixa's `:nome` +
    /// carrying the borrowed [`canteiro_types::DecomposeError`] source
    /// shares one typed view with every other per-`Acao` consumer that
    /// runs [`canteiro_types::decompose`] on a borrowed `:ci` slot —
    /// matching the peer [`Self::NotAnAcao`] / [`Self::MissingCi`]
    /// `#[from]` shape on the sibling [`caixa_core::KindMismatch`] /
    /// [`caixa_core::MissingCiSlot`] typed views above.
    #[error("{0}")]
    Decompose(#[from] CiDecomposeFailure),
}

/// The validated artifact [`validate`] returns on success — a minimal
/// typed reflection of the decomposed DAG, not a rendered workflow (see
/// crate docs for why full GitHub Actions emission is deliberately out
/// of scope for M0).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RenderedAcao {
    /// The caixa's `:nome`, carried through so a consumer that holds
    /// only a [`RenderedAcao`] can still name which caixa it came from.
    pub nome: String,
    /// Every declared node's name, in topological order (parents
    /// before children — [`canteiro_types::CanteiroDag::topo_order`]).
    /// A node that depends on another always appears strictly after it.
    pub node_names_topo: Vec<String>,
    /// Total declared dependency edges across every node (the sum of
    /// each [`canteiro_types::CiNode::deps`] length) — one per `deps`
    /// entry, exactly as [`canteiro_types::decompose`] wires them.
    pub edge_count: usize,
}

/// Validate a `:kind Acao` caixa's `:ci` slot by decomposing it into a
/// shigoto DAG via [`canteiro_types::decompose`].
///
/// Gate order mirrors every other per-kind `caixa-<target>` renderer's
/// entry-point convention ([`caixa_core::require_kind`] first, naming
/// the offending caixa on a kind mismatch before spending a diagnostic
/// on the `:ci` slot's own shape):
///
/// 1. `caixa.kind() == CaixaKind::Acao` ([`Error::NotAnAcao`]).
/// 2. `caixa.ci()` is present ([`Error::MissingCi`]).
/// 3. The declared [`CiRun`] decomposes cleanly ([`Error::Decompose`]).
///
/// # Errors
///
/// See the [`Error`] variants above — each names the offending caixa's
/// `:nome` and the specific axis that failed.
pub fn validate(caixa: &Caixa) -> Result<RenderedAcao, Error> {
    caixa_core::require_kind(caixa, CaixaKind::Acao)?;
    let ci = caixa_core::require_ci(caixa)?;
    let cd = caixa_core::decompose_ci(caixa, ci)?;
    let nome = caixa.nome().to_string();

    let topo = cd.topo_order().map_err(|_| CiDecomposeFailure {
        nome: nome.clone(),
        source: DecomposeError::Cycle,
    })?;

    let node_names_topo = topo
        .iter()
        .filter_map(|id| cd.nodes.get(id).map(|n| n.name.clone()))
        .collect();
    let edge_count = ci.nodes.iter().map(|n| n.deps.len()).sum();

    Ok(RenderedAcao {
        nome,
        node_names_topo,
        edge_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use canteiro_types::{ActionRef, CiNode, CiRun, EnvClass};

    fn action(name: &str) -> ActionRef {
        ActionRef {
            name: name.to_string(),
            command: "true".to_string(),
            args: vec![],
        }
    }

    fn acao_caixa(ci: Option<CiRun>) -> Caixa {
        let mut c = Caixa::from_lisp(
            r#"(defcaixa
                  :nome "hello-acao"
                  :versao "0.1.0"
                  :kind Acao)"#,
        )
        .expect("minimal Acao caixa parses");
        c.ci = ci;
        c
    }

    #[test]
    fn validate_decomposes_a_two_node_build_then_test_run() {
        let ci = CiRun {
            workspace: "pleme-io".to_string(),
            repo: "caixa".to_string(),
            nodes: vec![
                CiNode::new("build", EnvClass::None, action("build"), vec![]),
                CiNode::new(
                    "test",
                    EnvClass::None,
                    action("test"),
                    vec!["build".to_string()],
                ),
            ],
        };
        let c = acao_caixa(Some(ci));

        let rendered = validate(&c).expect("valid two-node CiRun decomposes");

        assert_eq!(rendered.nome, "hello-acao");
        assert_eq!(rendered.edge_count, 1);
        // Topological order: build's dependent (test) never precedes it.
        let build_ix = rendered
            .node_names_topo
            .iter()
            .position(|n| n == "build")
            .expect("build present in topo order");
        let test_ix = rendered
            .node_names_topo
            .iter()
            .position(|n| n == "test")
            .expect("test present in topo order");
        assert!(
            build_ix < test_ix,
            "build must precede test in topological order: {:?}",
            rendered.node_names_topo
        );
    }

    #[test]
    fn validate_rejects_a_cyclic_ci_run() {
        let ci = CiRun {
            workspace: "pleme-io".to_string(),
            repo: "caixa".to_string(),
            nodes: vec![
                CiNode::new("a", EnvClass::None, action("a"), vec!["b".to_string()]),
                CiNode::new("b", EnvClass::None, action("b"), vec!["a".to_string()]),
            ],
        };
        let c = acao_caixa(Some(ci));

        let err = validate(&c).expect_err("cyclic CiRun must be rejected");

        match err {
            Error::Decompose(failure) => {
                assert_eq!(failure.nome, "hello-acao");
                assert_eq!(failure.source, DecomposeError::Cycle);
            }
            other => panic!("expected Error::Decompose(Cycle), got {other:?}"),
        }
    }

    #[test]
    fn validate_decompose_gate_wraps_caixa_core_typed_view() {
        // Fail-before-pass-after pin on the [`Error::Decompose`]
        // `#[from]` on [`caixa_core::CiDecomposeFailure`]: pre-lift the
        // arm carried the `nome: String, #[source] source: DecomposeError`
        // shape open-coded at this crate's own [`validate`] call site,
        // constructed twice at each `.map_err(...)` site with no
        // compile-time link to any typed named-caixa view the sibling
        // per-`Acao` diagnostic axes carry. Converging the arm on the
        // substrate-canonical [`caixa_core::CiDecomposeFailure`] typed
        // view closes the drift potential structurally: every future
        // per-`Acao` consumer that runs [`canteiro_types::decompose`]
        // on a borrowed `:ci` slot (the deferred
        // `sui-supercacheci::canteiro::emit_gha` workflow renderer named
        // in this crate's own docs, the future per-`Acao` CR
        // materializer) reaches for the same typed view + `#[from]` arm
        // and gets the diagnostic-naming-the-offending-caixa contract
        // for free — matching the peer [`Error::NotAnAcao`] `#[from]`
        // on [`caixa_core::KindMismatch`] and [`Error::MissingCi`]
        // `#[from]` on [`caixa_core::MissingCiSlot`] shapes above.
        let ci = CiRun {
            workspace: "pleme-io".to_string(),
            repo: "caixa".to_string(),
            nodes: vec![
                CiNode::new("a", EnvClass::None, action("a"), vec!["b".to_string()]),
                CiNode::new("b", EnvClass::None, action("b"), vec!["a".to_string()]),
            ],
        };
        let c = acao_caixa(Some(ci));

        let err = validate(&c).expect_err("cyclic CiRun must be rejected");

        match err {
            Error::Decompose(failure) => {
                // The typed view carries the offending caixa's :nome
                // verbatim, byte-identical to what the lifted
                // [`caixa_core::Caixa::nome`] accessor returns for the
                // same fixture.
                assert_eq!(failure.nome, c.nome());
                // The Display impl routes through the caixa_core-owned
                // CiDecomposeFailure Display, so a future format edit
                // lands in exactly one place (caixa-core::render), not
                // duplicated across every per-`Acao` consumer.
                let msg = format!("{failure}");
                assert!(
                    msg.contains("hello-acao"),
                    "CiDecomposeFailure Display must name the offending \
                     caixa's :nome (got: {msg:?})"
                );
                assert!(
                    msg.contains(":ci"),
                    "CiDecomposeFailure Display must name the `:ci` slot \
                     the decompose failed on (got: {msg:?})"
                );
                // The `#[source]`-wired `DecomposeError` carrier is
                // reachable through the standard `std::error::Error::source()`
                // trait method, so downstream diagnostic frameworks
                // (`anyhow`'s chain formatter, `tracing`'s `error!` event
                // capture) walk the underlying arm rather than only
                // seeing the flattened Display bytes — same shape every
                // peer per-axis `#[source]`-carrying view exposes.
                let src = std::error::Error::source(&failure).expect(
                    "CiDecomposeFailure must expose its DecomposeError via Error::source()",
                );
                assert_eq!(
                    format!("{src}"),
                    format!("{}", DecomposeError::Cycle),
                    "Error::source() must borrow the underlying \
                     DecomposeError arm verbatim"
                );
            }
            other => panic!("expected Error::Decompose(CiDecomposeFailure), got {other:?}"),
        }
    }

    #[test]
    fn validate_decompose_call_routes_through_caixa_core_decompose_ci_helper() {
        // Fail-before-pass-after pin on the [`validate`] first-axis
        // `.map_err` on [`canteiro_types::decompose`]: pre-lift the arm
        // constructed a hand-authored `CiDecomposeFailure { nome:
        // nome.clone(), source }` at this crate's own [`validate`] call
        // site with no compile-time link to any typed named-caixa
        // predicate the sibling per-`Acao` presence-gate axis (lifted
        // at c0eaf3d via [`caixa_core::require_ci`]) carries.
        // Converging the arm on the substrate-canonical
        // [`caixa_core::decompose_ci`] predicate (peer of the sibling
        // [`caixa_core::require_ci`] presence gate on the per-`Acao`
        // `:ci`-slot diagnostic surface) closes the drift potential
        // structurally: every future per-`Acao` consumer that runs
        // [`canteiro_types::decompose`] on a borrowed `:ci` slot reaches
        // for one substrate primitive + `#[from]` arm and gets the
        // diagnostic-naming-the-offending-caixa contract for free —
        // matching the peer [`Error::MissingCi`] `#[from]` on
        // [`caixa_core::MissingCiSlot`] via [`caixa_core::require_ci`]
        // shape three lines above.
        //
        // Byte-for-byte parity assertion: [`validate`]'s first-axis
        // diagnostic (on the same fixture) must equal the
        // [`caixa_core::decompose_ci`] primitive's diagnostic —
        // otherwise the crate's own [`validate`] has drifted from the
        // substrate primitive and re-inlined the wrap. Trips at the
        // caller's build time, not silently at the diagnostic emission
        // site.
        let ci = CiRun {
            workspace: "pleme-io".to_string(),
            repo: "caixa".to_string(),
            nodes: vec![
                CiNode::new("a", EnvClass::None, action("a"), vec!["b".to_string()]),
                CiNode::new("b", EnvClass::None, action("b"), vec!["a".to_string()]),
            ],
        };
        let c = acao_caixa(Some(ci));
        let via_validate = validate(&c).expect_err("cyclic CiRun must be rejected");
        let ci_borrowed = c.ci().expect("fixture carries :ci");
        let via_primitive = caixa_core::decompose_ci(&c, ci_borrowed)
            .expect_err("cyclic CiRun must be rejected by decompose_ci");
        match via_validate {
            Error::Decompose(failure) => {
                assert_eq!(
                    failure.nome, via_primitive.nome,
                    "validate's decompose-axis :nome must equal \
                     caixa_core::decompose_ci's :nome on the same fixture — \
                     otherwise validate has re-inlined the CiDecomposeFailure \
                     construction and lost the substrate primitive"
                );
                assert_eq!(
                    failure.source, via_primitive.source,
                    "validate's decompose-axis DecomposeError arm must equal \
                     caixa_core::decompose_ci's DecomposeError arm on the same \
                     fixture — pins the pass-through-on-error contract"
                );
                let via_validate_msg = format!("{failure}");
                let via_primitive_msg = format!("{via_primitive}");
                assert_eq!(
                    via_validate_msg, via_primitive_msg,
                    "validate's decompose-axis Display bytes must equal \
                     caixa_core::decompose_ci's Display bytes — a future \
                     format edit lands in exactly one place \
                     (caixa-core::render::CiDecomposeFailure), not \
                     duplicated across every per-`Acao` consumer"
                );
            }
            other => panic!("expected Error::Decompose, got {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_missing_ci_slot() {
        let c = acao_caixa(None);

        let err = validate(&c).expect_err("Acao caixa with no :ci must be rejected");

        match err {
            Error::MissingCi(missing) => assert_eq!(missing.nome, "hello-acao"),
            other => panic!("expected Error::MissingCi, got {other:?}"),
        }
    }

    #[test]
    fn validate_missing_ci_gate_wraps_caixa_core_typed_view() {
        // Fail-before-pass-after pin on the [`Error::MissingCi`]
        // `#[from]` on [`caixa_core::MissingCiSlot`]: pre-lift the
        // gate constructed a hand-authored `Error::MissingCi { nome:
        // String }` at this crate's own [`validate`] call site with no
        // compile-time link to any typed named-caixa view the sibling
        // per-renderer entry-gate axes carry. Converging the arm on
        // the substrate-canonical [`caixa_core::MissingCiSlot`] typed
        // view (constructed by the peer [`caixa_core::require_ci`]
        // predicate) closes the drift potential structurally: every
        // future per-`Acao` consumer (the deferred
        // `sui-supercacheci::canteiro::emit_gha` workflow renderer
        // named in this crate's own docs, the future per-`Acao` CR
        // materializer) reaches for the same one-liner + `#[from]` arm
        // and gets the diagnostic-naming-the-offending-caixa contract
        // for free — matching the peer [`Error::NotAnAcao`] `#[from]`
        // on [`caixa_core::KindMismatch`] shape.
        let c = acao_caixa(None);
        let err = validate(&c).expect_err("Acao caixa with no :ci must be rejected");
        match err {
            Error::MissingCi(missing) => {
                // The typed view carries the offending caixa's :nome
                // verbatim, byte-identical to what
                // `caixa_core::require_ci` constructed via the lifted
                // `Caixa::nome` accessor's `.to_string()` extension.
                assert_eq!(missing.nome, c.nome());
                // The Display impl routes through the caixa_core-owned
                // MissingCiSlot Display, so a future format edit lands
                // in exactly one place (caixa-core::render), not
                // duplicated across every per-Acao consumer.
                let msg = format!("{missing}");
                assert!(
                    msg.contains("hello-acao"),
                    "MissingCiSlot Display must name the offending caixa's :nome \
                     (got: {msg:?})"
                );
                assert!(
                    msg.contains(":ci"),
                    "MissingCiSlot Display must name the missing `:ci` slot \
                     (got: {msg:?})"
                );
            }
            other => panic!("expected Error::MissingCi(MissingCiSlot), got {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_non_acao_kind() {
        let mut c = acao_caixa(None);
        c.kind = CaixaKind::Servico;
        c.servicos = vec!["servicos/hello.computeunit.yaml".to_string()];

        let err = validate(&c).expect_err("non-Acao kind must be rejected");

        match err {
            Error::NotAnAcao(mismatch) => {
                assert_eq!(mismatch.expected, CaixaKind::Acao);
                assert_eq!(mismatch.actual, CaixaKind::Servico);
            }
            other => panic!("expected Error::NotAnAcao, got {other:?}"),
        }
    }
}
