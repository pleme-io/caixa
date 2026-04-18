//! `caixa-lint` — linter for tatara-lisp / caixa sources.
//!
//! The rulebook encodes our distilled **Ruby + Rust best practices** for a
//! homoiconic package language:
//!
//! **From Ruby (principle of least surprise, convention over configuration):**
//!   - Single consistent naming: `:kebab-case` keywords, `PascalCase` enum
//!     discriminants, `snake_case` Rust-backed fields.
//!   - Descriptive names, descriptive defs — every top-level form documents
//!     itself via `:descricao`.
//!   - Small forms (analog of "small methods"): warn > 50 lines.
//!   - No placeholder text (`"FIXME"` literals never reach review).
//!
//! **From Rust (explicit at boundaries, deterministic, enforced by tooling):**
//!   - Deps pin a `:tag` or `:rev` — branches drift, tags don't.
//!   - Enum discriminants are bare symbols (not quoted strings) — catches
//!     typos at parse time.
//!   - Paired keyword args (no dangling `:key`).
//!   - Consistent quote style — never mix `'x` with `(quote x)` in one file.
//!   - Lacre coherence — if deps changed, the lock must be refreshed.
//!
//! **From Lisp (homoiconicity — the tool speaks the same language):**
//!   - Rules are themselves authorable as Lisp forms (phase 2; for now, Rust
//!     functions; the Rule trait shape is compatible with Lisp authoring via
//!     a future `#[derive(TataraDomain)]`).
//!
//! Each rule has an ID (stable, dash-separated), severity, description, and
//! a check fn. Rules are opt-out via config; defaults are all-on at the
//! severity they declare.

pub mod diagnostic;
pub mod lisp_config;
pub mod rule;
pub mod rules;
pub mod runner;

pub use diagnostic::{Diagnostic, Severity};
pub use lisp_config::{CustomRule, LintConfigLisp, RuleOverride};
pub use rule::{Rule, RuleCheck};
pub use rules::all_rules;
pub use runner::{LintError, lint_nodes, lint_source};
