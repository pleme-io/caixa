use serde::{Deserialize, Serialize};

/// What a caixa produces.
///
/// In `caixa.lisp`:
///
/// ```lisp
/// :kind Biblioteca   ; library (lib/<nome>.lisp entry)
/// :kind Binario      ; executable(s) under exe/
/// :kind Servico      ; long-running service under servicos/
/// :kind Supervisor   ; OTP-style typed supervisor tree (see supervisor.rs)
/// ```
///
/// Authored as bare symbols (`Biblioteca` not `:biblioteca`) to match the
/// tatara-lisp enum convention where symbols become enum discriminants via
/// the serde `Deserialize` fallthrough.
#[derive(
    Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, gen_platform::IsVariant,
)]
pub enum CaixaKind {
    /// Library — exports Lisp forms for other caixas to `(importar …)`.
    Biblioteca,
    /// Binary — one or more executables under `exe/`.
    Binario,
    /// Service — long-running daemon under `servicos/`.
    Servico,
    /// OTP-shaped supervisor — does not run any code itself; its
    /// children are other caixas, restarted under a typed strategy.
    /// See `supervisor.rs` for the full shape (`SupervisorSpec`).
    Supervisor,
    /// Typed application — composes multiple Servicos into a single
    /// declarative mesh with WIT-typed `:contratos`, mesh-level
    /// `:politicas`, and explicit `:placement`. See `aplicacao.rs`
    /// (`AplicacaoSpec`) and `theory/MESH-COMPOSITION.md` for the
    /// design frame.
    Aplicacao,
    /// Typed CI run — carries a `:ci` slot of
    /// `canteiro_types::CiRun` (a repo's CI run as a set of typed
    /// nodes + their dependency edges). Runs no code of its own and
    /// owns no `lib`/`exe`/`servicos`/`children`/`membros` code
    /// surface — its sole payload is the `ci` field on [`crate::Caixa`].
    /// See CANTEIRO §7.1-C (`pleme-io/sui`'s `canteiro-types` crate)
    /// for the DAG algebra (`decompose`/`affected_set`/`affected_waves`)
    /// this slot feeds, and the `caixa-actions` renderer (currently
    /// validate-only — see its crate docs) for the M0 consumer.
    Acao,
}

impl CaixaKind {
    /// A `Biblioteca` is expected to have at least one `lib/` entry.
    #[must_use]
    pub const fn requires_lib(self) -> bool {
        matches!(self, Self::Biblioteca)
    }

    /// A `Binario` is expected to have at least one `exe/` entry.
    #[must_use]
    pub const fn requires_exe(self) -> bool {
        matches!(self, Self::Binario)
    }

    /// A `Servico` is expected to have at least one `servicos/` entry.
    #[must_use]
    pub const fn requires_servicos(self) -> bool {
        matches!(self, Self::Servico)
    }

    /// A `Supervisor` is expected to have at least one `:children` entry
    /// (or a `SimpleOneForOne` strategy that spawns children dynamically).
    #[must_use]
    pub const fn requires_children(self) -> bool {
        matches!(self, Self::Supervisor)
    }

    /// An `Aplicacao` is expected to have at least one `:membros` entry.
    #[must_use]
    pub const fn requires_membros(self) -> bool {
        matches!(self, Self::Aplicacao)
    }

    /// An `Acao` is expected to carry a `:ci` slot (a typed
    /// `canteiro_types::CiRun`). Mirror of the sibling
    /// [`Self::requires_lib`]/[`Self::requires_exe`]/
    /// [`Self::requires_servicos`]/[`Self::requires_membros`] required-
    /// slot predicates on the fifth [`CaixaKind`] arm.
    #[must_use]
    pub const fn requires_ci(self) -> bool {
        matches!(self, Self::Acao)
    }

    /// The canonical human-readable name.
    ///
    /// The five arms route through the paired
    /// [`crate::render::CAIXA_KIND_LABEL_BIBLIOTECA`] /
    /// [`crate::render::CAIXA_KIND_LABEL_BINARIO`] /
    /// [`crate::render::CAIXA_KIND_LABEL_SERVICO`] /
    /// [`crate::render::CAIXA_KIND_LABEL_SUPERVISOR`] /
    /// [`crate::render::CAIXA_KIND_LABEL_APLICACAO`] lifted constants so
    /// every substrate consumer that formats a caixa's typed shape as
    /// user-facing text (the future wasm-operator's per-caixa startup
    /// log line, the future `feira app graph` per-member kind column,
    /// the future M4 CR materializer's admission-webhook rejection
    /// body) reads the same byte-string the [`std::fmt::Display`] impl
    /// (routed through this helper) emits — the pin test
    /// [`tests::caixa_kind_as_str_returns_lifted_peer_const`] asserts
    /// the five paths agree. Peer of the sibling
    /// [`crate::supervisor::RestartStrategy::as_str`] (09ffb2d),
    /// [`crate::supervisor::RestartPolicy::as_str`] (ccdf955), and M3
    /// [`crate::aplicacao::PlacementStrategy::as_str`] (cc8f749) on the
    /// sibling closed-set typed-enum discriminator axes — the fourth
    /// closed-set typed enum on the caixa surface to converge onto the
    /// same drift-detection posture.
    ///
    /// The two axes ([`Self::as_str`] returning lowercase Portuguese
    /// vs. the un-`rename`d `Serialize` derive emitting PascalCase
    /// `"Biblioteca"` / `"Binario"` / `"Servico"` / `"Supervisor"` /
    /// `"Aplicacao"`) are intentionally distinct: the wire format is
    /// the tatara-lisp author surface (`:kind Biblioteca`), while
    /// [`Self::as_str`] / [`std::fmt::Display`] emit the substrate's
    /// canonical human-readable form for diagnostics + graph output +
    /// audit views. Two paths on this enum surface (rather than three
    /// as on the sibling OTP-shape and M3 enums where wire ==
    /// human-readable), but the same "one canonical byte-string per
    /// axis, routed through a lifted const" discipline.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Biblioteca => crate::render::CAIXA_KIND_LABEL_BIBLIOTECA,
            Self::Binario => crate::render::CAIXA_KIND_LABEL_BINARIO,
            Self::Servico => crate::render::CAIXA_KIND_LABEL_SERVICO,
            Self::Supervisor => crate::render::CAIXA_KIND_LABEL_SUPERVISOR,
            Self::Aplicacao => crate::render::CAIXA_KIND_LABEL_APLICACAO,
            Self::Acao => crate::render::CAIXA_KIND_LABEL_ACAO,
        }
    }
}

/// [`std::fmt::Display`] routed through [`CaixaKind::as_str`], so the
/// pretty-printed byte-string every consumer that formats the caixa's
/// typed shape as user-facing text lands on (the future wasm-operator's
/// per-caixa startup log line, the future `feira app graph` per-member
/// kind column, the future M4 `wasm.pleme.io/v1alpha1/ComputeUnit` /
/// `mesh.pleme.io/v1alpha1/*` CR materializer's admission-webhook
/// rejection body) reaches for the same lifted
/// [`crate::render::CAIXA_KIND_LABEL_BIBLIOTECA`] /
/// [`crate::render::CAIXA_KIND_LABEL_BINARIO`] /
/// [`crate::render::CAIXA_KIND_LABEL_SERVICO`] /
/// [`crate::render::CAIXA_KIND_LABEL_SUPERVISOR`] /
/// [`crate::render::CAIXA_KIND_LABEL_APLICACAO`] const the
/// [`CaixaKind::as_str`] helper already returns.
///
/// Pre-convergence [`CaixaKind`] carried no [`std::fmt::Display`]
/// surface at all — every consumer past the wire format
/// (`Serialize` → PascalCase) had to pick between two paths
/// ([`CaixaKind::as_str`] returning the lowercase Portuguese label, or
/// `format!("{v:?}")` on the `Debug` derive returning the PascalCase
/// variant name), each with different bytes on every arm and no
/// compile-time link between the two — with the failure surfacing as a
/// downstream consumer's log / graph / diagnostic reading one spelling
/// while a peer consumer emitted another, far from any single-site
/// commit. Wiring [`std::fmt::Display`] through [`CaixaKind::as_str`]
/// closes the drift footgun structurally: every `format!("{v}")` call
/// reaches the same lifted [`crate::render::CAIXA_KIND_LABEL_*`] const
/// [`CaixaKind::as_str`] returns, and a future rebrand (a per-consumer
/// disambiguation of the `:kind` vocabulary, an English-canonical
/// rebrand of `"biblioteca"` → `"library"` under an M4 substrate-wide-
/// vocabulary shift) reaches every consumer through exactly one
/// const-edit.
///
/// The wire format axis (`Serialize` derive, PascalCase, tatara-lisp
/// author surface `:kind Biblioteca`) stays deliberately distinct from
/// the human-readable axis (`Display` / `as_str`, lowercase Portuguese
/// diagnostic form): the two-path split is by design, not drift. The
/// pin test
/// [`tests::caixa_kind_display_matches_as_str_and_not_serialize_wire`]
/// makes the split load-bearing so a future accidental collapse of
/// either axis onto the other (routing `Display` through the wire
/// format via `serde_json::to_string`, or routing `Serialize` through
/// [`CaixaKind::as_str`] via `#[serde(rename_all = "…")]`) trips at
/// build time rather than silently merging the two axes into one at
/// some future consumer.
///
/// Pin tests
/// [`tests::caixa_kind_display_routes_through_as_str_helper`] and
/// [`tests::caixa_kind_as_str_returns_lifted_peer_const`] assert the
/// two paths agree byte-for-byte on every variant, so a future variant
/// rename or per-arm serde attribute drift is a build error visible at
/// caixa-core test time.
///
/// Mirrors the M3 [`crate::aplicacao::PlacementStrategy`] `Display`
/// impl (aplicacao.rs:2306), the M2
/// [`crate::supervisor::RestartStrategy`] `Display` impl
/// (supervisor.rs:164), and the M2 [`crate::supervisor::RestartPolicy`]
/// `Display` impl (supervisor.rs:306) on the sibling closed-set typed-
/// enum discriminator axes — same as_str-through-Display convergence
/// discipline, extended to close the fourth (and structurally most
/// fundamental — every caixa carries a `:kind`) closed-set typed-enum
/// discriminator axis on the caixa typed surface.
impl std::fmt::Display for CaixaKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_requirements() {
        assert!(CaixaKind::Biblioteca.requires_lib());
        assert!(!CaixaKind::Biblioteca.requires_exe());
        assert!(CaixaKind::Binario.requires_exe());
        assert!(CaixaKind::Servico.requires_servicos());
        assert!(CaixaKind::Supervisor.requires_children());
        assert!(!CaixaKind::Servico.requires_children());
        assert!(CaixaKind::Acao.requires_ci());
        assert!(!CaixaKind::Servico.requires_ci());
    }

    #[test]
    fn kind_deserializes_from_pascal_symbol() {
        let v: CaixaKind = serde_json::from_str("\"Biblioteca\"").unwrap();
        assert_eq!(v, CaixaKind::Biblioteca);
        let v: CaixaKind = serde_json::from_str("\"Supervisor\"").unwrap();
        assert_eq!(v, CaixaKind::Supervisor);
    }

    #[test]
    fn supervisor_kind_has_canonical_name() {
        assert_eq!(CaixaKind::Supervisor.as_str(), "supervisor");
    }

    #[test]
    fn caixa_kind_as_str_returns_lifted_peer_const() {
        // The fail-before-pass-after pin: pre-lift the five
        // [`CaixaKind::as_str`] match arms each returned a hand-authored
        // byte-string literal (`"biblioteca"` / `"binario"` /
        // `"servico"` / `"supervisor"` / `"aplicacao"`) with no
        // compile-time link to any lifted peer const on the sibling
        // layout-diagnostic axis (whose bytes coincide on `Biblioteca` /
        // `Servico` by design — see the existing alignment pin in
        // `crate::layout::tests`). A future rebrand touching either
        // endpoint (a per-consumer disambiguation of the `:kind`
        // vocabulary, an English-canonical rename lands on one arm's
        // byte-string without touching the peer axis) would silently
        // desynchronize until a downstream consumer surfaced the drift
        // at diagnostic / graph time. Pinning the five arms to the five
        // lifted [`crate::render::CAIXA_KIND_LABEL_*`] consts makes any
        // future drift a caixa-core-build-time failure. Peer of the
        // sibling
        // [`crate::supervisor::tests::restart_strategy_variants_serialize_to_lifted_scalar_values`]
        // (09ffb2d) /
        // [`crate::supervisor::tests::restart_policy_variants_serialize_to_lifted_scalar_values`]
        // (ccdf955) /
        // `placement_strategy_variants_serialize_to_lifted_scalar_values`
        // (3f0e21c) pins on the sibling closed-set typed-enum
        // discriminator axes.
        for (variant, expected) in [
            (
                CaixaKind::Biblioteca,
                crate::render::CAIXA_KIND_LABEL_BIBLIOTECA,
            ),
            (CaixaKind::Binario, crate::render::CAIXA_KIND_LABEL_BINARIO),
            (CaixaKind::Servico, crate::render::CAIXA_KIND_LABEL_SERVICO),
            (
                CaixaKind::Supervisor,
                crate::render::CAIXA_KIND_LABEL_SUPERVISOR,
            ),
            (
                CaixaKind::Aplicacao,
                crate::render::CAIXA_KIND_LABEL_APLICACAO,
            ),
            (CaixaKind::Acao, crate::render::CAIXA_KIND_LABEL_ACAO),
        ] {
            assert_eq!(
                variant.as_str(),
                expected,
                "CaixaKind::{variant:?}.as_str() must return the lifted \
                 CAIXA_KIND_LABEL_* const"
            );
        }
    }

    #[test]
    fn caixa_kind_display_routes_through_as_str_helper() {
        // The fail-before-pass-after pin on the two-path convergence:
        // pre-lift [`CaixaKind`] carried no [`std::fmt::Display`]
        // surface at all — every consumer past the wire format had to
        // pick between [`CaixaKind::as_str`] returning the lowercase
        // Portuguese label or `format!("{v:?}")` on the `Debug` derive
        // returning the PascalCase variant name. Wiring
        // [`std::fmt::Display`] through [`CaixaKind::as_str`] closes
        // the drift footgun: every `format!("{v}")` call reaches the
        // same lifted [`crate::render::CAIXA_KIND_LABEL_*`] const the
        // [`CaixaKind::as_str`] helper already returns, so a future
        // variant rename lands at exactly one place. Pin the routing
        // here so a future `impl std::fmt::Display for CaixaKind`
        // reimplementation that hand-rolls the arms instead of
        // delegating to [`CaixaKind::as_str`] fails at caixa-core
        // build time. Peer of the sibling
        // [`crate::supervisor::tests::restart_strategy_display_routes_through_as_str_helper`]
        // /
        // [`crate::supervisor::tests::restart_policy_display_routes_through_as_str_helper`]
        // (15a1305) on the sibling M2 closed-set typed-enum
        // discriminator axes.
        for variant in [
            CaixaKind::Biblioteca,
            CaixaKind::Binario,
            CaixaKind::Servico,
            CaixaKind::Supervisor,
            CaixaKind::Aplicacao,
            CaixaKind::Acao,
        ] {
            assert_eq!(
                variant.to_string(),
                variant.as_str(),
                "CaixaKind::{variant:?} Display must route through \
                 CaixaKind::as_str (single source of truth: the lifted \
                 CAIXA_KIND_LABEL_* const)"
            );
        }
    }

    #[test]
    fn caixa_kind_display_matches_as_str_and_not_serialize_wire() {
        // The fail-before-pass-after pin on the two-axis split
        // discipline: unlike the sibling OTP-shape / M3 typed enums
        // ([`crate::supervisor::RestartStrategy`],
        // [`crate::supervisor::RestartPolicy`],
        // [`crate::aplicacao::PlacementStrategy`]) where the wire
        // format and the human-readable format share bytes (both
        // PascalCase / camelCase), [`CaixaKind`] carries two axes with
        // *distinct* byte-shapes by design: the wire format is
        // PascalCase (`"Biblioteca"` / `"Binario"` / `"Servico"` /
        // `"Supervisor"` / `"Aplicacao"` — the tatara-lisp author
        // surface `:kind Biblioteca`), while [`CaixaKind::as_str`] /
        // [`std::fmt::Display`] emit the lowercase Portuguese
        // diagnostic form (`"biblioteca"` etc.). The pin here makes
        // the split load-bearing: a future accidental collapse of
        // either axis onto the other (routing `Serialize` through
        // [`CaixaKind::as_str`] via `#[serde(rename_all = "lowercase")]`,
        // routing [`std::fmt::Display`] through the wire format
        // directly) would trip here at caixa-core build time rather
        // than silently merging the two axes at some future consumer's
        // dispatch step.
        for variant in [
            CaixaKind::Biblioteca,
            CaixaKind::Binario,
            CaixaKind::Servico,
            CaixaKind::Supervisor,
            CaixaKind::Aplicacao,
            CaixaKind::Acao,
        ] {
            let wire = serde_json::to_string(&variant).unwrap();
            let unquoted = wire
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .expect("serialized CaixaKind is a JSON string");
            let display = variant.to_string();
            assert_eq!(
                display,
                variant.as_str(),
                "CaixaKind::{variant:?} Display must byte-equal as_str"
            );
            assert_ne!(
                display, unquoted,
                "CaixaKind::{variant:?} Display / as_str (lowercase Portuguese \
                 diagnostic form) must stay structurally distinct from the \
                 Serialize wire format (PascalCase tatara-lisp author surface)"
            );
            assert!(
                unquoted.chars().next().is_some_and(char::is_uppercase),
                "CaixaKind::{variant:?} wire format must open with an \
                 uppercase byte (PascalCase tatara-lisp author surface)"
            );
            assert!(
                display.chars().next().is_some_and(char::is_lowercase),
                "CaixaKind::{variant:?} Display / as_str must open with a \
                 lowercase byte (Portuguese diagnostic form)"
            );
        }
    }

    #[test]
    fn caixa_kind_label_consts_pin_canonical_bytes() {
        // Drift-detection pin: the five
        // [`crate::render::CAIXA_KIND_LABEL_*`] lifted consts must
        // continue to carry their canonical byte-shapes. A rebrand
        // touching one const's declaration would silently desynchronize
        // every consumer this const routes through (the
        // [`CaixaKind::as_str`] arm, the [`std::fmt::Display`] route,
        // the byte-shape coincidence pins against
        // [`crate::render::LAYOUT_MISSING_ENTRY_KIND_BIBLIOTECA`] /
        // [`crate::render::LAYOUT_MISSING_ENTRY_KIND_SERVICO`] /
        // [`crate::render::FLEET_PROGRAMS_KEY_APLICACAO`]) — pinning
        // the five byte-shapes here makes any rebrand attempt a build
        // error at this call, forcing the author to reason about every
        // downstream consumer explicitly. Mirror of the peer
        // drift-detection pins the sibling OTP-shape / M3 closed-set
        // const families carry (`SUPERVISOR_ESTRATEGIA_*`,
        // `SUPERVISOR_CHILD_RESTART_*`, `M3_PLACEMENT_ESTRATEGIA_*`).
        assert_eq!(crate::render::CAIXA_KIND_LABEL_BIBLIOTECA, "biblioteca");
        assert_eq!(crate::render::CAIXA_KIND_LABEL_BINARIO, "binario");
        assert_eq!(crate::render::CAIXA_KIND_LABEL_SERVICO, "servico");
        assert_eq!(crate::render::CAIXA_KIND_LABEL_SUPERVISOR, "supervisor");
        assert_eq!(crate::render::CAIXA_KIND_LABEL_APLICACAO, "aplicacao");
        assert_eq!(crate::render::CAIXA_KIND_LABEL_ACAO, "acao");
    }

    #[test]
    fn caixa_kind_is_variant_predicates_partition_the_arm_set() {
        // Fail-before-pass-after pin on the [`gen_platform::IsVariant`]
        // derive: for each of the five variants, exactly one of the
        // generated `is_biblioteca` / `is_binario` / `is_servico` /
        // `is_supervisor` / `is_aplicacao` predicates returns `true` and
        // the other four return `false`. Prior to this derive the ten
        // production `caixa.kind() == CaixaKind::X` / `!=` sites in
        // `layout.rs` (`SupervisorOwnsCode` / `AplicacaoOwnsCode` /
        // `MeshSlotsOnNonAplicacao` / `SupervisorSlotsOnNonSupervisor` /
        // `ServicoSlotsOnNonServico` / `MissingLib` biblioteca-fallback
        // / Supervisor invariants / Aplicacao invariants) plus
        // `manifest.rs` (`aplicacao_view` / `supervisor_view` kind
        // gates) each open-coded a per-arm PartialEq compare against
        // the enum variant — ten sites that expressed no compile-time
        // link back to the closed-set typed dispatch a future sixth
        // `:kind` (e.g. an `Actor` virtual-actor arm for the
        // absorption-roadmap M5 Orleans-inspired kind) would have to
        // thread through in lockstep or one gate would silently
        // disagree with the others on which arms it treats as "runs
        // no code" / "declares mesh slots" / etc. Peer of the sibling
        // [`crate::supervisor::RestartStrategy`] / [`crate::supervisor::RestartPolicy`] /
        // [`crate::upgrade::UpgradeInstruction`] `IsVariant` derives on
        // the sibling closed-set typed-enum discriminator axes — extends
        // the same one-typed-dispatch-per-variant discipline onto the
        // fourth (and structurally most fundamental — every caixa
        // carries a `:kind`) closed-set typed-enum discriminator axis.
        let rows: [(CaixaKind, [bool; 6]); 6] = [
            (
                CaixaKind::Biblioteca,
                [true, false, false, false, false, false],
            ),
            (
                CaixaKind::Binario,
                [false, true, false, false, false, false],
            ),
            (
                CaixaKind::Servico,
                [false, false, true, false, false, false],
            ),
            (
                CaixaKind::Supervisor,
                [false, false, false, true, false, false],
            ),
            (
                CaixaKind::Aplicacao,
                [false, false, false, false, true, false],
            ),
            (CaixaKind::Acao, [false, false, false, false, false, true]),
        ];
        for (variant, expected) in rows {
            let observed = [
                variant.is_biblioteca(),
                variant.is_binario(),
                variant.is_servico(),
                variant.is_supervisor(),
                variant.is_aplicacao(),
                variant.is_acao(),
            ];
            assert_eq!(
                observed, expected,
                "CaixaKind::{variant:?} is_* predicates must partition \
                 the arm set (biblioteca, binario, servico, supervisor, \
                 aplicacao, acao); got {observed:?}"
            );
        }
    }

    #[test]
    fn caixa_kind_is_variant_predicates_are_const_fn() {
        // The [`gen_platform::IsVariant`] derive emits `const fn`
        // predicates on the peer [`crate::upgrade::UpgradeInstruction`] +
        // [`crate::supervisor::RestartStrategy`] +
        // [`crate::supervisor::RestartPolicy`] closed-set typed enums —
        // pin the same posture on [`CaixaKind`] so a future accidental
        // downgrade to non-`const` (an added runtime helper reachable
        // only from a non-`const` context, a manual hand-rolled `impl`
        // that shadows the derive-generated method) trips at
        // caixa-core build time rather than surfacing as a
        // downstream `const`-context regression far from the derive
        // declaration.
        const IS_APLICACAO: bool = CaixaKind::Aplicacao.is_aplicacao();
        const IS_BIBLIOTECA: bool = CaixaKind::Biblioteca.is_biblioteca();
        const IS_SERVICO: bool = CaixaKind::Servico.is_servico();
        const IS_SUPERVISOR: bool = CaixaKind::Supervisor.is_supervisor();
        const IS_BINARIO: bool = CaixaKind::Binario.is_binario();
        const IS_ACAO: bool = CaixaKind::Acao.is_acao();
        assert!(IS_APLICACAO);
        assert!(IS_BIBLIOTECA);
        assert!(IS_SERVICO);
        assert!(IS_SUPERVISOR);
        assert!(IS_BINARIO);
        assert!(IS_ACAO);
    }

    #[test]
    fn caixa_kind_label_consts_are_pairwise_distinct() {
        // Distinctness pin: the five
        // [`crate::render::CAIXA_KIND_LABEL_*`] consts must be pairwise
        // distinct — an accidental copy-paste flip that reroutes one
        // label's byte-string to also match another silently collapses
        // two per-kind labels onto one, so an operator reads
        // `biblioteca` for what should have surfaced as `servico` (or
        // vice versa). This pin catches any such flip at build time.
        // Mirror of the peer distinctness pins on other closed-set
        // typed axes (e.g.
        // `layout_missing_entry_kind_consts_are_pairwise_distinct`).
        let all = [
            crate::render::CAIXA_KIND_LABEL_BIBLIOTECA,
            crate::render::CAIXA_KIND_LABEL_BINARIO,
            crate::render::CAIXA_KIND_LABEL_SERVICO,
            crate::render::CAIXA_KIND_LABEL_SUPERVISOR,
            crate::render::CAIXA_KIND_LABEL_APLICACAO,
            crate::render::CAIXA_KIND_LABEL_ACAO,
        ];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                if i != j {
                    assert_ne!(
                        a, b,
                        "CAIXA_KIND_LABEL_* consts must be pairwise distinct \
                         — found duplicate byte-string {a:?} at indices {i} \
                         and {j}"
                    );
                }
            }
        }
    }
}
