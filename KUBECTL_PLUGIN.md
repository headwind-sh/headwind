# kubectl-headwind Plugin

A kubectl plugin for managing Headwind image updates and rollbacks directly from the command line.

## Installation

### Manual Installation

```bash
# Copy the plugin to your PATH
sudo cp kubectl-headwind /usr/local/bin/

# Make it executable
sudo chmod +x /usr/local/bin/kubectl-headwind

# Verify installation
kubectl headwind help
```

### Using Krew (Future)

```bash
# Once published to krew
kubectl krew install headwind
```

## Prerequisites

- `kubectl` installed and configured
- `jq` (for JSON formatting)
- `curl` (for API calls)
- Access to the Headwind API (via port-forward or external URL)

## Configuration

The plugin connects to the Headwind API. There are several ways to configure the connection:

### Port Forwarding (Default)

```bash
# In one terminal, port-forward the Headwind API
kubectl port-forward -n headwind-system svc/headwind-api 8081:8081

# In another terminal, use the plugin
kubectl headwind list
```

### External URL

If Headwind is exposed externally:

```bash
export HEADWIND_API_URL=https://headwind.example.com
kubectl headwind list
```

### Custom Service Name

If you've deployed Headwind with a custom service name:

```bash
export HEADWIND_API_SERVICE=my-headwind-api.my-namespace.svc.cluster.local:8081
kubectl headwind list
```

## Usage

### Rollback a Deployment

Rollback a deployment to its previous image:

```bash
# Rollback (auto-detects first container)
kubectl headwind rollback nginx-deployment -n production

# Rollback a specific container
kubectl headwind rollback nginx-deployment nginx -n production
```

### View Update History

See the history of updates for a deployment:

```bash
kubectl headwind history nginx-deployment -n production
```

Output example:
```
Container | Image           | Timestamp            | Approved By       | Update Request
----------|-----------------|----------------------|-------------------|------------------
nginx     | nginx:1.26.0    | 2025-11-06T10:30:00Z | admin@example.com | nginx-update-v1-26-0
nginx     | nginx:1.25.0    | 2025-11-05T14:20:00Z | webhook           | nginx-update-v1-25-0
```

### List Pending Updates

View all pending update requests across all namespaces:

```bash
kubectl headwind list
```

Output example:
```
Namespace  | Name                 | Deployment       | Current Image | New Image    | Phase
-----------|----------------------|------------------|---------------|--------------|--------
production | nginx-update-v1-27-0 | nginx-deployment | nginx:1.26.0  | nginx:1.27.0 | Pending
staging    | redis-update-v7-2-0  | redis-deployment | redis:7.0.0   | redis:7.2.0  | Pending
```

### Approve an Update

Approve a pending update request:

```bash
kubectl headwind approve nginx-update-v1-27-0 -n production --approver admin@example.com
```

Using environment variable for approver:

```bash
export HEADWIND_APPROVER=admin@example.com
kubectl headwind approve nginx-update-v1-27-0 -n production
```

### Reject an Update

Reject a pending update request with a reason:

```bash
kubectl headwind reject nginx-update-v1-27-0 "Not ready for production" -n production --approver admin@example.com
```

## Command Reference

### `rollback`

```bash
kubectl headwind rollback <deployment> [container] [options]
```

Roll back a deployment to its previous image.

**Arguments:**
- `deployment` - Name of the deployment to rollback (required)
- `container` - Name of the container (optional, defaults to first container)

**Options:**
- `-n, --namespace` - Namespace (defaults to current context namespace)
- `--api-url` - Custom Headwind API URL

**Examples:**
```bash
kubectl headwind rollback nginx-deployment
kubectl headwind rollback nginx-deployment nginx -n production
```

### `history`

```bash
kubectl headwind history <deployment> [options]
```

Show update history for a deployment.

**Arguments:**
- `deployment` - Name of the deployment (required)

**Options:**
- `-n, --namespace` - Namespace (defaults to current context namespace)
- `--api-url` - Custom Headwind API URL

**Examples:**
```bash
kubectl headwind history nginx-deployment
kubectl headwind history nginx-deployment -n production
```

### `list`

```bash
kubectl headwind list [options]
```

List all pending update requests.

**Options:**
- `--api-url` - Custom Headwind API URL

**Examples:**
```bash
kubectl headwind list
```

### `approve`

```bash
kubectl headwind approve <update-request> [options]
```

Approve a pending update request.

**Arguments:**
- `update-request` - Name of the UpdateRequest CRD (required)

**Options:**
- `-n, --namespace` - Namespace (defaults to current context namespace)
- `--approver` - Email of the approver (defaults to $HEADWIND_APPROVER or "system")
- `--api-url` - Custom Headwind API URL

**Examples:**
```bash
kubectl headwind approve nginx-update-v1-27-0 --approver admin@example.com
export HEADWIND_APPROVER=admin@example.com
kubectl headwind approve nginx-update-v1-27-0 -n production
```

### `reject`

```bash
kubectl headwind reject <update-request> [reason] [options]
```

Reject a pending update request.

**Arguments:**
- `update-request` - Name of the UpdateRequest CRD (required)
- `reason` - Reason for rejection (optional, defaults to "No reason provided")

**Options:**
- `-n, --namespace` - Namespace (defaults to current context namespace)
- `--approver` - Email of the approver (defaults to $HEADWIND_APPROVER or "system")
- `--api-url` - Custom Headwind API URL

**Examples:**
```bash
kubectl headwind reject nginx-update-v1-27-0 "Not ready for production" --approver admin@example.com
kubectl headwind reject nginx-update-v1-27-0 -n production
```

## Environment Variables

- `HEADWIND_API_URL` - Override the default API URL (default: `http://headwind-api.headwind-system.svc.cluster.local:8081`)
- `HEADWIND_API_SERVICE` - Override the default service name (default: `headwind-api.headwind-system.svc.cluster.local:8081`)
- `HEADWIND_APPROVER` - Default approver email for approve/reject operations

## Troubleshooting

### Cannot reach Headwind API

If you see this error:
```
Cannot reach Headwind API at http://headwind-api.headwind-system.svc.cluster.local:8081
```

**Solution 1: Port Forward**
```bash
kubectl port-forward -n headwind-system svc/headwind-api 8081:8081
```

**Solution 2: Use External URL**
```bash
export HEADWIND_API_URL=https://headwind.example.com
```

### jq not installed

If `jq` is not installed, you'll see raw JSON output. Install it for better formatting:

```bash
# macOS
brew install jq

# Ubuntu/Debian
sudo apt-get install jq

# RHEL/CentOS
sudo yum install jq
```

### Permission Denied

If you get permission denied:
```bash
chmod +x /usr/local/bin/kubectl-headwind
```

## Examples

### Complete Workflow

```bash
# 1. Port forward the API (in background or separate terminal)
kubectl port-forward -n headwind-system svc/headwind-api 8081:8081 &

# 2. List pending updates
kubectl headwind list

# 3. View deployment history before approving
kubectl headwind history nginx-deployment -n production

# 4. Approve an update
kubectl headwind approve nginx-update-v1-27-0 -n production --approver admin@example.com

# 5. If something goes wrong, rollback
kubectl headwind rollback nginx-deployment -n production

# 6. Check history to confirm rollback
kubectl headwind history nginx-deployment -n production
```

### Using with CI/CD

```bash
#!/bin/bash
# Example CI/CD script

# Set approver for automated approvals
export HEADWIND_APPROVER=ci-bot@example.com
export HEADWIND_API_URL=https://headwind-api.production.example.com

# Approve all pending updates in staging
kubectl headwind list | grep staging | awk '{print $2, $1}' | while read name ns; do
    kubectl headwind approve "$name" -n "$ns"
done

# Monitor deployments and rollback on failure
# (This is handled automatically by Headwind if auto-rollback is enabled)
```

## Contributing

Found a bug or have a feature request? Please open an issue at:
https://github.com/b1tsized/headwind/issues

## License

MIT License - see LICENSE file for details.
