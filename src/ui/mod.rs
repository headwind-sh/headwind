use axum::{Router, routing::get};
use std::net::SocketAddr;
use tower_http::services::ServeDir;
use tracing::info;

pub mod routes;
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
        // Serve static files (CSS, JS, images)
        .nest_service("/static", ServeDir::new("src/static"))
        // Health check endpoint
        .route("/health", get(routes::health_check))
        // Dashboard route (main page)
        .route("/", get(routes::dashboard))
        // Individual update request detail view
        .route("/updates/{namespace}/{name}", get(routes::update_detail))
}
