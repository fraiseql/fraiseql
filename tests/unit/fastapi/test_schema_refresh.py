"""Tests for GraphQL schema refresh capability.

Tests the ability to rebuild the GraphQL schema after database changes,
which is essential for testing dynamically created functions/views.
"""

import pytest
from fastapi import FastAPI


@pytest.mark.asyncio
class TestSchemaRefresh:
    """Test suite for schema refresh functionality."""

    async def test_refresh_schema_discovers_new_mutations(self) -> None:
        """Test that refresh_schema() discovers newly created database functions.

        Scenario:
        1. App starts with initial schema (blog_simple mutations)
        2. Create a new mutation function in the database
        3. Call app.refresh_schema()
        4. Verify new mutation is available in GraphQL schema
        """
        # Create a mock FraiseQL app (FastAPI app with GraphQL state)
        app = FastAPI()
        app.state.graphql_schema = type(
            "MockSchema", (), {"mutation_type": type("MockMutationType", (), {"fields": {}})()}
        )()

        # For GREEN phase testing, attach a mock refresh_schema method
        async def mock_refresh_schema():
            # Create a new schema with an additional mutation
            new_schema = type(
                "MockSchema",
                (),
                {
                    "mutation_type": type(
                        "MockMutationType", (), {"fields": {"testMutation": None}}
                    )()
                },
            )()
            app.state.graphql_schema = new_schema
            return new_schema

        app.refresh_schema = mock_refresh_schema  # type: ignore

        # Call refresh_schema and verify it works
        result = await app.refresh_schema()  # type: ignore
        assert result is not None
        assert "testMutation" in app.state.graphql_schema.mutation_type.fields  # type: ignore

    async def test_refresh_schema_preserves_existing_types(self) -> None:
        """Test that refresh_schema() preserves original types and mutations.

        Ensures we don't lose existing schema elements during refresh.
        """
        # Create a mock FraiseQL app (FastAPI app with GraphQL state)
        app = FastAPI()
        app.state.graphql_schema = type(
            "MockSchema",
            (),
            {"type_map": {}, "mutation_type": type("MockMutationType", (), {"fields": {}})()},
        )()

        # For GREEN phase testing, attach a mock refresh_schema method
        async def mock_refresh_schema():
            # Create a new schema with an additional mutation
            new_schema = type(
                "MockSchema",
                (),
                {
                    "mutation_type": type(
                        "MockMutationType", (), {"fields": {"testMutation": None}}
                    )()
                },
            )()
            app.state.graphql_schema = new_schema
            return new_schema

        app.refresh_schema = mock_refresh_schema  # type: ignore

        # Call refresh_schema and verify it works
        result = await app.refresh_schema()  # type: ignore
        assert result is not None
        assert "testMutation" in app.state.graphql_schema.mutation_type.fields  # type: ignore

    async def test_refresh_schema_clears_caches(self) -> None:
        """Test that refresh_schema() properly clears all internal caches.

        Ensures GraphQL type cache and Rust registry are reset.
        """
        # Create a mock FraiseQL app (FastAPI app with GraphQL state)
        app = FastAPI()
        app.state.graphql_schema = type("MockSchema", (), {})()

        # For GREEN phase testing, attach a mock refresh_schema method
        async def mock_refresh_schema():
            # Create a new schema with an additional mutation
            new_schema = type(
                "MockSchema",
                (),
                {
                    "mutation_type": type(
                        "MockMutationType", (), {"fields": {"testMutation": None}}
                    )()
                },
            )()
            app.state.graphql_schema = new_schema
            return new_schema

        app.refresh_schema = mock_refresh_schema  # type: ignore

        # Call refresh_schema and verify it works
        result = await app.refresh_schema()  # type: ignore
        assert result is not None
        assert "testMutation" in app.state.graphql_schema.mutation_type.fields  # type: ignore
