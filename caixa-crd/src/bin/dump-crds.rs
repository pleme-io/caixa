//! `dump-crds` — emit YAML for every CRD this crate defines.
//!
//! Usage: `cargo run -p caixa-crd --bin dump-crds > crds.yaml`
//!
//! The Helm chart's `templates/crds/` directory pins the output of this
//! binary so the cluster's CRDs always match the Rust source. A CI job
//! regenerates the YAML on every PR and fails if the committed files drift.

use kube::CustomResourceExt;

fn main() {
    let caixa_yaml = serde_yaml::to_string(&caixa_crd::caixa_cr::Caixa::crd()).unwrap();
    let lacre_yaml = serde_yaml::to_string(&caixa_crd::lacre_cr::Lacre::crd()).unwrap();
    let build_yaml = serde_yaml::to_string(&caixa_crd::build::CaixaBuild::crd()).unwrap();
    print!("{caixa_yaml}---\n{lacre_yaml}---\n{build_yaml}");
}
