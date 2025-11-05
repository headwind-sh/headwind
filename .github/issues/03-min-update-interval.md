# Issue #3: Respect Minimum Update Interval

**Labels**: `enhancement`, `medium-priority`

## Description

The `headwind.sh/min-update-interval` annotation exists but is not enforced. We need to prevent updates from happening too frequently by checking the last update timestamp.

## Current State

- ✅ Annotation is parsed from Deployment
- ✅ Default value of 300 seconds (5 minutes)
- ❌ Not checked before creating UpdateRequest
- ❌ No tracking of last update time

## What Needs to Be Done

### 1. Check Last Update Time Before Creating UpdateRequest

In webhook event processing:

```rust
// Get last update annotation
let last_update = deployment
    .metadata
    .annotations
    .as_ref()
    .and_then(|a| a.get(annotations::LAST_UPDATE))
    .and_then(|s| DateTime::parse_from_rfc3339(s).ok());

// Check if enough time has passed
if let Some(last) = last_update {
    let elapsed = Utc::now().signed_duration_since(last);
    let min_interval = Duration::seconds(policy.min_update_interval.unwrap_or(300) as i64);

    if elapsed < min_interval {
        info!(
            "Skipping update for {}/{}: min interval not reached ({} < {})",
            namespace, name,
            elapsed.num_seconds(),
            min_interval.num_seconds()
        );
        continue; // Skip this update
    }
}
```

### 2. Add Metric for Skipped Updates

```rust
// In src/metrics/mod.rs
pub static ref UPDATES_SKIPPED_INTERVAL: IntCounter = IntCounter::new(
    "headwind_updates_skipped_interval_total",
    "Total number of updates skipped due to min interval"
).unwrap();

// Increment when skipping
UPDATES_SKIPPED_INTERVAL.inc();
```

### 3. Add Time Remaining to API Response

When listing updates, show when the next update is allowed:

```rust
#[derive(Serialize)]
pub struct UpdateRequestWithTiming {
    #[serde(flatten)]
    pub update: UpdateRequest,
    pub next_allowed_update: Option<DateTime<Utc>>,
}

// Calculate next allowed time
let next_allowed = last_update_time + min_interval;
```

### 4. Handle Manual Override

Add ability to override interval check via API:

```rust
#[derive(Deserialize)]
pub struct ApprovalRequest {
    pub update_id: String,
    pub approved: bool,
    pub approver: Option<String>,
    pub reason: Option<String>,
    pub force: Option<bool>, // New field
}

// In approval logic
if approval.force.unwrap_or(false) {
    // Skip interval check
    info!("Forcing update, ignoring min interval");
}
```

## Acceptance Criteria

- [ ] Updates are blocked if min interval hasn't passed
- [ ] Last update time is checked from annotation
- [ ] Metric tracks skipped updates
- [ ] Log messages explain why updates were skipped
- [ ] API shows time until next allowed update
- [ ] Manual override option via `force` flag
- [ ] Tests verify interval enforcement
- [ ] Tests verify force override works

## Files to Modify

- `src/webhook/mod.rs` - Add interval checking
- `src/metrics/mod.rs` - Add skipped updates metric
- `src/models/update.rs` - Add timing fields
- `src/approval/mod.rs` - Add force flag support

## Testing Scenario

```yaml
# Deploy with 10-minute minimum interval
apiVersion: apps/v1
kind: Deployment
metadata:
  name: test-interval
  annotations:
    headwind.sh/policy: "all"
    headwind.sh/min-update-interval: "600" # 10 minutes
    headwind.sh/require-approval: "false"
spec:
  # ...
```

```bash
# 1. Send first update - should succeed
curl -X POST http://localhost:8080/webhook/...

# 2. Send second update immediately - should be skipped
curl -X POST http://localhost:8080/webhook/...

# 3. Check metrics
curl http://localhost:9090/metrics | grep skipped_interval

# 4. Wait 10 minutes or force update
curl -X POST http://localhost:8081/api/v1/updates/$ID/approve \
  -d '{"force": true, ...}'
```

## Related Issues

- Related to: #2 (Update Application)

## Estimated Effort

Small-Medium (2-4 hours)
