//! `ClickHouse` sink configuration for analytics events.

use std::env;

use serde::{Deserialize, Serialize};

use crate::error::{ObserverError, Result};

/// `ClickHouse` sink configuration for analytics events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickHouseConfig {
    /// `ClickHouse` HTTP endpoint (default: `http://localhost:8123`)
    #[serde(default = "default_clickhouse_url")]
    pub url: String,

    /// Database name (default: "default")
    #[serde(default = "default_clickhouse_database")]
    pub database: String,

    /// Table name (default: `fraiseql_events`)
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

fn default_clickhouse_url() -> String {
    env::var("FRAISEQL_CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".to_string())
}

fn default_clickhouse_database() -> String {
    env::var("FRAISEQL_CLICKHOUSE_DATABASE").unwrap_or_else(|_| "default".to_string())
}

fn default_clickhouse_table() -> String {
    env::var("FRAISEQL_CLICKHOUSE_TABLE").unwrap_or_else(|_| "fraiseql_events".to_string())
}

const fn default_clickhouse_batch_size() -> usize {
    10_000
}

const fn default_clickhouse_batch_timeout_secs() -> u64 {
    5
}

const fn default_clickhouse_max_retries() -> usize {
    3
}

impl Default for ClickHouseConfig {
    fn default() -> Self {
        Self {
            url: default_clickhouse_url(),
            database: default_clickhouse_database(),
            table: default_clickhouse_table(),
            batch_size: default_clickhouse_batch_size(),
            batch_timeout_secs: default_clickhouse_batch_timeout_secs(),
            max_retries: default_clickhouse_max_retries(),
        }
    }
}

impl ClickHouseConfig {
    /// Apply environment variable overrides
    #[must_use]
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(url) = env::var("FRAISEQL_CLICKHOUSE_URL") {
            self.url = url;
        }
        if let Ok(database) = env::var("FRAISEQL_CLICKHOUSE_DATABASE") {
            self.database = database;
        }
        if let Ok(table) = env::var("FRAISEQL_CLICKHOUSE_TABLE") {
            self.table = table;
        }
        if let Ok(batch_size) = env::var("FRAISEQL_CLICKHOUSE_BATCH_SIZE") {
            if let Ok(size) = batch_size.parse() {
                self.batch_size = size;
            }
        }
        if let Ok(timeout) = env::var("FRAISEQL_CLICKHOUSE_BATCH_TIMEOUT_SECS") {
            if let Ok(secs) = timeout.parse() {
                self.batch_timeout_secs = secs;
            }
        }
        if let Ok(retries) = env::var("FRAISEQL_CLICKHOUSE_MAX_RETRIES") {
            if let Ok(count) = retries.parse() {
                self.max_retries = count;
            }
        }
        self
    }

    /// Validate the configuration
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidConfig`] if `url`, `database`, or `table` is
    /// empty, `batch_size` is 0 or exceeds 100,000, or `batch_timeout_secs` is 0.
    pub fn validate(&self) -> Result<()> {
        if self.url.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "clickhouse.url cannot be empty".to_string(),
            });
        }
        if self.database.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "clickhouse.database cannot be empty".to_string(),
            });
        }
        if self.table.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "clickhouse.table cannot be empty".to_string(),
            });
        }
        if self.batch_size == 0 || self.batch_size > 100_000 {
            return Err(ObserverError::InvalidConfig {
                message: "clickhouse.batch_size must be between 1 and 100,000".to_string(),
            });
        }
        if self.batch_timeout_secs == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "clickhouse.batch_timeout_secs must be greater than 0".to_string(),
            });
        }
        Ok(())
    }
}
