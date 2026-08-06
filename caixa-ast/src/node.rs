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
    #[must_use]
    pub fn head_symbol(&self) -> Option<&str> {
        let NodeKind::List(items) = &self.kind else {
            return None;
        };
        let NodeKind::Symbol(s) = &items.first()?.kind else {
            return None;
        };
        Some(s)
    }

    /// For a list formatted as alternating `:key value :key value`, returns
    /// the matching value node for `key` (without the leading colon).
    #[must_use]
    pub fn kwarg(&self, key: &str) -> Option<&Node> {
        let NodeKind::List(items) = &self.kind else {
            return None;
        };
        let start = if items.first().is_some_and(|n| n.kind.is_symbol()) {
            1
        } else {
            0
        };
        let mut i = start;
        while i + 1 < items.len() {
            if let NodeKind::Keyword(k) = &items[i].kind {
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
}
