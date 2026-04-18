//! Integration tests: caixa.lisp source → typed `Caixa` via tatara-lisp derive.

use caixa_core::{Caixa, CaixaKind, DepSource, LayoutInvariants, StandardLayout};
use pretty_assertions::assert_eq;
use std::path::{Path, PathBuf};

#[test]
fn parse_minimal_caixa() {
    let src = r#"
(defcaixa
  :nome "pangea-tatara-aws"
  :versao "0.1.0"
  :kind Biblioteca
  :edicao "2026"
  :bibliotecas ("lib/pangea-tatara-aws.lisp"))
"#;
    let c = Caixa::from_lisp(src).expect("parse minimal");
    assert_eq!(c.nome, "pangea-tatara-aws");
    assert_eq!(c.versao, "0.1.0");
    assert_eq!(c.kind, CaixaKind::Biblioteca);
    assert_eq!(c.edicao.as_deref(), Some("2026"));
    assert_eq!(
        c.bibliotecas,
        vec!["lib/pangea-tatara-aws.lisp".to_string()]
    );
    assert!(c.deps.is_empty());
    assert!(c.deps_dev.is_empty());
    assert!(c.autores.is_empty());
}

#[test]
fn parse_full_caixa_with_deps() {
    let src = r#"
(defcaixa
  :nome "demo"
  :versao "0.1.0"
  :kind Biblioteca
  :descricao "A demo caixa"
  :repositorio "github:pleme-io/demo"
  :licenca "MIT"
  :autores ("alice" "bob")
  :etiquetas ("demo" "example")
  :deps ((:nome "caixa-teia" :versao "^0.1")
         (:nome "tatara-lisp" :versao "*"
          :fonte (:tipo git :repo "github:pleme-io/tatara" :tag "v0.9.0")))
  :deps-dev ((:nome "tatara-check" :versao "*")))
"#;
    let c = Caixa::from_lisp(src).expect("parse full");
    assert_eq!(c.descricao.as_deref(), Some("A demo caixa"));
    assert_eq!(c.repositorio.as_deref(), Some("github:pleme-io/demo"));
    assert_eq!(c.licenca.as_deref(), Some("MIT"));
    assert_eq!(c.autores, vec!["alice".to_string(), "bob".to_string()]);
    assert_eq!(c.etiquetas, vec!["demo".to_string(), "example".to_string()]);

    assert_eq!(c.deps.len(), 2);
    assert_eq!(c.deps[0].nome, "caixa-teia");
    assert_eq!(c.deps[0].versao, "^0.1");
    assert!(c.deps[0].fonte.is_none());

    assert_eq!(c.deps[1].nome, "tatara-lisp");
    match &c.deps[1].fonte {
        Some(DepSource::Git { repo, tag, .. }) => {
            assert_eq!(repo, "github:pleme-io/tatara");
            assert_eq!(tag.as_deref(), Some("v0.9.0"));
        }
        other => panic!("expected Git source, got {other:?}"),
    }

    assert_eq!(c.deps_dev.len(), 1);
    assert_eq!(c.deps_dev[0].nome, "tatara-check");
}

#[test]
fn missing_required_errors() {
    let src = r#"(defcaixa :nome "x")"#;
    assert!(Caixa::from_lisp(src).is_err());
}

#[test]
fn wrong_head_errors() {
    let src = r#"(defsomething :nome "x" :versao "0.1.0" :kind Biblioteca)"#;
    assert!(Caixa::from_lisp(src).is_err());
}

#[test]
fn binario_kind_parses() {
    let src = r#"
(defcaixa
  :nome "feira"
  :versao "0.1.0"
  :kind Binario
  :exe ("exe/feira"))
"#;
    let c = Caixa::from_lisp(src).expect("parse binario");
    assert_eq!(c.kind, CaixaKind::Binario);
    assert_eq!(c.exe, vec!["exe/feira".to_string()]);
}

#[test]
fn servico_kind_parses() {
    let src = r#"
(defcaixa
  :nome "watcher"
  :versao "0.1.0"
  :kind Servico
  :servicos ("servicos/watcher.lisp"))
"#;
    let c = Caixa::from_lisp(src).expect("parse servico");
    assert_eq!(c.kind, CaixaKind::Servico);
    assert_eq!(c.servicos, vec!["servicos/watcher.lisp".to_string()]);
}

#[test]
fn layout_validates_parsed_biblioteca() {
    let src = r#"
(defcaixa
  :nome "demo"
  :versao "0.1.0"
  :kind Biblioteca
  :bibliotecas ("lib/demo.lisp"))
"#;
    let c = Caixa::from_lisp(src).unwrap();
    let root = PathBuf::from("/virtual");
    let manifest = root.join("caixa.lisp");
    let lib = root.join("lib/demo.lisp");
    let layout = StandardLayout::new().with_path_exists(move |p| p == manifest || p == lib);
    layout.verify(&c, &root).expect("layout passes");
}

#[test]
fn layout_rejects_binario_without_exe_entry() {
    let src = r#"
(defcaixa
  :nome "demo-bin"
  :versao "0.1.0"
  :kind Binario)
"#;
    let c = Caixa::from_lisp(src).unwrap();
    let layout = StandardLayout::new().with_path_exists(|_| true);
    let err = layout.verify(&c, Path::new("/virtual")).unwrap_err();
    assert!(format!("{err}").contains("no :exe entries"));
}
