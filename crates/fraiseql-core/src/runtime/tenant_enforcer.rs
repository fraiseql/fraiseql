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
    pub const fn new(org_id: Option<String>) -> Self {
        Self {
            org_id,
            require_tenant: false,
        }
    }

    /// Create with tenant requirement
    pub const fn with_requirement(org_id: Option<String>, require_tenant: bool) -> Self {
        Self {
            org_id,
            require_tenant,
        }
    }

    /// Check if request is tenant-scoped
    pub const fn is_tenant_scoped(&self) -> bool {
        self.org_id.is_some()
    }

    /// Get the `org_id` for this request
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

    /// Enforce tenant scope for raw SQL queries
    ///
    /// Adds WHERE `org_id` = '<`org_id`>' to raw SQL if needed.
    /// This is a simpler approach for raw queries.
    ///
    /// # Security
    ///
    /// The `org_id` value is escaped to prevent SQL injection. Prefer using
    /// `enforce_tenant_scope()` with `WhereClause` AST for parameterized queries.
    ///
    /// # Arguments
    /// * `sql` - Original SQL query
    ///
    /// # Errors
    ///
    /// Returns an error string if tenant enforcement is required but `org_id` is not set.
    ///
    /// # Returns
    /// * Modified SQL with tenant filter, or original if no `org_id`
    pub fn enforce_tenant_scope_sql(&self, sql: &str) -> Result<String, String> {
        // Check if tenant enforcement is required
        if self.require_tenant && self.org_id.is_none() {
            return Err("Request must be tenant-scoped (missing org_id)".to_string());
        }

        // If no org_id, return original SQL unchanged
        let Some(org_id) = &self.org_id else {
            return Ok(sql.to_string());
        };

        // SECURITY: Escape single quotes to prevent SQL injection via org_id
        let escaped_org_id = org_id.replace('\'', "''");

        // For raw SQL, we need to be careful about WHERE clause placement
        let sql_upper = sql.to_uppercase();

        // Add WHERE clause if none exists
        let enforced_sql = if sql_upper.contains("WHERE") {
            // Append to existing WHERE with AND
            format!("{sql} AND org_id = '{escaped_org_id}'")
        } else if sql_upper.contains("GROUP BY") {
            // Insert before GROUP BY
            let parts: Vec<&str> = sql.splitn(2, "GROUP BY").collect();
            if parts.len() == 2 {
                format!("{} WHERE org_id = '{}' GROUP BY {}", parts[0], escaped_org_id, parts[1])
            } else {
                sql.to_string()
            }
        } else if sql_upper.contains("ORDER BY") {
            // Insert before ORDER BY
            let parts: Vec<&str> = sql.splitn(2, "ORDER BY").collect();
            if parts.len() == 2 {
                format!("{} WHERE org_id = '{}' ORDER BY {}", parts[0], escaped_org_id, parts[1])
            } else {
                sql.to_string()
            }
        } else {
            // Append at end
            format!("{sql} WHERE org_id = '{escaped_org_id}'")
        };

        Ok(enforced_sql)
    }
}
