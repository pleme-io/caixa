//! Integration test — drive the real resolver against a local bare-git
//! remote. Verifies the full `resolve_lacre` flow: clone → checkout → hash
//! → parse child caixa → transitive walk.
//!
//! No network is used; the remote is an empty temp directory initialized
//! with `git init --bare` and pushed to from a nearby worktree.

use std::path::Path;
use std::process::Command;

use caixa_core::{Caixa, CaixaKind, Dep, DepSource};
use caixa_lacre::Lacre;
use caixa_resolver::{CacheDir, ResolverConfig, resolve_lacre};
use tempfile::tempdir;

fn git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git_out(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn make_local_remote(name: &str, caixa_src: &str, tmp: &Path) -> String {
    // Bare remote.
    let bare = tmp.join(format!("{name}.git"));
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "--bare", "--quiet"]);

    // Worktree that pushes to it.
    let work = tmp.join(format!("{name}-work"));
    std::fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "--quiet"]);
    git(&work, &["config", "user.email", "test@example.com"]);
    git(&work, &["config", "user.name", "test"]);
    git(&work, &["checkout", "-b", "main"]);
    std::fs::write(work.join("caixa.lisp"), caixa_src).unwrap();
    git(&work, &["add", "caixa.lisp"]);
    git(&work, &["commit", "-q", "-m", "initial"]);
    git(&work, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(&work, &["push", "-q", "origin", "main"]);
    git(&work, &["tag", "v0.1.0"]);
    git(&work, &["push", "-q", "origin", "v0.1.0"]);

    format!("file://{}", bare.display())
}

#[test]
fn resolve_single_dep_against_local_bare_remote() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Upstream caixa, no deps of its own.
    let upstream_src = r#"(defcaixa :nome "upstream" :versao "0.1.0" :kind Biblioteca)"#;
    let upstream_url = make_local_remote("upstream", upstream_src, root);

    // Consumer caixa — hand-built struct so we avoid generating caixa.lisp twice.
    let consumer = Caixa {
        nome: "consumer".into(),
        versao: "0.1.0".into(),
        kind: CaixaKind::Biblioteca,
        edicao: None,
        descricao: None,
        repositorio: None,
        licenca: None,
        autores: vec![],
        etiquetas: vec![],
        deps: vec![Dep {
            nome: "upstream".into(),
            versao: "^0.1".into(),
            fonte: Some(DepSource::Git {
                repo: upstream_url,
                tag: Some("v0.1.0".into()),
                rev: None,
                branch: None,
            }),
            opcional: false,
            caracteristicas: vec![],
        }],
        deps_dev: vec![],
        exe: vec![],
        bibliotecas: vec![],
        servicos: vec![],
    };

    let cache = CacheDir::at(root.join("cache"));
    std::fs::create_dir_all(cache.root()).unwrap();
    let cfg = ResolverConfig::default();

    let lacre = resolve_lacre(&consumer, &cfg, &cache).expect("resolve must succeed");
    assert_eq!(lacre.entradas.len(), 1);
    let entry = &lacre.entradas[0];
    assert_eq!(entry.nome, "upstream");
    assert_eq!(entry.versao, "0.1.0");
    assert!(entry.conteudo.starts_with("git:"));
    assert!(entry.fechamento.starts_with("blake3:"));
    assert!(lacre.is_coherent(), "lacre root should match recomputation");

    // Re-running must hit cache and produce identical root hash.
    let lacre2 = resolve_lacre(&consumer, &cfg, &cache).unwrap();
    assert_eq!(lacre.raiz, lacre2.raiz, "resolution must be deterministic");
}

#[test]
fn resolve_transitive_two_level() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Leaf.
    let leaf_src = r#"(defcaixa :nome "leaf" :versao "0.1.0" :kind Biblioteca)"#;
    let leaf_url = make_local_remote("leaf", leaf_src, root);

    // Middle depends on leaf.
    let middle_src = format!(
        r#"(defcaixa :nome "middle" :versao "0.1.0" :kind Biblioteca
  :deps ((:nome "leaf" :versao "*"
          :fonte (:tipo git :repo {leaf_url:?} :tag "v0.1.0"))))"#
    );
    let middle_url = make_local_remote("middle", &middle_src, root);

    // Root depends on middle.
    let root_caixa = Caixa {
        nome: "root".into(),
        versao: "0.1.0".into(),
        kind: CaixaKind::Biblioteca,
        edicao: None,
        descricao: None,
        repositorio: None,
        licenca: None,
        autores: vec![],
        etiquetas: vec![],
        deps: vec![Dep {
            nome: "middle".into(),
            versao: "*".into(),
            fonte: Some(DepSource::Git {
                repo: middle_url,
                tag: Some("v0.1.0".into()),
                rev: None,
                branch: None,
            }),
            opcional: false,
            caracteristicas: vec![],
        }],
        deps_dev: vec![],
        exe: vec![],
        bibliotecas: vec![],
        servicos: vec![],
    };

    let cache = CacheDir::at(root.join("cache"));
    std::fs::create_dir_all(cache.root()).unwrap();
    let lacre = resolve_lacre(&root_caixa, &ResolverConfig::default(), &cache).unwrap();

    // Should contain both middle and leaf.
    let names: Vec<_> = lacre.entradas.iter().map(|e| e.nome.as_str()).collect();
    assert!(names.contains(&"middle"));
    assert!(names.contains(&"leaf"));
    assert!(lacre.is_coherent());

    // `middle` should list `leaf` as a direct dep.
    let middle_entry = lacre.entradas.iter().find(|e| e.nome == "middle").unwrap();
    assert_eq!(middle_entry.deps_diretas, vec!["leaf".to_string()]);
}

#[test]
fn lacre_round_trips_through_lisp_after_resolution() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    let upstream_src = r#"(defcaixa :nome "upstream" :versao "0.1.0" :kind Biblioteca)"#;
    let url = make_local_remote("upstream", upstream_src, root);

    let consumer = Caixa {
        nome: "consumer".into(),
        versao: "0.1.0".into(),
        kind: CaixaKind::Biblioteca,
        edicao: None,
        descricao: None,
        repositorio: None,
        licenca: None,
        autores: vec![],
        etiquetas: vec![],
        deps: vec![Dep {
            nome: "upstream".into(),
            versao: "*".into(),
            fonte: Some(DepSource::Git {
                repo: url,
                tag: Some("v0.1.0".into()),
                rev: None,
                branch: None,
            }),
            opcional: false,
            caracteristicas: vec![],
        }],
        deps_dev: vec![],
        exe: vec![],
        bibliotecas: vec![],
        servicos: vec![],
    };

    let cache = CacheDir::at(root.join("cache"));
    std::fs::create_dir_all(cache.root()).unwrap();
    let lacre = resolve_lacre(&consumer, &ResolverConfig::default(), &cache).unwrap();
    let src = lacre.to_lisp();
    let reparsed = Lacre::from_lisp(&src).expect("lacre round-trips through Lisp");
    assert_eq!(reparsed.raiz, lacre.raiz);
    assert_eq!(reparsed.entradas.len(), lacre.entradas.len());
}

fn _unused_sha_check() -> String {
    // Used to keep the `git_out` helper reachable from cfg(test) gates.
    let tmp = tempdir().unwrap();
    git(tmp.path(), &["init", "--quiet"]);
    git_out(tmp.path(), &["rev-parse", "HEAD"])
}
