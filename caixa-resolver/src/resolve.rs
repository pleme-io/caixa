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
    /// A resolved dep's `caixa.lisp` did not read as a package manifest.
    ///
    /// Distinct from [`Self::Lisp`] because the interesting case is
    /// `LeituraError::DialetoEstrangeiro`: a dep whose manifest is a
    /// repo-surface declaration rather than a package manifest is not a
    /// syntax error in that dep, it is the wrong KIND of file, and a resolver
    /// that flattened both into "lisp: …" would send the author hunting for a
    /// typo that is not there.
    #[error("manifest: {0}")]
    Manifesto(#[from] caixa_core::LeituraError),
    #[error("dep '{nome}' expected a pin (:tag or :rev); got neither")]
    MissingPin { nome: String },
    #[error("dep '{nome}' path source {path} does not exist")]
    MissingPath { nome: String, path: PathBuf },
    #[error("cyclic dependency detected involving '{0}'")]
    Cycle(String),
}

impl ResolveError {
    /// Substrate primitive constructor for the per-`Dep`
    /// `:fonte (:tipo path :caminho …)` on-disk-absence refusal
    /// diagnostic. Folds the pre-lift open-coded four-line
    /// `Err(ResolveError::MissingPath { nome: dep.nome().to_string(),
    /// path })` two-slot struct-literal inside [`fetch_path`] onto one
    /// dispatch. Peer with the sibling one-slot `MissingPin` `{ nome:
    /// String }` variant on the same envelope, which stays open-coded
    /// at [`fetch_git`]'s `.ok_or_else` closure arm pending its own
    /// dedicated lift. Every future consumer that surfaces the same
    /// on-disk-absence diagnostic outside `fetch_path` — a deferred
    /// `feira lock --path`-only pre-flight probe, a per-lacre overlay
    /// resolver rejecting a cluster-local `:caminho` overlay that
    /// vanished between resolve passes, the M4 `mesh.pleme.io/v1alpha1/
    /// Caixa` CR admission webhook re-checking a per-`:fonte`-patched
    /// candidate before the closure walker re-fires — reaches the
    /// variant through one call rather than re-inlining the two-slot
    /// struct-literal in lockstep with [`fetch_path`]'s wire-up.
    ///
    /// The `nome: &str` parameter accepts `&str` literals and `&String`
    /// via Deref coercion so the sole in-crate wire-up threads
    /// `dep.nome()` (a `&str` accessor via [`Dep::nome`]) through the
    /// ctor without a pre-conversion; the `path: impl Into<PathBuf>`
    /// parameter takes the observed on-disk-absent [`PathBuf`] built
    /// at the caller from `PathBuf::from(caminho)` and forwards it
    /// through the [`From<PathBuf>`] identity into the variant slot
    /// without an extra allocation.
    #[must_use]
    pub fn missing_path(nome: &str, path: impl Into<PathBuf>) -> ResolveError {
        ResolveError::MissingPath {
            nome: nome.to_string(),
            path: path.into(),
        }
    }
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
    //
    // The paired parent-`:nome` label carried on the queue's `from`
    // arm — the byte-string the cycle-diagnostic re-labeler's
    // `ResolveError::Cycle(from.clone())` map_err arm surfaces on any
    // future extension that promotes the closure walker's per-target
    // cycle-collapse into a first-class refusal — routes through the
    // typed [`caixa_core::Caixa::nome`] `&str`-return accessor rather
    // than the raw `root.nome.clone()` `String`-carry read. Byte-equal
    // today (`Caixa::nome` is `&self.nome`; `.to_string()` on the
    // borrowed `&str` allocates exactly one `String` byte-equal to the
    // prior raw `.nome.clone()` read); the queue's per-transitive-target
    // sibling push at line 77 already routes through the peer
    // [`caixa_core::Dep::nome`] accessor (`dep.nome().to_string()`), so
    // this converge closes the last unlifted parent-`:nome` field-read
    // in the closure walker's queue-carried `from` axis. Peer with every
    // prior per-`Caixa` universal-axis converge on the outer-`Caixa`
    // `:nome` scalar (caixa-helm 22461ef / a7420bd, caixa-flux 4a363bf /
    // 162e2e2, caixa-mesh 54bf2f3 / 980c059, caixa-crd 61d3429,
    // caixa-tatara e73b19f, caixa-feira 5131203 / 4b05240 / 3219a42 /
    // 5bc5178) and sibling to 5ce1b94's `Caixa::deps` / `Caixa::deps_dev`
    // converge in this same crate on the paired outer-`Caixa`
    // dependency-slot axis.
    let mut queue: VecDeque<(Dep, String)> = VecDeque::new();
    let mut seen: HashSet<String> = HashSet::new();
    for dep in root.deps() {
        queue.push_back((dep.clone(), root.nome().to_string()));
    }
    if cfg.include_dev {
        for dep in root.deps_dev() {
            queue.push_back((dep.clone(), root.nome().to_string()));
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
    //
    // Route the per-`Dep` `:fonte` presence-projection through the
    // lifted [`expand_fonte`] helper rather than an inline
    // `dep.fonte.clone().unwrap_or_else(...)` cascade. The helper reads
    // through the typed [`caixa_core::Dep::fonte`] `Option<&DepSource>`
    // accessor and materializes the `github:<org>/<nome>`-shaped
    // `default_host`-derived fallback for the author-omitted arm,
    // keeping the closure-walker's two per-`Dep` `:fonte`-expansion
    // consumers (this crate's [`fetch_dep`], caixa-feira/src/cmd/lock.rs's
    // `resolve_stub` — 33e5d9a converged the peer site onto the same
    // accessor) both routed through the substrate-primitive typed
    // dispatch rather than each open-coding its own `.clone()` cascade.
    let fonte = expand_fonte(dep, &cfg.default_host);

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
        return Err(ResolveError::missing_path(dep.nome(), path));
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

/// Per-`Dep` `:fonte` presence-projection with the resolver's
/// `default_host`-derived fallback baked in — the substrate-primitive
/// typed dispatch every caixa-resolver closure-walker per-`Dep`
/// `:fonte`-expansion consumer keys off. The helper reads through the
/// typed [`caixa_core::Dep::fonte`] `Option<&DepSource>`-return accessor
/// rather than the raw `.fonte.clone()` field-access, so any future
/// extension of the accessor's semantics (a per-scope source-override
/// table on `:fonte` the M4 CR materializer resolves at admission time,
/// a per-tenant `:fonte` rewrite overlay the roadmap acknowledges, a
/// lacre-projected concrete-source pin resolver) reaches this fallback
/// surface through exactly one caixa-core edit rather than a coordinated
/// rewrite of both open-coded expansions.
///
/// Sibling of caixa-feira/src/cmd/lock.rs's `resolve_stub`
/// (33e5d9a) per-`Dep` `:fonte` accessor-route on the peer stub-resolver
/// site — same "the emit path must route through the substrate-primitive
/// typed dispatch" discipline extended onto the caixa-resolver
/// closure-walker's per-target `:fonte`-expansion surface. Byte-equal to
/// the prior inline cascade today: on the author-set arm the accessor
/// returns `Some(&<verbatim>)` and `.cloned()` allocates exactly one
/// `DepSource` byte-equal to the pre-lift `.fonte.clone()` read; on the
/// author-omitted arm the accessor returns `None` and the fallback
/// materializes `DepSource::Git { repo: "<host>:<org>/<nome>", tag/rev/
/// branch: None }` byte-equal to the pre-lift `unwrap_or_else(...)`
/// tail. The `default_host` split cascades through
/// [`split_default_host`] so `"github:pleme-io"` → `github:pleme-io/…`
/// and `"acme-org"` → `github:acme-org/…` (the unrecognized-host arm
/// defaults `host` to `"github"`).
///
/// The fallback deliberately does NOT compose through
/// [`caixa_core::DepSource::default_github`] — that helper hardcodes
/// `"github"` as the host segment (`"github:{org}/{nome}"`), whereas
/// this helper honors an author-configurable `default_host` (e.g. a
/// `"codeberg:acme"` config would produce `codeberg:acme/<nome>`,
/// which `default_github` cannot express).
pub(crate) fn expand_fonte(dep: &Dep, default_host: &str) -> DepSource {
    dep.fonte().cloned().unwrap_or_else(|| {
        let (host, org) = split_default_host(default_host);
        DepSource::Git {
            repo: format!("{host}:{org}/{}", dep.nome()),
            tag: None,
            rev: None,
            branch: None,
        }
    })
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

    /// Pin that the per-`Dep` `:fonte` presence-projection under
    /// [`expand_fonte`] routes through the typed [`Dep::fonte`]
    /// `Option<&DepSource>`-return accessor rather than the raw
    /// `.fonte.clone()` field-access, and that the accessor's `None` arm
    /// materializes the `<host>:<org>/<nome>`-shaped `default_host`-
    /// derived fallback with `tag`/`rev`/`branch` all `None`. Sweeps
    /// three arms of the per-`Dep` `:fonte` presence bit × per-config
    /// `default_host` shape lattice the closure-walker's
    /// [`fetch_dep`] call-site keys off:
    ///
    /// 1. Author-omitted arm × the canonical `"github:pleme-io"`
    ///    default-host shape — accessor projects `None`, helper falls
    ///    back to `DepSource::Git { repo: "github:pleme-io/<nome>",
    ///    tag/rev/branch: None }`, byte-equal to the pre-lift
    ///    inline `format!("{host}:{org}/{}", dep.nome())` on the
    ///    canonical default-host arm.
    /// 2. Author-omitted arm × an unrecognized single-segment
    ///    `default_host` (`"acme-org"`) — [`split_default_host`]
    ///    defaults `host` to `"github"` and carries the whole string as
    ///    `org`, so the fallback is `DepSource::Git { repo:
    ///    "github:acme-org/<nome>", … }`. Pins the unrecognized-host
    ///    arm's fall-through under the helper.
    /// 3. Author-set arm × any `default_host` — accessor projects
    ///    `Some(&<verbatim>)`, helper carries the source through
    ///    `.cloned()` byte-verbatim regardless of `default_host` (the
    ///    fallback branch never fires when the author declared
    ///    `:fonte`).
    ///
    /// Byte-equal today (`.fonte()` returns `self.fonte.as_ref()`;
    /// `.cloned()` on `Option<&DepSource>` allocates exactly one
    /// `DepSource` byte-equal to the prior raw `.fonte.clone()` read);
    /// catches any future emit-side regression that re-introduces the
    /// raw `.fonte.clone()` cascade at the [`fetch_dep`] call site (a
    /// future accessor extension — a per-scope alias table, a per-tenant
    /// `:fonte` rewrite overlay, a lacre-projected concrete-source pin
    /// resolver — would then reach only the open-coded cascade and
    /// silently diverge from the helper's typed dispatch, tripping the
    /// author-set-arm pin below).
    ///
    /// Peer of the sibling caixa-feira/src/cmd/lock.rs's
    /// `resolve_stub_fonte_routes_through_dep_fonte_accessor` (33e5d9a)
    /// per-`Dep` `:fonte` accessor-route pin on the stub-resolver
    /// surface — same "the emit path must route through the substrate-
    /// primitive typed dispatch" discipline extended onto the
    /// caixa-resolver closure-walker's per-target `:fonte`-expansion
    /// helper.
    #[test]
    #[allow(clippy::too_many_lines)]
    fn expand_fonte_routes_through_dep_fonte_accessor() {
        // Arm 1: author-omitted `:fonte` × canonical `"github:pleme-io"`
        // default-host. Accessor must project `None` and the helper's
        // fallback must materialize the canonical
        // `github:pleme-io/<nome>` shorthand.
        let bare = Dep::simple("caixa-teia", "^0.1");
        assert!(
            bare.fonte().is_none(),
            "author-omitted :fonte must project None through the accessor \
             — pins the substrate contract the helper's unwrap_or_else \
             cascade discriminates on",
        );
        let expanded = expand_fonte(&bare, "github:pleme-io");
        assert_eq!(
            expanded,
            DepSource::Git {
                repo: "github:pleme-io/caixa-teia".to_string(),
                tag: None,
                rev: None,
                branch: None,
            },
            "author-omitted :fonte on the canonical github:pleme-io \
             default-host must fall back to the github:<org>/<nome>-shaped \
             Git shorthand byte-verbatim",
        );

        // Arm 2: author-omitted `:fonte` × unrecognized single-segment
        // `default_host` (`"acme-org"`). `split_default_host`'s
        // unrecognized-host arm defaults `host` to `"github"` and carries
        // the whole string as `org`, so the helper's fallback must land
        // `github:acme-org/<nome>` — pins that the helper reads the
        // `default_host` through the shared [`split_default_host`]
        // parser rather than re-inlining a `"github:"`-hardcoded
        // shorthand (which would produce the same output for this input
        // by coincidence but would diverge on a `"codeberg:acme"`-shaped
        // config the arm-3 assertion below stresses).
        let expanded_unrecognized_host = expand_fonte(&bare, "acme-org");
        assert_eq!(
            expanded_unrecognized_host,
            DepSource::Git {
                repo: "github:acme-org/caixa-teia".to_string(),
                tag: None,
                rev: None,
                branch: None,
            },
            "author-omitted :fonte on an unrecognized single-segment \
             default_host must default the host segment to `github` and \
             carry the whole config string as the org",
        );

        // Arm 3: author-set `:fonte` × any `default_host`. Accessor must
        // project `Some(&<verbatim>)`, and the helper must carry the
        // source through `.cloned()` byte-verbatim — the fallback branch
        // never fires when the author declared `:fonte` at authoring
        // time. Any regression that re-inlined the raw `.fonte.clone()`
        // cascade would still pass this arm today (both routes are
        // byte-equal on the `Some` arm) but a future accessor extension
        // (a per-scope alias table, a per-tenant rewrite overlay)
        // would silently diverge here — the pin catches that
        // divergence at compile time as soon as the accessor's return
        // shape widens beyond `self.fonte.as_ref()`.
        let with_path = Dep {
            nome: "child".into(),
            versao: "0.1.0".into(),
            fonte: Some(DepSource::Path {
                caminho: "/tmp/child".into(),
            }),
            opcional: false,
            caracteristicas: vec![],
        };
        assert_eq!(
            with_path.fonte(),
            Some(&DepSource::Path {
                caminho: "/tmp/child".into(),
            }),
            "author-set :fonte must project Some(&<verbatim>) through \
             the accessor — pins the substrate contract the helper's \
             .cloned() carry discriminates on",
        );
        let expanded_author_set = expand_fonte(&with_path, "github:pleme-io");
        assert_eq!(
            expanded_author_set,
            DepSource::Path {
                caminho: "/tmp/child".into(),
            },
            "author-set :fonte must carry through the helper .cloned() \
             byte-verbatim regardless of default_host — the fallback \
             branch must not fire when the accessor projects Some(&…)",
        );
        // Cross-check: swapping `default_host` on the author-set arm
        // must not perturb the output — the fallback string is dead
        // code on the `Some` arm.
        let expanded_author_set_alt_host = expand_fonte(&with_path, "codeberg:acme");
        assert_eq!(
            expanded_author_set, expanded_author_set_alt_host,
            "author-set :fonte must be default_host-agnostic — the \
             fallback shorthand must not leak into the output when the \
             accessor projects Some(&…)",
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

    /// Pin that the closure-walker's queue-seeding parent-`:nome` label
    /// axis — the `String` carried on the queue's `from` arm at both the
    /// outer `for dep in root.deps()` runtime seed and the
    /// `cfg.include_dev`-gated `for dep in root.deps_dev()` dev-only
    /// seed inside [`resolve_lacre`] — routes through the typed
    /// [`caixa_core::Caixa::nome`] `&str`-return accessor rather than a
    /// raw `root.nome.clone()` field read. The queue's per-transitive-
    /// target sibling push already routes the paired parent-`:nome`
    /// label through the peer [`caixa_core::Dep::nome`] accessor
    /// (`dep.nome().to_string()` at the transitive-walk head), so this
    /// pin closes the last unlifted parent-`:nome` field-read axis in
    /// the closure walker's queue-carried `from` surface.
    ///
    /// Byte-equal today (`Caixa::nome` is `&self.nome`; `.to_string()`
    /// on the borrowed `&str` allocates exactly one `String` byte-equal
    /// to the prior raw `.nome.clone()` read); pin catches any future
    /// silent detour that reintroduces a raw field read at either queue-
    /// seeding site. The hermetic tempdir fixture uses a byte-
    /// distinctive `:nome` on the root (`"root-abc"` — pairing DNS-1123
    /// legality with three ASCII bytes distinct from every child's
    /// `:nome` so a stub-that-hardcodes-a-fixed-string regression on
    /// either seeding site can only pass by accident), and asserts
    /// [`caixa_core::Caixa::nome`] returns byte-verbatim to what
    /// [`resolve_lacre`]'s queue-seed reads. The end-to-end assertion
    /// on the resulting [`crate::LacreEntry`] set pins that both
    /// `:deps` and `:deps-dev` walks reach their transitive targets
    /// after the accessor-route substitution, so a regression that
    /// broke the queue-seeding shape by dropping the second push arm
    /// (e.g. mis-collapsing the two guarded arms onto one) surfaces
    /// as a missing child in the resolved closure.
    ///
    /// Peer of the sibling [`resolve_lacre_routes_dep_slot_family_through_caixa_accessors`]
    /// per-`:deps` / `:deps-dev` slot-family pin above and of the
    /// [`resolve_lacre_fetched_target_versao_routes_through_caixa_versao_accessor`]
    /// per-`:versao` axis pin (0556249) on the same
    /// closure-walker emit surface — same "the emit path must route
    /// through the substrate-primitive typed dispatch" discipline the
    /// per-`Caixa` `.nome` universal-axis converges every peer per-kind
    /// renderer already carry, extended onto the outer top-level
    /// [`Caixa`] `:nome` `&str`-return universal-axis at the closure-
    /// resolver's queue-seeded per-transitive-target `from` label.
    #[test]
    fn resolve_lacre_queue_seed_parent_nome_routes_through_caixa_nome_accessor() {
        let td = tempdir().expect("tempdir");
        // Runtime-child caixa on disk — no deps of its own, distinct
        // `:nome` bytes so it can be identified in the resolved closure.
        let child_path = td.path().join("child");
        std::fs::create_dir_all(&child_path).unwrap();
        std::fs::write(
            child_path.join("caixa.lisp"),
            r#"(defcaixa
                  :nome "child"
                  :versao "0.1.0"
                  :kind Biblioteca
                  :bibliotecas ("lib/child.lisp"))"#,
        )
        .unwrap();

        // Dev-only sibling — distinct `:nome`, guards the second
        // seeding-arm branch.
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
            nome: "root-abc".into(),
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

        // Substrate-primitive byte-parity pin: `Caixa::nome()` returns
        // the raw `:nome` byte-string verbatim. Both queue-seeded push
        // arms compose `root.nome().to_string()` off this accessor, so
        // a future accessor extension (a canonicalization pass, an
        // aliasing overlay) reaches both call sites through one edit.
        assert_eq!(
            root.nome(),
            "root-abc",
            "Caixa::nome() must return the :nome byte-string verbatim — \
             pins the substrate contract both queue-seeded push arms \
             compose the parent-`:nome` label off of"
        );
        assert_eq!(
            root.nome().to_string(),
            "root-abc",
            "root.nome().to_string() must equal the raw :nome byte-string \
             verbatim — pins the accessor-routed String-carry the queue's \
             `from` arm receives at both seeding sites"
        );

        // End-to-end pin on the closure walker's two-arm seeding surface:
        // both `:deps` and `:deps-dev` walks must reach their transitive
        // targets after the accessor-route substitution. A regression
        // that dropped either arm's queue-seed push surfaces here as a
        // missing child in the resolved lacre.
        let cache_root = td.path().join("cache");
        std::fs::create_dir_all(&cache_root).unwrap();
        let cache = CacheDir::at(&cache_root);
        let cfg_with_dev = ResolverConfig {
            include_dev: true,
            ..Default::default()
        };
        let lacre =
            resolve_lacre(&root, &cfg_with_dev, &cache).expect("resolve closure with dev deps");
        let names: Vec<&str> = lacre.entradas.iter().map(|e| e.nome.as_str()).collect();
        assert!(
            names.contains(&"child"),
            "resolve_lacre with the accessor-routed queue-seed must \
             surface `child` via the runtime `:deps` arm — got {names:?}"
        );
        assert!(
            names.contains(&"devchild"),
            "resolve_lacre with the accessor-routed queue-seed must \
             surface `devchild` via the `cfg.include_dev`-gated \
             `:deps-dev` arm — got {names:?}"
        );
    }

    /// Pin that [`ResolveError::missing_path`] is byte-equal to the
    /// pre-lift open-coded four-line `{ nome: nome.to_string(), path }`
    /// two-slot struct-literal on a representative
    /// `("caixa-teia", PathBuf("/nonexistent/child"))` fixture. Catches
    /// any future ctor-side silent field-swap, `.to_string()` /
    /// `.into()` cascade drop, or variant rename that would leave the
    /// wire-up compiling but surface a different two-slot payload than
    /// the pre-lift struct-literal.
    #[test]
    fn missing_path_ctor_matches_struct_literal_wrap() {
        let ctor = ResolveError::missing_path("caixa-teia", PathBuf::from("/nonexistent/child"));
        let literal = ResolveError::MissingPath {
            nome: "caixa-teia".to_string(),
            path: PathBuf::from("/nonexistent/child"),
        };
        assert_eq!(
            format!("{ctor:?}"),
            format!("{literal:?}"),
            "missing_path ctor must debug-render byte-equal to the \
             open-coded two-slot struct-literal on the same fixture — \
             any silent field-swap or cascade drop surfaces here"
        );
        assert_eq!(
            format!("{ctor}"),
            format!("{literal}"),
            "missing_path ctor must display-render byte-equal to the \
             open-coded struct-literal on the same fixture — pins the \
             thiserror #[error] template's routing through both slots"
        );
    }

    /// Sweep [`ResolveError::missing_path`] across a boundary matrix of
    /// `nome` shapes × `path` shapes so any wrapper-side silent
    /// lowercase, trim, truncate, two-axis field-swap, or `.into()`
    /// cascade divergence on the two-field construction surfaces at
    /// assert time rather than at a downstream diagnostic consumer that
    /// reads the fields back and gets a different value than the one it
    /// stored.
    ///
    /// - `nome` axis: canonical DNS-1123 identifier (`"caixa-teia"`),
    ///   single-char (`"a"`), and an inner-hyphen + digit shape
    ///   (`"hello-rio-42"`) — the three canonical `Dep::nome`-return
    ///   shapes the `caixa-core::Dep::nome` accessor surfaces at the
    ///   wire-up.
    /// - `path` axis: absolute UNIX-style (`"/absent/child"`), relative
    ///   single-segment (`"child"`), and a nested relative with parent
    ///   traversal (`"../sibling/child"`) — the three canonical
    ///   `PathBuf::from(caminho)` shapes the `DepSource::Path
    ///   { caminho }` arm surfaces at the wire-up.
    ///
    /// Both `&str`-literal and `&String` (via Deref coercion) carriers
    /// are exercised for `nome` because the wire-up hands `dep.nome()`
    /// (a `&str` accessor); both `PathBuf`-owned and `&Path` (via
    /// `Into<PathBuf>`) carriers are exercised for `path` because the
    /// wire-up hands a `PathBuf` built from `PathBuf::from(caminho)`.
    #[test]
    fn missing_path_ctor_routes_nome_and_path_through_verbatim() {
        let nomes: &[&str] = &["caixa-teia", "a", "hello-rio-42"];
        let paths: &[PathBuf] = &[
            PathBuf::from("/absent/child"),
            PathBuf::from("child"),
            PathBuf::from("../sibling/child"),
        ];
        for nome in nomes {
            for path in paths {
                // &str carrier for nome × PathBuf-owned carrier for path.
                let ctor = ResolveError::missing_path(nome, path.clone());
                match &ctor {
                    ResolveError::MissingPath {
                        nome: got_nome,
                        path: got_path,
                    } => {
                        assert_eq!(
                            got_nome, nome,
                            "missing_path ctor must carry nome byte-verbatim \
                             — no silent lowercase/trim/truncate on the \
                             &str-literal carrier at {nome}"
                        );
                        assert_eq!(
                            got_path, path,
                            "missing_path ctor must carry path byte-verbatim \
                             — no silent normalization/canonicalization on \
                             the PathBuf-owned carrier at {path:?}"
                        );
                    }
                    other => panic!(
                        "missing_path ctor must construct the MissingPath \
                         variant — got {other:?} for ({nome:?}, {path:?})"
                    ),
                }
                // &String (Deref coercion) carrier for nome × &Path
                // (Into<PathBuf>) carrier for path — pins that the ctor
                // signature accepts both without a pre-conversion at the
                // call site.
                let owned_nome = String::from(*nome);
                let ctor2 = ResolveError::missing_path(&owned_nome, path.as_path());
                match &ctor2 {
                    ResolveError::MissingPath {
                        nome: got_nome,
                        path: got_path,
                    } => {
                        assert_eq!(
                            got_nome, nome,
                            "missing_path ctor must carry nome byte-verbatim \
                             on the &String Deref-coercion carrier at {nome}"
                        );
                        assert_eq!(
                            got_path, path,
                            "missing_path ctor must carry path byte-verbatim \
                             on the &Path Into<PathBuf> carrier at {path:?}"
                        );
                    }
                    other => panic!(
                        "missing_path ctor must construct the MissingPath \
                         variant on the &String/&Path carriers — got \
                         {other:?} for ({nome:?}, {path:?})"
                    ),
                }
            }
        }
    }

    /// Pin the end-to-end route from [`fetch_path`]'s on-disk-absence
    /// arm through [`ResolveError::missing_path`]: authoring a `:deps`
    /// entry whose `:fonte (:tipo path :caminho …)` points at a path
    /// that does not exist on disk must surface a
    /// `ResolveError::MissingPath { nome, path }` byte-equal to the
    /// substrate-primitive ctor on the same fixture, so a future silent
    /// de-lift of the wire-up back to the open-coded struct-literal
    /// trips at caixa-resolver test time rather than at a downstream
    /// diagnostic consumer far from the wire-up commit.
    #[test]
    fn fetch_path_absent_dir_routes_through_missing_path_ctor() {
        let td = tempdir().expect("tempdir");
        // Root caixa points at a `:caminho` under the tempdir that
        // was never created — the closure walker's `fetch_path` arm
        // must refuse before it opens the (nonexistent)
        // `caixa.lisp`.
        let absent = td.path().join("absent-child");
        assert!(
            !absent.exists(),
            "test precondition: the child path must be absent on disk \
             so fetch_path's `!path.exists()` refusal arm fires"
        );
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
                nome: "absent-child".into(),
                versao: "0.1.0".into(),
                fonte: Some(DepSource::Path {
                    caminho: absent.to_string_lossy().into_owned(),
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
        let err = resolve_lacre(&root, &cfg, &cache)
            .expect_err("resolve_lacre must refuse when a :caminho points at an absent path");
        let expected = ResolveError::missing_path("absent-child", absent.clone());
        assert_eq!(
            format!("{err:?}"),
            format!("{expected:?}"),
            "fetch_path absent-dir refusal must debug-render byte-equal \
             to the substrate-primitive missing_path ctor on the same \
             fixture — a silent de-lift of the wire-up back to the \
             open-coded struct-literal would still pass this arm today \
             (both routes are byte-equal at emit time) but a future \
             ctor-side extension (a per-cluster :caminho overlay \
             rewrite, a per-scope alias table) would silently diverge \
             here as soon as the ctor's construction path widens"
        );
        assert_eq!(
            format!("{err}"),
            format!("{expected}"),
            "fetch_path absent-dir refusal must display-render \
             byte-equal to the substrate-primitive missing_path ctor \
             — pins the #[error(...)] template routing on both slots"
        );
        match err {
            ResolveError::MissingPath {
                nome: got_nome,
                path: got_path,
            } => {
                assert_eq!(
                    got_nome, "absent-child",
                    "fetch_path refusal must surface the offending Dep::nome \
                     verbatim in the MissingPath::nome slot"
                );
                assert_eq!(
                    got_path, absent,
                    "fetch_path refusal must surface the offending \
                     PathBuf::from(caminho) verbatim in the \
                     MissingPath::path slot"
                );
            }
            other => panic!(
                "fetch_path absent-dir refusal must construct the \
                 MissingPath variant on the ResolveError envelope — got \
                 {other:?}"
            ),
        }
    }
}
