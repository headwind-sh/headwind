use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Metrics backend type
#[derive(Debug, Clone, PartialEq)]
pub enum MetricsBackend {
    Prometheus { url: String },
    VictoriaMetrics { url: String },
    InfluxDB { url: String, database: String },
    Live,
}

/// A single metric data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricPoint {
    pub timestamp: DateTime<Utc>,
    pub value: f64,
}

/// Metric value for instant queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricValue {
    pub value: f64,
    pub timestamp: DateTime<Utc>,
}

/// Prometheus/VictoriaMetrics query response
#[derive(Debug, Deserialize)]
struct PrometheusResponse {
    status: String,
    data: PrometheusData,
}

#[derive(Debug, Deserialize)]
struct PrometheusData {
    result: Vec<PrometheusResult>,
}

#[derive(Debug, Deserialize)]
struct PrometheusResult {
    value: Option<(f64, String)>,
    values: Option<Vec<(f64, String)>>,
}

/// Metrics client trait for querying different backends
#[async_trait::async_trait]
pub trait MetricsClient: Send + Sync {
    /// Query a time range of metrics
    async fn query_range(
        &self,
        query: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        step: &str,
    ) -> Result<Vec<MetricPoint>>;

    /// Query an instant metric value
    async fn query_instant(&self, query: &str) -> Result<MetricValue>;

    /// Get the backend type
    fn backend_type(&self) -> &str;
}

/// Prometheus metrics client
pub struct PrometheusClient {
    url: String,
    client: Client,
}

impl PrometheusClient {
    pub fn new(url: String) -> Self {
        Self {
            url,
            client: Client::new(),
        }
    }

    /// Check if Prometheus is available
    pub async fn is_available(&self) -> bool {
        match self
            .client
            .get(format!("{}/api/v1/query", self.url))
            .query(&[("query", "up")])
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }
}

#[async_trait::async_trait]
impl MetricsClient for PrometheusClient {
    async fn query_range(
        &self,
        query: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        step: &str,
    ) -> Result<Vec<MetricPoint>> {
        let url = format!("{}/api/v1/query_range", self.url);

        debug!("Querying Prometheus: {} from {} to {}", query, start, end);

        let response = self
            .client
            .get(&url)
            .query(&[
                ("query", query),
                ("start", &start.timestamp().to_string()),
                ("end", &end.timestamp().to_string()),
                ("step", step),
            ])
            .send()
            .await
            .map_err(|e| anyhow!("Failed to query Prometheus: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Prometheus query failed {}: {}", status, body));
        }

        let prom_response: PrometheusResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse Prometheus response: {}", e))?;

        if prom_response.status != "success" {
            return Err(anyhow!("Prometheus query returned non-success status"));
        }

        // Parse results
        let mut points = Vec::new();
        for result in prom_response.data.result {
            if let Some(values) = result.values {
                for (ts, value_str) in values {
                    if let Ok(value) = value_str.parse::<f64>() {
                        points.push(MetricPoint {
                            timestamp: DateTime::from_timestamp(ts as i64, 0)
                                .unwrap_or_else(Utc::now),
                            value,
                        });
                    }
                }
            }
        }

        Ok(points)
    }

    async fn query_instant(&self, query: &str) -> Result<MetricValue> {
        let url = format!("{}/api/v1/query", self.url);

        debug!("Querying Prometheus instant: {}", query);

        let response = self
            .client
            .get(&url)
            .query(&[("query", query)])
            .send()
            .await
            .map_err(|e| anyhow!("Failed to query Prometheus: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Prometheus query failed {}: {}", status, body));
        }

        let prom_response: PrometheusResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse Prometheus response: {}", e))?;

        if prom_response.status != "success" {
            return Err(anyhow!("Prometheus query returned non-success status"));
        }

        // Parse first result
        if let Some(result) = prom_response.data.result.first()
            && let Some((ts, value_str)) = &result.value
            && let Ok(value) = value_str.parse::<f64>()
        {
            return Ok(MetricValue {
                timestamp: DateTime::from_timestamp(*ts as i64, 0).unwrap_or_else(Utc::now),
                value,
            });
        }

        Err(anyhow!("No data returned from Prometheus"))
    }

    fn backend_type(&self) -> &str {
        "Prometheus"
    }
}

/// VictoriaMetrics client (uses same API as Prometheus)
pub struct VictoriaMetricsClient {
    prometheus_client: PrometheusClient,
}

impl VictoriaMetricsClient {
    pub fn new(url: String) -> Self {
        Self {
            prometheus_client: PrometheusClient::new(url),
        }
    }

    /// Check if VictoriaMetrics is available
    pub async fn is_available(&self) -> bool {
        self.prometheus_client.is_available().await
    }
}

#[async_trait::async_trait]
impl MetricsClient for VictoriaMetricsClient {
    async fn query_range(
        &self,
        query: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        step: &str,
    ) -> Result<Vec<MetricPoint>> {
        self.prometheus_client
            .query_range(query, start, end, step)
            .await
    }

    async fn query_instant(&self, query: &str) -> Result<MetricValue> {
        self.prometheus_client.query_instant(query).await
    }

    fn backend_type(&self) -> &str {
        "VictoriaMetrics"
    }
}

/// InfluxDB client (queries InfluxDB v2 API)
pub struct InfluxDBClient {
    url: String,
    org: String,
    bucket: String,
    token: String,
    client: Client,
}

impl InfluxDBClient {
    pub fn new(url: String, org: String, bucket: String, token: String) -> Self {
        Self {
            url,
            org,
            bucket,
            token,
            client: Client::new(),
        }
    }

    /// Check if InfluxDB is available
    pub async fn is_available(&self) -> bool {
        let ping_url = format!("{}/ping", self.url);
        match self.client.get(&ping_url).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }
}

#[async_trait::async_trait]
impl MetricsClient for InfluxDBClient {
    async fn query_range(
        &self,
        query: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        step: &str,
    ) -> Result<Vec<MetricPoint>> {
        // Convert step to duration for window aggregate
        let step_duration = match step {
            "5m" => "5m",
            "1m" => "1m",
            "15m" => "15m",
            "1h" => "1h",
            _ => "5m",
        };

        // Build Flux query
        let flux_query = format!(
            r#"from(bucket: "{}")
  |> range(start: {}, stop: {})
  |> filter(fn: (r) => r["_measurement"] == "{}")
  |> aggregateWindow(every: {}, fn: mean, createEmpty: false)
  |> yield(name: "mean")"#,
            self.bucket,
            start.to_rfc3339(),
            end.to_rfc3339(),
            query,
            step_duration
        );

        let url = format!("{}/api/v2/query?org={}", self.url, self.org);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Token {}", self.token))
            .header("Content-Type", "application/vnd.flux")
            .body(flux_query)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to query InfluxDB: {}", e))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow!("InfluxDB query failed: {}", error_text));
        }

        let body = response.text().await?;

        // Parse CSV response from InfluxDB
        let mut points = Vec::new();
        for line in body.lines().skip(1) {
            // Skip header
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() >= 6
                && let (Some(time_str), Some(value_str)) = (parts.get(5), parts.get(6))
                && let (Ok(timestamp), Ok(value)) = (
                    DateTime::parse_from_rfc3339(time_str),
                    value_str.parse::<f64>(),
                )
            {
                points.push(MetricPoint {
                    timestamp: timestamp.with_timezone(&Utc),
                    value,
                });
            }
        }

        Ok(points)
    }

    async fn query_instant(&self, query: &str) -> Result<MetricValue> {
        // Query latest value
        let flux_query = format!(
            r#"from(bucket: "{}")
  |> range(start: -1h)
  |> filter(fn: (r) => r["_measurement"] == "{}")
  |> last()
  |> yield(name: "last")"#,
            self.bucket, query
        );

        let url = format!("{}/api/v2/query?org={}", self.url, self.org);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Token {}", self.token))
            .header("Content-Type", "application/vnd.flux")
            .body(flux_query)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to query InfluxDB: {}", e))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow!("InfluxDB query failed: {}", error_text));
        }

        let body = response.text().await?;

        // Parse CSV response
        for line in body.lines().skip(1) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() >= 6
                && let (Some(time_str), Some(value_str)) = (parts.get(5), parts.get(6))
                && let (Ok(timestamp), Ok(value)) = (
                    DateTime::parse_from_rfc3339(time_str),
                    value_str.parse::<f64>(),
                )
            {
                return Ok(MetricValue {
                    timestamp: timestamp.with_timezone(&Utc),
                    value,
                });
            }
        }

        Err(anyhow!(
            "No data returned from InfluxDB for metric {}",
            query
        ))
    }

    fn backend_type(&self) -> &str {
        "InfluxDB"
    }
}

/// Live metrics client (reads from /metrics endpoint directly)
pub struct LiveMetricsClient {
    metrics_url: String,
    client: Client,
}

impl LiveMetricsClient {
    pub fn new(metrics_url: String) -> Self {
        Self {
            metrics_url,
            client: Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl MetricsClient for LiveMetricsClient {
    async fn query_range(
        &self,
        _query: &str,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
        _step: &str,
    ) -> Result<Vec<MetricPoint>> {
        // Live metrics don't support historical queries
        Err(anyhow!("Historical queries not supported in live mode"))
    }

    async fn query_instant(&self, query: &str) -> Result<MetricValue> {
        // For live mode, we parse the /metrics endpoint
        // This is a simplified implementation - in production you'd use a proper parser
        let response = self
            .client
            .get(&self.metrics_url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch metrics: {}", e))?;

        let body = response.text().await?;

        // Simple parsing - look for the metric name
        for line in body.lines() {
            if line.starts_with(query)
                && !line.starts_with('#')
                && let Some(value_str) = line.split_whitespace().nth(1)
                && let Ok(value) = value_str.parse::<f64>()
            {
                return Ok(MetricValue {
                    timestamp: Utc::now(),
                    value,
                });
            }
        }

        Err(anyhow!("Metric {} not found", query))
    }

    fn backend_type(&self) -> &str {
        "Live"
    }
}

/// Auto-discover and create the appropriate metrics client
#[allow(clippy::too_many_arguments)]
pub async fn create_metrics_client(
    backend_type: &str,
    prometheus_url: Option<String>,
    prometheus_enabled: bool,
    victoriametrics_url: Option<String>,
    victoriametrics_enabled: bool,
    influxdb_url: Option<String>,
    influxdb_enabled: bool,
    influxdb_org: Option<String>,
    influxdb_bucket: Option<String>,
    influxdb_token: Option<String>,
) -> Box<dyn MetricsClient> {
    match backend_type {
        "auto" => {
            info!("Auto-discovering metrics backend...");

            // Try Prometheus first
            if prometheus_enabled && let Some(url) = prometheus_url.as_ref() {
                let client = PrometheusClient::new(url.clone());
                if client.is_available().await {
                    info!("Discovered Prometheus at {}", url);
                    return Box::new(client);
                }
                warn!("Prometheus configured at {} but not available", url);
            }

            // Try VictoriaMetrics
            if victoriametrics_enabled && let Some(url) = victoriametrics_url.as_ref() {
                let client = VictoriaMetricsClient::new(url.clone());
                if client.is_available().await {
                    info!("Discovered VictoriaMetrics at {}", url);
                    return Box::new(client);
                }
                warn!("VictoriaMetrics configured at {} but not available", url);
            }

            // Try InfluxDB
            if influxdb_enabled
                && let Some(url) = influxdb_url.clone()
                && let Some(org) = influxdb_org.clone()
                && let Some(bucket) = influxdb_bucket.clone()
                && let Some(token) = influxdb_token.clone()
            {
                let client = InfluxDBClient::new(url.clone(), org, bucket, token);
                if client.is_available().await {
                    info!("Discovered InfluxDB at {}", url);
                    return Box::new(client);
                }
                warn!("InfluxDB configured at {} but not available", url);
            }

            // Fall back to live metrics
            info!("No metrics backend discovered, falling back to live metrics");
            Box::new(LiveMetricsClient::new(
                "http://localhost:9090/metrics".to_string(),
            ))
        },
        "prometheus" => {
            let url = prometheus_url.unwrap_or_else(|| {
                "http://prometheus-server.monitoring.svc.cluster.local:80".to_string()
            });
            info!("Using Prometheus at {}", url);
            Box::new(PrometheusClient::new(url))
        },
        "victoriametrics" => {
            let url = victoriametrics_url.unwrap_or_else(|| {
                "http://victoria-metrics.monitoring.svc.cluster.local:8428".to_string()
            });
            info!("Using VictoriaMetrics at {}", url);
            Box::new(VictoriaMetricsClient::new(url))
        },
        "influxdb" => {
            let url = influxdb_url
                .unwrap_or_else(|| "http://influxdb.monitoring.svc.cluster.local:8086".to_string());
            let org = influxdb_org.unwrap_or_else(|| "headwind".to_string());
            let bucket = influxdb_bucket.unwrap_or_else(|| "metrics".to_string());
            let token = influxdb_token.unwrap_or_else(|| "headwind-test-token".to_string());
            info!("Using InfluxDB at {}", url);
            Box::new(InfluxDBClient::new(url, org, bucket, token))
        },
        "live" => {
            info!("Using live metrics only");
            Box::new(LiveMetricsClient::new(
                "http://localhost:9090/metrics".to_string(),
            ))
        },
        _ => {
            warn!(
                "Unknown backend type '{}', falling back to live metrics",
                backend_type
            );
            Box::new(LiveMetricsClient::new(
                "http://localhost:9090/metrics".to_string(),
            ))
        },
    }
}
