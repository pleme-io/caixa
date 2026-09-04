//! `defcaixa` is spoken by two unrelated declarations. This module makes that
//! a **typed fact** instead of an anonymous parse failure.
//!
//! # The finding
//!
//! Measured 2026-07-31 over the pleme-io org checkout (270 `caixa.lisp` /
//! `*.caixa.lisp` files found with `rg --no-ignore`; a bare `rg` from the org
//! root returns 0, which is how this stayed invisible), the corpus splits into
//! two schemas that share zero required slots:
//!
//! * [`CaixaDialeto::Pacote`] — this crate's [`crate::Caixa`]. `:nome
//!   :versao :kind :deps :bibliotecas :exe :servicos` + the supervisor/mesh
//!   slots. It declares a **tatara-lisp package**: the thing `feira` resolves,
//!   builds, links and publishes.
//! * [`CaixaDialeto::Molde`] — `:name :kind :ecosystem :package {…} :workflows
//!   […] :ci-config {…} :files […]`. It declares a **repo's generated
//!   surface**: which foreign ecosystem (rust / go / python / …), that
//!   ecosystem's own package metadata, the CI shims to emit, and byte-captured
//!   file bodies. Read by `pleme-doc-gen`, never by `feira`.
//!
//! `:package`, `:ecosystem`, `:supports` and `:profile` have no counterpart in
//! [`crate::Caixa`] at all — the theory doc's own D4 note records the same
//! thing: those manifests "are authored against a schema that does not exist in
//! Rust". They are not two spellings of one declaration. They are two domains
//! that collided on one word, because *caixa* names a box and both are boxes.
//!
//! # Why this is not a bug report about broken files
//!
//! The Molde-dialect files are not malformed. They are correct inputs to their
//! own consumer, and nothing in the shipped `feira` reads them, so nothing is
//! failing today. The hazard is **latent and certain**: any new declarative
//! surface written against "a `.caixa.lisp` is a [`crate::Caixa`]" meets a
//! corpus where that is false for the large majority of files, and gets a flat
//! unknown-keyword rejection that reads as "this manifest is broken" rather
//! than "this manifest is not yours".
//!
//! # What this module does about it
//!
//! [`classify`] is total: every `(defcaixa …)` form lands in exactly one
//! [`CaixaDialeto`], including [`CaixaDialeto::Desconhecido`] for one that
//! matches neither. [`crate::Caixa::from_lisp`] runs it first, so a foreign
//! dialect is [`crate::ManifestError::DialetoEstrangeiro`] — an error that
//! names the dialect it found and the consumer that speaks it — rather than an
//! unknown-kwarg error indistinguishable from a typo.
//!
//! Tier-honest: this is **parse-time rejection with a named cause**, not
//! unrepresentability. A caller that ignores the `Err` still gets nothing
//! useful; what it can no longer do is mistake "wrong dialect" for "bad file".

use tatara_lisp::{Atom, Sexp};

/// Which `(defcaixa …)` declaration a source speaks.
///
/// The [`gen_platform::IsVariant`] derive emits per-arm arm-discriminator
/// predicates (`is_pacote` / `is_molde` / `is_molde_posicional` /
/// `is_desconhecido`) as substrate-side typed dispatches on the closed
/// four-arm dialect-classification discriminator. Peer of the sibling
/// closed-set fieldless typed enums' [`crate::CaixaKind`] /
/// [`crate::supervisor::RestartStrategy`] /
/// [`crate::supervisor::RestartPolicy`] /
/// [`crate::aplicacao::PlacementStrategy`] /
/// [`crate::aplicacao::RateLimitUnit`] /
/// [`crate::dep::DepList`] `IsVariant` derives on the sibling
/// closed-set typed-enum discriminator axes.
///
/// The pre-lift `is_molde_family` predicate hand-rolled its own
/// `matches!(self, Self::Molde | Self::MoldePosicional)` two-arm literal
/// with no compile-time link back to the closed set — post-lift it routes
/// through `self.is_molde() || self.is_molde_posicional()` so a future
/// arm rename (e.g. `Molde → MoldeKW` under an M4 vocabulary shift) trips
/// exhaustively at every derive-generated predicate site rather than
/// leaving the hand-rolled `matches!` silently drifting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, gen_platform::IsVariant)]
pub enum CaixaDialeto {
    /// This crate's [`crate::Caixa`] — a tatara-lisp package manifest.
    /// Keyword-argument form headed by `:nome`.
    Pacote,
    /// `pleme-doc-gen`'s repo-surface declaration, keyword-argument form
    /// headed by `:name` (plus `:ecosystem` / `:package`).
    Molde,
    /// The same declaration as [`Self::Molde`], written with the package name
    /// as a bare positional symbol — `(defcaixa todoku-go :kind :Biblioteca
    /// :ecosystem :go …)`. `pleme-doc-gen`'s parser reads the first token
    /// after the head as the name, so this is one arity of one declaration,
    /// not a third schema.
    MoldePosicional,
    /// A `(defcaixa …)` form matching neither. Kept as a variant rather than
    /// an error so [`classify`] is total and a census can COUNT the residue —
    /// a classifier that threw here would report "0 unknown" by construction.
    Desconhecido,
}

impl CaixaDialeto {
    /// Exhaustive iteration surface for every consumer that walks the
    /// closed four-arm [`CaixaDialeto`] discriminator set — the
    /// [`feira dialeto`](../../caixa_feira/cmd/dialeto/index.html)
    /// census counter's per-arm accept-set, a future
    /// `feira dialeto --list-dialects` CLI listing of the accepted
    /// classifications, a future M4 `mesh.pleme.io/v1alpha1/Manifesto`
    /// CR materializer's admission-webhook rejection body naming the
    /// accepted-dialect set, any future census-report shape probe that
    /// sweeps every arm to compute per-arm coverage. A future arm
    /// addition (a fifth dialect the [`crate::dialeto`] module doc's
    /// "third dialect" hazard actualises — the module explicitly frames
    /// its purpose as "what stops a third dialect appearing", and this
    /// slice is the substrate-side answer: the arm-set is one edit and
    /// every consumer picks up the new entry by construction) extends
    /// this slice as one edit and every downstream consumer picks up
    /// the new entry through the shared iteration; the compiler-checked
    /// exhaustiveness on the sibling method `match` arms
    /// ([`Self::palavra_canonica`] / [`Self::consumidor`] /
    /// [`Self::descricao`] / [`std::fmt::Display`]) is the build-time
    /// guarantee that no arm forgets to grow.
    ///
    /// Peer of the sibling closed-set typed enums'
    /// [`crate::CaixaKind::ALL`] (6b1f4fb) /
    /// [`crate::aplicacao::PlacementStrategy::ALL`] (18c7342) /
    /// [`crate::aplicacao::RateLimitUnit::ALL`] (6bce03d) /
    /// [`crate::dep::DepList::ALL`] (45ee563) /
    /// [`crate::supervisor::RestartStrategy::ALL`] (4eec29c) /
    /// [`crate::supervisor::RestartPolicy::ALL`] (dd32ccf)
    /// exhaustive-iteration surfaces — the seventh closed-set typed
    /// enum on the caixa surface to converge onto the same
    /// one-canonical-arm-list-per-enum discipline, and the first
    /// dialect-classification axis (as distinct from an OTP-shape M2
    /// slot or an M3 mesh slot) to reach it. Order matches variant
    /// declaration order verbatim (`Pacote` → `Molde` →
    /// `MoldePosicional` → `Desconhecido`) so the slice is the
    /// canonical ordering every listing / rendering consumer defers to.
    pub const ALL: &'static [Self] = &[
        Self::Pacote,
        Self::Molde,
        Self::MoldePosicional,
        Self::Desconhecido,
    ];

    /// Substrate-canonical `PascalCase` variant-name byte-string every consumer
    /// that formats the dialect as census-facing text lands on. Returns the
    /// per-arm `PascalCase` name of the variant (`"Pacote"` / `"Molde"` /
    /// `"MoldePosicional"` / `"Desconhecido"`) — the one canonical
    /// byte-string the paired [`std::fmt::Display`] impl routes through so
    /// every downstream consumer (the `feira dialeto` census counter output
    /// line, a future `feira dialeto --list-dialects` CLI enumeration, a
    /// future M4 `mesh.pleme.io/v1alpha1/Manifesto` CR materializer's
    /// admission-webhook rejection body naming the accepted-dialect set)
    /// reaches for the same substrate primitive rather than the pre-lift
    /// hand-rolled four-arm literal-string match every [`std::fmt::Display`]
    /// call previously routed through in place.
    ///
    /// Peer of the sibling closed-set typed enums'
    /// [`crate::CaixaKind::as_str`] / [`crate::supervisor::RestartStrategy::as_str`]
    /// / [`crate::supervisor::RestartPolicy::as_str`] /
    /// [`crate::aplicacao::PlacementStrategy::as_str`] /
    /// [`crate::dep::DepList::as_str`] projections on the sibling closed-set
    /// typed-enum discriminator axes — the seventh (and last unlifted)
    /// closed-set fieldless typed enum on the caixa surface to converge
    /// onto the same one-canonical-byte-string-per-arm-through-`as_str`
    /// discipline the six siblings already carry. Unlike [`crate::CaixaKind`]
    /// (which carries two axes: `as_str` returning lowercase Portuguese
    /// diagnostic form vs `wire_name` returning `PascalCase` tatara-lisp
    /// author-surface bytes), [`CaixaDialeto`] is an internal
    /// classification with no wire surface — the `PascalCase` variant name
    /// is the census-facing form every consumer reads, so `as_str`
    /// suffices without a paired `wire_name` axis.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pacote => "Pacote",
            Self::Molde => "Molde",
            Self::MoldePosicional => "MoldePosicional",
            Self::Desconhecido => "Desconhecido",
        }
    }

    /// Substrate-canonical reverse projection on the [`CaixaDialeto`]
    /// closed-set dialect-classification axis — parses the `PascalCase`
    /// variant-name byte-string back to the typed variant, or `None` when
    /// `s` is outside the closed-set arm-string set [`Self::as_str`]
    /// emits. Walks the same four `"Pacote"` / `"Molde"` /
    /// `"MoldePosicional"` / `"Desconhecido"` byte-strings the sibling
    /// [`Self::as_str`] emitter returns, so the parse and emit halves of
    /// the round-trip migrate through one caixa-core edit on any future
    /// arm addition (the module doc's "third dialect" hazard actualising
    /// as a fifth arm) — the compiler-checked exhaustiveness on
    /// [`Self::as_str`]'s `match self` arms and the round-trip pin
    /// [`tests::caixa_dialeto_round_trips_through_as_str_and_from_wire`]
    /// together lock the two halves mutually.
    ///
    /// Prior to this lift the substrate carried only the forward
    /// `Self → &str` projection on the dialect-classification axis (the
    /// [`Self::as_str`] emitter, the [`std::fmt::Display`] impl routed
    /// through it, the [`AsRef<str>`] impl routed through it) — every
    /// future consumer that wanted to promote the census-facing text back
    /// to the typed enum (a future `feira dialeto --filter
    /// <Pacote|Molde|MoldePosicional|Desconhecido>` CLI arg-parse that
    /// binds the wire form into the typed enum before dispatching to the
    /// per-arm counter, a future M4 `mesh.pleme.io/v1alpha1/Manifesto`
    /// CR materializer's admission-time re-parse of the per-dialect
    /// audit body, a future audit-report re-loader that binds a prior
    /// [`Self::as_str`] output back to the typed enum for cross-run
    /// comparison) would have had to re-inline a four-arm `match s`
    /// cascade that expressed no compile-time link back to the typed
    /// [`CaixaDialeto`] enum.
    ///
    /// Same closed-set-reverse-projection discipline the sibling
    /// [`crate::CaixaKind::from_wire`] (2aa6d23) /
    /// [`crate::supervisor::RestartStrategy::from_wire`] (4eec29c) /
    /// [`crate::supervisor::RestartPolicy::from_wire`] (dd32ccf) /
    /// [`crate::aplicacao::PlacementStrategy::from_wire`] (18c7342) /
    /// [`crate::dep::DepList::from_wire`] (45ee563) typed enums carry on
    /// the peer wire-side `str → Self` axes — extends the family onto
    /// the seventh closed-set fieldless typed enum on the caixa surface
    /// (the dialect-classification axis), matching the same
    /// two-way `str ↔ Self` round-trip every sibling closed-set enum
    /// already carries. Method-named `from_wire` (not `from_str`) to
    /// match the peer shapes verbatim and side-step the derived
    /// [`std::str::FromStr`] impls the sibling
    /// [`gen_platform::FromStrKind`]-carrying axes install on their
    /// kebab-case dispatcher-catalog identity. Returns `Option<Self>`
    /// (rather than `Result<Self, _>`) to match the peer shapes: the
    /// caller picks the diagnostic form appropriate for its use site.
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "Pacote" => Some(Self::Pacote),
            "Molde" => Some(Self::Molde),
            "MoldePosicional" => Some(Self::MoldePosicional),
            "Desconhecido" => Some(Self::Desconhecido),
            _ => None,
        }
    }

    /// The keyword an author should write for this dialect, once the
    /// migration named in [`Self::consumidor`] completes.
    #[must_use]
    pub const fn palavra_canonica(self) -> &'static str {
        match self {
            Self::Pacote => "defcaixa",
            Self::Molde | Self::MoldePosicional => "defmolde",
            Self::Desconhecido => "?",
        }
    }

    /// Who reads this dialect.
    #[must_use]
    pub const fn consumidor(self) -> &'static str {
        match self {
            Self::Pacote => "caixa-core / feira",
            Self::Molde | Self::MoldePosicional => "pleme-doc-gen",
            Self::Desconhecido => "nobody known",
        }
    }

    /// A one-line description for a census row or an error message.
    #[must_use]
    pub const fn descricao(self) -> &'static str {
        match self {
            Self::Pacote => "tatara-lisp package manifest (:nome :versao :kind :deps …)",
            Self::Molde => "repo-surface declaration (:name :ecosystem :package {…} …)",
            Self::MoldePosicional => {
                "repo-surface declaration, positional name (defcaixa <nome> :kind …)"
            }
            Self::Desconhecido => "unrecognised — matches no known defcaixa schema",
        }
    }

    /// True when this arm belongs to the `defmolde` declaration family —
    /// the two-arity closure of [`Self::Molde`] and [`Self::MoldePosicional`]
    /// under the shared `defmolde` head keyword the sibling
    /// [`Self::palavra_canonica`] projection already collapses onto
    /// `"defmolde"` for both arms (and the sibling [`Self::consumidor`]
    /// projection collapses onto `"pleme-doc-gen"` for the same two arms).
    /// False on [`Self::Pacote`] (the sibling `defcaixa` tatara-lisp
    /// package manifest, [`Self::palavra_canonica`] `→ "defcaixa"`) and
    /// on [`Self::Desconhecido`] (the residue that names no known
    /// declaration, [`Self::palavra_canonica`] `→ "?"`).
    ///
    /// The [`Self::Molde`] / [`Self::MoldePosicional`] split is one
    /// declaration written two ways ([`Self::MoldePosicional`]'s
    /// variant-declaration docstring at [`Self::MoldePosicional`] frames
    /// it exactly: "the same declaration as [`Self::Molde`], written with
    /// the package name as a bare positional symbol … this is one arity
    /// of one declaration, not a third schema"). Every downstream gate
    /// that keys off "does this dialect belong to the `defmolde` family"
    /// (as distinct from the four-arm-per-arm census-counter axis the
    /// sibling `feira dialeto` verb already fans on separately at
    /// `caixa-feira/src/cmd/dialeto.rs:110-127`) previously hand-rolled
    /// the two-arm collapse inline as `matches!(d, CaixaDialeto::Molde |
    /// CaixaDialeto::MoldePosicional)` — a compile-time-anonymous
    /// two-arm literal set with no link back to the [`CaixaDialeto`]
    /// variant declaration nor to the sibling
    /// [`Self::palavra_canonica`] / [`Self::consumidor`] projections
    /// that already carry the same two-arm collapse under the shared
    /// `defmolde` / `pleme-doc-gen` axis. The `feira dialeto` verb's
    /// [`caixa-feira/src/cmd/dialeto.rs`] carried the same
    /// `matches!` twice — once in the `--strict-palavra` gate that
    /// refuses a repo-surface declaration still written as
    /// `(defcaixa …)`, once in the wrong-declaration-under-`caixa.lisp`
    /// gate that refuses a repo-surface declaration under the filename
    /// `feira` loads as a package manifest — with no compile-time link
    /// between the two hand-rolled arm sets. A future arm addition (the
    /// module doc's "third dialect" hazard actualises as a fifth arm
    /// [`CaixaDialeto`] that belongs to the `defmolde` declaration
    /// family — a third arity variant, an alias-declaration family
    /// pleme-doc-gen sharpens as its schema evolves) would silently
    /// split the two hand-rolled `matches!` arm-sets from each other
    /// and from the paired [`Self::palavra_canonica`] projection: one
    /// call site picks up the new arm, one does not, and the disagreement
    /// surfaces far from the arm-addition commit as a `feira dialeto`
    /// consumer reporting a repo-surface declaration under one gate but
    /// not the other. Routing every "belongs to the `defmolde` family"
    /// predicate through this one substrate primitive closes the axis:
    /// a future arm addition lands one match arm here (a compile-time
    /// exhaustiveness error otherwise), not a coordinated per-`matches!`
    /// rewrite across every caller.
    ///
    /// Peer of the sibling [`crate::CaixaKind::requires_lib`] (0421c22)
    /// per-arm-set predicate on the [`crate::CaixaKind`] closed-set
    /// discriminator's "kind requires a `lib/` surface" axis — extends
    /// the same "one canonical typed predicate per per-arm-set gate,
    /// one dispatch on the substrate primitive" discipline onto the
    /// [`CaixaDialeto`] closed-set discriminator's "belongs to the
    /// `defmolde` declaration family" axis. The dialect-classification
    /// axis's second per-arm-set predicate (first being the implicit
    /// palavra_canonica-through-consumidor-through-descricao arm-set
    /// collapse already carried on the sibling projections) — the first
    /// explicitly-typed per-arm-set predicate on the axis, matching the
    /// discipline the sibling M2 [`crate::CaixaKind`] closed-set
    /// discriminator already carries with `requires_lib`.
    ///
    /// Three consumers now route through this one typed dispatch: the
    /// [`caixa-feira`](../../caixa_feira/cmd/dialeto/index.html) verb's
    /// `--strict-palavra` gate (refusing a repo-surface declaration
    /// still written as `(defcaixa …)`), the same verb's wrong-
    /// declaration-under-`caixa.lisp` gate (refusing a repo-surface
    /// declaration under the filename `feira` loads as a package
    /// manifest), and [`crate::Caixa::from_lisp`]'s foreign-dialect
    /// gate (raising [`crate::ManifestError::DialetoEstrangeiro`] before
    /// the derive's `parse_kwargs_strict` walk on any `defmolde`-family
    /// classification — the pre-lift hand-rolled three-arm
    /// `match { Pacote => {}, Desconhecido => {}, foreign => Err(…) }`
    /// literal whose `foreign =>` wildcard silently absorbed anything
    /// non-Pacote-non-Desconhecido, now the third external consumer of
    /// the `defmolde`-family partition).
    #[must_use]
    pub const fn is_molde_family(self) -> bool {
        // Routed through the derive-generated per-arm predicates
        // [`Self::is_molde`] + [`Self::is_molde_posicional`] so the
        // two-arm collapse links compile-time back to the closed-set
        // typed dispatch every peer arm-set predicate on the caixa
        // surface (e.g. [`crate::CaixaKind::requires_lib`] on the
        // sibling `:kind` axis) now carries. Byte-equivalent to the
        // pre-lift `matches!(self, Self::Molde | Self::MoldePosicional)`
        // form (the derived `is_*` predicates each expand to the same
        // `matches!(self, Self::X)` shape by construction), but a
        // future arm rename or IsVariant `#[is_variant(name = "…")]`
        // override lands at exactly one dispatch on the substrate
        // primitive rather than a hand-rolled two-arm literal.
        self.is_molde() || self.is_molde_posicional()
    }
}

/// [`std::fmt::Display`] routed through [`CaixaDialeto::as_str`], so the
/// pretty-printed byte-string every consumer that formats the dialect as
/// user-facing / census text lands on (the `feira dialeto` per-manifest
/// `--list` row, the `feira dialeto` census summary line's per-arm
/// counters, a future M4 admission-webhook's rejection body naming the
/// accepted-dialect set) reaches for the same `PascalCase` per-arm
/// byte-string the [`CaixaDialeto::as_str`] helper returns.
///
/// Prior to this lift the [`std::fmt::Display`] impl hand-rolled its own
/// four-arm literal-string match — the one hand-rolled per-arm dispatch
/// on the closed [`CaixaDialeto`] discriminator that had NO substrate
/// primitive accessor to defer to (the sibling [`CaixaDialeto::palavra_canonica`] /
/// [`CaixaDialeto::consumidor`] / [`CaixaDialeto::descricao`] projections
/// carry distinct byte-shapes per axis, so none of them could serve as
/// the Display source). A future variant addition (a fifth dialect the
/// module doc's "third dialect" hazard actualises) would land one arm at
/// the enum and per-arm returns at the paired accessors, but a hand-rolled
/// [`std::fmt::Display`] match would silently drop the new arm to compile-
/// fail-at-the-match-arm-site rather than through the shared substrate
/// primitive. Routing [`std::fmt::Display`] through [`CaixaDialeto::as_str`]
/// closes the last unlifted per-arm `PascalCase`-name projection on the
/// caixa surface — the seventh (and last unlifted) closed-set fieldless
/// typed enum on the caixa surface to converge onto the same
/// `Display`-through-`as_str` discipline the six siblings
/// ([`crate::CaixaKind`] / [`crate::supervisor::RestartStrategy`] /
/// [`crate::supervisor::RestartPolicy`] /
/// [`crate::aplicacao::PlacementStrategy`] / [`crate::aplicacao::RateLimitUnit`]
/// / [`crate::dep::DepList`]) already carry.
impl std::fmt::Display for CaixaDialeto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Substrate-canonical [`AsRef<str>`] projection on the [`CaixaDialeto`]
/// closed-set fieldless typed dialect-classification enum — routes through
/// the same [`CaixaDialeto::as_str`] `pub const fn` scalar accessor the
/// paired [`std::fmt::Display`] impl already delegates through, so any
/// future consumer that binds a [`CaixaDialeto`] through the standard-
/// library `impl AsRef<str>` bound (a [`std::process::Command::arg`]
/// shell-out that composes the canonical `PascalCase` variant-name into a
/// `feira dialeto --strict-palavra <Pacote|Molde|MoldePosicional|Desconhecido>`
/// diagnostic overlay, a `tracing::field::Value::Str`-arm structured-log
/// recorder on the [`crate::Caixa::from_lisp`] foreign-dialect
/// [`crate::ManifestError::DialetoEstrangeiro`] refusal path, a
/// [`std::collections::HashMap`] lookup keyed on the canonical name
/// through `map.get::<str>(dialeto.as_ref())` on a future M4 admission-
/// webhook's per-dialect rejection-body composition table) reaches the
/// paired `"Pacote"` / `"Molde"` / `"MoldePosicional"` / `"Desconhecido"`
/// byte-string through one substrate-primitive dispatch rather than an
/// open-coded `.as_str()` re-inlining at every wire-up.
///
/// Same "route the trait impl through the substrate-primitive accessor"
/// discipline the sibling [`crate::CaixaVersion`] [`AsRef<str>`] impl
/// (16d5c7e), the paired M2 [`crate::supervisor::RestartStrategy`]
/// [`AsRef<str>`] impl (63eb1a4), the paired M2
/// [`crate::supervisor::RestartPolicy`] [`AsRef<str>`] impl (419ea81),
/// the M3 [`crate::aplicacao::PlacementStrategy`] [`AsRef<str>`] impl
/// (d86edd2), the M3 [`crate::aplicacao::RateLimitUnit`] [`AsRef<str>`]
/// impl (d8136db), and the top-level [`crate::CaixaKind`] [`AsRef<str>`]
/// impl (cd2091f) carry — extends the substrate primitive's
/// [`AsRef<str>`] projection axis onto the seventh closed-set typed enum
/// on the caixa surface: the dialect-classification axis previously
/// carried [`fmt::Display`]-through-`as_str` but not yet the paired
/// [`AsRef<str>`] impl, so a downstream consumer that bound the enum
/// through the standard-library `AsRef<str>` trait had to reach the
/// canonical byte-string through an open-coded `.as_str()` call rather
/// than the trait-idiomatic `.as_ref()` the peer closed-set typed enums
/// already admit.
///
/// Pinned load-bearing by
/// [`tests::caixa_dialeto_as_ref_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaDialeto::as_str`] across the four-arm
/// closed set) and
/// [`tests::caixa_dialeto_as_ref_str_routes_through_display_via_shared_accessor`]
/// (three-path convergence: `AsRef<str>` + `Display` + `as_str` all
/// resolve to the same byte-string per arm) — any future silent detour
/// that routes the impl through a divergent projection (a per-arm inline
/// `match self { CaixaDialeto::Pacote => "Pacote", … }` re-inlining that
/// opens a compile-time link to the un-lifted arm-literal, a swap onto
/// the second-axis [`CaixaDialeto::palavra_canonica`] /
/// [`CaixaDialeto::consumidor`] / [`CaixaDialeto::descricao`] accessors
/// that carry distinct byte-shapes per axis) trips at caixa-core test
/// time under `assert_eq!` rather than at a downstream
/// `impl AsRef<str>`-bound consumer's silent split.
impl AsRef<str> for CaixaDialeto {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Trait-idiomatic reverse projection on the [`CaixaDialeto`] closed-set
/// dialect-classification typed enum — routes byte-for-byte through the
/// paired substrate-primitive [`CaixaDialeto::from_wire`] `Option<Self>`
/// accessor so every future consumer that binds a `PascalCase` variant-
/// name byte-string through the standard-library `.try_into()` /
/// [`TryFrom`] axis (a future `feira dialeto --filter
/// <Pacote|Molde|MoldePosicional|Desconhecido>` CLI arg-parse that
/// composes into `let d: CaixaDialeto = s.try_into()?`, a future audit-
/// report re-loader binding a prior [`CaixaDialeto::as_str`] output
/// through `CaixaDialeto::try_from(&s)?`, a generic
/// `<T: TryFrom<&str>>`-bound loader over any of the substrate's closed-
/// set typed enums) reaches the same four-arm accept-set the sibling
/// [`CaixaDialeto::from_wire`] parses through and the sibling
/// [`CaixaDialeto::as_str`] emits, rather than an open-coded per-arm
/// `match s { "Pacote" => …, … }` cascade whose arm-set has no
/// compile-time link back to the substrate primitive.
///
/// Complements the pre-existing forward-projection triple
/// ([`std::fmt::Display`], [`AsRef<str>`], [`CaixaDialeto::as_str`]) with
/// the paired trait-idiomatic reverse-projection axis: Rust-side
/// newtype/typed-enum convention pairs [`AsRef<str>`] with either
/// [`std::str::FromStr`] or [`TryFrom<&str>`] on the same primitive so a
/// caller who can project *out to* a `&str` can also project *in from*
/// one. The [`TryFrom<&str>`] axis is deliberately chosen over
/// [`std::str::FromStr`] to sidestep the `clippy::should_implement_trait`
/// lint that the sibling method-named `from_wire` would trigger under a
/// `FromStr` impl (the same design tradeoff the peer
/// [`crate::CaixaKind`] `TryFrom<&str>` impl (3c83606) and the peer
/// [`crate::provedor::ferrite::FerriteRuntime::from_wire`] block note)
/// — this impl closes the trait-idiomatic reverse axis without disturbing
/// the method-named `from_wire` shape every sibling closed-set typed
/// enum on the substrate already carries.
///
/// `type Error = ()` matches the sibling [`CaixaDialeto::from_wire`]'s
/// `Option<Self>` return-shape's deliberate deferral of error typing:
/// the caller picks the diagnostic form appropriate for its use site
/// (a future `feira dialeto --filter` arg-parse composes its own per-verb
/// "unknown dialect: <arg> — accepted: {…}" message enumerating
/// [`CaixaDialeto::ALL`], a future admission-webhook rejection body
/// wraps the `Err(())` outcome with the accepted-set enumeration for
/// operator diagnostics, a `Result::map_err` at the call site lifts the
/// unit-error to a per-verb error type). Same shape the peer
/// [`crate::CaixaKind`] `TryFrom<&str>` impl (3c83606) and the peer
/// [`FerriteRuntime::from_wire`] doc block motivate on the sibling
/// closed-set typed enums' reverse projections.
///
/// The paired [`TryFrom<&str>`] impl reaches the same four-arm accept-
/// set the [`CaixaDialeto::from_wire`] resolver dispatches through, so
/// any future arm addition (the module doc's "third dialect" hazard
/// actualises as a fifth arm belonging to the `defmolde` family or a
/// wholly new declaration) grows the trait-idiomatic axis by
/// construction — one caixa-core edit on [`CaixaDialeto::from_wire`]
/// extends both the method-named reverse projection every existing
/// consumer keys off and the trait-idiomatic reverse projection this
/// impl exposes, without a coordinated rewrite across every future
/// `TryFrom<&str>`-bound consumer's arm-set.
///
/// Pinned load-bearing by
/// [`tests::caixa_dialeto_try_from_str_routes_through_from_wire_accessor`]
/// (byte-parity pin against [`CaixaDialeto::from_wire`] across the four-
/// arm accept-set) and
/// [`tests::caixa_dialeto_try_from_str_rejects_unknown_byte_strings`]
/// (rejection witness against silent accept-set widening).
impl TryFrom<&str> for CaixaDialeto {
    type Error = ();

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::from_wire(s).ok_or(())
    }
}

/// Trait-idiomatic forward projection on the [`CaixaDialeto`] closed-set
/// dialect-classification typed enum — routes byte-for-byte through the
/// sibling substrate-primitive [`CaixaDialeto::as_str`] `pub const fn`
/// accessor so every future consumer that needs `&'static str` lifetime
/// bytes on the dialect-classification axis (a
/// `tracing::field::valuable::Value::Str` recording where the `Str` arm's
/// typing demands `&'static str`, a
/// `Cow::Borrowed::<'static, str>(dialeto.into())` composer on the future
/// M4 admission-webhook rejection body where the `Cow<'static, str>`
/// typing rules out the sibling [`AsRef<str>`] borrowed return, a generic
/// `<T: Into<&'static str>>`-bound serializer or error formatter that
/// requires the `'static` bound) reaches the same four `"Pacote"` /
/// `"Molde"` / `"MoldePosicional"` / `"Desconhecido"` byte-strings the
/// sibling [`CaixaDialeto::as_str`] emitter returns, rather than an
/// open-coded per-arm literal cascade whose arm-set has no compile-time
/// link back to the substrate primitive.
///
/// Return type is `&'static str` by construction — every
/// [`CaixaDialeto::as_str`] arm resolves to a compile-time `pub const fn`
/// return of a `&'static str` literal, so the trait's return-type promise
/// is upheld structurally without a `String::leak()` cast or a per-arm
/// inline literal.
///
/// Complements the pre-existing reverse-projection axis pair
/// ([`TryFrom<&str>`] above + method-named [`CaixaDialeto::from_wire`])
/// with the trait-idiomatic forward-projection axis: Rust-side
/// newtype/typed-enum convention pairs [`TryFrom<&str>`] with the mirror-
/// image [`From<Self> for &'static str`] on the same primitive so a
/// caller who can project *in from* a `&str` can also project *out to*
/// one under a `'static`-lifetime bound. The
/// [`AsRef<str>`] impl already carries the same emit-set on the borrowed
/// return path; this impl closes the trait-idiomatic axis pair with the
/// stricter `&'static str` lifetime the sibling `AsRef<str>` cannot
/// promise (its return borrows from `&self`, not from the
/// [`CaixaDialeto::as_str`] `pub const fn`'s static-string result).
///
/// Same "route the trait impl through the substrate-primitive accessor"
/// discipline the sibling [`crate::supervisor::RestartStrategy`]
/// `From<Self> for &'static str` impl (523157d — first-mover on this
/// forward-projection family), [`crate::supervisor::RestartPolicy`]
/// `From<Self> for &'static str` impl (9fb37d0 — second peer, closing
/// the M2 OTP-shape sibling pair), and [`crate::CaixaKind`]
/// `From<Self> for &'static str` impl (edb827b — third peer, opening
/// the campaign onto the top-level caixa surface) carry — extends the
/// substrate primitive's trait-idiomatic forward-projection axis onto
/// the fourth closed-set fieldless typed enum on the caixa surface: the
/// dialect-classification axis, previously carrying the paired
/// [`std::fmt::Display`] / [`AsRef<str>`] / [`CaixaDialeto::as_str`] /
/// [`TryFrom<&str>`] / [`CaixaDialeto::from_wire`] forward+reverse
/// projections but not yet the trait-idiomatic forward projection with
/// the `&'static str` lifetime bound.
///
/// Unlike the peer [`crate::CaixaKind`] impl (which carries a two-axis
/// split between the lowercase Portuguese `as_str` diagnostic axis and
/// the `PascalCase` `wire_name` author-surface axis, so the trait's
/// round-trip witness must cross through the wire axis rather than
/// composing the two trait impls directly), [`CaixaDialeto`] is an
/// internal classification whose [`CaixaDialeto::as_str`] output and
/// [`CaixaDialeto::from_wire`] input share the same `PascalCase`
/// vocabulary by construction — the trait-idiomatic axis pair
/// ([`From<Self> for &'static str`] + [`TryFrom<&str> for Self`])
/// therefore round-trips directly, without an intermediate wire-vocab
/// hop.
///
/// The paired [`CaixaDialeto::as_str`] accessor's four-arm emit-set is
/// the single source of truth — every future arm addition (the module
/// doc's "third dialect" hazard actualises as a fifth arm belonging to
/// the `defmolde` family or a wholly new declaration) grows the trait-
/// idiomatic forward axis by construction: one caixa-core edit on
/// [`CaixaDialeto::as_str`] extends every one of the sibling forward-
/// projection paths ([`std::fmt::Display`], [`AsRef<str>`],
/// [`CaixaDialeto::as_str`] itself, and this
/// [`From<Self> for &'static str`]) without a coordinated rewrite across
/// every future `Into<&'static str>`-bound consumer's arm-set. This lift
/// closes the fourth peer on the trait-idiomatic forward-projection
/// campaign the recently-landed peer commits opened; the remaining ten
/// closed-set typed enums on the caixa substrate surface
/// (`PlacementStrategy`, `WitShape`, `RateLimitUnit`,
/// `PathShapeViolation`, `InvariantKind`, `ArchVerdict`, `Severity`,
/// `FixSafety`, `Semantic`, `FerriteRuntime`) are the future targets
/// of this campaign.
///
/// Pinned load-bearing by
/// [`tests::caixa_dialeto_from_into_static_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaDialeto::as_str`] across the four-arm
/// emit-set, plus a `const`-context materialization witness for the
/// `&'static str` lifetime promise, plus a paired `.into()` shape
/// assertion covering the blanket-derived `Into<&'static str>` shape)
/// and
/// [`tests::caixa_dialeto_from_into_static_str_and_as_str_partition_the_emit_set`]
/// (partition pin asserting `<&'static str as From<CaixaDialeto>>::from`
/// and [`CaixaDialeto::as_str`] agree on every arm, plus a two-way
/// direct round-trip witness through the paired trait-idiomatic
/// [`TryFrom<&str>`] axis that closes the two-way `Self ↔ &'static str`
/// round-trip on the trait-idiomatic axis pair without the wire-vocab
/// intermediate the peer [`crate::CaixaKind`] axis pair requires).
impl From<CaixaDialeto> for &'static str {
    fn from(dialeto: CaixaDialeto) -> &'static str {
        dialeto.as_str()
    }
}

/// Trait-idiomatic *forward* projection on [`CaixaDialeto`] from a
/// *borrowed* input onto the `&'static str` axis — the borrowed-input
/// companion to the paired owned-input [`From<CaixaDialeto> for &'static
/// str`] impl immediately above. Routes byte-for-byte through the same
/// substrate-primitive [`CaixaDialeto::as_str`] `pub const fn` accessor so
/// every consumer that binds a `&CaixaDialeto` through the standard-
/// library `.into()` / [`From<&Self> for &'static str`] axis (a
/// `CaixaDialeto::ALL.iter().map(<&'static str>::from).collect::<Vec<_>>()`
/// per-arm accept-set materializer — whose iterator over
/// `&'static [CaixaDialeto]` yields `&CaixaDialeto`, not `CaixaDialeto`,
/// so the owned-input [`From<CaixaDialeto>`] axis alone forces every call
/// site through an explicit `.copied()` / dereference / [`Copy`]-bound
/// restatement rather than the direct trait-idiomatic projection; a
/// future generic `<T: Copy + for<'a> Into<&'static str>>`-bound
/// diagnostic column that walks the `iter().map(Into::into)` shape
/// verbatim over any of the substrate's closed-set typed enums; the
/// future M4 admission-webhook rejection body composer's per-dialect
/// accepted-set enumeration built from an iterated
/// `CaixaDialeto::ALL.iter().map(|d| d.into())` pipe rather than a
/// per-arm `match d { … }` cascade; a
/// `HashMap::<&'static str, CaixaDialeto>::from_iter(
///     CaixaDialeto::ALL.iter().map(|d| (d.into(), *d)))`-style
/// per-dialect reverse-lookup table the sibling [`TryFrom<&str>`] impl
/// cannot compose without this borrowed-input axis in place) reaches the
/// same four `"Pacote"` / `"Molde"` / `"MoldePosicional"` /
/// `"Desconhecido"` byte-strings the paired owned-input
/// [`From<CaixaDialeto> for &'static str`], the sibling
/// [`std::fmt::Display`], [`AsRef<str>`], and [`CaixaDialeto::as_str`]
/// surfaces already return.
///
/// Third peer on the substrate-wide trait-idiomatic *borrowed-input*
/// forward-projection family opened on
/// [`crate::dep::DepList`] (64aa742) and extended onto
/// [`crate::CaixaKind`] (5ab993a). Rust's `From` trait does not
/// auto-derive the `From<&Self>` sibling from a `From<Self>` impl (the
/// blanket `impl<T, U> From<&T> for U where T: Copy, U: From<T>` does
/// not exist in `core`), so every closed-set typed enum that carries
/// the owned-input axis but not the borrowed-input axis forces every
/// borrowed-input call site through a `.copied()` /
/// `<&'static str>::from(*dialeto)` / `dialeto.as_str()` detour whose
/// type bounds have no compile-time link to the substrate primitive.
///
/// Unlike the peer [`crate::CaixaKind`] impl (which carries a two-axis
/// split between the lowercase Portuguese `as_str` diagnostic axis and
/// the `PascalCase` `wire_name` author-surface axis, so a `.into()`
/// pipe over `CaixaKind::ALL` yields the diagnostic vocabulary rather
/// than the wire vocabulary), [`CaixaDialeto`]'s [`CaixaDialeto::as_str`]
/// and [`CaixaDialeto::from_wire`] share the same `PascalCase`
/// vocabulary by construction — the borrowed-input projection this impl
/// exposes therefore composes directly with the sibling
/// [`TryFrom<&str>`] axis to build reverse-lookup tables without the
/// wire-vocab intermediate hop the peer axis pair requires.
///
/// Pinned load-bearing by
/// [`tests::caixa_dialeto_from_borrowed_into_static_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaDialeto::as_str`] across the four-arm
/// emit-set via a borrowed input, plus a `const`-context materialization
/// witness for the `&'static str` lifetime promise, plus a blanket
/// `.into()` shape assertion) and
/// [`tests::caixa_dialeto_from_owned_and_borrowed_into_static_str_agree_on_every_arm`]
/// (cross-axis partition pin against the paired owned-input
/// [`From<CaixaDialeto> for &'static str`] impl, plus a
/// `.iter().map(Into::into)` pipe witness over [`CaixaDialeto::ALL`]
/// that materializes the four-arm accept-set through the borrowed-input
/// axis alone, plus a direct round-trip witness through the paired
/// trait-idiomatic [`TryFrom<&str>`] axis that closes the two-way
/// `&Self → &'static str → Self` round-trip on the borrowed-input axis
/// without the wire-vocab intermediate the peer [`crate::CaixaKind`]
/// axis pair requires).
impl From<&CaixaDialeto> for &'static str {
    fn from(dialeto: &CaixaDialeto) -> &'static str {
        dialeto.as_str()
    }
}

/// A source that is not a `(defcaixa …)` / `(defmolde …)` form at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DialetoError {
    #[error("source has no top-level form")]
    Vazio,
    #[error("top-level form is not a list — a manifest is `(defcaixa …)`")]
    NaoEhLista,
    #[error(
        "top-level form is headed by `{encontrado}`, not `defcaixa` or `defmolde` \
         (a manifest's first form must be the declaration itself)"
    )]
    CabecaErrada { encontrado: String },
    #[error("manifest does not parse as tatara-lisp: {0}")]
    Leitura(String),
}

impl DialetoError {
    /// Construct a [`DialetoError::CabecaErrada`] naming the offending
    /// head symbol found at the top-level form.
    ///
    /// Substrate primitive every [`classify_form`] wrong-head fallthrough
    /// wire-up site now routes through, folding the pre-lift uniform
    /// three-line `Self::CabecaErrada { encontrado: <head>.to_string() }`
    /// one-field struct-literal onto one substrate primitive matching the
    /// peer `LimitsError::unknown_byte_unit(unit: &str)` /
    /// `LimitsError::unknown_duration_unit(unit: &str)`
    /// (`limits_codec_unit_only_ctors!` — 29fac09) single-slot
    /// discipline on the sibling one-field `{ <field>: String }` envelope
    /// axis, and matching the peer `ManifestError::code_path_empty` /
    /// `BehaviorError::empty_path` / `UpgradeError::duplicate_from` /
    /// `AplicacaoError::placement_cluster_duplicate` (94dabc8 / 0e33b37 /
    /// 7e52aec / 92b1c92) single-slot inherent-ctor discipline every
    /// sibling `{ <field>: <T> }` error-envelope variant on caixa-core's
    /// error surface now carries.
    ///
    /// The one open-coded wire-up site — `classify_form`'s wrong-head
    /// fallthrough arm on the `head: &str` binding read from the
    /// top-level form via [`tatara_lisp::Sexp::as_symbol`] — opened the
    /// identical three-line
    /// `Self::CabecaErrada { encontrado: <head>.to_string() }` block
    /// against the codec-scoped `<head>: &str` binding. Now routes
    /// through `DialetoError::cabeca_errada(head)`, byte-equal to the
    /// pre-lift struct-literal on the same `&str` fixture, so any future
    /// widening of the diagnostic shape (e.g. carrying the source-file
    /// path alongside the head symbol, carrying the head symbol's
    /// position offset for an authoring-surface caret pointer) lands at
    /// exactly one dispatch on the substrate primitive rather than re-
    /// inlining the struct-literal at every wrong-head fallthrough
    /// consumer.
    #[must_use]
    pub fn cabeca_errada(encontrado: &str) -> Self {
        Self::CabecaErrada {
            encontrado: encontrado.to_string(),
        }
    }

    /// Construct a [`DialetoError::Leitura`] carrying the offending
    /// tatara-lisp reader-error message `reason` verbatim in the
    /// variant's tuple-newtype payload.
    ///
    /// Substrate primitive every [`classify`] tatara-lisp-reader
    /// map-err wire-up site now routes through, folding the pre-lift
    /// uniform `Self::Leitura(<into-String-expr>)` tuple-newtype
    /// construction onto one substrate primitive matching the peer
    /// `LimitsError::empty_byte_size` / `LimitsError::empty_duration`
    /// (7a4b003 / 319216c) `(String)` single-slot tuple-newtype
    /// discipline on the sibling
    /// [`crate::limits::LimitsError`] envelope's empty-shape axis of
    /// the paired codec-magnitude family. Peer to the sibling
    /// [`DialetoError::cabeca_errada`] ctor on the same envelope's
    /// wrong-head axis but on the tatara-lisp-reader axis rather than
    /// the classifier-fallthrough axis. Closes the last un-lifted
    /// variant on [`DialetoError`] — every one of the sole wire-up
    /// sites (the [`classify`] tatara-lisp-reader `.map_err(|e|
    /// Self::Leitura(e.to_string()))` arm) opened the identical
    /// `DialetoError::Leitura(<into-String-expr>)` block against the
    /// codec-scoped `String` (`e.to_string()`) binding, so the fold
    /// routes the site through one dispatch on a uniform
    /// `impl Into<String>` param, byte-equal to the pre-lift
    /// tuple-newtype construction on the same argument.
    ///
    /// The `impl Into<String>` bound covers both wire-up shapes on
    /// [`classify`] — a `String` binding (`e.to_string()` on the
    /// [`tatara_lisp::Error`]-carrying `e` binding) and a `&str`
    /// binding (a future admission-webhook consumer probing a
    /// caller-scoped `&'static str` fixture, a future
    /// `feira lint --tatara-reader-round-trip` verb sweeping every
    /// `tatara_lisp::read` return through the same shape gate) —
    /// without forcing the caller to spell the conversion at the
    /// wire-up site. Same shape the peer
    /// [`crate::limits::LimitsError::empty_byte_size`] /
    /// [`crate::limits::LimitsError::empty_duration`] /
    /// [`crate::limits::LimitsError::bad_millicores`] /
    /// [`crate::limits::LimitsError::bad_byte_magnitude`] /
    /// [`crate::limits::LimitsError::bad_duration_magnitude`] folds
    /// carry on the peer bad-magnitude and empty-shape axes of the
    /// same paired `(String)` tuple-newtype codec-magnitude family.
    /// `#[must_use]` fires a compile warning at any wire-up that
    /// mistakenly discards the constructed error.
    ///
    /// Every future consumer that wants to construct this variant
    /// outside [`classify`] (a deferred `feira lint --tatara-reader-
    /// round-trip` per-caixa admission verb probing each authored
    /// manifest against the tatara-lisp-reader shape gate, an M4
    /// typed `mesh.pleme.io/v1alpha1/Servico` CR materializer's
    /// per-manifest admission validator re-checking one edited
    /// `caixa.lisp` against the reader floor, a per-`caixa.lisp`
    /// value-shape pre-emitter probing each declared manifest ahead
    /// of the operator's admit-cycle) now reaches the variant
    /// through one call rather than re-inlining the tuple-newtype
    /// block in lockstep with the pre-existing wire-up.
    #[must_use]
    pub fn leitura(reason: impl Into<String>) -> Self {
        Self::Leitura(reason.into())
    }
}

/// Classify a manifest source without committing to either schema.
///
/// Deliberately reads only the head symbol and the set of top-level keywords —
/// enough to route, never enough to half-parse. A classifier that started
/// validating would grow into a third parser, which is the shape of the problem
/// it exists to name.
///
/// # Errors
/// [`DialetoError`] when the source is not a manifest declaration at all.
pub fn classify(src: &str) -> Result<CaixaDialeto, DialetoError> {
    let forms = tatara_lisp::read(src).map_err(|e| DialetoError::leitura(e.to_string()))?;
    let first = forms.first().ok_or(DialetoError::Vazio)?;
    classify_form(first)
}

/// [`classify`] over an already-read form.
///
/// # Errors
/// [`DialetoError`] when the form is not a manifest declaration.
pub fn classify_form(form: &Sexp) -> Result<CaixaDialeto, DialetoError> {
    let list = form.as_list().ok_or(DialetoError::NaoEhLista)?;
    let head = list
        .first()
        .and_then(Sexp::as_symbol)
        .ok_or(DialetoError::NaoEhLista)?;

    match head {
        // `defmolde` is unambiguous by construction — it exists precisely so a
        // consumer never has to infer which declaration it holds. Both arities
        // are the same declaration; the positional one keeps its own variant
        // only so a census can report the split.
        "defmolde" => {
            return Ok(if starts_with_positional_name(&list[1..]) {
                CaixaDialeto::MoldePosicional
            } else {
                CaixaDialeto::Molde
            });
        }
        "defcaixa" => {}
        other => {
            return Err(DialetoError::cabeca_errada(other));
        }
    }

    let args = &list[1..];

    // `(defcaixa <symbol> :kind … :ecosystem …)`. Only the Molde dialect has a
    // positional arity; `Caixa` is keyword-only, so a leading bare symbol
    // settles it without looking further.
    if starts_with_positional_name(args) {
        return Ok(CaixaDialeto::MoldePosicional);
    }

    let keys = top_level_keywords(args);
    let has = |k: &str| keys.iter().any(|s| s == k);

    // Order matters, and it is not arbitrary: `:nome` and `:name` are the two
    // required head slots and no file in the measured corpus carries both.
    // Checking them FIRST means the decision rests on the one slot each schema
    // makes mandatory, rather than on optional evidence like `:ecosystem`.
    if has("nome") {
        return Ok(CaixaDialeto::Pacote);
    }
    if has("name") || has("ecosystem") || has("package") {
        return Ok(CaixaDialeto::Molde);
    }
    Ok(CaixaDialeto::Desconhecido)
}

/// True when the first argument is a bare symbol rather than a keyword — the
/// positional-name arity.
fn starts_with_positional_name(args: &[Sexp]) -> bool {
    matches!(args.first(), Some(Sexp::Atom(Atom::Symbol(_))))
}

/// The top-level keyword names (without the leading `:`) of a kwarg list.
///
/// Steps in pairs so a keyword appearing as a VALUE — `:kind :Biblioteca`, or a
/// nested `(:nome "dep" :versao "^0.1")` inside `:deps` — is never counted as a
/// top-level slot. A naive scan for `:nome` anywhere in the source classifies
/// every Molde manifest with a `:deps` list as a Pacote.
fn top_level_keywords(args: &[Sexp]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if let Sexp::Atom(Atom::Keyword(k)) = &args[i] {
            out.push(k.clone());
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACOTE: &str = r#"
      (defcaixa
        :nome   "checkout"
        :versao "0.1.0"
        :kind   Servico
        :deps   ((:nome "caixa-teia" :versao "^0.1")))
    "#;

    const MOLDE: &str = r#"
      (defcaixa
        :name "base64"
        :kind :Biblioteca
        :ecosystem :rust-single-crate
        :package {:name "base64" :version "0.22.1"}
        :workflows [:auto-release])
    "#;

    const MOLDE_POSICIONAL: &str = r#"
      (defcaixa todoku-go
        :kind :Biblioteca
        :ecosystem :go
        :package {:name "todoku-go" :version "0.3.0"})
    "#;

    #[test]
    fn the_package_dialect_is_recognised() {
        assert_eq!(classify(PACOTE), Ok(CaixaDialeto::Pacote));
    }

    #[test]
    fn the_repo_surface_dialect_is_recognised() {
        assert_eq!(classify(MOLDE), Ok(CaixaDialeto::Molde));
    }

    #[test]
    fn the_positional_arity_is_recognised() {
        assert_eq!(
            classify(MOLDE_POSICIONAL),
            Ok(CaixaDialeto::MoldePosicional)
        );
    }

    #[test]
    fn defmolde_classifies_without_inference() {
        // The whole point of the new keyword: no schema sniffing required.
        let src = r#"(defmolde :name "x" :kind :Biblioteca :ecosystem :go)"#;
        assert_eq!(classify(src), Ok(CaixaDialeto::Molde));
        let pos = r"(defmolde todoku-go :kind :Biblioteca :ecosystem :go)";
        assert_eq!(classify(pos), Ok(CaixaDialeto::MoldePosicional));
    }

    #[test]
    fn a_nested_nome_does_not_make_a_repo_surface_look_like_a_package() {
        // The exact failure a substring scan produces: `:deps ((:nome …))`
        // contains `:nome`, but not as a top-level slot.
        let src = r#"
          (defcaixa
            :name "x"
            :ecosystem :rust-single-crate
            :deps ((:nome "inner" :versao "^0.1")))
        "#;
        assert_eq!(classify(src), Ok(CaixaDialeto::Molde));
    }

    #[test]
    fn a_keyword_in_value_position_is_not_a_slot() {
        // `:kind :Biblioteca` — the value is itself a keyword. Stepping one at
        // a time would read `:Biblioteca` as a top-level slot.
        let src = r#"(defcaixa :kind :Biblioteca :name "x")"#;
        assert_eq!(classify(src), Ok(CaixaDialeto::Molde));
    }

    #[test]
    fn an_unrecognised_defcaixa_is_reported_not_guessed() {
        let src = r#"(defcaixa :licenca "MIT")"#;
        assert_eq!(classify(src), Ok(CaixaDialeto::Desconhecido));
    }

    #[test]
    fn a_form_that_is_not_a_manifest_is_an_error_not_a_dialect() {
        assert_eq!(
            classify("(defflake :nome \"x\")"),
            Err(DialetoError::cabeca_errada("defflake"))
        );
        assert_eq!(classify(""), Err(DialetoError::Vazio));
    }

    #[test]
    fn every_dialect_names_its_consumer_and_its_canonical_keyword() {
        // Guards the routing table itself: a new variant added without an arm
        // here is a compile error in the match, and a variant that claims
        // `defcaixa` while being read by pleme-doc-gen would re-open the
        // collision this module closes. Sweeps [`CaixaDialeto::ALL`] rather
        // than the pre-lift open-coded four-arm literal list — a future arm
        // addition extends the slice as one edit and this pin picks it up
        // by construction.
        for &d in CaixaDialeto::ALL {
            assert!(!d.descricao().is_empty(), "{d}");
            assert!(!d.consumidor().is_empty(), "{d}");
        }
        assert_eq!(CaixaDialeto::Pacote.palavra_canonica(), "defcaixa");
        assert_eq!(CaixaDialeto::Molde.palavra_canonica(), "defmolde");
        assert_ne!(
            CaixaDialeto::Pacote.palavra_canonica(),
            CaixaDialeto::Molde.palavra_canonica(),
            "the two dialects must not share a canonical keyword — that IS the defect"
        );
    }

    #[test]
    fn caixa_dialeto_all_enumerates_every_variant_exactly_once() {
        // Three-legged exhaustiveness pin, peer of the sibling
        // `caixa_kind_all_enumerates_every_variant_exactly_once`
        // (caixa-core/src/kind.rs) /
        // `restart_strategy_all_enumerates_every_variant_exactly_once`
        // (caixa-core/src/supervisor.rs) shape.
        //
        // 1. arm-count invariant: `ALL.len()` matches the declared arm
        //    count (four — a fifth arm added without extending `ALL`
        //    fails this pin at caixa-core test time);
        // 2. pairwise-distinctness invariant: every variant appears at
        //    most once in the slice (a duplicate arm would silently
        //    double-count in the census consumer, so the pin rejects
        //    duplicates outright);
        // 3. coverage invariant: every literal `CaixaDialeto::X` is in
        //    the slice (the compiler-checked exhaustiveness on the peer
        //    per-arm `match self` in the accessors keeps the enum arm
        //    set and the `ALL` slice mutually aligned).
        assert_eq!(
            CaixaDialeto::ALL.len(),
            4,
            "ALL must list every arm exactly once; a fifth arm added \
             without extending ALL fails this pin — extend ALL alongside \
             the new variant"
        );

        let mut seen: Vec<CaixaDialeto> = Vec::new();
        for &d in CaixaDialeto::ALL {
            assert!(
                !seen.contains(&d),
                "ALL contains a duplicate arm: {d}. Every variant appears \
                 exactly once — a duplicate would double-count in every \
                 iteration consumer"
            );
            seen.push(d);
        }

        // Coverage: exhaustively assert every literal variant is somewhere
        // in the slice. Written as an exhaustive `match` so a future arm
        // addition fails to compile here (missing match arm) until the
        // corresponding `assert` is added — the compiler enforces the pin's
        // completeness rather than a hand-maintained variant list.
        for variant in [
            CaixaDialeto::Pacote,
            CaixaDialeto::Molde,
            CaixaDialeto::MoldePosicional,
            CaixaDialeto::Desconhecido,
        ] {
            let coverage_probe = match variant {
                CaixaDialeto::Pacote
                | CaixaDialeto::Molde
                | CaixaDialeto::MoldePosicional
                | CaixaDialeto::Desconhecido => variant,
            };
            assert!(
                CaixaDialeto::ALL.contains(&coverage_probe),
                "ALL is missing variant {coverage_probe} — extend the slice"
            );
        }
    }

    #[test]
    fn caixa_dialeto_all_is_const_and_matches_iteration_count() {
        // Pins the const-ness of the slice at const-fold time. A future
        // change that promoted `ALL` to a non-const initializer (a lazy-
        // static, a runtime-computed Vec) would fail to compile here —
        // the pin locks in the compile-time-known iteration surface
        // every consumer builds against. Peer of the sibling
        // `caixa_kind_all_is_const_and_matches_iteration_count` (kind.rs)
        // / `restart_strategy_all_is_const_and_matches_iteration_count`
        // (supervisor.rs) shape.
        const ALL: &[CaixaDialeto] = CaixaDialeto::ALL;
        assert_eq!(ALL.len(), CaixaDialeto::ALL.len());
        // Sweep the iterator without collapsing to `.len()` so a future
        // change to `ALL`'s carrier that decouples `.len()` from the
        // iteration count (a lazy-computed shape, an alias `impl Iterator`
        // return, a wrapper newtype) still passes here iff the two agree
        // arm-for-arm; the `#[allow]` opts this local pin out of the
        // clippy `iter_count` collapse that would defeat the intent.
        #[allow(clippy::iter_count)]
        let iterated = ALL.iter().count();
        assert_eq!(iterated, CaixaDialeto::ALL.len());
    }

    #[test]
    fn caixa_dialeto_all_covers_every_variant_by_display_probe() {
        // Fanning `Display` over the slice sweeps the paired accessors
        // ([`CaixaDialeto::palavra_canonica`] / [`CaixaDialeto::consumidor`]
        // / [`CaixaDialeto::descricao`]) at every arm — every returned
        // byte-string is non-empty (the accessors' contract). A future
        // arm added without extending its per-arm `match self` return
        // would compile-fail at the accessor call inside the loop;
        // together with the `ALL.len() == 4` pin above, this locks the
        // accessor arm-set and the `ALL` slice mutually.
        for &d in CaixaDialeto::ALL {
            let display_form = d.to_string();
            assert!(
                !display_form.is_empty(),
                "Display must render a non-empty byte-string for every \
                 arm; empty: {d:?}"
            );
            // Consumidor / descricao / palavra-canonica must each surface
            // a non-empty scalar; every downstream diagnostic consumer
            // reaches through these accessors.
            assert!(!d.palavra_canonica().is_empty(), "{d}");
            assert!(!d.consumidor().is_empty(), "{d}");
            assert!(!d.descricao().is_empty(), "{d}");
        }
    }

    #[test]
    fn caixa_dialeto_as_str_returns_pascal_case_variant_name() {
        // Fail-before-pass-after per-arm shape pin: the four
        // [`CaixaDialeto::as_str`] arms must return the canonical
        // `PascalCase` byte-string that names the variant. Pre-lift this
        // byte-string existed only inside the hand-rolled Display impl's
        // four-arm literal-string match — every consumer that wanted the
        // `PascalCase` name reached through `format!("{d}")`'s allocation
        // path. Pinning the four arms explicitly here refuses a future
        // regression that ever reroutes an arm to a distinct spelling
        // (`"pacote"` lowercase, `"MoldePositional"` English rebrand,
        // `"Unknown"` for `Desconhecido`) — the census output and the
        // typed accessor would silently disagree until a downstream
        // consumer surfaced the drift at census time. Peer of the sibling
        // [`crate::supervisor::tests::restart_strategy_variants_serialize_to_lifted_scalar_values`]
        // / `placement_strategy_variants_serialize_to_lifted_scalar_values`
        // / `caixa_kind_as_str_returns_lifted_peer_const` shape on the
        // sibling closed-set typed-enum discriminator axes — the seventh
        // (and last unlifted) closed-set typed enum on the caixa surface
        // to converge onto the same per-arm-shape-pin discipline.
        for (variant, expected) in [
            (CaixaDialeto::Pacote, "Pacote"),
            (CaixaDialeto::Molde, "Molde"),
            (CaixaDialeto::MoldePosicional, "MoldePosicional"),
            (CaixaDialeto::Desconhecido, "Desconhecido"),
        ] {
            assert_eq!(
                variant.as_str(),
                expected,
                "CaixaDialeto::{variant:?}.as_str() must return the \
                 canonical `PascalCase` variant-name byte-string; drift here \
                 splits the census-facing text from the substrate \
                 primitive every downstream consumer will read"
            );
        }
    }

    #[test]
    fn caixa_dialeto_display_routes_through_as_str_helper() {
        // Fail-before-pass-after convergence pin: for every arm in
        // [`CaixaDialeto::ALL`], the [`std::fmt::Display`] rendered form
        // must byte-equal [`CaixaDialeto::as_str`]'s return value. Pre-
        // lift these two paths were structurally independent — the
        // Display impl hand-rolled its own four-arm literal-string
        // match with no compile-time link back to any substrate accessor
        // — so a future variant rename could land at `Display` without
        // touching a paired accessor (or vice versa), silently splitting
        // the two paths on the renamed arm. Pinning the byte-equality
        // here makes any such split a caixa-core build-time failure at
        // this test rather than surfacing far from the rename commit as
        // a downstream census consumer emitting one spelling while the
        // typed accessor returned another. Peer of the sibling
        // [`crate::kind::tests::caixa_kind_display_routes_through_as_str_helper`]
        // (which pins the same convergence on the [`crate::CaixaKind`]
        // closed-set axis) — extends the discipline onto the seventh
        // (and last unlifted) closed-set fieldless typed enum on the
        // caixa surface.
        for &variant in CaixaDialeto::ALL {
            assert_eq!(
                variant.to_string(),
                variant.as_str(),
                "CaixaDialeto::{variant:?} Display must route through \
                 CaixaDialeto::as_str (single source of truth: the \
                 lifted per-arm `PascalCase` variant-name byte-string)"
            );
        }
    }

    #[test]
    fn caixa_dialeto_as_ref_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl AsRef<str> for CaixaDialeto` — asserts the standard-
        // library trait impl and the substrate-primitive
        // [`CaixaDialeto::as_str`] `pub const fn` accessor resolve to
        // the same `&str` per instance across the four-arm closed set,
        // so any future silent detour that routes the impl through a
        // divergent projection (a per-arm inline
        // `match self { CaixaDialeto::Pacote => "Pacote", … }` re-inlining
        // that opens a compile-time link to the un-lifted arm-literal,
        // a swap onto the second-axis
        // [`CaixaDialeto::palavra_canonica`] /
        // [`CaixaDialeto::consumidor`] / [`CaixaDialeto::descricao`]
        // accessors that carry distinct byte-shapes per axis) trips at
        // caixa-core test time under `PartialEq` rather than at a
        // downstream `impl AsRef<str>`-bound consumer's silent split.
        // Sweeps every one of the four arms [`CaixaDialeto::ALL`]
        // carries so no arm's projection is covered only by the sibling
        // `Display` path. Peer of the sibling
        // `rate_limit_unit_as_ref_str_routes_through_as_suffix_accessor`
        // (d8136db) on the M3 `:politicas :rate-limit` closed-set typed
        // enum, and the peer
        // [`crate::kind::tests::caixa_kind_as_ref_str_routes_through_as_str_accessor`]
        // (cd2091f) pin on the top-level closed-set typed
        // discriminator — the pins together close the substrate
        // primitive's `AsRef<str>` projection axis onto the seventh
        // closed-set fieldless typed enum on the caixa surface.
        for &variant in CaixaDialeto::ALL {
            assert_eq!(
                <CaixaDialeto as AsRef<str>>::as_ref(&variant),
                variant.as_str(),
                "AsRef<str> impl on CaixaDialeto::{variant:?} must \
                 byte-equal CaixaDialeto::as_str on the same instance \
                 — divergence signals a silent detour off the \
                 substrate-primitive accessor"
            );
        }
    }

    #[test]
    fn caixa_dialeto_as_ref_str_routes_through_display_via_shared_accessor() {
        // Fail-before-pass-after byte-parity pin on the three-path
        // convergence discipline the [`CaixaDialeto`] closed-set
        // dialect-classification enum now carries on the `&str`-
        // projection axis: `<CaixaDialeto as AsRef<str>>::as_ref(&v)`
        // (the newly lifted impl), `format!("{v}")` (the pre-existing
        // [`fmt::Display`] impl), and `v.as_str()` (the substrate-
        // primitive `pub const fn` accessor both trait impls delegate
        // through) must resolve to the same byte-string on every
        // instance across the four-arm closed set. Refuses any future
        // divergence between the two trait impls (a stray
        // [`fmt::Display::fmt`] rewrite that hand-rolls the arms
        // rather than delegating through the shared accessor; a
        // hypothetical `AsRef<str>` rewrite that inlines a per-arm
        // literal cascade) that would silently split the two
        // projection paths of the same closed-set typed enum. Mirrors
        // the sibling three-path-convergence discipline the peer
        // [`crate::aplicacao::RateLimitUnit`] typed enum carries
        // (`rate_limit_unit_as_ref_str_routes_through_display_via_shared_accessor`,
        // d8136db), the peer [`crate::CaixaKind`] triple
        // (`caixa_kind_as_ref_str_routes_through_display_via_shared_accessor`,
        // cd2091f), and the [`crate::CaixaVersion`] typed newtype
        // triple (`caixa_version_as_ref_str_routes_through_display_via_shared_accessor`,
        // 16d5c7e).
        for &variant in CaixaDialeto::ALL {
            let via_as_ref: &str = <CaixaDialeto as AsRef<str>>::as_ref(&variant);
            let via_display: String = format!("{variant}");
            let via_accessor: &str = variant.as_str();
            assert_eq!(via_as_ref, via_accessor);
            assert_eq!(via_display, via_accessor);
            assert_eq!(via_as_ref, via_display.as_str());
        }
    }

    #[test]
    fn caixa_dialeto_is_molde_family_returns_true_on_molde_and_positional_arms() {
        // Fail-before-pass-after per-arm shape pin on the two `defmolde`
        // declaration-family arms: [`CaixaDialeto::is_molde_family`] must
        // return `true` for [`CaixaDialeto::Molde`] and
        // [`CaixaDialeto::MoldePosicional`] — the two-arity closure of
        // one declaration ([`CaixaDialeto::MoldePosicional`]'s docstring:
        // "same declaration as [`Self::Molde`], written with the package
        // name as a bare positional symbol … one arity of one
        // declaration, not a third schema"). A future accidental flip that
        // reversed a per-arm arm's return without touching the paired
        // false-arm pin would silently open the substrate primitive to
        // false-positive on either arm — the `feira dialeto` verb's
        // `--strict-palavra` gate would then silently accept
        // repo-surface declarations under `(defcaixa …)` on one arm and
        // reject them on the other. Pinning the two true arms explicitly
        // here refuses that split at caixa-core build time.
        assert!(
            CaixaDialeto::Molde.is_molde_family(),
            "CaixaDialeto::Molde.is_molde_family() must return true — \
             Molde is the primary `defmolde` arm"
        );
        assert!(
            CaixaDialeto::MoldePosicional.is_molde_family(),
            "CaixaDialeto::MoldePosicional.is_molde_family() must return \
             true — MoldePosicional is the positional-arity form of the \
             same `defmolde` declaration Molde carries"
        );
    }

    #[test]
    fn caixa_dialeto_is_molde_family_returns_false_on_pacote_and_desconhecido_arms() {
        // Fail-before-pass-after per-arm shape pin on the two non-`defmolde`
        // arms: [`CaixaDialeto::is_molde_family`] must return `false` for
        // [`CaixaDialeto::Pacote`] (the sibling `defcaixa` tatara-lisp
        // package manifest, `palavra_canonica → "defcaixa"`) and for
        // [`CaixaDialeto::Desconhecido`] (the residue that names no
        // known declaration, `palavra_canonica → "?"`). Pinning the two
        // false arms explicitly here refuses a future accidental flip
        // that let the predicate widen to include either arm — the
        // `feira dialeto` verb's `--strict-palavra` gate would then
        // spuriously refuse every `(defcaixa …)` package manifest as if
        // it were a repo-surface declaration.
        assert!(
            !CaixaDialeto::Pacote.is_molde_family(),
            "CaixaDialeto::Pacote.is_molde_family() must return false — \
             Pacote is the `defcaixa` tatara-lisp package manifest, not \
             the `defmolde` repo-surface declaration"
        );
        assert!(
            !CaixaDialeto::Desconhecido.is_molde_family(),
            "CaixaDialeto::Desconhecido.is_molde_family() must return \
             false — the residue arm names no known declaration; it is \
             not silently promoted into the `defmolde` family"
        );
    }

    #[test]
    fn caixa_dialeto_is_molde_family_agrees_with_palavra_canonica_defmolde_projection() {
        // Load-bearing pin: for every arm in [`CaixaDialeto::ALL`], the
        // typed [`CaixaDialeto::is_molde_family`] predicate must agree
        // byte-for-byte with the paired [`CaixaDialeto::palavra_canonica`]
        // projection's `== "defmolde"` classifier — i.e. the two paths
        // partition the four-arm discriminator set into the same
        // `{Molde, MoldePosicional}` and `{Pacote, Desconhecido}` halves.
        // Pre-lift the sibling [`CaixaDialeto::palavra_canonica`] projection
        // (which returns `"defmolde"` for `Molde | MoldePosicional`,
        // `"defcaixa"` for `Pacote`, `"?"` for `Desconhecido`) was the
        // only substrate-side surface carrying the two-arm collapse; the
        // hand-rolled `matches!(d, CaixaDialeto::Molde |
        // CaixaDialeto::MoldePosicional)` sites in the `feira dialeto`
        // verb expressed no compile-time link back to it. A future arm
        // addition — the module doc's "third dialect" hazard actualises
        // as a fifth arm belonging to the `defmolde` family — would land
        // one match arm at [`Self::palavra_canonica`]'s `defmolde` return
        // (extending the sibling projection) but silently split the
        // hand-rolled two-arm `matches!` predicate sites if the new arm's
        // `is_molde_family` return were forgotten. Pinning byte-equality
        // between the two paths here makes any such split a caixa-core
        // build-time failure at this test rather than surfacing far from
        // the arm-addition commit as a downstream `--strict-palavra` /
        // `caixa.lisp`-holds-wrong-declaration gate silently ignoring the
        // new arm.
        for &d in CaixaDialeto::ALL {
            let via_palavra_canonica = d.palavra_canonica() == "defmolde";
            let via_is_molde_family = d.is_molde_family();
            assert_eq!(
                via_is_molde_family, via_palavra_canonica,
                "CaixaDialeto::{d:?}.is_molde_family() ({via_is_molde_family}) \
                 must agree with CaixaDialeto::{d:?}.palavra_canonica() == \
                 \"defmolde\" ({via_palavra_canonica}) — a split between the \
                 typed predicate and the sibling keyword projection would let \
                 a future arm addition land at one path and drift at the other, \
                 which is exactly the drift this pin refuses"
            );
        }
    }

    #[test]
    fn caixa_dialeto_is_molde_family_is_const_fn() {
        // Const-context pin: [`CaixaDialeto::is_molde_family`] must remain
        // `const fn` (its match is a fieldless-arm literal-pattern
        // discriminator, so no non-const operation exists on the resolution
        // path). Downstream consumers reaching for the predicate from a
        // `const` context (a future substrate-wide const-fold-driven audit
        // table that materializes per-arm gate-membership at build time,
        // a per-arm CR-admission-webhook gate registration in a `const`
        // context) rely on the const-ness. A future accidental downgrade
        // to non-`const` (an added runtime helper reachable only from a
        // non-`const` context) trips at caixa-core build time rather than
        // surfacing as a downstream `const`-context regression far from
        // the predicate declaration. Peer of the sibling
        // [`caixa_dialeto_as_str_is_const_fn`] pin on the paired
        // [`CaixaDialeto::as_str`] byte-string axis.
        const ARMS: [(CaixaDialeto, bool); 4] = [
            (CaixaDialeto::Pacote, CaixaDialeto::Pacote.is_molde_family()),
            (CaixaDialeto::Molde, CaixaDialeto::Molde.is_molde_family()),
            (
                CaixaDialeto::MoldePosicional,
                CaixaDialeto::MoldePosicional.is_molde_family(),
            ),
            (
                CaixaDialeto::Desconhecido,
                CaixaDialeto::Desconhecido.is_molde_family(),
            ),
        ];
        // Materialize the const-fold-evaluated table into a runtime slice
        // assertion — carries the same `bool = const fn call` shape a raw
        // `assert!(const_bool)` would, without tripping the
        // `assertions_on_constants` clippy lint that a per-arm
        // `assert!(CONST)` on a `const bool` triggers when the arm-count
        // is enumerated flat rather than compared as a whole-table shape.
        assert_eq!(
            ARMS,
            [
                (CaixaDialeto::Pacote, false),
                (CaixaDialeto::Molde, true),
                (CaixaDialeto::MoldePosicional, true),
                (CaixaDialeto::Desconhecido, false),
            ],
            "CaixaDialeto::is_molde_family() must evaluate in const context \
             for every arm and land on the {{false, true, true, false}} \
             partition — a future accidental downgrade to non-`const` \
             would trip the const-context array-initializer here"
        );
    }

    #[test]
    fn caixa_dialeto_as_str_is_const_fn() {
        // Const-context pin: [`CaixaDialeto::as_str`] must remain
        // `const fn` (its match arms return `pub const` byte-strings, so
        // no non-const operation exists on the resolution path).
        // Downstream consumers reaching for the accessor from a `const`
        // context (a future substrate-wide const-fold-driven audit table
        // that materializes every dialect's census label at build time,
        // a per-arm CR-admission-webhook message registration in a
        // `const` gate) rely on the const-ness. A future accidental
        // downgrade to non-`const` (an added runtime helper reachable
        // only from a non-`const` context, a manual hand-rolled `impl`
        // that shadows this method) trips at caixa-core build time
        // rather than surfacing as a downstream `const`-context
        // regression far from the accessor declaration. Peer of the
        // sibling [`crate::kind::tests::caixa_kind_wire_name_is_const_fn`]
        // pin on the paired [`crate::CaixaKind`] byte-string axis.
        const PACOTE: &str = CaixaDialeto::Pacote.as_str();
        const MOLDE: &str = CaixaDialeto::Molde.as_str();
        const MOLDE_POSICIONAL: &str = CaixaDialeto::MoldePosicional.as_str();
        const DESCONHECIDO: &str = CaixaDialeto::Desconhecido.as_str();
        assert_eq!(PACOTE, "Pacote");
        assert_eq!(MOLDE, "Molde");
        assert_eq!(MOLDE_POSICIONAL, "MoldePosicional");
        assert_eq!(DESCONHECIDO, "Desconhecido");
    }

    #[test]
    fn caixa_dialeto_is_variant_predicates_partition_the_arm_set() {
        // Fail-before-pass-after pin on the [`gen_platform::IsVariant`]
        // derive: for each of the four variants at [`CaixaDialeto::ALL`]`[idx]`
        // the observed four-slot predicate row must equal a one-hot row
        // with the `true` at exactly `idx`. Pre-derive the closed four-arm
        // dialect-classification partition lived only inside the paired
        // per-arm projections' four-arm match resolvers ([`Self::as_str`] /
        // [`Self::palavra_canonica`] / [`Self::consumidor`] /
        // [`Self::descricao`]) plus the two-arm [`Self::is_molde_family`]
        // hand-rolled `matches!` (now routed through the derived
        // predicates); a future rebrand (an accidental
        // `#[is_variant(name = "…")]` drift, a manual hand-rolled `impl`
        // that shadows the derive-generated method, an arm rename that
        // reroutes one arm through the wrong predicate lane) trips this
        // pin at caixa-core build time rather than surfacing far from the
        // derive declaration as a downstream [`Self::is_molde_family`]
        // consumer accepting the wrong arm-set. The expected row is
        // generated live from the [`Self::ALL`] declaration order rather
        // than transcribed by hand so a copy-paste flip reroutes at the
        // identity-diagonal assertion.
        //
        // Peer of the sibling
        // [`crate::kind::tests::caixa_kind_is_variant_predicates_partition_the_arm_set`]
        // / [`crate::supervisor::tests::restart_strategy_is_variant_predicates_partition_the_arm_set`]
        // / [`crate::aplicacao::tests::placement_strategy_is_variant_predicates_partition_the_arm_set`]
        // / [`crate::upgrade::tests::upgrade_instruction_is_variant_predicates_partition_the_arm_set`]
        // pins on the sibling closed-set typed-enum discriminator axes.
        for (idx, &variant) in CaixaDialeto::ALL.iter().enumerate() {
            let observed = [
                variant.is_pacote(),
                variant.is_molde(),
                variant.is_molde_posicional(),
                variant.is_desconhecido(),
            ];
            let mut expected = [false; 4];
            expected[idx] = true;
            assert_eq!(
                observed, expected,
                "CaixaDialeto::{variant:?} at ALL[{idx}] is_* predicates \
                 must fire only on their own arm lane (identity diagonal); \
                 got {observed:?}",
            );
        }
    }

    #[test]
    fn caixa_dialeto_is_variant_predicates_are_const_fn() {
        // The [`gen_platform::IsVariant`] derive emits `const fn`
        // predicates on the peer [`crate::CaixaKind`] +
        // [`crate::upgrade::UpgradeInstruction`] +
        // [`crate::supervisor::RestartStrategy`] +
        // [`crate::supervisor::RestartPolicy`] +
        // [`crate::aplicacao::PlacementStrategy`] +
        // [`crate::aplicacao::RateLimitUnit`] +
        // [`crate::dep::DepList`] closed-set typed enums — pin the same
        // posture on [`CaixaDialeto`] so a future accidental downgrade
        // to non-`const` (an added runtime helper reachable only from a
        // non-`const` context, a manual hand-rolled `impl` that shadows
        // the derive-generated method) trips at caixa-core build time
        // rather than surfacing as a downstream `const`-context
        // regression far from the derive declaration.
        // Use `const { assert!(…) }` (peer of the sibling
        // [`crate::render::PathShapeViolation`] +
        // [`crate::aplicacao::RateLimitUnit`] +
        // [`caixa_theme::style::Semantic`] const-fn pins) so the
        // const-context evaluation trips at const-fold time without
        // opening a per-`const bool` `assertions_on_constants` clippy
        // debt row this crate does not carry today for `dialeto.rs`.
        const { assert!(CaixaDialeto::Pacote.is_pacote()) };
        const { assert!(CaixaDialeto::Molde.is_molde()) };
        const { assert!(CaixaDialeto::MoldePosicional.is_molde_posicional()) };
        const { assert!(CaixaDialeto::Desconhecido.is_desconhecido()) };
    }

    #[test]
    fn caixa_dialeto_from_wire_accepts_every_as_str_output() {
        // Fail-before-pass-after per-arm accept pin on the newly lifted
        // [`CaixaDialeto::from_wire`] reverse projection: every arm in
        // [`CaixaDialeto::ALL`] must parse back through `from_wire` when
        // fed its own [`CaixaDialeto::as_str`] output, landing on
        // `Some(same_variant)` — a regression that hand-rolled either
        // side's per-arm match without threading through the shared
        // four-string closed set would silently disagree on any future
        // arm rename and this pin flags it at caixa-core build time.
        // Peer of the sibling
        // [`crate::kind::tests::caixa_kind_wire_round_trips_through_from_wire`]
        // (2aa6d23) /
        // `placement_strategy_from_wire_accepts_every_lifted_constant`
        // (18c7342) /
        // `dep_list_round_trips_through_as_str_and_from_wire` (45ee563)
        // shape on the sibling closed-set typed-enum reverse-projection
        // axes.
        for &variant in CaixaDialeto::ALL {
            let wire = variant.as_str();
            let parsed = CaixaDialeto::from_wire(wire).unwrap_or_else(|| {
                panic!(
                    "CaixaDialeto::from_wire({wire:?}) must accept every \
                     CaixaDialeto::as_str output — got None for the \
                     wire byte-string of {variant:?}"
                )
            });
            assert_eq!(
                parsed, variant,
                "CaixaDialeto::from_wire(CaixaDialeto::{variant:?}.as_str()) \
                 must return CaixaDialeto::{variant:?} — the (as_str, \
                 from_wire) pair must form a total round-trip on the \
                 closed four-arm CaixaDialeto arm-set"
            );
        }
    }

    #[test]
    fn caixa_dialeto_from_wire_rejects_unknown_byte_strings() {
        // Rejection pin on the parser's accept-set: any string outside
        // the four-arm [`CaixaDialeto::as_str`] output set must return
        // `None`. A future accidental widening of the accept-set (a
        // case-insensitive match that accepts `"pacote"` on the wire
        // axis, a hand-rolled Levenshtein-forgiving arm-lookup that
        // admits `"Pacotee"` typos, a silent acceptance of the sibling
        // [`Self::palavra_canonica`] `"defcaixa"` / `"defmolde"`
        // byte-shapes on this axis) would silently drift the parser's
        // accept-set from the emitter's — a downstream audit-report
        // re-loader that bound a prior audit's [`Self::as_str`] output
        // back to the typed enum through this parser would then bind a
        // malformed byte-string to a plausibly-wrong typed arm the
        // caller does not route through any fallback, silently
        // misclassifying the reloaded row. Also rejects the sibling
        // [`Self::palavra_canonica`] (`"defcaixa"` / `"defmolde"`) and
        // the sibling [`Self::consumidor`] (`"caixa-core / feira"`,
        // `"pleme-doc-gen"`, `"nobody known"`) byte-shapes, which are
        // the substrate's *distinct-axis* projections on the same enum
        // — the two-axis split the sibling
        // [`Self::palavra_canonica`] / [`Self::consumidor`] /
        // [`Self::descricao`] docstrings explicitly frame forbids
        // accepting one axis's byte-shapes as parseable on the other
        // axis. Peer of the sibling
        // [`crate::kind::tests::caixa_kind_from_wire_rejects_unknown_byte_strings`]
        // (2aa6d23) /
        // `placement_strategy_from_wire_rejects_unknown_byte_strings`
        // (18c7342) /
        // `dep_list_from_wire_returns_none_on_unknown_wire_scalar`
        // (45ee563) rejection pins on the sibling closed-set typed-enum
        // reverse-projection axes.
        for bad in [
            "",
            " ",
            "pacote",
            "PACOTE",
            "molde",
            "MoldePositional",
            "desconhecido",
            "Unknown",
            "defcaixa",
            "defmolde",
            "?",
            "caixa-core / feira",
            "pleme-doc-gen",
            "nobody known",
            "Pacote ",
            " Pacote",
        ] {
            assert!(
                CaixaDialeto::from_wire(bad).is_none(),
                "CaixaDialeto::from_wire({bad:?}) must return None — the \
                 parser's accept-set is exactly the four CaixaDialeto::as_str \
                 outputs; a widening would silently split the parser's \
                 accept-set from the emitter's arm-set"
            );
        }
    }

    #[test]
    fn cabeca_errada_ctor_matches_struct_literal_wrap() {
        // Fail-before-pass-after byte-identity pin: the lifted
        // [`DialetoError::cabeca_errada`] ctor MUST land on the exact
        // same struct-literal shape the pre-lift open-coded wire-up
        // block wrote by hand — `DialetoError::CabecaErrada {
        // encontrado: <head>.to_string() }`. A future accidental
        // divergence (`.into()` swap, per-arm constant substitution, an
        // added default field, an `.to_ascii_lowercase()` normalization
        // silently injected into the ctor body, a rebrand of the
        // `encontrado` field carrying a distinct byte-shape) trips this
        // pin at caixa-core build time rather than surfacing far from
        // the ctor declaration as a downstream `classify_form`
        // wrong-head consumer emitting one diagnostic shape while a
        // hand-written test peer opens another. Peer of the sibling
        // `unknown_byte_unit_ctor_matches_struct_literal_wrap`
        // (limits.rs; 29fac09) / `duplicate_from_ctor_matches_struct_
        // literal_wrap` (upgrade.rs; 7e52aec) shape on the sibling
        // single-slot `{ <field>: String }` envelope constructors.
        assert_eq!(
            DialetoError::cabeca_errada("defflake"),
            DialetoError::CabecaErrada {
                encontrado: "defflake".to_string(),
            },
            "DialetoError::cabeca_errada must byte-equal the pre-lift \
             open-coded struct-literal — a drift here means the ctor \
             stopped being a substrate primitive for the wrong-head \
             fallthrough site"
        );
    }

    #[test]
    fn cabeca_errada_routes_encontrado_verbatim_across_boundary_inputs() {
        // Fail-before-pass-after boundary-sweep pin: the lifted
        // [`DialetoError::cabeca_errada`] ctor MUST route its
        // `encontrado: &str` argument verbatim into the
        // [`DialetoError::CabecaErrada`] `encontrado: String` field
        // for every boundary-covering `&str` input — empty string, a
        // canonical `defcaixa`-adjacent head, a non-ASCII head, a
        // whitespace-carrying head, a Unicode-full-width head. Any
        // wrapper-side truncation, silent `.trim()`, accidental
        // `.to_ascii_lowercase()` normalization, or `.into()` divergence
        // on the ctor body surfaces here as a byte-mismatch against the
        // input rather than at a downstream
        // [`DialetoError::to_string()`] diagnostic-shape drift at a
        // wrong-head fallthrough consumer far from the ctor declaration.
        // Peer of the sibling `limits_codec_unit_only_ctors_route_unit_
        // verbatim_across_every_variant` (limits.rs; 29fac09) shape on
        // the sibling single-slot `{ <field>: String }` envelope
        // boundary-sweep discipline.
        for encontrado in [
            "",
            "defflake",
            "def-molde",
            "defcaixa ",
            " defcaixa",
            "μdefcaixa",
            "\u{00A0}defcaixa",
            "\u{3000}defcaixa",
            "def\u{2028}caixa",
        ] {
            let via_ctor = DialetoError::cabeca_errada(encontrado);
            let via_literal = DialetoError::CabecaErrada {
                encontrado: encontrado.to_string(),
            };
            assert_eq!(
                via_ctor, via_literal,
                "DialetoError::cabeca_errada({encontrado:?}) must byte- \
                 equal the open-coded struct-literal on the same input — \
                 a drift here would let the ctor silently normalize / \
                 truncate the head symbol before it reached the \
                 CabecaErrada envelope"
            );
            let DialetoError::CabecaErrada { encontrado: routed } = via_ctor else {
                panic!(
                    "DialetoError::cabeca_errada must construct the \
                     CabecaErrada arm — got a different variant on \
                     input {encontrado:?}"
                );
            };
            assert_eq!(
                routed, encontrado,
                "DialetoError::cabeca_errada must route the input \
                 {encontrado:?} verbatim into the encontrado field — \
                 any wrapper-side truncation / normalization surfaces \
                 here rather than at a downstream diagnostic shape drift"
            );
        }
    }

    #[test]
    fn classify_form_wrong_head_routes_through_cabeca_errada_ctor() {
        // Fail-before-pass-after routing pin: [`classify`]'s wrong-head
        // fallthrough site MUST construct its `Err(DialetoError::…)`
        // through the substrate-primitive [`DialetoError::cabeca_errada`]
        // ctor rather than through an open-coded struct-literal. Pre-
        // lift the wire-up hand-rolled a three-line
        // `Self::CabecaErrada { encontrado: other.to_string() }` block
        // with no compile-time link back to the substrate primitive; a
        // future accidental rebrand of the ctor body (an added
        // `.trim()` on `encontrado`, a per-arm constant prefix like
        // `"unknown-head:"`, a widening of the field into a
        // `(String, usize)` tuple carrying a caret offset) would then
        // silently split the two paths — the ctor consumers pick up
        // the new shape, the open-coded wire-up does not. Pinning
        // byte-equality between the observed `Err` and the ctor-
        // constructed `Err` refuses that split at caixa-core build
        // time rather than surfacing far from the wire-up commit as a
        // downstream diagnostic-consumer split.
        for head in ["defflake", "deffoobar", "defcaixaz", "let", "defmoldez"] {
            let src = format!("({head} :nome \"x\")");
            let observed = classify(&src);
            let via_ctor = Err(DialetoError::cabeca_errada(head));
            assert_eq!(
                observed, via_ctor,
                "classify({src:?}) must return the same Err shape as \
                 DialetoError::cabeca_errada({head:?}) — a drift here \
                 means the wire-up de-lifted its wrong-head fallthrough \
                 arm off the substrate primitive"
            );
        }
    }

    #[test]
    fn leitura_ctor_matches_tuple_literal_wrap_on_str_binding() {
        // Fail-before-pass-after byte-identity pin: the lifted
        // [`DialetoError::leitura`] ctor MUST land on the exact same
        // tuple-newtype wrap the pre-lift open-coded wire-up block wrote by
        // hand — `DialetoError::Leitura(<into-String-expr>)`. A future
        // accidental divergence (an added `.trim()` on the reader reason,
        // a per-arm constant prefix like `"tatara-lisp:"`, a widening of
        // the tuple carrying a caret offset, a rebrand of the payload
        // carrying a distinct byte-shape) trips this pin at caixa-core
        // build time rather than surfacing far from the ctor declaration
        // as a downstream [`classify`] tatara-lisp-reader consumer
        // emitting one diagnostic shape while a hand-written test peer
        // opens another. Peer of the sibling
        // `cabeca_errada_ctor_matches_struct_literal_wrap` pin above on
        // the same [`DialetoError`] envelope's wrong-head axis, and of
        // the peer `LimitsError::empty_byte_size` /
        // `LimitsError::empty_duration` (7a4b003 / 319216c) shape on the
        // sibling `(String)` single-slot tuple-newtype envelope
        // constructors.
        let reason: &str = "unclosed paren at 1:12";
        assert_eq!(
            DialetoError::leitura(reason),
            DialetoError::Leitura(reason.to_string()),
            "DialetoError::leitura must byte-equal the pre-lift open-coded \
             tuple-newtype wrap — a drift here means the ctor stopped \
             being a substrate primitive for the tatara-lisp-reader \
             fallthrough site"
        );
    }

    #[test]
    fn leitura_ctor_matches_tuple_literal_wrap_on_string_binding() {
        // Fail-before-pass-after byte-identity pin on the `String` wire-up
        // shape: the lifted [`DialetoError::leitura`] ctor MUST land on
        // the same tuple-newtype wrap when the caller passes an owned
        // `String` (the actual [`classify`] wire-up shape — `e.to_string()`
        // on a [`tatara_lisp::Error`]-carrying binding). Pins that the
        // `impl Into<String>` param covers the owned-`String` path with no
        // silent double-allocation or intermediate `&str` reslicing. Peer
        // of the sibling `_on_str_binding` pin above — together they close
        // the `impl Into<String>` bound's two authored wire-up shapes on
        // the ctor's substrate primitive.
        let reason: String = String::from("read: unexpected EOF at 3:1");
        let via_ctor = DialetoError::leitura(reason.clone());
        let via_literal = DialetoError::Leitura(reason.clone());
        assert_eq!(
            via_ctor, via_literal,
            "DialetoError::leitura must byte-equal the pre-lift open-coded \
             tuple-newtype wrap on the same owned-String fixture — a drift \
             here would let the ctor silently reshape the reader reason \
             before it reached the Leitura envelope"
        );
        let DialetoError::Leitura(routed) = via_ctor else {
            panic!(
                "DialetoError::leitura must construct the Leitura arm — \
                 got a different variant on input {reason:?}"
            );
        };
        assert_eq!(
            routed, reason,
            "DialetoError::leitura must route the input {reason:?} \
             verbatim into the tuple-newtype payload — any wrapper-side \
             truncation / normalization surfaces here rather than at a \
             downstream diagnostic shape drift"
        );
    }

    #[test]
    fn leitura_routes_reason_verbatim_across_boundary_inputs() {
        // Fail-before-pass-after boundary-sweep pin: the lifted
        // [`DialetoError::leitura`] ctor MUST route its
        // `reason: impl Into<String>` argument verbatim into the
        // [`DialetoError::Leitura`] tuple-newtype `String` payload for
        // every boundary-covering input — empty string, a canonical
        // tatara-lisp reader error, a non-ASCII reason, a
        // whitespace-carrying reason, a Unicode-full-width reason. Any
        // wrapper-side truncation, silent `.trim()`, accidental
        // `.to_ascii_lowercase()` normalization, or `.into()` divergence
        // on the ctor body surfaces here as a byte-mismatch against the
        // input rather than at a downstream [`DialetoError::to_string()`]
        // diagnostic-shape drift at a tatara-lisp-reader fallthrough
        // consumer far from the ctor declaration. Peer of the sibling
        // `cabeca_errada_routes_encontrado_verbatim_across_boundary_inputs`
        // pin above on the same [`DialetoError`] envelope's wrong-head
        // axis.
        for reason in [
            "",
            "unclosed paren at 1:12",
            "unexpected token ')'",
            "read: eof",
            " leading whitespace",
            "trailing whitespace ",
            "μnicode reason",
            "\u{00A0}NBSP-prefixed reason",
            "\u{3000}ideographic-space reason",
            "reason\u{2028}with-line-separator",
        ] {
            let via_ctor = DialetoError::leitura(reason);
            let via_literal = DialetoError::Leitura(reason.to_string());
            assert_eq!(
                via_ctor, via_literal,
                "DialetoError::leitura({reason:?}) must byte-equal the \
                 open-coded tuple-newtype wrap on the same input — a \
                 drift here would let the ctor silently normalize / \
                 truncate the reader reason before it reached the \
                 Leitura envelope"
            );
            let DialetoError::Leitura(routed) = via_ctor else {
                panic!(
                    "DialetoError::leitura must construct the Leitura \
                     arm — got a different variant on input {reason:?}"
                );
            };
            assert_eq!(
                routed, reason,
                "DialetoError::leitura must route the input {reason:?} \
                 verbatim into the tuple-newtype payload — any \
                 wrapper-side truncation / normalization surfaces here \
                 rather than at a downstream diagnostic shape drift"
            );
        }
    }

    #[test]
    fn classify_reader_error_routes_through_leitura_ctor() {
        // Fail-before-pass-after routing pin: [`classify`]'s
        // tatara-lisp-reader map-err site MUST construct its
        // `Err(DialetoError::…)` through the substrate-primitive
        // [`DialetoError::leitura`] ctor rather than through an
        // open-coded tuple-newtype wrap. Pre-lift the wire-up hand-rolled
        // a `Self::Leitura(e.to_string())` block with no compile-time
        // link back to the substrate primitive; a future accidental
        // rebrand of the ctor body (an added `.trim()` on the reader
        // reason, a per-arm constant prefix like `"tatara-lisp:"`, a
        // widening of the payload into a `(String, usize)` tuple
        // carrying a caret offset) would then silently split the two
        // paths — the ctor consumers pick up the new shape, the
        // open-coded wire-up does not. Pinning byte-equality between
        // the observed `Err` and the ctor-constructed `Err` refuses
        // that split at caixa-core build time rather than surfacing far
        // from the wire-up commit as a downstream diagnostic-consumer
        // split. Peer of the sibling
        // `classify_form_wrong_head_routes_through_cabeca_errada_ctor`
        // pin above on the same [`DialetoError`] envelope's wrong-head
        // fallthrough axis.
        //
        // The malformed sources below each name a distinct
        // tatara-lisp-reader failure shape (unclosed paren, stray close
        // paren, unterminated string), so together they sweep the
        // reader's rejection surface rather than pinning against one
        // specific error message the reader upstream is free to reword.
        for src in [
            "(defcaixa :nome \"x\"",
            "defcaixa :nome \"x\")",
            "(defcaixa :nome \"unterminated",
        ] {
            let observed = classify(src);
            let Err(DialetoError::Leitura(reason)) = observed.clone() else {
                panic!(
                    "classify({src:?}) must return the Leitura arm — got \
                     {observed:?}"
                );
            };
            let via_ctor: Result<CaixaDialeto, DialetoError> =
                Err(DialetoError::leitura(reason.clone()));
            assert_eq!(
                observed, via_ctor,
                "classify({src:?}) must return the same Err shape as \
                 DialetoError::leitura({reason:?}) — a drift here means \
                 the wire-up de-lifted its tatara-lisp-reader fallthrough \
                 arm off the substrate primitive"
            );
        }
    }

    #[test]
    fn caixa_dialeto_try_from_str_routes_through_from_wire_accessor() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl TryFrom<&str> for CaixaDialeto`: for every arm in
        // [`CaixaDialeto::ALL`], the `.try_into()` / `TryFrom::try_from`
        // path must resolve to the same variant the sibling
        // [`CaixaDialeto::from_wire`] resolver returns on the same
        // [`CaixaDialeto::as_str`] wire byte-string input. Pins the
        // three-path convergence discipline the [`CaixaDialeto`] closed-
        // set typed enum now carries on the `str → Self` reverse-
        // projection axis: `<CaixaDialeto as TryFrom<&str>>::try_from(s)`
        // (the newly lifted trait-idiomatic reverse projection),
        // `CaixaDialeto::from_wire(s)` (the substrate-primitive method-
        // named `Option<Self>` accessor the trait impl delegates through),
        // and the round-trip identity `variant.as_str() → variant`
        // (the four-arm closed accept-set shared between the emitter and
        // both reverse-projection consumers) must resolve to the same
        // typed [`CaixaDialeto`] discriminator on every arm.
        //
        // A future silent detour that routes the impl through a
        // divergent projection (a per-arm inline
        // `match s { "Pacote" => …, … }` re-inlining that opens a
        // compile-time link to the un-lifted arm-literal, a swap onto
        // the second-axis [`CaixaDialeto::palavra_canonica`] /
        // [`CaixaDialeto::consumidor`] / [`CaixaDialeto::descricao`]
        // accessors that carry distinct byte-shapes per axis, an accept-
        // set widening that silently accepts one axis's byte-shapes as
        // parseable on the other axis) trips at caixa-core test time
        // under `assert_eq!` rather than at a downstream
        // `TryFrom<&str>`-bound consumer's silent split. Peer of the
        // sibling
        // [`crate::kind::tests::caixa_kind_try_from_str_routes_through_from_wire_accessor`]
        // (3c83606) on the top-level [`crate::CaixaKind`] closed-set
        // discriminator's reverse-projection axis — extends the trait-
        // idiomatic reverse-projection axis onto the seventh closed-set
        // fieldless typed enum on the caixa surface (the second one to
        // carry the paired `TryFrom<&str>` impl).
        for &variant in CaixaDialeto::ALL {
            let wire = variant.as_str();
            let via_try_from: CaixaDialeto = <CaixaDialeto as TryFrom<&str>>::try_from(wire)
                .unwrap_or_else(|()| {
                    panic!(
                        "CaixaDialeto::try_from({wire:?}) must accept every \
                         CaixaDialeto::as_str output — got Err(()) for the \
                         wire byte-string of {variant:?}"
                    )
                });
            let via_from_wire: CaixaDialeto = CaixaDialeto::from_wire(wire).unwrap_or_else(|| {
                panic!(
                    "CaixaDialeto::from_wire({wire:?}) must accept every \
                     CaixaDialeto::as_str output — got None for the wire \
                     byte-string of {variant:?}"
                )
            });
            assert_eq!(
                via_try_from, variant,
                "CaixaDialeto::try_from(CaixaDialeto::{variant:?}.as_str()) \
                 must return CaixaDialeto::{variant:?} — the trait-idiomatic \
                 reverse projection must land on the same arm the method-named \
                 from_wire resolver does",
            );
            assert_eq!(
                via_try_from, via_from_wire,
                "CaixaDialeto::try_from({wire:?}) ({via_try_from:?}) must \
                 byte-equal CaixaDialeto::from_wire({wire:?}) ({via_from_wire:?}) \
                 on the same input — divergence signals a silent detour off the \
                 shared substrate-primitive resolver",
            );
            assert_eq!(
                <CaixaDialeto as TryFrom<&str>>::try_from(wire).ok(),
                CaixaDialeto::from_wire(wire),
                "the Result::ok() projection of TryFrom<&str> must byte-equal \
                 the sibling from_wire Option<Self> output on {wire:?} — the \
                 two accessors must share the same accept-set and typed \
                 outcome per arm",
            );
        }
    }

    #[test]
    fn caixa_dialeto_try_from_str_rejects_unknown_byte_strings() {
        // Rejection witness on the trait-idiomatic reverse-projection
        // axis: any string outside the four-arm [`CaixaDialeto::as_str`]
        // output set must resolve to `Err(())` through the lifted
        // [`impl TryFrom<&str> for CaixaDialeto`]. A future accidental
        // widening of the accept-set (a case-insensitive match that
        // accepts `"pacote"` on the wire axis, a hand-rolled Levenshtein-
        // forgiving arm-lookup that admits `"Pacotee"` typos, a silent
        // acceptance of the sibling [`CaixaDialeto::palavra_canonica`]
        // `"defcaixa"` / `"defmolde"` byte-shapes on this axis, a swap
        // onto the [`CaixaDialeto::consumidor`] `"pleme-doc-gen"` /
        // `"caixa-core / feira"` / `"nobody known"` byte-shapes) would
        // silently drift the trait-idiomatic parser's accept-set from
        // the sibling [`CaixaDialeto::from_wire`] resolver's — a
        // downstream `TryFrom<&str>`-bound consumer binding a malformed
        // byte-string through this impl would then bind a plausibly-
        // wrong typed arm the caller does not route through any fallback,
        // silently misclassifying the reloaded row.
        //
        // Sweeps the same rejection set the sibling
        // [`caixa_dialeto_from_wire_rejects_unknown_byte_strings`] pin
        // walks (the shared `from_wire` resolver both accessors delegate
        // through) so the trait-idiomatic axis and the method-named axis
        // stay locked to the same accept-set by construction. Peer of the
        // sibling
        // [`crate::kind::tests::caixa_kind_try_from_str_rejects_unknown_byte_strings`]
        // (3c83606) on the top-level [`crate::CaixaKind`] closed-set
        // discriminator's trait-idiomatic reverse-projection axis.
        for bad in [
            "",
            " ",
            "pacote",
            "PACOTE",
            "molde",
            "MoldePositional",
            "desconhecido",
            "Unknown",
            "defcaixa",
            "defmolde",
            "?",
            "caixa-core / feira",
            "pleme-doc-gen",
            "nobody known",
            "Pacote ",
            " Pacote",
        ] {
            assert_eq!(
                <CaixaDialeto as TryFrom<&str>>::try_from(bad),
                Err(()),
                "CaixaDialeto::try_from({bad:?}) must return Err(()) — the \
                 trait-idiomatic parser's accept-set is exactly the four \
                 CaixaDialeto::as_str outputs; a widening would silently \
                 split the trait-idiomatic reverse-projection axis from the \
                 sibling from_wire resolver's arm-set"
            );
        }
    }

    #[test]
    fn caixa_dialeto_from_into_static_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<CaixaDialeto> for &'static str` — asserts the
        // standard-library trait impl and the substrate-primitive
        // [`CaixaDialeto::as_str`] `pub const fn` accessor resolve to
        // the same four-arm emit-set across every arm the exhaustive
        // [`CaixaDialeto::ALL`] slice enumerates. Any future silent
        // detour that routes the trait impl through a divergent
        // projection (a per-arm inline `match dialeto { Pacote =>
        // "Pacote", … }` re-inlining that opens a compile-time link to
        // the un-lifted arm-literal, an accidental swap onto the second-
        // axis [`CaixaDialeto::palavra_canonica`] /
        // [`CaixaDialeto::consumidor`] / [`CaixaDialeto::descricao`]
        // accessors that carry distinct byte-shapes per axis) trips at
        // caixa-core test time under `assert_eq!` rather than at a
        // downstream `impl Into<&'static str>`-bound consumer's silent
        // split. Sweeps every one of the four arms [`CaixaDialeto::ALL`]
        // carries so no arm's projection is covered only by the sibling
        // method-named `as_str` / [`std::fmt::Display`] / [`AsRef<str>`]
        // paths. Materializes the `<&'static str as
        // From<CaixaDialeto>>::from` output in a `const`-shape binding
        // to make the `'static` lifetime promise a build-time invariant
        // — a future accidental downgrade of any of the four arms'
        // returned literals to a non-`&'static str` (a `String::leak()`-
        // produced return, a `Box::leak`-cast, an intermediate lifetime-
        // erasing helper) trips at caixa-core build time rather than at
        // a downstream `'static`-bound consumer. Peer of the sibling
        // [`crate::supervisor::tests::restart_strategy_from_into_static_str_routes_through_as_str_accessor`]
        // (523157d) /
        // [`crate::supervisor::tests::restart_policy_from_into_static_str_routes_through_as_str_accessor`]
        // (9fb37d0) /
        // [`crate::kind::tests::caixa_kind_from_into_static_str_routes_through_as_str_accessor`]
        // (edb827b) pins on the sibling closed-set typed-enum forward-
        // projection axes — extends the trait-idiomatic forward-
        // projection axis onto the fourth closed-set fieldless typed
        // enum on the caixa surface (the dialect-classification axis,
        // second-of-two closed-set typed enums in caixa-core outside
        // the OTP-shape M2 slot).
        const PACOTE: &str = CaixaDialeto::Pacote.as_str();
        const MOLDE: &str = CaixaDialeto::Molde.as_str();
        const MOLDE_POSICIONAL: &str = CaixaDialeto::MoldePosicional.as_str();
        const DESCONHECIDO: &str = CaixaDialeto::Desconhecido.as_str();
        for &variant in CaixaDialeto::ALL {
            let via_trait: &'static str = <&'static str as From<CaixaDialeto>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<CaixaDialeto> for &'static str impl must round-trip \
                 CaixaDialeto::{variant:?} to the same `PascalCase` byte-string \
                 CaixaDialeto::as_str returns — divergence signals a silent \
                 detour off the substrate-primitive accessor"
            );
            let via_into: &'static str = variant.into();
            assert_eq!(
                via_into, via_method,
                "Into<&'static str>::into on CaixaDialeto::{variant:?} must \
                 byte-equal CaixaDialeto::as_str on the same input — the \
                 blanket-derived Into shape must resolve to the same as_str \
                 dispatch as the explicit From impl"
            );
        }
        assert_eq!(
            [PACOTE, MOLDE, MOLDE_POSICIONAL, DESCONHECIDO],
            ["Pacote", "Molde", "MoldePosicional", "Desconhecido"],
            "const-context CaixaDialeto::as_str must resolve to the four \
             `PascalCase` variant-name byte-strings — a future accidental \
             downgrade of any arm to a non-const or non-static byte-string \
             breaks the `&'static str`-lifetime promise the paired \
             From<CaixaDialeto> for &'static str impl carries by \
             construction"
        );
    }

    #[test]
    fn caixa_dialeto_from_into_static_str_and_as_str_partition_the_emit_set() {
        // Cross-axis partition pin: the paired trait-idiomatic
        // `From<CaixaDialeto> for &'static str` forward projection and
        // the method-named [`CaixaDialeto::as_str`] forward projection
        // must resolve identically on *every* arm, not just the ones
        // named in the primary byte-parity pin above. Sweeps every
        // [`CaixaDialeto::ALL`] arm and asserts the trait's `From::from`
        // output byte-equals the method-named accessor's return-value on
        // each, locking the two forward-projection paths together by
        // construction so any future detour (a stray `From` special-case
        // that lands on a divergent per-arm literal outside the paired
        // `as_str` dispatch, a hypothetical rebrand touching one axis
        // without the other) trips at caixa-core test time. Peer of the
        // sibling forward-projection partition pins
        // [`crate::supervisor::tests::restart_strategy_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (523157d) /
        // [`crate::supervisor::tests::restart_policy_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (9fb37d0) /
        // [`crate::kind::tests::caixa_kind_from_into_static_str_and_as_str_partition_the_emit_set`]
        // (edb827b) — extends the round-trip discipline onto the fourth
        // closed-set typed enum on the caixa surface, closing the two-way
        // `Self ↔ &'static str` round-trip on the trait-idiomatic pair
        // (`From<Self> for &'static str` + `TryFrom<&str> for Self`) as
        // well as the pre-existing method-named pair (`as_str` +
        // `from_wire`).
        for &variant in CaixaDialeto::ALL {
            let via_trait: &'static str = <&'static str as From<CaixaDialeto>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<CaixaDialeto> for &'static str and \
                 CaixaDialeto::as_str must resolve identically on \
                 CaixaDialeto::{variant:?} — divergence signals the \
                 two forward-projection paths have drifted onto different \
                 emit-sets"
            );
        }
        // Round-trip witness: every arm's forward `From` output re-parses
        // through the paired trait-idiomatic reverse `TryFrom<&str>` back
        // to the original variant. Closes the two-way `CaixaDialeto ↔
        // &'static str` round-trip on the trait-idiomatic axis pair
        // directly (no wire-vocab intermediate the peer [`CaixaKind`]
        // axis pair requires — the emit-side [`CaixaDialeto::as_str`]
        // and the parse-side [`CaixaDialeto::from_wire`] share the same
        // `PascalCase` byte-string vocabulary by construction), mirroring
        // the pre-existing method-named `as_str` + `from_wire` round-trip
        // on the substrate-primitive axis pair.
        for &variant in CaixaDialeto::ALL {
            let emitted: &'static str = variant.into();
            let re_parsed: Result<CaixaDialeto, ()> =
                <CaixaDialeto as TryFrom<&str>>::try_from(emitted);
            assert_eq!(
                re_parsed,
                Ok(variant),
                "trait-idiomatic axis pair must round-trip \
                 CaixaDialeto::{variant:?} through `.into::<&'static \
                 str>()` and back through `TryFrom<&str>` — a break signals \
                 the forward-emit and reverse-parse axes have drifted onto \
                 different vocabularies"
            );
        }
    }

    #[test]
    fn caixa_dialeto_is_molde_family_routes_through_is_variant_derived_predicates() {
        // Byte-parity pin on the post-lift [`CaixaDialeto::is_molde_family`]
        // convergence: for every arm in [`CaixaDialeto::ALL`], the typed
        // predicate must byte-equal the direct
        // `self.is_molde() || self.is_molde_posicional()` composition of
        // the two derived per-arm predicates. Pre-lift the predicate
        // hand-rolled `matches!(self, Self::Molde | Self::MoldePosicional)`
        // with no compile-time link back to the closed-set typed dispatch;
        // post-lift it routes through the derived predicates so a future
        // arm rename or `#[is_variant(name = "…")]` override lands at
        // exactly one dispatch on the substrate primitive. Pinning the
        // byte-equality here refuses a future accidental split between
        // the composed predicate and the paired derived predicates
        // (a hand-rolled shadow `impl` that overrides one path but not
        // the other, an accidental rebrand of `is_molde_family`'s body
        // back to the pre-lift `matches!` form) at caixa-core build time.
        for &d in CaixaDialeto::ALL {
            let via_derived = d.is_molde() || d.is_molde_posicional();
            let via_is_molde_family = d.is_molde_family();
            assert_eq!(
                via_is_molde_family, via_derived,
                "CaixaDialeto::{d:?}.is_molde_family() ({via_is_molde_family}) \
                 must byte-equal the composed derived predicates \
                 is_molde() || is_molde_posicional() ({via_derived}) — a \
                 split between the composed predicate and its derived \
                 building blocks would let a future arm rename land at one \
                 path and drift at the other, which is exactly the drift \
                 the IsVariant lift refuses"
            );
        }
    }

    #[test]
    fn caixa_dialeto_from_borrowed_into_static_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl From<&CaixaDialeto> for &'static str` — asserts the
        // borrowed-input standard-library trait impl and the substrate-
        // primitive [`CaixaDialeto::as_str`] `pub const fn` accessor
        // resolve to the same four-arm emit-set across every arm the
        // exhaustive [`CaixaDialeto::ALL`] slice enumerates. Rust's
        // `From` trait does not auto-derive the borrowed-input sibling
        // from a paired owned-input impl (no `impl<T, U> From<&T> for U
        // where T: Copy, U: From<T>` blanket in `core`), so the
        // borrowed-input axis is a distinct trait-idiomatic surface that
        // a `.iter().map(Into::into)` shape over [`CaixaDialeto::ALL`]
        // (whose iterator yields `&CaixaDialeto`, not `CaixaDialeto`)
        // reaches through this impl and no other — the paired owned-
        // input [`From<CaixaDialeto>`] impl requires an explicit
        // `.copied()` / dereference before the trait fires.
        // Materializes the `<&'static str as From<&CaixaDialeto>>::from`
        // output in a `const`-shape binding to make the `'static`
        // lifetime promise a build-time invariant — a future accidental
        // downgrade of any of the four arms' returned literals to a
        // non-`&'static str` trips at caixa-core build time rather than
        // at a downstream `'static`-bound consumer. Peer of the sibling
        // [`crate::dep::tests::dep_list_from_borrowed_into_static_str_routes_through_as_str_accessor`]
        // (64aa742) /
        // [`crate::kind::tests::caixa_kind_from_borrowed_into_static_str_routes_through_as_str_accessor`]
        // (5ab993a) pins on the sibling closed-set typed-enum borrowed-
        // input forward-projection axes — extends the borrowed-input
        // axis discipline onto the third peer on the substrate-wide
        // campaign, the dialect-classification axis.
        const PACOTE: &str = CaixaDialeto::Pacote.as_str();
        const MOLDE: &str = CaixaDialeto::Molde.as_str();
        const MOLDE_POSICIONAL: &str = CaixaDialeto::MoldePosicional.as_str();
        const DESCONHECIDO: &str = CaixaDialeto::Desconhecido.as_str();
        for variant in CaixaDialeto::ALL {
            let via_trait: &'static str = <&'static str as From<&CaixaDialeto>>::from(variant);
            let via_method: &'static str = variant.as_str();
            assert_eq!(
                via_trait, via_method,
                "From<&CaixaDialeto> for &'static str impl must round-trip \
                 &CaixaDialeto::{variant:?} to the same `PascalCase` byte-\
                 string CaixaDialeto::as_str returns — divergence signals a \
                 silent detour off the substrate-primitive accessor"
            );
            let via_into: &'static str = variant.into();
            assert_eq!(
                via_into, via_method,
                "Into<&'static str>::into on &CaixaDialeto::{variant:?} must \
                 byte-equal CaixaDialeto::as_str on the same input — the \
                 blanket-derived Into shape must resolve to the same as_str \
                 dispatch as the explicit From impl"
            );
        }
        assert_eq!(
            [PACOTE, MOLDE, MOLDE_POSICIONAL, DESCONHECIDO],
            ["Pacote", "Molde", "MoldePosicional", "Desconhecido"],
            "const-context CaixaDialeto::as_str must resolve to the four \
             `PascalCase` variant-name byte-strings — the borrowed-input \
             From<&CaixaDialeto> for &'static str impl inherits its \
             `'static` lifetime promise from the same accessor the owned-\
             input sibling routes through"
        );
    }

    #[test]
    fn caixa_dialeto_from_owned_and_borrowed_into_static_str_agree_on_every_arm() {
        // Cross-axis partition pin: the paired trait-idiomatic
        // owned-input `From<CaixaDialeto> for &'static str` and
        // borrowed-input `From<&CaixaDialeto> for &'static str` (this
        // lift) forward projections must resolve identically on every
        // arm, locking the two input-shape paths together so any future
        // detour trips at caixa-core test time. Then a witness that a
        // `.iter().map(Into::into)` pipe over [`CaixaDialeto::ALL`]
        // (whose iterator yields `&CaixaDialeto`) materializes the four-
        // arm accept-set through the borrowed-input axis alone — the
        // exact shape a future M4 admission-webhook rejection body
        // composer, a future substrate-wide per-arm diagnostic column,
        // or a `HashMap::<&'static str, CaixaDialeto>::from_iter(
        //     CaixaDialeto::ALL.iter().map(|d| (d.into(), *d)))`-style
        // per-dialect lookup reaches through — closing the two-way
        // owned/borrowed input-shape symmetry on the forward-projection
        // trait-idiomatic axis. Peer of the sibling
        // [`crate::dep::tests::dep_list_from_owned_and_borrowed_into_static_str_agree_on_every_arm`]
        // (64aa742) /
        // [`crate::kind::tests::caixa_kind_from_owned_and_borrowed_into_static_str_agree_on_every_arm`]
        // (5ab993a) partition pins — extends the borrowed-input axis
        // discipline onto the third peer on the substrate-wide campaign.
        for &variant in CaixaDialeto::ALL {
            let owned: &'static str = <&'static str as From<CaixaDialeto>>::from(variant);
            let borrowed: &'static str = <&'static str as From<&CaixaDialeto>>::from(&variant);
            assert_eq!(
                owned, borrowed,
                "From<CaixaDialeto> and From<&CaixaDialeto> for &'static str \
                 must resolve identically on CaixaDialeto::{variant:?} — \
                 divergence signals the owned-input and borrowed-input \
                 forward-projection paths have drifted onto different \
                 emit-sets"
            );
        }
        let via_iter: Vec<&'static str> = CaixaDialeto::ALL.iter().map(Into::into).collect();
        let via_method: Vec<&'static str> = CaixaDialeto::ALL.iter().map(|d| d.as_str()).collect();
        assert_eq!(
            via_iter, via_method,
            "`.iter().map(Into::into)` over CaixaDialeto::ALL must byte-\
             equal `.iter().map(|d| d.as_str())` on every arm — the \
             borrowed-input `From<&CaixaDialeto> for &'static str` axis \
             is what makes the `.iter().map(Into::into)` shape route \
             through the substrate-primitive `CaixaDialeto::as_str` \
             accessor rather than through a per-call-site `.copied()` / \
             dereference detour"
        );
        // Direct round-trip witness on the borrowed-input axis: every
        // arm's borrowed `From` output re-parses through the paired
        // trait-idiomatic reverse `TryFrom<&str>` back to the original
        // variant. Unlike the peer [`crate::CaixaKind`] axis pair
        // (whose forward `From<Self> for &'static str` emits the
        // lowercase Portuguese `as_str` diagnostic vocabulary while
        // the reverse `TryFrom<&str>` parses the `PascalCase`
        // `wire_name` author-surface vocabulary, forcing the round-trip
        // through an intermediate wire-vocab hop), [`CaixaDialeto`]'s
        // [`CaixaDialeto::as_str`] emit and [`CaixaDialeto::from_wire`]
        // parse share the same `PascalCase` vocabulary by construction,
        // so the borrowed-input forward axis and the reverse axis
        // compose directly.
        for &variant in CaixaDialeto::ALL {
            let borrowed: &'static str = <&'static str as From<&CaixaDialeto>>::from(&variant);
            let re_parsed: Result<CaixaDialeto, ()> =
                <CaixaDialeto as TryFrom<&str>>::try_from(borrowed);
            assert_eq!(
                re_parsed,
                Ok(variant),
                "trait-idiomatic borrowed-input round-trip must project \
                 &CaixaDialeto::{variant:?} through \
                 `<&'static str>::from(&variant)` and back through \
                 `TryFrom<&str>` — a break signals the borrowed-input \
                 forward-emit axis and the reverse-parse axis have \
                 drifted onto different vocabularies"
            );
        }
    }
}
