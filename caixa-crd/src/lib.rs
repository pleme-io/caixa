//! `caixa-crd` — Kubernetes CRD wire formats.
//!
//! Three CRDs in one group `caixa.pleme.io/v1alpha1`:
//!
//!   - **`Caixa`** — the desired caixa: source (Git URL + rev), expected
//!     closure root, reconciliation policy. The operator watches these
//!     and produces `Lacre`s from them.
//!
//!   - **`Lacre`** — observed resolution state: the actual BLAKE3 root,
//!     per-entry content hashes, last-resolved timestamp. Mirrors
//!     `lacre.lisp` but as a K8s status-bearing resource.
//!
//!   - **`CaixaBuild`** — a one-shot "build this caixa at this rev"
//!     request. The operator runs `feira build` in a Job, attaches logs
//!     + artifacts to status, emits events on success/failure.
//!
//! These CRDs are the K8s-native peer of the same tatara-lisp types
//! (`defcaixa`, `deflacre`). Round-trip through [`caixa_into_cr`] /
//! [`caixa_from_cr`] keeps the Lisp authoring surface canonical.

pub mod build;
pub mod caixa_cr;
pub mod conversion;
pub mod lacre_cr;

pub use build::{CaixaBuild, CaixaBuildSpec, CaixaBuildStatus};
pub use caixa_cr::{Caixa as CaixaCr, CaixaSpec, CaixaStatus};
pub use conversion::{caixa_from_cr, caixa_into_cr};
pub use lacre_cr::{Lacre as LacreCr, LacreSpec, LacreStatus};
