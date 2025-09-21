# APQ Tenant Context Support Guide

## Overview

FraiseQL now supports passing request context to APQ (Automatic Persisted Queries) backends, enabling tenant-specific caching for multi-tenant applications.

## Features

### 1. Context-Aware Backend Methods

APQ backend methods now accept an optional `context` parameter:

```python
class APQStorageBackend(ABC):
    def get_cached_response(
        self, hash_value: str, context: Optional[Dict[str, Any]] = None
    ) -> Optional[Dict[str, Any]]:
        """Get cached response with optional context."""

    def store_cached_response(
        self, hash_value: str, response: Dict[str, Any], context: Optional[Dict[str, Any]] = None
    ) -> None:
        """Store cached response with optional context."""
```

### 2. Automatic Context Propagation

The router automatically passes context to APQ backends:

```python
# In your GraphQL request handler
context = build_graphql_context(request, db, user, config)

# Context is automatically passed to APQ backend
cached_response = handle_apq_request_with_cache(
    request, backend, config, context=context
)
```

### 3. Tenant ID Extraction Helper

The base backend class provides a helper to extract tenant_id from various context structures:

```python
def extract_tenant_id(self, context: Optional[Dict[str, Any]]) -> Optional[str]:
    """Extract tenant_id from context.

    Supports:
    - JWT metadata: context['user']['metadata']['tenant_id']
    - Direct on user: context['user']['tenant_id']
    - Direct in context: context['tenant_id']
    """
```

## Implementation Examples

### Basic Context-Aware Backend

```python
from fraiseql.storage.backends import APQStorageBackend

class MyAPQBackend(APQStorageBackend):
    def store_cached_response(self, hash_value: str, response: Dict[str, Any], context: Optional[Dict[str, Any]] = None):
        # Extract tenant_id if available
        tenant_id = self.extract_tenant_id(context) if context else None

        if tenant_id:
            # Store with tenant isolation
            self.cache[f"{tenant_id}:{hash_value}"] = response
        else:
            # Global cache
            self.cache[hash_value] = response
```

### Tenant-Specific Memory Backend

```python
class TenantAwareMemoryBackend(MemoryAPQBackend):
    """Memory backend with tenant-specific caching."""

    def _get_cache_key(self, hash_value: str, context: Optional[Dict[str, Any]] = None) -> str:
        """Generate cache key with tenant isolation."""
        if context:
            tenant_id = self.extract_tenant_id(context)
            if tenant_id:
                return f"{tenant_id}:{hash_value}"
        return hash_value

    def get_cached_response(self, hash_value: str, context: Optional[Dict[str, Any]] = None):
        cache_key = self._get_cache_key(hash_value, context)
        return self._response_storage.get(cache_key)

    def store_cached_response(self, hash_value: str, response: Dict[str, Any], context: Optional[Dict[str, Any]] = None):
        cache_key = self._get_cache_key(hash_value, context)
        self._response_storage[cache_key] = response
```

### PostgreSQL Backend with Tenant Support

```python
class PostgreSQLAPQBackend(APQStorageBackend):
    def store_cached_response(self, hash_value: str, response: Dict[str, Any], context: Optional[Dict[str, Any]] = None):
        tenant_id = self.extract_tenant_id(context) if context else None

        sql = """
            INSERT INTO apq_cache (hash, response, tenant_id, created_at)
            VALUES (%s, %s, %s, NOW())
            ON CONFLICT (hash, tenant_id) DO UPDATE
            SET response = EXCLUDED.response, updated_at = NOW()
        """

        self.execute(sql, (hash_value, json.dumps(response), tenant_id))

    def get_cached_response(self, hash_value: str, context: Optional[Dict[str, Any]] = None):
        tenant_id = self.extract_tenant_id(context) if context else None

        sql = "SELECT response FROM apq_cache WHERE hash = %s AND tenant_id = %s"
        result = self.fetch_one(sql, (hash_value, tenant_id))

        return json.loads(result[0]) if result else None
```

## Security Considerations

### Tenant Isolation

When implementing tenant-specific caching:

1. **Always validate tenant context**: Ensure the tenant_id comes from authenticated sources
2. **Separate cache keys**: Use tenant_id in cache keys to prevent cross-tenant access
3. **Test isolation**: Write tests to verify one tenant cannot access another's cached data

### Example Security Test

```python
def test_tenant_isolation():
    backend = TenantAwareMemoryBackend()

    # Tenant A stores sensitive data
    context_a = {"user": {"metadata": {"tenant_id": "tenant-a"}}}
    backend.store_cached_response("query123", {"secrets": "A"}, context=context_a)

    # Tenant B cannot access it
    context_b = {"user": {"metadata": {"tenant_id": "tenant-b"}}}
    leaked = backend.get_cached_response("query123", context=context_b)

    assert leaked is None, "Tenant isolation breach!"
```

## Backward Compatibility

The context parameter is optional and defaults to `None`, ensuring:

- Existing backends continue to work without modification
- Single-tenant applications don't need to provide context
- Gradual migration path for adding tenant support

```python
# Works without context (backward compatible)
backend.store_cached_response(hash_value, response)

# Works with context (new feature)
backend.store_cached_response(hash_value, response, context=context)
```

## Configuration

### Enabling Context-Aware Caching

```python
from fraiseql import FraiseQLConfig

config = FraiseQLConfig(
    # Enable APQ response caching
    apq_cache_responses=True,

    # Use custom backend with tenant support
    apq_storage_backend="custom",
    apq_backend_config={
        "class": "myapp.backends.TenantAwareAPQBackend",
        "connection_string": DATABASE_URL
    }
)
```

## Performance Considerations

### Cache Hit Rates

- **Global caching**: Higher hit rate (shared across tenants)
- **Tenant-specific**: Lower hit rate but better isolation
- **Hybrid approach**: Cache query text globally, responses per tenant

### Memory Usage

Tenant-specific caching increases memory usage:

```
Global: N queries cached
Tenant-specific: N queries × M tenants cached
```

Consider implementing cache eviction strategies:

```python
class LRUTenantCache(TenantAwareMemoryBackend):
    def __init__(self, max_size_per_tenant=1000):
        super().__init__()
        self.max_size_per_tenant = max_size_per_tenant

    def store_cached_response(self, hash_value: str, response: Dict[str, Any], context: Optional[Dict[str, Any]] = None):
        tenant_id = self.extract_tenant_id(context)

        if tenant_id:
            # Implement LRU eviction per tenant
            self._evict_if_needed(tenant_id)

        super().store_cached_response(hash_value, response, context)
```

## Migration Guide

### Step 1: Update FraiseQL

```bash
pip install --upgrade fraiseql>=0.9.3
```

### Step 2: Implement Context-Aware Backend

```python
# Extend your existing backend
class MyBackend(APQStorageBackend):
    def store_cached_response(self, hash_value: str, response: Dict[str, Any], context: Optional[Dict[str, Any]] = None):
        # Add context parameter to method signature
        # Existing logic still works
```

### Step 3: Add Tenant Logic (Optional)

```python
    def store_cached_response(self, hash_value: str, response: Dict[str, Any], context: Optional[Dict[str, Any]] = None):
        tenant_id = self.extract_tenant_id(context) if context else None

        if tenant_id:
            # New: tenant-specific caching
            key = f"{tenant_id}:{hash_value}"
        else:
            # Fallback: global caching
            key = hash_value

        self.cache[key] = response
```

### Step 4: Test

```python
def test_context_support():
    backend = MyBackend()

    # Test without context (backward compatibility)
    backend.store_cached_response("hash1", {"data": "test"})

    # Test with context (new feature)
    context = {"user": {"metadata": {"tenant_id": "tenant-1"}}}
    backend.store_cached_response("hash2", {"data": "test"}, context=context)
```

## Troubleshooting

### Context Not Reaching Backend

Check that you're using FraiseQL >= 0.9.3:

```python
import fraiseql
print(fraiseql.__version__)  # Should be >= 0.9.3
```

### Tenant ID Not Extracted

Verify your context structure matches supported patterns:

```python
# Supported patterns
context = {"user": {"metadata": {"tenant_id": "123"}}}  # JWT style
context = {"user": {"tenant_id": "123"}}                # Direct on user
context = {"tenant_id": "123"}                          # Direct in context
```

### Cache Not Isolated

Ensure you're using tenant_id in cache keys:

```python
# Wrong: same key for all tenants
cache[hash_value] = response

# Correct: tenant-specific keys
cache[f"{tenant_id}:{hash_value}"] = response
```

## Future Enhancements

Potential future improvements:

1. **Cache TTL per tenant**: Different cache durations based on tenant tier
2. **Cache warming**: Pre-populate cache for premium tenants
3. **Cache analytics**: Track hit rates per tenant
4. **Distributed caching**: Redis/Memcached backends with tenant support

## Contributing

To contribute to APQ tenant context support:

1. Fork the FraiseQL repository
2. Create a feature branch
3. Add tests for your changes
4. Submit a pull request

## Support

For issues or questions:

- GitHub Issues: https://github.com/fraiseql/fraiseql/issues
- Documentation: https://fraiseql.dev/docs/apq
- Examples: `/examples/apq_multi_tenant.py`
