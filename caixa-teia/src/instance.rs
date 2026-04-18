use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::value::TeiaValue;

/// One resource instance — the runtime result of compiling a `(defteia …)`
/// form. Rendered by backends into HCL / Ruby / Lisp / Go provider code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeiaInstance {
    /// Qualified resource type — e.g. `aws/vpc`, `akeyless/secret`.
    pub tipo: String,
    /// Instance name — e.g. `main`, `primary`.
    pub nome: String,
    /// Attribute values, keyed by attribute name.
    #[serde(default)]
    pub atributos: BTreeMap<String, TeiaValue>,
}

impl TeiaInstance {
    #[must_use]
    pub fn new(tipo: impl Into<String>, nome: impl Into<String>) -> Self {
        Self {
            tipo: tipo.into(),
            nome: nome.into(),
            atributos: BTreeMap::new(),
        }
    }

    /// Append an attribute — fluent builder.
    #[must_use]
    pub fn with_attr(mut self, key: impl Into<String>, value: TeiaValue) -> Self {
        self.atributos.insert(key.into(), value);
        self
    }

    /// Terraform-style `resource "aws_vpc" "main" { … }` rendering.
    #[must_use]
    pub fn to_hcl(&self) -> String {
        let tf_tipo = self.tipo.replace('/', "_");
        let mut out = format!("resource \"{tf_tipo}\" \"{}\" {{", self.nome);
        out.push('\n');
        for (k, v) in &self.atributos {
            out.push_str(&format!("  {k} = {}\n", v.to_hcl_string()));
        }
        out.push_str("}\n");
        out
    }

    /// Quick validation against an iac-forge schema: every required attribute
    /// must be present in this instance.
    #[must_use]
    pub fn missing_required(&self, schema: &iac_forge::ir::IacResource) -> Vec<String> {
        use iac_forge::ir::HasAttributes;
        schema
            .required_attribute_names()
            .into_iter()
            .filter(|name| !self.atributos.contains_key(&(*name).to_string()))
            .map(ToString::to_string)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::TeiaRef;

    #[test]
    fn hcl_rendering() {
        let vpc = TeiaInstance::new("aws/vpc", "main")
            .with_attr("cidr_block", TeiaValue::Str("10.0.0.0/16".into()));
        let out = vpc.to_hcl();
        assert!(out.contains("resource \"aws_vpc\" \"main\""));
        assert!(out.contains(r#"cidr_block = "10.0.0.0/16""#));
    }

    #[test]
    fn ref_rendering() {
        let r = TeiaRef::new("aws/vpc", "main").atributo("id");
        let v = TeiaValue::Ref(r);
        assert_eq!(v.to_hcl_string(), "${aws_vpc.main.id}");
    }
}
