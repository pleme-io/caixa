//! Shared per-verb IO entry-point for the `<root>/caixa.lisp` manifest.
//!
//! Closes the PRIME DIRECTIVE duplication on the verb-entry IO axis the
//! `feira` CLI carries. Before this lift the same four-line shape —
//! `root.join("caixa.lisp")` → `std::fs::read_to_string` →
//! `Caixa::from_lisp`, all with verbatim `"reading {…}"` and
//! `"parsing {…}"` `.with_context(...)` strings — appeared at every
//! `feira` verb's `Run::run` entry-point: `feira add` (cmd/add.rs),
//! `feira build` (cmd/build.rs), `feira chart` (cmd/chart.rs),
//! `feira deploy` (cmd/deploy.rs), `feira lock` (cmd/lock.rs),
//! `feira nix` (cmd/nix.rs), `feira publish` (cmd/publish.rs),
//! `feira resolve` (cmd/resolve.rs), and the `feira app` verbs'
//! shared `load_aplicacao` helper (cmd/app.rs). Nine inline copies of
//! the same load-bearing diagnostic shape, each one another place a
//! future change to the `"reading {…}"` / `"parsing {…}"` discipline
//! has to remember to touch.
//!
//! After the lift every per-verb entry-point routes through
//! [`load_caixa`]: the manifest-filename literal lives at exactly one
//! `pub(crate) const CAIXA_MANIFEST_FILENAME` definition, the
//! `read_to_string` and `Caixa::from_lisp` calls are wrapped with the
//! canonical context strings at exactly one call-site, and the
//! underlying `tatara_lisp::LispError` remains reachable on the
//! anyhow cause chain for typed-view downstream callers (peer with
//! the per-renderer typed-view discipline `KindMismatch`,
//! `ServicoCountMismatch`, etc. follow). A future per-caixa-root
//! `feira` verb — the future `feira oci publish` per-Servico OCI
//! packager, the future `feira validate --strict` per-caixa
//! admission verb, the future M4 `feira reconcile` per-cluster
//! diff verb the absorption-roadmap acknowledges — inherits the
//! canonical entry-point shape through this helper rather than
//! re-derives a skewed copy.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use caixa_core::Caixa;

/// The canonical caixa manifest filename. Every `feira` verb's
/// per-caixa-root entry-point resolves `<root>/caixa.lisp` through
/// [`caixa_manifest_path`] and reaches for the manifest through
/// [`load_caixa`]; no verb should hard-code the literal string.
pub(crate) const CAIXA_MANIFEST_FILENAME: &str = "caixa.lisp";

/// Resolve the per-caixa-root manifest path.
pub(crate) fn caixa_manifest_path(root: &Path) -> PathBuf {
    root.join(CAIXA_MANIFEST_FILENAME)
}

/// Read + parse the per-caixa-root `caixa.lisp` into a typed
/// [`Caixa`].
///
/// The canonical per-verb IO entry-point — every `feira` verb that
/// consumes a typed `Caixa` routes through this helper, so the
/// `"reading {…}"` / `"parsing {…}"` context strings stay verbatim
/// across every verb's rendered diagnostic. The underlying
/// `tatara_lisp::LispError` (the `Caixa::from_lisp` error type) is
/// preserved through the anyhow `.with_context(...)` wrap and remains
/// reachable on the cause chain — peer with the per-renderer
/// typed-view discipline (`KindMismatch`, `ServicoCountMismatch`,
/// etc.) the substrate's per-Servico / per-Aplicacao entry-points
/// already follow.
pub(crate) fn load_caixa(root: &Path) -> Result<Caixa> {
    let manifest = caixa_manifest_path(root);
    let src = std::fs::read_to_string(&manifest)
        .with_context(|| format!("reading {}", manifest.display()))?;
    Caixa::from_lisp(&src).with_context(|| format!("parsing {}", manifest.display()))
}

/// Read + parse a YAML file at `path` into an untyped
/// [`serde_yaml::Value`].
///
/// Closes the PRIME DIRECTIVE duplication on the per-verb YAML IO
/// axis. Before this lift the same four-line shape —
/// [`std::fs::read_to_string`] + [`serde_yaml::from_str`], both with
/// verbatim `"reading {…}"` / `"parsing {…}"` `.with_context(...)`
/// strings — appeared at every `feira` verb that consumes an
/// arbitrary YAML file: the per-Servico computeunit loader (`feira
/// chart` + `feira deploy` via [`super::chart::load_first_servico_yaml`])
/// and the cluster-side fleet-programs HelmRelease loader (`feira
/// deploy` itself). Two inline copies of the same load-bearing
/// diagnostic shape, each one another place a future change to the
/// `"reading {…}"` / `"parsing {…}"` discipline has to remember to
/// touch, and another place a future per-YAML `feira` verb (the
/// future `feira validate --strict` per-caixa admission verb, the
/// future M4 `feira reconcile` cluster-diff verb the
/// absorption-roadmap acknowledges) would re-derive a skewed copy.
///
/// After the lift every per-YAML `feira` verb routes through this
/// helper: the `"reading {…}"` / `"parsing {…}"` context strings live
/// at exactly one call-site, the underlying `serde_yaml::Error`
/// remains reachable on the anyhow cause chain (peer with the
/// per-renderer [`caixa_core::KindMismatch`] /
/// [`caixa_core::ServicoCountMismatch`] downcast discipline), and the
/// canonical entry-point shape is a single helper rather than two
/// independently-evolved copies. Mirrors [`load_caixa`] on the peer
/// `<root>/caixa.lisp` axis, completing the canonical per-verb IO
/// surface (the manifest loader + this YAML loader).
pub(crate) fn load_yaml(path: &Path) -> Result<serde_yaml::Value> {
    let src =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_yaml::from_str(&src).with_context(|| format!("parsing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use caixa_core::CaixaKind;
    use tempfile::tempdir;

    #[test]
    fn caixa_manifest_path_resolves_root_join_canonical_filename() {
        // The shape every per-verb call-site relied on (verbatim
        // `root.join("caixa.lisp")`) is now a single canonical
        // resolver. Pins that the path lands at `<root>/caixa.lisp`
        // and that the canonical manifest-filename constant stays
        // load-bearing for any future `feira` verb whose entry-point
        // joins the manifest by path.
        let root = PathBuf::from("/tmp/some-caixa");
        let p = caixa_manifest_path(&root);
        assert!(p.starts_with(&root), "manifest path must live under root");
        assert!(
            p.ends_with(CAIXA_MANIFEST_FILENAME),
            "manifest path must end with the canonical filename"
        );
        assert_eq!(p.file_name().and_then(|s| s.to_str()), Some("caixa.lisp"));
    }

    #[test]
    fn load_caixa_accepts_well_formed_manifest() {
        // The canonical happy-path — a well-formed `caixa.lisp` at
        // `<root>/caixa.lisp` loads cleanly and the typed Caixa
        // round-trips its `:nome` + `:kind` slots. Peer with every
        // `feira` verb's `Run::run` entry-point: each one used to
        // open-code this four-line load before the lift, and now
        // routes through the canonical helper.
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join(CAIXA_MANIFEST_FILENAME),
            r#"(defcaixa
                 :nome "hello"
                 :kind Biblioteca
                 :versao "0.1.0"
                 :bibliotecas ())"#,
        )
        .expect("write manifest");
        let caixa = load_caixa(dir.path()).expect("well-formed manifest must load");
        assert_eq!(caixa.nome, "hello");
        assert_eq!(caixa.kind, CaixaKind::Biblioteca);
        assert_eq!(caixa.versao, "0.1.0");
    }

    #[test]
    fn load_caixa_missing_manifest_diagnostic_names_path_with_reading_context() {
        // The diagnostic shape every per-verb consumer pins: the
        // `.with_context(|| format!("reading {}", manifest.display()))`
        // wrap renders the verbatim `"reading <path>"` string with the
        // resolved `caixa.lisp` path on missing-file failures. Before
        // this lift the same string appeared verbatim at every verb's
        // call-site — a future refactor of the helper can't silently
        // regress to a different context-string shape without this
        // pin firing.
        let dir = tempdir().expect("tempdir");
        let err = load_caixa(dir.path()).expect_err("missing manifest must error");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("reading"),
            "diagnostic must name the reading axis (got: {rendered:?})"
        );
        assert!(
            rendered.contains("caixa.lisp"),
            "diagnostic must name the canonical manifest filename (got: {rendered:?})"
        );
    }

    #[test]
    fn load_caixa_parse_error_diagnostic_names_path_with_parsing_context() {
        // The peer arm on the parse-failure axis: a malformed manifest
        // surfaces a diagnostic whose `.with_context` wrap names the
        // `"parsing <path>"` axis verbatim. Pins the canonical
        // context-string discipline on the parse-failure path so a
        // future refactor can't silently regress it independent of
        // the read-failure path.
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join(CAIXA_MANIFEST_FILENAME),
            "this is not a defcaixa form at all",
        )
        .expect("write malformed manifest");
        let err = load_caixa(dir.path()).expect_err("malformed manifest must error");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("parsing"),
            "diagnostic must name the parsing axis (got: {rendered:?})"
        );
        assert!(
            rendered.contains("caixa.lisp"),
            "diagnostic must name the canonical manifest filename (got: {rendered:?})"
        );
    }

    #[test]
    fn load_yaml_accepts_well_formed_document() {
        // The canonical happy-path — a well-formed YAML document loads
        // cleanly and the parsed [`serde_yaml::Value`] round-trips its
        // canonical top-level scalar. Peer with every per-verb
        // YAML-consuming entry-point: the per-Servico computeunit
        // loader (`super::chart::load_first_servico_yaml`) and the
        // per-cluster fleet-programs HelmRelease loader (`feira
        // deploy`'s `Run::run`) used to open-code the same four-line
        // read+parse shape before this lift, and now route through
        // this canonical helper.
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("hello.yaml");
        std::fs::write(&p, "name: hello\nport: 8080\n").expect("write yaml");
        let value = load_yaml(&p).expect("well-formed yaml must load");
        assert_eq!(
            value.get("name").and_then(serde_yaml::Value::as_str),
            Some("hello"),
            "parsed yaml must round-trip the canonical top-level scalar"
        );
    }

    #[test]
    fn load_yaml_missing_file_diagnostic_names_path_with_reading_context() {
        // The diagnostic shape every per-verb consumer pins: the
        // `.with_context(|| format!("reading {}", path.display()))`
        // wrap renders the verbatim `"reading <path>"` string with
        // the resolved path on missing-file failures. Before this
        // lift the same string appeared verbatim at every per-verb
        // YAML call-site — a future refactor of the helper can't
        // silently regress to a different context-string shape
        // without this pin firing. Peer with the
        // `load_caixa_missing_manifest_diagnostic_names_path_with_
        // reading_context` pin on the sibling manifest-IO axis.
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("absent.yaml");
        let err = load_yaml(&p).expect_err("missing yaml must error");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("reading"),
            "diagnostic must name the reading axis (got: {rendered:?})"
        );
        assert!(
            rendered.contains("absent.yaml"),
            "diagnostic must name the offending path (got: {rendered:?})"
        );
    }

    #[test]
    fn load_yaml_parse_error_diagnostic_names_path_with_parsing_context() {
        // The peer arm on the parse-failure axis: a malformed YAML
        // surfaces a diagnostic whose `.with_context` wrap names the
        // `"parsing <path>"` axis verbatim. Pins the canonical
        // context-string discipline on the parse-failure path so a
        // future refactor can't silently regress it independent of
        // the read-failure path. Peer with the
        // `load_caixa_parse_error_diagnostic_names_path_with_parsing_
        // context` pin on the sibling manifest-IO axis.
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("torn.yaml");
        std::fs::write(&p, "name: torn\n  bad-indent: [unclosed\n").expect("write malformed yaml");
        let err = load_yaml(&p).expect_err("malformed yaml must error");
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("parsing"),
            "diagnostic must name the parsing axis (got: {rendered:?})"
        );
        assert!(
            rendered.contains("torn.yaml"),
            "diagnostic must name the offending path (got: {rendered:?})"
        );
    }

    #[test]
    fn load_yaml_parse_error_preserves_underlying_serde_yaml_error_on_chain() {
        // Peer to the parse-context pin above: the anyhow
        // `.with_context(...)` wrap preserves the underlying
        // `serde_yaml::Error` on the cause chain — a future
        // structured-output mode (`feira chart --json`, the future
        // `feira validate --strict` per-caixa admission verb, the M4
        // `feira reconcile --json` cluster-diff verb) can downcast
        // through the chain and read the typed payload directly, peer
        // with the per-renderer [`caixa_core::KindMismatch`] /
        // [`caixa_core::ServicoCountMismatch`] typed-view downcast
        // discipline and with the
        // `load_caixa_parse_error_preserves_underlying_lisp_error_on_
        // chain` pin on the sibling manifest-IO axis.
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("torn.yaml");
        std::fs::write(&p, "name: torn\n  bad-indent: [unclosed\n").expect("write malformed yaml");
        let err = load_yaml(&p).expect_err("malformed yaml must error");
        let typed_reachable = err
            .chain()
            .any(|e| e.downcast_ref::<serde_yaml::Error>().is_some());
        assert!(
            typed_reachable,
            "underlying serde_yaml::Error must remain reachable on the anyhow chain"
        );
    }

    #[test]
    fn load_caixa_parse_error_preserves_underlying_lisp_error_on_chain() {
        // Peer to the parse-context pin above: the anyhow
        // `.with_context(...)` wrap preserves the underlying
        // `tatara_lisp::LispError` on the cause chain — a future
        // structured-output mode (`feira app graph --json`, the M4
        // `feira reconcile --json` diff verb) can downcast through
        // the chain and read the typed payload directly, peer with
        // the per-renderer `KindMismatch` / `ServicoCountMismatch`
        // typed-view downcast discipline.
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join(CAIXA_MANIFEST_FILENAME), "(((((")
            .expect("write malformed manifest");
        let err = load_caixa(dir.path()).expect_err("malformed manifest must error");
        let typed_reachable = err
            .chain()
            .any(|e| e.downcast_ref::<tatara_lisp::LispError>().is_some());
        assert!(
            typed_reachable,
            "underlying tatara_lisp::LispError must remain reachable on the anyhow chain"
        );
    }
}
