# Multi-Tenant Guide - FraiseQL Native Authentication

Complete guide for implementing multi-tenant authentication with schema-per-tenant or shared schema approaches.

## Multi-Tenancy Overview

FraiseQL native auth supports two multi-tenant architectures:

1. **Schema-per-Tenant**: Each tenant gets its own PostgreSQL schema (perfect isolation)
2. **Shared Schema with Row-Level Security**: All tenants share tables with tenant_id filtering

### When to Use Each Approach

| Factor | Schema-per-Tenant | Shared Schema + RLS |
|--------|-------------------|-------------------|
| **Number of tenants** | <1,000 tenants | 1,000+ tenants |
| **Data isolation** | Perfect (DB-level) | Good (App-level) |
| **Performance** | Excellent for <500 | Better for 1,000+ |
| **Compliance** | Ideal for regulated industries | Good for most cases |
| **Operations complexity** | Higher | Lower |
| **Cross-tenant analytics** | Impossible | Easy |
| **Migration time** | Linear (N × time) | Constant time |

## Schema-per-Tenant Implementation

### 1. Basic Setup

Each tenant gets their own PostgreSQL schema with complete table isolation:

```python
# tenant_manager.py
import asyncio
from typing import List
from psycopg_pool import AsyncConnectionPool
from fraiseql.auth.native import apply_native_auth_schema, create_native_auth_provider

class TenantManager:
    def __init__(self, db_pool: AsyncConnectionPool):
        self.db_pool = db_pool
        self.providers = {}  # Cache of tenant providers
    
    async def create_tenant_schema(self, tenant_id: str) -> None:
        """Create a new tenant schema with auth tables"""
        schema_name = f"tenant_{tenant_id}"
        
        async with self.db_pool.connection() as conn:
            # Create schema
            await conn.execute(f'CREATE SCHEMA IF NOT EXISTS "{schema_name}"')
            
            # Apply native auth schema
            await apply_native_auth_schema(self.db_pool, schema=schema_name)
            
            print(f"✅ Created tenant schema: {schema_name}")
    
    async def get_tenant_provider(self, tenant_id: str):
        """Get or create auth provider for tenant"""
        if tenant_id not in self.providers:
            schema_name = f"tenant_{tenant_id}"
            self.providers[tenant_id] = await create_native_auth_provider(
                self.db_pool,
                schema=schema_name
            )
        
        return self.providers[tenant_id]
    
    async def list_tenants(self) -> List[str]:
        """List all tenant schemas"""
        async with self.db_pool.connection() as conn:
            async with conn.cursor() as cursor:
                await cursor.execute("""
                    SELECT schema_name 
                    FROM information_schema.schemata 
                    WHERE schema_name LIKE 'tenant_%'
                """)
                results = await cursor.fetchall()
                return [result[0].replace('tenant_', '') for result in results]
    
    async def delete_tenant_schema(self, tenant_id: str) -> None:
        """Delete tenant schema and all data (DANGEROUS!)"""
        schema_name = f"tenant_{tenant_id}"
        
        async with self.db_pool.connection() as conn:
            await conn.execute(f'DROP SCHEMA IF EXISTS "{schema_name}" CASCADE')
            
        # Remove from cache
        if tenant_id in self.providers:
            del self.providers[tenant_id]
        
        print(f"⚠️  Deleted tenant schema: {schema_name}")

# Usage
tenant_manager = TenantManager(db_pool)
await tenant_manager.create_tenant_schema("acme_corp")
await tenant_manager.create_tenant_schema("globex")
```

### 2. Tenant Resolution Middleware

```python
# tenant_middleware.py
from fastapi import Request, HTTPException
from typing import Optional

class TenantResolutionMiddleware:
    def __init__(self, tenant_manager: TenantManager):
        self.tenant_manager = tenant_manager
    
    async def __call__(self, request: Request, call_next):
        # Extract tenant ID from request
        tenant_id = await self._extract_tenant_id(request)
        
        if not tenant_id:
            raise HTTPException(400, "Tenant ID required")
        
        # Verify tenant exists
        tenants = await self.tenant_manager.list_tenants()
        if tenant_id not in tenants:
            raise HTTPException(404, f"Tenant {tenant_id} not found")
        
        # Store tenant context
        request.state.tenant_id = tenant_id
        request.state.auth_provider = await self.tenant_manager.get_tenant_provider(tenant_id)
        
        return await call_next(request)
    
    async def _extract_tenant_id(self, request: Request) -> Optional[str]:
        """Extract tenant ID from request (multiple strategies)"""
        
        # Strategy 1: Subdomain (acme.yourapp.com)
        host = request.headers.get("host", "")
        if "." in host:
            subdomain = host.split(".")[0]
            if subdomain not in ["www", "api"]:
                return subdomain
        
        # Strategy 2: Header
        tenant_header = request.headers.get("X-Tenant-ID")
        if tenant_header:
            return tenant_header
        
        # Strategy 3: JWT Token
        auth_header = request.headers.get("authorization", "")
        if auth_header.startswith("Bearer "):
            try:
                token = auth_header.split(" ")[1]
                # Decode JWT to extract tenant (custom claim)
                import jwt
                payload = jwt.decode(token, options={"verify_signature": False})
                return payload.get("tenant_id")
            except:
                pass
        
        # Strategy 4: Query parameter (for testing)
        return request.query_params.get("tenant_id")

# Add to FastAPI app
app.add_middleware(TenantResolutionMiddleware, tenant_manager=tenant_manager)
```

### 3. Tenant-Aware Authentication Endpoints

```python
# tenant_auth_router.py
from fastapi import APIRouter, Request, HTTPException, Depends

router = APIRouter(prefix="/auth")

async def get_tenant_auth_provider(request: Request):
    """Dependency to get tenant-specific auth provider"""
    if not hasattr(request.state, 'auth_provider'):
        raise HTTPException(400, "Tenant not resolved")
    return request.state.auth_provider

@router.post("/register")
async def register(
    request: Request,
    user_data: RegisterInput,
    auth_provider = Depends(get_tenant_auth_provider)
):
    """Register user in tenant-specific schema"""
    tenant_id = request.state.tenant_id
    
    # Create user in tenant schema
    user = User(
        email=user_data.email,
        name=user_data.name,
        metadata={"tenant_id": tenant_id}  # Store tenant in metadata
    )
    user.set_password(user_data.password)
    
    # Save to tenant-specific schema
    async with auth_provider.db_pool.connection() as conn:
        async with conn.cursor() as cursor:
            await user.save(cursor, auth_provider.schema)
            await conn.commit()
    
    # Generate tokens
    tokens = await auth_provider.create_tokens_for_user(str(user.id))
    
    return {
        "user": user.to_dict(),
        "access_token": tokens["access_token"],
        "refresh_token": tokens["refresh_token"],
        "token_type": "bearer"
    }

@router.post("/login") 
async def login(
    request: Request,
    credentials: LoginInput,
    auth_provider = Depends(get_tenant_auth_provider)
):
    """Login user from tenant-specific schema"""
    # Authenticate against tenant schema
    async with auth_provider.db_pool.connection() as conn:
        async with conn.cursor() as cursor:
            user = await User.get_by_email(cursor, auth_provider.schema, credentials.email)
    
    if not user or not user.verify_password(credentials.password):
        raise HTTPException(401, "Invalid credentials")
    
    # Generate tokens
    tokens = await auth_provider.create_tokens_for_user(str(user.id))
    
    return {
        "user": user.to_dict(),
        "access_token": tokens["access_token"], 
        "refresh_token": tokens["refresh_token"],
        "token_type": "bearer"
    }
```

### 4. Tenant-Specific Data Isolation

```python
# tenant_repository.py
from fraiseql.database.repository import FraiseQLRepository

class TenantAwareRepository(FraiseQLRepository):
    def __init__(self, connection_pool, tenant_id: str):
        super().__init__(connection_pool)
        self.schema = f"tenant_{tenant_id}"
    
    async def run(self, query):
        """Execute query with tenant schema context"""
        async with self._pool.connection() as conn:
            # Set search path to tenant schema
            await conn.execute(f"SET search_path TO {self.schema}, public")
            
            # Execute query (will use tenant tables)
            async with conn.cursor() as cursor:
                await cursor.execute(query.sql, query.parameters)
                return await cursor.fetchall()

# Usage in GraphQL resolvers
@fraiseql.query
async def get_user_orders(info, user_id: str):
    # Get tenant from request context
    tenant_id = info.context["request"].state.tenant_id
    
    # Use tenant-aware repository
    repo = TenantAwareRepository(db_pool, tenant_id)
    
    # Query will only access tenant's data
    orders = await repo.find("orders", filters={"user_id": user_id})
    return orders
```

### 5. Migrations for Multi-Tenant Schemas

```python
# tenant_migrations.py
class TenantMigrationRunner:
    def __init__(self, tenant_manager: TenantManager):
        self.tenant_manager = tenant_manager
    
    async def migrate_all_tenants(self, migration_sql: str):
        """Apply migration to all tenant schemas"""
        tenants = await self.tenant_manager.list_tenants()
        
        for tenant_id in tenants:
            try:
                await self._migrate_tenant(tenant_id, migration_sql)
                print(f"✅ Migrated tenant: {tenant_id}")
            except Exception as e:
                print(f"❌ Failed to migrate tenant {tenant_id}: {e}")
                # Continue with other tenants
    
    async def _migrate_tenant(self, tenant_id: str, migration_sql: str):
        """Apply migration to specific tenant schema"""
        schema_name = f"tenant_{tenant_id}"
        
        async with self.tenant_manager.db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {schema_name}, public")
            await conn.execute(migration_sql)
            await conn.commit()

# Usage
migration_sql = """
ALTER TABLE tb_user ADD COLUMN phone_number VARCHAR(20);
CREATE INDEX idx_user_phone ON tb_user(phone_number);
"""

migration_runner = TenantMigrationRunner(tenant_manager)
await migration_runner.migrate_all_tenants(migration_sql)
```

## Shared Schema + Row-Level Security

For scenarios with many tenants (1,000+), shared schema with Row-Level Security provides better scalability:

### 1. Database Schema with Tenant ID

```sql
-- Modified auth tables with tenant_id
CREATE TABLE tb_user (
    pk_user UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,  -- Added for multi-tenancy
    email VARCHAR(255) NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    name VARCHAR(255),
    roles TEXT[] DEFAULT '{}',
    permissions TEXT[] DEFAULT '{}',
    metadata JSONB DEFAULT '{}',
    is_active BOOLEAN DEFAULT true,
    email_verified BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uk_user_email_tenant UNIQUE (email, tenant_id)  -- Modified constraint
);

-- Add tenant_id to all auth tables
ALTER TABLE tb_session ADD COLUMN tenant_id UUID NOT NULL;
ALTER TABLE tb_used_refresh_token ADD COLUMN tenant_id UUID NOT NULL;
ALTER TABLE tb_password_reset ADD COLUMN tenant_id UUID NOT NULL;
ALTER TABLE tb_auth_audit ADD COLUMN tenant_id UUID;

-- Enable Row Level Security
ALTER TABLE tb_user ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_session ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_used_refresh_token ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_password_reset ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_auth_audit ENABLE ROW LEVEL SECURITY;

-- Create RLS policies
CREATE POLICY user_tenant_isolation ON tb_user
    USING (tenant_id = current_setting('app.current_tenant')::UUID);

CREATE POLICY session_tenant_isolation ON tb_session
    USING (tenant_id = current_setting('app.current_tenant')::UUID);

-- Repeat for other tables...
```

### 2. Tenant-Aware User Model

```python
# shared_schema_models.py
from fraiseql.auth.native.models import User as BaseUser
from uuid import UUID

class MultiTenantUser(BaseUser):
    def __init__(self, tenant_id: UUID, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.tenant_id = tenant_id
    
    async def save(self, cursor, schema: str = "public"):
        """Save user with tenant_id"""
        if self.id:
            # Update existing user
            await cursor.execute(f"""
                UPDATE {schema}.tb_user SET
                    email = %s, password_hash = %s, name = %s,
                    roles = %s, permissions = %s, metadata = %s,
                    is_active = %s, email_verified = %s, updated_at = CURRENT_TIMESTAMP
                WHERE pk_user = %s AND tenant_id = %s
            """, (
                self.email, self._password_hash, self.name,
                self.roles, self.permissions, self.metadata,
                self.is_active, self.email_verified, self.id, self.tenant_id
            ))
        else:
            # Insert new user
            await cursor.execute(f"""
                INSERT INTO {schema}.tb_user 
                (tenant_id, email, password_hash, name, roles, permissions, metadata, is_active, email_verified)
                VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s)
                RETURNING pk_user
            """, (
                self.tenant_id, self.email, self._password_hash, self.name,
                self.roles, self.permissions, self.metadata, 
                self.is_active, self.email_verified
            ))
            
            result = await cursor.fetchone()
            self.id = result[0] if result else None
    
    @classmethod
    async def get_by_email(cls, cursor, schema: str, tenant_id: UUID, email: str):
        """Get user by email within tenant"""
        await cursor.execute(f"""
            SELECT pk_user, email, password_hash, name, roles, permissions, 
                   metadata, is_active, email_verified, created_at, updated_at
            FROM {schema}.tb_user 
            WHERE tenant_id = %s AND email = %s AND is_active = true
        """, (tenant_id, email))
        
        result = await cursor.fetchone()
        if not result:
            return None
        
        user = cls(tenant_id=tenant_id)
        user._populate_from_db_result(result)
        return user
```

### 3. Shared Schema Auth Provider

```python
# shared_schema_provider.py
from fraiseql.auth.native.provider import NativeAuthProvider
from uuid import UUID

class SharedSchemaAuthProvider(NativeAuthProvider):
    def __init__(self, token_manager, db_pool, tenant_id: UUID, schema: str = "public"):
        super().__init__(token_manager, db_pool, schema)
        self.tenant_id = tenant_id
    
    async def get_user_from_token(self, token: str):
        """Get user with tenant filtering"""
        # Validate token
        payload = await self.validate_token(token)
        user_id = payload.get("sub")
        
        if not user_id:
            raise InvalidTokenError("Token missing user ID")
        
        # Set tenant context for RLS
        async with self.db_pool.connection() as conn:
            await conn.execute(f"SET app.current_tenant = '{self.tenant_id}'")
            
            async with conn.cursor() as cursor:
                user = await MultiTenantUser.get_by_id(
                    cursor, self.schema, user_id, self.tenant_id
                )
        
        if not user:
            raise InvalidTokenError("User not found")
        
        return UserContext(
            user_id=str(user.id),
            email=user.email,
            name=user.name,
            roles=user.roles,
            permissions=user.permissions,
            metadata=user.metadata
        )
```

## Performance Considerations

### Schema-per-Tenant Scaling Limits

```python
# performance_monitor.py
class TenantPerformanceMonitor:
    async def check_schema_count_impact(self):
        """Monitor performance impact of schema count"""
        tenants = await self.tenant_manager.list_tenants()
        tenant_count = len(tenants)
        
        # Test migration time
        start_time = time.time()
        await self._test_migration_all_schemas()
        migration_time = time.time() - start_time
        
        # Test backup time
        start_time = time.time()
        await self._test_backup_all_schemas()
        backup_time = time.time() - start_time
        
        metrics = {
            "tenant_count": tenant_count,
            "migration_time_seconds": migration_time,
            "backup_time_seconds": backup_time,
            "migration_time_per_tenant": migration_time / tenant_count,
            "estimated_max_tenants": self._estimate_max_tenants(migration_time, tenant_count)
        }
        
        return metrics
    
    def _estimate_max_tenants(self, migration_time: float, tenant_count: int) -> int:
        """Estimate maximum tenants based on acceptable migration time"""
        MAX_ACCEPTABLE_MIGRATION_TIME = 600  # 10 minutes
        time_per_tenant = migration_time / tenant_count
        return int(MAX_ACCEPTABLE_MIGRATION_TIME / time_per_tenant)
```

### Connection Pool Optimization

```python
# For schema-per-tenant, optimize connection pooling
class TenantConnectionPoolManager:
    def __init__(self, base_database_url: str):
        self.base_url = base_database_url
        self.tenant_pools = {}
    
    async def get_tenant_pool(self, tenant_id: str, max_size: int = 10):
        """Get connection pool for specific tenant"""
        if tenant_id not in self.tenant_pools:
            pool = AsyncConnectionPool(
                self.base_url,
                min_size=2,      # Smaller per-tenant pools
                max_size=max_size
            )
            self.tenant_pools[tenant_id] = pool
        
        return self.tenant_pools[tenant_id]
    
    async def close_all_pools(self):
        """Cleanup all tenant pools"""
        for pool in self.tenant_pools.values():
            await pool.close()
        self.tenant_pools.clear()
```

## Monitoring and Observability

### Tenant-Specific Metrics

```python
# tenant_metrics.py
from prometheus_client import Counter, Histogram, Gauge

# Tenant-aware metrics
tenant_requests = Counter(
    'tenant_requests_total',
    'Total requests per tenant',
    ['tenant_id', 'endpoint', 'status']
)

tenant_auth_duration = Histogram(
    'tenant_auth_duration_seconds',
    'Auth duration per tenant',
    ['tenant_id']
)

active_tenants = Gauge(
    'active_tenants_total',
    'Number of tenants with active sessions'
)

tenant_users = Gauge(
    'tenant_users_total',
    'Number of users per tenant',
    ['tenant_id']
)

@app.middleware("http")
async def tenant_metrics_middleware(request: Request, call_next):
    tenant_id = getattr(request.state, 'tenant_id', 'unknown')
    start_time = time.time()
    
    response = await call_next(request)
    
    # Record metrics with tenant label
    duration = time.time() - start_time
    tenant_requests.labels(
        tenant_id=tenant_id,
        endpoint=request.url.path,
        status='success' if response.status_code < 400 else 'error'
    ).inc()
    
    if request.url.path.startswith('/auth/'):
        tenant_auth_duration.labels(tenant_id=tenant_id).observe(duration)
    
    return response
```

### Tenant Health Monitoring

```python
# tenant_health.py
@router.get("/admin/tenants/health")
async def tenant_health_check():
    """Check health of all tenants"""
    tenants = await tenant_manager.list_tenants()
    health_status = {}
    
    for tenant_id in tenants:
        try:
            # Test tenant database access
            provider = await tenant_manager.get_tenant_provider(tenant_id)
            async with provider.db_pool.connection() as conn:
                await conn.execute("SELECT 1")
            
            # Count users
            async with conn.cursor() as cursor:
                await cursor.execute(
                    f"SELECT COUNT(*) FROM {provider.schema}.tb_user"
                )
                user_count = (await cursor.fetchone())[0]
            
            health_status[tenant_id] = {
                "status": "healthy",
                "user_count": user_count,
                "last_checked": datetime.utcnow().isoformat()
            }
            
        except Exception as e:
            health_status[tenant_id] = {
                "status": "unhealthy",
                "error": str(e),
                "last_checked": datetime.utcnow().isoformat()
            }
    
    overall_health = "healthy" if all(
        tenant["status"] == "healthy" 
        for tenant in health_status.values()
    ) else "degraded"
    
    return {
        "overall_status": overall_health,
        "tenants": health_status,
        "total_tenants": len(tenants)
    }
```

## Best Practices

### 1. Tenant Naming Conventions

```python
# Use consistent naming patterns
TENANT_NAMING_RULES = {
    "max_length": 32,
    "allowed_chars": "abcdefghijklmnopqrstuvwxyz0123456789_",
    "reserved_names": ["admin", "api", "www", "public", "system"],
    "pattern": r"^[a-z][a-z0-9_]*[a-z0-9]$"  # Must start with letter, end with alphanumeric
}

def validate_tenant_id(tenant_id: str) -> bool:
    """Validate tenant ID meets naming requirements"""
    import re
    
    if len(tenant_id) > TENANT_NAMING_RULES["max_length"]:
        return False
    
    if tenant_id in TENANT_NAMING_RULES["reserved_names"]:
        return False
    
    if not re.match(TENANT_NAMING_RULES["pattern"], tenant_id):
        return False
    
    return True
```

### 2. Tenant Lifecycle Management

```python
# tenant_lifecycle.py
class TenantLifecycleManager:
    async def provision_tenant(self, tenant_id: str, admin_user: dict):
        """Complete tenant provisioning process"""
        # 1. Validate tenant ID
        if not validate_tenant_id(tenant_id):
            raise ValueError("Invalid tenant ID")
        
        # 2. Create schema and auth tables
        await self.tenant_manager.create_tenant_schema(tenant_id)
        
        # 3. Create initial admin user
        provider = await self.tenant_manager.get_tenant_provider(tenant_id)
        admin = MultiTenantUser(
            tenant_id=UUID(tenant_id),
            email=admin_user["email"],
            name=admin_user["name"],
            roles=["admin"],
            permissions=["*"]  # Full permissions
        )
        admin.set_password(admin_user["password"])
        
        async with provider.db_pool.connection() as conn:
            async with conn.cursor() as cursor:
                await admin.save(cursor, provider.schema)
                await conn.commit()
        
        # 4. Set up monitoring
        await self._setup_tenant_monitoring(tenant_id)
        
        # 5. Send welcome email
        await self._send_tenant_welcome_email(admin_user["email"], tenant_id)
        
        return {
            "tenant_id": tenant_id,
            "admin_user_id": str(admin.id),
            "status": "active"
        }
    
    async def deactivate_tenant(self, tenant_id: str):
        """Safely deactivate tenant"""
        # 1. Mark all users as inactive
        # 2. Revoke all sessions
        # 3. Stop accepting new requests
        # 4. Optionally preserve data for recovery period
        pass
    
    async def delete_tenant(self, tenant_id: str, confirmation: str):
        """Permanently delete tenant (DANGEROUS!)"""
        if confirmation != f"DELETE-{tenant_id}":
            raise ValueError("Confirmation string required")
        
        # 1. Final data export (if required)
        await self._export_tenant_data(tenant_id)
        
        # 2. Delete schema
        await self.tenant_manager.delete_tenant_schema(tenant_id)
        
        # 3. Remove monitoring
        await self._cleanup_tenant_monitoring(tenant_id)
```

### 3. Security Best Practices

```python
# tenant_security.py
class TenantSecurityPolicy:
    @staticmethod
    async def enforce_tenant_isolation(request: Request):
        """Ensure requests can't cross tenant boundaries"""
        tenant_id = request.state.tenant_id
        
        # Check if any parameters reference other tenants
        for param_name, param_value in request.query_params.items():
            if "tenant" in param_name.lower():
                if param_value != tenant_id:
                    raise HTTPException(403, "Cross-tenant access denied")
        
        # Check JSON body for tenant references
        if request.method in ["POST", "PUT", "PATCH"]:
            body = await request.json()
            if isinstance(body, dict):
                for key, value in body.items():
                    if "tenant" in key.lower() and value != tenant_id:
                        raise HTTPException(403, "Cross-tenant access denied")
    
    @staticmethod
    async def audit_tenant_access(tenant_id: str, user_id: str, action: str, resource: str):
        """Audit tenant access for compliance"""
        audit_event = {
            "timestamp": datetime.utcnow().isoformat(),
            "tenant_id": tenant_id,
            "user_id": user_id,
            "action": action,
            "resource": resource,
            "source": "tenant_access_audit"
        }
        
        # Log to tenant-specific audit trail
        logger = logging.getLogger(f"tenant.{tenant_id}.audit")
        logger.info("Tenant access", extra=audit_event)
```

## Conclusion

FraiseQL's multi-tenant authentication provides flexible options for different scales and security requirements:

- **Schema-per-Tenant**: Perfect isolation, ideal for <1,000 tenants
- **Shared Schema + RLS**: Better scalability, suitable for 1,000+ tenants

Choose based on your specific requirements for isolation, scale, and operational complexity. Both approaches maintain the security and performance benefits of FraiseQL's native authentication while providing the multi-tenancy your SaaS application needs.

## Next Steps

- **[Security Guide](security.md)** - Multi-tenant security considerations
- **[Deployment Guide](deployment.md)** - Production multi-tenant deployment
- **[API Reference](api-reference.md)** - Tenant-aware API endpoints