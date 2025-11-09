# Web UI Authentication

The Headwind Web UI supports four authentication modes to meet different security requirements and deployment scenarios. All authentication modes include comprehensive audit logging to track who performed which actions.

## Authentication Modes

Configure authentication using the `HEADWIND_UI_AUTH_MODE` environment variable in your deployment.

### Mode 1: None (Default)

**No authentication required.** All actions are logged as "web-ui-user".

**Use Case**: Development environments, trusted internal networks, or when authentication is not required.

**Configuration**:
```yaml
env:
  - name: HEADWIND_UI_AUTH_MODE
    value: "none"
```

**Security Note**: This mode provides no access control. Only use in trusted environments.

---

### Mode 2: Simple Header Authentication

**Reads username from HTTP header.** The Web UI trusts the username provided in the `X-User` header.

**Use Case**: Behind an authenticating reverse proxy (e.g., nginx with `auth_request`, Apache with `mod_auth`).

**Configuration**:
```yaml
env:
  - name: HEADWIND_UI_AUTH_MODE
    value: "simple"
```

**Example Usage**:
```bash
# With curl
curl -H "X-User: alice" http://headwind-ui:8082/

# Behind nginx (configured with auth_request)
# nginx sets X-User header after successful authentication
```

**Nginx Example**:
```nginx
location / {
    auth_request /auth;
    proxy_pass http://headwind-ui:8082;
    proxy_set_header X-User $remote_user;
}
```

**Security Note**: The proxy/reverse proxy must validate authentication before setting the `X-User` header. Headwind trusts whatever username is provided.

---

### Mode 3: Kubernetes Token Authentication

**Validates bearer tokens using Kubernetes TokenReview API** and extracts the authenticated username.

**Use Case**:
- Service account authentication
- kubectl authentication
- Kubernetes-native authentication workflows
- Integration with Kubernetes RBAC

**Configuration**:
```yaml
env:
  - name: HEADWIND_UI_AUTH_MODE
    value: "token"
```

**Requirements**:
- RBAC permission for `authentication.k8s.io/tokenreviews` (already included in `deploy/k8s/rbac.yaml`)

**Example Usage with Service Account**:

1. Create a service account:
```bash
kubectl create serviceaccount headwind-ui-user -n default
```

2. Create a token:
```bash
TOKEN=$(kubectl create token headwind-ui-user -n default)
```

3. Access the Web UI:
```bash
curl -H "Authorization: Bearer $TOKEN" http://headwind-ui:8082/
```

**Example Usage with kubectl**:
```bash
# Get your current user token
TOKEN=$(kubectl config view --raw -o jsonpath='{.users[0].user.token}')

# Access Web UI
curl -H "Authorization: Bearer $TOKEN" http://headwind-ui:8082/
```

**How It Works**:
1. Client sends bearer token in `Authorization` header
2. Headwind calls Kubernetes TokenReview API to validate token
3. Kubernetes responds with authentication status and username
4. Username extracted from response (e.g., `system:serviceaccount:default:headwind-ui-user`)
5. Actions audited with full username

**Security Note**: Token validation is performed on every request. Expired or invalid tokens are rejected with 401 Unauthorized.

---

### Mode 4: Proxy/Ingress Authentication

**Reads username from a configurable HTTP header** set by an ingress controller or authentication proxy.

**Use Case**:
- Kubernetes Ingress with external authentication
- oauth2-proxy
- Authelia
- Keycloak Gatekeeper
- Any ingress controller with authentication

**Configuration**:
```yaml
env:
  - name: HEADWIND_UI_AUTH_MODE
    value: "proxy"
  - name: HEADWIND_UI_PROXY_HEADER  # Optional, defaults to X-Forwarded-User
    value: "X-Auth-Request-User"
```

**Common Header Names**:
- `X-Forwarded-User` (default, used by many proxies)
- `X-Auth-Request-User` (oauth2-proxy)
- `X-Forwarded-Email` (some authentication proxies)
- `Remote-User` (traditional proxy auth)

**Example with oauth2-proxy**:

```yaml
# oauth2-proxy configuration
apiVersion: v1
kind: ConfigMap
metadata:
  name: oauth2-proxy-config
data:
  oauth2-proxy.cfg: |
    email_domains = ["*"]
    upstreams = ["http://headwind-ui:8082"]
    pass_user_headers = true

---
# Headwind deployment
env:
  - name: HEADWIND_UI_AUTH_MODE
    value: "proxy"
  - name: HEADWIND_UI_PROXY_HEADER
    value: "X-Forwarded-Email"
```

**Example with Ingress Annotation** (nginx-ingress with oauth2-proxy):
```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: headwind-ui
  annotations:
    nginx.ingress.kubernetes.io/auth-url: "https://oauth2-proxy.example.com/oauth2/auth"
    nginx.ingress.kubernetes.io/auth-signin: "https://oauth2-proxy.example.com/oauth2/start"
    nginx.ingress.kubernetes.io/auth-response-headers: "X-Auth-Request-User,X-Auth-Request-Email"
spec:
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

**Security Note**: The proxy/ingress must validate authentication before setting the username header. Headwind trusts the header value.

---

## Audit Logging

All authentication modes produce detailed audit logs for approval and rejection actions.

### Audit Log Format

```json
{
  "timestamp": "2025-11-08T23:00:00Z",
  "username": "alice",
  "action": "approve",
  "resource_type": "Deployment",
  "namespace": "default",
  "name": "nginx-update-1-26-0",
  "result": "success",
  "reason": null
}
```

**Fields**:
- `timestamp`: RFC3339 timestamp
- `username`: Authenticated username (varies by auth mode)
- `action`: `approve` or `reject`
- `resource_type`: Deployment, StatefulSet, DaemonSet, or HelmRelease
- `namespace`: Kubernetes namespace
- `name`: UpdateRequest name
- `result`: `success` or `error`
- `reason`: Rejection reason (only for rejections)

### Viewing Audit Logs

Audit logs use the dedicated log target `headwind::audit`:

```bash
# Filter audit logs only
kubectl logs -n headwind-system deployment/headwind | grep "headwind::audit"

# Follow audit logs in real-time
kubectl logs -n headwind-system deployment/headwind -f | grep "headwind::audit"

# Export audit logs to file
kubectl logs -n headwind-system deployment/headwind | grep "headwind::audit" > audit.log
```

### Username by Authentication Mode

| Mode | Example Username |
|------|------------------|
| None | `web-ui-user` |
| Simple | `alice` (from X-User header) |
| Token | `system:serviceaccount:default:my-sa` |
| Proxy | `alice@example.com` (from configured header) |

## RBAC Requirements

### Token Authentication Mode

Token authentication requires the `authentication.k8s.io/tokenreviews` permission:

```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: headwind
rules:
  # Required for token authentication
  - apiGroups: ["authentication.k8s.io"]
    resources: ["tokenreviews"]
    verbs: ["create"]
```

This permission is already included in `deploy/k8s/rbac.yaml`.

### Other Modes

No additional RBAC permissions required for None, Simple, or Proxy modes.

## Security Best Practices

1. **Use Token or Proxy mode in production** - Avoid "none" and "simple" modes unless behind a trusted authentication layer

2. **Enable audit logging** - Always monitor audit logs for suspicious activity

3. **Use HTTPS/TLS** - Always expose the Web UI over HTTPS in production via Ingress

4. **Limit network access** - Use NetworkPolicies to restrict access to the UI service

5. **Rotate tokens regularly** - For token mode, rotate service account tokens periodically

6. **Validate proxy configuration** - Ensure your authentication proxy cannot be bypassed

## Example Deployments

### Development (No Auth)

```yaml
env:
  - name: HEADWIND_UI_AUTH_MODE
    value: "none"
```

### Production with oauth2-proxy

```yaml
env:
  - name: HEADWIND_UI_AUTH_MODE
    value: "proxy"
  - name: HEADWIND_UI_PROXY_HEADER
    value: "X-Forwarded-Email"
```

### Production with Service Accounts

```yaml
env:
  - name: HEADWIND_UI_AUTH_MODE
    value: "token"
```

## Troubleshooting

### "Missing Authorization header" (Token Mode)

**Problem**: 401 error with message about missing header

**Solution**: Include bearer token in Authorization header:
```bash
curl -H "Authorization: Bearer <token>" http://headwind-ui:8082/
```

### "Token validation failed" (Token Mode)

**Problem**: Valid-looking token rejected

**Possible Causes**:
1. Token expired (service account tokens can expire)
2. Service account deleted
3. RBAC permission missing for TokenReview
4. Network connectivity to Kubernetes API

**Solution**:
```bash
# Check RBAC permissions
kubectl auth can-i create tokenreviews.authentication.k8s.io --as=system:serviceaccount:headwind-system:headwind

# Create fresh token
kubectl create token <service-account> -n <namespace>
```

### "Missing X-Forwarded-User header" (Proxy Mode)

**Problem**: 401 error about missing header

**Possible Causes**:
1. Proxy not configured to pass username header
2. Wrong header name configured

**Solution**: Verify proxy configuration and ensure `HEADWIND_UI_PROXY_HEADER` matches your proxy's header name

### Audit logs show "web-ui-user" instead of username

**Problem**: All audit entries show "web-ui-user"

**Cause**: Authentication mode is set to "none"

**Solution**: Configure proper authentication mode (simple, token, or proxy)

## Next Steps

- [Observability Dashboard](./observability-dashboard.md) - Monitor metrics
- [Configuration Management](../configuration/web-ui.md) - Customize Web UI settings
