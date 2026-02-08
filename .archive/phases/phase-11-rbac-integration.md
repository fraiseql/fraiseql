# Phase 11: RBAC & Permission Resolution in Rust

**Objective**: Move Role-Based Access Control (RBAC), permission resolution, and field-level authorization from Python to Rust for sub-millisecond permission checks.

**Current State**: RBAC implemented in Python with PostgreSQL caching (fraiseql/enterprise/rbac/)

**Target State**: Rust-native RBAC with integrated permission cache, role hierarchy, and field-level auth

---

## Context

**Why This Phase Matters:**
- Permission checks happen on EVERY field access (critical path)
- Role hierarchy computation is expensive in Python
- PostgreSQL cache queries add 0.5-2ms per uncached check
- Rust can reduce permission checks to <0.1ms (cached) and <1ms (uncached)

**Dependencies:**
- Phase 10 (Auth Integration) ✅ Required
- UserContext with roles/permissions from JWT
- PostgreSQL connection pool (Phase 1)

**Performance Target:**
- Cached permission check: <0.1ms
- Uncached permission check: <1ms
- Role hierarchy resolution: <2ms
- Field-level auth overhead: <0.05ms per field

---

## Files to Modify/Create

### Rust Files (fraiseql_rs/src/rbac/)
- **mod.rs** (NEW): RBAC module exports
- **errors.rs** (NEW): RBAC-specific error types
- **models.rs** (NEW): Role, Permission, UserRole models
- **hierarchy.rs** (NEW): Role hierarchy computation with CTEs
- **resolver.rs** (NEW): Permission resolver with caching
- **cache.rs** (NEW): Multi-layer permission cache (request + PostgreSQL)
- **directives.rs** (NEW): GraphQL directive enforcement (@requiresRole, @requiresPermission)
- **field_auth.rs** (NEW): Field-level authorization hooks

### Integration Files
- **fraiseql_rs/src/lib.rs**: Add RBAC module, PyRBAC class
- **fraiseql_rs/src/pipeline/unified.rs**: Integrate RBAC checks in execution
- **src/fraiseql/db.rs**: Keep schema metadata for RBAC tables

### Python Migration Files
- **src/fraiseql/enterprise/rbac/rust_resolver.py** (NEW): Python wrapper
- **src/fraiseql/enterprise/rbac/resolver.py**: Deprecate, redirect to Rust

### Test Files
- **tests/test_rust_rbac.py** (NEW): Integration tests
- **tests/unit/rbac/test_permission_resolution.rs** (NEW): Rust unit tests

---

## Implementation Steps

### Step 1: RBAC Models (models.rs)

```rust
//! RBAC data models matching PostgreSQL schema.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Role entity with hierarchical support
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Role {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub parent_role_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub is_system: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Permission entity
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Permission {
    pub id: Uuid,
    pub resource: String,
    pub action: String,
    pub description: Option<String>,
    pub constraints: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl Permission {
    /// Check if permission matches resource:action pattern
    pub fn matches(&self, resource: &str, action: &str) -> bool {
        // Exact match
        if self.resource == resource && self.action == action {
            return true;
        }

        // Wildcard matching: resource:* or *:action
        if self.action == "*" && self.resource == resource {
            return true;
        }
        if self.resource == "*" && self.action == action {
            return true;
        }
        if self.resource == "*" && self.action == "*" {
            return true;
        }

        false
    }
}

/// User-Role assignment
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserRole {
    pub id: Uuid,
    pub user_id: Uuid,
    pub role_id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub granted_by: Option<Uuid>,
    pub granted_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl UserRole {
    /// Check if role assignment is still valid
    pub fn is_valid(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() < expires_at
        } else {
            true
        }
    }
}

/// Role-Permission mapping
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RolePermission {
    pub id: Uuid,
    pub role_id: Uuid,
    pub permission_id: Uuid,
    pub granted_at: DateTime<Utc>,
}
```

### Step 1.5: RBAC Error Types (errors.rs)

```rust
//! RBAC-specific error types for better error handling.

use std::fmt;

/// Main RBAC error type
#[derive(Debug)]
pub enum RbacError {
    /// Database connection or query errors
    Database(sqlx::Error),

    /// Permission denied for specific resource:action
    PermissionDenied {
        resource: String,
        action: String,
        user_id: Option<String>,
    },

    /// Missing required role
    MissingRole {
        required_role: String,
        available_roles: Vec<String>,
    },

    /// User not found in RBAC system
    UserNotFound(String),

    /// Role not found
    RoleNotFound(String),

    /// Permission not found
    PermissionNotFound(String),

    /// Invalid permission format (expected "resource:action")
    InvalidPermissionFormat(String),

    /// Role hierarchy cycle detected
    HierarchyCycle(Vec<String>),

    /// Cache-related errors
    CacheError(String),

    /// Configuration errors
    ConfigError(String),

    /// GraphQL directive parsing errors
    DirectiveError(String),
}

pub type Result<T> = std::result::Result<T, RbacError>;

impl fmt::Display for RbacError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RbacError::Database(e) => write!(f, "Database error: {}", e),
            RbacError::PermissionDenied { resource, action, user_id } => {
                if let Some(user) = user_id {
                    write!(f, "Permission denied: {}:{} for user {}", resource, action, user)
                } else {
                    write!(f, "Permission denied: {}:{}", resource, action)
                }
            }
            RbacError::MissingRole { required_role, available_roles } => {
                write!(f, "Missing required role '{}'. Available roles: {:?}",
                       required_role, available_roles)
            }
            RbacError::UserNotFound(user_id) => {
                write!(f, "User not found in RBAC system: {}", user_id)
            }
            RbacError::RoleNotFound(role_name) => {
                write!(f, "Role not found: {}", role_name)
            }
            RbacError::PermissionNotFound(perm) => {
                write!(f, "Permission not found: {}", perm)
            }
            RbacError::InvalidPermissionFormat(perm) => {
                write!(f, "Invalid permission format '{}'. Expected 'resource:action'", perm)
            }
            RbacError::HierarchyCycle(roles) => {
                write!(f, "Role hierarchy cycle detected: {:?}", roles)
            }
            RbacError::CacheError(msg) => write!(f, "Cache error: {}", msg),
            RbacError::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
            RbacError::DirectiveError(msg) => write!(f, "Directive parsing error: {}", msg),
        }
    }
}

impl std::error::Error for RbacError {}

impl From<sqlx::Error> for RbacError {
    fn from(error: sqlx::Error) -> Self {
        RbacError::Database(error)
    }
}

impl From<uuid::Error> for RbacError {
    fn from(error: uuid::Error) -> Self {
        RbacError::ConfigError(format!("UUID parsing error: {}", error))
    }
}

/// Convert RBAC errors to Python exceptions
#[cfg(feature = "python")]
impl From<RbacError> for pyo3::PyErr {
    fn from(error: RbacError) -> Self {
        use pyo3::exceptions::*;

        match error {
            RbacError::PermissionDenied { .. } => PyPermissionError::new_err(error.to_string()),
            RbacError::Database(_) => PyRuntimeError::new_err(error.to_string()),
            RbacError::UserNotFound(_) => PyValueError::new_err(error.to_string()),
            _ => PyRuntimeError::new_err(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = RbacError::PermissionDenied {
            resource: "user".to_string(),
            action: "delete".to_string(),
            user_id: Some("user123".to_string()),
        };
        assert!(err.to_string().contains("Permission denied"));
        assert!(err.to_string().contains("user:delete"));
        assert!(err.to_string().contains("user123"));
    }

    #[test]
    fn test_missing_role_error() {
        let err = RbacError::MissingRole {
            required_role: "admin".to_string(),
            available_roles: vec!["user".to_string(), "moderator".to_string()],
        };
        let msg = err.to_string();
        assert!(msg.contains("Missing required role 'admin'"));
        assert!(msg.contains("user"));
        assert!(msg.contains("moderator"));
    }
}
```

### Step 2: Role Hierarchy (hierarchy.rs)

```rust
//! Role hierarchy computation using PostgreSQL CTEs.

use uuid::Uuid;
use sqlx::PgPool;
use super::{errors::Result, models::Role};

/// Role hierarchy resolver using recursive CTEs
pub struct RoleHierarchy {
    pool: PgPool,
}

impl RoleHierarchy {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get all roles in hierarchy (including inherited)
    pub async fn get_all_roles(
        &self,
        role_ids: &[Uuid],
        tenant_id: Option<Uuid>,
    ) -> Result<Vec<Role>> {
        // Use PostgreSQL recursive CTE to traverse hierarchy
        let sql = r#"
            WITH RECURSIVE role_hierarchy AS (
                -- Base case: direct roles
                SELECT r.*
                FROM roles r
                WHERE r.id = ANY($1)
                  AND ($2::uuid IS NULL OR r.tenant_id = $2 OR r.tenant_id IS NULL)

                UNION

                -- Recursive case: parent roles
                SELECT r.*
                FROM roles r
                INNER JOIN role_hierarchy rh ON r.id = rh.parent_role_id
                WHERE $2::uuid IS NULL OR r.tenant_id = $2 OR r.tenant_id IS NULL
            )
            SELECT DISTINCT * FROM role_hierarchy
        "#;

        let roles = sqlx::query_as::<_, Role>(sql)
            .bind(role_ids)
            .bind(tenant_id)
            .fetch_all(&self.pool)
            .await?;

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
                WHERE r.id = $1

                UNION

                -- Recursive case: child roles
                SELECT r.*
                FROM roles r
                INNER JOIN role_children rc ON r.parent_role_id = rc.id
                WHERE $2::uuid IS NULL OR r.tenant_id = $2
            )
            SELECT * FROM role_children WHERE id != $1
        "#;

        let roles = sqlx::query_as::<_, Role>(sql)
            .bind(role_id)
            .bind(tenant_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(roles)
    }
}
```

### Step 3: Permission Resolver (resolver.rs)

```rust
//! Permission resolver with multi-layer caching.

use uuid::Uuid;
use sqlx::PgPool;
use std::sync::Arc;
use super::{
    errors::{Result, RbacError},
    models::{Permission, UserRole},
    hierarchy::RoleHierarchy,
    cache::PermissionCache,
};

/// Permission resolver with caching and hierarchy support
pub struct PermissionResolver {
    pool: PgPool,
    hierarchy: RoleHierarchy,
    cache: Arc<PermissionCache>,
}

impl PermissionResolver {
    pub fn new(pool: PgPool, cache_capacity: usize) -> Self {
        let hierarchy = RoleHierarchy::new(pool.clone());
        let cache = Arc::new(PermissionCache::new(cache_capacity));

        Self {
            pool,
            hierarchy,
            cache,
        }
    }

    /// Get all effective permissions for a user
    pub async fn get_user_permissions(
        &self,
        user_id: Uuid,
        tenant_id: Option<Uuid>,
    ) -> Result<Vec<Permission>> {
        // Try cache first
        if let Some(cached) = self.cache.get(user_id, tenant_id) {
            return Ok(cached);
        }

        // Cache miss - compute from database
        let permissions = self.compute_permissions(user_id, tenant_id).await?;

        // Cache result
        self.cache.set(user_id, tenant_id, permissions.clone());

        Ok(permissions)
    }

    /// Check if user has specific permission
    pub async fn has_permission(
        &self,
        user_id: Uuid,
        resource: &str,
        action: &str,
        tenant_id: Option<Uuid>,
    ) -> Result<bool> {
        let permissions = self.get_user_permissions(user_id, tenant_id).await?;

        Ok(permissions.iter().any(|p| p.matches(resource, action)))
    }

    /// Compute permissions from database
    async fn compute_permissions(
        &self,
        user_id: Uuid,
        tenant_id: Option<Uuid>,
    ) -> Result<Vec<Permission>> {
        // 1. Get user's roles (including expired check)
        let user_roles = self.get_user_roles(user_id, tenant_id).await?;
        let role_ids: Vec<Uuid> = user_roles.iter().map(|ur| ur.role_id).collect();

        if role_ids.is_empty() {
            return Ok(vec![]);
        }

        // 2. Get all roles in hierarchy
        let all_roles = self.hierarchy.get_all_roles(&role_ids, tenant_id).await?;
        let all_role_ids: Vec<Uuid> = all_roles.iter().map(|r| r.id).collect();

        // 3. Get permissions for all roles
        let sql = r#"
            SELECT DISTINCT p.*
            FROM permissions p
            INNER JOIN role_permissions rp ON p.id = rp.permission_id
            WHERE rp.role_id = ANY($1)
            ORDER BY p.resource, p.action
        "#;

        let permissions = sqlx::query_as::<_, Permission>(sql)
            .bind(&all_role_ids)
            .fetch_all(&self.pool)
            .await?;

        Ok(permissions)
    }

    /// Get user's direct role assignments
    async fn get_user_roles(
        &self,
        user_id: Uuid,
        tenant_id: Option<Uuid>,
    ) -> Result<Vec<UserRole>> {
        let sql = r#"
            SELECT *
            FROM user_roles
            WHERE user_id = $1
              AND ($2::uuid IS NULL OR tenant_id = $2)
              AND (expires_at IS NULL OR expires_at > NOW())
        "#;

        let user_roles = sqlx::query_as::<_, UserRole>(sql)
            .bind(user_id)
            .bind(tenant_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(user_roles)
    }

    /// Clear cache for specific user
    pub fn invalidate_user(&self, user_id: Uuid) {
        self.cache.invalidate_user(user_id);
    }

    /// Clear entire cache
    pub fn clear_cache(&self) {
        self.cache.clear();
    }
}
```

### Step 4: Multi-Layer Cache (cache.rs)

```rust
//! Multi-layer permission cache with TTL expiry and LRU eviction.

use lru::LruCache;
use std::sync::Mutex;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};
use uuid::Uuid;
use super::models::Permission;

/// Permission cache with TTL expiry and LRU eviction
pub struct PermissionCache {
    cache: Mutex<LruCache<CacheKey, CacheEntry>>,
    default_ttl: Duration,
}

#[derive(Hash, Eq, PartialEq, Clone)]
struct CacheKey {
    user_id: Uuid,
    tenant_id: Option<Uuid>,
}

#[derive(Clone)]
struct CacheEntry {
    permissions: Vec<Permission>,
    expires_at: Instant,
}

impl PermissionCache {
    /// Create new cache with capacity and default TTL
    pub fn new(capacity: usize) -> Self {
        Self::with_ttl(capacity, Duration::from_secs(300)) // 5 minute default TTL
    }

    /// Create new cache with custom TTL
    pub fn with_ttl(capacity: usize, default_ttl: Duration) -> Self {
        Self {
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(capacity).unwrap())),
            default_ttl,
        }
    }

    /// Get cached permissions (with TTL check)
    pub fn get(&self, user_id: Uuid, tenant_id: Option<Uuid>) -> Option<Vec<Permission>> {
        let key = CacheKey { user_id, tenant_id };
        let mut cache = self.cache.lock().unwrap();

        if let Some(entry) = cache.get(&key) {
            if Instant::now() < entry.expires_at {
                return Some(entry.permissions.clone());
            } else {
                // Entry expired, remove it
                cache.pop(&key);
            }
        }
        None
    }

    /// Cache permissions with default TTL
    pub fn set(&self, user_id: Uuid, tenant_id: Option<Uuid>, permissions: Vec<Permission>) {
        self.set_with_ttl(user_id, tenant_id, permissions, self.default_ttl);
    }

    /// Cache permissions with custom TTL
    pub fn set_with_ttl(
        &self,
        user_id: Uuid,
        tenant_id: Option<Uuid>,
        permissions: Vec<Permission>,
        ttl: Duration,
    ) {
        let key = CacheKey { user_id, tenant_id };
        let entry = CacheEntry {
            permissions,
            expires_at: Instant::now() + ttl,
        };

        let mut cache = self.cache.lock().unwrap();
        cache.put(key, entry);
    }

    /// Invalidate specific user (all tenants)
    pub fn invalidate_user(&self, user_id: Uuid) {
        let mut cache = self.cache.lock().unwrap();

        let keys_to_remove: Vec<CacheKey> = cache
            .iter()
            .filter(|(k, _)| k.user_id == user_id)
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_remove {
            cache.pop(&key);
        }
    }

    /// Invalidate specific tenant (all users)
    pub fn invalidate_tenant(&self, tenant_id: Uuid) {
        let mut cache = self.cache.lock().unwrap();

        let keys_to_remove: Vec<CacheKey> = cache
            .iter()
            .filter(|(k, _)| k.tenant_id == Some(tenant_id))
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_remove {
            cache.pop(&key);
        }
    }

    /// Invalidate specific role (affects all users with this role)
    pub fn invalidate_role(&self, role_id: Uuid) {
        // Since we don't store role info in cache keys, we need to clear
        // potentially affected entries. For now, clear entire cache.
        // Phase 12 could optimize this with reverse index.
        self.clear();
    }

    /// Invalidate specific permission (affects all users with this permission)
    pub fn invalidate_permission(&self, permission_id: Uuid) {
        // Similar to role invalidation - clear entire cache for safety
        // Phase 12 could optimize with permission-based invalidation
        self.clear();
    }

    /// Clear entire cache
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }

    /// Clean expired entries (maintenance operation)
    pub fn cleanup_expired(&self) {
        let mut cache = self.cache.lock().unwrap();
        let now = Instant::now();

        // Remove expired entries
        let keys_to_remove: Vec<CacheKey> = cache
            .iter()
            .filter(|(_, entry)| now >= entry.expires_at)
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_remove {
            cache.pop(&key);
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.lock().unwrap();
        let now = Instant::now();

        let expired_count = cache
            .iter()
            .filter(|(_, entry)| now >= entry.expires_at)
            .count();

        CacheStats {
            capacity: cache.cap().get(),
            size: cache.len(),
            expired_count,
        }
    }
}

#[derive(Debug)]
pub struct CacheStats {
    pub capacity: usize,
    pub size: usize,
    pub expired_count: usize,
}

/// Cache invalidation strategies for RBAC changes
pub struct CacheInvalidation;

impl CacheInvalidation {
    /// Invalidate cache when user role is assigned/revoked
    pub fn on_user_role_change(cache: &PermissionCache, user_id: Uuid) {
        cache.invalidate_user(user_id);
    }

    /// Invalidate cache when role permissions change
    pub fn on_role_permission_change(cache: &PermissionCache, role_id: Uuid) {
        cache.invalidate_role(role_id);
    }

    /// Invalidate cache when user is deleted
    pub fn on_user_deleted(cache: &PermissionCache, user_id: Uuid) {
        cache.invalidate_user(user_id);
    }

    /// Invalidate cache when tenant is deleted
    pub fn on_tenant_deleted(cache: &PermissionCache, tenant_id: Uuid) {
        cache.invalidate_tenant(tenant_id);
    }

    /// Invalidate entire cache (for major RBAC changes)
    pub fn on_major_rbac_change(cache: &PermissionCache) {
        cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_cache_basic_operations() {
        let cache = PermissionCache::new(10);
        let user_id = Uuid::new_v4();

        // Cache should be empty initially
        assert!(cache.get(user_id, None).is_none());

        // Add permissions
        let permissions = vec![Permission {
            id: Uuid::new_v4(),
            resource: "user".to_string(),
            action: "read".to_string(),
            description: None,
            constraints: None,
            created_at: chrono::Utc::now(),
        }];

        cache.set(user_id, None, permissions.clone());

        // Should be able to retrieve
        let cached = cache.get(user_id, None).unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].resource, "user");
    }

    #[test]
    fn test_cache_ttl_expiry() {
        let cache = PermissionCache::with_ttl(10, Duration::from_millis(100));
        let user_id = Uuid::new_v4();

        let permissions = vec![Permission {
            id: Uuid::new_v4(),
            resource: "user".to_string(),
            action: "read".to_string(),
            description: None,
            constraints: None,
            created_at: chrono::Utc::now(),
        }];

        cache.set(user_id, None, permissions);

        // Should be available immediately
        assert!(cache.get(user_id, None).is_some());

        // Wait for expiry
        thread::sleep(Duration::from_millis(150));

        // Should be expired
        assert!(cache.get(user_id, None).is_none());
    }

    #[test]
    fn test_cache_invalidation() {
        let cache = PermissionCache::new(10);
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Add entries for user with different tenants
        let permissions = vec![Permission {
            id: Uuid::new_v4(),
            resource: "user".to_string(),
            action: "read".to_string(),
            description: None,
            constraints: None,
            created_at: chrono::Utc::now(),
        }];

        cache.set(user_id, None, permissions.clone());
        cache.set(user_id, Some(tenant_id), permissions);

        // Both should be present
        assert!(cache.get(user_id, None).is_some());
        assert!(cache.get(user_id, Some(tenant_id)).is_some());

        // Invalidate user
        cache.invalidate_user(user_id);

        // Both should be gone
        assert!(cache.get(user_id, None).is_none());
        assert!(cache.get(user_id, Some(tenant_id)).is_none());
    }
}
```

### Step 5: Field-Level Authorization (field_auth.rs)

```rust
//! Field-level authorization enforcement.

use uuid::Uuid;
use crate::pipeline::unified::UserContext;
use super::{errors::{Result, RbacError}, resolver::PermissionResolver};

/// Field authorization checker
pub struct FieldAuthChecker {
    resolver: PermissionResolver,
}

impl FieldAuthChecker {
    pub fn new(resolver: PermissionResolver) -> Self {
        Self { resolver }
    }

    /// Check field-level permissions before execution
    pub async fn check_field_access(
        &self,
        user_context: &UserContext,
        field_name: &str,
        field_permissions: &FieldPermissions,
        tenant_id: Option<Uuid>,
    ) -> Result<()> {
        // Check required roles (from UserContext - populated by Phase 10 auth)
        if !field_permissions.required_roles.is_empty() {
            let user_roles = &user_context.roles;
            for required_role in &field_permissions.required_roles {
                if !user_roles.contains(required_role) {
                    return Err(RbacError::MissingRole {
                        required_role: required_role.clone(),
                        available_roles: user_roles.clone(),
                    });
                }
            }
        }

        // Check required permissions
        if !field_permissions.required_permissions.is_empty() {
            if let Some(user_id_str) = &user_context.user_id {
                let user_id = Uuid::parse_str(user_id_str)
                    .map_err(|e| RbacError::ConfigError(format!("Invalid user ID in context: {}", e)))?;

                for perm in &field_permissions.required_permissions {
                    let (resource, action) = parse_permission(perm)?;

                    if !self.resolver.has_permission(user_id, &resource, &action, tenant_id).await? {
                        return Err(RbacError::PermissionDenied {
                            resource: resource.clone(),
                            action: action.clone(),
                            user_id: Some(user_id_str.clone()),
                        });
                    }
                }
            } else {
                return Err(RbacError::ConfigError("User context missing user_id for permission check".to_string()));
            }
        }

        // TODO: Implement custom_checks in Phase 12 (advanced constraints)

        Ok(())
    }

    /// Check field access for multiple fields (bulk operation)
    pub async fn check_fields_access(
        &self,
        user_context: &UserContext,
        fields: &[(&str, &FieldPermissions)],
        tenant_id: Option<Uuid>,
    ) -> Result<()> {
        for (field_name, field_permissions) in fields {
            self.check_field_access(user_context, field_name, field_permissions, tenant_id).await?;
        }
        Ok(())
    }
}

/// Field permission requirements (from GraphQL directives)
#[derive(Debug, Default, Clone)]
pub struct FieldPermissions {
    pub required_roles: Vec<String>,
    pub required_permissions: Vec<String>,
    pub custom_checks: Vec<String>,  // For Phase 12 advanced constraints
}

impl FieldPermissions {
    /// Check if any permissions are required
    pub fn has_requirements(&self) -> bool {
        !self.required_roles.is_empty() ||
        !self.required_permissions.is_empty() ||
        !self.custom_checks.is_empty()
    }

    /// Merge permissions (for nested field requirements)
    pub fn merge(&mut self, other: &FieldPermissions) {
        self.required_roles.extend(other.required_roles.iter().cloned());
        self.required_permissions.extend(other.required_permissions.iter().cloned());
        self.custom_checks.extend(other.custom_checks.iter().cloned());

        // Remove duplicates
        self.required_roles.sort();
        self.required_roles.dedup();
        self.required_permissions.sort();
        self.required_permissions.dedup();
        self.custom_checks.sort();
        self.custom_checks.dedup();
    }
}

/// Parse permission string "resource:action"
fn parse_permission(perm: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = perm.split(':').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(RbacError::InvalidPermissionFormat(perm.to_string()));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_permission_valid() {
        assert_eq!(parse_permission("user:read").unwrap(), ("user".to_string(), "read".to_string()));
        assert_eq!(parse_permission("post:create").unwrap(), ("post".to_string(), "create".to_string()));
    }

    #[test]
    fn test_parse_permission_invalid() {
        assert!(parse_permission("invalid").is_err());
        assert!(parse_permission("user:").is_err());
        assert!(parse_permission(":read").is_err());
        assert!(parse_permission("").is_err());
    }

    #[test]
    fn test_field_permissions_merge() {
        let mut fp1 = FieldPermissions {
            required_roles: vec!["admin".to_string()],
            required_permissions: vec!["user:read".to_string()],
            custom_checks: vec!["age_check".to_string()],
        };

        let fp2 = FieldPermissions {
            required_roles: vec!["admin".to_string(), "moderator".to_string()],
            required_permissions: vec!["user:write".to_string()],
            custom_checks: vec!["age_check".to_string(), "region_check".to_string()],
        };

        fp1.merge(&fp2);

        assert_eq!(fp1.required_roles, vec!["admin", "moderator"]);
        assert_eq!(fp1.required_permissions, vec!["user:read", "user:write"]);
        assert_eq!(fp1.custom_checks, vec!["age_check", "region_check"]);
    }
}
```

### Step 6: GraphQL Directives (directives.rs)

```rust
//! GraphQL directive enforcement (@requiresRole, @requiresPermission).

use graphql_parser::query::{Directive, Value};
use crate::graphql::types::{ParsedQuery, FieldSelection};
use super::{errors::{Result, RbacError}, field_auth::FieldPermissions};

/// Extract RBAC directives from parsed query
pub struct DirectiveExtractor;

impl DirectiveExtractor {
    /// Extract all field permissions from parsed query
    pub fn extract_field_permissions(query: &ParsedQuery) -> Result<Vec<(String, FieldPermissions)>> {
        let mut field_permissions = Vec::new();

        for selection in &query.selections {
            Self::extract_from_selection(selection, &mut field_permissions, Vec::new())?;
        }

        Ok(field_permissions)
    }

    /// Recursively extract permissions from field selection
    fn extract_from_selection(
        selection: &FieldSelection,
        permissions: &mut Vec<(String, FieldPermissions)>,
        path: Vec<String>,
    ) -> Result<()> {
        let mut current_path = path;
        current_path.push(selection.name.clone());

        // Extract directives for this field
        let field_perms = Self::extract_directives(&selection.directives)?;
        if field_perms.has_requirements() {
            let field_path = current_path.join(".");
            permissions.push((field_path, field_perms));
        }

        // Recursively process nested fields
        for nested in &selection.nested_fields {
            Self::extract_from_selection(nested, permissions, current_path.clone())?;
        }

        Ok(())
    }

    /// Parse directives into FieldPermissions
    fn extract_directives(directives: &[String]) -> Result<FieldPermissions> {
        let mut permissions = FieldPermissions::default();

        // Note: Current FieldSelection.directives only contains names.
        // This is a simplified implementation. Full implementation would need
        // to extend the GraphQL parser to capture directive arguments.

        // For Phase 11, we'll implement a basic version that assumes
        // directives are applied at schema level, not query level.
        // Phase 12 will add full directive parsing with arguments.

        for directive in directives {
            match directive.as_str() {
                "requiresRole" => {
                    // TODO: Parse role argument from directive
                    // For now, this is a placeholder for schema-level directives
                    // In full implementation: @requiresRole(role: "admin")
                    return Err(RbacError::DirectiveError(
                        "requiresRole directive parsing not implemented yet".to_string()
                    ));
                }
                "requiresPermission" => {
                    // TODO: Parse permission argument from directive
                    // For now, this is a placeholder for schema-level directives
                    // In full implementation: @requiresPermission(permission: "user:read")
                    return Err(RbacError::DirectiveError(
                        "requiresPermission directive parsing not implemented yet".to_string()
                    ));
                }
                _ => {
                    // Ignore other directives (like @include, @skip)
                }
            }
        }

        Ok(permissions)
    }
}

/// Extended GraphQL parsing for directive arguments (Phase 12)
/// This will replace the simplified version above
#[allow(dead_code)]
mod extended_parsing {
    use super::*;

    /// Full directive parsing with arguments
    pub fn parse_directive_arguments(directive: &Directive<String>) -> Result<FieldPermissions> {
        let mut permissions = FieldPermissions::default();

        match directive.name.as_str() {
            "requiresRole" => {
                if let Some(role_arg) = find_argument(&directive.arguments, "role") {
                    if let Value::String(role) = role_arg {
                        permissions.required_roles.push(role.clone());
                    }
                }
            }
            "requiresPermission" => {
                if let Some(perm_arg) = find_argument(&directive.arguments, "permission") {
                    if let Value::String(permission) = perm_arg {
                        permissions.required_permissions.push(permission.clone());
                    }
                }
            }
            _ => {}
        }

        Ok(permissions)
    }

    /// Find argument by name in directive arguments
    fn find_argument<'a>(arguments: &'a [(String, Value<String>)], name: &str) -> Option<&'a Value<String>> {
        arguments.iter()
            .find(|(arg_name, _)| arg_name == name)
            .map(|(_, value)| value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_field_permissions_empty() {
        let query = ParsedQuery {
            operation_type: "query".to_string(),
            operation_name: None,
            root_field: "users".to_string(),
            selections: vec![FieldSelection {
                name: "users".to_string(),
                alias: None,
                arguments: vec![],
                nested_fields: vec![],
                directives: vec![],
            }],
            variables: vec![],
            source: "query { users }".to_string(),
        };

        let permissions = DirectiveExtractor::extract_field_permissions(&query).unwrap();
        assert!(permissions.is_empty());
    }

    #[test]
    fn test_extract_field_permissions_nested() {
        let query = ParsedQuery {
            operation_type: "query".to_string(),
            operation_name: None,
            root_field: "users".to_string(),
            selections: vec![FieldSelection {
                name: "users".to_string(),
                alias: None,
                arguments: vec![],
                nested_fields: vec![FieldSelection {
                    name: "email".to_string(),
                    alias: None,
                    arguments: vec![],
                    nested_fields: vec![],
                    directives: vec!["requiresPermission".to_string()],
                }],
                directives: vec![],
            }],
            variables: vec![],
            source: "query { users { email @requiresPermission } }".to_string(),
        };

        let permissions = DirectiveExtractor::extract_field_permissions(&query).unwrap();
        assert_eq!(permissions.len(), 1);
        assert_eq!(permissions[0].0, "users.email");
        // Note: Actual permission extraction is stubbed for Phase 11
    }
}
```

### Step 7: Integration with Pipeline (unified.rs)

```rust
// Add RBAC checks to execute_sync()

use crate::rbac::{resolver::PermissionResolver, field_auth::FieldAuthChecker};

pub struct GraphQLPipeline {
    schema: SchemaMetadata,
    cache: Arc<QueryPlanCache>,
    rbac_resolver: Option<Arc<PermissionResolver>>,  // NEW
}

impl GraphQLPipeline {
    pub fn with_rbac(mut self, pool: PgPool, cache_capacity: usize) -> Self {
        self.rbac_resolver = Some(Arc::new(PermissionResolver::new(pool, cache_capacity)));
        self
    }

    pub fn execute_sync(
        &self,
        query_string: &str,
        variables: HashMap<String, JsonValue>,
        user_context: UserContext,
        auth_required: bool,
    ) -> Result<Vec<u8>> {
        // Phase 10: Auth check
        if auth_required && user_context.user_id.is_none() {
            return Err(anyhow!("Authentication required"));
        }

        // Phase 6: Parse GraphQL query
        let parsed_query = crate::graphql::parser::parse_query(query_string)?;

        // Phase 11: RBAC permission checks (NEW)
        if let Some(rbac) = &self.rbac_resolver {
            if let Some(user_id_str) = &user_context.user_id {
                let user_id = Uuid::parse_str(user_id_str)?;

                // Extract directive requirements
                let required_permissions = DirectiveExtractor::extract_permission_requirements(&parsed_query);

                // Check permissions
                for perm in required_permissions {
                    let (resource, action) = parse_permission(&perm)?;
                    if !rbac.has_permission(user_id, &resource, &action, None).await? {
                        return Err(anyhow!("Permission denied: {}", perm));
                    }
                }
            }
        }

        // Phase 7 + 8: Build SQL (with caching)
        // ... rest of pipeline ...
    }
}
```

### Step 8: Python Wrapper (rust_resolver.py)

```python
"""Rust-based RBAC resolver (Python wrapper)."""

from uuid import UUID

from fraiseql._fraiseql_rs import PyPermissionResolver, PyPermission
from fraiseql.enterprise.rbac.models import Permission


class RustPermissionResolver:
    """Permission resolver using Rust implementation.

    This is 10-100x faster than Python implementation.
    """

    def __init__(self, pool):
        """Initialize with database pool."""
        self._rust_resolver = PyPermissionResolver(pool, cache_capacity=10000)

    async def get_user_permissions(
        self, user_id: UUID, tenant_id: UUID | None = None
    ) -> list[Permission]:
        """Get all effective permissions for user."""
        rust_perms = await self._rust_resolver.get_user_permissions(
            str(user_id), str(tenant_id) if tenant_id else None
        )

        return [
            Permission(
                id=p.id,
                resource=p.resource,
                action=p.action,
                description=p.description,
                constraints=p.constraints,
                created_at=p.created_at,
            )
            for p in rust_perms
        ]

    async def has_permission(
        self,
        user_id: UUID,
        resource: str,
        action: str,
        tenant_id: UUID | None = None,
    ) -> bool:
        """Check if user has specific permission."""
        return await self._rust_resolver.has_permission(
            str(user_id), resource, action, str(tenant_id) if tenant_id else None
        )

    def invalidate_user(self, user_id: UUID):
        """Invalidate cache for specific user."""
        self._rust_resolver.invalidate_user(str(user_id))

    def clear_cache(self):
        """Clear entire permission cache."""
        self._rust_resolver.clear_cache()
```

### Step 9: PyO3 Bindings (lib.rs)

```rust
// Add to lib.rs

#[pyclass]
pub struct PyPermissionResolver {
    resolver: Arc<rbac::resolver::PermissionResolver>,
}

#[pymethods]
impl PyPermissionResolver {
    #[new]
    pub fn new(pool: Py<db::pool::DatabasePool>, cache_capacity: usize) -> PyResult<Self> {
        Python::with_gil(|py| {
            let rust_pool = pool.borrow(py).pool.clone();
            Ok(Self {
                resolver: Arc::new(rbac::resolver::PermissionResolver::new(
                    rust_pool,
                    cache_capacity,
                )),
            })
        })
    }

    /// Get user permissions
    pub fn get_user_permissions(
        &self,
        py: Python,
        user_id: String,
        tenant_id: Option<String>,
    ) -> PyResult<PyObject> {
        let resolver = self.resolver.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let user_uuid = Uuid::parse_str(&user_id)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

            let tenant_uuid = tenant_id
                .map(|t| Uuid::parse_str(&t))
                .transpose()
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

            let permissions = resolver.get_user_permissions(user_uuid, tenant_uuid)
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            // Convert to Python objects
            Ok(permissions)
        })
    }

    /// Check specific permission
    pub fn has_permission(
        &self,
        py: Python,
        user_id: String,
        resource: String,
        action: String,
        tenant_id: Option<String>,
    ) -> PyResult<PyObject> {
        let resolver = self.resolver.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let user_uuid = Uuid::parse_str(&user_id)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

            let tenant_uuid = tenant_id
                .map(|t| Uuid::parse_str(&t))
                .transpose()
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

            let has_perm = resolver.has_permission(user_uuid, &resource, &action, tenant_uuid)
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            Ok(has_perm)
        })
    }

    /// Invalidate user cache
    pub fn invalidate_user(&self, user_id: String) -> PyResult<()> {
        let user_uuid = Uuid::parse_str(&user_id)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

        self.resolver.invalidate_user(user_uuid);
        Ok(())
    }

    /// Clear entire cache
    pub fn clear_cache(&self) {
        self.resolver.clear_cache();
    }
}

// Add to module registration
fn fraiseql_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // ... existing exports ...

    m.add_class::<PyPermissionResolver>()?;

    Ok(())
}
```

---

## Verification Commands

### Build and Test
```bash
# Build Rust extension
cargo build --release
maturin develop --release

# Run RBAC tests
pytest tests/test_rust_rbac.py -xvs

# Run existing RBAC tests (should pass with Rust implementation)
pytest tests/integration/enterprise/rbac/ -xvs

# Performance benchmarks
pytest tests/performance/test_rbac_performance.py -xvs
```

### Expected Performance
```
Before (Python):
- Uncached permission check: 2-5ms
- Cached (PostgreSQL): 0.5-1ms
- Role hierarchy: 5-10ms

After (Rust):
- Uncached permission check: <1ms
- Cached (LRU): <0.1ms
- Role hierarchy: <2ms

Improvement: 10-100x faster
```

---

## Acceptance Criteria

**Functionality:**
- ✅ Role hierarchy resolution with recursive CTEs
- ✅ Permission resolution with caching
- ✅ Field-level authorization enforcement
- ✅ GraphQL directive support (@requiresRole, @requiresPermission)
- ✅ Multi-tenant permission isolation
- ✅ Cache invalidation on RBAC changes

**Performance:**
- ✅ Cached permission check: <0.1ms
- ✅ Uncached permission check: <1ms
- ✅ 10-100x faster than Python
- ✅ Cache hit rate >95%

**Testing:**
- ✅ All existing RBAC tests pass
- ✅ Rust unit tests for hierarchy and resolution
- ✅ Integration tests for field-level auth
- ✅ Performance benchmarks
- ✅ Cache invalidation tests

**Quality:**
- ✅ No compilation warnings
- ✅ Thread-safe caching
- ✅ Proper error handling
- ✅ Documentation

---

## DO NOT

❌ **DO NOT** implement UI/management APIs (keep in Python)
❌ **DO NOT** add complex constraint evaluation (defer to Phase 12)
❌ **DO NOT** implement audit logging here (Phase 12)
❌ **DO NOT** change RBAC database schema
❌ **DO NOT** add new RBAC features - only migrate existing

---

## Dependencies (Cargo.toml)

```toml
[dependencies]
# Existing...

# RBAC dependencies (Phase 11)
uuid = { version = "1.6", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
lru = "0.12"
# Note: Using custom RbacError type instead of thiserror for better control
```

---

## Migration Strategy

**Week 1: Core RBAC**
- Implement models, hierarchy, resolver
- Add caching layer
- Python wrapper

**Week 2: Field-Level Auth**
- Directive enforcement
- Integration with pipeline
- Testing

**Week 3: Production**
- Gradual rollout
- Monitor performance
- Deprecate Python RBAC

---

## Next Phase Preview

**Phase 12** will add:
- Rate limiting in Rust
- Security headers enforcement
- Audit logging
- Advanced constraint evaluation
