# Contributing to Headwind

Thank you for your interest in contributing to Headwind! This document provides guidelines and instructions for contributing.

## Code of Conduct

Be respectful, inclusive, and professional. We're all here to build something great together.

## Getting Started

### Prerequisites

- Rust 1.75 or later
- Docker (for building images)
- Kubernetes cluster (for testing)
  - [kind](https://kind.sigs.k8s.io/) (recommended for local development)
  - [minikube](https://minikube.sigs.k8s.io/)
  - Or any other Kubernetes cluster
- kubectl configured to access your cluster

### Development Setup

1. **Fork and clone the repository**

```bash
git clone https://github.com/YOUR_USERNAME/headwind.git
cd headwind
```

2. **Install development tools**

```bash
make install
# This installs:
# - cargo-audit (security auditing)
# - cargo-deny (dependency checking)
# - cargo-udeps (unused dependency detection)
# - cargo-tarpaulin (code coverage)
# - cargo-watch (file watcher)
# - pre-commit (git hooks)
```

3. **Set up pre-commit hooks**

```bash
pre-commit install
# Hooks will now run automatically on git commit
```

4. **Build the project**

```bash
make build
# or
cargo build
```

5. **Run tests**

```bash
make test
# or
cargo test
```

6. **Run locally (requires Kubernetes access)**

```bash
make run
# or
export RUST_LOG=headwind=debug,kube=debug
cargo run
```

### Using the Makefile

The project includes a Makefile with common development commands:

```bash
make help          # Show all available commands
make build         # Build the project
make test          # Run tests
make fmt           # Format code
make lint          # Run clippy
make check         # Check compilation
make all           # Run fmt, lint, and test
make ci            # Simulate CI checks locally
make quick         # Quick checks before commit
```

## Development Workflow

### 1. Create an Issue

Before starting work, create or comment on an issue describing:
- What you plan to do
- Why it's needed
- Your proposed approach

This helps avoid duplicate work and ensures alignment with project goals.

### 2. Create a Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/your-bug-fix
```

Branch naming conventions:
- `feature/` - New features
- `fix/` - Bug fixes
- `docs/` - Documentation updates
- `refactor/` - Code refactoring
- `test/` - Test additions/improvements

### 3. Make Your Changes

#### Code Style

- **Format code** before committing:
  ```bash
  make fmt
  # or
  cargo fmt
  ```

- **Run clippy** and fix warnings:
  ```bash
  make lint
  # or
  cargo clippy -- -D warnings
  ```

- **Pre-commit hooks** automatically run:
  - `cargo fmt` - Code formatting
  - `cargo check` - Compilation check
  - `cargo clippy` - Linting
  - YAML validation
  - Trailing whitespace removal
  - Secret detection

  To run manually:
  ```bash
  pre-commit run --all-files
  ```

- **Write tests** for new functionality
  - Unit tests in the same file as the code
  - Integration tests in `tests/` directory

- **Update documentation**
  - Add doc comments to public APIs
  - Update README.md if user-facing changes
  - Update CLAUDE.md if architectural changes

#### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): description

[optional body]

[optional footer]
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `test`: Adding or updating tests
- `chore`: Maintenance tasks

Examples:
```
feat(webhook): add support for GitHub Container Registry

fix(policy): handle version strings without 'v' prefix

docs(readme): add troubleshooting section

test(policy): add tests for glob pattern matching
```

### 4. Test Your Changes

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run with logging output
RUST_LOG=debug cargo test -- --nocapture

# Build release binary
cargo build --release

# Test in Kubernetes (requires cluster)
docker build -t headwind:test .
kind load docker-image headwind:test
kubectl apply -f deploy/k8s/
kubectl set image deployment/headwind -n headwind-system headwind=headwind:test
```

### 5. Submit a Pull Request

1. Push your branch to your fork
2. Open a PR against the `main` branch
3. Fill out the PR template completely
4. Link to the related issue
5. Wait for review

## PR Review Process

### What We Look For

- ✅ Tests pass
- ✅ Code is formatted (`cargo fmt`)
- ✅ No clippy warnings (`cargo clippy`)
- ✅ Documentation is updated
- ✅ Commit messages follow conventions
- ✅ PR description explains changes clearly
- ✅ No breaking changes (or clearly marked)

### Review Timeline

- Initial response: 1-3 days
- Full review: 3-7 days
- Merge after approval from at least one maintainer

### Addressing Feedback

- Make requested changes in new commits
- Don't force-push after review starts
- Respond to comments to confirm understanding
- Request re-review when ready

## Types of Contributions

### Bug Fixes

1. Create issue describing the bug
2. Include reproduction steps
3. Reference issue in PR
4. Add test that would have caught the bug

### New Features

1. Discuss in an issue first
2. Consider backwards compatibility
3. Add comprehensive tests
4. Update documentation
5. Add metrics where appropriate

### Documentation

- Fix typos and clarify explanations
- Add examples
- Improve error messages
- Update CLAUDE.md for architectural context

### Tests

- Increase code coverage
- Add integration tests
- Test edge cases
- Performance tests

## Project Structure

```
headwind/
├── src/
│   ├── main.rs              # Entry point
│   ├── approval/            # Approval API server
│   ├── controller/          # Kubernetes controllers
│   │   ├── deployment.rs    # Deployment controller
│   │   └── helm.rs          # Helm controller (stub)
│   ├── metrics/             # Prometheus metrics
│   ├── models/              # Data models
│   │   ├── policy.rs        # Policy types
│   │   ├── update.rs        # Update request types
│   │   └── webhook.rs       # Webhook payload types
│   ├── policy/              # Policy engine
│   └── webhook/             # Webhook server
├── deploy/k8s/              # Kubernetes manifests
├── examples/                # Example configurations
├── tests/                   # Integration tests (future)
└── docs/                    # Additional documentation (future)
```

## Writing Tests

### Unit Tests

Place tests in the same file as the code:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        // Arrange
        let input = "test";

        // Act
        let result = do_something(input);

        // Assert
        assert_eq!(result, expected);
    }
}
```

### Integration Tests

For testing multiple components together:

```rust
// tests/integration_test.rs
use headwind::*;

#[tokio::test]
async fn test_webhook_to_controller_flow() {
    // Test full flow
}
```

## Adding Metrics

All significant operations should have metrics:

```rust
// 1. Define metric in src/metrics/mod.rs
pub static ref MY_OPERATION_TOTAL: IntCounter = IntCounter::new(
    "headwind_my_operation_total",
    "Total number of my operations"
).unwrap();

// 2. Register in register_metrics()
REGISTRY.register(Box::new(MY_OPERATION_TOTAL.clone())).ok();

// 3. Use in your code
use crate::metrics::MY_OPERATION_TOTAL;
MY_OPERATION_TOTAL.inc();
```

## Adding Annotations

For new configuration options:

```rust
// 1. Add to src/models/policy.rs::annotations
pub const MY_ANNOTATION: &str = "headwind.sh/my-annotation";

// 2. Add field to ResourcePolicy
pub struct ResourcePolicy {
    // ...
    pub my_field: String,
}

// 3. Parse in controller
if let Some(value) = annotations.get(annotations::MY_ANNOTATION) {
    policy.my_field = value.to_string();
}

// 4. Document in README.md
```

## Performance Considerations

- Use `Arc` for shared state
- Prefer channels for communication between components
- Use `tokio::spawn` for concurrent operations
- Profile with `cargo flamegraph` if needed
- Keep reconciliation loops fast (<100ms typical)

## Security

- Never log sensitive data (tokens, secrets, etc.)
- Validate all external input (webhooks, API requests)
- Use parameterized queries if adding database
- Run as non-root user
- Use read-only filesystem where possible
- Follow least-privilege principle for RBAC

## Documentation

### Code Documentation

```rust
/// Short description of what this does.
///
/// Longer explanation if needed.
///
/// # Arguments
/// * `param` - Description of parameter
///
/// # Returns
/// Description of return value
///
/// # Errors
/// When this function errors
///
/// # Examples
/// ```
/// let result = do_something("input");
/// assert_eq!(result, "output");
/// ```
pub fn do_something(param: &str) -> Result<String> {
    // implementation
}
```

### README Updates

When adding user-facing features:
- Add to Features section
- Update Quick Start if needed
- Add to API Endpoints if applicable
- Update examples

### CLAUDE.md Updates

When changing architecture:
- Update component descriptions
- Update critical implementation gaps
- Add to design decisions
- Update troubleshooting

## Release Process

(For maintainers)

1. Update version in `Cargo.toml`
2. Update CHANGELOG.md
3. Create git tag: `git tag -a v0.2.0 -m "Release v0.2.0"`
4. Push tag: `git push origin v0.2.0`
5. Build and push Docker image
6. Create GitHub release with notes

## Getting Help

- **Questions**: Open an issue with `question` label
- **Bugs**: Open an issue with `bug` label
- **Feature Requests**: Open an issue with `enhancement` label
- **Security Issues**: Email security@example.com (do not open public issue)

## Resources

### Rust
- [The Rust Book](https://doc.rust-lang.org/book/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [Cargo Book](https://doc.rust-lang.org/cargo/)

### Kubernetes
- [kube-rs Documentation](https://docs.rs/kube/latest/kube/)
- [Kubernetes API Reference](https://kubernetes.io/docs/reference/kubernetes-api/)
- [Operator Pattern](https://kubernetes.io/docs/concepts/extend-kubernetes/operator/)

### Async Rust
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Async Book](https://rust-lang.github.io/async-book/)

### This Project
- [CLAUDE.md](./CLAUDE.md) - Architecture and development context
- [README.md](./README.md) - User documentation
- Examples in `examples/` directory

## License

By contributing, you agree that your contributions will be licensed under the MIT License.

---

Thank you for contributing to Headwind! 🎉
