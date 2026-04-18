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
        NodeKind::List(items) => {
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
