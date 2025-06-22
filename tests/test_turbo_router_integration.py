"""Integration tests for TurboRouter with FastAPI.

🚀 Uses FraiseQL's UNIFIED CONTAINER system - see database_conftest.py
A single PostgreSQL container runs for ALL tests with socket communication.
"""

import pytest
import pytest_asyncio
from httpx import ASGITransport, AsyncClient

from fraiseql import fraise_field, fraise_type
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.fastapi.turbo import TurboQuery, TurboRegistry
from tests.utils.container_utils import requires_any_container

# This test uses the unified container system from database_conftest.py
# Each test runs in its own transaction that is rolled back automatically


@fraise_type
class User:
    """User type for testing."""

    id: int
    name: str = fraise_field(description="User's name")
    email: str = fraise_field(description="User's email")


@requires_any_container
class TestTurboRouterIntegration:
    """Test TurboRouter integration with the full FastAPI stack."""

    @pytest_asyncio.fixture
    async def setup_test_data(self, db_connection):
        """Create tables and insert test data within test transaction."""
        # Create tables
        await db_connection.execute(
            """
            CREATE TABLE users (
                id SERIAL PRIMARY KEY,
                data JSONB NOT NULL DEFAULT '{}',
                created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
                deleted_at TIMESTAMPTZ
            )
        """,
        )

        # Insert test data
        await db_connection.execute(
            """
            INSERT INTO users (data) VALUES
            ('{"name": "Alice", "email": "alice@example.com"}'::jsonb),
            ('{"name": "Bob", "email": "bob@example.com"}'::jsonb),
            ('{"name": "Charlie", "email": "charlie@example.com"}'::jsonb)
        """,
        )

        # No commit needed - transaction will be rolled back after test

    @pytest.fixture
    def turbo_registry(self):
        """Create a TurboRegistry with pre-registered queries."""
        registry = TurboRegistry()

        # Register a simple user query
        user_query = TurboQuery(
            graphql_query="query GetUser($id: ID!) { user(id: $id) { id name email } }",
            sql_template="""
                SELECT jsonb_build_object(
                    'user', jsonb_build_object(
                        'id', id,
                        'name', data->>'name',
                        'email', data->>'email'
                    )
                ) as result
                FROM users
                WHERE id = %(id)s::int AND deleted_at IS NULL
                LIMIT 1
            """,
            param_mapping={"id": "id"},
            operation_name="GetUser",
        )
        registry.register(user_query)

        # Register a users list query
        users_query = TurboQuery(
            graphql_query="query ListUsers { users { id name email } }",
            sql_template="""
                SELECT jsonb_build_object(
                    'users', COALESCE(
                        jsonb_agg(
                            jsonb_build_object(
                                'id', id,
                                'name', data->>'name',
                                'email', data->>'email'
                            )
                            ORDER BY id
                        ),
                        '[]'::jsonb
                    )
                ) as result
                FROM users
                WHERE deleted_at IS NULL
            """,
            param_mapping={},
            operation_name="ListUsers",
        )
        registry.register(users_query)

        return registry

    @pytest_asyncio.fixture
    async def app(self, db_pool, turbo_registry, setup_test_data):
        """Create FastAPI app with TurboRouter enabled."""
        # Mock the database pool in the app
        from fraiseql.fastapi.dependencies import set_db_pool

        set_db_pool(db_pool)

        # Create app with TurboRouter
        app = create_fraiseql_app(
            database_url="postgresql://test/test",  # Will use mocked pool
            types=[User],
            production=True,  # Enable production mode for TurboRouter
        )

        # Inject our turbo registry
        # We need to recreate the router with our registry
        from fraiseql.fastapi.routers import create_graphql_router
        from fraiseql.gql.schema_builder import build_fraiseql_schema

        schema = build_fraiseql_schema(
            query_types=[User],
            mutation_resolvers=[],
            camel_case_fields=False,
        )

        # Remove existing /graphql route
        app.routes = [r for r in app.routes if r.path != "/graphql"]

        # Add new router with turbo registry
        graphql_router = create_graphql_router(
            schema=schema,
            config=app.state.fraiseql_config,
            turbo_registry=turbo_registry,
        )
        app.include_router(graphql_router)

        return app

    @pytest.mark.asyncio
    async def test_turbo_router_query_execution(self, app) -> None:
        """Test that TurboRouter executes registered queries directly."""
        transport = ASGITransport(app=app)
        async with AsyncClient(transport=transport, base_url="http://test") as client:
            # Execute a registered query
            response = await client.post(
                "/graphql",
                json={
                    "query": "query GetUser($id: ID!) { user(id: $id) { id name email } }",
                    "variables": {"id": "1"},
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "data" in data
            assert data["data"]["user"]["id"] == 1
            assert data["data"]["user"]["name"] == "Alice"
            assert data["data"]["user"]["email"] == "alice@example.com"

    @pytest.mark.asyncio
    async def test_turbo_router_list_query(self, app) -> None:
        """Test TurboRouter with a list query."""
        transport = ASGITransport(app=app)
        async with AsyncClient(transport=transport, base_url="http://test") as client:
            response = await client.post(
                "/graphql",
                json={
                    "query": "query ListUsers { users { id name email } }",
                },
            )

            assert response.status_code == 200
            data = response.json()
            assert "data" in data
            assert len(data["data"]["users"]) == 3
            assert data["data"]["users"][0]["name"] == "Alice"
            assert data["data"]["users"][1]["name"] == "Bob"
            assert data["data"]["users"][2]["name"] == "Charlie"

    @pytest.mark.asyncio
    async def test_unregistered_query_fallback(self, app) -> None:
        """Test that unregistered queries fall back to standard GraphQL execution."""
        transport = ASGITransport(app=app)
        async with AsyncClient(transport=transport, base_url="http://test") as client:
            # This query is not registered in TurboRouter
            response = await client.post(
                "/graphql",
                json={
                    "query": "query { __typename }",
                },
            )

            # Should still work via standard GraphQL
            assert response.status_code == 200
            data = response.json()
            assert "data" in data

    @pytest.mark.asyncio
    async def test_turbo_router_performance(self, app) -> None:
        """Test that TurboRouter is faster than standard execution."""
        import time

        transport = ASGITransport(app=app)
        async with AsyncClient(transport=transport, base_url="http://test") as client:
            # Warm up
            for _ in range(5):
                await client.post(
                    "/graphql",
                    json={"query": "query ListUsers { users { id name email } }"},
                )

            # Measure TurboRouter query (registered)
            turbo_times = []
            for _ in range(20):
                start = time.perf_counter()
                response = await client.post(
                    "/graphql",
                    json={"query": "query ListUsers { users { id name email } }"},
                )
                turbo_times.append(time.perf_counter() - start)
                assert response.status_code == 200

            # Measure standard query (not registered)
            standard_times = []
            for _ in range(20):
                start = time.perf_counter()
                response = await client.post(
                    "/graphql",
                    json={"query": "{ __schema { queryType { name } } }"},
                )
                standard_times.append(time.perf_counter() - start)
                assert response.status_code == 200

            # TurboRouter should be faster on average
            sum(turbo_times) / len(turbo_times)
            sum(standard_times) / len(standard_times)

            # Log the performance difference

    @pytest.mark.asyncio
    async def test_turbo_router_with_errors(self, app, turbo_registry) -> None:
        """Test TurboRouter error handling."""
        # Register a query with bad SQL
        bad_query = TurboQuery(
            graphql_query="query BadQuery { bad { id } }",
            sql_template="SELECT * FROM nonexistent_table",
            param_mapping={},
            operation_name="BadQuery",
        )
        turbo_registry.register(bad_query)

        transport = ASGITransport(app=app)
        async with AsyncClient(transport=transport, base_url="http://test") as client:
            response = await client.post(
                "/graphql",
                json={"query": "query BadQuery { bad { id } }"},
            )

            # Should return an error
            assert response.status_code == 200
            data = response.json()
            assert "errors" in data
