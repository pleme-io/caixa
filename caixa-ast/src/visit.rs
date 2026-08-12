//! Visitor — depth-first walk over a [`crate::Node`] tree.

use crate::node::{Node, NodeKind};

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
    match &node.kind {
        // Map and Vector are compound and MUST recurse. The `_` arm
        // below is a silent trap for new compound variants: it makes a
        // missing recursion compile cleanly and simply not visit the
        // children, so every lint that walks the tree would quietly stop
        // seeing anything nested inside `{ … }` or `[ … ]`.
        NodeKind::List(items) | NodeKind::Map(items) | NodeKind::Vector(items) => {
            for item in items {
                v.visit_node(item);
            }
        }
        _ => {}
    }
}
