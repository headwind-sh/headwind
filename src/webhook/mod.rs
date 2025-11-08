use crate::metrics::{WEBHOOK_EVENTS_PROCESSED, WEBHOOK_EVENTS_TOTAL};
use crate::models::webhook::{ChartPushEvent, DockerHubWebhook, ImagePushEvent, RegistryWebhook};
use crate::models::{EventSource, ResourcePolicy, annotations};
use crate::policy::PolicyEngine;
use anyhow::Result;
use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::post};
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use kube::{Api, Client, ResourceExt};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};

pub type EventSender = mpsc::UnboundedSender<ImagePushEvent>;
pub type EventReceiver = mpsc::UnboundedReceiver<ImagePushEvent>;
pub type ChartEventSender = mpsc::UnboundedSender<ChartPushEvent>;
pub type ChartEventReceiver = mpsc::UnboundedReceiver<ChartPushEvent>;

#[derive(Clone)]
struct WebhookState {
    event_tx: EventSender,
    chart_event_tx: ChartEventSender,
}

pub async fn start_webhook_server() -> Result<(JoinHandle<()>, EventSender, ChartEventSender)> {
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let (chart_event_tx, chart_event_rx) = mpsc::unbounded_channel();

    // Clone senders to return them
    let event_tx_clone = event_tx.clone();
    let chart_event_tx_clone = chart_event_tx.clone();

    // Spawn processors for both event types
    tokio::spawn(process_webhook_events(event_rx));
    tokio::spawn(process_chart_events(chart_event_rx));

    let state = WebhookState {
        event_tx,
        chart_event_tx,
    };

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

    Ok((handle, event_tx_clone, chart_event_tx_clone))
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
            // Check if this is a Helm chart based on mediaType
            let is_helm_chart = event
                .target
                .media_type
                .as_ref()
                .map(|mt| mt.contains("helm.config"))
                .unwrap_or(false);

            if is_helm_chart {
                // This is a Helm chart push
                let chart_event = ChartPushEvent {
                    registry: extract_registry(&event.target.repository),
                    repository: event.target.repository.clone(),
                    version: tag,
                    digest: Some(event.target.digest),
                };

                info!(
                    "Detected Helm chart push: {} version {}",
                    chart_event.base_oci_url(),
                    chart_event.version
                );

                if let Err(e) = state.chart_event_tx.send(chart_event) {
                    error!("Failed to send chart push event: {}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to process event");
                }
            } else {
                // This is a container image push
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

/// Macro to generate process_* functions for different Kubernetes resource types.
/// This eliminates ~200 lines of duplicated code across process_deployments,
/// process_statefulsets, and process_daemonsets.
///
/// Each generated function:
/// 1. Queries all resources of the specified type
/// 2. Checks for headwind annotations
/// 3. Extracts pod template spec
/// 4. Iterates containers to find matching images
/// 5. Calls the resource-specific update handler
macro_rules! impl_process_resources {
    ($fn_name:ident, $resource_type:ty, $resource_name:expr, $handler_path:path) => {
        async fn $fn_name(
            client: &Client,
            policy_engine: &Arc<PolicyEngine>,
            event: &ImagePushEvent,
        ) -> Result<()> {
            let resources: Api<$resource_type> = Api::all(client.clone());
            let resource_list = resources.list(&Default::default()).await?;

            debug!(
                "Checking {} {}s for matching images",
                resource_list.items.len(),
                $resource_name
            );

            for resource in resource_list.items {
                // Check if resource has headwind annotations
                let annotations = match &resource.metadata.annotations {
                    Some(ann) => ann,
                    None => continue,
                };

                // Skip if no policy annotation
                if !annotations.contains_key(annotations::POLICY) {
                    continue;
                }

                // Parse policy to check event source
                let policy = match parse_policy_from_annotations(annotations) {
                    Ok(p) => p,
                    Err(e) => {
                        warn!(
                            "Failed to parse policy for {} {}: {}",
                            $resource_name,
                            resource.name_any(),
                            e
                        );
                        continue;
                    },
                };

                // Check event source - only process webhook events if event_source is "webhook" or "both"
                if policy.event_source != EventSource::Webhook
                    && policy.event_source != EventSource::Both
                {
                    debug!(
                        "Skipping {} {} - event source is {:?}, not webhook",
                        $resource_name,
                        resource.name_any(),
                        policy.event_source
                    );
                    continue;
                }

                // Check each container in the resource
                let spec = match resource.spec.as_ref() {
                    Some(s) => s,
                    None => continue,
                };

                let template_spec = match spec.template.spec.as_ref() {
                    Some(s) => s,
                    None => continue,
                };

                for container in &template_spec.containers {
                    let current_image = match container.image.as_ref() {
                        Some(img) => img,
                        None => continue,
                    };

                    // Parse the current image
                    let (image_name, current_tag) = match parse_image_full(current_image) {
                        Ok(parts) => parts,
                        Err(e) => {
                            warn!("Failed to parse image {}: {}", current_image, e);
                            continue;
                        },
                    };

                    // Check if this container uses the image from the webhook event
                    let matches = images_match(&event.registry, &event.repository, &image_name);
                    if !matches {
                        continue;
                    }

                    info!(
                        "Found matching {} {}/{} container {} using {}",
                        $resource_name,
                        resource.namespace().unwrap_or_default(),
                        resource.name_any(),
                        container.name,
                        current_image
                    );

                    // Skip if it's the same version
                    if current_tag == event.tag {
                        debug!(
                            "Container {} already using tag {}, skipping",
                            container.name, event.tag
                        );
                        continue;
                    }

                    // Call the update handler
                    if let Err(e) =
                        $handler_path(client, policy_engine, &resource, &image_name, &event.tag)
                            .await
                    {
                        error!(
                            "Failed to handle image update for {} {}/{}: {}",
                            $resource_name,
                            resource.namespace().unwrap_or_default(),
                            resource.name_any(),
                            e
                        );
                    }
                }
            }

            Ok(())
        }
    };
}

// Generate process_statefulsets and process_daemonsets functions using the macro
// Note: process_deployments is not generated here because it has a different signature
// and additional logic (policy parsing, image filtering, etc.) that doesn't match
// the StatefulSet/DaemonSet pattern
impl_process_resources!(
    process_statefulsets,
    StatefulSet,
    "statefulset",
    crate::controller::handle_statefulset_image_update
);

impl_process_resources!(
    process_daemonsets,
    DaemonSet,
    "daemonset",
    crate::controller::handle_daemonset_image_update
);

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

/// Process Helm chart push events
async fn process_chart_events(mut rx: ChartEventReceiver) {
    info!("Starting chart event processor");

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
        info!(
            "Processing Helm chart push event: {} version {}",
            event.base_oci_url(),
            event.version
        );

        if let Err(e) = process_chart_push_event(&client, &policy_engine, &event).await {
            error!("Failed to process chart push event: {}", e);
            continue;
        }

        WEBHOOK_EVENTS_PROCESSED.inc();
    }

    warn!("Chart event processor stopped");
}

/// Process a single chart push event by finding matching HelmReleases
async fn process_chart_push_event(
    client: &Client,
    policy_engine: &Arc<PolicyEngine>,
    event: &ChartPushEvent,
) -> Result<()> {
    // Import HelmRelease and HelmRepository types
    use crate::models::{HelmRelease, HelmRepository};

    // Query all HelmReleases
    let helm_releases: Api<HelmRelease> = Api::all(client.clone());
    let release_list = helm_releases.list(&Default::default()).await?;

    debug!(
        "Checking {} HelmReleases for matching charts",
        release_list.items.len()
    );

    // For each HelmRelease, check if it uses this chart
    for helm_release in release_list.items {
        // Check if release has headwind annotations
        let annotations = match &helm_release.metadata.annotations {
            Some(ann) => ann,
            None => continue,
        };

        // Skip if no policy annotation
        if !annotations.contains_key(annotations::POLICY) {
            continue;
        }

        // Parse policy to check event source
        let policy = match parse_policy_from_annotations(annotations) {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    "Failed to parse policy for HelmRelease {}: {}",
                    helm_release.name_any(),
                    e
                );
                continue;
            },
        };

        // Check event source - only process webhook events if event_source is "webhook" or "both"
        if policy.event_source != EventSource::Webhook && policy.event_source != EventSource::Both {
            debug!(
                "Skipping HelmRelease {} - event source is {:?}, not webhook",
                helm_release.name_any(),
                policy.event_source
            );
            continue;
        }

        // Get chart name and source ref from HelmRelease spec
        let chart_name = &helm_release.spec.chart.spec.chart;
        let source_ref = &helm_release.spec.chart.spec.source_ref;

        // Only process OCI HelmRepositories for now
        if source_ref.kind != "HelmRepository" {
            continue;
        }

        // Get the HelmRepository
        let namespace = helm_release.namespace().unwrap_or_default();
        let repo_namespace = source_ref.namespace.as_ref().unwrap_or(&namespace);
        let helm_repos: Api<HelmRepository> = Api::namespaced(client.clone(), repo_namespace);

        let helm_repo = match helm_repos.get(&source_ref.name).await {
            Ok(repo) => repo,
            Err(e) => {
                warn!(
                    "Failed to get HelmRepository {}/{}: {}",
                    repo_namespace, source_ref.name, e
                );
                continue;
            },
        };

        // Check if the repository URL matches the event
        let repo_url = &helm_repo.spec.url;

        // Match repository URL to event
        // Event: oci://registry.example.com/charts/mychart
        // HelmRepo URL: oci://registry.example.com
        // Chart name: mychart
        let expected_full_url = format!("{}/{}", repo_url.trim_end_matches('/'), chart_name);
        let event_full_url = event.base_oci_url();

        if expected_full_url != event_full_url {
            debug!(
                "Chart mismatch: expected={} event={}",
                expected_full_url, event_full_url
            );
            continue;
        }

        info!(
            "Found matching HelmRelease {}/{} for chart {} version {}",
            namespace,
            helm_release.name_any(),
            chart_name,
            event.version
        );

        // Call the helm controller's chart update handler
        if let Err(e) = crate::controller::handle_helm_chart_update(
            client,
            policy_engine,
            &helm_release,
            &event.version,
        )
        .await
        {
            error!(
                "Failed to handle chart update for {}/{}: {}",
                namespace,
                helm_release.name_any(),
                e
            );
        }
    }

    Ok(())
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

        // Check event source - only process webhook events if event_source is "webhook" or "both"
        if policy.event_source != EventSource::Webhook && policy.event_source != EventSource::Both {
            debug!(
                "Skipping deployment {} - event source is {:?}, not webhook",
                deployment.name_any(),
                policy.event_source
            );
            continue;
        }

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
            let matches = images_match(&event.registry, &event.repository, &image_name);
            debug!(
                "Image match check: event=({}, {}) deployment={} => {}",
                event.registry, event.repository, image_name, matches
            );
            if !matches {
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
            if let Err(e) = crate::controller::handle_deployment_image_update(
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

    // Process StatefulSets
    process_statefulsets(client, policy_engine, event).await?;

    // Process DaemonSets
    process_daemonsets(client, policy_engine, event).await?;

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
    use tracing::debug;

    debug!(
        "images_match: event_registry={}, event_repository={}, deployment_image={}",
        event_registry, event_repository, deployment_image
    );

    // Strip tag/digest from deployment image to get just the image name
    // We need to be careful: "registry.example.com:5000/image:tag" should become "registry.example.com:5000/image"
    // Split by @ first for digests, then find the last : that's after a / for tags
    let without_digest = deployment_image
        .split('@')
        .next()
        .unwrap_or(deployment_image);
    let deployment_image_name = if let Some(slash_pos) = without_digest.rfind('/') {
        // If there's a slash, only consider colons after it for tag splitting
        if let Some(colon_pos) = without_digest[slash_pos..].rfind(':') {
            &without_digest[..slash_pos + colon_pos]
        } else {
            without_digest
        }
    } else if let Some(colon_pos) = without_digest.rfind(':') {
        // No slash - check if this looks like a port (registry.example.com:5000) or a tag (nginx:1.27)
        // If there's a dot before the colon, it's likely a port
        if without_digest[..colon_pos].contains('.') {
            without_digest // Keep the port
        } else {
            &without_digest[..colon_pos] // Strip the tag
        }
    } else {
        without_digest
    };

    // Normalize deployment image (remove docker.io prefix if present)
    let normalized_deployment = if let Some(rest) = deployment_image_name.strip_prefix("docker.io/")
    {
        rest.to_string()
    } else {
        deployment_image_name.to_string()
    };

    // Build event image based on registry
    let event_image = if event_registry == "docker.io" {
        // Docker Hub with explicit registry
        event_repository.to_string()
    } else if event_registry == "library" {
        // Docker Hub official images - registry is sent as "library"
        // This should match deployment images like "nginx" or "library/nginx"
        event_repository.to_string()
    } else if !event_registry.contains('.') && !event_registry.contains(':') {
        // Likely a Docker Hub user/org (no domain or port)
        event_repository.to_string()
    } else {
        // External registry (contains . or :)
        format!("{}/{}", event_registry, event_repository)
    };

    debug!("  deployment_image_name: {}", deployment_image_name);
    debug!("  normalized_deployment: {}", normalized_deployment);
    debug!("  event_image: {}", event_image);

    // For Docker Hub official images, also check with library/ prefix
    // This handles several cases:
    // 1. registry="library" (Docker Hub webhook sends this for official images)
    // 2. registry="docker.io", repository without "/" (e.g., "nginx")
    // 3. registry="docker.io", repository="library/nginx" (explicit library namespace)
    let is_library_image = event_registry == "library"
        || (event_registry == "docker.io" && !event_repository.contains('/'))
        || (event_registry == "docker.io" && event_repository.starts_with("library/"));

    if is_library_image {
        // Match both "nginx" and "library/nginx" in deployment
        let deployment_without_library = normalized_deployment
            .strip_prefix("library/")
            .unwrap_or(&normalized_deployment);
        let event_without_library = event_repository
            .strip_prefix("library/")
            .unwrap_or(event_repository);

        debug!(
            "  Library path comparison: {} == {} => {}",
            event_without_library,
            deployment_without_library,
            event_without_library == deployment_without_library
        );
        event_without_library == deployment_without_library
    } else {
        // Standard comparison
        debug!(
            "  Standard comparison: {} == {} => {}",
            event_image,
            normalized_deployment,
            event_image == normalized_deployment
        );
        event_image == normalized_deployment
    }
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

        // Docker Hub official images with library namespace (the bug we're fixing)
        assert!(images_match("library", "nginx", "nginx:1.27.0"));
        assert!(images_match("library", "nginx", "nginx"));
        assert!(images_match("library", "nginx", "library/nginx"));

        // Non-matching images
        assert!(!images_match("docker.io", "nginx", "redis"));
        assert!(!images_match(
            "gcr.io",
            "project/image",
            "gcr.io/other/image"
        ));
        // When registry is "docker.io" and repository is "library/nginx",
        // it should match "nginx" (official image without explicit library prefix)
        assert!(images_match("docker.io", "library/nginx", "nginx"));
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
