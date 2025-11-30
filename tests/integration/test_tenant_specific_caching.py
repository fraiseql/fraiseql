"""Tests for tenant-specific APQ response caching."""

import hashlib

import pytest

from fraiseql.storage.backends.memory import MemoryAPQBackend

pytestmark = pytest.mark.integration


class TestTenantSpecificCaching:
    """Test that responses are properly isolated by tenant."""

    def test_different_tenants_get_different_cached_responses(self) -> None:
        """Test that each tenant gets their own cached response."""
        backend = MemoryAPQBackend()

        # Same query hash for both tenants
        query = "query GetData { data { id name } }"
        query_hash = hashlib.sha256(query.encode()).hexdigest()

        # Tenant A's response
        response_a = {"data": {"result": "Tenant A Data"}}
        context_a = {"user": {"metadata": {"tenant_id": "tenant-a"}}}

        # Tenant B's response
        response_b = {"data": {"result": "Tenant B Data"}}
        context_b = {"user": {"metadata": {"tenant_id": "tenant-b"}}}

        # Store responses for both tenants
        backend.store_cached_response(query_hash, response_a, context=context_a)
        backend.store_cached_response(query_hash, response_b, context=context_b)

        # Each tenant should get their own response
        cached_a = backend.get_cached_response(query_hash, context=context_a)
        cached_b = backend.get_cached_response(query_hash, context=context_b)

        assert cached_a == response_a, "Tenant A should get their own data"
        assert cached_b == response_b, "Tenant B should get their own data"
        assert cached_a != cached_b, "Different tenants should have different responses"

    def test_no_context_uses_global_cache(self) -> None:
        """Test that requests without context use global cache."""
        backend = MemoryAPQBackend()

        query_hash = "test123"
        global_response = {"data": {"result": "Global"}}

        # Store without context (global)
        backend.store_cached_response(query_hash, global_response, context=None)

        # Retrieve without context should get global
        cached = backend.get_cached_response(query_hash, context=None)
        assert cached == global_response

        # Tenant-specific request should NOT get global cache
        tenant_context = {"user": {"metadata": {"tenant_id": "tenant-x"}}}
        tenant_cached = backend.get_cached_response(query_hash, context=tenant_context)
        assert tenant_cached is None, "Tenant should not see global cache"

    def test_tenant_isolation_prevents_data_leakage(self) -> None:
        """Test that one tenant cannot access another tenant's cached data."""
        backend = MemoryAPQBackend()

        query_hash = "sensitive123"

        # Tenant A stores sensitive data
        sensitive_data = {"data": {"secrets": ["password123", "api_key_xyz"]}}
        context_a = {"user": {"metadata": {"tenant_id": "tenant-a"}}}
        backend.store_cached_response(query_hash, sensitive_data, context=context_a)

        # Tenant B tries to access the same hash
        context_b = {"user": {"metadata": {"tenant_id": "tenant-b"}}}
        leaked = backend.get_cached_response(query_hash, context=context_b)

        assert leaked is None, "Tenant B should not see Tenant A's data"

    def test_cache_invalidation_per_tenant(self) -> None:
        """Test that cache can be invalidated per tenant."""
        backend = MemoryAPQBackend()

        query_hash = "data123"

        # Both tenants cache responses
        context_a = {"user": {"metadata": {"tenant_id": "tenant-a"}}}
        context_b = {"user": {"metadata": {"tenant_id": "tenant-b"}}}

        backend.store_cached_response(query_hash, {"data": "A"}, context=context_a)
        backend.store_cached_response(query_hash, {"data": "B"}, context=context_b)

        # Simulate invalidating tenant A's cache
        cache_key_a = backend._get_cache_key(query_hash, context_a)
        if cache_key_a in backend._response_storage:
            del backend._response_storage[cache_key_a]

        # Tenant A's cache is gone
        assert backend.get_cached_response(query_hash, context=context_a) is None

        # Tenant B's cache remains
        assert backend.get_cached_response(query_hash, context=context_b) == {"data": "B"}

    def test_tenant_id_extraction_variations(self) -> None:
        """Test that various context structures work correctly."""
        backend = MemoryAPQBackend()

        query_hash = "test456"
        test_response = {"data": "test"}

        # Test different context patterns
        contexts = [
            # JWT metadata style
            {"user": {"metadata": {"tenant_id": "jwt-tenant"}}},
            # Direct on user
            {"user": {"tenant_id": "direct-tenant"}},
            # Direct in context
            {"tenant_id": "context-tenant"},
        ]

        for ctx in contexts:
            backend.store_cached_response(query_hash, test_response, context=ctx)
            cached = backend.get_cached_response(query_hash, context=ctx)
            assert cached == test_response, f"Failed for context: {ctx}"

    def test_memory_backend_with_tenant_awareness(self) -> None:
        """Test that MemoryAPQBackend has built-in tenant isolation."""
        backend = MemoryAPQBackend()

        query_hash = "regular123"

        # Different tenants store different responses
        context_a = {"user": {"metadata": {"tenant_id": "tenant-a"}}}
        context_b = {"user": {"metadata": {"tenant_id": "tenant-b"}}}

        response_a = {"data": "A"}
        response_b = {"data": "B"}

        # Store for tenant A
        backend.store_cached_response(query_hash, response_a, context=context_a)

        # Store for tenant B (doesn't overwrite A due to tenant isolation)
        backend.store_cached_response(query_hash, response_b, context=context_b)

        # Each tenant gets their own response
        cached_a = backend.get_cached_response(query_hash, context=context_a)
        cached_b = backend.get_cached_response(query_hash, context=context_b)

        assert cached_a == response_a, "Tenant A gets their own response"
        assert cached_b == response_b, "Tenant B gets their own response"
        assert cached_a != cached_b, "Tenants are isolated"


class TestBackwardCompatibility:
    """Ensure that existing code without context still works."""

    def test_backend_works_without_context(self) -> None:
        """Test that backends work when no context is provided."""
        backend = MemoryAPQBackend()

        query_hash = "nocontext123"
        response = {"data": "test"}

        # Store and retrieve without context
        backend.store_cached_response(query_hash, response)
        cached = backend.get_cached_response(query_hash)

        assert cached == response, "Should work without context"

    def test_mixed_context_and_no_context(self) -> None:
        """Test that context and no-context calls don't interfere."""
        backend = MemoryAPQBackend()

        query_hash = "mixed123"

        # Store without context
        global_response = {"data": "global"}
        backend.store_cached_response(query_hash, global_response)

        # Store with context
        tenant_response = {"data": "tenant"}
        tenant_context = {"user": {"metadata": {"tenant_id": "tenant-1"}}}
        backend.store_cached_response(query_hash, tenant_response, context=tenant_context)

        # Retrieve without context gets global
        assert backend.get_cached_response(query_hash) == global_response

        # Retrieve with context gets tenant-specific
        assert backend.get_cached_response(query_hash, context=tenant_context) == tenant_response
