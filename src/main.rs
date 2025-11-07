use anyhow::Result;
use headwind::{approval, controller, metrics, notifications, polling, webhook};
use tracing::info;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "headwind=info,kube=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    info!("Starting Headwind - Kubernetes Update Operator");

    // Initialize notification manager
    notifications::init_notifications();

    // Initialize metrics server
    let metrics_handle = metrics::start_metrics_server().await?;

    // Initialize webhook server and get event senders (image & chart)
    let (webhook_handle, event_sender, chart_event_sender) =
        webhook::start_webhook_server().await?;

    // Initialize registry poller (optional, disabled by default)
    let polling_config = polling::PollingConfig {
        enabled: std::env::var("HEADWIND_POLLING_ENABLED")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(false),
        interval: std::env::var("HEADWIND_POLLING_INTERVAL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300),
    };
    let poller =
        polling::RegistryPoller::new(polling_config, event_sender, chart_event_sender).await?;
    let polling_handle = poller.start().await;

    // Initialize approval API server
    let approval_handle = approval::start_approval_server().await?;

    // Start Kubernetes controllers
    let controller_handle = controller::start_controllers().await?;

    info!("Headwind is running");

    // Wait for all services
    tokio::select! {
        _ = metrics_handle => info!("Metrics server stopped"),
        _ = webhook_handle => info!("Webhook server stopped"),
        _ = polling_handle => info!("Registry poller stopped"),
        _ = approval_handle => info!("Approval server stopped"),
        _ = controller_handle => info!("Controllers stopped"),
    }

    Ok(())
}
