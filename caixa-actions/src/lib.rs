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

use caixa_core::{Caixa, CiDecomposeFailure};
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
    ///
    /// Every [`CiDecomposeFailure`] this arm carries is constructed by
    /// exactly one substrate primitive — [`caixa_core::decompose_ci`]
    /// at `caixa-core/src/render.rs:342`. [`validate`] holds no `Err(_)`
    /// path that constructs a [`CiDecomposeFailure`] on its own; the
    /// only downstream call that could raise on the decompose axis is
    /// the sibling `?` on `decompose_ci`'s result, which passes the
    /// substrate-constructed view through unchanged. That leaves the
    /// substrate primitive as the sole author of the failure's `:nome`
    /// projection + `#[source] DecomposeError` arm, so a future edit to
    /// the diagnostic's shape (naming, source-arm carrying, byte-form)
    /// lands in exactly one place and propagates to every consumer.
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
    // Compound V0 per-`Acao` entry gate: gates the input on
    // `:kind Acao` + `:ci`-slot presence + `canteiro_types::decompose`
    // in one substrate primitive, matching the peer per-Servico
    // [`caixa_core::require_v0_servico_shape`] and per-Aplicacao
    // [`caixa_core::require_aplicacao_view`] compound gates. Every
    // per-`Acao` consumer downstream (the deferred
    // `sui-supercacheci::canteiro::emit_gha` workflow renderer named
    // in this crate's own docs, the future per-`Acao` CR
    // materializer's admission webhook, a future `feira validate
    // --acao` per-caixa admission verb) reaches for the same
    // one-liner + three `#[from]` arms and gets the diagnostic-
    // naming-the-offending-caixa contract for free.
    let (ci, cd) = caixa_core::require_acao_view::<Error>(caixa)?;

    // Route the per-`RenderedAcao::node_names_topo` topological-order
    // node-name projection through the substrate-canonical
    // [`caixa_core::ci_topo_node_names`] primitive rather than the
    // three-line `let topo = cd.topo_order().expect(...); topo.iter()
    // .filter_map(|id| cd.nodes.get(id).map(|n| n.name.clone()))
    // .collect()` open-coded chain. Peer of the sibling
    // [`caixa_core::ci_declared_edge_count`] `usize`-scalar projection
    // on the borrowed [`canteiro_types::CiRun`] axis, extended here
    // onto the owned [`canteiro_types::CanteiroDag`] axis — every
    // future per-`Acao` consumer that materializes a
    // `RenderedAcao`-shaped view reads through the paired substrate
    // one-liners rather than re-inlining the topo + filter_map + collect
    // chain. Substrate primitive owns the panic message on the
    // unreachable-post-`decompose_ci` `Err(_)` arm so a future
    // contract-regression diagnosis surfaces at one authored site.
    let node_names_topo = caixa_core::ci_topo_node_names(&cd);
    let edge_count = caixa_core::ci_declared_edge_count(ci);

    Ok(RenderedAcao {
        nome: caixa.nome().to_string(),
        node_names_topo,
        edge_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use caixa_core::CaixaKind;
    use canteiro_types::{ActionRef, CiNode, CiRun, DecomposeError, EnvClass};

    /// Per-fixture triple every table-driven CI-shape test in this
    /// module keys off — `(label, builder, expected-node-count)`. Names
    /// the shape once so per-test callers read as intent (`fixtures:
    /// [ValidateAcaoFixture; N]`) rather than the four-token positional
    /// declaration (`fn() -> CiRun` inside a tuple inside an array),
    /// and closes the sole `clippy::type_complexity` warning on this
    /// file — the last unlifted per-fixture-shape row on the caixa-
    /// actions test surface. The `fn() -> CiRun` builder-arm shape (not
    /// a `&'static CiRun` reference, not a boxed `Box<dyn Fn(…)>`)
    /// preserves the "two independent instances per fixture — one
    /// moved into `acao_caixa`, one borrowed by `decompose_ci`" pin
    /// documented on the sole call site, which pins that the fixture
    /// carries no assumption about `CiRun: Clone`. Named
    /// `ValidateAcaoFixture` to match the observable it drives
    /// ([`validate`], the crate's sole public entry point).
    type ValidateAcaoFixture = (&'static str, fn() -> CiRun, usize);

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
    fn validate_topo_order_axis_is_pass_through_after_decompose_ci_gate() {
        // Fail-before-pass-after pin on the post-`decompose_ci`
        // `topo_order()` axis: pre-lift, [`validate`] carried an inline
        // `.map_err(|_| CiDecomposeFailure { nome: nome.clone(), source:
        // DecomposeError::Cycle })?` on this axis — the last surviving
        // [`CiDecomposeFailure`] constructor outside
        // [`caixa_core::decompose_ci`] (the substrate primitive at
        // caixa-core/src/render.rs:342). That construction re-inlined
        // the substrate-primitive-only typed view outside its owning
        // primitive — the exact anti-pattern the peer per-`Acao` axes
        // pinned by [`validate_decompose_gate_wraps_caixa_core_typed_view`]
        // and [`validate_missing_ci_gate_wraps_caixa_core_typed_view`]
        // warn against — and, worse, hard-coded the failure kind as
        // `DecomposeError::Cycle` regardless of which underlying arm
        // ([`DecomposeError::DuplicateNodeName`],
        // [`DecomposeError::MissingDependency`], [`DecomposeError::Cycle`])
        // the (unreachable) `topo_order()` `Err` would have carried, so
        // any future substrate-contract regression would surface as a
        // silently misdiagnosed `Cycle` rather than the real cause. The
        // post-lift shape converges on `.expect(...)`, which:
        //
        //   1. leaves [`caixa_core::decompose_ci`] as the sole
        //      [`CiDecomposeFailure`] constructor in the workspace —
        //      matching the peer `Error::MissingCi` /
        //      `Error::NotAnAcao` arms' shape, where every construction
        //      of the wrapped typed view happens exactly once, inside
        //      its owning substrate primitive; and
        //   2. surfaces any future substrate-contract regression as a
        //      panic at the call site, rather than a silently
        //      misdiagnosed `Cycle` diagnostic.
        //
        // This pin asserts the observable side of that contract: for
        // every fixture the substrate primitive
        // [`caixa_core::decompose_ci`] accepts (regardless of DAG shape
        // — linear-chain, diamond, wide-fanout, singleton), [`validate`]
        // must return `Ok(_)` with a fully-populated topological order
        // whose length matches the fixture's declared node count. A
        // future regression that re-inlines a `.map_err(...)` +
        // hand-authored [`CiDecomposeFailure`] on the `topo_order()`
        // axis trips this test on the singleton fixture the moment the
        // decompose gate's contract is misread as fallible, before the
        // drift reaches any downstream per-`Acao` consumer.
        let fixtures: [ValidateAcaoFixture; 3] = [
            (
                "singleton",
                || CiRun {
                    workspace: "pleme-io".to_string(),
                    repo: "caixa".to_string(),
                    nodes: vec![CiNode::new("solo", EnvClass::None, action("solo"), vec![])],
                },
                1_usize,
            ),
            (
                "linear-chain",
                || CiRun {
                    workspace: "pleme-io".to_string(),
                    repo: "caixa".to_string(),
                    nodes: vec![
                        CiNode::new("a", EnvClass::None, action("a"), vec![]),
                        CiNode::new("b", EnvClass::None, action("b"), vec!["a".to_string()]),
                        CiNode::new("c", EnvClass::None, action("c"), vec!["b".to_string()]),
                    ],
                },
                3_usize,
            ),
            (
                "diamond",
                || CiRun {
                    workspace: "pleme-io".to_string(),
                    repo: "caixa".to_string(),
                    nodes: vec![
                        CiNode::new("root", EnvClass::None, action("root"), vec![]),
                        CiNode::new(
                            "left",
                            EnvClass::None,
                            action("left"),
                            vec!["root".to_string()],
                        ),
                        CiNode::new(
                            "right",
                            EnvClass::None,
                            action("right"),
                            vec!["root".to_string()],
                        ),
                        CiNode::new(
                            "sink",
                            EnvClass::None,
                            action("sink"),
                            vec!["left".to_string(), "right".to_string()],
                        ),
                    ],
                },
                4_usize,
            ),
        ];
        for (label, build_ci, expected_node_count) in fixtures {
            // Two independent instances of the same fixture — one
            // handed to acao_caixa (moved into the fixture Caixa's
            // `:ci` slot), one borrowed directly by decompose_ci — so
            // the pin makes no assumption about `CiRun: Clone`.
            let c = acao_caixa(Some(build_ci()));
            let ci_for_primitive = build_ci();
            // The substrate primitive's own contract: decompose_ci
            // accepts this fixture (no duplicate, no missing dep, no
            // cycle) — pinned here so the fixture inventory doubles as
            // a regression pin on the primitive itself.
            let cd = caixa_core::decompose_ci(&c, &ci_for_primitive).unwrap_or_else(|e| {
                panic!(
                    "fixture `{label}` must be accepted by caixa_core::decompose_ci — \
                     the substrate primitive is the pass-through-on-success gate this test \
                     pins the post-decompose axis of; got: {e}"
                )
            });
            // Post-decompose invariant: topo_order() is infallible.
            // Reads the DAG's own algebra directly, no second gate,
            // matching the discipline documented in
            // caixa-core/src/render.rs:28107-28116. Captured once and
            // reused so the pin makes no assumption about topo_order()'s
            // receiver kind (`&self` / `self` / `&mut self`).
            let topo = cd.topo_order().unwrap_or_else(|_| {
                panic!(
                    "fixture `{label}`: caixa_core::decompose_ci returned Ok(_) but \
                     CanteiroDag::topo_order returned Err(_) — the substrate contract \
                     documented at caixa-core/src/render.rs:28107 has regressed; \
                     the pre-lift map_err+CiDecomposeFailure shape would silently \
                     misdiagnose this as DecomposeError::Cycle regardless of the \
                     actual arm, which is precisely the drift this pin closes"
                )
            });
            // The substrate-primitive-projected node names, computed
            // via the exact same accessor chain the post-lift
            // [`validate`] impl uses (topo.iter().filter_map(|id|
            // cd.nodes.get(id).map(|n| n.name.clone())).collect()).
            // Materialized here so the byte-parity assertion below
            // pins that `validate` reads through the CanteiroDag's
            // algebra directly, with no reconstruction path a future
            // edit could drift onto.
            let expected_node_names: Vec<String> = topo
                .iter()
                .filter_map(|id| cd.nodes.get(id).map(|n| n.name.clone()))
                .collect();
            assert_eq!(
                expected_node_names.len(),
                expected_node_count,
                "fixture `{label}`: substrate-projected topo-order node-name list length \
                 must equal declared node count — pins the substrate primitive's \
                 pass-through-on-success contract at the topo_order axis",
            );
            // The observable pin: validate() must return Ok(_) on every
            // fixture decompose_ci accepts — no Err(_) may leak on the
            // topo_order axis. A future regression that re-inlines a
            // `.map_err(|_| CiDecomposeFailure { source: DecomposeError::
            // Cycle, .. })?` and treats topo_order as fallible would
            // still pass on these fixtures today (topo_order is
            // infallible post-decompose), but the byte-parity assertion
            // below on the node-name projection pins the specific
            // shape of the pass-through — a future regression that
            // silently swaps to an `.unwrap_or_default()` or drops a
            // node on the Err arm would trip here.
            let rendered = validate(&c).unwrap_or_else(|e| {
                panic!(
                    "fixture `{label}`: validate() must return Ok on every fixture \
                     caixa_core::decompose_ci accepts — got Err: {e}"
                )
            });
            assert_eq!(
                rendered.node_names_topo, expected_node_names,
                "fixture `{label}`: validate()'s node_names_topo must equal the substrate \
                 primitive's topo-order-projected node names byte-for-byte — pins that \
                 the post-decompose axis reads through the CanteiroDag's algebra directly, \
                 with no reconstruction path a future edit could drift",
            );
        }
    }

    #[test]
    fn validate_node_names_topo_routes_through_caixa_core_ci_topo_node_names_helper() {
        // Fail-before-pass-after pin on the [`validate`]
        // `node_names_topo` field construction axis: pre-lift the
        // function ran the three-line
        //
        //   let topo = cd.topo_order().expect(...);
        //   let node_names_topo = topo.iter()
        //       .filter_map(|id| cd.nodes.get(id).map(|n| n.name.clone()))
        //       .collect();
        //
        // block inline at this crate's own [`validate`] call site — one
        // of five open-coded copies of the same `topo.iter().filter_map
        // (...).collect()` chain across the workspace (the two other
        // per-crate sites the [`caixa_core::ci_topo_node_names`]
        // docstring names + the two `caixa_core::require_acao_view`
        // byte-parity pin sites at `caixa-core/src/render.rs:37769` /
        // `caixa-core/src/render.rs:37773`). Converging the arm on the
        // substrate-canonical [`caixa_core::ci_topo_node_names`]
        // primitive (peer of the sibling
        // [`caixa_core::ci_declared_edge_count`] scalar projection on
        // the borrowed [`canteiro_types::CiRun`] axis) closes the
        // per-`Acao` topological-node-name axis: every future
        // per-`Acao` consumer that materializes a `RenderedAcao`-shaped
        // view reaches for the paired substrate one-liners
        // ([`caixa_core::ci_topo_node_names`] for the topo-order names,
        // [`caixa_core::ci_declared_edge_count`] for the edge count)
        // rather than re-inlining the topo + filter_map + collect chain.
        //
        // Byte-for-byte parity assertion: on every fixture the
        // substrate primitive [`caixa_core::decompose_ci`] accepts,
        // [`validate`]'s returned `node_names_topo` must equal the
        // primitive's direct output on the same DAG. Trips at
        // caixa-actions build time if [`validate`] re-inlines the
        // three-line topo-projection block and drifts from the
        // substrate primitive.
        let ok_fixture = || {
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
            acao_caixa(Some(ci))
        };
        let c = ok_fixture();
        let via_validate = validate(&c).expect("valid two-node Acao decomposes");
        let ci_for_primitive = c.ci().expect("fixture carries :ci");
        let cd = caixa_core::decompose_ci(&c, ci_for_primitive).expect("acyclic run decomposes");
        let via_primitive = caixa_core::ci_topo_node_names(&cd);
        assert_eq!(
            via_validate.node_names_topo, via_primitive,
            "validate's node_names_topo must equal the substrate \
             primitive `caixa_core::ci_topo_node_names`'s output on the \
             same DAG — a future regression that re-inlines the \
             three-line `topo.iter().filter_map(...).collect()` chain \
             breaks the substrate-primitive-routing contract this pin \
             closes"
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
        // `expect_err` is usable again as of sui 96a811e: `CanteiroDag` now
        // derives `Debug`, and so does the `shigoto_dag::Dag` it wraps
        // (shigoto 0.1.12). Before that this whole test TARGET failed to
        // compile, which is worse than a failing test — the suite went
        // absent while the repo still looked green.
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

    #[test]
    fn validate_routes_entry_gate_through_caixa_core_require_acao_view_helper() {
        // Fail-before-pass-after pin on the [`validate`] entry-gate axis:
        // pre-lift the function ran the three-line prelude
        //
        //   caixa_core::require_kind(caixa, CaixaKind::Acao)?;
        //   let ci = caixa_core::require_ci(caixa)?;
        //   let cd = caixa_core::decompose_ci(caixa, ci)?;
        //
        // inline at this crate's own [`validate`] call site — the
        // exact same three-arm prelude the peer per-Servico and
        // per-Aplicacao renderers open-coded before the sibling
        // [`caixa_core::require_v0_servico_shape`] (per-Servico) and
        // [`caixa_core::require_aplicacao_view`] (per-Aplicacao)
        // compound-gate lifts closed the drift potential on those
        // axes. Converging the arm on the substrate-canonical
        // [`caixa_core::require_acao_view`] compound gate closes the
        // per-`Acao` axis: every future per-`Acao` consumer (the
        // deferred `sui-supercacheci::canteiro::emit_gha` workflow
        // renderer named in this crate's own docs, the future
        // per-`Acao` CR materializer's admission webhook, a future
        // `feira validate --acao` per-caixa admission verb) reaches
        // for the one-liner + three `#[from]` arms and gets the
        // diagnostic-naming-the-offending-caixa contract for free —
        // peer of the sibling `caixa-mesh::typed_view` /
        // `caixa-flux::require_v0_servico_shape` call sites the
        // per-Aplicacao / per-Servico compound gates already close.
        //
        // Four-axis byte-for-byte parity assertion: on every fixture
        // this pin sweeps (valid Acao, mis-kinded Servico, missing
        // `:ci` slot, cyclic `:ci` run), [`validate`]'s Ok/Err
        // discrimination must match the three-line prelude verbatim,
        // and on the Ok arm the substrate primitive's returned
        // [`canteiro_types::CanteiroDag`] must produce the same
        // topological-order node-name projection [`validate`] uses to
        // build [`RenderedAcao::node_names_topo`]. Trips at
        // caixa-actions build time if [`validate`] re-inlines the
        // three-line prelude and drifts from the substrate primitive.

        // Valid two-node acyclic fixture — the Ok-arm branch.
        let ok_fixture = || {
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
            acao_caixa(Some(ci))
        };
        let ok_caixa = ok_fixture();
        let via_validate = validate(&ok_caixa).expect("valid Acao passes validate");
        let via_primitive =
            caixa_core::require_acao_view::<Error>(&ok_caixa).expect("valid Acao passes compound");
        let (via_primitive_ci, via_primitive_cd) = via_primitive;
        // The substrate primitive's returned `(&CiRun, CanteiroDag)`
        // projects the same topological-order node-name list
        // [`validate`] builds — so the compound helper's Ok arm is
        // the exact source of truth `validate` reads off, no
        // reconstruction path a future edit could drift onto.
        let topo = via_primitive_cd
            .topo_order()
            .expect("acyclic CanteiroDag returns a valid topo_order");
        let expected_node_names: Vec<String> = topo
            .iter()
            .filter_map(|id| via_primitive_cd.nodes.get(id).map(|n| n.name.clone()))
            .collect();
        assert_eq!(
            via_validate.node_names_topo, expected_node_names,
            "validate's node_names_topo must equal the compound helper's \
             topo-order-projected node names byte-for-byte"
        );
        // The substrate primitive's borrowed `:ci` slot backs the
        // same `edge_count` projection [`validate`] reads off — pins
        // the pair-return contract of `require_acao_view` on the
        // `&CiRun` arm, routed through the sibling substrate primitive
        // [`caixa_core::ci_declared_edge_count`] so both `validate`'s
        // production emit and this byte-parity pin consult one typed
        // dispatch on the borrowed [`canteiro_types::CiRun`] rather
        // than open-coding `.nodes.iter().map(|n| n.deps.len()).sum()`
        // at two sites in lockstep.
        let expected_edge_count: usize = caixa_core::ci_declared_edge_count(via_primitive_ci);
        assert_eq!(
            via_validate.edge_count, expected_edge_count,
            "validate's edge_count must equal the compound helper's \
             borrowed-CiRun edge sum byte-for-byte"
        );

        // Mis-kinded Servico — the KindMismatch arm.
        let mut mis_kinded = ok_fixture();
        mis_kinded.kind = CaixaKind::Servico;
        mis_kinded.servicos = vec!["servicos/hello.computeunit.yaml".to_string()];
        let via_validate_err = validate(&mis_kinded).expect_err("mis-kinded Acao must be rejected");
        // `expect_err` is usable again as of sui 96a811e (CanteiroDag derives
        // Debug, and so does the shigoto_dag::Dag it wraps, 0.1.12).
        let via_primitive_err = caixa_core::require_acao_view(&mis_kinded)
            .expect_err("mis-kinded Acao must be rejected by compound");
        assert_eq!(
            format!("{via_validate_err}"),
            format!("{via_primitive_err}"),
            "validate's mis-kinded Display bytes must equal the compound \
             helper's Display bytes — a future kind-gate format edit lands \
             in exactly one place, not duplicated across every per-`Acao` \
             consumer"
        );
        match (via_validate_err, via_primitive_err) {
            (Error::NotAnAcao(a), Error::NotAnAcao(b)) => {
                assert_eq!(a.nome, b.nome);
                assert_eq!(a.expected, b.expected);
                assert_eq!(a.actual, b.actual);
            }
            (v, p) => panic!("expected both Error::NotAnAcao, got validate={v:?} primitive={p:?}"),
        }

        // Missing `:ci` slot past kind gate — the MissingCiSlot arm.
        let missing_ci = acao_caixa(None);
        let via_validate_err =
            validate(&missing_ci).expect_err("Acao with no :ci must be rejected");
        // `expect_err` is usable again as of sui 96a811e (CanteiroDag derives
        // Debug, and so does the shigoto_dag::Dag it wraps, 0.1.12).
        let via_primitive_err = caixa_core::require_acao_view(&missing_ci)
            .expect_err("Acao with no :ci must be rejected by compound");
        assert_eq!(
            format!("{via_validate_err}"),
            format!("{via_primitive_err}"),
            "validate's missing-:ci Display bytes must equal the compound \
             helper's Display bytes — the substrate primitive is the sole \
             MissingCiSlot constructor across every per-`Acao` consumer"
        );
        match (via_validate_err, via_primitive_err) {
            (Error::MissingCi(a), Error::MissingCi(b)) => {
                assert_eq!(a.nome, b.nome);
            }
            (v, p) => panic!("expected both Error::MissingCi, got validate={v:?} primitive={p:?}"),
        }

        // Cyclic `:ci` run past presence gate — the CiDecomposeFailure arm.
        let cyclic_ci = CiRun {
            workspace: "pleme-io".to_string(),
            repo: "caixa".to_string(),
            nodes: vec![
                CiNode::new("a", EnvClass::None, action("a"), vec!["b".to_string()]),
                CiNode::new("b", EnvClass::None, action("b"), vec!["a".to_string()]),
            ],
        };
        let cyclic = acao_caixa(Some(cyclic_ci));
        let via_validate_err = validate(&cyclic).expect_err("cyclic Acao must be rejected");
        // `expect_err` is usable again as of sui 96a811e (CanteiroDag derives
        // Debug, and so does the shigoto_dag::Dag it wraps, 0.1.12).
        let via_primitive_err = caixa_core::require_acao_view(&cyclic)
            .expect_err("cyclic Acao must be rejected by compound");
        assert_eq!(
            format!("{via_validate_err}"),
            format!("{via_primitive_err}"),
            "validate's decompose-failure Display bytes must equal the \
             compound helper's Display bytes — the substrate primitive is \
             the sole CiDecomposeFailure constructor across every \
             per-`Acao` consumer"
        );
        match (via_validate_err, via_primitive_err) {
            (Error::Decompose(a), Error::Decompose(b)) => {
                assert_eq!(a.nome, b.nome);
                assert_eq!(a.source, b.source);
            }
            (v, p) => panic!("expected both Error::Decompose, got validate={v:?} primitive={p:?}"),
        }
    }
}
