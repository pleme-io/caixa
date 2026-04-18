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
    // Direct deps from the root caixa.
    let mut queue: VecDeque<(Dep, String)> = VecDeque::new();
    let mut seen: HashSet<String> = HashSet::new();
    for dep in &root.deps {
        queue.push_back((dep.clone(), root.nome.clone()));
    }
    if cfg.include_dev {
        for dep in &root.deps_dev {
            queue.push_back((dep.clone(), root.nome.clone()));
        }
    }

    // Resolved entries keyed by nome, preserving deterministic deps_diretas.
    let mut resolved: BTreeMap<String, ResolvedDep> = BTreeMap::new();

    while let Some((dep, from)) = queue.pop_front() {
        if !seen.insert(dep.nome.clone()) {
            continue;
        }
        let fetched = fetch_dep(&dep, cfg, cache).map_err(|e| match e {
            ResolveError::Cycle(_) => ResolveError::Cycle(from.clone()),
            other => other,
        })?;
        for t in &fetched.child_deps {
            queue.push_back((t.clone(), dep.nome.clone()));
        }
        resolved.insert(
            dep.nome.clone(),
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
                .map(|c| fechamento.get(&c.nome).cloned())
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
            nome: r.dep.nome.clone(),
            versao: r.concrete_versao.clone(),
            fonte: r.resolved_fonte.clone(),
            conteudo: r.conteudo.clone(),
            fechamento: fechamento
                .get(&r.dep.nome)
                .cloned()
                .unwrap_or_else(|| hash_bytes(b"unresolved")),
            deps_diretas: r.child_deps.iter().map(|c| c.nome.clone()).collect(),
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
            repo: format!("{host}:{org}/{}", dep.nome),
            tag: None,
            rev: None,
            branch: None,
        }
    });

    match &fonte {
        DepSource::Path { caminho } => fetch_path(dep, caminho),
        DepSource::Git {
            repo,
            tag,
            rev,
            branch,
        } => fetch_git(
            dep,
            repo,
            tag.as_deref(),
            rev.as_deref(),
            branch.as_deref(),
            cache,
            fonte.clone(),
        ),
    }
}

fn fetch_path(dep: &Dep, caminho: &str) -> Result<FetchedDep, ResolveError> {
    let path = PathBuf::from(caminho);
    if !path.exists() {
        return Err(ResolveError::MissingPath {
            nome: dep.nome.clone(),
            path,
        });
    }
    let manifest = std::fs::read_to_string(path.join("caixa.lisp"))?;
    let target = Caixa::from_lisp(&manifest)?;
    Ok(FetchedDep {
        child_deps: target.deps.clone(),
        resolved_fonte: DepSource::Path {
            caminho: caminho.to_string(),
        },
        concrete_versao: target.versao.clone(),
        conteudo: format!("path:{caminho}"),
    })
}

fn fetch_git(
    dep: &Dep,
    repo: &str,
    tag: Option<&str>,
    rev: Option<&str>,
    branch: Option<&str>,
    cache: &CacheDir,
    original_fonte: DepSource,
) -> Result<FetchedDep, ResolveError> {
    let gitref = rev
        .or(tag)
        .or(branch)
        .ok_or_else(|| ResolveError::MissingPin {
            nome: dep.nome.clone(),
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
        child_deps: target.deps.clone(),
        resolved_fonte: resolved,
        concrete_versao: target.versao.clone(),
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

    #[test]
    fn split_default_host_parses_github() {
        assert_eq!(
            split_default_host("github:pleme-io"),
            ("github", "pleme-io")
        );
    }
}
