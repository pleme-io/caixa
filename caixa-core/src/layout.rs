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

        // Supervisors and Aplicacaos don't run code; reject
        // bibliotecas/exe/servicos declarations BEFORE checking those
        // paths exist (which would otherwise produce a less-helpful
        // "missing entry" error first).
        let has_code =
            !caixa.bibliotecas.is_empty() || !caixa.exe.is_empty() || !caixa.servicos.is_empty();
        if caixa.kind == CaixaKind::Supervisor && has_code {
            return Err(LayoutError::SupervisorOwnsCode(caixa.nome.clone()));
        }
        if caixa.kind == CaixaKind::Aplicacao && has_code {
            return Err(LayoutError::AplicacaoOwnsCode(caixa.nome.clone()));
        }

        // Kind ↔ slot coherence: the M3 mesh slots (:membros,
        // :contratos, :politicas, :placement, :entrada) compose the
        // typed graph of a :kind Aplicacao (MESH-COMPOSITION §III.1).
        // `Caixa::aplicacao_view` only folds them into a validatable
        // AplicacaoSpec when the kind is Aplicacao, and the
        // caixa-mesh/-flux/-helm renderers only emit them for an
        // Aplicacao — so on any *other* kind a declared mesh slot is the
        // manifest field's documented "ignored otherwise": it silently
        // passes verify and then vanishes (never validated, never
        // rendered), far from the source caixa.lisp. Reject it here —
        // before the path-existence loops — mirroring the
        // SupervisorOwnsCode / AplicacaoOwnsCode kind-coherence gates
        // above: a slot foreign to the kind is a build error, not a
        // silent drop. `declared_mesh_slots` is the single typed source
        // of the mesh-slot set + its canonical diagnostic order.
        if caixa.kind != CaixaKind::Aplicacao {
            let mesh_slots = caixa.declared_mesh_slots();
            if !mesh_slots.is_empty() {
                return Err(LayoutError::MeshSlotsOnNonAplicacao {
                    caixa: caixa.nome.clone(),
                    kind: caixa.kind,
                    slots: mesh_slots.join(" "),
                });
            }
        }

        // Kind ↔ slot coherence (mirror of the mesh-slot gate above on
        // the supervisor-tree slot set): the supervisor slots
        // (:estrategia, :max-restarts, :restart-window, :children)
        // compose the typed OTP supervisor of a :kind Supervisor
        // (INSPIRATIONS §II.2). `Caixa::supervisor_view` only folds them
        // into a validatable SupervisorSpec when the kind is Supervisor,
        // and the wasm-operator's hierarchical reconciler only consumes
        // them for one — so on any *other* kind a declared supervisor
        // slot is the manifest field's documented "ignored otherwise":
        // it silently passes verify and then vanishes (never validated,
        // never reconciled), far from the source caixa.lisp. Reject it
        // here — beside the mesh-slot gate, before the path-existence
        // loops — naming the offending kind + slot(s). `declared_
        // supervisor_slots` is the single typed source of the
        // supervisor-slot set + its canonical diagnostic order.
        if caixa.kind != CaixaKind::Supervisor {
            let supervisor_slots = caixa.declared_supervisor_slots();
            if !supervisor_slots.is_empty() {
                return Err(LayoutError::SupervisorSlotsOnNonSupervisor {
                    caixa: caixa.nome.clone(),
                    kind: caixa.kind,
                    slots: supervisor_slots.join(" "),
                });
            }
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

        // Upgrade-from entries: every entry's typed shape must hold
        // (`:from` is a valid semver; every instruction's `:module`
        // is a DNS-1123 label; every `:state-change :script` is
        // non-empty / relative / parent-escape-free) AND the
        // graph-edge-set invariant on the `:from` axis (at most one
        // entry per parsed semver — OTP appup picks at most one
        // matching block per running version, so two entries with the
        // same `:from` are an ambiguous edge), BEFORE the existing
        // path-existence pass runs, so the diagnostic names *which
        // slot* is malformed rather than the less-helpful "missing
        // upgrade-script" (which doesn't fire for non-script axes at
        // all). Mirrors the b0c8389 `BehaviorSpec::validate` wiring on
        // the peer M2 typed slot: the validate pass on the typed value
        // happens first, the on-disk path-existence pass happens
        // second.
        //
        // The duplicate-`:from` arm of [`validate_upgrade_from`]
        // closes the typed-graph-set invariant on the fifth axis to
        // get this discipline — `:children :caixa` (dbf50a9),
        // `:membros :caixa` (4bb3f3d), `:contratos` (5dbcfaf),
        // `:placement :clusters` (c7c7799), `:entrada :paths`
        // (eb3456d) are the prior four. Without it a caixa.lisp with
        // two `(:from "0.1.0" …)` blocks silently passed `feira
        // build` and the wasm-operator picked either set non-
        // deterministically at hot-upgrade time, far from the source.
        //
        // 26da2c7 closed the per-entry validate gap; this commit closes
        // the cross-entry one on the same wiring site.
        crate::upgrade::validate_upgrade_from(&caixa.upgrade_from).map_err(|err| {
            LayoutError::UpgradeViolation {
                caixa: caixa.nome.clone(),
                issue: err.to_string(),
            }
        })?;
        // Cross-slot precedence gate: every `:upgrade-from :from` must
        // be strictly less than the caixa's own `:versao` under
        // SemVer-2 precedence. An upgrade block whose `:from` is
        // greater than or equal to `:versao` is structurally
        // unreachable by the wasm-operator's `:from`-match dispatch
        // (the operator loads the current `:versao` and matches the
        // *running* version against each entry's `:from`; an entry
        // whose `:from >= :versao` is never reached because the
        // operator never runs a version >= the current one that it
        // could then upgrade *to* the current one).
        //
        // Same cross-slot value-shape discipline as the typed
        // `:placement` strategy ↔ `:shard-key` partition (934bc58):
        // one slot's value constrains the valid set of another's, and
        // the constraint becomes a structural property visible at
        // validate time. Runs *after* the per-entry shape pass + the
        // cross-entry duplicate gate so the precedence diagnostic
        // fires on already-parseable `:from`/`:versao` values (a
        // malformed `:from` surfaces as `BadFromVersion` first; a
        // malformed `:versao` falls through silently here and is
        // gated by the narrower `ManifestError::VersaoInvalid` arm
        // at its load-bearing call site). Mirrors the
        // arm-ordering posture every peer cross-axis gate uses
        // (`*_invalid_fires_before_duplicate_check` /
        // `*_takes_precedence_over_*` pins on every typed-graph axis).
        crate::upgrade::validate_upgrade_from_against_versao(&caixa.upgrade_from, &caixa.versao)
            .map_err(|err| LayoutError::UpgradeViolation {
                caixa: caixa.nome.clone(),
                issue: err.to_string(),
            })?;
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
    #[error("caixa '{caixa}' has invalid :upgrade-from entry: {issue}")]
    UpgradeViolation { caixa: String, issue: String },
    #[error("supervisor caixa '{caixa}' violates typed shape: {issue}")]
    SupervisorViolation { caixa: String, issue: String },
    #[error(
        "supervisor caixa '{0}' must not declare :bibliotecas, :exe, or :servicos — supervisors don't run code, they orchestrate other caixas"
    )]
    SupervisorOwnsCode(String),
    #[error("aplicacao caixa '{caixa}' violates typed shape: {issue}")]
    AplicacaoViolation { caixa: String, issue: String },
    #[error(
        "aplicacao caixa '{0}' must not declare :bibliotecas, :exe, or :servicos — aplicacaos compose Servicos, they don't run code themselves"
    )]
    AplicacaoOwnsCode(String),
    #[error(
        "caixa '{caixa}' is :kind {kind:?} but declares Aplicacao-only mesh slot(s): {slots} — \
         :membros / :contratos / :politicas / :placement / :entrada compose a :kind Aplicacao's \
         typed graph (MESH-COMPOSITION §III.1) and are silently ignored on every other kind \
         (never validated, never rendered); move them to a :kind Aplicacao caixa or remove them"
    )]
    MeshSlotsOnNonAplicacao {
        caixa: String,
        kind: CaixaKind,
        slots: String,
    },
    #[error(
        "caixa '{caixa}' is :kind {kind:?} but declares Supervisor-only slot(s): {slots} — \
         :estrategia / :max-restarts / :restart-window / :children compose a :kind Supervisor's \
         typed OTP supervisor (INSPIRATIONS §II.2) and are silently ignored on every other kind \
         (never validated, never reconciled); move them to a :kind Supervisor caixa or remove them"
    )]
    SupervisorSlotsOnNonSupervisor {
        caixa: String,
        kind: CaixaKind,
        slots: String,
    },
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
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == svc || p == init);
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
    fn upgrade_from_duplicate_surfaces_as_upgrade_violation() {
        // Wiring pin: the cross-entry duplicate-`:from` gate in
        // `validate_upgrade_from` lands on the same
        // `LayoutError::UpgradeViolation` axis the per-entry
        // `UpgradeFromEntry::validate` already does (26da2c7), so a
        // caixa.lisp with two `(:from "0.1.0" …)` blocks surfaces at
        // `feira build` time naming the offending caixa rather than
        // silently passing into the wasm-operator's non-deterministic
        // dispatch. Mirrors `behavior_empty_callback_is_violation` on
        // the peer M2 typed slot.
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.upgrade_from = vec![
            UpgradeFromEntry {
                from: "0.1.0".into(),
                instructions: vec![UpgradeInstruction::Restart],
            },
            UpgradeFromEntry {
                from: "0.1.0".into(),
                instructions: vec![UpgradeInstruction::Restart],
            },
        ];
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest || p == svc);
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::UpgradeViolation { caixa, issue } = err else {
            panic!("expected LayoutError::UpgradeViolation for duplicate `:from`, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("0.1.0"),
            "UpgradeViolation issue must name the offending `:from` verbatim, got {issue:?}"
        );
    }

    #[test]
    fn upgrade_from_downgrade_surfaces_as_upgrade_violation() {
        // Wiring pin: the cross-slot precedence gate in
        // `validate_upgrade_from_against_versao` lands on the same
        // `LayoutError::UpgradeViolation` axis the per-entry and
        // cross-entry gates already do (26da2c7, 7c6aef2), so a
        // caixa.lisp whose `:upgrade-from :from` is greater than the
        // caixa's own `:versao` surfaces at `feira build` time
        // naming the offending caixa rather than silently passing
        // into the wasm-operator's `:from`-match dispatch where the
        // entry would sit dormant forever. Mirrors
        // `upgrade_from_duplicate_surfaces_as_upgrade_violation` on
        // the peer cross-entry gate.
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let mut c = caixa(CaixaKind::Servico);
        c.versao = "0.1.5".into();
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.2.0".into(),
            instructions: vec![UpgradeInstruction::Restart],
        }];
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest || p == svc);
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::UpgradeViolation { caixa, issue } = err else {
            panic!(
                "expected LayoutError::UpgradeViolation for downgrade-shaped `:from`, got {err:?}"
            );
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("0.2.0") && issue.contains("0.1.5"),
            "UpgradeViolation issue must name both `:from` and `:versao` verbatim, got {issue:?}"
        );
    }

    #[test]
    fn upgrade_from_equal_to_versao_surfaces_as_upgrade_violation() {
        // Self-upgrade no-op arm: `:from "0.1.0"` while
        // `:versao "0.1.0"` declares "upgrade from myself to
        // myself", which the operator's dispatch either skips
        // silently or trivially "succeeds" with no observable
        // transition. Surfaces at validate time naming both values
        // so the author can fix in one edit.
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let mut c = caixa(CaixaKind::Servico);
        c.versao = "0.1.0".into();
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![UpgradeInstruction::Restart],
        }];
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest || p == svc);
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::UpgradeViolation { caixa, issue } = err else {
            panic!(
                "expected LayoutError::UpgradeViolation for self-upgrade `:from == :versao`, got \
                 {err:?}"
            );
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("0.1.0"),
            "UpgradeViolation issue must name the equal `:from`/`:versao` verbatim, got {issue:?}"
        );
    }

    #[test]
    fn upgrade_from_strict_upgrade_passes_layout() {
        // Positive control for the precedence gate at the
        // LayoutInvariants level: a valid `:from < :versao` chain
        // (`0.1.0 → 0.2.0`) must not regress into a false-positive
        // `UpgradeViolation`. Mirrors `behavior_callback_path_must_exist`'s
        // positive-control arm.
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let mut c = caixa(CaixaKind::Servico);
        c.versao = "0.2.0".into();
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![UpgradeInstruction::Restart],
        }];
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest || p == svc);
        layout.verify(&c, &root).unwrap();
    }

    #[test]
    fn upgrade_script_path_must_exist() {
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        use std::path::PathBuf;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let mut c = caixa(CaixaKind::Servico);
        // `:versao` past the entry's `:from` so the cross-slot
        // precedence gate (`FromNotBeforeVersao`) lets this case
        // through to the path-existence pass under test.
        c.versao = "0.2.0".into();
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
    fn mesh_slots_on_servico_rejected() {
        // The canonical real-world footgun: an author adds :entrada to a
        // :kind Servico expecting it to expose ingress. aplicacao_view
        // returns None for Servico, so the slot is the manifest's
        // "ignored otherwise" — never validated, never rendered. The
        // kind-coherence gate rejects it at build time (before the
        // :servicos existence loop), naming the offending slot + kind.
        use crate::Entrada;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.entrada = Some(Entrada {
            host: "demo.example.com".into(),
            para: "demo".into(),
            paths: vec![],
            port: 8080,
        });
        let err = layout.verify(&c, &root).unwrap_err();
        match err {
            LayoutError::MeshSlotsOnNonAplicacao { caixa, kind, slots } => {
                assert_eq!(caixa, "demo");
                assert_eq!(kind, CaixaKind::Servico);
                assert_eq!(slots, ":entrada");
            }
            other => panic!("expected MeshSlotsOnNonAplicacao, got {other:?}"),
        }
    }

    #[test]
    fn mesh_slots_on_non_aplicacao_lists_slots_in_canonical_order() {
        // All five mesh slots declared on a Biblioteca → the diagnostic
        // enumerates them in canonical declaration order, deterministic
        // across runs. The gate fires on declared-ness only (the values
        // need not be a *valid* AplicacaoSpec — aplicacao_view is never
        // called for a non-Aplicacao kind).
        use crate::{Entrada, Membro, MeshPolicy, Placement, PlacementStrategy, WitContract};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.membros = vec![Membro {
            caixa: "a".into(),
            versao: "^0.1".into(),
        }];
        c.contratos = vec![WitContract {
            de: "a".into(),
            para: "a".into(),
            wit: "wasi:http/proxy".into(),
            endpoint: Some("/x".into()),
            subject: None,
            slot: None,
        }];
        c.politicas = Some(MeshPolicy::default());
        c.placement = Some(Placement {
            estrategia: PlacementStrategy::Replicated,
            clusters: vec!["rio".into()],
            affinity: None,
            shard_key: None,
        });
        c.entrada = Some(Entrada {
            host: "x.example.com".into(),
            para: "a".into(),
            paths: vec![],
            port: 8080,
        });
        let err = layout.verify(&c, &root).unwrap_err();
        match err {
            LayoutError::MeshSlotsOnNonAplicacao { slots, .. } => {
                assert_eq!(slots, ":membros :contratos :politicas :placement :entrada");
            }
            other => panic!("expected MeshSlotsOnNonAplicacao, got {other:?}"),
        }
    }

    #[test]
    fn servico_without_mesh_slots_still_verifies() {
        // Pass-after control: a well-formed Servico carrying no mesh
        // slots must remain accepted — the gate keys off declared-ness,
        // so it must not over-fire on the common case.
        let root = PathBuf::from("/tmp/x");
        let servico = root.join("servicos/demo.computeunit.yaml");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest || p == servico);
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        layout.verify(&c, &root).unwrap();
    }

    #[test]
    fn supervisor_slots_on_servico_rejected() {
        // Mirror of `mesh_slots_on_servico_rejected` on the
        // supervisor-tree slot set: an author adds `:children` to a
        // `:kind Servico` expecting it to spawn workers. supervisor_view
        // returns None for Servico, so the slot is the manifest's
        // "ignored otherwise" — never validated, never reconciled. The
        // kind-coherence gate rejects it at build time (before the
        // :servicos existence loop), naming the offending slot + kind.
        use crate::{ChildSpec, RestartPolicy};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.children = vec![ChildSpec {
            caixa: "worker".into(),
            versao: "^0.1".into(),
            restart: RestartPolicy::Permanent,
        }];
        let err = layout.verify(&c, &root).unwrap_err();
        match err {
            LayoutError::SupervisorSlotsOnNonSupervisor { caixa, kind, slots } => {
                assert_eq!(caixa, "demo");
                assert_eq!(kind, CaixaKind::Servico);
                assert_eq!(slots, ":children");
            }
            other => panic!("expected SupervisorSlotsOnNonSupervisor, got {other:?}"),
        }
    }

    #[test]
    fn supervisor_slots_on_non_supervisor_lists_slots_in_canonical_order() {
        // All four supervisor slots declared on a Biblioteca → the
        // diagnostic enumerates them in canonical declaration order
        // (`:estrategia` → `:max-restarts` → `:restart-window` →
        // `:children`), deterministic across runs. The gate fires on
        // declared-ness only (the values need not be a *valid*
        // SupervisorSpec — supervisor_view is never called for a
        // non-Supervisor kind). Mirror of
        // `mesh_slots_on_non_aplicacao_lists_slots_in_canonical_order`.
        use crate::{ChildSpec, RestartPolicy, RestartStrategy};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.estrategia = Some(RestartStrategy::OneForOne);
        c.max_restarts = Some(5);
        c.restart_window = Some("60s".into());
        c.children = vec![ChildSpec {
            caixa: "worker".into(),
            versao: "^0.1".into(),
            restart: RestartPolicy::Permanent,
        }];
        let err = layout.verify(&c, &root).unwrap_err();
        match err {
            LayoutError::SupervisorSlotsOnNonSupervisor { slots, .. } => {
                assert_eq!(slots, ":estrategia :max-restarts :restart-window :children");
            }
            other => panic!("expected SupervisorSlotsOnNonSupervisor, got {other:?}"),
        }
    }

    #[test]
    fn aplicacao_with_supervisor_slots_rejected() {
        // Cross-kind pin: an Aplicacao (the other no-code orchestrator
        // kind) that declares a supervisor slot is rejected by the
        // supervisor-slot gate, just as a Supervisor declaring a mesh
        // slot is rejected by the mesh-slot gate — the two kind ↔ slot
        // coherence gates are symmetric and mutually exclusive. The
        // gate fires before the Aplicacao typed-graph validation, so
        // the diagnostic names the foreign supervisor slot rather than
        // a downstream AplicacaoViolation.
        use crate::{Membro, Placement, PlacementStrategy, RestartStrategy};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Aplicacao);
        c.membros = vec![Membro {
            caixa: "service-a".into(),
            versao: "^0.1".into(),
        }];
        c.placement = Some(Placement {
            estrategia: PlacementStrategy::Replicated,
            clusters: vec!["rio".into()],
            affinity: None,
            shard_key: None,
        });
        c.estrategia = Some(RestartStrategy::OneForAll);
        let err = layout.verify(&c, &root).unwrap_err();
        match err {
            LayoutError::SupervisorSlotsOnNonSupervisor { caixa, kind, slots } => {
                assert_eq!(caixa, "demo");
                assert_eq!(kind, CaixaKind::Aplicacao);
                assert_eq!(slots, ":estrategia");
            }
            other => panic!("expected SupervisorSlotsOnNonSupervisor, got {other:?}"),
        }
    }

    #[test]
    fn servico_without_supervisor_slots_still_verifies() {
        // Pass-after control: a well-formed Servico carrying no
        // supervisor slots must remain accepted — the gate keys off
        // declared-ness, so it must not over-fire on the common case.
        let root = PathBuf::from("/tmp/x");
        let servico = root.join("servicos/demo.computeunit.yaml");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest || p == servico);
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
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

    // ── :upgrade-from entry validation pipes through layout ─────────────

    #[test]
    fn upgrade_invalid_module_surfaces_as_layout_violation() {
        // End-to-end pin that
        // [`crate::UpgradeFromEntry::validate`] runs *inside*
        // `LayoutInvariants::verify` and surfaces value-shape
        // violations through the new `UpgradeViolation` arm
        // (parallel to `BehaviorViolation`, `LimitsViolation`,
        // `SupervisorViolation`, `AplicacaoViolation`). Until this
        // wiring landed the entry validator was unreachable from any
        // build-pipeline caller — an `:upgrade-from
        // ((:from "0.1.0" :instructions ((:load-module "Hello")))` (uppercase
        // module name the K8s apiserver would reject on the per-
        // ComputeUnit `metadata.name` axis) silently passed
        // `feira lint` / `feira build` and surfaced only at wasm-engine
        // hot-upgrade time as a per-backend "module not found" /
        // `code:load_module/1` `badarg` runtime error, far from the
        // source caixa.lisp. Pinning the wiring here so a future
        // refactor that drops the `entry.validate()` call surfaces as
        // a build-pipeline regression at this test, not as a runtime
        // surprise per consumer.
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        use std::path::PathBuf;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![UpgradeInstruction::LoadModule {
                module: "Hello".into(), // uppercase — not DNS-1123
            }],
        }];
        let manifest_clone = manifest.clone();
        let svc_clone = svc.clone();
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest_clone || p == svc_clone);
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::UpgradeViolation { caixa, issue } = err else {
            panic!("expected UpgradeViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":load-module"),
            "issue must name the lisp-form of the offending instruction: {issue}"
        );
        assert!(
            issue.contains("Hello"),
            "issue must name the offending :module verbatim: {issue}"
        );
    }

    #[test]
    fn upgrade_empty_module_surfaces_as_layout_violation() {
        // Companion to the DNS-1123 footgun above on the narrower
        // empty arm. Every Module-bearing variant's empty value
        // reaches the layout pipeline through the kind-tagged
        // `ModuleEmpty` diagnostic naming its lisp-form.
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        use std::path::PathBuf;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![UpgradeInstruction::SoftPurge {
                module: String::new(),
            }],
        }];
        let manifest_clone = manifest.clone();
        let svc_clone = svc.clone();
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest_clone || p == svc_clone);
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::UpgradeViolation { caixa, issue } = err else {
            panic!("expected UpgradeViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":soft-purge"),
            "issue must name the lisp-form of the empty instruction: {issue}"
        );
    }

    #[test]
    fn upgrade_invalid_state_change_script_surfaces_as_layout_violation() {
        // Pins that the b0c8389 script value-shape gates
        // (AbsoluteScript / ParentEscapeScript) — previously
        // unreachable from any build-pipeline caller — now fire
        // through the same `UpgradeViolation` arm before the path-
        // existence pass would otherwise emit the less-helpful
        // "missing upgrade-script" (or, worse, *succeed* against
        // /etc/passwd, proving the sandbox bypass — same defect
        // the b0c8389 BehaviorSpec wiring closed on the peer M2
        // slot).
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        use std::path::PathBuf;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let etc_passwd = PathBuf::from("/etc/passwd");
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![UpgradeInstruction::StateChange {
                script: PathBuf::from("/etc/passwd"),
            }],
        }];
        let manifest_clone = manifest.clone();
        let svc_clone = svc.clone();
        let etc_passwd_clone = etc_passwd.clone();
        // Critically: /etc/passwd "exists" in our mock — without the
        // value-shape pre-check, the existence loop would *succeed*
        // and the path-traversal exit from the project sandbox would
        // pass `feira build` silently.
        let layout = StandardLayout::new().with_path_exists(move |p| {
            p == manifest_clone || p == svc_clone || p == etc_passwd_clone
        });
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::UpgradeViolation { caixa, issue } = err else {
            panic!("expected UpgradeViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("absolute") || issue.contains("Absolute"),
            "issue must name the violation kind (absolute): {issue}"
        );
    }

    #[test]
    fn upgrade_well_formed_passes_layout() {
        // Positive control — every documented authoring shape
        // (`:load-module`, `:state-change` with a relative path,
        // `:soft-purge`, `:purge`, sole `:restart`) passes the wired
        // gate. The typed sequence (`:load-module` → `:state-change`
        // → `:soft-purge` → `:purge`) lives in one entry; the sole
        // `:restart` fallback lives in a *separate* entry on a
        // different `:from` (the within-entry restart-exclusivity
        // gate added in this commit rejects mixing the fallback with
        // the typed sequence — per the UpgradeInstruction::Restart
        // doc, `:restart` is terminal and any other instructions in
        // the same entry are dead code). Drift here = a future
        // tighten that rejects any canonical shape surfaces as a
        // regression at this layout-level pin, not piecemeal across
        // per-renderer call sites.
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        use std::path::PathBuf;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let migration = root.join("lib/migrations/v01-to-v02.lisp");
        let mut c = caixa(CaixaKind::Servico);
        // `:versao` past both entries' `:from` so the cross-slot
        // precedence gate (`FromNotBeforeVersao`) lets this canonical
        // authoring shape through to the positive-control assertion.
        c.versao = "0.2.0".into();
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.upgrade_from = vec![
            UpgradeFromEntry {
                from: "0.1.0".into(),
                instructions: vec![
                    UpgradeInstruction::LoadModule {
                        module: "hello-rio".into(),
                    },
                    UpgradeInstruction::StateChange {
                        script: PathBuf::from("lib/migrations/v01-to-v02.lisp"),
                    },
                    UpgradeInstruction::SoftPurge {
                        module: "hello-rio-old".into(),
                    },
                    UpgradeInstruction::Purge {
                        module: "hello-rio-old".into(),
                    },
                ],
            },
            UpgradeFromEntry {
                from: "0.0.9".into(),
                instructions: vec![UpgradeInstruction::Restart],
            },
        ];
        let manifest_clone = manifest.clone();
        let svc_clone = svc.clone();
        let migration_clone = migration.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| {
            p == manifest_clone || p == svc_clone || p == migration_clone
        });
        layout.verify(&c, &root).unwrap();
    }

    #[test]
    fn upgrade_from_restart_mixed_surfaces_as_upgrade_violation() {
        // Wiring pin: the within-entry `(:restart)`-exclusivity gate
        // (`UpgradeFromEntry::validate_restart_exclusive`) lands on
        // the same `LayoutError::UpgradeViolation` axis the per-entry
        // shape gate (26da2c7), the cross-entry duplicate-`:from`
        // gate (7c6aef2), and the cross-slot `:from < :versao`
        // precedence gate (de7ab1a) already do. A caixa.lisp whose
        // `:upgrade-from` entry mixes `(:restart)` with a typed
        // instruction surfaces at `feira build` time naming the
        // offending caixa + the entry's `:from` rather than silently
        // passing into the wasm-operator with semantically dead code
        // in the operator's dispatch table. Mirrors
        // `upgrade_from_duplicate_surfaces_as_upgrade_violation` on
        // the peer cross-entry gate.
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let mut c = caixa(CaixaKind::Servico);
        c.versao = "0.2.0".into();
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![
                UpgradeInstruction::LoadModule {
                    module: "hello-rio".into(),
                },
                UpgradeInstruction::Restart,
            ],
        }];
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest || p == svc);
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::UpgradeViolation { caixa, issue } = err else {
            panic!("expected LayoutError::UpgradeViolation for restart-mixed entry, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("0.1.0"),
            "UpgradeViolation issue must name the offending entry's `:from` verbatim, got \
             {issue:?}"
        );
        assert!(
            issue.contains(":restart"),
            "UpgradeViolation issue must name the `:restart` axis verbatim, got {issue:?}"
        );
        assert!(
            issue.contains(":load-module"),
            "UpgradeViolation issue must name the non-:restart peer instruction's lisp-form \
             verbatim, got {issue:?}"
        );
    }

    #[test]
    fn upgrade_from_restart_duplicated_surfaces_as_upgrade_violation() {
        // Companion arm: the duplicate-`(:restart)` mode of
        // `RestartNotExclusive` (no typed peers, just multiple
        // `Restart` variants) surfaces through the same wiring as the
        // mixed-with-typed mode above. The diagnostic still names the
        // offending entry's `:from` verbatim even when `other_kinds`
        // is empty.
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let mut c = caixa(CaixaKind::Servico);
        c.versao = "0.2.0".into();
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![UpgradeInstruction::Restart, UpgradeInstruction::Restart],
        }];
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest || p == svc);
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::UpgradeViolation { caixa, issue } = err else {
            panic!(
                "expected LayoutError::UpgradeViolation for duplicate-restart entry, got \
                 {err:?}"
            );
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("0.1.0"),
            "UpgradeViolation issue must name the offending entry's `:from` verbatim, got \
             {issue:?}"
        );
        assert!(
            issue.contains("(:restart)") || issue.contains(":restart"),
            "UpgradeViolation issue must name the `:restart` axis verbatim, got {issue:?}"
        );
    }

    #[test]
    fn upgrade_bad_from_version_surfaces_as_layout_violation() {
        // The `:from` semver gate (`UpgradeError::BadFromVersion`)
        // was likewise unreachable before this wiring landed — a
        // typo-shaped `:from "v0.1.0"` (git-tag-shape leaking into
        // the semver slot) silently passed `feira build` and
        // surfaced only when the operator's hot-upgrade decision
        // engine tried to match against the version key it couldn't
        // parse. Now wired through `UpgradeViolation`.
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        use std::path::PathBuf;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "v0.1.0".into(), // git-tag-shape, not semver
            instructions: vec![UpgradeInstruction::Restart],
        }];
        let manifest_clone = manifest.clone();
        let svc_clone = svc.clone();
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest_clone || p == svc_clone);
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::UpgradeViolation { caixa, issue } = err else {
            panic!("expected UpgradeViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("v0.1.0") || issue.contains(":from"),
            "issue must name the offending :from value or slot: {issue}"
        );
    }
}
