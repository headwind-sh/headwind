use crate::controller::update_deployment_image_with_tracking;
use crate::models::crd::{UpdatePhase, UpdateRequest, UpdateRequestStatus};
use crate::models::update::ApprovalRequest;
use crate::notifications::{self, DeploymentInfo};
use crate::rollback::{
    AutoRollbackConfig, HealthChecker, HealthStatus, RollbackManager, UpdateHistory,
};
use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Utc;
use k8s_openapi::api::apps::v1::Deployment;
use kube::api::{Patch, PatchParams};
use kube::{Api, Client};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::task::JoinHandle;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};

#[derive(Clone)]
struct ApprovalState {
    client: Client,
}

pub async fn start_approval_server() -> Result<JoinHandle<()>> {
    let client = Client::try_default().await?;
    let state = ApprovalState { client };

    let app = Router::new()
        .route("/api/v1/updates", get(list_updates))
        .route("/api/v1/updates/{namespace}/{name}", get(get_update))
        .route(
            "/api/v1/updates/{namespace}/{name}/approve",
            post(approve_update),
        )
        .route(
            "/api/v1/updates/{namespace}/{name}/reject",
            post(reject_update),
        )
        .route(
            "/api/v1/rollback/{namespace}/{deployment}",
            get(get_rollback_history),
        )
        .route(
            "/api/v1/rollback/{namespace}/{deployment}",
            post(rollback_deployment),
        )
        .route("/health", get(health_check))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = "0.0.0.0:8081";
    info!("Starting approval API server on {}", addr);

    let handle = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .expect("Failed to bind approval server");

        axum::serve(listener, app)
            .await
            .expect("Approval server failed");
    });

    Ok(handle)
}

async fn list_updates(
    State(state): State<ApprovalState>,
) -> Result<Json<Vec<UpdateRequest>>, StatusCode> {
    // Query all UpdateRequest CRDs across all namespaces
    let update_requests: Api<UpdateRequest> = Api::all(state.client);

    match update_requests.list(&Default::default()).await {
        Ok(list) => Ok(Json(list.items)),
        Err(e) => {
            error!("Failed to list UpdateRequests: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        },
    }
}

async fn get_update(
    State(state): State<ApprovalState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<UpdateRequest>, StatusCode> {
    let update_requests: Api<UpdateRequest> = Api::namespaced(state.client, &namespace);

    match update_requests.get(&name).await {
        Ok(ur) => Ok(Json(ur)),
        Err(e) => {
            warn!("UpdateRequest {}/{} not found: {}", namespace, name, e);
            Err(StatusCode::NOT_FOUND)
        },
    }
}

async fn approve_update(
    State(state): State<ApprovalState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(approval): Json<ApprovalRequest>,
) -> impl IntoResponse {
    let update_requests: Api<UpdateRequest> = Api::namespaced(state.client.clone(), &namespace);

    // Get the UpdateRequest
    let update_request = match update_requests.get(&name).await {
        Ok(ur) => ur,
        Err(e) => {
            warn!("UpdateRequest {}/{} not found: {}", namespace, name, e);
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": format!("UpdateRequest not found: {}", e)})),
            );
        },
    };

    // Check if already approved/rejected
    if let Some(status) = &update_request.status
        && status.phase != UpdatePhase::Pending
    {
        warn!(
            "UpdateRequest {}/{} is not in pending state: {:?}",
            namespace, name, status.phase
        );
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!("UpdateRequest is in {:?} state, cannot approve", status.phase),
                "current_phase": format!("{:?}", status.phase)
            })),
        );
    }

    info!(
        "Approving UpdateRequest {}/{} by {:?}",
        namespace,
        name,
        approval.approver.as_deref().unwrap_or("unknown")
    );

    // Execute the update
    let update_result = execute_update(
        &state.client,
        &update_request,
        Some(name.clone()),
        approval.approver.clone(),
        true, // Enable automatic rollback monitoring
    )
    .await;

    // Build deployment info for notifications
    let deployment_info = DeploymentInfo {
        name: update_request.spec.target_ref.name.clone(),
        namespace: update_request.spec.target_ref.namespace.clone(),
        current_image: update_request.spec.current_image.clone(),
        new_image: update_request.spec.new_image.clone(),
        container: update_request.spec.container_name.clone(),
        resource_kind: Some(update_request.spec.target_ref.kind.clone()),
    };

    // Send approval notification
    notifications::notify_update_approved(
        deployment_info.clone(),
        approval
            .approver
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        name.clone(),
    );

    // Update the CRD status
    let new_status = match update_result {
        Ok(()) => {
            info!("Successfully applied update {}/{}", namespace, name);

            // Send completion notification
            notifications::notify_update_completed(deployment_info.clone());

            UpdateRequestStatus {
                phase: UpdatePhase::Completed,
                approved_by: approval.approver.clone(),
                approved_at: Some(Utc::now()),
                message: Some("Update applied successfully".to_string()),
                last_updated: Some(Utc::now()),
                ..Default::default()
            }
        },
        Err(e) => {
            error!("Failed to apply update {}/{}: {}", namespace, name, e);

            // Send failure notification
            notifications::notify_update_failed(deployment_info.clone(), e.to_string());

            UpdateRequestStatus {
                phase: UpdatePhase::Failed,
                approved_by: approval.approver.clone(),
                approved_at: Some(Utc::now()),
                message: Some(format!("Update failed: {}", e)),
                last_updated: Some(Utc::now()),
                ..Default::default()
            }
        },
    };

    // Patch the status
    let status_patch = json!({
        "apiVersion": "headwind.sh/v1alpha1",
        "kind": "UpdateRequest",
        "status": new_status
    });

    match update_requests
        .patch_status(&name, &PatchParams::default(), &Patch::Merge(status_patch))
        .await
    {
        Ok(updated_ur) => {
            info!("Updated status for UpdateRequest {}/{}", namespace, name);
            (StatusCode::OK, Json(json!(updated_ur)))
        },
        Err(e) => {
            error!(
                "Failed to update status for UpdateRequest {}/{}: {}",
                namespace, name, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Failed to update status: {}", e)})),
            )
        },
    }
}

async fn reject_update(
    State(state): State<ApprovalState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(approval): Json<ApprovalRequest>,
) -> impl IntoResponse {
    let update_requests: Api<UpdateRequest> = Api::namespaced(state.client.clone(), &namespace);

    // Get the UpdateRequest
    let update_request = match update_requests.get(&name).await {
        Ok(ur) => ur,
        Err(e) => {
            warn!("UpdateRequest {}/{} not found: {}", namespace, name, e);
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": format!("UpdateRequest not found: {}", e)})),
            );
        },
    };

    // Check if already approved/rejected
    if let Some(status) = &update_request.status
        && status.phase != UpdatePhase::Pending
    {
        warn!(
            "UpdateRequest {}/{} is not in pending state: {:?}",
            namespace, name, status.phase
        );
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!("UpdateRequest is in {:?} state, cannot reject", status.phase),
                "current_phase": format!("{:?}", status.phase)
            })),
        );
    }

    info!(
        "Rejecting UpdateRequest {}/{} by {:?}: {:?}",
        namespace,
        name,
        approval.approver.as_deref().unwrap_or("unknown"),
        approval.reason
    );

    // Build deployment info for notifications
    let deployment_info = DeploymentInfo {
        name: update_request.spec.target_ref.name.clone(),
        namespace: update_request.spec.target_ref.namespace.clone(),
        current_image: update_request.spec.current_image.clone(),
        new_image: update_request.spec.new_image.clone(),
        container: update_request.spec.container_name.clone(),
        resource_kind: Some(update_request.spec.target_ref.kind.clone()),
    };

    // Send rejection notification
    notifications::notify_update_rejected(
        deployment_info,
        approval
            .approver
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        approval
            .reason
            .clone()
            .unwrap_or_else(|| "No reason provided".to_string()),
        name.clone(),
    );

    // Update the CRD status
    let new_status = UpdateRequestStatus {
        phase: UpdatePhase::Rejected,
        rejected_by: approval.approver.clone(),
        rejected_at: Some(Utc::now()),
        message: approval
            .reason
            .clone()
            .or(Some("Rejected by user".to_string())),
        last_updated: Some(Utc::now()),
        ..Default::default()
    };

    // Patch the status
    let status_patch = json!({
        "apiVersion": "headwind.sh/v1alpha1",
        "kind": "UpdateRequest",
        "status": new_status
    });

    match update_requests
        .patch_status(&name, &PatchParams::default(), &Patch::Merge(status_patch))
        .await
    {
        Ok(updated_ur) => {
            info!("Updated status for UpdateRequest {}/{}", namespace, name);
            (StatusCode::OK, Json(json!(updated_ur)))
        },
        Err(e) => {
            error!(
                "Failed to update status for UpdateRequest {}/{}: {}",
                namespace, name, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Failed to update status: {}", e)})),
            )
        },
    }
}

async fn execute_update(
    client: &Client,
    update_request: &UpdateRequest,
    update_request_name: Option<String>,
    approved_by: Option<String>,
    enable_auto_rollback: bool,
) -> Result<()> {
    let spec = &update_request.spec;
    let target = &spec.target_ref;

    debug!(
        "Executing update for {}/{} in namespace {}",
        target.kind, target.name, target.namespace
    );

    // Route to appropriate update handler based on resource kind
    match target.kind.as_str() {
        "Deployment" => {
            execute_deployment_update(
                client,
                update_request,
                update_request_name,
                approved_by,
                enable_auto_rollback,
            )
            .await
        },
        "HelmRelease" => {
            execute_helmrelease_update(client, update_request, update_request_name, approved_by)
                .await
        },
        _ => Err(anyhow::anyhow!(
            "Unsupported resource kind: {}. Only Deployment and HelmRelease are supported.",
            target.kind
        )),
    }
}

async fn execute_deployment_update(
    client: &Client,
    update_request: &UpdateRequest,
    update_request_name: Option<String>,
    approved_by: Option<String>,
    enable_auto_rollback: bool,
) -> Result<()> {
    let spec = &update_request.spec;
    let target = &spec.target_ref;

    // Get the container name
    let container_name = spec
        .container_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Container name not specified in UpdateRequest"))?;

    // Verify the deployment exists and get auto-rollback config
    let deployments: Api<Deployment> = Api::namespaced(client.clone(), &target.namespace);
    let deployment = deployments.get(&target.name).await?;

    // Verify the container exists in the deployment
    let pod_spec = deployment
        .spec
        .as_ref()
        .and_then(|s| s.template.spec.as_ref())
        .ok_or_else(|| anyhow::anyhow!("Deployment has no pod spec"))?;

    let container_exists = pod_spec
        .containers
        .iter()
        .any(|c| c.name == *container_name);

    if !container_exists {
        return Err(anyhow::anyhow!(
            "Container '{}' not found in deployment {}",
            container_name,
            target.name
        ));
    }

    // Get auto-rollback config from deployment annotations
    let auto_rollback_config = deployment
        .metadata
        .annotations
        .as_ref()
        .map(AutoRollbackConfig::from_annotations)
        .unwrap_or_default();

    // Store the current image for potential rollback
    let current_image = pod_spec
        .containers
        .iter()
        .find(|c| c.name == *container_name)
        .and_then(|c| c.image.as_ref())
        .cloned();

    // Call the update function with tracking metadata
    update_deployment_image_with_tracking(
        client.clone(),
        &target.namespace,
        &target.name,
        container_name,
        &spec.new_image,
        update_request_name.clone(),
        approved_by.clone(),
    )
    .await?;

    // If auto-rollback is enabled, spawn a background task to monitor health
    if enable_auto_rollback && auto_rollback_config.enabled {
        let client_clone = client.clone();
        let deployment_name = target.name.clone();
        let namespace = target.namespace.clone();
        let container_name_clone = container_name.clone();
        let new_image = spec.new_image.clone();

        tokio::spawn(async move {
            info!(
                "Auto-rollback enabled for {}/{}, monitoring deployment health...",
                namespace, deployment_name
            );

            let health_checker = HealthChecker::new(client_clone.clone());
            match health_checker
                .monitor_deployment_health(&deployment_name, &namespace, &auto_rollback_config)
                .await
            {
                Ok(HealthStatus::Healthy) => {
                    info!(
                        "Deployment {}/{} is healthy after update to {}",
                        namespace, deployment_name, new_image
                    );
                },
                Ok(HealthStatus::Failed(reason)) => {
                    error!(
                        "Automatic rollback triggered for {}/{}: {}",
                        namespace, deployment_name, reason
                    );

                    // Send rollback trigger notification
                    let deployment_info = DeploymentInfo {
                        name: deployment_name.clone(),
                        namespace: namespace.clone(),
                        current_image: new_image.clone(),
                        new_image: current_image.clone().unwrap_or_default(),
                        container: Some(container_name_clone.clone()),
                        resource_kind: None,
                    };
                    notifications::notify_rollback_triggered(
                        deployment_info.clone(),
                        reason.clone(),
                    );

                    // Attempt rollback
                    if let Some(rollback_image) = current_image {
                        match update_deployment_image_with_tracking(
                            client_clone,
                            &namespace,
                            &deployment_name,
                            &container_name_clone,
                            &rollback_image,
                            None,
                            Some("headwind-auto-rollback".to_string()),
                        )
                        .await
                        {
                            Ok(()) => {
                                info!(
                                    "Successfully rolled back {}/{} from {} to {}",
                                    namespace, deployment_name, new_image, rollback_image
                                );
                                notifications::notify_rollback_completed(deployment_info.clone());
                            },
                            Err(e) => {
                                error!(
                                    "Failed to rollback {}/{}: {}",
                                    namespace, deployment_name, e
                                );
                                notifications::notify_rollback_failed(
                                    deployment_info.clone(),
                                    e.to_string(),
                                );
                            },
                        }
                    } else {
                        warn!(
                            "Cannot rollback {}/{}: no previous image found",
                            namespace, deployment_name
                        );
                    }
                },
                Ok(HealthStatus::Timeout) => {
                    error!(
                        "Automatic rollback triggered for {}/{}: Health check timeout",
                        namespace, deployment_name
                    );

                    // Send rollback trigger notification
                    let deployment_info = DeploymentInfo {
                        name: deployment_name.clone(),
                        namespace: namespace.clone(),
                        current_image: new_image.clone(),
                        new_image: current_image.clone().unwrap_or_default(),
                        container: Some(container_name_clone.clone()),
                        resource_kind: None,
                    };
                    notifications::notify_rollback_triggered(
                        deployment_info.clone(),
                        "Health check timeout".to_string(),
                    );

                    // Attempt rollback
                    if let Some(rollback_image) = current_image {
                        match update_deployment_image_with_tracking(
                            client_clone,
                            &namespace,
                            &deployment_name,
                            &container_name_clone,
                            &rollback_image,
                            None,
                            Some("headwind-auto-rollback".to_string()),
                        )
                        .await
                        {
                            Ok(()) => {
                                info!(
                                    "Successfully rolled back {}/{} from {} to {} due to timeout",
                                    namespace, deployment_name, new_image, rollback_image
                                );
                                notifications::notify_rollback_completed(deployment_info.clone());
                            },
                            Err(e) => {
                                error!(
                                    "Failed to rollback {}/{}: {}",
                                    namespace, deployment_name, e
                                );
                                notifications::notify_rollback_failed(
                                    deployment_info.clone(),
                                    e.to_string(),
                                );
                            },
                        }
                    } else {
                        warn!(
                            "Cannot rollback {}/{}: no previous image found",
                            namespace, deployment_name
                        );
                    }
                },
                Ok(HealthStatus::Progressing) => {
                    warn!(
                        "Deployment {}/{} still progressing after timeout",
                        namespace, deployment_name
                    );
                },
                Err(e) => {
                    error!(
                        "Error monitoring deployment {}/{}: {}",
                        namespace, deployment_name, e
                    );
                },
            }
        });
    }

    Ok(())
}

async fn execute_helmrelease_update(
    client: &Client,
    update_request: &UpdateRequest,
    _update_request_name: Option<String>,
    _approved_by: Option<String>,
) -> Result<()> {
    use crate::models::HelmRelease;
    use kube::api::{Patch, PatchParams};
    use serde_json::json;

    let spec = &update_request.spec;
    let target = &spec.target_ref;

    info!(
        "Executing Helm chart update for {}/{} in namespace {}",
        target.kind, target.name, target.namespace
    );

    // Extract chart name and new version from the new_image field (format: "chart:version")
    let (chart_name, new_version) = spec
        .new_image
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("Invalid chart version format in new_image"))?;

    debug!(
        "Updating HelmRelease {}/{} chart {} to version {}",
        target.namespace, target.name, chart_name, new_version
    );

    // Get the HelmRelease
    let helm_releases: Api<HelmRelease> = Api::namespaced(client.clone(), &target.namespace);
    let helm_release = helm_releases.get(&target.name).await?;

    // Verify the chart name matches
    if helm_release.spec.chart.spec.chart != chart_name {
        return Err(anyhow::anyhow!(
            "Chart name mismatch: expected {}, found {}",
            chart_name,
            helm_release.spec.chart.spec.chart
        ));
    }

    // Prepare the patch to update the chart version
    let patch = json!({
        "spec": {
            "chart": {
                "spec": {
                    "version": new_version
                }
            }
        }
    });

    // Apply the patch using strategic merge
    let patch_params = PatchParams::default();
    let _patched_release = helm_releases
        .patch(&target.name, &patch_params, &Patch::Merge(&patch))
        .await?;

    info!(
        "Successfully updated HelmRelease {}/{} to chart version {}",
        target.namespace, target.name, new_version
    );

    // Send success notification
    let deployment_info = crate::notifications::DeploymentInfo {
        name: target.name.clone(),
        namespace: target.namespace.clone(),
        current_image: spec.current_image.clone(),
        new_image: spec.new_image.clone(),
        container: None,
        resource_kind: Some("HelmRelease".to_string()),
    };

    crate::notifications::notify_update_completed(deployment_info);

    // Increment metrics
    crate::metrics::HELM_UPDATES_APPLIED.inc();

    Ok(())
}

/// Query parameters for rollback
#[derive(Debug, Deserialize)]
struct RollbackQuery {
    /// Container name to rollback (optional, defaults to all containers)
    container: Option<String>,
}

/// Request body for rollback
#[derive(Debug, Deserialize, Serialize)]
struct RollbackRequest {
    /// Container name to rollback
    pub container: String,
    /// Index of the history entry to rollback to (0 = current, 1 = previous, etc.)
    /// If not specified, defaults to 1 (previous version)
    pub index: Option<usize>,
    /// User performing the rollback
    pub user: Option<String>,
    /// Reason for rollback
    pub reason: Option<String>,
}

/// Get rollback history for a deployment
async fn get_rollback_history(
    State(state): State<ApprovalState>,
    Path((namespace, deployment)): Path<(String, String)>,
    Query(query): Query<RollbackQuery>,
) -> Result<Json<UpdateHistory>, StatusCode> {
    let rollback_manager = RollbackManager::new(state.client);

    match rollback_manager.get_history(&deployment, &namespace).await {
        Ok(history) => {
            // If container is specified, filter to that container's history
            if let Some(container) = query.container {
                let filtered_entries: Vec<_> = history
                    .entries()
                    .iter()
                    .filter(|e| e.container == container)
                    .cloned()
                    .collect();

                let mut filtered_history = UpdateHistory::new();
                for entry in filtered_entries {
                    filtered_history.add_entry(entry);
                }
                Ok(Json(filtered_history))
            } else {
                Ok(Json(history))
            }
        },
        Err(e) => {
            error!(
                "Failed to get rollback history for {}/{}: {}",
                namespace, deployment, e
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        },
    }
}

/// Rollback a deployment to a previous version
async fn rollback_deployment(
    State(state): State<ApprovalState>,
    Path((namespace, deployment)): Path<(String, String)>,
    Json(request): Json<RollbackRequest>,
) -> impl IntoResponse {
    let rollback_manager = RollbackManager::new(state.client.clone());
    let index = request.index.unwrap_or(1); // Default to previous version

    info!(
        "Rollback requested for {}/{} container {} to index {} by {:?}",
        namespace,
        deployment,
        request.container,
        index,
        request.user.as_deref().unwrap_or("unknown")
    );

    // Get the target image from history
    let target_image = match rollback_manager
        .get_image_by_index(&deployment, &namespace, &request.container, index)
        .await
    {
        Ok(Some(image)) => image,
        Ok(None) => {
            warn!(
                "No history entry found at index {} for {}/{} container {}",
                index, namespace, deployment, request.container
            );
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": format!("No history entry found at index {}", index),
                    "deployment": deployment,
                    "container": request.container,
                    "index": index
                })),
            );
        },
        Err(e) => {
            error!(
                "Failed to get rollback history for {}/{}: {}",
                namespace, deployment, e
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Failed to retrieve rollback history: {}", e)})),
            );
        },
    };

    info!(
        "Rolling back {}/{} container {} to image {}",
        namespace, deployment, request.container, target_image
    );

    // Perform the rollback
    let rollback_result = update_deployment_image_with_tracking(
        state.client.clone(),
        &namespace,
        &deployment,
        &request.container,
        &target_image,
        None, // No UpdateRequest for manual rollbacks
        request.user.clone(),
    )
    .await;

    match rollback_result {
        Ok(()) => {
            info!(
                "Successfully rolled back {}/{} container {} to {}",
                namespace, deployment, request.container, target_image
            );
            (
                StatusCode::OK,
                Json(json!({
                    "message": "Rollback successful",
                    "deployment": deployment,
                    "namespace": namespace,
                    "container": request.container,
                    "image": target_image,
                    "user": request.user,
                    "reason": request.reason
                })),
            )
        },
        Err(e) => {
            error!(
                "Failed to rollback {}/{} container {}: {}",
                namespace, deployment, request.container, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": format!("Rollback failed: {}", e),
                    "deployment": deployment,
                    "container": request.container
                })),
            )
        },
    }
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}
