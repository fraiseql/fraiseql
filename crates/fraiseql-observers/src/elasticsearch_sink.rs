//! Elasticsearch sink for indexing observer events.
//!
//! Consumes `EntityEvent` from a channel and indexes them into Elasticsearch
//! for operational search and debugging. Uses the HTTP API (not the elasticsearch crate)
//! to maintain loose coupling.
//!
//! # Architecture
//!
//! ```text
//! EntityEvent (from NATS)
//!     ↓
//! mpsc::channel
//!     ↓
//! ElasticsearchSink::run()
//!     ├─ Buffer events
//!     ├─ Batch on size or timeout
//!     └─ Bulk index to ES
//! ```

use std::{env, sync::Arc, time::Duration};

use reqwest::Client;

/// Timeout for all Elasticsearch HTTP requests.
const ES_SINK_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum byte size for an Elasticsearch bulk API response.
///
/// Bulk responses contain per-item status entries. 50 `MiB` is generous for
/// large batches while blocking allocation bombs from a compromised node.
const MAX_ES_BULK_RESPONSE_BYTES: usize = 50 * 1024 * 1024; // 50 MiB
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::{
    error::{ObserverError, Result},
    event::EntityEvent,
    ssrf::validate_outbound_url,
};

/// Elasticsearch sink configuration
#[derive(Debug, Clone)]
pub struct ElasticsearchSinkConfig {
    /// Elasticsearch base URL (e.g., "http://localhost:9200")
    pub url:                 String,
    /// Index name prefix (default: "fraiseql-events")
    pub index_prefix:        String,
    /// Bulk request size threshold (default: 1000)
    pub bulk_size:           usize,
    /// Flush timeout in seconds (default: 5)
    pub flush_interval_secs: u64,
    /// Max retries for bulk requests (default: 3)
    pub max_retries:         usize,
}

impl Default for ElasticsearchSinkConfig {
    fn default() -> Self {
        Self {
            url:                 env::var("FRAISEQL_ELASTICSEARCH_URL")
                .unwrap_or_else(|_| "http://localhost:9200".to_string()),
            index_prefix:        env::var("FRAISEQL_ELASTICSEARCH_INDEX_PREFIX")
                .unwrap_or_else(|_| "fraiseql-events".to_string()),
            bulk_size:           env::var("FRAISEQL_ELASTICSEARCH_BULK_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1000),
            flush_interval_secs: env::var("FRAISEQL_ELASTICSEARCH_FLUSH_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            max_retries:         env::var("FRAISEQL_ELASTICSEARCH_MAX_RETRIES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3),
        }
    }
}

impl ElasticsearchSinkConfig {
    /// Apply environment variable overrides
    #[must_use]
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(url) = env::var("FRAISEQL_ELASTICSEARCH_URL") {
            self.url = url;
        }
        if let Ok(prefix) = env::var("FRAISEQL_ELASTICSEARCH_INDEX_PREFIX") {
            self.index_prefix = prefix;
        }
        if let Ok(size) = env::var("FRAISEQL_ELASTICSEARCH_BULK_SIZE") {
            if let Ok(parsed) = size.parse() {
                self.bulk_size = parsed;
            }
        }
        if let Ok(interval) = env::var("FRAISEQL_ELASTICSEARCH_FLUSH_INTERVAL_SECS") {
            if let Ok(parsed) = interval.parse() {
                self.flush_interval_secs = parsed;
            }
        }
        if let Ok(retries) = env::var("FRAISEQL_ELASTICSEARCH_MAX_RETRIES") {
            if let Ok(parsed) = retries.parse() {
                self.max_retries = parsed;
            }
        }
        self
    }

    /// Validate configuration.
    ///
    /// In addition to field sanity checks, validates the URL for SSRF risks:
    /// private/loopback/link-local addresses are rejected.
    pub fn validate(&self) -> Result<()> {
        if self.url.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "elasticsearch.url cannot be empty".to_string(),
            });
        }
        validate_outbound_url(&self.url)?;
        if self.index_prefix.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "elasticsearch.index_prefix cannot be empty".to_string(),
            });
        }
        if self.bulk_size == 0 || self.bulk_size > 100_000 {
            return Err(ObserverError::InvalidConfig {
                message: "elasticsearch.bulk_size must be between 1 and 100,000".to_string(),
            });
        }
        if self.flush_interval_secs == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "elasticsearch.flush_interval_secs must be > 0".to_string(),
            });
        }
        Ok(())
    }
}

/// Elasticsearch sink for consuming and indexing events
pub struct ElasticsearchSink {
    client: Arc<Client>,
    config: ElasticsearchSinkConfig,
}

impl ElasticsearchSink {
    /// Create a new Elasticsearch sink
    pub fn new(config: ElasticsearchSinkConfig) -> Result<Self> {
        config.validate()?;

        info!(
            url = %config.url,
            index_prefix = %config.index_prefix,
            bulk_size = config.bulk_size,
            flush_interval_secs = config.flush_interval_secs,
            "Creating Elasticsearch sink"
        );

        let client = Client::builder()
            .timeout(ES_SINK_REQUEST_TIMEOUT)
            .build()
            .unwrap_or_default();
        Ok(Self {
            client: Arc::new(client),
            config,
        })
    }

    /// Create a sink without SSRF validation — for use in tests only.
    #[cfg(test)]
    pub(crate) fn new_unchecked(config: ElasticsearchSinkConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(ES_SINK_REQUEST_TIMEOUT)
            .build()
            .unwrap_or_default();
        Ok(Self {
            client: Arc::new(client),
            config,
        })
    }

    /// Health check - verify Elasticsearch is reachable
    pub async fn health_check(&self) -> Result<()> {
        let response = self
            .client
            .get(format!("{}/_cluster/health", self.config.url))
            .send()
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Elasticsearch health check failed: {e}"),
            })?;

        if response.status().is_success() {
            info!("Elasticsearch health check passed");
            Ok(())
        } else {
            Err(ObserverError::DatabaseError {
                reason: format!("Elasticsearch health check failed: {}", response.status()),
            })
        }
    }

    /// Run the sink, consuming events from the channel and indexing them
    pub async fn run(&self, mut rx: mpsc::Receiver<EntityEvent>) -> Result<()> {
        info!("Starting Elasticsearch sink");

        let mut event_buffer = Vec::with_capacity(self.config.bulk_size);
        let flush_timeout = std::time::Duration::from_secs(self.config.flush_interval_secs);

        loop {
            tokio::select! {
                // Receive next event
                Some(event) = rx.recv() => {
                    event_buffer.push(event);
                    if event_buffer.len() >= self.config.bulk_size {
                        if let Err(e) = self.flush_buffer(&mut event_buffer).await {
                            error!(error = %e, "Failed to flush buffer");
                            return Err(e);
                        }
                    }
                }

                // Flush on timeout
                () = tokio::time::sleep(flush_timeout) => {
                    if !event_buffer.is_empty() {
                        info!(count = event_buffer.len(), "Flushing buffer due to timeout");
                        if let Err(e) = self.flush_buffer(&mut event_buffer).await {
                            error!(error = %e, "Failed to flush buffer on timeout");
                            return Err(e);
                        }
                    }
                }
            }
        }
    }

    /// Flush event buffer to Elasticsearch
    async fn flush_buffer(&self, buffer: &mut Vec<EntityEvent>) -> Result<()> {
        if buffer.is_empty() {
            return Ok(());
        }

        let count = buffer.len();
        let mut last_error = None;

        // Retry logic with exponential backoff
        for attempt in 1..=self.config.max_retries {
            match self.try_bulk_index(buffer).await {
                Ok(()) => {
                    info!(count, attempt, "Batch indexed successfully");
                    buffer.clear();
                    return Ok(());
                },
                Err(e) => {
                    let error_msg = e.to_string();
                    if self.is_transient_error(&error_msg) {
                        warn!(
                            count,
                            attempt,
                            error = %error_msg,
                            "Transient error, retrying..."
                        );
                        last_error = Some(e);

                        if attempt < self.config.max_retries {
                            let backoff =
                                std::time::Duration::from_millis(100 * (2_u64.pow(attempt as u32)));
                            tokio::time::sleep(backoff).await;
                        }
                    } else {
                        error!(count, error = %error_msg, "Permanent error, giving up");
                        return Err(e);
                    }
                },
            }
        }

        Err(last_error.unwrap_or_else(|| ObserverError::DatabaseError {
            reason: "Failed to index events after retries".to_string(),
        }))
    }

    /// Attempt bulk indexing to Elasticsearch
    async fn try_bulk_index(&self, events: &[EntityEvent]) -> Result<()> {
        let mut body: Vec<Value> = Vec::new();

        for event in events {
            // Determine index name based on event timestamp
            let index_name =
                format!("{}-{}", self.config.index_prefix, event.timestamp.format("%Y.%m"));

            // Bulk API format: action metadata, then document
            body.push(json!({
                "index": {
                    "_index": index_name,
                    "_id": event.id.to_string()
                }
            }));

            // Create searchable text combining event details
            let search_text = format!(
                "{:?} {} {} {}",
                event.event_type,
                event.entity_type,
                event.entity_id,
                serde_json::to_string(&event.data).unwrap_or_default()
            );

            // Document body
            body.push(json!({
                "event_id": event.id.to_string(),
                "event_type": event.event_type,
                "entity_type": event.entity_type,
                "entity_id": event.entity_id.to_string(),
                "timestamp": event.timestamp.to_rfc3339(),
                "data": event.data,
                "changes": event.changes,
                "user_id": event.user_id,
                "search_text": search_text
            }));
        }

        let url = format!("{}/_bulk", self.config.url);
        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/x-ndjson")
            .json(&body)
            .send()
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Elasticsearch bulk request failed: {e}"),
            })?;

        let http_status = response.status();
        let body_bytes = response.bytes().await.map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to read Elasticsearch bulk response: {e}"),
        })?;
        if body_bytes.len() > MAX_ES_BULK_RESPONSE_BYTES {
            return Err(ObserverError::DatabaseError {
                reason: format!(
                    "Elasticsearch bulk response too large ({} bytes, max {MAX_ES_BULK_RESPONSE_BYTES})",
                    body_bytes.len()
                ),
            });
        }
        if !http_status.is_success() {
            return Err(ObserverError::DatabaseError {
                reason: format!("Elasticsearch bulk request returned HTTP {http_status}"),
            });
        }

        let response_body: Value =
            serde_json::from_slice(&body_bytes).map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to parse Elasticsearch response: {e}"),
            })?;

        // Check for errors in bulk response
        if response_body["errors"].as_bool().unwrap_or(false) {
            warn!(
                "Bulk indexing had errors: {}",
                response_body
                    .get("items")
                    .and_then(|items| items.as_array())
                    .map_or(0, std::vec::Vec::len)
            );
        }

        Ok(())
    }

    /// Classify whether an error is transient (retriable) or permanent
    fn is_transient_error(&self, error: &str) -> bool {
        error.contains("Connection refused")
            || error.contains("connection reset")
            || error.contains("timeout")
            || error.contains("503")
            || error.contains("502")
            || error.contains("Service Unavailable")
            || error.contains("Bad Gateway")
    }
}

#[allow(clippy::unwrap_used)]  // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = ElasticsearchSinkConfig::default();
        assert_eq!(config.bulk_size, 1000);
        assert_eq!(config.flush_interval_secs, 5);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_config_validate_empty_url() {
        let config = ElasticsearchSinkConfig {
            url: String::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_empty_prefix() {
        let config = ElasticsearchSinkConfig {
            index_prefix: String::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_invalid_bulk_size() {
        let config = ElasticsearchSinkConfig {
            bulk_size: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = ElasticsearchSinkConfig {
            bulk_size: 200_000,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_invalid_timeout() {
        let config = ElasticsearchSinkConfig {
            flush_interval_secs: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_valid() {
        let config = ElasticsearchSinkConfig {
            url: "https://es.example.com:9200".to_string(),
            ..ElasticsearchSinkConfig::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_is_transient_error() {
        let config = ElasticsearchSinkConfig {
            url: "https://es.example.com:9200".to_string(),
            ..ElasticsearchSinkConfig::default()
        };
        let sink = ElasticsearchSink::new(config).unwrap();

        assert!(sink.is_transient_error("Connection refused"));
        assert!(sink.is_transient_error("timeout"));
        assert!(sink.is_transient_error("503 Service Unavailable"));
        assert!(sink.is_transient_error("502 Bad Gateway"));
        assert!(!sink.is_transient_error("Invalid index"));
    }

    #[test]
    fn test_is_transient_error_connection_reset() {
        let config = ElasticsearchSinkConfig {
            url: "https://es.example.com:9200".to_string(),
            ..ElasticsearchSinkConfig::default()
        };
        let sink = ElasticsearchSink::new(config).unwrap();
        assert!(sink.is_transient_error("connection reset by peer"));
        assert!(!sink.is_transient_error("404 Not Found"));
        assert!(!sink.is_transient_error("400 Bad Request"));
    }

    #[test]
    fn test_config_max_bulk_size_boundary() {
        // 100_000 is invalid (upper bound exclusive)
        let config = ElasticsearchSinkConfig {
            bulk_size: 100_001,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // 100_000 is the maximum valid value
        let config = ElasticsearchSinkConfig {
            url: "https://es.example.com:9200".to_string(),
            bulk_size: 100_000,
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_with_env_overrides_returns_valid_config() {
        // with_env_overrides() is callable and produces a consistent config.
        // Full override behaviour is tested via env-var integration; here we
        // verify the function compiles, returns Self, and produces a valid result.
        let base = ElasticsearchSinkConfig {
            url: "https://es.example.com:9200".to_string(),
            ..ElasticsearchSinkConfig::default()
        };
        let after = base.with_env_overrides();
        assert!(
            after.validate().is_ok(),
            "config after with_env_overrides must still be valid"
        );
    }

    #[test]
    fn test_config_custom_values_validate() {
        let config = ElasticsearchSinkConfig {
            url:                 "https://es.example.com:9200".to_string(),
            index_prefix:        "my-app-events".to_string(),
            bulk_size:           500,
            flush_interval_secs: 30,
            max_retries:         5,
        };
        assert!(config.validate().is_ok());
    }

    // ── S23-H4: Elasticsearch sink timeout + bulk response cap ────────────────

    #[test]
    fn es_sink_timeout_is_set() {
        let secs = ES_SINK_REQUEST_TIMEOUT.as_secs();
        assert!(secs > 0 && secs <= 120, "ES sink timeout should be 1–120 s, got {secs}");
    }

    #[test]
    fn es_sink_bulk_response_cap_is_reasonable() {
        const { assert!(MAX_ES_BULK_RESPONSE_BYTES >= 1024 * 1024) }
        const { assert!(MAX_ES_BULK_RESPONSE_BYTES <= 500 * 1024 * 1024) }
    }

    #[tokio::test]
    async fn es_sink_oversized_bulk_response_is_rejected() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers::{method, path}};

        let mock = MockServer::start().await;
        let oversized = vec![b'x'; MAX_ES_BULK_RESPONSE_BYTES + 1];
        Mock::given(method("POST"))
            .and(path("/_bulk"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(oversized))
            .mount(&mock)
            .await;

        let config = ElasticsearchSinkConfig {
            url:                 mock.uri(),
            index_prefix:        "test".to_string(),
            bulk_size:           10,
            flush_interval_secs: 5,
            max_retries:         1,
        };
        let sink = ElasticsearchSink::new_unchecked(config).unwrap();

        // Drive the private try_bulk_index path via flush_buffer through a mock event.
        // We create a minimal event buffer and call the internal path indirectly.
        let event = crate::event::EntityEvent {
            id:          uuid::Uuid::nil(),
            event_type:  crate::event::EventKind::Created,
            entity_type: "Order".to_string(),
            entity_id:   uuid::Uuid::nil(),
            timestamp:   chrono::Utc::now(),
            data:        serde_json::json!({}),
            changes:     None,
            user_id:     None,
            tenant_id:   Some("tenant-1".to_string()),
        };
        let result = sink.try_bulk_index(&[event]).await;
        assert!(result.is_err(), "oversized bulk response must be rejected");
        let reason = match result.unwrap_err() {
            ObserverError::DatabaseError { reason } => reason,
            e => panic!("expected DatabaseError, got {e:?}"),
        };
        assert!(reason.contains("too large"), "error must mention size limit: {reason}");
    }
}
