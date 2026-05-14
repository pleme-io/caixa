use serde::{Deserialize, Serialize};
use tatara_lisp::DeriveTataraDomain;

use std::time::Duration;

use crate::{
    behavior::BehaviorSpec, dep::DepError, limits::LimitsSpec, supervisor::SupervisorSpec,
    upgrade::{UpgradeError, UpgradeFromEntry},
    CaixaKind, Dep,
};

/// Inline duration parser for `restart_window`. Mirrors
/// `supervisor::duration_codec::parse` but keeps the typed Caixa lib
/// minimal (one tiny shared parser).
fn parse_window_inline(s: &str) -> Option<Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let split = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let num: f64 = num.trim().parse().ok()?;
    if num < 0.0 {
        return None;
    }
    Some(match unit.trim() {
        "ms" => Duration::from_secs_f64(num / 1000.0),
        "s" | "" => Duration::from_secs_f64(num),
        "m" => Duration::from_secs_f64(num * 60.0),
        "h" => Duration::from_secs_f64(num * 3600.0),
        _ => return None,
    })
}

/// Top-level manifest for a caixa (a tatara-lisp package).
///
/// Authored as `caixa.lisp`:
///
/// ```lisp
/// (defcaixa
///   :nome        "pangea-tatara-aws"
///   :versao      "0.1.0"
///   :kind        Biblioteca
///   :edicao      "2026"
///   :descricao   "AWS provider caixa for tatara-lisp"
///   :repositorio "github:pleme-io/pangea-tatara-aws"
///   :licenca     "MIT"
///   :autores     ("pleme-io")
///   :etiquetas   ("iac" "aws" "pangea")
///   :deps        ((:nome "caixa-teia"    :versao "^0.1")
///                 (:nome "iac-forge-ir"  :versao "^0.5"))
///   :deps-dev    ((:nome "tatara-check"  :versao "*"))
///   :bibliotecas ("lib/pangea-tatara-aws.lisp"))
/// ```
///
/// Because `Caixa` derives [`tatara_lisp::domain::TataraDomain`], the manifest
/// is parsed directly by the tatara-lisp compiler — an ill-formed manifest is
/// a compile error, not a runtime error.
#[derive(DeriveTataraDomain, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defcaixa")]
pub struct Caixa {
    /// Package name — the canonical string used in `:deps`, the registry, and
    /// the default lib/exe entry names.
    pub nome: String,

    /// Package version — a semver literal like `"0.1.0"`. Parsed lazily via
    /// [`crate::CaixaVersion::parse`].
    pub versao: String,

    /// What this caixa produces. See [`CaixaKind`].
    pub kind: CaixaKind,

    /// Language edition — determines macro surface + compatibility flags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edicao: Option<String>,

    /// Free-form description shown in the registry listing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descricao: Option<String>,

    /// Homepage or repo URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repositorio: Option<String>,

    /// SPDX license expression — `"MIT"`, `"Apache-2.0 OR MIT"`, etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licenca: Option<String>,

    /// Authors — free-form strings.
    #[serde(default)]
    pub autores: Vec<String>,

    /// Topical tags used for registry search.
    #[serde(default)]
    pub etiquetas: Vec<String>,

    /// Runtime dependencies.
    #[serde(default)]
    pub deps: Vec<Dep>,

    /// Development-only dependencies (tests, lint, bench).
    #[serde(default)]
    pub deps_dev: Vec<Dep>,

    /// Paths to executable entry points (relative to the package root).
    /// Required when `:kind Binario`.
    #[serde(default)]
    pub exe: Vec<String>,

    /// Paths to library entry points (relative to the package root).
    /// First entry is the canonical `lib/<nome>.lisp`; when omitted under
    /// `:kind Biblioteca`, the layout check expects `lib/<nome>.lisp`.
    #[serde(default)]
    pub bibliotecas: Vec<String>,

    /// Paths to service manifests (relative to the package root).
    /// Required when `:kind Servico`.
    #[serde(default)]
    pub servicos: Vec<String>,

    // ── M2 typed-substrate extensions per theory/ABSORPTION-ROADMAP.md ──
    //
    // All four are optional + default to "absent"; existing caixas
    // round-trip unchanged. Each maps onto a prior-art primitive named
    // in theory/INSPIRATIONS.md:
    //
    //   :limits        — Lunatic per-process limits (§III.1)
    //   :behavior      — OTP gen_server callbacks  (§II.3)
    //   :upgrade-from  — OTP appup migration       (§II.4)
    //   :estrategia    — OTP supervisor strategy   (§II.2 + §III.2)
    //   :children      — OTP supervisor children    (§II.2 + §III.2)
    //
    // The supervisor slots are flat on Caixa (vs nested under a
    // SupervisorSpec sub-form) to keep tatara-lisp authoring at one
    // level of nesting; SupervisorSpec exists for validation +
    // composition convenience (`Caixa::supervisor_view()`).

    /// Lunatic-style per-process resource limits. None = unbounded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<LimitsSpec>,

    /// OTP-shaped behavior callbacks for Servico-kind caixas.
    /// Authored as `(:on-init "..." :on-call "..." …)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behavior: Option<BehaviorSpec>,

    /// OTP appup — declarative upgrade instructions per prior version.
    /// Empty list = no hot-upgrade path declared (caller falls back to
    /// `:Restart` strategy).
    #[serde(default)]
    pub upgrade_from: Vec<UpgradeFromEntry>,

    /// OTP supervisor strategy. Required when `:kind Supervisor`;
    /// ignored otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estrategia: Option<crate::supervisor::RestartStrategy>,

    /// Max restarts before the supervisor itself fails. Defaults via
    /// SupervisorSpec at validation time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_restarts: Option<u32>,

    /// Sliding window for `max_restarts`. Authored as a duration
    /// string (`"60s"`, `"5m"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart_window: Option<String>,

    /// Static children of a supervisor. Required for OneForOne /
    /// OneForAll / RestForOne; must be empty for SimpleOneForOne.
    #[serde(default)]
    pub children: Vec<crate::supervisor::ChildSpec>,

    // ── M3 Aplicacao slots (theory/MESH-COMPOSITION.md) ─────────────────
    //
    // Required when :kind Aplicacao; ignored otherwise.
    // Composed into a typed AplicacaoSpec via Caixa::aplicacao_view().

    /// Member Servicos that make up this Aplicacao. Each is a
    /// caixa-name + version-constraint pair. Required for Aplicacao.
    #[serde(default)]
    pub membros: Vec<crate::aplicacao::Membro>,

    /// WIT-typed inter-Servico contracts. Each `:de` and `:para`
    /// must reference a name in `:membros`.
    #[serde(default)]
    pub contratos: Vec<crate::aplicacao::WitContract>,

    /// Mesh-level policies (timeout, retries, circuit-breaker, mTLS,
    /// rate-limit). Apply to every contrato unless overridden per-edge
    /// in M4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub politicas: Option<crate::aplicacao::MeshPolicy>,

    /// Placement strategy across the cluster fleet
    /// (single-node | replicated | sharded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<crate::aplicacao::Placement>,

    /// External entry point — gateway / ingress shape. Optional;
    /// only for public Aplicacaos.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrada: Option<crate::aplicacao::Entrada>,
}

impl Caixa {
    /// Parse a `caixa.lisp` source string to a typed `Caixa`.
    ///
    /// Delegates to the TataraDomain derive; the first top-level form must be
    /// `(defcaixa …)` — any other shape is an error.
    pub fn from_lisp(src: &str) -> Result<Self, tatara_lisp::LispError> {
        use tatara_lisp::domain::TataraDomain;
        let forms = tatara_lisp::read(src)?;
        let first = forms
            .first()
            .ok_or_else(|| tatara_lisp::LispError::Compile {
                form: "defcaixa".into(),
                message: "empty manifest".into(),
            })?;
        Self::compile_from_sexp(first)
    }

    /// Register `Caixa` with the global tatara-lisp domain registry so
    /// `defcaixa` is dispatchable from any tatara-lisp binary that seeds
    /// the registry (e.g. `tatara-check`).
    pub fn register() {
        tatara_lisp::domain::register::<Self>();
    }

    /// Compose the Aplicacao-related flat slots into a single typed
    /// [`crate::aplicacao::AplicacaoSpec`] for validation +
    /// downstream renderer consumption. Returns `None` when the
    /// caixa isn't a `:kind Aplicacao`.
    #[must_use]
    pub fn aplicacao_view(&self) -> Option<crate::aplicacao::AplicacaoSpec> {
        if self.kind != CaixaKind::Aplicacao {
            return None;
        }
        Some(crate::aplicacao::AplicacaoSpec {
            membros: self.membros.clone(),
            contratos: self.contratos.clone(),
            politicas: self.politicas.clone().unwrap_or_default(),
            placement: self.placement.clone().unwrap_or_default(),
            entrada: self.entrada.clone(),
        })
    }

    /// Validate every entry of `:deps` and `:deps-dev` through
    /// [`Dep::validate`] — closing the parity loop with the per-axis
    /// `:versao` gates already wired into the typed-graph
    /// ([`crate::AplicacaoSpec::validate_membros`] for `:membros`,
    /// 9888b13) and typed supervisor tree
    /// ([`crate::SupervisorSpec::validate`] for `:children`, b38ff3a).
    ///
    /// Until this gate landed `:deps :versao` and `:deps-dev :versao`
    /// were the only `:versao` axes still untyped past
    /// [`Caixa::from_lisp`]: the derive macro stored the requirement
    /// as a String without parsing it, so a malformed-but-non-empty
    /// requirement (`"^bad-version"`, `"^^0.1"`, `"v0.1"`, `"not-a-req"`)
    /// silently passed parse and the `semver::Error` surfaced at
    /// lacre-resolve time, far from the source caixa.lisp, with no
    /// field naming which `:deps` entry carried the typo. Lifting the
    /// gate here makes the four `:versao` typed surfaces (`:deps`,
    /// `:deps-dev`, `:membros`, `:children`) structurally equivalent —
    /// every requirement string past `validate_deps` is round-trippable
    /// through [`crate::parse_requirement`] without re-checking at the
    /// resolver layer.
    ///
    /// Both lists run through the same per-entry validator so a typo
    /// in `:deps-dev` surfaces with the same diagnostic as one in
    /// `:deps` — neither axis is a second-class citizen of the typed
    /// surface.
    pub fn validate_deps(&self) -> Result<(), DepError> {
        for dep in &self.deps {
            dep.validate()?;
        }
        for dep in &self.deps_dev {
            dep.validate()?;
        }
        Ok(())
    }

    /// Validate every `:upgrade-from` entry through
    /// [`UpgradeFromEntry::validate`] *and* reject duplicate `:from`
    /// values across the whole list — closing the only remaining M2
    /// typed-slot validation gap.
    ///
    /// Until this gate landed `:upgrade-from` was the lone M2 slot whose
    /// per-entry [`UpgradeFromEntry::validate`] (which enforces
    /// semver-parseable `:from`, non-empty `LoadModule`/`SoftPurge`/`Purge`
    /// `:module`, and the value-shape gates `EmptyScript` / `AbsoluteScript`
    /// / `ParentEscapeScript` on every `StateChange` `:script`) was *defined*
    /// but never reached from any caller: [`crate::layout::StandardLayout::verify`]
    /// only iterated `caixa.upgrade_from` to check `:script` path existence
    /// via `instr.declared_path()`, so a malformed `:from` ("not-a-semver",
    /// "v0.1"), an empty `:module`, or an absolute `:script` ("/etc/foo.lisp"
    /// — the same `Path::join` sandbox-bypass closed for `:behavior` paths
    /// in b0c8389) all silently passed validate and the diagnostic surfaced
    /// far downstream (semver::Error at lacre-resolve time; a confusing
    /// `MissingEntry { kind: "upgrade-script", path: /etc/foo.lisp }` whose
    /// path is *outside* the caixa root; or worst, a successful build when
    /// the absolute path happens to exist on the host). Wiring the per-entry
    /// validator through here makes the four M2 typed-slot validate() entry
    /// points (`:limits`, `:behavior`, `:upgrade-from`, supervisor's
    /// `:children`) and the M3 mesh ones (`:membros`, `:contratos`,
    /// `:politicas`, `:placement`, `:entrada`) structurally equivalent —
    /// every typed slot's `validate()` is reachable from a single layout
    /// entry point, and a regression at any axis surfaces at build time
    /// with a diagnostic naming the offending slot.
    ///
    /// On top of the per-entry pass: duplicate `:from` values across the
    /// `Vec<UpgradeFromEntry>` are themselves a typed-set discipline
    /// violation — the same graph-node-set / multiset distinction closed
    /// for `:membros` (4bb3f3d), `:placement :clusters` (c7c7799),
    /// `:entrada :paths` (eb3456d), `:children` (dbf50a9), and
    /// `:contratos` (5dbcfaf). OTP appup's per-version instruction list
    /// is a *function* of the prior version, not a multimap (`.appup`
    /// files reject `[{"0.1.0", […]}, {"0.1.0", […]}]` at the
    /// `release_handler` boundary): two entries sharing a `:from` would
    /// force the wasm-operator to pick one nondeterministically at
    /// hot-upgrade time. Surface the collision at build time.
    ///
    /// Iteration is in declaration order so a multi-malformed manifest
    /// surfaces the first offending entry deterministically — same
    /// trajectory as `validate_deps`'s "runtime deps first, dev deps
    /// second" pin (the runtime axis is load-bearing; surface it first).
    pub fn validate_upgrade_from(&self) -> Result<(), UpgradeError> {
        let mut seen = std::collections::HashSet::new();
        for entry in &self.upgrade_from {
            entry.validate()?;
            if !seen.insert(entry.from.as_str()) {
                return Err(UpgradeError::DuplicateFrom {
                    from: entry.from.clone(),
                });
            }
        }
        Ok(())
    }

    /// Compose the supervisor-related flat slots into a single
    /// [`SupervisorSpec`] for validation. Returns `None` when the
    /// caixa isn't a `:kind Supervisor`.
    ///
    /// The flat representation in [`Caixa`] keeps tatara-lisp authoring
    /// simple (one form, no nested `:supervisor (…)` block); this view
    /// is the "typed shape" the operator + supervisor reconciler
    /// consume.
    #[must_use]
    pub fn supervisor_view(&self) -> Option<SupervisorSpec> {
        if self.kind != CaixaKind::Supervisor {
            return None;
        }
        let restart_window = self
            .restart_window
            .as_deref()
            .and_then(parse_window_inline);
        Some(SupervisorSpec {
            estrategia: self.estrategia.unwrap_or_default(),
            max_restarts: self.max_restarts.unwrap_or(5),
            restart_window,
            children: self.children.clone(),
        })
    }

    /// A minimal starter manifest emitted by `feira init`.
    #[must_use]
    pub fn template(nome: &str) -> String {
        format!(
            "(defcaixa\n  \
               :nome        {nome:?}\n  \
               :versao      \"0.1.0\"\n  \
               :kind        Biblioteca\n  \
               :edicao      \"2026\"\n  \
               :descricao   \"FIXME — describe this caixa\"\n  \
               :autores     ()\n  \
               :etiquetas   ()\n  \
               :deps        ()\n  \
               :deps-dev    ()\n  \
               :bibliotecas (\"lib/{nome}.lisp\"))\n"
        )
    }

    /// Serialize to a canonical `caixa.lisp` source — suitable for writing
    /// back after mutation (e.g. `feira add`).
    ///
    /// Goes through serde JSON → canonical Sexp → per-field pretty print.
    /// The derive-macro `compile_from_sexp` path is the inverse, so any
    /// `Caixa` round-trips through `to_lisp` + `from_lisp`.
    #[must_use]
    pub fn to_lisp(&self) -> String {
        let json = serde_json::to_value(self).expect("Caixa serialize");
        let sexp = tatara_lisp::domain::json_to_sexp(&json);
        let tatara_lisp::Sexp::List(items) = sexp else {
            return format!("(defcaixa {sexp})\n");
        };
        let mut out = String::from("(defcaixa");
        let mut i = 0;
        while i + 1 < items.len() {
            out.push_str("\n  ");
            out.push_str(&items[i].to_string());
            out.push(' ');
            out.push_str(&items[i + 1].to_string());
            i += 2;
        }
        out.push_str(")\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_round_trips() {
        let src = Caixa::template("demo");
        let c = Caixa::from_lisp(&src).expect("template must parse");
        assert_eq!(c.nome, "demo");
        assert_eq!(c.versao, "0.1.0");
        assert_eq!(c.kind, CaixaKind::Biblioteca);
        assert_eq!(c.bibliotecas, vec!["lib/demo.lisp".to_string()]);
        assert!(c.deps.is_empty());
        assert!(c.deps_dev.is_empty());
    }

    #[test]
    fn register_populates_registry() {
        Caixa::register();
        let kws = tatara_lisp::domain::registered_keywords();
        assert!(kws.contains(&"defcaixa"));
    }

    #[test]
    fn to_lisp_round_trips() {
        let src = Caixa::template("demo");
        let c1 = Caixa::from_lisp(&src).unwrap();
        let emitted = c1.to_lisp();
        let c2 = Caixa::from_lisp(&emitted).expect("emitted lisp parses back");
        assert_eq!(c1, c2);
    }

    // ── M2 typed-substrate slot tests (limits, behavior, upgrade-from, supervisor) ──

    #[test]
    fn limits_round_trip_via_json() {
        use crate::LimitsSpec;
        use std::time::Duration;
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.limits = Some(LimitsSpec {
            memory: Some(64 * 1024 * 1024),
            fuel: Some(1_000_000),
            wall_clock: Some(Duration::from_secs(30)),
            cpu: Some(500),
        });
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"limits\""));
        assert!(json.contains("\"64MiB\""));
        assert!(json.contains("\"30s\""));
        assert!(json.contains("\"500m\""));
        let back: Caixa = serde_json::from_str(&json).unwrap();
        assert_eq!(c.limits, back.limits);
    }

    #[test]
    fn behavior_round_trip_via_json() {
        use crate::BehaviorSpec;
        use std::path::PathBuf;
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.behavior = Some(BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            on_call: Some(PathBuf::from("lib/handlers.lisp")),
            ..Default::default()
        });
        let json = serde_json::to_string(&c).unwrap();
        let back: Caixa = serde_json::from_str(&json).unwrap();
        assert_eq!(c.behavior, back.behavior);
    }

    #[test]
    fn upgrade_from_round_trip_via_json() {
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        use std::path::PathBuf;
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![
                UpgradeInstruction::LoadModule {
                    module: "demo".into(),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/migrations/v01-to-v02.lisp"),
                },
                UpgradeInstruction::SoftPurge {
                    module: "demo-old".into(),
                },
            ],
        }];
        let json = serde_json::to_string(&c).unwrap();
        let back: Caixa = serde_json::from_str(&json).unwrap();
        assert_eq!(c.upgrade_from, back.upgrade_from);
    }

    #[test]
    fn supervisor_view_returns_typed_shape() {
        use crate::{ChildSpec, RestartPolicy, RestartStrategy};
        let mut c = Caixa::from_lisp(&Caixa::template("root")).unwrap();
        c.kind = CaixaKind::Supervisor;
        c.bibliotecas.clear();
        c.estrategia = Some(RestartStrategy::OneForOne);
        c.max_restarts = Some(5);
        c.restart_window = Some("60s".into());
        c.children = vec![ChildSpec {
            caixa: "worker".into(),
            versao: "^0.1".into(),
            restart: RestartPolicy::Permanent,
        }];
        let view = c.supervisor_view().expect("Supervisor kind has a view");
        assert_eq!(view.estrategia, RestartStrategy::OneForOne);
        assert_eq!(view.max_restarts, 5);
        assert_eq!(view.restart_window, Some(std::time::Duration::from_secs(60)));
        assert_eq!(view.children.len(), 1);
        view.validate().unwrap();
    }

    #[test]
    fn supervisor_view_none_for_non_supervisor_kinds() {
        let c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        assert!(c.supervisor_view().is_none());
    }

    #[test]
    fn existing_manifests_unaffected_by_new_optional_slots() {
        // Regression test: a caixa.lisp authored before M2 typed slots
        // should still parse + serialize cleanly. The bare `defcaixa`
        // emitted by `Caixa::template` has none of the new fields.
        let src = Caixa::template("legacy");
        let c = Caixa::from_lisp(&src).unwrap();
        assert!(c.limits.is_none());
        assert!(c.behavior.is_none());
        assert!(c.upgrade_from.is_empty());
        assert!(c.estrategia.is_none());
        assert!(c.children.is_empty());

        // And to_lisp emits a manifest with the new slots in the
        // empty/default state — round-trippable.
        let emitted = c.to_lisp();
        let back = Caixa::from_lisp(&emitted).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn validate_deps_accepts_canonical_caixa() {
        // Positive control: the bare template — zero deps, zero
        // deps_dev — passes the gate trivially. A future axis added to
        // `Dep::validate` mustn't regress an empty-deps caixa to a
        // build error.
        let c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.validate_deps().unwrap();
    }

    #[test]
    fn validate_deps_rejects_invalid_versao_in_deps() {
        // Fail-before-pass-after pin: a malformed `:deps :versao`
        // surfaces at validate_deps() time, not at lacre-resolve time.
        // Mirrors `rejects_invalid_membro_versao_requirement` and
        // `validate_rejects_invalid_child_versao_requirement` on the
        // other two `:versao` axes.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps = vec![Dep::simple("caixa-teia", "^bad-version")];
        let err = c.validate_deps().unwrap_err();
        assert!(
            matches!(
                err,
                crate::dep::DepError::VersaoInvalid { ref nome, ref versao, .. }
                    if nome == "caixa-teia" && versao == "^bad-version"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_deps_rejects_invalid_versao_in_deps_dev() {
        // Parity pin: `:deps-dev` must run through the same per-entry
        // validator as `:deps` — a typo in either axis surfaces the
        // same diagnostic. Without this leg, `:deps-dev` would be a
        // second-class citizen of the typed surface and an author
        // could land a build that passes validate_deps but fails at
        // `feira lock`-time when the dev-dep is resolved for a test
        // build.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps_dev = vec![Dep::simple("tatara-check", "^^0.1")];
        let err = c.validate_deps().unwrap_err();
        assert!(
            matches!(
                err,
                crate::dep::DepError::VersaoInvalid { ref nome, ref versao, .. }
                    if nome == "tatara-check" && versao == "^^0.1"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_deps_runs_deps_before_deps_dev() {
        // Order pin: when both lists carry typos, the `:deps`
        // diagnostic surfaces first. The author's mental model is
        // "runtime deps are load-bearing; dev deps are scaffolding";
        // surfacing the runtime axis first matches that hierarchy.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps = vec![Dep::simple("runtime-dep", "^bad-runtime")];
        c.deps_dev = vec![Dep::simple("dev-dep", "^bad-dev")];
        let err = c.validate_deps().unwrap_err();
        assert!(
            matches!(
                err,
                crate::dep::DepError::VersaoInvalid { ref nome, .. }
                    if nome == "runtime-dep"
            ),
            "expected `:deps` typo to surface first, got {err:?}"
        );
    }

    #[test]
    fn validate_deps_accepts_canonical_versao_forms_in_both_lists() {
        // Positive control sweep across both lists. Pin every
        // canonical Cargo-shaped form so a future tightening of the
        // accepted set surfaces here as a test failure (parity with
        // `accepts_canonical_membro_versao_forms` and
        // `validate_accepts_canonical_child_versao_forms`).
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps = vec![
            Dep::simple("caret", "^0.1"),
            Dep::simple("tilde", "~0.1.2"),
            Dep::simple("exact", "0.1.0"),
            Dep::simple("wildcard", "*"),
            Dep::simple("multi-range", ">=0.1, <2"),
        ];
        c.deps_dev = vec![
            Dep::simple("dev-caret", "^0.1"),
            Dep::simple("dev-wildcard", "*"),
        ];
        c.validate_deps().unwrap();
    }

    #[test]
    fn validate_deps_diagnostic_carries_offending_dep() {
        // Diagnostic-shape pin: the error names the offending entry's
        // `:nome` + `:versao` verbatim and carries a non-empty
        // `reason` from `semver::VersionReq::parse`, so a `feira lint`
        // run can render the diagnostic without re-parsing.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps = vec![Dep::simple("caixa-teia", "not-a-req")];
        let err = c.validate_deps().unwrap_err();
        let crate::dep::DepError::VersaoInvalid {
            nome,
            versao,
            reason,
        } = err
        else {
            panic!("expected VersaoInvalid, got other variant");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(versao, "not-a-req");
        assert!(
            !reason.is_empty(),
            "VersaoInvalid `reason` must carry the parser's wording verbatim"
        );
    }

    #[test]
    fn to_lisp_preserves_deps() {
        let src = r#"
(defcaixa
  :nome "x"
  :versao "0.1.0"
  :kind Biblioteca
  :deps ((:nome "a" :versao "^0.1")
         (:nome "b" :versao "*" :fonte (:tipo git :repo "github:o/b" :tag "v1"))))
"#;
        let c1 = Caixa::from_lisp(src).unwrap();
        let emitted = c1.to_lisp();
        let c2 = Caixa::from_lisp(&emitted).expect("round trip");
        assert_eq!(c1.deps, c2.deps);
    }

    // ── :upgrade-from typed gate (Caixa::validate_upgrade_from) ────────

    #[test]
    fn validate_upgrade_from_accepts_canonical_empty_list() {
        // Positive control: the bare template — zero upgrade entries —
        // passes trivially. A future axis added to the gate mustn't
        // regress an `:upgrade-from ()` caixa to a build error.
        let c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.validate_upgrade_from().unwrap();
    }

    #[test]
    fn validate_upgrade_from_accepts_canonical_chain() {
        // Positive control sweep: a realistic multi-version chain with
        // each instruction variant exercised at least once. Pins the
        // accepted set so a future tightening surfaces here as a test
        // failure (parity with `validate_deps_accepts_canonical_versao_forms_in_both_lists`).
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        use std::path::PathBuf;
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.upgrade_from = vec![
            UpgradeFromEntry {
                from: "0.1.0".into(),
                instructions: vec![
                    UpgradeInstruction::LoadModule {
                        module: "demo".into(),
                    },
                    UpgradeInstruction::StateChange {
                        script: PathBuf::from("lib/migrations/v01.lisp"),
                    },
                    UpgradeInstruction::SoftPurge {
                        module: "demo-old".into(),
                    },
                ],
            },
            UpgradeFromEntry {
                from: "0.1.5".into(),
                instructions: vec![UpgradeInstruction::Purge {
                    module: "demo-old".into(),
                }],
            },
            UpgradeFromEntry {
                from: "0.2.0-rc.1".into(),
                instructions: vec![UpgradeInstruction::Restart],
            },
        ];
        c.validate_upgrade_from().unwrap();
    }

    #[test]
    fn validate_upgrade_from_rejects_non_semver_from() {
        // Fail-before-pass-after pin: a malformed `:from` surfaces at
        // validate_upgrade_from() time, not at hot-upgrade time when
        // the wasm-operator's appup matcher would otherwise fail with a
        // far-from-source diagnostic.
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "not-a-semver".into(),
            instructions: vec![UpgradeInstruction::Restart],
        }];
        let err = c.validate_upgrade_from().unwrap_err();
        assert!(
            matches!(err, UpgradeError::BadFromVersion(ref s) if s == "not-a-semver"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_upgrade_from_rejects_empty_module() {
        // Per-instruction value-shape gate: `LoadModule` / `SoftPurge` /
        // `Purge` with an empty `:module` is rejected — the wasm-engine
        // would otherwise try to load/purge a module whose name is the
        // empty string, surfacing a registry-side `module not found ""`
        // at hot-upgrade time.
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![UpgradeInstruction::LoadModule {
                module: String::new(),
            }],
        }];
        let err = c.validate_upgrade_from().unwrap_err();
        assert_eq!(err, UpgradeError::EmptyModule);
    }

    #[test]
    fn validate_upgrade_from_rejects_absolute_script() {
        // Sandbox-bypass pin: an absolute `:script` ("/etc/foo.lisp")
        // silently subverts `root.join(p)` (Path::join replaces the
        // base when the right side is absolute). Same trajectory as
        // BehaviorSpec's AbsolutePath gate (b0c8389): the value-shape
        // pass surfaces the sandbox bypass before the existence check
        // would either fail with a confusing "/etc/foo.lisp" path or
        // pass when the absolute path happens to exist on the host.
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        use std::path::PathBuf;
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![UpgradeInstruction::StateChange {
                script: PathBuf::from("/etc/migrations.lisp"),
            }],
        }];
        let err = c.validate_upgrade_from().unwrap_err();
        assert!(
            matches!(err, UpgradeError::AbsoluteScript { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_upgrade_from_rejects_parent_escape_script() {
        // Parity pin: a `..`-traversing `:script` is rejected for the
        // same sandbox-bypass reason as an absolute script. Mid-path
        // `..` is also caught (`lib/../../escaped.lisp`).
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        use std::path::PathBuf;
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![UpgradeInstruction::StateChange {
                script: PathBuf::from("../sibling/migrations.lisp"),
            }],
        }];
        let err = c.validate_upgrade_from().unwrap_err();
        assert!(
            matches!(err, UpgradeError::ParentEscapeScript { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_upgrade_from_rejects_duplicate_from() {
        // Typed-set discipline pin: two entries with the same `:from`
        // are the same authoring footgun closed for `:membros`
        // (4bb3f3d), `:placement :clusters` (c7c7799), `:entrada :paths`
        // (eb3456d), `:children` (dbf50a9), and `:contratos` (5dbcfaf).
        // OTP appup's per-version instruction list is a function of the
        // prior version — `[{"0.1.0", […]}, {"0.1.0", […]}]` is
        // rejected by `release_handler`; pleme-io enforces the same
        // shape at build time.
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.upgrade_from = vec![
            UpgradeFromEntry {
                from: "0.1.0".into(),
                instructions: vec![UpgradeInstruction::LoadModule {
                    module: "demo".into(),
                }],
            },
            UpgradeFromEntry {
                from: "0.1.0".into(),
                instructions: vec![UpgradeInstruction::Restart],
            },
        ];
        let err = c.validate_upgrade_from().unwrap_err();
        assert!(
            matches!(err, UpgradeError::DuplicateFrom { ref from } if from == "0.1.0"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_upgrade_from_surfaces_first_offender_deterministically() {
        // Order pin: when an early entry is value-shape valid but a
        // later one is malformed, the diagnostic names the later one
        // (declaration-order iteration). When two distinct
        // value-shape failures coexist, the *first* declared entry's
        // failure surfaces — same per-entry-before-cross-entry shape
        // as `validate_deps_runs_deps_before_deps_dev` (the per-list
        // axis runs before the cross-list axis surfaces).
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.upgrade_from = vec![
            UpgradeFromEntry {
                from: "not-a-semver".into(),
                instructions: vec![],
            },
            UpgradeFromEntry {
                from: "also-not-semver".into(),
                instructions: vec![],
            },
        ];
        let err = c.validate_upgrade_from().unwrap_err();
        assert!(
            matches!(err, UpgradeError::BadFromVersion(ref s) if s == "not-a-semver"),
            "expected first offending entry's :from to surface; got {err:?}"
        );
    }
}
