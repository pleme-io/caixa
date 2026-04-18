use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `Caixa` — desired caixa state. A cluster-friendly mirror of
/// `caixa.lisp` + the reconciliation policy for the operator.
#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema, PartialEq)]
#[kube(
    group = "caixa.pleme.io",
    version = "v1alpha1",
    kind = "Caixa",
    plural = "caixas",
    singular = "caixa",
    shortname = "cxa",
    namespaced,
    status = "CaixaStatus",
    printcolumn = r#"{"name":"Versao","type":"string","jsonPath":".spec.versao"}"#,
    printcolumn = r#"{"name":"Source","type":"string","jsonPath":".spec.source.repo"}"#,
    printcolumn = r#"{"name":"Ref","type":"string","jsonPath":".spec.source.gitRef"}"#,
    printcolumn = r#"{"name":"Root","type":"string","jsonPath":".status.fechamentoRoot"}"#,
    printcolumn = r#"{"name":"Ready","type":"string","jsonPath":".status.ready"}"#,
    printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct CaixaSpec {
    /// Caixa nome — must equal `spec.source.repo`'s basename by convention.
    pub nome: String,

    /// Pinned version.
    pub versao: String,

    /// Kind — Biblioteca | Binario | Servico.
    pub kind: String,

    /// Authoritative source of the caixa.
    pub source: CaixaSource,

    /// Optional — reconciliation policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconcile: Option<ReconcilePolicy>,

    /// Optional — pre-materialized deps. When unset, the operator resolves
    /// them via its cached resolver.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deps: Vec<DepRef>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CaixaSource {
    /// Git URL — may use `github:org/repo` shorthand.
    pub repo: String,
    /// Git ref — tag / branch / sha. The operator pins `status.resolvedRev`.
    pub git_ref: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReconcilePolicy {
    /// How often the operator re-fetches the source to check for updates.
    /// Defaults to 5 minutes at operator-side when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_seconds: Option<i64>,
    /// Whether the operator should auto-refresh the lacre when the source
    /// moves.
    #[serde(default)]
    pub auto_resolve: bool,
    /// Include :deps-dev in resolution.
    #[serde(default)]
    pub include_dev: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DepRef {
    pub nome: String,
    pub versao: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<CaixaSource>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CaixaStatus {
    /// Observed generation — tracks `.metadata.generation` for consumers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,

    /// Concrete Git SHA the operator resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_rev: Option<String>,

    /// BLAKE3 root of the closure after resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fechamento_root: Option<String>,

    /// Name of the associated Lacre resource (1:1 with Caixa).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lacre_ref: Option<String>,

    /// Ready — True | False | Unknown (stored as enum string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready: Option<String>,

    /// Last successful reconciliation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_reconciled: Option<String>,

    /// Standard K8s conditions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<Condition>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    pub kind: String,
    pub status: String,
    pub reason: String,
    pub message: String,
    pub last_transition_time: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caixa_serializes_to_yaml() {
        let spec = CaixaSpec {
            nome: "demo".into(),
            versao: "0.1.0".into(),
            kind: "Biblioteca".into(),
            source: CaixaSource {
                repo: "github:pleme-io/demo".into(),
                git_ref: "v0.1.0".into(),
            },
            reconcile: Some(ReconcilePolicy {
                interval_seconds: Some(300),
                auto_resolve: true,
                include_dev: false,
            }),
            deps: vec![],
        };
        let cr = Caixa::new("demo", spec);
        let yaml = serde_yaml::to_string(&cr).unwrap();
        assert!(yaml.contains("kind: Caixa"));
        assert!(yaml.contains("apiVersion: caixa.pleme.io/v1alpha1"));
        assert!(yaml.contains("nome: demo"));
        assert!(yaml.contains("gitRef: v0.1.0"));
    }
}
