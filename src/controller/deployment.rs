use crate::metrics::{DEPLOYMENTS_WATCHED, RECONCILE_DURATION, RECONCILE_ERRORS};
use crate::models::{annotations, ResourcePolicy, UpdatePolicy};
use crate::policy::PolicyEngine;
use anyhow::Result;
use futures::StreamExt;
use k8s_openapi::api::apps::v1::Deployment;
use kube::{
    api::{Api, Patch, PatchParams},
    client::Client,
    runtime::{
        controller::{Action, Controller},
        watcher::Config,
    },
    ResourceExt,
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
        let deployments: Api<Deployment> = Api::all(self.client.clone());

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

    // TODO: For each container in the deployment:
    // 1. Extract image and tag
    // 2. Check if we should update based on policy
    // 3. If update needed and approval required, create update request
    // 4. If update needed and no approval required, update immediately

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
}
