//! CRDs must serialize to YAML that the K8s API server would accept.

use caixa_crd::{build::CaixaBuild, caixa_cr::Caixa, lacre_cr::Lacre};
use kube::CustomResourceExt;

#[test]
fn caixa_crd_has_expected_shape() {
    let crd = Caixa::crd();
    let yaml = serde_yaml::to_string(&crd).unwrap();
    assert!(yaml.contains("apiVersion: apiextensions.k8s.io/v1"));
    assert!(yaml.contains("kind: CustomResourceDefinition"));
    assert!(yaml.contains("name: caixas.caixa.pleme.io"));
    assert!(yaml.contains("scope: Namespaced"));
    assert!(yaml.contains("singular: caixa"));
    assert!(yaml.contains("shortNames"));
    assert!(yaml.contains("cxa"));
}

#[test]
fn lacre_crd_has_expected_shape() {
    let crd = Lacre::crd();
    let yaml = serde_yaml::to_string(&crd).unwrap();
    assert!(yaml.contains("name: lacres.caixa.pleme.io"));
    assert!(yaml.contains("singular: lacre"));
    assert!(yaml.contains("lcr"));
}

#[test]
fn caixabuild_crd_has_expected_shape() {
    let crd = CaixaBuild::crd();
    let yaml = serde_yaml::to_string(&crd).unwrap();
    assert!(yaml.contains("name: caixabuilds.caixa.pleme.io"));
    assert!(yaml.contains("singular: caixabuild"));
    assert!(yaml.contains("cxb"));
}

#[test]
fn all_three_crds_share_api_group() {
    for crd_yaml in [
        serde_yaml::to_string(&Caixa::crd()).unwrap(),
        serde_yaml::to_string(&Lacre::crd()).unwrap(),
        serde_yaml::to_string(&CaixaBuild::crd()).unwrap(),
    ] {
        assert!(crd_yaml.contains("group: caixa.pleme.io"));
    }
}
