use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tatara_lisp::DeriveTataraDomain;

use thiserror::Error;

use crate::{
    CaixaKind, Dep,
    behavior::BehaviorSpec,
    dep::DepError,
    limits::LimitsSpec,
    render::{PathShapeViolation, is_dns_1123_label, is_git_repo_url, is_sandboxed_relative_path},
    supervisor::SupervisorSpec,
    upgrade::UpgradeFromEntry,
};

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

    /// The kebab-case `:slot` tags of every M3 mesh slot this caixa
    /// *declares* a value on, in canonical declaration order
    /// (`:membros` → `:contratos` → `:politicas` → `:placement` →
    /// `:entrada`). A slot counts as declared when its backing field
    /// carries a value — a non-empty `Vec`, or a `Some(...)`.
    ///
    /// The M3 mesh slots compose the typed graph of a `:kind Aplicacao`
    /// (MESH-COMPOSITION §III.1). [`Self::aplicacao_view`] only folds
    /// them into a validatable [`crate::aplicacao::AplicacaoSpec`] when
    /// the kind matches (returns `None` otherwise), and the caixa-mesh /
    /// caixa-flux / caixa-helm renderers only emit them for an
    /// Aplicacao. On any *other* kind a declared mesh slot is the
    /// manifest field's documented "ignored otherwise" (see the
    /// `:membros` … `:entrada` field docs): it silently passes
    /// [`Caixa::from_lisp`] and then vanishes — never validated, never
    /// rendered — far from the source caixa.lisp.
    /// [`crate::StandardLayout::verify`] consults this to reject that
    /// silent-drop at caixa-build time
    /// ([`crate::LayoutError::MeshSlotsOnNonAplicacao`]), mirroring the
    /// `SupervisorOwnsCode` / `AplicacaoOwnsCode` kind-coherence gates:
    /// a slot foreign to the kind is a build error, not a silent drop.
    ///
    /// Lifted as a typed method (rather than an inline disjunction at
    /// the verify call site) so the mesh-slot set lives in one place —
    /// a future M4 axis added to the Aplicacao surface (per-edge policy
    /// overlay, distributed-app takeover config) is one push here, and
    /// every consumer reaching for "which mesh slots are set" (the
    /// verify gate, a future `feira lint` kind-coherence advisory)
    /// inherits the canonical order without rolling its own.
    #[must_use]
    pub fn declared_mesh_slots(&self) -> Vec<&'static str> {
        let mut slots = Vec::new();
        if !self.membros.is_empty() {
            slots.push(":membros");
        }
        if !self.contratos.is_empty() {
            slots.push(":contratos");
        }
        if self.politicas.is_some() {
            slots.push(":politicas");
        }
        if self.placement.is_some() {
            slots.push(":placement");
        }
        if self.entrada.is_some() {
            slots.push(":entrada");
        }
        slots
    }

    /// The kebab-case `:slot` tags of every supervisor-tree slot this
    /// caixa *declares* a value on, in canonical declaration order
    /// (`:estrategia` → `:max-restarts` → `:restart-window` →
    /// `:children`). A slot counts as declared when its backing field
    /// carries a value — a `Some(...)`, or a non-empty `Vec`.
    ///
    /// The supervisor-tree slots compose the typed OTP supervisor of a
    /// `:kind Supervisor` (INSPIRATIONS §II.2; the `:estrategia` +
    /// `:children` field docs above). [`Self::supervisor_view`] only
    /// folds them into a validatable [`SupervisorSpec`] when the kind
    /// matches (returns `None` otherwise), and the wasm-operator's
    /// hierarchical reconciler only consumes them for a Supervisor. On
    /// any *other* kind a declared supervisor slot is the manifest
    /// field's documented "ignored otherwise" (see the `:estrategia` …
    /// `:children` field docs): it silently passes [`Caixa::from_lisp`]
    /// and then vanishes — never validated, never reconciled — far from
    /// the source caixa.lisp. [`crate::StandardLayout::verify`] consults
    /// this to reject that silent-drop at caixa-build time
    /// ([`crate::LayoutError::SupervisorSlotsOnNonSupervisor`]), the
    /// exact mirror of the [`Self::declared_mesh_slots`] /
    /// [`crate::LayoutError::MeshSlotsOnNonAplicacao`] gate on the
    /// Aplicacao-only slot set: a slot foreign to the kind is a build
    /// error, not a silent drop.
    #[must_use]
    pub fn declared_supervisor_slots(&self) -> Vec<&'static str> {
        let mut slots = Vec::new();
        if self.estrategia.is_some() {
            slots.push(":estrategia");
        }
        if self.max_restarts.is_some() {
            slots.push(":max-restarts");
        }
        if self.restart_window.is_some() {
            slots.push(":restart-window");
        }
        if !self.children.is_empty() {
            slots.push(":children");
        }
        slots
    }

    /// The kebab-case `:slot` tags of every M2 Servico-runtime slot this
    /// caixa *declares* a value on, in canonical declaration order
    /// (`:limits` → `:behavior` → `:upgrade-from`). A slot counts as
    /// declared when its backing field carries a value — a `Some(...)`,
    /// or a non-empty `Vec`.
    ///
    /// The M2 slots configure the runtime of a long-running wasm
    /// component, i.e. a `:kind Servico`: `:limits` is Lunatic
    /// per-process sandboxing (INSPIRATIONS §III.1), `:behavior` is the
    /// OTP `gen_server` callback set (§II.3), `:upgrade-from` is the OTP
    /// appup hot-code-reload table (§II.4). The caixa-helm / caixa-flux
    /// renderers gate on [`crate::require_kind`]`(_, Servico)` and only
    /// emit these slots for a Servico; on any *other* kind a declared M2
    /// slot is the manifest field's documented "ignored otherwise": its
    /// well-formedness is checked by [`crate::StandardLayout::verify`]
    /// but the value is never rendered into a chart / programs.yaml entry
    /// — it silently passes [`Caixa::from_lisp`] + `feira build` and then
    /// vanishes, far from the source caixa.lisp.
    /// [`crate::StandardLayout::verify`] consults this to reject that
    /// silent-drop at caixa-build time
    /// ([`crate::LayoutError::ServicoSlotsOnNonServico`]), the exact
    /// mirror of the [`Self::declared_mesh_slots`] /
    /// [`Self::declared_supervisor_slots`] gates on the peer
    /// kind-exclusive slot sets: a slot foreign to the kind is a build
    /// error, not a silent drop.
    #[must_use]
    pub fn declared_servico_slots(&self) -> Vec<&'static str> {
        let mut slots = Vec::new();
        if self.limits.is_some() {
            slots.push(":limits");
        }
        if self.behavior.is_some() {
            slots.push(":behavior");
        }
        if !self.upgrade_from.is_empty() {
            slots.push(":upgrade-from");
        }
        slots
    }

    /// The kebab-case `:slot` tags of every code-surface slot this caixa
    /// declares a value on that its [`CaixaKind`] doesn't natively own,
    /// in canonical declaration order (`:exe` → `:servicos`). A
    /// code-surface slot is owned by exactly one kind: `:exe` by
    /// [`CaixaKind::Binario`] (the nix-built executable surface), and
    /// `:servicos` by [`CaixaKind::Servico`] (the wasm component +
    /// `ComputeUnit` daemon surface).
    ///
    /// Each is silently ignored when declared on the wrong kind: the
    /// caixa-helm / caixa-flux / caixa-flake renderers gate on
    /// [`crate::require_kind`]`(_, <owning-kind>)`, so on any *other*
    /// code-running kind a declared `:exe` / `:servicos` is the manifest
    /// field's documented "ignored otherwise" — its path is checked for
    /// existence by the layout's `bibliotecas`/`exe`/`servicos` loops
    /// (which run after [`Caixa::from_lisp`]), but the value is never
    /// rendered into a build target or programs.yaml entry. It silently
    /// passes [`Caixa::from_lisp`] + `feira build`, far from the source
    /// caixa.lisp, with no field naming which slot is foreign.
    ///
    /// [`crate::StandardLayout::verify`] consults this to reject that
    /// silent-drop at caixa-build time
    /// ([`crate::LayoutError::ForeignCodeSlot`]), beside the M2
    /// servico-runtime, supervisor-tree, and M3 mesh kind-coherence
    /// gates ([`Self::declared_servico_slots`] /
    /// [`Self::declared_supervisor_slots`] /
    /// [`Self::declared_mesh_slots`]): the fourth kind ↔ slot algebra
    /// axis to be closed on the typed surface. The Supervisor /
    /// Aplicacao "no code at all" cases ([`crate::LayoutError::SupervisorOwnsCode`]
    /// / [`crate::LayoutError::AplicacaoOwnsCode`]) keep their dedicated
    /// diagnostics — they fire ahead of this gate on the same `verify`
    /// pass, so for Supervisor / Aplicacao the `OwnCode` arm always wins
    /// and this method is moot. For Biblioteca / Binario / Servico, this
    /// gate fires when a code-running kind declares another code-running
    /// kind's exclusive code surface.
    ///
    /// `:bibliotecas` is deliberately excluded — a Binario or Servico
    /// may legitimately ship a `lib/` helper that the underlying
    /// substrate (the nix flake for Binario, the wasm component build
    /// for Servico) bundles into its build, so the slot's
    /// declared-on-wrong-kind cardinality isn't a structural error on
    /// either code-running kind. A Biblioteca declaring `:bibliotecas`
    /// is the native case (the slot's owning kind). Supervisor /
    /// Aplicacao declaring `:bibliotecas` is gated upstream by
    /// [`crate::LayoutError::SupervisorOwnsCode`] /
    /// [`crate::LayoutError::AplicacaoOwnsCode`].
    ///
    /// Lifted as a typed method (rather than an inline disjunction at
    /// the verify call site) so the foreign-code-slot set lives in one
    /// place — a future kind that gains its own code-surface slot is
    /// one push here, and every consumer reaching for "which code
    /// surfaces are foreign to this kind" (the verify gate, a future
    /// `feira lint` kind-coherence advisory, the future `app-operator`'s
    /// per-caixa build-target classifier) inherits the canonical order
    /// without rolling its own.
    #[must_use]
    pub fn declared_foreign_code_slots(&self) -> Vec<&'static str> {
        let mut slots = Vec::new();
        if !self.exe.is_empty() && !self.kind.requires_exe() {
            slots.push(":exe");
        }
        if !self.servicos.is_empty() && !self.kind.requires_servicos() {
            slots.push(":servicos");
        }
        slots
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
    ///
    /// Within each list, [`DepError::DuplicateNome`] closes the
    /// set-not-multiset discipline on the `:nome` axis: two entries
    /// naming the same caixa carry two `:versao` / `:fonte` / feature
    /// triples that the caixa-resolver's lacre pipeline collapses to one
    /// via its `HashMap`-keyed-by-`:nome` consumption — the second entry
    /// silently overwrites the first at `concrete_versao`-resolve time
    /// (the same "second wins / one silently overwrites the other"
    /// shape the peer typed-graph duplicate gates already close on every
    /// other Vec-shaped authoring surface that keys by name). The
    /// duplicate check fires per-list and runs *after* each per-entry
    /// [`Dep::validate`] call so a malformed-and-duplicated entry
    /// surfaces its narrower per-entry diagnostic
    /// ([`DepError::NomeInvalid`], [`DepError::VersaoInvalid`],
    /// [`DepError::FonteRepoEmpty`], …) before the cross-entry duplicate
    /// diagnostic — the canonical "per-entry shape before cross-entry
    /// uniqueness" precedence the peer `:children :caixa`
    /// ([`crate::SupervisorSpec::validate`]), `:membros :caixa`
    /// ([`crate::AplicacaoSpec::validate_membros`]), `:contratos`
    /// ([`crate::AplicacaoSpec::validate`]), `:placement :clusters`
    /// ([`crate::AplicacaoSpec::validate_placement`]),
    /// `:entrada :paths` ([`crate::AplicacaoSpec::validate`]),
    /// `:upgrade-from :from` ([`crate::upgrade::validate_upgrade_from`]),
    /// and the within-`:upgrade-from`-entry per-instruction-class
    /// singularity gates ([`crate::UpgradeError::DuplicateLoadModule`],
    /// [`crate::UpgradeError::DuplicateStateChange`],
    /// [`crate::UpgradeError::DuplicateCleanup`]) all establish.
    ///
    /// Cross-list (`:deps` ↔ `:deps-dev`) coincidence is *not* gated
    /// here: Cargo's `[dependencies]` + `[dev-dependencies]` accept the
    /// same name in both tables (the dev table's pin overrides the
    /// runtime table's pin in test/dev contexts), and caixa's surface
    /// mirrors that convention until a deliberate choice retires the
    /// override pattern. Only within-list duplicates are structurally
    /// incoherent — those are what this gate closes.
    pub fn validate_deps(&self) -> Result<(), DepError> {
        let mut seen = std::collections::HashSet::new();
        for dep in &self.deps {
            dep.validate()?;
            if !seen.insert(dep.nome.as_str()) {
                return Err(DepError::DuplicateNome {
                    nome: dep.nome.clone(),
                    list: ":deps",
                });
            }
        }
        let mut seen_dev = std::collections::HashSet::new();
        for dep in &self.deps_dev {
            dep.validate()?;
            if !seen_dev.insert(dep.nome.as_str()) {
                return Err(DepError::DuplicateNome {
                    nome: dep.nome.clone(),
                    list: ":deps-dev",
                });
            }
        }
        Ok(())
    }

    /// Reject `:nome` values the K8s apiserver would refuse at admission
    /// time. The top-level Caixa identity flows directly into every
    /// substrate-side artifact's `metadata.name` axis: the
    /// `lareira-<nome>` Helm chart name ([`caixa-helm::lib::chart_name`]),
    /// the programs.yaml `name:` entry the `lareira-fleet-programs`
    /// aggregator keys ComputeUnit derivation off
    /// ([`caixa-flux::lib::programs_yaml_entry`]), the
    /// `LABEL_APLICACAO` label value carried on every Aplicacao-owned
    /// pod and the per-`:contratos` CiliumNetworkPolicy `metadata.name`
    /// (`<aplicacao>-<de>-to-<para>`) and the per-`:entrada`
    /// `<aplicacao>-<para>` HTTPRoute `metadata.name`
    /// ([`caixa-mesh::lib::cilium_network_policies`],
    /// [`caixa-mesh::lib::gateway_routes`]), and the default
    /// `lib/<nome>.lisp` / `exe/<nome>` layout paths
    /// ([`crate::StandardLayout::verify`]). Each K8s apiserver-side
    /// schema enforces the DNS-1123 label rule on admission; a
    /// structurally invalid `:nome` (`"MyApp"` — the canonical
    /// "I copied the display name verbatim" footgun, `"my_app"` — the
    /// Python-/Postgres-leak, `"team.app"` — `:nome` is a single label
    /// not a subdomain, `"-app"` / `"app-"` — DNS-1123 boundary
    /// violations, `"my app"` — the paste-from-doc footgun, `"café"` —
    /// IDN must be pre-encoded as Punycode, the 64-byte UUID-shaped
    /// over-cap slug) silently passed [`Caixa::from_lisp`] and the
    /// failure surfaced at `kubectl apply` time as a `metadata.name:
    /// Invalid value` rejection on whichever derived artifact admitted
    /// first, far from the source `caixa.lisp` and without any field
    /// naming the offending `:nome`.
    ///
    /// Thin wrapper around [`crate::render::is_dns_1123_label`] (the
    /// substrate-side predicate the per-axis name gates already share:
    /// `:membros :caixa` 3f9d7a0, `:placement :clusters` 6cbb900,
    /// `:children :caixa` 31bfa43) that maps the shared parser-shaped
    /// reason into the [`ManifestError::NomeInvalid`] variant, so the
    /// diagnostic is self-locating (the offending `:nome` is named
    /// verbatim) and the author can grep their `caixa.lisp` for
    /// `:nome "<value>"` and fix it in one edit. Same diagnostic shape
    /// every per-axis sibling gate already exposes
    /// ([`crate::AplicacaoError::MembroCaixaInvalid`],
    /// [`crate::AplicacaoError::PlacementClusterInvalid`],
    /// [`crate::SupervisorError::ChildCaixaInvalid`]).
    ///
    /// Empty `:nome` (which [`Caixa::from_lisp`] does not reject — the
    /// derive macro stores the raw String) is gated by the narrower
    /// [`ManifestError::NomeEmpty`] arm before the predicate is
    /// consulted, mirroring the empty-first cascade every per-axis
    /// name gate already uses (e.g. `MembroCaixaEmpty` before
    /// `MembroCaixaInvalid`, `EmptyChildName` before `ChildCaixaInvalid`).
    pub fn validate_nome(&self) -> Result<(), ManifestError> {
        if self.nome.is_empty() {
            return Err(ManifestError::NomeEmpty);
        }
        is_dns_1123_label(&self.nome).map_err(|reason| ManifestError::NomeInvalid {
            nome: self.nome.clone(),
            reason,
        })
    }

    /// Reject `:versao` values that don't parse as [`semver::Version`].
    /// The top-level Caixa version flows directly into every
    /// substrate-side artifact that carries a "this is which version of
    /// the caixa" axis: the `lareira-<nome>` Helm chart's `Chart.yaml`
    /// `version:` + `appVersion:` axes ([`caixa-helm::lib`] —
    /// SemVer-2-strict at `helm template` / `helm install` time per
    /// https://helm.sh/docs/topics/charts/#charts-and-versioning), the
    /// `feira publish` Zig-style `v<versao>` git tag
    /// ([`caixa-flux::lib::programs_yaml_entry`] / the
    /// `caixa-publish.yml` reusable workflow), the programs.yaml entry's
    /// `versao:` value the `lareira-fleet-programs` aggregator carries
    /// onto each rendered ComputeUnit, the OCI image's `:v<versao>` /
    /// `:latest` tags the substrate's `wasi-service-flake` builds with
    /// `skopeo push`, the lacre closure's pinned versions
    /// ([`caixa-resolver`] keys `concrete_versao`), and the
    /// `:upgrade-from :from` references peers in this exact `versao`
    /// shape (`semver::Version`, not `VersionReq`). Each consumer
    /// expects a strict three-part `MAJOR.MINOR.PATCH` (optionally
    /// `-prerelease` and/or `+build`); a structurally invalid `:versao`
    /// (`"0.1"` — missing patch, the canonical "I shortened it" footgun;
    /// `"v0.1.0"` — the git-tag-shape-leaking-into-versao typo;
    /// `"latest"` / `"main"` — the "I confused it with a docker tag"
    /// footgun; `"^0.1"` / `"~0.1.2"` — the requirement-shape leaking
    /// into the version field a peer `:deps :versao` accepts;
    /// `"0.1.0.0"` — the four-part Java/Microsoft convention DNS
    /// SemVer-2 forbids) silently passed [`Caixa::from_lisp`] (the
    /// derive macro stores the raw String) and the failure surfaced at
    /// the *first* downstream consumer that strict-parses it: at
    /// `helm install` time as a chart-version rejection, at
    /// `feira publish` time as a malformed git tag, at lacre-resolve
    /// time as a `semver::Error` not naming the offending caixa, at
    /// `feira upgrade --to <versao>` time as an unresolvable
    /// `:upgrade-from :from` match — far from the source `caixa.lisp`
    /// and without any field naming the offending `:versao`.
    ///
    /// Thin wrapper around [`semver::Version::parse`] — the same parser
    /// [`crate::CaixaVersion::parse`] (the typed `:versao` accessor)
    /// and [`crate::UpgradeFromEntry::validate`] (the peer
    /// `:upgrade-from :from` axis, 26da2c7) consume. Maps the
    /// `semver::Error` reason into the [`ManifestError::VersaoInvalid`]
    /// variant, carrying the offending `:versao` verbatim + a
    /// parser-shaped reason naming the specific violation, so the
    /// diagnostic is self-locating (the author can grep their
    /// `caixa.lisp` for `:versao "<value>"` and fix it in one edit).
    /// Same diagnostic shape as [`ManifestError::NomeInvalid`]
    /// (6c992f8) and [`crate::UpgradeError::BadFromVersion`]
    /// (b0c8389) on the peer axes. With this gate, the typed `:versao`
    /// surfaces — top-level `:versao`, `:upgrade-from :from` — are
    /// now structurally equivalent (every value past validate is
    /// round-trippable through [`semver::Version::parse`] without
    /// re-checking at the renderer, resolver, or operator hot-upgrade
    /// layer), peer with the four `:versao` requirement axes (`:deps`,
    /// `:deps-dev`, `:membros`, `:children`) the prior commits
    /// (2420c44, 9888b13, b38ff3a) wired through `parse_requirement`.
    ///
    /// Empty `:versao` (which [`Caixa::from_lisp`] does not reject —
    /// the derive macro stores the raw String) is gated by the
    /// narrower [`ManifestError::VersaoEmpty`] arm before the parser is
    /// consulted, mirroring the empty-first cascade every per-axis
    /// version gate already uses (e.g. `MembroVersaoEmpty` before
    /// `MembroVersaoInvalid`, `EmptyChildVersion` before
    /// `ChildVersaoInvalid`, `NomeEmpty` before `NomeInvalid`).
    pub fn validate_versao(&self) -> Result<(), ManifestError> {
        if self.versao.is_empty() {
            return Err(ManifestError::VersaoEmpty);
        }
        semver::Version::parse(&self.versao).map_err(|e| ManifestError::VersaoInvalid {
            versao: self.versao.clone(),
            reason: e.to_string(),
        })?;
        Ok(())
    }

    /// Reject `:restart-window` values the shared
    /// [`crate::supervisor::duration_codec::parse`] refuses. The flat
    /// `restart_window: Option<String>` slot on [`Caixa`] is stored
    /// raw by the derive macro (the typed [`SupervisorSpec`] holds an
    /// `Option<Duration>` routed through the shared codec via `with =
    /// "duration_codec"`); the inline `Caixa → SupervisorSpec`
    /// view-construction path ([`Self::supervisor_view`]) folds the
    /// raw string through the same shared codec and soft-swallows the
    /// parse error as `None` to keep the view best-effort. Without
    /// this gate a malformed `:restart-window` (`"1.5s"` — the
    /// fractional-seconds drift class; `"1.0s"` — the decimal-shaped
    /// integer drift; `"0.5m"` — the unit-fraction drift; `"+30s"` /
    /// `"-30s"` — the leading-sign drift; `"30x"` — the unknown-unit
    /// footgun; `"abc"` — pure garbage; `""` — the empty-after-trim
    /// edge case) silently produced a `SupervisorSpec` with
    /// `restart_window: None`, indistinguishable from the canonical
    /// "omit the slot to express no reset" authoring shape — Erlang/OTP's
    /// `MaxIntensity / Period` invariant turns into a never-reset
    /// supervisor far from the source `caixa.lisp`, with no field
    /// naming the offending `:restart-window`. Lifting the gate to a
    /// Caixa-level validator mirrors the trajectory of the peer
    /// per-axis identity gates ([`Self::validate_nome`] 6c992f8,
    /// [`Self::validate_versao`] 1fdaa02, [`Self::validate_deps`]
    /// a7f0d8c) and the ABSORPTION-ROADMAP.md M2.2 test pin
    /// (line 196: "reject invalid `:restart-window` (non-duration)").
    ///
    /// Thin wrapper around [`crate::supervisor::duration_codec::parse`]
    /// (the shared codec backing `:supervisor :restart-window` as
    /// serde-routed on [`SupervisorSpec`], `:politicas :timeout`, and
    /// `:politicas :circuit-breaker :window` — all three covered by
    /// the integer-magnitude gate 1c55a2a). Maps the codec's parse
    /// error verbatim into the [`ManifestError::RestartWindowMalformed`]
    /// variant, carrying the offending raw string + a parser-shaped
    /// reason naming the canonical authoring form, so the diagnostic
    /// is self-locating (the author can grep their `caixa.lisp` for
    /// `:restart-window "<value>"` and fix it in one edit) and
    /// uniform with every other manifest-level validate diagnostic.
    /// With this gate the four `:restart-window`-shaped surfaces (the
    /// flat raw string on [`Caixa`], the typed `Option<Duration>` on
    /// [`SupervisorSpec`], the two `MeshPolicy` peer durations) are
    /// now structurally equivalent — every value past the codec is in
    /// one accepted set, by construction.
    ///
    /// `None` (the canonical "omit the slot to express no reset"
    /// shape) is accepted trivially — the gate is a no-op when the
    /// author didn't author a window. The empty string is rejected by
    /// the shared codec (its digit-only gate refuses an empty
    /// magnitude), surfacing the same `RestartWindowMalformed`
    /// diagnostic as every other rejected non-canonical shape.
    pub fn validate_restart_window(&self) -> Result<(), ManifestError> {
        let Some(s) = self.restart_window.as_deref() else {
            return Ok(());
        };
        crate::supervisor::duration_codec::parse(s)
            .map(|_| ())
            .map_err(|reason| ManifestError::RestartWindowMalformed {
                restart_window: s.to_string(),
                reason,
            })
    }

    /// Reject per-entry values on the three Caixa-level code-surface
    /// path lists (`:bibliotecas`, `:exe`, `:servicos`) that the
    /// layout checker's `root.join(p)` sandbox would silently subvert.
    /// Same three structural footguns the peer
    /// [`BehaviorSpec::validate`] (b0c8389) and
    /// [`crate::UpgradeInstruction::validate`] `StateChange` arm
    /// (26da2c7) already close on the M2 `:behavior :on-*` and
    /// `:upgrade-from :state-change :script` axes, here lifted onto
    /// the three top-level code-path axes through the shared
    /// [`is_sandboxed_relative_path`] predicate:
    ///
    ///   - empty entry (`(:bibliotecas (""))` / `(:exe (""))` /
    ///     `(:servicos (""))`): `PathBuf::new()` round-trips through
    ///     [`Path::join`] as the base itself — `root.join("")` ==
    ///     `root`, so the existence check (`self.exists(&root)`)
    ///     trivially passes (the project root exists), and the layout
    ///     silently treats the project root as a biblioteca / exe /
    ///     servico entry. The `:bibliotecas` loop then hands the root
    ///     to `tatara_lisp::read` at `feira build` time as if the root
    ///     directory itself were a Lisp source file — a parse error
    ///     far from the source `caixa.lisp` with no field naming the
    ///     offending entry.
    ///   - absolute path (`(:bibliotecas ("/etc/passwd"))`):
    ///     [`Path::join`] *replaces* the base when the right-hand side
    ///     is absolute, so `root.join("/etc/passwd")` resolves to
    ///     `"/etc/passwd"` and escapes the project sandbox entirely.
    ///     The existence check then silently consults whatever the
    ///     escaped path resolves to — for `:bibliotecas`, the layout
    ///     has no `starts_with`-fence (only `:exe` is fenced under
    ///     `exe/` and `:servicos` under `servicos/`), so an absolute
    ///     `:bibliotecas` entry that happens to resolve on disk
    ///     silently passes. For `:exe` / `:servicos` the fence catches
    ///     the absolute case downstream as `ExeOutsideDir` /
    ///     `ServicoOutsideDir` (or `MissingEntry` if the absolute path
    ///     doesn't exist), but with a downstream-shaped diagnostic
    ///     that names the resolved escape path rather than the
    ///     authoring footgun at the source.
    ///   - parent-escape (`(:bibliotecas ("../sibling/x.lisp"))` /
    ///     `(:exe ("exe/../../escape.lisp"))`): a [`PathBuf`] with any
    ///     [`std::path::Component::ParentDir`] anywhere round-trips
    ///     through [`Path::join`] as a traversal above the caixa root.
    ///     The `:exe` / `:servicos` `starts_with(<dir>)` fence is
    ///     *component-aware* (not canonical-path-aware), so
    ///     `root.join("exe/../../escape.lisp")` `starts_with(exe_dir)`
    ///     is **true** even though the canonical resolution
    ///     `{parent of root}/escape.lisp` lives outside the caixa root
    ///     — the fence silently lets the parent-escape through, and
    ///     the existence check passes if that escape-target happens
    ///     to exist. Caught regardless of where the `..` sits
    ///     (leading, mid-path, trailing) so the gate matches the peer
    ///     predicate's full coverage.
    ///
    /// Same `Empty` → `Absolute` → `ParentEscape` arm-ordering every peer
    /// `is_sandboxed_relative_path` consumer follows (b0c8389 / 26da2c7);
    /// same per-slot diagnostic shape every peer per-axis path-gate
    /// exposes (`*Empty { slot }` / `*Absolute { slot, path }` /
    /// `*ParentEscape { slot, path }`). Cross-slot precedence is
    /// `:bibliotecas` → `:exe` → `:servicos` — the same declaration
    /// order [`Caixa::declared_foreign_code_slots`] uses for its
    /// canonical foreign-code-slot diagnostic, so a manifest with
    /// multiple malformed slots surfaces the lexicographically-earliest
    /// slot's diagnostic deterministically.
    ///
    /// Lifted to the typed surface as a Caixa-level validator (peer
    /// of [`Self::validate_nome`] / [`Self::validate_versao`] /
    /// [`Self::validate_deps`] / [`Self::validate_restart_window`])
    /// and wired into [`crate::StandardLayout::verify`] before the
    /// existence-check loops so the diagnostic names the offending
    /// slot at the source caixa.lisp rather than reporting a
    /// downstream `MissingEntry` / `ExeOutsideDir` /
    /// `ServicoOutsideDir` against the resolved sandbox-escape path.
    /// The fourth typed code-path surface — every author-supplied
    /// path on the manifest — is now structurally accept-shaped
    /// past validate, peer with `:behavior :on-*` and
    /// `:upgrade-from :state-change :script`.
    pub fn validate_code_paths(&self) -> Result<(), ManifestError> {
        for (slot, list) in [
            (":bibliotecas", &self.bibliotecas),
            (":exe", &self.exe),
            (":servicos", &self.servicos),
        ] {
            for entry in list {
                let path = Path::new(entry);
                match is_sandboxed_relative_path(path) {
                    Ok(()) => {}
                    Err(PathShapeViolation::Empty) => {
                        return Err(ManifestError::CodePathEmpty { slot });
                    }
                    Err(PathShapeViolation::Absolute) => {
                        return Err(ManifestError::CodePathAbsolute {
                            slot,
                            path: path.to_path_buf(),
                        });
                    }
                    Err(PathShapeViolation::ParentEscape) => {
                        return Err(ManifestError::CodePathParentEscape {
                            slot,
                            path: path.to_path_buf(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Reject `:etiquetas` lists with an empty entry or with two entries
    /// agreeing on the same string. `:etiquetas` is the universal
    /// registry-search-tag axis on [`Caixa`] (every kind carries the
    /// `Vec<String>` slot) and lands verbatim as the Helm chart
    /// `Chart.yaml` `keywords:` array on every Servico (caixa-helm's
    /// `build_chart_yaml` at `caixa-helm/src/lib.rs:236` folds it through
    /// a [`std::collections::BTreeSet`] alongside the four substrate-
    /// fixed tags `lareira` / `wasm` / `tatara-lisp` / `caixa-servico`).
    /// Two authoring footguns silently passed validate without this gate:
    ///
    ///   - Empty entry (`(:etiquetas (""))` — the canonical paste-from-
    ///     blank-doc footgun) rendered as `keywords: ["", "caixa-servico",
    ///     "lareira", "tatara-lisp", "wasm"]` in `Chart.yaml`. Helm's
    ///     `chart.metadata.keywords` admits the value without a strict
    ///     parser-side gate, but the empty keyword has no operational
    ///     meaning — it indexes nothing in the future caixa-registry
    ///     search axis and clutters the rendered chart with a no-op tag.
    ///   - Duplicate entries (`(:etiquetas ("demo" "demo"))` — the
    ///     copy-paste-the-wrong-tag footgun) silently passed validate
    ///     and were silently dedup'd by caixa-helm's `BTreeSet` collect
    ///     at chart render — a "second wins / one silently disappears"
    ///     shape divergent from every peer typed-graph set gate
    ///     ([`crate::AplicacaoError::MembroDuplicate`] on `:membros`,
    ///     [`crate::AplicacaoError::PlacementClusterDuplicate`] on
    ///     `:placement :clusters`, [`crate::AplicacaoError::EntradaPathDuplicate`]
    ///     on `:entrada :paths`, [`crate::AplicacaoError::ContratoDuplicate`]
    ///     on `:contratos`, [`crate::DepError::DuplicateNome`] on
    ///     `:deps` / `:deps-dev` per 359fba5, [`crate::UpgradeError::DuplicateFrom`]
    ///     on `:upgrade-from`, the per-instruction-class singularity
    ///     gates [`crate::UpgradeError::DuplicateLoadModule`] /
    ///     [`crate::UpgradeError::DuplicateStateChange`] /
    ///     [`crate::UpgradeError::DuplicateCleanup`]). The typed-graph
    ///     discipline is uniform: every Vec-shaped author-supplied list
    ///     past validate is set-not-multiset, by construction.
    ///
    /// Same empty-first cascade discipline every peer per-axis gate
    /// uses: the per-entry empty arm fires before the cross-entry
    /// duplicate arm, so an `("" "" "demo")` authoring shape surfaces
    /// the narrower [`ManifestError::EtiquetaEmpty`] (the structural
    /// "this entry has no value" defect) rather than collapsing two
    /// unrelated authoring errors into the duplicate diagnostic.
    /// Walks the list in declaration order so the first-collision
    /// diagnostic surfaces the lexicographically-earliest offending
    /// position, peer with every other duplicate gate on this surface.
    ///
    /// Universal-axis (every kind carries `:etiquetas`), so wired at the
    /// caixa-build gate alongside the peer universal gates
    /// [`Self::validate_nome`] / [`Self::validate_versao`] /
    /// [`Self::validate_deps`] / [`Self::validate_code_paths`] — before
    /// the kind-coherence gates ([`crate::LayoutError::MeshSlotsOnNonAplicacao`]
    /// / [`crate::LayoutError::SupervisorSlotsOnNonSupervisor`] /
    /// [`crate::LayoutError::ServicoSlotsOnNonServico`] /
    /// [`crate::LayoutError::ForeignCodeSlot`]) which fence kind-specific
    /// slot sets. The future caixa-registry search axis can reach for
    /// `caixa.etiquetas` knowing every entry is a non-empty distinct
    /// string without re-deriving the precondition.
    pub fn validate_etiquetas(&self) -> Result<(), ManifestError> {
        let mut seen = std::collections::HashSet::new();
        for etiqueta in &self.etiquetas {
            if etiqueta.is_empty() {
                return Err(ManifestError::EtiquetaEmpty);
            }
            if !seen.insert(etiqueta.as_str()) {
                return Err(ManifestError::EtiquetaDuplicate {
                    etiqueta: etiqueta.clone(),
                });
            }
        }
        Ok(())
    }

    /// Reject `:autores` lists with an empty entry or with two entries
    /// agreeing on the same string. `:autores` is the universal
    /// maintainer-axis on [`Caixa`] (every kind carries the
    /// `Vec<String>` slot) and lands verbatim as the Helm chart
    /// `Chart.yaml` `maintainers:` array on every Servico (caixa-helm's
    /// `build_chart_yaml` at `caixa-helm/src/lib.rs:251` maps each entry
    /// to a `Maintainer { name, email: None }` without dedup). Two
    /// authoring footguns silently passed validate without this gate:
    ///
    ///   - Empty entry (`(:autores (""))` — the canonical paste-from-
    ///     blank-doc footgun) rendered as
    ///     `maintainers: [{name: "", email: null}]` in `Chart.yaml`. The
    ///     empty maintainer name has no operational meaning — it
    ///     identifies no one in the substrate's authorship index and
    ///     clutters the rendered chart with a no-op maintainer.
    ///   - Duplicate entries (`(:autores ("pleme-io" "pleme-io"))` —
    ///     the copy-paste-the-wrong-author footgun) silently passed
    ///     validate and rendered as two identical maintainer entries.
    ///     Unlike the [`Self::validate_etiquetas`] peer (caixa-helm's
    ///     `BTreeSet`-collect on `:etiquetas` silently dedups the
    ///     rendered `keywords:` array at chart-render time), the
    ///     `maintainers:` rendering has *no* dedup — duplicate `:autores`
    ///     entries stack verbatim in the chart, divergent from every
    ///     peer typed-graph set gate ([`crate::AplicacaoError::MembroDuplicate`]
    ///     on `:membros`, [`crate::AplicacaoError::PlacementClusterDuplicate`]
    ///     on `:placement :clusters`, [`crate::AplicacaoError::EntradaPathDuplicate`]
    ///     on `:entrada :paths`, [`crate::AplicacaoError::ContratoDuplicate`]
    ///     on `:contratos`, [`crate::DepError::DuplicateNome`] on
    ///     `:deps` / `:deps-dev`, [`crate::UpgradeError::DuplicateFrom`]
    ///     on `:upgrade-from`, [`ManifestError::EtiquetaDuplicate`] on
    ///     `:etiquetas`).
    ///
    /// Same empty-first cascade discipline every peer per-axis gate
    /// uses: the per-entry empty arm fires before the cross-entry
    /// duplicate arm. Walks the list in declaration order so the
    /// first-collision diagnostic surfaces the lexicographically-
    /// earliest offending position, peer with every other duplicate
    /// gate on this surface.
    ///
    /// Universal-axis (every kind carries `:autores`), so wired at the
    /// caixa-build gate alongside the peer universal gates
    /// [`Self::validate_nome`] / [`Self::validate_versao`] /
    /// [`Self::validate_deps`] / [`Self::validate_etiquetas`] /
    /// [`Self::validate_code_paths`] — before the kind-coherence gates
    /// ([`crate::LayoutError::MeshSlotsOnNonAplicacao`] /
    /// [`crate::LayoutError::SupervisorSlotsOnNonSupervisor`] /
    /// [`crate::LayoutError::ServicoSlotsOnNonServico`] /
    /// [`crate::LayoutError::ForeignCodeSlot`]) which fence kind-specific
    /// slot sets.
    pub fn validate_autores(&self) -> Result<(), ManifestError> {
        let mut seen = std::collections::HashSet::new();
        for autor in &self.autores {
            if autor.is_empty() {
                return Err(ManifestError::AutorEmpty);
            }
            if !seen.insert(autor.as_str()) {
                return Err(ManifestError::AutorDuplicate {
                    autor: autor.clone(),
                });
            }
        }
        Ok(())
    }

    /// Reject `:repositorio` values whose shape the shared
    /// [`crate::render::is_git_repo_url`] predicate refuses. The flat
    /// `repositorio: Option<String>` slot on [`Caixa`] is the
    /// universal git-shaped homepage axis every kind carries — the
    /// substrate routes the same string through two load-bearing
    /// consumers:
    ///
    ///   - [`caixa-helm`] folds it verbatim into the rendered
    ///     `lareira-<nome>` Helm chart's `Chart.yaml` `home:` field
    ///     (`build_chart_yaml` at `caixa-helm/src/lib.rs:268`) and into
    ///     the chart `README.md` `repo = …` interpolation
    ///     (`caixa-helm/src/lib.rs:359`).
    ///   - [`caixa-flux`] folds it verbatim into the standalone
    ///     `ClusterBundleOpts::for_caixa` `git_url:` field
    ///     (`caixa-flux/src/lib.rs:293`), which becomes the `FluxCD`
    ///     `GitRepository.spec.url` the cluster's source-controller
    ///     polls — the load-bearing deploy-time axis.
    ///
    /// Both consumers use `Option::unwrap_or_else(|| <fallback>)` to
    /// substitute a placeholder when the slot is absent (`None` → the
    /// fallback fires); a `Some("")` *skips the fallback* and silently
    /// passes the empty string through to `Chart.yaml home: ""` /
    /// `GitRepository url: ""` — Helm's chart lint and `FluxCD`'s source
    /// controller both reject the empty URL far from the source
    /// `caixa.lisp`, with no field naming the offending `:repositorio`.
    /// Similarly a malformed `:repositorio` (whitespace, control char,
    /// missing `:` separator, leading `-`) silently lands in the
    /// rendered artifacts and breaks at `git clone` / `helm template`
    /// / `flux reconcile` time.
    ///
    /// Thin wrapper around [`crate::render::is_git_repo_url`] — the
    /// same shared predicate the peer [`crate::DepSource::validate`]
    /// routes the `:fonte (:tipo git :repo …)` axis through. With this
    /// gate the two `git URL`-shaped surfaces on the typed Caixa
    /// (`:repositorio` here, `:deps :fonte :repo` peer) are
    /// structurally equivalent: every value past validate is
    /// guaranteed-acceptable by the predicate's union of constraints
    /// (non-empty, length-bounded, no leading `-`, no whitespace, no
    /// control chars, ASCII only, no leading `:`, contains a `:`
    /// separator). The predicate accepts every documented authoring
    /// shape — `github:org/repo` shorthand, `https://host/path`,
    /// `ssh://[user@]host/path`, `git://host/path`, `git@host:path`
    /// scp-style SSH, `file:///path` — and refuses the canonical
    /// paste-from-blank-doc / paste-from-multiline-doc / CLI-arg-
    /// injection footguns at validate time. Maps the predicate's
    /// `String` reason verbatim into the
    /// [`ManifestError::RepositorioInvalid`] variant, carrying the
    /// offending value + parser-shaped reason so the diagnostic is
    /// self-locating (the author can grep their `caixa.lisp` for
    /// `:repositorio "<value>"` and fix it in one edit).
    ///
    /// `None` (the canonical "omit the slot to express no published
    /// homepage" shape) is accepted trivially — the gate is a no-op
    /// when the author didn't declare a value. `Some("")` is gated by
    /// the narrower [`ManifestError::RepositorioEmpty`] arm before the
    /// shape predicate is consulted, mirroring the empty-first cascade
    /// every peer per-axis identity gate uses
    /// ([`ManifestError::NomeEmpty`] → [`ManifestError::NomeInvalid`],
    /// [`ManifestError::VersaoEmpty`] → [`ManifestError::VersaoInvalid`],
    /// [`crate::DepError::FonteRepoEmpty`] →
    /// [`crate::DepError::FonteRepoInvalid`]).
    ///
    /// Universal-axis (every kind carries `:repositorio`), so wired at
    /// the caixa-build gate alongside the peer universal gates
    /// [`Self::validate_nome`] / [`Self::validate_versao`] /
    /// [`Self::validate_deps`] / [`Self::validate_etiquetas`] /
    /// [`Self::validate_autores`] / [`Self::validate_code_paths`] —
    /// before the kind-coherence gates
    /// ([`crate::LayoutError::MeshSlotsOnNonAplicacao`] /
    /// [`crate::LayoutError::SupervisorSlotsOnNonSupervisor`] /
    /// [`crate::LayoutError::ServicoSlotsOnNonServico`] /
    /// [`crate::LayoutError::ForeignCodeSlot`]) which fence kind-
    /// specific slot sets.
    pub fn validate_repositorio(&self) -> Result<(), ManifestError> {
        let Some(s) = self.repositorio.as_deref() else {
            return Ok(());
        };
        if s.is_empty() {
            return Err(ManifestError::RepositorioEmpty);
        }
        is_git_repo_url(s).map_err(|reason| ManifestError::RepositorioInvalid {
            repositorio: s.to_string(),
            reason,
        })
    }

    /// Reject `:descricao` values that are the empty string. The flat
    /// `descricao: Option<String>` slot on [`Caixa`] is the universal
    /// free-form-prose homepage axis every kind carries — the
    /// substrate routes the same string through two load-bearing
    /// consumers in the [`caixa-helm`] renderer:
    ///
    ///   - `build_chart_yaml` folds it verbatim into the rendered
    ///     `lareira-<nome>` Helm chart's `Chart.yaml` `description:`
    ///     field (`caixa-helm/src/lib.rs:232-235`).
    ///   - `build_readme` folds it verbatim into the rendered chart
    ///     `README.md` header (`caixa-helm/src/lib.rs:333-336`).
    ///
    /// Both consumers use `Option::unwrap_or_else(|| <fallback>)` to
    /// substitute a `caixa.nome`-derived placeholder when the slot is
    /// absent (`None` → the fallback fires); a `Some("")` *skips the
    /// fallback* and silently passes the empty string through to
    /// `Chart.yaml description: ""` / a blank chart `README.md`
    /// header. Helm's chart spec requires a non-empty `description:`
    /// field on `apiVersion: v2` charts (`helm lint` surfaces it as
    /// `WARNING [chart.metadata.description]: description is required`),
    /// so the empty `Some("")` silently lands in the rendered
    /// artifacts and breaks at `helm lint` / `helm install` time far
    /// from the source `caixa.lisp`, with no field naming the
    /// offending `:descricao`.
    ///
    /// `None` (the canonical "omit the slot to defer to the renderer's
    /// `caixa.nome`-derived fallback" shape) is accepted trivially —
    /// the gate is a no-op when the author didn't declare a value.
    /// `Some("")` is gated by the narrower
    /// [`ManifestError::DescricaoEmpty`] arm, mirroring the empty-arm
    /// shape every peer per-axis empty gate uses
    /// ([`ManifestError::NomeEmpty`], [`ManifestError::VersaoEmpty`],
    /// [`ManifestError::EtiquetaEmpty`], [`ManifestError::AutorEmpty`],
    /// [`ManifestError::RepositorioEmpty`]).
    ///
    /// Universal-axis (every kind carries `:descricao`), so wired at
    /// the caixa-build gate alongside the peer universal gates
    /// [`Self::validate_nome`] / [`Self::validate_versao`] /
    /// [`Self::validate_deps`] / [`Self::validate_etiquetas`] /
    /// [`Self::validate_autores`] / [`Self::validate_repositorio`] /
    /// [`Self::validate_code_paths`] — before the kind-coherence
    /// gates ([`crate::LayoutError::MeshSlotsOnNonAplicacao`] /
    /// [`crate::LayoutError::SupervisorSlotsOnNonSupervisor`] /
    /// [`crate::LayoutError::ServicoSlotsOnNonServico`] /
    /// [`crate::LayoutError::ForeignCodeSlot`]) which fence kind-
    /// specific slot sets.
    pub fn validate_descricao(&self) -> Result<(), ManifestError> {
        let Some(s) = self.descricao.as_deref() else {
            return Ok(());
        };
        if s.is_empty() {
            return Err(ManifestError::DescricaoEmpty);
        }
        Ok(())
    }

    /// Reject `:licenca` values that are the empty string. The flat
    /// `licenca: Option<String>` slot on [`Caixa`] is the universal
    /// SPDX-shaped license-expression axis every kind carries — the
    /// substrate routes the same string through the [`caixa-helm`]
    /// renderer's `build_readme` which folds it verbatim into the
    /// rendered `lareira-<nome>` Helm chart's `README.md` `## License`
    /// section (`caixa-helm/src/lib.rs:361`) via
    /// `caixa.licenca.clone().unwrap_or_else(|| "MIT".into())`. The
    /// fallback only fires on `None`; a `Some("")` *skips the
    /// fallback* and silently passes the empty string through to a
    /// chart `README.md` whose `License` section renders as the bare
    /// trailing period (`.\n`) — peer footgun with the
    /// `Some("")`-skips-`unwrap_or_else` shape the
    /// [`Self::validate_descricao`] and [`Self::validate_repositorio`]
    /// gates close on the sibling free-form-prose and git-URL axes.
    ///
    /// `None` (the canonical "omit the slot to defer to the
    /// renderer's `MIT` fallback" shape every existing fixture
    /// carries) is accepted trivially — the gate is a no-op when the
    /// author didn't declare a value. `Some("")` is gated by the
    /// narrower [`ManifestError::LicencaEmpty`] arm, mirroring the
    /// empty-arm shape every peer per-axis empty gate uses
    /// ([`ManifestError::NomeEmpty`], [`ManifestError::VersaoEmpty`],
    /// [`ManifestError::EtiquetaEmpty`], [`ManifestError::AutorEmpty`],
    /// [`ManifestError::RepositorioEmpty`],
    /// [`ManifestError::DescricaoEmpty`]).
    ///
    /// Universal-axis (every kind carries `:licenca`), so wired at
    /// the caixa-build gate alongside the peer universal gates
    /// [`Self::validate_nome`] / [`Self::validate_versao`] /
    /// [`Self::validate_deps`] / [`Self::validate_etiquetas`] /
    /// [`Self::validate_autores`] / [`Self::validate_repositorio`] /
    /// [`Self::validate_descricao`] / [`Self::validate_code_paths`]
    /// — before the kind-coherence gates
    /// ([`crate::LayoutError::MeshSlotsOnNonAplicacao`] /
    /// [`crate::LayoutError::SupervisorSlotsOnNonSupervisor`] /
    /// [`crate::LayoutError::ServicoSlotsOnNonServico`] /
    /// [`crate::LayoutError::ForeignCodeSlot`]) which fence kind-
    /// specific slot sets.
    ///
    /// Past the empty arm the gate enforces the SPDX-expression shape
    /// predicate via [`crate::render::is_spdx_expression_shape`]: the
    /// structural alphabet floor every realistic SPDX expression in
    /// the wild uses — ASCII alphanumeric plus `.`, `-`, `+`, `(`,
    /// `)`, `:` (the `DocumentRef-…:LicenseRef-…` separator), and a
    /// single ASCII space (token separator). Closes the canonical
    /// paste-from-doc footguns the bare empty-arm gate left open:
    /// paste-from-doc whitespace (`"MIT "`, `" MIT"`), paste-from-
    /// multiline-doc CRLF (`"MIT\n"`), tab-from-aligned-doc
    /// (`"MIT\tOR Apache-2.0"`), non-ASCII smart-quote paste,
    /// underscore-instead-of-hyphen typo (`"Apache_2.0"`),
    /// comma-instead-of-`OR`-keyword colloquial idiom (`"MIT,
    /// Apache-2.0"`), slash-dual-license colloquial idiom (`"MIT/
    /// Apache-2.0"`), and semicolon-list-separator confusion
    /// (`"MIT; Apache-2.0"`). Mirrors the shape-predicate cascade
    /// [`Self::validate_repositorio`] / [`Self::validate_edicao`]
    /// establish past their own empty arms.
    ///
    /// The empty-first cascade discipline mirrors every peer per-axis
    /// identity gate: [`ManifestError::LicencaEmpty`] runs before
    /// [`ManifestError::LicencaInvalid`], so the narrower empty
    /// diagnostic surfaces on `Some("")` rather than the broader
    /// shape-predicate diagnostic — peer with how
    /// [`ManifestError::EdicaoEmpty`] runs before
    /// [`ManifestError::EdicaoInvalid`],
    /// [`ManifestError::RepositorioEmpty`] runs before
    /// [`ManifestError::RepositorioInvalid`].
    ///
    /// A future tightening on this axis can extend the alphabet
    /// floor into a full SPDX expression parser + license-id
    /// allowlist (rejecting alphabet-valid values that don't name a
    /// real SPDX license identifier — e.g., `"NotAReal"` is
    /// alphabet-valid but no `NotAReal` license-id exists). That
    /// parser only becomes meaningful past a real SPDX-spec
    /// dependency; this gate establishes the structural floor by
    /// refusing every non-SPDX-alphabet value at validate time.
    pub fn validate_licenca(&self) -> Result<(), ManifestError> {
        let Some(s) = self.licenca.as_deref() else {
            return Ok(());
        };
        if s.is_empty() {
            return Err(ManifestError::LicencaEmpty);
        }
        crate::render::is_spdx_expression_shape(s).map_err(|reason| {
            ManifestError::LicencaInvalid {
                licenca: s.to_string(),
                reason,
            }
        })?;
        Ok(())
    }

    /// Reject `:edicao` values that are the empty string. The flat
    /// `edicao: Option<String>` slot on [`Caixa`] is the universal
    /// language-edition axis every kind carries — it determines the
    /// tatara-lisp macro surface + compatibility flags the substrate
    /// applies when building a caixa, and lands verbatim in the
    /// `Caixa::template` author-time scaffold (the canonical
    /// `:edicao "2026"` line every `feira init` emits via
    /// [`Caixa::template`] at `caixa-core/src/manifest.rs:1193`) and
    /// in every renderer-side fixture (`caixa-helm/src/lib.rs:375`,
    /// `caixa-flux/src/lib.rs:445`, `caixa-mesh/src/lib.rs:629`,
    /// `caixa-core/src/render.rs:2510`) via
    /// `edicao: Some("2026".into())`.
    ///
    /// `None` (the canonical "omit the slot to defer to the
    /// substrate's default edition" shape every existing
    /// [`caixa-resolver`] integration test fixture carries via
    /// `edicao: None` — see `caixa-resolver/tests/git_integration.rs`)
    /// is accepted trivially — the gate is a no-op when the author
    /// didn't declare a value. `Some("")` is gated by the narrower
    /// [`ManifestError::EdicaoEmpty`] arm, mirroring the empty-arm
    /// shape every peer per-axis empty gate uses
    /// ([`ManifestError::NomeEmpty`], [`ManifestError::VersaoEmpty`],
    /// [`ManifestError::EtiquetaEmpty`], [`ManifestError::AutorEmpty`],
    /// [`ManifestError::RepositorioEmpty`],
    /// [`ManifestError::DescricaoEmpty`], [`ManifestError::LicencaEmpty`]).
    ///
    /// Universal-axis (every kind carries `:edicao`), so wired at
    /// the caixa-build gate alongside the peer universal gates
    /// [`Self::validate_nome`] / [`Self::validate_versao`] /
    /// [`Self::validate_deps`] / [`Self::validate_etiquetas`] /
    /// [`Self::validate_autores`] / [`Self::validate_repositorio`] /
    /// [`Self::validate_descricao`] / [`Self::validate_licenca`] /
    /// [`Self::validate_code_paths`] — before the kind-coherence
    /// gates ([`crate::LayoutError::MeshSlotsOnNonAplicacao`] /
    /// [`crate::LayoutError::SupervisorSlotsOnNonSupervisor`] /
    /// [`crate::LayoutError::ServicoSlotsOnNonServico`] /
    /// [`crate::LayoutError::ForeignCodeSlot`]) which fence kind-
    /// specific slot sets.
    ///
    /// Past the empty arm the gate enforces the canonical year-shape
    /// predicate: every documented tatara-lisp edition is a 4-digit
    /// ASCII decimal year (`"2026"` is the only edition currently
    /// minted; future-introduced siblings will follow the same
    /// shape, peer with Cargo's `[package] edition` grammar which
    /// every value Cargo has ever accepted matches — `"2015"`,
    /// `"2018"`, `"2021"`, `"2024"`). Any value that's not exactly
    /// 4 ASCII decimal bytes is rejected with the narrower
    /// [`ManifestError::EdicaoInvalid`] arm, mirroring the
    /// shape-predicate cascade [`Self::validate_repositorio`]
    /// establishes past its own empty arm
    /// ([`ManifestError::RepositorioEmpty`] →
    /// [`ManifestError::RepositorioInvalid`]). Closes the canonical
    /// paste-from-doc footguns the bare empty-arm gate left open:
    ///
    ///   - leading / trailing whitespace from a paste-from-doc
    ///     (`"2026 "`, `" 2026"`)
    ///   - control characters / CRLF from a paste-from-multiline-doc
    ///     (`"2026\n"`)
    ///   - non-ASCII look-alikes from a fullwidth keyboard
    ///     (`"２０２６"`) which would silently land as a non-ASCII
    ///     string in the rendered caixa.lisp
    ///   - free-form non-year values (`"x"`, `"latest"`,
    ///     `"nightly"`) that have no operational meaning on the
    ///     substrate's build-time edition selector
    ///   - leading non-digit prefixes (`"v2026"`, `"e2026"`,
    ///     `"r2026"`) — common version-tag idioms that don't apply
    ///     to the year-shaped edition axis
    ///   - decimal-shaped values (`"2026.1"`, `"2026.0"`) — every
    ///     edition is a year, not a fractional version
    ///   - wrong-length numeric values (`"26"`, `"202"`, `"20260"`,
    ///     `"00026"`) that don't name a year
    ///
    /// `None` (the canonical "omit the slot to defer to the
    /// substrate's default edition" shape every existing
    /// [`caixa-resolver`] integration test fixture carries via
    /// `edicao: None` — see `caixa-resolver/tests/git_integration.rs`)
    /// is accepted trivially — the gate is a no-op when the author
    /// didn't declare a value. The empty-first cascade discipline
    /// mirrors every peer per-axis identity gate:
    /// [`ManifestError::EdicaoEmpty`] runs before
    /// [`ManifestError::EdicaoInvalid`], so the narrower empty
    /// diagnostic surfaces on `Some("")` rather than the broader
    /// shape-predicate diagnostic — peer with how
    /// [`ManifestError::NomeEmpty`] runs before
    /// [`ManifestError::NomeInvalid`],
    /// [`ManifestError::VersaoEmpty`] runs before
    /// [`ManifestError::VersaoInvalid`],
    /// [`ManifestError::RepositorioEmpty`] runs before
    /// [`ManifestError::RepositorioInvalid`].
    ///
    /// A future tightening on this axis can extend the shape
    /// predicate into a known-edition allowlist (rejecting
    /// year-shaped values that don't name a tatara-lisp edition
    /// the substrate actually understands — e.g., `"1999"` is
    /// year-shaped but no `1999` edition exists). That allowlist
    /// only becomes meaningful past the introduction of a sibling
    /// edition to `"2026"`; this gate establishes the structural
    /// floor by refusing every non-year-shaped value at validate
    /// time.
    pub fn validate_edicao(&self) -> Result<(), ManifestError> {
        let Some(s) = self.edicao.as_deref() else {
            return Ok(());
        };
        if s.is_empty() {
            return Err(ManifestError::EdicaoEmpty);
        }
        if s.len() != 4 || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ManifestError::EdicaoInvalid {
                edicao: s.to_string(),
                reason: "must be a 4-digit ASCII decimal year (canonical \"2026\")".to_string(),
            });
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
        // Fold through the shared `supervisor::duration_codec::parse`
        // — the same parser the serde-routed `with = "duration_codec"`
        // on `SupervisorSpec::restart_window`, the `:politicas
        // :timeout` codec, and the `:politicas :circuit-breaker
        // :window` codec all consume. The prior inline f64-shaped
        // duplicate (`parse_window_inline`) admitted every magnitude
        // the integer-magnitude gate (1c55a2a) rejects on the three
        // serde-routed siblings — `"1.5s"`, `"1.0s"`, `"0.5m"`,
        // `"+30s"`, `"-30s"` — and silently dropped malformed input as
        // `None` (i.e. "no reset"), divergent from the shared codec's
        // integer-magnitude discipline by construction. The fold
        // closes the divergence: every value the typed
        // `SupervisorSpec` carries past `supervisor_view` is in the
        // shared codec's accepted set. The `.ok()` here preserves the
        // existing soft-swallow shape on this view-construction path;
        // the new [`Caixa::validate_restart_window`] (sibling of
        // [`Self::validate_nome`] / [`Self::validate_versao`]) names
        // the offending raw string at build time so authoring tools
        // (`feira lint`, the future layout-side wire-up) surface a
        // self-locating diagnostic instead of a silently dropped
        // window.
        let restart_window = self
            .restart_window
            .as_deref()
            .and_then(|s| crate::supervisor::duration_codec::parse(s).ok());
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

/// Errors raised by top-level [`Caixa`] validators that don't fit
/// the per-axis [`DepError`] / [`crate::AplicacaoError`] /
/// [`crate::SupervisorError`] / [`crate::LayoutError`] families —
/// the Caixa's own identity axes (`:nome`, `:versao`) that flow
/// through every substrate-side artifact's `metadata.name` /
/// version derivation.
///
/// A future top-level sum (the M4 `CaixaError` the [`DepError`]
/// doc-comment anticipates) can hold one of each per-axis error
/// family without reshaping individual diagnostics; this enum is
/// the first such per-Caixa-identity family.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error(
        ":nome is empty (every caixa must name itself; the value flows \
         into every K8s artifact's `metadata.name` derivation and into \
         the default `lib/<nome>.lisp` / `exe/<nome>` layout paths)"
    )]
    NomeEmpty,
    #[error(
        ":nome {nome:?} is not a valid DNS-1123 label: {reason} (the K8s \
         apiserver enforces this rule on every `metadata.name` the \
         caixa's substrate-side renderers derive from `:nome` — the \
         `lareira-<nome>` Helm chart name, the programs.yaml entry \
         name, the `LABEL_APLICACAO` label value, the `<aplicacao>-<de>-to-<para>` \
         CiliumNetworkPolicy name, the `<aplicacao>-<para>` HTTPRoute \
         name; use a lowercase alphanumeric + hyphen identifier like \
         `\"checkout\"` or `\"cart-v2\"`)"
    )]
    NomeInvalid { nome: String, reason: String },
    #[error(
        ":versao is empty (every caixa must pin its own version; the value flows \
         into the `lareira-<nome>` Helm chart's `Chart.yaml` version + appVersion, \
         the `feira publish` `v<versao>` git tag, the OCI image's `:v<versao>` / \
         `:latest` tags, the lacre closure's `concrete_versao`, and the \
         `:upgrade-from :from` peers — use a SemVer-2 literal like `\"0.1.0\"`)"
    )]
    VersaoEmpty,
    #[error(
        ":versao {versao:?} is not a valid SemVer-2 version: {reason} (the substrate \
         consumes this string as `semver::Version` — three-part `MAJOR.MINOR.PATCH` \
         with optional `-prerelease` and `+build` — across every artifact derived \
         from `:versao`: the `lareira-<nome>` Helm chart's `Chart.yaml` version + \
         appVersion (Helm SemVer-2-strict), the `feira publish` `v<versao>` git tag, \
         the OCI image's `:v<versao>` tag, the lacre closure's `concrete_versao`, \
         and the `:upgrade-from :from` peers that match against this exact shape; \
         use a literal like `\"0.1.0\"`, `\"0.2.0-rc.1\"`, or `\"1.0.0+build.42\"` — \
         not a git-tag-shape like `\"v0.1.0\"`, a docker-tag-shape like `\"latest\"`, \
         a requirement-shape like `\"^0.1\"`, or a four-part `\"0.1.0.0\"`)"
    )]
    VersaoInvalid { versao: String, reason: String },
    #[error(
        ":restart-window {restart_window:?} is not a valid duration: {reason} (the \
         substrate consumes this string through the shared \
         `supervisor::duration_codec` — the same parser routed via `with = \
         \"duration_codec\"` onto the typed `SupervisorSpec::restart_window`, \
         `:politicas :timeout`, and `:politicas :circuit-breaker :window` slots; \
         the canonical authoring form is `<integer><unit>` where the unit is one \
         of `ms` / `s` / `m` / `h` and the magnitude has no decimal point and no \
         leading `+` / `-` sign — e.g. `\"60s\"`, `\"5m\"`, `\"1h\"`, `\"500ms\"`. \
         Without this gate a malformed `:restart-window` silently produced a \
         supervisor with `restart_window: None` (\"never reset\"), turning OTP's \
         `MaxIntensity / Period` invariant into a never-reset supervisor far from \
         the source `caixa.lisp`; the gate moves the diagnostic to the manifest \
         layer with the offending value named verbatim. Omit the slot entirely to \
         express \"no reset\"; carry a positive integer duration to express the \
         sliding window)"
    )]
    RestartWindowMalformed {
        restart_window: String,
        reason: String,
    },
    #[error(
        "{slot} entry is an empty path string — every {slot} entry must name \
         a file relative to the caixa root; omit the entry to omit the file \
         (the layout checker's `root.join(\"\")` resolves to the caixa root \
         itself, so an empty entry silently aliases the project root as a \
         declared {slot} file, then fails downstream at parse / existence \
         time with a diagnostic that names the root rather than the offending \
         entry)"
    )]
    CodePathEmpty { slot: &'static str },
    #[error(
        "{slot} entry {} is an absolute path — entries must be relative to \
         the caixa root, since `Path::join` replaces the base with an absolute \
         right-hand side and `root.join(\"/abs/...\")` resolves to \"/abs/...\" \
         outside the caixa root sandbox; rewrite the entry as a relative path \
         under the caixa root (e.g. `\"lib/<name>.lisp\"`, `\"exe/<name>\"`, \
         `\"servicos/<name>.computeunit.yaml\"`)",
        path.display()
    )]
    CodePathAbsolute { slot: &'static str, path: PathBuf },
    #[error(
        "{slot} entry {} contains a `..` component — entries must not traverse \
         above the caixa root (the layout's `starts_with(<dir>)` fence on \
         `:exe` / `:servicos` is component-aware, not canonical-path-aware, \
         so a mid-path `..` silently traverses the sandbox; `:bibliotecas` \
         has no such fence, so a leading `..` escapes unconditionally if the \
         resolved target happens to exist)",
        path.display()
    )]
    CodePathParentEscape { slot: &'static str, path: PathBuf },
    #[error(
        ":etiquetas entry is empty (every tag must carry a non-empty \
         registry-search identifier; the empty entry has no operational \
         meaning — it indexes nothing in the future caixa-registry search \
         axis and clutters the rendered Helm `Chart.yaml` `keywords:` array \
         with a no-op tag; omit the entry to express \"no tag on this \
         position\")"
    )]
    EtiquetaEmpty,
    #[error(
        ":etiquetas entry {etiqueta:?} appears more than once (the \
         registry-search tag set is a set, not a multiset; duplicate \
         entries are silently dedup'd by caixa-helm's `BTreeSet` collect \
         at chart render — a \"second wins / one silently disappears\" \
         shape divergent from every peer typed-graph set gate \
         (`:membros :caixa`, `:placement :clusters`, `:entrada :paths`, \
         `:contratos`, `:deps :nome`, `:upgrade-from :from`); drop the \
         duplicate or rename it to the actual tag intended)"
    )]
    EtiquetaDuplicate { etiqueta: String },
    #[error(
        ":autores entry is empty (every maintainer must carry a non-empty \
         identifier; the empty entry has no operational meaning — it \
         identifies no one in the substrate's authorship index and renders \
         as `maintainers: [{{name: \"\", email: null}}]` in the Helm chart's \
         `Chart.yaml`, a no-op maintainer the substrate cannot route to; \
         omit the entry to express \"no maintainer on this position\")"
    )]
    AutorEmpty,
    #[error(
        ":autores entry {autor:?} appears more than once (the maintainer \
         set is a set, not a multiset; unlike `:etiquetas`, caixa-helm's \
         `maintainers:` rendering does *no* dedup — duplicate entries \
         stack verbatim in `Chart.yaml` as two identical \
         `Maintainer {{ name, email: None }}` records, divergent from every \
         peer typed-graph set gate (`:etiquetas`, `:membros :caixa`, \
         `:placement :clusters`, `:entrada :paths`, `:contratos`, \
         `:deps :nome`, `:upgrade-from :from`); drop the duplicate or \
         rename it to the actual author intended)"
    )]
    AutorDuplicate { autor: String },
    #[error(
        ":repositorio is the empty string (every published caixa names its \
         git source via a non-empty `:repositorio` locator — the value \
         flows verbatim into the rendered `lareira-<nome>` Helm chart's \
         `Chart.yaml` `home:` field via `caixa-helm` and into the FluxCD \
         `GitRepository.spec.url` via `caixa-flux`'s \
         `ClusterBundleOpts::for_caixa`; both consumers' \
         `Option::unwrap_or_else` fallbacks only fire when the slot is \
         `None`, so an empty `Some(\"\")` silently lands as `home: \"\"` / \
         `url: \"\"` in the rendered artifacts and breaks at `helm \
         template` / FluxCD source-controller reconcile time far from the \
         source caixa.lisp; omit the slot entirely to defer to the \
         renderer's `https://github.com/pleme-io/<nome>` / \
         `caixa.nome`-derived fallback, or carry a canonical authoring \
         shape like `\"github:org/repo\"`, `\"https://host/path\"`, \
         `\"ssh://[user@]host/path\"`, `\"git@host:path\"`, or \
         `\"file:///path\"`)"
    )]
    RepositorioEmpty,
    #[error(
        ":repositorio {repositorio:?} is not a valid git repo URL: {reason} \
         (the substrate consumes this string through the shared \
         `crate::render::is_git_repo_url` predicate — the same parser the \
         peer `:deps :fonte (:tipo git :repo …)` axis routes its `:repo` \
         value through via `DepSource::validate`; the canonical authoring \
         shapes are `\"github:org/repo\"` shorthand, `\"https://host/path\"` \
         / `\"ssh://[user@]host/path\"` / `\"git://host/path\"` / \
         `\"file:///path\"` URL schemes, or the `\"git@host:path\"` \
         scp-style SSH form. Without this gate a malformed `:repositorio` \
         (whitespace from a paste-from-doc; control characters / CRLF \
         from a paste-from-multiline-doc; a leading `-` from a \
         CLI-argument-injection footgun; a missing `:` separator from a \
         bare `org/repo` shape git treats as a relative filesystem path) \
         silently landed in the rendered `Chart.yaml home:` and the \
         FluxCD `GitRepository.spec.url` and broke at `git clone` / \
         FluxCD reconcile time far from the source caixa.lisp; the gate \
         moves the diagnostic to the manifest layer with the offending \
         value named verbatim)"
    )]
    RepositorioInvalid { repositorio: String, reason: String },
    #[error(
        ":descricao is the empty string (every published caixa names \
         its purpose via a non-empty `:descricao` summary — the value \
         flows verbatim into the rendered `lareira-<nome>` Helm \
         chart's `Chart.yaml` `description:` field via `caixa-helm`'s \
         `build_chart_yaml` and into the chart `README.md` header via \
         `build_readme`; both consumers' `Option::unwrap_or_else` \
         `caixa.nome`-derived fallbacks only fire when the slot is \
         `None`, so an empty `Some(\"\")` silently lands as \
         `description: \"\"` / a blank `README.md` header in the \
         rendered artifacts and breaks at `helm lint` time \
         (`WARNING [chart.metadata.description]: description is \
         required` on `apiVersion: v2` charts) far from the source \
         caixa.lisp; omit the slot entirely to defer to the \
         renderer's `\"Generated chart for caixa Servico <nome>\"` / \
         `\"caixa Servico <nome>\"` fallbacks, or carry a non-empty \
         summary like `\"Canonical Rust→wasm32-wasip2 caixa \
         Servico.\"`)"
    )]
    DescricaoEmpty,
    #[error(
        ":licenca is the empty string (every published caixa names \
         its license via a non-empty `:licenca` SPDX expression — the \
         value flows verbatim into the rendered `lareira-<nome>` Helm \
         chart's `README.md` `## License` section via `caixa-helm`'s \
         `build_readme` at `caixa-helm/src/lib.rs:361`; the consumer's \
         `Option::unwrap_or_else(|| \"MIT\".into())` `MIT` fallback \
         only fires when the slot is `None`, so an empty `Some(\"\")` \
         silently lands as a bare trailing period in the rendered \
         chart `README.md` `License` section far from the source \
         caixa.lisp; omit the slot entirely to defer to the \
         renderer's `MIT` fallback, or carry a canonical SPDX \
         expression like `\"MIT\"`, `\"Apache-2.0\"`, \
         `\"Apache-2.0 OR MIT\"`)"
    )]
    LicencaEmpty,
    #[error(
        ":licenca {licenca:?} is not a valid SPDX expression shape: {reason} \
         (the substrate consumes this string through the shared \
         `crate::render::is_spdx_expression_shape` predicate — the same \
         alphabet-floor parser every peer per-axis value-shape gate routes \
         its value through; the canonical authoring shapes are single \
         license identifiers like `\"MIT\"`, `\"Apache-2.0\"`, `\"BSD-3-Clause\"`, \
         compound expressions like `\"Apache-2.0 OR MIT\"`, \
         `\"MIT AND BSD-3-Clause\"`, `\"(MIT OR Apache-2.0) AND ISC\"`, \
         license-with-exception forms like `\"Apache-2.0 WITH LLVM-exception\"`, \
         `+`-suffix variants like `\"GPL-2.0+\"`, and user-defined references \
         like `\"LicenseRef-MyLicense\"` / \
         `\"DocumentRef-doc:LicenseRef-MyLicense\"`. Without this gate a \
         malformed `:licenca` (paste-from-doc whitespace `\"MIT \"` / \
         `\" MIT\"`; paste-from-multiline-doc CRLF `\"MIT\\n\"`; \
         tab-from-aligned-doc `\"MIT\\tOR Apache-2.0\"`; non-ASCII byte from \
         a smart-quote paste; underscore-instead-of-hyphen typo \
         `\"Apache_2.0\"`; comma-instead-of-`OR`-keyword colloquial idiom \
         `\"MIT, Apache-2.0\"`; slash-dual-license colloquial idiom \
         `\"MIT/Apache-2.0\"`; semicolon-list-separator confusion \
         `\"MIT; Apache-2.0\"`) silently landed in the rendered chart \
         `README.md` `## License` section + a future SPDX-aware \
         `Chart.yaml license:` emitter would refuse the value at \
         `helm lint` time far from the source caixa.lisp; the gate moves \
         the diagnostic to the manifest layer with the offending value \
         named verbatim)"
    )]
    LicencaInvalid { licenca: String, reason: String },
    #[error(
        ":edicao is the empty string (every published caixa names \
         its language edition via a non-empty `:edicao` value — the \
         edition determines the tatara-lisp macro surface + \
         compatibility flags the substrate applies when building \
         the caixa; the canonical `Caixa::template` scaffold every \
         `feira init` emits carries `:edicao \"2026\"` verbatim and \
         every renderer-side fixture (`caixa-helm`, `caixa-flux`, \
         `caixa-mesh`) carries `edicao: Some(\"2026\".into())` by \
         construction, so an empty `Some(\"\")` silently lands as a \
         bare `(:edicao \"\")` line in the rendered `caixa.lisp` and \
         a future renderer-side consumer that folds it through \
         `Option::unwrap_or_else` will skip the fallback and pass the \
         empty edition through to the substrate's build-time edition \
         selector far from the source caixa.lisp; omit the slot \
         entirely to defer to the substrate's default edition, or \
         carry a canonical edition like `\"2026\"`)"
    )]
    EdicaoEmpty,
    #[error(
        ":edicao {edicao:?} is not a valid edition: {reason} (every \
         documented tatara-lisp edition is a 4-digit ASCII decimal \
         year — `\"2026\"` is the only edition currently minted; \
         future-introduced siblings will follow the same shape, peer \
         with Cargo's `[package] edition` grammar which every value \
         Cargo has ever accepted matches: `\"2015\"`, `\"2018\"`, \
         `\"2021\"`, `\"2024\"`. Without this gate the canonical \
         paste-from-doc footguns silently passed: a trailing space \
         (`\"2026 \"`) from a paste-from-doc, a CRLF (`\"2026\\n\"`) \
         from a paste-from-multiline-doc, a fullwidth-keyboard \
         look-alike (`\"２０２６\"`), a free-form non-year value \
         (`\"x\"`, `\"latest\"`, `\"nightly\"`), a leading non-digit \
         version-tag prefix (`\"v2026\"`, `\"e2026\"`), a \
         decimal-shaped pseudo-version (`\"2026.1\"`), or a \
         wrong-length numeric value (`\"26\"`, `\"202\"`, \
         `\"20260\"`) all landed as `(:edicao \"<garbage>\")` in the \
         rendered caixa.lisp and broke at the substrate's \
         build-time edition selector far from the source caixa.lisp; \
         omit the slot entirely to defer to the substrate's default \
         edition, or carry a canonical 4-digit ASCII decimal year \
         like `\"2026\"`)"
    )]
    EdicaoInvalid { edicao: String, reason: String },
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
        assert_eq!(
            view.restart_window,
            Some(std::time::Duration::from_secs(60))
        );
        assert_eq!(view.children.len(), 1);
        view.validate().unwrap();
    }

    #[test]
    fn supervisor_view_none_for_non_supervisor_kinds() {
        let c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        assert!(c.supervisor_view().is_none());
    }

    #[test]
    fn declared_mesh_slots_empty_for_bare_caixa() {
        let c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        assert!(c.declared_mesh_slots().is_empty());
    }

    #[test]
    fn declared_mesh_slots_reports_only_set_slots_in_canonical_order() {
        use crate::{Entrada, Membro};
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        // Set a non-adjacent pair (:membros + :entrada) to pin that the
        // canonical declaration order is preserved regardless of which
        // subset is populated.
        c.membros = vec![Membro {
            caixa: "a".into(),
            versao: "^0.1".into(),
        }];
        c.entrada = Some(Entrada {
            host: "x.example.com".into(),
            para: "a".into(),
            paths: vec![],
            port: 8080,
        });
        assert_eq!(c.declared_mesh_slots(), vec![":membros", ":entrada"]);
    }

    #[test]
    fn declared_supervisor_slots_empty_for_bare_caixa() {
        let c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        assert!(c.declared_supervisor_slots().is_empty());
    }

    #[test]
    fn declared_supervisor_slots_reports_only_set_slots_in_canonical_order() {
        use crate::RestartStrategy;
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        // Set a non-adjacent pair (:estrategia + :restart-window) to pin
        // that the canonical declaration order is preserved regardless
        // of which subset is populated.
        c.estrategia = Some(RestartStrategy::OneForOne);
        c.restart_window = Some("60s".into());
        assert_eq!(
            c.declared_supervisor_slots(),
            vec![":estrategia", ":restart-window"]
        );
    }

    #[test]
    fn declared_servico_slots_empty_for_bare_caixa() {
        let c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        assert!(c.declared_servico_slots().is_empty());
    }

    #[test]
    fn declared_servico_slots_reports_only_set_slots_in_canonical_order() {
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        // Set a non-adjacent pair (:limits + :upgrade-from) to pin that
        // the canonical declaration order is preserved regardless of
        // which subset is populated.
        c.limits = Some(crate::LimitsSpec {
            fuel: Some(1_000_000),
            ..Default::default()
        });
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![UpgradeInstruction::Restart],
        }];
        assert_eq!(c.declared_servico_slots(), vec![":limits", ":upgrade-from"]);
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
    fn validate_deps_rejects_ambiguous_fonte_in_deps_dev() {
        // Cross-axis pin: `validate_deps` walks both :deps and
        // :deps-dev through `Dep::validate`, and the new fonte gate
        // (`:tag` + `:branch` both set — the canonical "pin drift"
        // footgun) must surface from the :deps-dev arm with the
        // offending entry's :nome named. Pin the :deps-dev arm
        // explicitly so a future shortcut that only walks :deps
        // surfaces here as a regression.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps_dev = vec![Dep {
            nome: "dev-only".into(),
            versao: "^0.1".into(),
            fonte: Some(crate::DepSource::Git {
                repo: "github:p/x".into(),
                tag: Some("v1".into()),
                rev: None,
                branch: Some("main".into()),
            }),
            opcional: false,
            caracteristicas: vec![],
        }];
        let err = c.validate_deps().unwrap_err();
        let crate::dep::DepError::FontePinAmbiguous { nome, pins } = err else {
            panic!("expected FontePinAmbiguous from :deps-dev walk");
        };
        assert_eq!(nome, "dev-only");
        assert!(pins.contains(":tag") && pins.contains(":branch"));
    }

    #[test]
    fn validate_deps_rejects_empty_repo_in_deps() {
        // Parity pin on the :deps arm: an empty :repo on the runtime
        // deps list surfaces the same FonteRepoEmpty diagnostic the
        // dep.rs per-entry tests pin, naming the offending entry.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps = vec![Dep {
            nome: "runtime".into(),
            versao: "^0.1".into(),
            fonte: Some(crate::DepSource::Git {
                repo: String::new(),
                tag: Some("v1".into()),
                rev: None,
                branch: None,
            }),
            opcional: false,
            caracteristicas: vec![],
        }];
        let err = c.validate_deps().unwrap_err();
        assert!(
            matches!(
                err,
                crate::dep::DepError::FonteRepoEmpty { ref nome }
                    if nome == "runtime"
            ),
            "got {err:?}"
        );
    }

    // ── validate_deps: within-list :nome set-not-multiset gate ─────────

    #[test]
    fn validate_deps_rejects_duplicate_nome_in_deps() {
        // Fail-before-pass-after pin: two `:deps` entries naming the same
        // caixa carry two `:versao` / `:fonte` / feature triples that the
        // caixa-resolver's lacre pipeline collapses (the second silently
        // overwrites the first at `concrete_versao`-resolve time). The
        // gate surfaces the duplicate at validate-time, naming the
        // offending caixa + the list, before the resolver-side silent
        // drop. Mirrors the peer typed-graph duplicate gates
        // (`DuplicateChildCaixa`, `MembroDuplicate`, `DuplicateFrom`, …).
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps = vec![
            Dep::simple("caixa-teia", "^0.1"),
            Dep::simple("caixa-teia", "^0.2"),
        ];
        let err = c.validate_deps().unwrap_err();
        assert!(
            matches!(
                err,
                crate::dep::DepError::DuplicateNome { ref nome, list }
                    if nome == "caixa-teia" && list == ":deps"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_deps_rejects_duplicate_nome_in_deps_dev() {
        // Parity pin: `:deps-dev` runs through the same per-list
        // duplicate check as `:deps` — neither axis is a second-class
        // citizen of the set-not-multiset discipline.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps_dev = vec![
            Dep::simple("tatara-check", "*"),
            Dep::simple("tatara-check", "^0.1"),
        ];
        let err = c.validate_deps().unwrap_err();
        assert!(
            matches!(
                err,
                crate::dep::DepError::DuplicateNome { ref nome, list }
                    if nome == "tatara-check" && list == ":deps-dev"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_deps_accepts_cross_list_same_nome() {
        // The Cargo `[dependencies]` + `[dev-dependencies]` override
        // convention is preserved: a name appearing in *both* lists is
        // valid (the dev-pin overrides at test/dev time). Only
        // within-list duplicates are structurally incoherent — pin the
        // permissive cross-list semantics so a future shortcut that
        // collapses the two seen-sets into one surfaces here as a test
        // failure.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps = vec![Dep::simple("caixa-teia", "^0.1")];
        c.deps_dev = vec![Dep::simple("caixa-teia", "^0.2")];
        c.validate_deps().unwrap();
    }

    #[test]
    fn validate_deps_accepts_distinct_nome_in_both_lists() {
        // Positive control: distinct names within each list pass — the
        // gate's identity element on the canonical authoring shape.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps = vec![
            Dep::simple("caixa-teia", "^0.1"),
            Dep::simple("pleme-mesh", "*"),
        ];
        c.deps_dev = vec![
            Dep::simple("tatara-check", "*"),
            Dep::simple("dev-shim", "^0.1"),
        ];
        c.validate_deps().unwrap();
    }

    #[test]
    fn validate_deps_per_entry_validate_fires_before_duplicate_in_deps() {
        // Diagnostic-precedence pin: a malformed `:versao` on the
        // duplicating entry surfaces its narrower `VersaoInvalid`
        // diagnostic first, before the cross-entry duplicate gate fires
        // — the canonical "per-entry shape before cross-entry uniqueness"
        // precedence every peer set-not-multiset gate establishes
        // (`*_invalid_fires_before_duplicate_check` pins on
        // `SupervisorSpec::validate`, `AplicacaoSpec::validate_membros`,
        // `validate_upgrade_from`).
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps = vec![
            Dep::simple("caixa-teia", "^0.1"),
            Dep::simple("caixa-teia", "^bad-version"),
        ];
        let err = c.validate_deps().unwrap_err();
        assert!(
            matches!(
                err,
                crate::dep::DepError::VersaoInvalid { ref nome, ref versao, .. }
                    if nome == "caixa-teia" && versao == "^bad-version"
            ),
            "expected VersaoInvalid to surface before DuplicateNome, got {err:?}"
        );
    }

    #[test]
    fn validate_deps_duplicate_diagnostic_names_first_collision() {
        // First-collision determinism pin: with three entries naming the
        // same caixa, the first colliding pair surfaces — not the last.
        // Mirrors the peer first-collision posture on every
        // duplicate-target gate
        // (`validate_upgrade_from_duplicate_diagnostic_names_second_collision`
        // — the second entry is the first collision; this gate uses the
        // same shape: the second entry's `:nome` lands in the diagnostic
        // because `seen.insert(first.nome)` already populated the set).
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps = vec![
            Dep::simple("caixa-teia", "^0.1"),
            Dep::simple("caixa-teia", "^0.2"),
            Dep::simple("caixa-teia", "^0.3"),
        ];
        let err = c.validate_deps().unwrap_err();
        // The diagnostic carries the offending caixa name; the
        // implementation surfaces on the *second* entry (the first
        // collision), so the test pins the `:nome` value.
        assert!(
            matches!(
                err,
                crate::dep::DepError::DuplicateNome { ref nome, list: ":deps" }
                    if nome == "caixa-teia"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_deps_duplicate_in_deps_fires_before_duplicate_in_deps_dev() {
        // Cross-list precedence pin: when both lists carry duplicates,
        // the `:deps` diagnostic surfaces first — same author-mental-
        // model ordering the `validate_deps_runs_deps_before_deps_dev`
        // pin establishes for malformed `:versao` (runtime axis before
        // dev axis).
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps = vec![
            Dep::simple("runtime-dep", "^0.1"),
            Dep::simple("runtime-dep", "^0.2"),
        ];
        c.deps_dev = vec![Dep::simple("dev-dep", "*"), Dep::simple("dev-dep", "^0.1")];
        let err = c.validate_deps().unwrap_err();
        assert!(
            matches!(
                err,
                crate::dep::DepError::DuplicateNome { ref nome, list: ":deps" }
                    if nome == "runtime-dep"
            ),
            "expected :deps duplicate to surface before :deps-dev duplicate, got {err:?}"
        );
    }

    #[test]
    fn validate_deps_empty_lists_pass_duplicate_gate() {
        // Empty-set identity pin: the bare template (zero deps, zero
        // deps_dev) passes the duplicate gate as the gate's identity
        // element. A future tighten that conflates "empty" with
        // "missing" would regress this baseline.
        let c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.validate_deps().unwrap();
    }

    #[test]
    fn validate_deps_duplicate_diagnostic_carries_list_tag() {
        // Diagnostic-shape pin: the `list:` field tags which list the
        // duplicate landed in (`:deps` vs `:deps-dev`) verbatim, so a
        // `feira lint` run can route the author to the right block in
        // their caixa.lisp without re-deriving the list from context.
        // Same self-locating shape every peer per-axis diagnostic
        // already exposes.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps_dev = vec![
            Dep::simple("dev-thing", "*"),
            Dep::simple("dev-thing", "^0.1"),
        ];
        let err = c.validate_deps().unwrap_err();
        let crate::dep::DepError::DuplicateNome { nome, list } = err else {
            panic!("expected DuplicateNome from :deps-dev walk");
        };
        assert_eq!(nome, "dev-thing");
        assert_eq!(list, ":deps-dev");
    }

    // ── validate_deps: per-entry :caracteristicas set-discipline gate ──

    #[test]
    fn validate_deps_surfaces_caracteristicas_duplicate_in_deps_list() {
        // Thread-through pin on `:deps`: the per-entry
        // `Dep::validate_caracteristicas` gate fires inside
        // `Caixa::validate_deps`'s linear walk, so a malformed feature
        // list on any `:deps` entry surfaces as a `DepError` from
        // `validate_deps` — the same reachability shape every per-entry
        // `Dep::validate` arm threads through. Without this pin a future
        // shortcut that skips the per-entry `Dep::validate` call on the
        // cross-entry-uniqueness path would mask the within-entry
        // `:caracteristicas` gates.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps = vec![Dep {
            nome: "caixa-teia".into(),
            versao: "^0.1".into(),
            fonte: None,
            opcional: false,
            caracteristicas: vec!["http".into(), "http".into()],
        }];
        let err = c.validate_deps().unwrap_err();
        let crate::dep::DepError::CaracteristicaDuplicate {
            nome,
            caracteristica,
        } = err
        else {
            panic!("expected CaracteristicaDuplicate from :deps walk, got {err:?}");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(caracteristica, "http");
    }

    #[test]
    fn validate_deps_surfaces_caracteristicas_empty_in_deps_dev_list() {
        // Peer thread-through pin on `:deps-dev`: same reachability as
        // the `:deps` arm above, on the dev-only authoring axis. Pins
        // that the `validate_deps` walk visits both lists' per-entry
        // gates uniformly. The empty-feature arm carries here so both
        // new `:caracteristicas` arms are surfaced via at least one
        // `validate_deps` thread-through.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps_dev = vec![Dep {
            nome: "caixa-teia".into(),
            versao: "^0.1".into(),
            fonte: None,
            opcional: false,
            caracteristicas: vec![String::new()],
        }];
        let err = c.validate_deps().unwrap_err();
        let crate::dep::DepError::CaracteristicaEmpty { nome } = err else {
            panic!("expected CaracteristicaEmpty from :deps-dev walk, got {err:?}");
        };
        assert_eq!(nome, "caixa-teia");
    }

    #[test]
    fn validate_deps_surfaces_caracteristicas_invalid_in_deps_list() {
        // Thread-through pin on `:deps`: the per-entry
        // `Dep::validate_caracteristicas` value-shape gate (lifted via
        // `crate::render::is_cargo_feature_name`) fires inside
        // `Caixa::validate_deps`'s linear walk on the `:deps` list, so
        // a structurally invalid feature name on any `:deps` entry
        // surfaces as `DepError::CaracteristicaInvalid` from
        // `validate_deps` — the same reachability shape every per-entry
        // `Dep::validate` arm threads through. Without this pin a
        // future shortcut that skips the per-entry `Dep::validate` call
        // on the cross-entry-uniqueness path would mask the within-
        // entry `:caracteristicas` value-shape gate.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps = vec![Dep {
            nome: "caixa-teia".into(),
            versao: "^0.1".into(),
            fonte: None,
            opcional: false,
            caracteristicas: vec!["+http".into()],
        }];
        let err = c.validate_deps().unwrap_err();
        let crate::dep::DepError::CaracteristicaInvalid {
            nome,
            caracteristica,
            ..
        } = err
        else {
            panic!("expected CaracteristicaInvalid from :deps walk, got {err:?}");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(caracteristica, "+http");
    }

    #[test]
    fn validate_deps_surfaces_caracteristicas_invalid_in_deps_dev_list() {
        // Peer thread-through pin on `:deps-dev`: same reachability as
        // the `:deps` arm above, on the dev-only authoring axis. The
        // `http/json` shape carries here so the segment-separator
        // diagnostic (the canonical Cargo `dep/feat` namespaced-dep
        // confusion footgun) is surfaced via the cross-entry walk too —
        // pinning that the `:deps-dev` list visits the same per-entry
        // value-shape gate as the `:deps` list.
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps_dev = vec![Dep {
            nome: "caixa-teia".into(),
            versao: "^0.1".into(),
            fonte: None,
            opcional: false,
            caracteristicas: vec!["http/json".into()],
        }];
        let err = c.validate_deps().unwrap_err();
        let crate::dep::DepError::CaracteristicaInvalid {
            nome,
            caracteristica,
            ..
        } = err
        else {
            panic!("expected CaracteristicaInvalid from :deps-dev walk, got {err:?}");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(caracteristica, "http/json");
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

    // ── Caixa::validate_nome — top-level :nome value-shape gate ─────────

    fn caixa_with_nome(nome: &str) -> Caixa {
        let mut c = Caixa::from_lisp(&Caixa::template("placeholder")).unwrap();
        c.nome = nome.to_string();
        c
    }

    #[test]
    fn validate_nome_accepts_canonical_template() {
        // Positive control: the bare `feira init`-style template's
        // `:nome` ("demo") is a canonical DNS-1123 label; the gate must
        // not regress this baseline shape. A future tightening of the
        // accepted set surfaces here as a test failure first.
        let c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.validate_nome().unwrap();
    }

    #[test]
    fn validate_nome_accepts_canonical_forms() {
        // Positive-set sweep: each realistic caixa-name shape the K8s
        // apiserver accepts as a `metadata.name` label must pass —
        // single-word, hyphen-joined, version-suffixed, single-char,
        // two-char, digit-start (DNS-1123 allows this; the stricter
        // DNS-1035 Service-name rule doesn't), version-suffix-bearing.
        // Mirrors `accepts_canonical_membro_caixa_forms` (3f9d7a0) on
        // the peer member-name axis.
        for nome in [
            "checkout",
            "cart-v2",
            "a",
            "db",
            "3rd-party-shim",
            "payment-retry",
            "0",
        ] {
            caixa_with_nome(nome)
                .validate_nome()
                .unwrap_or_else(|e| panic!("canonical :nome {nome:?} must validate, got {e:?}"));
        }
    }

    #[test]
    fn validate_nome_rejects_empty() {
        // Fail-before-pass-after pin: `Caixa::from_lisp` does not refuse
        // an empty `:nome` (the derive macro stores the raw String);
        // the gate's empty arm names the offending axis with a narrower
        // diagnostic than the `NomeInvalid` parse arm would emit.
        let c = caixa_with_nome("");
        let err = c.validate_nome().unwrap_err();
        assert_eq!(err, ManifestError::NomeEmpty);
    }

    #[test]
    fn validate_nome_rejects_uppercase() {
        // The canonical "I copied the TitleCase display name verbatim"
        // footgun. The K8s apiserver rejects `metadata.name: MyApp` at
        // admission on every derived artifact (Helm chart, ComputeUnit,
        // CNP, HTTPRoute, label values); the gate moves the diagnostic
        // to the source `caixa.lisp` and the reason suggests the
        // lowercased fix verbatim.
        let c = caixa_with_nome("MyApp");
        let err = c.validate_nome().unwrap_err();
        let ManifestError::NomeInvalid { nome, reason } = err else {
            panic!("expected NomeInvalid for uppercase :nome");
        };
        assert_eq!(nome, "MyApp");
        assert!(
            reason.contains("uppercase") && reason.contains("myapp"),
            "diagnostic must name the violation + the lowercased fix, got {reason:?}"
        );
    }

    #[test]
    fn validate_nome_rejects_underscore() {
        // The Python-/Postgres-style `snake_case` leak. DNS-1123 forbids
        // `_`; the apiserver rejects on admission across every derived
        // artifact. Same fixture pinned for `:membros :caixa` (3f9d7a0)
        // and `:children :caixa` (31bfa43).
        let c = caixa_with_nome("my_app");
        let err = c.validate_nome().unwrap_err();
        assert!(
            matches!(
                err,
                ManifestError::NomeInvalid { ref nome, ref reason }
                    if nome == "my_app" && reason.contains('_')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_nome_rejects_dot() {
        // A `:nome` is a single DNS-1123 label, not a subdomain. The
        // "I want to namespace with `.`" footgun the gate redirects to
        // `-` via the shared predicate's reason wording.
        let c = caixa_with_nome("team.app");
        let err = c.validate_nome().unwrap_err();
        assert!(
            matches!(
                err,
                ManifestError::NomeInvalid { ref nome, ref reason }
                    if nome == "team.app" && reason.contains('.')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_nome_rejects_leading_hyphen() {
        // DNS-1123 boundary rule: the label must start with an ASCII
        // alphanumeric. Pin the leading-`-` arm explicitly.
        let c = caixa_with_nome("-app");
        let err = c.validate_nome().unwrap_err();
        assert!(
            matches!(
                err,
                ManifestError::NomeInvalid { ref nome, .. } if nome == "-app"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_nome_rejects_trailing_hyphen() {
        // Symmetric arm of the boundary rule, pinned separately so a
        // future relaxation that only checks the leading position
        // surfaces here. Mirrors `rejects_membro_caixa_with_trailing_hyphen`
        // and `_with_trailing_hyphen` on the supervisor / aplicacao
        // axes.
        let c = caixa_with_nome("app-");
        let err = c.validate_nome().unwrap_err();
        assert!(
            matches!(
                err,
                ManifestError::NomeInvalid { ref nome, .. } if nome == "app-"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_nome_rejects_unicode() {
        // IDN must be pre-encoded as Punycode (`xn--…`); raw Unicode
        // bytes are rejected by the K8s apiserver on every name axis.
        let c = caixa_with_nome("café");
        let err = c.validate_nome().unwrap_err();
        assert!(
            matches!(
                err,
                ManifestError::NomeInvalid { ref nome, .. } if nome == "café"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_nome_rejects_whitespace() {
        // The paste-from-sketch / paste-from-spec footgun. Internal
        // whitespace is rejected by every K8s name axis.
        let c = caixa_with_nome("my app");
        let err = c.validate_nome().unwrap_err();
        assert!(
            matches!(
                err,
                ManifestError::NomeInvalid { ref nome, .. } if nome == "my app"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_nome_rejects_too_long() {
        // 64-byte boundary pin: the K8s apiserver rejects any
        // `metadata.name` over 63 bytes at admission; the diagnostic
        // names both the 63-byte cap and the actual length so the
        // author can shorten in one edit. Mirrors `_too_long` on the
        // peer member-/cluster-/child-name axes.
        let over = "a".repeat(crate::DNS_1123_LABEL_MAX_LEN + 1);
        let c = caixa_with_nome(&over);
        let err = c.validate_nome().unwrap_err();
        let ManifestError::NomeInvalid { nome, reason } = err else {
            panic!("expected NomeInvalid for over-cap :nome");
        };
        assert_eq!(nome.len(), crate::DNS_1123_LABEL_MAX_LEN + 1);
        assert!(
            reason.contains("63") && reason.contains("64"),
            "diagnostic must name the cap + actual length, got {reason:?}"
        );
    }

    #[test]
    fn nome_max_length_validates() {
        // The 63-byte cap exactly — the boundary-accepting case pinned
        // alongside `validate_nome_rejects_too_long` so a future cap
        // shift surfaces both arms simultaneously. Mirrors
        // `membro_caixa_max_length_validates`,
        // `placement_cluster_max_length_validates`,
        // `child_caixa_max_length_validates`.
        let at_cap = "a".repeat(crate::DNS_1123_LABEL_MAX_LEN);
        caixa_with_nome(&at_cap).validate_nome().unwrap();
    }

    #[test]
    fn nome_empty_takes_precedence_over_invalid() {
        // Order pin: the empty arm fires before the predicate is
        // consulted. Empty < invalid in self-locating-ness — the
        // narrower `NomeEmpty` diagnostic doesn't carry a useless
        // `nome: ""` reference into the parser-shaped reason. Mirrors
        // `membro_caixa_empty_takes_precedence_over_invalid` on the
        // peer axis (3f9d7a0).
        let c = caixa_with_nome("");
        assert_eq!(c.validate_nome().unwrap_err(), ManifestError::NomeEmpty);
    }

    #[test]
    fn nome_invalid_diagnostic_carries_offending_nome() {
        // Diagnostic-shape pin: the error names the offending `:nome`
        // verbatim with a non-empty parser-shaped reason, so a `feira
        // lint` run can render the diagnostic without re-parsing.
        // Mirrors `membro_caixa_invalid_diagnostic_carries_offending_caixa`.
        let c = caixa_with_nome("MyApp");
        let err = c.validate_nome().unwrap_err();
        let ManifestError::NomeInvalid { nome, reason } = err else {
            panic!("expected NomeInvalid variant");
        };
        assert_eq!(nome, "MyApp");
        assert!(
            !reason.is_empty(),
            "NomeInvalid `reason` must carry the predicate's wording verbatim"
        );
    }

    // ── Caixa::validate_versao — top-level :versao value-shape gate ─────

    fn caixa_with_versao(versao: &str) -> Caixa {
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.versao = versao.to_string();
        c
    }

    #[test]
    fn validate_versao_accepts_canonical_template() {
        // Positive control: the bare `feira init`-style template's
        // `:versao` ("0.1.0") is a canonical SemVer-2 literal; the gate
        // must not regress this baseline shape. A future tightening of
        // the accepted set surfaces here as a test failure first.
        let c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.validate_versao().unwrap();
    }

    #[test]
    fn validate_versao_accepts_canonical_forms() {
        // Positive-set sweep: each realistic SemVer-2 shape the
        // substrate's downstream consumers accept must pass — bare
        // MAJOR.MINOR.PATCH, pre-release tags (`-rc.1`, `-alpha.0`),
        // build metadata (`+build.42`), the combined form, and the
        // `0.0.0` boundary case. Mirrors `accepts_canonical_forms` on
        // the peer `:nome` axis (6c992f8).
        for versao in [
            "0.1.0",
            "0.0.0",
            "1.0.0",
            "0.2.0-rc.1",
            "1.0.0-alpha.0",
            "1.0.0+build.42",
            "1.0.0-rc.1+build.42",
            "10.20.30",
        ] {
            caixa_with_versao(versao)
                .validate_versao()
                .unwrap_or_else(|e| {
                    panic!("canonical :versao {versao:?} must validate, got {e:?}")
                });
        }
    }

    #[test]
    fn validate_versao_rejects_empty() {
        // Fail-before-pass-after pin: `Caixa::from_lisp` does not refuse
        // an empty `:versao` (the derive macro stores the raw String);
        // the gate's empty arm names the offending axis with a narrower
        // diagnostic than the `VersaoInvalid` parse arm would emit.
        // Mirrors `validate_nome_rejects_empty` (6c992f8).
        let c = caixa_with_versao("");
        let err = c.validate_versao().unwrap_err();
        assert_eq!(err, ManifestError::VersaoEmpty);
    }

    #[test]
    fn validate_versao_rejects_git_tag_shape() {
        // The canonical "I copied the git tag verbatim" footgun —
        // `feira publish` *emits* `v<versao>` git tags, so a leaked
        // `v0.1.0` in `:versao` would render as `vv0.1.0` and silently
        // shift every downstream consumer's version axis. `semver`
        // rejects the leading `v` at parse time; the gate moves the
        // diagnostic to the source `caixa.lisp`.
        let c = caixa_with_versao("v0.1.0");
        let err = c.validate_versao().unwrap_err();
        let ManifestError::VersaoInvalid { versao, reason } = err else {
            panic!("expected VersaoInvalid for git-tag-shape :versao");
        };
        assert_eq!(versao, "v0.1.0");
        assert!(
            !reason.is_empty(),
            "VersaoInvalid `reason` must carry the parser's wording, got {reason:?}"
        );
    }

    #[test]
    fn validate_versao_rejects_missing_patch() {
        // The canonical "I shortened it" footgun — SemVer-2 requires
        // three parts. Cargo's `version =` field accepts the shortened
        // form as a requirement, conflating the two leaks across the
        // typed `:deps :versao` vs top-level `:versao` axes; the gate
        // pins the top-level axis to the strict three-part shape.
        let c = caixa_with_versao("0.1");
        let err = c.validate_versao().unwrap_err();
        assert!(
            matches!(
                err,
                ManifestError::VersaoInvalid { ref versao, .. } if versao == "0.1"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_versao_rejects_requirement_shape() {
        // The canonical "I leaked a requirement into a version" footgun —
        // the typed `:deps :versao` / `:membros :versao` axes accept
        // `^0.1` (a `VersionReq`); the top-level `:versao` requires a
        // concrete `Version`. Without this gate the two typed surfaces
        // would silently overlap, and a top-level `^0.1` would surface
        // at `helm install` time as a Chart.yaml version rejection far
        // from the source `caixa.lisp`.
        let c = caixa_with_versao("^0.1");
        let err = c.validate_versao().unwrap_err();
        assert!(
            matches!(
                err,
                ManifestError::VersaoInvalid { ref versao, .. } if versao == "^0.1"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_versao_rejects_docker_tag_shape() {
        // The "I confused it with a docker tag" footgun — `latest`,
        // `main`, `stable` parse as identifiers, not SemVer-2 versions.
        // SemVer rejects at parse time; the gate moves the diagnostic
        // to the source `caixa.lisp`.
        for bad in ["latest", "main", "stable"] {
            let c = caixa_with_versao(bad);
            let err = c.validate_versao().unwrap_err();
            assert!(
                matches!(
                    err,
                    ManifestError::VersaoInvalid { ref versao, .. } if versao == bad
                ),
                "got {err:?} for {bad:?}"
            );
        }
    }

    #[test]
    fn validate_versao_rejects_four_part_form() {
        // The Java/Microsoft "MAJOR.MINOR.PATCH.BUILD" convention
        // SemVer-2 forbids. A leak from a non-SemVer ecosystem; the
        // semver crate rejects the extra `.0` at parse time.
        let c = caixa_with_versao("0.1.0.0");
        let err = c.validate_versao().unwrap_err();
        assert!(
            matches!(
                err,
                ManifestError::VersaoInvalid { ref versao, .. } if versao == "0.1.0.0"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn versao_empty_takes_precedence_over_invalid() {
        // Order pin: the empty arm fires before the parser is consulted.
        // Empty < invalid in self-locating-ness — the narrower
        // `VersaoEmpty` diagnostic doesn't carry a useless `versao: ""`
        // reference into the parser-shaped reason. Mirrors
        // `nome_empty_takes_precedence_over_invalid` (6c992f8) on the
        // peer axis.
        let c = caixa_with_versao("");
        assert_eq!(c.validate_versao().unwrap_err(), ManifestError::VersaoEmpty);
    }

    #[test]
    fn versao_invalid_diagnostic_carries_offending_versao() {
        // Diagnostic-shape pin: the error names the offending `:versao`
        // verbatim with a non-empty parser-shaped reason, so a `feira
        // lint` run can render the diagnostic without re-parsing.
        // Mirrors `nome_invalid_diagnostic_carries_offending_nome`.
        let c = caixa_with_versao("v0.1.0");
        let err = c.validate_versao().unwrap_err();
        let ManifestError::VersaoInvalid { versao, reason } = err else {
            panic!("expected VersaoInvalid variant");
        };
        assert_eq!(versao, "v0.1.0");
        assert!(
            !reason.is_empty(),
            "VersaoInvalid `reason` must carry the parser's wording verbatim"
        );
    }

    #[test]
    fn validate_versao_accepts_what_upgrade_from_from_accepts() {
        // Parity pin: every shape `UpgradeFromEntry::validate` accepts
        // for `:upgrade-from :from` must also pass `validate_versao` —
        // the two `:versao`-typed surfaces (top-level `:versao`,
        // `:upgrade-from :from`) consume the *same* `semver::Version`
        // parser, so they must agree on the accepted set. Without this
        // pin, a future tightening of one axis could silently diverge
        // from the other. Mirrors the `:versao` requirement-axis
        // parity (`:deps`/`:deps-dev`/`:membros`/`:children`) the prior
        // commits established.
        for versao in ["0.1.0", "0.2.0-rc.1", "1.0.0+build.42"] {
            // From the canonical UpgradeFromEntry round-trip fixture
            // (`upgrade::tests::round_trip_load_module` peers).
            let entry = crate::UpgradeFromEntry {
                from: versao.to_string(),
                instructions: Vec::new(),
            };
            entry
                .validate()
                .unwrap_or_else(|e| panic!(":from {versao:?} must validate, got {e:?}"));
            caixa_with_versao(versao)
                .validate_versao()
                .unwrap_or_else(|e| {
                    panic!(":versao {versao:?} must validate, got {e:?} — peer axis diverges")
                });
        }
    }

    // ── Caixa::validate_restart_window — supervisor restart-window
    //    folds through the shared `supervisor::duration_codec` ────────

    fn caixa_with_restart_window(window: Option<&str>) -> Caixa {
        let mut c = Caixa::from_lisp(&Caixa::template("root")).unwrap();
        c.kind = CaixaKind::Supervisor;
        c.restart_window = window.map(str::to_string);
        c
    }

    #[test]
    fn validate_restart_window_accepts_none() {
        // The canonical "omit the slot to express no reset" shape — a
        // `None` raw string is the absence of the typed
        // `:restart-window` slot, which is exactly the SupervisorSpec
        // "never reset" semantics. The gate must be a no-op here; a
        // future tightening that rejected `None` would force every
        // supervisor caixa to authoring-time pin a window even when
        // the OTP semantics call for none.
        caixa_with_restart_window(None)
            .validate_restart_window()
            .unwrap();
    }

    #[test]
    fn validate_restart_window_accepts_canonical_forms() {
        // Positive-set sweep across the canonical authoring units the
        // shared `supervisor::duration_codec::parse` accepts —
        // matches the codec-side `parse_accepts_integer_canonical_units`
        // pin in supervisor::tests so a future codec-side tightening
        // surfaces simultaneously on both axes.
        for window in ["60s", "5m", "1h", "500ms", "30", "0s"] {
            caixa_with_restart_window(Some(window))
                .validate_restart_window()
                .unwrap_or_else(|e| {
                    panic!("canonical :restart-window {window:?} must validate, got {e:?}")
                });
        }
    }

    #[test]
    fn validate_restart_window_rejects_fractional_seconds() {
        // Fail-before-pass-after pin: the `"1.5s"` drift class (parses
        // as f64 to 1.5 → renders back as `"1500ms"` on first
        // serialize). Prior to the fold + this gate, the inline
        // `parse_window_inline` accepted f64 magnitudes and silently
        // produced a `Duration::from_secs_f64(1.5)`, divergent from
        // the shared codec's integer-magnitude discipline on the
        // serde-routed siblings. The gate now surfaces a self-locating
        // diagnostic at the manifest layer.
        let err = caixa_with_restart_window(Some("1.5s"))
            .validate_restart_window()
            .unwrap_err();
        let ManifestError::RestartWindowMalformed {
            restart_window,
            reason,
        } = err
        else {
            panic!("expected RestartWindowMalformed for fractional seconds");
        };
        assert_eq!(restart_window, "1.5s");
        assert!(
            reason.contains("\"1.5\"") && reason.contains("not a non-negative integer"),
            "diagnostic must carry shared-codec wording, got {reason:?}"
        );
    }

    #[test]
    fn validate_restart_window_rejects_decimal_shaped_integer() {
        // The `"1.0s"` class — numerically `1s` exactly, but the
        // canonical form is `"1s"` not `"1.0s"`. Decimal-shape leak
        // gets the same canonical-form diagnostic.
        let err = caixa_with_restart_window(Some("1.0s"))
            .validate_restart_window()
            .unwrap_err();
        assert!(
            matches!(
                err,
                ManifestError::RestartWindowMalformed { ref restart_window, .. }
                    if restart_window == "1.0s"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_restart_window_rejects_half_unit_minute() {
        // `"0.5m"` is the unit-fraction footgun — author writes a
        // human-readable half-minute, the prior inline parser silently
        // produced `Duration::from_secs_f64(30.0)` and serde
        // re-emitted as `"30s"`, rewriting author intent. The gate
        // closes the loop at the manifest layer.
        let err = caixa_with_restart_window(Some("0.5m"))
            .validate_restart_window()
            .unwrap_err();
        let ManifestError::RestartWindowMalformed {
            restart_window,
            reason,
        } = err
        else {
            panic!("expected RestartWindowMalformed");
        };
        assert_eq!(restart_window, "0.5m");
        assert!(
            reason.contains("\"30s\""),
            "diagnostic must point at the canonical-form remediation, got {reason:?}"
        );
    }

    #[test]
    fn validate_restart_window_rejects_leading_sign() {
        // `"+30s"` and `"-30s"` both round-tripped through f64 cleanly
        // on the prior parser (`+30` parses as `30.0`; `-30` parsed
        // and was caught by the `num < 0.0` arm which silently
        // returned `None`, dropping the author-supplied window). The
        // shared codec's digit-only gate rejects both with a unified
        // canonical-form diagnostic; the manifest-layer wrapper names
        // the offending value.
        for bad in ["+30s", "-30s"] {
            let err = caixa_with_restart_window(Some(bad))
                .validate_restart_window()
                .unwrap_err();
            assert!(
                matches!(
                    err,
                    ManifestError::RestartWindowMalformed { ref restart_window, .. }
                        if restart_window == bad
                ),
                "got {err:?} for {bad:?}"
            );
        }
    }

    #[test]
    fn validate_restart_window_rejects_unknown_unit() {
        // `"30x"` — the typo / wrong-unit footgun. The shared codec's
        // unit dispatch surfaces an `unknown duration unit` reason;
        // the manifest-layer wrapper names the offending value.
        let err = caixa_with_restart_window(Some("30x"))
            .validate_restart_window()
            .unwrap_err();
        let ManifestError::RestartWindowMalformed {
            restart_window,
            reason,
        } = err
        else {
            panic!("expected RestartWindowMalformed for unknown unit");
        };
        assert_eq!(restart_window, "30x");
        assert!(
            reason.contains("unknown duration unit"),
            "diagnostic must carry shared-codec unit-rejection wording, got {reason:?}"
        );
    }

    #[test]
    fn validate_restart_window_rejects_garbage() {
        // Pure non-numeric magnitude (`"abc"`) falls through to the
        // shared codec's narrower `"bad duration magnitude"` arm. Same
        // diagnostic shape as the codec-side
        // `parse_garbage_still_falls_through_to_bad_magnitude` pin.
        let err = caixa_with_restart_window(Some("abc"))
            .validate_restart_window()
            .unwrap_err();
        let ManifestError::RestartWindowMalformed {
            restart_window,
            reason,
        } = err
        else {
            panic!("expected RestartWindowMalformed for garbage");
        };
        assert_eq!(restart_window, "abc");
        assert!(
            reason.contains("bad duration magnitude"),
            "diagnostic must carry shared-codec garbage-rejection wording, got {reason:?}"
        );
    }

    #[test]
    fn validate_restart_window_rejects_empty_string() {
        // The empty-after-trim edge case — distinct from the `None`
        // canonical "omit the slot" shape. The shared codec's
        // digit-only gate refuses an empty magnitude; the manifest
        // layer names the offending `""` so the author can grep for
        // the literal empty value in their `caixa.lisp` and either
        // remove the slot (the canonical "no reset" shape) or pin a
        // positive duration.
        let err = caixa_with_restart_window(Some(""))
            .validate_restart_window()
            .unwrap_err();
        assert!(
            matches!(
                err,
                ManifestError::RestartWindowMalformed { ref restart_window, .. }
                    if restart_window.is_empty()
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_restart_window_diagnostic_carries_offending_value() {
        // Diagnostic-shape pin (peer with
        // `nome_invalid_diagnostic_carries_offending_nome` /
        // `versao_invalid_diagnostic_carries_offending_versao`): the
        // error names the offending raw `:restart-window` verbatim
        // with a non-empty shared-codec-shaped reason, so a `feira
        // lint` run can render the diagnostic without re-parsing.
        let err = caixa_with_restart_window(Some("1.5s"))
            .validate_restart_window()
            .unwrap_err();
        let ManifestError::RestartWindowMalformed {
            restart_window,
            reason,
        } = err
        else {
            panic!("expected RestartWindowMalformed variant");
        };
        assert_eq!(restart_window, "1.5s");
        assert!(
            !reason.is_empty(),
            "RestartWindowMalformed `reason` must carry the codec's wording verbatim"
        );
    }

    #[test]
    fn supervisor_view_folds_through_shared_codec_on_canonical_form() {
        // Behavioral parity pin after the fold (`parse_window_inline`
        // deletion): the canonical `"60s"` still produces
        // `Duration::from_secs(60)` on the typed view — the fold is
        // semantically equivalent to the prior inline parser on the
        // accepted set. Mirrors the pre-fold `supervisor_view_returns_typed_shape`
        // pin, narrowed to the parser-side contract.
        let c = caixa_with_restart_window(Some("60s"));
        let view = c.supervisor_view().expect("Supervisor kind has a view");
        assert_eq!(
            view.restart_window,
            Some(std::time::Duration::from_secs(60))
        );
    }

    #[test]
    fn supervisor_view_soft_swallows_what_validate_rejects() {
        // Parity pin between the view-construction path and the
        // manifest-level validator: the same `"1.5s"` that surfaces
        // `RestartWindowMalformed` at `validate_restart_window` time
        // becomes `restart_window: None` on the typed view (the fold
        // preserves the existing best-effort shape of `supervisor_view`).
        // The contract is: a layout-verifier / `feira lint` flow that
        // cares about the malformed-window axis MUST consult
        // `validate_restart_window` — relying solely on the view's
        // `None` swallows the diagnostic silently. This pin makes the
        // expectation a typed invariant.
        let c = caixa_with_restart_window(Some("1.5s"));
        let view = c.supervisor_view().expect("Supervisor kind has a view");
        assert_eq!(
            view.restart_window, None,
            "view-construction path soft-swallows the parse error to None"
        );
        // And the manifest-level validator does NOT soft-swallow:
        assert!(
            matches!(
                c.validate_restart_window().unwrap_err(),
                ManifestError::RestartWindowMalformed { ref restart_window, .. }
                    if restart_window == "1.5s"
            ),
            "validator must surface the offending value",
        );
    }

    // ── validate_code_paths — per-entry shape on :bibliotecas / :exe / :servicos ──

    fn caixa_with_code_paths(bibliotecas: Vec<&str>, exe: Vec<&str>, servicos: Vec<&str>) -> Caixa {
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.bibliotecas = bibliotecas.into_iter().map(String::from).collect();
        c.exe = exe.into_iter().map(String::from).collect();
        c.servicos = servicos.into_iter().map(String::from).collect();
        c
    }

    #[test]
    fn validate_code_paths_accepts_canonical_template() {
        // The bare `Caixa::template` shape is the gate's identity element
        // on the canonical authoring shape — `:bibliotecas
        // ("lib/demo.lisp")` + empty `:exe` + empty `:servicos`. Pins
        // that the gate is non-disruptive against every existing caixa.
        let c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.validate_code_paths().unwrap();
    }

    #[test]
    fn validate_code_paths_accepts_explicit_relative_paths_on_every_slot() {
        // Positive control sweep: a canonical-shaped path on every slot
        // passes. Mirrors the peer
        // `behavior::validate_every_slot_relative_is_ok` pin.
        let c = caixa_with_code_paths(
            vec!["lib/demo.lisp", "lib/helpers.lisp"],
            vec!["exe/demo", "exe/tool"],
            vec!["servicos/demo.computeunit.yaml"],
        );
        c.validate_code_paths().unwrap();
    }

    #[test]
    fn validate_code_paths_accepts_all_empty_lists() {
        // The empty-list identity element: every Caixa with no declared
        // code paths trivially passes (Supervisor / Aplicacao kinds rely
        // on this — the OwnCode gate already rejected them before the
        // path-shape gate runs in the layout, but the validator itself
        // must accept the empty shape).
        let c = caixa_with_code_paths(vec![], vec![], vec![]);
        c.validate_code_paths().unwrap();
    }

    #[test]
    fn validate_code_paths_rejects_empty_bibliotecas_entry() {
        let c = caixa_with_code_paths(vec![""], vec![], vec![]);
        let err = c.validate_code_paths().unwrap_err();
        assert!(
            matches!(
                err,
                ManifestError::CodePathEmpty {
                    slot: ":bibliotecas"
                }
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_code_paths_rejects_empty_exe_entry() {
        let c = caixa_with_code_paths(vec![], vec![""], vec![]);
        let err = c.validate_code_paths().unwrap_err();
        assert!(
            matches!(err, ManifestError::CodePathEmpty { slot: ":exe" }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_code_paths_rejects_empty_servicos_entry() {
        let c = caixa_with_code_paths(vec![], vec![], vec![""]);
        let err = c.validate_code_paths().unwrap_err();
        assert!(
            matches!(err, ManifestError::CodePathEmpty { slot: ":servicos" }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_code_paths_rejects_absolute_bibliotecas_entry() {
        // `:bibliotecas` has no `starts_with(<dir>)` fence downstream,
        // so an absolute path that resolves on disk silently passes the
        // layout's existence check — the canonical sandbox-escape on
        // the biblioteca axis.
        let c = caixa_with_code_paths(vec!["/etc/passwd"], vec![], vec![]);
        let err = c.validate_code_paths().unwrap_err();
        let ManifestError::CodePathAbsolute { slot, path } = err else {
            panic!("expected CodePathAbsolute, got {err:?}");
        };
        assert_eq!(slot, ":bibliotecas");
        assert_eq!(path, PathBuf::from("/etc/passwd"));
    }

    #[test]
    fn validate_code_paths_rejects_absolute_exe_entry() {
        let c = caixa_with_code_paths(vec![], vec!["/usr/bin/env"], vec![]);
        let err = c.validate_code_paths().unwrap_err();
        let ManifestError::CodePathAbsolute { slot, path } = err else {
            panic!("expected CodePathAbsolute, got {err:?}");
        };
        assert_eq!(slot, ":exe");
        assert_eq!(path, PathBuf::from("/usr/bin/env"));
    }

    #[test]
    fn validate_code_paths_rejects_absolute_servicos_entry() {
        let c = caixa_with_code_paths(vec![], vec![], vec!["/var/servicos/x.yaml"]);
        let err = c.validate_code_paths().unwrap_err();
        let ManifestError::CodePathAbsolute { slot, path } = err else {
            panic!("expected CodePathAbsolute, got {err:?}");
        };
        assert_eq!(slot, ":servicos");
        assert_eq!(path, PathBuf::from("/var/servicos/x.yaml"));
    }

    #[test]
    fn validate_code_paths_rejects_parent_escape_bibliotecas_leading() {
        // Canonical "I want a lib from a sibling caixa" footgun on the
        // biblioteca axis. `:bibliotecas` has no `starts_with` fence
        // downstream, so a leading `..` traverses to the parent of the
        // caixa root with no diagnostic at layout time if the resolved
        // target exists.
        let c = caixa_with_code_paths(vec!["../sibling/x.lisp"], vec![], vec![]);
        let err = c.validate_code_paths().unwrap_err();
        let ManifestError::CodePathParentEscape { slot, path } = err else {
            panic!("expected CodePathParentEscape, got {err:?}");
        };
        assert_eq!(slot, ":bibliotecas");
        assert_eq!(path, PathBuf::from("../sibling/x.lisp"));
    }

    #[test]
    fn validate_code_paths_rejects_parent_escape_exe_mid_path() {
        // Mid-path `..` defeats the layout's component-aware
        // `starts_with(exe_dir)` fence — `root.join("exe/../../escape")`
        // `starts_with(<root>/exe)` is true, but the canonical resolution
        // lives outside the caixa root. Caught regardless of where the
        // `..` sits — mirrors the peer
        // `behavior::validate_rejects_parent_escape_mid_path` pin.
        let c = caixa_with_code_paths(vec![], vec!["exe/../../escape"], vec![]);
        let err = c.validate_code_paths().unwrap_err();
        let ManifestError::CodePathParentEscape { slot, path } = err else {
            panic!("expected CodePathParentEscape, got {err:?}");
        };
        assert_eq!(slot, ":exe");
        assert_eq!(path, PathBuf::from("exe/../../escape"));
    }

    #[test]
    fn validate_code_paths_rejects_parent_escape_servicos_trailing() {
        let c = caixa_with_code_paths(vec![], vec![], vec!["servicos/foo/../../escape.yaml"]);
        let err = c.validate_code_paths().unwrap_err();
        let ManifestError::CodePathParentEscape { slot, path } = err else {
            panic!("expected CodePathParentEscape, got {err:?}");
        };
        assert_eq!(slot, ":servicos");
        assert_eq!(path, PathBuf::from("servicos/foo/../../escape.yaml"));
    }

    #[test]
    fn validate_code_paths_cross_slot_precedence_bibliotecas_before_exe_before_servicos() {
        // Cross-slot precedence pin: `:bibliotecas` → `:exe` →
        // `:servicos`. A manifest with malformed entries on all three
        // surfaces surfaces the `:bibliotecas` defect first, mirroring
        // the canonical declaration order
        // `Caixa::declared_foreign_code_slots` already establishes for
        // the foreign-code-slot diagnostic.
        let c = caixa_with_code_paths(vec![""], vec![""], vec![""]);
        let err = c.validate_code_paths().unwrap_err();
        assert!(
            matches!(
                err,
                ManifestError::CodePathEmpty {
                    slot: ":bibliotecas"
                }
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_code_paths_within_slot_precedence_empty_before_absolute_before_parent_escape() {
        // Within-slot precedence pin: empty → absolute → parent-escape,
        // matching the [`PathShapeViolation`] arm-ordering every peer
        // `is_sandboxed_relative_path` caller follows (b0c8389
        // BehaviorSpec, 26da2c7 UpgradeInstruction::StateChange). A
        // `:bibliotecas` list whose first entry is empty *and* whose
        // later entries are absolute/parent-escape surfaces the empty
        // arm first, on the lexicographically-earliest offending entry.
        let c = caixa_with_code_paths(vec!["", "/etc/passwd", "../escape.lisp"], vec![], vec![]);
        let err = c.validate_code_paths().unwrap_err();
        assert!(
            matches!(
                err,
                ManifestError::CodePathEmpty {
                    slot: ":bibliotecas"
                }
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_code_paths_first_offender_per_slot_wins() {
        // Within a single slot, the first declaration-order offender
        // surfaces — pins that the gate is left-to-right deterministic
        // (peer of every `*_first_collision_*` pin on duplicate gates).
        let c = caixa_with_code_paths(
            vec!["lib/ok.lisp", "/etc/escape", "../also-escape"],
            vec![],
            vec![],
        );
        let err = c.validate_code_paths().unwrap_err();
        let ManifestError::CodePathAbsolute { slot, path } = err else {
            panic!("expected CodePathAbsolute, got {err:?}");
        };
        assert_eq!(slot, ":bibliotecas");
        assert_eq!(path, PathBuf::from("/etc/escape"));
    }

    #[test]
    fn validate_code_paths_diagnostic_carries_offending_slot_and_path() {
        // Diagnostic-shape pin (peer with
        // `nome_invalid_diagnostic_carries_offending_nome` /
        // `versao_invalid_diagnostic_carries_offending_versao`): the
        // error's Display surfaces both the offending `:slot` tag and
        // the offending path verbatim, so a `feira lint` run can render
        // the diagnostic without re-parsing.
        let c = caixa_with_code_paths(vec!["/etc/passwd"], vec![], vec![]);
        let rendered = c.validate_code_paths().unwrap_err().to_string();
        assert!(
            rendered.contains(":bibliotecas"),
            "diagnostic must name the offending slot: {rendered}",
        );
        assert!(
            rendered.contains("/etc/passwd"),
            "diagnostic must quote the offending path: {rendered}",
        );
    }

    // ── validate_etiquetas — universal-axis registry-search-tag shape ──

    fn caixa_with_etiquetas(etiquetas: Vec<&str>) -> Caixa {
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.etiquetas = etiquetas.into_iter().map(String::from).collect();
        c
    }

    #[test]
    fn validate_etiquetas_accepts_empty_list() {
        // The empty-list identity: every caixa with no declared tags
        // trivially passes — `Caixa::template` emits `:etiquetas ()`,
        // so the gate is non-disruptive against every existing manifest.
        let c = caixa_with_etiquetas(vec![]);
        c.validate_etiquetas().unwrap();
    }

    #[test]
    fn validate_etiquetas_accepts_canonical_forms() {
        // Positive control sweep: a canonical-shaped non-empty distinct
        // tag list passes, mirroring the example checkout-aplicacao
        // (`:etiquetas ("example" "aplicacao" "mesh" "ecommerce" "demo")`)
        // and the hello-rio fixture (`("hello-world" "wasm" "rust")`).
        let c = caixa_with_etiquetas(vec!["example", "aplicacao", "mesh", "ecommerce", "demo"]);
        c.validate_etiquetas().unwrap();
    }

    #[test]
    fn validate_etiquetas_rejects_empty_entry() {
        // Canonical paste-from-blank-doc footgun. Without the gate the
        // empty entry rendered as `keywords: [""]` in `Chart.yaml`, a
        // no-op tag indexing nothing in the future caixa-registry.
        let c = caixa_with_etiquetas(vec![""]);
        let err = c.validate_etiquetas().unwrap_err();
        assert!(matches!(err, ManifestError::EtiquetaEmpty), "got {err:?}",);
    }

    #[test]
    fn validate_etiquetas_rejects_duplicate_entry() {
        // Canonical copy-paste-the-wrong-tag footgun. Without the gate
        // the duplicate was silently dedup'd by caixa-helm's BTreeSet
        // collect at chart render — a "second wins / one silently
        // disappears" shape divergent from every peer typed-graph set
        // gate. The duplicate-arm names the offending tag verbatim.
        let c = caixa_with_etiquetas(vec!["demo", "demo"]);
        let err = c.validate_etiquetas().unwrap_err();
        let ManifestError::EtiquetaDuplicate { etiqueta } = err else {
            panic!("expected EtiquetaDuplicate, got {err:?}");
        };
        assert_eq!(etiqueta, "demo");
    }

    #[test]
    fn validate_etiquetas_empty_takes_precedence_over_duplicate() {
        // Empty-first cascade pin: `("" "demo" "demo")` surfaces
        // `EtiquetaEmpty` not `EtiquetaDuplicate` — the narrower
        // structural "this entry has no value" defect dominates the
        // cross-entry uniqueness diagnostic. Mirrors the peer
        // empty-before-duplicate cascades on `:caracteristicas`
        // (`CaracteristicaEmpty` before `CaracteristicaDuplicate`,
        // fc3b4d5) and `:membros :caixa` (`MembroCaixaEmpty` before
        // `MembroDuplicate`).
        let c = caixa_with_etiquetas(vec!["", "demo", "demo"]);
        let err = c.validate_etiquetas().unwrap_err();
        assert!(matches!(err, ManifestError::EtiquetaEmpty), "got {err:?}",);
    }

    #[test]
    fn validate_etiquetas_duplicate_reports_first_collision() {
        // First-collision pin: `("a" "b" "a" "b")` surfaces the `"a"`
        // duplicate (the lexicographically-earliest offending position
        // — the second `"a"` at index 2 collides with the first `"a"`
        // at index 0), not the later `"b"` collision at index 3,
        // peer with every other first-collision diagnostic posture on
        // this surface (`validate_load_singularity_reports_first_collision`,
        // `validate_cleanup_singularity_reports_first_collision`).
        let c = caixa_with_etiquetas(vec!["a", "b", "a", "b"]);
        let err = c.validate_etiquetas().unwrap_err();
        let ManifestError::EtiquetaDuplicate { etiqueta } = err else {
            panic!("expected EtiquetaDuplicate, got {err:?}");
        };
        assert_eq!(etiqueta, "a");
    }

    #[test]
    fn validate_etiquetas_case_sensitive() {
        // Case-sensitivity pin: `("Foo" "foo")` is two distinct entries,
        // mirroring the peer `:membros :caixa` / `:children :caixa`
        // exact-string-match discipline. The downstream
        // `is_dns_1123_label` predicate would catch `"Foo"` as
        // uppercase if `:etiquetas` ever gained a per-entry shape gate,
        // but case-sensitivity at the duplicate-set layer is structural
        // — two distinct strings are two distinct entries.
        let c = caixa_with_etiquetas(vec!["Foo", "foo"]);
        c.validate_etiquetas().unwrap();
    }

    #[test]
    fn validate_etiquetas_diagnostic_carries_offending_tag() {
        // Diagnostic-shape pin (peer with
        // `validate_code_paths_diagnostic_carries_offending_slot_and_path`):
        // the error's Display surfaces the offending tag verbatim, so a
        // `feira lint` run can render the diagnostic without re-parsing
        // and the author can grep their caixa.lisp for the offending
        // value.
        let c = caixa_with_etiquetas(vec!["demo", "demo"]);
        let rendered = c.validate_etiquetas().unwrap_err().to_string();
        assert!(
            rendered.contains(":etiquetas"),
            "diagnostic must name the offending slot: {rendered}",
        );
        assert!(
            rendered.contains("demo"),
            "diagnostic must quote the offending tag: {rendered}",
        );
    }

    // ── validate_autores — universal-axis maintainer shape ────────────

    fn caixa_with_autores(autores: Vec<&str>) -> Caixa {
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.autores = autores.into_iter().map(String::from).collect();
        c
    }

    #[test]
    fn validate_autores_accepts_empty_list() {
        // The empty-list identity: `Caixa::template` emits `:autores ()`,
        // so the gate is non-disruptive against every existing manifest.
        let c = caixa_with_autores(vec![]);
        c.validate_autores().unwrap();
    }

    #[test]
    fn validate_autores_accepts_canonical_forms() {
        // Positive control sweep: every canonical-shaped non-empty
        // distinct maintainer list passes — the hello-rio / checkout-
        // aplicacao fixtures' `:autores ("pleme-io")` shape, plus the
        // multi-author shape downstream packaging surfaces emit.
        let c = caixa_with_autores(vec!["pleme-io"]);
        c.validate_autores().unwrap();
        let c = caixa_with_autores(vec!["alice <alice@example.com>", "bob <bob@example.com>"]);
        c.validate_autores().unwrap();
    }

    #[test]
    fn validate_autores_rejects_empty_entry() {
        // Canonical paste-from-blank-doc footgun. Without the gate the
        // empty entry rendered as `maintainers: [{name: "", email: null}]`
        // in `Chart.yaml`, a no-op maintainer the substrate cannot route
        // to.
        let c = caixa_with_autores(vec![""]);
        let err = c.validate_autores().unwrap_err();
        assert!(matches!(err, ManifestError::AutorEmpty), "got {err:?}",);
    }

    #[test]
    fn validate_autores_rejects_duplicate_entry() {
        // Canonical copy-paste-the-wrong-author footgun. Unlike the
        // `:etiquetas` peer (caixa-helm's `BTreeSet` collect silently
        // dedups the rendered `keywords:` array), the `maintainers:`
        // rendering has *no* dedup — duplicates stack verbatim. The
        // duplicate-arm names the offending author verbatim.
        let c = caixa_with_autores(vec!["pleme-io", "pleme-io"]);
        let err = c.validate_autores().unwrap_err();
        let ManifestError::AutorDuplicate { autor } = err else {
            panic!("expected AutorDuplicate, got {err:?}");
        };
        assert_eq!(autor, "pleme-io");
    }

    #[test]
    fn validate_autores_empty_takes_precedence_over_duplicate() {
        // Empty-first cascade pin: `("" "pleme-io" "pleme-io")` surfaces
        // `AutorEmpty` not `AutorDuplicate` — the narrower structural
        // "this entry has no value" defect dominates the cross-entry
        // uniqueness diagnostic. Mirrors the peer empty-before-duplicate
        // cascades on `:etiquetas` (`EtiquetaEmpty` before
        // `EtiquetaDuplicate`, 360a499), `:caracteristicas`
        // (`CaracteristicaEmpty` before `CaracteristicaDuplicate`,
        // fc3b4d5), and `:membros :caixa` (`MembroCaixaEmpty` before
        // `MembroDuplicate`).
        let c = caixa_with_autores(vec!["", "pleme-io", "pleme-io"]);
        let err = c.validate_autores().unwrap_err();
        assert!(matches!(err, ManifestError::AutorEmpty), "got {err:?}",);
    }

    #[test]
    fn validate_autores_duplicate_reports_first_collision() {
        // First-collision pin: `("a" "b" "a" "b")` surfaces the `"a"`
        // duplicate (the lexicographically-earliest offending position
        // — the second `"a"` at index 2 collides with the first `"a"`
        // at index 0), not the later `"b"` collision at index 3,
        // peer with every other first-collision diagnostic posture on
        // this surface.
        let c = caixa_with_autores(vec!["a", "b", "a", "b"]);
        let err = c.validate_autores().unwrap_err();
        let ManifestError::AutorDuplicate { autor } = err else {
            panic!("expected AutorDuplicate, got {err:?}");
        };
        assert_eq!(autor, "a");
    }

    #[test]
    fn validate_autores_case_sensitive() {
        // Case-sensitivity pin: `("Pleme-io" "pleme-io")` is two distinct
        // entries, mirroring the peer `:etiquetas` / `:membros :caixa`
        // / `:children :caixa` exact-string-match discipline.
        let c = caixa_with_autores(vec!["Pleme-io", "pleme-io"]);
        c.validate_autores().unwrap();
    }

    #[test]
    fn validate_autores_diagnostic_carries_offending_author() {
        // Diagnostic-shape pin (peer with
        // `validate_etiquetas_diagnostic_carries_offending_tag`): the
        // error's Display surfaces the offending author verbatim, so a
        // `feira lint` run can render the diagnostic without re-parsing
        // and the author can grep their caixa.lisp for the offending
        // value.
        let c = caixa_with_autores(vec!["pleme-io", "pleme-io"]);
        let rendered = c.validate_autores().unwrap_err().to_string();
        assert!(
            rendered.contains(":autores"),
            "diagnostic must name the offending slot: {rendered}",
        );
        assert!(
            rendered.contains("pleme-io"),
            "diagnostic must quote the offending author: {rendered}",
        );
    }

    // ── validate_repositorio — universal-axis git-repo-URL shape ──────

    fn caixa_with_repositorio(repositorio: Option<&str>) -> Caixa {
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.repositorio = repositorio.map(String::from);
        c
    }

    #[test]
    fn validate_repositorio_accepts_none() {
        // The omit-the-slot identity: `:repositorio` is optional. The
        // gate is a no-op when the author didn't declare a value —
        // every caixa without a `:repositorio` line trivially passes,
        // and the substrate-side renderers fall back to their
        // documented placeholder (`caixa-helm`'s `home: None`,
        // `caixa-flux`'s `https://github.com/pleme-io/<nome>` derived
        // URL). Mirrors the peer `validate_restart_window_accepts_none`
        // posture on the other `Option<String>` Caixa slot.
        let c = caixa_with_repositorio(None);
        c.validate_repositorio().unwrap();
    }

    #[test]
    fn validate_repositorio_accepts_canonical_forms() {
        // Positive control sweep across every documented `:repositorio`
        // authoring shape — the same union the shared
        // `crate::render::is_git_repo_url` predicate accepts and the
        // peer `:deps :fonte :repo` axis already routes through.
        // Covers the `github:` shorthand (the canonical pleme-io
        // convention used in the `:repositorio` field of every
        // manifest fixture across `caixa-helm` / `caixa-mesh` and the
        // `examples/`), the `https://…` URL the README quickstart uses,
        // the `ssh://`, `git://`, `git@host:path` scp-style SSH, and
        // `file://` URL schemes the shared predicate documents.
        for repo in [
            "github:pleme-io/hello-rio",
            "github:pleme-io/checkout",
            "https://github.com/pleme-io/hello-rio",
            "ssh://git@github.com/pleme-io/hello-rio.git",
            "git://github.com/pleme-io/hello-rio.git",
            "git@github.com:pleme-io/hello-rio.git",
            "file:///srv/pleme/hello-rio",
        ] {
            let c = caixa_with_repositorio(Some(repo));
            c.validate_repositorio()
                .unwrap_or_else(|err| panic!("canonical {repo:?} must pass: {err:?}"));
        }
    }

    #[test]
    fn validate_repositorio_rejects_empty_some() {
        // Canonical paste-from-blank-doc footgun. The narrower
        // [`ManifestError::RepositorioEmpty`] arm fires before the
        // shape predicate is consulted, mirroring the empty-first
        // cascade every peer per-axis identity gate uses
        // (`NomeEmpty` → `NomeInvalid`, `VersaoEmpty` → `VersaoInvalid`,
        // `FonteRepoEmpty` → `FonteRepoInvalid`). Without this gate
        // the empty `Some("")` silently passed the renderer's
        // `Option::unwrap_or_else(|| <fallback>)` (which only fires
        // on `None`) and landed as `home: ""` in `Chart.yaml` /
        // `url: ""` in the FluxCD `GitRepository`.
        let c = caixa_with_repositorio(Some(""));
        let err = c.validate_repositorio().unwrap_err();
        assert!(
            matches!(err, ManifestError::RepositorioEmpty),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_repositorio_rejects_whitespace() {
        // Paste-from-doc whitespace footgun. The shared
        // `is_git_repo_url` predicate refuses any whitespace byte; a
        // trailing space in a `:repositorio` value silently broke
        // `git clone '<value> '` at clone time. The diagnostic names
        // the offending value verbatim.
        let c = caixa_with_repositorio(Some("github:pleme-io/hello-rio "));
        let err = c.validate_repositorio().unwrap_err();
        let ManifestError::RepositorioInvalid { repositorio, .. } = err else {
            panic!("expected RepositorioInvalid, got {err:?}");
        };
        assert_eq!(repositorio, "github:pleme-io/hello-rio ");
    }

    #[test]
    fn validate_repositorio_rejects_control_char() {
        // Paste-from-multiline-doc CRLF footgun — control characters
        // at the URL boundary are a class of subprocess-arg injection
        // and break git's URL parser at every porcelain entry point.
        let c = caixa_with_repositorio(Some("https://example.com/repo\n"));
        let err = c.validate_repositorio().unwrap_err();
        assert!(
            matches!(err, ManifestError::RepositorioInvalid { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_repositorio_rejects_leading_dash() {
        // Canonical CLI-argument-injection footgun: `git clone <repo>`
        // interprets a leading `-` as a CLI flag, so a
        // `-upload-pack=…` value escapes the subprocess argument
        // boundary. The shared predicate refuses every leading-`-`
        // shape at validate time.
        let c = caixa_with_repositorio(Some("-upload-pack=evil"));
        let err = c.validate_repositorio().unwrap_err();
        assert!(
            matches!(err, ManifestError::RepositorioInvalid { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_repositorio_rejects_missing_colon_separator() {
        // The bare `org/repo` ambiguity footgun — `git clone` reads
        // a no-`:` form as a relative filesystem path rather than the
        // GitHub-shorthand expansion the author probably intended.
        // The shared predicate refuses every shape without a `:`
        // separator.
        let c = caixa_with_repositorio(Some("pleme-io/hello-rio"));
        let err = c.validate_repositorio().unwrap_err();
        assert!(
            matches!(err, ManifestError::RepositorioInvalid { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_repositorio_empty_takes_precedence_over_shape() {
        // Empty-first cascade pin: the empty `Some("")` surfaces the
        // narrower `RepositorioEmpty` not the shape-predicate-wrapped
        // `RepositorioInvalid`, mirroring the peer
        // `NomeEmpty` → `NomeInvalid`, `VersaoEmpty` → `VersaoInvalid`,
        // `FonteRepoEmpty` → `FonteRepoInvalid` cascades. The shared
        // `is_git_repo_url` predicate also rejects the empty input
        // (defensively, with its own `"must not be empty"` reason),
        // but the manifest-layer empty arm runs first to surface the
        // narrower diagnostic verbatim.
        let c = caixa_with_repositorio(Some(""));
        let err = c.validate_repositorio().unwrap_err();
        assert!(
            matches!(err, ManifestError::RepositorioEmpty),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_repositorio_diagnostic_carries_offending_value() {
        // Diagnostic-shape pin (peer with
        // `validate_autores_diagnostic_carries_offending_author`): the
        // error's Display surfaces the offending value + slot name
        // verbatim, so a `feira lint` run can render the diagnostic
        // without re-parsing and the author can grep their caixa.lisp
        // for the offending `:repositorio` value.
        let c = caixa_with_repositorio(Some("pleme-io/hello-rio"));
        let rendered = c.validate_repositorio().unwrap_err().to_string();
        assert!(
            rendered.contains(":repositorio"),
            "diagnostic must name the offending slot: {rendered}",
        );
        assert!(
            rendered.contains("pleme-io/hello-rio"),
            "diagnostic must quote the offending value: {rendered}",
        );
    }

    // ── validate_descricao — universal-axis Chart.yaml description shape ──

    fn caixa_with_descricao(descricao: Option<&str>) -> Caixa {
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.descricao = descricao.map(String::from);
        c
    }

    #[test]
    fn validate_descricao_accepts_none() {
        // The omit-the-slot identity: `:descricao` is optional. The
        // gate is a no-op when the author didn't declare a value —
        // every caixa without a `:descricao` line trivially passes,
        // and the substrate-side renderers fall back to their
        // documented `caixa.nome`-derived placeholder. Mirrors the
        // peer `validate_repositorio_accepts_none` posture on the
        // sibling `Option<String>` Caixa slot.
        let c = caixa_with_descricao(None);
        c.validate_descricao().unwrap();
    }

    #[test]
    fn validate_descricao_accepts_canonical_summary() {
        // Positive control: the canonical pleme-io descricao shape —
        // a short free-form prose summary — passes the gate. Covers
        // the fixture shapes the `caixa-helm` / `caixa-flux` /
        // `caixa-mesh` test fixtures use (`"Canonical Rust→wasm32-
        // wasip2 caixa Servico."`, `"Checkout flow."`).
        for desc in [
            "Canonical Rust→wasm32-wasip2 caixa Servico.",
            "Checkout flow.",
            "AWS provider caixa for tatara-lisp",
            "x",
        ] {
            let c = caixa_with_descricao(Some(desc));
            c.validate_descricao()
                .unwrap_or_else(|err| panic!("canonical {desc:?} must pass: {err:?}"));
        }
    }

    #[test]
    fn validate_descricao_rejects_empty_some() {
        // Canonical paste-from-blank-doc footgun. Without this gate
        // the empty `Some("")` silently passed the renderer's
        // `Option::unwrap_or_else(|| <fallback>)` (which only fires
        // on `None`) and landed as `description: ""` in `Chart.yaml`
        // and a blank `README.md` header. Mirrors the peer
        // [`ManifestError::RepositorioEmpty`] empty-arm on the
        // sibling `Option<String>` Caixa slot.
        let c = caixa_with_descricao(Some(""));
        let err = c.validate_descricao().unwrap_err();
        assert!(matches!(err, ManifestError::DescricaoEmpty), "got {err:?}",);
    }

    #[test]
    fn validate_descricao_template_passes() {
        // Round-trip pin: the bare `Caixa::template` shape carries
        // `:descricao "FIXME — describe this caixa"` (a non-empty
        // sentinel), so the template-derived Caixa passes the gate by
        // construction. A future template-shape change that omits or
        // empties `:descricao` would surface here as a regression.
        let c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.validate_descricao().unwrap();
    }

    #[test]
    fn validate_descricao_diagnostic_names_offending_slot() {
        // Diagnostic-shape pin (peer with
        // `validate_repositorio_diagnostic_carries_offending_value`):
        // the error's Display surfaces the `:descricao` slot name
        // verbatim, so a `feira lint` run can render the diagnostic
        // without re-parsing and the author can grep their caixa.lisp
        // for the offending `:descricao` line.
        let c = caixa_with_descricao(Some(""));
        let rendered = c.validate_descricao().unwrap_err().to_string();
        assert!(
            rendered.contains(":descricao"),
            "diagnostic must name the offending slot: {rendered}",
        );
    }

    // ── validate_licenca — universal-axis chart README license shape ──

    fn caixa_with_licenca(licenca: Option<&str>) -> Caixa {
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.licenca = licenca.map(String::from);
        c
    }

    #[test]
    fn validate_licenca_accepts_none() {
        // The omit-the-slot identity: `:licenca` is optional. The
        // gate is a no-op when the author didn't declare a value —
        // every caixa without a `:licenca` line trivially passes,
        // and the substrate-side `caixa-helm` renderer falls back to
        // the documented `"MIT"` placeholder. Mirrors the peer
        // `validate_descricao_accepts_none` posture on the sibling
        // `Option<String>` Caixa slot.
        let c = caixa_with_licenca(None);
        c.validate_licenca().unwrap();
    }

    #[test]
    fn validate_licenca_accepts_canonical_expressions() {
        // Positive control: every canonical SPDX expression shape
        // pleme-io carries in its existing fixtures + the canonical
        // SPDX dual-license / with-exception / `+`-suffix / grouped /
        // user-defined-reference shapes all pass the gate. Covers
        // the single-license, `OR`-compound, `AND`-compound,
        // `WITH`-exception, parenthesis-grouped, `+`-suffix, and
        // `LicenseRef-` / `DocumentRef-:LicenseRef-` shapes — every
        // production the SPDX 2.1 expression grammar admits that
        // sits within the alphabet floor the
        // `is_spdx_expression_shape` predicate enforces.
        for lic in [
            "MIT",
            "Apache-2.0",
            "Apache-2.0 OR MIT",
            "Apache-2.0 AND MIT",
            "BSD-3-Clause",
            "MPL-2.0",
            "GPL-3.0-or-later",
            "GPL-2.0+",
            "Apache-2.0 WITH LLVM-exception",
            "(MIT OR Apache-2.0) AND BSD-3-Clause",
            "(MIT OR Apache-2.0) AND BSD-3-Clause AND ISC",
            "LicenseRef-MyLicense",
            "DocumentRef-spdx-tool:LicenseRef-MIT-Style",
            "x",
        ] {
            let c = caixa_with_licenca(Some(lic));
            c.validate_licenca()
                .unwrap_or_else(|err| panic!("canonical {lic:?} must pass: {err:?}"));
        }
    }

    #[test]
    fn validate_licenca_rejects_trailing_whitespace() {
        // Paste-from-doc whitespace footgun. A trailing space in the
        // `:licenca` value would silently break a downstream SPDX
        // parser that splits on exact `AND` / `OR` / `WITH` keyword
        // boundaries. The shape predicate refuses every trailing
        // whitespace byte by construction. Peer with
        // `validate_repositorio_rejects_whitespace` and
        // `validate_edicao_rejects_trailing_whitespace`.
        let c = caixa_with_licenca(Some("MIT "));
        let err = c.validate_licenca().unwrap_err();
        let ManifestError::LicencaInvalid { licenca, .. } = err else {
            panic!("expected LicencaInvalid, got {err:?}");
        };
        assert_eq!(licenca, "MIT ");
    }

    #[test]
    fn validate_licenca_rejects_leading_whitespace() {
        // Symmetric paste-from-doc whitespace footgun on the leading
        // boundary — the gate refuses every shape that starts with a
        // space byte by construction. Peer with
        // `validate_edicao_rejects_leading_whitespace`.
        let c = caixa_with_licenca(Some(" MIT"));
        let err = c.validate_licenca().unwrap_err();
        assert!(
            matches!(err, ManifestError::LicencaInvalid { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_licenca_rejects_control_char() {
        // Paste-from-multiline-doc CRLF footgun — control characters
        // at the value boundary land as a malformed line in the
        // rendered chart `README.md` `## License` section. Peer with
        // `validate_repositorio_rejects_control_char` and
        // `validate_edicao_rejects_control_char`.
        for lic in ["MIT\n", "MIT\r\n", "MIT\rApache-2.0"] {
            let c = caixa_with_licenca(Some(lic));
            let err = c.validate_licenca().unwrap_err();
            assert!(
                matches!(err, ManifestError::LicencaInvalid { .. }),
                "expected LicencaInvalid on {lic:?}, got {err:?}",
            );
        }
    }

    #[test]
    fn validate_licenca_rejects_tab() {
        // Tab-from-aligned-doc footgun — SPDX expressions use a
        // single ASCII space between tokens; a tab breaks every
        // downstream SPDX parser that splits on exact `" "`
        // boundaries.
        let c = caixa_with_licenca(Some("MIT\tOR Apache-2.0"));
        let err = c.validate_licenca().unwrap_err();
        assert!(
            matches!(err, ManifestError::LicencaInvalid { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_licenca_rejects_non_ascii() {
        // Smart-quote / non-ASCII paste footgun — SPDX identifiers
        // are ASCII per the `idstring = 1*(ALPHA / DIGIT / "-" /
        // ".")` production. The shape predicate refuses every
        // non-ASCII byte by construction; peer with
        // `validate_edicao_rejects_non_ascii_lookalike`.
        for lic in ["MIT\u{a0}OR Apache-2.0", "MIT\u{2013}1.0", "Café-1.0"] {
            let c = caixa_with_licenca(Some(lic));
            let err = c.validate_licenca().unwrap_err();
            assert!(
                matches!(err, ManifestError::LicencaInvalid { .. }),
                "expected LicencaInvalid on {lic:?}, got {err:?}",
            );
        }
    }

    #[test]
    fn validate_licenca_rejects_underscore() {
        // Underscore-instead-of-hyphen typo footgun — `Apache_2.0` /
        // `MIT_Style` / `BSD_3_Clause` are familiar shapes from
        // snake-case identifier conventions that don't apply to the
        // SPDX `idstring` grammar (which admits only `ALPHA / DIGIT /
        // "-" / "."`). The shape predicate refuses every underscore
        // byte by construction.
        for lic in ["Apache_2.0", "MIT_Style", "BSD_3_Clause"] {
            let c = caixa_with_licenca(Some(lic));
            let err = c.validate_licenca().unwrap_err();
            assert!(
                matches!(err, ManifestError::LicencaInvalid { .. }),
                "expected LicencaInvalid on {lic:?}, got {err:?}",
            );
        }
    }

    #[test]
    fn validate_licenca_rejects_comma_separator() {
        // Comma-instead-of-`OR`-keyword colloquial idiom footgun —
        // SPDX expressions compose multiple licenses via `AND` / `OR`
        // keywords, not the comma separator. The shape predicate
        // refuses every comma byte by construction.
        for lic in ["MIT, Apache-2.0", "MIT,Apache-2.0"] {
            let c = caixa_with_licenca(Some(lic));
            let err = c.validate_licenca().unwrap_err();
            assert!(
                matches!(err, ManifestError::LicencaInvalid { .. }),
                "expected LicencaInvalid on {lic:?}, got {err:?}",
            );
        }
    }

    #[test]
    fn validate_licenca_rejects_slash_dual_license() {
        // Slash-dual-license colloquial idiom footgun — the
        // `MIT/Apache-2.0` shape is common in Cargo's pre-SPDX
        // `package.license` field but non-SPDX; the SPDX equivalent
        // is `MIT OR Apache-2.0`. The shape predicate refuses every
        // forward-slash byte by construction.
        for lic in ["MIT/Apache-2.0", "MIT/BSD-3-Clause"] {
            let c = caixa_with_licenca(Some(lic));
            let err = c.validate_licenca().unwrap_err();
            assert!(
                matches!(err, ManifestError::LicencaInvalid { .. }),
                "expected LicencaInvalid on {lic:?}, got {err:?}",
            );
        }
    }

    #[test]
    fn validate_licenca_rejects_semicolon_separator() {
        // Semicolon-list-separator confusion footgun — adjacent to
        // the comma-separator idiom, every list-separator-belongs-
        // to-list-grammar confusion lands here.
        let c = caixa_with_licenca(Some("MIT; Apache-2.0"));
        let err = c.validate_licenca().unwrap_err();
        assert!(
            matches!(err, ManifestError::LicencaInvalid { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_licenca_empty_takes_precedence_over_shape() {
        // Empty-first cascade pin: the empty `Some("")` surfaces the
        // narrower `LicencaEmpty` not the shape-predicate-wrapped
        // `LicencaInvalid`, mirroring the peer
        // `validate_edicao_empty_takes_precedence_over_shape` and
        // `validate_repositorio_empty_takes_precedence_over_shape`
        // (`RepositorioEmpty` → `RepositorioInvalid`), `NomeEmpty` →
        // `NomeInvalid`, `VersaoEmpty` → `VersaoInvalid` cascades.
        // The shape predicate also refuses the empty input
        // (defensively — `"must not be empty"`), but the manifest-
        // layer empty arm runs first to surface the narrower
        // diagnostic verbatim.
        let c = caixa_with_licenca(Some(""));
        let err = c.validate_licenca().unwrap_err();
        assert!(matches!(err, ManifestError::LicencaEmpty), "got {err:?}",);
    }

    #[test]
    fn validate_licenca_invalid_diagnostic_carries_offending_value() {
        // Diagnostic-shape pin on the shape-predicate arm (peer with
        // `validate_edicao_invalid_diagnostic_carries_offending_value`
        // and `validate_repositorio_diagnostic_carries_offending_value`):
        // the error's Display surfaces the offending value + slot
        // name verbatim, so a `feira lint` run can render the
        // diagnostic without re-parsing and the author can grep
        // their caixa.lisp for the offending `:licenca` value.
        let c = caixa_with_licenca(Some("Apache_2.0"));
        let rendered = c.validate_licenca().unwrap_err().to_string();
        assert!(
            rendered.contains(":licenca"),
            "diagnostic must name the offending slot: {rendered}",
        );
        assert!(
            rendered.contains("Apache_2.0"),
            "diagnostic must quote the offending value: {rendered}",
        );
    }

    #[test]
    fn validate_licenca_rejects_empty_some() {
        // Canonical paste-from-blank-doc footgun. Without this gate
        // the empty `Some("")` silently passed the renderer's
        // `Option::unwrap_or_else(|| "MIT".into())` (which only
        // fires on `None`) and landed as a bare trailing period in
        // the rendered chart `README.md` `## License` section.
        // Mirrors the peer [`ManifestError::DescricaoEmpty`] empty-
        // arm on the sibling `Option<String>` Caixa slot.
        let c = caixa_with_licenca(Some(""));
        let err = c.validate_licenca().unwrap_err();
        assert!(matches!(err, ManifestError::LicencaEmpty), "got {err:?}",);
    }

    #[test]
    fn validate_licenca_template_passes() {
        // Round-trip pin: the bare `Caixa::template` shape (whether
        // it carries `:licenca` or omits it) passes the gate by
        // construction. A future template-shape change that
        // introduced `(:licenca "")` would surface here as a
        // regression. Mirrors the peer
        // `validate_descricao_template_passes` pin.
        let c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.validate_licenca().unwrap();
    }

    #[test]
    fn validate_licenca_diagnostic_names_offending_slot() {
        // Diagnostic-shape pin (peer with
        // `validate_descricao_diagnostic_names_offending_slot`):
        // the error's Display surfaces the `:licenca` slot name
        // verbatim, so a `feira lint` run can render the diagnostic
        // without re-parsing and the author can grep their caixa.lisp
        // for the offending `:licenca` line.
        let c = caixa_with_licenca(Some(""));
        let rendered = c.validate_licenca().unwrap_err().to_string();
        assert!(
            rendered.contains(":licenca"),
            "diagnostic must name the offending slot: {rendered}",
        );
    }

    // ── validate_edicao — universal-axis language-edition shape ──

    fn caixa_with_edicao(edicao: Option<&str>) -> Caixa {
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.edicao = edicao.map(String::from);
        c
    }

    #[test]
    fn validate_edicao_accepts_none() {
        // The omit-the-slot identity: `:edicao` is optional. The
        // gate is a no-op when the author didn't declare a value —
        // every caixa without an `:edicao` line trivially passes,
        // and the substrate-side build pipeline falls back to the
        // documented default edition. Mirrors the peer
        // `validate_licenca_accepts_none` posture on the sibling
        // `Option<String>` Caixa slot.
        let c = caixa_with_edicao(None);
        c.validate_edicao().unwrap();
    }

    #[test]
    fn validate_edicao_accepts_canonical_value() {
        // Positive control: the canonical `"2026"` edition every
        // existing renderer-side fixture (`caixa-helm`, `caixa-flux`,
        // `caixa-mesh`) carries by construction passes the gate.
        // Future-introduced sibling editions (`"2027"`, `"2030"`,
        // `"2049"`) that match the same 4-digit ASCII decimal year
        // shape must also trivially pass — the structural shape
        // predicate accepts every well-formed year regardless of
        // whether the substrate yet understands the specific value
        // (a future known-edition allowlist tightens that).
        for ed in ["2026", "2027", "2030", "2049"] {
            let c = caixa_with_edicao(Some(ed));
            c.validate_edicao()
                .unwrap_or_else(|err| panic!("canonical {ed:?} must pass: {err:?}"));
        }
    }

    #[test]
    fn validate_edicao_rejects_empty_some() {
        // Canonical paste-from-blank-doc footgun. Without this gate
        // the empty `Some("")` silently lands as `(:edicao "")` in
        // the rendered caixa.lisp and a future renderer-side
        // consumer's `Option::unwrap_or_else` (which only fires on
        // `None`) skips its fallback. Mirrors the peer
        // [`ManifestError::LicencaEmpty`] empty-arm on the sibling
        // `Option<String>` Caixa slot.
        let c = caixa_with_edicao(Some(""));
        let err = c.validate_edicao().unwrap_err();
        assert!(matches!(err, ManifestError::EdicaoEmpty), "got {err:?}",);
    }

    #[test]
    fn validate_edicao_rejects_free_form_non_year() {
        // Free-form non-year footgun: the bare `"x"` / `"latest"` /
        // `"nightly"` shapes carry no operational meaning on the
        // substrate's build-time edition selector. Until this gate
        // landed the bare empty-arm check let every such value
        // through and broke far from the source caixa.lisp. Peer
        // with the shape-predicate cascade
        // `validate_repositorio_rejects_missing_colon_separator`
        // establishes past its own empty arm.
        for ed in ["x", "latest", "nightly", "stable"] {
            let c = caixa_with_edicao(Some(ed));
            let err = c.validate_edicao().unwrap_err();
            assert!(
                matches!(err, ManifestError::EdicaoInvalid { .. }),
                "expected EdicaoInvalid on {ed:?}, got {err:?}",
            );
        }
    }

    #[test]
    fn validate_edicao_rejects_trailing_whitespace() {
        // Paste-from-doc whitespace footgun. A trailing space in
        // the `:edicao` value would silently break the substrate's
        // build-time edition match-table lookup at the rendered
        // artifact's edition-selector consumer. The shape predicate
        // refuses every whitespace byte by construction (any byte
        // outside `0-9` fails `is_ascii_digit`). Peer with
        // `validate_repositorio_rejects_whitespace`.
        let c = caixa_with_edicao(Some("2026 "));
        let err = c.validate_edicao().unwrap_err();
        let ManifestError::EdicaoInvalid { edicao, .. } = err else {
            panic!("expected EdicaoInvalid, got {err:?}");
        };
        assert_eq!(edicao, "2026 ");
    }

    #[test]
    fn validate_edicao_rejects_leading_whitespace() {
        // Symmetric paste-from-doc whitespace footgun on the leading
        // boundary — the gate refuses every shape with a non-digit
        // byte by construction.
        let c = caixa_with_edicao(Some(" 2026"));
        let err = c.validate_edicao().unwrap_err();
        assert!(
            matches!(err, ManifestError::EdicaoInvalid { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_edicao_rejects_control_char() {
        // Paste-from-multiline-doc CRLF footgun — control characters
        // at the value boundary break the substrate's build-time
        // edition-selector parser. Peer with
        // `validate_repositorio_rejects_control_char`.
        let c = caixa_with_edicao(Some("2026\n"));
        let err = c.validate_edicao().unwrap_err();
        assert!(
            matches!(err, ManifestError::EdicaoInvalid { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_edicao_rejects_non_ascii_lookalike() {
        // Fullwidth-keyboard look-alike footgun — `"２０２６"` is
        // the U+FF12 U+FF10 U+FF12 U+FF16 sequence (CJK fullwidth
        // digits), 4 codepoints but 12 UTF-8 bytes; the substrate's
        // edition selector wants an ASCII year, and the gate
        // refuses every non-ASCII shape by construction (length in
        // bytes is 12 ≠ 4, *and* every byte falls outside
        // `is_ascii_digit`'s `0-9` range).
        let c = caixa_with_edicao(Some("２０２６"));
        let err = c.validate_edicao().unwrap_err();
        assert!(
            matches!(err, ManifestError::EdicaoInvalid { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_edicao_rejects_version_tag_prefix() {
        // Common version-tag idiom footgun — `"v2026"` / `"e2026"`
        // / `"r2026"` are familiar shapes from git-tag / Rust
        // edition / release-tag conventions that don't apply to
        // the year-shaped edition axis. The shape predicate refuses
        // every leading non-digit prefix.
        for ed in ["v2026", "e2026", "r2026"] {
            let c = caixa_with_edicao(Some(ed));
            let err = c.validate_edicao().unwrap_err();
            assert!(
                matches!(err, ManifestError::EdicaoInvalid { .. }),
                "expected EdicaoInvalid on {ed:?}, got {err:?}",
            );
        }
    }

    #[test]
    fn validate_edicao_rejects_decimal_shape() {
        // Decimal-shaped pseudo-version footgun — `"2026.1"` /
        // `"2026.0"` are familiar shapes from semver / float
        // conventions that don't apply to the year-shaped edition
        // axis. The shape predicate refuses every non-digit byte
        // (`.` falls outside `is_ascii_digit`).
        for ed in ["2026.1", "2026.0", "2026.0.1"] {
            let c = caixa_with_edicao(Some(ed));
            let err = c.validate_edicao().unwrap_err();
            assert!(
                matches!(err, ManifestError::EdicaoInvalid { .. }),
                "expected EdicaoInvalid on {ed:?}, got {err:?}",
            );
        }
    }

    #[test]
    fn validate_edicao_rejects_wrong_length_numeric() {
        // Wrong-length numeric footgun — `"26"` (truncated) /
        // `"202"` (truncated) / `"20260"` (extra digit) / `"00026"`
        // (zero-padded too wide) all parse as integers but don't
        // name a 4-digit year. The shape predicate refuses every
        // value whose length isn't exactly 4 bytes.
        for ed in ["26", "202", "20260", "00026", "9"] {
            let c = caixa_with_edicao(Some(ed));
            let err = c.validate_edicao().unwrap_err();
            assert!(
                matches!(err, ManifestError::EdicaoInvalid { .. }),
                "expected EdicaoInvalid on {ed:?}, got {err:?}",
            );
        }
    }

    #[test]
    fn validate_edicao_empty_takes_precedence_over_shape() {
        // Empty-first cascade pin: the empty `Some("")` surfaces
        // the narrower `EdicaoEmpty` not the shape-predicate-
        // wrapped `EdicaoInvalid`, mirroring the peer
        // `validate_repositorio_empty_takes_precedence_over_shape`
        // (`RepositorioEmpty` → `RepositorioInvalid`),
        // `NomeEmpty` → `NomeInvalid`, `VersaoEmpty` →
        // `VersaoInvalid`, `FonteRepoEmpty` → `FonteRepoInvalid`
        // cascades. The shape predicate also refuses the empty
        // input (defensively — `s.len() != 4`), but the
        // manifest-layer empty arm runs first to surface the
        // narrower diagnostic verbatim.
        let c = caixa_with_edicao(Some(""));
        let err = c.validate_edicao().unwrap_err();
        assert!(matches!(err, ManifestError::EdicaoEmpty), "got {err:?}",);
    }

    #[test]
    fn validate_edicao_template_passes() {
        // Round-trip pin: the bare `Caixa::template` shape (which
        // carries `:edicao "2026"` verbatim) passes the gate by
        // construction. A future template-shape change that
        // introduced `(:edicao "")` or a non-year value would
        // surface here as a regression. Mirrors the peer
        // `validate_licenca_template_passes` pin.
        let c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.validate_edicao().unwrap();
    }

    #[test]
    fn validate_edicao_diagnostic_names_offending_slot() {
        // Diagnostic-shape pin (peer with
        // `validate_licenca_diagnostic_names_offending_slot`): the
        // error's Display surfaces the `:edicao` slot name verbatim,
        // so a `feira lint` run can render the diagnostic without
        // re-parsing and the author can grep their caixa.lisp for
        // the offending `:edicao` line.
        let c = caixa_with_edicao(Some(""));
        let rendered = c.validate_edicao().unwrap_err().to_string();
        assert!(
            rendered.contains(":edicao"),
            "diagnostic must name the offending slot: {rendered}",
        );
    }

    #[test]
    fn validate_edicao_invalid_diagnostic_carries_offending_value() {
        // Diagnostic-shape pin on the shape-predicate arm (peer
        // with `validate_repositorio_diagnostic_carries_offending_value`):
        // the error's Display surfaces the offending value + slot
        // name verbatim, so a `feira lint` run can render the
        // diagnostic without re-parsing and the author can grep
        // their caixa.lisp for the offending `:edicao` value.
        let c = caixa_with_edicao(Some("v2026"));
        let rendered = c.validate_edicao().unwrap_err().to_string();
        assert!(
            rendered.contains(":edicao"),
            "diagnostic must name the offending slot: {rendered}",
        );
        assert!(
            rendered.contains("v2026"),
            "diagnostic must quote the offending value: {rendered}",
        );
    }
}
