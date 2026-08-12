use crate::span::Span;
use crate::trivia::Trivia;

/// A parsed Lisp node with span + attached trivia.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub kind: NodeKind,
    pub span: Span,
    /// Comments / blank lines immediately before this node.
    pub leading: Vec<Trivia>,
    /// For a compound node: trivia sitting between its last child and its
    /// closing delimiter, with no child to attach to. Emitted INSIDE the
    /// form, before the `)`.
    ///
    /// Note this slot is overloaded relative to its original meaning
    /// ("trailing on the same line"); `sequence()` claimed it for the
    /// dangling case. That is why [`Self::after`] exists rather than this
    /// being reused again.
    pub trailing: Vec<Trivia>,
    /// Trivia that follows this node at its own level — OUTSIDE any
    /// delimiter it owns.
    ///
    /// The distinction from [`Self::trailing`] is load-bearing, not
    /// pedantry: `(define x 1) ; why` and `(define x 1 ; why\n)` are
    /// different documents, and a single slot cannot represent both. With
    /// only the two original slots the top-level case had nowhere to go
    /// and was DISCARDED at EOF — measurably: one mass-format destroyed 44
    /// trailing comments in `pleme-io/actions` alone.
    pub after: Vec<Trivia>,
}

/// The typed variant discriminator on the caixa-ast surface — every
/// [`Node`]'s carrying-shape (atom family, compound family, quote family)
/// projects through this closed thirteen-arm partition.
///
/// The [`gen_platform::IsVariant`] derive emits per-arm arm-discriminator
/// predicates — [`Self::is_nil`], [`Self::is_symbol`], [`Self::is_keyword`],
/// [`Self::is_str`], [`Self::is_int`], [`Self::is_float`], [`Self::is_bool`],
/// [`Self::is_list`], [`Self::is_map`], [`Self::is_vector`], [`Self::is_quote`],
/// [`Self::is_quasiquote`], [`Self::is_unquote`], [`Self::is_unquote_splice`]
/// — so every downstream consumer that only needs the arm-discriminator
/// projection (not the borrowed field value) reaches for one typed dispatch
/// on the substrate primitive rather than a hand-rolled
/// `matches!(x.kind, NodeKind::X(_))` literal. Peer of the caixa-core
/// [`caixa_core::CaixaKind`] / [`caixa_core::CaixaDialeto`] /
/// [`caixa_core::DepList`] / [`caixa_core::UpgradeInstruction`] /
/// caixa-lint / caixa-arch / caixa-provedor / caixa-theme sibling enums
/// that already carry the `gen_platform::IsVariant` discipline — the first
/// closed-set-typed-enum lift on the caixa-ast surface, extending the
/// discipline onto the AST-node-family axis every downstream authoring
/// consumer (`caixa-fmt`, `caixa-lint`, `caixa-lsp`) partitions on.
#[derive(Debug, Clone, PartialEq, gen_platform::IsVariant)]
pub enum NodeKind {
    Nil,
    Symbol(String),
    Keyword(String),
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    List(Vec<Node>),
    /// `{ :k v … }` — the brace dialect. REAL SYNTAX per
    /// theory/TATARA-LISP-CONSOLIDATION.md D4; 62 live caixa.lisp
    /// manifests author nested maps and are consumed today.
    Map(Vec<Node>),
    /// `[ a b … ]` — the vector dialect, D4's sibling.
    Vector(Vec<Node>),
    Quote(Box<Node>),
    Quasiquote(Box<Node>),
    Unquote(Box<Node>),
    UnquoteSplice(Box<Node>),
}

impl NodeKind {
    /// Substrate-canonical projection onto the [`Self::Keyword`] arm's
    /// borrowed scalar payload — returns `Some(&str)` byte-borrowed from
    /// the arm's own [`String`] storage, and [`None`] on every other arm
    /// of the closed fourteen-arm [`NodeKind`] variant set.
    ///
    /// Six production consumers today across two caixa-monorepo crates
    /// — the [`Node::kwarg`] pair-loop `:key value` alternator, and the
    /// caixa-lint rule surface's [`caixa_lint::rules`]::`check_keyword_kebab`
    /// walker, `check_enum_pascal` kwarg-loop filter, `keyword_present`
    /// walker, `matches_kwarg` kwarg-loop filter, and `items_has_key`
    /// kwarg-loop filter — which previously reached the underlying
    /// keyword-name scalar through six raw `if let NodeKind::Keyword(k)
    /// = &n.kind` (or `matches!(&n.kind, NodeKind::Keyword(k) if k ==
    /// key)`) open-coded per-arm pattern-matches that expressed no
    /// compile-time link back to the substrate primitive's typed
    /// scalar-arm projection. A future `NodeKind` arm addition (a
    /// `TaggedKeyword(String, KeywordTag)` shape once the tatara-lisp
    /// reader grows a per-keyword scope tag, a `NamespacedKeyword(String,
    /// String)` shape once the sexp→JSON bridge stabilizes the
    /// `::ns/key` sugar theory/TATARA-LISP-CONSOLIDATION.md D6 sketches)
    /// reaches every downstream per-`Keyword`-arm consumer through this
    /// one dispatch by construction — no coordinated six-way rewrite
    /// across every per-rule projection site.
    ///
    /// Zero-copy — the returned `&str` borrows from the arm's own
    /// [`String`] storage (pinned by the
    /// `as_keyword_is_by_borrow_pointer_identity` test), the same
    /// discipline as the sibling [`caixa_teia::TeiaValue::as_str`]
    /// (7304ffe) / [`caixa_teia::TeiaValue::as_object`] (7304ffe)
    /// outer-`TeiaValue` sum-type per-arm projections and the sibling
    /// [`caixa_teia::TeiaRefRepr::tipo`] (a856d67) /
    /// [`caixa_teia::TeiaRefRepr::nome`] (15bcdef) /
    /// [`caixa_teia::TeiaRefRepr::atributo`] outer-`TeiaRefRepr` scalar
    /// accessors on the substrate's IaC-side per-`(ref …)` reference
    /// carrier — one axis level up on the sibling AST-side per-`NodeKind`
    /// sum-type projection surface.
    ///
    /// First `Option<&<payload>>` projection accessor on the outer
    /// [`NodeKind`] sum-type — opens the `as_<variant>` typed projection
    /// family the sibling per-arm [`Self::as_symbol`] (e96eea1) and
    /// [`Self::as_str`] projections fold on the same shape at their
    /// consumer surfaces, extending the discipline onto the caixa-ast
    /// per-AST-node-family arm-set every downstream authoring consumer
    /// (`caixa-fmt`, `caixa-lint`, `caixa-lsp`) partitions on.
    #[must_use]
    pub fn as_keyword(&self) -> Option<&str> {
        match self {
            Self::Keyword(k) => Some(k.as_str()),
            _ => None,
        }
    }

    /// Substrate-canonical projection onto the [`Self::Symbol`] arm's
    /// borrowed scalar payload — returns `Some(&str)` byte-borrowed from
    /// the arm's own [`String`] storage, and [`None`] on every other arm
    /// of the closed fourteen-arm [`NodeKind`] variant set.
    ///
    /// Eight production consumers today across four caixa-monorepo crates
    /// — the caixa-ast [`Node::head_symbol`] list-head projection, the
    /// caixa-fmt printer's `special_head_arity` head-lookup and
    /// `head_symbol_is_command` command-head gate, the caixa-lint rule
    /// surface's `check_paired_kwargs` positional-KW-head skip,
    /// `check_aplicacao_timeout` `:kind Aplicacao` match,
    /// `check_git_pin` `:tipo git` `matches_kwarg`-predicate closure,
    /// and `check_consistent_quote` `(quote …)`-form detector, and the
    /// caixa-teia `is_ref_form` `(ref …)`-form detector — which
    /// previously reached the underlying symbol-name scalar through
    /// eight raw `if let NodeKind::Symbol(s) = &n.kind` /
    /// `matches!(&n.kind, NodeKind::Symbol(s) if s == "…")` /
    /// `matches!(items.first().map(|n| &n.kind), Some(NodeKind::Symbol(s))
    /// if s == "…")` open-coded per-arm pattern-matches that expressed
    /// no compile-time link back to the substrate primitive's typed
    /// scalar-arm projection.
    ///
    /// Zero-copy — the returned `&str` borrows from the arm's own
    /// [`String`] storage (pinned by the
    /// `as_symbol_is_by_borrow_pointer_identity` test), the same
    /// discipline as the sibling [`Self::as_keyword`] (6804427) /
    /// [`caixa_teia::TeiaValue::as_str`] (7304ffe) /
    /// [`caixa_teia::TeiaValue::as_object`] (7304ffe) outer-sum-type
    /// per-arm projections. Second `Option<&<payload>>` projection
    /// accessor on the outer [`NodeKind`] sum-type — extends the
    /// `as_<variant>` typed projection family the sibling
    /// [`Self::as_keyword`] opened onto the second load-bearing scalar-
    /// arm axis (symbol names — every head symbol lookup, every enum
    /// variant match, every form-head-tag detector) across the
    /// caixa-ast/caixa-fmt/caixa-lint/caixa-teia consumer surface.
    #[must_use]
    pub fn as_symbol(&self) -> Option<&str> {
        match self {
            Self::Symbol(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Substrate-canonical projection onto the [`Self::Str`] arm's
    /// borrowed scalar payload — returns `Some(&str)` byte-borrowed from
    /// the arm's own [`String`] storage, and [`None`] on every other arm
    /// of the closed fourteen-arm [`NodeKind`] variant set.
    ///
    /// Three production consumers today across two caixa-monorepo crates
    /// — the caixa-lint rule surface's [`caixa_lint::rules`]::
    /// `check_nome_kebab` `:nome`-value kebab-case gate,
    /// `check_no_fixme` `:descricao`-value FIXME-placeholder gate, and
    /// the caixa-fmt printer's `is_flag_token` `-flag`/`--flag`
    /// string-literal command-argument-group detector — which previously
    /// reached the underlying string-literal scalar through three raw
    /// `if let NodeKind::Str(s) = &n.kind` / `matches!(&n.kind,
    /// NodeKind::Str(s) if …)` open-coded per-arm pattern-matches that
    /// expressed no compile-time link back to the substrate primitive's
    /// typed scalar-arm projection.
    ///
    /// Zero-copy — the returned `&str` borrows from the arm's own
    /// [`String`] storage (pinned by the
    /// `as_str_is_by_borrow_pointer_identity` test), the same discipline
    /// as the sibling [`Self::as_keyword`] (6804427) / [`Self::as_symbol`]
    /// (e96eea1) outer-`NodeKind` sum-type per-arm projections and the
    /// peer [`caixa_teia::TeiaValue::as_str`] (7304ffe) outer-`TeiaValue`
    /// sum-type per-arm projection. Third `Option<&<payload>>` projection
    /// accessor on the outer [`NodeKind`] sum-type — closes the
    /// `as_<variant>` typed projection family the sibling [`Self::as_keyword`]
    /// and [`Self::as_symbol`] opened onto the third and final load-
    /// bearing scalar-arm axis (string-literal payloads — every
    /// `:nome` / `:descricao` value gate, every `-flag` token detector,
    /// every quoted-string `:kind` mis-authoring diagnostic) across the
    /// caixa-lint/caixa-fmt consumer surface.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Substrate-canonical projection onto the disjunctive
    /// [`Self::Symbol`] | [`Self::Str`] | [`Self::Keyword`] atom-string-
    /// carrying arm-set — returns `Some(&str)` byte-borrowed from the
    /// matched arm's own [`String`] storage, and [`None`] on every other
    /// arm of the closed fourteen-arm [`NodeKind`] variant set.
    ///
    /// Two production consumers today across the caixa-teia manifest
    /// parser — the `kwarg_symbol` `:tipo` / `:nome` value-shape gate
    /// (`(defteia :tipo aws/vpc :nome main …)` — accepts a bare symbol,
    /// a quoted `"aws/vpc"` string, or a `:aws/vpc` keyword form
    /// depending on author preference) and the `build_ref` `:atributo`
    /// slot (`(ref aws/vpc main id)` / `(ref aws/vpc main :id)` — the
    /// third position accepts any of the three atom-string-carrying arm
    /// shapes, error-messaged as "must be a symbol/keyword/string").
    /// Both previously reached the underlying scalar through a raw
    /// three-arm `NodeKind::Symbol(s) | NodeKind::Str(s) |
    /// NodeKind::Keyword(s) => s.clone()` open-coded per-arm disjunctive
    /// pattern-match that expressed no compile-time link back to the
    /// substrate primitive's typed atom-string-carrying arm-set.
    ///
    /// Semantically distinct from the sibling per-arm [`Self::as_symbol`]
    /// / [`Self::as_str`] / [`Self::as_keyword`] projections — each of
    /// those returns `Some(&str)` on exactly one arm; this one returns
    /// `Some(&str)` on the three-arm disjunction of atom-string-carrying
    /// arms. A future `NodeKind` arm addition that carries a `String`
    /// payload usable as a name-slot value (a hypothetical
    /// `NodeKind::TaggedSymbol(String, SymbolTag)` once the tatara-lisp
    /// reader grows a per-symbol scope tag, a `NodeKind::NamespacedSymbol
    /// (String, String)` shape once the sexp→JSON bridge stabilizes the
    /// `ns/sym` sugar) folds into this projection by extending the
    /// accessor's arm-set once at the substrate primitive, rather than a
    /// two-way rewrite across every caixa-teia manifest-parser call site.
    ///
    /// Zero-copy — the returned `&str` borrows from the matched arm's
    /// own [`String`] storage (pinned by the
    /// `as_atom_string_is_by_borrow_pointer_identity` test), the same
    /// discipline as the sibling per-arm [`Self::as_keyword`] (6804427)
    /// / [`Self::as_symbol`] (e96eea1) / [`Self::as_str`] (55b2909)
    /// scalar-arm projections and the peer [`caixa_teia::TeiaValue::as_str`]
    /// (7304ffe) outer-sum-type projection. Fourth `Option<&<payload>>`
    /// projection accessor on the outer [`NodeKind`] sum-type — the
    /// first *disjunctive* accessor on the projection family the three
    /// sibling per-arm accessors already opened, extending the discipline
    /// onto the atom-string-carrying arm-set every caixa-teia name-slot
    /// gate partitions on.
    #[must_use]
    pub fn as_atom_string(&self) -> Option<&str> {
        match self {
            Self::Symbol(s) | Self::Str(s) | Self::Keyword(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Substrate-canonical projection onto the disjunctive
    /// [`Self::Symbol`] | [`Self::Str`] atom-name-carrying arm-set —
    /// returns `Some(&str)` byte-borrowed from the matched arm's own
    /// [`String`] storage, and [`None`] on every other arm of the closed
    /// fourteen-arm [`NodeKind`] variant set.
    ///
    /// Two production consumers today across two caixa-monorepo crates
    /// — the caixa-teia `build_ref` `:nome` slot (`(ref aws/vpc main id)`
    /// — the second position accepts either a bare symbol `main` or a
    /// quoted `"main"` string, error-messaged as "must be a symbol or
    /// string") and the caixa-lsp `document_symbol` `:nome`-detail
    /// projection (the `DocumentSymbol.detail` field takes the
    /// `:nome value` from every top-level `(defX …)` form, where authors
    /// spell the name as either a bare `nome-slug` symbol or a quoted
    /// `"nome-slug"` string). Both previously reached the underlying
    /// scalar through a raw two-arm `NodeKind::Symbol(s) |
    /// NodeKind::Str(s) => s.clone()` open-coded per-arm disjunctive
    /// pattern-match that expressed no compile-time link back to the
    /// substrate primitive's typed atom-name-carrying arm-set.
    ///
    /// Semantically distinct from the sibling three-arm
    /// [`Self::as_atom_string`] (3c3ca48) projection — that one accepts
    /// the full atom-string-carrying arm-set including `Keyword` (a `:foo`
    /// keyword literal counts as an atom-string in caixa-teia's
    /// `:tipo`/`:nome` / `:atributo` slot); this one accepts only the
    /// two-arm subset that excludes `Keyword`, matching the caixa-teia
    /// `build_ref` `:nome` gate's "must be a symbol or string" contract
    /// and the caixa-lsp `document_symbol` detail's "bare-symbol or
    /// quoted-string name" author-facing shape.
    ///
    /// Zero-copy — the returned `&str` borrows from the matched arm's
    /// own [`String`] storage (pinned by the
    /// `as_symbol_or_str_is_by_borrow_pointer_identity` test), the same
    /// discipline as the sibling per-arm [`Self::as_keyword`] (6804427)
    /// / [`Self::as_symbol`] (e96eea1) / [`Self::as_str`] (55b2909) /
    /// three-arm [`Self::as_atom_string`] (3c3ca48) projections and the
    /// peer [`caixa_teia::TeiaValue::as_str`] (7304ffe) outer-sum-type
    /// projection. Fifth `Option<&<payload>>` projection accessor on the
    /// outer [`NodeKind`] sum-type — the second *disjunctive* accessor
    /// on the projection family the three sibling per-arm accessors
    /// opened, extending the discipline onto the two-arm atom-name-
    /// carrying subset every caixa-teia `build_ref` `:nome` / caixa-lsp
    /// `document_symbol` `:nome`-detail site partitions on.
    #[must_use]
    pub fn as_symbol_or_str(&self) -> Option<&str> {
        match self {
            Self::Symbol(s) | Self::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Substrate-canonical projection onto the [`Self::List`] arm's
    /// borrowed compound payload — returns `Some(&[Node])` byte-borrowed
    /// from the arm's own [`Vec<Node>`] storage, and [`None`] on every
    /// other arm of the closed fourteen-arm [`NodeKind`] variant set.
    ///
    /// Eleven production consumers today across four caixa-monorepo crates
    /// — the caixa-ast [`Node::head_symbol`] list-head projection and
    /// [`Node::kwarg`] `:key value` pair-loop; the caixa-lint rule
    /// surface's `check_enum_pascal`, `check_paired_kwargs`, and
    /// `check_git_pin` per-list walkers; the caixa-teia manifest parser's
    /// `instance_from_node` `:atributos` kwargs-list gate; the caixa-fmt
    /// `commented_head_degrades_out_of_the_plist_shape` printer test's
    /// list-shape unwrap; the caixa-fmt `float_roundtrip` reparse test's
    /// list-shape unwrap; and the caixa-ast `parse_list`,
    /// `parse_map_and_vector`, and `parse_reader_macros` parser tests'
    /// list-shape unwraps — which previously reached the underlying
    /// [`Vec<Node>`] child sequence through eleven raw `let NodeKind::
    /// List(items) = &X.kind else { … }` open-coded per-arm let-else
    /// pattern-matches that expressed no compile-time link back to the
    /// substrate primitive's typed compound-arm projection.
    ///
    /// Semantically distinct from the sibling `Map` and `Vector`
    /// compound-arm carrying shapes — every one of those arms also
    /// carries a `Vec<Node>` payload (`Map(Vec<Node>)` is the brace
    /// dialect's `{ :k v … }`; `Vector(Vec<Node>)` is the bracket
    /// dialect's `[ a b … ]`), so a hand-rolled `matches!(&_.kind,
    /// NodeKind::List(_))` gate has to keep the strict-`List`-only
    /// boundary in view at every site. This projection pins the boundary
    /// on the substrate primitive — the sibling `Map` / `Vector` arms
    /// return `None` even though they carry the same shape payload —
    /// which is why the seven caixa-lint / caixa-teia / caixa-fmt /
    /// caixa-ast production consumers all reach for the strict `List`-arm
    /// gate rather than any compound-arm disjunction (a `(defcaixa …)`
    /// form is a `List`, not a `Map` or `Vector`; a `:atributos` kwargs
    /// slot is a `List`, not a `Map`; the parser's positional-run detection
    /// runs on `List`, not the D4 brace/bracket dialect).
    ///
    /// Zero-copy — the returned `&[Node]` borrows from the arm's own
    /// [`Vec<Node>`] storage (pinned by the
    /// `as_list_is_by_borrow_pointer_identity` test), the same discipline
    /// as the sibling per-arm [`Self::as_keyword`] (6804427) /
    /// [`Self::as_symbol`] (e96eea1) / [`Self::as_str`] (55b2909) scalar
    /// projections and the sibling disjunctive [`Self::as_atom_string`]
    /// (3c3ca48) / [`Self::as_symbol_or_str`] (fd39cea) two-/three-arm
    /// atom-name projections. Sixth `Option<&<payload>>` projection
    /// accessor on the outer [`NodeKind`] sum-type — the first accessor
    /// on the *compound*-arm axis (Map/List/Vector all carry
    /// `Vec<Node>`), extending the projection family from the three
    /// scalar-arm accessors and two disjunctive-scalar-arm accessors onto
    /// the compound-arm axis every downstream authoring-tool and
    /// manifest-parser walker partitions on.
    #[must_use]
    pub fn as_list(&self) -> Option<&[Node]> {
        match self {
            Self::List(items) => Some(items.as_slice()),
            _ => None,
        }
    }
}

impl Node {
    #[must_use]
    pub fn new(kind: NodeKind, span: Span) -> Self {
        Self {
            kind,
            span,
            leading: Vec::new(),
            trailing: Vec::new(),
            after: Vec::new(),
        }
    }

    /// Drop all spans + trivia, lowering into the plain `tatara_lisp::Sexp`
    /// used by the compile pipeline.
    #[must_use]
    pub fn to_tatara_sexp(&self) -> tatara_lisp::Sexp {
        use tatara_lisp::{Atom, Sexp};
        match &self.kind {
            NodeKind::Nil => Sexp::Nil,
            NodeKind::Symbol(s) => Sexp::Atom(Atom::Symbol(s.clone())),
            NodeKind::Keyword(s) => Sexp::Atom(Atom::Keyword(s.clone())),
            NodeKind::Str(s) => Sexp::Atom(Atom::Str(s.clone())),
            NodeKind::Int(i) => Sexp::Atom(Atom::Int(*i)),
            NodeKind::Float(f) => Sexp::Atom(Atom::Float(*f)),
            NodeKind::Bool(b) => Sexp::Atom(Atom::Bool(*b)),
            NodeKind::List(items) => Sexp::List(items.iter().map(Node::to_tatara_sexp).collect()),
            // `tatara_lisp::Sexp` has no Map/Vector variant yet — adding
            // them is a LANGUAGE change, sequenced as Phase 2 of
            // theory/TATARA-LISP-CONSOLIDATION.md D4 and gated on its own
            // differential run over the 1,123-file corpus (correction C4).
            // Until that lands, both lower to a plain list: the elements
            // survive in order, only the brace-ness is dropped. That is
            // strictly closer to intent than today's behaviour, where the
            // delimiters lowered as literal `{` / `}` SYMBOLS inside the
            // list. This projection is used only by the round-trip
            // equivalence tests, which stay honest because formatting
            // re-emits the delimiters and re-parsing recovers the node.
            NodeKind::Map(items) | NodeKind::Vector(items) => {
                Sexp::List(items.iter().map(Node::to_tatara_sexp).collect())
            }
            NodeKind::Quote(inner) => Sexp::Quote(Box::new(inner.to_tatara_sexp())),
            NodeKind::Quasiquote(inner) => Sexp::Quasiquote(Box::new(inner.to_tatara_sexp())),
            NodeKind::Unquote(inner) => Sexp::Unquote(Box::new(inner.to_tatara_sexp())),
            NodeKind::UnquoteSplice(inner) => Sexp::UnquoteSplice(Box::new(inner.to_tatara_sexp())),
        }
    }

    /// Head symbol for a list node like `(defX ...)`. Returns None unless this
    /// is a `List` whose first element is a `Symbol`.
    ///
    /// Routes the head-slot symbol-name projection through the lifted
    /// [`NodeKind::as_symbol`] `Option<&str>` accessor rather than the
    /// raw `let NodeKind::Symbol(s) = &items.first()?.kind else …`
    /// open-coded per-arm pattern-match — sibling in shape to the
    /// caixa-fmt `special_head_arity` / `head_symbol_is_command`,
    /// caixa-lint `check_paired_kwargs` / `check_aplicacao_timeout` /
    /// `check_git_pin` / `check_consistent_quote`, and caixa-teia
    /// `is_ref_form` sites (all converged in this run) that partition
    /// on the outer-`NodeKind` `Symbol` arm through the same substrate-
    /// canonical accessor.
    #[must_use]
    pub fn head_symbol(&self) -> Option<&str> {
        self.kind.as_list()?.first()?.kind.as_symbol()
    }

    /// For a list formatted as alternating `:key value :key value`, returns
    /// the matching value node for `key` (without the leading colon).
    #[must_use]
    pub fn kwarg(&self, key: &str) -> Option<&Node> {
        let items = self.kind.as_list()?;
        let start = if items.first().is_some_and(|n| n.kind.is_symbol()) {
            1
        } else {
            0
        };
        let mut i = start;
        while i + 1 < items.len() {
            // Route the per-`:key value` pair-loop's per-item keyword-
            // name scalar projection through the lifted
            // [`NodeKind::as_keyword`] `Option<&str>` accessor rather
            // than the raw `if let NodeKind::Keyword(k) = &items[i]
            // .kind` open-coded per-arm pattern-match — the per-
            // `Keyword`-arm scalar projection now keys off the
            // substrate-canonical sum-type per-arm accessor every
            // downstream caixa-ast/caixa-lint per-`Keyword`-arm consumer
            // (`caixa_lint::rules::check_keyword_kebab`,
            // `check_enum_pascal`, `keyword_present`, `matches_kwarg`,
            // `items_has_key`) routes through, so any future
            // `NodeKind::Keyword`-adjacent arm extension picks up this
            // dispatch through exactly one edit on the substrate primitive.
            if let Some(k) = items[i].kind.as_keyword() {
                if k == key {
                    return Some(&items[i + 1]);
                }
            }
            i += 2;
        }
        None
    }
}

#[cfg(test)]
mod is_variant_tests {
    use super::*;

    fn all_variants() -> Vec<(NodeKind, &'static str)> {
        vec![
            (NodeKind::Nil, "Nil"),
            (NodeKind::Symbol("x".into()), "Symbol"),
            (NodeKind::Keyword("k".into()), "Keyword"),
            (NodeKind::Str("s".into()), "Str"),
            (NodeKind::Int(0), "Int"),
            (NodeKind::Float(0.0), "Float"),
            (NodeKind::Bool(false), "Bool"),
            (NodeKind::List(Vec::new()), "List"),
            (NodeKind::Map(Vec::new()), "Map"),
            (NodeKind::Vector(Vec::new()), "Vector"),
            (
                NodeKind::Quote(Box::new(Node::new(NodeKind::Nil, Span::new(0, 0)))),
                "Quote",
            ),
            (
                NodeKind::Quasiquote(Box::new(Node::new(NodeKind::Nil, Span::new(0, 0)))),
                "Quasiquote",
            ),
            (
                NodeKind::Unquote(Box::new(Node::new(NodeKind::Nil, Span::new(0, 0)))),
                "Unquote",
            ),
            (
                NodeKind::UnquoteSplice(Box::new(Node::new(NodeKind::Nil, Span::new(0, 0)))),
                "UnquoteSplice",
            ),
        ]
    }

    fn predicate_row(k: &NodeKind) -> [bool; 14] {
        [
            k.is_nil(),
            k.is_symbol(),
            k.is_keyword(),
            k.is_str(),
            k.is_int(),
            k.is_float(),
            k.is_bool(),
            k.is_list(),
            k.is_map(),
            k.is_vector(),
            k.is_quote(),
            k.is_quasiquote(),
            k.is_unquote(),
            k.is_unquote_splice(),
        ]
    }

    // Fail-before-pass-after pin on the [`gen_platform::IsVariant`]
    // derive-generated per-arm predicate partition — for every variant
    // in `all_variants()`, the observed 14-slot predicate row must
    // equal a one-hot row with the `true` at exactly the same index as
    // the variant's declaration order. Expected rows are generated
    // live from the enumeration rather than transcribed by hand, so a
    // copy-paste flip that reroutes one arm through the wrong
    // predicate lane trips at the identity-diagonal assertion the way
    // every peer `CaixaKind` / `CaixaDialeto` / `DepList` /
    // `PathShapeViolation` / `RestartStrategy` partition pin already
    // does on the sibling caixa-core surface.
    #[test]
    fn node_kind_is_variant_predicates_partition_the_arm_set() {
        let variants = all_variants();
        for (idx, (variant, name)) in variants.iter().enumerate() {
            let observed = predicate_row(variant);
            let mut expected = [false; 14];
            expected[idx] = true;
            assert_eq!(
                observed, expected,
                "NodeKind::{name} at declaration-order slot {idx} must \
                 satisfy exactly one is_* predicate (its own); observed \
                 row must equal the one-hot expected row"
            );
        }
    }

    // Byte-parity pin on the two field-agnostic `matches!` shapes this
    // lift replaces at production call sites: the `NodeKind::Symbol(_)`
    // gate (caixa-ast/src/node.rs `kwarg` head-skip, caixa-fmt/src/
    // printer.rs `kwargs_head_len` take-while) and the
    // `NodeKind::Keyword(_)` gate (caixa-fmt/src/printer.rs
    // `kwargs_head_len` pair-check + `inline_slot_count` pair-detect,
    // caixa-lint/src/rules.rs `paired-kwargs` first-arg + second-arg
    // gates). Refuses a future accidental split between the derived
    // predicate and its pre-lift `matches!` shape (a hand-rolled
    // shadow `impl` that overrides one path, an accidental rebrand of
    // one converged call site back to the `matches!` form) on the two
    // load-bearing arm-discriminator axes every downstream authoring
    // consumer (caixa-fmt, caixa-lint) partitions on.
    #[test]
    fn node_kind_is_symbol_and_is_keyword_byte_equal_pre_lift_matches_shape() {
        for (variant, name) in all_variants() {
            let via_matches_symbol = matches!(variant, NodeKind::Symbol(_));
            let via_predicate_symbol = variant.is_symbol();
            assert_eq!(
                via_predicate_symbol, via_matches_symbol,
                "NodeKind::{name}.is_symbol() must byte-equal \
                 matches!(_, NodeKind::Symbol(_)) — otherwise the \
                 converged call sites in caixa-ast/caixa-fmt would \
                 silently disagree with their pre-lift shape"
            );
            let via_matches_keyword = matches!(variant, NodeKind::Keyword(_));
            let via_predicate_keyword = variant.is_keyword();
            assert_eq!(
                via_predicate_keyword, via_matches_keyword,
                "NodeKind::{name}.is_keyword() must byte-equal \
                 matches!(_, NodeKind::Keyword(_)) — otherwise the \
                 converged call sites in caixa-fmt/caixa-lint would \
                 silently disagree with their pre-lift shape"
            );
        }
    }

    // Projection contract on the outer-`NodeKind` sum-type's `Keyword`
    // scalar-arm accessor: exactly the `Keyword` arm returns
    // `Some(&str)` byte-borrowed from the arm's own [`String`] storage;
    // every other arm returns `None`. Pins the "one canonical
    // projection dispatch per typed arm on the substrate primitive"
    // discipline the six per-`Keyword`-arm consumer sites (caixa-ast
    // [`Node::kwarg`] pair-loop; caixa-lint `check_keyword_kebab`,
    // `check_enum_pascal`, `keyword_present`, `matches_kwarg`,
    // `items_has_key`) route through via
    // `.as_keyword()`/`.and_then(NodeKind::as_keyword)`. A regression
    // that admitted a non-`Keyword` arm through this projection would
    // silently classify a bare symbol or string literal as a keyword
    // at the six per-consumer sites — `kwarg` would return the wrong
    // pair value, `keyword_present` would find phantom `:timeout`s in
    // string literals, `matches_kwarg`/`items_has_key` would treat
    // positional args as keyword pairs. This test guards that surface
    // across every arm of the closed fourteen-arm partition.
    #[test]
    fn as_keyword_projects_only_keyword_arm() {
        let variants = all_variants();
        for (variant, name) in &variants {
            let projected = variant.as_keyword();
            if matches!(variant, NodeKind::Keyword(_)) {
                let NodeKind::Keyword(k) = variant else {
                    unreachable!("guarded by matches! above");
                };
                assert_eq!(
                    projected,
                    Some(k.as_str()),
                    "NodeKind::{name} is the Keyword arm — as_keyword() \
                     must project onto its own String payload"
                );
            } else {
                assert_eq!(
                    projected, None,
                    "NodeKind::{name} is not the Keyword arm — \
                     as_keyword() must return None"
                );
            }
        }
        // Empty keyword — the accessor is a projection, not a gate; an
        // empty-`:` keyword (author-declared or parser-produced) round-
        // trips as `Some("")`, not `None`.
        assert_eq!(NodeKind::Keyword(String::new()).as_keyword(), Some(""));
    }

    // Zero-copy pin — `k.as_keyword()` must borrow from the `Keyword`
    // arm's own [`String`] storage, not clone into a fresh buffer.
    // Fails at build time if a future rewrite regresses to
    // `Some(k.clone().leak())` or any other detour that silently
    // allocates on every call (the same shape as the sibling
    // [`caixa_teia::TeiaValue::as_str_is_by_borrow_pointer_identity`]
    // pin on the outer-`TeiaValue` scalar accessor).
    #[test]
    fn as_keyword_is_by_borrow_pointer_identity() {
        let k = NodeKind::Keyword("timeout".into());
        let via_accessor: &str = k.as_keyword().unwrap();
        let NodeKind::Keyword(ref inner) = k else {
            unreachable!("constructed above as NodeKind::Keyword");
        };
        assert_eq!(
            via_accessor.as_ptr(),
            inner.as_ptr(),
            "NodeKind::as_keyword must borrow from the Keyword arm's \
             String backing storage (zero-copy projection)",
        );
        assert_eq!(
            via_accessor.len(),
            inner.len(),
            "NodeKind::as_keyword and inner.as_str() must byte-equal \
             in length (same slice)",
        );
    }

    // Byte-parity pin on the pre-lift `if let NodeKind::Keyword(k) =
    // &n.kind { … if k == key … }` shape the six caixa-ast/caixa-lint
    // consumer sites route through today via
    // `n.kind.as_keyword() == Some(key)` (or `if let Some(k) =
    // n.kind.as_keyword() { if k == key … }`). Refuses a future
    // accidental split between the accessor's return contract and its
    // pre-lift `matches!`/`if let` shape (a hand-rolled shadow
    // `impl` that overrides one path, an accidental rebrand of one
    // converged call site back to the raw `NodeKind::Keyword(k)`
    // form) on the load-bearing keyword-name-projection axis every
    // downstream caixa-lint rule's `:key value` pair-loop / walker
    // partitions on.
    #[test]
    fn as_keyword_byte_equal_pre_lift_pattern_match_shape() {
        for (variant, name) in all_variants() {
            let via_pattern: Option<&str> = match &variant {
                NodeKind::Keyword(k) => Some(k.as_str()),
                _ => None,
            };
            let via_accessor = variant.as_keyword();
            assert_eq!(
                via_accessor, via_pattern,
                "NodeKind::{name}.as_keyword() must byte-equal \
                 `match &_ {{ NodeKind::Keyword(k) => Some(k.as_str()), \
                 _ => None }}` — otherwise the six converged caixa-ast/ \
                 caixa-lint call sites would silently disagree with \
                 their pre-lift shape"
            );
        }
    }

    // Projection contract on the outer-`NodeKind` sum-type's `Symbol`
    // scalar-arm accessor: exactly the `Symbol` arm returns
    // `Some(&str)` byte-borrowed from the arm's own [`String`] storage;
    // every other arm returns `None`. Pins the "one canonical
    // projection dispatch per typed arm on the substrate primitive"
    // discipline the eight per-`Symbol`-arm consumer sites (caixa-ast
    // [`Node::head_symbol`]; caixa-fmt `special_head_arity`,
    // `head_symbol_is_command`; caixa-lint `check_paired_kwargs`,
    // `check_aplicacao_timeout`, `check_git_pin`,
    // `check_consistent_quote`; caixa-teia `is_ref_form`) route
    // through via `.as_symbol()` / `.as_symbol() == Some("…")` /
    // `.and_then(|n| n.kind.as_symbol())`. A regression that admitted
    // a non-`Symbol` arm through this projection would silently
    // classify a keyword or string literal as a symbol at the eight
    // per-consumer sites — `head_symbol` would return names of quoted
    // keyword forms, `head_symbol_is_command` would fire on `:tag`
    // literals, `check_git_pin`'s `:tipo git` gate would accept
    // `"git"` strings, `is_ref_form` would classify `("ref" …)` as a
    // ref. This test guards that surface across every arm of the
    // closed fourteen-arm partition.
    #[test]
    fn as_symbol_projects_only_symbol_arm() {
        let variants = all_variants();
        for (variant, name) in &variants {
            let projected = variant.as_symbol();
            if matches!(variant, NodeKind::Symbol(_)) {
                let NodeKind::Symbol(s) = variant else {
                    unreachable!("guarded by matches! above");
                };
                assert_eq!(
                    projected,
                    Some(s.as_str()),
                    "NodeKind::{name} is the Symbol arm — as_symbol() \
                     must project onto its own String payload"
                );
            } else {
                assert_eq!(
                    projected, None,
                    "NodeKind::{name} is not the Symbol arm — \
                     as_symbol() must return None"
                );
            }
        }
        // Empty symbol — the accessor is a projection, not a gate; an
        // empty-name symbol (parser-produced from a stray reader edge
        // case) round-trips as `Some("")`, not `None`.
        assert_eq!(NodeKind::Symbol(String::new()).as_symbol(), Some(""));
    }

    // Zero-copy pin — `k.as_symbol()` must borrow from the `Symbol`
    // arm's own [`String`] storage, not clone into a fresh buffer.
    // Fails at build time if a future rewrite regresses to
    // `Some(s.clone().leak())` or any other detour that silently
    // allocates on every call (the same shape as the sibling
    // [`as_keyword_is_by_borrow_pointer_identity`] pin on the peer
    // outer-`NodeKind` `Keyword`-arm scalar accessor and the
    // [`caixa_teia::TeiaValue::as_str_is_by_borrow_pointer_identity`]
    // pin on the outer-`TeiaValue` `Str`-arm scalar accessor).
    #[test]
    fn as_symbol_is_by_borrow_pointer_identity() {
        let k = NodeKind::Symbol("defcaixa".into());
        let via_accessor: &str = k.as_symbol().unwrap();
        let NodeKind::Symbol(ref inner) = k else {
            unreachable!("constructed above as NodeKind::Symbol");
        };
        assert_eq!(
            via_accessor.as_ptr(),
            inner.as_ptr(),
            "NodeKind::as_symbol must borrow from the Symbol arm's \
             String backing storage (zero-copy projection)",
        );
        assert_eq!(
            via_accessor.len(),
            inner.len(),
            "NodeKind::as_symbol and inner.as_str() must byte-equal \
             in length (same slice)",
        );
    }

    // Byte-parity pin on the pre-lift `let NodeKind::Symbol(s) = &n.kind
    // else …` / `matches!(&n.kind, NodeKind::Symbol(s) if s == "…")`
    // shape the eight caixa-ast/caixa-fmt/caixa-lint/caixa-teia
    // consumer sites route through today via `.as_symbol()` /
    // `.as_symbol() == Some("…")`. Refuses a future accidental split
    // between the accessor's return contract and its pre-lift shape
    // (a hand-rolled shadow `impl` that overrides one path, an
    // accidental rebrand of one converged call site back to the raw
    // `NodeKind::Symbol(s)` form) on the load-bearing symbol-name-
    // projection axis every downstream head-symbol / form-tag /
    // enum-variant partition keys off.
    #[test]
    fn as_symbol_byte_equal_pre_lift_pattern_match_shape() {
        for (variant, name) in all_variants() {
            let via_pattern: Option<&str> = match &variant {
                NodeKind::Symbol(s) => Some(s.as_str()),
                _ => None,
            };
            let via_accessor = variant.as_symbol();
            assert_eq!(
                via_accessor, via_pattern,
                "NodeKind::{name}.as_symbol() must byte-equal \
                 `match &_ {{ NodeKind::Symbol(s) => Some(s.as_str()), \
                 _ => None }}` — otherwise the eight converged caixa-ast/ \
                 caixa-fmt/caixa-lint/caixa-teia call sites would silently \
                 disagree with their pre-lift shape"
            );
        }
    }

    // Projection contract on the outer-`NodeKind` sum-type's `Str`
    // scalar-arm accessor: exactly the `Str` arm returns `Some(&str)`
    // byte-borrowed from the arm's own [`String`] storage; every other
    // arm returns `None`. Pins the "one canonical projection dispatch
    // per typed arm on the substrate primitive" discipline the three
    // per-`Str`-arm consumer sites (caixa-lint `check_nome_kebab`,
    // `check_no_fixme`; caixa-fmt `is_flag_token`) route through via
    // `.as_str()` / `.as_str().is_some_and(…)`. A regression that
    // admitted a non-`Str` arm through this projection would silently
    // classify a bare symbol or keyword literal as a string at the three
    // per-consumer sites — `check_nome_kebab` would flag `:nome` bare-
    // symbol values as non-kebab, `check_no_fixme` would scan symbol
    // names for FIXME, `is_flag_token` would layout-align symbols as
    // `-flag` tokens. This test guards that surface across every arm of
    // the closed fourteen-arm partition.
    #[test]
    fn as_str_projects_only_str_arm() {
        let variants = all_variants();
        for (variant, name) in &variants {
            let projected = variant.as_str();
            if matches!(variant, NodeKind::Str(_)) {
                let NodeKind::Str(s) = variant else {
                    unreachable!("guarded by matches! above");
                };
                assert_eq!(
                    projected,
                    Some(s.as_str()),
                    "NodeKind::{name} is the Str arm — as_str() \
                     must project onto its own String payload"
                );
            } else {
                assert_eq!(
                    projected, None,
                    "NodeKind::{name} is not the Str arm — \
                     as_str() must return None"
                );
            }
        }
        // Empty string — the accessor is a projection, not a gate; an
        // empty-`""` string literal (author-declared or parser-produced)
        // round-trips as `Some("")`, not `None`.
        assert_eq!(NodeKind::Str(String::new()).as_str(), Some(""));
    }

    // Zero-copy pin — `k.as_str()` must borrow from the `Str` arm's own
    // [`String`] storage, not clone into a fresh buffer. Fails at build
    // time if a future rewrite regresses to `Some(s.clone().leak())` or
    // any other detour that silently allocates on every call (the same
    // shape as the sibling [`as_keyword_is_by_borrow_pointer_identity`]
    // / [`as_symbol_is_by_borrow_pointer_identity`] pins on the peer
    // outer-`NodeKind` `Keyword`- / `Symbol`-arm scalar accessors and
    // the [`caixa_teia::TeiaValue::as_str_is_by_borrow_pointer_identity`]
    // pin on the outer-`TeiaValue` `Str`-arm scalar accessor).
    #[test]
    fn as_str_is_by_borrow_pointer_identity() {
        let k = NodeKind::Str("demo".into());
        let via_accessor: &str = k.as_str().unwrap();
        let NodeKind::Str(ref inner) = k else {
            unreachable!("constructed above as NodeKind::Str");
        };
        assert_eq!(
            via_accessor.as_ptr(),
            inner.as_ptr(),
            "NodeKind::as_str must borrow from the Str arm's \
             String backing storage (zero-copy projection)",
        );
        assert_eq!(
            via_accessor.len(),
            inner.len(),
            "NodeKind::as_str and inner.as_str() must byte-equal \
             in length (same slice)",
        );
    }

    // Byte-parity pin on the pre-lift `if let NodeKind::Str(s) = &n.kind
    // { … }` / `matches!(&n.kind, NodeKind::Str(s) if …)` shape the
    // three caixa-lint/caixa-fmt consumer sites route through today via
    // `.as_str()` / `.as_str().is_some_and(…)`. Refuses a future
    // accidental split between the accessor's return contract and its
    // pre-lift shape (a hand-rolled shadow `impl` that overrides one
    // path, an accidental rebrand of one converged call site back to
    // the raw `NodeKind::Str(s)` form) on the load-bearing string-
    // literal-projection axis every downstream `:nome`/`:descricao`
    // value-shape / flag-token layout partition keys off.
    #[test]
    fn as_str_byte_equal_pre_lift_pattern_match_shape() {
        for (variant, name) in all_variants() {
            let via_pattern: Option<&str> = match &variant {
                NodeKind::Str(s) => Some(s.as_str()),
                _ => None,
            };
            let via_accessor = variant.as_str();
            assert_eq!(
                via_accessor, via_pattern,
                "NodeKind::{name}.as_str() must byte-equal \
                 `match &_ {{ NodeKind::Str(s) => Some(s.as_str()), \
                 _ => None }}` — otherwise the three converged \
                 caixa-lint/caixa-fmt call sites would silently disagree \
                 with their pre-lift shape"
            );
        }
    }

    // Projection contract on the outer-`NodeKind` sum-type's disjunctive
    // `Symbol` | `Str` | `Keyword` atom-string-carrying accessor: exactly
    // the three atom-string-carrying arms return `Some(&str)` byte-
    // borrowed from the matched arm's own [`String`] storage; every
    // other arm returns `None`. Pins the "one canonical projection
    // dispatch per typed arm-set on the substrate primitive" discipline
    // the two per-disjunctive-arm-set consumer sites (caixa-teia
    // `kwarg_symbol` `:tipo`/`:nome` value-shape gate; `build_ref`
    // `:atributo`-slot projection) route through via
    // `.as_atom_string().map(str::to_owned)` /
    // `.as_atom_string().map(str::to_owned).ok_or(…)`. A regression
    // that admitted a non-atom-string-carrying arm (a numeric literal,
    // a list, a quote) through this projection would silently classify
    // a bare `42` or `(a b)` as a `:tipo`/`:nome` name at the caixa-teia
    // manifest parser — `kwarg_symbol` would surface a stringified
    // integer as an instance name, `build_ref` would let `(ref aws/vpc
    // main (a b))` slip past its "must be a symbol/keyword/string"
    // guard. This test guards that surface across every arm of the
    // closed fourteen-arm partition.
    #[test]
    fn as_atom_string_projects_only_symbol_str_keyword_arms() {
        let variants = all_variants();
        for (variant, name) in &variants {
            let projected = variant.as_atom_string();
            let is_atom_string_arm = matches!(
                variant,
                NodeKind::Symbol(_) | NodeKind::Str(_) | NodeKind::Keyword(_)
            );
            if is_atom_string_arm {
                let expected = match variant {
                    NodeKind::Symbol(s) | NodeKind::Str(s) | NodeKind::Keyword(s) => s.as_str(),
                    _ => unreachable!("guarded by is_atom_string_arm above"),
                };
                assert_eq!(
                    projected,
                    Some(expected),
                    "NodeKind::{name} is an atom-string-carrying arm — \
                     as_atom_string() must project onto its own String \
                     payload"
                );
            } else {
                assert_eq!(
                    projected, None,
                    "NodeKind::{name} is not an atom-string-carrying \
                     arm — as_atom_string() must return None"
                );
            }
        }
        // Empty payload on each of the three carrying arms — the
        // accessor is a projection, not a gate; a nameless
        // symbol/string/keyword round-trips as `Some("")`, not `None`.
        assert_eq!(
            NodeKind::Symbol(String::new()).as_atom_string(),
            Some(""),
            "empty-payload Symbol arm must project to Some(\"\")"
        );
        assert_eq!(
            NodeKind::Str(String::new()).as_atom_string(),
            Some(""),
            "empty-payload Str arm must project to Some(\"\")"
        );
        assert_eq!(
            NodeKind::Keyword(String::new()).as_atom_string(),
            Some(""),
            "empty-payload Keyword arm must project to Some(\"\")"
        );
    }

    // Zero-copy pin — `k.as_atom_string()` must borrow from the matched
    // arm's own [`String`] storage, not clone into a fresh buffer. Fails
    // at build time if a future rewrite regresses to `Some(s.clone()
    // .leak())` or any other detour that silently allocates on every
    // call. Pinned across all three atom-string-carrying arms
    // (Symbol/Str/Keyword) — a per-arm-specific regression would slip
    // past a single-arm pin — the same shape as the sibling
    // [`as_symbol_is_by_borrow_pointer_identity`] /
    // [`as_str_is_by_borrow_pointer_identity`] /
    // [`as_keyword_is_by_borrow_pointer_identity`] pins on the peer
    // outer-`NodeKind` per-arm scalar accessors.
    #[test]
    fn as_atom_string_is_by_borrow_pointer_identity() {
        for (constructor, ctor_name) in [
            (
                (|s: String| NodeKind::Symbol(s)) as fn(String) -> NodeKind,
                "Symbol",
            ),
            (
                (|s: String| NodeKind::Str(s)) as fn(String) -> NodeKind,
                "Str",
            ),
            (
                (|s: String| NodeKind::Keyword(s)) as fn(String) -> NodeKind,
                "Keyword",
            ),
        ] {
            let payload = format!("aws/vpc-{ctor_name}");
            let payload_len = payload.len();
            let k = constructor(payload);
            let via_accessor: &str = k.as_atom_string().unwrap();
            let inner_ptr = match &k {
                NodeKind::Symbol(inner) | NodeKind::Str(inner) | NodeKind::Keyword(inner) => {
                    inner.as_ptr()
                }
                _ => unreachable!("constructed above via atom-string arm ctor"),
            };
            assert_eq!(
                via_accessor.as_ptr(),
                inner_ptr,
                "NodeKind::as_atom_string on the {ctor_name} arm must \
                 borrow from that arm's String backing storage \
                 (zero-copy projection)",
            );
            assert_eq!(
                via_accessor.len(),
                payload_len,
                "NodeKind::as_atom_string on the {ctor_name} arm must \
                 byte-equal in length to the arm's payload",
            );
        }
    }

    // Byte-parity pin on the pre-lift three-arm disjunctive
    // `match &v.kind { NodeKind::Symbol(s) | NodeKind::Str(s) |
    // NodeKind::Keyword(s) => Some(s.clone()), _ => None }` /
    // `NodeKind::Symbol(s) | NodeKind::Str(s) | NodeKind::Keyword(s)
    // => s.clone()` shape the two caixa-teia manifest-parser consumer
    // sites (`kwarg_symbol` :tipo/:nome value-shape gate, `build_ref`
    // :atributo-slot projection) route through today via
    // `.as_atom_string().map(str::to_owned)`. Refuses a future
    // accidental split between the accessor's return contract and its
    // pre-lift disjunctive shape (a hand-rolled shadow `impl` that
    // overrides one path, an accidental rebrand of one converged call
    // site back to the raw three-arm `NodeKind::Symbol(s) | …` form)
    // on the load-bearing atom-string-carrying disjunctive axis every
    // caixa-teia name-slot gate keys off.
    #[test]
    fn as_atom_string_byte_equal_pre_lift_pattern_match_shape() {
        for (variant, name) in all_variants() {
            let via_pattern: Option<&str> = match &variant {
                NodeKind::Symbol(s) | NodeKind::Str(s) | NodeKind::Keyword(s) => Some(s.as_str()),
                _ => None,
            };
            let via_accessor = variant.as_atom_string();
            assert_eq!(
                via_accessor, via_pattern,
                "NodeKind::{name}.as_atom_string() must byte-equal \
                 `match &_ {{ NodeKind::Symbol(s) | NodeKind::Str(s) | \
                 NodeKind::Keyword(s) => Some(s.as_str()), _ => None \
                 }}` — otherwise the two converged caixa-teia \
                 manifest-parser call sites would silently disagree \
                 with their pre-lift shape"
            );
        }
    }

    // Projection contract on the outer-`NodeKind` sum-type's disjunctive
    // two-arm `Symbol` | `Str` atom-name-carrying accessor: exactly the
    // two atom-name-carrying arms return `Some(&str)` byte-borrowed from
    // the matched arm's own [`String`] storage; every other arm (including
    // the `Keyword` arm — the strict-subset boundary vs the sibling
    // three-arm [`NodeKind::as_atom_string`] projection) returns `None`.
    // Pins the "one canonical projection dispatch per typed arm-set on
    // the substrate primitive" discipline the two per-two-arm-disjunction
    // consumer sites (caixa-teia `build_ref` `:nome`-slot projection;
    // caixa-lsp `document_symbol` `:nome`-detail projection) route through
    // via `.as_symbol_or_str().map(str::to_owned)`. A regression that
    // admitted a non-atom-name-carrying arm (a keyword literal, a numeric,
    // a list, a quote) through this projection would silently classify
    // `:foo` / `42` / `(a b)` as a `:nome` name at the caixa-teia manifest
    // parser and the caixa-lsp document-symbol pane — `build_ref` would
    // let `(ref aws/vpc :foo id)` slip past its "must be a symbol or
    // string" guard, `document_symbol.detail` would surface a stringified
    // keyword or integer as an instance name. This test guards that
    // surface across every arm of the closed fourteen-arm partition.
    #[test]
    fn as_symbol_or_str_projects_only_symbol_and_str_arms() {
        let variants = all_variants();
        for (variant, name) in &variants {
            let projected = variant.as_symbol_or_str();
            let is_symbol_or_str_arm = matches!(variant, NodeKind::Symbol(_) | NodeKind::Str(_));
            if is_symbol_or_str_arm {
                let expected = match variant {
                    NodeKind::Symbol(s) | NodeKind::Str(s) => s.as_str(),
                    _ => unreachable!("guarded by is_symbol_or_str_arm above"),
                };
                assert_eq!(
                    projected,
                    Some(expected),
                    "NodeKind::{name} is an atom-name-carrying arm — \
                     as_symbol_or_str() must project onto its own String \
                     payload"
                );
            } else {
                assert_eq!(
                    projected, None,
                    "NodeKind::{name} is not an atom-name-carrying arm \
                     (Symbol|Str) — as_symbol_or_str() must return None"
                );
            }
        }
        // Empty payload on each of the two carrying arms — the accessor
        // is a projection, not a gate; a nameless symbol/string round-
        // trips as `Some("")`, not `None`.
        assert_eq!(
            NodeKind::Symbol(String::new()).as_symbol_or_str(),
            Some(""),
            "empty-payload Symbol arm must project to Some(\"\")"
        );
        assert_eq!(
            NodeKind::Str(String::new()).as_symbol_or_str(),
            Some(""),
            "empty-payload Str arm must project to Some(\"\")"
        );
        // Strict-subset boundary vs the sibling three-arm
        // [`NodeKind::as_atom_string`] (3c3ca48): the `Keyword` arm is
        // in the three-arm superset but NOT in this two-arm subset — a
        // future refactor that widened this accessor to admit `Keyword`
        // (silently collapsing the semantic distinction between the
        // two accessors) trips at this pin before the caixa-teia
        // `build_ref` `:nome` guard and the caixa-lsp `document_symbol`
        // detail silently start accepting `:keyword` literals.
        assert_eq!(
            NodeKind::Keyword("k".into()).as_symbol_or_str(),
            None,
            "Keyword arm is in as_atom_string's three-arm set but MUST NOT \
             be in as_symbol_or_str's two-arm subset — the strict-subset \
             boundary is the whole point of a distinct accessor"
        );
    }

    // Zero-copy pin — `k.as_symbol_or_str()` must borrow from the matched
    // arm's own [`String`] storage, not clone into a fresh buffer. Fails
    // at build time if a future rewrite regresses to `Some(s.clone()
    // .leak())` or any other detour that silently allocates on every call.
    // Pinned across both atom-name-carrying arms (Symbol/Str) — a per-
    // arm-specific regression would slip past a single-arm pin — the same
    // shape as the sibling [`as_atom_string_is_by_borrow_pointer_identity`]
    // pin on the peer three-arm outer-`NodeKind` disjunctive projection.
    #[test]
    fn as_symbol_or_str_is_by_borrow_pointer_identity() {
        for (constructor, ctor_name) in [
            (
                (|s: String| NodeKind::Symbol(s)) as fn(String) -> NodeKind,
                "Symbol",
            ),
            (
                (|s: String| NodeKind::Str(s)) as fn(String) -> NodeKind,
                "Str",
            ),
        ] {
            let payload = format!("aws-vpc-{ctor_name}");
            let payload_len = payload.len();
            let k = constructor(payload);
            let via_accessor: &str = k.as_symbol_or_str().unwrap();
            let inner_ptr = match &k {
                NodeKind::Symbol(inner) | NodeKind::Str(inner) => inner.as_ptr(),
                _ => unreachable!("constructed above via atom-name arm ctor"),
            };
            assert_eq!(
                via_accessor.as_ptr(),
                inner_ptr,
                "NodeKind::as_symbol_or_str on the {ctor_name} arm must \
                 borrow from that arm's String backing storage \
                 (zero-copy projection)",
            );
            assert_eq!(
                via_accessor.len(),
                payload_len,
                "NodeKind::as_symbol_or_str on the {ctor_name} arm must \
                 byte-equal in length to the arm's payload",
            );
        }
    }

    // Byte-parity pin on the pre-lift two-arm disjunctive
    // `match &_.kind { NodeKind::Symbol(s) | NodeKind::Str(s) => s.clone(),
    // _ => … }` shape the two caixa-teia `build_ref` `:nome`-slot and
    // caixa-lsp `document_symbol` `:nome`-detail consumer sites route
    // through today via `.as_symbol_or_str().map(str::to_owned)`. Refuses
    // a future accidental split between the accessor's return contract
    // and its pre-lift disjunctive shape (a hand-rolled shadow `impl` that
    // overrides one path, an accidental rebrand of one converged call site
    // back to the raw two-arm `NodeKind::Symbol(s) | NodeKind::Str(s)`
    // form, or a widening of the accessor to admit `Keyword`) on the
    // load-bearing atom-name-carrying disjunctive axis every `:nome` slot
    // partitions on.
    #[test]
    fn as_symbol_or_str_byte_equal_pre_lift_pattern_match_shape() {
        for (variant, name) in all_variants() {
            let via_pattern: Option<&str> = match &variant {
                NodeKind::Symbol(s) | NodeKind::Str(s) => Some(s.as_str()),
                _ => None,
            };
            let via_accessor = variant.as_symbol_or_str();
            assert_eq!(
                via_accessor, via_pattern,
                "NodeKind::{name}.as_symbol_or_str() must byte-equal \
                 `match &_ {{ NodeKind::Symbol(s) | NodeKind::Str(s) => \
                 Some(s.as_str()), _ => None }}` — otherwise the two \
                 converged caixa-teia/caixa-lsp `:nome` call sites would \
                 silently disagree with their pre-lift shape"
            );
        }
    }

    // Byte-parity pin on the disjunctive `NodeKind::Int(_) |
    // NodeKind::Float(_)` shape the caixa-fmt/src/printer.rs
    // `is_numeric` grid-column right-align gate keys off. Refuses a
    // future arm addition (a hypothetical `NodeKind::Rational(_)` /
    // `NodeKind::Ratio(_)` when the reader grows a rational literal)
    // that lands on the disjunction without a matching predicate
    // extension — the pin trips at build time before the fmt printer
    // silently miscategorises the new numeric arm.
    #[test]
    fn node_kind_is_int_or_is_float_byte_equal_pre_lift_numeric_matches_shape() {
        for (variant, name) in all_variants() {
            let via_matches = matches!(variant, NodeKind::Int(_) | NodeKind::Float(_));
            let via_predicate = variant.is_int() || variant.is_float();
            assert_eq!(
                via_predicate, via_matches,
                "NodeKind::{name}.is_int() || .is_float() must \
                 byte-equal matches!(_, NodeKind::Int(_) | \
                 NodeKind::Float(_)) — otherwise caixa-fmt's grid-column \
                 numeric-right-align gate would silently disagree with \
                 its pre-lift shape"
            );
        }
    }

    // Projection contract on the outer-`NodeKind` sum-type's `List`
    // compound-arm accessor: exactly the `List` arm returns `Some(&[Node])`
    // byte-borrowed from the arm's own [`Vec<Node>`] storage; every other
    // arm — INCLUDING the sibling `Map` and `Vector` arms that also carry
    // `Vec<Node>` payloads — returns `None`. Pins the "one canonical
    // projection dispatch per typed arm on the substrate primitive"
    // discipline the eleven per-`List`-arm consumer sites (caixa-ast
    // [`Node::head_symbol`], [`Node::kwarg`], `parse_list`,
    // `parse_map_and_vector`, `parse_reader_macros`; caixa-lint
    // `check_enum_pascal`, `check_paired_kwargs`, `check_git_pin`;
    // caixa-teia `instance_from_node` `:atributos` gate; caixa-fmt
    // `commented_head_degrades_out_of_the_plist_shape`, `float_roundtrip`)
    // route through via `.as_list()`. A regression that admitted a `Map`
    // or `Vector` arm through this projection would silently classify
    // brace-dialect `{ :k v … }` forms and bracket-dialect `[ a b … ]`
    // forms as ordinary parenthesized `(a b …)` lists at the eleven
    // per-consumer sites — `head_symbol` would surface the first element
    // of a brace map as a form-head, `kwarg` would scan bracket vectors
    // for `:key value` pairs, `check_enum_pascal` would flag Vec elements
    // as enum-variant mis-authorings, `instance_from_node`'s `:atributos`
    // gate would accept a hoisted `{ :k v }` shape it explicitly refuses.
    // This test guards that surface across every arm of the closed
    // fourteen-arm partition, with the `Map` / `Vector` non-`List`-arm
    // gates called out explicitly as the strict-`List`-only boundary the
    // whole point of a per-arm accessor turns on.
    #[test]
    fn as_list_projects_only_list_arm() {
        let variants = all_variants();
        for (variant, name) in &variants {
            let projected = variant.as_list();
            if matches!(variant, NodeKind::List(_)) {
                let NodeKind::List(items) = variant else {
                    unreachable!("guarded by matches! above");
                };
                assert_eq!(
                    projected,
                    Some(items.as_slice()),
                    "NodeKind::{name} is the List arm — as_list() must \
                     project onto its own Vec<Node> payload"
                );
            } else {
                assert_eq!(
                    projected, None,
                    "NodeKind::{name} is not the List arm — as_list() \
                     must return None"
                );
            }
        }
        // Empty list — the accessor is a projection, not a gate; an
        // empty-`()` list (author-declared or parser-produced from a stray
        // reader edge case) round-trips as `Some(&[])`, not `None`.
        assert_eq!(NodeKind::List(Vec::new()).as_list(), Some(&[] as &[Node]));
        // Strict-`List`-only boundary vs the sibling compound arms `Map`
        // and `Vector` — both also carry `Vec<Node>` payloads, so a hand-
        // rolled `matches!(_.kind, NodeKind::List(_))` gate has to keep
        // the boundary in view at every site. A future refactor that
        // widened this accessor to admit `Map` or `Vector` (silently
        // collapsing the semantic distinction between the parenthesized-
        // list, brace-map, and bracket-vector dialects) trips at these
        // pins before the eleven per-consumer sites silently start
        // accepting brace / bracket forms in `(defX …)` / `:atributos` /
        // `:key value` slots.
        assert_eq!(
            NodeKind::Map(Vec::new()).as_list(),
            None,
            "Map arm carries Vec<Node> but MUST NOT project through \
             as_list — the strict-List-only boundary is the whole point \
             of a distinct compound-arm accessor"
        );
        assert_eq!(
            NodeKind::Vector(Vec::new()).as_list(),
            None,
            "Vector arm carries Vec<Node> but MUST NOT project through \
             as_list — the strict-List-only boundary is the whole point \
             of a distinct compound-arm accessor"
        );
    }

    // Zero-copy pin — `k.as_list()` must borrow from the `List` arm's
    // own [`Vec<Node>`] storage, not clone into a fresh Vec. Fails at
    // build time if a future rewrite regresses to `Some(items.clone()
    // .as_slice().to_vec().leak())` or any other detour that silently
    // allocates on every call (the same shape as the sibling
    // [`as_keyword_is_by_borrow_pointer_identity`] /
    // [`as_symbol_is_by_borrow_pointer_identity`] /
    // [`as_str_is_by_borrow_pointer_identity`] /
    // [`as_atom_string_is_by_borrow_pointer_identity`] /
    // [`as_symbol_or_str_is_by_borrow_pointer_identity`] pins on the
    // peer outer-`NodeKind` scalar and disjunctive-scalar accessors).
    #[test]
    fn as_list_is_by_borrow_pointer_identity() {
        let k = NodeKind::List(vec![
            Node::new(NodeKind::Symbol("defcaixa".into()), Span::new(0, 8)),
            Node::new(NodeKind::Symbol("demo".into()), Span::new(9, 13)),
        ]);
        let via_accessor: &[Node] = k.as_list().unwrap();
        let NodeKind::List(ref inner) = k else {
            unreachable!("constructed above as NodeKind::List");
        };
        assert_eq!(
            via_accessor.as_ptr(),
            inner.as_ptr(),
            "NodeKind::as_list must borrow from the List arm's \
             Vec<Node> backing storage (zero-copy projection)",
        );
        assert_eq!(
            via_accessor.len(),
            inner.len(),
            "NodeKind::as_list and inner.as_slice() must byte-equal \
             in length (same slice)",
        );
    }

    // Byte-parity pin on the pre-lift `let NodeKind::List(items) = &X.kind
    // else { … }` shape the eleven caixa-ast/caixa-lint/caixa-teia/
    // caixa-fmt consumer sites route through today via `.as_list()`.
    // Refuses a future accidental split between the accessor's return
    // contract and its pre-lift shape (a hand-rolled shadow `impl` that
    // overrides one path, an accidental rebrand of one converged call
    // site back to the raw `NodeKind::List(items)` form, a widening to
    // admit `Map` or `Vector`) on the load-bearing list-shape-projection
    // axis every downstream form walker / `:atributos` gate / positional
    // classifier partitions on.
    #[test]
    fn as_list_byte_equal_pre_lift_pattern_match_shape() {
        for (variant, name) in all_variants() {
            let via_pattern: Option<&[Node]> = match &variant {
                NodeKind::List(items) => Some(items.as_slice()),
                _ => None,
            };
            let via_accessor = variant.as_list();
            assert_eq!(
                via_accessor, via_pattern,
                "NodeKind::{name}.as_list() must byte-equal \
                 `match &_ {{ NodeKind::List(items) => \
                 Some(items.as_slice()), _ => None }}` — otherwise the \
                 eleven converged caixa-ast/caixa-lint/caixa-teia/ \
                 caixa-fmt call sites would silently disagree with their \
                 pre-lift shape"
            );
        }
    }
}
