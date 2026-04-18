//! `caixa-provedor` — iac-forge backend that emits **ferrite-typed Go** for
//! OpenTofu / Terraform providers.
//!
//! Ferrite is pleme-io's linear-ownership overlay on Go (`ferrite/rt` + AST
//! analysis). Standard `terraform-forge` emits GC-tracked Go; this backend
//! emits the same CRUD shape but with:
//!
//!   - `ferrite/rt` instead of bare pointers — every `*T` becomes `Owned[T]`.
//!   - Explicit `Drop()` at the end of scope.
//!   - Borrow passes for `Read/Create/Update/Delete`.
//!   - Generated source passes `ferrite-check` **by construction** — we
//!     write it to match the analyzer's rules, not the other way around.
//!
//! The eventual mutator swap (`ferrite/rt` → `ferrite/rt/arena`) drops GC
//! participation entirely; `feira publish-provider --target opentofu
//! <name>` outputs a Go binary running `GOGC=off`.
//!
//! This crate is phase-3. The emitter below produces **one resource**
//! end-to-end, validating the template choices. Scaling to whole providers
//! (all 448 AWS resources, etc.) is a template expansion + CRUD router.

pub mod backend;
pub mod ferrite;
pub mod imports;

pub use backend::FerriteTofuBackend;
pub use ferrite::{ferrite_rt_import, ferrite_runtime_variant};
