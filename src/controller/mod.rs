mod deployment;

use anyhow::Result;
use tokio::task::JoinHandle;
use tracing::info;

pub use deployment::{
    DeploymentController, handle_image_update, update_deployment_image,
    update_deployment_image_with_tracking,
};

pub async fn start_controllers() -> Result<JoinHandle<()>> {
    info!("Starting Kubernetes controllers");

    // Check if controllers should be disabled (useful for testing webhooks only)
    let controllers_enabled = std::env::var("HEADWIND_CONTROLLERS_ENABLED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(true);

    let handle = if controllers_enabled {
        // Start deployment controller
        let deployment_controller = DeploymentController::new().await?;

        tokio::spawn(async move {
            deployment_controller.run().await;
            tracing::info!("Deployment controller stopped");

            // TODO: Start Helm controller and join both
            // tokio::join!(deployment_handle, helm_handle);
        })
    } else {
        info!("Controllers disabled via HEADWIND_CONTROLLERS_ENABLED=false");
        // Return a task that never completes
        tokio::spawn(async {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        })
    };

    Ok(handle)
}
