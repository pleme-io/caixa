use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `CaixaBuild` — one-shot build of a specific caixa at a specific rev.
///
/// The operator materializes a K8s `Job` from this CR, executes `feira
/// build`, and writes artifact digests + logs to the status.
#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema, PartialEq)]
#[kube(
    group = "caixa.pleme.io",
    version = "v1alpha1",
    kind = "CaixaBuild",
    plural = "caixabuilds",
    singular = "caixabuild",
    shortname = "cxb",
    namespaced,
    status = "CaixaBuildStatus",
    printcolumn = r#"{"name":"Caixa","type":"string","jsonPath":".spec.caixaRef"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#,
    printcolumn = r#"{"name":"Started","type":"date","jsonPath":".status.startedAt"}"#,
    printcolumn = r#"{"name":"Finished","type":"date","jsonPath":".status.finishedAt"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct CaixaBuildSpec {
    /// Target Caixa resource.
    pub caixa_ref: String,

    /// Override the rev from the Caixa spec.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev_override: Option<String>,

    /// Backend to build against — "lisp" | "go-ferrite" | "ruby-pangea" | "nix".
    #[serde(default)]
    pub backends: Vec<String>,

    /// When true, push artifacts to the configured OCI registry on success.
    #[serde(default)]
    pub push_artifacts: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CaixaBuildStatus {
    /// Pending | Running | Succeeded | Failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,

    /// BLAKE3 of each produced artifact, by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<BuildArtifact>,

    /// The K8s Job owning this build's pod(s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_ref: Option<String>,

    /// Truncated log tail (full logs via `kubectl logs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_tail: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BuildArtifact {
    pub name: String,
    pub backend: String,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pushed_uri: Option<String>,
}
