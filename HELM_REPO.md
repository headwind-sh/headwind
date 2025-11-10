# Helm Repository Setup

This document explains how Headwind's Helm chart is published and how to use it.

## For Users: Installing from the Helm Repository

Add the Headwind Helm repository:

```bash
helm repo add headwind https://headwind.sh/charts
helm repo update
```

Install Headwind:

```bash
helm install headwind headwind/headwind -n headwind-system --create-namespace
```

Search for available versions:

```bash
helm search repo headwind
```

Install a specific version:

```bash
helm install headwind headwind/headwind --version 0.1.0 -n headwind-system --create-namespace
```

## How It Works

### GitHub Pages Hosting

The Helm chart is automatically published to GitHub Pages when changes are pushed to the `main` branch. The workflow:

1. **Chart Packaging**: The `chart-releaser` GitHub Action packages the chart from `charts/headwind/`
2. **GitHub Release**: Creates a GitHub release with the chart package (`.tgz` file)
3. **Index Update**: Updates `index.yaml` in the `gh-pages` branch under `charts/` subdirectory
4. **GitHub Pages**: Serves the chart repository from `https://headwind.sh/charts`

### GitHub Actions Workflow

The `.github/workflows/helm-release.yml` workflow triggers on:
- Push to `main` branch (if chart files changed)
- Manual trigger via `workflow_dispatch`

### Chart Versioning

Chart versions are managed in `charts/headwind/Chart.yaml`:

```yaml
apiVersion: v2
name: headwind
version: 0.1.0  # Chart version
appVersion: "0.1.0"  # Application version
```

**Important**: Increment the `version` field before merging to `main` to trigger a new release.

### Repository Configuration

The `.github/cr.yaml` file configures the chart-releaser:

```yaml
owner: headwind-sh
git-repo: headwind
charts-repo-url: https://headwind.sh/charts
```

## For Maintainers: Publishing a New Chart Version

### 1. Update Chart Version

Edit `charts/headwind/Chart.yaml`:

```yaml
version: 0.2.0  # Increment this
appVersion: "0.2.0"  # Update if application version changed
```

### 2. Update Chart README

Update `charts/headwind/README.md` with any new parameters or changes.

### 3. Test Locally

```bash
# Lint the chart
helm lint charts/headwind/

# Template the chart
helm template test charts/headwind/ -n headwind-system

# Install locally for testing
helm install headwind charts/headwind/ -n headwind-system --create-namespace
```

### 4. Commit and Push

```bash
git add charts/headwind/
git commit -m "chore(helm): bump chart version to 0.2.0"
git push origin main
```

### 5. Workflow Execution

The GitHub Action will:
1. Package the chart
2. Create a GitHub release (tag: `headwind-0.2.0`)
3. Upload the chart package to the release
4. Update the `gh-pages` branch with the new `index.yaml`

### 6. Verify

Check the release was created:

```bash
gh release list
```

Verify the chart is available:

```bash
helm repo update
helm search repo headwind --versions
```

## Enabling GitHub Pages

### First-Time Setup

1. **Go to Repository Settings** → **Pages**
2. **Source**: Deploy from a branch
3. **Branch**: `gh-pages`
4. **Folder**: `/ (root)`
5. **Save**

GitHub will automatically deploy the `gh-pages` branch to `https://headwind-sh.github.io/headwind`.

### Verifying GitHub Pages

After the workflow runs, visit:

```
https://headwind.sh/charts/index.yaml
```

This should show the Helm repository index.

## Troubleshooting

### Chart Not Appearing in Repository

1. Check workflow ran successfully:
   ```bash
   gh run list --workflow=helm-release.yml
   ```

2. Verify `gh-pages` branch exists and has `index.yaml`:
   ```bash
   git fetch origin
   git ls-tree -r origin/gh-pages
   ```

3. Ensure chart version was incremented in `Chart.yaml`

### GitHub Pages Not Serving

1. Check repository settings → Pages is enabled
2. Verify `gh-pages` branch is selected as source
3. Check GitHub Actions permissions:
   - Settings → Actions → General → Workflow permissions
   - Enable "Read and write permissions"

### Chart Version Conflicts

The `chart-releaser` action will skip releases if a version already exists. Always increment the version in `Chart.yaml`.

## Manual Publishing (Alternative)

If you need to publish manually without GitHub Actions:

```bash
# Install chart-releaser
brew install chart-releaser

# Package the chart
helm package charts/headwind/

# Create GitHub release and upload chart
cr upload -o headwind-sh -r headwind -p .cr-release-packages

# Update index
cr index -o headwind-sh -r headwind -c https://headwind-sh.github.io/headwind -i index.yaml

# Push to gh-pages branch
git checkout gh-pages
git add index.yaml
git commit -m "Update index"
git push origin gh-pages
```

## Resources

- [Helm Chart Releaser](https://github.com/helm/chart-releaser)
- [Helm Chart Releaser Action](https://github.com/helm/chart-releaser-action)
- [GitHub Pages Documentation](https://docs.github.com/en/pages)
- [Helm Chart Repository Guide](https://helm.sh/docs/topics/chart_repository/)
