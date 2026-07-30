//! `caixa-fmt` — canonical pretty-printer for tatara-lisp / caixa sources.
//!
//! Goals (in priority order):
//!   1. **Deterministic** — same input, same output, on every platform.
//!   2. **Round-trippable** — `parse(fmt(src)) == parse(src)`. The formatter
//!      changes whitespace but never changes meaning.
//!   3. **Opinionated** — no options for quote style or indentation. Like
//!      `rustfmt`, there is exactly one way for a caixa to look.
//!   4. **Comment-preserving** — leading line-comments and blank lines carry
//!      over from the input via [`caixa_ast::Trivia`].
//!
//! Layout rules:
//!   - Top-level forms separated by one blank line.
//!   - List fits inline if its inline rendering ≤ `line_width` columns.
//!     Else, break it up.
//!   - Kwargs list (head symbol followed by `:k v :k v …`): emit `:k v`
//!     pairs one per line, keywords aligned to the first column after the
//!     head symbol.
//!   - Generic list (non-kwargs): first element on the opening line, rest
//!     indented by `indent` (default 2).

//! ## Formatted-ness is a parse-time property
//!
//! [`canonical::parse_canonical`] is the gate: it parses, re-renders,
//! compares bytes, and refuses to return an AST when they differ. Wired
//! into the reader an evaluator uses, a non-canonical `.tlisp` does not
//! run. See that module for the tier this actually holds
//! (parse-rejected, not truly-unrepresentable) and for the ordering
//! constraint on switching it on.
//!
//! The layout space itself is a closed set — `printer::FormShape` — so
//! the dispatch is an exhaustive match and a new syntax form cannot
//! silently inherit another's layout.

pub mod canonical;
pub mod config;
pub mod lisp_config;
pub mod printer;

pub use canonical::{CanonicalError, is_canonical, parse_canonical};
pub use config::FmtConfig;
pub use lisp_config::FmtConfigLisp;
pub use printer::{FmtError, format_nodes, format_source};
