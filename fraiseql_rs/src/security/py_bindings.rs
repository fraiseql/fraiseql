//! Python bindings for security constraints

use pyo3::prelude::*;
use pyo3_async_runtimes::tokio::future_into_py;

use super::constraints::{ComplexityAnalyzer, IpFilter, RateLimiter};

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
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e))?;

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
