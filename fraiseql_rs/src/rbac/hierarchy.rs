//! Role hierarchy computation using PostgreSQL CTEs.

use uuid::Uuid;
use deadpool_postgres::Pool;
use super::{errors::Result, models::Role};

/// Role hierarchy resolver using recursive CTEs
pub struct RoleHierarchy {
    pool: Pool,
}

impl RoleHierarchy {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Get all roles in hierarchy (including inherited)
    pub async fn get_all_roles(
        &self,
        role_ids: &[Uuid],
        tenant_id: Option<Uuid>,
    ) -> Result<Vec<Role>> {
        if role_ids.is_empty() {
            return Ok(vec![]);
        }

        // Use PostgreSQL recursive CTE to traverse hierarchy
        let sql = r#"
            WITH RECURSIVE role_hierarchy AS (
                -- Base case: direct roles
                SELECT r.*
                FROM roles r
                WHERE r.id::text = ANY($1)
                  AND ($2::text IS NULL OR r.tenant_id::text = $2 OR r.tenant_id IS NULL)

                UNION

                -- Recursive case: parent roles
                SELECT r.*
                FROM roles r
                INNER JOIN role_hierarchy rh ON r.id = rh.parent_role_id
                WHERE $2::text IS NULL OR r.tenant_id::text = $2 OR r.tenant_id IS NULL
            )
            SELECT DISTINCT * FROM role_hierarchy
        "#;

        let client = self.pool.get().await?;
        let role_id_strings: Vec<String> = role_ids.iter().map(|id| id.to_string()).collect();
        let tenant_id_string = tenant_id.map(|id| id.to_string());
        let rows = client.query(sql, &[&role_id_strings, &tenant_id_string]).await?;
        let roles: Vec<Role> = rows.into_iter().map(Role::from_row).collect();

        Ok(roles)
    }

    /// Get all child roles (for role deletion validation)
    pub async fn get_child_roles(
        &self,
        role_id: Uuid,
        tenant_id: Option<Uuid>,
    ) -> Result<Vec<Role>> {
        let sql = r#"
            WITH RECURSIVE role_children AS (
                -- Base case: direct role
                SELECT r.*
                FROM roles r
                WHERE r.id::text = $1

                UNION

                -- Recursive case: child roles
                SELECT r.*
                FROM roles r
                INNER JOIN role_children rc ON r.parent_role_id = rc.id
                WHERE $2::text IS NULL OR r.tenant_id::text = $2
            )
            SELECT * FROM role_children WHERE id != $1
        "#;

        let client = self.pool.get().await?;
        let role_id_string = role_id.to_string();
        let tenant_id_string = tenant_id.map(|id| id.to_string());
        let rows = client.query(sql, &[&role_id_string, &tenant_id_string]).await?;
        let roles: Vec<Role> = rows.into_iter().map(Role::from_row).collect();

        Ok(roles)
    }

    /// Check for hierarchy cycles
    pub async fn detect_cycles(&self, tenant_id: Option<Uuid>) -> Result<Vec<String>> {
        // Find cycles by looking for roles that appear in their own hierarchy
        let sql = r#"
            WITH RECURSIVE role_paths AS (
                SELECT
                    r.id,
                    r.name,
                    ARRAY[r.id::text] as path,
                    false as cycle_detected
                FROM roles r
                WHERE $1::text IS NULL OR r.tenant_id::text = $1

                UNION ALL

                SELECT
                    r.id,
                    r.name,
                    rp.path || r.id::text,
                    r.id::text = ANY(rp.path) -- Cycle detected
                FROM roles r
                JOIN role_paths rp ON r.parent_role_id = rp.id
                WHERE NOT rp.cycle_detected
                  AND ($1::text IS NULL OR r.tenant_id::text = $1)
            )
            SELECT DISTINCT name
            FROM role_paths
            WHERE cycle_detected
            ORDER BY name
        "#;

        let client = self.pool.get().await?;
        let tenant_id_string = tenant_id.map(|id| id.to_string());
        let rows = client.query(sql, &[&tenant_id_string]).await?;
        let cycle_roles: Vec<String> = rows.into_iter().map(|row| row.get(0)).collect();

        Ok(cycle_roles)
    }
}
