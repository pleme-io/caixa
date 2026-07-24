use std::path::PathBuf;

use anyhow::{Context, Result};
use caixa_core::{Caixa, LayoutInvariants, StandardLayout};
use clap::Args;

use super::load::{caixa_root, load_caixa};

/// Validate caixa.lisp + layout + every `:bibliotecas` entry parses.
///
/// This is the phase-1 analog of `cargo check` — no actual compilation, but
/// every declared library source is parsed through `tatara_lisp::read` so
/// lexical / structural errors surface before an `importar` from another
/// caixa. Real compilation (`tatara-lispc`) wires in next phase.
#[derive(Args)]
pub struct Build {
    /// caixa root (defaults to CWD).
    #[arg(long)]
    pub path: Option<PathBuf>,
}

impl Build {
    pub fn run(self) -> Result<()> {
        let root = caixa_root(self.path.as_deref());
        let caixa = load_caixa(&root)?;

        StandardLayout::new()
            .verify(&caixa, &root)
            .context("layout invariants violated")?;

        for entry in caixa.bibliotecas() {
            let path = root.join(entry);
            let src = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            tatara_lisp::read(&src).with_context(|| format!("parsing {}", path.display()))?;
            eprintln!("  ✓ {entry}");
        }

        eprintln!("{}", build_summary_line(&caixa));
        Ok(())
    }
}

/// Substrate-canonical `feira build` per-caixa post-parse summary
/// line composer — derives the `"caixa {nome} v{versao} — layout +
/// lisp parse clean ({n} lib entry/ies)"` byte-string every operator
/// sees on stderr past a clean `feira build` through the typed
/// [`caixa_core::Caixa::nome`] / [`caixa_core::Caixa::versao`] /
/// [`caixa_core::Caixa::bibliotecas`] accessors rather than the raw
/// `.nome` / `.versao` field-accesses the inline emit at
/// [`Build::run`] used before this lift. Peer with every prior per-
/// substrate-side renderer converge on the outer-`Caixa` `:nome` /
/// `:versao` universal-axis accessors (caixa-helm 22461ef / eb912de /
/// 05a7701, caixa-flux 4a363bf / 162e2e2 / 2fc5f81, caixa-mesh
/// 54bf2f3 / 980c059, caixa-crd 61d3429 / 41ab9a3) — this closes the
/// analogous last unlifted `feira build` summary-line raw-field-
/// access site in caixa-feira on both axes at once (the emit site
/// composes `:nome` and `:versao` in the same `format!` template, so
/// converging one without the other would leave a mixed-mode emit).
pub(crate) fn build_summary_line(caixa: &Caixa) -> String {
    format!(
        "caixa {} v{} — layout + lisp parse clean ({} lib entry/ies)",
        caixa.nome(),
        caixa.versao(),
        caixa.bibliotecas().len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_summary_line_routes_through_caixa_nome_and_versao_accessors() {
        // Emit-path pin: the `feira build` per-caixa post-parse
        // summary line composer's terminal `{nome}` / `{versao}` /
        // `{n}` scalars must derive through the typed
        // [`caixa_core::Caixa::nome`] / [`caixa_core::Caixa::versao`] /
        // [`caixa_core::Caixa::bibliotecas`] accessors byte-for-byte.
        // Before this converge the emit site at [`Build::run`]
        // carried raw `caixa.nome` / `caixa.versao` field-accesses
        // into the inline `eprintln!("caixa {} v{} — layout + lisp
        // parse clean ({} lib entry/ies)", ...)` template, bypassing
        // the typed dispatch every peer substrate-side renderer's
        // `:nome` / `:versao` emit-site already routes through.
        //
        // Byte-equal today (both accessors are `&self.<field>`); the
        // pin catches any future accessor extension (SemVer-2 build-
        // metadata canonicalization the CAIXA-SDLC §I SemVer-2 pin
        // acknowledges, per-edition pre-release-tag overlay
        // dispatched through the sibling [`caixa_core::Caixa::edicao`]
        // universal-axis scalar) whose `feira build` summary regresses
        // to the raw field and silently splits the byte-string the
        // operator reads on stderr from the paired downstream artefact
        // emit (the [`caixa_helm`] `Chart.yaml` `version:` / `appVersion:`,
        // the [`caixa_flux`] `programs.yaml` `versao:` fold + the
        // `cluster_bundle` `GitRepository` `spec.ref.tag`, the
        // [`caixa_crd`] `CaixaSpec.versao` CR-side emit) that already
        // routes through the accessor.
        let caixa = Caixa::from_lisp(
            r#"(defcaixa
                 :nome "b-summary"
                 :kind Biblioteca
                 :versao "0.2.3"
                 :bibliotecas ())"#,
        )
        .expect("parse");
        let line = build_summary_line(&caixa);
        assert_eq!(
            line,
            format!(
                "caixa {} v{} — layout + lisp parse clean ({} lib entry/ies)",
                caixa.nome(),
                caixa.versao(),
                caixa.bibliotecas().len()
            ),
            "summary line must route the `{{nome}}` / `{{versao}}` / `{{n}}` \
             scalars through the typed Caixa::nome / Caixa::versao / \
             Caixa::bibliotecas accessors — any regression to the raw \
             `caixa.nome` / `caixa.versao` field-accesses would silently \
             pass today (accessors are `&self.<field>`) but drift on the \
             first accessor extension"
        );
        assert!(
            line.contains("caixa b-summary v0.2.3"),
            "summary must carry the caixa's `:nome` / `:versao` verbatim \
             (got: {line:?})"
        );
        assert!(
            line.contains("(0 lib entry/ies)"),
            "summary must carry the `:bibliotecas` entry count (got: {line:?})"
        );
    }

    #[test]
    fn build_summary_line_carries_nonzero_bibliotecas_count() {
        // Peer of the zero-count arm above: a `:kind Biblioteca` caixa
        // with two declared `:bibliotecas` entries surfaces the
        // parenthesized `(n lib entry/ies)` suffix with `n == 2`, so
        // the summary line's `:bibliotecas`-count projection remains
        // reachable past a caixa that actually declares library
        // sources. Guards against a regression that hard-codes `0` or
        // drops the count projection entirely.
        let caixa = Caixa::from_lisp(
            r#"(defcaixa
                 :nome "b-two-libs"
                 :kind Biblioteca
                 :versao "0.1.0"
                 :bibliotecas ("lib/a.lisp" "lib/b.lisp"))"#,
        )
        .expect("parse");
        let line = build_summary_line(&caixa);
        assert!(
            line.contains("caixa b-two-libs v0.1.0"),
            "summary must carry the caixa's `:nome` / `:versao` verbatim \
             (got: {line:?})"
        );
        assert!(
            line.contains("(2 lib entry/ies)"),
            "summary must carry the actual `:bibliotecas` entry count \
             (got: {line:?})"
        );
    }
}
