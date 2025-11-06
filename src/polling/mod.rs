use crate::metrics::{POLLING_CYCLES_TOTAL, POLLING_IMAGES_CHECKED, POLLING_NEW_TAGS_FOUND};
use crate::models::policy::{ResourcePolicy, UpdatePolicy, annotations};
use crate::models::webhook::ImagePushEvent;
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
}

/// Cache entry for tracking both tag and digest of an image
#[derive(Clone, Debug, PartialEq)]
struct CachedImageInfo {
    tag: String,
    digest: String,
}

/// Tracks the last seen tag and digest for each image
type ImageCache = Arc<RwLock<HashMap<String, CachedImageInfo>>>;

pub struct RegistryPoller {
    config: PollingConfig,
    cache: ImageCache,
    event_sender: crate::webhook::EventSender,
    client: Client,
}

impl RegistryPoller {
    pub async fn new(
        config: PollingConfig,
        event_sender: crate::webhook::EventSender,
    ) -> Result<Self> {
        let client = Client::try_default().await?;
        Ok(Self {
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            client,
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
                // Keep the event sender alive by moving it into an infinite loop
                // This prevents the webhook event channel from closing
                let _sender = self.event_sender;
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

        // Get list of images to track from Kubernetes
        let images = self.get_tracked_images().await?;
        info!("Found {} images to track", images.len());

        // Poll each image for updates
        for image_info in images {
            if let Err(e) = self.poll_image(&image_info).await {
                error!("Failed to poll image {}: {}", image_info.image, e);
            }
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

            let pattern = annotations.get(annotations::PATTERN).cloned();

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
        let auth = &RegistryAuth::Anonymous;

        // Step 1: Check if the current tag's digest has changed
        let current_digest = match client.fetch_manifest_digest(&reference, auth).await {
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
                .check_for_new_tags(&client, &reference, image_info)
                .await?
        {
            info!(
                "New tag discovered for {}: {} -> {}",
                image, current_tag, new_tag
            );

            // Fetch digest for the new tag
            let new_ref_str = format!("{}:{}", reference.repository(), new_tag);
            let new_ref = Reference::try_from(new_ref_str.as_str())?;

            if let Ok(new_digest) = client.fetch_manifest_digest(&new_ref, auth).await {
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
        image_info: &ImageToTrack,
    ) -> Result<Option<String>> {
        let auth = &RegistryAuth::Anonymous;

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
