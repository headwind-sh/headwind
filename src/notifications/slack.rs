use super::{NotificationPayload, Notifier, SlackConfig};
use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde_json::json;
use std::time::Duration;
use tracing::debug;

pub struct SlackNotifier {
    config: SlackConfig,
    client: Client,
}

impl SlackNotifier {
    pub fn new(config: SlackConfig) -> Result<Self> {
        if !config.enabled {
            return Err(anyhow!("Slack notifier is disabled"));
        }

        if config.webhook_url.is_none() {
            return Err(anyhow!("Slack webhook URL is required"));
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { config, client })
    }

    /// Build Slack message in Block Kit format
    fn build_message(&self, payload: &NotificationPayload) -> serde_json::Value {
        let emoji = payload.event.emoji();
        let color = payload.event.color();
        let title = payload.title();

        let mut fields = Vec::new();

        // Add policy field if present
        if let Some(policy) = &payload.policy {
            fields.push(json!({
                "type": "mrkdwn",
                "text": format!("*Policy:*\n{}", policy)
            }));
        }

        // Add approval info if present
        if let Some(approver) = &payload.approved_by {
            fields.push(json!({
                "type": "mrkdwn",
                "text": format!("*Approved by:*\n{}", approver)
            }));
        }

        // Add rejection reason if present
        if let Some(reason) = &payload.rejection_reason {
            fields.push(json!({
                "type": "mrkdwn",
                "text": format!("*Rejection reason:*\n{}", reason)
            }));
        }

        // Add error message if present
        if let Some(error) = &payload.error_message {
            fields.push(json!({
                "type": "mrkdwn",
                "text": format!("*Error:*\n```{}```", error)
            }));
        }

        // Format "HelmRelease" as "Helm Release" for better readability
        let resource_kind = payload
            .deployment
            .resource_kind
            .as_deref()
            .unwrap_or("Deployment");
        let formatted_kind = if resource_kind == "HelmRelease" {
            "Helm Release"
        } else {
            resource_kind
        };

        let mut blocks = vec![
            json!({
                "type": "header",
                "text": {
                    "type": "plain_text",
                    "text": format!("{} {}", emoji, title),
                    "emoji": true
                }
            }),
            json!({
                "type": "section",
                "fields": [
                    {
                        "type": "mrkdwn",
                        "text": format!("*Namespace:*\n{}", payload.deployment.namespace)
                    },
                    {
                        "type": "mrkdwn",
                        "text": format!("*{}:*\n{}", formatted_kind, payload.deployment.name)
                    },
                    {
                        "type": "mrkdwn",
                        "text": format!("*Current Image:*\n`{}`", payload.deployment.current_image)
                    },
                    {
                        "type": "mrkdwn",
                        "text": format!("*New Image:*\n`{}`", payload.deployment.new_image)
                    }
                ]
            }),
        ];

        // Add additional fields if present
        if !fields.is_empty() {
            blocks.push(json!({
                "type": "section",
                "fields": fields
            }));
        }

        // Add approval button if approval URL is present
        if let Some(approval_url) = &payload.approval_url {
            blocks.push(json!({
                "type": "actions",
                "elements": [
                    {
                        "type": "button",
                        "text": {
                            "type": "plain_text",
                            "text": "Approve",
                            "emoji": true
                        },
                        "style": "primary",
                        "url": approval_url,
                        "action_id": "approve_update"
                    },
                    {
                        "type": "button",
                        "text": {
                            "type": "plain_text",
                            "text": "View Details",
                            "emoji": true
                        },
                        "url": approval_url.replace("/approve", ""),
                        "action_id": "view_details"
                    }
                ]
            }));
        }

        // Add context with timestamp
        blocks.push(json!({
            "type": "context",
            "elements": [
                {
                    "type": "mrkdwn",
                    "text": format!("<!date^{}^{{date_short_pretty}} at {{time}}|{}>",
                        payload.timestamp.timestamp(),
                        payload.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
                    )
                }
            ]
        }));

        let mut message = json!({
            "blocks": blocks,
            "attachments": [{
                "color": color,
                "fallback": title.clone()
            }]
        });

        // Note: For Incoming Webhooks, the channel is pre-configured in the webhook URL
        // and cannot be overridden in the payload. The channel config is ignored for
        // Incoming Webhooks but may be used in the future for other Slack integration methods.

        // Add username if configured
        if let Some(username) = &self.config.username {
            message["username"] = json!(username);
        } else {
            message["username"] = json!("Headwind");
        }

        // Add icon if configured
        if let Some(icon) = &self.config.icon_emoji {
            message["icon_emoji"] = json!(icon);
        } else {
            message["icon_emoji"] = json!(":robot_face:");
        }

        message
    }
}

#[async_trait::async_trait]
impl Notifier for SlackNotifier {
    async fn send(&self, payload: &NotificationPayload) -> Result<()> {
        let webhook_url = self
            .config
            .webhook_url
            .as_ref()
            .ok_or_else(|| anyhow!("Slack webhook URL not configured"))?;

        let message = self.build_message(payload);

        debug!("Sending Slack notification to: {}", webhook_url);
        debug!("Webhook URL length: {} chars", webhook_url.len());
        debug!("Webhook URL bytes: {:?}", webhook_url.as_bytes());
        debug!(
            "Message payload: {}",
            serde_json::to_string_pretty(&message).unwrap_or_default()
        );

        // Try to serialize the message to catch any serialization issues early
        let json_str =
            serde_json::to_string(&message).context("Failed to serialize message to JSON")?;
        debug!("Serialized JSON length: {} bytes", json_str.len());

        let response = self
            .client
            .post(webhook_url)
            .header("Content-Type", "application/json")
            .body(json_str)
            .send()
            .await
            .map_err(|e| {
                let error_msg = format!("Failed to send Slack notification: {} | is_timeout: {} | is_connect: {} | is_body: {} | is_decode: {} | is_request: {}",
                    e,
                    e.is_timeout(),
                    e.is_connect(),
                    e.is_body(),
                    e.is_decode(),
                    e.is_request()
                );
                anyhow!(error_msg)
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response".to_string());
            return Err(anyhow!("Slack API returned error {}: {}", status, body));
        }

        debug!("Slack notification sent successfully");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Slack"
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
    fn test_slack_notifier_creation() {
        let config = SlackConfig {
            enabled: true,
            webhook_url: Some("https://hooks.slack.com/services/TEST".to_string()),
            channel: Some("#deployments".to_string()),
            username: Some("Headwind Bot".to_string()),
            icon_emoji: Some(":rocket:".to_string()),
        };

        let notifier = SlackNotifier::new(config);
        assert!(notifier.is_ok());
        assert!(notifier.unwrap().is_enabled());
    }

    #[test]
    fn test_slack_notifier_disabled() {
        let config = SlackConfig {
            enabled: false,
            webhook_url: Some("https://hooks.slack.com/services/TEST".to_string()),
            channel: None,
            username: None,
            icon_emoji: None,
        };

        let notifier = SlackNotifier::new(config);
        assert!(notifier.is_err());
    }

    #[test]
    fn test_slack_notifier_missing_webhook() {
        let config = SlackConfig {
            enabled: true,
            webhook_url: None,
            channel: None,
            username: None,
            icon_emoji: None,
        };

        let notifier = SlackNotifier::new(config);
        assert!(notifier.is_err());
    }

    #[test]
    fn test_build_message() {
        let config = SlackConfig {
            enabled: true,
            webhook_url: Some("https://hooks.slack.com/services/TEST".to_string()),
            channel: Some("#deployments".to_string()),
            username: Some("Headwind".to_string()),
            icon_emoji: Some(":rocket:".to_string()),
        };

        let notifier = SlackNotifier::new(config).unwrap();

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

        let message = notifier.build_message(&payload);

        assert_eq!(message["username"], "Headwind");
        assert_eq!(message["icon_emoji"], ":rocket:");
        // Note: channel field is not included in the message for Incoming Webhooks
        // as it's pre-configured in the webhook URL and cannot be overridden
        assert!(message["blocks"].is_array());
        assert!(!message["blocks"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_build_message_with_error() {
        let config = SlackConfig {
            enabled: true,
            webhook_url: Some("https://hooks.slack.com/services/TEST".to_string()),
            channel: None,
            username: None,
            icon_emoji: None,
        };

        let notifier = SlackNotifier::new(config).unwrap();

        let deployment = DeploymentInfo {
            name: "nginx".to_string(),
            namespace: "production".to_string(),
            current_image: "nginx:1.25.0".to_string(),
            new_image: "nginx:1.26.0".to_string(),
            container: None,
            resource_kind: None,
        };

        let payload = NotificationPayload::new(NotificationEvent::UpdateFailed, deployment)
            .with_error("Failed to pull image");

        let message = notifier.build_message(&payload);

        let message_str = serde_json::to_string(&message).unwrap();
        assert!(message_str.contains("Failed to pull image"));
    }
}
