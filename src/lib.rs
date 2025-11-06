// Library exports for integration testing
//
// This file exposes internal modules for integration tests while keeping
// the binary entrypoint in main.rs

pub mod approval;
pub mod controller;
pub mod metrics;
pub mod models;
pub mod policy;
pub mod polling;
pub mod webhook;

// Re-export commonly used types for testing
pub use models::crd::UpdateRequest;
pub use models::policy::{ResourcePolicy, UpdatePolicy};
pub use models::webhook::{DockerHubWebhook, ImagePushEvent, RegistryWebhook};

// Helper functions for testing
/// Convenience function for testing policy engine
pub fn test_should_update(
    current: &str,
    new_version: &str,
    policy_type: UpdatePolicy,
    pattern: Option<&str>,
) -> bool {
    let engine = policy::PolicyEngine;
    let policy = ResourcePolicy {
        policy: policy_type,
        pattern: pattern.map(String::from),
        ..Default::default()
    };

    engine
        .should_update(&policy, current, new_version)
        .unwrap_or(false)
}
