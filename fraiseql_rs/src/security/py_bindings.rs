//! Python bindings for security constraints

use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_async_runtimes::tokio::future_into_py;

use super::audit::{AuditEntry, AuditLevel, AuditLogger};
use super::constraints::{ComplexityAnalyzer, IpFilter, RateLimiter};
use chrono::Utc;

/// Python wrapper for rate limiter
#[pyclass]
pub struct PyRateLimiter {
    limiter: RateLimiter,
}

#[pymethods]
impl PyRateLimiter {
    /// Create a new rate limiter
    #[new]
    fn new(max_requests: u32, window_seconds: u64) -> Self {
        Self {
            limiter: RateLimiter::new(max_requests, window_seconds),
        }
    }

    /// Check if request is allowed
    fn check<'py>(&self, py: Python<'py>, key: String) -> PyResult<Bound<'py, PyAny>> {
        let limiter = self.limiter.clone();
        future_into_py(py, async move { Ok(limiter.check(&key).await) })
    }

    /// Reset rate limit for a key
    fn reset<'py>(&self, py: Python<'py>, key: String) -> PyResult<Bound<'py, PyAny>> {
        let limiter = self.limiter.clone();
        future_into_py(py, async move {
            limiter.reset(&key).await;
            Ok(())
        })
    }
}

/// Python wrapper for IP filter
#[pyclass]
pub struct PyIpFilter {
    filter: IpFilter,
}

#[pymethods]
impl PyIpFilter {
    /// Create a new IP filter
    #[new]
    fn new(allowlist: Vec<String>, blocklist: Vec<String>) -> PyResult<Self> {
        let filter = IpFilter::new(allowlist, blocklist)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

        Ok(Self { filter })
    }

    /// Check if IP is allowed
    fn check<'py>(&self, py: Python<'py>, ip: String) -> PyResult<Bound<'py, PyAny>> {
        let filter = self.filter.clone();
        future_into_py(py, async move { Ok(filter.check(&ip).await) })
    }
}

/// Python wrapper for complexity analyzer
#[pyclass]
pub struct PyComplexityAnalyzer {
    analyzer: ComplexityAnalyzer,
}

#[pymethods]
impl PyComplexityAnalyzer {
    /// Create a new complexity analyzer
    #[new]
    fn new(max_complexity: usize) -> Self {
        Self {
            analyzer: ComplexityAnalyzer::new(max_complexity),
        }
    }

    /// Check if query complexity is acceptable
    fn check<'py>(&self, py: Python<'py>, query: String) -> PyResult<Bound<'py, PyAny>> {
        let analyzer = self.analyzer.clone();
        future_into_py(py, async move { Ok(analyzer.check(&query).await) })
    }
}

/// Python wrapper for audit logger
#[pyclass]
pub struct PyAuditLogger {
    logger: AuditLogger,
}

#[pymethods]
impl PyAuditLogger {
    /// Create a new audit logger
    #[new]
    fn new(pool: &crate::db::pool::DatabasePool) -> PyResult<Self> {
        let deadpool = pool.get_pool().ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Pool not available")
        })?;

        Ok(Self {
            logger: AuditLogger::new(std::sync::Arc::new(deadpool)),
        })
    }

    /// Log an audit entry
    #[allow(clippy::too_many_arguments)]
    fn log<'py>(
        &self,
        py: Python<'py>,
        level: String,
        user_id: i64,
        tenant_id: i64,
        operation: String,
        query: String,
        variables: String,
        ip_address: String,
        user_agent: String,
        error: Option<String>,
        duration_ms: Option<i32>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let logger = self.logger.clone();

        // Parse level
        let audit_level = AuditLevel::parse(&level);

        // Parse variables JSON
        let variables_value: serde_json::Value = serde_json::from_str(&variables).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Invalid JSON in variables: {}",
                e
            ))
        })?;

        future_into_py(py, async move {
            let entry = AuditEntry {
                id: None,
                timestamp: Utc::now(),
                level: audit_level,
                user_id,
                tenant_id,
                operation,
                query,
                variables: variables_value,
                ip_address,
                user_agent,
                error,
                duration_ms,
            };

            let id = logger.log(entry).await.map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Failed to log audit entry: {}",
                    e
                ))
            })?;

            Ok(id)
        })
    }

    /// Get recent logs for a tenant
    fn get_recent_logs<'py>(
        &self,
        py: Python<'py>,
        tenant_id: i64,
        level: Option<String>,
        limit: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let logger = self.logger.clone();
        let audit_level = level.map(|l| AuditLevel::parse(&l));

        future_into_py(py, async move {
            let entries = logger
                .get_recent_logs(tenant_id, audit_level, limit)
                .await
                .map_err(|e| {
                    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                        "Failed to get audit logs: {}",
                        e
                    ))
                })?;

            // Convert entries to Python dicts
            let result: Vec<PyObject> = entries
                .into_iter()
                .map(|entry| {
                    Python::with_gil(|py| {
                        let dict = PyDict::new(py);
                        let _ = dict.set_item("id", entry.id);
                        let _ = dict.set_item("timestamp", entry.timestamp.to_rfc3339());
                        let _ = dict.set_item("level", entry.level.as_str());
                        let _ = dict.set_item("user_id", entry.user_id);
                        let _ = dict.set_item("tenant_id", entry.tenant_id);
                        let _ = dict.set_item("operation", entry.operation);
                        let _ = dict.set_item("query", entry.query);
                        let _ = dict.set_item("variables", entry.variables.to_string());
                        let _ = dict.set_item("ip_address", entry.ip_address);
                        let _ = dict.set_item("user_agent", entry.user_agent);
                        let _ = dict.set_item("error", entry.error);
                        let _ = dict.set_item("duration_ms", entry.duration_ms);
                        dict.into()
                    })
                })
                .collect();
            Ok(result)
        })
    }
}
