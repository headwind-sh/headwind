use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// HelmRepository defines a Helm chart repository (Flux CD v2 API)
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "source.toolkit.fluxcd.io",
    version = "v1",
    kind = "HelmRepository",
    namespaced
)]
#[kube(status = "HelmRepositoryStatus")]
#[serde(rename_all = "camelCase")]
pub struct HelmRepositorySpec {
    /// URL of the Helm repository (HTTP/HTTPS or OCI)
    pub url: String,

    /// Interval at which to check the repository for updates
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval: Option<String>,

    /// Timeout for repository operations
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,

    /// Reference to a Secret containing authentication credentials
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_ref: Option<SecretReference>,

    /// Reference to a Secret containing TLS client certificate
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cert_secret_ref: Option<SecretReference>,

    /// Whether to pass credentials to hosts other than the URL host
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pass_credentials: Option<bool>,

    /// Type of repository (default, oci)
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub repository_type: Option<String>,

    /// Cloud provider for automatic authentication (generic, aws, azure, gcp)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelmRepositoryStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditions: Option<Vec<HelmRepositoryCondition>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<HelmRepositoryArtifact>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct HelmRepositoryCondition {
    #[serde(rename = "type")]
    pub condition_type: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastTransitionTime"
    )]
    pub last_transition_time: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelmRepositoryArtifact {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_update_time: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct SecretReference {
    pub name: String,
}
