# Headwind

[![CI](https://github.com/headwind-sh/headwind/actions/workflows/ci.yml/badge.svg)](https://github.com/headwind-sh/headwind/actions/workflows/ci.yml)
[![Security](https://github.com/headwind-sh/headwind/actions/workflows/security.yml/badge.svg)](https://github.com/headwind-sh/headwind/actions/workflows/security.yml)
[![Release](https://github.com/headwind-sh/headwind/actions/workflows/release.yml/badge.svg)](https://github.com/headwind-sh/headwind/actions/workflows/release.yml)
[![Documentation](https://github.com/headwind-sh/headwind/actions/workflows/deploy-docs.yml/badge.svg)](https://github.com/headwind-sh/headwind/actions/workflows/deploy-docs.yml)

A Kubernetes operator for automating workload updates based on container image changes, written in Rust.

Headwind monitors container registries and automatically updates your Kubernetes workloads when new images are available, with intelligent semantic versioning policies and approval workflows.

## Features

- **Dual Update Triggers**: Event-driven webhooks **or** registry polling for maximum flexibility
- **Semver Policy Engine**: Intelligent update decisions based on semantic versioning (patch, minor, major, glob, force, all)
- **Web UI Dashboard**: Modern web interface with:
  - Real-time filtering, sorting, and pagination
  - Multi-mode authentication (none, simple header, Kubernetes token, proxy/ingress)
  - Audit logging for all approval/rejection actions
  - Auto-refresh every 30 seconds
  - Responsive design for desktop and mobile
- **Observability Dashboard**: Built-in metrics visualization with:
  - Multi-backend support (Prometheus, VictoriaMetrics, InfluxDB)
  - Auto-discovery of available backends
  - Real-time metrics cards and time-series data
  - Hot-reload configuration management
- **Approval Workflow**: Full HTTP API for approval requests with integration possibilities (Slack, webhooks, etc.)
- **Rollback Support**: Manual rollback to previous versions with update history tracking and automatic rollback on failures
- **Notifications**: Slack, Microsoft Teams, and generic webhook notifications with dashboard links for all deployment events
- **Full Observability**: Prometheus metrics (35+ metrics), distributed tracing, and structured logging
- **Resource Support**:
  - Kubernetes Deployments ✅
  - Kubernetes StatefulSets ✅
  - Kubernetes DaemonSets ✅
  - Flux HelmReleases ✅
- **Lightweight**: Single binary, no database required
- **Secure**: Runs as non-root, read-only filesystem, minimal permissions

## Quick Start

### Prerequisites

- Kubernetes cluster (1.25+)
- kubectl configured
- Docker (for building the image)

### Installation

```bash
# Build the Docker image
docker build -t headwind:latest .

# Load into your cluster (for kind/minikube)
kind load docker-image headwind:latest  # or minikube image load headwind:latest

# Apply all Kubernetes manifests
kubectl apply -f deploy/k8s/namespace.yaml
kubectl apply -f deploy/k8s/crds/updaterequest.yaml

# Optional: Apply HelmRepository CRD if you want Helm chart auto-discovery
# (Skip if you already have Flux CD installed)
kubectl apply -f deploy/k8s/crds/helmrepository.yaml

kubectl apply -f deploy/k8s/rbac.yaml
kubectl apply -f deploy/k8s/deployment.yaml
kubectl apply -f deploy/k8s/service.yaml
```

### Configuration

Add annotations to your Deployments to enable Headwind:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-app
  annotations:
    # Update policy: none, patch, minor, major, glob, force, all
    headwind.sh/policy: "minor"

    # Require approval before updating (default: true)
    headwind.sh/require-approval: "true"

    # Minimum time between updates in seconds (default: 300)
    headwind.sh/min-update-interval: "300"

    # Specific images to track (comma-separated, empty = all)
    headwind.sh/images: "nginx, redis"

    # Event source: webhook, polling, both, none (default: webhook)
    headwind.sh/event-source: "webhook"

    # Per-resource polling interval in seconds (overrides global HEADWIND_POLLING_INTERVAL)
    # Only applies when event-source is "polling" or "both"
    headwind.sh/polling-interval: "600"

    # Automatic rollback on deployment failures (default: false)
    headwind.sh/auto-rollback: "true"

    # Rollback timeout in seconds (default: 300)
    headwind.sh/rollback-timeout: "300"

    # Health check retries before rollback (default: 3)
    headwind.sh/health-check-retries: "3"
spec:
  # ... rest of deployment spec
```

### Flux HelmRelease Support

Headwind can monitor Flux HelmRelease resources and **automatically discover new Helm chart versions** from Helm repositories, updating based on semantic versioning policies.

#### Prerequisites

Headwind requires the HelmRepository CRD to query Helm repositories for available chart versions:

**If you have Flux CD installed:** The CRD already exists - no action needed!

**If you DON'T have Flux CD:** Apply the HelmRepository CRD:
```bash
kubectl apply -f deploy/k8s/crds/helmrepository.yaml
```

#### Setup

Headwind supports both **traditional HTTP Helm repositories** and modern **OCI registries** (like ECR, GCR, ACR, Harbor, JFrog Artifactory, GitHub Container Registry, etc.).

1. Create a HelmRepository resource pointing to your Helm repository:

**HTTP Helm Repository:**
```yaml
apiVersion: source.toolkit.fluxcd.io/v1
kind: HelmRepository
metadata:
  name: my-repo
  namespace: default
spec:
  url: https://charts.example.com  # Traditional HTTP Helm repository
  interval: 5m
  type: default
  # Optional: for private repositories
  secretRef:
    name: helm-repo-credentials  # Secret with username/password keys
```

**OCI Registry (ECR, GCR, ACR, Harbor, JFrog, GHCR, etc.):**
```yaml
apiVersion: source.toolkit.fluxcd.io/v1
kind: HelmRepository
metadata:
  name: my-oci-repo
  namespace: default
spec:
  url: oci://registry.example.com/helm-charts  # OCI registry URL
  interval: 5m
  type: oci
  # Optional: for private registries
  secretRef:
    name: oci-registry-credentials  # Secret with username/password keys
```

**Note:** Headwind automatically detects whether to use HTTP or OCI based on the URL scheme (`https://` vs `oci://`).

#### Known Limitations

**OCI Registry Support**: Due to a limitation in the underlying `oci-distribution` Rust crate (v0.11), OCI Helm repositories may incorrectly query Docker Hub when the chart name matches a common Docker image name (e.g., `busybox`, `nginx`, `redis`, `postgres`). This results in discovering Docker container image tags instead of Helm chart versions.

**Workaround**: Use traditional HTTP Helm repositories (fully supported) or ensure your OCI Helm chart names don't conflict with popular Docker Hub image names. This limitation is expected to be resolved in future crate updates.

**Status**: HTTP Helm repositories work perfectly and are the recommended approach until this OCI limitation is addressed.

2. Create a HelmRelease with Headwind annotations:

```yaml
apiVersion: helm.toolkit.fluxcd.io/v2
kind: HelmRelease
metadata:
  name: my-app
  namespace: default
  annotations:
    # Update policy: none, patch, minor, major, glob, force, all
    headwind.sh/policy: "minor"

    # Require approval before updating (default: true)
    headwind.sh/require-approval: "true"

    # Minimum time between updates in seconds (default: 300)
    headwind.sh/min-update-interval: "300"

    # Event source: webhook, polling, both, none (default: webhook)
    headwind.sh/event-source: "webhook"

    # Per-resource polling interval in seconds (overrides global HEADWIND_POLLING_INTERVAL)
    # Only applies when event-source is "polling" or "both"
    headwind.sh/polling-interval: "600"
spec:
  interval: 5m
  chart:
    spec:
      chart: my-app
      version: "1.2.3"  # Headwind monitors this version
      sourceRef:
        kind: HelmRepository
        name: my-repo
        namespace: default
  values:
    # ... your values
```

**How it works:**
1. Headwind watches all HelmRelease resources with `headwind.sh/policy` annotation
2. **Automatically queries the referenced HelmRepository for available chart versions**
3. Uses the PolicyEngine to find the best matching version based on your policy
4. Compares discovered versions with `status.lastAttemptedRevision` or `spec.chart.spec.version`
5. Either:
   - Creates an UpdateRequest CRD if `require-approval: "true"` (default)
   - Applies the update directly if `require-approval: "false"` (respects `min-update-interval`)
6. Sends notifications (Slack, Teams, webhooks) about the update

**Configuration:**

Automatic version discovery is enabled by default. To disable:
```yaml
# deploy/k8s/deployment.yaml
env:
- name: HEADWIND_HELM_AUTO_DISCOVERY
  value: "false"
```

**Private Helm Repositories:**

For private repositories requiring authentication, create a Secret:
```yaml
apiVersion: v1
kind: Secret
metadata:
  name: helm-repo-credentials
  namespace: default
type: Opaque
stringData:
  username: myusername
  password: mypassword
```

**Metrics:**
Helm-specific metrics are available at `/metrics`:
- `headwind_helm_releases_watched` - Number of HelmReleases being monitored
- `headwind_helm_chart_versions_checked_total` - Version checks performed
- `headwind_helm_updates_found_total` - Updates discovered
- `headwind_helm_updates_approved_total` - Updates approved by policy
- `headwind_helm_updates_rejected_total` - Updates rejected by policy
- `headwind_helm_updates_applied_total` - Updates successfully applied to HelmReleases
- `headwind_helm_repository_queries_total` - Repository index queries performed
- `headwind_helm_repository_errors_total` - Repository query errors
- `headwind_helm_repository_query_duration_seconds` - Repository query duration

## Update Policies

- **none**: Never update automatically (default)
- **patch**: Only update patch versions (1.2.3 → 1.2.4)
- **minor**: Update minor versions (1.2.3 → 1.3.0)
- **major**: Update major versions (1.2.3 → 2.0.0)
- **all**: Update to any new version
- **glob**: Match glob pattern (specify with `headwind.sh/pattern`)
- **force**: Force update regardless of version

## Update Triggers

Headwind supports two methods for detecting new images:

### 1. Webhooks (Recommended)

Event-driven updates are faster and more efficient. Configure your registry to send webhooks to Headwind.

**Docker Hub:**
```
Webhook URL: http://<headwind-webhook-service>/webhook/dockerhub
```

**Generic Registry (Harbor, GitLab, GCR, etc.):**
```
Webhook URL: http://<headwind-webhook-service>/webhook/registry
```

For external access, use an Ingress or LoadBalancer service.

### 2. Registry Polling (Fallback)

If webhooks aren't available, enable registry polling:

```yaml
# deploy/k8s/deployment.yaml
env:
- name: HEADWIND_POLLING_ENABLED
  value: "true"
- name: HEADWIND_POLLING_INTERVAL
  value: "300"  # Poll every 5 minutes
```

**When to use polling:**
- Registry doesn't support webhooks
- Headwind is not publicly accessible
- Testing or development environments

**Note:** Polling is less efficient and has a delay. Use webhooks when possible.

### 3. Per-Resource Event Source Configuration

By default, all resources use webhooks as their event source (`headwind.sh/event-source: "webhook"`). You can override this on a per-resource basis:

**Event Source Options:**
- `webhook` (default) - Only respond to webhook events, skip registry polling
- `polling` - Only use registry polling, ignore webhook events
- `both` - Respond to both webhooks and polling (redundant but ensures coverage)
- `none` - Disable all update triggers for this resource

**Use Cases:**

**Webhook-only resources** (default):
```yaml
metadata:
  annotations:
    headwind.sh/policy: "minor"
    headwind.sh/event-source: "webhook"  # Can be omitted (default)
```
Best for registries with webhook support. Updates are immediate when new images are pushed.

**Polling-only resources:**
```yaml
metadata:
  annotations:
    headwind.sh/policy: "minor"
    headwind.sh/event-source: "polling"
    headwind.sh/polling-interval: "600"  # Optional: poll every 10 minutes
```
Best for:
- Registries without webhook support
- Resources that should be checked less frequently
- Development/staging environments

**Both webhooks and polling:**
```yaml
metadata:
  annotations:
    headwind.sh/policy: "minor"
    headwind.sh/event-source: "both"
```
Provides redundancy - updates will be detected via webhooks (fast) or polling (fallback).

**Per-resource polling intervals:**

When using `event-source: "polling"` or `event-source: "both"`, you can override the global `HEADWIND_POLLING_INTERVAL` for specific resources:

```yaml
metadata:
  annotations:
    headwind.sh/policy: "minor"
    headwind.sh/event-source: "polling"
    headwind.sh/polling-interval: "60"   # Poll this resource every 60 seconds
```

This allows you to poll critical resources more frequently while checking less critical resources less often, reducing registry API load.

**Example: Mixed event sources in a namespace:**

```yaml
# Production API - webhook-only (fastest)
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: api-production
  annotations:
    headwind.sh/policy: "patch"
    headwind.sh/event-source: "webhook"
---
# Staging API - polling every 5 minutes
apiVersion: apps/v1
kind: Deployment
metadata:
  name: api-staging
  annotations:
    headwind.sh/policy: "all"
    headwind.sh/event-source: "polling"
    headwind.sh/polling-interval: "300"
    headwind.sh/require-approval: "false"
---
# Background job - polling every 30 minutes (low priority)
apiVersion: apps/v1
kind: Deployment
metadata:
  name: background-job
  annotations:
    headwind.sh/policy: "minor"
    headwind.sh/event-source: "polling"
    headwind.sh/polling-interval: "1800"
```

## Working with UpdateRequests

Headwind creates `UpdateRequest` custom resources when it detects a new image version that matches a Deployment's policy. These CRDs track the approval workflow.

### Viewing UpdateRequests

```bash
# List all UpdateRequests
kubectl get updaterequests -A

# Get details of a specific UpdateRequest
kubectl get updaterequest <name> -n <namespace> -o yaml

# Watch for new UpdateRequests in real-time
kubectl get updaterequests -A --watch
```

### UpdateRequest Status

Each UpdateRequest has a phase indicating its current state:
- **Pending**: Waiting for approval
- **Completed**: Approved and successfully applied
- **Rejected**: Rejected by approver
- **Failed**: Approval granted but update failed

### Example UpdateRequest

```yaml
apiVersion: headwind.sh/v1alpha1
kind: UpdateRequest
metadata:
  name: nginx-update-1-26-0
  namespace: default
spec:
  targetRef:
    kind: Deployment
    name: nginx-example
    namespace: default
  containerName: nginx
  currentImage: nginx:1.25.0
  newImage: nginx:1.26.0
  policy: minor
status:
  phase: Pending
  createdAt: "2025-11-06T01:00:00Z"
  lastUpdated: "2025-11-06T01:00:00Z"
```

## Web UI Dashboard

Headwind provides a modern web-based dashboard for viewing and managing update requests.

### Accessing the Web UI

The Web UI is available on port **8082** by default:

```bash
# Port forward to access locally
kubectl port-forward -n headwind-system svc/headwind-ui 8082:8082

# Open in browser
open http://localhost:8082
```

### Features

- **Dashboard View**: List all pending and completed UpdateRequests across all namespaces
- **Filtering & Search**:
  - Real-time search by resource name or image
  - Filter by namespace
  - Filter by resource kind (Deployment, StatefulSet, DaemonSet, HelmRelease)
  - Filter by policy type
- **Sorting**: Sort by date (newest/oldest first), namespace, or resource name
- **Pagination**: View updates in pages of 20 items
- **One-Click Actions**:
  - Approve updates with confirmation
  - Reject updates with reason (modal dialog)
  - View detailed information for each update
- **Real-time Notifications**: Toast notifications for success/error
- **Responsive Design**: Works on desktop and mobile

### Screenshots

The Web UI provides:
- **Stats Cards**: Quick overview of pending and completed updates
- **Pending Updates Table**: Actionable list with approve/reject buttons
- **Completed Updates**: Collapsible history of processed updates
- **Detail View**: Full information about each UpdateRequest

Access at `http://localhost:8082` when port-forwarded, or expose via Service/Ingress for remote access.

### Authentication

The Web UI supports four authentication modes configured via the `HEADWIND_UI_AUTH_MODE` environment variable:

#### 1. None (Default)
No authentication required. All actions are logged as "web-ui-user".

```yaml
env:
  - name: HEADWIND_UI_AUTH_MODE
    value: "none"
```

#### 2. Simple Header Authentication
Reads username from `X-User` HTTP header. Suitable for use behind an authenticating reverse proxy.

```yaml
env:
  - name: HEADWIND_UI_AUTH_MODE
    value: "simple"
```

Example usage:
```bash
curl -H "X-User: alice" http://localhost:8082/
```

#### 3. Kubernetes Token Authentication
Validates bearer tokens using Kubernetes TokenReview API and extracts the authenticated username.

```yaml
env:
  - name: HEADWIND_UI_AUTH_MODE
    value: "token"
```

**Requirements**:
- RBAC permission for `authentication.k8s.io/tokenreviews` (already included in `deploy/k8s/rbac.yaml`)

Example usage:
```bash
# Get service account token
TOKEN=$(kubectl create token my-service-account -n default)

# Access Web UI with token
curl -H "Authorization: Bearer $TOKEN" http://localhost:8082/
```

#### 4. Proxy/Ingress Authentication
Reads username from a configurable HTTP header set by an ingress controller or authentication proxy (e.g., oauth2-proxy, Authelia).

```yaml
env:
  - name: HEADWIND_UI_AUTH_MODE
    value: "proxy"
  - name: HEADWIND_UI_PROXY_HEADER  # Optional, defaults to X-Forwarded-User
    value: "X-Auth-Request-User"
```

### Audit Logging

All approval and rejection actions are logged with structured audit information:

```json
{
  "timestamp": "2025-11-08T23:00:00Z",
  "username": "alice",
  "action": "approve",
  "resource_type": "Deployment",
  "namespace": "default",
  "name": "nginx-update-1-26-0",
  "result": "success"
}
```

Audit logs use the dedicated log target `headwind::audit` and can be filtered with:

```bash
kubectl logs -n headwind-system deployment/headwind | grep headwind::audit
```

### Auto-Refresh

The dashboard automatically refreshes every 30 seconds to show the latest UpdateRequests. This can be disabled by clicking the "Auto-refresh" toggle in the UI.

### Configuration Management

The Web UI supports hot-reload configuration via ConfigMap. Changes to the ConfigMap are detected automatically without requiring pod restarts.

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: headwind-ui-config
  namespace: headwind-system
data:
  config.yaml: |
    refresh_interval: 30
    max_items_per_page: 20
```

Mount the ConfigMap in the deployment:
```yaml
volumeMounts:
  - name: ui-config
    mountPath: /etc/headwind/ui
volumes:
  - name: ui-config
    configMap:
      name: headwind-ui-config
```

### Observability Dashboard

The Web UI includes a comprehensive observability dashboard at `/observability` with real-time metrics visualization.

#### Features

- **Multi-Backend Support**: Automatically detects and connects to Prometheus, VictoriaMetrics, or InfluxDB v2
- **Auto-Discovery**: Automatically finds available metrics backends in your cluster
- **Fallback Mode**: Falls back to parsing `/metrics` endpoint if no backend is available
- **Real-Time Data**: Auto-refreshes every 30 seconds
- **Interactive Time-Series Charts**: Chart.js-powered visualizations showing 24-hour trends (Prometheus/VictoriaMetrics/InfluxDB only)
  - Updates Over Time (approved, applied, failed)
  - Resources Watched (deployments, statefulsets, daemonsets, helmreleases)
- **Key Metrics Cards**:
  - Updates: Pending, Approved, Applied, Failed
  - Resources Watched: Deployments, StatefulSets, DaemonSets, HelmReleases

#### Configuration

Configure metrics backend via ConfigMap:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: headwind-config
  namespace: headwind-system
data:
  config.yaml: |
    observability:
      metricsBackend: "auto"  # auto | prometheus | victoriametrics | influxdb | live
      prometheus:
        enabled: true
        url: "http://prometheus-server.monitoring.svc.cluster.local:80"
      victoriametrics:
        enabled: false
        url: "http://victoria-metrics.monitoring.svc.cluster.local:8428"
      influxdb:
        enabled: false
        url: "http://influxdb.monitoring.svc.cluster.local:8086"
        org: "headwind"              # InfluxDB v2 organization
        bucket: "metrics"            # InfluxDB v2 bucket
        token: "your-api-token"      # InfluxDB v2 API token
```

**Backend Options:**
- `auto` - Automatically detects available backend (default)
- `prometheus` - Use Prometheus for metrics storage and queries
- `victoriametrics` - Use VictoriaMetrics (Prometheus-compatible API)
- `influxdb` - Use InfluxDB v2 for time-series data
- `live` - Parse metrics directly from `/metrics` endpoint (no external backend)

**Auto-Discovery Priority:** Prometheus → VictoriaMetrics → InfluxDB → Live

#### API Endpoints

```bash
# Get current metrics from configured backend
curl http://localhost:8082/api/v1/metrics

# Get 24-hour time-series data for specific metric
curl http://localhost:8082/api/v1/metrics/timeseries/headwind_updates_pending
```

#### Prometheus Integration Example

Deploy Prometheus to scrape Headwind metrics:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: prometheus-config
  namespace: monitoring
data:
  prometheus.yml: |
    scrape_configs:
      - job_name: 'headwind'
        static_configs:
          - targets: ['headwind-metrics.headwind-system.svc.cluster.local:9090']
        scrape_interval: 15s
```

The observability dashboard will automatically detect Prometheus and display metrics from it.

## API Endpoints

### Approval API (Port 8081)

```bash
# List all pending updates across all namespaces
curl http://headwind-api:8081/api/v1/updates

# Get specific update by namespace and name
curl http://headwind-api:8081/api/v1/updates/{namespace}/{name}

# Approve update (automatically executes the update)
curl -X POST http://headwind-api:8081/api/v1/updates/{namespace}/{name}/approve \
  -H "Content-Type: application/json" \
  -d '{"approver":"user@example.com"}'

# Reject update with reason
curl -X POST http://headwind-api:8081/api/v1/updates/{namespace}/{name}/reject \
  -H "Content-Type: application/json" \
  -d '{"approver":"user@example.com","reason":"Not ready for production"}'

# Example: Approve an update
curl -X POST http://localhost:8081/api/v1/updates/default/nginx-update-1-26-0/approve \
  -H "Content-Type: application/json" \
  -d '{"approver":"admin@example.com"}'
```

**Note**: Approving an update immediately executes the deployment update and updates the UpdateRequest CRD status.

### Rollback API

Headwind automatically tracks update history for all deployments and provides manual rollback capabilities.

#### Using kubectl Plugin (Recommended)

```bash
# Install the kubectl plugin
sudo cp kubectl-headwind /usr/local/bin/
sudo chmod +x /usr/local/bin/kubectl-headwind

# Rollback a deployment
kubectl headwind rollback nginx-deployment -n production

# View update history
kubectl headwind history nginx-deployment -n production

# List all pending updates
kubectl headwind list

# Approve/reject updates
kubectl headwind approve nginx-update-v1-27-0 --approver admin@example.com
kubectl headwind reject nginx-update-v1-27-0 "Not ready" --approver admin@example.com
```

See [KUBECTL_PLUGIN.md](KUBECTL_PLUGIN.md) for complete plugin documentation.

#### Using curl directly

```bash
# Get update history for a deployment
curl http://headwind-api:8081/api/v1/rollback/{namespace}/{deployment}/history

# Rollback to the previous image
curl -X POST http://headwind-api:8081/api/v1/rollback/{namespace}/{deployment}/{container}

# Example: Rollback nginx deployment
curl -X POST http://localhost:8081/api/v1/rollback/default/nginx-example/nginx

# Get history
curl http://localhost:8081/api/v1/rollback/default/nginx-example/history
```

#### Automatic Rollback

When enabled, Headwind automatically monitors deployment health after updates and rolls back if failures are detected:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-app
  annotations:
    # Enable automatic rollback (default: false)
    headwind.sh/auto-rollback: "true"

    # How long to monitor deployment health (default: 300s)
    headwind.sh/rollback-timeout: "300"

    # Number of failed health checks before rollback (default: 3)
    headwind.sh/health-check-retries: "3"
```

Automatic rollback triggers on:
- **CrashLoopBackOff**: Pods repeatedly crashing
- **ImagePullBackOff**: Unable to pull new image
- **High restart count**: Container restarts > 5 times
- **Readiness failures**: Pods not becoming ready
- **Deployment deadline exceeded**: ProgressDeadlineExceeded condition

When a failure is detected, Headwind automatically:
1. Logs the failure reason
2. Reverts to the previous working image
3. Creates a rollback entry in the update history
4. Continues monitoring the rolled-back deployment

#### Update History

All updates are tracked in deployment annotations:

```bash
# View update history in deployment annotations
kubectl get deployment my-app -o jsonpath='{.metadata.annotations.headwind\.sh/update-history}' | jq

# Example output:
[
  {
    "container": "app",
    "image": "myapp:v1.2.0",
    "timestamp": "2025-11-06T10:30:00Z",
    "updateRequestName": "myapp-update-v1-2-0",
    "approvedBy": "admin@example.com"
  },
  {
    "container": "app",
    "image": "myapp:v1.1.0",
    "timestamp": "2025-11-05T14:20:00Z",
    "updateRequestName": "myapp-update-v1-1-0",
    "approvedBy": "webhook"
  }
]
```

Headwind keeps the last 10 updates per container.

### Notifications

Headwind can send notifications about deployment updates to Slack, Microsoft Teams, or generic webhooks.

#### Configuration

Configure notifications using environment variables in `deploy/k8s/deployment.yaml`:

```yaml
env:
# Slack Configuration
- name: SLACK_ENABLED
  value: "true"
- name: SLACK_WEBHOOK_URL
  value: "https://hooks.slack.com/services/YOUR/WEBHOOK/URL"
- name: SLACK_CHANNEL  # Optional: override webhook default
  value: "#deployments"
- name: SLACK_USERNAME  # Optional: customize bot name
  value: "Headwind Bot"
- name: SLACK_ICON_EMOJI  # Optional: customize bot icon
  value: ":rocket:"

# Microsoft Teams Configuration
- name: TEAMS_ENABLED
  value: "true"
- name: TEAMS_WEBHOOK_URL
  value: "https://outlook.office.com/webhook/YOUR-WEBHOOK-URL"

# Generic Webhook Configuration
- name: WEBHOOK_ENABLED
  value: "true"
- name: WEBHOOK_URL
  value: "https://your-webhook-endpoint.com/notifications"
- name: WEBHOOK_SECRET  # Optional: HMAC signature verification
  value: "your-secret-key"
- name: WEBHOOK_TIMEOUT  # Optional: timeout in seconds (default: 10)
  value: "10"
- name: WEBHOOK_MAX_RETRIES  # Optional: max retries (default: 3)
  value: "3"

# Dashboard Integration
- name: HEADWIND_UI_URL  # Optional: adds "View in Dashboard" links to notifications
  value: "https://headwind.example.com"  # or http://localhost:8082 for local
```

#### Notification Events

Headwind sends notifications for the following events:

- **UpdateRequestCreated**: New UpdateRequest CRD created (requires approval)
- **UpdateApproved**: Update approved by user
- **UpdateRejected**: Update rejected by user
- **UpdateCompleted**: Update successfully applied
- **UpdateFailed**: Update failed to apply
- **RollbackTriggered**: Automatic rollback triggered due to health check failure
- **RollbackCompleted**: Rollback completed successfully
- **RollbackFailed**: Rollback failed

#### Slack Integration

Slack notifications use Block Kit for rich formatting with:
- Color-coded messages by event type
- Deployment details (namespace, name, images)
- Interactive "View in Dashboard" button (when `HEADWIND_UI_URL` is set)
- Interactive "Approve" button (when approval API is available)
- Timestamp with relative time formatting

#### Microsoft Teams Integration

Teams notifications use Adaptive Cards with:
- Color themes matching event severity
- Structured fact display
- "View in Dashboard" action button (when `HEADWIND_UI_URL` is set)
- "Approve" action button (when approval API is available)
- Kubernetes logo branding

#### Generic Webhook Format

Generic webhooks receive JSON payloads with HMAC SHA256 signature verification:

```json
{
  "event": "update_completed",
  "timestamp": "2025-11-06T10:30:00Z",
  "deployment": {
    "name": "nginx",
    "namespace": "production",
    "currentImage": "nginx:1.25.0",
    "newImage": "nginx:1.26.0",
    "container": "nginx"
  },
  "policy": "minor",
  "requiresApproval": true,
  "updateRequestName": "nginx-update-1-26-0"
}
```

Signature is sent in the `X-Headwind-Signature` header as `sha256=<hex>`.

To verify:
```python
import hmac
import hashlib

def verify_signature(secret, payload, signature):
    expected = hmac.new(
        secret.encode(),
        payload.encode(),
        hashlib.sha256
    ).hexdigest()
    return f"sha256={expected}" == signature
```

#### Notification Metrics

Monitor notification delivery with Prometheus metrics:
- `headwind_notifications_sent_total` - Total notifications sent successfully
- `headwind_notifications_failed_total` - Total notification failures
- `headwind_notifications_slack_sent_total` - Notifications sent to Slack
- `headwind_notifications_teams_sent_total` - Notifications sent to Teams
- `headwind_notifications_webhook_sent_total` - Notifications sent via webhook

### Metrics (Port 9090)

Prometheus metrics available at:
```
http://headwind-metrics:9090/metrics
```

Available metrics:
- `headwind_webhook_events_total` - Total webhook events received
- `headwind_webhook_events_processed` - Successfully processed events
- `headwind_polling_cycles_total` - Total polling cycles completed
- `headwind_polling_images_checked_total` - Images checked during polling
- `headwind_polling_new_tags_found_total` - New tags discovered via polling
- `headwind_polling_helm_charts_checked_total` - Helm charts checked during polling
- `headwind_polling_helm_new_versions_found_total` - Helm chart versions discovered via polling
- `headwind_polling_errors_total` - Polling errors encountered
- `headwind_updates_pending` - Updates awaiting approval
- `headwind_updates_approved_total` - Total approved updates
- `headwind_updates_rejected_total` - Total rejected updates
- `headwind_updates_applied_total` - Successfully applied updates
- `headwind_updates_failed_total` - Failed update attempts
- `headwind_updates_skipped_interval_total` - Updates skipped due to minimum interval not elapsed
- `headwind_reconcile_duration_seconds` - Controller reconciliation time
- `headwind_deployments_watched` - Number of watched Deployments
- `headwind_helm_releases_watched` - Number of watched HelmReleases
- `headwind_helm_chart_versions_checked_total` - Helm chart version checks performed
- `headwind_helm_updates_found_total` - Helm chart updates discovered
- `headwind_helm_updates_approved_total` - Helm chart updates approved by policy
- `headwind_helm_updates_rejected_total` - Helm chart updates rejected by policy
- `headwind_helm_updates_applied_total` - Helm chart updates successfully applied
- `headwind_rollbacks_total` - Total rollback operations performed
- `headwind_rollbacks_manual_total` - Manual rollback operations
- `headwind_rollbacks_automatic_total` - Automatic rollback operations
- `headwind_rollbacks_failed_total` - Failed rollback operations
- `headwind_deployment_health_checks_total` - Deployment health checks performed
- `headwind_deployment_health_failures_total` - Deployment health check failures detected
- `headwind_notifications_sent_total` - Total notifications sent successfully
- `headwind_notifications_failed_total` - Total notification failures
- `headwind_notifications_slack_sent_total` - Notifications sent to Slack
- `headwind_notifications_teams_sent_total` - Notifications sent to Teams
- `headwind_notifications_webhook_sent_total` - Notifications sent via webhook

## Architecture

```
┌─────────────────┐
│  Registry       │
│  (Docker Hub,   │
│   Harbor, etc)  │
└────┬────────┬───┘
     │        │
     │Webhook │Polling
     │        │(optional)
     ▼        ▼
┌──────────────────┐
│  Headwind        │
│  ┌────────────┐  │
│  │  Webhook   │  │◄─── Port 8080
│  │  Server    │  │
│  └──────┬─────┘  │
│         │        │
│  ┌──────▼─────┐  │
│  │  Registry  │  │
│  │  Poller    │  │
│  └──────┬─────┘  │
│         │        │
│  ┌──────▼─────┐  │
│  │  Policy    │  │
│  │  Engine    │  │
│  └──────┬─────┘  │
│         │        │
│  ┌──────▼─────┐  │
│  │  Approval  │  │◄─── Port 8081 (API)
│  │  System    │  │
│  └──────┬─────┘  │
│         │        │
│  ┌──────▼─────┐  │
│  │Controller  │  │
│  └──────┬─────┘  │
│         │        │
│  ┌──────▼─────┐  │
│  │  Metrics   │  │◄─── Port 9090
│  └────────────┘  │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│   Kubernetes     │
│   API Server     │
└──────────────────┘
```

## Development

### Build

```bash
make build
# or
cargo build --release
```

### Test

```bash
# Run all tests (unit + integration)
make test
# or
cargo test

# Run only unit tests
cargo test --lib

# Run only integration tests
cargo test --test '*'

# Run specific integration test file
cargo test --test policy_integration_test
cargo test --test webhook_integration_test

# Run with output
cargo test -- --nocapture
```

#### Test Structure

The project includes both unit and integration tests:

**Unit Tests** (30 tests) - Located within source modules (`src/`)
- Test individual functions and components in isolation
- Run with `cargo test --lib`

**Integration Tests** (40 tests) - Located in `tests/` directory
- Test end-to-end functionality and module interaction
- `tests/policy_integration_test.rs` - Policy engine tests (12 tests)
  - Semantic versioning policies (patch, minor, major)
  - Special policies (all, none, force, glob)
  - Version prefix handling (v1.0.0)
  - Prerelease and build metadata
  - Real-world scenarios (Kubernetes versions, Docker tags)
- `tests/webhook_integration_test.rs` - Webhook parsing tests (10 tests)
  - Docker Hub webhook format
  - OCI registry webhook format
  - Multiple events in single webhook
  - Edge cases (missing tags, special characters)
- `tests/rollback_integration_test.rs` - Rollback functionality tests (18 tests)
  - Update history tracking and serialization
  - Automatic rollback configuration
  - Health status monitoring
  - History entry management (max entries, multiple containers)
  - camelCase JSON serialization

**Test Helpers** - Located in `tests/common/mod.rs`
- Reusable test fixtures and helper functions
- `create_test_deployment()` - Create Kubernetes Deployment fixtures
- `headwind_annotations()` - Generate Headwind annotation sets
- `create_dockerhub_webhook_payload()` - Docker Hub webhook JSON
- `create_registry_webhook_payload()` - OCI registry webhook JSON

#### Running Specific Test Categories

```bash
# Policy engine tests
cargo test should_update

# Webhook tests
cargo test webhook

# Test a specific policy type
cargo test patch_policy
cargo test minor_policy
cargo test glob_policy

# Test version handling
cargo test version_prefix
cargo test prerelease
```

### Development Tools

Install all development tools:

```bash
make install
```

This installs:
- `cargo-audit` - Security vulnerability scanning
- `cargo-deny` - Dependency license and security checking
- `cargo-udeps` - Unused dependency detection
- `cargo-tarpaulin` - Code coverage
- `cargo-watch` - Auto-rebuild on file changes
- `pre-commit` - Git hooks for code quality

### Pre-commit Hooks

The project uses pre-commit hooks to ensure code quality:

```bash
# Install hooks
pre-commit install

# Run manually
pre-commit run --all-files

# Hooks automatically run on git commit:
# - cargo fmt (formatting)
# - cargo clippy (linting)
# - cargo check (compilation)
# - YAML validation
# - Secret detection
# - Trailing whitespace removal
```

### Run Locally

```bash
make run
# or
RUST_LOG=headwind=debug cargo run
```

Requires `KUBECONFIG` to be set and pointing to a valid Kubernetes cluster.

## Current Status

Headwind is currently in **beta** stage (v0.2.0-alpha). Core functionality is complete and tested:

### ✅ Completed Features
- ✅ Webhook events connected to controller and create UpdateRequests
- ✅ Approved updates are automatically applied to Deployments
- ✅ Registry polling with digest-based and version discovery
- ✅ Full approval workflow with UpdateRequest CRDs
- ✅ Policy engine works and is well-tested
- ✅ All servers operational (webhook:8080, API:8081, metrics:9090)
- ✅ Kubernetes controller watches and updates Deployments
- ✅ Flux HelmRelease support with version monitoring
- ✅ Minimum update interval respected
- ✅ Deduplication to avoid update request spam
- ✅ Private registry authentication (Docker Hub, ECR, GCR, ACR, Harbor, GHCR, GitLab)
- ✅ Manual rollback functionality with update history tracking
- ✅ Automatic rollback on deployment failures
- ✅ Notification integrations (Slack, Teams, webhooks)

### 🚧 In Progress
- 🚧 Comprehensive integration tests (70 tests passing, manual testing successful)
- 🚧 CI/CD pipeline enhancements

### 📋 Planned Features
- StatefulSet and DaemonSet support
- Full Helm repository querying for automatic version discovery
- Web UI for approvals

**Production readiness**: Core workflow is functional. Suitable for testing environments. For production use, we recommend waiting for comprehensive integration tests and private registry support.

## Troubleshooting

### Headwind Not Starting

```bash
# Check logs
kubectl logs -n headwind-system deployment/headwind

# Common issues:
# 1. RBAC permissions - verify ServiceAccount has correct permissions
# 2. Cluster connectivity - ensure pod can reach Kubernetes API
# 3. Image pull - verify image is accessible
```

### Webhooks Not Received

```bash
# Test webhook endpoint
kubectl port-forward -n headwind-system svc/headwind-webhook 8080:8080
curl -X POST http://localhost:8080/webhook/dockerhub \
  -H "Content-Type: application/json" \
  -d '{
    "push_data": {"tag": "v1.2.3"},
    "repository": {"repo_name": "myimage"}
  }'

# Check webhook metrics
curl http://localhost:9090/metrics | grep webhook_events
```

### Updates Not Applying

Check the status in the approval API:

```bash
kubectl port-forward -n headwind-system svc/headwind-api 8081:8081
curl http://localhost:8081/api/v1/updates | jq
```

### Viewing Metrics

```bash
kubectl port-forward -n headwind-system svc/headwind-metrics 9090:9090
open http://localhost:9090/metrics
```

Or configure Prometheus to scrape:

```yaml
- job_name: 'headwind'
  kubernetes_sd_configs:
  - role: pod
    namespaces:
      names:
      - headwind-system
  relabel_configs:
  - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_scrape]
    action: keep
    regex: true
  - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_port]
    action: replace
    target_label: __address__
    regex: (.+):(.+)
    replacement: $1:9090
```

## Security Considerations

### Running in Production

1. **Use RBAC least-privilege**
   - Headwind only needs permissions on resources it manages
   - Review and customize `deploy/k8s/rbac.yaml`

2. **Secure webhook endpoints**
   - Use Ingress with TLS
   - Implement webhook signature verification
   - Use network policies to restrict access

3. **Protect approval API**
   - Add authentication (OAuth2/OIDC)
   - Use TLS for all connections
   - Audit all approval actions

4. **Container security**
   - Headwind runs as non-root (UID 1000)
   - Read-only root filesystem
   - No privileged escalation
   - Minimal base image (Debian slim)

### Network Policies

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: headwind-network-policy
  namespace: headwind-system
spec:
  podSelector:
    matchLabels:
      app: headwind
  policyTypes:
  - Ingress
  - Egress
  ingress:
  - from:
    - namespaceSelector: {}
    ports:
    - protocol: TCP
      port: 8080  # Webhooks
    - protocol: TCP
      port: 8081  # API
    - protocol: TCP
      port: 9090  # Metrics
  egress:
  - to:
    - namespaceSelector: {}
    ports:
    - protocol: TCP
      port: 443  # Kubernetes API
```

## Roadmap

### v0.2.0 - Core Functionality ✅ COMPLETE (except testing)
- [x] Project structure and foundation
- [x] Connect webhook events to controller (PR #21)
- [x] Implement update application (PR #20, PR #22)
- [x] Respect minimum update interval (PR #21)
- [x] UpdateRequest CRD implementation (PR #19)
- [x] Registry polling implementation (in progress - feat/registry-polling branch)
- [ ] Add comprehensive integration tests
- [ ] CI/CD pipeline enhancements

### v0.3.0 - Extended Support (Medium Priority)
- [x] Private registry authentication (completed)
- [x] Manual rollback functionality (completed)
- [x] Automatic rollback on deployment failures (completed)
- [x] Rollback metrics (completed)
- [x] kubectl plugin for rollback and approvals (completed)
- [x] Notification system (Slack, Teams, generic webhooks) (completed)
- [x] Flux HelmRelease support with automatic version discovery (completed)
  - Automatic chart version discovery from HTTP and OCI registries
  - UpdateRequest creation for chart updates
  - Approval workflow integration
  - Chart version patching on approval
  - Full metrics and notification support
- [x] StatefulSet/DaemonSet support (completed)
  - StatefulSet and DaemonSet controllers
  - Same annotation-based configuration as Deployments
  - Approval workflow integration
  - Full metrics support
- [ ] Multi-architecture Docker images (arm64, amd64)

### v0.4.0 - Enhanced UX (Low Priority)
- [ ] Web dashboard for approvals
- [ ] Custom Resource Definition for policy config
- [ ] Slack/Teams interactive approvals
- [ ] Advanced scheduling (maintenance windows, etc.)

### Future Ideas
- [ ] Multi-cluster support
- [ ] Canary deployment integration
- [ ] Custom update strategies (blue/green, rolling window)
- [ ] A/B testing support
- [ ] Rate limiting per namespace
- [ ] Policy simulation/dry-run mode

## FAQ

**Q: How is this different from Argo CD or Flux?**

A: Argo CD and Flux are GitOps tools that sync from Git. Headwind updates workloads when new *container images* are pushed to registries, regardless of Git state. They're complementary - you can use both.

**Q: Can I use this with Flux/Argo?**

A: Yes! Headwind can update the image tags, and Flux/Argo will see the change and sync. Or let Flux handle chart updates and Headwind handle image updates.

**Q: Does this work with private registries?**

A: Yes! Headwind reads credentials from your Kubernetes `imagePullSecrets`. Supports:
- Docker Hub (including Personal Access Tokens)
- AWS ECR
- Google GCR/Artifact Registry
- Azure ACR
- Harbor, GHCR, GitLab, and other registries

Simply configure your ServiceAccount's imagePullSecrets as usual, and Headwind will use them automatically.

**Q: What about rollbacks?**

A: Headwind includes both manual and automatic rollback support:
- **Manual rollback**: Use the API to rollback to previous versions (`POST /api/v1/rollback/{namespace}/{deployment}/{container}`)
- **Automatic rollback**: Enable `headwind.sh/auto-rollback: "true"` to automatically detect and rollback failed updates
- **Update history**: View the last 10 updates per container in deployment annotations
- You can also use `kubectl rollout undo` for immediate rollbacks

**Q: Can I test updates in staging first?**

A: Yes! Use different policies per namespace:
```yaml
# staging namespace - auto-update all
headwind.sh/policy: "all"
headwind.sh/require-approval: "false"

# production namespace - require approval
headwind.sh/policy: "minor"
headwind.sh/require-approval: "true"
```

**Q: What if I want to pin a specific version?**

A: Use policy: "none" to prevent any updates, or remove Headwind annotations entirely.

## Performance

Expected performance characteristics:

- **Webhook processing**: <10ms per event
- **Reconciliation loop**: <100ms per Deployment
- **Memory usage**: ~50-100MB typical
- **CPU usage**: <0.1 core typical, <0.5 core under load

Tested with:
- 1000 Deployments with Headwind annotations
- 100 webhooks/minute
- Single replica of Headwind

For larger scale, consider:
- Running multiple replicas
- Using leader election
- Filtering namespaces with label selectors

## Contributing

We welcome contributions! Please see:

- [CONTRIBUTING.md](CONTRIBUTING.md) - Contribution guidelines
- [CLAUDE.md](CLAUDE.md) - Architecture and development context
- [Issues](../../issues) - Open issues and feature requests
- [Pull Requests](../../pulls) - Current PRs

### Quick Start for Contributors

```bash
# Fork and clone
git clone https://github.com/YOUR_USERNAME/headwind.git
cd headwind

# Build and test
cargo build
cargo test

# Run locally (requires k8s cluster)
export RUST_LOG=headwind=debug
cargo run

# Create a branch
git checkout -b feature/my-feature

# Make changes, commit, and push
git commit -m "feat: add new feature"
git push origin feature/my-feature

# Open a pull request
```

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Acknowledgments

- Built with [kube-rs](https://kube.rs)
- Uses [Tokio](https://tokio.rs) async runtime
- Thanks to all contributors!
