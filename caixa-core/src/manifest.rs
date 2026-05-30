use serde::{Deserialize, Serialize};
use tatara_lisp::DeriveTataraDomain;

use thiserror::Error;

use crate::{
    CaixaKind, Dep, behavior::BehaviorSpec, dep::DepError, limits::LimitsSpec,
    render::is_dns_1123_label, supervisor::SupervisorSpec, upgrade::UpgradeFromEntry,
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
}
