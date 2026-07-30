//! Visitor — depth-first walk over a [`crate::Node`] tree.

use crate::node::{Node, NodeKind};

/// Visitor trait — override the methods you care about, defaults recurse.
pub trait Visitor {
    fn visit_node(&mut self, node: &Node) {
        walk(self, node);
    }
}

pub fn walk<V: Visitor + ?Sized>(v: &mut V, node: &Node) {
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
        NodeKind::Quote(inner)
        | NodeKind::Quasiquote(inner)
        | NodeKind::Unquote(inner)
        | NodeKind::UnquoteSplice(inner) => {
            v.visit_node(inner);
        }
        _ => {}
    }
}
