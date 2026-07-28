//! RBAC Database Backend
//!
//! PostgreSQL-backed operations for role and permission management.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row, postgres::PgRow};
use tracing::debug;
use uuid::Uuid;

use super::{PermissionDto, RoleDto, UserRoleDto};

/// Error type for RBAC database operations.
///
/// The distinction between [`Self::InvalidInput`] and [`Self::QueryError`] is
/// load-bearing: the handlers derive their HTTP status from this enum, and before
/// #769 every one of them mapped *every* failure — a malformed `resource:action`
/// string, a non-UUID tenant, and an unreachable database alike — to
/// `409 role_duplicate`. An operator debugging a dead database was told their role
/// already existed.
#[derive(Debug)]
#[non_exhaustive]
pub enum RbacDbError {
    /// Database connection error.
    ConnectionError(String),
    /// The caller supplied something malformed (a non-UUID id, a permission string
    /// with no `:`). Distinct from [`Self::QueryError`], which is the server's fault.
    InvalidInput(String),
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
            Self::InvalidInput(msg) => write!(f, "{msg}"),
            Self::RoleNotFound => write!(f, "Role not found"),
            Self::PermissionNotFound => write!(f, "Permission not found"),
            Self::RoleDuplicate => write!(f, "Role already exists"),
            Self::PermissionDuplicate => write!(f, "Permission already exists"),
            Self::AssignmentNotFound => write!(f, "Assignment not found"),
            Self::AssignmentDuplicate => write!(f, "Assignment already exists"),
            Self::PermissionInUse => write!(f, "Permission has active assignments"),
            Self::QueryError(msg) => write!(f, "Query error: {msg}"),
            Self::TransactionError(msg) => write!(f, "Transaction error: {msg}"),
        }
    }
}

impl std::error::Error for RbacDbError {}

/// The WHERE clause shared by the audit read and its COUNT.
///
/// An `IS NULL OR` arm per filter, every value bound: one statement covers every
/// combination of the endpoint's parameters, and the two queries cannot drift into
/// disagreeing about what "matching" means. Composing this with `format!` over
/// caller input is how #794/#795 were built.
const AUDIT_PREDICATE: &str = "($1::text IS NULL OR user_id = $1)
     AND ($2::uuid IS NULL OR role_id = $2)
     AND ($3::text IS NULL OR event_type = $3)
     AND ($4::uuid IS NULL OR tenant_id = $4)
     AND ($5::timestamptz IS NULL OR occurred_at >= $5)
     AND ($6::timestamptz IS NULL OR occurred_at <= $6)";

/// Parse a caller-supplied UUID, reporting a malformed one as the caller's error.
fn parse_uuid(what: &str, value: &str) -> Result<Uuid, RbacDbError> {
    Uuid::parse_str(value)
        .map_err(|_| RbacDbError::InvalidInput(format!("Invalid {what}: '{value}' is not a UUID")))
}

/// Parse an optional caller-supplied UUID.
fn parse_opt_uuid(what: &str, value: Option<&str>) -> Result<Option<Uuid>, RbacDbError> {
    value.map(|v| parse_uuid(what, v)).transpose()
}

/// Database backend for RBAC operations.
#[derive(Clone)]
pub struct RbacDbBackend {
    pool: PgPool,
}

impl RbacDbBackend {
    /// Create a new RBAC database backend from a connection pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Ensure the RBAC database schema exists.
    ///
    /// Creates all required tables and indexes if they don't already exist.
    /// This operation is idempotent.
    ///
    /// Per-tenant role-name uniqueness is expressed as a **unique index over an
    /// expression**, not as a table-level `UNIQUE` constraint: PostgreSQL accepts
    /// only bare column names inside a table constraint, so the original
    /// `UNIQUE(name, COALESCE(tenant_id, …))` was a parse error that made this DDL —
    /// and therefore the whole server, since boot runs it whenever `admin_token` is
    /// set — fail outright (#748). The `COALESCE` is load-bearing: a plain
    /// `UNIQUE (name, tenant_id)` would let two identically-named global roles
    /// coexist, because NULLs compare distinct.
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
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_fraiseql_roles_name_tenant
                ON fraiseql_roles (
                    name,
                    COALESCE(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid)
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

            CREATE TABLE IF NOT EXISTS fraiseql_permission_audit (
                id UUID PRIMARY KEY,
                occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                event_type TEXT NOT NULL,
                role_id UUID,
                role_name TEXT,
                user_id TEXT,
                permission_id UUID,
                tenant_id UUID,
                details JSONB NOT NULL DEFAULT '{}'::jsonb
            );

            CREATE INDEX IF NOT EXISTS idx_fraiseql_roles_tenant
                ON fraiseql_roles(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_fraiseql_user_roles_user
                ON fraiseql_user_roles(user_id);
            CREATE INDEX IF NOT EXISTS idx_fraiseql_user_roles_role
                ON fraiseql_user_roles(role_id);
            CREATE INDEX IF NOT EXISTS idx_fraiseql_permission_audit_time
                ON fraiseql_permission_audit(occurred_at DESC);
            CREATE INDEX IF NOT EXISTS idx_fraiseql_permission_audit_user
                ON fraiseql_permission_audit(user_id);
            CREATE INDEX IF NOT EXISTS idx_fraiseql_permission_audit_role
                ON fraiseql_permission_audit(role_id);",
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
        let tenant_uuid = parse_opt_uuid("tenant ID", tenant_id)?;

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

        record_audit(
            &mut tx,
            &AuditRecord {
                event_type:    AuditEventType::RoleCreated,
                role_id:       Some(role_id),
                role_name:     Some(name),
                user_id:       None,
                permission_id: None,
                tenant_id:     tenant_uuid,
                details:       serde_json::json!({ "permissions": permissions }),
            },
        )
        .await?;

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
        let role_uuid = parse_uuid("role ID", role_id)?;

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
    /// The returned [`Page`] carries the unpaged `total`, so a caller can never
    /// mistake a truncated page for the whole set — which is exactly what
    /// `GET /api/roles` did while it hard-coded `limit 100, offset 0` (#769).
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::InvalidInput` if `tenant_id` is not a valid UUID, and
    /// `RbacDbError::QueryError` if the query fails.
    pub async fn list_roles(
        &self,
        tenant_id: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Page<RoleDto>, RbacDbError> {
        let tenant_uuid = parse_opt_uuid("tenant ID", tenant_id)?;

        // `$1::uuid IS NULL OR tenant_id = $1` keeps one statement for both the
        // scoped and the global view, so the two cannot drift apart.
        let rows = sqlx::query(
            "SELECT id, name, description, tenant_id, created_at, updated_at
             FROM fraiseql_roles
             WHERE $1::uuid IS NULL OR tenant_id = $1
             ORDER BY name LIMIT $2 OFFSET $3",
        )
        .bind(tenant_uuid)
        .bind(i64::from(limit))
        .bind(i64::from(offset))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM fraiseql_roles WHERE $1::uuid IS NULL OR tenant_id = $1",
        )
        .bind(tenant_uuid)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        let mut roles = Vec::with_capacity(rows.len());
        for row in &rows {
            let id: Uuid = row.get("id");
            let permissions = self.get_role_permissions(id).await?;
            roles.push(role_dto_from_row(row, permissions));
        }
        Ok(Page::new(roles, total, limit, offset))
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
        let role_uuid = parse_uuid("role ID", role_id)?;
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

        record_audit(
            &mut tx,
            &AuditRecord {
                event_type:    AuditEventType::RoleUpdated,
                role_id:       Some(role_uuid),
                role_name:     Some(name),
                user_id:       None,
                permission_id: None,
                tenant_id:     None,
                details:       serde_json::json!({ "permissions": permissions }),
            },
        )
        .await?;

        tx.commit().await.map_err(|e| RbacDbError::TransactionError(e.to_string()))?;

        // Fetch the updated role to get tenant_id and timestamps
        self.get_role(role_id).await
    }

    /// Delete a role by ID (cascades to `role_permissions` and `user_roles`).
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::InvalidInput` if `role_id` is not a valid UUID.
    /// Returns `RbacDbError::RoleNotFound` if no role with the given ID exists.
    /// Returns `RbacDbError::QueryError` if the query fails.
    pub async fn delete_role(&self, role_id: &str) -> Result<(), RbacDbError> {
        let role_uuid = parse_uuid("role ID", role_id)?;

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RbacDbError::ConnectionError(e.to_string()))?;

        // Read the name before the delete so the audit entry names what was removed;
        // after the cascade there is nothing left to describe.
        let name: Option<String> =
            sqlx::query_scalar("SELECT name FROM fraiseql_roles WHERE id = $1")
                .bind(role_uuid)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| RbacDbError::QueryError(e.to_string()))?;
        let Some(name) = name else {
            return Err(RbacDbError::RoleNotFound);
        };

        let result = sqlx::query("DELETE FROM fraiseql_roles WHERE id = $1")
            .bind(role_uuid)
            .execute(&mut *tx)
            .await
            .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(RbacDbError::RoleNotFound);
        }

        record_audit(
            &mut tx,
            &AuditRecord {
                event_type:    AuditEventType::RoleDeleted,
                role_id:       Some(role_uuid),
                role_name:     Some(&name),
                user_id:       None,
                permission_id: None,
                tenant_id:     None,
                details:       serde_json::Value::Object(serde_json::Map::new()),
            },
        )
        .await?;

        tx.commit().await.map_err(|e| RbacDbError::TransactionError(e.to_string()))?;
        Ok(())
    }

    // =========================================================================
    // Permission Operations
    // =========================================================================

    /// Create a new permission.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::PermissionDuplicate` if a permission with the same resource and action
    /// already exists. Returns `RbacDbError::QueryError` if the database insert fails.
    pub async fn create_permission(
        &self,
        resource: &str,
        action: &str,
        description: Option<&str>,
    ) -> Result<PermissionDto, RbacDbError> {
        let perm_id = Uuid::new_v4();
        let now = Utc::now();

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RbacDbError::ConnectionError(e.to_string()))?;

        sqlx::query(
            "INSERT INTO fraiseql_permissions (id, resource, action, description, created_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(perm_id)
        .bind(resource)
        .bind(action)
        .bind(description)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                RbacDbError::PermissionDuplicate
            } else {
                RbacDbError::QueryError(e.to_string())
            }
        })?;

        record_audit(
            &mut tx,
            &AuditRecord {
                event_type:    AuditEventType::PermissionCreated,
                role_id:       None,
                role_name:     None,
                user_id:       None,
                permission_id: Some(perm_id),
                tenant_id:     None,
                details:       serde_json::json!({ "resource": resource, "action": action }),
            },
        )
        .await?;

        tx.commit().await.map_err(|e| RbacDbError::TransactionError(e.to_string()))?;

        Ok(PermissionDto {
            id:          perm_id.to_string(),
            resource:    resource.to_string(),
            action:      action.to_string(),
            description: description.map(String::from),
            created_at:  now.to_rfc3339(),
        })
    }

    /// List permissions, paginated.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::QueryError` if the database query fails.
    pub async fn list_permissions(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<Page<PermissionDto>, RbacDbError> {
        let rows = sqlx::query(
            "SELECT id, resource, action, description, created_at
             FROM fraiseql_permissions ORDER BY resource, action LIMIT $1 OFFSET $2",
        )
        .bind(i64::from(limit))
        .bind(i64::from(offset))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fraiseql_permissions")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        Ok(Page::new(
            rows.iter().map(permission_dto_from_row).collect(),
            total,
            limit,
            offset,
        ))
    }

    /// Get a permission by ID.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::QueryError` if `permission_id` is not a valid UUID or the query fails.
    /// Returns `RbacDbError::PermissionNotFound` if no permission with the given ID exists.
    pub async fn get_permission(&self, permission_id: &str) -> Result<PermissionDto, RbacDbError> {
        let perm_uuid = parse_uuid("permission ID", permission_id)?;

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
    /// Returns `RbacDbError::InvalidInput` if `permission_id` is not a valid UUID.
    /// Returns `RbacDbError::PermissionInUse` if the permission is referenced by one or more roles.
    /// Returns `RbacDbError::PermissionNotFound` if no permission with the given ID exists.
    /// Returns `RbacDbError::QueryError` if the query fails.
    pub async fn delete_permission(&self, permission_id: &str) -> Result<(), RbacDbError> {
        let perm_uuid = parse_uuid("permission ID", permission_id)?;

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RbacDbError::ConnectionError(e.to_string()))?;

        // Check if permission is referenced by any role
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM fraiseql_role_permissions WHERE permission_id = $1",
        )
        .bind(perm_uuid)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        if count > 0 {
            return Err(RbacDbError::PermissionInUse);
        }

        let result = sqlx::query("DELETE FROM fraiseql_permissions WHERE id = $1")
            .bind(perm_uuid)
            .execute(&mut *tx)
            .await
            .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(RbacDbError::PermissionNotFound);
        }

        record_audit(
            &mut tx,
            &AuditRecord {
                event_type:    AuditEventType::PermissionDeleted,
                role_id:       None,
                role_name:     None,
                user_id:       None,
                permission_id: Some(perm_uuid),
                tenant_id:     None,
                details:       serde_json::Value::Object(serde_json::Map::new()),
            },
        )
        .await?;

        tx.commit().await.map_err(|e| RbacDbError::TransactionError(e.to_string()))?;
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
        let role_uuid = parse_uuid("role ID", role_id)?;
        let tenant_uuid = parse_opt_uuid("tenant ID", tenant_id)?;
        let now = Utc::now();

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RbacDbError::ConnectionError(e.to_string()))?;

        // Verify role exists
        let role_name: Option<String> =
            sqlx::query_scalar("SELECT name FROM fraiseql_roles WHERE id = $1")
                .bind(role_uuid)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        let Some(role_name) = role_name else {
            return Err(RbacDbError::RoleNotFound);
        };

        sqlx::query(
            "INSERT INTO fraiseql_user_roles (user_id, role_id, tenant_id, assigned_at)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(user_id)
        .bind(role_uuid)
        .bind(tenant_uuid)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                RbacDbError::AssignmentDuplicate
            } else {
                RbacDbError::QueryError(e.to_string())
            }
        })?;

        record_audit(
            &mut tx,
            &AuditRecord {
                event_type:    AuditEventType::RoleAssigned,
                role_id:       Some(role_uuid),
                role_name:     Some(&role_name),
                user_id:       Some(user_id),
                permission_id: None,
                tenant_id:     tenant_uuid,
                details:       serde_json::Value::Object(serde_json::Map::new()),
            },
        )
        .await?;

        tx.commit().await.map_err(|e| RbacDbError::TransactionError(e.to_string()))?;

        Ok(UserRoleDto {
            user_id:     user_id.to_string(),
            role_id:     role_id.to_string(),
            tenant_id:   tenant_uuid.map(|u| u.to_string()),
            assigned_at: now.to_rfc3339(),
        })
    }

    /// List a user's role assignments, paginated and optionally tenant-scoped.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::InvalidInput` if `tenant_id` is not a valid UUID, and
    /// `RbacDbError::QueryError` if the database query fails.
    pub async fn list_user_roles(
        &self,
        user_id: &str,
        tenant_id: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Page<UserRoleDto>, RbacDbError> {
        let tenant_uuid = parse_opt_uuid("tenant ID", tenant_id)?;

        let rows = sqlx::query(
            "SELECT user_id, role_id, tenant_id, assigned_at
             FROM fraiseql_user_roles
             WHERE user_id = $1 AND ($2::uuid IS NULL OR tenant_id = $2)
             ORDER BY assigned_at LIMIT $3 OFFSET $4",
        )
        .bind(user_id)
        .bind(tenant_uuid)
        .bind(i64::from(limit))
        .bind(i64::from(offset))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM fraiseql_user_roles
             WHERE user_id = $1 AND ($2::uuid IS NULL OR tenant_id = $2)",
        )
        .bind(user_id)
        .bind(tenant_uuid)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        let items = rows
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
            .collect();
        Ok(Page::new(items, total, limit, offset))
    }

    /// Revoke a role from a user.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::InvalidInput` if `role_id` is not a valid UUID.
    /// Returns `RbacDbError::AssignmentNotFound` if the user does not have this role assigned.
    /// Returns `RbacDbError::QueryError` if the query fails.
    pub async fn revoke_role_from_user(
        &self,
        user_id: &str,
        role_id: &str,
    ) -> Result<(), RbacDbError> {
        let role_uuid = parse_uuid("role ID", role_id)?;

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RbacDbError::ConnectionError(e.to_string()))?;

        let result =
            sqlx::query("DELETE FROM fraiseql_user_roles WHERE user_id = $1 AND role_id = $2")
                .bind(user_id)
                .bind(role_uuid)
                .execute(&mut *tx)
                .await
                .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(RbacDbError::AssignmentNotFound);
        }

        record_audit(
            &mut tx,
            &AuditRecord {
                event_type:    AuditEventType::RoleRevoked,
                role_id:       Some(role_uuid),
                role_name:     None,
                user_id:       Some(user_id),
                permission_id: None,
                tenant_id:     None,
                details:       serde_json::Value::Object(serde_json::Map::new()),
            },
        )
        .await?;

        tx.commit().await.map_err(|e| RbacDbError::TransactionError(e.to_string()))?;
        Ok(())
    }

    /// Read recorded permission-change events.
    ///
    /// # Errors
    ///
    /// Returns `RbacDbError::InvalidInput` if `role_id` or `tenant_id` is not a valid
    /// UUID, and `RbacDbError::QueryError` if the database query fails.
    pub async fn query_audit(
        &self,
        filter: &AuditFilter<'_>,
    ) -> Result<Page<AuditEventDto>, RbacDbError> {
        let role_uuid = parse_opt_uuid("role ID", filter.role_id)?;
        let tenant_uuid = parse_opt_uuid("tenant ID", filter.tenant_id)?;

        // One statement for every combination of filters: an `IS NULL OR` arm per
        // predicate, all bound. Composing the WHERE clause with `format!` is how the
        // #794/#795 injection holes were built; there is no reason to do it here.
        let rows = sqlx::query(&format!(
            "SELECT id, occurred_at, event_type, role_id, role_name, user_id,
                    permission_id, tenant_id, details
             FROM fraiseql_permission_audit
             WHERE {AUDIT_PREDICATE}
             ORDER BY occurred_at, id LIMIT $7 OFFSET $8"
        ))
        .bind(filter.user_id)
        .bind(role_uuid)
        .bind(filter.event_type)
        .bind(tenant_uuid)
        .bind(filter.start_time)
        .bind(filter.end_time)
        .bind(i64::from(filter.limit))
        .bind(i64::from(filter.offset))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        let total: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM fraiseql_permission_audit WHERE {AUDIT_PREDICATE}"
        ))
        .bind(filter.user_id)
        .bind(role_uuid)
        .bind(filter.event_type)
        .bind(tenant_uuid)
        .bind(filter.start_time)
        .bind(filter.end_time)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RbacDbError::QueryError(e.to_string()))?;

        let items = rows.iter().map(audit_dto_from_row).collect();
        Ok(Page::new(items, total, filter.limit, filter.offset))
    }

    // =========================================================================
    // Internal Helpers
    // =========================================================================

    /// Get the `"resource:action"` permission strings for a role.
    ///
    /// # Errors
    ///
    /// Returns [`RbacDbError::QueryError`] if the database query fails.
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
    ///
    /// # Errors
    ///
    /// Returns [`RbacDbError::QueryError`] if the SELECT or INSERT query fails.
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
///
/// # Errors
///
/// Returns [`RbacDbError::InvalidInput`] if the string does not contain a `:`
/// separator. It is the caller's mistake, not the database's — mapping it to
/// `409 role_duplicate` is half of #769.
pub(crate) fn parse_permission(perm: &str) -> Result<(&str, &str), RbacDbError> {
    perm.split_once(':').ok_or_else(|| {
        RbacDbError::InvalidInput(format!(
            "Invalid permission format '{perm}': expected 'resource:action'"
        ))
    })
}

// =============================================================================
// Pagination
// =============================================================================

/// One page of results, carrying enough context that a truncated page cannot be
/// mistaken for a complete one.
///
/// `GET /api/roles` used to return a bare JSON array capped at a hard-coded 100
/// with no query parameters: the 101st role existed, was grantable by id, and was
/// invisible (#769). `total` and `has_more` are what make that impossible.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    /// Results on this page.
    pub items:    Vec<T>,
    /// Total matching rows, ignoring `limit`/`offset`.
    pub total:    u64,
    /// The page size that produced `items`.
    pub limit:    u32,
    /// The offset that produced `items`.
    pub offset:   u32,
    /// Whether rows remain beyond this page.
    pub has_more: bool,
}

impl<T> Page<T> {
    /// Build a page, deriving `has_more` from the unpaged total.
    fn new(items: Vec<T>, total: i64, limit: u32, offset: u32) -> Self {
        let total = u64::try_from(total).unwrap_or(0);
        let seen = u64::from(offset).saturating_add(items.len() as u64);
        Self {
            has_more: seen < total,
            items,
            total,
            limit,
            offset,
        }
    }
}

// =============================================================================
// Permission audit trail (#768)
// =============================================================================

/// The permission-changing events the store records.
///
/// Every mutating method on [`RbacDbBackend`] writes exactly one of these **inside
/// its own transaction**, so an audit entry cannot outlive a rolled-back change and
/// a committed change cannot go unrecorded. Before this existed,
/// `GET /api/audit/permissions` returned a hard-coded empty array under an
/// audit-trail claim, which reads to a compliance reviewer as a positive assertion
/// that nothing happened (#768).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuditEventType {
    /// A role was created.
    RoleCreated,
    /// A role's name, description or permission set changed.
    RoleUpdated,
    /// A role was deleted (cascading to its assignments).
    RoleDeleted,
    /// A permission was created directly.
    PermissionCreated,
    /// A permission was deleted.
    PermissionDeleted,
    /// A role was granted to a user.
    RoleAssigned,
    /// A role was revoked from a user.
    RoleRevoked,
}

impl AuditEventType {
    /// Every event type, so a caller (or a test) can enumerate the surface.
    pub const ALL: [Self; 7] = [
        Self::RoleCreated,
        Self::RoleUpdated,
        Self::RoleDeleted,
        Self::PermissionCreated,
        Self::PermissionDeleted,
        Self::RoleAssigned,
        Self::RoleRevoked,
    ];

    /// The stable wire name of this event.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RoleCreated => "role_created",
            Self::RoleUpdated => "role_updated",
            Self::RoleDeleted => "role_deleted",
            Self::PermissionCreated => "permission_created",
            Self::PermissionDeleted => "permission_deleted",
            Self::RoleAssigned => "role_assigned",
            Self::RoleRevoked => "role_revoked",
        }
    }
}

/// A row to append to the audit trail.
struct AuditRecord<'a> {
    event_type:    AuditEventType,
    role_id:       Option<Uuid>,
    role_name:     Option<&'a str>,
    user_id:       Option<&'a str>,
    permission_id: Option<Uuid>,
    tenant_id:     Option<Uuid>,
    details:       serde_json::Value,
}

/// Append an audit row **within the caller's transaction**.
///
/// Taking `&mut Transaction` rather than `&PgPool` is the whole design: it is not
/// possible to record an event for a change that then rolls back, and the compiler
/// makes a mutating method that forgot to open a transaction visible immediately.
async fn record_audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    record: &AuditRecord<'_>,
) -> Result<(), RbacDbError> {
    sqlx::query(
        "INSERT INTO fraiseql_permission_audit
             (id, occurred_at, event_type, role_id, role_name, user_id,
              permission_id, tenant_id, details)
         VALUES ($1, NOW(), $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(Uuid::new_v4())
    .bind(record.event_type.as_str())
    .bind(record.role_id)
    .bind(record.role_name)
    .bind(record.user_id)
    .bind(record.permission_id)
    .bind(record.tenant_id)
    .bind(&record.details)
    .execute(&mut **tx)
    .await
    .map_err(|e| RbacDbError::QueryError(format!("Audit write failed: {e}")))?;
    Ok(())
}

/// Filters for [`RbacDbBackend::query_audit`].
///
/// Every field is a documented query parameter of `GET /api/audit/permissions`. The
/// endpoint used to accept them without extracting them at all.
#[derive(Debug, Clone, Copy)]
pub struct AuditFilter<'a> {
    /// Restrict to events about this subject user.
    pub user_id:    Option<&'a str>,
    /// Restrict to events about this role.
    pub role_id:    Option<&'a str>,
    /// Restrict to one [`AuditEventType::as_str`] value.
    pub event_type: Option<&'a str>,
    /// Restrict to one tenant.
    pub tenant_id:  Option<&'a str>,
    /// Inclusive lower bound on `occurred_at`.
    pub start_time: Option<chrono::DateTime<Utc>>,
    /// Inclusive upper bound on `occurred_at`.
    pub end_time:   Option<chrono::DateTime<Utc>>,
    /// Page size.
    pub limit:      u32,
    /// Page offset.
    pub offset:     u32,
}

/// A recorded permission-change event, as served by `GET /api/audit/permissions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEventDto {
    /// Event identifier.
    pub id:            String,
    /// When the change committed (ISO 8601).
    pub occurred_at:   String,
    /// One of [`AuditEventType::as_str`].
    pub event_type:    String,
    /// The role the event concerns, if any.
    pub role_id:       Option<String>,
    /// The role's name at the time of the event, if known.
    pub role_name:     Option<String>,
    /// The subject user, for assignment events.
    pub user_id:       Option<String>,
    /// The permission the event concerns, if any.
    pub permission_id: Option<String>,
    /// The tenant the event was scoped to, if any.
    pub tenant_id:     Option<String>,
    /// Event-specific payload (e.g. the permission set a role was created with).
    pub details:       serde_json::Value,
}

/// Convert a database row to an [`AuditEventDto`].
fn audit_dto_from_row(row: &PgRow) -> AuditEventDto {
    let id: Uuid = row.get("id");
    let occurred_at: chrono::DateTime<Utc> = row.get("occurred_at");
    let role_id: Option<Uuid> = row.get("role_id");
    let permission_id: Option<Uuid> = row.get("permission_id");
    let tenant_id: Option<Uuid> = row.get("tenant_id");
    AuditEventDto {
        id:            id.to_string(),
        occurred_at:   occurred_at.to_rfc3339(),
        event_type:    row.get("event_type"),
        role_id:       role_id.map(|u| u.to_string()),
        role_name:     row.get("role_name"),
        user_id:       row.get("user_id"),
        permission_id: permission_id.map(|u| u.to_string()),
        tenant_id:     tenant_id.map(|u| u.to_string()),
        details:       row.get("details"),
    }
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
