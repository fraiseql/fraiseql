"""Tests for APQ context propagation from router to backend.

Phase 2 RED: These tests verify that context is properly passed from
the router to APQ backend methods when processing requests.
"""

import hashlib
import json
from typing import Any, Dict, Optional
from unittest.mock import AsyncMock, Mock, patch

import pytest
from httpx import AsyncClient

from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.fastapi.routers import GraphQLRequest
from fraiseql.storage.backends.base import APQStorageBackend
from fraiseql.storage.backends.memory import MemoryAPQBackend


class ContextCapturingBackend(MemoryAPQBackend):
    """Test backend that captures context passed to methods."""

    def __init__(self):
        super().__init__()
        self.captured_store_context = None
        self.captured_get_context = None

    def store_cached_response(
        self, hash_value: str, response: Dict[str, Any], context: Optional[Dict[str, Any]] = None
    ) -> None:
        """Capture context when storing responses."""
        self.captured_store_context = context
        super().store_cached_response(hash_value, response, context)

    def get_cached_response(
        self, hash_value: str, context: Optional[Dict[str, Any]] = None
    ) -> Optional[Dict[str, Any]]:
        """Capture context when getting responses."""
        self.captured_get_context = context
        return super().get_cached_response(hash_value, context)


class TestAPQContextPropagation:
    """Test that context flows from router to APQ backend."""

    def test_router_passes_context_when_storing_response(self):
        """Test that router passes context to store_cached_response."""
        from fraiseql.fastapi.routers import handle_graphql_request

        # Create test backend
        backend = ContextCapturingBackend()

        # Mock configuration
        config = Mock(spec=FraiseQLConfig)
        config.apq_storage_backend = "memory"
        config.apq_cache_responses = True
        config.apq_backend_config = {}
        config.environment = "development"

        # Create test request with APQ
        test_query = "query GetUser { user { id name } }"
        query_hash = hashlib.sha256(test_query.encode()).hexdigest()

        request = GraphQLRequest(
            query=test_query,
            variables=None,
            operationName=None,
            extensions={
                "persistedQuery": {
                    "version": 1,
                    "sha256Hash": query_hash
                }
            }
        )

        # Create test context with user/tenant
        test_context = {
            "db": Mock(),
            "user": {
                "user_id": "test-user",
                "metadata": {"tenant_id": "tenant-123"}
            },
            "authenticated": True,
            "config": config
        }

        # Mock the APQ backend getter to return our test backend
        with patch('fraiseql.middleware.apq_caching.get_apq_backend', return_value=backend):
            with patch('fraiseql.fastapi.routers.build_graphql_context', return_value=test_context):
                # This test should fail initially because context isn't passed
                # Expected: backend.captured_store_context contains user/tenant info
                # Actual: backend.captured_store_context is None or doesn't have user info

                # Simulate router handling the request
                from fraiseql.fastapi import routers

                # Mock the router's internal APQ handling
                # After execution, store_cached_response should be called with context

                # This assertion should fail in RED phase
                assert backend.captured_store_context is not None, "Context was not passed to store_cached_response"
                assert "user" in backend.captured_store_context, "User not in context passed to backend"
                assert backend.captured_store_context["user"]["metadata"]["tenant_id"] == "tenant-123"

    def test_router_passes_context_when_getting_cached_response(self):
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
            extensions={
                "persistedQuery": {
                    "version": 1,
                    "sha256Hash": query_hash
                }
            }
        )

        # Create test context
        test_context = {
            "user": {
                "user_id": "test-user",
                "metadata": {"tenant_id": "tenant-456"}
            }
        }

        config = Mock(spec=FraiseQLConfig)
        config.apq_cache_responses = True

        # Call the function WITH context - it should pass context to backend
        cached = handle_apq_request_with_cache(request, backend, config, context=test_context)

        # Now this should pass in GREEN phase
        assert backend.captured_get_context is not None, "Context was not passed to get_cached_response"
        assert "user" in backend.captured_get_context, "User not in context"
        assert backend.captured_get_context["user"]["metadata"]["tenant_id"] == "tenant-456"

    @pytest.mark.asyncio
    async def test_full_apq_flow_with_context(self):
        """Integration test: Full APQ flow with context propagation."""
        from fraiseql.fastapi.app import create_app

        # Create test backend
        backend = ContextCapturingBackend()

        # Create app with test config
        config = FraiseQLConfig(
            database_url="postgresql://test",
            apq_storage_backend="memory",
            apq_cache_responses=True,
            environment="development"
        )

        # Mock the backend creation
        with patch('fraiseql.middleware.apq_caching.get_apq_backend', return_value=backend):
            # Create test app
            app = create_app(config)

            async with AsyncClient(app=app, base_url="http://test") as client:
                # Create query with APQ
                test_query = "query GetTest { test }"
                query_hash = hashlib.sha256(test_query.encode()).hexdigest()

                # Mock JWT token with tenant_id
                headers = {
                    "Authorization": "Bearer test-token",
                    "Content-Type": "application/json"
                }

                # First request: registration (query + hash)
                response = await client.post(
                    "/graphql",
                    json={
                        "query": test_query,
                        "extensions": {
                            "persistedQuery": {
                                "version": 1,
                                "sha256Hash": query_hash
                            }
                        }
                    },
                    headers=headers
                )

                # Context should have been passed to store_cached_response
                # This will fail in RED phase
                assert backend.captured_store_context is not None
                assert "user" in backend.captured_store_context

                # Second request: hash-only
                response = await client.post(
                    "/graphql",
                    json={
                        "query": None,
                        "extensions": {
                            "persistedQuery": {
                                "version": 1,
                                "sha256Hash": query_hash
                            }
                        }
                    },
                    headers=headers
                )

                # Context should have been passed to get_cached_response
                # This will fail in RED phase
                assert backend.captured_get_context is not None
                assert "user" in backend.captured_get_context


class TestContextExtraction:
    """Test context extraction in different scenarios."""

    def test_context_available_at_apq_processing_time(self):
        """Verify that context is built before APQ processing."""
        from fraiseql.fastapi.routers import handle_graphql_request

        # This test verifies the claim that context is built before APQ
        # It should pass even in RED phase since this is existing behavior

        call_order = []

        def mock_build_context(*args, **kwargs):
            call_order.append("build_context")
            return {"user": {"metadata": {"tenant_id": "test"}}}

        def mock_apq_processing(*args, **kwargs):
            call_order.append("apq_processing")
            return None

        with patch('fraiseql.fastapi.routers.build_graphql_context', side_effect=mock_build_context):
            with patch('fraiseql.middleware.apq_caching.handle_apq_request_with_cache', side_effect=mock_apq_processing):
                # Simulate request handling
                # Context should be built before APQ processing

                # Verify order
                assert call_order.index("build_context") < call_order.index("apq_processing"), \
                    "Context must be built before APQ processing"

    def test_context_includes_jwt_tenant_info(self):
        """Test that JWT tenant_id is included in context."""
        from fraiseql.auth.base import UserContext
        from fraiseql.fastapi.dependencies import build_graphql_context

        # Create mock request with JWT user
        mock_request = Mock()
        mock_request.state.user = UserContext(
            user_id="user-123",
            email="test@example.com",
            name="Test User",
            roles=["user"],
            permissions=[],
            metadata={"tenant_id": "tenant-789", "org": "TestOrg"}
        )

        mock_db = Mock()

        # Build context
        context = build_graphql_context(
            request=mock_request,
            db=mock_db,
            user=mock_request.state.user,
            config=Mock()
        )

        # Verify tenant_id is in context
        assert context["user"].metadata["tenant_id"] == "tenant-789"
        assert context["authenticated"] is True
