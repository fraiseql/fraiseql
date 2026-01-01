//! Python bindings for RBAC components.

use super::resolver::PermissionResolver;
use pyo3::prelude::*;
use std::sync::Arc;
use uuid::Uuid;

/// Python wrapper for `PermissionResolver`
#[derive(Debug)]
#[pyclass]
pub struct PyPermissionResolver {
    resolver: Arc<PermissionResolver>,
}

#[pymethods]
impl PyPermissionResolver {
    /// # Errors
    ///
    /// Returns a Python error if the database pool is not initialized.
    #[new]
    #[allow(clippy::needless_pass_by_value)] // PyO3 requires Py<T> to be passed by value
    pub fn new(pool: Py<crate::db::pool::DatabasePool>, cache_capacity: usize) -> PyResult<Self> {
        Python::with_gil(|py| {
            let db_pool = pool.borrow(py);
            let rust_pool = db_pool
                .get_pool()
                .ok_or_else(|| {
                    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                        "Database pool not initialized",
                    )
                })?
                .clone();
            let resolver = PermissionResolver::new(rust_pool, cache_capacity);
            Ok(Self {
                resolver: Arc::new(resolver),
            })
        })
    }

    /// Get user permissions (placeholder - full async implementation needed)
    ///
    /// # Errors
    ///
    /// Currently never returns an error (placeholder implementation).
    pub fn get_user_permissions(
        &self,
        _user_id: String,
        _tenant_id: Option<String>,
    ) -> PyResult<String> {
        // TODO: Implement full async Python binding
        Ok("get_user_permissions not yet implemented".to_string())
    }

    /// Check specific permission (placeholder)
    ///
    /// # Errors
    ///
    /// Currently never returns an error (placeholder implementation).
    pub fn has_permission(
        &self,
        _user_id: String,
        _resource: String,
        _action: String,
        _tenant_id: Option<String>,
    ) -> PyResult<String> {
        // TODO: Implement full async Python binding
        Ok("has_permission not yet implemented".to_string())
    }

    /// Invalidate user cache
    ///
    /// # Errors
    ///
    /// Returns a Python error if the user ID is not a valid UUID.
    pub fn invalidate_user(&self, user_id: &str) -> PyResult<()> {
        let user_uuid = Uuid::parse_str(user_id)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

        self.resolver.invalidate_user(user_uuid);
        Ok(())
    }

    /// Clear entire cache
    pub fn clear_cache(&self) {
        self.resolver.clear_cache();
    }

    /// Get cache statistics
    ///
    /// # Errors
    ///
    /// Currently never returns an error, but the Result type allows for future validation.
    pub fn cache_stats(&self) -> PyResult<String> {
        let stats = self.resolver.cache_stats();
        Ok(format!(
            "Cache stats: capacity={}, size={}, expired={}",
            stats.capacity, stats.size, stats.expired_count
        ))
    }
}

/// Python wrapper for `FieldAuthChecker`
#[derive(Debug)]
#[pyclass]
pub struct PyFieldAuthChecker {
    #[allow(dead_code)]
    checker: super::field_auth::FieldAuthChecker,
}

#[pymethods]
impl PyFieldAuthChecker {
    /// # Errors
    ///
    /// Currently never returns an error, but the Result type allows for future validation.
    #[new]
    pub fn new(resolver: &PyPermissionResolver) -> PyResult<Self> {
        let checker = super::field_auth::FieldAuthChecker::new(Arc::clone(&resolver.resolver));
        Ok(Self { checker })
    }

    /// Check field access (placeholder)
    ///
    /// # Errors
    ///
    /// Currently never returns an error (placeholder implementation).
    pub fn check_field_access(
        &self,
        _user_id: Option<String>,
        _roles: Vec<String>,
        _field_name: String,
        _field_permissions: PyObject,
        _tenant_id: Option<String>,
    ) -> PyResult<String> {
        // TODO: Implement full async Python binding
        Ok("check_field_access not yet implemented".to_string())
    }

    /// Get cache statistics from the associated resolver
    ///
    /// # Errors
    ///
    /// Currently never returns an error (placeholder implementation).
    pub fn get_resolver_stats(&self) -> PyResult<String> {
        Ok("FieldAuthChecker stats not yet implemented".to_string())
    }
}
