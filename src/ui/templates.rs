use maud::{DOCTYPE, Markup, html};
use serde::{Deserialize, Serialize};

/// Represents an update request for display in the UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRequestView {
    pub name: String,
    pub namespace: String,
    pub resource_kind: String,
    pub resource_name: String,
    pub current_image: String,
    pub new_image: String,
    pub current_version: String,
    pub new_version: String,
    pub policy: String,
    pub status: String,
    pub created_at: String,
    pub approved_by: Option<String>,
    pub rejected_by: Option<String>,
    pub rejection_reason: Option<String>,
}

/// Base layout template - shared layout for all pages
pub fn base_layout(title: &str, content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" data-theme="light" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (title) }

                // Favicon
                link rel="icon" type="image/x-icon" href="/static/img/favicon.ico";
                link rel="icon" type="image/png" sizes="32x32" href="/static/img/logo.png";

                // DaisyUI + Tailwind CSS
                link href="https://cdn.jsdelivr.net/npm/daisyui@4.4.19/dist/full.min.css" rel="stylesheet" type="text/css";
                script src="https://cdn.tailwindcss.com" {}

                // HTMX
                script src="https://unpkg.com/htmx.org@1.9.10" {}

                // Chart.js
                script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.0/dist/chart.umd.min.js" {}

                // Custom styles
                link rel="stylesheet" href="/static/css/custom.css";
            }
            body class="bg-base-200 min-h-screen flex flex-col" hx-ext="response-targets" {
                // Notification Toast Container (for success/error messages)
                div id="toast-container" class="toast toast-top toast-end z-50" {}

                // Navigation Bar
                div class="navbar bg-base-100 shadow-lg sticky top-0 z-40" {
                    div class="flex-1" {
                        a href="/" class="btn btn-ghost normal-case text-xl gap-2" {
                            img src="/static/img/logo.png" alt="Headwind Logo" class="w-8 h-8";
                            span class="headwind-logo" { "Headwind" }
                        }
                    }
                    div class="flex-none" {
                        ul class="menu menu-horizontal px-1" {
                            li { a href="/" { "Dashboard" } }
                            li { a href="/observability" { "Observability" } }
                            li { a href="/settings" { "Settings" } }
                            li { a href="/health" { "Health" } }
                        }
                    }
                }

                // Main Content
                div class="container mx-auto p-6 flex-grow" {
                    (content)
                }

                // Footer
                footer class="footer footer-center p-4 bg-base-100 text-base-content" {
                    div {
                        p { "Headwind v0.2.0-alpha | Kubernetes Update Automation" }
                    }
                }

                // HTMX event handlers for notifications
                script {
                    (maud::PreEscaped(r#"
                    document.body.addEventListener('htmx:afterRequest', function(evt) {
                        const xhr = evt.detail.xhr;
                        const target = evt.detail.target;

                        if (xhr.status >= 200 && xhr.status < 300) {
                            showToast('Success!', 'success');
                            // Reload page after 1 second for successful actions
                            setTimeout(() => window.location.reload(), 1000);
                        } else if (xhr.status >= 400) {
                            showToast('Error: ' + xhr.statusText, 'error');
                        }
                    });

                    function showToast(message, type) {
                        const container = document.getElementById('toast-container');
                        const toast = document.createElement('div');
                        const alertClass = type === 'success' ? 'alert-success' : 'alert-error';

                        toast.className = 'alert ' + alertClass + ' shadow-lg';
                        toast.innerHTML = '<span>' + message + '</span>';
                        container.appendChild(toast);

                        setTimeout(() => {
                            toast.remove();
                        }, 3000);
                    }

                    // Pagination state
                    let currentPage = 1;
                    const itemsPerPage = 20;
                    let filteredRows = [];

                    // Filter, sort, and paginate pending updates
                    function filterAndSortUpdates() {
                        const searchInput = document.getElementById('search-input');
                        const namespaceFilter = document.getElementById('namespace-filter');
                        const kindFilter = document.getElementById('kind-filter');
                        const policyFilter = document.getElementById('policy-filter');
                        const sortBy = document.getElementById('sort-by');

                        if (!searchInput || !namespaceFilter || !kindFilter || !policyFilter || !sortBy) return;

                        const searchTerm = searchInput.value.toLowerCase();
                        const selectedNamespace = namespaceFilter.value;
                        const selectedKind = kindFilter.value;
                        const selectedPolicy = policyFilter.value;
                        const sortOption = sortBy.value;

                        const allRows = Array.from(document.querySelectorAll('.pending-update-row'));

                        // Filter rows
                        filteredRows = allRows.filter(row => {
                            const namespace = row.getAttribute('data-namespace') || '';
                            const kind = row.getAttribute('data-kind') || '';
                            const policy = row.getAttribute('data-policy') || '';
                            const resourceName = row.getAttribute('data-resource-name') || '';
                            const currentImage = row.getAttribute('data-current-image') || '';
                            const newImage = row.getAttribute('data-new-image') || '';

                            const searchableText = (resourceName + ' ' + currentImage + ' ' + newImage).toLowerCase();

                            const matchesSearch = !searchTerm || searchableText.includes(searchTerm);
                            const matchesNamespace = !selectedNamespace || namespace === selectedNamespace;
                            const matchesKind = !selectedKind || kind === selectedKind;
                            const matchesPolicy = !selectedPolicy || policy === selectedPolicy;

                            return matchesSearch && matchesNamespace && matchesKind && matchesPolicy;
                        });

                        // Sort rows
                        filteredRows.sort((a, b) => {
                            if (sortOption === 'date-desc') {
                                return (b.getAttribute('data-created-at') || '').localeCompare(a.getAttribute('data-created-at') || '');
                            } else if (sortOption === 'date-asc') {
                                return (a.getAttribute('data-created-at') || '').localeCompare(b.getAttribute('data-created-at') || '');
                            } else if (sortOption === 'namespace') {
                                return (a.getAttribute('data-namespace') || '').localeCompare(b.getAttribute('data-namespace') || '');
                            } else if (sortOption === 'resource') {
                                return (a.getAttribute('data-resource-name') || '').localeCompare(b.getAttribute('data-resource-name') || '');
                            }
                            return 0;
                        });

                        // Reset to page 1 when filters change
                        currentPage = 1;

                        // Apply pagination
                        applyPagination();

                        // Update count badge
                        updateFilteredCount(filteredRows.length, allRows.length);
                    }

                    // Apply pagination to filtered rows
                    function applyPagination() {
                        const allRows = document.querySelectorAll('.pending-update-row');
                        const totalPages = Math.ceil(filteredRows.length / itemsPerPage);
                        const startIdx = (currentPage - 1) * itemsPerPage;
                        const endIdx = startIdx + itemsPerPage;

                        // Hide all rows first
                        allRows.forEach(row => row.style.display = 'none');

                        // Show only rows for current page
                        filteredRows.slice(startIdx, endIdx).forEach(row => row.style.display = '');

                        // Update pagination UI
                        const pageInfo = document.getElementById('page-info');
                        const prevBtn = document.getElementById('prev-page');
                        const nextBtn = document.getElementById('next-page');
                        const paginationInfo = document.getElementById('pagination-info');

                        if (pageInfo) pageInfo.textContent = 'Page ' + currentPage + ' of ' + (totalPages || 1);
                        if (prevBtn) prevBtn.disabled = currentPage === 1;
                        if (nextBtn) nextBtn.disabled = currentPage >= totalPages || totalPages === 0;

                        const showing = Math.min(filteredRows.length, endIdx);
                        if (paginationInfo) {
                            paginationInfo.textContent = 'Showing ' + (startIdx + 1) + '-' + showing + ' of ' + filteredRows.length;
                        }
                    }

                    // Update the filtered count badge
                    function updateFilteredCount(visible, total) {
                        const badge = document.querySelector('.card-title .badge-warning');
                        if (badge) {
                            badge.textContent = visible === total ? total : visible + ' / ' + total;
                        }
                    }

                    // Navigate to previous page
                    function previousPage() {
                        if (currentPage > 1) {
                            currentPage--;
                            applyPagination();
                        }
                    }

                    // Navigate to next page
                    function nextPage() {
                        const totalPages = Math.ceil(filteredRows.length / itemsPerPage);
                        if (currentPage < totalPages) {
                            currentPage++;
                            applyPagination();
                        }
                    }

                    // Clear all filters
                    function clearFilters() {
                        const searchInput = document.getElementById('search-input');
                        const namespaceFilter = document.getElementById('namespace-filter');
                        const kindFilter = document.getElementById('kind-filter');
                        const policyFilter = document.getElementById('policy-filter');
                        const sortBy = document.getElementById('sort-by');

                        if (searchInput) searchInput.value = '';
                        if (namespaceFilter) namespaceFilter.value = '';
                        if (kindFilter) kindFilter.value = '';
                        if (policyFilter) policyFilter.value = '';
                        if (sortBy) sortBy.value = 'date-desc';

                        filterAndSortUpdates();
                    }

                    // Initialize on page load
                    document.addEventListener('DOMContentLoaded', function() {
                        filterAndSortUpdates();
                    });
                    "#))
                }
            }
        }
    }
}

/// Dashboard template - main view showing all pending updates
pub fn dashboard(
    pending_updates: &[UpdateRequestView],
    completed_updates: &[UpdateRequestView],
) -> Markup {
    let total_pending = pending_updates.len();
    let total_completed = completed_updates.len();

    let content = html! {
        // Stats
        div class="stats shadow mb-6 w-full" {
            div class="stat" {
                div class="stat-figure text-warning" {
                    svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" class="inline-block w-8 h-8 stroke-current" {
                        path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z";
                    }
                }
                div class="stat-title" { "Pending Updates" }
                div class="stat-value text-warning" { (total_pending) }
                div class="stat-desc" { "Requiring approval" }
            }

            div class="stat" {
                div class="stat-figure text-success" {
                    svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" class="inline-block w-8 h-8 stroke-current" {
                        path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7";
                    }
                }
                div class="stat-title" { "Completed" }
                div class="stat-value text-success" { (total_completed) }
                div class="stat-desc" { "Updates processed" }
            }
        }

        // Pending Updates Section
        div class="card bg-base-100 shadow-xl mb-6" {
            div class="card-body" {
                h2 class="card-title text-2xl mb-4" {
                    span class="badge badge-warning" { (total_pending) }
                    "Pending Updates"
                }

                // Filter and Search Controls
                @if !pending_updates.is_empty() {
                    div class="flex flex-wrap gap-4 mb-4" {
                        // Search input
                        div class="form-control flex-1 min-w-64" {
                            label class="label" {
                                span class="label-text" { "Search" }
                            }
                            input type="text" id="search-input" placeholder="Filter by resource name or image..."
                                class="input input-bordered w-full"
                                oninput="filterAndSortUpdates()";
                        }

                        // Namespace filter
                        div class="form-control" {
                            label class="label" {
                                span class="label-text" { "Namespace" }
                            }
                            select id="namespace-filter" class="select select-bordered" onchange="filterAndSortUpdates()" {
                                option value="" selected { "All Namespaces" }
                                @for ns in get_unique_namespaces(pending_updates) {
                                    option value=(ns) { (ns) }
                                }
                            }
                        }

                        // Resource kind filter
                        div class="form-control" {
                            label class="label" {
                                span class="label-text" { "Resource Kind" }
                            }
                            select id="kind-filter" class="select select-bordered" onchange="filterAndSortUpdates()" {
                                option value="" selected { "All Kinds" }
                                @for kind in get_unique_kinds(pending_updates) {
                                    option value=(kind) { (kind) }
                                }
                            }
                        }

                        // Policy filter
                        div class="form-control" {
                            label class="label" {
                                span class="label-text" { "Policy" }
                            }
                            select id="policy-filter" class="select select-bordered" onchange="filterAndSortUpdates()" {
                                option value="" selected { "All Policies" }
                                @for policy in get_unique_policies(pending_updates) {
                                    option value=(policy) { (policy) }
                                }
                            }
                        }

                        // Sort by
                        div class="form-control" {
                            label class="label" {
                                span class="label-text" { "Sort By" }
                            }
                            select id="sort-by" class="select select-bordered" onchange="filterAndSortUpdates()" {
                                option value="date-desc" selected { "Newest First" }
                                option value="date-asc" { "Oldest First" }
                                option value="namespace" { "Namespace A-Z" }
                                option value="resource" { "Resource A-Z" }
                            }
                        }

                        // Clear filters button
                        div class="form-control" {
                            label class="label" {
                                span class="label-text" { " " }
                            }
                            button class="btn btn-ghost" onclick="clearFilters()" {
                                "Clear Filters"
                            }
                        }
                    }

                    // Pagination controls
                    div class="flex justify-between items-center mb-4" {
                        div class="text-sm opacity-70" {
                            span id="pagination-info" { "Showing all updates" }
                        }
                        div class="join" {
                            button id="prev-page" class="join-item btn btn-sm" onclick="previousPage()" disabled {
                                "« Previous"
                            }
                            span id="page-info" class="join-item btn btn-sm btn-disabled" { "Page 1" }
                            button id="next-page" class="join-item btn btn-sm" onclick="nextPage()" {
                                "Next »"
                            }
                        }
                    }
                }

                @if pending_updates.is_empty() {
                    div class="alert alert-success" {
                        svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24" {
                            path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z";
                        }
                        span { "No pending updates at this time!" }
                    }
                } @else {
                    div class="overflow-x-auto" {
                        table class="table table-zebra" {
                            thead {
                                tr {
                                    th { "Resource" }
                                    th { "Namespace" }
                                    th { "Current Version" }
                                    th { "New Version" }
                                    th { "Policy" }
                                    th { "Created" }
                                    th { "Actions" }
                                }
                            }
                            tbody id="pending-updates-tbody" {
                                @for update in pending_updates {
                                    tr id=(format!("update-row-{}-{}", update.namespace, update.name))
                                        class="pending-update-row"
                                        data-namespace=(update.namespace)
                                        data-kind=(update.resource_kind)
                                        data-resource-name=(update.resource_name)
                                        data-current-image=(update.current_image)
                                        data-new-image=(update.new_image)
                                        data-policy=(update.policy)
                                        data-created-at=(update.created_at) {
                                        td {
                                            div class="flex flex-col" {
                                                span class="badge badge-outline badge-sm mb-1" { (update.resource_kind) }
                                                span class="font-semibold" { (update.resource_name) }
                                            }
                                        }
                                        td { span class="badge badge-ghost" { (update.namespace) } }
                                        td { code class="version-display" { (update.current_version) } }
                                        td { code class="version-display text-success font-bold" { (update.new_version) } }
                                        td { span class="badge badge-info" { (update.policy) } }
                                        td class="text-sm opacity-70" { (update.created_at) }
                                        td {
                                            div class="flex gap-2" {
                                                button class="btn btn-success btn-sm"
                                                    hx-post=(format!("/api/v1/updates/{}/{}/approve", update.namespace, update.name))
                                                    hx-vals=r#"{"approver": "web-ui"}"#
                                                    hx-confirm="Are you sure you want to approve this update?"
                                                    hx-target=(format!("#update-row-{}-{}", update.namespace, update.name))
                                                    hx-swap="outerHTML" {
                                                    "✓ Approve"
                                                }
                                                button class="btn btn-error btn-sm"
                                                    onclick=(format!("reject_modal_{}_{}.showModal()", update.namespace, update.name)) {
                                                    "✗ Reject"
                                                }
                                                a href=(format!("/updates/{}/{}", update.namespace, update.name)) class="btn btn-ghost btn-sm" {
                                                    "Details"
                                                }
                                            }

                                            // Reject Modal
                                            dialog id=(format!("reject_modal_{}_{}", update.namespace, update.name)) class="modal" {
                                                div class="modal-box" {
                                                    h3 class="font-bold text-lg" { "Reject Update" }
                                                    p class="py-4" { "Please provide a reason for rejecting this update:" }
                                                    form method="dialog" {
                                                        textarea id=(format!("reject_reason_{}_{}", update.namespace, update.name))
                                                            class="textarea textarea-bordered w-full"
                                                            placeholder="Reason for rejection..."
                                                            rows="3" {}
                                                        div class="modal-action" {
                                                            button class="btn" { "Cancel" }
                                                            button type="button" class="btn btn-error"
                                                                hx-post=(format!("/api/v1/updates/{}/{}/reject", update.namespace, update.name))
                                                                hx-vals=(format!(r#"js:{{approver: "web-ui", reason: document.getElementById("reject_reason_{}_{}").value}}"#, update.namespace, update.name))
                                                                hx-target=(format!("#update-row-{}-{}", update.namespace, update.name))
                                                                hx-swap="outerHTML"
                                                                onclick=(format!("reject_modal_{}_{}.close()", update.namespace, update.name)) {
                                                                "Reject Update"
                                                            }
                                                        }
                                                    }
                                                }
                                                form method="dialog" class="modal-backdrop" {
                                                    button { "close" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Completed Updates Section (Collapsed)
        div class="collapse collapse-arrow bg-base-100 shadow-xl" {
            input type="checkbox";
            div class="collapse-title text-xl font-medium" {
                span class="badge badge-info mr-2" { (total_completed) }
                "Completed Updates"
            }
            div class="collapse-content" {
                @if completed_updates.is_empty() {
                    p class="text-gray-500 py-4" { "No completed updates yet." }
                } @else {
                    div class="overflow-x-auto" {
                        table class="table table-sm" {
                            thead {
                                tr {
                                    th { "Resource" }
                                    th { "Namespace" }
                                    th { "Version" }
                                    th { "Status" }
                                    th { "Approved/Rejected By" }
                                }
                            }
                            tbody {
                                @for update in completed_updates {
                                    tr {
                                        td {
                                            span class="badge badge-outline badge-sm" { (update.resource_kind) }
                                            " "
                                            (update.resource_name)
                                        }
                                        td { (update.namespace) }
                                        td {
                                            code class="text-xs" { (update.current_version) }
                                            " → "
                                            code class="text-xs" { (update.new_version) }
                                        }
                                        td {
                                            @if update.status == "Completed" {
                                                span class="badge badge-success" { (update.status) }
                                            } @else if update.status == "Rejected" {
                                                span class="badge badge-error" { (update.status) }
                                            } @else {
                                                span class="badge badge-warning" { (update.status) }
                                            }
                                        }
                                        td {
                                            @if let Some(ref approver) = update.approved_by {
                                                "✅ " (approver)
                                            } @else if let Some(ref rejector) = update.rejected_by {
                                                "❌ " (rejector)
                                            } @else {
                                                "-"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    base_layout("Headwind Dashboard", content)
}

/// Detail template - individual update request view
pub fn detail(update: &UpdateRequestView) -> Markup {
    let content = html! {
        // Breadcrumbs
        div class="text-sm breadcrumbs mb-4" {
            ul {
                li { a href="/" { "Dashboard" } }
                li { (update.namespace) }
                li { (update.name) }
            }
        }

        // Update Request Details Card
        div class="card bg-base-100 shadow-xl" {
            div class="card-body" {
                div class="flex justify-between items-start" {
                    div {
                        h2 class="card-title text-3xl" { (update.resource_name) }
                        div class="flex gap-2 mt-2" {
                            span class="badge badge-outline" { (update.resource_kind) }
                            span class="badge badge-ghost" { (update.namespace) }
                            span class="badge badge-info" { (update.policy) }
                            @if update.status == "Pending" {
                                span class="badge badge-warning" { (update.status) }
                            } @else if update.status == "Completed" {
                                span class="badge badge-success" { (update.status) }
                            } @else if update.status == "Rejected" {
                                span class="badge badge-error" { (update.status) }
                            } @else {
                                span class="badge" { (update.status) }
                            }
                        }
                    }
                }

                div class="divider" {}

                // Version Information
                div class="grid grid-cols-1 md:grid-cols-2 gap-6" {
                    div {
                        h3 class="text-lg font-semibold mb-2" { "Current Image" }
                        div class="mockup-code" {
                            pre { code { (update.current_image) } }
                        }
                        div class="mt-2" {
                            span class="text-sm opacity-70" { "Version:" }
                            code class="version-display ml-2" { (update.current_version) }
                        }
                    }

                    div {
                        h3 class="text-lg font-semibold mb-2" { "New Image" }
                        div class="mockup-code bg-success text-success-content" {
                            pre { code { (update.new_image) } }
                        }
                        div class="mt-2" {
                            span class="text-sm opacity-70" { "Version:" }
                            code class="version-display ml-2 bg-success text-success-content" { (update.new_version) }
                        }
                    }
                }

                div class="divider" {}

                // Metadata
                div class="grid grid-cols-1 md:grid-cols-2 gap-4" {
                    div {
                        p class="text-sm opacity-70" { "Created At" }
                        p class="font-semibold" { (update.created_at) }
                    }

                    @if let Some(ref approver) = update.approved_by {
                        div {
                            p class="text-sm opacity-70" { "Approved By" }
                            p class="font-semibold text-success" { "✅ " (approver) }
                        }
                    }

                    @if let Some(ref rejector) = update.rejected_by {
                        div {
                            p class="text-sm opacity-70" { "Rejected By" }
                            p class="font-semibold text-error" { "❌ " (rejector) }
                        }
                    }

                    @if let Some(ref reason) = update.rejection_reason {
                        div class="col-span-2" {
                            p class="text-sm opacity-70" { "Rejection Reason" }
                            div class="alert alert-error mt-2" {
                                svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24" {
                                    path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z";
                                }
                                span { (reason) }
                            }
                        }
                    }
                }

                @if update.status == "Pending" {
                    div class="divider" {}

                    // Action Buttons
                    div class="card-actions justify-end" {
                        a href="/" class="btn btn-ghost" { "Back to Dashboard" }
                        button class="btn btn-error" onclick="reject_modal.showModal()" {
                            "✗ Reject"
                        }
                        button class="btn btn-success"
                            hx-post=(format!("/api/v1/updates/{}/{}/approve", update.namespace, update.name))
                            hx-vals=r#"{"approver": "web-ui"}"#
                            hx-confirm="Are you sure you want to approve this update?"
                            hx-on--after-request="window.location.href='/'" {
                            "✓ Approve Update"
                        }
                    }

                    // Reject Modal
                    dialog id="reject_modal" class="modal" {
                        div class="modal-box" {
                            h3 class="font-bold text-lg" { "Reject Update" }
                            p class="py-4" { "Please provide a reason for rejecting this update:" }
                            form method="dialog" {
                                textarea id="reject_reason" class="textarea textarea-bordered w-full" placeholder="Reason for rejection..." rows="4" {}
                                div class="modal-action" {
                                    button class="btn" { "Cancel" }
                                    button type="button" class="btn btn-error"
                                        hx-post=(format!("/api/v1/updates/{}/{}/reject", update.namespace, update.name))
                                        hx-vals=r#"js:{approver: "web-ui", reason: document.getElementById("reject_reason").value}"#
                                        hx-on--after-request="window.location.href='/'"
                                        onclick="reject_modal.close()" {
                                        "Reject Update"
                                    }
                                }
                            }
                        }
                        form method="dialog" class="modal-backdrop" {
                            button { "close" }
                        }
                    }
                } @else {
                    div class="divider" {}
                    div class="card-actions justify-end" {
                        a href="/" class="btn btn-primary" { "Back to Dashboard" }
                    }
                }
            }
        }
    };

    base_layout(&format!("Update Request - {}", update.name), content)
}

/// Helper function to get unique namespaces from updates
fn get_unique_namespaces(updates: &[UpdateRequestView]) -> Vec<String> {
    let mut namespaces: Vec<String> = updates.iter().map(|u| u.namespace.clone()).collect();
    namespaces.sort();
    namespaces.dedup();
    namespaces
}

/// Helper function to get unique resource kinds from updates
fn get_unique_kinds(updates: &[UpdateRequestView]) -> Vec<String> {
    let mut kinds: Vec<String> = updates.iter().map(|u| u.resource_kind.clone()).collect();
    kinds.sort();
    kinds.dedup();
    kinds
}

/// Helper function to get unique policies from updates
fn get_unique_policies(updates: &[UpdateRequestView]) -> Vec<String> {
    let mut policies: Vec<String> = updates.iter().map(|u| u.policy.clone()).collect();
    policies.sort();
    policies.dedup();
    policies
}

/// Settings page template
pub fn settings() -> Markup {
    let content = html! {
        h1 class="text-3xl font-bold mb-6" { "Settings" }

        // Loading indicator
        div id="settings-loading" class="flex justify-center items-center py-12" {
            span class="loading loading-spinner loading-lg" {}
        }

        // Settings form (hidden until loaded)
        div id="settings-form" class="hidden" {
            // Polling Configuration
            div class="card bg-base-100 shadow-xl mb-6" {
                div class="card-body" {
                    h2 class="card-title text-2xl mb-4" { "Polling Configuration" }

                    div class="form-control mb-4" {
                        label class="label cursor-pointer" {
                            span class="label-text" { "Enable Registry Polling" }
                            input type="checkbox" id="polling-enabled" class="checkbox checkbox-primary";
                        }
                    }

                    div class="form-control" {
                        label class="label" {
                            span class="label-text" { "Polling Interval (seconds)" }
                        }
                        input type="number" id="polling-interval" class="input input-bordered" min="60" step="60" value="300";
                    }
                }
            }

            // Helm Configuration
            div class="card bg-base-100 shadow-xl mb-6" {
                div class="card-body" {
                    h2 class="card-title text-2xl mb-4" { "Helm Configuration" }

                    div class="form-control" {
                        label class="label cursor-pointer" {
                            span class="label-text" { "Enable Helm Auto-Discovery" }
                            input type="checkbox" id="helm-auto-discovery" class="checkbox checkbox-primary" checked;
                        }
                    }
                }
            }

            // Controllers Configuration
            div class="card bg-base-100 shadow-xl mb-6" {
                div class="card-body" {
                    h2 class="card-title text-2xl mb-4" { "Controllers Configuration" }

                    div class="form-control" {
                        label class="label cursor-pointer" {
                            span class="label-text" { "Enable Kubernetes Controllers" }
                            input type="checkbox" id="controllers-enabled" class="checkbox checkbox-primary" checked;
                        }
                    }
                }
            }

            // Slack Notifications
            div class="card bg-base-100 shadow-xl mb-6" {
                div class="card-body" {
                    h2 class="card-title text-2xl mb-4" {
                        "Slack Notifications"
                        button class="btn btn-sm btn-outline ml-4" onclick="testNotification('slack')" {
                            "Test"
                        }
                    }

                    div class="form-control mb-4" {
                        label class="label cursor-pointer" {
                            span class="label-text" { "Enable Slack Notifications" }
                            input type="checkbox" id="slack-enabled" class="checkbox checkbox-primary";
                        }
                    }

                    div class="form-control mb-4" {
                        label class="label" {
                            span class="label-text" { "Webhook URL" }
                        }
                        input type="url" id="slack-webhook-url" class="input input-bordered" placeholder="https://hooks.slack.com/services/...";
                    }

                    div class="form-control mb-4" {
                        label class="label" {
                            span class="label-text" { "Channel (optional)" }
                        }
                        input type="text" id="slack-channel" class="input input-bordered" placeholder="#deployments";
                    }

                    div class="grid grid-cols-2 gap-4" {
                        div class="form-control" {
                            label class="label" {
                                span class="label-text" { "Username" }
                            }
                            input type="text" id="slack-username" class="input input-bordered" value="Headwind";
                        }

                        div class="form-control" {
                            label class="label" {
                                span class="label-text" { "Icon Emoji" }
                            }
                            input type="text" id="slack-icon-emoji" class="input input-bordered" value=":sailboat:";
                        }
                    }
                }
            }

            // Teams Notifications
            div class="card bg-base-100 shadow-xl mb-6" {
                div class="card-body" {
                    h2 class="card-title text-2xl mb-4" {
                        "Microsoft Teams Notifications"
                        button class="btn btn-sm btn-outline ml-4" onclick="testNotification('teams')" {
                            "Test"
                        }
                    }

                    div class="form-control mb-4" {
                        label class="label cursor-pointer" {
                            span class="label-text" { "Enable Teams Notifications" }
                            input type="checkbox" id="teams-enabled" class="checkbox checkbox-primary";
                        }
                    }

                    div class="form-control" {
                        label class="label" {
                            span class="label-text" { "Webhook URL" }
                        }
                        input type="url" id="teams-webhook-url" class="input input-bordered" placeholder="https://outlook.office.com/webhook/...";
                    }
                }
            }

            // Generic Webhook Notifications
            div class="card bg-base-100 shadow-xl mb-6" {
                div class="card-body" {
                    h2 class="card-title text-2xl mb-4" {
                        "Generic Webhook Notifications"
                        button class="btn btn-sm btn-outline ml-4" onclick="testNotification('webhook')" {
                            "Test"
                        }
                    }

                    div class="form-control mb-4" {
                        label class="label cursor-pointer" {
                            span class="label-text" { "Enable Webhook Notifications" }
                            input type="checkbox" id="webhook-enabled" class="checkbox checkbox-primary";
                        }
                    }

                    div class="form-control" {
                        label class="label" {
                            span class="label-text" { "Webhook URL" }
                        }
                        input type="url" id="webhook-url" class="input input-bordered" placeholder="https://example.com/webhook";
                    }
                }
            }

            // Observability / Metrics Storage
            div class="card bg-base-100 shadow-xl mb-6" {
                div class="card-body" {
                    h2 class="card-title text-2xl mb-4" { "Observability / Metrics Storage" }

                    div class="form-control mb-4" {
                        label class="label" {
                            span class="label-text" { "Metrics Backend" }
                        }
                        select id="observability-metrics-backend" class="select select-bordered w-full" {
                            option value="auto" { "Auto (Discover Prometheus/VictoriaMetrics)" }
                            option value="prometheus" { "Prometheus" }
                            option value="victoriametrics" { "VictoriaMetrics" }
                            option value="influxdb" { "InfluxDB" }
                            option value="live" { "Live Only (No History)" }
                        }
                    }

                    div class="divider" { "Prometheus Configuration" }

                    div class="form-control mb-4" {
                        label class="label cursor-pointer" {
                            span class="label-text" { "Enable Prometheus" }
                            input type="checkbox" id="observability-prometheus-enabled" class="checkbox checkbox-primary";
                        }
                    }

                    div class="form-control mb-4" {
                        label class="label" {
                            span class="label-text" { "Prometheus URL" }
                        }
                        input type="url" id="observability-prometheus-url" class="input input-bordered" placeholder="http://prometheus-server.monitoring.svc.cluster.local:80";
                    }

                    div class="divider" { "VictoriaMetrics Configuration" }

                    div class="form-control mb-4" {
                        label class="label cursor-pointer" {
                            span class="label-text" { "Enable VictoriaMetrics" }
                            input type="checkbox" id="observability-victoriametrics-enabled" class="checkbox checkbox-primary";
                        }
                    }

                    div class="form-control mb-4" {
                        label class="label" {
                            span class="label-text" { "VictoriaMetrics URL" }
                        }
                        input type="url" id="observability-victoriametrics-url" class="input input-bordered" placeholder="http://victoria-metrics.monitoring.svc.cluster.local:8428";
                    }

                    div class="divider" { "InfluxDB Configuration" }

                    div class="form-control mb-4" {
                        label class="label cursor-pointer" {
                            span class="label-text" { "Enable InfluxDB" }
                            input type="checkbox" id="observability-influxdb-enabled" class="checkbox checkbox-primary";
                        }
                    }

                    div class="form-control mb-4" {
                        label class="label" {
                            span class="label-text" { "InfluxDB URL" }
                        }
                        input type="url" id="observability-influxdb-url" class="input input-bordered" placeholder="http://influxdb.monitoring.svc.cluster.local:8086";
                    }

                    div class="form-control mb-4" {
                        label class="label" {
                            span class="label-text" { "InfluxDB Organization" }
                        }
                        input type="text" id="observability-influxdb-org" class="input input-bordered" placeholder="headwind";
                    }

                    div class="form-control mb-4" {
                        label class="label" {
                            span class="label-text" { "InfluxDB Bucket" }
                        }
                        input type="text" id="observability-influxdb-bucket" class="input input-bordered" placeholder="metrics";
                    }

                    div class="form-control mb-4" {
                        label class="label" {
                            span class="label-text" { "InfluxDB API Token" }
                        }
                        input type="password" id="observability-influxdb-token" class="input input-bordered" placeholder="your-api-token";
                    }
                }
            }

            // Action buttons
            div class="flex gap-4 mt-6" {
                button class="btn btn-primary" onclick="saveSettings()" {
                    "Save Settings"
                }
                button class="btn btn-ghost" onclick="window.location.reload()" {
                    "Cancel"
                }
            }
        }

        // JavaScript for settings management
        script {
            (maud::PreEscaped(r#"
            // Load settings on page load
            document.addEventListener('DOMContentLoaded', function() {
                loadSettings();
            });

            // Load current settings from API
            async function loadSettings() {
                try {
                    const response = await fetch('/api/v1/settings');
                    const config = await response.json();

                    // Populate form fields
                    document.getElementById('polling-enabled').checked = config.polling.enabled;
                    document.getElementById('polling-interval').value = config.polling.interval;
                    document.getElementById('helm-auto-discovery').checked = config.helm.autoDiscovery;
                    document.getElementById('controllers-enabled').checked = config.controllers.enabled;

                    document.getElementById('slack-enabled').checked = config.notifications.slack.enabled;
                    document.getElementById('slack-webhook-url').value = config.notifications.slack.webhookUrl || '';
                    document.getElementById('slack-channel').value = config.notifications.slack.channel || '';
                    document.getElementById('slack-username').value = config.notifications.slack.username || 'Headwind';
                    document.getElementById('slack-icon-emoji').value = config.notifications.slack.iconEmoji || ':sailboat:';

                    document.getElementById('teams-enabled').checked = config.notifications.teams.enabled;
                    document.getElementById('teams-webhook-url').value = config.notifications.teams.webhookUrl || '';

                    document.getElementById('webhook-enabled').checked = config.notifications.webhook.enabled;
                    document.getElementById('webhook-url').value = config.notifications.webhook.url || '';

                    // Observability settings
                    document.getElementById('observability-metrics-backend').value = config.observability.metricsBackend || 'auto';
                    document.getElementById('observability-prometheus-enabled').checked = config.observability.prometheus.enabled;
                    document.getElementById('observability-prometheus-url').value = config.observability.prometheus.url || '';
                    document.getElementById('observability-victoriametrics-enabled').checked = config.observability.victoriametrics.enabled;
                    document.getElementById('observability-victoriametrics-url').value = config.observability.victoriametrics.url || '';
                    document.getElementById('observability-influxdb-enabled').checked = config.observability.influxdb.enabled;
                    document.getElementById('observability-influxdb-url').value = config.observability.influxdb.url || '';
                    document.getElementById('observability-influxdb-org').value = config.observability.influxdb.org || '';
                    document.getElementById('observability-influxdb-bucket').value = config.observability.influxdb.bucket || '';
                    document.getElementById('observability-influxdb-token').value = config.observability.influxdb.token || '';

                    // Show form, hide loading
                    document.getElementById('settings-loading').classList.add('hidden');
                    document.getElementById('settings-form').classList.remove('hidden');
                } catch (error) {
                    console.error('Failed to load settings:', error);
                    showToast('Failed to load settings', 'error');
                }
            }

            // Save settings to API
            async function saveSettings() {
                const config = {
                    polling: {
                        enabled: document.getElementById('polling-enabled').checked,
                        interval: parseInt(document.getElementById('polling-interval').value)
                    },
                    helm: {
                        autoDiscovery: document.getElementById('helm-auto-discovery').checked
                    },
                    controllers: {
                        enabled: document.getElementById('controllers-enabled').checked
                    },
                    notifications: {
                        slack: {
                            enabled: document.getElementById('slack-enabled').checked,
                            webhookUrl: document.getElementById('slack-webhook-url').value || null,
                            channel: document.getElementById('slack-channel').value || null,
                            username: document.getElementById('slack-username').value || null,
                            iconEmoji: document.getElementById('slack-icon-emoji').value || null
                        },
                        teams: {
                            enabled: document.getElementById('teams-enabled').checked,
                            webhookUrl: document.getElementById('teams-webhook-url').value || null
                        },
                        webhook: {
                            enabled: document.getElementById('webhook-enabled').checked,
                            url: document.getElementById('webhook-url').value || null
                        }
                    },
                    observability: {
                        metricsBackend: document.getElementById('observability-metrics-backend').value,
                        prometheus: {
                            enabled: document.getElementById('observability-prometheus-enabled').checked,
                            url: document.getElementById('observability-prometheus-url').value || null
                        },
                        victoriametrics: {
                            enabled: document.getElementById('observability-victoriametrics-enabled').checked,
                            url: document.getElementById('observability-victoriametrics-url').value || null
                        },
                        influxdb: {
                            enabled: document.getElementById('observability-influxdb-enabled').checked,
                            url: document.getElementById('observability-influxdb-url').value || null,
                            org: document.getElementById('observability-influxdb-org').value || null,
                            bucket: document.getElementById('observability-influxdb-bucket').value || null,
                            token: document.getElementById('observability-influxdb-token').value || null
                        }
                    }
                };

                try {
                    const response = await fetch('/api/v1/settings', {
                        method: 'PUT',
                        headers: {
                            'Content-Type': 'application/json'
                        },
                        body: JSON.stringify(config)
                    });

                    if (response.ok) {
                        showToast('Settings saved successfully!', 'success');
                        setTimeout(() => window.location.href = '/', 2000);
                    } else {
                        const error = await response.json();
                        showToast('Failed to save settings: ' + (error.error || 'Unknown error'), 'error');
                    }
                } catch (error) {
                    console.error('Failed to save settings:', error);
                    showToast('Failed to save settings', 'error');
                }
            }

            // Test notification
            async function testNotification(type) {
                try {
                    const response = await fetch('/api/v1/settings/test-notification', {
                        method: 'POST',
                        headers: {
                            'Content-Type': 'application/json'
                        },
                        body: JSON.stringify({ type })
                    });

                    const result = await response.json();
                    if (response.ok) {
                        showToast(result.message, 'success');
                    } else {
                        showToast('Test failed: ' + (result.error || 'Unknown error'), 'error');
                    }
                } catch (error) {
                    console.error('Test notification failed:', error);
                    showToast('Test notification failed', 'error');
                }
            }
            "#))
        }
    };

    base_layout("Settings - Headwind", content)
}
/// Observability page template
pub fn observability() -> Markup {
    let content = html! {
        h1 class="text-3xl font-bold mb-6" { "Observability" }

        // Loading indicator
        div id="metrics-loading" class="flex justify-center items-center py-12" {
            span class="loading loading-spinner loading-lg" {}
        }

        // Metrics dashboard (hidden until loaded)
        div id="metrics-dashboard" class="hidden" {
            // Backend info
            div class="alert alert-info mb-6" {
                div {
                    svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" class="stroke-current shrink-0 w-6 h-6" {
                        path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" {}
                    }
                    span { "Metrics backend: " span id="backend-type" class="font-bold" { "Loading..." } }
                }
            }

            // Key metrics cards
            div class="grid grid-cols-1 md:grid-cols-3 lg:grid-cols-5 gap-4 mb-6" {
                // Updates Pending
                div class="card bg-base-100 shadow-xl" {
                    div class="card-body" {
                        h2 class="card-title text-sm" { "Updates Pending" }
                        p class="text-4xl font-bold" id="metric-updates-pending" { "0" }
                    }
                }
                // Updates Approved
                div class="card bg-base-100 shadow-xl" {
                    div class="card-body" {
                        h2 class="card-title text-sm" { "Updates Approved" }
                        p class="text-4xl font-bold text-success" id="metric-updates-approved" { "0" }
                    }
                }
                // Updates Applied
                div class="card bg-base-100 shadow-xl" {
                    div class="card-body" {
                        h2 class="card-title text-sm" { "Updates Applied" }
                        p class="text-4xl font-bold text-primary" id="metric-updates-applied" { "0" }
                    }
                }
                // Updates Rejected
                div class="card bg-base-100 shadow-xl" {
                    div class="card-body" {
                        h2 class="card-title text-sm" { "Updates Rejected" }
                        p class="text-4xl font-bold text-secondary" id="metric-updates-rejected" { "0" }
                    }
                }
                // Updates Failed
                div class="card bg-base-100 shadow-xl" {
                    div class="card-body" {
                        h2 class="card-title text-sm" { "Updates Failed" }
                        p class="text-4xl font-bold text-error" id="metric-updates-failed" { "0" }
                    }
                }
            }

            // Resources watched
            div class="card bg-base-100 shadow-xl mb-6" {
                div class="card-body" {
                    h2 class="card-title text-2xl mb-4" { "Resources Watched" }
                    div class="grid grid-cols-2 md:grid-cols-4 gap-4" {
                        div {
                            p class="text-sm text-base-content/70" { "Deployments" }
                            p class="text-2xl font-bold" id="metric-deployments-watched" { "0" }
                        }
                        div {
                            p class="text-sm text-base-content/70" { "StatefulSets" }
                            p class="text-2xl font-bold" id="metric-statefulsets-watched" { "0" }
                        }
                        div {
                            p class="text-sm text-base-content/70" { "DaemonSets" }
                            p class="text-2xl font-bold" id="metric-daemonsets-watched" { "0" }
                        }
                        div {
                            p class="text-sm text-base-content/70" { "Helm Releases" }
                            p class="text-2xl font-bold" id="metric-helm-releases-watched" { "0" }
                        }
                    }
                }
            }

            // Timeframe selector
            div class="card bg-base-100 shadow-xl mb-6" {
                div class="card-body" {
                    h2 class="card-title text-xl mb-4" { "Time Range" }
                    div class="btn-group" {
                        button class="btn btn-sm" onclick="setTimeRange('1h')" { "1 Hour" }
                        button class="btn btn-sm btn-active" onclick="setTimeRange('6h')" { "6 Hours" }
                        button class="btn btn-sm" onclick="setTimeRange('24h')" { "24 Hours" }
                        button class="btn btn-sm" onclick="setTimeRange('7d')" { "7 Days" }
                        button class="btn btn-sm" onclick="setTimeRange('30d')" { "30 Days" }
                    }
                }
            }

            // Time-series charts
            div class="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-6" {
                // Updates over time chart
                div class="card bg-base-100 shadow-xl" {
                    div class="card-body" {
                        h2 class="card-title text-xl mb-4" { "Updates Over Time" }
                        canvas id="updates-chart" {}
                    }
                }
                // Resources watched over time chart
                div class="card bg-base-100 shadow-xl" {
                    div class="card-body" {
                        h2 class="card-title text-xl mb-4" { "Resources Watched" }
                        canvas id="resources-chart" {}
                    }
                }
            }

            // Additional metrics charts
            div class="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-6" {
                // Rollback metrics chart
                div class="card bg-base-100 shadow-xl" {
                    div class="card-body" {
                        h2 class="card-title text-xl mb-4" { "Rollback Operations" }
                        canvas id="rollback-chart" {}
                    }
                }
                // Polling metrics chart
                div class="card bg-base-100 shadow-xl" {
                    div class="card-body" {
                        h2 class="card-title text-xl mb-4" { "Registry Polling" }
                        canvas id="polling-chart" {}
                    }
                }
            }

            div class="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-6" {
                // Helm metrics chart
                div class="card bg-base-100 shadow-xl" {
                    div class="card-body" {
                        h2 class="card-title text-xl mb-4" { "Helm Operations" }
                        canvas id="helm-chart" {}
                    }
                }
                // Notification metrics chart
                div class="card bg-base-100 shadow-xl" {
                    div class="card-body" {
                        h2 class="card-title text-xl mb-4" { "Notifications" }
                        canvas id="notifications-chart" {}
                    }
                }
            }

            div class="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-6" {
                // Webhook events chart
                div class="card bg-base-100 shadow-xl" {
                    div class="card-body" {
                        h2 class="card-title text-xl mb-4" { "Webhook Events" }
                        canvas id="webhook-chart" {}
                    }
                }
                // Reconciliation errors chart
                div class="card bg-base-100 shadow-xl" {
                    div class="card-body" {
                        h2 class="card-title text-xl mb-4" { "Controller Errors" }
                        canvas id="errors-chart" {}
                    }
                }
            }
        }

        // JavaScript for loading metrics
        script {
            (maud::PreEscaped(r#"
            document.addEventListener('DOMContentLoaded', function() {
                loadMetrics();
                // Refresh metrics every 30 seconds
                setInterval(loadMetrics, 30000);
            });

            let updatesChart = null;
            let resourcesChart = null;
            let rollbackChart = null;
            let pollingChart = null;
            let helmChart = null;
            let notificationsChart = null;
            let webhookChart = null;
            let errorsChart = null;
            let currentTimeRange = '6h';

            // Helper function to format timestamps for chart labels based on timeframe
            function formatTimestamp(timestamp, timeRange) {
                const date = new Date(timestamp);
                const now = new Date();
                const diffMs = now - date;
                const diffHours = diffMs / (1000 * 60 * 60);
                const diffDays = diffMs / (1000 * 60 * 60 * 24);

                // For 7d and 30d ranges, show date only
                if (timeRange === '7d' || timeRange === '30d') {
                    return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
                }
                // For data within the last 24 hours, show time only
                if (diffHours < 24) {
                    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
                }
                // For older data in short ranges, show date and time
                return date.toLocaleDateString([], { month: 'short', day: 'numeric' }) + ' ' +
                       date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
            }

            function setTimeRange(range) {
                currentTimeRange = range;
                // Update button states
                document.querySelectorAll('.btn-group .btn').forEach(btn => {
                    btn.classList.remove('btn-active');
                });
                event.target.classList.add('btn-active');
                // Reload charts with new timeframe
                loadCharts();
            }

            async function loadMetrics() {
                try {
                    const response = await fetch('/api/v1/metrics');
                    const metrics = await response.json();

                    // Update backend type
                    document.getElementById('backend-type').textContent = metrics.backend || 'Unknown';

                    // Update resource metric values from Prometheus
                    const resourceMetricIds = [
                        'deployments_watched',
                        'statefulsets_watched',
                        'daemonsets_watched',
                        'helm_releases_watched'
                    ];

                    resourceMetricIds.forEach(metricId => {
                        const elem = document.getElementById('metric-' + metricId.replace(/_/g, '-'));
                        if (elem && metrics[metricId] !== undefined) {
                            elem.textContent = Math.round(metrics[metricId]);
                        }
                    });

                    // Load UpdateRequest counts from API (these are persistent, not reset on restart)
                    await loadUpdateCounts();

                    // Show dashboard, hide loading
                    document.getElementById('metrics-loading').classList.add('hidden');
                    document.getElementById('metrics-dashboard').classList.remove('hidden');

                    // Load time-series charts
                    if (metrics.backend !== 'Live') {
                        await loadCharts();
                    }
                } catch (error) {
                    console.error('Failed to load metrics:', error);
                }
            }

            async function loadUpdateCounts() {
                try {
                    // Fetch all UpdateRequests
                    const response = await fetch('/api/v1/updates');
                    const updates = await response.json();

                    // Count by phase
                    const counts = {
                        pending: 0,
                        approved: 0,
                        applied: 0,
                        rejected: 0,
                        failed: 0
                    };

                    updates.forEach(update => {
                        let phase = update.status?.phase?.toLowerCase() || 'pending';

                        // Count approved separately (any update with approvedBy field set)
                        if (update.status?.approvedBy) {
                            counts.approved++;
                        }

                        // Map "completed" to "applied" for display
                        if (phase === 'completed') {
                            phase = 'applied';
                        }

                        // Count pending, applied, rejected, failed by phase
                        if (phase === 'pending' || phase === 'applied' || phase === 'rejected' || phase === 'failed') {
                            counts[phase]++;
                        }
                    });

                    // Update the cards
                    document.getElementById('metric-updates-pending').textContent = counts.pending;
                    document.getElementById('metric-updates-approved').textContent = counts.approved;
                    document.getElementById('metric-updates-applied').textContent = counts.applied;
                    document.getElementById('metric-updates-rejected').textContent = counts.rejected;
                    document.getElementById('metric-updates-failed').textContent = counts.failed;
                } catch (error) {
                    console.error('Failed to load update counts:', error);
                }
            }

            async function loadCharts() {
                try {
                    const timeParam = `?range=${currentTimeRange}`;

                    // Load updates time series
                    const updatesData = await Promise.all([
                        fetch('/api/v1/metrics/timeseries/headwind_updates_approved_total' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_updates_applied_total' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_updates_failed_total' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_updates_rejected_total' + timeParam).then(r => r.json())
                    ]);

                    // Load resources time series
                    const resourcesData = await Promise.all([
                        fetch('/api/v1/metrics/timeseries/headwind_deployments_watched' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_statefulsets_watched' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_daemonsets_watched' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_helm_releases_watched' + timeParam).then(r => r.json())
                    ]);

                    // Load rollback metrics
                    const rollbackData = await Promise.all([
                        fetch('/api/v1/metrics/timeseries/headwind_rollbacks_total' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_rollbacks_automatic_total' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_rollbacks_manual_total' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_rollbacks_failed_total' + timeParam).then(r => r.json())
                    ]);

                    // Load polling metrics
                    const pollingData = await Promise.all([
                        fetch('/api/v1/metrics/timeseries/headwind_polling_cycles_total' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_polling_new_tags_found_total' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_polling_errors_total' + timeParam).then(r => r.json())
                    ]);

                    // Load Helm metrics
                    const helmData = await Promise.all([
                        fetch('/api/v1/metrics/timeseries/headwind_helm_updates_found_total' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_helm_updates_applied_total' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_helm_repository_queries_total' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_helm_repository_errors_total' + timeParam).then(r => r.json())
                    ]);

                    // Load notification metrics
                    const notificationData = await Promise.all([
                        fetch('/api/v1/metrics/timeseries/headwind_notifications_sent_total' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_notifications_failed_total' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_notifications_slack_sent_total' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_notifications_teams_sent_total' + timeParam).then(r => r.json())
                    ]);

                    // Load webhook metrics
                    const webhookData = await Promise.all([
                        fetch('/api/v1/metrics/timeseries/headwind_webhook_events_total' + timeParam).then(r => r.json()),
                        fetch('/api/v1/metrics/timeseries/headwind_webhook_events_processed' + timeParam).then(r => r.json())
                    ]);

                    // Load error metrics
                    const errorData = await Promise.all([
                        fetch('/api/v1/metrics/timeseries/headwind_reconcile_errors_total' + timeParam).then(r => r.json())
                    ]);

                    // Create updates chart
                    if (updatesChart) updatesChart.destroy();
                    const updatesCtx = document.getElementById('updates-chart').getContext('2d');
                    updatesChart = new Chart(updatesCtx, {
                        type: 'line',
                        data: {
                            labels: updatesData[0].map(p => formatTimestamp(p.timestamp, currentTimeRange)),
                            datasets: [
                                {
                                    label: 'Approved',
                                    data: updatesData[0].map(p => p.value),
                                    borderColor: 'rgb(75, 192, 192)',
                                    backgroundColor: 'rgba(75, 192, 192, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'Applied',
                                    data: updatesData[1].map(p => p.value),
                                    borderColor: 'rgb(54, 162, 235)',
                                    backgroundColor: 'rgba(54, 162, 235, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'Failed',
                                    data: updatesData[2].map(p => p.value),
                                    borderColor: 'rgb(255, 99, 132)',
                                    backgroundColor: 'rgba(255, 99, 132, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'Rejected',
                                    data: updatesData[3].map(p => p.value),
                                    borderColor: 'rgb(255, 159, 64)',
                                    backgroundColor: 'rgba(255, 159, 64, 0.1)',
                                    tension: 0.4
                                }
                            ]
                        },
                        options: {
                            responsive: true,
                            maintainAspectRatio: true,
                            plugins: {
                                legend: {
                                    position: 'bottom'
                                }
                            },
                            scales: {
                                y: {
                                    beginAtZero: true
                                }
                            }
                        }
                    });

                    // Create resources chart
                    if (resourcesChart) resourcesChart.destroy();
                    const resourcesCtx = document.getElementById('resources-chart').getContext('2d');
                    resourcesChart = new Chart(resourcesCtx, {
                        type: 'line',
                        data: {
                            labels: resourcesData[0].map(p => formatTimestamp(p.timestamp, currentTimeRange)),
                            datasets: [
                                {
                                    label: 'Deployments',
                                    data: resourcesData[0].map(p => p.value),
                                    borderColor: 'rgb(153, 102, 255)',
                                    backgroundColor: 'rgba(153, 102, 255, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'StatefulSets',
                                    data: resourcesData[1].map(p => p.value),
                                    borderColor: 'rgb(255, 159, 64)',
                                    backgroundColor: 'rgba(255, 159, 64, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'DaemonSets',
                                    data: resourcesData[2].map(p => p.value),
                                    borderColor: 'rgb(255, 205, 86)',
                                    backgroundColor: 'rgba(255, 205, 86, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'Helm Releases',
                                    data: resourcesData[3].map(p => p.value),
                                    borderColor: 'rgb(201, 203, 207)',
                                    backgroundColor: 'rgba(201, 203, 207, 0.1)',
                                    tension: 0.4
                                }
                            ]
                        },
                        options: {
                            responsive: true,
                            maintainAspectRatio: true,
                            plugins: {
                                legend: {
                                    position: 'bottom'
                                }
                            },
                            scales: {
                                y: {
                                    beginAtZero: true
                                }
                            }
                        }
                    });

                    // Create rollback chart
                    if (rollbackChart) rollbackChart.destroy();
                    const rollbackCtx = document.getElementById('rollback-chart').getContext('2d');
                    rollbackChart = new Chart(rollbackCtx, {
                        type: 'line',
                        data: {
                            labels: rollbackData[0].map(p => formatTimestamp(p.timestamp, currentTimeRange)),
                            datasets: [
                                {
                                    label: 'Total Rollbacks',
                                    data: rollbackData[0].map(p => p.value),
                                    borderColor: 'rgb(153, 102, 255)',
                                    backgroundColor: 'rgba(153, 102, 255, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'Automatic',
                                    data: rollbackData[1].map(p => p.value),
                                    borderColor: 'rgb(54, 162, 235)',
                                    backgroundColor: 'rgba(54, 162, 235, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'Manual',
                                    data: rollbackData[2].map(p => p.value),
                                    borderColor: 'rgb(75, 192, 192)',
                                    backgroundColor: 'rgba(75, 192, 192, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'Failed',
                                    data: rollbackData[3].map(p => p.value),
                                    borderColor: 'rgb(255, 99, 132)',
                                    backgroundColor: 'rgba(255, 99, 132, 0.1)',
                                    tension: 0.4
                                }
                            ]
                        },
                        options: {
                            responsive: true,
                            maintainAspectRatio: true,
                            plugins: { legend: { position: 'bottom' } },
                            scales: { y: { beginAtZero: true } }
                        }
                    });

                    // Create polling chart
                    if (pollingChart) pollingChart.destroy();
                    const pollingCtx = document.getElementById('polling-chart').getContext('2d');
                    pollingChart = new Chart(pollingCtx, {
                        type: 'line',
                        data: {
                            labels: pollingData[0].map(p => formatTimestamp(p.timestamp, currentTimeRange)),
                            datasets: [
                                {
                                    label: 'Poll Cycles',
                                    data: pollingData[0].map(p => p.value),
                                    borderColor: 'rgb(54, 162, 235)',
                                    backgroundColor: 'rgba(54, 162, 235, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'New Tags Found',
                                    data: pollingData[1].map(p => p.value),
                                    borderColor: 'rgb(75, 192, 192)',
                                    backgroundColor: 'rgba(75, 192, 192, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'Errors',
                                    data: pollingData[2].map(p => p.value),
                                    borderColor: 'rgb(255, 99, 132)',
                                    backgroundColor: 'rgba(255, 99, 132, 0.1)',
                                    tension: 0.4
                                }
                            ]
                        },
                        options: {
                            responsive: true,
                            maintainAspectRatio: true,
                            plugins: { legend: { position: 'bottom' } },
                            scales: { y: { beginAtZero: true } }
                        }
                    });

                    // Create Helm chart
                    if (helmChart) helmChart.destroy();
                    const helmCtx = document.getElementById('helm-chart').getContext('2d');
                    helmChart = new Chart(helmCtx, {
                        type: 'line',
                        data: {
                            labels: helmData[0].map(p => formatTimestamp(p.timestamp, currentTimeRange)),
                            datasets: [
                                {
                                    label: 'Updates Found',
                                    data: helmData[0].map(p => p.value),
                                    borderColor: 'rgb(75, 192, 192)',
                                    backgroundColor: 'rgba(75, 192, 192, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'Updates Applied',
                                    data: helmData[1].map(p => p.value),
                                    borderColor: 'rgb(54, 162, 235)',
                                    backgroundColor: 'rgba(54, 162, 235, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'Repository Queries',
                                    data: helmData[2].map(p => p.value),
                                    borderColor: 'rgb(153, 102, 255)',
                                    backgroundColor: 'rgba(153, 102, 255, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'Query Errors',
                                    data: helmData[3].map(p => p.value),
                                    borderColor: 'rgb(255, 99, 132)',
                                    backgroundColor: 'rgba(255, 99, 132, 0.1)',
                                    tension: 0.4
                                }
                            ]
                        },
                        options: {
                            responsive: true,
                            maintainAspectRatio: true,
                            plugins: { legend: { position: 'bottom' } },
                            scales: { y: { beginAtZero: true } }
                        }
                    });

                    // Create notifications chart
                    if (notificationsChart) notificationsChart.destroy();
                    const notificationsCtx = document.getElementById('notifications-chart').getContext('2d');
                    notificationsChart = new Chart(notificationsCtx, {
                        type: 'line',
                        data: {
                            labels: notificationData[0].map(p => formatTimestamp(p.timestamp, currentTimeRange)),
                            datasets: [
                                {
                                    label: 'Total Sent',
                                    data: notificationData[0].map(p => p.value),
                                    borderColor: 'rgb(75, 192, 192)',
                                    backgroundColor: 'rgba(75, 192, 192, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'Failed',
                                    data: notificationData[1].map(p => p.value),
                                    borderColor: 'rgb(255, 99, 132)',
                                    backgroundColor: 'rgba(255, 99, 132, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'Slack',
                                    data: notificationData[2].map(p => p.value),
                                    borderColor: 'rgb(54, 162, 235)',
                                    backgroundColor: 'rgba(54, 162, 235, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'Teams',
                                    data: notificationData[3].map(p => p.value),
                                    borderColor: 'rgb(153, 102, 255)',
                                    backgroundColor: 'rgba(153, 102, 255, 0.1)',
                                    tension: 0.4
                                }
                            ]
                        },
                        options: {
                            responsive: true,
                            maintainAspectRatio: true,
                            plugins: { legend: { position: 'bottom' } },
                            scales: { y: { beginAtZero: true } }
                        }
                    });

                    // Create webhook chart
                    if (webhookChart) webhookChart.destroy();
                    const webhookCtx = document.getElementById('webhook-chart').getContext('2d');
                    webhookChart = new Chart(webhookCtx, {
                        type: 'line',
                        data: {
                            labels: webhookData[0].map(p => formatTimestamp(p.timestamp, currentTimeRange)),
                            datasets: [
                                {
                                    label: 'Events Received',
                                    data: webhookData[0].map(p => p.value),
                                    borderColor: 'rgb(54, 162, 235)',
                                    backgroundColor: 'rgba(54, 162, 235, 0.1)',
                                    tension: 0.4
                                },
                                {
                                    label: 'Events Processed',
                                    data: webhookData[1].map(p => p.value),
                                    borderColor: 'rgb(75, 192, 192)',
                                    backgroundColor: 'rgba(75, 192, 192, 0.1)',
                                    tension: 0.4
                                }
                            ]
                        },
                        options: {
                            responsive: true,
                            maintainAspectRatio: true,
                            plugins: { legend: { position: 'bottom' } },
                            scales: { y: { beginAtZero: true } }
                        }
                    });

                    // Create errors chart
                    if (errorsChart) errorsChart.destroy();
                    const errorsCtx = document.getElementById('errors-chart').getContext('2d');
                    errorsChart = new Chart(errorsCtx, {
                        type: 'line',
                        data: {
                            labels: errorData[0].map(p => formatTimestamp(p.timestamp, currentTimeRange)),
                            datasets: [
                                {
                                    label: 'Reconcile Errors',
                                    data: errorData[0].map(p => p.value),
                                    borderColor: 'rgb(255, 99, 132)',
                                    backgroundColor: 'rgba(255, 99, 132, 0.1)',
                                    tension: 0.4
                                }
                            ]
                        },
                        options: {
                            responsive: true,
                            maintainAspectRatio: true,
                            plugins: { legend: { position: 'bottom' } },
                            scales: { y: { beginAtZero: true } }
                        }
                    });
                } catch (error) {
                    console.error('Failed to load charts:', error);
                }
            }
            "#))
        }
    };

    base_layout("Observability - Headwind", content)
}
