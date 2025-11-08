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
