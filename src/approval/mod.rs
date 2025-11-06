use crate::controller::update_deployment_image;
use crate::models::crd::{UpdatePhase, UpdateRequest, UpdateRequestStatus};
use crate::models::update::ApprovalRequest;
use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Utc;
use k8s_openapi::api::apps::v1::Deployment;
use kube::api::{Patch, PatchParams};
use kube::{Api, Client};
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
    let update_result = execute_update(&state.client, &update_request).await;

    // Update the CRD status
    let new_status = match update_result {
        Ok(()) => {
            info!("Successfully applied update {}/{}", namespace, name);
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

async fn execute_update(client: &Client, update_request: &UpdateRequest) -> Result<()> {
    let spec = &update_request.spec;
    let target = &spec.target_ref;

    debug!(
        "Executing update for {}/{} in namespace {}",
        target.kind, target.name, target.namespace
    );

    // Currently only support Deployment updates
    if target.kind != "Deployment" {
        return Err(anyhow::anyhow!(
            "Unsupported resource kind: {}. Only Deployment is currently supported.",
            target.kind
        ));
    }

    // Get the container name
    let container_name = spec
        .container_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Container name not specified in UpdateRequest"))?;

    // Verify the deployment exists
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

    // Call the update function
    update_deployment_image(
        client.clone(),
        &target.namespace,
        &target.name,
        container_name,
        &spec.new_image,
    )
    .await?;

    Ok(())
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}
