"""Smoke tests for GraphQL query parameter test fixtures."""

import pytest

pytestmark = pytest.mark.integration


class TestGraphQLFixtures:
    """Verify test fixtures work correctly."""

    @pytest.mark.asyncio
    async def test_setup_graphql_table_creates_table_and_view(
        self, db_connection, setup_graphql_table
    ):
        """setup_graphql_table should create table and view."""
        await setup_graphql_table("fixture_test")

        # Verify table exists
        result = await db_connection.execute("""
            SELECT EXISTS (
                SELECT FROM information_schema.tables
                WHERE table_name = 'tb_fixture_test'
            )
        """)
        row = await result.fetchone()
        assert row[0] is True, "Table should exist"

        # Verify view exists
        result = await db_connection.execute("""
            SELECT EXISTS (
                SELECT FROM information_schema.views
                WHERE table_name = 'v_fixture_test'
            )
        """)
        row = await result.fetchone()
        assert row[0] is True, "View should exist"

    @pytest.mark.asyncio
    async def test_seed_graphql_data_inserts_records(
        self, db_connection, setup_graphql_table, seed_graphql_data
    ):
        """seed_graphql_data should insert JSONB records."""
        await setup_graphql_table("seed_test")
        await seed_graphql_data(
            "tb_seed_test",
            [
                {"name": "Alice", "value": 1},
                {"name": "Bob", "value": 2},
            ],
        )

        result = await db_connection.execute("SELECT COUNT(*) FROM tb_seed_test")
        row = await result.fetchone()
        assert row[0] == 2, "Should have 2 records"

    @pytest.mark.asyncio
    async def test_gql_context_provides_repository(self, gql_context):
        """gql_context should provide FraiseQLRepository."""
        from fraiseql.db import FraiseQLRepository

        assert "db" in gql_context
        assert isinstance(gql_context["db"], FraiseQLRepository)
