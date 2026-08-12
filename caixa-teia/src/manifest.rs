//! Top-level manifest — collects every `(defteia …)` in a source file into
//! a deterministic list, ready for backend rendering.

use std::collections::BTreeMap;

use caixa_ast::{Node, NodeKind, ParseError, parse};
use thiserror::Error;

use crate::instance::TeiaInstance;
use crate::value::{TeiaRefRepr, TeiaValue};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TeiaManifest {
    pub instances: Vec<TeiaInstance>,
}

#[derive(Debug, Error)]
pub enum TeiaError {
    #[error("parse: {0}")]
    Parse(#[from] ParseError),
    #[error("defteia form at offset {0}: {1}")]
    BadForm(u32, &'static str),
}

/// Parse a tatara-lisp source string, collecting every `(defteia …)`.
///
/// Non-`defteia` forms are silently ignored at this layer — other domains
/// (e.g. `defarquitetura`) consume the same source in their own pass.
pub fn parse_teia_source(src: &str) -> Result<TeiaManifest, TeiaError> {
    let nodes = parse(src)?;
    let mut out = TeiaManifest::default();
    for n in &nodes {
        if n.head_symbol() == Some("defteia") {
            out.instances.push(instance_from_node(n)?);
        }
    }
    Ok(out)
}

fn instance_from_node(n: &Node) -> Result<TeiaInstance, TeiaError> {
    let tipo = kwarg_symbol(n, "tipo").ok_or(TeiaError::BadForm(n.span.start, "missing :tipo"))?;
    let nome = kwarg_symbol(n, "nome").ok_or(TeiaError::BadForm(n.span.start, "missing :nome"))?;
    let mut atributos: BTreeMap<String, TeiaValue> = BTreeMap::new();
    if let Some(attrs_node) = n.kwarg("atributos") {
        let NodeKind::List(items) = &attrs_node.kind else {
            return Err(TeiaError::BadForm(
                attrs_node.span.start,
                ":atributos must be a kwargs list",
            ));
        };
        let mut i = 0;
        while i + 1 < items.len() {
            let NodeKind::Keyword(k) = &items[i].kind else {
                return Err(TeiaError::BadForm(items[i].span.start, "expected :keyword"));
            };
            atributos.insert(k.clone(), node_to_value(&items[i + 1])?);
            i += 2;
        }
    }
    Ok(TeiaInstance {
        tipo,
        nome,
        atributos,
    })
}

fn kwarg_symbol(n: &Node, key: &str) -> Option<String> {
    // Route the `:tipo`/`:nome` value-shape projection through the
    // lifted [`caixa_ast::NodeKind::as_atom_string`] `Option<&str>`
    // accessor rather than the raw three-arm `NodeKind::Symbol(s) |
    // NodeKind::Str(s) | NodeKind::Keyword(s) => Some(s.clone())`
    // open-coded per-arm disjunctive pattern-match — sibling in shape
    // to the `build_ref` `:atributo` slot converge on the same
    // atom-string-carrying disjunctive axis (this run).
    n.kwarg(key)?.kind.as_atom_string().map(str::to_owned)
}

fn node_to_value(n: &Node) -> Result<TeiaValue, TeiaError> {
    match &n.kind {
        NodeKind::Nil => Ok(TeiaValue::Null),
        NodeKind::Bool(b) => Ok(TeiaValue::Bool(*b)),
        NodeKind::Int(i) => Ok(TeiaValue::Int(*i)),
        NodeKind::Float(f) => Ok(TeiaValue::Float(*f)),
        NodeKind::Str(s) => Ok(TeiaValue::Str(s.clone())),
        NodeKind::Symbol(s) => Ok(TeiaValue::Str(s.clone())),
        NodeKind::Keyword(s) => Ok(TeiaValue::Str(format!(":{s}"))),
        NodeKind::List(items) if is_ref_form(items) => build_ref(items),
        NodeKind::List(items) if is_kwargs(items) => build_object(items),
        NodeKind::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(node_to_value(item)?);
            }
            Ok(TeiaValue::List(out))
        }
        // `{ :k v … }` is an object BY CONSTRUCTION — that is what the
        // braces mean. A parenthesised list only becomes one when
        // `is_kwargs` sniffs the alternating shape; a map needs no
        // sniffing, and `build_object` rejects a non-keyword key loudly
        // instead of silently degrading to a positional list.
        //
        // Before caixa-ast learned these delimiters, a brace map reached
        // here as a List whose first and last elements were the literal
        // SYMBOLS `{` and `}`. That is an odd-length run with a
        // non-keyword head, so `is_kwargs` rejected it and every nested
        // map in every manifest silently became a positional
        // `TeiaValue::List` of stringified braces.
        NodeKind::Map(items) => build_object(items),
        // `[ a b … ]` is a list by construction, with no kwargs sniffing:
        // a vector of `:keyword` atoms (`:workflows [ :auto-release ]`)
        // is a LIST OF KEYWORDS, not a half-written object.
        NodeKind::Vector(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(node_to_value(item)?);
            }
            Ok(TeiaValue::List(out))
        }
        NodeKind::Quote(inner)
        | NodeKind::Quasiquote(inner)
        | NodeKind::Unquote(inner)
        | NodeKind::UnquoteSplice(inner) => node_to_value(inner),
    }
}

/// `(ref aws/vpc main id)` pattern detector.
///
/// Routes the head-slot symbol-name projection through the lifted
/// [`caixa_ast::NodeKind::as_symbol`] `Option<&str>` accessor rather
/// than the raw `matches!(items.first().map(|n| &n.kind), Some(NodeKind::
/// Symbol(s)) if s == "ref")` guarded per-arm pattern-match — sibling
/// in shape to the caixa-lint `check_consistent_quote` `(quote …)`-
/// form detector on the same list-head symbol-name axis (converged in
/// this run) and to the caixa-ast [`caixa_ast::Node::head_symbol`]
/// list-head projection.
fn is_ref_form(items: &[Node]) -> bool {
    items.first().and_then(|n| n.kind.as_symbol()) == Some("ref") && items.len() == 4
}

fn build_ref(items: &[Node]) -> Result<TeiaValue, TeiaError> {
    let tipo = match &items[1].kind {
        NodeKind::Symbol(s) => s.clone(),
        _ => {
            return Err(TeiaError::BadForm(
                items[1].span.start,
                "ref tipo must be a symbol",
            ));
        }
    };
    // Route the `:nome`-slot projection through the lifted
    // [`caixa_ast::NodeKind::as_symbol_or_str`] `Option<&str>` accessor
    // rather than the raw two-arm `NodeKind::Symbol(s) | NodeKind::Str(s)
    // => s.clone()` open-coded per-arm disjunctive pattern-match —
    // sibling in shape to the caixa-lsp `document_symbol` `:nome`-detail
    // projection converge on the same atom-name-carrying disjunctive axis
    // (this run) and to the neighbouring `:atributo`-slot converge on
    // the three-arm [`NodeKind::as_atom_string`] projection (3c3ca48).
    let nome = items[2]
        .kind
        .as_symbol_or_str()
        .map(str::to_owned)
        .ok_or(TeiaError::BadForm(
            items[2].span.start,
            "ref nome must be a symbol or string",
        ))?;
    // Route the `:atributo`-slot projection through the lifted
    // [`caixa_ast::NodeKind::as_atom_string`] `Option<&str>` accessor
    // rather than the raw three-arm `NodeKind::Symbol(s) |
    // NodeKind::Str(s) | NodeKind::Keyword(s) => s.clone()` open-coded
    // per-arm disjunctive pattern-match — sibling in shape to the
    // `kwarg_symbol` `:tipo`/`:nome` value-shape gate converge on the
    // same atom-string-carrying disjunctive axis (this run).
    let atributo = items[3]
        .kind
        .as_atom_string()
        .map(str::to_owned)
        .ok_or(TeiaError::BadForm(
            items[3].span.start,
            "ref atributo must be a symbol/keyword/string",
        ))?;
    Ok(TeiaValue::Ref(TeiaRefRepr {
        tipo,
        nome,
        atributo,
    }))
}

fn is_kwargs(items: &[Node]) -> bool {
    !items.is_empty()
        && items.len() % 2 == 0
        && items
            .iter()
            .step_by(2)
            .all(|n| matches!(n.kind, NodeKind::Keyword(_)))
}

fn build_object(items: &[Node]) -> Result<TeiaValue, TeiaError> {
    let mut out = BTreeMap::new();
    let mut i = 0;
    while i + 1 < items.len() {
        let NodeKind::Keyword(k) = &items[i].kind else {
            return Err(TeiaError::BadForm(
                items[i].span.start,
                "kwargs key must be :keyword",
            ));
        };
        out.insert(k.clone(), node_to_value(&items[i + 1])?);
        i += 2;
    }
    Ok(TeiaValue::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_one_defteia() {
        let src = r#"
(defteia
  :tipo     aws/vpc
  :nome     main
  :atributos (:cidr-block "10.0.0.0/16"
              :tags (:name "main")))
"#;
        let m = parse_teia_source(src).unwrap();
        assert_eq!(m.instances.len(), 1);
        let inst = &m.instances[0];
        assert_eq!(inst.tipo, "aws/vpc");
        assert_eq!(inst.nome, "main");
        assert_eq!(
            inst.atributos.get("cidr-block"),
            Some(&TeiaValue::Str("10.0.0.0/16".into()))
        );
        let tags = inst.atributos.get("tags").unwrap();
        match tags {
            TeiaValue::Object(m) => {
                assert_eq!(m.get("name"), Some(&TeiaValue::Str("main".into())));
            }
            other => panic!("expected object tags, got {other:?}"),
        }
    }

    #[test]
    fn parses_ref() {
        let src = r#"
(defteia :tipo aws/vpc :nome main :atributos (:cidr-block "10.0.0.0/16"))
(defteia :tipo aws/igw :nome main :atributos (:vpc-id (ref aws/vpc main id)))
"#;
        let m = parse_teia_source(src).unwrap();
        assert_eq!(m.instances.len(), 2);
        let igw = &m.instances[1];
        let vpc_id = igw.atributos.get("vpc-id").unwrap();
        match vpc_id {
            TeiaValue::Ref(r) => {
                assert_eq!(r.tipo, "aws/vpc");
                assert_eq!(r.nome, "main");
                assert_eq!(r.atributo, "id");
            }
            other => panic!("expected ref, got {other:?}"),
        }
    }

    #[test]
    fn renders_to_hcl() {
        let src = r#"(defteia :tipo aws/vpc :nome main :atributos (:cidr-block "10.0.0.0/16"))"#;
        let m = parse_teia_source(src).unwrap();
        let hcl = m.instances[0].to_hcl();
        assert!(hcl.contains("resource \"aws_vpc\" \"main\""));
        assert!(hcl.contains(r#""10.0.0.0/16""#));
    }
}
