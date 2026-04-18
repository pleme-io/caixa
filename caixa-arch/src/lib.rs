//! `caixa-arch` — reasoning layer.
//!
//! `feira tofu plan` chains: Lisp → teia → **arch proof** → HCL → tofu.
//! The `arch proof` step is this crate: walk every `TeiaInstance` as a
//! JSON tree, run built-in + user-declared invariants, refuse to emit HCL
//! if any invariant fails.
//!
//! The contract shape (Source/Target/mutate) mirrors arch-synthesizer's
//! `TypeMutation`, so callers can swap this for the full arch-synthesizer
//! typescape + catalog later. Until then, we lean on iac-forge's own
//! content-hash + sexpr discipline.

pub mod invariants;
pub mod report;
pub mod run;

pub use invariants::{Invariant, InvariantKind, Violation, builtin_invariants};
pub use report::{ArchReport, ArchVerdict};
pub use run::check_manifest;
