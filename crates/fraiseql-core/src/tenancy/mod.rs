//! Tenant identity for FraiseQL.
//!
//! [`TenantContext`] carries a tenant's identity and metadata — typically
//! extracted from JWT claims — for use by transports and middleware.
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
//! use serde_json::json;
//!
//! // Create tenant context
//! let tenant = TenantContext::new("acme-corp");
//!
//! // Or extract from JWT claims
//! let claims = json!({"tenant_id": "acme-corp", "sub": "user123"});
//! let tenant = TenantContext::from_jwt_claims(&claims).unwrap();
//! assert_eq!(tenant.id(), "acme-corp");
//! ```

use std::collections::HashMap;

use chrono::Utc;
use serde_json::Value as JsonValue;

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

    /// Create a `TenantContext` from JWT claims.
    ///
    /// Extracts the `tenant_id` from JWT claims and creates a new `TenantContext`.
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
    /// ```rust
    /// use serde_json::json;
    /// use fraiseql_core::tenancy::TenantContext;
    ///
    /// let claims = json!({
    ///     "sub": "user123",
    ///     "tenant_id": "acme-corp",
    ///     "email": "alice@acme.com"
    /// });
    ///
    /// let tenant = TenantContext::from_jwt_claims(&claims).unwrap();
    /// assert_eq!(tenant.id(), "acme-corp");
    /// ```
    /// # Errors
    ///
    /// Returns a `String` error if the `tenant_id` claim is missing or not a string.
    pub fn from_jwt_claims(claims: &JsonValue) -> Result<Self, String> {
        // Extract tenant_id from claims
        let tenant_id = claims
            .get("tenant_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing or invalid 'tenant_id' claim in JWT".to_string())?;

        Ok(Self::new(tenant_id))
    }
}

#[cfg(test)]
mod tests;
