// Integration tests for Webhook models and parsing
//
// These tests verify that webhook payloads from different registries
// are correctly parsed

mod common;

use headwind::models::webhook::{DockerHubWebhook, ImagePushEvent, RegistryWebhook};

#[test]
fn test_dockerhub_webhook_parsing() {
    let payload = common::create_dockerhub_webhook_payload("myorg/myapp", "v1.2.3");

    let webhook: DockerHubWebhook = serde_json::from_value(payload).expect("Failed to parse");

    assert_eq!(webhook.repository.repo_name, "myorg/myapp");
    assert_eq!(webhook.push_data.tag, "v1.2.3");
}

#[test]
fn test_registry_webhook_parsing() {
    let payload = common::create_registry_webhook_payload("myorg/myapp", "v1.2.3");

    let webhook: RegistryWebhook = serde_json::from_value(payload).expect("Failed to parse");

    assert!(!webhook.events.is_empty());
    assert_eq!(webhook.events[0].action, "push");
    assert_eq!(webhook.events[0].target.repository, "myorg/myapp");
    assert_eq!(webhook.events[0].target.tag.as_ref().unwrap(), "v1.2.3");
}

#[test]
fn test_multiple_registry_events() {
    let payload = serde_json::json!({
        "events": [
            {
                "action": "push",
                "target": {
                    "repository": "app1",
                    "tag": "v1.0.0",
                    "digest": "sha256:abc123"
                }
            },
            {
                "action": "push",
                "target": {
                    "repository": "app2",
                    "tag": "v2.0.0",
                    "digest": "sha256:def456"
                }
            }
        ]
    });

    let webhook: RegistryWebhook = serde_json::from_value(payload).unwrap();

    assert_eq!(webhook.events.len(), 2);
    assert_eq!(webhook.events[0].target.repository, "app1");
    assert_eq!(webhook.events[0].target.tag.as_ref().unwrap(), "v1.0.0");
    assert_eq!(webhook.events[1].target.repository, "app2");
    assert_eq!(webhook.events[1].target.tag.as_ref().unwrap(), "v2.0.0");
}

#[test]
fn test_registry_event_without_tag() {
    let payload = serde_json::json!({
        "events": [{
            "action": "push",
            "target": {
                "repository": "myapp",
                "digest": "sha256:abc123"
            }
        }]
    });

    let webhook: RegistryWebhook = serde_json::from_value(payload).unwrap();

    assert_eq!(webhook.events.len(), 1);
    assert_eq!(webhook.events[0].target.repository, "myapp");
    assert!(webhook.events[0].target.tag.is_none());
}

#[test]
fn test_dockerhub_webhook_with_special_characters() {
    let payload = serde_json::json!({
        "push_data": {
            "tag": "v1.2.3-rc1+build.123"
        },
        "repository": {
            "repo_name": "my-org/my-app-name",
            "namespace": "my-org",
            "name": "my-app-name"
        }
    });

    let webhook: DockerHubWebhook = serde_json::from_value(payload).unwrap();

    assert_eq!(webhook.repository.repo_name, "my-org/my-app-name");
    assert_eq!(webhook.push_data.tag, "v1.2.3-rc1+build.123");
}

#[test]
fn test_image_push_event_full_image_name() {
    // Test full_image() method
    let event = ImagePushEvent {
        repository: "myorg/myapp".to_string(),
        tag: "v1.2.3".to_string(),
        digest: Some("sha256:abc123".to_string()),
        registry: "docker.io".to_string(),
    };

    // Docker Hub should omit registry in output
    assert_eq!(event.full_image(), "myorg/myapp:v1.2.3");

    // Other registries should include registry
    let event2 = ImagePushEvent {
        repository: "myorg/myapp".to_string(),
        tag: "v1.2.3".to_string(),
        digest: Some("sha256:abc123".to_string()),
        registry: "gcr.io".to_string(),
    };
    assert_eq!(event2.full_image(), "gcr.io/myorg/myapp:v1.2.3");

    // Empty registry should also omit
    let event3 = ImagePushEvent {
        repository: "library/nginx".to_string(),
        tag: "latest".to_string(),
        digest: None,
        registry: "".to_string(),
    };
    assert_eq!(event3.full_image(), "library/nginx:latest");
}

#[test]
fn test_dockerhub_webhook_minimal_payload() {
    // Minimal valid Docker Hub webhook
    let payload = serde_json::json!({
        "push_data": {
            "tag": "latest"
        },
        "repository": {
            "repo_name": "library/nginx",
            "namespace": "library",
            "name": "nginx"
        }
    });

    let webhook: DockerHubWebhook = serde_json::from_value(payload).unwrap();

    assert_eq!(webhook.repository.repo_name, "library/nginx");
    assert_eq!(webhook.push_data.tag, "latest");
}

#[test]
fn test_registry_webhook_with_optional_fields() {
    let payload = serde_json::json!({
        "events": [{
            "action": "push",
            "target": {
                "repository": "test/app",
                "tag": "v1.0.0",
                "digest": "sha256:abc123",
                "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
                "url": "https://registry.example.com/v2/test/app/manifests/v1.0.0"
            },
            "request": {
                "id": "12345",
                "method": "PUT",
                "useragent": "docker/20.10"
            }
        }]
    });

    let webhook: RegistryWebhook = serde_json::from_value(payload).unwrap();

    assert_eq!(webhook.events.len(), 1);
    assert_eq!(webhook.events[0].target.repository, "test/app");
    assert_eq!(
        webhook.events[0].target.media_type.as_ref().unwrap(),
        "application/vnd.docker.distribution.manifest.v2+json"
    );
    assert!(webhook.events[0].request.is_some());
}
