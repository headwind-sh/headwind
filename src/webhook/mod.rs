use crate::metrics::{WEBHOOK_EVENTS_PROCESSED, WEBHOOK_EVENTS_TOTAL};
use crate::models::webhook::{DockerHubWebhook, ImagePushEvent, RegistryWebhook};
use crate::models::{ResourcePolicy, annotations};
use crate::policy::PolicyEngine;
use anyhow::Result;
use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::post};
use k8s_openapi::api::apps::v1::Deployment;
use kube::{Api, Client, ResourceExt};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};

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
    WEBHOOK_EVENTS_TOTAL.inc();

    info!(
        "Received registry webhook with {} events",
        payload.events.len()
    );

    for event in payload.events {
        if event.action == "push"
            && let Some(tag) = event.target.tag
        {
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

    // Create Kubernetes client
    let client = match Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create Kubernetes client: {}", e);
            return;
        },
    };

    let policy_engine = Arc::new(PolicyEngine);

    while let Some(event) = rx.recv().await {
        info!("Processing image push event: {}", event.full_image());

        if let Err(e) = process_image_push_event(&client, &policy_engine, &event).await {
            error!("Failed to process image push event: {}", e);
            continue;
        }

        WEBHOOK_EVENTS_PROCESSED.inc();
    }

    warn!("Webhook event processor stopped");
}

async fn process_image_push_event(
    client: &Client,
    policy_engine: &Arc<PolicyEngine>,
    event: &ImagePushEvent,
) -> Result<()> {
    // Query all deployments
    let deployments: Api<Deployment> = Api::all(client.clone());
    let deployment_list = deployments.list(&Default::default()).await?;

    debug!(
        "Checking {} deployments for matching images",
        deployment_list.items.len()
    );

    for deployment in deployment_list.items {
        // Check if deployment has headwind annotations
        let annotations = match &deployment.metadata.annotations {
            Some(ann) => ann,
            None => continue,
        };

        // Skip if no policy annotation
        if !annotations.contains_key(annotations::POLICY) {
            continue;
        }

        // Parse policy
        let policy = match parse_policy_from_annotations(annotations) {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    "Failed to parse policy for deployment {}: {}",
                    deployment.name_any(),
                    e
                );
                continue;
            },
        };

        // Check each container
        let spec = match deployment.spec.as_ref() {
            Some(s) => s,
            None => continue,
        };

        let template_spec = match spec.template.spec.as_ref() {
            Some(s) => s,
            None => continue,
        };

        for container in &template_spec.containers {
            // Skip containers not in the tracked images list (if specified)
            if !policy.images.is_empty() && !policy.images.contains(&container.name) {
                continue;
            }

            let current_image = match container.image.as_ref() {
                Some(img) => img,
                None => continue,
            };

            // Parse the current image to extract name and registry
            let (image_name, current_tag) = match parse_image_full(current_image) {
                Ok(parts) => parts,
                Err(e) => {
                    warn!("Failed to parse image {}: {}", current_image, e);
                    continue;
                },
            };

            // Check if this container uses the image from the webhook event
            if !images_match(&event.registry, &event.repository, &image_name) {
                continue;
            }

            info!(
                "Found matching deployment {}/{} container {} using {}",
                deployment.namespace().unwrap_or_default(),
                deployment.name_any(),
                container.name,
                current_image
            );

            // Build the new image tag
            let new_image = format_image(&event.registry, &event.repository, &event.tag);

            // Skip if it's the same version
            if current_tag == event.tag {
                debug!(
                    "Container {} already using tag {}, skipping",
                    container.name, event.tag
                );
                continue;
            }

            // Call the update handler
            if let Err(e) = crate::controller::handle_image_update(
                client.clone(),
                policy_engine.clone(),
                &deployment,
                &policy,
                &container.name,
                current_image,
                &new_image,
            )
            .await
            {
                error!(
                    "Failed to handle image update for {}/{}: {}",
                    deployment.namespace().unwrap_or_default(),
                    deployment.name_any(),
                    e
                );
            }
        }
    }

    Ok(())
}

/// Parse image into (full_name, tag)
/// Examples:
///   "nginx:1.25.0" -> ("nginx", "1.25.0")
///   "gcr.io/project/image:v1.0" -> ("gcr.io/project/image", "v1.0")
fn parse_image_full(image: &str) -> Result<(String, String)> {
    match image.rsplit_once(':') {
        Some((name, tag)) => {
            // Check if the part after ':' looks like a port
            if tag.contains('/') {
                Ok((image.to_string(), "latest".to_string()))
            } else {
                Ok((name.to_string(), tag.to_string()))
            }
        },
        None => Ok((image.to_string(), "latest".to_string())),
    }
}

/// Check if two images match (handling registry prefixes)
fn images_match(event_registry: &str, event_repository: &str, deployment_image: &str) -> bool {
    // Build the full event image name
    let event_image = if event_registry == "docker.io" {
        event_repository.to_string()
    } else {
        format!("{}/{}", event_registry, event_repository)
    };

    // Normalize deployment image (remove docker.io prefix if present)
    let normalized_deployment = if let Some(rest) = deployment_image.strip_prefix("docker.io/") {
        rest.to_string()
    } else {
        deployment_image.to_string()
    };

    event_image == normalized_deployment || event_repository == normalized_deployment
}

/// Format image with registry and tag
fn format_image(registry: &str, repository: &str, tag: &str) -> String {
    if registry == "docker.io" {
        format!("{}:{}", repository, tag)
    } else {
        format!("{}/{}:{}", registry, repository, tag)
    }
}

fn parse_policy_from_annotations(
    annotations: &std::collections::BTreeMap<String, String>,
) -> Result<ResourcePolicy> {
    let mut policy = ResourcePolicy::default();

    if let Some(policy_str) = annotations.get(annotations::POLICY) {
        policy.policy = policy_str.parse()?;
    }

    if let Some(pattern) = annotations.get(annotations::PATTERN) {
        policy.pattern = Some(pattern.clone());
    }

    if let Some(require_approval) = annotations.get(annotations::REQUIRE_APPROVAL) {
        policy.require_approval = require_approval.parse().unwrap_or(true);
    }

    if let Some(interval) = annotations.get(annotations::MIN_UPDATE_INTERVAL) {
        policy.min_update_interval = interval.parse().ok();
    }

    if let Some(images) = annotations.get(annotations::IMAGES) {
        policy.images = images
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    Ok(policy)
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

    #[test]
    fn test_parse_image_full() {
        // Image with tag
        let (name, tag) = parse_image_full("nginx:1.25.0").unwrap();
        assert_eq!(name, "nginx");
        assert_eq!(tag, "1.25.0");

        // Image with registry and tag
        let (name, tag) = parse_image_full("gcr.io/project/image:v1.0").unwrap();
        assert_eq!(name, "gcr.io/project/image");
        assert_eq!(tag, "v1.0");

        // Image without tag (defaults to latest)
        let (name, tag) = parse_image_full("nginx").unwrap();
        assert_eq!(name, "nginx");
        assert_eq!(tag, "latest");

        // Image with port in registry (no tag)
        let (name, tag) = parse_image_full("localhost:5000/myimage").unwrap();
        assert_eq!(name, "localhost:5000/myimage");
        assert_eq!(tag, "latest");
    }

    #[test]
    fn test_images_match() {
        // Docker Hub images (various formats)
        assert!(images_match("docker.io", "nginx", "nginx"));
        assert!(images_match("docker.io", "nginx", "docker.io/nginx"));
        assert!(images_match("docker.io", "library/nginx", "library/nginx"));
        assert!(images_match(
            "docker.io",
            "library/nginx",
            "docker.io/library/nginx"
        ));

        // Private registry images
        assert!(images_match(
            "gcr.io",
            "project/image",
            "gcr.io/project/image"
        ));
        assert!(images_match(
            "registry.example.com:5000",
            "myimage",
            "registry.example.com:5000/myimage"
        ));

        // Non-matching images
        assert!(!images_match("docker.io", "nginx", "redis"));
        assert!(!images_match(
            "gcr.io",
            "project/image",
            "gcr.io/other/image"
        ));
        assert!(!images_match("docker.io", "library/nginx", "nginx"));
    }

    #[test]
    fn test_format_image() {
        // Docker Hub
        assert_eq!(format_image("docker.io", "nginx", "1.25.0"), "nginx:1.25.0");
        assert_eq!(
            format_image("docker.io", "library/nginx", "latest"),
            "library/nginx:latest"
        );

        // Private registry
        assert_eq!(
            format_image("gcr.io", "project/image", "v1.0"),
            "gcr.io/project/image:v1.0"
        );
        assert_eq!(
            format_image("registry.example.com:5000", "myimage", "dev"),
            "registry.example.com:5000/myimage:dev"
        );
    }

    #[test]
    fn test_parse_policy_from_annotations() {
        use std::collections::BTreeMap;

        let mut annotations = BTreeMap::new();
        annotations.insert(annotations::POLICY.to_string(), "minor".to_string());
        annotations.insert(
            annotations::REQUIRE_APPROVAL.to_string(),
            "false".to_string(),
        );
        annotations.insert(annotations::IMAGES.to_string(), "nginx, redis".to_string());

        let policy = parse_policy_from_annotations(&annotations).unwrap();

        assert_eq!(policy.policy, crate::models::UpdatePolicy::Minor);
        assert!(!policy.require_approval);
        assert_eq!(policy.images, vec!["nginx", "redis"]);
    }
}
