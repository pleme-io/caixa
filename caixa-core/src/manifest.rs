use serde::{Deserialize, Serialize};
use tatara_lisp::DeriveTataraDomain;
use thiserror::Error;

use std::time::Duration;

use crate::{
    behavior::BehaviorSpec, dep::DepError, limits::LimitsSpec, supervisor::SupervisorSpec,
    upgrade::UpgradeFromEntry, CaixaKind, Dep,
};

/// RFC 1123 DNS-1123 label max length, in bytes — same value the K8s
/// apiserver-side OpenAPI schema enforces on every `metadata.name` field
/// (1..=63), and the same per-label cap [`crate::aplicacao::Entrada::host`]
/// validation already enforces inside the Gateway API hostname regex.
/// Lifted as a typed const so a future per-axis nome / member /
/// contrato-endpoint identity gate (see [`Caixa::validate_nome`]) reads
/// the bound from one place rather than re-declaring the magic number.
const NOME_LABEL_MAX_LEN: usize = 63;

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

    /// Reject `:nome` values the K8s apiserver would refuse at admission
    /// time. The contract — exactly the regex K8s applies to every
    /// resource's `metadata.name` field, the canonical RFC 1123
    /// DNS-1123 label rule (also enforced by the Gateway API hostname
    /// regex on every per-label position; same shape
    /// [`crate::aplicacao`]'s `validate_entrada_host` enforces inside
    /// the per-label loop):
    ///
    ///   - 1..=63 bytes;
    ///   - lowercase ASCII alphanumeric + hyphen (`[a-z0-9-]`); no
    ///     uppercase, no underscore, no Unicode (IDN-style names are
    ///     never round-trippable through K8s metadata.name);
    ///   - non-hyphen alphanumeric at both label boundaries (no `-foo`,
    ///     no `foo-`).
    ///
    /// Until this gate landed `Caixa::nome` was a free `String` past
    /// [`Caixa::from_lisp`]: the derive-macro path stored whatever the
    /// author wrote, so a malformed `:nome` (`""`, `"FooBar"`,
    /// `"my_caixa"`, `"checkout!"`, `"-checkout"`, `"checkout-"`,
    /// `"a".repeat(64)`) silently passed parse and the K8s apiserver's
    /// `metadata.name: Invalid value` admission error surfaced at
    /// `kubectl apply` time, far from the source caixa.lisp, with no
    /// field naming the offending caixa. Lifting the gate to caixa-build
    /// time turns the apiserver-side regex into a structural property
    /// of the validated typed value: every consumer reaching for
    /// `caixa.nome` as a K8s identifier (caixa-mesh's CiliumNetworkPolicy
    /// `metadata.name` at `<aplicacao>-<de>-to-<para>` composition
    /// (caixa-mesh/src/lib.rs:250), Gateway `metadata.name` at
    /// caixa-mesh/src/lib.rs:382, HTTPRoute `metadata.name` at
    /// caixa-mesh/src/lib.rs:423, lib/<nome>.lisp default-path
    /// resolution at [`crate::layout::StandardLayout::verify`],
    /// programs.yaml `name:` emission at caixa-flux's
    /// `programs_yaml_entry`, the future `lareira-<nome>` Helm chart
    /// release-name composition at caixa-helm) reads a value the
    /// apiserver accepts without re-validating at the renderer or
    /// admission layer.
    ///
    /// Same trajectory as the c7d05ec `:entrada :host` Gateway API v1
    /// Hostname value-shape gate — the missing peer of that gate on the
    /// rootmost identity axis (the package's own `:nome`, which flows
    /// into every K8s `metadata.name` the renderers emit). The 9888b13
    /// / b38ff3a / 2420c44 trio closed the per-`:versao` axes
    /// (`:membros`, `:children`, `:deps`/`:deps-dev`); this commit
    /// closes the per-`:nome` axis at the root.
    ///
    /// The diagnostic carries the offending `nome:` verbatim plus a
    /// parser-shaped `reason:` naming the specific violation, so the
    /// author can grep their caixa.lisp for `:nome "<nome>"` and fix
    /// it in one edit. Same diagnostic shape as `MembroVersaoInvalid`
    /// (9888b13) and `EntradaHostInvalid` (c7d05ec).
    ///
    /// # Errors
    ///
    /// Returns [`CaixaNomeError::Empty`] when `:nome` is the empty
    /// string (the more self-locating arm — empty + invalid would both
    /// reject, but empty is the canonical "I forgot to set :nome"
    /// footgun and deserves its own diagnostic). Returns
    /// [`CaixaNomeError::Invalid`] for every other DNS-1123 label
    /// violation, with the offending `nome` + parser-shaped `reason`
    /// surfaced in the diagnostic.
    pub fn validate_nome(&self) -> Result<(), CaixaNomeError> {
        validate_caixa_nome(&self.nome)
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

/// Reject `:nome` values the K8s apiserver would refuse at admission
/// time as a `metadata.name`. See [`Caixa::validate_nome`] for the full
/// contract + compounding rationale.
///
/// Lifted as a free function (rather than inlining the cascade in
/// [`Caixa::validate_nome`]) so the contract lives in one place — every
/// future per-name axis (the future `Membro::caixa` value-shape gate,
/// the future `WitContract::de`/`para` shape gate, the M4
/// `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer's nome validator)
/// reaches for the same predicate, not its own. Same compounding shape
/// as [`crate::aplicacao`]'s `validate_entrada_host` lift (c7d05ec).
fn validate_caixa_nome(nome: &str) -> Result<(), CaixaNomeError> {
    if nome.is_empty() {
        return Err(CaixaNomeError::Empty);
    }
    if nome.len() > NOME_LABEL_MAX_LEN {
        return Err(CaixaNomeError::Invalid {
            nome: nome.to_string(),
            reason: format!(
                "exceeds DNS-1123 label max length of {NOME_LABEL_MAX_LEN} bytes (got {} bytes; \
                 the K8s apiserver rejects longer metadata.name values at admission time)",
                nome.len()
            ),
        });
    }
    let bytes = nome.as_bytes();
    if !bytes[0].is_ascii_alphanumeric() || !bytes[bytes.len() - 1].is_ascii_alphanumeric() {
        return Err(CaixaNomeError::Invalid {
            nome: nome.to_string(),
            reason: "must start and end with an ASCII alphanumeric (no leading or trailing `-`; \
                     the K8s apiserver rejects metadata.name values that begin or end with `-`)"
                .to_string(),
        });
    }
    for &b in bytes {
        let valid = b.is_ascii_digit() || b.is_ascii_lowercase() || b == b'-';
        if !valid {
            let reason = if b.is_ascii_uppercase() {
                format!(
                    "contains uppercase character {ch:?} (DNS-1123 metadata.name values are \
                     lowercase-only; use {lower:?})",
                    ch = b as char,
                    lower = nome.to_ascii_lowercase()
                )
            } else if b == b'_' {
                "contains `_` (DNS-1123 metadata.name values allow only `[a-z0-9-]`; use `-` \
                 instead)"
                    .to_string()
            } else if b == b'.' {
                "contains `.` (DNS-1123 *label* values forbid `.`; the K8s apiserver enforces \
                 the single-label form on metadata.name. Hostnames carrying `.` belong in \
                 `:entrada :host`, not `:nome`)"
                    .to_string()
            } else if b.is_ascii_whitespace() {
                "contains whitespace".to_string()
            } else {
                format!(
                    "contains invalid character {ch:?} (DNS-1123 metadata.name values allow \
                     only `[a-z0-9-]`)",
                    ch = b as char
                )
            };
            return Err(CaixaNomeError::Invalid {
                nome: nome.to_string(),
                reason,
            });
        }
    }
    Ok(())
}

/// The two-arm typed-error family for [`Caixa::validate_nome`]. Same
/// shape as the recent c7d05ec `EntradaHostInvalid` and 9888b13
/// `MembroVersaoInvalid` diagnostics: the offending value verbatim
/// plus a parser-shaped reason naming the specific violation, so the
/// author can grep their caixa.lisp for `:nome "<nome>"` and fix it in
/// one edit. Mirrors the empty-vs-invalid split every other typed
/// shape gate carries (`MembroVersaoEmpty` / `MembroVersaoInvalid`,
/// `EmptyEntradaHost` / `EntradaHostInvalid`) so the more
/// self-locating "I forgot to set :nome" arm leads with its own
/// diagnostic.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CaixaNomeError {
    #[error(
        ":nome must be non-empty (every caixa names itself; the empty string is never a \
         valid `metadata.name` for any K8s resource the renderers emit)"
    )]
    Empty,
    #[error(
        ":nome {nome:?} is not a valid DNS-1123 label: {reason} (the K8s apiserver enforces \
         this rule on every resource's metadata.name; use a name like \"checkout\" or \
         \"hello-rio\" — lowercase RFC 1123 alphanumeric + hyphen, max 63 bytes)"
    )]
    Invalid { nome: String, reason: String },
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

    // ── Caixa::validate_nome — DNS-1123 label value-shape gate ───────────

    #[test]
    fn validate_nome_accepts_canonical_caixa() {
        // Positive control: the bare template's `:nome "demo"` is the
        // canonical author-surface shape (lowercase, single-label,
        // alphanumeric). A future tightening of the accepted set
        // mustn't regress the trivially-valid case.
        let c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.validate_nome().unwrap();
    }

    #[test]
    fn validate_nome_accepts_canonical_dns_label_forms() {
        // Positive-control sweep across the typed-author surface:
        // every shape K8s metadata.name accepts must round-trip through
        // the gate without error. Pin every leg so a future tightening
        // surfaces here.
        for nome in [
            "checkout",  // single lowercase label — most common shape
            "hello-rio", // hyphen-bearing — also common
            "x",         // 1-byte (valid metadata.name lower bound)
            "a1",        // alphanumeric mix
            "1a",        // digit-leading alphanumeric (DNS-1123 allows; only DNS-1035 forbids)
            "abcdef0123456789-abcdef0123456789-abcdef0123456789-abcdefghijkl", // 63 bytes (cap)
            "caixa-teia-forge", // multi-hyphen
        ] {
            let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
            c.nome = nome.into();
            c.validate_nome()
                .unwrap_or_else(|e| panic!("canonical nome {nome:?} must validate, got {e:?}"));
        }
    }

    #[test]
    fn validate_nome_rejects_empty() {
        // Fail-before-pass-after pin: the empty `:nome` is the
        // canonical "I forgot to set :nome" footgun. Until this gate
        // landed, every pre-gate codebase silently accepted `:nome ""`
        // and the K8s apiserver rejected the rendered metadata.name at
        // admission time, far from the source caixa.lisp.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.nome = String::new();
        assert_eq!(c.validate_nome().unwrap_err(), CaixaNomeError::Empty);
    }

    #[test]
    fn validate_nome_rejects_uppercase() {
        // The canonical "I'm thinking of a Java/JS package name" leak
        // — `"FooBar"` looks like a class name, but DNS-1123 forbids
        // uppercase. Diagnostic carries the lowercase-equivalent
        // suggestion verbatim.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.nome = "FooBar".into();
        let err = c.validate_nome().unwrap_err();
        let CaixaNomeError::Invalid { nome, reason } = err else {
            panic!("expected Invalid, got other variant");
        };
        assert_eq!(nome, "FooBar");
        assert!(
            reason.contains("uppercase") && reason.contains("foobar"),
            "diagnostic must name the lowercase suggestion: {reason}"
        );
    }

    #[test]
    fn validate_nome_rejects_underscore() {
        // The canonical "I'm thinking of a Python module" leak —
        // `"my_caixa"` reads naturally to a Python author but DNS-1123
        // forbids `_`. The diagnostic explicitly suggests `-`.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.nome = "my_caixa".into();
        let err = c.validate_nome().unwrap_err();
        assert!(
            matches!(err, CaixaNomeError::Invalid { ref nome, ref reason }
                if nome == "my_caixa" && reason.contains('_') && reason.contains('-')),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_nome_rejects_dotted_hostname_shape() {
        // The canonical "I'm thinking of the host" leak — pasting a
        // hostname (`"checkout.quero.cloud"`) into `:nome` instead of
        // `:entrada :host`. DNS-1123 *labels* (which is what
        // metadata.name is) forbid `.`. Diagnostic explicitly redirects
        // the author to `:entrada :host`.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.nome = "checkout.quero.cloud".into();
        let err = c.validate_nome().unwrap_err();
        assert!(
            matches!(err, CaixaNomeError::Invalid { ref reason, .. }
                if reason.contains(":entrada :host")),
            "diagnostic must redirect to :entrada :host: {err:?}"
        );
    }

    #[test]
    fn validate_nome_rejects_leading_hyphen() {
        // DNS-1123 boundary rule: labels must start with an
        // alphanumeric, never `-`. The canonical typo: a CLI flag name
        // (`"-checkout"`) leaking into `:nome`.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.nome = "-checkout".into();
        let err = c.validate_nome().unwrap_err();
        assert!(
            matches!(err, CaixaNomeError::Invalid { ref reason, .. }
                if reason.contains("alphanumeric")),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_nome_rejects_trailing_hyphen() {
        // DNS-1123 boundary rule: labels must end with an alphanumeric,
        // never `-`. The other half of the boundary rule.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.nome = "checkout-".into();
        let err = c.validate_nome().unwrap_err();
        assert!(
            matches!(err, CaixaNomeError::Invalid { ref reason, .. }
                if reason.contains("alphanumeric")),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_nome_rejects_too_long() {
        // RFC 1123 label cap: 63 bytes. K8s apiserver rejects any
        // metadata.name over the cap at admission time.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.nome = "a".repeat(64);
        let err = c.validate_nome().unwrap_err();
        assert!(
            matches!(err, CaixaNomeError::Invalid { ref reason, .. }
                if reason.contains("63") && reason.contains("64")),
            "diagnostic must name both the cap and the actual length: {err:?}"
        );
    }

    #[test]
    fn validate_nome_max_length_validates() {
        // Boundary pin: exactly 63 bytes (the cap) passes. Pins the
        // off-by-one trajectory — a `>=` typo in the gate would surface
        // here.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.nome = "a".repeat(63);
        c.validate_nome().unwrap();
    }

    #[test]
    fn validate_nome_rejects_special_characters() {
        // Sweep over the most-common non-alphanumeric typos —
        // exclamation point, slash, space, colon, plus. Each surfaces
        // an `Invalid` arm with the offending character named.
        for bad in ["checkout!", "check/out", "check out", "check:out", "a+b"] {
            let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
            c.nome = bad.into();
            let err = c.validate_nome().unwrap_err();
            assert!(
                matches!(err, CaixaNomeError::Invalid { ref nome, .. } if nome == bad),
                "expected Invalid for {bad:?}, got {err:?}"
            );
        }
    }

    #[test]
    fn validate_nome_empty_takes_precedence_over_invalid() {
        // Order pin: the more self-locating `Empty` arm fires before
        // the generic `Invalid` arm on the empty string. Same shape as
        // `MembroVersaoEmpty` vs `MembroVersaoInvalid` (9888b13) and
        // `EmptyEntradaHost` vs `EntradaHostInvalid` (c7d05ec).
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.nome = String::new();
        assert_eq!(c.validate_nome().unwrap_err(), CaixaNomeError::Empty);
    }

    #[test]
    fn validate_nome_diagnostic_carries_offending_nome() {
        // Diagnostic-shape pin: the error names the offending value
        // verbatim and carries a non-empty `reason`, so the author can
        // grep their caixa.lisp for `:nome "<nome>"` and fix it in one
        // edit. Same diagnostic shape as `MembroVersaoInvalid` and
        // `EntradaHostInvalid`.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.nome = "Checkout_Service".into();
        let err = c.validate_nome().unwrap_err();
        let CaixaNomeError::Invalid { nome, reason } = err else {
            panic!("expected Invalid, got other variant");
        };
        assert_eq!(nome, "Checkout_Service");
        assert!(
            !reason.is_empty(),
            "Invalid `reason` must carry a parser-shaped wording"
        );
    }
}
