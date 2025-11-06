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
4. **Deduplication**: Tracks unique image+policy combinations to avoid redundant checks
5. **Caching**: Maintains in-memory cache of last seen tag+digest per image

**Note**: Currently uses anonymous registry access. Private registry authentication support is planned.

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

#### 5. Kubernetes Controller (`src/controller/deployment.rs`)
- **Purpose**: Watches Deployments, processes image update events, and creates UpdateRequests
- **Key Functions**:
  - `reconcile()` - Main reconciliation loop for Deployment changes
  - `parse_policy_from_annotations()` - Reads Headwind annotations
  - `update_deployment_image()` - Updates container image in Deployment spec
  - `process_webhook_events()` - Processes image push events from webhooks/polling
  - `handle_image_event()` - Matches events to Deployments and creates UpdateRequests
  - `find_matching_deployments()` - Queries Deployments that use specific image
  - `extract_images_from_deployment()` - Gets all container images from Deployment
- **Annotations Used**:
  - `headwind.sh/policy` - Update policy
  - `headwind.sh/pattern` - Glob pattern (for glob policy)
  - `headwind.sh/require-approval` - Boolean, default true
  - `headwind.sh/min-update-interval` - Seconds between updates
  - `headwind.sh/images` - Comma-separated list of images to track

**Current State**:
- ✅ Watches all Deployments
- ✅ Parses annotations and builds ResourcePolicy
- ✅ Processes webhook and polling events
- ✅ Creates UpdateRequest CRDs for approval workflow
- ✅ Directly applies updates when approval not required
- ✅ Respects minimum update interval
- ✅ Handles both namespaced and all-namespace queries
- ✅ Deduplicates UpdateRequests to avoid spam

**Status**: ✅ **FULLY FUNCTIONAL** - Complete end-to-end workflow operational

#### 6. Metrics (`src/metrics/mod.rs`)
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
  - `headwind_reconcile_duration_seconds` - Histogram
  - `headwind_reconcile_errors_total` - Counter
  - `headwind_deployments_watched` - Gauge
  - `headwind_helm_releases_watched` - Gauge

**Important**: Remember to increment metrics when implementing new features!

### Data Models (`src/models/`)

#### Policy Models (`models/policy.rs`)
- `UpdatePolicy` enum - The 7 policy types
- `ResourcePolicy` struct - Full policy configuration
- `annotations` module - Annotation key constants

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

### 4. **Helm Support** (Low Priority)
Stub exists in `src/controller/helm.rs`. Would need:
- Watch HelmRelease CRDs (Flux CD style)
- Query Helm chart repositories for new versions
- Update HelmRelease spec when new version available

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
4. Begin watching Deployments

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

### Before Creating a Pull Request

**CRITICAL CHECKLIST**:

1. ✅ **Run all tests**: `cargo test`
2. ✅ **Pass clippy**: `cargo clippy --all-features --all-targets -- -D warnings`
3. ✅ **Format code**: `cargo fmt --all`
4. ✅ **Test manually**: If possible, test changes in a real Kubernetes cluster
5. ✅ **Update docs**: Update README.md, CLAUDE.md, or inline documentation as needed
6. ✅ **Check metrics**: Ensure new features increment appropriate metrics
7. ✅ **Run pre-commit checks**: `pre-commit run --all-files` (if hooks are installed)

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

### High Priority (Next)
1. Add integration tests with real cluster
2. Private registry authentication support
3. Comprehensive end-to-end testing
4. CI/CD pipeline for automated testing

### Medium Priority
1. Helm Release support
2. StatefulSet/DaemonSet support
3. Web UI for approvals
4. Slack/Teams notifications
5. Rollback functionality
6. Automatic rollback on deployment failures

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

## Contact & Support

For questions about this codebase, open an issue on GitHub with the `question` label.

---

Last Updated: 2025-11-06
Version: 0.2.0-alpha (Core functionality complete, awaiting integration tests)
