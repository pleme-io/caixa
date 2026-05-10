//! Layout invariants — the Rust-enforced package structure.
//!
//! This is the caixa analog of Cargo's implicit `src/lib.rs` vs `src/main.rs`
//! rule: the Rust type system dictates the package shape, and the invariant
//! checker runs before any build step. [`StandardLayout`] encodes the
//! canonical layout:
//!
//! - `caixa.lisp`           — always required
//! - `lib/<nome>.lisp`      — required when `:kind Biblioteca` and
//!                            `:bibliotecas` is empty
//! - each `:bibliotecas`    — must resolve on disk
//! - each `:exe`            — must resolve on disk, under `exe/`
//! - each `:servicos`       — must resolve on disk, under `servicos/`
//!
//! Filesystem I/O is injected through [`StandardLayout::with_path_exists`]
//! so tests can run without touching disk.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;

use crate::{Caixa, CaixaKind};

/// Contract — a caixa layout checker.
pub trait LayoutInvariants {
    /// Verify every declared path resolves + kind-specific invariants hold.
    fn verify(&self, caixa: &Caixa, root: &Path) -> Result<(), LayoutError>;
}

type ExistsFn = Arc<dyn Fn(&Path) -> bool + Send + Sync>;

/// The default layout contract.
#[derive(Default, Clone)]
pub struct StandardLayout {
    path_exists: Option<ExistsFn>,
}

impl StandardLayout {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Override how file existence is tested. Useful for in-memory tests.
    #[must_use]
    pub fn with_path_exists<F>(mut self, f: F) -> Self
    where
        F: Fn(&Path) -> bool + Send + Sync + 'static,
    {
        self.path_exists = Some(Arc::new(f));
        self
    }

    fn exists(&self, p: &Path) -> bool {
        self.path_exists
            .as_ref()
            .map_or_else(|| p.exists(), |f| f(p))
    }
}

impl std::fmt::Debug for StandardLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StandardLayout")
            .field("custom_exists", &self.path_exists.is_some())
            .finish()
    }
}

impl LayoutInvariants for StandardLayout {
    fn verify(&self, caixa: &Caixa, root: &Path) -> Result<(), LayoutError> {
        let manifest = root.join("caixa.lisp");
        if !self.exists(&manifest) {
            return Err(LayoutError::MissingManifest(manifest));
        }

        // `:nome` value-shape gate runs first among the struct-level
        // checks: every downstream invariant that references the nome
        // (the `lib/<nome>.lisp` default-path resolution below, every
        // `LayoutError::*Violation` arm whose payload carries
        // `caixa.nome.clone()`, every K8s `metadata.name` the M3
        // renderers emit) reads a value the apiserver accepts without
        // re-validating. A malformed `:nome` here would otherwise
        // surface as a less-helpful "missing biblioteca lib/<bad>.lisp"
        // — or, worse, pass layout verify and fail at `kubectl apply`
        // time when the apiserver rejects the rendered metadata.name
        // far from the source caixa.lisp.
        caixa.validate_nome().map_err(|err| match err {
            crate::manifest::CaixaNomeError::Empty => LayoutError::NomeInvalid {
                nome: caixa.nome.clone(),
                reason: "must be non-empty (every caixa names itself)".to_string(),
            },
            crate::manifest::CaixaNomeError::Invalid { nome, reason } => {
                LayoutError::NomeInvalid { nome, reason }
            }
        })?;

        // Supervisors and Aplicacaos don't run code; reject
        // bibliotecas/exe/servicos declarations BEFORE checking those
        // paths exist (which would otherwise produce a less-helpful
        // "missing entry" error first).
        let has_code = !caixa.bibliotecas.is_empty()
            || !caixa.exe.is_empty()
            || !caixa.servicos.is_empty();
        if caixa.kind == CaixaKind::Supervisor && has_code {
            return Err(LayoutError::SupervisorOwnsCode(caixa.nome.clone()));
        }
        if caixa.kind == CaixaKind::Aplicacao && has_code {
            return Err(LayoutError::AplicacaoOwnsCode(caixa.nome.clone()));
        }

        if caixa.kind == CaixaKind::Biblioteca && caixa.bibliotecas.is_empty() {
            let expected = root.join("lib").join(format!("{}.lisp", caixa.nome));
            if !self.exists(&expected) {
                return Err(LayoutError::MissingLib {
                    caixa: caixa.nome.clone(),
                    expected,
                });
            }
        }

        if caixa.kind.requires_exe() && caixa.exe.is_empty() {
            return Err(LayoutError::BinarioWithoutExe(caixa.nome.clone()));
        }

        if caixa.kind.requires_servicos() && caixa.servicos.is_empty() {
            return Err(LayoutError::ServicoWithoutServicos(caixa.nome.clone()));
        }

        for p in &caixa.bibliotecas {
            let full = root.join(p);
            if !self.exists(&full) {
                return Err(LayoutError::MissingEntry {
                    kind: "biblioteca",
                    path: full,
                });
            }
        }

        let exe_dir = root.join("exe");
        for p in &caixa.exe {
            let full = root.join(p);
            if !self.exists(&full) {
                return Err(LayoutError::MissingEntry {
                    kind: "exe",
                    path: full,
                });
            }
            if !full.starts_with(&exe_dir) {
                return Err(LayoutError::ExeOutsideDir(full));
            }
        }

        let servicos_dir = root.join("servicos");
        for p in &caixa.servicos {
            let full = root.join(p);
            if !self.exists(&full) {
                return Err(LayoutError::MissingEntry {
                    kind: "servico",
                    path: full,
                });
            }
            if !full.starts_with(&servicos_dir) {
                return Err(LayoutError::ServicoOutsideDir(full));
            }
        }

        // ── M2 typed-substrate invariants ────────────────────────────────

        // Lunatic-style per-process limits: every declared axis must
        // be meaningfully non-zero. A zero on any axis is the same
        // authorial-intent footgun as a 0-failure circuit-breaker or
        // a 0-port :entrada — wasmtime would consume the value as
        // "trap immediately" rather than the author's "an unspecified
        // bound". See `LimitsSpec::validate` for the full rationale.
        if let Some(l) = &caixa.limits {
            l.validate().map_err(|err| LayoutError::LimitsViolation {
                caixa: caixa.nome.clone(),
                issue: err.to_string(),
            })?;
        }

        // Behavior callbacks: every declared callback must (a) be
        // value-shape valid (no empty / absolute / parent-escaping
        // path values that would silently subvert the layout
        // checker's `root.join(p)` sandbox), then (b) resolve on disk.
        // The shape pass runs first so the diagnostic names *which
        // :behavior slot* is malformed before the existence check
        // would otherwise surface a less-helpful "missing entry".
        if let Some(b) = &caixa.behavior {
            b.validate().map_err(|err| LayoutError::BehaviorViolation {
                caixa: caixa.nome.clone(),
                issue: err.to_string(),
            })?;
            for p in b.declared_paths() {
                let full = root.join(p);
                if !self.exists(&full) {
                    return Err(LayoutError::MissingEntry {
                        kind: "behavior-callback",
                        path: full,
                    });
                }
            }
        }

        // Upgrade scripts: every state-change instruction must point at
        // an existing tatara-lisp file.
        for entry in &caixa.upgrade_from {
            for instr in &entry.instructions {
                if let Some(p) = instr.declared_path() {
                    let full = root.join(p);
                    if !self.exists(&full) {
                        return Err(LayoutError::MissingEntry {
                            kind: "upgrade-script",
                            path: full,
                        });
                    }
                }
            }
        }

        // Supervisor invariants (typed shape — children, restart strategy).
        // The "supervisor doesn't own code" check is at the top of verify()
        // so it fires before the existence-check loops.
        if caixa.kind == CaixaKind::Supervisor {
            let view = caixa
                .supervisor_view()
                .expect("Supervisor kind must have a supervisor_view");
            view.validate()
                .map_err(|err| LayoutError::SupervisorViolation {
                    caixa: caixa.nome.clone(),
                    issue: err.to_string(),
                })?;
        }

        // Aplicacao invariants — typed graph composition. Like
        // Supervisor, an Aplicacao runs no code itself.
        if caixa.kind == CaixaKind::Aplicacao {
            let view = caixa
                .aplicacao_view()
                .expect("Aplicacao kind must have an aplicacao_view");
            view.validate()
                .map_err(|err| LayoutError::AplicacaoViolation {
                    caixa: caixa.nome.clone(),
                    issue: err.to_string(),
                })?;
        }

        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LayoutError {
    #[error("manifest missing: {}", .0.display())]
    MissingManifest(PathBuf),
    #[error(
        "caixa :nome {nome:?} is not a valid DNS-1123 label: {reason} (the K8s apiserver \
         enforces this rule on every metadata.name the renderers emit; use a name like \
         \"checkout\" or \"hello-rio\" — lowercase RFC 1123 alphanumeric + hyphen, max 63 \
         bytes)"
    )]
    NomeInvalid { nome: String, reason: String },
    #[error("caixa '{caixa}' is a Biblioteca but has no lib entry — expected {}", expected.display())]
    MissingLib { caixa: String, expected: PathBuf },
    #[error("caixa '{0}' is a Binario but has no :exe entries")]
    BinarioWithoutExe(String),
    #[error("caixa '{0}' is a Servico but has no :servicos entries")]
    ServicoWithoutServicos(String),
    #[error("declared {kind} entry missing: {}", path.display())]
    MissingEntry { kind: &'static str, path: PathBuf },
    #[error("exe entry outside exe/ directory: {}", .0.display())]
    ExeOutsideDir(PathBuf),
    #[error("servico entry outside servicos/ directory: {}", .0.display())]
    ServicoOutsideDir(PathBuf),
    #[error("caixa '{caixa}' has invalid :limits: {issue}")]
    LimitsViolation { caixa: String, issue: String },
    #[error("caixa '{caixa}' has invalid :behavior callback: {issue}")]
    BehaviorViolation { caixa: String, issue: String },
    #[error("supervisor caixa '{caixa}' violates typed shape: {issue}")]
    SupervisorViolation { caixa: String, issue: String },
    #[error("supervisor caixa '{0}' must not declare :bibliotecas, :exe, or :servicos — supervisors don't run code, they orchestrate other caixas")]
    SupervisorOwnsCode(String),
    #[error("aplicacao caixa '{caixa}' violates typed shape: {issue}")]
    AplicacaoViolation { caixa: String, issue: String },
    #[error("aplicacao caixa '{0}' must not declare :bibliotecas, :exe, or :servicos — aplicacaos compose Servicos, they don't run code themselves")]
    AplicacaoOwnsCode(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Caixa, CaixaKind};
    use std::path::PathBuf;

    fn caixa(kind: CaixaKind) -> Caixa {
        Caixa {
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
            // M2 typed-substrate slots default to absent.
            limits: None,
            behavior: None,
            upgrade_from: vec![],
            estrategia: None,
            max_restarts: None,
            restart_window: None,
            children: vec![],
            // M3 Aplicacao slots default to absent.
            membros: vec![],
            contratos: vec![],
            politicas: None,
            placement: None,
            entrada: None,
        }
    }

    #[test]
    fn missing_manifest_errors() {
        let layout = StandardLayout::new().with_path_exists(|_| false);
        let err = layout
            .verify(&caixa(CaixaKind::Biblioteca), Path::new("/tmp/x"))
            .unwrap_err();
        assert!(matches!(err, LayoutError::MissingManifest(_)));
    }

    #[test]
    fn biblioteca_needs_default_lib_path() {
        let root = PathBuf::from("/tmp/x");
        let expect_manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == expect_manifest);
        let err = layout
            .verify(&caixa(CaixaKind::Biblioteca), &root)
            .unwrap_err();
        assert!(matches!(err, LayoutError::MissingLib { .. }));
    }

    #[test]
    fn biblioteca_passes_when_default_lib_exists() {
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        layout
            .verify(&caixa(CaixaKind::Biblioteca), &root)
            .expect("should pass");
    }

    #[test]
    fn binario_without_exe_errors() {
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let err = layout
            .verify(&caixa(CaixaKind::Binario), &root)
            .unwrap_err();
        assert!(matches!(err, LayoutError::BinarioWithoutExe(_)));
    }

    #[test]
    fn exe_outside_dir_errors() {
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let outside = root.join("../sibling/tool");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest || p == outside);
        let mut c = caixa(CaixaKind::Binario);
        c.exe = vec!["../sibling/tool".into()];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(matches!(err, LayoutError::ExeOutsideDir(_)));
    }

    // ── M2 typed-substrate invariants ────────────────────────────────────

    #[test]
    fn behavior_callback_path_must_exist() {
        use crate::BehaviorSpec;
        use std::path::PathBuf;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        let svc = root.join("servicos/demo.computeunit.yaml");
        c.behavior = Some(BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            ..Default::default()
        });
        let manifest_clone = manifest.clone();
        let svc_clone = svc.clone();
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest_clone || p == svc_clone);
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(matches!(
            err,
            LayoutError::MissingEntry {
                kind: "behavior-callback",
                ..
            }
        ));

        // Now declare the path exists — passes.
        let init = root.join("lib/init.lisp");
        let layout = StandardLayout::new()
            .with_path_exists(move |p| p == manifest || p == svc || p == init);
        layout.verify(&c, &root).unwrap();
    }

    #[test]
    fn behavior_absolute_callback_is_violation_not_missing() {
        // An absolute path silently subverts `root.join(p)` (Path::join
        // replaces the base when the right side is absolute). Before
        // BehaviorSpec::validate ran, an `:on-init "/etc/passwd"` would
        // surface as a confusing "missing behavior-callback /etc/passwd"
        // — or, worse, pass when /etc/passwd happens to exist. Now it's
        // a value-shape error naming the slot.
        use crate::BehaviorSpec;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.behavior = Some(BehaviorSpec {
            on_init: Some(PathBuf::from("/etc/passwd")),
            ..Default::default()
        });
        // Path exists check would *succeed* on /etc/passwd (proving the
        // sandbox bypass) — value-shape pass must fire first.
        let layout = StandardLayout::new()
            .with_path_exists(move |p| p == manifest || p == svc || p == Path::new("/etc/passwd"));
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::BehaviorViolation { ref caixa, .. } if caixa == "demo"),
            "got {err:?}",
        );
    }

    #[test]
    fn behavior_empty_callback_is_violation() {
        use crate::BehaviorSpec;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.behavior = Some(BehaviorSpec {
            on_call: Some(PathBuf::new()),
            ..Default::default()
        });
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest || p == svc);
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(matches!(err, LayoutError::BehaviorViolation { .. }));
    }

    #[test]
    fn upgrade_script_path_must_exist() {
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        use std::path::PathBuf;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![UpgradeInstruction::StateChange {
                script: PathBuf::from("lib/migrations/v01-to-v02.lisp"),
            }],
        }];
        let manifest_clone = manifest.clone();
        let svc_clone = svc.clone();
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest_clone || p == svc_clone);
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(matches!(
            err,
            LayoutError::MissingEntry {
                kind: "upgrade-script",
                ..
            }
        ));
    }

    #[test]
    fn supervisor_must_have_children() {
        use crate::RestartStrategy;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Supervisor);
        c.estrategia = Some(RestartStrategy::OneForOne);
        c.max_restarts = Some(5);
        // No children → should fail
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(matches!(err, LayoutError::SupervisorViolation { .. }));
    }

    #[test]
    fn supervisor_must_not_have_bibliotecas() {
        use crate::{ChildSpec, RestartPolicy, RestartStrategy};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Supervisor);
        c.estrategia = Some(RestartStrategy::OneForOne);
        c.max_restarts = Some(5);
        c.bibliotecas = vec!["lib/code.lisp".into()];
        c.children = vec![ChildSpec {
            caixa: "worker".into(),
            versao: "^0.1".into(),
            restart: RestartPolicy::Permanent,
        }];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(matches!(err, LayoutError::SupervisorOwnsCode(_)));
    }

    // ── Aplicacao layout tests ──────────────────────────────────────────

    #[test]
    fn aplicacao_must_have_membros() {
        use crate::{Membro, Placement, PlacementStrategy};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Aplicacao);
        c.placement = Some(Placement {
            estrategia: PlacementStrategy::Replicated,
            clusters: vec!["rio".into()],
            affinity: None,
            shard_key: None,
        });
        // No membros → fails
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(matches!(err, LayoutError::AplicacaoViolation { .. }));

        // With membros → passes
        c.membros = vec![Membro {
            caixa: "service-a".into(),
            versao: "^0.1".into(),
        }];
        layout.verify(&c, &root).unwrap();
    }

    #[test]
    fn aplicacao_must_not_have_bibliotecas() {
        use crate::{Membro, Placement, PlacementStrategy};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Aplicacao);
        c.bibliotecas = vec!["lib/code.lisp".into()];
        c.membros = vec![Membro {
            caixa: "x".into(),
            versao: "^0.1".into(),
        }];
        c.placement = Some(Placement {
            estrategia: PlacementStrategy::Replicated,
            clusters: vec!["rio".into()],
            affinity: None,
            shard_key: None,
        });
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(matches!(err, LayoutError::AplicacaoOwnsCode(_)));
    }

    #[test]
    fn aplicacao_with_unknown_contrato_member_fails() {
        use crate::{Membro, Placement, PlacementStrategy, WitContract};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Aplicacao);
        c.membros = vec![Membro {
            caixa: "service-a".into(),
            versao: "^0.1".into(),
        }];
        c.contratos = vec![WitContract {
            de: "service-a".into(),
            para: "phantom".into(),
            wit: "wasi:http/proxy".into(),
            endpoint: Some("/x".into()),
            subject: None,
            slot: None,
        }];
        c.placement = Some(Placement {
            estrategia: PlacementStrategy::Replicated,
            clusters: vec!["rio".into()],
            affinity: None,
            shard_key: None,
        });
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(matches!(err, LayoutError::AplicacaoViolation { .. }));
    }

    #[test]
    fn limits_zero_axis_surfaces_as_layout_violation() {
        use crate::LimitsSpec;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.limits = Some(LimitsSpec {
            fuel: Some(0),
            ..Default::default()
        });
        let manifest_clone = manifest.clone();
        let svc_clone = svc.clone();
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest_clone || p == svc_clone);
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::LimitsViolation { caixa, issue } = err else {
            panic!("expected LimitsViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(issue.contains(":fuel"), "issue must name the axis: {issue}");
    }

    #[test]
    fn limits_well_formed_passes_layout() {
        use crate::LimitsSpec;
        use std::time::Duration;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.limits = Some(LimitsSpec {
            memory: Some(64 * 1024 * 1024),
            fuel: Some(1_000_000),
            wall_clock: Some(Duration::from_secs(30)),
            cpu: Some(500),
        });
        let manifest_clone = manifest.clone();
        let svc_clone = svc.clone();
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest_clone || p == svc_clone);
        layout.verify(&c, &root).unwrap();
    }

    // ── Caixa nome value-shape (DNS-1123 label) ─────────────────────────

    #[test]
    fn nome_invalid_surfaces_as_layout_violation() {
        // Fail-before-pass-after pin: a malformed `:nome` (uppercase)
        // surfaces at layout-verify time as `LayoutError::NomeInvalid`,
        // not as a downstream "missing biblioteca lib/<bad>.lisp"
        // diagnostic or — worse — a `kubectl apply` admission failure
        // far from the source caixa.lisp. The c7d05ec `:entrada :host`
        // gate's peer on the rootmost identity axis.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.nome = "FooBar".into();
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::NomeInvalid { nome, reason } = err else {
            panic!("expected NomeInvalid, got {err:?}");
        };
        assert_eq!(nome, "FooBar");
        assert!(reason.contains("uppercase"), "got reason: {reason}");
    }

    #[test]
    fn nome_invalid_fires_before_missing_lib_check() {
        // Order pin: the `:nome` value-shape gate runs *before* the
        // biblioteca-default-lib path-existence check, so a malformed
        // `:nome` doesn't surface as a confusing "missing
        // lib/FooBar.lisp" — the more self-locating diagnostic
        // ("nome is not a valid DNS-1123 label") leads. Without this
        // ordering an author with a typoed `:nome` would chase the
        // wrong fix (creating the lib file) instead of the actual one
        // (correcting the name).
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.nome = "FooBar".into();
        // Even though no `lib/FooBar.lisp` exists (would normally raise
        // MissingLib), the nome gate fires first.
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::NomeInvalid { .. }),
            "expected NomeInvalid before MissingLib, got {err:?}"
        );
    }

    #[test]
    fn nome_empty_surfaces_as_layout_violation() {
        // The empty `:nome` arm — distinct diagnostic from the generic
        // Invalid arm. Same self-locating split as
        // `MembroVersaoEmpty` / `MembroVersaoInvalid` (9888b13) and
        // `EmptyEntradaHost` / `EntradaHostInvalid` (c7d05ec).
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.nome = String::new();
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::NomeInvalid { ref reason, .. }
                if reason.contains("non-empty")),
            "got {err:?}"
        );
    }

    #[test]
    fn nome_invalid_fires_before_aplicacao_validate() {
        // A malformed `:nome` on an Aplicacao-kind caixa must surface
        // before the typed `AplicacaoSpec::validate` runs (which would
        // otherwise raise `AplicacaoViolation` for empty :membros). The
        // nome gate runs at the top of verify, before any kind-specific
        // dispatch — pin it explicitly so a future refactor that
        // reorders the gates surfaces here.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Aplicacao);
        c.nome = "FOO_BAR".into();
        // No :membros set — would normally raise AplicacaoViolation (NoMembros).
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::NomeInvalid { .. }),
            "expected NomeInvalid before AplicacaoViolation, got {err:?}"
        );
    }

    #[test]
    fn nome_valid_passes_layout_verify_through_to_kind_specific_checks() {
        // Positive control: a canonical `:nome "hello-rio"` (the most
        // common author-surface shape) passes the nome gate and reaches
        // the kind-specific checks below. Pin the happy-path so a
        // future tightening of the nome gate that accidentally rejects
        // a canonical form surfaces here.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("hello-rio.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.nome = "hello-rio".into();
        layout.verify(&c, &root).unwrap();
    }

    #[test]
    fn supervisor_with_valid_children_passes() {
        use crate::{ChildSpec, RestartPolicy, RestartStrategy};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Supervisor);
        c.estrategia = Some(RestartStrategy::OneForOne);
        c.max_restarts = Some(5);
        c.children = vec![
            ChildSpec {
                caixa: "worker".into(),
                versao: "^0.1".into(),
                restart: RestartPolicy::Permanent,
            },
            ChildSpec {
                caixa: "cache".into(),
                versao: "^0.1".into(),
                restart: RestartPolicy::Transient,
            },
        ];
        layout.verify(&c, &root).unwrap();
    }
}
