# Issue #4: Add Integration Tests

**Labels**: `testing`, `medium-priority`

## Description

Currently only unit tests exist. We need comprehensive integration tests that verify the full workflow with a real Kubernetes cluster.

## Current State

- ✅ 9 unit tests passing
- ❌ No integration tests
- ❌ No end-to-end tests
- ❌ No CI/CD pipeline

## What Needs to Be Done

### 1. Set Up Test Framework

```toml
# In Cargo.toml [dev-dependencies]
kube = { version = "0.97", features = ["test"] }
tokio-test = "0.4"
serial_test = "3.0"
```

### 2. Create Integration Test Suite

```rust
// tests/integration_tests.rs
use headwind::*;
use kube::Client;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn test_full_webhook_flow() {
    // 1. Create test deployment
    let client = Client::try_default().await.unwrap();
    let deployment = create_test_deployment();

    // 2. Send webhook event
    let event = ImagePushEvent { /* ... */ };
    send_webhook_event(event).await;

    // 3. Verify UpdateRequest created
    tokio::time::sleep(Duration::from_secs(1)).await;
    let updates = get_pending_updates().await;
    assert_eq!(updates.len(), 1);

    // 4. Approve update
    approve_update(&updates[0].id).await;

    // 5. Verify deployment updated
    tokio::time::sleep(Duration::from_secs(2)).await;
    let updated = get_deployment(&client, "test").await;
    assert_eq!(updated.spec.template.spec.containers[0].image, "new-image");
}
```

### 3. Test Scenarios to Cover

#### Basic Flow Tests
- [ ] Webhook → UpdateRequest → Approval → Update Applied
- [ ] Webhook with no approval required → Direct update
- [ ] Multiple deployments using same image
- [ ] Deployment with multiple containers

#### Policy Tests
- [ ] Patch policy allows 1.0.0 → 1.0.1, blocks 1.1.0
- [ ] Minor policy allows 1.0.0 → 1.1.0, blocks 2.0.0
- [ ] Major policy allows all updates
- [ ] Glob policy matches pattern correctly
- [ ] Force policy updates regardless of version
- [ ] None policy blocks all updates

#### Edge Cases
- [ ] Deployment without annotations (should be ignored)
- [ ] Invalid semver tags
- [ ] Image update for non-existent deployment
- [ ] Update rejection workflow
- [ ] Concurrent updates to same deployment
- [ ] Network failures during update

#### Interval Tests
- [ ] Min update interval is respected
- [ ] Force flag bypasses interval check
- [ ] First update always allowed

### 4. Test Helpers

```rust
// tests/helpers.rs
use k8s_openapi::api::apps::v1::Deployment;

pub async fn create_test_deployment(name: &str, annotations: HashMap<String, String>) -> Deployment {
    let client = Client::try_default().await.unwrap();
    let deployments: Api<Deployment> = Api::namespaced(client, "default");

    let deployment = Deployment {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            annotations: Some(annotations),
            ..Default::default()
        },
        spec: Some(DeploymentSpec { /* ... */ }),
        ..Default::default()
    };

    deployments.create(&PostParams::default(), &deployment).await.unwrap()
}

pub async fn cleanup_test_resources() {
    // Delete test deployments
    // Delete test namespaces
    // Clear approval store
}
```

### 5. Set Up CI/CD

```yaml
# .github/workflows/test.yml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Run unit tests
        run: cargo test --lib

      - name: Set up kind cluster
        uses: helm/kind-action@v1

      - name: Run integration tests
        run: cargo test --test integration_tests

      - name: Check formatting
        run: cargo fmt -- --check

      - name: Run clippy
        run: cargo clippy -- -D warnings
```

### 6. Performance Tests

```rust
#[tokio::test]
async fn test_high_webhook_volume() {
    // Send 100 webhook events rapidly
    for i in 0..100 {
        send_webhook_event(create_event(i)).await;
    }

    // Verify all processed within reasonable time
    tokio::time::timeout(
        Duration::from_secs(30),
        wait_for_all_processed()
    ).await.unwrap();
}

#[tokio::test]
async fn test_many_deployments() {
    // Create 50 deployments with headwind
    for i in 0..50 {
        create_test_deployment(&format!("test-{}", i), test_annotations()).await;
    }

    // Send webhook
    send_webhook_event(test_event()).await;

    // Verify all matched deployments got UpdateRequests
    let updates = get_pending_updates().await;
    assert!(updates.len() >= 50);
}
```

## Acceptance Criteria

- [ ] Integration test framework set up
- [ ] At least 20 integration tests covering main scenarios
- [ ] Tests run in CI/CD pipeline
- [ ] Test helpers for common operations
- [ ] Cleanup after tests
- [ ] Performance tests for scale
- [ ] Documentation for running tests
- [ ] Tests are deterministic and reliable

## Files to Create

- `tests/integration_tests.rs` - Main test suite
- `tests/helpers.rs` - Test utilities
- `tests/policy_integration.rs` - Policy-specific tests
- `tests/performance.rs` - Performance tests
- `.github/workflows/test.yml` - CI pipeline
- `.github/workflows/release.yml` - Release pipeline

## Running Tests

```bash
# Unit tests only
cargo test --lib

# Integration tests only (requires k8s cluster)
cargo test --test integration_tests

# All tests
cargo test

# With logging
RUST_LOG=debug cargo test -- --nocapture

# Specific test
cargo test test_full_webhook_flow
```

## Related Issues

- Blocks: Production readiness
- Depends on: #1, #2 (core features)

## Estimated Effort

Large (12-20 hours)
