use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRequest {
    pub id: String,
    pub namespace: String,
    pub resource_name: String,
    pub resource_kind: ResourceKind,
    pub current_image: String,
    pub new_image: String,
    pub created_at: DateTime<Utc>,
    pub status: UpdateStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceKind {
    Deployment,
    StatefulSet,
    DaemonSet,
    HelmRelease,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateStatus {
    PendingApproval,
    Approved,
    Rejected,
    Applied,
    Failed { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub update_id: String,
    pub approved: bool,
    pub approver: Option<String>,
    pub reason: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateEvent {
    pub update_id: String,
    pub event_type: EventType,
    pub timestamp: DateTime<Utc>,
    pub message: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventType {
    Created,
    Approved,
    Rejected,
    Applied,
    Failed,
}
