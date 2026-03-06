//! RBAC Database Backend
//!
//! PostgreSQL-backed operations for role and permission management.

use chrono::Utc;
use sqlx::{PgPool, Row, postgres::PgRow};
use tracing::debug;
use uuid::Uuid;

use super::{PermissionDto, RoleDto, UserRoleDto};

/// Error type for RBAC database operations.
#[derive(Debug)]
pub enum RbacDbError {
    /// Database connection error.
    ConnectionError(String),
    /// Role not found.
    RoleNotFound,
    /// Permission not found.
    PermissionNotFound,
    /// Role already exists.
    RoleDuplicate,
    /// Permission already exists.
    PermissionDuplicate,
    /// User role assignment not found.
    AssignmentNotFound,
    /// Assignment already exists.
    AssignmentDuplicate,
    /// Role has active assignments.
    RoleInUse,
    /// Permission has active assignments.
    PermissionInUse,
    /// Database query error.
    QueryError(String),
    /// Transaction error.
    TransactionError(String),
}

impl std::fmt::Display for RbacDbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionError(msg) => write!(f, "Connection error: {msg}"),
            Self::RoleNotFound => write!(f, "Role not found"),
            Self::PermissionNotFound => write!(f, "Permission not found"),
            Self::RoleDuplicate => write!(f, "Role already exists"),
            Self::PermissionDuplicate => write!(f, "Permission already exists"),
            Self::AssignmentNotFound => write!(f, "Assignment not found"),
            Self::AssignmentDuplicate => write!(f, "Assignment already exists"),
            Self::RoleInUse => write!(f, "Role has active assignments"),
            Self::PermissionInUse => write!(f, "Permission has active assignments"),
            Self::QueryError(msg) => write!(f, "Query error: {msg}"),
            Self::TransactionError(msg) => write!(f, "Transaction error: {msg}"),
        }
    }
}

impl std::error::Error for RbacDbError {}

/// Database backend for RBAC operations.
#[derive(Clone)]
pub struct RbacDbBackend {
    pool: PgPool,
}

impl RbacDbBackend {
    /// Create a new RBAC database backend from a connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Ensure the RBAC database schema exists.
    ///
    /// Creates all required tables and indexes if they don't already exist.
    /// This operation is idempotent.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::QueryError` if the schema creation SQL fails.
    pub async fn ensure_schema(&self) -> Result<(), RbacDbError> {
        sqlx::raw_sql(
            "CREATE TABLE IF NOT EXISTS fraiseql_roles (
                id UUID PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                tenant_id UUID,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE(name, COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid))
            );

            CREATE TABLE IF NOT EXISTS fraiseql_permissions (
                id UUID PRIMARY KEY,
                resource TEXT NOT NULL,
                action TEXT NOT NULL,
                description TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE(resource, action)
            );

            CREATE TABLE IF NOT EXISTS fraiseql_role_permissions (
                role_id UUID REFERENCES fraiseql_roles(id) ON DELETE CASCADE,
                permission_id UUID REFERENCES fraiseql_permissions(id) ON DELETE CASCADE,
                PRIMARY KEY (role_id, permission_id)
            );

            CREATE TABLE IF NOT EXISTS fraiseql_user_roles (
                user_id TEXT NOT NULL,
                role_id UUID REFERENCES fraiseql_roles(id) ON DELETE CASCADE,
                tenant_id UUID,
                assigned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                PRIMARY KEY (user_id, role_id)
            );

            CREATE INDEX IF NOT EXISTS idx_fraiseql_roles_tenant
                ON fraiseql_roles(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_fraiseql_user_roles_user
                ON fraiseql_user_roles(user_id);
            CREATE INDEX IF NOT EXISTS idx_fraiseql_user_roles_role
                ON fraiseql_user_roles(role_id);",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| RbacDbError::QueryError(format!("Schema creation failed: {e}")))?;

        debug!("RBAC schema ensured");
        Ok(())
    }

    // =========================================================================
    // Role Operations
    // =========================================================================

    /// Create a new role with associated permissions.
    ///
    /// Permissions are specified as `"resource:action"` strings. Each permission
    /// is created if it doesn't already exist, then linked to the role.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::QueryError` if `tenant_id` is not a valid UUID.
    /// Returns `RbacDbError::ConnectionError` if a transaction cannot be started.
    /// Returns `RbacDbError::RoleDuplicate` if a role with the same name already exists.
    /// Returns `RbacDbError::QueryError` if any database operation fails.
    /// Returns `RbacDbError::TransactionError` if the transaction cannot be committed.
    pub async fn create_role(
        &self,
        name: &str,
        description: Option<&str>,
        permissions: Vec<String>,
        tenant_id: Option<&str>,
    ) -> Result<RoleDto, RbacDbError> {
        let role_id = Uuid::new_v4();
        let now = Utc::now();
        let tenant_uuid = tenant_id
            .map(|tid| {
                Uuid::parse_str(tid)
                    .map_err(|_| RbacDbError::QueryError("Invalid tenant ID".into()))
            })
            .transpose()?;

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RbacDbError::ConnectionError(e.to_string()))?;

        // Insert role
        sqlx::query(
            "INSERT INTO fraiseql_roles (id, name, description, tenant_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $5)",
        )
        .bind(role_id)
        .bind(name)
        .bind(description)
        .bind(tenant_uuid)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                RbacDbError::RoleDuplicate
            } else {
                RbacDbError::QueryError(e.to_string())
            }
        })?;

        // Create or find permissions, then link to role
        for perm_str in &permissions {
            let (resource, action) = parse_permission(perm_str)?;
            let perm_id = self.ensure_permission(&mut tx, resource, action).await?;
            sqlx::query(
                "INSERT INTO fraiseql_role_permissions (role_id, permission_id)
                 VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(role_id)
            .bind(perm_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| RbacDbError::QueryError(e.to_string()))?;
        }

        tx.commit().await.map_err(|e| RbacDbError::TransactionError(e.to_string()))?;

        Ok(RoleDto {
            id: role_id.to_string(),
            name: name.to_string(),
            description: description.map(String::from),
            permissions,
            tenant_id: tenant_uuid.map(|u| u.to_string()),
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        })
    }

    /// Get role by ID with its associated permissions.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::QueryError` if `role_id` is not a valid UUID or the query fails.
    /// Returns `RbacDbError::RoleNotFound` if no role with the given ID exists.
    pub async fn get_role(&self, role_id: &str) -> Result<RoleDto, RbacDbError> {
        let role_uuid = Uuid::parse_str(role_id)
            .map_err(|_| RbacDbError::QueryError("Invalid role ID".into()))?;

        let row = sqlx::query(
            "SELECT id, name, description, tenant_id, created_at, updated_at
             FROM fraiseql_roles WHERE id = $1",
        )
        .bind(role_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?
        .ok_or(RbacDbError::RoleNotFound)?;

        let permissions = self.get_role_permissions(role_uuid).await?;

        Ok(role_dto_from_row(&row, permissions))
    }

    /// List roles with optional tenant filtering and pagination.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::QueryError` if `tenant_id` is not a valid UUID or the query fails.
    pub async fn list_roles(
        &self,
        tenant_id: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<RoleDto>, RbacDbError> {
        let tenant_uuid = tenant_id
            .map(|tid| {
                Uuid::parse_str(tid)
                    .map_err(|_| RbacDbError::QueryError("Invalid tenant ID".into()))
            })
            .transpose()?;

        let rows = if let Some(tid) = tenant_uuid {
            sqlx::query(
                "SELECT id, name, description, tenant_id, created_at, updated_at
                 FROM fraiseql_roles WHERE tenant_id = $1
                 ORDER BY name LIMIT $2 OFFSET $3",
            )
            .bind(tid)
            .bind(i64::from(limit))
            .bind(i64::from(offset))
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query(
                "SELECT id, name, description, tenant_id, created_at, updated_at
                 FROM fraiseql_roles
                 ORDER BY name LIMIT $1 OFFSET $2",
            )
            .bind(i64::from(limit))
            .bind(i64::from(offset))
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        let mut roles = Vec::with_capacity(rows.len());
        for row in &rows {
            let id: Uuid = row.get("id");
            let permissions = self.get_role_permissions(id).await?;
            roles.push(role_dto_from_row(row, permissions));
        }
        Ok(roles)
    }

    /// Update an existing role's name, description, and permissions.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::QueryError` if `role_id` is not a valid UUID.
    /// Returns `RbacDbError::ConnectionError` if a transaction cannot be started.
    /// Returns `RbacDbError::RoleDuplicate` if the new name conflicts with an existing role.
    /// Returns `RbacDbError::RoleNotFound` if no role with the given ID exists.
    /// Returns `RbacDbError::QueryError` if any database operation fails.
    /// Returns `RbacDbError::TransactionError` if the transaction cannot be committed.
    pub async fn update_role(
        &self,
        role_id: &str,
        name: &str,
        description: Option<&str>,
        permissions: Vec<String>,
    ) -> Result<RoleDto, RbacDbError> {
        let role_uuid = Uuid::parse_str(role_id)
            .map_err(|_| RbacDbError::QueryError("Invalid role ID".into()))?;
        let now = Utc::now();

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RbacDbError::ConnectionError(e.to_string()))?;

        // Update role metadata
        let result = sqlx::query(
            "UPDATE fraiseql_roles SET name = $1, description = $2, updated_at = $3
             WHERE id = $4",
        )
        .bind(name)
        .bind(description)
        .bind(now)
        .bind(role_uuid)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                RbacDbError::RoleDuplicate
            } else {
                RbacDbError::QueryError(e.to_string())
            }
        })?;

        if result.rows_affected() == 0 {
            return Err(RbacDbError::RoleNotFound);
        }

        // Replace permissions: delete existing, add new
        sqlx::query("DELETE FROM fraiseql_role_permissions WHERE role_id = $1")
            .bind(role_uuid)
            .execute(&mut *tx)
            .await
            .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        for perm_str in &permissions {
            let (resource, action) = parse_permission(perm_str)?;
            let perm_id = self.ensure_permission(&mut tx, resource, action).await?;
            sqlx::query(
                "INSERT INTO fraiseql_role_permissions (role_id, permission_id)
                 VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(role_uuid)
            .bind(perm_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| RbacDbError::QueryError(e.to_string()))?;
        }

        tx.commit().await.map_err(|e| RbacDbError::TransactionError(e.to_string()))?;

        // Fetch the updated role to get tenant_id and timestamps
        self.get_role(role_id).await
    }

    /// Delete a role by ID (cascades to role_permissions and user_roles).
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::QueryError` if `role_id` is not a valid UUID or the query fails.
    /// Returns `RbacDbError::RoleNotFound` if no role with the given ID exists.
    pub async fn delete_role(&self, role_id: &str) -> Result<(), RbacDbError> {
        let role_uuid = Uuid::parse_str(role_id)
            .map_err(|_| RbacDbError::QueryError("Invalid role ID".into()))?;

        let result = sqlx::query("DELETE FROM fraiseql_roles WHERE id = $1")
            .bind(role_uuid)
            .execute(&self.pool)
            .await
            .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(RbacDbError::RoleNotFound);
        }
        Ok(())
    }

    // =========================================================================
    // Permission Operations
    // =========================================================================

    /// Create a new permission.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::PermissionDuplicate` if a permission with the same resource and action already exists.
    /// Returns `RbacDbError::QueryError` if the database insert fails.
    pub async fn create_permission(
        &self,
        resource: &str,
        action: &str,
        description: Option<&str>,
    ) -> Result<PermissionDto, RbacDbError> {
        let perm_id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            "INSERT INTO fraiseql_permissions (id, resource, action, description, created_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(perm_id)
        .bind(resource)
        .bind(action)
        .bind(description)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                RbacDbError::PermissionDuplicate
            } else {
                RbacDbError::QueryError(e.to_string())
            }
        })?;

        Ok(PermissionDto {
            id:          perm_id.to_string(),
            resource:    resource.to_string(),
            action:      action.to_string(),
            description: description.map(String::from),
            created_at:  now.to_rfc3339(),
        })
    }

    /// List all permissions.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::QueryError` if the database query fails.
    pub async fn list_permissions(&self) -> Result<Vec<PermissionDto>, RbacDbError> {
        let rows = sqlx::query(
            "SELECT id, resource, action, description, created_at
             FROM fraiseql_permissions ORDER BY resource, action",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        Ok(rows.iter().map(permission_dto_from_row).collect())
    }

    /// Get a permission by ID.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::QueryError` if `permission_id` is not a valid UUID or the query fails.
    /// Returns `RbacDbError::PermissionNotFound` if no permission with the given ID exists.
    pub async fn get_permission(&self, permission_id: &str) -> Result<PermissionDto, RbacDbError> {
        let perm_uuid = Uuid::parse_str(permission_id)
            .map_err(|_| RbacDbError::QueryError("Invalid permission ID".into()))?;

        let row = sqlx::query(
            "SELECT id, resource, action, description, created_at
             FROM fraiseql_permissions WHERE id = $1",
        )
        .bind(perm_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?
        .ok_or(RbacDbError::PermissionNotFound)?;

        Ok(permission_dto_from_row(&row))
    }

    /// Delete a permission by ID.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::QueryError` if `permission_id` is not a valid UUID or the query fails.
    /// Returns `RbacDbError::PermissionInUse` if the permission is referenced by one or more roles.
    /// Returns `RbacDbError::PermissionNotFound` if no permission with the given ID exists.
    pub async fn delete_permission(&self, permission_id: &str) -> Result<(), RbacDbError> {
        let perm_uuid = Uuid::parse_str(permission_id)
            .map_err(|_| RbacDbError::QueryError("Invalid permission ID".into()))?;

        // Check if permission is referenced by any role
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM fraiseql_role_permissions WHERE permission_id = $1",
        )
        .bind(perm_uuid)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        if count > 0 {
            return Err(RbacDbError::PermissionInUse);
        }

        let result = sqlx::query("DELETE FROM fraiseql_permissions WHERE id = $1")
            .bind(perm_uuid)
            .execute(&self.pool)
            .await
            .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(RbacDbError::PermissionNotFound);
        }
        Ok(())
    }

    // =========================================================================
    // User-Role Assignment Operations
    // =========================================================================

    /// Assign a role to a user.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::QueryError` if `role_id` or `tenant_id` is not a valid UUID.
    /// Returns `RbacDbError::RoleNotFound` if no role with the given ID exists.
    /// Returns `RbacDbError::AssignmentDuplicate` if the user already has this role.
    /// Returns `RbacDbError::QueryError` if the database insert fails.
    pub async fn assign_role_to_user(
        &self,
        user_id: &str,
        role_id: &str,
        tenant_id: Option<&str>,
    ) -> Result<UserRoleDto, RbacDbError> {
        let role_uuid = Uuid::parse_str(role_id)
            .map_err(|_| RbacDbError::QueryError("Invalid role ID".into()))?;
        let tenant_uuid = tenant_id
            .map(|tid| {
                Uuid::parse_str(tid)
                    .map_err(|_| RbacDbError::QueryError("Invalid tenant ID".into()))
            })
            .transpose()?;
        let now = Utc::now();

        // Verify role exists
        let role_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fraiseql_roles WHERE id = $1)")
                .bind(role_uuid)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        if !role_exists {
            return Err(RbacDbError::RoleNotFound);
        }

        sqlx::query(
            "INSERT INTO fraiseql_user_roles (user_id, role_id, tenant_id, assigned_at)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(user_id)
        .bind(role_uuid)
        .bind(tenant_uuid)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                RbacDbError::AssignmentDuplicate
            } else {
                RbacDbError::QueryError(e.to_string())
            }
        })?;

        Ok(UserRoleDto {
            user_id:     user_id.to_string(),
            role_id:     role_id.to_string(),
            tenant_id:   tenant_uuid.map(|u| u.to_string()),
            assigned_at: now.to_rfc3339(),
        })
    }

    /// List all role assignments for a user.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::QueryError` if the database query fails.
    pub async fn list_user_roles(&self, user_id: &str) -> Result<Vec<UserRoleDto>, RbacDbError> {
        let rows = sqlx::query(
            "SELECT user_id, role_id, tenant_id, assigned_at
             FROM fraiseql_user_roles WHERE user_id = $1
             ORDER BY assigned_at",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        Ok(rows
            .iter()
            .map(|row| {
                let role_id: Uuid = row.get("role_id");
                let tenant_id: Option<Uuid> = row.get("tenant_id");
                let assigned_at: chrono::DateTime<Utc> = row.get("assigned_at");
                UserRoleDto {
                    user_id:     row.get::<String, _>("user_id"),
                    role_id:     role_id.to_string(),
                    tenant_id:   tenant_id.map(|u| u.to_string()),
                    assigned_at: assigned_at.to_rfc3339(),
                }
            })
            .collect())
    }

    /// Revoke a role from a user.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::QueryError` if `role_id` is not a valid UUID or the query fails.
    /// Returns `RbacDbError::AssignmentNotFound` if the user does not have this role assigned.
    pub async fn revoke_role_from_user(
        &self,
        user_id: &str,
        role_id: &str,
    ) -> Result<(), RbacDbError> {
        let role_uuid = Uuid::parse_str(role_id)
            .map_err(|_| RbacDbError::QueryError("Invalid role ID".into()))?;

        let result =
            sqlx::query("DELETE FROM fraiseql_user_roles WHERE user_id = $1 AND role_id = $2")
                .bind(user_id)
                .bind(role_uuid)
                .execute(&self.pool)
                .await
                .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(RbacDbError::AssignmentNotFound);
        }
        Ok(())
    }

    // =========================================================================
    // Internal Helpers
    // =========================================================================

    /// Get the `"resource:action"` permission strings for a role.
    async fn get_role_permissions(&self, role_id: Uuid) -> Result<Vec<String>, RbacDbError> {
        let rows = sqlx::query(
            "SELECT p.resource, p.action
             FROM fraiseql_permissions p
             JOIN fraiseql_role_permissions rp ON rp.permission_id = p.id
             WHERE rp.role_id = $1
             ORDER BY p.resource, p.action",
        )
        .bind(role_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        Ok(rows
            .iter()
            .map(|r| {
                let resource: String = r.get("resource");
                let action: String = r.get("action");
                format!("{resource}:{action}")
            })
            .collect())
    }

    /// Find or create a permission, returning its UUID.
    async fn ensure_permission(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        resource: &str,
        action: &str,
    ) -> Result<Uuid, RbacDbError> {
        // Try to find existing
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM fraiseql_permissions WHERE resource = $1 AND action = $2",
        )
        .bind(resource)
        .bind(action)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        if let Some(id) = existing {
            return Ok(id);
        }

        // Create new
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO fraiseql_permissions (id, resource, action, created_at)
             VALUES ($1, $2, $3, NOW())",
        )
        .bind(id)
        .bind(resource)
        .bind(action)
        .execute(&mut **tx)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        Ok(id)
    }
}

/// Parse a `"resource:action"` string into its components.
fn parse_permission(perm: &str) -> Result<(&str, &str), RbacDbError> {
    perm.split_once(':').ok_or_else(|| {
        RbacDbError::QueryError(format!(
            "Invalid permission format '{perm}': expected 'resource:action'"
        ))
    })
}

/// Check if a sqlx error is a unique constraint violation.
fn is_unique_violation(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = e {
        db_err.code().as_deref() == Some("23505")
    } else {
        false
    }
}

/// Convert a database row to a `RoleDto`.
fn role_dto_from_row(row: &PgRow, permissions: Vec<String>) -> RoleDto {
    let id: Uuid = row.get("id");
    let tenant_id: Option<Uuid> = row.get("tenant_id");
    let created_at: chrono::DateTime<Utc> = row.get("created_at");
    let updated_at: chrono::DateTime<Utc> = row.get("updated_at");
    RoleDto {
        id: id.to_string(),
        name: row.get("name"),
        description: row.get("description"),
        permissions,
        tenant_id: tenant_id.map(|u| u.to_string()),
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
    }
}

/// Convert a database row to a `PermissionDto`.
fn permission_dto_from_row(row: &PgRow) -> PermissionDto {
    let id: Uuid = row.get("id");
    let created_at: chrono::DateTime<Utc> = row.get("created_at");
    PermissionDto {
        id:          id.to_string(),
        resource:    row.get("resource"),
        action:      row.get("action"),
        description: row.get("description"),
        created_at:  created_at.to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_permission_valid() {
        let (resource, action) = parse_permission("content:write").unwrap();
        assert_eq!(resource, "content");
        assert_eq!(action, "write");
    }

    #[test]
    fn test_parse_permission_wildcard() {
        let (resource, action) = parse_permission("*:*").unwrap();
        assert_eq!(resource, "*");
        assert_eq!(action, "*");
    }

    #[test]
    fn test_parse_permission_invalid() {
        assert!(parse_permission("no_colon").is_err());
    }

    #[test]
    fn test_rbac_db_error_display() {
        assert_eq!(format!("{}", RbacDbError::RoleNotFound), "Role not found");
        assert_eq!(format!("{}", RbacDbError::RoleDuplicate), "Role already exists");
    }
}
