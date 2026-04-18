//! The small semantic-style enum every caixa tool agrees on.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Semantic {
    /// Language keywords — `defcaixa`, `defteia`, `let`, `lambda`, etc.
    Keyword,
    /// Non-keyword symbols — identifiers, function names, variant names.
    Symbol,
    /// `:keyword-positioned` atoms.
    KeywordArg,
    /// `"string literals"`.
    String,
    /// `42`, `3.14`.
    Number,
    /// `#t`, `#f`, `nil`.
    Literal,
    /// `; comments`.
    Comment,
    /// Primary accent — useful for highlights, carets, focused tokens.
    Accent,
    /// Dim text — metadata, line numbers, help text.
    Muted,

    // Diagnostic severities.
    Error,
    Warning,
    Info,
    Hint,

    // Diff decorations — used by formatter preview and lint output.
    Added,
    Removed,
    Unchanged,
}
