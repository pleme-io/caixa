//! Round-trip between `caixa.lisp` ([`caixa_core::Caixa`]) and the K8s CR.
//!
//! The mapping is almost 1:1 — the CR carries a single source (a Caixa is
//! expected to live at one Git URL), while the Lisp manifest's `:deps` list
//! becomes `spec.deps` in the CR.

use caixa_core::{Caixa, CaixaKind, Dep, DepSource};

use crate::caixa_cr::{Caixa as CaixaCr, CaixaSource, CaixaSpec, DepRef, ReconcilePolicy};

/// Build a K8s `Caixa` resource from a `caixa.lisp`-parsed struct.
///
/// `source` is the Git reference the cluster should pin.
#[must_use]
pub fn caixa_into_cr(caixa: &Caixa, source: CaixaSource) -> CaixaCr {
    let spec = CaixaSpec {
        // Read every projection of the outer-`Caixa` `:nome` axis through
        // the typed [`caixa_core::Caixa::nome`] `&str`-return accessor so
        // both the `CaixaSpec.nome` `String`-carry emit-site and the
        // paired [`CaixaCr::new`] `&str` `.metadata.name` emit-site
        // route through one typed dispatch; any future extension of the
        // accessor's accept-set reaches both sites by construction. Peer
        // of the substrate-side [`caixa-helm`] (22461ef) /
        // [`caixa-flux`] (162e2e2) / [`caixa-mesh`] (980c059)
        // `Caixa::nome` non-`.clone()` field-access converges on the same
        // axis in the sibling per-target renderer crates.
        nome: caixa.nome().to_owned(),
        // Read the outer-`Caixa` `:versao` `String`-carry emit-site
        // through the typed [`caixa_core::Caixa::versao`] `&str`-return
        // accessor so the `CaixaSpec.versao` `String`-carry projection —
        // the axis the operator-side reconciler binds every per-Caixa
        // CR revision against, the axis every downstream `HelmRelease`
        // `spec.chart.spec.version` / OCI-tag / Artifact Hub release-
        // note surface consults — routes through one typed dispatch;
        // any future extension of the accessor's accept-set (SemVer-2
        // build-metadata canonicalization, OCI-tag normalization, per-
        // edition pre-release-tag overlay) reaches this emit site by
        // construction. Peer of caixa-helm eb912de's two-site
        // `caixa.versao.clone()` converge on the `Chart.yaml` `version:`
        // / `appVersion:` pair; closes the last unlifted per-`Caixa`
        // `.versao.clone()` raw-field-access `String`-carry axis in the
        // K8s-CR conversion crate.
        versao: caixa.versao().to_owned(),
        // Route the outer-`Caixa` `:kind` closed-set discriminator
        // through the typed [`caixa_core::Caixa::kind`] `CaixaKind`-return
        // accessor so the `CaixaSpec.kind` `String`-carry projection —
        // the axis every `caixa_from_cr` reverse-lookup `match` arm keys
        // off — routes through one typed dispatch. Any future extension
        // of the accessor's semantics (a canonicalizing rename overlay,
        // a per-`Acao` roll-out gate the M4 CR materializer resolves at
        // admission time, a promotion of `CaixaKind` through a
        // discriminant-carrying newtype) reaches this emit site by
        // construction. Peer of the sibling per-`Caixa` `:nome` /
        // `:versao` converges above on the same axis family; closes the
        // per-`Caixa` `:kind` universal-axis raw-field-access site.
        kind: format!("{:?}", caixa.kind()),
        source,
        reconcile: Some(ReconcilePolicy {
            interval_seconds: Some(300),
            auto_resolve: false,
            include_dev: false,
        }),
        // Route the outer-`Caixa` `:deps` `Vec<Dep>` slot through the
        // typed [`caixa_core::Caixa::deps`] `&[Dep]`-return accessor so
        // the `CaixaSpec.deps` per-entry fan-out — the axis every
        // `caixa_from_cr` reverse projection re-lifts through
        // `dep_from_ref` — routes through one typed dispatch on the
        // substrate primitive. Any future extension of the accessor's
        // semantics (a per-cluster canary-dep overlay the operator pins
        // through a future `:placement`-scoped slot, a lacre-projected
        // concrete-source rewrite, an M4 dep-alias table the CR
        // materializer resolves per-CR) reaches this fan-out surface
        // through exactly one caixa-core edit rather than a coordinated
        // rewrite. Peer of the sibling per-`:deps` entry accessor
        // converges in `dep_into_ref` on the inner per-entry axes, and
        // of the outer top-level [`caixa_core::Caixa::deps`] (5ce1b94)
        // caixa-resolver converge; closes the last unlifted per-`Caixa`
        // `:deps` universal-axis raw-field-access site in the K8s-CR
        // conversion crate.
        deps: caixa.deps().iter().map(dep_into_ref).collect(),
    };
    CaixaCr::new(caixa.nome(), spec)
}

/// Lower a K8s `Caixa` back to a `caixa_core::Caixa`. Loses trailing
/// optional metadata (autores, etiquetas, etc.) — when round-tripping the
/// Lisp authoring surface, prefer `caixa.lisp` as the source of truth.
pub fn caixa_from_cr(cr: &CaixaCr) -> Caixa {
    Caixa {
        nome: cr.spec.nome.clone(),
        versao: cr.spec.versao.clone(),
        kind: match cr.spec.kind.as_str() {
            "Biblioteca" => CaixaKind::Biblioteca,
            "Binario" => CaixaKind::Binario,
            "Servico" => CaixaKind::Servico,
            "Supervisor" => CaixaKind::Supervisor,
            "Aplicacao" => CaixaKind::Aplicacao,
            "Acao" => CaixaKind::Acao,
            _ => CaixaKind::Biblioteca,
        },
        edicao: None,
        descricao: None,
        repositorio: Some(cr.spec.source.repo.clone()),
        licenca: None,
        autores: vec![],
        etiquetas: vec![],
        deps: cr.spec.deps.iter().map(dep_from_ref).collect(),
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
        // The CR carries no `:ci` projection today (same "trailing
        // optional metadata" loss the doc comment above already names
        // for autores/etiquetas/etc.) — an Acao caixa's `:ci` slot is
        // authored + validated from `caixa.lisp` directly, not
        // round-tripped through this CR.
        ci: None,
    }
}

fn dep_into_ref(d: &Dep) -> DepRef {
    // Route every projection of the per-`:deps` entry's typed slots
    // through the lifted [`caixa_core::Dep`] accessors — `:nome`
    // through [`Dep::nome`] (eba2cde), `:versao` through
    // [`Dep::versao_requirement`] (05529b1), `:fonte` through the
    // newly-lifted [`Dep::fonte`] outer-`Dep` `Option<&DepSource>`
    // composite-reference accessor — so all three emit-sites in the
    // K8s-CR conversion crate route through one typed dispatch per
    // axis; any future accessor extension (per-scope alias table on
    // `:nome`, per-cluster canary-version overlay on `:versao`, per-
    // scope source-override table on `:fonte`) reaches this emit
    // surface by construction. Peer of the sibling top-level
    // [`Caixa`] `Caixa::nome` (61d3429) / [`Caixa::versao`] (41ab9a3)
    // converges in the enclosing `caixa_into_cr` on the same axis
    // family.
    DepRef {
        nome: d.nome().to_owned(),
        versao: d.versao_requirement().to_owned(),
        // Route the per-`DepSource::Git`-arm `git_ref` sole-set-pin
        // projection through the lifted [`DepSource::sole_pin`]
        // substrate accessor rather than the raw
        // `rev.clone().or(tag.clone()).or(branch.clone())` cascade —
        // the two pre-lift consumers of the sole-set-pin projection
        // (this crate's `dep_into_ref` `CaixaSource.git_ref` fill,
        // caixa-resolver's `fetch_git` `git checkout` target) now key
        // off exactly one typed dispatch on the substrate primitive,
        // so any future rebrand on the precedence axis (a `:commit`
        // pin peer once signed-commit-verification lands, a `:ref` pin
        // the M4 operator resolves per-cluster, a promotion of the
        // plain `Option<String>` pins to a typed `GitPin` newtype)
        // migrates as a single caixa-core edit rather than a
        // coordinated rewrite of both consumer sites. The
        // `.unwrap_or_else(|| "main".into())` fallback remains here
        // because caixa-crd's `git_ref` fill is required (the CRD
        // schema declares `git_ref: String` — not `Option<String>`);
        // the fallback fires on the unpinned-git shape
        // [`DepSource::default_github`] materializes (which
        // caixa-core's [`DepSource::validate`] rejects), so an
        // in-workspace validated `Dep` never lands on the fallback,
        // but the CRD round-trip via `dep_from_ref` emits the shape
        // with `tag: None, rev: Some(git_ref), branch: None` and
        // survives the round-trip byte-for-byte.
        source: d.fonte().and_then(|s| match s {
            DepSource::Git { repo, .. } => Some(CaixaSource {
                repo: repo.clone(),
                git_ref: s
                    .sole_pin()
                    .map_or_else(|| "main".to_string(), str::to_owned),
            }),
            DepSource::Path { caminho } => Some(CaixaSource {
                repo: format!("path:{caminho}"),
                git_ref: "HEAD".into(),
            }),
        }),
    }
}

fn dep_from_ref(r: &DepRef) -> Dep {
    Dep {
        nome: r.nome.clone(),
        versao: r.versao.clone(),
        fonte: r.source.as_ref().map(|s| DepSource::Git {
            repo: s.repo.clone(),
            tag: None,
            rev: Some(s.git_ref.clone()),
            branch: None,
        }),
        opcional: false,
        caracteristicas: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use caixa_core::{CaixaKind, Dep, DepSource};

    #[test]
    fn round_trip_preserves_core_fields() {
        let c = Caixa {
            nome: "demo".into(),
            versao: "0.1.0".into(),
            kind: CaixaKind::Biblioteca,
            edicao: None,
            descricao: None,
            repositorio: None,
            licenca: None,
            autores: vec![],
            etiquetas: vec![],
            deps: vec![Dep {
                nome: "x".into(),
                versao: "^0.1".into(),
                fonte: Some(DepSource::Git {
                    repo: "github:o/x".into(),
                    tag: Some("v1".into()),
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
        let cr = caixa_into_cr(
            &c,
            CaixaSource {
                repo: "github:pleme-io/demo".into(),
                git_ref: "v0.1.0".into(),
            },
        );
        let back = caixa_from_cr(&cr);
        assert_eq!(back.nome, c.nome);
        assert_eq!(back.versao, c.versao);
        assert_eq!(back.kind, c.kind);
        assert_eq!(back.deps.len(), c.deps.len());
    }

    /// Pin that both projections of the outer-`Caixa` `:nome` axis in
    /// [`caixa_into_cr`] — the `CaixaSpec.nome` `String`-carry emit-site
    /// and the paired [`CaixaCr::new`] `&str` `.metadata.name` emit-site
    /// — route through the typed [`Caixa::nome`] `&str`-return accessor.
    /// Byte-equal today (accessor returns `&self.nome`); catches any
    /// future accessor extension whose either emit-site regresses to a
    /// raw field read. Peer of the sibling substrate-side
    /// caixa-helm / caixa-flux / caixa-mesh `Caixa::nome`
    /// non-`.clone()` field-access converges (22461ef / 162e2e2 /
    /// 980c059).
    #[test]
    fn caixa_into_cr_nome_routes_through_caixa_nome_accessor() {
        let c = Caixa {
            nome: "demo".into(),
            versao: "0.1.0".into(),
            kind: CaixaKind::Biblioteca,
            edicao: None,
            descricao: None,
            repositorio: None,
            licenca: None,
            autores: vec![],
            etiquetas: vec![],
            deps: vec![],
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
        let cr = caixa_into_cr(
            &c,
            CaixaSource {
                repo: "github:pleme-io/demo".into(),
                git_ref: "v0.1.0".into(),
            },
        );
        // CaixaSpec.nome emit-site echoes the accessor.
        assert_eq!(cr.spec.nome, c.nome());
        // CaixaCr::new `.metadata.name` emit-site echoes the accessor.
        assert_eq!(cr.metadata.name.as_deref(), Some(c.nome()));
    }

    /// Pin that the outer-`Caixa` `:versao` `String`-carry emit-site in
    /// [`caixa_into_cr`] — the `CaixaSpec.versao` projection every
    /// operator-side per-Caixa CR revision binds against, and every
    /// downstream `HelmRelease` `spec.chart.spec.version` / OCI-tag /
    /// Artifact Hub release-note surface consults — routes through the
    /// typed [`Caixa::versao`] `&str`-return accessor. Byte-equal today
    /// (accessor returns `&self.versao`); catches any future accessor
    /// extension whose emit-side write regresses to a raw field read.
    /// Peer of caixa-helm eb912de's `Chart.yaml` `version:` / `appVersion:`
    /// two-site converge on `caixa.versao.clone()`.
    #[test]
    fn caixa_into_cr_versao_routes_through_caixa_versao_accessor() {
        let c = Caixa {
            nome: "demo".into(),
            versao: "0.1.0".into(),
            kind: CaixaKind::Biblioteca,
            edicao: None,
            descricao: None,
            repositorio: None,
            licenca: None,
            autores: vec![],
            etiquetas: vec![],
            deps: vec![],
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
        let cr = caixa_into_cr(
            &c,
            CaixaSource {
                repo: "github:pleme-io/demo".into(),
                git_ref: "v0.1.0".into(),
            },
        );
        // CaixaSpec.versao emit-site echoes the accessor.
        assert_eq!(cr.spec.versao, c.versao());
    }

    /// Pin that every per-`:deps` entry projection in [`dep_into_ref`]
    /// — the `DepRef.nome` `String`-carry, the `DepRef.versao`
    /// `String`-carry, and the `DepRef.source` two-arm [`DepSource`]
    /// projection — routes through the typed [`caixa_core::Dep`]
    /// accessors ([`Dep::nome`] / [`Dep::versao_requirement`] /
    /// [`Dep::fonte`]) rather than raw `.nome.clone()` /
    /// `.versao.clone()` / `.fonte.as_ref()` field reads. Byte-equal
    /// today (each accessor returns a borrow into its own storage);
    /// catches any future emit-site regression that reintroduces a raw
    /// field read, and pins the two-arm `CaixaSource` projection
    /// (`DepSource::Git` → `{repo, git_ref: rev|tag|branch}`,
    /// `DepSource::Path { caminho }` → `{"path:<caminho>", "HEAD"}`)
    /// against the accessor-routed `:fonte` value. Peer of the sibling
    /// `caixa_into_cr_nome_routes_through_caixa_nome_accessor` /
    /// `caixa_into_cr_versao_routes_through_caixa_versao_accessor`
    /// pins on the outer-`Caixa` altitude.
    #[test]
    fn dep_into_ref_routes_through_dep_accessors() {
        // Git-source arm.
        let git = Dep {
            nome: "caixa-teia".into(),
            versao: "^0.1".into(),
            fonte: Some(DepSource::Git {
                repo: "github:pleme-io/caixa-teia".into(),
                tag: Some("v0.1.0".into()),
                rev: None,
                branch: None,
            }),
            opcional: false,
            caracteristicas: vec![],
        };
        let r = dep_into_ref(&git);
        assert_eq!(r.nome, git.nome());
        assert_eq!(r.versao, git.versao_requirement());
        let src = r.source.as_ref().expect("git dep projects a source");
        assert_eq!(src.repo, "github:pleme-io/caixa-teia");
        assert_eq!(src.git_ref, "v0.1.0");
        // Confirm the projector read `:fonte` through the accessor,
        // not the raw field — the accessor returned a `Some(&Git{…})`
        // whose `repo` byte-string is what the two-arm projection
        // consumed.
        match git.fonte() {
            Some(DepSource::Git { repo, .. }) => assert_eq!(repo, &src.repo),
            other => panic!("expected git :fonte from accessor, got {other:?}"),
        }

        // Path-source arm.
        let path = Dep {
            nome: "caixa-teia".into(),
            versao: "0.1.0".into(),
            fonte: Some(DepSource::Path {
                caminho: "../caixa-teia".into(),
            }),
            opcional: false,
            caracteristicas: vec![],
        };
        let r = dep_into_ref(&path);
        assert_eq!(r.nome, path.nome());
        assert_eq!(r.versao, path.versao_requirement());
        let src = r.source.as_ref().expect("path dep projects a source");
        assert_eq!(src.repo, "path:../caixa-teia");
        assert_eq!(src.git_ref, "HEAD");

        // Author-omitted `:fonte` arm — the accessor projects `None`
        // and the projector's `and_then` short-circuits to `None`.
        let none = Dep::simple("caixa-teia", "^0.1");
        let r = dep_into_ref(&none);
        assert_eq!(r.nome, none.nome());
        assert_eq!(r.versao, none.versao_requirement());
        assert!(none.fonte().is_none());
        assert!(r.source.is_none());
    }

    /// Pin that the outer-`Caixa` `:kind` closed-set discriminator
    /// projection in [`caixa_into_cr`] — the `CaixaSpec.kind` `String`-
    /// carry emit-site every `caixa_from_cr` reverse `match` arm keys
    /// off — routes through the typed [`Caixa::kind`] `CaixaKind`-return
    /// accessor rather than the raw `.kind` field access. Byte-equal
    /// today (accessor returns `self.kind`); catches any future emit-
    /// site regression that reintroduces a raw field read on any of the
    /// six [`CaixaKind`] arms the round-trip `caixa_from_cr` reverse
    /// lookup consumes. Peer of the sibling
    /// `caixa_into_cr_nome_routes_through_caixa_nome_accessor` /
    /// `caixa_into_cr_versao_routes_through_caixa_versao_accessor` pins
    /// on the outer-`Caixa` altitude — same "accessor is the sole read
    /// path across every arm" discipline extended onto the last
    /// unlifted per-`Caixa` universal-axis raw-field-access site in the
    /// K8s-CR conversion crate.
    #[test]
    fn caixa_into_cr_kind_routes_through_caixa_kind_accessor() {
        // Sweep every [`CaixaKind`] arm the round-trip reverse lookup
        // consumes — a future silent detour that re-inlined `caixa.kind`
        // on any arm's format-projected `String` byte-carry would
        // surface here as a byte-equal miss (the reverse `caixa_from_cr`
        // `match` arm would silently fall through to the
        // `_ => CaixaKind::Biblioteca` default arm without the accessor-
        // routed emit-site the six-arm accept-set requires).
        for kind in [
            CaixaKind::Biblioteca,
            CaixaKind::Binario,
            CaixaKind::Servico,
            CaixaKind::Supervisor,
            CaixaKind::Aplicacao,
            CaixaKind::Acao,
        ] {
            let c = Caixa {
                nome: "demo".into(),
                versao: "0.1.0".into(),
                kind,
                edicao: None,
                descricao: None,
                repositorio: None,
                licenca: None,
                autores: vec![],
                etiquetas: vec![],
                deps: vec![],
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
            // `caixa_into_cr` reads only the six per-`Caixa` axes it
            // projects (`nome`, `versao`, `kind`, `deps`, plus the
            // caller-supplied `source`); no per-arm layout invariant
            // fires at this altitude, so the discriminant sweep needs
            // no per-arm layout surface tweak.
            let cr = caixa_into_cr(
                &c,
                CaixaSource {
                    repo: "github:pleme-io/demo".into(),
                    git_ref: "v0.1.0".into(),
                },
            );
            assert_eq!(
                cr.spec.kind,
                format!("{:?}", c.kind()),
                "CaixaSpec.kind emit-site must byte-equal the typed \
                 `Caixa::kind` accessor's `Debug` projection — a \
                 regression that re-inlines `caixa.kind` at the emit \
                 site silently splits the per-arm reverse `caixa_from_cr` \
                 lookup input from the accessor-routed source of truth"
            );
        }
    }

    /// Pin that the outer-`Caixa` `:deps` per-entry fan-out in
    /// [`caixa_into_cr`] — the `CaixaSpec.deps` `Vec<DepRef>` emit-site
    /// every `caixa_from_cr` reverse projection re-lifts through
    /// `dep_from_ref` — routes through the typed [`Caixa::deps`]
    /// `&[Dep]`-return accessor rather than the raw `.deps` field
    /// access. Byte-equal today (accessor returns `self.deps.as_slice()`);
    /// catches any future emit-site regression that reintroduces a raw
    /// field read on the fan-out traversal head. Peer of the sibling
    /// `dep_into_ref_routes_through_dep_accessors` per-entry pin on the
    /// inner `Dep::nome` / `Dep::versao_requirement` / `Dep::fonte`
    /// axes, of the outer top-level
    /// `caixa_into_cr_nome_routes_through_caixa_nome_accessor` /
    /// `caixa_into_cr_versao_routes_through_caixa_versao_accessor`
    /// pins on the outer-`Caixa` altitude, and of the sibling
    /// [`caixa_core::Caixa::deps`] (5ce1b94) caixa-resolver converge on
    /// the same axis in the sibling resolver crate.
    #[test]
    fn caixa_into_cr_deps_routes_through_caixa_deps_accessor() {
        // Fixture: two `:deps` entries, one Git-source arm and one
        // author-omitted `:fonte` arm — the per-entry fan-out must
        // preserve both order (a `Vec<DepRef>` is order-sensitive; the
        // reverse `caixa_from_cr` projection walks it index-by-index)
        // and byte-content across every axis the accessor exposes. A
        // future silent detour that re-inlined `caixa.deps.iter()` at
        // the fan-out head would still satisfy the round-trip pin
        // today (accessor is byte-equal to the raw field), but any
        // future accessor extension (per-cluster canary overlay,
        // lacre-projected concrete-source rewrite, M4 dep-alias table)
        // would silently split the fan-out from the accessor-routed
        // source of truth.
        let c = Caixa {
            nome: "demo".into(),
            versao: "0.1.0".into(),
            kind: CaixaKind::Biblioteca,
            edicao: None,
            descricao: None,
            repositorio: None,
            licenca: None,
            autores: vec![],
            etiquetas: vec![],
            deps: vec![
                Dep {
                    nome: "caixa-teia".into(),
                    versao: "^0.1".into(),
                    fonte: Some(DepSource::Git {
                        repo: "github:pleme-io/caixa-teia".into(),
                        tag: Some("v0.1.0".into()),
                        rev: None,
                        branch: None,
                    }),
                    opcional: false,
                    caracteristicas: vec![],
                },
                Dep::simple("caixa-lacre", "^0.2"),
            ],
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
        let cr = caixa_into_cr(
            &c,
            CaixaSource {
                repo: "github:pleme-io/demo".into(),
                git_ref: "v0.1.0".into(),
            },
        );
        // Fan-out length matches the accessor-projected slice length.
        assert_eq!(
            cr.spec.deps.len(),
            c.deps().len(),
            "CaixaSpec.deps fan-out length must byte-equal the typed \
             `Caixa::deps` accessor's slice length — a regression that \
             re-inlines `caixa.deps.iter()` at the fan-out head silently \
             splits the fan-out sizing from the accessor-routed source \
             of truth"
        );
        // Per-entry projection round-trips through the accessor at both
        // ends: the emit-side `DepRef.nome` / `DepRef.versao` byte-strings
        // agree with the accessor-routed per-entry `Dep::nome()` /
        // `Dep::versao_requirement()` projections.
        for (dep, dep_ref) in c.deps().iter().zip(cr.spec.deps.iter()) {
            assert_eq!(dep_ref.nome, dep.nome());
            assert_eq!(dep_ref.versao, dep.versao_requirement());
        }
    }
}
