use caixa_core::DepSource;
use serde::{Deserialize, Serialize};

use crate::hash::LacreSummary;

/// One resolved dependency in a lacre — concrete source, pinned version,
/// content + closure hashes.
///
/// ```lisp
/// (:nome        "caixa-teia"
///  :versao      "0.1.0"
///  :fonte       (:tipo git :repo "github:pleme-io/caixa-teia" :rev "deadbeefcafe...")
///  :conteudo    "blake3:ab12…"
///  :fechamento  "blake3:cd34…"
///  :deps-diretas ("iac-forge-ir" "tatara-lisp"))
/// ```
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LacreEntry {
    /// Caixa name.
    pub nome: String,

    /// Concrete resolved version (not a `VersionReq`).
    pub versao: String,

    /// The resolved source. Registry deps end up with
    /// [`DepSource::Git`] pointing at the indexed repo + rev, or
    /// [`DepSource::Feira`] with `:registro` set for private registries.
    pub fonte: DepSource,

    /// BLAKE3 of the caixa's own content (as fetched).
    pub conteudo: String,

    /// BLAKE3 of the closure — own content + all transitive closures, sorted.
    pub fechamento: String,

    /// Names of direct deps, for audit / topology — does not affect hashing.
    #[serde(default)]
    pub deps_diretas: Vec<String>,
}

impl LacreEntry {
    #[must_use]
    pub fn summary(&self) -> LacreSummary {
        LacreSummary {
            nome: self.nome.clone(),
            fechamento: self.fechamento.clone(),
        }
    }
}
