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

        // Caixa-identity value-shape gates on the two universal axes
        // (`:nome`, `:versao`) every substrate-side artifact's
        // `metadata.name` / version derivation flows through. The
        // [`Caixa::validate_nome`] / [`Caixa::validate_versao`] doc-
        // comments name the canonical authoring footguns verbatim —
        // `:nome` "MyApp" / "my_app" / "team.app" / "-app" / "café"
        // (DNS-1123 violations the K8s apiserver refuses at admission
        // time on every derived `metadata.name`: `lareira-<nome>`,
        // programs.yaml entry, CiliumNetworkPolicy / HTTPRoute names,
        // `LABEL_APLICACAO` value); `:versao` "0.1" / "v0.1.0" / "latest"
        // / "^0.1" / "0.1.0.0" (SemVer-2 violations Helm / OCI tag /
        // `feira publish` git tag / lacre `concrete_versao` /
        // `:upgrade-from :from` peer matching each refuse downstream).
        // Until this wire-up landed both validators existed as `pub fn`
        // on [`Caixa`] (with full per-arm test coverage in
        // `manifest::tests`) but no production code path called them —
        // `feira build` (the canonical author-time gate) silently
        // accepted a malformed `:nome` / `:versao` and the failure
        // surfaced at `helm install` / `kubectl apply` / `feira publish`
        // time on the *first* downstream consumer to strict-parse the
        // value, far from the source `caixa.lisp` and without any field
        // naming the offending Caixa identity axis. The gate runs
        // *after* [`LayoutError::MissingManifest`] (no caixa to check
        // when the manifest is missing) and *before* every kind-coherence
        // gate (each of which carries `caixa.nome` verbatim in its
        // diagnostic — running them first on a structurally-invalid
        // identity would surface a "this kind has slot X" diagnostic
        // against an unrecoverable name). Cross-axis precedence is
        // `:nome` → `:versao` — the canonical declaration order on
        // [`Caixa`] and the same author-grep ordering the
        // [`ManifestError`] family uses. Same per-axis `*Violation
        // { caixa, issue }` envelope every peer per-axis wrap exposes
        // ([`LayoutError::CodePathViolation`] b868442,
        // [`LayoutError::LimitsViolation`] / [`LayoutError::BehaviorViolation`]
        // / [`LayoutError::UpgradeViolation`] / [`LayoutError::SupervisorViolation`]
        // / [`LayoutError::AplicacaoViolation`]).
        caixa
            .validate_nome()
            .map_err(|err| LayoutError::nome_violation(caixa, err))?;
        // `:nome`-side joint-length budget on the canonical
        // `lareira-<nome>` chart-name shape — the second arm on the
        // shared `:nome` axis after the bare-DNS-1123 gate above. Runs
        // through the same [`LayoutError::NomeViolation`] envelope so
        // every per-axis diagnostic on `:nome` carries one wrap shape,
        // peer with the [`Caixa::validate_nome`] → `NomeInvalid`
        // routing already at this site. The chart-name budget is the
        // second-axis ceiling [`Caixa::validate_nome`] cannot see — a
        // 56-byte DNS-1123-valid `:nome` passes the bare-`:nome` shape
        // but produces a 64-byte `lareira-<nome>` chart name the
        // apiserver / `helm lint` rejects at admission, far from the
        // source `caixa.lisp` and naming none of the joint-length
        // overflow's three carriers (DNS-1123 cap, prefix, `:nome`
        // length). Closing it at this wire-up turns the
        // [`lareira_chart_name`] doc-comment's explicit M4-admission
        // deferral (caixa-core/src/render.rs:3198) into a build-time
        // structural property of every emitted artifact.
        caixa
            .validate_nome_chart_name_budget()
            .map_err(|err| LayoutError::nome_violation(caixa, err))?;
        caixa
            .validate_versao()
            .map_err(|err| LayoutError::versao_violation(caixa, err))?;

        // `:deps` / `:deps-dev` per-entry shape gate. The third Caixa-
        // level orphan validator on the universal authoring surface (peer
        // of [`Caixa::validate_nome`] / [`Caixa::validate_versao`] wired
        // immediately above): [`Caixa::validate_deps`] walks every
        // [`Dep::validate`] arm — empty / non-DNS-1123 `:nome`, empty /
        // unparseable `:versao` requirement, malformed `:fonte` repo /
        // pin / `:caminho`, malformed `:caracteristicas` Cargo-feature
        // name (de68c0c) — and then closes the per-list set-not-multiset
        // duplicate-`:nome` invariant on each of `:deps` and `:deps-dev`
        // (359fba5). Until this wire-up landed `validate_deps` existed as
        // `pub fn` on [`Caixa`] with full per-arm unit coverage in
        // `manifest::tests` + `dep::tests` (validate_deps_rejects_*,
        // 53 dep-axis tests) but no production code path called it —
        // `feira build` (the canonical author-time gate;
        // `caixa-feira/src/cmd/build.rs:29` routes through
        // `StandardLayout::verify`) silently accepted a malformed `:deps`
        // entry and the failure surfaced at the *first* downstream
        // consumer to strict-parse it: at lacre-resolve time as a
        // `semver::Error` not naming the offending dep (`:versao` per-
        // entry); at `git clone` time as a fetch failure quoting the
        // shell-escape `repo` (`:fonte :repo`); at the resolver's
        // `HashMap<:nome>` collapse as a silent "second-wins" overwrite
        // (within-list `:nome` duplicate); at `cargo metadata` time as a
        // feature-name rejection on the *target* caixa rather than the
        // dep entry referencing it (`:caracteristicas`); at `helm
        // install` / `kubectl apply` time as an apiserver `metadata.name`
        // rejection on the rendered `lareira-<nome>` chart's per-dep
        // derivation (DNS-1123-violating `:deps :nome`) — each far from
        // the source `caixa.lisp`, none naming the offending `:deps` /
        // `:deps-dev` axis. Runs *after* the Caixa-identity gates (the
        // diagnostic carries `caixa.nome().to_string()` verbatim, which the
        // peer [`Caixa::validate_nome`] gate above has just guaranteed is
        // a valid DNS-1123 label) and *before* every kind-coherence gate
        // (the dep surface is universal — every kind has `:deps` /
        // `:deps-dev` — so its shape diagnostic is more fundamental than
        // the kind-coherence partitions on `:bibliotecas` / `:exe` /
        // `:servicos` / `:membros` / `:children` / M2 slots that follow).
        // Same per-axis `*Violation { caixa, issue }` envelope every peer
        // per-axis wrap exposes ([`LayoutError::NomeViolation`] /
        // [`LayoutError::VersaoViolation`] (1f74a5f),
        // [`LayoutError::CodePathViolation`] (b868442),
        // [`LayoutError::LimitsViolation`] / [`LayoutError::BehaviorViolation`]
        // / [`LayoutError::UpgradeViolation`] / [`LayoutError::SupervisorViolation`]
        // / [`LayoutError::AplicacaoViolation`]). Threads [`DepError`]
        // Display through verbatim — every per-arm reason already names
        // the offending dep's `:nome` (e.g. `":deps entry "caixa-teia"
        // :versao "^bad" is not a valid semver requirement: …"`), so the
        // wrap envelope's `issue` carries a self-locating "which dep,
        // which axis, why" without re-shaping the per-arm parser-side
        // reason. With this wire-up the canonical author-time gate
        // refuses every ill-formed `:deps` / `:deps-dev` value-shape by
        // construction — closing the second-to-last orphan-validator gap
        // on the typed Caixa surface (`validate_restart_window` is the
        // remaining orphan, Supervisor-axis specific and wired into the
        // Supervisor branch below alongside `view.validate()`).
        // Compound per-Caixa entry gate on the dep-graph axis: the
        // layout pipeline's two-dispatch `:deps` / `:deps-dev` cascade
        // — the per-entry + within-list duplicate-`:nome` gate (the
        // [`crate::Dep::validate`] + [`crate::render::insert_first_seen`]
        // cascade `Caixa::validate_deps` opened on, 359fba5) and the
        // cross-slot self-edge gate
        // ([`crate::dep::validate_no_self_dep`], ad4abf1) — folded
        // onto the [`crate::Caixa::validate_deps`] substrate primitive.
        // The two arms run in the same canonical order at the primitive
        // (per-entry + cross-entry duplicate → cross-slot self-edge) so
        // the fold is byte-for-byte equivalent to the pre-fold
        // two-block cascade this call site formerly carried, pinned by
        // the paired
        // `validate_deps_folds_{per_entry,self_edge}_arm_matches_gate`
        // equivalence pins and the
        // `validate_deps_per_entry_arm_fires_before_self_edge_arm`
        // ordering pin in the [`crate::Caixa::validate_deps`] pin
        // family (`manifest.rs`).
        //
        // Same lift discipline the peer per-slot compound gates
        // ([`crate::AplicacaoSpec::validate_contratos`] and its
        // `:membros` / `:entrada` / `:placement` / `:politicas` peers,
        // [`crate::MeshPolicy::validate`],
        // [`crate::SupervisorSpec::validate_children`],
        // [`crate::Caixa::validate_upgrade_from`] d6801df) each carry —
        // one named substrate-primitive gate per typed slot folds every
        // structural + cross-slot axis on that slot onto one call, so
        // every future consumer that wants to re-check the dep-graph
        // after a per-entry patch (the deferred
        // `caixa.pleme.io/v1alpha1/Caixa` CR materializer's admission
        // webhook, a future `feira validate --deps` per-caixa admission
        // verb, a per-`:deps` overlay resolver) reaches the two-arm
        // compound gate through one dispatch rather than re-inlining
        // the two-dispatch cascade in lockstep with this wire-up.
        caixa
            .validate_deps()
            .map_err(|err| LayoutError::deps_violation(caixa, err))?;

        // `:etiquetas` per-entry empty + cross-entry duplicate gate. The
        // fourth universal-axis Caixa-level value-shape gate (peer of
        // [`Caixa::validate_nome`] / [`Caixa::validate_versao`] /
        // [`Caixa::validate_deps`] wired immediately above and
        // [`Caixa::validate_code_paths`] wired below the kind-coherence
        // gates) on the typed Caixa surface. `:etiquetas` is the
        // registry-search-tag axis every kind carries (universal
        // `Vec<String>` slot on [`Caixa`]) and lands verbatim as the
        // Helm chart `Chart.yaml` `keywords:` array on every Servico
        // (`caixa-helm/src/lib.rs:236` folds it through a
        // [`std::collections::BTreeSet`]). Until this wire-up landed
        // `:etiquetas` had no shape gate at any layer — an empty entry
        // (`(:etiquetas (""))` — the canonical paste-from-blank-doc
        // footgun) silently rendered as `keywords: [""]` in `Chart.yaml`,
        // and duplicate entries (`(:etiquetas ("demo" "demo"))` — the
        // copy-paste-the-wrong-tag footgun) were silently dedup'd by
        // the renderer's `BTreeSet` collect — a "second wins / one
        // silently disappears" shape divergent from every peer typed-
        // graph set gate (`:membros :caixa`, `:placement :clusters`,
        // `:entrada :paths`, `:contratos`, `:deps :nome`,
        // `:upgrade-from :from`, the per-instruction-class singularity
        // gates on `:upgrade-from :instructions`). Runs *after* the
        // peer universal `:nome` / `:versao` / `:deps` gates (declaration
        // order on [`Caixa`] is `:nome` → `:versao` → `:edicao` →
        // `:descricao` → `:repositorio` → `:licenca` → `:autores` →
        // `:etiquetas` → `:deps` → `:deps-dev`, but the gate order
        // follows the same identity-axis-first cascade the peer gates
        // establish: `:nome` → `:versao` are the load-bearing identity
        // axes that flow into every diagnostic's caixa prefix, and
        // `:deps` is the universal dep surface that dominates every
        // kind-coherence gate; `:etiquetas` runs after this trio so the
        // diagnostic carries an already-validated `:nome` and the
        // peer universal axes' narrower diagnostics surface first when
        // multiple axes are malformed) and *before* the kind-coherence
        // gates ([`Self::MeshSlotsOnNonAplicacao`] /
        // [`Self::SupervisorSlotsOnNonSupervisor`] /
        // [`Self::ServicoSlotsOnNonServico`] / [`Self::ForeignCodeSlot`])
        // — `:etiquetas` is universal so its shape diagnostic is more
        // fundamental than the kind-coherence partitions on kind-
        // exclusive slot sets.
        //
        // Same per-axis `*Violation { caixa, issue }` envelope every peer
        // per-axis wrap exposes ([`Self::NomeViolation`] /
        // [`Self::VersaoViolation`] 1f74a5f, [`Self::DepsViolation`]
        // aa77d0f, [`Self::CodePathViolation`] b868442,
        // [`Self::RestartWindowViolation`] 10e321a). Threads
        // [`ManifestError::EtiquetaEmpty`] / [`ManifestError::EtiquetaDuplicate`]
        // Display through verbatim — each per-arm reason already names
        // the offending tag (for the duplicate arm) or the structural
        // "empty entry" defect (for the empty arm), so the wrap
        // envelope's `issue` carries a self-locating "which axis, which
        // entry, why" without re-shaping the per-arm reason.
        caixa
            .validate_etiquetas()
            .map_err(|err| LayoutError::etiquetas_violation(caixa, err))?;

        // `:autores` per-entry empty + cross-entry duplicate gate. The
        // fifth universal-axis Caixa-level value-shape gate (peer of
        // [`Caixa::validate_nome`] / [`Caixa::validate_versao`] /
        // [`Caixa::validate_deps`] / [`Caixa::validate_etiquetas`] wired
        // immediately above and [`Caixa::validate_code_paths`] wired
        // below the kind-coherence gates) on the typed Caixa surface.
        // `:autores` is the maintainer-axis every kind carries
        // (universal `Vec<String>` slot on [`Caixa`]) and lands verbatim
        // as the Helm chart `Chart.yaml` `maintainers:` array on every
        // Servico (`caixa-helm/src/lib.rs:251` maps each entry to a
        // `Maintainer { name, email: None }` without dedup). Until this
        // wire-up landed `:autores` had no shape gate at any layer — an
        // empty entry (`(:autores (""))` — the canonical paste-from-
        // blank-doc footgun) silently rendered as
        // `maintainers: [{name: "", email: null}]` in `Chart.yaml`, and
        // duplicate entries (`(:autores ("pleme-io" "pleme-io"))` —
        // the copy-paste-the-wrong-author footgun) stacked verbatim in
        // the chart. Unlike the peer `:etiquetas` axis (where the
        // renderer's `BTreeSet` collect silently dedups the `keywords:`
        // array at chart render — a "second wins / one silently
        // disappears" shape), `maintainers:` has *no* renderer-side
        // dedup, so duplicate `:autores` entries render as two identical
        // maintainer records by construction — a strictly worse footgun
        // than the peer `:etiquetas` shape. Runs *after* the peer
        // universal `:nome` / `:versao` / `:deps` / `:etiquetas` gates
        // (the gate order follows the canonical identity-axis-first
        // cascade the peer gates establish; `:autores` and `:etiquetas`
        // are the two Vec-shaped universal metadata axes — they sit
        // adjacent in the cascade after the load-bearing identity +
        // dep trio) and *before* the kind-coherence gates
        // ([`Self::MeshSlotsOnNonAplicacao`] /
        // [`Self::SupervisorSlotsOnNonSupervisor`] /
        // [`Self::ServicoSlotsOnNonServico`] / [`Self::ForeignCodeSlot`])
        // — `:autores` is universal so its shape diagnostic is more
        // fundamental than the kind-coherence partitions on kind-
        // exclusive slot sets.
        //
        // Same per-axis `*Violation { caixa, issue }` envelope every peer
        // per-axis wrap exposes ([`Self::NomeViolation`] /
        // [`Self::VersaoViolation`] 1f74a5f, [`Self::DepsViolation`]
        // aa77d0f, [`Self::EtiquetasViolation`] 360a499,
        // [`Self::CodePathViolation`] b868442,
        // [`Self::RestartWindowViolation`] 10e321a). Threads
        // [`ManifestError::AutorEmpty`] / [`ManifestError::AutorDuplicate`]
        // Display through verbatim — each per-arm reason already names
        // the offending author (for the duplicate arm) or the structural
        // "empty entry" defect (for the empty arm), so the wrap
        // envelope's `issue` carries a self-locating "which axis, which
        // entry, why" without re-shaping the per-arm reason.
        caixa
            .validate_autores()
            .map_err(|err| LayoutError::autores_violation(caixa, err))?;

        // `:repositorio` git-repo-URL shape gate. The sixth
        // universal-axis Caixa-level value-shape gate (peer of
        // [`Caixa::validate_nome`] / [`Caixa::validate_versao`] /
        // [`Caixa::validate_deps`] / [`Caixa::validate_etiquetas`] /
        // [`Caixa::validate_autores`] wired immediately above and
        // [`Caixa::validate_code_paths`] wired below the kind-coherence
        // gates) on the typed Caixa surface. `:repositorio` is the
        // universal git-shaped homepage axis every kind carries
        // (universal `Option<String>` slot on [`Caixa`]) and routes
        // through two load-bearing substrate consumers:
        // [`caixa-helm`] folds it verbatim into the rendered
        // `lareira-<nome>` Helm chart's `Chart.yaml` `home:` field
        // (`build_chart_yaml` at `caixa-helm/src/lib.rs:268`) and into
        // the chart `README.md` `repo = …` interpolation
        // (`caixa-helm/src/lib.rs:359`); [`caixa-flux`] folds it
        // verbatim into the standalone `ClusterBundleOpts::for_caixa`
        // `git_url:` field (`caixa-flux/src/lib.rs:293`), which
        // becomes the FluxCD `GitRepository.spec.url` the cluster's
        // source-controller polls — the load-bearing deploy-time axis.
        // Both consumers use `Option::unwrap_or_else(|| <fallback>)`
        // to substitute a placeholder when the slot is absent (`None`
        // → the fallback fires); a `Some("")` *skips the fallback*
        // and silently passes the empty string through to
        // `Chart.yaml home: ""` / `GitRepository url: ""`. Until this
        // wire-up landed `:repositorio` had no shape gate at any
        // layer — empty (`(:repositorio "")` — the canonical
        // paste-from-blank-doc footgun) and malformed (whitespace,
        // control char / CRLF, leading `-` CLI-arg-injection,
        // missing `:` separator) values silently landed in the
        // rendered artifacts and broke at `helm template` / FluxCD
        // reconcile time far from the source `caixa.lisp`.
        //
        // Runs *after* the peer universal `:nome` / `:versao` /
        // `:deps` / `:etiquetas` / `:autores` gates (the gate order
        // follows the canonical identity-axis-first cascade the peer
        // gates establish; `:repositorio` is the universal git-URL
        // axis — it sits adjacent to `:autores` in the cascade after
        // the load-bearing identity + dep trio + the two Vec-shaped
        // universal metadata axes) and *before* the kind-coherence
        // gates ([`Self::MeshSlotsOnNonAplicacao`] /
        // [`Self::SupervisorSlotsOnNonSupervisor`] /
        // [`Self::ServicoSlotsOnNonServico`] / [`Self::ForeignCodeSlot`])
        // — `:repositorio` is universal so its shape diagnostic is
        // more fundamental than the kind-coherence partitions on
        // kind-exclusive slot sets.
        //
        // Same per-axis `*Violation { caixa, issue }` envelope every
        // peer per-axis wrap exposes ([`Self::NomeViolation`] /
        // [`Self::VersaoViolation`] 1f74a5f, [`Self::DepsViolation`]
        // aa77d0f, [`Self::EtiquetasViolation`] 360a499,
        // [`Self::AutoresViolation`] 86c769b, [`Self::CodePathViolation`]
        // b868442, [`Self::RestartWindowViolation`] 10e321a). Threads
        // [`ManifestError::RepositorioEmpty`] /
        // [`ManifestError::RepositorioInvalid`] Display through
        // verbatim — each per-arm reason already names the offending
        // `:repositorio` value (for the invalid arm) or the
        // structural "empty entry" defect (for the empty arm), so
        // the wrap envelope's `issue` carries a self-locating "which
        // axis, which value, why" without re-shaping the per-arm
        // reason. With this gate the two `git URL`-shaped surfaces on
        // the typed Caixa (`:repositorio` here, `:deps :fonte :repo`
        // peer routed through the same shared
        // [`crate::render::is_git_repo_url`] predicate via
        // [`crate::DepSource::validate`]) are now structurally
        // equivalent — every value past validate is
        // guaranteed-acceptable by the shared predicate's constraint
        // union, by construction.
        caixa
            .validate_repositorio()
            .map_err(|err| LayoutError::repositorio_violation(caixa, err))?;

        // `:descricao` non-empty shape gate. The seventh universal-
        // axis Caixa-level value-shape gate (peer of
        // [`Caixa::validate_nome`] / [`Caixa::validate_versao`] /
        // [`Caixa::validate_deps`] / [`Caixa::validate_etiquetas`] /
        // [`Caixa::validate_autores`] / [`Caixa::validate_repositorio`]
        // wired immediately above and [`Caixa::validate_code_paths`]
        // wired below the kind-coherence gates) on the typed Caixa
        // surface. `:descricao` is the universal free-form-prose
        // summary axis every kind carries (universal `Option<String>`
        // slot on [`Caixa`]) and routes through two load-bearing
        // [`caixa-helm`] consumers: `build_chart_yaml` folds it
        // verbatim into the rendered `lareira-<nome>` Helm chart's
        // `Chart.yaml` `description:` field
        // (`caixa-helm/src/lib.rs:232-235`), and `build_readme` folds
        // it verbatim into the chart `README.md` header
        // (`caixa-helm/src/lib.rs:333-336`). Both consumers use
        // `Option::unwrap_or_else(|| <fallback>)` to substitute a
        // `caixa.nome`-derived placeholder when the slot is absent
        // (`None` → the fallback fires); a `Some("")` *skips the
        // fallback* and silently passes the empty string through to
        // `Chart.yaml description: ""` / a blank `README.md` header
        // — exact same footgun shape as the peer `:repositorio`
        // surface above. Until this wire-up landed `:descricao` had
        // no shape gate at any layer — the empty
        // (`(:descricao "")` — the canonical paste-from-blank-doc
        // footgun) silently landed in the rendered artifacts and
        // broke at `helm lint` time (`WARNING [chart.metadata.description]:
        // description is required` on `apiVersion: v2` charts) far
        // from the source `caixa.lisp`.
        //
        // Runs *after* the peer universal `:nome` / `:versao` /
        // `:deps` / `:etiquetas` / `:autores` / `:repositorio` gates
        // (the gate order follows the canonical identity-axis-first
        // cascade the peer gates establish; `:descricao` is the
        // universal free-form-prose axis — it sits adjacent to
        // `:repositorio` in the cascade after the load-bearing
        // identity + dep trio + the two Vec-shaped universal
        // metadata axes + the universal git-URL axis) and *before*
        // the kind-coherence gates ([`Self::MeshSlotsOnNonAplicacao`]
        // / [`Self::SupervisorSlotsOnNonSupervisor`] /
        // [`Self::ServicoSlotsOnNonServico`] /
        // [`Self::ForeignCodeSlot`]) — `:descricao` is universal so
        // its shape diagnostic is more fundamental than the kind-
        // coherence partitions on kind-exclusive slot sets.
        //
        // Same per-axis `*Violation { caixa, issue }` envelope every
        // peer per-axis wrap exposes ([`Self::NomeViolation`] /
        // [`Self::VersaoViolation`] 1f74a5f, [`Self::DepsViolation`]
        // aa77d0f, [`Self::EtiquetasViolation`] 360a499,
        // [`Self::AutoresViolation`] 86c769b,
        // [`Self::RepositorioViolation`] 577b0a9,
        // [`Self::CodePathViolation`] b868442,
        // [`Self::RestartWindowViolation`] 10e321a). Threads
        // [`ManifestError::DescricaoEmpty`] Display through verbatim
        // — the per-arm reason already names the offending
        // `:descricao` slot + cites the renderer-side footgun, so
        // the wrap envelope's `issue` carries a self-locating
        // "which axis, why" without re-shaping the per-arm reason.
        caixa
            .validate_descricao()
            .map_err(|err| LayoutError::descricao_violation(caixa, err))?;

        // `:licenca` non-empty shape gate. The eighth universal-axis
        // Caixa-level value-shape gate (peer of [`Caixa::validate_nome`]
        // / [`Caixa::validate_versao`] / [`Caixa::validate_deps`] /
        // [`Caixa::validate_etiquetas`] / [`Caixa::validate_autores`] /
        // [`Caixa::validate_repositorio`] / [`Caixa::validate_descricao`]
        // wired immediately above and [`Caixa::validate_code_paths`]
        // wired below the kind-coherence gates) on the typed Caixa
        // surface. `:licenca` is the universal SPDX-shaped license-
        // expression axis every kind carries (universal `Option<String>`
        // slot on [`Caixa`]) and routes through one load-bearing
        // [`caixa-helm`] consumer: `build_readme` folds it verbatim into
        // the rendered `lareira-<nome>` Helm chart's `README.md` `##
        // License` section (`caixa-helm/src/lib.rs:361`) via
        // `caixa.licenca.clone().unwrap_or_else(|| "MIT".into())`. The
        // consumer's fallback only fires when the slot is absent (`None`
        // → the `MIT` fallback fires); a `Some("")` *skips the
        // fallback* and silently passes the empty string through to a
        // chart `README.md` whose `License` section renders as a bare
        // trailing period — exact same footgun shape as the peer
        // `:repositorio` (577b0a9) and `:descricao` (4e6db38) surfaces
        // above. Until this wire-up landed `:licenca` had no shape
        // gate at any layer — the empty (`(:licenca "")` — the
        // canonical paste-from-blank-doc footgun) silently landed in
        // the rendered chart `README.md` far from the source
        // `caixa.lisp`.
        //
        // Runs *after* the peer universal `:nome` / `:versao` /
        // `:deps` / `:etiquetas` / `:autores` / `:repositorio` /
        // `:descricao` gates (the gate order follows the canonical
        // identity-axis-first cascade the peer gates establish;
        // `:licenca` sits adjacent to `:descricao` in the cascade
        // after the load-bearing identity + dep trio + the two
        // Vec-shaped universal metadata axes + the universal
        // git-URL + free-form-prose axes) and *before* the kind-
        // coherence gates ([`Self::MeshSlotsOnNonAplicacao`] /
        // [`Self::SupervisorSlotsOnNonSupervisor`] /
        // [`Self::ServicoSlotsOnNonServico`] /
        // [`Self::ForeignCodeSlot`]) — `:licenca` is universal so
        // its shape diagnostic is more fundamental than the kind-
        // coherence partitions on kind-exclusive slot sets.
        //
        // Same per-axis `*Violation { caixa, issue }` envelope every
        // peer per-axis wrap exposes ([`Self::NomeViolation`] /
        // [`Self::VersaoViolation`] 1f74a5f, [`Self::DepsViolation`]
        // aa77d0f, [`Self::EtiquetasViolation`] 360a499,
        // [`Self::AutoresViolation`] 86c769b,
        // [`Self::RepositorioViolation`] 577b0a9,
        // [`Self::DescricaoViolation`] 4e6db38,
        // [`Self::CodePathViolation`] b868442,
        // [`Self::RestartWindowViolation`] 10e321a). Threads
        // [`ManifestError::LicencaEmpty`] Display through verbatim
        // — the per-arm reason already names the offending
        // `:licenca` slot + cites the renderer-side footgun, so
        // the wrap envelope's `issue` carries a self-locating
        // "which axis, why" without re-shaping the per-arm reason.
        caixa
            .validate_licenca()
            .map_err(|err| LayoutError::licenca_violation(caixa, err))?;

        // `:edicao` non-empty shape gate. The ninth (and last
        // un-gated) universal-axis Caixa-level value-shape gate
        // (peer of [`Caixa::validate_nome`] / [`Caixa::validate_versao`]
        // / [`Caixa::validate_deps`] / [`Caixa::validate_etiquetas`] /
        // [`Caixa::validate_autores`] / [`Caixa::validate_repositorio`]
        // / [`Caixa::validate_descricao`] / [`Caixa::validate_licenca`]
        // wired immediately above and [`Caixa::validate_code_paths`]
        // wired below the kind-coherence gates) on the typed Caixa
        // surface. `:edicao` is the universal language-edition axis
        // every kind carries (universal `Option<String>` slot on
        // [`Caixa`]) that selects the tatara-lisp macro surface +
        // compatibility flags the substrate applies when building
        // the caixa. The canonical [`Caixa::template`] scaffold every
        // `feira init` emits carries `:edicao "2026"` verbatim
        // (`caixa-core/src/manifest.rs:1193`) and every renderer-side
        // fixture carries `edicao: Some("2026".into())` by
        // construction (`caixa-helm/src/lib.rs:375`,
        // `caixa-flux/src/lib.rs:445`, `caixa-mesh/src/lib.rs:629`,
        // `caixa-core/src/render.rs:2510`). Until this wire-up landed
        // `:edicao` had no shape gate at any layer — the empty
        // (`(:edicao "")` — the canonical paste-from-blank-doc
        // footgun) silently landed as a bare `(:edicao "")` line
        // in the rendered caixa.lisp and a future renderer-side
        // consumer that folds the value through
        // `Option::unwrap_or_else` would skip its fallback (which
        // only fires on `None`) and pass the empty edition through
        // to the substrate's build-time edition selector far from
        // the source `caixa.lisp` — exact same
        // `Some("")`-skips-`unwrap_or_else` footgun shape as the
        // peer `:repositorio` (577b0a9), `:descricao` (4e6db38),
        // and `:licenca` (3d1e535) surfaces above.
        //
        // Runs *after* the peer universal `:nome` / `:versao` /
        // `:deps` / `:etiquetas` / `:autores` / `:repositorio` /
        // `:descricao` / `:licenca` gates (the gate order follows
        // the canonical identity-axis-first cascade the peer gates
        // establish; `:edicao` sits at the tail of the cascade
        // after the load-bearing identity + dep trio + the two
        // Vec-shaped universal metadata axes + the three universal
        // `Option<String>` chart-metadata axes) and *before* the
        // kind-coherence gates ([`Self::MeshSlotsOnNonAplicacao`] /
        // [`Self::SupervisorSlotsOnNonSupervisor`] /
        // [`Self::ServicoSlotsOnNonServico`] /
        // [`Self::ForeignCodeSlot`]) — `:edicao` is universal so
        // its shape diagnostic is more fundamental than the kind-
        // coherence partitions on kind-exclusive slot sets.
        //
        // Same per-axis `*Violation { caixa, issue }` envelope every
        // peer per-axis wrap exposes ([`Self::NomeViolation`] /
        // [`Self::VersaoViolation`] 1f74a5f, [`Self::DepsViolation`]
        // aa77d0f, [`Self::EtiquetasViolation`] 360a499,
        // [`Self::AutoresViolation`] 86c769b,
        // [`Self::RepositorioViolation`] 577b0a9,
        // [`Self::DescricaoViolation`] 4e6db38,
        // [`Self::LicencaViolation`] 3d1e535,
        // [`Self::CodePathViolation`] b868442,
        // [`Self::RestartWindowViolation`] 10e321a). Threads
        // [`ManifestError::EdicaoEmpty`] Display through verbatim
        // — the per-arm reason already names the offending
        // `:edicao` slot + cites the renderer-side footgun, so
        // the wrap envelope's `issue` carries a self-locating
        // "which axis, why" without re-shaping the per-arm reason.
        // With this gate every universal-axis `Option<String>`
        // surface on the typed Caixa (`:repositorio` 577b0a9,
        // `:descricao` 4e6db38, `:licenca` 3d1e535, `:edicao` here)
        // now carries the same structural empty-arm gate by
        // construction.
        caixa
            .validate_edicao()
            .map_err(|err| LayoutError::edicao_violation(caixa, err))?;

        // Supervisors, Aplicacaos, and Acaos don't run code; reject
        // bibliotecas/exe/servicos declarations BEFORE checking those
        // paths exist (which would otherwise produce a less-helpful
        // "missing entry" error first).
        let has_code = !caixa.bibliotecas().is_empty()
            || !caixa.exe().is_empty()
            || !caixa.servicos().is_empty();
        if caixa.kind().is_supervisor() && has_code {
            return Err(LayoutError::supervisor_owns_code(caixa));
        }
        if caixa.kind().is_aplicacao() && has_code {
            return Err(LayoutError::aplicacao_owns_code(caixa));
        }
        // An Acao's sole payload is its `:ci` slot (CANTEIRO §7.1-C) —
        // like Supervisor/Aplicacao it runs no code of its own, so a
        // declared :bibliotecas/:exe/:servicos is the same "silently
        // ignored" footgun the two gates above already close for the
        // other two no-code kinds.
        if caixa.kind().is_acao() && has_code {
            return Err(LayoutError::acao_owns_code(caixa));
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
        if !caixa.kind().is_aplicacao() {
            let mesh_slots = caixa.declared_mesh_slots();
            if !mesh_slots.is_empty() {
                return Err(LayoutError::mesh_slots_on_non_aplicacao(caixa, mesh_slots));
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
        if !caixa.kind().is_supervisor() {
            let supervisor_slots = caixa.declared_supervisor_slots();
            if !supervisor_slots.is_empty() {
                return Err(LayoutError::supervisor_slots_on_non_supervisor(
                    caixa,
                    supervisor_slots,
                ));
            }
        }

        // Kind ↔ slot coherence (mirror of the mesh-slot + supervisor-slot
        // gates above on the M2 Servico-runtime slot set): the M2 slots
        // (:limits, :behavior, :upgrade-from) configure the runtime of a
        // long-running wasm component, i.e. a :kind Servico — :limits is
        // Lunatic per-process sandboxing (INSPIRATIONS §III.1), :behavior
        // the OTP gen_server callback set (§II.3), :upgrade-from the OTP
        // appup hot-reload table (§II.4). The caixa-helm / caixa-flux
        // renderers gate on `require_kind(_, Servico)` and only emit these
        // slots for a Servico — so on any *other* kind a declared M2 slot
        // is the manifest field's documented "ignored otherwise": its
        // well-formedness is checked by the M2 invariant blocks below, but
        // the value is never rendered into a chart / programs.yaml entry —
        // it silently passes verify and then vanishes, far from the source
        // caixa.lisp. Reject it here — beside the mesh- and supervisor-slot
        // gates, before the M2 validate blocks (which would otherwise spend
        // their diagnostics on a value the kind can never render) — naming
        // the offending kind + slot(s). `declared_servico_slots` is the
        // single typed source of the M2-slot set + its canonical
        // diagnostic order.
        if !caixa.kind().is_servico() {
            let servico_slots = caixa.declared_servico_slots();
            if !servico_slots.is_empty() {
                return Err(LayoutError::servico_slots_on_non_servico(
                    caixa,
                    servico_slots,
                ));
            }
        }

        // Kind ↔ slot coherence (mirror of the three gates above on the
        // Acao `:ci` slot, CANTEIRO §7.1-C): `:ci` carries a typed CI
        // run — a canteiro_types::CiRun — that only the caixa-actions
        // renderer decomposes + validates, and only for a :kind Acao.
        // On any *other* kind a declared `:ci` is the manifest field's
        // documented "ignored otherwise": it silently passes verify and
        // then vanishes (never decomposed, never rendered), far from the
        // source caixa.lisp. Reject it here — beside the mesh-,
        // supervisor-, and servico-slot gates — naming the offending
        // kind. Unlike its three siblings this axis is a single
        // `Option` field, not a Vec-of-named-slots, so the gate reads
        // `caixa.ci().is_some()` directly rather than reaching for a
        // `declared_*_slots()` helper.
        if caixa.ci().is_some() && !caixa.kind().is_acao() {
            return Err(LayoutError::CiOnNonAcao {
                caixa: caixa.nome().to_string(),
                kind: caixa.kind(),
            });
        }

        // Kind ↔ slot coherence on the fourth and final axis — the
        // code-surface slot set (the trio M2/Supervisor/Aplicacao gates
        // above close on the M2 runtime, supervisor-tree, and M3 mesh
        // axes; this gate closes the symmetric "kind owns this code
        // shape" relation on `:exe` + `:servicos`). `:exe` is the nix-
        // built executable surface owned only by Binario; `:servicos`
        // is the wasm-component daemon surface owned only by Servico.
        // The caixa-helm / caixa-flux / caixa-flake renderers gate on
        // `require_kind(_, <owning-kind>)` and only emit the slot for
        // its owning kind — so on any *other* code-running kind a
        // declared `:exe` / `:servicos` is the manifest field's
        // documented "ignored otherwise" (see the field docs on
        // `Caixa::exe` + `Caixa::servicos`): the path is validated by
        // the per-kind path-existence loops below, but the value is
        // never rendered into a build target or programs.yaml entry —
        // it silently passes `feira build` and then vanishes, far from
        // the source caixa.lisp.
        //
        // Reject it here — beside the M2/Supervisor/Aplicacao slot
        // gates, after the `SupervisorOwnsCode` / `AplicacaoOwnsCode`
        // OwnCode gates which dominate on those two no-code kinds (a
        // Supervisor / Aplicacao with any of `:bibliotecas` / `:exe` /
        // `:servicos` surfaces the OwnCode diagnostic first), and
        // before the path-existence loops which would otherwise spend
        // a less-helpful `MissingEntry` diagnostic on the foreign
        // slot's path. `declared_foreign_code_slots` is the single
        // typed source of the foreign-code-slot set + its canonical
        // diagnostic order (`:exe` → `:servicos`).
        //
        // Mirrors the 9d37f98 / 510c00a / 760a430 kind ↔ slot
        // coherence trio's "declared-but-inert" footgun closure on the
        // M2 / supervisor-tree / M3 axes, now extended onto the code-
        // surface axis — every code-running kind's exclusive code
        // surface is structurally fenced from every other code-running
        // kind. `:bibliotecas` is deliberately excluded from the foreign
        // set on Binario / Servico (a `lib/` helper bundled into the
        // nix flake's build or the wasm-component's source tree is a
        // legitimate cross-kind authoring shape); on Biblioteca it is
        // the native slot, and on Supervisor / Aplicacao the OwnCode
        // gates above already close it.
        let foreign_code_slots = caixa.declared_foreign_code_slots();
        if !foreign_code_slots.is_empty() {
            return Err(LayoutError::foreign_code_slot(caixa, foreign_code_slots));
        }

        // Per-entry path-shape gate on the three Caixa-level code-surface
        // path lists (`:bibliotecas`, `:exe`, `:servicos`): each entry must
        // be non-empty, relative, and free of `..` components — the same
        // [`crate::render::is_sandboxed_relative_path`] discipline the
        // peer `:behavior :on-*` (b0c8389) and
        // `:upgrade-from :state-change :script` (26da2c7) axes already
        // route through. Runs *after* the kind-coherence gates above (so
        // a `:exe` on a Servico surfaces ForeignCodeSlot rather than a
        // per-entry shape diagnostic, and a Supervisor/Aplicacao with any
        // code surface surfaces OwnCode first) and *before* the existence
        // loops below (so an empty / absolute / parent-escaping entry
        // surfaces its self-locating per-slot diagnostic rather than a
        // downstream `MissingEntry` / `ExeOutsideDir` /
        // `ServicoOutsideDir` against the resolved sandbox-escape path).
        caixa
            .validate_code_paths()
            .map_err(|err| LayoutError::code_path_violation(caixa, err))?;

        if caixa.kind().requires_lib() && caixa.bibliotecas().is_empty() {
            let expected = root
                .join(crate::render::LAYOUT_DIR_LIB)
                .join(format!("{}.lisp", caixa.nome()));
            if !self.exists(&expected) {
                return Err(LayoutError::MissingLib {
                    caixa: caixa.nome().to_string(),
                    expected,
                });
            }
        }

        if caixa.kind().requires_exe() && caixa.exe().is_empty() {
            return Err(LayoutError::binario_without_exe(caixa));
        }

        if caixa.kind().requires_servicos() && caixa.servicos().is_empty() {
            return Err(LayoutError::servico_without_servicos(caixa));
        }

        // Required-slot gate on the fifth [`CaixaKind`] arm — mirror of
        // the `BinarioWithoutExe` / `ServicoWithoutServicos` pair above.
        // An `Acao`'s sole payload is its `:ci` slot; an `Acao` caixa
        // that doesn't declare one has no CI run to decompose or
        // validate, so `feira build` refuses it here rather than
        // letting a downstream `caixa-actions::validate` call fail with
        // a less-helpful "no :ci" surprise far from the source
        // caixa.lisp.
        if caixa.kind().requires_ci() && caixa.ci().is_none() {
            return Err(LayoutError::missing_ci(caixa));
        }

        for p in caixa.bibliotecas() {
            let full = root.join(p);
            if !self.exists(&full) {
                return Err(LayoutError::missing_entry(
                    crate::render::LAYOUT_MISSING_ENTRY_KIND_BIBLIOTECA,
                    full,
                ));
            }
        }

        let exe_dir = root.join(crate::render::LAYOUT_DIR_EXE);
        for p in caixa.exe() {
            let full = root.join(p);
            if !self.exists(&full) {
                return Err(LayoutError::missing_entry(
                    crate::render::LAYOUT_MISSING_ENTRY_KIND_EXE,
                    full,
                ));
            }
            if !full.starts_with(&exe_dir) {
                return Err(LayoutError::ExeOutsideDir(full));
            }
        }

        let servicos_dir = root.join(crate::render::LAYOUT_DIR_SERVICOS);
        for p in caixa.servicos() {
            let full = root.join(p);
            if !self.exists(&full) {
                return Err(LayoutError::missing_entry(
                    crate::render::LAYOUT_MISSING_ENTRY_KIND_SERVICO,
                    full,
                ));
            }
            if !full.starts_with(&servicos_dir) {
                return Err(LayoutError::ServicoOutsideDir(full));
            }
        }

        // ── M2 typed-substrate invariants ────────────────────────────────

        // Compound per-Caixa entry gate on the M2 `:limits` slot: the
        // layout pipeline's `if let Some(l) = caixa.limits() { l.validate() }`
        // `Option::None → Ok(()) | Some(_) → dispatch` unwrap-and-
        // dispatch pattern — the four-axis cascade on the present-slot
        // arm ([`crate::LimitsSpec::validate`]'s `:memory` wasm32
        // zero-floor / below-page / above-cap / non-page-multiple;
        // `:fuel` zero-floor / cap; `:wall-clock` zero-floor / cap;
        // `:cpu` zero-floor / cap) folded onto the
        // [`crate::Caixa::validate_limits`] substrate primitive. The
        // absent-slot arm (`limits: None`, the canonical "no bound
        // declared — engine-default applies" author shape) is the
        // fold's identity element and passes trivially through the
        // primitive, byte-equal to the pre-lift `if let Some(l) = …`
        // guard this call site formerly carried. Pinned by the paired
        // `validate_limits_folds_arm_matches_gate` equivalence pin and
        // the `validate_limits_accepts_none` / `_accepts_clean_fixture`
        // positive-control pins in the [`crate::Caixa::validate_limits`]
        // pin family (`manifest.rs`).
        //
        // Same lift discipline the peer per-Caixa compound gates
        // ([`crate::Caixa::validate_upgrade_from`] d6801df,
        // [`crate::Caixa::validate_deps`] b5dd55e) each carry — one
        // named substrate-primitive gate per typed slot folds every
        // structural axis on that slot (plus the `Option::None`
        // identity element for the `Option`-shaped slots) onto one
        // call, so every future consumer that wants to re-check
        // `:limits` after a per-`{:memory, :fuel, :wall-clock, :cpu}`
        // patch (the deferred `caixa.pleme.io/v1alpha1/Caixa` CR
        // materializer's admission webhook, a future `feira validate
        // --limits` per-caixa admission verb, a per-`:limits` overlay
        // resolver) reaches the four-axis cascade through one dispatch
        // rather than re-inlining the `if let Some(l) = …` unwrap-and-
        // dispatch pattern in lockstep with this wire-up.
        caixa
            .validate_limits()
            .map_err(|err| LayoutError::limits_violation(caixa, err))?;

        // Compound per-Caixa entry gate on the M2 `:behavior` slot's
        // pure value-shape surface: the layout pipeline's
        // `if let Some(b) = caixa.behavior() { b.validate() }`
        // `Option::None → Ok(()) | Some(_) → dispatch` unwrap-and-
        // dispatch pattern — the six-slot value-shape cascade on the
        // present-slot arm ([`crate::BehaviorSpec::validate`]'s per-
        // `:on-init` / `:on-call` / `:on-cast` / `:on-info` /
        // `:on-state-change` / `:on-terminate` non-empty / relative /
        // no-`..`-parent-escape / terminating-`.lisp`-extension
        // arm-set routed through the shared
        // [`crate::render::require_sandboxed_lisp_path`] helper) —
        // folded onto the [`crate::Caixa::validate_behavior`] substrate
        // primitive. The absent-slot arm (`behavior: None`, the
        // canonical "no callback declared — the runtime falls back to
        // the wasm-engine's default per arm" author shape) is the
        // fold's identity element and passes trivially through the
        // primitive, byte-equal to the pre-lift `if let Some(b) = …`
        // guard this call site formerly carried. Pinned by the paired
        // `validate_behavior_folds_arm_matches_gate` equivalence pin
        // and the `validate_behavior_accepts_none` /
        // `_accepts_clean_fixture` positive-control pins in the
        // [`crate::Caixa::validate_behavior`] pin family
        // (`manifest.rs`).
        //
        // The value-shape gate runs BEFORE the on-disk callback-path
        // existence walk below so a malformed `:behavior` slot
        // surfaces its self-locating per-slot diagnostic (naming the
        // offending `:on-*` slot) rather than the less-helpful
        // "missing behavior-callback" the existence probe would raise
        // against the resolved sandbox-escape path.
        //
        // Same lift discipline the peer per-Caixa compound gates
        // ([`crate::Caixa::validate_limits`] baa4688,
        // [`crate::Caixa::validate_upgrade_from`] d6801df,
        // [`crate::Caixa::validate_deps`] b5dd55e) each carry — one
        // named substrate-primitive gate per typed slot folds every
        // structural axis on that slot (plus the `Option::None`
        // identity element for the `Option`-shaped slots) onto one
        // call, so every future consumer that wants to re-check
        // `:behavior` after a per-`{:on-init, …, :on-terminate}`
        // patch (the deferred `caixa.pleme.io/v1alpha1/Caixa` CR
        // materializer's admission webhook, a future `feira validate
        // --behavior` per-caixa admission verb, a per-`:behavior`
        // overlay resolver) reaches the six-slot cascade through one
        // dispatch rather than re-inlining the `if let Some(b) = …`
        // unwrap-and-dispatch pattern in lockstep with this wire-up.
        // The paired on-disk existence walk stays open-coded at this
        // altitude because it needs the [`LayoutInvariants::exists`]
        // filesystem oracle the pure typed-shape surface has no
        // reference to — mirror of the peer M2 `:upgrade-from` per-
        // instruction script-path existence probe that stayed at this
        // altitude after the [`crate::Caixa::validate_upgrade_from`]
        // lift for the same reason.
        caixa
            .validate_behavior()
            .map_err(|err| LayoutError::behavior_violation(caixa, err))?;
        if let Some(b) = caixa.behavior() {
            for p in b.declared_paths() {
                let full = root.join(p);
                if !self.exists(&full) {
                    return Err(LayoutError::missing_entry(
                        crate::render::LAYOUT_MISSING_ENTRY_KIND_BEHAVIOR_CALLBACK,
                        full,
                    ));
                }
            }
        }

        // Compound per-Caixa entry gate on `:upgrade-from`: the layout
        // pipeline's three-dispatch M2 `:upgrade-from` cascade — the
        // per-entry shape + cross-entry duplicate-`:from` gate
        // ([`crate::upgrade::validate_upgrade_from`]), the cross-slot
        // `:from < :versao` SemVer-2 precedence gate
        // ([`crate::upgrade::validate_upgrade_from_against_versao`]), and
        // the cross-slot `:state-change` ↔ `:on-state-change` composition
        // gate ([`crate::upgrade::validate_upgrade_from_against_behavior`])
        // — folded onto the [`crate::Caixa::validate_upgrade_from`]
        // substrate primitive. The three dispatches run in the same
        // canonical order at the primitive (per-entry → versao → behavior)
        // so the fold is byte-for-byte equivalent to the pre-fold
        // three-block cascade this call site formerly carried, pinned by
        // the paired
        // `validate_upgrade_from_folds_{per_entry,versao,behavior}_arm_matches_gate`
        // equivalence pins and the
        // `validate_upgrade_from_{per_entry_arm_fires_before_versao_arm,
        // versao_arm_fires_before_behavior_arm}` ordering pins in the
        // [`crate::Caixa::validate_upgrade_from`] pin family
        // (`manifest.rs`).
        //
        // Runs BEFORE the existing per-instruction script-path existence
        // pass below so a malformed typed slot surfaces its own
        // self-locating diagnostic rather than the less-helpful "missing
        // upgrade-script" (which doesn't fire for non-script axes at all).
        // Same lift discipline the peer per-slot compound gates
        // ([`crate::AplicacaoSpec::validate_contratos`] and its
        // `:membros` / `:entrada` / `:placement` / `:politicas` peers,
        // [`crate::MeshPolicy::validate`],
        // [`crate::SupervisorSpec::validate_children`]) each carry — one
        // named substrate-primitive gate per typed slot folds every
        // structural axis on that slot onto one call, so every future
        // consumer that wants to re-check `:upgrade-from` after a
        // per-entry patch (the deferred `caixa.pleme.io/v1alpha1/Caixa`
        // CR materializer's admission webhook, a future `feira validate
        // --upgrade` per-caixa admission verb, a per-`:upgrade-from`
        // overlay resolver) reaches the three-arm compound gate through
        // one dispatch rather than re-inlining the three-dispatch
        // cascade in lockstep with this wire-up.
        //
        // The per-instruction on-disk existence-probe walk below stays
        // open-coded at the layout wire-up site — that arm needs the
        // filesystem oracle on the [`LayoutInvariants`] trait, not on
        // the pure per-Caixa typed-shape surface the compound gate
        // folds. Same posture [`crate::Caixa::validate_code_paths`] takes
        // on the sibling code-path axes: the typed-shape gate fires on
        // the per-Caixa surface, the on-disk existence check fires on
        // the [`StandardLayout`] surface.
        caixa
            .validate_upgrade_from()
            .map_err(|err| LayoutError::upgrade_violation(caixa, err))?;
        for entry in caixa.upgrade_from() {
            for instr in entry.instructions() {
                if let Some(p) = instr.declared_path() {
                    let full = root.join(p);
                    if !self.exists(&full) {
                        return Err(LayoutError::missing_entry(
                            crate::render::LAYOUT_MISSING_ENTRY_KIND_UPGRADE_SCRIPT,
                            full,
                        ));
                    }
                }
            }
        }

        // Supervisor invariants (typed shape — children, restart strategy).
        // The "supervisor doesn't own code" check is at the top of verify()
        // so it fires before the existence-check loops.
        if caixa.kind().is_supervisor() {
            // Raw `:restart-window` parse gate on the flat
            // `Caixa::restart_window: Option<String>` axis — the last
            // orphan-validator on the typed Caixa surface flagged by the
            // [`Self::validate_deps`] wire-up's closing comment (the
            // "Supervisor-axis specific" remainder) and the
            // [`Caixa::supervisor_view`] doc-comment's "the future
            // layout-side wire-up" pin. Until this gate landed
            // [`Caixa::validate_restart_window`] existed as `pub fn` on
            // [`Caixa`] with full per-arm unit coverage in `manifest::tests`
            // (`validate_restart_window_rejects_*` — fractional seconds,
            // decimal-shaped integer, half-unit minute, leading sign,
            // unknown unit, garbage, empty-after-trim; eight rejection
            // arms total), but no production code path called it —
            // `feira build` (the canonical author-time gate; routes
            // through [`StandardLayout::verify`]) silently accepted a
            // malformed `:restart-window` and [`Caixa::supervisor_view`]
            // soft-swallowed the parse failure as `restart_window: None`
            // (i.e. the canonical "omit the slot to express no reset"
            // sentinel), turning every malformed window into a never-reset
            // supervisor far from the source `caixa.lisp`, with no field
            // naming the offending `:restart-window`. The Erlang/OTP
            // `MaxIntensity / Period` invariant the typed [`SupervisorSpec`]
            // gate (`view.validate()` immediately below) enforces on the
            // `Option<Duration>` value never reached the gate at all on
            // these inputs: the parse error was already laundered to
            // `None`, and `None` is the canonical "never reset" shape
            // that always validates cleanly. Lifting the parse gate to
            // the layout-pipeline wire-up closes the laundering — every
            // value past this gate either parses through the shared
            // `crate::supervisor::duration_codec::parse` (and therefore
            // round-trips canonically) or fires the new
            // [`Self::RestartWindowViolation`] envelope at the source.
            //
            // Runs *inside* the `kind == Supervisor` branch (rather than
            // alongside the peer flat-Caixa gates `validate_nome` /
            // `validate_versao` / `validate_deps` / `validate_code_paths`
            // above the kind dispatch) because `:restart-window` is in
            // the Supervisor slot set per [`Caixa::declared_supervisor_slots`]
            // — every non-Supervisor caixa with `:restart-window` set
            // already errors upstream via the
            // [`Self::SupervisorSlotsOnNonSupervisor`] kind-coherence gate
            // (line 243-252), so reaching this gate on a non-Supervisor
            // kind would be a no-op (the field is `None` by construction).
            // Runs *before* `view.validate()` so the parse-side diagnostic
            // surfaces first on the raw-string axis — a `:restart-window
            // "1.5s"` lands on the more self-locating
            // `RestartWindowViolation` (which names the offending raw
            // string verbatim) rather than the laundered-to-`None`
            // soft-pass that the typed view would silently let through.
            //
            // Same per-axis `*Violation { caixa, issue }` envelope every
            // peer flat-Caixa wrap exposes ([`Self::NomeViolation`] /
            // [`Self::VersaoViolation`] 1f74a5f,
            // [`Self::DepsViolation`] aa77d0f, [`Self::CodePathViolation`]
            // b868442). Threads [`ManifestError::RestartWindowMalformed`]
            // Display through verbatim — the per-arm reason already names
            // the offending raw value (e.g. `":restart-window \"1.5s\" is
            // not a canonical duration: …"`), so the wrap's `issue`
            // carries a self-locating "which axis, which value, why"
            // without re-shaping the parser-side reason.
            caixa
                .validate_restart_window()
                .map_err(|err| LayoutError::restart_window_violation(caixa, err))?;
            // Compound per-Caixa entry gate on the Supervisor-kind
            // supervision-tree slot family: the layout pipeline's paired
            // `let view = caixa.supervisor_view().expect(...);
            // view.validate() … validate_no_self_supervision(...) …`
            // cascade — the typed-shape cascade
            // ([`crate::SupervisorSpec::validate`]'s per-slot gates on
            // `:estrategia` ↔ `:children` invariants, `:max-restarts` /
            // `:restart-window` bounds, per-child DNS-1123 `:caixa`
            // names, semver-valid `:versao` constraints, the
            // set-not-multiset duplicate-child gate) and the cross-slot
            // self-edge gate
            // ([`crate::supervisor::validate_no_self_supervision`], the
            // `:children :caixa` ≠ `:nome` invariant the typed view
            // cannot enforce on its own because it carries the children
            // but not the parent `:nome`) — folded onto the
            // [`crate::Caixa::validate_supervisor_shape`] substrate
            // primitive. The two arms run in the same canonical order at
            // the primitive (typed-shape cascade → cross-slot self-edge)
            // so the fold is byte-for-byte equivalent to the pre-fold
            // two-block cascade this call site formerly carried, pinned
            // by the paired
            // `validate_supervisor_shape_folds_{view,self_supervision}_arm_matches_gate`
            // equivalence pins and the
            // `validate_supervisor_shape_view_arm_fires_before_self_supervision_arm`
            // ordering pin in the
            // [`crate::Caixa::validate_supervisor_shape`] pin family
            // (`manifest.rs`).
            //
            // Same lift discipline the peer per-Caixa compound gates
            // ([`crate::Caixa::validate_aplicacao_shape`] 949a7a0,
            // [`crate::Caixa::validate_upgrade_from`] d6801df,
            // [`crate::Caixa::validate_deps`] b5dd55e,
            // [`crate::Caixa::validate_limits`] baa4688,
            // [`crate::Caixa::validate_behavior`] 0d2877a) each carry —
            // one named substrate-primitive gate folds every structural +
            // cross-slot axis on that slot family onto one call, so every
            // future consumer that wants to re-check the Supervisor shape
            // after a per-slot patch (the wasm-operator's hierarchical
            // reconciliation scheduler, the M4
            // `mesh.pleme.io/v1alpha1/Supervisor` CR materializer's
            // admission webhook, a future `feira validate --supervisor`
            // per-caixa admission verb, a per-Supervisor overlay
            // resolver) reaches the two-arm compound gate through one
            // dispatch rather than re-inlining the two-dispatch cascade
            // in lockstep with this wire-up. Peer with the
            // [`crate::render::require_supervisor_view`] compound entry
            // gate every per-Supervisor renderer would route through
            // (which already folds the same `spec.validate()` +
            // `validate_no_self_supervision` two-arm cascade behind its
            // `require_kind` + `validate_restart_window` prelude): the
            // two consumers of the Supervisor-shape cascade now share
            // one substrate primitive on each side of the
            // author-time-vs-renderer split, rather than two open-coded
            // cascades kept in lockstep.
            //
            // `:restart-window` stays wired above through its own
            // per-axis `LayoutError::RestartWindowViolation` envelope
            // (the raw-string parse gate on the flat
            // `Caixa::restart_window: Option<String>` axis, distinct
            // from the typed view's `Duration`-shape gate) — the fold
            // covers the two arms that share the
            // `LayoutError::SupervisorViolation` envelope; the
            // parse-gate arm keeps its self-locating envelope so a
            // malformed `:restart-window` surfaces the raw-value
            // diagnostic rather than the laundered-to-`None` soft-pass.
            caixa
                .validate_supervisor_shape()
                .map_err(|err| LayoutError::supervisor_violation(caixa, err))?;
        }

        // Aplicacao invariants — typed graph composition. Like
        // Supervisor, an Aplicacao runs no code itself.
        //
        // Compound per-Caixa entry gate on the Aplicacao-kind mesh-slot
        // family: the layout pipeline's paired `let view =
        // caixa.aplicacao_view().expect(...); view.validate() …
        // validate_no_self_membership(...) …` cascade — the typed-shape
        // cascade ([`crate::AplicacaoSpec::validate`]'s per-slot gates
        // on `:membros`, `:contratos`, `:entrada`, `:placement`,
        // `:politicas`, in that declared order) and the cross-slot
        // self-edge gate ([`crate::aplicacao::validate_no_self_membership`],
        // the `:membros :caixa` ≠ `:nome` invariant the typed view
        // cannot enforce on its own because it carries the membros but
        // not the parent `:nome`) — folded onto the
        // [`crate::Caixa::validate_aplicacao_shape`] substrate primitive.
        // The two arms run in the same canonical order at the primitive
        // (typed-shape cascade → cross-slot self-edge) so the fold is
        // byte-for-byte equivalent to the pre-fold two-block cascade
        // this call site formerly carried, pinned by the paired
        // `validate_aplicacao_shape_folds_{view,self_membership}_arm_matches_gate`
        // equivalence pins and the
        // `validate_aplicacao_shape_view_arm_fires_before_self_membership_arm`
        // ordering pin in the [`crate::Caixa::validate_aplicacao_shape`]
        // pin family (`manifest.rs`).
        //
        // Same lift discipline the peer per-slot compound gates
        // ([`crate::Caixa::validate_upgrade_from`] d6801df,
        // [`crate::Caixa::validate_deps`] b5dd55e,
        // [`crate::Caixa::validate_limits`] baa4688,
        // [`crate::Caixa::validate_behavior`] 0d2877a) each carry — one
        // named substrate-primitive gate folds every structural +
        // cross-slot axis on that slot family onto one call, so every
        // future consumer that wants to re-check the Aplicacao shape
        // after a per-slot patch (the deferred
        // `caixa.pleme.io/v1alpha1/Caixa` CR materializer's admission
        // webhook, a future `feira validate --aplicacao` per-caixa
        // admission verb, a per-Aplicacao overlay resolver) reaches the
        // two-arm compound gate through one dispatch rather than
        // re-inlining the two-dispatch cascade in lockstep with this
        // wire-up. Peer with the [`crate::render::require_aplicacao_view`]
        // compound entry gate every per-Aplicacao renderer routes
        // through (3aefefb): the two consumers of the Aplicacao-shape
        // cascade now share one substrate primitive on each side of the
        // author-time-vs-renderer split, rather than two open-coded
        // cascades kept in lockstep.
        //
        // The outer `if caixa.kind().is_aplicacao()` guard stays because
        // [`crate::Caixa::validate_aplicacao_shape`] is the fold's
        // identity element on non-Aplicacao kinds (returns `Ok(())`
        // without touching the mesh slots — same posture as
        // [`crate::Caixa::validate_limits`] / [`Self`]::
        // [`crate::Caixa::validate_behavior`] on their `Option`-shaped
        // slots); the guard is a redundant but zero-cost fast-path that
        // preserves the peer supervisor branch's `if
        // caixa.kind().is_supervisor()` parallel structure at this
        // altitude.
        if caixa.kind().is_aplicacao() {
            caixa
                .validate_aplicacao_shape()
                .map_err(|err| LayoutError::aplicacao_violation(caixa, err))?;
        }

        // Acao invariants — typed CI-run decompose. Like Supervisor and
        // Aplicacao, an Acao runs no code itself; unlike them, its sole
        // payload is a [`canteiro_types::CiRun`] whose declared-node
        // shape can carry three structural violations
        // ([`canteiro_types::DecomposeError`]: `DuplicateNode`,
        // `UnknownDep`, `Cycle`) the layout pipeline formerly deferred
        // to [`caixa_actions::validate`] — layout only checked `:ci`
        // *presence* via [`Self::MissingCi`] above, so a `:kind Acao`
        // carrying a structurally illegal `:ci` (a duplicate node
        // name, a dependency on an undeclared node, a dependency
        // cycle) passed `feira build` cleanly and surfaced the
        // diagnostic only when [`caixa_actions::validate`] later
        // refused it — far from the source `caixa.lisp` on the
        // author-time gate side.
        //
        // Compound per-Caixa entry gate on the Acao-kind `:ci` slot
        // family: the [`crate::render::decompose_ci`] typed decompose
        // gate — the sibling axis owned by the substrate-canonical
        // [`crate::render::require_acao_view`] compound helper every
        // per-`Acao` renderer routes through — folded onto the
        // [`crate::Caixa::validate_acao_shape`] substrate primitive.
        // The single-arm fold is byte-for-byte equivalent to the
        // pre-fold `decompose_ci(caixa, ci).map(|_| ())?` call this
        // wire-up sees, pinned by the paired
        // `validate_acao_shape_folds_decompose_arm_matches_gate`
        // equivalence pin and the
        // `validate_acao_shape_accepts_non_acao_kind` /
        // `validate_acao_shape_accepts_absent_ci_slot` identity-
        // element pins in the
        // [`crate::Caixa::validate_acao_shape`] pin family
        // (`manifest.rs`).
        //
        // Same lift discipline the peer per-kind compound gates
        // ([`crate::Caixa::validate_supervisor_shape`] 4c70105,
        // [`crate::Caixa::validate_aplicacao_shape`] 949a7a0,
        // [`crate::Caixa::validate_upgrade_from`] d6801df,
        // [`crate::Caixa::validate_deps`] b5dd55e,
        // [`crate::Caixa::validate_limits`] baa4688,
        // [`crate::Caixa::validate_behavior`] 0d2877a) each carry —
        // one named substrate-primitive gate folds every structural
        // axis on that kind onto one call, so every future consumer
        // that wants to re-check the Acao shape after a per-node
        // patch (a per-`Acao` CR materializer's admission webhook, a
        // future `feira validate --acao` per-caixa admission verb, a
        // per-`Acao` overlay resolver) reaches the compound gate
        // through one dispatch rather than re-inlining the decompose
        // cascade in lockstep with this wire-up. Peer with the
        // [`crate::render::require_acao_view`] compound entry gate
        // every per-`Acao` renderer routes through: the two consumers
        // of the Acao-shape cascade now share one substrate primitive
        // on each side of the author-time-vs-renderer split, rather
        // than two open-coded cascades kept in lockstep. Closes the
        // last per-kind asymmetry — with this wire-up the four typed
        // named-caixa kinds (`Servico` / `Aplicacao` / `Supervisor` /
        // `Acao`) each route through one compound per-Caixa shape
        // gate at the layout altitude.
        //
        // Runs *after* the [`Self::MissingCi`] presence gate above
        // (so a `:kind Acao` caixa with `ci = None` surfaces the
        // presence diagnostic first — the fold's identity-element arm
        // passes cleanly on absent `:ci`) and *after* the sibling
        // Supervisor / Aplicacao shape gates so the per-kind
        // diagnostic ordering at the layout altitude reads as the
        // canonical `Supervisor → Aplicacao → Acao` sweep.
        //
        // The outer `if caixa.kind().is_acao()` guard stays because
        // [`crate::Caixa::validate_acao_shape`] is the fold's
        // identity element on non-Acao kinds (returns `Ok(())`
        // without touching the `:ci` slot — same posture as
        // [`crate::Caixa::validate_supervisor_shape`] /
        // [`crate::Caixa::validate_aplicacao_shape`] on the sibling
        // typed-view-carrying arms); the guard is a redundant but
        // zero-cost fast-path that preserves the peer supervisor /
        // aplicacao branches' `if caixa.kind().is_<kind>()` parallel
        // structure at this altitude.
        if caixa.kind().is_acao() {
            caixa
                .validate_acao_shape()
                .map_err(|err| LayoutError::acao_violation(caixa, err))?;
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
    #[error("caixa '{caixa}' has invalid :nome: {issue}")]
    NomeViolation { caixa: String, issue: String },
    #[error("caixa '{caixa}' has invalid :versao: {issue}")]
    VersaoViolation { caixa: String, issue: String },
    #[error("caixa '{caixa}' has invalid :deps / :deps-dev entry: {issue}")]
    DepsViolation { caixa: String, issue: String },
    #[error("caixa '{caixa}' has invalid :etiquetas entry: {issue}")]
    EtiquetasViolation { caixa: String, issue: String },
    #[error("caixa '{caixa}' has invalid :autores entry: {issue}")]
    AutoresViolation { caixa: String, issue: String },
    #[error("caixa '{caixa}' has invalid :repositorio: {issue}")]
    RepositorioViolation { caixa: String, issue: String },
    #[error("caixa '{caixa}' has invalid :descricao: {issue}")]
    DescricaoViolation { caixa: String, issue: String },
    #[error("caixa '{caixa}' has invalid :licenca: {issue}")]
    LicencaViolation { caixa: String, issue: String },
    #[error("caixa '{caixa}' has invalid :edicao: {issue}")]
    EdicaoViolation { caixa: String, issue: String },
    #[error("caixa '{caixa}' has invalid code-path entry: {issue}")]
    CodePathViolation { caixa: String, issue: String },
    #[error("caixa '{caixa}' has invalid :limits: {issue}")]
    LimitsViolation { caixa: String, issue: String },
    #[error("caixa '{caixa}' has invalid :behavior callback: {issue}")]
    BehaviorViolation { caixa: String, issue: String },
    #[error("caixa '{caixa}' has invalid :upgrade-from entry: {issue}")]
    UpgradeViolation { caixa: String, issue: String },
    #[error("supervisor caixa '{caixa}' violates typed shape: {issue}")]
    SupervisorViolation { caixa: String, issue: String },
    #[error("supervisor caixa '{caixa}' has invalid :restart-window: {issue}")]
    RestartWindowViolation { caixa: String, issue: String },
    #[error(
        "supervisor caixa '{0}' must not declare :bibliotecas, :exe, or :servicos — supervisors don't run code, they orchestrate other caixas"
    )]
    SupervisorOwnsCode(String),
    #[error("aplicacao caixa '{caixa}' violates typed shape: {issue}")]
    AplicacaoViolation { caixa: String, issue: String },
    #[error("acao caixa '{caixa}' violates typed shape: {issue}")]
    AcaoViolation { caixa: String, issue: String },
    #[error(
        "aplicacao caixa '{0}' must not declare :bibliotecas, :exe, or :servicos — aplicacaos compose Servicos, they don't run code themselves"
    )]
    AplicacaoOwnsCode(String),
    #[error(
        "acao caixa '{0}' must not declare :bibliotecas, :exe, or :servicos — acaos carry a typed CI run (:ci), they don't run code themselves"
    )]
    AcaoOwnsCode(String),
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
    #[error(
        "caixa '{caixa}' is :kind {kind:?} but declares Servico-only slot(s): {slots} — \
         :limits / :behavior / :upgrade-from configure the runtime of a long-running :kind Servico \
         wasm component (INSPIRATIONS §III.1 / §II.3 / §II.4) and are silently ignored on every \
         other kind (never rendered into a chart or programs.yaml entry); move them to a :kind \
         Servico caixa or remove them"
    )]
    ServicoSlotsOnNonServico {
        caixa: String,
        kind: CaixaKind,
        slots: String,
    },
    #[error(
        "caixa '{caixa}' is :kind {kind:?} but declares foreign code-surface slot(s): {slots} — \
         :exe is the nix-built executable surface owned only by :kind Binario, :servicos is the \
         wasm-component + ComputeUnit daemon surface owned only by :kind Servico; \
         caixa-helm / caixa-flux / caixa-flake gate emission on `require_kind(_, <owning-kind>)`, \
         so a declared :exe / :servicos on the wrong code-running kind is silently ignored — the \
         path is validated by the layout's path-existence loops but never rendered into a build \
         target or programs.yaml entry. Move the slot to its owning kind, change :kind to match \
         (Binario for :exe, Servico for :servicos), or drop the slot entirely"
    )]
    ForeignCodeSlot {
        caixa: String,
        kind: CaixaKind,
        slots: String,
    },
    #[error("caixa '{0}' is an Acao but has no :ci slot")]
    MissingCi(String),
    #[error(
        "caixa '{caixa}' is :kind {kind:?} but declares the Acao-only :ci slot — \
         :ci carries a typed CI run (canteiro_types::CiRun, CANTEIRO §7.1-C) that only the \
         caixa-actions renderer validates for :kind Acao, and is silently ignored on every \
         other kind (never decomposed, never rendered); move it to a :kind Acao caixa or \
         remove it"
    )]
    CiOnNonAcao { caixa: String, kind: CaixaKind },
}

// Fold the layout-pipeline per-Caixa violation wrap onto one substrate
// primitive per typed slot. Every `LayoutError::*Violation { caixa, issue }`
// variant follows the same uniform shape — `caixa = layout-Caixa's :nome`,
// `issue = the gate's per-arm Display` — and every wire-up in
// [`StandardLayout::verify`] used to open-code the identical five-line
// `.map_err(|err| LayoutError::XxxViolation { caixa: caixa.nome().to_string(),
// issue: err.to_string() })` block, once per typed slot. Sixteen distinct
// wrap-variants × eighteen wire-up sites is exactly the duplication the
// PRIME DIRECTIVE names as a bug: every future consumer that wants to add a
// new per-slot gate (the deferred `caixa.pleme.io/v1alpha1/Caixa` CR
// materializer's admission webhook, a future per-slot `feira validate`
// verb, a per-slot overlay resolver) had to re-inline the five-line block
// in lockstep with the pre-existing wire-ups.
//
// The macro below generates one static constructor per variant of shape
// `fn <slot>_violation(caixa: &Caixa, err: impl Display) -> LayoutError`,
// so every wire-up site collapses onto one dispatch:
// `caixa.validate_<slot>().map_err(|err| LayoutError::<slot>_violation(caixa,
// err))?;`. The uniform two-slot construction (`caixa: caixa.nome()
// .to_string()`, `issue: err.to_string()`) is spelled once — inside the
// macro — rather than at every wire-up site. Every constructor is
// `#[must_use]` so a caller who mistakenly discards the wrapped error
// (rather than routing it through `?`) trips a compile warning at the
// wire-up site.
//
// Peer with the per-slot compound entry gates every substrate primitive
// on the M2/M3 typed-slot family already carries
// ([`crate::Caixa::validate_deps`] b5dd55e, [`crate::Caixa::validate_limits`]
// baa4688, [`crate::Caixa::validate_behavior`] 0d2877a,
// [`crate::Caixa::validate_upgrade_from`] d6801df,
// [`crate::Caixa::validate_aplicacao_shape`] 949a7a0,
// [`crate::AplicacaoSpec::validate_contratos`],
// [`crate::MeshPolicy::validate`],
// [`crate::SupervisorSpec::validate_children`]): the author-time gates
// fold onto one substrate primitive per typed slot; here the layout-side
// error-wrap folds onto one substrate primitive per typed variant, so the
// two sides of the layout pipeline's per-slot cascade (the gate, the
// wrap) each route through one call rather than N open-coded block
// repetitions.
macro_rules! layout_violation_ctors {
    ($($ctor:ident => $variant:ident),* $(,)?) => {
        impl LayoutError {
            $(
                #[doc = concat!(
                    "Construct a [`LayoutError::",
                    stringify!($variant),
                    "`] wrapping `err` under `caixa.nome()`. Folds the ",
                    "uniform `{ caixa: caixa.nome().to_string(), issue: ",
                    "err.to_string() }` two-slot construction onto one ",
                    "substrate primitive so every ",
                    "[`StandardLayout::verify`] wire-up on this variant ",
                    "reads through one dispatch rather than the pre-lift ",
                    "five-line open-coded block."
                )]
                #[must_use]
                pub fn $ctor<E: std::fmt::Display>(caixa: &crate::Caixa, err: E) -> Self {
                    Self::$variant {
                        caixa: caixa.nome().to_string(),
                        issue: err.to_string(),
                    }
                }
            )*
        }
    };
}

layout_violation_ctors! {
    nome_violation => NomeViolation,
    versao_violation => VersaoViolation,
    deps_violation => DepsViolation,
    etiquetas_violation => EtiquetasViolation,
    autores_violation => AutoresViolation,
    repositorio_violation => RepositorioViolation,
    descricao_violation => DescricaoViolation,
    licenca_violation => LicencaViolation,
    edicao_violation => EdicaoViolation,
    code_path_violation => CodePathViolation,
    limits_violation => LimitsViolation,
    behavior_violation => BehaviorViolation,
    upgrade_violation => UpgradeViolation,
    restart_window_violation => RestartWindowViolation,
    supervisor_violation => SupervisorViolation,
    aplicacao_violation => AplicacaoViolation,
    acao_violation => AcaoViolation,
}

// Fold the four `LayoutError::*SlotsOn*` / `LayoutError::ForeignCodeSlot`
// kind-coherence wrap sites onto one substrate primitive per typed variant —
// the sibling of [`layout_violation_ctors!`] above on the second uniform
// error-envelope shape `LayoutError` carries: `{ caixa: caixa.nome(),
// kind: caixa.kind(), slots: <declared_*_slots()>.join(" ") }`. Every
// wire-up in [`StandardLayout::verify`] on this shape (four sites: the
// M3-mesh gate on non-Aplicacao, the supervisor-tree gate on
// non-Supervisor, the M2-runtime gate on non-Servico, the code-surface
// `ForeignCodeSlot` gate) used to open-code the identical four-field
// `.{ caixa: caixa.nome().to_string(), kind: caixa.kind(), slots:
// <declared_*_slots>.join(" ") }` block — the exact "same block re-inlined
// at every consumer" shape the PRIME DIRECTIVE names as a bug on the same
// altitude the peer [`layout_violation_ctors!`] macro just closed on the
// `{ caixa, issue }` sibling shape.
//
// The macro below generates one static constructor per variant of shape
// `fn <slot>_on_non_<owner>(caixa: &Caixa, slots: Vec<&'static str>) ->
// LayoutError`, so every wire-up site collapses onto one dispatch:
// `return Err(LayoutError::<slot>_on_non_<owner>(caixa, <declared_slots>));`.
// The uniform four-field construction is spelled once — inside the macro —
// rather than at every wire-up site. Every constructor is `#[must_use]` so
// a caller who mistakenly discards the constructed error (rather than
// routing it through `return Err(…)`) trips a compile warning at the
// wire-up site.
//
// Peer with the `_violation` constructor family above on the same
// `LayoutError` — the two together now fold every uniform-shape
// `LayoutError` variant carried by [`StandardLayout::verify`] onto one
// substrate primitive per typed variant, so the layout-side error-wrap
// surface reads through one dispatch per variant rather than N open-coded
// blocks. Every future consumer that wants to construct one of these
// variants outside the layout pipeline (a per-slot admission webhook, a
// `feira validate --kind X` verb, an overlay resolver rejecting a
// kind-foreign patch) reaches its variant through one call, matching the
// `_violation` family's substrate-primitive discipline.
macro_rules! layout_slot_kind_ctors {
    ($($ctor:ident => $variant:ident),* $(,)?) => {
        impl LayoutError {
            $(
                #[doc = concat!(
                    "Construct a [`LayoutError::",
                    stringify!($variant),
                    "`] naming the offending slot list under `caixa.nome()` ",
                    "at `caixa.kind()`. Folds the uniform `{ caixa: caixa.",
                    "nome().to_string(), kind: caixa.kind(), slots: slots.",
                    "join(\" \") }` four-field construction onto one substrate ",
                    "primitive so every [`StandardLayout::verify`] wire-up on ",
                    "this variant reads through one dispatch rather than the ",
                    "pre-lift open-coded block."
                )]
                #[must_use]
                pub fn $ctor(caixa: &crate::Caixa, slots: Vec<&'static str>) -> Self {
                    Self::$variant {
                        caixa: caixa.nome().to_string(),
                        kind: caixa.kind(),
                        slots: slots.join(" "),
                    }
                }
            )*
        }
    };
}

layout_slot_kind_ctors! {
    mesh_slots_on_non_aplicacao => MeshSlotsOnNonAplicacao,
    supervisor_slots_on_non_supervisor => SupervisorSlotsOnNonSupervisor,
    servico_slots_on_non_servico => ServicoSlotsOnNonServico,
    foreign_code_slot => ForeignCodeSlot,
}

// Fold the five [`LayoutError::MissingEntry`] wire-up sites at
// [`StandardLayout::verify`] onto one substrate primitive on `LayoutError` —
// the third and last uniform-shape envelope on `LayoutError` after the
// `{ caixa, issue }` family the [`layout_violation_ctors!`] macro closed
// (131ca0d) and the `{ caixa, kind, slots }` family the peer
// [`layout_slot_kind_ctors!`] macro closed (0419438). Each of the five
// wire-up sites on `MissingEntry` (`:bibliotecas` iteration line 823,
// `:exe` iteration line 834, `:servicos` iteration line 848, `:behavior`
// on-disk callback-path iteration line 957, `:upgrade-from` per-
// instruction script-path iteration line 1021) opened the same four-line
// `LayoutError::MissingEntry { kind: crate::render::LAYOUT_MISSING_ENTRY_KIND_<slot>,
// path: full }` struct-literal block — the exact "same block re-inlined
// at every consumer" shape the PRIME DIRECTIVE names as a bug, on the
// same altitude the peer `_violation` / `_slots_on_non_*` families each
// closed on the sibling `LayoutError` envelopes.
//
// One `#[must_use]` inherent constructor on `LayoutError` collapses the
// five sites onto one dispatch:
// `return Err(LayoutError::missing_entry(<kind-label>, full));`, byte-
// equal to the pre-lift struct-literal block. A macro is not warranted
// on the one-variant envelope shape `{ kind: &'static str, path: PathBuf }`
// (unlike the sibling 16-variant `_violation` / 4-variant
// `_slots_on_non_*` shapes), but the same substrate-primitive discipline
// applies: every future consumer that wants to construct a `MissingEntry`
// outside the layout pipeline (a per-slot admission webhook probing a
// declared path against an out-of-band filesystem oracle, a `feira
// validate --lib` / `--exe` / `--servico` / `--behavior` / `--upgrade`
// per-caixa admission verb, the deferred `caixa.pleme.io/v1alpha1/Caixa`
// CR materializer's admission-webhook floor, a per-cluster overlay
// resolver rejecting a missing entry against a cluster-local filesystem
// snapshot) reaches the variant through one call rather than re-inlining
// the four-line struct-literal block in lockstep with the five
// layout-pipeline wire-up sites.
impl LayoutError {
    /// Construct a [`LayoutError::MissingEntry`] naming the missing
    /// declared entry at `path` under the canonical `kind` label from
    /// [`crate::render::LAYOUT_MISSING_ENTRY_KIND_BIBLIOTECA`] /
    /// [`crate::render::LAYOUT_MISSING_ENTRY_KIND_EXE`] /
    /// [`crate::render::LAYOUT_MISSING_ENTRY_KIND_SERVICO`] /
    /// [`crate::render::LAYOUT_MISSING_ENTRY_KIND_BEHAVIOR_CALLBACK`] /
    /// [`crate::render::LAYOUT_MISSING_ENTRY_KIND_UPGRADE_SCRIPT`].
    /// Folds the uniform `{ kind, path }` two-slot construction onto one
    /// substrate primitive so every [`StandardLayout::verify`] wire-up
    /// on this variant reads through one dispatch rather than the
    /// pre-lift open-coded struct-literal block.
    #[must_use]
    pub fn missing_entry(kind: &'static str, path: PathBuf) -> Self {
        Self::MissingEntry { kind, path }
    }
}

// Fold the six `LayoutError::<Variant>(caixa.nome().to_string())` nome-
// only tuple-variant wire-up sites at [`StandardLayout::verify`] onto one
// substrate primitive per typed variant — the fourth uniform-shape
// envelope on `LayoutError` after the `{ caixa, issue }` family the
// [`layout_violation_ctors!`] macro closed (131ca0d), the
// `{ caixa, kind, slots }` family the peer [`layout_slot_kind_ctors!`]
// macro closed (0419438), and the `{ kind, path }`
// [`LayoutError::missing_entry`] one-variant ctor (1b09f9d). Each of the
// six wire-up sites on this shape (`SupervisorOwnsCode` /
// `AplicacaoOwnsCode` / `AcaoOwnsCode` at the no-code kind-coherence gate;
// `BinarioWithoutExe` / `ServicoWithoutServicos` / `MissingCi` at the
// required-slot gate) opened the identical one-line
// `LayoutError::<Variant>(caixa.nome().to_string())` tuple-literal — the
// exact "same block re-inlined at every consumer" shape the PRIME
// DIRECTIVE names as a bug, on the same altitude the peer `_violation` /
// `_slots_on_non_*` / `missing_entry` families each closed on the
// sibling `LayoutError` envelopes.
//
// The macro below generates one static constructor per variant of shape
// `fn <slot>(caixa: &Caixa) -> LayoutError`, so every wire-up site
// collapses onto one dispatch:
// `return Err(LayoutError::<slot>(caixa));`, byte-equal to the pre-lift
// tuple-literal. The uniform one-field construction (`caixa: caixa.nome()
// .to_string()`) is spelled once — inside the macro — rather than at
// every wire-up site. Every constructor is `#[must_use]` so a caller who
// mistakenly discards the constructed error (rather than routing it
// through `return Err(…)`) trips a compile warning at the wire-up site.
//
// Peer with the three prior `LayoutError`-envelope constructor families
// — the four together now fold every uniform-shape `LayoutError` variant
// carried by [`StandardLayout::verify`] onto one substrate primitive per
// typed variant, so every layout-side error-wrap on `LayoutError` reads
// through one dispatch per variant rather than N open-coded blocks.
// Every future consumer that wants to construct one of these variants
// outside the layout pipeline (a per-slot admission webhook probing an
// Acao's `:ci` slot, a `feira validate --kind <X>` verb refusing a
// no-code kind that declares `:bibliotecas` / `:exe` / `:servicos`, an
// overlay resolver rejecting a required-slot omission against a
// cluster-local snapshot) reaches its variant through one call, matching
// the `_violation` / `_slots_on_non_*` / `missing_entry` families'
// substrate-primitive discipline.
macro_rules! layout_nome_only_ctors {
    ($($ctor:ident => $variant:ident),* $(,)?) => {
        impl LayoutError {
            $(
                #[doc = concat!(
                    "Construct a [`LayoutError::",
                    stringify!($variant),
                    "`] naming the offending `caixa.nome()`. Folds the ",
                    "uniform `Self::",
                    stringify!($variant),
                    "(caixa.nome().to_string())` one-field tuple-",
                    "literal onto one substrate primitive so every ",
                    "[`StandardLayout::verify`] wire-up on this variant ",
                    "reads through one dispatch rather than the pre-lift ",
                    "open-coded block."
                )]
                #[must_use]
                pub fn $ctor(caixa: &crate::Caixa) -> Self {
                    Self::$variant(caixa.nome().to_string())
                }
            )*
        }
    };
}

layout_nome_only_ctors! {
    binario_without_exe => BinarioWithoutExe,
    servico_without_servicos => ServicoWithoutServicos,
    missing_ci => MissingCi,
    supervisor_owns_code => SupervisorOwnsCode,
    aplicacao_owns_code => AplicacaoOwnsCode,
    acao_owns_code => AcaoOwnsCode,
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
            ci: None,
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

    // ── LayoutError::*_violation constructor family ──────────────────────
    //
    // The [`layout_violation_ctors!`] macro (below the `LayoutError` enum
    // definition) generates one static constructor per `*Violation { caixa,
    // issue }` variant that folds the uniform `{ caixa: caixa.nome()
    // .to_string(), issue: err.to_string() }` two-slot construction onto
    // one substrate primitive. The per-variant equivalence pins below
    // (fail-before-pass-after by construction — a byte-mismatched macro
    // arm would trip its equivalence pin first) lock each generated
    // constructor to its struct-literal peer under `PartialEq`, so every
    // wire-up in [`StandardLayout::verify`] on that variant produces a
    // byte-equal `LayoutError` to the pre-lift open-coded block. The
    // fixture caixa fires under `caixa("demo")` so the `caixa: "demo"`
    // half is pinned; the fixture error fires under a fixed `&str` so the
    // `issue: <literal>` half is pinned; the two together pin every field
    // of every generated variant.

    fn layout_violation_ctor_fixture() -> (Caixa, &'static str) {
        (caixa(CaixaKind::Biblioteca), "sample issue text")
    }

    // `assert_eq!` uses `PartialEq::eq(&self, &other)` under the hood, so
    // `actual`/`expected` are only ever read, never moved into anything —
    // the ergonomic tradeoff (owned + move-in vs. reference + &-borrow at
    // every call site) favors the owned form for a test-only assertion
    // helper called from 17 wire-up pins. The lint targets the general
    // API-shape case where callers still have downstream uses for the
    // moved value; the assertion helper terminates on the equality check.
    #[allow(clippy::needless_pass_by_value)]
    fn assert_violation_ctor_matches(actual: LayoutError, expected: LayoutError) {
        assert_eq!(
            actual, expected,
            "generated constructor must produce byte-equal LayoutError to open-coded struct-literal wrap",
        );
    }

    #[test]
    fn nome_violation_ctor_matches_struct_literal_wrap() {
        let (c, issue) = layout_violation_ctor_fixture();
        assert_violation_ctor_matches(
            LayoutError::nome_violation(&c, issue),
            LayoutError::NomeViolation {
                caixa: c.nome().to_string(),
                issue: issue.to_string(),
            },
        );
    }

    #[test]
    fn versao_violation_ctor_matches_struct_literal_wrap() {
        let (c, issue) = layout_violation_ctor_fixture();
        assert_violation_ctor_matches(
            LayoutError::versao_violation(&c, issue),
            LayoutError::VersaoViolation {
                caixa: c.nome().to_string(),
                issue: issue.to_string(),
            },
        );
    }

    #[test]
    fn deps_violation_ctor_matches_struct_literal_wrap() {
        let (c, issue) = layout_violation_ctor_fixture();
        assert_violation_ctor_matches(
            LayoutError::deps_violation(&c, issue),
            LayoutError::DepsViolation {
                caixa: c.nome().to_string(),
                issue: issue.to_string(),
            },
        );
    }

    #[test]
    fn etiquetas_violation_ctor_matches_struct_literal_wrap() {
        let (c, issue) = layout_violation_ctor_fixture();
        assert_violation_ctor_matches(
            LayoutError::etiquetas_violation(&c, issue),
            LayoutError::EtiquetasViolation {
                caixa: c.nome().to_string(),
                issue: issue.to_string(),
            },
        );
    }

    #[test]
    fn autores_violation_ctor_matches_struct_literal_wrap() {
        let (c, issue) = layout_violation_ctor_fixture();
        assert_violation_ctor_matches(
            LayoutError::autores_violation(&c, issue),
            LayoutError::AutoresViolation {
                caixa: c.nome().to_string(),
                issue: issue.to_string(),
            },
        );
    }

    #[test]
    fn repositorio_violation_ctor_matches_struct_literal_wrap() {
        let (c, issue) = layout_violation_ctor_fixture();
        assert_violation_ctor_matches(
            LayoutError::repositorio_violation(&c, issue),
            LayoutError::RepositorioViolation {
                caixa: c.nome().to_string(),
                issue: issue.to_string(),
            },
        );
    }

    #[test]
    fn descricao_violation_ctor_matches_struct_literal_wrap() {
        let (c, issue) = layout_violation_ctor_fixture();
        assert_violation_ctor_matches(
            LayoutError::descricao_violation(&c, issue),
            LayoutError::DescricaoViolation {
                caixa: c.nome().to_string(),
                issue: issue.to_string(),
            },
        );
    }

    #[test]
    fn licenca_violation_ctor_matches_struct_literal_wrap() {
        let (c, issue) = layout_violation_ctor_fixture();
        assert_violation_ctor_matches(
            LayoutError::licenca_violation(&c, issue),
            LayoutError::LicencaViolation {
                caixa: c.nome().to_string(),
                issue: issue.to_string(),
            },
        );
    }

    #[test]
    fn edicao_violation_ctor_matches_struct_literal_wrap() {
        let (c, issue) = layout_violation_ctor_fixture();
        assert_violation_ctor_matches(
            LayoutError::edicao_violation(&c, issue),
            LayoutError::EdicaoViolation {
                caixa: c.nome().to_string(),
                issue: issue.to_string(),
            },
        );
    }

    #[test]
    fn code_path_violation_ctor_matches_struct_literal_wrap() {
        let (c, issue) = layout_violation_ctor_fixture();
        assert_violation_ctor_matches(
            LayoutError::code_path_violation(&c, issue),
            LayoutError::CodePathViolation {
                caixa: c.nome().to_string(),
                issue: issue.to_string(),
            },
        );
    }

    #[test]
    fn limits_violation_ctor_matches_struct_literal_wrap() {
        let (c, issue) = layout_violation_ctor_fixture();
        assert_violation_ctor_matches(
            LayoutError::limits_violation(&c, issue),
            LayoutError::LimitsViolation {
                caixa: c.nome().to_string(),
                issue: issue.to_string(),
            },
        );
    }

    #[test]
    fn behavior_violation_ctor_matches_struct_literal_wrap() {
        let (c, issue) = layout_violation_ctor_fixture();
        assert_violation_ctor_matches(
            LayoutError::behavior_violation(&c, issue),
            LayoutError::BehaviorViolation {
                caixa: c.nome().to_string(),
                issue: issue.to_string(),
            },
        );
    }

    #[test]
    fn upgrade_violation_ctor_matches_struct_literal_wrap() {
        let (c, issue) = layout_violation_ctor_fixture();
        assert_violation_ctor_matches(
            LayoutError::upgrade_violation(&c, issue),
            LayoutError::UpgradeViolation {
                caixa: c.nome().to_string(),
                issue: issue.to_string(),
            },
        );
    }

    #[test]
    fn restart_window_violation_ctor_matches_struct_literal_wrap() {
        let (c, issue) = layout_violation_ctor_fixture();
        assert_violation_ctor_matches(
            LayoutError::restart_window_violation(&c, issue),
            LayoutError::RestartWindowViolation {
                caixa: c.nome().to_string(),
                issue: issue.to_string(),
            },
        );
    }

    #[test]
    fn supervisor_violation_ctor_matches_struct_literal_wrap() {
        let (c, issue) = layout_violation_ctor_fixture();
        assert_violation_ctor_matches(
            LayoutError::supervisor_violation(&c, issue),
            LayoutError::SupervisorViolation {
                caixa: c.nome().to_string(),
                issue: issue.to_string(),
            },
        );
    }

    #[test]
    fn aplicacao_violation_ctor_matches_struct_literal_wrap() {
        let (c, issue) = layout_violation_ctor_fixture();
        assert_violation_ctor_matches(
            LayoutError::aplicacao_violation(&c, issue),
            LayoutError::AplicacaoViolation {
                caixa: c.nome().to_string(),
                issue: issue.to_string(),
            },
        );
    }

    #[test]
    fn acao_violation_ctor_matches_struct_literal_wrap() {
        // Sibling of [`aplicacao_violation_ctor_matches_struct_literal_wrap`]
        // / [`supervisor_violation_ctor_matches_struct_literal_wrap`] on
        // the third per-kind compound-shape wrap envelope on
        // `LayoutError`. Pins the macro-generated `acao_violation`
        // constructor to its struct-literal peer under `PartialEq`, so
        // every wire-up in [`StandardLayout::verify`] on the
        // [`LayoutError::AcaoViolation`] variant produces a byte-equal
        // `LayoutError` to the pre-lift open-coded block. Closes the
        // pin family the peer per-kind shape wraps already carry.
        let (c, issue) = layout_violation_ctor_fixture();
        assert_violation_ctor_matches(
            LayoutError::acao_violation(&c, issue),
            LayoutError::AcaoViolation {
                caixa: c.nome().to_string(),
                issue: issue.to_string(),
            },
        );
    }

    #[test]
    fn violation_ctor_routes_issue_through_display_impl() {
        // Pin the fold's `issue = err.to_string()` half against any type
        // implementing `Display` — a per-arm error type from a foreign
        // module (here, `std::io::Error`) threads through byte-equal to
        // the struct-literal `.to_string()` construction, so the fold
        // does not silently collapse onto `&str`-only inputs.
        let c = caixa(CaixaKind::Biblioteca);
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "sample io display source");
        let expected_issue = io_err.to_string();
        let actual = LayoutError::deps_violation(&c, &io_err);
        assert_eq!(
            actual,
            LayoutError::DepsViolation {
                caixa: c.nome().to_string(),
                issue: expected_issue,
            },
        );
    }

    #[test]
    fn violation_ctor_routes_caixa_prefix_through_nome_accessor() {
        // Pin the fold's `caixa = caixa.nome().to_string()` half against
        // a non-default `:nome` — the accessor threads the caller's
        // `:nome` verbatim into the wrap envelope, so the fold does not
        // silently collapse onto the default `"demo"` fixture nome.
        let mut c = caixa(CaixaKind::Biblioteca);
        c.nome = "alt-nome".into();
        let actual = LayoutError::behavior_violation(&c, "sample issue");
        assert_eq!(
            actual,
            LayoutError::BehaviorViolation {
                caixa: "alt-nome".to_string(),
                issue: "sample issue".to_string(),
            },
        );
    }

    // ── LayoutError kind-coherence constructor family ────────────────────
    //
    // The [`layout_slot_kind_ctors!`] macro (sibling of
    // [`layout_violation_ctors!`] beside the `LayoutError` enum definition)
    // generates one static constructor per `*SlotsOn*` / `ForeignCodeSlot`
    // variant that folds the uniform `{ caixa: caixa.nome().to_string(),
    // kind: caixa.kind(), slots: slots.join(" ") }` four-field construction
    // onto one substrate primitive. The per-variant equivalence pins below
    // (fail-before-pass-after by construction — a byte-mismatched macro arm
    // would trip its equivalence pin first) lock each generated constructor
    // to its struct-literal peer under `PartialEq`, so every wire-up in
    // [`StandardLayout::verify`] on that variant produces a byte-equal
    // `LayoutError` to the pre-lift open-coded block. The three cross-axis
    // pins that follow (non-default `:nome`, non-default kind, non-trivial
    // slots list) route each of the three constructor input axes through
    // its declared accessor / arg, so the fold does not silently collapse
    // onto a fixture default on any axis.

    fn layout_slot_kind_ctor_fixture() -> (Caixa, Vec<&'static str>) {
        (caixa(CaixaKind::Biblioteca), vec![":membros", ":contratos"])
    }

    // Same rationale as `assert_violation_ctor_matches` above: the helper
    // terminates on the equality check, so the owned-arg lint's general
    // API-shape target does not apply.
    #[allow(clippy::needless_pass_by_value)]
    fn assert_slot_kind_ctor_matches(actual: LayoutError, expected: LayoutError) {
        assert_eq!(
            actual, expected,
            "generated constructor must produce byte-equal LayoutError to open-coded struct-literal wrap",
        );
    }

    #[test]
    fn mesh_slots_on_non_aplicacao_ctor_matches_struct_literal_wrap() {
        let (c, slots) = layout_slot_kind_ctor_fixture();
        assert_slot_kind_ctor_matches(
            LayoutError::mesh_slots_on_non_aplicacao(&c, slots.clone()),
            LayoutError::MeshSlotsOnNonAplicacao {
                caixa: c.nome().to_string(),
                kind: c.kind(),
                slots: slots.join(" "),
            },
        );
    }

    #[test]
    fn supervisor_slots_on_non_supervisor_ctor_matches_struct_literal_wrap() {
        let (c, slots) = layout_slot_kind_ctor_fixture();
        assert_slot_kind_ctor_matches(
            LayoutError::supervisor_slots_on_non_supervisor(&c, slots.clone()),
            LayoutError::SupervisorSlotsOnNonSupervisor {
                caixa: c.nome().to_string(),
                kind: c.kind(),
                slots: slots.join(" "),
            },
        );
    }

    #[test]
    fn servico_slots_on_non_servico_ctor_matches_struct_literal_wrap() {
        let (c, slots) = layout_slot_kind_ctor_fixture();
        assert_slot_kind_ctor_matches(
            LayoutError::servico_slots_on_non_servico(&c, slots.clone()),
            LayoutError::ServicoSlotsOnNonServico {
                caixa: c.nome().to_string(),
                kind: c.kind(),
                slots: slots.join(" "),
            },
        );
    }

    #[test]
    fn foreign_code_slot_ctor_matches_struct_literal_wrap() {
        let (c, slots) = layout_slot_kind_ctor_fixture();
        assert_slot_kind_ctor_matches(
            LayoutError::foreign_code_slot(&c, slots.clone()),
            LayoutError::ForeignCodeSlot {
                caixa: c.nome().to_string(),
                kind: c.kind(),
                slots: slots.join(" "),
            },
        );
    }

    #[test]
    fn slot_kind_ctor_routes_caixa_prefix_through_nome_accessor() {
        // Pin the fold's `caixa = caixa.nome().to_string()` half against a
        // non-default `:nome` — the accessor threads the caller's `:nome`
        // verbatim into the wrap envelope, so the fold does not silently
        // collapse onto the default `"demo"` fixture nome. Peer of the
        // sibling `violation_ctor_routes_caixa_prefix_through_nome_accessor`
        // pin on the `{ caixa, issue }` envelope; extended here onto the
        // `{ caixa, kind, slots }` envelope so both `LayoutError`-shape
        // constructor families guarantee the `:nome`-derived-caixa slot
        // routes through [`Caixa::nome`] rather than a hard-coded string.
        let mut c = caixa(CaixaKind::Biblioteca);
        c.nome = "alt-nome".into();
        let actual = LayoutError::mesh_slots_on_non_aplicacao(&c, vec![":membros"]);
        assert_eq!(
            actual,
            LayoutError::MeshSlotsOnNonAplicacao {
                caixa: "alt-nome".to_string(),
                kind: CaixaKind::Biblioteca,
                slots: ":membros".to_string(),
            },
        );
    }

    #[test]
    fn slot_kind_ctor_routes_kind_through_caixa_kind_accessor() {
        // Pin the fold's `kind = caixa.kind()` half against a non-default
        // kind — the accessor threads the caller's `:kind` verbatim into
        // the wrap envelope, so the fold does not silently collapse onto
        // one hard-coded kind. Sweeps every non-Aplicacao / non-Supervisor
        // / non-Servico kind the corresponding gate can fire on so the
        // pin covers the kind-derivation axis on every downstream variant.
        for kind in [
            CaixaKind::Biblioteca,
            CaixaKind::Binario,
            CaixaKind::Servico,
            CaixaKind::Supervisor,
            CaixaKind::Aplicacao,
            CaixaKind::Acao,
        ] {
            let c = caixa(kind);
            let actual = LayoutError::foreign_code_slot(&c, vec![":exe"]);
            assert_eq!(
                actual,
                LayoutError::ForeignCodeSlot {
                    caixa: c.nome().to_string(),
                    kind,
                    slots: ":exe".to_string(),
                },
                "foreign_code_slot ctor must thread `caixa.kind()` verbatim on every kind",
            );
        }
    }

    #[test]
    fn slot_kind_ctor_routes_slots_through_join_separator() {
        // Pin the fold's `slots = slots.join(" ")` half against a
        // multi-entry slots list — the join threads exactly one ASCII
        // space between entries, in caller-supplied order, so the fold
        // does not silently collapse onto a fixed separator (`", "`, `";
        // "`, `"\n"`), a sorted order, or a single-entry pass-through.
        // Uses the M2 servico-slot vocabulary since these are what the
        // corresponding `servico_slots_on_non_servico` gate reports.
        let c = caixa(CaixaKind::Biblioteca);
        let actual = LayoutError::servico_slots_on_non_servico(
            &c,
            vec![":limits", ":behavior", ":upgrade-from"],
        );
        assert_eq!(
            actual,
            LayoutError::ServicoSlotsOnNonServico {
                caixa: c.nome().to_string(),
                kind: c.kind(),
                slots: ":limits :behavior :upgrade-from".to_string(),
            },
        );
    }

    // ── LayoutError::missing_entry substrate-primitive constructor ───────
    //
    // The [`LayoutError::missing_entry`] constructor beside the enum
    // definition folds the `{ kind: &'static str, path: PathBuf }`
    // uniform-shape envelope onto one substrate primitive — the third
    // and last uniform-shape envelope on `LayoutError` after the
    // `{ caixa, issue }` family the [`layout_violation_ctors!`] macro
    // closed (131ca0d) and the `{ caixa, kind, slots }` family the peer
    // [`layout_slot_kind_ctors!`] macro closed (0419438). The pins below
    // (fail-before-pass-after by construction — a byte-mismatched
    // constructor arm would trip its equivalence pin first) lock the
    // constructor to its struct-literal peer under `PartialEq`, so every
    // wire-up in [`StandardLayout::verify`] on this variant produces a
    // byte-equal `LayoutError` to the pre-lift open-coded block. The two
    // cross-axis pins that follow (canonical-kind-label sweep, non-
    // default path) route each of the two constructor input axes through
    // its arg verbatim, so the fold does not silently collapse onto a
    // fixture default on either axis.

    #[test]
    fn missing_entry_ctor_matches_struct_literal_wrap() {
        // Per-envelope equivalence pin — the `missing_entry` constructor
        // produces a `LayoutError::MissingEntry` byte-equal under
        // `PartialEq` to the open-coded four-line struct-literal wrap on
        // the same `(kind, path)` fixture. Peer of the sibling
        // `<slot>_violation_ctor_matches_struct_literal_wrap` /
        // `<slot>_slots_on_non_<owner>_ctor_matches_struct_literal_wrap`
        // pins on the two prior uniform-shape envelopes on the same
        // `LayoutError`.
        let path = PathBuf::from("/tmp/x/lib/demo.lisp");
        assert_eq!(
            LayoutError::missing_entry(
                crate::render::LAYOUT_MISSING_ENTRY_KIND_BIBLIOTECA,
                path.clone(),
            ),
            LayoutError::MissingEntry {
                kind: crate::render::LAYOUT_MISSING_ENTRY_KIND_BIBLIOTECA,
                path,
            },
        );
    }

    #[test]
    fn missing_entry_ctor_routes_kind_through_arg_verbatim() {
        // Pin the fold's `kind: &'static str` arg through every canonical
        // [`crate::render::LAYOUT_MISSING_ENTRY_KIND_*`] label the five
        // wire-up sites in [`StandardLayout::verify`] pass — so the fold
        // does not silently collapse onto one hard-coded label. Sweep
        // matches the arm set the peer
        // `layout_missing_entry_kind_m2_consts_pin_canonical_kebab_case_labels`
        // pin (below) covers on the const-label declarations.
        let path = PathBuf::from("/tmp/x/entry");
        for kind in [
            crate::render::LAYOUT_MISSING_ENTRY_KIND_BIBLIOTECA,
            crate::render::LAYOUT_MISSING_ENTRY_KIND_EXE,
            crate::render::LAYOUT_MISSING_ENTRY_KIND_SERVICO,
            crate::render::LAYOUT_MISSING_ENTRY_KIND_BEHAVIOR_CALLBACK,
            crate::render::LAYOUT_MISSING_ENTRY_KIND_UPGRADE_SCRIPT,
        ] {
            assert_eq!(
                LayoutError::missing_entry(kind, path.clone()),
                LayoutError::MissingEntry {
                    kind,
                    path: path.clone(),
                },
                "missing_entry ctor must thread `kind` verbatim on every canonical label",
            );
        }
    }

    #[test]
    fn missing_entry_ctor_routes_path_through_arg_verbatim() {
        // Pin the fold's `path: PathBuf` arg against a non-default,
        // multi-component `PathBuf` — the ctor threads the caller's
        // `PathBuf` verbatim into the wrap envelope, so the fold does
        // not silently collapse onto a fixed component prefix, a
        // canonicalized form, or a single-component pass-through.
        let path = PathBuf::from("/alt/root")
            .join("servicos")
            .join("hello-rio.computeunit.yaml");
        assert_eq!(
            LayoutError::missing_entry(
                crate::render::LAYOUT_MISSING_ENTRY_KIND_SERVICO,
                path.clone(),
            ),
            LayoutError::MissingEntry {
                kind: crate::render::LAYOUT_MISSING_ENTRY_KIND_SERVICO,
                path,
            },
        );
    }

    // ── LayoutError::<nome-only> constructor family ──────────────────────
    //
    // The [`layout_nome_only_ctors!`] macro (below the `LayoutError` enum
    // definition) generates one static constructor per `<Variant>(String)`
    // tuple-variant that folds the uniform `Self::<Variant>(caixa.nome()
    // .to_string())` one-field construction onto one substrate primitive.
    // The per-variant equivalence pins below (fail-before-pass-after by
    // construction — a byte-mismatched macro arm would trip its
    // equivalence pin first) lock each generated constructor to its
    // tuple-literal peer under `PartialEq`, so every wire-up in
    // [`StandardLayout::verify`] on that variant produces a byte-equal
    // `LayoutError` to the pre-lift open-coded tuple-literal. The
    // cross-axis pin that follows (non-default `:nome`) routes the sole
    // constructor input axis through its declared accessor, so the fold
    // does not silently collapse onto the fixture default `:nome`.

    fn layout_nome_only_ctor_fixture() -> Caixa {
        caixa(CaixaKind::Biblioteca)
    }

    // Same rationale as `assert_violation_ctor_matches` / `assert_slot_
    // kind_ctor_matches` above: the helper terminates on the equality
    // check, so the owned-arg lint's general API-shape target does not
    // apply.
    #[allow(clippy::needless_pass_by_value)]
    fn assert_nome_only_ctor_matches(actual: LayoutError, expected: LayoutError) {
        assert_eq!(
            actual, expected,
            "generated constructor must produce byte-equal LayoutError to open-coded tuple-literal wrap",
        );
    }

    #[test]
    fn binario_without_exe_ctor_matches_tuple_literal_wrap() {
        let c = layout_nome_only_ctor_fixture();
        assert_nome_only_ctor_matches(
            LayoutError::binario_without_exe(&c),
            LayoutError::BinarioWithoutExe(c.nome().to_string()),
        );
    }

    #[test]
    fn servico_without_servicos_ctor_matches_tuple_literal_wrap() {
        let c = layout_nome_only_ctor_fixture();
        assert_nome_only_ctor_matches(
            LayoutError::servico_without_servicos(&c),
            LayoutError::ServicoWithoutServicos(c.nome().to_string()),
        );
    }

    #[test]
    fn missing_ci_ctor_matches_tuple_literal_wrap() {
        let c = layout_nome_only_ctor_fixture();
        assert_nome_only_ctor_matches(
            LayoutError::missing_ci(&c),
            LayoutError::MissingCi(c.nome().to_string()),
        );
    }

    #[test]
    fn supervisor_owns_code_ctor_matches_tuple_literal_wrap() {
        let c = layout_nome_only_ctor_fixture();
        assert_nome_only_ctor_matches(
            LayoutError::supervisor_owns_code(&c),
            LayoutError::SupervisorOwnsCode(c.nome().to_string()),
        );
    }

    #[test]
    fn aplicacao_owns_code_ctor_matches_tuple_literal_wrap() {
        let c = layout_nome_only_ctor_fixture();
        assert_nome_only_ctor_matches(
            LayoutError::aplicacao_owns_code(&c),
            LayoutError::AplicacaoOwnsCode(c.nome().to_string()),
        );
    }

    #[test]
    fn acao_owns_code_ctor_matches_tuple_literal_wrap() {
        let c = layout_nome_only_ctor_fixture();
        assert_nome_only_ctor_matches(
            LayoutError::acao_owns_code(&c),
            LayoutError::AcaoOwnsCode(c.nome().to_string()),
        );
    }

    #[test]
    fn nome_only_ctor_routes_caixa_through_nome_accessor() {
        // Pin the fold's `caixa.nome().to_string()` sole-field construction
        // against a non-default `:nome` — the accessor threads the caller's
        // `:nome` verbatim into the tuple-variant, so the fold does not
        // silently collapse onto the default `"demo"` fixture nome. Peer of
        // the sibling `violation_ctor_routes_caixa_prefix_through_nome_
        // accessor` / `slot_kind_ctor_routes_caixa_prefix_through_nome_
        // accessor` pins on the `{ caixa, issue }` / `{ caixa, kind,
        // slots }` envelopes; extended here onto the sixth
        // `<Variant>(String)` envelope so every LayoutError-shape ctor
        // family guarantees the `:nome`-derived-caixa slot routes through
        // [`Caixa::nome`] rather than a hard-coded string. Sweeps the six
        // ctors in the [`layout_nome_only_ctors!`] macro so the pin covers
        // every generated arm.
        let mut c = caixa(CaixaKind::Biblioteca);
        c.nome = "alt-nome".into();
        assert_eq!(
            LayoutError::binario_without_exe(&c),
            LayoutError::BinarioWithoutExe("alt-nome".to_string()),
        );
        assert_eq!(
            LayoutError::servico_without_servicos(&c),
            LayoutError::ServicoWithoutServicos("alt-nome".to_string()),
        );
        assert_eq!(
            LayoutError::missing_ci(&c),
            LayoutError::MissingCi("alt-nome".to_string()),
        );
        assert_eq!(
            LayoutError::supervisor_owns_code(&c),
            LayoutError::SupervisorOwnsCode("alt-nome".to_string()),
        );
        assert_eq!(
            LayoutError::aplicacao_owns_code(&c),
            LayoutError::AplicacaoOwnsCode("alt-nome".to_string()),
        );
        assert_eq!(
            LayoutError::acao_owns_code(&c),
            LayoutError::AcaoOwnsCode("alt-nome".to_string()),
        );
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
        // A relative entry that lives under the caixa root but *not*
        // under `exe/` — the canonical case the `starts_with(exe_dir)`
        // fence catches. The prior parent-escape shape this test used
        // (`"../sibling/tool"`) is now caught at validate time by
        // [`Caixa::validate_code_paths`] with the narrower
        // [`crate::ManifestError::CodePathParentEscape`] diagnostic
        // (see the layout-level integration pin
        // `code_path_violation_on_parent_escape_fires_before_existence_check`),
        // so this fence pin uses a non-`..` non-absolute shape outside
        // `exe/` to preserve coverage of the ExeOutsideDir surface.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let outside = root.join("lib/tool");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest || p == outside);
        let mut c = caixa(CaixaKind::Binario);
        c.exe = vec!["lib/tool".into()];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(matches!(err, LayoutError::ExeOutsideDir(_)));
    }

    // ── code-path shape gate (lifted to layout-level verify) ─────────────

    #[test]
    fn code_path_violation_on_empty_bibliotecas_entry() {
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.bibliotecas = vec![String::new()];
        let err = layout.verify(&c, &root).unwrap_err();
        // The wire-up wraps `ManifestError` Display into the
        // CodePathViolation envelope (peer of LimitsViolation /
        // BehaviorViolation / UpgradeViolation), so the issue string
        // names the offending slot at the source.
        let LayoutError::CodePathViolation { caixa, issue } = err else {
            panic!("expected LayoutError::CodePathViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":bibliotecas"),
            "issue must name the offending slot: {issue}",
        );
    }

    #[test]
    fn code_path_violation_on_absolute_servicos_entry() {
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["/etc/servicos/escape.yaml".into()];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::CodePathViolation { caixa, issue } = err else {
            panic!("expected LayoutError::CodePathViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":servicos"),
            "issue must name the offending slot: {issue}",
        );
        assert!(
            issue.contains("/etc/servicos/escape.yaml"),
            "issue must quote the offending path: {issue}",
        );
    }

    #[test]
    fn code_path_violation_on_parent_escape_fires_before_existence_check() {
        // The new gate runs BEFORE the existence loops, so a
        // parent-escaping `:exe` entry surfaces CodePathViolation
        // (naming `:exe` at the source) rather than the downstream
        // ExeOutsideDir / MissingEntry against the resolved sandbox-
        // escape path. Even if the resolved escape target exists
        // on disk (which we simulate here by claiming it does), the
        // shape diagnostic wins.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let resolved_escape = root.join("exe/../../escape.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == resolved_escape);
        let mut c = caixa(CaixaKind::Binario);
        c.exe = vec!["exe/../../escape.lisp".into()];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::CodePathViolation { caixa, issue } = err else {
            panic!("expected LayoutError::CodePathViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":exe"),
            "issue must name the offending slot: {issue}",
        );
    }

    // ── etiquetas universal-axis gate wired into verify ─────────────────
    //
    // Pins the layout-pipeline wire-up of [`Caixa::validate_etiquetas`]:
    // the fourth universal-axis Caixa-level value-shape gate (peer of
    // `validate_nome` / `validate_versao` / `validate_deps` /
    // `validate_code_paths`), wired before the kind-coherence gates so
    // a structurally-invalid `:etiquetas` entry on any kind surfaces
    // the per-axis `EtiquetasViolation { caixa, issue }` envelope at
    // the source rather than silently rendering as `keywords: [""]`
    // in `Chart.yaml` (Servico kind, via caixa-helm's `BTreeSet`
    // collect) or silently dedup'ing at chart render (every kind).
    // Until this wire-up landed `:etiquetas` had no shape gate at any
    // layer — the registry-search-tag axis was the largest universal
    // authoring surface on the typed Caixa surface with no validate
    // discipline.
    //
    // Same per-axis `*Violation { caixa, issue }` envelope every peer
    // per-axis wrap exposes; the wire-up runs after `validate_deps`
    // (universal axis ordering: `:nome` → `:versao` → `:deps` →
    // `:etiquetas`) and before every kind-coherence gate
    // (`:etiquetas` is universal so its shape diagnostic is more
    // fundamental than the partition-on-kind diagnostics).

    #[test]
    fn etiquetas_violation_on_empty_entry() {
        // Canonical paste-from-blank-doc footgun on every kind. The
        // wrap envelope wraps [`ManifestError::EtiquetaEmpty`]'s
        // Display through verbatim, so the issue string names the
        // offending `:etiquetas` axis at the source — the author can
        // grep their caixa.lisp for `:etiquetas` and fix the empty
        // entry in one edit. Mirrors the peer
        // `code_path_violation_on_empty_bibliotecas_entry` shape
        // (b868442) on the `:bibliotecas` axis.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.etiquetas = vec![String::new()];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::EtiquetasViolation { caixa, issue } = err else {
            panic!("expected LayoutError::EtiquetasViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":etiquetas"),
            "issue must name the offending slot: {issue}",
        );
    }

    #[test]
    fn etiquetas_violation_on_duplicate_entry() {
        // Canonical copy-paste-the-wrong-tag footgun. Without the wire-
        // up the duplicate was silently dedup'd by caixa-helm's
        // `BTreeSet` collect at chart render — a "second wins / one
        // silently disappears" shape. The wrap envelope names the
        // offending tag verbatim through the inner
        // [`ManifestError::EtiquetaDuplicate`]'s Display.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.etiquetas = vec!["demo".into(), "demo".into()];
        // The servicos path doesn't exist in this fixture, but the
        // `:etiquetas` gate fires before the existence loop (universal
        // axis dominates kind-specific existence checks). Wire is
        // intact iff the wrap envelope surfaces first.
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::EtiquetasViolation { caixa, issue } = err else {
            panic!("expected LayoutError::EtiquetasViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("demo"),
            "issue must quote the offending tag: {issue}",
        );
    }

    #[test]
    fn etiquetas_violation_fires_before_kind_coherence_mesh_slot() {
        // Cross-axis precedence pin: a Biblioteca with malformed
        // `:etiquetas` *and* declared mesh slots (`:membros`) surfaces
        // the universal `:etiquetas` diagnostic first, not the
        // kind-coherence `MeshSlotsOnNonAplicacao` diagnostic.
        // `:etiquetas` is universal (every kind owns the slot), so its
        // shape diagnostic is more fundamental than the partition-on-
        // kind diagnostic. Mirrors the peer
        // `deps_violation_fires_before_*` precedence pins (aa77d0f) on
        // the universal `:deps` axis vs the same kind-coherence gates.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.etiquetas = vec![String::new()];
        c.membros = vec![crate::aplicacao::Membro {
            caixa: "x".into(),
            versao: "^0.1".into(),
        }];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::EtiquetasViolation { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn etiquetas_violation_fires_after_deps_violation() {
        // Cross-axis precedence pin (inside the universal-axis trio):
        // a caixa with both a malformed `:deps` entry *and* a malformed
        // `:etiquetas` entry surfaces `DepsViolation` first — `:deps`
        // is the third universal axis in declaration order
        // (`:nome` → `:versao` → `:deps` → `:etiquetas`) and runs first
        // in `verify`. Mirrors the peer
        // `nome_violation_fires_before_versao_violation` shape on the
        // identity-axis pair.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.deps = vec![crate::Dep::simple("Caixa-Teia", "^0.1")]; // uppercase :nome
        c.etiquetas = vec![String::new()];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::DepsViolation { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn etiquetas_violation_accepts_canonical_template() {
        // Positive control sanity pin: the canonical `Caixa::template`
        // shape (`:etiquetas ()` — empty list) passes the gate
        // trivially. Mirrors the peer
        // `validate_code_paths_accepts_canonical_template` pin.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let c = caixa(CaixaKind::Biblioteca);
        layout.verify(&c, &root).expect("template must pass");
    }

    #[test]
    fn etiquetas_violation_on_non_chart_keyword_shape() {
        // Canonical CSV-list-separator-confusion footgun: the author
        // confused the CSV-style separator with the `:etiquetas` list
        // grammar. The shape gate fires past the empty + duplicate
        // arms via [`Caixa::validate_etiquetas`]'s new
        // `is_chart_keyword_shape` cascade, and the layout envelope
        // wraps [`ManifestError::EtiquetaInvalid`]'s Display through
        // verbatim — the issue string names both the offending slot
        // and the offending value (debug-escaped). Peer with the
        // `autores_violation_on_non_chart_maintainer_shape` pin on
        // the sibling universal-axis `Vec<String>` surface — the
        // second layout pin on the Vec<String> per-entry shape
        // cascade.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.etiquetas = vec!["mesh,http,grpc".into()];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::EtiquetasViolation { caixa, issue } = err else {
            panic!("expected LayoutError::EtiquetasViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":etiquetas"),
            "issue must name the offending slot: {issue}",
        );
        assert!(
            issue.contains("mesh,http,grpc"),
            "issue must quote the offending value: {issue}",
        );
    }

    // ── autores universal-axis gate wired into verify ───────────────────
    //
    // Pins the layout-pipeline wire-up of [`Caixa::validate_autores`]:
    // the fifth universal-axis Caixa-level value-shape gate (peer of
    // `validate_nome` / `validate_versao` / `validate_deps` /
    // `validate_etiquetas` / `validate_code_paths`), wired immediately
    // after `validate_etiquetas` so the two Vec-shaped universal
    // metadata axes sit adjacent in the cascade. Until this wire-up
    // landed `:autores` had no shape gate at any layer — the
    // maintainer-axis was the second largest universal authoring
    // surface on the typed Caixa surface with no validate discipline,
    // and unlike `:etiquetas` (caixa-helm dedups the rendered
    // `keywords:` array via `BTreeSet` collect at chart render),
    // `maintainers:` has *no* renderer-side dedup, so duplicate
    // `:autores` entries render verbatim as two identical
    // `Maintainer { name, email: None }` records — a strictly worse
    // footgun than the peer `:etiquetas` shape.

    #[test]
    fn autores_violation_on_empty_entry() {
        // Canonical paste-from-blank-doc footgun on every kind. The
        // wrap envelope wraps [`ManifestError::AutorEmpty`]'s Display
        // through verbatim, so the issue string names the offending
        // `:autores` axis at the source — the author can grep their
        // caixa.lisp for `:autores` and fix the empty entry in one
        // edit. Mirrors the peer `etiquetas_violation_on_empty_entry`
        // shape (360a499) on the `:etiquetas` axis.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.autores = vec![String::new()];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::AutoresViolation { caixa, issue } = err else {
            panic!("expected LayoutError::AutoresViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":autores"),
            "issue must name the offending slot: {issue}",
        );
    }

    #[test]
    fn autores_violation_on_duplicate_entry() {
        // Canonical copy-paste-the-wrong-author footgun. Unlike the
        // peer `:etiquetas` axis (silently dedup'd by caixa-helm's
        // `BTreeSet` collect at chart render), `:autores` duplicates
        // stack verbatim in the rendered `maintainers:` — the gate
        // closes the footgun at validate time before any renderer
        // sees it. The wrap envelope names the offending author
        // verbatim through the inner [`ManifestError::AutorDuplicate`]'s
        // Display.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.autores = vec!["pleme-io".into(), "pleme-io".into()];
        // The servicos path doesn't exist in this fixture, but the
        // `:autores` gate fires before the existence loop (universal
        // axis dominates kind-specific existence checks). Wire is
        // intact iff the wrap envelope surfaces first.
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::AutoresViolation { caixa, issue } = err else {
            panic!("expected LayoutError::AutoresViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("pleme-io"),
            "issue must quote the offending author: {issue}",
        );
    }

    #[test]
    fn autores_violation_fires_before_kind_coherence_mesh_slot() {
        // Cross-axis precedence pin: a Biblioteca with malformed
        // `:autores` *and* declared mesh slots (`:membros`) surfaces
        // the universal `:autores` diagnostic first, not the
        // kind-coherence `MeshSlotsOnNonAplicacao` diagnostic.
        // `:autores` is universal (every kind owns the slot), so its
        // shape diagnostic is more fundamental than the partition-on-
        // kind diagnostic. Mirrors the peer
        // `etiquetas_violation_fires_before_kind_coherence_mesh_slot`
        // pin (360a499) on the `:etiquetas` axis vs the same kind-
        // coherence gates.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.autores = vec![String::new()];
        c.membros = vec![crate::aplicacao::Membro {
            caixa: "x".into(),
            versao: "^0.1".into(),
        }];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::AutoresViolation { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn autores_violation_fires_after_etiquetas_violation() {
        // Cross-axis precedence pin (inside the Vec-shaped universal
        // metadata pair): a caixa with both a malformed `:etiquetas`
        // entry *and* a malformed `:autores` entry surfaces
        // `EtiquetasViolation` first — `:etiquetas` is the fourth
        // universal axis in the cascade and runs before `:autores`,
        // peer with the canonical identity-axis-first cascade the
        // peer gates establish. Mirrors the peer
        // `etiquetas_violation_fires_after_deps_violation` precedence
        // pin (360a499) on the dep-axis-before-tag-axis pair.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.etiquetas = vec![String::new()];
        c.autores = vec![String::new()];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::EtiquetasViolation { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn autores_violation_accepts_canonical_template() {
        // Positive control sanity pin: the canonical `Caixa::template`
        // shape (`:autores ()` — empty list) passes the gate trivially.
        // Mirrors the peer `etiquetas_violation_accepts_canonical_template`
        // pin (360a499).
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let c = caixa(CaixaKind::Biblioteca);
        layout.verify(&c, &root).expect("template must pass");
    }

    #[test]
    fn autores_violation_on_non_chart_maintainer_shape() {
        // Canonical paste-from-multiline-doc footgun: the author
        // pasted a multi-line block of author records into one
        // `:autores` entry instead of splitting into one entry per
        // author. The shape gate fires past the empty + duplicate arms
        // via [`Caixa::validate_autores`]'s new
        // `is_chart_maintainer_name_shape` cascade, and the layout
        // envelope wraps [`ManifestError::AutorInvalid`]'s Display
        // through verbatim — the issue string names both the offending
        // slot and the offending value (debug-escaped). Peer with the
        // `descricao_violation_on_non_chart_shape` pin on the sibling
        // universal-axis `Option<String>` surface and the
        // `licenca_violation_on_non_spdx_shape` /
        // `edicao_violation_on_non_year_shape` peers — and the first
        // layout pin on the Vec<String> per-entry shape cascade.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.autores = vec!["alice\nbob".into()];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::AutoresViolation { caixa, issue } = err else {
            panic!("expected LayoutError::AutoresViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":autores"),
            "issue must name the offending slot: {issue}",
        );
        assert!(
            issue.contains("alice\\nbob"),
            "issue must quote the offending value (debug-escaped): {issue}",
        );
    }

    // ── repositorio universal-axis gate wired into verify ────────────────
    //
    // Pins the layout-pipeline wire-up of [`Caixa::validate_repositorio`]:
    // the sixth universal-axis Caixa-level value-shape gate (peer of
    // `validate_nome` / `validate_versao` / `validate_deps` /
    // `validate_etiquetas` / `validate_autores` / `validate_code_paths`),
    // wired immediately after `validate_autores` so the universal
    // git-URL axis sits adjacent to the two Vec-shaped universal
    // metadata axes (`:etiquetas`, `:autores`) in the cascade. Until
    // this wire-up landed `:repositorio` had no shape gate at any
    // layer — the universal git-shaped homepage axis was the third
    // largest universal authoring surface on the typed Caixa with no
    // validate discipline, routing the same string through two
    // load-bearing substrate consumers (`caixa-helm`'s `Chart.yaml
    // home:` field and `caixa-flux`'s FluxCD `GitRepository.spec.url`)
    // via `Option::unwrap_or_else` fallbacks that only fire on `None` —
    // a `Some("")` silently passed every fallback and rendered as an
    // empty URL in both consumers, breaking at `helm template` /
    // FluxCD reconcile time far from the source `caixa.lisp`. The
    // gate closes the divergence and makes the two `git URL`-shaped
    // surfaces on the typed Caixa (`:repositorio` here, `:deps :fonte
    // :repo` peer routed through the same shared
    // `crate::render::is_git_repo_url` predicate) structurally
    // equivalent by construction.

    #[test]
    fn repositorio_violation_on_empty_some() {
        // Canonical paste-from-blank-doc footgun on every kind. The
        // wrap envelope wraps [`ManifestError::RepositorioEmpty`]'s
        // Display through verbatim, so the issue string names the
        // offending `:repositorio` axis at the source — the author
        // can grep their caixa.lisp for `:repositorio ""` and fix the
        // empty value in one edit. Mirrors the peer
        // `autores_violation_on_empty_entry` shape (86c769b) on the
        // `:autores` axis.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.repositorio = Some(String::new());
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::RepositorioViolation { caixa, issue } = err else {
            panic!("expected LayoutError::RepositorioViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":repositorio"),
            "issue must name the offending slot: {issue}",
        );
    }

    #[test]
    fn repositorio_violation_on_malformed_shape() {
        // Canonical CLI-argument-injection footgun: a leading `-`
        // value (`-upload-pack=evil`) escapes the `git clone <repo>`
        // subprocess argument boundary at clone time. The shared
        // `is_git_repo_url` predicate — the same parser the peer
        // `:deps :fonte :repo` axis routes through via
        // `DepSource::validate` — refuses every leading-`-` shape at
        // validate time. The wrap envelope names the offending value
        // verbatim through the inner [`ManifestError::RepositorioInvalid`]'s
        // Display.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.repositorio = Some("-upload-pack=evil".into());
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::RepositorioViolation { caixa, issue } = err else {
            panic!("expected LayoutError::RepositorioViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("-upload-pack=evil"),
            "issue must quote the offending value: {issue}",
        );
    }

    #[test]
    fn repositorio_violation_fires_before_kind_coherence_mesh_slot() {
        // Cross-axis precedence pin: a Biblioteca with malformed
        // `:repositorio` *and* declared mesh slots (`:membros`)
        // surfaces the universal `:repositorio` diagnostic first, not
        // the kind-coherence `MeshSlotsOnNonAplicacao` diagnostic.
        // `:repositorio` is universal (every kind owns the slot), so
        // its shape diagnostic is more fundamental than the
        // partition-on-kind diagnostic. Mirrors the peer
        // `autores_violation_fires_before_kind_coherence_mesh_slot`
        // pin (86c769b) on the `:autores` axis vs the same
        // kind-coherence gates.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.repositorio = Some(String::new());
        c.membros = vec![crate::aplicacao::Membro {
            caixa: "x".into(),
            versao: "^0.1".into(),
        }];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::RepositorioViolation { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn repositorio_violation_fires_after_autores_violation() {
        // Cross-axis precedence pin (inside the universal metadata
        // trio): a caixa with both a malformed `:autores` entry *and*
        // a malformed `:repositorio` value surfaces `AutoresViolation`
        // first — `:autores` is the fifth universal axis in the
        // cascade and runs before `:repositorio`, peer with the
        // canonical identity-axis-first cascade the peer gates
        // establish. Mirrors the peer
        // `autores_violation_fires_after_etiquetas_violation`
        // precedence pin (86c769b) on the tag-axis-before-author-axis
        // pair.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.autores = vec![String::new()];
        c.repositorio = Some(String::new());
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::AutoresViolation { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn repositorio_violation_accepts_canonical_template() {
        // Positive control sanity pin: the canonical `Caixa::template`
        // shape (omits `:repositorio` entirely → `None` on the typed
        // surface) passes the gate trivially — the gate is a no-op
        // when the author didn't author a value. Mirrors the peer
        // `autores_violation_accepts_canonical_template` pin (86c769b).
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let c = caixa(CaixaKind::Biblioteca);
        layout.verify(&c, &root).expect("template must pass");
    }

    #[test]
    fn repositorio_violation_accepts_canonical_github_shorthand() {
        // Positive control pin on the canonical pleme-io `:repositorio`
        // shape: the `github:org/repo` shorthand the README quickstart
        // and the `caixa-helm` / `caixa-mesh` / `caixa-flux` fixtures
        // all use passes the gate end-to-end. Closes the structural
        // equivalence between this surface and the peer `:deps :fonte
        // :repo` axis — both consume `crate::render::is_git_repo_url`
        // and both must agree on the same accepted shape set.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.repositorio = Some("github:pleme-io/hello-rio".into());
        layout.verify(&c, &root).expect("canonical shape must pass");
    }

    // ── descricao universal-axis gate wired into verify ──────────────────
    //
    // Pins the layout-pipeline wire-up of [`Caixa::validate_descricao`]:
    // the seventh universal-axis Caixa-level value-shape gate (peer of
    // `validate_nome` / `validate_versao` / `validate_deps` /
    // `validate_etiquetas` / `validate_autores` / `validate_repositorio` /
    // `validate_code_paths`), wired immediately after `validate_repositorio`
    // so the universal free-form-prose axis sits adjacent to the
    // universal git-URL axis in the cascade. Until this wire-up landed
    // `:descricao` had no shape gate at any layer — the empty
    // `Some("")` silently passed both `caixa-helm` consumers'
    // `Option::unwrap_or_else(|| <fallback>)` (which only fire on
    // `None`) and rendered as `Chart.yaml description: ""` plus a
    // blank `README.md` header, breaking at `helm lint` time
    // (`WARNING [chart.metadata.description]: description is required`
    // on `apiVersion: v2` charts) far from the source `caixa.lisp`.
    // Closes the same `Some("")` skips-`unwrap_or_else` footgun the
    // peer `:repositorio` gate (577b0a9) closed, on the universal
    // free-form-prose summary axis.

    #[test]
    fn descricao_violation_on_empty_some() {
        // Canonical paste-from-blank-doc footgun on every kind. The
        // wrap envelope wraps [`ManifestError::DescricaoEmpty`]'s
        // Display through verbatim, so the issue string names the
        // offending `:descricao` axis at the source — the author can
        // grep their caixa.lisp for `:descricao ""` and fix the empty
        // value in one edit. Mirrors the peer
        // `repositorio_violation_on_empty_some` shape (577b0a9) on
        // the sibling `Option<String>` `:repositorio` axis.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.descricao = Some(String::new());
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::DescricaoViolation { caixa, issue } = err else {
            panic!("expected LayoutError::DescricaoViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":descricao"),
            "issue must name the offending slot: {issue}",
        );
    }

    #[test]
    fn descricao_violation_fires_before_kind_coherence_mesh_slot() {
        // Cross-axis precedence pin: a Biblioteca with empty
        // `:descricao` *and* declared mesh slots (`:membros`)
        // surfaces the universal `:descricao` diagnostic first, not
        // the kind-coherence `MeshSlotsOnNonAplicacao` diagnostic.
        // `:descricao` is universal (every kind owns the slot), so
        // its shape diagnostic is more fundamental than the
        // partition-on-kind diagnostic. Mirrors the peer
        // `repositorio_violation_fires_before_kind_coherence_mesh_slot`
        // pin (577b0a9) on the `:repositorio` axis vs the same
        // kind-coherence gates.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.descricao = Some(String::new());
        c.membros = vec![crate::aplicacao::Membro {
            caixa: "x".into(),
            versao: "^0.1".into(),
        }];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::DescricaoViolation { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn descricao_violation_fires_after_repositorio_violation() {
        // Cross-axis precedence pin (inside the universal metadata
        // cascade): a caixa with both a malformed `:repositorio` *and*
        // an empty `:descricao` surfaces `RepositorioViolation`
        // first — `:repositorio` is the sixth universal axis in the
        // cascade and runs before `:descricao`, peer with the
        // canonical identity-axis-first cascade the peer gates
        // establish. Mirrors the peer
        // `repositorio_violation_fires_after_autores_violation`
        // precedence pin (577b0a9) on the autores-axis-before-
        // repositorio-axis pair.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.repositorio = Some(String::new());
        c.descricao = Some(String::new());
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::RepositorioViolation { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn descricao_violation_accepts_none() {
        // Positive control sanity pin: a caixa that omits
        // `:descricao` entirely (the canonical `Caixa::template` shape
        // carries `Some("FIXME — describe this caixa")`, but the
        // layout-test fixture defaults to `None`) passes the gate
        // trivially — the gate is a no-op when the author didn't
        // author a value. Mirrors the peer
        // `repositorio_violation_accepts_canonical_template` pin
        // (577b0a9).
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let c = caixa(CaixaKind::Biblioteca);
        layout.verify(&c, &root).expect("None must pass");
    }

    #[test]
    fn descricao_violation_accepts_canonical_summary() {
        // Positive control pin on the canonical pleme-io `:descricao`
        // shape: a short free-form prose summary the `caixa-helm` /
        // `caixa-flux` / `caixa-mesh` fixtures all carry passes the
        // gate end-to-end.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.descricao = Some("Canonical Rust→wasm32-wasip2 caixa Servico.".into());
        layout
            .verify(&c, &root)
            .expect("canonical summary must pass");
    }

    #[test]
    fn descricao_violation_on_non_chart_shape() {
        // Shape-predicate wire-up pin: a malformed `:descricao` value
        // that's a non-empty `Some(s)` but carries a paste-from-
        // multiline-doc embedded newline surfaces the
        // `DescricaoViolation` envelope via the manifest-layer
        // `ManifestError::DescricaoInvalid` arm. Mirrors the peer
        // `descricao_violation_on_empty_some` shape on the empty arm
        // of the same axis and the peer
        // `licenca_violation_on_non_spdx_shape` shape on the sibling
        // `:licenca` axis. Until this gate landed a value like
        // `"Checkout\nflow."` (an embedded newline) or `"Checkout
        // flow. "` (a trailing whitespace) silently passed
        // `StandardLayout::verify` and landed in the rendered
        // Chart.yaml `description:` field as a YAML-illegal
        // multi-line scalar or a silently-trimmed whitespace
        // round-trip far from the source caixa.lisp.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.descricao = Some("Checkout\nflow.".into());
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::DescricaoViolation { caixa, issue } = err else {
            panic!("expected LayoutError::DescricaoViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":descricao"),
            "issue must name the offending slot: {issue}",
        );
        // The wrapped `ManifestError::DescricaoInvalid` Display uses
        // `{descricao:?}` (Debug) so the embedded newline surfaces
        // debug-escaped as `\n` in the issue string.
        assert!(
            issue.contains("Checkout\\nflow."),
            "issue must quote the offending value (debug-escaped): {issue}",
        );
    }

    // ── :licenca empty-Some shape wired into verify (universal axis) ──
    //
    // Until this wire-up landed `Caixa::validate_licenca` did not
    // exist — the universal SPDX-shaped license-expression axis had
    // no shape gate at any layer, so an empty `Some("")` silently
    // passed `Caixa::from_lisp` and `StandardLayout::verify` and
    // landed as a bare trailing period in the rendered
    // `lareira-<nome>` chart's `README.md` `## License` section via
    // the `caixa-helm` consumer's `caixa.licenca.clone().unwrap_or_else(||
    // "MIT".into())` (which only fires on `None`) at
    // `caixa-helm/src/lib.rs:361`. Closes the same `Some("")`
    // skips-`unwrap_or_else` footgun the peer `:repositorio`
    // (577b0a9) and `:descricao` (4e6db38) gates closed, on the
    // universal license-expression axis.

    #[test]
    fn licenca_violation_on_empty_some() {
        // Canonical paste-from-blank-doc footgun on every kind. The
        // wrap envelope wraps [`ManifestError::LicencaEmpty`]'s
        // Display through verbatim, so the issue string names the
        // offending `:licenca` axis at the source — the author can
        // grep their caixa.lisp for `:licenca ""` and fix the empty
        // value in one edit. Mirrors the peer
        // `descricao_violation_on_empty_some` shape (4e6db38) on
        // the sibling `Option<String>` `:licenca` axis.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.licenca = Some(String::new());
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::LicencaViolation { caixa, issue } = err else {
            panic!("expected LayoutError::LicencaViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":licenca"),
            "issue must name the offending slot: {issue}",
        );
    }

    #[test]
    fn licenca_violation_fires_before_kind_coherence_mesh_slot() {
        // Cross-axis precedence pin: a Biblioteca with empty
        // `:licenca` *and* declared mesh slots (`:membros`)
        // surfaces the universal `:licenca` diagnostic first, not
        // the kind-coherence `MeshSlotsOnNonAplicacao` diagnostic.
        // `:licenca` is universal (every kind owns the slot), so
        // its shape diagnostic is more fundamental than the
        // partition-on-kind diagnostic. Mirrors the peer
        // `descricao_violation_fires_before_kind_coherence_mesh_slot`
        // pin (4e6db38) on the `:descricao` axis vs the same
        // kind-coherence gates.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.licenca = Some(String::new());
        c.membros = vec![crate::aplicacao::Membro {
            caixa: "x".into(),
            versao: "^0.1".into(),
        }];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::LicencaViolation { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn licenca_violation_fires_after_descricao_violation() {
        // Cross-axis precedence pin (inside the universal metadata
        // cascade): a caixa with both an empty `:descricao` *and*
        // an empty `:licenca` surfaces `DescricaoViolation`
        // first — `:descricao` is the seventh universal axis in the
        // cascade and runs before `:licenca`, peer with the
        // canonical identity-axis-first cascade the peer gates
        // establish. Mirrors the peer
        // `descricao_violation_fires_after_repositorio_violation`
        // precedence pin (4e6db38) on the repositorio-axis-before-
        // descricao-axis pair.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.descricao = Some(String::new());
        c.licenca = Some(String::new());
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::DescricaoViolation { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn licenca_violation_accepts_none() {
        // Positive control sanity pin: a caixa that omits `:licenca`
        // entirely (the layout-test fixture defaults to `None`)
        // passes the gate trivially — the gate is a no-op when the
        // author didn't author a value. Mirrors the peer
        // `descricao_violation_accepts_none` pin (4e6db38).
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let c = caixa(CaixaKind::Biblioteca);
        layout.verify(&c, &root).expect("None must pass");
    }

    #[test]
    fn licenca_violation_accepts_canonical_expression() {
        // Positive control pin on the canonical pleme-io `:licenca`
        // shape: a non-empty SPDX expression the `caixa-helm` /
        // `caixa-flux` / `caixa-mesh` fixtures all carry (`"MIT"`)
        // passes the gate end-to-end.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.licenca = Some("Apache-2.0 OR MIT".into());
        layout
            .verify(&c, &root)
            .expect("canonical SPDX expression must pass");
    }

    #[test]
    fn licenca_violation_on_non_spdx_shape() {
        // Shape-predicate wire-up pin: a malformed `:licenca` value
        // that's a non-empty `Some(s)` but falls outside the SPDX
        // expression alphabet floor surfaces the `LicencaViolation`
        // envelope via the manifest-layer `ManifestError::LicencaInvalid`
        // arm. Mirrors the peer `licenca_violation_on_empty_some`
        // shape on the empty arm of the same axis and the peer
        // `edicao_violation_on_non_year_shape` shape on the sibling
        // `:edicao` axis. Until this gate landed a value like
        // `"Apache_2.0"` (an underscore-instead-of-hyphen typo) or
        // `"MIT, Apache-2.0"` (a comma-instead-of-`OR`-keyword
        // colloquial idiom) silently passed `StandardLayout::verify`
        // and landed in the rendered chart `README.md` `## License`
        // section + a future SPDX-aware Chart.yaml `license:`
        // emitter would refuse the value at `helm lint` time far
        // from the source caixa.lisp.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.licenca = Some("Apache_2.0".into());
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::LicencaViolation { caixa, issue } = err else {
            panic!("expected LayoutError::LicencaViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":licenca"),
            "issue must name the offending slot: {issue}",
        );
        assert!(
            issue.contains("Apache_2.0"),
            "issue must quote the offending value: {issue}",
        );
    }

    // ── :edicao empty-Some shape wired into verify (universal axis) ──
    //
    // Until this wire-up landed `Caixa::validate_edicao` did not
    // exist — the universal language-edition axis had no shape gate
    // at any layer, so an empty `Some("")` silently passed
    // `Caixa::from_lisp` and `StandardLayout::verify` and landed as a
    // bare `(:edicao "")` line in the rendered caixa.lisp, ready for
    // a future renderer-side consumer's `Option::unwrap_or_else`
    // (which only fires on `None`) to skip its fallback. Closes the
    // same `Some("")`-skips-`unwrap_or_else` footgun the peer
    // `:repositorio` (577b0a9), `:descricao` (4e6db38), and
    // `:licenca` (3d1e535) gates closed, on the universal language-
    // edition axis — the last un-gated universal-axis
    // `Option<String>` Caixa-level value-shape surface.

    #[test]
    fn edicao_violation_on_empty_some() {
        // Canonical paste-from-blank-doc footgun on every kind. The
        // wrap envelope wraps [`ManifestError::EdicaoEmpty`]'s
        // Display through verbatim, so the issue string names the
        // offending `:edicao` axis at the source — the author can
        // grep their caixa.lisp for `:edicao ""` and fix the empty
        // value in one edit. Mirrors the peer
        // `licenca_violation_on_empty_some` shape (3d1e535) on the
        // sibling `Option<String>` `:edicao` axis.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.edicao = Some(String::new());
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::EdicaoViolation { caixa, issue } = err else {
            panic!("expected LayoutError::EdicaoViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":edicao"),
            "issue must name the offending slot: {issue}",
        );
    }

    #[test]
    fn edicao_violation_fires_before_kind_coherence_mesh_slot() {
        // Cross-axis precedence pin: a Biblioteca with empty
        // `:edicao` *and* declared mesh slots (`:membros`) surfaces
        // the universal `:edicao` diagnostic first, not the
        // kind-coherence `MeshSlotsOnNonAplicacao` diagnostic.
        // `:edicao` is universal (every kind owns the slot), so
        // its shape diagnostic is more fundamental than the
        // partition-on-kind diagnostic. Mirrors the peer
        // `licenca_violation_fires_before_kind_coherence_mesh_slot`
        // pin (3d1e535) on the `:licenca` axis vs the same
        // kind-coherence gates.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.edicao = Some(String::new());
        c.membros = vec![crate::aplicacao::Membro {
            caixa: "x".into(),
            versao: "^0.1".into(),
        }];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::EdicaoViolation { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn edicao_violation_fires_after_licenca_violation() {
        // Cross-axis precedence pin (inside the universal metadata
        // cascade): a caixa with both an empty `:licenca` *and* an
        // empty `:edicao` surfaces `LicencaViolation` first —
        // `:licenca` is the eighth universal axis in the cascade
        // and runs before `:edicao`, peer with the canonical
        // identity-axis-first cascade the peer gates establish.
        // Mirrors the peer
        // `licenca_violation_fires_after_descricao_violation`
        // precedence pin (3d1e535) on the descricao-axis-before-
        // licenca-axis pair.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.licenca = Some(String::new());
        c.edicao = Some(String::new());
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::LicencaViolation { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn edicao_violation_accepts_none() {
        // Positive control sanity pin: a caixa that omits `:edicao`
        // entirely (the layout-test fixture defaults to `None`)
        // passes the gate trivially — the gate is a no-op when the
        // author didn't author a value. Mirrors the peer
        // `licenca_violation_accepts_none` pin (3d1e535).
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let c = caixa(CaixaKind::Biblioteca);
        layout.verify(&c, &root).expect("None must pass");
    }

    #[test]
    fn edicao_violation_accepts_canonical_value() {
        // Positive control pin on the canonical pleme-io `:edicao`
        // shape: the `"2026"` edition every `caixa-helm` /
        // `caixa-flux` / `caixa-mesh` / `caixa-core/src/render.rs`
        // fixture carries by construction passes the gate end-to-end.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.edicao = Some("2026".into());
        layout
            .verify(&c, &root)
            .expect("canonical edition must pass");
    }

    #[test]
    fn edicao_violation_on_non_year_shape() {
        // Shape-predicate wire-up pin: a malformed `:edicao` value
        // that's a non-empty `Some(s)` but not a 4-digit ASCII
        // decimal year surfaces the `EdicaoViolation` envelope via
        // the manifest-layer `ManifestError::EdicaoInvalid` arm.
        // Mirrors the peer `edicao_violation_on_empty_some` shape
        // on the empty arm of the same axis. Until this gate landed
        // a value like `"v2026"` (a familiar git-tag idiom that
        // doesn't apply to the year-shaped edition axis) silently
        // passed `StandardLayout::verify` and broke at the
        // substrate's build-time edition selector far from the
        // source caixa.lisp.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.edicao = Some("v2026".into());
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::EdicaoViolation { caixa, issue } = err else {
            panic!("expected LayoutError::EdicaoViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":edicao"),
            "issue must name the offending slot: {issue}",
        );
        assert!(
            issue.contains("v2026"),
            "issue must quote the offending value: {issue}",
        );
    }

    // ── Caixa-identity gates (`:nome`, `:versao`) wired into verify ────
    //
    // Until this wire-up landed `Caixa::validate_nome` and
    // `Caixa::validate_versao` lived as `pub fn` on `Caixa` with full
    // per-arm unit coverage in `manifest::tests`, but no production
    // path called them — `feira build` silently accepted malformed
    // `:nome` / `:versao` and the failure surfaced at `helm install` /
    // `kubectl apply` / `feira publish` / lacre-resolve / `:upgrade-from
    // :from` matching time, far from the source `caixa.lisp`. The
    // following pins fence the layout-pipeline wire-up: every layout
    // verify on a structurally-invalid Caixa identity axis surfaces
    // the per-axis `*Violation { caixa, issue }` envelope before any
    // kind-coherence, code-path, or downstream gate sees it.

    #[test]
    fn nome_violation_on_uppercase() {
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.nome = "MyApp".into();
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::NomeViolation { caixa, issue } = err else {
            panic!("expected LayoutError::NomeViolation, got {err:?}");
        };
        assert_eq!(caixa, "MyApp");
        assert!(
            issue.contains("MyApp"),
            "issue must quote the offending nome: {issue}",
        );
    }

    #[test]
    fn nome_violation_on_underscore() {
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.nome = "my_app".into();
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::NomeViolation { ref caixa, .. } if caixa == "my_app"),
            "got {err:?}",
        );
    }

    #[test]
    fn nome_violation_on_empty() {
        // Empty `:nome` surfaces NomeViolation wrapping the narrower
        // `ManifestError::NomeEmpty` arm — the empty-first cascade the
        // peer per-axis name gates already use.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.nome = String::new();
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::NomeViolation { caixa, issue } = err else {
            panic!("expected LayoutError::NomeViolation, got {err:?}");
        };
        assert!(caixa.is_empty());
        assert!(
            issue.contains(":nome is empty"),
            "issue must surface the empty-arm diagnostic: {issue}",
        );
    }

    #[test]
    fn versao_violation_on_missing_patch() {
        // `"0.1"` — the canonical "I shortened it" footgun. Helm /
        // OCI / lacre-resolve / `:upgrade-from :from` all strict-parse
        // through `semver::Version::parse`, which refuses a two-part
        // shape.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.versao = "0.1".into();
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::VersaoViolation { caixa, issue } = err else {
            panic!("expected LayoutError::VersaoViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("0.1"),
            "issue must quote the offending versao: {issue}",
        );
    }

    #[test]
    fn versao_violation_on_git_tag_shape() {
        // `"v0.1.0"` — the git-tag-shape-leaking-into-versao typo.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.versao = "v0.1.0".into();
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::VersaoViolation { ref issue, .. }
                if issue.contains("v0.1.0")),
            "got {err:?}",
        );
    }

    #[test]
    fn versao_violation_on_empty() {
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.versao = String::new();
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::VersaoViolation { caixa, issue } = err else {
            panic!("expected LayoutError::VersaoViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":versao is empty"),
            "issue must surface the empty-arm diagnostic: {issue}",
        );
    }

    #[test]
    fn nome_violation_fires_before_versao_violation() {
        // Precedence pin: when both `:nome` and `:versao` are malformed,
        // `:nome` surfaces first — the canonical declaration-order
        // precedence the `ManifestError` family establishes, the same
        // grep-order the author follows when fixing in `caixa.lisp`.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.nome = "MyApp".into();
        c.versao = "0.1".into();
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::NomeViolation { .. }),
            "got {err:?} — nome must fire before versao",
        );
    }

    #[test]
    fn nome_violation_fires_before_kind_coherence() {
        // Precedence pin: a Biblioteca caixa with a malformed `:nome`
        // AND a declared mesh slot surfaces NomeViolation, not
        // MeshSlotsOnNonAplicacao — the identity-axis gate is more
        // fundamental than the kind-coherence gate (which carries
        // `caixa.nome` verbatim in its diagnostic, and so depends on the
        // name being structurally valid to render a useful message).
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.nome = "MyApp".into();
        c.membros = vec![crate::aplicacao::Membro {
            caixa: "x".into(),
            versao: "^0.1".into(),
        }];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::NomeViolation { .. }),
            "got {err:?} — nome must fire before MeshSlotsOnNonAplicacao",
        );
    }

    #[test]
    fn nome_violation_fires_before_owncode() {
        // Precedence pin: a Supervisor with a malformed `:nome` AND
        // declared `:bibliotecas` surfaces NomeViolation, not
        // SupervisorOwnsCode — same rationale as above.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Supervisor);
        c.nome = "MyApp".into();
        c.bibliotecas = vec!["lib/x.lisp".into()];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::NomeViolation { .. }),
            "got {err:?} — nome must fire before SupervisorOwnsCode",
        );
    }

    #[test]
    fn versao_violation_fires_before_kind_coherence() {
        // Precedence pin: a Biblioteca with a valid `:nome` but a
        // malformed `:versao` AND a declared servico slot surfaces
        // VersaoViolation before ServicoSlotsOnNonServico.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.versao = "v0.1.0".into();
        c.limits = Some(crate::LimitsSpec {
            memory: Some(64 * 1024 * 1024),
            ..Default::default()
        });
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::VersaoViolation { .. }),
            "got {err:?} — versao must fire before ServicoSlotsOnNonServico",
        );
    }

    #[test]
    fn nome_violation_fires_before_missing_lib() {
        // Precedence pin: a Biblioteca with a malformed `:nome` and no
        // lib entry surfaces NomeViolation, not MissingLib — the
        // identity-axis gate is more fundamental than the layout's
        // `lib/<nome>.lisp` default-path check (which derives the
        // expected path from `:nome` itself, so would surface a
        // misleading "expected lib/MyApp.lisp" diagnostic against an
        // unrecoverable name).
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.nome = "MyApp".into();
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::NomeViolation { .. }),
            "got {err:?} — nome must fire before MissingLib",
        );
    }

    #[test]
    fn nome_versao_violations_fire_after_missing_manifest() {
        // Precedence pin: `MissingManifest` still dominates — there's
        // no caixa to identity-check when the manifest is missing.
        let root = PathBuf::from("/tmp/x");
        let layout = StandardLayout::new().with_path_exists(|_| false);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.nome = "MyApp".into();
        c.versao = "0.1".into();
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::MissingManifest(_)),
            "got {err:?} — MissingManifest must dominate identity gates",
        );
    }

    #[test]
    fn valid_nome_versao_passes_to_downstream_gates() {
        // Sanity pin: the canonical "demo" / "0.1.0" identity passes
        // both axes; downstream gates (MissingLib here) take over.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let err = layout
            .verify(&caixa(CaixaKind::Biblioteca), &root)
            .unwrap_err();
        assert!(
            matches!(err, LayoutError::MissingLib { .. }),
            "got {err:?} — valid identity must pass to MissingLib",
        );
    }

    // ── :deps / :deps-dev shape gate (lifted to layout-level verify) ─────
    //
    // Until this wire-up landed `Caixa::validate_deps` lived as `pub fn`
    // on `Caixa` with full per-arm unit coverage in `manifest::tests` +
    // `dep::tests` but no production path called it — `feira build`
    // silently accepted a malformed `:deps` / `:deps-dev` entry and the
    // failure surfaced at lacre-resolve / `git clone` / `cargo metadata`
    // / `helm install` time on the *first* downstream consumer to
    // strict-parse the value, far from the source `caixa.lisp` and
    // without any field naming the offending `:deps` axis. The following
    // pins fence the layout-pipeline wire-up: every layout verify on a
    // structurally-invalid `:deps` value-shape surfaces the per-axis
    // `DepsViolation { caixa, issue }` envelope (peer of
    // `NomeViolation` / `VersaoViolation` / `CodePathViolation` /
    // `LimitsViolation` / `BehaviorViolation` / `UpgradeViolation` /
    // `SupervisorViolation` / `AplicacaoViolation`) before any kind-
    // coherence, code-path, or downstream gate sees it.

    #[test]
    fn deps_violation_on_empty_dep_nome() {
        // Empty `:nome` on a `:deps` entry surfaces the narrower
        // `DepError::NomeEmpty` arm through the wrap envelope.
        use crate::Dep;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.deps = vec![Dep::simple("", "^0.1")];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::DepsViolation { caixa, issue } = err else {
            panic!("expected LayoutError::DepsViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":deps") && issue.contains(":nome"),
            "issue must name the offending slot + axis: {issue}",
        );
    }

    #[test]
    fn deps_violation_on_uppercase_dep_nome() {
        // Uppercase `:nome` on a `:deps` entry surfaces
        // `DepError::NomeInvalid` (DNS-1123 violation) through the wrap.
        use crate::Dep;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.deps = vec![Dep::simple("Caixa-Teia", "^0.1")];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::DepsViolation { caixa, issue } = err else {
            panic!("expected LayoutError::DepsViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("Caixa-Teia"),
            "issue must quote the offending dep nome verbatim: {issue}",
        );
    }

    #[test]
    fn deps_violation_on_unparseable_dep_versao() {
        // Unparseable `:versao` requirement on a `:deps` entry surfaces
        // `DepError::VersaoInvalid` through the wrap — the canonical
        // "the semver::Error reached the resolver, far from the source"
        // footgun closed at author time.
        use crate::Dep;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.deps = vec![Dep::simple("caixa-teia", "not-a-req")];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::DepsViolation { caixa, issue } = err else {
            panic!("expected LayoutError::DepsViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("caixa-teia") && issue.contains("not-a-req"),
            "issue must quote the dep nome + offending versao: {issue}",
        );
    }

    #[test]
    fn deps_violation_on_duplicate_nome_in_deps() {
        // Within-list `:deps :nome` duplicate surfaces
        // `DepError::DuplicateNome { list: ":deps" }` through the wrap.
        use crate::Dep;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.deps = vec![
            Dep::simple("caixa-teia", "^0.1"),
            Dep::simple("caixa-teia", "^0.2"),
        ];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::DepsViolation { caixa, issue } = err else {
            panic!("expected LayoutError::DepsViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("caixa-teia") && issue.contains(":deps"),
            "issue must quote the duplicated nome + list: {issue}",
        );
    }

    #[test]
    fn deps_violation_on_duplicate_nome_in_deps_dev() {
        // Within-list `:deps-dev :nome` duplicate surfaces the same
        // diagnostic on the dev-only axis — neither list is a
        // second-class citizen of the typed surface.
        use crate::Dep;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.deps_dev = vec![
            Dep::simple("caixa-teia", "^0.1"),
            Dep::simple("caixa-teia", "^0.2"),
        ];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::DepsViolation { caixa, issue } = err else {
            panic!("expected LayoutError::DepsViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":deps-dev"),
            "issue must name the offending list: {issue}",
        );
    }

    #[test]
    fn deps_violation_in_deps_fires_before_deps_dev() {
        // Precedence pin: when *both* `:deps` and `:deps-dev` carry a
        // malformed entry, the `:deps` walk fires first — the canonical
        // declaration-order precedence `Caixa::validate_deps` establishes
        // (the same author-grep ordering the typed-graph peers use on
        // every other Vec-shaped surface).
        use crate::Dep;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.deps = vec![Dep::simple("Bad-In-Deps", "^0.1")];
        c.deps_dev = vec![Dep::simple("Bad-In-Deps-Dev", "^0.1")];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::DepsViolation { caixa: _, issue } = err else {
            panic!("expected LayoutError::DepsViolation, got {err:?}");
        };
        assert!(
            issue.contains("Bad-In-Deps") && !issue.contains("Bad-In-Deps-Dev"),
            "issue must name the :deps offender, not :deps-dev: {issue}",
        );
    }

    #[test]
    fn deps_violation_fires_after_versao_violation() {
        // Precedence pin: when both the top-level `:versao` and a `:deps`
        // entry are malformed, the Caixa-identity gate fires first — the
        // canonical declaration order on `Caixa` (`:nome` → `:versao` →
        // ... → `:deps`) and the same identity-axis-dominates-content-
        // axis discipline the peer `validate_nome` / `validate_versao`
        // wire-up established (1f74a5f). A malformed `:versao` would
        // otherwise quote `caixa.nome` against a downstream-shaped
        // diagnostic.
        use crate::Dep;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.versao = "v0.1.0".into();
        c.deps = vec![Dep::simple("Bad-Dep", "^0.1")];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::VersaoViolation { .. }),
            "got {err:?} — versao must fire before DepsViolation",
        );
    }

    #[test]
    fn deps_violation_fires_before_kind_coherence() {
        // Precedence pin: a Supervisor with a malformed `:deps` entry
        // AND declared `:bibliotecas` (the canonical SupervisorOwnsCode
        // shape) surfaces DepsViolation, not SupervisorOwnsCode — the
        // dep surface is universal across all kinds and its shape gate
        // is more fundamental than the kind-coherence partitions on
        // `:bibliotecas` / `:exe` / `:servicos`. The author can fix the
        // dep typo without first being told to move their `:bibliotecas`
        // off a Supervisor.
        use crate::Dep;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Supervisor);
        c.deps = vec![Dep::simple("Bad-Dep", "^0.1")];
        c.bibliotecas = vec!["lib/x.lisp".into()];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::DepsViolation { .. }),
            "got {err:?} — DepsViolation must fire before SupervisorOwnsCode",
        );
    }

    #[test]
    fn deps_violation_fires_after_missing_manifest() {
        // Precedence pin: `MissingManifest` still dominates — there's no
        // caixa to deps-check when the manifest is missing.
        use crate::Dep;
        let root = PathBuf::from("/tmp/x");
        let layout = StandardLayout::new().with_path_exists(|_| false);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.deps = vec![Dep::simple("Bad-Dep", "^0.1")];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::MissingManifest(_)),
            "got {err:?} — MissingManifest must dominate the deps gate",
        );
    }

    #[test]
    fn deps_violation_on_self_dep_in_deps() {
        // Cross-slot self-edge: a caixa whose `:deps` lists its own
        // `:nome` is rejected at the layout wire-up, the diagnostic
        // surfaces through the `DepsViolation` envelope with both the
        // offending list tag (`":deps"`) and the parent's `:nome`
        // verbatim. Until this wire-up landed the self-dep silently
        // passed `feira build` and the resolver's lacre-pipeline
        // closure walk either rejected mid-traversal (infinite
        // recursion detected far from the source caixa.lisp) or, on
        // the unbounded path, recursed until it exhausted its stack.
        // Mirrors the supervision-tree
        // [`supervisor_violation_on_self_supervision`] and the
        // Aplicacao-membership self-edge wire-up tests on the peer
        // typed-name-graph axes.
        use crate::Dep;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.deps = vec![Dep::simple("demo", "^0.1")];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::DepsViolation { caixa, issue } = err else {
            panic!("expected LayoutError::DepsViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":deps") && issue.contains("demo"),
            "issue must name the offending list + parent :nome: {issue}",
        );
    }

    #[test]
    fn deps_violation_on_self_dep_in_deps_dev() {
        // Same cross-slot self-edge gate on the `:deps-dev` axis —
        // neither dep list is a second-class citizen of the typed
        // surface. The diagnostic names `:deps-dev` so the author can
        // grep their caixa.lisp for the offending block directly.
        use crate::Dep;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.deps_dev = vec![Dep::simple("demo", "^0.1")];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::DepsViolation { caixa, issue } = err else {
            panic!("expected LayoutError::DepsViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains(":deps-dev"),
            "issue must name the offending list: {issue}",
        );
    }

    #[test]
    fn self_dep_fires_after_per_entry_dep_shape() {
        // Precedence pin: the per-entry shape gates of
        // [`Caixa::validate_deps`] (DNS-1123 / SemVer / fonte / etc.)
        // fire first on a self-dep entry whose `:nome` is malformed.
        // Same ordering posture every peer cross-slot gate uses
        // (`validate_no_self_supervision` after `SupervisorSpec::validate`,
        // `validate_no_self_membership` after `AplicacaoSpec::validate`).
        // A malformed self-dep `:nome` surfaces the narrower
        // per-entry diagnostic (which already names the parser-side
        // reason) before the self-edge gate sees the entry.
        use crate::Dep;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        // The parent is "demo" (DNS-1123 valid); the dep is "DEMO"
        // (DNS-1123 invalid). The per-entry shape gate fires on the
        // upper-case nome, masking the self-edge gate (and that's the
        // canonical precedence — fix the dep shape first, then the
        // structural self-edge becomes the next live diagnostic).
        c.deps = vec![Dep::simple("DEMO", "^0.1")];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::DepsViolation { caixa: _, issue } = err else {
            panic!("expected LayoutError::DepsViolation, got {err:?}");
        };
        assert!(
            issue.contains("DNS-1123"),
            "issue must be the per-entry shape diagnostic, not the self-edge gate: {issue}",
        );
    }

    #[test]
    fn valid_deps_pass_to_downstream_gates() {
        // Positive control pin: the canonical authoring shape (one
        // `:deps` entry naming a DNS-1123 nome + Cargo-shaped requirement,
        // one `:deps-dev` entry on a distinct nome) passes the dep gate;
        // downstream gates (MissingLib here) take over. Drift here =
        // a future tighten that rejects any canonical shape surfaces as
        // a regression at this layout-level pin.
        use crate::Dep;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.deps = vec![Dep::simple("caixa-teia", "^0.1")];
        c.deps_dev = vec![Dep::simple("caixa-lint", "^0.2")];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::MissingLib { .. }),
            "got {err:?} — valid deps must pass to MissingLib",
        );
    }

    #[test]
    fn code_path_gate_runs_after_foreign_code_slot_gate() {
        // Precedence pin: a Servico that declares `:exe` (foreign code
        // surface) surfaces ForeignCodeSlot, *not* a per-entry path
        // shape diagnostic, even when the `:exe` entry is itself
        // malformed. The kind-coherence gate is the load-bearing
        // diagnostic at this site — once the slot is moved off the
        // wrong kind, the per-entry shape gate becomes the next live
        // diagnostic.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/ok.yaml".into()];
        c.exe = vec!["/etc/foreign".into()];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::ForeignCodeSlot { .. }),
            "expected ForeignCodeSlot (kind-coherence wins over per-entry shape), got {err:?}",
        );
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
            LayoutError::MissingEntry { kind, .. }
                if kind == crate::render::LAYOUT_MISSING_ENTRY_KIND_BEHAVIOR_CALLBACK
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
        use crate::{BehaviorSpec, UpgradeFromEntry, UpgradeInstruction};
        use std::path::PathBuf;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let on_state_change = root.join("lib/migrations.lisp");
        let mut c = caixa(CaixaKind::Servico);
        // `:versao` past the entry's `:from` so the cross-slot
        // precedence gate (`FromNotBeforeVersao`) lets this case
        // through to the path-existence pass under test.
        c.versao = "0.2.0".into();
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        // `:on-state-change` declared so the cross-slot composition
        // gate (`validate_upgrade_from_against_behavior`) lets the
        // `:state-change` entry through to the path-existence pass
        // under test. Without the callback the missing-callback gate
        // would surface first and the path-existence pass wouldn't be
        // exercised.
        c.behavior = Some(BehaviorSpec {
            on_state_change: Some(PathBuf::from("lib/migrations.lisp")),
            ..Default::default()
        });
        // A `:load-module` precedes the `:state-change` so the entry
        // satisfies the within-entry state-change-ordering gate
        // (`StateChangeWithoutPriorLoad`) and the path-existence pass
        // under test is the gate actually exercised. `:load-module`
        // carries no on-disk path, so it adds no existence requirement.
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![
                UpgradeInstruction::LoadModule {
                    module: "demo".into(),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/migrations/v01-to-v02.lisp"),
                },
            ],
        }];
        let manifest_clone = manifest.clone();
        let svc_clone = svc.clone();
        let on_state_change_clone = on_state_change.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| {
            p == manifest_clone || p == svc_clone || p == on_state_change_clone
        });
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(matches!(
            err,
            LayoutError::MissingEntry { kind, .. }
                if kind == crate::render::LAYOUT_MISSING_ENTRY_KIND_UPGRADE_SCRIPT
        ));
    }

    #[test]
    fn layout_missing_entry_kind_m2_consts_pin_canonical_kebab_case_labels() {
        // Byte-identity pin: the two per-M2-slot leaf-kind labels the
        // [`LayoutError::MissingEntry`] `kind: &'static str`
        // discriminator surfaces under (the M2 `:behavior` per-callback
        // on-disk-leaf axis, the M2 `:upgrade-from :instructions`
        // per-`:state-change` script-path on-disk-leaf axis) route
        // through the lifted [`crate::render::LAYOUT_MISSING_ENTRY_KIND_BEHAVIOR_CALLBACK`]
        // / [`crate::render::LAYOUT_MISSING_ENTRY_KIND_UPGRADE_SCRIPT`]
        // consts, so a future rebrand that reaches the const but not
        // the production emit / test probe (or vice versa) surfaces
        // here at build time rather than at runtime as a downstream
        // [`LayoutError::MissingEntry`] `kind: <stale-label>`
        // diagnostic mismatch far from the rename's commit. Mirror of
        // the peer
        // [`crate::aplicacao::tests::contrato_author_key_consts_pin_canonical_kebab_case_labels`]
        // (f50c875) and
        // [`crate::upgrade::tests::upgrade_instruction_kind_consts_pin_canonical_kebab_case_tags`]
        // (56120ef) byte-identity pins on the sibling M3 `:contratos`
        // per-entry endpoint-label + M2 `:upgrade-from :instructions`
        // per-variant kind-tag axes.
        assert_eq!(
            crate::render::LAYOUT_MISSING_ENTRY_KIND_BEHAVIOR_CALLBACK,
            "behavior-callback"
        );
        assert_eq!(
            crate::render::LAYOUT_MISSING_ENTRY_KIND_UPGRADE_SCRIPT,
            "upgrade-script"
        );
    }

    #[test]
    fn layout_missing_entry_kind_m0_consts_pin_canonical_code_slot_labels() {
        // Byte-identity pin: the three per-M0-code-slot leaf-kind
        // labels the [`LayoutError::MissingEntry`] `kind: &'static
        // str` discriminator surfaces under (the `:bibliotecas`
        // per-entry axis, the `:exe` per-entry axis, the `:servicos`
        // per-entry axis) route through the lifted
        // [`crate::render::LAYOUT_MISSING_ENTRY_KIND_BIBLIOTECA`] /
        // [`crate::render::LAYOUT_MISSING_ENTRY_KIND_EXE`] /
        // [`crate::render::LAYOUT_MISSING_ENTRY_KIND_SERVICO`] consts,
        // so a future rebrand that reaches the const but not the
        // production emit (or vice versa) surfaces here at build time
        // rather than at runtime as a downstream
        // [`LayoutError::MissingEntry`] `kind: <stale-label>`
        // diagnostic mismatch far from the rename's commit. Mirror of
        // the peer M2-tier pin
        // [`layout_missing_entry_kind_m2_consts_pin_canonical_kebab_case_labels`]
        // (95c9c4c) on the sibling `:behavior` / `:upgrade-from`
        // per-slot leaf-kind axes.
        assert_eq!(
            crate::render::LAYOUT_MISSING_ENTRY_KIND_BIBLIOTECA,
            "biblioteca"
        );
        assert_eq!(crate::render::LAYOUT_MISSING_ENTRY_KIND_EXE, "exe");
        assert_eq!(crate::render::LAYOUT_MISSING_ENTRY_KIND_SERVICO, "servico");
    }

    #[test]
    fn layout_missing_entry_kind_m0_consts_align_with_caixa_kind_as_str() {
        // Cross-axis byte-identity pin: the two `:kind`-namesake M0
        // leaf-kind labels ([`crate::render::LAYOUT_MISSING_ENTRY_KIND_BIBLIOTECA`]
        // = `"biblioteca"`,
        // [`crate::render::LAYOUT_MISSING_ENTRY_KIND_SERVICO`] =
        // `"servico"`) must equal [`crate::CaixaKind::Biblioteca`] /
        // [`crate::CaixaKind::Servico`]'s
        // [`crate::CaixaKind::as_str`] outputs verbatim — the
        // substrate's canonical human-readable-kind axis and the
        // layout diagnostic's per-slot leaf-kind axis share one
        // vocabulary for these two arms by design (both label the
        // caixa's code-producing shape by its Portuguese-native
        // idiom), so drift between the two lands as a build-time
        // pattern-arm miss here rather than as a runtime diagnostic
        // that reads inconsistently across `feira build`'s
        // per-invocation output.
        //
        // The third M0 arm ([`crate::render::LAYOUT_MISSING_ENTRY_KIND_EXE`]
        // = `"exe"`) is deliberately *distinct* from
        // [`crate::CaixaKind::Binario`]'s [`crate::CaixaKind::as_str`]
        // output (`"binario"`) — the `:exe` code slot names the
        // per-directory leaf-kind at the `exe/` subtree, whereas
        // [`crate::CaixaKind::Binario`] names the caixa's own runtime
        // kind. Two axes, two labels — the inequality assertion here
        // pins the split so a future accidental collapse of the two
        // onto one scalar (a rebrand that reroutes either axis to
        // match the other) trips at build time.
        assert_eq!(
            crate::render::LAYOUT_MISSING_ENTRY_KIND_BIBLIOTECA,
            CaixaKind::Biblioteca.as_str()
        );
        assert_eq!(
            crate::render::LAYOUT_MISSING_ENTRY_KIND_SERVICO,
            CaixaKind::Servico.as_str()
        );
        assert_ne!(
            crate::render::LAYOUT_MISSING_ENTRY_KIND_EXE,
            CaixaKind::Binario.as_str(),
            "`exe` leaf-kind label names the per-directory code-slot \
             axis; `binario` names the caixa-kind axis — the two must \
             not silently collapse onto one scalar"
        );
    }

    #[test]
    fn layout_missing_entry_kind_consts_are_pairwise_distinct() {
        // Distinctness pin: the five [`LayoutError::MissingEntry`]
        // `kind: &'static str` accept-set members
        // ([`crate::render::LAYOUT_MISSING_ENTRY_KIND_BIBLIOTECA`] /
        // [`crate::render::LAYOUT_MISSING_ENTRY_KIND_EXE`] /
        // [`crate::render::LAYOUT_MISSING_ENTRY_KIND_SERVICO`] on the
        // M0 code-slot arms plus
        // [`crate::render::LAYOUT_MISSING_ENTRY_KIND_BEHAVIOR_CALLBACK`]
        // / [`crate::render::LAYOUT_MISSING_ENTRY_KIND_UPGRADE_SCRIPT`]
        // on the M2 slot arms) must be pairwise distinct — an
        // accidental copy-paste flip that reroutes one label's byte-
        // string to also match another silently collapses two
        // per-slot diagnostics onto one, so an operator running
        // `feira build` reads `kind: "biblioteca"` for what should
        // have surfaced as a `:behavior :on-init` script-not-found
        // diagnostic (or vice versa). This pin catches any such
        // flip at build time. Mirror of the peer
        // [`crate::render::tests::m2_limits_key_consts_are_pairwise_distinct`]
        // / peer distinctness pins on other closed-set typed axes.
        let entries: &[(&str, &str)] = &[
            (
                "LAYOUT_MISSING_ENTRY_KIND_BIBLIOTECA",
                crate::render::LAYOUT_MISSING_ENTRY_KIND_BIBLIOTECA,
            ),
            (
                "LAYOUT_MISSING_ENTRY_KIND_EXE",
                crate::render::LAYOUT_MISSING_ENTRY_KIND_EXE,
            ),
            (
                "LAYOUT_MISSING_ENTRY_KIND_SERVICO",
                crate::render::LAYOUT_MISSING_ENTRY_KIND_SERVICO,
            ),
            (
                "LAYOUT_MISSING_ENTRY_KIND_BEHAVIOR_CALLBACK",
                crate::render::LAYOUT_MISSING_ENTRY_KIND_BEHAVIOR_CALLBACK,
            ),
            (
                "LAYOUT_MISSING_ENTRY_KIND_UPGRADE_SCRIPT",
                crate::render::LAYOUT_MISSING_ENTRY_KIND_UPGRADE_SCRIPT,
            ),
        ];
        for (i, (name_a, value_a)) in entries.iter().enumerate() {
            for (name_b, value_b) in entries.iter().skip(i + 1) {
                assert_ne!(
                    value_a, value_b,
                    "LAYOUT_MISSING_ENTRY_KIND_* consts must be \
                     pairwise-distinct byte-strings — {name_a} and \
                     {name_b} both resolve to {value_a:?}"
                );
            }
        }
    }

    #[test]
    fn layout_dir_consts_pin_canonical_directory_names() {
        // Scalar-value pin for the three [`crate::render::LAYOUT_DIR_*`]
        // consts naming the CSE-invariant per-[`CaixaKind`]
        // on-disk-directory-name axes the substrate's layout invariants
        // pin (`lib/` for [`CaixaKind::Biblioteca`], `exe/` for
        // [`CaixaKind::Binario`], `servicos/` for [`CaixaKind::Servico`]).
        // A future rebrand of any of the three on-disk directory landing
        // conventions must reach this pin — the const-edit lands on one
        // arm, the assertion here re-pins the new byte-string, and every
        // downstream consumer (the caixa-feira `init` / `fmt` / `lint` /
        // `tofu` scaffolders, the [`crate::LayoutInvariants::verify`]
        // sandbox reconstruction, the future
        // `feira app deploy`-cluster scaffolder) picks up the new
        // directory name at build time. Mirror of the peer
        // [`layout_missing_entry_kind_m0_consts_pin_canonical_code_slot_labels`]
        // (fe2a898) on the sibling
        // [`crate::LayoutError::MissingEntry`] `kind:` discriminator
        // axis this on-disk-directory axis composes with.
        assert_eq!(crate::render::LAYOUT_DIR_LIB, "lib");
        assert_eq!(crate::render::LAYOUT_DIR_EXE, "exe");
        assert_eq!(crate::render::LAYOUT_DIR_SERVICOS, "servicos");
    }

    #[test]
    fn layout_dir_consts_are_pairwise_distinct() {
        // Distinctness pin: the three per-[`CaixaKind`]
        // on-disk-directory-name arms must resolve to pairwise-distinct
        // byte-strings — a future accidental copy-paste flip that
        // reroutes any one of the three onto another's value silently
        // collapses two per-kind on-disk sandboxes onto one, so
        // [`crate::LayoutInvariants::verify`] would gate a
        // [`CaixaKind::Binario`] caixa's `:exe` entries against the
        // wrong sub-tree (or a `:kind Servico` caixa's `:servicos`
        // entries against `lib/` and pass every entry `feira build`
        // should have rejected as [`crate::LayoutError::ServicoOutsideDir`]).
        // Mirror of the peer
        // [`layout_missing_entry_kind_consts_are_pairwise_distinct`]
        // (fe2a898) on the sibling leaf-kind label accept-set.
        let entries: &[(&str, &str)] = &[
            ("LAYOUT_DIR_LIB", crate::render::LAYOUT_DIR_LIB),
            ("LAYOUT_DIR_EXE", crate::render::LAYOUT_DIR_EXE),
            ("LAYOUT_DIR_SERVICOS", crate::render::LAYOUT_DIR_SERVICOS),
        ];
        for (i, (name_a, value_a)) in entries.iter().enumerate() {
            for (name_b, value_b) in entries.iter().skip(i + 1) {
                assert_ne!(
                    value_a, value_b,
                    "LAYOUT_DIR_* consts must be pairwise-distinct \
                     byte-strings — {name_a} and {name_b} both resolve \
                     to {value_a:?}"
                );
            }
        }
    }

    #[test]
    fn layout_dir_exe_matches_layout_missing_entry_kind_exe() {
        // Cross-axis byte-identity pin: [`crate::render::LAYOUT_DIR_EXE`]
        // (the on-disk-directory-name arm) equals
        // [`crate::render::LAYOUT_MISSING_ENTRY_KIND_EXE`] (the
        // [`LayoutError::MissingEntry`] `kind:` leaf-kind categorization
        // arm) verbatim — the M0 `:kind Binario` on-disk-directory axis
        // and the [`crate::LayoutError::MissingEntry`] `kind:` leaf-kind
        // discriminator name the same three-byte sub-tree (`exe/`), a
        // coincidence [`crate::LayoutInvariants::verify`] itself relies
        // on: it joins `root` with [`crate::render::LAYOUT_DIR_EXE`] to
        // reconstruct `exe_dir` and emits [`crate::LayoutError::MissingEntry
        // { kind: LAYOUT_MISSING_ENTRY_KIND_EXE, path: <under exe_dir> }`]
        // for every non-resolving entry. Making the coincidence
        // load-bearing means a future rebrand touching either axis
        // without the other (a per-consumer disambiguation collapsing
        // the leaf-kind label onto `"binary"` while the directory stays
        // `"exe"`, or vice versa) trips at caixa-core build time rather
        // than surfacing at runtime as a mismatched
        // [`crate::LayoutInvariants::verify`] diagnostic whose `kind:`
        // reads one label while the `path:` sits under a differently-named
        // sub-tree.
        assert_eq!(
            crate::render::LAYOUT_DIR_EXE,
            crate::render::LAYOUT_MISSING_ENTRY_KIND_EXE,
            "LAYOUT_DIR_EXE must equal LAYOUT_MISSING_ENTRY_KIND_EXE — \
             both name the M0 `:kind Binario` sub-tree by the same \
             three-byte scalar"
        );
    }

    #[test]
    fn layout_dir_bib_is_distinct_from_layout_missing_entry_kind_bib() {
        // Cross-axis distinctness pin: [`crate::render::LAYOUT_DIR_LIB`]
        // (`"lib"`, the Cargo-style abbreviated on-disk directory name)
        // is *deliberately* distinct from
        // [`crate::render::LAYOUT_MISSING_ENTRY_KIND_BIBLIOTECA`]
        // (`"biblioteca"`, the full-form Portuguese-native leaf-kind
        // label) — the substrate splits the on-disk convention terse
        // (`lib/`) from the diagnostic vocabulary full (`biblioteca`),
        // matching Cargo's `src/lib.rs` abbreviation of the `library`
        // crate-type discriminator. A future accidental collapse of the
        // two axes onto one scalar (a rebrand aligning either arm with
        // the other for schema-clarity, an English-uniformity pass that
        // renames `LAYOUT_DIR_LIB` to `LAYOUT_DIR_BIBLIOTECA` or the
        // diagnostic label to `"lib"`) would silently reroute either
        // consumer onto the other's byte-string. This pin catches the
        // collapse at build time. Peer of the sibling
        // [`layout_missing_entry_kind_m0_consts_align_with_caixa_kind_as_str`]
        // (fe2a898) that pins the analogous *equality* between the M0
        // `:kind Biblioteca` diagnostic-label arm and
        // [`crate::CaixaKind::Biblioteca`]'s [`crate::CaixaKind::as_str`]
        // output — the two pins jointly encode the "which of the three
        // Biblioteca-related scalars are load-bearing-equal, which are
        // load-bearing-distinct" invariant across the substrate's
        // per-kind vocabulary.
        assert_ne!(
            crate::render::LAYOUT_DIR_LIB,
            crate::render::LAYOUT_MISSING_ENTRY_KIND_BIBLIOTECA,
            "LAYOUT_DIR_LIB (`\"lib\"`) and LAYOUT_MISSING_ENTRY_KIND_BIBLIOTECA \
             (`\"biblioteca\"`) name two distinct axes — the on-disk \
             directory convention (Cargo-style abbreviated) and the \
             layout-diagnostic leaf-kind label (full-form Portuguese) — \
             and must not silently collapse onto one scalar"
        );
    }

    #[test]
    fn layout_dir_servicos_is_distinct_from_layout_missing_entry_kind_servico() {
        // Cross-axis distinctness pin: [`crate::render::LAYOUT_DIR_SERVICOS`]
        // (`"servicos"`, the Portuguese-*plural* on-disk directory
        // name) is *deliberately* distinct from
        // [`crate::render::LAYOUT_MISSING_ENTRY_KIND_SERVICO`]
        // (`"servico"`, the singular leaf-kind label) — the on-disk
        // sub-tree houses one-or-more ComputeUnit YAML descriptors per
        // caixa (hence the plural), the diagnostic label names the
        // caixa's own kind (singular). A future accidental collapse
        // onto one scalar (a per-consumer disambiguation aligning the
        // two, a hypothetical English-uniformity pass renaming
        // `"servicos"` → `"services"` while retaining `"servico"` on
        // the diagnostic arm — or vice versa) would silently reroute
        // either consumer onto the other's byte-string. Peer of the
        // sibling [`layout_dir_bib_is_distinct_from_layout_missing_entry_kind_bib`]
        // pin on the M0 `:kind Biblioteca` split axis; two of the three
        // per-kind on-disk / leaf-kind splits carry a distinctness
        // pin here, the third ([`crate::render::LAYOUT_DIR_EXE`] vs
        // [`crate::render::LAYOUT_MISSING_ENTRY_KIND_EXE`]) carries an
        // equality pin under
        // [`layout_dir_exe_matches_layout_missing_entry_kind_exe`].
        assert_ne!(
            crate::render::LAYOUT_DIR_SERVICOS,
            crate::render::LAYOUT_MISSING_ENTRY_KIND_SERVICO,
            "LAYOUT_DIR_SERVICOS (`\"servicos\"`, plural on-disk sub-tree) \
             and LAYOUT_MISSING_ENTRY_KIND_SERVICO (`\"servico\"`, singular \
             leaf-kind label) name two distinct axes and must not silently \
             collapse onto one scalar"
        );
    }

    #[test]
    fn layout_invariants_reconstruct_sandbox_roots_through_lifted_layout_dir_consts() {
        // Production-through-const pin: [`LayoutInvariants::verify`]
        // routes its three per-kind sandbox-root joins
        // (`root.join(LAYOUT_DIR_LIB)` for the `:kind Biblioteca`
        // default `lib/<nome>.lisp` reconstruction, `root.join(LAYOUT_DIR_EXE)`
        // for the [`LayoutError::ExeOutsideDir`] gate,
        // `root.join(LAYOUT_DIR_SERVICOS)` for the
        // [`LayoutError::ServicoOutsideDir`] gate) through the three
        // lifted consts, not through inline `"lib"` / `"exe"` /
        // `"servicos"` `&str` literals. This test drives the
        // [`LayoutError::ExeOutsideDir`] arm through a `:kind Binario`
        // caixa whose declared `:exe` entry deliberately escapes
        // `root.join(LAYOUT_DIR_EXE)` (a sibling `bin/tool` path) —
        // if the production emit reads the wrong const (or reverts to
        // an inline literal that drifts from the const) the diagnostic
        // arm surfaces the wrong variant, catching the drift at build
        // time rather than as a per-invocation runtime mismatch.
        //
        // Mirror of the peer production-through-const pin
        // [`crate::dep::tests::validate_no_self_dep_deps_field_routes_through_dep_author_key`]
        // (4da6fba) on the sibling M0 `:deps` `list:` diagnostic axis.
        use std::path::PathBuf;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let bin_entry_outside = root.join("bin/tool");
        let mut c = caixa(CaixaKind::Binario);
        c.exe = vec!["bin/tool".into()];
        let manifest_clone = manifest.clone();
        let outside_clone = bin_entry_outside.clone();
        let layout = StandardLayout::new()
            .with_path_exists(move |p| p == manifest_clone || p == outside_clone);
        let err = layout.verify(&c, &root).unwrap_err();
        match err {
            LayoutError::ExeOutsideDir(path) => {
                assert_eq!(
                    path, bin_entry_outside,
                    "ExeOutsideDir must carry the resolved `:exe` entry that \
                     escapes `root.join(LAYOUT_DIR_EXE)`"
                );
                // Byte-identity check: the escape must be against the
                // lifted `LAYOUT_DIR_EXE` sub-tree, not a stale inline
                // literal — a future const-edit that drifts from `"exe"`
                // reroutes `exe_dir` off the sandbox `bin/tool` escapes
                // from, and this pattern-arm miss re-surfaces here.
                assert!(
                    !path.starts_with(root.join(crate::render::LAYOUT_DIR_EXE)),
                    "resolved `:exe` entry {path:?} must escape the \
                     `root.join(LAYOUT_DIR_EXE)` sub-tree the production \
                     emit uses to gate the [`LayoutError::ExeOutsideDir`] arm"
                );
            }
            other => panic!("expected ExeOutsideDir, got {other:?}"),
        }
    }

    #[test]
    fn upgrade_state_change_without_behavior_callback_surfaces_as_upgrade_violation() {
        // Wiring pin for the cross-slot composition gate
        // (`validate_upgrade_from_against_behavior`): a caixa whose
        // `:upgrade-from` declares a `(:state-change "lib/m.lisp")`
        // instruction but does not declare `:behavior :on-state-change`
        // surfaces at `feira build` time as a `LayoutError::UpgradeViolation`
        // naming the offending caixa + the entry's `:from` + the
        // offending script — not at hot-upgrade dispatch when the
        // operator reaches for the missing callback. Mirrors
        // `upgrade_from_downgrade_surfaces_as_upgrade_violation` on the
        // peer `:from` ↔ `:versao` cross-slot precedence gate.
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        use std::path::PathBuf;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let mut c = caixa(CaixaKind::Servico);
        c.versao = "0.2.0".into();
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        // `:behavior` is None (the canonical "I added the upgrade path
        // but never declared :behavior" footgun the gate closes); a
        // peer arm covers the BehaviorSpec-Some-but-on-state-change-
        // None shape in `upgrade::tests::behavior_gate_rejects_state_
        // change_when_on_state_change_is_none`.
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![
                UpgradeInstruction::LoadModule {
                    module: "demo".into(),
                },
                UpgradeInstruction::StateChange {
                    script: PathBuf::from("lib/migrations/v01-to-v02.lisp"),
                },
            ],
        }];
        let manifest_clone = manifest.clone();
        let svc_clone = svc.clone();
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest_clone || p == svc_clone);
        let err = layout.verify(&c, &root).unwrap_err();
        match err {
            LayoutError::UpgradeViolation { caixa, issue } => {
                assert_eq!(caixa, "demo", "diagnostic must name the offending caixa");
                assert!(
                    issue.contains(crate::render::M2_BEHAVIOR_AUTHOR_KEY_ON_STATE_CHANGE),
                    "diagnostic must name the missing callback slot for self-locating fix, \
                     got {issue:?}"
                );
                assert!(
                    issue.contains("0.1.0"),
                    "diagnostic must name the offending entry's :from, got {issue:?}"
                );
                assert!(
                    issue.contains("v01-to-v02.lisp"),
                    "diagnostic must name the offending :script for self-locating fix, \
                     got {issue:?}"
                );
            }
            other => panic!("expected UpgradeViolation, got {other:?}"),
        }
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
    fn supervisor_self_referential_child_is_violation() {
        // A Supervisor whose `:children` names its own `:nome` is a
        // one-node supervision cycle. The cross-slot gate fires at
        // verify time, surfacing as a SupervisorViolation that names the
        // offending supervisor — not at the cluster apply far from
        // source. The `caixa()` helper's `:nome` is "demo".
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
                caixa: "demo".into(),
                versao: "^0.1".into(),
                restart: RestartPolicy::Permanent,
            },
        ];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::SupervisorViolation { caixa, issue } = err else {
            panic!("expected SupervisorViolation for self-referential child, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("demo") && issue.contains("itself"),
            "issue must name the self-supervising caixa, got {issue:?}"
        );
    }

    #[test]
    fn supervisor_distinct_children_pass_self_supervision_gate() {
        // Positive control: a Supervisor whose children are all distinct
        // from its own `:nome` verifies cleanly.
        use crate::{ChildSpec, RestartPolicy, RestartStrategy};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Supervisor);
        c.estrategia = Some(RestartStrategy::OneForOne);
        c.max_restarts = Some(5);
        c.children = vec![ChildSpec {
            caixa: "worker".into(),
            versao: "^0.1".into(),
            restart: RestartPolicy::Permanent,
        }];
        layout.verify(&c, &root).unwrap();
    }

    #[test]
    fn cross_slot_self_edge_gates_route_parent_nome_through_lifted_accessor() {
        // Composition pin: every cross-slot self-edge gate fired from
        // `LayoutInvariants::verify` — the supervision-tree arm's
        // `crate::supervisor::validate_no_self_supervision` call, the
        // Aplicacao arm's `crate::aplicacao::validate_no_self_membership`
        // call, and the dep-graph arm's
        // `crate::dep::validate_no_self_dep` call — must key its
        // `parent_nome` arg off the typed [`Caixa::nome`] accessor, not
        // the raw `&caixa.nome` `&String`-borrow of the underlying
        // field.
        //
        // Structurally: a rename of the storage field or a hypothetical
        // accessor rebrand (a per-cluster alias table pinned through a
        // future `:placement`-scoped slot, the M4 CR materializer's
        // per-CR namespace-qualified rewrite, a `:nome-suffix` overlay
        // the MESH-COMPOSITION §III.2 roadmap acknowledges) would land
        // through the accessor by construction; a raw-borrow bypass
        // would silently disagree with every peer consumer that already
        // routes through `caixa.nome()` (the caixa-mesh 980c059,
        // caixa-helm 22461ef, caixa-flux 162e2e2, caixa-crd 61d3429,
        // caixa-feira ef83332 raw-borrow converges), reintroducing the
        // drift surface the sibling converges closed. Each arm fires
        // its per-kind `LayoutError` variant (`SupervisorViolation` /
        // `AplicacaoViolation` / `DepsViolation`) whose `caixa` field
        // carries the offending parent name verbatim through
        // `caixa.nome().clone()`; asserting the field equals
        // `caixa.nome()` on the mutated fixture pins the accessor-
        // routed parent-nome projection at every call site — a future
        // silent detour that had the gate observe a stale / aliased
        // name at the arg boundary would surface here as a
        // `caixa != "demo"` inequality.
        //
        // Peer of the sibling per-caixa-crate `nome`-arg raw-borrow
        // convergence pin discipline (54bf2f3 / 22461ef / 162e2e2 on the
        // renderer crates; ef83332 on the CLI) — extends the "one typed
        // dispatch per `:nome` consumer" discipline onto the substrate's
        // own [`LayoutInvariants::verify`] cross-slot self-edge gate
        // wire-up on all three typed-name-graph kinds.
        use crate::{
            ChildSpec, Dep, Membro, Placement, PlacementStrategy, RestartPolicy, RestartStrategy,
        };
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);

        // Supervisor arm — the `caixa()` helper's `:nome` is "demo",
        // and the accessor's return `caixa.nome()` must equal the
        // parent-nome that the self-supervision gate observes.
        let mut sup = caixa(CaixaKind::Supervisor);
        sup.estrategia = Some(RestartStrategy::OneForOne);
        sup.max_restarts = Some(5);
        sup.children = vec![ChildSpec {
            caixa: "demo".into(),
            versao: "^0.1".into(),
            restart: RestartPolicy::Permanent,
        }];
        let parent_nome_via_accessor = sup.nome();
        assert_eq!(
            parent_nome_via_accessor, "demo",
            "the caixa() fixture helper's `:nome` must be \"demo\" — \
             the accessor's return is the pin's ground truth for the \
             cross-slot gate's parent-nome arg",
        );
        let err = layout.verify(&sup, &root).unwrap_err();
        let LayoutError::SupervisorViolation { caixa: c_nome, .. } = err else {
            panic!("expected SupervisorViolation for self-referential child, got {err:?}");
        };
        assert_eq!(
            c_nome, parent_nome_via_accessor,
            "the SupervisorViolation's `caixa` field must equal \
             `sup.nome()` — the cross-slot self-supervision gate's \
             `parent_nome` arg must route through the lifted \
             [`Caixa::nome`] accessor, not the raw `&caixa.nome` \
             `&String`-borrow of the underlying field",
        );

        // Aplicacao arm — same discipline on the peer typed-name-graph
        // kind. Constructed alongside the supervisor arm so any future
        // accessor drift lands on both arms in the same pin.
        let mut app = caixa(CaixaKind::Aplicacao);
        app.placement = Some(Placement {
            estrategia: PlacementStrategy::Replicated,
            clusters: vec!["rio".into()],
            affinity: None,
            shard_key: None,
        });
        app.membros = vec![Membro {
            caixa: "demo".into(),
            versao: "^0.1".into(),
        }];
        let parent_nome_via_accessor = app.nome();
        assert_eq!(
            parent_nome_via_accessor, "demo",
            "the caixa() fixture helper's `:nome` must be \"demo\" on \
             the Aplicacao arm too — same accessor-ground-truth as the \
             sibling supervisor arm above",
        );
        let err = layout.verify(&app, &root).unwrap_err();
        let LayoutError::AplicacaoViolation { caixa: c_nome, .. } = err else {
            panic!("expected AplicacaoViolation for self-referential membro, got {err:?}");
        };
        assert_eq!(
            c_nome, parent_nome_via_accessor,
            "the AplicacaoViolation's `caixa` field must equal \
             `app.nome()` — the cross-slot self-membership gate's \
             `parent_nome` arg must route through the lifted \
             [`Caixa::nome`] accessor, not the raw `&caixa.nome` \
             `&String`-borrow of the underlying field",
        );

        // Dep-graph arm — third typed-name-graph kind on the
        // `parent_nome` arg boundary. Same discipline as the peer
        // supervision-tree and Aplicacao-membership arms above.
        // Constructed alongside so any future accessor drift lands on
        // all three arms in the same pin. Needs a distinct layout
        // fixture from the supervisor / aplicacao arms above because
        // the `Biblioteca` kind's code-path existence gate demands the
        // canonical `lib/<nome>.lisp` path also `exists`, so the shim
        // covers both `caixa.lisp` and `lib/demo.lisp`.
        let default_lib = root.join("lib").join("demo.lisp");
        let manifest_dep = manifest.clone();
        let default_lib_clone = default_lib.clone();
        let layout_dep = StandardLayout::new()
            .with_path_exists(move |p| p == manifest_dep || p == default_lib_clone);
        let mut lib = caixa(CaixaKind::Biblioteca);
        lib.deps = vec![Dep::simple("demo", "^0.1")];
        let parent_nome_via_accessor = lib.nome();
        assert_eq!(
            parent_nome_via_accessor, "demo",
            "the caixa() fixture helper's `:nome` must be \"demo\" on \
             the Biblioteca arm too — same accessor-ground-truth as the \
             sibling supervisor + Aplicacao arms above",
        );
        let err = layout_dep.verify(&lib, &root).unwrap_err();
        let LayoutError::DepsViolation { caixa: c_nome, .. } = err else {
            panic!("expected DepsViolation for self-referential :deps entry, got {err:?}");
        };
        assert_eq!(
            c_nome, parent_nome_via_accessor,
            "the DepsViolation's `caixa` field must equal \
             `lib.nome()` — the cross-slot self-dep gate's `parent_nome` \
             arg must route through the lifted [`Caixa::nome`] accessor, \
             not the raw `&caixa.nome` `&String`-borrow of the underlying \
             field",
        );
    }

    #[test]
    fn upgrade_against_versao_gate_routes_current_versao_through_lifted_accessor() {
        // Composition pin: the cross-slot `:upgrade-from :from` ↔
        // `:versao` precedence gate fired from
        // `LayoutInvariants::verify` — the
        // `crate::upgrade::validate_upgrade_from_against_versao` call —
        // must key its `versao` arg off the typed [`Caixa::versao`]
        // accessor, not the raw `&caixa.versao` `&String`-borrow of
        // the underlying field.
        //
        // Same "arg-boundary reads through the lifted accessor"
        // discipline as the sibling
        // [`cross_slot_self_edge_gates_route_parent_nome_through_lifted_accessor`]
        // pin above on the `:nome`-arg axis of the three typed-name-
        // graph self-edge gates — extended here onto the `:versao`-arg
        // axis of the substrate's remaining `LayoutInvariants::verify`
        // cross-slot arg-carrying call site. Structurally byte-equal
        // today (the accessor is `pub fn versao(&self) -> &str { &self.versao }`,
        // so both paths coerce to the same `&str`); the pin catches a
        // future silent detour (an accessor rebrand that no longer
        // shipped the raw slot verbatim — a per-`:edicao` overlay,
        // a promotion of `:versao` to a `CaixaVersion` newtype with a
        // canonicalizing accessor, an M4 CR-materializer-side pinning
        // through a resolver-annotated `:versao-resolved` slot) that
        // would silently split the substrate's own precedence gate
        // from every peer consumer already routing `:versao` reads
        // through the lifted accessor.
        //
        // The gate fires an `UpgradeViolation { caixa, issue }` when a
        // `:upgrade-from` entry's `:from` is not strictly less than
        // the top-level `:versao` — the `issue` string names both the
        // offending prior version and the current version verbatim,
        // so asserting the substring `caixa.versao()` appears in the
        // fired diagnostic pins the accessor-routed current-versao
        // projection at the arg boundary. A raw-borrow bypass would
        // still surface the same bytes today, but the presence of
        // this pin makes any future divergence between the accessor's
        // return and the raw slot's contents a build-time failure at
        // this call site.
        use crate::{UpgradeFromEntry, UpgradeInstruction};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let servico_path = root.join("servicos").join("demo.computeunit.yaml");
        let manifest_clone = manifest.clone();
        let servico_clone = servico_path.clone();
        let layout = StandardLayout::new()
            .with_path_exists(move |p| p == manifest_clone || p == servico_clone);
        let mut svc = caixa(CaixaKind::Servico);
        svc.versao = "0.1.0".into();
        svc.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        // `:from` >= current `:versao` — trips the precedence gate the
        // `validate_upgrade_from_against_versao` cross-slot call
        // enforces. `:load-module` carries no on-disk path so the
        // path-existence gate downstream stays inert; the precedence
        // gate is what fires. No `:on-state-change` needed because the
        // instruction list carries no `:state-change` entry, so the
        // sibling `validate_upgrade_from_against_behavior` gate is
        // inert too.
        svc.upgrade_from = vec![UpgradeFromEntry {
            from: "0.2.0".into(),
            instructions: vec![UpgradeInstruction::LoadModule {
                module: "demo".into(),
            }],
        }];
        let current_versao_via_accessor = svc.versao().to_string();
        assert_eq!(
            current_versao_via_accessor, "0.1.0",
            "the mutated fixture's `:versao` must be observable through \
             the accessor before layout verification fires — a drift on \
             `Caixa::versao` would surface here as a `!= \"0.1.0\"` \
             inequality",
        );
        let err = layout.verify(&svc, &root).unwrap_err();
        let LayoutError::UpgradeViolation {
            caixa: c_nome,
            issue,
        } = err
        else {
            panic!("expected UpgradeViolation for :from >= :versao, got {err:?}");
        };
        assert_eq!(c_nome, svc.nome(), "wrap envelope names the caixa");
        assert!(
            issue.contains(&current_versao_via_accessor),
            "the UpgradeViolation's `issue` must quote the current \
             `:versao` byte-string verbatim — the cross-slot precedence \
             gate's `versao` arg must route through the lifted \
             [`Caixa::versao`] accessor, not the raw `&caixa.versao` \
             `&String`-borrow of the underlying field. issue: {issue}",
        );
    }

    #[test]
    fn layout_violation_envelopes_carry_caixa_nome_through_lifted_accessor() {
        // Wrap-envelope drift-detection pin: every per-axis
        // `LayoutError::*Violation { caixa, issue }` envelope fired
        // from `LayoutInvariants::verify` must key its offending-caixa
        // field off the typed [`Caixa::nome`] accessor's
        // `.to_string()` extension, not the raw
        // `caixa.nome.clone()` `String::clone()` of the underlying
        // field. Structurally byte-equal today (each accessor is
        // `pub fn nome(&self) -> &str { &self.nome }`, so
        // `caixa.nome().to_string()` and `caixa.nome.clone()` produce
        // the same bytes); the pin catches a future silent detour
        // (an accessor rebrand that no longer shipped the raw slot
        // verbatim — a per-cluster alias table pinned through a
        // future `:placement`-scoped slot, the M4 CR materializer's
        // per-CR namespace-qualified rewrite, a `:nome-suffix`
        // overlay the MESH-COMPOSITION §III.2 roadmap acknowledges)
        // that would silently split the substrate's own layout
        // invariant verifier's diagnostic surface from every peer
        // caixa-crate consumer that already routes `:nome` reads
        // through the lifted accessor (the caixa-mesh 980c059,
        // caixa-helm 22461ef, caixa-flux 162e2e2, caixa-crd 61d3429,
        // caixa-feira ef83332 raw-borrow converges).
        //
        // Exercises a representative variant on each of the three
        // wrap-envelope arm shapes the substrate's per-axis fan-out
        // carries: (1) `LayoutError::NomeViolation` (the leading arm
        // in the `verify` order — the `:nome` axis's DNS-1123 shape
        // gate fires immediately after the manifest-existence gate),
        // (2) `LayoutError::BinarioWithoutExe` (a tuple-variant on
        // the kind-coherence family — different envelope shape than
        // the struct-variant `*Violation { caixa, issue }` family
        // but the same converge target on the `caixa.nome().to_string()`
        // arg), and (3) `LayoutError::ServicoWithoutServicos` (the
        // sibling tuple-variant on the same kind-coherence family).
        // Together they cover the two `LayoutError` envelope shapes
        // (struct-variant + tuple-variant) the layout invariants file
        // emits on `:nome`-carrying arms.
        //
        // Peer of the sibling
        // [`cross_slot_self_edge_gates_route_parent_nome_through_lifted_accessor`]
        // pin above — extends the "wrap-envelope `caixa:` field
        // reads through the lifted accessor" discipline from the
        // cross-slot self-edge gates' `parent_nome` arg boundary
        // onto the per-axis `LayoutError::*Violation` envelope's
        // `caixa:` field boundary.

        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);

        // (1) `NomeViolation` on the struct-variant envelope: force a
        // DNS-1123-invalid `:nome` (uppercase byte — `is_dns_1123_label`
        // rejects) and assert the fired envelope's `caixa:` field
        // byte-equals `c.nome().to_string()`.
        let mut c = caixa(CaixaKind::Biblioteca);
        c.nome = "BAD_NAME".into();
        c.bibliotecas = vec!["lib/demo.lisp".into()];
        let expected_nome_via_accessor = c.nome().to_string();
        assert_eq!(
            expected_nome_via_accessor, "BAD_NAME",
            "the mutated fixture's `:nome` must be observable through \
             the accessor before layout verification fires — a drift \
             on `Caixa::nome` would surface here as a `!= \"BAD_NAME\"` \
             inequality",
        );
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::NomeViolation { caixa: c_nome, .. } = err else {
            panic!("expected NomeViolation for DNS-1123-invalid :nome, got {err:?}");
        };
        assert_eq!(
            c_nome, expected_nome_via_accessor,
            "the NomeViolation's `caixa` field must equal \
             `c.nome().to_string()` — the wrap envelope's per-axis \
             projection must route through the lifted [`Caixa::nome`] \
             accessor's `.to_string()` extension, not the raw \
             `caixa.nome.clone()` `String::clone()` of the underlying \
             field",
        );

        // (2) `BinarioWithoutExe` on the tuple-variant envelope: a
        // Binario-kind caixa with an empty `:exe` list fires the
        // kind-coherence gate whose payload is a bare `String`, so the
        // pattern is `LayoutError::BinarioWithoutExe(String)` rather
        // than the struct-variant `{ caixa, issue }` family. The
        // converge target is the same — `caixa.nome().to_string()` — but
        // the envelope shape is different, so the pin exercises both.
        let mut c = caixa(CaixaKind::Binario);
        // `:exe` empty is the trigger — the fixture helper defaults
        // it to `vec![]`, so no mutation is needed.
        c.nome = "binario-demo".into();
        let expected_nome_via_accessor = c.nome().to_string();
        assert_eq!(
            expected_nome_via_accessor, "binario-demo",
            "the mutated fixture's `:nome` must be observable through \
             the accessor before layout verification fires",
        );
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::BinarioWithoutExe(c_nome) = err else {
            panic!("expected BinarioWithoutExe for empty :exe list on Binario kind, got {err:?}");
        };
        assert_eq!(
            c_nome, expected_nome_via_accessor,
            "the BinarioWithoutExe's payload must equal \
             `c.nome().to_string()` — the tuple-variant envelope's \
             per-axis projection must route through the lifted \
             [`Caixa::nome`] accessor's `.to_string()` extension, not \
             the raw `caixa.nome.clone()` `String::clone()` of the \
             underlying field",
        );

        // (3) `ServicoWithoutServicos` on the sibling tuple-variant
        // envelope: same discipline on the peer kind-coherence
        // partition arm. Constructed alongside the Binario arm so any
        // future accessor drift lands on both arms in the same pin.
        let mut c = caixa(CaixaKind::Servico);
        // `:servicos` empty is the trigger — the fixture helper
        // defaults it to `vec![]`, so no mutation is needed.
        c.nome = "servico-demo".into();
        let expected_nome_via_accessor = c.nome().to_string();
        assert_eq!(
            expected_nome_via_accessor, "servico-demo",
            "the mutated fixture's `:nome` must be observable through \
             the accessor before layout verification fires",
        );
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::ServicoWithoutServicos(c_nome) = err else {
            panic!(
                "expected ServicoWithoutServicos for empty :servicos list on Servico kind, \
                 got {err:?}"
            );
        };
        assert_eq!(
            c_nome, expected_nome_via_accessor,
            "the ServicoWithoutServicos's payload must equal \
             `c.nome().to_string()` — same converge discipline as the \
             sibling `BinarioWithoutExe` tuple-variant arm above",
        );
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

    // ── Caixa::validate_restart_window wired into Supervisor verify ─────
    //
    // Until this wire-up landed `Caixa::validate_restart_window` lived as
    // `pub fn` on `Caixa` with full per-arm unit coverage in
    // `manifest::tests` (`validate_restart_window_rejects_*` — fractional,
    // decimal-shaped integer, half-unit minute, leading sign, unknown
    // unit, garbage, empty-after-trim) but no production path called it;
    // `feira build` silently accepted malformed `:restart-window` and
    // `Caixa::supervisor_view` soft-swallowed the parse failure as
    // `restart_window: None` (the canonical "no reset" sentinel), turning
    // every authoring footgun into a never-reset supervisor far from the
    // source caixa.lisp. The following pins fence the layout-pipeline
    // wire-up: every layout verify on a structurally-invalid `:restart-
    // window` axis surfaces the per-axis `RestartWindowViolation { caixa,
    // issue }` envelope before the typed `SupervisorSpec::validate` gate
    // sees the laundered `None`.

    fn supervisor_with_window(window: Option<&str>) -> Caixa {
        use crate::{ChildSpec, RestartPolicy, RestartStrategy};
        let mut c = caixa(CaixaKind::Supervisor);
        c.estrategia = Some(RestartStrategy::OneForOne);
        c.max_restarts = Some(5);
        c.restart_window = window.map(str::to_string);
        c.children = vec![ChildSpec {
            caixa: "worker".into(),
            versao: "^0.1".into(),
            restart: RestartPolicy::Permanent,
        }];
        c
    }

    #[test]
    fn restart_window_violation_on_fractional_seconds() {
        // `"1.5s"` is the canonical fractional-seconds drift footgun the
        // shared integer-magnitude codec (1c55a2a) rejects: round-trips
        // through `render` as `"1500ms"` on first serialize, breaking
        // THEORY.md §V.2.7 render-determinism. Before this wire-up
        // `supervisor_view` soft-swallowed the parse error as
        // `restart_window: None`, masking the drift as a never-reset
        // supervisor.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let c = supervisor_with_window(Some("1.5s"));
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::RestartWindowViolation { caixa, issue } = err else {
            panic!("expected LayoutError::RestartWindowViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("1.5s"),
            "issue must quote the offending raw value: {issue}",
        );
    }

    #[test]
    fn restart_window_violation_on_decimal_shaped_integer() {
        // `"1.0s"` — decimal-shaped integer the codec also rejects (a
        // canonical authoring form is `"1s"`). Sibling of the fractional
        // case; same codec arm.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let c = supervisor_with_window(Some("1.0s"));
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(
                err,
                LayoutError::RestartWindowViolation { ref caixa, ref issue }
                    if caixa == "demo" && issue.contains("1.0s")
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn restart_window_violation_on_leading_sign() {
        // `"+30s"` / `"-30s"` — leading-sign drift the codec rejects.
        // Canonical form is `"30s"`. Pin both signs separately because
        // a future relaxation might accept one but not the other.
        for raw in ["+30s", "-30s"] {
            let root = PathBuf::from("/tmp/x");
            let manifest = root.join("caixa.lisp");
            let manifest_clone = manifest.clone();
            let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
            let c = supervisor_with_window(Some(raw));
            let err = layout.verify(&c, &root).unwrap_err();
            assert!(
                matches!(
                    err,
                    LayoutError::RestartWindowViolation { ref caixa, ref issue }
                        if caixa == "demo" && issue.contains(raw)
                ),
                "leading-sign {raw:?} got {err:?}",
            );
        }
    }

    #[test]
    fn restart_window_violation_on_unknown_unit() {
        // `"30x"` — unknown duration unit. The codec admits only
        // `ms`/`s`/`m`/`h`.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let c = supervisor_with_window(Some("30x"));
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(
                err,
                LayoutError::RestartWindowViolation { ref caixa, .. } if caixa == "demo"
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn restart_window_violation_on_garbage() {
        // `"abc"` — pure garbage. The codec's parse fails before the
        // unit dispatch; the wrap envelope still surfaces the
        // self-locating diagnostic at the source.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let c = supervisor_with_window(Some("abc"));
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::RestartWindowViolation { ref caixa, .. } if caixa == "demo"),
            "got {err:?}",
        );
    }

    #[test]
    fn restart_window_violation_on_empty_string() {
        // `""` — empty after trim. The shared codec's digit-only gate
        // refuses an empty magnitude. Distinguished here from the
        // `None` ("omit the slot") canonical authoring shape: an empty
        // string is an authored-but-empty slot, never the author's
        // intent.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let c = supervisor_with_window(Some(""));
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::RestartWindowViolation { ref caixa, .. } if caixa == "demo"),
            "got {err:?}",
        );
    }

    #[test]
    fn verify_accepts_supervisor_without_restart_window() {
        // `None` is the canonical "omit the slot to express no reset"
        // shape — never reaches the codec, validates cleanly.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let c = supervisor_with_window(None);
        layout.verify(&c, &root).unwrap();
    }

    #[test]
    fn verify_accepts_supervisor_with_canonical_restart_window() {
        // Every canonical form the shared codec round-trips losslessly
        // must pass — `"500ms"`, `"30s"`, `"60s"`, `"1m"`, `"2m"`,
        // `"1h"`. Pin every form so a future tightening of the codec's
        // accepted set surfaces here as a test failure.
        for form in ["500ms", "30s", "60s", "1m", "2m", "1h"] {
            let root = PathBuf::from("/tmp/x");
            let manifest = root.join("caixa.lisp");
            let manifest_clone = manifest.clone();
            let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
            let c = supervisor_with_window(Some(form));
            layout
                .verify(&c, &root)
                .unwrap_or_else(|e| panic!("canonical {form:?} must validate, got {e:?}"));
        }
    }

    #[test]
    fn restart_window_violation_fires_before_supervisor_view_validate() {
        // Diagnostic-precedence pin: a Supervisor with a malformed
        // `:restart-window` AND a typed-shape defect on the typed view
        // (zero `:max-restarts`, which `SupervisorSpec::validate`'s
        // `ZeroMaxRestarts` arm rejects) surfaces the raw-string
        // diagnostic first — the narrower self-locating gate wins. Until
        // this wire-up landed `supervisor_view` would silently launder
        // the malformed `:restart-window` to `None` and then the typed
        // view's `ZeroMaxRestarts` gate would surface, masking the
        // raw-string footgun.
        use crate::{ChildSpec, RestartPolicy, RestartStrategy};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Supervisor);
        c.estrategia = Some(RestartStrategy::OneForOne);
        c.max_restarts = Some(0);
        c.restart_window = Some("1.5s".into());
        c.children = vec![ChildSpec {
            caixa: "worker".into(),
            versao: "^0.1".into(),
            restart: RestartPolicy::Permanent,
        }];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::RestartWindowViolation { .. }),
            "got {err:?} — RestartWindowViolation must fire before SupervisorViolation",
        );
    }

    #[test]
    fn supervisor_slots_on_non_supervisor_fires_before_restart_window_violation() {
        // Order pin: a non-Supervisor caixa with a malformed
        // `:restart-window` surfaces `SupervisorSlotsOnNonSupervisor`
        // (the kind-coherence gate at the top of verify) before the
        // raw-string parse gate inside the Supervisor branch — because
        // `:restart-window` is foreign to non-Supervisor kinds, the
        // kind-coherence diagnostic is the load-bearing one. Mirrors
        // the existing `nome_violation_on_*` ordering tests that fence
        // the precedence between universal and kind-specific gates.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let default_lib = root.join("lib").join("demo.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == default_lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.restart_window = Some("1.5s".into());
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::SupervisorSlotsOnNonSupervisor { .. }),
            "got {err:?} — kind-coherence must fire before RestartWindowViolation",
        );
    }

    #[test]
    fn restart_window_violation_diagnostic_carries_offending_value() {
        // Diagnostic-shape pin: the wrap envelope's `issue` carries the
        // codec's parser-shaped reason verbatim (which names the
        // offending raw value), so the author can grep their caixa.lisp
        // for `:restart-window "<value>"` and fix in one edit. Mirrors
        // `nome_violation_*_carries_offending_*` shape pins.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let c = supervisor_with_window(Some("0.5m"));
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::RestartWindowViolation { caixa, issue } = err else {
            panic!("expected LayoutError::RestartWindowViolation, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("0.5m"),
            "issue must quote the offending raw value verbatim: {issue}",
        );
        assert!(
            !issue.is_empty(),
            "issue must carry the codec's parser-shaped reason",
        );
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
    fn aplicacao_self_referential_membro_is_violation() {
        // An Aplicacao whose `:membros` names its own `:nome` is a
        // one-node lacre-closure recursion. The cross-slot gate fires
        // at verify time, surfacing as an AplicacaoViolation that
        // names the offending aplicacao — not at lacre-resolve time
        // far from source. The `caixa()` helper's `:nome` is "demo".
        // Peer of `supervisor_self_referential_child_is_violation`
        // on the supervision-tree axis.
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
        c.membros = vec![
            Membro {
                caixa: "service-a".into(),
                versao: "^0.1".into(),
            },
            Membro {
                caixa: "demo".into(),
                versao: "^0.1".into(),
            },
        ];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::AplicacaoViolation { caixa, issue } = err else {
            panic!("expected AplicacaoViolation for self-referential membro, got {err:?}");
        };
        assert_eq!(caixa, "demo");
        assert!(
            issue.contains("demo") && issue.contains("lists itself"),
            "issue must name the self-membering aplicacao, got {issue:?}"
        );
    }

    #[test]
    fn aplicacao_distinct_membros_pass_self_membership_gate() {
        // Positive control: an Aplicacao whose membros are all distinct
        // from its own `:nome` verifies cleanly.
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
        c.membros = vec![
            Membro {
                caixa: "service-a".into(),
                versao: "^0.1".into(),
            },
            Membro {
                caixa: "service-b".into(),
                versao: "^0.1".into(),
            },
        ];
        layout.verify(&c, &root).unwrap();
    }

    #[test]
    fn aplicacao_self_membership_fires_after_view_validate() {
        // Diagnostic-precedence pin: a self-referential membro alongside
        // a duplicate-:caixa shape surfaces the more-fundamental
        // `MembroDuplicate` (from `view.validate()`) first; only when the
        // per-membros shape diagnostics pass does the cross-slot
        // self-membership gate fire. Mirrors the ordering pin
        // `supervisor_self_referential_child_is_violation` carries on
        // the peer supervision-tree axis (`view.validate()` runs first,
        // then the cross-slot gate). Without this ordering a future
        // refactor that swaps the two calls would silently mask the
        // narrower per-membro defect.
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
        c.membros = vec![
            Membro {
                caixa: "service-a".into(),
                versao: "^0.1".into(),
            },
            Membro {
                caixa: "service-a".into(),
                versao: "^0.2".into(),
            },
            Membro {
                caixa: "demo".into(),
                versao: "^0.1".into(),
            },
        ];
        let err = layout.verify(&c, &root).unwrap_err();
        let LayoutError::AplicacaoViolation { issue, .. } = err else {
            panic!("expected AplicacaoViolation, got {err:?}");
        };
        // The per-membros duplicate diagnostic (from view.validate())
        // surfaces ahead of the cross-slot self-membership gate, so the
        // `service-a` duplicate is named — not the `demo` self-reference.
        assert!(
            issue.contains("service-a") && issue.contains("more than once"),
            "duplicate-:caixa diagnostic must surface before self-membership gate, \
             got {issue:?}"
        );
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
                assert_eq!(slots, crate::render::M3_AUTHOR_KEY_ENTRADA);
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
                assert_eq!(
                    slots,
                    format!(
                        "{} {} {} {} {}",
                        crate::render::M3_AUTHOR_KEY_MEMBROS,
                        crate::render::M3_AUTHOR_KEY_CONTRATOS,
                        crate::render::M3_AUTHOR_KEY_POLITICAS,
                        crate::render::M3_AUTHOR_KEY_PLACEMENT,
                        crate::render::M3_AUTHOR_KEY_ENTRADA,
                    )
                );
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
                assert_eq!(slots, crate::render::SUPERVISOR_AUTHOR_KEY_CHILDREN);
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
                assert_eq!(slots, crate::render::SUPERVISOR_AUTHOR_KEY_ESTRATEGIA);
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
    fn servico_slots_on_biblioteca_rejected() {
        // Mirror of `mesh_slots_on_servico_rejected` /
        // `supervisor_slots_on_servico_rejected` on the M2
        // Servico-runtime slot set: an author adds `:limits` to a
        // `:kind Biblioteca` expecting per-process sandboxing. The
        // caixa-helm / caixa-flux renderers gate on `require_kind(_,
        // Servico)`, so the slot is the manifest's "ignored otherwise" —
        // never rendered into any artifact. The kind-coherence gate
        // rejects it at build time (before the M2 validate blocks),
        // naming the offending slot + kind.
        use crate::LimitsSpec;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let lib = root.join("lib").join("demo.lisp");
        let manifest_clone = manifest.clone();
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest_clone || p == lib);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.limits = Some(LimitsSpec {
            fuel: Some(1_000_000),
            ..Default::default()
        });
        let err = layout.verify(&c, &root).unwrap_err();
        match err {
            LayoutError::ServicoSlotsOnNonServico { caixa, kind, slots } => {
                assert_eq!(caixa, "demo");
                assert_eq!(kind, CaixaKind::Biblioteca);
                assert_eq!(slots, crate::render::M2_AUTHOR_KEY_LIMITS);
            }
            other => panic!("expected ServicoSlotsOnNonServico, got {other:?}"),
        }
    }

    #[test]
    fn servico_slots_on_non_servico_lists_slots_in_canonical_order() {
        // All three M2 slots declared on a Biblioteca → the diagnostic
        // enumerates them in canonical declaration order (`:limits` →
        // `:behavior` → `:upgrade-from`), deterministic across runs. The
        // gate fires on declared-ness only (the values need not pass the
        // M2 validate blocks — those run only after the kind-coherence
        // gate, and never for a non-Servico declared-slot caixa). Mirror
        // of the mesh/supervisor `*_lists_slots_in_canonical_order` pins.
        use crate::{BehaviorSpec, LimitsSpec, UpgradeFromEntry, UpgradeInstruction};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.limits = Some(LimitsSpec {
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
        let err = layout.verify(&c, &root).unwrap_err();
        match err {
            LayoutError::ServicoSlotsOnNonServico { slots, .. } => {
                assert_eq!(
                    slots,
                    format!(
                        "{} {} {}",
                        crate::render::M2_AUTHOR_KEY_LIMITS,
                        crate::render::M2_AUTHOR_KEY_BEHAVIOR,
                        crate::render::M2_AUTHOR_KEY_UPGRADE_FROM,
                    )
                );
            }
            other => panic!("expected ServicoSlotsOnNonServico, got {other:?}"),
        }
    }

    #[test]
    fn aplicacao_with_servico_slots_rejected() {
        // Cross-kind pin (mirror of `aplicacao_with_supervisor_slots_rejected`):
        // an Aplicacao that declares an M2 Servico-runtime slot is
        // rejected by the Servico-slot gate, just as a Supervisor
        // declaring a mesh slot is rejected by the mesh-slot gate — the
        // three kind ↔ slot coherence gates are symmetric and mutually
        // exclusive. The gate fires before the Aplicacao typed-graph
        // validation, so the diagnostic names the foreign M2 slot rather
        // than a downstream AplicacaoViolation about missing :membros.
        use crate::{Membro, Placement, PlacementStrategy, UpgradeFromEntry, UpgradeInstruction};
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
        c.upgrade_from = vec![UpgradeFromEntry {
            from: "0.1.0".into(),
            instructions: vec![UpgradeInstruction::Restart],
        }];
        let err = layout.verify(&c, &root).unwrap_err();
        match err {
            LayoutError::ServicoSlotsOnNonServico { caixa, kind, slots } => {
                assert_eq!(caixa, "demo");
                assert_eq!(kind, CaixaKind::Aplicacao);
                assert_eq!(slots, crate::render::M2_AUTHOR_KEY_UPGRADE_FROM);
            }
            other => panic!("expected ServicoSlotsOnNonServico, got {other:?}"),
        }
    }

    #[test]
    fn servico_with_servico_slots_still_verifies() {
        // Pass-after control: a well-formed Servico carrying all three M2
        // slots must remain accepted — the gate is guarded by `kind !=
        // Servico`, so it must not over-fire on the kind these slots
        // exist for. Mirror of `servico_without_{mesh,supervisor}_slots_
        // still_verifies` on the legitimate-declaration axis.
        use crate::{BehaviorSpec, LimitsSpec, UpgradeFromEntry, UpgradeInstruction};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let init = root.join("lib/init.lisp");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == svc || p == init);
        let mut c = caixa(CaixaKind::Servico);
        c.versao = "0.2.0".into();
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.limits = Some(LimitsSpec {
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
    fn acao_must_not_have_bibliotecas() {
        // Mirror of `supervisor_must_not_have_bibliotecas` /
        // `aplicacao_must_not_have_bibliotecas` on the third no-code
        // kind. `has_code` fires before the `:ci`-presence gates below
        // it, so this must surface `AcaoOwnsCode` even though the
        // caixa also lacks a `:ci` slot (which would otherwise surface
        // as `MissingCi`) — the more-fundamental "this kind runs no
        // code at all" diagnostic wins.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Acao);
        c.bibliotecas = vec!["lib/code.lisp".into()];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(matches!(err, LayoutError::AcaoOwnsCode(_)));
    }

    #[test]
    fn acao_without_ci_errors() {
        // Mirror of `binario_without_exe_errors` on the fifth required-
        // slot axis.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let err = layout.verify(&caixa(CaixaKind::Acao), &root).unwrap_err();
        assert!(matches!(err, LayoutError::MissingCi(_)));
    }

    #[test]
    fn acao_with_ci_passes() {
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest);
        let mut c = caixa(CaixaKind::Acao);
        c.ci = Some(canteiro_types::CiRun {
            workspace: "pleme-io".into(),
            repo: "caixa".into(),
            nodes: vec![],
        });
        layout
            .verify(&c, &root)
            .expect("an Acao caixa with a declared :ci slot passes layout verify");
    }

    #[test]
    fn acao_with_cyclic_ci_rejected_at_layout() {
        // Layout-side wire-up pin on the compound
        // [`crate::Caixa::validate_acao_shape`] gate: a `:kind Acao`
        // caixa declaring a structurally illegal `:ci` (here — a
        // minimal two-node cycle `a → b → a`, one of the three
        // `canteiro_types::DecomposeError` arms
        // [`crate::decompose_ci`] refuses) surfaces
        // [`LayoutError::AcaoViolation`] at `feira build` time rather
        // than passing the layout gate silently and deferring the
        // diagnostic to [`caixa_actions::validate`] at renderer time.
        //
        // Pre-lift the layout pipeline only checked `:ci` *presence*
        // via [`LayoutError::MissingCi`]; the decompose gate lived
        // only wired open-coded at
        // [`caixa_actions::validate`] via the substrate-canonical
        // [`crate::require_acao_view`] compound helper. This wire-up
        // pin locks the new layout-side compound-shape gate in place
        // — a future regression that dropped the `if
        // caixa.kind().is_acao() { validate_acao_shape() }` block or
        // relaxed the diagnostic surface trips here at caixa-core
        // build time. Sibling in shape to the peer
        // [`aplicacao_must_not_have_bibliotecas`] /
        // [`supervisor_must_not_have_bibliotecas`] /
        // [`acao_must_not_have_bibliotecas`] layout wire-up pins on
        // the sibling per-kind shape gates.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Acao);
        c.ci = Some(canteiro_types::CiRun {
            workspace: "pleme-io".into(),
            repo: "caixa".into(),
            nodes: vec![
                canteiro_types::CiNode::new(
                    "a",
                    canteiro_types::EnvClass::None,
                    canteiro_types::ActionRef {
                        name: "a".into(),
                        command: "true".into(),
                        args: vec![],
                    },
                    vec!["b".into()],
                ),
                canteiro_types::CiNode::new(
                    "b",
                    canteiro_types::EnvClass::None,
                    canteiro_types::ActionRef {
                        name: "b".into(),
                        command: "true".into(),
                        args: vec![],
                    },
                    vec!["a".into()],
                ),
            ],
        });
        let err = layout.verify(&c, &root).unwrap_err();
        match err {
            LayoutError::AcaoViolation { caixa, issue } => {
                assert_eq!(caixa, "demo");
                assert!(
                    issue.contains("decompose"),
                    "AcaoViolation issue must name the decompose axis (got: {issue:?})",
                );
                assert!(
                    issue.contains("demo"),
                    "AcaoViolation issue must name the offending caixa nome via the folded \
                     CiDecomposeFailure Display (got: {issue:?})",
                );
            }
            other => panic!("expected AcaoViolation on a cyclic :ci, got {other:?}"),
        }
    }

    #[test]
    fn ci_on_non_acao_errors() {
        // Mirror of `mesh_slots_on_non_aplicacao_lists_slots_in_canonical_order`
        // on the Acao-only `:ci` axis — declaring `:ci` on any other
        // kind is the same "silently ignored" footgun the sibling
        // mesh-/supervisor-/servico-slot gates already close.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.ci = Some(canteiro_types::CiRun {
            workspace: "pleme-io".into(),
            repo: "caixa".into(),
            nodes: vec![],
        });
        let err = layout.verify(&c, &root).unwrap_err();
        match err {
            LayoutError::CiOnNonAcao { caixa, kind } => {
                assert_eq!(caixa, "demo");
                assert_eq!(kind, CaixaKind::Biblioteca);
            }
            other => panic!("expected CiOnNonAcao, got {other:?}"),
        }
    }

    // ── ForeignCodeSlot — kind ↔ code-surface coherence ────────────────

    #[test]
    fn biblioteca_with_exe_rejected() {
        // Fail-before-pass-after pin: a `:kind Biblioteca` declaring
        // `:exe` is the "I added a CLI to my library" footgun — the nix
        // flake renderer for Binario gates on `require_kind(_, Binario)`,
        // so on a Biblioteca the `:exe` path is silently dropped past
        // the layout's path-existence check (no executable target is
        // ever generated). The diagnostic names the offending kind +
        // slot verbatim so the author can grep their caixa.lisp for
        // `:exe` and fix in one edit (drop the slot or change
        // `:kind Biblioteca` → `:kind Binario`).
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let lib = root.join("lib").join("demo.lisp");
        let exe_path = root.join("exe").join("tool");
        let layout = StandardLayout::new()
            .with_path_exists(move |p| p == manifest || p == lib || p == exe_path);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.exe = vec!["exe/tool".into()];
        let err = layout.verify(&c, &root).unwrap_err();
        match err {
            LayoutError::ForeignCodeSlot { caixa, kind, slots } => {
                assert_eq!(caixa, "demo");
                assert_eq!(kind, CaixaKind::Biblioteca);
                assert_eq!(slots, ":exe");
            }
            other => panic!("expected ForeignCodeSlot, got {other:?}"),
        }
    }

    #[test]
    fn biblioteca_with_servicos_rejected() {
        // Symmetric to `biblioteca_with_exe_rejected` on the
        // `:servicos` axis: a `:kind Biblioteca` declaring a Servico
        // computeunit silently passed validate and the daemon's
        // ComputeUnit / lareira chart never materialized (caixa-helm /
        // caixa-flux gate emission on `require_kind(_, Servico)`).
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let lib = root.join("lib").join("demo.lisp");
        let svc = root.join("servicos").join("demo.computeunit.yaml");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == lib || p == svc);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        let err = layout.verify(&c, &root).unwrap_err();
        match err {
            LayoutError::ForeignCodeSlot { caixa, kind, slots } => {
                assert_eq!(caixa, "demo");
                assert_eq!(kind, CaixaKind::Biblioteca);
                assert_eq!(slots, ":servicos");
            }
            other => panic!("expected ForeignCodeSlot, got {other:?}"),
        }
    }

    #[test]
    fn biblioteca_with_exe_and_servicos_lists_slots_in_canonical_order() {
        // Both foreign code slots declared on a Biblioteca → the
        // diagnostic enumerates them in canonical declaration order
        // (`:exe` → `:servicos`), deterministic across runs. Mirrors the
        // mesh/supervisor/servico-slot `*_lists_slots_in_canonical_order`
        // pins on the peer kind ↔ slot algebra axes; drift in the
        // [`Caixa::declared_foreign_code_slots`] iteration order surfaces
        // here.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Biblioteca);
        c.exe = vec!["exe/tool".into()];
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        let err = layout.verify(&c, &root).unwrap_err();
        match err {
            LayoutError::ForeignCodeSlot { slots, .. } => {
                assert_eq!(slots, ":exe :servicos");
            }
            other => panic!("expected ForeignCodeSlot, got {other:?}"),
        }
    }

    #[test]
    fn binario_with_servicos_rejected() {
        // The peer footgun on the Binario kind: declaring a Servico
        // computeunit on a `:kind Binario` caixa. The caixa-helm /
        // caixa-flux renderers gate on `require_kind(_, Servico)`, so
        // the `:servicos` slot vanishes past the layout's path-
        // existence check — no ComputeUnit, no Helm chart. `:exe` stays
        // valid (Binario's native code surface), so the kind-coherence
        // diagnostic targets only `:servicos`.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let exe_path = root.join("exe").join("tool");
        let svc = root.join("servicos").join("demo.computeunit.yaml");
        let layout = StandardLayout::new()
            .with_path_exists(move |p| p == manifest || p == exe_path || p == svc);
        let mut c = caixa(CaixaKind::Binario);
        c.exe = vec!["exe/tool".into()];
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        let err = layout.verify(&c, &root).unwrap_err();
        match err {
            LayoutError::ForeignCodeSlot { caixa, kind, slots } => {
                assert_eq!(caixa, "demo");
                assert_eq!(kind, CaixaKind::Binario);
                assert_eq!(slots, ":servicos");
            }
            other => panic!("expected ForeignCodeSlot, got {other:?}"),
        }
    }

    #[test]
    fn servico_with_exe_rejected() {
        // Symmetric to `binario_with_servicos_rejected` on the other
        // code-running peer: a `:kind Servico` declaring an `:exe` is
        // the "I added a host-side CLI to my wasm component" footgun —
        // the nix flake's Binario target gates on `require_kind(_,
        // Binario)`, so the `:exe` path vanishes past the layout's
        // path-existence check.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos").join("demo.computeunit.yaml");
        let exe_path = root.join("exe").join("tool");
        let layout = StandardLayout::new()
            .with_path_exists(move |p| p == manifest || p == svc || p == exe_path);
        let mut c = caixa(CaixaKind::Servico);
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        c.exe = vec!["exe/tool".into()];
        let err = layout.verify(&c, &root).unwrap_err();
        match err {
            LayoutError::ForeignCodeSlot { caixa, kind, slots } => {
                assert_eq!(caixa, "demo");
                assert_eq!(kind, CaixaKind::Servico);
                assert_eq!(slots, ":exe");
            }
            other => panic!("expected ForeignCodeSlot, got {other:?}"),
        }
    }

    #[test]
    fn binario_without_servicos_still_verifies() {
        // Pass-after control: a well-formed Binario carrying only its
        // native `:exe` surface must remain accepted — the gate keys off
        // declared-ness of the *foreign* slots, so it must not over-fire
        // on the legitimate same-kind case. Mirror of
        // `servico_with_servico_slots_still_verifies` on the peer axis.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let exe_path = root.join("exe").join("tool");
        let layout =
            StandardLayout::new().with_path_exists(move |p| p == manifest || p == exe_path);
        let mut c = caixa(CaixaKind::Binario);
        c.exe = vec!["exe/tool".into()];
        layout.verify(&c, &root).unwrap();
    }

    #[test]
    fn biblioteca_with_only_bibliotecas_still_verifies() {
        // Pass-after control: a well-formed Biblioteca carrying only
        // its native `:bibliotecas` surface (or the default
        // `lib/<nome>.lisp`) must remain accepted. The gate keys off
        // declared-ness of `:exe` + `:servicos` only — `:bibliotecas`
        // is deliberately excluded from the foreign-set on every
        // code-running kind (`declared_foreign_code_slots` doc), so a
        // Biblioteca with the canonical lib surface alone passes.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let lib = root.join("lib").join("demo.lisp");
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest || p == lib);
        layout
            .verify(&caixa(CaixaKind::Biblioteca), &root)
            .expect("Biblioteca with default lib must verify");
    }

    #[test]
    fn binario_with_bibliotecas_helper_still_verifies() {
        // Pass-after control on the deliberate `:bibliotecas`-as-helper
        // shape: a `:kind Binario` may legitimately bundle a `lib/`
        // helper its nix flake build consumes (the same shape a
        // `:kind Servico` may bundle for its wasm-component source).
        // The foreign-code-slot gate must NOT fire on `:bibliotecas` for
        // either code-running kind; pinned here so a future tightening
        // that adds `:bibliotecas` to the foreign set on Binario /
        // Servico surfaces as a test failure rather than as a silent
        // over-reach.
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let exe_path = root.join("exe").join("tool");
        let lib = root.join("lib").join("helper.lisp");
        let layout = StandardLayout::new()
            .with_path_exists(move |p| p == manifest || p == exe_path || p == lib);
        let mut c = caixa(CaixaKind::Binario);
        c.exe = vec!["exe/tool".into()];
        c.bibliotecas = vec!["lib/helper.lisp".into()];
        layout.verify(&c, &root).unwrap();
    }

    #[test]
    fn supervisor_with_exe_still_surfaces_owns_code() {
        // Diagnostic-precedence pin: a `:kind Supervisor` declaring
        // `:exe` is *both* "Supervisor with code" and "foreign code
        // slot". The more-fundamental `SupervisorOwnsCode` must win
        // (Supervisor doesn't run code at all — the foreign-slot
        // diagnostic would mislead the author toward changing `:kind`
        // when the underlying defect is that supervisors orchestrate
        // children, not code). Guards the call order in `verify`
        // against silent reordering.
        use crate::{ChildSpec, RestartPolicy, RestartStrategy};
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_clone = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_clone);
        let mut c = caixa(CaixaKind::Supervisor);
        c.estrategia = Some(RestartStrategy::OneForOne);
        c.max_restarts = Some(5);
        c.exe = vec!["exe/tool".into()];
        c.children = vec![ChildSpec {
            caixa: "worker".into(),
            versao: "^0.1".into(),
            restart: RestartPolicy::Permanent,
        }];
        let err = layout.verify(&c, &root).unwrap_err();
        assert!(
            matches!(err, LayoutError::SupervisorOwnsCode(_)),
            "Supervisor-with-:exe must surface as SupervisorOwnsCode (the more-fundamental \
             no-code-at-all diagnostic), got {err:?}"
        );
    }

    #[test]
    fn declared_foreign_code_slots_returns_canonical_order() {
        // Unit-level pin for the lifted method: the canonical iteration
        // order is `:exe` → `:servicos`, independent of which subset is
        // populated. Empty input + each single-slot subset + the full
        // pair are all checked so a future axis added to the method
        // (a hypothetical fifth code-surface slot) is one extension
        // point + one assertion update here, not a coordinated rewrite
        // across the layout-test sites that reach for the canonical
        // order.
        let mut c = caixa(CaixaKind::Biblioteca);
        assert!(c.declared_foreign_code_slots().is_empty());
        c.exe = vec!["exe/tool".into()];
        assert_eq!(c.declared_foreign_code_slots(), vec![":exe"]);
        c.exe.clear();
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        assert_eq!(c.declared_foreign_code_slots(), vec![":servicos"]);
        c.exe = vec!["exe/tool".into()];
        assert_eq!(c.declared_foreign_code_slots(), vec![":exe", ":servicos"]);
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
            issue.contains(crate::render::M2_UPGRADE_INSTRUCTION_KIND_LOAD_MODULE),
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
            issue.contains(crate::render::M2_UPGRADE_INSTRUCTION_KIND_SOFT_PURGE),
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
        //
        // `:soft-purge` and `:purge` target *distinct* old-version
        // modules (`hello-rio-old` and `hello-rio-oldest`) so the
        // within-entry cleanup-singularity gate
        // (`UpgradeError::DuplicateCleanup`) passes — that gate
        // rejects more than one cleanup per module per entry (one
        // semantic per old version; mixing drain + discard on one
        // module is the soft-then-hard fallback footgun the author
        // shouldn't write because the operator handles cleanup
        // failure escalation itself). The two distinct names cover
        // the legitimate "drain a recent old, hard-discard an
        // older-still" shape — both authoring forms remain load-
        // bearing in this positive-control enumeration.
        use crate::{BehaviorSpec, UpgradeFromEntry, UpgradeInstruction};
        use std::path::PathBuf;
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let svc = root.join("servicos/demo.computeunit.yaml");
        let migration = root.join("lib/migrations/v01-to-v02.lisp");
        let on_state_change = root.join("lib/migrations.lisp");
        let mut c = caixa(CaixaKind::Servico);
        // `:versao` past both entries' `:from` so the cross-slot
        // precedence gate (`FromNotBeforeVersao`) lets this canonical
        // authoring shape through to the positive-control assertion.
        c.versao = "0.2.0".into();
        c.servicos = vec!["servicos/demo.computeunit.yaml".into()];
        // `:on-state-change` declared alongside the `(:state-change …)`
        // instruction below — the cross-slot composition gate
        // (`validate_upgrade_from_against_behavior`) rejects a
        // `:state-change` without the callback, so the canonical
        // authoring shape this positive control pins now includes the
        // runtime delivery hook (the `gen_server:code_change/3` analog
        // that the per-version script is invoked through during hot
        // upgrade per the upgrade.rs module doc "Composes with"
        // promise).
        c.behavior = Some(BehaviorSpec {
            on_state_change: Some(PathBuf::from("lib/migrations.lisp")),
            ..Default::default()
        });
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
                        module: "hello-rio-oldest".into(),
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
        let on_state_change_clone = on_state_change.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| {
            p == manifest_clone
                || p == svc_clone
                || p == migration_clone
                || p == on_state_change_clone
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
            issue.contains(crate::render::M2_UPGRADE_INSTRUCTION_KIND_RESTART),
            "UpgradeViolation issue must name the `:restart` axis verbatim, got {issue:?}"
        );
        assert!(
            issue.contains(crate::render::M2_UPGRADE_INSTRUCTION_KIND_LOAD_MODULE),
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
            issue.contains("(:restart)")
                || issue.contains(crate::render::M2_UPGRADE_INSTRUCTION_KIND_RESTART),
            "UpgradeViolation issue must name the `:restart` axis verbatim, got {issue:?}"
        );
    }

    #[test]
    fn upgrade_from_invalid_surfaces_as_layout_violation() {
        // The `:from` semver gate (`UpgradeError::FromInvalid`)
        // was likewise unreachable before this wiring landed — a
        // typo-shaped `:from "v0.1.0"` (git-tag-shape leaking into
        // the semver slot) silently passed `feira build` and
        // surfaced only when the operator's hot-upgrade decision
        // engine tried to match against the version key it couldn't
        // parse. Now wired through `UpgradeViolation` with the
        // peer-shaped `{ from, reason }` payload — the
        // parser-shaped `reason` flows through `Display` so the
        // wrapped issue string carries both the offending value
        // *and* the SemVer-2 parser's wording (peer with the
        // `VersaoInvalid` / `MembroVersaoInvalid` envelopes on the
        // sibling SemVer-2 axes).
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
            issue.contains("v0.1.0"),
            "UpgradeViolation issue must name the offending :from value verbatim, got {issue:?}"
        );
        assert!(
            issue.contains(":from"),
            "UpgradeViolation issue must name the :from slot verbatim, got {issue:?}"
        );
        // Pin the parser-shaped reason flow-through: the renamed
        // `FromInvalid { from, reason }` carries the SemVer-2 parser's
        // wording verbatim, and the [`UpgradeError`] Display routes it
        // into the wrapped `issue` string so the layout envelope
        // surfaces both the offending value *and* the parser's
        // diagnosis. Mirrors the peer flow-through on
        // `ManifestError::VersaoInvalid` (top-level `:versao`) and
        // `AplicacaoError::MembroVersaoInvalid` (`:membros :versao`).
        assert!(
            issue.contains("SemVer-2"),
            "UpgradeViolation issue must carry the parser-shaped reason (\"SemVer-2\"), got {issue:?}"
        );
    }

    #[test]
    fn missing_lib_gate_routes_through_kind_requires_lib_and_caixa_nome() {
        // Fail-before-pass-after pin on the two-part converge landed
        // at layout.rs:844-847:
        //   (a) `caixa.kind().is_biblioteca()` →
        //       `caixa.kind().requires_lib()` — routes the biblioteca
        //       required-slot gate onto the same `requires_*()`
        //       predicate family the three sibling required-slot
        //       gates (`requires_exe()` at :856, `requires_servicos()`
        //       at :860, `requires_ci()` at :874) already key off.
        //       All four gates in the block now share one convention;
        //       a future kind that gains its own required-slot gate
        //       (an M4/M5 typed arm the CAIXA-SDLC §I six-kind roster
        //       may grow) reaches for the same predicate family and
        //       inherits the accessor discipline for free.
        //   (b) raw `caixa.nome` → `caixa.nome()` — routes the
        //       `expected` path composition through the typed
        //       [`crate::Caixa::nome`] accessor, closing the last
        //       unlifted raw `caixa.nome` production field-access
        //       site in `caixa-core/src/layout.rs` (every peer
        //       diagnostic in the file already routes through
        //       `caixa.nome().to_string()`).
        //
        // The behavioral pin: for a Biblioteca kind with no fallback
        // `lib/<nome>.lisp` file, MissingLib fires and its `expected`
        // path composes through `Caixa::nome()`; for every other
        // kind, MissingLib does NOT fire (the gate short-circuits on
        // kinds where `requires_lib()` returns false), even when the
        // fallback file is likewise absent. A future regression that
        // reroutes the gate off `requires_lib()` (e.g. onto
        // `is_biblioteca()` again, or onto a hand-authored
        // `matches!(caixa.kind(), CaixaKind::Biblioteca)`) that
        // *happens* to agree byte-for-byte on today's arm-set trips
        // this test the moment a future kind's `requires_lib()`
        // returns true for a non-`Biblioteca` arm (or the sibling
        // required-slot gates diverge from the same convention).
        let root = PathBuf::from("/tmp/x");
        let manifest = root.join("caixa.lisp");
        let manifest_only = manifest.clone();
        let layout = StandardLayout::new().with_path_exists(move |p| p == manifest_only);

        // Biblioteca kind + no lib fallback → MissingLib fires with
        // the expected path composed through `Caixa::nome()`.
        let bib = caixa(CaixaKind::Biblioteca);
        assert!(
            bib.kind().requires_lib(),
            "requires_lib() must return true for Biblioteca — the four-required-\
             slot-gate family's routing depends on this arm's assignment"
        );
        let err = layout.verify(&bib, &root).unwrap_err();
        let LayoutError::MissingLib {
            caixa: cname,
            expected,
        } = err
        else {
            panic!("expected MissingLib for Biblioteca kind with no lib fallback, got {err:?}");
        };
        assert_eq!(
            cname,
            bib.nome(),
            "MissingLib `caixa:` carrier must byte-equal Caixa::nome()"
        );
        assert_eq!(
            expected,
            root.join(crate::render::LAYOUT_DIR_LIB)
                .join(format!("{}.lisp", bib.nome())),
            "MissingLib `expected:` path must compose through Caixa::nome() \
             verbatim — a raw-field-access regression would silently drift \
             the composed path on any future `:nome` axis extension \
             (namespace-qualified rewrite, per-cluster alias overlay)"
        );

        // Non-Biblioteca kinds → the MissingLib gate short-circuits.
        // Different kinds fail on their own required-slot gate
        // (BinarioWithoutExe, ServicoWithoutServicos, MissingCi) or
        // on downstream M2/M3 invariants; none of them may surface as
        // MissingLib, because `requires_lib()` returns false for each.
        for kind in [
            CaixaKind::Binario,
            CaixaKind::Servico,
            CaixaKind::Supervisor,
            CaixaKind::Aplicacao,
            CaixaKind::Acao,
        ] {
            assert!(
                !kind.requires_lib(),
                "requires_lib() must return false for {kind:?} — the \
                 four-required-slot-gate family's arm assignment pins \
                 exactly one kind (Biblioteca) as the arm that requires \
                 a `lib/` entry"
            );
            let c = caixa(kind);
            let result = layout.verify(&c, &root);
            assert!(
                !matches!(result, Err(LayoutError::MissingLib { .. })),
                "MissingLib gate at layout.rs:844 must short-circuit for \
                 kinds where requires_lib() returns false; unexpectedly \
                 fired for {kind:?}: {result:?}"
            );
        }
    }
}
