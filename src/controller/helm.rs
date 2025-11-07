use crate::helm::{HelmRepositoryClient, OciHelmClient};
use crate::metrics::{
    HELM_CHART_VERSIONS_CHECKED, HELM_RELEASES_WATCHED, HELM_REPOSITORY_ERRORS,
    HELM_REPOSITORY_QUERIES, HELM_REPOSITORY_QUERY_DURATION, HELM_UPDATES_APPROVED,
    HELM_UPDATES_FOUND, HELM_UPDATES_REJECTED, RECONCILE_DURATION, RECONCILE_ERRORS,
};
use crate::models::crd::{
    TargetRef, UpdatePhase, UpdatePolicyType, UpdateRequest, UpdateRequestSpec,
    UpdateRequestStatus, UpdateType,
};
use crate::models::policy::annotations;
use crate::models::{HelmRelease, HelmRepository, ResourcePolicy, UpdatePolicy};
use crate::policy::PolicyEngine;
use anyhow::Result;
use futures::StreamExt;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::{
    Api, Client, ResourceExt,
    api::ListParams,
    runtime::{Controller, controller::Action, watcher::Config},
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tracing::{debug, error, info, warn};

pub struct HelmController {
    client: Client,
    policy_engine: Arc<PolicyEngine>,
    auto_discovery_enabled: bool,
}

impl HelmController {
    pub async fn new(policy_engine: Arc<PolicyEngine>) -> Result<Self> {
        let client = Client::try_default().await?;

        // Check if auto-discovery is enabled via environment variable (default: true)
        let auto_discovery_enabled = std::env::var("HEADWIND_HELM_AUTO_DISCOVERY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(true);

        info!(
            "Helm controller initialized (auto-discovery: {})",
            auto_discovery_enabled
        );

        Ok(Self {
            client,
            policy_engine,
            auto_discovery_enabled,
        })
    }

    pub async fn run(self) {
        let api: Api<HelmRelease> = Api::all(self.client.clone());

        // Create Helm repository client for version discovery
        let helm_repo_client = HelmRepositoryClient::with_kube_client()
            .await
            .expect("Failed to create Helm repository client");

        // Create OCI Helm client for OCI registries
        let oci_helm_client = OciHelmClient::new();

        // Create context to pass to reconcile function
        let context = Arc::new(ControllerContext {
            client: self.client.clone(),
            policy_engine: self.policy_engine.clone(),
            helm_repo_client,
            oci_helm_client,
            auto_discovery_enabled: self.auto_discovery_enabled,
        });

        // Set up controller with exponential backoff
        Controller::new(api, Config::default())
            .shutdown_on_signal()
            .run(reconcile, error_policy, context)
            .filter_map(|x| async move { std::result::Result::ok(x) })
            .for_each(|_| futures::future::ready(()))
            .await;
    }
}

struct ControllerContext {
    client: Client,
    policy_engine: Arc<PolicyEngine>,
    helm_repo_client: HelmRepositoryClient,
    oci_helm_client: OciHelmClient,
    auto_discovery_enabled: bool,
}

async fn reconcile(
    helm_release: Arc<HelmRelease>,
    ctx: Arc<ControllerContext>,
) -> Result<Action, kube::Error> {
    let _timer = RECONCILE_DURATION.start_timer();

    let namespace = helm_release.namespace().ok_or_else(|| {
        kube::Error::Api(kube::error::ErrorResponse {
            status: "Failure".to_string(),
            message: "HelmRelease must be namespaced".to_string(),
            reason: "BadRequest".to_string(),
            code: 400,
        })
    })?;
    let name = helm_release.name_any();

    debug!(
        "Reconciling HelmRelease {}/{} (generation {})",
        namespace,
        name,
        helm_release.metadata.generation.unwrap_or_default()
    );

    // Parse policy from annotations
    let policy = parse_policy_from_annotations(helm_release.metadata.annotations.as_ref());

    if policy == UpdatePolicy::None {
        debug!(
            "HelmRelease {}/{} has policy=none, skipping",
            namespace, name
        );
        return Ok(Action::requeue(Duration::from_secs(3600)));
    }

    // Extract chart information
    let chart_name = &helm_release.spec.chart.spec.chart;
    let current_version = helm_release
        .spec
        .chart
        .spec
        .version
        .as_deref()
        .unwrap_or("*");

    debug!(
        "HelmRelease {}/{} - Chart: {}, Current version: {}, Policy: {:?}",
        namespace, name, chart_name, current_version, policy
    );

    // Update metrics
    update_helm_releases_count(&ctx.client).await;

    // Get current deployed version from status (last_attempted_revision for Flux v2)
    let deployed_version = helm_release
        .status
        .as_ref()
        .and_then(|s| s.last_attempted_revision.as_deref());

    // Determine the version to compare against
    // Priority: deployed_version > current_version (spec)
    let base_version = deployed_version.unwrap_or(current_version);

    // Only attempt auto-discovery if enabled
    if !ctx.auto_discovery_enabled {
        debug!(
            "HelmRelease {}/{} - Auto-discovery disabled, skipping",
            namespace, name
        );
        return Ok(Action::requeue(Duration::from_secs(3600)));
    }

    // Attempt to discover new versions from Helm repository
    if let Some(new_version) =
        discover_new_version(&ctx, &helm_release, chart_name, base_version, &policy).await
    {
        debug!(
            "HelmRelease {}/{} - New version {} discovered (current: {})",
            namespace, name, new_version, base_version
        );

        // Increment version check metric
        HELM_CHART_VERSIONS_CHECKED.inc();

        // Potential update available - increment found metric
        HELM_UPDATES_FOUND.inc();

        // Build resource policy from annotations
        let resource_policy =
            build_resource_policy(helm_release.metadata.annotations.as_ref(), policy);

        // Check if update should proceed based on policy
        match ctx
            .policy_engine
            .should_update(&resource_policy, base_version, &new_version)
        {
            Ok(true) => {
                // Increment approved metric
                HELM_UPDATES_APPROVED.inc();

                info!(
                    "HelmRelease {}/{} - Update from {} to {} approved by policy",
                    namespace, name, base_version, new_version
                );

                // Create and persist UpdateRequest
                match create_update_request(
                    ctx.client.clone(),
                    &namespace,
                    &name,
                    chart_name,
                    base_version,
                    &new_version,
                    &resource_policy,
                )
                .await
                {
                    Ok(update_request_name) => {
                        info!(
                            "Created update request {} for HelmRelease {}/{}",
                            update_request_name, namespace, name
                        );

                        // Send notification for UpdateRequest creation
                        crate::notifications::notify_update_request_created(
                            crate::notifications::DeploymentInfo {
                                name: name.clone(),
                                namespace: namespace.clone(),
                                current_image: format!("{}:{}", chart_name, base_version),
                                new_image: format!("{}:{}", chart_name, new_version),
                                container: None,
                                resource_kind: Some("HelmRelease".to_string()),
                            },
                            format!("{:?}", resource_policy.policy),
                            resource_policy.require_approval,
                            update_request_name,
                        );
                    },
                    Err(e) => {
                        warn!(
                            "Failed to create UpdateRequest for HelmRelease {}/{}: {}",
                            namespace, name, e
                        );
                    },
                }
            },
            Ok(false) => {
                // Increment rejected metric
                HELM_UPDATES_REJECTED.inc();

                debug!(
                    "HelmRelease {}/{} - Update from {} to {} rejected by policy",
                    namespace, name, base_version, new_version
                );
            },
            Err(e) => {
                warn!(
                    "HelmRelease {}/{} - Error checking update policy: {}",
                    namespace, name, e
                );
            },
        }
    } else {
        debug!(
            "HelmRelease {}/{} - No new version discovered",
            namespace, name
        );
    }

    // Requeue after a reasonable interval
    Ok(Action::requeue(Duration::from_secs(300)))
}

/// Discover new chart versions by querying the Helm repository (HTTP or OCI)
async fn discover_new_version(
    ctx: &Arc<ControllerContext>,
    helm_release: &HelmRelease,
    chart_name: &str,
    current_version: &str,
    policy: &UpdatePolicy,
) -> Option<String> {
    // Get the HelmRepository reference from the HelmRelease
    let source_ref = &helm_release.spec.chart.spec.source_ref;

    // Only handle HelmRepository sources (not GitRepository, Bucket, etc.)
    if source_ref.kind != "HelmRepository" {
        debug!(
            "HelmRelease references {}, not HelmRepository - skipping auto-discovery",
            source_ref.kind
        );
        return None;
    }

    let namespace = helm_release.namespace().unwrap_or_default();
    let repo_namespace = source_ref.namespace.as_deref().unwrap_or(&namespace);
    let repo_name = &source_ref.name;

    // Fetch the HelmRepository resource
    let repo_api: Api<HelmRepository> = Api::namespaced(ctx.client.clone(), repo_namespace);
    let helm_repo = match repo_api.get(repo_name).await {
        Ok(repo) => repo,
        Err(e) => {
            warn!(
                "Failed to fetch HelmRepository {}/{}: {}",
                repo_namespace, repo_name, e
            );
            return None;
        },
    };

    let repo_url = &helm_repo.spec.url;

    // Start timer for repository query duration
    let _timer = HELM_REPOSITORY_QUERY_DURATION.start_timer();

    // Determine if this is an OCI registry or HTTP repository
    if repo_url.starts_with("oci://") {
        // OCI Registry
        debug!("Detected OCI registry: {}", repo_url);
        discover_oci_version(
            ctx,
            &helm_repo,
            repo_namespace,
            chart_name,
            current_version,
            policy,
        )
        .await
    } else {
        // Traditional HTTP Helm repository
        debug!("Detected HTTP Helm repository: {}", repo_url);
        discover_http_version(
            ctx,
            &helm_repo,
            repo_namespace,
            chart_name,
            current_version,
            policy,
        )
        .await
    }
}

/// Discover versions from OCI registry
async fn discover_oci_version(
    ctx: &Arc<ControllerContext>,
    helm_repo: &HelmRepository,
    repo_namespace: &str,
    chart_name: &str,
    current_version: &str,
    policy: &UpdatePolicy,
) -> Option<String> {
    let repo_url = &helm_repo.spec.url;

    // Build full OCI URL with chart name
    let full_oci_url = format!("{}/{}", repo_url.trim_end_matches('/'), chart_name);

    debug!("Querying OCI registry for chart: {}", full_oci_url);

    // Get credentials if available
    let (username, password) = if let Some(secret_ref) = &helm_repo.spec.secret_ref {
        match ctx
            .helm_repo_client
            .read_secret_credentials(repo_namespace, &secret_ref.name)
            .await
        {
            Ok(creds) => (Some(creds.username), Some(creds.password)),
            Err(e) => {
                warn!(
                    "Failed to read credentials from secret {}/{}: {}",
                    repo_namespace, secret_ref.name, e
                );
                HELM_REPOSITORY_ERRORS.inc();
                return None;
            },
        }
    } else {
        (None, None)
    };

    // Increment repository query counter
    HELM_REPOSITORY_QUERIES.inc();

    // List available versions (tags) from OCI registry
    let versions = match ctx
        .oci_helm_client
        .get_chart_versions(&full_oci_url, username.as_deref(), password.as_deref())
        .await
    {
        Ok(versions) => versions,
        Err(e) => {
            warn!("Failed to list OCI tags from {}: {}", full_oci_url, e);
            HELM_REPOSITORY_ERRORS.inc();
            return None;
        },
    };

    debug!(
        "Found {} versions in OCI registry: {:?}",
        versions.len(),
        versions
    );

    // Find best version using policy
    ctx.oci_helm_client
        .find_best_version(&versions, current_version, policy)
}

/// Discover versions from HTTP Helm repository
async fn discover_http_version(
    ctx: &Arc<ControllerContext>,
    helm_repo: &HelmRepository,
    repo_namespace: &str,
    chart_name: &str,
    current_version: &str,
    policy: &UpdatePolicy,
) -> Option<String> {
    let repo_url = &helm_repo.spec.url;

    // Check if authentication is required
    let index = if let Some(secret_ref) = &helm_repo.spec.secret_ref {
        // Fetch credentials from Secret
        match ctx
            .helm_repo_client
            .read_secret_credentials(repo_namespace, &secret_ref.name)
            .await
        {
            Ok(creds) => {
                debug!(
                    "Using authentication for repository {} (secret: {})",
                    repo_url, secret_ref.name
                );

                // Increment repository query counter
                HELM_REPOSITORY_QUERIES.inc();

                match ctx
                    .helm_repo_client
                    .fetch_index_with_auth(repo_url, &creds.username, &creds.password)
                    .await
                {
                    Ok(idx) => idx,
                    Err(e) => {
                        warn!("Failed to fetch index from {} (with auth): {}", repo_url, e);
                        HELM_REPOSITORY_ERRORS.inc();
                        return None;
                    },
                }
            },
            Err(e) => {
                warn!(
                    "Failed to read credentials from secret {}/{}: {}",
                    repo_namespace, secret_ref.name, e
                );
                HELM_REPOSITORY_ERRORS.inc();
                return None;
            },
        }
    } else {
        // No authentication required
        debug!("Fetching public repository index from {}", repo_url);

        // Increment repository query counter
        HELM_REPOSITORY_QUERIES.inc();

        match ctx.helm_repo_client.fetch_index(repo_url).await {
            Ok(idx) => idx,
            Err(e) => {
                warn!("Failed to fetch index from {}: {}", repo_url, e);
                HELM_REPOSITORY_ERRORS.inc();
                return None;
            },
        }
    };

    // Find the best version matching the policy
    ctx.helm_repo_client
        .find_best_version(&index, chart_name, current_version, policy)
}

fn error_policy(
    _helm_release: Arc<HelmRelease>,
    error: &kube::Error,
    _ctx: Arc<ControllerContext>,
) -> Action {
    RECONCILE_ERRORS.inc();
    error!("Reconciliation error: {}", error);
    Action::requeue(Duration::from_secs(60))
}

fn parse_policy_from_annotations(annotations: Option<&BTreeMap<String, String>>) -> UpdatePolicy {
    annotations
        .and_then(|ann| ann.get(annotations::POLICY))
        .map(|policy_str| match policy_str.to_lowercase().as_str() {
            "patch" => UpdatePolicy::Patch,
            "minor" => UpdatePolicy::Minor,
            "major" => UpdatePolicy::Major,
            "all" => UpdatePolicy::All,
            "glob" => UpdatePolicy::Glob,
            "force" => UpdatePolicy::Force,
            "none" => UpdatePolicy::None,
            _ => {
                warn!("Unknown policy value: {}, defaulting to None", policy_str);
                UpdatePolicy::None
            },
        })
        .unwrap_or(UpdatePolicy::None)
}

async fn update_helm_releases_count(client: &Client) {
    let api: Api<HelmRelease> = Api::all(client.clone());
    match api.list(&ListParams::default()).await {
        Ok(list) => {
            HELM_RELEASES_WATCHED.set(list.items.len() as i64);
        },
        Err(e) => {
            error!("Failed to count HelmReleases: {}", e);
        },
    }
}

fn build_resource_policy(
    annotations: Option<&BTreeMap<String, String>>,
    policy: UpdatePolicy,
) -> ResourcePolicy {
    let pattern = annotations
        .and_then(|ann| ann.get(annotations::PATTERN))
        .map(|s| s.to_string());

    let require_approval = annotations
        .and_then(|ann| ann.get(annotations::REQUIRE_APPROVAL))
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(true);

    let min_update_interval = annotations
        .and_then(|ann| ann.get(annotations::MIN_UPDATE_INTERVAL))
        .and_then(|v| v.parse::<u64>().ok());

    ResourcePolicy {
        policy,
        pattern,
        require_approval,
        min_update_interval,
        images: Vec::new(),
    }
}

async fn create_update_request(
    client: kube::Client,
    namespace: &str,
    name: &str,
    chart_name: &str,
    current_version: &str,
    new_version: &str,
    policy: &ResourcePolicy,
) -> Result<String, kube::Error> {
    use kube::{Api, api::PostParams};

    let update_requests: Api<UpdateRequest> = Api::namespaced(client, namespace);

    let policy_type = match policy.policy {
        UpdatePolicy::Patch => UpdatePolicyType::Patch,
        UpdatePolicy::Minor => UpdatePolicyType::Minor,
        UpdatePolicy::Major => UpdatePolicyType::Major,
        UpdatePolicy::Glob => UpdatePolicyType::Glob,
        _ => UpdatePolicyType::None,
    };

    let ur_name = format!("{}-{}", name, chrono::Utc::now().timestamp());

    let spec = UpdateRequestSpec {
        target_ref: TargetRef {
            api_version: "helm.toolkit.fluxcd.io/v2".to_string(),
            kind: "HelmRelease".to_string(),
            name: name.to_string(),
            namespace: namespace.to_string(),
        },
        update_type: UpdateType::HelmChart,
        container_name: None,
        current_image: format!("{}:{}", chart_name, current_version),
        new_image: format!("{}:{}", chart_name, new_version),
        policy: policy_type,
        reason: Some(format!("New chart version {} available", new_version)),
        require_approval: policy.require_approval,
        expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(24)),
    };

    let status = UpdateRequestStatus {
        phase: if policy.require_approval {
            UpdatePhase::Pending
        } else {
            UpdatePhase::Approved
        },
        ..Default::default()
    };

    let update_request = UpdateRequest {
        metadata: ObjectMeta {
            name: Some(ur_name.clone()),
            namespace: Some(namespace.to_string()),
            ..Default::default()
        },
        spec,
        status: Some(status),
    };

    info!(
        "Creating UpdateRequest {} for HelmRelease {}/{}",
        ur_name, namespace, name
    );

    update_requests
        .create(&PostParams::default(), &update_request)
        .await?;

    Ok(ur_name)
}
