use crate::metrics::{DEPLOYMENTS_WATCHED, RECONCILE_DURATION, RECONCILE_ERRORS};
use crate::models::{ResourcePolicy, UpdatePolicy, UpdateRequest, UpdateRequestSpec, TargetRef, UpdateType, UpdatePolicyType, annotations};
use crate::policy::PolicyEngine;
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
use tracing::{debug, error, info, instrument};

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
        let deployments: Api<Deployment> = Api::all(self.client.clone());

        info!("Creating controller for deployments");
        Controller::new(deployments, Config::default())
            .run(
                reconcile,
                error_policy,
                Arc::new(ControllerContext {
                    client: self.client,
                    policy_engine: self.policy_engine,
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
                        error!("Reconciliation error: {}", e);
                        RECONCILE_ERRORS.inc();
                    },
                }
            })
            .await;
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
            debug!("Skipping container {} (not in tracked images list)", container.name);
            continue;
        }

        let current_image = container.image.as_ref().ok_or_else(|| {
            create_error(&format!("Container {} has no image", container.name))
        })?;

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
#[allow(dead_code)]
async fn handle_image_update(
    ctx: Arc<ControllerContext>,
    deployment: &Deployment,
    policy: &ResourcePolicy,
    container_name: &str,
    current_image: &str,
    new_image: &str,
) -> Result<(), kube::Error> {
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

    info!(
        "Update available for {}/{} container {}: {} -> {}",
        namespace, name, container_name, current_tag, new_tag
    );

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
    let deployments: Api<Deployment> = Api::namespaced(client, namespace);

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
        assert_eq!(map_policy_to_crd(&UpdatePolicy::Major), UpdatePolicyType::Major);
        assert_eq!(map_policy_to_crd(&UpdatePolicy::Minor), UpdatePolicyType::Minor);
        assert_eq!(map_policy_to_crd(&UpdatePolicy::Patch), UpdatePolicyType::Patch);
        assert_eq!(map_policy_to_crd(&UpdatePolicy::Glob), UpdatePolicyType::Glob);
        assert_eq!(map_policy_to_crd(&UpdatePolicy::None), UpdatePolicyType::None);
        assert_eq!(map_policy_to_crd(&UpdatePolicy::All), UpdatePolicyType::None);
        assert_eq!(map_policy_to_crd(&UpdatePolicy::Force), UpdatePolicyType::None);
    }
}
