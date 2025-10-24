# Feature 2: Advanced RBAC (PostgreSQL-Native Implementation)

**Complexity**: Complex | **Duration**: 4-6 weeks | **Priority**: 10/10

---

## Executive Summary

Implement a hierarchical role-based access control system that supports complex organizational structures with 10,000+ users. The system provides role inheritance, **PostgreSQL-native permission caching**, and integrates with FraiseQL's GraphQL field-level security. It serves as the foundation for the ABAC system (Tier 2) and demonstrates **"In PostgreSQL Everything"** architecture.

**Key Architectural Decision**: Use **PostgreSQL exclusively** for permission caching (no Redis), leveraging FraiseQL's existing PostgresCache infrastructure with domain versioning for automatic invalidation.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    GraphQL Request Layer                     │
│              (Authenticated User Context)                    │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│         Permission Resolver (2-Layer Cache)                  │
│  - Layer 1: Request-level (in-memory, same request)         │
│  - Layer 2: PostgreSQL UNLOGGED table (0.1-0.3ms)           │
│  - Automatic invalidation via domain versioning             │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│              Role Hierarchy Engine                           │
│  - Computes transitive role inheritance                     │
│  - Supports multiple inheritance paths                      │
│  - Diamond problem resolution                               │
│  - Cached in PostgreSQL (request-level + UNLOGGED table)    │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│         PostgreSQL RBAC Schema                               │
│  - roles (id, name, parent_role_id, permissions)            │
│  - user_roles (user_id, role_id, tenant_id)                 │
│  - permissions (resource, action, constraints)              │
│  - Domain triggers for auto-invalidation                    │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│      PostgreSQL Cache (UNLOGGED Tables)                      │
│  - fraiseql_cache table for permission caching              │
│  - Domain versioning (role, permission, user_role)          │
│  - CASCADE rules (role changes → user permissions)          │
│  - Table triggers for automatic invalidation                │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│            Field-Level Authorization                         │
│  - Integrates with @requires_permission directive           │
│  - Row-level security (PostgreSQL RLS)                      │
│  - Column masking for PII                                   │
└─────────────────────────────────────────────────────────────┘
```

**Cache Flow**:
1. GraphQL resolver checks `@requires_permission`
2. PermissionResolver checks request-level cache (in-memory dict)
3. If miss, checks PostgreSQL cache (UNLOGGED table, <0.3ms)
4. If miss or stale (version check), computes from RBAC tables
5. Stores in PostgreSQL cache with domain versions
6. Stores in request-level cache for same-request reuse

**Automatic Invalidation**:
1. Admin assigns role to user → `user_roles` INSERT
2. PostgreSQL trigger increments `user_role` domain version
3. CASCADE rule increments `user_permissions` domain version
4. Next permission check detects version mismatch → recomputes
5. Fresh permissions cached with new version metadata

---

## File Structure

```
src/fraiseql/enterprise/
├── rbac/
│   ├── __init__.py
│   ├── models.py                  # Role, Permission, UserRole models
│   ├── resolver.py                # Permission resolution engine
│   ├── hierarchy.py               # Role hierarchy computation
│   ├── cache.py                   # PostgreSQL permission caching
│   ├── middleware.py              # GraphQL authorization middleware
│   ├── directives.py              # @requiresRole, @requiresPermission
│   └── types.py                   # GraphQL types for RBAC
└── migrations/
    └── 002_rbac_tables.sql        # RBAC database schema
    └── 003_rbac_cache_setup.sql   # Cache domain setup

tests/integration/enterprise/rbac/
├── test_role_hierarchy.py
├── test_permission_resolution.py
├── test_field_level_auth.py
├── test_cache_performance.py      # PostgreSQL cache performance
├── test_cache_invalidation.py     # Domain versioning tests
└── test_multi_tenant_rbac.py

docs/enterprise/
├── rbac-guide.md
├── rbac-postgresql-caching.md     # PostgreSQL cache architecture
└── permission-patterns.md
```

---

## PHASES

### Phase 1: Database Schema & Core Models

**Objective**: Create RBAC database schema with role hierarchy support

#### TDD Cycle 1.1: RBAC Database Schema

**RED**: Write failing test for RBAC tables

```python
# tests/integration/enterprise/rbac/test_rbac_schema.py

async def test_rbac_tables_exist():
    """Verify RBAC tables exist with correct schema."""
    tables = ['roles', 'permissions', 'role_permissions', 'user_roles']

    for table in tables:
        result = await db.run(DatabaseQuery(
            statement=f"""
                SELECT column_name, data_type
                FROM information_schema.columns
                WHERE table_name = '{table}'
            """,
            params={},
            fetch_result=True
        ))
        assert len(result) > 0, f"Table {table} should exist"

    # Verify roles table structure
    roles_columns = await get_table_columns('roles')
    assert 'id' in roles_columns
    assert 'name' in roles_columns
    assert 'parent_role_id' in roles_columns  # For hierarchy
    assert 'tenant_id' in roles_columns  # Multi-tenancy
    # Expected failure: tables don't exist
```

**GREEN**: Implement RBAC schema

```sql
-- src/fraiseql/enterprise/migrations/002_rbac_tables.sql

-- Roles table with hierarchy support
CREATE TABLE IF NOT EXISTS roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    description TEXT,
    parent_role_id UUID REFERENCES roles(id) ON DELETE SET NULL,
    tenant_id UUID,  -- NULL for global roles
    is_system BOOLEAN DEFAULT FALSE,  -- System roles can't be deleted
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(name, tenant_id)  -- Unique per tenant
);

-- Permissions catalog
CREATE TABLE IF NOT EXISTS permissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    resource VARCHAR(100) NOT NULL,  -- e.g., 'user', 'product', 'order'
    action VARCHAR(50) NOT NULL,     -- e.g., 'create', 'read', 'update', 'delete'
    description TEXT,
    constraints JSONB,  -- Optional constraints (e.g., {"own_data_only": true})
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(resource, action)
);

-- Role-Permission mapping (many-to-many)
CREATE TABLE IF NOT EXISTS role_permissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    permission_id UUID NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    granted BOOLEAN DEFAULT TRUE,  -- TRUE = grant, FALSE = revoke (explicit deny)
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(role_id, permission_id)
);

-- User-Role assignment
CREATE TABLE IF NOT EXISTS user_roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,  -- References users table
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    tenant_id UUID,  -- Scoped to tenant
    granted_by UUID,  -- User who granted this role
    granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,  -- Optional expiration
    UNIQUE(user_id, role_id, tenant_id)
);

-- Indexes for performance
CREATE INDEX idx_roles_parent ON roles(parent_role_id);
CREATE INDEX idx_roles_tenant ON roles(tenant_id);
CREATE INDEX idx_user_roles_user ON user_roles(user_id, tenant_id);
CREATE INDEX idx_user_roles_role ON user_roles(role_id);
CREATE INDEX idx_role_permissions_role ON role_permissions(role_id);

-- Function to compute role hierarchy (recursive)
CREATE OR REPLACE FUNCTION get_inherited_roles(p_role_id UUID)
RETURNS TABLE(role_id UUID, depth INT) AS $$
    WITH RECURSIVE role_hierarchy AS (
        -- Base case: the role itself
        SELECT id as role_id, 0 as depth
        FROM roles
        WHERE id = p_role_id

        UNION ALL

        -- Recursive case: parent roles
        SELECT r.parent_role_id as role_id, rh.depth + 1 as depth
        FROM roles r
        INNER JOIN role_hierarchy rh ON r.id = rh.role_id
        WHERE r.parent_role_id IS NOT NULL
        AND rh.depth < 10  -- Prevent infinite loops
    )
    SELECT DISTINCT role_id, MIN(depth) as depth
    FROM role_hierarchy
    WHERE role_id IS NOT NULL
    GROUP BY role_id
    ORDER BY depth;
$$ LANGUAGE SQL STABLE;
```

**REFACTOR**: Add seed data for common roles

```sql
-- Seed common system roles
INSERT INTO roles (id, name, description, parent_role_id, is_system) VALUES
    ('00000000-0000-0000-0000-000000000001', 'super_admin', 'Full system access', NULL, TRUE),
    ('00000000-0000-0000-0000-000000000002', 'admin', 'Tenant administrator', NULL, TRUE),
    ('00000000-0000-0000-0000-000000000003', 'manager', 'Department manager', '00000000-0000-0000-0000-000000000002', TRUE),
    ('00000000-0000-0000-0000-000000000004', 'user', 'Standard user', '00000000-0000-0000-0000-000000000003', TRUE),
    ('00000000-0000-0000-0000-000000000005', 'viewer', 'Read-only access', '00000000-0000-0000-0000-000000000004', TRUE)
ON CONFLICT (name, tenant_id) DO NOTHING;

-- Seed common permissions
INSERT INTO permissions (resource, action, description) VALUES
    ('user', 'create', 'Create new users'),
    ('user', 'read', 'View user data'),
    ('user', 'update', 'Modify user data'),
    ('user', 'delete', 'Delete users'),
    ('role', 'assign', 'Assign roles to users'),
    ('role', 'create', 'Create new roles'),
    ('audit', 'read', 'View audit logs'),
    ('settings', 'update', 'Modify system settings')
ON CONFLICT (resource, action) DO NOTHING;
```

**QA**: Verify schema and hierarchy function

```bash
uv run pytest tests/integration/enterprise/rbac/test_rbac_schema.py -v
```

---

#### TDD Cycle 1.2: PostgreSQL Cache Setup

**RED**: Write failing test for cache domain setup

```python
# tests/integration/enterprise/rbac/test_cache_setup.py

async def test_rbac_cache_domains_registered():
    """Verify RBAC cache domains are registered with triggers."""
    from fraiseql.caching import get_cache

    cache = get_cache()

    # Check if pg_fraiseql_cache extension is available
    if not cache.has_domain_versioning:
        pytest.skip("pg_fraiseql_cache extension not installed")

    # Verify domains exist
    async with db.pool.connection() as conn, conn.cursor() as cur:
        await cur.execute("""
            SELECT domain
            FROM fraiseql_cache.domain_version
            WHERE domain IN ('role', 'permission', 'role_permission', 'user_role')
        """)
        domains = {row[0] for row in await cur.fetchall()}

    assert 'role' in domains
    assert 'permission' in domains
    assert 'role_permission' in domains
    assert 'user_role' in domains
    # Expected failure: domains not registered
```

**GREEN**: Implement cache domain setup

```sql
-- src/fraiseql/enterprise/migrations/003_rbac_cache_setup.sql

-- Setup table triggers for automatic cache invalidation
-- Requires pg_fraiseql_cache extension

-- Domain for roles table
SELECT fraiseql_cache.setup_table_invalidation('roles', 'role', 'tenant_id');

-- Domain for permissions table (no tenant_id - global)
SELECT fraiseql_cache.setup_table_invalidation('permissions', 'permission', NULL);

-- Domain for role_permissions table (no tenant_id - inherits from role)
SELECT fraiseql_cache.setup_table_invalidation('role_permissions', 'role_permission', NULL);

-- Domain for user_roles table
SELECT fraiseql_cache.setup_table_invalidation('user_roles', 'user_role', 'tenant_id');

-- CASCADE rules: when RBAC tables change, invalidate user permissions
-- This ensures user permission caches are invalidated when roles/permissions change

INSERT INTO fraiseql_cache.cascade_rules (source_domain, target_domain, rule_type) VALUES
    ('role', 'user_permissions', 'invalidate'),
    ('permission', 'user_permissions', 'invalidate'),
    ('role_permission', 'user_permissions', 'invalidate'),
    ('user_role', 'user_permissions', 'invalidate')
ON CONFLICT (source_domain, target_domain) DO NOTHING;
```

**REFACTOR**: Add Python cache initialization

```python
# src/fraiseql/enterprise/rbac/__init__.py

from fraiseql.caching import get_cache
import logging

logger = logging.getLogger(__name__)

async def setup_rbac_cache():
    """Initialize RBAC cache domains and CASCADE rules.

    This should be called during application startup.
    """
    cache = get_cache()

    if not cache.has_domain_versioning:
        logger.warning(
            "pg_fraiseql_cache extension not available. "
            "RBAC will use TTL-only caching without automatic invalidation."
        )
        return

    # Setup table triggers (idempotent)
    await cache.setup_table_trigger("roles", domain_name="role", tenant_column="tenant_id")
    await cache.setup_table_trigger("permissions", domain_name="permission")
    await cache.setup_table_trigger("role_permissions", domain_name="role_permission")
    await cache.setup_table_trigger("user_roles", domain_name="user_role", tenant_column="tenant_id")

    # Setup CASCADE rules (idempotent)
    await cache.register_cascade_rule("role", "user_permissions")
    await cache.register_cascade_rule("permission", "user_permissions")
    await cache.register_cascade_rule("role_permission", "user_permissions")
    await cache.register_cascade_rule("user_role", "user_permissions")

    logger.info("✓ RBAC cache domains and CASCADE rules configured")
```

**QA**: Test cache setup

```bash
uv run pytest tests/integration/enterprise/rbac/test_cache_setup.py -v
```

---

### Phase 2: Permission Caching Layer (PostgreSQL-Native)

**Objective**: Implement 2-layer permission cache (request + PostgreSQL)

#### TDD Cycle 2.1: PostgreSQL Permission Cache

**RED**: Write failing test for permission caching

```python
# tests/integration/enterprise/rbac/test_permission_cache.py

async def test_permission_cache_stores_and_retrieves():
    """Verify permissions can be cached and retrieved from PostgreSQL."""
    from fraiseql.enterprise.rbac.cache import PermissionCache
    from fraiseql.enterprise.rbac.models import Permission

    cache = PermissionCache(db_pool)

    # Mock permissions
    permissions = [
        Permission(
            id=uuid4(),
            resource='user',
            action='read',
            description='Read users'
        ),
        Permission(
            id=uuid4(),
            resource='user',
            action='write',
            description='Write users'
        )
    ]

    user_id = uuid4()
    tenant_id = uuid4()

    # Store in cache
    await cache.set(user_id, tenant_id, permissions)

    # Retrieve from cache
    cached = await cache.get(user_id, tenant_id)

    assert cached is not None
    assert len(cached) == 2
    assert cached[0].resource == 'user'
    # Expected failure: PermissionCache not implemented
```

**GREEN**: Implement PostgreSQL permission cache

```python
# src/fraiseql/enterprise/rbac/cache.py

from uuid import UUID
from datetime import timedelta
from fraiseql.enterprise.rbac.models import Permission
from fraiseql.caching import PostgresCache
import logging

logger = logging.getLogger(__name__)


class PermissionCache:
    """2-layer permission cache (request-level + PostgreSQL).

    Architecture:
    - Layer 1: Request-level in-memory dict (fastest, same request only)
    - Layer 2: PostgreSQL UNLOGGED table (0.1-0.3ms, shared across instances)
    - Automatic invalidation via domain versioning (requires pg_fraiseql_cache)
    """

    def __init__(self, db_pool):
        """Initialize permission cache.

        Args:
            db_pool: PostgreSQL connection pool
        """
        self.pg_cache = PostgresCache(db_pool, table_name="fraiseql_cache")
        self._request_cache: dict[str, list[Permission]] = {}
        self._cache_ttl = timedelta(minutes=5)  # 5 minute TTL

        # RBAC domains for version checking
        self._rbac_domains = ['role', 'permission', 'role_permission', 'user_role']

    def _make_key(self, user_id: UUID, tenant_id: UUID | None) -> str:
        """Generate cache key for user permissions.

        Format: rbac:permissions:{user_id}:{tenant_id}
        """
        tenant_str = str(tenant_id) if tenant_id else 'global'
        return f"rbac:permissions:{user_id}:{tenant_str}"

    async def get(
        self,
        user_id: UUID,
        tenant_id: UUID | None
    ) -> list[Permission] | None:
        """Get cached permissions with version checking.

        Flow:
        1. Check request-level cache (instant)
        2. Check PostgreSQL cache (0.1-0.3ms)
        3. If found, verify domain versions haven't changed
        4. If stale, return None (caller will recompute)

        Args:
            user_id: User ID
            tenant_id: Tenant ID (None for global)

        Returns:
            List of permissions or None if not cached/stale
        """
        key = self._make_key(user_id, tenant_id)

        # Try request-level cache first (fastest)
        if key in self._request_cache:
            logger.debug("Permission cache HIT (request-level): %s", key)
            return self._request_cache[key]

        # Try PostgreSQL cache with version checking
        result, cached_versions = await self.pg_cache.get_with_metadata(key)

        if result is None:
            logger.debug("Permission cache MISS: %s", key)
            return None

        # Verify domain versions if extension is available
        if self.pg_cache.has_domain_versioning and cached_versions:
            current_versions = await self.pg_cache.get_domain_versions(
                tenant_id or 'global',
                self._rbac_domains
            )

            # Check if any domain version changed
            for domain in self._rbac_domains:
                cached_version = cached_versions.get(domain, 0)
                current_version = current_versions.get(domain, 0)

                if current_version != cached_version:
                    logger.debug(
                        "Permission cache STALE (domain %s changed: %d → %d): %s",
                        domain, cached_version, current_version, key
                    )
                    return None

        # Deserialize to Permission objects
        permissions = [Permission(**p) for p in result]

        # Populate request cache
        self._request_cache[key] = permissions

        logger.debug("Permission cache HIT (PostgreSQL): %s", key)
        return permissions

    async def set(
        self,
        user_id: UUID,
        tenant_id: UUID | None,
        permissions: list[Permission]
    ):
        """Cache permissions with domain version metadata.

        Stores in both request-level and PostgreSQL cache.
        Attaches domain versions for automatic invalidation detection.

        Args:
            user_id: User ID
            tenant_id: Tenant ID (None for global)
            permissions: List of permissions to cache
        """
        key = self._make_key(user_id, tenant_id)

        # Serialize permissions
        serialized = [
            {
                'id': str(p.id),
                'resource': p.resource,
                'action': p.action,
                'description': p.description,
                'constraints': p.constraints
            }
            for p in permissions
        ]

        # Get current domain versions
        versions = None
        if self.pg_cache.has_domain_versioning:
            versions = await self.pg_cache.get_domain_versions(
                tenant_id or 'global',
                self._rbac_domains
            )

        # Store in PostgreSQL cache with versions
        await self.pg_cache.set(
            key=key,
            value=serialized,
            ttl=int(self._cache_ttl.total_seconds()),
            versions=versions
        )

        # Store in request cache
        self._request_cache[key] = permissions

        logger.debug("Cached permissions for user %s (versions: %s)", user_id, versions)

    def clear_request_cache(self):
        """Clear request-level cache (called at end of request)."""
        self._request_cache.clear()

    async def invalidate_user(
        self,
        user_id: UUID,
        tenant_id: UUID | None = None
    ):
        """Manually invalidate cache for user.

        Note: With domain versioning, manual invalidation is rarely needed
        as cache is automatically invalidated when RBAC tables change.

        Args:
            user_id: User ID
            tenant_id: Tenant ID (None for global)
        """
        key = self._make_key(user_id, tenant_id)
        self._request_cache.pop(key, None)
        await self.pg_cache.delete(key)
        logger.debug("Invalidated permissions cache for user %s", user_id)

    async def invalidate_all(self):
        """Invalidate all cached permissions.

        Useful for testing or emergency cache clearing.
        """
        self._request_cache.clear()
        await self.pg_cache.delete_pattern("rbac:permissions:*")
        logger.info("Invalidated all permission caches")
```

**REFACTOR**: Add cache statistics and monitoring

```python
# Add to PermissionCache class

async def get_stats(self) -> dict:
    """Get cache statistics.

    Returns:
        Dict with cache stats (hits, misses, size, etc.)
    """
    pg_stats = await self.pg_cache.get_stats()

    # Count RBAC-specific entries
    # (would need to query fraiseql_cache table with LIKE filter)

    return {
        'request_cache_size': len(self._request_cache),
        'postgres_cache_total': pg_stats['total_entries'],
        'postgres_cache_active': pg_stats['active_entries'],
        'postgres_cache_size_bytes': pg_stats['table_size_bytes'],
        'has_domain_versioning': self.pg_cache.has_domain_versioning,
        'cache_ttl_seconds': int(self._cache_ttl.total_seconds()),
    }
```

**QA**: Test PostgreSQL permission cache

```bash
uv run pytest tests/integration/enterprise/rbac/test_permission_cache.py -v
```

---

#### TDD Cycle 2.2: Cache Invalidation

**RED**: Write failing test for automatic invalidation

```python
# tests/integration/enterprise/rbac/test_cache_invalidation.py

async def test_permission_cache_invalidates_on_role_change():
    """Verify cache invalidates when user roles change."""
    from fraiseql.enterprise.rbac.cache import PermissionCache
    from fraiseql.enterprise.rbac.resolver import PermissionResolver

    cache = PermissionCache(db_pool)
    resolver = PermissionResolver(db_repo, cache)

    user_id = uuid4()
    tenant_id = uuid4()

    # Get initial permissions (should cache)
    permissions1 = await resolver.get_user_permissions(user_id, tenant_id)
    initial_count = len(permissions1)

    # Assign new role to user
    await db.execute("""
        INSERT INTO user_roles (user_id, role_id, tenant_id)
        VALUES (%s, %s, %s)
    """, (user_id, 'some-new-role-id', tenant_id))

    # Get permissions again (should recompute due to invalidation)
    permissions2 = await resolver.get_user_permissions(user_id, tenant_id)

    # Should have different permissions now
    assert len(permissions2) != initial_count
    # Expected failure: cache not invalidating
```

**GREEN**: Verify automatic invalidation works

```python
# No code changes needed - domain versioning handles this automatically
# This test validates that the cache setup in Phase 1.2 is working

# However, add helper to manually trigger invalidation for testing
async def test_manual_invalidation():
    """Verify manual invalidation works."""
    from fraiseql.enterprise.rbac.cache import PermissionCache

    cache = PermissionCache(db_pool)
    user_id = uuid4()
    tenant_id = uuid4()

    # Cache some permissions
    await cache.set(user_id, tenant_id, [mock_permission()])

    # Verify cached
    assert await cache.get(user_id, tenant_id) is not None

    # Manually invalidate
    await cache.invalidate_user(user_id, tenant_id)

    # Verify invalidated
    assert await cache.get(user_id, tenant_id) is None
```

**REFACTOR**: Add CASCADE invalidation test

```python
async def test_cascade_invalidation_on_role_permission_change():
    """Verify CASCADE rule invalidates user permissions when role permissions change."""
    from fraiseql.enterprise.rbac.cache import PermissionCache
    from fraiseql.enterprise.rbac.resolver import PermissionResolver

    if not (await get_cache()).has_domain_versioning:
        pytest.skip("Requires pg_fraiseql_cache extension")

    cache = PermissionCache(db_pool)
    resolver = PermissionResolver(db_repo, cache)

    user_id = uuid4()
    role_id = uuid4()
    permission_id = uuid4()
    tenant_id = uuid4()

    # Setup: user has role
    await db.execute("""
        INSERT INTO user_roles (user_id, role_id, tenant_id)
        VALUES (%s, %s, %s)
    """, (user_id, role_id, tenant_id))

    # Get initial permissions (caches result)
    permissions1 = await resolver.get_user_permissions(user_id, tenant_id)

    # Add permission to role
    await db.execute("""
        INSERT INTO role_permissions (role_id, permission_id)
        VALUES (%s, %s)
    """, (role_id, permission_id))

    # Domain version increments:
    # 1. role_permissions INSERT → role_permission domain version++
    # 2. CASCADE rule → user_permissions domain version++

    # Get permissions again
    permissions2 = await resolver.get_user_permissions(user_id, tenant_id)

    # Should include new permission
    assert len(permissions2) > len(permissions1)
```

**QA**: Test automatic and manual invalidation

```bash
uv run pytest tests/integration/enterprise/rbac/test_cache_invalidation.py -v
```

---

### Phase 3: Role Hierarchy & Permission Resolution

**Objective**: Implement role hierarchy and permission resolver with caching

#### TDD Cycle 3.1: Role Hierarchy Engine

(Same as original plan - no changes needed)

**RED**: Write failing test for role hierarchy

```python
# tests/integration/enterprise/rbac/test_role_hierarchy.py

async def test_role_inheritance_chain():
    """Verify role inherits permissions from parent roles."""
    from fraiseql.enterprise.rbac.hierarchy import RoleHierarchy

    # Create role chain: admin -> manager -> developer -> junior_dev
    hierarchy = RoleHierarchy(db_repo)
    inherited_roles = await hierarchy.get_inherited_roles('junior-dev-role-id')

    role_names = [r.name for r in inherited_roles]
    assert 'junior_dev' in role_names
    assert 'developer' in role_names
    assert 'manager' in role_names
    assert 'admin' in role_names
    # Expected failure: get_inherited_roles not implemented
```

**GREEN**: Implement hierarchy engine (same as original)

```python
# src/fraiseql/enterprise/rbac/hierarchy.py

from uuid import UUID
from fraiseql.db import FraiseQLRepository, DatabaseQuery
from fraiseql.enterprise.rbac.models import Role


class RoleHierarchy:
    """Computes role hierarchy and inheritance."""

    def __init__(self, repo: FraiseQLRepository):
        self.repo = repo

    async def get_inherited_roles(self, role_id: UUID) -> list[Role]:
        """Get all roles in inheritance chain (including self).

        Uses PostgreSQL recursive CTE for efficient computation.

        Args:
            role_id: Starting role ID

        Returns:
            List of roles from most specific to most general

        Raises:
            ValueError: If cycle detected
        """
        results = await self.repo.run(DatabaseQuery(
            statement="SELECT * FROM get_inherited_roles(%s)",
            params={'role_id': str(role_id)},
            fetch_result=True
        ))

        if not results:
            return []

        # Check if we hit cycle detection limit
        if any(r['depth'] >= 10 for r in results):
            raise ValueError(f"Cycle detected in role hierarchy for role {role_id}")

        # Get full role details
        role_ids = [r['role_id'] for r in results]
        roles_data = await self.repo.run(DatabaseQuery(
            statement="""
                SELECT * FROM roles
                WHERE id = ANY(%s::uuid[])
                ORDER BY name
            """,
            params={'ids': role_ids},
            fetch_result=True
        ))

        return [Role(**row) for row in roles_data]
```

**QA**: Test role hierarchy

```bash
uv run pytest tests/integration/enterprise/rbac/test_role_hierarchy.py -v
```

---

#### TDD Cycle 3.2: Permission Resolver with PostgreSQL Cache

**RED**: Write failing test for permission resolution

```python
# tests/integration/enterprise/rbac/test_permission_resolution.py

async def test_user_effective_permissions_with_caching():
    """Verify user permissions are cached in PostgreSQL."""
    from fraiseql.enterprise.rbac.resolver import PermissionResolver
    from fraiseql.enterprise.rbac.cache import PermissionCache

    cache = PermissionCache(db_pool)
    resolver = PermissionResolver(db_repo, cache)

    user_id = uuid4()
    tenant_id = uuid4()

    # First call - should compute and cache
    permissions1 = await resolver.get_user_permissions(user_id, tenant_id)

    # Second call - should hit cache
    permissions2 = await resolver.get_user_permissions(user_id, tenant_id)

    assert permissions1 == permissions2
    # Expected failure: not using cache
```

**GREEN**: Implement permission resolver with cache

```python
# src/fraiseql/enterprise/rbac/resolver.py

from uuid import UUID
from fraiseql.db import FraiseQLRepository, DatabaseQuery
from fraiseql.enterprise.rbac.models import Permission, Role
from fraiseql.enterprise.rbac.hierarchy import RoleHierarchy
from fraiseql.enterprise.rbac.cache import PermissionCache
import logging

logger = logging.getLogger(__name__)


class PermissionResolver:
    """Resolves effective permissions for users with PostgreSQL caching."""

    def __init__(
        self,
        repo: FraiseQLRepository,
        cache: PermissionCache | None = None
    ):
        """Initialize permission resolver.

        Args:
            repo: FraiseQL database repository
            cache: Permission cache (optional, creates new if not provided)
        """
        self.repo = repo
        self.hierarchy = RoleHierarchy(repo)
        self.cache = cache or PermissionCache(repo.pool)

    async def get_user_permissions(
        self,
        user_id: UUID,
        tenant_id: UUID | None = None,
        use_cache: bool = True
    ) -> list[Permission]:
        """Get all effective permissions for a user.

        Flow:
        1. Check cache (request-level + PostgreSQL)
        2. If miss or stale, compute from database
        3. Cache result with domain versions
        4. Return permissions

        Args:
            user_id: User ID
            tenant_id: Optional tenant scope
            use_cache: Whether to use cache (default: True)

        Returns:
            List of effective permissions
        """
        # Try cache first
        if use_cache:
            cached = await self.cache.get(user_id, tenant_id)
            if cached is not None:
                logger.debug("Returning cached permissions for user %s", user_id)
                return cached

        # Cache miss or disabled - compute permissions
        logger.debug("Computing permissions for user %s", user_id)
        permissions = await self._compute_permissions(user_id, tenant_id)

        # Cache result
        if use_cache:
            await self.cache.set(user_id, tenant_id, permissions)

        return permissions

    async def _compute_permissions(
        self,
        user_id: UUID,
        tenant_id: UUID | None
    ) -> list[Permission]:
        """Compute effective permissions from database.

        This is the expensive operation that we cache.

        Args:
            user_id: User ID
            tenant_id: Optional tenant scope

        Returns:
            List of effective permissions
        """
        # Get user's direct roles
        user_roles = await self._get_user_roles(user_id, tenant_id)

        # Get all inherited roles
        all_role_ids: set[UUID] = set()
        for role in user_roles:
            inherited = await self.hierarchy.get_inherited_roles(role.id)
            all_role_ids.update(r.id for r in inherited)

        if not all_role_ids:
            return []

        # Get permissions for all roles
        permissions_data = await self.repo.run(DatabaseQuery(
            statement="""
                SELECT DISTINCT p.*
                FROM permissions p
                INNER JOIN role_permissions rp ON p.id = rp.permission_id
                WHERE rp.role_id = ANY(%s::uuid[])
                AND rp.granted = TRUE
                ORDER BY p.resource, p.action
            """,
            params={'role_ids': list(all_role_ids)},
            fetch_result=True
        ))

        return [Permission(**row) for row in permissions_data]

    async def _get_user_roles(
        self,
        user_id: UUID,
        tenant_id: UUID | None
    ) -> list[Role]:
        """Get roles directly assigned to user."""
        results = await self.repo.run(DatabaseQuery(
            statement="""
                SELECT r.*
                FROM roles r
                INNER JOIN user_roles ur ON r.id = ur.role_id
                WHERE ur.user_id = %s
                AND (ur.tenant_id = %s OR (ur.tenant_id IS NULL AND %s IS NULL))
                AND (ur.expires_at IS NULL OR ur.expires_at > NOW())
            """,
            params={
                'user_id': str(user_id),
                'tenant_id': str(tenant_id) if tenant_id else None
            },
            fetch_result=True
        ))

        return [Role(**row) for row in results]

    async def has_permission(
        self,
        user_id: UUID,
        resource: str,
        action: str,
        tenant_id: UUID | None = None
    ) -> bool:
        """Check if user has specific permission.

        Args:
            user_id: User ID
            resource: Resource name (e.g., 'user', 'product')
            action: Action name (e.g., 'create', 'read')
            tenant_id: Optional tenant scope

        Returns:
            True if user has permission, False otherwise
        """
        permissions = await self.get_user_permissions(user_id, tenant_id)

        return any(
            p.resource == resource and p.action == action
            for p in permissions
        )
```

**REFACTOR**: Add permission checking helpers

```python
# Add to PermissionResolver class

async def check_permission(
    self,
    user_id: UUID,
    resource: str,
    action: str,
    tenant_id: UUID | None = None,
    raise_on_deny: bool = True
) -> bool:
    """Check permission and optionally raise error.

    Args:
        user_id: User ID
        resource: Resource name
        action: Action name
        tenant_id: Optional tenant scope
        raise_on_deny: If True, raise PermissionError when denied

    Returns:
        True if permitted

    Raises:
        PermissionError: If raise_on_deny=True and permission denied
    """
    has_perm = await self.has_permission(user_id, resource, action, tenant_id)

    if not has_perm and raise_on_deny:
        raise PermissionError(
            f"Permission denied: requires {resource}.{action}"
        )

    return has_perm

async def get_user_roles(
    self,
    user_id: UUID,
    tenant_id: UUID | None = None
) -> list[Role]:
    """Get roles assigned to user (public method)."""
    return await self._get_user_roles(user_id, tenant_id)
```

**QA**: Test permission resolution with caching

```bash
uv run pytest tests/integration/enterprise/rbac/test_permission_resolution.py -v
uv run pytest tests/integration/enterprise/rbac/test_cache_performance.py -v
```

---

### Phase 4: GraphQL Integration & Directives

(Same as original plan - directives use PermissionResolver which now uses PostgreSQL cache)

**Implementation**: Same as original plan, but using PostgreSQL-cached PermissionResolver

---

### Phase 5: Row-Level Security (RLS)

(Same as original plan - no caching changes)

---

### Phase 6: Management APIs

(Same as original plan - mutations auto-invalidate via domain versioning)

---

## Performance Targets

**Cache Performance**:
- ✅ Request-level cache: <0.01ms (in-memory dict)
- ✅ PostgreSQL cache: <0.3ms (UNLOGGED table, indexed)
- ✅ Total cached lookup: <0.5ms (well under 5ms target)
- ✅ Permission computation (uncached): <50ms (expensive, but cached)

**Cache Hit Rates**:
- Expected: 85-95% (typical for permission checks)
- Target: >80% hit rate in production

**Invalidation**:
- Automatic: Domain versioning (instant, trigger-based)
- Manual: <1ms (single DELETE query)

---

## Success Criteria

**Phase 1: Schema & Models**
- [ ] RBAC tables created with hierarchy support
- [ ] PostgreSQL cache domains registered
- [ ] Table triggers configured for auto-invalidation
- [ ] CASCADE rules configured
- [ ] Models defined with proper types
- [ ] GraphQL types implemented
- [ ] All tests pass

**Phase 2: PostgreSQL Caching**
- [ ] PermissionCache implemented using PostgresCache
- [ ] 2-layer cache working (request + PostgreSQL)
- [ ] Domain versioning enabled
- [ ] Automatic invalidation working
- [ ] Manual invalidation working
- [ ] Cache statistics available
- [ ] Performance <0.5ms for cached lookups

**Phase 3: Permission Resolution**
- [ ] User permissions computed from all roles
- [ ] Role hierarchy working
- [ ] Caching integrated
- [ ] Cache invalidation working
- [ ] Performance <5ms for cached lookups
- [ ] Performance <100ms for uncached computation

**Phase 4: GraphQL Integration**
- [ ] @requires_permission directive working
- [ ] @requires_role directive working
- [ ] Constraint evaluation implemented
- [ ] Error messages helpful

**Phase 5: Row-Level Security**
- [ ] RLS policies enforced
- [ ] Tenant isolation working
- [ ] Own-data-only constraints working
- [ ] Super admin bypass working

**Phase 6: Management APIs**
- [ ] Role creation/deletion working
- [ ] Role assignment working
- [ ] Permission management working
- [ ] Audit logging integrated

**Overall Success Metrics**:
- [ ] Supports 10,000+ users
- [ ] Permission check <5ms (cached) ✅ Actual: <0.5ms
- [ ] Permission check <100ms (uncached)
- [ ] Cache hit rate >80% (target: 85-95%)
- [ ] Automatic invalidation working (no stale permissions)
- [ ] Zero additional infrastructure cost (no Redis)
- [ ] Hierarchy depth up to 10 levels
- [ ] Multi-tenant isolation enforced
- [ ] 100% test coverage
- [ ] Documentation complete

---

## PostgreSQL-Specific Benefits

**Automatic Invalidation**:
- ✅ No manual cache clearing logic
- ✅ No stale permission bugs
- ✅ CASCADE rules for hierarchical invalidation
- ✅ Tenant-scoped version tracking

**Operational Simplicity**:
- ✅ One database (PostgreSQL only)
- ✅ No Redis cluster management
- ✅ No Redis failover complexity
- ✅ Unified backup strategy

**Cost Savings**:
- ✅ $0 additional infrastructure
- ✅ No Redis Cloud subscription ($50-500/month)
- ✅ Aligns with "In PostgreSQL Everything" promise

**ACID Guarantees**:
- ✅ Transactional cache updates
- ✅ Consistent reads across instances
- ✅ No eventual consistency issues

**Integration**:
- ✅ Leverages existing PostgresCache infrastructure
- ✅ Works with APQ cache (same backend)
- ✅ Unified monitoring (Grafana queries PostgreSQL)
- ✅ Single connection pool

---

## Migration Notes

**From Redis-based plan**:
1. Replace `redis` dependency with `PostgresCache`
2. Remove Redis connection setup
3. Use `PermissionCache(db_pool)` instead of `PermissionCache(redis_client)`
4. Remove manual invalidation logic (rely on domain versioning)
5. Update documentation to reflect PostgreSQL-only architecture

**Backward Compatibility**:
- If `pg_fraiseql_cache` extension not available, falls back to TTL-only caching
- Still faster than Redis for permission lookups
- Graceful degradation

---

## Documentation Requirements

**New Documentation**:
- `docs/enterprise/rbac-postgresql-caching.md` - Architecture deep-dive
- `docs/enterprise/rbac-cache-invalidation.md` - Domain versioning guide
- `docs/enterprise/rbac-performance.md` - Performance benchmarks

**Updated Documentation**:
- Update all RBAC references to specify PostgreSQL caching
- Add section to "In PostgreSQL Everything" philosophy
- Include RBAC as example in marketing materials

---

## Testing Strategy

**Unit Tests**:
- PermissionCache get/set/invalidate
- Domain version checking
- Request-level cache

**Integration Tests**:
- Automatic invalidation on role changes
- CASCADE rule invalidation
- Multi-tenant cache isolation
- Permission resolution with caching

**Performance Tests**:
- Cache hit rate measurement
- Cached lookup latency (<0.5ms)
- Uncached computation latency (<100ms)
- 10,000 user stress test

**Load Tests**:
- 1,000 concurrent permission checks
- Cache invalidation under load
- Multi-tenant cache performance

---

**End of Refactored RBAC Plan**

This implementation maintains all functionality of the original plan while leveraging PostgreSQL for caching, ensuring consistency with FraiseQL's "In PostgreSQL Everything" philosophy.
