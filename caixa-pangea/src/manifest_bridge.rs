//! arch-synthesizer-compatible `TypeMutation` from `TeiaInstance` to the
//! terraform resource-block shape.

use caixa_teia::{TeiaInstance, TeiaValue};
use serde_json::{Map, Value, json};

/// Structural trait — intentionally shaped to be plug-compatible with
/// `arch_synthesizer::traits::TypeMutation` (Source/Target + `mutate`),
/// without pulling the full arch-synthesizer dep graph.
pub trait TeiaInstanceMutation {
    type Source;
    type Target;
    fn mutate(&self, source: &Self::Source) -> Self::Target;
}

/// Lower a single [`TeiaInstance`] into the Terraform JSON
/// `resource."<tf_type>"."<name>" = { … }` block shape.
///
/// Preserves all attribute names verbatim — tatara-lisp's snake_case→kebab
/// convention stays on the Lisp side; when an instance reaches this layer
/// it has already been through [`caixa_teia::parse_teia_source`].
pub struct InstanceToHcl;

impl TeiaInstanceMutation for InstanceToHcl {
    type Source = TeiaInstance;
    type Target = (String, String, Value); // (tf_type, name, block)

    fn mutate(&self, inst: &Self::Source) -> Self::Target {
        // Terraform resource types are snake_case <provider>_<kind>; neither
        // hyphens nor slashes are valid. Normalize both.
        let tf_type = inst.tipo.replace(['/', '-'], "_");
        let mut block = Map::new();
        for (k, v) in &inst.atributos {
            let key = k.replace('-', "_");
            block.insert(key, value_to_json(v));
        }
        (tf_type, inst.nome.clone(), Value::Object(block))
    }
}

fn value_to_json(v: &TeiaValue) -> Value {
    match v {
        TeiaValue::Str(s) => Value::String(s.clone()),
        TeiaValue::Int(n) => json!(*n),
        TeiaValue::Float(f) => json!(*f),
        TeiaValue::Bool(b) => Value::Bool(*b),
        TeiaValue::Null => Value::Null,
        TeiaValue::List(items) => Value::Array(items.iter().map(value_to_json).collect()),
        TeiaValue::Object(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                out.insert(k.replace('-', "_"), value_to_json(v));
            }
            Value::Object(out)
        }
        TeiaValue::Ref(r) => {
            let tf = r.tipo.replace(['/', '-'], "_");
            Value::String(format!("${{{tf}.{}.{}}}", r.nome, r.atributo))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use caixa_teia::parse_teia_source;

    #[test]
    fn lowers_simple_instance() {
        let src = r#"
(defteia
  :tipo aws/vpc
  :nome main
  :atributos (:cidr-block "10.0.0.0/16" :tags (:name "main")))
"#;
        let m = parse_teia_source(src).unwrap();
        let (tf_type, name, block) = InstanceToHcl.mutate(&m.instances[0]);
        assert_eq!(tf_type, "aws_vpc");
        assert_eq!(name, "main");
        assert_eq!(block.get("cidr_block").unwrap(), "10.0.0.0/16");
        let tags = block.get("tags").unwrap().as_object().unwrap();
        assert_eq!(tags.get("name").unwrap(), "main");
    }

    #[test]
    fn lowers_ref_as_interpolation() {
        let src =
            r#"(defteia :tipo aws/igw :nome main :atributos (:vpc-id (ref aws/vpc main id)))"#;
        let m = parse_teia_source(src).unwrap();
        let (_, _, block) = InstanceToHcl.mutate(&m.instances[0]);
        assert_eq!(block.get("vpc_id").unwrap(), "${aws_vpc.main.id}");
    }
}
