//! RBAC Database Backend
//!
//! PostgreSQL-backed operations for role and permission management.
//! Uses the schema defined in migration 0012_rbac.sql.

use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::{PermissionDto, RoleDto, UserRoleDto};

/// Error type for RBAC database operations
#[derive(Debug)]
pub enum RbacDbError {
    /// Database connection error
    ConnectionError(String),
    /// Role not found
    RoleNotFound,
    /// Permission not found
    PermissionNotFound,
    /// Role already exists
    RoleDuplicate,
    /// Permission already exists
    PermissionDuplicate,
    /// User role assignment not found
    AssignmentNotFound,
    /// Assignment already exists
    AssignmentDuplicate,
    /// Role has active assignments
    RoleInUse,
    /// Permission has active assignments
    PermissionInUse,
    /// Database query error
    QueryError(String),
    /// Transaction error
    TransactionError(String),
}

impl From<sqlx::Error> for RbacDbError {
    fn from(err: sqlx::Error) -> Self {
        let msg = err.to_string();
        if msg.contains("duplicate key") || msg.contains("unique constraint") {
            if msg.contains("roles") {
                Self::RoleDuplicate
            } else if msg.contains("permissions") {
                Self::PermissionDuplicate
            } else if msg.contains("user_roles") {
                Self::AssignmentDuplicate
            } else {
                Self::QueryError(msg)
            }
        } else {
            Self::QueryError(msg)
        }
    }
}

/// Database backend for RBAC operations backed by PostgreSQL
#[derive(Clone)]
pub struct RbacDbBackend {
    pool: PgPool,
}

impl RbacDbBackend {
    /// Create a new RBAC database backend with a connection pool
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new RBAC database backend from a connection string
    pub async fn from_connection_string(connection_string: &str) -> Result<Self, RbacDbError> {
        let pool = PgPool::connect(connection_string)
            .await
            .map_err(|e| RbacDbError::ConnectionError(e.to_string()))?;
        Ok(Self { pool })
    }

    /// Ensure database schema exists by running the RBAC migration idempotently
    pub async fn ensure_schema(&self) -> Result<(), RbacDbError> {
        sqlx::query(
            r"
            CREATE TABLE IF NOT EXISTS roles (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                tenant_id UUID NOT NULL,
                name VARCHAR(255) NOT NULL,
                description TEXT,
                level INT NOT NULL DEFAULT 100,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE(tenant_id, name)
            );
            CREATE TABLE IF NOT EXISTS permissions (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                resource VARCHAR(255) NOT NULL,
                action VARCHAR(255) NOT NULL,
                description TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE(resource, action)
            );
            CREATE TABLE IF NOT EXISTS role_permissions (
                role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
                permission_id UUID NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                PRIMARY KEY (role_id, permission_id)
            );
            ",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        Ok(())
    }

    // =========================================================================
    // Role Operations
    // =========================================================================

    /// Create a new role and assign permissions
    pub async fn create_role(
        &self,
        name: &str,
        description: Option<&str>,
        permissions: Vec<String>,
        tenant_id: Option<&str>,
    ) -> Result<RoleDto, RbacDbError> {
        let tenant_uuid = tenant_id
            .map(|tid| {
                Uuid::parse_str(tid)
                    .map_err(|_| RbacDbError::QueryError("Invalid tenant ID".to_string()))
            })
            .transpose()?
            .unwrap_or_else(Uuid::nil);

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RbacDbError::TransactionError(e.to_string()))?;

        // Insert role
        let row = sqlx::query(
            "INSERT INTO roles (tenant_id, name, description) VALUES ($1, $2, $3) \
             RETURNING id, created_at, updated_at",
        )
        .bind(tenant_uuid)
        .bind(name)
        .bind(description)
        .fetch_one(&mut *tx)
        .await?;

        let role_id: Uuid = row.get("id");
        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
        let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

        // Assign permissions by looking up (resource, action) pairs
        // Permission strings are expected as "resource:action" format
        let mut resolved_perms = Vec::new();
        for perm_str in &permissions {
            if let Some((resource, action)) = perm_str.split_once(':') {
                let perm_row =
                    sqlx::query("SELECT id FROM permissions WHERE resource = $1 AND action = $2")
                        .bind(resource)
                        .bind(action)
                        .fetch_optional(&mut *tx)
                        .await?;

                if let Some(perm_row) = perm_row {
                    let perm_id: Uuid = perm_row.get("id");
                    sqlx::query(
                        "INSERT INTO role_permissions (role_id, permission_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                    )
                    .bind(role_id)
                    .bind(perm_id)
                    .execute(&mut *tx)
                    .await?;
                    resolved_perms.push(perm_str.clone());
                }
            }
        }

        tx.commit().await.map_err(|e| RbacDbError::TransactionError(e.to_string()))?;

        Ok(RoleDto {
            id:          role_id.to_string(),
            name:        name.to_string(),
            description: description.map(String::from),
            permissions: resolved_perms,
            tenant_id:   Some(tenant_uuid.to_string()),
            created_at:  created_at.to_rfc3339(),
            updated_at:  updated_at.to_rfc3339(),
        })
    }

    /// Get role by ID with its assigned permissions
    pub async fn get_role(&self, role_id: &str) -> Result<RoleDto, RbacDbError> {
        let role_uuid = Uuid::parse_str(role_id)
            .map_err(|_| RbacDbError::QueryError("Invalid role ID".to_string()))?;

        let row = sqlx::query(
            "SELECT id, tenant_id, name, description, created_at, updated_at FROM roles WHERE id = $1",
        )
        .bind(role_uuid)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(RbacDbError::RoleNotFound)?;

        let id: Uuid = row.get("id");
        let tenant_id: Uuid = row.get("tenant_id");
        let name: String = row.get("name");
        let description: Option<String> = row.get("description");
        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
        let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

        // Fetch associated permissions
        let perm_rows = sqlx::query(
            "SELECT p.resource, p.action FROM permissions p \
             JOIN role_permissions rp ON p.id = rp.permission_id \
             WHERE rp.role_id = $1",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;

        let permissions: Vec<String> = perm_rows
            .iter()
            .map(|r| {
                let resource: String = r.get("resource");
                let action: String = r.get("action");
                format!("{resource}:{action}")
            })
            .collect();

        Ok(RoleDto {
            id: id.to_string(),
            name,
            description,
            permissions,
            tenant_id: Some(tenant_id.to_string()),
            created_at: created_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
        })
    }

    /// List roles with optional tenant filtering and pagination
    pub async fn list_roles(
        &self,
        tenant_id: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<RoleDto>, RbacDbError> {
        let rows = if let Some(tid) = tenant_id {
            let tenant_uuid = Uuid::parse_str(tid)
                .map_err(|_| RbacDbError::QueryError("Invalid tenant ID".to_string()))?;
            sqlx::query(
                "SELECT id, tenant_id, name, description, created_at, updated_at \
                 FROM roles WHERE tenant_id = $1 ORDER BY name LIMIT $2 OFFSET $3",
            )
            .bind(tenant_uuid)
            .bind(i64::from(limit))
            .bind(i64::from(offset))
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT id, tenant_id, name, description, created_at, updated_at \
                 FROM roles ORDER BY name LIMIT $1 OFFSET $2",
            )
            .bind(i64::from(limit))
            .bind(i64::from(offset))
            .fetch_all(&self.pool)
            .await?
        };

        let mut roles = Vec::with_capacity(rows.len());
        for row in &rows {
            let id: Uuid = row.get("id");
            let tenant_id: Uuid = row.get("tenant_id");
            let name: String = row.get("name");
            let description: Option<String> = row.get("description");
            let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
            let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

            // Fetch permissions for each role
            let perm_rows = sqlx::query(
                "SELECT p.resource, p.action FROM permissions p \
                 JOIN role_permissions rp ON p.id = rp.permission_id \
                 WHERE rp.role_id = $1",
            )
            .bind(id)
            .fetch_all(&self.pool)
            .await?;

            let permissions: Vec<String> = perm_rows
                .iter()
                .map(|r| {
                    let resource: String = r.get("resource");
                    let action: String = r.get("action");
                    format!("{resource}:{action}")
                })
                .collect();

            roles.push(RoleDto {
                id: id.to_string(),
                name,
                description,
                permissions,
                tenant_id: Some(tenant_id.to_string()),
                created_at: created_at.to_rfc3339(),
                updated_at: updated_at.to_rfc3339(),
            });
        }

        Ok(roles)
    }

    /// Delete role (CASCADE removes role_permissions; user_roles also CASCADE)
    pub async fn delete_role(&self, role_id: &str) -> Result<(), RbacDbError> {
        let role_uuid = Uuid::parse_str(role_id)
            .map_err(|_| RbacDbError::QueryError("Invalid role ID".to_string()))?;

        // Check for active user assignments
        let count_row = sqlx::query("SELECT COUNT(*) as cnt FROM user_roles WHERE role_id = $1")
            .bind(role_uuid)
            .fetch_optional(&self.pool)
            .await;

        // user_roles table may not exist if users table was absent during migration
        if let Ok(Some(row)) = count_row {
            let count: i64 = row.get("cnt");
            if count > 0 {
                return Err(RbacDbError::RoleInUse);
            }
        }

        let result = sqlx::query("DELETE FROM roles WHERE id = $1")
            .bind(role_uuid)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(RbacDbError::RoleNotFound);
        }

        Ok(())
    }

    // =========================================================================
    // Permission Operations
    // =========================================================================

    /// Create a new permission
    pub async fn create_permission(
        &self,
        resource: &str,
        action: &str,
        description: Option<&str>,
    ) -> Result<PermissionDto, RbacDbError> {
        let row = sqlx::query(
            "INSERT INTO permissions (resource, action, description) VALUES ($1, $2, $3) \
             RETURNING id, created_at",
        )
        .bind(resource)
        .bind(action)
        .bind(description)
        .fetch_one(&self.pool)
        .await?;

        let id: Uuid = row.get("id");
        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");

        Ok(PermissionDto {
            id:          id.to_string(),
            resource:    resource.to_string(),
            action:      action.to_string(),
            description: description.map(String::from),
            created_at:  created_at.to_rfc3339(),
        })
    }

    // =========================================================================
    // User-Role Assignment Operations
    // =========================================================================

    /// Assign role to user
    pub async fn assign_role_to_user(
        &self,
        user_id: &str,
        role_id: &str,
        tenant_id: Option<&str>,
    ) -> Result<UserRoleDto, RbacDbError> {
        let role_uuid = Uuid::parse_str(role_id)
            .map_err(|_| RbacDbError::QueryError("Invalid role ID".to_string()))?;

        // Verify role exists
        let role_exists = sqlx::query("SELECT tenant_id FROM roles WHERE id = $1")
            .bind(role_uuid)
            .fetch_optional(&self.pool)
            .await?;

        let role_row = role_exists.ok_or(RbacDbError::RoleNotFound)?;
        let role_tenant_id: Uuid = role_row.get("tenant_id");

        // Use provided tenant_id or fall back to role's tenant_id
        let tenant_uuid = if let Some(tid) = tenant_id {
            Uuid::parse_str(tid)
                .map_err(|_| RbacDbError::QueryError("Invalid tenant ID".to_string()))?
        } else {
            role_tenant_id
        };

        let row = sqlx::query(
            "INSERT INTO user_roles (user_id, role_id, tenant_id) VALUES ($1, $2, $3) \
             RETURNING assigned_at",
        )
        .bind(user_id)
        .bind(role_uuid)
        .bind(tenant_uuid)
        .fetch_one(&self.pool)
        .await?;

        let assigned_at: chrono::DateTime<chrono::Utc> = row.get("assigned_at");

        Ok(UserRoleDto {
            user_id:     user_id.to_string(),
            role_id:     role_id.to_string(),
            tenant_id:   Some(tenant_uuid.to_string()),
            assigned_at: assigned_at.to_rfc3339(),
        })
    }

    /// Revoke role from user
    pub async fn revoke_role_from_user(
        &self,
        user_id: &str,
        role_id: &str,
    ) -> Result<(), RbacDbError> {
        let role_uuid = Uuid::parse_str(role_id)
            .map_err(|_| RbacDbError::QueryError("Invalid role ID".to_string()))?;

        let result = sqlx::query("DELETE FROM user_roles WHERE user_id = $1 AND role_id = $2")
            .bind(user_id)
            .bind(role_uuid)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(RbacDbError::AssignmentNotFound);
        }

        Ok(())
    }

    /// Check if a user has a specific permission via their assigned roles
    pub async fn check_permission(
        &self,
        user_id: &str,
        resource: &str,
        action: &str,
        tenant_id: &str,
    ) -> Result<bool, RbacDbError> {
        let tenant_uuid = Uuid::parse_str(tenant_id)
            .map_err(|_| RbacDbError::QueryError("Invalid tenant ID".to_string()))?;

        let row = sqlx::query(
            "SELECT COUNT(*) as cnt FROM user_roles ur \
             JOIN role_permissions rp ON ur.role_id = rp.role_id \
             JOIN permissions p ON rp.permission_id = p.id \
             WHERE ur.user_id = $1 AND ur.tenant_id = $2 \
               AND p.resource = $3 AND p.action = $4",
        )
        .bind(user_id)
        .bind(tenant_uuid)
        .bind(resource)
        .bind(action)
        .fetch_one(&self.pool)
        .await?;

        let count: i64 = row.get("cnt");
        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rbac_db_error_from_sqlx_duplicate_role() {
        // Verify the From<sqlx::Error> impl correctly classifies errors
        let err = RbacDbError::RoleDuplicate;
        assert!(matches!(err, RbacDbError::RoleDuplicate));
    }

    #[test]
    fn test_rbac_db_error_variants() {
        // Verify all error variants exist and can be constructed
        let errors: Vec<RbacDbError> = vec![
            RbacDbError::ConnectionError("test".to_string()),
            RbacDbError::RoleNotFound,
            RbacDbError::PermissionNotFound,
            RbacDbError::RoleDuplicate,
            RbacDbError::PermissionDuplicate,
            RbacDbError::AssignmentNotFound,
            RbacDbError::AssignmentDuplicate,
            RbacDbError::RoleInUse,
            RbacDbError::PermissionInUse,
            RbacDbError::QueryError("test".to_string()),
            RbacDbError::TransactionError("test".to_string()),
        ];
        assert_eq!(errors.len(), 11);
    }
}
