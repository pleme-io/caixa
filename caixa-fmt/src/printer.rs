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
            p.emit_leading(&n.leading, 0);
        }
        p.emit(n, 0);
    }
    if cfg.trailing_newline && !p.out.ends_with('\n') {
        p.out.push('\n');
    }
    p.out
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
            NodeKind::List(items) => self.emit_list(items, indent),
        }
    }

    fn emit_list(&mut self, items: &[Node], indent: usize) {
        if items.is_empty() {
            self.out.push_str("()");
            return;
        }

        let inline = render_inline(items);
        let current_col = current_column(&self.out);
        if inline.len() + 2 + current_col <= self.cfg.line_width {
            self.out.push('(');
            self.out.push_str(&inline);
            self.out.push(')');
            return;
        }

        // Multi-line: open paren, then body, then close.
        self.out.push('(');
        let child_indent = indent + self.cfg.indent;

        if is_kwargs_list(items) {
            // Head symbol on the opening line, pairs indented below.
            let (head, start) = match &items.first().map(|n| &n.kind) {
                Some(NodeKind::Symbol(s)) => (Some(s.as_str()), 1usize),
                _ => (None, 0usize),
            };
            if let Some(h) = head {
                self.out.push_str(h);
            }
            let mut i = start;
            while i + 1 < items.len() {
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
                self.out.push('\n');
                push_spaces(&mut self.out, child_indent);
                self.emit(item, child_indent);
            }
        }
        self.out.push(')');
    }
}

/// Does this list follow the `(head :k v :k v …)` or `(:k v :k v …)`
/// convention?
fn is_kwargs_list(items: &[Node]) -> bool {
    let start = if matches!(items.first().map(|n| &n.kind), Some(NodeKind::Symbol(_))) {
        1
    } else {
        0
    };
    let rest = items.len().saturating_sub(start);
    if rest == 0 || rest % 2 != 0 {
        return false;
    }
    let mut i = start;
    while i + 1 < items.len() {
        if !matches!(items[i].kind, NodeKind::Keyword(_)) {
            return false;
        }
        i += 2;
    }
    true
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
}
