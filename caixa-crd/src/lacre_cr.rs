use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `Lacre` — resolved closure, produced by the operator from a `Caixa`.
///
/// Status-heavy: the spec carries the identity (which caixa + at what rev),
/// the status carries the full BLAKE3 closure.
#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema, PartialEq)]
#[kube(
    group = "caixa.pleme.io",
    version = "v1alpha1",
    kind = "Lacre",
    plural = "lacres",
    singular = "lacre",
    shortname = "lcr",
    namespaced,
    status = "LacreStatus",
    printcolumn = r#"{"name":"Caixa","type":"string","jsonPath":".spec.caixaRef"}"#,
    printcolumn = r#"{"name":"Root","type":"string","jsonPath":".status.raiz"}"#,
    printcolumn = r#"{"name":"Entries","type":"integer","jsonPath":".status.entradaCount"}"#,
    printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct LacreSpec {
    /// Name of the `Caixa` resource this lacre belongs to.
    pub caixa_ref: String,

    /// The rev (git SHA) the lacre was resolved at.
    pub resolved_rev: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LacreStatus {
    /// Blake3 root — `blake3:<hex>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raiz: Option<String>,

    /// Number of resolved entries — for the printer column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrada_count: Option<i32>,

    /// The per-dep entries. Large-ish; the whole closure lives in status.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entradas: Vec<LacreEntryCr>,

    /// When the lacre was last updated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LacreEntryCr {
    pub nome: String,
    pub versao: String,
    pub fonte: String,
    pub conteudo: String,
    pub fechamento: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deps_diretas: Vec<String>,
}
