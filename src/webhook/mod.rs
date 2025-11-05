use crate::models::webhook::{DockerHubWebhook, ImagePushEvent, RegistryWebhook};
use anyhow::Result;
use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

pub type EventSender = mpsc::UnboundedSender<ImagePushEvent>;
pub type EventReceiver = mpsc::UnboundedReceiver<ImagePushEvent>;

#[derive(Clone)]
struct WebhookState {
    event_tx: EventSender,
}

pub async fn start_webhook_server() -> Result<(JoinHandle<()>, EventSender)> {
    let (event_tx, event_rx) = mpsc::unbounded_channel();

    // Clone sender to return it
    let event_tx_clone = event_tx.clone();

    // Store the receiver globally or pass it to the controller
    tokio::spawn(process_webhook_events(event_rx));

    let state = WebhookState { event_tx };

    let app = Router::new()
        .route("/webhook/registry", post(handle_registry_webhook))
        .route("/webhook/dockerhub", post(handle_dockerhub_webhook))
        .route("/health", axum::routing::get(health_check))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = "0.0.0.0:8080";
    info!("Starting webhook server on {}", addr);

    let handle = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .expect("Failed to bind webhook server");

        axum::serve(listener, app)
            .await
            .expect("Webhook server failed");
    });

    Ok((handle, event_tx_clone))
}

async fn handle_registry_webhook(
    State(state): State<WebhookState>,
    Json(payload): Json<RegistryWebhook>,
) -> impl IntoResponse {
    info!(
        "Received registry webhook with {} events",
        payload.events.len()
    );

    for event in payload.events {
        if event.action == "push" {
            if let Some(tag) = event.target.tag {
                let push_event = ImagePushEvent {
                    registry: extract_registry(&event.target.repository),
                    repository: event.target.repository.clone(),
                    tag,
                    digest: Some(event.target.digest),
                };

                if let Err(e) = state.event_tx.send(push_event) {
                    error!("Failed to send push event: {}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to process event");
                }
            }
        }
    }

    (StatusCode::OK, "Webhook processed")
}

async fn handle_dockerhub_webhook(
    State(state): State<WebhookState>,
    Json(payload): Json<DockerHubWebhook>,
) -> impl IntoResponse {
    info!(
        "Received Docker Hub webhook for {}",
        payload.repository.repo_name
    );

    let push_event = ImagePushEvent {
        registry: "docker.io".to_string(),
        repository: payload.repository.repo_name,
        tag: payload.push_data.tag,
        digest: None,
    };

    if let Err(e) = state.event_tx.send(push_event) {
        error!("Failed to send push event: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to process event");
    }

    (StatusCode::OK, "Webhook processed")
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn process_webhook_events(mut rx: EventReceiver) {
    info!("Starting webhook event processor");

    while let Some(event) = rx.recv().await {
        info!("Processing image push event: {}", event.full_image());

        // TODO: Query Kubernetes for resources watching this image
        // TODO: Check policies and create update requests
        // This will be connected to the controller module
    }

    warn!("Webhook event processor stopped");
}

fn extract_registry(repository: &str) -> String {
    if repository.contains('/') {
        let parts: Vec<&str> = repository.splitn(2, '/').collect();
        if parts[0].contains('.') || parts[0].contains(':') {
            return parts[0].to_string();
        }
    }
    "docker.io".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_registry() {
        assert_eq!(extract_registry("nginx"), "docker.io");
        assert_eq!(extract_registry("library/nginx"), "docker.io");
        assert_eq!(extract_registry("gcr.io/project/image"), "gcr.io");
        assert_eq!(
            extract_registry("registry.example.com:5000/image"),
            "registry.example.com:5000"
        );
    }

    #[test]
    fn test_full_image() {
        let event = ImagePushEvent {
            registry: "docker.io".to_string(),
            repository: "nginx".to_string(),
            tag: "latest".to_string(),
            digest: None,
        };
        assert_eq!(event.full_image(), "nginx:latest");

        let event2 = ImagePushEvent {
            registry: "gcr.io".to_string(),
            repository: "project/image".to_string(),
            tag: "v1.0.0".to_string(),
            digest: None,
        };
        assert_eq!(event2.full_image(), "gcr.io/project/image:v1.0.0");
    }
}
