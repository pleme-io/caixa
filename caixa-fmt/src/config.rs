use serde::{Deserialize, Serialize};

/// Formatter configuration. Sensible defaults; surface minimal knobs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FmtConfig {
    /// Target line width in columns. A HARD ceiling, not a preference:
    /// `line_width` is the only quantity the group-break rule consults, so
    /// the same tree at the same width always yields the same bytes.
    pub line_width: usize,
    /// Indent step in spaces.
    pub indent: usize,
    /// End every file with exactly one newline.
    pub trailing_newline: bool,
    /// Preserve leading line-comments and blank lines.
    pub preserve_comments: bool,
    /// Maximum *slots* an s-expression may hold and still render flat,
    /// where a `:key value` kwarg pair counts as ONE slot.
    ///
    /// Width alone lets a wide-but-short form sprawl sideways — a 9-element
    /// list fitting in 78 columns is legal under a pure width rule, and
    /// reads as a wall. Vertical stacking is the house style: an
    /// s-expression goes DOWN, indented, rather than ACROSS. This bounds
    /// flatness by arity as well as by width, so a form stays inline only
    /// when it is genuinely small in both senses.
    ///
    /// Counting PAIRS rather than raw items is the load-bearing detail:
    /// `(:x 1 :y 2)` is four nodes but two ideas, and stacking it reads
    /// worse than leaving it alone. A raw-item bound would break exactly
    /// the small kwarg forms this style is meant to keep legible.
    ///
    /// Deterministic by construction — a fixed count, never a heuristic.
    pub max_inline_items: usize,
}

impl Default for FmtConfig {
    fn default() -> Self {
        Self {
            // 80, the operator's standing rule. Not 100: the wider budget is
            // what let deeply-nested forms stay flat and drift right.
            line_width: 80,
            indent: 2,
            trailing_newline: true,
            preserve_comments: true,
            // A head plus up to two operands (e.g. `(+ 1 2)`, `(:key val)`)
            // stays on one line; anything larger stacks. Chosen so kwarg
            // pairs and small applications stay readable while every
            // multi-field form gets the vertical shape.
            max_inline_items: 3,
        }
    }
}
