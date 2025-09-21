"""Tests for context-aware APQ backend functionality.

Phase 1 RED: These tests should fail initially, defining the expected behavior
for passing context to APQ backend methods.
"""

import json
from typing import Any, Dict, Optional
from unittest.mock import Mock, patch

import pytest

from fraiseql.storage.backends.base import APQStorageBackend
from fraiseql.storage.backends.memory import MemoryAPQBackend


class TestContextAwareAPQBackend:
    """Test suite for context-aware APQ backend methods."""

    def test_store_cached_response_accepts_context(self):
        """Test that store_cached_response can accept an optional context parameter."""
        backend = MemoryAPQBackend()

        test_hash = "abc123"
        test_response = {"data": {"user": {"id": "1", "name": "Test"}}}
        test_context = {
            "user": {
                "user_id": "user-123",
                "metadata": {"tenant_id": "tenant-456"}
            }
        }

        # This should not raise TypeError
        backend.store_cached_response(test_hash, test_response, context=test_context)

        # Verify it was stored - must use same context to retrieve tenant-specific cache
        stored = backend.get_cached_response(test_hash, context=test_context)
        assert stored is not None

    def test_get_cached_response_accepts_context(self):
        """Test that get_cached_response can accept an optional context parameter."""
        backend = MemoryAPQBackend()

        test_hash = "def456"
        test_response = {"data": {"orders": [{"id": "1"}]}}
        test_context = {
            "user": {
                "user_id": "user-789",
                "metadata": {"tenant_id": "tenant-012"}
            }
        }

        # Store without context (global cache)
        backend.store_cached_response(test_hash, test_response)

        # This should not raise TypeError
        cached_with_context = backend.get_cached_response(test_hash, context=test_context)

        # With tenant isolation, context query won't find global cache
        assert cached_with_context is None

        # But global query will find it
        cached_global = backend.get_cached_response(test_hash)
        assert cached_global == test_response

    def test_backward_compatibility_without_context(self):
        """Test that existing code without context still works."""
        backend = MemoryAPQBackend()

        test_hash = "ghi789"
        test_response = {"data": {"products": []}}

        # These should work without context (backward compatibility)
        backend.store_cached_response(test_hash, test_response)
        cached = backend.get_cached_response(test_hash)

        assert cached == test_response

    def test_base_class_signature_supports_context(self):
        """Test that the base class abstract methods support context parameter."""

        # Check method signatures using inspection
        import inspect

        # For APQStorageBackend (the main base class)
        store_sig = inspect.signature(APQStorageBackend.store_cached_response)
        get_sig = inspect.signature(APQStorageBackend.get_cached_response)

        # Should have context parameter
        # Currently fails: 'context' not in parameters
        assert 'context' in store_sig.parameters
        assert 'context' in get_sig.parameters

        # Context should be optional (default None)
        assert store_sig.parameters['context'].default is None
        assert get_sig.parameters['context'].default is None

    def test_context_extraction_helpers(self):
        """Test helper methods for extracting tenant_id from context."""
        backend = MemoryAPQBackend()

        # Test various context structures
        contexts = [
            # JWT metadata style (Auth0)
            {
                "user": {
                    "user_id": "123",
                    "metadata": {"tenant_id": "tenant-a"}
                }
            },
            # Direct tenant_id on user
            {
                "user": {
                    "user_id": "456",
                    "tenant_id": "tenant-b"
                }
            },
            # Direct tenant_id in context
            {
                "tenant_id": "tenant-c",
                "user": {"user_id": "789"}
            },
            # No tenant_id
            {
                "user": {"user_id": "000"}
            },
            # No user
            {},
            # None context
            None
        ]

        expected_tenant_ids = [
            "tenant-a",
            "tenant-b",
            "tenant-c",
            None,
            None,
            None
        ]

        # This method should exist
        # Currently fails: AttributeError: 'MemoryAPQBackend' object has no attribute 'extract_tenant_id'
        for context, expected in zip(contexts, expected_tenant_ids):
            tenant_id = backend.extract_tenant_id(context)
            assert tenant_id == expected, f"Failed for context: {context}"

    def test_postgresql_backend_accepts_context(self):
        """Test that PostgreSQL backend also accepts context."""
        try:
            import psycopg2
        except ImportError:
            pytest.skip("psycopg2 not installed")

        from fraiseql.storage.backends.postgresql import PostgreSQLAPQBackend

        # Mock the connection
        with patch('psycopg2.connect') as mock_connect:
            mock_conn = Mock()
            mock_cursor = Mock()
            mock_conn.cursor.return_value = mock_cursor
            mock_connect.return_value = mock_conn

            backend = PostgreSQLAPQBackend(connection_string="postgresql://test")

            test_hash = "xyz999"
            test_response = {"data": {"test": True}}
            test_context = {"user": {"metadata": {"tenant_id": "tenant-xyz"}}}

            # These should not raise TypeError
            # Currently fails: unexpected keyword argument 'context'
            backend.store_cached_response(test_hash, test_response, context=test_context)
            backend.get_cached_response(test_hash, context=test_context)

    def test_cache_key_generation_with_tenant(self):
        """Test that base backend implements tenant isolation."""
        backend = MemoryAPQBackend()

        test_hash = "query123"
        test_response = {"data": {"result": "test"}}

        # Store with tenant A
        context_a = {"user": {"metadata": {"tenant_id": "tenant-a"}}}
        backend.store_cached_response(test_hash, test_response, context=context_a)

        # Store different response with tenant B (same query hash)
        response_b = {"data": {"result": "different"}}
        context_b = {"user": {"metadata": {"tenant_id": "tenant-b"}}}
        backend.store_cached_response(test_hash, response_b, context=context_b)

        # Base backend now implements tenant isolation
        cached_a = backend.get_cached_response(test_hash, context=context_a)
        cached_b = backend.get_cached_response(test_hash, context=context_b)

        # Each tenant should get their own response
        assert cached_a == test_response
        assert cached_b == response_b
        assert cached_a != cached_b


class TestContextPropagationFromRouter:
    """Test that context is properly passed from router to APQ backend.

    These tests belong to Phase 2 but are included here to show the full picture.
    """

    @pytest.mark.skip(reason="Phase 2 - Router integration")
    def test_router_passes_context_to_store_cached_response(self):
        """Test that the router passes context when storing cached responses."""
        # This will be implemented in Phase 2
        pass

    @pytest.mark.skip(reason="Phase 2 - Router integration")
    def test_router_passes_context_to_get_cached_response(self):
        """Test that the router passes context when getting cached responses."""
        # This will be implemented in Phase 2
        pass
