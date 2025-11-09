# Observability Dashboard

The Headwind Observability Dashboard provides real-time metrics visualization and monitoring capabilities. It supports multiple metrics backends and automatically discovers available infrastructure.

## Overview

The Observability Dashboard (`/observability`) displays:
- Real-time update statistics (pending, approved, applied, failed)
- Resource monitoring counts (Deployments, StatefulSets, DaemonSets, HelmReleases)
- Time-series data from your metrics backend
- Backend health and connectivity status

## Accessing the Dashboard

Navigate to the observability page:

```bash
# Port forward the Web UI
kubectl port-forward -n headwind-system svc/headwind-ui 8082:8082

# Open in browser
open http://localhost:8082/observability
```

Or click **"Observability"** in the Web UI navigation menu.

## Metrics Backends

Headwind supports four metrics backend types:

### 1. Prometheus (Recommended)

**Features**:
- Full time-series support
- PromQL query capabilities
- 24-hour historical data
- Industry-standard metrics storage

**Auto-Discovery**: Headwind automatically detects Prometheus at:
- `http://prometheus-server.monitoring.svc.cluster.local:80`
- `http://prometheus.monitoring.svc.cluster.local:9090`

**Manual Configuration**:
```yaml
observability:
  metricsBackend: "prometheus"
  prometheus:
    enabled: true
    url: "http://your-prometheus.namespace.svc.cluster.local:9090"
```

### 2. VictoriaMetrics

**Features**:
- Prometheus-compatible API
- High performance
- Long-term storage
- Resource efficient

**Auto-Discovery**: Detects VictoriaMetrics at:
- `http://victoria-metrics.monitoring.svc.cluster.local:8428`

**Manual Configuration**:
```yaml
observability:
  metricsBackend: "victoriametrics"
  victoriametrics:
    enabled: true
    url: "http://victoria-metrics.namespace.svc.cluster.local:8428"
```

### 3. InfluxDB v2

**Features**:
- Purpose-built time-series database
- Advanced querying with Flux
- Built-in visualization

**Configuration** (no auto-discovery):
```yaml
observability:
  metricsBackend: "influxdb"
  influxdb:
    enabled: true
    url: "http://influxdb.monitoring.svc.cluster.local:8086"
    database: "headwind"
```

**InfluxDB v2 Configuration**:
```yaml
observability:
  metricsBackend: "influxdb"
  influxdb:
    enabled: true
    url: "http://influxdb.monitoring.svc.cluster.local:8086"
    org: "headwind"           # InfluxDB organization
    bucket: "metrics"         # InfluxDB bucket name
    token: "your-api-token"   # InfluxDB API token
```

### 4. Live Metrics (Fallback)

**Features**:
- No external backend required
- Parses `/metrics` endpoint directly
- Instant values only (no time-series)
- Zero configuration

**When Used**:
- Automatically activated when no backend is available
- Useful for testing and development
- No historical data

## Auto-Discovery

By default, Headwind uses **auto-discovery** to find available metrics backends:

**Discovery Priority**:
1. Prometheus (if available)
2. VictoriaMetrics (if available)
3. InfluxDB (if configured and enabled)
4. Live metrics (always available)

**Configuration**:
```yaml
observability:
  metricsBackend: "auto"  # Default
```

The dashboard displays which backend is currently active in an alert banner.

## Configuration

### Via ConfigMap

Create or update the Headwind ConfigMap:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: headwind-config
  namespace: headwind-system
data:
  config.yaml: |
    observability:
      metricsBackend: "auto"
      prometheus:
        enabled: true
        url: "http://prometheus-server.monitoring.svc.cluster.local:80"
      victoriametrics:
        enabled: false
        url: "http://victoria-metrics.monitoring.svc.cluster.local:8428"
      influxdb:
        enabled: false
        url: "http://influxdb.monitoring.svc.cluster.local:8086"
        database: "headwind"
```

### Via Web UI Settings

Navigate to **Settings** in the Web UI and configure under **Observability / Metrics Storage**:

1. Select metrics backend (auto, prometheus, victoriametrics, influxdb, live)
2. Enable/disable specific backends
3. Configure URLs
4. Click **Save Settings**
5. Configuration hot-reloads automatically (no restart required)

## Dashboard Features

### Metrics Cards

**Updates Section**:
- **Pending**: Updates awaiting approval
- **Approved**: Total approved updates
- **Applied**: Successfully applied updates
- **Failed**: Updates that failed to apply

**Resources Section**:
- **Deployments Watched**: Active Deployment monitoring
- **StatefulSets Watched**: Active StatefulSet monitoring
- **DaemonSets Watched**: Active DaemonSet monitoring
- **Helm Releases Watched**: Active HelmRelease monitoring

### Time-Series Charts

When using Prometheus, VictoriaMetrics, or InfluxDB backends, the dashboard displays interactive time-series charts powered by Chart.js.

**Charts Included**:

1. **Updates Over Time** - 24-hour historical view:
   - Approved updates (teal line)
   - Applied updates (green line)
   - Failed updates (red line)

2. **Resources Watched** - 24-hour resource monitoring:
   - Deployments watched (blue line)
   - StatefulSets watched (indigo line)
   - DaemonSets watched (purple line)
   - Helm Releases watched (pink line)

**Features**:
- Interactive hover tooltips showing exact values
- Smooth line charts with 5-minute data points
- Responsive design adapts to screen size
- Legend at bottom for better visibility
- Auto-refresh updates charts every 30 seconds

**Note**: Charts are only available with Prometheus, VictoriaMetrics, or InfluxDB backends. The "Live" fallback mode doesn't support historical data, so charts won't display.

### Auto-Refresh

The observability dashboard auto-refreshes every 30 seconds to display the latest metrics and update charts.

**Visual Indicators**:
- Loading spinner while fetching data
- Error messages if backend unavailable
- Backend name displayed in alert banner
- Charts automatically reload with new data

### Backend Status

A colored alert banner shows the current backend status:

- **🟢 Green (Prometheus/VictoriaMetrics)**: External backend active
- **🟡 Yellow (InfluxDB)**: InfluxDB backend (experimental)
- **🔵 Blue (Live)**: Fallback mode, no external backend

## API Endpoints

### Get Current Metrics

Fetch current metric values:

```bash
curl http://headwind-ui:8082/api/v1/metrics
```

**Response**:
```json
{
  "backend": "Prometheus",
  "metrics": {
    "updates_pending": 5,
    "updates_approved": 120,
    "updates_rejected": 3,
    "updates_applied": 115,
    "updates_failed": 2,
    "deployments_watched": 10,
    "statefulsets_watched": 3,
    "daemonsets_watched": 5,
    "helm_releases_watched": 8
  }
}
```

### Get Time-Series Data

Fetch 24-hour historical data for a specific metric:

```bash
curl http://headwind-ui:8082/api/v1/metrics/timeseries/headwind_updates_pending
```

**Response**:
```json
[
  {"timestamp": "2025-11-08T00:00:00Z", "value": 3},
  {"timestamp": "2025-11-08T00:05:00Z", "value": 5},
  {"timestamp": "2025-11-08T00:10:00Z", "value": 4},
  ...
]
```

**Parameters**:
- Metric name: Any valid Headwind metric (see [Metrics Reference](../api/metrics.md))
- Time range: Fixed 24 hours
- Step: 5 minutes

## Prometheus Integration

### Installing Prometheus

Deploy Prometheus to your cluster:

```yaml
apiVersion: v1
kind: Namespace
metadata:
  name: monitoring

---
apiVersion: v1
kind: ConfigMap
metadata:
  name: prometheus-config
  namespace: monitoring
data:
  prometheus.yml: |
    global:
      scrape_interval: 15s

    scrape_configs:
      - job_name: 'headwind'
        static_configs:
          - targets: ['headwind-metrics.headwind-system.svc.cluster.local:9090']
        scrape_interval: 15s

---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: prometheus
  namespace: monitoring
spec:
  selector:
    matchLabels:
      app: prometheus
  template:
    metadata:
      labels:
        app: prometheus
    spec:
      containers:
        - name: prometheus
          image: prom/prometheus:v2.48.0
          args:
            - '--config.file=/etc/prometheus/prometheus.yml'
            - '--storage.tsdb.path=/prometheus'
          ports:
            - containerPort: 9090
          volumeMounts:
            - name: config
              mountPath: /etc/prometheus
            - name: storage
              mountPath: /prometheus
      volumes:
        - name: config
          configMap:
            name: prometheus-config
        - name: storage
          emptyDir: {}

---
apiVersion: v1
kind: Service
metadata:
  name: prometheus-server
  namespace: monitoring
spec:
  selector:
    app: prometheus
  ports:
    - port: 80
      targetPort: 9090
```

Apply the manifest:

```bash
kubectl apply -f prometheus.yaml
```

### Verification

Check that Prometheus is scraping Headwind:

```bash
# Port forward Prometheus
kubectl port-forward -n monitoring svc/prometheus-server 9090:80

# Open Prometheus UI
open http://localhost:9090

# Query Headwind metrics
# Navigate to Graph tab and query: headwind_updates_pending
```

The Observability Dashboard will automatically detect Prometheus and switch to it.

## VictoriaMetrics Integration

VictoriaMetrics is a Prometheus-compatible alternative with better performance:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: victoria-metrics
  namespace: monitoring
spec:
  selector:
    matchLabels:
      app: victoria-metrics
  template:
    metadata:
      labels:
        app: victoria-metrics
    spec:
      containers:
        - name: victoria-metrics
          image: victoriametrics/victoria-metrics:latest
          args:
            - '-promscrape.config=/etc/prometheus/prometheus.yml'
            - '-storageDataPath=/victoria-metrics-data'
          ports:
            - containerPort: 8428
          volumeMounts:
            - name: config
              mountPath: /etc/prometheus
            - name: storage
              mountPath: /victoria-metrics-data
      volumes:
        - name: config
          configMap:
            name: prometheus-config  # Reuse same config
        - name: storage
          emptyDir: {}
```

## Troubleshooting

### "Backend: Live" shown instead of Prometheus

**Problem**: Auto-discovery not finding Prometheus

**Solutions**:

1. **Check Prometheus is running**:
```bash
kubectl get pods -n monitoring -l app=prometheus
```

2. **Check service name and port**:
```bash
kubectl get svc -n monitoring prometheus-server
```

3. **Check connectivity from Headwind pod**:
```bash
kubectl exec -n headwind-system deployment/headwind -- curl -v http://prometheus-server.monitoring.svc.cluster.local:80/api/v1/query?query=up
```

4. **Manually configure URL** in ConfigMap if auto-discovery fails

### "Failed to fetch metrics"

**Problem**: API requests failing

**Possible Causes**:
1. Backend URL incorrect
2. Backend not responding
3. Network policy blocking access
4. Authentication required (not yet supported)

**Solutions**:
- Check backend logs
- Verify URL in ConfigMap
- Test connectivity from Headwind pod
- Check NetworkPolicies

### Time-series data showing "No data"

**Problem**: 24-hour query returns empty results

**Possible Causes**:
1. Prometheus retention too short
2. Metric name incorrect
3. No data collected yet (new installation)

**Solutions**:
- Wait for Prometheus to collect data (15s intervals)
- Verify metric name exists: `curl http://headwind-metrics:9090/metrics | grep headwind`
- Check Prometheus retention settings

### Metrics showing 0 or incorrect values

**Problem**: Values don't match expected state

**Possible Causes**:
1. Prometheus scrape failing
2. Stale data cached
3. Clock skew

**Solutions**:
```bash
# Check Prometheus is scraping successfully
kubectl logs -n monitoring deployment/prometheus | grep headwind

# Check /metrics endpoint directly
kubectl port-forward -n headwind-system svc/headwind-metrics 9090:9090
curl localhost:9090/metrics | grep headwind_updates
```

## Available Metrics

See the full list of metrics in the [Metrics API Reference](../api/metrics.md).

**Key Metrics for Observability Dashboard**:
- `headwind_updates_pending` - Current pending updates
- `headwind_updates_approved_total` - Total approved updates
- `headwind_updates_applied_total` - Total applied updates
- `headwind_updates_failed_total` - Total failed updates
- `headwind_deployments_watched` - Deployments being monitored
- `headwind_statefulsets_watched` - StatefulSets being monitored
- `headwind_daemonsets_watched` - DaemonSets being monitored
- `headwind_helm_releases_watched` - HelmReleases being monitored

## Next Steps

- [Metrics API Reference](../api/metrics.md) - Complete metrics documentation
- [Web UI Overview](./web-ui.md) - Main dashboard features
- [Configuration](../configuration/web-ui.md) - Customize settings
