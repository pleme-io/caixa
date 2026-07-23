use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tatara_lisp::DeriveTataraDomain;

use thiserror::Error;

use crate::{
    CaixaKind, Dep,
    behavior::BehaviorSpec,
    dep::DepError,
    limits::LimitsSpec,
    render::{
        PathShapeViolation, is_computeunit_yaml_extension, is_git_repo_url, is_lisp_extension,
        is_sandboxed_relative_path,
    },
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

    /// Substrate-canonical per-`Caixa` `:licenca` SPDX-expression scalar
    /// accessor every consumer of the top-level manifest's license axis
    /// keys off — returns the author-declared `:licenca` byte-string
    /// verbatim as an `Option<&str>`, borrowed from the typed slot's own
    /// `Option<String>` storage. `None` when the slot is absent (the
    /// canonical "omit to defer to the caixa-helm renderer's `MIT`
    /// fallback" shape [`Self::validate_licenca`] documents at
    /// caixa-core/src/manifest.rs:1560; the peer [`caixa-helm`]
    /// `build_readme` fold at caixa-helm/src/lib.rs:962 reads this
    /// predicate too, so an authored-but-unset `:licenca` round-trips to
    /// a rendered `lareira-<nome>` chart's `README.md` `## License`
    /// section structurally identical to one that omits the slot).
    ///
    /// The `:licenca` slot carries the universal-axis SPDX-expression
    /// license identifier every kind of caixa emits under (CAIXA-SDLC
    /// §I — the author-facing surface every `defcaixa` form supplies) —
    /// the typed slot's `Option<String>` accept-set (empty-string
    /// rejected through [`ManifestError::LicencaEmpty`], SPDX-alphabet-
    /// invalid rejected through [`ManifestError::LicencaInvalid`]) maps
    /// onto the `lareira-<nome>` Helm chart's `README.md` `## License`
    /// section (caixa-helm/src/lib.rs:962) and (through future
    /// tightening documented at [`Self::validate_licenca`]) the
    /// Chart.yaml `annotations["artifacthub.io/license"]` axis every
    /// registry-facing chart carries. Every downstream consumer that
    /// reads the license byte-string keys off this scalar (the
    /// [`Self::validate_licenca`] empty-arm + SPDX-shape gate that
    /// routes through `self.licenca.as_deref()`, the caixa-helm
    /// `build_readme` `unwrap_or_else(|| "MIT".into())` fold that keys
    /// the fallback off the `Option::is_none()` arm, every future
    /// per-`Caixa` registry-facing renderer the CAIXA-SDLC §I roadmap
    /// acknowledges).
    ///
    /// Prior to this lift the `.licenca` field was accessed inline at
    /// two production sites — [`Self::validate_licenca`]'s
    /// `self.licenca.as_deref()` empty-and-shape gate binding and the
    /// caixa-helm `build_readme` `caixa.licenca.clone().unwrap_or_else(||
    /// "MIT".into())` `README.md` `## License` fold — two open-coded
    /// field-accesses that expressed no compile-time link back to the
    /// typed slot. A future extension of the `:licenca` axis to a
    /// richer author surface — a per-`:licenca` structured SPDX
    /// expression parser + license-id allowlist (the future tightening
    /// [`Self::validate_licenca`]'s docstring acknowledges), a
    /// per-cluster license-default overlay the M4 CR materializer
    /// resolves per-CR (the "cluster policy pins `Apache-2.0` for every
    /// unlisted caixa" arm), a promotion of the plain
    /// `Option<String>` byte-string to a richer `SpdxExpression` enum
    /// once the SPDX-expression parser lands — would have had to be
    /// threaded through both open-coded copies in lockstep or the
    /// validate gate and the caixa-helm emit path would silently
    /// disagree on which license a given [`Caixa`] resolves to (an
    /// author's `:licenca "MIT OR Apache-2.0"` would satisfy validate
    /// while the emit path silently rendered a stale `MIT` fallback,
    /// or vice versa). Lifting the resolution to a typed method on the
    /// substrate primitive means every downstream consumer of the
    /// caixa's per-`Caixa` license surface reaches for exactly one
    /// typed dispatch — the resolver's accept-set migrates as a unit
    /// on any future axis addition.
    ///
    /// First `Option<&str>`-return top-level [`Caixa`] scalar accessor —
    /// opens the "outer [`Caixa`] `Option<&str>` scalar" projection
    /// pattern the sibling per-`Caixa` `:descricao` / `:repositorio` /
    /// `:edicao` future lifts fold on. Same "one typed dispatch on the
    /// substrate primitive, thin projections at each consumer"
    /// discipline the peer per-`:placement` [`crate::aplicacao::Placement::shard_key`]
    /// (7cd2a28) / [`crate::aplicacao::Placement::affinity`] (74ec2d3)
    /// / per-`:contratos` [`crate::aplicacao::WitContract::endpoint`]
    /// (7020470) / [`crate::aplicacao::WitContract::subject`] (90de675)
    /// / [`crate::aplicacao::WitContract::slot`] (ed22b66)
    /// `Option<&str>`-return accessors carry on the sibling M2 / M3
    /// typed-slot atom axes, extended here to the outer top-level
    /// `Caixa` universal-axis surface. Named `licenca()` to match the
    /// storage field's name; the accessor's identity maps onto the
    /// canonical CAIXA-SDLC §I vocabulary the slot's docstring already
    /// carries.
    #[must_use]
    pub fn licenca(&self) -> Option<&str> {
        self.licenca.as_deref()
    }

    /// Substrate-canonical per-`Caixa` `:repositorio` git-repo-URL scalar
    /// accessor every consumer of the top-level manifest's homepage /
    /// source-of-truth axis keys off — returns the author-declared
    /// `:repositorio` byte-string verbatim as an `Option<&str>`, borrowed
    /// from the typed slot's own `Option<String>` storage. `None` when
    /// the slot is absent (the canonical "omit to defer to the renderer's
    /// per-target placeholder" shape — [`caixa-helm`]'s `ChartYaml.home`
    /// carries the `Option<String>` through verbatim so an author-omitted
    /// `:repositorio` renders a `Chart.yaml` without a `home:` field
    /// (`skip_serializing_if = "Option::is_none"`), while [`caixa-flux`]'s
    /// `ClusterBundleOpts::for_caixa` folds the omitted slot through a
    /// `format!("https://github.com/{DEFAULT_PLEME_GIT_ORG}/{nome}")`
    /// fallback derived from `caixa.nome`).
    ///
    /// The `:repositorio` slot carries the universal-axis git-repo-URL
    /// homepage identifier every kind of caixa emits under (CAIXA-SDLC
    /// §I — the author-facing surface every `defcaixa` form supplies) —
    /// the typed slot's `Option<String>` accept-set (empty-string
    /// rejected through [`ManifestError::RepositorioEmpty`], git-repo-URL-
    /// shape-invalid rejected through [`ManifestError::RepositorioInvalid`]
    /// past the shared [`crate::render::is_git_repo_url`] predicate the
    /// peer per-`:deps :fonte :repo` axis also routes through) maps onto
    /// four load-bearing downstream consumers:
    ///
    ///   - [`Self::validate_repositorio`]'s empty-arm + shape-predicate
    ///     gate binding at caixa-core/src/manifest.rs:1456 — the
    ///     universal-axis identity gate wired at caixa-build time.
    ///   - [`caixa-helm`]'s `build_chart_yaml` `ChartYaml.home` fold at
    ///     caixa-helm/src/lib.rs:840 — the rendered `lareira-<nome>`
    ///     Helm chart's `Chart.yaml` `home:` field, which every registry
    ///     that ingests the chart (ArtifactHub, chartmuseum,
    ///     `helm search repo`) surfaces as the chart's canonical source-
    ///     of-truth link.
    ///   - [`caixa-helm`]'s `build_readme` `## Source` fold at
    ///     caixa-helm/src/lib.rs:957 — the rendered `lareira-<nome>`
    ///     chart's `README.md` header link back to the source repo,
    ///     which every author who inspects the rendered chart bundle
    ///     lands at.
    ///   - [`caixa-flux`]'s `ClusterBundleOpts::for_caixa`
    ///     `GitRepository.spec.url` fold at caixa-flux/src/lib.rs:2006 —
    ///     the rendered `GitRepository` CR's `spec.url` field, which
    ///     FluxCD's `source-controller` polls to reconcile the caixa's
    ///     manifest bundle from git.
    ///
    /// Prior to this lift the `.repositorio` field was accessed inline
    /// at four production sites — [`Self::validate_repositorio`]'s
    /// `self.repositorio.as_deref()` empty-and-shape gate binding, the
    /// caixa-helm `build_chart_yaml` `caixa.repositorio.clone()`
    /// `Chart.yaml` `home:` field fold, the caixa-helm `build_readme`
    /// `caixa.repositorio.clone().unwrap_or_else(|| caixa.nome.clone())`
    /// `README.md` `## Source` fold, and the caixa-flux
    /// `ClusterBundleOpts::for_caixa`
    /// `caixa.repositorio.clone().unwrap_or_else(|| format!(...))`
    /// `GitRepository.spec.url` fold — four open-coded field-accesses
    /// that expressed no compile-time link back to the typed slot. A
    /// future extension of the `:repositorio` axis to a richer author
    /// surface — a per-`:repositorio` structured
    /// [`crate::render::GitRepoUrl`]-shaped scheme+host+path parse
    /// (the future tightening [`Self::validate_repositorio`]'s
    /// docstring anticipates alongside the peer per-`:deps :fonte
    /// :repo` axis), a per-cluster repo-mirror overlay the M4 CR
    /// materializer resolves per-CR (the "cluster policy rewrites
    /// `github:pleme-io/...` to `git.internal/mirror/pleme-io/...`"
    /// arm the private-registry story acknowledges), a promotion of
    /// the plain `Option<String>` byte-string to a richer
    /// `RepoUrl` enum discriminated on scheme — would have had to be
    /// threaded through all four open-coded copies in lockstep or the
    /// validate gate and the three emit paths would silently disagree
    /// on which URL a given [`Caixa`] resolves to (an author's
    /// `:repositorio "github:pleme-io/checkout"` would satisfy validate
    /// while one of the emit paths silently rendered a stale URL, or
    /// vice versa). Lifting the resolution to a typed method on the
    /// substrate primitive means every downstream consumer of the
    /// caixa's per-`Caixa` repo-URL surface reaches for exactly one
    /// typed dispatch — the resolver's accept-set migrates as a unit on
    /// any future axis addition.
    ///
    /// Second outer top-level [`Caixa`] `Option<&str>`-return scalar
    /// accessor — sibling of [`Self::licenca`] (6d5bc28), the accessor
    /// that opened the "outer [`Caixa`] `Option<&str>` scalar"
    /// projection pattern this lift folds on. Same "one typed dispatch
    /// on the substrate primitive, thin projections at each consumer"
    /// discipline the peer per-`:placement`
    /// [`crate::aplicacao::Placement::shard_key`] (7cd2a28) /
    /// [`crate::aplicacao::Placement::affinity`] (74ec2d3) /
    /// per-`:contratos` [`crate::aplicacao::WitContract::endpoint`]
    /// (7020470) / [`crate::aplicacao::WitContract::subject`] (90de675)
    /// / [`crate::aplicacao::WitContract::slot`] (ed22b66)
    /// `Option<&str>`-return accessors carry on the sibling M2 / M3
    /// typed-slot atom axes, extended here to the second outer top-level
    /// `Caixa` universal-axis surface. Named `repositorio()` to match
    /// the storage field's name; the accessor's identity maps onto the
    /// canonical CAIXA-SDLC §I vocabulary the slot's docstring already
    /// carries.
    #[must_use]
    pub fn repositorio(&self) -> Option<&str> {
        self.repositorio.as_deref()
    }

    /// Substrate-canonical per-`Caixa` `:descricao` free-form-prose
    /// chart-description scalar accessor every consumer of the top-level
    /// manifest's Chart.yaml `description:` axis keys off — returns the
    /// author-declared `:descricao` byte-string verbatim as an
    /// `Option<&str>`, borrowed from the typed slot's own
    /// `Option<String>` storage. `None` when the slot is absent (the
    /// canonical "omit to defer to the per-renderer `caixa.nome`-derived
    /// fallback" shape — [`caixa-helm`]'s `build_chart_yaml` folds the
    /// omitted slot through a `format!("Generated chart for caixa Servico
    /// {}", caixa.nome)` fallback, [`caixa-helm`]'s `build_readme` folds
    /// it through a `format!("caixa Servico {}", caixa.nome)` fallback,
    /// and [`caixa-feira`]'s `render_flake` folds it through a
    /// `format!("caixa {}", c.nome)` `flake.nix` `description = ""`
    /// fallback — each derived from `caixa.nome` on the null-carrier arm).
    ///
    /// The `:descricao` slot carries the universal-axis free-form-prose
    /// chart-description identifier every kind of caixa emits under
    /// (CAIXA-SDLC §I — the author-facing surface every `defcaixa` form
    /// supplies) — the typed slot's `Option<String>` accept-set
    /// (empty-string rejected through [`ManifestError::DescricaoEmpty`],
    /// chart-description-shape-invalid rejected through
    /// [`ManifestError::DescricaoInvalid`] past the shared
    /// [`crate::render::is_chart_description_shape`] predicate the peer
    /// per-`Caixa` `:descricao` axis also routes through) maps onto four
    /// load-bearing downstream consumers:
    ///
    ///   - [`Self::validate_descricao`]'s empty-arm + shape-predicate
    ///     gate binding — the universal-axis identity gate wired at
    ///     caixa-build time.
    ///   - [`caixa-helm`]'s `build_chart_yaml` `ChartYaml.description`
    ///     `Chart.yaml` field fold — the rendered `lareira-<nome>` Helm
    ///     chart's `Chart.yaml` `description:` field, which
    ///     `apiVersion: v2` charts require non-empty (`helm lint` fires
    ///     `WARNING [chart.metadata.description]: description is required`
    ///     when absent) and which every registry that ingests the chart
    ///     (ArtifactHub, chartmuseum, `helm search repo`) surfaces as the
    ///     chart's canonical one-line prose descriptor.
    ///   - [`caixa-helm`]'s `build_readme` chart-`README.md` header fold
    ///     — the rendered `lareira-<nome>` chart's `README.md` prose
    ///     header directly beneath the `# <chart-name>` title, which
    ///     every author who inspects the rendered chart bundle lands at.
    ///   - [`caixa-feira`]'s `render_flake` `flake.nix` `description = ""`
    ///     top-level fold — the emitted `flake.nix`'s `description`
    ///     field, which every Nix consumer (`nix flake show`,
    ///     `nix flake metadata`, downstream flake-registry ingestors)
    ///     surfaces as the flake's canonical descriptor.
    ///
    /// Prior to this lift the `.descricao` field was accessed inline at
    /// four production sites — [`Self::validate_descricao`]'s
    /// `self.descricao.as_deref()` empty-and-shape gate binding, the
    /// caixa-helm `build_chart_yaml`
    /// `caixa.descricao.clone().unwrap_or_else(|| format!(...))`
    /// `Chart.yaml` `description:` fold, the caixa-helm `build_readme`
    /// `caixa.descricao.clone().unwrap_or_else(|| format!(...))`
    /// `README.md` header fold, and the caixa-feira `render_flake`
    /// `c.descricao.clone().unwrap_or_else(|| format!(...))` `flake.nix`
    /// `description = ""` fold — four open-coded field-accesses that
    /// expressed no compile-time link back to the typed slot. A future
    /// extension of the `:descricao` axis to a richer author surface —
    /// a per-`:descricao` locale-tagged multi-language descriptor map
    /// (the "one caixa, N language-tagged prose descriptions" arm
    /// author-tooling internationalization anticipates), a
    /// per-registry-target length-and-shape overlay the M4 CR
    /// materializer resolves per-CR (the "ArtifactHub caps description
    /// at 512 bytes but the internal registry caps at 256" arm), a
    /// promotion of the plain `Option<String>` byte-string to a richer
    /// `ChartDescription` newtype guaranteeing the
    /// `is_chart_description_shape` predicate at the type level — would
    /// have had to be threaded through all four open-coded copies in
    /// lockstep or the validate gate and the three emit paths would
    /// silently disagree on which prose string a given [`Caixa`]
    /// resolves to (an author's
    /// `:descricao "Checkout flow orchestration."` would satisfy
    /// validate while one of the emit paths silently rendered a stale
    /// `caixa.nome`-derived fallback, or vice versa). Lifting the
    /// resolution to a typed method on the substrate primitive means
    /// every downstream consumer of the caixa's per-`Caixa`
    /// chart-description surface reaches for exactly one typed dispatch
    /// — the resolver's accept-set migrates as a unit on any future
    /// axis addition.
    ///
    /// Third outer top-level [`Caixa`] `Option<&str>`-return scalar
    /// accessor — sibling of [`Self::licenca`] (6d5bc28) and
    /// [`Self::repositorio`] (cc7332d), the accessors that opened the
    /// "outer [`Caixa`] `Option<&str>` scalar" projection pattern this
    /// lift folds on. Same "one typed dispatch on the substrate
    /// primitive, thin projections at each consumer" discipline the
    /// peer per-`:placement`
    /// [`crate::aplicacao::Placement::shard_key`] (7cd2a28) /
    /// [`crate::aplicacao::Placement::affinity`] (74ec2d3) /
    /// per-`:contratos` [`crate::aplicacao::WitContract::endpoint`]
    /// (7020470) / [`crate::aplicacao::WitContract::subject`] (90de675)
    /// / [`crate::aplicacao::WitContract::slot`] (ed22b66)
    /// `Option<&str>`-return accessors carry on the sibling M2 / M3
    /// typed-slot atom axes, extended here to the third outer top-level
    /// `Caixa` universal-axis surface. Named `descricao()` to match the
    /// storage field's name; the accessor's identity maps onto the
    /// canonical CAIXA-SDLC §I vocabulary the slot's docstring already
    /// carries. The one remaining universal `Option<String>` slot
    /// (`:edicao`) folds on this pattern next.
    #[must_use]
    pub fn descricao(&self) -> Option<&str> {
        self.descricao.as_deref()
    }

    /// Substrate-canonical per-`Caixa` `:edicao` language-edition scalar
    /// accessor every consumer of the top-level manifest's tatara-lisp
    /// edition-selector axis keys off — returns the author-declared
    /// `:edicao` byte-string verbatim as an `Option<&str>`, borrowed from
    /// the typed slot's own `Option<String>` storage. `None` when the
    /// slot is absent (the canonical "omit the slot to defer to the
    /// substrate's default edition" shape every existing
    /// [`caixa-resolver`] integration test fixture carries via
    /// `edicao: None` — see `caixa-resolver/tests/git_integration.rs`;
    /// the peer [`Self::validate_edicao`] gate is a no-op on the omitted
    /// arm by construction, so an author-omitted `:edicao` round-trips
    /// to a build without triggering the year-shape predicate).
    ///
    /// The `:edicao` slot carries the universal-axis 4-digit-ASCII-
    /// decimal-year language-edition identifier every kind of caixa
    /// emits under (CAIXA-SDLC §I — the author-facing surface every
    /// `defcaixa` form supplies) — the typed slot's `Option<String>`
    /// accept-set (empty-string rejected through
    /// [`ManifestError::EdicaoEmpty`], year-shape-invalid rejected
    /// through [`ManifestError::EdicaoInvalid`] past the 4-digit-ASCII-
    /// decimal-year predicate [`Self::validate_edicao`] enforces) maps
    /// onto one load-bearing downstream consumer today
    /// ([`Self::validate_edicao`]'s empty-arm + year-shape-predicate
    /// gate binding at caixa-core/src/manifest.rs:1959) plus every
    /// future edition-aware substrate consumer the CAIXA-SDLC §I
    /// roadmap anticipates (the tatara-lisp compiler's macro-surface
    /// selector every edition-aware build step keys off, the future
    /// per-edition compatibility-flag overlay the M4 CR materializer
    /// resolves per-CR, the peer [`Caixa::template`] canonical
    /// `:edicao "2026"` scaffold every `feira init` emits verbatim,
    /// and the renderer-side fixtures at `caixa-helm/src/lib.rs:978` /
    /// `caixa-flux/src/lib.rs:2319` / `caixa-mesh/src/lib.rs:3208` that
    /// carry `edicao: Some("2026".into())` by construction).
    ///
    /// Prior to this lift the `.edicao` field was accessed inline at
    /// one production site — [`Self::validate_edicao`]'s
    /// `self.edicao.as_deref()` empty-and-shape gate binding — one
    /// open-coded field-access that expressed no compile-time link
    /// back to the typed slot. A future extension of the `:edicao`
    /// axis to a richer author surface — a per-`:edicao` known-
    /// edition allowlist (the future tightening
    /// [`Self::validate_edicao`]'s docstring acknowledges past the
    /// structural year-shape floor, rejecting year-shaped values that
    /// don't name a tatara-lisp edition the substrate actually
    /// understands — `"1999"` is year-shaped but no `1999` edition
    /// exists), a per-edition compatibility-flag overlay the M4 CR
    /// materializer resolves per-CR (the "edition `"2026"` enables
    /// macro-surface features the sibling `"2018"` gates behind a
    /// feature flag" arm the edition-selector story anticipates), a
    /// promotion of the plain `Option<String>` byte-string to a
    /// richer `CaixaEdition` enum discriminated on year once a sibling
    /// edition to `"2026"` lands — would have had to be threaded
    /// through the open-coded copy in lockstep with every future
    /// edition-aware consumer, or the validate gate and the future
    /// edition-aware consumer path would silently disagree on which
    /// edition a given [`Caixa`] resolves to (an author's
    /// `:edicao "2026"` would satisfy validate while a future
    /// edition-aware consumer silently defaulted to a stale edition,
    /// or vice versa). Lifting the resolution to a typed method on
    /// the substrate primitive means every downstream consumer of the
    /// caixa's per-`Caixa` edition surface reaches for exactly one
    /// typed dispatch — the resolver's accept-set migrates as a unit
    /// on any future axis addition.
    ///
    /// Fourth and final outer top-level [`Caixa`] `Option<&str>`-return
    /// scalar accessor — sibling of [`Self::licenca`] (6d5bc28),
    /// [`Self::repositorio`] (cc7332d), and [`Self::descricao`]
    /// (3f16e2f), the accessors that opened the "outer [`Caixa`]
    /// `Option<&str>` scalar" projection pattern this lift folds on.
    /// Same "one typed dispatch on the substrate primitive, thin
    /// projections at each consumer" discipline the peer per-`:placement`
    /// [`crate::aplicacao::Placement::shard_key`] (7cd2a28) /
    /// [`crate::aplicacao::Placement::affinity`] (74ec2d3) /
    /// per-`:contratos` [`crate::aplicacao::WitContract::endpoint`]
    /// (7020470) / [`crate::aplicacao::WitContract::subject`] (90de675)
    /// / [`crate::aplicacao::WitContract::slot`] (ed22b66)
    /// `Option<&str>`-return accessors carry on the sibling M2 / M3
    /// typed-slot atom axes, extended here to close the outer top-level
    /// `Caixa` universal-axis surface's last unlifted `Option<String>`
    /// slot. Named `edicao()` to match the storage field's name; the
    /// accessor's identity maps onto the canonical CAIXA-SDLC §I
    /// vocabulary the slot's docstring already carries.
    #[must_use]
    pub fn edicao(&self) -> Option<&str> {
        self.edicao.as_deref()
    }

    /// Substrate-canonical per-`Caixa` `:nome` universal-axis DNS-1123-
    /// label caixa-identity scalar accessor every consumer of the top-
    /// level manifest's identity axis keys off — returns the author-
    /// declared `:nome` byte-string verbatim as an `&str`, borrowed from
    /// the typed slot's own `String` storage. Non-optional (`:nome` is
    /// a required-axis scalar every `defcaixa` form must supply; the
    /// [`Self::from_lisp`] derive rejects an omitted / non-string
    /// `:nome` at parse time, so a `Caixa` past parse definitionally
    /// carries a non-`None` `:nome`).
    ///
    /// The `:nome` slot carries the universal-axis DNS-1123-label
    /// caixa-identity every kind of caixa emits under (CAIXA-SDLC §I —
    /// the primary identity axis every `defcaixa` form supplies
    /// alongside `:versao` / `:kind`; the substrate-wide identity every
    /// other typed surface that names a caixa reaches through — `:deps`
    /// entries, `:membros` entries, `:children` entries, the
    /// `lareira-<nome>` Helm chart name every per-Servico renderer
    /// derives, the `pleme-program-<nome>` label every per-Aplicacao
    /// renderer emits) — the typed slot's `String` accept-set (empty
    /// rejected through [`ManifestError::NomeEmpty`], DNS-1123-shape-
    /// invalid rejected through [`ManifestError::NomeInvalid`] past
    /// the shared [`crate::render::require_valid_dns_1123_label`] gate
    /// the peer name axes each land on, joint-length-with-`lareira-`-
    /// prefix rejected through
    /// [`ManifestError::NomeChartNameBudgetExceeded`] past
    /// [`crate::render::is_lareira_chart_name_shape`]) maps onto every
    /// load-bearing downstream consumer the substrate carries — the
    /// two universal-axis validate gates at caixa-build time
    /// ([`Self::validate_nome`] + [`Self::validate_nome_chart_name_budget`]),
    /// [`crate::lareira_chart_name`]'s `lareira-<nome>` Helm chart-name
    /// derivation every per-Servico renderer keys off, the caixa-helm
    /// `Chart.yaml`'s `name:` axis, caixa-flux's `programs.yaml` entry
    /// `name:` axis, caixa-mesh's Cilium `CiliumNetworkPolicy` /
    /// `HTTPRoute` per-Aplicacao name axes at
    /// caixa-mesh/src/lib.rs:{2650, 2797, 2919, 2925},
    /// [`crate::pleme_program_selector`] /
    /// [`crate::pleme_program_in_aplicacao_selector`] label-selector
    /// derivations, and every future substrate renderer that emits an
    /// artifact keyed by the caixa's identity.
    ///
    /// Prior to this lift the `.nome` field was accessed inline at a
    /// dozen production sites across `caixa-core` (the two universal-
    /// axis validate gates + [`Dep::validate`]-adjacent duplicate
    /// tracking), `caixa-helm` (the `lareira_chart_name` fold, the
    /// `ChartYaml.name` / `ChartYaml.description` / `Chart.yaml`
    /// `keywords` fallback), `caixa-flux` (the `programs.yaml`
    /// entry `name:` fold, the `flux_kustomization_source_subtree`
    /// per-cluster subpath derivation), and `caixa-mesh` (the
    /// `pleme_program_in_aplicacao_selector` label-selector fold, the
    /// `cilium_network_policy_name` / `gateway_api_http_route_name`
    /// per-CR name derivations, the `LABEL_APLICACAO` labels-map
    /// insert) — a dozen open-coded field-accesses that expressed no
    /// compile-time link back to the typed slot. A future extension of
    /// the `:nome` axis to a richer author surface — a per-`:nome`
    /// structured `CaixaIdentity` newtype that carries the joint-
    /// length-with-prefix invariant [`Self::validate_nome_chart_name_budget`]
    /// enforces at the type level (rather than as a validate-time
    /// gate), a per-registry `:nome` namespacing overlay the M4 CR
    /// materializer resolves per-CR (the "`pleme-io/checkout` vs
    /// `partner-org/checkout` collision" arm the multi-tenant-registry
    /// story acknowledges), a promotion of the plain `String` byte-
    /// string to a richer `CaixaNome` newtype discriminated on
    /// namespace prefix — would have had to be threaded through every
    /// open-coded copy in lockstep or the two validate gates and the
    /// dozen emit paths would silently disagree on which identity a
    /// given [`Caixa`] resolves to (an author's `:nome "checkout"`
    /// would satisfy validate while one of the emit paths silently
    /// rendered a drifted other identity, or vice versa). Lifting the
    /// resolution to a typed method on the substrate primitive means
    /// every downstream consumer of the caixa's per-`Caixa` identity
    /// surface reaches for exactly one typed dispatch — the resolver's
    /// accept-set migrates as a unit on any future axis addition.
    ///
    /// First outer top-level [`Caixa`] `&str`-return required-scalar
    /// accessor — opens the "outer [`Caixa`] `&str` required-scalar"
    /// projection pattern the sibling per-`Caixa` `:versao` future lift
    /// folds on. Sibling in shape to the peer per-`:membros`
    /// [`crate::aplicacao::Membro::nome`] (4a32abf) / per-`:contratos`
    /// [`crate::aplicacao::WitContract::source`] /
    /// [`crate::aplicacao::WitContract::destination`] (7f0fd43),
    /// [`crate::aplicacao::WitContract::world_ref`] (0804823),
    /// [`crate::aplicacao::Membro::versao_requirement`] (a40b0e3),
    /// [`crate::aplicacao::Entrada::destination`] (6db982c),
    /// [`crate::aplicacao::CircuitBreaker::max_failures`] (3a74062),
    /// per-sub-struct required-axis accessors carry on the sibling M3
    /// mesh-slot-atom scalar-value axes, extended here to open the
    /// outer top-level [`Caixa`] `&str`-return required-scalar surface.
    /// Named `nome()` to match the storage field's name; the accessor's
    /// identity maps onto the canonical CAIXA-SDLC §I vocabulary the
    /// slot's docstring already carries.
    #[must_use]
    pub fn nome(&self) -> &str {
        &self.nome
    }

    /// Substrate-canonical per-`Caixa` `:versao` universal-axis SemVer-2
    /// pinned-version scalar accessor every consumer of the top-level
    /// manifest's version axis keys off — returns the author-declared
    /// `:versao` byte-string verbatim as an `&str`, borrowed from the
    /// typed slot's own `String` storage. Non-optional (`:versao` is a
    /// required-axis scalar every `defcaixa` form must supply alongside
    /// `:nome` / `:kind`; the [`Self::from_lisp`] derive rejects an
    /// omitted / non-string `:versao` at parse time, so a `Caixa` past
    /// parse definitionally carries a non-`None` `:versao`).
    ///
    /// The `:versao` slot carries the universal-axis SemVer-2
    /// concrete-version body every kind of caixa emits under
    /// (CAIXA-SDLC §I — the required-scalar every `defcaixa` form
    /// supplies alongside `:nome` / `:kind`; the substrate-wide
    /// pinned-version every downstream artifact-emitting consumer
    /// composes under — the `lareira-<nome>` Helm chart's `Chart.yaml`
    /// `version:` + `appVersion:` axes, the `feira publish` Zig-style
    /// `v<versao>` git tag the [`crate::DEFAULT_PUBLISH_TAG_PREFIX`]
    /// prefix composes on top of, the programs.yaml entry's `versao:`
    /// value the `lareira-fleet-programs` aggregator carries onto each
    /// rendered `ComputeUnit`, the OCI image's `:v<versao>` / `:latest`
    /// tags every substrate-side `skopeo push` writes, the lacre
    /// closure's pinned `concrete_versao`, and the `:upgrade-from :from`
    /// prior-version references peers in the exact same SemVer-2 shape).
    /// The typed slot's `String` accept-set (empty rejected through
    /// [`ManifestError::VersaoEmpty`], SemVer-2-shape-invalid rejected
    /// through [`ManifestError::VersaoInvalid`] past
    /// [`semver::Version::parse`]) maps onto every load-bearing
    /// downstream consumer the substrate carries — the [`Self::validate_versao`]
    /// universal-axis validate gate at caixa-build time, the
    /// [`crate::CaixaVersion::parse`] typed-wrapper resolver,
    /// [`caixa-helm`]'s `Chart.yaml` `version:` / `appVersion:` fold,
    /// [`caixa-flux`]'s `programs.yaml` entry `versao:` fold + the
    /// `cluster_bundle` `GitRepository` `ref: { tag: v<versao> }`
    /// derivation, [`caixa-mesh`]'s per-Aplicacao `programs.yaml` fan-
    /// out entry `versao:` fold, [`caixa-feira`]'s `feira publish` git-
    /// tag derivation (`format!("{prefix}{versao}")`), and every future
    /// substrate renderer that emits an artifact keyed by the caixa's
    /// pinned version.
    ///
    /// Prior to this lift the `.versao` field was accessed inline at a
    /// dozen production sites across `caixa-core` (the universal-axis
    /// [`Self::validate_versao`] gate + [`Dep::validate`]-adjacent
    /// version-shape gates), `caixa-helm` (the `ChartYaml.version` /
    /// `ChartYaml.app_version` folds), `caixa-flux` (the `programs.yaml`
    /// entry `versao:` fold, the `cluster_bundle` `GitRepository` `ref:
    /// { tag: v<versao> }` derivation), `caixa-mesh` (the per-Aplicacao
    /// `programs.yaml` fan-out entry `versao:` fold), and `caixa-feira`
    /// (the `feira publish` git-tag derivation + the `feira app graph` /
    /// `feira app deploy` diagnostic renderers) — a dozen open-coded
    /// field-accesses that expressed no compile-time link back to the
    /// typed slot. A future extension of the `:versao` axis to a richer
    /// author surface — a per-`:versao` structured `CaixaVersion` at the
    /// storage layer (the substrate already carries a `CaixaVersion`
    /// newtype at [`crate::version::CaixaVersion`], deferred until the
    /// serde-transparent-newtype-through-DeriveTataraDomain path lands),
    /// a per-registry `:versao` immutability overlay the M4 CR
    /// materializer enforces per-CR, a promotion of the plain `String`
    /// byte-string to a richer `PinnedVersao` newtype discriminated on
    /// SemVer-2 pre-release / build-metadata presence — would have had
    /// to be threaded through every open-coded copy in lockstep or the
    /// validate gate and the dozen emit paths would silently disagree
    /// on which version a given [`Caixa`] resolves to (an author's
    /// `:versao "0.1.0"` would satisfy validate while one of the emit
    /// paths silently rendered a drifted other version, or vice versa).
    /// Lifting the resolution to a typed method on the substrate
    /// primitive means every downstream consumer of the caixa's
    /// per-`Caixa` pinned-version surface reaches for exactly one typed
    /// dispatch — the resolver's accept-set migrates as a unit on any
    /// future axis addition.
    ///
    /// Second outer top-level [`Caixa`] `&str`-return required-scalar
    /// accessor — folds on the "outer [`Caixa`] `&str` required-scalar"
    /// projection pattern the sibling per-`Caixa` [`Self::nome`]
    /// (e6b7d97) opened. Sibling in shape to the peer per-`:membros`
    /// [`crate::aplicacao::Membro::versao_requirement`] (4127bb6) /
    /// per-`:children` [`crate::supervisor::ChildSpec::versao_requirement`]
    /// (2c053c8) / per-`:upgrade-from` [`crate::UpgradeFromEntry::prior_versao`]
    /// (75d27a8) per-sub-struct `:versao`-shaped `&str`-return accessors
    /// on the sibling per-typed-slot version-carrier axes, extended here
    /// to close the second outer top-level [`Caixa`] required-`&str`-
    /// carrying axis so the two universal-axis identity-carrying
    /// scalars every `defcaixa` form supplies (`:nome` + `:versao`)
    /// share the same "one typed dispatch per axis" discipline. Named
    /// `versao()` to match the storage field's name; the accessor's
    /// identity maps onto the canonical CAIXA-SDLC §I vocabulary the
    /// slot's docstring already carries.
    #[must_use]
    pub fn versao(&self) -> &str {
        &self.versao
    }

    /// Substrate-canonical per-`Caixa` `:kind` universal-axis
    /// closed-set-enum discriminant accessor every consumer of the top-
    /// level manifest's kind axis keys off — returns the author-declared
    /// `:kind` variant verbatim as a [`CaixaKind`], `Copy`-projected
    /// from the typed slot's own [`CaixaKind`] storage. Non-optional
    /// (`:kind` is a required-axis discriminant every `defcaixa` form
    /// must supply alongside `:nome` / `:versao`; the [`Self::from_lisp`]
    /// derive rejects an omitted / non-symbol `:kind` at parse time, so
    /// a `Caixa` past parse definitionally carries a valid [`CaixaKind`]
    /// variant).
    ///
    /// The `:kind` slot carries the universal-axis closed-set typed-
    /// discriminant every substrate-side dispatch keys off (CAIXA-SDLC
    /// §I — the primary shape gate every renderer / verifier /
    /// operator branches on; the five variants `Biblioteca` /
    /// `Binario` / `Servico` / `Supervisor` / `Aplicacao` partition
    /// the caixa surface into disjoint runtime contracts) — the typed
    /// slot's [`CaixaKind`] accept-set (parse-time-rejected non-symbol
    /// values through the derive-macro's symbol-arm gate, exhaustively
    /// matched at every downstream dispatch site) maps onto every
    /// load-bearing downstream consumer the substrate carries:
    ///
    ///   - [`crate::render::require_kind`]'s per-renderer entry-gate
    ///     predicate — the canonical two-line
    ///     `require_kind(caixa, Servico)?` prelude every per-Servico
    ///     renderer (`caixa-helm`, `caixa-flux`, the future `caixa-otel`
    ///     / per-Servico OCI packager / M4 `wasm.pleme.io/v1alpha1/
    ///     ComputeUnit` CR materializer) runs at its entry-point,
    ///     alongside the [`crate::render::KindMismatch`] error carrier's
    ///     `actual:` field the diagnostic surfaces to name the offending
    ///     caixa's variant.
    ///   - [`Self::aplicacao_view`]'s + [`Self::supervisor_view`]'s
    ///     per-view kind-gate binding — the two `Option<TypedSpec>`
    ///     `_view` composers that fold the flat mesh-slot / supervisor-
    ///     slot columns into their typed sub-spec only when the kind
    ///     matches (returns `None` otherwise); the future per-Servico
    ///     M2-view composer (`servico_view`) will follow the same shape.
    ///   - [`Self::declared_foreign_code_slots`]'s per-slot kind-
    ///     coherence gate — the `!self.kind.requires_exe()` /
    ///     `!self.kind.requires_servicos()` predicates that fence
    ///     each code-surface slot from the wrong owning kind.
    ///   - [`crate::LayoutInvariants::verify`]'s kind ↔ code-surface
    ///     coherence gates — the six `caixa.kind == CaixaKind::X` /
    ///     `caixa.kind != CaixaKind::X` predicates and the four kind-
    ///     coherence error carriers (`SupervisorOwnsCode` /
    ///     `AplicacaoOwnsCode` / `MeshSlotsOnNonAplicacao` /
    ///     `SupervisorSlotsOnNonSupervisor` / `ServicoSlotsOnNonServico`
    ///     / `ForeignCodeSlot`) which each name the offending caixa's
    ///     variant in their `kind:` field.
    ///
    /// Prior to this lift the `.kind` field was accessed inline at
    /// twenty-plus production sites across `caixa-core` (the
    /// [`crate::render::require_kind`] entry-gate predicate + the
    /// [`crate::render::KindMismatch`] `actual:` field, the two `_view`
    /// composers, the `declared_foreign_code_slots` per-slot kind-
    /// coherence gate, and the six [`crate::LayoutInvariants::verify`]
    /// kind ↔ code-surface predicates + four error carriers) — a score
    /// of open-coded field-accesses that expressed no compile-time link
    /// back to the typed slot. A future extension of the `:kind` axis
    /// to a richer author surface — a per-`:kind` sub-variant discriminant
    /// (e.g. `Servico(ServicoRuntime)` splitting the current single
    /// variant across the wasm-component / legacy-container / native-
    /// binary runtime axes the M5 roadmap acknowledges), a per-cluster
    /// kind-overlay the M4 CR materializer resolves per-CR (the
    /// "cluster policy demotes `Aplicacao` to `Servico` on a single-
    /// tenant cluster" arm), a promotion of the plain [`CaixaKind`]
    /// enum to a richer `KindWithRuntime` discriminated on the
    /// component-model world axis — would have had to be threaded
    /// through every open-coded copy in lockstep or the entry gate,
    /// the view composers, and the layout invariants would silently
    /// disagree on which kind a given [`Caixa`] resolves to. Lifting
    /// the resolution to a typed method on the substrate primitive
    /// means every downstream consumer of the caixa's per-`Caixa`
    /// kind surface reaches for exactly one typed dispatch — the
    /// resolver's accept-set migrates as a unit on any future axis
    /// addition.
    ///
    /// First outer top-level [`Caixa`] `Copy`-return required-enum-
    /// discriminant accessor — opens the "outer [`Caixa`] `Copy`-return
    /// required-discriminant" projection pattern. Sibling in shape to
    /// the peer per-`:supervisor` [`crate::supervisor::SupervisorSpec::estrategia`]
    /// (eafb619), per-`:placement` [`crate::aplicacao::Placement::estrategia`]
    /// (921fe1b), and per-`:children` [`crate::supervisor::ChildSpec::restart`]
    /// (dfb4a81) `Copy`-return closed-set-enum discriminant accessors
    /// on the sibling nested-spec typed-slot discriminator axes,
    /// extended here to the outer top-level [`Caixa`] universal-axis
    /// surface. Named `kind()` to match the storage field's name;
    /// the accessor's identity maps onto the canonical CAIXA-SDLC §I
    /// vocabulary the slot's docstring already carries.
    #[must_use]
    pub fn kind(&self) -> CaixaKind {
        self.kind
    }

    /// Substrate-canonical per-`Caixa` `:autores` universal-axis
    /// maintainer-name-list slice-accessor every consumer of the top-
    /// level manifest's maintainer axis keys off — returns the author-
    /// declared `:autores` list verbatim as a `&[String]` slice-view over
    /// the same backing buffer the raw `self.autores.as_slice()` field
    /// access borrows from. Empty-list-carrying (`:autores` is a default-
    /// empty axis every `defcaixa` form supplies with an empty `()` when
    /// unset; the [`Self::from_lisp`] derive folds an omitted `:autores`
    /// through `#[serde(default)]` to `Vec::new()`, so a `Caixa` past
    /// parse definitionally carries a `Vec<String>` slot — possibly
    /// empty — and the returned `&[String]` degenerates to an empty
    /// slice on that arm without any silent `None` collapse).
    ///
    /// The `:autores` slot carries the universal-axis maintainer-name
    /// list every kind of caixa emits under (CAIXA-SDLC §I — the author-
    /// facing surface every `defcaixa` form supplies alongside `:nome` /
    /// `:versao` / `:kind`; the substrate-wide contact-carrying axis
    /// every downstream registry-facing artifact emits under) — the
    /// typed slot's `Vec<String>` accept-set (empty-per-entry rejected
    /// through [`ManifestError::AutorEmpty`], non-chart-maintainer-shape
    /// rejected through [`ManifestError::AutorInvalid`], cross-entry
    /// duplicate rejected through [`ManifestError::AutorDuplicate`]) maps
    /// onto every load-bearing downstream consumer the substrate carries
    /// — the [`Self::validate_autores`] universal-axis empty-per-entry +
    /// shape + duplicate gate at caixa-core/src/manifest.rs, the
    /// caixa-helm `build_chart_yaml` `maintainers:` fold at
    /// caixa-helm/src/lib.rs that walks each entry into a `Maintainer {
    /// name, email: None }` record, every future per-`Caixa` registry-
    /// facing renderer the CAIXA-SDLC §I roadmap acknowledges (the
    /// future `artifacthub.io/maintainers` `Chart.yaml` annotation the
    /// caixa-helm docstring alludes to at [`Self::validate_licenca`],
    /// the future per-cluster author-notification overlay the M4 CR
    /// materializer resolves per-CR).
    ///
    /// Prior to this lift the `.autores` field was accessed inline at
    /// two production sites — [`Self::validate_autores`]'s `for autor
    /// in &self.autores` walk that gates every entry through
    /// [`ManifestError::AutorEmpty`] / `AutorInvalid` / `AutorDuplicate`,
    /// and the caixa-helm `build_chart_yaml` `caixa.autores.iter().map(|a|
    /// Maintainer { name: a.clone(), email: None }).collect()` fold that
    /// materializes every entry into a `Chart.yaml` `maintainers:` row —
    /// two open-coded field-accesses that expressed no compile-time link
    /// back to the typed slot. A future extension of the `:autores` axis
    /// to a richer author surface — a per-`:autores` structured
    /// `Maintainer { name, email, url }` at the storage layer once the
    /// substrate absorbs `artifacthub.io/maintainers`' name+email+url
    /// tuple, a per-registry `:autores` allowlist the M4 CR materializer
    /// enforces per-CR (the "cluster policy demands every author declare
    /// an on-file `mailto:` contact" arm), a promotion of the plain
    /// `Vec<String>` byte-string list to a richer
    /// `Vec<ChartMaintainer>` newtype discriminated on the RFC-5322
    /// `<name> [<email>]` grammar the `is_chart_maintainer_name_shape`
    /// predicate already resolves through — would have had to be
    /// threaded through both open-coded copies in lockstep or the
    /// validate gate and the caixa-helm emit path would silently
    /// disagree on which authors a given [`Caixa`] resolves to (an
    /// author's `:autores ("alice" "bob")` would satisfy validate while
    /// the caixa-helm emit path silently rendered a drifted other
    /// maintainer list, or vice versa). Lifting the resolution to a
    /// typed method on the substrate primitive means every downstream
    /// consumer of the caixa's per-`Caixa` maintainer surface reaches
    /// for exactly one typed dispatch — the resolver's accept-set
    /// migrates as a unit on any future axis addition.
    ///
    /// First outer top-level [`Caixa`] `&[T]`-return slice-accessor —
    /// opens the "outer [`Caixa`] `&[T]` slice" projection pattern the
    /// sibling per-`Caixa` `:etiquetas` / `:deps` / `:deps-dev` / `:exe`
    /// / `:bibliotecas` / `:servicos` / `:upgrade-from` / `:children`
    /// future lifts fold on. Sibling in shape to the peer per-`:supervisor`
    /// [`crate::supervisor::SupervisorSpec::children`] (bc92bce), per-`:placement`
    /// [`crate::aplicacao::Placement::clusters`] (a6e18d7), per-`:membros`
    /// [`crate::aplicacao::AplicacaoSpec::membros`] (6c77e36), per-`:contratos`
    /// [`crate::aplicacao::AplicacaoSpec::contratos`] (0dcc926), and
    /// per-`:upgrade-from :instructions` [`crate::upgrade::UpgradeFromEntry::instructions`]
    /// (0137e5a) `&[T]`-return slice accessors on the sibling per-M2 /
    /// per-M3 typed-slot list axes, extended here to the outer top-level
    /// [`Caixa`] universal-axis surface. Returns `&[String]` (not
    /// `&Vec<String>`) because every downstream consumer of the author
    /// list treats it as a read-only sequence — the slice-view is the
    /// narrowest borrow that supports every present + roadmapped consumer
    /// (`.iter()`, `.len()`, `.is_empty()`) without leaking the backing
    /// `Vec`'s grow/push/reserve surface no consumer of the typed view
    /// reaches for (the storage-side `Vec` remains reachable through the
    /// `pub autores` field for the mutation-carrying serde round-trip and
    /// per-test fixture-mutation paths). Named `autores()` to match the
    /// storage field's name; the accessor's identity maps onto the
    /// canonical CAIXA-SDLC §I vocabulary the slot's docstring already
    /// carries.
    #[must_use]
    pub fn autores(&self) -> &[String] {
        self.autores.as_slice()
    }

    /// Substrate-canonical per-`Caixa` `:etiquetas` universal-axis
    /// registry-search-tag-list slice-accessor every consumer of the
    /// top-level manifest's topical-tag axis keys off — returns the
    /// author-declared `:etiquetas` list verbatim as a `&[String]`
    /// slice-view over the same backing buffer the raw
    /// `self.etiquetas.as_slice()` field access borrows from. Empty-
    /// list-carrying (`:etiquetas` is a default-empty axis every
    /// `defcaixa` form supplies with an empty `()` when unset; the
    /// [`Self::from_lisp`] derive folds an omitted `:etiquetas` through
    /// `#[serde(default)]` to `Vec::new()`, so a `Caixa` past parse
    /// definitionally carries a `Vec<String>` slot — possibly empty —
    /// and the returned `&[String]` degenerates to an empty slice on
    /// that arm without any silent `None` collapse).
    ///
    /// The `:etiquetas` slot carries the universal-axis topical-tag
    /// list every kind of caixa emits under (CAIXA-SDLC §I — the
    /// author-facing surface every `defcaixa` form supplies alongside
    /// `:nome` / `:versao` / `:kind`; the substrate-wide registry-
    /// search-facing axis every downstream registry-facing artifact
    /// emits under) — the typed slot's `Vec<String>` accept-set
    /// (empty-per-entry rejected through [`ManifestError::EtiquetaEmpty`],
    /// non-chart-keyword-shape rejected through
    /// [`ManifestError::EtiquetaInvalid`], cross-entry duplicate
    /// rejected through [`ManifestError::EtiquetaDuplicate`]) maps onto
    /// every load-bearing downstream consumer the substrate carries —
    /// the [`Self::validate_etiquetas`] universal-axis empty-per-entry
    /// + shape + duplicate gate at caixa-core/src/manifest.rs, the
    /// caixa-helm `build_chart_yaml` `keywords:` fold at
    /// caixa-helm/src/lib.rs that walks each entry into the rendered
    /// `Chart.yaml` `keywords:` array (chained with the
    /// [`crate::LAREIRA_CHART_KEYWORDS`] substrate-wide floor set and
    /// dedup'd through a `BTreeSet` at emit time), every future per-
    /// `Caixa` registry-facing renderer the CAIXA-SDLC §I roadmap
    /// acknowledges (the future `artifacthub.io/keywords` `Chart.yaml`
    /// annotation, the future per-cluster tag-notification overlay the
    /// M4 CR materializer resolves per-CR).
    ///
    /// Prior to this lift the `.etiquetas` field was accessed inline at
    /// two production sites — [`Self::validate_etiquetas`]'s `for
    /// etiqueta in &self.etiquetas` walk that gates every entry through
    /// [`ManifestError::EtiquetaEmpty`] / `EtiquetaInvalid` /
    /// `EtiquetaDuplicate`, and the caixa-helm `build_chart_yaml`
    /// `caixa.etiquetas.iter().cloned().chain(...)` fold that
    /// materializes every entry into a `Chart.yaml` `keywords:` row —
    /// two open-coded field-accesses that expressed no compile-time
    /// link back to the typed slot. A future extension of the
    /// `:etiquetas` axis to a richer tag surface — a per-`:etiquetas`
    /// structured `ChartKeyword { name, uri, category }` at the storage
    /// layer once the substrate absorbs `artifacthub.io/keywords`
    /// richer tag tuple, a per-registry `:etiquetas` allowlist the M4
    /// CR materializer enforces per-CR (the "cluster policy demands
    /// every tag come from a substrate-approved taxonomy" arm), a
    /// promotion of the plain `Vec<String>` byte-string list to a
    /// richer `Vec<ChartKeyword>` newtype discriminated on the DNS-
    /// 1123-label-shaped grammar the `is_chart_keyword_shape` predicate
    /// already resolves through — would have had to be threaded through
    /// both open-coded copies in lockstep or the validate gate and the
    /// caixa-helm emit path would silently disagree on which tags a
    /// given [`Caixa`] resolves to (an author's `:etiquetas ("demo"
    /// "aplicacao")` would satisfy validate while the caixa-helm emit
    /// path silently rendered a drifted other keyword list, or vice
    /// versa). Lifting the resolution to a typed method on the
    /// substrate primitive means every downstream consumer of the
    /// caixa's per-`Caixa` topical-tag surface reaches for exactly one
    /// typed dispatch — the resolver's accept-set migrates as a unit
    /// on any future axis addition.
    ///
    /// Second outer top-level [`Caixa`] `&[T]`-return slice-accessor —
    /// folds on the "outer [`Caixa`] `&[T]` slice" projection pattern
    /// [`Self::autores`] (b5d813f) opened, sibling in shape and
    /// idiom. The remaining unlifted outer-`Caixa` slice-carrying axes
    /// (`:deps` / `:deps-dev` / `:exe` / `:bibliotecas` / `:servicos`
    /// / `:upgrade-from` / `:children` / `:membros` / `:contratos`)
    /// fold onto the same pattern in future lifts. Sibling in shape to
    /// the peer per-`:supervisor`
    /// [`crate::supervisor::SupervisorSpec::children`] (bc92bce),
    /// per-`:placement` [`crate::aplicacao::Placement::clusters`]
    /// (a6e18d7), per-`:membros`
    /// [`crate::aplicacao::AplicacaoSpec::membros`] (6c77e36),
    /// per-`:contratos` [`crate::aplicacao::AplicacaoSpec::contratos`]
    /// (0dcc926), and per-`:upgrade-from :instructions`
    /// [`crate::upgrade::UpgradeFromEntry::instructions`] (0137e5a)
    /// `&[T]`-return slice accessors on the sibling per-M2 / per-M3
    /// typed-slot list axes, extended here to the outer top-level
    /// [`Caixa`] universal-axis surface. Returns `&[String]` (not
    /// `&Vec<String>`) because every downstream consumer of the tag
    /// list treats it as a read-only sequence — the slice-view is the
    /// narrowest borrow that supports every present + roadmapped
    /// consumer (`.iter()`, `.len()`, `.is_empty()`) without leaking
    /// the backing `Vec`'s grow/push/reserve surface no consumer of
    /// the typed view reaches for (the storage-side `Vec` remains
    /// reachable through the `pub etiquetas` field for the mutation-
    /// carrying serde round-trip and per-test fixture-mutation paths).
    /// Named `etiquetas()` to match the storage field's name; the
    /// accessor's identity maps onto the canonical CAIXA-SDLC §I
    /// vocabulary the slot's docstring already carries.
    #[must_use]
    pub fn etiquetas(&self) -> &[String] {
        self.etiquetas.as_slice()
    }

    /// Substrate-canonical per-`Caixa` `:bibliotecas` universal-axis
    /// library-source-path-list slice-accessor every consumer of the
    /// top-level manifest's Biblioteca-source axis keys off — returns
    /// the author-declared `:bibliotecas` list verbatim as a
    /// `&[String]` slice-view over the same backing buffer the raw
    /// `self.bibliotecas.as_slice()` field access borrows from. Empty-
    /// list-carrying (`:bibliotecas` is a default-empty axis every
    /// `defcaixa` form supplies with an empty `()` when unset; the
    /// [`Self::from_lisp`] derive folds an omitted `:bibliotecas`
    /// through `#[serde(default)]` to `Vec::new()`, so a `Caixa` past
    /// parse definitionally carries a `Vec<String>` slot — possibly
    /// empty — and the returned `&[String]` degenerates to an empty
    /// slice on that arm without any silent `None` collapse).
    ///
    /// The `:bibliotecas` slot carries the universal-axis lisp-library
    /// entry-path list every `:kind Biblioteca` caixa emits under
    /// (CAIXA-SDLC §I — the author-facing surface every `defcaixa`
    /// form supplies alongside `:nome` / `:versao` / `:kind`; the
    /// substrate-wide library-carrier axis every downstream
    /// authoring-facing consumer keys off) — the typed slot's
    /// `Vec<String>` accept-set (empty-per-entry rejected through
    /// [`ManifestError::CodePathEmpty { slot: ":bibliotecas" }`],
    /// non-sandboxed-relative-shape rejected through
    /// [`ManifestError::CodePathShape`], non-`.lisp`-extension rejected
    /// through [`ManifestError::CodePathNonLispExtension`], cross-entry
    /// duplicate rejected through [`ManifestError::CodePathDuplicate`])
    /// maps onto every load-bearing downstream consumer the substrate
    /// carries — the [`crate::LayoutInvariants`] Biblioteca-arm
    /// empty-check + per-entry file-exists loop at
    /// caixa-core/src/layout.rs that gates each entry through
    /// [`crate::LayoutError::MissingLib`] / `MissingEntry`, the
    /// [`Self::validate_code_paths`] per-slot shape gate at
    /// caixa-core/src/manifest.rs that walks each entry through the
    /// sandbox-relative / `.lisp`-extension / cross-entry duplicate
    /// gates, the `feira build` per-entry `tatara_lisp::read` parse
    /// walk at caixa-feira/src/cmd/build.rs that phase-1-checks each
    /// declared library file for lexical / structural errors before
    /// downstream `importar` resolution, every future per-`Caixa`
    /// library-facing renderer the CAIXA-SDLC §I roadmap acknowledges
    /// (the future `tatara-lispc` compilation entry the docstring at
    /// caixa-feira/src/cmd/build.rs alludes to, the future per-cluster
    /// bytecode-caching overlay the M4 CR materializer resolves per-CR,
    /// the future `caixa-lsp` per-library semantic-token stream the
    /// caixa-lsp docstring roadmaps).
    ///
    /// Prior to this lift the `.bibliotecas` field was accessed inline
    /// at three production sites — [`crate::LayoutInvariants`]'s
    /// `caixa.bibliotecas.is_empty()` `MissingLib`-arm gate + `for p
    /// in &caixa.bibliotecas` `MissingEntry` walk that gates each
    /// declared library path through the on-disk-existence check,
    /// the compound-code-path `has_code = !caixa.bibliotecas.is_empty()
    /// || !caixa.exe.is_empty() || !caixa.servicos.is_empty()` OR-fold
    /// on the [`crate::LayoutError::SupervisorOwnsCode`] /
    /// `AplicacaoOwnsCode` kind-coherence gate, and the `feira build`
    /// per-entry `for entry in &caixa.bibliotecas` + `caixa.bibliotecas.
    /// len()` phase-1 tatara-lispc-precursor parse walk — three open-
    /// coded field-accesses that expressed no compile-time link back
    /// to the typed slot. A future extension of the `:bibliotecas`
    /// axis to a richer library surface — a per-`:bibliotecas`
    /// structured `BibliotecaEntry { path, edition, exports }` at the
    /// storage layer once the substrate absorbs the per-library
    /// language-edition + explicit-exports tuple the tatara-lisp
    /// module-system roadmap acknowledges, a per-registry
    /// `:bibliotecas` allowlist the M4 CR materializer enforces
    /// per-CR (the "cluster policy demands every biblioteca declare
    /// its own :edicao" arm), a promotion of the plain `Vec<String>`
    /// byte-string list to a richer `Vec<LibraryPath>` newtype
    /// discriminated on the `lib/<nome>.lisp`-shape grammar the
    /// [`crate::render::is_sandboxed_relative_path`] +
    /// [`crate::render::is_lisp_extension`] predicates already resolve
    /// through — would have had to be threaded through all three
    /// open-coded copies in lockstep or the layout gate, the shape
    /// validator, and the `feira build` phase-1 parse walk would
    /// silently disagree on which library paths a given [`Caixa`]
    /// resolves to (an author's `:bibliotecas ("lib/foo.lisp"
    /// "lib/bar.lisp")` would satisfy layout while `feira build`
    /// silently parsed a drifted other list, or vice versa). Lifting
    /// the resolution to a typed method on the substrate primitive
    /// means every downstream consumer of the caixa's per-`Caixa`
    /// library-source surface reaches for exactly one typed dispatch
    /// — the resolver's accept-set migrates as a unit on any future
    /// axis addition.
    ///
    /// Third outer top-level [`Caixa`] `&[T]`-return slice-accessor —
    /// folds on the "outer [`Caixa`] `&[T]` slice" projection pattern
    /// [`Self::autores`] (b5d813f) opened and [`Self::etiquetas`]
    /// (78c7d3c) folded on, sibling in shape and idiom. The remaining
    /// unlifted outer-`Caixa` slice-carrying axes (`:deps` /
    /// `:deps-dev` / `:exe` / `:servicos` / `:upgrade-from` /
    /// `:children` / `:membros` / `:contratos`) fold onto the same
    /// pattern in future lifts. Sibling in shape to the peer
    /// per-`:supervisor`
    /// [`crate::supervisor::SupervisorSpec::children`] (bc92bce),
    /// per-`:placement` [`crate::aplicacao::Placement::clusters`]
    /// (a6e18d7), per-`:membros`
    /// [`crate::aplicacao::AplicacaoSpec::membros`] (6c77e36),
    /// per-`:contratos` [`crate::aplicacao::AplicacaoSpec::contratos`]
    /// (0dcc926), and per-`:upgrade-from :instructions`
    /// [`crate::upgrade::UpgradeFromEntry::instructions`] (0137e5a)
    /// `&[T]`-return slice accessors on the sibling per-M2 / per-M3
    /// typed-slot list axes, extended here to the outer top-level
    /// [`Caixa`] universal-axis surface. Returns `&[String]` (not
    /// `&Vec<String>`) because every downstream consumer of the
    /// library-source list treats it as a read-only sequence — the
    /// slice-view is the narrowest borrow that supports every
    /// present + roadmapped consumer (`.iter()`, `.len()`,
    /// `.is_empty()`) without leaking the backing `Vec`'s
    /// grow/push/reserve surface no consumer of the typed view
    /// reaches for (the storage-side `Vec` remains reachable through
    /// the `pub bibliotecas` field for the mutation-carrying serde
    /// round-trip and per-test fixture-mutation paths). Named
    /// `bibliotecas()` to match the storage field's name; the
    /// accessor's identity maps onto the canonical CAIXA-SDLC §I
    /// vocabulary the slot's docstring already carries.
    #[must_use]
    pub fn bibliotecas(&self) -> &[String] {
        self.bibliotecas.as_slice()
    }

    /// Substrate-canonical per-`Caixa` `:exe` universal-axis
    /// nix-built-executable-entry-path-list slice-accessor every consumer
    /// of the top-level manifest's Binario-executable axis keys off —
    /// returns the author-declared `:exe` list verbatim as a `&[String]`
    /// slice-view over the same backing buffer the raw
    /// `self.exe.as_slice()` field access borrows from. Empty-list-
    /// carrying (`:exe` is a default-empty axis every `defcaixa` form
    /// supplies with an empty `()` when unset; the [`Self::from_lisp`]
    /// derive folds an omitted `:exe` through `#[serde(default)]` to
    /// `Vec::new()`, so a `Caixa` past parse definitionally carries a
    /// `Vec<String>` slot — possibly empty — and the returned `&[String]`
    /// degenerates to an empty slice on that arm without any silent
    /// `None` collapse).
    ///
    /// The `:exe` slot carries the universal-axis nix-built executable
    /// entry-path list every `:kind Binario` caixa emits under
    /// (CAIXA-SDLC §I — the author-facing surface every `defcaixa`
    /// form supplies alongside `:nome` / `:versao` / `:kind`; the
    /// substrate-wide `exe/`-directory-fenced entry-carrier axis every
    /// downstream flake-build-facing consumer keys off) — the typed
    /// slot's `Vec<String>` accept-set (empty-per-entry rejected
    /// through [`ManifestError::CodePathEmpty { slot: ":exe" }`],
    /// non-sandboxed-relative-shape rejected through
    /// [`ManifestError::CodePathShape`], cross-entry duplicate rejected
    /// through [`ManifestError::CodePathDuplicate`], out-of-`exe/`-
    /// directory paths rejected past the layout's
    /// [`crate::LayoutError::ExeOutsideDir`] `starts_with` fence) maps
    /// onto every load-bearing downstream consumer the substrate carries
    /// — the [`crate::LayoutInvariants`] Binario-arm empty-check +
    /// per-entry file-exists + `exe/`-directory-fence loop at
    /// caixa-core/src/layout.rs that gates each entry through
    /// [`crate::LayoutError::BinarioWithoutExe`] / `MissingEntry` /
    /// `ExeOutsideDir`, the compound `has_code` OR-fold on the
    /// [`crate::LayoutError::SupervisorOwnsCode`] /
    /// [`crate::LayoutError::AplicacaoOwnsCode`] kind-coherence gate
    /// that fences code-surface slots off from the two no-code kinds,
    /// [`Self::declared_foreign_code_slots`]'s `!self.exe.is_empty()`
    /// arm on the [`crate::LayoutError::ForeignCodeSlot`] gate that
    /// fences the `:exe` code surface off from every non-Binario code-
    /// running kind, [`Self::validate_code_paths`]'s per-slot shape gate
    /// that walks each entry through the sandbox-relative / cross-entry
    /// duplicate gates, every future per-`Caixa` executable-facing
    /// renderer the CAIXA-SDLC §I roadmap acknowledges (the future
    /// `caixa-flake` per-Binario `packages.<system>.<nome>` derivation
    /// entry the caixa-flake docstring roadmaps, the future per-cluster
    /// `nix-store` overlay the M4 CR materializer resolves per-CR, the
    /// future `feira nix` per-executable Binario-target emit path).
    ///
    /// Prior to this lift the `.exe` field was accessed inline at three
    /// production sites — the compound-code-path `has_code =
    /// !caixa.bibliotecas().is_empty() || !caixa.exe.is_empty() ||
    /// !caixa.servicos.is_empty()` OR-fold on the
    /// [`crate::LayoutError::SupervisorOwnsCode`] /
    /// `AplicacaoOwnsCode` kind-coherence gate, the Binario-arm
    /// `caixa.exe.is_empty()` [`crate::LayoutError::BinarioWithoutExe`]
    /// gate, the per-entry `for p in &caixa.exe`
    /// `MissingEntry`/`ExeOutsideDir` walk, and the
    /// [`Self::declared_foreign_code_slots`]'s
    /// `!self.exe.is_empty()` arm on the `ForeignCodeSlot` gate — four
    /// open-coded field-accesses that expressed no compile-time link
    /// back to the typed slot. A future extension of the `:exe` axis
    /// to a richer executable surface — a per-`:exe` structured
    /// `BinarioEntry { path, wrapper, capabilities }` at the storage
    /// layer once the substrate absorbs the per-executable
    /// nix-wrapper + linux-capabilities tuple the CAIXA-SDLC §I
    /// executable roadmap acknowledges, a per-registry `:exe` allowlist
    /// the M4 CR materializer enforces per-CR (the "cluster policy
    /// demands every Binario declare an explicit `:wrapper`" arm), a
    /// promotion of the plain `Vec<String>` byte-string list to a
    /// richer `Vec<ExecutablePath>` newtype discriminated on the
    /// `exe/<nome>`-shape grammar the layout's `starts_with(exe_dir)`
    /// fence already resolves through — would have had to be threaded
    /// through all four open-coded copies in lockstep or the layout
    /// gate, the shape validator, and the `feira nix` emit path would
    /// silently disagree on which executable paths a given [`Caixa`]
    /// resolves to (an author's `:exe ("exe/cli" "exe/serve")` would
    /// satisfy layout while `feira nix` silently packaged a drifted
    /// other list, or vice versa). Lifting the resolution to a typed
    /// method on the substrate primitive means every downstream
    /// consumer of the caixa's per-`Caixa` executable-source surface
    /// reaches for exactly one typed dispatch — the resolver's accept-
    /// set migrates as a unit on any future axis addition.
    ///
    /// Fourth outer top-level [`Caixa`] `&[T]`-return slice-accessor —
    /// folds on the "outer [`Caixa`] `&[T]` slice" projection pattern
    /// [`Self::autores`] (b5d813f) opened, [`Self::etiquetas`]
    /// (78c7d3c) folded on, and [`Self::bibliotecas`] (8a36c23) closed
    /// the universal-axis text-tag family of. Opens the outer-`Caixa`
    /// foreign-code-slot `&[T]` sub-family the sibling `:servicos`
    /// future lift closes onto (per the trio of code-surface list slots
    /// the [`Self::validate_code_paths`] per-slot dispatch tuple
    /// already carries — `:bibliotecas` + `:exe` + `:servicos`, of which
    /// `:bibliotecas` landed at 8a36c23 and `:servicos` remains as the
    /// last unlifted code-surface slot). Sibling in shape to the peer
    /// per-`:supervisor` [`crate::supervisor::SupervisorSpec::children`]
    /// (bc92bce), per-`:placement`
    /// [`crate::aplicacao::Placement::clusters`] (a6e18d7),
    /// per-`:membros` [`crate::aplicacao::AplicacaoSpec::membros`]
    /// (6c77e36), per-`:contratos`
    /// [`crate::aplicacao::AplicacaoSpec::contratos`] (0dcc926), and
    /// per-`:upgrade-from :instructions`
    /// [`crate::upgrade::UpgradeFromEntry::instructions`] (0137e5a)
    /// `&[T]`-return slice accessors on the sibling per-M2 / per-M3
    /// typed-slot list axes, extended here to the outer top-level
    /// [`Caixa`] universal-axis surface. Returns `&[String]` (not
    /// `&Vec<String>`) because every downstream consumer of the
    /// executable-source list treats it as a read-only sequence — the
    /// slice-view is the narrowest borrow that supports every
    /// present + roadmapped consumer (`.iter()`, `.len()`,
    /// `.is_empty()`) without leaking the backing `Vec`'s
    /// grow/push/reserve surface no consumer of the typed view
    /// reaches for (the storage-side `Vec` remains reachable through
    /// the `pub exe` field for the mutation-carrying serde
    /// round-trip and per-test fixture-mutation paths). Named `exe()`
    /// to match the storage field's name; the accessor's identity
    /// maps onto the canonical CAIXA-SDLC §I vocabulary the slot's
    /// docstring already carries.
    #[must_use]
    pub fn exe(&self) -> &[String] {
        self.exe.as_slice()
    }

    /// Substrate-canonical per-`Caixa` `:servicos` universal-axis
    /// ComputeUnit-CR-YAML-entry-path-list slice-accessor every consumer
    /// of the top-level manifest's Servico-component axis keys off —
    /// returns the author-declared `:servicos` list verbatim as a
    /// `&[String]` slice-view over the same backing buffer the raw
    /// `self.servicos.as_slice()` field access borrows from. Empty-list-
    /// carrying (`:servicos` is a default-empty axis every `defcaixa`
    /// form supplies with an empty `()` when unset; the
    /// [`Self::from_lisp`] derive folds an omitted `:servicos` through
    /// `#[serde(default)]` to `Vec::new()`, so a `Caixa` past parse
    /// definitionally carries a `Vec<String>` slot — possibly empty —
    /// and the returned `&[String]` degenerates to an empty slice on
    /// that arm without any silent `None` collapse).
    ///
    /// The `:servicos` slot carries the universal-axis
    /// `.computeunit.yaml` ComputeUnit-CR entry-path list every
    /// `:kind Servico` caixa emits under (CAIXA-SDLC §I — the
    /// author-facing surface every `defcaixa` form supplies alongside
    /// `:nome` / `:versao` / `:kind`; the substrate-wide
    /// `servicos/`-directory-fenced entry-carrier axis every downstream
    /// Servico-facing renderer keys off) — the typed slot's
    /// `Vec<String>` accept-set (empty-per-entry rejected through
    /// [`ManifestError::CodePathEmpty { slot: ":servicos" }`],
    /// non-sandboxed-relative-shape rejected through
    /// [`ManifestError::CodePathShape`], non-`.computeunit.yaml`
    /// extension rejected through
    /// [`ManifestError::CodePathNonComputeUnitYamlExtension`], cross-
    /// entry duplicate rejected through
    /// [`ManifestError::CodePathDuplicate`], `len != 1` rejected by the
    /// V0 [`crate::ServicoCountMismatch`] gate on the per-Servico
    /// renderer entry-points, out-of-`servicos/`-directory paths
    /// rejected past the layout's [`crate::LayoutError::ServicoOutsideDir`]
    /// `starts_with` fence) maps onto every load-bearing downstream
    /// consumer the substrate carries — the [`crate::LayoutInvariants`]
    /// Servico-arm empty-check + per-entry file-exists + `servicos/`-
    /// directory-fence loop at caixa-core/src/layout.rs that gates each
    /// entry through [`crate::LayoutError::ServicoWithoutServicos`] /
    /// `MissingEntry` / `ServicoOutsideDir`, the compound `has_code`
    /// OR-fold on the [`crate::LayoutError::SupervisorOwnsCode`] /
    /// [`crate::LayoutError::AplicacaoOwnsCode`] kind-coherence gate
    /// that fences code-surface slots off from the two no-code kinds,
    /// [`Self::declared_foreign_code_slots`]'s
    /// `!self.servicos.is_empty()` arm on the
    /// [`crate::LayoutError::ForeignCodeSlot`] gate that fences the
    /// `:servicos` code surface off from every non-Servico code-running
    /// kind, [`Self::validate_code_paths`]'s per-slot shape gate that
    /// walks each entry through the sandbox-relative / `.computeunit.
    /// yaml`-extension / cross-entry duplicate gates, the
    /// [`crate::require_single_servico`] V0 singularity gate every
    /// per-Servico renderer entry-point runs through
    /// [`crate::require_v0_servico_shape`], the `feira chart` /
    /// `feira deploy` per-verb `first_servico_path` walk at
    /// caixa-feira/src/cmd/chart.rs that resolves the singleton
    /// ComputeUnit-CR file, every future per-`Caixa` Servico-facing
    /// renderer the CAIXA-SDLC §I roadmap acknowledges (the future
    /// per-Servico OCI packager, the future M4
    /// `wasm.pleme.io/v1alpha1/ComputeUnit` CR materializer, the future
    /// per-Servico OTel collector-config emit).
    ///
    /// Prior to this lift the `.servicos` field was accessed inline at
    /// five production sites — the compound-code-path `has_code =
    /// !caixa.bibliotecas().is_empty() || !caixa.exe().is_empty() ||
    /// !caixa.servicos.is_empty()` OR-fold on the
    /// [`crate::LayoutError::SupervisorOwnsCode`] /
    /// `AplicacaoOwnsCode` kind-coherence gate, the Servico-arm
    /// `caixa.servicos.is_empty()`
    /// [`crate::LayoutError::ServicoWithoutServicos`] gate, the
    /// per-entry `for p in &caixa.servicos`
    /// `MissingEntry`/`ServicoOutsideDir` walk, the
    /// [`Self::declared_foreign_code_slots`]'s
    /// `!self.servicos.is_empty()` arm on the `ForeignCodeSlot` gate,
    /// and the [`crate::require_single_servico`] V0 count gate's
    /// `caixa.servicos.len() == 1` / `caixa.servicos.len()` count
    /// projection (both the accept-arm predicate and the
    /// diagnostic-carrying `ServicoCountMismatch { count }`
    /// projection) — five open-coded field-accesses across three
    /// crates that expressed no compile-time link back to the typed
    /// slot. A future extension of the `:servicos` axis to a richer
    /// component surface — a per-`:servicos` structured
    /// `ServicoEntry { path, world, capabilities }` at the storage
    /// layer once the substrate absorbs the per-component WIT-world +
    /// capability-set tuple the CAIXA-SDLC §I Servico roadmap
    /// acknowledges, a per-registry `:servicos` allowlist the M4 CR
    /// materializer enforces per-CR (the "cluster policy demands every
    /// Servico declare an explicit `:world`" arm), a promotion of the
    /// plain `Vec<String>` byte-string list to a richer
    /// `Vec<ComputeUnitPath>` newtype discriminated on the
    /// `servicos/<nome>.computeunit.yaml`-shape grammar the layout's
    /// `starts_with(servicos_dir)` fence and the
    /// [`crate::render::is_computeunit_yaml_extension`] predicate
    /// already resolve through, a promotion of the V0 singleton
    /// contract to a multi-component `Vec<ComputeUnitPath>` past the M5
    /// component-model multi-world boundary — would have had to be
    /// threaded through all five open-coded copies in lockstep or the
    /// layout gate, the shape validator, the V0 count gate, and the
    /// `feira chart` / `feira deploy` entry-point walks would silently
    /// disagree on which ComputeUnit-CR paths a given [`Caixa`]
    /// resolves to (an author's `:servicos ("servicos/foo.computeunit.
    /// yaml")` would satisfy layout while `feira chart` silently
    /// packaged a drifted other list, or vice versa). Lifting the
    /// resolution to a typed method on the substrate primitive means
    /// every downstream consumer of the caixa's per-`Caixa`
    /// ComputeUnit-CR-source surface reaches for exactly one typed
    /// dispatch — the resolver's accept-set migrates as a unit on any
    /// future axis addition.
    ///
    /// Fifth and final outer top-level [`Caixa`] `&[T]`-return slice-
    /// accessor — folds on the "outer [`Caixa`] `&[T]` slice"
    /// projection pattern [`Self::autores`] (b5d813f) opened,
    /// [`Self::etiquetas`] (78c7d3c) folded on, [`Self::bibliotecas`]
    /// (8a36c23) closed the universal-axis text-tag family of, and
    /// [`Self::exe`] (65d9527) opened the foreign-code-slot sub-family
    /// of. Closes the outer-`Caixa` foreign-code-slot `&[T]` sub-family
    /// — with `:bibliotecas`, `:exe`, and `:servicos` now each carrying
    /// a substrate-canonical slice accessor, the trio of code-surface
    /// list slots the [`Self::validate_code_paths`] per-slot dispatch
    /// tuple carries is complete on the typed dispatch surface (the
    /// internal `[(":bibliotecas", &self.bibliotecas, ..), (":exe",
    /// &self.exe, ..), (":servicos", &self.servicos, ..)]` per-slot
    /// dispatch tuple's homogeneous `&Vec<String>`-typed shape blocks a
    /// per-element accessor swap in isolation — a future companion lift
    /// promotes the tuple's element type to `&[String]` and threads the
    /// triple of typed dispatches through as a unit). Sibling in shape
    /// to the peer per-`:supervisor`
    /// [`crate::supervisor::SupervisorSpec::children`] (bc92bce),
    /// per-`:placement` [`crate::aplicacao::Placement::clusters`]
    /// (a6e18d7), per-`:membros`
    /// [`crate::aplicacao::AplicacaoSpec::membros`] (6c77e36),
    /// per-`:contratos`
    /// [`crate::aplicacao::AplicacaoSpec::contratos`] (0dcc926), and
    /// per-`:upgrade-from :instructions`
    /// [`crate::upgrade::UpgradeFromEntry::instructions`] (0137e5a)
    /// `&[T]`-return slice accessors on the sibling per-M2 / per-M3
    /// typed-slot list axes, extended here to the outer top-level
    /// [`Caixa`] universal-axis surface. Returns `&[String]` (not
    /// `&Vec<String>`) because every downstream consumer of the
    /// ComputeUnit-CR-source list treats it as a read-only sequence —
    /// the slice-view is the narrowest borrow that supports every
    /// present + roadmapped consumer (`.iter()`, `.len()`,
    /// `.is_empty()`, `.first()`) without leaking the backing `Vec`'s
    /// grow/push/reserve surface no consumer of the typed view reaches
    /// for (the storage-side `Vec` remains reachable through the
    /// `pub servicos` field for the mutation-carrying serde round-trip
    /// and per-test fixture-mutation paths, and for the
    /// [`Self::validate_code_paths`] per-slot dispatch tuple whose
    /// homogeneous-element-type shape carries the raw field access
    /// until the trio-closure lift promotes the tuple as a unit).
    /// Named `servicos()` to match the storage field's name; the
    /// accessor's identity maps onto the canonical CAIXA-SDLC §I
    /// vocabulary the slot's docstring already carries.
    #[must_use]
    pub fn servicos(&self) -> &[String] {
        self.servicos.as_slice()
    }

    /// Substrate-canonical per-`Caixa` `:deps` universal-axis
    /// runtime-dependency-declaration-list slice-accessor every consumer
    /// of the top-level manifest's runtime-dep-graph axis keys off —
    /// returns the author-declared `:deps` list verbatim as a `&[Dep]`
    /// slice-view over the same backing buffer the raw
    /// `self.deps.as_slice()` field access borrows from. Empty-list-
    /// carrying (`:deps` is a default-empty axis every `defcaixa` form
    /// supplies with an empty `()` when unset; the [`Self::from_lisp`]
    /// derive folds an omitted `:deps` through `#[serde(default)]` to
    /// `Vec::new()`, so a `Caixa` past parse definitionally carries a
    /// `Vec<Dep>` slot — possibly empty — and the returned `&[Dep]`
    /// degenerates to an empty slice on that arm without any silent
    /// `None` collapse).
    ///
    /// The `:deps` slot carries the universal-axis runtime dependency
    /// list every kind of caixa emits under (CAIXA-SDLC §I — the author-
    /// facing surface every `defcaixa` form supplies alongside `:nome` /
    /// `:versao` / `:kind`; the substrate-wide runtime-closure-input axis
    /// every downstream resolver-facing artifact emits under) — the
    /// typed slot's `Vec<Dep>` accept-set (empty-`:nome` rejected through
    /// [`DepError::NomeEmpty`], non-DNS-1123-label `:nome` rejected
    /// through [`DepError::NomeInvalid`], malformed `:versao` rejected
    /// through [`DepError::VersaoInvalid`], empty `:fonte.repo` rejected
    /// through [`DepError::FonteRepoEmpty`], within-list duplicate `:nome`
    /// rejected through [`DepError::DuplicateNome { list: ":deps" }`])
    /// maps onto every load-bearing downstream consumer the substrate
    /// carries — the [`Self::validate_deps`] per-entry
    /// [`Dep::validate`] + within-list dedup walk at
    /// caixa-core/src/manifest.rs, the [`crate::dep::validate_no_self_dep`]
    /// cross-list self-reference gate at caixa-core/src/layout.rs that
    /// checks each entry against the caixa's own `:nome`, the
    /// caixa-resolver `for dep in &root.deps` closure walk at
    /// caixa-resolver/src/resolve.rs that seeds every git-clone target
    /// through the resolver's [`crate::Dep`]-keyed pipeline, the
    /// caixa-crd `caixa.deps.iter().map(dep_into_ref).collect()` fold at
    /// caixa-crd/src/conversion.rs that materializes each entry into the
    /// K8s `Caixa` CR's `spec.deps` field, every future per-`Caixa`
    /// resolver-facing renderer the CAIXA-SDLC §I roadmap acknowledges
    /// (the future per-cluster runtime-closure-audit overlay the M4 CR
    /// materializer resolves per-CR, the future `lacre.lisp` BLAKE3-
    /// closure emit walk the caixa-resolver docstring roadmaps).
    ///
    /// First outer top-level [`Caixa`] `&[Dep]`-return slice-accessor —
    /// opens the outer-`Caixa` dependency-slot `&[Dep]` sub-family the
    /// sibling `:deps-dev` future lift closes on. Peer of the closed
    /// outer-`Caixa` foreign-code-slot `&[String]` sub-family
    /// ([`Self::bibliotecas`] 8a36c23, [`Self::exe`] 65d9527,
    /// [`Self::servicos`] 611f78b) and the outer-`Caixa` universal-axis
    /// text-tag family ([`Self::autores`] b5d813f, [`Self::etiquetas`]
    /// 78c7d3c) — extends the "outer [`Caixa`] `&[T]` slice" projection
    /// pattern onto a novel element-type axis (`Dep` composite vs the
    /// prior sibling family's `String` scalar). Sibling in shape to the
    /// peer per-`:supervisor`
    /// [`crate::supervisor::SupervisorSpec::children`] (bc92bce),
    /// per-`:placement` [`crate::aplicacao::Placement::clusters`]
    /// (a6e18d7), per-`:membros`
    /// [`crate::aplicacao::AplicacaoSpec::membros`] (6c77e36),
    /// per-`:contratos` [`crate::aplicacao::AplicacaoSpec::contratos`]
    /// (0dcc926), and per-`:upgrade-from :instructions`
    /// [`crate::upgrade::UpgradeFromEntry::instructions`] (0137e5a)
    /// `&[T]`-return slice accessors on the sibling per-M2 / per-M3
    /// typed-slot list axes, extended here to the outer top-level
    /// [`Caixa`] universal-axis dep-graph surface. Returns `&[Dep]`
    /// (not `&Vec<Dep>`) because every downstream consumer of the
    /// runtime-dep list treats it as a read-only sequence — the slice-
    /// view is the narrowest borrow that supports every present +
    /// roadmapped consumer (`.iter()`, `.len()`, `.is_empty()`) without
    /// leaking the backing `Vec`'s grow/push/reserve surface no consumer
    /// of the typed view reaches for (the storage-side `Vec` remains
    /// reachable through the `pub deps` field for the mutation-carrying
    /// serde round-trip and per-test fixture-mutation paths). Named
    /// `deps()` to match the storage field's name; the accessor's
    /// identity maps onto the canonical CAIXA-SDLC §I vocabulary the
    /// slot's docstring already carries.
    #[must_use]
    pub fn deps(&self) -> &[Dep] {
        self.deps.as_slice()
    }

    /// Substrate-canonical per-`Caixa` `:deps-dev` universal-axis
    /// development-only-dependency-declaration-list slice-accessor every
    /// consumer of the top-level manifest's dev-dep-graph axis keys off —
    /// returns the author-declared `:deps-dev` list verbatim as a `&[Dep]`
    /// slice-view over the same backing buffer the raw
    /// `self.deps_dev.as_slice()` field access borrows from. Empty-list-
    /// carrying (`:deps-dev` is a default-empty axis every `defcaixa`
    /// form supplies with an empty `()` when unset; the
    /// [`Self::from_lisp`] derive folds an omitted `:deps-dev` through
    /// `#[serde(default)]` to `Vec::new()`, so a `Caixa` past parse
    /// definitionally carries a `Vec<Dep>` slot — possibly empty — and
    /// the returned `&[Dep]` degenerates to an empty slice on that arm
    /// without any silent `None` collapse).
    ///
    /// The `:deps-dev` slot carries the universal-axis dev-only
    /// dependency list every kind of caixa emits under (CAIXA-SDLC §I —
    /// the author-facing sibling of `:deps` that every `defcaixa` form
    /// supplies to declare tests / lint / bench closures the runtime
    /// `:deps` axis does not carry; the substrate-wide dev-closure-input
    /// axis every downstream test-facing artifact emits under, matching
    /// Cargo's `[dev-dependencies]` table's dev-time-only visibility
    /// contract) — the typed slot's `Vec<Dep>` accept-set (empty-`:nome`
    /// rejected through [`DepError::NomeEmpty`], non-DNS-1123-label
    /// `:nome` rejected through [`DepError::NomeInvalid`], malformed
    /// `:versao` rejected through [`DepError::VersaoInvalid`], empty
    /// `:fonte.repo` rejected through [`DepError::FonteRepoEmpty`],
    /// within-list duplicate `:nome` rejected through
    /// [`DepError::DuplicateNome { list: ":deps-dev" }`]) maps onto every
    /// load-bearing downstream consumer the substrate carries — the
    /// [`Self::validate_deps`] per-entry [`Dep::validate`] + within-list
    /// dedup walk at caixa-core/src/manifest.rs, the
    /// [`crate::dep::validate_no_self_dep`] cross-list self-reference
    /// gate at caixa-core/src/layout.rs that checks each entry against
    /// the caixa's own `:nome`, the caixa-resolver
    /// `for dep in &root.deps_dev` closure walk at
    /// caixa-resolver/src/resolve.rs that seeds every dev-only git-clone
    /// target through the resolver's [`crate::Dep`]-keyed pipeline, and
    /// every future per-`Caixa` resolver-facing renderer the CAIXA-SDLC
    /// §I roadmap acknowledges (the future per-cluster dev-closure-audit
    /// overlay the M4 CR materializer resolves per-CR, the future
    /// `lacre.lisp` BLAKE3-closure emit walk the caixa-resolver docstring
    /// roadmaps).
    ///
    /// Second outer top-level [`Caixa`] `&[Dep]`-return slice-accessor —
    /// closes the outer-`Caixa` dependency-slot `&[Dep]` sub-family the
    /// sibling [`Self::deps`] (ad34b4e) opened on. The two accessors
    /// jointly close the two-list dep-graph surface every downstream
    /// resolver-facing consumer keys off (runtime `:deps` +
    /// dev-only `:deps-dev`, the canonical Cargo-shaped dependency-table
    /// pair the [`Self::validate_deps`] gate already walks in canonical
    /// order). Peer of the closed outer-`Caixa` foreign-code-slot
    /// `&[String]` sub-family ([`Self::bibliotecas`] 8a36c23,
    /// [`Self::exe`] 65d9527, [`Self::servicos`] 611f78b) and the outer-
    /// `Caixa` universal-axis text-tag family ([`Self::autores`]
    /// b5d813f, [`Self::etiquetas`] 78c7d3c) — folds the "outer
    /// [`Caixa`] `&[T]` slice" projection pattern onto the sibling
    /// dev-dep composite-element axis (`Dep` composite, matching the
    /// [`Self::deps`] element type). Sibling in shape to the peer
    /// per-`:supervisor` [`crate::supervisor::SupervisorSpec::children`]
    /// (bc92bce), per-`:placement`
    /// [`crate::aplicacao::Placement::clusters`] (a6e18d7),
    /// per-`:membros` [`crate::aplicacao::AplicacaoSpec::membros`]
    /// (6c77e36), per-`:contratos`
    /// [`crate::aplicacao::AplicacaoSpec::contratos`] (0dcc926), and
    /// per-`:upgrade-from :instructions`
    /// [`crate::upgrade::UpgradeFromEntry::instructions`] (0137e5a)
    /// `&[T]`-return slice accessors on the sibling per-M2 / per-M3
    /// typed-slot list axes, folded here to the outer top-level
    /// [`Caixa`] universal-axis dev-dep-graph surface. Returns `&[Dep]`
    /// (not `&Vec<Dep>`) because every downstream consumer of the
    /// dev-dep list treats it as a read-only sequence — the slice-view
    /// is the narrowest borrow that supports every present +
    /// roadmapped consumer (`.iter()`, `.len()`, `.is_empty()`) without
    /// leaking the backing `Vec`'s grow/push/reserve surface no consumer
    /// of the typed view reaches for (the storage-side `Vec` remains
    /// reachable through the `pub deps_dev` field for the mutation-
    /// carrying serde round-trip and per-test fixture-mutation paths).
    /// Named `deps_dev()` to match the storage field's `snake_case` name;
    /// the kebab-case author-surface tag `:deps-dev` is the same axis
    /// after tatara-lisp's kebab↔snake fold and the accessor's identity
    /// maps onto the canonical CAIXA-SDLC §I vocabulary the slot's
    /// docstring already carries.
    #[must_use]
    pub fn deps_dev(&self) -> &[Dep] {
        self.deps_dev.as_slice()
    }

    /// Compose the Aplicacao-related flat slots into a single typed
    /// [`crate::aplicacao::AplicacaoSpec`] for validation +
    /// downstream renderer consumption. Returns `None` when the
    /// caixa isn't a `:kind Aplicacao`.
    #[must_use]
    pub fn aplicacao_view(&self) -> Option<crate::aplicacao::AplicacaoSpec> {
        if self.kind() != CaixaKind::Aplicacao {
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
    ///
    /// Each per-arm kebab-case label is routed through the peer
    /// [`crate::M3_AUTHOR_KEY_MEMBROS`] /
    /// [`crate::M3_AUTHOR_KEY_CONTRATOS`] /
    /// [`crate::M3_AUTHOR_KEY_POLITICAS`] /
    /// [`crate::M3_AUTHOR_KEY_PLACEMENT`] /
    /// [`crate::M3_AUTHOR_KEY_ENTRADA`] consts declared next to the
    /// [`crate::M3_KEY_PLACEMENT`] renderer-side wire-key peer, so both
    /// halves of every M3 top-level mesh slot's dual axis (author-facing
    /// kebab-case label + renderer-side artifact key) route through one
    /// canonical declaration per arm — same discipline the peer
    /// [`crate::M2_AUTHOR_KEY_LIMITS`] / [`crate::M2_AUTHOR_KEY_BEHAVIOR`]
    /// / [`crate::M2_AUTHOR_KEY_UPGRADE_FROM`] top-level M2 slot consts
    /// (f49c8b0) establish on the sibling per-Servico M2 top-level slot
    /// axis, extended here to close the M3 mesh-slot author-facing-label
    /// axis so both altitudes of the typed-slot algebra
    /// (per-Servico M2 + per-Aplicacao M3) share the same
    /// "one canonical byte-string per arm, next to the axis" discipline.
    #[must_use]
    pub fn declared_mesh_slots(&self) -> Vec<&'static str> {
        let mut slots = Vec::new();
        if !self.membros.is_empty() {
            slots.push(crate::render::M3_AUTHOR_KEY_MEMBROS);
        }
        if !self.contratos.is_empty() {
            slots.push(crate::render::M3_AUTHOR_KEY_CONTRATOS);
        }
        if self.politicas.is_some() {
            slots.push(crate::render::M3_AUTHOR_KEY_POLITICAS);
        }
        if self.placement.is_some() {
            slots.push(crate::render::M3_AUTHOR_KEY_PLACEMENT);
        }
        if self.entrada.is_some() {
            slots.push(crate::render::M3_AUTHOR_KEY_ENTRADA);
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
            slots.push(crate::render::SUPERVISOR_AUTHOR_KEY_ESTRATEGIA);
        }
        if self.max_restarts.is_some() {
            slots.push(crate::render::SUPERVISOR_AUTHOR_KEY_MAX_RESTARTS);
        }
        if self.restart_window.is_some() {
            slots.push(crate::render::SUPERVISOR_AUTHOR_KEY_RESTART_WINDOW);
        }
        if !self.children.is_empty() {
            slots.push(crate::render::SUPERVISOR_AUTHOR_KEY_CHILDREN);
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
    ///
    /// Each per-arm kebab-case label is routed through the peer
    /// [`crate::M2_AUTHOR_KEY_LIMITS`] / [`crate::M2_AUTHOR_KEY_BEHAVIOR`] /
    /// [`crate::M2_AUTHOR_KEY_UPGRADE_FROM`] consts declared next to the
    /// [`crate::M2_KEY_LIMITS`] / [`crate::M2_KEY_BEHAVIOR`] /
    /// [`crate::M2_KEY_UPGRADE_FROM`] renderer-side wire-key peers, so
    /// both halves of the M2 top-level slot's dual axis (author-facing
    /// kebab-case label + renderer-side camelCase overlay-container wire
    /// key) route through one canonical declaration per arm — same
    /// discipline the peer [`crate::M2_BEHAVIOR_AUTHOR_KEY_ON_*`] sub-slot
    /// author-label consts (889dc18) establish on the sibling
    /// per-callback axis inside the `:behavior` overlay block.
    #[must_use]
    pub fn declared_servico_slots(&self) -> Vec<&'static str> {
        let mut slots = Vec::new();
        if self.limits.is_some() {
            slots.push(crate::render::M2_AUTHOR_KEY_LIMITS);
        }
        if self.behavior.is_some() {
            slots.push(crate::render::M2_AUTHOR_KEY_BEHAVIOR);
        }
        if !self.upgrade_from.is_empty() {
            slots.push(crate::render::M2_AUTHOR_KEY_UPGRADE_FROM);
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
        if !self.exe().is_empty() && !self.kind().requires_exe() {
            slots.push(":exe");
        }
        if !self.servicos().is_empty() && !self.kind().requires_servicos() {
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
        for dep in self.deps() {
            dep.validate()?;
            crate::render::insert_first_seen(&mut seen, dep.nome.as_str(), || {
                DepError::DuplicateNome {
                    nome: dep.nome.clone(),
                    list: crate::render::DEP_AUTHOR_KEY_DEPS,
                }
            })?;
        }
        let mut seen_dev = std::collections::HashSet::new();
        for dep in self.deps_dev() {
            dep.validate()?;
            crate::render::insert_first_seen(&mut seen_dev, dep.nome.as_str(), || {
                DepError::DuplicateNome {
                    nome: dep.nome.clone(),
                    list: crate::render::DEP_AUTHOR_KEY_DEPS_DEV,
                }
            })?;
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
        // Routes through the shared
        // [`crate::render::require_valid_dns_1123_label`] gate the peer
        // name axes each land on so drift between the eight axes'
        // accepted DNS-1123-label sets is structurally impossible.
        let nome = self.nome();
        crate::render::require_valid_dns_1123_label(
            nome,
            || ManifestError::NomeEmpty,
            |reason| ManifestError::NomeInvalid {
                nome: nome.to_string(),
                reason,
            },
        )
    }

    /// Reject `:nome` values whose joint length with the canonical
    /// [`crate::LAREIRA_CHART_NAME_PREFIX`] (`"lareira-"`) overflows
    /// the K8s DNS-1123 label cap [`crate::DNS_1123_LABEL_MAX_LEN`]
    /// (63 bytes). Every per-Servico / per-Aplicacao renderer the
    /// substrate carries materializes the caixa's `:nome` through the
    /// canonical [`crate::lareira_chart_name`] helper (f7320d7) into a
    /// `lareira-<nome>` artifact that lands as a K8s `metadata.name` /
    /// Helm chart name / `HelmRelease` `release_name`: `caixa-helm`'s
    /// `ChartDir.name` + `Chart.yaml::name`
    /// (caixa-helm/src/lib.rs:207), `caixa-flux`'s `cluster_bundle`
    /// `HelmRelease` `chart:` slot (caixa-flux/src/lib.rs:329),
    /// `caixa-tatara`'s `process_for_aplicacao` `release_name` +
    /// `oci://<registry>/lareira-<nome>` chart ref
    /// (caixa-tatara/src/lib.rs:124,178). Helm's own `Chart.yaml::name`
    /// admission rule strict-parses against DNS-1123-label, the Helm
    /// operator's tracking-secret name is derived from `release_name`
    /// and is itself DNS-1123-label-bounded, and the rendered chart's
    /// K8s object `metadata.name` axes embed the chart name as a
    /// prefix — every one fails admission on a > 63-byte chart name.
    ///
    /// The per-axis [`Self::validate_nome`] gate (6c992f8) already
    /// caps `:nome` itself at 63 bytes via [`is_dns_1123_label`], so a
    /// `:nome` of 56–63 bytes silently passed validate (the inner
    /// DNS-1123 check accepts the bare `:nome`) but produced a
    /// `lareira-<nome>` of 64–71 bytes that the apiserver / `helm lint`
    /// rejected at admission — far from the source `caixa.lisp`, with
    /// no field naming the overflow root cause. The
    /// [`lareira_chart_name`] helper's own doc comment
    /// (caixa-core/src/render.rs:3198) explicitly deferred the fix:
    /// "the M4 admission webhook will pin the joint-length invariant
    /// when it lands". This gate lands the invariant at the
    /// manifest-validate layer rather than waiting for the apiserver
    /// — the same fail-at-the-source posture every peer per-axis
    /// value-shape gate (DNS-1123 on `:nome`, SemVer-2 on `:versao`,
    /// SPDX-expression-shape on `:licenca`, 4-digit decimal year on
    /// `:edicao`, etc.) takes.
    ///
    /// Thin wrapper around
    /// [`crate::render::is_lareira_chart_name_shape`] (the
    /// substrate-side predicate that composes [`lareira_chart_name`] +
    /// [`is_dns_1123_label`] via the lifted
    /// [`crate::LAREIRA_CHART_NAME_NOME_MAX_LEN`] budget); maps the
    /// shared parser-shaped reason into the
    /// [`ManifestError::NomeChartNameBudgetExceeded`] variant so the
    /// diagnostic is self-locating (the offending `:nome` is named
    /// verbatim alongside the rendered chart name and the budget) and
    /// the author can shorten in one edit. The gate runs across every
    /// `:kind` — `:nome` is the substrate-wide identity axis any
    /// future renderer the substrate adds can derive a
    /// `lareira-<nome>` artifact from, and uniform enforcement closes
    /// the drift footgun where a future kind grows a chart-emitting
    /// render path while the validate cascade doesn't catch it.
    ///
    /// Runs *after* [`Self::validate_nome`] so the narrower
    /// `NomeEmpty` / `NomeInvalid` shape diagnostics fire first — a
    /// structurally-malformed `:nome` (empty, uppercase, underscore,
    /// dot, leading/trailing hyphen, Unicode, > 63 bytes) surfaces its
    /// specific shape error rather than the chart-name-budget error,
    /// preserving the legitimate "well-shaped `:nome` that happens to
    /// overflow the joint cap" arm for this gate.
    pub fn validate_nome_chart_name_budget(&self) -> Result<(), ManifestError> {
        let nome = self.nome();
        crate::render::is_lareira_chart_name_shape(nome).map_err(|reason| {
            ManifestError::NomeChartNameBudgetExceeded {
                nome: nome.to_string(),
                reason,
            }
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
    /// (6c992f8) and [`crate::UpgradeError::FromInvalid`]
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
        let versao = self.versao();
        if versao.is_empty() {
            return Err(ManifestError::VersaoEmpty);
        }
        semver::Version::parse(versao).map_err(|e| ManifestError::VersaoInvalid {
            versao: versao.to_string(),
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
        /// Per-slot file-type contract for the three Caixa-level
        /// code-path surfaces (`:bibliotecas`, `:exe`, `:servicos`).
        /// Each variant names the predicate the per-entry file-type
        /// gate consults; [`Self::None`] opts the slot out of any
        /// file-type contract. Lifted as a typed local enum so the
        /// per-slot dispatch is exhaustive at the `match` — adding a
        /// future axis to the typed-substrate `:` slot set (the
        /// future `:assets` resource axis the M5 roadmap names, the
        /// future `:nix-flake` derivation axis the caixa-flake
        /// emitter consults) lands as one variant + one `match` arm,
        /// not a coordinated rewrite of every per-slot bool flag.
        ///
        /// Peer of the typed-substrate per-slot variant disciplines
        /// already established on this surface
        /// ([`crate::supervisor::RestartStrategy`] +
        /// [`crate::supervisor::RestartPolicy`] on the OTP-shape
        /// supervision-tree axis,
        /// [`crate::aplicacao::PlacementStrategy`] on the §III.1
        /// placement axis, [`crate::aplicacao::WitTarget`] on the
        /// `:contratos` payload-target axis): the typed `enum` is
        /// the substrate's single source of truth for the per-axis
        /// dispatch, and every consumer (the per-arm body here, the
        /// future feira-lint per-slot diagnostic renderer, the M4
        /// per-axis admission webhook) reaches for the same typed
        /// surface rather than re-deriving the partition from inline
        /// flag combinations.
        enum CodePathFileType {
            /// `:exe` — nix-build derivation output, no terminating-
            /// extension contract (the canonical `"exe/<name>"`
            /// fixtures the layout's `ExeOutsideDir` error message
            /// documents carry no extension by convention).
            None,
            /// `:bibliotecas` — tatara-lisp source files the
            /// `feira build` loop reads through `tatara_lisp::read`
            /// at parse time. Routes to [`is_lisp_extension`].
            LispSource,
            /// `:servicos` — ComputeUnit-CR YAML files the
            /// caixa-helm / caixa-flux renderers consume through
            /// `serde_yaml::from_str`. Routes to
            /// [`is_computeunit_yaml_extension`].
            ComputeUnitYaml,
        }

        // The per-slot [`CodePathFileType`] selects which axes carry the
        // lifted file-type predicate. `:bibliotecas` is the tatara-lisp
        // source axis (the `feira build` loop at
        // `caixa-feira/src/cmd/build.rs:33` reads each entry through
        // `tatara_lisp::read` at parse time) — the lifted
        // [`is_lisp_extension`] predicate gates the `.lisp` extension.
        // `:exe` is the nix-built executable surface (per the canonical
        // `"exe/<name>"`-shaped fixtures the layout's `ExeOutsideDir`
        // error message documents and every in-tree
        // `caixa_with_code_paths` positive control uses) — its file-type
        // contract is "nix-build derivation output", not a typed source
        // file, so [`CodePathFileType::None`] opts the slot out of any
        // file-type gate. `:servicos` is the `.computeunit.yaml`
        // ComputeUnit-CR axis (the peer caixa-helm / caixa-flux
        // renderers consume each entry through `serde_yaml::from_str` as
        // a typed `ComputeUnit` CR) — the lifted
        // [`is_computeunit_yaml_extension`] predicate gates the compound
        // `.computeunit.yaml` suffix. All three axes are surfaced through
        // the same iteration so the sandbox-shape + duplicate gates
        // apply uniformly; the typed file-type dispatch fires per-slot
        // exactly where the downstream consumer's accepted set demands
        // it. The third file-type variant ([`ComputeUnitYaml`]) is the
        // compounding lift on the peer 64772a9 `:bibliotecas`
        // `.lisp`-gate trajectory — the second of the three code-path
        // axes to land on a typed compound-suffix gate, with the same
        // self-locating per-slot diagnostic shape every peer per-axis
        // file-type lift uses (`*NonLispExtension { slot, path }` /
        // `*NonComputeUnitYamlExtension { slot, path }`).
        for (slot, list, file_type) in [
            (
                ":bibliotecas",
                &self.bibliotecas,
                CodePathFileType::LispSource,
            ),
            (":exe", &self.exe, CodePathFileType::None),
            (
                ":servicos",
                &self.servicos,
                CodePathFileType::ComputeUnitYaml,
            ),
        ] {
            // Per-slot set-not-multiset gate on the typed code-path axis.
            // Every peer Vec-shaped author-supplied list past validate is
            // a set, not a multiset: `:membros :caixa`
            // ([`crate::AplicacaoError::MembroDuplicate`]), `:placement
            // :clusters` ([`crate::AplicacaoError::PlacementClusterDuplicate`]),
            // `:entrada :paths` ([`crate::AplicacaoError::EntradaPathDuplicate`]),
            // `:contratos` ([`crate::AplicacaoError::ContratoDuplicate`]),
            // `:children :caixa` ([`crate::SupervisorError::DuplicateChild`]),
            // `:deps` / `:deps-dev` `:nome` ([`crate::DepError::DuplicateNome`]
            // per 359fba5), `:upgrade-from :from` ([`crate::UpgradeError::DuplicateFrom`]),
            // `:etiquetas` ([`ManifestError::EtiquetaDuplicate`] per 360a499),
            // `:autores` ([`ManifestError::AutorDuplicate`] per 86c769b) —
            // the three code-path lists are the last Vec-shaped author-
            // supplied slots on the typed Caixa surface still admitting a
            // duplicate entry silently. Scope is per-list (`:bibliotecas`
            // duplicates are flagged within `:bibliotecas`, not across
            // `:bibliotecas` ↔ `:exe`) — the same per-list scope `:deps`
            // ↔ `:deps-dev` use (a `:nome` present in both lists is a
            // legitimate dev-vs-runtime shape on the dep axis, fenced
            // separately by [`crate::dep::validate_no_self_dep`]). On the
            // code-path axis a cross-slot collision is structurally
            // impossible by the layout's `starts_with(<exe|servicos>_dir)`
            // fence — `:exe` and `:servicos` entries are confined to their
            // own directory trees, so the only way a string could appear
            // on two code-path lists is the (rare, structurally invalid)
            // case where `:bibliotecas` carries an `"exe/<x>"` or
            // `"servicos/<x>.yaml"`-shaped path.
            //
            // Without the gate three authoring footguns silently passed:
            //
            //   - `:bibliotecas ("lib/foo.lisp" "lib/foo.lisp")` — the
            //     canonical copy-paste-the-wrong-file footgun. `feira
            //     build` (`caixa-feira/src/cmd/build.rs:33`) walks the
            //     list and re-parses the same file twice, wasting work
            //     and silently masking the author's intent to declare a
            //     *second* biblioteca.
            //   - `:exe ("exe/cli" "exe/cli")` — the same footgun on the
            //     Binario surface. The future `caixa-flake` `nix flake`
            //     emitter that materializes each `:exe` entry as a flake
            //     `packages.<exe-name>` derivation would collide on the
            //     duplicate package name and surface a flake-eval error
            //     far from the source `caixa.lisp`.
            //   - `:servicos ("servicos/x.computeunit.yaml"
            //     "servicos/x.computeunit.yaml")` — the same footgun on
            //     the Servico surface. The peer `caixa-helm` / `caixa-flux`
            //     renderers already refuse `:servicos.len() != 1` with
            //     the narrower [`UnsupportedServicoCount`] diagnostic, but
            //     that diagnostic surfaces "too many servicos" without
            //     naming "duplicate entry" — the typed self-locating
            //     "which entry is the duplicate" framing only lands at
            //     this gate.
            //
            // Same `seen.insert(entry.as_str())` shape every peer per-list
            // duplicate gate uses (`:etiquetas` 360a499, `:autores`
            // 86c769b, `:deps` 359fba5) and the same "structural shape
            // checks fire before the duplicate check on the same entry"
            // ordering (a `(:bibliotecas ("" "lib/x.lisp" "lib/x.lisp"))`
            // shape surfaces the narrower [`Self::CodePathEmpty`] for the
            // empty entry first, not the duplicate on the later pair).
            let mut seen = std::collections::HashSet::new();
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
                // The per-slot file-type gate dispatched through the
                // typed [`CodePathFileType`] selector above. Each variant
                // routes to the lifted predicate the downstream consumer
                // demands:
                //
                //   - [`LispSource`] → [`is_lisp_extension`] for
                //     `:bibliotecas` (the `feira build` loop's
                //     `tatara_lisp::read` consumer);
                //   - [`ComputeUnitYaml`] → [`is_computeunit_yaml_extension`]
                //     for `:servicos` (the caixa-helm / caixa-flux
                //     `serde_yaml::from_str` consumer's `ComputeUnit` CR
                //     accepted set);
                //   - [`None`] for `:exe` — the nix-build derivation-
                //     output axis has no terminating-extension contract.
                //
                // Fires after the sandbox-shape arms so a path that is
                // *both* sandbox-escaping and wrong-extension surfaces
                // the more fundamental sandbox-shape diagnostic first
                // (mirrors the peer `EmptyPath` → `AbsolutePath` →
                // `ParentEscape` → `NonLispExtension` arm-ordering on
                // `:behavior :on-*` c97815a, and `EmptyScript` →
                // `AbsoluteScript` → `ParentEscapeScript` →
                // `NonLispExtensionScript` on
                // `:upgrade-from :state-change :script` 33cc830), and
                // before the duplicate gate so the narrower per-entry
                // file-type shape dominates the cross-entry uniqueness
                // diagnostic (a
                // `("servicos/x.yaml" "servicos/x.yaml")` shape on
                // `:servicos` surfaces
                // `CodePathNonComputeUnitYamlExtension` on the first
                // entry rather than `CodePathDuplicate` on the pair —
                // peer with the 64772a9 `:bibliotecas`
                // `("lib/x.txt" "lib/x.txt")` ordering).
                match file_type {
                    CodePathFileType::None => {}
                    CodePathFileType::LispSource => {
                        if !is_lisp_extension(path) {
                            return Err(ManifestError::CodePathNonLispExtension {
                                slot,
                                path: path.to_path_buf(),
                            });
                        }
                    }
                    CodePathFileType::ComputeUnitYaml => {
                        if !is_computeunit_yaml_extension(path) {
                            return Err(ManifestError::CodePathNonComputeUnitYamlExtension {
                                slot,
                                path: path.to_path_buf(),
                            });
                        }
                    }
                }
                crate::render::insert_first_seen(&mut seen, entry.as_str(), || {
                    ManifestError::CodePathDuplicate {
                        slot,
                        path: path.to_path_buf(),
                    }
                })?;
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
    /// Past the empty arm the gate enforces the chart-keyword shape
    /// predicate via [`crate::render::is_chart_keyword_shape`]: Cargo's
    /// crates.io `[package] keywords` grammar — 1..=20 bytes, starts
    /// with an ASCII letter, ASCII alphanumeric / `_` / `-`
    /// continuation. Closes the canonical paste-from-doc footguns the
    /// bare empty + duplicate arms left open: paste-from-aligned-doc
    /// whitespace (`" mesh"`, `"mesh "`), paste-from-multiline-doc
    /// newline (`"mesh\nhttp"` — the author pasted a multi-tag block
    /// into one entry instead of splitting), paste-from-Windows-CRLF-doc
    /// carriage return, CSV-list-separator confusion (`"mesh,http,grpc"`
    /// — the author meant three separate list entries), path-separator
    /// confusion (`"caixa/servico"`), namespace-suffix (`"http.1"`),
    /// leading-digit (`"1foo"`), kebab-leak (`"-foo"`), snake-leak
    /// (`"_foo"`), non-ASCII (`"café"`), and paste-from-binary-blob
    /// control bytes that would silently land as malformed search tags
    /// in the rendered Chart.yaml `keywords:` array and break the
    /// Artifact Hub keyword index lookup far from the source caixa.lisp.
    /// Mirrors the [`Self::validate_autores`] shape-predicate cascade
    /// established on the sibling universal-axis `Vec<String>` surface
    /// — the second universal-axis Vec<String> surface to land the
    /// empty-first-then-shape-then-duplicate per-entry cascade.
    ///
    /// Same empty-first cascade discipline every peer per-axis gate
    /// uses: the per-entry empty arm fires before the per-entry shape
    /// arm fires before the cross-entry duplicate arm, so an
    /// `("" "mesh" "mesh")` authoring shape surfaces the narrower
    /// [`ManifestError::EtiquetaEmpty`] (the structural "this entry
    /// has no value" defect) before either the shape or the duplicate
    /// diagnostic. Walks the list in declaration order so the
    /// first-collision diagnostic surfaces the lexicographically-
    /// earliest offending position, peer with every other duplicate
    /// gate on this surface.
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
    /// chart-keyword-shaped string without re-deriving the precondition.
    pub fn validate_etiquetas(&self) -> Result<(), ManifestError> {
        let mut seen = std::collections::HashSet::new();
        for etiqueta in self.etiquetas() {
            if etiqueta.is_empty() {
                return Err(ManifestError::EtiquetaEmpty);
            }
            crate::render::is_chart_keyword_shape(etiqueta).map_err(|reason| {
                ManifestError::EtiquetaInvalid {
                    etiqueta: etiqueta.clone(),
                    reason,
                }
            })?;
            crate::render::insert_first_seen(&mut seen, etiqueta.as_str(), || {
                ManifestError::EtiquetaDuplicate {
                    etiqueta: etiqueta.clone(),
                }
            })?;
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
    /// Past the empty arm the gate enforces the chart-maintainer-name
    /// shape predicate via [`crate::render::is_chart_maintainer_name_shape`]:
    /// the structural single-line printable-UTF-8 floor every realistic
    /// Helm chart maintainer name carries — 1..=128 bytes, no leading
    /// or trailing whitespace, no ASCII control characters anywhere,
    /// Unicode bytes accepted. Closes the canonical paste-from-doc
    /// footguns the bare empty + duplicate arms left open:
    /// paste-from-aligned-doc whitespace (`" pleme-io"`, `"pleme-io "`),
    /// paste-from-multiline-doc newline (`"alice\nbob"` — the author
    /// pasted a multi-line block of author records into one `:autores`
    /// entry instead of splitting into one entry per author),
    /// paste-from-Windows-CRLF-doc carriage return, tab-from-aligned-doc,
    /// and the paste-from-binary-blob control bytes that would silently
    /// land as YAML-illegal byte sequences in the rendered Chart.yaml
    /// `maintainers:` array. Mirrors the shape-predicate cascade
    /// [`Self::validate_descricao`] / [`Self::validate_licenca`] /
    /// [`Self::validate_edicao`] / [`Self::validate_repositorio`]
    /// establish past their own empty arms on the sibling universal-axis
    /// `Option<String>` surfaces — the first universal-axis Vec<String>
    /// surface to land the empty-first-then-shape-then-duplicate per-entry
    /// cascade.
    ///
    /// Same empty-first cascade discipline every peer per-axis gate
    /// uses: the per-entry empty arm fires before the per-entry shape
    /// arm before the cross-entry duplicate arm. Walks the list in
    /// declaration order so the first-collision diagnostic surfaces the
    /// lexicographically-earliest offending position, peer with every
    /// other duplicate gate on this surface.
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
        for autor in self.autores() {
            if autor.is_empty() {
                return Err(ManifestError::AutorEmpty);
            }
            crate::render::is_chart_maintainer_name_shape(autor).map_err(|reason| {
                ManifestError::AutorInvalid {
                    autor: autor.clone(),
                    reason,
                }
            })?;
            crate::render::insert_first_seen(&mut seen, autor.as_str(), || {
                ManifestError::AutorDuplicate {
                    autor: autor.clone(),
                }
            })?;
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
        let Some(s) = self.repositorio() else {
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
    ///
    /// Past the empty arm the gate enforces the chart-description
    /// shape predicate via [`crate::render::is_chart_description_shape`]:
    /// the structural single-line UTF-8 floor every realistic chart
    /// description in the wild matches — 1..=512 bytes, no leading
    /// or trailing whitespace, no ASCII control characters anywhere
    /// (`0x00..=0x1F` plus `0x7F` DEL — banning tab, newline,
    /// carriage return, and every other control byte), Unicode
    /// continuation bytes accepted (the canonical fixtures carry
    /// `→` and `—`). Closes the canonical paste-from-doc footguns
    /// the bare empty-arm gate left open: paste-from-aligned-doc
    /// leading / trailing whitespace (`" Checkout flow."`,
    /// `"Checkout flow. "`), paste-from-multiline-doc newline
    /// (`"Checkout\nflow."`), paste-from-Windows-CRLF-doc CR
    /// (`"Checkout\rflow."`), tab-from-aligned-doc
    /// (`"Checkout\tflow."`), and paste-from-binary-blob NUL / BEL /
    /// ESC / DEL bytes. Mirrors the shape-predicate cascade
    /// [`Self::validate_repositorio`] / [`Self::validate_licenca`] /
    /// [`Self::validate_edicao`] establish past their own empty arms
    /// on the sibling universal-axis `Option<String>` Caixa-level
    /// value-shape surfaces.
    ///
    /// The empty-first cascade discipline mirrors every peer per-axis
    /// identity gate: [`ManifestError::DescricaoEmpty`] runs before
    /// [`ManifestError::DescricaoInvalid`], so the narrower empty
    /// diagnostic surfaces on `Some("")` rather than the broader
    /// shape-predicate diagnostic — peer with how
    /// [`ManifestError::LicencaEmpty`] runs before
    /// [`ManifestError::LicencaInvalid`],
    /// [`ManifestError::EdicaoEmpty`] runs before
    /// [`ManifestError::EdicaoInvalid`],
    /// [`ManifestError::RepositorioEmpty`] runs before
    /// [`ManifestError::RepositorioInvalid`].
    pub fn validate_descricao(&self) -> Result<(), ManifestError> {
        let Some(s) = self.descricao() else {
            return Ok(());
        };
        if s.is_empty() {
            return Err(ManifestError::DescricaoEmpty);
        }
        crate::render::is_chart_description_shape(s).map_err(|reason| {
            ManifestError::DescricaoInvalid {
                descricao: s.to_string(),
                reason,
            }
        })?;
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
        let Some(s) = self.licenca() else {
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
        let Some(s) = self.edicao() else {
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
        if self.kind() != CaixaKind::Supervisor {
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
        ":nome {nome:?} overflows the joint-length budget on the canonical \
         `lareira-<nome>` chart-name shape: {reason} (every per-Servico / \
         per-Aplicacao renderer the substrate carries — `caixa-helm`'s \
         `Chart.yaml::name`, `caixa-flux`'s `cluster_bundle` HelmRelease \
         `chart:` slot, `caixa-tatara`'s `release_name` + \
         `oci://<registry>/lareira-<nome>` chart ref — derives the same \
         joint name through the canonical `lareira_chart_name` helper, and \
         Helm's `Chart.yaml::name` admission rule + the K8s apiserver's \
         DNS-1123 label cap on every chart-name-derived `metadata.name` \
         reject any joint name exceeding 63 bytes; the narrower \
         `:nome` shape (`NomeInvalid`) gates the bare-`:nome` budget, this \
         arm gates the chart-name budget downstream renderers inherit)"
    )]
    NomeChartNameBudgetExceeded { nome: String, reason: String },
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
        "{slot} entry {} does not terminate in the `.lisp` extension — every \
         `:bibliotecas` entry is a tatara-lisp source file the `feira build` \
         loop reads through `tatara_lisp::read` at parse time, so any other \
         extension (`.rs`, `.txt`, `.lisp.bak`) or no-extension shape is \
         structurally a parser error far from the source caixa.lisp, with \
         no field naming the offending `:bibliotecas` entry. Pin a relative \
         path under the caixa root whose terminating extension is \
         lowercase-`.lisp` (e.g. `\"lib/<name>.lisp\"`, \
         `\"lib/handlers.lisp\"`) — the same file-type contract the peer \
         `:behavior :on-*` (c97815a) and `:upgrade-from :state-change :script` \
         (33cc830) axes already carry through the same lifted \
         `is_lisp_extension` predicate",
        path.display()
    )]
    CodePathNonLispExtension { slot: &'static str, path: PathBuf },
    #[error(
        "{slot} entry {} does not terminate in the `.computeunit.yaml` \
         compound suffix — every `:servicos` entry is a typed `ComputeUnit` \
         CR YAML file the peer caixa-helm / caixa-flux renderers consume \
         through `serde_yaml::from_str` at chart / FluxCD bundle render \
         time, so any other extension (`.yaml`, `.yml`, `.json`, the \
         off-by-one-segment `.computeunit-yaml`, the editor-backup \
         `.computeunit.yaml.bak`) or no-extension shape is structurally a \
         YAML-parser error / `ComputeUnit` schema-mismatch far from the \
         source caixa.lisp, with no field naming the offending `:servicos` \
         entry. Pin a relative path under the caixa root whose terminating \
         compound suffix is lowercase-`.computeunit.yaml` (e.g. \
         `\"servicos/<name>.computeunit.yaml\"`, \
         `\"servicos/hello-rio.computeunit.yaml\"`) — the same file-type \
         contract the sibling `:bibliotecas` axis (64772a9) already carries \
         on the tatara-lisp-source axis through the peer lifted \
         `is_lisp_extension` predicate, here on the compound-suffix axis \
         `Path::extension` can't express on its own through the lifted \
         `is_computeunit_yaml_extension` predicate",
        path.display()
    )]
    CodePathNonComputeUnitYamlExtension { slot: &'static str, path: PathBuf },
    #[error(
        "{slot} entry {} appears more than once (the code-path list is \
         a set, not a multiset; every peer Vec-shaped author-supplied \
         list past validate is set-not-multiset — `:membros :caixa`, \
         `:placement :clusters`, `:entrada :paths`, `:contratos`, \
         `:children :caixa`, `:deps` / `:deps-dev` `:nome`, \
         `:upgrade-from :from`, `:etiquetas`, `:autores` — and the three \
         code-path lists are the last Vec-shaped author-supplied slots on \
         the typed Caixa surface still admitting a duplicate entry. \
         `:bibliotecas` duplicates re-parse the same file at \
         `feira build` time and silently mask the author's intent to \
         declare a *second* biblioteca; `:exe` duplicates collide on the \
         flake `packages.<name>` derivation key at the future \
         `caixa-flake` materializer; `:servicos` duplicates surface as the \
         narrower [`caixa-helm`] / [`caixa-flux`] `UnsupportedServicoCount` \
         rejection far from the source `caixa.lisp`. Drop the duplicate \
         or rename it to the actual second file intended)",
        path.display()
    )]
    CodePathDuplicate { slot: &'static str, path: PathBuf },
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
        ":etiquetas entry {etiqueta:?} is not a valid chart-keyword shape: \
         {reason} (the substrate consumes this string through the shared \
         `crate::render::is_chart_keyword_shape` predicate — the same \
         Cargo crates.io `[package] keywords` grammar entry shape: 1..=20 \
         bytes, starts with an ASCII letter, ASCII alphanumeric / `_` / `-` \
         continuation. The canonical authoring shapes are short kebab-case \
         identifiers like `\"mesh\"`, `\"wasm\"`, `\"tatara-lisp\"`, \
         `\"hello-world\"`, `\"caixa-servico\"`, `\"infrastructure\"`. \
         Without this gate a malformed `:etiquetas` entry (paste-from-doc \
         leading / trailing whitespace `\" mesh\"` / `\"mesh \"`; \
         paste-from-multiline-doc newline `\"mesh\\nhttp\"`; \
         paste-from-Windows-CRLF-doc CR; CSV-list-separator confusion \
         `\"mesh,http,grpc\"` — the author meant to author three separate \
         list entries; path-separator confusion `\"caixa/servico\"`; \
         namespace-suffix `\"http.1\"`; leading-digit `\"1foo\"`; \
         kebab-leak `\"-foo\"`; snake-leak `\"_foo\"`; non-ASCII \
         `\"café\"` — every legitimate search tag is strict ASCII; \
         paste-from-binary-blob NUL / BEL / ESC / DEL byte) silently \
         passed `from_lisp` + `validate_etiquetas` + \
         `StandardLayout::verify` and landed in the rendered \
         `lareira-<nome>` Helm chart's `Chart.yaml keywords:` array as a \
         malformed search tag — Artifact Hub's keyword index + the future \
         caixa-registry's keyword index would either silently drop the \
         tag or fail to index it far from the source caixa.lisp; the gate \
         moves the diagnostic to the manifest layer with the offending \
         value named verbatim)"
    )]
    EtiquetaInvalid { etiqueta: String, reason: String },
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
        ":autores entry {autor:?} is not a valid chart-maintainer-name shape: \
         {reason} (the substrate consumes this string through the shared \
         `crate::render::is_chart_maintainer_name_shape` predicate — the same \
         single-line-UTF-8 floor every realistic chart maintainer name carries: \
         1..=128 bytes, no leading or trailing whitespace, no ASCII control \
         characters anywhere, Unicode bytes accepted. The canonical authoring \
         shapes are short single-line identifiers like `\"pleme-io\"`, \
         `\"Pleme Contributors\"`, `\"alice <alice@example.com>\"`, \
         `\"François Dupont\"`. Without this gate a malformed `:autores` entry \
         (paste-from-aligned-doc leading whitespace `\" pleme-io\"` / trailing \
         whitespace `\"pleme-io \"`; paste-from-multiline-doc newline \
         `\"alice\\nbob\"` — the author pasted a multi-line block of author \
         records into one entry instead of splitting into one entry per author; \
         paste-from-Windows-CRLF-doc carriage return `\"alice\\rbob\"`; \
         tab-from-aligned-doc `\"Pleme\\tContributors\"`; paste-from-binary-blob \
         NUL / BEL / ESC / DEL byte) silently passed `from_lisp` + \
         `validate_autores` + `StandardLayout::verify` and landed in the \
         rendered `lareira-<nome>` Helm chart's `Chart.yaml maintainers:` array \
         as a YAML-illegal multi-line scalar or a silently-trimmed whitespace \
         round-trip — every chart-aware UI (`helm list`, `helm search`, \
         Artifact Hub maintainer index) would render the maintainer name in a \
         single-line column far from the source caixa.lisp; the gate moves the \
         diagnostic to the manifest layer with the offending value named \
         verbatim)"
    )]
    AutorInvalid { autor: String, reason: String },
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
        ":descricao {descricao:?} is not a valid chart-description shape: \
         {reason} (the substrate consumes this string through the shared \
         `crate::render::is_chart_description_shape` predicate — the same \
         single-line-UTF-8 floor every realistic chart description carries: \
         1..=512 bytes, no leading or trailing whitespace, no ASCII control \
         characters anywhere, Unicode prose bytes accepted. The canonical \
         authoring shapes are short single-line summaries like `\"Canonical \
         Rust→wasm32-wasip2 caixa Servico.\"`, `\"Checkout flow.\"`, \
         `\"AWS provider caixa for tatara-lisp\"`. Without this gate a \
         malformed `:descricao` (paste-from-aligned-doc leading whitespace \
         `\" Checkout flow.\"` / trailing whitespace `\"Checkout flow. \"`; \
         paste-from-multiline-doc newline `\"Checkout\\nflow.\"`; \
         paste-from-Windows-CRLF-doc carriage return `\"Checkout\\rflow.\"`; \
         tab-from-aligned-doc `\"Checkout\\tflow.\"`; paste-from-binary-blob \
         NUL / BEL / ESC / DEL byte) silently passed `from_lisp` + \
         `validate_descricao` + `StandardLayout::verify` and landed in the \
         rendered `lareira-<nome>` Helm chart's `Chart.yaml description:` \
         field + `README.md` header paragraph as a YAML-illegal multi-line \
         scalar or a silently-trimmed whitespace round-trip — every \
         chart-aware UI (`helm list`, `helm search`, Artifact Hub) would \
         render the description in a single-line column far from the source \
         caixa.lisp; the gate moves the diagnostic to the manifest layer \
         with the offending value named verbatim)"
    )]
    DescricaoInvalid { descricao: String, reason: String },
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
        assert_eq!(
            c.declared_mesh_slots(),
            vec![
                crate::render::M3_AUTHOR_KEY_MEMBROS,
                crate::render::M3_AUTHOR_KEY_ENTRADA,
            ]
        );
    }

    #[test]
    fn m3_top_level_author_key_consts_pin_canonical_kebab_case_labels() {
        // Scalar-value pin: the five author-facing kebab-case labels the
        // `(defcaixa … :<slot> (…))` surface admits on the M3 top-level
        // mesh slot axis, one arm per typed slot. Mirrors the peer
        // scalar-value pin the sibling
        // [`crate::M2_AUTHOR_KEY_LIMITS`] /
        // [`crate::M2_AUTHOR_KEY_BEHAVIOR`] /
        // [`crate::M2_AUTHOR_KEY_UPGRADE_FROM`] M2 top-level slot consts
        // carry (f49c8b0), so both altitudes of the typed-slot algebra
        // (per-Servico M2 + per-Aplicacao M3) share the same
        // "one canonical byte-string per arm" discipline. A future
        // rebrand (`:membros` → `:members`, `:contratos` → `:contracts`,
        // `:politicas` → `:policies`, `:placement` → `:distribution`,
        // `:entrada` → `:ingress`) lands as an edit to exactly one const,
        // and every consumer that reaches for the label picks it up at
        // build time rather than at runtime as a downstream mismatch.
        assert_eq!(crate::render::M3_AUTHOR_KEY_MEMBROS, ":membros");
        assert_eq!(crate::render::M3_AUTHOR_KEY_CONTRATOS, ":contratos");
        assert_eq!(crate::render::M3_AUTHOR_KEY_POLITICAS, ":politicas");
        assert_eq!(crate::render::M3_AUTHOR_KEY_PLACEMENT, ":placement");
        assert_eq!(crate::render::M3_AUTHOR_KEY_ENTRADA, ":entrada");
    }

    #[test]
    fn declared_mesh_slots_route_through_lifted_m3_author_key_consts() {
        // Production-through-const pin: the five per-arm labels the
        // [`Caixa::declared_mesh_slots`] tagger pushes onto its return
        // `Vec` route through the lifted
        // [`crate::M3_AUTHOR_KEY_MEMBROS`] /
        // [`crate::M3_AUTHOR_KEY_CONTRATOS`] /
        // [`crate::M3_AUTHOR_KEY_POLITICAS`] /
        // [`crate::M3_AUTHOR_KEY_PLACEMENT`] /
        // [`crate::M3_AUTHOR_KEY_ENTRADA`] consts, in canonical
        // declaration order. A future re-order or drift at the tagger
        // (a rename that reaches the tagger but not the const, or vice
        // versa) surfaces here at build time rather than at runtime as
        // a [`crate::LayoutError::MeshSlotsOnNonAplicacao`]
        // `slots: <stale-kebab-case>` diagnostic far from the rename's
        // commit. Mirror of the peer
        // [`declared_servico_slots_route_through_lifted_m2_author_key_consts`]
        // pin (f49c8b0) on the sibling per-Servico M2 top-level slot
        // axis.
        use crate::{Entrada, Membro, MeshPolicy, Placement, PlacementStrategy, WitContract};
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
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
        assert_eq!(
            c.declared_mesh_slots(),
            vec![
                crate::render::M3_AUTHOR_KEY_MEMBROS,
                crate::render::M3_AUTHOR_KEY_CONTRATOS,
                crate::render::M3_AUTHOR_KEY_POLITICAS,
                crate::render::M3_AUTHOR_KEY_PLACEMENT,
                crate::render::M3_AUTHOR_KEY_ENTRADA,
            ]
        );
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
            vec![
                crate::render::SUPERVISOR_AUTHOR_KEY_ESTRATEGIA,
                crate::render::SUPERVISOR_AUTHOR_KEY_RESTART_WINDOW,
            ]
        );
    }

    #[test]
    fn supervisor_top_level_author_key_consts_pin_canonical_kebab_case_labels() {
        // Scalar-value pin: the four author-facing kebab-case labels the
        // `(defcaixa … :<slot> (…))` surface admits on the Supervisor
        // supervision-tree slot axis, one arm per typed slot. Mirrors the
        // peer scalar-value pins the sibling
        // [`crate::render::M2_AUTHOR_KEY_LIMITS`] /
        // [`crate::render::M2_AUTHOR_KEY_BEHAVIOR`] /
        // [`crate::render::M2_AUTHOR_KEY_UPGRADE_FROM`] top-level M2 slot
        // consts and [`crate::render::M3_AUTHOR_KEY_MEMBROS`] etc.
        // top-level M3 slot consts carry, so all three kind-scoped
        // typed-slot-family author-facing-label axes route through one
        // canonical per-arm declaration. A future rebrand
        // (`:estrategia` → `:strategy` for English uniformity,
        // `:max-restarts` → `:max-intensity` matching Erlang/OTP's
        // `MaxIntensity` name, `:restart-window` → `:period` matching
        // OTP's `Period` name, `:children` → `:workers` matching Elixir
        // idiom) lands as an edit to exactly one const, and every
        // consumer that reaches for the label picks it up at build time
        // rather than at runtime as a downstream mismatch.
        assert_eq!(
            crate::render::SUPERVISOR_AUTHOR_KEY_ESTRATEGIA,
            ":estrategia"
        );
        assert_eq!(
            crate::render::SUPERVISOR_AUTHOR_KEY_MAX_RESTARTS,
            ":max-restarts"
        );
        assert_eq!(
            crate::render::SUPERVISOR_AUTHOR_KEY_RESTART_WINDOW,
            ":restart-window"
        );
        assert_eq!(crate::render::SUPERVISOR_AUTHOR_KEY_CHILDREN, ":children");
    }

    #[test]
    fn declared_supervisor_slots_route_through_lifted_supervisor_author_key_consts() {
        // Production-through-const pin: the four per-arm labels the
        // [`Caixa::declared_supervisor_slots`] tagger pushes onto its
        // return `Vec` route through the lifted
        // [`crate::render::SUPERVISOR_AUTHOR_KEY_ESTRATEGIA`] /
        // [`crate::render::SUPERVISOR_AUTHOR_KEY_MAX_RESTARTS`] /
        // [`crate::render::SUPERVISOR_AUTHOR_KEY_RESTART_WINDOW`] /
        // [`crate::render::SUPERVISOR_AUTHOR_KEY_CHILDREN`] consts, in
        // canonical declaration order. A future re-order or drift at the
        // tagger (a rename that reaches the tagger but not the const, or
        // vice versa) surfaces here at build time rather than at runtime
        // as a [`crate::LayoutError::SupervisorSlotsOnNonSupervisor`]
        // `slots: <stale-kebab-case>` diagnostic far from the rename's
        // commit. Mirror of the peer
        // [`declared_servico_slots_route_through_lifted_m2_author_key_consts`]
        // (f49c8b0) and
        // [`declared_mesh_slots_route_through_lifted_m3_author_key_consts`]
        // (882f498) pins on the sibling M2 / M3 top-level slot axes.
        use crate::{ChildSpec, RestartPolicy, RestartStrategy};
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.estrategia = Some(RestartStrategy::OneForOne);
        c.max_restarts = Some(5);
        c.restart_window = Some("60s".into());
        c.children = vec![ChildSpec {
            caixa: "worker".into(),
            versao: "^0.1".into(),
            restart: RestartPolicy::Permanent,
        }];
        assert_eq!(
            c.declared_supervisor_slots(),
            vec![
                crate::render::SUPERVISOR_AUTHOR_KEY_ESTRATEGIA,
                crate::render::SUPERVISOR_AUTHOR_KEY_MAX_RESTARTS,
                crate::render::SUPERVISOR_AUTHOR_KEY_RESTART_WINDOW,
                crate::render::SUPERVISOR_AUTHOR_KEY_CHILDREN,
            ]
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
        assert_eq!(
            c.declared_servico_slots(),
            vec![
                crate::render::M2_AUTHOR_KEY_LIMITS,
                crate::render::M2_AUTHOR_KEY_UPGRADE_FROM,
            ]
        );
    }

    #[test]
    fn m2_top_level_author_key_consts_pin_canonical_kebab_case_labels() {
        // Scalar-value pin: the three author-facing kebab-case labels
        // the `(defcaixa … :<slot> (…))` surface admits on the M2
        // top-level slot axis, one arm per typed slot. Mirrors the peer
        // scalar-value pin the sibling renderer-side
        // [`crate::M2_KEY_LIMITS`] / [`crate::M2_KEY_BEHAVIOR`] /
        // [`crate::M2_KEY_UPGRADE_FROM`] camelCase overlay-container
        // consts carry, so both halves of the M2 top-level slot dual
        // axis (author-facing kebab-case label + renderer-side
        // camelCase overlay-container wire key) route through one
        // canonical per-arm declaration. A future rebrand
        // (`:limits` → `:sandbox` matching Lunatic per-process
        // terminology INSPIRATIONS §III.1, `:behavior` → `:gen-server`
        // matching Erlang's verbatim name, `:upgrade-from` → `:appup`
        // matching Erlang's verbatim appup name) lands as an edit to
        // exactly one const, and every consumer that reaches for the
        // label picks it up at build time rather than at runtime as a
        // downstream mismatch.
        assert_eq!(crate::render::M2_AUTHOR_KEY_LIMITS, ":limits");
        assert_eq!(crate::render::M2_AUTHOR_KEY_BEHAVIOR, ":behavior");
        assert_eq!(crate::render::M2_AUTHOR_KEY_UPGRADE_FROM, ":upgrade-from");
    }

    #[test]
    fn declared_servico_slots_route_through_lifted_m2_author_key_consts() {
        // Production-through-const pin: the three per-arm labels the
        // [`Caixa::declared_servico_slots`] tagger pushes onto its
        // return `Vec` route through the lifted
        // [`crate::M2_AUTHOR_KEY_LIMITS`] /
        // [`crate::M2_AUTHOR_KEY_BEHAVIOR`] /
        // [`crate::M2_AUTHOR_KEY_UPGRADE_FROM`] consts, in canonical
        // declaration order. A future re-order or drift at the tagger
        // (a rename that reaches the tagger but not the const, or vice
        // versa) surfaces here at build time rather than at runtime as
        // a [`crate::LayoutError::ServicoSlotsOnNonServico`]
        // `slots: <stale-kebab-case>` diagnostic far from the rename's
        // commit. Mirror of the peer
        // [`crate::behavior::BehaviorSpec::declared_slots`] production
        // tagger pin (889dc18) on the sibling per-callback axis.
        use crate::{BehaviorSpec, UpgradeFromEntry, UpgradeInstruction};
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.limits = Some(crate::LimitsSpec {
            fuel: Some(1_000_000),
            ..Default::default()
        });
        c.behavior = Some(BehaviorSpec {
            on_init: Some(PathBuf::from("lib/init.lisp")),
            ..Default::default()
        });
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![UpgradeInstruction::Restart],
        }];
        assert_eq!(
            c.declared_servico_slots(),
            vec![
                crate::render::M2_AUTHOR_KEY_LIMITS,
                crate::render::M2_AUTHOR_KEY_BEHAVIOR,
                crate::render::M2_AUTHOR_KEY_UPGRADE_FROM,
            ]
        );
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
                    if nome == "caixa-teia" && list == crate::render::DEP_AUTHOR_KEY_DEPS
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
                    if nome == "tatara-check" && list == crate::render::DEP_AUTHOR_KEY_DEPS_DEV
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
                crate::dep::DepError::DuplicateNome { ref nome, list }
                    if nome == "caixa-teia" && list == crate::render::DEP_AUTHOR_KEY_DEPS
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
                crate::dep::DepError::DuplicateNome { ref nome, list }
                    if nome == "runtime-dep" && list == crate::render::DEP_AUTHOR_KEY_DEPS
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
        assert_eq!(list, crate::render::DEP_AUTHOR_KEY_DEPS_DEV);
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

    // ── Caixa::validate_nome_chart_name_budget — joint-length on `:nome` ──
    //
    // The bare-`:nome` axis [`Caixa::validate_nome`] caps at 63 bytes
    // via DNS-1123; this second-axis gate caps the joint
    // `lareira-<nome>` chart name at the same 63-byte ceiling. The
    // canonical [`crate::lareira_chart_name`] helper's doc comment
    // (f7320d7, caixa-core/src/render.rs:3198) explicitly deferred:
    // "the M4 admission webhook will pin the joint-length invariant
    // when it lands". These tests pin it at the manifest-validate
    // layer instead, fail-before-pass-after on the 56-byte boundary.

    #[test]
    fn validate_nome_chart_name_budget_accepts_canonical_template() {
        // Positive control: the bare `feira init`-style template's
        // `:nome` ("demo") sits far below the cap; the gate must not
        // regress this baseline. Same shape every peer
        // value-shape-gate baseline pin uses.
        let c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.validate_nome_chart_name_budget().unwrap();
    }

    #[test]
    fn validate_nome_chart_name_budget_accepts_canonical_fixtures() {
        // Positive-set sweep across the canonical author surface every
        // in-tree fixture uses (`hello-rio`, `cart`, `checkout`,
        // `worker`, the `checkout-aplicacao` example members, the
        // `akeyless-attest` caixa-tatara fixture). Every value sits
        // far below the 55-byte per-`:nome` budget. Same shape every
        // peer per-axis baseline pin uses.
        for nome in [
            "hello-rio",
            "cart",
            "checkout",
            "worker",
            "akeyless-attest",
            "demo",
            "a",
        ] {
            caixa_with_nome(nome)
                .validate_nome_chart_name_budget()
                .unwrap_or_else(|e| {
                    panic!("canonical :nome {nome:?} must pass chart-name budget, got {e:?}")
                });
        }
    }

    #[test]
    fn validate_nome_chart_name_budget_accepts_nome_at_cap() {
        // Boundary-accepting case at the 55-byte per-`:nome` budget —
        // the joint chart name is exactly 63 bytes, the DNS-1123 label
        // cap. Pinned alongside the rejecting-arm test so a future cap
        // shift surfaces both arms simultaneously. Mirrors
        // `nome_max_length_validates` on the peer bare-`:nome` axis.
        let at_cap = "a".repeat(crate::LAREIRA_CHART_NAME_NOME_MAX_LEN);
        caixa_with_nome(&at_cap)
            .validate_nome_chart_name_budget()
            .unwrap();
    }

    #[test]
    fn validate_nome_chart_name_budget_rejects_nome_one_over_cap() {
        // Fail-before-pass-after pin on the 56-byte boundary: the
        // smallest `:nome` length that overflows the joint chart-name
        // cap. The inner [`is_dns_1123_label`] gate
        // (`Caixa::validate_nome`) accepts it (56 ≤ 63), so prior to
        // this gate it silently passed the manifest-validate cascade
        // and surfaced as a `helm lint` / apiserver rejection on the
        // rendered chart name far from the source `caixa.lisp`, with
        // no field naming the overflow. With this gate the diagnostic
        // names the offending `:nome` verbatim alongside the rendered
        // chart name and the budget, so the author can shorten in one
        // edit. Mirrors `validate_nome_rejects_too_long` on the peer
        // bare-`:nome` axis.
        let over = "a".repeat(crate::LAREIRA_CHART_NAME_NOME_MAX_LEN + 1);
        let c = caixa_with_nome(&over);
        let err = c.validate_nome_chart_name_budget().unwrap_err();
        let ManifestError::NomeChartNameBudgetExceeded { nome, reason } = err else {
            panic!("expected NomeChartNameBudgetExceeded for over-budget :nome");
        };
        assert_eq!(nome.len(), crate::LAREIRA_CHART_NAME_NOME_MAX_LEN + 1);
        assert_eq!(nome, over);
        assert!(
            reason.contains("63") && reason.contains("64") && reason.contains("55"),
            "diagnostic must name the DNS-1123 cap (63), the actual chart-name length (64), \
             and the per-`:nome` budget (55), got {reason:?}"
        );
    }

    #[test]
    fn validate_nome_chart_name_budget_rejects_nome_at_bare_dns_cap() {
        // The 63-byte `:nome` boundary — passes the bare-`:nome`
        // [`is_dns_1123_label`] cap exactly, but produces a 71-byte
        // joint chart name that overflows the DNS-1123 label cap
        // structurally. The most stringent fail-before-pass-after
        // surface: every `:nome` in the 56..=63-byte range passed the
        // prior cascade and broke at admission.
        let bare_max = "a".repeat(crate::DNS_1123_LABEL_MAX_LEN);
        let c = caixa_with_nome(&bare_max);
        // The bare-`:nome` gate accepts the 63-byte length.
        c.validate_nome().unwrap();
        // The new joint-length gate rejects it.
        let err = c.validate_nome_chart_name_budget().unwrap_err();
        assert!(
            matches!(
                err,
                ManifestError::NomeChartNameBudgetExceeded { ref nome, .. }
                    if nome.len() == crate::DNS_1123_LABEL_MAX_LEN
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_nome_chart_name_budget_diagnostic_carries_offending_chart_name() {
        // Diagnostic-shape pin: the rendered `lareira-<nome>` chart
        // name appears verbatim in the diagnostic so the author sees
        // exactly the string the apiserver / `helm lint` would have
        // rejected — no re-derivation required to grep the source.
        // Peer with `nome_invalid_diagnostic_carries_offending_nome`
        // on the bare-`:nome` axis.
        let over = "x".repeat(crate::LAREIRA_CHART_NAME_NOME_MAX_LEN + 5);
        let c = caixa_with_nome(&over);
        let err = c.validate_nome_chart_name_budget().unwrap_err();
        let ManifestError::NomeChartNameBudgetExceeded { nome, reason } = err else {
            panic!("expected NomeChartNameBudgetExceeded variant");
        };
        assert_eq!(nome, over);
        let expected_chart = crate::lareira_chart_name(&over);
        assert!(
            reason.contains(&expected_chart),
            "diagnostic must carry the rendered chart name {expected_chart:?} verbatim, \
             got {reason:?}"
        );
        assert!(
            reason.contains("lareira-"),
            "diagnostic must name the canonical chart-name prefix verbatim, got {reason:?}"
        );
    }

    #[test]
    fn validate_nome_chart_name_budget_runs_after_nome_shape_via_layout_verify() {
        // Order pin on the layout cascade: the narrower
        // `NomeInvalid` (bare-DNS-1123 shape) fires before the
        // joint-length budget. A structurally-malformed `:nome` (here:
        // uppercase) surfaces its specific shape error rather than
        // the chart-name-budget error, even when the joint length
        // would also overflow — the narrower diagnostic is more
        // self-locating. Mirrors the cascade-precedence pins peer
        // gates already use (e.g. `EntradaParaEmpty` before
        // `EntradaParaInvalid`).
        let over = "A".repeat(crate::LAREIRA_CHART_NAME_NOME_MAX_LEN + 1);
        let c = caixa_with_nome(&over);
        // The bare-shape gate fires first.
        let err = c.validate_nome().unwrap_err();
        assert!(
            matches!(err, ManifestError::NomeInvalid { .. }),
            "bare-shape gate must fire before chart-name-budget gate; got {err:?}"
        );
        // And the layout verify cascade surfaces that diagnostic, not
        // the budget arm. Inject a path-exists oracle so the cascade
        // gets past the manifest-presence check and into the
        // value-shape gates.
        let layout = crate::StandardLayout::new().with_path_exists(|_| true);
        let err = crate::LayoutInvariants::verify(
            &layout,
            &c,
            std::path::Path::new("/tmp/caixa-test-fake-root"),
        )
        .unwrap_err();
        let issue = err.to_string();
        assert!(
            issue.contains("DNS-1123") || issue.contains("uppercase"),
            "layout cascade must surface the bare-DNS-1123 diagnostic on a \
             structurally-malformed :nome, not the chart-name-budget diagnostic; got {issue:?}"
        );
    }

    #[test]
    fn layout_verify_routes_chart_name_budget_through_nome_violation() {
        // Cross-axis envelope pin: the layout cascade wraps both
        // bare-`:nome` and joint-length-`:nome` failures through the
        // same [`LayoutError::NomeViolation`] envelope, since both
        // arms are on the `:nome` axis. The user's diagnostic stays
        // self-locating ("which axis"), and a future consumer that
        // dispatches on the layout-error variant (e.g. a `feira lint`
        // exit-code mapping) sees a single per-axis envelope. The
        // wrapped `issue:` carries the full inner diagnostic.
        let over = "a".repeat(crate::LAREIRA_CHART_NAME_NOME_MAX_LEN + 1);
        let c = caixa_with_nome(&over);
        // The bare-shape gate accepts.
        c.validate_nome().unwrap();
        let layout = crate::StandardLayout::new().with_path_exists(|_| true);
        let err = crate::LayoutInvariants::verify(
            &layout,
            &c,
            std::path::Path::new("/tmp/caixa-test-fake-root"),
        )
        .unwrap_err();
        let crate::LayoutError::NomeViolation { caixa, issue } = err else {
            panic!("expected LayoutError::NomeViolation, got {err:?}");
        };
        assert_eq!(caixa, over);
        assert!(
            issue.contains("lareira-") && issue.contains("63") && issue.contains("55"),
            "wrapped issue must carry the joint-length diagnostic verbatim, got {issue:?}"
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

    #[test]
    fn validate_code_paths_rejects_duplicate_bibliotecas_entry() {
        // Canonical copy-paste-the-wrong-file footgun on the biblioteca
        // axis. Without the gate `feira build` re-parses the same lib
        // twice, wasting work and silently masking the author's intent
        // to declare a *second* biblioteca.
        let c = caixa_with_code_paths(vec!["lib/demo.lisp", "lib/demo.lisp"], vec![], vec![]);
        let err = c.validate_code_paths().unwrap_err();
        let ManifestError::CodePathDuplicate { slot, path } = err else {
            panic!("expected CodePathDuplicate, got {err:?}");
        };
        assert_eq!(slot, ":bibliotecas");
        assert_eq!(path, PathBuf::from("lib/demo.lisp"));
    }

    #[test]
    fn validate_code_paths_rejects_duplicate_exe_entry() {
        // Same footgun on the Binario surface. The future `caixa-flake`
        // emitter that materializes each `:exe` entry as a flake
        // `packages.<name>` derivation would collide on the duplicate
        // package key — surfaced here at the typed-validate layer with a
        // self-locating diagnostic instead.
        let c = caixa_with_code_paths(vec![], vec!["exe/cli", "exe/cli"], vec![]);
        let err = c.validate_code_paths().unwrap_err();
        let ManifestError::CodePathDuplicate { slot, path } = err else {
            panic!("expected CodePathDuplicate, got {err:?}");
        };
        assert_eq!(slot, ":exe");
        assert_eq!(path, PathBuf::from("exe/cli"));
    }

    #[test]
    fn validate_code_paths_rejects_duplicate_servicos_entry() {
        // Same footgun on the Servico surface. The peer caixa-helm /
        // caixa-flux renderers refuse `:servicos.len() != 1` with the
        // narrower `UnsupportedServicoCount` diagnostic, but that
        // diagnostic surfaces "too many servicos" without naming
        // "duplicate entry" — the typed self-locating framing only lands
        // at this gate.
        let c = caixa_with_code_paths(
            vec![],
            vec![],
            vec![
                "servicos/demo.computeunit.yaml",
                "servicos/demo.computeunit.yaml",
            ],
        );
        let err = c.validate_code_paths().unwrap_err();
        let ManifestError::CodePathDuplicate { slot, path } = err else {
            panic!("expected CodePathDuplicate, got {err:?}");
        };
        assert_eq!(slot, ":servicos");
        assert_eq!(path, PathBuf::from("servicos/demo.computeunit.yaml"));
    }

    #[test]
    fn validate_code_paths_accepts_same_path_across_slots() {
        // Per-list scope pin: a `:bibliotecas` entry that happens to
        // collide with an `:exe` or `:servicos` entry as a *string* is
        // not a duplicate by this gate (each list gets its own HashSet),
        // mirroring the peer `:deps` ↔ `:deps-dev` per-list scope
        // (a `:nome` present in both lists is a legitimate dev-vs-runtime
        // shape on the dep axis). The structural `starts_with(<exe |
        // servicos>_dir)` fence at layout time prevents the realistic
        // cross-slot collision case from existing on disk, but the gate's
        // per-list scope is correct independent of that downstream fence.
        let c = caixa_with_code_paths(
            vec!["lib/x.lisp"],
            vec!["exe/x"],
            vec!["servicos/x.computeunit.yaml"],
        );
        c.validate_code_paths().unwrap();
    }

    #[test]
    fn validate_code_paths_duplicate_fires_after_structural_checks_on_same_slot() {
        // Within-slot ordering pin: structural defects (empty / absolute
        // / parent-escape) fire before the duplicate gate on the same
        // slot. A `:bibliotecas ("" "lib/x.lisp" "lib/x.lisp")` shape
        // surfaces the narrower `CodePathEmpty` for the empty entry
        // first, not the duplicate on the later pair — same arm-ordering
        // every peer per-list duplicate gate uses (`:etiquetas` 360a499,
        // `:autores` 86c769b, `:deps` 359fba5).
        let c = caixa_with_code_paths(vec!["", "lib/x.lisp", "lib/x.lisp"], vec![], vec![]);
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
    fn validate_code_paths_duplicate_in_bibliotecas_fires_before_duplicate_in_exe() {
        // Cross-slot ordering pin on the duplicate arm: `:bibliotecas`
        // duplicates surface before `:exe` duplicates, matching the
        // canonical `:bibliotecas` → `:exe` → `:servicos` declaration
        // order every peer per-slot diagnostic on this surface follows.
        let c = caixa_with_code_paths(
            vec!["lib/x.lisp", "lib/x.lisp"],
            vec!["exe/y", "exe/y"],
            vec![],
        );
        let err = c.validate_code_paths().unwrap_err();
        let ManifestError::CodePathDuplicate { slot, path } = err else {
            panic!("expected CodePathDuplicate, got {err:?}");
        };
        assert_eq!(slot, ":bibliotecas");
        assert_eq!(path, PathBuf::from("lib/x.lisp"));
    }

    #[test]
    fn validate_code_paths_duplicate_diagnostic_carries_offending_slot_and_path() {
        // Diagnostic-shape pin (peer with
        // `validate_code_paths_diagnostic_carries_offending_slot_and_path`
        // on the structural arm): the duplicate-arm Display surfaces both
        // the offending `:slot` tag and the offending path verbatim, so a
        // `feira lint` run can render the diagnostic without re-parsing.
        let c = caixa_with_code_paths(
            vec![],
            vec![],
            vec![
                "servicos/demo.computeunit.yaml",
                "servicos/demo.computeunit.yaml",
            ],
        );
        let rendered = c.validate_code_paths().unwrap_err().to_string();
        assert!(
            rendered.contains(":servicos"),
            "diagnostic must name the offending slot: {rendered}",
        );
        assert!(
            rendered.contains("servicos/demo.computeunit.yaml"),
            "diagnostic must quote the offending path: {rendered}",
        );
    }

    // ── validate_code_paths — `.lisp` extension gate on :bibliotecas ──
    //
    // The lifted [`crate::render::is_lisp_extension`] predicate (33cc830)
    // now gates `:bibliotecas` entries on the tatara-lisp-source file-type
    // contract. The `feira build` loop (`caixa-feira/src/cmd/build.rs:33`)
    // reads every declared `:bibliotecas` entry through `tatara_lisp::read`
    // at parse time — the same downstream consumer the peer `:behavior
    // :on-*` (c97815a, [`crate::BehaviorError::NonLispExtension`]) and
    // `:upgrade-from :state-change :script` (33cc830,
    // [`crate::UpgradeError::NonLispExtensionScript`]) axes route through.
    // `:exe` and `:servicos` are deliberately excluded — `:exe` is the
    // nix-built executable surface (`"exe/<name>"` shape per the canonical
    // [`crate::LayoutError::ExeOutsideDir`] error message and every
    // in-tree `caixa_with_code_paths` positive control), and `:servicos`
    // is the `.computeunit.yaml` ComputeUnit-CR axis.

    #[test]
    fn validate_code_paths_rejects_no_extension_bibliotecas_entry() {
        // Canonical "I dragged the wrong file from the workspace tree"
        // footgun on the biblioteca axis. Without the gate `feira build`
        // hands the extensionless path to `tatara_lisp::read` and fails
        // with a parser-shaped diagnostic far from the source caixa.lisp,
        // with no field naming the offending `:bibliotecas` entry.
        for relpath in ["lib/demo", "demo", "lib/handlers/inner"] {
            let c = caixa_with_code_paths(vec![relpath], vec![], vec![]);
            let err = c.validate_code_paths().unwrap_err();
            let ManifestError::CodePathNonLispExtension { slot, path } = err else {
                panic!("expected CodePathNonLispExtension for {relpath:?}, got {err:?}");
            };
            assert_eq!(slot, ":bibliotecas");
            assert_eq!(path, PathBuf::from(relpath));
        }
    }

    #[test]
    fn validate_code_paths_rejects_wrong_extension_bibliotecas_entry() {
        // Wrong-extension sweep across common authoring footguns. Same
        // sweep posture as the peer
        // `behavior::validate_rejects_wrong_extension` (c97815a) and
        // `upgrade::tests::state_change_rejects_wrong_extension_script`
        // (33cc830) cases.
        for relpath in [
            "lib/demo.rs",
            "lib/demo.txt",
            "lib/demo.md",
            "lib/demo.json",
            "lib/demo.yaml",
            "lib/demo.toml",
            "lib/demo.lisp.bak",
            "lib/demo.lispx",
            "lib/demo.lis",
        ] {
            let c = caixa_with_code_paths(vec![relpath], vec![], vec![]);
            let err = c.validate_code_paths().unwrap_err();
            let ManifestError::CodePathNonLispExtension { slot, path } = err else {
                panic!("expected CodePathNonLispExtension for {relpath:?}, got {err:?}");
            };
            assert_eq!(slot, ":bibliotecas");
            assert_eq!(path, PathBuf::from(relpath));
        }
    }

    #[test]
    fn validate_code_paths_rejects_case_folded_extension_bibliotecas_entry() {
        // Case-sensitivity sweep — pins the strict lowercase `.lisp`
        // contract. An uppercase `.LISP` shape that the layout's existence
        // check would (case-insensitively, on case-insensitive volumes)
        // match the on-disk file still mismatches the canonical form the
        // codec emits, breaking the THEORY.md §V.2.7 render-determinism
        // contract. Mirrors the peer
        // `behavior::validate_rejects_case_folded_extension` (c97815a) and
        // `upgrade::tests::state_change_rejects_case_folded_extension_script`
        // (33cc830) sweeps.
        for relpath in [
            "lib/demo.LISP",
            "lib/demo.Lisp",
            "lib/demo.LiSp",
            "lib/demo.lISP",
        ] {
            let c = caixa_with_code_paths(vec![relpath], vec![], vec![]);
            let err = c.validate_code_paths().unwrap_err();
            let ManifestError::CodePathNonLispExtension { slot, path } = err else {
                panic!("expected CodePathNonLispExtension for {relpath:?}, got {err:?}");
            };
            assert_eq!(slot, ":bibliotecas");
            assert_eq!(path, PathBuf::from(relpath));
        }
    }

    #[test]
    fn validate_code_paths_accepts_canonical_lisp_shapes() {
        // Positive-control sweep through every canonical authoring shape
        // every in-tree fixture and the `Caixa::template` scaffold use.
        // Mirrors the peer `behavior::validate_accepts_canonical_lisp_paths`
        // (c97815a) and the lifted predicate's own
        // `is_lisp_extension_accepts_canonical_shapes` sweep in render.rs
        // (33cc830).
        for relpath in [
            "lib/demo.lisp",
            "lib/handlers.lisp",
            "lib/migrations/v01-to-v02.lisp",
            "demo.lisp",
            "a.lisp",
            "./lib/demo.lisp",
            "lib/./handlers.lisp",
            "lib/migrations/v.0.1.lisp",
        ] {
            let c = caixa_with_code_paths(vec![relpath], vec![], vec![]);
            c.validate_code_paths()
                .unwrap_or_else(|e| panic!("canonical shape {relpath:?} must pass, got {e:?}"));
        }
    }

    #[test]
    fn validate_code_paths_non_lisp_extension_does_not_fire_on_exe_or_servicos() {
        // The file-type gate is per-slot — only `:bibliotecas` carries the
        // tatara-lisp-source contract. An extensionless `:exe` entry
        // (`exe/demo`) and a `.computeunit.yaml` `:servicos` entry are the
        // canonical shapes every in-tree fixture uses, and must continue
        // to pass validate. Pins that a future tightening that broadens
        // the `.lisp` gate to either axis surfaces as a test failure
        // rather than as a silent breaking change to existing valid
        // manifests.
        let c = caixa_with_code_paths(
            vec![],
            vec!["exe/demo", "exe/tool"],
            vec!["servicos/demo.computeunit.yaml"],
        );
        c.validate_code_paths().unwrap();
    }

    #[test]
    fn validate_code_paths_sandbox_shape_arms_precede_non_lisp_extension() {
        // Cross-arm precedence pin: a `:bibliotecas` entry that is *both*
        // sandbox-escaping and non-`.lisp` surfaces the more fundamental
        // sandbox-shape diagnostic first (the `.lisp` remediation would
        // be misleading when the offending path can never resolve under
        // the caixa root anyway). Mirrors the peer
        // `EmptyPath` → `AbsolutePath` → `ParentEscape` → `NonLispExtension`
        // ordering on `:behavior :on-*` (c97815a) and `EmptyScript` →
        // `AbsoluteScript` → `ParentEscapeScript` → `NonLispExtensionScript`
        // on `:upgrade-from :state-change :script` (33cc830).
        //
        // Empty wins (the strictly-smaller-scope structural arm).
        let c = caixa_with_code_paths(vec![""], vec![], vec![]);
        assert!(
            matches!(
                c.validate_code_paths().unwrap_err(),
                ManifestError::CodePathEmpty {
                    slot: ":bibliotecas"
                }
            ),
            "empty must win over non-lisp-extension",
        );
        // Absolute wins (the path can't resolve under the caixa root).
        let c = caixa_with_code_paths(vec!["/etc/passwd"], vec![], vec![]);
        let err = c.validate_code_paths().unwrap_err();
        let ManifestError::CodePathAbsolute { slot, .. } = err else {
            panic!("absolute must win over non-lisp-extension, got {err:?}");
        };
        assert_eq!(slot, ":bibliotecas");
        // ParentEscape wins (the path escapes the caixa root).
        let c = caixa_with_code_paths(vec!["../sibling/x.txt"], vec![], vec![]);
        let err = c.validate_code_paths().unwrap_err();
        let ManifestError::CodePathParentEscape { slot, .. } = err else {
            panic!("parent-escape must win over non-lisp-extension, got {err:?}");
        };
        assert_eq!(slot, ":bibliotecas");
    }

    #[test]
    fn validate_code_paths_non_lisp_extension_precedes_duplicate() {
        // Within-slot precedence pin: the per-entry file-type shape gate
        // fires before the cross-entry duplicate gate, so the narrower
        // structural defect dominates the uniqueness diagnostic. A
        // `("lib/x.txt" "lib/x.txt")` shape surfaces
        // `CodePathNonLispExtension` on the first entry rather than
        // `CodePathDuplicate` on the pair — same posture every per-entry
        // shape-gate-precedes-duplicate cascade follows on this surface
        // (the empty / absolute / parent-escape arms already precede the
        // duplicate arm; the lifted file-type arm joins that set).
        let c = caixa_with_code_paths(vec!["lib/x.txt", "lib/x.txt"], vec![], vec![]);
        let err = c.validate_code_paths().unwrap_err();
        let ManifestError::CodePathNonLispExtension { slot, path } = err else {
            panic!("expected CodePathNonLispExtension, got {err:?}");
        };
        assert_eq!(slot, ":bibliotecas");
        assert_eq!(path, PathBuf::from("lib/x.txt"));
    }

    #[test]
    fn validate_code_paths_non_lisp_extension_diagnostic_carries_offending_slot_and_path() {
        // Diagnostic-shape pin (peer with
        // `validate_code_paths_diagnostic_carries_offending_slot_and_path`
        // on the sandbox-shape arms and
        // `validate_code_paths_duplicate_diagnostic_carries_offending_slot_and_path`
        // on the duplicate arm): the file-type-arm Display surfaces both
        // the offending `:slot` tag, the offending path verbatim, and the
        // expected `.lisp` extension named in the remediation text, so a
        // `feira lint` run can render the diagnostic without re-parsing.
        let c = caixa_with_code_paths(vec!["lib/demo.rs"], vec![], vec![]);
        let rendered = c.validate_code_paths().unwrap_err().to_string();
        assert!(
            rendered.contains(":bibliotecas"),
            "diagnostic must name the offending slot: {rendered}",
        );
        assert!(
            rendered.contains("lib/demo.rs"),
            "diagnostic must quote the offending path: {rendered}",
        );
        assert!(
            rendered.contains(".lisp"),
            "diagnostic must name the expected extension: {rendered}",
        );
    }

    // ── validate_code_paths — `.computeunit.yaml` compound-suffix gate on :servicos ──
    //
    // The lifted [`crate::render::is_computeunit_yaml_extension`] predicate
    // now gates `:servicos` entries on the ComputeUnit-CR YAML file-type
    // contract. The peer caixa-helm / caixa-flux renderers consume each
    // `:servicos` entry through `serde_yaml::from_str` as a typed
    // `ComputeUnit` CR — same downstream-consumer-shape lift as the peer
    // `:bibliotecas` `.lisp` gate (64772a9), here on the compound-suffix
    // axis `Path::extension` can't express on its own.

    #[test]
    fn validate_code_paths_rejects_no_extension_servicos_entry() {
        // Canonical "I dragged the wrong file from the workspace tree"
        // footgun on the Servico axis. Without the gate the peer
        // caixa-helm / caixa-flux renderers hand the extensionless path
        // to `serde_yaml::from_str` and fail with a parser-shaped
        // diagnostic far from the source caixa.lisp, with no field
        // naming the offending `:servicos` entry.
        for relpath in ["servicos/demo", "demo", "servicos/sub/nested"] {
            let c = caixa_with_code_paths(vec![], vec![], vec![relpath]);
            let err = c.validate_code_paths().unwrap_err();
            let ManifestError::CodePathNonComputeUnitYamlExtension { slot, path } = err else {
                panic!(
                    "expected CodePathNonComputeUnitYamlExtension for {relpath:?}, \
                     got {err:?}"
                );
            };
            assert_eq!(slot, ":servicos");
            assert_eq!(path, PathBuf::from(relpath));
        }
    }

    #[test]
    fn validate_code_paths_rejects_wrong_extension_servicos_entry() {
        // Wrong-extension sweep across common authoring footguns on the
        // Servico axis. Bare `.yaml` is the canonical "I forgot the
        // `.computeunit` segment" typo; the off-by-one-segment shapes
        // (`.computeunit-yaml` / `.computeunit_yaml`) silently pass the
        // bare `Path::extension` view but mismatch the typed compound
        // suffix the renderers' `serde_yaml::from_str` consumer demands.
        // Same sweep-posture as the peer
        // `validate_code_paths_rejects_wrong_extension_bibliotecas_entry`
        // (64772a9) on the sibling tatara-lisp-source axis.
        for relpath in [
            "servicos/demo.yaml",
            "servicos/demo.yml",
            "servicos/demo.json",
            "servicos/demo.toml",
            "servicos/demo.txt",
            "servicos/demo.computeunit.yaml.bak",
            "servicos/demo.computeunit.yam",
            "servicos/demo.computeunit",
            "servicos/demo-computeunit.yaml",
            "servicos/demo_computeunit.yaml",
        ] {
            let c = caixa_with_code_paths(vec![], vec![], vec![relpath]);
            let err = c.validate_code_paths().unwrap_err();
            let ManifestError::CodePathNonComputeUnitYamlExtension { slot, path } = err else {
                panic!(
                    "expected CodePathNonComputeUnitYamlExtension for {relpath:?}, \
                     got {err:?}"
                );
            };
            assert_eq!(slot, ":servicos");
            assert_eq!(path, PathBuf::from(relpath));
        }
    }

    #[test]
    fn validate_code_paths_rejects_case_folded_extension_servicos_entry() {
        // Case-sensitivity sweep — pins the strict lowercase
        // `.computeunit.yaml` contract. A case-folded shape that the
        // layout's existence check would (case-insensitively, on
        // case-insensitive volumes) match the on-disk file still
        // mismatches the canonical form the codec emits, breaking the
        // THEORY.md §V.2.7 render-determinism contract. Mirrors the peer
        // `validate_code_paths_rejects_case_folded_extension_bibliotecas_entry`
        // (64772a9) sweep on the sibling tatara-lisp-source axis.
        for relpath in [
            "servicos/demo.ComputeUnit.yaml",
            "servicos/demo.COMPUTEUNIT.yaml",
            "servicos/demo.computeunit.YAML",
            "servicos/demo.computeunit.Yaml",
            "servicos/demo.COMPUTEUNIT.YAML",
        ] {
            let c = caixa_with_code_paths(vec![], vec![], vec![relpath]);
            let err = c.validate_code_paths().unwrap_err();
            let ManifestError::CodePathNonComputeUnitYamlExtension { slot, path } = err else {
                panic!(
                    "expected CodePathNonComputeUnitYamlExtension for {relpath:?}, \
                     got {err:?}"
                );
            };
            assert_eq!(slot, ":servicos");
            assert_eq!(path, PathBuf::from(relpath));
        }
    }

    #[test]
    fn validate_code_paths_rejects_empty_stem_servicos_entry() {
        // Degenerate hidden-file shape: a file name exactly equal to the
        // suffix (`.computeunit.yaml` — no stem preceding the suffix) is
        // the structural "Servico declared with no identity" footgun.
        // The substrate identifies each ComputeUnit by the file-stem
        // segment that precedes `.computeunit.yaml` (the rendered
        // `lareira-<stem>` Helm chart, the per-Servico `metadata.name`,
        // the M3 `:contratos` membership lookup), so an empty stem
        // leaves the Servico unidentifiable. Pinned at the typed-axis
        // level so a future regression that drops the `name.len() >
        // SUFFIX.len()` bound at the predicate surfaces here, not
        // piecemeal as a `lareira-` chart-name collision at render time.
        for relpath in ["servicos/.computeunit.yaml"] {
            let c = caixa_with_code_paths(vec![], vec![], vec![relpath]);
            let err = c.validate_code_paths().unwrap_err();
            let ManifestError::CodePathNonComputeUnitYamlExtension { slot, path } = err else {
                panic!(
                    "expected CodePathNonComputeUnitYamlExtension for {relpath:?}, \
                     got {err:?}"
                );
            };
            assert_eq!(slot, ":servicos");
            assert_eq!(path, PathBuf::from(relpath));
        }
    }

    #[test]
    fn validate_code_paths_accepts_canonical_computeunit_yaml_shapes() {
        // Positive-control sweep through every canonical authoring shape
        // every in-tree fixture and the `Caixa::template` scaffold use.
        // Mirrors the peer
        // `validate_code_paths_accepts_canonical_lisp_shapes` (64772a9)
        // and the lifted predicate's own
        // `computeunit_yaml_extension_accepts_canonical_shapes` sweep in
        // render.rs.
        for relpath in [
            "servicos/demo.computeunit.yaml",
            "servicos/hello-rio.computeunit.yaml",
            "servicos/my-service.computeunit.yaml",
            "servicos/a.computeunit.yaml",
            "./servicos/demo.computeunit.yaml",
            "servicos/./demo.computeunit.yaml",
            "servicos/sub/nested.computeunit.yaml",
            "servicos/v0.1.computeunit.yaml",
        ] {
            let c = caixa_with_code_paths(vec![], vec![], vec![relpath]);
            c.validate_code_paths()
                .unwrap_or_else(|e| panic!("canonical shape {relpath:?} must pass, got {e:?}"));
        }
    }

    #[test]
    fn validate_code_paths_non_computeunit_yaml_extension_does_not_fire_on_bibliotecas_or_exe() {
        // The file-type gate is per-slot — only `:servicos` carries the
        // ComputeUnit-CR YAML contract. A canonical `.lisp` `:bibliotecas`
        // entry and an extensionless `:exe` entry are the canonical
        // shapes every in-tree fixture uses, and must continue to pass
        // validate. Peer of
        // `validate_code_paths_non_lisp_extension_does_not_fire_on_exe_or_servicos`
        // (64772a9) — together pin that the typed
        // [`CodePathFileType`] dispatch is exhaustively per-slot, with no
        // cross-axis leakage in either direction.
        let c = caixa_with_code_paths(
            vec!["lib/demo.lisp"],
            vec!["exe/demo", "exe/tool"],
            vec!["servicos/demo.computeunit.yaml"],
        );
        c.validate_code_paths().unwrap();
    }

    #[test]
    fn validate_code_paths_sandbox_shape_arms_precede_non_computeunit_yaml_extension() {
        // Cross-arm precedence pin: a `:servicos` entry that is *both*
        // sandbox-escaping and wrong-extension surfaces the more
        // fundamental sandbox-shape diagnostic first (the
        // `.computeunit.yaml` remediation would be misleading when the
        // offending path can never resolve under the caixa root
        // anyway). Mirrors the peer
        // `validate_code_paths_sandbox_shape_arms_precede_non_lisp_extension`
        // (64772a9) ordering on the sibling `:bibliotecas` axis and the
        // peer `EmptyPath` → `AbsolutePath` → `ParentEscape` →
        // `NonComputeUnitYamlExtension` arm-ordering the dispatch
        // table establishes.
        //
        // Empty wins (the strictly-smaller-scope structural arm).
        let c = caixa_with_code_paths(vec![], vec![], vec![""]);
        assert!(
            matches!(
                c.validate_code_paths().unwrap_err(),
                ManifestError::CodePathEmpty { slot: ":servicos" }
            ),
            "empty must win over non-computeunit-yaml-extension",
        );
        // Absolute wins (the path can't resolve under the caixa root).
        let c = caixa_with_code_paths(vec![], vec![], vec!["/etc/foo.yaml"]);
        let err = c.validate_code_paths().unwrap_err();
        let ManifestError::CodePathAbsolute { slot, .. } = err else {
            panic!("absolute must win over non-computeunit-yaml-extension, got {err:?}");
        };
        assert_eq!(slot, ":servicos");
        // ParentEscape wins (the path escapes the caixa root).
        let c = caixa_with_code_paths(vec![], vec![], vec!["../sibling/x.yaml"]);
        let err = c.validate_code_paths().unwrap_err();
        let ManifestError::CodePathParentEscape { slot, .. } = err else {
            panic!("parent-escape must win over non-computeunit-yaml-extension, got {err:?}");
        };
        assert_eq!(slot, ":servicos");
    }

    #[test]
    fn validate_code_paths_non_computeunit_yaml_extension_precedes_duplicate() {
        // Within-slot precedence pin: the per-entry file-type shape gate
        // fires before the cross-entry duplicate gate, so the narrower
        // structural defect dominates the uniqueness diagnostic. A
        // `("servicos/x.yaml" "servicos/x.yaml")` shape surfaces
        // `CodePathNonComputeUnitYamlExtension` on the first entry
        // rather than `CodePathDuplicate` on the pair — same posture
        // every per-entry shape-gate-precedes-duplicate cascade follows
        // on this surface, peer of the 64772a9 `:bibliotecas`
        // `("lib/x.txt" "lib/x.txt")` ordering.
        let c = caixa_with_code_paths(vec![], vec![], vec!["servicos/x.yaml", "servicos/x.yaml"]);
        let err = c.validate_code_paths().unwrap_err();
        let ManifestError::CodePathNonComputeUnitYamlExtension { slot, path } = err else {
            panic!("expected CodePathNonComputeUnitYamlExtension, got {err:?}");
        };
        assert_eq!(slot, ":servicos");
        assert_eq!(path, PathBuf::from("servicos/x.yaml"));
    }

    #[test]
    fn validate_code_paths_non_computeunit_yaml_extension_diagnostic_carries_offending_slot_and_path()
     {
        // Diagnostic-shape pin (peer with
        // `validate_code_paths_non_lisp_extension_diagnostic_carries_offending_slot_and_path`
        // on the sibling tatara-lisp-source axis): the file-type-arm
        // Display surfaces both the offending `:slot` tag, the
        // offending path verbatim, and the expected
        // `.computeunit.yaml` compound suffix named in the remediation
        // text, so a `feira lint` run can render the diagnostic without
        // re-parsing.
        let c = caixa_with_code_paths(vec![], vec![], vec!["servicos/demo.yaml"]);
        let rendered = c.validate_code_paths().unwrap_err().to_string();
        assert!(
            rendered.contains(":servicos"),
            "diagnostic must name the offending slot: {rendered}",
        );
        assert!(
            rendered.contains("servicos/demo.yaml"),
            "diagnostic must quote the offending path: {rendered}",
        );
        assert!(
            rendered.contains(".computeunit.yaml"),
            "diagnostic must name the expected compound suffix: {rendered}",
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
        // exact-string-match discipline. The shape gate this routine
        // landed (`is_chart_keyword_shape`, Cargo crates.io keyword
        // grammar) accepts mixed case — crates.io's keyword rule is
        // "case-insensitive" at the index layer but admits mixed case
        // at the entry layer (the canonical Helm chart `keywords:`
        // shape is lowercase by convention, but the grammar admits
        // uppercase). Case-sensitivity at the duplicate-set layer
        // remains structural — two distinct strings are two distinct
        // entries.
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

    #[test]
    fn validate_etiquetas_rejects_leading_whitespace_entry() {
        // Canonical paste-from-aligned-doc footgun. Without the shape
        // gate `" mesh"` silently passed validate and landed as a
        // YAML plain-style scalar with leading whitespace in the
        // rendered Chart.yaml `keywords:` array — every YAML 1.2
        // dumper trims leading whitespace from plain-style scalars,
        // so the authored space round-tripped inconsistently back
        // through `caixa.lisp`. Mirrors the peer
        // `validate_autores_rejects_leading_whitespace_entry`.
        let c = caixa_with_etiquetas(vec![" mesh"]);
        let err = c.validate_etiquetas().unwrap_err();
        let ManifestError::EtiquetaInvalid { etiqueta, reason } = err else {
            panic!("expected EtiquetaInvalid, got {err:?}");
        };
        assert_eq!(etiqueta, " mesh");
        assert!(reason.contains("whitespace"), "got: {reason}");
    }

    #[test]
    fn validate_etiquetas_rejects_embedded_newline_entry() {
        // Canonical paste-from-multiline-doc footgun — the author
        // pasted a multi-tag block into one `:etiquetas` entry
        // instead of splitting into one entry per tag. Without the
        // shape gate `"mesh\nhttp"` silently passed validate and
        // landed as a YAML-illegal multi-line scalar in the rendered
        // Chart.yaml `keywords:` array.
        let c = caixa_with_etiquetas(vec!["mesh\nhttp"]);
        let err = c.validate_etiquetas().unwrap_err();
        let ManifestError::EtiquetaInvalid { etiqueta, reason } = err else {
            panic!("expected EtiquetaInvalid, got {err:?}");
        };
        assert_eq!(etiqueta, "mesh\nhttp");
        assert!(reason.contains("newline"), "got: {reason}");
    }

    #[test]
    fn validate_etiquetas_rejects_embedded_comma_entry() {
        // Canonical CSV-list-separator-confusion footgun: the author
        // confused the CSV-style separator convention with the
        // `:etiquetas` list grammar. Without the shape gate
        // `"mesh,http,grpc"` silently passed validate and landed as a
        // single malformed search tag in the rendered Chart.yaml
        // `keywords:` array — Artifact Hub's keyword index would
        // either silently drop the tag or index it as
        // `mesh,http,grpc` instead of three separate tags.
        let c = caixa_with_etiquetas(vec!["mesh,http,grpc"]);
        let err = c.validate_etiquetas().unwrap_err();
        let ManifestError::EtiquetaInvalid { etiqueta, reason } = err else {
            panic!("expected EtiquetaInvalid, got {err:?}");
        };
        assert_eq!(etiqueta, "mesh,http,grpc");
        assert!(reason.contains('`'), "got: {reason}");
        assert!(reason.contains(','), "got: {reason}");
    }

    #[test]
    fn validate_etiquetas_rejects_embedded_slash_entry() {
        // Canonical path-separator-confusion footgun: the author
        // confused namespace-path notation with the keyword grammar.
        let c = caixa_with_etiquetas(vec!["caixa/servico"]);
        let err = c.validate_etiquetas().unwrap_err();
        let ManifestError::EtiquetaInvalid { etiqueta, reason } = err else {
            panic!("expected EtiquetaInvalid, got {err:?}");
        };
        assert_eq!(etiqueta, "caixa/servico");
        assert!(reason.contains('/'), "got: {reason}");
    }

    #[test]
    fn validate_etiquetas_rejects_leading_digit_entry() {
        // Canonical paste-from-numbered-list footgun: the author
        // copied `1. mesh` from a numbered doc and the `1` leaked
        // into the tag.
        let c = caixa_with_etiquetas(vec!["1mesh"]);
        let err = c.validate_etiquetas().unwrap_err();
        let ManifestError::EtiquetaInvalid { etiqueta, reason } = err else {
            panic!("expected EtiquetaInvalid, got {err:?}");
        };
        assert_eq!(etiqueta, "1mesh");
        assert!(reason.contains("digit"), "got: {reason}");
    }

    #[test]
    fn validate_etiquetas_rejects_leading_hyphen_entry() {
        // Canonical kebab-leak footgun.
        let c = caixa_with_etiquetas(vec!["-foo"]);
        let err = c.validate_etiquetas().unwrap_err();
        let ManifestError::EtiquetaInvalid { etiqueta, reason } = err else {
            panic!("expected EtiquetaInvalid, got {err:?}");
        };
        assert_eq!(etiqueta, "-foo");
        assert!(reason.contains('-'), "got: {reason}");
    }

    #[test]
    fn validate_etiquetas_rejects_non_ascii_entry() {
        // Canonical paste-from-Unicode-doc footgun. Every legitimate
        // search tag is strict ASCII; raw non-ASCII silently
        // round-trips inconsistently across NFC/NFD normalization on
        // APFS / case-folding filesystems and breaks the Artifact Hub
        // keyword search index lookup.
        let c = caixa_with_etiquetas(vec!["café"]);
        let err = c.validate_etiquetas().unwrap_err();
        let ManifestError::EtiquetaInvalid { etiqueta, reason } = err else {
            panic!("expected EtiquetaInvalid, got {err:?}");
        };
        assert_eq!(etiqueta, "café");
        assert!(reason.contains("non-ASCII"), "got: {reason}");
    }

    #[test]
    fn validate_etiquetas_rejects_period_entry() {
        // Canonical namespace-confusion / version-suffix footgun
        // (`"http.1"` / `"v1.0"`): Cargo's crates.io keyword grammar
        // excludes `.` from the continuation set even though the
        // sibling `:caracteristicas` axis (Cargo's feature-name
        // grammar) admits it. Tighter than the sibling axis, peer
        // with Cargo's own crates.io keyword shape.
        let c = caixa_with_etiquetas(vec!["http.1"]);
        let err = c.validate_etiquetas().unwrap_err();
        let ManifestError::EtiquetaInvalid { etiqueta, reason } = err else {
            panic!("expected EtiquetaInvalid, got {err:?}");
        };
        assert_eq!(etiqueta, "http.1");
        assert!(reason.contains('.'), "got: {reason}");
    }

    #[test]
    fn validate_etiquetas_empty_takes_precedence_over_shape() {
        // Per-entry empty-first cascade pin: an entry that is both
        // empty *and* shape-invalid surfaces `EtiquetaEmpty` (the
        // narrower "this entry has no value" structural defect
        // dominates the broader shape-predicate diagnostic). The
        // empty arm fires before the shape predicate is consulted,
        // mirroring the peer `validate_autores_empty_takes_precedence_over_shape`
        // cascade established on the sibling universal-axis Vec<String>
        // surface.
        let c = caixa_with_etiquetas(vec![""]);
        let err = c.validate_etiquetas().unwrap_err();
        assert!(matches!(err, ManifestError::EtiquetaEmpty), "got {err:?}",);
    }

    #[test]
    fn validate_etiquetas_shape_takes_precedence_over_duplicate() {
        // Per-entry shape-before-cross-entry-duplicate cascade pin: an
        // entry that is malformed surfaces `EtiquetaInvalid` even when
        // a later entry would have collided on duplicate. The
        // per-entry shape arm fires inside the same loop iteration as
        // the empty arm, before the seen-set insert at end-of-iteration
        // — structural per-entry defects dominate the cross-entry
        // uniqueness diagnostic. Mirrors the peer
        // `validate_autores_shape_takes_precedence_over_duplicate`.
        let c = caixa_with_etiquetas(vec!["mesh\nhttp", "mesh\nhttp"]);
        let err = c.validate_etiquetas().unwrap_err();
        assert!(
            matches!(err, ManifestError::EtiquetaInvalid { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_etiquetas_invalid_diagnostic_names_offending_slot_and_value() {
        // Diagnostic-shape pin on the new shape arm (peer with
        // `validate_autores_invalid_diagnostic_names_offending_slot_and_value`):
        // the rendered Display surfaces both the offending slot name
        // and the offending value verbatim, so a `feira lint` run
        // points the author at the exact `:etiquetas` entry to fix.
        let c = caixa_with_etiquetas(vec!["mesh\nhttp"]);
        let rendered = c.validate_etiquetas().unwrap_err().to_string();
        assert!(
            rendered.contains(":etiquetas"),
            "diagnostic must name the offending slot: {rendered}",
        );
        assert!(
            rendered.contains("mesh\\nhttp"),
            "diagnostic must quote the offending value (debug-escaped): {rendered}",
        );
    }

    #[test]
    fn validate_etiquetas_rejects_at_21_byte_boundary() {
        // The 20-byte cap pin — boundary-exceeding case rejected,
        // boundary-accepting case passes. Mirrors the peer
        // `chart_keyword_shape_rejects_at_21_byte_boundary` substrate-
        // side pin, surfaced at the per-axis caller so the cap
        // propagates through validate end-to-end. Constructed as a
        // single all-`a` token so only the cap arm fires.
        let max_ok = "a".repeat(20);
        let c = caixa_with_etiquetas(vec![max_ok.as_str()]);
        c.validate_etiquetas().unwrap();
        let too_long = "a".repeat(21);
        let c = caixa_with_etiquetas(vec![too_long.as_str()]);
        let err = c.validate_etiquetas().unwrap_err();
        let ManifestError::EtiquetaInvalid { reason, .. } = err else {
            panic!("expected EtiquetaInvalid, got {err:?}");
        };
        assert!(reason.contains("20"), "got: {reason}");
        assert!(reason.contains("21"), "got: {reason}");
    }

    #[test]
    fn validate_etiquetas_accepts_canonical_shaped_forms() {
        // Positive control sweep: every canonical-shaped tag from the
        // hello-rio / checkout-aplicacao / pangea-tatara-akeyless
        // example fixtures plus the substrate-fixed tags caixa-helm
        // unions in at chart render. Drift between this list and the
        // substrate-side `chart_keyword_shape_accepts_canonical_forms`
        // sweep surfaces here — one source of truth for the rule.
        let c = caixa_with_etiquetas(vec![
            "example",
            "aplicacao",
            "mesh",
            "ecommerce",
            "demo",
            "infrastructure",
            "aws",
            "akeyless",
            "pangea-native",
            "hello-world",
            "wasm",
            "rust",
            "tatara-lisp",
            "caixa-servico",
            "lareira",
        ]);
        c.validate_etiquetas().unwrap();
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

    #[test]
    fn validate_autores_rejects_leading_whitespace_entry() {
        // Canonical paste-from-aligned-doc footgun. Without the shape
        // gate `" pleme-io"` silently passed validate and landed as a
        // YAML plain-style scalar with leading whitespace in the
        // rendered Chart.yaml `maintainers:` array — every YAML 1.2
        // dumper trims leading whitespace from plain-style scalars, so
        // the authored space round-tripped inconsistently back through
        // `caixa.lisp`. Mirrors the peer
        // `validate_descricao_rejects_leading_whitespace`.
        let c = caixa_with_autores(vec![" pleme-io"]);
        let err = c.validate_autores().unwrap_err();
        let ManifestError::AutorInvalid { autor, reason } = err else {
            panic!("expected AutorInvalid, got {err:?}");
        };
        assert_eq!(autor, " pleme-io");
        assert!(reason.contains("whitespace"), "got: {reason}");
    }

    #[test]
    fn validate_autores_rejects_trailing_whitespace_entry() {
        // Canonical paste-from-doc footgun.
        let c = caixa_with_autores(vec!["pleme-io "]);
        let err = c.validate_autores().unwrap_err();
        let ManifestError::AutorInvalid { autor, reason } = err else {
            panic!("expected AutorInvalid, got {err:?}");
        };
        assert_eq!(autor, "pleme-io ");
        assert!(reason.contains("whitespace"), "got: {reason}");
    }

    #[test]
    fn validate_autores_rejects_embedded_newline_entry() {
        // Canonical paste-from-multiline-doc footgun — the author
        // pasted a multi-line block of author records into one
        // `:autores` entry instead of splitting into one entry per
        // author. Without the shape gate `"alice\nbob"` silently
        // passed validate and landed as a YAML-illegal multi-line
        // scalar in the rendered Chart.yaml `maintainers:` array.
        let c = caixa_with_autores(vec!["alice\nbob"]);
        let err = c.validate_autores().unwrap_err();
        let ManifestError::AutorInvalid { autor, reason } = err else {
            panic!("expected AutorInvalid, got {err:?}");
        };
        assert_eq!(autor, "alice\nbob");
        assert!(reason.contains("newline"), "got: {reason}");
    }

    #[test]
    fn validate_autores_rejects_embedded_carriage_return_entry() {
        // Canonical paste-from-Windows-CRLF-doc footgun.
        let c = caixa_with_autores(vec!["alice\rbob"]);
        let err = c.validate_autores().unwrap_err();
        let ManifestError::AutorInvalid { autor, reason } = err else {
            panic!("expected AutorInvalid, got {err:?}");
        };
        assert_eq!(autor, "alice\rbob");
        assert!(reason.contains("carriage return"), "got: {reason}");
    }

    #[test]
    fn validate_autores_rejects_embedded_tab_entry() {
        // Canonical tab-from-aligned-doc footgun.
        let c = caixa_with_autores(vec!["Pleme\tContributors"]);
        let err = c.validate_autores().unwrap_err();
        let ManifestError::AutorInvalid { autor, reason } = err else {
            panic!("expected AutorInvalid, got {err:?}");
        };
        assert_eq!(autor, "Pleme\tContributors");
        assert!(reason.contains("tab"), "got: {reason}");
    }

    #[test]
    fn validate_autores_rejects_embedded_control_bytes_entry() {
        // Paste-from-binary-blob footguns: NUL, BEL, ESC, DEL all
        // surface the same control-byte arm.
        for entry in [
            "alice\x00bob",
            "alice\x07bob",
            "alice\x1bbob",
            "alice\x7fbob",
        ] {
            let c = caixa_with_autores(vec![entry]);
            let err = c.validate_autores().unwrap_err();
            let ManifestError::AutorInvalid { autor, reason } = err else {
                panic!("expected AutorInvalid for {entry:?}, got {err:?}");
            };
            assert_eq!(autor, entry);
            assert!(
                reason.contains("control character"),
                "{entry:?} reason: {reason}",
            );
        }
    }

    #[test]
    fn validate_autores_accepts_unicode_entry() {
        // Unicode positive control: realistic maintainer names carry
        // Unicode (`François`, `日本語`, `naïve`). The predicate must
        // round-trip Unicode losslessly, peer with the
        // `chart_maintainer_name_shape_accepts_unicode` substrate-side
        // sweep.
        let c = caixa_with_autores(vec![
            "François Dupont",
            "日本語の名前",
            "naïve <naive@example.com>",
        ]);
        c.validate_autores().unwrap();
    }

    #[test]
    fn validate_autores_empty_takes_precedence_over_shape() {
        // Per-entry empty-first cascade pin: an entry that is both
        // empty *and* shape-invalid surfaces `AutorEmpty` (the narrower
        // "this entry has no value" structural defect dominates the
        // broader shape-predicate diagnostic). The empty arm fires
        // before the shape predicate is consulted, mirroring the peer
        // `validate_repositorio_empty_takes_precedence_over_shape`
        // cascade on the universal `Option<String>` siblings — and now
        // established on the Vec<String> per-entry surface.
        let c = caixa_with_autores(vec![""]);
        let err = c.validate_autores().unwrap_err();
        assert!(matches!(err, ManifestError::AutorEmpty), "got {err:?}",);
    }

    #[test]
    fn validate_autores_shape_takes_precedence_over_duplicate() {
        // Per-entry shape-before-cross-entry-duplicate cascade pin: an
        // entry that is malformed surfaces `AutorInvalid` even when a
        // later entry would have collided on duplicate. The per-entry
        // shape arm fires inside the same loop iteration as the empty
        // arm, before the seen-set insert at end-of-iteration —
        // structural per-entry defects dominate the cross-entry
        // uniqueness diagnostic.
        let c = caixa_with_autores(vec!["alice\nbob", "alice\nbob"]);
        let err = c.validate_autores().unwrap_err();
        assert!(
            matches!(err, ManifestError::AutorInvalid { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_autores_invalid_diagnostic_names_offending_slot_and_value() {
        // Diagnostic-shape pin on the new shape arm (peer with
        // `validate_descricao_invalid_diagnostic_carries_offending_value`):
        // the rendered Display surfaces both the offending slot name
        // and the offending value verbatim, so a `feira lint` run
        // points the author at the exact `:autores` entry to fix.
        let c = caixa_with_autores(vec!["alice\nbob"]);
        let rendered = c.validate_autores().unwrap_err().to_string();
        assert!(
            rendered.contains(":autores"),
            "diagnostic must name the offending slot: {rendered}",
        );
        assert!(
            rendered.contains("alice\\nbob"),
            "diagnostic must quote the offending value (debug-escaped): {rendered}",
        );
    }

    #[test]
    fn validate_autores_rejects_at_129_byte_boundary() {
        // The 128-byte cap pin — boundary-exceeding case rejected,
        // boundary-accepting case passes. Mirrors the peer
        // `chart_maintainer_name_shape_rejects_at_129_byte_boundary`
        // substrate-side pin, surfaced at the per-axis caller so the
        // cap propagates through validate end-to-end. Constructed as
        // a single all-`a` token so only the cap arm fires.
        let max_ok = "a".repeat(128);
        let c = caixa_with_autores(vec![max_ok.as_str()]);
        c.validate_autores().unwrap();
        let too_long = "a".repeat(129);
        let c = caixa_with_autores(vec![too_long.as_str()]);
        let err = c.validate_autores().unwrap_err();
        let ManifestError::AutorInvalid { reason, .. } = err else {
            panic!("expected AutorInvalid, got {err:?}");
        };
        assert!(reason.contains("128"), "got: {reason}");
        assert!(reason.contains("129"), "got: {reason}");
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
    fn validate_repositorio_rejects_fragment_anchor() {
        // Paste-from-browser-address-bar footgun on the
        // `:repositorio` axis — an author copies a GitHub permalink
        // to a README section / line-permalink and forgets to trim
        // the `#fragment` tail. The shared `is_git_repo_url`
        // predicate refuses the byte at the URL-grammar layer
        // (libcurl strips the fragment before opening the
        // transport, so the byte rides verbatim into the rendered
        // `Chart.yaml` `home:` and FluxCD `GitRepository` `url:`
        // fields but is silently dropped on the wire — two
        // manifest variants whose values differ only in their
        // fragment anchor lock to two distinct rendered artifacts
        // for the byte-identical clone, defeating the THEORY.md
        // §V.2 render-determinism contract on the `:repositorio`
        // axis the peer `:fonte :repo` axis already closes).
        let c = caixa_with_repositorio(Some("https://github.com/pleme-io/hello-rio#readme"));
        let err = c.validate_repositorio().unwrap_err();
        let ManifestError::RepositorioInvalid {
            repositorio,
            reason,
        } = err
        else {
            panic!("expected RepositorioInvalid, got {err:?}");
        };
        assert_eq!(repositorio, "https://github.com/pleme-io/hello-rio#readme");
        assert!(
            reason.contains("must not contain `#`"),
            "reason must surface the fragment-`#` arm, got {reason:?}"
        );
    }

    #[test]
    fn validate_repositorio_rejects_query_string() {
        // Paste-from-browser-address-bar footgun on the
        // `:repositorio` axis (peer with the a68f818 fragment-`#`
        // arm on the same axis). An author copies a GitHub tab
        // deep-link out of the address bar and forgets to trim
        // the `?tab=…` query tail. The shared `is_git_repo_url`
        // predicate refuses the byte at the URL-grammar layer
        // (GitHub / GitLab / Bitbucket silently ignore the
        // `?query` tail and serve the same repo regardless, so
        // the byte rides verbatim into the rendered `Chart.yaml`
        // `home:` and FluxCD `GitRepository` `url:` fields but
        // is silently masked at the wire — two manifest variants
        // whose values differ only in their query tail lock to
        // two distinct rendered artifacts for the byte-identical
        // clone, defeating the THEORY.md §V.2 render-determinism
        // contract on the `:repositorio` axis the peer `:fonte
        // :repo` axis already closes).
        let c = caixa_with_repositorio(Some(
            "https://github.com/pleme-io/hello-rio?tab=readme-ov-file",
        ));
        let err = c.validate_repositorio().unwrap_err();
        let ManifestError::RepositorioInvalid {
            repositorio,
            reason,
        } = err
        else {
            panic!("expected RepositorioInvalid, got {err:?}");
        };
        assert_eq!(
            repositorio,
            "https://github.com/pleme-io/hello-rio?tab=readme-ov-file"
        );
        assert!(
            reason.contains("must not contain `?`"),
            "reason must surface the query-`?` arm, got {reason:?}"
        );
    }

    #[test]
    fn validate_repositorio_rejects_embedded_backslash() {
        // Windows-file-path-confusion footgun on the `:repositorio`
        // axis (peer with the prior fragment-`#` / query-`?` arms on
        // the same axis, and peer with the new dep-level `:fonte :repo`
        // backslash arm on the URL-grammar trajectory). An author
        // pastes a Windows Explorer address-bar `file:///C:\Users\me\
        // hello-rio` into the `:repositorio` slot, expecting the
        // `lareira-<nome>` chart's `home:` field and the FluxCD
        // `GitRepository` `url:` field to render the canonical local
        // file-URI. The shared `is_git_repo_url` predicate refuses
        // the byte at the URL-grammar layer (libcurl silently
        // translates `\` → `/` on some platforms and refuses it on
        // others, so the byte rides verbatim into the rendered
        // artifacts but is silently rewritten or rejected at the wire
        // — two manifest variants whose values differ only in
        // backslash-vs-forward-slash lock to two distinct rendered
        // artifacts for the byte-identical clone, defeating the
        // THEORY.md §V.2 render-determinism contract on the
        // `:repositorio` axis the peer `:fonte :repo` axis already
        // closes).
        let c = caixa_with_repositorio(Some("file:///C:\\Users\\me\\hello-rio"));
        let err = c.validate_repositorio().unwrap_err();
        let ManifestError::RepositorioInvalid {
            repositorio,
            reason,
        } = err
        else {
            panic!("expected RepositorioInvalid, got {err:?}");
        };
        assert_eq!(repositorio, "file:///C:\\Users\\me\\hello-rio");
        assert!(
            reason.contains("must not contain `\\`"),
            "reason must surface the backslash-`\\` arm, got {reason:?}"
        );
    }

    #[test]
    fn validate_repositorio_rejects_uri_template_placeholder() {
        // URI Template (RFC 6570) placeholder footgun on the
        // `:repositorio` axis (peer with the prior fragment-`#` /
        // query-`?` / backslash-`\` arms on the same axis, and peer
        // with the new dep-level `:fonte :repo` `{` / `}` arm on the
        // URL-grammar trajectory). An author pastes a quick-start
        // README snippet / OpenAPI `servers:` URL / Helm chart
        // `home:` template carrying unresolved `{org}` / `{repo}`
        // placeholders into the `:repositorio` slot, expecting the
        // substrate to resolve the placeholder downstream. The
        // shared `is_git_repo_url` predicate refuses the byte at the
        // URL-grammar layer (libcurl percent-encodes `{` / `}` to
        // `%7B` / `%7D` on the wire, so the byte round-trips
        // inconsistently between the rendered `Chart.yaml home:` /
        // FluxCD `GitRepository url:` and the resolver's `git clone`
        // invocation, defeating the THEORY.md §V.2 render-
        // determinism contract on the `:repositorio` axis the peer
        // `:fonte :repo` axis already closes; every git porcelain
        // entry-point additionally fetches a nonexistent literal-
        // `{placeholder}`-named path far from the source caixa.lisp).
        let c = caixa_with_repositorio(Some("https://github.com/{org}/hello-rio"));
        let err = c.validate_repositorio().unwrap_err();
        let ManifestError::RepositorioInvalid {
            repositorio,
            reason,
        } = err
        else {
            panic!("expected RepositorioInvalid, got {err:?}");
        };
        assert_eq!(repositorio, "https://github.com/{org}/hello-rio");
        assert!(
            reason.contains("must not contain `{`"),
            "reason must surface the open-brace `{{` arm, got {reason:?}"
        );
        assert!(
            reason.contains("URI Template") || reason.contains("RFC 6570"),
            "reason must name the RFC 6570 URI Template grammar, got {reason:?}"
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
            "FIXME — describe this caixa",
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
    fn validate_descricao_rejects_leading_whitespace() {
        // Paste-from-aligned-doc footgun: a leading ASCII space the
        // bare empty-arm gate accepted, the shape predicate now
        // refuses. The diagnostic carries the offending value
        // verbatim (with the leading space preserved) so the author
        // can grep their caixa.lisp for the exact `:descricao` line
        // and fix the round-trip-inconsistent leading whitespace.
        // Mirrors the peer
        // `validate_licenca_rejects_leading_whitespace` arm on the
        // sibling `:licenca` axis.
        let c = caixa_with_descricao(Some(" Checkout flow."));
        let err = c.validate_descricao().unwrap_err();
        let ManifestError::DescricaoInvalid { descricao, reason } = err else {
            panic!("expected DescricaoInvalid, got {err:?}");
        };
        assert_eq!(descricao, " Checkout flow.");
        assert!(reason.contains("whitespace"), "got: {reason:?}");
    }

    #[test]
    fn validate_descricao_rejects_trailing_whitespace() {
        // Paste-from-doc footgun: a trailing ASCII space the bare
        // empty-arm gate accepted, the shape predicate now refuses.
        let c = caixa_with_descricao(Some("Checkout flow. "));
        let err = c.validate_descricao().unwrap_err();
        let ManifestError::DescricaoInvalid { descricao, reason } = err else {
            panic!("expected DescricaoInvalid, got {err:?}");
        };
        assert_eq!(descricao, "Checkout flow. ");
        assert!(reason.contains("whitespace"), "got: {reason:?}");
    }

    #[test]
    fn validate_descricao_rejects_embedded_newline() {
        // Paste-from-multiline-doc footgun: an embedded LF the bare
        // empty-arm gate accepted, the shape predicate now refuses.
        // Without this gate the embedded newline silently landed in
        // the rendered Chart.yaml as a multi-line YAML block scalar,
        // and every chart-aware UI (`helm list`, `helm search`,
        // Artifact Hub) renders the description in a single-line
        // column so the embedded newline is silently dropped at
        // every downstream consumer.
        let c = caixa_with_descricao(Some("Checkout\nflow."));
        let err = c.validate_descricao().unwrap_err();
        assert!(
            matches!(err, ManifestError::DescricaoInvalid { .. }),
            "got {err:?}",
        );
        assert!(err.to_string().contains("newline"), "got {err}");
    }

    #[test]
    fn validate_descricao_rejects_embedded_carriage_return() {
        // Paste-from-Windows-CRLF-doc footgun.
        let c = caixa_with_descricao(Some("Checkout\rflow."));
        let err = c.validate_descricao().unwrap_err();
        assert!(
            matches!(err, ManifestError::DescricaoInvalid { .. }),
            "got {err:?}",
        );
        assert!(err.to_string().contains("carriage return"), "got {err}");
    }

    #[test]
    fn validate_descricao_rejects_embedded_tab() {
        // Tab-from-aligned-doc footgun.
        let c = caixa_with_descricao(Some("Checkout\tflow."));
        let err = c.validate_descricao().unwrap_err();
        assert!(
            matches!(err, ManifestError::DescricaoInvalid { .. }),
            "got {err:?}",
        );
        assert!(err.to_string().contains("tab"), "got {err}");
    }

    #[test]
    fn validate_descricao_rejects_embedded_control_bytes() {
        // Paste-from-binary-blob footgun: every other control byte
        // (NUL, BEL, ESC, DEL) is refused at validate time. Mirrors
        // the peer SPDX-expression control-byte arm.
        for s in [
            "Checkout\x00flow.",
            "Checkout\x07flow.",
            "Checkout\x1bflow.",
            "Checkout\x7fflow.",
        ] {
            let c = caixa_with_descricao(Some(s));
            let err = c.validate_descricao().unwrap_err();
            assert!(
                matches!(err, ManifestError::DescricaoInvalid { .. }),
                "{s:?} got {err:?}",
            );
            assert!(
                err.to_string().contains("control character"),
                "{s:?} got {err}",
            );
        }
    }

    #[test]
    fn validate_descricao_accepts_unicode_prose() {
        // Positive control: Unicode prose is accepted — the
        // canonical fixtures carry `→` (U+2192) and `—` (U+2014),
        // and `Caixa::template`'s `"FIXME — describe this caixa"`
        // scaffold every `feira init` emits must continue to pass.
        for s in [
            "Canonical Rust→wasm32-wasip2 caixa Servico.",
            "FIXME — describe this caixa",
            "Caixa pour le projet tâche",
            "日本語の説明",
        ] {
            let c = caixa_with_descricao(Some(s));
            c.validate_descricao()
                .unwrap_or_else(|err| panic!("Unicode {s:?} must pass: {err:?}"));
        }
    }

    #[test]
    fn validate_descricao_empty_takes_precedence_over_shape() {
        // Cascade pin: a `Some("")` surfaces the narrower
        // `DescricaoEmpty` arm, not the broader `DescricaoInvalid`
        // shape-predicate arm. Mirrors the peer
        // `validate_licenca_empty_takes_precedence_over_shape` pin
        // on the sibling `:licenca` axis.
        let c = caixa_with_descricao(Some(""));
        let err = c.validate_descricao().unwrap_err();
        assert!(matches!(err, ManifestError::DescricaoEmpty), "got {err:?}",);
    }

    #[test]
    fn validate_descricao_invalid_diagnostic_carries_offending_value_and_slot() {
        // Diagnostic-shape pin: the error's Display surfaces both
        // the `:descricao` slot name and the offending value
        // verbatim, so a `feira lint` run can render the diagnostic
        // without re-parsing and the author can grep their caixa.lisp
        // for the offending `:descricao` line. Mirrors the peer
        // `validate_licenca_invalid_diagnostic_carries_offending_value_and_slot`
        // pin (ee2e888) on the sibling `:licenca` axis.
        // The `{descricao:?}` Debug format escapes embedded control
        // bytes; the quoted offending value surfaces as
        // `"Checkout\nflow."` (literal backslash-n) in the rendered
        // diagnostic. The author can grep their caixa.lisp for the
        // literal `Checkout` summary prefix.
        let c = caixa_with_descricao(Some("Checkout\nflow."));
        let rendered = c.validate_descricao().unwrap_err().to_string();
        assert!(
            rendered.contains(":descricao"),
            "diagnostic must name the offending slot: {rendered}",
        );
        assert!(
            rendered.contains("Checkout\\nflow."),
            "diagnostic must quote the offending value (debug-escaped): {rendered}",
        );
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

    // ── Caixa::licenca — outer top-level Option<&str> scalar accessor ──

    #[test]
    fn licenca_returns_licenca_byte_string_verbatim_across_permutations() {
        // The canonical per-`Caixa` `:licenca` SPDX-expression scalar
        // pin: [`Caixa::licenca`] must return the `:licenca` typed
        // byte-string verbatim as an `Option<&str>`, byte-equal to the
        // raw `self.licenca.as_deref()` access across every
        // representative value in the accept-set — `None` (the "omit
        // the slot to defer to the caixa-helm renderer's `MIT`
        // fallback" arm every existing fixture without a `:licenca`
        // line carries), `Some("")` (a past-the-guard sentinel that
        // pins the accessor doesn't perform a silent
        // `Some("") → None` collapse on the empty arm — validate
        // rejects `Some("")` through `LicencaEmpty` but the accessor
        // must ship the raw slot verbatim so a validate-time gate
        // regression surfaces at the caixa-helm emit boundary rather
        // than being silently absorbed into the fallback), `Some("MIT")`
        // (the canonical single-license shape every `feira init`
        // template scaffolds), `Some("Apache-2.0 OR MIT")` (the
        // canonical `OR`-compound shape the peer
        // `validate_licenca_accepts_canonical_expressions` positive
        // sweep exercises), `Some("(MIT OR Apache-2.0) AND
        // BSD-3-Clause")` (the canonical parenthesis-grouped shape),
        // `Some("MIT ")` / `Some(" MIT")` / `Some("MIT\n")` /
        // `Some("Apache_2.0")` / `Some("MIT,Apache-2.0")` (past-the-
        // guard sentinels — validate rejects each through
        // `LicencaInvalid` but the accessor must ship the raw slot
        // verbatim).
        //
        // First outer top-level [`Caixa`] `Option<&str>`-return scalar
        // accessor pin on the substrate primitive — opens the "outer
        // [`Caixa`] `Option<&str>` scalar" projection pattern the
        // sibling per-`Caixa` `:descricao` / `:repositorio` / `:edicao`
        // future lifts fold on. Sibling in shape to the peer per-`:placement`
        // [`crate::aplicacao::Placement::shard_key`] (7cd2a28) /
        // [`crate::aplicacao::Placement::affinity`] (74ec2d3) accessor
        // pins on the sibling per-M3-mesh-slot `Option<&str>`-return
        // axes, extended onto the outer top-level [`Caixa`] universal-
        // axis surface. Pins against a future silent detour that
        // returned an owned `Option<String>` (which would type-check
        // but silently allocate on every accessor call, breaking the
        // zero-cost projection every peer sibling accessor carries), a
        // `Some("") → None` collapse (which would silently absorb the
        // `LicencaEmpty` refusal case at the accessor boundary and the
        // caixa-helm emit path would silently fall back to `"MIT"` on
        // a struct-literal `Caixa { licenca: Some(""), .. }`), or a
        // `None → Some("MIT")` collapse (which would silently reify
        // the caixa-helm renderer's `"MIT"` fallback at the accessor
        // boundary and every downstream consumer keying off the
        // `Option::is_none()` discriminator would lose the "author
        // omitted the slot" signal).
        for licenca in [
            None,
            Some(""),
            Some("MIT"),
            Some("Apache-2.0 OR MIT"),
            Some("(MIT OR Apache-2.0) AND BSD-3-Clause"),
            Some("MIT "),
            Some(" MIT"),
            Some("MIT\n"),
            Some("Apache_2.0"),
            Some("MIT,Apache-2.0"),
        ] {
            let c = caixa_with_licenca(licenca);
            assert_eq!(
                c.licenca(),
                licenca,
                "Caixa::licenca must return :licenca verbatim (got {:?}, \
                 expected {licenca:?})",
                c.licenca(),
            );
            assert_eq!(
                c.licenca(),
                c.licenca.as_deref(),
                "Caixa::licenca must byte-equal the raw \
                 `self.licenca.as_deref()` field access across every \
                 value in the Option<&str> accept-set",
            );
        }
    }

    #[test]
    fn validate_licenca_empty_arm_routes_through_accessor() {
        // Composition pin: [`Caixa::validate_licenca`]'s empty-arm gate
        // must key off [`Caixa::licenca`], not the raw
        // `self.licenca.as_deref()` field access. Structurally: a
        // `Caixa { licenca: Some(""), .. }` must surface the
        // `LicencaEmpty` refusal exactly, and a
        // `Caixa { licenca: Some("MIT"), .. }` (the canonical
        // single-license form) must pass validate. The pair jointly
        // pins the accessor + validate-gate composition: any future
        // silent detour that had the accessor return `None` on the
        // empty arm (a `.filter(|s| !s.is_empty())` collapse) would
        // silently absorb the `LicencaEmpty` refusal at the accessor
        // boundary and the validate gate would accept a struct-literal
        // `Caixa { licenca: Some(""), .. }` — the composition pin
        // catches that at caixa-core build time.
        //
        // Peer of the per-`:politicas :circuit-breaker`
        // [`crate::aplicacao::CircuitBreaker::max_failures`] (3a74062)
        // accessor-composition pin
        // (`validate_politicas_max_failures_zero_floor_arm_routes_through_accessor`)
        // on the sibling per-M3-mesh-slot required-`u32` axis — same
        // "the validate / shape-gate predicate must route through the
        // substrate-primitive typed dispatch" discipline extended onto
        // the outer top-level [`Caixa`] universal-axis
        // `Option<&str>`-composition surface.
        let c = caixa_with_licenca(Some(""));
        assert!(
            matches!(c.validate_licenca(), Err(ManifestError::LicencaEmpty)),
            "validate_licenca must reject licenca == Some(\"\") with \
             LicencaEmpty — the accessor and the validate gate must \
             route through the same substrate-primitive typed dispatch \
             on the :licenca empty arm",
        );
        let c = caixa_with_licenca(Some("MIT"));
        assert!(
            c.validate_licenca().is_ok(),
            "validate_licenca must accept licenca == Some(\"MIT\") \
             (the canonical single-license SPDX shape)",
        );
    }

    #[test]
    fn licenca_projects_option_str_by_borrow() {
        // The by-borrow pin: [`Caixa::licenca`] returns
        // `Option<&str>` by borrow — the `&str` borrows the underlying
        // `String` storage of the `Option<String>` slot and the
        // accessor must not allocate a fresh `String` on every call.
        // Peer of the per-`:placement`
        // [`crate::aplicacao::Placement::shard_key`] (7cd2a28) by-
        // borrow pin on the peer per-M3-mesh-slot
        // `Option<&str>`-return axis, extended onto the outer top-
        // level [`Caixa`] universal-axis `Option<&str>` shape — the
        // accessor's returned `&str` must borrow from `&self` (the
        // returned reference's lifetime is tied to `&self`), and
        // calling the accessor twice on the same [`Caixa`] must yield
        // the same `Option<&str>` verbatim (idempotent, no side
        // effects on `&self`).
        //
        // Pins against a future silent detour that returned an owned
        // `Option<String>` (which would type-check but silently
        // allocate on every call, breaking the zero-cost projection
        // every peer sibling accessor carries), or a one-arm-only
        // accessor that returned a saturating value on some sentinel
        // input (breaking the pass-through invariant the sibling
        // required-scalar accessors carry).
        for licenca in [None, Some(""), Some("MIT"), Some("Apache-2.0 OR MIT")] {
            let c = caixa_with_licenca(licenca);
            let first = c.licenca();
            let second = c.licenca();
            assert_eq!(
                first, second,
                "Caixa::licenca must be idempotent — two successive \
                 calls on the same &self must return the same \
                 Option<&str>",
            );
            assert_eq!(
                first, licenca,
                "Caixa::licenca must return :licenca verbatim by \
                 borrow — got {first:?}, expected {licenca:?}",
            );
        }
    }

    // ── Caixa::repositorio — outer top-level Option<&str> scalar accessor ──

    #[test]
    fn repositorio_returns_repositorio_byte_string_verbatim_across_permutations() {
        // The canonical per-`Caixa` `:repositorio` git-repo-URL scalar
        // pin: [`Caixa::repositorio`] must return the `:repositorio`
        // typed byte-string verbatim as an `Option<&str>`, byte-equal
        // to the raw `self.repositorio.as_deref()` access across every
        // representative value in the accept-set — `None` (the "omit
        // the slot to defer to the per-renderer placeholder" arm every
        // existing fixture without a `:repositorio` line carries),
        // `Some("")` (a past-the-guard sentinel that pins the accessor
        // doesn't perform a silent `Some("") → None` collapse on the
        // empty arm — validate rejects `Some("")` through
        // `RepositorioEmpty` but the accessor must ship the raw slot
        // verbatim so a validate-time gate regression surfaces at the
        // caixa-helm / caixa-flux emit boundary rather than being
        // silently absorbed into the per-renderer fallback),
        // `Some("github:pleme-io/hello-rio")` (the canonical `github:`
        // shorthand every existing manifest fixture across
        // `caixa-helm` / `caixa-mesh` and the `examples/` uses),
        // `Some("https://github.com/pleme-io/checkout")` (the canonical
        // `https://` URL the README quickstart uses),
        // `Some("ssh://git@github.com/pleme-io/checkout.git")` /
        // `Some("git://github.com/pleme-io/checkout.git")` /
        // `Some("git@github.com:pleme-io/checkout.git")` /
        // `Some("file:///opt/mirrors/pleme-io/checkout")` (every non-
        // github scheme the shared `is_git_repo_url` predicate
        // documents), and five past-the-guard sentinels for the
        // `RepositorioInvalid` refusal cases (`Some("pleme-io/checkout")`
        // missing-colon, `Some("-upload-pack=evil")` leading-dash, /
        // `Some("github:pleme-io/checkout?ref=main")` query-string, /
        // `Some("github:pleme-io/checkout#main")` fragment-anchor, /
        // `Some("github:pleme-io/{tpl}")` URI-template-placeholder — the
        // sentinels pin the accessor doesn't silently absorb the
        // refusal cases into a fallback).
        //
        // Second outer top-level [`Caixa`] `Option<&str>`-return scalar
        // accessor pin on the substrate primitive — sibling of the peer
        // [`Caixa::licenca`] (6d5bc28) pin
        // (`licenca_returns_licenca_byte_string_verbatim_across_permutations`)
        // that opened the "outer [`Caixa`] `Option<&str>` scalar"
        // projection pin pattern this pin folds on. Sibling in shape to
        // the peer per-`:placement`
        // [`crate::aplicacao::Placement::shard_key`] (7cd2a28) /
        // [`crate::aplicacao::Placement::affinity`] (74ec2d3) accessor
        // pins on the sibling per-M3-mesh-slot `Option<&str>`-return
        // axes, extended onto the outer top-level [`Caixa`] universal-
        // axis surface. Pins against a future silent detour that
        // returned an owned `Option<String>` (which would type-check
        // but silently allocate on every accessor call, breaking the
        // zero-cost projection every peer sibling accessor carries), a
        // `Some("") → None` collapse (which would silently absorb the
        // `RepositorioEmpty` refusal case at the accessor boundary and
        // the caixa-helm `Chart.yaml` `home:` fold would silently
        // render a `home: null` / omitted field on a struct-literal
        // `Caixa { repositorio: Some(""), .. }`), or a
        // `None → Some(<default>)` collapse (which would silently reify
        // the per-renderer fallback at the accessor boundary and every
        // downstream consumer keying off the `Option::is_none()`
        // discriminator would lose the "author omitted the slot"
        // signal).
        for repositorio in [
            None,
            Some(""),
            Some("github:pleme-io/hello-rio"),
            Some("https://github.com/pleme-io/checkout"),
            Some("ssh://git@github.com/pleme-io/checkout.git"),
            Some("git://github.com/pleme-io/checkout.git"),
            Some("git@github.com:pleme-io/checkout.git"),
            Some("file:///opt/mirrors/pleme-io/checkout"),
            Some("pleme-io/checkout"),
            Some("-upload-pack=evil"),
            Some("github:pleme-io/checkout?ref=main"),
            Some("github:pleme-io/checkout#main"),
            Some("github:pleme-io/{tpl}"),
        ] {
            let c = caixa_with_repositorio(repositorio);
            assert_eq!(
                c.repositorio(),
                repositorio,
                "Caixa::repositorio must return :repositorio verbatim \
                 (got {:?}, expected {repositorio:?})",
                c.repositorio(),
            );
            assert_eq!(
                c.repositorio(),
                c.repositorio.as_deref(),
                "Caixa::repositorio must byte-equal the raw \
                 `self.repositorio.as_deref()` field access across every \
                 value in the Option<&str> accept-set",
            );
        }
    }

    #[test]
    fn validate_repositorio_empty_arm_routes_through_accessor() {
        // Composition pin: [`Caixa::validate_repositorio`]'s empty-arm
        // gate must key off [`Caixa::repositorio`], not the raw
        // `self.repositorio.as_deref()` field access. Structurally: a
        // `Caixa { repositorio: Some(""), .. }` must surface the
        // `RepositorioEmpty` refusal exactly, and a
        // `Caixa { repositorio: Some("github:pleme-io/hello-rio"), .. }`
        // (the canonical `github:` shorthand form) must pass validate.
        // The pair jointly pins the accessor + validate-gate
        // composition: any future silent detour that had the accessor
        // return `None` on the empty arm (a `.filter(|s| !s.is_empty())`
        // collapse) would silently absorb the `RepositorioEmpty` refusal
        // at the accessor boundary and the validate gate would accept a
        // struct-literal `Caixa { repositorio: Some(""), .. }` — the
        // composition pin catches that at caixa-core build time.
        //
        // Peer of the [`Caixa::licenca`] (6d5bc28)
        // `validate_licenca_empty_arm_routes_through_accessor`
        // composition pin on the sibling outer top-level [`Caixa`]
        // `Option<&str>` universal-axis surface — same "the validate /
        // shape-gate predicate must route through the substrate-
        // primitive typed dispatch" discipline extended onto the second
        // outer top-level [`Caixa`] universal-axis `Option<&str>`-
        // composition surface.
        let c = caixa_with_repositorio(Some(""));
        assert!(
            matches!(
                c.validate_repositorio(),
                Err(ManifestError::RepositorioEmpty),
            ),
            "validate_repositorio must reject repositorio == Some(\"\") \
             with RepositorioEmpty — the accessor and the validate gate \
             must route through the same substrate-primitive typed \
             dispatch on the :repositorio empty arm",
        );
        let c = caixa_with_repositorio(Some("github:pleme-io/hello-rio"));
        assert!(
            c.validate_repositorio().is_ok(),
            "validate_repositorio must accept repositorio == \
             Some(\"github:pleme-io/hello-rio\") (the canonical \
             `github:` shorthand git-repo-URL shape)",
        );
    }

    #[test]
    fn repositorio_projects_option_str_by_borrow() {
        // The by-borrow pin: [`Caixa::repositorio`] returns
        // `Option<&str>` by borrow — the `&str` borrows the underlying
        // `String` storage of the `Option<String>` slot and the
        // accessor must not allocate a fresh `String` on every call.
        // Peer of the per-`:placement`
        // [`crate::aplicacao::Placement::shard_key`] (7cd2a28) and the
        // [`Caixa::licenca`] (6d5bc28) by-borrow pins on the peer
        // `Option<&str>`-return axes, extended onto the second outer
        // top-level [`Caixa`] universal-axis `Option<&str>` shape —
        // the accessor's returned `&str` must borrow from `&self` (the
        // returned reference's lifetime is tied to `&self`), and
        // calling the accessor twice on the same [`Caixa`] must yield
        // the same `Option<&str>` verbatim (idempotent, no side effects
        // on `&self`).
        //
        // Pins against a future silent detour that returned an owned
        // `Option<String>` (which would type-check but silently
        // allocate on every call, breaking the zero-cost projection
        // every peer sibling accessor carries), or a one-arm-only
        // accessor that returned a saturating value on some sentinel
        // input (breaking the pass-through invariant the sibling
        // required-scalar accessors carry).
        for repositorio in [
            None,
            Some(""),
            Some("github:pleme-io/hello-rio"),
            Some("https://github.com/pleme-io/checkout"),
        ] {
            let c = caixa_with_repositorio(repositorio);
            let first = c.repositorio();
            let second = c.repositorio();
            assert_eq!(
                first, second,
                "Caixa::repositorio must be idempotent — two successive \
                 calls on the same &self must return the same \
                 Option<&str>",
            );
            assert_eq!(
                first, repositorio,
                "Caixa::repositorio must return :repositorio verbatim by \
                 borrow — got {first:?}, expected {repositorio:?}",
            );
        }
    }

    // ── Caixa::descricao — outer top-level Option<&str> scalar accessor ──

    #[test]
    fn descricao_returns_descricao_byte_string_verbatim_across_permutations() {
        // The canonical per-`Caixa` `:descricao` free-form-prose scalar
        // pin: [`Caixa::descricao`] must return the `:descricao` typed
        // byte-string verbatim as an `Option<&str>`, byte-equal to the
        // raw `self.descricao.as_deref()` access across every
        // representative value in the accept-set — `None` (the "omit
        // the slot to defer to the per-renderer `caixa.nome`-derived
        // fallback" arm every existing fixture without a `:descricao`
        // line carries), `Some("")` (a past-the-guard sentinel that
        // pins the accessor doesn't perform a silent `Some("") → None`
        // collapse on the empty arm — validate rejects `Some("")`
        // through `DescricaoEmpty` but the accessor must ship the raw
        // slot verbatim so a validate-time gate regression surfaces at
        // the caixa-helm / caixa-feira emit boundary rather than being
        // silently absorbed into the per-renderer `caixa.nome`-derived
        // fallback), `Some("Checkout flow.")` (the canonical one-line
        // prose descriptor the peer
        // `validate_descricao_accepts_canonical_value` positive sweep
        // exercises), `Some("Canonical Rust→wasm32-wasip2 caixa
        // Servico.")` (the multi-byte Unicode continuation-byte shape
        // the `hello-rio` fixture carries), `Some("→ — · ✓")` (a
        // multi-glyph Unicode shape the peer
        // `is_chart_description_shape` predicate accepts), and five
        // past-the-guard sentinels for the `DescricaoInvalid` refusal
        // cases (`Some(" Checkout flow.")` leading-whitespace,
        // `Some("Checkout flow. ")` trailing-whitespace,
        // `Some("Checkout\nflow.")` embedded-LF,
        // `Some("Checkout\tflow.")` embedded-TAB, and
        // `Some("Checkout\x00flow.")` embedded-NUL — the sentinels pin
        // the accessor doesn't silently absorb the refusal cases into
        // a fallback).
        //
        // Third outer top-level [`Caixa`] `Option<&str>`-return scalar
        // accessor pin on the substrate primitive — sibling of the peer
        // [`Caixa::licenca`] (6d5bc28) and [`Caixa::repositorio`]
        // (cc7332d) pins that opened the "outer [`Caixa`]
        // `Option<&str>` scalar" projection pin pattern this pin folds
        // on. Sibling in shape to the peer per-`:placement`
        // [`crate::aplicacao::Placement::shard_key`] (7cd2a28) /
        // [`crate::aplicacao::Placement::affinity`] (74ec2d3) accessor
        // pins on the sibling per-M3-mesh-slot `Option<&str>`-return
        // axes, extended onto the outer top-level [`Caixa`] universal-
        // axis surface. Pins against a future silent detour that
        // returned an owned `Option<String>` (which would type-check
        // but silently allocate on every accessor call, breaking the
        // zero-cost projection every peer sibling accessor carries), a
        // `Some("") → None` collapse (which would silently absorb the
        // `DescricaoEmpty` refusal case at the accessor boundary and
        // the caixa-helm `Chart.yaml` `description:` fold would
        // silently render a `caixa.nome`-derived fallback on a
        // struct-literal `Caixa { descricao: Some(""), .. }`), or a
        // `None → Some(<default>)` collapse (which would silently
        // reify the per-renderer `caixa.nome`-derived fallback at the
        // accessor boundary and every downstream consumer keying off
        // the `Option::is_none()` discriminator would lose the "author
        // omitted the slot" signal).
        for descricao in [
            None,
            Some(""),
            Some("Checkout flow."),
            Some("Canonical Rust→wasm32-wasip2 caixa Servico."),
            Some("→ — · ✓"),
            Some(" Checkout flow."),
            Some("Checkout flow. "),
            Some("Checkout\nflow."),
            Some("Checkout\tflow."),
            Some("Checkout\x00flow."),
        ] {
            let c = caixa_with_descricao(descricao);
            assert_eq!(
                c.descricao(),
                descricao,
                "Caixa::descricao must return :descricao verbatim (got \
                 {:?}, expected {descricao:?})",
                c.descricao(),
            );
            assert_eq!(
                c.descricao(),
                c.descricao.as_deref(),
                "Caixa::descricao must byte-equal the raw \
                 `self.descricao.as_deref()` field access across every \
                 value in the Option<&str> accept-set",
            );
        }
    }

    #[test]
    fn validate_descricao_empty_arm_routes_through_accessor() {
        // Composition pin: [`Caixa::validate_descricao`]'s empty-arm
        // gate must key off [`Caixa::descricao`], not the raw
        // `self.descricao.as_deref()` field access. Structurally: a
        // `Caixa { descricao: Some(""), .. }` must surface the
        // `DescricaoEmpty` refusal exactly, and a
        // `Caixa { descricao: Some("Checkout flow."), .. }` (the
        // canonical one-line-prose form) must pass validate. The pair
        // jointly pins the accessor + validate-gate composition: any
        // future silent detour that had the accessor return `None` on
        // the empty arm (a `.filter(|s| !s.is_empty())` collapse) would
        // silently absorb the `DescricaoEmpty` refusal at the accessor
        // boundary and the validate gate would accept a struct-literal
        // `Caixa { descricao: Some(""), .. }` — the composition pin
        // catches that at caixa-core build time.
        //
        // Peer of the [`Caixa::licenca`] (6d5bc28)
        // `validate_licenca_empty_arm_routes_through_accessor` and
        // [`Caixa::repositorio`] (cc7332d)
        // `validate_repositorio_empty_arm_routes_through_accessor`
        // composition pins on the sibling outer top-level [`Caixa`]
        // `Option<&str>` universal-axis surface — same "the validate /
        // shape-gate predicate must route through the substrate-
        // primitive typed dispatch" discipline extended onto the third
        // outer top-level [`Caixa`] universal-axis `Option<&str>`-
        // composition surface.
        let c = caixa_with_descricao(Some(""));
        assert!(
            matches!(c.validate_descricao(), Err(ManifestError::DescricaoEmpty),),
            "validate_descricao must reject descricao == Some(\"\") \
             with DescricaoEmpty — the accessor and the validate gate \
             must route through the same substrate-primitive typed \
             dispatch on the :descricao empty arm",
        );
        let c = caixa_with_descricao(Some("Checkout flow."));
        assert!(
            c.validate_descricao().is_ok(),
            "validate_descricao must accept descricao == \
             Some(\"Checkout flow.\") (the canonical one-line-prose \
             chart-description shape)",
        );
    }

    #[test]
    fn descricao_projects_option_str_by_borrow() {
        // The by-borrow pin: [`Caixa::descricao`] returns
        // `Option<&str>` by borrow — the `&str` borrows the underlying
        // `String` storage of the `Option<String>` slot and the
        // accessor must not allocate a fresh `String` on every call.
        // Peer of the [`Caixa::licenca`] (6d5bc28) and
        // [`Caixa::repositorio`] (cc7332d) by-borrow pins on the peer
        // outer top-level [`Caixa`] `Option<&str>`-return axes, and of
        // the per-`:placement`
        // [`crate::aplicacao::Placement::shard_key`] (7cd2a28) by-
        // borrow pin on the peer per-M3-mesh-slot `Option<&str>`-
        // return axis, extended onto the third outer top-level
        // [`Caixa`] universal-axis `Option<&str>` shape — the
        // accessor's returned `&str` must borrow from `&self` (the
        // returned reference's lifetime is tied to `&self`), and
        // calling the accessor twice on the same [`Caixa`] must yield
        // the same `Option<&str>` verbatim (idempotent, no side
        // effects on `&self`).
        //
        // Pins against a future silent detour that returned an owned
        // `Option<String>` (which would type-check but silently
        // allocate on every call, breaking the zero-cost projection
        // every peer sibling accessor carries), or a one-arm-only
        // accessor that returned a saturating value on some sentinel
        // input (breaking the pass-through invariant the sibling
        // required-scalar accessors carry).
        for descricao in [
            None,
            Some(""),
            Some("Checkout flow."),
            Some("Canonical Rust→wasm32-wasip2 caixa Servico."),
        ] {
            let c = caixa_with_descricao(descricao);
            let first = c.descricao();
            let second = c.descricao();
            assert_eq!(
                first, second,
                "Caixa::descricao must be idempotent — two successive \
                 calls on the same &self must return the same \
                 Option<&str>",
            );
            assert_eq!(
                first, descricao,
                "Caixa::descricao must return :descricao verbatim by \
                 borrow — got {first:?}, expected {descricao:?}",
            );
        }
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

    // ── Caixa::edicao — outer top-level Option<&str> scalar accessor ──

    #[test]
    fn edicao_returns_edicao_byte_string_verbatim_across_permutations() {
        // The canonical per-`Caixa` `:edicao` language-edition scalar
        // pin: [`Caixa::edicao`] must return the `:edicao` typed
        // byte-string verbatim as an `Option<&str>`, byte-equal to the
        // raw `self.edicao.as_deref()` access across every representative
        // value in the accept-set — `None` (the "omit the slot to defer
        // to the substrate's default edition" arm every existing
        // [`caixa-resolver`] fixture without an `:edicao` line carries),
        // `Some("")` (a past-the-guard sentinel that pins the accessor
        // doesn't perform a silent `Some("") → None` collapse on the
        // empty arm — validate rejects `Some("")` through `EdicaoEmpty`
        // but the accessor must ship the raw slot verbatim so a
        // validate-time gate regression surfaces at any future edition-
        // aware consumer's boundary rather than being silently absorbed
        // into the substrate's default edition), `Some("2026")` (the
        // canonical 4-digit-ASCII-decimal-year shape every `feira init`
        // template scaffolds via [`Caixa::template`] and every
        // renderer-side fixture at `caixa-helm/src/lib.rs:978` /
        // `caixa-flux/src/lib.rs:2319` / `caixa-mesh/src/lib.rs:3208`
        // carries by construction), `Some("2018")` / `Some("2021")` /
        // `Some("2024")` (canonical 4-digit-ASCII-decimal-year shapes
        // peer with Cargo's `[package] edition` grammar every future-
        // introduced sibling to `"2026"` will follow), and eight
        // past-the-guard sentinels for the `EdicaoInvalid` refusal cases
        // (`Some("2026 ")` trailing-whitespace, `Some(" 2026")` leading-
        // whitespace, `Some("2026\n")` embedded-LF, `Some("２０２６")`
        // fullwidth-non-ASCII-lookalike, `Some("v2026")` version-tag-
        // prefix, `Some("2026.1")` decimal-shape, `Some("26")` wrong-
        // length-numeric, `Some("latest")` free-form-non-year — the
        // sentinels pin the accessor doesn't silently absorb the
        // refusal cases into a substrate-default-edition fallback).
        //
        // Fourth and final outer top-level [`Caixa`] `Option<&str>`-
        // return scalar accessor pin on the substrate primitive —
        // sibling of the peer [`Caixa::licenca`] (6d5bc28),
        // [`Caixa::repositorio`] (cc7332d), and [`Caixa::descricao`]
        // (3f16e2f) pins that opened the "outer [`Caixa`]
        // `Option<&str>` scalar" projection pin pattern this pin folds
        // on. Sibling in shape to the peer per-`:placement`
        // [`crate::aplicacao::Placement::shard_key`] (7cd2a28) /
        // [`crate::aplicacao::Placement::affinity`] (74ec2d3) accessor
        // pins on the sibling per-M3-mesh-slot `Option<&str>`-return
        // axes, extended onto the outer top-level [`Caixa`] universal-
        // axis surface's last unlifted `Option<String>` slot. Pins
        // against a future silent detour that returned an owned
        // `Option<String>` (which would type-check but silently
        // allocate on every accessor call, breaking the zero-cost
        // projection every peer sibling accessor carries), a
        // `Some("") → None` collapse (which would silently absorb the
        // `EdicaoEmpty` refusal case at the accessor boundary and any
        // future edition-aware consumer would silently fall back to
        // the substrate's default edition on a struct-literal
        // `Caixa { edicao: Some(""), .. }`), or a
        // `None → Some("2026")` collapse (which would silently reify
        // the substrate's default edition at the accessor boundary
        // and every downstream consumer keying off the
        // `Option::is_none()` discriminator would lose the "author
        // omitted the slot" signal).
        for edicao in [
            None,
            Some(""),
            Some("2026"),
            Some("2018"),
            Some("2021"),
            Some("2024"),
            Some("2026 "),
            Some(" 2026"),
            Some("2026\n"),
            Some("２０２６"),
            Some("v2026"),
            Some("2026.1"),
            Some("26"),
            Some("latest"),
        ] {
            let c = caixa_with_edicao(edicao);
            assert_eq!(
                c.edicao(),
                edicao,
                "Caixa::edicao must return :edicao verbatim (got {:?}, \
                 expected {edicao:?})",
                c.edicao(),
            );
            assert_eq!(
                c.edicao(),
                c.edicao.as_deref(),
                "Caixa::edicao must byte-equal the raw \
                 `self.edicao.as_deref()` field access across every \
                 value in the Option<&str> accept-set",
            );
        }
    }

    #[test]
    fn validate_edicao_empty_arm_routes_through_accessor() {
        // Composition pin: [`Caixa::validate_edicao`]'s empty-arm gate
        // must key off [`Caixa::edicao`], not the raw
        // `self.edicao.as_deref()` field access. Structurally: a
        // `Caixa { edicao: Some(""), .. }` must surface the
        // `EdicaoEmpty` refusal exactly, and a
        // `Caixa { edicao: Some("2026"), .. }` (the canonical
        // 4-digit-ASCII-decimal-year form) must pass validate. The
        // pair jointly pins the accessor + validate-gate composition:
        // any future silent detour that had the accessor return `None`
        // on the empty arm (a `.filter(|s| !s.is_empty())` collapse)
        // would silently absorb the `EdicaoEmpty` refusal at the
        // accessor boundary and the validate gate would accept a
        // struct-literal `Caixa { edicao: Some(""), .. }` — the
        // composition pin catches that at caixa-core build time.
        //
        // Peer of the [`Caixa::licenca`] (6d5bc28)
        // `validate_licenca_empty_arm_routes_through_accessor`,
        // [`Caixa::repositorio`] (cc7332d)
        // `validate_repositorio_empty_arm_routes_through_accessor`,
        // and [`Caixa::descricao`] (3f16e2f)
        // `validate_descricao_empty_arm_routes_through_accessor`
        // composition pins on the sibling outer top-level [`Caixa`]
        // `Option<&str>` universal-axis surface — same "the validate /
        // shape-gate predicate must route through the substrate-
        // primitive typed dispatch" discipline extended onto the
        // fourth and final outer top-level [`Caixa`] universal-axis
        // `Option<&str>`-composition surface, closing the accessor-
        // composition family.
        let c = caixa_with_edicao(Some(""));
        assert!(
            matches!(c.validate_edicao(), Err(ManifestError::EdicaoEmpty)),
            "validate_edicao must reject edicao == Some(\"\") with \
             EdicaoEmpty — the accessor and the validate gate must \
             route through the same substrate-primitive typed dispatch \
             on the :edicao empty arm",
        );
        let c = caixa_with_edicao(Some("2026"));
        assert!(
            c.validate_edicao().is_ok(),
            "validate_edicao must accept edicao == Some(\"2026\") \
             (the canonical 4-digit-ASCII-decimal-year shape)",
        );
    }

    #[test]
    fn edicao_projects_option_str_by_borrow() {
        // The by-borrow pin: [`Caixa::edicao`] returns
        // `Option<&str>` by borrow — the `&str` borrows the underlying
        // `String` storage of the `Option<String>` slot and the
        // accessor must not allocate a fresh `String` on every call.
        // Peer of the [`Caixa::licenca`] (6d5bc28),
        // [`Caixa::repositorio`] (cc7332d), and [`Caixa::descricao`]
        // (3f16e2f) by-borrow pins on the peer outer top-level
        // [`Caixa`] `Option<&str>`-return axes, and of the
        // per-`:placement`
        // [`crate::aplicacao::Placement::shard_key`] (7cd2a28) by-
        // borrow pin on the peer per-M3-mesh-slot `Option<&str>`-
        // return axis, extended onto the fourth and final outer top-
        // level [`Caixa`] universal-axis `Option<&str>` shape — the
        // accessor's returned `&str` must borrow from `&self` (the
        // returned reference's lifetime is tied to `&self`), and
        // calling the accessor twice on the same [`Caixa`] must yield
        // the same `Option<&str>` verbatim (idempotent, no side
        // effects on `&self`).
        //
        // Pins against a future silent detour that returned an owned
        // `Option<String>` (which would type-check but silently
        // allocate on every call, breaking the zero-cost projection
        // every peer sibling accessor carries), or a one-arm-only
        // accessor that returned a saturating value on some sentinel
        // input (breaking the pass-through invariant the sibling
        // required-scalar accessors carry).
        for edicao in [None, Some(""), Some("2026"), Some("2018")] {
            let c = caixa_with_edicao(edicao);
            let first = c.edicao();
            let second = c.edicao();
            assert_eq!(
                first, second,
                "Caixa::edicao must be idempotent — two successive \
                 calls on the same &self must return the same \
                 Option<&str>",
            );
            assert_eq!(
                first, edicao,
                "Caixa::edicao must return :edicao verbatim by \
                 borrow — got {first:?}, expected {edicao:?}",
            );
        }
    }

    #[test]
    fn nome_returns_nome_byte_string_verbatim_across_permutations() {
        // The canonical per-`Caixa` `:nome` universal-axis DNS-1123-
        // label caixa-identity scalar pin: [`Caixa::nome`] must return
        // the `:nome` typed `String` verbatim as `&str`, byte-equal to
        // the raw field access across every representative value in
        // the accept-set — the canonical `"demo"` template baseline
        // (the same `feira init`-scaffolded default the sibling
        // `validate_nome_accepts_canonical_template` positive-control
        // gate pins), plus every sibling per-typed-slot atom accessor's
        // canonical positive-arm byte-string (`"catalog"` per
        // [`crate::aplicacao::Membro::nome`], `"cart"` per the peer
        // per-`:contratos` `:de`, `"hello-rio"` per the canonical
        // `caixa-helm`/`caixa-flux` cross-crate integration-test
        // fixture, `"checkout"` per the M3 mesh-slot Aplicacao
        // canonical example), plus every past-the-guard sentinel for
        // the `NomeEmpty` / `NomeInvalid` / `NomeChartNameBudgetExceeded`
        // refusal cases (`""`, `"Bad_Name"`, `"a"` × 56 — 56 bytes fits
        // the bare DNS-1123 63-byte cap but overflows the joint
        // `lareira-<nome>` chart-name budget the sibling
        // [`Caixa::validate_nome_chart_name_budget`] gate closes on).
        //
        // The past-the-guard sentinels pin the accessor doesn't
        // silently absorb the refusal cases into a template-derived
        // fallback (a future `.nome().is_empty().then(|| "demo")`
        // collapse would silently absorb the `NomeEmpty` refusal at
        // the accessor boundary and the validate gate would accept a
        // struct-literal `Caixa { nome: "".into(), .. }` — the pin
        // catches that at caixa-core build time).
        //
        // First outer top-level [`Caixa`] `&str`-return required-
        // scalar accessor pin — opens the "outer [`Caixa`] `&str`
        // required-scalar" projection pattern the sibling per-`Caixa`
        // `:versao` future lift folds on. Sibling in shape to the peer
        // per-`:membros` [`crate::aplicacao::Membro::nome`] (4a32abf)
        // required-`String`-carry accessor pin on the sibling per-
        // sub-struct required-axis, extended onto the outer top-level
        // [`Caixa`] universal-axis required-`String`-carry axis.
        for nome in [
            "demo",
            "catalog",
            "cart",
            "hello-rio",
            "checkout",
            "",
            "Bad_Name",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ] {
            let c = caixa_with_nome(nome);
            assert_eq!(
                c.nome(),
                nome,
                "Caixa::nome must return :nome verbatim (got {}, \
                 expected {nome})",
                c.nome(),
            );
            assert_eq!(
                c.nome(),
                c.nome.as_str(),
                "Caixa::nome must byte-equal the raw .nome field \
                 access across every value in the String accept-set",
            );
        }
    }

    #[test]
    fn validate_nome_empty_arm_routes_through_accessor() {
        // Composition pin: [`Caixa::validate_nome`]'s empty-arm must
        // key off [`Caixa::nome`], not the raw `.nome` field access.
        // Structurally: a `Caixa { nome: "".into(), .. }` must surface
        // the `NomeEmpty` refusal exactly, and the canonical `"demo"`
        // template baseline (the peer positive-arm the sibling
        // `validate_nome_accepts_canonical_template` gate carves out)
        // must pass validate. The pair jointly pins the accessor +
        // validate-gate composition: any future silent detour that
        // had the accessor return a fresh `"demo"` on the empty arm
        // (a `.nome().is_empty().then(|| "demo")` fallback collapse)
        // would silently absorb the `NomeEmpty` refusal at the
        // accessor boundary and the validate gate would accept a
        // struct-literal `Caixa { nome: "".into(), .. }` — the
        // composition pin catches that at caixa-core build time.
        //
        // Peer of the sibling per-`Caixa`
        // `validate_licenca_empty_arm_routes_through_accessor` (6d5bc28)
        // / `validate_repositorio_empty_arm_routes_through_accessor`
        // (cc7332d) / `validate_descricao_empty_arm_routes_through_accessor`
        // (3f16e2f) / `validate_edicao_empty_arm_routes_through_accessor`
        // (2641cbd) composition pins on the sibling outer top-level
        // [`Caixa`] `Option<&str>` axes — same "the validate /
        // shape-gate predicate must route through the substrate-
        // primitive typed dispatch" discipline extended onto the peer
        // outer top-level [`Caixa`] required-`&str` composition axis.
        let c = caixa_with_nome("");
        assert!(
            matches!(c.validate_nome(), Err(ManifestError::NomeEmpty)),
            "validate_nome must reject nome == \"\" with NomeEmpty — \
             the accessor and the validate gate must route through the \
             same substrate-primitive typed dispatch on the :nome \
             empty-arm",
        );
        let c = caixa_with_nome("demo");
        assert!(
            c.validate_nome().is_ok(),
            "validate_nome must accept nome == \"demo\" (the canonical \
             DNS-1123-label template baseline)",
        );
    }

    #[test]
    fn nome_projects_str_by_borrow() {
        // The by-borrow pin: [`Caixa::nome`] returns `&str` by borrow
        // — the `&str` borrows the underlying `String` storage of the
        // required `nome` slot and the accessor must not allocate a
        // fresh `String` on every call. Peer of the [`Caixa::licenca`]
        // (6d5bc28) / [`Caixa::repositorio`] (cc7332d) /
        // [`Caixa::descricao`] (3f16e2f) / [`Caixa::edicao`] (2641cbd)
        // by-borrow pins on the peer outer top-level [`Caixa`]
        // `Option<&str>`-return axes, extended onto the first outer
        // top-level [`Caixa`] required-`&str`-return axis — the
        // accessor's returned `&str` must borrow from `&self` (the
        // returned reference's lifetime is tied to `&self`), and
        // calling the accessor twice on the same [`Caixa`] must yield
        // the same `&str` verbatim (idempotent, no side effects on
        // `&self`).
        //
        // Pins against a future silent detour that returned an owned
        // `String` (which would type-check but silently allocate on
        // every call, breaking the zero-cost projection every peer
        // sibling accessor carries), an accidental
        // `.nome.to_lowercase()` detour that returned a fresh
        // allocation through an already-DNS-1123-lowercase-only
        // string (breaking a future `const fn` regression), or a
        // one-arm-only accessor that returned a canonicalized value
        // on some sentinel input (breaking the pass-through invariant
        // the sibling required-scalar accessors carry).
        for nome in ["demo", "catalog", "hello-rio", "checkout"] {
            let c = caixa_with_nome(nome);
            let first = c.nome();
            let second = c.nome();
            assert_eq!(
                first, second,
                "Caixa::nome must be idempotent — two successive calls \
                 on the same &self must return the same &str",
            );
            assert_eq!(
                first, nome,
                "Caixa::nome must return :nome verbatim by borrow — \
                 got {first}, expected {nome}",
            );
        }
    }

    #[test]
    fn versao_returns_versao_byte_string_verbatim_across_permutations() {
        // The canonical per-`Caixa` `:versao` universal-axis SemVer-2
        // pinned-version scalar pin: [`Caixa::versao`] must return the
        // `:versao` typed `String` verbatim as `&str`, byte-equal to the
        // raw `.versao` field access across every representative value
        // in the accept-set — the canonical `"0.1.0"` template baseline
        // (the same `feira init`-scaffolded default the sibling
        // `validate_versao_accepts_canonical_template` positive-control
        // gate pins), plus every canonical SemVer-2 shape the sibling
        // `validate_versao_accepts_canonical_forms` positive-arm sweep
        // covers (`"0.0.0"`, `"1.0.0"`, `"0.2.0-rc.1"`,
        // `"1.0.0-alpha.0"`, `"1.0.0+build.42"`, `"1.0.0-rc.1+build.42"`,
        // `"10.20.30"`), plus every past-the-guard sentinel for the
        // `VersaoEmpty` / `VersaoInvalid` refusal cases (`""` the empty
        // arm, `"v0.1.0"` the git-tag-shape-leak footgun, `"0.1"` the
        // missing-patch footgun, `"^0.1"` the requirement-shape-leak
        // footgun, `"0.1.0.0"` the four-part-Java-convention footgun,
        // `"latest"` the docker-tag-shape footgun — the sentinels pin
        // the accessor doesn't silently absorb the refusal cases into a
        // template-derived fallback like `"0.1.0"`).
        //
        // The past-the-guard sentinels pin the accessor doesn't silently
        // absorb the refusal cases into a template-derived fallback (a
        // future `.versao().is_empty().then(|| "0.1.0")` collapse would
        // silently absorb the `VersaoEmpty` refusal at the accessor
        // boundary and the validate gate would accept a struct-literal
        // `Caixa { versao: "".into(), .. }` — the pin catches that at
        // caixa-core build time).
        //
        // Second outer top-level [`Caixa`] `&str`-return required-scalar
        // accessor pin — folds on the "outer [`Caixa`] `&str` required-
        // scalar" projection pattern the sibling per-`Caixa`
        // [`Caixa::nome`] (e6b7d97) opened. Sibling in shape to the peer
        // per-`:membros` [`crate::aplicacao::Membro::versao_requirement`]
        // (4127bb6) / per-`:children`
        // [`crate::supervisor::ChildSpec::versao_requirement`] (2c053c8)
        // / per-`:upgrade-from`
        // [`crate::UpgradeFromEntry::prior_versao`] (75d27a8) per-sub-
        // struct `:versao`-shaped `&str`-return accessor pins on the
        // sibling per-typed-slot version-carrier axes, extended onto the
        // second outer top-level [`Caixa`] universal-axis required-
        // `String`-carry axis so the two universal-axis identity-
        // carrying scalars every `defcaixa` form supplies (`:nome` +
        // `:versao`) share the same "one typed dispatch per axis" pin
        // discipline.
        for versao in [
            "0.1.0",
            "0.0.0",
            "1.0.0",
            "0.2.0-rc.1",
            "1.0.0-alpha.0",
            "1.0.0+build.42",
            "1.0.0-rc.1+build.42",
            "10.20.30",
            "",
            "v0.1.0",
            "0.1",
            "^0.1",
            "0.1.0.0",
            "latest",
        ] {
            let c = caixa_with_versao(versao);
            assert_eq!(
                c.versao(),
                versao,
                "Caixa::versao must return :versao verbatim (got {}, \
                 expected {versao})",
                c.versao(),
            );
            assert_eq!(
                c.versao(),
                c.versao.as_str(),
                "Caixa::versao must byte-equal the raw .versao field \
                 access across every value in the String accept-set",
            );
        }
    }

    #[test]
    fn validate_versao_empty_arm_routes_through_accessor() {
        // Composition pin: [`Caixa::validate_versao`]'s empty-arm gate
        // must key off [`Caixa::versao`], not the raw `.versao` field
        // access. Structurally: a `Caixa { versao: "".into(), .. }` must
        // surface the `VersaoEmpty` refusal exactly, and the canonical
        // `"0.1.0"` template baseline (the peer positive-arm the sibling
        // `validate_versao_accepts_canonical_template` gate carves out)
        // must pass validate. The pair jointly pins the accessor +
        // validate-gate composition: any future silent detour that had
        // the accessor return a fresh `"0.1.0"` on the empty arm
        // (a `.versao().is_empty().then(|| "0.1.0")` fallback collapse)
        // would silently absorb the `VersaoEmpty` refusal at the
        // accessor boundary and the validate gate would accept a
        // struct-literal `Caixa { versao: "".into(), .. }` — the
        // composition pin catches that at caixa-core build time.
        //
        // Peer of the sibling per-`Caixa`
        // `validate_nome_empty_arm_routes_through_accessor` (e6b7d97)
        // composition pin on the sibling outer top-level [`Caixa`]
        // required-`&str` universal-axis surface — same "the validate /
        // shape-gate predicate must route through the substrate-
        // primitive typed dispatch" discipline extended onto the peer
        // outer top-level [`Caixa`] required-`&str` universal-axis
        // pinned-version composition axis, closing the second
        // coordinate of the "one canonical typed dispatch per per-Caixa
        // required-`&str` universal-axis" discipline.
        let c = caixa_with_versao("");
        assert!(
            matches!(c.validate_versao(), Err(ManifestError::VersaoEmpty)),
            "validate_versao must reject versao == \"\" with VersaoEmpty — \
             the accessor and the validate gate must route through the \
             same substrate-primitive typed dispatch on the :versao \
             empty-arm",
        );
        let c = caixa_with_versao("0.1.0");
        assert!(
            c.validate_versao().is_ok(),
            "validate_versao must accept versao == \"0.1.0\" (the \
             canonical SemVer-2 template baseline)",
        );
    }

    #[test]
    fn versao_projects_str_by_borrow() {
        // The by-borrow pin: [`Caixa::versao`] returns `&str` by borrow
        // — the `&str` borrows the underlying `String` storage of the
        // required `versao` slot and the accessor must not allocate a
        // fresh `String` on every call. Peer of the [`Caixa::nome`]
        // (e6b7d97) by-borrow pin on the sibling outer top-level
        // [`Caixa`] required-`&str`-return axis, extended onto the
        // second outer top-level [`Caixa`] required-`&str`-return
        // universal-axis pinned-version surface — the accessor's
        // returned `&str` must borrow from `&self` (the returned
        // reference's lifetime is tied to `&self`), and calling the
        // accessor twice on the same [`Caixa`] must yield the same
        // `&str` verbatim (idempotent, no side effects on `&self`).
        //
        // Pins against a future silent detour that returned an owned
        // `String` (which would type-check but silently allocate on
        // every call, breaking the zero-cost projection every peer
        // sibling accessor carries), an accidental
        // `semver::Version::parse(&self.versao).unwrap().to_string()`
        // detour that returned a canonicalized fresh allocation through
        // an already-canonical byte-string (breaking a future `const fn`
        // regression and silently absorbing the `VersaoInvalid` refusal
        // at the accessor boundary), or a one-arm-only accessor that
        // returned a canonicalized value on some sentinel input
        // (breaking the pass-through invariant the sibling required-
        // scalar accessors carry).
        for versao in ["0.1.0", "1.0.0", "0.2.0-rc.1", "1.0.0+build.42"] {
            let c = caixa_with_versao(versao);
            let first = c.versao();
            let second = c.versao();
            assert_eq!(
                first, second,
                "Caixa::versao must be idempotent — two successive \
                 calls on the same &self must return the same &str",
            );
            assert_eq!(
                first, versao,
                "Caixa::versao must return :versao verbatim by borrow \
                 — got {first}, expected {versao}",
            );
        }
    }

    fn caixa_with_kind(kind: CaixaKind) -> Caixa {
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.kind = kind;
        c
    }

    #[test]
    fn kind_returns_kind_variant_verbatim_across_permutations() {
        // The canonical per-`Caixa` `:kind` universal-axis closed-set-
        // enum discriminant pin: [`Caixa::kind`] must return the `:kind`
        // typed [`CaixaKind`] variant verbatim by `Copy`, byte-equal to
        // the raw `.kind` field access across every variant in the
        // closed accept-set (`Biblioteca` — the library kind that
        // exports lisp forms; `Binario` — the nix-built executable kind
        // under `exe/`; `Servico` — the wasm-component daemon kind
        // under `servicos/`; `Supervisor` — the OTP-shaped hierarchical
        // reconciliation kind; `Aplicacao` — the M3 typed-mesh
        // composition kind).
        //
        // Pins against a future silent detour that re-derived the kind
        // from a peer axis (an accidental fallback to
        // `if !servicos.is_empty() { Servico } else if
        // !membros.is_empty() { Aplicacao } else { Biblioteca }`
        // collapse that read the code-surface / mesh-slot columns into
        // the kind discriminator), a variant remap the operator
        // authors on one consumer without the other, or a stale-derive
        // detour that substituted [`CaixaKind::Biblioteca`] as the
        // default when the field held any other variant (which would
        // silently collapse the distinction between "author explicitly
        // declared `:kind Servico`" and "author declared any other
        // kind" every downstream renderer-dispatch site depends on).
        //
        // First outer top-level [`Caixa`] `Copy`-return required-enum-
        // discriminant accessor pin — opens the "outer [`Caixa`]
        // `Copy`-return required-discriminant" projection pattern.
        // Sibling in shape to the peer per-`:supervisor`
        // [`crate::supervisor::SupervisorSpec::estrategia`] (eafb619),
        // per-`:placement` [`crate::aplicacao::Placement::estrategia`]
        // (921fe1b), and per-`:children`
        // [`crate::supervisor::ChildSpec::restart`] (dfb4a81)
        // `Copy`-return closed-set-enum discriminant accessor pins on
        // the sibling nested-spec typed-slot discriminator axes,
        // extended here to the outer top-level [`Caixa`] universal-
        // axis surface.
        for kind in [
            CaixaKind::Biblioteca,
            CaixaKind::Binario,
            CaixaKind::Servico,
            CaixaKind::Supervisor,
            CaixaKind::Aplicacao,
        ] {
            let c = caixa_with_kind(kind);
            assert_eq!(
                c.kind(),
                kind,
                "Caixa::kind must return :kind verbatim (got {:?}, \
                 expected {kind:?})",
                c.kind(),
            );
            assert_eq!(
                c.kind(),
                c.kind,
                "Caixa::kind accessor and .kind field access must \
                 byte-equal — the accessor is the substrate-primitive \
                 typed dispatch every downstream kind-gate consumer \
                 must route through",
            );
        }
    }

    #[test]
    fn require_kind_reads_through_lifted_kind_accessor() {
        // Two-consumer coherence pin: the [`crate::render::require_kind`]
        // entry-gate predicate (the canonical two-line
        // `require_kind(caixa, Servico)?` prelude every per-Servico /
        // per-Aplicacao renderer runs at its entry-point) and the
        // sibling [`crate::render::KindMismatch`] error carrier's
        // `actual:` field (which names the offending caixa's variant
        // in the diagnostic) must both key off the lifted accessor, so
        // any future rebrand on the typed slot's reader shape lands at
        // exactly one place. Pins the two-site coherence by exercising
        // every off-diagonal `(actual, expected)` pair across the
        // closed accept-set — the `KindMismatch { actual, expected }`
        // surfaced on the mismatch arm must byte-equal the pair the
        // accessor returns for each side.
        //
        // Peer of the sibling per-`:placement`
        // `validate_placement_reads_through_lifted_estrategia_accessor`
        // (921fe1b) two-arm consumer-coherence pin on the M3 mesh-slot
        // `Copy`-return discriminant axis — same "the entry-gate
        // predicate and the error carrier's `actual:` field must route
        // through the substrate-primitive typed dispatch" discipline
        // extended onto the outer top-level [`Caixa`] universal-axis
        // discriminant surface.
        for expected in [
            CaixaKind::Biblioteca,
            CaixaKind::Binario,
            CaixaKind::Servico,
            CaixaKind::Supervisor,
            CaixaKind::Aplicacao,
        ] {
            for actual in [
                CaixaKind::Biblioteca,
                CaixaKind::Binario,
                CaixaKind::Servico,
                CaixaKind::Supervisor,
                CaixaKind::Aplicacao,
            ] {
                let c = caixa_with_kind(actual);
                let result = crate::render::require_kind(&c, expected);
                if expected == actual {
                    assert!(
                        result.is_ok(),
                        "require_kind must accept when actual == expected \
                         (actual={actual:?}, expected={expected:?})",
                    );
                } else {
                    let err = result.expect_err("require_kind must reject when actual != expected");
                    assert_eq!(
                        err.actual,
                        c.kind(),
                        "KindMismatch.actual must byte-equal Caixa::kind() \
                         — the error carrier's `actual:` field reads \
                         through the lifted accessor",
                    );
                    assert_eq!(
                        err.expected, expected,
                        "KindMismatch.expected must byte-equal the \
                         expected variant passed to require_kind",
                    );
                }
            }
        }
    }

    #[test]
    fn aplicacao_view_kind_gate_routes_through_accessor() {
        // Composition pin: [`Caixa::aplicacao_view`]'s kind-gate arm
        // must key off [`Caixa::kind`], not the raw `.kind` field
        // access. Structurally: a `Caixa { kind: X, .. }` for any
        // non-`Aplicacao` variant must fold to `None` on the
        // `aplicacao_view` composer (the "kind mismatch → no typed
        // view" contract every downstream Aplicacao consumer keys off
        // via `?`), and a `Caixa { kind: Aplicacao, .. }` must fold to
        // `Some(_)`. The pair jointly pins the accessor + view-gate
        // composition: any future silent detour that had the accessor
        // return a fresh [`CaixaKind::Aplicacao`] on some sentinel
        // input would silently absorb the kind-mismatch case at the
        // accessor boundary and every per-Aplicacao renderer would
        // silently render a non-Aplicacao caixa's mesh slots — the
        // composition pin catches that at caixa-core build time.
        //
        // Peer of the sibling per-`Caixa`
        // `validate_nome_empty_arm_routes_through_accessor` (e6b7d97) /
        // `validate_versao_empty_arm_routes_through_accessor` (20c0539)
        // composition pins on the sibling outer top-level [`Caixa`]
        // required-`&str` universal-axis surfaces — same "the
        // composer / validate gate must route through the substrate-
        // primitive typed dispatch" discipline extended onto the
        // outer top-level [`Caixa`] `Copy`-return required-
        // discriminant composition axis.
        for kind in [
            CaixaKind::Biblioteca,
            CaixaKind::Binario,
            CaixaKind::Servico,
            CaixaKind::Supervisor,
        ] {
            let c = caixa_with_kind(kind);
            assert!(
                c.aplicacao_view().is_none(),
                "aplicacao_view must return None on non-Aplicacao \
                 kind {kind:?} — the composer's kind-gate must route \
                 through Caixa::kind()",
            );
        }
        let c = caixa_with_kind(CaixaKind::Aplicacao);
        assert!(
            c.aplicacao_view().is_some(),
            "aplicacao_view must return Some on kind Aplicacao — \
             the composer's kind-gate must accept the matching arm \
             through Caixa::kind()",
        );
    }

    #[test]
    fn supervisor_view_kind_gate_routes_through_accessor() {
        // Composition pin (mirror of the sibling
        // `aplicacao_view_kind_gate_routes_through_accessor` on the
        // second `_view` composer): [`Caixa::supervisor_view`]'s kind-
        // gate arm must key off [`Caixa::kind`], not the raw `.kind`
        // field access. A `Caixa { kind: X, .. }` for any non-
        // `Supervisor` variant must fold to `None` on the
        // `supervisor_view` composer, and a `Caixa { kind:
        // Supervisor, .. }` must fold to `Some(_)`. Same peer
        // composition pin discipline on the second `_view` composer
        // axis.
        for kind in [
            CaixaKind::Biblioteca,
            CaixaKind::Binario,
            CaixaKind::Servico,
            CaixaKind::Aplicacao,
        ] {
            let c = caixa_with_kind(kind);
            assert!(
                c.supervisor_view().is_none(),
                "supervisor_view must return None on non-Supervisor \
                 kind {kind:?} — the composer's kind-gate must route \
                 through Caixa::kind()",
            );
        }
        let mut c = caixa_with_kind(CaixaKind::Supervisor);
        // A Supervisor caixa needs a strategy + at least one child to
        // fold to a Some(_) that also validates; the composer itself
        // requires only the kind arm, so bare kind flip is enough to
        // pin the `Some(_)` return, but we populate the minimum
        // supervisor shape so a future strengthening of the composer
        // to reject an empty spec doesn't false-positive this pin.
        c.estrategia = Some(crate::supervisor::RestartStrategy::OneForOne);
        c.children = vec![crate::supervisor::ChildSpec {
            caixa: "child".into(),
            versao: "^0.1".into(),
            restart: crate::supervisor::RestartPolicy::Permanent,
        }];
        assert!(
            c.supervisor_view().is_some(),
            "supervisor_view must return Some on kind Supervisor — \
             the composer's kind-gate must accept the matching arm \
             through Caixa::kind()",
        );
    }

    #[test]
    fn kind_projects_by_copy() {
        // The by-`Copy` pin: [`Caixa::kind`] returns a fresh
        // [`CaixaKind`] by `Copy` — the accessor must not borrow from
        // `&self` (the returned value is owned, `Copy`-projected from
        // the underlying [`CaixaKind`] storage; two calls on the same
        // [`Caixa`] must yield byte-equal values). Peer of the peer
        // per-`:placement` `Placement::estrategia` / per-`:supervisor`
        // `SupervisorSpec::estrategia` / per-`:children`
        // `ChildSpec::restart` `Copy`-return discriminant accessor
        // pins on the sibling nested-spec typed-slot discriminator
        // axes, extended onto the first outer top-level [`Caixa`]
        // required-`Copy`-return axis — pins against a future silent
        // detour that returned `&CaixaKind` (which would type-check
        // but silently constrain every consumer's callsite to a
        // borrow-shaped dispatch, breaking the zero-cost `Copy`
        // projection every peer sibling accessor carries).
        for kind in [
            CaixaKind::Biblioteca,
            CaixaKind::Binario,
            CaixaKind::Servico,
            CaixaKind::Supervisor,
            CaixaKind::Aplicacao,
        ] {
            let c = caixa_with_kind(kind);
            let first: CaixaKind = c.kind();
            let second: CaixaKind = c.kind();
            assert_eq!(
                first, second,
                "Caixa::kind must be idempotent — two successive \
                 calls on the same &self must return the same \
                 CaixaKind variant",
            );
            assert_eq!(
                first, kind,
                "Caixa::kind must return :kind verbatim by Copy — \
                 got {first:?}, expected {kind:?}",
            );
        }
    }

    // ── Caixa::autores — outer top-level &[T] slice accessor ──────────

    #[test]
    fn autores_returns_autores_slice_verbatim_across_permutations() {
        // The canonical per-`Caixa` `:autores` universal-axis maintainer-
        // name-list slice pin: [`Caixa::autores`] must return the
        // `:autores` typed [`Vec<String>`] list verbatim as a
        // `&[String]`, byte-equal to the raw `self.autores.as_slice()`
        // access across every representative value in the accept-set —
        // `[]` (the "no maintainers declared" arm every existing
        // fixture without an `:autores` line carries), `[""]` (a past-
        // the-guard sentinel that pins the accessor doesn't perform a
        // silent `[""] → []` collapse on the empty-entry arm — validate
        // rejects `[""]` through `AutorEmpty` but the accessor must
        // ship the raw slot verbatim so a validate-time gate regression
        // surfaces at the caixa-helm emit boundary rather than being
        // silently absorbed into a maintainer-drop), `["pleme-io"]` (the
        // canonical single-maintainer form every `feira init` template
        // scaffolds), `["alice", "bob"]` (a canonical multi-maintainer
        // form), `["alice <alice@example.com>", "bob <bob@example.com>"]`
        // (the canonical RFC-5322 `<name> <email>` form the
        // `is_chart_maintainer_name_shape` predicate accepts), and
        // `["pleme-io", "pleme-io"]` (a past-the-guard duplicate
        // sentinel — validate rejects through `AutorDuplicate` but the
        // accessor must ship the raw slot verbatim).
        //
        // First outer top-level [`Caixa`] `&[T]`-return slice accessor
        // pin on the substrate primitive — opens the "outer [`Caixa`]
        // `&[T]` slice" projection pattern the sibling per-`Caixa`
        // `:etiquetas` / `:deps` / `:deps-dev` / `:exe` / `:bibliotecas`
        // / `:servicos` / `:upgrade-from` / `:children` future lifts
        // fold on. Sibling in shape to the peer per-`:supervisor`
        // [`crate::supervisor::SupervisorSpec::children`] (bc92bce),
        // per-`:placement` [`crate::aplicacao::Placement::clusters`]
        // (a6e18d7), per-`:membros`
        // [`crate::aplicacao::AplicacaoSpec::membros`] (6c77e36),
        // per-`:contratos` [`crate::aplicacao::AplicacaoSpec::contratos`]
        // (0dcc926), and per-`:upgrade-from :instructions`
        // [`crate::upgrade::UpgradeFromEntry::instructions`] (0137e5a)
        // `&[T]`-return slice accessor pins on the sibling per-M2 /
        // per-M3 typed-slot list axes, extended onto the outer top-
        // level [`Caixa`] universal-axis surface. Pins against a future
        // silent detour that returned an owned `Vec<String>` (which
        // would type-check but silently clone on every accessor call,
        // breaking the zero-cost projection every peer sibling slice
        // accessor carries), a `[""] → []` collapse (which would
        // silently absorb the `AutorEmpty` refusal case at the accessor
        // boundary), or a `["a", "a"] → ["a"]` dedup collapse (which
        // would silently absorb the `AutorDuplicate` refusal case at
        // the accessor boundary and the caixa-helm `maintainers:` fold
        // would silently render a dedupped list on a struct-literal
        // `Caixa { autores: vec!["a".into(), "a".into()], .. }`).
        for autores in [
            vec![],
            vec![""],
            vec!["pleme-io"],
            vec!["alice", "bob"],
            vec!["alice <alice@example.com>", "bob <bob@example.com>"],
            vec!["pleme-io", "pleme-io"],
        ] {
            let c = caixa_with_autores(autores.clone());
            let expected: Vec<String> = autores.iter().map(|s| (*s).to_string()).collect();
            assert_eq!(
                c.autores(),
                expected.as_slice(),
                "Caixa::autores must return :autores verbatim (got {:?}, \
                 expected {expected:?})",
                c.autores(),
            );
            assert_eq!(
                c.autores(),
                c.autores.as_slice(),
                "Caixa::autores must byte-equal the raw \
                 `self.autores.as_slice()` field access across every \
                 value in the Vec<String> accept-set",
            );
        }
    }

    #[test]
    fn validate_autores_empty_entry_arm_routes_through_accessor() {
        // Composition pin: [`Caixa::validate_autores`]'s per-entry
        // empty-arm gate must key off [`Caixa::autores`], not the raw
        // `&self.autores` field-borrow walk. Structurally: a
        // `Caixa { autores: vec!["".into()], .. }` must surface the
        // `AutorEmpty` refusal exactly, and a
        // `Caixa { autores: vec!["pleme-io".into()], .. }` (the
        // canonical single-maintainer form) must pass validate. The
        // pair jointly pins the accessor + validate-gate composition:
        // any future silent detour that had the accessor return an
        // empty slice on the `[""]` arm (a
        // `.iter().filter(|s| !s.is_empty()).collect()` collapse)
        // would silently absorb the `AutorEmpty` refusal at the
        // accessor boundary and the validate gate would accept a
        // struct-literal `Caixa { autores: vec!["".into()], .. }` —
        // the composition pin catches that at caixa-core build time.
        //
        // Peer of the per-`Caixa` [`Caixa::validate_licenca`] (6d5bc28)
        // accessor-composition pin
        // (`validate_licenca_empty_arm_routes_through_accessor`) on the
        // sibling `Option<&str>`-composition axis and the
        // per-`:politicas :circuit-breaker`
        // [`crate::aplicacao::CircuitBreaker::max_failures`] (3a74062)
        // accessor-composition pin
        // (`validate_politicas_max_failures_zero_floor_arm_routes_through_accessor`)
        // on the sibling required-`u32`-composition axis — same "the
        // validate / shape-gate predicate must route through the
        // substrate-primitive typed dispatch" discipline extended onto
        // the outer top-level [`Caixa`] universal-axis `&[T]`-
        // composition surface.
        let c = caixa_with_autores(vec![""]);
        assert!(
            matches!(c.validate_autores(), Err(ManifestError::AutorEmpty)),
            "validate_autores must reject autores == vec![\"\"] with \
             AutorEmpty — the accessor and the validate gate must \
             route through the same substrate-primitive typed dispatch \
             on the :autores per-entry empty arm",
        );
        let c = caixa_with_autores(vec!["pleme-io"]);
        assert!(
            c.validate_autores().is_ok(),
            "validate_autores must accept autores == vec![\"pleme-io\"] \
             (the canonical single-maintainer shape every `feira init` \
             template scaffolds)",
        );
    }

    #[test]
    fn autores_projects_slice_by_borrow() {
        // The by-borrow pin: [`Caixa::autores`] returns `&[String]` by
        // borrow — the returned slice borrows the underlying
        // `Vec<String>` storage of the `:autores` slot and the
        // accessor must not clone the backing `Vec` on every call.
        // Peer of the per-`:membros`
        // [`crate::aplicacao::AplicacaoSpec::membros`] (6c77e36) /
        // per-`:contratos` [`crate::aplicacao::AplicacaoSpec::contratos`]
        // (0dcc926) / per-`:placement`
        // [`crate::aplicacao::Placement::clusters`] (a6e18d7) /
        // per-`:supervisor` [`crate::supervisor::SupervisorSpec::children`]
        // (bc92bce) by-borrow pins on the sibling per-M2 / per-M3
        // typed-slot `&[T]`-return axes, extended onto the outer top-
        // level [`Caixa`] universal-axis `&[String]` shape — the
        // accessor's returned slice must borrow from `&self` (the
        // returned reference's lifetime is tied to `&self`), and
        // calling the accessor twice on the same [`Caixa`] must yield
        // slices that are pointer-equal (the underlying byte-buffer is
        // the storage `Vec`'s allocation, not a fresh copy) as well as
        // value-equal (idempotent, no side effects on `&self`).
        //
        // Pins against a future silent detour that returned an owned
        // `Vec<String>` (which would type-check but silently clone on
        // every call, breaking the zero-cost projection every peer
        // sibling slice accessor carries), a `&Vec<String>` return
        // (which would leak the backing `Vec`'s grow/push/reserve
        // surface no downstream consumer reaches for), or a one-arm-
        // only accessor that returned a saturating value on some
        // sentinel input (breaking the pass-through invariant the
        // sibling slice accessors carry).
        for autores in [
            vec![],
            vec!["pleme-io"],
            vec!["alice", "bob"],
            vec!["pleme-io", "pleme-io"],
        ] {
            let c = caixa_with_autores(autores.clone());
            let expected: Vec<String> = autores.iter().map(|s| (*s).to_string()).collect();
            let first = c.autores();
            let second = c.autores();
            assert_eq!(
                first, second,
                "Caixa::autores must be idempotent — two successive \
                 calls on the same &self must return the same \
                 &[String]",
            );
            assert_eq!(
                first.as_ptr(),
                second.as_ptr(),
                "Caixa::autores must borrow the underlying Vec<String> \
                 storage — two successive calls must return slices \
                 with the same backing pointer (a fresh Vec<String> \
                 clone would change the pointer on every call)",
            );
            assert_eq!(
                first,
                expected.as_slice(),
                "Caixa::autores must return :autores verbatim by \
                 borrow — got {first:?}, expected {expected:?}",
            );
        }
    }

    // ── Caixa::etiquetas — outer top-level &[T] slice accessor ────────

    #[test]
    fn etiquetas_returns_etiquetas_slice_verbatim_across_permutations() {
        // The canonical per-`Caixa` `:etiquetas` universal-axis
        // registry-search-tag-list slice pin: [`Caixa::etiquetas`] must
        // return the `:etiquetas` typed [`Vec<String>`] list verbatim
        // as a `&[String]`, byte-equal to the raw
        // `self.etiquetas.as_slice()` access across every representative
        // value in the accept-set — `[]` (the "no tags declared" arm
        // every existing fixture without an `:etiquetas` line carries),
        // `[""]` (a past-the-guard sentinel that pins the accessor
        // doesn't perform a silent `[""] → []` collapse on the empty-
        // entry arm — validate rejects `[""]` through `EtiquetaEmpty`
        // but the accessor must ship the raw slot verbatim so a
        // validate-time gate regression surfaces at the caixa-helm emit
        // boundary rather than being silently absorbed into a keyword-
        // drop), `["demo"]` (the canonical single-tag form every
        // `feira init` template scaffolds), `["example", "aplicacao",
        // "mesh", "ecommerce", "demo"]` (the canonical multi-tag form
        // the checkout-aplicacao fixture emits), and `["demo", "demo"]`
        // (a past-the-guard duplicate sentinel — validate rejects
        // through `EtiquetaDuplicate` but the accessor must ship the
        // raw slot verbatim so the caixa-helm `BTreeSet::collect` dedup
        // at chart-render time isn't silently promoted into the
        // accessor boundary and struct-literal
        // `Caixa { etiquetas: vec!["demo".into(), "demo".into()], .. }`
        // fixtures continue to expose the duplicate at the accessor).
        //
        // Second outer top-level [`Caixa`] `&[T]`-return slice accessor
        // pin on the substrate primitive — folds on the "outer
        // [`Caixa`] `&[T]` slice" projection pattern
        // `autores_returns_autores_slice_verbatim_across_permutations`
        // (b5d813f) opened, sibling in shape and idiom. Pins against a
        // future silent detour that returned an owned `Vec<String>`
        // (which would type-check but silently clone on every accessor
        // call, breaking the zero-cost projection every peer sibling
        // slice accessor carries), a `[""] → []` collapse (which would
        // silently absorb the `EtiquetaEmpty` refusal case at the
        // accessor boundary), or a `["a", "a"] → ["a"]` dedup collapse
        // (which would silently absorb the `EtiquetaDuplicate` refusal
        // case at the accessor boundary — the caixa-helm chart-render
        // `BTreeSet::collect` dedup is downstream of the accessor and
        // must not be silently promoted into it).
        for etiquetas in [
            vec![],
            vec![""],
            vec!["demo"],
            vec!["example", "aplicacao", "mesh", "ecommerce", "demo"],
            vec!["demo", "demo"],
        ] {
            let c = caixa_with_etiquetas(etiquetas.clone());
            let expected: Vec<String> = etiquetas.iter().map(|s| (*s).to_string()).collect();
            assert_eq!(
                c.etiquetas(),
                expected.as_slice(),
                "Caixa::etiquetas must return :etiquetas verbatim (got \
                 {:?}, expected {expected:?})",
                c.etiquetas(),
            );
            assert_eq!(
                c.etiquetas(),
                c.etiquetas.as_slice(),
                "Caixa::etiquetas must byte-equal the raw \
                 `self.etiquetas.as_slice()` field access across every \
                 value in the Vec<String> accept-set",
            );
        }
    }

    #[test]
    fn validate_etiquetas_empty_entry_arm_routes_through_accessor() {
        // Composition pin: [`Caixa::validate_etiquetas`]'s per-entry
        // empty-arm gate must key off [`Caixa::etiquetas`], not the raw
        // `&self.etiquetas` field-borrow walk. Structurally: a
        // `Caixa { etiquetas: vec!["".into()], .. }` must surface the
        // `EtiquetaEmpty` refusal exactly, and a
        // `Caixa { etiquetas: vec!["demo".into()], .. }` (the canonical
        // single-tag form) must pass validate. The pair jointly pins
        // the accessor + validate-gate composition: any future silent
        // detour that had the accessor return an empty slice on the
        // `[""]` arm (a
        // `.iter().filter(|s| !s.is_empty()).collect()` collapse) would
        // silently absorb the `EtiquetaEmpty` refusal at the accessor
        // boundary and the validate gate would accept a struct-literal
        // `Caixa { etiquetas: vec!["".into()], .. }` — the composition
        // pin catches that at caixa-core build time.
        //
        // Peer of the per-`Caixa` `validate_autores_empty_arm_routes_
        // through_accessor` (b5d813f) accessor-composition pin on the
        // sibling `&[T]`-composition axis — same "the validate / shape-
        // gate predicate must route through the substrate-primitive
        // typed dispatch" discipline extended onto the sibling outer
        // top-level [`Caixa`] `&[T]`-composition surface.
        let c = caixa_with_etiquetas(vec![""]);
        assert!(
            matches!(c.validate_etiquetas(), Err(ManifestError::EtiquetaEmpty)),
            "validate_etiquetas must reject etiquetas == vec![\"\"] \
             with EtiquetaEmpty — the accessor and the validate gate \
             must route through the same substrate-primitive typed \
             dispatch on the :etiquetas per-entry empty arm",
        );
        let c = caixa_with_etiquetas(vec!["demo"]);
        assert!(
            c.validate_etiquetas().is_ok(),
            "validate_etiquetas must accept etiquetas == vec![\"demo\"] \
             (the canonical single-tag shape every `feira init` \
             template scaffolds)",
        );
    }

    #[test]
    fn etiquetas_projects_slice_by_borrow() {
        // The by-borrow pin: [`Caixa::etiquetas`] returns `&[String]`
        // by borrow — the returned slice borrows the underlying
        // `Vec<String>` storage of the `:etiquetas` slot and the
        // accessor must not clone the backing `Vec` on every call.
        // Peer of the per-`Caixa` `autores_projects_slice_by_borrow`
        // (b5d813f) by-borrow pin on the sibling outer top-level
        // [`Caixa`] `&[String]`-return axis — the accessor's returned
        // slice must borrow from `&self` (the returned reference's
        // lifetime is tied to `&self`), and calling the accessor twice
        // on the same [`Caixa`] must yield slices that are pointer-
        // equal (the underlying byte-buffer is the storage `Vec`'s
        // allocation, not a fresh copy) as well as value-equal
        // (idempotent, no side effects on `&self`).
        //
        // Pins against a future silent detour that returned an owned
        // `Vec<String>` (which would type-check but silently clone on
        // every call, breaking the zero-cost projection every peer
        // sibling slice accessor carries), a `&Vec<String>` return
        // (which would leak the backing `Vec`'s grow/push/reserve
        // surface no downstream consumer reaches for), or a one-arm-
        // only accessor that returned a saturating value on some
        // sentinel input (breaking the pass-through invariant the
        // sibling slice accessors carry).
        for etiquetas in [
            vec![],
            vec!["demo"],
            vec!["example", "aplicacao", "mesh"],
            vec!["demo", "demo"],
        ] {
            let c = caixa_with_etiquetas(etiquetas.clone());
            let expected: Vec<String> = etiquetas.iter().map(|s| (*s).to_string()).collect();
            let first = c.etiquetas();
            let second = c.etiquetas();
            assert_eq!(
                first, second,
                "Caixa::etiquetas must be idempotent — two successive \
                 calls on the same &self must return the same \
                 &[String]",
            );
            assert_eq!(
                first.as_ptr(),
                second.as_ptr(),
                "Caixa::etiquetas must borrow the underlying \
                 Vec<String> storage — two successive calls must \
                 return slices with the same backing pointer (a fresh \
                 Vec<String> clone would change the pointer on every \
                 call)",
            );
            assert_eq!(
                first,
                expected.as_slice(),
                "Caixa::etiquetas must return :etiquetas verbatim by \
                 borrow — got {first:?}, expected {expected:?}",
            );
        }
    }

    // ── Caixa::bibliotecas — outer top-level &[T] slice accessor ──────

    #[test]
    fn bibliotecas_returns_bibliotecas_slice_verbatim_across_permutations() {
        // The canonical per-`Caixa` `:bibliotecas` universal-axis
        // library-source-path-list slice pin: [`Caixa::bibliotecas`]
        // must return the `:bibliotecas` typed [`Vec<String>`] list
        // verbatim as a `&[String]`, byte-equal to the raw
        // `self.bibliotecas.as_slice()` access across every
        // representative value in the accept-set — `[]` (the "no
        // libraries declared" arm every `:kind` other than `Biblioteca`
        // + every `Biblioteca` relying on the canonical
        // `lib/<nome>.lisp` implicit-default path carries; the
        // layout's [`crate::LayoutInvariants`] `MissingLib` arm-gate
        // fires exactly on this empty-slot + `Biblioteca`-kind
        // combination), `[""]` (a past-the-guard sentinel that pins
        // the accessor doesn't perform a silent `[""] → []` collapse
        // on the empty-entry arm — validate rejects `[""]` through
        // `CodePathEmpty { slot: ":bibliotecas" }` but the accessor
        // must ship the raw slot verbatim so a validate-time gate
        // regression surfaces at the `feira build` phase-1 parse
        // boundary rather than being silently absorbed into a
        // library-drop), `["lib/demo.lisp"]` (the canonical single-
        // entry form `Caixa::template` scaffolds and every `feira init`
        // template emits), `["lib/demo.lisp", "lib/helpers.lisp"]`
        // (the canonical multi-library form the
        // `validate_code_paths_accepts_explicit_relative_paths_on_
        // every_slot` fixture emits), and `["lib/foo.lisp",
        // "lib/foo.lisp"]` (a past-the-guard duplicate sentinel —
        // validate rejects through `CodePathDuplicate { slot:
        // ":bibliotecas" }` per the per-slot set-not-multiset gate,
        // but the accessor must ship the raw slot verbatim so the
        // `feira build` `for entry in caixa.bibliotecas()` parse walk
        // sees the duplicate at the accessor boundary and struct-
        // literal `Caixa { bibliotecas: vec!["lib/foo.lisp".into(),
        // "lib/foo.lisp".into()], .. }` fixtures continue to expose
        // the duplicate at the accessor).
        //
        // Third outer top-level [`Caixa`] `&[T]`-return slice accessor
        // pin on the substrate primitive — folds on the "outer
        // [`Caixa`] `&[T]` slice" projection pattern
        // `autores_returns_autores_slice_verbatim_across_permutations`
        // (b5d813f) opened and
        // `etiquetas_returns_etiquetas_slice_verbatim_across_permutations`
        // (78c7d3c) folded on, sibling in shape and idiom. Pins
        // against a future silent detour that returned an owned
        // `Vec<String>` (which would type-check but silently clone on
        // every accessor call, breaking the zero-cost projection
        // every peer sibling slice accessor carries), a `[""] → []`
        // collapse (which would silently absorb the `CodePathEmpty`
        // refusal case at the accessor boundary), or a `["lib/foo.lisp",
        // "lib/foo.lisp"] → ["lib/foo.lisp"]` dedup collapse (which
        // would silently absorb the `CodePathDuplicate` refusal case
        // at the accessor boundary — the per-slot set-not-multiset
        // gate is downstream of the accessor and must not be silently
        // promoted into it).
        for bibliotecas in [
            vec![],
            vec![""],
            vec!["lib/demo.lisp"],
            vec!["lib/demo.lisp", "lib/helpers.lisp"],
            vec!["lib/foo.lisp", "lib/foo.lisp"],
        ] {
            let c = caixa_with_code_paths(bibliotecas.clone(), vec![], vec![]);
            let expected: Vec<String> = bibliotecas.iter().map(|s| (*s).to_string()).collect();
            assert_eq!(
                c.bibliotecas(),
                expected.as_slice(),
                "Caixa::bibliotecas must return :bibliotecas verbatim \
                 (got {:?}, expected {expected:?})",
                c.bibliotecas(),
            );
            assert_eq!(
                c.bibliotecas(),
                c.bibliotecas.as_slice(),
                "Caixa::bibliotecas must byte-equal the raw \
                 `self.bibliotecas.as_slice()` field access across \
                 every value in the Vec<String> accept-set",
            );
        }
    }

    #[test]
    fn validate_code_paths_bibliotecas_empty_arm_routes_through_accessor() {
        // Composition pin: [`Caixa::validate_code_paths`]'s per-entry
        // empty-arm gate on the `:bibliotecas` slot must key off
        // [`Caixa::bibliotecas`], not a divergent raw
        // `&self.bibliotecas` field-borrow walk. Structurally: a
        // `Caixa { bibliotecas: vec!["".into()], .. }` must surface
        // the `CodePathEmpty { slot: ":bibliotecas" }` refusal
        // exactly, and a `Caixa { bibliotecas: vec!["lib/demo.lisp".
        // into()], .. }` (the canonical single-library form
        // `Caixa::template` scaffolds) must pass validate. The pair
        // jointly pins the accessor + validate-gate composition: any
        // future silent detour that had the accessor return an empty
        // slice on the `[""]` arm (a `.iter().filter(|s|
        // !s.is_empty()).collect()` collapse) would silently absorb
        // the `CodePathEmpty` refusal at the accessor boundary and
        // the validate gate would accept a struct-literal
        // `Caixa { bibliotecas: vec!["".into()], .. }` — the
        // composition pin catches that at caixa-core build time.
        //
        // Peer of the per-`Caixa` `validate_autores_empty_arm_routes_
        // through_accessor` (b5d813f) and
        // `validate_etiquetas_empty_entry_arm_routes_through_accessor`
        // (78c7d3c) accessor-composition pins on the sibling `&[T]`-
        // composition axes — same "the validate / shape-gate
        // predicate must route through the substrate-primitive typed
        // dispatch" discipline extended onto the sibling outer top-
        // level [`Caixa`] `&[T]`-composition surface. Nominally the
        // in-tree `validate_code_paths` production body still keys
        // off the internal `[(":bibliotecas", &self.bibliotecas,
        // CodePathFileType::LispSource), (":exe", &self.exe, ..),
        // (":servicos", &self.servicos, ..)]` per-slot dispatch tuple
        // (the tuple's homogeneous slice-typed shape blocks a per-
        // element accessor swap in isolation — a future companion
        // lift for `:exe` and `:servicos` on the same outer-`Caixa`
        // `&[T]` slice-accessor axis closes that tuple onto the
        // triple of typed dispatches as a unit); the composition pin
        // catches any future accessor-side silent filter drop against
        // that eventual tuple-closure regardless of whether the
        // `:bibliotecas` slot is threaded through the accessor or the
        // raw field access at the tuple's construction site.
        let c = caixa_with_code_paths(vec![""], vec![], vec![]);
        assert!(
            matches!(
                c.validate_code_paths(),
                Err(ManifestError::CodePathEmpty {
                    slot: ":bibliotecas"
                })
            ),
            "validate_code_paths must reject bibliotecas == vec![\"\"] \
             with CodePathEmpty {{ slot: \":bibliotecas\" }} — the \
             accessor and the validate gate must route through the \
             same substrate-primitive typed dispatch on the \
             :bibliotecas per-entry empty arm",
        );
        let c = caixa_with_code_paths(vec!["lib/demo.lisp"], vec![], vec![]);
        assert!(
            c.validate_code_paths().is_ok(),
            "validate_code_paths must accept bibliotecas == \
             vec![\"lib/demo.lisp\"] (the canonical single-library \
             shape every `feira init` template scaffolds)",
        );
    }

    #[test]
    fn bibliotecas_projects_slice_by_borrow() {
        // The by-borrow pin: [`Caixa::bibliotecas`] returns
        // `&[String]` by borrow — the returned slice borrows the
        // underlying `Vec<String>` storage of the `:bibliotecas` slot
        // and the accessor must not clone the backing `Vec` on every
        // call. Peer of the per-`Caixa` `autores_projects_slice_by_borrow`
        // (b5d813f) and `etiquetas_projects_slice_by_borrow` (78c7d3c)
        // by-borrow pins on the sibling outer top-level [`Caixa`]
        // `&[String]`-return axes — the accessor's returned slice
        // must borrow from `&self` (the returned reference's lifetime
        // is tied to `&self`), and calling the accessor twice on the
        // same [`Caixa`] must yield slices that are pointer-equal
        // (the underlying byte-buffer is the storage `Vec`'s
        // allocation, not a fresh copy) as well as value-equal
        // (idempotent, no side effects on `&self`).
        //
        // Pins against a future silent detour that returned an owned
        // `Vec<String>` (which would type-check but silently clone on
        // every call, breaking the zero-cost projection every peer
        // sibling slice accessor carries), a `&Vec<String>` return
        // (which would leak the backing `Vec`'s grow/push/reserve
        // surface no downstream consumer reaches for), or a one-arm-
        // only accessor that returned a saturating value on some
        // sentinel input (breaking the pass-through invariant the
        // sibling slice accessors carry).
        for bibliotecas in [
            vec![],
            vec!["lib/demo.lisp"],
            vec!["lib/demo.lisp", "lib/helpers.lisp"],
            vec!["lib/foo.lisp", "lib/foo.lisp"],
        ] {
            let c = caixa_with_code_paths(bibliotecas.clone(), vec![], vec![]);
            let expected: Vec<String> = bibliotecas.iter().map(|s| (*s).to_string()).collect();
            let first = c.bibliotecas();
            let second = c.bibliotecas();
            assert_eq!(
                first, second,
                "Caixa::bibliotecas must be idempotent — two \
                 successive calls on the same &self must return the \
                 same &[String]",
            );
            assert_eq!(
                first.as_ptr(),
                second.as_ptr(),
                "Caixa::bibliotecas must borrow the underlying \
                 Vec<String> storage — two successive calls must \
                 return slices with the same backing pointer (a \
                 fresh Vec<String> clone would change the pointer on \
                 every call)",
            );
            assert_eq!(
                first,
                expected.as_slice(),
                "Caixa::bibliotecas must return :bibliotecas verbatim \
                 by borrow — got {first:?}, expected {expected:?}",
            );
        }
    }

    // ── Caixa::exe — outer top-level &[T] slice accessor ──────────────

    #[test]
    fn exe_returns_exe_slice_verbatim_across_permutations() {
        // The canonical per-`Caixa` `:exe` universal-axis
        // nix-built-executable-entry-path-list slice pin: [`Caixa::exe`]
        // must return the `:exe` typed [`Vec<String>`] list verbatim as
        // a `&[String]`, byte-equal to the raw `self.exe.as_slice()`
        // access across every representative value in the accept-set —
        // `[]` (the "no executable declared" arm every `:kind` other
        // than `Binario` carries; the layout's [`crate::LayoutInvariants`]
        // `BinarioWithoutExe` arm-gate fires exactly on this empty-slot
        // + `Binario`-kind combination), `[""]` (a past-the-guard
        // sentinel that pins the accessor doesn't perform a silent
        // `[""] → []` collapse on the empty-entry arm — validate rejects
        // `[""]` through `CodePathEmpty { slot: ":exe" }` but the
        // accessor must ship the raw slot verbatim so a validate-time
        // gate regression surfaces at the layout / `feira nix` boundary
        // rather than being silently absorbed into an executable-drop),
        // `["exe/cli"]` (the canonical single-entry Binario form every
        // in-tree `caixa_with_code_paths` positive control uses),
        // `["exe/cli", "exe/serve"]` (the canonical multi-executable
        // form the `validate_code_paths_accepts_explicit_relative_paths_
        // on_every_slot` fixture emits), and `["exe/cli", "exe/cli"]`
        // (a past-the-guard duplicate sentinel — validate rejects
        // through `CodePathDuplicate { slot: ":exe" }` per the per-slot
        // set-not-multiset gate, but the accessor must ship the raw
        // slot verbatim so struct-literal `Caixa { exe: vec!["exe/cli".
        // into(), "exe/cli".into()], .. }` fixtures continue to expose
        // the duplicate at the accessor).
        //
        // Fourth outer top-level [`Caixa`] `&[T]`-return slice accessor
        // pin on the substrate primitive — folds on the "outer
        // [`Caixa`] `&[T]` slice" projection pattern
        // `autores_returns_autores_slice_verbatim_across_permutations`
        // (b5d813f) opened,
        // `etiquetas_returns_etiquetas_slice_verbatim_across_permutations`
        // (78c7d3c) folded on, and
        // `bibliotecas_returns_bibliotecas_slice_verbatim_across_permutations`
        // (8a36c23) closed the universal-axis text-tag family of.
        // Opens the outer-`Caixa` foreign-code-slot `&[T]` sub-family
        // the sibling `:servicos` future lift closes onto. Pins against
        // a future silent detour that returned an owned `Vec<String>`
        // (which would type-check but silently clone on every accessor
        // call, breaking the zero-cost projection every peer sibling
        // slice accessor carries), a `[""] → []` collapse (which would
        // silently absorb the `CodePathEmpty` refusal case at the
        // accessor boundary), or an `["exe/cli", "exe/cli"] →
        // ["exe/cli"]` dedup collapse (which would silently absorb the
        // `CodePathDuplicate` refusal case at the accessor boundary —
        // the per-slot set-not-multiset gate is downstream of the
        // accessor and must not be silently promoted into it).
        for exe in [
            vec![],
            vec![""],
            vec!["exe/cli"],
            vec!["exe/cli", "exe/serve"],
            vec!["exe/cli", "exe/cli"],
        ] {
            let c = caixa_with_code_paths(vec![], exe.clone(), vec![]);
            let expected: Vec<String> = exe.iter().map(|s| (*s).to_string()).collect();
            assert_eq!(
                c.exe(),
                expected.as_slice(),
                "Caixa::exe must return :exe verbatim (got {:?}, \
                 expected {expected:?})",
                c.exe(),
            );
            assert_eq!(
                c.exe(),
                c.exe.as_slice(),
                "Caixa::exe must byte-equal the raw \
                 `self.exe.as_slice()` field access across every value \
                 in the Vec<String> accept-set",
            );
        }
    }

    #[test]
    fn validate_code_paths_exe_empty_arm_routes_through_accessor() {
        // Composition pin: [`Caixa::validate_code_paths`]'s per-entry
        // empty-arm gate on the `:exe` slot must key off
        // [`Caixa::exe`], not a divergent raw `&self.exe` field-borrow
        // walk. Structurally: a `Caixa { exe: vec!["".into()], .. }`
        // must surface the `CodePathEmpty { slot: ":exe" }` refusal
        // exactly, and a `Caixa { exe: vec!["exe/cli".into()], .. }`
        // (the canonical single-executable form every in-tree
        // `caixa_with_code_paths` positive control uses) must pass
        // validate. The pair jointly pins the accessor + validate-gate
        // composition: any future silent detour that had the accessor
        // return an empty slice on the `[""]` arm (a
        // `.iter().filter(|s| !s.is_empty()).collect()` collapse) would
        // silently absorb the `CodePathEmpty` refusal at the accessor
        // boundary and the validate gate would accept a struct-literal
        // `Caixa { exe: vec!["".into()], .. }` — the composition pin
        // catches that at caixa-core build time.
        //
        // Peer of the per-`Caixa`
        // `validate_code_paths_bibliotecas_empty_arm_routes_through_accessor`
        // (8a36c23), `validate_autores_empty_arm_routes_through_accessor`
        // (b5d813f), and
        // `validate_etiquetas_empty_entry_arm_routes_through_accessor`
        // (78c7d3c) accessor-composition pins on the sibling `&[T]`-
        // composition axes — same "the validate / shape-gate predicate
        // must route through the substrate-primitive typed dispatch"
        // discipline extended onto the sibling outer top-level [`Caixa`]
        // `&[T]`-composition surface. Nominally the in-tree
        // `validate_code_paths` production body still keys off the
        // internal `[(":bibliotecas", &self.bibliotecas,
        // CodePathFileType::LispSource), (":exe", &self.exe, ..),
        // (":servicos", &self.servicos, ..)]` per-slot dispatch tuple
        // (the tuple's homogeneous slice-typed shape blocks a per-
        // element accessor swap in isolation — a future companion lift
        // for `:servicos` on the same outer-`Caixa` `&[T]` slice-
        // accessor axis closes that tuple onto the triple of typed
        // dispatches as a unit); the composition pin catches any future
        // accessor-side silent filter drop against that eventual tuple-
        // closure regardless of whether the `:exe` slot is threaded
        // through the accessor or the raw field access at the tuple's
        // construction site.
        let c = caixa_with_code_paths(vec![], vec![""], vec![]);
        assert!(
            matches!(
                c.validate_code_paths(),
                Err(ManifestError::CodePathEmpty { slot: ":exe" })
            ),
            "validate_code_paths must reject exe == vec![\"\"] \
             with CodePathEmpty {{ slot: \":exe\" }} — the \
             accessor and the validate gate must route through the \
             same substrate-primitive typed dispatch on the \
             :exe per-entry empty arm",
        );
        let c = caixa_with_code_paths(vec![], vec!["exe/cli"], vec![]);
        assert!(
            c.validate_code_paths().is_ok(),
            "validate_code_paths must accept exe == vec![\"exe/cli\"] \
             (the canonical single-executable shape every in-tree \
             `caixa_with_code_paths` positive control uses)",
        );
    }

    #[test]
    fn exe_projects_slice_by_borrow() {
        // The by-borrow pin: [`Caixa::exe`] returns `&[String]` by
        // borrow — the returned slice borrows the underlying
        // `Vec<String>` storage of the `:exe` slot and the accessor
        // must not clone the backing `Vec` on every call. Peer of the
        // per-`Caixa` `autores_projects_slice_by_borrow` (b5d813f),
        // `etiquetas_projects_slice_by_borrow` (78c7d3c), and
        // `bibliotecas_projects_slice_by_borrow` (8a36c23) by-borrow
        // pins on the sibling outer top-level [`Caixa`] `&[String]`-
        // return axes — the accessor's returned slice must borrow from
        // `&self` (the returned reference's lifetime is tied to
        // `&self`), and calling the accessor twice on the same
        // [`Caixa`] must yield slices that are pointer-equal (the
        // underlying byte-buffer is the storage `Vec`'s allocation,
        // not a fresh copy) as well as value-equal (idempotent, no
        // side effects on `&self`).
        //
        // Pins against a future silent detour that returned an owned
        // `Vec<String>` (which would type-check but silently clone on
        // every call, breaking the zero-cost projection every peer
        // sibling slice accessor carries), a `&Vec<String>` return
        // (which would leak the backing `Vec`'s grow/push/reserve
        // surface no downstream consumer reaches for), or a one-arm-
        // only accessor that returned a saturating value on some
        // sentinel input (breaking the pass-through invariant the
        // sibling slice accessors carry).
        for exe in [
            vec![],
            vec!["exe/cli"],
            vec!["exe/cli", "exe/serve"],
            vec!["exe/cli", "exe/cli"],
        ] {
            let c = caixa_with_code_paths(vec![], exe.clone(), vec![]);
            let expected: Vec<String> = exe.iter().map(|s| (*s).to_string()).collect();
            let first = c.exe();
            let second = c.exe();
            assert_eq!(
                first, second,
                "Caixa::exe must be idempotent — two successive calls \
                 on the same &self must return the same &[String]",
            );
            assert_eq!(
                first.as_ptr(),
                second.as_ptr(),
                "Caixa::exe must borrow the underlying Vec<String> \
                 storage — two successive calls must return slices \
                 with the same backing pointer (a fresh Vec<String> \
                 clone would change the pointer on every call)",
            );
            assert_eq!(
                first,
                expected.as_slice(),
                "Caixa::exe must return :exe verbatim by borrow — \
                 got {first:?}, expected {expected:?}",
            );
        }
    }

    // ── Caixa::servicos — outer top-level &[T] slice accessor ─────────

    #[test]
    fn servicos_returns_servicos_slice_verbatim_across_permutations() {
        // The canonical per-`Caixa` `:servicos` universal-axis
        // ComputeUnit-CR-YAML-entry-path-list slice pin:
        // [`Caixa::servicos`] must return the `:servicos` typed
        // [`Vec<String>`] list verbatim as a `&[String]`, byte-equal to
        // the raw `self.servicos.as_slice()` access across every
        // representative value in the accept-set — `[]` (the "no
        // ComputeUnit-CR declared" arm every `:kind` other than
        // `Servico` carries; the layout's [`crate::LayoutInvariants`]
        // `ServicoWithoutServicos` arm-gate fires exactly on this
        // empty-slot + `Servico`-kind combination), `[""]` (a past-the-
        // guard sentinel that pins the accessor doesn't perform a
        // silent `[""] → []` collapse on the empty-entry arm — validate
        // rejects `[""]` through `CodePathEmpty { slot: ":servicos" }`
        // but the accessor must ship the raw slot verbatim so a
        // validate-time gate regression surfaces at the layout /
        // per-Servico renderer boundary rather than being silently
        // absorbed into a component-drop),
        // `["servicos/demo.computeunit.yaml"]` (the canonical
        // singleton V0-shape every in-tree `caixa_with_code_paths`
        // positive control uses; the same shape
        // [`crate::require_single_servico`] admits),
        // `["servicos/a.computeunit.yaml", "servicos/b.computeunit.
        // yaml"]` (a past-the-guard `len != 1` sentinel — the V0
        // singularity gate rejects through `ServicoCountMismatch
        // { count: 2 }` but the accessor must ship the raw slot
        // verbatim so struct-literal `Caixa { servicos: vec![...,
        // ...], .. }` fixtures continue to expose the count at the
        // accessor), and `["servicos/a.computeunit.yaml",
        // "servicos/a.computeunit.yaml"]` (a past-the-guard duplicate
        // sentinel — validate rejects through
        // `CodePathDuplicate { slot: ":servicos" }` per the per-slot
        // set-not-multiset gate, but the accessor must ship the raw
        // slot verbatim so struct-literal fixtures continue to expose
        // the duplicate at the accessor).
        //
        // Fifth and final outer top-level [`Caixa`] `&[T]`-return
        // slice accessor pin on the substrate primitive — folds on the
        // "outer [`Caixa`] `&[T]` slice" projection pattern
        // `autores_returns_autores_slice_verbatim_across_permutations`
        // (b5d813f) opened,
        // `etiquetas_returns_etiquetas_slice_verbatim_across_permutations`
        // (78c7d3c) folded on,
        // `bibliotecas_returns_bibliotecas_slice_verbatim_across_permutations`
        // (8a36c23) closed the universal-axis text-tag family of, and
        // `exe_returns_exe_slice_verbatim_across_permutations`
        // (65d9527) opened the foreign-code-slot sub-family of. Closes
        // the outer-`Caixa` foreign-code-slot `&[T]` sub-family — the
        // trio of code-surface list slots (`:bibliotecas` + `:exe` +
        // `:servicos`) now each carries a substrate-canonical slice
        // accessor. Pins against a future silent detour that returned
        // an owned `Vec<String>` (which would type-check but silently
        // clone on every accessor call, breaking the zero-cost
        // projection every peer sibling slice accessor carries), a
        // `[""] → []` collapse (which would silently absorb the
        // `CodePathEmpty` refusal case at the accessor boundary), an
        // `[a, a] → [a]` dedup collapse (which would silently absorb
        // the `CodePathDuplicate` refusal case at the accessor
        // boundary — the per-slot set-not-multiset gate is downstream
        // of the accessor and must not be silently promoted into it),
        // or a `[a, b] → [a]` singleton collapse (which would silently
        // absorb the V0 `ServicoCountMismatch` refusal case at the
        // accessor boundary — the V0 singularity gate is downstream of
        // the accessor and must not be silently promoted into it).
        for servicos in [
            vec![],
            vec![""],
            vec!["servicos/demo.computeunit.yaml"],
            vec!["servicos/a.computeunit.yaml", "servicos/b.computeunit.yaml"],
            vec!["servicos/a.computeunit.yaml", "servicos/a.computeunit.yaml"],
        ] {
            let c = caixa_with_code_paths(vec![], vec![], servicos.clone());
            let expected: Vec<String> = servicos.iter().map(|s| (*s).to_string()).collect();
            assert_eq!(
                c.servicos(),
                expected.as_slice(),
                "Caixa::servicos must return :servicos verbatim (got \
                 {:?}, expected {expected:?})",
                c.servicos(),
            );
            assert_eq!(
                c.servicos(),
                c.servicos.as_slice(),
                "Caixa::servicos must byte-equal the raw \
                 `self.servicos.as_slice()` field access across every \
                 value in the Vec<String> accept-set",
            );
        }
    }

    #[test]
    fn validate_code_paths_servicos_empty_arm_routes_through_accessor() {
        // Composition pin: [`Caixa::validate_code_paths`]'s per-entry
        // empty-arm gate on the `:servicos` slot must key off
        // [`Caixa::servicos`], not a divergent raw `&self.servicos`
        // field-borrow walk. Structurally: a `Caixa { servicos:
        // vec!["".into()], .. }` must surface the `CodePathEmpty
        // { slot: ":servicos" }` refusal exactly, and a `Caixa
        // { servicos: vec!["servicos/demo.computeunit.yaml".into()],
        // .. }` (the canonical singleton V0-shape every in-tree
        // `caixa_with_code_paths` positive control uses) must pass
        // validate. The pair jointly pins the accessor + validate-gate
        // composition: any future silent detour that had the accessor
        // return an empty slice on the `[""]` arm (a `.iter().filter
        // (|s| !s.is_empty()).collect()` collapse) would silently
        // absorb the `CodePathEmpty` refusal at the accessor boundary
        // and the validate gate would accept a struct-literal
        // `Caixa { servicos: vec!["".into()], .. }` — the composition
        // pin catches that at caixa-core build time.
        //
        // Peer of the per-`Caixa`
        // `validate_code_paths_bibliotecas_empty_arm_routes_through_accessor`
        // (8a36c23), `validate_code_paths_exe_empty_arm_routes_through_accessor`
        // (65d9527), `validate_autores_empty_arm_routes_through_accessor`
        // (b5d813f), and
        // `validate_etiquetas_empty_entry_arm_routes_through_accessor`
        // (78c7d3c) accessor-composition pins on the sibling `&[T]`-
        // composition axes — same "the validate / shape-gate predicate
        // must route through the substrate-primitive typed dispatch"
        // discipline extended onto the sibling outer top-level
        // [`Caixa`] `&[T]`-composition surface, closing the trio of
        // code-surface accessor-composition pins on the same axis.
        // Nominally the in-tree `validate_code_paths` production body
        // still keys off the internal
        // `[(":bibliotecas", &self.bibliotecas,
        // CodePathFileType::LispSource), (":exe", &self.exe, ..),
        // (":servicos", &self.servicos, ..)]` per-slot dispatch tuple
        // (the tuple's homogeneous `&Vec<String>`-typed shape blocks a
        // per-element accessor swap in isolation — a future companion
        // lift promotes the tuple's element type to `&[String]` and
        // threads the triple of typed dispatches through as a unit);
        // the composition pin catches any future accessor-side silent
        // filter drop against that eventual tuple-closure regardless
        // of whether the `:servicos` slot is threaded through the
        // accessor or the raw field access at the tuple's construction
        // site.
        let c = caixa_with_code_paths(vec![], vec![], vec![""]);
        assert!(
            matches!(
                c.validate_code_paths(),
                Err(ManifestError::CodePathEmpty { slot: ":servicos" })
            ),
            "validate_code_paths must reject servicos == vec![\"\"] \
             with CodePathEmpty {{ slot: \":servicos\" }} — the \
             accessor and the validate gate must route through the \
             same substrate-primitive typed dispatch on the \
             :servicos per-entry empty arm",
        );
        let c = caixa_with_code_paths(vec![], vec![], vec!["servicos/demo.computeunit.yaml"]);
        assert!(
            c.validate_code_paths().is_ok(),
            "validate_code_paths must accept servicos == \
             vec![\"servicos/demo.computeunit.yaml\"] (the canonical \
             singleton V0-shape every in-tree `caixa_with_code_paths` \
             positive control uses)",
        );
    }

    #[test]
    fn servicos_projects_slice_by_borrow() {
        // The by-borrow pin: [`Caixa::servicos`] returns `&[String]` by
        // borrow — the returned slice borrows the underlying
        // `Vec<String>` storage of the `:servicos` slot and the
        // accessor must not clone the backing `Vec` on every call.
        // Peer of the per-`Caixa` `autores_projects_slice_by_borrow`
        // (b5d813f), `etiquetas_projects_slice_by_borrow` (78c7d3c),
        // `bibliotecas_projects_slice_by_borrow` (8a36c23), and
        // `exe_projects_slice_by_borrow` (65d9527) by-borrow pins on
        // the sibling outer top-level [`Caixa`] `&[String]`-return
        // axes — the accessor's returned slice must borrow from
        // `&self` (the returned reference's lifetime is tied to
        // `&self`), and calling the accessor twice on the same
        // [`Caixa`] must yield slices that are pointer-equal (the
        // underlying byte-buffer is the storage `Vec`'s allocation,
        // not a fresh copy) as well as value-equal (idempotent, no
        // side effects on `&self`).
        //
        // Pins against a future silent detour that returned an owned
        // `Vec<String>` (which would type-check but silently clone on
        // every call, breaking the zero-cost projection every peer
        // sibling slice accessor carries), a `&Vec<String>` return
        // (which would leak the backing `Vec`'s grow/push/reserve
        // surface no downstream consumer reaches for), or a one-arm-
        // only accessor that returned a saturating value on some
        // sentinel input (breaking the pass-through invariant the
        // sibling slice accessors carry).
        for servicos in [
            vec![],
            vec!["servicos/demo.computeunit.yaml"],
            vec!["servicos/a.computeunit.yaml", "servicos/b.computeunit.yaml"],
            vec!["servicos/a.computeunit.yaml", "servicos/a.computeunit.yaml"],
        ] {
            let c = caixa_with_code_paths(vec![], vec![], servicos.clone());
            let expected: Vec<String> = servicos.iter().map(|s| (*s).to_string()).collect();
            let first = c.servicos();
            let second = c.servicos();
            assert_eq!(
                first, second,
                "Caixa::servicos must be idempotent — two successive \
                 calls on the same &self must return the same &[String]",
            );
            assert_eq!(
                first.as_ptr(),
                second.as_ptr(),
                "Caixa::servicos must borrow the underlying \
                 Vec<String> storage — two successive calls must \
                 return slices with the same backing pointer (a fresh \
                 Vec<String> clone would change the pointer on every \
                 call)",
            );
            assert_eq!(
                first,
                expected.as_slice(),
                "Caixa::servicos must return :servicos verbatim by \
                 borrow — got {first:?}, expected {expected:?}",
            );
        }
    }

    // ── Caixa::deps — outer top-level &[Dep] slice accessor ───────────

    fn caixa_with_deps(deps: Vec<Dep>) -> Caixa {
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps = deps;
        c
    }

    #[test]
    fn deps_returns_deps_slice_verbatim_across_permutations() {
        // The canonical per-`Caixa` `:deps` universal-axis runtime-
        // dependency-declaration-list slice pin: [`Caixa::deps`] must
        // return the `:deps` typed [`Vec<Dep>`] list verbatim as a
        // `&[Dep]`, element-equal to the raw `self.deps.as_slice()`
        // access across every representative value in the accept-set —
        // `[]` (the "no runtime deps declared" arm every existing
        // fixture without a `:deps` line carries; the
        // [`Caixa::template`] scaffold emits `:deps ()`), a canonical
        // single-entry list (the shape most consumer caixas carry), a
        // canonical two-entry list (the multi-dep runtime closure), and
        // two past-the-guard sentinels — a `[""]`-`:nome` entry
        // ([`Self::validate_deps`] rejects through `NomeEmpty` /
        // `NomeInvalid` but the accessor must ship the raw slot
        // verbatim) and a `[a, a]` duplicate (validate rejects through
        // `DuplicateNome { list: ":deps" }` but the accessor must ship
        // the raw slot verbatim so struct-literal fixtures continue to
        // expose the duplicate at the accessor).
        //
        // First outer top-level [`Caixa`] `&[Dep]`-return slice accessor
        // pin on the substrate primitive — opens the outer-`Caixa`
        // dependency-slot `&[Dep]` sub-family the sibling `:deps-dev`
        // future lift closes on. Peer of the closed outer-`Caixa`
        // foreign-code-slot `&[String]` sub-family
        // (`bibliotecas_returns_bibliotecas_slice_verbatim_across_permutations`
        // 8a36c23, `exe_returns_exe_slice_verbatim_across_permutations`
        // 65d9527, `servicos_returns_servicos_slice_verbatim_across_permutations`
        // 611f78b) and the outer-`Caixa` universal-axis text-tag family
        // (`autores_returns_autores_slice_verbatim_across_permutations`
        // b5d813f, `etiquetas_returns_etiquetas_slice_verbatim_across_permutations`
        // 78c7d3c) — extends the "outer [`Caixa`] `&[T]` slice"
        // projection pattern onto a novel element-type axis (`Dep`
        // composite vs the prior sibling family's `String` scalar).
        // Pins against a future silent detour that returned an owned
        // `Vec<Dep>` (which would type-check but silently clone on every
        // accessor call, breaking the zero-cost projection every peer
        // sibling slice accessor carries), a `[""] → []` collapse (which
        // would silently absorb the `NomeEmpty` refusal case at the
        // accessor boundary), or a `[a, a] → [a]` dedup collapse (which
        // would silently absorb the `DuplicateNome` refusal case at the
        // accessor boundary).
        for deps in [
            vec![],
            vec![Dep::simple("", "^0.1")],
            vec![Dep::simple("caixa-teia", "^0.1")],
            vec![
                Dep::simple("caixa-teia", "^0.1"),
                Dep::simple("caixa-core", "^0.1"),
            ],
            vec![
                Dep::simple("caixa-teia", "^0.1"),
                Dep::simple("caixa-teia", "^0.2"),
            ],
        ] {
            let c = caixa_with_deps(deps.clone());
            assert_eq!(
                c.deps(),
                deps.as_slice(),
                "Caixa::deps must return :deps verbatim (got {:?}, \
                 expected {deps:?})",
                c.deps(),
            );
            assert_eq!(
                c.deps(),
                c.deps.as_slice(),
                "Caixa::deps must element-equal the raw \
                 `self.deps.as_slice()` field access across every \
                 value in the Vec<Dep> accept-set",
            );
        }
    }

    #[test]
    fn validate_deps_duplicate_arm_routes_through_accessor() {
        // Composition pin: [`Caixa::validate_deps`]'s within-`:deps`
        // duplicate-`:nome` gate must key off [`Caixa::deps`], not the
        // raw `&self.deps` field-borrow walk. Structurally: a `Caixa
        // { deps: vec![Dep::simple("d", "^0.1"), Dep::simple("d",
        // "^0.2")], .. }` must surface the `DuplicateNome { list:
        // ":deps" }` refusal exactly, and a `Caixa { deps: vec![
        // Dep::simple("d", "^0.1")], .. }` (the canonical single-entry
        // form) must pass validate. The pair jointly pins the accessor +
        // validate-gate composition: any future silent detour that had
        // the accessor return a dedupped slice on the `[a, a]` arm (a
        // `.iter().unique_by(|d| d.nome.as_str()).collect()` collapse)
        // would silently absorb the `DuplicateNome` refusal at the
        // accessor boundary and the validate gate would accept a
        // struct-literal `Caixa` carrying the drift — the composition
        // pin catches that at caixa-core build time.
        //
        // Peer of the per-`Caixa`
        // `validate_autores_empty_entry_arm_routes_through_accessor`
        // (b5d813f), `validate_etiquetas_empty_entry_arm_routes_through_accessor`
        // (78c7d3c), `validate_code_paths_bibliotecas_empty_arm_routes_through_accessor`
        // (8a36c23), `validate_code_paths_exe_empty_arm_routes_through_accessor`
        // (65d9527), and `validate_code_paths_servicos_empty_arm_routes_through_accessor`
        // (611f78b) accessor-composition pins on the sibling `&[T]`-
        // composition axes — same "the validate gate must route through
        // the substrate-primitive typed dispatch" discipline extended
        // onto the sibling outer top-level [`Caixa`] `&[Dep]`-
        // composition surface, opening the outer-`Caixa` dependency-slot
        // arm of the composition-pin family.
        let c = caixa_with_deps(vec![Dep::simple("d", "^0.1"), Dep::simple("d", "^0.2")]);
        let err = c.validate_deps().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::DuplicateNome { ref nome, list } if nome == "d"
                    && list == crate::render::DEP_AUTHOR_KEY_DEPS
            ),
            "validate_deps must reject deps == \
             vec![Dep(\"d\",\"^0.1\"), Dep(\"d\",\"^0.2\")] with \
             DuplicateNome {{ nome: \"d\", list: \":deps\" }} — the \
             accessor and the validate gate must route through the \
             same substrate-primitive typed dispatch on the :deps \
             within-list duplicate arm (got {err:?})",
        );
        let c = caixa_with_deps(vec![Dep::simple("d", "^0.1")]);
        assert!(
            c.validate_deps().is_ok(),
            "validate_deps must accept deps == vec![Dep(\"d\",\"^0.1\")] \
             (the canonical single-entry form)",
        );
    }

    #[test]
    fn deps_projects_slice_by_borrow() {
        // The by-borrow pin: [`Caixa::deps`] returns `&[Dep]` by borrow
        // — the returned slice borrows the underlying `Vec<Dep>` storage
        // of the `:deps` slot and the accessor must not clone the
        // backing `Vec` on every call. Peer of the per-`Caixa`
        // `autores_projects_slice_by_borrow` (b5d813f),
        // `etiquetas_projects_slice_by_borrow` (78c7d3c),
        // `bibliotecas_projects_slice_by_borrow` (8a36c23),
        // `exe_projects_slice_by_borrow` (65d9527), and
        // `servicos_projects_slice_by_borrow` (611f78b) by-borrow pins
        // on the sibling outer top-level [`Caixa`] `&[String]`-return
        // axes — the accessor's returned slice must borrow from `&self`
        // (the returned reference's lifetime is tied to `&self`), and
        // calling the accessor twice on the same [`Caixa`] must yield
        // slices that are pointer-equal (the underlying byte-buffer is
        // the storage `Vec`'s allocation, not a fresh copy) as well as
        // value-equal (idempotent, no side effects on `&self`).
        //
        // Pins against a future silent detour that returned an owned
        // `Vec<Dep>` (which would type-check but silently clone on
        // every call), a `&Vec<Dep>` return (which would leak the
        // backing `Vec`'s grow/push/reserve surface no downstream
        // consumer reaches for), or a one-arm-only accessor that
        // returned a saturating value on some sentinel input.
        for deps in [
            vec![],
            vec![Dep::simple("caixa-teia", "^0.1")],
            vec![
                Dep::simple("caixa-teia", "^0.1"),
                Dep::simple("caixa-core", "^0.1"),
            ],
        ] {
            let c = caixa_with_deps(deps.clone());
            let first = c.deps();
            let second = c.deps();
            assert_eq!(
                first, second,
                "Caixa::deps must be idempotent — two successive calls \
                 on the same &self must return the same &[Dep]",
            );
            assert_eq!(
                first.as_ptr(),
                second.as_ptr(),
                "Caixa::deps must borrow the underlying Vec<Dep> \
                 storage — two successive calls must return slices \
                 with the same backing pointer (a fresh Vec<Dep> clone \
                 would change the pointer on every call)",
            );
            assert_eq!(
                first,
                deps.as_slice(),
                "Caixa::deps must return :deps verbatim by borrow — \
                 got {first:?}, expected {deps:?}",
            );
        }
    }

    // ── Caixa::deps_dev — outer top-level &[Dep] slice accessor ──────

    fn caixa_with_deps_dev(deps_dev: Vec<Dep>) -> Caixa {
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps_dev = deps_dev;
        c
    }

    #[test]
    fn deps_dev_returns_deps_dev_slice_verbatim_across_permutations() {
        // The canonical per-`Caixa` `:deps-dev` universal-axis dev-only-
        // dependency-declaration-list slice pin: [`Caixa::deps_dev`]
        // must return the `:deps-dev` typed [`Vec<Dep>`] list verbatim as
        // a `&[Dep]`, element-equal to the raw `self.deps_dev.as_slice()`
        // access across every representative value in the accept-set —
        // `[]` (the "no dev deps declared" arm every existing fixture
        // without a `:deps-dev` line carries; the [`Caixa::template`]
        // scaffold emits `:deps-dev ()`), a canonical single-entry list
        // (the shape most consumer caixas carry — a `tatara-check` dev
        // pin), a canonical two-entry list (the multi-dev-dep closure),
        // and two past-the-guard sentinels — a `[""]`-`:nome` entry
        // ([`Self::validate_deps`] rejects through `NomeEmpty` /
        // `NomeInvalid` but the accessor must ship the raw slot
        // verbatim) and a `[a, a]` duplicate (validate rejects through
        // `DuplicateNome { list: ":deps-dev" }` but the accessor must
        // ship the raw slot verbatim so struct-literal fixtures continue
        // to expose the duplicate at the accessor).
        //
        // Second outer top-level [`Caixa`] `&[Dep]`-return slice-accessor
        // pin on the substrate primitive — closes the outer-`Caixa`
        // dependency-slot `&[Dep]` sub-family the sibling
        // `deps_returns_deps_slice_verbatim_across_permutations`
        // (ad34b4e) opened on. Folds the "outer [`Caixa`] `&[Dep]`
        // slice" projection pattern onto the sibling dev-dep axis —
        // pins against a future silent detour that returned an owned
        // `Vec<Dep>` (which would type-check but silently clone on every
        // accessor call, breaking the zero-cost projection every peer
        // sibling slice accessor carries), a `[""] → []` collapse (which
        // would silently absorb the `NomeEmpty` refusal case at the
        // accessor boundary), or a `[a, a] → [a]` dedup collapse (which
        // would silently absorb the `DuplicateNome` refusal case at the
        // accessor boundary).
        for deps_dev in [
            vec![],
            vec![Dep::simple("", "^0.1")],
            vec![Dep::simple("tatara-check", "^0.1")],
            vec![
                Dep::simple("tatara-check", "^0.1"),
                Dep::simple("caixa-lint", "^0.1"),
            ],
            vec![
                Dep::simple("tatara-check", "^0.1"),
                Dep::simple("tatara-check", "^0.2"),
            ],
        ] {
            let c = caixa_with_deps_dev(deps_dev.clone());
            assert_eq!(
                c.deps_dev(),
                deps_dev.as_slice(),
                "Caixa::deps_dev must return :deps-dev verbatim (got \
                 {:?}, expected {deps_dev:?})",
                c.deps_dev(),
            );
            assert_eq!(
                c.deps_dev(),
                c.deps_dev.as_slice(),
                "Caixa::deps_dev must element-equal the raw \
                 `self.deps_dev.as_slice()` field access across every \
                 value in the Vec<Dep> accept-set",
            );
        }
    }

    #[test]
    fn validate_deps_duplicate_deps_dev_arm_routes_through_accessor() {
        // Composition pin: [`Caixa::validate_deps`]'s within-`:deps-dev`
        // duplicate-`:nome` gate must key off [`Caixa::deps_dev`], not
        // the raw `&self.deps_dev` field-borrow walk. Structurally: a
        // `Caixa { deps_dev: vec![Dep::simple("d", "^0.1"),
        // Dep::simple("d", "^0.2")], .. }` must surface the
        // `DuplicateNome { list: ":deps-dev" }` refusal exactly, and a
        // `Caixa { deps_dev: vec![Dep::simple("d", "^0.1")], .. }` (the
        // canonical single-entry form) must pass validate. The pair
        // jointly pins the accessor + validate-gate composition: any
        // future silent detour that had the accessor return a dedupped
        // slice on the `[a, a]` arm (a
        // `.iter().unique_by(|d| d.nome.as_str()).collect()` collapse)
        // would silently absorb the `DuplicateNome` refusal at the
        // accessor boundary and the validate gate would accept a
        // struct-literal `Caixa` carrying the drift — the composition
        // pin catches that at caixa-core build time.
        //
        // Peer of `validate_deps_duplicate_arm_routes_through_accessor`
        // (ad34b4e) on the sibling `:deps` axis — same "the validate
        // gate must route through the substrate-primitive typed
        // dispatch" discipline folded onto the sibling `:deps-dev`
        // axis, closing the two-list dep-graph composition-pin family.
        // The `:deps-dev` diagnostic must carry the
        // `DEP_AUTHOR_KEY_DEPS_DEV` list-tag (not
        // `DEP_AUTHOR_KEY_DEPS`) so the emitted error names the
        // offending list unambiguously.
        let c = caixa_with_deps_dev(vec![Dep::simple("d", "^0.1"), Dep::simple("d", "^0.2")]);
        let err = c.validate_deps().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::DuplicateNome { ref nome, list } if nome == "d"
                    && list == crate::render::DEP_AUTHOR_KEY_DEPS_DEV
            ),
            "validate_deps must reject deps_dev == \
             vec![Dep(\"d\",\"^0.1\"), Dep(\"d\",\"^0.2\")] with \
             DuplicateNome {{ nome: \"d\", list: \":deps-dev\" }} — the \
             accessor and the validate gate must route through the \
             same substrate-primitive typed dispatch on the :deps-dev \
             within-list duplicate arm (got {err:?})",
        );
        let c = caixa_with_deps_dev(vec![Dep::simple("d", "^0.1")]);
        assert!(
            c.validate_deps().is_ok(),
            "validate_deps must accept deps_dev == \
             vec![Dep(\"d\",\"^0.1\")] (the canonical single-entry form)",
        );
    }

    #[test]
    fn deps_dev_projects_slice_by_borrow() {
        // The by-borrow pin: [`Caixa::deps_dev`] returns `&[Dep]` by
        // borrow — the returned slice borrows the underlying `Vec<Dep>`
        // storage of the `:deps-dev` slot and the accessor must not
        // clone the backing `Vec` on every call. Peer of
        // `deps_projects_slice_by_borrow` (ad34b4e) on the sibling
        // `:deps` axis, and of the per-`Caixa`
        // `autores_projects_slice_by_borrow` (b5d813f),
        // `etiquetas_projects_slice_by_borrow` (78c7d3c),
        // `bibliotecas_projects_slice_by_borrow` (8a36c23),
        // `exe_projects_slice_by_borrow` (65d9527), and
        // `servicos_projects_slice_by_borrow` (611f78b) by-borrow pins
        // on the sibling outer top-level [`Caixa`] `&[String]`-return
        // axes — the accessor's returned slice must borrow from `&self`
        // (the returned reference's lifetime is tied to `&self`), and
        // calling the accessor twice on the same [`Caixa`] must yield
        // slices that are pointer-equal (the underlying byte-buffer is
        // the storage `Vec`'s allocation, not a fresh copy) as well as
        // value-equal (idempotent, no side effects on `&self`).
        //
        // Pins against a future silent detour that returned an owned
        // `Vec<Dep>` (which would type-check but silently clone on
        // every call), a `&Vec<Dep>` return (which would leak the
        // backing `Vec`'s grow/push/reserve surface no downstream
        // consumer reaches for), or a one-arm-only accessor that
        // returned a saturating value on some sentinel input.
        for deps_dev in [
            vec![],
            vec![Dep::simple("tatara-check", "^0.1")],
            vec![
                Dep::simple("tatara-check", "^0.1"),
                Dep::simple("caixa-lint", "^0.1"),
            ],
        ] {
            let c = caixa_with_deps_dev(deps_dev.clone());
            let first = c.deps_dev();
            let second = c.deps_dev();
            assert_eq!(
                first, second,
                "Caixa::deps_dev must be idempotent — two successive \
                 calls on the same &self must return the same &[Dep]",
            );
            assert_eq!(
                first.as_ptr(),
                second.as_ptr(),
                "Caixa::deps_dev must borrow the underlying Vec<Dep> \
                 storage — two successive calls must return slices \
                 with the same backing pointer (a fresh Vec<Dep> clone \
                 would change the pointer on every call)",
            );
            assert_eq!(
                first,
                deps_dev.as_slice(),
                "Caixa::deps_dev must return :deps-dev verbatim by \
                 borrow — got {first:?}, expected {deps_dev:?}",
            );
        }
    }

    // ── drift-detection: Caixa top-level multi-word serde-derive-to-const identity ──

    #[test]
    fn caixa_multi_word_serde_keys_match_lifted_top_level_key_consts() {
        // Load-bearing invariant: every multi-word top-level [`Caixa`]
        // serde-derived JSON key routes through a lifted `&'static str`
        // const. The Rust field names are `snake_case`
        // (`deps_dev` / `upgrade_from` / `max_restarts` /
        // `restart_window`); [`Caixa`]'s `#[serde(rename_all =
        // "camelCase")]` derive attribute maps each to the camelCase
        // byte-string the [`Caixa::to_lisp`] round-trip's
        // `serde_json::to_value(self)` step lands under before
        // `tatara_lisp::domain::json_to_sexp` re-projects the JSON keys
        // to the kebab-case `:deps-dev` / `:upgrade-from` /
        // `:max-restarts` / `:restart-window` author surface. Serialize
        // a fully-populated [`Caixa`] and pin that each canonical
        // byte-sequence appears verbatim in the JSON — a future
        // accidental `rename_all = "snake_case"` / `"kebab-case"` /
        // verbatim-field-name flip at the derive attribute (any of
        // which would silently break every [`Caixa::to_lisp`]
        // round-trip and the future M4 operator-side manifest ingest's
        // `Value::get(<key>)` navigation) surfaces here as a build-time
        // test failure at `manifest.rs`, not as an apply-time
        // `.get(<stale-canonical-const>)` returning `None` far from the
        // derive-attr drift's commit. Same discipline the sibling
        // `supervisor_spec_serde_keys_match_lifted_supervisor_key_consts`
        // (40cc4e5), `membro_serde_keys_match_lifted_membro_key_consts`
        // (ce80ca0), and `upgrade_from_entry_serde_keys_match_lifted_
        // m2_upgrade_from_key_consts` (36ffe65) pins established on the
        // sibling M2 supervision-tree, M3 [`Membro`] per-entry, and M2
        // [`UpgradeFromEntry`] per-entry axes — extended here to the
        // enclosing M0 [`Caixa`] top-level axis so the last of the four
        // multi-word top-level [`Caixa`] serde-derived JSON keys
        // (`depsDev`) joins the substrate's "one canonical byte-string
        // per typed serialized-key axis" discipline.
        use crate::supervisor::{ChildSpec, RestartPolicy, RestartStrategy};
        use crate::upgrade::{UpgradeFromEntry, UpgradeInstruction};
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps_dev = vec![Dep::simple("tatara-check", "^0.1")];
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.0.1".into(),
            instructions: vec![UpgradeInstruction::Restart],
        }];
        c.estrategia = Some(RestartStrategy::OneForOne);
        c.max_restarts = Some(3);
        c.restart_window = Some("60s".into());
        c.children = vec![ChildSpec {
            caixa: "child".into(),
            versao: "^0.1".into(),
            restart: RestartPolicy::Permanent,
        }];
        let json = serde_json::to_string(&c).unwrap();
        for key in [
            crate::render::CAIXA_KEY_DEPS_DEV,
            crate::render::M2_KEY_UPGRADE_FROM,
            crate::render::SUPERVISOR_KEY_MAX_RESTARTS,
            crate::render::SUPERVISOR_KEY_RESTART_WINDOW,
        ] {
            let quoted = format!("\"{key}\"");
            assert!(
                json.contains(&quoted),
                "serialized Caixa must carry the lifted top-level \
                 multi-word byte-sequence {quoted} verbatim in the JSON \
                 emission (got: {json})",
            );
        }
    }

    #[test]
    fn caixa_top_level_multi_word_key_consts_are_pairwise_distinct() {
        // Cross-axis drift-detection pin: a future collapse of the four
        // canonical [`Caixa`] top-level multi-word byte-strings onto the
        // same value (e.g. an accidental copy-paste flip of
        // [`crate::render::CAIXA_KEY_DEPS_DEV`] to also read
        // `"upgradeFrom"`) would silently reroute every downstream
        // `Value::get(<key>)` probe on one axis onto the sibling axis's
        // top-level entry and pass every propagation-probe test that
        // expected only the stale axis's value. Peer of the sibling
        // four-way distinct pin on the `SUPERVISOR_KEY_*` tetrad
        // (40cc4e5) and the two-way pin on `MEMBRO_KEY_*` (ce80ca0).
        let all = [
            crate::render::CAIXA_KEY_DEPS_DEV,
            crate::render::M2_KEY_UPGRADE_FROM,
            crate::render::SUPERVISOR_KEY_MAX_RESTARTS,
            crate::render::SUPERVISOR_KEY_RESTART_WINDOW,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(
                    a, b,
                    "Caixa top-level multi-word key consts must be \
                     pairwise-distinct canonical byte-sequences — got \
                     `{a}` == `{b}`",
                );
            }
        }
    }

    #[test]
    fn caixa_top_level_multi_word_key_consts_are_lower_camel_case_shape() {
        // Shape-pin: every [`Caixa`] top-level multi-word key const must
        // be a lowerCamelCase byte-sequence (no `snake_case`
        // underscores, no `kebab-case` hyphens, no leading colon, no
        // `PascalCase` leading capital, no whitespace / dots) — the
        // canonical shape the `#[serde(rename_all = "camelCase")]`
        // derive produces on [`Caixa`]. A future flip to a
        // non-camelCase attribute at the derive surfaces both here
        // (this test fails on the stale-constant shape) and at
        // `caixa_multi_word_serde_keys_match_lifted_top_level_key_consts`
        // (that test fails on the mismatch between const and derive).
        // Peer with `membro_key_consts_are_lower_camel_case_shape`
        // (ce80ca0) and `supervisor_key_consts_are_lower_camel_case_shape`
        // (40cc4e5) on the sibling per-entry / supervisor-tree axes.
        for key in [
            crate::render::CAIXA_KEY_DEPS_DEV,
            crate::render::M2_KEY_UPGRADE_FROM,
            crate::render::SUPERVISOR_KEY_MAX_RESTARTS,
            crate::render::SUPERVISOR_KEY_RESTART_WINDOW,
        ] {
            assert!(
                !key.is_empty(),
                "Caixa top-level multi-word key const must be non-empty \
                 (got {key:?})"
            );
            let first = key.chars().next().unwrap();
            assert!(
                first.is_ascii_lowercase(),
                "Caixa top-level multi-word key const must lead with an \
                 ASCII-lowercase byte (got {key:?}, leads with {first:?})",
            );
            assert!(
                key.chars().all(|c| c.is_ascii_alphanumeric()),
                "Caixa top-level multi-word key const must be \
                 ASCII-alphanumeric only — no `_` / `-` / `:` / `.` / \
                 whitespace (got {key:?})",
            );
        }
    }

    #[test]
    fn caixa_key_deps_dev_pins_canonical_camel_case_byte_string() {
        // Scalar-value pin: the byte-string the
        // [`crate::render::CAIXA_KEY_DEPS_DEV`] const resolves to,
        // asserted verbatim. A future rebrand (`depsDev` → `devDeps`
        // matching Cargo's verbatim `dev-dependencies` axis, `depsDev`
        // → `depsTest` matching a hypothetical per-test-target
        // vocabulary flip) lands as an edit to exactly one const AND
        // one derive attribute — the sibling
        // `caixa_multi_word_serde_keys_match_lifted_top_level_key_consts`
        // pin already ties the const to the derive attribute, so a
        // rebrand that touches only one side of the pair fails at
        // caixa-core build time. Same "scalar-value pin per const"
        // discipline the sibling
        // `m2_top_level_author_key_consts_pin_canonical_kebab_case_labels`
        // (f49c8b0) and `contrato_key_consts_pin_canonical_camel_case_labels`
        // (ca463a4) pins carry on the peer M2 / M3 top-level slot axes.
        assert_eq!(crate::render::CAIXA_KEY_DEPS_DEV, "depsDev");
    }

    #[test]
    fn caixa_key_deps_pins_canonical_byte_string() {
        // Scalar-value pin: the byte-string the
        // [`crate::render::CAIXA_KEY_DEPS`] const resolves to, asserted
        // verbatim. Peer of `caixa_key_deps_dev_pins_canonical_camel_case_byte_string`
        // on the two-list dep-graph serialized-key axis — the sibling
        // pin covers the multi-word `deps_dev → depsDev` camelCase
        // arm, this pin covers the single-word `deps → deps` no-op arm
        // (the [`crate::Caixa::deps`] field name carries no `_`, so the
        // `#[serde(rename_all = "camelCase")]` derive is a no-op on this
        // axis and the emitted JSON key equals the source-side field
        // name byte-for-byte). A future [`crate::Caixa::deps`] field
        // rename (`deps` → `dependencies` matching Cargo's verbatim
        // `[dependencies]` axis, `deps` → `runtime_deps` matching a
        // hypothetical per-runtime-target vocabulary flip) OR an added
        // `#[serde(rename = "…")]` explicit override lands as an edit
        // to exactly one const AND one derive-attr / field name — the
        // sibling `caixa_deps_serde_key_matches_lifted_caixa_key_deps`
        // pin ties the const to the emitted JSON key, so a rebrand
        // that touches only one side of the pair fails at caixa-core
        // build time.
        assert_eq!(crate::render::CAIXA_KEY_DEPS, "deps");
    }

    #[test]
    fn caixa_deps_serde_key_matches_lifted_caixa_key_deps() {
        // Load-bearing invariant on the single-word `deps` top-level
        // axis: the byte-string [`crate::render::CAIXA_KEY_DEPS`] pins
        // must appear verbatim in the JSON [`Caixa::to_lisp`]'s
        // `serde_json::to_value(self)` step emits. Serialize a
        // populated [`Caixa`] whose `:deps` slot carries at least one
        // entry (the `#[serde(default)]` attribute on the field emits
        // an empty `[]` even without members, but a non-empty vec
        // additionally covers the codec's per-`Dep`-entry emission
        // path) and pin that `"deps"` appears verbatim in the JSON
        // emission — a future accidental `rename_all = "snake_case"` /
        // `"kebab-case"` flip at the derive attribute (or an added
        // `#[serde(rename = "…")]` explicit override on the field, or
        // a Rust field rename) would break every [`Caixa::to_lisp`]
        // round-trip and the future M4 operator-side manifest ingest's
        // `Value::get(CAIXA_KEY_DEPS)` navigation — surfaces here as a
        // build-time test failure at `manifest.rs`, not as an
        // apply-time `.get(<stale-canonical-const>)` returning `None`
        // far from the drift's commit. Peer of the sibling
        // `caixa_multi_word_serde_keys_match_lifted_top_level_key_consts`
        // multi-word pin on the same M0 [`Caixa`] top-level
        // serialized-key axis, extended here to the single-word arm
        // the multi-word test's `rename_all = "camelCase"` sweep can't
        // reach (single-word `deps → deps` is a no-op the multi-word
        // pin's `\"depsDev\"` / `\"upgradeFrom\"` / `\"maxRestarts\"` /
        // `\"restartWindow\"` byte-scan can never observe).
        let mut c = Caixa::from_lisp(&Caixa::template("demo")).unwrap();
        c.deps = vec![Dep::simple("caixa-core", "^0.1")];
        let json = serde_json::to_string(&c).unwrap();
        let quoted = format!("\"{}\"", crate::render::CAIXA_KEY_DEPS);
        assert!(
            json.contains(&quoted),
            "serialized Caixa must carry the lifted top-level `deps` \
             byte-sequence {quoted} verbatim in the JSON emission (got: \
             {json})",
        );
    }

    #[test]
    fn caixa_dep_graph_two_list_key_consts_are_pairwise_distinct() {
        // Cross-axis drift-detection pin on the two-list dep-graph
        // renderer-side wire-key axis: a future collapse of the
        // canonical [`crate::render::CAIXA_KEY_DEPS`] /
        // [`crate::render::CAIXA_KEY_DEPS_DEV`] byte-strings onto the
        // same value (e.g. an accidental copy-paste flip of
        // `CAIXA_KEY_DEPS_DEV` to also read `"deps"`) would silently
        // reroute every downstream `Value::get(<key>)` probe on one
        // axis onto the sibling axis's dep-list and pass every
        // propagation-probe test that expected only the stale axis's
        // value — a dev-only dep would land in the runtime closure at
        // publish time, or a runtime dep would be excluded from the
        // published lacre. Peer of the sibling four-way distinct pin
        // on the top-level multi-word tetrad
        // (`caixa_top_level_multi_word_key_consts_are_pairwise_distinct`)
        // and the two-way pin on the sibling
        // [`DEP_AUTHOR_KEY_DEPS`] / [`DEP_AUTHOR_KEY_DEPS_DEV`]
        // author-facing arm (4da6fba's test), extended here to the
        // renderer-side wire-key arm of the same two-list dep-graph
        // axis so both halves of the "one canonical byte-string per
        // typed axis per (author, wire)" grid carry the same
        // distinct-ness discipline.
        assert_ne!(
            crate::render::CAIXA_KEY_DEPS,
            crate::render::CAIXA_KEY_DEPS_DEV,
            "CAIXA_KEY_DEPS and CAIXA_KEY_DEPS_DEV must be distinct \
             canonical byte-sequences on the two-list dep-graph \
             renderer-side wire-key axis"
        );
    }
}
