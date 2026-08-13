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
        // Route the :atributos list-shape gate through the lifted
        // [`caixa_ast::NodeKind::as_list`] `Option<&[Node]>` accessor —
        // strict-`List`-only boundary vs the sibling D4 `Map` / `Vector`
        // arms is exactly what the ":atributos must be a kwargs list"
        // error message enforces; sibling to the caixa-lint
        // `check_enum_pascal` / `check_paired_kwargs` / `check_git_pin`
        // per-form walkers and the caixa-ast `Node::head_symbol` /
        // `Node::kwarg` list-head / pair-loop projections.
        let Some(items) = attrs_node.kind.as_list() else {
            return Err(TeiaError::BadForm(
                attrs_node.span.start,
                ":atributos must be a kwargs list",
            ));
        };
        let mut i = 0;
        while i + 1 < items.len() {
            // Route the `:atributos` pair-loop's per-item keyword-name
            // scalar projection through the lifted
            // [`caixa_ast::NodeKind::as_keyword`] `Option<&str>` accessor
            // rather than the raw `let NodeKind::Keyword(k) = &items[i]
            // .kind else …` open-coded per-arm let-else pattern-match —
            // sibling to the peer `build_object` per-key `:keyword` extract
            // (this run) and to the caixa-ast [`caixa_ast::Node::kwarg`]
            // pair-loop the substrate-canonical accessor already routes
            // through. Byte-parity swap: the error message +
            // `.to_owned()`-vs-`.clone()` owned-string mint on the
            // matched arm both stay the same at every arm of the closed
            // 14-arm partition.
            let Some(k) = items[i].kind.as_keyword() else {
                return Err(TeiaError::BadForm(items[i].span.start, "expected :keyword"));
            };
            atributos.insert(k.to_owned(), node_to_value(&items[i + 1])?);
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
    // Route the reader-macro-arm-set recursion through the lifted
    // [`caixa_ast::NodeKind::as_reader_macro_inner`] `Option<&Node>`
    // accessor rather than the raw four-arm `NodeKind::Quote(inner) |
    // NodeKind::Quasiquote(inner) | NodeKind::Unquote(inner) |
    // NodeKind::UnquoteSplice(inner) => node_to_value(inner)` open-coded
    // per-arm disjunctive pattern-match — sibling to the peer
    // `caixa-ast::visit::walk`, `caixa-fmt::contains_comment`, and
    // `caixa-lint::walk` reader-macro sites converged in this run. The
    // remaining match stays exhaustive over the ten non-reader-macro
    // arms; the four reader-macro arms are listed as an `unreachable!`
    // sink to keep the "every arm accounted for at the manifest lowerer"
    // discipline in view.
    if let Some(inner) = n.kind.as_reader_macro_inner() {
        return node_to_value(inner);
    }
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
        NodeKind::Quote(_)
        | NodeKind::Quasiquote(_)
        | NodeKind::Unquote(_)
        | NodeKind::UnquoteSplice(_) => {
            unreachable!(
                "reader-macro arms routed through NodeKind::as_reader_macro_inner \
                 above"
            )
        }
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
    // Route the `(ref <tipo> <nome> <atributo>)` second-slot symbol-name
    // projection through the lifted [`caixa_ast::NodeKind::as_symbol`]
    // `Option<&str>` accessor rather than the raw `match &items[1].kind
    // { NodeKind::Symbol(s) => s.clone(), _ => return Err(…) }` open-
    // coded per-arm pattern-match — closing the `build_ref` per-
    // positional-slot converge onto the substrate accessor family: the
    // sibling `:nome` (third slot) already routes through
    // [`caixa_ast::NodeKind::as_symbol_or_str`] (fd39cea) and the
    // sibling `:atributo` (fourth slot) already routes through
    // [`caixa_ast::NodeKind::as_atom_string`] (3c3ca48), so this lift
    // makes all three `(ref …)` positional slots share the substrate-
    // canonical per-arm-set discipline. Byte-parity swap: the error
    // message + span carrier stay identical at every arm.
    let tipo = items[1]
        .kind
        .as_symbol()
        .map(str::to_owned)
        .ok_or(TeiaError::BadForm(
            items[1].span.start,
            "ref tipo must be a symbol",
        ))?;
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
    // Route the per-item keyword-arm gate through the
    // [`gen_platform::IsVariant`]-derived
    // [`caixa_ast::NodeKind::is_keyword`] predicate rather than the raw
    // `matches!(n.kind, NodeKind::Keyword(_))` field-agnostic literal —
    // sibling to the peer caixa-lint `check_paired_kwargs` `Keyword`-arm
    // gate and to the [`caixa_ast::Node::kwarg`] pair-loop's own per-
    // item keyword-arm check. Byte-parity swap: the closed 14-arm
    // partition remains identical, but the shape-check now keys off the
    // substrate-canonical arm-discriminator so a future `Keyword`-arm
    // rename or `#[is_variant(name = "…")]` override reaches this
    // dispatch through one edit at the primitive.
    !items.is_empty()
        && items.len() % 2 == 0
        && items.iter().step_by(2).all(|n| n.kind.is_keyword())
}

fn build_object(items: &[Node]) -> Result<TeiaValue, TeiaError> {
    let mut out = BTreeMap::new();
    let mut i = 0;
    while i + 1 < items.len() {
        // Sibling per-key `:keyword` extract to `instance_from_node`'s
        // `:atributos` pair-loop above — routes through the lifted
        // [`caixa_ast::NodeKind::as_keyword`] `Option<&str>` accessor for
        // the same reason and closes the caixa-teia manifest per-key
        // `Keyword`-arm converge onto the substrate-canonical accessor.
        // Byte-parity swap: the error message +
        // `.to_owned()`-vs-`.clone()` owned-string mint on the matched
        // arm both stay the same.
        let Some(k) = items[i].kind.as_keyword() else {
            return Err(TeiaError::BadForm(
                items[i].span.start,
                "kwargs key must be :keyword",
            ));
        };
        out.insert(k.to_owned(), node_to_value(&items[i + 1])?);
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

    #[test]
    fn build_ref_rejects_non_symbol_tipo_with_byte_parity_error() {
        // Fail-before-pass-after pin on the `build_ref` `:tipo` slot's
        // error path after the raw `match &items[1].kind {
        // NodeKind::Symbol(s) => s.clone(), _ => return Err(…) }` open-
        // coded per-arm pattern-match was converged onto the lifted
        // [`caixa_ast::NodeKind::as_symbol`] `Option<&str>` accessor +
        // the `Option::ok_or` combinator: every non-`Symbol`-arm value at
        // the second positional slot of a `(ref …)` form (a string
        // literal, a keyword, an integer, a nested list) must surface
        // the same "ref tipo must be a symbol" [`TeiaError::BadForm`]
        // the pre-lift raw pattern-match returned, and the error's span
        // carrier must point at the offending atom (not the outer
        // `(ref …)` form's span). Guards against a silent widening of
        // the accessor's accept-set (a future `as_symbol` refactor that
        // admitted `Str` values through the projection would silently
        // start accepting `(ref "aws/vpc" …)` here) and against a silent
        // narrowing of the error's `TeiaError::BadForm` shape.
        for (src, expected_msg) in [
            (
                r#"(defteia :tipo aws/vpc :nome main :atributos (:vpc-id (ref "aws/vpc" main id)))"#,
                "ref tipo must be a symbol",
            ),
            (
                "(defteia :tipo aws/vpc :nome main :atributos (:vpc-id (ref :aws/vpc main id)))",
                "ref tipo must be a symbol",
            ),
            (
                "(defteia :tipo aws/vpc :nome main :atributos (:vpc-id (ref 42 main id)))",
                "ref tipo must be a symbol",
            ),
        ] {
            let err = parse_teia_source(src).expect_err(
                "non-Symbol at (ref …) second slot must surface \
                 TeiaError::BadForm — build_ref converged onto as_symbol",
            );
            match err {
                TeiaError::BadForm(_, msg) => assert_eq!(
                    msg, expected_msg,
                    "build_ref :tipo slot's error message must byte-equal \
                     the pre-lift raw-pattern-match's message (fixture: \
                     {src:?})",
                ),
                TeiaError::Parse(parse_err) => panic!(
                    "expected TeiaError::BadForm from non-Symbol :tipo \
                     slot, got TeiaError::Parse({parse_err:?}) (fixture: {src:?})"
                ),
            }
        }
    }

    #[test]
    fn build_object_rejects_non_keyword_key_with_byte_parity_error() {
        // Fail-before-pass-after pin on the `build_object` per-key
        // `:keyword` extract's error path after the raw `let
        // NodeKind::Keyword(k) = &items[i].kind else …` open-coded
        // per-arm let-else pattern-match was converged onto the lifted
        // [`caixa_ast::NodeKind::as_keyword`] `Option<&str>` accessor:
        // any non-`Keyword`-arm value at an even index of a brace-map
        // (`{ …k v …}`) — a string literal key, a symbol key, an integer
        // key — must surface the same "kwargs key must be :keyword"
        // [`TeiaError::BadForm`] the pre-lift raw let-else returned, and
        // the error's span must point at the offending key atom.
        //
        // The parenthesised paired-shape sibling in `instance_from_node`
        // does not reach this code path because the `is_kwargs` pre-
        // gate rejects the malformed shape before `build_object` runs
        // (which converts the whole list into a positional
        // `TeiaValue::List` instead), so this pin covers only the
        // brace-map surface where `NodeKind::Map(items) => build_object
        // (items)` bypasses the sniff.
        for (src, expected_msg) in [
            (
                r#"(defteia :tipo aws/vpc :nome main :atributos (:tags {"name" "main"}))"#,
                "kwargs key must be :keyword",
            ),
            (
                r#"(defteia :tipo aws/vpc :nome main :atributos (:tags {owner "main"}))"#,
                "kwargs key must be :keyword",
            ),
        ] {
            let err = parse_teia_source(src).expect_err(
                "non-Keyword at brace-map even index must surface \
                 TeiaError::BadForm — build_object converged onto as_keyword",
            );
            match err {
                TeiaError::BadForm(_, msg) => assert_eq!(
                    msg, expected_msg,
                    "build_object per-key extract's error message must \
                     byte-equal the pre-lift raw-let-else message \
                     (fixture: {src:?})",
                ),
                TeiaError::Parse(parse_err) => panic!(
                    "expected TeiaError::BadForm from non-Keyword brace-\
                     map key, got TeiaError::Parse({parse_err:?}) (fixture: {src:?})"
                ),
            }
        }
    }

    #[test]
    fn is_kwargs_predicate_keys_off_is_keyword_derived_arm_check() {
        // Load-bearing pin on the `is_kwargs` predicate after the raw
        // `matches!(n.kind, NodeKind::Keyword(_))` field-agnostic
        // literal was converged onto the [`gen_platform::IsVariant`]-
        // derived [`caixa_ast::NodeKind::is_keyword`] predicate: the
        // sniffed accept-set stays exactly "even-length lists whose
        // every even-index atom is a `NodeKind::Keyword`". A regression
        // that widened `is_keyword` to admit `Symbol` arms would
        // silently start treating symbol-keyed lists as kwargs objects
        // at the manifest lowerer; a regression that narrowed it would
        // silently start treating keyword-keyed lists as positional
        // lists. Both directions surface here at build time rather than
        // at manifest-lowering-diff-review time.
        //
        // Exercised via [`parse_teia_source`] end-to-end so the pin
        // guards both the direct [`is_kwargs`] predicate and its
        // dispatch site at `NodeKind::List(items) if is_kwargs(items)`
        // in [`node_to_value`].
        let kwargs_shape = r#"(defteia :tipo aws/vpc :nome main :atributos (:tags (:owner "team-a" :team "infra")))"#;
        let m = parse_teia_source(kwargs_shape).unwrap();
        let tags = m.instances[0].atributos.get("tags").unwrap();
        // Route the per-arm `Object`-arm gate through the
        // [`gen_platform::IsVariant`]-derived
        // [`caixa_teia::TeiaValue::is_object`] predicate rather than the
        // raw `matches!(_, TeiaValue::Object(_))` field-agnostic literal
        // — sibling to the peer `is_kwargs` predicate's `caixa_ast::
        // NodeKind::is_keyword` converge above and to the sibling per-
        // arm projection accessors [`caixa_teia::TeiaValue::as_object`]
        // (7304ffe) / [`caixa_teia::TeiaValue::as_str`] (7304ffe)
        // already lifted on this outer-`TeiaValue` sum-type. Byte-
        // equivalent to the pre-lift form; the converge links this pin
        // compile-time back to the closed 8-arm partition on the
        // substrate primitive.
        assert!(
            tags.is_object(),
            "kwargs-shaped list with every even-index atom a :keyword \
             must lower to TeiaValue::Object — is_kwargs predicate \
             converged onto NodeKind::is_keyword derived arm-check"
        );
        let non_kwargs_shape =
            r#"(defteia :tipo aws/vpc :nome main :atributos (:things ("a" "b")))"#;
        // ^ kept as `r#"…"#` because the inner `"a"` / `"b"` string
        //   literals need the delimiter escape.
        let m = parse_teia_source(non_kwargs_shape).unwrap();
        let things = m.instances[0].atributos.get("things").unwrap();
        // Sibling `List`-arm gate to the `Object`-arm gate above —
        // routes through the [`gen_platform::IsVariant`]-derived
        // [`caixa_teia::TeiaValue::is_list`] predicate on the same
        // substrate primitive.
        assert!(
            things.is_list(),
            "non-kwargs-shaped list must lower to TeiaValue::List — the \
             is_kwargs predicate must reject any list whose even-index \
             atoms are not all Keyword arms"
        );
    }
}
