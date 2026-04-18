use serde::{Deserialize, Serialize};

/// What a caixa produces.
///
/// In `caixa.lisp`:
///
/// ```lisp
/// :kind Biblioteca   ; library (lib/<nome>.lisp entry)
/// :kind Binario      ; executable(s) under exe/
/// :kind Servico      ; long-running service under servicos/
/// ```
///
/// Authored as bare symbols (`Biblioteca` not `:biblioteca`) to match the
/// tatara-lisp enum convention where symbols become enum discriminants via
/// the serde `Deserialize` fallthrough.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaixaKind {
    /// Library — exports Lisp forms for other caixas to `(importar …)`.
    Biblioteca,
    /// Binary — one or more executables under `exe/`.
    Binario,
    /// Service — long-running daemon under `servicos/`.
    Servico,
}

impl CaixaKind {
    /// A `Biblioteca` is expected to have at least one `lib/` entry.
    #[must_use]
    pub const fn requires_lib(self) -> bool {
        matches!(self, Self::Biblioteca)
    }

    /// A `Binario` is expected to have at least one `exe/` entry.
    #[must_use]
    pub const fn requires_exe(self) -> bool {
        matches!(self, Self::Binario)
    }

    /// A `Servico` is expected to have at least one `servicos/` entry.
    #[must_use]
    pub const fn requires_servicos(self) -> bool {
        matches!(self, Self::Servico)
    }

    /// The canonical human-readable name (`"biblioteca"`, `"binario"`, `"servico"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Biblioteca => "biblioteca",
            Self::Binario => "binario",
            Self::Servico => "servico",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_requirements() {
        assert!(CaixaKind::Biblioteca.requires_lib());
        assert!(!CaixaKind::Biblioteca.requires_exe());
        assert!(CaixaKind::Binario.requires_exe());
        assert!(CaixaKind::Servico.requires_servicos());
    }

    #[test]
    fn kind_deserializes_from_pascal_symbol() {
        let v: CaixaKind = serde_json::from_str("\"Biblioteca\"").unwrap();
        assert_eq!(v, CaixaKind::Biblioteca);
    }
}
