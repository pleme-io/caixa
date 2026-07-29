use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use caixa_core::{Caixa, CaixaKind};
use clap::Args;

use super::load::{caixa_root, load_caixa};

/// Render the per-program lareira-<name> Helm chart for the caixa Servico
/// in CWD.
///
/// Reads:
///   ./caixa.lisp
///   ./servicos/<name>.computeunit.yaml   (first servicos[] entry)
///
/// Writes:
///   <out>/lareira-<name>/Chart.yaml
///   <out>/lareira-<name>/values.yaml
///   <out>/lareira-<name>/README.md
///
/// The chart is "thin" by design — it depends on the
/// `pleme-computeunit` library chart in helmworks, which owns the K8s
/// templates. caixa-helm only wires the metadata + values block.
#[derive(Args)]
pub struct Chart {
    /// Where to write the chart directory. Default: `./.caixa/chart`.
    #[arg(long, default_value = ".caixa/chart")]
    pub out: PathBuf,

    /// caixa root (defaults to CWD).
    #[arg(long)]
    pub path: Option<PathBuf>,
}

impl Chart {
    pub fn run(self) -> Result<()> {
        let root = caixa_root(self.path.as_deref());
        let caixa = load_caixa(&root)?;

        let cu_yaml = load_first_servico_yaml(&caixa, &root)?;

        let dir = caixa_helm::render_chart_for_servico(&caixa, &cu_yaml)?;
        dir.write_to(&self.out)
            .with_context(|| format!("writing chart to {}", self.out.display()))?;

        eprintln!(
            "rendered {} → {}",
            dir.name,
            self.out.join(&dir.name).display()
        );
        Ok(())
    }
}

/// Read + parse the single `:servicos` computeunit YAML for a
/// per-Servico `feira` verb (`feira chart` + `feira deploy`).
///
/// Closes the PRIME DIRECTIVE duplication (THEORY.md §I.3.5) on the
/// per-Servico computeunit IO axis. Before this lift both verbs
/// open-coded the same four-line shape — [`first_servico_path`] →
/// [`std::fs::read_to_string`] → [`serde_yaml::from_str`], all with
/// verbatim `"reading {…}"` / `"parsing {…}"` `.with_context(...)`
/// strings — at every entry-point that consumes the per-Servico
/// `servicos/<name>.computeunit.yaml`. Two inline copies of the same
/// load-bearing diagnostic shape, each one another place a future
/// change to the `"reading {…}"` / `"parsing {…}"` discipline has to
/// remember to touch.
///
/// After the lift every per-Servico-verb entry-point routes through
/// this helper; the `"reading {…}"` / `"parsing {…}"` context strings
/// live at exactly one call-site, the underlying `serde_yaml::Error`
/// remains reachable on the anyhow cause chain for typed-view
/// downstream callers (peer with the per-renderer
/// [`caixa_core::KindMismatch`] / [`caixa_core::ServicoCountMismatch`]
/// downcast discipline), and a future per-Servico-verb consumer (the
/// future `feira oci publish` per-Servico OCI packager, the future
/// `feira validate --strict` per-caixa admission verb) inherits the
/// canonical entry-point shape rather than re-derives a skewed copy.
///
/// Mirrors [`super::load::load_caixa`] on the peer
/// `<root>/caixa.lisp` axis — the canonical per-verb IO entry-point
/// the db76969 lift established. The two helpers compose the
/// per-Servico verb's IO entry-point in two canonical calls (the
/// manifest loader + this per-computeunit loader); every diagnostic
/// names the offending path with the same `"reading …"` /
/// `"parsing …"` discipline, and the [`first_servico_path`] gate's
/// typed-view chain (kind + count + file-existence) remains
/// reachable through the anyhow chain for downstream structured-output
/// consumers.
pub(crate) fn load_first_servico_yaml(
    caixa: &Caixa,
    root: &std::path::Path,
) -> Result<serde_yaml::Value> {
    let cu_path = first_servico_path(caixa, root)?;
    super::load::load_yaml(&cu_path)
}

/// Resolve the single `:servicos` entry's on-disk path for a per-Servico
/// `feira` verb (`feira chart` + `feira deploy`), gating the V0
/// Servico-shape contract at the verb entry-point through the canonical
/// lifted predicates in `caixa-core::render`.
///
/// The gate pair runs at the verb's entry-point — peer with the
/// `feira app` verb-gate (`load_aplicacao`, 9e8b444) and with every
/// per-Servico renderer's entry-point (`caixa-helm`'s
/// `render_chart_for_servico_with`, plus `caixa-flux`'s
/// `programs_yaml_entry` and `cluster_bundle`, all wired through the
/// same lifted helpers by c4213a4 / 06b2981 / 1548fd2). Before this
/// lift the helper called `.servicos.first().ok_or_else(...)` with a
/// `"caixa.lisp has no :servicos entry"` Display that named neither
/// the offending caixa's `:nome` nor the `:kind` axis it actually
/// violated; a `:kind Biblioteca` or `:kind Aplicacao` caixa fed to
/// `feira chart` / `feira deploy` raised the same generic "no
/// servicos" string rather than the canonical
/// [`KindMismatch`][caixa_core::KindMismatch] shape the renderer
/// entry-points already surface, and a `:kind Servico` caixa with 2+
/// `:servicos` entries silently picked the first and deferred the
/// V0-count violation to the downstream renderer.
///
/// After the lift both axes route through `caixa_core::require_kind`
/// alongside `caixa_core::require_single_servico`, so every
/// diagnostic on the verb's entry-point path names the offending
/// caixa verbatim — peer with the renderer entry-points the same
/// lifted helpers already wired. The remaining file-existence check
/// carries the caixa's `:nome` so the "declared servicos entry not
/// found" diagnostic also names the offending caixa rather than only
/// the missing path.
pub(crate) fn first_servico_path(caixa: &Caixa, root: &std::path::Path) -> Result<PathBuf> {
    caixa_core::require_kind(caixa, CaixaKind::Servico)
        .with_context(|| "feira chart / feira deploy require :kind Servico")?;
    caixa_core::require_single_servico(caixa)
        .with_context(|| "feira chart / feira deploy require exactly one :servicos entry")?;
    // After `require_single_servico` guarantees `caixa.servicos().len() == 1`,
    // `.first()` is infallible; the V0 invariant is now a structural
    // property of every verb-entry-point on the per-Servico axis.
    let s = caixa
        .servicos()
        .first()
        .expect("require_single_servico guarantees one entry");
    let p = root.join(s);
    if !p.exists() {
        bail!(
            "caixa {:?}: declared :servicos entry not found on disk: {}",
            caixa.nome(),
            p.display()
        );
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn parse(src: &str) -> Caixa {
        Caixa::from_lisp(src).expect("parse")
    }

    #[test]
    fn first_servico_path_accepts_singleton_servico_kind() {
        // The canonical happy-path — a `:kind Servico` caixa with
        // exactly one `:servicos` entry whose file is present on
        // disk passes both lifted gates + the file-existence check.
        // Peer with the per-renderer `render_chart_for_servico_with` /
        // `programs_yaml_entry` happy-paths the same lifted helpers
        // pin.
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("servicos")).expect("mkdir servicos");
        std::fs::write(
            dir.path().join("servicos/hello.computeunit.yaml"),
            "name: hello\n",
        )
        .expect("write cu");
        let caixa = parse(
            r#"(defcaixa
                 :nome "hello"
                 :kind Servico
                 :versao "0.1.0"
                 :servicos ("servicos/hello.computeunit.yaml"))"#,
        );
        let p = first_servico_path(&caixa, dir.path()).expect("singleton must pass");
        assert!(p.ends_with("servicos/hello.computeunit.yaml"));
    }

    #[test]
    fn first_servico_path_rejects_non_servico_kind_with_named_caixa() {
        // The load-bearing property the lift closes on the
        // `:kind` axis: a `:kind Biblioteca` caixa fed to a
        // per-Servico verb surfaces a diagnostic whose anyhow
        // cause chain *names the offending caixa*, peer with the
        // 9e8b444 `feira app` lift on the sibling Aplicacao-verb
        // axis. Before the lift this helper raised the generic
        // "caixa.lisp has no :servicos entry" string — the
        // operator had to grep their source tree for which
        // caixa.lisp triggered it, and worse: the diagnostic
        // misnamed the violated axis (the failure was a `:kind`
        // mismatch, not a missing `:servicos`).
        let dir = tempdir().expect("tempdir");
        let caixa = parse(
            r#"(defcaixa
                 :nome "lib-shape"
                 :kind Biblioteca
                 :versao "0.1.0"
                 :bibliotecas ())"#,
        );
        let err = first_servico_path(&caixa, dir.path()).expect_err("Biblioteca must reject");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("lib-shape"),
            "diagnostic must name the offending caixa nome (got: {rendered:?})"
        );
        assert!(
            rendered.contains("Servico"),
            "diagnostic must name the expected kind (got: {rendered:?})"
        );
        assert!(
            rendered.contains("Biblioteca"),
            "diagnostic must name the actual kind (got: {rendered:?})"
        );
        assert!(
            rendered.contains("feira chart") || rendered.contains("feira deploy"),
            "diagnostic must keep the per-Servico verb context (got: {rendered:?})"
        );
    }

    #[test]
    fn first_servico_path_kind_rejection_carries_typed_view() {
        // Peer to the [`KindMismatch`]-named-caixa pin above: the
        // anyhow error chain's underlying source downcasts to
        // [`caixa_core::KindMismatch`], so a future caller that
        // inspects the chain reads the typed view's `nome` /
        // `expected` / `actual` slots verbatim — peer with the
        // `caixa-helm` / `caixa-flux` / `caixa-mesh`
        // `#[from] KindMismatch` test families and with the
        // 9e8b444 `feira app` verb-gate test.
        let dir = tempdir().expect("tempdir");
        let caixa = parse(
            r#"(defcaixa
                 :nome "app-shape"
                 :kind Aplicacao
                 :versao "0.1.0"
                 :membros ())"#,
        );
        let err = first_servico_path(&caixa, dir.path()).expect_err("Aplicacao must reject");
        let km = err
            .chain()
            .find_map(|e| e.downcast_ref::<caixa_core::KindMismatch>())
            .expect("KindMismatch must be reachable through the anyhow chain");
        assert_eq!(km.nome, "app-shape");
        assert_eq!(km.expected, CaixaKind::Servico);
        assert_eq!(km.actual, CaixaKind::Aplicacao);
    }

    #[test]
    fn first_servico_path_rejects_empty_servicos_with_named_caixa() {
        // The V0 `:servicos`-singularity axis at the verb
        // entry-point: a `:kind Servico` caixa with an empty
        // `:servicos` list surfaces the canonical
        // [`ServicoCountMismatch`] view (peer with the
        // 06b2981 lift on the per-renderer-entry-point axis),
        // *not* the prior generic "caixa.lisp has no :servicos
        // entry" string. The lifted typed view carries the
        // offending caixa's `:nome` so the diagnostic names
        // which `caixa.lisp` needs author attention.
        let dir = tempdir().expect("tempdir");
        let caixa = parse(
            r#"(defcaixa
                 :nome "no-servicos"
                 :kind Servico
                 :versao "0.1.0"
                 :servicos ())"#,
        );
        let err = first_servico_path(&caixa, dir.path()).expect_err("empty :servicos must reject");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("no-servicos"),
            "diagnostic must name the offending caixa nome (got: {rendered:?})"
        );
        assert!(
            rendered.contains(":servicos"),
            "diagnostic must name the :servicos axis (got: {rendered:?})"
        );
        let scm = err
            .chain()
            .find_map(|e| e.downcast_ref::<caixa_core::ServicoCountMismatch>())
            .expect("ServicoCountMismatch must be reachable through the chain");
        assert_eq!(scm.nome, "no-servicos");
        assert_eq!(scm.count, 0);
    }

    #[test]
    fn first_servico_path_rejects_multi_entry_servicos_with_named_caixa() {
        // The peer arm of the V0 invariant: a `:kind Servico`
        // caixa with 2+ `:servicos` entries fails at the verb
        // entry-point with the same lifted typed view, rather
        // than silently picking `.first()` and deferring the V0
        // violation to the downstream renderer's gate (the
        // canonical "the V0 invariant is enforced at every
        // per-Servico entry-point except this one" footgun the
        // 1548fd2 lift closed on the `cluster_bundle` axis).
        let dir = tempdir().expect("tempdir");
        let caixa = parse(
            r#"(defcaixa
                 :nome "two-servicos"
                 :kind Servico
                 :versao "0.1.0"
                 :servicos ("servicos/a.computeunit.yaml"
                            "servicos/b.computeunit.yaml"))"#,
        );
        let err = first_servico_path(&caixa, dir.path()).expect_err("multi :servicos must reject");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("two-servicos"),
            "diagnostic must name the offending caixa nome (got: {rendered:?})"
        );
        let scm = err
            .chain()
            .find_map(|e| e.downcast_ref::<caixa_core::ServicoCountMismatch>())
            .expect("ServicoCountMismatch must be reachable through the chain");
        assert_eq!(scm.nome, "two-servicos");
        assert_eq!(scm.count, 2);
    }

    #[test]
    fn first_servico_path_missing_file_diagnostic_routes_through_caixa_nome_accessor() {
        // Fail-before-pass-after pin: the `first_servico_path` helper's
        // file-existence `bail!` diagnostic's terminal `{:?}` `:nome`
        // scalar must derive through the typed
        // [`caixa_core::Caixa::nome`] accessor byte-for-byte. Before
        // this converge the emit site at :144 carried a raw
        // `caixa.nome` field-access into the inline `bail!(...)`
        // template, bypassing the typed dispatch every peer `feira`
        // verb's `:nome` emit-site already routes through (ef83332
        // `feira build`, 3219a42 `feira app`, 4b05240 `feira deploy`).
        // This helper sits shared between `feira chart` and
        // `feira deploy` (both verbs route their per-Servico
        // entry-point through it), so a re-inlined raw field-access
        // here silently splits the offending-caixa diagnostic each
        // verb surfaces from the byte-string the paired downstream
        // artefacts already carry under the accessor — with the
        // failure surfacing as an operator hitting the file-existence
        // gate on either verb and reading a `:nome` scalar that
        // disagrees with the `Chart.yaml` / `programs.yaml` emit at
        // the same caixa's downstream artefact site far from the
        // regression's source.
        //
        // Byte-equal today (both paths return the same String bytes);
        // the pin catches any future accessor extension (a SemVer-2
        // build-metadata canonicalization on `Caixa::nome`, a per-
        // edition per-`:nome` overlay dispatch through the sibling
        // [`caixa_core::Caixa::edicao`] axis) whose `first_servico_path`
        // diagnostic regresses to the raw field and silently splits
        // the offending-caixa scalar from the paired downstream
        // accessor-derived emits. Peer with the sibling
        // [`super::build::tests::build_summary_line_routes_through_caixa_nome_and_versao_accessors`] /
        // [`super::deploy::tests::deploy_summary_line_routes_through_caixa_nome_and_versao_accessors`] /
        // [`super::app::tests::graph_header_line_routes_through_caixa_nome_and_versao_accessors`]
        // pins on the peer `feira build` / `feira deploy` / `feira app`
        // verb-emit surfaces.
        let dir = tempdir().expect("tempdir");
        let caixa = parse(
            r#"(defcaixa
                 :nome "missing-file-nome"
                 :kind Servico
                 :versao "0.1.0"
                 :servicos ("servicos/missing-file-nome.computeunit.yaml"))"#,
        );
        let err = first_servico_path(&caixa, dir.path()).expect_err("missing file must reject");
        let missing_path = dir
            .path()
            .join("servicos/missing-file-nome.computeunit.yaml");
        assert_eq!(
            format!("{err}"),
            format!(
                "caixa {:?}: declared :servicos entry not found on disk: {}",
                caixa.nome(),
                missing_path.display()
            ),
            "first_servico_path's file-existence bail! diagnostic must \
             derive its :nome scalar through the typed Caixa::nome \
             accessor — a regression that re-inlines the raw caixa.nome \
             field-access at the emit site silently splits the offending-\
             caixa diagnostic both `feira chart` and `feira deploy` route \
             through this helper from the peer accessor-derived \
             substrate-side emit"
        );
    }

    #[test]
    fn first_servico_path_missing_file_diagnostic_names_offending_caixa_nome() {
        // The remaining file-existence check carries the caixa's
        // `:nome` — peer with the lifted typed-view diagnostics
        // on the kind + count axes above. Before this lift the
        // helper raised `"declared servicos entry not found:
        // <path>"` naming only the missing path; the operator hit
        // the gate at `feira chart` / `feira deploy` time without
        // a direct pointer to which `caixa.lisp` declared the
        // unresolved entry.
        let dir = tempdir().expect("tempdir");
        let caixa = parse(
            r#"(defcaixa
                 :nome "absent-file"
                 :kind Servico
                 :versao "0.1.0"
                 :servicos ("servicos/absent-file.computeunit.yaml"))"#,
        );
        let err = first_servico_path(&caixa, dir.path()).expect_err("missing file must reject");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("absent-file"),
            "diagnostic must name the offending caixa nome (got: {rendered:?})"
        );
        assert!(
            rendered.contains("not found"),
            "diagnostic must name the file-existence axis (got: {rendered:?})"
        );
    }

    #[test]
    fn load_first_servico_yaml_accepts_well_formed_computeunit() {
        // The canonical happy-path — a `:kind Servico` caixa with one
        // `:servicos` entry whose file is on disk and parses as YAML
        // returns the typed `serde_yaml::Value`, peer with every
        // per-Servico verb's `Run::run` entry-point: each one used to
        // open-code the same four-line `first_servico_path` → read →
        // parse shape before this lift, and now routes through the
        // canonical helper.
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("servicos")).expect("mkdir servicos");
        std::fs::write(
            dir.path().join("servicos/hello.computeunit.yaml"),
            "name: hello\nimage: ghcr.io/pleme-io/hello:0.1.0\n",
        )
        .expect("write cu");
        let caixa = parse(
            r#"(defcaixa
                 :nome "hello"
                 :kind Servico
                 :versao "0.1.0"
                 :servicos ("servicos/hello.computeunit.yaml"))"#,
        );
        let cu_yaml = load_first_servico_yaml(&caixa, dir.path()).expect("well-formed must load");
        assert_eq!(
            cu_yaml.get("name").and_then(serde_yaml::Value::as_str),
            Some("hello"),
            "parsed YAML must round-trip the canonical top-level scalar"
        );
    }

    #[test]
    fn load_first_servico_yaml_parse_error_diagnostic_names_path_with_parsing_context() {
        // The diagnostic-shape pin on the parse-failure arm: a
        // malformed computeunit YAML surfaces the verbatim
        // `"parsing <path>"` `.with_context(...)` string with the
        // resolved `servicos/<name>.computeunit.yaml` path, so a
        // future refactor of the helper can't silently regress to a
        // different context-string shape every per-Servico-verb
        // consumer relied on before the lift. Peer with the
        // `super::load::load_caixa_parse_error_diagnostic_names_path_
        // with_parsing_context` pin on the sibling manifest-IO axis.
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("servicos")).expect("mkdir servicos");
        std::fs::write(
            dir.path().join("servicos/broken.computeunit.yaml"),
            "name: broken\n  bad-indent: [unclosed\n",
        )
        .expect("write malformed cu");
        let caixa = parse(
            r#"(defcaixa
                 :nome "broken"
                 :kind Servico
                 :versao "0.1.0"
                 :servicos ("servicos/broken.computeunit.yaml"))"#,
        );
        let err =
            load_first_servico_yaml(&caixa, dir.path()).expect_err("malformed YAML must reject");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("parsing"),
            "diagnostic must name the parsing axis (got: {rendered:?})"
        );
        assert!(
            rendered.contains("broken.computeunit.yaml"),
            "diagnostic must name the offending computeunit path (got: {rendered:?})"
        );
    }

    #[test]
    fn load_first_servico_yaml_parse_error_preserves_underlying_serde_yaml_error_on_chain() {
        // Peer to the parse-context pin above: the anyhow
        // `.with_context(...)` wrap preserves the underlying
        // `serde_yaml::Error` on the cause chain — a future
        // structured-output mode (`feira chart --json`, a future
        // `feira validate --strict` per-Servico admission verb) can
        // downcast through the chain and read the typed payload
        // directly, peer with the per-renderer
        // [`caixa_core::KindMismatch`] /
        // [`caixa_core::ServicoCountMismatch`] typed-view downcast
        // discipline the per-renderer + per-verb entry-points already
        // follow, and peer with the `super::load::load_caixa_parse_
        // error_preserves_underlying_lisp_error_on_chain` pin on the
        // sibling manifest-IO axis.
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("servicos")).expect("mkdir servicos");
        std::fs::write(
            dir.path().join("servicos/torn.computeunit.yaml"),
            "name: torn\n  bad-indent: [unclosed\n",
        )
        .expect("write malformed cu");
        let caixa = parse(
            r#"(defcaixa
                 :nome "torn"
                 :kind Servico
                 :versao "0.1.0"
                 :servicos ("servicos/torn.computeunit.yaml"))"#,
        );
        let err =
            load_first_servico_yaml(&caixa, dir.path()).expect_err("malformed YAML must reject");
        let typed_reachable = err
            .chain()
            .any(|e| e.downcast_ref::<serde_yaml::Error>().is_some());
        assert!(
            typed_reachable,
            "underlying serde_yaml::Error must remain reachable on the anyhow chain"
        );
    }

    #[test]
    fn load_first_servico_yaml_routes_kind_mismatch_through_first_servico_path() {
        // The lifted helper composes `first_servico_path` on its
        // entry-point path — every typed-view diagnostic the gate
        // surfaces (kind mismatch, V0 `:servicos` count mismatch,
        // file-existence) remains reachable through this helper's
        // anyhow chain, so the canonical per-Servico verb-gate
        // discipline (864f761 / 06b2981 / 1548fd2) is preserved by
        // construction. This pin closes the regression vector where
        // a future refactor of the helper might bypass the lifted
        // gate and read the file directly, silently demoting a
        // typed `KindMismatch` to a generic parse failure.
        let dir = tempdir().expect("tempdir");
        let caixa = parse(
            r#"(defcaixa
                 :nome "lib-shape"
                 :kind Biblioteca
                 :versao "0.1.0"
                 :bibliotecas ())"#,
        );
        let err = load_first_servico_yaml(&caixa, dir.path()).expect_err("Biblioteca must reject");
        let km = err
            .chain()
            .find_map(|e| e.downcast_ref::<caixa_core::KindMismatch>())
            .expect("KindMismatch must remain reachable through the helper's anyhow chain");
        assert_eq!(km.nome, "lib-shape");
        assert_eq!(km.expected, CaixaKind::Servico);
        assert_eq!(km.actual, CaixaKind::Biblioteca);
    }
}
