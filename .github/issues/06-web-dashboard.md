# Issue #6: Create Web Dashboard for Approvals

**Labels**: `enhancement`, `ui`, `help-wanted`

## Description

Create a web-based dashboard for viewing pending updates and approving/rejecting them. This provides a better UX than curl commands for the approval API.

## Current State

- ✅ Approval API exists with full CRUD operations
- ✅ Slack notifications send approval URLs with buttons
- ❌ No web interface (Slack buttons currently just link to API endpoints)
- ❌ No visualization of pending updates
- ❌ No authentication/authorization

**Note**: Slack Incoming Webhooks don't support interactive components. The approval buttons in Slack notifications currently link to the approval API endpoints but need a web UI to properly handle the approve/reject actions in a browser.

## What Needs to Be Done

### 1. Choose Frontend Technology

Options:
- **React/Next.js** - Full SPA with good k8s integration
- **HTMX + Axum templates** - Server-side rendering, simpler
- **Yew** - Rust WASM frontend

Recommendation: Start with HTMX for simplicity, can add React later.

### 2. Add Template Support to Approval Server

```toml
[dependencies]
askama = "0.12"  # Template engine
askama_axum = "0.4"
```

```rust
// src/approval/templates.rs
use askama::Template;

#[derive(Template)]
#[template(path = "updates.html")]
struct UpdatesTemplate {
    updates: Vec<UpdateRequest>,
}
```

### 3. Create Dashboard Routes

```rust
// src/approval/mod.rs
let app = Router::new()
    .route("/", get(dashboard_home))
    .route("/updates", get(list_updates_page))
    .route("/updates/:id", get(update_detail_page))
    .route("/api/v1/updates", get(list_updates_api))
    // ... existing API routes
```

### 4. Design Dashboard Pages

#### Home Page
- Summary statistics
- Recent updates
- Pending approval count
- Success/failure rates

#### Updates List Page
- Table of all updates
- Filter by status, namespace, resource
- Sort by date, priority
- Quick approve/reject buttons

#### Update Detail Page
- Full update information
- Deployment diff view
- Approval form with reason field
- Update history/timeline

### 5. Add Real-time Updates

```rust
// Use Server-Sent Events for live updates
async fn sse_updates(
    State(state): State<ApprovalState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream! {
        loop {
            let updates = state.updates.read().await;
            let count = updates.values()
                .filter(|u| u.status == UpdateStatus::PendingApproval)
                .count();

            yield Ok(Event::default().data(count.to_string()));
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    };

    Sse::new(stream)
}
```

### 6. Add Authentication

```rust
// Simple token-based auth to start
use axum_extra::headers::{Authorization, Bearer};

async fn require_auth(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<(), StatusCode> {
    // Verify token against Kubernetes ServiceAccount token
    // Or use OIDC integration
}
```

### 7. Example Templates

```html
<!-- templates/updates.html -->
<!DOCTYPE html>
<html>
<head>
    <title>Headwind Dashboard</title>
    <script src="https://unpkg.com/htmx.org@1.9.10"></script>
</head>
<body>
    <h1>Pending Updates</h1>
    <div hx-get="/api/v1/updates" hx-trigger="every 5s">
        {% for update in updates %}
        <div class="update-card">
            <h3>{{ update.resource_name }}</h3>
            <p>{{ update.current_image }} → {{ update.new_image }}</p>
            <button hx-post="/api/v1/updates/{{ update.id }}/approve">
                Approve
            </button>
            <button hx-post="/api/v1/updates/{{ update.id }}/reject">
                Reject
            </button>
        </div>
        {% endfor %}
    </div>
</body>
</html>
```

## Acceptance Criteria

- [ ] Web dashboard accessible at approval server root
- [ ] Shows all pending updates
- [ ] Approve/reject buttons work (integrates with existing approval API)
- [ ] Supports direct links from Slack notification buttons
- [ ] Real-time updates via SSE or polling
- [ ] Basic authentication implemented
- [ ] Responsive design (mobile-friendly)
- [ ] Shows update history
- [ ] Displays metrics/statistics
- [ ] Links to Kubernetes resources
- [ ] Documentation for accessing dashboard

### Slack Integration Requirements

- [ ] Approval button URLs from Slack (`/api/v1/updates/{namespace}/{name}/approve`) redirect to approval page
- [ ] View Details button URLs from Slack (`/api/v1/updates/{namespace}/{name}`) show update details page
- [ ] Approval page shows update info with approve/reject form
- [ ] After approval/rejection, show confirmation page and optionally redirect back to list

## UI Mockup Features

```
┌─────────────────────────────────────────┐
│ Headwind Dashboard          [Metrics] ▼ │
├─────────────────────────────────────────┤
│ Pending: 5  Approved: 23  Failed: 2     │
├─────────────────────────────────────────┤
│ Filters: [All] [Pending] [Approved]     │
│ Namespace: [All ▼]  Sort: [Date ▼]      │
├─────────────────────────────────────────┤
│ ┌─────────────────────────────────────┐ │
│ │ nginx-example (default)              │ │
│ │ nginx:1.25.0 → nginx:1.26.0          │ │
│ │ Policy: minor | Created: 2m ago      │ │
│ │ [Approve] [Reject] [Details]         │ │
│ └─────────────────────────────────────┘ │
│ ┌─────────────────────────────────────┐ │
│ │ redis-cache (production)             │ │
│ │ redis:7.0.0 → redis:7.2.0            │ │
│ │ Policy: major | Created: 5m ago      │ │
│ │ [Approve] [Reject] [Details]         │ │
│ └─────────────────────────────────────┘ │
└─────────────────────────────────────────┘
```

## Testing

```bash
# Access dashboard
kubectl port-forward -n headwind-system svc/headwind-api 8081:8081
open http://localhost:8081
```

## Related Issues

- Related to: #2 (Update Application)
- Enhances: Approval workflow

## Estimated Effort

Large (12-20 hours)
