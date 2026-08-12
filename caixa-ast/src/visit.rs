//! Visitor — depth-first walk over a [`crate::Node`] tree.

use crate::node::Node;

/// Visitor trait — override the methods you care about, defaults recurse.
pub trait Visitor {
    fn visit_node(&mut self, node: &Node) {
        walk(self, node);
    }
}

pub fn walk<V: Visitor + ?Sized>(v: &mut V, node: &Node) {
    // Route the reader-macro-arm-set recursion through the lifted
    // [`NodeKind::as_reader_macro_inner`] `Option<&Node>` accessor
    // rather than the raw four-arm `NodeKind::Quote(inner) |
    // NodeKind::Quasiquote(inner) | NodeKind::Unquote(inner) |
    // NodeKind::UnquoteSplice(inner) => v.visit_node(inner)` open-coded
    // per-arm disjunctive pattern-match — sibling in shape to the peer
    // `caixa-fmt::contains_comment`, `caixa-teia::node_to_value`, and
    // `caixa-lint::walk` reader-macro sites (all converged in this run)
    // that all key off the four-arm reader-macro-carrying arm-set on
    // the outer-`NodeKind` sum-type.
    if let Some(inner) = node.kind.as_reader_macro_inner() {
        v.visit_node(inner);
        return;
    }
    // Route the D4-dialect compound-body recursion through the lifted
    // [`crate::NodeKind::as_seq_body`] `Option<&[Node]>` accessor
    // rather than the raw three-arm `NodeKind::List(items) |
    // NodeKind::Map(items) | NodeKind::Vector(items) => …` open-coded
    // per-arm disjunctive pattern-match. The prior open-coded shape
    // relied on a silent `_ => {}` trap for a future compound variant
    // — a new D4-adjacent compound arm (a hypothetical
    // `NodeKind::Set(Vec<Node>)`, a `NodeKind::Tuple(Vec<Node>)`) would
    // compile cleanly and silently stop being visited; the lifted
    // accessor centralises the compound-arm-set at the substrate
    // primitive, so extending it there reaches every walker by
    // construction.
    if let Some(items) = node.kind.as_seq_body() {
        for item in items {
            v.visit_node(item);
        }
    }
}
