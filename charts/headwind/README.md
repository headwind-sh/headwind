# Headwind Helm Chart

Headwind is a Kubernetes operator that automates workload updates based on container image changes. It provides webhook-driven updates, registry polling, approval workflows, and comprehensive observability.

## TL;DR

```bash
helm repo add headwind https://headwind.sh/charts
helm repo update
helm install headwind headwind/headwind -n headwind-system --create-namespace
```

## Introduction

This chart bootstraps a Headwind deployment on a Kubernetes cluster using the Helm package manager.

## Prerequisites

- Kubernetes 1.19+
- Helm 3.2.0+
- PV provisioner support in the underlying infrastructure (for InfluxDB persistence)

## Installing the Chart

To install the chart with the release name `headwind`:

```bash
helm install headwind headwind/headwind -n headwind-system --create-namespace
```

The command deploys Headwind on the Kubernetes cluster with default configuration. The [Parameters](#parameters) section lists the parameters that can be configured during installation.

## Uninstalling the Chart

To uninstall/delete the `headwind` deployment:

```bash
helm delete headwind -n headwind-system
```

The command removes all the Kubernetes components associated with the chart and deletes the release.

## Parameters

### Global Parameters

| Name                      | Description                                     | Value   |
|---------------------------|-------------------------------------------------|---------|
| `replicaCount`            | Number of Headwind replicas to deploy           | `1`     |
| `nameOverride`            | String to partially override headwind.fullname  | `""`    |
| `fullnameOverride`        | String to fully override headwind.fullname      | `""`    |

### Image Parameters

| Name                | Description                          | Value                          |
|---------------------|--------------------------------------|--------------------------------|
| `image.repository`  | Headwind image repository            | `ghcr.io/headwind-sh/headwind` |
| `image.tag`         | Headwind image tag (defaults to appVersion) | `""`                    |
| `image.pullPolicy`  | Headwind image pull policy           | `IfNotPresent`                 |
| `imagePullSecrets`  | Image pull secrets                   | `[]`                           |

### Service Account Parameters

| Name                         | Description                                      | Value  |
|------------------------------|--------------------------------------------------|--------|
| `serviceAccount.create`      | Enable creation of ServiceAccount                | `true` |
| `serviceAccount.automount`   | Automount API credentials                        | `true` |
| `serviceAccount.annotations` | Annotations for service account                  | `{}`   |
| `serviceAccount.name`        | Name of the service account to use               | `""`   |

### RBAC Parameters

| Name          | Description                        | Value  |
|---------------|------------------------------------|--------|
| `rbac.create` | Create RBAC resources              | `true` |
| `rbac.rules`  | Custom RBAC rules                  | See values.yaml |

### Service Parameters

| Name                    | Description                    | Value       |
|-------------------------|--------------------------------|-------------|
| `service.type`          | Kubernetes service type        | `ClusterIP` |
| `service.webhookPort`   | Webhook server port            | `8080`      |
| `service.apiPort`       | Approval API port              | `8081`      |
| `service.uiPort`        | Web UI port                    | `8082`      |
| `service.metricsPort`   | Prometheus metrics port        | `9090`      |
| `service.annotations`   | Service annotations            | `{}`        |

### Ingress Parameters

| Name                  | Description                                      | Value   |
|-----------------------|--------------------------------------------------|---------|
| `ingress.enabled`     | Enable ingress controller resource               | `false` |
| `ingress.className`   | Ingress class name                               | `""`    |
| `ingress.annotations` | Ingress annotations                              | `{}`    |
| `ingress.hosts`       | Hostname(s) for the ingress resource             | `[]`    |
| `ingress.tls`         | TLS configuration for ingress                    | `[]`    |

### Resource Limits

| Name                      | Description                  | Value    |
|---------------------------|------------------------------|----------|
| `resources.limits.cpu`    | CPU resource limits          | `500m`   |
| `resources.limits.memory` | Memory resource limits       | `512Mi`  |
| `resources.requests.cpu`  | CPU resource requests        | `100m`   |
| `resources.requests.memory` | Memory resource requests   | `128Mi`  |

### Environment Variables

| Name                             | Description                                    | Value             |
|----------------------------------|------------------------------------------------|-------------------|
| `env.RUST_LOG`                   | Rust logging configuration                     | `headwind=info,kube=info` |
| `env.HEADWIND_UI_URL`            | Web UI URL for notifications                   | `""`              |
| `env.HEADWIND_POLLING_ENABLED`   | Enable registry polling                        | `"false"`         |
| `env.HEADWIND_POLLING_INTERVAL`  | Polling interval in seconds                    | `"300"`           |
| `env.HEADWIND_UI_AUTH_MODE`      | Web UI authentication mode                     | `"none"`          |
| `env.HEADWIND_UI_PROXY_HEADER`   | Proxy authentication header name               | `"X-Forwarded-User"` |

### Notification Parameters

| Name                               | Description                        | Value   |
|------------------------------------|------------------------------------|---------|
| `notifications.createSecret`       | Create secret for notifications    | `true`  |
| `notifications.slack.enabled`      | Enable Slack notifications         | `false` |
| `notifications.slack.webhookUrl`   | Slack webhook URL                  | `""`    |
| `notifications.teams.enabled`      | Enable Teams notifications         | `false` |
| `notifications.teams.webhookUrl`   | Teams webhook URL                  | `""`    |
| `notifications.webhook.enabled`    | Enable generic webhook             | `false` |
| `notifications.webhook.url`        | Generic webhook URL                | `""`    |

### Observability Parameters

| Name                                        | Description                              | Value                          |
|---------------------------------------------|------------------------------------------|--------------------------------|
| `observability.create`                      | Deploy observability stack               | `false`                        |
| `observability.influxdb.enabled`            | Enable InfluxDB deployment               | `false`                        |
| `observability.influxdb.version`            | InfluxDB version                         | `"2.7"`                        |
| `observability.influxdb.retentionHours`     | Data retention in hours                  | `720`                          |
| `observability.influxdb.storageSize`        | Storage size for InfluxDB                | `100Gi`                        |
| `observability.influxdb.storageClass`       | Storage class for PVC                    | `""`                           |
| `observability.influxdb.organization`       | InfluxDB organization                    | `"headwind"`                   |
| `observability.influxdb.bucket`             | InfluxDB bucket name                     | `"metrics"`                    |
| `observability.influxdb.adminUser`          | InfluxDB admin username                  | `"admin"`                      |
| `observability.influxdb.adminPassword`      | InfluxDB admin password (auto-generated) | `""`                           |

### Telegraf Sidecar Parameters

| Name                              | Description                          | Value                    |
|-----------------------------------|--------------------------------------|--------------------------|
| `telegraf.enabled`                | Enable Telegraf sidecar              | `false`                  |
| `telegraf.image.repository`       | Telegraf image repository            | `telegraf`               |
| `telegraf.image.tag`              | Telegraf image tag                   | `"1.28-alpine"`          |
| `telegraf.resources.limits.cpu`   | Telegraf CPU limits                  | `100m`                   |
| `telegraf.resources.limits.memory` | Telegraf memory limits              | `128Mi`                  |

### Monitoring Parameters

| Name                        | Description                            | Value   |
|-----------------------------|----------------------------------------|---------|
| `serviceMonitor.enabled`    | Create ServiceMonitor (Prometheus Operator) | `false` |
| `serviceMonitor.interval`   | Scrape interval                        | `30s`   |
| `podMonitor.enabled`        | Create PodMonitor (Prometheus Operator) | `false` |

### Network Policy

| Name                     | Description                | Value   |
|--------------------------|----------------------------|---------|
| `networkPolicy.enabled`  | Enable NetworkPolicy       | `false` |

## Configuration Examples

### Basic Installation

Install Headwind with default settings:

```bash
helm install headwind headwind/headwind -n headwind-system --create-namespace
```

### With Registry Polling Enabled

```bash
helm install headwind headwind/headwind \
  -n headwind-system --create-namespace \
  --set env.HEADWIND_POLLING_ENABLED=true \
  --set env.HEADWIND_POLLING_INTERVAL=300
```

### With Slack Notifications

```bash
helm install headwind headwind/headwind \
  -n headwind-system --create-namespace \
  --set notifications.slack.enabled=true \
  --set notifications.slack.webhookUrl="https://hooks.slack.com/services/YOUR/WEBHOOK/URL"
```

### With InfluxDB Observability Stack

```bash
helm install headwind headwind/headwind \
  -n headwind-system --create-namespace \
  --set observability.create=true \
  --set observability.influxdb.enabled=true \
  --set observability.influxdb.storageSize=100Gi \
  --set telegraf.enabled=true
```

### With Ingress (NGINX)

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

### With Custom Values File

Create a `values.yaml` file:

```yaml
ingress:
  enabled: true
  className: nginx
  hosts:
    - host: headwind.example.com
      paths:
        - path: /
          pathType: Prefix
          backend: ui

env:
  HEADWIND_POLLING_ENABLED: "true"
  HEADWIND_POLLING_INTERVAL: "300"
  HEADWIND_UI_URL: "https://headwind.example.com"

notifications:
  slack:
    enabled: true
    webhookUrl: "https://hooks.slack.com/services/YOUR/WEBHOOK/URL"

observability:
  create: true
  influxdb:
    enabled: true
    storageSize: 100Gi

telegraf:
  enabled: true

resources:
  limits:
    cpu: 1000m
    memory: 1Gi
  requests:
    cpu: 200m
    memory: 256Mi
```

Then install:

```bash
helm install headwind headwind/headwind -n headwind-system --create-namespace -f values.yaml
```

## Using Headwind

### Quick Start

After installation, add Headwind annotations to your Deployments, StatefulSets, DaemonSets, or HelmReleases:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-app
  annotations:
    headwind.sh/policy: "minor"              # Required: Update policy
    headwind.sh/require-approval: "true"     # Optional: Require manual approval (default: true)
    headwind.sh/min-update-interval: "300"   # Optional: Min seconds between updates (default: 300)
    headwind.sh/event-source: "webhook"      # Optional: webhook, polling, both (default: webhook)
spec:
  template:
    spec:
      containers:
      - name: my-app
        image: myregistry.io/my-app:v1.2.0
```

### Update Policies

- `patch` - Only patch version updates (1.2.3 → 1.2.4)
- `minor` - Only minor version updates (1.2.3 → 1.3.0)
- `major` - Any version update including major (1.2.3 → 2.0.0)
- `all` - Any new version
- `glob` - Pattern-based matching (requires `headwind.sh/pattern` annotation)
- `none` - Disable updates (default)

### Accessing the Web UI

The Web UI provides a dashboard for viewing and managing update requests.

**Via Port Forward:**

```bash
kubectl port-forward -n headwind-system svc/headwind 8082:8082
# Visit: http://localhost:8082
```

**Via Ingress:**

If you've enabled ingress, visit the configured hostname (e.g., `https://headwind.example.com`).

### Authentication Modes

Headwind supports four authentication modes for the Web UI:

1. **None** (default): No authentication
2. **Simple**: Username from HTTP header (`X-User`)
3. **Token**: Kubernetes TokenReview validation (bearer tokens)
4. **Proxy**: Ingress/proxy headers (e.g., `X-Forwarded-User`)

Configure via:

```yaml
env:
  HEADWIND_UI_AUTH_MODE: "token"  # or "simple", "proxy", "none"
  HEADWIND_UI_PROXY_HEADER: "X-Forwarded-User"  # for proxy mode
```

## Metrics

Headwind exposes Prometheus metrics on port 9090:

```bash
kubectl port-forward -n headwind-system svc/headwind 9090:9090
curl http://localhost:9090/metrics
```

Key metrics:
- `headwind_deployments_watched` - Number of Deployments being monitored
- `headwind_statefulsets_watched` - Number of StatefulSets being monitored
- `headwind_daemonsets_watched` - Number of DaemonSets being monitored
- `headwind_helm_releases_watched` - Number of HelmReleases being monitored
- `headwind_updates_pending` - Pending update requests
- `headwind_updates_applied_total` - Successfully applied updates
- `headwind_updates_failed_total` - Failed updates

See [metrics documentation](https://headwind.sh/docs/api/metrics) for the full list.

## Troubleshooting

### Headwind not watching my resources

Check the logs:

```bash
kubectl logs -n headwind-system deployment/headwind -c headwind
```

Verify annotations are correct:

```bash
kubectl get deployment my-app -o yaml | grep headwind
```

### Webhook events not being processed

Verify webhook configuration in your registry points to the correct endpoint:

```
http://headwind.headwind-system.svc.cluster.local:8080/webhook/registry
```

Or use port-forward for testing:

```bash
kubectl port-forward -n headwind-system svc/headwind 8080:8080
curl -X POST http://localhost:8080/webhook/registry \
  -H "Content-Type: application/json" \
  -d '{"repository":"myimage","tag":"v1.2.3"}'
```

### InfluxDB metrics not showing

1. Verify InfluxDB is running:

```bash
kubectl get pods -n headwind-system -l app.kubernetes.io/component=influxdb
```

2. Check Telegraf sidecar logs:

```bash
kubectl logs -n headwind-system deployment/headwind -c telegraf
```

3. Verify token configuration:

```bash
kubectl get configmap headwind-config -n headwind-system -o yaml | grep token
```

## Upgrading

### To 0.2.0

Version 0.2.0 includes InfluxDB observability integration. If upgrading from 0.1.0:

```bash
helm upgrade headwind headwind/headwind -n headwind-system --reuse-values
```

No breaking changes.

## Support

- Documentation: https://headwind.sh
- GitHub: https://github.com/headwind-sh/headwind
- Issues: https://github.com/headwind-sh/headwind/issues

## License

Apache 2.0 - See [LICENSE](https://github.com/headwind-sh/headwind/blob/main/LICENSE) for details.
