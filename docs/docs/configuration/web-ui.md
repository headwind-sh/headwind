# Web UI Configuration

Configure the Headwind Web UI using environment variables and ConfigMap settings for hot-reload configuration management.

## Environment Variables

Configure via the Headwind deployment manifest (`deploy/k8s/deployment.yaml`):

### Authentication

```yaml
env:
  # Authentication mode: none, simple, token, proxy
  - name: HEADWIND_UI_AUTH_MODE
    value: "none"

  # Proxy mode only: header name to read username from
  - name: HEADWIND_UI_PROXY_HEADER
    value: "X-Forwarded-User"
```

See [Web UI Authentication Guide](../guides/web-ui-authentication.md) for detailed authentication configuration.

### Notification Integration

```yaml
env:
  # Web UI URL for dashboard links in notifications
  - name: HEADWIND_UI_URL
    value: "https://headwind.example.com"  # or http://localhost:8082
```

When configured, Slack, Teams, and webhook notifications will include "View in Dashboard" buttons linking to specific UpdateRequests.

## ConfigMap Settings

The Web UI supports hot-reload configuration via ConfigMap. Changes are detected automatically without pod restarts.

### Creating the ConfigMap

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: headwind-config
  namespace: headwind-system
data:
  config.yaml: |
    # Dashboard settings
    refresh_interval: 30        # Auto-refresh interval in seconds
    max_items_per_page: 20      # Pagination size

    # Observability settings
    observability:
      metricsBackend: "auto"    # auto | prometheus | victoriametrics | influxdb | live

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

### Mounting the ConfigMap

Update the Headwind deployment to mount the ConfigMap:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: headwind
  namespace: headwind-system
spec:
  template:
    spec:
      containers:
        - name: headwind
          volumeMounts:
            - name: config
              mountPath: /etc/headwind
              readOnly: true
      volumes:
        - name: config
          configMap:
            name: headwind-config
```

Apply the changes:

```bash
kubectl apply -f deploy/k8s/deployment.yaml
```

### Updating Configuration

Modify the ConfigMap and changes apply automatically:

```bash
# Edit ConfigMap
kubectl edit configmap headwind-config -n headwind-system

# Or apply from file
kubectl apply -f headwind-config.yaml

# Configuration reloads automatically (watch logs for confirmation)
kubectl logs -n headwind-system deployment/headwind -f | grep "Configuration reloaded"
```

**No pod restart required** - Headwind watches the ConfigMap for changes.

## Configuration via Web UI Settings

Access the settings page at `/settings` in the Web UI to configure:

### Observability Settings

1. Navigate to **Settings** → **Observability / Metrics Storage**
2. Select metrics backend:
   - **auto**: Automatically detect available backend
   - **prometheus**: Use Prometheus
   - **victoriametrics**: Use VictoriaMetrics
   - **influxdb**: Use InfluxDB
   - **live**: Use live metrics (no external backend)
3. Configure backend URLs and options
4. Click **Save Settings**

Changes are saved to the ConfigMap and hot-reload automatically.

### Dashboard Settings

Configure refresh interval and pagination:

1. Navigate to **Settings** → **Dashboard**
2. Set **Refresh Interval** (seconds)
3. Set **Max Items Per Page** (pagination)
4. Click **Save Settings**

## Configuration Reference

### Dashboard Settings

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `refresh_interval` | integer | `30` | Auto-refresh interval in seconds |
| `max_items_per_page` | integer | `20` | Number of items per page in dashboard |

### Observability Settings

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `observability.metricsBackend` | string | `auto` | Backend selection: auto, prometheus, victoriametrics, influxdb, live |
| `observability.prometheus.enabled` | boolean | `true` | Enable Prometheus backend |
| `observability.prometheus.url` | string | `http://prometheus-server.monitoring.svc.cluster.local:80` | Prometheus URL |
| `observability.victoriametrics.enabled` | boolean | `false` | Enable VictoriaMetrics backend |
| `observability.victoriametrics.url` | string | `http://victoria-metrics.monitoring.svc.cluster.local:8428` | VictoriaMetrics URL |
| `observability.influxdb.enabled` | boolean | `false` | Enable InfluxDB v2 backend |
| `observability.influxdb.url` | string | `http://influxdb.monitoring.svc.cluster.local:8086` | InfluxDB v2 URL |
| `observability.influxdb.org` | string | `headwind` | InfluxDB v2 organization |
| `observability.influxdb.bucket` | string | `metrics` | InfluxDB v2 bucket name |
| `observability.influxdb.token` | string | `headwind-test-token` | InfluxDB v2 API token |

## Service Configuration

Configure the Web UI service in `deploy/k8s/service.yaml`:

### ClusterIP (Default)

Internal access only:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: headwind-ui
  namespace: headwind-system
spec:
  type: ClusterIP
  selector:
    app: headwind
  ports:
    - name: ui
      port: 8082
      targetPort: 8082
```

Access via port-forward:
```bash
kubectl port-forward -n headwind-system svc/headwind-ui 8082:8082
```

### LoadBalancer

External access with cloud provider load balancer:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: headwind-ui
  namespace: headwind-system
spec:
  type: LoadBalancer
  selector:
    app: headwind
  ports:
    - name: ui
      port: 80
      targetPort: 8082
```

Get external IP:
```bash
kubectl get svc headwind-ui -n headwind-system
```

### NodePort

External access via node port:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: headwind-ui
  namespace: headwind-system
spec:
  type: NodePort
  selector:
    app: headwind
  ports:
    - name: ui
      port: 8082
      targetPort: 8082
      nodePort: 30082  # Optional: specify port (30000-32767)
```

Access via:
```bash
# Get node IP
kubectl get nodes -o wide

# Access at http://<node-ip>:30082
```

## Ingress Configuration

Expose the Web UI via Kubernetes Ingress:

### Basic Ingress

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: headwind-ui
  namespace: headwind-system
spec:
  ingressClassName: nginx
  rules:
    - host: headwind.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: headwind-ui
                port:
                  number: 8082
```

### Ingress with TLS

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: headwind-ui
  namespace: headwind-system
  annotations:
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
spec:
  ingressClassName: nginx
  tls:
    - hosts:
        - headwind.example.com
      secretName: headwind-tls
  rules:
    - host: headwind.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: headwind-ui
                port:
                  number: 8082
```

### Ingress with Authentication (oauth2-proxy)

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: headwind-ui
  namespace: headwind-system
  annotations:
    nginx.ingress.kubernetes.io/auth-url: "https://oauth2-proxy.example.com/oauth2/auth"
    nginx.ingress.kubernetes.io/auth-signin: "https://oauth2-proxy.example.com/oauth2/start"
    nginx.ingress.kubernetes.io/auth-response-headers: "X-Auth-Request-User,X-Auth-Request-Email"
spec:
  ingressClassName: nginx
  tls:
    - hosts:
        - headwind.example.com
      secretName: headwind-tls
  rules:
    - host: headwind.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: headwind-ui
                port:
                  number: 8082
```

With corresponding Headwind configuration:
```yaml
env:
  - name: HEADWIND_UI_AUTH_MODE
    value: "proxy"
  - name: HEADWIND_UI_PROXY_HEADER
    value: "X-Auth-Request-User"
  - name: HEADWIND_UI_URL
    value: "https://headwind.example.com"
```

## Security Configuration

### Read-Only Root Filesystem

The Web UI container runs with a read-only root filesystem:

```yaml
securityContext:
  readOnlyRootFilesystem: true
  runAsNonRoot: true
  runAsUser: 1001
  capabilities:
    drop:
      - ALL
```

### Network Policies

Restrict network access to the Web UI:

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: headwind-ui
  namespace: headwind-system
spec:
  podSelector:
    matchLabels:
      app: headwind
  policyTypes:
    - Ingress
  ingress:
    # Allow from ingress controller
    - from:
        - namespaceSelector:
            matchLabels:
              name: ingress-nginx
      ports:
        - protocol: TCP
          port: 8082
```

## Resource Limits

Configure resource requests and limits:

```yaml
resources:
  requests:
    memory: "128Mi"
    cpu: "100m"
  limits:
    memory: "512Mi"
    cpu: "500m"
```

## Troubleshooting

### Configuration not reloading

**Problem**: Changes to ConfigMap not detected

**Solutions**:
1. Check ConfigMap is mounted correctly:
```bash
kubectl exec -n headwind-system deployment/headwind -- ls -la /etc/headwind
```

2. Check logs for reload messages:
```bash
kubectl logs -n headwind-system deployment/headwind | grep -i config
```

3. Restart pod if necessary:
```bash
kubectl rollout restart deployment/headwind -n headwind-system
```

### Settings page shows defaults

**Problem**: ConfigMap settings not loading

**Solutions**:
1. Verify ConfigMap exists:
```bash
kubectl get configmap headwind-config -n headwind-system -o yaml
```

2. Check YAML syntax:
```bash
kubectl get configmap headwind-config -n headwind-system -o jsonpath='{.data.config\.yaml}' | yq eval
```

3. Check file permissions:
```bash
kubectl exec -n headwind-system deployment/headwind -- cat /etc/headwind/config.yaml
```

## Examples

See complete example configurations in the repository:
- `deploy/k8s/configmap.yaml` - ConfigMap with all options
- `deploy/k8s/deployment.yaml` - Deployment with environment variables
- `deploy/k8s/service.yaml` - Service configurations
- `examples/ingress/` - Ingress examples

## Next Steps

- [Web UI Overview](../guides/web-ui.md) - Dashboard features
- [Authentication](../guides/web-ui-authentication.md) - Configure authentication
- [Observability Dashboard](../guides/observability-dashboard.md) - Metrics visualization
