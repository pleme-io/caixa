use caixa_ast::{ParseError, parse};
use thiserror::Error;

use crate::diagnostic::{Diagnostic, FixSafety};
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

/// Outcome of running [`apply_fixes`] against a source string.
#[derive(Debug, Clone)]
pub struct FixResult {
    /// Source after applying every selected fix.
    pub source: String,
    /// How many edits were applied.
    pub applied: usize,
    /// Diagnostics that had a fix but were skipped (overlapping span,
    /// or filtered out by safety).
    pub skipped: usize,
}

/// Apply every fix in `diags` whose `safety <= max_safety`. Edits
/// are sorted by `span.start` descending and applied in that order
/// so earlier offsets stay stable. Overlapping edits — where a later
/// edit would apply inside the span of one already applied — are
/// skipped (the linter is expected to lint again after a fix pass
/// to catch downstream consequences).
///
/// `max_safety = FixSafety::Safe` applies only mechanically-safe
/// fixes. `Unsafe` applies all.
#[must_use]
pub fn apply_fixes(src: &str, diags: &[Diagnostic], max_safety: FixSafety) -> FixResult {
    // Collect (span, replacement) pairs, filtered by safety. Span
    // fields are u32 in caixa-ast; widen to usize for slice indexing.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut skipped = 0usize;
    for d in diags {
        let Some(fix) = &d.fix else { continue };
        match (max_safety, fix.safety) {
            (FixSafety::Safe, FixSafety::Unsafe) => {
                skipped += 1;
                continue;
            }
            _ => {}
        }
        for edit in &fix.edits {
            edits.push((
                edit.span.start as usize,
                edit.span.end as usize,
                edit.replacement.clone(),
            ));
        }
    }
    // Apply in reverse order of start offset so earlier offsets stay
    // valid. On equal start, larger end first (covers wider spans).
    edits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

    let mut out = src.to_string();
    let mut applied = 0usize;
    let mut last_start = usize::MAX; // start of most recently applied edit
    for (start, end, replacement) in edits {
        // Skip overlapping edits (where this one's end exceeds the
        // start of the previously applied — they'd be in conflict).
        if end > last_start {
            skipped += 1;
            continue;
        }
        if end > out.len() {
            skipped += 1;
            continue;
        }
        out.replace_range(start..end, &replacement);
        last_start = start;
        applied += 1;
    }
    FixResult {
        source: out,
        applied,
        skipped,
    }
}
