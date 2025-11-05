mod deployment;

use anyhow::Result;
use tokio::task::JoinHandle;
use tracing::info;

pub use deployment::DeploymentController;

pub async fn start_controllers() -> Result<JoinHandle<()>> {
    info!("Starting Kubernetes controllers");

    let handle = tokio::spawn(async {
        // Start deployment controller
        let deployment_controller = DeploymentController::new()
            .await
            .expect("Failed to create deployment controller");

        let deployment_handle = tokio::spawn(async move { deployment_controller.run().await });

        // TODO: Start Helm controller
        // let helm_handle = tokio::spawn(async move {
        //     helm_controller.run().await
        // });

        // Wait for all controllers
        if let Err(e) = deployment_handle.await {
            tracing::error!("Deployment controller failed: {}", e);
        }
    });

    Ok(handle)
}
