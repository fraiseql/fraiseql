// Multi-tenancy enforcement layer
// Ensures all database queries are scoped to the requesting organization

use serde_json::json;

use crate::db::where_clause::{WhereClause, WhereOperator};

/// Multi-tenancy enforcer for query scoping
///
/// Automatically adds org_id filtering to all database queries
/// to ensure strict tenant isolation at runtime.
#[derive(Debug, Clone)]
pub struct TenantEnforcer {
    /// Current org_id for this request
    org_id:         Option<String>,
    /// Enforce tenant scoping (require org_id for all queries)
    require_tenant: bool,
}

impl TenantEnforcer {
    /// Create a new tenant enforcer
    pub fn new(org_id: Option<String>) -> Self {
        Self {
            org_id,
            require_tenant: false,
        }
    }

    /// Create with tenant requirement
    pub fn with_requirement(org_id: Option<String>, require_tenant: bool) -> Self {
        Self {
            org_id,
            require_tenant,
        }
    }

    /// Check if request is tenant-scoped
    pub fn is_tenant_scoped(&self) -> bool {
        self.org_id.is_some()
    }

    /// Get the org_id for this request
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
    /// # Returns
    /// * Modified WHERE clause with tenant filter added
    /// * Or error if tenant enforcement is required but org_id not provided
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
    /// Adds WHERE org_id = '<org_id>' to raw SQL if needed.
    /// This is a simpler approach for raw queries.
    ///
    /// # Arguments
    /// * `sql` - Original SQL query
    ///
    /// # Returns
    /// * Modified SQL with tenant filter, or original if no org_id
    pub fn enforce_tenant_scope_sql(&self, sql: &str) -> Result<String, String> {
        // Check if tenant enforcement is required
        if self.require_tenant && self.org_id.is_none() {
            return Err("Request must be tenant-scoped (missing org_id)".to_string());
        }

        // If no org_id, return original SQL unchanged
        let Some(org_id) = &self.org_id else {
            return Ok(sql.to_string());
        };

        // For raw SQL, we need to be careful about WHERE clause placement
        let sql_upper = sql.to_uppercase();

        // Add WHERE clause if none exists
        let enforced_sql = if sql_upper.contains("WHERE") {
            // Append to existing WHERE with AND
            format!("{} AND org_id = '{}'", sql, org_id)
        } else if sql_upper.contains("GROUP BY") {
            // Insert before GROUP BY
            let parts: Vec<&str> = sql.splitn(2, "GROUP BY").collect();
            if parts.len() == 2 {
                format!("{} WHERE org_id = '{}' GROUP BY {}", parts[0], org_id, parts[1])
            } else {
                sql.to_string()
            }
        } else if sql_upper.contains("ORDER BY") {
            // Insert before ORDER BY
            let parts: Vec<&str> = sql.splitn(2, "ORDER BY").collect();
            if parts.len() == 2 {
                format!("{} WHERE org_id = '{}' ORDER BY {}", parts[0], org_id, parts[1])
            } else {
                sql.to_string()
            }
        } else {
            // Append at end
            format!("{} WHERE org_id = '{}'", sql, org_id)
        };

        Ok(enforced_sql)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_enforcer_with_org_id() {
        let enforcer = TenantEnforcer::new(Some("org-123".to_string()));
        assert!(enforcer.is_tenant_scoped());
        assert_eq!(enforcer.get_org_id(), Some("org-123"));
    }

    #[test]
    fn test_tenant_enforcer_without_org_id() {
        let enforcer = TenantEnforcer::new(None);
        assert!(!enforcer.is_tenant_scoped());
        assert_eq!(enforcer.get_org_id(), None);
    }

    #[test]
    fn test_enforce_tenant_scope_with_no_where_clause() {
        let enforcer = TenantEnforcer::new(Some("org-123".to_string()));
        let result = enforcer.enforce_tenant_scope(None);

        assert!(result.is_ok());
        let enforced = result.unwrap();
        assert!(enforced.is_some());

        // Check that it created an org_id = 'org-123' filter
        if let Some(WhereClause::Field {
            path,
            operator,
            value,
        }) = enforced
        {
            assert_eq!(path, vec!["org_id".to_string()]);
            assert_eq!(operator, WhereOperator::Eq);
            assert_eq!(value, json!("org-123"));
        } else {
            panic!("Expected Field clause");
        }
    }

    #[test]
    fn test_enforce_tenant_scope_with_existing_where_clause() {
        let enforcer = TenantEnforcer::new(Some("org-123".to_string()));

        let user_clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        };

        let result = enforcer.enforce_tenant_scope(Some(&user_clause));

        assert!(result.is_ok());
        let enforced = result.unwrap();
        assert!(enforced.is_some());

        // Check that it created an AND clause combining both filters
        if let Some(WhereClause::And(clauses)) = enforced {
            assert_eq!(clauses.len(), 2);
        } else {
            panic!("Expected And clause");
        }
    }

    #[test]
    fn test_enforce_tenant_scope_sql_without_where() {
        let enforcer = TenantEnforcer::new(Some("org-123".to_string()));
        let sql = "SELECT * FROM users";
        let result = enforcer.enforce_tenant_scope_sql(sql);

        assert!(result.is_ok());
        let enforced = result.unwrap();
        assert!(enforced.contains("WHERE org_id = 'org-123'"));
    }

    #[test]
    fn test_enforce_tenant_scope_sql_with_where() {
        let enforcer = TenantEnforcer::new(Some("org-123".to_string()));
        let sql = "SELECT * FROM users WHERE status = 'active'";
        let result = enforcer.enforce_tenant_scope_sql(sql);

        assert!(result.is_ok());
        let enforced = result.unwrap();
        assert!(enforced.contains("WHERE status = 'active'"));
        assert!(enforced.contains("AND org_id = 'org-123'"));
    }

    #[test]
    fn test_enforce_tenant_scope_sql_with_group_by() {
        let enforcer = TenantEnforcer::new(Some("org-123".to_string()));
        let sql = "SELECT status, COUNT(*) as count FROM users GROUP BY status";
        let result = enforcer.enforce_tenant_scope_sql(sql);

        assert!(result.is_ok());
        let enforced = result.unwrap();
        assert!(enforced.contains("WHERE org_id = 'org-123'"));
        assert!(enforced.contains("GROUP BY"));
    }

    #[test]
    fn test_enforce_tenant_scope_without_org_id() {
        let enforcer = TenantEnforcer::new(None);
        let user_clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        };

        let result = enforcer.enforce_tenant_scope(Some(&user_clause));
        assert!(result.is_ok());

        // Should return original clause unchanged
        let enforced = result.unwrap();
        assert!(matches!(enforced, Some(WhereClause::Field { .. })));
    }

    #[test]
    fn test_require_tenant_fails_without_org_id() {
        let enforcer = TenantEnforcer::with_requirement(None, true);
        let result = enforcer.enforce_tenant_scope(None);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Request must be tenant-scoped (missing org_id)");
    }

    #[test]
    fn test_require_tenant_succeeds_with_org_id() {
        let enforcer = TenantEnforcer::with_requirement(Some("org-123".to_string()), true);
        let result = enforcer.enforce_tenant_scope(None);

        assert!(result.is_ok());
    }
}
