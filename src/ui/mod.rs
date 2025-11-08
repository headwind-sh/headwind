use axum::{
    Router,
    routing::{get, post, put},
};
use std::net::SocketAddr;
use tracing::info;

pub mod routes;
pub mod static_files;
pub mod templates;

/// Start the Web UI server
pub async fn start_ui_server() -> Result<(), Box<dyn std::error::Error>> {
    let app = create_router();

    let addr = SocketAddr::from(([0, 0, 0, 0], 8082));
    info!("Starting Web UI server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Create the Axum router for the Web UI
fn create_router() -> Router {
    Router::new()
        // Serve embedded static files (CSS, JS, images)
        .route("/static/{*path}", get(static_files::serve_static))
        // Health check endpoint
        .route("/health", get(routes::health_check))
        // Dashboard route (main page)
        .route("/", get(routes::dashboard))
        // Settings page
        .route("/settings", get(routes::settings_page))
        // Observability page
        .route("/observability", get(routes::observability_page))
        // Individual update request detail view
        .route("/updates/{namespace}/{name}", get(routes::update_detail))
        // Settings API endpoints
        .route("/api/v1/settings", get(routes::get_settings))
        .route("/api/v1/settings", put(routes::update_settings))
        .route(
            "/api/v1/settings/test-notification",
            post(routes::test_notification),
        )
        // Observability API endpoints
        .route("/api/v1/metrics", get(routes::get_metrics_data))
        .route(
            "/api/v1/metrics/timeseries/{metric_name}",
            get(routes::get_metrics_timeseries),
        )
        // UpdateRequest API endpoint for counts
        .route("/api/v1/updates", get(routes::list_update_requests))
}
