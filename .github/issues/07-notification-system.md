# Issue #7: Add Notification System

**Labels**: `enhancement`, `medium-priority`

## Description

Send notifications to external systems (Slack, Teams, email, webhooks) when updates are pending, approved, or failed.

## What Needs to Be Done

### 1. Define Notification Config

```yaml
# ConfigMap or environment variables
apiVersion: v1
kind: ConfigMap
metadata:
  name: headwind-config
  namespace: headwind-system
data:
  notifications.yaml: |
    slack:
      enabled: true
      webhook_url: https://hooks.slack.com/services/...
      channel: "#deployments"
      events:
        - update_pending
        - update_approved
        - update_failed

    teams:
      enabled: false
      webhook_url: https://...

    webhook:
      enabled: true
      url: https://my-webhook-receiver/headwind
      events:
        - update_applied
```

### 2. Implement Notification Providers

```rust
// src/notifications/mod.rs
#[async_trait]
pub trait NotificationProvider: Send + Sync {
    async fn send(&self, event: &NotificationEvent) -> Result<()>;
}

pub struct SlackNotifier {
    webhook_url: String,
    channel: String,
}

impl NotificationProvider for SlackNotifier {
    async fn send(&self, event: &NotificationEvent) -> Result<()> {
        let payload = json!({
            "channel": self.channel,
            "text": format!("🚀 Headwind Update: {}", event.title),
            "attachments": [{
                "color": event.color(),
                "fields": [
                    {"title": "Resource", "value": event.resource_name, "short": true},
                    {"title": "Namespace", "value": event.namespace, "short": true},
                    {"title": "Image", "value": event.new_image, "short": false},
                ]
            }]
        });

        reqwest::Client::new()
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await?;

        Ok(())
    }
}
```

### 3. Define Notification Events

```rust
#[derive(Clone, Debug)]
pub enum NotificationEvent {
    UpdatePending(UpdateRequest),
    UpdateApproved { request: UpdateRequest, approver: String },
    UpdateRejected { request: UpdateRequest, approver: String, reason: String },
    UpdateApplied(UpdateRequest),
    UpdateFailed { request: UpdateRequest, error: String },
}

impl NotificationEvent {
    fn color(&self) -> &str {
        match self {
            Self::UpdatePending(_) => "#FFA500", // Orange
            Self::UpdateApproved { .. } => "#00FF00", // Green
            Self::UpdateRejected { .. } => "#FF0000", // Red
            Self::UpdateApplied(_) => "#0000FF", // Blue
            Self::UpdateFailed { .. } => "#FF0000", // Red
        }
    }

    fn emoji(&self) -> &str {
        match self {
            Self::UpdatePending(_) => "⏳",
            Self::UpdateApproved { .. } => "✅",
            Self::UpdateRejected { .. } => "❌",
            Self::UpdateApplied(_) => "🚀",
            Self::UpdateFailed { .. } => "💥",
        }
    }
}
```

### 4. Integrate into Workflow

```rust
// In approval/mod.rs
async fn approve_update(...) -> impl IntoResponse {
    // ... existing logic ...

    // Send notification
    let event = NotificationEvent::UpdateApproved {
        request: update.clone(),
        approver: approval.approver.unwrap_or_default(),
    };
    notifier.send(&event).await.ok();

    // ... rest of logic ...
}
```

### 5. Add Slack Message Examples

```json
{
  "text": "⏳ Headwind Update Pending Approval",
  "blocks": [
    {
      "type": "section",
      "text": {
        "type": "mrkdwn",
        "text": "*New update available for approval*"
      }
    },
    {
      "type": "section",
      "fields": [
        {"type": "mrkdwn", "text": "*Resource:*\nnginx-example"},
        {"type": "mrkdwn", "text": "*Namespace:*\nproduction"},
        {"type": "mrkdwn", "text": "*Current Image:*\nnginx:1.25.0"},
        {"type": "mrkdwn", "text": "*New Image:*\nnginx:1.26.0"},
        {"type": "mrkdwn", "text": "*Policy:*\nminor"}
      ]
    },
    {
      "type": "actions",
      "elements": [
        {
          "type": "button",
          "text": {"type": "plain_text", "text": "Approve"},
          "style": "primary",
          "url": "https://headwind.example.com/updates/abc123"
        },
        {
          "type": "button",
          "text": {"type": "plain_text", "text": "View Details"},
          "url": "https://headwind.example.com/updates/abc123"
        }
      ]
    }
  ]
}
```

### 6. Configuration Loading

```rust
// src/config.rs
#[derive(Deserialize)]
pub struct NotificationConfig {
    pub slack: Option<SlackConfig>,
    pub teams: Option<TeamsConfig>,
    pub webhook: Option<WebhookConfig>,
}

pub fn load_config() -> Result<NotificationConfig> {
    // Load from ConfigMap or environment
    let config_path = env::var("HEADWIND_CONFIG_PATH")
        .unwrap_or("/etc/headwind/notifications.yaml".to_string());

    let config_str = std::fs::read_to_string(config_path)?;
    let config = serde_yaml::from_str(&config_str)?;
    Ok(config)
}
```

## Notification Types

### Slack
- Rich formatting with blocks
- Approval buttons (requires Slack app)
- Thread-based updates
- Emoji and color coding

### Microsoft Teams
- Adaptive cards
- Action buttons
- Channel webhooks

### Generic Webhook
- JSON payload with event data
- Custom headers support
- Retry logic

### Email (future)
- HTML templates
- SMTP configuration
- Attachment support

## Acceptance Criteria

- [ ] Slack notifications working
- [ ] Teams notifications working
- [ ] Generic webhook notifications working
- [ ] Configuration via ConfigMap
- [ ] Environment variable overrides
- [ ] Notification on all event types
- [ ] Rich formatting (colors, emojis, fields)
- [ ] Error handling and retries
- [ ] Metrics for notification success/failure
- [ ] Documentation for setup
- [ ] Example configurations

## Configuration Example

```yaml
# deploy/k8s/configmap.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: headwind-notifications
  namespace: headwind-system
data:
  slack_webhook_url: "https://hooks.slack.com/services/..."
  slack_channel: "#kubernetes-updates"
  teams_webhook_url: ""
  webhook_url: "https://example.com/webhook"
```

```yaml
# deploy/k8s/deployment.yaml
env:
- name: SLACK_WEBHOOK_URL
  valueFrom:
    configMapKeyRef:
      name: headwind-notifications
      key: slack_webhook_url
```

## Testing

```bash
# Send test notification
curl -X POST http://localhost:8081/api/v1/test-notification \
  -H "Content-Type: application/json" \
  -d '{"provider":"slack","event":"update_pending"}'
```

## Related Issues

- Related to: #2, #6 (Approval workflow)

## Estimated Effort

Medium-Large (8-12 hours)
