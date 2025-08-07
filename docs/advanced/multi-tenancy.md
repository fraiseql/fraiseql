---
← [Event Sourcing](event-sourcing.md) | [Advanced Topics](index.md) | [Next: Bounded Contexts](bounded-contexts.md) →
---

# Multi-tenancy

> **In this section:** Implement secure multi-tenant architectures with FraiseQL
> **Prerequisites:** Understanding of [security patterns](security.md) and [database design](../core-concepts/database-views.md)
> **Time to complete:** 30 minutes

FraiseQL provides several multi-tenancy patterns to isolate tenant data while maintaining performance and security.

## Tenancy Patterns

### 1. Schema-per-Tenant (High Isolation)

#### Database Schema
```sql
-- Create tenant schemas dynamically
CREATE SCHEMA tenant_acme_corp;
CREATE SCHEMA tenant_globex_ltd;

-- Each tenant gets identical table structure
CREATE TABLE tenant_acme_corp.tb_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    email TEXT UNIQUE NOT NULL,
    created_at TIMESTAMP DEFAULT NOW()
);

CREATE TABLE tenant_globex_ltd.tb_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    email TEXT UNIQUE NOT NULL,
    created_at TIMESTAMP DEFAULT NOW()
);
```

#### Dynamic Schema Resolution
```python
from fraiseql import FraiseQL
from fraiseql.repository import FraiseQLRepository

class MultiTenantRepository(FraiseQLRepository):
    def __init__(self, database_url: str, tenant_id: str):
        super().__init__(database_url)
        self.tenant_schema = f"tenant_{tenant_id}"

    async def find(self, view_name: str, **kwargs):
        """Override to use tenant schema"""
        qualified_view = f"{self.tenant_schema}.{view_name}"
        return await super().find(qualified_view, **kwargs)

    async def find_one(self, view_name: str, **kwargs):
        """Override to use tenant schema"""
        qualified_view = f"{self.tenant_schema}.{view_name}"
        return await super().find_one(qualified_view, **kwargs)

# Context setup
async def get_tenant_context(request):
    # Extract tenant from subdomain, header, or JWT
    tenant_id = extract_tenant_id(request)

    if not tenant_id:
        raise HTTPException(401, "Tenant not specified")

    return {
        "repo": MultiTenantRepository(DATABASE_URL, tenant_id),
        "tenant_id": tenant_id,
        "user": await get_current_user(request)
    }
```

### 2. Row-Level Security (Shared Schema)

#### RLS Setup
```sql
-- Enable RLS on tables
ALTER TABLE tb_user ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_post ENABLE ROW LEVEL SECURITY;

-- Add tenant_id to all tables
ALTER TABLE tb_user ADD COLUMN tenant_id UUID NOT NULL;
ALTER TABLE tb_post ADD COLUMN tenant_id UUID NOT NULL;

-- Create RLS policies
CREATE POLICY tenant_isolation_user ON tb_user
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

CREATE POLICY tenant_isolation_post ON tb_post
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

-- Views with RLS
CREATE VIEW v_user AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at
    ) AS data
FROM tb_user
WHERE tenant_id = current_setting('app.current_tenant_id')::UUID;
```

#### RLS Repository Implementation
```python
class RLSRepository(FraiseQLRepository):
    def __init__(self, database_url: str):
        super().__init__(database_url)

    async def set_tenant_context(self, tenant_id: str):
        """Set tenant context for RLS"""
        await self.execute(
            "SELECT set_config('app.current_tenant_id', $1, true)",
            tenant_id
        )

    async def with_tenant(self, tenant_id: str):
        """Context manager for tenant operations"""
        await self.set_tenant_context(tenant_id)
        return self

# Usage in resolvers
@fraiseql.query
async def users(info) -> list[User]:
    repo = info.context["repo"]
    tenant_id = info.context["tenant_id"]

    async with repo.with_tenant(tenant_id):
        return await repo.find("v_user")
```

### 3. Discriminator Column (Simple)

#### Schema with Tenant Column
```sql
-- Simple tenant_id column approach
CREATE TABLE tb_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),

    -- Unique constraints scoped to tenant
    UNIQUE(tenant_id, email)
);

-- Views automatically filter by tenant
CREATE VIEW v_user AS
SELECT
    id,
    tenant_id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at
    ) AS data
FROM tb_user;
```

#### Application-Level Filtering
```python
@fraiseql.query
async def users(info, limit: int = 10) -> list[User]:
    """Users scoped to current tenant"""
    repo = info.context["repo"]
    tenant_id = info.context["tenant_id"]

    return await repo.find(
        "v_user",
        where={"tenant_id": tenant_id},
        limit=limit
    )

@fraiseql.mutation
async def create_user(info, name: str, email: str) -> User:
    """Create user in current tenant"""
    repo = info.context["repo"]
    tenant_id = info.context["tenant_id"]

    user_id = await repo.call_function(
        "fn_create_user",
        p_tenant_id=tenant_id,
        p_name=name,
        p_email=email
    )

    result = await repo.find_one(
        "v_user",
        where={"id": user_id, "tenant_id": tenant_id}
    )
    return User(**result)
```

## Tenant Management

### Tenant Registration
```sql
-- Tenant management tables
CREATE TABLE tb_tenant (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    slug TEXT UNIQUE NOT NULL,
    subscription_tier TEXT DEFAULT 'basic',
    created_at TIMESTAMP DEFAULT NOW(),
    is_active BOOLEAN DEFAULT TRUE
);

CREATE TABLE tb_tenant_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tb_tenant(id),
    user_id UUID NOT NULL,
    role TEXT NOT NULL DEFAULT 'member',
    created_at TIMESTAMP DEFAULT NOW(),

    UNIQUE(tenant_id, user_id)
);
```

### Tenant Provisioning
```python
@fraiseql.mutation
async def create_tenant(info, name: str, slug: str) -> Tenant:
    """Create new tenant with schema"""
    repo = info.context["repo"]
    user = info.context["user"]

    async with repo.transaction():
        # Create tenant record
        tenant_id = await repo.call_function(
            "fn_create_tenant",
            p_name=name,
            p_slug=slug,
            p_owner_id=user.id
        )

        # For schema-per-tenant: create schema
        if TENANCY_MODEL == "schema":
            schema_name = f"tenant_{slug}"
            await repo.execute(f"CREATE SCHEMA {schema_name}")

            # Run migration scripts for new schema
            await provision_tenant_schema(repo, schema_name)

        result = await repo.find_one("v_tenant", where={"id": tenant_id})
        return Tenant(**result)

async def provision_tenant_schema(repo: FraiseQLRepository, schema_name: str):
    """Provision tenant schema with tables and views"""
    migration_sql = f"""
    CREATE TABLE {schema_name}.tb_user (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        name TEXT NOT NULL,
        email TEXT UNIQUE NOT NULL,
        created_at TIMESTAMP DEFAULT NOW()
    );

    CREATE VIEW {schema_name}.v_user AS
    SELECT
        id,
        jsonb_build_object(
            'id', id,
            'name', name,
            'email', email,
            'created_at', created_at
        ) AS data
    FROM {schema_name}.tb_user;
    """

    await repo.execute(migration_sql)
```

## Tenant Context Resolution

### JWT-Based Tenant Resolution
```python
import jwt
from fastapi import HTTPException, Request

async def extract_tenant_from_jwt(request: Request) -> str:
    """Extract tenant from JWT token"""
    auth_header = request.headers.get("authorization")
    if not auth_header or not auth_header.startswith("Bearer "):
        raise HTTPException(401, "Missing authentication")

    token = auth_header[7:]
    try:
        payload = jwt.decode(token, JWT_SECRET, algorithms=["HS256"])
        tenant_id = payload.get("tenant_id")
        if not tenant_id:
            raise HTTPException(401, "Tenant not specified in token")
        return tenant_id
    except jwt.InvalidTokenError:
        raise HTTPException(401, "Invalid token")
```

### Subdomain-Based Resolution
```python
async def extract_tenant_from_subdomain(request: Request) -> str:
    """Extract tenant from subdomain"""
    host = request.headers.get("host", "")
    if not host:
        raise HTTPException(400, "Host header required")

    parts = host.split(".")
    if len(parts) < 2:
        raise HTTPException(400, "Subdomain required")

    subdomain = parts[0]
    if subdomain in ["www", "api", "admin"]:
        raise HTTPException(400, "Invalid tenant subdomain")

    return subdomain
```

### Header-Based Resolution
```python
async def extract_tenant_from_header(request: Request) -> str:
    """Extract tenant from custom header"""
    tenant_id = request.headers.get("x-tenant-id")
    if not tenant_id:
        raise HTTPException(400, "X-Tenant-ID header required")
    return tenant_id
```

## Multi-Tenant Security

### Tenant Access Control
```python
class TenantAccessControl:
    @staticmethod
    async def verify_tenant_access(user_id: str, tenant_id: str, repo: FraiseQLRepository) -> bool:
        """Verify user has access to tenant"""
        result = await repo.find_one(
            "tb_tenant_user",
            where={"user_id": user_id, "tenant_id": tenant_id}
        )
        return result is not None

    @staticmethod
    async def verify_tenant_role(user_id: str, tenant_id: str, required_role: str, repo: FraiseQLRepository) -> bool:
        """Verify user has required role in tenant"""
        result = await repo.find_one(
            "tb_tenant_user",
            where={"user_id": user_id, "tenant_id": tenant_id}
        )

        if not result:
            return False

        user_role = result["role"]
        role_hierarchy = ["member", "admin", "owner"]

        return (role_hierarchy.index(user_role) >=
                role_hierarchy.index(required_role))

# Usage in resolvers
@fraiseql.query
async def tenant_users(info) -> list[User]:
    """Admin-only: list all users in tenant"""
    repo = info.context["repo"]
    user = info.context["user"]
    tenant_id = info.context["tenant_id"]

    # Check permission
    if not await TenantAccessControl.verify_tenant_role(
        user.id, tenant_id, "admin", repo
    ):
        raise GraphQLError("Insufficient permissions", code="FORBIDDEN")

    return await repo.find("v_user", where={"tenant_id": tenant_id})
```

### Cross-Tenant Data Protection
```python
@fraiseql.query
async def user(info, id: ID) -> User | None:
    """Ensure user belongs to current tenant"""
    repo = info.context["repo"]
    tenant_id = info.context["tenant_id"]

    # Always include tenant_id in queries
    result = await repo.find_one(
        "v_user",
        where={"id": id, "tenant_id": tenant_id}
    )

    return User(**result) if result else None

# Middleware to enforce tenant isolation
@app.middleware("http")
async def enforce_tenant_isolation(request: Request, call_next):
    """Middleware to verify all operations are tenant-scoped"""
    response = await call_next(request)

    # Log cross-tenant access attempts
    if hasattr(request.state, "tenant_violations"):
        logger.warning(f"Cross-tenant access attempt: {request.state.tenant_violations}")

    return response
```

## Performance Optimization

### Connection Pooling per Tenant
```python
from typing import Dict
import asyncpg

class MultiTenantConnectionManager:
    def __init__(self):
        self.pools: Dict[str, asyncpg.Pool] = {}

    async def get_pool(self, tenant_id: str) -> asyncpg.Pool:
        """Get or create connection pool for tenant"""
        if tenant_id not in self.pools:
            self.pools[tenant_id] = await asyncpg.create_pool(
                DATABASE_URL,
                min_size=5,
                max_size=20,
                command_timeout=60
            )
        return self.pools[tenant_id]

    async def close_all(self):
        """Close all tenant pools"""
        for pool in self.pools.values():
            await pool.close()

# Global connection manager
connection_manager = MultiTenantConnectionManager()
```

### Tenant-Specific Caching
```python
from typing import Dict, Any
import redis

class MultiTenantCache:
    def __init__(self, redis_url: str):
        self.redis = redis.from_url(redis_url)

    def _tenant_key(self, tenant_id: str, key: str) -> str:
        """Scope cache keys to tenant"""
        return f"tenant:{tenant_id}:{key}"

    async def get(self, tenant_id: str, key: str) -> Any:
        """Get tenant-scoped cache value"""
        tenant_key = self._tenant_key(tenant_id, key)
        return await self.redis.get(tenant_key)

    async def set(self, tenant_id: str, key: str, value: Any, ttl: int = 3600):
        """Set tenant-scoped cache value"""
        tenant_key = self._tenant_key(tenant_id, key)
        await self.redis.setex(tenant_key, ttl, value)

    async def invalidate_tenant(self, tenant_id: str):
        """Invalidate all cache for tenant"""
        pattern = f"tenant:{tenant_id}:*"
        keys = await self.redis.keys(pattern)
        if keys:
            await self.redis.delete(*keys)
```

## Migration and Scaling

### Schema Migration for Multi-Tenant
```python
class TenantMigrator:
    def __init__(self, repo: FraiseQLRepository):
        self.repo = repo

    async def migrate_all_tenants(self, migration_sql: str):
        """Apply migration to all tenant schemas"""
        tenants = await self.repo.find("tb_tenant", where={"is_active": True})

        for tenant in tenants:
            try:
                if TENANCY_MODEL == "schema":
                    # Schema-per-tenant migration
                    schema_name = f"tenant_{tenant['slug']}"
                    tenant_migration = migration_sql.replace(
                        "{{schema}}", schema_name
                    )
                    await self.repo.execute(tenant_migration)
                else:
                    # Shared schema migration (run once)
                    await self.repo.execute(migration_sql)
                    break

                logger.info(f"Migrated tenant {tenant['id']}")

            except Exception as e:
                logger.error(f"Migration failed for tenant {tenant['id']}: {e}")
                raise
```

### Tenant Archival
```python
@fraiseql.mutation
async def archive_tenant(info, tenant_id: ID) -> bool:
    """Archive inactive tenant data"""
    repo = info.context["repo"]
    user = info.context["user"]

    # Verify permission (platform admin only)
    if not user.is_platform_admin:
        raise GraphQLError("Insufficient permissions", code="FORBIDDEN")

    async with repo.transaction():
        # Mark tenant as archived
        await repo.execute(
            "UPDATE tb_tenant SET is_active = FALSE, archived_at = NOW() WHERE id = $1",
            tenant_id
        )

        if TENANCY_MODEL == "schema":
            # For schema-per-tenant: rename schema for archival
            tenant = await repo.find_one("tb_tenant", where={"id": tenant_id})
            old_schema = f"tenant_{tenant['slug']}"
            archived_schema = f"archived_{tenant['slug']}_{datetime.now().strftime('%Y%m%d')}"

            await repo.execute(f"ALTER SCHEMA {old_schema} RENAME TO {archived_schema}")

        return True
```

## Best Practices

### Security
- Always validate tenant context in every request
- Use parameterized queries to prevent injection
- Implement proper role-based access within tenants
- Log cross-tenant access attempts
- Regular security audits of tenant isolation

### Performance
- Use connection pooling per tenant for schema-per-tenant
- Implement tenant-aware caching strategies
- Consider tenant data distribution for sharding
- Monitor query performance per tenant

### Operational
- Automate tenant provisioning and deprovisioning
- Implement tenant-aware monitoring and alerting
- Plan for tenant data migration and archival
- Document tenant onboarding procedures

## See Also

### Related Concepts
- [**Security Patterns**](security.md) - Authentication and authorization
- [**Performance Tuning**](performance.md) - Optimization strategies
- [**Database Views**](../core-concepts/database-views.md) - View design patterns

### Implementation
- [**Authentication**](authentication.md) - User authentication patterns
- [**CQRS**](cqrs.md) - Multi-tenant CQRS patterns
- [**Testing**](../testing/integration-testing.md) - Multi-tenant testing

### Advanced Topics
- [**Bounded Contexts**](bounded-contexts.md) - Domain boundaries
- [**Event Sourcing**](event-sourcing.md) - Multi-tenant event stores
- [**Deployment**](../deployment/index.md) - Multi-tenant deployment
