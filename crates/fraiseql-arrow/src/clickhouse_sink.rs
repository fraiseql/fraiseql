//! ClickHouse sink for consuming Arrow RecordBatches and inserting analytics events.
//!
//! This module provides a high-performance sink that converts Arrow RecordBatches
//! (from the NATS→Arrow bridge) into ClickHouse database events. It handles batching,
//! retry logic, and graceful shutdown.
//!
//! # Architecture
//!
//! ```text
//! Arrow RecordBatch (8 columns)
//!     ↓
//! ClickHouseSink::run(mpsc::Receiver)
//!     ↓
//! Extract columns via downcast
//!     ↓
//! Convert to EventRow structs
//!     ↓
//! clickhouse::Inserter (batching)
//!     ↓
//! ClickHouse MergeTree table
//! ```

use std::time::Duration;

use arrow::{
    array::{Array, StringArray, TimestampMicrosecondArray},
    record_batch::RecordBatch,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::error::{ArrowFlightError, Result};

/// ClickHouse sink configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickHouseSinkConfig {
    /// ClickHouse HTTP endpoint (e.g., "http://localhost:8123")
    #[serde(default = "default_clickhouse_url")]
    pub url: String,

    /// Database name (default: "default")
    #[serde(default = "default_clickhouse_database")]
    pub database: String,

    /// Table name (default: "fraiseql_events")
    #[serde(default = "default_clickhouse_table")]
    pub table: String,

    /// Batch size before flushing (default: 10000)
    #[serde(default = "default_clickhouse_batch_size")]
    pub batch_size: usize,

    /// Batch timeout in seconds (default: 5)
    #[serde(default = "default_clickhouse_batch_timeout_secs")]
    pub batch_timeout_secs: u64,

    /// Maximum number of retries for transient errors (default: 3)
    #[serde(default = "default_clickhouse_max_retries")]
    pub max_retries: usize,
}

/// Default ClickHouse URL
fn default_clickhouse_url() -> String {
    std::env::var("FRAISEQL_CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".to_string())
}

/// Default ClickHouse database
fn default_clickhouse_database() -> String {
    std::env::var("FRAISEQL_CLICKHOUSE_DATABASE").unwrap_or_else(|_| "default".to_string())
}

/// Default ClickHouse table
fn default_clickhouse_table() -> String {
    std::env::var("FRAISEQL_CLICKHOUSE_TABLE").unwrap_or_else(|_| "fraiseql_events".to_string())
}

/// Default batch size
fn default_clickhouse_batch_size() -> usize {
    std::env::var("FRAISEQL_CLICKHOUSE_BATCH_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

/// Default batch timeout
fn default_clickhouse_batch_timeout_secs() -> u64 {
    std::env::var("FRAISEQL_CLICKHOUSE_BATCH_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5)
}

/// Default max retries
fn default_clickhouse_max_retries() -> usize {
    std::env::var("FRAISEQL_CLICKHOUSE_MAX_RETRIES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3)
}

impl Default for ClickHouseSinkConfig {
    fn default() -> Self {
        Self {
            url:                default_clickhouse_url(),
            database:           default_clickhouse_database(),
            table:              default_clickhouse_table(),
            batch_size:         default_clickhouse_batch_size(),
            batch_timeout_secs: default_clickhouse_batch_timeout_secs(),
            max_retries:        default_clickhouse_max_retries(),
        }
    }
}

impl ClickHouseSinkConfig {
    /// Apply environment variable overrides to the configuration
    #[must_use]
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(url) = std::env::var("FRAISEQL_CLICKHOUSE_URL") {
            self.url = url;
        }
        if let Ok(database) = std::env::var("FRAISEQL_CLICKHOUSE_DATABASE") {
            self.database = database;
        }
        if let Ok(table) = std::env::var("FRAISEQL_CLICKHOUSE_TABLE") {
            self.table = table;
        }
        if let Ok(batch_size) = std::env::var("FRAISEQL_CLICKHOUSE_BATCH_SIZE") {
            if let Ok(size) = batch_size.parse() {
                self.batch_size = size;
            }
        }
        if let Ok(timeout) = std::env::var("FRAISEQL_CLICKHOUSE_BATCH_TIMEOUT_SECS") {
            if let Ok(secs) = timeout.parse() {
                self.batch_timeout_secs = secs;
            }
        }
        if let Ok(retries) = std::env::var("FRAISEQL_CLICKHOUSE_MAX_RETRIES") {
            if let Ok(count) = retries.parse() {
                self.max_retries = count;
            }
        }
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.url.is_empty() {
            return Err(ArrowFlightError::Configuration(
                "ClickHouse URL cannot be empty".to_string(),
            ));
        }
        if self.database.is_empty() {
            return Err(ArrowFlightError::Configuration(
                "ClickHouse database cannot be empty".to_string(),
            ));
        }
        if self.table.is_empty() {
            return Err(ArrowFlightError::Configuration(
                "ClickHouse table cannot be empty".to_string(),
            ));
        }
        if self.batch_size == 0 || self.batch_size > 100_000 {
            return Err(ArrowFlightError::Configuration(
                "Batch size must be between 1 and 100,000".to_string(),
            ));
        }
        if self.batch_timeout_secs == 0 {
            return Err(ArrowFlightError::Configuration(
                "Batch timeout must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

/// Event row for ClickHouse insertion
#[derive(Debug, Clone, Serialize, Deserialize, clickhouse::Row)]
pub struct EventRow {
    /// Unique event identifier
    pub event_id:    String,
    /// Type of event (e.g., "created", "updated", "deleted")
    pub event_type:  String,
    /// Type of entity that was affected (e.g., "User", "Product")
    pub entity_type: String,
    /// ID of the entity that was affected
    pub entity_id:   String,
    /// Timestamp in microseconds since UTC epoch
    pub timestamp:   i64,
    /// Event data as JSON string
    pub data:        String,
    /// Optional user ID associated with the event
    pub user_id:     Option<String>,
    /// Optional organization ID associated with the event
    pub org_id:      Option<String>,
}

/// ClickHouse sink for consuming Arrow RecordBatches
pub struct ClickHouseSink {
    config: ClickHouseSinkConfig,
    #[cfg(feature = "clickhouse")]
    client: clickhouse::Client,
}

impl ClickHouseSink {
    /// Create a new ClickHouse sink with the given configuration
    pub fn new(config: ClickHouseSinkConfig) -> Result<Self> {
        config.validate()?;

        #[cfg(feature = "clickhouse")]
        {
            let client = clickhouse::Client::default()
                .with_url(&config.url)
                .with_database(&config.database);

            info!(
                url = %config.url,
                database = %config.database,
                table = %config.table,
                batch_size = config.batch_size,
                timeout_secs = config.batch_timeout_secs,
                "Creating ClickHouse sink"
            );

            Ok(Self { config, client })
        }

        #[cfg(not(feature = "clickhouse"))]
        {
            Err(ArrowFlightError::External(
                "ClickHouse feature not enabled (compile with --features clickhouse)".to_string(),
            ))
        }
    }

    /// Run the sink, consuming RecordBatches from the channel
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The channel is closed unexpectedly
    /// - Conversion fails (Arrow → EventRow)
    /// - ClickHouse insertion fails permanently after retries
    #[cfg(feature = "clickhouse")]
    pub async fn run(&self, mut rx: mpsc::Receiver<RecordBatch>) -> Result<()> {
        let mut batch_buffer: Vec<EventRow> = Vec::with_capacity(self.config.batch_size);
        let batch_timeout = Duration::from_secs(self.config.batch_timeout_secs);

        loop {
            tokio::select! {
                // Receive next batch from channel
                Some(record_batch) = rx.recv() => {
                    match self.process_batch(&record_batch) {
                        Ok(rows) => {
                            batch_buffer.extend(rows);
                            if batch_buffer.len() >= self.config.batch_size {
                                self.flush_batch(&batch_buffer).await?;
                                batch_buffer.clear();
                            }
                        }
                        Err(e) => {
                            error!(error = %e, "Failed to process batch");
                            return Err(e);
                        }
                    }
                }

                // Flush on timeout
                _ = tokio::time::sleep(batch_timeout) => {
                    if !batch_buffer.is_empty() {
                        info!(count = batch_buffer.len(), "Flushing batch due to timeout");
                        self.flush_batch(&batch_buffer).await?;
                        batch_buffer.clear();
                    }
                }
            }
        }
    }

    /// Process a single Arrow RecordBatch, converting to EventRows
    fn process_batch(&self, batch: &RecordBatch) -> Result<Vec<EventRow>> {
        let num_rows = batch.num_rows();

        // Extract columns by name
        let event_id = batch
            .column_by_name("event_id")
            .ok_or_else(|| ArrowFlightError::Conversion("Missing 'event_id' column".to_string()))?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| {
                ArrowFlightError::Conversion("'event_id' column is not StringArray".to_string())
            })?;

        let event_type = batch
            .column_by_name("event_type")
            .ok_or_else(|| ArrowFlightError::Conversion("Missing 'event_type' column".to_string()))?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| {
                ArrowFlightError::Conversion("'event_type' column is not StringArray".to_string())
            })?;

        let entity_type = batch
            .column_by_name("entity_type")
            .ok_or_else(|| {
                ArrowFlightError::Conversion("Missing 'entity_type' column".to_string())
            })?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| {
                ArrowFlightError::Conversion("'entity_type' column is not StringArray".to_string())
            })?;

        let entity_id = batch
            .column_by_name("entity_id")
            .ok_or_else(|| ArrowFlightError::Conversion("Missing 'entity_id' column".to_string()))?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| {
                ArrowFlightError::Conversion("'entity_id' column is not StringArray".to_string())
            })?;

        let timestamp = batch
            .column_by_name("timestamp")
            .ok_or_else(|| ArrowFlightError::Conversion("Missing 'timestamp' column".to_string()))?
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .ok_or_else(|| {
                ArrowFlightError::Conversion(
                    "'timestamp' column is not TimestampMicrosecondArray".to_string(),
                )
            })?;

        let data = batch
            .column_by_name("data")
            .ok_or_else(|| ArrowFlightError::Conversion("Missing 'data' column".to_string()))?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| {
                ArrowFlightError::Conversion("'data' column is not StringArray".to_string())
            })?;

        let user_id = batch
            .column_by_name("user_id")
            .ok_or_else(|| ArrowFlightError::Conversion("Missing 'user_id' column".to_string()))?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| {
                ArrowFlightError::Conversion("'user_id' column is not StringArray".to_string())
            })?;

        let org_id = batch
            .column_by_name("org_id")
            .ok_or_else(|| ArrowFlightError::Conversion("Missing 'org_id' column".to_string()))?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| {
                ArrowFlightError::Conversion("'org_id' column is not StringArray".to_string())
            })?;

        // Build event rows from columns
        let mut rows = Vec::with_capacity(num_rows);
        for i in 0..num_rows {
            let row = EventRow {
                event_id:    event_id.value(i).to_string(),
                event_type:  event_type.value(i).to_string(),
                entity_type: entity_type.value(i).to_string(),
                entity_id:   entity_id.value(i).to_string(),
                timestamp:   timestamp.value(i),
                data:        data.value(i).to_string(),
                user_id:     if user_id.is_null(i) {
                    None
                } else {
                    Some(user_id.value(i).to_string())
                },
                org_id:      if org_id.is_null(i) {
                    None
                } else {
                    Some(org_id.value(i).to_string())
                },
            };
            rows.push(row);
        }

        Ok(rows)
    }

    /// Flush a batch of rows to ClickHouse with retry logic
    #[cfg(feature = "clickhouse")]
    async fn flush_batch(&self, rows: &[EventRow]) -> Result<()> {
        let mut last_error = None;

        for attempt in 1..=self.config.max_retries {
            match self.try_insert(rows).await {
                Ok(()) => {
                    info!(count = rows.len(), attempt, "Batch inserted successfully");
                    return Ok(());
                },
                Err(e) => {
                    let error_msg = e.to_string();
                    if self.is_transient_error(&error_msg) {
                        warn!(
                            count = rows.len(),
                            attempt,
                            error = %error_msg,
                            "Transient error, retrying..."
                        );
                        last_error = Some(e);

                        if attempt < self.config.max_retries {
                            let backoff = Duration::from_millis(100 * (2_u64.pow(attempt as u32)));
                            tokio::time::sleep(backoff).await;
                        }
                    } else {
                        error!(
                            count = rows.len(),
                            error = %error_msg,
                            "Permanent error, giving up"
                        );
                        return Err(e);
                    }
                },
            }
        }

        Err(last_error.unwrap_or_else(|| {
            ArrowFlightError::External("Failed to insert batch after retries".to_string())
        }))
    }

    /// Attempt to insert rows into ClickHouse
    #[cfg(feature = "clickhouse")]
    async fn try_insert(&self, rows: &[EventRow]) -> Result<()> {
        use clickhouse::inserter::Inserter;
        let mut inserter: Inserter<EventRow> = self.client.inserter(&self.config.table);

        for row in rows {
            inserter.write(row).await.map_err(|e| {
                ArrowFlightError::External(format!("Failed to write to ClickHouse: {}", e))
            })?;
        }

        inserter.end().await.map_err(|e| {
            ArrowFlightError::External(format!("Failed to finalize ClickHouse insert: {}", e))
        })?;

        Ok(())
    }

    /// Classify whether an error is transient (retriable) or permanent
    fn is_transient_error(&self, error: &str) -> bool {
        error.contains("Connection refused")
            || error.contains("timeout")
            || error.contains("TEMPORARY_ERROR")
            || error.contains("503")
            || error.contains("Service Unavailable")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = ClickHouseSinkConfig::default();
        assert_eq!(config.batch_size, 10_000);
        assert_eq!(config.batch_timeout_secs, 5);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_config_validate_empty_url() {
        let config = ClickHouseSinkConfig {
            url: String::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_empty_database() {
        let config = ClickHouseSinkConfig {
            database: String::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_empty_table() {
        let config = ClickHouseSinkConfig {
            table: String::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_invalid_batch_size() {
        let config = ClickHouseSinkConfig {
            batch_size: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = ClickHouseSinkConfig {
            batch_size: 200_000,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_invalid_timeout() {
        let config = ClickHouseSinkConfig {
            batch_timeout_secs: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_valid() {
        let config = ClickHouseSinkConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_is_transient_error() {
        let config = ClickHouseSinkConfig::default();
        let sink = ClickHouseSink::new(config).unwrap();

        assert!(sink.is_transient_error("Connection refused"));
        assert!(sink.is_transient_error("timeout"));
        assert!(sink.is_transient_error("TEMPORARY_ERROR"));
        assert!(sink.is_transient_error("503 Service Unavailable"));
        assert!(!sink.is_transient_error("Invalid schema"));
    }
}
