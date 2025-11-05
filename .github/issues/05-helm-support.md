# Issue #5: Implement Helm Chart Update Support

**Labels**: `enhancement`, `low-priority`, `help-wanted`

## Description

Add support for automatically updating Helm releases when new chart versions are available. This follows the Flux CD HelmRelease CRD pattern.

## Current State

- ✅ Stub exists in `src/controller/helm.rs`
- ❌ No actual Helm integration
- ❌ No chart repository querying
- ❌ No HelmRelease watching

## What Needs to Be Done

### 1. Add Helm Dependencies

```toml
[dependencies]
# Add to Cargo.toml
helm = "0.3"  # Or appropriate helm-rs crate
```

### 2. Define HelmRelease CRD

```rust
// src/models/helm.rs
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "helm.toolkit.fluxcd.io",
    version = "v2beta1",
    kind = "HelmRelease",
    namespaced
)]
pub struct HelmReleaseSpec {
    pub chart: ChartSpec,
    pub interval: String,
    pub values: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct ChartSpec {
    pub spec: ChartTemplateSpec,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct ChartTemplateSpec {
    pub chart: String,
    pub version: Option<String>,
    pub source_ref: SourceRef,
}
```

### 3. Implement Helm Controller

```rust
// src/controller/helm.rs
pub struct HelmController {
    client: Client,
    policy_engine: Arc<PolicyEngine>,
}

impl HelmController {
    pub async fn run(self) {
        let helm_releases: Api<HelmRelease> = Api::all(self.client.clone());

        Controller::new(helm_releases, Config::default())
            .run(reconcile_helm, error_policy, context)
            .for_each(|res| async move {
                // Handle reconciliation results
            })
            .await;
    }
}

async fn reconcile_helm(release: Arc<HelmRelease>, ctx: Arc<Context>) -> Result<Action> {
    // 1. Get current chart version
    // 2. Query chart repository for available versions
    // 3. Check policy
    // 4. Create UpdateRequest if new version available
}
```

### 4. Query Chart Repositories

```rust
async fn get_latest_chart_version(
    chart_name: &str,
    repo_url: &str,
) -> Result<String> {
    // Query Helm repository index.yaml
    // Parse available versions
    // Return latest matching policy
}
```

### 5. Update HelmRelease

```rust
async fn update_helm_release(
    client: Client,
    namespace: &str,
    name: &str,
    new_version: &str,
) -> Result<()> {
    let releases: Api<HelmRelease> = Api::namespaced(client, namespace);

    let patch = json!({
        "spec": {
            "chart": {
                "spec": {
                    "version": new_version
                }
            }
        }
    });

    releases.patch(
        name,
        &PatchParams::default(),
        &Patch::Merge(patch),
    ).await?;

    Ok(())
}
```

## Acceptance Criteria

- [ ] HelmRelease CRD support added
- [ ] Controller watches HelmRelease resources
- [ ] Chart repositories can be queried
- [ ] Annotations work on HelmRelease
- [ ] Policy engine applies to chart versions
- [ ] Approval workflow works for Helm
- [ ] Metrics track Helm updates
- [ ] Tests cover Helm scenarios
- [ ] Documentation updated

## Example Usage

```yaml
apiVersion: helm.toolkit.fluxcd.io/v2beta1
kind: HelmRelease
metadata:
  name: my-app
  namespace: default
  annotations:
    headwind.sh/policy: "minor"
    headwind.sh/require-approval: "true"
spec:
  interval: 5m
  chart:
    spec:
      chart: my-app
      version: "1.2.0"
      sourceRef:
        kind: HelmRepository
        name: my-repo
```

## Resources

- Flux CD HelmRelease: https://fluxcd.io/flux/components/helm/helmreleases/
- helm-rs crate: https://crates.io/crates/helm

## Related Issues

- Related to: #1, #2 (core functionality)

## Estimated Effort

Large (16-24 hours)
