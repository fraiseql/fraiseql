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
//!
//! // Create tenant context
//! let tenant = TenantContext::new("acme-corp");
//!
//! // Use in query execution
//! let executor = Executor::with_tenant(schema, db_pool, tenant)?;
//! let result = executor.execute("query { users { id name } }").await?;
//! ```

use std::collections::HashMap;
use chrono::Utc;

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
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests;
