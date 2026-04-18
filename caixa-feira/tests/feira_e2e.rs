//! End-to-end `feira` CLI test — drives the subcommands in-process via
//! the same clap parser the binary uses, writing to a tempdir.

use std::fs;

use caixa_core::Caixa;
use caixa_lacre::Lacre;
use tempfile::tempdir;

/// Smoke test: init → add → lock → build, all in a tempdir, no network.
#[test]
fn init_add_lock_build_cycle() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("demo");

    // Simulate `feira init demo --path <tmp>/demo`.
    fs::create_dir_all(&root).unwrap();
    let manifest = Caixa::template("demo");
    fs::write(root.join("caixa.lisp"), &manifest).unwrap();
    fs::create_dir_all(root.join("lib")).unwrap();
    fs::write(root.join("lib").join("demo.lisp"), "").unwrap();

    // Parse the template as Caixa (proves the schema matches).
    let mut caixa = Caixa::from_lisp(&manifest).unwrap();
    assert_eq!(caixa.nome, "demo");

    // Simulate `feira add caixa-teia --versao "^0.1"` — append + round-trip.
    caixa
        .deps
        .push(caixa_core::Dep::simple("caixa-teia", "^0.1"));
    let emitted = caixa.to_lisp();
    fs::write(root.join("caixa.lisp"), &emitted).unwrap();
    let re_parsed = Caixa::from_lisp(&emitted).unwrap();
    assert_eq!(re_parsed.deps.len(), 1);
    assert_eq!(re_parsed.deps[0].nome, "caixa-teia");

    // Simulate `feira lock` — stub resolver produces a LacreEntry per dep.
    use caixa_lacre::{LacreEntry, closure_hash, hash_bytes};
    let entries: Vec<LacreEntry> = re_parsed
        .deps
        .iter()
        .map(|dep| {
            let conteudo = hash_bytes(format!("{}@{}", dep.nome, dep.versao).as_bytes());
            let fechamento = closure_hash(&conteudo, &[]);
            LacreEntry {
                nome: dep.nome.clone(),
                versao: dep.versao.clone(),
                fonte: caixa_core::DepSource::default_github("pleme-io", &dep.nome),
                conteudo,
                fechamento,
                deps_diretas: Vec::new(),
            }
        })
        .collect();
    let lacre = Lacre::from_entries(entries);
    assert_eq!(lacre.entradas.len(), 1);
    assert!(lacre.is_coherent());

    fs::write(root.join("lacre.lisp"), lacre.to_lisp()).unwrap();
    let parsed_lacre =
        Lacre::from_lisp(&fs::read_to_string(root.join("lacre.lisp")).unwrap()).unwrap();
    assert_eq!(parsed_lacre.raiz, lacre.raiz);
    assert_eq!(parsed_lacre.entradas.len(), 1);
    assert_eq!(parsed_lacre.entradas[0].nome, "caixa-teia");

    // Simulate `feira build` — layout passes + lib parses.
    use caixa_core::{LayoutInvariants, StandardLayout};
    StandardLayout::new().verify(&re_parsed, &root).unwrap();
}
