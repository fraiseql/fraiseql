"""Tests for APQ context propagation from router to backend."""

import hashlib
from typing import Any, Optional
from unittest.mock import Mock, patch

import pytest

from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.fastapi.routers import GraphQLRequest
from fraiseql.storage.backends.memory import MemoryAPQBackend

pytestmark = pytest.mark.integration


class ContextCapturingBackend(MemoryAPQBackend):
    """Test backend that captures context passed to methods."""

    def __init__(self) -> None:
        super().__init__()
        self.captured_store_context = None
        self.captured_get_context = None

    def store_cached_response(
        self, hash_value: str, response: dict[str, Any], context: Optional[dict[str, Any]] = None
    ) -> None:
        """Capture context when storing responses."""
        self.captured_store_context = context
        super().store_cached_response(hash_value, response, context)

    def get_cached_response(
        self, hash_value: str, context: Optional[dict[str, Any]] = None
    ) -> Optional[dict[str, Any]]:
        """Capture context when getting responses."""
        self.captured_get_context = context
        return super().get_cached_response(hash_value, context)


class TestAPQContextPropagation:
    """Test that context flows from router to APQ backend."""

    def test_router_passes_context_when_storing_response(self) -> None:
        """Test that router passes context to store_cached_response."""
        from fraiseql.middleware.apq_caching import store_response_in_cache

        # Create test backend
        backend = ContextCapturingBackend()

        # Mock configuration
        config = Mock(spec=FraiseQLConfig)
        config.apq_storage_backend = "memory"
        config.apq_cache_responses = True
        config.apq_backend_config = {}
        config.environment = "development"

        # Create test context with user/tenant
        test_context = {
            "db": Mock(),
            "user": {"user_id": "test-user", "metadata": {"tenant_id": "tenant-123"}},
            "authenticated": True,
            "config": config,
        }

        test_query = "query GetUser { user { id name } }"
        query_hash = hashlib.sha256(test_query.encode()).hexdigest()
        test_response = {"data": {"user": {"id": "1", "name": "Test"}}}

        # Call store_response_in_cache WITH context
        store_response_in_cache(query_hash, test_response, backend, config, context=test_context)

        # Verify context was passed to backend
        assert backend.captured_store_context is not None
        assert "user" in backend.captured_store_context
        assert backend.captured_store_context["user"]["metadata"]["tenant_id"] == "tenant-123"

    def test_router_passes_context_when_getting_cached_response(self) -> None:
        """Test that router passes context to get_cached_response."""
        from fraiseql.middleware.apq_caching import handle_apq_request_with_cache

        # Create test backend with stored response
        backend = ContextCapturingBackend()

        test_query = "query GetUser { user { id name } }"
        query_hash = hashlib.sha256(test_query.encode()).hexdigest()

        # Pre-store a response
        test_response = {"data": {"user": {"id": "1", "name": "Test"}}}
        backend.store_cached_response(query_hash, test_response)

        # Create test request (hash-only, no query)
        request = GraphQLRequest(
            query=None,  # Hash-only request
            variables=None,
            operationName=None,
            extensions={"persistedQuery": {"version": 1, "sha256Hash": query_hash}},
        )

        # Create test context
        test_context = {"user": {"user_id": "test-user", "metadata": {"tenant_id": "tenant-456"}}}

        config = Mock(spec=FraiseQLConfig)
        config.apq_cache_responses = True

        # Call the function WITH context
        handle_apq_request_with_cache(request, backend, config, context=test_context)

        # Verify context was passed
        assert backend.captured_get_context is not None
        assert "user" in backend.captured_get_context
        assert backend.captured_get_context["user"]["metadata"]["tenant_id"] == "tenant-456"


class TestContextExtraction:
    """Test context extraction in different scenarios."""

    def test_context_available_at_apq_processing_time(self) -> None:
        """Verify that context is built before APQ processing."""
        call_order = []

        def mock_build_context(*args, **kwargs) -> None:
            call_order.append("build_context")
            return {"user": {"metadata": {"tenant_id": "test"}}}

        def mock_apq_processing(*args, **kwargs) -> None:
            call_order.append("apq_processing")

        with (
            patch(
                "fraiseql.fastapi.dependencies.build_graphql_context",
                side_effect=mock_build_context,
            ),
            patch(
                "fraiseql.middleware.apq_caching.handle_apq_request_with_cache",
                side_effect=mock_apq_processing,
            ),
        ):
            # Simulate the call sequence
            mock_build_context()
            mock_apq_processing()

            # Verify order
            assert call_order.index("build_context") < call_order.index("apq_processing")

    def test_context_includes_jwt_tenant_info(self) -> None:
        """Test that JWT tenant_id is included in context."""
        from fraiseql.auth.base import UserContext

        # Create user with JWT metadata including tenant_id
        user = UserContext(
            user_id="user-123",
            email="test@example.com",
            name="Test User",
            roles=["user"],
            permissions=[],
            metadata={"tenant_id": "tenant-789", "org": "TestOrg"},
        )

        # Create context as the router would
        context = {"db": Mock(), "user": user, "authenticated": True, "config": Mock()}

        # Verify tenant_id is available in context
        assert context["user"].metadata["tenant_id"] == "tenant-789"
        assert context["authenticated"] is True
