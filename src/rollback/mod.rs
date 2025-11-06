// Rollback functionality for Headwind
//
// This module provides rollback capabilities for deployments by:
// 1. Tracking update history in deployment annotations
// 2. Allowing manual rollback to previous image versions
// 3. Creating UpdateRequests for rollback operations

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Annotation key for storing update history
pub const HISTORY_ANNOTATION: &str = "headwind.sh/update-history";

/// Maximum number of history entries to keep per container
pub const MAX_HISTORY_ENTRIES: usize = 10;

/// Represents a single update in the history
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateHistoryEntry {
    /// Container name that was updated
    pub container: String,

    /// Image that was deployed
    pub image: String,

    /// Timestamp of the update
    pub timestamp: DateTime<Utc>,

    /// Name of the UpdateRequest that performed this update
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_request_name: Option<String>,

    /// User or system that approved the update
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by: Option<String>,
}

/// Update history for a deployment
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateHistory {
    /// List of updates, newest first
    entries: Vec<UpdateHistoryEntry>,
}

impl UpdateHistory {
    /// Create a new empty update history
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Parse update history from deployment annotations
    pub fn from_deployment(deployment: &Deployment) -> Result<Self> {
        let annotations = deployment
            .metadata
            .annotations
            .as_ref()
            .ok_or_else(|| anyhow!("Deployment has no annotations"))?;

        match annotations.get(HISTORY_ANNOTATION) {
            Some(json_str) => {
                let entries: Vec<UpdateHistoryEntry> = serde_json::from_str(json_str)
                    .context("Failed to parse update history from annotation")?;
                Ok(Self { entries })
            },
            None => Ok(Self::new()),
        }
    }

    /// Add a new update entry to the history
    pub fn add_entry(&mut self, entry: UpdateHistoryEntry) {
        // Add to the front (newest first)
        self.entries.insert(0, entry);

        // Trim to max size per container
        self.trim_history();
    }

    /// Trim history to keep only MAX_HISTORY_ENTRIES per container
    fn trim_history(&mut self) {
        let mut container_counts: BTreeMap<String, usize> = BTreeMap::new();

        self.entries.retain(|entry| {
            let count = container_counts.entry(entry.container.clone()).or_insert(0);
            *count += 1;
            *count <= MAX_HISTORY_ENTRIES
        });
    }

    /// Get all entries for a specific container
    pub fn get_container_history(&self, container: &str) -> Vec<&UpdateHistoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.container == container)
            .collect()
    }

    /// Get the previous image for a container (the one before current)
    pub fn get_previous_image(&self, container: &str) -> Option<&UpdateHistoryEntry> {
        self.get_container_history(container).get(1).copied()
    }

    /// Get a specific historical entry by index (0 = current, 1 = previous, etc.)
    pub fn get_entry_by_index(&self, container: &str, index: usize) -> Option<&UpdateHistoryEntry> {
        self.get_container_history(container).get(index).copied()
    }

    /// Get all entries
    pub fn entries(&self) -> &[UpdateHistoryEntry] {
        &self.entries
    }

    /// Serialize to JSON string for annotation
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(&self.entries).context("Failed to serialize update history")
    }
}

/// Rollback manager for handling rollback operations
pub struct RollbackManager {
    client: Client,
}

impl RollbackManager {
    /// Create a new rollback manager
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Track an update in the deployment history
    pub async fn track_update(
        &self,
        deployment_name: &str,
        namespace: &str,
        container: &str,
        new_image: &str,
        update_request_name: Option<String>,
        approved_by: Option<String>,
    ) -> Result<()> {
        let deployments: Api<Deployment> = Api::namespaced(self.client.clone(), namespace);
        let deployment = deployments
            .get(deployment_name)
            .await
            .context("Failed to get deployment")?;

        let mut history =
            UpdateHistory::from_deployment(&deployment).unwrap_or_else(|_| UpdateHistory::new());

        let entry = UpdateHistoryEntry {
            container: container.to_string(),
            image: new_image.to_string(),
            timestamp: Utc::now(),
            update_request_name,
            approved_by,
        };

        history.add_entry(entry);

        // Update deployment annotation
        let history_json = history.to_json()?;
        let mut annotations = deployment.metadata.annotations.clone().unwrap_or_default();
        annotations.insert(HISTORY_ANNOTATION.to_string(), history_json);

        // Patch the deployment with new annotation
        let patch = serde_json::json!({
            "metadata": {
                "annotations": annotations
            }
        });

        deployments
            .patch(
                deployment_name,
                &kube::api::PatchParams::default(),
                &kube::api::Patch::Strategic(patch),
            )
            .await
            .context("Failed to update deployment annotations")?;

        info!(
            deployment = deployment_name,
            namespace = namespace,
            container = container,
            image = new_image,
            "Tracked update in deployment history"
        );

        Ok(())
    }

    /// Get update history for a deployment
    pub async fn get_history(
        &self,
        deployment_name: &str,
        namespace: &str,
    ) -> Result<UpdateHistory> {
        let deployments: Api<Deployment> = Api::namespaced(self.client.clone(), namespace);
        let deployment = deployments
            .get(deployment_name)
            .await
            .context("Failed to get deployment")?;

        UpdateHistory::from_deployment(&deployment).or_else(|_| Ok(UpdateHistory::new()))
    }

    /// Get the previous image for a container in a deployment
    pub async fn get_previous_image(
        &self,
        deployment_name: &str,
        namespace: &str,
        container: &str,
    ) -> Result<Option<String>> {
        let history = self.get_history(deployment_name, namespace).await?;

        Ok(history
            .get_previous_image(container)
            .map(|entry| entry.image.clone()))
    }

    /// Get a specific image from history by index
    pub async fn get_image_by_index(
        &self,
        deployment_name: &str,
        namespace: &str,
        container: &str,
        index: usize,
    ) -> Result<Option<String>> {
        let history = self.get_history(deployment_name, namespace).await?;

        Ok(history
            .get_entry_by_index(container, index)
            .map(|entry| entry.image.clone()))
    }
}

/// Health status of a deployment
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    /// Deployment is healthy - all pods are ready
    Healthy,
    /// Deployment is progressing - pods are starting/updating
    Progressing,
    /// Deployment has failed - pods are crash looping or not becoming ready
    Failed(String),
    /// Health check timed out
    Timeout,
}

/// Configuration for automatic rollback
#[derive(Debug, Clone)]
pub struct AutoRollbackConfig {
    /// Enable automatic rollback
    pub enabled: bool,
    /// Timeout for health checks (seconds)
    pub timeout: u64,
    /// Number of health check retries before rolling back
    pub retries: u32,
}

impl Default for AutoRollbackConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout: 300, // 5 minutes
            retries: 3,
        }
    }
}

impl AutoRollbackConfig {
    /// Parse config from deployment annotations
    pub fn from_annotations(annotations: &BTreeMap<String, String>) -> Self {
        use crate::models::policy::annotations;

        let enabled = annotations
            .get(annotations::AUTO_ROLLBACK)
            .and_then(|v| v.parse().ok())
            .unwrap_or(false);

        let timeout = annotations
            .get(annotations::ROLLBACK_TIMEOUT)
            .and_then(|v| v.parse().ok())
            .unwrap_or(300);

        let retries = annotations
            .get(annotations::HEALTH_CHECK_RETRIES)
            .and_then(|v| v.parse().ok())
            .unwrap_or(3);

        Self {
            enabled,
            timeout,
            retries,
        }
    }
}

/// Health checker for deployments
pub struct HealthChecker {
    client: Client,
}

impl HealthChecker {
    /// Create a new health checker
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Check the health of a deployment
    pub async fn check_deployment_health(
        &self,
        deployment_name: &str,
        namespace: &str,
    ) -> Result<HealthStatus> {
        let deployments: Api<Deployment> = Api::namespaced(self.client.clone(), namespace);
        let deployment = deployments.get(deployment_name).await?;

        // Check deployment status
        let status = deployment
            .status
            .as_ref()
            .ok_or_else(|| anyhow!("Deployment has no status"))?;

        // Check for ProgressDeadlineExceeded condition
        if let Some(conditions) = &status.conditions {
            for condition in conditions {
                if condition.type_ == "Progressing"
                    && condition.status == "False"
                    && condition.reason.as_deref() == Some("ProgressDeadlineExceeded")
                {
                    return Ok(HealthStatus::Failed(
                        "Deployment progress deadline exceeded".to_string(),
                    ));
                }
            }
        }

        let replicas = status.replicas.unwrap_or(0);
        let ready_replicas = status.ready_replicas.unwrap_or(0);
        let updated_replicas = status.updated_replicas.unwrap_or(0);

        debug!(
            "Deployment {}/{}: replicas={}, ready={}, updated={}",
            namespace, deployment_name, replicas, ready_replicas, updated_replicas
        );

        // Check if all replicas are ready
        if ready_replicas == replicas && replicas > 0 {
            // All pods are ready, now check if any are crash looping
            let pod_status = self.check_pod_health(deployment_name, namespace).await?;
            if pod_status == HealthStatus::Healthy {
                return Ok(HealthStatus::Healthy);
            }
            return Ok(pod_status);
        }

        // Check if deployment is still progressing
        if updated_replicas < replicas {
            return Ok(HealthStatus::Progressing);
        }

        // Check pod health for more details
        self.check_pod_health(deployment_name, namespace).await
    }

    /// Check the health of pods for a deployment
    async fn check_pod_health(
        &self,
        deployment_name: &str,
        namespace: &str,
    ) -> Result<HealthStatus> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);

        // List pods with deployment selector
        let deployments: Api<Deployment> = Api::namespaced(self.client.clone(), namespace);
        let deployment = deployments.get(deployment_name).await?;

        let selector = deployment
            .spec
            .as_ref()
            .and_then(|s| s.selector.match_labels.as_ref())
            .ok_or_else(|| anyhow!("Deployment has no selector"))?;

        let label_selector = selector
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(",");

        let lp = kube::api::ListParams::default().labels(&label_selector);
        let pod_list = pods.list(&lp).await?;

        if pod_list.items.is_empty() {
            return Ok(HealthStatus::Progressing);
        }

        // Check for crash looping or failing pods
        for pod in &pod_list.items {
            if let Some(status) = &pod.status {
                // Check container statuses
                if let Some(container_statuses) = &status.container_statuses {
                    for container_status in container_statuses {
                        // Check for CrashLoopBackOff
                        if let Some(waiting) = &container_status
                            .state
                            .as_ref()
                            .and_then(|s| s.waiting.as_ref())
                        {
                            if waiting.reason.as_deref() == Some("CrashLoopBackOff") {
                                return Ok(HealthStatus::Failed(format!(
                                    "Container {} is in CrashLoopBackOff",
                                    container_status.name
                                )));
                            }
                            if waiting.reason.as_deref() == Some("ImagePullBackOff")
                                || waiting.reason.as_deref() == Some("ErrImagePull")
                            {
                                return Ok(HealthStatus::Failed(format!(
                                    "Container {} failed to pull image",
                                    container_status.name
                                )));
                            }
                        }

                        // Check restart count
                        if container_status.restart_count > 5 {
                            warn!(
                                "Container {} has high restart count: {}",
                                container_status.name, container_status.restart_count
                            );
                            return Ok(HealthStatus::Failed(format!(
                                "Container {} has high restart count ({})",
                                container_status.name, container_status.restart_count
                            )));
                        }

                        // Check if not ready
                        if !container_status.ready {
                            debug!("Container {} is not ready", container_status.name);
                            return Ok(HealthStatus::Progressing);
                        }
                    }
                }
            }
        }

        Ok(HealthStatus::Healthy)
    }

    /// Monitor deployment health with timeout and retries
    pub async fn monitor_deployment_health(
        &self,
        deployment_name: &str,
        namespace: &str,
        config: &AutoRollbackConfig,
    ) -> Result<HealthStatus> {
        let timeout_duration = Duration::from_secs(config.timeout);
        let check_interval = Duration::from_secs(10); // Check every 10 seconds
        let start = std::time::Instant::now();
        let mut consecutive_failures = 0;

        info!(
            "Monitoring health of {}/{} (timeout: {}s, retries: {})",
            namespace, deployment_name, config.timeout, config.retries
        );

        loop {
            // Check if timeout exceeded
            if start.elapsed() > timeout_duration {
                warn!(
                    "Health check timeout for {}/{} after {}s",
                    namespace, deployment_name, config.timeout
                );
                return Ok(HealthStatus::Timeout);
            }

            // Check health
            match self
                .check_deployment_health(deployment_name, namespace)
                .await
            {
                Ok(HealthStatus::Healthy) => {
                    info!("Deployment {}/{} is healthy", namespace, deployment_name);
                    return Ok(HealthStatus::Healthy);
                },
                Ok(HealthStatus::Failed(reason)) => {
                    consecutive_failures += 1;
                    error!(
                        "Deployment {}/{} health check failed ({}/{}): {}",
                        namespace, deployment_name, consecutive_failures, config.retries, reason
                    );

                    if consecutive_failures >= config.retries {
                        return Ok(HealthStatus::Failed(reason));
                    }
                },
                Ok(HealthStatus::Progressing) => {
                    consecutive_failures = 0; // Reset on progressing
                    debug!(
                        "Deployment {}/{} is still progressing...",
                        namespace, deployment_name
                    );
                },
                Ok(HealthStatus::Timeout) => {
                    return Ok(HealthStatus::Timeout);
                },
                Err(e) => {
                    error!(
                        "Error checking health of {}/{}: {}",
                        namespace, deployment_name, e
                    );
                    // Don't count API errors as health failures
                },
            }

            // Wait before next check
            tokio::time::sleep(check_interval).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_history_new() {
        let history = UpdateHistory::new();
        assert_eq!(history.entries().len(), 0);
    }

    #[test]
    fn test_add_entry() {
        let mut history = UpdateHistory::new();

        let entry = UpdateHistoryEntry {
            container: "nginx".to_string(),
            image: "nginx:1.26.0".to_string(),
            timestamp: Utc::now(),
            update_request_name: Some("nginx-update".to_string()),
            approved_by: Some("admin".to_string()),
        };

        history.add_entry(entry.clone());

        assert_eq!(history.entries().len(), 1);
        assert_eq!(history.entries()[0], entry);
    }

    #[test]
    fn test_get_container_history() {
        let mut history = UpdateHistory::new();

        let entry1 = UpdateHistoryEntry {
            container: "nginx".to_string(),
            image: "nginx:1.26.0".to_string(),
            timestamp: Utc::now(),
            update_request_name: None,
            approved_by: None,
        };

        let entry2 = UpdateHistoryEntry {
            container: "nginx".to_string(),
            image: "nginx:1.25.0".to_string(),
            timestamp: Utc::now(),
            update_request_name: None,
            approved_by: None,
        };

        history.add_entry(entry1);
        history.add_entry(entry2);

        let nginx_history = history.get_container_history("nginx");
        assert_eq!(nginx_history.len(), 2);
    }

    #[test]
    fn test_get_previous_image() {
        let mut history = UpdateHistory::new();

        history.add_entry(UpdateHistoryEntry {
            container: "nginx".to_string(),
            image: "nginx:1.26.0".to_string(),
            timestamp: Utc::now(),
            update_request_name: None,
            approved_by: None,
        });

        history.add_entry(UpdateHistoryEntry {
            container: "nginx".to_string(),
            image: "nginx:1.25.0".to_string(),
            timestamp: Utc::now(),
            update_request_name: None,
            approved_by: None,
        });

        let previous = history.get_previous_image("nginx");
        assert!(previous.is_some());
        assert_eq!(previous.unwrap().image, "nginx:1.26.0");
    }

    #[test]
    fn test_trim_history() {
        let mut history = UpdateHistory::new();

        // Add more than MAX_HISTORY_ENTRIES
        for i in 0..15 {
            history.add_entry(UpdateHistoryEntry {
                container: "nginx".to_string(),
                image: format!("nginx:1.{}.0", i),
                timestamp: Utc::now(),
                update_request_name: None,
                approved_by: None,
            });
        }

        let nginx_history = history.get_container_history("nginx");
        assert_eq!(nginx_history.len(), MAX_HISTORY_ENTRIES);
    }

    #[test]
    fn test_json_serialization() {
        let mut history = UpdateHistory::new();

        history.add_entry(UpdateHistoryEntry {
            container: "nginx".to_string(),
            image: "nginx:1.26.0".to_string(),
            timestamp: Utc::now(),
            update_request_name: Some("nginx-update".to_string()),
            approved_by: Some("admin".to_string()),
        });

        let json = history.to_json().unwrap();
        assert!(json.contains("nginx:1.26.0"));
        assert!(json.contains("nginx-update"));
    }
}
