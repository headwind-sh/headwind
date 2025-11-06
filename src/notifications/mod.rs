use crate::metrics;
use anyhow::Result;
use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{error, info};

mod slack;
mod teams;
mod webhook;

pub use slack::SlackNotifier;
pub use teams::TeamsNotifier;
pub use webhook::WebhookNotifier;

/// Notification event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationEvent {
    /// New version detected but not yet created as UpdateRequest
    UpdateDetected,
    /// UpdateRequest CRD created
    UpdateRequestCreated,
    /// Update approved by user
    UpdateApproved,
    /// Update rejected by user
    UpdateRejected,
    /// Update successfully applied
    UpdateCompleted,
    /// Update failed to apply
    UpdateFailed,
    /// Automatic rollback triggered
    RollbackTriggered,
    /// Rollback completed successfully
    RollbackCompleted,
    /// Rollback failed
    RollbackFailed,
}

impl NotificationEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UpdateDetected => "update.detected",
            Self::UpdateRequestCreated => "update.request.created",
            Self::UpdateApproved => "update.approved",
            Self::UpdateRejected => "update.rejected",
            Self::UpdateCompleted => "update.completed",
            Self::UpdateFailed => "update.failed",
            Self::RollbackTriggered => "rollback.triggered",
            Self::RollbackCompleted => "rollback.completed",
            Self::RollbackFailed => "rollback.failed",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            Self::UpdateDetected => "🔔",
            Self::UpdateRequestCreated => "📦",
            Self::UpdateApproved => "✅",
            Self::UpdateRejected => "❌",
            Self::UpdateCompleted => "🎉",
            Self::UpdateFailed => "⚠️",
            Self::RollbackTriggered => "🔄",
            Self::RollbackCompleted => "✅",
            Self::RollbackFailed => "💥",
        }
    }

    pub fn color(&self) -> &'static str {
        match self {
            Self::UpdateDetected => "#2196F3",       // Blue
            Self::UpdateRequestCreated => "#9C27B0", // Purple
            Self::UpdateApproved => "#4CAF50",       // Green
            Self::UpdateRejected => "#F44336",       // Red
            Self::UpdateCompleted => "#4CAF50",      // Green
            Self::UpdateFailed => "#FF9800",         // Orange
            Self::RollbackTriggered => "#FF9800",    // Orange
            Self::RollbackCompleted => "#4CAF50",    // Green
            Self::RollbackFailed => "#F44336",       // Red
        }
    }
}

/// Notification payload containing event details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationPayload {
    pub event: NotificationEvent,
    pub timestamp: DateTime<Utc>,
    pub deployment: DeploymentInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_approval: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_request_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentInfo {
    pub name: String,
    pub namespace: String,
    pub current_image: String,
    pub new_image: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
}

/// Notification configuration loaded from ConfigMap/Secrets
#[derive(Debug, Clone)]
pub struct NotificationConfig {
    pub slack: SlackConfig,
    pub teams: TeamsConfig,
    pub webhook: WebhookConfig,
}

#[derive(Debug, Clone, Default)]
pub struct SlackConfig {
    pub enabled: bool,
    pub webhook_url: Option<String>,
    pub channel: Option<String>,
    pub username: Option<String>,
    pub icon_emoji: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TeamsConfig {
    pub enabled: bool,
    pub webhook_url: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct WebhookConfig {
    pub enabled: bool,
    pub url: Option<String>,
    pub secret: Option<String>,
    pub timeout_seconds: u64,
    pub max_retries: u32,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            slack: SlackConfig::default(),
            teams: TeamsConfig::default(),
            webhook: WebhookConfig {
                enabled: false,
                url: None,
                secret: None,
                timeout_seconds: 10,
                max_retries: 3,
            },
        }
    }
}

impl NotificationConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        Self {
            slack: SlackConfig::from_env(),
            teams: TeamsConfig::from_env(),
            webhook: WebhookConfig::from_env(),
        }
    }

    /// Load configuration from a Kubernetes ConfigMap
    /// Falls back to environment variables for any missing values
    pub async fn from_configmap(client: kube::Client, name: &str, namespace: &str) -> Result<Self> {
        use k8s_openapi::api::core::v1::ConfigMap;
        use kube::Api;

        let config_maps: Api<ConfigMap> = Api::namespaced(client, namespace);

        match config_maps.get(name).await {
            Ok(configmap) => {
                let data = configmap.data.unwrap_or_default();

                // Parse YAML configuration from ConfigMap
                if let Some(config_yaml) = data.get("notifications.yaml") {
                    match serde_yaml::from_str::<ConfigMapNotificationConfig>(config_yaml) {
                        Ok(cm_config) => {
                            info!(
                                "Loaded notification configuration from ConfigMap {}/{}",
                                namespace, name
                            );
                            return Ok(Self::from_configmap_config(cm_config));
                        },
                        Err(e) => {
                            error!(
                                "Failed to parse notifications.yaml from ConfigMap: {}. Falling back to environment variables.",
                                e
                            );
                        },
                    }
                }

                // Fall back to loading individual keys from ConfigMap data
                info!(
                    "ConfigMap {}/{} found but no notifications.yaml key. Falling back to environment variables.",
                    namespace, name
                );
                Ok(Self::from_env())
            },
            Err(e) => {
                info!(
                    "ConfigMap {}/{} not found: {}. Using environment variables.",
                    namespace, name, e
                );
                Ok(Self::from_env())
            },
        }
    }

    /// Convert ConfigMap config to NotificationConfig
    fn from_configmap_config(cm_config: ConfigMapNotificationConfig) -> Self {
        Self {
            slack: SlackConfig::from_configmap_config(cm_config.slack),
            teams: TeamsConfig::from_configmap_config(cm_config.teams),
            webhook: WebhookConfig::from_configmap_config(cm_config.webhook),
        }
    }

    /// Check if any notification channels are enabled
    pub fn has_enabled_channels(&self) -> bool {
        self.slack.enabled || self.teams.enabled || self.webhook.enabled
    }
}

/// Configuration structure for deserializing from ConfigMap YAML
#[derive(Debug, Clone, Deserialize)]
struct ConfigMapNotificationConfig {
    #[serde(default)]
    slack: Option<ConfigMapSlackConfig>,
    #[serde(default)]
    teams: Option<ConfigMapTeamsConfig>,
    #[serde(default)]
    webhook: Option<ConfigMapWebhookConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct ConfigMapSlackConfig {
    enabled: Option<bool>,
    webhook_url: Option<String>,
    channel: Option<String>,
    username: Option<String>,
    icon_emoji: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ConfigMapTeamsConfig {
    enabled: Option<bool>,
    webhook_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ConfigMapWebhookConfig {
    enabled: Option<bool>,
    url: Option<String>,
    secret: Option<String>,
    timeout_seconds: Option<u64>,
    max_retries: Option<u32>,
}

impl SlackConfig {
    /// Load Slack configuration from environment variables
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("SLACK_ENABLED")
                .unwrap_or_default()
                .parse()
                .unwrap_or(false),
            webhook_url: std::env::var("SLACK_WEBHOOK_URL").ok(),
            channel: std::env::var("SLACK_CHANNEL").ok(),
            username: std::env::var("SLACK_USERNAME").ok(),
            icon_emoji: std::env::var("SLACK_ICON_EMOJI").ok(),
        }
    }

    /// Load Slack configuration from ConfigMap, falling back to environment variables
    fn from_configmap_config(cm_config: Option<ConfigMapSlackConfig>) -> Self {
        if let Some(cm) = cm_config {
            Self {
                enabled: cm.enabled.unwrap_or_else(|| {
                    std::env::var("SLACK_ENABLED")
                        .unwrap_or_default()
                        .parse()
                        .unwrap_or(false)
                }),
                webhook_url: cm
                    .webhook_url
                    .or_else(|| std::env::var("SLACK_WEBHOOK_URL").ok()),
                channel: cm.channel.or_else(|| std::env::var("SLACK_CHANNEL").ok()),
                username: cm.username.or_else(|| std::env::var("SLACK_USERNAME").ok()),
                icon_emoji: cm
                    .icon_emoji
                    .or_else(|| std::env::var("SLACK_ICON_EMOJI").ok()),
            }
        } else {
            Self::from_env()
        }
    }
}

impl TeamsConfig {
    /// Load Teams configuration from environment variables
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("TEAMS_ENABLED")
                .unwrap_or_default()
                .parse()
                .unwrap_or(false),
            webhook_url: std::env::var("TEAMS_WEBHOOK_URL").ok(),
        }
    }

    /// Load Teams configuration from ConfigMap, falling back to environment variables
    fn from_configmap_config(cm_config: Option<ConfigMapTeamsConfig>) -> Self {
        if let Some(cm) = cm_config {
            Self {
                enabled: cm.enabled.unwrap_or_else(|| {
                    std::env::var("TEAMS_ENABLED")
                        .unwrap_or_default()
                        .parse()
                        .unwrap_or(false)
                }),
                webhook_url: cm
                    .webhook_url
                    .or_else(|| std::env::var("TEAMS_WEBHOOK_URL").ok()),
            }
        } else {
            Self::from_env()
        }
    }
}

impl WebhookConfig {
    /// Load webhook configuration from environment variables
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("WEBHOOK_ENABLED")
                .unwrap_or_default()
                .parse()
                .unwrap_or(false),
            url: std::env::var("WEBHOOK_URL").ok(),
            secret: std::env::var("WEBHOOK_SECRET").ok(),
            timeout_seconds: std::env::var("WEBHOOK_TIMEOUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            max_retries: std::env::var("WEBHOOK_MAX_RETRIES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3),
        }
    }

    /// Load webhook configuration from ConfigMap, falling back to environment variables
    fn from_configmap_config(cm_config: Option<ConfigMapWebhookConfig>) -> Self {
        if let Some(cm) = cm_config {
            Self {
                enabled: cm.enabled.unwrap_or_else(|| {
                    std::env::var("WEBHOOK_ENABLED")
                        .unwrap_or_default()
                        .parse()
                        .unwrap_or(false)
                }),
                url: cm.url.or_else(|| std::env::var("WEBHOOK_URL").ok()),
                secret: cm.secret.or_else(|| std::env::var("WEBHOOK_SECRET").ok()),
                timeout_seconds: cm.timeout_seconds.unwrap_or_else(|| {
                    std::env::var("WEBHOOK_TIMEOUT")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(10)
                }),
                max_retries: cm.max_retries.unwrap_or_else(|| {
                    std::env::var("WEBHOOK_MAX_RETRIES")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(3)
                }),
            }
        } else {
            Self::from_env()
        }
    }
}

/// Notifier trait for different notification backends
#[async_trait::async_trait]
pub trait Notifier: Send + Sync {
    async fn send(&self, payload: &NotificationPayload) -> Result<()>;
    fn name(&self) -> &'static str;
    fn is_enabled(&self) -> bool;
}

/// Main notification manager that coordinates all notifiers
pub struct NotificationManager {
    notifiers: Vec<Box<dyn Notifier>>,
}

impl NotificationManager {
    pub fn new(config: NotificationConfig) -> Self {
        let mut notifiers: Vec<Box<dyn Notifier>> = Vec::new();

        // Add Slack notifier if enabled
        if config.slack.enabled {
            match SlackNotifier::new(config.slack.clone()) {
                Ok(notifier) => notifiers.push(Box::new(notifier)),
                Err(e) => error!("Failed to create Slack notifier: {}", e),
            }
        }

        // Add Teams notifier if enabled
        if config.teams.enabled {
            match TeamsNotifier::new(config.teams.clone()) {
                Ok(notifier) => notifiers.push(Box::new(notifier)),
                Err(e) => error!("Failed to create Teams notifier: {}", e),
            }
        }

        // Add webhook notifier if enabled
        if config.webhook.enabled {
            match WebhookNotifier::new(config.webhook.clone()) {
                Ok(notifier) => notifiers.push(Box::new(notifier)),
                Err(e) => error!("Failed to create webhook notifier: {}", e),
            }
        }

        info!(
            "Notification manager initialized with {} notifiers",
            notifiers.len()
        );

        Self { notifiers }
    }

    /// Send notification to all enabled notifiers
    pub async fn notify(&self, payload: &NotificationPayload) {
        if self.notifiers.is_empty() {
            return;
        }

        info!(
            "Sending notification: {} for {}/{}",
            payload.event.as_str(),
            payload.deployment.namespace,
            payload.deployment.name
        );

        for notifier in &self.notifiers {
            if !notifier.is_enabled() {
                continue;
            }

            match notifier.send(payload).await {
                Ok(()) => {
                    info!("Notification sent successfully via {}", notifier.name());
                    metrics::NOTIFICATIONS_SENT_TOTAL.inc();

                    // Increment per-channel metrics
                    match notifier.name() {
                        "Slack" => metrics::NOTIFICATIONS_SLACK_SENT.inc(),
                        "Microsoft Teams" => metrics::NOTIFICATIONS_TEAMS_SENT.inc(),
                        "Webhook" => metrics::NOTIFICATIONS_WEBHOOK_SENT.inc(),
                        _ => {},
                    }
                },
                Err(e) => {
                    error!("Failed to send notification via {}: {}", notifier.name(), e);
                    metrics::NOTIFICATIONS_FAILED_TOTAL.inc();
                },
            }
        }
    }

    /// Check if any notifiers are enabled
    pub fn has_enabled_notifiers(&self) -> bool {
        self.notifiers.iter().any(|n| n.is_enabled())
    }

    /// Get count of enabled notifiers
    pub fn enabled_count(&self) -> usize {
        self.notifiers.iter().filter(|n| n.is_enabled()).count()
    }
}

impl NotificationPayload {
    pub fn new(event: NotificationEvent, deployment: DeploymentInfo) -> Self {
        Self {
            event,
            timestamp: Utc::now(),
            deployment,
            policy: None,
            requires_approval: None,
            approval_url: None,
            approved_by: None,
            rejection_reason: None,
            error_message: None,
            update_request_name: None,
            metadata: None,
        }
    }

    pub fn with_policy(mut self, policy: impl Into<String>) -> Self {
        self.policy = Some(policy.into());
        self
    }

    pub fn with_requires_approval(mut self, requires: bool) -> Self {
        self.requires_approval = Some(requires);
        self
    }

    pub fn with_approval_url(mut self, url: impl Into<String>) -> Self {
        self.approval_url = Some(url.into());
        self
    }

    pub fn with_approved_by(mut self, approver: impl Into<String>) -> Self {
        self.approved_by = Some(approver.into());
        self
    }

    pub fn with_rejection_reason(mut self, reason: impl Into<String>) -> Self {
        self.rejection_reason = Some(reason.into());
        self
    }

    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error_message = Some(error.into());
        self
    }

    pub fn with_update_request(mut self, name: impl Into<String>) -> Self {
        self.update_request_name = Some(name.into());
        self
    }

    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Generate a human-readable title for the notification
    pub fn title(&self) -> String {
        match self.event {
            NotificationEvent::UpdateDetected => {
                format!(
                    "New version detected: {}/{}",
                    self.deployment.namespace, self.deployment.name
                )
            },
            NotificationEvent::UpdateRequestCreated => {
                format!(
                    "Update request created: {}/{}",
                    self.deployment.namespace, self.deployment.name
                )
            },
            NotificationEvent::UpdateApproved => {
                format!(
                    "Update approved: {}/{}",
                    self.deployment.namespace, self.deployment.name
                )
            },
            NotificationEvent::UpdateRejected => {
                format!(
                    "Update rejected: {}/{}",
                    self.deployment.namespace, self.deployment.name
                )
            },
            NotificationEvent::UpdateCompleted => {
                format!(
                    "Update completed: {}/{}",
                    self.deployment.namespace, self.deployment.name
                )
            },
            NotificationEvent::UpdateFailed => {
                format!(
                    "Update failed: {}/{}",
                    self.deployment.namespace, self.deployment.name
                )
            },
            NotificationEvent::RollbackTriggered => {
                format!(
                    "Rollback triggered: {}/{}",
                    self.deployment.namespace, self.deployment.name
                )
            },
            NotificationEvent::RollbackCompleted => {
                format!(
                    "Rollback completed: {}/{}",
                    self.deployment.namespace, self.deployment.name
                )
            },
            NotificationEvent::RollbackFailed => {
                format!(
                    "Rollback failed: {}/{}",
                    self.deployment.namespace, self.deployment.name
                )
            },
        }
    }

    /// Generate a human-readable description for the notification
    pub fn description(&self) -> String {
        let mut desc = format!(
            "Image update: `{}` → `{}`",
            self.deployment.current_image, self.deployment.new_image
        );

        if let Some(policy) = &self.policy {
            desc.push_str(&format!("\nPolicy: {}", policy));
        }

        if let Some(approver) = &self.approved_by {
            desc.push_str(&format!("\nApproved by: {}", approver));
        }

        if let Some(reason) = &self.rejection_reason {
            desc.push_str(&format!("\nReason: {}", reason));
        }

        if let Some(error) = &self.error_message {
            desc.push_str(&format!("\nError: {}", error));
        }

        desc
    }
}

// Global notification manager instance
lazy_static! {
    static ref GLOBAL_NOTIFIER: RwLock<Option<Arc<NotificationManager>>> = RwLock::new(None);
}

/// Initialize the global notification manager
pub fn init_notifications() {
    let config = NotificationConfig::from_env();
    let manager = Arc::new(NotificationManager::new(config));

    let mut global = GLOBAL_NOTIFIER.write().unwrap();
    *global = Some(manager);
}

/// Send a notification using the global notification manager
/// This is a fire-and-forget operation - notifications are sent in the background
pub fn notify(payload: NotificationPayload) {
    let notifier = GLOBAL_NOTIFIER.read().unwrap().clone();

    if let Some(manager) = notifier {
        // Spawn a background task to send notifications asynchronously
        tokio::spawn(async move {
            manager.notify(&payload).await;
        });
    }
}

/// Helper function to send update detected notification
pub fn notify_update_detected(deployment: DeploymentInfo) {
    let payload = NotificationPayload::new(NotificationEvent::UpdateDetected, deployment);
    notify(payload);
}

/// Helper function to send UpdateRequest created notification
pub fn notify_update_request_created(
    deployment: DeploymentInfo,
    policy: String,
    requires_approval: bool,
    update_request_name: String,
) {
    let mut payload =
        NotificationPayload::new(NotificationEvent::UpdateRequestCreated, deployment.clone())
            .with_policy(policy)
            .with_requires_approval(requires_approval)
            .with_update_request(update_request_name);

    // Add approval URL if requires_approval is true
    if requires_approval {
        // Use environment variable or default to localhost for testing
        let base_url = std::env::var("HEADWIND_API_URL")
            .unwrap_or_else(|_| "http://localhost:8081".to_string());
        let approval_url = format!(
            "{}/api/v1/updates/{}/{}/approve",
            base_url,
            deployment.namespace,
            payload
                .update_request_name
                .as_ref()
                .unwrap_or(&"unknown".to_string())
        );
        payload = payload.with_approval_url(approval_url);
    }

    notify(payload);
}

/// Helper function to send approval notification
pub fn notify_update_approved(
    deployment: DeploymentInfo,
    approved_by: String,
    update_request_name: String,
) {
    let payload = NotificationPayload::new(NotificationEvent::UpdateApproved, deployment)
        .with_approved_by(approved_by)
        .with_update_request(update_request_name);

    notify(payload);
}

/// Helper function to send rejection notification
pub fn notify_update_rejected(
    deployment: DeploymentInfo,
    rejected_by: String,
    reason: String,
    update_request_name: String,
) {
    let payload = NotificationPayload::new(NotificationEvent::UpdateRejected, deployment)
        .with_approved_by(rejected_by)
        .with_rejection_reason(reason)
        .with_update_request(update_request_name);

    notify(payload);
}

/// Helper function to send update completed notification
pub fn notify_update_completed(deployment: DeploymentInfo) {
    let payload = NotificationPayload::new(NotificationEvent::UpdateCompleted, deployment);
    notify(payload);
}

/// Helper function to send update failed notification
pub fn notify_update_failed(deployment: DeploymentInfo, error: String) {
    let payload =
        NotificationPayload::new(NotificationEvent::UpdateFailed, deployment).with_error(error);
    notify(payload);
}

/// Helper function to send rollback triggered notification
pub fn notify_rollback_triggered(deployment: DeploymentInfo, reason: String) {
    let payload = NotificationPayload::new(NotificationEvent::RollbackTriggered, deployment)
        .with_error(reason);
    notify(payload);
}

/// Helper function to send rollback completed notification
pub fn notify_rollback_completed(deployment: DeploymentInfo) {
    let payload = NotificationPayload::new(NotificationEvent::RollbackCompleted, deployment);
    notify(payload);
}

/// Helper function to send rollback failed notification
pub fn notify_rollback_failed(deployment: DeploymentInfo, error: String) {
    let payload =
        NotificationPayload::new(NotificationEvent::RollbackFailed, deployment).with_error(error);
    notify(payload);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_event_string() {
        assert_eq!(
            NotificationEvent::UpdateDetected.as_str(),
            "update.detected"
        );
        assert_eq!(
            NotificationEvent::UpdateApproved.as_str(),
            "update.approved"
        );
    }

    #[test]
    fn test_notification_payload_builder() {
        let deployment = DeploymentInfo {
            name: "test-deploy".to_string(),
            namespace: "default".to_string(),
            current_image: "nginx:1.25.0".to_string(),
            new_image: "nginx:1.26.0".to_string(),
            container: Some("nginx".to_string()),
        };

        let payload = NotificationPayload::new(NotificationEvent::UpdateDetected, deployment)
            .with_policy("minor")
            .with_requires_approval(true);

        assert_eq!(payload.event, NotificationEvent::UpdateDetected);
        assert_eq!(payload.policy, Some("minor".to_string()));
        assert_eq!(payload.requires_approval, Some(true));
    }

    #[test]
    fn test_payload_title() {
        let deployment = DeploymentInfo {
            name: "nginx".to_string(),
            namespace: "production".to_string(),
            current_image: "nginx:1.25.0".to_string(),
            new_image: "nginx:1.26.0".to_string(),
            container: None,
        };

        let payload = NotificationPayload::new(NotificationEvent::UpdateApproved, deployment);
        assert_eq!(payload.title(), "Update approved: production/nginx");
    }

    #[test]
    fn test_payload_description() {
        let deployment = DeploymentInfo {
            name: "nginx".to_string(),
            namespace: "production".to_string(),
            current_image: "nginx:1.25.0".to_string(),
            new_image: "nginx:1.26.0".to_string(),
            container: None,
        };

        let payload = NotificationPayload::new(NotificationEvent::UpdateApproved, deployment)
            .with_policy("minor")
            .with_approved_by("admin@example.com");

        let desc = payload.description();
        assert!(desc.contains("nginx:1.25.0"));
        assert!(desc.contains("nginx:1.26.0"));
        assert!(desc.contains("Policy: minor"));
        assert!(desc.contains("Approved by: admin@example.com"));
    }
}
