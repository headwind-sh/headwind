# Headwind Helm Charts

This directory contains the Helm chart repository index for Headwind.

The chart repository is automatically updated by the helm-release workflow when changes are made to charts in the repository.

## Usage

Add the Helm repository:

```bash
helm repo add headwind https://headwind.sh/charts
helm repo update
```

Install Headwind:

```bash
helm install headwind headwind/headwind -n headwind-system --create-namespace
```

For more information, see the [Helm Installation Guide](https://headwind.sh/docs/guides/helm-installation).
