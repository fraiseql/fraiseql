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

### Step 2: Role Hierarchy (hierarchy.rs)

```rust
//! Role hierarchy computation using PostgreSQL CTEs.

use anyhow::Result;
use uuid::Uuid;
use sqlx::PgPool;
use super::models::Role;

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

use anyhow::Result;
use uuid::Uuid;
use sqlx::PgPool;
use std::sync::Arc;
use super::{
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
//! Multi-layer permission cache (request-level + LRU).

use lru::LruCache;
use std::sync::Mutex;
use std::num::NonZeroUsize;
use uuid::Uuid;
use super::models::Permission;

/// Permission cache with LRU eviction
pub struct PermissionCache {
    cache: Mutex<LruCache<CacheKey, Vec<Permission>>>,
}

#[derive(Hash, Eq, PartialEq, Clone)]
struct CacheKey {
    user_id: Uuid,
    tenant_id: Option<Uuid>,
}

impl PermissionCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(capacity).unwrap())),
        }
    }

    /// Get cached permissions
    pub fn get(&self, user_id: Uuid, tenant_id: Option<Uuid>) -> Option<Vec<Permission>> {
        let key = CacheKey { user_id, tenant_id };
        let mut cache = self.cache.lock().unwrap();
        cache.get(&key).cloned()
    }

    /// Cache permissions
    pub fn set(&self, user_id: Uuid, tenant_id: Option<Uuid>, permissions: Vec<Permission>) {
        let key = CacheKey { user_id, tenant_id };
        let mut cache = self.cache.lock().unwrap();
        cache.put(key, permissions);
    }

    /// Invalidate specific user
    pub fn invalidate_user(&self, user_id: Uuid) {
        let mut cache = self.cache.lock().unwrap();

        // Remove all entries for this user (all tenants)
        let keys_to_remove: Vec<CacheKey> = cache
            .iter()
            .filter(|(k, _)| k.user_id == user_id)
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_remove {
            cache.pop(&key);
        }
    }

    /// Clear entire cache
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.lock().unwrap();
        CacheStats {
            capacity: cache.cap().get(),
            size: cache.len(),
        }
    }
}

#[derive(Debug)]
pub struct CacheStats {
    pub capacity: usize,
    pub size: usize,
}
```

### Step 5: Field-Level Authorization (field_auth.rs)

```rust
//! Field-level authorization enforcement.

use anyhow::{Result, anyhow};
use uuid::Uuid;
use crate::graphql::types::ParsedQuery;
use super::resolver::PermissionResolver;

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
        user_id: Uuid,
        field_name: &str,
        field_permissions: &FieldPermissions,
        tenant_id: Option<Uuid>,
    ) -> Result<()> {
        // Check required roles
        if !field_permissions.required_roles.is_empty() {
            // Get user permissions to extract roles
            let permissions = self.resolver.get_user_permissions(user_id, tenant_id).await?;

            // For role checks, we need to query user roles directly
            // (permissions don't contain role info)
            // This is a simplified version - full implementation would cache roles too

            // For now, assume roles are in UserContext from Phase 10
        }

        // Check required permissions
        for perm in &field_permissions.required_permissions {
            let (resource, action) = parse_permission(perm)?;

            if !self.resolver.has_permission(user_id, &resource, &action, tenant_id).await? {
                return Err(anyhow!("Missing permission: {}", perm));
            }
        }

        Ok(())
    }
}

/// Field permission requirements (from GraphQL directives)
#[derive(Debug, Default)]
pub struct FieldPermissions {
    pub required_roles: Vec<String>,
    pub required_permissions: Vec<String>,
    pub custom_checks: Vec<String>,
}

/// Parse permission string "resource:action"
fn parse_permission(perm: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = perm.split(':').collect();
    if parts.len() != 2 {
        return Err(anyhow!("Invalid permission format: {}", perm));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}
```

### Step 6: GraphQL Directives (directives.rs)

```rust
//! GraphQL directive enforcement (@requiresRole, @requiresPermission).

use crate::graphql::types::{ParsedQuery, Selection};

/// Extract RBAC directives from parsed query
pub struct DirectiveExtractor;

impl DirectiveExtractor {
    /// Extract @requiresRole directives
    pub fn extract_role_requirements(query: &ParsedQuery) -> Vec<String> {
        let mut roles = Vec::new();

        for selection in &query.selections {
            if let Some(field_roles) = Self::extract_field_roles(selection) {
                roles.extend(field_roles);
            }
        }

        roles
    }

    /// Extract @requiresPermission directives
    pub fn extract_permission_requirements(query: &ParsedQuery) -> Vec<String> {
        let mut permissions = Vec::new();

        for selection in &query.selections {
            if let Some(field_perms) = Self::extract_field_permissions(selection) {
                permissions.extend(field_perms);
            }
        }

        permissions
    }

    fn extract_field_roles(selection: &Selection) -> Option<Vec<String>> {
        // Parse GraphQL directives from selection
        // This would inspect the AST for @requiresRole directives
        // Simplified for phase plan
        None
    }

    fn extract_field_permissions(selection: &Selection) -> Option<Vec<String>> {
        // Parse GraphQL directives from selection
        // This would inspect the AST for @requiresPermission directives
        // Simplified for phase plan
        None
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
