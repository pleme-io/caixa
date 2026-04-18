use serde::{Deserialize, Serialize};
use tatara_lisp::DeriveTataraDomain;

use crate::entry::LacreEntry;
use crate::hash::root_hash;

/// The lock file — `lacre.lisp`.
///
/// ```lisp
/// (deflacre
///   :versao-lacre "0.1.0"
///   :raiz         "blake3:…"
///   :entradas (
///     (:nome "caixa-teia" :versao "0.1.0"
///      :fonte (:tipo git :repo "github:pleme-io/caixa-teia" :rev "deadbeef…")
///      :conteudo "blake3:…" :fechamento "blake3:…"
///      :deps-diretas ("iac-forge-ir"))
///     …))
/// ```
///
/// The root hash is a deterministic summary of every entry's (name,
/// closure hash) pair, sorted by name — plug it into sui-cache for
/// content-addressed binary caching of the whole closure.
#[derive(DeriveTataraDomain, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "deflacre")]
pub struct Lacre {
    /// Lacre format version — bump when the schema changes.
    pub versao_lacre: String,

    /// Root BLAKE3 hash of the full closure (sorted entries' fechamentos).
    pub raiz: String,

    /// Resolved dependency entries. Canonical order: sorted by `:nome`.
    #[serde(default)]
    pub entradas: Vec<LacreEntry>,
}

impl Lacre {
    /// The current lacre format version.
    pub const CURRENT_VERSAO: &'static str = "0.1.0";

    /// Build a lacre from resolved entries, computing the root hash from
    /// the entries themselves. Entries are sorted by name on the way in.
    #[must_use]
    pub fn from_entries(mut entradas: Vec<LacreEntry>) -> Self {
        entradas.sort_by(|a, b| a.nome.cmp(&b.nome));
        let summaries: Vec<_> = entradas.iter().map(LacreEntry::summary).collect();
        let raiz = root_hash(&summaries);
        Self {
            versao_lacre: Self::CURRENT_VERSAO.to_string(),
            raiz,
            entradas,
        }
    }

    /// Recompute the root hash over current entries — useful after mutation.
    #[must_use]
    pub fn recomputed_root(&self) -> String {
        let summaries: Vec<_> = self.entradas.iter().map(LacreEntry::summary).collect();
        root_hash(&summaries)
    }

    /// Returns `true` iff `self.raiz` matches the recomputed root.
    #[must_use]
    pub fn is_coherent(&self) -> bool {
        self.raiz == self.recomputed_root()
    }

    /// Parse a `lacre.lisp` source.
    pub fn from_lisp(src: &str) -> Result<Self, tatara_lisp::LispError> {
        use tatara_lisp::domain::TataraDomain;
        let forms = tatara_lisp::read(src)?;
        let first = forms
            .first()
            .ok_or_else(|| tatara_lisp::LispError::Compile {
                form: "deflacre".into(),
                message: "empty lock file".into(),
            })?;
        Self::compile_from_sexp(first)
    }

    /// Serialize to a canonical, indented `lacre.lisp` source.
    #[must_use]
    pub fn to_lisp(&self) -> String {
        let mut out = String::from("(deflacre\n");
        out.push_str(&format!("  :versao-lacre {:?}\n", self.versao_lacre));
        out.push_str(&format!("  :raiz {:?}\n", self.raiz));
        if self.entradas.is_empty() {
            out.push_str("  :entradas ())\n");
            return out;
        }
        out.push_str("  :entradas (\n");
        for (i, e) in self.entradas.iter().enumerate() {
            let json = serde_json::to_value(e).expect("LacreEntry serialize");
            let sexp = tatara_lisp::domain::json_to_sexp(&json);
            out.push_str("    ");
            out.push_str(&sexp.to_string());
            if i + 1 < self.entradas.len() {
                out.push('\n');
            } else {
                out.push('\n');
            }
        }
        out.push_str("  ))\n");
        out
    }

    /// Register `Lacre` with the global tatara-lisp domain registry.
    pub fn register() {
        tatara_lisp::domain::register::<Self>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use caixa_core::DepSource;

    fn entry(nome: &str, fechamento: &str) -> LacreEntry {
        let _ = DepSource::Path {
            caminho: "unused".into(),
        }; // prove variant path still reachable
        LacreEntry {
            nome: nome.to_string(),
            versao: "0.1.0".to_string(),
            fonte: DepSource::Git {
                repo: format!("github:pleme-io/{nome}"),
                tag: None,
                rev: Some("deadbeef".to_string()),
                branch: None,
            },
            conteudo: crate::hash::hash_bytes(nome.as_bytes()),
            fechamento: fechamento.to_string(),
            deps_diretas: vec![],
        }
    }

    #[test]
    fn from_entries_sorts_and_computes_root() {
        let fa = crate::hash::hash_bytes(b"a");
        let fb = crate::hash::hash_bytes(b"b");

        let l = Lacre::from_entries(vec![entry("b", &fb), entry("a", &fa)]);
        assert_eq!(l.entradas[0].nome, "a");
        assert_eq!(l.entradas[1].nome, "b");
        assert!(l.is_coherent());
    }

    #[test]
    fn round_trip_through_lisp() {
        let fa = crate::hash::hash_bytes(b"a");
        let l1 = Lacre::from_entries(vec![entry("a", &fa)]);
        let src = l1.to_lisp();
        let l2 = Lacre::from_lisp(&src).expect("parse lacre.lisp");
        assert_eq!(l1, l2);
    }

    #[test]
    fn empty_lacre_round_trips() {
        let l1 = Lacre::from_entries(vec![]);
        let src = l1.to_lisp();
        let l2 = Lacre::from_lisp(&src).expect("parse empty lacre");
        assert_eq!(l1, l2);
        assert!(l1.is_coherent());
    }

    #[test]
    fn tampering_with_raiz_breaks_coherence() {
        let mut l = Lacre::from_entries(vec![entry("a", &crate::hash::hash_bytes(b"a"))]);
        l.raiz = "blake3:0000000000000000000000000000000000000000000000000000000000000000".into();
        assert!(!l.is_coherent());
    }

    #[test]
    fn register_populates_registry() {
        Lacre::register();
        assert!(tatara_lisp::domain::registered_keywords().contains(&"deflacre"));
    }
}
