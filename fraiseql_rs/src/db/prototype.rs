//! Phase 0 Prototype: `PyO3` Async Bridge Validation
//!
//! This module contains a minimal prototype to validate:
//! - `PyO3` async/await integration with Tokio
//! - GIL handling during async operations
//! - Connection pool lifecycle
//! - Error propagation across FFI boundary
//! - Cancellation handling
//!
//! **THIS IS A PROTOTYPE - NOT PRODUCTION CODE**
//!
//! Success criteria:
//! - ✅ Basic query execution works
//! - ✅ Concurrent queries don't deadlock
//! - ✅ Cancellation works correctly
//! - ✅ Errors propagate cleanly (Rust → Python)
//! - ✅ No memory leaks after 1000 queries

use deadpool_postgres::{Manager, Pool};
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio::future_into_py;
use std::sync::Arc;
use tokio_postgres::NoTls;

/// Minimal prototype pool for `PyO3` async validation
///
/// This is a stripped-down implementation focused on validating
/// the `PyO3` async bridge, not a production pool.
#[pyclass(name = "PrototypePool")]
#[derive(Debug, Clone)]
pub struct PrototypePool {
    /// Shared pool handle (Arc for cloning into async blocks)
    pool: Arc<Pool>,
}

impl PrototypePool {
    /// Create a new prototype pool
    ///
    /// # Errors
    ///
    /// Returns an error if pool creation fails
    fn new_internal(
        host: &str,
        port: u16,
        database: &str,
        username: &str,
        password: Option<&str>,
        max_connections: usize,
    ) -> Result<Self, String> {
        // Build tokio-postgres configuration
        let mut pg_config = tokio_postgres::Config::new();
        pg_config.host(host);
        pg_config.port(port);
        pg_config.dbname(database);
        pg_config.user(username);

        if let Some(pwd) = password {
            pg_config.password(pwd);
        }

        // Application name for PostgreSQL logging
        pg_config.application_name("fraiseql_prototype");

        // Connection timeout (10 seconds)
        pg_config.connect_timeout(std::time::Duration::from_secs(10));

        // Create deadpool manager
        let mgr = Manager::new(pg_config, NoTls);

        // Create pool with size limit
        let pool = Pool::builder(mgr)
            .max_size(max_connections)
            .build()
            .map_err(|e| format!("Failed to create pool: {e}"))?;

        Ok(Self {
            pool: Arc::new(pool),
        })
    }
}

#[pymethods]
impl PrototypePool {
    /// Create a new prototype pool from Python
    ///
    /// # Arguments
    ///
    /// * `host` - Database host (default: "localhost")
    /// * `port` - Database port (default: 5432)
    /// * `database` - Database name
    /// * `username` - Database username (default: "postgres")
    /// * `password` - Optional database password
    /// * `max_connections` - Maximum pool size (default: 10)
    ///
    /// # Returns
    ///
    /// A new `PrototypePool` instance
    ///
    /// # Errors
    ///
    /// Returns a Python `RuntimeError` if:
    /// - Pool configuration is invalid
    /// - Cannot create pool
    #[new]
    #[pyo3(signature = (database, host="localhost", port=5432, username="postgres", password=None, max_connections=10))]
    fn py_new(
        database: &str,
        host: &str,
        port: u16,
        username: &str,
        password: Option<&str>,
        max_connections: usize,
    ) -> PyResult<Self> {
        Self::new_internal(host, port, database, username, password, max_connections)
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)
    }

    /// Execute a SQL query asynchronously
    ///
    /// This is the core function being tested - it bridges async Rust to Python coroutines.
    ///
    /// # Test Coverage
    ///
    /// - Connection acquisition from pool
    /// - Query execution
    /// - Result conversion to JSON
    /// - GIL handling (no deadlocks)
    /// - Error propagation
    ///
    /// # Arguments
    ///
    /// * `sql` - SQL query string
    ///
    /// # Returns
    ///
    /// Python coroutine that resolves to a list of JSON objects
    ///
    /// # Errors
    ///
    /// Returns a Python error if:
    /// - Cannot acquire connection from pool
    /// - Query execution fails
    /// - Result conversion fails
    #[pyo3(name = "execute_query")]
    fn execute_query_py<'py>(&self, py: Python<'py>, sql: String) -> PyResult<Bound<'py, PyAny>> {
        // Helper function to extract column value as JSON
        #[allow(clippy::excessive_nesting, clippy::option_if_let_else)]
        fn row_column_to_json(row: &tokio_postgres::Row, idx: usize) -> serde_json::Value {
            // Extract value as JSON (basic types only for prototype)
            if let Ok(v) = row.try_get::<_, i32>(idx) {
                serde_json::json!(v)
            } else if let Ok(v) = row.try_get::<_, i64>(idx) {
                serde_json::json!(v)
            } else if let Ok(v) = row.try_get::<_, String>(idx) {
                serde_json::json!(v)
            } else if let Ok(v) = row.try_get::<_, bool>(idx) {
                serde_json::json!(v)
            } else {
                serde_json::Value::Null
            }
        }

        // Clone Arc for move into async block
        let pool = Arc::clone(&self.pool);

        // Bridge async Rust to Python coroutine
        future_into_py(py, async move {
            // Phase 1: Acquire connection (test pool acquisition)
            let client = pool.get().await.map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to get connection: {e}"))
            })?;

            // Phase 2: Execute query (test query execution)
            let rows = client.query(&sql, &[]).await.map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Query failed: {e}"))
            })?;

            // Phase 3: Convert rows to JSON
            // For FraiseQL production: data is JSONB in column 0
            // For prototype testing: handle any column types
            #[allow(clippy::option_if_let_else)]
            let results: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    // Try JSONB first (FraiseQL production pattern)
                    if let Ok(json_val) = row.try_get::<_, serde_json::Value>(0) {
                        json_val
                    } else {
                        // Fallback: Build JSON object from all columns
                        let mut map = serde_json::Map::new();
                        #[allow(clippy::excessive_nesting)]
                        for (idx, column) in row.columns().iter().enumerate() {
                            let key = column.name().to_string();
                            let value = row_column_to_json(row, idx);
                            map.insert(key, value);
                        }
                        serde_json::Value::Object(map)
                    }
                })
                .collect();

            // Phase 4: Convert to Python list (test FFI boundary)
            // Convert results to strings first (outside with_gil)
            let json_strings: Result<Vec<String>, _> = results
                .iter()
                .map(|json_val| {
                    serde_json::to_string(json_val).map_err(|e| {
                        pyo3::exceptions::PyValueError::new_err(format!(
                            "JSON serialization failed: {e}"
                        ))
                    })
                })
                .collect();

            let json_strings = json_strings?;

            // Return as Vec<String> - PyO3 will handle conversion
            Ok(json_strings)
        })
    }

    /// Get pool statistics
    ///
    /// # Returns
    ///
    /// String with pool statistics (connections, idle)
    #[pyo3(name = "stats")]
    fn stats_py(&self) -> String {
        let status = self.pool.status();
        format!(
            "Pool stats: {} total, {} available",
            status.size, status.available
        )
    }

    /// Health check - verify pool can acquire connection
    ///
    /// # Returns
    ///
    /// Python coroutine that resolves to `True` if healthy
    ///
    /// # Errors
    ///
    /// Returns a Python error if health check fails
    #[pyo3(name = "health_check")]
    fn health_check_py<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let pool = Arc::clone(&self.pool);

        future_into_py(py, async move {
            let client = pool.get().await.map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Health check failed: {e}"))
            })?;

            client.simple_query("SELECT 1").await.map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Health check query failed: {e}"))
            })?;

            Ok(true)
        })
    }

    /// Python string representation
    fn __repr__(&self) -> String {
        let status = self.pool.status();
        format!(
            "PrototypePool(size={}, available={})",
            status.size, status.available
        )
    }
}

// Note: FraiseQL uses JSONB CQRS pattern - all data is already JSONB in PostgreSQL
// No complex row-to-JSON conversion needed!
