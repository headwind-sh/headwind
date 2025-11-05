# Headwind Setup Guide

This guide covers setting up Headwind for development and deployment.

## Table of Contents

- [Development Setup](#development-setup)
- [Pre-commit Hooks](#pre-commit-hooks)
- [GitHub Actions](#github-actions)
- [Docker Build](#docker-build)
- [Kubernetes Deployment](#kubernetes-deployment)

## Development Setup

### Prerequisites

- **Rust** 1.75 or later
- **Docker** (optional, for building images)
- **Kubernetes cluster** (optional, for testing)
- **Python** 3.7+ (for pre-commit)
- **Make** (optional, but recommended)

### Quick Start

```bash
# Clone the repository
git clone https://github.com/b1tsized/headwind.git
cd headwind

# Install all development tools
make install

# Set up pre-commit hooks
pre-commit install

# Build and test
make all

# Run locally (requires Kubernetes)
make run
```

### Manual Setup

If you don't use the Makefile:

```bash
# Install Rust tools
cargo install cargo-audit
cargo install cargo-deny
cargo install cargo-udeps
cargo install cargo-tarpaulin
cargo install cargo-watch

# Install Python tools
pip install pre-commit
pre-commit install

# Build
cargo build

# Test
cargo test

# Run
RUST_LOG=headwind=debug cargo run
```

## Pre-commit Hooks

Pre-commit hooks automatically run quality checks before each commit.

### Installation

```bash
# Install pre-commit (if not already installed)
pip install pre-commit

# Install hooks in your local repository
pre-commit install
```

### What Gets Checked

On every `git commit`:

1. **Rust Formatting** (`cargo fmt`)
   - Ensures consistent code style
   - Auto-fixes formatting issues

2. **Compilation** (`cargo check`)
   - Verifies code compiles
   - Catches syntax errors early

3. **Linting** (`cargo clippy`)
   - Detects common mistakes
   - Enforces best practices
   - Fails on warnings

4. **File Checks**
   - Trims trailing whitespace
   - Fixes end-of-file newlines
   - Validates YAML syntax
   - Checks for large files (>1MB)
   - Detects merge conflicts

5. **Security** (on push)
   - `cargo audit` - Vulnerability scanning
   - `cargo deny` - Dependency checking
   - Secret detection in filenames

### Running Manually

```bash
# Run all hooks on all files
pre-commit run --all-files

# Run specific hook
pre-commit run cargo-check --all-files

# Skip hooks for a commit (not recommended)
git commit --no-verify
```

### Troubleshooting

**Issue**: Pre-commit hooks fail
```bash
# Update hooks to latest versions
pre-commit autoupdate

# Clean and reinstall
pre-commit clean
pre-commit install
```

**Issue**: Hooks are slow
```bash
# Skip certain hooks in .pre-commit-config.yaml
# Set SKIP environment variable
SKIP=cargo-audit git commit -m "message"
```

## GitHub Actions

The project uses GitHub Actions for CI/CD. All checks run automatically on pull requests and pushes to main.

### Workflows

#### 1. CI Workflow (`.github/workflows/ci.yml`)

Runs on every PR and push to main:

- **Format Check**: Verifies code is formatted
- **Clippy Lints**: Runs linter with warnings as errors
- **Tests**: Runs on Ubuntu and macOS with stable/nightly Rust
- **Compilation**: Checks debug and release builds
- **Security Audit**: Scans for vulnerabilities
- **Dependency Check**: Validates licenses and sources
- **Unused Dependencies**: Detects unnecessary deps
- **Documentation**: Ensures docs build without errors
- **Docker Build**: Verifies Dockerfile
- **Kubernetes Validation**: Checks manifests

#### 2. Security Workflow (`.github/workflows/security.yml`)

Runs daily and on every push:

- **Cargo Audit**: Dependency vulnerability scanning
- **Gitleaks**: Secret detection in commits
- **Semgrep**: Static analysis security testing
- **Cargo Vet**: Supply chain security

#### 3. Release Workflow (`.github/workflows/release.yml`)

Triggered by version tags (e.g., `v0.1.0`):

- Builds binaries for multiple platforms
- Creates GitHub release
- Builds and pushes Docker images (multi-arch)
- Publishes to crates.io

#### 4. Dependabot

Automatically creates PRs for dependency updates:

- Weekly Cargo dependency updates
- Weekly GitHub Actions updates
- Auto-merges patch/minor version bumps

### Local CI Simulation

Run the same checks locally before pushing:

```bash
# Run all CI checks
make ci

# Individual checks
make fmt-check    # Formatting
make lint         # Clippy
make test         # Tests
make audit        # Security audit
make deny         # Dependency check
```

### Secrets Required

For full CI/CD, configure these GitHub secrets:

- `DOCKER_USERNAME` - Docker Hub username
- `DOCKER_PASSWORD` - Docker Hub token
- `CARGO_REGISTRY_TOKEN` - crates.io API token

## Docker Build

### Local Build

```bash
# Build image
make docker

# Or manually
docker build -t headwind:latest .

# Run container (requires Kubernetes config)
docker run --rm -it \
  -v ~/.kube:/root/.kube:ro \
  headwind:latest
```

### Multi-architecture Build

```bash
# Set up buildx
docker buildx create --use

# Build for multiple platforms
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  -t headwind:latest \
  --push \
  .
```

### Image Size

Expected image size: ~50-100MB (Debian slim + binary)

## Kubernetes Deployment

### Prerequisites

- Kubernetes 1.25+
- kubectl configured
- Cluster admin access (for RBAC)

### Deploy to Cluster

```bash
# Build and load image (for kind/minikube)
make kind-load

# Apply manifests
kubectl apply -f deploy/k8s/namespace.yaml
kubectl apply -f deploy/k8s/rbac.yaml
kubectl apply -f deploy/k8s/deployment.yaml
kubectl apply -f deploy/k8s/service.yaml

# Verify deployment
kubectl get pods -n headwind-system
kubectl logs -n headwind-system -l app=headwind
```

### Configuration

#### Enable Polling

```yaml
# deploy/k8s/deployment.yaml
env:
- name: HEADWIND_POLLING_ENABLED
  value: "true"
- name: HEADWIND_POLLING_INTERVAL
  value: "300"  # seconds
```

#### Adjust Resources

```yaml
resources:
  requests:
    memory: "128Mi"
    cpu: "100m"
  limits:
    memory: "512Mi"
    cpu: "500m"
```

### Access Services

```bash
# Webhook server (port 8080)
kubectl port-forward -n headwind-system svc/headwind-webhook 8080:80

# Approval API (port 8081)
kubectl port-forward -n headwind-system svc/headwind-api 8081:80

# Metrics (port 9090)
kubectl port-forward -n headwind-system svc/headwind-metrics 9090:9090
```

### Verify Installation

```bash
# Check webhook endpoint
curl http://localhost:8080/health

# Check approval API
curl http://localhost:8081/api/v1/updates

# Check metrics
curl http://localhost:9090/metrics
```

## Development Workflow

### Typical Development Cycle

```bash
# 1. Create branch
git checkout -b feature/my-feature

# 2. Make changes
vim src/...

# 3. Run quick checks
make quick

# 4. Run full test suite
make all

# 5. Commit (pre-commit hooks run automatically)
git commit -m "feat: add new feature"

# 6. Push (triggers GitHub Actions)
git push origin feature/my-feature

# 7. Create PR on GitHub
# All CI checks run automatically

# 8. Address feedback, repeat 2-6

# 9. Merge when approved
```

### Testing with Kubernetes

```bash
# Start a local cluster
kind create cluster --name headwind-dev

# Build and deploy
make build
make docker
make kind-load
kubectl apply -f deploy/k8s/

# Test with example deployment
kubectl apply -f examples/deployment-with-headwind.yaml

# Watch logs
kubectl logs -n headwind-system -f deployment/headwind

# Clean up
kind delete cluster --name headwind-dev
```

## Troubleshooting

### Pre-commit Issues

```bash
# Hooks not running
pre-commit install  # Reinstall

# Hooks failing on installed tools
make install  # Reinstall tools

# Bypass hooks (emergency only)
git commit --no-verify
```

### CI Failures

```bash
# Run CI checks locally
make ci

# Check specific failure
cargo clippy  # For lint failures
cargo test    # For test failures
cargo fmt     # For format failures
```

### Build Issues

```bash
# Clean and rebuild
make clean
make build

# Update dependencies
cargo update

# Check for compilation errors
cargo check
```

## Next Steps

- Read [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines
- Check [CLAUDE.md](CLAUDE.md) for architecture details
- Review [GitHub Issues](.github/issues/) for planned work
- Join discussions in GitHub Discussions

## Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [Cargo Book](https://doc.rust-lang.org/cargo/)
- [kube-rs Documentation](https://docs.rs/kube/latest/kube/)
- [Pre-commit Documentation](https://pre-commit.com/)
- [GitHub Actions Documentation](https://docs.github.com/en/actions)
