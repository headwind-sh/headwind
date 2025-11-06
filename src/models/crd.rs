use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// UpdateRequest is a CRD that represents a pending update to a Kubernetes resource
#[allow(dead_code)]
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "headwind.sh",
    version = "v1alpha1",
    kind = "UpdateRequest",
    plural = "updaterequests",
    shortname = "ur",
    shortname = "upd",
    namespaced,
    status = "UpdateRequestStatus",
    printcolumn = r#"{"name":"Target", "type":"string", "jsonPath":".spec.targetRef.name"}"#,
    printcolumn = r#"{"name":"Container", "type":"string", "jsonPath":".spec.containerName"}"#,
    printcolumn = r#"{"name":"Current", "type":"string", "jsonPath":".spec.currentImage"}"#,
    printcolumn = r#"{"name":"New", "type":"string", "jsonPath":".spec.newImage"}"#,
    printcolumn = r#"{"name":"Policy", "type":"string", "jsonPath":".spec.policy"}"#,
    printcolumn = r#"{"name":"Phase", "type":"string", "jsonPath":".status.phase"}"#,
    printcolumn = r#"{"name":"Age", "type":"date", "jsonPath":".metadata.creationTimestamp"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRequestSpec {
    /// Reference to the target resource to update
    pub target_ref: TargetRef,

    /// Type of update (image or helmChart)
    pub update_type: UpdateType,

    /// Name of the container to update (for image updates)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,

    /// Current image or chart version
    pub current_image: String,

    /// New image or chart version
    pub new_image: String,

    /// Policy that triggered this update
    pub policy: UpdatePolicyType,

    /// Human-readable reason for the update
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Whether this update requires manual approval
    #[serde(default = "default_require_approval")]
    pub require_approval: bool,

    /// Optional expiration time for this update request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

fn default_require_approval() -> bool {
    true
}

/// Reference to the target Kubernetes resource
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct TargetRef {
    /// API version of the target resource
    pub api_version: String,

    /// Kind of the target resource (e.g., Deployment, StatefulSet)
    pub kind: String,

    /// Name of the target resource
    pub name: String,

    /// Namespace of the target resource
    pub namespace: String,
}

/// Type of update
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub enum UpdateType {
    Image,
    HelmChart,
}

/// Policy type for the update
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum UpdatePolicyType {
    Major,
    Minor,
    Patch,
    Glob,
    None,
}

/// Status of the UpdateRequest
#[derive(Deserialize, Serialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct UpdateRequestStatus {
    /// Current phase of the update request
    #[serde(default)]
    pub phase: UpdatePhase,

    /// User or system that approved the update
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by: Option<String>,

    /// When the update was approved
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<DateTime<Utc>>,

    /// User or system that rejected the update
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected_by: Option<String>,

    /// When the update was rejected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected_at: Option<DateTime<Utc>>,

    /// Status message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    /// Last time this status was updated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<DateTime<Utc>>,
}

/// Phase of the UpdateRequest lifecycle
#[derive(Deserialize, Serialize, Clone, Debug, Default, JsonSchema, PartialEq, Eq)]
#[allow(dead_code)]
pub enum UpdatePhase {
    #[default]
    Pending,
    Approved,
    Rejected,
    Completed,
    Failed,
    Expired,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_request_creation() {
        let spec = UpdateRequestSpec {
            target_ref: TargetRef {
                api_version: "apps/v1".to_string(),
                kind: "Deployment".to_string(),
                name: "nginx".to_string(),
                namespace: "default".to_string(),
            },
            update_type: UpdateType::Image,
            container_name: Some("nginx".to_string()),
            current_image: "nginx:1.25.0".to_string(),
            new_image: "nginx:1.26.0".to_string(),
            policy: UpdatePolicyType::Minor,
            reason: Some("New minor version available".to_string()),
            require_approval: true,
            expires_at: None,
        };

        assert_eq!(spec.target_ref.name, "nginx");
        assert_eq!(spec.update_type, UpdateType::Image);
        assert_eq!(spec.policy, UpdatePolicyType::Minor);
    }

    #[test]
    fn test_update_phase_default() {
        let status = UpdateRequestStatus::default();
        assert_eq!(status.phase, UpdatePhase::Pending);
    }

    #[test]
    fn test_require_approval_default() {
        assert!(default_require_approval());
    }
}
