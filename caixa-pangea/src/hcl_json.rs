//! Render a full `main.tf.json` from a `TeiaManifest` — ready for `tofu init`.

use caixa_teia::TeiaManifest;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::manifest_bridge::{InstanceToHcl, TeiaInstanceMutation};

/// Per-render Terraform / OpenTofu settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TofuConfig {
    /// The name → version map emitted into `terraform.required_providers`.
    pub required_providers: Vec<RequiredProvider>,
    /// Optional `backend "…" { … }` block contents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<(String, Value)>,
    /// Top-level `provider "name" { … }` blocks, keyed by provider name.
    pub providers: Vec<ProviderBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequiredProvider {
    pub name: String,
    pub source: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBlock {
    pub name: String,
    pub config: Value,
}

/// Emit the full `.tf.json` as a [`serde_json::Value`]. Callers serialize
/// to a string with `serde_json::to_string_pretty`.
#[must_use]
pub fn emit_tf_json(manifest: &TeiaManifest, tofu: &TofuConfig) -> Value {
    let mut root = Map::new();

    // terraform { required_providers { … }, backend … }
    let mut tf_block = Map::new();
    if !tofu.required_providers.is_empty() {
        let mut rps = Map::new();
        for rp in &tofu.required_providers {
            rps.insert(
                rp.name.clone(),
                json!({ "source": rp.source, "version": rp.version }),
            );
        }
        tf_block.insert("required_providers".into(), Value::Object(rps));
    }
    if let Some((name, cfg)) = &tofu.backend {
        tf_block.insert("backend".into(), json!({ name: cfg }));
    }
    if !tf_block.is_empty() {
        root.insert(
            "terraform".into(),
            Value::Array(vec![Value::Object(tf_block)]),
        );
    }

    // provider blocks — a list of { "<name>": { … } } entries.
    if !tofu.providers.is_empty() {
        let items: Vec<Value> = tofu
            .providers
            .iter()
            .map(|p| json!({ p.name.clone(): p.config.clone() }))
            .collect();
        root.insert("provider".into(), Value::Array(items));
    }

    // resource blocks — group by tf_type, then by name.
    let mut resources: Map<String, Value> = Map::new();
    for inst in &manifest.instances {
        let (tf_type, name, block) = InstanceToHcl.mutate(inst);
        let by_name = resources
            .entry(tf_type)
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(map) = by_name {
            map.insert(name, block);
        }
    }
    if !resources.is_empty() {
        root.insert("resource".into(), Value::Object(resources));
    }

    Value::Object(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use caixa_teia::parse_teia_source;

    #[test]
    fn emits_resource_block() {
        let src = r#"
(defteia :tipo aws/vpc :nome main :atributos (:cidr-block "10.0.0.0/16"))
"#;
        let m = parse_teia_source(src).unwrap();
        let out = emit_tf_json(&m, &TofuConfig::default());
        let s = serde_json::to_string(&out).unwrap();
        assert!(s.contains("\"resource\""));
        assert!(s.contains("\"aws_vpc\""));
        assert!(s.contains("\"cidr_block\":\"10.0.0.0/16\""));
    }

    #[test]
    fn emits_required_providers() {
        let src = r#"(defteia :tipo aws/vpc :nome main :atributos (:cidr-block "10.0.0.0/16"))"#;
        let m = parse_teia_source(src).unwrap();
        let cfg = TofuConfig {
            required_providers: vec![RequiredProvider {
                name: "aws".into(),
                source: "hashicorp/aws".into(),
                version: "~> 5.0".into(),
            }],
            backend: None,
            providers: vec![ProviderBlock {
                name: "aws".into(),
                config: json!({ "region": "us-east-1" }),
            }],
        };
        let out = emit_tf_json(&m, &cfg);
        let s = serde_json::to_string(&out).unwrap();
        assert!(s.contains("hashicorp/aws"));
        assert!(s.contains("us-east-1"));
    }

    #[test]
    fn groups_resources_by_type() {
        let src = r#"
(defteia :tipo aws/vpc :nome main :atributos (:cidr-block "10.0.0.0/16"))
(defteia :tipo aws/vpc :nome backup :atributos (:cidr-block "10.1.0.0/16"))
(defteia :tipo aws/igw :nome main :atributos (:vpc-id (ref aws/vpc main id)))
"#;
        let m = parse_teia_source(src).unwrap();
        let out = emit_tf_json(&m, &TofuConfig::default());
        let res = out.get("resource").unwrap().as_object().unwrap();
        let aws_vpcs = res.get("aws_vpc").unwrap().as_object().unwrap();
        assert_eq!(aws_vpcs.len(), 2);
        assert!(aws_vpcs.contains_key("main"));
        assert!(aws_vpcs.contains_key("backup"));
        assert!(res.get("aws_igw").is_some());
    }
}
