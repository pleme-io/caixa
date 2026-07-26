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
        // hyphens nor slashes are valid. Normalize both. The `:tipo` and
        // `:nome` reads route through the lifted [`caixa_teia::TeiaInstance
        // ::tipo`] / [`caixa_teia::TeiaInstance::nome`] scalar accessors
        // rather than the raw `inst.tipo` / `inst.nome` field accesses —
        // this per-instance `<provider>_<kind>` Terraform-JSON type-name
        // mint + `<name>` block-name carry and every `caixa-arch::
        // invariants` per-instance dedup key / `Violation::instance_tipo`
        // / `Violation::instance_nome` carrier now key off the same
        // substrate-canonical resolvers.
        let tf_type = inst.tipo().replace(['/', '-'], "_");
        let mut block = Map::new();
        for (k, v) in &inst.atributos {
            let key = k.replace('-', "_");
            block.insert(key, value_to_json(v));
        }
        (tf_type, inst.nome().to_string(), Value::Object(block))
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
            // Terraform resource types are snake_case <provider>_<kind>;
            // neither hyphens nor slashes are valid. The `:tipo` read
            // routes through the lifted [`caixa_teia::TeiaRefRepr::tipo`]
            // scalar accessor rather than the raw `r.tipo` field access
            // — this per-`Ref` `<provider>_<kind>` Terraform-JSON type-
            // name mint and every `caixa-arch::invariants`
            // `no-unresolved-refs` per-`Ref` declared-set lookup key +
            // refusal-message Display interpolation now key off the same
            // substrate-canonical resolver.
            let tf = r.tipo().replace(['/', '-'], "_");
            // Same discipline for the per-`Ref` `<nome>` and
            // `<attr>` carries — route through the lifted
            // [`caixa_teia::TeiaRefRepr::nome`] /
            // [`caixa_teia::TeiaRefRepr::atributo`] scalar accessors
            // rather than the raw `r.nome` / `r.atributo` field
            // accesses. The Terraform-JSON `${<tf>.<nome>.<attr>}`
            // per-`Ref` interpolation lower now keys off the same
            // substrate-canonical per-`Ref` `:nome` / `:atributo`
            // resolvers the `caixa-arch::invariants`
            // `no-unresolved-refs` refusal-message Display
            // interpolation and the substrate primitive's own
            // `TeiaValue::to_hcl_string` per-`Ref`
            // `${<tf>.<nome>.<attr>}` HCL emit path route through.
            Value::String(format!("${{{tf}.{}.{}}}", r.nome(), r.atributo()))
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
