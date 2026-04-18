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
        nome: caixa.nome.clone(),
        versao: caixa.versao.clone(),
        kind: format!("{:?}", caixa.kind),
        source,
        reconcile: Some(ReconcilePolicy {
            interval_seconds: Some(300),
            auto_resolve: false,
            include_dev: false,
        }),
        deps: caixa.deps.iter().map(dep_into_ref).collect(),
    };
    CaixaCr::new(&caixa.nome, spec)
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
    }
}

fn dep_into_ref(d: &Dep) -> DepRef {
    DepRef {
        nome: d.nome.clone(),
        versao: d.versao.clone(),
        source: d.fonte.as_ref().and_then(|s| match s {
            DepSource::Git {
                repo,
                tag,
                rev,
                branch,
            } => Some(CaixaSource {
                repo: repo.clone(),
                git_ref: rev
                    .clone()
                    .or(tag.clone())
                    .or(branch.clone())
                    .unwrap_or_else(|| "main".to_string()),
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
}
