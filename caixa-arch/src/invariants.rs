//! Built-in invariants — the ones a reasonable infra caixa should never break.
//!
//! Each invariant is a pure `fn(&TeiaManifest) -> Vec<Violation>`.  Custom
//! policies are phase-2 work (iac-forge's `policy::Policy` is the extension
//! point — we don't depend on its full tree here to stay light).

use caixa_teia::{TeiaInstance, TeiaManifest, TeiaValue};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, gen_platform::IsVariant,
)]
pub enum InvariantKind {
    /// A hard safety property — refuse to emit HCL when violated.
    Safety,
    /// A compliance property — report, don't block by default.
    Compliance,
    /// A best-practice hint — never blocks.
    Hint,
}

impl InvariantKind {
    /// Exhaustive iteration surface for every consumer that walks the
    /// closed three-arm [`InvariantKind`] discriminator set — the
    /// `caixa-arch` [`crate::run::check_manifest`] verdict-summary
    /// per-arm-count aggregator (which reads every arm's population
    /// out of one `.violations` sweep), a future
    /// `feira arch --list-severities` CLI enumeration, a future
    /// admission-webhook rejection body naming the accepted-severity
    /// set. A future variant addition (a fourth severity axis the
    /// `iac-forge` policy-engine grows — a `Warning` tier between
    /// [`Self::Compliance`] and [`Self::Hint`], a `Fatal` tier above
    /// [`Self::Safety`]) extends this slice as one edit and every
    /// consumer picks up the new entry by construction; the compiler-
    /// checked exhaustiveness on the sibling [`gen_platform::IsVariant`]-
    /// derive-generated `is_*` predicates is the build-time guarantee
    /// that no arm forgets to grow.
    ///
    /// Peer of the sibling closed-set typed enums'
    /// [`caixa_core::CaixaKind::ALL`] (6b1f4fb) /
    /// [`caixa_core::CaixaDialeto::ALL`] (dd4f541) /
    /// [`caixa_core::aplicacao::PlacementStrategy::ALL`] (18c7342) /
    /// [`caixa_core::aplicacao::RateLimitUnit::ALL`] (6bce03d) /
    /// [`caixa_core::supervisor::RestartStrategy::ALL`] (4eec29c) /
    /// [`caixa_core::supervisor::RestartPolicy::ALL`] (dd32ccf) /
    /// [`caixa_core::dep::DepList::ALL`] (45ee563) exhaustive-iteration
    /// surfaces — the eighth closed-set fieldless typed enum on the
    /// caixa surface (and the first outside `caixa-core`) to converge
    /// onto the same one-canonical-arm-list-per-enum discipline. Order
    /// matches variant declaration order verbatim
    /// (`Safety` → `Compliance` → `Hint`) so the slice is the
    /// canonical severity ordering every listing / rendering consumer
    /// defers to.
    pub const ALL: &'static [Self] = &[Self::Safety, Self::Compliance, Self::Hint];

    /// Substrate-canonical per-[`InvariantKind`] lowercase-tag scalar
    /// accessor every consumer that renders the arch severity axis as
    /// user-facing text keys off — returns the per-arm byte-string
    /// (`"safety"` / `"compliance"` / `"hint"`) as a `&'static str`, the
    /// same three tags the pre-lift `format!("{:?}", v.kind).to_lowercase()`
    /// consumer at `caixa-feira/src/cmd/tofu.rs`'s per-violation render
    /// site produced by round-tripping through the derived
    /// [`std::fmt::Debug`] output — with two silent drift footguns the
    /// substrate-canonical accessor closes at build time:
    ///
    ///   - the `Debug` derive's per-arm output is *not* a stability
    ///     guarantee (Rust's own convention gives it as *no guarantee at
    ///     all*), so a `#[derive(Debug)]` swap for a hand-rolled
    ///     `impl Debug` that pretty-prints the arm with per-arm context
    ///     (`"Safety(hard)"`, `"Hint(recommend)"`) would silently reroute
    ///     the diagnostic tag through a stale byte-string with no
    ///     downstream signal until an operator scrolled the `feira tofu`
    ///     terminal output;
    ///   - `format!("{:?}", v.kind).to_lowercase()` allocates a fresh
    ///     `String` per violation on every render pass — a per-arm
    ///     `&'static str` return eliminates the allocation at every
    ///     substrate-side per-`InvariantKind` render consumer.
    ///
    /// Peer of the sibling substrate-wide closed-set fieldless typed-enum
    /// canonical-lowercase-tag scalar accessors [`caixa_lint::Severity::as_str`]
    /// (per the caixa-lint diagnostic module's four-arm severity-classification
    /// axis returning `"error"` / `"warning"` / `"info"` / `"hint"`),
    /// [`caixa_core::CaixaKind::as_str`] (per the caixa-core top-level `:kind`
    /// axis returning `"biblioteca"` / `"binario"` / …), and the M2/M3
    /// [`caixa_core::supervisor::RestartStrategy::as_str`] / [`caixa_core::supervisor::RestartPolicy::as_str`]
    /// / [`caixa_core::aplicacao::PlacementStrategy::as_str`] siblings —
    /// extends the substrate-wide "one canonical lowercase-tag accessor
    /// per closed-set fieldless typed enum" discipline onto the caixa-arch
    /// invariant-severity axis, the first outside-caixa-core / outside-
    /// caixa-lint closed-set enum on the caixa surface to reach the axis.
    ///
    /// `pub const fn` — matches the sibling
    /// [`gen_platform::IsVariant`]-derive-generated per-arm `is_*`
    /// predicates' `const fn` posture, so every future substrate-side
    /// `const`-context consumer (a `const _: () = assert!(…)` module-scope
    /// pin on a per-fixture typed [`InvariantKind`], a future M4 admission-
    /// webhook `const fn` per-severity rejection-body composer, a
    /// compile-time `HashMap<&'static str, _>`-shaped per-severity policy
    /// table) reaches the paired byte-string through one substrate-primitive
    /// dispatch at compile time as at runtime.
    ///
    /// A future variant addition (a `Warning` tier between
    /// [`Self::Compliance`] and [`Self::Hint`] the `iac-forge` policy-
    /// engine grows, a `Fatal` tier above [`Self::Safety`]) reaches the
    /// paired [`std::fmt::Display`] impl + [`AsRef<str>`] impl + every
    /// downstream `.as_str()` consumer through one match-arm edit here,
    /// not a coordinated rewrite of every open-coded `format!("{:?}", …)`
    /// re-inlining. Named `as_str` (not `label` / `tag`) to match the
    /// sibling closed-set-enum `as_str` convention the substrate already
    /// carries verbatim across every peer typed enum.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Safety => "safety",
            Self::Compliance => "compliance",
            Self::Hint => "hint",
        }
    }

    /// Substrate-canonical reverse projection on the [`InvariantKind`]
    /// closed-set arch-severity axis — parses the lowercase per-arm
    /// byte-string [`Self::as_str`] emits back to the typed variant, or
    /// `None` when `s` is outside the closed three-arm accept-set
    /// (`"safety"` / `"compliance"` / `"hint"`). Walks exactly the same
    /// three byte-strings the sibling [`Self::as_str`] forward emitter
    /// returns, so the parse and emit halves of the round-trip migrate
    /// through one caixa-arch edit on any future arm addition (a
    /// `Warning` tier between [`Self::Compliance`] and [`Self::Hint`] the
    /// `iac-forge` policy-engine grows, a `Fatal` tier above
    /// [`Self::Safety`]): the compiler-checked exhaustiveness on
    /// [`Self::as_str`]'s `match self` arms and the round-trip pin
    /// [`tests::invariant_kind_from_wire_accepts_every_as_str_output`]
    /// together lock the two halves mutually.
    ///
    /// Prior to this lift the substrate carried only the forward
    /// `Self → &str` projection on the arch-severity axis (the
    /// [`Self::as_str`] emitter, the paired [`std::fmt::Display`] impl
    /// routed through it, the paired [`AsRef<str>`] impl routed through
    /// it) — every future consumer that wanted to promote the arch-
    /// verdict tag back to the typed enum (a future `feira arch
    /// --severity <safety|compliance|hint>` CLI arg-parse that binds
    /// the wire byte-string into the typed enum before dispatching to
    /// the per-arm filter, a future M4 `mesh.pleme.io/v1alpha1/ArchAudit`
    /// CR materializer's admission-time re-parse of the per-violation
    /// severity axis, an `iac-forge` policy-engine audit-report re-loader
    /// that binds a prior [`Self::as_str`] output back to the typed enum
    /// for cross-run severity-histogram diff) would have had to re-inline
    /// a three-arm `match s` cascade that expressed no compile-time link
    /// back to the typed [`InvariantKind`] enum.
    ///
    /// Same closed-set-reverse-projection discipline the sibling
    /// [`caixa_core::CaixaKind::from_wire`] (2aa6d23) /
    /// [`caixa_core::CaixaDialeto::from_wire`] (d0e65ea) /
    /// [`caixa_core::supervisor::RestartStrategy::from_wire`] (4eec29c) /
    /// [`caixa_core::supervisor::RestartPolicy::from_wire`] (dd32ccf) /
    /// [`caixa_core::aplicacao::PlacementStrategy::from_wire`] (18c7342) /
    /// [`caixa_core::dep::DepList::from_wire`] (45ee563) typed enums
    /// carry on the peer wire-side `str → Self` axes — extends the
    /// substrate-wide `(as_str, from_wire)` round-trip family onto the
    /// first outside-caixa-core closed-set fieldless typed enum on the
    /// caixa surface (the caixa-arch invariant-severity axis), matching
    /// the same two-way `str ↔ Self` round-trip every sibling
    /// closed-set enum already carries. Method-named `from_wire` (not
    /// `from_str`) to match the peer shapes verbatim and side-step a
    /// `clippy::should_implement_trait` lint that a plain `from_str`
    /// name would otherwise trigger without paired
    /// [`std::str::FromStr`] impl scaffolding this axis does not carry
    /// today. Returns `Option<Self>` (rather than `Result<Self, _>`) to
    /// match the peer shapes: the caller picks the diagnostic form
    /// appropriate for its use site (a `feira arch --severity` CLI
    /// arg-parse renders its own per-verb error message; an admission-
    /// webhook rejection body wraps the `None` outcome with the
    /// accepted-set enumeration `InvariantKind::ALL.iter().map(…)` for
    /// operator diagnostics).
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "safety" => Some(Self::Safety),
            "compliance" => Some(Self::Compliance),
            "hint" => Some(Self::Hint),
            _ => None,
        }
    }
}

/// Route the derived-style [`std::fmt::Display`] impl on [`InvariantKind`]
/// through the substrate-canonical [`InvariantKind::as_str`] `pub const fn`
/// accessor so every consumer that binds an [`InvariantKind`] through
/// the standard-library `{}` formatting axis (a future `feira arch`
/// per-severity summary line, a `tracing::field::Value::from(kind)`
/// structured-log recorder on the operator's per-violation emission path,
/// any `format!("{kind}")` interpolation in a future audit surface)
/// reaches the canonical byte-string through one substrate-primitive
/// dispatch rather than an open-coded per-arm match at every wire-up.
///
/// Follows the same closed-set-typed-enum `Display`-through-`as_str`
/// convention the substrate-wide siblings [`caixa_core::CaixaKind`],
/// [`caixa_core::aplicacao::PlacementStrategy`],
/// [`caixa_core::supervisor::RestartStrategy`],
/// [`caixa_core::supervisor::RestartPolicy`], and [`caixa_core::dep::DepList`]
/// already carry — closes the [`InvariantKind`] closed-set enum's
/// `(as_str, Display, AsRef<str>)` canonical-projection triple.
impl std::fmt::Display for InvariantKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Route the standard-library [`AsRef<str>`] projection on [`InvariantKind`]
/// through the substrate-canonical [`InvariantKind::as_str`] `pub const fn`
/// accessor so every consumer that binds an [`InvariantKind`] through
/// the trait-idiomatic `.as_ref()` (a future `HashMap::get::<str>(kind.as_ref())`
/// per-severity policy-table lookup, a `Command::arg` shell-out composing
/// the canonical severity tag into a `feira arch --severity=<tag>` filter,
/// any `impl AsRef<str>`-bound generic function) reaches the canonical
/// byte-string through one substrate-primitive dispatch rather than an
/// open-coded `.as_str()` re-inlining at every wire-up.
///
/// Peer of the substrate-wide sibling closed-set-enum
/// `AsRef<str>`-through-`as_str` family already carried by
/// [`caixa_core::CaixaKind`], [`caixa_core::CaixaDialeto`],
/// [`caixa_core::aplicacao::PlacementStrategy`],
/// [`caixa_core::aplicacao::RateLimitUnit`],
/// [`caixa_core::supervisor::RestartStrategy`],
/// [`caixa_core::supervisor::RestartPolicy`],
/// [`caixa_core::dep::DepList`], [`caixa_core::CaixaVersion`], and
/// [`caixa_lint::Severity`] — extends the axis onto the caixa-arch
/// invariant-severity closed-set enum, closing the
/// `(as_str, Display, AsRef<str>)` canonical-projection triple.
impl AsRef<str> for InvariantKind {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Trait-idiomatic reverse projection on the [`InvariantKind`]
/// closed-set caixa-arch invariant-severity axis — routes byte-for-byte
/// through the paired substrate-primitive [`InvariantKind::from_wire`]
/// `Option<Self>` accessor so every future consumer that binds a
/// canonical arch-severity tag through the standard-library
/// `.try_into()` / [`TryFrom`] axis (a future
/// `feira arch --severity=<safety|compliance|hint>` CLI arg-parse that
/// composes into `let axis: InvariantKind = s.try_into()?`, a future M4
/// `mesh.pleme.io/v1alpha1/ArchAudit` CR admission-webhook rejection-body
/// parser that folds a prior audit's `spec.severity: String` through
/// `InvariantKind::try_from(&s)?`, a generic `<T: TryFrom<&str>>`-bound
/// audit-report re-loader over any of the substrate's closed-set typed
/// enums) reaches the same three-arm accept-set the sibling
/// [`InvariantKind::from_wire`] resolver parses through and the sibling
/// [`InvariantKind::as_str`] emits, rather than an open-coded per-arm
/// `match s { "safety" => …, "compliance" => …, "hint" => …, _ => … }`
/// cascade whose arm-set has no compile-time link back to the substrate
/// primitive.
///
/// Complements the pre-existing forward-projection triple
/// ([`std::fmt::Display`], [`AsRef<str>`], [`InvariantKind::as_str`])
/// with the paired trait-idiomatic reverse-projection axis: Rust-side
/// newtype/typed-enum convention pairs [`AsRef<str>`] with either
/// [`std::str::FromStr`] or [`TryFrom<&str>`] on the same primitive so a
/// caller who can project *out to* a `&str` can also project *in from*
/// one. The [`TryFrom<&str>`] axis is deliberately chosen over
/// [`std::str::FromStr`] to sidestep the `clippy::should_implement_trait`
/// lint the sibling method-named [`InvariantKind::from_wire`] would
/// trigger under a `FromStr` impl (the same design tradeoff the peer
/// [`caixa_core::CaixaKind`] (3c83606), [`caixa_core::CaixaDialeto`]
/// (bf33136), [`caixa_core::aplicacao::PlacementStrategy`] (6fd00cd),
/// [`caixa_core::supervisor::RestartStrategy`] (5b828ed),
/// [`caixa_core::supervisor::RestartPolicy`] (6fdd0d9),
/// [`caixa_core::aplicacao::WitShape`] (5472902),
/// [`caixa_core::aplicacao::RateLimitUnit`] (bf78400), and
/// [`caixa_core::render::PathShapeViolation`] (e67e48a) blocks note) —
/// this impl closes the trait-idiomatic reverse axis without disturbing
/// the method-named `from_wire` shape every peer closed-set typed enum
/// already carries.
///
/// `type Error = ()` matches the sibling [`InvariantKind::from_wire`]'s
/// `Option<Self>` return-shape's deliberate deferral of error typing: the
/// caller picks the diagnostic form appropriate for its use site (a
/// future `feira arch --severity` CLI arg-parse composes its own per-verb
/// "unknown arch-severity axis: <arg> — accepted: {…}" message
/// enumerating [`InvariantKind::ALL`], a future M4 admission-webhook
/// rejection body wraps the `Err(())` outcome with the accepted-set
/// enumeration for operator diagnostics, a `Result::map_err` at the
/// call site lifts the axis-error to a per-verb error type). Same
/// shape the peer sibling reverse-projection axes carry.
///
/// The paired [`TryFrom<&str>`] impl reaches the same three-arm
/// accept-set the [`InvariantKind::from_wire`] resolver dispatches
/// through, so any future arm addition (a `Warning` tier between
/// [`Self::Compliance`] and [`Self::Hint`] the `iac-forge` policy-engine
/// grows, a `Fatal` tier above [`Self::Safety`] — both trajectory items
/// the sibling [`InvariantKind::ALL`] doc block already names) grows the
/// trait-idiomatic axis by construction: one caixa-arch edit on
/// [`InvariantKind::from_wire`] extends both the method-named reverse
/// projection every existing consumer keys off and the trait-idiomatic
/// reverse projection this impl exposes, without a coordinated rewrite
/// across every future `TryFrom<&str>`-bound consumer's arm-set.
///
/// Extends the substrate-wide closed-set-enum trait-idiomatic
/// reverse-projection family ([`caixa_core::CaixaKind`] via 3c83606,
/// [`caixa_core::CaixaDialeto`] via bf33136,
/// [`caixa_core::aplicacao::PlacementStrategy`] via 6fd00cd,
/// [`caixa_core::supervisor::RestartStrategy`] via 5b828ed,
/// [`caixa_core::supervisor::RestartPolicy`] via 6fdd0d9,
/// [`caixa_core::aplicacao::WitShape`] via 5472902,
/// [`caixa_core::aplicacao::RateLimitUnit`] via bf78400, and
/// [`caixa_core::render::PathShapeViolation`] via e67e48a) onto the first
/// outside-`caixa-core` closed-set fieldless typed enum on the caixa
/// surface — the caixa-arch invariant-severity three-arm accept-set every
/// `feira arch` / `feira tofu` per-violation render site and every future
/// M4 admission-webhook / policy-engine audit-loader dispatches through.
///
/// Pinned load-bearing by
/// [`tests::invariant_kind_try_from_str_routes_through_from_wire_accessor`]
/// (byte-parity pin against [`InvariantKind::from_wire`] across the
/// three-arm accept-set) and
/// [`tests::invariant_kind_try_from_str_rejects_unknown_byte_strings`]
/// (rejection witness against silent accept-set widening).
impl TryFrom<&str> for InvariantKind {
    type Error = ();

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::from_wire(s).ok_or(())
    }
}

/// Standard-library trait-idiomatic forward projection on the
/// [`InvariantKind`] closed-set caixa-arch invariant-severity axis.
/// Routes byte-for-byte through the paired substrate-primitive
/// [`InvariantKind::as_str`] `pub const fn` accessor so
/// `<&'static str>::from(kind)` / `kind.into::<&'static str>()` reaches
/// the same three-arm `"safety"` / `"compliance"` / `"hint"` canonical-
/// lowercase emit-set the sibling method-named accessor dispatches
/// through and the sibling [`std::fmt::Display for InvariantKind`] /
/// [`AsRef<str> for InvariantKind`] impls also route through.
///
/// Extends the substrate-wide closed-set-enum trait-idiomatic
/// forward-projection family
/// ([`caixa_core::supervisor::RestartStrategy`] via 523157d,
/// [`caixa_core::supervisor::RestartPolicy`] via 9fb37d0,
/// [`caixa_core::CaixaKind`] via edb827b,
/// [`caixa_core::CaixaDialeto`] via c189a6f,
/// [`caixa_core::aplicacao::PlacementStrategy`] via afa3562,
/// [`caixa_core::aplicacao::WitShape`] via 56998ec,
/// [`caixa_core::aplicacao::RateLimitUnit`] via 7fdfbf4,
/// [`caixa_core::render::PathShapeViolation`] via 070a6de) onto the
/// first outside-`caixa-core` closed-set fieldless typed enum on the
/// caixa surface — the caixa-arch invariant-severity three-arm accept-
/// set every `feira arch` / `feira tofu` per-violation render site and
/// every future M4 admission-webhook / policy-engine audit-loader
/// dispatches through. Opens the trait-idiomatic forward-projection
/// family on the outside-caixa-core closed-set fieldless typed-enum
/// surface so a downstream `impl From<T> for &'static str`-bound generic
/// consumer reaches the caixa-arch invariant-severity axis through the
/// same uniform trait dispatch every caixa-core sibling already carries.
///
/// Pairs with the sibling [`TryFrom<&str> for InvariantKind`] impl
/// (e21a857) to close the two-way `Self ↔ &'static str` round-trip on
/// the trait-idiomatic axis pair, mirroring the pre-existing
/// method-named [`InvariantKind::as_str`] +
/// [`InvariantKind::from_wire`] pair on the substrate-primitive axis
/// pair.
///
/// Return type is `&'static str` by construction — every
/// [`InvariantKind::as_str`] arm resolves to an inline `"safety"` /
/// `"compliance"` / `"hint"` `&'static str` literal, so the trait's
/// return-type promise is upheld structurally without a
/// [`String::leak`] cast or a per-arm inline literal outside the paired
/// [`InvariantKind::as_str`] dispatch.
///
/// The paired [`InvariantKind::as_str`] accessor's three-arm emit-set
/// is the single source of truth — every future arm addition (a
/// `Warning` tier between [`Self::Compliance`] and [`Self::Hint`] the
/// `iac-forge` policy-engine grows, a `Fatal` tier above
/// [`Self::Safety`] — both trajectory items the sibling
/// [`InvariantKind::ALL`] doc block already names) grows the trait-
/// idiomatic forward axis by construction: one caixa-arch edit on
/// [`InvariantKind::as_str`] extends every one of the sibling forward-
/// projection paths ([`std::fmt::Display`], [`AsRef<str>`],
/// [`InvariantKind::as_str`] itself, and this
/// [`From<Self> for &'static str`]) without a coordinated rewrite
/// across every future `Into<&'static str>`-bound consumer's arm-set.
///
/// Pinned load-bearing by
/// [`tests::invariant_kind_from_into_static_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`InvariantKind::as_str`] across the three-
/// arm emit-set, plus a `const`-context materialization witness for the
/// `&'static str` lifetime promise routed through the paired
/// [`InvariantKind::as_str`] `pub const fn` accessor, plus a paired
/// `.into()` shape assertion covering the blanket-derived
/// `Into<&'static str>` shape) and
/// [`tests::invariant_kind_from_into_static_str_and_as_str_partition_the_emit_set`]
/// (partition pin asserting `<&'static str as
/// From<InvariantKind>>::from` and [`InvariantKind::as_str`] agree on
/// every arm, plus a two-way direct round-trip witness through the
/// paired trait-idiomatic [`TryFrom<&str>`] axis that closes the two-way
/// `Self ↔ &'static str` round-trip on the trait-idiomatic axis pair —
/// the emit-side [`InvariantKind::as_str`] and the parse-side
/// [`InvariantKind::from_wire`] dispatch on the same three inline
/// canonical-lowercase byte-strings by construction, so round-tripping
/// composes the two trait impls directly).
impl From<InvariantKind> for &'static str {
    fn from(kind: InvariantKind) -> &'static str {
        kind.as_str()
    }
}

/// Trait-idiomatic *borrowed-input* forward projection on
/// [`InvariantKind`] onto the `&'static str` axis — the borrowed-input
/// companion to the paired owned-input [`From<InvariantKind> for
/// &'static str`] impl immediately above. Routes byte-for-byte through
/// the same substrate-primitive [`InvariantKind::as_str`] `pub const fn`
/// accessor so every consumer that binds a `&InvariantKind` through the
/// standard-library `.into()` / [`From<&Self> for &'static str`] axis (a
/// `InvariantKind::ALL.iter().map(<&'static str>::from).collect::<Vec<_>>()`
/// per-arm accept-set materializer — whose iterator over
/// `&'static [InvariantKind]` yields `&InvariantKind`, not
/// `InvariantKind`, so the owned-input [`From<InvariantKind>`] axis
/// alone forces every call site through an explicit `.copied()` /
/// dereference / [`Copy`]-bound restatement rather than the direct
/// trait-idiomatic projection; a future `feira arch --list-severities`
/// CLI enumeration composed via
/// `InvariantKind::ALL.iter().map(Into::into)`; a future M4 admission-
/// webhook rejection body whose accepted-set enumeration walks the
/// same iterator shape; a future `HashMap::<&'static str, usize>::
/// from_iter(violations.iter().map(|v| (<&'static str>::from(&v.kind), 0)))`
/// per-severity histogram seed on the operator's audit path — whose
/// borrowed access off `&Violation.kind` avoids a `.copied()` /
/// [`Copy`]-bound dereference on the arch-severity field) reaches the
/// same three-arm `"safety"` / `"compliance"` / `"hint"` canonical-
/// lowercase emit-set the paired owned-input [`From<InvariantKind> for
/// &'static str`], the sibling [`std::fmt::Display`], [`AsRef<str>`],
/// and [`InvariantKind::as_str`] surfaces already return.
///
/// First outside-`caixa-core` peer on the substrate-wide trait-idiomatic
/// *borrowed-input* `&'static str`-returning forward-projection family
/// opened on [`caixa_core::dep::DepList`] (64aa742) and extended onto
/// [`caixa_core::CaixaKind`], [`caixa_core::CaixaDialeto`],
/// [`caixa_core::supervisor::RestartStrategy`],
/// [`caixa_core::supervisor::RestartPolicy`],
/// [`caixa_core::aplicacao::PlacementStrategy`],
/// [`caixa_core::aplicacao::WitShape`],
/// [`caixa_core::aplicacao::RateLimitUnit`], and
/// [`caixa_core::render::PathShapeViolation`] (cdf4e95, first render-
/// side arm). Rust's `From` trait does not auto-derive the `From<&Self>`
/// sibling from a `From<Self>` impl (the blanket
/// `impl<T, U> From<&T> for U where T: Copy, U: From<T>` does not exist
/// in `core`), so every closed-set typed enum that carries the owned-
/// input axis but not the borrowed-input axis forces every borrowed-
/// input call site through a `.copied()` / `<&'static str>::from(*kind)`
/// / `kind.as_str()` detour whose type bounds have no compile-time link
/// to the substrate primitive. Lifting the borrowed-input axis on the
/// first outside-`caixa-core` closed-set fieldless typed enum on the
/// caixa surface (the caixa-arch invariant-severity axis) closes that
/// gap on the same trajectory the paired owned-input axis
/// ([`impl From<InvariantKind> for &'static str`] immediately above)
/// already opened.
///
/// Pinned load-bearing by
/// [`tests::invariant_kind_from_borrowed_into_static_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`InvariantKind::as_str`] across the three-
/// arm emit-set via a borrowed input, plus a `const`-context
/// materialization witness for the `&'static str` lifetime promise) and
/// [`tests::invariant_kind_from_owned_and_borrowed_into_static_str_agree_on_every_arm`]
/// (cross-axis partition pin against the paired owned-input
/// [`From<InvariantKind> for &'static str`] impl, plus a
/// `.iter().map(Into::into)` pipe witness over [`InvariantKind::ALL`]
/// whose iterator yields `&InvariantKind` by construction so this
/// borrowed-input axis is what routes the pipe through the substrate-
/// primitive accessor without a spurious `Copy` deref).
impl From<&InvariantKind> for &'static str {
    fn from(kind: &InvariantKind) -> &'static str {
        kind.as_str()
    }
}

/// Trait-idiomatic *owned-input, owned-`String` output* forward
/// projection on [`InvariantKind`] onto the owned-`String` axis —
/// the owned-`String` companion to the paired [`From<InvariantKind>
/// for &'static str`] and [`From<&InvariantKind> for &'static str`]
/// siblings immediately above. Routes byte-for-byte through the
/// substrate-primitive [`InvariantKind::as_str`] `pub const fn`
/// accessor via [`str::to_owned`] so every consumer that binds an
/// [`InvariantKind`] through the standard-library `.into()` /
/// [`From<Self> for String`] axis (a `let key: String =
/// kind.into();`-shaped downstream call site; a future
/// `serde_json::Value::String(kind.into())` structured-payload
/// composer where the `Value::String` arm typing demands an owned
/// [`String`] and the sibling `&'static str`-returning axes force an
/// explicit `.to_owned()` / [`String::from`] restatement at every
/// call site; a future `HashMap::<String, InvariantKind>::from_iter`
/// per-severity lookup on the operator's audit path where the
/// map's key type is owned [`String`] rather than `&'static str`; a
/// future [`std::borrow::Cow::<'static, str>::Owned(kind.into())`]
/// composer on a future M4 admission-webhook rejection body's owned-
/// arm; a future caixa-build pipeline's per-severity structured-log
/// emit where the JSON serializer's [`Serialize`] impl on [`String`]
/// owns the emit-path) reaches the same three-arm `"safety"` /
/// `"compliance"` / `"hint"` canonical-lowercase emit-set the
/// paired `&'static str`-returning axes, the sibling
/// [`std::fmt::Display`], [`AsRef<str>`], and
/// [`InvariantKind::as_str`] surfaces already return — no
/// `.to_owned()` / `String::from(kind.as_str())` detour whose type
/// bounds have no compile-time link to the substrate primitive.
///
/// Rust's standard library does not carry a blanket
/// `impl<T: AsRef<str>> From<T> for String` (nor an
/// `impl<T: fmt::Display> From<T> for String`), so every closed-
/// set typed enum that carries the paired [`AsRef<str>`] /
/// [`std::fmt::Display`] / [`From<Self> for &'static str`] /
/// [`From<&Self> for &'static str`] quadruple but not the owned-
/// `String` axis forces every owned-string call site through the
/// detour above. This lift closes that axis on the first outside-
/// `caixa-core` closed-set fieldless typed enum on the caixa
/// surface (the caixa-arch invariant-severity three-arm axis),
/// matching the trajectory each of the nine prior peer enums —
/// [`caixa_core::supervisor::RestartStrategy`] (7baa18a, first-
/// mover on this axis), [`caixa_core::supervisor::RestartPolicy`]
/// (7851725), [`caixa_core::CaixaKind`] (231a18c),
/// [`caixa_core::CaixaDialeto`] (88942cd),
/// [`caixa_core::dep::DepList`] (32b0ee8),
/// [`caixa_core::aplicacao::PlacementStrategy`] (1154c2f),
/// [`caixa_core::aplicacao::WitShape`] (79a8723),
/// [`caixa_core::aplicacao::RateLimitUnit`] (c7d687d), and
/// [`caixa_core::render::PathShapeViolation`] (6e0479a, first
/// render-side arm) — followed on the same 2×2-completion campaign.
///
/// Pinned load-bearing by
/// [`tests::invariant_kind_from_into_owned_string_routes_through_as_str_accessor`]
/// (byte-parity pin against [`InvariantKind::as_str`] across the
/// three-arm emit-set via the owned-`String` surface) and
/// [`tests::invariant_kind_from_into_owned_string_and_static_str_agree_on_every_arm`]
/// (cross-axis partition pin against the paired owned-input
/// `&'static str`-returning [`From<InvariantKind> for &'static
/// str`] impl and the [`ToString::to_string`]-through-
/// [`std::fmt::Display`] surface, plus a `.iter().copied().map(String::from)`
/// pipe witness over [`InvariantKind::ALL`], plus a direct `Self →
/// String → Self` round-trip witness through the paired
/// [`TryFrom<&str>`] axis on the owned-[`String`]'s
/// [`String::as_str`] borrow).
impl From<InvariantKind> for String {
    fn from(kind: InvariantKind) -> String {
        kind.as_str().to_owned()
    }
}

/// Trait-idiomatic *borrowed-input, owned-`String` output* forward
/// projection on [`InvariantKind`] onto the owned-`String` axis — the
/// fourth (and closing) corner of the substrate-wide
/// `{Self, &Self} × {&'static str, String}` 2×2 trait-idiomatic
/// projection family on the first outside-`caixa-core` closed-set
/// fieldless typed enum on the caixa surface (the caixa-arch
/// invariant-severity three-arm axis). Routes byte-for-byte through the
/// substrate-primitive [`InvariantKind::as_str`] `pub const fn`
/// accessor (via [`str::to_owned`]) so every consumer that holds a
/// borrowed [`&InvariantKind`] and needs an owned [`String`] — a
/// future `serde_json::Value::String(String::from(&kind))` structured-
/// payload composer over a borrowed [`Violation::kind`] field, a
/// future `Iterator::map` over `&[Violation]` that projects to owned
/// severity keys through `.iter().map(|v| String::from(&v.kind))`
/// (whose iterator yields `&InvariantKind`, not [`InvariantKind`], so
/// the owned-input [`From<InvariantKind> for String`] axis alone forces
/// every call site through an explicit `.copied()` / spurious [`Copy`]
/// deref restatement rather than the direct trait-idiomatic
/// projection), a future
/// `HashMap::<String, usize>::from_iter(violations.iter().map(|v| (String::from(&v.kind), 0)))`
/// per-severity histogram seed on the operator's audit path where the
/// map's key type is owned [`String`] and the borrowed-iteration axis
/// over `&Violation.kind` avoids a spurious [`Copy`] on the arch-
/// severity field, a future M4 admission-webhook rejection body
/// composer whose accepted-severity enumeration walks
/// `InvariantKind::ALL.iter().map(String::from)` (whose iterator
/// yields `&InvariantKind` by construction) — reaches the same three-
/// arm `"safety"` / `"compliance"` / `"hint"` canonical-lowercase
/// byte-strings the paired [`std::fmt::Display`], [`AsRef<str>`],
/// [`InvariantKind::as_str`], and the three other trait-idiomatic
/// forward-projection impls
/// ([`From<InvariantKind> for &'static str`],
/// [`From<&InvariantKind> for &'static str`],
/// [`From<InvariantKind> for String`]) already return.
///
/// Eleventh peer on the substrate-wide trait-idiomatic *borrowed-
/// input, owned-`String` output* forward-projection family opened on
/// [`caixa_core::supervisor::RestartStrategy`] (579385f), closed on
/// the M2 OTP-shape sibling axis pair by
/// [`caixa_core::supervisor::RestartPolicy`] (8465740), extended onto
/// the two-list dep-graph peer by [`caixa_core::dep::DepList`]
/// (e0cb617), onto the top-level [`caixa_core::CaixaKind`] peer by
/// (e76436d), the dialect-classification peer
/// [`caixa_core::CaixaDialeto`] (d3c0d1d), the M3 mesh-primitive
/// [`caixa_core::aplicacao::PlacementStrategy`] (d3dc000),
/// [`caixa_core::aplicacao::WitShape`] (d638fd3),
/// [`caixa_core::aplicacao::RateLimitUnit`] (6424e45 — closing the
/// whole M3 triple's 2×2 corner), and
/// [`caixa_core::render::PathShapeViolation`] (b90e193 — first
/// outside-manifest-surface arm on this axis). *First outside-`caixa-
/// core` peer* on this axis — the M2 / M3 slot enums, the two-list
/// dep-graph axis, the top-level `:kind` axis, the dialect-
/// classification axis, and the render-side path-shape-diagnostic
/// axis form the caixa-core arm; this lift closes the 2×2 on the
/// first outside-caixa-core arm (the caixa-arch invariant-severity
/// three-arm axis every `feira arch` / `feira tofu` per-violation
/// render site dispatches through), on the same trajectory the paired
/// owned-input owned-[`String`] axis (1afd8d5 — the tenth peer) and
/// the paired borrowed-input owned-[`&'static str`] axis (238d886)
/// already took onto the same enum.
///
/// Rust's standard library does not carry a blanket
/// `impl<T: AsRef<str>> From<&T> for String` (nor an
/// `impl<T: fmt::Display> From<&T> for String`), so every closed-set
/// typed enum that carries the paired [`AsRef<str>`] /
/// [`std::fmt::Display`] / [`From<Self> for &'static str`] /
/// [`From<&Self> for &'static str`] / [`From<Self> for String`]
/// quintuple but not the borrowed-input owned-[`String`] axis forces
/// every borrowed-input owned-string call site through a
/// `kind.as_str().to_owned()` / `String::from(*kind)` (with a spurious
/// [`Copy`]) / `kind.to_string()` (through [`std::fmt::Display`])
/// detour whose type bounds have no compile-time link to the
/// substrate primitive.
///
/// Same three-path convergence discipline as the paired owned-input
/// impl (this borrowed-input axis, the paired owned-input
/// [`From<InvariantKind> for String`], and [`InvariantKind::as_str`]
/// all route through the same three-arm inline canonical-lowercase
/// byte-strings), so a future variant addition (a `Warning` tier
/// between `Compliance` and `Hint` the `iac-forge` policy-engine
/// grows, a `Fatal` tier above `Safety` — both trajectory items the
/// [`InvariantKind::ALL`] and [`InvariantKind::as_str`] doc blocks
/// already name) reaches every one of the paired forward-projection
/// paths through exactly one caixa-arch edit on the
/// [`InvariantKind::as_str`] `pub const fn` accessor.
///
/// The [`InvariantKind::as_str`] emit and [`InvariantKind::from_wire`]
/// parse share the same three inline canonical-lowercase byte-strings
/// by construction — so the borrowed-input owned-[`String`] forward
/// axis and the reverse [`TryFrom<&str>`] axis compose directly (via
/// the owned-[`String`]'s [`String::as_str`] borrow) without the
/// intermediate wire-vocab hop the peer [`caixa_core::CaixaKind`]
/// axis pair requires. The round-trip witness pin below locks this
/// direct composition on the caixa-arch invariant-severity enum's
/// borrowed-input owned-[`String`] axis pair.
///
/// Pinned load-bearing by
/// [`tests::invariant_kind_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
/// (byte-parity pin against [`InvariantKind::as_str`] across the
/// three-arm emit-set through the borrowed-input surface) and
/// [`tests::invariant_kind_from_into_borrowed_owned_string_agrees_with_paired_axes_on_every_arm`]
/// (cross-axis partition pin against the paired owned-input owned-
/// [`String`] [`From<InvariantKind> for String`] impl, the paired
/// borrowed-input owned-[`&'static str`]
/// [`From<&InvariantKind> for &'static str`] impl, the paired owned-
/// input owned-[`&'static str`] [`From<InvariantKind> for &'static
/// str`] impl — every corner of the 2×2 — plus a
/// `.iter().map(String::from)` pipe witness over
/// [`InvariantKind::ALL`] (whose iterator yields `&InvariantKind` by
/// construction, so the borrowed-input owned-[`String`] axis is what
/// routes the pipe through the substrate-primitive
/// [`InvariantKind::as_str`] accessor without a spurious [`Copy`]
/// deref), plus a direct round-trip witness through
/// [`TryFrom<&str>`] on the owned-[`String`]'s [`String::as_str`]
/// borrow that closes the two-way `&Self → String → Self` round-trip
/// on the trait-idiomatic borrowed-input owned-[`String`] forward +
/// reverse axis pair — no intermediate wire-vocab hop like the peer
/// [`caixa_core::CaixaKind`] axis pair requires).
impl From<&InvariantKind> for String {
    fn from(kind: &InvariantKind) -> String {
        kind.as_str().to_owned()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub invariant_id: String,
    pub kind: InvariantKind,
    pub instance_tipo: String,
    pub instance_nome: String,
    pub message: String,
}

#[derive(Clone)]
pub struct Invariant {
    pub id: &'static str,
    pub kind: InvariantKind,
    pub description: &'static str,
    pub check: fn(&TeiaManifest) -> Vec<Violation>,
}

#[must_use]
pub fn builtin_invariants() -> Vec<Invariant> {
    vec![
        Invariant {
            id: "unique-resource-names",
            kind: InvariantKind::Safety,
            description: "no two instances share (tipo, nome) — Terraform would reject it",
            check: unique_resource_names,
        },
        Invariant {
            id: "no-unresolved-refs",
            kind: InvariantKind::Safety,
            description: "every (ref tipo nome attr) points at an instance that exists",
            check: no_unresolved_refs,
        },
        Invariant {
            id: "no-public-ingress-without-tags",
            kind: InvariantKind::Compliance,
            description: "resources exposed to 0.0.0.0/0 must carry an owner/team tag",
            check: no_public_ingress_without_tags,
        },
        Invariant {
            id: "cidr-block-looks-valid",
            kind: InvariantKind::Hint,
            description: ":cidr-block values should look like IPv4/CIDR notation",
            check: cidr_block_format_hint,
        },
    ]
}

fn unique_resource_names(m: &TeiaManifest) -> Vec<Violation> {
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for inst in &m.instances {
        // Read the per-`TeiaInstance` `:tipo` provider-qualified
        // resource-type identity and the per-`TeiaInstance` `:nome`
        // per-resource instance-identity through the lifted
        // [`caixa_teia::TeiaInstance::tipo`] / [`caixa_teia::TeiaInstance
        // ::nome`] scalar accessors rather than the raw `inst.tipo` /
        // `inst.nome` field accesses — the `(tipo, nome)` dedup key,
        // the `Violation::instance_tipo` / `Violation::instance_nome`
        // carriers, and the `{}` Display interpolations all key off the
        // substrate-canonical `:tipo` / `:nome` resolvers so a future
        // rebrand on either typed slot's raw-slot reader lands at
        // exactly one place.
        let key = (inst.tipo().to_string(), inst.nome().to_string());
        if !seen.insert(key.clone()) {
            out.push(Violation {
                invariant_id: "unique-resource-names".into(),
                kind: InvariantKind::Safety,
                instance_tipo: inst.tipo().to_string(),
                instance_nome: inst.nome().to_string(),
                message: format!(
                    "duplicate instance {} / {} — Terraform resource names must be unique per type",
                    inst.tipo(),
                    inst.nome(),
                ),
            });
        }
    }
    out
}

fn no_unresolved_refs(m: &TeiaManifest) -> Vec<Violation> {
    let mut declared: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for inst in &m.instances {
        // Same accessor-routed `:tipo` / `:nome` reads as the sibling
        // `unique-resource-names` invariant — the two dedup / lookup
        // sets are identity-comparable only because they share exactly
        // one `:tipo` / `:nome` resolver pair.
        declared.insert((inst.tipo().to_string(), inst.nome().to_string()));
    }
    let mut out = Vec::new();
    for inst in &m.instances {
        for v in inst.atributos.values() {
            collect_ref_violations(inst, v, &declared, &mut out);
        }
    }
    out
}

fn collect_ref_violations(
    inst: &TeiaInstance,
    v: &TeiaValue,
    declared: &std::collections::HashSet<(String, String)>,
    out: &mut Vec<Violation>,
) {
    match v {
        TeiaValue::Ref(r) => {
            // Read the per-`TeiaRefRepr` `:tipo` provider-qualified
            // resource-type identity, the per-`TeiaRefRepr` `:nome`
            // per-reference target-instance-identity, and the
            // per-`TeiaRefRepr` `:atributo` per-target
            // attribute-projection through the lifted
            // [`caixa_teia::TeiaRefRepr::tipo`] /
            // [`caixa_teia::TeiaRefRepr::nome`] /
            // [`caixa_teia::TeiaRefRepr::atributo`] scalar accessors
            // rather than the raw `r.tipo` / `r.nome` / `r.atributo`
            // field accesses — the declared-set `(tipo, nome)` lookup
            // key + the `(ref <tipo> <nome> <attr>)` refusal-message
            // Display interpolation both key off the substrate-
            // canonical per-`Ref` `:tipo` / `:nome` / `:atributo`
            // resolvers, so a future rebrand on any typed slot's
            // raw-slot reader lands at exactly one place and the
            // `no-unresolved-refs` gate stays lockstep with the
            // `caixa-pangea` `${<tf>.<nome>.<attr>}` Terraform-JSON
            // emit path's `<tf>` / `<nome>` / `<attr>` mints.
            if !declared.contains(&(r.tipo().to_string(), r.nome().to_string())) {
                out.push(Violation {
                    invariant_id: "no-unresolved-refs".into(),
                    kind: InvariantKind::Safety,
                    instance_tipo: inst.tipo().to_string(),
                    instance_nome: inst.nome().to_string(),
                    message: format!(
                        "(ref {} {} {}) targets an undeclared instance",
                        r.tipo(),
                        r.nome(),
                        r.atributo()
                    ),
                });
            }
        }
        TeiaValue::List(items) => items
            .iter()
            .for_each(|i| collect_ref_violations(inst, i, declared, out)),
        TeiaValue::Object(map) => map
            .values()
            .for_each(|i| collect_ref_violations(inst, i, declared, out)),
        _ => {}
    }
}

fn no_public_ingress_without_tags(m: &TeiaManifest) -> Vec<Violation> {
    let mut out = Vec::new();
    for inst in &m.instances {
        // Bind the accessor's return-slice once so both the kebab-case
        // and snake_case `security_group`-substring gates key off the
        // same borrow — every `:tipo` read on this invariant funnels
        // through exactly one accessor call.
        let tipo = inst.tipo();
        let sg_like = tipo.contains("security-group") || tipo.contains("security_group");
        let has_public_cidr = flatten_strings(&inst.atributos)
            .iter()
            .any(|s| s.contains("0.0.0.0/0"));
        // Route the per-`:tags` `TeiaValue::Object` arm projection
        // through the lifted [`caixa_teia::TeiaValue::as_object`]
        // typed `Option<&BTreeMap<…>>` accessor rather than the raw
        // `.and_then(|v| match v { TeiaValue::Object(m) => Some(m),
        //  _ => None })` inline closure — the per-`Object`-arm map
        // projection now keys off the substrate-canonical sum-type
        // per-arm accessor every downstream `TeiaValue`-facing
        // consumer that needs the map payload (without walking the
        // whole value tree) routes through, sibling to the peer
        // per-`Str`-arm projection at [`cidr_block_format_hint`]'s
        // `TeiaValue::as_str` route. Any future arm-set extension
        // (an `Enum(String)` variant, a per-provider tagged shape)
        // picks up this dispatch through exactly one edit on the
        // substrate primitive.
        let has_owner_tag = inst
            .atributos
            .get("tags")
            .and_then(TeiaValue::as_object)
            .is_some_and(|m| {
                m.keys().any(|k| {
                    k.eq_ignore_ascii_case("owner")
                        || k.eq_ignore_ascii_case("team")
                        || k.eq_ignore_ascii_case("dono")
                })
            });
        if sg_like && has_public_cidr && !has_owner_tag {
            out.push(Violation {
                invariant_id: "no-public-ingress-without-tags".into(),
                kind: InvariantKind::Compliance,
                instance_tipo: tipo.to_string(),
                instance_nome: inst.nome().to_string(),
                message: "public-ingress security group needs :owner or :team tag".into(),
            });
        }
    }
    out
}

fn cidr_block_format_hint(m: &TeiaManifest) -> Vec<Violation> {
    let mut out = Vec::new();
    for inst in &m.instances {
        // Route the per-`:cidr-block` `TeiaValue::Str` arm projection
        // through the lifted [`caixa_teia::TeiaValue::as_str`] typed
        // `Option<&str>` accessor rather than the raw
        // `if let Some(TeiaValue::Str(s)) = inst.atributos.get(...)`
        // open-coded per-arm pattern-match — the per-`Str`-arm scalar
        // projection now keys off the substrate-canonical sum-type
        // per-arm accessor every downstream `TeiaValue`-facing consumer
        // that needs a plain string scalar (without walking the whole
        // value tree) routes through, sibling to the peer per-`Object`-
        // arm projection at [`no_public_ingress_without_tags`]'s
        // `TeiaValue::as_object` route.
        if let Some(s) = inst.atributos.get("cidr-block").and_then(TeiaValue::as_str)
            && !looks_like_cidr(s)
        {
            out.push(Violation {
                invariant_id: "cidr-block-looks-valid".into(),
                kind: InvariantKind::Hint,
                instance_tipo: inst.tipo().to_string(),
                instance_nome: inst.nome().to_string(),
                message: format!(":cidr-block {s:?} does not look like IPv4/CIDR"),
            });
        }
    }
    out
}

fn looks_like_cidr(s: &str) -> bool {
    let Some((ip, mask)) = s.split_once('/') else {
        return false;
    };
    if mask.parse::<u8>().map_or(true, |m| m > 32) {
        return false;
    }
    let parts: Vec<&str> = ip.split('.').collect();
    parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok())
}

fn flatten_strings(map: &std::collections::BTreeMap<String, TeiaValue>) -> Vec<String> {
    let mut out = Vec::new();
    for v in map.values() {
        collect_strings(v, &mut out);
    }
    out
}

fn collect_strings(v: &TeiaValue, out: &mut Vec<String>) {
    match v {
        TeiaValue::Str(s) => out.push(s.clone()),
        TeiaValue::List(items) => items.iter().for_each(|i| collect_strings(i, out)),
        TeiaValue::Object(m) => m.values().for_each(|i| collect_strings(i, out)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invariant_kind_all_lists_every_variant_in_declaration_order() {
        // The fail-before-pass-after pin on the closed three-arm
        // [`InvariantKind`] discriminator set: any future variant
        // addition (a `Warning` tier between [`InvariantKind::Compliance`]
        // and [`InvariantKind::Hint`], a `Fatal` tier above
        // [`InvariantKind::Safety`] the `iac-forge` policy-engine grows)
        // that lands the new variant on the enum without extending
        // [`InvariantKind::ALL`] trips this test — the compiler-checked
        // exhaustiveness on the sibling
        // [`gen_platform::IsVariant`]-derived per-arm predicates
        // ([`InvariantKind::is_safety`] / [`InvariantKind::is_compliance`]
        // / [`InvariantKind::is_hint`]) covers the projection axis; this
        // pin covers the exhaustive-iteration axis so both halves of the
        // closed-set discipline migrate as one edit.
        assert_eq!(
            InvariantKind::ALL,
            &[
                InvariantKind::Safety,
                InvariantKind::Compliance,
                InvariantKind::Hint,
            ],
        );
        // Every arm satisfies exactly the paired per-arm predicate — the
        // byte-parity pin between the [`InvariantKind::ALL`] iteration
        // axis and the per-arm [`gen_platform::IsVariant`]-derived
        // predicate axis: for every arm, the paired predicate returns
        // `true` and every other predicate returns `false`. Same
        // discipline the sibling [`caixa_core::CaixaKind::ALL`] +
        // per-arm `requires_*` peer pins carry.
        for arm in InvariantKind::ALL {
            let (safety, compliance, hint) = (arm.is_safety(), arm.is_compliance(), arm.is_hint());
            assert_eq!(
                usize::from(safety) + usize::from(compliance) + usize::from(hint),
                1,
                "InvariantKind::{arm:?} must satisfy exactly one of \
                 is_safety / is_compliance / is_hint",
            );
        }
    }

    #[test]
    fn invariant_kind_predicates_are_byte_equal_to_matches_family() {
        // The fail-before-pass-after pin on the two-axis convergence
        // of the four pre-lift `matches!(v.kind, InvariantKind::…)`
        // sites at [`crate::run::check_manifest`] (3 sites — safety /
        // compliance / hint) + [`crate::report::ArchReport::safety_count`]
        // (1 site — safety) onto the
        // [`gen_platform::IsVariant`]-derive-generated per-arm
        // predicate family: for every arm, each predicate agrees
        // byte-for-byte with the pre-lift `matches!` shape.
        //
        // A future rebrand touching either endpoint (a
        // `#[is_variant(name = "…")]` attribute drift on the derive,
        // an arm rename, an accidental peer predicate that shadows the
        // derive-generated one) would silently split the two paths and
        // trip this pin. Peer of the sibling
        // [`caixa_core::supervisor::tests::restart_strategy_is_simple_one_for_one_matches_typed_dispatch`]
        // and [`caixa_core::supervisor::tests::restart_policy_predicates_are_byte_equal_to_matches`]
        // pins on the peer closed-set-enum IsVariant convergence axes.
        for arm in InvariantKind::ALL {
            assert_eq!(
                arm.is_safety(),
                matches!(arm, InvariantKind::Safety),
                "InvariantKind::{arm:?}.is_safety() must agree with \
                 matches!(_, InvariantKind::Safety) byte-for-byte",
            );
            assert_eq!(
                arm.is_compliance(),
                matches!(arm, InvariantKind::Compliance),
                "InvariantKind::{arm:?}.is_compliance() must agree with \
                 matches!(_, InvariantKind::Compliance) byte-for-byte",
            );
            assert_eq!(
                arm.is_hint(),
                matches!(arm, InvariantKind::Hint),
                "InvariantKind::{arm:?}.is_hint() must agree with \
                 matches!(_, InvariantKind::Hint) byte-for-byte",
            );
        }
    }

    #[test]
    fn invariant_kind_as_str_returns_canonical_lowercase_tag_per_arm() {
        // Fail-before-pass-after per-arm tag pin on the substrate-canonical
        // [`InvariantKind::as_str`] `pub const fn` accessor: for every arm
        // in [`InvariantKind::ALL`], the accessor returns the canonical
        // lowercase byte-string the paired `caixa-feira/src/cmd/tofu.rs`
        // per-violation render site's pre-lift
        // `format!("{:?}", v.kind).to_lowercase()` shape produced by
        // round-tripping through the derived [`std::fmt::Debug`] output.
        //
        // A rename of any of the three lowercase tags (or a swap onto a
        // PascalCase / snake-case variant that would silently split the
        // three closed-set arms' downstream diagnostic-tag identity from
        // the substrate-canonical byte-string every peer closed-set-enum
        // `as_str` accessor returns) trips this pin at caixa-arch test
        // time rather than at a downstream `feira tofu` operator's silent
        // misclassification. Peer of the sibling
        // [`caixa_lint::diagnostic::tests::severity_as_ref_str_returns_canonical_lowercase_tag_per_arm`]
        // (ce9d1e3) pin on the caixa-lint `Severity` axis and the sibling
        // per-arm as_str pins on the caixa-core closed-set-enum family
        // (`CaixaKind::as_str`, `PlacementStrategy::as_str`,
        // `RestartStrategy::as_str`, `RestartPolicy::as_str`, `DepList::as_str`,
        // `CaixaDialeto::as_str`).
        for (arm, expected) in [
            (InvariantKind::Safety, "safety"),
            (InvariantKind::Compliance, "compliance"),
            (InvariantKind::Hint, "hint"),
        ] {
            assert_eq!(
                arm.as_str(),
                expected,
                "InvariantKind::{arm:?}.as_str() must return the canonical \
                 lowercase tag {expected:?}",
            );
        }
    }

    #[test]
    fn invariant_kind_as_str_byte_equals_pre_lift_debug_lowercase_form() {
        // Fail-before-pass-after byte-parity pin on the substrate-canonical
        // [`InvariantKind::as_str`] `pub const fn` accessor vs the pre-lift
        // `format!("{:?}", v.kind).to_lowercase()` shape at
        // `caixa-feira/src/cmd/tofu.rs` — the sole production consumer
        // this lift retargets — for every arm in [`InvariantKind::ALL`].
        //
        // The pre-lift shape depended on the [`std::fmt::Debug`] derive's
        // per-arm byte-string output (Rust convention gives no stability
        // guarantee) plus a per-render `String` allocation from
        // [`str::to_lowercase`]; the post-lift `.as_str()` return is a
        // `&'static str` reached in one substrate-primitive dispatch. This
        // pin makes the two paths' byte-agreement load-bearing so a future
        // silent drift between them (a hand-rolled `impl Debug` that
        // pretty-prints the arm with per-arm context, an arm rename that
        // touches `Debug` but not `as_str`, or the reverse) trips at
        // caixa-arch build time rather than at the downstream `feira tofu`
        // consumer's silent tag drift.
        for arm in InvariantKind::ALL {
            let pre_lift = format!("{arm:?}").to_lowercase();
            assert_eq!(
                arm.as_str(),
                pre_lift,
                "InvariantKind::{arm:?}.as_str() must byte-equal the pre-lift \
                 format!(\"{{:?}}\", v.kind).to_lowercase() shape",
            );
        }
    }

    #[test]
    fn invariant_kind_display_and_as_ref_str_route_through_as_str_accessor() {
        // Three-path convergence pin: the paired [`std::fmt::Display`] impl,
        // the paired [`AsRef<str>`] impl, and the substrate-canonical
        // [`InvariantKind::as_str`] `pub const fn` accessor must resolve
        // to the same `&'static str` per arm.
        //
        // Guards against any future silent detour that routes one impl
        // through a divergent projection (a hand-rolled per-arm match in
        // the `fmt` body, an `impl AsRef<str>` swap onto a hypothetical
        // wire_name axis, a rename that touches one endpoint but not the
        // paired sibling) — the pin trips at caixa-arch test time rather
        // than at a downstream consumer's silent tag split. Peer of the
        // sibling
        // [`caixa_lint::diagnostic::tests::severity_as_ref_str_routes_through_as_str_accessor`]
        // (ce9d1e3) three-path convergence pin on the caixa-lint
        // `Severity` axis.
        for &arm in InvariantKind::ALL {
            let via_as_str: &str = arm.as_str();
            let via_as_ref: &str = arm.as_ref();
            let via_display = arm.to_string();
            assert_eq!(
                via_as_ref, via_as_str,
                "InvariantKind::{arm:?} AsRef<str>::as_ref() must byte-equal \
                 as_str()",
            );
            assert_eq!(
                via_display, via_as_str,
                "InvariantKind::{arm:?} Display::fmt() must byte-equal \
                 as_str()",
            );
        }
    }

    #[test]
    fn invariant_kind_from_wire_accepts_every_as_str_output() {
        // Fail-before-pass-after per-arm accept pin on the newly lifted
        // [`InvariantKind::from_wire`] reverse projection: every arm in
        // [`InvariantKind::ALL`] must parse back through `from_wire`
        // when fed its own [`InvariantKind::as_str`] output, landing on
        // `Some(same_variant)`. A regression that hand-rolled either
        // side's per-arm match without threading through the shared
        // three-string closed set would silently disagree on any future
        // arm rename (or a new arm the `iac-forge` policy-engine grows
        // — a `Warning` tier between `Compliance` and `Hint`, a `Fatal`
        // tier above `Safety`) and this pin flags it at caixa-arch build
        // time rather than at a downstream `feira arch --severity`
        // consumer's silent tag misclassification. Peer of the sibling
        // [`caixa_core::kind::tests::caixa_kind_wire_round_trips_through_from_wire`]
        // (2aa6d23), the caixa-core
        // `caixa_dialeto_from_wire_accepts_every_as_str_output` (d0e65ea),
        // `placement_strategy_from_wire_accepts_every_lifted_constant`
        // (18c7342), and `dep_list_round_trips_through_as_str_and_from_wire`
        // (45ee563) round-trip pins on the sibling closed-set typed-enum
        // reverse-projection axes.
        for &variant in InvariantKind::ALL {
            let wire = variant.as_str();
            let parsed = InvariantKind::from_wire(wire).unwrap_or_else(|| {
                panic!(
                    "InvariantKind::from_wire({wire:?}) must accept every \
                     InvariantKind::as_str output — got None for the wire \
                     byte-string of {variant:?}"
                )
            });
            assert_eq!(
                parsed, variant,
                "InvariantKind::from_wire(InvariantKind::{variant:?}.as_str()) \
                 must return InvariantKind::{variant:?} — the (as_str, \
                 from_wire) pair must form a total round-trip on the \
                 closed three-arm InvariantKind arm-set",
            );
        }
    }

    #[test]
    fn invariant_kind_from_wire_rejects_unknown_byte_strings() {
        // Rejection pin on the [`InvariantKind::from_wire`] parser's
        // accept-set: any string outside the three-arm
        // [`InvariantKind::as_str`] output set must return `None`. A
        // future accidental widening of the accept-set (a case-
        // insensitive match that accepts `"SAFETY"` / `"Safety"`, a
        // silent acceptance of the pre-lift PascalCase Debug-derived
        // shapes `"Safety"` / `"Compliance"` / `"Hint"` on the wire axis,
        // a Levenshtein-forgiving arm-lookup that admits `"safey"`
        // typos, a silent absorption of the sibling
        // [`caixa_lint::Severity::as_str`] four-arm accept-set
        // (`"error"` / `"warning"` / `"info"` / `"hint"`) — the two axes
        // share the `"hint"` byte-string but disagree everywhere else,
        // so accepting `"error"` on this axis would silently misclassify
        // a lint-severity-shaped byte-string as a caixa-arch invariant
        // severity) would silently drift the parser's accept-set from
        // the emitter's — a downstream audit-report re-loader that
        // bound a prior audit's [`Self::as_str`] output back to the
        // typed enum through this parser would then bind a malformed
        // byte-string to a plausibly-wrong typed arm the caller does
        // not route through any fallback, silently misclassifying the
        // reloaded row.
        //
        // Also rejects the sibling
        // [`caixa_lint::Severity::as_str`] axis's non-shared arms
        // (`"error"` / `"warning"` / `"info"`) which are distinct-axis
        // projections on a peer closed-set enum — the two axes' shared
        // `"hint"` arm is a coincidence of lowercase-tag choice, not a
        // typed cross-axis promise, and accepting `"error"` /
        // `"warning"` / `"info"` on this axis would silently split the
        // parser's accept-set from the emitter's arm-set. Peer of the
        // sibling
        // [`caixa_core::kind::tests::caixa_kind_from_wire_rejects_unknown_byte_strings`]
        // (2aa6d23),
        // `caixa_dialeto_from_wire_rejects_unknown_byte_strings`
        // (d0e65ea),
        // `placement_strategy_from_wire_rejects_unknown_byte_strings`
        // (18c7342), and
        // `dep_list_from_wire_returns_none_on_unknown_wire_scalar`
        // (45ee563) rejection pins on the sibling closed-set typed-enum
        // reverse-projection axes.
        for bad in [
            "",
            " ",
            "Safety",
            "SAFETY",
            "Compliance",
            "COMPLIANCE",
            "Hint",
            "HINT",
            "safey",
            "hnt",
            "warning",
            "error",
            "info",
            "fatal",
            "safety ",
            " safety",
            "safety\n",
            "safety\t",
        ] {
            assert!(
                InvariantKind::from_wire(bad).is_none(),
                "InvariantKind::from_wire({bad:?}) must return None — \
                 the parser's accept-set is exactly the three \
                 InvariantKind::as_str outputs; a widening would \
                 silently split the parser's accept-set from the \
                 emitter's arm-set",
            );
        }
    }

    #[test]
    fn invariant_kind_try_from_str_routes_through_from_wire_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl TryFrom<&str> for InvariantKind` — asserts the standard-
        // library trait impl and the substrate-primitive
        // [`super::InvariantKind::from_wire`] `Option<Self>` accessor
        // resolve to the same three-arm accept-set across every arm the
        // exhaustive [`super::InvariantKind::ALL`] slice enumerates. Any
        // future silent detour that routes the trait impl through a
        // divergent projection (a per-arm inline `match s { "safety" =>
        // Ok(Self::Safety), … }` re-inlining that opens a compile-time
        // link to the un-lifted arm-literal, a silent case-fold that
        // admits `"Safety"` / `"Compliance"` / `"Hint"` and would collide
        // the canonical-lowercase accept-set the emitter dispatches on)
        // trips at caixa-arch test time under `assert_eq!` rather than at
        // a downstream `impl TryFrom<&str>`-bound consumer's silent split.
        // Sweeps every one of the three arms
        // [`super::InvariantKind::ALL`] carries so no arm's projection is
        // covered only by the sibling method-named `from_wire` path.
        //
        // Peer of the sibling
        // [`caixa_core::kind::tests::caixa_kind_try_from_str_routes_through_from_wire_accessor`]
        // (3c83606),
        // [`caixa_core::dialeto::tests::caixa_dialeto_try_from_str_routes_through_from_wire_accessor`]
        // (bf33136),
        // `placement_strategy_try_from_str_routes_through_from_wire_accessor`
        // (6fd00cd),
        // `rate_limit_unit_try_from_str_routes_through_from_suffix_accessor`
        // (bf78400), and
        // `path_shape_violation_try_from_str_routes_through_from_wire_accessor`
        // (e67e48a) — extends the trait-idiomatic reverse-projection axis
        // onto the first outside-caixa-core closed-set fieldless typed
        // enum on the caixa surface (the caixa-arch invariant-severity
        // axis).
        for &variant in InvariantKind::ALL {
            let wire = variant.as_str();
            assert_eq!(
                <InvariantKind as TryFrom<&str>>::try_from(wire),
                Ok(variant),
                "TryFrom<&str> impl on InvariantKind must round-trip \
                 InvariantKind::{variant:?}.as_str() = {wire:?} back to \
                 Ok(InvariantKind::{variant:?}) — divergence from \
                 InvariantKind::from_wire signals a silent detour off the \
                 substrate-primitive accessor",
            );
            assert_eq!(
                <InvariantKind as TryFrom<&str>>::try_from(wire).ok(),
                InvariantKind::from_wire(wire),
                "TryFrom<&str> ok()-projection on {wire:?} must byte-equal \
                 InvariantKind::from_wire on the same input",
            );
        }
    }

    #[test]
    fn invariant_kind_try_from_str_rejects_unknown_byte_strings() {
        // Rejection witness on the `impl TryFrom<&str> for InvariantKind`
        // — sweeps a candidate set of byte-strings outside the three-arm
        // canonical-lowercase wire accept-set the sibling
        // [`super::InvariantKind::as_str`] emits and asserts every one
        // lands on `Err(())`, so a future accidental widening of the
        // trait impl's accept-set (a stray additional
        // `_ if s.eq_ignore_ascii_case("safety") => Ok(…)` case-fold
        // path, a silent acceptance of the pre-lift PascalCase Debug-
        // derived shapes `"Safety"` / `"Compliance"` / `"Hint"` on the
        // wire axis, a Levenshtein-forgiving arm-lookup that admits
        // `"safey"` typos — the exact form a
        // `format!("{:?}", …).to_lowercase()` round-trip on the paired
        // [`std::fmt::Debug`] derive would otherwise land on, the drift
        // footgun the emitter's documentation explicitly names as the
        // reason the substrate-canonical lowercase `"safety"` /
        // `"compliance"` / `"hint"` slug set exists) trips at caixa-arch
        // test time. The candidate set includes the empty string,
        // whitespace-only padding, uppercase rebrand candidates,
        // Levenshtein-neighbor typos, sibling closed-set-enum canonical
        // tags on the peer [`caixa_lint::Severity`] four-arm severity
        // axis (`"error"` / `"warning"` / `"info"`) — non-shared with
        // this axis's three-arm arch-severity set (the two axes' shared
        // `"hint"` arm is a coincidence of lowercase-tag choice, not a
        // typed cross-axis promise; accepting `"error"` / `"warning"` /
        // `"info"` on this axis would silently split the parser's
        // accept-set from the emitter's arm-set and misclassify a
        // lint-severity-shaped byte-string as a caixa-arch invariant
        // severity), and trailing/leading-whitespace-padded canonical
        // tags.
        //
        // Peer of the sibling
        // [`caixa_core::kind::tests::caixa_kind_try_from_str_rejects_unknown_byte_strings`]
        // (3c83606),
        // [`caixa_core::dialeto::tests::caixa_dialeto_try_from_str_rejects_unknown_byte_strings`]
        // (bf33136),
        // `rate_limit_unit_try_from_str_rejects_unknown_byte_strings`
        // (bf78400), and
        // `path_shape_violation_try_from_str_rejects_unknown_byte_strings`
        // (e67e48a) rejection pins on the sibling closed-set typed-enum
        // trait-idiomatic reverse-projection axes.
        for bad in [
            "",
            " ",
            "Safety",
            "SAFETY",
            "Compliance",
            "COMPLIANCE",
            "Hint",
            "HINT",
            "safey",
            "hnt",
            "warning",
            "error",
            "info",
            "fatal",
            "biblioteca",
            "servico",
            "one-for-one",
            "empty",
            "safety ",
            " safety",
            "safety\n",
            "safety\t",
        ] {
            assert_eq!(
                <InvariantKind as TryFrom<&str>>::try_from(bad),
                Err(()),
                "TryFrom<&str> for InvariantKind({bad:?}) must return \
                 Err(()) — the trait impl's accept-set is exactly the \
                 three InvariantKind::as_str outputs; a widening would \
                 silently split the trait impl's accept-set from the \
                 emitter's arm-set",
            );
        }
    }

    #[test]
    fn invariant_kind_try_from_str_and_from_wire_partition_the_accept_set() {
        // Cross-axis partition pin: the trait-idiomatic
        // [`TryFrom<&str>`] and the method-named
        // [`super::InvariantKind::from_wire`] projections must return
        // equivalent decisions on every input — the trait impl's `.ok()`
        // project-out from `Result<Self, ()>` and the method's
        // `Option<Self>` return must byte-equal each other on both
        // accepts and rejects. A future silent bifurcation (the trait
        // impl gaining a case-fold path the method does not carry, the
        // method gaining a synonym alias the trait impl does not honor)
        // trips at caixa-arch test time under a single pin rather than at
        // a downstream generic-bound consumer that dispatches through one
        // axis while a peer dispatches through the other. Sweeps both the
        // three-arm accept-set (via [`super::InvariantKind::ALL`]
        // threaded through [`super::InvariantKind::as_str`]) and a
        // canonical rejection sample so both halves of the partition are
        // covered.
        for &variant in InvariantKind::ALL {
            let wire = variant.as_str();
            assert_eq!(
                <InvariantKind as TryFrom<&str>>::try_from(wire).ok(),
                InvariantKind::from_wire(wire),
                "TryFrom<&str>::ok() and from_wire must agree on \
                 InvariantKind::{variant:?}.as_str() = {wire:?}",
            );
        }
        for bad in ["", "Safety", "unknown", "warning", "one-for-one"] {
            assert_eq!(
                <InvariantKind as TryFrom<&str>>::try_from(bad).ok(),
                InvariantKind::from_wire(bad),
                "TryFrom<&str>::ok() and from_wire must agree on the \
                 rejection outcome for {bad:?}",
            );
        }
    }

    #[test]
    fn invariant_kind_from_into_static_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<InvariantKind> for &'static str` — asserts the
        // standard-library trait impl and the substrate-primitive
        // [`super::InvariantKind::as_str`] `pub const fn` accessor
        // resolve to the same three-arm canonical-lowercase emit-set
        // across every arm the exhaustive [`super::InvariantKind::ALL`]
        // slice enumerates. Any future silent detour that routes the
        // trait impl through a divergent projection (a per-arm inline
        // `match kind { Safety => "safety", … }` re-inlining that opens
        // a compile-time link to the un-lifted arm-literal outside the
        // paired [`super::InvariantKind::as_str`] dispatch, a swap onto
        // a `format!("{:?}", …).to_lowercase()` round-trip through the
        // `#[derive(Debug)]` output whose stability is *not* guaranteed
        // and would silently reroute the diagnostic tag through a stale
        // byte-string with no downstream signal until an operator
        // scrolled the `feira tofu` terminal — the exact drift footgun
        // the sibling [`super::InvariantKind::as_str`] documentation
        // explicitly names) trips at caixa-arch test time under
        // `assert_eq!` rather than at a downstream `impl Into<&'static
        // str>`-bound consumer's silent split. Sweeps every one of the
        // three arms [`super::InvariantKind::ALL`] carries so no arm's
        // projection is covered only by the sibling method-named
        // `as_str` / [`std::fmt::Display`] / [`AsRef<str>`] paths.
        // Materializes the `<&'static str as
        // From<InvariantKind>>::from` output in three `const`-shape
        // bindings against the paired [`super::InvariantKind::as_str`]
        // `pub const fn` accessor to make the `'static` lifetime promise
        // a build-time invariant — a future accidental downgrade of any
        // of the three arms' inline canonical-lowercase byte-strings to
        // a non-`&'static str` (a `String::leak()`-produced return, a
        // `Box::leak`-cast, an intermediate lifetime-erasing helper)
        // trips at caixa-arch build time rather than at a downstream
        // `'static`-bound consumer.
        //
        // Peer of the sibling
        // [`caixa_core::supervisor::tests::restart_strategy_from_into_static_str_routes_through_as_str_accessor`]
        // (523157d),
        // [`caixa_core::supervisor::tests::restart_policy_from_into_static_str_routes_through_as_str_accessor`]
        // (9fb37d0),
        // [`caixa_core::kind::tests::caixa_kind_from_into_static_str_routes_through_as_str_accessor`]
        // (edb827b),
        // [`caixa_core::dialeto::tests::caixa_dialeto_from_into_static_str_routes_through_as_str_accessor`]
        // (c189a6f),
        // [`caixa_core::aplicacao::tests::placement_strategy_from_into_static_str_routes_through_as_str_accessor`]
        // (afa3562),
        // [`caixa_core::aplicacao::tests::wit_shape_from_into_static_str_routes_through_as_str_accessor`]
        // (56998ec),
        // [`caixa_core::aplicacao::tests::rate_limit_unit_from_into_static_str_routes_through_as_suffix_accessor`]
        // (7fdfbf4), and
        // [`caixa_core::render::tests::path_shape_violation_from_into_static_str_routes_through_as_str_accessor`]
        // (070a6de) pins on the sibling closed-set typed-enum forward-
        // projection axes — extends the trait-idiomatic forward-
        // projection axis onto the first outside-caixa-core closed-set
        // fieldless typed enum on the caixa surface (the caixa-arch
        // invariant-severity axis), opening the trait-idiomatic
        // forward-projection family on the outside-caixa-core surface.
        const SAFETY: &str = super::InvariantKind::Safety.as_str();
        const COMPLIANCE: &str = super::InvariantKind::Compliance.as_str();
        const HINT: &str = super::InvariantKind::Hint.as_str();
        for &variant in super::InvariantKind::ALL {
            let via_trait: &'static str =
                <&'static str as From<super::InvariantKind>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<InvariantKind> for &'static str impl must round-trip \
                 InvariantKind::{variant:?} to the same canonical-lowercase \
                 byte-string InvariantKind::as_str returns — divergence \
                 signals a silent detour off the substrate-primitive \
                 accessor"
            );
            let via_into: &'static str = variant.into();
            assert_eq!(
                via_into, via_method,
                "Into<&'static str>::into on InvariantKind::{variant:?} \
                 must byte-equal InvariantKind::as_str on the same input \
                 — the blanket-derived Into shape must resolve to the \
                 same as_str dispatch as the explicit From impl"
            );
        }
        assert_eq!(
            [SAFETY, COMPLIANCE, HINT],
            ["safety", "compliance", "hint"],
            "const-context InvariantKind::as_str must resolve to the \
             three canonical-lowercase byte-strings — a future accidental \
             downgrade of any arm to a non-const or non-static \
             byte-string breaks the `&'static str`-lifetime promise the \
             paired From<InvariantKind> for &'static str impl carries by \
             construction"
        );
    }

    #[test]
    fn invariant_kind_from_into_static_str_and_as_str_partition_the_emit_set() {
        // Cross-axis partition pin: the paired trait-idiomatic
        // `From<InvariantKind> for &'static str` forward projection and
        // the method-named [`super::InvariantKind::as_str`] forward
        // projection must resolve identically on *every* arm, not just
        // the ones named in the primary byte-parity pin above. Sweeps
        // every [`super::InvariantKind::ALL`] arm and asserts the
        // trait's `From::from` output byte-equals the method-named
        // accessor's return-value on each, locking the two forward-
        // projection paths together by construction so any future
        // detour (a stray `From` special-case that lands on a divergent
        // per-arm literal outside the paired `as_str` dispatch, a
        // hypothetical rebrand touching one axis without the other)
        // trips at caixa-arch test time.
        //
        // Peer of the sibling forward-projection partition pins
        // [`caixa_core::supervisor::tests::restart_strategy_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (523157d),
        // [`caixa_core::supervisor::tests::restart_policy_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (9fb37d0),
        // [`caixa_core::kind::tests::caixa_kind_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (edb827b),
        // [`caixa_core::dialeto::tests::caixa_dialeto_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (c189a6f),
        // [`caixa_core::aplicacao::tests::placement_strategy_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (afa3562),
        // [`caixa_core::aplicacao::tests::wit_shape_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (56998ec),
        // [`caixa_core::aplicacao::tests::rate_limit_unit_from_into_static_str_and_as_suffix_partition_the_emit_set`]
        // (7fdfbf4), and
        // [`caixa_core::render::tests::path_shape_violation_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (070a6de) — extends the round-trip discipline onto the first
        // outside-caixa-core closed-set fieldless typed enum on the
        // caixa surface, closing the two-way `Self ↔ &'static str`
        // round-trip on the trait-idiomatic pair (`From<Self> for
        // &'static str` + `TryFrom<&str> for Self`) as well as the
        // pre-existing method-named pair (`as_str` + `from_wire`).
        for &variant in super::InvariantKind::ALL {
            let via_trait: &'static str =
                <&'static str as From<super::InvariantKind>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<InvariantKind> for &'static str and \
                 InvariantKind::as_str must resolve identically on \
                 InvariantKind::{variant:?} — divergence signals the two \
                 forward-projection paths have drifted onto different \
                 emit-sets"
            );
        }
        // Round-trip witness: every arm's forward `From` output re-parses
        // through the paired trait-idiomatic reverse `TryFrom<&str>` back
        // to the original variant. Closes the two-way `InvariantKind ↔
        // &'static str` round-trip on the trait-idiomatic axis pair
        // directly (no wire-vocab intermediate — the emit-side
        // [`super::InvariantKind::as_str`] and the parse-side
        // [`super::InvariantKind::from_wire`] dispatch on the same three
        // inline canonical-lowercase byte-strings by construction),
        // mirroring the pre-existing method-named `as_str` + `from_wire`
        // round-trip on the substrate-primitive axis pair and the peer
        // [`super::InvariantKind`] two-halves lock pin
        // [`tests::invariant_kind_try_from_str_and_from_wire_partition_the_accept_set`]
        // on the sibling parse axis.
        for &variant in super::InvariantKind::ALL {
            let emitted: &'static str = variant.into();
            let re_parsed: Result<super::InvariantKind, ()> =
                <super::InvariantKind as TryFrom<&str>>::try_from(emitted);
            assert_eq!(
                re_parsed,
                Ok(variant),
                "trait-idiomatic axis pair must round-trip \
                 InvariantKind::{variant:?} through `.into::<&'static \
                 str>()` and back through `TryFrom<&str>` — a break \
                 signals the forward-emit and reverse-parse axes have \
                 drifted onto different vocabularies"
            );
        }
    }

    #[test]
    fn invariant_kind_from_borrowed_into_static_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<&InvariantKind> for &'static str` — asserts the
        // borrowed-input standard-library trait impl and the substrate-
        // primitive [`super::InvariantKind::as_str`] `pub const fn`
        // accessor resolve to the same three-arm canonical-lowercase
        // emit-set across every arm the exhaustive
        // [`super::InvariantKind::ALL`] slice enumerates. Rust's `From`
        // trait does not auto-derive the borrowed-input sibling from a
        // paired owned-input impl (no `impl<T, U> From<&T> for U where
        // T: Copy, U: From<T>` blanket in `core`), so the borrowed-
        // input axis is a distinct trait-idiomatic surface that a
        // `.iter().map(Into::into)` shape over
        // [`super::InvariantKind::ALL`] (whose iterator yields
        // `&InvariantKind`, not `InvariantKind`) reaches through this
        // impl and no other — the paired owned-input
        // [`From<InvariantKind>`] impl requires an explicit `.copied()`
        // / dereference before the trait fires. Materializes the
        // `<&'static str as From<&InvariantKind>>::from` output in a
        // `const`-shape binding against the paired
        // [`super::InvariantKind::as_str`] `pub const fn` accessor to
        // make the `'static` lifetime promise a build-time invariant —
        // a future accidental downgrade of any of the three arms'
        // inline canonical-lowercase byte-strings to a
        // non-`&'static str` (a `String::leak()`-produced return, a
        // `Box::leak`-cast, an intermediate lifetime-erasing helper)
        // trips at caixa-arch build time rather than at a downstream
        // `'static`-bound consumer.
        const SAFETY: &str = super::InvariantKind::Safety.as_str();
        const COMPLIANCE: &str = super::InvariantKind::Compliance.as_str();
        const HINT: &str = super::InvariantKind::Hint.as_str();
        for variant in super::InvariantKind::ALL {
            let via_trait: &'static str =
                <&'static str as From<&super::InvariantKind>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<&InvariantKind> for &'static str impl must \
                 round-trip &InvariantKind::{variant:?} to the same \
                 canonical-lowercase byte-string InvariantKind::as_str \
                 returns — divergence signals a silent detour off the \
                 substrate-primitive accessor"
            );
            let via_into: &'static str = variant.into();
            assert_eq!(
                via_into, via_method,
                "Into<&'static str>::into on &InvariantKind::{variant:?} \
                 must byte-equal InvariantKind::as_str on the same \
                 input — the blanket-derived Into shape must resolve to \
                 the same as_str dispatch as the explicit From impl"
            );
        }
        assert_eq!(
            [SAFETY, COMPLIANCE, HINT],
            ["safety", "compliance", "hint"],
            "const-context InvariantKind::as_str must resolve to the \
             three canonical-lowercase byte-strings — the borrowed-\
             input From<&InvariantKind> for &'static str impl inherits \
             its `'static` lifetime promise from the same accessor the \
             owned-input sibling routes through"
        );
    }

    #[test]
    fn invariant_kind_from_owned_and_borrowed_into_static_str_agree_on_every_arm() {
        // Cross-axis partition pin: the paired trait-idiomatic
        // owned-input `From<InvariantKind> for &'static str` and
        // borrowed-input `From<&InvariantKind> for &'static str` (this
        // lift) forward projections must resolve identically on every
        // arm, locking the two input-shape paths together so any future
        // detour (a stray borrowed-input special-case that lands on a
        // divergent per-arm literal outside the paired `as_str`
        // dispatch, a hypothetical rebrand touching one axis without
        // the other) trips at caixa-arch test time. Then a witness
        // that a `.iter().map(Into::into)` pipe over
        // [`super::InvariantKind::ALL`] (whose iterator yields
        // `&InvariantKind`) materializes the three-arm accept-set
        // through the borrowed-input axis alone — the exact shape a
        // future M4 admission-webhook rejection body composer, a future
        // substrate-wide per-arm diagnostic column, or a
        // `HashMap::<&'static str, super::InvariantKind>::from_iter(
        //     super::InvariantKind::ALL.iter().map(|k| (k.into(), *k)))`-
        // style per-severity lookup reaches through — closing the two-
        // way owned/borrowed input-shape symmetry on the forward-
        // projection trait-idiomatic axis. Peer of the sibling
        // [`caixa_core::kind::tests::caixa_kind_from_owned_and_borrowed_into_static_str_agree_on_every_arm`]
        // (edbb27b campaign-shape) partition pin on the structurally
        // most fundamental closed-set typed enum on the caixa surface —
        // extends the borrowed-input axis discipline onto the first
        // outside-`caixa-core` closed-set fieldless typed enum on the
        // caixa surface, the caixa-arch invariant-severity axis.
        for &variant in super::InvariantKind::ALL {
            let owned: &'static str = <&'static str as From<super::InvariantKind>>::from(variant);
            let borrowed: &'static str =
                <&'static str as From<&super::InvariantKind>>::from(&variant);
            assert_eq!(
                owned, borrowed,
                "From<InvariantKind> and From<&InvariantKind> for \
                 &'static str must resolve identically on \
                 InvariantKind::{variant:?} — divergence signals the \
                 owned-input and borrowed-input forward-projection \
                 paths have drifted onto different emit-sets"
            );
        }
        let via_iter: Vec<&'static str> =
            super::InvariantKind::ALL.iter().map(Into::into).collect();
        let via_method: Vec<&'static str> = super::InvariantKind::ALL
            .iter()
            .map(|k| k.as_str())
            .collect();
        assert_eq!(
            via_iter, via_method,
            "`.iter().map(Into::into)` over InvariantKind::ALL must \
             byte-equal `.iter().map(|k| k.as_str())` on every arm — \
             the borrowed-input `From<&InvariantKind> for &'static str` \
             axis is what makes the `.iter().map(Into::into)` shape \
             route through the substrate-primitive \
             `InvariantKind::as_str` accessor rather than through a \
             per-call-site `.copied()` / dereference detour"
        );
    }

    #[test]
    fn invariant_kind_from_into_owned_string_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<InvariantKind> for String` — asserts the owned-
        // `String`-returning standard-library trait impl and the
        // substrate-primitive [`super::InvariantKind::as_str`] `pub
        // const fn` accessor resolve to the same three-arm canonical-
        // lowercase emit-set across every arm the exhaustive
        // [`super::InvariantKind::ALL`] slice enumerates. Rust's
        // standard library does not carry a blanket
        // `impl<T: AsRef<str>> From<T> for String`, so the owned-
        // `String` axis is a distinct trait-idiomatic surface that a
        // `let key: String = kind.into();`-shaped downstream call
        // site reaches through this impl and no other — the sibling
        // `&'static str`-returning axes force an explicit
        // `.to_owned()` / [`String::from`] restatement whose type
        // bounds have no compile-time link to the substrate primitive.
        // Sweeps every one of the three arms
        // [`super::InvariantKind::ALL`] carries so no arm's projection
        // is covered only by the sibling method-named `as_str` /
        // [`std::fmt::Display`] / [`AsRef<str>`] / owned-input
        // `&'static str`-returning paths.
        //
        // Peer of the sibling
        // [`caixa_core::render::tests::path_shape_violation_from_into_owned_string_routes_through_as_str_accessor`]
        // (6e0479a — first render-side arm on the owned-`String` axis)
        // and the eight prior owned-`String` axis pins — extends the
        // trait-idiomatic owned-`String`-returning forward-projection
        // family onto the first outside-`caixa-core` closed-set
        // fieldless typed enum on the caixa surface, the caixa-arch
        // invariant-severity axis.
        for &variant in super::InvariantKind::ALL {
            let via_trait: String = <String as From<super::InvariantKind>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait.as_str(),
                via_method,
                "From<InvariantKind> for String impl must round-trip \
                 InvariantKind::{variant:?} to the same canonical-\
                 lowercase byte-string InvariantKind::as_str returns — \
                 divergence signals a silent detour off the substrate-\
                 primitive accessor"
            );
            let via_into: String = variant.into();
            assert_eq!(
                via_into.as_str(),
                via_method,
                "Into<String>::into on InvariantKind::{variant:?} must \
                 byte-equal InvariantKind::as_str on the same input — \
                 the blanket-derived Into shape must resolve to the \
                 same as_str dispatch as the explicit From impl"
            );
        }
    }

    #[test]
    fn invariant_kind_from_into_owned_string_and_static_str_agree_on_every_arm() {
        // Cross-axis partition pin: the paired trait-idiomatic owned-
        // input `&'static str`-returning `From<InvariantKind> for
        // &'static str` and owned-`String`-returning
        // `From<InvariantKind> for String` (this lift) forward
        // projections must resolve identically on every arm, locking
        // the two output-shape paths together so any future detour (a
        // stray owned-`String` special-case that lands on a divergent
        // per-arm literal outside the paired `as_str` dispatch, a
        // hypothetical rebrand touching one axis without the other)
        // trips at caixa-arch test time. Then a witness that the
        // `ToString::to_string`-through-[`std::fmt::Display`] surface
        // (`variant.to_string()`) byte-equals the trait-idiomatic
        // owned-`String` axis (`String::from(variant)`) on every arm,
        // so a future consumer that reaches for `.to_string()` and
        // one that reaches for `.into::<String>()` land on the same
        // substrate-primitive vocabulary. Plus a
        // `.iter().copied().map(String::from)` pipe witness over
        // [`super::InvariantKind::ALL`] — the exact shape a future
        // per-severity histogram key materializer or admission-webhook
        // rejection body composer reaches through — materializes the
        // three-arm accept-set through the owned-`String` axis alone.
        // Plus a direct `Self → String → Self` round-trip witness
        // through the paired [`TryFrom<&str>`] axis on the owned-
        // `String`'s [`String::as_str`] borrow, closing the two-way
        // round-trip on the owned-`String` axis directly (no wire-
        // vocab intermediate — [`super::InvariantKind::as_str`] and
        // [`super::InvariantKind::from_wire`] dispatch on the same
        // three inline canonical-lowercase byte-strings by
        // construction).
        for &variant in super::InvariantKind::ALL {
            let owned_string: String = <String as From<super::InvariantKind>>::from(variant);
            let owned_static: &'static str =
                <&'static str as From<super::InvariantKind>>::from(variant);
            assert_eq!(
                owned_string.as_str(),
                owned_static,
                "From<InvariantKind> for String and From<InvariantKind> \
                 for &'static str must resolve identically on \
                 InvariantKind::{variant:?} — divergence signals the \
                 two output-shape forward-projection paths have drifted \
                 onto different emit-sets"
            );
            let via_display: String = variant.to_string();
            assert_eq!(
                owned_string, via_display,
                "From<InvariantKind> for String and ToString::to_string \
                 via Display must resolve identically on \
                 InvariantKind::{variant:?} — divergence signals the \
                 trait-idiomatic owned-`String` axis and the Display-\
                 routed ToString axis have drifted onto different \
                 vocabularies"
            );
        }
        let via_iter: Vec<String> = super::InvariantKind::ALL
            .iter()
            .copied()
            .map(String::from)
            .collect();
        let via_method: Vec<String> = super::InvariantKind::ALL
            .iter()
            .map(|k| k.as_str().to_owned())
            .collect();
        assert_eq!(
            via_iter, via_method,
            "`.iter().copied().map(String::from)` over \
             InvariantKind::ALL must byte-equal \
             `.iter().map(|k| k.as_str().to_owned())` on every arm — \
             the owned-`String` `From<InvariantKind> for String` axis \
             is what makes the `.map(String::from)` shape route \
             through the substrate-primitive `InvariantKind::as_str` \
             accessor rather than through a per-call-site `.to_owned()` \
             / `String::from(kind.as_str())` detour"
        );
        for &variant in super::InvariantKind::ALL {
            let emitted: String = variant.into();
            let re_parsed: Result<super::InvariantKind, ()> =
                <super::InvariantKind as TryFrom<&str>>::try_from(emitted.as_str());
            assert_eq!(
                re_parsed,
                Ok(variant),
                "trait-idiomatic owned-`String` axis pair must round-\
                 trip InvariantKind::{variant:?} through \
                 `.into::<String>()` and back through \
                 `TryFrom<&str>` on the owned-`String`'s \
                 `String::as_str` borrow — a break signals the \
                 forward-emit owned-`String` axis and the reverse-\
                 parse `TryFrom<&str>` axis have drifted onto \
                 different vocabularies"
            );
        }
    }

    #[test]
    fn invariant_kind_from_into_borrowed_owned_string_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<&InvariantKind> for String` — asserts the
        // borrowed-input owned-`String`-returning standard-library
        // trait impl and the substrate-primitive
        // [`super::InvariantKind::as_str`] `pub const fn` accessor
        // resolve to the same three-arm canonical-lowercase emit-set
        // across every arm the exhaustive [`super::InvariantKind::ALL`]
        // slice enumerates. Rust's standard library does not carry a
        // blanket `impl<T: AsRef<str>> From<&T> for String` (nor an
        // `impl<T: fmt::Display> From<&T> for String`), so the
        // borrowed-input owned-`String` forward-projection axis is a
        // distinct trait-idiomatic surface that a
        // `let key: String = (&kind).into();`-shaped call site reaches
        // through this impl and no other — the paired sibling
        // `From<InvariantKind> for String` impl forces every borrowed-
        // input call site through an explicit `Copy` deref
        // (`String::from(*kind)`) or an `.as_str().to_owned()` /
        // `.to_string()` detour whose type bounds have no compile-time
        // link to the substrate primitive.
        //
        // Peer of the first-mover
        // [`caixa_core::supervisor::tests::restart_strategy_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (579385f), the second-peer
        // [`caixa_core::supervisor::tests::restart_policy_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (8465740), the third-peer
        // [`caixa_core::dep::tests::dep_list_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (e0cb617), the fourth-peer
        // [`caixa_core::kind::tests::caixa_kind_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (e76436d), the fifth-peer
        // [`caixa_core::dialeto::tests::caixa_dialeto_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (d3c0d1d), the sixth-peer
        // [`caixa_core::aplicacao::tests::placement_strategy_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (d3dc000), the seventh-peer
        // [`caixa_core::aplicacao::tests::wit_shape_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (d638fd3), the eighth-peer
        // [`caixa_core::aplicacao::tests::rate_limit_unit_from_into_borrowed_owned_string_routes_through_as_suffix_accessor`]
        // (6424e45), the ninth-peer
        // [`caixa_core::render::tests::path_shape_violation_from_into_borrowed_owned_string_routes_through_as_str_accessor`]
        // (b90e193 — first outside-manifest-surface arm) pins on the
        // sibling closed-set typed-enum borrowed-input owned-`String`
        // forward-projection axes — closes the whole
        // `{Self, &Self} × {&'static str, String}` 2×2 trait-
        // idiomatic projection corner on the first outside-`caixa-
        // core` closed-set fieldless typed enum on the caixa surface
        // (the caixa-arch invariant-severity three-arm axis every
        // `feira arch` / `feira tofu` per-violation render site
        // dispatches through), on the same trajectory the paired
        // owned-input owned-`String` axis lift (1afd8d5) and the
        // paired borrowed-input owned-`&'static str` axis lift
        // (238d886) already took onto the same enum.
        for &variant in super::InvariantKind::ALL {
            let via_trait: String = <String as From<&super::InvariantKind>>::from(&variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait.as_str(),
                via_method,
                "From<&InvariantKind> for String impl must round-trip \
                 &InvariantKind::{variant:?} to the same canonical-\
                 lowercase byte-string InvariantKind::as_str returns — \
                 divergence signals a silent detour off the substrate-\
                 primitive accessor"
            );
            let via_into: String = (&variant).into();
            assert_eq!(
                via_into.as_str(),
                via_method,
                "Into<String>::into on &InvariantKind::{variant:?} \
                 must byte-equal InvariantKind::as_str on the same \
                 input — the blanket-derived Into shape must resolve \
                 to the same as_str dispatch as the explicit From impl"
            );
        }
    }

    #[test]
    fn invariant_kind_from_into_borrowed_owned_string_agrees_with_paired_axes_on_every_arm() {
        // Cross-axis partition pin: the newly lifted trait-idiomatic
        // borrowed-input owned-`String`
        // `From<&InvariantKind> for String` (this lift), the paired
        // owned-input owned-`String`
        // `From<InvariantKind> for String` (1afd8d5), the paired
        // borrowed-input owned-`&'static str`
        // `From<&InvariantKind> for &'static str` (238d886), and the
        // paired owned-input owned-`&'static str`
        // `From<InvariantKind> for &'static str` (070a6de) — every
        // corner of the `{Self, &Self} × {&'static str, String}` 2×2
        // trait-idiomatic projection family — must resolve identically
        // on every arm, locking the four return-shape × input-shape
        // paths together so any future detour trips at caixa-arch test
        // time. Also byte-parity witness against the sibling
        // [`ToString::to_string`] surface routed through
        // [`std::fmt::Display`] and a direct round-trip witness through
        // the paired trait-idiomatic reverse [`TryFrom<&str>`] axis on
        // the owned-`String`'s [`String::as_str`] borrow that closes
        // the two-way `&Self → String → Self` round-trip on the
        // trait-idiomatic borrowed-input owned-`String` forward +
        // reverse axis pair.
        for &variant in super::InvariantKind::ALL {
            let borrowed_string: String = <String as From<&super::InvariantKind>>::from(&variant);
            let owned_string: String = <String as From<super::InvariantKind>>::from(variant);
            let borrowed_static: &'static str =
                <&'static str as From<&super::InvariantKind>>::from(&variant);
            let owned_static: &'static str =
                <&'static str as From<super::InvariantKind>>::from(variant);
            assert_eq!(
                borrowed_string, owned_string,
                "From<&InvariantKind> for String and From<InvariantKind> \
                 for String must resolve identically on \
                 InvariantKind::{variant:?} — divergence signals the \
                 owned-`String` axis pair's borrowed-input and owned-\
                 input arms have drifted onto different emit-sets"
            );
            assert_eq!(
                borrowed_string.as_str(),
                borrowed_static,
                "From<&InvariantKind> for String and From<&InvariantKind> \
                 for &'static str must resolve identically on \
                 InvariantKind::{variant:?} — divergence signals the \
                 borrowed-input axis pair's two output-shape arms have \
                 drifted onto different emit-sets"
            );
            assert_eq!(
                borrowed_string.as_str(),
                owned_static,
                "From<&InvariantKind> for String and From<InvariantKind> \
                 for &'static str must resolve identically on \
                 InvariantKind::{variant:?} — cross-diagonal of the \
                 2×2 must agree, locking the four corners onto a \
                 single substrate-primitive emit-set"
            );
            let via_display: String = variant.to_string();
            assert_eq!(
                borrowed_string, via_display,
                "From<&InvariantKind> for String and ToString::to_string \
                 via Display must resolve identically on \
                 InvariantKind::{variant:?} — divergence signals the \
                 trait-idiomatic borrowed-input owned-`String` axis \
                 and the Display-routed ToString axis have drifted \
                 onto different vocabularies"
            );
        }
        let via_iter: Vec<String> = super::InvariantKind::ALL.iter().map(String::from).collect();
        let via_method: Vec<String> = super::InvariantKind::ALL
            .iter()
            .map(|k| k.as_str().to_owned())
            .collect();
        assert_eq!(
            via_iter, via_method,
            "`.iter().map(String::from)` over InvariantKind::ALL must \
             byte-equal `.iter().map(|k| k.as_str().to_owned())` on \
             every arm — the borrowed-input owned-`String` \
             `From<&InvariantKind> for String` axis is what makes the \
             `.iter().map(String::from)` shape route through the \
             substrate-primitive `InvariantKind::as_str` accessor \
             (whose iterator yields `&InvariantKind` by construction) \
             rather than through a per-call-site `.copied()` / \
             spurious `Copy` deref detour"
        );
        for &variant in super::InvariantKind::ALL {
            let emitted: String = (&variant).into();
            let re_parsed: Result<super::InvariantKind, ()> =
                <super::InvariantKind as TryFrom<&str>>::try_from(emitted.as_str());
            assert_eq!(
                re_parsed,
                Ok(variant),
                "trait-idiomatic borrowed-input owned-`String` axis \
                 pair must round-trip InvariantKind::{variant:?} \
                 through `(&variant).into::<String>()` and back \
                 through `TryFrom<&str>` on the owned-`String`'s \
                 `String::as_str` borrow — a break signals the \
                 forward-emit borrowed-input owned-`String` axis and \
                 the reverse-parse `TryFrom<&str>` axis have drifted \
                 onto different vocabularies"
            );
        }
    }
}
