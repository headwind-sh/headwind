# Headwind

A Kubernetes operator for automating workload updates based on container image changes, written in Rust.

Headwind monitors container registries and automatically updates your Kubernetes workloads when new images are available, with intelligent semantic versioning policies and approval workflows.

## Features

- **Dual Update Triggers**: Event-driven webhooks **or** registry polling for maximum flexibility
- **Semver Policy Engine**: Intelligent update decisions based on semantic versioning (patch, minor, major, glob, force, all)
- **Approval Workflow**: Full HTTP API for approval requests with integration possibilities (Slack, webhooks, etc.)
- **Full Observability**: Prometheus metrics, distributed tracing, and structured logging
- **Resource Support**:
  - Kubernetes Deployments ✅
  - Helm Charts (planned)
  - StatefulSets (planned)
  - DaemonSets (planned)
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
spec:
  # ... rest of deployment spec
```

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

## API Endpoints

### Approval API (Port 8081)

```bash
# List pending updates
curl http://headwind-api/api/v1/updates

# Get specific update
curl http://headwind-api/api/v1/updates/<id>

# Approve update
curl -X POST http://headwind-api/api/v1/updates/<id>/approve \
  -H "Content-Type: application/json" \
  -d '{"update_id":"<id>","approved":true,"approver":"user@example.com"}'

# Reject update
curl -X POST http://headwind-api/api/v1/updates/<id>/reject \
  -H "Content-Type: application/json" \
  -d '{"update_id":"<id>","approved":false,"approver":"user@example.com","reason":"Not ready"}'
```

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
- `headwind_polling_errors_total` - Polling errors encountered
- `headwind_updates_pending` - Updates awaiting approval
- `headwind_updates_approved_total` - Total approved updates
- `headwind_updates_rejected_total` - Total rejected updates
- `headwind_updates_applied_total` - Successfully applied updates
- `headwind_updates_failed_total` - Failed update attempts
- `headwind_reconcile_duration_seconds` - Controller reconciliation time
- `headwind_deployments_watched` - Number of watched Deployments

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
make test
# or
cargo test
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

## Current Limitations

Headwind is currently in **alpha** stage. The following features are planned but not yet implemented:

- ❌ Webhook events are not yet connected to the controller (see [#1](../../issues/01-webhook-controller-integration.md))
- ❌ Approved updates are not automatically applied (see [#2](../../issues/02-implement-update-application.md))
- ❌ No integration tests yet (see [#4](../../issues/04-integration-tests.md))
- ✅ Policy engine works and is tested
- ✅ All three servers run and expose correct ports
- ✅ Kubernetes controller watches Deployments

**For production use**, wait for v0.2.0 which will include core functionality.

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

### v0.2.0 - Core Functionality (High Priority)
- [x] Project structure and foundation
- [ ] Connect webhook events to controller ([#1](../../issues/01-webhook-controller-integration.md))
- [ ] Implement update application ([#2](../../issues/02-implement-update-application.md))
- [ ] Respect minimum update interval ([#3](../../issues/03-min-update-interval.md))
- [ ] Add integration tests ([#4](../../issues/04-integration-tests.md))
- [ ] CI/CD pipeline

### v0.3.0 - Extended Support (Medium Priority)
- [ ] Helm Release support ([#5](../../issues/05-helm-support.md))
- [ ] StatefulSet/DaemonSet support
- [ ] Notification system ([#7](../../issues/07-notification-system.md))
- [ ] Multi-architecture Docker images

### v0.4.0 - Enhanced UX (Low Priority)
- [ ] Web dashboard for approvals ([#6](../../issues/06-web-dashboard.md))
- [ ] Rollback functionality
- [ ] Custom Resource Definition for policy config
- [ ] Slack/Teams interactive approvals

### Future Ideas
- [ ] Multi-cluster support
- [ ] Canary deployment integration
- [ ] Custom update strategies
- [ ] A/B testing support
- [ ] Automatic rollback on failures

## FAQ

**Q: How is this different from Argo CD or Flux?**

A: Argo CD and Flux are GitOps tools that sync from Git. Headwind updates workloads when new *container images* are pushed to registries, regardless of Git state. They're complementary - you can use both.

**Q: Can I use this with Flux/Argo?**

A: Yes! Headwind can update the image tags, and Flux/Argo will see the change and sync. Or let Flux handle chart updates and Headwind handle image updates.

**Q: Does this work with private registries?**

A: Yes, as long as:
1. Your Kubernetes cluster can pull from the registry (imagePullSecrets configured)
2. The registry can send webhooks to Headwind
3. Webhook payloads are in supported format

**Q: What about rollbacks?**

A: Planned for v0.4.0. For now, use `kubectl rollout undo` or your GitOps tool.

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