//! Multi-tenancy support for FraiseQL (Phase 11.4)
//!
//! Provides tenant isolation, context extraction, and row-level security.
//!
//! # Architecture
//!
//! Tenants are isolated at the data level:
//! - Each tenant has a unique ID
//! - Queries automatically include tenant filter (WHERE tenant_id = $1)
//! - JWT claims carry tenant_id for authorization
//! - Cross-tenant access is denied
//!
//! # Example
//!
//! ```ignore
//! use fraiseql_core::tenancy::TenantContext;
//! use serde_json::json;
//!
//! // Create tenant context
//! let tenant = TenantContext::new("acme-corp");
//!
//! // Or extract from JWT claims
//! let claims = json!({"tenant_id": "acme-corp", "sub": "user123"});
//! let tenant = TenantContext::from_jwt_claims(&claims)?;
//!
//! // Use in query execution
//! let executor = Executor::with_tenant(schema, db_pool, tenant)?;
//! let result = executor.execute("query { users { id name } }").await?;
//! ```

use std::collections::HashMap;
use chrono::Utc;
use serde_json::Value as JsonValue;

/// Tenant context for row-level security and data isolation.
///
/// Represents a single tenant in a multi-tenant system.
/// All queries executed with this context will be filtered to only include data
/// belonging to this tenant.
#[derive(Debug, Clone)]
pub struct TenantContext {
    /// Tenant identifier (e.g., "acme-corp", UUID, or subdomain).
    id: String,

    /// ISO 8601 formatted creation timestamp.
    created_at: String,

    /// Optional metadata for the tenant.
    metadata: HashMap<String, String>,
}

impl TenantContext {
    /// Create a new tenant context.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique tenant identifier
    ///
    /// # Example
    ///
    /// ```ignore
    /// let tenant = TenantContext::new("company-123");
    /// assert_eq!(tenant.id(), "company-123");
    /// ```
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            created_at: Utc::now().to_rfc3339(),
            metadata: HashMap::new(),
        }
    }

    /// Get the tenant ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the creation timestamp in ISO 8601 format.
    #[must_use]
    pub fn created_at_iso8601(&self) -> Option<&str> {
        if self.created_at.is_empty() {
            None
        } else {
            Some(&self.created_at)
        }
    }

    /// Set metadata key-value pair for the tenant.
    pub fn set_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata.insert(key.into(), value.into());
    }

    /// Get metadata value by key.
    #[must_use]
    pub fn get_metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(String::as_str)
    }

    /// Get all metadata.
    #[must_use]
    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }

    /// Create a TenantContext from JWT claims.
    ///
    /// Extracts the `tenant_id` from JWT claims and creates a new TenantContext.
    ///
    /// # Arguments
    ///
    /// * `claims` - JWT claims as JSON object
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `tenant_id` claim is missing
    /// - `tenant_id` is not a string
    ///
    /// # Example
    ///
    /// ```ignore
    /// use serde_json::json;
    /// use fraiseql_core::tenancy::TenantContext;
    ///
    /// let claims = json!({
    ///     "sub": "user123",
    ///     "tenant_id": "acme-corp",
    ///     "email": "alice@acme.com"
    /// });
    ///
    /// let tenant = TenantContext::from_jwt_claims(&claims)?;
    /// assert_eq!(tenant.id(), "acme-corp");
    /// ```
    pub fn from_jwt_claims(claims: &JsonValue) -> Result<Self, String> {
        // Extract tenant_id from claims
        let tenant_id = claims
            .get("tenant_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                "Missing or invalid 'tenant_id' claim in JWT".to_string()
            })?;

        Ok(Self::new(tenant_id))
    }

    /// Generate a WHERE clause for tenant filtering.
    ///
    /// Returns a WHERE clause that restricts data to this tenant.
    /// Can be combined with existing WHERE clauses using AND.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let tenant = TenantContext::new("acme-corp");
    /// let clause = tenant.where_clause();  // "tenant_id = 'acme-corp'"
    /// ```
    #[must_use]
    pub fn where_clause(&self) -> String {
        format!("tenant_id = '{}'", self.id)
    }

    /// Generate a parameterized WHERE clause for PostgreSQL.
    ///
    /// For use with parameterized queries to prevent SQL injection.
    ///
    /// # Arguments
    ///
    /// * `param_index` - Parameter placeholder index (1-based for PostgreSQL)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let tenant = TenantContext::new("acme-corp");
    /// let clause = tenant.where_clause_postgresql(1);  // "tenant_id = $1"
    /// ```
    #[must_use]
    pub fn where_clause_postgresql(&self, param_index: usize) -> String {
        format!("tenant_id = ${}", param_index)
    }

    /// Generate a parameterized WHERE clause for MySQL/SQLite.
    ///
    /// For use with parameterized queries to prevent SQL injection.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let tenant = TenantContext::new("acme-corp");
    /// let clause = tenant.where_clause_parameterized();  // "tenant_id = ?"
    /// ```
    #[must_use]
    pub fn where_clause_parameterized(&self) -> String {
        "tenant_id = ?".to_string()
    }
}

// ============================================================================
// Query Filtering
// ============================================================================

/// Generates a WHERE clause for tenant filtering.
///
/// Returns a WHERE clause that restricts data to a specific tenant.
/// Can be combined with existing WHERE clauses using AND.
///
/// # Example
///
/// ```ignore
/// let tenant = TenantContext::new("acme-corp");
/// let clause = tenant.where_clause();  // "tenant_id = 'acme-corp'"
/// ```
pub fn where_clause(tenant_id: &str) -> String {
    format!("tenant_id = '{}'", tenant_id)
}

/// Generates a parameterized WHERE clause for PostgreSQL.
///
/// For use with parameterized queries to prevent SQL injection.
pub fn where_clause_postgresql(param_index: usize) -> String {
    format!("tenant_id = ${}", param_index)
}

/// Generates a parameterized WHERE clause for MySQL/SQLite.
///
/// For use with parameterized queries to prevent SQL injection.
pub fn where_clause_parameterized() -> String {
    "tenant_id = ?".to_string()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests;

#[cfg(test)]
mod jwt_extraction_tests;
