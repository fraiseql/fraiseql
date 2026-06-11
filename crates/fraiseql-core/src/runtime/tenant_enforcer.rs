//! Multi-tenancy enforcement layer.
//!
//! Ensures all database queries are scoped to the requesting organization by
//! automatically injecting `org_id` filters into WHERE clauses. Supports both
//! the structured [`WhereClause`] AST (preferred, parameterized) and raw SQL
//! string injection as a fallback.

use serde_json::json;

use crate::db::where_clause::{WhereClause, WhereOperator};

/// Multi-tenancy enforcer for query scoping
///
/// Automatically adds `org_id` filtering to all database queries
/// to ensure strict tenant isolation at runtime.
#[derive(Debug, Clone)]
pub struct TenantEnforcer {
    /// Current `org_id` for this request
    org_id:         Option<String>,
    /// Enforce tenant scoping (require `org_id` for all queries)
    require_tenant: bool,
}

impl TenantEnforcer {
    /// Create a new tenant enforcer
    #[must_use]
    pub const fn new(org_id: Option<String>) -> Self {
        Self {
            org_id,
            require_tenant: false,
        }
    }

    /// Create with tenant requirement
    #[must_use]
    pub const fn with_requirement(org_id: Option<String>, require_tenant: bool) -> Self {
        Self {
            org_id,
            require_tenant,
        }
    }

    /// Check if request is tenant-scoped
    #[must_use]
    pub const fn is_tenant_scoped(&self) -> bool {
        self.org_id.is_some()
    }

    /// Get the `org_id` for this request
    #[must_use]
    pub fn get_org_id(&self) -> Option<&str> {
        self.org_id.as_deref()
    }

    /// Enforce tenant scoping on a WHERE clause
    ///
    /// Automatically adds an `AND org_id = <org_id>` condition
    /// to ensure all queries return only data for the current tenant.
    ///
    /// # Arguments
    /// * `where_clause` - User-provided WHERE clause
    ///
    /// # Errors
    ///
    /// Returns an error string if tenant enforcement is required but `org_id` is not set.
    ///
    /// # Returns
    /// * Modified WHERE clause with tenant filter added
    /// * Or error if tenant enforcement is required but `org_id` not provided
    pub fn enforce_tenant_scope(
        &self,
        where_clause: Option<&WhereClause>,
    ) -> Result<Option<WhereClause>, String> {
        // Check if tenant enforcement is required
        if self.require_tenant && self.org_id.is_none() {
            return Err("Request must be tenant-scoped (missing org_id)".to_string());
        }

        // If no org_id, return original clause unchanged (public/unauthenticated)
        let Some(org_id) = &self.org_id else {
            return Ok(where_clause.cloned());
        };

        // Build org_id filter clause
        let org_id_filter = WhereClause::Field {
            path:     vec!["org_id".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(org_id),
        };

        // Combine with user's WHERE clause
        let enforced_clause = match where_clause {
            None => org_id_filter,
            Some(user_clause) => WhereClause::And(vec![user_clause.clone(), org_id_filter]),
        };

        Ok(Some(enforced_clause))
    }
}
