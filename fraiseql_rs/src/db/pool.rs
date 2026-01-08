//! Database connection pool (production implementation).
//!
//! # Architecture
//! - `ProductionPool` (deadpool-based) implements `PoolBackend` trait
//! - Storage layer uses `PoolBackend` abstraction, not concrete pool types
//! - Enables swapping pool implementations without changing storage code

pub mod traits;

use crate::db::{
    pool_config::{DatabaseConfig, SslMode},
    pool_production::ProductionPool,
};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_async_runtimes::tokio::future_into_py;
use std::str::FromStr;
use std::sync::Arc;

pub use traits::PoolBackend;

/// Python-facing database pool with context manager support.
#[pyclass(name = "DatabasePool")]
#[derive(Clone, Debug)]
pub struct DatabasePool {
    /// Inner production pool
    inner: Arc<ProductionPool>,
}

impl DatabasePool {
    /// Get the underlying `deadpool_postgres::Pool` for internal use.
    ///
    /// This method is for backward compatibility with RBAC and other internal components.
    #[must_use]
    pub fn get_pool(&self) -> Option<deadpool_postgres::Pool> {
        // The production pool always has a valid pool, so return Some()
        Some(self.inner.get_underlying_pool())
    }

    /// Create a new database pool from a production pool (internal use).
    ///
    /// This is used internally for creating pools from Rust code.
    #[must_use]
    pub fn new(production_pool: ProductionPool) -> Self {
        Self {
            inner: Arc::new(production_pool),
        }
    }
}

#[pymethods]
impl DatabasePool {
    /// Create a new database pool from Python.
    ///
    /// # Arguments
    ///
    /// * `database` - Database name
    /// * `host` - Database host (default: "localhost")
    /// * `port` - Database port (default: 5432)
    /// * `username` - Username (default: "postgres")
    /// * `password` - Password (optional)
    /// * `max_size` - Max pool size (default: 10)
    /// * `ssl_mode` - SSL mode: "disable", "prefer", "require" (default: "prefer")
    /// * `url` - Connection URL (alternative to individual params)
    ///
    /// # Returns
    ///
    /// A new `DatabasePool` instance
    ///
    /// # Errors
    ///
    /// Returns a Python error if pool creation fails
    ///
    /// # Examples
    ///
    /// ```python
    /// # Individual parameters
    /// pool = DatabasePool(
    ///     database="mydb",
    ///     password="secret",
    ///     max_size=20
    /// )
    ///
    /// # From URL
    /// pool = DatabasePool(url="postgresql://user:pass@localhost/mydb")
    /// ```
    #[new]
    #[pyo3(signature = (
        database=None,
        host="localhost",
        port=5432,
        username="postgres",
        password=None,
        max_size=10,
        ssl_mode="prefer",
        url=None
    ))]
    fn py_new(
        database: Option<&str>,
        host: &str,
        port: u16,
        username: &str,
        password: Option<&str>,
        max_size: usize,
        ssl_mode: &str,
        url: Option<&str>,
    ) -> PyResult<Self> {
        // Build config from URL or individual params
        let config = if let Some(url_str) = url {
            DatabaseConfig::from_url(url_str)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?
        } else {
            let database = database.ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(
                    "Either 'database' or 'url' parameter is required",
                )
            })?;

            // Parse SSL mode
            let ssl_mode = SslMode::from_str(ssl_mode)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

            // Build config
            let mut config = DatabaseConfig::new(database)
                .with_host(host)
                .with_port(port)
                .with_username(username)
                .with_max_size(max_size)
                .with_ssl_mode(ssl_mode);

            if let Some(pwd) = password {
                config = config.with_password(pwd);
            }

            config
        };

        // Create pool
        let inner = ProductionPool::new(config)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    /// Execute a SQL query asynchronously.
    ///
    /// Returns a list of JSON strings (`FraiseQL` CQRS pattern).
    ///
    /// # Arguments
    ///
    /// * `sql` - SQL query string
    ///
    /// # Returns
    ///
    /// Python coroutine resolving to list of JSON strings
    ///
    /// # Example
    ///
    /// ```python
    /// results = await pool.execute_query("SELECT data FROM tv_user LIMIT 10")
    /// print(f"Got {len(results)} results")
    /// ```
    #[pyo3(name = "execute_query")]
    fn execute_query_py<'py>(&self, py: Python<'py>, sql: String) -> PyResult<Bound<'py, PyAny>> {
        let pool = Arc::clone(&self.inner);

        future_into_py(py, async move {
            let results = pool
                .execute_query(&sql)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

            // Convert to JSON strings
            let json_strings: Result<Vec<String>, _> =
                results.iter().map(serde_json::to_string).collect();

            let json_strings =
                json_strings.map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

            Ok(json_strings)
        })
    }

    /// Perform a health check.
    ///
    /// # Returns
    ///
    /// Python coroutine resolving to boolean (`True` if healthy)
    ///
    /// # Example
    ///
    /// ```python
    /// is_healthy = await pool.health_check()
    /// if not is_healthy:
    ///     print("Pool unhealthy!")
    /// ```
    #[pyo3(name = "health_check")]
    fn health_check_py<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let pool = Arc::clone(&self.inner);

        future_into_py(py, async move {
            let result = pool
                .health_check()
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

            Ok(result.healthy)
        })
    }

    /// Get pool statistics.
    ///
    /// # Returns
    ///
    /// Dictionary with pool stats
    ///
    /// # Example
    ///
    /// ```python
    /// stats = pool.stats()
    /// print(f"Active: {stats['active']}/{stats['max_size']}")
    /// ```
    fn stats(&self, py: Python) -> PyResult<Py<PyDict>> {
        let stats = self.inner.stats();

        let dict = PyDict::new(py);
        dict.set_item("size", stats.size)?;
        dict.set_item("available", stats.available)?;
        dict.set_item("max_size", stats.max_size)?;
        dict.set_item("active", stats.size - stats.available)?;

        Ok(dict.into())
    }

    /// Begin a database transaction.
    ///
    /// Returns a transaction object that must be explicitly committed or rolled back.
    ///
    /// # Returns
    ///
    /// Python coroutine resolving to None
    ///
    /// # Example
    ///
    /// ```python
    /// await pool.begin_transaction()
    /// try:
    ///     await pool.execute_query("INSERT ...")
    ///     await pool.execute_query("UPDATE ...")
    ///     await pool.commit_transaction()
    /// except Exception:
    ///     await pool.rollback_transaction()
    ///     raise
    /// ```
    #[pyo3(name = "begin_transaction")]
    fn begin_transaction_py<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let pool = Arc::clone(&self.inner);

        future_into_py(py, async move {
            pool.execute_query("BEGIN")
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(())
        })
    }

    /// Commit the current transaction.
    ///
    /// # Returns
    ///
    /// Python coroutine resolving to None
    ///
    /// # Example
    ///
    /// ```python
    /// await pool.begin_transaction()
    /// # ... execute queries ...
    /// await pool.commit_transaction()
    /// ```
    #[pyo3(name = "commit_transaction")]
    fn commit_transaction_py<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let pool = Arc::clone(&self.inner);

        future_into_py(py, async move {
            pool.execute_query("COMMIT")
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(())
        })
    }

    /// Rollback the current transaction.
    ///
    /// # Returns
    ///
    /// Python coroutine resolving to None
    ///
    /// # Example
    ///
    /// ```python
    /// await pool.begin_transaction()
    /// try:
    ///     # ... execute queries ...
    ///     await pool.commit_transaction()
    /// except Exception:
    ///     await pool.rollback_transaction()
    ///     raise
    /// ```
    #[pyo3(name = "rollback_transaction")]
    fn rollback_transaction_py<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let pool = Arc::clone(&self.inner);

        future_into_py(py, async move {
            pool.execute_query("ROLLBACK")
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(())
        })
    }

    /// Create a savepoint within a transaction.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the savepoint
    ///
    /// # Returns
    ///
    /// Python coroutine resolving to None
    ///
    /// # Example
    ///
    /// ```python
    /// await pool.begin_transaction()
    /// await pool.savepoint("sp1")
    /// # ... execute queries ...
    /// await pool.rollback_to_savepoint("sp1")  # Rollback to savepoint
    /// await pool.commit_transaction()
    /// ```
    #[pyo3(name = "savepoint")]
    fn savepoint_py<'py>(&self, py: Python<'py>, name: String) -> PyResult<Bound<'py, PyAny>> {
        let pool = Arc::clone(&self.inner);

        future_into_py(py, async move {
            let sql = format!("SAVEPOINT {name}");
            pool.execute_query(&sql)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(())
        })
    }

    /// Rollback to a savepoint.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the savepoint
    ///
    /// # Returns
    ///
    /// Python coroutine resolving to None
    ///
    /// # Example
    ///
    /// ```python
    /// await pool.begin_transaction()
    /// await pool.savepoint("sp1")
    /// # ... execute queries ...
    /// await pool.rollback_to_savepoint("sp1")
    /// await pool.commit_transaction()
    /// ```
    #[pyo3(name = "rollback_to_savepoint")]
    fn rollback_to_savepoint_py<'py>(
        &self,
        py: Python<'py>,
        name: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let pool = Arc::clone(&self.inner);

        future_into_py(py, async move {
            let sql = format!("ROLLBACK TO SAVEPOINT {name}");
            pool.execute_query(&sql)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(())
        })
    }

    /// Execute a query with LIMIT and OFFSET for chunked/paginated results.
    ///
    /// This enables memory-efficient processing of large result sets by fetching
    /// data in chunks.
    ///
    /// # Arguments
    ///
    /// * `sql` - Base SQL query (without LIMIT/OFFSET)
    /// * `limit` - Maximum rows to fetch per chunk
    /// * `offset` - Number of rows to skip
    ///
    /// # Returns
    ///
    /// Python coroutine resolving to list of JSON strings
    ///
    /// # Example
    ///
    /// ```python
    /// # Fetch first 100 rows
    /// chunk1 = await pool.execute_query_chunked("SELECT * FROM users", 100, 0)
    ///
    /// # Fetch next 100 rows
    /// chunk2 = await pool.execute_query_chunked("SELECT * FROM users", 100, 100)
    ///
    /// # Process in chunks
    /// offset = 0
    /// chunk_size = 1000
    /// while True:
    ///     chunk = await pool.execute_query_chunked(
    ///         "SELECT * FROM large_table",
    ///         chunk_size,
    ///         offset
    ///     )
    ///     if not chunk:
    ///         break
    ///     # Process chunk...
    ///     offset += chunk_size
    /// ```
    #[pyo3(name = "execute_query_chunked")]
    fn execute_query_chunked_py<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        limit: i64,
        offset: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let pool = Arc::clone(&self.inner);

        future_into_py(py, async move {
            // Append LIMIT and OFFSET to the query
            let chunked_sql = format!(
                "{} LIMIT {} OFFSET {}",
                sql.trim_end_matches(';'),
                limit,
                offset
            );

            let results = pool
                .execute_query(&chunked_sql)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

            // Convert to JSON strings
            let json_strings: Result<Vec<String>, _> =
                results.iter().map(serde_json::to_string).collect();

            let json_strings =
                json_strings.map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

            Ok(json_strings)
        })
    }

    /// Close the pool gracefully.
    ///
    /// Waits for in-flight queries to complete.
    ///
    /// # Example
    ///
    /// ```python
    /// pool.close()
    /// ```
    fn close(&self) {
        self.inner.close();
    }

    /// Async context manager entry.
    ///
    /// # Example
    ///
    /// ```python
    /// async with DatabasePool(database="mydb") as pool:
    ///     results = await pool.execute_query("SELECT ...")
    /// # Pool automatically closed
    /// ```
    fn __aenter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let pool = self.clone();
        future_into_py(py, async move { Ok(pool) })
    }

    /// Async context manager exit.
    fn __aexit__<'py>(
        &self,
        py: Python<'py>,
        _exc_type: &Bound<'py, PyAny>,
        _exc_val: &Bound<'py, PyAny>,
        _exc_tb: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let pool = self.clone();
        future_into_py(py, async move {
            pool.inner.close();
            Ok(())
        })
    }

    /// Get pool metrics.
    ///
    /// Returns dictionary with execution metrics:
    /// - `queries_executed`: Total successful queries
    /// - `query_errors`: Total failed queries
    /// - `health_checks`: Total health checks performed
    /// - `health_check_failures`: Total failed health checks
    /// - `query_success_rate`: Success rate (0.0-1.0)
    /// - `health_check_success_rate`: Health check success rate (0.0-1.0)
    ///
    /// # Example
    ///
    /// ```python
    /// metrics = pool.metrics()
    /// print(f"Queries: {metrics['queries_executed']}")
    /// print(f"Success rate: {metrics['query_success_rate']:.2%}")
    /// ```
    fn metrics(&self, py: Python) -> PyResult<Py<PyDict>> {
        let metrics = self.inner.metrics();

        let dict = PyDict::new(py);
        dict.set_item("queries_executed", metrics.queries_executed)?;
        dict.set_item("query_errors", metrics.query_errors)?;
        dict.set_item("health_checks", metrics.health_checks)?;
        dict.set_item("health_check_failures", metrics.health_check_failures)?;
        dict.set_item("query_success_rate", metrics.query_success_rate())?;
        dict.set_item(
            "health_check_success_rate",
            metrics.health_check_success_rate(),
        )?;

        Ok(dict.into())
    }

    /// String representation.
    fn __repr__(&self) -> String {
        let stats = self.inner.stats();
        format!(
            "DatabasePool(size={}/{}, available={})",
            stats.size, stats.max_size, stats.available
        )
    }
}
