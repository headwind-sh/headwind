use axum::{
    Form,
    extract::{Path, Query},
    http::StatusCode,
    response::{
        IntoResponse, Json,
        sse::{Event, Sse},
    },
};
use chrono::{DateTime, Duration, Utc};
use futures::StreamExt as FuturesStreamExt;
use futures::stream::Stream;
use kube::{Api, Client};
use serde::Deserialize;
use tracing::{error, info};

use crate::config::HeadwindConfig;
use crate::models::crd::UpdateRequest;
use crate::ui::auth::{AuditLogEntry, UserIdentity};

use super::templates::{self, UpdateRequestView};

/// Health check endpoint for the Web UI
/// Returns 200 OK if the UI server is running and can connect to Kubernetes API
/// Returns 503 Service Unavailable if Kubernetes API is unreachable
pub async fn health_check() -> impl IntoResponse {
    match Client::try_default().await {
        Ok(_) => (StatusCode::OK, "OK"),
        Err(e) => {
            error!("Health check failed: Kubernetes API unreachable: {}", e);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "Service Unavailable: Cannot reach Kubernetes API",
            )
        },
    }
}

/// Dashboard route - main page showing all update requests
pub async fn dashboard() -> impl IntoResponse {
    info!("Rendering dashboard");

    // Get Kubernetes client
    let client = Client::try_default()
        .await
        .expect("Failed to create Kubernetes client");

    // Query all UpdateRequest CRDs across all namespaces
    let api: Api<UpdateRequest> = Api::all(client);
    let update_requests = api
        .list(&Default::default())
        .await
        .map(|list| list.items)
        .unwrap_or_else(|e| {
            error!("Failed to list UpdateRequests: {}", e);
            Vec::new()
        });

    // Convert UpdateRequests to view models
    let mut pending_updates = Vec::new();
    let mut completed_updates = Vec::new();

    for ur in update_requests {
        let view = convert_to_view(&ur);

        match view.status.as_str() {
            "Pending" => pending_updates.push(view),
            "Completed" | "Rejected" | "Failed" => completed_updates.push(view),
            _ => pending_updates.push(view), // Default to pending
        }
    }

    templates::dashboard(&pending_updates, &completed_updates)
}

/// Update detail route - show individual update request
pub async fn update_detail(Path((namespace, name)): Path<(String, String)>) -> impl IntoResponse {
    info!("Rendering detail view for {}/{}", namespace, name);

    // Get Kubernetes client
    let client = Client::try_default()
        .await
        .expect("Failed to create Kubernetes client");

    // Get specific UpdateRequest
    let api: Api<UpdateRequest> = Api::namespaced(client, &namespace);
    let update_request = api.get(&name).await.unwrap_or_else(|e| {
        error!("Failed to get UpdateRequest {}/{}: {}", namespace, name, e);
        panic!("UpdateRequest not found");
    });

    let view = convert_to_view(&update_request);

    templates::detail(&view)
}

/// Convert UpdateRequest CRD to view model
fn convert_to_view(ur: &UpdateRequest) -> UpdateRequestView {
    let metadata = &ur.metadata;
    let spec = &ur.spec;
    let status = ur.status.as_ref();

    // Extract current and new versions from images
    let (current_version, new_version) = extract_versions(&spec.current_image, &spec.new_image);

    UpdateRequestView {
        name: metadata.name.clone().unwrap_or_default(),
        namespace: metadata.namespace.clone().unwrap_or_default(),
        resource_kind: spec.target_ref.kind.to_string(),
        resource_name: spec.target_ref.name.clone(),
        current_image: spec.current_image.clone(),
        new_image: spec.new_image.clone(),
        current_version,
        new_version,
        policy: format!("{:?}", spec.policy),
        status: status
            .map(|s| format!("{:?}", s.phase))
            .unwrap_or_else(|| "Pending".to_string()),
        created_at: metadata
            .creation_timestamp
            .as_ref()
            .map(|ts| ts.0.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_default(),
        approved_by: status.and_then(|s| s.approved_by.clone()),
        rejected_by: status.and_then(|s| s.rejected_by.clone()),
        rejection_reason: status.and_then(|s| s.message.clone()),
    }
}

/// Extract version tags from image strings
fn extract_versions(current_image: &str, new_image: &str) -> (String, String) {
    let current_version = current_image
        .split(':')
        .next_back()
        .unwrap_or("unknown")
        .to_string();

    let new_version = new_image
        .split(':')
        .next_back()
        .unwrap_or("unknown")
        .to_string();

    (current_version, new_version)
}

/// Settings page - displays settings management UI
pub async fn settings_page() -> impl IntoResponse {
    info!("Rendering settings page");
    templates::settings()
}

/// Get current settings from ConfigMap and Secret
pub async fn get_settings() -> impl IntoResponse {
    info!("Getting Headwind settings");

    let client = match Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create Kubernetes client: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to connect to Kubernetes API"
                })),
            )
                .into_response();
        },
    };

    match HeadwindConfig::load(client).await {
        Ok(config) => (StatusCode::OK, Json(config)).into_response(),
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to load configuration: {}", e)
                })),
            )
                .into_response()
        },
    }
}

/// Update settings in ConfigMap and Secret
pub async fn update_settings(Json(config): Json<HeadwindConfig>) -> impl IntoResponse {
    info!("Updating Headwind settings");

    let client = match Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create Kubernetes client: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to connect to Kubernetes API"
                })),
            )
                .into_response();
        },
    };

    match config.save(client).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": "Configuration updated successfully"
            })),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to save configuration: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to save configuration: {}", e)
                })),
            )
                .into_response()
        },
    }
}

/// Test notification endpoint - sends a test notification
pub async fn test_notification(Json(payload): Json<serde_json::Value>) -> impl IntoResponse {
    use crate::notifications::{
        DeploymentInfo, NotificationEvent, NotificationPayload, Notifier, SlackConfig,
        SlackNotifier, TeamsConfig, TeamsNotifier, WebhookConfig, WebhookNotifier,
    };

    info!("Testing notification: {:?}", payload);

    // Extract notification type from payload
    let notification_type = payload
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Get Kubernetes client and load current configuration
    let client = match Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create Kubernetes client: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to connect to Kubernetes API"
                })),
            )
                .into_response();
        },
    };

    let config = match HeadwindConfig::load(client).await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to load configuration: {}", e)
                })),
            )
                .into_response();
        },
    };

    // Create a test notification payload
    let test_deployment = DeploymentInfo {
        name: "test-deployment".to_string(),
        namespace: "default".to_string(),
        current_image: "nginx:1.25.0".to_string(),
        new_image: "nginx:1.26.0".to_string(),
        container: Some("nginx".to_string()),
        resource_kind: Some("Deployment".to_string()),
    };

    let test_payload =
        NotificationPayload::new(NotificationEvent::UpdateRequestCreated, test_deployment)
            .with_policy("minor")
            .with_requires_approval(true);

    // Send notification based on type
    match notification_type {
        "slack" => {
            let slack_config = SlackConfig {
                enabled: config.notifications.slack.enabled,
                webhook_url: config.notifications.slack.webhook_url.clone(),
                channel: config.notifications.slack.channel.clone(),
                username: config.notifications.slack.username.clone(),
                icon_emoji: config.notifications.slack.icon_emoji.clone(),
            };

            match SlackNotifier::new(slack_config) {
                Ok(notifier) => match notifier.send(&test_payload).await {
                    Ok(_) => (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "message": "Test Slack notification sent successfully"
                        })),
                    )
                        .into_response(),
                    Err(e) => {
                        error!("Failed to send test Slack notification: {}", e);
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({
                                "error": format!("Failed to send Slack notification: {}", e)
                            })),
                        )
                            .into_response()
                    },
                },
                Err(e) => {
                    error!("Failed to create Slack notifier: {}", e);
                    (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": format!("Slack not configured: {}", e)
                        })),
                    )
                        .into_response()
                },
            }
        },
        "teams" => {
            let teams_config = TeamsConfig {
                enabled: config.notifications.teams.enabled,
                webhook_url: config.notifications.teams.webhook_url.clone(),
            };

            match TeamsNotifier::new(teams_config) {
                Ok(notifier) => match notifier.send(&test_payload).await {
                    Ok(_) => (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "message": "Test Teams notification sent successfully"
                        })),
                    )
                        .into_response(),
                    Err(e) => {
                        error!("Failed to send test Teams notification: {}", e);
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({
                                "error": format!("Failed to send Teams notification: {}", e)
                            })),
                        )
                            .into_response()
                    },
                },
                Err(e) => {
                    error!("Failed to create Teams notifier: {}", e);
                    (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": format!("Teams not configured: {}", e)
                        })),
                    )
                        .into_response()
                },
            }
        },
        "webhook" => {
            let webhook_config = WebhookConfig {
                enabled: config.notifications.webhook.enabled,
                url: config.notifications.webhook.url.clone(),
                secret: None,
                timeout_seconds: 10,
                max_retries: 3,
            };

            match WebhookNotifier::new(webhook_config) {
                Ok(notifier) => match notifier.send(&test_payload).await {
                    Ok(_) => (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "message": "Test webhook notification sent successfully"
                        })),
                    )
                        .into_response(),
                    Err(e) => {
                        error!("Failed to send test webhook notification: {}", e);
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({
                                "error": format!("Failed to send webhook notification: {}", e)
                            })),
                        )
                            .into_response()
                    },
                },
                Err(e) => {
                    error!("Failed to create webhook notifier: {}", e);
                    (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": format!("Webhook not configured: {}", e)
                        })),
                    )
                        .into_response()
                },
            }
        },
        _ => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Invalid notification type. Must be 'slack', 'teams', or 'webhook'"
            })),
        )
            .into_response(),
    }
}

/// Observability page - metrics dashboard
pub async fn observability_page() -> impl IntoResponse {
    info!("Rendering observability page");
    templates::observability()
}

/// Get metrics data for dashboard
pub async fn get_metrics_data() -> impl IntoResponse {
    use crate::metrics::client::create_metrics_client;

    info!("Fetching metrics data");

    let client = match Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create Kubernetes client: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to connect to Kubernetes API"
                })),
            )
                .into_response();
        },
    };

    let config = match HeadwindConfig::load(client).await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to load configuration: {}", e)
                })),
            )
                .into_response();
        },
    };

    // Create metrics client
    let metrics_client = create_metrics_client(
        &config.observability.metrics_backend,
        config.observability.prometheus.url.clone(),
        config.observability.prometheus.enabled,
        config.observability.victoriametrics.url.clone(),
        config.observability.victoriametrics.enabled,
        config.observability.influxdb.url.clone(),
        config.observability.influxdb.enabled,
        config.observability.influxdb.org.clone(),
        config.observability.influxdb.bucket.clone(),
        config.observability.influxdb.token.clone(),
    )
    .await;

    // Query key metrics
    let mut metrics = serde_json::Map::new();
    metrics.insert(
        "backend".to_string(),
        serde_json::json!(metrics_client.backend_type()),
    );

    // Query instant metrics
    let metric_queries = vec![
        ("updates_pending", "headwind_updates_pending"),
        ("updates_approved", "headwind_updates_approved_total"),
        ("updates_rejected", "headwind_updates_rejected_total"),
        ("updates_applied", "headwind_updates_applied_total"),
        ("updates_failed", "headwind_updates_failed_total"),
        ("deployments_watched", "headwind_deployments_watched"),
        ("statefulsets_watched", "headwind_statefulsets_watched"),
        ("daemonsets_watched", "headwind_daemonsets_watched"),
        ("helm_releases_watched", "headwind_helm_releases_watched"),
    ];

    for (key, query) in metric_queries {
        match metrics_client.query_instant(query).await {
            Ok(value) => {
                metrics.insert(key.to_string(), serde_json::json!(value.value));
            },
            Err(e) => {
                error!("Failed to query metric {}: {}", query, e);
                metrics.insert(key.to_string(), serde_json::json!(0));
            },
        }
    }

    (StatusCode::OK, Json(metrics)).into_response()
}

/// Get metrics time series for charts
pub async fn get_metrics_timeseries(
    Path(metric_name): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    use crate::metrics::client::create_metrics_client;

    info!("Fetching time series for metric: {}", metric_name);

    let client = match Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create Kubernetes client: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to connect to Kubernetes API"
                })),
            )
                .into_response();
        },
    };

    let config = match HeadwindConfig::load(client).await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to load configuration: {}", e)
                })),
            )
                .into_response();
        },
    };

    // Create metrics client
    let metrics_client = create_metrics_client(
        &config.observability.metrics_backend,
        config.observability.prometheus.url.clone(),
        config.observability.prometheus.enabled,
        config.observability.victoriametrics.url.clone(),
        config.observability.victoriametrics.enabled,
        config.observability.influxdb.url.clone(),
        config.observability.influxdb.enabled,
        config.observability.influxdb.org.clone(),
        config.observability.influxdb.bucket.clone(),
        config.observability.influxdb.token.clone(),
    )
    .await;

    // Parse time range from query parameter (default: 6h)
    let time_range = params.get("range").map(|s| s.as_str()).unwrap_or("6h");
    let end = Utc::now();
    let (start, step) = match time_range {
        "1h" => (end - Duration::hours(1), "1m"),
        "6h" => (end - Duration::hours(6), "5m"),
        "24h" => (end - Duration::hours(24), "10m"),
        "7d" => (end - Duration::days(7), "1h"),
        "30d" => (end - Duration::days(30), "6h"),
        _ => (end - Duration::hours(6), "5m"), // default to 6h
    };

    match metrics_client
        .query_range(&metric_name, start, end, step)
        .await
    {
        Ok(points) => {
            // Fill in missing time intervals with zeros for better visualization
            let filled_points = fill_missing_intervals(points, start, end, time_range);
            (StatusCode::OK, Json(filled_points)).into_response()
        },
        Err(e) => {
            error!("Failed to query metric time series {}: {}", metric_name, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to query metric: {}", e)
                })),
            )
                .into_response()
        },
    }
}

/// Fill in missing time intervals with zero values for better chart visualization
fn fill_missing_intervals(
    points: Vec<crate::metrics::client::MetricPoint>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    time_range: &str,
) -> Vec<crate::metrics::client::MetricPoint> {
    use crate::metrics::client::MetricPoint;
    use std::collections::HashMap;

    // For short ranges (1h, 6h, 24h), return as-is to avoid too many data points
    if time_range == "1h" || time_range == "6h" || time_range == "24h" {
        return points;
    }

    // For 7d and 30d, generate daily intervals
    let interval = Duration::days(1);

    // Create a map of existing data points by date (ignoring time)
    let mut data_map: HashMap<String, f64> = HashMap::new();
    for point in &points {
        let date_key = point.timestamp.format("%Y-%m-%d").to_string();
        // Sum values for the same day (or take max, depending on metric type)
        data_map
            .entry(date_key)
            .and_modify(|v| *v = v.max(point.value))
            .or_insert(point.value);
    }

    // Generate complete time series with zeros for missing days
    let mut filled = Vec::new();
    let mut current = start;

    while current <= end {
        let date_key = current.format("%Y-%m-%d").to_string();
        let value = data_map.get(&date_key).copied().unwrap_or(0.0);

        filled.push(MetricPoint {
            timestamp: current,
            value,
        });

        current += interval;
    }

    filled
}

/// List all UpdateRequest CRDs (for update counts in observability dashboard)
pub async fn list_update_requests() -> impl IntoResponse {
    let client = match Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create Kubernetes client: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to connect to Kubernetes API"
                })),
            )
                .into_response();
        },
    };

    // Query all UpdateRequests across all namespaces
    let update_requests: Api<UpdateRequest> = Api::all(client);

    match update_requests.list(&Default::default()).await {
        Ok(list) => {
            // Convert to a simpler format for the frontend
            let updates: Vec<serde_json::Value> = list
                .items
                .iter()
                .map(|ur| {
                    serde_json::json!({
                        "namespace": ur.metadata.namespace.as_ref().unwrap_or(&"default".to_string()),
                        "name": ur.metadata.name.as_ref().unwrap_or(&"unknown".to_string()),
                        "status": ur.status.as_ref().map(|s| serde_json::json!({
                            "phase": format!("{:?}", s.phase),
                            "approvedBy": s.approved_by.clone(),
                        })),
                    })
                })
                .collect();

            (StatusCode::OK, Json(updates)).into_response()
        },
        Err(e) => {
            error!("Failed to list UpdateRequests: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to list UpdateRequests: {}", e)
                })),
            )
                .into_response()
        },
    }
}

/// Bulk approve updates
#[derive(serde::Deserialize)]
pub struct BulkApproveRequest {
    updates: Vec<UpdateIdentifier>,
    approver: String,
}

#[derive(serde::Deserialize)]
pub struct UpdateIdentifier {
    namespace: String,
    name: String,
}

#[derive(serde::Serialize)]
pub struct BulkOperationResponse {
    success_count: usize,
    failed_count: usize,
    errors: Vec<String>,
}

pub async fn bulk_approve(
    user: UserIdentity,
    Json(request): Json<BulkApproveRequest>,
) -> impl IntoResponse {
    info!(
        "Bulk approving {} updates by {}",
        request.updates.len(),
        user.username
    );

    let mut success_count = 0;
    let mut failed_count = 0;
    let mut errors = Vec::new();

    for update in &request.updates {
        // Call the approval API endpoint for each update
        let approval_url = format!(
            "http://localhost:8081/api/v1/updates/{}/{}/approve",
            update.namespace, update.name
        );

        let approve_payload = serde_json::json!({
            "approver": request.approver
        });

        match reqwest::Client::new()
            .post(&approval_url)
            .json(&approve_payload)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    success_count += 1;
                    info!("Successfully approved {}/{}", update.namespace, update.name);

                    // Log successful bulk approval
                    AuditLogEntry::new(
                        user.username.clone(),
                        "bulk_approve".to_string(),
                        "UpdateRequest".to_string(),
                        update.namespace.clone(),
                        update.name.clone(),
                        "success".to_string(),
                        None,
                    )
                    .log();
                } else {
                    failed_count += 1;
                    let error_msg = format!(
                        "{}/{}: HTTP {}",
                        update.namespace,
                        update.name,
                        response.status()
                    );
                    errors.push(error_msg.clone());
                    error!(
                        "Failed to approve {}/{}: {}",
                        update.namespace, update.name, error_msg
                    );

                    // Log failed bulk approval
                    AuditLogEntry::new(
                        user.username.clone(),
                        "bulk_approve".to_string(),
                        "UpdateRequest".to_string(),
                        update.namespace.clone(),
                        update.name.clone(),
                        "failed".to_string(),
                        Some(error_msg),
                    )
                    .log();
                }
            },
            Err(e) => {
                failed_count += 1;
                let error_msg = format!("{}/{}: {}", update.namespace, update.name, e);
                errors.push(error_msg.clone());
                error!(
                    "Failed to approve {}/{}: {}",
                    update.namespace, update.name, e
                );

                // Log failed bulk approval
                AuditLogEntry::new(
                    user.username.clone(),
                    "bulk_approve".to_string(),
                    "UpdateRequest".to_string(),
                    update.namespace.clone(),
                    update.name.clone(),
                    "failed".to_string(),
                    Some(error_msg),
                )
                .log();
            },
        }
    }

    let response = BulkOperationResponse {
        success_count,
        failed_count,
        errors,
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// Bulk reject updates
#[derive(serde::Deserialize)]
pub struct BulkRejectRequest {
    updates: Vec<UpdateIdentifier>,
    reason: String,
}

pub async fn bulk_reject(
    user: UserIdentity,
    Json(request): Json<BulkRejectRequest>,
) -> impl IntoResponse {
    info!(
        "Bulk rejecting {} updates by {} with reason: {}",
        request.updates.len(),
        user.username,
        request.reason
    );

    let mut success_count = 0;
    let mut failed_count = 0;
    let mut errors = Vec::new();

    for update in &request.updates {
        // Call the approval API endpoint for each update
        let reject_url = format!(
            "http://localhost:8081/api/v1/updates/{}/{}/reject",
            update.namespace, update.name
        );

        let reject_payload = serde_json::json!({
            "reason": request.reason
        });

        match reqwest::Client::new()
            .post(&reject_url)
            .json(&reject_payload)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    success_count += 1;
                    info!("Successfully rejected {}/{}", update.namespace, update.name);

                    // Log successful bulk rejection
                    AuditLogEntry::new(
                        user.username.clone(),
                        "bulk_reject".to_string(),
                        "UpdateRequest".to_string(),
                        update.namespace.clone(),
                        update.name.clone(),
                        "success".to_string(),
                        Some(request.reason.clone()),
                    )
                    .log();
                } else {
                    failed_count += 1;
                    let error_msg = format!(
                        "{}/{}: HTTP {}",
                        update.namespace,
                        update.name,
                        response.status()
                    );
                    errors.push(error_msg.clone());
                    error!(
                        "Failed to reject {}/{}: {}",
                        update.namespace, update.name, error_msg
                    );

                    // Log failed bulk rejection
                    AuditLogEntry::new(
                        user.username.clone(),
                        "bulk_reject".to_string(),
                        "UpdateRequest".to_string(),
                        update.namespace.clone(),
                        update.name.clone(),
                        "failed".to_string(),
                        Some(error_msg),
                    )
                    .log();
                }
            },
            Err(e) => {
                failed_count += 1;
                let error_msg = format!("{}/{}: {}", update.namespace, update.name, e);
                errors.push(error_msg.clone());
                error!(
                    "Failed to reject {}/{}: {}",
                    update.namespace, update.name, e
                );

                // Log failed bulk rejection
                AuditLogEntry::new(
                    user.username.clone(),
                    "bulk_reject".to_string(),
                    "UpdateRequest".to_string(),
                    update.namespace.clone(),
                    update.name.clone(),
                    "failed".to_string(),
                    Some(error_msg),
                )
                .log();
            },
        }
    }

    let response = BulkOperationResponse {
        success_count,
        failed_count,
        errors,
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// Server-Sent Events endpoint for real-time UpdateRequest changes
pub async fn updates_stream() -> Sse<impl Stream<Item = Result<Event, anyhow::Error>>> {
    use kube::runtime::watcher;

    info!("New SSE client connected for UpdateRequest stream");

    let stream = async_stream::stream! {
        // Create Kubernetes client
        let client = match Client::try_default().await {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to create Kubernetes client for SSE: {}", e);
                return;
            },
        };

        // Create API for UpdateRequests across all namespaces
        let api: Api<UpdateRequest> = Api::all(client);

        // Watch for changes
        let watcher = watcher(api, Default::default());
        let mut stream = watcher.boxed();

        while let Some(event) = tokio_stream::StreamExt::next(&mut stream).await {
            match event {
                Ok(watcher_event) => {
                    use kube::runtime::watcher::Event as WatchEvent;

                    match watcher_event {
                        WatchEvent::Apply(ur) => {
                            // Send an update event for applied/modified resources
                            let view = convert_to_view(&ur);
                            match serde_json::to_string(&view) {
                                Ok(json) => {
                                    let event = Event::default()
                                        .event("updated")
                                        .data(json);
                                    yield Ok(event);
                                },
                                Err(e) => {
                                    error!("Failed to serialize UpdateRequest for SSE: {}", e);
                                },
                            }
                        },
                        WatchEvent::Delete(ur) => {
                            // Send a delete event
                            let view = convert_to_view(&ur);
                            match serde_json::to_string(&view) {
                                Ok(json) => {
                                    let event = Event::default()
                                        .event("deleted")
                                        .data(json);
                                    yield Ok(event);
                                },
                                Err(e) => {
                                    error!("Failed to serialize UpdateRequest for SSE: {}", e);
                                },
                            }
                        },
                        WatchEvent::Init => {
                            // Send a refresh event when the watch initializes
                            let event = Event::default()
                                .event("refresh")
                                .data("Watch initialized");
                            yield Ok(event);
                        },
                        WatchEvent::InitApply(ur) => {
                            // Send an update event for initial resources during watch initialization
                            let view = convert_to_view(&ur);
                            match serde_json::to_string(&view) {
                                Ok(json) => {
                                    let event = Event::default()
                                        .event("updated")
                                        .data(json);
                                    yield Ok(event);
                                },
                                Err(e) => {
                                    error!("Failed to serialize UpdateRequest for SSE: {}", e);
                                },
                            }
                        },
                        WatchEvent::InitDone => {
                            // Send an event when initial watch synchronization completes
                            let event = Event::default()
                                .event("init-done")
                                .data("Initial watch sync completed");
                            yield Ok(event);
                        },
                    }
                },
                Err(e) => {
                    error!("Watch error in SSE stream: {}", e);
                    // Send an error event to the client
                    let event = Event::default()
                        .event("error")
                        .data(format!("Watch error: {}", e));
                    yield Ok(event);
                },
            }
        }

        info!("SSE stream ended");
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    )
}

#[derive(Deserialize)]
pub struct ApprovalForm {
    approver: Option<String>,
}

/// Approve an individual update request (proxy to approval API)
pub async fn approve_update(
    user: UserIdentity,
    Path((namespace, name)): Path<(String, String)>,
    Form(form): Form<ApprovalForm>,
) -> impl IntoResponse {
    let approver = user.username.clone();

    let approval_url = format!(
        "http://localhost:8081/api/v1/updates/{}/{}/approve",
        namespace, name
    );

    // Convert form to JSON for the approval API
    let json_body = serde_json::json!({
        "approver": form.approver
    });

    match reqwest::Client::new()
        .post(&approval_url)
        .json(&json_body)
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            match response.json::<serde_json::Value>().await {
                Ok(body) => {
                    // Log successful approval
                    AuditLogEntry::new(
                        approver,
                        "approve".to_string(),
                        "UpdateRequest".to_string(),
                        namespace.clone(),
                        name.clone(),
                        "success".to_string(),
                        None,
                    )
                    .log();

                    (status, Json(body))
                },
                Err(_) => {
                    // Log failed approval (parse error)
                    AuditLogEntry::new(
                        approver,
                        "approve".to_string(),
                        "UpdateRequest".to_string(),
                        namespace,
                        name,
                        "failed".to_string(),
                        Some("Failed to parse approval API response".to_string()),
                    )
                    .log();

                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "Failed to parse response"})),
                    )
                },
            }
        },
        Err(e) => {
            // Log failed approval (request error)
            AuditLogEntry::new(
                approver,
                "approve".to_string(),
                "UpdateRequest".to_string(),
                namespace,
                name,
                "failed".to_string(),
                Some(format!("Failed to call approval API: {}", e)),
            )
            .log();

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        },
    }
}

#[derive(Deserialize)]
pub struct RejectionForm {
    approver: Option<String>,
    reason: Option<String>,
}

/// Reject an individual update request (proxy to approval API)
pub async fn reject_update(
    user: UserIdentity,
    Path((namespace, name)): Path<(String, String)>,
    Form(form): Form<RejectionForm>,
) -> impl IntoResponse {
    let approver = user.username.clone();
    let rejection_reason = form.reason.clone();

    let approval_url = format!(
        "http://localhost:8081/api/v1/updates/{}/{}/reject",
        namespace, name
    );

    // Convert form to JSON for the approval API
    let json_body = serde_json::json!({
        "approver": form.approver,
        "reason": form.reason
    });

    match reqwest::Client::new()
        .post(&approval_url)
        .json(&json_body)
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            match response.json::<serde_json::Value>().await {
                Ok(body) => {
                    // Log successful rejection
                    AuditLogEntry::new(
                        approver,
                        "reject".to_string(),
                        "UpdateRequest".to_string(),
                        namespace.clone(),
                        name.clone(),
                        "success".to_string(),
                        rejection_reason,
                    )
                    .log();

                    (status, Json(body))
                },
                Err(_) => {
                    // Log failed rejection (parse error)
                    AuditLogEntry::new(
                        approver,
                        "reject".to_string(),
                        "UpdateRequest".to_string(),
                        namespace,
                        name,
                        "failed".to_string(),
                        Some("Failed to parse approval API response".to_string()),
                    )
                    .log();

                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "Failed to parse response"})),
                    )
                },
            }
        },
        Err(e) => {
            // Log failed rejection (request error)
            AuditLogEntry::new(
                approver,
                "reject".to_string(),
                "UpdateRequest".to_string(),
                namespace,
                name,
                "failed".to_string(),
                Some(format!("Failed to call approval API: {}", e)),
            )
            .log();

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        },
    }
}
