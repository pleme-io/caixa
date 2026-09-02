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
    /// Exhaustive iteration surface for every consumer that walks the
    /// closed six-arm [`CaixaKind`] discriminator set (the future M4
    /// `mesh.pleme.io/v1alpha1/Caixa` CR materializer's admission-webhook
    /// rejection body naming the accepted-`:kind` list, a future
    /// `feira --kind …` CLI arg-parse's "did you mean" hint via a
    /// [`Self::from_wire`]-scan over the slice, the future
    /// `feira app graph` per-Aplicacao `:kind`-histogram column, any
    /// future round-trip fuzz harness that sweeps every arm). A future
    /// variant addition (an `Actor` virtual-actor arm the
    /// [`ABSORPTION-ROADMAP`](https://github.com/pleme-io/theory/blob/main/ABSORPTION-ROADMAP.md)
    /// M5 Orleans-inspired kind reaches through — a candidate future
    /// arm named in the sibling [`Self::from_wire`] doc block —
    /// extends this slice as a single edit and every consumer picks up
    /// the new entry by construction; the compiler-checked
    /// exhaustiveness on the sibling method `match` arms
    /// ([`Self::as_str`] / [`Self::wire_name`] / [`Self::from_wire`] /
    /// the `requires_*` predicates) is the build-time guarantee that
    /// no arm forgets to grow.
    ///
    /// Peer of the sibling closed-set typed enums'
    /// [`crate::aplicacao::PlacementStrategy::ALL`] (18c7342) /
    /// [`crate::aplicacao::RateLimitUnit::ALL`] (6bce03d) /
    /// [`crate::dep::DepList::ALL`] (45ee563) exhaustive-iteration
    /// surfaces — the fourth (and structurally most fundamental —
    /// every caixa carries a `:kind`) closed-set typed enum on the
    /// caixa surface to converge onto the same
    /// one-canonical-arm-list-per-enum discipline.
    pub const ALL: &'static [Self] = &[
        Self::Biblioteca,
        Self::Binario,
        Self::Servico,
        Self::Supervisor,
        Self::Aplicacao,
        Self::Acao,
    ];

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

    /// Substrate-canonical per-[`CaixaKind`] PascalCase wire byte-string
    /// every consumer that emits the Caixa's `:kind` axis onto a wire
    /// surface outside the caixa-core boundary keys off — returns the
    /// per-arm byte-string the paired
    /// [`crate::render::CAIXA_KIND_WIRE_BIBLIOTECA`] /
    /// [`crate::render::CAIXA_KIND_WIRE_BINARIO`] /
    /// [`crate::render::CAIXA_KIND_WIRE_SERVICO`] /
    /// [`crate::render::CAIXA_KIND_WIRE_SUPERVISOR`] /
    /// [`crate::render::CAIXA_KIND_WIRE_APLICACAO`] /
    /// [`crate::render::CAIXA_KIND_WIRE_ACAO`] lifted consts pin, and
    /// [`Self::from_wire`] parses back into the typed [`CaixaKind`]
    /// discriminator.
    ///
    /// Byte-identical to the un-`rename`d `Serialize` derive's per-arm
    /// wire scalar (`serde_json::to_string(&kind).unwrap()` unquoted),
    /// with the pin test
    /// [`tests::caixa_kind_wire_name_matches_serialize_wire_byte_string`]
    /// making the two paths' byte-agreement load-bearing so a future
    /// `#[serde(rename_all = "…")]` attribute drift at the derive
    /// surface trips at caixa-core build time rather than silently
    /// splitting the wire byte-shape from every consumer that reaches
    /// for this typed dispatch. The paired [`Self::from_wire`] returns
    /// `Some` on every string [`Self::wire_name`] emits and `None` on
    /// every other input — the round-trip pin
    /// [`tests::caixa_kind_wire_round_trips_through_from_wire`] locks
    /// the two paths' accept-sets together by construction.
    ///
    /// Distinct axis from the sibling [`Self::as_str`] — which returns
    /// the lowercase Portuguese diagnostic byte-string (`"biblioteca"`
    /// / `"binario"` / …) every consumer that formats the caixa's
    /// typed shape as user-facing text lands on — by design, not by
    /// drift: the two-axis split the load-bearing pin
    /// [`tests::caixa_kind_display_matches_as_str_and_not_serialize_wire`]
    /// already encodes. This wire accessor closes the third axis on
    /// the [`CaixaKind`] closed-set discriminator (wire byte-string),
    /// peer of [`Self::as_str`] (human-readable byte-string) and the
    /// [`std::fmt::Display`] impl (routed through [`Self::as_str`]).
    ///
    /// Prior to this lift, the six [`caixa-crd::conversion`] +
    /// [`caixa-feira`] + future-M4-CR-materializer consumers that
    /// needed the PascalCase wire byte-shape reached for one of two
    /// fragile paths: `format!("{:?}", kind)` (couples the wire format
    /// to `Debug`'s stability guarantee, which Rust's own conventions
    /// give as *no guarantee at all* — a `#[derive(Debug)]` swap for
    /// a hand-rolled `impl Debug` that pretty-prints the variant with
    /// extra context is a permitted mechanical edit whose apply-time
    /// symptom would be every downstream K8s CR carrying a stale wire
    /// byte-string), or `serde_json::to_string(&kind)` + string-trim of
    /// the outer quotes (introduces an allocation + error-handling
    /// path for a byte-shape the compiler knows verbatim at build
    /// time). Lifting the resolver to a typed method on the substrate
    /// primitive means every downstream consumer of the Caixa's
    /// `:kind` wire surface reaches for exactly one typed dispatch —
    /// the resolver's accept-set migrates as a unit on any future arm
    /// addition (a future virtual-actor `Actor` arm the
    /// `theory/ABSORPTION-ROADMAP.md` M5 Orleans-inspired kind reaches
    /// through, a per-cluster kind-alias table the M4 CR materializer
    /// resolves per-CR).
    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Biblioteca => crate::render::CAIXA_KIND_WIRE_BIBLIOTECA,
            Self::Binario => crate::render::CAIXA_KIND_WIRE_BINARIO,
            Self::Servico => crate::render::CAIXA_KIND_WIRE_SERVICO,
            Self::Supervisor => crate::render::CAIXA_KIND_WIRE_SUPERVISOR,
            Self::Aplicacao => crate::render::CAIXA_KIND_WIRE_APLICACAO,
            Self::Acao => crate::render::CAIXA_KIND_WIRE_ACAO,
        }
    }

    /// Substrate-canonical inverse of [`Self::wire_name`] — parses a
    /// PascalCase wire byte-string into the typed [`CaixaKind`]
    /// discriminator, returning `None` on any string not in the six-arm
    /// accept-set the sibling [`Self::wire_name`] emits.
    ///
    /// The pair `(wire_name, from_wire)` forms a total round-trip
    /// discipline on the six [`CaixaKind`] arms — every
    /// [`Self::wire_name`] output parses back through this accessor
    /// (pinned load-bearing by the sibling
    /// [`tests::caixa_kind_wire_round_trips_through_from_wire`] test),
    /// so consumers that emit a wire byte-string through
    /// [`Self::wire_name`] and later re-parse it here (the K8s
    /// [`caixa_crd`] `CaixaSpec.kind` `String`-carry round-trip through
    /// `caixa_into_cr` + `caixa_from_cr`, the future M4
    /// `mesh.pleme.io/v1alpha1/Caixa` CR materializer's admission-time
    /// wire re-parse, the future `feira` CLI verb that accepts a
    /// `--kind <Biblioteca|Servico|…>` arg and binds it into the typed
    /// enum) reach for one typed dispatch on the substrate primitive
    /// instead of the hand-rolled per-arm `match` cascade every
    /// pre-lift consumer previously carried verbatim. A future arm
    /// addition (a virtual-actor `Actor` arm the M5 Orleans-inspired
    /// kind reaches through) lands one caixa-core edit — the parser's
    /// arm-set migrates as a unit — rather than a coordinated rewrite
    /// across every hand-rolled `match cr.spec.kind.as_str()` at every
    /// downstream consumer site.
    ///
    /// Prior to this lift, the sole in-tree consumer of the reverse
    /// parse — [`caixa_crd::conversion::caixa_from_cr`] — carried a
    /// six-arm `match cr.spec.kind.as_str() { "Biblioteca" => …,
    /// "Binario" => …, "Servico" => …, "Supervisor" => …, "Aplicacao"
    /// => …, "Acao" => …, _ => CaixaKind::Biblioteca }` cascade that
    /// hard-coded every wire byte-string as a per-arm string literal
    /// with no compile-time link back to the typed
    /// [`crate::CaixaKind`] enum. A future variant rename or a serde
    /// attribute drift on the derive would silently split the wire
    /// format the forward `caixa_into_cr` emits from the reverse
    /// parser's arm-set — the CR would round-trip through JSON cleanly
    /// but land on the `_ => CaixaKind::Biblioteca` silent fallback
    /// on every non-Biblioteca variant, far from the derive-attribute
    /// commit that caused the drift. Lifting the resolver to a typed
    /// method on the substrate primitive closes the drift footgun by
    /// construction: the parser's accept-set is the same set the
    /// [`Self::wire_name`] emitter walks, so both halves of the
    /// round-trip migrate through one caixa-core edit on any future
    /// arm addition.
    ///
    /// Returns `Option<CaixaKind>` rather than `Result<CaixaKind, _>`
    /// because the existing in-tree consumer [`caixa_from_cr`] carries
    /// a hard-coded silent fallback (`_ => CaixaKind::Biblioteca`) —
    /// the fallback's shape is preserved verbatim by the caller's
    /// `.unwrap_or(CaixaKind::Biblioteca)` on the return value, so
    /// this lift is a byte-equal behavioral swap on today's call site
    /// (the CR round-trip is invariant), and future callers that want
    /// a typed error (a future `feira --kind …` arg-parse that
    /// surfaces `unknown kind: <arg>` at the CLI) can build one on top
    /// without disturbing the existing consumer's contract.
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            crate::render::CAIXA_KIND_WIRE_BIBLIOTECA => Some(Self::Biblioteca),
            crate::render::CAIXA_KIND_WIRE_BINARIO => Some(Self::Binario),
            crate::render::CAIXA_KIND_WIRE_SERVICO => Some(Self::Servico),
            crate::render::CAIXA_KIND_WIRE_SUPERVISOR => Some(Self::Supervisor),
            crate::render::CAIXA_KIND_WIRE_APLICACAO => Some(Self::Aplicacao),
            crate::render::CAIXA_KIND_WIRE_ACAO => Some(Self::Acao),
            _ => None,
        }
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

/// Substrate-canonical [`AsRef<str>`] projection on the structurally
/// most fundamental closed-set typed enum on the caixa surface — every
/// [`crate::Caixa`] carries a `:kind` — routing through the same
/// [`CaixaKind::as_str`] `pub const fn` accessor the paired
/// [`std::fmt::Display`] impl already delegates through, so any future
/// consumer that binds a [`CaixaKind`] through the standard-library
/// `impl AsRef<str>` bound (a [`std::process::Command::arg`] shell-out
/// composing the diagnostic byte-string into the future `feira` verb
/// dispatch, a `tracing::field::Value::Str`-arm structured-log recorder
/// on the future `caixa-operator`'s per-caixa reconcile step, a
/// [`std::collections::HashMap`] lookup keyed on the human-readable
/// label through `map.get::<str>(kind.as_ref())` on a future per-kind
/// diagnostic-dispatch table the M4 admission-webhook rejection body
/// composes) reaches the paired
/// [`crate::render::CAIXA_KIND_LABEL_BIBLIOTECA`] /
/// [`crate::render::CAIXA_KIND_LABEL_BINARIO`] /
/// [`crate::render::CAIXA_KIND_LABEL_SERVICO`] /
/// [`crate::render::CAIXA_KIND_LABEL_SUPERVISOR`] /
/// [`crate::render::CAIXA_KIND_LABEL_APLICACAO`] /
/// [`crate::render::CAIXA_KIND_LABEL_ACAO`] lifted-const through one
/// substrate-primitive dispatch rather than an open-coded `.as_str()`
/// projection at every wire-up.
///
/// Deliberately routes through the human-readable
/// [`CaixaKind::as_str`] axis, not the `PascalCase`
/// [`CaixaKind::wire_name`] axis — the two-axis split the sibling
/// [`tests::caixa_kind_display_matches_as_str_and_not_serialize_wire`]
/// pin makes load-bearing is preserved here by construction: `AsRef`
/// and `Display` land on the diagnostic byte-string, while the wire
/// axis (tatara-lisp author surface `:kind Biblioteca`) stays reachable
/// only through the explicit [`CaixaKind::wire_name`] +
/// [`serde::Serialize`] paths. Rust-side newtype/typed-enum convention
/// pairs [`AsRef<str>`] with [`fmt::Display`] on the same primitive so
/// a caller who has one has both; before this lift, [`CaixaKind`]
/// carried [`fmt::Display`] but not the paired [`AsRef<str>`] impl the
/// convention names.
///
/// Same "route the trait impl through the substrate-primitive
/// accessor" discipline the sibling [`crate::CaixaVersion`]
/// [`AsRef<str>`] impl (16d5c7e), the paired M2
/// [`crate::supervisor::RestartStrategy`] [`AsRef<str>`] impl
/// (63eb1a4), the paired M2 [`crate::supervisor::RestartPolicy`]
/// [`AsRef<str>`] impl (419ea81), and the M3
/// [`crate::aplicacao::PlacementStrategy`] [`AsRef<str>`] impl
/// (d86edd2) carry — closes the substrate primitive's
/// [`AsRef<str>`] projection axis onto the top-level [`CaixaKind`]
/// discriminator, so every closed-set typed enum on the caixa surface
/// (top-level `:kind`, M2 `:supervisor` per-child + sibling restart,
/// M3 `:placement :estrategia`, `:versao` newtype) now carries the
/// paired [`AsRef<str>`] + [`fmt::Display`] + `as_str` triple through
/// one lifted `CAIXA_KIND_LABEL_*` / `SUPERVISOR_*` /
/// `M3_PLACEMENT_ESTRATEGIA_*` const family.
///
/// Pinned load-bearing by
/// [`tests::caixa_kind_as_ref_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaKind::as_str`] across the six-arm
/// closed set) and
/// [`tests::caixa_kind_as_ref_str_routes_through_display_via_shared_accessor`]
/// (three-path convergence: `AsRef<str>` + `Display` + `as_str` all
/// resolve to the same lifted `CAIXA_KIND_LABEL_*` const per arm) —
/// any future silent detour that routes the impl through a divergent
/// projection (a per-arm inline `match self { … }` re-inlining that
/// opens a compile-time link to the un-lifted arm-literal, a swap onto
/// the `PascalCase` [`CaixaKind::wire_name`] axis that would collide
/// the human-readable / wire two-axis split) trips at caixa-core test
/// time under `assert_eq!` rather than at a downstream
/// `impl AsRef<str>`-bound consumer's silent split.
impl AsRef<str> for CaixaKind {
    fn as_ref(&self) -> &str {
        self.as_str()
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
        for &variant in CaixaKind::ALL {
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
        for &variant in CaixaKind::ALL {
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
        //
        // The pin lives inside a `const { assert!(..) }` block so the
        // compiler enforces both halves (arm predicate is `const`-
        // callable AND returns `true` for the matching arm) at
        // caixa-core compile time — a `#[test]` body without the
        // `const { .. }` wrapper would enforce const-callability at
        // compile time (through the named `const` binding) but the
        // arm-value pin only at test-run time, splitting the
        // enforcement axis into two windows.
        const {
            assert!(CaixaKind::Aplicacao.is_aplicacao());
            assert!(CaixaKind::Biblioteca.is_biblioteca());
            assert!(CaixaKind::Servico.is_servico());
            assert!(CaixaKind::Supervisor.is_supervisor());
            assert!(CaixaKind::Binario.is_binario());
            assert!(CaixaKind::Acao.is_acao());
        }
    }

    #[test]
    fn caixa_kind_wire_name_returns_lifted_peer_const() {
        // Fail-before-pass-after pin: the six [`CaixaKind::wire_name`]
        // match arms each return one of the paired
        // [`crate::render::CAIXA_KIND_WIRE_*`] lifted consts, and a
        // future rebrand touching either endpoint (a per-consumer
        // disambiguation of the `:kind` vocabulary, a rename lands on
        // one arm's byte-string without touching the peer axis) would
        // silently desynchronize the emitter from the paired
        // [`CaixaKind::from_wire`] parser's accept-set. Pinning the six
        // arms to the six lifted consts makes any future drift a
        // caixa-core-build-time failure — peer of the sibling
        // [`caixa_kind_as_str_returns_lifted_peer_const`] pin on the
        // human-readable-label axis.
        for (variant, expected) in [
            (
                CaixaKind::Biblioteca,
                crate::render::CAIXA_KIND_WIRE_BIBLIOTECA,
            ),
            (CaixaKind::Binario, crate::render::CAIXA_KIND_WIRE_BINARIO),
            (CaixaKind::Servico, crate::render::CAIXA_KIND_WIRE_SERVICO),
            (
                CaixaKind::Supervisor,
                crate::render::CAIXA_KIND_WIRE_SUPERVISOR,
            ),
            (
                CaixaKind::Aplicacao,
                crate::render::CAIXA_KIND_WIRE_APLICACAO,
            ),
            (CaixaKind::Acao, crate::render::CAIXA_KIND_WIRE_ACAO),
        ] {
            assert_eq!(
                variant.wire_name(),
                expected,
                "CaixaKind::{variant:?}.wire_name() must return the \
                 lifted CAIXA_KIND_WIRE_* const"
            );
        }
    }

    #[test]
    fn caixa_kind_wire_name_matches_serialize_wire_byte_string() {
        // Load-bearing pin on the derive-to-const identity: the six
        // [`CaixaKind::wire_name`] outputs must byte-equal the
        // un-`rename`d `Serialize` derive's per-arm wire scalar (the
        // JSON quoted-string with its outer quotes stripped). A future
        // accidental `#[serde(rename_all = "…")]` attribute drift at
        // the derive surface would silently split the wire byte-shape
        // every K8s-CR / tatara-lisp author-surface / future-M4-CR
        // materializer consumer emits through this typed dispatch from
        // the shape the un-`rename`d derive projects — the drift's
        // apply-time symptom (a K8s Caixa CR whose `spec.kind` no
        // longer round-trips through `caixa_from_cr` because the
        // reverse [`CaixaKind::from_wire`] parser rejects the new
        // rename-transformed byte-string) would surface far from the
        // derive attribute's commit. Pinning the identity here makes
        // any such drift a caixa-core-build-time failure. Peer of the
        // sibling
        // [`caixa_kind_display_matches_as_str_and_not_serialize_wire`]
        // pin — that one keeps `Display` structurally *distinct* from
        // the wire byte-shape (by design); this one keeps
        // [`CaixaKind::wire_name`] structurally *aligned* with the
        // wire byte-shape (by design).
        for &variant in CaixaKind::ALL {
            let json = serde_json::to_string(&variant).unwrap();
            let unquoted = json
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .expect("serialized CaixaKind is a JSON string");
            assert_eq!(
                variant.wire_name(),
                unquoted,
                "CaixaKind::{variant:?}.wire_name() must byte-equal the \
                 un-renamed Serialize derive's wire scalar — a mismatch \
                 means either the derive attributes drifted or the \
                 CAIXA_KIND_WIRE_* const family drifted; either way \
                 downstream K8s-CR round-trip through caixa_from_cr \
                 silently splits from the accessor-routed source of \
                 truth"
            );
        }
    }

    #[test]
    fn caixa_kind_wire_round_trips_through_from_wire() {
        // Total-round-trip pin on the (wire_name, from_wire) pair:
        // every arm's [`CaixaKind::wire_name`] output must parse back
        // through [`CaixaKind::from_wire`] to the same variant. Any
        // future accessor extension that adds a new arm to one side of
        // the pair without extending the other — a new `CaixaKind`
        // variant whose `wire_name` arm lands but whose `from_wire`
        // arm is forgotten, or a rename that touches `wire_name`'s
        // per-arm const without threading through `from_wire`'s peer
        // arm — trips here at caixa-core build time rather than
        // surfacing as a downstream K8s-CR round-trip miss (a
        // `caixa_into_cr` emit that lands a `spec.kind` byte-string
        // the paired `caixa_from_cr`'s `CaixaKind::from_wire` cannot
        // parse, silently falling through to the caller's
        // `.unwrap_or(CaixaKind::Biblioteca)` fallback).
        for &variant in CaixaKind::ALL {
            let wire = variant.wire_name();
            let parsed = CaixaKind::from_wire(wire).unwrap_or_else(|| {
                panic!(
                    "CaixaKind::from_wire({wire:?}) must accept every \
                     CaixaKind::wire_name output — got None for the \
                     wire byte-string of {variant:?}"
                )
            });
            assert_eq!(
                parsed, variant,
                "CaixaKind::from_wire(CaixaKind::{variant:?}.wire_name()) \
                 must return CaixaKind::{variant:?} — the (wire_name, \
                 from_wire) pair must form a total round-trip on the \
                 closed six-arm CaixaKind arm-set"
            );
        }
    }

    #[test]
    fn caixa_kind_from_wire_rejects_unknown_byte_strings() {
        // Rejection pin on the parser's accept-set: any string outside
        // the six-arm [`CaixaKind::wire_name`] output set must return
        // `None`. A future accidental widening of the accept-set (a
        // case-insensitive match that accepts `"biblioteca"` on the
        // wire axis, a hand-rolled Levenshtein-forgiving arm-lookup
        // that admits `"Biblioteka"` typos) would silently drift the
        // parser's accept-set from the emitter's — a K8s CR carrying
        // a malformed `spec.kind` byte-string that today's parser
        // rejects (letting the caller's `.unwrap_or(Biblioteca)`
        // fallback fire on the operator-visible-drift path) would then
        // land on a plausibly-wrong typed arm the caller does not
        // route through the fallback, silently binding the CR to the
        // wrong runtime contract. Also rejects the sibling
        // lowercase-Portuguese diagnostic-form strings (`"biblioteca"`
        // / `"servico"`), which are the *human-readable* form the
        // [`CaixaKind::as_str`] axis emits — the two-axis split
        // documented on the sibling
        // [`caixa_kind_display_matches_as_str_and_not_serialize_wire`]
        // pin explicitly forbids accepting one axis's byte-shapes as
        // parseable on the other axis.
        for bad in [
            "",
            "biblioteca",
            "servico",
            "aplicacao",
            "ACao",
            "unknown",
            "actor",
            "Biblioteka",
        ] {
            assert!(
                CaixaKind::from_wire(bad).is_none(),
                "CaixaKind::from_wire({bad:?}) must return None — the \
                 parser's accept-set is exactly the six CaixaKind::wire_name \
                 outputs; a widening would silently split the parser's \
                 accept-set from the emitter's"
            );
        }
    }

    #[test]
    fn caixa_kind_wire_name_is_const_fn() {
        // Const-context pin: [`CaixaKind::wire_name`] must remain
        // `const fn` (its match arms return `pub const` byte-strings,
        // so no non-const operation exists on the resolution path).
        // Downstream consumers reaching for the accessor from a
        // `const` context (a future substrate-wide const-fold-driven
        // audit table that materializes every kind's wire byte-string
        // at build time, a `const` gate on a per-arm CR-schema
        // registration) rely on the const-ness. A future accidental
        // downgrade to non-`const` (an added runtime helper reachable
        // only from a non-`const` context, a manual hand-rolled `impl`
        // that shadows this method) trips at caixa-core build time
        // rather than surfacing as a downstream `const`-context
        // regression far from the accessor declaration. Peer of the
        // sibling [`caixa_kind_is_variant_predicates_are_const_fn`]
        // pin on the [`gen_platform::IsVariant`] derive's per-arm
        // predicates.
        const BIBLIOTECA_WIRE: &str = CaixaKind::Biblioteca.wire_name();
        const BINARIO_WIRE: &str = CaixaKind::Binario.wire_name();
        const SERVICO_WIRE: &str = CaixaKind::Servico.wire_name();
        const SUPERVISOR_WIRE: &str = CaixaKind::Supervisor.wire_name();
        const APLICACAO_WIRE: &str = CaixaKind::Aplicacao.wire_name();
        const ACAO_WIRE: &str = CaixaKind::Acao.wire_name();
        assert_eq!(BIBLIOTECA_WIRE, "Biblioteca");
        assert_eq!(BINARIO_WIRE, "Binario");
        assert_eq!(SERVICO_WIRE, "Servico");
        assert_eq!(SUPERVISOR_WIRE, "Supervisor");
        assert_eq!(APLICACAO_WIRE, "Aplicacao");
        assert_eq!(ACAO_WIRE, "Acao");
    }

    #[test]
    fn caixa_kind_wire_consts_are_pairwise_distinct() {
        // Distinctness pin: the six [`crate::render::CAIXA_KIND_WIRE_*`]
        // consts must be pairwise distinct — an accidental copy-paste
        // flip that reroutes one arm's byte-string to also match
        // another silently collapses two per-kind wire arms onto one,
        // so a downstream K8s CR carrying the collapsed byte-string
        // round-trips through [`CaixaKind::from_wire`] to whichever
        // arm the parser's match cascade lands on first (the collapse
        // makes the outcome match-arm-ordering-dependent). Peer of the
        // sibling [`caixa_kind_label_consts_are_pairwise_distinct`]
        // pin on the human-readable-label axis.
        let all = [
            crate::render::CAIXA_KIND_WIRE_BIBLIOTECA,
            crate::render::CAIXA_KIND_WIRE_BINARIO,
            crate::render::CAIXA_KIND_WIRE_SERVICO,
            crate::render::CAIXA_KIND_WIRE_SUPERVISOR,
            crate::render::CAIXA_KIND_WIRE_APLICACAO,
            crate::render::CAIXA_KIND_WIRE_ACAO,
        ];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                if i != j {
                    assert_ne!(
                        a, b,
                        "CAIXA_KIND_WIRE_* consts must be pairwise \
                         distinct — found duplicate byte-string {a:?} \
                         at indices {i} and {j}"
                    );
                }
            }
        }
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

    #[test]
    fn caixa_kind_all_enumerates_every_variant_exactly_once() {
        // Fail-before-pass-after pin on the [`CaixaKind::ALL`]
        // exhaustive-iteration surface: the slice length matches the
        // arm count of the closed six-arm set, every variant appears
        // at least once, and no variant appears twice. A future arm
        // addition (an `Actor` virtual-actor arm the M5 Orleans-
        // inspired kind reaches through, per the sibling
        // [`CaixaKind::from_wire`] doc block) that grows the enum but
        // forgets to grow [`Self::ALL`] silently truncates every
        // downstream consumer's accept-set at the pre-addition
        // boundary — a future `feira --kind …` CLI-side "did you
        // mean" scan that iterates the slice, the future M4
        // admission-webhook's rejection body naming the accepted-
        // `:kind` list, any future round-trip fuzz harness — all read
        // through this slice, so a truncation there silently splits
        // every accept-set from the arm-set the paired
        // [`gen_platform::IsVariant`] predicates + [`Self::wire_name`]
        // / [`Self::as_str`] / [`Self::from_wire`] siblings walk.
        // Pinning the exhaustive-enumeration invariant here catches
        // the drift at caixa-core build time.
        //
        // Peer of the sibling
        // [`crate::aplicacao::tests::placement_strategy_all_enumerates_every_variant_once`]
        // (18c7342) / `rate_limit_unit_all_enumerates_every_variant_once`
        // (6bce03d) / `dep_list_all_enumerates_every_variant_once`
        // (45ee563) pins on the peer closed-set typed-enum axes.
        let all: &[CaixaKind] = CaixaKind::ALL;
        assert_eq!(
            all.len(),
            6,
            "CaixaKind::ALL must enumerate every variant of the \
             six-arm closed set (Biblioteca, Binario, Servico, \
             Supervisor, Aplicacao, Acao); got {all:?}"
        );
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                if i != j {
                    assert_ne!(
                        a, b,
                        "CaixaKind::ALL must carry every variant \
                         exactly once — got duplicate {a:?} at \
                         indices {i} and {j}"
                    );
                }
            }
        }
        for variant in [
            CaixaKind::Biblioteca,
            CaixaKind::Binario,
            CaixaKind::Servico,
            CaixaKind::Supervisor,
            CaixaKind::Aplicacao,
            CaixaKind::Acao,
        ] {
            assert!(
                all.contains(&variant),
                "CaixaKind::ALL must contain {variant:?} — a future \
                 variant addition that grows the enum but forgets to \
                 grow the ALL slice silently truncates every \
                 downstream consumer's accept-set at the pre-addition \
                 boundary"
            );
        }
    }

    #[test]
    fn caixa_kind_all_is_the_from_wire_accept_set() {
        // Load-bearing pin on the two-axis identity: [`CaixaKind::ALL`]
        // is exactly the variant image of the [`CaixaKind::from_wire`]
        // accept-set — every arm in the slice parses back through
        // `from_wire ∘ wire_name` to itself, and the paired
        // [`caixa_kind_from_wire_rejects_unknown_byte_strings`] pin
        // guarantees `from_wire` rejects everything outside the six-
        // arm wire-string image. Together these two pins make the
        // slice the authoritative arm-set every consumer of the
        // closed six-arm [`CaixaKind`] discriminator reads through:
        // the future M4 admission-webhook can enumerate accepted
        // `:kind` values by walking [`Self::ALL`] and rendering each
        // arm's `wire_name`, the future `feira --kind …` "did you
        // mean" hint can score against the same slice, and no
        // consumer needs to re-inline a six-arm literal list. Any
        // future arm addition that grows one axis and forgets the
        // other trips here at caixa-core build time.
        for &variant in CaixaKind::ALL {
            let wire = variant.wire_name();
            assert_eq!(
                CaixaKind::from_wire(wire),
                Some(variant),
                "CaixaKind::from_wire(CaixaKind::{variant:?}.wire_name() = \
                 {wire:?}) must return Some({variant:?}) — CaixaKind::ALL \
                 must be a subset of the from_wire accept-set"
            );
        }
    }

    #[test]
    fn caixa_kind_as_ref_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl AsRef<str> for CaixaKind` — asserts the standard-library
        // trait impl and the substrate-primitive [`CaixaKind::as_str`]
        // `pub const fn` accessor resolve to the same `&str` per
        // instance across the six-arm closed set, so any future silent
        // detour that routes the impl through a divergent projection
        // (a per-arm inline `match self { CaixaKind::Servico => "servico",
        // … }` re-inlining that opens a compile-time link to the
        // un-lifted arm-literal, a swap onto the PascalCase
        // [`CaixaKind::wire_name`] axis that would collide the
        // diagnostic / wire two-axis split the sibling
        // [`caixa_kind_display_matches_as_str_and_not_serialize_wire`]
        // pin makes load-bearing) trips at caixa-core test time under
        // `assert_eq!` rather than at a downstream `impl AsRef<str>`-
        // bound consumer's silent split. Sweeps every one of the six
        // arms [`CaixaKind::ALL`] carries so no arm's projection is
        // covered only by the sibling wire-format `Serialize` derive
        // path. Peer of the sibling
        // [`crate::supervisor::tests::restart_policy_as_ref_str_routes_through_as_str_accessor`]
        // (419ea81) /
        // [`crate::supervisor::tests::restart_strategy_as_ref_str_routes_through_as_str_accessor`]
        // (63eb1a4) /
        // [`crate::aplicacao::tests::placement_strategy_as_ref_str_routes_through_as_str_accessor`]
        // (d86edd2) pins on the paired M2/M3 closed-set typed enums,
        // and the
        // [`crate::version::tests::caixa_version_as_ref_str_routes_through_as_str_accessor`]
        // (16d5c7e) pin on the paired top-level `:versao` typed
        // newtype — the five pins together close the substrate
        // primitive's `AsRef<str>` projection axis on every closed-set
        // typed enum/newtype on the top-level + M2 + M3 caixa surface.
        for &variant in CaixaKind::ALL {
            assert_eq!(
                <CaixaKind as AsRef<str>>::as_ref(&variant),
                variant.as_str(),
                "AsRef<str> impl on CaixaKind::{variant:?} must \
                 byte-equal CaixaKind::as_str on the same instance — \
                 divergence signals a silent detour off the substrate-\
                 primitive accessor"
            );
        }
    }

    #[test]
    fn caixa_kind_as_ref_str_routes_through_display_via_shared_accessor() {
        // Fail-before-pass-after byte-parity pin on the three-path
        // convergence discipline the top-level [`CaixaKind`] discriminator
        // now carries on the diagnostic-`&str`-projection axis:
        // `<CaixaKind as AsRef<str>>::as_ref(&v)` (the newly lifted
        // impl), `format!("{v}")` (the pre-existing [`fmt::Display`]
        // impl), and `v.as_str()` (the substrate-primitive
        // `pub const fn` accessor both trait impls delegate through)
        // must resolve to the same byte-string on every instance
        // across the six-arm closed set. Refuses any future divergence
        // between the two trait impls (a stray [`fmt::Display::fmt`]
        // rewrite that hand-rolls the arms rather than delegating
        // through the shared accessor; a hypothetical `AsRef<str>`
        // rewrite that inlines a per-arm literal cascade) that would
        // silently split the two projection paths of the top-level
        // typed discriminator. Preserves the two-axis split the
        // sibling
        // [`caixa_kind_display_matches_as_str_and_not_serialize_wire`]
        // pin makes load-bearing: this pin asserts three paths
        // *converge* on the diagnostic axis, and the sibling pin
        // asserts the wire axis stays *distinct* from it. Mirrors the
        // sibling three-path-convergence discipline the peer
        // [`crate::aplicacao::PlacementStrategy`] carries on its
        // `AsRef<str>` / `Display` / `as_str` triple (aplicacao.rs pin
        // `placement_strategy_as_ref_str_routes_through_display_via_shared_accessor`,
        // d86edd2), the peer [`crate::supervisor::RestartPolicy`]
        // triple (supervisor.rs pin
        // `restart_policy_as_ref_str_routes_through_display_via_shared_accessor`,
        // 419ea81), the peer [`crate::supervisor::RestartStrategy`]
        // triple (supervisor.rs pin
        // `restart_strategy_as_ref_str_routes_through_display_via_shared_accessor`,
        // 63eb1a4), and the [`crate::CaixaVersion`] typed newtype
        // triple (version.rs pin
        // `caixa_version_as_ref_str_routes_through_display_via_shared_accessor`,
        // 16d5c7e).
        for &variant in CaixaKind::ALL {
            let via_as_ref: &str = <CaixaKind as AsRef<str>>::as_ref(&variant);
            let via_display: String = format!("{variant}");
            let via_accessor: &str = variant.as_str();
            assert_eq!(via_as_ref, via_accessor);
            assert_eq!(via_display, via_accessor);
            assert_eq!(via_as_ref, via_display.as_str());
        }
    }

    #[test]
    fn caixa_kind_all_is_const_and_matches_iteration_count() {
        // Const-context pin: [`CaixaKind::ALL`] is a `pub const`
        // slice, so it materializes at build time. Downstream
        // consumers reaching for the slice from a `const` context (a
        // future substrate-wide const-fold-driven audit table that
        // materializes every kind's wire byte-string at build time, a
        // per-arm CR-schema registration in a `const` gate) rely on
        // the const-ness. A future accidental downgrade to a runtime
        // `fn ALL() -> Vec<Self>` reachable only from a non-`const`
        // context trips at caixa-core build time. Peer of the sibling
        // [`caixa_kind_wire_name_is_const_fn`] +
        // [`caixa_kind_is_variant_predicates_are_const_fn`] pins on
        // the peer accessor axes.
        const ALL: &[CaixaKind] = CaixaKind::ALL;
        const LEN: usize = ALL.len();
        assert_eq!(
            LEN, 6,
            "CaixaKind::ALL length must byte-equal the six-arm closed-set \
             cardinality at const-fold time"
        );
    }
}
