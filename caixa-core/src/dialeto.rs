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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
        matches!(self, Self::Molde | Self::MoldePosicional)
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
    let forms = tatara_lisp::read(src).map_err(|e| DialetoError::Leitura(e.to_string()))?;
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
            return Err(DialetoError::CabecaErrada {
                encontrado: other.to_string(),
            });
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
            Err(DialetoError::CabecaErrada {
                encontrado: "defflake".into()
            })
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
}
