// Common test utilities for integration tests
//
// This module provides helper functions and fixtures for testing Headwind
// in a Kubernetes-like environment

use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{Container, PodSpec, PodTemplateSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use std::collections::BTreeMap;

/// Creates a sample Deployment for testing
pub fn create_test_deployment(
    name: &str,
    namespace: &str,
    image: &str,
    annotations: Option<BTreeMap<String, String>>,
) -> Deployment {
    let mut labels = BTreeMap::new();
    labels.insert("app".to_string(), name.to_string());

    Deployment {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            namespace: Some(namespace.to_string()),
            annotations,
            ..Default::default()
        },
        spec: Some(k8s_openapi::api::apps::v1::DeploymentSpec {
            replicas: Some(1),
            selector: LabelSelector {
                match_labels: Some(labels.clone()),
                ..Default::default()
            },
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(labels),
                    ..Default::default()
                }),
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: "app".to_string(),
                        image: Some(image.to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            },
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Creates Headwind annotations for a Deployment
pub fn headwind_annotations(
    policy: &str,
    require_approval: bool,
    pattern: Option<&str>,
) -> BTreeMap<String, String> {
    let mut annotations = BTreeMap::new();
    annotations.insert("headwind.sh/policy".to_string(), policy.to_string());
    annotations.insert(
        "headwind.sh/require-approval".to_string(),
        require_approval.to_string(),
    );
    if let Some(p) = pattern {
        annotations.insert("headwind.sh/pattern".to_string(), p.to_string());
    }
    annotations
}

/// Creates a Docker Hub webhook payload
pub fn create_dockerhub_webhook_payload(repo: &str, tag: &str) -> serde_json::Value {
    // Parse repo into namespace and name
    let parts: Vec<&str> = repo.split('/').collect();
    let (namespace, name) = if parts.len() == 2 {
        (parts[0], parts[1])
    } else {
        ("library", repo)
    };

    serde_json::json!({
        "push_data": {
            "tag": tag
        },
        "repository": {
            "repo_name": repo,
            "namespace": namespace,
            "name": name
        }
    })
}

/// Creates an OCI registry webhook payload
pub fn create_registry_webhook_payload(image: &str, tag: &str) -> serde_json::Value {
    serde_json::json!({
        "events": [{
            "action": "push",
            "target": {
                "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
                "size": 1234,
                "digest": "sha256:abc123",
                "length": 1234,
                "repository": image,
                "url": format!("https://registry.example.com/v2/{}/manifests/{}", image, tag),
                "tag": tag
            }
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_test_deployment() {
        let deploy = create_test_deployment(
            "test-app",
            "default",
            "nginx:1.0.0",
            Some(headwind_annotations("minor", true, None)),
        );

        assert_eq!(deploy.metadata.name, Some("test-app".to_string()));
        assert_eq!(deploy.metadata.namespace, Some("default".to_string()));

        let annotations = deploy.metadata.annotations.unwrap();
        assert_eq!(
            annotations.get("headwind.sh/policy"),
            Some(&"minor".to_string())
        );
        assert_eq!(
            annotations.get("headwind.sh/require-approval"),
            Some(&"true".to_string())
        );
    }

    #[test]
    fn test_headwind_annotations() {
        let annotations = headwind_annotations("patch", false, Some("v1.*"));

        assert_eq!(
            annotations.get("headwind.sh/policy"),
            Some(&"patch".to_string())
        );
        assert_eq!(
            annotations.get("headwind.sh/require-approval"),
            Some(&"false".to_string())
        );
        assert_eq!(
            annotations.get("headwind.sh/pattern"),
            Some(&"v1.*".to_string())
        );
    }
}
