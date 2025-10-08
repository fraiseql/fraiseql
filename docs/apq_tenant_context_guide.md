# APQ Tenant Context Support Guide

## Overview

FraiseQL provides built-in tenant-aware caching for Automatic Persisted Queries (APQ), enabling secure multi-tenant applications with automatic response isolation.

## How It Works

### Automatic Tenant Isolation

When you pass context with tenant information to APQ operations, FraiseQL automatically:

- Generates tenant-specific cache keys
- Isolates cached responses between tenants
- Prevents cross-tenant data leakage

```python
# Context with tenant information
context = {
    "user": {
        "metadata": {"tenant_id": "acme-corp"}
    }
}

# Responses are automatically isolated by tenant
cached_response = backend.get_cached_response(hash_value, context=context)
```

### Supported Context Structures

FraiseQL's `extract_tenant_id()` method supports multiple context patterns:

```python
# JWT metadata style (recommended)
context = {"user": {"metadata": {"tenant_id": "tenant-123"}}}

# Direct on user object
context = {"user": {"tenant_id": "tenant-123"}}

# Direct in context
context = {"tenant_id": "tenant-123"}
```

## Built-in Backend Support

### MemoryAPQBackend

The in-memory backend automatically implements tenant isolation:

```python
from fraiseql.storage.backends.memory import MemoryAPQBackend

backend = MemoryAPQBackend()

# Each tenant's data is isolated
backend.store_cached_response(hash, response_a, context={"user": {"metadata": {"tenant_id": "tenant-a"}}})
backend.store_cached_response(hash, response_b, context={"user": {"metadata": {"tenant_id": "tenant-b"}}})

# Tenants can only access their own cached responses
```

### PostgreSQLAPQBackend

The PostgreSQL backend stores tenant_id in the database:

```sql
CREATE TABLE apq_responses (
    hash VARCHAR(64) NOT NULL,
    tenant_id VARCHAR(255),
    response JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    PRIMARY KEY (hash, COALESCE(tenant_id, ''))
);
```

## Configuration

```python
from fraiseql import FraiseQLConfig, create_fraiseql_app

config = FraiseQLConfig(
    database_url="postgresql://localhost/myapp",
    apq_storage_backend="memory",  # or "postgresql"
    apq_cache_responses=True,
    apq_cache_ttl=3600,
)

app = create_fraiseql_app(config)
```

## Adding Statistics Tracking

You can extend the base backends to add custom functionality:

```python
from fraiseql.storage.backends.memory import MemoryAPQBackend

class APQBackendWithStats(MemoryAPQBackend):
    """Backend with cache statistics."""

    def __init__(self):
        super().__init__()
        self._stats = {"hits": {}, "misses": {}}

    def get_cached_response(self, hash_value, context=None):
        tenant_id = self.extract_tenant_id(context) or "global"
        response = super().get_cached_response(hash_value, context)

        if response:
            self._stats["hits"][tenant_id] = self._stats["hits"].get(tenant_id, 0) + 1
        else:
            self._stats["misses"][tenant_id] = self._stats["misses"].get(tenant_id, 0) + 1

        return response
```

## Security Best Practices

### 1. Always Validate Tenant Context

Ensure tenant_id comes from authenticated, trusted sources:

```python
@app.middleware("http")
async def add_tenant_context(request, call_next):
    # Decode and validate JWT
    token = request.headers.get("Authorization", "").replace("Bearer ", "")
    payload = jwt.decode(token, SECRET_KEY, algorithms=["HS256"])

    # Add validated tenant_id to context
    request.state.tenant_id = payload.get("tenant_id")

    response = await call_next(request)
    return response
```

### 2. Test Tenant Isolation

Write tests to verify tenant isolation:

```python
def test_tenant_isolation():
    backend = MemoryAPQBackend()

    # Store sensitive data for tenant A
    context_a = {"user": {"metadata": {"tenant_id": "tenant-a"}}}
    backend.store_cached_response("query123", {"secrets": "A"}, context=context_a)

    # Tenant B cannot access it
    context_b = {"user": {"metadata": {"tenant_id": "tenant-b"}}}
    leaked = backend.get_cached_response("query123", context=context_b)

    assert leaked is None, "Tenant isolation breach!"
```

## Performance Considerations

### Cache Hit Rates

Tenant-specific caching results in lower cache hit rates compared to global caching, but provides essential security isolation:

```
Global caching: N queries cached (shared across all tenants)
Tenant caching: N queries × M tenants cached (isolated per tenant)
```

### Memory Management

For high-tenant-count applications, consider:

- Implementing cache eviction policies (LRU, TTL)
- Using external cache stores (Redis, PostgreSQL)
- Monitoring memory usage per tenant

## Migration from Custom Implementations

If you previously implemented custom tenant-aware backends, you can now use the built-in functionality:

### Before (Custom Implementation Required)
```python
class TenantAwareBackend(MemoryAPQBackend):
    def _get_cache_key(self, hash_value, context=None):
        # Custom logic needed
        ...
```

### After (Built-in Support)
```python
# Just use the base backend directly
backend = MemoryAPQBackend()
# Tenant isolation works automatically!
```

## Example: Multi-Tenant SaaS Application

```python
from fraiseql import FraiseQLConfig, create_fraiseql_app
from fraiseql.storage.backends.memory import MemoryAPQBackend

# Configuration
config = FraiseQLConfig(
    database_url="postgresql://localhost/saas_app",
    apq_storage_backend="memory",
    apq_cache_responses=True,
    apq_cache_ttl=3600,
)

# Create app
app = create_fraiseql_app(config)

# Add middleware to extract tenant from JWT
@app.middleware("http")
async def add_tenant_context(request, call_next):
    # Decode JWT and extract tenant_id
    token = request.headers.get("Authorization", "")
    if token.startswith("Bearer "):
        payload = decode_jwt(token[7:])
        request.state.tenant_id = payload.get("tenant_id")

    response = await call_next(request)
    return response
```

## Troubleshooting

### Tenant ID Not Being Extracted

Verify your context structure matches supported patterns:

```python
# Debug tenant extraction
backend = MemoryAPQBackend()
tenant_id = backend.extract_tenant_id(your_context)
print(f"Extracted tenant_id: {tenant_id}")
```

### Cache Not Isolated

Ensure you're passing context to APQ operations:

```python
# Wrong: No context provided
response = backend.get_cached_response(hash_value)

# Correct: Context with tenant_id
response = backend.get_cached_response(hash_value, context=context)
```

## Support

For issues or questions:

- GitHub Issues: https://github.com/fraiseql/fraiseql/issues
- Documentation: https://fraiseql.dev/docs/apq
- Examples: `/examples/apq_multi_tenant.py`
