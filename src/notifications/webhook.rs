use super::{NotificationPayload, Notifier, WebhookConfig};
use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tracing::{debug, warn};

pub struct WebhookNotifier {
    config: WebhookConfig,
    client: Client,
}

impl WebhookNotifier {
    pub fn new(config: WebhookConfig) -> Result<Self> {
        if !config.enabled {
            return Err(anyhow!("Webhook notifier is disabled"));
        }

        if config.url.is_none() {
            return Err(anyhow!("Webhook URL is required"));
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { config, client })
    }

    /// Generate HMAC signature for the payload
    fn generate_signature(&self, payload: &str) -> Option<String> {
        self.config.secret.as_ref().map(|secret| {
            let mut hasher = Sha256::new();
            hasher.update(secret.as_bytes());
            hasher.update(payload.as_bytes());
            let result = hasher.finalize();
            format!("sha256={}", hex::encode(result))
        })
    }

    /// Send webhook with retry logic
    async fn send_with_retry(&self, payload: &NotificationPayload) -> Result<()> {
        let url = self
            .config
            .url
            .as_ref()
            .ok_or_else(|| anyhow!("Webhook URL not configured"))?;

        let body = serde_json::to_string(payload).context("Failed to serialize payload")?;

        let mut last_error = None;
        let mut backoff_ms = 1000u64; // Start with 1 second

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                debug!(
                    "Retrying webhook notification (attempt {}/{})",
                    attempt, self.config.max_retries
                );
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms *= 2; // Exponential backoff
            }

            let mut request = self
                .client
                .post(url)
                .header("Content-Type", "application/json");

            // Add signature if secret is configured
            if let Some(signature) = self.generate_signature(&body) {
                request = request.header("X-Headwind-Signature", signature);
            }

            match request.body(body.clone()).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        debug!("Webhook notification sent successfully to {}", url);
                        return Ok(());
                    } else {
                        last_error = Some(anyhow!(
                            "Webhook returned non-success status: {}",
                            response.status()
                        ));
                        warn!(
                            "Webhook returned status {}: {}",
                            response.status(),
                            response
                                .text()
                                .await
                                .unwrap_or_else(|_| "Unable to read response".to_string())
                        );
                    }
                },
                Err(e) => {
                    last_error = Some(anyhow!("HTTP request failed: {}", e));
                    warn!("Failed to send webhook notification: {}", e);
                },
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("Webhook notification failed after all retries")))
    }
}

#[async_trait::async_trait]
impl Notifier for WebhookNotifier {
    async fn send(&self, payload: &NotificationPayload) -> Result<()> {
        self.send_with_retry(payload).await
    }

    fn name(&self) -> &'static str {
        "Webhook"
    }

    fn is_enabled(&self) -> bool {
        self.config.enabled && self.config.url.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_notifier_creation() {
        let config = WebhookConfig {
            enabled: true,
            url: Some("https://example.com/webhook".to_string()),
            secret: Some("test-secret".to_string()),
            timeout_seconds: 10,
            max_retries: 3,
        };

        let notifier = WebhookNotifier::new(config);
        assert!(notifier.is_ok());
        assert!(notifier.unwrap().is_enabled());
    }

    #[test]
    fn test_webhook_notifier_disabled() {
        let config = WebhookConfig {
            enabled: false,
            url: Some("https://example.com/webhook".to_string()),
            secret: None,
            timeout_seconds: 10,
            max_retries: 3,
        };

        let notifier = WebhookNotifier::new(config);
        assert!(notifier.is_err());
    }

    #[test]
    fn test_webhook_notifier_missing_url() {
        let config = WebhookConfig {
            enabled: true,
            url: None,
            secret: None,
            timeout_seconds: 10,
            max_retries: 3,
        };

        let notifier = WebhookNotifier::new(config);
        assert!(notifier.is_err());
    }

    #[test]
    fn test_signature_generation() {
        let config = WebhookConfig {
            enabled: true,
            url: Some("https://example.com/webhook".to_string()),
            secret: Some("test-secret".to_string()),
            timeout_seconds: 10,
            max_retries: 3,
        };

        let notifier = WebhookNotifier::new(config).unwrap();
        let signature = notifier.generate_signature("test payload");
        assert!(signature.is_some());
        assert!(signature.unwrap().starts_with("sha256="));
    }

    #[test]
    fn test_signature_without_secret() {
        let config = WebhookConfig {
            enabled: true,
            url: Some("https://example.com/webhook".to_string()),
            secret: None,
            timeout_seconds: 10,
            max_retries: 3,
        };

        let notifier = WebhookNotifier::new(config).unwrap();
        let signature = notifier.generate_signature("test payload");
        assert!(signature.is_none());
    }
}
