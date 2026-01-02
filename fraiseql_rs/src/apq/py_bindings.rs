//! Python bindings for APQ
//!
//! Exposes APQ functionality to Python through PyO3.

use pyo3::prelude::*;
use std::sync::Arc;

use crate::apq::backends::MemoryApqStorage;
use crate::apq::ApqHandler;

/// Python wrapper for APQ handler
///
/// Provides access to APQ functionality from Python.
/// Supports memory and PostgreSQL backends.
#[pyclass]
pub struct PyApqHandler {
    handler: Arc<ApqHandler>,
}

#[pymethods]
impl PyApqHandler {
    /// Create APQ handler with in-memory LRU cache backend
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of queries to cache (default: 1000)
    ///
    /// # Returns
    ///
    /// PyApqHandler instance with memory backend
    #[staticmethod]
    fn with_memory(capacity: Option<usize>) -> Self {
        let cap = capacity.unwrap_or(1000);
        let storage = Arc::new(MemoryApqStorage::new(cap));
        let handler = Arc::new(ApqHandler::new(storage));
        Self { handler }
    }

    /// Get current metrics as JSON string
    ///
    /// # Returns
    ///
    /// JSON string with metrics (hits, misses, stored, errors, hit_rate)
    fn metrics(&self) -> String {
        self.handler.metrics().as_json().to_string()
    }
}

/// Python module for APQ bindings
///
/// Provides PyO3 module initialization for APQ functionality.
///
/// # Note
///
/// This is called automatically when importing fraiseql._fraiseql_rs
pub fn init_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyApqHandler>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_py_apq_handler_with_memory() {
        // Just test that we can create the handler
        // Full testing requires async runtime
        let _handler = PyApqHandler::with_memory(Some(500));
    }
}
