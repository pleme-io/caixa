//! `caixa-pangea` — the Lisp-to-Pangea-to-HCL bridge.
//!
//! Ruby Pangea composes typed resources → a nested-hash IR → a
//! TerraformSynthesizer renders HCL/JSON. We compose Lisp `(defteia …)`
//! forms the same way, with the same trait shape, into:
//!
//! ```text
//! TeiaManifest  ──►  iac_forge::IacResource set  ──►  main.tf.json  ──►  tofu plan/apply
//!                    (schema + values)            (Terraform JSON config)
//! ```
//!
//! The renderer emits Terraform's `.tf.json` — the supported JSON form of
//! HCL. That avoids hand-rolling an HCL printer and keeps every string a
//! serde_json value; cloud providers consume `.tf.json` identically to `.tf`.
//!
//! Phase 1 scope: resource + provider blocks. Data sources, variables,
//! outputs, locals, modules — phase 2 template work.

pub mod hcl_json;
pub mod manifest_bridge;

pub use hcl_json::{ProviderBlock, RequiredProvider, TofuConfig, emit_tf_json};
pub use manifest_bridge::{InstanceToHcl, TeiaInstanceMutation};
