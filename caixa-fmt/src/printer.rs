use caixa_ast::{Node, NodeKind, ParseError, Trivia, TriviaKind, parse};
use thiserror::Error;

use crate::config::FmtConfig;

#[derive(Debug, Error)]
pub enum FmtError {
    #[error("parse: {0}")]
    Parse(#[from] ParseError),
}

/// Parse + format in one call.
pub fn format_source(src: &str, cfg: &FmtConfig) -> Result<String, FmtError> {
    let nodes = parse(src)?;
    Ok(format_nodes(&nodes, cfg))
}

/// Format an already-parsed node slice.
#[must_use]
pub fn format_nodes(nodes: &[Node], cfg: &FmtConfig) -> String {
    let mut p = Printer {
        out: String::new(),
        cfg,
    };
    for (i, n) in nodes.iter().enumerate() {
        if i > 0 {
            p.out.push('\n');
            p.out.push('\n');
        }
        if cfg.preserve_comments {
            // Top-level: the separator above already provides the
            // single blank line between top-level forms. Skip any
            // leading BlankLine trivia at the start so the formatter
            // is idempotent (otherwise each pass would accumulate
            // an extra blank line).
            let leading = trim_leading_blanks(&n.leading);
            p.emit_leading(leading, 0);
        }
        p.emit(n, 0);
    }
    if cfg.trailing_newline && !p.out.ends_with('\n') {
        p.out.push('\n');
    }
    p.out
}

/// Drop any prefix of [`TriviaKind::BlankLine`] entries.
fn trim_leading_blanks(trivia: &[Trivia]) -> &[Trivia] {
    let drop = trivia
        .iter()
        .take_while(|t| matches!(t.kind, TriviaKind::BlankLine))
        .count();
    &trivia[drop..]
}

struct Printer<'a> {
    out: String,
    cfg: &'a FmtConfig,
}

impl Printer<'_> {
    fn emit_leading(&mut self, trivia: &[Trivia], indent: usize) {
        for t in trivia {
            match &t.kind {
                TriviaKind::LineComment(text) => {
                    push_spaces(&mut self.out, indent);
                    self.out.push(';');
                    self.out.push_str(text);
                    self.out.push('\n');
                }
                TriviaKind::BlankLine => {
                    self.out.push('\n');
                }
            }
        }
    }

    fn emit(&mut self, n: &Node, indent: usize) {
        match &n.kind {
            NodeKind::Nil => self.out.push_str("nil"),
            NodeKind::Bool(true) => self.out.push_str("#t"),
            NodeKind::Bool(false) => self.out.push_str("#f"),
            NodeKind::Int(i) => self.out.push_str(&i.to_string()),
            NodeKind::Float(f) => self.out.push_str(&format_float(*f)),
            NodeKind::Str(s) => emit_string(s, &mut self.out),
            NodeKind::Symbol(s) => self.out.push_str(s),
            NodeKind::Keyword(s) => {
                self.out.push(':');
                self.out.push_str(s);
            }
            NodeKind::Quote(inner) => {
                self.out.push('\'');
                self.emit(inner, indent);
            }
            NodeKind::Quasiquote(inner) => {
                self.out.push('`');
                self.emit(inner, indent);
            }
            NodeKind::Unquote(inner) => {
                self.out.push(',');
                self.emit(inner, indent);
            }
            NodeKind::UnquoteSplice(inner) => {
                self.out.push_str(",@");
                self.emit(inner, indent);
            }
            NodeKind::List(items) => self.emit_list(items, &n.trailing, indent),
        }
    }

    /// Emit the comment / blank-line trivia that sits *before* a child, each
    /// on its own line at `indent`.
    ///
    /// Deliberately leaves the cursor at the end of the last comment with no
    /// newline: the caller still owes the child its own `\n` + indent, so
    /// emitting one here would double it. A `BlankLine` contributes exactly
    /// one `\n`, which combines with the caller's to re-parse as one blank
    /// line — that is what keeps the formatter idempotent.
    fn emit_child_trivia(&mut self, trivia: &[Trivia], indent: usize) {
        if !self.cfg.preserve_comments {
            return;
        }
        for t in trivia {
            match &t.kind {
                TriviaKind::LineComment(text) => {
                    self.out.push('\n');
                    push_spaces(&mut self.out, indent);
                    self.out.push(';');
                    self.out.push_str(text);
                }
                TriviaKind::BlankLine => self.out.push('\n'),
            }
        }
    }

    fn emit_list(&mut self, items: &[Node], dangling: &[Trivia], indent: usize) {
        let keep_comments = self.cfg.preserve_comments;
        let has_dangling_comment = keep_comments && contains_comment_trivia(dangling);

        if items.is_empty() && !has_dangling_comment {
            self.out.push_str("()");
            return;
        }

        // A comment anywhere in this subtree forces the multi-line shape.
        // `render_inline` flattens to a single line and has nowhere to put a
        // `; …` (it would swallow the rest of the line), so inlining a
        // commented form is exactly how comments used to disappear.
        let force_break =
            has_dangling_comment || (keep_comments && items.iter().any(contains_comment));

        if !force_break {
            let inline = render_inline(items);
            let current_col = current_column(&self.out);
            if inline.len() + 2 + current_col <= self.cfg.line_width {
                self.out.push('(');
                self.out.push_str(&inline);
                self.out.push(')');
                return;
            }
        }

        // Multi-line: open paren, then body, then close.
        self.out.push('(');
        let child_indent = indent + self.cfg.indent;

        // The kwargs shape only applies when the leading symbols carry no
        // comments — they share the opening line, which has no room for one.
        let head_len = kwargs_head_len(items).filter(|n| {
            !keep_comments
                || !items[..*n]
                    .iter()
                    .any(|i| contains_comment_trivia(&i.leading))
        });

        if let Some(head_len) = head_len {
            // Head symbols (`defcaixa`, and the name it defines) share the
            // opening line; the `:key value` pairs are indented below it.
            for (k, item) in items[..head_len].iter().enumerate() {
                if k > 0 {
                    self.out.push(' ');
                }
                self.emit(item, indent);
            }
            let mut i = head_len;
            while i + 1 < items.len() {
                self.emit_child_trivia(&items[i].leading, child_indent);
                self.out.push('\n');
                push_spaces(&mut self.out, child_indent);
                // key
                self.emit(&items[i], child_indent);
                self.out.push(' ');
                // value
                self.emit(&items[i + 1], child_indent);
                i += 2;
            }
        } else {
            // Head on the opening line, rest indented.
            self.emit(&items[0], indent + 1);
            for item in &items[1..] {
                self.emit_child_trivia(&item.leading, child_indent);
                self.out.push('\n');
                push_spaces(&mut self.out, child_indent);
                self.emit(item, child_indent);
            }
        }
        if has_dangling_comment {
            self.emit_child_trivia(dangling, child_indent);
            self.out.push('\n');
            push_spaces(&mut self.out, indent);
        }
        self.out.push(')');
    }
}

/// Does any comment live anywhere in this node's subtree?
fn contains_comment(n: &Node) -> bool {
    if contains_comment_trivia(&n.leading) || contains_comment_trivia(&n.trailing) {
        return true;
    }
    match &n.kind {
        NodeKind::List(items) => items.iter().any(contains_comment),
        NodeKind::Quote(inner)
        | NodeKind::Quasiquote(inner)
        | NodeKind::Unquote(inner)
        | NodeKind::UnquoteSplice(inner) => contains_comment(inner),
        _ => false,
    }
}

fn contains_comment_trivia(trivia: &[Trivia]) -> bool {
    trivia
        .iter()
        .any(|t| matches!(t.kind, TriviaKind::LineComment(_)))
}

/// If this list follows the `(head… :k v :k v …)` convention, how many
/// leading symbols precede the first keyword?
///
/// Returns `Some(0)` for a bare `(:k v …)` plist, `Some(1)` for
/// `(defteia :k v …)`, and `Some(2)` for `(defcaixa todoku-go :k v …)` —
/// the canonical `caixa.lisp` shape, which the old single-symbol version
/// rejected. That rejection is why every real manifest fell through to the
/// generic branch and got exploded one atom per line, splitting `:kind` from
/// its value.
fn kwargs_head_len(items: &[Node]) -> Option<usize> {
    let head = items
        .iter()
        .take_while(|n| matches!(n.kind, NodeKind::Symbol(_)))
        .count();
    let rest = items.len().saturating_sub(head);
    if rest == 0 || rest % 2 != 0 {
        return None;
    }
    let mut i = head;
    while i + 1 < items.len() {
        if !matches!(items[i].kind, NodeKind::Keyword(_)) {
            return None;
        }
        i += 2;
    }
    Some(head)
}

/// Render a list's inner body as a single-line string, for width checking.
fn render_inline(items: &[Node]) -> String {
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        render_node_inline(item, &mut out);
    }
    out
}

fn render_node_inline(n: &Node, out: &mut String) {
    match &n.kind {
        NodeKind::Nil => out.push_str("nil"),
        NodeKind::Bool(true) => out.push_str("#t"),
        NodeKind::Bool(false) => out.push_str("#f"),
        NodeKind::Int(i) => out.push_str(&i.to_string()),
        NodeKind::Float(f) => out.push_str(&format_float(*f)),
        NodeKind::Str(s) => emit_string(s, out),
        NodeKind::Symbol(s) => out.push_str(s),
        NodeKind::Keyword(s) => {
            out.push(':');
            out.push_str(s);
        }
        NodeKind::Quote(inner) => {
            out.push('\'');
            render_node_inline(inner, out);
        }
        NodeKind::Quasiquote(inner) => {
            out.push('`');
            render_node_inline(inner, out);
        }
        NodeKind::Unquote(inner) => {
            out.push(',');
            render_node_inline(inner, out);
        }
        NodeKind::UnquoteSplice(inner) => {
            out.push_str(",@");
            render_node_inline(inner, out);
        }
        NodeKind::List(items) => {
            out.push('(');
            out.push_str(&render_inline(items));
            out.push(')');
        }
    }
}

fn emit_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str(r#"\""#),
            '\\' => out.push_str(r"\\"),
            '\n' => out.push_str(r"\n"),
            '\r' => out.push_str(r"\r"),
            '\t' => out.push_str(r"\t"),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn format_float(f: f64) -> String {
    if f == f.trunc() && f.is_finite() {
        // Preserve float-ness with a trailing ".0" to disambiguate from int.
        format!("{f:.1}")
    } else {
        format!("{f}")
    }
}

fn push_spaces(out: &mut String, n: usize) {
    for _ in 0..n {
        out.push(' ');
    }
}

fn current_column(out: &str) -> usize {
    out.rsplit('\n').next().map_or(0, str::len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(src: &str) -> String {
        format_source(src, &FmtConfig::default()).unwrap()
    }

    #[test]
    fn inline_small_forms() {
        assert_eq!(fmt("(a b c)"), "(a b c)\n");
        assert_eq!(fmt("(:x 1 :y 2)"), "(:x 1 :y 2)\n");
    }

    #[test]
    fn kwargs_break_when_wide() {
        let src = r#"(defcaixa :nome "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx" :versao "0.1.0" :kind Biblioteca :descricao "description goes here")"#;
        let out = fmt(src);
        assert!(out.contains("(defcaixa\n"));
        assert!(out.contains(":nome \""));
        assert!(out.contains(":versao \"0.1.0\""));
    }

    #[test]
    fn empty_list_inline() {
        assert_eq!(fmt("()"), "()\n");
    }

    #[test]
    fn round_trip_preserves_parse() {
        let src = r#"(defcaixa :nome "demo" :versao "0.1.0" :kind Biblioteca)"#;
        let a = parse(src).unwrap();
        let formatted = format_nodes(&a, &FmtConfig::default());
        let b = parse(&formatted).unwrap();
        // tatara-lisp Sexp round-trip (ignores spans)
        let sa: Vec<_> = a.iter().map(Node::to_tatara_sexp).collect();
        let sb: Vec<_> = b.iter().map(Node::to_tatara_sexp).collect();
        assert_eq!(sa, sb);
    }

    #[test]
    fn preserves_leading_comments() {
        let src = "; top doc\n(a b c)\n";
        let out = fmt(src);
        assert!(out.starts_with("; top doc\n"));
    }

    #[test]
    fn float_gets_decimal_point() {
        assert_eq!(fmt("3.0"), "3.0\n");
        assert_eq!(fmt("3.14"), "3.14\n");
    }

    #[test]
    fn string_escapes() {
        let src = r#"(:msg "hello\nworld")"#;
        let out = fmt(src);
        assert!(out.contains(r#""hello\nworld""#));
    }

    /// `(defcaixa <name> :k v …)` is THE caixa.lisp shape. `is_kwargs_list`
    /// only ever skipped one leading symbol, so the name in slot 1 made the
    /// list "not kwargs" and every real manifest fell through to the generic
    /// branch — one atom per line, `:kind` severed from its value.
    #[test]
    fn named_def_form_keeps_key_value_pairs_together() {
        let src = r#"(defcaixa todoku-go :kind :Biblioteca :ecosystem :go :versao "0.3.0" :descricao "long enough to force this form onto multiple lines")"#;
        let out = fmt(src);
        assert!(
            out.starts_with("(defcaixa todoku-go\n"),
            "head + name should share the opening line, got:\n{out}"
        );
        assert!(out.contains("  :kind :Biblioteca\n"), "got:\n{out}");
        assert!(out.contains("  :ecosystem :go\n"), "got:\n{out}");
    }

    #[test]
    fn kwargs_head_len_shapes() {
        let head_of = |src: &str| {
            let nodes = parse(src).unwrap();
            match &nodes[0].kind {
                NodeKind::List(items) => kwargs_head_len(items),
                other => panic!("expected list, got {other:?}"),
            }
        };
        assert_eq!(head_of("(:k v)"), Some(0));
        assert_eq!(head_of("(defteia :k v)"), Some(1));
        assert_eq!(head_of("(defcaixa demo :k v)"), Some(2));
        // Not a plist: no keywords at all.
        assert_eq!(head_of("(a b c)"), None);
        // Not a plist: dangling value.
        assert_eq!(head_of("(defcaixa demo :k)"), None);
    }

    /// Comments *inside* a form used to be dropped outright: `emit_leading`
    /// was only ever called from the top-level loop, and any form narrow
    /// enough to inline was flattened by `render_inline`, which has nowhere
    /// to put a `; …`.
    #[test]
    fn preserves_comments_inside_a_form() {
        let src = "(defcaixa demo\n  ;; why this kind\n  :kind :Biblioteca)\n";
        let out = fmt(src);
        assert!(out.contains(";; why this kind"), "got:\n{out}");
        assert!(out.contains(":kind :Biblioteca"), "got:\n{out}");
    }

    /// Trivia between the last item and `)` has no child to attach to; the
    /// parser used to discard it, deleting the last comment of every form.
    #[test]
    fn preserves_comment_before_the_closing_paren() {
        let src = "(defcaixa demo\n  :kind :Biblioteca\n  ;; trailing note\n  )\n";
        let out = fmt(src);
        assert!(out.contains(";; trailing note"), "got:\n{out}");
    }

    /// A commented form must never be inlined — inlining is what swallowed
    /// the comment, and a `;` on a joined line would eat the rest of it.
    #[test]
    fn commented_form_never_inlines() {
        let out = fmt("(a ;; note\n b)\n");
        assert!(out.contains(";; note"), "got:\n{out}");
        assert!(parse(&out).is_ok(), "a re-parse must still succeed:\n{out}");
    }

    #[test]
    fn comment_preservation_is_idempotent() {
        let src = "(defcaixa demo\n  ;; a\n  :kind :Biblioteca\n\n  ;; b\n  :versao \"0.1.0\"\n  ;; dangling\n  )\n";
        let once = fmt(src);
        let twice = fmt(&once);
        assert_eq!(once, twice, "not idempotent; first pass was:\n{once}");
    }
}
