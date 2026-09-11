//! `caixa-theme` — colors + semantic styles shared across the caixa
//! ecosystem.
//!
//! Three tiers:
//!   - [`palette`] — Nord (the canonical pleme-io palette), exposed as
//!     `#RRGGBB` hex strings + 24-bit tuples.
//!   - [`Semantic`] — the small set of named styles every tool maps to
//!     (Keyword, Symbol, String, Comment, Error, Warning, Info, Hint,
//!     Added, Removed, Caret, Muted).
//!   - [`blackmatter`] — overlays that pick which Nord color a given
//!     Semantic renders as. The default overlay is `blackmatter_dark`,
//!     matching blackmatter-nvim + blackmatter-shell.
//!
//! Consumers (caixa-fmt, caixa-lint, caixa-lsp, caixa.nvim) pick a
//! [`Theme`] at startup and ask `theme.style(Semantic::Error)` to get a
//! concrete RGB triple.

pub mod blackmatter;
pub mod palette;
pub mod style;

pub use blackmatter::Theme;
pub use palette::{Nord, Rgb};
pub use style::{
    CAIXA_THEME_SEMANTIC_WIRE_ACCENT, CAIXA_THEME_SEMANTIC_WIRE_ADDED,
    CAIXA_THEME_SEMANTIC_WIRE_COMMENT, CAIXA_THEME_SEMANTIC_WIRE_ERROR,
    CAIXA_THEME_SEMANTIC_WIRE_HINT, CAIXA_THEME_SEMANTIC_WIRE_INFO,
    CAIXA_THEME_SEMANTIC_WIRE_KEYWORD, CAIXA_THEME_SEMANTIC_WIRE_KEYWORD_ARG,
    CAIXA_THEME_SEMANTIC_WIRE_LITERAL, CAIXA_THEME_SEMANTIC_WIRE_MUTED,
    CAIXA_THEME_SEMANTIC_WIRE_NUMBER, CAIXA_THEME_SEMANTIC_WIRE_REMOVED,
    CAIXA_THEME_SEMANTIC_WIRE_STRING, CAIXA_THEME_SEMANTIC_WIRE_SYMBOL,
    CAIXA_THEME_SEMANTIC_WIRE_UNCHANGED, CAIXA_THEME_SEMANTIC_WIRE_WARNING, Semantic,
};
