#!/usr/bin/env python
"""
Example: Multi-tenant APQ with tenant-specific caching.

This example demonstrates how to implement tenant-aware APQ caching
for a multi-tenant SaaS application.
"""

import hashlib
import json
from typing import Any, Dict, Optional

from fraiseql import FraiseQLConfig, create_fraiseql_app
from fraiseql.storage.backends.base import APQStorageBackend
from fraiseql.storage.backends.memory import MemoryAPQBackend


class TenantAwareAPQBackend(MemoryAPQBackend):
    """
    APQ backend with tenant-specific response caching.

    This backend ensures that each tenant's cached responses are isolated,
    preventing data leakage between tenants.
    """

    def __init__(self):
        super().__init__()
        self._stats = {
            "cache_hits": {},
            "cache_misses": {},
            "cache_stores": {},
        }

    def _get_cache_key(self, hash_value: str, context: Optional[Dict[str, Any]] = None) -> str:
        """
        Generate cache key that includes tenant_id for isolation.

        Args:
            hash_value: SHA256 hash of the GraphQL query
            context: Request context containing user/tenant information

        Returns:
            Cache key with format "tenant_id:hash" or just "hash" for global
        """
        if context:
            tenant_id = self.extract_tenant_id(context)
            if tenant_id:
                return f"{tenant_id}:{hash_value}"
        return f"global:{hash_value}"

    def get_cached_response(
        self, hash_value: str, context: Optional[Dict[str, Any]] = None
    ) -> Optional[Dict[str, Any]]:
        """Get cached response with tenant isolation."""
        cache_key = self._get_cache_key(hash_value, context)
        response = self._response_storage.get(cache_key)

        # Track statistics
        tenant_id = self.extract_tenant_id(context) if context else "global"
        if response:
            self._stats["cache_hits"][tenant_id] = self._stats["cache_hits"].get(tenant_id, 0) + 1
            print(f"Cache HIT for tenant '{tenant_id}': {hash_value[:8]}...")
        else:
            self._stats["cache_misses"][tenant_id] = (
                self._stats["cache_misses"].get(tenant_id, 0) + 1
            )
            print(f"Cache MISS for tenant '{tenant_id}': {hash_value[:8]}...")

        return response

    def store_cached_response(
        self, hash_value: str, response: Dict[str, Any], context: Optional[Dict[str, Any]] = None
    ) -> None:
        """Store cached response with tenant isolation."""
        cache_key = self._get_cache_key(hash_value, context)
        self._response_storage[cache_key] = response

        # Track statistics
        tenant_id = self.extract_tenant_id(context) if context else "global"
        self._stats["cache_stores"][tenant_id] = self._stats["cache_stores"].get(tenant_id, 0) + 1
        print(f"Stored response for tenant '{tenant_id}': {hash_value[:8]}...")

    def get_stats(self) -> Dict[str, Any]:
        """Get cache statistics per tenant."""
        return self._stats

    def clear_tenant_cache(self, tenant_id: str) -> int:
        """
        Clear all cached responses for a specific tenant.

        Args:
            tenant_id: The tenant whose cache should be cleared

        Returns:
            Number of entries cleared
        """
        keys_to_delete = [
            key for key in self._response_storage.keys() if key.startswith(f"{tenant_id}:")
        ]

        for key in keys_to_delete:
            del self._response_storage[key]

        print(f"Cleared {len(keys_to_delete)} cache entries for tenant '{tenant_id}'")
        return len(keys_to_delete)


def simulate_multi_tenant_requests():
    """Simulate APQ requests from multiple tenants."""
    print("=" * 60)
    print("Multi-Tenant APQ Caching Example")
    print("=" * 60)

    # Create the backend
    backend = TenantAwareAPQBackend()

    # Simulate queries from different tenants
    queries = {
        "get_users": "query GetUsers { users { id name email } }",
        "get_products": "query GetProducts { products { id name price } }",
        "get_orders": "query GetOrders { orders { id status total } }",
    }

    # Calculate hashes
    query_hashes = {name: hashlib.sha256(query.encode()).hexdigest() for name, query in queries.items()}

    # Simulate requests from three tenants
    tenants = [
        {"tenant_id": "acme-corp", "name": "ACME Corporation"},
        {"tenant_id": "globex-inc", "name": "Globex Inc"},
        {"tenant_id": "initech", "name": "Initech"},
    ]

    print("\n--- Phase 1: Initial Requests (Cache Misses) ---")
    for tenant in tenants:
        context = {"user": {"metadata": {"tenant_id": tenant["tenant_id"]}}}

        for query_name, query_hash in query_hashes.items():
            # First request - cache miss
            cached = backend.get_cached_response(query_hash, context)
            assert cached is None

            # Simulate executing query and storing response
            response = {
                "data": {
                    query_name: f"Data for {tenant['name']}",
                    "tenant": tenant["tenant_id"],
                }
            }
            backend.store_cached_response(query_hash, response, context)

    print("\n--- Phase 2: Repeated Requests (Cache Hits) ---")
    for tenant in tenants:
        context = {"user": {"metadata": {"tenant_id": tenant["tenant_id"]}}}

        for query_name, query_hash in query_hashes.items():
            # Second request - cache hit
            cached = backend.get_cached_response(query_hash, context)
            assert cached is not None
            assert cached["data"]["tenant"] == tenant["tenant_id"]

    print("\n--- Phase 3: Verify Tenant Isolation ---")
    # ACME tries to access Globex's cache (should fail)
    acme_context = {"user": {"metadata": {"tenant_id": "acme-corp"}}}
    globex_context = {"user": {"metadata": {"tenant_id": "globex-inc"}}}

    # Get the same query hash for both
    test_hash = query_hashes["get_users"]

    acme_response = backend.get_cached_response(test_hash, acme_context)
    globex_response = backend.get_cached_response(test_hash, globex_context)

    assert acme_response["data"]["tenant"] == "acme-corp"
    assert globex_response["data"]["tenant"] == "globex-inc"
    print("✅ Tenant isolation verified - no data leakage")

    print("\n--- Phase 4: Cache Invalidation ---")
    # Clear ACME's cache
    cleared = backend.clear_tenant_cache("acme-corp")

    # ACME's cache should be empty
    acme_cached = backend.get_cached_response(test_hash, acme_context)
    assert acme_cached is None

    # Other tenants' cache should remain
    globex_cached = backend.get_cached_response(test_hash, globex_context)
    assert globex_cached is not None
    print("✅ Selective cache invalidation working")

    print("\n--- Cache Statistics ---")
    stats = backend.get_stats()
    for tenant_id in ["acme-corp", "globex-inc", "initech"]:
        hits = stats["cache_hits"].get(tenant_id, 0)
        misses = stats["cache_misses"].get(tenant_id, 0)
        stores = stats["cache_stores"].get(tenant_id, 0)
        hit_rate = (hits / (hits + misses) * 100) if (hits + misses) > 0 else 0

        print(f"{tenant_id:12} - Hits: {hits:3}, Misses: {misses:3}, Hit Rate: {hit_rate:.1f}%")


def create_multi_tenant_app():
    """Create a FraiseQL app with multi-tenant APQ support."""
    config = FraiseQLConfig(
        database_url="postgresql://localhost/multi_tenant_db",
        apq_storage_backend="custom",
        apq_backend_config={
            "class": "examples.apq_multi_tenant.TenantAwareAPQBackend",
        },
        apq_cache_responses=True,
        apq_cache_ttl=3600,  # 1 hour
    )

    app = create_fraiseql_app(config)

    # Add middleware to extract tenant from JWT
    @app.middleware("http")
    async def add_tenant_context(request, call_next):
        """Extract tenant_id from JWT and add to request state."""
        # In production, decode JWT and extract tenant_id
        # For example:
        # token = request.headers.get("Authorization", "").replace("Bearer ", "")
        # payload = jwt.decode(token, SECRET_KEY)
        # request.state.tenant_id = payload.get("tenant_id")

        response = await call_next(request)
        return response

    return app


if __name__ == "__main__":
    # Run the simulation
    simulate_multi_tenant_requests()

    print("\n" + "=" * 60)
    print("Example Complete!")
    print("=" * 60)

    print("\nTo use in production:")
    print("1. Copy TenantAwareAPQBackend to your project")
    print("2. Configure FraiseQL to use your custom backend")
    print("3. Ensure JWT/auth adds tenant_id to context")
    print("4. Monitor cache hit rates per tenant")
    print("5. Implement cache eviction strategies as needed")
