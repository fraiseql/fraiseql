"""Tests for TurboRouter functionality."""

from unittest.mock import AsyncMock

import pytest

from fraiseql.fastapi.routers import TurboRouter
from fraiseql.fastapi.turbo import TurboQuery, TurboRegistry


class TestTurboRouter:
    """Test TurboRouter query registration and execution."""

    @pytest.fixture
    def turbo_registry(self):
        """Create a TurboRegistry instance."""
        return TurboRegistry()

    @pytest.fixture
    def sample_query(self) -> str:
        """Sample GraphQL query for testing."""
        return """
        query GetUser($id: ID!) {
            user(id: $id) {
                id
                name
                email
            }
        }
        """

    @pytest.fixture
    def sample_sql(self) -> str:
        """Sample SQL query that corresponds to the GraphQL query."""
        return """
        SELECT jsonb_build_object(
            'id', id,
            'name', data->>'name',
            'email', data->>'email'
        ) as result
        FROM users
        WHERE id = %(id)s AND deleted_at IS NULL
        """

    def test_turbo_query_creation(self, sample_query, sample_sql) -> None:
        """Test creating a TurboQuery instance."""
        turbo_query = TurboQuery(
            graphql_query=sample_query,
            sql_template=sample_sql,
            param_mapping={"id": "id"},
            operation_name="GetUser",
        )

        assert turbo_query.graphql_query == sample_query
        assert turbo_query.sql_template == sample_sql
        assert turbo_query.param_mapping == {"id": "id"}
        assert turbo_query.operation_name == "GetUser"

    def test_query_hash_generation(self, turbo_registry, sample_query) -> None:
        """Test that query hashing is consistent and normalized."""
        # Same query with different whitespace should produce same hash
        query_variations = [
            sample_query,
            sample_query.strip(),
            sample_query.replace("\n", " "),
            """query GetUser($id: ID!) { user(id: $id) { id name email } }""",
        ]

        hashes = [turbo_registry.hash_query(q) for q in query_variations]

        # All variations should produce the same hash
        assert len(set(hashes)) == 1

        # Hash should be a string
        assert isinstance(hashes[0], str)

        # Different query should produce different hash
        different_query = "query GetPosts { posts { id title } }"
        different_hash = turbo_registry.hash_query(different_query)
        assert different_hash != hashes[0]

    def test_register_turbo_query(self, turbo_registry, sample_query, sample_sql) -> None:
        """Test registering a turbo query."""
        turbo_query = TurboQuery(
            graphql_query=sample_query,
            sql_template=sample_sql,
            param_mapping={"id": "id"},
            operation_name="GetUser",
        )

        # Register the query
        query_hash = turbo_registry.register(turbo_query)

        # Should return the hash
        assert isinstance(query_hash, str)

        # Should be able to retrieve it
        retrieved = turbo_registry.get(sample_query)
        assert retrieved is not None
        assert retrieved.sql_template == sample_sql
        assert retrieved.param_mapping == {"id": "id"}

    def test_get_unregistered_query(self, turbo_registry) -> None:
        """Test getting a query that hasn't been registered."""
        unregistered_query = "query Unknown { unknown { id } }"
        result = turbo_registry.get(unregistered_query)
        assert result is None

    @pytest.mark.asyncio
    async def test_turbo_router_execution_registered_query(
        self, turbo_registry, sample_query, sample_sql,
    ) -> None:
        """Test executing a registered turbo query."""
        # Register a turbo query
        turbo_query = TurboQuery(
            graphql_query=sample_query,
            sql_template=sample_sql,
            param_mapping={"id": "id"},
            operation_name="GetUser",
        )
        turbo_registry.register(turbo_query)

        # Create mock context with database
        mock_db_result = [
            {"result": {"id": "123", "name": "Test User", "email": "test@example.com"}},
        ]
        mock_db = AsyncMock()
        mock_db.fetch = AsyncMock(return_value=mock_db_result)

        context = {"db": mock_db}
        variables = {"id": "123"}

        # Create turbo router
        turbo_router = TurboRouter(turbo_registry)

        # Execute the query
        result = await turbo_router.execute(
            query=sample_query,
            variables=variables,
            context=context,
        )

        # Should have executed the SQL directly
        assert result is not None
        assert result["data"] == {
            "user": {"id": "123", "name": "Test User", "email": "test@example.com"},
        }

        # Verify SQL was called with correct parameters
        mock_db.fetch.assert_called_once_with(sample_sql, {"id": "123"})

    @pytest.mark.asyncio
    async def test_turbo_router_execution_unregistered_query(self, turbo_registry) -> None:
        """Test that unregistered queries return None."""
        unregistered_query = "query Unknown { unknown { id } }"

        # Create turbo router
        turbo_router = TurboRouter(turbo_registry)

        # Execute the query
        result = await turbo_router.execute(
            query=unregistered_query,
            variables={},
            context={},
        )

        # Should return None for unregistered queries
        assert result is None

    @pytest.mark.asyncio
    async def test_turbo_router_with_complex_variables(self, turbo_registry) -> None:
        """Test turbo router with complex variable mappings."""
        query = """
        query SearchUsers($filters: UserFilters!) {
            searchUsers(filters: $filters) {
                id
                name
                email
            }
        }
        """

        sql = """
        SELECT jsonb_agg(
            jsonb_build_object(
                'id', id,
                'name', data->>'name',
                'email', data->>'email'
            )
        ) as result
        FROM users
        WHERE
            (%(name_pattern)s IS NULL OR data->>'name' ILIKE %(name_pattern)s)
            AND (%(email_domain)s IS NULL OR data->>'email' LIKE %(email_domain)s)
            AND deleted_at IS NULL
        """

        turbo_query = TurboQuery(
            graphql_query=query,
            sql_template=sql,
            param_mapping={
                "filters.namePattern": "name_pattern",
                "filters.emailDomain": "email_domain",
            },
            operation_name="SearchUsers",
        )
        turbo_registry.register(turbo_query)

        # Mock database
        mock_db = AsyncMock()
        mock_db.fetch = AsyncMock(
            return_value=[
                {
                    "result": [
                        {"id": "1", "name": "Alice", "email": "alice@example.com"},
                        {"id": "2", "name": "Alex", "email": "alex@example.com"},
                    ],
                },
            ],
        )

        context = {"db": mock_db}
        variables = {
            "filters": {
                "namePattern": "Al%",
                "emailDomain": "%@example.com",
            },
        }

        turbo_router = TurboRouter(turbo_registry)
        result = await turbo_router.execute(query, variables, context)

        assert result is not None
        assert len(result["data"]["searchUsers"]) == 2

        # Check SQL parameters were mapped correctly
        mock_db.fetch.assert_called_once_with(
            sql,
            {"name_pattern": "Al%", "email_domain": "%@example.com"},
        )

    def test_turbo_registry_clear(self, turbo_registry, sample_query, sample_sql) -> None:
        """Test clearing the turbo registry."""
        turbo_query = TurboQuery(
            graphql_query=sample_query,
            sql_template=sample_sql,
            param_mapping={"id": "id"},
            operation_name="GetUser",
        )

        # Register and verify it exists
        turbo_registry.register(turbo_query)
        assert turbo_registry.get(sample_query) is not None

        # Clear and verify it's gone
        turbo_registry.clear()
        assert turbo_registry.get(sample_query) is None

    def test_turbo_registry_size_limit(self, turbo_registry) -> None:
        """Test that registry respects size limits."""
        # Set a small size limit
        turbo_registry.max_size = 2

        # Register queries up to the limit
        for i in range(3):
            query = f"query Q{i} {{ field{i} }}"
            sql = f"SELECT {i}"
            turbo_query = TurboQuery(
                graphql_query=query,
                sql_template=sql,
                param_mapping={},
                operation_name=f"Q{i}",
            )
            turbo_registry.register(turbo_query)

        # First query should have been evicted
        assert turbo_registry.get("query Q0 { field0 }") is None
        # Last two should still be there
        assert turbo_registry.get("query Q1 { field1 }") is not None
        assert turbo_registry.get("query Q2 { field2 }") is not None

    @pytest.mark.asyncio
    async def test_turbo_router_error_handling(self, turbo_registry, sample_query, sample_sql) -> None:
        """Test error handling in turbo router execution."""
        # Register a query
        turbo_query = TurboQuery(
            graphql_query=sample_query,
            sql_template=sample_sql,
            param_mapping={"id": "id"},
            operation_name="GetUser",
        )
        turbo_registry.register(turbo_query)

        # Mock database that throws an error
        mock_db = AsyncMock()
        mock_db.fetch = AsyncMock(side_effect=Exception("Database error"))

        context = {"db": mock_db}
        variables = {"id": "123"}

        turbo_router = TurboRouter(turbo_registry)

        # Should raise the exception
        with pytest.raises(Exception, match="Database error"):
            await turbo_router.execute(sample_query, variables, context)

    def test_turbo_query_with_fragments(self, turbo_registry) -> None:
        """Test handling queries with fragments."""
        query_with_fragment = """
        fragment UserFields on User {
            id
            name
            email
        }

        query GetUser($id: ID!) {
            user(id: $id) {
                ...UserFields
            }
        }
        """

        # Should normalize to same hash as expanded query
        expanded_query = """
        query GetUser($id: ID!) {
            user(id: $id) {
                id
                name
                email
            }
        }
        """

        # For now, these will have different hashes
        # In a full implementation, we'd parse and normalize the AST
        hash1 = turbo_registry.hash_query(query_with_fragment)
        hash2 = turbo_registry.hash_query(expanded_query)

        # These will be different without AST normalization
        assert hash1 != hash2
