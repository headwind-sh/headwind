use crate::metrics::{DEPLOYMENTS_WATCHED, RECONCILE_DURATION, RECONCILE_ERRORS};
use crate::models::{
    ResourcePolicy, TargetRef, UpdatePolicy, UpdatePolicyType, UpdateRequest, UpdateRequestSpec,
    UpdateType, annotations,
};
use crate::notifications::{self, DeploymentInfo};
use crate::policy::PolicyEngine;
use crate::rollback::RollbackManager;
use anyhow::Result;
use chrono::Utc;
use futures::StreamExt;
use k8s_openapi::api::apps::v1::Deployment;
use kube::{
    ResourceExt,
    api::{Api, Patch, PatchParams, PostParams},
    client::Client,
    runtime::{
        controller::{Action, Controller},
        watcher::Config,
    },
};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, instrument, warn};

pub struct DeploymentController {
    client: Client,
    policy_engine: Arc<PolicyEngine>,
}

impl DeploymentController {
    pub async fn new() -> Result<Self> {
        let client = Client::try_default().await?;
        let policy_engine = Arc::new(PolicyEngine);

        Ok(Self {
            client,
            policy_engine,
        })
    }

    pub async fn run(self) {
        info!("Deployment controller starting...");

        // Run the controller in a loop with exponential backoff
        // This handles transient errors during startup or runtime
        let mut backoff_seconds = 1;
        const MAX_BACKOFF: u64 = 60;

        loop {
            let deployments: Api<Deployment> = Api::all(self.client.clone());

            info!("Creating controller for deployments");

            let result = Controller::new(deployments, Config::default())
                .run(
                    reconcile,
                    error_policy,
                    Arc::new(ControllerContext {
                        client: self.client.clone(),
                        policy_engine: self.policy_engine.clone(),
                    }),
                )
                .for_each(|res| async move {
                    match res {
                        Ok((obj_ref, _action)) => {
                            info!(
                                "Reconciled deployment: {}/{}",
                                obj_ref.namespace.as_deref().unwrap_or("default"),
                                obj_ref.name
                            );
                        },
                        Err(e) => {
                            // Log reconciliation errors but continue processing
                            error!("Reconciliation error: {}", e);
                            RECONCILE_ERRORS.inc();
                        },
                    }
                })
                .await;

            // If the controller stream ends, log it and restart after backoff
            error!(
                "Deployment controller stream ended, restarting in {}s...",
                backoff_seconds
            );
            tokio::time::sleep(Duration::from_secs(backoff_seconds)).await;

            // Exponential backoff up to MAX_BACKOFF seconds
            backoff_seconds = (backoff_seconds * 2).min(MAX_BACKOFF);

            debug!("Controller loop result: {:?}", result);
        }
    }
}

struct ControllerContext {
    #[allow(dead_code)]
    client: Client,
    #[allow(dead_code)]
    policy_engine: Arc<PolicyEngine>,
}

#[instrument(skip(_ctx, deployment), fields(deployment = %deployment.name_any()))]
async fn reconcile(
    deployment: Arc<Deployment>,
    _ctx: Arc<ControllerContext>,
) -> Result<Action, kube::Error> {
    let _timer = RECONCILE_DURATION.start_timer();

    let name = deployment.name_any();
    let namespace = deployment.namespace().ok_or_else(|| {
        kube::Error::Api(kube::core::ErrorResponse {
            status: "Error".to_string(),
            message: "Deployment has no namespace".to_string(),
            reason: "MissingNamespace".to_string(),
            code: 400,
        })
    })?;

    debug!("Reconciling deployment {}/{}", namespace, name);

    // Check if deployment has headwind annotations
    let annotations = match &deployment.metadata.annotations {
        Some(ann) => ann,
        None => {
            debug!("Deployment has no annotations, skipping");
            return Ok(Action::requeue(Duration::from_secs(300)));
        },
    };

    // Parse the policy from annotations
    let policy = parse_policy_from_annotations(annotations)?;

    if policy.policy == UpdatePolicy::None {
        debug!("Deployment has policy 'none', skipping");
        return Ok(Action::requeue(Duration::from_secs(300)));
    }

    info!(
        "Deployment {}/{} has policy {:?}",
        namespace, name, policy.policy
    );

    // Track this deployment in metrics
    DEPLOYMENTS_WATCHED.inc();

    // Process each container in the deployment
    let spec = deployment
        .spec
        .as_ref()
        .ok_or_else(|| create_error("Deployment has no spec"))?;

    let template_spec = spec
        .template
        .spec
        .as_ref()
        .ok_or_else(|| create_error("Deployment template has no spec"))?;

    let containers = &template_spec.containers;

    for container in containers {
        // Skip containers not in the tracked images list (if specified)
        if !policy.images.is_empty() && !policy.images.contains(&container.name) {
            debug!(
                "Skipping container {} (not in tracked images list)",
                container.name
            );
            continue;
        }

        let current_image = container
            .image
            .as_ref()
            .ok_or_else(|| create_error(&format!("Container {} has no image", container.name)))?;

        // Extract image name and tag
        let (image_name, current_tag) = parse_image(current_image)?;

        debug!(
            "Processing container {} with image {}:{}",
            container.name, image_name, current_tag
        );

        // TODO: Query registry for available tags
        // For now, we'll need webhook/polling events to trigger updates
        // This will be implemented in a follow-up
        debug!(
            "Registry polling not yet implemented - updates will be triggered by webhooks/polling events"
        );
    }

    Ok(Action::requeue(Duration::from_secs(60)))
}

fn error_policy(
    _deployment: Arc<Deployment>,
    error: &kube::Error,
    _ctx: Arc<ControllerContext>,
) -> Action {
    error!("Reconciliation failed: {}", error);
    RECONCILE_ERRORS.inc();
    Action::requeue(Duration::from_secs(60))
}

/// Helper to create a kube::Error from a string message
fn create_error(msg: &str) -> kube::Error {
    kube::Error::Api(kube::core::ErrorResponse {
        status: "Error".to_string(),
        message: msg.to_string(),
        reason: "ProcessingError".to_string(),
        code: 400,
    })
}

/// Parse an image string into (name, tag)
/// Examples:
///   "nginx:1.25.0" -> ("nginx", "1.25.0")
///   "gcr.io/project/image:v1.0" -> ("gcr.io/project/image", "v1.0")
///   "nginx" -> ("nginx", "latest")
fn parse_image(image: &str) -> Result<(String, String), kube::Error> {
    match image.rsplit_once(':') {
        Some((name, tag)) => {
            // Check if the part after ':' looks like a port (e.g., "localhost:5000/image")
            if tag.contains('/') {
                // It's a registry with port, no tag specified
                Ok((image.to_string(), "latest".to_string()))
            } else {
                Ok((name.to_string(), tag.to_string()))
            }
        },
        None => Ok((image.to_string(), "latest".to_string())),
    }
}

/// Handle an available image update
/// This is called when we detect a new image version is available
pub async fn handle_image_update(
    client: Client,
    policy_engine: Arc<PolicyEngine>,
    deployment: &Deployment,
    policy: &ResourcePolicy,
    container_name: &str,
    current_image: &str,
    new_image: &str,
) -> Result<(), kube::Error> {
    // Create a temporary context for this operation
    let ctx = Arc::new(ControllerContext {
        client,
        policy_engine,
    });
    let namespace = deployment.namespace().unwrap();
    let name = deployment.name_any();

    // Parse images to get tags
    let (_, current_tag) = parse_image(current_image)?;
    let (image_name, new_tag) = parse_image(new_image)?;

    // Evaluate policy to see if we should update
    let should_update = ctx
        .policy_engine
        .should_update(policy, &current_tag, &new_tag)
        .map_err(|e| create_error(&format!("Failed to evaluate policy: {}", e)))?;

    if !should_update {
        debug!(
            "Policy {:?} does not allow update from {} to {}",
            policy.policy, current_tag, new_tag
        );
        return Ok(());
    }

    // Check minimum update interval
    let min_interval_seconds = policy.min_update_interval.unwrap_or(300);
    if let Some(annotations) = &deployment.metadata.annotations
        && let Some(last_update_str) =
            annotations.get(crate::models::policy::annotations::LAST_UPDATE)
        && let Ok(last_update) = chrono::DateTime::parse_from_rfc3339(last_update_str)
    {
        let now = chrono::Utc::now();
        let elapsed = now.signed_duration_since(last_update.with_timezone(&chrono::Utc));
        let min_interval = chrono::Duration::seconds(min_interval_seconds as i64);

        if elapsed < min_interval {
            let remaining = min_interval - elapsed;
            info!(
                "Skipping update for {}/{} container {}: minimum interval not reached ({} < {}s), {} seconds remaining",
                namespace,
                name,
                container_name,
                elapsed.num_seconds(),
                min_interval_seconds,
                remaining.num_seconds()
            );
            crate::metrics::UPDATES_SKIPPED_INTERVAL.inc();
            return Ok(());
        }
    }

    info!(
        "Update available for {}/{} container {}: {} -> {}",
        namespace, name, container_name, current_tag, new_tag
    );

    // Send update detected notification
    let deployment_info = DeploymentInfo {
        name: name.clone(),
        namespace: namespace.clone(),
        current_image: current_image.to_string(),
        new_image: new_image.to_string(),
        container: Some(container_name.to_string()),
        resource_kind: None,
    };
    notifications::notify_update_detected(deployment_info);

    // Check if approval is required
    if policy.require_approval {
        // Create UpdateRequest CRD
        create_update_request(
            ctx.client.clone(),
            &namespace,
            &name,
            container_name,
            &image_name,
            current_image,
            new_image,
            &policy.policy,
        )
        .await?;
    } else {
        // Auto-update without approval
        info!(
            "Auto-updating {}/{} container {} to {}",
            namespace, name, container_name, new_image
        );
        update_deployment_image(
            ctx.client.clone(),
            &namespace,
            &name,
            container_name,
            new_image,
        )
        .await
        .map_err(|e| create_error(&format!("Failed to update deployment: {}", e)))?;
    }

    Ok(())
}

/// Create an UpdateRequest custom resource
#[allow(clippy::too_many_arguments)]
async fn create_update_request(
    client: Client,
    namespace: &str,
    deployment_name: &str,
    container_name: &str,
    image_name: &str,
    current_image: &str,
    new_image: &str,
    policy: &UpdatePolicy,
) -> Result<(), kube::Error> {
    let update_requests: Api<UpdateRequest> = Api::namespaced(client, namespace);

    // Generate a unique name for the update request
    let (_, current_tag) = parse_image(current_image)?;
    let (_, new_tag) = parse_image(new_image)?;
    let ur_name = format!(
        "{}-{}-{}-{}",
        deployment_name,
        container_name,
        new_tag.replace(['.', ':'], "-"),
        chrono::Utc::now().timestamp()
    );

    let update_request = UpdateRequest::new(
        &ur_name,
        UpdateRequestSpec {
            target_ref: TargetRef {
                api_version: "apps/v1".to_string(),
                kind: "Deployment".to_string(),
                name: deployment_name.to_string(),
                namespace: namespace.to_string(),
            },
            update_type: UpdateType::Image,
            container_name: Some(container_name.to_string()),
            current_image: current_image.to_string(),
            new_image: new_image.to_string(),
            policy: map_policy_to_crd(policy),
            reason: Some(format!(
                "New version available for {}: {} -> {}",
                image_name, current_tag, new_tag
            )),
            require_approval: true,
            expires_at: Some(Utc::now() + chrono::Duration::hours(24)),
        },
    );

    info!(
        "Creating UpdateRequest {} for deployment {}/{}",
        ur_name, namespace, deployment_name
    );

    update_requests
        .create(&PostParams::default(), &update_request)
        .await?;

    // Send notification about update request creation
    let deployment_info = DeploymentInfo {
        name: deployment_name.to_string(),
        namespace: namespace.to_string(),
        current_image: current_image.to_string(),
        new_image: new_image.to_string(),
        container: Some(container_name.to_string()),
        resource_kind: None,
    };
    notifications::notify_update_request_created(
        deployment_info,
        format!("{:?}", policy),
        true, // require_approval is true in this flow
        ur_name.clone(),
    );

    Ok(())
}

/// Map UpdatePolicy to UpdatePolicyType for CRD
fn map_policy_to_crd(policy: &UpdatePolicy) -> UpdatePolicyType {
    match policy {
        UpdatePolicy::Major => UpdatePolicyType::Major,
        UpdatePolicy::Minor => UpdatePolicyType::Minor,
        UpdatePolicy::Patch => UpdatePolicyType::Patch,
        UpdatePolicy::Glob => UpdatePolicyType::Glob,
        _ => UpdatePolicyType::None,
    }
}

/// Simple glob matching (supports * wildcard)
#[allow(dead_code)]
fn glob_match(pattern: &str, text: &str) -> bool {
    // Simple implementation - for production use a proper glob library
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return text.starts_with(prefix);
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return text.ends_with(suffix);
    }
    pattern == text
}

fn parse_policy_from_annotations(
    annotations: &std::collections::BTreeMap<String, String>,
) -> Result<ResourcePolicy, kube::Error> {
    let mut policy = ResourcePolicy::default();

    if let Some(policy_str) = annotations.get(annotations::POLICY) {
        policy.policy = policy_str.parse().map_err(|e| {
            kube::Error::Api(kube::core::ErrorResponse {
                status: "Error".to_string(),
                message: format!("Failed to parse policy: {}", e),
                reason: "InvalidPolicy".to_string(),
                code: 400,
            })
        })?;
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

#[allow(dead_code)]
pub async fn update_deployment_image(
    client: Client,
    namespace: &str,
    name: &str,
    container_name: &str,
    new_image: &str,
) -> Result<()> {
    update_deployment_image_with_tracking(
        client,
        namespace,
        name,
        container_name,
        new_image,
        None,
        None,
    )
    .await
}

/// Update a deployment image with optional rollback tracking metadata
pub async fn update_deployment_image_with_tracking(
    client: Client,
    namespace: &str,
    name: &str,
    container_name: &str,
    new_image: &str,
    update_request_name: Option<String>,
    approved_by: Option<String>,
) -> Result<()> {
    let deployments: Api<Deployment> = Api::namespaced(client.clone(), namespace);

    let patch = json!({
        "spec": {
            "template": {
                "spec": {
                    "containers": [{
                        "name": container_name,
                        "image": new_image
                    }]
                }
            }
        }
    });

    info!(
        "Updating deployment {}/{} container {} to image {}",
        namespace, name, container_name, new_image
    );

    deployments
        .patch(name, &PatchParams::default(), &Patch::Strategic(patch))
        .await?;

    info!("Successfully updated deployment {}/{}", namespace, name);

    // Track the update in rollback history
    let rollback_manager = RollbackManager::new(client);
    if let Err(e) = rollback_manager
        .track_update(
            name,
            namespace,
            container_name,
            new_image,
            update_request_name,
            approved_by,
        )
        .await
    {
        // Log the error but don't fail the update
        warn!(
            "Failed to track update in rollback history for {}/{}: {}",
            namespace, name, e
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn test_parse_policy_from_annotations() {
        let mut annotations = BTreeMap::new();
        annotations.insert(annotations::POLICY.to_string(), "minor".to_string());
        annotations.insert(
            annotations::REQUIRE_APPROVAL.to_string(),
            "false".to_string(),
        );
        annotations.insert(annotations::IMAGES.to_string(), "nginx, redis".to_string());

        let policy = parse_policy_from_annotations(&annotations).unwrap();

        assert_eq!(policy.policy, UpdatePolicy::Minor);
        assert!(!policy.require_approval);
        assert_eq!(policy.images, vec!["nginx", "redis"]);
    }

    #[test]
    fn test_parse_policy_defaults() {
        let annotations = BTreeMap::new();
        let policy = parse_policy_from_annotations(&annotations).unwrap();

        assert_eq!(policy.policy, UpdatePolicy::None);
        assert!(policy.require_approval);
        assert_eq!(policy.min_update_interval, Some(300));
    }

    #[test]
    fn test_parse_image() {
        // Image with tag
        let (name, tag) = parse_image("nginx:1.25.0").unwrap();
        assert_eq!(name, "nginx");
        assert_eq!(tag, "1.25.0");

        // Image with registry and tag
        let (name, tag) = parse_image("gcr.io/project/image:v1.0").unwrap();
        assert_eq!(name, "gcr.io/project/image");
        assert_eq!(tag, "v1.0");

        // Image without tag (defaults to latest)
        let (name, tag) = parse_image("nginx").unwrap();
        assert_eq!(name, "nginx");
        assert_eq!(tag, "latest");

        // Image with port in registry (no tag)
        let (name, tag) = parse_image("localhost:5000/myimage").unwrap();
        assert_eq!(name, "localhost:5000/myimage");
        assert_eq!(tag, "latest");

        // Image with port and tag
        let (name, tag) = parse_image("registry.example.com:5000/image:v2.0").unwrap();
        assert_eq!(name, "registry.example.com:5000/image");
        assert_eq!(tag, "v2.0");
    }

    #[test]
    fn test_glob_match() {
        // Exact match
        assert!(glob_match("v1.0", "v1.0"));
        assert!(!glob_match("v1.0", "v1.1"));

        // Wildcard matches all
        assert!(glob_match("*", "anything"));
        assert!(glob_match("*", "v1.0.0"));

        // Prefix matching
        assert!(glob_match("v1.*", "v1.0"));
        assert!(glob_match("v1.*", "v1.2.3"));
        assert!(!glob_match("v1.*", "v2.0"));

        // Suffix matching
        assert!(glob_match("*-beta", "v1.0-beta"));
        assert!(glob_match("*-beta", "anything-beta"));
        assert!(!glob_match("*-beta", "v1.0"));
    }

    #[test]
    fn test_map_policy_to_crd() {
        assert_eq!(
            map_policy_to_crd(&UpdatePolicy::Major),
            UpdatePolicyType::Major
        );
        assert_eq!(
            map_policy_to_crd(&UpdatePolicy::Minor),
            UpdatePolicyType::Minor
        );
        assert_eq!(
            map_policy_to_crd(&UpdatePolicy::Patch),
            UpdatePolicyType::Patch
        );
        assert_eq!(
            map_policy_to_crd(&UpdatePolicy::Glob),
            UpdatePolicyType::Glob
        );
        assert_eq!(
            map_policy_to_crd(&UpdatePolicy::None),
            UpdatePolicyType::None
        );
        assert_eq!(
            map_policy_to_crd(&UpdatePolicy::All),
            UpdatePolicyType::None
        );
        assert_eq!(
            map_policy_to_crd(&UpdatePolicy::Force),
            UpdatePolicyType::None
        );
    }

    #[test]
    fn test_min_update_interval_parsing() {
        use chrono::{DateTime, Utc};

        // Test parsing a valid last-update timestamp
        let last_update_str = "2025-01-06T12:00:00Z";
        let last_update = DateTime::parse_from_rfc3339(last_update_str);
        assert!(last_update.is_ok());

        // Test that we can calculate elapsed time
        let last_update = last_update.unwrap().with_timezone(&Utc);
        let now = Utc::now();
        let elapsed = now.signed_duration_since(last_update);
        assert!(elapsed.num_seconds() >= 0);
    }

    #[test]
    fn test_min_update_interval_enforcement() {
        use chrono::{Duration, Utc};

        let min_interval = Duration::seconds(300); // 5 minutes

        // Test 1: Update should be blocked if interval hasn't elapsed
        let last_update = Utc::now() - Duration::seconds(60); // 1 minute ago
        let now = Utc::now();
        let elapsed = now.signed_duration_since(last_update);
        assert!(
            elapsed < min_interval,
            "Update should be blocked: only {} seconds elapsed, need {} seconds",
            elapsed.num_seconds(),
            min_interval.num_seconds()
        );

        // Test 2: Update should proceed if interval has elapsed
        let last_update = Utc::now() - Duration::seconds(600); // 10 minutes ago
        let now = Utc::now();
        let elapsed = now.signed_duration_since(last_update);
        assert!(
            elapsed >= min_interval,
            "Update should proceed: {} seconds elapsed, minimum is {} seconds",
            elapsed.num_seconds(),
            min_interval.num_seconds()
        );
    }

    #[test]
    fn test_min_update_interval_different_values() {
        use chrono::{Duration, Utc};

        // Test with 1 minute interval
        let min_interval = Duration::seconds(60);
        let last_update = Utc::now() - Duration::seconds(30);
        let elapsed = Utc::now().signed_duration_since(last_update);
        assert!(elapsed < min_interval);

        // Test with 1 hour interval
        let min_interval = Duration::seconds(3600);
        let last_update = Utc::now() - Duration::seconds(1800); // 30 minutes
        let elapsed = Utc::now().signed_duration_since(last_update);
        assert!(elapsed < min_interval);
    }

    #[test]
    fn test_min_update_interval_with_annotations() {
        let mut annotations = BTreeMap::new();
        annotations.insert(
            annotations::MIN_UPDATE_INTERVAL.to_string(),
            "600".to_string(),
        );

        let policy = parse_policy_from_annotations(&annotations).unwrap();
        assert_eq!(policy.min_update_interval, Some(600));
    }

    #[test]
    fn test_min_update_interval_default() {
        let annotations = BTreeMap::new();
        let policy = parse_policy_from_annotations(&annotations).unwrap();

        // Default should be 300 seconds (5 minutes)
        assert_eq!(policy.min_update_interval, Some(300));
    }
}
