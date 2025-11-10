---
sidebar_position: 1
---

# Helm Installation

The easiest way to install Headwind is using the official Helm chart.

## Prerequisites

- Kubernetes cluster (1.19+)
- Helm 3.2.0+
- kubectl configured to access your cluster

## Quick Start

### Add the Helm Repository

```bash
helm repo add headwind https://headwind.sh/charts
helm repo update
```

### Install Headwind

Basic installation with default settings:

```bash
helm install headwind headwind/headwind \
  -n headwind-system \
  --create-namespace
```

### Verify Installation

Check that Headwind is running:

```bash
kubectl get pods -n headwind-system
```

You should see the Headwind pod in `Running` state:

```
NAME                        READY   STATUS    RESTARTS   AGE
headwind-6d66bb7b9d-xxxxx   2/2     Running   0          1m
```

## Installation Options

### With Registry Polling

Enable registry polling to check for new images periodically:

```bash
helm install headwind headwind/headwind \
  -n headwind-system --create-namespace \
  --set env.HEADWIND_POLLING_ENABLED=true \
  --set env.HEADWIND_POLLING_INTERVAL=300
```

### With InfluxDB Observability

Deploy with the integrated InfluxDB observability stack:

```bash
helm install headwind headwind/headwind \
  -n headwind-system --create-namespace \
  --set observability.create=true \
  --set observability.influxdb.enabled=true \
  --set observability.influxdb.storageSize=100Gi \
  --set telegraf.enabled=true
```

### With Ingress (NGINX)

Expose Headwind Web UI via NGINX Ingress:

```bash
helm install headwind headwind/headwind \
  -n headwind-system --create-namespace \
  --set ingress.enabled=true \
  --set ingress.className=nginx \
  --set ingress.hosts[0].host=headwind.example.com \
  --set ingress.hosts[0].paths[0].path=/ \
  --set ingress.hosts[0].paths[0].pathType=Prefix \
  --set ingress.hosts[0].paths[0].backend=ui
```

### With Slack Notifications

Enable Slack notifications for update events:

```bash
helm install headwind headwind/headwind \
  -n headwind-system --create-namespace \
  --set notifications.slack.enabled=true \
  --set notifications.slack.webhookUrl="https://hooks.slack.com/services/YOUR/WEBHOOK/URL"
```

## Custom Configuration

For advanced configuration, create a `values.yaml` file:

```yaml title="values.yaml"
# Enable registry polling
env:
  HEADWIND_POLLING_ENABLED: "true"
  HEADWIND_POLLING_INTERVAL: "300"
  HEADWIND_UI_URL: "https://headwind.example.com"

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
    webhookUrl: "https://hooks.slack.com/services/YOUR/WEBHOOK/URL"

# Enable observability
observability:
  create: true
  influxdb:
    enabled: true
    storageSize: 100Gi

telegraf:
  enabled: true

# Resource limits
resources:
  limits:
    cpu: 1000m
    memory: 1Gi
  requests:
    cpu: 200m
    memory: 256Mi
```

Install with your custom values:

```bash
helm install headwind headwind/headwind \
  -n headwind-system --create-namespace \
  -f values.yaml
```

## Upgrading

### Upgrade to Latest Version

```bash
helm repo update
helm upgrade headwind headwind/headwind -n headwind-system
```

### Upgrade with New Values

```bash
helm upgrade headwind headwind/headwind \
  -n headwind-system \
  --reuse-values \
  --set observability.create=true
```

### Upgrade to Specific Version

```bash
helm upgrade headwind headwind/headwind \
  -n headwind-system \
  --version 0.2.0
```

## Uninstalling

To remove Headwind:

```bash
helm uninstall headwind -n headwind-system
```

:::warning
This will delete all Headwind resources but will keep:
- Custom Resource Definitions (CRDs)
- InfluxDB data (if persistence is enabled)
- Secrets with `helm.sh/resource-policy: keep` annotation
:::

To completely remove everything:

```bash
# Uninstall the chart
helm uninstall headwind -n headwind-system

# Delete CRDs
kubectl delete crd updaterequests.headwind.sh

# Delete namespace (removes all remaining resources)
kubectl delete namespace headwind-system
```

## Configuration Parameters

For a complete list of configuration parameters, see the [Helm Chart README](https://github.com/headwind-sh/headwind/blob/main/charts/headwind/README.md).

Key parameters include:

- **Image**: `image.repository`, `image.tag`, `image.pullPolicy`
- **Replicas**: `replicaCount`
- **Resources**: `resources.limits`, `resources.requests`
- **Service**: `service.type`, `service.webhookPort`, `service.apiPort`, `service.uiPort`
- **Ingress**: `ingress.enabled`, `ingress.className`, `ingress.hosts`
- **Notifications**: `notifications.slack.*`, `notifications.teams.*`, `notifications.webhook.*`
- **Observability**: `observability.influxdb.*`, `telegraf.*`
- **Environment**: `env.*`

## Troubleshooting

### Pods Not Starting

Check pod status and logs:

```bash
kubectl get pods -n headwind-system
kubectl describe pod -n headwind-system <pod-name>
kubectl logs -n headwind-system <pod-name> -c headwind
```

### InfluxDB Issues

Check InfluxDB pod status:

```bash
kubectl get pods -n headwind-system -l app.kubernetes.io/component=influxdb
kubectl logs -n headwind-system headwind-influxdb-0
```

Verify InfluxDB token configuration:

```bash
kubectl get secret headwind-influxdb -n headwind-system -o yaml
kubectl get configmap headwind-config -n headwind-system -o yaml
```

### Ingress Not Working

Verify ingress resource:

```bash
kubectl get ingress -n headwind-system
kubectl describe ingress -n headwind-system headwind
```

Check ingress controller logs:

```bash
kubectl logs -n ingress-nginx deployment/ingress-nginx-controller
```

## Next Steps

- [Set Up Notifications](../configuration/notifications.md) - Configure Slack, Teams, or webhook notifications
- [Enable Observability](../configuration/observability.md) - Set up metrics and monitoring
- [View API Reference](../api/metrics.md) - Complete metrics documentation
