// Integration tests for rollback functionality
//
// These tests verify update history tracking, rollback operations,
// and health check monitoring

mod common;

use chrono::Utc;
use headwind::rollback::{AutoRollbackConfig, HealthStatus, UpdateHistory, UpdateHistoryEntry};
use std::collections::BTreeMap;

#[test]
fn test_update_history_entry_serialization() {
    let entry = UpdateHistoryEntry {
        container: "app".to_string(),
        image: "nginx:1.1.0".to_string(),
        timestamp: Utc::now(),
        update_request_name: Some("req-123".to_string()),
        approved_by: Some("webhook".to_string()),
    };

    let json = serde_json::to_string(&entry).expect("Failed to serialize");
    let deserialized: UpdateHistoryEntry =
        serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(deserialized.container, "app");
    assert_eq!(deserialized.image, "nginx:1.1.0");
    assert_eq!(
        deserialized.update_request_name,
        Some("req-123".to_string())
    );
    assert_eq!(deserialized.approved_by, Some("webhook".to_string()));
}

#[test]
fn test_update_history_entry_without_optional_fields() {
    let entry = UpdateHistoryEntry {
        container: "app".to_string(),
        image: "nginx:1.1.0".to_string(),
        timestamp: Utc::now(),
        update_request_name: None,
        approved_by: None,
    };

    let json = serde_json::to_string(&entry).expect("Failed to serialize");
    let deserialized: UpdateHistoryEntry =
        serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(deserialized.update_request_name, None);
    assert_eq!(deserialized.approved_by, None);
}

#[test]
fn test_update_history_new() {
    let history = UpdateHistory::new();
    assert_eq!(history.entries().len(), 0);
}

#[test]
fn test_update_history_add_entry() {
    let mut history = UpdateHistory::new();

    let entry1 = UpdateHistoryEntry {
        container: "app".to_string(),
        image: "nginx:1.0.0".to_string(),
        timestamp: Utc::now(),
        update_request_name: None,
        approved_by: None,
    };

    history.add_entry(entry1.clone());
    assert_eq!(history.entries().len(), 1);
    assert_eq!(history.entries()[0].image, "nginx:1.0.0");

    let entry2 = UpdateHistoryEntry {
        container: "app".to_string(),
        image: "nginx:1.1.0".to_string(),
        timestamp: Utc::now(),
        update_request_name: None,
        approved_by: None,
    };

    history.add_entry(entry2.clone());
    assert_eq!(history.entries().len(), 2);
    // Newest first
    assert_eq!(history.entries()[0].image, "nginx:1.1.0");
    assert_eq!(history.entries()[1].image, "nginx:1.0.0");
}

#[test]
fn test_update_history_get_previous_image() {
    let mut history = UpdateHistory::new();

    // Add entries in order: oldest to newest
    let entry1 = UpdateHistoryEntry {
        container: "app".to_string(),
        image: "nginx:1.0.0".to_string(),
        timestamp: Utc::now(),
        update_request_name: None,
        approved_by: None,
    };

    let entry2 = UpdateHistoryEntry {
        container: "app".to_string(),
        image: "nginx:1.1.0".to_string(),
        timestamp: Utc::now(),
        update_request_name: None,
        approved_by: None,
    };

    history.add_entry(entry1);
    history.add_entry(entry2);

    // Get previous (should be 1.0.0)
    let previous = history.get_previous_image("app");
    assert!(previous.is_some());
    assert_eq!(previous.unwrap().image, "nginx:1.0.0");
}

#[test]
fn test_update_history_max_entries() {
    let mut history = UpdateHistory::new();

    // Add more than MAX_HISTORY_ENTRIES (10)
    for i in 0..15 {
        let entry = UpdateHistoryEntry {
            container: "app".to_string(),
            image: format!("nginx:1.{}.0", i),
            timestamp: Utc::now(),
            update_request_name: None,
            approved_by: None,
        };
        history.add_entry(entry);
    }

    // Should keep only the 10 most recent
    let app_history = history.get_container_history("app");
    assert_eq!(app_history.len(), 10);
    // Most recent should be nginx:1.14.0
    assert_eq!(app_history[0].image, "nginx:1.14.0");
}

#[test]
fn test_update_history_multiple_containers() {
    let mut history = UpdateHistory::new();

    // Add entries for different containers
    let entry1 = UpdateHistoryEntry {
        container: "app".to_string(),
        image: "nginx:1.0.0".to_string(),
        timestamp: Utc::now(),
        update_request_name: None,
        approved_by: None,
    };

    let entry2 = UpdateHistoryEntry {
        container: "sidecar".to_string(),
        image: "envoy:1.0.0".to_string(),
        timestamp: Utc::now(),
        update_request_name: None,
        approved_by: None,
    };

    history.add_entry(entry1);
    history.add_entry(entry2);

    let app_history = history.get_container_history("app");
    let sidecar_history = history.get_container_history("sidecar");

    assert_eq!(app_history.len(), 1);
    assert_eq!(sidecar_history.len(), 1);
    assert_eq!(app_history[0].image, "nginx:1.0.0");
    assert_eq!(sidecar_history[0].image, "envoy:1.0.0");
}

#[test]
fn test_auto_rollback_config_from_annotations() {
    // Test with all annotations set
    let mut annotations = BTreeMap::new();
    annotations.insert("headwind.sh/auto-rollback".to_string(), "true".to_string());
    annotations.insert(
        "headwind.sh/rollback-timeout".to_string(),
        "600".to_string(),
    );
    annotations.insert(
        "headwind.sh/health-check-retries".to_string(),
        "5".to_string(),
    );

    let config = AutoRollbackConfig::from_annotations(&annotations);

    assert!(config.enabled);
    assert_eq!(config.timeout, 600);
    assert_eq!(config.retries, 5);
}

#[test]
fn test_auto_rollback_config_defaults() {
    // Test with no annotations (should use defaults)
    let annotations = BTreeMap::new();

    let config = AutoRollbackConfig::from_annotations(&annotations);

    assert!(!config.enabled); // Default is disabled
    assert_eq!(config.timeout, 300); // 5 minutes default
    assert_eq!(config.retries, 3); // 3 retries default
}

#[test]
fn test_auto_rollback_config_partial_annotations() {
    // Test with only some annotations set
    let mut annotations = BTreeMap::new();
    annotations.insert("headwind.sh/auto-rollback".to_string(), "true".to_string());
    annotations.insert(
        "headwind.sh/rollback-timeout".to_string(),
        "900".to_string(),
    );
    // health-check-retries not set, should use default

    let config = AutoRollbackConfig::from_annotations(&annotations);

    assert!(config.enabled);
    assert_eq!(config.timeout, 900);
    assert_eq!(config.retries, 3); // Default
}

#[test]
fn test_auto_rollback_config_invalid_values() {
    // Test with invalid annotation values (should use defaults)
    let mut annotations = BTreeMap::new();
    annotations.insert(
        "headwind.sh/auto-rollback".to_string(),
        "not-a-bool".to_string(),
    );
    annotations.insert(
        "headwind.sh/rollback-timeout".to_string(),
        "not-a-number".to_string(),
    );
    annotations.insert(
        "headwind.sh/health-check-retries".to_string(),
        "invalid".to_string(),
    );

    let config = AutoRollbackConfig::from_annotations(&annotations);

    // All should fallback to defaults when parsing fails
    assert!(!config.enabled);
    assert_eq!(config.timeout, 300);
    assert_eq!(config.retries, 3);
}

#[test]
fn test_health_status_equality() {
    assert_eq!(HealthStatus::Healthy, HealthStatus::Healthy);
    assert_eq!(HealthStatus::Progressing, HealthStatus::Progressing);
    assert_eq!(HealthStatus::Timeout, HealthStatus::Timeout);

    assert_eq!(
        HealthStatus::Failed("reason1".to_string()),
        HealthStatus::Failed("reason1".to_string())
    );

    assert_ne!(
        HealthStatus::Failed("reason1".to_string()),
        HealthStatus::Failed("reason2".to_string())
    );

    assert_ne!(HealthStatus::Healthy, HealthStatus::Progressing);
    assert_ne!(HealthStatus::Healthy, HealthStatus::Timeout);
    assert_ne!(
        HealthStatus::Healthy,
        HealthStatus::Failed("test".to_string())
    );
}

#[test]
fn test_update_history_to_json() {
    let mut history = UpdateHistory::new();

    let entry = UpdateHistoryEntry {
        container: "app".to_string(),
        image: "nginx:1.0.0".to_string(),
        timestamp: Utc::now(),
        update_request_name: Some("req-123".to_string()),
        approved_by: Some("admin".to_string()),
    };

    history.add_entry(entry);

    let json = history.to_json().expect("Failed to serialize to JSON");
    assert!(json.contains("nginx:1.0.0"));
    assert!(json.contains("req-123"));
    assert!(json.contains("admin"));
}

#[test]
fn test_auto_rollback_disabled_by_default() {
    let config = AutoRollbackConfig::default();

    assert!(!config.enabled);
    assert_eq!(config.timeout, 300);
    assert_eq!(config.retries, 3);
}

#[test]
fn test_update_history_entry_camel_case_serialization() {
    // Verify camelCase JSON format
    let entry = UpdateHistoryEntry {
        container: "app".to_string(),
        image: "nginx:1.0.0".to_string(),
        timestamp: Utc::now(),
        update_request_name: Some("req-123".to_string()),
        approved_by: Some("admin".to_string()),
    };

    let json = serde_json::to_string(&entry).expect("Failed to serialize");

    // Should use camelCase field names
    assert!(json.contains("updateRequestName"));
    assert!(json.contains("approvedBy"));
    assert!(!json.contains("update_request_name"));
    assert!(!json.contains("approved_by"));
}

#[test]
fn test_get_entry_by_index() {
    let mut history = UpdateHistory::new();

    for i in 0..5 {
        let entry = UpdateHistoryEntry {
            container: "app".to_string(),
            image: format!("nginx:1.{}.0", i),
            timestamp: Utc::now(),
            update_request_name: None,
            approved_by: None,
        };
        history.add_entry(entry);
    }

    // Index 0 = current (most recent)
    let current = history.get_entry_by_index("app", 0);
    assert!(current.is_some());
    assert_eq!(current.unwrap().image, "nginx:1.4.0");

    // Index 1 = previous
    let previous = history.get_entry_by_index("app", 1);
    assert!(previous.is_some());
    assert_eq!(previous.unwrap().image, "nginx:1.3.0");

    // Index 4 = oldest
    let oldest = history.get_entry_by_index("app", 4);
    assert!(oldest.is_some());
    assert_eq!(oldest.unwrap().image, "nginx:1.0.0");

    // Index beyond entries
    let beyond = history.get_entry_by_index("app", 10);
    assert!(beyond.is_none());
}
