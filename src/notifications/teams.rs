use super::{NotificationPayload, Notifier, TeamsConfig};
use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde_json::json;
use std::time::Duration;
use tracing::debug;

pub struct TeamsNotifier {
    config: TeamsConfig,
    client: Client,
}

impl TeamsNotifier {
    pub fn new(config: TeamsConfig) -> Result<Self> {
        if !config.enabled {
            return Err(anyhow!("Teams notifier is disabled"));
        }

        if config.webhook_url.is_none() {
            return Err(anyhow!("Teams webhook URL is required"));
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { config, client })
    }

    /// Build Microsoft Teams Adaptive Card
    fn build_adaptive_card(&self, payload: &NotificationPayload) -> serde_json::Value {
        let emoji = payload.event.emoji();
        let color = payload.event.color();
        let title = payload.title();

        let mut facts = vec![
            json!({
                "title": "Namespace",
                "value": payload.deployment.namespace
            }),
            json!({
                "title": "Deployment",
                "value": payload.deployment.name
            }),
            json!({
                "title": "Current Image",
                "value": payload.deployment.current_image
            }),
            json!({
                "title": "New Image",
                "value": payload.deployment.new_image
            }),
        ];

        // Add policy if present
        if let Some(policy) = &payload.policy {
            facts.push(json!({
                "title": "Policy",
                "value": policy
            }));
        }

        // Add approver if present
        if let Some(approver) = &payload.approved_by {
            facts.push(json!({
                "title": "Approved By",
                "value": approver
            }));
        }

        // Add rejection reason if present
        if let Some(reason) = &payload.rejection_reason {
            facts.push(json!({
                "title": "Rejection Reason",
                "value": reason
            }));
        }

        // Add error if present
        if let Some(error) = &payload.error_message {
            facts.push(json!({
                "title": "Error",
                "value": error
            }));
        }

        let sections = vec![json!({
            "activityTitle": format!("{} {}", emoji, title),
            "activitySubtitle": format!("Event: {}", payload.event.as_str()),
            "activityImage": "https://raw.githubusercontent.com/kubernetes/kubernetes/master/logo/logo.png",
            "facts": facts,
            "markdown": true
        })];

        // Build actions array for buttons
        let mut potential_actions = Vec::new();

        // Add "View in Dashboard" button if UI URL is present
        if let Some(ui_url) = &payload.ui_url {
            potential_actions.push(json!({
                "@type": "OpenUri",
                "name": "View in Dashboard",
                "targets": [{
                    "os": "default",
                    "uri": ui_url
                }]
            }));
        }

        // Add "Approve" button if approval URL is present
        if let Some(approval_url) = &payload.approval_url {
            potential_actions.push(json!({
                "@type": "OpenUri",
                "name": "Approve Update",
                "targets": [{
                    "os": "default",
                    "uri": approval_url
                }]
            }));
        }

        let mut card = json!({
            "@type": "MessageCard",
            "@context": "https://schema.org/extensions",
            "summary": title,
            "themeColor": color.trim_start_matches('#'),
            "sections": sections
        });

        if !potential_actions.is_empty() {
            card["potentialAction"] = json!(potential_actions);
        }

        card
    }
}

#[async_trait::async_trait]
impl Notifier for TeamsNotifier {
    async fn send(&self, payload: &NotificationPayload) -> Result<()> {
        let webhook_url = self
            .config
            .webhook_url
            .as_ref()
            .ok_or_else(|| anyhow!("Teams webhook URL not configured"))?;

        let card = self.build_adaptive_card(payload);

        let response = self
            .client
            .post(webhook_url)
            .json(&card)
            .send()
            .await
            .context("Failed to send Teams notification")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response".to_string());
            return Err(anyhow!("Teams API returned error {}: {}", status, body));
        }

        debug!("Teams notification sent successfully");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Microsoft Teams"
    }

    fn is_enabled(&self) -> bool {
        self.config.enabled && self.config.webhook_url.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifications::{DeploymentInfo, NotificationEvent};

    #[test]
    fn test_teams_notifier_creation() {
        let config = TeamsConfig {
            enabled: true,
            webhook_url: Some("https://outlook.office.com/webhook/test".to_string()),
        };

        let notifier = TeamsNotifier::new(config);
        assert!(notifier.is_ok());
        assert!(notifier.unwrap().is_enabled());
    }

    #[test]
    fn test_teams_notifier_disabled() {
        let config = TeamsConfig {
            enabled: false,
            webhook_url: Some("https://outlook.office.com/webhook/test".to_string()),
        };

        let notifier = TeamsNotifier::new(config);
        assert!(notifier.is_err());
    }

    #[test]
    fn test_teams_notifier_missing_webhook() {
        let config = TeamsConfig {
            enabled: true,
            webhook_url: None,
        };

        let notifier = TeamsNotifier::new(config);
        assert!(notifier.is_err());
    }

    #[test]
    fn test_build_adaptive_card() {
        let config = TeamsConfig {
            enabled: true,
            webhook_url: Some("https://outlook.office.com/webhook/test".to_string()),
        };

        let notifier = TeamsNotifier::new(config).unwrap();

        let deployment = DeploymentInfo {
            name: "nginx".to_string(),
            namespace: "production".to_string(),
            current_image: "nginx:1.25.0".to_string(),
            new_image: "nginx:1.26.0".to_string(),
            container: None,
            resource_kind: None,
        };

        let payload = NotificationPayload::new(NotificationEvent::UpdateRequestCreated, deployment)
            .with_policy("minor")
            .with_requires_approval(true)
            .with_approval_url("https://headwind.example.com/approve");

        let card = notifier.build_adaptive_card(&payload);

        assert_eq!(card["@type"], "MessageCard");
        assert!(card["sections"].is_array());
        assert!(card["potentialAction"].is_array());
        assert!(card["themeColor"].is_string());
    }

    #[test]
    fn test_build_adaptive_card_with_error() {
        let config = TeamsConfig {
            enabled: true,
            webhook_url: Some("https://outlook.office.com/webhook/test".to_string()),
        };

        let notifier = TeamsNotifier::new(config).unwrap();

        let deployment = DeploymentInfo {
            name: "nginx".to_string(),
            namespace: "production".to_string(),
            current_image: "nginx:1.25.0".to_string(),
            new_image: "nginx:1.26.0".to_string(),
            container: None,
            resource_kind: None,
        };

        let payload = NotificationPayload::new(NotificationEvent::UpdateFailed, deployment)
            .with_error("Failed to pull image: timeout");

        let card = notifier.build_adaptive_card(&payload);

        let card_str = serde_json::to_string(&card).unwrap();
        assert!(card_str.contains("Failed to pull image"));
    }

    #[test]
    fn test_build_card_approved() {
        let config = TeamsConfig {
            enabled: true,
            webhook_url: Some("https://outlook.office.com/webhook/test".to_string()),
        };

        let notifier = TeamsNotifier::new(config).unwrap();

        let deployment = DeploymentInfo {
            name: "nginx".to_string(),
            namespace: "production".to_string(),
            current_image: "nginx:1.25.0".to_string(),
            new_image: "nginx:1.26.0".to_string(),
            container: None,
            resource_kind: None,
        };

        let payload = NotificationPayload::new(NotificationEvent::UpdateApproved, deployment)
            .with_approved_by("admin@example.com");

        let card = notifier.build_adaptive_card(&payload);

        let card_str = serde_json::to_string(&card).unwrap();
        assert!(card_str.contains("admin@example.com"));
        assert!(card_str.contains("Approved By"));
    }
}
