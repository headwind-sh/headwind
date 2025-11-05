# Issue #1: Connect Webhook Events to Controller

**Labels**: `enhancement`, `high-priority`, `good-first-issue`

## Description

Currently, webhook events are received by the webhook server but not processed. We need to connect the webhook event pipeline to the Kubernetes controller so that image push events trigger update checks.

## Current State

- ✅ Webhook server receives events from registries
- ✅ Events are parsed and normalized into `ImagePushEvent`
- ✅ Events are sent to a processing channel
- ❌ Events are not queried against Kubernetes resources
- ❌ No UpdateRequests are created

## What Needs to Be Done

### 1. Query Kubernetes for Matching Deployments

In `src/webhook/mod.rs::process_webhook_events()`:

```rust
async fn process_webhook_events(mut rx: EventReceiver) {
    // Get Kubernetes client
    let client = Client::try_default().await.unwrap();
    let deployments: Api<Deployment> = Api::all(client.clone());

    while let Some(event) = rx.recv().await {
        // 1. List all Deployments with headwind annotations
        // 2. For each deployment, extract container images
        // 3. Match against event.full_image()
        // 4. If match found, proceed to policy check
    }
}
```

### 2. Extract Current and New Versions

```rust
// From deployment spec
let current_image = container.image; // e.g., "nginx:1.25.0"
let (image_name, current_tag) = parse_image(&current_image);

// From webhook event
let new_tag = event.tag; // e.g., "1.26.0"
```

### 3. Check Policy

```rust
let policy = parse_policy_from_annotations(&deployment.metadata.annotations);
let policy_engine = PolicyEngine;

if policy_engine.should_update(&policy, &current_tag, &new_tag)? {
    // Proceed with update or create approval request
}
```

### 4. Create UpdateRequest or Apply Directly

```rust
if policy.require_approval {
    // Create UpdateRequest for approval
    let update_request = UpdateRequest {
        id: Uuid::new_v4().to_string(),
        namespace: deployment.namespace().unwrap(),
        resource_name: deployment.name_any(),
        resource_kind: ResourceKind::Deployment,
        current_image,
        new_image: event.full_image(),
        created_at: Utc::now(),
        status: UpdateStatus::PendingApproval,
    };

    // Store in approval system
    // Increment UPDATES_PENDING metric
} else {
    // Apply update directly
    update_deployment_image(client, &namespace, &name, &container_name, &new_image).await?;
    // Increment UPDATES_APPLIED metric
}
```

## Acceptance Criteria

- [ ] Webhook events trigger deployment queries
- [ ] Container images are extracted and matched correctly
- [ ] Policy engine is consulted for update decisions
- [ ] UpdateRequests are created when approval required
- [ ] Direct updates happen when no approval required
- [ ] Metrics are updated appropriately
- [ ] Error handling is comprehensive
- [ ] Unit tests added for new functions
- [ ] Integration test with mock webhook event

## Files to Modify

- `src/webhook/mod.rs` - Main implementation
- `src/controller/deployment.rs` - May need helper functions
- `src/metrics/mod.rs` - Increment relevant metrics
- `tests/webhook_integration.rs` (new) - Integration tests

## Related Issues

- Depends on: None
- Blocks: #2 (Implement Update Application)

## Resources

- See CLAUDE.md section "Critical Implementation Gaps #1"
- kube-rs API docs: https://docs.rs/kube/latest/kube/

## Estimated Effort

Medium (4-8 hours)
