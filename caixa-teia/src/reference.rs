//! References — `(ref aws/vpc main id)` producing cross-resource attribute
//! lookups. The terraform-synthesizer's `ResourceReference.method_missing`
//! equivalent, but type-checked through TataraDomain where possible.

use crate::value::TeiaRefRepr;

/// Lightweight builder for producing a [`TeiaRefRepr`].
#[derive(Debug, Clone, PartialEq)]
pub struct TeiaRef {
    pub tipo: String,
    pub nome: String,
}

impl TeiaRef {
    #[must_use]
    pub fn new(tipo: impl Into<String>, nome: impl Into<String>) -> Self {
        Self {
            tipo: tipo.into(),
            nome: nome.into(),
        }
    }

    /// `ref.atributo("id")` → a ready-to-render reference.
    #[must_use]
    pub fn atributo(&self, atributo: impl Into<String>) -> TeiaRefRepr {
        TeiaRefRepr {
            tipo: self.tipo.clone(),
            nome: self.nome.clone(),
            atributo: atributo.into(),
        }
    }
}
