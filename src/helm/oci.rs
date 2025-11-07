use anyhow::{Context, Result};
use oci_distribution::secrets::RegistryAuth;
use oci_distribution::{Client, Reference};
use std::str::FromStr;
use tracing::{debug, warn};

/// OCI client for Helm charts stored in OCI registries
pub struct OciHelmClient {
    client: Client,
}

impl OciHelmClient {
    pub fn new() -> Self {
        let client = Client::new(oci_distribution::client::ClientConfig {
            protocol: oci_distribution::client::ClientProtocol::Https,
            ..Default::default()
        });

        Self { client }
    }

    /// List all tags for a Helm chart in an OCI registry
    ///
    /// oci_url format: oci://registry.example.com/repo/chart
    pub async fn list_tags(
        &self,
        oci_url: &str,
        auth: Option<RegistryAuth>,
    ) -> Result<Vec<String>> {
        // Parse OCI URL
        let url = oci_url
            .strip_prefix("oci://")
            .ok_or_else(|| anyhow::anyhow!("OCI URL must start with oci://"))?;

        debug!("Listing tags for OCI chart: {}", url);

        // Parse as OCI reference
        let reference = Reference::from_str(url).context("Failed to parse OCI reference")?;

        debug!(
            "Parsed OCI reference - registry: {:?}, repository: {:?}",
            reference.registry(),
            reference.repository()
        );

        // List tags
        let tag_response = self
            .client
            .list_tags(
                &reference,
                &auth.unwrap_or(RegistryAuth::Anonymous),
                None,
                None,
            )
            .await
            .context("Failed to list OCI tags")?;

        debug!(
            "Found {} tags for {}: {:?}",
            tag_response.tags.len(),
            url,
            tag_response.tags
        );

        Ok(tag_response.tags)
    }

    /// Get all available versions for a Helm chart in OCI format
    pub async fn get_chart_versions(
        &self,
        oci_url: &str,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Result<Vec<String>> {
        // Create authentication
        let auth = if let (Some(user), Some(pass)) = (username, password) {
            debug!("Using basic authentication for OCI registry");
            RegistryAuth::Basic(user.to_string(), pass.to_string())
        } else {
            debug!("Using anonymous access for OCI registry");
            RegistryAuth::Anonymous
        };

        // List tags (tags are versions in Helm OCI)
        let tags = self.list_tags(oci_url, Some(auth)).await?;

        // Filter out non-semver tags if needed
        // For now, return all tags
        Ok(tags)
    }

    /// Find the best version matching a policy
    ///
    /// This uses the same PolicyEngine logic as the HTTP repository client
    pub fn find_best_version(
        &self,
        versions: &[String],
        current_version: &str,
        policy: &crate::models::UpdatePolicy,
    ) -> Option<String> {
        if versions.is_empty() {
            warn!("No versions available");
            return None;
        }

        debug!(
            "Found {} versions, filtering with policy {:?} from current: {}",
            versions.len(),
            policy,
            current_version
        );

        // Filter versions that match the policy
        let policy_engine = crate::policy::PolicyEngine;
        let resource_policy = crate::models::ResourcePolicy {
            policy: *policy,
            pattern: None,
            require_approval: true,
            min_update_interval: None,
            images: Vec::new(),
        };

        let mut valid_versions: Vec<String> = versions
            .iter()
            .filter(
                |v| match policy_engine.should_update(&resource_policy, current_version, v) {
                    Ok(should_update) => should_update,
                    Err(e) => {
                        debug!("Version {} rejected by policy: {}", v, e);
                        false
                    },
                },
            )
            .cloned()
            .collect();

        if valid_versions.is_empty() {
            debug!("No valid update versions found");
            return None;
        }

        // Sort versions (newest first)
        valid_versions.sort_by(|a, b| {
            match (
                semver::Version::parse(a.trim_start_matches('v')),
                semver::Version::parse(b.trim_start_matches('v')),
            ) {
                (Ok(va), Ok(vb)) => vb.cmp(&va), // Reverse for descending order
                _ => b.cmp(a),                   // Fallback to string comparison
            }
        });

        debug!("Best version: {:?}", valid_versions.first());

        valid_versions.first().cloned()
    }
}

impl Default for OciHelmClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oci_url_parsing() {
        let url = "oci://registry.example.com/helm/my-chart";
        let stripped = url.strip_prefix("oci://").unwrap();
        assert_eq!(stripped, "registry.example.com/helm/my-chart");
    }

    #[test]
    fn test_find_best_version() {
        let client = OciHelmClient::new();
        let versions = vec![
            "1.0.0".to_string(),
            "1.1.0".to_string(),
            "1.2.0".to_string(),
            "2.0.0".to_string(),
        ];

        let best =
            client.find_best_version(&versions, "1.0.0", &crate::models::UpdatePolicy::Minor);

        assert_eq!(best, Some("1.2.0".to_string()));
    }
}
