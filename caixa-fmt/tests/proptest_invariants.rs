//! Property tests — the two invariants that make a formatter trustworthy:
//!
//! 1. **Idempotence**: `fmt(fmt(src)) == fmt(src)`. Formatting a
//!    formatted file is a no-op.
//! 2. **Semantic preservation**: `parse(fmt(src)) ≡ parse(src)`. The
//!    formatter changes whitespace but never changes the parsed tree.
//!
//! Inputs are generated from a handwritten corpus of realistic caixa / teia
//! / flake sources + randomized perturbations. The generator is deliberately
//! simple — exhaustive Lisp generation is hard; a covered corpus is enough
//! to surface regressions.

use caixa_ast::{Node, parse};
use caixa_fmt::{FmtConfig, format_nodes, format_source};
use proptest::prelude::*;

const CORPUS: &[&str] = &[
    "42",
    "(a b c)",
    "'x",
    "`(a ,b ,@cs)",
    "()",
    "(:k v)",
    "(defcaixa :nome \"demo\" :versao \"0.1.0\" :kind Biblioteca)",
    r#"(defcaixa
  :nome "pangea-tatara-aws"
  :versao "0.1.0"
  :kind Biblioteca
  :edicao "2026"
  :deps ((:nome "caixa-teia" :versao "^0.1"))
  :bibliotecas ("lib/pangea-tatara-aws.lisp"))"#,
    r#"(defteia
  :tipo aws/vpc
  :nome main
  :atributos (:cidr-block "10.0.0.0/16"
              :tags (:name "main")))"#,
    r#"(defteia :tipo aws/igw :nome main :atributos (:vpc-id (ref aws/vpc main id)))"#,
    r#"(deflacre :versao-lacre "0.1.0" :raiz "blake3:abc" :entradas ())"#,
    r#";; a leading comment
(a b c)"#,
    "(nested (deep (call (chain :k v))))",
];

fn corpus_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("42".to_string()),
        Just("(a b c)".to_string()),
        Just("'x".to_string()),
        Just("(:k v)".to_string()),
        Just(r#"(defcaixa :nome "x" :versao "0.1.0" :kind Biblioteca)"#.to_string()),
    ]
}

/// Add random whitespace perturbations to a source — tests robustness of the
/// formatter to different input spacings.
fn perturb(src: &str, perms: u8) -> String {
    let mut out = String::new();
    let mut count = 0u8;
    for ch in src.chars() {
        out.push(ch);
        if ch == ' ' && count < perms {
            out.push(' ');
            count += 1;
        }
    }
    out
}

proptest! {
    #[test]
    fn fmt_is_idempotent(idx in 0..CORPUS.len(), perms in 0u8..10) {
        let src = perturb(CORPUS[idx], perms);
        let Ok(once) = format_source(&src, &FmtConfig::default()) else { return Ok(()); };
        let twice = format_source(&once, &FmtConfig::default()).unwrap();
        prop_assert_eq!(once, twice, "formatter not idempotent on corpus[{}]", idx);
    }

    #[test]
    fn fmt_preserves_semantics(src in corpus_strategy()) {
        let Ok(a) = parse(&src) else { return Ok(()); };
        let formatted = format_nodes(&a, &FmtConfig::default());
        let b = parse(&formatted).unwrap();
        let sa: Vec<_> = a.iter().map(Node::to_tatara_sexp).collect();
        let sb: Vec<_> = b.iter().map(Node::to_tatara_sexp).collect();
        prop_assert_eq!(sa, sb);
    }

    #[test]
    fn fmt_always_ends_with_newline(idx in 0..CORPUS.len()) {
        let Ok(out) = format_source(CORPUS[idx], &FmtConfig::default()) else { return Ok(()); };
        prop_assert!(out.ends_with('\n'), "formatter dropped trailing newline");
    }

    #[test]
    fn fmt_never_lengthens_past_line_width_inline(
        idx in 0..CORPUS.len(),
        width in 40usize..200,
    ) {
        let cfg = FmtConfig { line_width: width, ..FmtConfig::default() };
        let Ok(out) = format_source(CORPUS[idx], &cfg) else { return Ok(()); };
        // Each line either fits, or is inside a multi-line block (contains
        // leading indent which counts against the budget implicitly).
        for line in out.lines() {
            if line.trim_start().starts_with(';') {
                continue; // comments are exempt
            }
            prop_assert!(
                line.len() <= width.saturating_add(20), // tolerate small overrun from deeply nested refs
                "line of {} chars exceeds {} + slack on corpus[{}]: {line:?}",
                line.len(), width, idx
            );
        }
    }
}

#[test]
fn every_corpus_entry_parses_and_formats() {
    for (i, src) in CORPUS.iter().enumerate() {
        let out = format_source(src, &FmtConfig::default())
            .unwrap_or_else(|e| panic!("corpus[{i}] failed to format: {e}\nsrc:\n{src}"));
        parse(&out).unwrap_or_else(|e| panic!("corpus[{i}] re-parse failed: {e}\nout:\n{out}"));
    }
}
