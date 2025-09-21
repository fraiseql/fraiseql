"""Tests for tenant-specific APQ response caching.

Phase 3 RED: These tests verify that cached responses are properly
isolated by tenant when context is provided.
"""

import hashlib
from typing import Any, Dict, Optional

import pytest

from fraiseql.storage.backends.memory import MemoryAPQBackend


class TenantAwareMemoryBackend(MemoryAPQBackend):
    """Memory backend with tenant-specific caching support."""

    def _get_cache_key(self, hash_value: str, context: Optional[Dict[str, Any]] = None) -> str:
        """Generate cache key that includes tenant_id when available."""
        if context:
            tenant_id = self.extract_tenant_id(context)
            if tenant_id:
                return f"{tenant_id}:{hash_value}"
        return hash_value

    def get_cached_response(
        self, hash_value: str, context: Optional[Dict[str, Any]] = None
    ) -> Optional[Dict[str, Any]]:
        """Get cached response with tenant isolation."""
        cache_key = self._get_cache_key(hash_value, context)
        return self._response_storage.get(cache_key)

    def store_cached_response(
        self, hash_value: str, response: Dict[str, Any], context: Optional[Dict[str, Any]] = None
    ) -> None:
        """Store cached response with tenant isolation."""
        cache_key = self._get_cache_key(hash_value, context)
        self._response_storage[cache_key] = response


class TestTenantSpecificCaching:
    """Test that responses are properly isolated by tenant."""

    def test_different_tenants_get_different_cached_responses(self):
        """Test that each tenant gets their own cached response."""
        backend = TenantAwareMemoryBackend()

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

    def test_no_context_uses_global_cache(self):
        """Test that requests without context use global cache."""
        backend = TenantAwareMemoryBackend()

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

    def test_tenant_isolation_prevents_data_leakage(self):
        """Test that one tenant cannot access another tenant's cached data."""
        backend = TenantAwareMemoryBackend()

        query_hash = "sensitive123"

        # Tenant A stores sensitive data
        sensitive_data = {"data": {"secrets": ["password123", "api_key_xyz"]}}
        context_a = {"user": {"metadata": {"tenant_id": "tenant-a"}}}
        backend.store_cached_response(query_hash, sensitive_data, context=context_a)

        # Tenant B tries to access the same hash
        context_b = {"user": {"metadata": {"tenant_id": "tenant-b"}}}
        leaked = backend.get_cached_response(query_hash, context=context_b)

        assert leaked is None, "Tenant B should not see Tenant A's data"

    def test_cache_invalidation_per_tenant(self):
        """Test that cache can be invalidated per tenant."""
        backend = TenantAwareMemoryBackend()

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

    def test_tenant_id_extraction_variations(self):
        """Test that various context structures work correctly."""
        backend = TenantAwareMemoryBackend()

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

    def test_memory_backend_without_tenant_awareness(self):
        """Test that regular MemoryAPQBackend ignores context (Phase 1-2 behavior)."""
        backend = MemoryAPQBackend()  # Regular backend, not tenant-aware

        query_hash = "regular123"

        # Different tenants store different responses
        context_a = {"user": {"metadata": {"tenant_id": "tenant-a"}}}
        context_b = {"user": {"metadata": {"tenant_id": "tenant-b"}}}

        response_a = {"data": "A"}
        response_b = {"data": "B"}

        # Store for tenant A
        backend.store_cached_response(query_hash, response_a, context=context_a)

        # Store for tenant B (overwrites A because context is ignored)
        backend.store_cached_response(query_hash, response_b, context=context_b)

        # Both get the same (last stored) response
        cached_a = backend.get_cached_response(query_hash, context=context_a)
        cached_b = backend.get_cached_response(query_hash, context=context_b)

        # This is current behavior - no tenant isolation
        assert cached_a == cached_b == response_b, "Regular backend doesn't isolate by tenant"


class TestBackwardCompatibility:
    """Ensure that existing code without context still works."""

    def test_backend_works_without_context(self):
        """Test that backends work when no context is provided."""
        backend = TenantAwareMemoryBackend()

        query_hash = "nocontext123"
        response = {"data": "test"}

        # Store and retrieve without context
        backend.store_cached_response(query_hash, response)
        cached = backend.get_cached_response(query_hash)

        assert cached == response, "Should work without context"

    def test_mixed_context_and_no_context(self):
        """Test that context and no-context calls don't interfere."""
        backend = TenantAwareMemoryBackend()

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
