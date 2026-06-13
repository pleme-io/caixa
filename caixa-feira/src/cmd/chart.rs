use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use caixa_core::{Caixa, CaixaKind};
use clap::Args;

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
        let root = self.path.clone().unwrap_or_else(|| PathBuf::from("."));
        let manifest_path = root.join("caixa.lisp");
        let src = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        let caixa = Caixa::from_lisp(&src)
            .with_context(|| format!("parsing {}", manifest_path.display()))?;

        let cu_path = first_servico_path(&caixa, &root)?;
        let cu_src = std::fs::read_to_string(&cu_path)
            .with_context(|| format!("reading {}", cu_path.display()))?;
        let cu_yaml: serde_yaml::Value = serde_yaml::from_str(&cu_src)
            .with_context(|| format!("parsing {}", cu_path.display()))?;

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
    // After `require_single_servico` guarantees `caixa.servicos.len() == 1`,
    // `.first()` is infallible; the V0 invariant is now a structural
    // property of every verb-entry-point on the per-Servico axis.
    let s = caixa
        .servicos
        .first()
        .expect("require_single_servico guarantees one entry");
    let p = root.join(s);
    if !p.exists() {
        bail!(
            "caixa {:?}: declared :servicos entry not found on disk: {}",
            caixa.nome,
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
}
