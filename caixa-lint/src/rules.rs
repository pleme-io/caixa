//! The canonical rulebook. Each rule encodes one distilled best practice —
//! see crate-level docs for the Ruby + Rust lineage.
//!
//! Adding a new rule: define a `fn rule_x(node: &Node, diags: &mut Vec<Diagnostic>)`
//! and append a [`Rule::new`] entry in [`all_rules`].

use caixa_ast::{Node, NodeKind};

use crate::diagnostic::{Diagnostic, Severity};
use crate::rule::Rule;

/// Every built-in rule, all enabled at default severity.
#[must_use]
pub fn all_rules() -> Vec<Rule> {
    vec![
        Rule::new(
            "keyword-kebab-case",
            "keywords should be kebab-case — matches the tatara-lisp sexp→JSON bridge convention",
            Severity::Warning,
            check_keyword_kebab,
        ),
        Rule::new(
            "enum-variant-pascal-case",
            "enum variants are bare PascalCase symbols, never quoted strings",
            Severity::Warning,
            check_enum_pascal,
        ),
        Rule::new(
            "caixa-nome-kebab-case",
            ":nome should be kebab-case to match Rust crate + Git repo conventions",
            Severity::Warning,
            check_nome_kebab,
        ),
        Rule::new(
            "paired-kwargs",
            "keyword args must come in pairs (:key value)",
            Severity::Error,
            check_paired_kwargs,
        ),
        Rule::new(
            "defcaixa-descricao",
            "a defcaixa should carry :descricao — readers expect self-documenting manifests",
            Severity::Info,
            check_defcaixa_descricao,
        ),
        Rule::new(
            "no-fixme-descricao",
            ":descricao must not be a FIXME placeholder in committed code",
            Severity::Error,
            check_no_fixme,
        ),
        Rule::new(
            "git-dep-needs-pin",
            "git deps should pin :tag or :rev — :branch alone drifts",
            Severity::Warning,
            check_git_pin,
        ),
        Rule::new(
            "small-forms",
            "top-level forms should be small (≤ 60 lines) — Ruby's \"small method\" principle",
            Severity::Info,
            check_small_forms,
        ),
        Rule::new(
            "consistent-quote-style",
            "do not mix 'x and (quote x) within a single file",
            Severity::Warning,
            check_consistent_quote,
        ),
        Rule::new(
            "explicit-kind",
            "a defcaixa must set :kind explicitly",
            Severity::Error,
            check_explicit_kind,
        ),
    ]
}

// ─────────────────────────────────────────────────────────────────────
// Naming rules
// ─────────────────────────────────────────────────────────────────────

fn check_keyword_kebab(node: &Node, diags: &mut Vec<Diagnostic>) {
    walk(node, &mut |n| {
        if let NodeKind::Keyword(k) = &n.kind {
            if !is_kebab(k) {
                diags.push(
                    Diagnostic::new(
                        "keyword-kebab-case",
                        Severity::Warning,
                        n.span,
                        format!(":{k} should be kebab-case"),
                    )
                    .with_hint(format!("rename to :{}", to_kebab(k))),
                );
            }
        }
    });
}

fn check_enum_pascal(node: &Node, diags: &mut Vec<Diagnostic>) {
    // Target-fields that conventionally take enum variants.
    const ENUM_KEYS: &[&str] = &["kind", "severity", "tipo", "horizon", "coordination"];
    let NodeKind::List(items) = &node.kind else {
        return;
    };
    let start = usize::from(matches!(
        items.first().map(|n| &n.kind),
        Some(NodeKind::Symbol(_))
    ));
    let mut i = start;
    while i + 1 < items.len() {
        if let NodeKind::Keyword(k) = &items[i].kind {
            if ENUM_KEYS.contains(&k.as_str()) {
                let v = &items[i + 1];
                match &v.kind {
                    NodeKind::Str(s) => {
                        diags.push(
                            Diagnostic::new(
                                "enum-variant-pascal-case",
                                Severity::Warning,
                                v.span,
                                format!(":{k} expects a bare symbol, not a quoted string"),
                            )
                            .with_hint(format!("write `{}` without quotes", s)),
                        );
                    }
                    NodeKind::Symbol(s) if !is_pascal(s) => {
                        diags.push(
                            Diagnostic::new(
                                "enum-variant-pascal-case",
                                Severity::Warning,
                                v.span,
                                format!(":{k} symbol '{s}' should be PascalCase"),
                            )
                            .with_hint(format!("rename to {}", to_pascal(s))),
                        );
                    }
                    _ => {}
                }
            }
        }
        i += 2;
    }
}

fn check_nome_kebab(node: &Node, diags: &mut Vec<Diagnostic>) {
    let Some(v) = node.kwarg("nome") else { return };
    if let NodeKind::Str(s) = &v.kind {
        if !is_kebab(s) {
            diags.push(
                Diagnostic::new(
                    "caixa-nome-kebab-case",
                    Severity::Warning,
                    v.span,
                    format!(":nome {s:?} should be kebab-case"),
                )
                .with_hint(format!("rename to {:?}", to_kebab(s))),
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Kwargs integrity
// ─────────────────────────────────────────────────────────────────────

fn check_paired_kwargs(node: &Node, diags: &mut Vec<Diagnostic>) {
    walk(node, &mut |n| {
        let NodeKind::List(items) = &n.kind else {
            return;
        };
        // Either the whole list is kwargs, or it starts with a head symbol.
        let start = usize::from(matches!(
            items.first().map(|n| &n.kind),
            Some(NodeKind::Symbol(_))
        ));
        let rest = items.len().saturating_sub(start);
        // Only flag lists that clearly look kwargs-y (first non-head is keyword).
        let looks_kwargs = items
            .get(start)
            .is_some_and(|n| matches!(n.kind, NodeKind::Keyword(_)));
        if looks_kwargs && rest % 2 != 0 {
            let last = items.last().map_or(n.span, |it| it.span);
            diags.push(Diagnostic::new(
                "paired-kwargs",
                Severity::Error,
                last,
                "dangling keyword — kwargs must be pairs of (:key value)",
            ));
        }
    });
}

// ─────────────────────────────────────────────────────────────────────
// Manifest quality rules
// ─────────────────────────────────────────────────────────────────────

fn check_defcaixa_descricao(node: &Node, diags: &mut Vec<Diagnostic>) {
    if node.head_symbol() != Some("defcaixa") {
        return;
    }
    if node.kwarg("descricao").is_none() {
        diags.push(Diagnostic::new(
            "defcaixa-descricao",
            Severity::Info,
            node.span,
            "defcaixa should carry :descricao for registry listings",
        ));
    }
}

fn check_no_fixme(node: &Node, diags: &mut Vec<Diagnostic>) {
    if node.head_symbol() != Some("defcaixa") {
        return;
    }
    let Some(d) = node.kwarg("descricao") else {
        return;
    };
    if let NodeKind::Str(s) = &d.kind {
        if s.contains("FIXME") {
            diags.push(Diagnostic::new(
                "no-fixme-descricao",
                Severity::Error,
                d.span,
                ":descricao still contains FIXME",
            ));
        }
    }
}

fn check_explicit_kind(node: &Node, diags: &mut Vec<Diagnostic>) {
    if node.head_symbol() != Some("defcaixa") {
        return;
    }
    if node.kwarg("kind").is_none() {
        diags.push(Diagnostic::new(
            "explicit-kind",
            Severity::Error,
            node.span,
            "defcaixa must set :kind explicitly (Biblioteca | Binario | Servico)",
        ));
    }
}

fn check_git_pin(node: &Node, diags: &mut Vec<Diagnostic>) {
    // Walk every `(:tipo git …)` source and ensure :tag or :rev is set.
    walk(node, &mut |n| {
        let NodeKind::List(items) = &n.kind else {
            return;
        };
        // A :fonte value list looks like (:tipo git :repo "…" :tag "…" …).
        let has_tipo_git = matches_kwarg(
            items,
            "tipo",
            |v| matches!(&v.kind, NodeKind::Symbol(s) if s == "git"),
        );
        if !has_tipo_git {
            return;
        }
        let has_tag = items_has_key(items, "tag");
        let has_rev = items_has_key(items, "rev");
        if !has_tag && !has_rev {
            diags.push(
                Diagnostic::new(
                    "git-dep-needs-pin",
                    Severity::Warning,
                    n.span,
                    "git source should pin :tag or :rev",
                )
                .with_hint("add :tag \"v1.2.3\" or :rev \"abcdef…\""),
            );
        }
    });
}

fn check_small_forms(node: &Node, diags: &mut Vec<Diagnostic>) {
    // Only flag top-level def* forms that span > 60 source lines.
    if let Some(head) = node.head_symbol() {
        if head.starts_with("def") {
            let lines = span_lines(node);
            if lines > 60 {
                diags.push(Diagnostic::new(
                    "small-forms",
                    Severity::Info,
                    node.span,
                    format!("'{head}' form is {lines} lines — consider splitting"),
                ));
            }
        }
    }
}

fn check_consistent_quote(node: &Node, diags: &mut Vec<Diagnostic>) {
    let mut saw_reader_quote = false;
    let mut saw_quote_form = false;
    let mut first_offender_span = None;
    walk(node, &mut |n| match &n.kind {
        NodeKind::Quote(_) => saw_reader_quote = true,
        NodeKind::List(items) if matches!(items.first().map(|n| &n.kind), Some(NodeKind::Symbol(s)) if s == "quote") =>
        {
            saw_quote_form = true;
            if first_offender_span.is_none() {
                first_offender_span = Some(n.span);
            }
        }
        _ => {}
    });
    if saw_reader_quote && saw_quote_form {
        if let Some(s) = first_offender_span {
            diags.push(
                Diagnostic::new(
                    "consistent-quote-style",
                    Severity::Warning,
                    s,
                    "file mixes 'x reader quote with (quote x) form",
                )
                .with_hint("pick one — we prefer the reader quote 'x"),
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────

fn walk<F: FnMut(&Node)>(node: &Node, f: &mut F) {
    f(node);
    match &node.kind {
        NodeKind::List(items) => {
            for it in items {
                walk(it, f);
            }
        }
        NodeKind::Quote(inner)
        | NodeKind::Quasiquote(inner)
        | NodeKind::Unquote(inner)
        | NodeKind::UnquoteSplice(inner) => walk(inner, f),
        _ => {}
    }
}

fn matches_kwarg<P: Fn(&Node) -> bool>(items: &[Node], key: &str, pred: P) -> bool {
    let mut i = 0;
    while i + 1 < items.len() {
        if let NodeKind::Keyword(k) = &items[i].kind {
            if k == key && pred(&items[i + 1]) {
                return true;
            }
        }
        i += 2;
    }
    false
}

fn items_has_key(items: &[Node], key: &str) -> bool {
    let mut i = 0;
    while i + 1 < items.len() {
        if let NodeKind::Keyword(k) = &items[i].kind {
            if k == key {
                return true;
            }
        }
        i += 2;
    }
    false
}

fn span_lines(n: &Node) -> u32 {
    // Approximation: count newlines in the span's extent.
    // The node's span is byte offsets; we need source text to count. Without
    // it, fall back to estimating from span length / 40 chars/line.
    n.span.len() / 40
}

fn is_kebab(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}

fn is_pascal(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && s.chars().all(|c| c.is_ascii_alphanumeric())
}

fn to_kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.extend(c.to_ascii_lowercase().to_string().chars());
        } else if c == '_' {
            out.push('-');
        } else {
            out.push(c.to_ascii_lowercase());
        }
    }
    out
}

fn to_pascal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = true;
    for c in s.chars() {
        if c == '-' || c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_ascii_uppercase().to_string().chars());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use caixa_ast::parse;

    fn lint(src: &str) -> Vec<Diagnostic> {
        let nodes = parse(src).unwrap();
        let rules = all_rules();
        let mut diags = Vec::new();
        for node in &nodes {
            for rule in &rules {
                (rule.check)(node, &mut diags);
            }
        }
        diags
    }

    #[test]
    fn kebab_case_converter() {
        assert_eq!(to_kebab("fooBar"), "foo-bar");
        assert_eq!(to_kebab("foo_bar"), "foo-bar");
        assert_eq!(to_kebab("FooBarBaz"), "foo-bar-baz");
        assert!(is_kebab("caixa-teia"));
        assert!(!is_kebab("caixaTeia"));
    }

    #[test]
    fn pascal_case_converter() {
        assert_eq!(to_pascal("bib-lio-teca"), "BibLioTeca");
        assert_eq!(to_pascal("biblioteca"), "Biblioteca");
        assert!(is_pascal("Biblioteca"));
        assert!(!is_pascal("biblioteca"));
    }

    #[test]
    fn flags_fixme_descricao() {
        let src = r#"(defcaixa :nome "demo" :versao "0.1.0" :kind Biblioteca :descricao "FIXME — describe this")"#;
        let d = lint(src);
        assert!(d.iter().any(|d| d.rule_id == "no-fixme-descricao"));
    }

    #[test]
    fn flags_missing_kind() {
        let src = r#"(defcaixa :nome "demo" :versao "0.1.0")"#;
        let d = lint(src);
        assert!(d.iter().any(|d| d.rule_id == "explicit-kind"));
    }

    #[test]
    fn flags_quoted_enum_variant() {
        let src = r#"(defcaixa :nome "demo" :versao "0.1.0" :kind "Biblioteca")"#;
        let d = lint(src);
        assert!(
            d.iter().any(|d| d.rule_id == "enum-variant-pascal-case"),
            "diags: {:?}",
            d
        );
    }

    #[test]
    fn flags_camel_keyword() {
        let src = r#"(defcaixa :nomeCaixa "x")"#; // camelCase keyword — bad
        let d = lint(src);
        assert!(d.iter().any(|d| d.rule_id == "keyword-kebab-case"));
    }

    #[test]
    fn flags_git_without_pin() {
        let src = r#"
(defcaixa :nome "demo" :versao "0.1.0" :kind Biblioteca
  :deps ((:nome "x" :versao "*"
          :fonte (:tipo git :repo "github:o/x" :branch "main"))))
"#;
        let d = lint(src);
        assert!(d.iter().any(|d| d.rule_id == "git-dep-needs-pin"));
    }

    #[test]
    fn clean_manifest_only_info_level() {
        let src = r#"(defcaixa
  :nome "demo"
  :versao "0.1.0"
  :kind Biblioteca
  :descricao "a demo"
  :deps ((:nome "caixa-teia" :versao "^0.1"
          :fonte (:tipo git :repo "github:pleme-io/caixa-teia" :tag "v0.1.0"))))"#;
        let d = lint(src);
        let errors: Vec<_> = d.iter().filter(|d| d.severity == Severity::Error).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }
}
