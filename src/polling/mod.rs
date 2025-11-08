mod auth;

use self::auth::AuthManager;
use crate::metrics::{
    POLLING_CYCLES_TOTAL, POLLING_HELM_CHARTS_CHECKED, POLLING_HELM_NEW_VERSIONS_FOUND,
    POLLING_IMAGES_CHECKED, POLLING_NEW_TAGS_FOUND, POLLING_RESOURCES_FILTERED,
};
use crate::models::policy::{EventSource, ResourcePolicy, UpdatePolicy, annotations};
use crate::models::webhook::{ChartPushEvent, ImagePushEvent};
use crate::models::{HelmRelease, HelmRepository};
use crate::policy::PolicyEngine;
use anyhow::Result;
use k8s_openapi::api::apps::v1::Deployment;
use kube::{Api, Client};
use oci_distribution::{Client as OciClient, Reference, secrets::RegistryAuth};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Configuration for registry polling
#[derive(Clone, Debug)]
pub struct PollingConfig {
    /// How often to poll registries (in seconds)
    pub interval: u64,
    /// Enable/disable polling
    pub enabled: bool,
}

impl Default for PollingConfig {
    fn default() -> Self {
        Self {
            interval: 300,  // 5 minutes
            enabled: false, // Disabled by default, webhooks preferred
        }
    }
}

/// Metadata for an image to track
#[derive(Clone, Debug)]
pub(crate) struct ImageToTrack {
    image: String,
    policy: UpdatePolicy,
    #[allow(dead_code)] // Will be used for semver/glob matching in future
    pattern: Option<String>,
    namespace: String,
    /// Per-resource polling interval in seconds (overrides global interval)
    polling_interval: Option<u64>,
}

/// Metadata for a Helm chart to track
#[derive(Clone, Debug)]
pub(crate) struct HelmChartToTrack {
    chart_name: String,
    repository_url: String, // Full URL: oci://registry.io/charts/mychart OR https://charts.example.com
    repository_type: HelmRepositoryType,
    current_version: String,
    policy: UpdatePolicy,
    pattern: Option<String>,
    namespace: String,
    #[allow(dead_code)] // May be used for correlation in future
    release_name: String, // HelmRelease name for correlation
    /// Per-resource polling interval in seconds (overrides global interval)
    polling_interval: Option<u64>,
}

/// Type of Helm repository
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum HelmRepositoryType {
    Oci,
    Http,
}

/// Cache entry for tracking both tag and digest of an image
#[derive(Clone, Debug, PartialEq)]
struct CachedImageInfo {
    tag: String,
    digest: String,
}

/// Cache entry for tracking Helm chart versions
#[derive(Clone, Debug, PartialEq)]
struct CachedChartInfo {
    version: String,
}

/// Tracks the last seen tag and digest for each image
type ImageCache = Arc<RwLock<HashMap<String, CachedImageInfo>>>;

/// Tracks the last seen version for each Helm chart
type ChartCache = Arc<RwLock<HashMap<String, CachedChartInfo>>>;

/// Tracks the last poll time for each resource (by unique key)
type LastPollCache = Arc<RwLock<HashMap<String, std::time::Instant>>>;

pub struct RegistryPoller {
    config: PollingConfig,
    cache: ImageCache,
    chart_cache: ChartCache,
    last_poll_cache: LastPollCache,
    event_sender: crate::webhook::EventSender,
    chart_event_sender: crate::webhook::ChartEventSender,
    client: Client,
    auth_manager: Arc<RwLock<AuthManager>>,
}

impl RegistryPoller {
    pub async fn new(
        config: PollingConfig,
        event_sender: crate::webhook::EventSender,
        chart_event_sender: crate::webhook::ChartEventSender,
    ) -> Result<Self> {
        let client = Client::try_default().await?;
        let auth_manager = AuthManager::new(client.clone());
        Ok(Self {
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
            chart_cache: Arc::new(RwLock::new(HashMap::new())),
            last_poll_cache: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            chart_event_sender,
            client,
            auth_manager: Arc::new(RwLock::new(auth_manager)),
        })
    }

    pub async fn start(self) -> JoinHandle<()> {
        info!(
            "Starting registry poller (enabled: {}, interval: {}s)",
            self.config.enabled, self.config.interval
        );

        tokio::spawn(async move {
            if !self.config.enabled {
                info!("Registry polling is disabled");
                // Keep the event senders alive by moving them into an infinite loop
                // This prevents the webhook event channels from closing
                let _sender = self.event_sender;
                let _chart_sender = self.chart_event_sender;
                loop {
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                }
            }

            loop {
                if let Err(e) = self.poll_registries().await {
                    error!("Error polling registries: {}", e);
                }

                tokio::time::sleep(Duration::from_secs(self.config.interval)).await;
            }
        })
    }

    async fn poll_registries(&self) -> Result<()> {
        debug!("Starting registry poll cycle");
        POLLING_CYCLES_TOTAL.inc();

        let now = std::time::Instant::now();

        // Get list of images to track from Kubernetes
        let images = self.get_tracked_images().await?;
        info!("Found {} images to track", images.len());

        // Poll each image for updates, respecting per-resource intervals
        for image_info in images {
            let key = format!("image::{}", image_info.image);
            let interval = image_info.polling_interval.unwrap_or(self.config.interval);

            // Check if enough time has elapsed since last poll
            let should_poll = {
                let last_poll_cache = self.last_poll_cache.read().await;
                match last_poll_cache.get(&key) {
                    Some(last_poll) => now.duration_since(*last_poll).as_secs() >= interval,
                    None => true, // Never polled before
                }
            };

            if !should_poll {
                debug!(
                    "Skipping image {} - interval {}s not elapsed",
                    image_info.image, interval
                );
                continue;
            }

            if let Err(e) = self.poll_image(&image_info).await {
                error!("Failed to poll image {}: {}", image_info.image, e);
            }

            // Update last poll time
            let mut last_poll_cache = self.last_poll_cache.write().await;
            last_poll_cache.insert(key, now);
        }

        // Get list of Helm charts to track from Kubernetes
        let charts = self.get_tracked_helm_releases().await?;
        info!("Found {} Helm charts to track", charts.len());

        // Poll each chart for updates based on repository type, respecting per-resource intervals
        for chart_info in charts {
            let key = format!("chart::{}", chart_info.repository_url);
            let interval = chart_info.polling_interval.unwrap_or(self.config.interval);

            // Check if enough time has elapsed since last poll
            let should_poll = {
                let last_poll_cache = self.last_poll_cache.read().await;
                match last_poll_cache.get(&key) {
                    Some(last_poll) => now.duration_since(*last_poll).as_secs() >= interval,
                    None => true, // Never polled before
                }
            };

            if !should_poll {
                debug!(
                    "Skipping chart {} - interval {}s not elapsed",
                    chart_info.chart_name, interval
                );
                continue;
            }

            let result = match chart_info.repository_type {
                HelmRepositoryType::Oci => self.poll_oci_helm_chart(&chart_info).await,
                HelmRepositoryType::Http => self.poll_http_helm_chart(&chart_info).await,
            };

            if let Err(e) = result {
                error!(
                    "Failed to poll {:?} Helm chart {}: {}",
                    chart_info.repository_type, chart_info.chart_name, e
                );
            }

            // Update last poll time
            let mut last_poll_cache = self.last_poll_cache.write().await;
            last_poll_cache.insert(key, now);
        }

        info!("Registry poll cycle completed");
        Ok(())
    }

    /// Get the list of images to track from Kubernetes Deployments
    async fn get_tracked_images(&self) -> Result<Vec<ImageToTrack>> {
        let deployments: Api<Deployment> = Api::all(self.client.clone());
        let deployment_list = deployments.list(&Default::default()).await?;

        let mut images = Vec::new();
        let mut seen = HashSet::new(); // Track unique image+policy combinations

        for deployment in deployment_list.items {
            let metadata = &deployment.metadata;
            let annotations = match &metadata.annotations {
                Some(ann) => ann,
                None => continue,
            };

            // Skip deployments without headwind policy annotation
            let policy_str = match annotations.get(annotations::POLICY) {
                Some(p) if p != "none" => p,
                _ => continue,
            };

            let policy = match UpdatePolicy::from_str(policy_str) {
                Ok(p) => p,
                Err(e) => {
                    warn!("Invalid policy '{}': {}", policy_str, e);
                    continue;
                },
            };

            // Check event source - only poll if event_source is "polling" or "both"
            let event_source = annotations
                .get(annotations::EVENT_SOURCE)
                .and_then(|v| v.parse::<EventSource>().ok())
                .unwrap_or_default(); // defaults to Webhook

            if event_source != EventSource::Polling && event_source != EventSource::Both {
                debug!(
                    "Skipping deployment {}/{} - event source is {:?}, not polling",
                    metadata
                        .namespace
                        .as_ref()
                        .unwrap_or(&"default".to_string()),
                    metadata.name.as_ref().unwrap_or(&"unknown".to_string()),
                    event_source
                );
                POLLING_RESOURCES_FILTERED.inc();
                continue;
            }

            let pattern = annotations.get(annotations::PATTERN).cloned();

            // Parse per-resource polling interval (overrides global interval)
            let polling_interval = annotations
                .get(annotations::POLLING_INTERVAL)
                .and_then(|v| v.parse::<u64>().ok());

            debug!(
                "Processing deployment {}/{} with policy {:?}",
                metadata
                    .namespace
                    .as_ref()
                    .unwrap_or(&"default".to_string()),
                metadata.name.as_ref().unwrap_or(&"unknown".to_string()),
                policy
            );

            // Extract images from pod template
            if let Some(spec) = &deployment.spec
                && let Some(template) = &spec.template.spec
            {
                for container in &template.containers {
                    if let Some(image) = &container.image {
                        // Create unique key for deduplication
                        let key = format!("{}::{:?}", image, policy);
                        if seen.insert(key) {
                            debug!("  Adding image to track: {} (policy: {:?})", image, policy);
                            images.push(ImageToTrack {
                                image: image.clone(),
                                policy,
                                pattern: pattern.clone(),
                                namespace: metadata
                                    .namespace
                                    .clone()
                                    .unwrap_or_else(|| "default".to_string()),
                                polling_interval,
                            });
                        }
                    }
                }
            }
        }

        Ok(images)
    }

    /// Poll a specific image for updates
    /// Checks both for digest changes (same-tag updates) and new tags (new versions)
    #[allow(dead_code)]
    #[allow(private_interfaces)]
    pub async fn poll_image(&self, image_info: &ImageToTrack) -> Result<Option<String>> {
        let image = &image_info.image;
        let reference = Reference::try_from(image.as_str())?;
        let current_tag = reference.tag().unwrap_or("latest");

        debug!(
            "Polling image: {} (tag: {}, policy: {:?})",
            image, current_tag, image_info.policy
        );
        POLLING_IMAGES_CHECKED.inc();

        // Create OCI client
        let client = OciClient::new(Default::default());

        // Get authentication for this image
        let mut auth_manager = self.auth_manager.write().await;
        let auth = auth_manager
            .get_auth_for_image(&image_info.image, &image_info.namespace)
            .await?;
        drop(auth_manager);

        // Step 1: Check if the current tag's digest has changed
        let current_digest = match client.fetch_manifest_digest(&reference, &auth).await {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to fetch digest for {}: {}", image, e);
                return Ok(None);
            },
        };

        debug!(
            "Current digest for {}:{}: {}",
            image,
            current_tag,
            &current_digest[..16]
        );

        // Check cache
        let cache = self.cache.read().await;
        let cached_info = cache.get(image).cloned(); // Clone to avoid borrow issues
        let cache_key = image.to_string();
        drop(cache);

        // Check if current tag's digest changed (same-tag update detection)
        if let Some(cached) = cached_info {
            if cached.digest != current_digest {
                info!(
                    "Digest change detected for {}:{} - {} -> {}",
                    image,
                    current_tag,
                    &cached.digest[..12],
                    &current_digest[..12]
                );

                // Update cache
                let mut cache = self.cache.write().await;
                cache.insert(
                    cache_key.clone(),
                    CachedImageInfo {
                        tag: current_tag.to_string(),
                        digest: current_digest.clone(),
                    },
                );
                drop(cache);

                // Send event for digest change
                self.send_update_event(&reference, current_tag, &current_digest)?;
                POLLING_NEW_TAGS_FOUND.inc();
                return Ok(Some(current_digest));
            }
        } else {
            // First time seeing this image
            debug!("First poll for {}, caching current state", image);
            let mut cache = self.cache.write().await;
            cache.insert(
                cache_key.clone(),
                CachedImageInfo {
                    tag: current_tag.to_string(),
                    digest: current_digest.clone(),
                },
            );
            drop(cache);
        }

        // Step 2: Check for new tags (if policy allows)
        if image_info.policy != UpdatePolicy::None
            && image_info.policy != UpdatePolicy::Force
            && let Some(new_tag) = self
                .check_for_new_tags(&client, &reference, &auth, image_info)
                .await?
        {
            info!(
                "New tag discovered for {}: {} -> {}",
                image, current_tag, new_tag
            );

            // Fetch digest for the new tag
            let new_ref_str = format!("{}:{}", reference.repository(), new_tag);
            let new_ref = Reference::try_from(new_ref_str.as_str())?;

            if let Ok(new_digest) = client.fetch_manifest_digest(&new_ref, &auth).await {
                // Update cache to new tag
                let mut cache = self.cache.write().await;
                cache.insert(
                    cache_key,
                    CachedImageInfo {
                        tag: new_tag.clone(),
                        digest: new_digest.clone(),
                    },
                );
                drop(cache);

                // Send event for new tag
                self.send_update_event(&reference, &new_tag, &new_digest)?;
                POLLING_NEW_TAGS_FOUND.inc();
                return Ok(Some(new_digest));
            }
        }

        Ok(None)
    }

    /// Check for new tags that match the policy
    async fn check_for_new_tags(
        &self,
        client: &OciClient,
        reference: &Reference,
        auth: &RegistryAuth,
        image_info: &ImageToTrack,
    ) -> Result<Option<String>> {
        // List available tags
        let tag_response = match client.list_tags(reference, auth, None, None).await {
            Ok(resp) => resp,
            Err(e) => {
                debug!(
                    "Failed to list tags for {}: {} (registry may not support listing)",
                    reference.repository(),
                    e
                );
                return Ok(None);
            },
        };

        let current_tag = reference.tag().unwrap_or("latest");

        debug!(
            "Found {} tags for {} (current: {}, policy: {:?})",
            tag_response.tags.len(),
            reference.repository(),
            current_tag,
            image_info.policy
        );

        // Build ResourcePolicy from image_info
        let resource_policy = ResourcePolicy {
            policy: image_info.policy,
            pattern: image_info.pattern.clone(),
            require_approval: true,
            min_update_interval: None,
            images: vec![],
            event_source: Default::default(),
            polling_interval: None,
        };

        let policy_engine = PolicyEngine;
        let mut best_version: Option<String> = None;

        // Find the best matching tag according to policy
        for tag in &tag_response.tags {
            // Skip non-version-looking tags for semver policies
            if matches!(
                image_info.policy,
                UpdatePolicy::Patch | UpdatePolicy::Minor | UpdatePolicy::Major
            ) {
                // Quick sanity check: does it look like a version?
                // Must start with digit or 'v'
                if !tag
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit() || c == 'v')
                {
                    debug!("Skipping non-version tag: {}", tag);
                    continue;
                }
            }

            // Check if this tag should be considered for update
            match policy_engine.should_update(&resource_policy, current_tag, tag) {
                Ok(true) => {
                    debug!("Tag {} matches policy {:?}", tag, image_info.policy);

                    // If we don't have a best version yet, or this one is better
                    if best_version.is_none() {
                        best_version = Some(tag.clone());
                    } else if let Some(ref current_best) = best_version {
                        // Check if new tag is better than current best
                        match policy_engine.should_update(&resource_policy, current_best, tag) {
                            Ok(true) => {
                                debug!("Tag {} is better than current best {}", tag, current_best);
                                best_version = Some(tag.clone());
                            },
                            Ok(false) => {
                                debug!("Tag {} is not better than {}", tag, current_best);
                            },
                            Err(e) => {
                                debug!("Failed to compare {} with {}: {}", tag, current_best, e);
                            },
                        }
                    }
                },
                Ok(false) => {
                    debug!("Tag {} does not match policy", tag);
                },
                Err(e) => {
                    debug!("Failed to check if tag {} matches policy: {}", tag, e);
                },
            }
        }

        if let Some(ref best) = best_version {
            info!(
                "Best version found for {}: {} -> {} (policy: {:?})",
                reference.repository(),
                current_tag,
                best,
                image_info.policy
            );
        }

        Ok(best_version)
    }

    /// Send an update event for a new image version
    fn send_update_event(&self, reference: &Reference, tag: &str, digest: &str) -> Result<()> {
        let event = ImagePushEvent {
            registry: extract_registry(reference.registry()),
            repository: reference.repository().to_string(),
            tag: tag.to_string(),
            digest: Some(digest.to_string()),
        };

        if let Err(e) = self.event_sender.send(event) {
            error!("Failed to send polling event: {}", e);
        }

        Ok(())
    }

    /// Get the list of Helm charts to track from Kubernetes HelmReleases
    async fn get_tracked_helm_releases(&self) -> Result<Vec<HelmChartToTrack>> {
        let helm_releases: Api<HelmRelease> = Api::all(self.client.clone());
        let release_list = helm_releases.list(&Default::default()).await?;

        let mut charts = Vec::new();
        let mut seen = HashSet::new(); // Track unique chart+policy combinations

        for helm_release in release_list.items {
            let metadata = &helm_release.metadata;
            let annotations = match &metadata.annotations {
                Some(ann) => ann,
                None => continue,
            };

            // Skip HelmReleases without headwind policy annotation
            let policy_str = match annotations.get(annotations::POLICY) {
                Some(p) if p != "none" => p,
                _ => continue,
            };

            let policy = match UpdatePolicy::from_str(policy_str) {
                Ok(p) => p,
                Err(e) => {
                    warn!("Invalid policy '{}': {}", policy_str, e);
                    continue;
                },
            };

            // Check event source - only poll if event_source is "polling" or "both"
            let event_source = annotations
                .get(annotations::EVENT_SOURCE)
                .and_then(|v| v.parse::<EventSource>().ok())
                .unwrap_or_default(); // defaults to Webhook

            let namespace = metadata
                .namespace
                .clone()
                .unwrap_or_else(|| "default".to_string());
            let release_name = metadata
                .name
                .clone()
                .unwrap_or_else(|| "unknown".to_string());

            if event_source != EventSource::Polling && event_source != EventSource::Both {
                debug!(
                    "Skipping HelmRelease {}/{} - event source is {:?}, not polling",
                    namespace, release_name, event_source
                );
                POLLING_RESOURCES_FILTERED.inc();
                continue;
            }

            let pattern = annotations.get(annotations::PATTERN).cloned();

            // Parse per-resource polling interval (overrides global interval)
            let polling_interval = annotations
                .get(annotations::POLLING_INTERVAL)
                .and_then(|v| v.parse::<u64>().ok());

            // Get chart information from HelmRelease spec
            let chart_name = &helm_release.spec.chart.spec.chart;
            let source_ref = &helm_release.spec.chart.spec.source_ref;

            // Only process HelmRepository sources (not GitRepository, Bucket, etc.)
            if source_ref.kind != "HelmRepository" {
                debug!(
                    "Skipping HelmRelease {}/{} with source kind {}",
                    namespace, release_name, source_ref.kind
                );
                continue;
            }

            // Only process OCI repositories for now
            let repo_namespace = source_ref.namespace.as_ref().unwrap_or(&namespace);
            let helm_repos: Api<HelmRepository> =
                Api::namespaced(self.client.clone(), repo_namespace);

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

            // Determine repository type and build URL
            let repo_url = &helm_repo.spec.url;
            let (repository_url, repository_type) = if repo_url.starts_with("oci://") {
                // OCI repository: oci://registry.io/charts/mychart
                (
                    format!("{}/{}", repo_url.trim_end_matches('/'), chart_name),
                    HelmRepositoryType::Oci,
                )
            } else if repo_url.starts_with("http://") || repo_url.starts_with("https://") {
                // HTTP/HTTPS repository: just use the base URL
                (repo_url.clone(), HelmRepositoryType::Http)
            } else {
                debug!(
                    "Skipping HelmRepository {} with unsupported URL scheme: {}",
                    source_ref.name, repo_url
                );
                continue;
            };

            // Get current version
            let current_version = match &helm_release.spec.chart.spec.version {
                Some(v) => v.clone(),
                None => {
                    debug!(
                        "Skipping HelmRelease {}/{} without version specified",
                        namespace, release_name
                    );
                    continue;
                },
            };

            // Create unique key for deduplication
            let key = format!("{}::{:?}", repository_url, policy);
            if seen.insert(key) {
                debug!(
                    "  Adding Helm chart to track: {} (type: {:?}, version: {}, policy: {:?})",
                    repository_url, repository_type, current_version, policy
                );
                charts.push(HelmChartToTrack {
                    chart_name: chart_name.clone(),
                    repository_url,
                    repository_type,
                    current_version,
                    policy,
                    pattern,
                    namespace,
                    release_name,
                    polling_interval,
                });
            }
        }

        Ok(charts)
    }

    /// Poll a specific OCI Helm chart for updates
    async fn poll_oci_helm_chart(&self, chart_info: &HelmChartToTrack) -> Result<()> {
        debug!(
            "Polling OCI Helm chart: {} (version: {}, policy: {:?})",
            chart_info.repository_url, chart_info.current_version, chart_info.policy
        );
        POLLING_HELM_CHARTS_CHECKED.inc();

        // Parse OCI URL to get registry and repository
        // Format: oci://registry.io/path/to/chart
        let url_without_scheme = chart_info
            .repository_url
            .strip_prefix("oci://")
            .ok_or_else(|| anyhow::anyhow!("Invalid OCI URL: {}", chart_info.repository_url))?;

        let reference_str = format!("{}:{}", url_without_scheme, chart_info.current_version);
        let reference = Reference::try_from(reference_str.as_str())?;

        // Create OCI client
        let client = OciClient::new(Default::default());

        // Get authentication for this chart (charts use same auth as images)
        let mut auth_manager = self.auth_manager.write().await;
        let auth = auth_manager
            .get_auth_for_image(&chart_info.repository_url, &chart_info.namespace)
            .await?;
        drop(auth_manager);

        // List available versions (tags)
        let tag_response = match client.list_tags(&reference, &auth, None, None).await {
            Ok(resp) => resp,
            Err(e) => {
                debug!(
                    "Failed to list tags for {}: {} (registry may not support listing)",
                    reference.repository(),
                    e
                );
                return Ok(());
            },
        };

        let current_version = &chart_info.current_version;
        let policy_engine = Arc::new(PolicyEngine);

        // Build ResourcePolicy for policy checks
        let resource_policy = ResourcePolicy {
            policy: chart_info.policy,
            pattern: chart_info.pattern.clone(),
            require_approval: true,
            min_update_interval: None,
            images: Vec::new(),
            event_source: Default::default(),
            polling_interval: None,
        };

        let mut best_version: Option<String> = None;

        // Check each tag to find the best match
        for tag in &tag_response.tags {
            // Skip non-semantic version tags for semver policies
            if matches!(
                chart_info.policy,
                UpdatePolicy::Patch | UpdatePolicy::Minor | UpdatePolicy::Major
            ) {
                if tag.starts_with('v') {
                    // semver crate handles v prefix
                } else if semver::Version::parse(tag).is_err() {
                    debug!("Skipping non-version tag: {}", tag);
                    continue;
                }
            }

            // Check if this version should be considered for update
            match policy_engine.should_update(&resource_policy, current_version, tag) {
                Ok(true) => {
                    debug!("Version {} matches policy {:?}", tag, chart_info.policy);

                    // If we don't have a best version yet, or this one is better
                    if best_version.is_none() {
                        best_version = Some(tag.clone());
                    } else if let Some(ref current_best) = best_version {
                        // Check if new version is better than current best
                        match policy_engine.should_update(&resource_policy, current_best, tag) {
                            Ok(true) => {
                                debug!(
                                    "Version {} is better than current best {}",
                                    tag, current_best
                                );
                                best_version = Some(tag.clone());
                            },
                            Ok(false) => {
                                debug!("Version {} is not better than {}", tag, current_best);
                            },
                            Err(e) => {
                                debug!("Failed to compare {} with {}: {}", tag, current_best, e);
                            },
                        }
                    }
                },
                Ok(false) => {
                    debug!("Version {} does not match policy", tag);
                },
                Err(e) => {
                    debug!("Failed to check if version {} matches policy: {}", tag, e);
                },
            }
        }

        // If we found a better version, send an event
        if let Some(new_version) = best_version
            && &new_version != current_version
        {
            info!(
                "New version found for Helm chart {}: {} -> {} (policy: {:?})",
                chart_info.chart_name, current_version, new_version, chart_info.policy
            );

            // Check cache to avoid duplicate events
            let cache_key = chart_info.repository_url.clone();
            let cache = self.chart_cache.read().await;
            let should_send = match cache.get(&cache_key) {
                Some(cached) => cached.version != new_version,
                None => true,
            };
            drop(cache);

            if should_send {
                // Update cache
                let mut cache = self.chart_cache.write().await;
                cache.insert(
                    cache_key,
                    CachedChartInfo {
                        version: new_version.clone(),
                    },
                );
                drop(cache);

                // Send chart event
                self.send_chart_event(
                    &chart_info.repository_url,
                    &chart_info.chart_name,
                    &new_version,
                )?;
                POLLING_HELM_NEW_VERSIONS_FOUND.inc();
            }
        }

        Ok(())
    }

    /// Poll a specific HTTP/HTTPS Helm chart for updates
    async fn poll_http_helm_chart(&self, chart_info: &HelmChartToTrack) -> Result<()> {
        debug!(
            "Polling HTTP Helm chart: {} (chart: {}, version: {}, policy: {:?})",
            chart_info.repository_url,
            chart_info.chart_name,
            chart_info.current_version,
            chart_info.policy
        );
        POLLING_HELM_CHARTS_CHECKED.inc();

        // Fetch index.yaml from HTTP repository
        let index_url = format!(
            "{}/index.yaml",
            chart_info.repository_url.trim_end_matches('/')
        );
        debug!("Fetching Helm repository index from: {}", index_url);

        let response = match reqwest::get(&index_url).await {
            Ok(resp) => resp,
            Err(e) => {
                debug!("Failed to fetch Helm repository index: {}", e);
                return Ok(());
            },
        };

        let index_yaml = match response.text().await {
            Ok(text) => text,
            Err(e) => {
                debug!("Failed to read Helm repository index response: {}", e);
                return Ok(());
            },
        };

        // Parse index.yaml
        let index: serde_yaml::Value = match serde_yaml::from_str(&index_yaml) {
            Ok(idx) => idx,
            Err(e) => {
                debug!("Failed to parse Helm repository index YAML: {}", e);
                return Ok(());
            },
        };

        // Extract versions for this specific chart
        let entries = match index.get("entries") {
            Some(serde_yaml::Value::Mapping(map)) => map,
            _ => {
                debug!("Helm index.yaml missing 'entries' field");
                return Ok(());
            },
        };

        let chart_versions =
            match entries.get(serde_yaml::Value::String(chart_info.chart_name.clone())) {
                Some(serde_yaml::Value::Sequence(versions)) => versions,
                _ => {
                    debug!(
                        "Chart '{}' not found in repository index",
                        chart_info.chart_name
                    );
                    return Ok(());
                },
            };

        let current_version = &chart_info.current_version;
        let policy_engine = Arc::new(PolicyEngine);

        // Build ResourcePolicy for policy checks
        let resource_policy = ResourcePolicy {
            policy: chart_info.policy,
            pattern: chart_info.pattern.clone(),
            require_approval: true,
            min_update_interval: None,
            images: Vec::new(),
            event_source: Default::default(),
            polling_interval: None,
        };

        let mut best_version: Option<String> = None;

        // Check each version from the index
        for version_entry in chart_versions {
            // Extract version field
            let version = match version_entry.get("version") {
                Some(serde_yaml::Value::String(v)) => v,
                _ => continue,
            };

            // Skip non-semantic version tags for semver policies
            if matches!(
                chart_info.policy,
                UpdatePolicy::Patch | UpdatePolicy::Minor | UpdatePolicy::Major
            ) {
                if version.starts_with('v') {
                    // semver crate handles v prefix
                } else if semver::Version::parse(version).is_err() {
                    debug!("Skipping non-version tag: {}", version);
                    continue;
                }
            }

            // Check if this version should be considered for update
            match policy_engine.should_update(&resource_policy, current_version, version) {
                Ok(true) => {
                    debug!("Version {} matches policy {:?}", version, chart_info.policy);

                    // If we don't have a best version yet, or this one is better
                    if best_version.is_none() {
                        best_version = Some(version.clone());
                    } else if let Some(ref current_best) = best_version {
                        // Check if new version is better than current best
                        match policy_engine.should_update(&resource_policy, current_best, version) {
                            Ok(true) => {
                                debug!(
                                    "Version {} is better than current best {}",
                                    version, current_best
                                );
                                best_version = Some(version.clone());
                            },
                            Ok(false) => {
                                debug!("Version {} is not better than {}", version, current_best);
                            },
                            Err(e) => {
                                debug!(
                                    "Failed to compare {} with {}: {}",
                                    version, current_best, e
                                );
                            },
                        }
                    }
                },
                Ok(false) => {
                    debug!("Version {} does not match policy", version);
                },
                Err(e) => {
                    debug!(
                        "Failed to check if version {} matches policy: {}",
                        version, e
                    );
                },
            }
        }

        // If we found a better version, send an event
        if let Some(new_version) = best_version
            && &new_version != current_version
        {
            info!(
                "New version found for Helm chart {}: {} -> {} (policy: {:?})",
                chart_info.chart_name, current_version, new_version, chart_info.policy
            );

            // Check cache to avoid duplicate events
            let cache_key = format!("{}::{}", chart_info.repository_url, chart_info.chart_name);
            let cache = self.chart_cache.read().await;
            let should_send = match cache.get(&cache_key) {
                Some(cached) => cached.version != new_version,
                None => true,
            };
            drop(cache);

            if should_send {
                // Update cache
                let mut cache = self.chart_cache.write().await;
                cache.insert(
                    cache_key,
                    CachedChartInfo {
                        version: new_version.clone(),
                    },
                );
                drop(cache);

                // Send chart event
                self.send_chart_event(
                    &chart_info.repository_url,
                    &chart_info.chart_name,
                    &new_version,
                )?;
                POLLING_HELM_NEW_VERSIONS_FOUND.inc();
            }
        }

        Ok(())
    }

    /// Send a chart update event
    /// Supports both OCI (oci://registry.io/path/to/chart) and HTTP (<https://charts.example.com>) URLs
    fn send_chart_event(
        &self,
        repository_url: &str,
        chart_name: &str,
        version: &str,
    ) -> Result<()> {
        let (registry, repository) = if repository_url.starts_with("oci://") {
            // Parse OCI URL: oci://registry.io/path/to/chart
            let url_without_scheme = repository_url
                .strip_prefix("oci://")
                .ok_or_else(|| anyhow::anyhow!("Invalid OCI URL: {}", repository_url))?;

            // Split into registry and repository
            let parts: Vec<&str> = url_without_scheme.splitn(2, '/').collect();
            if parts.len() != 2 {
                return Err(anyhow::anyhow!(
                    "Invalid OCI URL format: {}",
                    repository_url
                ));
            }

            (parts[0].to_string(), parts[1].to_string())
        } else {
            // HTTP/HTTPS URL: extract hostname as registry, chart name as repository
            // Example: https://charts.example.com -> registry: charts.example.com, repo: chartname
            let url = reqwest::Url::parse(repository_url)?;
            let registry = url
                .host_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid HTTP URL: no host"))?
                .to_string();

            (registry, chart_name.to_string())
        };

        let event = ChartPushEvent {
            registry,
            repository,
            version: version.to_string(),
            digest: None,
        };

        if let Err(e) = self.chart_event_sender.send(event) {
            error!("Failed to send chart polling event: {}", e);
        }

        Ok(())
    }
}

fn extract_registry(registry: &str) -> String {
    if registry.is_empty() {
        "docker.io".to_string()
    } else {
        registry.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_polling_config_default() {
        let config = PollingConfig::default();
        assert_eq!(config.interval, 300);
        assert!(!config.enabled);
    }

    #[test]
    fn test_extract_registry() {
        assert_eq!(extract_registry(""), "docker.io");
        assert_eq!(extract_registry("gcr.io"), "gcr.io");
        assert_eq!(
            extract_registry("registry.example.com"),
            "registry.example.com"
        );
    }
}
