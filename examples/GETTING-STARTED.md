# Getting Started with Headwind

This guide shows you the easiest way to get your Kubernetes workloads monitored by Headwind after installing via Helm.

## Quick Start - 3 Steps

### 1. Add Headwind Annotations

Add these annotations to your Deployment, StatefulSet, DaemonSet, or HelmRelease:

```yaml
metadata:
  annotations:
    headwind.sh/policy: "minor"  # Required: Update policy
```

That's it! Headwind will now watch for updates to this workload.

### 2. Apply Your Workload

```bash
kubectl apply -f your-deployment.yaml
```

### 3. Verify Headwind is Monitoring

Check the observability dashboard:
- Via Ingress: `http://your-headwind-ui-host/observability`
- Via port-forward: `kubectl port-forward -n headwind-system svc/headwind 8082:8082`
  Then visit: `http://localhost:8082/observability`

You should see the "Deployments Watched" (or StatefulSets/DaemonSets/Helm Releases) count increase.

## Update Policies

Choose the update policy that makes sense for your workload:

- **`patch`** - Only patch version updates (1.2.3 → 1.2.4)
- **`minor`** - Only minor version updates (1.2.3 → 1.3.0)
- **`major`** - Any version update including major (1.2.3 → 2.0.0)
- **`all`** - Any new version
- **`glob`** - Pattern matching (requires `headwind.sh/pattern` annotation)
- **`none`** - Disable updates (useful for debugging)

## Common Annotation Examples

### Basic Monitoring (Auto-Update with Approval)
```yaml
metadata:
  annotations:
    headwind.sh/policy: "minor"
```

### Auto-Update WITHOUT Approval
```yaml
metadata:
  annotations:
    headwind.sh/policy: "patch"
    headwind.sh/require-approval: "false"
```

### Polling Instead of Webhooks
```yaml
metadata:
  annotations:
    headwind.sh/policy: "minor"
    headwind.sh/event-source: "polling"
    headwind.sh/polling-interval: "60"  # Check every 60 seconds
```

### Both Webhooks and Polling
```yaml
metadata:
  annotations:
    headwind.sh/policy: "minor"
    headwind.sh/event-source: "both"
```

### Pattern-Based Updates (Glob)
```yaml
metadata:
  annotations:
    headwind.sh/policy: "glob"
    headwind.sh/pattern: "v1.*-stable"
```

## Full Example Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-app
  namespace: default
  annotations:
    # Required
    headwind.sh/policy: "minor"

    # Optional (with defaults shown)
    headwind.sh/require-approval: "true"
    headwind.sh/min-update-interval: "300"
    headwind.sh/event-source: "webhook"
spec:
  replicas: 2
  selector:
    matchLabels:
      app: my-app
  template:
    metadata:
      labels:
        app: my-app
    spec:
      containers:
      - name: my-app
        image: myregistry.io/my-app:v1.2.0
        ports:
        - containerPort: 8080
```

## What Happens Next?

1. **Headwind Detects Your Workload**: Once you apply the manifest with Headwind annotations, the controller immediately detects it and starts watching
2. **New Version Available**: When a new version is pushed to your registry:
   - **Webhook mode**: Registry sends webhook to Headwind
   - **Polling mode**: Headwind discovers it during the next poll
3. **Policy Check**: Headwind validates the new version against your policy
4. **UpdateRequest Created**: If approval required, an UpdateRequest CRD is created
5. **View in Dashboard**: See pending updates at `http://your-headwind-ui-host/`
6. **Approve/Reject**: Click approve or reject in the Web UI
7. **Update Applied**: Headwind updates the image in your workload spec

## Next Steps

- **Configure Webhooks**: Set up registry webhooks to push to Headwind's webhook endpoint
- **Enable Notifications**: Configure Slack/Teams notifications in Helm values
- **Monitor Metrics**: View Prometheus metrics at `/metrics` or use the observability dashboard
- **Check Logs**: `kubectl logs -n headwind-system deployment/headwind`

## Troubleshooting

### My workload isn't being watched

Check the logs:
```bash
kubectl logs -n headwind-system deployment/headwind -c headwind | grep "your-workload-name"
```

You should see: `Deployment default/your-workload-name has policy Minor`

### I don't see updates in the dashboard

1. Verify registry webhooks are configured and reaching Headwind
2. Or enable polling: `headwind.sh/event-source: "polling"`
3. Check logs for webhook events: `grep webhook`

### Updates aren't being applied

- Check if `headwind.sh/require-approval: "true"` - you need to approve via Web UI
- Check minimum update interval - might be too soon after last update
- View UpdateRequest CRDs: `kubectl get updaterequests -A`

## More Information

- [Full Documentation](https://headwind.sh)
- [Helm Chart README](../charts/headwind/README.md)
- [Example Manifests](./test-manifests/)
