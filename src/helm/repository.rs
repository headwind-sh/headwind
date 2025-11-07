use anyhow::{Context, Result};
use k8s_openapi::api::core::v1::Secret;
use kube::Api;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, warn};

/// Represents a Helm repository index.yaml file
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexYaml {
    pub api_version: String,
    pub entries: HashMap<String, Vec<ChartEntry>>,
    #[serde(default)]
    pub generated: Option<String>,
}

/// Represents a single chart version entry in the index
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChartEntry {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub home: Option<String>,
    #[serde(default)]
    pub sources: Option<Vec<String>>,
    pub urls: Vec<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub app_version: Option<String>,
}

/// Credentials for Helm repository authentication
#[derive(Debug, Clone)]
pub struct RepositoryCredentials {
    pub username: String,
    pub password: String,
}

/// Client for querying Helm chart repositories
pub struct HelmRepositoryClient {
    client: Client,
    kube_client: Option<kube::Client>,
}

impl HelmRepositoryClient {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("headwind/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            kube_client: None,
        })
    }

    pub async fn with_kube_client() -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("headwind/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;

        let kube_client = kube::Client::try_default().await?;

        Ok(Self {
            client,
            kube_client: Some(kube_client),
        })
    }

    /// Read credentials from a Kubernetes Secret
    pub async fn read_secret_credentials(
        &self,
        namespace: &str,
        secret_name: &str,
    ) -> Result<RepositoryCredentials> {
        let kube_client = self
            .kube_client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Kubernetes client not initialized"))?;

        let secrets: Api<Secret> = Api::namespaced(kube_client.clone(), namespace);
        let secret = secrets.get(secret_name).await.context(format!(
            "Failed to get secret {}/{}",
            namespace, secret_name
        ))?;

        let data = secret
            .data
            .ok_or_else(|| anyhow::anyhow!("Secret has no data"))?;

        let username_bytes = data
            .get("username")
            .ok_or_else(|| anyhow::anyhow!("Secret missing 'username' key"))?;
        let password_bytes = data
            .get("password")
            .ok_or_else(|| anyhow::anyhow!("Secret missing 'password' key"))?;

        let username =
            String::from_utf8(username_bytes.0.clone()).context("Invalid UTF-8 in username")?;
        let password =
            String::from_utf8(password_bytes.0.clone()).context("Invalid UTF-8 in password")?;

        Ok(RepositoryCredentials { username, password })
    }

    /// Fetch and parse the index.yaml from a Helm repository
    pub async fn fetch_index(&self, repo_url: &str) -> Result<IndexYaml> {
        let index_url = if repo_url.ends_with('/') {
            format!("{}index.yaml", repo_url)
        } else {
            format!("{}/index.yaml", repo_url)
        };

        debug!("Fetching Helm repository index from: {}", index_url);

        let response = self
            .client
            .get(&index_url)
            .send()
            .await
            .context("Failed to fetch index.yaml")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "HTTP {} fetching index.yaml from {}",
                response.status(),
                index_url
            ));
        }

        let body = response
            .text()
            .await
            .context("Failed to read index.yaml response")?;

        let index: IndexYaml = serde_yaml::from_str(&body).context("Failed to parse index.yaml")?;

        debug!(
            "Successfully parsed index.yaml with {} charts",
            index.entries.len()
        );

        Ok(index)
    }

    /// Fetch index with basic authentication
    pub async fn fetch_index_with_auth(
        &self,
        repo_url: &str,
        username: &str,
        password: &str,
    ) -> Result<IndexYaml> {
        let index_url = if repo_url.ends_with('/') {
            format!("{}index.yaml", repo_url)
        } else {
            format!("{}/index.yaml", repo_url)
        };

        debug!(
            "Fetching Helm repository index with auth from: {}",
            index_url
        );

        let response = self
            .client
            .get(&index_url)
            .basic_auth(username, Some(password))
            .send()
            .await
            .context("Failed to fetch index.yaml with auth")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "HTTP {} fetching index.yaml from {} (with auth)",
                response.status(),
                index_url
            ));
        }

        let body = response
            .text()
            .await
            .context("Failed to read index.yaml response")?;

        let index: IndexYaml = serde_yaml::from_str(&body).context("Failed to parse index.yaml")?;

        debug!(
            "Successfully parsed index.yaml with {} charts (authenticated)",
            index.entries.len()
        );

        Ok(index)
    }

    /// Get all available versions for a specific chart
    pub fn get_chart_versions(&self, index: &IndexYaml, chart_name: &str) -> Vec<String> {
        index
            .entries
            .get(chart_name)
            .map(|entries| entries.iter().map(|entry| entry.version.clone()).collect())
            .unwrap_or_default()
    }

    /// Find the latest version for a chart that matches a version constraint
    /// Uses the same PolicyEngine logic as image updates
    pub fn find_best_version(
        &self,
        index: &IndexYaml,
        chart_name: &str,
        current_version: &str,
        policy: &crate::models::UpdatePolicy,
    ) -> Option<String> {
        let versions = self.get_chart_versions(index, chart_name);

        if versions.is_empty() {
            warn!("No versions found for chart: {}", chart_name);
            return None;
        }

        debug!(
            "Found {} versions for chart {}: {:?}",
            versions.len(),
            chart_name,
            versions
        );

        // Filter versions that are valid semver and newer than current
        let mut valid_versions: Vec<String> = versions
            .into_iter()
            .filter(|v| {
                // Use PolicyEngine to determine if this is a valid update
                let policy_engine = crate::policy::PolicyEngine;
                let resource_policy = crate::models::ResourcePolicy {
                    policy: *policy,
                    pattern: None,
                    require_approval: true,
                    min_update_interval: None,
                    images: Vec::new(),
                };

                match policy_engine.should_update(&resource_policy, current_version, v) {
                    Ok(should_update) => should_update,
                    Err(e) => {
                        debug!("Version {} rejected by policy: {}", v, e);
                        false
                    },
                }
            })
            .collect();

        if valid_versions.is_empty() {
            debug!("No valid update versions found for {}", chart_name);
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

        debug!(
            "Best version for {} (current: {}): {:?}",
            chart_name,
            current_version,
            valid_versions.first()
        );

        valid_versions.first().cloned()
    }
}

impl Default for HelmRepositoryClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default HelmRepositoryClient")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_index_yaml() {
        let yaml = r#"
apiVersion: v1
entries:
  alpine:
    - name: alpine
      version: 0.2.0
      description: Deploy a basic Alpine Linux pod
      urls:
        - https://example.com/charts/alpine-0.2.0.tgz
      created: "2016-10-06T16:23:20.499814565-06:00"
      digest: 99c76e403d752c84ead610644d4b1c2f2b453a74b921f422b9dcb8a7c8b559cd
    - name: alpine
      version: 0.1.0
      urls:
        - https://example.com/charts/alpine-0.1.0.tgz
      created: "2016-10-06T16:23:20.499543808-06:00"
"#;

        let index: IndexYaml = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(index.api_version, "v1");
        assert_eq!(index.entries.len(), 1);

        let alpine_versions = index.entries.get("alpine").unwrap();
        assert_eq!(alpine_versions.len(), 2);
        assert_eq!(alpine_versions[0].version, "0.2.0");
        assert_eq!(alpine_versions[1].version, "0.1.0");
    }

    #[test]
    fn test_get_chart_versions() {
        let yaml = r#"
apiVersion: v1
entries:
  nginx:
    - name: nginx
      version: 1.2.0
      urls: ["https://example.com/nginx-1.2.0.tgz"]
    - name: nginx
      version: 1.1.0
      urls: ["https://example.com/nginx-1.1.0.tgz"]
    - name: nginx
      version: 1.0.0
      urls: ["https://example.com/nginx-1.0.0.tgz"]
"#;

        let index: IndexYaml = serde_yaml::from_str(yaml).unwrap();
        let client = HelmRepositoryClient::new().unwrap();

        let versions = client.get_chart_versions(&index, "nginx");
        assert_eq!(versions.len(), 3);
        assert!(versions.contains(&"1.2.0".to_string()));
        assert!(versions.contains(&"1.1.0".to_string()));
        assert!(versions.contains(&"1.0.0".to_string()));
    }
}
