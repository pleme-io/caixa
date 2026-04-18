use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A recursive attribute value — scalars, lists, objects, and typed refs.
///
/// BTreeMap for objects keeps serialization deterministic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TeiaValue {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    List(Vec<TeiaValue>),
    Object(BTreeMap<String, TeiaValue>),
    /// A typed reference produced by `(ref aws/vpc main id)`. The renderer
    /// emits `${aws_vpc.main.id}` (Terraform) or the platform equivalent.
    Ref(TeiaRefRepr),
    Null,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeiaRefRepr {
    pub tipo: String,
    pub nome: String,
    pub atributo: String,
}

impl TeiaValue {
    #[must_use]
    pub fn to_hcl_string(&self) -> String {
        match self {
            Self::Str(s) => format!("{s:?}"),
            Self::Int(i) => i.to_string(),
            Self::Float(f) => f.to_string(),
            Self::Bool(b) => b.to_string(),
            Self::Null => "null".to_string(),
            Self::List(items) => {
                let parts: Vec<String> = items.iter().map(Self::to_hcl_string).collect();
                format!("[{}]", parts.join(", "))
            }
            Self::Object(map) => {
                let parts: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("{k} = {}", v.to_hcl_string()))
                    .collect();
                format!("{{ {} }}", parts.join(", "))
            }
            Self::Ref(r) => {
                let tipo = r.tipo.replace('/', "_");
                format!("${{{tipo}.{}.{}}}", r.nome, r.atributo)
            }
        }
    }
}
