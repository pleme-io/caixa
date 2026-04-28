use caixa_ast::Span;
use caixa_theme::{Semantic, Theme};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl Severity {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Hint => "hint",
        }
    }

    #[must_use]
    pub const fn as_semantic(self) -> Semantic {
        match self {
            Self::Error => Semantic::Error,
            Self::Warning => Semantic::Warning,
            Self::Info => Semantic::Info,
            Self::Hint => Semantic::Hint,
        }
    }
}

/// A textual edit — replace `span` with `replacement` in the source.
/// Edits never overlap; the autofix driver sorts them by `span.start`
/// descending and applies in reverse order so earlier offsets stay
/// stable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edit {
    pub span: Span,
    pub replacement: String,
}

/// Auto-applicable correction for a [`Diagnostic`]. A single fix may
/// involve multiple edits (e.g. rename a name + every reference);
/// all edits apply atomically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fix {
    pub description: String,
    pub edits: Vec<Edit>,
    pub safety: FixSafety,
}

/// How safe is a fix to apply automatically?
///
/// * `Safe` — mechanical, semantics-preserving for pure round-trips.
///   `feira lint --fix` applies these by default.
/// * `Unsafe` — heuristic; may change runtime behavior in edge cases.
///   Requires explicit `--fix-unsafe` opt-in.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FixSafety {
    Safe,
    Unsafe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub rule_id: &'static str,
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    pub hint: Option<String>,
    /// Optional autofix. When present, `feira lint --fix` applies it
    /// (subject to the requested safety threshold).
    pub fix: Option<Fix>,
}

impl Diagnostic {
    #[must_use]
    pub fn new(
        rule_id: &'static str,
        severity: Severity,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        Self {
            rule_id,
            severity,
            span,
            message: message.into(),
            hint: None,
            fix: None,
        }
    }

    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Attach an auto-applicable correction.
    #[must_use]
    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }

    /// Convenience: attach a single-edit safe fix that replaces this
    /// diagnostic's own span with `replacement`.
    #[must_use]
    pub fn with_fix_replace(self, description: impl Into<String>, replacement: impl Into<String>) -> Self {
        let span = self.span;
        self.with_fix(Fix {
            description: description.into(),
            edits: vec![Edit {
                span,
                replacement: replacement.into(),
            }],
            safety: FixSafety::Safe,
        })
    }

    /// Render this diagnostic against a source string, Nord-themed.
    #[must_use]
    pub fn render(&self, src: &str, theme: &Theme) -> String {
        let pos = caixa_ast::line_column(src, self.span.start);
        let sev = theme.paint(self.severity.as_semantic(), self.severity.as_str());
        let id = theme.paint(Semantic::Muted, &format!("[{}]", self.rule_id));
        let at = theme.paint(Semantic::Muted, &format!("{pos}"));
        let mut out = format!("{sev} {id} {at}: {}", self.message);
        if let Some(h) = &self.hint {
            out.push('\n');
            out.push_str("  ");
            out.push_str(&theme.paint(Semantic::Hint, "hint"));
            out.push_str(": ");
            out.push_str(h);
        }
        out
    }
}
