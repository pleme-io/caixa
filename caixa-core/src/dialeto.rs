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
}

impl std::fmt::Display for CaixaDialeto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Pacote => "Pacote",
            Self::Molde => "Molde",
            Self::MoldePosicional => "MoldePosicional",
            Self::Desconhecido => "Desconhecido",
        })
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
        let pos = r#"(defmolde todoku-go :kind :Biblioteca :ecosystem :go)"#;
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
        // collision this module closes.
        for d in [
            CaixaDialeto::Pacote,
            CaixaDialeto::Molde,
            CaixaDialeto::MoldePosicional,
            CaixaDialeto::Desconhecido,
        ] {
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
}
