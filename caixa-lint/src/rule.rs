use caixa_ast::Node;

use crate::diagnostic::{Diagnostic, Severity};

/// A pure check function. No I/O; no mutable state outside the diagnostics
/// sink. Given a top-level `Node`, the function reports any violations it
/// finds into `diags` and returns. Rules may walk the subtree internally.
pub type RuleCheck = fn(&Node, &mut Vec<Diagnostic>);

#[derive(Debug, Clone, Copy)]
pub struct Rule {
    pub id: &'static str,
    pub description: &'static str,
    pub severity: Severity,
    pub check: RuleCheck,
}

impl Rule {
    #[must_use]
    pub const fn new(
        id: &'static str,
        description: &'static str,
        severity: Severity,
        check: RuleCheck,
    ) -> Self {
        Self {
            id,
            description,
            severity,
            check,
        }
    }
}
