# Web UI Dashboard

Headwind provides a modern, responsive web-based dashboard for managing and monitoring update requests. The Web UI offers real-time visibility into pending updates, filtering and search capabilities, and one-click approval/rejection actions.

## Overview

The Web UI Dashboard is a powerful interface for:
- Viewing all pending and completed UpdateRequests across namespaces
- Filtering by namespace, resource kind, policy type, or search terms
- Approving or rejecting updates with detailed audit logging
- Monitoring update statistics and trends
- Accessing the observability dashboard for metrics visualization

## Accessing the Dashboard

The Web UI runs on **port 8082** by default. Access it using:

```bash
# Port forward to access locally
kubectl port-forward -n headwind-system svc/headwind-ui 8082:8082

# Open in your browser
open http://localhost:8082
```

For production deployments, expose the UI via an Ingress or LoadBalancer service.

## Dashboard Features

### Main Dashboard

The main dashboard (`/`) displays:

#### Statistics Cards
- **Pending Updates**: Number of updates awaiting approval
- **Completed Updates**: Number of processed updates (approved + rejected)
- **Quick Metrics**: Real-time update counts

#### Pending Updates Table
Interactive table with:
- Resource name, namespace, kind (Deployment/StatefulSet/DaemonSet/HelmRelease)
- Current and new image/version
- Update policy (patch, minor, major, etc.)
- Created timestamp
- **Action Buttons**:
  - **Approve**: Approve and execute the update immediately
  - **Reject**: Reject with reason (opens modal dialog)
  - **View**: See detailed information

#### Completed Updates (Collapsible)
Historical view of all approved and rejected updates with:
- Approval/rejection details
- Approver username (from audit log)
- Timestamps
- Status (Approved/Rejected/Failed)

### Filtering & Search

Real-time filtering without page reload:

**Search Bar**:
- Search by resource name or image/chart name
- Instant results as you type

**Filter Options**:
- **Namespace**: Dropdown with all unique namespaces
- **Resource Kind**: Deployment, StatefulSet, DaemonSet, HelmRelease
- **Policy Type**: patch, minor, major, all, glob, none

### Sorting

Sort UpdateRequests by:
- **Date**: Newest first (default) or oldest first
- **Namespace**: Alphabetical A-Z
- **Resource Name**: Alphabetical A-Z

### Pagination

- 20 items per page (configurable via ConfigMap)
- Previous/Next navigation buttons
- Maintains filters and search across pages

### Auto-Refresh

The dashboard automatically refreshes every 30 seconds to show the latest UpdateRequests. Toggle the auto-refresh feature using the button in the UI.

**Configuration**:
```yaml
# In ConfigMap
config.yaml: |
  refresh_interval: 30  # seconds
```

## Detail View

Click "View" on any update to see the detail page (`/updates/{namespace}/{name}`):

- Full resource information
- Complete image/version details
- Policy configuration
- Update history and status
- Approval/rejection actions
- Detailed timestamps

## Approval Workflow

### Approving Updates

1. Click **Approve** button on any pending update
2. Confirm in the dialog
3. Update executes immediately
4. Status updates in real-time
5. Notification sent (if configured)
6. Audit log entry created

**What Happens on Approval**:
- UpdateRequest CRD status updated to "Completed"
- Resource (Deployment/StatefulSet/DaemonSet/HelmRelease) updated with new image/version
- Kubernetes applies the change
- Metrics incremented (`headwind_updates_approved_total`, `headwind_updates_applied_total`)
- Approver username recorded in audit log

### Rejecting Updates

1. Click **Reject** button
2. Modal opens requesting reason
3. Enter rejection reason (required)
4. Confirm rejection
5. UpdateRequest marked as rejected
6. Audit log entry created

**Rejection Reason**:
The rejection reason is stored in the UpdateRequest CRD and visible in:
- Web UI detail view
- API responses (`GET /api/v1/updates`)
- Audit logs
- Notifications (if configured)

## Bulk Actions

Select multiple updates using checkboxes and perform bulk operations:
- **Bulk Approve**: Approve multiple updates at once
- **Bulk Reject**: Reject multiple updates with a single reason

All bulk actions are individually audited.

## Notifications

When you approve or reject an update via the Web UI:
- Success toast notification appears (green, 3 seconds)
- Error notification appears if action fails (red, persistent)
- Page automatically refreshes to show updated status

## Mobile Support

The Web UI is fully responsive and works on:
- Desktop browsers (Chrome, Firefox, Safari, Edge)
- Tablet devices
- Mobile phones (iOS Safari, Chrome Mobile)

The layout automatically adapts to screen size with mobile-optimized:
- Collapsible navigation
- Touch-friendly buttons
- Readable text sizes
- Scrollable tables

## Next Steps

- [Authentication](./web-ui-authentication.md) - Configure user authentication
- [Observability Dashboard](./observability-dashboard.md) - Metrics visualization
- [Configuration Management](../configuration/web-ui.md) - Customize Web UI settings
