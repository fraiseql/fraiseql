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

use std::{env, sync::Arc};

use reqwest::Client;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::{
    error::{ObserverError, Result},
    event::EntityEvent,
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

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        if self.url.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "elasticsearch.url cannot be empty".to_string(),
            });
        }
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

        Ok(Self {
            client: Arc::new(Client::new()),
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

        let response_body: Value =
            response.json().await.map_err(|e| ObserverError::DatabaseError {
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
        let config = ElasticsearchSinkConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_is_transient_error() {
        let config = ElasticsearchSinkConfig::default();
        let sink = ElasticsearchSink::new(config).unwrap();

        assert!(sink.is_transient_error("Connection refused"));
        assert!(sink.is_transient_error("timeout"));
        assert!(sink.is_transient_error("503 Service Unavailable"));
        assert!(sink.is_transient_error("502 Bad Gateway"));
        assert!(!sink.is_transient_error("Invalid index"));
    }
}
