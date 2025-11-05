# Issue #2: Implement Update Application Logic

**Labels**: `enhancement`, `high-priority`

## Description

The approval system can approve/reject updates, but those approvals don't actually trigger Kubernetes resource updates. We need to connect the approval flow to the controller's update application logic.

## Current State

- ✅ Approval API can approve/reject UpdateRequests
- ✅ `update_deployment_image()` function exists
- ❌ Approved updates don't trigger actual Kubernetes changes
- ❌ No feedback loop from approval to controller
- ❌ No Kubernetes events emitted

## What Needs to Be Done

### 1. Add Update Application Watcher

Create a background task that watches for approved updates:

```rust
// In src/approval/mod.rs
pub fn watch_approved_updates(store: UpdateStore) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let updates = store.read().await;
            for (id, update) in updates.iter() {
                if update.status == UpdateStatus::Approved {
                    // Apply the update
                    apply_update(update).await;
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    })
}
```

### 2. Implement Update Application

```rust
async fn apply_update(update: &UpdateRequest) -> Result<()> {
    let client = Client::try_default().await?;

    // Parse image to get container name
    let (container_name, new_image) = parse_update_details(&update.new_image);

    // Apply based on resource kind
    match update.resource_kind {
        ResourceKind::Deployment => {
            update_deployment_image(
                client,
                &update.namespace,
                &update.resource_name,
                &container_name,
                &new_image,
            ).await?;
        }
        // Handle other resource kinds when implemented
        _ => return Err(anyhow!("Unsupported resource kind")),
    }

    // Update status to Applied
    update.status = UpdateStatus::Applied;

    // Emit Kubernetes event
    emit_update_event(&client, update, "Updated").await?;

    // Update metrics
    UPDATES_APPLIED.inc();

    Ok(())
}
```

### 3. Add Kubernetes Event Emission

```rust
use k8s_openapi::api::core::v1::Event;

async fn emit_update_event(
    client: &Client,
    update: &UpdateRequest,
    reason: &str,
) -> Result<()> {
    let events: Api<Event> = Api::namespaced(client, &update.namespace);

    let event = Event {
        metadata: ObjectMeta {
            name: Some(format!("headwind-{}", update.id)),
            namespace: Some(update.namespace.clone()),
            ..Default::default()
        },
        involved_object: ObjectReference {
            api_version: Some("apps/v1".to_string()),
            kind: Some(update.resource_kind.to_string()),
            name: Some(update.resource_name.clone()),
            namespace: Some(update.namespace.clone()),
            ..Default::default()
        },
        reason: Some(reason.to_string()),
        message: Some(format!(
            "Updated image from {} to {}",
            update.current_image, update.new_image
        )),
        type_: Some("Normal".to_string()),
        ..Default::default()
    };

    events.create(&PostParams::default(), &event).await?;
    Ok(())
}
```

### 4. Add Last Update Annotation

```rust
// After successful update, add annotation to track when it was updated
let now = Utc::now().to_rfc3339();
let patch = json!({
    "metadata": {
        "annotations": {
            "headwind.sh/last-update": now,
            "headwind.sh/last-update-id": update.id,
        }
    }
});

deployments.patch(
    &update.resource_name,
    &PatchParams::default(),
    &Patch::Merge(patch),
).await?;
```

### 5. Handle Update Failures

```rust
if let Err(e) = apply_update(update).await {
    update.status = UpdateStatus::Failed {
        reason: e.to_string(),
    };
    UPDATES_FAILED.inc();
    error!("Failed to apply update {}: {}", update.id, e);

    // Emit failure event
    emit_update_event(&client, update, "UpdateFailed").await.ok();
}
```

## Acceptance Criteria

- [ ] Approved updates are applied to Kubernetes resources
- [ ] Update status changes to Applied or Failed appropriately
- [ ] Kubernetes events are emitted for updates
- [ ] Last update timestamp annotation is added
- [ ] Metrics are updated (UPDATES_APPLIED, UPDATES_FAILED)
- [ ] Errors are handled gracefully
- [ ] Failed updates include error reason
- [ ] Unit tests for update application
- [ ] Integration test with real Kubernetes cluster

## Files to Modify

- `src/approval/mod.rs` - Add update watcher and application logic
- `src/controller/deployment.rs` - Use existing `update_deployment_image()`
- `src/models/policy.rs` - Add LAST_UPDATE_ID annotation constant
- `src/main.rs` - Start update watcher task
- `tests/update_application.rs` (new) - Integration tests

## Related Issues

- Depends on: #1 (Webhook-Controller Integration)
- Related to: #3 (Respect Min Update Interval)

## Testing Plan

```bash
# 1. Deploy test deployment
kubectl apply -f examples/deployment-with-headwind.yaml

# 2. Send webhook event
curl -X POST http://localhost:8080/webhook/dockerhub \
  -H "Content-Type: application/json" \
  -d '{"push_data":{"tag":"1.26.0"},"repository":{"repo_name":"nginx"}}'

# 3. Approve update
UPDATE_ID=$(curl http://localhost:8081/api/v1/updates | jq -r '.[0].id')
curl -X POST http://localhost:8081/api/v1/updates/$UPDATE_ID/approve \
  -H "Content-Type: application/json" \
  -d "{\"update_id\":\"$UPDATE_ID\",\"approved\":true,\"approver\":\"test\"}"

# 4. Verify deployment updated
kubectl get deployment nginx-example -o jsonpath='{.spec.template.spec.containers[0].image}'

# 5. Check events
kubectl get events --sort-by='.lastTimestamp' | grep headwind
```

## Estimated Effort

Medium-Large (6-12 hours)
