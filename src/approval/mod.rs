use crate::models::update::{ApprovalRequest, UpdateRequest, UpdateStatus};
use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

type UpdateStore = Arc<RwLock<HashMap<String, UpdateRequest>>>;

#[derive(Clone)]
struct ApprovalState {
    updates: UpdateStore,
}

pub async fn start_approval_server() -> Result<JoinHandle<()>> {
    let state = ApprovalState {
        updates: Arc::new(RwLock::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/api/v1/updates", get(list_updates))
        .route("/api/v1/updates/{id}", get(get_update))
        .route("/api/v1/updates/{id}/approve", post(approve_update))
        .route("/api/v1/updates/{id}/reject", post(reject_update))
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
    let updates = state.updates.read().await;
    let list: Vec<UpdateRequest> = updates.values().cloned().collect();
    Ok(Json(list))
}

async fn get_update(
    State(state): State<ApprovalState>,
    Path(id): Path<String>,
) -> Result<Json<UpdateRequest>, StatusCode> {
    let updates = state.updates.read().await;
    updates
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn approve_update(
    State(state): State<ApprovalState>,
    Path(id): Path<String>,
    Json(approval): Json<ApprovalRequest>,
) -> impl IntoResponse {
    let mut updates = state.updates.write().await;

    if let Some(update) = updates.get_mut(&id) {
        if update.status == UpdateStatus::PendingApproval {
            update.status = UpdateStatus::Approved;
            info!(
                "Update {} approved by {:?}",
                id,
                approval.approver.as_deref().unwrap_or("unknown")
            );

            // TODO: Trigger the actual update in Kubernetes
            // This will be connected to the controller module

            return (StatusCode::OK, Json(update.clone()));
        } else {
            warn!("Update {} is not in pending state", id);
            return (StatusCode::CONFLICT, Json(update.clone()));
        }
    }

    (
        StatusCode::NOT_FOUND,
        Json(UpdateRequest {
            id: id.clone(),
            namespace: String::new(),
            resource_name: String::new(),
            resource_kind: crate::models::update::ResourceKind::Deployment,
            current_image: String::new(),
            new_image: String::new(),
            created_at: chrono::Utc::now(),
            status: UpdateStatus::Failed {
                reason: "Not found".to_string(),
            },
        }),
    )
}

async fn reject_update(
    State(state): State<ApprovalState>,
    Path(id): Path<String>,
    Json(approval): Json<ApprovalRequest>,
) -> impl IntoResponse {
    let mut updates = state.updates.write().await;

    if let Some(update) = updates.get_mut(&id) {
        if update.status == UpdateStatus::PendingApproval {
            update.status = UpdateStatus::Rejected;
            info!(
                "Update {} rejected by {:?}: {:?}",
                id,
                approval.approver.as_deref().unwrap_or("unknown"),
                approval.reason
            );

            return (StatusCode::OK, Json(update.clone()));
        } else {
            warn!("Update {} is not in pending state", id);
            return (StatusCode::CONFLICT, Json(update.clone()));
        }
    }

    (
        StatusCode::NOT_FOUND,
        Json(UpdateRequest {
            id: id.clone(),
            namespace: String::new(),
            resource_name: String::new(),
            resource_kind: crate::models::update::ResourceKind::Deployment,
            current_image: String::new(),
            new_image: String::new(),
            created_at: chrono::Utc::now(),
            status: UpdateStatus::Failed {
                reason: "Not found".to_string(),
            },
        }),
    )
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

// Public API for other modules to create update requests
#[allow(dead_code)]
pub async fn create_update_request(store: UpdateStore, request: UpdateRequest) -> Result<()> {
    let mut updates = store.write().await;
    updates.insert(request.id.clone(), request);
    Ok(())
}

// Public API to get the update store reference
#[allow(dead_code)]
pub fn get_update_store() -> UpdateStore {
    Arc::new(RwLock::new(HashMap::new()))
}
