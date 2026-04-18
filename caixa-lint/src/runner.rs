use caixa_ast::{ParseError, parse};
use thiserror::Error;

use crate::diagnostic::Diagnostic;
use crate::rule::Rule;
use crate::rules::all_rules;

#[derive(Debug, Error)]
pub enum LintError {
    #[error("parse: {0}")]
    Parse(#[from] ParseError),
}

/// Convenience: parse source, run the default rulebook, return diagnostics.
pub fn lint_source(src: &str) -> Result<Vec<Diagnostic>, LintError> {
    let nodes = parse(src)?;
    Ok(lint_nodes(&nodes, &all_rules()))
}

/// Run a specific rule set over already-parsed nodes.
#[must_use]
pub fn lint_nodes(nodes: &[caixa_ast::Node], rules: &[Rule]) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for node in nodes {
        for rule in rules {
            (rule.check)(node, &mut diags);
        }
    }
    diags
}
