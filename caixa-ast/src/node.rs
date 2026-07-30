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

#[derive(Debug, Clone, PartialEq)]
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
        let start = if items
            .first()
            .is_some_and(|n| matches!(n.kind, NodeKind::Symbol(_)))
        {
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
