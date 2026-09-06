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

/// Trait-idiomatic reverse projection on the top-level [`CaixaKind`]
/// closed-set typed enum — routes byte-for-byte through the paired
/// substrate-primitive [`CaixaKind::from_wire`] `Option<Self>` accessor
/// so every future consumer that binds a `PascalCase` wire byte-string
/// through the standard-library `.try_into()` / [`TryFrom`] axis (a
/// future `feira --kind <Biblioteca|Servico|…>` CLI arg-parse that
/// composes into `let kind: CaixaKind = s.try_into()?`, a future
/// `mesh.pleme.io/v1alpha1/Caixa` CR admission-webhook that folds a
/// `spec.kind: String` field through `CaixaKind::try_from(&s)?`, a
/// generic `<T: TryFrom<&str>>`-bound loader over any of the
/// substrate's closed-set typed enums) reaches the same six-arm
/// accept-set the sibling [`CaixaKind::from_wire`] parses through and
/// the sibling [`CaixaKind::wire_name`] emits, rather than an open-
/// coded per-arm `match s { "Biblioteca" => …, … }` cascade whose
/// arm-set has no compile-time link back to the substrate primitive.
///
/// Complements the pre-existing forward-projection triple
/// ([`std::fmt::Display`], [`AsRef<str>`], [`CaixaKind::as_str`]) with
/// the paired trait-idiomatic reverse-projection axis: Rust-side
/// newtype/typed-enum convention pairs [`AsRef<str>`] with either
/// [`std::str::FromStr`] or [`TryFrom<&str>`] on the same primitive so
/// a caller who can project *out to* a `&str` can also project *in
/// from* one. The [`TryFrom<&str>`] axis is deliberately chosen over
/// [`std::str::FromStr`] to sidestep the `clippy::should_implement_trait`
/// lint that the sibling method-named `from_wire` would trigger under
/// a `FromStr` impl (the same design tradeoff the peer
/// [`crate::provedor::ferrite::FerriteRuntime::from_wire`] block
/// notes) — this impl closes the trait-idiomatic reverse axis without
/// disturbing the method-named `from_wire` shape every sibling
/// closed-set typed enum on the substrate already carries.
///
/// `type Error = ()` matches the sibling [`CaixaKind::from_wire`]'s
/// `Option<Self>` return-shape's deliberate deferral of error typing:
/// the caller picks the diagnostic form appropriate for its use site
/// (a future `feira --kind` arg-parse composes its own per-verb
/// "unknown kind: <arg> — accepted: {…}" message enumerating
/// [`CaixaKind::ALL`], a future admission-webhook rejection body
/// wraps the `Err(())` outcome with the accepted-set enumeration for
/// operator diagnostics, a `Result::map_err` at the call site lifts
/// the unit-error to a per-verb error type). Same shape the peer
/// [`FerriteRuntime::from_wire`] doc block motivates on the
/// `caixa-provedor` closed-set typed enum's reverse projection.
///
/// The paired [`TryFrom<&str>`] impl reaches the same six-arm accept-
/// set the [`CaixaKind::from_wire`] resolver dispatches through, so
/// any future arm addition (a virtual-actor `Actor` arm the
/// [`ABSORPTION-ROADMAP`](https://github.com/pleme-io/theory/blob/main/ABSORPTION-ROADMAP.md)
/// M5 Orleans-inspired kind reaches through) grows the trait-
/// idiomatic axis by construction — one caixa-core edit on
/// [`CaixaKind::from_wire`] extends both the method-named reverse
/// projection every existing consumer keys off and the trait-
/// idiomatic reverse projection this impl exposes, without a
/// coordinated rewrite across every future `TryFrom<&str>`-bound
/// consumer's arm-set.
///
/// Pinned load-bearing by
/// [`tests::caixa_kind_try_from_str_routes_through_from_wire_accessor`]
/// (byte-parity pin against [`CaixaKind::from_wire`] across the
/// six-arm accept-set) and
/// [`tests::caixa_kind_try_from_str_rejects_unknown_byte_strings`]
/// (rejection witness against silent accept-set widening).
impl TryFrom<&str> for CaixaKind {
    type Error = ();

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::from_wire(s).ok_or(())
    }
}

/// Trait-idiomatic *forward* projection on the structurally most
/// fundamental closed-set typed enum on the caixa surface — every
/// [`crate::Caixa`] carries a `:kind` — routing through the paired
/// substrate-primitive [`CaixaKind::as_str`] `pub const fn` accessor so
/// every future consumer that binds a [`CaixaKind`] through the
/// standard-library `.into()` / [`From<Self> for &'static str`]
/// (equivalently [`Into<&'static str>`]) axis (a future
/// `tracing::field::valuable::Value::Str(kind.into())` structured-log
/// recorder where the `Str` arm typing demands `&'static str` and the
/// sibling [`AsRef<str>`] impl's borrowed `&str` return-type does not
/// satisfy the bound, a future
/// `Cow::Borrowed::<'static, str>(kind.into())` composer on the future
/// M4 admission-webhook rejection body where the `Cow<'static, str>`
/// typing rules out the sibling [`AsRef<str>`] borrowed return, a
/// generic `<T: Into<&'static str>>`-bound serializer on a per-kind
/// diagnostic column, a per-kind [`std::collections::HashMap`] whose
/// key type is `&'static str` requiring the `'static` bound through
/// [`Into::into`]) reaches the same lifted
/// [`crate::render::CAIXA_KIND_LABEL_BIBLIOTECA`] /
/// [`crate::render::CAIXA_KIND_LABEL_BINARIO`] /
/// [`crate::render::CAIXA_KIND_LABEL_SERVICO`] /
/// [`crate::render::CAIXA_KIND_LABEL_SUPERVISOR`] /
/// [`crate::render::CAIXA_KIND_LABEL_APLICACAO`] /
/// [`crate::render::CAIXA_KIND_LABEL_ACAO`] const the paired
/// [`std::fmt::Display`], [`AsRef<str>`], and [`CaixaKind::as_str`]
/// surfaces already return, rather than an open-coded per-arm
/// `match kind { Biblioteca => "biblioteca", … }` cascade whose
/// arm-set has no compile-time link back to the substrate primitive.
///
/// Complements the pre-existing quadruple ([`std::fmt::Display`],
/// [`AsRef<str>`], [`CaixaKind::as_str`], [`TryFrom<&str>`] via
/// 3c83606) with the paired trait-idiomatic forward-projection axis:
/// Rust-side newtype/typed-enum convention pairs [`TryFrom<&str>`]
/// (trait-idiomatic reverse) with [`From<Self> for &'static str`]
/// (trait-idiomatic forward) on the same primitive so a caller who
/// can project *in from* a `&str` via the trait axis can also project
/// *out to* one — mirroring the `strum::IntoStaticStr` /
/// `serde::Serialize`-shape idiom where both projection halves share
/// one trait-driven vocabulary. Before this lift the substrate
/// carried a `&str`-returning [`AsRef<str>`] but not the paired
/// `&'static str`-returning [`From<Self> for &'static str`] axis
/// every downstream generic that specifically needs `'static`
/// byte-string bytes reaches for.
///
/// Deliberately routes through the human-readable
/// [`CaixaKind::as_str`] axis, not the `PascalCase`
/// [`CaixaKind::wire_name`] axis — the two-axis split the sibling
/// [`tests::caixa_kind_display_matches_as_str_and_not_serialize_wire`]
/// pin makes load-bearing is preserved here by construction: the
/// paired [`AsRef<str>`], [`std::fmt::Display`], and this
/// [`From<Self> for &'static str`] trait triple all land on the
/// diagnostic byte-string, while the wire axis (tatara-lisp author
/// surface `:kind Biblioteca`) stays reachable only through the
/// explicit [`CaixaKind::wire_name`] + [`serde::Serialize`] paths.
///
/// The paired [`CaixaKind::as_str`] returns `&'static str` by
/// construction (each `match` arm resolves to a
/// [`crate::render::CAIXA_KIND_LABEL_*`] `pub const &str` with static
/// lifetime), so the trait's return-type promise is upheld
/// structurally. Any future silent detour that routes the impl
/// through a non-static projection (a per-arm inline
/// `String::from("biblioteca")`-shaped re-inlining that would
/// `.leak()`-cast for the `'static` bound, a hypothetical rebrand of
/// one arm's [`crate::render::CAIXA_KIND_LABEL_*`] const to a
/// non-`const &str`) is a caixa-core-build-time failure through the
/// `pub const fn as_str` signature the trait routes through.
///
/// The paired impl reaches the same six-arm emit-set the
/// [`CaixaKind::as_str`] accessor dispatches through, so any future
/// arm addition (a virtual-actor `Actor` arm the
/// [`ABSORPTION-ROADMAP`](https://github.com/pleme-io/theory/blob/main/ABSORPTION-ROADMAP.md)
/// M5 Orleans-inspired kind reaches through — a candidate future arm
/// named in the sibling [`CaixaKind::from_wire`] doc block) grows
/// the trait-idiomatic forward axis by construction — one caixa-core
/// edit on [`CaixaKind::as_str`] extends every one of the four
/// sibling forward-projection paths ([`std::fmt::Display`],
/// [`AsRef<str>`], [`CaixaKind::as_str`] itself, and this
/// [`From<Self> for &'static str`]) without a coordinated rewrite
/// across every future `Into<&'static str>`-bound consumer's arm-set.
///
/// Extends the substrate-wide trait-idiomatic *forward*-projection
/// family opened on [`crate::supervisor::RestartStrategy`] (523157d)
/// and [`crate::supervisor::RestartPolicy`] (9fb37d0) — the two M2
/// OTP-shape closed-set typed enum peers whose paired
/// `From<Self> for &'static str` axis established the "route through
/// `as_str`" precedent — onto the structurally most fundamental
/// closed-set typed enum on the caixa surface. Third peer on the
/// forward-projection family, and the first outside the M2 OTP-shape
/// sibling axis pair; the remaining eleven closed-set typed enums on
/// the caixa substrate surface (`CaixaDialeto`, `PlacementStrategy`,
/// `WitShape`, `RateLimitUnit`, `PathShapeViolation`, `InvariantKind`,
/// `ArchVerdict`, `Severity`, `FixSafety`, `Semantic`, `FerriteRuntime`)
/// are the future targets of this campaign.
///
/// Pinned load-bearing by
/// [`tests::caixa_kind_from_into_static_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaKind::as_str`] across the six-arm
/// emit-set, plus a `const`-context materialization witness for the
/// `&'static str` lifetime promise) and
/// [`tests::caixa_kind_from_into_static_str_and_as_str_partition_the_emit_set`]
/// (partition pin asserting `<&'static str as From<CaixaKind>>::from`
/// and [`CaixaKind::as_str`] agree on every arm, plus a two-way
/// round-trip witness through the paired trait-idiomatic
/// [`TryFrom<&str>`] axis on the `PascalCase` wire vocabulary that
/// closes the two-way `Self ↔ &'static str` round-trip on the
/// trait-idiomatic axis pair via the intermediate
/// [`CaixaKind::wire_name`] axis — the two projection endpoints
/// resolve to different vocabularies by design, so the round-trip
/// crosses through the wire axis rather than composing the two trait
/// impls directly).
impl From<CaixaKind> for &'static str {
    fn from(kind: CaixaKind) -> &'static str {
        kind.as_str()
    }
}

/// Trait-idiomatic *forward* projection on [`CaixaKind`] from a
/// *borrowed* input onto the `&'static str` axis — the borrowed-input
/// companion to the paired owned-input [`From<CaixaKind> for &'static
/// str`] impl immediately above. Routes byte-for-byte through the same
/// substrate-primitive [`CaixaKind::as_str`] `pub const fn` accessor so
/// every consumer that binds a `&CaixaKind` through the standard-
/// library `.into()` / [`From<&Self> for &'static str`] axis (a
/// `CaixaKind::ALL.iter().map(<&'static str>::from).collect::<Vec<_>>()`
/// per-arm accept-set materializer — whose iterator over
/// `&'static [CaixaKind]` yields `&CaixaKind`, not `CaixaKind`, so the
/// owned-input [`From<CaixaKind>`] axis alone forces every call site
/// through an explicit `.copied()` / dereference / [`Copy`]-bound
/// restatement rather than the direct trait-idiomatic projection; a
/// future generic `<T: Copy + for<'a> Into<&'static str>>`-bound
/// diagnostic column that walks the `iter().map(Into::into)` shape
/// verbatim; the future M4 admission-webhook rejection body that
/// composes the accepted-set enumeration from an iterated
/// `CaixaKind::ALL.iter().map(|k| k.into())` pipe rather than a
/// per-arm `match k { … }` cascade) reaches the same six-arm lifted
/// [`crate::render::CAIXA_KIND_LABEL_*`] const the paired owned-input
/// [`From<CaixaKind> for &'static str`], the sibling
/// [`std::fmt::Display`], [`AsRef<str>`], and [`CaixaKind::as_str`]
/// surfaces already return.
///
/// Second peer on the substrate-wide trait-idiomatic *borrowed-input*
/// forward-projection family opened on
/// [`crate::dep::DepList`] (64aa742). Rust's `From` trait does not
/// auto-derive the `From<&Self>` sibling from a `From<Self>` impl (the
/// blanket `impl<T, U> From<&T> for U where T: Copy, U: From<T>` does
/// not exist in `core`), so every closed-set typed enum that carries
/// the owned-input axis but not the borrowed-input axis forces every
/// borrowed-input call site through a `.copied()` /
/// `<&'static str>::from(*kind)` / `kind.as_str()` detour whose type
/// bounds have no compile-time link to the substrate primitive.
///
/// Pinned load-bearing by
/// [`tests::caixa_kind_from_borrowed_into_static_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaKind::as_str`] across the six-arm
/// emit-set via a borrowed input, plus a `const`-context materialization
/// witness) and
/// [`tests::caixa_kind_from_owned_and_borrowed_into_static_str_agree_on_every_arm`]
/// (cross-axis partition pin against the paired owned-input
/// [`From<CaixaKind> for &'static str`] impl, plus a
/// `.iter().map(Into::into)` pipe witness over [`CaixaKind::ALL`]).
impl From<&CaixaKind> for &'static str {
    fn from(kind: &CaixaKind) -> &'static str {
        kind.as_str()
    }
}

/// Trait-idiomatic *owned-`String`* forward projection on the
/// structurally most fundamental closed-set typed enum on the caixa
/// surface ([`CaixaKind`]) — the owned-heap-string companion to the
/// paired `&'static str`-returning [`From<CaixaKind> for &'static str`]
/// / [`From<&CaixaKind> for &'static str`] impls immediately above.
/// Routes byte-for-byte through the substrate-primitive
/// [`CaixaKind::as_str`] `pub const fn` accessor (via [`str::to_owned`])
/// so every consumer that binds a [`CaixaKind`] through the standard-
/// library `.into()` / [`From<Self> for String`] (equivalently
/// [`Into<String>`]) axis — a future `serde_json::Value::String(kind.into())`
/// structured-payload composer where the `Value::String` arm typing
/// demands an owned [`String`] and the sibling
/// [`&'static str`]-returning axis forces an explicit `.to_owned()` /
/// [`String::from`] restatement at every call site, a future
/// `HashMap::<String, CaixaKind>::from_iter(CaixaKind::ALL.iter().copied()
/// .map(|k| (k.into(), k)))` per-kind lookup where the map's key type
/// is owned [`String`] rather than [`&'static str`], a future
/// `Cow::<'static, str>::Owned(kind.into())` composer on the future M4
/// admission-webhook rejection body's owned-arm, the future wasm-
/// operator's per-Caixa post-decompose `serde_json::json!({ "kind":
/// kind })` diagnostic emit where the JSON serializer's [`serde::Serialize`]
/// impl on [`String`] owns the emit-path — reaches the same six-arm
/// lifted [`crate::render::CAIXA_KIND_LABEL_BIBLIOTECA`] /
/// [`crate::render::CAIXA_KIND_LABEL_BINARIO`] /
/// [`crate::render::CAIXA_KIND_LABEL_SERVICO`] /
/// [`crate::render::CAIXA_KIND_LABEL_SUPERVISOR`] /
/// [`crate::render::CAIXA_KIND_LABEL_APLICACAO`] /
/// [`crate::render::CAIXA_KIND_LABEL_ACAO`] const the paired
/// [`std::fmt::Display`], [`AsRef<str>`], [`CaixaKind::as_str`], and
/// the two `&'static str`-returning forward-projection impls already
/// return.
///
/// Extends the trait-idiomatic *owned-`String`* forward-projection
/// axis onto the structurally most fundamental closed-set typed enum
/// on the caixa substrate surface — third peer on the family opened
/// on [`crate::supervisor::RestartStrategy`] (7baa18a, first-mover)
/// and [`crate::supervisor::RestartPolicy`] (7851725, second peer on
/// the M2 OTP-shape sibling axis). Rust's standard library does not
/// carry a blanket `impl<T: AsRef<str>> From<T> for String` (nor an
/// `impl<T: fmt::Display> From<T> for String`), so every closed-set
/// typed enum that carries the paired [`AsRef<str>`] /
/// [`std::fmt::Display`] / [`From<Self> for &'static str`] triple
/// but not the owned-[`String`] axis forces every owned-string call
/// site through a `.to_string()` / `.as_str().to_owned()` /
/// `String::from(kind.as_str())` detour whose type bounds have no
/// compile-time link to the substrate primitive.
///
/// Deliberately routes through the human-readable
/// [`CaixaKind::as_str`] axis, not the `PascalCase`
/// [`CaixaKind::wire_name`] axis — the two-axis split the sibling
/// [`tests::caixa_kind_display_matches_as_str_and_not_serialize_wire`]
/// pin makes load-bearing is preserved here by construction: the
/// paired [`AsRef<str>`], [`std::fmt::Display`], the two
/// `&'static str`-returning forward-projection impls, and this
/// owned-[`String`] impl all land on the diagnostic byte-string,
/// while the wire axis (tatara-lisp author surface `:kind Biblioteca`)
/// stays reachable only through the explicit
/// [`CaixaKind::wire_name`] + [`serde::Serialize`] paths.
///
/// Unlike the peer M2 OTP-shape axis pair
/// ([`crate::supervisor::RestartPolicy`] whose forward `From` axis and
/// reverse [`TryFrom<&str>`] axis share one `PascalCase` vocabulary
/// by construction, so the owned-[`String`] forward + reverse
/// round-trip composes directly), the [`CaixaKind`] forward
/// owned-[`String`] axis emits lowercase Portuguese diagnostic bytes
/// (`"biblioteca"`) while the paired trait-idiomatic reverse
/// [`TryFrom<&str>`] axis (via [`CaixaKind::from_wire`]) parses
/// `PascalCase` wire bytes (`"Biblioteca"`) — the round-trip witness
/// crosses through [`CaixaKind::wire_name`] as the reverse-axis
/// vocabulary rather than composing the owned-[`String`] emit
/// directly.
///
/// Pinned load-bearing by
/// [`tests::caixa_kind_from_into_owned_string_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaKind::as_str`] across the six-arm
/// emit-set, plus a blanket `.into::<String>()` shape witness) and
/// [`tests::caixa_kind_from_into_owned_string_and_static_str_agree_on_every_arm`]
/// (cross-axis partition pin against the paired owned-input
/// [`From<CaixaKind> for &'static str`] impl and the sibling
/// [`ToString::to_string`] surface routed through
/// [`std::fmt::Display`], plus a `.iter().copied().map(String::from)`
/// pipe witness over [`CaixaKind::ALL`], plus a wire-vocab round-trip
/// witness through [`CaixaKind::wire_name`] + [`TryFrom<&str>`] that
/// closes the two-way `Self → String → Self` round-trip on the
/// trait-idiomatic owned-[`String`] forward axis + reverse axis pair
/// via the wire vocabulary rather than the emit vocabulary — the
/// two-axis split by design).
impl From<CaixaKind> for String {
    fn from(kind: CaixaKind) -> String {
        kind.as_str().to_owned()
    }
}

/// Trait-idiomatic *borrowed-input, owned-`String` output* forward
/// projection on the structurally most fundamental closed-set typed
/// enum on the caixa surface ([`CaixaKind`]) — the fourth (and
/// closing) corner of the `{Self, &Self} × {&'static str, String}`
/// 2×2 trait-idiomatic projection family on this enum, mirror of the
/// peer M2 OTP-shape [`From<&crate::supervisor::RestartStrategy> for
/// String`] (579385f) / [`From<&crate::supervisor::RestartPolicy> for
/// String`] (8465740) and the sibling two-list dep-graph
/// [`From<&crate::dep::DepList> for String`] (e0cb617) that opened
/// the corner off the M2 OTP-shape sibling axis pair onto the first
/// non-M2 closed-set fieldless typed enum peer. Routes byte-for-byte
/// through the substrate-primitive [`CaixaKind::as_str`] `pub const fn`
/// accessor (via [`str::to_owned`]) so every consumer that holds a
/// borrowed [`&CaixaKind`] and needs an owned [`String`] — a future
/// `serde_json::Value::String(String::from(&kind))` structured-payload
/// composer over a borrowed field, a future `Iterator::map` over
/// `&[CaixaKind]` that projects to owned keys through
/// `.iter().map(String::from)` (whose iterator yields `&CaixaKind`,
/// not `CaixaKind`, so the owned-input [`From<CaixaKind> for String`]
/// axis alone forces every call site through an explicit `.copied()` /
/// spurious [`Copy`] deref restatement rather than the direct trait-
/// idiomatic projection), a future
/// `HashMap::<String, CaixaKind>::from_iter` that keys off a borrowed-
/// iteration axis where dereferencing the kind would force an
/// unnecessary [`Copy`] at every step, the future wasm-operator's
/// per-manifest `kind_axes.iter().map(String::from).collect()`
/// per-kind author-surface-tag diagnostic emit whose iteration axis
/// is borrowed by construction — reaches the same six-arm lifted
/// [`crate::render::CAIXA_KIND_LABEL_BIBLIOTECA`] /
/// [`crate::render::CAIXA_KIND_LABEL_BINARIO`] /
/// [`crate::render::CAIXA_KIND_LABEL_SERVICO`] /
/// [`crate::render::CAIXA_KIND_LABEL_SUPERVISOR`] /
/// [`crate::render::CAIXA_KIND_LABEL_APLICACAO`] /
/// [`crate::render::CAIXA_KIND_LABEL_ACAO`] const the paired
/// [`std::fmt::Display`], [`AsRef<str>`], [`CaixaKind::as_str`], and
/// the three other trait-idiomatic forward-projection impls
/// ([`From<CaixaKind> for &'static str`],
/// [`From<&CaixaKind> for &'static str`],
/// [`From<CaixaKind> for String`]) already return.
///
/// Fourth peer on the substrate-wide trait-idiomatic *borrowed-input,
/// owned-`String` output* forward-projection family opened on
/// [`crate::supervisor::RestartStrategy`] (579385f), closed on the M2
/// OTP-shape sibling axis pair by [`crate::supervisor::RestartPolicy`]
/// (8465740), and extended onto the first non-M2 peer by
/// [`crate::dep::DepList`] (e0cb617) — extends the
/// `{Self, &Self} × {&'static str, String}` 2×2 projection corner off
/// the two-list dep-graph axis onto the structurally most fundamental
/// closed-set fieldless typed enum peer on the caixa surface (every
/// caixa carries a `:kind`). Rust's standard library does not carry a
/// blanket `impl<T: AsRef<str>> From<&T> for String` (nor an
/// `impl<T: fmt::Display> From<&T> for String`), so every closed-set
/// typed enum that carries the paired `AsRef<str>` / `Display` /
/// `From<Self> for &'static str` / `From<&Self> for &'static str` /
/// `From<Self> for String` quintuple but not the borrowed-input
/// owned-[`String`] axis forces every borrowed-input owned-string call
/// site through a `kind.as_str().to_owned()` / `String::from(*kind)`
/// (with a spurious [`Copy`]) / `kind.to_string()` (through
/// [`std::fmt::Display`]) detour whose type bounds have no compile-
/// time link to the substrate primitive.
///
/// Unlike the peer M2 OTP-shape axis pair
/// ([`crate::supervisor::RestartStrategy`] /
/// [`crate::supervisor::RestartPolicy`] whose forward `From` emit and
/// reverse [`TryFrom<&str>`] parse share one `PascalCase` vocabulary
/// by construction, so the borrowed-input owned-[`String`] forward +
/// reverse round-trip composes directly through the owned-[`String`]'s
/// [`String::as_str`] borrow) and unlike the two-list dep-graph peer
/// ([`crate::dep::DepList`] whose [`crate::dep::DepList::as_str`] emit
/// and [`crate::dep::DepList::from_wire`] parse resolve through the
/// same lifted [`crate::render::DEP_AUTHOR_KEY_DEPS`] /
/// [`crate::render::DEP_AUTHOR_KEY_DEPS_DEV`] consts by construction),
/// the [`CaixaKind`] forward borrowed-input owned-[`String`] axis
/// emits lowercase Portuguese diagnostic bytes (`"biblioteca"`) while
/// the paired trait-idiomatic reverse [`TryFrom<&str>`] axis (via
/// [`CaixaKind::from_wire`]) parses `PascalCase` wire bytes
/// (`"Biblioteca"`) — the two-axis split the sibling
/// [`tests::caixa_kind_display_matches_as_str_and_not_serialize_wire`]
/// pin makes load-bearing is preserved here by construction, so the
/// round-trip witness crosses through [`CaixaKind::wire_name`] as the
/// reverse-axis vocabulary rather than composing the borrowed-input
/// owned-[`String`] emit directly.
///
/// Pinned load-bearing by
/// [`tests::caixa_kind_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaKind::as_str`] across the six-arm
/// emit-set through the borrowed-input surface) and
/// [`tests::caixa_kind_from_into_borrowed_owned_string_agrees_with_paired_axes_on_every_arm`]
/// (cross-axis partition pin against the paired owned-input owned-
/// [`String`] [`From<CaixaKind> for String`] impl, the paired
/// borrowed-input owned-[`&'static str`]
/// [`From<&CaixaKind> for &'static str`] impl, the paired owned-input
/// owned-[`&'static str`] [`From<CaixaKind> for &'static str`] impl,
/// and the sibling [`ToString::to_string`] surface routed through
/// [`std::fmt::Display`], plus a `.iter().map(String::from)` pipe
/// witness over [`CaixaKind::ALL`] (whose iterator yields
/// `&CaixaKind` by construction, so the borrowed-input owned-
/// [`String`] axis is what routes the pipe through the substrate-
/// primitive [`CaixaKind::as_str`] accessor without a spurious
/// [`Copy`] deref), plus a round-trip witness through
/// [`CaixaKind::wire_name`] + [`TryFrom<&str>`] that closes the
/// two-way `&Self → String → wire → Self` round-trip via the wire
/// vocabulary rather than the emit vocabulary — the two-axis split
/// by design).
impl From<&CaixaKind> for String {
    fn from(kind: &CaixaKind) -> String {
        kind.as_str().to_owned()
    }
}

/// Trait-idiomatic *owned-input, [`std::borrow::Cow<'static, str>`]
/// output* forward projection on the structurally most fundamental
/// closed-set typed enum on the caixa surface ([`CaixaKind`]) — routes
/// byte-for-byte through the substrate-primitive [`CaixaKind::as_str`]
/// `pub const fn` accessor (via [`std::borrow::Cow::Borrowed`]) so every
/// consumer that binds a [`CaixaKind`] through the standard-library
/// `.into()` / [`From<Self> for std::borrow::Cow<'static, str>`]
/// (equivalently [`Into<Cow<'static, str>>`]) axis — a future
/// `axum::response::IntoResponse` body composer whose typing folds a
/// per-arm rejection line into a [`std::borrow::Cow<'static, str>`]
/// boundary, a future M4 admission-webhook rejection body composer
/// whose typing rules out the sibling [`AsRef<str>`] borrowed return
/// and the sibling [`From<Self> for &'static str`] axis's non-
/// [`Cow`]-parameterized shape, a future substrate-wide per-arm
/// diagnostic surface that folds either the zero-alloc
/// [`Cow::Borrowed`] arm (for the closed-set arms whose byte-string is
/// build-time-lifted) or the [`Cow::Owned`] arm (for a caller that
/// mutates the projection) through one uniform trait dispatch, a
/// future generic `<T: Into<Cow<'static, str>>>`-bound emitter on a
/// per-kind structured-log or admission-webhook rejection body — reaches
/// the same six-arm lifted
/// [`crate::render::CAIXA_KIND_LABEL_BIBLIOTECA`] /
/// [`crate::render::CAIXA_KIND_LABEL_BINARIO`] /
/// [`crate::render::CAIXA_KIND_LABEL_SERVICO`] /
/// [`crate::render::CAIXA_KIND_LABEL_SUPERVISOR`] /
/// [`crate::render::CAIXA_KIND_LABEL_APLICACAO`] /
/// [`crate::render::CAIXA_KIND_LABEL_ACAO`] const the paired
/// [`std::fmt::Display`], [`AsRef<str>`], [`CaixaKind::as_str`], and
/// the four
/// `{Self, &Self} × {&'static str, String}` 2×2 trait-idiomatic
/// forward-projection corners
/// ([`From<CaixaKind> for &'static str`],
/// [`From<&CaixaKind> for &'static str`],
/// [`From<CaixaKind> for String`],
/// [`From<&CaixaKind> for String`]) already return, rather than an
/// open-coded per-call-site `std::borrow::Cow::Borrowed(kind.as_str())`
/// / `std::borrow::Cow::Owned(kind.to_string())` composition whose type
/// bounds have no compile-time link back to the substrate primitive.
///
/// Deliberately returns [`std::borrow::Cow::Borrowed`] rather than
/// [`std::borrow::Cow::Owned`] — the substrate-primitive
/// [`CaixaKind::as_str`] accessor's return carries the `&'static str`
/// lifetime by construction (each `match` arm resolves to a
/// [`crate::render::CAIXA_KIND_LABEL_*`] `pub const &str` with static
/// lifetime), so the zero-alloc borrowed arm is the type-correct
/// projection with no runtime allocation. The paired
/// [`std::borrow::Cow::Owned`] arm stays reachable at the call site
/// through the existing [`From<CaixaKind> for String`] axis composed
/// with [`std::borrow::Cow::from`] on the resulting owned [`String`] —
/// a caller who chose to mutate the projection lands on the owned arm
/// by their own composition, not by the substrate-primitive projection
/// silently allocating on their behalf.
///
/// Deliberately routes through the human-readable
/// [`CaixaKind::as_str`] axis, not the `PascalCase`
/// [`CaixaKind::wire_name`] axis — the two-axis split the sibling
/// [`tests::caixa_kind_display_matches_as_str_and_not_serialize_wire`]
/// pin makes load-bearing is preserved here by construction: the
/// paired [`AsRef<str>`], [`std::fmt::Display`], the four
/// `{Self, &Self} × {&'static str, String}` 2×2 forward-projection
/// corners, and this [`std::borrow::Cow<'static, str>`] axis all land
/// on the diagnostic byte-string, while the wire axis (tatara-lisp
/// author surface `:kind Biblioteca`) stays reachable only through
/// the explicit [`CaixaKind::wire_name`] + [`serde::Serialize`] paths.
///
/// First-mover on the substrate-wide trait-idiomatic
/// [`std::borrow::Cow<'static, str>`] forward-projection family —
/// Rust's standard library does not carry a blanket
/// `impl<T: AsRef<str>> From<T> for Cow<'static, str>` (nor an
/// `impl<T: fmt::Display> From<T> for Cow<'static, str>`), so every
/// closed-set fieldless typed enum on the substrate that carries the
/// paired [`AsRef<str>`] / [`std::fmt::Display`] /
/// [`From<Self> for &'static str`] / [`From<&Self> for &'static str`] /
/// [`From<Self> for String`] / [`From<&Self> for String`] sextet but
/// not the [`std::borrow::Cow<'static, str>`] axis forces every
/// [`Cow<'static, str>`]-parameterized call site through a
/// `std::borrow::Cow::Borrowed(kind.as_str())` /
/// `std::borrow::Cow::Owned(kind.to_string())` / `String::from(kind)
/// .into()` detour whose type bounds have no compile-time link to the
/// substrate primitive. Opening the axis on the structurally most
/// fundamental closed-set fieldless typed enum peer on the caixa
/// surface (every caixa carries a `:kind`) establishes the "route
/// through `as_str` via [`Cow::Borrowed`]" discipline; every future
/// closed-set fieldless typed enum peer on the substrate
/// ([`crate::supervisor::RestartStrategy`],
/// [`crate::supervisor::RestartPolicy`],
/// [`crate::aplicacao::PlacementStrategy`],
/// [`crate::aplicacao::RateLimitUnit`], [`crate::dep::DepList`],
/// [`crate::dialeto::CaixaDialeto`], and the outside-`caixa-core`
/// peers `WitShape`, `PathShapeViolation`, `InvariantKind`,
/// `ArchVerdict`, `Severity`, `FixSafety`, `Semantic`,
/// `FerriteRuntime`) is a future target of the campaign, tracking the
/// same discipline the closed 2×2 corner already established.
///
/// Pinned load-bearing by
/// [`tests::caixa_kind_from_into_static_cow_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaKind::as_str`] across the six-arm
/// emit-set, plus a [`std::borrow::Cow::Borrowed`] discriminator
/// witness that the projection lands on the zero-alloc arm rather
/// than silently allocating through [`std::borrow::Cow::Owned`]) and
/// [`tests::caixa_kind_from_into_static_cow_str_agrees_with_paired_axes_on_every_arm`]
/// (cross-axis partition pin against the paired
/// [`From<CaixaKind> for &'static str`],
/// [`From<CaixaKind> for String`], and [`ToString::to_string`]
/// surfaces, plus a `.iter().copied().map(std::borrow::Cow::from)`
/// pipe witness over [`CaixaKind::ALL`]).
impl From<CaixaKind> for std::borrow::Cow<'static, str> {
    fn from(kind: CaixaKind) -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed(kind.as_str())
    }
}

/// Trait-idiomatic *borrowed-input, [`std::borrow::Cow<'static, str>`]
/// output* forward projection on the structurally most fundamental
/// closed-set typed enum on the caixa surface ([`CaixaKind`]) — the
/// borrowed-input companion to the paired owned-input
/// [`From<CaixaKind> for std::borrow::Cow<'static, str>`] impl
/// immediately above. Routes byte-for-byte through the same
/// substrate-primitive [`CaixaKind::as_str`] `pub const fn` accessor
/// (via [`std::borrow::Cow::Borrowed`]) so every consumer that holds a
/// borrowed [`&CaixaKind`] and needs a [`std::borrow::Cow<'static, str>`]
/// — a `CaixaKind::ALL.iter().map(std::borrow::Cow::from).collect::<Vec<_>>()`
/// per-arm accept-set materializer (whose iterator over
/// `&'static [CaixaKind]` yields `&CaixaKind`, not `CaixaKind`, so the
/// owned-input [`From<CaixaKind> for std::borrow::Cow<'static, str>`]
/// axis alone forces every call site through an explicit `.copied()` /
/// dereference / [`Copy`]-bound restatement rather than the direct
/// trait-idiomatic projection), a future generic
/// `<T: for<'a> Into<std::borrow::Cow<'static, str>>>`-bound emitter on
/// a per-kind diagnostic column that walks the `iter().map(Into::into)`
/// shape verbatim, the future M4 admission-webhook rejection body that
/// composes the accepted-set enumeration from an iterated
/// `CaixaKind::ALL.iter().map(|k| k.into())` pipe rather than a per-arm
/// `match k { … }` cascade — reaches the same six-arm lifted
/// [`crate::render::CAIXA_KIND_LABEL_BIBLIOTECA`] /
/// [`crate::render::CAIXA_KIND_LABEL_BINARIO`] /
/// [`crate::render::CAIXA_KIND_LABEL_SERVICO`] /
/// [`crate::render::CAIXA_KIND_LABEL_SUPERVISOR`] /
/// [`crate::render::CAIXA_KIND_LABEL_APLICACAO`] /
/// [`crate::render::CAIXA_KIND_LABEL_ACAO`] const the paired
/// [`std::fmt::Display`], [`AsRef<str>`], [`CaixaKind::as_str`], the
/// four `{Self, &Self} × {&'static str, String}` 2×2 trait-idiomatic
/// forward-projection corners, and the paired owned-input
/// [`From<CaixaKind> for std::borrow::Cow<'static, str>`] impl already
/// return.
///
/// Deliberately returns [`std::borrow::Cow::Borrowed`] rather than
/// [`std::borrow::Cow::Owned`] — the substrate-primitive
/// [`CaixaKind::as_str`] accessor's return carries the `&'static str`
/// lifetime by construction (each `match` arm resolves to a
/// [`crate::render::CAIXA_KIND_LABEL_*`] `pub const &str` with static
/// lifetime), so the zero-alloc borrowed arm is the type-correct
/// projection with no runtime allocation.
///
/// Second peer on the substrate-wide trait-idiomatic
/// [`std::borrow::Cow<'static, str>`] forward-projection family opened
/// on the paired owned-input [`From<CaixaKind> for std::borrow::Cow<'static, str>`]
/// impl one commit prior (99c1735) — closes the `{Self, &Self}` input-
/// shape corner of the [`Cow<'static, str>`] axis on the structurally
/// most fundamental closed-set fieldless typed enum peer on the caixa
/// surface. Rust's standard library does not carry a blanket
/// `impl<T: AsRef<str>> From<&T> for Cow<'static, str>` (nor an
/// `impl<T: fmt::Display> From<&T> for Cow<'static, str>`), so every
/// closed-set fieldless typed enum peer on the substrate that carries
/// the paired owned-input [`Cow<'static, str>`] axis but not the
/// borrowed-input axis forces every borrowed-input
/// [`Cow<'static, str>`]-parameterized call site through a spurious
/// [`Copy`] deref (`std::borrow::Cow::from(*kind)`) or a
/// `std::borrow::Cow::Borrowed(kind.as_str())` open-code whose type
/// bounds have no compile-time link to the substrate primitive.
///
/// Pinned load-bearing by
/// [`tests::caixa_kind_from_borrowed_into_static_cow_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaKind::as_str`] across the six-arm
/// emit-set through the borrowed-input surface, plus a
/// [`std::borrow::Cow::Borrowed`] discriminator witness that the
/// projection lands on the zero-alloc arm) and
/// [`tests::caixa_kind_from_borrowed_into_static_cow_str_agrees_with_paired_axes_on_every_arm`]
/// (cross-axis partition pin against the paired owned-input
/// [`From<CaixaKind> for std::borrow::Cow<'static, str>`], the paired
/// borrowed-input owned-`&'static str`
/// [`From<&CaixaKind> for &'static str`], and the paired borrowed-
/// input owned-`String` [`From<&CaixaKind> for String`] impls, plus a
/// `.iter().map(std::borrow::Cow::from)` pipe witness over
/// [`CaixaKind::ALL`] — whose iterator yields `&CaixaKind` by
/// construction, so the borrowed-input [`Cow<'static, str>`] axis is
/// what routes the pipe through the substrate-primitive
/// [`CaixaKind::as_str`] accessor with the zero-alloc
/// [`Cow::Borrowed`] arm by construction and without a spurious
/// [`Copy`] deref).
impl From<&CaixaKind> for std::borrow::Cow<'static, str> {
    fn from(kind: &CaixaKind) -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed(kind.as_str())
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
    fn caixa_kind_try_from_str_routes_through_from_wire_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl TryFrom<&str> for CaixaKind` — asserts the standard-
        // library trait impl and the substrate-primitive
        // [`CaixaKind::from_wire`] `Option<Self>` accessor resolve to
        // the same six-arm accept-set across every arm the exhaustive
        // [`CaixaKind::ALL`] slice enumerates. Any future silent detour
        // that routes the trait impl through a divergent projection
        // (a per-arm inline `match s { "Biblioteca" => Ok(Self::Biblioteca),
        // … }` re-inlining that opens a compile-time link to the
        // un-lifted arm-literal, a hypothetical `#[serde(rename_all =
        // "…")]` attribute drift that silently splits the wire byte-
        // string from every consumer that reaches for this typed
        // dispatch) trips at caixa-core test time under `assert_eq!`
        // rather than at a downstream `impl TryFrom<&str>`-bound
        // consumer's silent split. Sweeps every one of the six arms
        // [`CaixaKind::ALL`] carries so no arm's projection is covered
        // only by the sibling method-named `from_wire` path.
        for &variant in CaixaKind::ALL {
            let wire = variant.wire_name();
            assert_eq!(
                <CaixaKind as TryFrom<&str>>::try_from(wire),
                Ok(variant),
                "TryFrom<&str> impl on CaixaKind must round-trip \
                 CaixaKind::{variant:?}.wire_name() = {wire:?} back to \
                 Ok(CaixaKind::{variant:?}) — divergence from \
                 CaixaKind::from_wire signals a silent detour off the \
                 substrate-primitive accessor"
            );
            assert_eq!(
                <CaixaKind as TryFrom<&str>>::try_from(wire).ok(),
                CaixaKind::from_wire(wire),
                "TryFrom<&str> ok()-projection on {wire:?} must \
                 byte-equal CaixaKind::from_wire on the same input"
            );
        }
    }

    #[test]
    fn caixa_kind_try_from_str_rejects_unknown_byte_strings() {
        // Rejection witness on the `impl TryFrom<&str> for CaixaKind`
        // — sweeps a candidate set of byte-strings outside the six-arm
        // PascalCase wire accept-set the sibling [`CaixaKind::wire_name`]
        // emits and asserts every one lands on `Err(())`, so a future
        // accidental widening of the trait impl's accept-set (a stray
        // additional `_ if s.eq_ignore_ascii_case("Biblioteca") =>
        // Ok(…)` case-fold path, a silent inclusion of the lowercase
        // Portuguese [`CaixaKind::as_str`] surface onto the wire axis
        // that would collide the two-axis split the sibling
        // [`caixa_kind_display_matches_as_str_and_not_serialize_wire`]
        // pin makes load-bearing) trips at caixa-core test time. The
        // candidate set includes the empty string, the six-arm
        // lowercase Portuguese diagnostic byte-strings the peer
        // [`CaixaKind::as_str`] axis emits (a caller who confuses the
        // wire axis with the diagnostic axis trips here rather than at
        // a downstream consumer's silent reject), a lowercase / mixed-
        // case fold of each PascalCase arm (a caller who assumes
        // case-fold acceptance trips here), and a small residual set
        // of plausible-but-wrong strings.
        let rejected: &[&str] = &[
            "",
            "biblioteca",
            "binario",
            "servico",
            "supervisor",
            "aplicacao",
            "acao",
            "BIBLIOTECA",
            "BINARIO",
            "SERVICO",
            "SUPERVISOR",
            "APLICACAO",
            "ACAO",
            "biBlioteca",
            "Bibliotecas",
            "Servicos",
            "library",
            "binary",
            "service",
            "application",
            " Biblioteca",
            "Biblioteca ",
            "Biblioteca\n",
            "\"Biblioteca\"",
        ];
        for &input in rejected {
            assert_eq!(
                <CaixaKind as TryFrom<&str>>::try_from(input),
                Err(()),
                "TryFrom<&str> impl on CaixaKind must reject the \
                 non-wire byte-string {input:?} — silent acceptance \
                 signals an accept-set widening off the paired \
                 CaixaKind::from_wire resolver"
            );
        }
    }

    #[test]
    fn caixa_kind_try_from_str_and_from_wire_partition_the_accept_set() {
        // Cross-axis partition pin: the paired `TryFrom<&str>` and
        // `from_wire` reverse projections must resolve identically on
        // *every* input, not just the ones [`CaixaKind::ALL`]
        // enumerates. Sweeps a mixed candidate set spanning accepted
        // (six-arm PascalCase wire byte-strings) and rejected
        // (lowercase diagnostic axis, empty, whitespace-padded,
        // quoted, English rebrand candidates) inputs and asserts the
        // trait's `Result::ok()` projection byte-equals the method-
        // named resolver's `Option<Self>` return-shape on each,
        // locking the two paths together by construction so any future
        // detour (a stray `try_from` special-case that widens or
        // narrows the accept-set outside the paired `from_wire`
        // resolver) trips at caixa-core test time. Pairs with the
        // sibling
        // [`caixa_kind_wire_round_trips_through_from_wire`] pin (which
        // asserts the forward+reverse round-trip on the method-named
        // axis) — this pin extends the round-trip discipline onto the
        // trait-idiomatic reverse axis.
        let candidates: &[&str] = &[
            "Biblioteca",
            "Binario",
            "Servico",
            "Supervisor",
            "Aplicacao",
            "Acao",
            "",
            "biblioteca",
            "servico",
            "unknown",
            "biBlioteca",
            "\"Biblioteca\"",
            " Servico ",
        ];
        for &input in candidates {
            let via_trait: Option<CaixaKind> = <CaixaKind as TryFrom<&str>>::try_from(input).ok();
            let via_method: Option<CaixaKind> = CaixaKind::from_wire(input);
            assert_eq!(
                via_trait, via_method,
                "TryFrom<&str> and from_wire must resolve identically \
                 on input {input:?} — divergence signals the two reverse-\
                 projection paths have drifted onto different accept-sets"
            );
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

    #[test]
    fn caixa_kind_from_into_static_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<CaixaKind> for &'static str` — asserts the standard-
        // library trait impl and the substrate-primitive
        // [`CaixaKind::as_str`] `pub const fn` accessor resolve to the
        // same six-arm emit-set across every arm the exhaustive
        // [`CaixaKind::ALL`] slice enumerates. Any future silent detour
        // that routes the trait impl through a divergent projection (a
        // per-arm inline `match kind { Biblioteca => "biblioteca", … }`
        // re-inlining that opens a compile-time link to the un-lifted
        // arm-literal, an accidental swap onto the sibling `PascalCase`
        // [`CaixaKind::wire_name`] axis that would collide the two-axis
        // human-readable/wire split the sibling
        // [`caixa_kind_display_matches_as_str_and_not_serialize_wire`]
        // pin makes load-bearing) trips at caixa-core test time under
        // `assert_eq!` rather than at a downstream
        // `impl Into<&'static str>`-bound consumer's silent split.
        // Sweeps every one of the six arms [`CaixaKind::ALL`] carries so
        // no arm's projection is covered only by the sibling method-
        // named `as_str` / [`std::fmt::Display`] / [`AsRef<str>`] paths.
        // Materializes the `<&'static str as From<CaixaKind>>::from`
        // output in a `const`-shape binding to make the `'static`
        // lifetime promise a build-time invariant — a future accidental
        // downgrade of any of the six arms'
        // [`crate::render::CAIXA_KIND_LABEL_*`] constants to a
        // non-`&'static str` (a `String::leak()`-produced return, a
        // `Box::leak`-cast) trips at caixa-core build time rather than
        // at a downstream `'static`-bound consumer.
        const BIBLIOTECA: &str = CaixaKind::Biblioteca.as_str();
        const BINARIO: &str = CaixaKind::Binario.as_str();
        const SERVICO: &str = CaixaKind::Servico.as_str();
        const SUPERVISOR: &str = CaixaKind::Supervisor.as_str();
        const APLICACAO: &str = CaixaKind::Aplicacao.as_str();
        const ACAO: &str = CaixaKind::Acao.as_str();
        for &variant in CaixaKind::ALL {
            let via_trait: &'static str = <&'static str as From<CaixaKind>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<CaixaKind> for &'static str impl must round-trip \
                 CaixaKind::{variant:?} to the same lifted \
                 CAIXA_KIND_LABEL_* const CaixaKind::as_str returns — \
                 divergence signals a silent detour off the substrate-\
                 primitive accessor"
            );
            let via_into: &'static str = variant.into();
            assert_eq!(
                via_into, via_method,
                "Into<&'static str>::into on CaixaKind::{variant:?} must \
                 byte-equal CaixaKind::as_str on the same input — the \
                 blanket-derived Into shape must resolve to the same \
                 as_str dispatch as the explicit From impl"
            );
        }
        assert_eq!(
            [BIBLIOTECA, BINARIO, SERVICO, SUPERVISOR, APLICACAO, ACAO],
            [
                crate::render::CAIXA_KIND_LABEL_BIBLIOTECA,
                crate::render::CAIXA_KIND_LABEL_BINARIO,
                crate::render::CAIXA_KIND_LABEL_SERVICO,
                crate::render::CAIXA_KIND_LABEL_SUPERVISOR,
                crate::render::CAIXA_KIND_LABEL_APLICACAO,
                crate::render::CAIXA_KIND_LABEL_ACAO,
            ],
            "const-context CaixaKind::as_str must resolve to the six \
             lifted CAIXA_KIND_LABEL_* consts — a future accidental \
             downgrade of any arm to a non-const or non-static byte-\
             string breaks the `&'static str`-lifetime promise the \
             paired From<CaixaKind> for &'static str impl carries by \
             construction"
        );
    }

    #[test]
    fn caixa_kind_from_into_static_str_and_as_str_partition_the_emit_set() {
        // Cross-axis partition pin: the paired trait-idiomatic
        // `From<CaixaKind> for &'static str` forward projection and the
        // method-named [`CaixaKind::as_str`] forward projection must
        // resolve identically on *every* arm, not just the ones named
        // in the primary byte-parity pin above. Sweeps every
        // [`CaixaKind::ALL`] arm and asserts the trait's `From::from`
        // output byte-equals the method-named accessor's return-value
        // on each, locking the two forward-projection paths together by
        // construction so any future detour (a stray `From` special-
        // case that lands on a divergent per-arm literal outside the
        // paired `as_str` dispatch, a hypothetical rebrand touching one
        // axis without the other) trips at caixa-core test time. Peer
        // of the sibling M2-OTP-shape
        // [`crate::supervisor::tests::restart_strategy_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (523157d) and
        // [`crate::supervisor::tests::restart_policy_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (9fb37d0) partition pins on the sibling M2-OTP-shape closed-
        // set typed-enum discriminator axes — extends the round-trip
        // discipline onto the trait-idiomatic *forward* axis on the
        // structurally most fundamental closed-set typed enum on the
        // caixa surface.
        //
        // Also asserts three-path convergence on the diagnostic axis:
        // `<&'static str as From<CaixaKind>>::from`, `AsRef<str>::as_ref`,
        // and `std::fmt::Display` all resolve to the same lifted
        // [`crate::render::CAIXA_KIND_LABEL_*`] const per arm. The four-
        // path family (with the substrate-primitive
        // [`CaixaKind::as_str`] accessor) is closed by construction so
        // any future silent bifurcation lands at caixa-core test time.
        //
        // The round-trip on the trait-idiomatic axis pair
        // (`From<Self> for &'static str` + `TryFrom<&str> for Self`)
        // crosses through the [`CaixaKind::wire_name`] intermediate
        // vocabulary: the two projection endpoints resolve to
        // different byte-strings by design (the human-readable
        // lowercase Portuguese `"biblioteca"` on the forward axis vs.
        // the `PascalCase` wire `"Biblioteca"` on the reverse axis —
        // the two-axis split the sibling
        // [`caixa_kind_display_matches_as_str_and_not_serialize_wire`]
        // pin makes load-bearing). The round-trip witness threads
        // through [`CaixaKind::wire_name`] as the reverse-axis
        // vocabulary, so a future silent accidental collapse of either
        // axis onto the other (routing `From<Self> for &'static str`
        // through `wire_name`, or routing `TryFrom<&str>` through
        // `as_str`) trips here.
        for &variant in CaixaKind::ALL {
            let via_trait: &'static str = <&'static str as From<CaixaKind>>::from(variant);
            let via_method: &'static str = variant.as_str();
            let via_as_ref: &str = variant.as_ref();
            let via_display: String = variant.to_string();
            assert_eq!(
                via_trait, via_method,
                "From<CaixaKind> for &'static str and CaixaKind::as_str \
                 must resolve identically on CaixaKind::{variant:?} — \
                 divergence signals the two forward-projection paths \
                 have drifted onto different emit-sets"
            );
            assert_eq!(
                via_trait, via_as_ref,
                "From<CaixaKind> for &'static str and AsRef<str>::as_ref \
                 must resolve identically on CaixaKind::{variant:?} — \
                 the four-path forward-projection family (as_str, \
                 AsRef, Display, From<Self> for &'static str) must \
                 land on one lifted CAIXA_KIND_LABEL_* const per arm"
            );
            assert_eq!(
                via_trait,
                via_display.as_str(),
                "From<CaixaKind> for &'static str and Display must \
                 resolve identically on CaixaKind::{variant:?} — the \
                 four-path forward-projection family (as_str, AsRef, \
                 Display, From<Self> for &'static str) must land on one \
                 lifted CAIXA_KIND_LABEL_* const per arm"
            );
        }
        // Round-trip witness: every arm's forward-projected
        // `wire_name` output re-parses through the paired trait-
        // idiomatic reverse `TryFrom<&str>` back to the original
        // variant. The reverse `TryFrom<&str>` axis parses the
        // `PascalCase` wire vocabulary that [`CaixaKind::wire_name`]
        // emits, not the lowercase Portuguese diagnostic vocabulary
        // this commit's `From<Self> for &'static str` axis emits —
        // the two-axis split is by design, and the wire round-trip
        // through `wire_name` + `TryFrom<&str>` closes the vocabulary
        // gap by construction.
        for &variant in CaixaKind::ALL {
            let wire: &'static str = variant.wire_name();
            let re_parsed: Result<CaixaKind, ()> = <CaixaKind as TryFrom<&str>>::try_from(wire);
            assert_eq!(
                re_parsed,
                Ok(variant),
                "trait-idiomatic reverse axis (TryFrom<&str>) must \
                 round-trip CaixaKind::{variant:?} through wire_name \
                 back to Ok(CaixaKind::{variant:?}) — a break signals \
                 the wire-emit and reverse-parse axes have drifted \
                 onto different vocabularies"
            );
        }
        // Explicit two-axis-split witness: the forward
        // `From<Self> for &'static str` axis and the reverse
        // `TryFrom<&str>` axis land on *different* byte-strings by
        // design on every arm whose human-readable label differs from
        // its PascalCase wire form. Assert the two vocabularies stay
        // disjoint per-arm so a future accidental collapse (routing
        // one through the other's accessor) trips at caixa-core test
        // time under the inequality assertion below.
        for &variant in CaixaKind::ALL {
            let forward: &'static str = <&'static str as From<CaixaKind>>::from(variant);
            let wire: &'static str = variant.wire_name();
            assert_ne!(
                forward, wire,
                "forward `From<CaixaKind> for &'static str` axis \
                 (lowercase Portuguese label) and `wire_name` axis \
                 (PascalCase wire) must stay disjoint on \
                 CaixaKind::{variant:?} — a byte-equal collapse \
                 signals the two-axis split the substrate keys off has \
                 silently merged onto one vocabulary"
            );
        }
    }

    #[test]
    fn caixa_kind_from_borrowed_into_static_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<&CaixaKind> for &'static str` — asserts the
        // borrowed-input standard-library trait impl and the substrate-
        // primitive [`CaixaKind::as_str`] `pub const fn` accessor
        // resolve to the same six-arm emit-set across every arm the
        // exhaustive [`CaixaKind::ALL`] slice enumerates. Rust's `From`
        // trait does not auto-derive the borrowed-input sibling from a
        // paired owned-input impl (no `impl<T, U> From<&T> for U where
        // T: Copy, U: From<T>` blanket in `core`), so the borrowed-
        // input axis is a distinct trait-idiomatic surface that a
        // `.iter().map(Into::into)` shape over [`CaixaKind::ALL`]
        // (whose iterator yields `&CaixaKind`, not `CaixaKind`)
        // reaches through this impl and no other — the paired owned-
        // input [`From<CaixaKind>`] impl requires an explicit
        // `.copied()` / dereference before the trait fires.
        // Materializes the `<&'static str as From<&CaixaKind>>::from`
        // output in a `const`-shape binding to make the `'static`
        // lifetime promise a build-time invariant.
        const BIBLIOTECA: &str = CaixaKind::Biblioteca.as_str();
        const BINARIO: &str = CaixaKind::Binario.as_str();
        const SERVICO: &str = CaixaKind::Servico.as_str();
        const SUPERVISOR: &str = CaixaKind::Supervisor.as_str();
        const APLICACAO: &str = CaixaKind::Aplicacao.as_str();
        const ACAO: &str = CaixaKind::Acao.as_str();
        for variant in CaixaKind::ALL {
            let via_trait: &'static str = <&'static str as From<&CaixaKind>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<&CaixaKind> for &'static str impl must round-trip \
                 &CaixaKind::{variant:?} to the same lifted \
                 CAIXA_KIND_LABEL_* const CaixaKind::as_str returns — \
                 divergence signals a silent detour off the substrate-\
                 primitive accessor"
            );
            let via_into: &'static str = variant.into();
            assert_eq!(
                via_into, via_method,
                "Into<&'static str>::into on &CaixaKind::{variant:?} \
                 must byte-equal CaixaKind::as_str on the same input — \
                 the blanket-derived Into shape must resolve to the \
                 same as_str dispatch as the explicit From impl"
            );
        }
        assert_eq!(
            [BIBLIOTECA, BINARIO, SERVICO, SUPERVISOR, APLICACAO, ACAO],
            [
                crate::render::CAIXA_KIND_LABEL_BIBLIOTECA,
                crate::render::CAIXA_KIND_LABEL_BINARIO,
                crate::render::CAIXA_KIND_LABEL_SERVICO,
                crate::render::CAIXA_KIND_LABEL_SUPERVISOR,
                crate::render::CAIXA_KIND_LABEL_APLICACAO,
                crate::render::CAIXA_KIND_LABEL_ACAO,
            ],
            "const-context CaixaKind::as_str must resolve to the six \
             lifted CAIXA_KIND_LABEL_* consts — the borrowed-input \
             From<&CaixaKind> for &'static str impl inherits its \
             `'static` lifetime promise from the same accessor the \
             owned-input sibling routes through"
        );
    }

    #[test]
    fn caixa_kind_from_owned_and_borrowed_into_static_str_agree_on_every_arm() {
        // Cross-axis partition pin: the paired trait-idiomatic
        // owned-input `From<CaixaKind> for &'static str` (edbb27b
        // campaign-shape) and borrowed-input `From<&CaixaKind> for
        // &'static str` (this lift) forward projections must resolve
        // identically on every arm, locking the two input-shape paths
        // together so any future detour trips at caixa-core test time.
        // Then a witness that a `.iter().map(Into::into)` pipe over
        // [`CaixaKind::ALL`] (whose iterator yields `&CaixaKind`)
        // materializes the six-arm accept-set through the borrowed-
        // input axis alone — the exact shape a future M4 admission-
        // webhook rejection body composer, a future substrate-wide
        // per-arm diagnostic column, or a
        // `HashMap::<&'static str, CaixaKind>::from_iter(
        //     CaixaKind::ALL.iter().map(|k| (k.into(), *k)))`-style
        // per-kind lookup reaches through — closing the two-way owned/
        // borrowed input-shape symmetry on the forward-projection
        // trait-idiomatic axis. Peer of the sibling
        // [`crate::dep::tests::dep_list_from_owned_and_borrowed_into_static_str_agree_on_every_arm`]
        // (64aa742) partition pin on the two-list dep-graph closed-set
        // discriminator axis — extends the borrowed-input axis
        // discipline onto the structurally most fundamental closed-set
        // typed enum on the caixa surface.
        for &variant in CaixaKind::ALL {
            let owned: &'static str = <&'static str as From<CaixaKind>>::from(variant);
            let borrowed: &'static str = <&'static str as From<&CaixaKind>>::from(&variant);
            assert_eq!(
                owned, borrowed,
                "From<CaixaKind> and From<&CaixaKind> for &'static str \
                 must resolve identically on CaixaKind::{variant:?} — \
                 divergence signals the owned-input and borrowed-input \
                 forward-projection paths have drifted onto different \
                 emit-sets"
            );
        }
        let via_iter: Vec<&'static str> = CaixaKind::ALL.iter().map(Into::into).collect();
        let via_method: Vec<&'static str> = CaixaKind::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(
            via_iter, via_method,
            "`.iter().map(Into::into)` over CaixaKind::ALL must byte-\
             equal `.iter().map(|k| k.as_str())` on every arm — the \
             borrowed-input `From<&CaixaKind> for &'static str` axis \
             is what makes the `.iter().map(Into::into)` shape route \
             through the substrate-primitive `CaixaKind::as_str` \
             accessor rather than through a per-call-site `.copied()` \
             / dereference detour"
        );
    }

    #[test]
    fn caixa_kind_from_into_owned_string_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<CaixaKind> for String` — asserts the owned-`String`
        // -returning standard-library trait impl and the substrate-
        // primitive [`CaixaKind::as_str`] `pub const fn` accessor
        // resolve to the same six-arm emit-set across every arm the
        // exhaustive [`CaixaKind::ALL`] slice enumerates. Rust's
        // standard library does not carry a blanket
        // `impl<T: AsRef<str>> From<T> for String` (nor an
        // `impl<T: fmt::Display> From<T> for String`), so the
        // owned-`String` forward-projection axis is a distinct trait-
        // idiomatic surface that a `let key: String = kind.into();`-
        // shaped call site reaches through this impl and no other —
        // the paired sibling `From<CaixaKind> for &'static str` impl
        // forces every owned-`String` call site through an explicit
        // `.to_owned()` / `String::from` restatement. Peer of the
        // first-mover
        // [`crate::supervisor::tests::restart_strategy_from_into_owned_string_routes_through_as_str_accessor`]
        // (7baa18a) and the second-peer
        // [`crate::supervisor::tests::restart_policy_from_into_owned_string_routes_through_as_str_accessor`]
        // (7851725) — extends the trait-idiomatic owned-`String`
        // forward-projection axis onto the third (and structurally
        // most fundamental — every caixa carries a `:kind`) closed-
        // set typed enum on the caixa surface.
        for &variant in CaixaKind::ALL {
            let via_trait: String = <String as From<CaixaKind>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait.as_str(),
                via_method,
                "From<CaixaKind> for String impl must round-trip \
                 CaixaKind::{variant:?} to the same lifted \
                 CAIXA_KIND_LABEL_* const CaixaKind::as_str returns — \
                 divergence signals a silent detour off the substrate-\
                 primitive accessor"
            );
            let via_into: String = variant.into();
            assert_eq!(
                via_into.as_str(),
                via_method,
                "Into<String>::into on CaixaKind::{variant:?} must \
                 byte-equal CaixaKind::as_str on the same input — the \
                 blanket-derived Into shape must resolve to the same \
                 as_str dispatch as the explicit From impl"
            );
        }
    }

    #[test]
    fn caixa_kind_from_into_owned_string_and_static_str_agree_on_every_arm() {
        // Cross-axis partition pin: the paired trait-idiomatic
        // owned-`String` `From<CaixaKind> for String` (this lift) and
        // owned-`&'static str` `From<CaixaKind> for &'static str`
        // (edbb27b) forward projections must resolve identically on
        // every arm, locking the two return-type-shape paths together
        // so any future detour trips at caixa-core test time. Also
        // byte-parity witness against the sibling
        // [`ToString::to_string`] surface routed through
        // [`std::fmt::Display`] — the three owned-heap-string paths
        // (`.into::<String>()`, `String::from`, `.to_string()`) must
        // resolve identically on every arm so a future consumer that
        // picks any of the three lands on the same lifted
        // CAIXA_KIND_LABEL_* const. Then a `.iter().copied()
        // .map(String::from)` pipe witness over [`CaixaKind::ALL`]
        // that materializes the six-arm accept-set through the
        // owned-`String` axis alone — the exact shape a future M4
        // admission-webhook rejection body composer or a
        // `HashMap::<String, CaixaKind>::from_iter(
        //     CaixaKind::ALL.iter().copied().map(|k| (k.into(), k)))`-style
        // owned-key per-kind lookup reaches through — closing the
        // owned-`String` forward-projection axis's iterator-pipe
        // shape. Then a round-trip witness through the paired trait-
        // idiomatic reverse [`TryFrom<&str>`] axis on the wire-vocab
        // [`CaixaKind::wire_name`] emission (not the owned-`String`'s
        // [`String::as_str`] borrow, which lands on the lowercase
        // Portuguese diagnostic vocabulary the reverse axis does not
        // accept) — unlike the peer
        // [`crate::supervisor::RestartPolicy`] axis pair (whose
        // forward `From` emit and reverse `TryFrom` parse share one
        // `PascalCase` vocabulary by construction, so the owned-
        // `String` forward + reverse round-trip composes directly),
        // the [`CaixaKind::as_str`] emit and [`CaixaKind::from_wire`]
        // parse land on disjoint vocabularies by design (the two-axis
        // split the sibling
        // [`caixa_kind_display_matches_as_str_and_not_serialize_wire`]
        // pin makes load-bearing), so the round-trip crosses through
        // [`CaixaKind::wire_name`] as the reverse-axis vocabulary
        // rather than composing the owned-`String` emit directly.
        for &variant in CaixaKind::ALL {
            let owned_string: String = <String as From<CaixaKind>>::from(variant);
            let owned_static: &'static str = <&'static str as From<CaixaKind>>::from(variant);
            assert_eq!(
                owned_string.as_str(),
                owned_static,
                "From<CaixaKind> for String and From<CaixaKind> for \
                 &'static str must resolve identically on \
                 CaixaKind::{variant:?} — divergence signals the \
                 owned-`String` and owned-`&'static str` forward-\
                 projection return-type-shape paths have drifted onto \
                 different emit-sets"
            );
            let via_to_string: String = variant.to_string();
            assert_eq!(
                owned_string, via_to_string,
                "From<CaixaKind> for String must byte-equal \
                 CaixaKind::to_string on CaixaKind::{variant:?} — \
                 divergence signals the trait-idiomatic owned-`String` \
                 forward-projection axis and the ToString-through-\
                 Display axis have drifted onto different emit-sets"
            );
        }
        let via_iter: Vec<String> = CaixaKind::ALL.iter().copied().map(String::from).collect();
        let via_method: Vec<String> = CaixaKind::ALL
            .iter()
            .map(|k| k.as_str().to_owned())
            .collect();
        assert_eq!(
            via_iter, via_method,
            "`.iter().copied().map(String::from)` over CaixaKind::ALL \
             must byte-equal `.iter().map(|k| k.as_str().to_owned())` \
             on every arm — the owned-`String` `From<CaixaKind> for \
             String` axis is what makes the `String::from` composition \
             route through the substrate-primitive `CaixaKind::as_str` \
             accessor rather than through a per-call-site `.to_owned()` \
             / `String::from(kind.as_str())` detour"
        );
        for &variant in CaixaKind::ALL {
            let wire: &'static str = variant.wire_name();
            let re_parsed: Result<CaixaKind, ()> = <CaixaKind as TryFrom<&str>>::try_from(wire);
            assert_eq!(
                re_parsed,
                Ok(variant),
                "trait-idiomatic owned-`String` forward-projection + \
                 reverse-projection axis pair must round-trip \
                 CaixaKind::{variant:?} through wire_name and back \
                 through TryFrom<&str> — a break signals the wire-emit \
                 and reverse-parse axes have drifted onto different \
                 vocabularies (the owned-`String` forward emit lands on \
                 the lowercase Portuguese diagnostic vocabulary by \
                 design, so the round-trip crosses through wire_name \
                 rather than composing the owned-`String` emit directly)"
            );
        }
    }

    #[test]
    fn caixa_kind_from_into_borrowed_owned_string_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<&CaixaKind> for String` — asserts the borrowed-
        // input owned-`String`-returning standard-library trait impl
        // and the substrate-primitive [`super::CaixaKind::as_str`]
        // `pub const fn` accessor resolve to the same six-arm emit-set
        // across every arm the exhaustive [`super::CaixaKind::ALL`]
        // slice enumerates. Rust's standard library does not carry a
        // blanket `impl<T: AsRef<str>> From<&T> for String` (nor an
        // `impl<T: fmt::Display> From<&T> for String`), so the
        // borrowed-input owned-`String` forward-projection axis is a
        // distinct trait-idiomatic surface that a
        // `let key: String = (&kind).into();`-shaped call site reaches
        // through this impl and no other — the paired sibling
        // `From<CaixaKind> for String` impl forces every borrowed-
        // input call site through an explicit `Copy` deref
        // (`String::from(*kind)`) or an `.as_str().to_owned()` /
        // `.to_string()` detour. Peer of the first-mover
        // [`crate::supervisor::tests::restart_strategy_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (579385f), the second-peer
        // [`crate::supervisor::tests::restart_policy_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (8465740), and the third-peer
        // [`crate::dep::tests::dep_list_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (e0cb617) — extends the trait-idiomatic borrowed-input
        // owned-`String` forward-projection axis off the two-list
        // dep-graph axis onto the structurally most fundamental
        // closed-set fieldless typed enum peer on the caixa surface
        // (every caixa carries a `:kind`).
        for &variant in CaixaKind::ALL {
            let via_trait: String = <String as From<&CaixaKind>>::from(&variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait.as_str(),
                via_method,
                "From<&CaixaKind> for String impl must round-trip \
                 &CaixaKind::{variant:?} to the same lifted \
                 CAIXA_KIND_LABEL_* const CaixaKind::as_str returns — \
                 divergence signals a silent detour off the substrate-\
                 primitive accessor"
            );
            let via_into: String = (&variant).into();
            assert_eq!(
                via_into.as_str(),
                via_method,
                "Into<String>::into on &CaixaKind::{variant:?} must \
                 byte-equal CaixaKind::as_str on the same input — the \
                 blanket-derived Into shape must resolve to the same \
                 as_str dispatch as the explicit From impl"
            );
        }
    }

    #[test]
    fn caixa_kind_from_into_borrowed_owned_string_agrees_with_paired_axes_on_every_arm() {
        // Cross-axis partition pin: the newly lifted trait-idiomatic
        // borrowed-input owned-`String` `From<&CaixaKind> for String`
        // (this lift), the paired owned-input owned-`String`
        // `From<CaixaKind> for String` (231a18c), the paired
        // borrowed-input owned-`&'static str` `From<&CaixaKind> for
        // &'static str` (5ab993a), and the paired owned-input
        // owned-`&'static str` `From<CaixaKind> for &'static str`
        // (edbb27b) — every corner of the `{Self, &Self} × {&'static
        // str, String}` 2×2 trait-idiomatic projection family — must
        // resolve identically on every arm, locking the four return-
        // shape × input-shape paths together so any future detour
        // trips at caixa-core test time. Also byte-parity witness
        // against the sibling [`ToString::to_string`] surface routed
        // through [`std::fmt::Display`] and a round-trip witness
        // through the paired trait-idiomatic reverse [`TryFrom<&str>`]
        // axis on the wire-vocab [`super::CaixaKind::wire_name`]
        // emission (not the owned-`String`'s [`String::as_str`]
        // borrow, which lands on the lowercase Portuguese diagnostic
        // vocabulary the reverse axis does not accept — the two-axis
        // split by design) that closes the two-way `&Self → String →
        // wire → Self` round-trip on the trait-idiomatic borrowed-
        // input owned-`String` forward + reverse axis pair via the
        // wire vocabulary rather than the emit vocabulary. Peer of
        // the first-mover
        // [`crate::supervisor::tests::restart_strategy_from_into_borrowed_owned_string_agrees_with_paired_axes_on_every_arm`]
        // (579385f), the second-peer
        // [`crate::supervisor::tests::restart_policy_from_into_borrowed_owned_string_agrees_with_paired_axes_on_every_arm`]
        // (8465740), and the third-peer
        // [`crate::dep::tests::dep_list_from_into_borrowed_owned_string_agrees_with_paired_axes_on_every_arm`]
        // (e0cb617) — closes the whole `{Self, &Self} × {&'static
        // str, String}` 2×2 projection corner on the fourth
        // substrate-wide closed-set fieldless typed enum peer (the
        // structurally most fundamental peer on the caixa surface).
        //
        // Unlike the peer M2 OTP-shape and dep-graph axis pairs
        // (whose forward `From` emit and reverse `TryFrom` parse
        // share one vocabulary by construction, so the borrowed-input
        // owned-`String` forward + reverse round-trip composes
        // directly through the owned-`String`'s [`String::as_str`]
        // borrow), the [`super::CaixaKind::as_str`] emit and
        // [`super::CaixaKind::from_wire`] parse land on disjoint
        // vocabularies by design — the two-axis split the sibling
        // [`caixa_kind_display_matches_as_str_and_not_serialize_wire`]
        // pin makes load-bearing — so the round-trip crosses through
        // [`super::CaixaKind::wire_name`] as the reverse-axis
        // vocabulary rather than composing the borrowed-input
        // owned-`String` emit directly.
        for &kind in CaixaKind::ALL {
            let borrowed_string: String = <String as From<&CaixaKind>>::from(&kind);
            let owned_string: String = <String as From<CaixaKind>>::from(kind);
            let borrowed_static: &'static str = <&'static str as From<&CaixaKind>>::from(&kind);
            let owned_static: &'static str = <&'static str as From<CaixaKind>>::from(kind);
            assert_eq!(
                borrowed_string, owned_string,
                "From<&CaixaKind> for String and From<CaixaKind> for \
                 String must resolve identically on \
                 CaixaKind::{kind:?} — divergence signals the \
                 borrowed-input and owned-input owned-`String` \
                 forward-projection input-shape paths have drifted \
                 onto different emit-sets"
            );
            assert_eq!(
                borrowed_string.as_str(),
                borrowed_static,
                "From<&CaixaKind> for String and From<&CaixaKind> for \
                 &'static str must resolve identically on \
                 CaixaKind::{kind:?} — divergence signals the \
                 borrowed-input `&'static str` and owned-`String` \
                 return-shape paths have drifted onto different \
                 emit-sets"
            );
            assert_eq!(
                borrowed_string.as_str(),
                owned_static,
                "From<&CaixaKind> for String and From<CaixaKind> for \
                 &'static str must resolve identically on \
                 CaixaKind::{kind:?} — divergence signals a break in \
                 the diagonal corner of the {{Self, &Self}} × \
                 {{&'static str, String}} 2×2 trait-idiomatic \
                 projection family"
            );
            let via_to_string: String = kind.to_string();
            assert_eq!(
                borrowed_string, via_to_string,
                "From<&CaixaKind> for String must byte-equal \
                 CaixaKind::to_string on CaixaKind::{kind:?} — \
                 divergence signals the trait-idiomatic borrowed-\
                 input owned-`String` forward-projection axis and the \
                 ToString-through-Display axis have drifted onto \
                 different emit-sets"
            );
        }
        let via_iter: Vec<String> = CaixaKind::ALL.iter().map(String::from).collect();
        let via_method: Vec<String> = CaixaKind::ALL
            .iter()
            .map(|k| k.as_str().to_owned())
            .collect();
        assert_eq!(
            via_iter, via_method,
            "`.iter().map(String::from)` over CaixaKind::ALL — a call \
             site whose iteration axis holds `&CaixaKind` by \
             construction — must byte-equal `.iter().map(|k| \
             k.as_str().to_owned())` on every arm — the borrowed-\
             input owned-`String` `From<&CaixaKind> for String` axis \
             is what makes the `String::from` composition route \
             through the substrate-primitive `CaixaKind::as_str` \
             accessor without a spurious `Copy` deref (which would \
             only be reachable through the owned-input `From<\
             CaixaKind> for String` axis by first calling `.copied()` \
             on the iterator)"
        );
        for &variant in CaixaKind::ALL {
            let wire: &'static str = variant.wire_name();
            let re_parsed: Result<CaixaKind, ()> = <CaixaKind as TryFrom<&str>>::try_from(wire);
            assert_eq!(
                re_parsed,
                Ok(variant),
                "trait-idiomatic borrowed-input owned-`String` \
                 forward-projection + reverse-projection axis pair \
                 must round-trip CaixaKind::{variant:?} through \
                 wire_name and back through TryFrom<&str> — a break \
                 signals the wire-emit and reverse-parse axes have \
                 drifted onto different vocabularies (the borrowed-\
                 input owned-`String` forward emit lands on the \
                 lowercase Portuguese diagnostic vocabulary by \
                 design, so the round-trip crosses through wire_name \
                 rather than composing the borrowed-input \
                 owned-`String` emit directly)"
            );
        }
    }

    #[test]
    fn caixa_kind_from_into_static_cow_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<CaixaKind> for std::borrow::Cow<'static, str>` —
        // asserts the standard-library trait impl and the substrate-
        // primitive [`super::CaixaKind::as_str`] `pub const fn`
        // accessor resolve to the same six-arm emit-set across every
        // arm the exhaustive [`super::CaixaKind::ALL`] slice
        // enumerates. Rust's standard library does not carry a
        // blanket `impl<T: AsRef<str>> From<T> for Cow<'static, str>`
        // (nor an `impl<T: fmt::Display> From<T> for
        // Cow<'static, str>`), so the `Cow<'static, str>` forward-
        // projection axis is a distinct trait-idiomatic surface that a
        // `let key: Cow<'static, str> = kind.into();`-shaped call site
        // reaches through this impl and no other — the paired sibling
        // `From<CaixaKind> for &'static str` and `From<CaixaKind> for
        // String` impls force every `Cow<'static, str>`-parameterized
        // call site through a `Cow::Borrowed(kind.as_str())` /
        // `Cow::Owned(kind.to_string())` composition whose type bounds
        // have no compile-time link back to the substrate primitive.
        //
        // Also asserts the projection lands on the zero-alloc
        // [`std::borrow::Cow::Borrowed`] arm (not the
        // [`std::borrow::Cow::Owned`] arm) — the substrate-primitive
        // [`super::CaixaKind::as_str`] accessor's `&'static str`
        // return lifetime by construction makes the borrowed arm the
        // type-correct projection with no runtime allocation. Any
        // future silent detour that routes the impl through the
        // owned arm (an accidental `Cow::Owned(kind.to_string())`
        // rewrite that would allocate on every call site where the
        // `&'static str` return of [`super::CaixaKind::as_str`] makes
        // the zero-alloc borrowed projection type-correct) trips at
        // caixa-core test time under the [`std::borrow::Cow::Borrowed`]
        // discriminator witness rather than at a downstream
        // `Cow<'static, str>`-bound consumer's silent allocation.
        //
        // First-mover on the substrate-wide trait-idiomatic
        // [`std::borrow::Cow<'static, str>`] forward-projection family
        // — extends the substrate discipline off the closed
        // `{Self, &Self} × {&'static str, String}` 2×2 forward-
        // projection corner onto the [`Cow<'static, str>`] axis on the
        // structurally most fundamental closed-set fieldless typed
        // enum peer on the caixa surface (every caixa carries a
        // `:kind`). Every future closed-set fieldless typed enum peer
        // on the substrate is a future target of the campaign.
        for &variant in CaixaKind::ALL {
            let via_trait: std::borrow::Cow<'static, str> =
                <std::borrow::Cow<'static, str> as From<CaixaKind>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait.as_ref(),
                via_method,
                "From<CaixaKind> for Cow<'static, str> impl must \
                 round-trip CaixaKind::{variant:?} to the same lifted \
                 CAIXA_KIND_LABEL_* const CaixaKind::as_str returns — \
                 divergence signals a silent detour off the \
                 substrate-primitive accessor"
            );
            assert!(
                matches!(via_trait, std::borrow::Cow::Borrowed(_)),
                "From<CaixaKind> for Cow<'static, str> impl must land \
                 on the zero-alloc Cow::Borrowed arm on \
                 CaixaKind::{variant:?} — a Cow::Owned outcome \
                 signals the projection has silently allocated where \
                 the substrate-primitive CaixaKind::as_str \
                 `&'static str` return makes the borrowed arm the \
                 type-correct projection"
            );
            let via_into: std::borrow::Cow<'static, str> = variant.into();
            assert_eq!(
                via_into.as_ref(),
                via_method,
                "Into<Cow<'static, str>>::into on \
                 CaixaKind::{variant:?} must byte-equal \
                 CaixaKind::as_str on the same input — the blanket-\
                 derived Into shape must resolve to the same as_str \
                 dispatch as the explicit From impl"
            );
            assert!(
                matches!(via_into, std::borrow::Cow::Borrowed(_)),
                "Into<Cow<'static, str>>::into on \
                 CaixaKind::{variant:?} must land on the zero-alloc \
                 Cow::Borrowed arm — the blanket-derived Into shape \
                 must resolve to the same Cow::Borrowed dispatch as \
                 the explicit From impl"
            );
        }
    }

    #[test]
    fn caixa_kind_from_into_static_cow_str_agrees_with_paired_axes_on_every_arm() {
        // Cross-axis partition pin: the newly lifted trait-idiomatic
        // `From<CaixaKind> for std::borrow::Cow<'static, str>` (this
        // lift), the paired owned-input `From<CaixaKind> for &'static
        // str` (edbb27b), and the paired owned-input
        // `From<CaixaKind> for String` (231a18c) forward projections
        // must resolve identically on every arm, locking the three
        // return-shape paths together by construction so any future
        // detour trips at caixa-core test time. Also byte-parity
        // witness against the sibling [`ToString::to_string`] surface
        // routed through [`std::fmt::Display`] — every owned-heap-
        // string path (the `Cow::Owned` promotion of this axis's
        // `.into_owned()`, `From<CaixaKind> for String`, and
        // `.to_string()`) resolves to the same lifted
        // [`crate::render::CAIXA_KIND_LABEL_*`] const per arm.
        //
        // Then a `.iter().copied().map(std::borrow::Cow::from)` pipe
        // witness over [`super::CaixaKind::ALL`] that materializes the
        // six-arm accept-set through the [`std::borrow::Cow<'static,
        // str>`] axis alone — the exact shape a future
        // `axum::response::IntoResponse` per-arm rejection-body
        // composer, a future M4 admission-webhook per-arm rejection-
        // reason emitter whose typing rules out the sibling
        // [`AsRef<str>`] borrowed return, or a future substrate-wide
        // per-arm diagnostic surface that binds through a
        // [`Cow<'static, str>`] boundary reaches through — closing
        // the composable-projection axis on the structurally most
        // fundamental closed-set fieldless typed enum peer on the
        // caixa surface. The pipe witness also pins the zero-alloc
        // discipline: every element in the collected vector satisfies
        // the [`std::borrow::Cow::Borrowed`] arm predicate, so a
        // future accidental silent-allocation regression on the
        // pipe's iteration axis is a caixa-core-test-time failure.
        for &variant in CaixaKind::ALL {
            let via_cow: std::borrow::Cow<'static, str> =
                <std::borrow::Cow<'static, str> as From<CaixaKind>>::from(variant);
            let via_static: &'static str = <&'static str as From<CaixaKind>>::from(variant);
            let via_string: String = <String as From<CaixaKind>>::from(variant);
            assert_eq!(
                via_cow.as_ref(),
                via_static,
                "From<CaixaKind> for Cow<'static, str> and \
                 From<CaixaKind> for &'static str must resolve \
                 identically on CaixaKind::{variant:?} — divergence \
                 signals the Cow<'static, str> and &'static str \
                 return-shape paths have drifted onto different \
                 emit-sets"
            );
            assert_eq!(
                via_cow.as_ref(),
                via_string.as_str(),
                "From<CaixaKind> for Cow<'static, str> and \
                 From<CaixaKind> for String must resolve identically \
                 on CaixaKind::{variant:?} — divergence signals the \
                 Cow<'static, str> and String return-shape paths \
                 have drifted onto different emit-sets"
            );
            let via_to_string: String = variant.to_string();
            assert_eq!(
                via_cow.as_ref(),
                via_to_string.as_str(),
                "From<CaixaKind> for Cow<'static, str> must byte-\
                 equal CaixaKind::to_string on CaixaKind::{variant:?} \
                 — divergence signals the trait-idiomatic \
                 Cow<'static, str> forward-projection axis and the \
                 ToString-through-Display axis have drifted onto \
                 different emit-sets"
            );
        }
        let via_iter: Vec<std::borrow::Cow<'static, str>> = CaixaKind::ALL
            .iter()
            .copied()
            .map(std::borrow::Cow::from)
            .collect();
        let via_method: Vec<std::borrow::Cow<'static, str>> = CaixaKind::ALL
            .iter()
            .map(|k| std::borrow::Cow::Borrowed(k.as_str()))
            .collect();
        assert_eq!(
            via_iter, via_method,
            "`.iter().copied().map(Cow::from)` over CaixaKind::ALL \
             must byte-equal `.iter().map(|k| \
             Cow::Borrowed(k.as_str()))` on every arm — the trait-\
             idiomatic `From<CaixaKind> for Cow<'static, str>` axis \
             is what makes the `Cow::from` composition route through \
             the substrate-primitive `CaixaKind::as_str` accessor \
             with the zero-alloc Cow::Borrowed arm by construction, \
             rather than a per-call-site \
             `Cow::Owned(kind.to_string())` allocation"
        );
        for cow in &via_iter {
            assert!(
                matches!(cow, std::borrow::Cow::Borrowed(_)),
                "every element of the .iter().copied().map(Cow::from) \
                 pipe over CaixaKind::ALL must land on the zero-alloc \
                 Cow::Borrowed arm — a Cow::Owned outcome on any arm \
                 signals the pipe's iteration axis has silently \
                 allocated where the substrate-primitive \
                 CaixaKind::as_str `&'static str` return makes the \
                 borrowed arm the type-correct projection"
            );
        }
    }

    #[test]
    fn caixa_kind_from_borrowed_into_static_cow_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<&CaixaKind> for std::borrow::Cow<'static, str>` —
        // asserts the borrowed-input standard-library trait impl and
        // the substrate-primitive [`super::CaixaKind::as_str`] `pub
        // const fn` accessor resolve to the same six-arm emit-set
        // across every arm the exhaustive [`super::CaixaKind::ALL`]
        // slice enumerates. Rust's standard library does not carry a
        // blanket `impl<T: AsRef<str>> From<&T> for Cow<'static, str>`
        // (nor a `Copy`-based `impl<T: Copy, U: From<T>> From<&T> for
        // U`), so the borrowed-input `Cow<'static, str>` forward-
        // projection axis is a distinct trait-idiomatic surface that a
        // `let key: Cow<'static, str> = (&kind).into();`-shaped call
        // site or a `CaixaKind::ALL.iter().map(Cow::from)`-shaped pipe
        // reaches through this impl and no other — the paired owned-
        // input `From<CaixaKind> for Cow<'static, str>` impl forces
        // every borrowed-input call site through an explicit `Copy`
        // deref (`Cow::from(*kind)`) or a `Cow::Borrowed(kind.as_str())`
        // open-code whose type bounds have no compile-time link back
        // to the substrate primitive.
        //
        // Also asserts the projection lands on the zero-alloc
        // [`std::borrow::Cow::Borrowed`] arm (not the
        // [`std::borrow::Cow::Owned`] arm) — the substrate-primitive
        // [`super::CaixaKind::as_str`] accessor's `&'static str`
        // return lifetime by construction makes the borrowed arm the
        // type-correct projection with no runtime allocation on the
        // borrowed-input surface just as on the paired owned-input
        // surface.
        //
        // Second peer on the substrate-wide trait-idiomatic
        // [`std::borrow::Cow<'static, str>`] forward-projection family
        // opened on the paired owned-input `From<CaixaKind> for
        // Cow<'static, str>` (99c1735) — closes the `{Self, &Self}`
        // input-shape corner of the [`Cow<'static, str>`] axis on the
        // structurally most fundamental closed-set fieldless typed
        // enum peer on the caixa surface (every caixa carries a
        // `:kind`). Every future closed-set fieldless typed enum peer
        // on the substrate is a future target of the campaign.
        for &variant in CaixaKind::ALL {
            let via_trait: std::borrow::Cow<'static, str> =
                <std::borrow::Cow<'static, str> as From<&CaixaKind>>::from(&variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait.as_ref(),
                via_method,
                "From<&CaixaKind> for Cow<'static, str> impl must \
                 round-trip &CaixaKind::{variant:?} to the same lifted \
                 CAIXA_KIND_LABEL_* const CaixaKind::as_str returns — \
                 divergence signals a silent detour off the \
                 substrate-primitive accessor"
            );
            assert!(
                matches!(via_trait, std::borrow::Cow::Borrowed(_)),
                "From<&CaixaKind> for Cow<'static, str> impl must land \
                 on the zero-alloc Cow::Borrowed arm on \
                 CaixaKind::{variant:?} — a Cow::Owned outcome \
                 signals the projection has silently allocated where \
                 the substrate-primitive CaixaKind::as_str \
                 `&'static str` return makes the borrowed arm the \
                 type-correct projection"
            );
            let via_into: std::borrow::Cow<'static, str> = (&variant).into();
            assert_eq!(
                via_into.as_ref(),
                via_method,
                "Into<Cow<'static, str>>::into on &CaixaKind::{variant:?} \
                 must byte-equal CaixaKind::as_str on the same input \
                 — the blanket-derived Into shape must resolve to the \
                 same as_str dispatch as the explicit From impl"
            );
            assert!(
                matches!(via_into, std::borrow::Cow::Borrowed(_)),
                "Into<Cow<'static, str>>::into on &CaixaKind::{variant:?} \
                 must land on the zero-alloc Cow::Borrowed arm — the \
                 blanket-derived Into shape must resolve to the same \
                 Cow::Borrowed dispatch as the explicit From impl"
            );
        }
    }

    #[test]
    fn caixa_kind_from_borrowed_into_static_cow_str_agrees_with_paired_axes_on_every_arm() {
        // Cross-axis partition pin: the newly lifted trait-idiomatic
        // borrowed-input `From<&CaixaKind> for std::borrow::Cow<'static, str>`
        // (this lift), the paired owned-input `From<CaixaKind> for
        // std::borrow::Cow<'static, str>` (99c1735), the paired
        // borrowed-input owned-`&'static str` `From<&CaixaKind> for
        // &'static str` (5ab993a), and the paired borrowed-input
        // owned-`String` `From<&CaixaKind> for String` must resolve
        // identically on every arm, locking the four
        // return-shape × input-shape paths together by construction so
        // any future detour trips at caixa-core test time. Also byte-
        // parity witness against the sibling [`ToString::to_string`]
        // surface routed through [`std::fmt::Display`] — every owned-
        // heap-string path (this axis's `.into_owned()` promotion, the
        // paired [`From<&CaixaKind> for String`], and `.to_string()`)
        // resolves to the same lifted
        // [`crate::render::CAIXA_KIND_LABEL_*`] const per arm.
        //
        // Then a `.iter().map(std::borrow::Cow::from)` pipe witness
        // over [`super::CaixaKind::ALL`] — whose iterator yields
        // `&CaixaKind` by construction, so the borrowed-input
        // [`Cow<'static, str>`] axis is what routes the pipe through
        // the substrate-primitive [`super::CaixaKind::as_str`]
        // accessor without a spurious [`Copy`] deref (which would only
        // be reachable through the owned-input
        // [`From<CaixaKind> for Cow<'static, str>`] axis by first
        // calling `.copied()` on the iterator). The pipe witness also
        // pins the zero-alloc discipline: every element in the
        // collected vector satisfies the [`std::borrow::Cow::Borrowed`]
        // arm predicate, so a future accidental silent-allocation
        // regression on the pipe's iteration axis is a caixa-core-
        // test-time failure.
        for &kind in CaixaKind::ALL {
            let borrowed_cow: std::borrow::Cow<'static, str> =
                <std::borrow::Cow<'static, str> as From<&CaixaKind>>::from(&kind);
            let owned_cow: std::borrow::Cow<'static, str> =
                <std::borrow::Cow<'static, str> as From<CaixaKind>>::from(kind);
            let borrowed_static: &'static str = <&'static str as From<&CaixaKind>>::from(&kind);
            let borrowed_string: String = <String as From<&CaixaKind>>::from(&kind);
            assert_eq!(
                borrowed_cow, owned_cow,
                "From<&CaixaKind> for Cow<'static, str> and \
                 From<CaixaKind> for Cow<'static, str> must resolve \
                 identically on CaixaKind::{kind:?} — divergence \
                 signals the borrowed-input and owned-input \
                 Cow<'static, str> forward-projection input-shape \
                 paths have drifted onto different emit-sets"
            );
            assert_eq!(
                borrowed_cow.as_ref(),
                borrowed_static,
                "From<&CaixaKind> for Cow<'static, str> and \
                 From<&CaixaKind> for &'static str must resolve \
                 identically on CaixaKind::{kind:?} — divergence \
                 signals the borrowed-input Cow<'static, str> and \
                 &'static str return-shape paths have drifted onto \
                 different emit-sets"
            );
            assert_eq!(
                borrowed_cow.as_ref(),
                borrowed_string.as_str(),
                "From<&CaixaKind> for Cow<'static, str> and \
                 From<&CaixaKind> for String must resolve identically \
                 on CaixaKind::{kind:?} — divergence signals the \
                 borrowed-input Cow<'static, str> and owned-`String` \
                 return-shape paths have drifted onto different \
                 emit-sets"
            );
            let via_to_string: String = kind.to_string();
            assert_eq!(
                borrowed_cow.as_ref(),
                via_to_string.as_str(),
                "From<&CaixaKind> for Cow<'static, str> must byte-\
                 equal CaixaKind::to_string on CaixaKind::{kind:?} — \
                 divergence signals the trait-idiomatic borrowed-\
                 input Cow<'static, str> forward-projection axis and \
                 the ToString-through-Display axis have drifted onto \
                 different emit-sets"
            );
        }
        let via_iter: Vec<std::borrow::Cow<'static, str>> =
            CaixaKind::ALL.iter().map(std::borrow::Cow::from).collect();
        let via_method: Vec<std::borrow::Cow<'static, str>> = CaixaKind::ALL
            .iter()
            .map(|k| std::borrow::Cow::Borrowed(k.as_str()))
            .collect();
        assert_eq!(
            via_iter, via_method,
            "`.iter().map(Cow::from)` over CaixaKind::ALL — a call \
             site whose iteration axis holds `&CaixaKind` by \
             construction — must byte-equal `.iter().map(|k| \
             Cow::Borrowed(k.as_str()))` on every arm — the \
             borrowed-input Cow<'static, str> `From<&CaixaKind> for \
             Cow<'static, str>` axis is what makes the `Cow::from` \
             composition route through the substrate-primitive \
             `CaixaKind::as_str` accessor with the zero-alloc \
             Cow::Borrowed arm by construction and without a spurious \
             `Copy` deref (which would only be reachable through the \
             owned-input `From<CaixaKind> for Cow<'static, str>` axis \
             by first calling `.copied()` on the iterator)"
        );
        for cow in &via_iter {
            assert!(
                matches!(cow, std::borrow::Cow::Borrowed(_)),
                "every element of the .iter().map(Cow::from) pipe \
                 over CaixaKind::ALL must land on the zero-alloc \
                 Cow::Borrowed arm — a Cow::Owned outcome on any arm \
                 signals the pipe's iteration axis has silently \
                 allocated where the substrate-primitive \
                 CaixaKind::as_str `&'static str` return makes the \
                 borrowed arm the type-correct projection"
            );
        }
    }
}
