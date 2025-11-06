mod deployment;

use anyhow::Result;
use tokio::task::JoinHandle;
use tracing::info;

pub use deployment::DeploymentController;

pub async fn start_controllers() -> Result<JoinHandle<()>> {
    info!("Starting Kubernetes controllers");

    // Start deployment controller
    let deployment_controller = DeploymentController::new().await?;

    let handle = tokio::spawn(async move {
        deployment_controller.run().await;
        tracing::info!("Deployment controller stopped");

        // TODO: Start Helm controller and join both
        // tokio::join!(deployment_handle, helm_handle);
    });

    Ok(handle)
}
