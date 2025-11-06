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

- [x] Slack notifications working (tested)
- [ ] Teams notifications working (needs testing - no Teams account available)
- [x] Generic webhook notifications working (tested)
- [x] Configuration via environment variables
- [x] Configuration via ConfigMap (implemented with fallback to environment variables)
- [x] Notification on all event types
- [x] Rich formatting (colors, emojis, fields)
- [x] Error handling and retries
- [x] Metrics for notification success/failure
- [ ] Documentation for setup (in progress)
- [x] Example configurations

## Configuration Examples

### Option 1: ConfigMap (Recommended for Kubernetes)

```yaml
# deploy/k8s/notification-configmap.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: headwind-notifications
  namespace: headwind-system
data:
  notifications.yaml: |
    slack:
      enabled: true
      webhook_url: "https://hooks.slack.com/services/YOUR/WEBHOOK/URL"
      channel: "#kubernetes-updates"
      username: "Headwind Bot"
      icon_emoji: ":robot_face:"

    teams:
      enabled: false
      webhook_url: "https://outlook.office.com/webhook/YOUR/WEBHOOK/URL"

    webhook:
      enabled: true
      url: "https://your-webhook-receiver.example.com/headwind"
      secret: "your-webhook-secret"
      timeout_seconds: 10
      max_retries: 3
```

To use ConfigMap configuration, call `NotificationConfig::from_configmap()` in your code:

```rust
// Load from ConfigMap with fallback to environment variables
let client = kube::Client::try_default().await?;
let config = NotificationConfig::from_configmap(
    client,
    "headwind-notifications",
    "headwind-system"
).await?;
```

### Option 2: Environment Variables

```yaml
# deploy/k8s/deployment.yaml
env:
- name: SLACK_ENABLED
  value: "true"
- name: SLACK_WEBHOOK_URL
  value: "https://hooks.slack.com/services/..."
- name: SLACK_CHANNEL
  value: "#kubernetes-updates"
- name: WEBHOOK_ENABLED
  value: "true"
- name: WEBHOOK_URL
  value: "https://example.com/webhook"
- name: WEBHOOK_SECRET
  value: "your-secret"
```

To use environment variable configuration:

```rust
let config = NotificationConfig::from_env();
```

## Testing

```bash
# Send test notification
curl -X POST http://localhost:8081/api/v1/test-notification \
  -H "Content-Type: application/json" \
  -d '{"provider":"slack","event":"update_pending"}'
```

## Webhook External Access Configuration

### Overview

When deploying Headwind into Kubernetes, you may want to receive webhook notifications from external container registries (Docker Hub, Harbor, GitLab Container Registry, etc.) when new images are pushed. To enable this, you need to expose Headwind's webhook endpoint externally using either a Kubernetes Ingress or a LoadBalancer service.

Headwind's webhook server runs on port 8080 and provides two endpoints:
- `/webhook/dockerhub` - For Docker Hub webhook notifications
- `/webhook/registry` - For generic container registry webhooks

### Architecture

Headwind uses a three-service architecture for security and separation of concerns:

```yaml
# Service Architecture
headwind-webhook:8080  # Webhook server (external access needed)
headwind-api:8081      # API server (internal/optional external)
headwind-metrics:9090  # Prometheus metrics (internal only)
```

### Option 1: NGINX Ingress (Recommended)

#### Basic Ingress Configuration

```yaml
# deploy/k8s/webhook-ingress.yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: headwind-webhook
  namespace: headwind-system
  annotations:
    nginx.ingress.kubernetes.io/rewrite-target: /
spec:
  ingressClassName: nginx
  rules:
  - host: headwind.example.com
    http:
      paths:
      - path: /webhook
        pathType: Prefix
        backend:
          service:
            name: headwind-webhook
            port:
              number: 8080
```

Apply the Ingress:

```bash
kubectl apply -f deploy/k8s/webhook-ingress.yaml
```

Your webhook URLs will be:
- `https://headwind.example.com/webhook/dockerhub`
- `https://headwind.example.com/webhook/registry`

#### Ingress with TLS/HTTPS (Production Recommended)

```yaml
# deploy/k8s/webhook-ingress-tls.yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: headwind-webhook
  namespace: headwind-system
  annotations:
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
    nginx.ingress.kubernetes.io/ssl-redirect: "true"
    nginx.ingress.kubernetes.io/force-ssl-redirect: "true"
spec:
  ingressClassName: nginx
  tls:
  - hosts:
    - headwind.example.com
    secretName: headwind-webhook-tls
  rules:
  - host: headwind.example.com
    http:
      paths:
      - path: /webhook
        pathType: Prefix
        backend:
          service:
            name: headwind-webhook
            port:
              number: 8080
```

Prerequisites for TLS:
1. Install cert-manager: `kubectl apply -f https://github.com/cert-manager/cert-manager/releases/download/v1.13.0/cert-manager.yaml`
2. Create ClusterIssuer:

```yaml
# deploy/k8s/cluster-issuer.yaml
apiVersion: cert-manager.io/v1
kind: ClusterIssuer
metadata:
  name: letsencrypt-prod
spec:
  acme:
    server: https://acme-v02.api.letsencrypt.org/directory
    email: your-email@example.com
    privateKeySecretRef:
      name: letsencrypt-prod
    solvers:
    - http01:
        ingress:
          class: nginx
```

#### Ingress with Rate Limiting

Protect your webhook endpoint from abuse with rate limiting:

```yaml
# deploy/k8s/webhook-ingress-ratelimit.yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: headwind-webhook
  namespace: headwind-system
  annotations:
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
    nginx.ingress.kubernetes.io/ssl-redirect: "true"
    # Rate limiting: 10 requests per minute per IP
    nginx.ingress.kubernetes.io/limit-rps: "10"
    nginx.ingress.kubernetes.io/limit-burst-multiplier: "2"
    # Connection limiting
    nginx.ingress.kubernetes.io/limit-connections: "5"
spec:
  ingressClassName: nginx
  tls:
  - hosts:
    - headwind.example.com
    secretName: headwind-webhook-tls
  rules:
  - host: headwind.example.com
    http:
      paths:
      - path: /webhook
        pathType: Prefix
        backend:
          service:
            name: headwind-webhook
            port:
              number: 8080
```

### Option 2: LoadBalancer Service

For clusters that support LoadBalancer services (AWS, GCP, Azure):

```yaml
# deploy/k8s/webhook-loadbalancer.yaml
apiVersion: v1
kind: Service
metadata:
  name: headwind-webhook-external
  namespace: headwind-system
  annotations:
    # AWS specific
    service.beta.kubernetes.io/aws-load-balancer-type: "nlb"
    service.beta.kubernetes.io/aws-load-balancer-ssl-cert: "arn:aws:acm:region:account:certificate/cert-id"
    service.beta.kubernetes.io/aws-load-balancer-ssl-ports: "443"
    # GCP specific (uncomment if using GCP)
    # cloud.google.com/load-balancer-type: "External"
spec:
  type: LoadBalancer
  ports:
  - name: https
    port: 443
    targetPort: 8080
    protocol: TCP
  selector:
    app: headwind
```

### Container Registry Configuration Examples

#### Docker Hub

1. Go to Docker Hub → Repository → Webhooks
2. Add webhook URL: `https://headwind.example.com/webhook/dockerhub`
3. Configure webhook to trigger on "Repository Push"

Example Docker Hub webhook payload:

```json
{
  "push_data": {
    "tag": "v1.2.3",
    "pushed_at": 1622547600
  },
  "repository": {
    "repo_name": "myorg/myapp",
    "namespace": "myorg",
    "name": "myapp"
  }
}
```

#### Harbor Registry

Harbor supports generic webhook format:

1. Go to Harbor → Project → Webhooks
2. Add webhook: `https://headwind.example.com/webhook/registry`
3. Event Type: "Artifact pushed"
4. Add custom header (if using webhook secret):
   - `X-Headwind-Signature: <your-secret>`

Example Harbor configuration:

```yaml
# Harbor webhook configuration
webhook:
  url: https://headwind.example.com/webhook/registry
  event_types:
    - PUSH_ARTIFACT
  headers:
    X-Headwind-Signature: "your-webhook-secret"
```

#### GitLab Container Registry

1. Go to GitLab Project → Settings → Webhooks
2. URL: `https://headwind.example.com/webhook/registry`
3. Trigger: "Push events" (Container Registry)
4. Secret Token: `your-webhook-secret`

#### GitHub Container Registry (GHCR)

GitHub Actions workflow to trigger webhook on package publish:

```yaml
# .github/workflows/notify-headwind.yaml
name: Notify Headwind on Package Publish

on:
  registry_package:
    types: [published]

jobs:
  notify:
    runs-on: ubuntu-latest
    steps:
    - name: Trigger Headwind Webhook
      run: |
        curl -X POST https://headwind.example.com/webhook/registry \
          -H "Content-Type: application/json" \
          -H "X-Headwind-Signature: ${{ secrets.HEADWIND_WEBHOOK_SECRET }}" \
          -d '{
            "repository": "${{ github.repository }}",
            "tag": "${{ github.event.registry_package.package_version.version }}",
            "image": "ghcr.io/${{ github.repository }}:${{ github.event.registry_package.package_version.version }}"
          }'
```

#### Amazon ECR

Use EventBridge and Lambda to forward ECR events to Headwind:

```yaml
# CloudFormation template excerpt
Resources:
  ECREventRule:
    Type: AWS::Events::Rule
    Properties:
      EventPattern:
        source:
          - aws.ecr
        detail-type:
          - ECR Image Action
        detail:
          action-type:
            - PUSH
          result:
            - SUCCESS
      Targets:
        - Arn: !GetAtt HeadwindWebhookFunction.Arn
          Id: HeadwindWebhook

  HeadwindWebhookFunction:
    Type: AWS::Lambda::Function
    Properties:
      Runtime: python3.9
      Handler: index.handler
      Code:
        ZipFile: |
          import json
          import urllib3
          http = urllib3.PoolManager()

          def handler(event, context):
              detail = event['detail']
              payload = {
                  'repository': detail['repository-name'],
                  'tag': detail['image-tag'],
                  'image': f"{detail['repository-name']}:{detail['image-tag']}"
              }

              response = http.request(
                  'POST',
                  'https://headwind.example.com/webhook/registry',
                  body=json.dumps(payload),
                  headers={
                      'Content-Type': 'application/json',
                      'X-Headwind-Signature': 'your-webhook-secret'
                  }
              )
              return {'statusCode': 200}
```

### Security Best Practices

#### 1. Network Policies

Restrict webhook service access:

```yaml
# deploy/k8s/webhook-network-policy.yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: headwind-webhook-policy
  namespace: headwind-system
spec:
  podSelector:
    matchLabels:
      app: headwind
  policyTypes:
  - Ingress
  ingress:
  # Allow ingress controller
  - from:
    - namespaceSelector:
        matchLabels:
          name: ingress-nginx
    ports:
    - protocol: TCP
      port: 8080
  # Allow internal cluster access
  - from:
    - podSelector: {}
    ports:
    - protocol: TCP
      port: 8081  # API
    - protocol: TCP
      port: 9090  # Metrics
```

#### 2. Webhook Signature Verification

Always configure webhook secrets for signature verification:

```yaml
# In your notification ConfigMap
webhook:
  enabled: true
  url: "https://your-receiver.example.com/headwind"
  secret: "use-strong-random-secret-here"  # Generate with: openssl rand -hex 32
```

Headwind generates HMAC-SHA256 signatures for outgoing webhooks using this secret.

#### 3. IP Whitelisting

For known registries, whitelist their IP ranges:

```yaml
# Ingress with IP whitelisting
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: headwind-webhook
  namespace: headwind-system
  annotations:
    nginx.ingress.kubernetes.io/whitelist-source-range: |
      192.0.2.0/24,198.51.100.0/24,203.0.113.0/24
spec:
  # ... rest of configuration
```

Common registry IP ranges:
- Docker Hub: Check [Docker Hub IP ranges](https://docs.docker.com/docker-hub/webhooks/)
- GitLab: Check [GitLab webhook IP ranges](https://docs.gitlab.com/ee/user/gitlab_com/index.html#ip-range)

#### 4. TLS Client Certificate Authentication

For maximum security, use mutual TLS:

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: headwind-webhook
  namespace: headwind-system
  annotations:
    nginx.ingress.kubernetes.io/auth-tls-verify-client: "on"
    nginx.ingress.kubernetes.io/auth-tls-secret: "headwind-system/client-ca-cert"
spec:
  # ... rest of configuration
```

### Testing Webhook Configuration

#### Test with curl

```bash
# Test Docker Hub format
curl -X POST https://headwind.example.com/webhook/dockerhub \
  -H "Content-Type: application/json" \
  -d '{
    "push_data": {
      "tag": "v1.0.0"
    },
    "repository": {
      "repo_name": "myorg/myapp",
      "namespace": "myorg",
      "name": "myapp"
    }
  }'

# Test generic registry format
curl -X POST https://headwind.example.com/webhook/registry \
  -H "Content-Type: application/json" \
  -H "X-Headwind-Signature: your-secret" \
  -d '{
    "repository": "myorg/myapp",
    "tag": "v1.0.0",
    "image": "registry.example.com/myorg/myapp:v1.0.0"
  }'
```

#### Verify in Headwind logs

```bash
# Check webhook server logs
kubectl logs -n headwind-system -l app=headwind --tail=100 -f | grep webhook

# You should see:
# [INFO  headwind::webhook] Received webhook from Docker Hub for myorg/myapp:v1.0.0
# [INFO  headwind::webhook] Processing image update: myorg/myapp:v1.0.0
```

### Complete End-to-End Example

Here's a complete setup for production:

1. **Install prerequisites**:

```bash
# Install NGINX Ingress Controller
kubectl apply -f https://raw.githubusercontent.com/kubernetes/ingress-nginx/controller-v1.8.2/deploy/static/provider/cloud/deploy.yaml

# Install cert-manager
kubectl apply -f https://github.com/cert-manager/cert-manager/releases/download/v1.13.0/cert-manager.yaml

# Wait for installations
kubectl wait --namespace ingress-nginx \
  --for=condition=ready pod \
  --selector=app.kubernetes.io/component=controller \
  --timeout=120s
```

2. **Create ClusterIssuer**:

```bash
kubectl apply -f - <<EOF
apiVersion: cert-manager.io/v1
kind: ClusterIssuer
metadata:
  name: letsencrypt-prod
spec:
  acme:
    server: https://acme-v02.api.letsencrypt.org/directory
    email: admin@example.com
    privateKeySecretRef:
      name: letsencrypt-prod
    solvers:
    - http01:
        ingress:
          class: nginx
EOF
```

3. **Deploy Headwind with notification config**:

```bash
# Create notification ConfigMap
kubectl apply -f examples/notification-configmap.yaml

# Deploy Headwind
kubectl apply -f deploy/k8s/deployment.yaml
kubectl apply -f deploy/k8s/service.yaml
```

4. **Create secure Ingress**:

```bash
kubectl apply -f - <<EOF
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: headwind-webhook
  namespace: headwind-system
  annotations:
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
    nginx.ingress.kubernetes.io/ssl-redirect: "true"
    nginx.ingress.kubernetes.io/limit-rps: "10"
spec:
  ingressClassName: nginx
  tls:
  - hosts:
    - headwind.yourdomain.com
    secretName: headwind-webhook-tls
  rules:
  - host: headwind.yourdomain.com
    http:
      paths:
      - path: /webhook
        pathType: Prefix
        backend:
          service:
            name: headwind-webhook
            port:
              number: 8080
EOF
```

5. **Configure your container registry**:

Point your registry's webhook to: `https://headwind.yourdomain.com/webhook/registry`

6. **Test the setup**:

```bash
# Push a test image
docker tag myapp:latest registry.example.com/myapp:test
docker push registry.example.com/myapp:test

# Check Headwind logs
kubectl logs -n headwind-system -l app=headwind --tail=50
```

### Troubleshooting

#### Webhook not receiving events

1. **Check Ingress status**:
```bash
kubectl get ingress -n headwind-system
kubectl describe ingress headwind-webhook -n headwind-system
```

2. **Verify DNS resolution**:
```bash
nslookup headwind.example.com
dig headwind.example.com
```

3. **Test connectivity from outside cluster**:
```bash
curl -v https://headwind.example.com/webhook/registry
# Should return 405 Method Not Allowed (GET not allowed, only POST)
```

4. **Check certificate**:
```bash
kubectl get certificate -n headwind-system
kubectl describe certificate headwind-webhook-tls -n headwind-system
```

5. **Review NGINX Ingress logs**:
```bash
kubectl logs -n ingress-nginx -l app.kubernetes.io/component=controller --tail=100
```

#### 502 Bad Gateway errors

- Verify service and pod are running:
```bash
kubectl get svc -n headwind-system
kubectl get pods -n headwind-system
kubectl logs -n headwind-system -l app=headwind
```

#### Certificate issues

- Check cert-manager logs:
```bash
kubectl logs -n cert-manager -l app=cert-manager
kubectl describe certificate headwind-webhook-tls -n headwind-system
kubectl describe certificaterequest -n headwind-system
```

## Related Issues

- Related to: #2, #6 (Approval workflow)

## Estimated Effort

Medium-Large (8-12 hours)
