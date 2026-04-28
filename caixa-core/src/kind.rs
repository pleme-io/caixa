use serde::{Deserialize, Serialize};

/// What a caixa produces.
///
/// In `caixa.lisp`:
///
/// ```lisp
/// :kind Biblioteca   ; library (lib/<nome>.lisp entry)
/// :kind Binario      ; executable(s) under exe/
/// :kind Servico      ; long-running service under servicos/
/// :kind Supervisor   ; OTP-style typed supervisor tree (see supervisor.rs)
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
    /// OTP-shaped supervisor — does not run any code itself; its
    /// children are other caixas, restarted under a typed strategy.
    /// See `supervisor.rs` for the full shape (`SupervisorSpec`).
    Supervisor,
    /// Typed application — composes multiple Servicos into a single
    /// declarative mesh with WIT-typed `:contratos`, mesh-level
    /// `:politicas`, and explicit `:placement`. See `aplicacao.rs`
    /// (`AplicacaoSpec`) and `theory/MESH-COMPOSITION.md` for the
    /// design frame.
    Aplicacao,
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

    /// A `Supervisor` is expected to have at least one `:children` entry
    /// (or a `SimpleOneForOne` strategy that spawns children dynamically).
    #[must_use]
    pub const fn requires_children(self) -> bool {
        matches!(self, Self::Supervisor)
    }

    /// An `Aplicacao` is expected to have at least one `:membros` entry.
    #[must_use]
    pub const fn requires_membros(self) -> bool {
        matches!(self, Self::Aplicacao)
    }

    /// The canonical human-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Biblioteca => "biblioteca",
            Self::Binario => "binario",
            Self::Servico => "servico",
            Self::Supervisor => "supervisor",
            Self::Aplicacao => "aplicacao",
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
        assert!(CaixaKind::Supervisor.requires_children());
        assert!(!CaixaKind::Servico.requires_children());
    }

    #[test]
    fn kind_deserializes_from_pascal_symbol() {
        let v: CaixaKind = serde_json::from_str("\"Biblioteca\"").unwrap();
        assert_eq!(v, CaixaKind::Biblioteca);
        let v: CaixaKind = serde_json::from_str("\"Supervisor\"").unwrap();
        assert_eq!(v, CaixaKind::Supervisor);
    }

    #[test]
    fn supervisor_kind_has_canonical_name() {
        assert_eq!(CaixaKind::Supervisor.as_str(), "supervisor");
    }
}
