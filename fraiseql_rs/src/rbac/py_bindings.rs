//! Python bindings for RBAC components.
//!
//! Provides Python wrappers for:
//! - PermissionResolver: Field-level authorization checking
//! - FieldAuthChecker: Per-field permission validation
//! - RowConstraintResolver: Row-level access filtering
//! - WhereMerger: Safe WHERE clause composition

use super::resolver::PermissionResolver;
use super::row_constraints::RowConstraintResolver;
use super::where_merger::WhereMerger;
use pyo3::prelude::*;
use serde_json::Value;
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
            let rust_pool = db_pool.get_pool().ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Database pool not initialized")
            })?;
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

    /// Invalidate tenant cache
    ///
    /// # Errors
    ///
    /// Returns a Python error if the tenant ID is not a valid UUID.
    pub fn invalidate_tenant(&self, tenant_id: &str) -> PyResult<()> {
        let tenant_uuid = Uuid::parse_str(tenant_id)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

        self.resolver.invalidate_tenant(tenant_uuid);
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

    /// Check multiple fields access (placeholder)
    ///
    /// # Errors
    ///
    /// Currently never returns an error (placeholder implementation).
    pub fn check_fields_access(
        &self,
        _user_id: Option<String>,
        _roles: Vec<String>,
        _fields: Vec<(String, PyObject)>,
        _tenant_id: Option<String>,
    ) -> PyResult<String> {
        // TODO: Implement full async Python binding
        Ok("check_fields_access not yet implemented".to_string())
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

/// Python wrapper for `RowConstraintResolver`
#[derive(Debug)]
#[pyclass]
pub struct PyRowConstraintResolver {
    resolver: RowConstraintResolver,
}

#[pymethods]
impl PyRowConstraintResolver {
    /// # Errors
    ///
    /// Returns a Python error if the database pool is not initialized.
    #[new]
    #[allow(clippy::needless_pass_by_value)] // PyO3 requires Py<T> to be passed by value
    pub fn new(pool: Py<crate::db::pool::DatabasePool>, cache_capacity: usize) -> PyResult<Self> {
        Python::with_gil(|py| {
            let db_pool = pool.borrow(py);
            let rust_pool = db_pool.get_pool().ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Database pool not initialized")
            })?;
            let resolver = RowConstraintResolver::new(rust_pool, cache_capacity);
            Ok(Self { resolver })
        })
    }

    /// Get row-level filters for a user on a table (placeholder - full async implementation needed)
    ///
    /// # Errors
    ///
    /// Currently never returns an error (placeholder implementation).
    pub fn get_row_filters(
        &self,
        _user_id: String,
        _table_name: String,
        _roles: Vec<String>,
        _tenant_id: Option<String>,
    ) -> PyResult<String> {
        // TODO: Implement full async Python binding with pyo3_asyncio
        Ok("get_row_filters not yet implemented".to_string())
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
}

/// Python wrapper for `WhereMerger`
#[derive(Debug)]
#[pyclass]
pub struct PyWhereMerger;

#[pymethods]
impl PyWhereMerger {
    /// Merge explicit WHERE clause with row-level auth filter
    ///
    /// # Arguments
    ///
    /// - `explicit_where`: User-provided WHERE clause (JSON object as string)
    /// - `auth_filter`: Row-level filter (JSON object as string)
    /// - `strategy`: Conflict handling strategy ("error", "override", or "log")
    ///
    /// # Returns
    ///
    /// Merged WHERE clause as JSON string, or None if no filtering needed.
    ///
    /// # Errors
    ///
    /// Returns a Python error if:
    /// - WHERE clause structures are invalid
    /// - JSON parsing fails
    /// - Conflicting fields detected (strategy = "error")
    #[staticmethod]
    pub fn merge_where(
        explicit_where: Option<String>,
        auth_filter: Option<String>,
        strategy: &str,
    ) -> PyResult<Option<String>> {
        // Parse strategy
        let conflict_strategy = match strategy {
            "error" => super::where_merger::ConflictStrategy::Error,
            "override" => super::where_merger::ConflictStrategy::Override,
            "log" => super::where_merger::ConflictStrategy::Log,
            other => {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    format!("Invalid strategy: {}. Must be 'error', 'override', or 'log'", other),
                ))
            }
        };

        // Parse JSON values
        let explicit_value = explicit_where
            .as_deref()
            .map(|s| serde_json::from_str::<Value>(s))
            .transpose()
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Invalid explicit WHERE JSON: {}",
                    e
                ))
            })?;

        let auth_value = auth_filter
            .as_deref()
            .map(|s| serde_json::from_str::<Value>(s))
            .transpose()
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Invalid auth filter JSON: {}",
                    e
                ))
            })?;

        // Merge using WhereMerger
        let merged = WhereMerger::merge_where(explicit_value.as_ref(), auth_value.as_ref(), conflict_strategy)
            .map_err(|e| {
                // Convert Rust error to Python error
                match e {
                    super::where_merger::WhereMergeError::ConflictingFields {
                        field,
                        explicit_op,
                        auth_op,
                    } => {
                        PyErr::new::<pyo3::exceptions::PyPermissionError, _>(format!(
                            "WHERE clause conflict: field '{}' uses {} in explicit WHERE but {} in auth filter",
                            field, explicit_op, auth_op
                        ))
                    }
                    super::where_merger::WhereMergeError::InvalidStructure(msg) => {
                        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                            "Invalid WHERE clause structure: {}",
                            msg
                        ))
                    }
                    super::where_merger::WhereMergeError::SerializationError(msg) => {
                        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                            "Serialization error: {}",
                            msg
                        ))
                    }
                }
            })?;

        // Convert result back to JSON string
        Ok(merged.map(|v| v.to_string()))
    }

    /// Validate WHERE clause structure
    ///
    /// # Arguments
    ///
    /// - `where_clause`: WHERE clause to validate (JSON object as string)
    ///
    /// # Errors
    ///
    /// Returns a Python error if the WHERE clause structure is invalid.
    #[staticmethod]
    pub fn validate_where(where_clause: &str) -> PyResult<()> {
        let value = serde_json::from_str::<Value>(where_clause).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Invalid WHERE JSON: {}",
                e
            ))
        })?;

        WhereMerger::validate_where(&value).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string())
        })?;

        Ok(())
    }
}
