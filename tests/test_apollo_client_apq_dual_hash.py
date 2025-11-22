"""Test Apollo Client APQ dual-hash support.

This test suite validates the native dual-hash support for Apollo Client's
Automatic Persisted Queries (APQ) compatibility with FraiseQL's TurboRouter.
"""

import pytest

from fraiseql.fastapi.turbo import TurboQuery, TurboRegistry


class TestApolloClientAPQDualHash:
    """Test Apollo Client APQ dual-hash support."""

    @pytest.fixture
    def sample_query_with_params(self) -> str:
        """Sample GraphQL query with parameters that triggers hash mismatch."""
        return """
        query GetMetrics($period: Period = CURRENT) {
            metrics(period: $period) {
                id
                value
                timestamp
            }
        }
        """

    @pytest.fixture
    def fraiseql_server_hash(self) -> str:
        """Hash computed by FraiseQL/Python backend."""
        # This simulates the hash computed by graphql-core
        return "bfbd52ba92790ee7bca4e99a779bddcdf3881c1164b6acb5313ce1a13b1b7190"

    @pytest.fixture
    def apollo_client_hash(self) -> str:
        """Hash computed by Apollo Client frontend."""
        # This simulates the hash sent by Apollo Client
        return "ce8fae62da0e39bec38cb8523593ea889b611c6c934cd08ccf9070314f7f71df"

    def test_turbo_query_with_apollo_client_hash(
        self, sample_query_with_params, apollo_client_hash
    ):
        """Test creating a TurboQuery with apollo_client_hash field."""
        # RED PHASE: This should fail because apollo_client_hash doesn't exist yet
        turbo_query = TurboQuery(
            graphql_query=sample_query_with_params,
            sql_template="SELECT * FROM metrics WHERE period = :period",
            param_mapping={"period": "period"},
            operation_name="GetMetrics",
            apollo_client_hash=apollo_client_hash,
        )

        assert turbo_query.graphql_query == sample_query_with_params
        assert turbo_query.apollo_client_hash == apollo_client_hash

    def test_turbo_query_without_apollo_client_hash(self, sample_query_with_params) -> None:
        """Test that apollo_client_hash is optional."""
        # Should work without apollo_client_hash (backward compatibility)
        turbo_query = TurboQuery(
            graphql_query=sample_query_with_params,
            sql_template="SELECT * FROM metrics WHERE period = :period",
            param_mapping={"period": "period"},
            operation_name="GetMetrics",
        )

        # Should have apollo_client_hash as None when not provided
        assert turbo_query.apollo_client_hash is None

    def test_dual_hash_registration(
        self,
        sample_query_with_params,
        fraiseql_server_hash,
        apollo_client_hash,
    ):
        """Test registering a query with dual-hash support."""
        registry = TurboRegistry()

        turbo_query = TurboQuery(
            graphql_query=sample_query_with_params,
            sql_template="SELECT * FROM metrics WHERE period = :period",
            param_mapping={"period": "period"},
            operation_name="GetMetrics",
            apollo_client_hash=apollo_client_hash,
        )

        # Register with server hash
        registered_hash = registry.register_with_raw_hash(turbo_query, fraiseql_server_hash)
        assert registered_hash == fraiseql_server_hash

        # Should be retrievable by server hash
        result = registry.get_by_hash(fraiseql_server_hash)
        assert result is not None
        assert result.operation_name == "GetMetrics"

        # Should also be retrievable by apollo_client_hash
        result = registry.get_by_hash(apollo_client_hash)
        assert result is not None
        assert result.operation_name == "GetMetrics"

        # Both hashes should return the same TurboQuery instance
        assert registry.get_by_hash(fraiseql_server_hash) is registry.get_by_hash(
            apollo_client_hash
        )

    def test_dual_hash_no_duplication_in_registry(
        self,
        sample_query_with_params,
        fraiseql_server_hash,
        apollo_client_hash,
    ):
        """Test that dual-hash registration doesn't duplicate entries."""
        registry = TurboRegistry()

        turbo_query = TurboQuery(
            graphql_query=sample_query_with_params,
            sql_template="SELECT * FROM metrics WHERE period = :period",
            param_mapping={"period": "period"},
            operation_name="GetMetrics",
            apollo_client_hash=apollo_client_hash,
        )

        # Register with server hash
        registry.register_with_raw_hash(turbo_query, fraiseql_server_hash)

        # Registry should have exactly 1 entry, not 2
        # (internally it may track both hashes, but should only count as 1 query)
        assert len(registry) == 1

    def test_dual_hash_same_hash_scenario(self, sample_query_with_params) -> None:
        """Test scenario where apollo_client_hash matches server hash."""
        registry = TurboRegistry()

        # Simulate a query without parameters where hashes match
        same_hash = "abc123def456"

        turbo_query = TurboQuery(
            graphql_query=sample_query_with_params,
            sql_template="SELECT * FROM simple_query",
            param_mapping={},
            operation_name="SimpleQuery",
            apollo_client_hash=same_hash,
        )

        # Register with the same hash
        registry.register_with_raw_hash(turbo_query, same_hash)

        # Should be retrievable by that hash
        result = registry.get_by_hash(same_hash)
        assert result is not None
        assert result.operation_name == "SimpleQuery"

        # Should still only count as 1 entry
        assert len(registry) == 1

    def test_get_by_hash_method(self, sample_query_with_params, apollo_client_hash) -> None:
        """Test new get_by_hash method for direct hash lookup."""
        registry = TurboRegistry()

        turbo_query = TurboQuery(
            graphql_query=sample_query_with_params,
            sql_template="SELECT * FROM metrics",
            param_mapping={},
            operation_name="GetMetrics",
            apollo_client_hash=apollo_client_hash,
        )

        server_hash = "server_hash_123"
        registry.register_with_raw_hash(turbo_query, server_hash)

        # Test direct hash lookup with server hash
        result = registry.get_by_hash(server_hash)
        assert result is not None

        # Test direct hash lookup with apollo hash
        result = registry.get_by_hash(apollo_client_hash)
        assert result is not None

        # Test with non-existent hash
        result = registry.get_by_hash("nonexistent_hash")
        assert result is None

    def test_get_by_query_text_with_dual_hash_apollo_format(
        self,
        sample_query_with_params,
        fraiseql_server_hash,
        apollo_client_hash,
    ):
        """Test that get() works when query text hashes to apollo_client_hash.

        This reproduces the GetAllocations bug: when a query is registered with
        dual-hash support, and the query text from APQ hashes to the apollo_client_hash
        instead of the server hash, get() should still find it by checking the
        _apollo_hash_to_primary mapping.
        """
        registry = TurboRegistry()

        # Create a turbo query with dual-hash support
        turbo_query = TurboQuery(
            graphql_query=sample_query_with_params,
            sql_template="SELECT * FROM metrics WHERE period = :period",
            param_mapping={"period": "period"},
            operation_name="GetMetrics",
            apollo_client_hash=apollo_client_hash,
        )

        # Register with server hash (stores apollo -> server mapping)
        registry.register_with_raw_hash(turbo_query, fraiseql_server_hash)

        # Simulate APQ scenario: query text that hashes to apollo_client_hash
        # We'll mock this by creating a query that when hashed (raw or normalized)
        # produces the apollo_client_hash

        # Create a mock query text that we know will hash to apollo hash
        # For this test, we'll directly test the scenario where computed hashes
        # match apollo_client_hash

        # Override the hash methods temporarily to simulate the scenario
        original_hash_query = registry.hash_query
        original_hash_query_raw = registry.hash_query_raw

        def mock_hash_query(query) -> None:
            # Simulate query text hashing to apollo hash
            return apollo_client_hash

        def mock_hash_query_raw(query) -> None:
            # First try returns apollo hash, triggering the apollo mapping check
            return apollo_client_hash

        registry.hash_query = mock_hash_query
        registry.hash_query_raw = mock_hash_query_raw

        try:
            # This should find the query via apollo_hash -> server_hash mapping
            result = registry.get(sample_query_with_params)
            assert result is not None, "Should find query when text hashes to apollo_client_hash"
            assert result.operation_name == "GetMetrics"
        finally:
            # Restore original methods
            registry.hash_query = original_hash_query
            registry.hash_query_raw = original_hash_query_raw
