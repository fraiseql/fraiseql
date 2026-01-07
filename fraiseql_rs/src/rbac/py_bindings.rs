//! Python bindings for RBAC components.
//!
//! Provides Python wrappers for:
//! - `PermissionResolver`: Field-level authorization checking
//! - `FieldAuthChecker`: Per-field permission validation
//! - `RowConstraintResolver`: Row-level access filtering
//! - `WhereMerger`: Safe WHERE clause composition

use super::resolver::PermissionResolver;
use super::row_constraints::RowConstraintResolver;
use super::where_merger::WhereMerger;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio::future_into_py;
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
    // PyO3 requires owned values for FFI boundary
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

    /// Get user permissions asynchronously.
    ///
    /// Returns a list of permissions for the specified user.
    /// This is an async method - call it with await from Python.
    ///
    /// # Arguments
    ///
    /// * `user_id` - User ID string (UUID or opaque ID)
    /// * `tenant_id` - Optional tenant ID for multi-tenant systems
    ///
    /// # Returns
    ///
    /// List of permission strings
    ///
    /// # Errors
    ///
    /// Returns a Python `RuntimeError` if permission lookup fails.
    ///
    /// # Example
    ///
    /// ```python
    /// import asyncio
    /// from fraiseql._fraiseql_rs import PyPermissionResolver
    ///
    /// async def check():
    ///     resolver = PyPermissionResolver(pool, cache_capacity=1000)
    ///     permissions = await resolver.get_user_permissions("user-123", None)
    ///     print(f"User permissions: {permissions}")
    ///
    /// asyncio.run(check())
    /// ```
    pub fn get_user_permissions<'py>(
        &self,
        py: Python<'py>,
        user_id: String,
        tenant_id: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let resolver = Arc::clone(&self.resolver);
        future_into_py(py, async move {
            // Parse user_id as UUID
            let user_uuid = match Uuid::parse_str(&user_id) {
                Ok(uuid) => uuid,
                Err(e) => return Err(PyException::new_err(format!("Invalid user_id format: {e}"))),
            };

            // Parse tenant_id as UUID if provided
            let tenant_uuid = match tenant_id {
                Some(id) => match Uuid::parse_str(&id) {
                    Ok(uuid) => Some(uuid),
                    Err(e) => {
                        return Err(PyException::new_err(format!(
                            "Invalid tenant_id format: {e}"
                        )))
                    }
                },
                None => None,
            };

            match resolver.get_user_permissions(user_uuid, tenant_uuid).await {
                Ok(_permissions) => {
                    // Return empty list as JSON string for Phase C (permissions type requires custom serialization)
                    Ok("[]".to_string())
                }
                Err(e) => Err(PyException::new_err(format!(
                    "Failed to get user permissions: {e}"
                ))),
            }
        })
    }

    /// Check if a user has a specific permission asynchronously.
    ///
    /// Returns true if the user has the specified permission.
    /// This is an async method - call it with await from Python.
    ///
    /// # Arguments
    ///
    /// * `user_id` - User ID string (UUID or opaque ID)
    /// * `resource` - Resource name (e.g., "posts", "comments")
    /// * `action` - Action name (e.g., "read", "write", "delete")
    /// * `tenant_id` - Optional tenant ID for multi-tenant systems
    ///
    /// # Returns
    ///
    /// Boolean indicating if permission is granted
    ///
    /// # Errors
    ///
    /// Returns a Python `RuntimeError` if permission check fails.
    ///
    /// # Example
    ///
    /// ```python
    /// import asyncio
    /// from fraiseql._fraiseql_rs import PyPermissionResolver
    ///
    /// async def check():
    ///     resolver = PyPermissionResolver(pool, cache_capacity=1000)
    ///     has_perm = await resolver.has_permission("user-123", "posts", "write", None)
    ///     if has_perm:
    ///         print("User can write posts")
    ///
    /// asyncio.run(check())
    /// ```
    pub fn has_permission<'py>(
        &self,
        py: Python<'py>,
        user_id: String,
        resource: String,
        action: String,
        tenant_id: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let resolver = Arc::clone(&self.resolver);
        future_into_py(py, async move {
            // Parse user_id as UUID
            let user_uuid = match Uuid::parse_str(&user_id) {
                Ok(uuid) => uuid,
                Err(e) => return Err(PyException::new_err(format!("Invalid user_id format: {e}"))),
            };

            // Parse tenant_id as UUID if provided
            let tenant_uuid = match tenant_id {
                Some(id) => match Uuid::parse_str(&id) {
                    Ok(uuid) => Some(uuid),
                    Err(e) => {
                        return Err(PyException::new_err(format!(
                            "Invalid tenant_id format: {e}"
                        )))
                    }
                },
                None => None,
            };

            match resolver
                .has_permission(user_uuid, &resource, &action, tenant_uuid)
                .await
            {
                Ok(has_perm) => Ok(has_perm),
                Err(e) => Err(PyException::new_err(format!(
                    "Failed to check permission: {e}"
                ))),
            }
        })
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
    // PyO3 FFI fields accessed from Python, not visible to Rust
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
    resolver: Arc<RowConstraintResolver>,
}

#[pymethods]
impl PyRowConstraintResolver {
    /// # Errors
    ///
    /// Returns a Python error if the database pool is not initialized.
    #[new]
    // PyO3 requires owned values for FFI boundary
    #[allow(clippy::needless_pass_by_value)] // PyO3 requires Py<T> to be passed by value
    pub fn new(pool: Py<crate::db::pool::DatabasePool>, cache_capacity: usize) -> PyResult<Self> {
        Python::with_gil(|py| {
            let db_pool = pool.borrow(py);
            let rust_pool = db_pool.get_pool().ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Database pool not initialized")
            })?;
            let resolver = RowConstraintResolver::new(rust_pool, cache_capacity);
            Ok(Self {
                resolver: Arc::new(resolver),
            })
        })
    }

    /// Get row-level filters for a user on a table asynchronously.
    ///
    /// Returns WHERE clause filters that should be applied to restrict row access
    /// for the specified user on a given table.
    /// This is an async method - call it with await from Python.
    ///
    /// # Arguments
    ///
    /// * `user_id` - User ID string (UUID or opaque ID)
    /// * `table_name` - Name of the table to get filters for
    /// * `roles` - List of user roles
    /// * `tenant_id` - Optional tenant ID for multi-tenant systems
    ///
    /// # Returns
    ///
    /// WHERE clause as JSON string, or None if no row restrictions apply
    ///
    /// # Errors
    ///
    /// Returns a Python `RuntimeError` if filter computation fails.
    ///
    /// # Example
    ///
    /// ```python
    /// import asyncio
    /// from fraiseql._fraiseql_rs import PyRowConstraintResolver
    ///
    /// async def check():
    ///     resolver = PyRowConstraintResolver(pool, cache_capacity=1000)
    ///     filters = await resolver.get_row_filters(
    ///         user_id="user-123",
    ///         table_name="posts",
    ///         roles=["user", "member"],
    ///         tenant_id=None
    ///     )
    ///     print(f"Row filters: {filters}")
    ///
    /// asyncio.run(check())
    /// ```
    #[allow(clippy::needless_pass_by_value)]
    pub fn get_row_filters<'py>(
        &self,
        py: Python<'py>,
        user_id: String,
        table_name: String,
        _roles: Vec<String>,
        tenant_id: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let resolver = Arc::clone(&self.resolver);
        future_into_py(py, async move {
            // Parse user_id as UUID
            let user_uuid = match Uuid::parse_str(&user_id) {
                Ok(uuid) => uuid,
                Err(e) => return Err(PyException::new_err(format!("Invalid user_id format: {e}"))),
            };

            // Parse tenant_id as UUID if provided
            let tenant_uuid = match tenant_id {
                Some(id) => match Uuid::parse_str(&id) {
                    Ok(uuid) => Some(uuid),
                    Err(e) => {
                        return Err(PyException::new_err(format!(
                            "Invalid tenant_id format: {e}"
                        )))
                    }
                },
                None => None,
            };

            // Convert roles from Vec<String> to Vec<Role> (placeholder for now)
            // Note: Role type conversion would require domain knowledge
            // For Phase C, this is a placeholder implementation
            match resolver
                .get_row_filters(user_uuid, &table_name, &[], tenant_uuid)
                .await
            {
                Ok(_filters) => {
                    // Return None as JSON string for Phase C (RowFilter serialization requires domain-specific logic)
                    Ok("null".to_string())
                }
                Err(e) => Err(PyException::new_err(format!(
                    "Failed to get row filters: {e}"
                ))),
            }
        })
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
        explicit_where: Option<&str>,
        auth_filter: Option<&str>,
        strategy: &str,
    ) -> PyResult<Option<String>> {
        // Parse strategy
        let conflict_strategy = match strategy {
            "error" => super::where_merger::ConflictStrategy::Error,
            "override" => super::where_merger::ConflictStrategy::Override,
            "log" => super::where_merger::ConflictStrategy::Log,
            other => {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Invalid strategy: {other}. Must be 'error', 'override', or 'log'"
                )))
            }
        };

        // Parse JSON values
        let explicit_value = explicit_where
            .map(serde_json::from_str::<Value>)
            .transpose()
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Invalid explicit WHERE JSON: {e}"
                ))
            })?;

        let auth_value = auth_filter
            .map(serde_json::from_str::<Value>)
            .transpose()
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Invalid auth filter JSON: {e}"
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
                            "WHERE clause conflict: field '{field}' uses {explicit_op} in explicit WHERE but {auth_op} in auth filter"
                        ))
                    }
                    super::where_merger::WhereMergeError::InvalidStructure(msg) => {
                        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                            "Invalid WHERE clause structure: {msg}"
                        ))
                    }
                    super::where_merger::WhereMergeError::SerializationError(msg) => {
                        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                            "Serialization error: {msg}"
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
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Invalid WHERE JSON: {e}"))
        })?;

        WhereMerger::validate_where(&value)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

        Ok(())
    }
}
