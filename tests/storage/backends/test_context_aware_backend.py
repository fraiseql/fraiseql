"""Tests for context-aware APQ backend functionality."""

from fraiseql.storage.backends.memory import MemoryAPQBackend


class TestContextAwareAPQBackend:
    """Test that APQ backends accept and use optional context parameter."""

    def test_store_cached_response_accepts_context(self) -> None:
        """Test that store_cached_response accepts an optional context parameter."""
        backend = MemoryAPQBackend()

        test_hash = "abc123"
        test_response = {"data": {"user": {"id": "1", "name": "Test"}}}
        test_context = {"user": {"user_id": "user-123", "metadata": {"tenant_id": "tenant-456"}}}

        # Store with context
        backend.store_cached_response(test_hash, test_response, context=test_context)

        # Retrieve with same context to get tenant-specific cache
        stored = backend.get_cached_response(test_hash, context=test_context)
        assert stored is not None

    def test_get_cached_response_accepts_context(self) -> None:
        """Test that get_cached_response accepts an optional context parameter."""
        backend = MemoryAPQBackend()

        test_hash = "def456"
        test_response = {"data": {"orders": [{"id": "1"}]}}
        test_context = {"user": {"user_id": "user-789", "metadata": {"tenant_id": "tenant-012"}}}

        # Store without context (global cache)
        backend.store_cached_response(test_hash, test_response)

        # Query with context (won't find global cache)
        cached_with_context = backend.get_cached_response(test_hash, context=test_context)
        assert cached_with_context is None

        # Query without context (finds global cache)
        cached_global = backend.get_cached_response(test_hash)
        assert cached_global == test_response

    def test_backward_compatibility_without_context(self) -> None:
        """Test that existing code without context still works."""
        backend = MemoryAPQBackend()

        test_hash = "ghi789"
        test_response = {"data": {"products": []}}

        # Works without context
        backend.store_cached_response(test_hash, test_response)
        cached = backend.get_cached_response(test_hash)

        assert cached == test_response

    def test_base_class_signature_supports_context(self) -> None:
        """Test that the abstract base class defines context parameter."""
        import inspect

        from fraiseql.storage.backends.base import APQStorageBackend

        # Check method signatures include context parameter
        store_sig = inspect.signature(APQStorageBackend.store_cached_response)
        get_sig = inspect.signature(APQStorageBackend.get_cached_response)

        # Verify context parameter exists
        assert "context" in store_sig.parameters
        assert "context" in get_sig.parameters

    def test_context_extraction_helpers(self) -> None:
        """Test the extract_tenant_id helper method."""
        backend = MemoryAPQBackend()

        # JWT metadata style
        context1 = {"user": {"metadata": {"tenant_id": "tenant-1"}}}
        assert backend.extract_tenant_id(context1) == "tenant-1"

        # Direct on user
        context2 = {"user": {"tenant_id": "tenant-2"}}
        assert backend.extract_tenant_id(context2) == "tenant-2"

        # Direct in context
        context3 = {"tenant_id": "tenant-3"}
        assert backend.extract_tenant_id(context3) == "tenant-3"

        # No tenant_id
        context4 = {"user": {"id": "123"}}
        assert backend.extract_tenant_id(context4) is None

        # None context
        assert backend.extract_tenant_id(None) is None

    def test_cache_key_generation_with_tenant(self) -> None:
        """Test that base backend implements tenant isolation."""
        backend = MemoryAPQBackend()

        test_hash = "query123"
        test_response = {"data": {"result": "test"}}

        # Store with tenant A
        context_a = {"user": {"metadata": {"tenant_id": "tenant-a"}}}
        backend.store_cached_response(test_hash, test_response, context=context_a)

        # Store different response with tenant B
        response_b = {"data": {"result": "different"}}
        context_b = {"user": {"metadata": {"tenant_id": "tenant-b"}}}
        backend.store_cached_response(test_hash, response_b, context=context_b)

        # Base backend implements tenant isolation
        cached_a = backend.get_cached_response(test_hash, context=context_a)
        cached_b = backend.get_cached_response(test_hash, context=context_b)

        # Each tenant gets their own response
        assert cached_a == test_response
        assert cached_b == response_b
        assert cached_a != cached_b
