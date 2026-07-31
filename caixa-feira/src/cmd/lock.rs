use std::path::PathBuf;

use anyhow::{Context, Result};
use caixa_lacre::{Lacre, LacreEntry, closure_hash, hash_bytes};
use clap::Args;

use super::load::{caixa_root, load_caixa};

/// Resolve deps and write `lacre.lisp`.
///
/// **Phase 1 resolver**: every declared dep becomes a `LacreEntry` whose
/// `:conteudo` hash is taken over `"{nome}@{versao}"` and whose
/// `:fechamento` is the closure hash (content + zero transitive deps).
/// No cloning, no transitive walk, no network — that lands in the phase 1.B
/// `feira resolve`, which replaces this stub.
#[derive(Args)]
pub struct Lock {
    /// caixa root (defaults to CWD).
    #[arg(long)]
    pub path: Option<PathBuf>,

    /// Don't actually write lacre.lisp — print the resolved content instead.
    #[arg(long)]
    pub dry_run: bool,
}

impl Lock {
    pub fn run(self) -> Result<()> {
        let root = caixa_root(self.path.as_deref());
        let caixa = load_caixa(&root)?;

        // Route the outer-`Caixa` `:deps` per-entry fan-out through the
        // typed [`caixa_core::Caixa::deps`] `&[Dep]`-return accessor
        // rather than the raw `.deps` field-access. Byte-equal today
        // (accessor returns `self.deps.as_slice()`); any future extension
        // of the accessor's semantics (a per-cluster canary-dep overlay
        // the operator pins through a future `:placement`-scoped slot, a
        // lacre-projected concrete-source rewrite, an M4 dep-alias table
        // the CR materializer resolves per-CR) reaches this fan-out
        // surface through exactly one caixa-core edit rather than a
        // coordinated rewrite. Peer of caixa-crd `caixa_into_cr` 210b9c5
        // + caixa-resolver `resolve_lacre` 5ce1b94's per-`:deps`
        // fan-out converges on the sibling K8s-CR-conversion + closure-
        // resolver consumers of the same axis; closes the last unlifted
        // per-`Caixa` `:deps` raw-field-access site in the `feira` CLI's
        // stub-resolver verb.
        let entries: Vec<LacreEntry> = caixa.deps().iter().map(resolve_stub).collect();

        let lacre = Lacre::from_entries(entries);
        let out = lacre.to_lisp();

        if self.dry_run {
            print!("{out}");
            return Ok(());
        }

        let lacre_path = root.join("lacre.lisp");
        std::fs::write(&lacre_path, &out)
            .with_context(|| format!("writing {}", lacre_path.display()))?;
        eprintln!(
            "locked {} dep(s); raiz = {}",
            lacre.entradas.len(),
            lacre.raiz
        );
        Ok(())
    }
}

/// Stub resolver — used when caixa-resolver isn't wired in. Defaults a
/// missing `:fonte` to `github:pleme-io/<nome>`, following the Zig-style
/// git-only store model.
pub(crate) fn resolve_stub(dep: &caixa_core::Dep) -> LacreEntry {
    // Route the per-`:deps` entry's `:fonte` presence-projection through
    // the typed [`caixa_core::Dep::fonte`] `Option<&DepSource>`-return
    // accessor rather than the raw `.fonte.clone()` field-access. Byte-
    // equal today (`.fonte()` returns `self.fonte.as_ref()`, and
    // `.cloned()` on `Option<&DepSource>` allocates exactly one
    // `DepSource` byte-equal to the prior raw `.fonte.clone()` read);
    // any future extension of the accessor's semantics (a per-scope
    // source-override table on `:fonte` the M4 CR materializer resolves
    // at admission time, a per-tenant `:fonte` rewrite overlay the
    // roadmap acknowledges, a lacre-projected concrete-source pin
    // resolver) reaches this fallback surface through exactly one
    // caixa-core edit rather than a coordinated rewrite. Peer of the
    // sibling caixa-crd `dep_into_ref` a64c9e6 per-`Dep::fonte` accessor-
    // route on the K8s-CR conversion surface — same "the emit path must
    // route through the substrate-primitive typed dispatch" discipline
    // extended onto the last unlifted `caixa-feira` stub-resolver
    // per-`Dep` `:fonte` field-access site.
    let fonte = dep.fonte().cloned().unwrap_or_else(|| {
        caixa_core::DepSource::default_github(caixa_core::DEFAULT_PLEME_GIT_ORG, dep.nome())
    });
    let conteudo = hash_bytes(format!("{}@{}", dep.nome(), dep.versao_requirement()).as_bytes());
    let fechamento = closure_hash(&conteudo, &[]);
    LacreEntry {
        nome: dep.nome().to_string(),
        versao: dep.versao_requirement().to_string(),
        fonte,
        conteudo,
        fechamento,
        deps_diretas: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use caixa_core::{Dep, DepSource};

    /// Pin that the per-`:deps` entry's `:fonte` presence-projection
    /// under [`resolve_stub`] routes through the typed [`Dep::fonte`]
    /// `Option<&DepSource>`-return accessor rather than the raw
    /// `.fonte.clone()` field-access. Sweeps both the author-omitted
    /// arm (accessor returns `None`, resolver defaults to
    /// `DepSource::default_github(DEFAULT_PLEME_GIT_ORG, nome)`) and the
    /// author-set arm (accessor returns `Some(&<verbatim>)`, resolver
    /// carries the source through `.cloned()` byte-verbatim). Byte-equal
    /// today (`.fonte()` returns `self.fonte.as_ref()`); catches any
    /// future emit-side regression that re-introduces the raw
    /// `.fonte.clone()` field-access, and any future extension of the
    /// accessor's semantics reaches this fallback surface through one
    /// typed dispatch.
    #[test]
    fn resolve_stub_fonte_routes_through_dep_fonte_accessor() {
        // Author-omitted `:fonte` arm — the accessor projects `None`
        // and the resolver's `unwrap_or_else` fires, materializing the
        // canonical `github:pleme-io/<nome>` default.
        let bare = Dep::simple("caixa-teia", "^0.1");
        assert!(
            bare.fonte().is_none(),
            "author-omitted :fonte must project None through the accessor",
        );
        let entry = resolve_stub(&bare);
        assert_eq!(entry.nome, "caixa-teia");
        assert_eq!(entry.versao, "^0.1");
        assert_eq!(
            entry.fonte,
            DepSource::default_github(caixa_core::DEFAULT_PLEME_GIT_ORG, "caixa-teia"),
            "author-omitted :fonte must fall back to \
             DepSource::default_github — a regression that re-inlines \
             the raw `.fonte.clone()` field-access would still pass \
             today (byte-equal), but any future accessor extension would \
             silently split the fallback from the accessor-routed source \
             of truth",
        );

        // Author-set `:fonte` arm — the accessor projects `Some(&<verbatim>)`
        // and the resolver's `.cloned()` carries the source through
        // byte-verbatim, bypassing the fallback branch.
        let author_set = Dep {
            nome: "caixa-teia".into(),
            versao: "^0.2".into(),
            fonte: Some(DepSource::Git {
                repo: "github:example/caixa-teia".into(),
                tag: Some("v1.2.3".into()),
                rev: None,
                branch: None,
            }),
            opcional: false,
            caracteristicas: vec![],
        };
        match author_set.fonte() {
            Some(DepSource::Git { repo, tag, .. }) => {
                assert_eq!(repo, "github:example/caixa-teia");
                assert_eq!(tag.as_deref(), Some("v1.2.3"));
            }
            other => panic!("expected author-set git :fonte from accessor, got {other:?}"),
        }
        let entry = resolve_stub(&author_set);
        assert_eq!(
            entry.fonte,
            DepSource::Git {
                repo: "github:example/caixa-teia".into(),
                tag: Some("v1.2.3".into()),
                rev: None,
                branch: None,
            },
            "author-set :fonte must survive `resolve_stub` byte-verbatim \
             through the accessor-routed `.cloned()` — a regression that \
             re-inlines the raw `.fonte.clone()` field-access would still \
             pass today (byte-equal), but any future accessor extension \
             (per-scope source-override table, per-tenant rewrite overlay, \
             lacre-projected concrete-source pin) would silently split \
             the carried source from the accessor-routed source of truth",
        );
    }
}
