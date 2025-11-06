use anyhow::{Context, Result};
use base64::prelude::*;
use k8s_openapi::api::core::v1::{Secret, ServiceAccount};
use kube::{Api, Client};
use oci_distribution::secrets::RegistryAuth;
use serde::Deserialize;
use std::collections::HashMap;
use tracing::{debug, warn};

/// Docker config.json structure
#[derive(Debug, Deserialize)]
struct DockerConfig {
    auths: HashMap<String, DockerAuthEntry>,
}

/// Auth entry in docker config
#[derive(Debug, Deserialize)]
struct DockerAuthEntry {
    #[serde(default)]
    auth: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
}

/// Credentials for a specific registry
#[derive(Debug, Clone)]
pub struct RegistryCredentials {
    #[allow(dead_code)] // Kept for future use
    pub registry: String,
    pub username: String,
    pub password: String,
}

/// Manager for registry authentication
pub struct AuthManager {
    client: Client,
    /// Cache of registry -> credentials
    credentials_cache: HashMap<String, RegistryCredentials>,
}

impl AuthManager {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            credentials_cache: HashMap::new(),
        }
    }

    /// Get authentication for a specific image
    /// Returns RegistryAuth for use with OCI client
    pub async fn get_auth_for_image(&mut self, image: &str, namespace: &str) -> Result<RegistryAuth> {
        // Extract registry from image
        let registry = extract_registry_from_image(image);

        debug!("Getting auth for registry: {} (image: {})", registry, image);

        // Check cache first
        if let Some(creds) = self.credentials_cache.get(&registry) {
            debug!("Using cached credentials for {}", registry);
            return Ok(RegistryAuth::Basic(
                creds.username.clone(),
                creds.password.clone(),
            ));
        }

        // Try to fetch credentials from Kubernetes
        match self.fetch_credentials_from_k8s(&registry, namespace).await {
            Ok(Some(creds)) => {
                debug!("Found credentials for {} in Kubernetes", registry);
                let auth = RegistryAuth::Basic(creds.username.clone(), creds.password.clone());

                // Cache credentials
                self.credentials_cache.insert(registry.clone(), creds);

                Ok(auth)
            }
            Ok(None) => {
                debug!("No credentials found for {}, using anonymous", registry);
                Ok(RegistryAuth::Anonymous)
            }
            Err(e) => {
                warn!("Error fetching credentials for {}: {}", registry, e);
                Ok(RegistryAuth::Anonymous)
            }
        }
    }

    /// Fetch credentials from Kubernetes secrets
    async fn fetch_credentials_from_k8s(
        &self,
        registry: &str,
        namespace: &str,
    ) -> Result<Option<RegistryCredentials>> {
        // Get the default service account
        let sa_api: Api<ServiceAccount> = Api::namespaced(self.client.clone(), namespace);
        let sa = match sa_api.get("default").await {
            Ok(sa) => sa,
            Err(e) => {
                debug!("Failed to get default service account: {}", e);
                return Ok(None);
            }
        };

        // Get imagePullSecrets from service account
        let secret_names = match &sa.image_pull_secrets {
            Some(secrets) => secrets,
            None => {
                debug!("No imagePullSecrets in service account");
                return Ok(None);
            }
        };

        // Try each secret
        let secrets_api: Api<Secret> = Api::namespaced(self.client.clone(), namespace);

        for secret_ref in secret_names {
            let secret_name = &secret_ref.name;
            debug!("Checking secret: {}", secret_name);

            match secrets_api.get(secret_name.as_str()).await {
                Ok(secret) => {
                    if let Some(creds) = self.parse_secret(&secret, registry)? {
                        return Ok(Some(creds));
                    }
                }
                Err(e) => {
                    warn!("Failed to get secret {}: {}", secret_name, e);
                }
            }
        }

        debug!("No matching credentials found in secrets");
        Ok(None)
    }

    /// Parse a Kubernetes secret for docker registry credentials
    fn parse_secret(&self, secret: &Secret, registry: &str) -> Result<Option<RegistryCredentials>> {
        let data = match &secret.data {
            Some(d) => d,
            None => return Ok(None),
        };

        // Check for .dockerconfigjson (kubernetes.io/dockerconfigjson)
        if let Some(dockerconfigjson) = data.get(".dockerconfigjson") {
            return self.parse_dockerconfigjson(&dockerconfigjson.0, registry);
        }

        // Check for .dockercfg (legacy format)
        if let Some(dockercfg) = data.get(".dockercfg") {
            return self.parse_dockercfg(&dockercfg.0, registry);
        }

        Ok(None)
    }

    /// Parse .dockerconfigjson format
    fn parse_dockerconfigjson(&self, data: &[u8], registry: &str) -> Result<Option<RegistryCredentials>> {
        let config: DockerConfig = serde_json::from_slice(data)
            .context("Failed to parse .dockerconfigjson")?;

        // Try exact registry match first
        if let Some(entry) = config.auths.get(registry) {
            return self.parse_auth_entry(entry, registry);
        }

        // Try with https:// prefix
        let https_registry = format!("https://{}", registry);
        if let Some(entry) = config.auths.get(&https_registry) {
            return self.parse_auth_entry(entry, registry);
        }

        // Try registry aliases
        for (key, entry) in &config.auths {
            if registry_matches(key, registry) {
                return self.parse_auth_entry(entry, registry);
            }
        }

        Ok(None)
    }

    /// Parse legacy .dockercfg format
    fn parse_dockercfg(&self, data: &[u8], registry: &str) -> Result<Option<RegistryCredentials>> {
        // Legacy format is similar but without the "auths" wrapper
        let auths: HashMap<String, DockerAuthEntry> = serde_json::from_slice(data)
            .context("Failed to parse .dockercfg")?;

        if let Some(entry) = auths.get(registry) {
            return self.parse_auth_entry(entry, registry);
        }

        Ok(None)
    }

    /// Parse an auth entry and extract credentials
    fn parse_auth_entry(&self, entry: &DockerAuthEntry, registry: &str) -> Result<Option<RegistryCredentials>> {
        // If username and password are directly provided
        if !entry.username.is_empty() && !entry.password.is_empty() {
            return Ok(Some(RegistryCredentials {
                registry: registry.to_string(),
                username: entry.username.clone(),
                password: entry.password.clone(),
            }));
        }

        // If auth token is provided (base64 encoded username:password)
        if !entry.auth.is_empty() {
            let decoded = BASE64_STANDARD
                .decode(entry.auth.as_bytes())
                .context("Failed to decode auth token")?;

            let auth_str = String::from_utf8(decoded)
                .context("Auth token is not valid UTF-8")?;

            if let Some((username, password)) = auth_str.split_once(':') {
                return Ok(Some(RegistryCredentials {
                    registry: registry.to_string(),
                    username: username.to_string(),
                    password: password.to_string(),
                }));
            }
        }

        Ok(None)
    }

    /// Clear the credentials cache (useful for testing or credential rotation)
    #[allow(dead_code)] // Available for future credential rotation feature
    pub fn clear_cache(&mut self) {
        self.credentials_cache.clear();
    }
}

/// Extract registry hostname from image reference
fn extract_registry_from_image(image: &str) -> String {
    // Parse image reference: [registry/]repository[:tag][@digest]

    // First split by '/' to find potential registry
    let parts: Vec<&str> = image.split('/').collect();

    if parts.len() > 1 {
        let first_part = parts[0];
        // If first part contains '.', ':', or is 'localhost', it's a registry
        if first_part.contains('.') || first_part.contains(':') || first_part == "localhost" {
            return first_part.to_string();
        }
    }

    // Default to Docker Hub
    "docker.io".to_string()
}

/// Check if a registry key matches the target registry
fn registry_matches(key: &str, target: &str) -> bool {
    // Remove https:// or http:// prefix
    let key_clean = key
        .trim_start_matches("https://")
        .trim_start_matches("http://");

    // Direct match
    if key_clean == target {
        return true;
    }

    // Docker Hub aliases
    if target == "docker.io" {
        return key_clean == "index.docker.io"
            || key_clean == "registry-1.docker.io"
            || key_clean == "index.docker.io/v1/"
            || key_clean == "registry-1.docker.io/v1/";
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_registry_from_image() {
        assert_eq!(extract_registry_from_image("nginx:1.21"), "docker.io");
        assert_eq!(extract_registry_from_image("nginx"), "docker.io");
        assert_eq!(extract_registry_from_image("library/nginx:1.21"), "docker.io");
        assert_eq!(extract_registry_from_image("gcr.io/project/image:tag"), "gcr.io");
        assert_eq!(extract_registry_from_image("registry.example.com/repo/image"), "registry.example.com");
        assert_eq!(extract_registry_from_image("localhost:5000/image"), "localhost:5000");
    }

    #[test]
    fn test_registry_matches() {
        assert!(registry_matches("docker.io", "docker.io"));
        assert!(registry_matches("https://docker.io", "docker.io"));
        assert!(registry_matches("index.docker.io", "docker.io"));
        assert!(registry_matches("registry-1.docker.io", "docker.io"));
        assert!(registry_matches("https://index.docker.io/v1/", "docker.io"));
        assert!(registry_matches("https://registry-1.docker.io/v1/", "docker.io"));
        assert!(registry_matches("gcr.io", "gcr.io"));
        assert!(registry_matches("https://gcr.io", "gcr.io"));

        assert!(!registry_matches("gcr.io", "docker.io"));
        assert!(!registry_matches("other.io", "docker.io"));
    }

    #[test]
    fn test_parse_auth_entry() {
        // This test would require a k8s client, skip for now
        // Testing the logic through integration tests would be better
    }
}
