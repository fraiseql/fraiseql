//! Tenant identity for FraiseQL.
//!
//! [`TenantContext`] carries a tenant's identity and metadata for use by
//! transports and middleware.
//!
//! This module does **not** enforce isolation. Data-level tenant isolation is
//! enforced by the runtime's security machinery: `inject_params` tenant
//! filters, the `rls_policy` WHERE-clause composition, and (for schema-mode
//! tenancy) per-tenant `search_path` pools in `fraiseql-server`, each with
//! their own real-database test suites.
//!
//! # Example
//!
//! ```rust
//! use fraiseql_core::tenancy::TenantContext;
//!
//! let tenant = TenantContext::new("acme-corp");
//! assert_eq!(tenant.id(), "acme-corp");
//! ```

use std::collections::HashMap;

use chrono::Utc;

/// A tenant's identity and metadata.
///
/// Represents a single tenant in a multi-tenant system. This type carries
/// identity only; query filtering is performed by the runtime's security
/// machinery (`inject_params`, `rls_policy`, per-tenant pools), not by this
/// struct.
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
    /// ```rust
    /// # use fraiseql_core::tenancy::TenantContext;
    /// let tenant = TenantContext::new("company-123");
    /// assert_eq!(tenant.id(), "company-123");
    /// ```
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id:         id.into(),
            created_at: Utc::now().to_rfc3339(),
            metadata:   HashMap::new(),
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
    pub const fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }
}

#[cfg(test)]
mod tests;
