use anyhow::Result;
use axum::{Router, http::StatusCode, response::IntoResponse, routing::get};
use lazy_static::lazy_static;
use prometheus::{Encoder, Histogram, HistogramOpts, IntCounter, IntGauge, Registry, TextEncoder};
use tokio::task::JoinHandle;
use tracing::info;

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();

    // Webhook metrics
    pub static ref WEBHOOK_EVENTS_TOTAL: IntCounter = IntCounter::new(
        "headwind_webhook_events_total",
        "Total number of webhook events received"
    ).unwrap();

    pub static ref WEBHOOK_EVENTS_PROCESSED: IntCounter = IntCounter::new(
        "headwind_webhook_events_processed",
        "Total number of webhook events successfully processed"
    ).unwrap();

    // Update metrics
    pub static ref UPDATES_PENDING: IntGauge = IntGauge::new(
        "headwind_updates_pending",
        "Number of updates pending approval"
    ).unwrap();

    pub static ref UPDATES_APPROVED: IntCounter = IntCounter::new(
        "headwind_updates_approved_total",
        "Total number of updates approved"
    ).unwrap();

    pub static ref UPDATES_REJECTED: IntCounter = IntCounter::new(
        "headwind_updates_rejected_total",
        "Total number of updates rejected"
    ).unwrap();

    pub static ref UPDATES_APPLIED: IntCounter = IntCounter::new(
        "headwind_updates_applied_total",
        "Total number of updates successfully applied"
    ).unwrap();

    pub static ref UPDATES_FAILED: IntCounter = IntCounter::new(
        "headwind_updates_failed_total",
        "Total number of updates that failed to apply"
    ).unwrap();

    // Controller metrics
    pub static ref RECONCILE_DURATION: Histogram = Histogram::with_opts(
        HistogramOpts::new(
            "headwind_reconcile_duration_seconds",
            "Time spent reconciling resources"
        ).buckets(vec![0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0])
    ).unwrap();

    pub static ref RECONCILE_ERRORS: IntCounter = IntCounter::new(
        "headwind_reconcile_errors_total",
        "Total number of reconciliation errors"
    ).unwrap();

    // Resource metrics
    pub static ref DEPLOYMENTS_WATCHED: IntGauge = IntGauge::new(
        "headwind_deployments_watched",
        "Number of Deployments being watched"
    ).unwrap();

    pub static ref HELM_RELEASES_WATCHED: IntGauge = IntGauge::new(
        "headwind_helm_releases_watched",
        "Number of Helm releases being watched"
    ).unwrap();

    // Polling metrics
    pub static ref POLLING_CYCLES_TOTAL: IntCounter = IntCounter::new(
        "headwind_polling_cycles_total",
        "Total number of registry polling cycles"
    ).unwrap();

    pub static ref POLLING_ERRORS_TOTAL: IntCounter = IntCounter::new(
        "headwind_polling_errors_total",
        "Total number of registry polling errors"
    ).unwrap();

    pub static ref POLLING_IMAGES_CHECKED: IntCounter = IntCounter::new(
        "headwind_polling_images_checked_total",
        "Total number of images checked during polling"
    ).unwrap();

    pub static ref POLLING_NEW_TAGS_FOUND: IntCounter = IntCounter::new(
        "headwind_polling_new_tags_found_total",
        "Total number of new tags discovered via polling"
    ).unwrap();

    // Helm metrics
    pub static ref HELM_CHART_VERSIONS_CHECKED: IntCounter = IntCounter::new(
        "headwind_helm_chart_versions_checked_total",
        "Total number of Helm chart version checks performed"
    ).unwrap();

    pub static ref HELM_UPDATES_FOUND: IntCounter = IntCounter::new(
        "headwind_helm_updates_found_total",
        "Total number of Helm chart updates discovered"
    ).unwrap();

    pub static ref HELM_UPDATES_APPROVED: IntCounter = IntCounter::new(
        "headwind_helm_updates_approved_total",
        "Total number of Helm chart updates approved by policy"
    ).unwrap();

    pub static ref HELM_UPDATES_REJECTED: IntCounter = IntCounter::new(
        "headwind_helm_updates_rejected_total",
        "Total number of Helm chart updates rejected by policy"
    ).unwrap();

    pub static ref HELM_UPDATES_APPLIED: IntCounter = IntCounter::new(
        "headwind_helm_updates_applied_total",
        "Total number of Helm chart updates successfully applied"
    ).unwrap();

    pub static ref HELM_REPOSITORY_QUERIES: IntCounter = IntCounter::new(
        "headwind_helm_repository_queries_total",
        "Total number of Helm repository index queries performed"
    ).unwrap();

    pub static ref HELM_REPOSITORY_ERRORS: IntCounter = IntCounter::new(
        "headwind_helm_repository_errors_total",
        "Total number of Helm repository query errors"
    ).unwrap();

    pub static ref HELM_REPOSITORY_QUERY_DURATION: Histogram = Histogram::with_opts(
        HistogramOpts::new(
            "headwind_helm_repository_query_duration_seconds",
            "Time spent querying Helm repositories for available versions"
        ).buckets(vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0])
    ).unwrap();

    // Rollback metrics
    pub static ref ROLLBACKS_TOTAL: IntCounter = IntCounter::new(
        "headwind_rollbacks_total",
        "Total number of rollback operations performed"
    ).unwrap();

    pub static ref ROLLBACKS_MANUAL: IntCounter = IntCounter::new(
        "headwind_rollbacks_manual_total",
        "Total number of manual rollback operations"
    ).unwrap();

    pub static ref ROLLBACKS_AUTOMATIC: IntCounter = IntCounter::new(
        "headwind_rollbacks_automatic_total",
        "Total number of automatic rollback operations"
    ).unwrap();

    pub static ref ROLLBACKS_FAILED: IntCounter = IntCounter::new(
        "headwind_rollbacks_failed_total",
        "Total number of failed rollback operations"
    ).unwrap();

    pub static ref DEPLOYMENT_HEALTH_CHECKS: IntCounter = IntCounter::new(
        "headwind_deployment_health_checks_total",
        "Total number of deployment health checks performed"
    ).unwrap();

    pub static ref DEPLOYMENT_HEALTH_FAILURES: IntCounter = IntCounter::new(
        "headwind_deployment_health_failures_total",
        "Total number of deployment health check failures detected"
    ).unwrap();

    // Notification metrics
    pub static ref NOTIFICATIONS_SENT_TOTAL: IntCounter = IntCounter::new(
        "headwind_notifications_sent_total",
        "Total number of notifications sent"
    ).unwrap();

    pub static ref NOTIFICATIONS_FAILED_TOTAL: IntCounter = IntCounter::new(
        "headwind_notifications_failed_total",
        "Total number of failed notification attempts"
    ).unwrap();

    pub static ref NOTIFICATIONS_SLACK_SENT: IntCounter = IntCounter::new(
        "headwind_notifications_slack_sent_total",
        "Total number of notifications sent to Slack"
    ).unwrap();

    pub static ref NOTIFICATIONS_TEAMS_SENT: IntCounter = IntCounter::new(
        "headwind_notifications_teams_sent_total",
        "Total number of notifications sent to Microsoft Teams"
    ).unwrap();

    pub static ref NOTIFICATIONS_WEBHOOK_SENT: IntCounter = IntCounter::new(
        "headwind_notifications_webhook_sent_total",
        "Total number of notifications sent via generic webhook"
    ).unwrap();

    // Update interval metrics
    pub static ref UPDATES_SKIPPED_INTERVAL: IntCounter = IntCounter::new(
        "headwind_updates_skipped_interval_total",
        "Total number of updates skipped due to minimum interval not elapsed"
    ).unwrap();
}

pub fn register_metrics() {
    REGISTRY
        .register(Box::new(WEBHOOK_EVENTS_TOTAL.clone()))
        .ok();
    REGISTRY
        .register(Box::new(WEBHOOK_EVENTS_PROCESSED.clone()))
        .ok();
    REGISTRY.register(Box::new(UPDATES_PENDING.clone())).ok();
    REGISTRY.register(Box::new(UPDATES_APPROVED.clone())).ok();
    REGISTRY.register(Box::new(UPDATES_REJECTED.clone())).ok();
    REGISTRY.register(Box::new(UPDATES_APPLIED.clone())).ok();
    REGISTRY.register(Box::new(UPDATES_FAILED.clone())).ok();
    REGISTRY.register(Box::new(RECONCILE_DURATION.clone())).ok();
    REGISTRY.register(Box::new(RECONCILE_ERRORS.clone())).ok();
    REGISTRY
        .register(Box::new(DEPLOYMENTS_WATCHED.clone()))
        .ok();
    REGISTRY
        .register(Box::new(HELM_RELEASES_WATCHED.clone()))
        .ok();
    REGISTRY
        .register(Box::new(POLLING_CYCLES_TOTAL.clone()))
        .ok();
    REGISTRY
        .register(Box::new(POLLING_ERRORS_TOTAL.clone()))
        .ok();
    REGISTRY
        .register(Box::new(POLLING_IMAGES_CHECKED.clone()))
        .ok();
    REGISTRY
        .register(Box::new(POLLING_NEW_TAGS_FOUND.clone()))
        .ok();
    REGISTRY
        .register(Box::new(HELM_CHART_VERSIONS_CHECKED.clone()))
        .ok();
    REGISTRY.register(Box::new(HELM_UPDATES_FOUND.clone())).ok();
    REGISTRY
        .register(Box::new(HELM_UPDATES_APPROVED.clone()))
        .ok();
    REGISTRY
        .register(Box::new(HELM_UPDATES_REJECTED.clone()))
        .ok();
    REGISTRY
        .register(Box::new(HELM_UPDATES_APPLIED.clone()))
        .ok();
    REGISTRY
        .register(Box::new(HELM_REPOSITORY_QUERIES.clone()))
        .ok();
    REGISTRY
        .register(Box::new(HELM_REPOSITORY_ERRORS.clone()))
        .ok();
    REGISTRY
        .register(Box::new(HELM_REPOSITORY_QUERY_DURATION.clone()))
        .ok();
    REGISTRY.register(Box::new(ROLLBACKS_TOTAL.clone())).ok();
    REGISTRY.register(Box::new(ROLLBACKS_MANUAL.clone())).ok();
    REGISTRY
        .register(Box::new(ROLLBACKS_AUTOMATIC.clone()))
        .ok();
    REGISTRY.register(Box::new(ROLLBACKS_FAILED.clone())).ok();
    REGISTRY
        .register(Box::new(DEPLOYMENT_HEALTH_CHECKS.clone()))
        .ok();
    REGISTRY
        .register(Box::new(DEPLOYMENT_HEALTH_FAILURES.clone()))
        .ok();
    REGISTRY
        .register(Box::new(NOTIFICATIONS_SENT_TOTAL.clone()))
        .ok();
    REGISTRY
        .register(Box::new(NOTIFICATIONS_FAILED_TOTAL.clone()))
        .ok();
    REGISTRY
        .register(Box::new(NOTIFICATIONS_SLACK_SENT.clone()))
        .ok();
    REGISTRY
        .register(Box::new(NOTIFICATIONS_TEAMS_SENT.clone()))
        .ok();
    REGISTRY
        .register(Box::new(NOTIFICATIONS_WEBHOOK_SENT.clone()))
        .ok();
    REGISTRY
        .register(Box::new(UPDATES_SKIPPED_INTERVAL.clone()))
        .ok();

    info!("Metrics registered");
}

pub async fn start_metrics_server() -> Result<JoinHandle<()>> {
    register_metrics();

    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/health", get(health_check));

    let addr = "0.0.0.0:9090";
    info!("Starting metrics server on {}", addr);

    let handle = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .expect("Failed to bind metrics server");

        axum::serve(listener, app)
            .await
            .expect("Metrics server failed");
    });

    Ok(handle)
}

async fn metrics_handler() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = vec![];

    if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to encode metrics: {}", e),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        buffer,
    )
        .into_response()
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}
