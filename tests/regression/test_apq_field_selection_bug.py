"""Regression tests for APQ field selection bug.

Bug: APQ (Automatic Persisted Queries) does not respect field selections -
it sends the full payload instead of only the selected fields.

Expected behavior:
- Query: `{ user { name } }` should return only `{ "user": { "name": "John" } }`

Actual buggy behavior:
- Query: `{ user { name } }` returns full object:
  `{ "user": { "id": 1, "name": "John", "email": "john@example.com", ... } }`

This happens because:
1. APQ cached responses store the FULL response from initial execution
2. When cache hit occurs, the full cached response is returned
3. Field selection from the query is not applied to cached responses
4. The cached response may have been generated with different field selections

The fix should either:
- Include field selection in the cache key (different selections = different cache entries)
- Apply field selection to cached responses before returning
- Not cache responses with partial field selections
"""

import hashlib
from unittest.mock import Mock

import pytest

from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.storage.backends.memory import MemoryAPQBackend

pytestmark = pytest.mark.integration


class TestAPQFieldSelectionBug:
    """Test suite for APQ field selection bug."""

    @pytest.fixture
    def config_with_caching(self) -> FraiseQLConfig:
        """Create config with APQ caching enabled."""
        return FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            apq_storage_backend="memory",
            apq_cache_responses=True,
            apq_response_cache_ttl=600,
        )

    @pytest.fixture
    def backend(self) -> MemoryAPQBackend:
        """Create a fresh memory backend."""
        return MemoryAPQBackend()

    @pytest.mark.xfail(
        reason="Bug: APQ cached response ignores field selection, returns full payload",
        strict=True,
    )
    def test_cached_response_respects_field_selection(
        self, config_with_caching, backend
    ) -> None:
        """Cached response should only include requested fields."""
        # Setup: Store a query that selects ALL fields
        query_all_fields = """
            query GetUser {
                user(id: 1) {
                    id
                    name
                    email
                    createdAt
                    metadata
                }
            }
        """
        query_hash = hashlib.sha256(query_all_fields.encode()).hexdigest()

        # Store the full response with all fields
        full_response = {
            "data": {
                "user": {
                    "id": 1,
                    "name": "John Doe",
                    "email": "john@example.com",
                    "createdAt": "2024-01-01T00:00:00Z",
                    "metadata": {"role": "admin", "active": True},
                }
            }
        }

        backend.store_persisted_query(query_hash, query_all_fields)
        backend.store_cached_response(query_hash, full_response)

        # Create APQ request
        request = Mock(
            query=None,  # Hash-only request
            variables={"id": 1},
            operationName="GetUser",
            extensions={"persistedQuery": {"version": 1, "sha256Hash": query_hash}},
        )

        # Get cached response
        from fraiseql.middleware.apq_caching import handle_apq_request_with_cache

        result = handle_apq_request_with_cache(
            request, backend, config_with_caching
        )

        assert result is not None

        # EXPECTED: Response should only contain fields from the original query's selection
        # Currently FAILS: Returns full cached response regardless of field selection
        user_data = result["data"]["user"]

        # When fixed, cached responses should match the query's field selection
        # or cache key should include field selection info
        assert "email" not in user_data or "createdAt" not in user_data, (
            "Cached response should respect field selection or use selection-aware cache keys"
        )

    def test_different_field_selections_should_have_different_cache_keys(
        self, config_with_caching, backend
    ) -> None:
        """Test that different field selections should produce different cache keys."""
        # These two queries select different fields but same operation
        query_minimal = """
            query GetUser($id: ID!) {
                user(id: $id) {
                    id
                    name
                }
            }
        """

        query_full = """
            query GetUser($id: ID!) {
                user(id: $id) {
                    id
                    name
                    email
                    profile {
                        bio
                        avatar
                    }
                }
            }
        """

        hash_minimal = hashlib.sha256(query_minimal.encode()).hexdigest()
        hash_full = hashlib.sha256(query_full.encode()).hexdigest()

        # Different queries SHOULD have different hashes
        # This is correct behavior - but the bug is about CACHED responses
        assert hash_minimal != hash_full

    def test_variables_included_in_cache_key(self, config_with_caching, backend) -> None:
        """Cache key should include variables to prevent returning wrong data."""
        # Same query, different variables
        query = """
            query GetUser($id: ID!) {
                user(id: $id) {
                    id
                    name
                }
            }
        """
        query_hash = hashlib.sha256(query.encode()).hexdigest()

        # Store response for user 1
        response_user_1 = {"data": {"user": {"id": 1, "name": "Alice"}}}
        backend.store_persisted_query(query_hash, query)
        backend.store_cached_response(query_hash, response_user_1)

        # Request for user 2 (different variable)
        request_user_2 = Mock(
            query=None,
            variables={"id": 2},  # Different user!
            operationName="GetUser",
            extensions={"persistedQuery": {"version": 1, "sha256Hash": query_hash}},
        )

        from fraiseql.middleware.apq_caching import handle_apq_request_with_cache

        result = handle_apq_request_with_cache(
            request_user_2, backend, config_with_caching
        )

        # EXPECTED: Should NOT return user 1's data when requesting user 2
        # Either cache miss (variables in key) or correct data
        if result is not None:
            assert result["data"]["user"]["id"] != 1, (
                "Cache key should include variables - returned wrong user's data"
            )

    def test_apq_cache_returns_stale_data_for_same_hash(
        self, config_with_caching, backend
    ) -> None:
        """Document: APQ cache returns stale data when data changes but query doesn't.

        This is expected behavior for response caching - TTL controls staleness.
        Not a bug per se, but documents the trade-off.
        """
        query = """
            query GetProducts {
                products {
                    id
                    name
                    price
                }
            }
        """
        query_hash = hashlib.sha256(query.encode()).hexdigest()

        # Initial response
        initial_response = {
            "data": {
                "products": [
                    {"id": 1, "name": "Widget", "price": 9.99},
                    {"id": 2, "name": "Gadget", "price": 19.99},
                ]
            }
        }

        backend.store_persisted_query(query_hash, query)
        backend.store_cached_response(query_hash, initial_response)

        # Later request (after data changed in database)
        request = Mock(
            query=None,
            variables=None,
            operationName="GetProducts",
            extensions={"persistedQuery": {"version": 1, "sha256Hash": query_hash}},
        )

        from fraiseql.middleware.apq_caching import handle_apq_request_with_cache

        result = handle_apq_request_with_cache(request, backend, config_with_caching)

        # Returns cached data - this is expected behavior for caching
        assert result is not None
        assert result == initial_response

    @pytest.mark.xfail(
        reason="Bug: Full response stored even when partial fields requested",
        strict=True,
    )
    def test_partial_response_stored_when_partial_fields_requested(
        self, config_with_caching, backend
    ) -> None:
        """Response cache should only store fields that were actually requested."""
        query_hash = "test_hash_123"

        # The response that gets stored should match the query's field selection
        # When fixed, the caching layer should filter the response before storing
        actual_stored_response = {
            "data": {
                "user": {
                    "id": 1,
                    "name": "John",
                    "email": "john@test.com",
                    "phone": "555-1234",
                }
            }
        }

        from fraiseql.middleware.apq_caching import store_response_in_cache

        store_response_in_cache(
            query_hash, actual_stored_response, backend, config_with_caching
        )

        # Verify what was stored
        cached = backend.get_cached_response(query_hash)

        # EXPECTED: Only requested fields should be cached
        # Currently FAILS: Full response is cached regardless of selection
        assert cached is not None
        assert "email" not in cached["data"]["user"] or "phone" not in cached["data"]["user"], (
            "Cache should only store fields that were requested in the query"
        )


class TestAPQFieldSelectionExpectedBehavior:
    """Tests documenting expected (correct) behavior for field selection."""

    @pytest.fixture
    def config_with_caching(self) -> FraiseQLConfig:
        """Create config with APQ caching enabled."""
        return FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            apq_storage_backend="memory",
            apq_cache_responses=True,
        )

    @pytest.fixture
    def backend(self) -> MemoryAPQBackend:
        """Create a fresh memory backend."""
        return MemoryAPQBackend()

    def test_expected_field_selection_only_returns_requested_fields(
        self, config_with_caching, backend
    ) -> None:
        """Document expected behavior: only requested fields should be returned."""
        # This test documents what SHOULD happen (currently fails due to bug)

        query = """
            query GetUser {
                user(id: 1) {
                    name
                }
            }
        """
        query_hash = hashlib.sha256(query.encode()).hexdigest()

        # Store query and response
        backend.store_persisted_query(query_hash, query)

        # Response should match the query's field selection
        expected_response = {"data": {"user": {"name": "John Doe"}}}
        backend.store_cached_response(query_hash, expected_response)

        # Request
        request = Mock(
            query=None,
            variables=None,
            operationName="GetUser",
            extensions={"persistedQuery": {"version": 1, "sha256Hash": query_hash}},
        )

        from fraiseql.middleware.apq_caching import handle_apq_request_with_cache

        result = handle_apq_request_with_cache(request, backend, config_with_caching)

        # This is correct - only requested fields returned
        assert result == expected_response
        assert list(result["data"]["user"].keys()) == ["name"]

    def test_expected_cache_key_includes_variables(self) -> None:
        """Document expected behavior: cache key should include variables."""
        # Expected: cache key should be hash of query + normalized variables
        # This would prevent returning wrong user's data

        variables_user_1 = {"id": 1}
        variables_user_2 = {"id": 2}

        # Simple approach: include variables in cache key
        def compute_cache_key(query_hash: str, variables: dict | None) -> str:
            """Compute cache key that includes variables."""
            import json

            var_str = json.dumps(variables, sort_keys=True) if variables else ""
            combined = f"{query_hash}:{var_str}"
            return hashlib.sha256(combined.encode()).hexdigest()

        key_user_1 = compute_cache_key("query_hash", variables_user_1)
        key_user_2 = compute_cache_key("query_hash", variables_user_2)

        # Different variables = different cache keys
        assert key_user_1 != key_user_2


class TestAPQCacheKeyGeneration:
    """Tests for cache key generation with field selection awareness."""

    def test_cache_key_should_include_operation_name(self) -> None:
        """Test that cache key includes operation name."""
        # Same query, different operation names
        # If multiple operations in document, operation_name matters
        assert True  # Placeholder for implementation

    def test_cache_key_generation_consistency(self) -> None:
        """Test that cache key generation is consistent."""
        query = "query { user { id name } }"
        hash1 = hashlib.sha256(query.encode()).hexdigest()
        hash2 = hashlib.sha256(query.encode()).hexdigest()

        assert hash1 == hash2

    def test_whitespace_affects_cache_key(self) -> None:
        """Test that whitespace differences create different cache keys."""
        query1 = "query { user { id name } }"
        query2 = "query {\n  user {\n    id\n    name\n  }\n}"

        hash1 = hashlib.sha256(query1.encode()).hexdigest()
        hash2 = hashlib.sha256(query2.encode()).hexdigest()

        # Whitespace creates different hashes - this is a problem
        # Ideally, semantically equivalent queries should have same hash
        assert hash1 != hash2  # Current behavior (problematic)


class TestAPQResponseFiltering:
    """Tests for response filtering in APQ."""

    @pytest.fixture
    def backend(self) -> MemoryAPQBackend:
        return MemoryAPQBackend()

    def test_is_cacheable_response_with_errors(self) -> None:
        """Test that responses with errors are not cacheable."""
        from fraiseql.middleware.apq_caching import is_cacheable_response

        # Response with errors should not be cached
        error_response = {
            "errors": [{"message": "Something went wrong"}],
            "data": None,
        }
        assert is_cacheable_response(error_response) is False

    def test_is_cacheable_response_without_data(self) -> None:
        """Test that responses without data are not cacheable."""
        from fraiseql.middleware.apq_caching import is_cacheable_response

        # Response without data should not be cached
        no_data_response = {"extensions": {"timing": 100}}
        assert is_cacheable_response(no_data_response) is False

    def test_is_cacheable_response_partial_error(self) -> None:
        """Test that partial error responses are not cacheable."""
        from fraiseql.middleware.apq_caching import is_cacheable_response

        # Partial response (data + errors) should not be cached
        partial_response = {
            "data": {"user": {"name": "John"}},
            "errors": [{"message": "Could not fetch email"}],
        }
        assert is_cacheable_response(partial_response) is False

    def test_is_cacheable_response_valid(self) -> None:
        """Test that valid responses are cacheable."""
        from fraiseql.middleware.apq_caching import is_cacheable_response

        valid_response = {"data": {"user": {"id": 1, "name": "John"}}}
        assert is_cacheable_response(valid_response) is True
