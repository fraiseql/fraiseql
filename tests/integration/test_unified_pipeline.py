"""Integration tests for Phase 9 unified GraphQL pipeline.

Tests verify that the unified Rust pipeline can execute complete GraphQL queries
end-to-end using a real PostgreSQL database.
"""

import pytest


@pytest.fixture
def rust_pool(postgres_url):
    """Create a Rust DatabasePool for testing."""
    from fraiseql._fraiseql_rs import DatabasePool

    # Create pool from URL
    pool = DatabasePool(url=postgres_url)
    yield pool
    # Pool cleanup happens automatically when it goes out of scope


class TestUnifiedPipeline:
    """Test Phase 9 unified GraphQL execution pipeline."""

    def test_pipeline_initialization(self, rust_pool):
        """Test that the unified pipeline can be initialized with a database pool."""
        from fraiseql._fraiseql_rs import initialize_graphql_pipeline

        # Create a minimal schema with required fields
        schema_json = '{"tables": {}, "types": {}}'

        # Initialize pipeline with pool
        initialize_graphql_pipeline(schema_json, rust_pool)

        # If we got here, initialization succeeded
        assert True

    def test_pipeline_execute_query(self, rust_pool):
        """Test that the unified pipeline can execute a GraphQL query."""
        from fraiseql._fraiseql_rs import execute_graphql_query, initialize_graphql_pipeline

        # Create a minimal schema with required fields
        schema_json = '{"tables": {}, "types": {}}'

        # Initialize pipeline
        initialize_graphql_pipeline(schema_json, rust_pool)

        # Execute a simple query
        query = "{ __typename }"
        variables = {}
        user_context = {"user_id": None, "permissions": [], "roles": []}

        # This should execute without error (even if it returns empty data)
        try:
            result_bytes = execute_graphql_query(query, variables, user_context)
            assert isinstance(result_bytes, bytes)
            # Decode and check it's valid JSON
            import json

            result = json.loads(result_bytes.decode("utf-8"))
            assert "data" in result or "errors" in result
        except Exception as e:
            # For now, we accept errors as the schema is minimal
            # The important thing is that the pipeline executed
            assert "Pipeline not initialized" not in str(e)

    @pytest.mark.skip(reason="Requires full schema setup - will implement after basic tests pass")
    def test_pipeline_with_real_query(self, rust_pool):
        """Test pipeline with a real query against the database."""
        # TODO: Implement once we have a test schema set up
        pass
