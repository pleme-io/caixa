//! Top-level resolver — turn a [`Caixa`] into a [`Lacre`] with BLAKE3
//! fechamento hashes over the full transitive closure.

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use caixa_core::{Caixa, Dep, DepSource};
use caixa_lacre::{Lacre, LacreEntry, closure_hash, hash_bytes};
use thiserror::Error;

use crate::cache::CacheDir;
use crate::config::ResolverConfig;
use crate::git::{self, GitError};
use crate::url::expand_shorthand;

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("git: {0}")]
    Git(#[from] GitError),
    #[error("lisp: {0}")]
    Lisp(#[from] tatara_lisp::LispError),
    #[error("dep '{nome}' expected a pin (:tag or :rev); got neither")]
    MissingPin { nome: String },
    #[error("dep '{nome}' path source {path} does not exist")]
    MissingPath { nome: String, path: PathBuf },
    #[error("cyclic dependency detected involving '{0}'")]
    Cycle(String),
}

/// Resolve a root caixa's deps into a canonical lacre, offline if the cache
/// is warm, otherwise cloning/fetching from git.
pub fn resolve_lacre(
    root: &Caixa,
    cfg: &ResolverConfig,
    cache: &CacheDir,
) -> Result<Lacre, ResolveError> {
    // Direct deps from the root caixa. Read both the runtime `:deps`
    // and dev-only `:deps-dev` lists through the typed
    // [`Caixa::deps`] / [`Caixa::deps_dev`] `&[Dep]`-return slice
    // accessors so every projection of the outer-`Caixa`
    // dependency-slot axes in the top-level closure resolver's
    // queue-initialization surface routes through one typed
    // dispatch — any future accessor extension (a per-scope alias
    // table, per-cluster canary-version overlay, per-Aplicacao
    // dev-closure-audit projection the M4 CR materializer's
    // per-CR reconcile pass consumes) reaches both walks by
    // construction. Peer of the sibling per-`:deps` /
    // `:deps-dev` accessor family the ad34b4e / f7fd81e lifts
    // opened on the outer-`Caixa` slot, extended here onto the
    // caixa-resolver top-level closure resolver's outermost
    // queue-seeding walks.
    let mut queue: VecDeque<(Dep, String)> = VecDeque::new();
    let mut seen: HashSet<String> = HashSet::new();
    for dep in root.deps() {
        queue.push_back((dep.clone(), root.nome.clone()));
    }
    if cfg.include_dev {
        for dep in root.deps_dev() {
            queue.push_back((dep.clone(), root.nome.clone()));
        }
    }

    // Resolved entries keyed by nome, preserving deterministic deps_diretas.
    let mut resolved: BTreeMap<String, ResolvedDep> = BTreeMap::new();

    while let Some((dep, from)) = queue.pop_front() {
        if !seen.insert(dep.nome().to_string()) {
            continue;
        }
        let fetched = fetch_dep(&dep, cfg, cache).map_err(|e| match e {
            ResolveError::Cycle(_) => ResolveError::Cycle(from.clone()),
            other => other,
        })?;
        for t in &fetched.child_deps {
            queue.push_back((t.clone(), dep.nome().to_string()));
        }
        resolved.insert(
            dep.nome().to_string(),
            ResolvedDep {
                dep,
                child_deps: fetched.child_deps,
                resolved_fonte: fetched.resolved_fonte,
                concrete_versao: fetched.concrete_versao,
                conteudo: fetched.conteudo,
            },
        );
    }

    // Compute closure hashes in reverse-topological order.
    let mut fechamento: BTreeMap<String, String> = BTreeMap::new();
    // Simple fixpoint: re-run until all are hashable (acyclic → terminates).
    let names: Vec<_> = resolved.keys().cloned().collect();
    for _ in 0..names.len() {
        let mut all_done = true;
        for name in &names {
            if fechamento.contains_key(name) {
                continue;
            }
            let r = &resolved[name];
            let child_closures: Option<Vec<String>> = r
                .child_deps
                .iter()
                .map(|c| fechamento.get(c.nome()).cloned())
                .collect();
            if let Some(closures) = child_closures {
                fechamento.insert(name.clone(), closure_hash(&r.conteudo, &closures));
            } else {
                all_done = false;
            }
        }
        if all_done {
            break;
        }
    }

    // Build entries in sorted-name order.
    let entries: Vec<LacreEntry> = resolved
        .values()
        .map(|r| LacreEntry {
            nome: r.dep.nome().to_string(),
            versao: r.concrete_versao.clone(),
            fonte: r.resolved_fonte.clone(),
            conteudo: r.conteudo.clone(),
            fechamento: fechamento
                .get(r.dep.nome())
                .cloned()
                .unwrap_or_else(|| hash_bytes(b"unresolved")),
            deps_diretas: r.child_deps.iter().map(|c| c.nome().to_string()).collect(),
        })
        .collect();

    Ok(Lacre::from_entries(entries))
}

struct ResolvedDep {
    dep: Dep,
    child_deps: Vec<Dep>,
    resolved_fonte: DepSource,
    concrete_versao: String,
    conteudo: String,
}

struct FetchedDep {
    child_deps: Vec<Dep>,
    resolved_fonte: DepSource,
    concrete_versao: String,
    conteudo: String,
}

fn fetch_dep(
    dep: &Dep,
    cfg: &ResolverConfig,
    cache: &CacheDir,
) -> Result<FetchedDep, ResolveError> {
    // Expand :fonte — None → default host shorthand.
    let fonte = dep.fonte.clone().unwrap_or_else(|| {
        let (host, org) = split_default_host(&cfg.default_host);
        DepSource::Git {
            repo: format!("{host}:{org}/{}", dep.nome()),
            tag: None,
            rev: None,
            branch: None,
        }
    });

    match &fonte {
        DepSource::Path { caminho } => fetch_path(dep, caminho),
        // Route the per-`DepSource::Git`-arm sole-set-pin projection
        // through the lifted [`DepSource::sole_pin`] substrate
        // accessor rather than passing broken-out `tag`/`rev`/`branch`
        // `Option<&str>` triples down for `fetch_git` to re-inline the
        // `rev.or(tag).or(branch)` cascade on — the two pre-lift
        // consumers of the sole-set-pin projection (this crate's
        // `fetch_git` `git checkout` target, caixa-crd's
        // `dep_into_ref` `CaixaSource.git_ref` fill) now key off
        // exactly one typed dispatch on the substrate primitive, so
        // any future rebrand on the precedence axis (a `:commit` pin
        // peer once signed-commit-verification lands, a `:ref` pin the
        // M4 operator resolves per-cluster, a promotion of the plain
        // `Option<String>` pins to a typed `GitPin` newtype) migrates
        // as a single caixa-core edit rather than a coordinated
        // rewrite of both consumer sites.
        DepSource::Git { repo, .. } => fetch_git(dep, repo, fonte.sole_pin(), cache, fonte.clone()),
    }
}

fn fetch_path(dep: &Dep, caminho: &str) -> Result<FetchedDep, ResolveError> {
    let path = PathBuf::from(caminho);
    if !path.exists() {
        return Err(ResolveError::MissingPath {
            nome: dep.nome().to_string(),
            path,
        });
    }
    let manifest = std::fs::read_to_string(path.join("caixa.lisp"))?;
    let target = Caixa::from_lisp(&manifest)?;
    Ok(FetchedDep {
        // Route the fetched child's `:deps` `Vec<Dep>`-carry projection
        // through the typed [`Caixa::deps`] `&[Dep]`-return slice
        // accessor so the transitive walk keys off the same typed
        // dispatch the outer queue-seeding read at [`resolve_lacre`]
        // already routes through — a `.to_vec()` on the accessor's
        // borrowed slice allocates exactly one `Vec<Dep>` per fetched
        // target, byte-equal in element order to the prior raw
        // `target.deps.clone()` read.
        child_deps: target.deps().to_vec(),
        resolved_fonte: DepSource::Path {
            caminho: caminho.to_string(),
        },
        // Route the fetched child's `:versao` `String`-carry projection
        // through the typed [`Caixa::versao`] `&str`-return accessor —
        // sibling to the paired `target.deps().to_vec()` accessor-route
        // above; a `.to_string()` on the accessor's borrowed `&str`
        // allocates exactly one `String` byte-equal to the prior raw
        // `target.versao.clone()` read. Closes the last unlifted per-
        // `Caixa` universal-axis raw-field-access `String`-carry site on
        // the caixa-resolver closure-walker's per-fetched-target
        // [`FetchedDep`] emit surface, sibling to the peer per-`Caixa`
        // universal-axis converges every peer per-kind renderer (caixa-
        // helm eb912de, caixa-flux 2fc5f81, caixa-mesh 980c059, caixa-
        // tatara e73b19f, caixa-crd 41ab9a3) already routes its `:versao`
        // `String`-carry through.
        concrete_versao: target.versao().to_string(),
        conteudo: format!("path:{caminho}"),
    })
}

fn fetch_git(
    dep: &Dep,
    repo: &str,
    sole_pin: Option<&str>,
    cache: &CacheDir,
    original_fonte: DepSource,
) -> Result<FetchedDep, ResolveError> {
    let gitref = sole_pin.ok_or_else(|| ResolveError::MissingPin {
        nome: dep.nome().to_string(),
    })?;
    let full_url = expand_shorthand(repo);
    let key_bytes = format!("{full_url}#{gitref}");
    let key = hash_bytes(key_bytes.as_bytes());
    let short = &key["blake3:".len()..][..16];
    let dest = cache.source_dir(short);

    git::clone_or_fetch(&full_url, &dest)?;
    git::checkout(&dest, gitref)?;
    let sha = git::head_sha(&dest)?;
    let conteudo = format!("git:{sha}");

    let manifest_path = dest.join("caixa.lisp");
    let manifest = std::fs::read_to_string(&manifest_path)?;
    let target = Caixa::from_lisp(&manifest)?;

    // Freeze :fonte into the lacre with the resolved commit — lock files
    // are reproducible even if the upstream moves the tag.
    let resolved = match original_fonte {
        DepSource::Git {
            repo: r,
            tag: t,
            branch: b,
            ..
        } => DepSource::Git {
            repo: r,
            tag: t,
            rev: Some(sha),
            branch: b,
        },
        other => other,
    };

    Ok(FetchedDep {
        // Same accessor-route as the sibling [`fetch_path`] arm — the
        // git-fetched child's `:deps` `Vec<Dep>`-carry projection
        // reaches for the typed [`Caixa::deps`] `&[Dep]`-return slice
        // accessor so both fetch-arm branches of the transitive walk
        // key off the same typed dispatch as the outer
        // [`resolve_lacre`] queue-seeding read.
        child_deps: target.deps().to_vec(),
        resolved_fonte: resolved,
        // Sibling `:versao` `String`-carry converge to the paired
        // [`fetch_path`] arm above — the git-fetched child's `:versao`
        // scalar routes through the typed [`Caixa::versao`] `&str`-
        // return accessor rather than the raw `.versao.clone()` field
        // access, so both fetch-arm branches of the closure walker land
        // the concrete-versao carry through the same typed dispatch.
        concrete_versao: target.versao().to_string(),
        conteudo,
    })
}

/// Split `"github:pleme-io"` → `("github", "pleme-io")`. Unrecognized hosts
/// return `("github", default_host_as_is)`.
fn split_default_host(default_host: &str) -> (&str, &str) {
    default_host
        .split_once(':')
        .unwrap_or(("github", default_host))
}

#[allow(dead_code)]
fn _unused_path(_p: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use caixa_core::CaixaKind;
    use tempfile::tempdir;

    #[test]
    fn split_default_host_parses_github() {
        assert_eq!(
            split_default_host("github:pleme-io"),
            ("github", "pleme-io")
        );
    }

    /// Pin that every projection of the outer-`Caixa` `:deps` /
    /// `:deps-dev` dependency-slot family in [`resolve_lacre`] and the
    /// per-target [`fetch_path`] transitive walk — the outer
    /// queue-seeding walks over `root.deps()` and `root.deps_dev()`
    /// (the two entry points every downstream git-clone target flows
    /// through), plus the per-fetched-target `target.deps().to_vec()`
    /// child-dep collection inside [`fetch_path`] the transitive walk
    /// keys off — routes through the typed [`caixa_core::Caixa::deps`]
    /// / [`caixa_core::Caixa::deps_dev`] `&[Dep]`-return slice
    /// accessors rather than raw `&root.deps` / `&root.deps_dev` /
    /// `target.deps.clone()` field reads. Byte-equal today (each
    /// accessor returns `self.<field>.as_slice()`); catches any future
    /// emit-site regression that reintroduces a raw field read, and
    /// pins the closure-walker's four-site accessor-routing against a
    /// hermetic tempdir-hosted `defcaixa`-parsed three-caixa closure
    /// (root → child → grandchild via `:fonte (:tipo path :caminho …)`
    /// on each edge, plus a dev-only sibling under `:deps-dev` that
    /// the closure walker admits only when `cfg.include_dev` is true).
    /// Peer of the sibling caixa-crd's `dep_into_ref_routes_through_dep_accessors`
    /// (d65d1bf) per-entry `:deps` sub-slot family pin on the paired
    /// per-`Dep` `Option<&DepSource>` composite-reference axis — same
    /// "the emit path must route through the substrate-primitive
    /// typed dispatch" discipline extended onto the outer top-level
    /// [`Caixa`] `&[Dep]` slice axes at the closure-resolver's
    /// queue-seeding surface.
    #[test]
    #[allow(clippy::too_many_lines)]
    fn resolve_lacre_routes_dep_slot_family_through_caixa_accessors() {
        let td = tempdir().expect("tempdir");
        // Grandchild caixa on disk — no deps.
        let grandchild_path = td.path().join("grandchild");
        std::fs::create_dir_all(&grandchild_path).unwrap();
        std::fs::write(
            grandchild_path.join("caixa.lisp"),
            r#"(defcaixa
                  :nome "grandchild"
                  :versao "0.1.0"
                  :kind Biblioteca
                  :bibliotecas ("lib/grandchild.lisp"))"#,
        )
        .unwrap();

        // Child caixa on disk — depends on grandchild via Path.
        let child_path = td.path().join("child");
        std::fs::create_dir_all(&child_path).unwrap();
        let child_lisp = format!(
            r#"(defcaixa
                  :nome "child"
                  :versao "0.1.0"
                  :kind Biblioteca
                  :bibliotecas ("lib/child.lisp")
                  :deps ((:nome "grandchild" :versao "0.1.0"
                          :fonte (:tipo path :caminho "{}"))))"#,
            grandchild_path.display()
        );
        std::fs::write(child_path.join("caixa.lisp"), child_lisp).unwrap();

        // Dev-only sibling — no deps of its own.
        let devchild_path = td.path().join("devchild");
        std::fs::create_dir_all(&devchild_path).unwrap();
        std::fs::write(
            devchild_path.join("caixa.lisp"),
            r#"(defcaixa
                  :nome "devchild"
                  :versao "0.1.0"
                  :kind Biblioteca
                  :bibliotecas ("lib/devchild.lisp"))"#,
        )
        .unwrap();

        let root = Caixa {
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
                nome: "child".into(),
                versao: "0.1.0".into(),
                fonte: Some(DepSource::Path {
                    caminho: child_path.to_string_lossy().into_owned(),
                }),
                opcional: false,
                caracteristicas: vec![],
            }],
            deps_dev: vec![Dep {
                nome: "devchild".into(),
                versao: "0.1.0".into(),
                fonte: Some(DepSource::Path {
                    caminho: devchild_path.to_string_lossy().into_owned(),
                }),
                opcional: false,
                caracteristicas: vec![],
            }],
            exe: vec![],
            bibliotecas: vec![],
            servicos: vec![],
            limits: None,
            behavior: None,
            upgrade_from: vec![],
            estrategia: None,
            max_restarts: None,
            restart_window: None,
            children: vec![],
            membros: vec![],
            contratos: vec![],
            politicas: None,
            placement: None,
            entrada: None,
            ci: None,
        };

        let cache_root = td.path().join("cache");
        std::fs::create_dir_all(&cache_root).unwrap();
        let cache = CacheDir::at(&cache_root);

        // include_dev=false: the closure-walker admits only `:deps`
        // entries + their transitives. `:deps-dev` sibling must NOT
        // appear, and the walker must have iterated `root.deps()`
        // (surfacing `"child"`) and then `child.deps().to_vec()` inside
        // `fetch_path` (surfacing `"grandchild"`).
        let cfg = ResolverConfig::default();
        let lacre = resolve_lacre(&root, &cfg, &cache).expect("resolve runtime-only closure");
        let names: Vec<&str> = lacre.entradas.iter().map(|e| e.nome.as_str()).collect();
        assert!(
            names.contains(&"child"),
            "resolve_lacre must surface `child` via root.deps() — got {names:?}"
        );
        assert!(
            names.contains(&"grandchild"),
            "resolve_lacre must surface `grandchild` via the transitive \
             fetch_path target.deps().to_vec() walk — got {names:?}"
        );
        assert!(
            !names.contains(&"devchild"),
            "resolve_lacre must NOT surface `devchild` when \
             include_dev=false (deps_dev accessor must be walked only \
             under the include_dev cfg arm) — got {names:?}"
        );

        // include_dev=true: the closure-walker also admits `:deps-dev`
        // entries. The `devchild` sibling must appear via
        // `root.deps_dev()`.
        let cfg_with_dev = ResolverConfig {
            include_dev: true,
            ..Default::default()
        };
        let lacre_with_dev =
            resolve_lacre(&root, &cfg_with_dev, &cache).expect("resolve with dev closure");
        let names_with_dev: Vec<&str> = lacre_with_dev
            .entradas
            .iter()
            .map(|e| e.nome.as_str())
            .collect();
        assert!(
            names_with_dev.contains(&"child"),
            "include_dev=true closure must still surface `child` via \
             root.deps() — got {names_with_dev:?}"
        );
        assert!(
            names_with_dev.contains(&"grandchild"),
            "include_dev=true closure must still surface `grandchild` \
             via the transitive walk — got {names_with_dev:?}"
        );
        assert!(
            names_with_dev.contains(&"devchild"),
            "include_dev=true closure must surface `devchild` via \
             root.deps_dev() — got {names_with_dev:?}"
        );
    }

    /// Pin that both fetch-arm branches of the closure walker's per-
    /// fetched-target [`FetchedDep`] emit surface — the [`fetch_path`]
    /// arm and the [`fetch_git`] arm — carry each target's `:versao`
    /// scalar into the resulting [`crate::LacreEntry`] via the typed
    /// [`caixa_core::Caixa::versao`] `&str`-return accessor rather than a
    /// raw `target.versao.clone()` field access. Byte-equal today
    /// (`Caixa::versao` is `&self.versao`); catches any future emit-side
    /// regression that reintroduces a raw field read and pins the two
    /// [`FetchedDep::concrete_versao`] construction sites against a
    /// hermetic tempdir-hosted three-caixa closure whose `:versao` bytes
    /// distinguish each layer (root `"0.9.0"` → child `"0.5.2"` →
    /// grandchild `"0.1.3"`), so a stub-that-hardcodes-a-fixed-string
    /// regression trips on any layer.
    ///
    /// Peer of the sibling [`resolve_lacre_routes_dep_slot_family_through_caixa_accessors`]
    /// per-`Caixa` `:deps` / `:deps-dev` slot-family pin on the sibling
    /// per-`Caixa` `&[Dep]` slice-return accessor axis — same "the
    /// emit path must route through the substrate-primitive typed
    /// dispatch" discipline extended onto the outer top-level [`Caixa`]
    /// `:versao` `&str`-return universal-axis at the closure-resolver's
    /// per-fetched-target [`FetchedDep`] emit surface. Sibling to the
    /// peer per-`Caixa` universal-axis converges every peer per-kind
    /// renderer (caixa-helm eb912de, caixa-flux 2fc5f81, caixa-mesh
    /// 980c059, caixa-tatara e73b19f, caixa-crd 41ab9a3) already routes
    /// its `:versao` `String`-carry through.
    #[test]
    fn resolve_lacre_fetched_target_versao_routes_through_caixa_versao_accessor() {
        let td = tempdir().expect("tempdir");
        // Grandchild caixa on disk — `:versao "0.1.3"`.
        let grandchild_path = td.path().join("grandchild");
        std::fs::create_dir_all(&grandchild_path).unwrap();
        std::fs::write(
            grandchild_path.join("caixa.lisp"),
            r#"(defcaixa
                  :nome "grandchild"
                  :versao "0.1.3"
                  :kind Biblioteca
                  :bibliotecas ("lib/grandchild.lisp"))"#,
        )
        .unwrap();

        // Child caixa on disk — `:versao "0.5.2"`, depends on grandchild
        // via Path so the closure walker traverses through
        // [`fetch_path`] on both edges.
        let child_path = td.path().join("child");
        std::fs::create_dir_all(&child_path).unwrap();
        let child_lisp = format!(
            r#"(defcaixa
                  :nome "child"
                  :versao "0.5.2"
                  :kind Biblioteca
                  :bibliotecas ("lib/child.lisp")
                  :deps ((:nome "grandchild" :versao "0.1.3"
                          :fonte (:tipo path :caminho "{}"))))"#,
            grandchild_path.display()
        );
        std::fs::write(child_path.join("caixa.lisp"), child_lisp).unwrap();

        let root = Caixa {
            nome: "root".into(),
            versao: "0.9.0".into(),
            kind: CaixaKind::Biblioteca,
            edicao: None,
            descricao: None,
            repositorio: None,
            licenca: None,
            autores: vec![],
            etiquetas: vec![],
            deps: vec![Dep {
                nome: "child".into(),
                versao: "0.5.2".into(),
                fonte: Some(DepSource::Path {
                    caminho: child_path.to_string_lossy().into_owned(),
                }),
                opcional: false,
                caracteristicas: vec![],
            }],
            deps_dev: vec![],
            exe: vec![],
            bibliotecas: vec![],
            servicos: vec![],
            limits: None,
            behavior: None,
            upgrade_from: vec![],
            estrategia: None,
            max_restarts: None,
            restart_window: None,
            children: vec![],
            membros: vec![],
            contratos: vec![],
            politicas: None,
            placement: None,
            entrada: None,
            ci: None,
        };

        let cache_root = td.path().join("cache");
        std::fs::create_dir_all(&cache_root).unwrap();
        let cache = CacheDir::at(&cache_root);
        let cfg = ResolverConfig::default();
        let lacre = resolve_lacre(&root, &cfg, &cache).expect("resolve closure");

        // Every fetched target's `:versao` scalar must land in the
        // resulting `LacreEntry.versao` byte-verbatim to what
        // `Caixa::versao()` returns for the source caixa on disk. A
        // regression that hardcoded a fixed string on either fetch-arm
        // side of the closure walker trips on the layer whose byte-shape
        // it drifted off.
        let pairs: Vec<(&str, &str)> = lacre
            .entradas
            .iter()
            .map(|e| (e.nome.as_str(), e.versao.as_str()))
            .collect();
        assert!(
            pairs.contains(&("child", "0.5.2")),
            "child entry must carry `:versao \"0.5.2\"` verbatim through \
             fetch_path -> Caixa::versao() -> FetchedDep::concrete_versao \
             -> LacreEntry.versao — got {pairs:?}"
        );
        assert!(
            pairs.contains(&("grandchild", "0.1.3")),
            "grandchild entry must carry `:versao \"0.1.3\"` verbatim \
             through the transitive walk's Caixa::versao() accessor route \
             — got {pairs:?}"
        );
    }
}
