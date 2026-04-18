use serde::{Deserialize, Serialize};

/// Formatter configuration. Sensible defaults; surface minimal knobs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FmtConfig {
    /// Target line width in columns.
    pub line_width: usize,
    /// Indent step in spaces.
    pub indent: usize,
    /// End every file with exactly one newline.
    pub trailing_newline: bool,
    /// Preserve leading line-comments and blank lines.
    pub preserve_comments: bool,
}

impl Default for FmtConfig {
    fn default() -> Self {
        Self {
            line_width: 100,
            indent: 2,
            trailing_newline: true,
            preserve_comments: true,
        }
    }
}
