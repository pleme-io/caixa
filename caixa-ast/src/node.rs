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
    ///
    /// `pub const fn` — the body reads the arm discriminant through a
    /// per-arm pattern-match on `&self` that binds `k: &String` on the
    /// `Keyword` arm and projects onto its byte-borrowed `&str` view via
    /// [`String::as_str`] (`pub const fn` since Rust 1.87, well before
    /// this workspace's 1.89 MSRV floor); no arm-storage owning-borrow is
    /// taken, no drop is invoked on any arm's `String` / `Vec<Node>` /
    /// `Box<Node>` payload. Extends the const-eval discipline the sibling
    /// caixa-ast source-position primitive family (`Span::new` /
    /// `Span::point` / `Span::contains` / `Span::union` / `Span::len` /
    /// `Span::is_empty`, `Position::new` / `Position::origin`,
    /// `line_column`) and the paired [`Self::seq_delims`] /
    /// [`Self::reader_macro_prefix`] writer-half siblings on the
    /// compound-arm / reader-macro-arm sets already carry onto the
    /// caixa-ast [`NodeKind`] outer-sum-type's per-`Keyword`-arm
    /// borrowed-scalar-projection axis. Every downstream reader that
    /// wants a compile-time keyword-name-arm identity fixture (a `const
    /// LOOKUP: Option<&str> = NodeKind::Keyword(…).as_keyword();` future
    /// caixa-lint kwarg-key const-lookup table, a per-arm identity oracle
    /// a future caixa-lsp writer const-registry consults at compile time,
    /// a compile-time keyword-arm-set partition truth table the caixa-fmt
    /// writer keys off) now reads through one substrate-primitive const
    /// dispatch rather than being forced onto the runtime code path.
    #[must_use]
    pub const fn as_keyword(&self) -> Option<&str> {
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
    ///
    /// `pub const fn` — sibling in `const`-eval posture to the peer
    /// [`Self::as_keyword`] promotion on the same substrate-primitive
    /// per-arm scalar-projection family; the body is body-preserving
    /// verbatim (same `match &self { Self::Symbol(s) => Some(s.as_str()),
    /// _ => None }` shape, same [`String::as_str`] `pub const fn` const-
    /// stable since Rust 1.87 the [`Self::as_keyword`] promotion routes
    /// through), so the promotion extends the const-eval discipline onto
    /// the second per-arm borrowed-scalar-projection axis of the closed
    /// three-arm `Keyword` / `Symbol` / `Str` per-`String`-arm family the
    /// sibling [`Self::as_str`] promotion closes onto the third arm.
    #[must_use]
    pub const fn as_symbol(&self) -> Option<&str> {
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
    ///
    /// `pub const fn` — closes the const-eval-surface promotion the
    /// paired [`Self::as_keyword`] / [`Self::as_symbol`] siblings opened
    /// onto the third and last per-`String`-arm axis of the closed
    /// three-arm `Keyword` / `Symbol` / `Str` per-arm scalar-projection
    /// family. Body-preserving verbatim (same `match &self { Self::Str(s)
    /// => Some(s.as_str()), _ => None }` shape, same [`String::as_str`]
    /// `pub const fn` const-stable since Rust 1.87 the two sibling
    /// promotions route through), so every downstream consumer that
    /// wants a compile-time string-literal-arm identity fixture (a
    /// `const IS_FLAG: Option<&str> = NodeKind::Str("--flag".into())
    /// .as_str();`-shaped future caixa-fmt writer const-lookup table over
    /// a `const` fixture that widens `String::from` to `const` once that
    /// lands upstream, a per-arm identity oracle a future caixa-lint
    /// no-string-literal-in-numeric-position rule consults at compile
    /// time, a compile-time Str-arm-set partition truth table the
    /// caixa-lsp writer keys off) now reads through one substrate-
    /// primitive const dispatch rather than being forced onto the runtime
    /// code path.
    #[must_use]
    pub const fn as_str(&self) -> Option<&str> {
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

    /// Substrate-canonical projection onto the disjunctive
    /// [`Self::Quote`] | [`Self::Quasiquote`] | [`Self::Unquote`] |
    /// [`Self::UnquoteSplice`] reader-macro arm-set — returns
    /// `Some(&Node)` byte-borrowed from the matched arm's own
    /// [`Box<Node>`] storage, and [`None`] on every other arm of the
    /// closed fourteen-arm [`NodeKind`] variant set.
    ///
    /// Four production consumers today across four caixa-monorepo crates
    /// — the caixa-ast [`crate::visit::walk`] visitor recursion, the
    /// caixa-fmt printer's `contains_comment` sub-tree comment probe,
    /// the caixa-teia `node_to_value` manifest lowerer, and the caixa-
    /// lint per-file `walk` traversal — which previously reached the
    /// underlying inner-node payload through four raw `NodeKind::Quote
    /// (inner) | NodeKind::Quasiquote(inner) | NodeKind::Unquote(inner)
    /// | NodeKind::UnquoteSplice(inner) => recurse(inner)` open-coded
    /// four-arm disjunctive pattern-matches that expressed no compile-
    /// time link back to the substrate primitive's typed reader-macro-
    /// carrying arm-set. Each is a transparent-wrapper unwrap: the
    /// consumer does not care WHICH reader macro variant it is, only
    /// that the wrapper hides an inner [`Node`] whose recursion is the
    /// whole traversal.
    ///
    /// Semantically distinct from the peer per-arm compound projections
    /// [`Self::as_list`] (48efa3b) and from the sibling per-arm scalar
    /// projections [`Self::as_keyword`] (6804427) / [`Self::as_symbol`]
    /// (e96eea1) / [`Self::as_str`] (55b2909) / disjunctive scalar
    /// [`Self::as_atom_string`] (3c3ca48) / [`Self::as_symbol_or_str`]
    /// (fd39cea) — those project onto borrowed scalars or borrowed
    /// [`Vec<Node>`] slices; this one projects onto a borrowed inner
    /// [`Node`], the first projection accessor on the outer-`NodeKind`
    /// sum-type to reach across the [`Box<Node>`] indirection every
    /// reader-macro arm carries. A future reader-macro arm addition (a
    /// hypothetical `NodeKind::Splice(Box<Node>)` shape once the
    /// tatara-lisp reader grows a splicing-quote variant, a
    /// `NodeKind::TaggedQuote(Box<Node>, QuoteTag)` shape once the
    /// sexp→JSON bridge stabilizes tagged-quote sugar) folds into this
    /// projection by extending the accessor's arm-set once at the
    /// substrate primitive, rather than a four-way coordinated rewrite
    /// across every downstream walker.
    ///
    /// Zero-copy — the returned `&Node` borrows from the matched arm's
    /// own [`Box<Node>`] storage (pinned by the
    /// `as_reader_macro_inner_is_by_borrow_pointer_identity` test), the
    /// same discipline as the sibling per-arm scalar and compound
    /// projections. Seventh `Option<&<payload>>` projection accessor on
    /// the outer [`NodeKind`] sum-type — the first accessor on the
    /// *reader-macro* arm-family (Quote/Quasiquote/Unquote/UnquoteSplice
    /// all carry `Box<Node>`), extending the projection family from the
    /// three scalar-arm, two disjunctive-scalar-arm, and one compound-
    /// arm accessors onto the fourth arm-family every downstream
    /// authoring-tool walker and manifest-lowerer partitions on.
    #[must_use]
    pub fn as_reader_macro_inner(&self) -> Option<&Node> {
        match self {
            Self::Quote(inner)
            | Self::Quasiquote(inner)
            | Self::Unquote(inner)
            | Self::UnquoteSplice(inner) => Some(inner),
            _ => None,
        }
    }

    /// Substrate-canonical projection onto the disjunctive
    /// [`Self::List`] | [`Self::Map`] | [`Self::Vector`] compound-body
    /// arm-set — returns `Some(&[Node])` byte-borrowed from the matched
    /// arm's own [`Vec<Node>`] storage on any of the three D4-dialect
    /// compound-carrying arms (parenthesized list, brace map, bracket
    /// vector), and [`None`] on every other arm of the closed
    /// fourteen-arm [`NodeKind`] variant set.
    ///
    /// Three production consumers today across two caixa-monorepo crates
    /// — the caixa-ast [`crate::visit::walk`] visitor recursion (which
    /// treats every compound as a child-bearing container to descend
    /// into), the caixa-fmt printer's `emit_header_operand` inlineable
    /// gate (which flattens any D4-dialect compound header operand that
    /// fits the width budget), and the caixa-fmt printer's `is_atom`
    /// grid-cell classifier (which refuses any D4-dialect compound as a
    /// grid cell because it carries its own layout) — which previously
    /// reached the underlying `Vec<Node>` child sequence through three
    /// raw `NodeKind::List(items) | NodeKind::Map(items) |
    /// NodeKind::Vector(items) => …` open-coded three-arm disjunctive
    /// pattern-matches that expressed no compile-time link back to the
    /// substrate primitive's typed compound-body arm-set. Each site is a
    /// compound-shape-agnostic projection: the consumer does not care
    /// WHICH D4-dialect compound arm it is (the delimiter distinction
    /// matters to per-arm emit sites, not to walkers), only that the
    /// arm hides a `Vec<Node>` child sequence whose iteration is the
    /// whole traversal.
    ///
    /// Semantically distinct from the sibling [`Self::as_list`] (48efa3b)
    /// single-arm compound projection — that one pins the strict-`List`-
    /// only boundary the manifest-parser and positional-classifier sites
    /// gate on (a `(defcaixa …)` form is a `List`, not a `Map` or
    /// `Vector`); this one lifts the disjunctive three-arm compound-body
    /// arm-set every compound-shape-agnostic walker / header-inliner /
    /// grid-cell classifier reaches for. A future D4-adjacent compound
    /// arm addition (a hypothetical `NodeKind::Set(Vec<Node>)` shape
    /// once the tatara-lisp reader grows a `#{…}` set literal, a
    /// `NodeKind::Tuple(Vec<Node>)` shape once the sexp→JSON bridge
    /// stabilizes fixed-arity tuple sugar) folds into this projection
    /// by extending the accessor's arm-set once at the substrate
    /// primitive, rather than a three-way coordinated rewrite across
    /// every downstream walker.
    ///
    /// Zero-copy — the returned `&[Node]` borrows from the matched arm's
    /// own [`Vec<Node>`] storage (pinned by the
    /// `as_seq_body_is_by_borrow_pointer_identity` test), the same
    /// discipline as the sibling per-arm [`Self::as_list`] (48efa3b)
    /// compound and [`Self::as_reader_macro_inner`] (20be266) reader-
    /// macro projections. Eighth `Option<&<payload>>` projection
    /// accessor on the outer [`NodeKind`] sum-type — extends the
    /// projection family from the two disjunctive-scalar and one
    /// reader-macro-arm-set accessors onto the disjunctive-compound-arm
    /// axis every downstream compound-shape-agnostic walker partitions
    /// on.
    #[must_use]
    pub fn as_seq_body(&self) -> Option<&[Node]> {
        match self {
            Self::List(items) | Self::Map(items) | Self::Vector(items) => Some(items.as_slice()),
            _ => None,
        }
    }

    /// Substrate-canonical projection onto the delimiter-pair `(open, close)`
    /// that the tatara-lisp reader consumes to build the three D4-dialect
    /// compound arms — `Self::List` reads as `('(', ')')`, `Self::Map` as
    /// `('{', '}')`, `Self::Vector` as `('[', ']')` — and returns [`None`]
    /// on every other arm of the closed fourteen-arm [`NodeKind`] variant
    /// set. Companion to the sibling [`Self::as_seq_body`] compound-body
    /// projection: the reader/writer duality's two halves — the body slice
    /// the arm carries, and the two delimiter bytes the arm reads back as
    /// — now each dispatch through one substrate accessor rather than a
    /// three-way per-arm pattern-match repeated at every writer site.
    ///
    /// Four production consumer sites today in caixa-fmt/src/printer.rs
    /// — the top-level `emit` main compound-arm dispatch, the
    /// `emit_header_operand` inlineable branch (previously guarded by
    /// [`Self::as_seq_body`] then re-matched with a `_ => unreachable!`
    /// trap), the `render_node_inline` inline-render main compound-arm
    /// dispatch, and the `classify_is_total_and_names_the_expected_shape`
    /// test helper — which previously reached the delimiter pair through
    /// three raw `NodeKind::List(_) => Delims::PAREN | NodeKind::Map(_)
    /// => Delims::BRACE | NodeKind::Vector(_) => Delims::BRACKET`
    /// open-coded per-arm pattern-matches. The doc-comment on caixa-fmt's
    /// own `Delims` type already names the invariant this accessor lifts:
    /// "`(…)`, `{…}` and `[…]` differ ONLY in these two bytes."
    ///
    /// Contract with [`Self::as_seq_body`]: for every variant, either both
    /// return [`Some`] (the three D4-dialect compound arms) or both return
    /// [`None`] (every other arm) — pinned by
    /// `seq_delims_partitions_the_same_arm_set_as_as_seq_body`. A future
    /// D4-adjacent compound arm addition (a hypothetical
    /// `NodeKind::Set(Vec<Node>)` shape once the tatara-lisp reader grows
    /// a `#{…}` set literal) folds onto both accessors at exactly the
    /// substrate — the writer half by extending this partition once, the
    /// walker half by extending [`Self::as_seq_body`]'s — rather than a
    /// coordinated rewrite across every per-consumer writer site.
    ///
    /// `pub const fn` — the body reads the arm discriminant through a
    /// `_`-binding pattern-match that projects onto a `Copy`-shaped
    /// `Option<(char, char)>` return, no arm-storage borrow is taken, and
    /// no drop is invoked on any arm's `Vec<Node>` payload (the `_`
    /// pattern binds nothing). Pattern matching on `&Self` where the
    /// enum carries drop-typed arms has been const-stable in Rust since
    /// long before this workspace's 1.89 MSRV floor. Extends the
    /// const-eval discipline the sibling caixa-ast source-position
    /// primitive family (`Span::new` / `Span::point` / `Span::contains`
    /// / `Span::union` / `Span::len` / `Span::is_empty`, `Position::new`
    /// / `Position::origin`, `line_column`) and the paired
    /// [`Self::reader_macro_prefix`] writer-half sibling on the reader-
    /// macro-arm-set already carry onto the caixa-ast [`NodeKind`] outer-
    /// sum-type's per-arm identity-projection axis. Every downstream
    /// writer that wants a compile-time delimiter-pair fixture (a
    /// `const PAREN: Option<(char, char)> =
    /// NodeKind::List(…).seq_delims();` future caixa-fmt writer
    /// const-lookup table, a per-arm identity oracle a future caixa-lint
    /// no-delimiter-in-non-compound-context rule consults at compile
    /// time, a compile-time compound-arm-set partition truth table the
    /// caixa-lsp writer keys off) now reads through one substrate-
    /// primitive const dispatch rather than being forced onto the
    /// runtime code path.
    #[must_use]
    pub const fn seq_delims(&self) -> Option<(char, char)> {
        match self {
            Self::List(_) => Some(('(', ')')),
            Self::Map(_) => Some(('{', '}')),
            Self::Vector(_) => Some(('[', ']')),
            _ => None,
        }
    }

    /// Substrate-canonical projection onto the writer-side sigil prefix
    /// that the tatara-lisp reader consumes to build the four reader-macro
    /// arms — `Self::Quote` reads as one apostrophe byte, `Self::Quasiquote`
    /// as one backtick byte, `Self::Unquote` as one comma byte,
    /// `Self::UnquoteSplice` as two bytes (comma then at-sign) — and
    /// returns [`None`] on every other arm of the closed fourteen-arm
    /// [`NodeKind`] variant set. Companion to the sibling
    /// [`Self::as_reader_macro_inner`] reader-macro-inner projection: the
    /// reader/writer duality's two halves on the reader-macro-arm-set —
    /// the boxed inner node the arm carries, and the sigil bytes the arm
    /// reads back as — now each dispatch through one substrate accessor
    /// rather than a four-arm per-arm pattern-match repeated at every
    /// writer site.
    ///
    /// Two production consumer sites today in caixa-fmt/src/printer.rs
    /// — the top-level `Printer::emit` main reader-macro-arm dispatch, and
    /// the peer `render_node_inline` inline-render main reader-macro-arm
    /// dispatch — which previously reached the sigil bytes through four
    /// raw per-arm pattern-matches (`Quote` pushing the apostrophe byte,
    /// `Quasiquote` pushing the backtick byte, `Unquote` pushing the
    /// comma byte, `UnquoteSplice` pushing the two-byte comma-then-at-
    /// sign) restated at each site. The fact that the four reader-macro
    /// arms differ ONLY in these one-or-two sigil bytes had no home in
    /// the type system — it lived as prose in the printer's own arm
    /// dispatch, and a copy-paste that flipped one arm's sigil (routed
    /// `Quote` through the backtick byte and `Quasiquote` through the
    /// apostrophe byte) would silently rewrite every quote as a
    /// quasiquote and vice versa at every writer site.
    ///
    /// Contract with [`Self::as_reader_macro_inner`]: for every variant,
    /// either both accessors return [`Some`] (the four reader-macro arms)
    /// or both return [`None`] (every other arm) — pinned by
    /// `reader_macro_prefix_partitions_the_same_arm_set_as_as_reader_macro_inner`.
    /// A future reader-macro arm addition (a hypothetical
    /// `NodeKind::Splice(Box<Node>)` shape once the tatara-lisp reader
    /// grows a splicing-quote variant, a `NodeKind::TaggedQuote(Box<Node>,
    /// QuoteTag)` shape once the sexp→JSON bridge stabilizes tagged-quote
    /// sugar — the same extension axis the sibling
    /// [`Self::as_reader_macro_inner`] doc-comment names) folds onto both
    /// accessors at exactly the substrate — the writer half by extending
    /// this partition once, the walker half by extending
    /// [`Self::as_reader_macro_inner`]'s — rather than a coordinated
    /// rewrite across every per-consumer writer site.
    ///
    /// Tenth `Option<&<payload>>`-shaped projection accessor on the
    /// outer-`NodeKind` sum-type — the writer-half sibling to
    /// [`Self::as_reader_macro_inner`] on the reader-macro-arm-set,
    /// mirroring the shape of the paired
    /// [`Self::seq_delims`] / [`Self::as_seq_body`] compound-arm-set
    /// accessors on the sibling D4-dialect compound axis. Returns
    /// `&'static str` rather than a borrow into arm storage because the
    /// sigil bytes are the arm's IDENTITY (like `seq_delims`'s
    /// `(char, char)` pair) rather than a payload the arm carries.
    ///
    /// `pub const fn` — the body reads the arm discriminant through a
    /// `_`-binding pattern-match that projects onto a `Copy`-shaped
    /// `Option<&'static str>` return, no arm-storage borrow is taken, and
    /// no drop is invoked on any arm's `Box<Node>` payload (the `_`
    /// pattern binds nothing). Pattern matching on `&Self` where the
    /// enum carries drop-typed arms has been const-stable in Rust since
    /// long before this workspace's 1.89 MSRV floor. Extends the
    /// const-eval discipline the sibling caixa-ast source-position
    /// primitive family (`Span::new` / `Span::point` / `Span::contains`
    /// / `Span::union` / `Span::len` / `Span::is_empty`, `Position::new`
    /// / `Position::origin`, `line_column`) already carries onto the
    /// caixa-ast [`NodeKind`] outer-sum-type's per-arm identity-projection
    /// axis. Every downstream writer that wants a compile-time reader-
    /// macro-sigil fixture (a `const QUOTE: Option<&'static str> =
    /// NodeKind::Quote(…).reader_macro_prefix();` future caixa-fmt writer
    /// const-lookup table, a per-arm identity oracle a future caixa-lint
    /// no-quote-sigil-in-non-reader-macro-context rule consults at
    /// compile time, a compile-time reader-macro-arm-set partition truth
    /// table the caixa-lsp writer keys off) now reads through one
    /// substrate-primitive const dispatch rather than being forced onto
    /// the runtime code path.
    #[must_use]
    pub const fn reader_macro_prefix(&self) -> Option<&'static str> {
        match self {
            Self::Quote(_) => Some("'"),
            Self::Quasiquote(_) => Some("`"),
            Self::Unquote(_) => Some(","),
            Self::UnquoteSplice(_) => Some(",@"),
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
    ///
    /// Route the D4-dialect compound-body lowering through the lifted
    /// [`NodeKind::as_seq_body`] `Option<&[Node]>` accessor rather than
    /// the raw four-arm `NodeKind::List(items) => Sexp::List(…) |
    /// NodeKind::Map(items) | NodeKind::Vector(items) => Sexp::List(…)`
    /// open-coded per-arm dispatch — sibling in shape to the peer
    /// [`crate::visit::walk`], `caixa-fmt::Printer::emit`,
    /// `caixa-fmt::render_node_inline`, `caixa-fmt::emit_header_operand`,
    /// `caixa-fmt::is_atom`, and `caixa-teia::node_to_value` compound-body
    /// sites that all key off the same substrate-canonical accessor. The
    /// three D4-dialect compound arms produce IDENTICAL `Sexp::List`
    /// bodies (only the brace-ness is dropped — see the historical note
    /// below), so the pre-lift shape was two match arms restating the
    /// same `Sexp::List(items.iter().map(Node::to_tatara_sexp).collect())`
    /// body; the lift folds both onto one dispatch that gates on the
    /// substrate-primitive compound-body-carrying arm-set.
    ///
    /// `tatara_lisp::Sexp` has no Map/Vector variant yet — adding them is
    /// a LANGUAGE change, sequenced as Phase 2 of
    /// theory/TATARA-LISP-CONSOLIDATION.md D4 and gated on its own
    /// differential run over the 1,123-file corpus (correction C4). Until
    /// that lands, all three compound arms lower to a plain list: the
    /// elements survive in order, only the brace-ness is dropped. That is
    /// strictly closer to intent than the pre-D4 behaviour, where the
    /// delimiters lowered as literal `{` / `}` SYMBOLS inside the list.
    /// This projection is used only by the round-trip equivalence tests,
    /// which stay honest because formatting re-emits the delimiters and
    /// re-parsing recovers the node.
    #[must_use]
    pub fn to_tatara_sexp(&self) -> tatara_lisp::Sexp {
        use tatara_lisp::{Atom, Sexp};
        if let Some(items) = self.kind.as_seq_body() {
            return Sexp::List(items.iter().map(Node::to_tatara_sexp).collect());
        }
        match &self.kind {
            NodeKind::Nil => Sexp::Nil,
            NodeKind::Symbol(s) => Sexp::Atom(Atom::Symbol(s.clone())),
            NodeKind::Keyword(s) => Sexp::Atom(Atom::Keyword(s.clone())),
            NodeKind::Str(s) => Sexp::Atom(Atom::Str(s.clone())),
            NodeKind::Int(i) => Sexp::Atom(Atom::Int(*i)),
            NodeKind::Float(f) => Sexp::Atom(Atom::Float(*f)),
            NodeKind::Bool(b) => Sexp::Atom(Atom::Bool(*b)),
            NodeKind::Quote(inner) => Sexp::Quote(Box::new(inner.to_tatara_sexp())),
            NodeKind::Quasiquote(inner) => Sexp::Quasiquote(Box::new(inner.to_tatara_sexp())),
            NodeKind::Unquote(inner) => Sexp::Unquote(Box::new(inner.to_tatara_sexp())),
            NodeKind::UnquoteSplice(inner) => Sexp::UnquoteSplice(Box::new(inner.to_tatara_sexp())),
            NodeKind::List(_) | NodeKind::Map(_) | NodeKind::Vector(_) => {
                unreachable!("compound-arm-set routed through NodeKind::as_seq_body above")
            }
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
        let start = usize::from(items.first().is_some_and(|n| n.kind.is_symbol()));
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
            if let Some(k) = items[i].kind.as_keyword()
                && k == key
            {
                return Some(&items[i + 1]);
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

    // Pin the const-eval surface: the substrate-primitive per-arm
    // scalar-projection family — [`NodeKind::as_keyword`],
    // [`NodeKind::as_symbol`], [`NodeKind::as_str`] — reaches into `const`
    // context, so a future compile-time caixa-fmt writer-side per-arm-
    // identity truth-table / caixa-lint keyword-key const-lookup /
    // caixa-lsp writer const-registry can key off the three sibling
    // borrowed-scalar-projection accessors without being forced onto the
    // runtime code path. The `const _: () = assert!(…)` bindings resolve
    // each projection at compile time on the non-`String`-payload arm-set
    // (the three `String`-carrying arms themselves cannot be constructed
    // in `const` context yet — `String::new` widens the const-eval surface
    // once `String::from` / `From<&str>` reach `const`, sibling to the
    // same rust-lang/rust #143874 tracking issue the peer `Span::union`
    // open-codes `Ord::min` / `Ord::max` around), pinning the four `Nil` /
    // `Int` / `Bool` / `Float` non-`String`-payload atom arms — each of
    // which is `const`-constructible in-place through its raw arm ctor
    // and each of which must fall through the accessor's `_ => None` arm
    // on all three projections. Any regression that drops `pub const fn`
    // back to `pub fn` on any of the three (a body edit that reaches for
    // a non-const operation on the accessor path) fails this test at
    // compile time rather than at runtime, matching the sibling caixa-ast
    // source-position primitive family's `Span::new` / `Span::point` /
    // `Span::contains` / `Span::union` / `Span::len` / `Span::is_empty` /
    // `Position::new` / `Position::origin` / `line_column` `pub const fn`
    // shape's const-eval discipline and the paired [`NodeKind::seq_delims`]
    // / [`NodeKind::reader_macro_prefix`] writer-half sibling promotions
    // on the compound-arm / reader-macro-arm sets — extended onto the
    // caixa-ast [`NodeKind`] outer-sum-type's per-`String`-arm borrowed-
    // scalar-projection axis.
    #[test]
    fn as_keyword_as_symbol_as_str_are_const() {
        const NIL: NodeKind = NodeKind::Nil;
        const NIL_KEYWORD: Option<&str> = NIL.as_keyword();
        const NIL_SYMBOL: Option<&str> = NIL.as_symbol();
        const NIL_STR: Option<&str> = NIL.as_str();
        const _: () = assert!(NIL_KEYWORD.is_none());
        const _: () = assert!(NIL_SYMBOL.is_none());
        const _: () = assert!(NIL_STR.is_none());

        const INT: NodeKind = NodeKind::Int(0);
        const INT_KEYWORD: Option<&str> = INT.as_keyword();
        const INT_SYMBOL: Option<&str> = INT.as_symbol();
        const INT_STR: Option<&str> = INT.as_str();
        const _: () = assert!(INT_KEYWORD.is_none());
        const _: () = assert!(INT_SYMBOL.is_none());
        const _: () = assert!(INT_STR.is_none());

        const BOOL: NodeKind = NodeKind::Bool(false);
        const BOOL_KEYWORD: Option<&str> = BOOL.as_keyword();
        const BOOL_SYMBOL: Option<&str> = BOOL.as_symbol();
        const BOOL_STR: Option<&str> = BOOL.as_str();
        const _: () = assert!(BOOL_KEYWORD.is_none());
        const _: () = assert!(BOOL_SYMBOL.is_none());
        const _: () = assert!(BOOL_STR.is_none());

        const FLOAT: NodeKind = NodeKind::Float(0.0);
        const FLOAT_KEYWORD: Option<&str> = FLOAT.as_keyword();
        const FLOAT_SYMBOL: Option<&str> = FLOAT.as_symbol();
        const FLOAT_STR: Option<&str> = FLOAT.as_str();
        const _: () = assert!(FLOAT_KEYWORD.is_none());
        const _: () = assert!(FLOAT_SYMBOL.is_none());
        const _: () = assert!(FLOAT_STR.is_none());
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

    // Projection contract on the outer-`NodeKind` sum-type's disjunctive
    // four-arm `Quote` | `Quasiquote` | `Unquote` | `UnquoteSplice`
    // reader-macro-carrying accessor: exactly the four reader-macro-
    // carrying arms return `Some(&Node)` byte-borrowed from the matched
    // arm's own [`Box<Node>`] storage; every other arm of the closed
    // fourteen-arm partition returns `None`. Pins the "one canonical
    // projection dispatch per typed arm-set on the substrate primitive"
    // discipline the four per-disjunctive-arm-set consumer sites (caixa-
    // ast [`crate::visit::walk`] visitor recursion, caixa-fmt printer
    // `contains_comment` sub-tree probe, caixa-teia `node_to_value`
    // manifest lowerer, caixa-lint per-file `walk` traversal) route
    // through via `.as_reader_macro_inner().map(recurse)`. A regression
    // that admitted a non-reader-macro arm (a bare List, an atom) through
    // this projection would silently reroute the walker's recursion into
    // a compound-body traversal at every reader-macro site — the four-
    // arm walker converges would stop descending into unquoted forms and
    // start descending twice into every list.
    #[test]
    fn as_reader_macro_inner_projects_only_reader_macro_arms() {
        let variants = all_variants();
        for (variant, name) in &variants {
            let projected = variant.as_reader_macro_inner();
            let is_reader_macro_arm = matches!(
                variant,
                NodeKind::Quote(_)
                    | NodeKind::Quasiquote(_)
                    | NodeKind::Unquote(_)
                    | NodeKind::UnquoteSplice(_)
            );
            if is_reader_macro_arm {
                let expected: &Node = match variant {
                    NodeKind::Quote(inner)
                    | NodeKind::Quasiquote(inner)
                    | NodeKind::Unquote(inner)
                    | NodeKind::UnquoteSplice(inner) => inner.as_ref(),
                    _ => unreachable!("guarded by is_reader_macro_arm above"),
                };
                assert_eq!(
                    projected,
                    Some(expected),
                    "NodeKind::{name} is a reader-macro-carrying arm — \
                     as_reader_macro_inner() must project onto its own \
                     Box<Node> inner payload"
                );
            } else {
                assert_eq!(
                    projected, None,
                    "NodeKind::{name} is not a reader-macro-carrying arm \
                     — as_reader_macro_inner() must return None"
                );
            }
        }
        // Strict-boundary vs the sibling compound-carrying arms — both
        // `List` and `Map` and `Vector` carry compound child sequences,
        // but reader-macros carry a single boxed inner node; a future
        // refactor that widened this accessor to admit the compound arms
        // (silently collapsing the semantic distinction between a
        // reader-macro wrapper `'x` and a container `(x)`) trips at
        // these pins before the four per-consumer walker sites silently
        // start unwrapping list bodies as reader-macro inners.
        assert_eq!(
            NodeKind::List(Vec::new()).as_reader_macro_inner(),
            None,
            "List arm carries a Vec<Node> body but MUST NOT project through \
             as_reader_macro_inner — the reader-macro-only boundary is the \
             whole point of a distinct projection"
        );
        assert_eq!(
            NodeKind::Map(Vec::new()).as_reader_macro_inner(),
            None,
            "Map arm carries a Vec<Node> body but MUST NOT project through \
             as_reader_macro_inner — the reader-macro-only boundary is the \
             whole point of a distinct projection"
        );
        assert_eq!(
            NodeKind::Vector(Vec::new()).as_reader_macro_inner(),
            None,
            "Vector arm carries a Vec<Node> body but MUST NOT project through \
             as_reader_macro_inner — the reader-macro-only boundary is the \
             whole point of a distinct projection"
        );
    }

    // Zero-copy pin — `k.as_reader_macro_inner()` must borrow from the
    // matched arm's own [`Box<Node>`] storage, not clone into a fresh
    // Node. Fails at build time if a future rewrite regresses to
    // `Some(Box::leak(inner.clone()))` or any other detour that silently
    // allocates on every call. Pinned across all four reader-macro-
    // carrying arms (Quote/Quasiquote/Unquote/UnquoteSplice) — a per-
    // arm-specific regression would slip past a single-arm pin — the
    // same shape as the sibling
    // [`as_atom_string_is_by_borrow_pointer_identity`] /
    // [`as_symbol_or_str_is_by_borrow_pointer_identity`] pins on the
    // peer disjunctive scalar accessors, extended onto the Box<Node>
    // indirection every reader-macro arm carries.
    #[test]
    fn as_reader_macro_inner_is_by_borrow_pointer_identity() {
        for (constructor, ctor_name) in [
            (
                (|n: Node| NodeKind::Quote(Box::new(n))) as fn(Node) -> NodeKind,
                "Quote",
            ),
            (
                (|n: Node| NodeKind::Quasiquote(Box::new(n))) as fn(Node) -> NodeKind,
                "Quasiquote",
            ),
            (
                (|n: Node| NodeKind::Unquote(Box::new(n))) as fn(Node) -> NodeKind,
                "Unquote",
            ),
            (
                (|n: Node| NodeKind::UnquoteSplice(Box::new(n))) as fn(Node) -> NodeKind,
                "UnquoteSplice",
            ),
        ] {
            let inner = Node::new(NodeKind::Symbol(format!("s-{ctor_name}")), Span::new(0, 8));
            let k = constructor(inner);
            let via_accessor: &Node = k.as_reader_macro_inner().unwrap();
            let inner_ref: &Node = match &k {
                NodeKind::Quote(b)
                | NodeKind::Quasiquote(b)
                | NodeKind::Unquote(b)
                | NodeKind::UnquoteSplice(b) => b.as_ref(),
                _ => unreachable!("constructed above via reader-macro arm ctor"),
            };
            assert!(
                std::ptr::eq(via_accessor, inner_ref),
                "NodeKind::as_reader_macro_inner on the {ctor_name} arm \
                 must borrow from that arm's Box<Node> heap allocation \
                 (zero-copy projection through the boxed indirection)",
            );
        }
    }

    // Byte-parity pin on the pre-lift four-arm disjunctive
    // `match &_.kind { NodeKind::Quote(inner) | NodeKind::Quasiquote(inner)
    // | NodeKind::Unquote(inner) | NodeKind::UnquoteSplice(inner) =>
    // recurse(inner), _ => … }` shape the four caixa-ast/caixa-fmt/
    // caixa-teia/caixa-lint consumer sites route through today via
    // `.as_reader_macro_inner()`. Refuses a future accidental split
    // between the accessor's return contract and its pre-lift disjunctive
    // shape (a hand-rolled shadow `impl` that overrides one path, an
    // accidental rebrand of one converged call site back to the raw
    // four-arm `NodeKind::Quote(inner) | …` form, a widening to admit a
    // compound arm) on the load-bearing reader-macro-carrying disjunctive
    // axis every downstream walker recursion partitions on.
    #[test]
    fn as_reader_macro_inner_byte_equal_pre_lift_pattern_match_shape() {
        for (variant, name) in all_variants() {
            let via_pattern: Option<&Node> = match &variant {
                NodeKind::Quote(inner)
                | NodeKind::Quasiquote(inner)
                | NodeKind::Unquote(inner)
                | NodeKind::UnquoteSplice(inner) => Some(inner.as_ref()),
                _ => None,
            };
            let via_accessor = variant.as_reader_macro_inner();
            assert_eq!(
                via_accessor, via_pattern,
                "NodeKind::{name}.as_reader_macro_inner() must byte-equal \
                 `match &_ {{ NodeKind::Quote(inner) | \
                 NodeKind::Quasiquote(inner) | NodeKind::Unquote(inner) | \
                 NodeKind::UnquoteSplice(inner) => Some(inner.as_ref()), \
                 _ => None }}` — otherwise the four converged caixa-ast/ \
                 caixa-fmt/caixa-teia/caixa-lint walker sites would \
                 silently disagree with their pre-lift shape"
            );
        }
    }

    // Projection contract on the outer-`NodeKind` sum-type's disjunctive
    // three-arm `List` | `Map` | `Vector` D4-dialect compound-body arm-
    // set accessor: exactly the three compound-carrying arms return
    // `Some(&[Node])` byte-borrowed from the matched arm's own
    // [`Vec<Node>`] storage; every other arm of the closed fourteen-arm
    // partition returns `None`. Pins the "one canonical projection
    // dispatch per typed arm-set on the substrate primitive" discipline
    // the three per-disjunctive-arm-set consumer sites (caixa-ast
    // [`crate::visit::walk`] visitor recursion, caixa-fmt printer
    // `emit_header_operand` inlineable gate, caixa-fmt printer `is_atom`
    // grid-cell classifier) route through via `.as_seq_body()`. A
    // regression that admitted a non-compound arm (an atom, a reader-
    // macro wrapper) through this projection would silently reroute the
    // walker's recursion into an atom-body traversal at every compound
    // site — the three-arm walker converges would start descending into
    // atoms and stop descending into brace/bracket dialects.
    #[test]
    fn as_seq_body_projects_only_compound_arms() {
        let variants = all_variants();
        for (variant, name) in &variants {
            let projected = variant.as_seq_body();
            let is_compound_arm = matches!(
                variant,
                NodeKind::List(_) | NodeKind::Map(_) | NodeKind::Vector(_)
            );
            if is_compound_arm {
                let expected: &[Node] = match variant {
                    NodeKind::List(items) | NodeKind::Map(items) | NodeKind::Vector(items) => {
                        items.as_slice()
                    }
                    _ => unreachable!("guarded by is_compound_arm above"),
                };
                assert_eq!(
                    projected,
                    Some(expected),
                    "NodeKind::{name} is a compound-carrying arm — \
                     as_seq_body() must project onto its own Vec<Node> \
                     body"
                );
            } else {
                assert_eq!(
                    projected, None,
                    "NodeKind::{name} is not a compound-carrying arm — \
                     as_seq_body() must return None"
                );
            }
        }
        // Empty compound — the accessor is a projection, not a gate; an
        // empty-`()` list, empty-`{}` map, and empty-`[]` vector each
        // round-trip as `Some(&[])`, not `None`, because the arm still
        // carries a (zero-length) `Vec<Node>` body a walker is entitled
        // to iterate over.
        assert_eq!(
            NodeKind::List(Vec::new()).as_seq_body(),
            Some(&[] as &[Node])
        );
        assert_eq!(
            NodeKind::Map(Vec::new()).as_seq_body(),
            Some(&[] as &[Node])
        );
        assert_eq!(
            NodeKind::Vector(Vec::new()).as_seq_body(),
            Some(&[] as &[Node])
        );
        // Strict-compound-body-only boundary vs the sibling reader-macro-
        // carrying arms — all four reader-macro arms carry a boxed inner
        // `Node`, not a `Vec<Node>` body, so a widening to admit a
        // reader-macro arm through `as_seq_body` (silently collapsing the
        // semantic distinction between a container `(x y)` and a wrapper
        // `'x`) would break every walker that gates on the compound-body
        // arm-set. Pinned across all four reader-macro arms — a per-arm-
        // specific regression would slip past a single-arm pin.
        assert_eq!(
            NodeKind::Quote(Box::new(Node::new(NodeKind::Nil, Span::new(0, 0)))).as_seq_body(),
            None,
            "Quote arm carries a Box<Node> inner but MUST NOT project \
             through as_seq_body — the compound-body-only boundary is \
             the whole point of a distinct projection"
        );
        assert_eq!(
            NodeKind::Quasiquote(Box::new(Node::new(NodeKind::Nil, Span::new(0, 0)))).as_seq_body(),
            None,
            "Quasiquote arm carries a Box<Node> inner but MUST NOT \
             project through as_seq_body — the compound-body-only \
             boundary is the whole point of a distinct projection"
        );
        assert_eq!(
            NodeKind::Unquote(Box::new(Node::new(NodeKind::Nil, Span::new(0, 0)))).as_seq_body(),
            None,
            "Unquote arm carries a Box<Node> inner but MUST NOT project \
             through as_seq_body — the compound-body-only boundary is \
             the whole point of a distinct projection"
        );
        assert_eq!(
            NodeKind::UnquoteSplice(Box::new(Node::new(NodeKind::Nil, Span::new(0, 0))))
                .as_seq_body(),
            None,
            "UnquoteSplice arm carries a Box<Node> inner but MUST NOT \
             project through as_seq_body — the compound-body-only \
             boundary is the whole point of a distinct projection"
        );
    }

    // Zero-copy pin — `k.as_seq_body()` must borrow from the matched
    // arm's own [`Vec<Node>`] storage, not clone into a fresh Vec.
    // Fails at build time if a future rewrite regresses to
    // `Some(items.clone().as_slice().to_vec().leak())` or any other
    // detour that silently allocates on every call. Pinned across all
    // three compound-carrying arms (List/Map/Vector) — a per-arm-specific
    // regression would slip past a single-arm pin — the same shape as
    // the sibling
    // [`as_reader_macro_inner_is_by_borrow_pointer_identity`] pin on
    // the peer disjunctive reader-macro accessor, extended onto the
    // Vec<Node> compound-body indirection every D4-dialect arm carries.
    #[test]
    fn as_seq_body_is_by_borrow_pointer_identity() {
        for (constructor, ctor_name) in [
            (
                (|items: Vec<Node>| NodeKind::List(items)) as fn(Vec<Node>) -> NodeKind,
                "List",
            ),
            (
                (|items: Vec<Node>| NodeKind::Map(items)) as fn(Vec<Node>) -> NodeKind,
                "Map",
            ),
            (
                (|items: Vec<Node>| NodeKind::Vector(items)) as fn(Vec<Node>) -> NodeKind,
                "Vector",
            ),
        ] {
            let payload = vec![
                Node::new(NodeKind::Symbol(format!("s-{ctor_name}")), Span::new(0, 8)),
                Node::new(NodeKind::Int(42), Span::new(9, 11)),
            ];
            let k = constructor(payload);
            let via_accessor: &[Node] = k.as_seq_body().unwrap();
            let inner: &Vec<Node> = match &k {
                NodeKind::List(items) | NodeKind::Map(items) | NodeKind::Vector(items) => items,
                _ => unreachable!("constructed above via compound arm ctor"),
            };
            assert_eq!(
                via_accessor.as_ptr(),
                inner.as_ptr(),
                "NodeKind::as_seq_body on the {ctor_name} arm must borrow \
                 from that arm's Vec<Node> backing storage (zero-copy \
                 projection)",
            );
            assert_eq!(
                via_accessor.len(),
                inner.len(),
                "NodeKind::as_seq_body and inner.as_slice() must byte-equal \
                 in length (same slice) on the {ctor_name} arm",
            );
        }
    }

    // Byte-parity pin on the pre-lift three-arm disjunctive
    // `match &_.kind { NodeKind::List(items) | NodeKind::Map(items) |
    // NodeKind::Vector(items) => Some(items.as_slice()), _ => None }`
    // shape the three caixa-ast/caixa-fmt consumer sites route through
    // today via `.as_seq_body()`. Refuses a future accidental split
    // between the accessor's return contract and its pre-lift
    // disjunctive shape (a hand-rolled shadow `impl` that overrides
    // one path, an accidental rebrand of one converged call site back
    // to the raw three-arm `NodeKind::List(items) | …` form, a
    // widening to admit a reader-macro arm) on the load-bearing
    // compound-body-carrying disjunctive axis every downstream walker
    // recursion / header-inliner / grid-cell classifier partitions on.
    #[test]
    fn as_seq_body_byte_equal_pre_lift_pattern_match_shape() {
        for (variant, name) in all_variants() {
            let via_pattern: Option<&[Node]> = match &variant {
                NodeKind::List(items) | NodeKind::Map(items) | NodeKind::Vector(items) => {
                    Some(items.as_slice())
                }
                _ => None,
            };
            let via_accessor = variant.as_seq_body();
            assert_eq!(
                via_accessor, via_pattern,
                "NodeKind::{name}.as_seq_body() must byte-equal \
                 `match &_ {{ NodeKind::List(items) | NodeKind::Map(items) \
                 | NodeKind::Vector(items) => Some(items.as_slice()), _ \
                 => None }}` — otherwise the three converged caixa-ast/ \
                 caixa-fmt walker / header-inliner / grid-cell classifier \
                 sites would silently disagree with their pre-lift shape"
            );
        }
    }

    // Projection contract on the outer-`NodeKind` sum-type's writer-side
    // delimiter-pair accessor: exactly the three D4-dialect compound-
    // carrying arms return `Some((open, close))` with the per-arm
    // reader-side delimiter pair (`List` → `('(', ')')`, `Map` → `('{',
    // '}')`, `Vector` → `('[', ']')`); every other arm of the closed
    // fourteen-arm partition returns `None`. Pins the "one canonical
    // projection dispatch per typed arm-set on the substrate primitive"
    // discipline the four per-consumer writer sites in
    // caixa-fmt/src/printer.rs route through via `.seq_delims()`. A
    // regression that flipped one arm's delimiter pair (a copy-paste
    // that routed `List` through `('[', ']')` and `Vector` through
    // `('(', ')')`) would silently start rendering every parenthesised
    // form as a bracket vector and every vector as a list at every
    // writer site.
    #[test]
    fn seq_delims_projects_only_compound_arms_with_dialect_pair() {
        let variants = all_variants();
        for (variant, name) in &variants {
            let projected = variant.seq_delims();
            let expected = match variant {
                NodeKind::List(_) => Some(('(', ')')),
                NodeKind::Map(_) => Some(('{', '}')),
                NodeKind::Vector(_) => Some(('[', ']')),
                _ => None,
            };
            assert_eq!(
                projected, expected,
                "NodeKind::{name}.seq_delims() must project onto the \
                 per-arm reader-side delimiter pair (List → ('(', ')'), \
                 Map → ('{{', '}}'), Vector → ('[', ']')) — otherwise \
                 the four converged caixa-fmt writer sites would \
                 silently start emitting the wrong delimiters"
            );
        }
        // Empty compound — the accessor is a projection over the arm
        // discriminator, not a gate on the body length; an empty-`()`
        // list, empty-`{}` map, and empty-`[]` vector each round-trip
        // as `Some((open, close))` with the arm's own pair, because the
        // arm still IS a `List` / `Map` / `Vector` regardless of body
        // arity.
        assert_eq!(NodeKind::List(Vec::new()).seq_delims(), Some(('(', ')')));
        assert_eq!(NodeKind::Map(Vec::new()).seq_delims(), Some(('{', '}')));
        assert_eq!(NodeKind::Vector(Vec::new()).seq_delims(), Some(('[', ']')));
        // Strict-compound-arm-only boundary vs the sibling reader-macro-
        // carrying arms — all four reader-macro arms are wrapper syntax
        // (`'x` / `` `x `` / `,x` / `,@x`) with no delimiter PAIR (the
        // one-character sigil is not a matched pair the writer emits
        // around a body), so a future widening to admit a reader-macro
        // arm through `seq_delims` would silently start bracketing
        // reader-macro inners as if they were containers. Pinned across
        // all four reader-macro arms — a per-arm-specific regression
        // would slip past a single-arm pin.
        assert_eq!(
            NodeKind::Quote(Box::new(Node::new(NodeKind::Nil, Span::new(0, 0)))).seq_delims(),
            None,
            "Quote arm is reader-macro sigil syntax with no delimiter \
             pair — MUST NOT project through seq_delims"
        );
        assert_eq!(
            NodeKind::Quasiquote(Box::new(Node::new(NodeKind::Nil, Span::new(0, 0)))).seq_delims(),
            None,
            "Quasiquote arm is reader-macro sigil syntax with no \
             delimiter pair — MUST NOT project through seq_delims"
        );
        assert_eq!(
            NodeKind::Unquote(Box::new(Node::new(NodeKind::Nil, Span::new(0, 0)))).seq_delims(),
            None,
            "Unquote arm is reader-macro sigil syntax with no delimiter \
             pair — MUST NOT project through seq_delims"
        );
        assert_eq!(
            NodeKind::UnquoteSplice(Box::new(Node::new(NodeKind::Nil, Span::new(0, 0))))
                .seq_delims(),
            None,
            "UnquoteSplice arm is reader-macro sigil syntax with no \
             delimiter pair — MUST NOT project through seq_delims"
        );
    }

    // Contract with sibling [`NodeKind::as_seq_body`] — for every
    // variant, either both accessors return `Some` (the three D4-
    // dialect compound arms) or both return `None` (every other arm).
    // Pinned live rather than by inspection: a future addition of a
    // fourth compound arm (a hypothetical `NodeKind::Set(Vec<Node>)`
    // once the tatara-lisp reader grows a `#{…}` set literal) that
    // extended one accessor but not the other would silently produce
    // a walker-half-of-the-reader/writer-duality drift (as_seq_body
    // Some but seq_delims None → the emit dispatch's `.expect(…)` on
    // the writer half would panic at runtime for a body the walker
    // already treats as compound; the reverse would let the writer
    // emit delimiters around a body the walker refuses to descend
    // into). The `.expect(…)` calls at the caixa-fmt converge sites
    // are load-bearing on this partition-equivalence — this test
    // is what turns them from a runtime assertion into a build-time
    // pin.
    #[test]
    fn seq_delims_partitions_the_same_arm_set_as_as_seq_body() {
        for (variant, name) in all_variants() {
            assert_eq!(
                variant.seq_delims().is_some(),
                variant.as_seq_body().is_some(),
                "NodeKind::{name} — seq_delims().is_some() must equal \
                 as_seq_body().is_some() (reader/writer-duality partition \
                 equivalence); the caixa-fmt writer sites depend on this \
                 for their `expect(…)` on the delims half after gating \
                 through as_seq_body"
            );
        }
    }

    // Byte-parity pin on the pre-lift three-arm disjunctive
    // `match &_.kind { NodeKind::List(_) => Some(('(', ')')) | … | _ =>
    // None }` shape the four caixa-fmt writer sites route through
    // today via `.seq_delims()`. Refuses a future accidental split
    // between the accessor's return contract and its pre-lift
    // per-arm shape (a hand-rolled shadow `impl` that overrides one
    // path, an accidental rebrand of one converged call site back to
    // the raw `NodeKind::List(_) => Delims::PAREN | …` form, a per-
    // arm delimiter-pair flip) on the load-bearing writer-half-of-the-
    // reader/writer-duality axis every downstream compound-emit site
    // partitions on.
    #[test]
    fn seq_delims_byte_equal_pre_lift_pattern_match_shape() {
        for (variant, name) in all_variants() {
            let via_pattern: Option<(char, char)> = match &variant {
                NodeKind::List(_) => Some(('(', ')')),
                NodeKind::Map(_) => Some(('{', '}')),
                NodeKind::Vector(_) => Some(('[', ']')),
                _ => None,
            };
            let via_accessor = variant.seq_delims();
            assert_eq!(
                via_accessor, via_pattern,
                "NodeKind::{name}.seq_delims() must byte-equal \
                 `match &_ {{ NodeKind::List(_) => Some(('(', ')')) | \
                 NodeKind::Map(_) => Some(('{{', '}}')) | \
                 NodeKind::Vector(_) => Some(('[', ']')) | _ => None }}` \
                 — otherwise the four converged caixa-fmt writer sites \
                 would silently disagree with their pre-lift shape"
            );
        }
    }

    // Pin the const-eval surface: the substrate-primitive writer-half-of-
    // the-reader/writer-duality projection accessor reaches into `const`
    // context, so a future compile-time caixa-fmt writer-side delimiter-
    // pair-lookup truth-table / caixa-lint no-delimiter-in-non-compound-
    // context rule / caixa-lsp writer const-registry can key off
    // `NodeKind::seq_delims` without being forced onto the runtime code
    // path. The `const _: () = assert!(…)` bindings resolve the projection
    // at compile time on the non-compound arm-set (the compound arms
    // themselves carry a `Vec<Node>` payload with an `impl Drop` whose
    // const-drop stability is behind the same rust-lang/rust #143874
    // tracking issue the sibling `Span::union` open-codes `Ord::min` /
    // `Ord::max` around), pinning the four `Nil` / `Int` / `Bool` /
    // `Float` non-compound atom arms — each of which is
    // `const`-constructible in-place through its raw arm ctor
    // (`NodeKind::Nil` a unit ctor; `NodeKind::Int(i64)` / `Bool(bool)` /
    // `Float(f64)` all one-slot tuple-newtype ctors on `Copy` scalar
    // payloads) and each of which must fall through the accessor's
    // `_ => None` arm. Any regression that drops `pub const fn` back to
    // `pub fn` (a body edit that reaches for a non-const operation on
    // the accessor path) fails this test at compile time rather than at
    // runtime, matching the sibling
    // `reader_macro_prefix_is_const` pin on the paired reader-macro-arm-
    // set writer-half accessor and the caixa-ast source-position primitive
    // family's `Span::new` / `Span::point` / `Span::contains` /
    // `Span::union` / `Span::len` / `Span::is_empty` / `Position::new` /
    // `Position::origin` / `line_column` `pub const fn` shape's const-
    // eval discipline extended onto the caixa-ast [`NodeKind`] outer-
    // sum-type's per-arm identity-projection axis.
    #[test]
    fn seq_delims_is_const() {
        const NIL: NodeKind = NodeKind::Nil;
        const NIL_DELIMS: Option<(char, char)> = NIL.seq_delims();
        const _: () = assert!(NIL_DELIMS.is_none());

        const INT: NodeKind = NodeKind::Int(0);
        const INT_DELIMS: Option<(char, char)> = INT.seq_delims();
        const _: () = assert!(INT_DELIMS.is_none());

        const BOOL: NodeKind = NodeKind::Bool(false);
        const BOOL_DELIMS: Option<(char, char)> = BOOL.seq_delims();
        const _: () = assert!(BOOL_DELIMS.is_none());

        const FLOAT: NodeKind = NodeKind::Float(0.0);
        const FLOAT_DELIMS: Option<(char, char)> = FLOAT.seq_delims();
        const _: () = assert!(FLOAT_DELIMS.is_none());
    }

    // Projection contract on the outer-`NodeKind` sum-type's writer-side
    // reader-macro-sigil accessor: exactly the four reader-macro-carrying
    // arms return `Some(&'static str)` with the per-arm sigil bytes
    // (`Quote` → `"'"`, `Quasiquote` → `` "`" ``, `Unquote` → `","`,
    // `UnquoteSplice` → `",@"`); every other arm of the closed fourteen-
    // arm partition returns `None`. Pins the "one canonical projection
    // dispatch per typed arm-set on the substrate primitive" discipline
    // the two per-consumer writer sites in caixa-fmt/src/printer.rs route
    // through via `.reader_macro_prefix()`. A regression that flipped one
    // arm's sigil (a copy-paste that routed `Quote` through `` "`" `` and
    // `Quasiquote` through `"'"`, or `Unquote` through `",@"` and
    // `UnquoteSplice` through `","`) would silently rewrite every quote
    // as a quasiquote and every unquote-splice as an unquote at every
    // writer site.
    #[test]
    fn reader_macro_prefix_projects_only_reader_macro_arms_with_sigil_bytes() {
        let variants = all_variants();
        for (variant, name) in &variants {
            let projected = variant.reader_macro_prefix();
            let expected = match variant {
                NodeKind::Quote(_) => Some("'"),
                NodeKind::Quasiquote(_) => Some("`"),
                NodeKind::Unquote(_) => Some(","),
                NodeKind::UnquoteSplice(_) => Some(",@"),
                _ => None,
            };
            assert_eq!(
                projected, expected,
                "NodeKind::{name}.reader_macro_prefix() must project onto \
                 the per-arm reader-side sigil (Quote → \"'\", Quasiquote \
                 → \"`\", Unquote → \",\", UnquoteSplice → \",@\") — \
                 otherwise the two converged caixa-fmt writer sites would \
                 silently start emitting the wrong sigil"
            );
        }
        // Strict-reader-macro-arm-only boundary vs the sibling D4-dialect
        // compound-carrying arms — all three compound arms carry a
        // `Vec<Node>` body with matched delimiter pairs (`(…)`, `{…}`,
        // `[…]`), not a one- or two-byte sigil prefix, so a future
        // widening to admit a compound arm through `reader_macro_prefix`
        // (silently collapsing the semantic distinction between a
        // reader-macro wrapper `'x` and a container `(x)`) would break
        // every writer site that gates on the reader-macro-arm-set.
        // Pinned across all three compound arms — a per-arm-specific
        // regression would slip past a single-arm pin.
        assert_eq!(
            NodeKind::List(Vec::new()).reader_macro_prefix(),
            None,
            "List arm is a matched-delimiter compound container with no \
             one-byte reader-macro sigil prefix — MUST NOT project through \
             reader_macro_prefix"
        );
        assert_eq!(
            NodeKind::Map(Vec::new()).reader_macro_prefix(),
            None,
            "Map arm is a matched-delimiter compound container with no \
             one-byte reader-macro sigil prefix — MUST NOT project through \
             reader_macro_prefix"
        );
        assert_eq!(
            NodeKind::Vector(Vec::new()).reader_macro_prefix(),
            None,
            "Vector arm is a matched-delimiter compound container with no \
             one-byte reader-macro sigil prefix — MUST NOT project through \
             reader_macro_prefix"
        );
    }

    // Contract with sibling [`NodeKind::as_reader_macro_inner`] — for
    // every variant, either both accessors return `Some` (the four reader-
    // macro arms) or both return `None` (every other arm). Pinned live
    // rather than by inspection: a future addition of a fifth reader-macro
    // arm (a hypothetical `NodeKind::Splice(Box<Node>)` once the tatara-
    // lisp reader grows a splicing-quote variant, a `NodeKind::TaggedQuote
    // (Box<Node>, QuoteTag)` shape once the sexp→JSON bridge stabilizes
    // tagged-quote sugar) that extended one accessor but not the other
    // would silently produce a walker-half-of-the-reader/writer-duality
    // drift (as_reader_macro_inner Some but reader_macro_prefix None →
    // the emit dispatch's `.expect(…)` on the writer half would panic at
    // runtime for a wrapper the walker already treats as reader-macro;
    // the reverse would let the writer emit a sigil around a payload the
    // walker refuses to descend into). The `.expect(…)` calls at the
    // caixa-fmt converge sites are load-bearing on this partition-
    // equivalence — this test is what turns them from a runtime assertion
    // into a build-time pin. Sibling in shape to
    // `seq_delims_partitions_the_same_arm_set_as_as_seq_body` on the
    // peer D4-dialect compound axis.
    #[test]
    fn reader_macro_prefix_partitions_the_same_arm_set_as_as_reader_macro_inner() {
        for (variant, name) in all_variants() {
            assert_eq!(
                variant.reader_macro_prefix().is_some(),
                variant.as_reader_macro_inner().is_some(),
                "NodeKind::{name} — reader_macro_prefix().is_some() must \
                 equal as_reader_macro_inner().is_some() (reader/writer-\
                 duality partition equivalence on the reader-macro-arm-\
                 set); the caixa-fmt writer sites depend on this for their \
                 `expect(…)` on the prefix half after gating through \
                 as_reader_macro_inner"
            );
        }
    }

    // Byte-parity pin on the pre-lift four-arm disjunctive
    // `match &_.kind { NodeKind::Quote(_) => Some("'") | Quasiquote(_) =>
    // Some("`") | Unquote(_) => Some(",") | UnquoteSplice(_) => Some(",@")
    // | _ => None }` shape the two caixa-fmt writer sites route through
    // today via `.reader_macro_prefix()`. Refuses a future accidental
    // split between the accessor's return contract and its pre-lift
    // per-arm shape (a hand-rolled shadow `impl` that overrides one path,
    // an accidental rebrand of one converged call site back to the raw
    // four-arm `NodeKind::Quote(inner) => self.out.push('\'') | …` form,
    // a per-arm sigil flip) on the load-bearing writer-half-of-the-
    // reader/writer-duality axis every downstream reader-macro-emit site
    // partitions on.
    #[test]
    fn reader_macro_prefix_byte_equal_pre_lift_pattern_match_shape() {
        for (variant, name) in all_variants() {
            let via_pattern: Option<&'static str> = match &variant {
                NodeKind::Quote(_) => Some("'"),
                NodeKind::Quasiquote(_) => Some("`"),
                NodeKind::Unquote(_) => Some(","),
                NodeKind::UnquoteSplice(_) => Some(",@"),
                _ => None,
            };
            let via_accessor = variant.reader_macro_prefix();
            assert_eq!(
                via_accessor, via_pattern,
                "NodeKind::{name}.reader_macro_prefix() must byte-equal \
                 `match &_ {{ NodeKind::Quote(_) => Some(\"'\") | \
                 Quasiquote(_) => Some(\"`\") | Unquote(_) => Some(\",\") \
                 | UnquoteSplice(_) => Some(\",@\") | _ => None }}` — \
                 otherwise the two converged caixa-fmt writer sites would \
                 silently disagree with their pre-lift shape"
            );
        }
    }

    // Pin the const-eval surface: the substrate-primitive writer-half-of-
    // the-reader/writer-duality projection accessor reaches into `const`
    // context, so a future compile-time caixa-fmt writer-side sigil-lookup
    // truth-table / caixa-lint no-quote-sigil-in-non-reader-macro-context
    // rule / caixa-lsp writer const-registry can key off
    // `NodeKind::reader_macro_prefix` without being forced onto the runtime
    // code path. The `const _: () = assert!(…)` bindings resolve the
    // projection at compile time on the non-reader-macro arm-set (the
    // reader-macro arms themselves carry a `Box<Node>` payload which
    // `Box::new` cannot construct in `const` context yet — that
    // half of the const-eval surface unlocks on the same
    // rust-lang/rust #143874 tracking issue the sibling `Span::union`
    // open-codes `Ord::min` / `Ord::max` around), pinning the four `Nil`
    // / `Int` / `Bool` / `Float` non-reader-macro atom arms — each of
    // which is `const`-constructible in-place through its raw arm ctor
    // (`NodeKind::Nil` a unit ctor; `NodeKind::Int(i64)` / `Bool(bool)` /
    // `Float(f64)` all one-slot tuple-newtype ctors on `Copy` scalar
    // payloads) and each of which must fall through the accessor's
    // `_ => None` arm. Any regression that drops `pub const fn` back to
    // `pub fn` (a body edit that reaches for a non-const operation on
    // the accessor path) fails this test at compile time rather than at
    // runtime, matching the sibling caixa-ast source-position primitive
    // family's `Span::new` / `Span::point` / `Span::contains` /
    // `Span::union` / `Span::len` / `Span::is_empty` / `Position::new` /
    // `Position::origin` / `line_column` `pub const fn` shape's const-
    // eval discipline extended onto the caixa-ast [`NodeKind`] outer-
    // sum-type's per-arm identity-projection axis.
    #[test]
    fn reader_macro_prefix_is_const() {
        const NIL: NodeKind = NodeKind::Nil;
        const NIL_PREFIX: Option<&'static str> = NIL.reader_macro_prefix();
        const _: () = assert!(NIL_PREFIX.is_none());

        const INT: NodeKind = NodeKind::Int(0);
        const INT_PREFIX: Option<&'static str> = INT.reader_macro_prefix();
        const _: () = assert!(INT_PREFIX.is_none());

        const BOOL: NodeKind = NodeKind::Bool(false);
        const BOOL_PREFIX: Option<&'static str> = BOOL.reader_macro_prefix();
        const _: () = assert!(BOOL_PREFIX.is_none());

        const FLOAT: NodeKind = NodeKind::Float(0.0);
        const FLOAT_PREFIX: Option<&'static str> = FLOAT.reader_macro_prefix();
        const _: () = assert!(FLOAT_PREFIX.is_none());
    }

    // Pin the [`Node::to_tatara_sexp`] compound-arm-set converge onto the
    // lifted [`NodeKind::as_seq_body`] `Option<&[Node]>` accessor.
    //
    // Every one of the three D4-dialect compound arms (List, Map, Vector)
    // must lower to a `Sexp::List` whose body is the arm's own child
    // sequence lowered element-wise — the pre-lift shape had `List` and
    // `Map | Vector` as two match arms restating the identical
    // `Sexp::List(items.iter().map(Node::to_tatara_sexp).collect())`
    // body, and this pin refuses a future accidental split where one arm
    // silently changes shape (a per-arm `NodeKind::Map(items) =>
    // Sexp::List(items.iter().rev().map(…).collect())` reorder, a
    // `NodeKind::Vector(items) => Sexp::Nil` drop, a partial arm-set
    // widening back to the raw pattern-match that misses a future D4-
    // adjacent compound arm addition). Sibling in shape to the peer
    // `as_seq_body_projects_only_compound_arms` partition pin on the
    // walker half of the compound axis, extended onto the substrate-side
    // lowering-half converge in `Node::to_tatara_sexp`.
    #[test]
    fn to_tatara_sexp_lowers_every_compound_arm_to_sexp_list_with_identical_body() {
        use tatara_lisp::{Atom, Sexp};
        let expected_body: Vec<Sexp> = vec![
            Sexp::Atom(Atom::Symbol("a".into())),
            Sexp::Atom(Atom::Int(1)),
            Sexp::Atom(Atom::Keyword("k".into())),
        ];
        let child_nodes: Vec<Node> = vec![
            Node::new(NodeKind::Symbol("a".into()), Span::new(0, 0)),
            Node::new(NodeKind::Int(1), Span::new(0, 0)),
            Node::new(NodeKind::Keyword("k".into()), Span::new(0, 0)),
        ];
        for (ctor, name) in [
            (NodeKind::List as fn(Vec<Node>) -> NodeKind, "List"),
            (NodeKind::Map, "Map"),
            (NodeKind::Vector, "Vector"),
        ] {
            let node = Node::new(ctor(child_nodes.clone()), Span::new(0, 0));
            let lowered = node.to_tatara_sexp();
            match lowered {
                Sexp::List(body) => assert_eq!(
                    body, expected_body,
                    "NodeKind::{name} — to_tatara_sexp must lower to \
                     Sexp::List whose body is the element-wise lowering \
                     of the arm's own child sequence, byte-identically \
                     across all three D4-dialect compound arms"
                ),
                other => panic!(
                    "NodeKind::{name}.to_tatara_sexp() must lower to \
                     Sexp::List (Sexp has no Map/Vector variant yet — see \
                     theory/TATARA-LISP-CONSOLIDATION.md D4 Phase 2); \
                     got {other:?}"
                ),
            }
        }
        // Empty compound — the lowering is a projection, not a gate; an
        // empty-`()` list, empty-`{}` map, and empty-`[]` vector each
        // lower to `Sexp::List(vec![])`, not `Sexp::Nil`, because the
        // arm still carries a (zero-length) child sequence a lowerer is
        // entitled to iterate over. Pinned across all three compound
        // arms — a per-arm-specific regression would slip past a single-
        // arm pin.
        for (ctor, name) in [
            (NodeKind::List as fn(Vec<Node>) -> NodeKind, "List"),
            (NodeKind::Map, "Map"),
            (NodeKind::Vector, "Vector"),
        ] {
            let empty = Node::new(ctor(Vec::new()), Span::new(0, 0));
            let lowered = empty.to_tatara_sexp();
            match lowered {
                Sexp::List(body) => assert!(
                    body.is_empty(),
                    "empty NodeKind::{name} — to_tatara_sexp must lower \
                     to Sexp::List(vec![]), not Sexp::Nil or a \
                     stringified delimiter"
                ),
                other => panic!(
                    "empty NodeKind::{name}.to_tatara_sexp() must lower \
                     to Sexp::List(vec![]); got {other:?}"
                ),
            }
        }
    }

    // Byte-parity pin on the pre-lift four-arm compound-body shape
    // `match &self.kind { NodeKind::List(items) => Sexp::List(items.iter()
    // .map(Node::to_tatara_sexp).collect()) | NodeKind::Map(items) |
    // NodeKind::Vector(items) => Sexp::List(items.iter().map(Node::
    // to_tatara_sexp).collect()) | _ => (fall through to atoms) }` shape
    // the [`Node::to_tatara_sexp`] compound-body dispatch routed through
    // before the converge. Refuses a future accidental split between the
    // accessor-routed compound-body dispatch and its pre-lift per-arm
    // shape (a hand-rolled shadow `impl` that overrides one path, an
    // accidental rebrand of the converged dispatch back to the raw two-
    // arm pattern-match with a fourth new-D4-arm silently missed, a per-
    // arm body-map reorder that would silently corrupt only one of the
    // three arms). Sibling in shape to the peer
    // `seq_delims_byte_equal_pre_lift_pattern_match_shape` /
    // `as_seq_body_byte_equal_pre_lift_pattern_match_shape` pins on the
    // sibling writer + walker halves of the compound axis.
    #[test]
    #[allow(
        clippy::match_same_arms,
        reason = "the two same-body compound arms ARE the pre-lift shape \
                  this pin reproduces; collapsing them would erase what \
                  the pin documents"
    )]
    fn to_tatara_sexp_compound_arm_body_byte_equal_pre_lift_pattern_match_shape() {
        use tatara_lisp::Sexp;
        let child_nodes: Vec<Node> = vec![
            Node::new(NodeKind::Nil, Span::new(0, 0)),
            Node::new(NodeKind::Symbol("h".into()), Span::new(0, 0)),
            Node::new(NodeKind::Str("s".into()), Span::new(0, 0)),
            Node::new(NodeKind::Float(1.5), Span::new(0, 0)),
            Node::new(NodeKind::Bool(true), Span::new(0, 0)),
        ];
        for (ctor, name) in [
            (NodeKind::List as fn(Vec<Node>) -> NodeKind, "List"),
            (NodeKind::Map, "Map"),
            (NodeKind::Vector, "Vector"),
        ] {
            let node = Node::new(ctor(child_nodes.clone()), Span::new(0, 0));
            let via_accessor = node.to_tatara_sexp();
            // Reconstruct the pre-lift per-arm shape byte-for-byte from the
            // arm's own child sequence, independent of the accessor path
            // — a hand-rolled shadow that reproduces the pre-lift dispatch.
            let via_pattern = match &node.kind {
                NodeKind::List(items) => {
                    Sexp::List(items.iter().map(Node::to_tatara_sexp).collect())
                }
                NodeKind::Map(items) | NodeKind::Vector(items) => {
                    Sexp::List(items.iter().map(Node::to_tatara_sexp).collect())
                }
                _ => unreachable!("guarded by the ctor loop above"),
            };
            let via_pattern_body = match via_pattern {
                Sexp::List(b) => b,
                other => panic!("pre-lift shape must produce Sexp::List; got {other:?}"),
            };
            let via_accessor_body = match via_accessor {
                Sexp::List(b) => b,
                other => panic!(
                    "NodeKind::{name}.to_tatara_sexp() — accessor-routed \
                     dispatch must produce Sexp::List; got {other:?}"
                ),
            };
            assert_eq!(
                via_accessor_body, via_pattern_body,
                "NodeKind::{name}.to_tatara_sexp() body must byte-equal \
                 the pre-lift per-arm `Sexp::List(items.iter().map(Node::\
                 to_tatara_sexp).collect())` shape — otherwise the \
                 converged dispatch would silently disagree with the \
                 pre-lift shape at exactly this compound arm"
            );
            // A grounding sanity check: the body carries five children
            // (one per child_nodes element), each of which lowered to a
            // Sexp::Atom (the atom arms) — a per-child drop or duplicate
            // would slip past a pure body-vs-body equality if BOTH the
            // accessor path and the pre-lift shadow shared the same bug.
            assert_eq!(
                via_accessor_body.len(),
                child_nodes.len(),
                "NodeKind::{name}.to_tatara_sexp() must preserve the \
                 arm's child-count exactly"
            );
            for (i, s) in via_accessor_body.iter().enumerate() {
                assert!(
                    matches!(s, Sexp::Atom(_) | Sexp::Nil),
                    "NodeKind::{name}.to_tatara_sexp() body[{i}] — every \
                     child in this fixture is an atom or nil arm; a \
                     compound projection here would signal a lowerer \
                     drift"
                );
            }
        }
    }
}
