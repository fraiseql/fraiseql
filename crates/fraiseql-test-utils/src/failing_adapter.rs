#![allow(clippy::unwrap_used)] // Reason: test/bench code, panics are acceptable
//! Failure-injecting database adapter for error path testing.
//!
//! Provides a configurable `DatabaseAdapter` implementation that can simulate
//! database failures, timeouts, and connection pool errors on demand.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use async_trait::async_trait;
#[cfg(feature = "grpc")]
use fraiseql_core::db::types::{ColumnSpec, ColumnValue};
use fraiseql_core::{
    db::{
        CursorValue, DatabaseAdapter, DatabaseType, MutationCapable, RelayDatabaseAdapter,
        WhereClause,
        traits::RelayPageResult,
        types::{JsonbValue, PoolMetrics},
    },
    error::{FraiseQLError, Result},
    schema::SqlProjectionHint,
};

/// Configuration for failure injection.
///
/// Controls when and how the `FailingAdapter` produces errors.
#[derive(Debug, Clone, Default)]
pub struct FailConfig {
    /// Fail on the Nth query (0-indexed).
    pub fail_on_query:     Option<u64>,
    /// Return a `Timeout` error with this duration in milliseconds.
    pub timeout_ms:        Option<u64>,
    /// Return this specific error on failure.
    pub error:             Option<FailError>,
    /// Make `health_check` return an error.
    pub fail_health_check: bool,
}

/// Serializable error specification for failure injection.
///
/// We can't clone `FraiseQLError` directly, so we use this enum
/// to specify which error variant to produce.
#[derive(Debug, Clone)]
pub enum FailError {
    /// Database error with message and optional SQL state.
    Database {
        /// Error message.
        message:   String,
        /// SQL state code.
        sql_state: Option<String>,
    },
    /// Connection pool error.
    ConnectionPool {
        /// Error message.
        message: String,
    },
    /// Timeout error.
    Timeout {
        /// Timeout duration in milliseconds.
        timeout_ms: u64,
    },
    /// Cancelled error.
    Cancelled {
        /// Query identifier.
        query_id: String,
        /// Reason for cancellation.
        reason:   String,
    },
    /// Internal error.
    Internal {
        /// Error message.
        message: String,
    },
}

impl FailError {
    fn into_error(self) -> FraiseQLError {
        match self {
            Self::Database { message, sql_state } => FraiseQLError::Database { message, sql_state },
            Self::ConnectionPool { message } => FraiseQLError::ConnectionPool { message },
            Self::Timeout { timeout_ms } => FraiseQLError::Timeout {
                timeout_ms,
                query: None,
            },
            Self::Cancelled { query_id, reason } => FraiseQLError::Cancelled { query_id, reason },
            Self::Internal { message } => FraiseQLError::internal(message),
        }
    }
}

/// A `DatabaseAdapter` that can be configured to fail on demand.
///
/// Supports canned responses per view and configurable failure injection
/// for testing error paths.
///
/// All fields are `Arc`-wrapped, so cloning is cheap and shares state.
#[derive(Clone)]
pub struct FailingAdapter {
    /// Canned responses per view name.
    responses:          Arc<Mutex<HashMap<String, Vec<JsonbValue>>>>,
    /// Canned function call responses per function name.
    function_responses: Arc<Mutex<HashMap<String, Vec<HashMap<String, serde_json::Value>>>>>,
    /// Canned row-shaped responses per view name (for gRPC `execute_row_query`).
    #[cfg(feature = "grpc")]
    row_responses:      Arc<Mutex<HashMap<String, Vec<Vec<ColumnValue>>>>>,
    /// Log of WHERE clauses passed to `execute_row_query` (for RLS assertion).
    #[cfg(feature = "grpc")]
    where_clause_log:   Arc<Mutex<Vec<Option<String>>>>,
    /// Failure injection configuration.
    fail_config:        Arc<Mutex<FailConfig>>,
    /// Query counter (increments on every query attempt).
    query_count:        Arc<AtomicU64>,
    /// Log of all query view names for assertion.
    query_log:          Arc<Mutex<Vec<String>>>,
}

impl FailingAdapter {
    /// Create a new `FailingAdapter` with no canned responses and no failures.
    #[must_use]
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
            function_responses: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(feature = "grpc")]
            row_responses: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(feature = "grpc")]
            where_clause_log: Arc::new(Mutex::new(Vec::new())),
            fail_config: Arc::new(Mutex::new(FailConfig::default())),
            query_count: Arc::new(AtomicU64::new(0)),
            query_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Set a canned response for a specific view.
    ///
    /// # Panics
    ///
    /// Panics if the internal responses mutex is poisoned.
    #[must_use]
    pub fn with_response(self, view: &str, data: Vec<JsonbValue>) -> Self {
        self.responses.lock().unwrap().insert(view.to_string(), data);
        self
    }

    /// Set a canned response for a specific database function.
    ///
    /// # Panics
    ///
    /// Panics if the internal function responses mutex is poisoned.
    #[must_use]
    pub fn with_function_response(
        self,
        function_name: &str,
        data: Vec<HashMap<String, serde_json::Value>>,
    ) -> Self {
        self.function_responses.lock().unwrap().insert(function_name.to_string(), data);
        self
    }

    /// Set a canned row-shaped response for a specific view (for gRPC transport).
    ///
    /// # Panics
    ///
    /// Panics if the internal row responses mutex is poisoned.
    #[cfg(feature = "grpc")]
    #[must_use]
    pub fn with_row_response(self, view: &str, data: Vec<Vec<ColumnValue>>) -> Self {
        self.row_responses.lock().unwrap().insert(view.to_string(), data);
        self
    }

    /// Configure failure on the Nth query (0-indexed).
    ///
    /// # Panics
    ///
    /// Panics if the internal config mutex is poisoned.
    #[must_use]
    pub fn fail_on_query(self, n: u64) -> Self {
        self.fail_config.lock().unwrap().fail_on_query = Some(n);
        self
    }

    /// Configure timeout error response.
    ///
    /// # Panics
    ///
    /// Panics if the internal config mutex is poisoned.
    #[must_use]
    pub fn fail_with_timeout(self, ms: u64) -> Self {
        self.fail_config.lock().unwrap().timeout_ms = Some(ms);
        self
    }

    /// Configure a specific error to return on failure.
    ///
    /// # Panics
    ///
    /// Panics if the internal config mutex is poisoned.
    #[must_use]
    pub fn fail_with_error(self, err: FailError) -> Self {
        self.fail_config.lock().unwrap().error = Some(err);
        self
    }

    /// Configure health check to fail.
    ///
    /// # Panics
    ///
    /// Panics if the internal config mutex is poisoned.
    #[must_use]
    pub fn fail_health_check(self) -> Self {
        self.fail_config.lock().unwrap().fail_health_check = true;
        self
    }

    /// Reset failure config and query count.
    ///
    /// # Panics
    ///
    /// Panics if any internal mutex is poisoned.
    pub fn reset(&self) {
        *self.fail_config.lock().unwrap() = FailConfig::default();
        self.query_count.store(0, Ordering::SeqCst);
        self.query_log.lock().unwrap().clear();
        self.function_responses.lock().unwrap().clear();
    }

    /// Get all recorded query view names.
    ///
    /// # Panics
    ///
    /// Panics if the internal query log mutex is poisoned.
    #[must_use]
    pub fn recorded_queries(&self) -> Vec<String> {
        self.query_log.lock().unwrap().clone()
    }

    /// Get all recorded WHERE clauses from `execute_row_query` calls.
    ///
    /// Each entry is `Some(sql)` when a WHERE clause was passed, or `None`
    /// when the query was executed without filtering.
    ///
    /// # Panics
    ///
    /// Panics if the internal where clause log mutex is poisoned.
    #[cfg(feature = "grpc")]
    #[must_use]
    pub fn recorded_where_clauses(&self) -> Vec<Option<String>> {
        self.where_clause_log.lock().unwrap().clone()
    }

    /// Get the current query count.
    #[must_use]
    pub fn query_count(&self) -> u64 {
        self.query_count.load(Ordering::SeqCst)
    }

    /// Re-configure failure on the Nth query after construction.
    ///
    /// # Panics
    ///
    /// Panics if the internal config mutex is poisoned.
    pub fn set_fail_on_query(&self, n: u64) {
        self.fail_config.lock().unwrap().fail_on_query = Some(n);
    }

    /// Re-configure timeout error after construction.
    ///
    /// # Panics
    ///
    /// Panics if the internal config mutex is poisoned.
    pub fn set_fail_with_timeout(&self, ms: u64) {
        self.fail_config.lock().unwrap().timeout_ms = Some(ms);
    }

    /// Re-configure a specific error after construction.
    ///
    /// # Panics
    ///
    /// Panics if the internal config mutex is poisoned.
    pub fn set_fail_with_error(&self, err: FailError) {
        self.fail_config.lock().unwrap().error = Some(err);
    }

    /// Check if the current query should fail, returning the error if so.
    fn check_failure(&self, view: &str) -> Result<()> {
        let current = self.query_count.fetch_add(1, Ordering::SeqCst);
        self.query_log.lock().unwrap().push(view.to_string());

        let config = self.fail_config.lock().unwrap();

        // Check fail_on_query
        if let Some(n) = config.fail_on_query {
            if current == n {
                // Determine which error to return
                if let Some(ref err) = config.error {
                    return Err(err.clone().into_error());
                }
                if let Some(ms) = config.timeout_ms {
                    return Err(FraiseQLError::Timeout {
                        timeout_ms: ms,
                        query:      Some(view.to_string()),
                    });
                }
                return Err(FraiseQLError::Database {
                    message:   format!("injected failure on query {current}"),
                    sql_state: None,
                });
            }
            return Ok(());
        }

        // Global failure modes (apply to every query)
        if let Some(ref err) = config.error {
            return Err(err.clone().into_error());
        }
        if let Some(ms) = config.timeout_ms {
            return Err(FraiseQLError::Timeout {
                timeout_ms: ms,
                query:      Some(view.to_string()),
            });
        }

        Ok(())
    }

    /// Get canned response for a view, or empty vec.
    fn get_response(&self, view: &str) -> Vec<JsonbValue> {
        self.responses.lock().unwrap().get(view).cloned().unwrap_or_default()
    }
}

impl Default for FailingAdapter {
    fn default() -> Self {
        Self::new()
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DatabaseAdapter for FailingAdapter {
    async fn execute_where_query(
        &self,
        view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        self.check_failure(view)?;
        Ok(self.get_response(view))
    }

    async fn execute_with_projection(
        &self,
        view: &str,
        _projection: Option<&SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        self.check_failure(view)?;
        Ok(self.get_response(view))
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> Result<()> {
        if self.fail_config.lock().unwrap().fail_health_check {
            return Err(FraiseQLError::Database {
                message:   "health check failed (injected)".to_string(),
                sql_state: None,
            });
        }
        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            total_connections:  10,
            idle_connections:   5,
            active_connections: 3,
            waiting_requests:   0,
        }
    }

    async fn execute_raw_query(
        &self,
        sql: &str,
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        self.check_failure(sql)?;
        Ok(vec![])
    }

    async fn execute_parameterized_aggregate(
        &self,
        sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        self.check_failure(sql)?;
        Ok(vec![])
    }

    async fn execute_function_call(
        &self,
        function_name: &str,
        _args: &[serde_json::Value],
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        self.check_failure(function_name)?;
        let responses = self.function_responses.lock().unwrap();
        Ok(responses.get(function_name).cloned().unwrap_or_default())
    }

    #[cfg(feature = "grpc")]
    async fn execute_row_query(
        &self,
        view: &str,
        _columns: &[ColumnSpec],
        where_clause: Option<&str>,
        _order_by: Option<&str>,
        _limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<Vec<Vec<ColumnValue>>> {
        self.check_failure(view)?;
        self.where_clause_log.lock().unwrap().push(where_clause.map(String::from));
        let responses = self.row_responses.lock().unwrap();
        Ok(responses.get(view).cloned().unwrap_or_default())
    }
}

impl MutationCapable for FailingAdapter {}

impl RelayDatabaseAdapter for FailingAdapter {
    async fn execute_relay_page<'a>(
        &'a self,
        view: &'a str,
        _cursor_column: &'a str,
        _after: Option<CursorValue>,
        _before: Option<CursorValue>,
        limit: u32,
        _forward: bool,
        _where_clause: Option<&'a WhereClause>,
        _order_by: Option<&'a [fraiseql_core::db::types::sql_hints::OrderByClause]>,
        _include_total_count: bool,
    ) -> Result<RelayPageResult> {
        self.check_failure(view)?;
        let all_rows = self.get_response(view);
        let rows: Vec<JsonbValue> = all_rows.into_iter().take(limit as usize).collect();
        Ok(RelayPageResult {
            rows,
            total_count: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_default_returns_empty() {
        let adapter = FailingAdapter::new();
        let result = adapter.execute_where_query("v_user", None, None, None).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_canned_response() {
        let adapter = FailingAdapter::new()
            .with_response("v_user", vec![JsonbValue::new(serde_json::json!({"id": 1}))]);
        let result = adapter.execute_where_query("v_user", None, None, None).await.unwrap();
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn test_fail_on_query_zero() {
        let adapter = FailingAdapter::new().fail_on_query(0);
        let result = adapter.execute_where_query("v_user", None, None, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_query_count_and_log() {
        let adapter = FailingAdapter::new();
        let _ = adapter.execute_where_query("v_user", None, None, None).await;
        let _ = adapter.execute_where_query("v_post", None, None, None).await;
        assert_eq!(adapter.query_count(), 2);
        assert_eq!(adapter.recorded_queries(), vec!["v_user", "v_post"]);
    }

    #[tokio::test]
    async fn test_reset() {
        let adapter = FailingAdapter::new().fail_on_query(0);
        assert!(adapter.execute_where_query("v_user", None, None, None).await.is_err());
        adapter.reset();
        assert!(adapter.execute_where_query("v_user", None, None, None).await.is_ok());
        assert_eq!(adapter.query_count(), 1);
    }
}
