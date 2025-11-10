# Headwind - Claude Development Context

This document contains essential context for AI assistants (particularly Claude) working on the Headwind project. It explains the architecture, design decisions, and critical information needed for effective development.

## Project Overview

Headwind is a Kubernetes operator written in Rust that automates workload updates based on container image changes. It provides both webhook-driven updates and registry polling, a full approval workflow system, and comprehensive observability.

### Core Philosophy

1. **Flexible triggers** - Support both webhooks (preferred) and polling (fallback)
2. **Safety first** - Approval workflows prevent unintended updates
3. **Observable by default** - Comprehensive metrics and tracing
4. **Minimal dependencies** - No database, single binary
5. **Rust for reliability** - Type safety, memory safety, performance

## Architecture

### High-Level Flow

```
Registry ──┬─→ Webhook Server ──→ Policy Engine → Approval System → Controller → Kubernetes API
           │                                                              ↓
           └─→ Registry Poller ──┘                                 Metrics/Tracing
              (optional)
```

### Components

#### 1. Webhook Server (`src/webhook/mod.rs`)
- **Port**: 8080
- **Purpose**: Receives webhooks from container registries
- **Endpoints**:
  - `/webhook/registry` - Generic OCI registry webhooks (Harbor, GitLab, etc.)
  - `/webhook/dockerhub` - Docker Hub specific format
  - `/health` - Health check
- **Key Functions**:
  - `start_webhook_server()` - Initializes Axum server
  - `handle_registry_webhook()` - Processes OCI registry events
  - `handle_dockerhub_webhook()` - Processes Docker Hub events
  - `process_webhook_events()` - Event processing loop (currently a stub)

**Status**: ✅ **COMPLETED** - Webhook events are now connected to the controller and create UpdateRequests

**Features**:
1. **Event-source filtering**: Only processes webhook events for resources with `event-source: webhook` or `event-source: both`
2. **Resource type support**: Filters implemented for Deployments, StatefulSets, DaemonSets, and HelmReleases

#### 2. Registry Poller (`src/polling/mod.rs`)
- **Purpose**: Alternative to webhooks - polls registries for new tags and digest changes
- **Configuration**:
  - `HEADWIND_POLLING_ENABLED` - Enable/disable (default: false)
  - `HEADWIND_POLLING_INTERVAL` - Poll interval in seconds (default: 300)
- **Key Functions**:
  - `start()` - Starts polling loop
  - `poll_registries()` - Main polling cycle
  - `get_tracked_images()` - Queries Kubernetes for Deployments with headwind annotations
  - `poll_image()` - Checks specific image for digest changes and new tags
  - `check_for_new_tags()` - Lists available tags and finds best match using PolicyEngine
- **Metrics**:
  - `POLLING_CYCLES_TOTAL` - Total poll cycles
  - `POLLING_IMAGES_CHECKED` - Images checked
  - `POLLING_NEW_TAGS_FOUND` - New tags discovered

**Current State**:
- ✅ Framework in place
- ✅ Configuration support
- ✅ Metrics tracking
- ✅ OCI registry integration (tag listing and manifest fetching)
- ✅ Kubernetes resource discovery (queries Deployments with annotations)
- ✅ Dual detection: digest changes (same-tag updates) + new version discovery
- ✅ PolicyEngine integration for semver-aware tag selection
- ✅ Sends events through webhook channel for processing

**Features**:
1. **Digest-based detection**: Detects when images are rebuilt and pushed to the same tag
2. **Version discovery**: Lists all tags and finds the best match based on update policy
3. **Smart filtering**: Skips non-version tags for semver policies
4. **Event-source filtering**: Only polls resources with `event-source: polling` or `event-source: both` (via POLLING_RESOURCES_FILTERED metric)
5. **Per-resource polling intervals**: Respects `headwind.sh/polling-interval` annotation to override global interval per-resource
6. **Deduplication**: Tracks unique image+policy combinations to avoid redundant checks
7. **Caching**: Maintains in-memory cache of last seen tag+digest per image and last poll time per resource

**Private Registry Authentication**: ✅ Fully supported via Kubernetes imagePullSecrets. Reads credentials from ServiceAccount and uses them for registry API calls. Supports Docker Hub, ECR, GCR, ACR, Harbor, GHCR, and GitLab registries.

**Helm Chart Polling**: ✅ Fully supported for both OCI and HTTP/HTTPS Helm repositories. Polling discovers HelmReleases with headwind annotations, queries the referenced HelmRepository for available versions, applies policy engine for version selection, and creates UpdateRequests when new versions are found. Supports both traditional HTTP repos (index.yaml parsing) and OCI registries (tag listing). Controlled by same `HEADWIND_POLLING_ENABLED` environment variable.

#### 3. Policy Engine (`src/policy/mod.rs`)
- **Purpose**: Determines if an update should happen based on semantic versioning
- **Policies**:
  - `patch` - Only 1.2.3 → 1.2.4
  - `minor` - Only 1.2.3 → 1.3.0
  - `major` - Any 1.2.3 → 2.0.0
  - `all` - Any new version
  - `glob` - Pattern matching (e.g., `v1.*-stable`)
  - `force` - Always update
  - `none` - Never update (default)
- **Key Functions**:
  - `should_update()` - Main decision function
  - `check_semver_policy()` - Semver comparison logic
  - `parse_version()` - Handles `v` prefix and other common patterns

**Tests**: Well covered in `src/policy/mod.rs` tests module
**Status**: ✅ **FULLY FUNCTIONAL** - Used by both webhook processing and registry polling

#### 4. Approval System (`src/approval/mod.rs`)
- **Port**: 8081
- **Purpose**: HTTP API for managing update approvals and executing approved updates
- **Endpoints**:
  - `GET /api/v1/updates` - List all UpdateRequest CRDs across all namespaces
  - `GET /api/v1/updates/{namespace}/{name}` - Get specific UpdateRequest
  - `POST /api/v1/updates/{namespace}/{name}/approve` - Approve and execute an update
  - `POST /api/v1/updates/{namespace}/{name}/reject` - Reject an update with reason
  - `GET /health` - Health check
- **Storage**: Kubernetes UpdateRequest CRDs (persistent via Kubernetes API)
- **Key Types**:
  - `UpdateRequest` - CRD representing a pending update
  - `ApprovalRequest` - Approval/rejection payload
  - `UpdatePhase` - Pending, Completed, Rejected, Failed

**Current State**:
- ✅ Full CRUD operations on UpdateRequest CRDs
- ✅ Approval workflow with approver tracking
- ✅ Rejection workflow with reason tracking
- ✅ Automatic update execution on approval
- ✅ Status tracking with timestamps (approved_at, rejected_at, last_updated)
- ✅ Error handling and reporting in UpdateRequest status

**Key Functions**:
  - `execute_update()` - Validates and applies approved updates to Deployments
  - `approve_update()` - Approves request, executes update, updates CRD status
  - `reject_update()` - Rejects request with reason, updates CRD status

#### 5. Kubernetes Controllers (`src/controller/`)
Headwind includes dedicated controllers for different Kubernetes workload types:

##### Deployment Controller (`src/controller/deployment.rs`)
- **Purpose**: Watches Deployments, processes image update events, and creates UpdateRequests
- **Key Functions**:
  - `reconcile()` - Main reconciliation loop for Deployment changes
  - `parse_policy_from_annotations()` - Reads Headwind annotations
  - `update_deployment_image()` - Updates container image in Deployment spec
  - `process_webhook_events()` - Processes image push events from webhooks/polling
  - `handle_image_event()` - Matches events to Deployments and creates UpdateRequests
  - `find_matching_deployments()` - Queries Deployments that use specific image
  - `extract_images_from_deployment()` - Gets all container images from Deployment

**Status**: ✅ **FULLY FUNCTIONAL** - Complete end-to-end workflow operational

##### StatefulSet Controller (`src/controller/statefulset.rs`)
- **Purpose**: Watches StatefulSets for stateful applications requiring persistent storage and stable network identity
- **Key Functions**:
  - `reconcile()` - Main reconciliation loop for StatefulSet changes
  - `parse_policy_from_annotations()` - Reads Headwind annotations from StatefulSet
  - `update_statefulset_image()` - Updates container image in StatefulSet spec
  - `update_statefulset_image_with_tracking()` - Updates with approval tracking
  - `extract_images_from_statefulset()` - Gets all container images from StatefulSet
- **Metrics**:
  - `STATEFULSETS_WATCHED` - Gauge of StatefulSets being monitored
- **Tests**: 4 unit tests covering image parsing, policy parsing, and glob matching

**Status**: ✅ **FULLY FUNCTIONAL** - Complete StatefulSet update workflow operational

##### DaemonSet Controller (`src/controller/daemonset.rs`)
- **Purpose**: Watches DaemonSets for per-node applications (logging, monitoring, network agents)
- **Key Functions**:
  - `reconcile()` - Main reconciliation loop for DaemonSet changes
  - `parse_policy_from_annotations()` - Reads Headwind annotations from DaemonSet
  - `update_daemonset_image()` - Updates container image in DaemonSet spec
  - `update_daemonset_image_with_tracking()` - Updates with approval tracking
  - `extract_images_from_daemonset()` - Gets all container images from DaemonSet
- **Metrics**:
  - `DAEMONSETS_WATCHED` - Gauge of DaemonSets being monitored
- **Tests**: 4 unit tests covering image parsing, policy parsing, and glob matching

**Status**: ✅ **FULLY FUNCTIONAL** - Complete DaemonSet update workflow operational

##### Common Annotations (All Controllers)
All workload controllers support the same set of Headwind annotations:
  - `headwind.sh/policy` - Update policy (patch, minor, major, all, glob, force, none)
  - `headwind.sh/pattern` - Glob pattern (for glob policy)
  - `headwind.sh/require-approval` - Boolean, default true
  - `headwind.sh/min-update-interval` - Minimum seconds between updates (default: 300)
  - `headwind.sh/last-update` - RFC3339 timestamp of last update (managed by Headwind)
  - `headwind.sh/images` - Comma-separated list of images to track
  - `headwind.sh/auto-rollback` - Enable automatic rollback on failures
  - `headwind.sh/rollback-timeout` - Health check monitoring duration
  - `headwind.sh/health-check-retries` - Failed health checks before rollback

**Implementation Pattern**: Each controller follows the same architecture:
- Watches resources using kube-rs Controller runtime
- Parses annotations to build ResourcePolicy
- Creates UpdateRequest CRDs for approval workflow
- Directly applies updates when approval not required
- Respects minimum update interval
- Updates metrics and sends notifications

**Current State**:
- ✅ All three controllers watch their respective resources
- ✅ Parse annotations and build ResourcePolicy
- ✅ Process webhook and polling events
- ✅ Create UpdateRequest CRDs for approval workflow
- ✅ Apply updates directly when approval not required
- ✅ Respect minimum update interval
- ✅ Handle both namespaced and all-namespace queries
- ✅ Deduplicate UpdateRequests to avoid spam

#### 6. Helm Controller (`src/controller/helm.rs`)
- **Purpose**: Watches Flux HelmRelease CRDs, automatically discovers new chart versions, and manages chart updates
- **Key Functions**:
  - `reconcile()` - Main reconciliation loop for HelmRelease changes
  - `parse_policy_from_annotations()` - Reads Headwind annotations from HelmRelease
  - `build_resource_policy()` - Constructs ResourcePolicy from annotations
  - `create_update_request()` - Creates and persists UpdateRequest CRD for Helm chart updates
  - `update_helm_releases_count()` - Updates metrics gauge
  - `check_for_chart_updates()` - Automatically queries Helm repositories for new versions
  - `get_helm_repository()` - Fetches HelmRepository CRD referenced by HelmRelease
- **Annotations Used**:
  - `headwind.sh/policy` - Update policy
  - `headwind.sh/pattern` - Glob pattern (for glob policy)
  - `headwind.sh/require-approval` - Boolean, default true
  - `headwind.sh/min-update-interval` - Minimum seconds between updates (default: 300)
- **Metrics**:
  - `HELM_RELEASES_WATCHED` - Gauge of HelmReleases being monitored
  - `HELM_CHART_VERSIONS_CHECKED` - Counter of version checks performed
  - `HELM_UPDATES_FOUND` - Counter of updates discovered
  - `HELM_UPDATES_APPROVED` - Counter of updates approved by policy
  - `HELM_UPDATES_REJECTED` - Counter of updates rejected by policy
  - `HELM_UPDATES_APPLIED` - Counter of chart updates successfully applied
  - `HELM_REPOSITORY_QUERIES` - Counter of repository queries performed
  - `HELM_REPOSITORY_ERRORS` - Counter of repository query errors

**Current State**:
- ✅ Watches all HelmRelease CRDs (Flux CD v2)
- ✅ Parses annotations and builds ResourcePolicy
- ✅ Automatically queries HTTP Helm repositories for available chart versions
- ✅ Supports OCI registries (with known limitations - see README)
- ✅ Uses PolicyEngine for semantic version validation
- ✅ Creates and persists UpdateRequest CRDs to Kubernetes
- ✅ Executes chart updates via approval API (JSON patch on spec.chart.spec.version)
- ✅ Sends notifications with resource kind differentiation
- ✅ Full metrics tracking
- ✅ Private repository authentication via secretRef
- ✅ Registry polling for Helm charts (both OCI and HTTP repositories)

**Repository Support**:
- **HTTP Helm Repositories**: ✅ Fully supported (parses index.yaml, semantic versioning)
- **OCI Registries**: ⚠️ Supported with limitations (oci-distribution crate v0.11 issue with common chart names)

**Integration Points**:
- `src/helm/http.rs` - HTTP repository client (index.yaml parsing)
- `src/helm/oci.rs` - OCI registry client (tag listing via oci-distribution crate)
- `src/approval/mod.rs` - Update execution via `execute_helmrelease_update()` function

**Status**: ✅ **FULLY FUNCTIONAL** - Complete Helm chart auto-discovery and update workflow operational

#### 7. Web UI (`src/ui/`)
- **Port**: 8082
- **Purpose**: Web-based dashboard for viewing and managing UpdateRequests
- **Tech Stack**:
  - **Templating**: Maud 0.27 (Rust-based HTML templates, compile-time type safety)
  - **CSS Framework**: DaisyUI + Tailwind CSS
  - **Interactivity**: HTMX 1.9.10 (hypermedia-driven, minimal JavaScript)
  - **Server**: Axum 0.8

**Key Files**:
- `src/ui/mod.rs` - Router and server initialization
- `src/ui/routes.rs` - Route handlers (dashboard, detail, health)
- `src/ui/templates.rs` - Maud templates with filtering/sorting/pagination
- `src/ui/auth.rs` - Multi-mode authentication and audit logging
- `src/static/css/custom.css` - Custom styles for status badges and UI elements

**Routes**:
- `GET /` - Dashboard view (all UpdateRequests)
- `GET /updates/{namespace}/{name}` - Detail view for specific UpdateRequest
- `GET /health` - Health check endpoint

**Features**:
- **Dashboard**: List all pending and completed UpdateRequests across namespaces
- **Filtering**:
  - Real-time search by resource name or image
  - Filter by namespace (dropdown with unique values)
  - Filter by resource kind (Deployment/StatefulSet/DaemonSet/HelmRelease)
  - Filter by policy type (patch/minor/major/all/glob/none)
- **Sorting**: By date (newest/oldest first), namespace A-Z, resource name A-Z
- **Pagination**: 20 items per page with prev/next buttons
- **Actions**:
  - Approve updates with confirmation dialog (via HTMX POST to approval API)
  - Reject updates with reason modal
  - View detailed information
- **Notifications**: Toast messages for success/error with auto-dismiss (3 seconds)
- **Responsive**: Works on desktop and mobile devices

**Implementation Details**:
- Server-side rendered using Maud templates (type-safe Rust macros)
- Client-side filtering/sorting/pagination via vanilla JavaScript (no framework)
- HTMX handles approve/reject actions without page reload
- Integrates with approval API (port 8081) for update execution
- Data attributes on table rows enable efficient filtering (`data-namespace`, `data-kind`, `data-policy`, `data-created-at`)

**Authentication** (`src/ui/auth.rs`):
Headwind Web UI supports four authentication modes via `HEADWIND_UI_AUTH_MODE` environment variable:

1. **None (default)**: No authentication
   - All actions logged as "web-ui-user"
   - Suitable for development or trusted environments

2. **Simple**: Username from HTTP header
   - Set `HEADWIND_UI_AUTH_MODE=simple`
   - Reads username from `X-User` header
   - Trusts the provided username (requires auth proxy upstream)
   - Use case: Basic auth proxy (e.g., nginx with auth_request)

3. **Token**: Kubernetes TokenReview validation
   - Set `HEADWIND_UI_AUTH_MODE=token`
   - Validates bearer tokens via Kubernetes TokenReview API
   - Extracts authenticated username from token
   - Requires RBAC permission: `authentication.k8s.io/tokenreviews` create
   - Use case: Service account tokens, kubectl authentication
   - Example: `curl -H "Authorization: Bearer $(cat token.txt)" http://localhost:8082/`

4. **Proxy**: Ingress/proxy authentication headers
   - Set `HEADWIND_UI_AUTH_MODE=proxy`
   - Reads username from configurable header (default: `X-Forwarded-User`)
   - Configure header name via `HEADWIND_UI_PROXY_HEADER` environment variable
   - Use case: Kubernetes ingress with external auth (e.g., oauth2-proxy, Authelia)

**Audit Logging**:
- All approval/rejection actions logged with username, action, resource details, timestamp
- Dedicated log target: `headwind::audit` (structured JSON logging)
- Audit log fields: `timestamp`, `username`, `action`, `resource_type`, `namespace`, `name`, `result`, `reason`
- Example: `{"timestamp":"2025-11-08T23:00:00Z","username":"alice","action":"approve","resource_type":"Deployment","namespace":"default","name":"test-approval-nginx-1-28-0","result":"success"}`

**Technical Implementation**:
- Uses Axum 0.8 native async traits (`FromRequestParts` trait) without `#[async_trait]` macro
- `UserIdentity` extractor automatically handles authentication based on configured mode
- TokenReview integration validates Kubernetes tokens and extracts service account usernames
- All route handlers accept `UserIdentity` parameter for automatic user extraction

**Status**: ✅ **FULLY FUNCTIONAL** - Complete web interface with filtering, sorting, pagination, actions, and multi-mode authentication

#### 8. Metrics (`src/metrics/mod.rs`)
- **Port**: 9090
- **Purpose**: Prometheus metrics and health checks
- **Metrics Available**:
  - `headwind_webhook_events_total` - Counter
  - `headwind_webhook_events_processed` - Counter
  - `headwind_updates_pending` - Gauge
  - `headwind_updates_approved_total` - Counter
  - `headwind_updates_rejected_total` - Counter
  - `headwind_updates_applied_total` - Counter
  - `headwind_updates_failed_total` - Counter
  - `headwind_updates_skipped_interval_total` - Counter (updates skipped due to min interval)
  - `headwind_reconcile_duration_seconds` - Histogram
  - `headwind_reconcile_errors_total` - Counter
  - `headwind_deployments_watched` - Gauge
  - `headwind_statefulsets_watched` - Gauge
  - `headwind_daemonsets_watched` - Gauge
  - `headwind_helm_releases_watched` - Gauge
  - `headwind_helm_chart_versions_checked_total` - Counter
  - `headwind_helm_updates_found_total` - Counter
  - `headwind_helm_updates_approved_total` - Counter
  - `headwind_helm_updates_rejected_total` - Counter
  - `headwind_helm_updates_applied_total` - Counter
  - `headwind_helm_repository_queries_total` - Counter
  - `headwind_helm_repository_errors_total` - Counter
  - `headwind_helm_repository_query_duration_seconds` - Histogram
  - `headwind_rollbacks_total` - Counter (all rollback operations)
  - `headwind_rollbacks_manual_total` - Counter (manual rollbacks)
  - `headwind_rollbacks_automatic_total` - Counter (automatic rollbacks)
  - `headwind_rollbacks_failed_total` - Counter (failed rollbacks)
  - `headwind_deployment_health_checks_total` - Counter
  - `headwind_deployment_health_failures_total` - Counter
  - `headwind_notifications_sent_total` - Counter
  - `headwind_notifications_failed_total` - Counter
  - `headwind_notifications_slack_sent_total` - Counter
  - `headwind_notifications_teams_sent_total` - Counter
  - `headwind_notifications_webhook_sent_total` - Counter
  - `headwind_polling_cycles_total` - Counter
  - `headwind_polling_errors_total` - Counter
  - `headwind_polling_images_checked_total` - Counter
  - `headwind_polling_new_tags_found_total` - Counter
  - `headwind_polling_helm_charts_checked_total` - Counter
  - `headwind_polling_helm_new_versions_found_total` - Counter

**Important**: Remember to increment metrics when implementing new features!

#### 9. Helm Chart (`charts/headwind/`)

**Purpose**: Official Helm chart for deploying Headwind to Kubernetes clusters

**Status**: ✅ **PRODUCTION READY** - Full chart with InfluxDB observability integration

**Key Files**:
- `Chart.yaml` - Chart metadata and version
- `values.yaml` - Configuration parameters and defaults
- `README.md` - Comprehensive user documentation
- `templates/` - Kubernetes resource templates

**Important Templates**:
- `deployment.yaml` - Main Headwind deployment with optional Telegraf sidecar
- `service.yaml` - Multi-port service (webhook:8080, api:8081, ui:8082, metrics:9090)
- `ingress.yaml` - Optional ingress with multi-backend support
- `configmap.yaml` - Configuration including InfluxDB token injection
- `influxdb-statefulset.yaml` - Optional InfluxDB deployment
- `influxdb-secret.yaml` - InfluxDB credentials with token persistence
- `rbac.yaml` - ServiceAccount, ClusterRole, ClusterRoleBinding

**Helm Repository**: `https://headwind.sh/charts`

**Installation**:
```bash
helm repo add headwind https://headwind.sh/charts
helm install headwind headwind/headwind -n headwind-system --create-namespace
```

**Publishing**: Automated via `.github/workflows/helm-release.yml`
- Triggers on changes to `charts/headwind/**` in main branch
- Uses `helm/chart-releaser-action` to package and publish
- Creates GitHub releases with chart `.tgz` files
- Updates `charts/index.yaml` in `gh-pages` branch
- Served at `https://headwind.sh/charts/index.yaml`

**Key Features**:
1. **Token Persistence**: InfluxDB tokens persist across helm upgrades using `lookup` function
2. **ConfigMap Integration**: Tokens automatically injected into ConfigMap for Rust code access
3. **Resource Policy**: Secrets marked with `helm.sh/resource-policy: keep` for safety
4. **Multi-backend Ingress**: Supports separate hosts for UI, webhook, and metrics
5. **Observability Stack**: Optional integrated InfluxDB + Telegraf deployment

**Configuration Examples**:
```yaml
# Enable observability
observability:
  create: true
  influxdb:
    enabled: true
    storageSize: 100Gi

telegraf:
  enabled: true

# Enable ingress
ingress:
  enabled: true
  className: nginx
  hosts:
    - host: headwind.example.com
      paths:
        - path: /
          pathType: Prefix
          backend: ui

# Enable notifications
notifications:
  slack:
    enabled: true
    webhookUrl: "https://hooks.slack.com/services/..."
```

**Critical Implementation Details**:

1. **InfluxDB Token Management** (`templates/influxdb-secret.yaml`):
   - Uses `lookup` function to check for existing secret
   - Generates random 64-char token on first install
   - Reuses existing token on upgrades (prevents regeneration)
   - Adds `helm.sh/resource-policy: keep` annotation

2. **ConfigMap Token Injection** (`templates/configmap.yaml`):
   - Reads token from InfluxDB secret using `lookup`
   - Injects as `observability.influxdb.token` key
   - Enables Rust code to read token from ConfigMap
   - Fixes 401 unauthorized errors

**See Also**:
- `charts/headwind/README.md` - User documentation
- `HELM_REPO.md` - Maintainer guide for publishing charts
- `.github/cr.yaml` - Chart releaser configuration

#### 10. Observability Stack

**Purpose**: Integrated metrics collection, storage, and visualization

**Status**: ✅ **FULLY FUNCTIONAL** - InfluxDB integration complete

**Components**:
1. **InfluxDB 2.7** - Time-series database for metrics storage
2. **Telegraf Sidecar** - Scrapes Prometheus metrics and writes to InfluxDB
3. **Observability Dashboard** (`src/ui/routes.rs`) - Real-time visualization

**Deployment** (via Helm):
```yaml
observability:
  create: true
  influxdb:
    enabled: true
    version: "2.7"
    retentionHours: 720  # 30 days
    storageSize: 100Gi
    organization: "headwind"
    bucket: "metrics"
    adminUser: "admin"
    adminPassword: ""  # Auto-generated

telegraf:
  enabled: true
  resources:
    limits:
      cpu: 100m
      memory: 128Mi
```

**InfluxDB Configuration**:
- **Automatic Setup**: Organization, bucket, and admin user created on first start
- **Token Management**: Admin token auto-generated and persisted
- **Persistent Storage**: Uses PersistentVolumeClaim for data retention
- **Retention Policy**: Configurable retention period (default: 30 days)

**Telegraf Configuration**:
- **Input**: Prometheus scraper (http://localhost:9090/metrics)
- **Output**: InfluxDB v2 (http://headwind-influxdb:8086)
- **Flush Interval**: 10 seconds
- **Batch Size**: 1000 metrics

**Metrics Flow**:
```
Headwind Rust App
  → Prometheus Metrics (port 9090)
    → Telegraf Sidecar
      → InfluxDB
        → Observability Dashboard (port 8082/observability)
```

**Dashboard Features**:
- **Multi-backend Support**: Auto-detects Prometheus, VictoriaMetrics, or InfluxDB
- **Real-time Metrics**: Updates every 30 seconds
- **Time-series Charts**: Historical trends for all metrics
- **Metric Cards**: Current values for key metrics
- **Hot-reload**: Configuration changes picked up automatically

**Configuration Loading** (`src/config/mod.rs`):
- Loads from ConfigMap (`headwind-config`) and Secret (`headwind-secrets`)
- Supports hot-reload via Kubernetes watchers
- InfluxDB token read from `observability.influxdb.token` ConfigMap key
- Falls back to environment variable `HEADWIND_INFLUXDB_TOKEN`

**Troubleshooting**:

1. **401 Unauthorized Errors**:
   - Verify token matches between secret and ConfigMap
   - Check ConfigMap has `observability.influxdb.token` key
   - Restart Headwind deployment to reload configuration

2. **Token Mismatch After Upgrade**:
   - Ensure `lookup` function in `influxdb-secret.yaml` is present
   - Delete and recreate InfluxDB StatefulSet if needed
   - Token should persist across helm upgrades

3. **Metrics Not Showing**:
   - Check Telegraf logs for write errors
   - Verify InfluxDB is running and healthy
   - Test direct query: `influx query 'from(bucket: "metrics") |> range(start: -1h)'`

**See Also**:
- `docs/docs/configuration/observability.md` - User documentation
- `charts/headwind/templates/influxdb-*.yaml` - InfluxDB templates
- `src/metrics/client.rs` - InfluxDB query client

### Data Models (`src/models/`)

#### Policy Models (`models/policy.rs`)
- `UpdatePolicy` enum - The 7 policy types
- `EventSource` enum - 4 event source types (webhook, polling, both, none)
- `ResourcePolicy` struct - Full policy configuration including event source and polling interval
- `annotations` module - Annotation key constants including `EVENT_SOURCE` and `POLLING_INTERVAL`

#### Update Models (`models/update.rs`)
- `UpdateRequest` - Pending update request
- `UpdateStatus` - Status enum
- `ResourceKind` - Deployment, StatefulSet, DaemonSet, HelmRelease
- `ApprovalRequest` - Approval payload
- `UpdateEvent` - Event tracking (unused currently)

#### Webhook Models (`models/webhook.rs`)
- `RegistryWebhook` - Generic OCI registry format
- `DockerHubWebhook` - Docker Hub specific format
- `ImagePushEvent` - Normalized internal format

## ~~Critical Implementation Gaps~~ Implementation Status

### ✅ 1. **Webhook Events Connected to Controller** (COMPLETED)
Webhook and polling events are now fully connected to the controller:

```rust
// In src/controller/deployment.rs::process_webhook_events()
// ✅ Queries Kubernetes for Deployments with headwind annotations
// ✅ Matches events to Deployments using the image
// ✅ Extracts current and new versions
// ✅ Uses PolicyEngine to validate update policy
// ✅ Creates UpdateRequest CRD if approval required
// ✅ Applies update directly if no approval needed
// ✅ Respects minimum update interval
```

### ✅ 2. **Update Application Implemented** (COMPLETED)
Updates are now fully functional:

```rust
// In src/controller/deployment.rs & src/approval/mod.rs
// ✅ update_deployment_image() updates container image in Deployment
// ✅ Metrics updated (UPDATES_APPLIED, UPDATES_FAILED)
// ✅ Last update timestamp tracked in UpdateRequest CRD
// ✅ Error handling and status reporting
// ✅ Approval workflow executes updates via approval API
```

### ✅ 3. **State Sharing Between Components** (COMPLETED)
Components now communicate via:
- ✅ Tokio channels for webhook/polling events
- ✅ Kubernetes API for UpdateRequest CRDs (shared state)
- ✅ Direct Kubernetes API access (no shared state needed)

### ✅ 4. **Helm Support** (COMPLETED)
Full Helm chart auto-discovery and update workflow implemented:

```rust
// In src/controller/helm.rs
// ✅ Watches HelmRelease CRDs (Flux CD v2 API)
// ✅ Automatically queries Helm repositories for available chart versions
// ✅ Supports both HTTP Helm repositories and OCI registries
// ✅ Uses PolicyEngine for semver-aware version selection
// ✅ Creates and persists UpdateRequest CRDs to Kubernetes
// ✅ Private repository authentication via secretRef
// ✅ Full metrics and notification integration

// In src/approval/mod.rs::execute_helmrelease_update()
// ✅ Applies chart updates via JSON merge patch
// ✅ Updates spec.chart.spec.version on approval
// ✅ Full error handling and status reporting
// ✅ HELM_UPDATES_APPLIED metric tracking
```

**Implementation Modules**:
- `src/helm/http.rs` - HTTP Helm repository client (parses index.yaml)
- `src/helm/oci.rs` - OCI registry client (uses oci-distribution crate)
- `src/controller/helm.rs` - HelmRelease controller with auto-discovery
- `src/approval/mod.rs` - Update execution for HelmReleases

**Known Limitation**: OCI Helm repositories may query Docker Hub when chart names match common Docker images (oci-distribution crate v0.11 issue). HTTP repositories work perfectly.

## Development Guidelines

### Running Locally

```bash
# Requires a Kubernetes cluster accessible via KUBECONFIG
export RUST_LOG=headwind=debug,kube=debug
cargo run
```

The operator will:
1. Start metrics server on :9090
2. Start webhook server on :8080
3. Start approval API on :8081
4. Begin watching Deployments, StatefulSets, DaemonSets, and HelmReleases

### Testing

```bash
# Run all tests
cargo test

# Run specific test
cargo test policy::tests::test_patch_policy

# Run with output
cargo test -- --nocapture
```

### Code Quality Checks

**IMPORTANT**: Always run these checks before committing:

```bash
# 1. Run tests
cargo test

# 2. Check formatting
cargo fmt --all -- --check

# 3. Run clippy
cargo clippy --all-features --all-targets -- -D warnings

# 4. Check compilation
cargo check --all-features --all-targets
```

Or use pre-commit hooks (recommended):

```bash
# Install pre-commit hooks
pre-commit install

# Run all hooks manually
pre-commit run --all-files
```

The pre-commit hooks will automatically run:
- `cargo fmt` (formatting)
- `cargo clippy` (linting)
- `cargo check` (compilation)
- YAML validation
- Secret detection
- Trailing whitespace removal

### MANDATORY: Test All Changes Before Committing

**CRITICAL IMPERATIVE FOR AI ASSISTANTS**:

Before committing ANY code changes, you MUST:

1. ✅ **Test manually in the local Kubernetes cluster**: Every code change affecting runtime behavior MUST be tested end-to-end with the actual cluster, not just unit tests
2. ✅ **Run all unit tests**: `cargo test` - All tests must pass
3. ✅ **Pass clippy**: `cargo clippy --all-features --all-targets -- -D warnings` - Zero warnings allowed
4. ✅ **Format code**: `cargo fmt --all` - Consistent formatting required
5. ✅ **Run pre-commit checks**: `pre-commit run --all-files` (if hooks are installed)

**Testing Guidelines**:
- For webhook changes: Send test webhook payloads and verify processing
- For controller changes: Deploy test resources and verify reconciliation
- For approval changes: Test approval/rejection workflows via API
- For polling changes: Enable polling and verify detection
- For notification changes: Verify Slack/webhook notifications are sent

**DO NOT** commit code that has only been verified to compile and pass unit tests. Real integration testing in the cluster is mandatory.

### Before Creating a Pull Request

**CRITICAL CHECKLIST**:

1. ✅ **Test manually**: MANDATORY - Test all changes in the local Kubernetes cluster
2. ✅ **Run all tests**: `cargo test`
3. ✅ **Pass clippy**: `cargo clippy --all-features --all-targets -- -D warnings`
4. ✅ **Format code**: `cargo fmt --all`
5. ✅ **Update docs**: MANDATORY - Update ALL documentation to reflect changes
   - **README.md**: User-facing documentation (features, configuration, metrics)
   - **CLAUDE.md**: Architecture documentation (implementation details, design decisions)
   - **Docusaurus docs** (`docs/docs/`): Update or create pages for new features
     - Configuration guides: `docs/docs/configuration/*.md`
     - User guides: `docs/docs/guides/*.md`
     - API reference: `docs/docs/api/*.md`
   - **All docs must be updated BEFORE creating the PR, not after**
   - New features require corresponding documentation pages
   - Changed features require updates to existing pages
6. ✅ **Check metrics**: Ensure new features increment appropriate metrics
7. ✅ **Update metrics documentation**: Add new metrics to `docs/docs/api/metrics.md`
8. ✅ **Run pre-commit checks**: `pre-commit run --all-files` (if hooks are installed)
9. ✅ **GitHub PR metadata**: MANDATORY - Set before creating PR
   - **Assignee**: Assign to yourself or the primary contributor
   - **Labels**: Add appropriate labels (e.g., `enhancement`, `bug`, `documentation`, `priority:high`)
   - **Linked issues**: Use GitHub's "Development" section to link related issues
   - **Closes/Fixes**: Use `Closes #X` or `Fixes #X` in PR description to auto-close issues
   - **Related issues**: Use `Relates to #X` for context without auto-closing
10. ✅ **Reference templates**: Use `.github/PULL_REQUEST_TEMPLATE/pull_request_template.md` format
11. ✅ **PR title**: Use conventional commits format (e.g., `feat:`, `fix:`, `docs:`, `chore:`)

**Testing in Kubernetes**:

```bash
# Apply CRDs and RBAC
kubectl apply -f deploy/k8s/crds/
kubectl apply -f deploy/k8s/namespace.yaml
kubectl apply -f deploy/k8s/rbac.yaml

# Build and run locally
cargo build --release
RUST_LOG=headwind=debug ./target/release/headwind

# Or build Docker image and deploy
docker build -t headwind:test .
kind load docker-image headwind:test  # or minikube
kubectl apply -f deploy/k8s/deployment.yaml
```

### Adding New Features

1. **Add metrics first** - Define in `src/metrics/mod.rs`
2. **Update models** - Add types in `src/models/`
3. **Implement logic** - Add to appropriate module
4. **Add tests** - Unit tests in same file
5. **Test thoroughly** - Run tests and manual testing
6. **Run pre-commit checks** - Ensure code quality
7. **Update docs** - README.md and this file

### Common Patterns

#### Adding a new annotation

```rust
// In src/models/policy.rs::annotations
pub const MY_ANNOTATION: &str = "headwind.sh/my-annotation";

// In src/controller/deployment.rs::parse_policy_from_annotations()
if let Some(value) = annotations.get(annotations::MY_ANNOTATION) {
    policy.my_field = value.parse().unwrap_or(default_value);
}
```

#### Adding a new metric

```rust
// In src/metrics/mod.rs
pub static ref MY_METRIC: IntCounter = IntCounter::new(
    "headwind_my_metric_total",
    "Description of metric"
).unwrap();

// In register_metrics()
REGISTRY.register(Box::new(MY_METRIC.clone())).ok();

// In your code
MY_METRIC.inc();
```

#### Emitting Kubernetes events

```rust
use k8s_openapi::api::core::v1::Event;
use kube::api::{Api, PostParams};

let events: Api<Event> = Api::namespaced(client, &namespace);
// Create and post event
```

## Deployment

### Building Docker Image

```bash
docker build -t headwind:latest .
```

The Dockerfile uses multi-stage builds:
1. Builder stage: Compiles Rust binary
2. Runtime stage: Minimal Debian with binary only

### Deploying to Kubernetes

```bash
kubectl apply -f deploy/k8s/namespace.yaml
kubectl apply -f deploy/k8s/rbac.yaml
kubectl apply -f deploy/k8s/deployment.yaml
kubectl apply -f deploy/k8s/service.yaml
```

### RBAC Permissions Needed

The ServiceAccount needs:
- **deployments**: get, list, watch, update, patch
- **statefulsets**: get, list, watch, update, patch
- **daemonsets**: get, list, watch, update, patch
- **events**: create, patch
- **helmreleases** (Flux CD): get, list, watch, update, patch

## Troubleshooting

### Common Issues

1. **"Failed to create deployment controller"**
   - Check KUBECONFIG is set
   - Verify cluster is accessible
   - Check RBAC permissions

2. **Webhooks not processing**
   - Check logs: `kubectl logs -n headwind-system deployment/headwind`
   - Verify webhook URL is accessible
   - Test with curl to /webhook/dockerhub or /webhook/registry

3. **Metrics not showing**
   - Check Prometheus scrape config
   - Verify port 9090 is accessible
   - Check `/metrics` endpoint directly

### Debugging Tips

```bash
# Watch logs
kubectl logs -n headwind-system -f deployment/headwind

# Check metrics
kubectl port-forward -n headwind-system svc/headwind-metrics 9090:9090
curl localhost:9090/metrics

# Test webhook
kubectl port-forward -n headwind-system svc/headwind-webhook 8080:8080
curl -X POST localhost:8080/webhook/dockerhub \
  -H "Content-Type: application/json" \
  -d '{"push_data":{"tag":"v1.2.3"},"repository":{"repo_name":"myimage"}}'

# Test approval API
kubectl port-forward -n headwind-system svc/headwind-api 8081:8081
curl localhost:8081/api/v1/updates
```

## Design Decisions & Rationale

### Why Rust?
- Memory safety without garbage collection
- Excellent async runtime (Tokio)
- Strong type system prevents bugs
- Great Kubernetes ecosystem (kube-rs)
- Single binary deployment

### Why No Database?
- Keeps deployment simple
- State is in Kubernetes (source of truth)
- Approval requests could be stored as CRDs if persistence needed
- Reduces operational overhead

### Why Annotations over CRDs?
- Lower barrier to entry - works with existing resources
- No additional API types to learn
- Can add CRDs later for advanced features
- Familiar pattern from tools like Flux, Argo

### Why Both Webhooks and Polling?
- **Webhooks are preferred**: Immediate, efficient, event-driven
- **Polling is fallback**: Works when webhooks aren't available
- **Flexibility**: Not all registries support webhooks
- **Compatibility**: Some environments can't expose webhook endpoints
- **User choice**: Let users pick what works for their setup

Default: Polling disabled, webhooks recommended

### Why Three Separate Servers?
- Separation of concerns
- Different security requirements (webhook needs external access)
- Easier to scale independently if needed
- Clear port assignments for monitoring

## Future Enhancements

### ~~High Priority~~ COMPLETED ✅
1. ✅ Complete webhook → controller integration
2. ✅ Implement actual image updates
3. ✅ Persistent approval request storage (CRDs)
4. ✅ Registry polling implementation
5. ✅ Flux HelmRelease support (basic version monitoring)

### ✅ 5. **StatefulSet/DaemonSet Support** (COMPLETED)
Full support for StatefulSet and DaemonSet resources:

```rust
// In src/controller/statefulset.rs
// ✅ Watches StatefulSet resources (Kubernetes apps/v1 API)
// ✅ Parses Headwind annotations (same as Deployments)
// ✅ Uses PolicyEngine for semantic version validation
// ✅ Creates UpdateRequest CRDs for approval workflow
// ✅ Applies updates via strategic merge patch
// ✅ STATEFULSETS_WATCHED metric tracking

// In src/controller/daemonset.rs
// ✅ Watches DaemonSet resources (Kubernetes apps/v1 API)
// ✅ Parses Headwind annotations (same as Deployments)
// ✅ Uses PolicyEngine for semantic version validation
// ✅ Creates UpdateRequest CRDs for approval workflow
// ✅ Applies updates via strategic merge patch
// ✅ DAEMONSETS_WATCHED metric tracking

// In src/approval/mod.rs::execute_update()
// ✅ Routes StatefulSet updates via execute_statefulset_update()
// ✅ Routes DaemonSet updates via execute_daemonset_update()
// ✅ Full error handling and status reporting
```

**Implementation**: All three workload controllers (Deployment, StatefulSet, DaemonSet) follow identical patterns and share the same annotation schema.

### ✅ 6. **Direct HelmRelease Updates** (COMPLETED)
Full support for bypassing approval workflow when `headwind.sh/require-approval: "false"`:

```rust
// In src/controller/helm.rs::reconcile()
// ✅ Checks resource_policy.require_approval flag
// ✅ Creates UpdateRequest CRD when approval is required (true)
// ✅ Performs direct update when approval not required (false)
// ✅ Enforces minimum update interval (headwind.sh/min-update-interval)
// ✅ Updates headwind.sh/last-update annotation after direct updates
// ✅ Increments UPDATES_SKIPPED_INTERVAL metric when throttled
// ✅ Calls update_helmrelease_chart_version() for direct updates

// In src/controller/helm.rs::handle_chart_event()
// ✅ Same direct update logic for webhook/polling events
// ✅ Feature parity between reconcile() and event handling
```

**Implementation Details**:
- **Direct update path**: When `require-approval: "false"`, the reconcile function checks the minimum update interval, then calls `update_helmrelease_chart_version()` and updates the last-update annotation
- **Minimum interval enforcement**: Uses `headwind.sh/last-update` and `headwind.sh/min-update-interval` annotations to prevent update spam
- **Repository type support**: Tested and working with both OCI and HTTP Helm repositories
- **Metrics tracking**: Increments `UPDATES_SKIPPED_INTERVAL` when updates are throttled

**Testing**:
- ✅ OCI Helm charts: busybox-direct (1.0.0 → 1.37.0) via jfrog-oci repository
- ✅ HTTP Helm charts: busybox-http-direct (1.0.0 → 1.1.0) via jfrog-http repository
- ✅ Last-update annotation correctly set after both update types
- ✅ Minimum interval enforcement verified

**Bug Fix**: Issue #49 - reconcile() function was missing direct update logic that handle_chart_event() already had. Now both code paths have identical behavior.

### High Priority (Next)
1. Add integration tests with real cluster
2. Comprehensive end-to-end testing
3. CI/CD pipeline for automated testing
4. Wiki documentation (#36)

### Medium Priority
1. Web UI for approvals
2. Advanced Slack/Teams notification features
3. Rollback functionality enhancements
4. Automatic rollback on deployment failures

### Low Priority
1. Multi-cluster support
2. Custom update strategies
3. Canary deployments
4. A/B testing integration
5. Rate limiting per namespace
6. Advanced scheduling (maintenance windows, etc.)

## References

- [kube-rs docs](https://docs.rs/kube/latest/kube/)
- [kube-runtime controller guide](https://docs.rs/kube-runtime/latest/kube_runtime/controller/)
- [Kubernetes Operator Pattern](https://kubernetes.io/docs/concepts/extend-kubernetes/operator/)
- [Semantic Versioning](https://semver.org/)
- [OCI Distribution Spec](https://github.com/opencontainers/distribution-spec)

## Questions for Future Developers

If you're stuck, consider:

1. **Where should this logic go?**
   - Image parsing → models or controller
   - Version comparison → policy
   - K8s updates → controller
   - HTTP endpoints → approval or webhook
   - Monitoring → metrics

2. **Do I need to update metrics?**
   - Yes, always add metrics for new operations

3. **Should this be configurable?**
   - If yes, add annotation or environment variable
   - Document in README

4. **What could go wrong?**
   - Add error handling
   - Log appropriately
   - Increment error metrics

## Repository Organization

### Directory Structure

```
headwind/
├── .github/              # GitHub Actions workflows and issue/PR templates
│   └── workflows/        # CI/CD workflows (tests, linting, docs deployment)
├── deploy/               # Kubernetes deployment manifests
│   └── k8s/             # K8s YAML files (CRDs, RBAC, deployments, services)
│       ├── crds/        # Custom Resource Definitions
│       └── ...
├── docs/                 # Docusaurus documentation site
│   ├── docs/            # Documentation markdown files
│   │   ├── configuration/  # Configuration guides
│   │   ├── guides/         # User guides
│   │   └── api/            # API reference
│   ├── src/             # Custom React components
│   └── static/          # Static assets (images, CNAME)
├── examples/             # Example configurations and test files
│   ├── test-manifests/  # Test YAML files for manual testing
│   ├── scripts/         # Test scripts (webhooks, notifications)
│   └── *.yaml           # Production-ready example configurations
├── scripts/              # Development and build scripts
├── src/                  # Rust source code
│   ├── controller/      # Kubernetes controllers
│   ├── webhook/         # Webhook server
│   ├── approval/        # Approval API
│   ├── policy/          # Policy engine
│   ├── models/          # Data models
│   ├── metrics/         # Prometheus metrics
│   ├── helm/            # Helm repository clients
│   └── ...
├── tests/                # Integration tests
├── target/               # Rust build artifacts (gitignored)
├── CLAUDE.md             # Architecture and development context (for AI assistants)
├── README.md             # User-facing documentation
├── CONTRIBUTING.md       # Contribution guidelines
├── KUBECTL_PLUGIN.md     # kubectl plugin documentation
├── Cargo.toml            # Rust dependencies
├── Dockerfile            # Container image build
└── Makefile              # Build and development commands
```

### File Organization Guidelines

**Test Files:**
- All test manifests go in `examples/test-manifests/`
- Test scripts go in `examples/scripts/`
- Never commit test files to the root directory

**Documentation:**
- User guides → `docs/docs/guides/`
- Configuration docs → `docs/docs/configuration/`
- API reference → `docs/docs/api/`
- Architecture → `CLAUDE.md`
- Quick start → `README.md`

**Examples:**
- Production-ready examples → `examples/*.yaml`
- Test/development examples → `examples/test-manifests/`

**Deployment:**
- All Kubernetes manifests → `deploy/k8s/`
- CRDs → `deploy/k8s/crds/`

## Contact & Support

For questions about this codebase, open an issue on GitHub with the `question` label.

---

Last Updated: 2025-11-10
Version: 0.1.0 (Production-ready Helm chart with InfluxDB observability, automated publishing to headwind.sh/charts)
