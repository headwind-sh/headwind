# Headwind Custom Resource Definitions (CRDs)

This directory contains the CRDs required by Headwind.

## Required CRDs

### updaterequest.yaml
**Always required** - This is Headwind's core CRD for tracking update requests.

```bash
kubectl apply -f updaterequest.yaml
```

## Optional CRDs

### helmrepository.yaml
**Required for Helm chart auto-discovery** - Only needed if you want Headwind to automatically discover new Helm chart versions from Helm repositories.

**When to apply:**
- ✅ You want automatic Helm chart version discovery
- ✅ You DON'T have Flux CD installed (Flux CD already provides this CRD)

**Skip if:**
- ❌ You already have Flux CD installed (CRD already exists)
- ❌ You only use Deployments (not HelmReleases)
- ❌ You don't need automatic Helm version discovery

```bash
# Check if CRD already exists (from Flux CD)
kubectl get crd helmrepositories.source.toolkit.fluxcd.io

# If not found, apply it
kubectl apply -f helmrepository.yaml
```

## Generating CRDs

The CRDs are generated from Rust code using the `kube` crate's `CustomResourceExt` trait.

To regenerate the HelmRepository CRD:
```bash
cargo run --example generate_helmrepo_crd 2>/dev/null > deploy/k8s/crds/helmrepository.yaml
```

## API Groups

- **UpdateRequest**: `headwind.sh/v1alpha1`
- **HelmRepository**: `source.toolkit.fluxcd.io/v1` (Flux CD compatible)
