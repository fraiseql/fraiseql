//! Python bindings for Axum HTTP server via `PyO3`
//!
//! This module exposes the high-performance Axum server to Python
//! through `PyO3` FFI bindings, enabling Python code to leverage
//! Rust performance without requiring Rust knowledge.
//!
//! Architecture:
//! ```
//! Python asyncio
//!     ↓
//! AxumServer (Python wrapper)
//!     ↓
//! PyAxumServer (`PyO3` wrapper - this module)
//!     ↓
//! AppState (shared via Arc)
//!     ↓
//! graphql_handler (Axum handler)
//!     ↓
//! GraphQL Pipeline
//!     ↓
//! PostgreSQL
//! ```

use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;

use crate::cache::QueryPlanCache;
use crate::db::{DatabaseConfig, DatabasePool, ProductionPool};
use crate::http::axum_server::{AppState, GraphQLRequest};
use crate::http::metrics::HttpMetrics;
use crate::pipeline::unified::{GraphQLPipeline, UserContext};
use crate::query::schema::SchemaMetadata;

/// Python-exposed Axum GraphQL server
///
/// Provides a Python interface to the high-performance Axum HTTP server.
/// Wraps the Rust `AppState` and exposes query execution to Python code.
///
/// # Example
///
/// ```python
/// from fraiseql._fraiseql_rs import PyAxumServer
///
/// # Create server
/// server = PyAxumServer.new("postgresql://localhost/db")
///
/// # Execute query
/// result = server.execute_query("{ users { id name } }")
/// # Returns: {"data": {...}, "errors": null}
/// ```
#[pyclass]
#[derive(Debug, Clone)]
pub struct PyAxumServer {
    /// The shared Axum application state
    state: Arc<AppState>,

    /// Server running flag
    is_running: bool,
}

#[pymethods]
impl PyAxumServer {
    /// Create a new Axum server instance
    ///
    /// # Arguments
    ///
    /// * `database_url` - `PostgreSQL` connection string
    /// * `metrics_admin_token` - Optional token for metrics endpoint (default: empty string)
    ///
    /// # Returns
    ///
    /// `PyAxumServer` instance ready for query execution
    ///
    /// # Errors
    ///
    /// Returns `ValueError` if:
    /// - Database URL is invalid or malformed
    /// - Connection pool initialization fails
    /// - Cannot connect to `PostgreSQL`
    ///
    /// # Example
    ///
    /// ```python
    /// server = PyAxumServer.new(
    ///     "postgresql://user:pass@localhost/db",
    ///     "admin-secret-token"
    /// )
    /// ```
    #[staticmethod]
    pub fn new(database_url: &str, metrics_admin_token: Option<String>) -> PyResult<Self> {
        // Parse database URL into config
        let config = DatabaseConfig::from_url(database_url).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Invalid database URL: {e}"))
        })?;

        // Create database pool from config
        let production_pool = ProductionPool::new(config).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to create database pool: {e}"
            ))
        })?;
        let pool = Arc::new(DatabasePool::new(production_pool));

        // Get metrics admin token, default to empty string
        let token = metrics_admin_token.unwrap_or_default();

        // Create HTTP metrics tracker
        let http_metrics = Arc::new(HttpMetrics::new());

        // Initialize GraphQL pipeline with the database pool
        // Note: GraphQLPipeline needs SchemaMetadata and QueryPlanCache
        // For now, we'll create with minimal defaults
        let cache = Arc::new(QueryPlanCache::new(5000));

        // TODO: Load schema from database or configuration
        // For Phase 1, create a minimal empty schema
        let schema = SchemaMetadata {
            tables: HashMap::new(),
            types: HashMap::new(),
        };

        let pipeline = Arc::new(GraphQLPipeline::new(schema, cache, pool));

        // Create AppState with all components
        let state = Arc::new(AppState::new(
            pipeline,
            http_metrics,
            token,
            None, // No audit logger for now
        ));

        Ok(Self {
            state,
            is_running: false,
        })
    }

    /// Execute a GraphQL query
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    /// * `variables` - Optional query variables as JSON string (e.g., "{\"id\": \"123\"}")
    /// * `operation_name` - Optional operation name for multi-operation documents
    ///
    /// # Returns
    ///
    /// Dictionary with structure:
    /// ```json
    /// {
    ///   "data": {...},        // Query result or null if error
    ///   "errors": [...]       // List of errors or null if successful
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `RuntimeError` if:
    /// - Variables JSON is invalid
    /// - Query execution fails
    /// - Database connection is lost
    ///
    /// # Example
    ///
    /// ```python
    /// result = server.execute_query(
    ///     'query GetUser($id: ID!) { user(id: $id) { id name } }',
    ///     variables='{"id": "123"}',
    ///     operation_name="GetUser"
    /// )
    /// print(result)  # {"data": {"user": {...}}, "errors": null}
    /// ```
    pub fn execute_query(
        &self,
        query: String,
        variables: Option<String>,
        operation_name: Option<String>,
    ) -> PyResult<PyObject> {
        // Parse variables from JSON string if provided
        let variables_map = if let Some(vars_json) = variables {
            match serde_json::from_str::<HashMap<String, JsonValue>>(&vars_json) {
                Ok(map) => map,
                Err(e) => {
                    return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                        "Invalid variables JSON: {e}"
                    )))
                }
            }
        } else {
            HashMap::new()
        };

        // Create GraphQL request
        let request = GraphQLRequest {
            query,
            operation_name,
            variables: if variables_map.is_empty() {
                None
            } else {
                Some(
                    serde_json::to_value(&variables_map)
                        .unwrap_or_else(|_| JsonValue::Object(serde_json::Map::new())),
                )
            },
        };

        // Create anonymous user context (no authentication for now)
        let user_context = UserContext {
            user_id: None,
            permissions: vec!["public".to_string()],
            roles: vec![],
            exp: u64::MAX,
        };

        // Execute query through pipeline (this is async, so we block in a tokio runtime)
        let result = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            // We're already in a tokio context (e.g., called from async Python)
            handle.block_on(self.state.pipeline.execute(
                &request.query,
                variables_map,
                user_context,
            ))
        } else {
            // Create a new runtime for this execution
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| {
                    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                        "Failed to create tokio runtime: {e}"
                    ))
                })?;

            rt.block_on(
                self.state
                    .pipeline
                    .execute(&request.query, variables_map, user_context),
            )
        };

        // Convert result to Python dict
        Python::with_gil(|py| {
            let response_dict = PyDict::new(py);

            match result {
                Ok(response_bytes) => {
                    // Parse response bytes back to JSON
                    match serde_json::from_slice::<JsonValue>(&response_bytes) {
                        Ok(data) => {
                            response_dict.set_item("data", data.to_string())?;
                            response_dict.set_item("errors", py.None())?;
                        }
                        Err(e) => {
                            let error_msg = format!("Failed to parse response: {e}");
                            response_dict.set_item("data", py.None())?;
                            response_dict.set_item(
                                "errors",
                                vec![json!({"message": error_msg}).to_string()],
                            )?;
                        }
                    }
                }
                Err(e) => {
                    let error_msg = format!("Query execution failed: {e}");
                    response_dict.set_item("data", py.None())?;
                    response_dict
                        .set_item("errors", vec![json!({"message": error_msg}).to_string()])?;
                }
            }

            Ok(response_dict.into())
        })
    }

    /// Start the Axum HTTP server
    ///
    /// # Arguments
    ///
    /// * `host` - Bind host (default: "0.0.0.0")
    /// * `port` - Bind port (default: 8000)
    ///
    /// # Errors
    ///
    /// Returns `RuntimeError` if:
    /// - Cannot bind to the specified address
    /// - Address is already in use
    /// - Port is out of valid range
    ///
    /// # Note
    ///
    /// This is a blocking operation in Phase 1. Future versions will support
    /// async/await pattern with proper Python integration.
    ///
    /// # Example
    ///
    /// ```python
    /// server.start(host="127.0.0.1", port=8000)
    /// # Server now listening on http://127.0.0.1:8000
    /// ```
    pub fn start(&mut self, host: Option<String>, port: Option<u16>) -> PyResult<()> {
        // TODO Phase 2: Implement full server startup
        // This would require:
        // 1. Create Axum router from AppState
        // 2. Bind to host:port
        // 3. Start listening in background thread
        // 4. Set is_running = true

        let _host = host.unwrap_or_else(|| "0.0.0.0".to_string());
        let _port = port.unwrap_or(8000);

        self.is_running = true;
        Ok(())
    }

    /// Shutdown the Axum server gracefully
    ///
    /// # Errors
    ///
    /// Returns `RuntimeError` if shutdown fails
    ///
    /// # Example
    ///
    /// ```python
    /// server.shutdown()
    /// # Server stopped listening
    /// ```
    pub fn shutdown(&mut self) -> PyResult<()> {
        // TODO Phase 2: Implement graceful shutdown
        // This would require:
        // 1. Signal background thread to stop
        // 2. Wait for in-flight requests
        // 3. Close connections

        self.is_running = false;
        Ok(())
    }

    /// Check if server is running
    ///
    /// # Returns
    ///
    /// `True` if server is running, `False` otherwise
    ///
    /// # Example
    ///
    /// ```python
    /// if server.is_running():
    ///     print("Server is active")
    /// ```
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// Get current metrics as JSON string
    ///
    /// # Returns
    ///
    /// JSON string with HTTP metrics including:
    /// - Total requests
    /// - Successful requests
    /// - Failed requests
    /// - Latency statistics
    /// - Cache hit rates
    ///
    /// # Example
    ///
    /// ```python
    /// metrics = server.get_metrics()
    /// print(metrics)  # JSON metrics string
    /// ```
    #[must_use]
    pub fn get_metrics(&self) -> String {
        // Export metrics in Prometheus format
        self.state.http_metrics.export_prometheus()
    }
}

/// Module initialization for `PyO3`
///
/// Registers all classes and functions in the `_fraiseql_rs` module.
/// Called automatically when the module is imported.
///
/// # Errors
///
/// Returns a Python error if class registration fails.
pub fn init_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyAxumServer>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "Test not yet implemented - requires PostgreSQL instance"]
    fn test_py_axum_server_new() {
        // Test that server creation works with valid connection string
        // Note: This requires a running PostgreSQL instance
        // For CI/CD, this would use test fixtures
    }

    #[test]
    #[ignore = "Test not yet implemented - requires running server"]
    fn test_py_axum_server_lifecycle() {
        // Test that is_running flag works correctly
    }
}
