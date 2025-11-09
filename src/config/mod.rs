use futures::StreamExt;
use k8s_openapi::api::core::v1::{ConfigMap, Secret};
use kube::runtime::{WatchStreamExt, watcher};
use kube::{Api, Client};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use tokio::time::Duration;
use tracing::{debug, error, info, warn};

const CONFIGMAP_NAME: &str = "headwind-config";
const SECRET_NAME: &str = "headwind-secrets";
const NAMESPACE: &str = "headwind-system";

/// Headwind configuration loaded from ConfigMap and Secret
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadwindConfig {
    pub polling: PollingConfig,
    pub helm: HelmConfig,
    pub controllers: ControllersConfig,
    pub notifications: NotificationsConfig,
    pub observability: ObservabilityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollingConfig {
    pub enabled: bool,
    pub interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelmConfig {
    #[serde(rename = "autoDiscovery")]
    pub auto_discovery: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllersConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationsConfig {
    pub slack: SlackConfig,
    pub teams: TeamsConfig,
    pub webhook: WebhookConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    pub enabled: bool,
    #[serde(rename = "webhookUrl")]
    pub webhook_url: Option<String>,
    pub channel: Option<String>,
    pub username: Option<String>,
    #[serde(rename = "iconEmoji")]
    pub icon_emoji: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsConfig {
    pub enabled: bool,
    #[serde(rename = "webhookUrl")]
    pub webhook_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub enabled: bool,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    #[serde(rename = "metricsBackend")]
    pub metrics_backend: String, // auto, prometheus, victoriametrics, influxdb, live
    pub prometheus: PrometheusConfig,
    pub victoriametrics: VictoriaMetricsConfig,
    pub influxdb: InfluxDBConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrometheusConfig {
    pub enabled: bool,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VictoriaMetricsConfig {
    pub enabled: bool,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfluxDBConfig {
    pub enabled: bool,
    pub url: Option<String>,
    pub org: Option<String>,
    pub bucket: Option<String>,
    pub token: Option<String>,
}

impl Default for HeadwindConfig {
    fn default() -> Self {
        Self {
            polling: PollingConfig {
                enabled: false,
                interval: 300,
            },
            helm: HelmConfig {
                auto_discovery: true,
            },
            controllers: ControllersConfig { enabled: true },
            notifications: NotificationsConfig {
                slack: SlackConfig {
                    enabled: false,
                    webhook_url: None,
                    channel: None,
                    username: Some("Headwind".to_string()),
                    icon_emoji: Some(":sailboat:".to_string()),
                },
                teams: TeamsConfig {
                    enabled: false,
                    webhook_url: None,
                },
                webhook: WebhookConfig {
                    enabled: false,
                    url: None,
                },
            },
            observability: ObservabilityConfig {
                metrics_backend: "auto".to_string(),
                prometheus: PrometheusConfig {
                    enabled: true,
                    url: Some(
                        "http://prometheus-server.monitoring.svc.cluster.local:80".to_string(),
                    ),
                },
                victoriametrics: VictoriaMetricsConfig {
                    enabled: true,
                    url: Some(
                        "http://victoria-metrics.monitoring.svc.cluster.local:8428".to_string(),
                    ),
                },
                influxdb: InfluxDBConfig {
                    enabled: false,
                    url: Some("http://influxdb.monitoring.svc.cluster.local:8086".to_string()),
                    org: Some("headwind".to_string()),
                    bucket: Some("metrics".to_string()),
                    token: Some("headwind-test-token".to_string()),
                },
            },
        }
    }
}

impl HeadwindConfig {
    /// Load configuration from ConfigMap and Secret
    pub async fn load(client: Client) -> Result<Self, Box<dyn std::error::Error>> {
        info!("Loading Headwind configuration from ConfigMap and Secret");

        let configmap_api: Api<ConfigMap> = Api::namespaced(client.clone(), NAMESPACE);
        let secret_api: Api<Secret> = Api::namespaced(client.clone(), NAMESPACE);

        // Load ConfigMap (create with defaults if doesn't exist)
        let config_data = match configmap_api.get(CONFIGMAP_NAME).await {
            Ok(cm) => cm.data.unwrap_or_default(),
            Err(e) => {
                warn!(
                    "ConfigMap {} not found, using defaults: {}",
                    CONFIGMAP_NAME, e
                );
                BTreeMap::new()
            },
        };

        // Load Secret (create with empty values if doesn't exist)
        let secret_data = match secret_api.get(SECRET_NAME).await {
            Ok(secret) => secret
                .data
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| {
                    (
                        k,
                        String::from_utf8(v.0)
                            .unwrap_or_default()
                            .trim()
                            .to_string(),
                    )
                })
                .collect::<BTreeMap<String, String>>(),
            Err(e) => {
                warn!(
                    "Secret {} not found, using empty values: {}",
                    SECRET_NAME, e
                );
                BTreeMap::new()
            },
        };

        // Parse configuration
        let config = Self {
            polling: PollingConfig {
                enabled: parse_bool(&config_data, "polling.enabled", false),
                interval: parse_u64(&config_data, "polling.interval", 300),
            },
            helm: HelmConfig {
                auto_discovery: parse_bool(&config_data, "helm.autoDiscovery", true),
            },
            controllers: ControllersConfig {
                enabled: parse_bool(&config_data, "controllers.enabled", true),
            },
            notifications: NotificationsConfig {
                slack: SlackConfig {
                    enabled: parse_bool(&config_data, "slack.enabled", false),
                    webhook_url: get_secret_value(&secret_data, "slack-webhook-url"),
                    channel: parse_optional_string(&config_data, "slack.channel"),
                    username: parse_optional_string(&config_data, "slack.username")
                        .or_else(|| Some("Headwind".to_string())),
                    icon_emoji: parse_optional_string(&config_data, "slack.iconEmoji")
                        .or_else(|| Some(":sailboat:".to_string())),
                },
                teams: TeamsConfig {
                    enabled: parse_bool(&config_data, "teams.enabled", false),
                    webhook_url: get_secret_value(&secret_data, "teams-webhook-url"),
                },
                webhook: WebhookConfig {
                    enabled: parse_bool(&config_data, "webhook.enabled", false),
                    url: get_secret_value(&secret_data, "webhook-url"),
                },
            },
            observability: ObservabilityConfig {
                metrics_backend: parse_optional_string(
                    &config_data,
                    "observability.metricsBackend",
                )
                .unwrap_or_else(|| "auto".to_string()),
                prometheus: PrometheusConfig {
                    enabled: parse_bool(&config_data, "observability.prometheus.enabled", true),
                    url: parse_optional_string(&config_data, "observability.prometheus.url")
                        .or_else(|| {
                            Some(
                                "http://prometheus-server.monitoring.svc.cluster.local:80"
                                    .to_string(),
                            )
                        }),
                },
                victoriametrics: VictoriaMetricsConfig {
                    enabled: parse_bool(
                        &config_data,
                        "observability.victoriametrics.enabled",
                        true,
                    ),
                    url: parse_optional_string(&config_data, "observability.victoriametrics.url")
                        .or_else(|| {
                            Some(
                                "http://victoria-metrics.monitoring.svc.cluster.local:8428"
                                    .to_string(),
                            )
                        }),
                },
                influxdb: InfluxDBConfig {
                    enabled: parse_bool(&config_data, "observability.influxdb.enabled", false),
                    url: parse_optional_string(&config_data, "observability.influxdb.url").or_else(
                        || Some("http://influxdb.monitoring.svc.cluster.local:8086".to_string()),
                    ),
                    org: parse_optional_string(&config_data, "observability.influxdb.org")
                        .or_else(|| Some("headwind".to_string())),
                    bucket: parse_optional_string(&config_data, "observability.influxdb.bucket")
                        .or_else(|| Some("metrics".to_string())),
                    token: parse_optional_string(&config_data, "observability.influxdb.token")
                        .or_else(|| Some("headwind-test-token".to_string())),
                },
            },
        };

        debug!("Loaded configuration: {:?}", config);
        Ok(config)
    }

    /// Save configuration to ConfigMap and Secret
    pub async fn save(&self, client: Client) -> Result<(), Box<dyn std::error::Error>> {
        info!("Saving Headwind configuration to ConfigMap and Secret");

        let configmap_api: Api<ConfigMap> = Api::namespaced(client.clone(), NAMESPACE);
        let secret_api: Api<Secret> = Api::namespaced(client, NAMESPACE);

        // Build ConfigMap data
        let mut config_data = BTreeMap::new();
        config_data.insert(
            "polling.enabled".to_string(),
            self.polling.enabled.to_string(),
        );
        config_data.insert(
            "polling.interval".to_string(),
            self.polling.interval.to_string(),
        );
        config_data.insert(
            "helm.autoDiscovery".to_string(),
            self.helm.auto_discovery.to_string(),
        );
        config_data.insert(
            "controllers.enabled".to_string(),
            self.controllers.enabled.to_string(),
        );
        config_data.insert(
            "slack.enabled".to_string(),
            self.notifications.slack.enabled.to_string(),
        );
        config_data.insert(
            "slack.channel".to_string(),
            self.notifications.slack.channel.clone().unwrap_or_default(),
        );
        config_data.insert(
            "slack.username".to_string(),
            self.notifications
                .slack
                .username
                .clone()
                .unwrap_or_else(|| "Headwind".to_string()),
        );
        config_data.insert(
            "slack.iconEmoji".to_string(),
            self.notifications
                .slack
                .icon_emoji
                .clone()
                .unwrap_or_else(|| ":sailboat:".to_string()),
        );
        config_data.insert(
            "teams.enabled".to_string(),
            self.notifications.teams.enabled.to_string(),
        );
        config_data.insert(
            "webhook.enabled".to_string(),
            self.notifications.webhook.enabled.to_string(),
        );
        config_data.insert(
            "observability.metricsBackend".to_string(),
            self.observability.metrics_backend.clone(),
        );
        config_data.insert(
            "observability.prometheus.enabled".to_string(),
            self.observability.prometheus.enabled.to_string(),
        );
        config_data.insert(
            "observability.prometheus.url".to_string(),
            self.observability
                .prometheus
                .url
                .clone()
                .unwrap_or_default(),
        );
        config_data.insert(
            "observability.victoriametrics.enabled".to_string(),
            self.observability.victoriametrics.enabled.to_string(),
        );
        config_data.insert(
            "observability.victoriametrics.url".to_string(),
            self.observability
                .victoriametrics
                .url
                .clone()
                .unwrap_or_default(),
        );
        config_data.insert(
            "observability.influxdb.enabled".to_string(),
            self.observability.influxdb.enabled.to_string(),
        );
        config_data.insert(
            "observability.influxdb.url".to_string(),
            self.observability.influxdb.url.clone().unwrap_or_default(),
        );
        config_data.insert(
            "observability.influxdb.org".to_string(),
            self.observability.influxdb.org.clone().unwrap_or_default(),
        );
        config_data.insert(
            "observability.influxdb.bucket".to_string(),
            self.observability
                .influxdb
                .bucket
                .clone()
                .unwrap_or_default(),
        );
        config_data.insert(
            "observability.influxdb.token".to_string(),
            self.observability
                .influxdb
                .token
                .clone()
                .unwrap_or_default(),
        );

        // Update or create ConfigMap
        let configmap = ConfigMap {
            metadata: kube::api::ObjectMeta {
                name: Some(CONFIGMAP_NAME.to_string()),
                namespace: Some(NAMESPACE.to_string()),
                labels: Some(
                    [("app".to_string(), "headwind".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..Default::default()
            },
            data: Some(config_data),
            ..Default::default()
        };

        match configmap_api.get(CONFIGMAP_NAME).await {
            Ok(_) => {
                configmap_api
                    .replace(CONFIGMAP_NAME, &Default::default(), &configmap)
                    .await?;
                info!("Updated ConfigMap {}", CONFIGMAP_NAME);
            },
            Err(_) => {
                configmap_api
                    .create(&Default::default(), &configmap)
                    .await?;
                info!("Created ConfigMap {}", CONFIGMAP_NAME);
            },
        }

        // Build Secret data
        let mut secret_data = BTreeMap::new();
        if let Some(url) = &self.notifications.slack.webhook_url {
            secret_data.insert("slack-webhook-url".to_string(), url.clone());
        }
        if let Some(url) = &self.notifications.teams.webhook_url {
            secret_data.insert("teams-webhook-url".to_string(), url.clone());
        }
        if let Some(url) = &self.notifications.webhook.url {
            secret_data.insert("webhook-url".to_string(), url.clone());
        }

        // Update or create Secret
        let secret = Secret {
            metadata: kube::api::ObjectMeta {
                name: Some(SECRET_NAME.to_string()),
                namespace: Some(NAMESPACE.to_string()),
                labels: Some(
                    [("app".to_string(), "headwind".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..Default::default()
            },
            string_data: Some(secret_data),
            ..Default::default()
        };

        match secret_api.get(SECRET_NAME).await {
            Ok(_) => {
                secret_api
                    .replace(SECRET_NAME, &Default::default(), &secret)
                    .await?;
                info!("Updated Secret {}", SECRET_NAME);
            },
            Err(_) => {
                secret_api.create(&Default::default(), &secret).await?;
                info!("Created Secret {}", SECRET_NAME);
            },
        }

        Ok(())
    }
}

// Helper functions for parsing configuration values
fn parse_bool(data: &BTreeMap<String, String>, key: &str, default: bool) -> bool {
    data.get(key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn parse_u64(data: &BTreeMap<String, String>, key: &str, default: u64) -> u64 {
    data.get(key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn parse_optional_string(data: &BTreeMap<String, String>, key: &str) -> Option<String> {
    data.get(key)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

fn get_secret_value(data: &BTreeMap<String, String>, key: &str) -> Option<String> {
    data.get(key)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

/// Global configuration cache for hot-reload
static GLOBAL_CONFIG: once_cell::sync::Lazy<Arc<RwLock<Option<HeadwindConfig>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(None)));

/// Get the current cached configuration
pub fn get_cached_config() -> Option<HeadwindConfig> {
    GLOBAL_CONFIG.read().ok()?.clone()
}

/// Update the cached configuration
fn update_cached_config(config: HeadwindConfig) {
    if let Ok(mut cache) = GLOBAL_CONFIG.write() {
        *cache = Some(config);
        info!("Configuration cache updated");
    }
}

/// Start watching ConfigMap and Secret for changes
/// This enables hot-reload of configuration without restarting the application
pub async fn start_config_watcher(client: Client) {
    info!("Starting configuration watcher for hot-reload");

    let configmap_api: Api<ConfigMap> = Api::namespaced(client.clone(), NAMESPACE);
    let secret_api: Api<Secret> = Api::namespaced(client.clone(), NAMESPACE);

    // Load initial configuration
    match HeadwindConfig::load(client.clone()).await {
        Ok(config) => {
            info!("Initial configuration loaded successfully");
            update_cached_config(config);
        },
        Err(e) => {
            error!("Failed to load initial configuration: {}", e);
        },
    }

    // Spawn ConfigMap watcher
    let cm_client = client.clone();
    tokio::spawn(async move {
        loop {
            let watcher_config = watcher::Config::default().timeout(60).any_semantic();

            let mut stream = watcher(configmap_api.clone(), watcher_config)
                .default_backoff()
                .boxed();

            info!(
                "ConfigMap watcher started for {}/{}",
                NAMESPACE, CONFIGMAP_NAME
            );

            while let Some(event) = stream.next().await {
                match event {
                    Ok(watcher::Event::Apply(cm)) => {
                        if cm.metadata.name.as_deref() == Some(CONFIGMAP_NAME) {
                            info!(
                                "ConfigMap {} changed, reloading configuration",
                                CONFIGMAP_NAME
                            );
                            if let Ok(config) = HeadwindConfig::load(cm_client.clone()).await {
                                update_cached_config(config);
                            } else {
                                error!("Failed to reload configuration after ConfigMap change");
                            }
                        }
                    },
                    Ok(watcher::Event::Delete(_)) => {
                        warn!("ConfigMap {} was deleted", CONFIGMAP_NAME);
                    },
                    Ok(watcher::Event::Init) => {
                        info!("ConfigMap watcher initialized");
                    },
                    Ok(watcher::Event::InitApply(cm)) => {
                        if cm.metadata.name.as_deref() == Some(CONFIGMAP_NAME) {
                            info!("ConfigMap {} initial load", CONFIGMAP_NAME);
                        }
                    },
                    Ok(watcher::Event::InitDone) => {
                        info!("ConfigMap watcher init done");
                    },
                    Err(e) => {
                        error!("ConfigMap watcher error: {}", e);
                    },
                }
            }

            warn!("ConfigMap watcher stream ended, restarting in 5 seconds...");
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });

    // Spawn Secret watcher
    let secret_client = client.clone();
    tokio::spawn(async move {
        loop {
            let watcher_config = watcher::Config::default().timeout(60).any_semantic();

            let mut stream = watcher(secret_api.clone(), watcher_config)
                .default_backoff()
                .boxed();

            info!("Secret watcher started for {}/{}", NAMESPACE, SECRET_NAME);

            while let Some(event) = stream.next().await {
                match event {
                    Ok(watcher::Event::Apply(secret)) => {
                        if secret.metadata.name.as_deref() == Some(SECRET_NAME) {
                            info!("Secret {} changed, reloading configuration", SECRET_NAME);
                            if let Ok(config) = HeadwindConfig::load(secret_client.clone()).await {
                                update_cached_config(config);
                            } else {
                                error!("Failed to reload configuration after Secret change");
                            }
                        }
                    },
                    Ok(watcher::Event::Delete(_)) => {
                        warn!("Secret {} was deleted", SECRET_NAME);
                    },
                    Ok(watcher::Event::Init) => {
                        info!("Secret watcher initialized");
                    },
                    Ok(watcher::Event::InitApply(secret)) => {
                        if secret.metadata.name.as_deref() == Some(SECRET_NAME) {
                            info!("Secret {} initial load", SECRET_NAME);
                        }
                    },
                    Ok(watcher::Event::InitDone) => {
                        info!("Secret watcher init done");
                    },
                    Err(e) => {
                        error!("Secret watcher error: {}", e);
                    },
                }
            }

            warn!("Secret watcher stream ended, restarting in 5 seconds...");
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });

    info!("Configuration watchers started successfully");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = HeadwindConfig::default();
        assert!(!config.polling.enabled);
        assert_eq!(config.polling.interval, 300);
        assert!(config.helm.auto_discovery);
        assert!(config.controllers.enabled);
        assert!(!config.notifications.slack.enabled);
    }

    #[test]
    fn test_parse_bool() {
        let mut data = BTreeMap::new();
        data.insert("test.key".to_string(), "true".to_string());
        assert!(parse_bool(&data, "test.key", false));

        data.insert("test.key".to_string(), "false".to_string());
        assert!(!parse_bool(&data, "test.key", true));

        assert!(parse_bool(&data, "missing.key", true));
    }

    #[test]
    fn test_parse_u64() {
        let mut data = BTreeMap::new();
        data.insert("test.key".to_string(), "500".to_string());
        assert_eq!(parse_u64(&data, "test.key", 100), 500);

        assert_eq!(parse_u64(&data, "missing.key", 100), 100);
    }
}
