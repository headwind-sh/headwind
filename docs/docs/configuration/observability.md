---
sidebar_position: 4
---

# Observability

Headwind provides comprehensive observability through Prometheus metrics, structured logging, and an optional integrated InfluxDB stack with real-time visualization.

## Metrics Backends

Headwind supports multiple metrics backends with automatic discovery:

- **Prometheus** - Native Prometheus endpoint (always available)
- **VictoriaMetrics** - High-performance time-series database
- **InfluxDB** - Integrated observability stack (optional)

The observability dashboard automatically detects available backends and displays data accordingly.

## Built-in Observability Dashboard

Access the dashboard at `/observability` on the Web UI:

```bash
# Via port-forward
kubectl port-forward -n headwind-system svc/headwind 8082:8082
# Visit: http://localhost:8082/observability

# Via ingress
# Visit: https://your-headwind-domain.com/observability
```

The dashboard provides:
- **Real-time metrics cards** - Current values for key metrics
- **Time-series charts** - Historical trends for all metrics
- **Auto-refresh** - Updates every 30 seconds
- **Multi-backend support** - Works with Prometheus, VictoriaMetrics, or InfluxDB

## InfluxDB Integration

### Quick Setup

Deploy Headwind with the integrated InfluxDB stack:

```bash
helm install headwind headwind/headwind \
  -n headwind-system --create-namespace \
  --set observability.create=true \
  --set observability.influxdb.enabled=true \
  --set observability.influxdb.storageSize=100Gi \
  --set telegraf.enabled=true
```

This deploys:
1. **InfluxDB 2.7** - Time-series database for metrics storage
2. **Telegraf Sidecar** - Scrapes Prometheus metrics and writes to InfluxDB
3. **Automatic Configuration** - Token management, organization, and bucket setup

### InfluxDB Configuration

```yaml title="values.yaml"
observability:
  create: true
  influxdb:
    enabled: true
    version: "2.7"

    # Data retention
    retentionHours: 720  # 30 days

    # Storage
    storageSize: 100Gi
    storageClass: ""  # Use default storage class

    # Organization and bucket
    organization: "headwind"
    bucket: "metrics"

    # Admin credentials
    adminUser: "admin"
    adminPassword: ""  # Auto-generated if empty

    # Optional: Use existing secret
    existingSecret: ""
```

### Telegraf Configuration

The Telegraf sidecar automatically scrapes Headwind's Prometheus metrics and writes them to InfluxDB:

```yaml title="values.yaml"
telegraf:
  enabled: true
  image:
    repository: telegraf
    tag: "1.28-alpine"
  resources:
    limits:
      cpu: 100m
      memory: 128Mi
    requests:
      cpu: 50m
      memory: 64Mi
```

### Token Management

InfluxDB tokens are automatically managed:

1. **First Install**: A random 64-character token is generated
2. **Upgrades**: Token persists across helm upgrades (using `lookup` function)
3. **ConfigMap**: Token is automatically injected into ConfigMap for Headwind access
4. **Secret Retention**: Secrets are kept on helm uninstall (`helm.sh/resource-policy: keep`)

Verify token configuration:

```bash
# Check secret
kubectl get secret headwind-influxdb -n headwind-system \
  -o jsonpath='{.data.admin-token}' | base64 -d

# Check ConfigMap
kubectl get configmap headwind-config -n headwind-system \
  -o jsonpath='{.data.observability\.influxdb\.token}'
```

### Accessing InfluxDB UI

Enable ingress for InfluxDB UI (optional):

```yaml title="values.yaml"
observability:
  influxdb:
    ingress:
      enabled: true
      className: nginx
      hosts:
        - host: influxdb.example.com
          paths:
            - path: /
              pathType: Prefix
```

Or use port-forward:

```bash
kubectl port-forward -n headwind-system headwind-influxdb-0 8086:8086
# Visit: http://localhost:8086
```

Login with:
- **Username**: `admin`
- **Password**: Retrieved from secret
  ```bash
  kubectl get secret headwind-influxdb -n headwind-system \
    -o jsonpath='{.data.admin-password}' | base64 -d
  ```

## Prometheus Integration

### Native Endpoint

Headwind exposes Prometheus metrics on port 9090:

```bash
kubectl port-forward -n headwind-system svc/headwind 9090:9090
curl http://localhost:9090/metrics
```

### Prometheus Operator

Enable ServiceMonitor for automatic scraping:

```yaml title="values.yaml"
serviceMonitor:
  enabled: true
  interval: 30s
  scrapeTimeout: 10s
  labels:
    prometheus: kube-prometheus
```

### PodMonitor

Alternatively, use PodMonitor:

```yaml title="values.yaml"
podMonitor:
  enabled: true
  interval: 30s
  labels:
    prometheus: kube-prometheus
```

## VictoriaMetrics Integration

Configure Headwind to query VictoriaMetrics:

```yaml title="values.yaml"
configMap:
  data:
    observability.victoriametrics.enabled: "true"
    observability.victoriametrics.url: "http://victoria-metrics.monitoring.svc.cluster.local:8428"
    observability.metricsBackend: "victoriametrics"
```

## Key Metrics

### Resource Tracking

- `headwind_deployments_watched` - Number of Deployments being monitored
- `headwind_statefulsets_watched` - Number of StatefulSets being monitored
- `headwind_daemonsets_watched` - Number of DaemonSets being monitored
- `headwind_helm_releases_watched` - Number of HelmReleases being monitored

### Update Lifecycle

- `headwind_updates_pending` - Pending update requests
- `headwind_updates_applied_total` - Successfully applied updates
- `headwind_updates_failed_total` - Failed updates
- `headwind_updates_rejected_total` - Rejected updates
- `headwind_updates_skipped_interval_total` - Updates skipped due to minimum interval

### Event Processing

- `headwind_webhook_events_total` - Total webhook events received
- `headwind_webhook_events_processed` - Webhook events successfully processed
- `headwind_polling_cycles_total` - Registry polling cycles completed
- `headwind_polling_new_tags_found_total` - New image tags discovered via polling

### Helm Charts

- `headwind_helm_chart_versions_checked_total` - Chart versions checked
- `headwind_helm_updates_found_total` - New chart versions found
- `headwind_helm_repository_queries_total` - Repository queries performed
- `headwind_helm_repository_errors_total` - Repository query errors

### Notifications

- `headwind_notifications_sent_total` - Total notifications sent
- `headwind_notifications_failed_total` - Failed notification deliveries
- `headwind_notifications_slack_sent_total` - Slack notifications sent
- `headwind_notifications_teams_sent_total` - Teams notifications sent

### Performance

- `headwind_reconcile_duration_seconds` - Controller reconciliation duration (histogram)
- `headwind_reconcile_errors_total` - Controller reconciliation errors
- `headwind_helm_repository_query_duration_seconds` - Helm repository query duration

See the [complete metrics reference](../api/metrics.md) for all 35+ available metrics.

## Structured Logging

Headwind uses structured JSON logging with configurable levels:

```yaml title="values.yaml"
env:
  RUST_LOG: "headwind=info,kube=info"
```

Log levels:
- `error` - Errors only
- `warn` - Warnings and errors
- `info` - Informational messages (default)
- `debug` - Debug information
- `trace` - Verbose trace logging

View logs:

```bash
# Main application logs
kubectl logs -n headwind-system deployment/headwind -c headwind

# Telegraf logs
kubectl logs -n headwind-system deployment/headwind -c telegraf

# InfluxDB logs
kubectl logs -n headwind-system headwind-influxdb-0
```

## Audit Logging

All approval and rejection actions are logged with:
- **Username** - Who performed the action
- **Action** - approve or reject
- **Resource** - Type, namespace, and name
- **Timestamp** - When the action occurred
- **Result** - Success or failure

Example audit log:

```json
{
  "timestamp": "2025-11-08T23:00:00Z",
  "username": "alice",
  "action": "approve",
  "resource_type": "Deployment",
  "namespace": "default",
  "name": "my-app",
  "result": "success"
}
```

Filter audit logs:

```bash
kubectl logs -n headwind-system deployment/headwind -c headwind | grep '"target":"headwind::audit"'
```

## Troubleshooting

### InfluxDB 401 Unauthorized Errors

If you see unauthorized errors in logs:

1. Verify token matches between secret and ConfigMap:
   ```bash
   # Token in secret
   kubectl get secret headwind-influxdb -n headwind-system \
     -o jsonpath='{.data.admin-token}' | base64 -d

   # Token in ConfigMap
   kubectl get configmap headwind-config -n headwind-system \
     -o jsonpath='{.data.observability\.influxdb\.token}'
   ```

2. If tokens don't match, restart Headwind:
   ```bash
   kubectl rollout restart deployment/headwind -n headwind-system
   ```

### Telegraf Not Writing Metrics

Check Telegraf logs:

```bash
kubectl logs -n headwind-system deployment/headwind -c telegraf
```

Common issues:
- **Token mismatch**: Verify InfluxDB token configuration
- **Network issues**: Check connectivity to InfluxDB service
- **Permission issues**: Verify Telegraf has write access to bucket

### Metrics Not Showing in Dashboard

1. Verify backend is enabled:
   ```bash
   kubectl get configmap headwind-config -n headwind-system -o yaml
   ```

2. Check Headwind logs for query errors:
   ```bash
   kubectl logs -n headwind-system deployment/headwind -c headwind | grep -i influx
   ```

3. Test metric query directly:
   ```bash
   kubectl exec -n headwind-system headwind-influxdb-0 -- \
     influx query 'from(bucket: "metrics") |> range(start: -1h) |> limit(n: 5)'
   ```

## Best Practices

1. **Storage Sizing**: Plan for ~1GB per week of metrics with default retention
2. **Retention Policy**: Set `retentionHours` based on compliance requirements
3. **Backup**: Use persistent volumes with backup strategy for InfluxDB
4. **Resource Limits**: Allocate sufficient CPU/memory for Telegraf sidecar
5. **Monitoring**: Set up alerts on `headwind_updates_failed_total` and `headwind_reconcile_errors_total`

## Next Steps

- [View Metrics Reference](../api/metrics.md) - Complete list of all metrics
- [Helm Installation Guide](../guides/helm-installation.md) - Install Headwind with observability enabled
