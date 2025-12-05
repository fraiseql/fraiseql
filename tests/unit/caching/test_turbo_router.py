"""Unit tests for TurboRouter functionality.

These tests use mocking for all database interactions and verify:
- TurboQuery creation and configuration
- TurboRegistry query hashing and registration
- TurboRouter execution with mocked database connections
- Error handling in isolated contexts

Note: Moved from integration tests since all dependencies are mocked.
"""

from unittest.mock import AsyncMock, MagicMock

import pytest

from fraiseql.fastapi.routers import TurboRouter
from fraiseql.fastapi.turbo import TurboQuery, TurboRegistry

pytestmark = pytest.mark.unit


class TestTurboRouter:
    """Test TurboRouter query registration and execution."""

    @pytest.fixture
    def turbo_registry(self) -> None:
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
        self, turbo_registry, sample_query, sample_sql
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
            {"result": {"id": "123", "name": "Test User", "email": "test@example.com"}}
        ]

        # Create a mock that simulates the database behavior expected by TurboRouter
        mock_db = AsyncMock()

        # Mock the run_in_transaction method
        async def mock_transaction(func) -> None:
            # Create a mock connection with cursor
            mock_conn = AsyncMock()
            mock_cursor = AsyncMock()

            # Set up cursor methods
            mock_cursor.execute = AsyncMock()
            mock_cursor.fetchall = AsyncMock(return_value=mock_db_result)

            # Create cursor context manager that returns the cursor
            cursor_cm = AsyncMock()
            cursor_cm.__aenter__ = AsyncMock(return_value=mock_cursor)
            cursor_cm.__aexit__ = AsyncMock(return_value=None)

            # Make cursor() return the context manager
            mock_conn.cursor = MagicMock(return_value=cursor_cm)

            # Call the transaction function
            return await func(mock_conn)

        mock_db.run_in_transaction = AsyncMock(side_effect=mock_transaction)

        context = {"db": mock_db}
        variables = {"id": "123"}

        # Create turbo router
        turbo_router = TurboRouter(turbo_registry)

        # Execute the query
        result = await turbo_router.execute(
            query=sample_query, variables=variables, context=context
        )

        # Should have executed the SQL directly
        assert result is not None, "Turbo router returned None"

        # The turbo router wraps the result in a GraphQL response format
        assert "data" in result
        assert "user" in result["data"]
        assert result["data"]["user"] == {
            "id": "123",
            "name": "Test User",
            "email": "test@example.com",
        }

    @pytest.mark.asyncio
    async def test_turbo_router_execution_unregistered_query(self, turbo_registry) -> None:
        """Test that unregistered queries return None."""
        unregistered_query = "query Unknown { unknown { id } }"

        # Create turbo router
        turbo_router = TurboRouter(turbo_registry)

        # Execute the query
        result = await turbo_router.execute(query=unregistered_query, variables={}, context={})

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
        mock_db_result = [
            {
                "result": [
                    {"id": "1", "name": "Alice", "email": "alice@example.com"},
                    {"id": "2", "name": "Alex", "email": "alex@example.com"},
                ]
            }
        ]

        # Create a mock that simulates the database behavior expected by TurboRouter
        mock_db = AsyncMock()

        # Mock the run_in_transaction method
        async def mock_transaction(func) -> None:
            # Create a mock connection with cursor
            mock_conn = AsyncMock()
            mock_cursor = AsyncMock()

            # Set up cursor methods
            mock_cursor.execute = AsyncMock()
            mock_cursor.fetchall = AsyncMock(return_value=mock_db_result)

            # Create cursor context manager that returns the cursor
            cursor_cm = AsyncMock()
            cursor_cm.__aenter__ = AsyncMock(return_value=mock_cursor)
            cursor_cm.__aexit__ = AsyncMock(return_value=None)

            # Make cursor() return the context manager
            mock_conn.cursor = MagicMock(return_value=cursor_cm)

            # Call the transaction function
            return await func(mock_conn)

        mock_db.run_in_transaction = AsyncMock(side_effect=mock_transaction)

        context = {"db": mock_db}
        variables = {"filters": {"namePattern": "Al%", "emailDomain": "%@example.com"}}

        turbo_router = TurboRouter(turbo_registry)
        result = await turbo_router.execute(query, variables, context)

        assert result is not None
        assert "data" in result
        assert "searchUsers" in result["data"]
        assert len(result["data"]["searchUsers"]) == 2

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
                graphql_query=query, sql_template=sql, param_mapping={}, operation_name=f"Q{i}"
            )
            turbo_registry.register(turbo_query)

        # First query should have been evicted
        assert turbo_registry.get("query Q0 { field0 }") is None
        # Last two should still be there
        assert turbo_registry.get("query Q1 { field1 }") is not None
        assert turbo_registry.get("query Q2 { field2 }") is not None

    @pytest.mark.asyncio
    async def test_turbo_router_error_handling(
        self, turbo_registry, sample_query, sample_sql
    ) -> None:
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

        # Mock the run_in_transaction method to raise an exception
        async def mock_transaction_error(func) -> None:
            # Create a mock connection with cursor
            mock_conn = AsyncMock()
            mock_cursor = AsyncMock()

            # Set up cursor to raise error on execute
            mock_cursor.execute = AsyncMock(side_effect=Exception("Database error"))

            # Create cursor context manager that returns the cursor
            cursor_cm = AsyncMock()
            cursor_cm.__aenter__ = AsyncMock(return_value=mock_cursor)
            cursor_cm.__aexit__ = AsyncMock(return_value=None)

            # Make cursor() return the context manager
            mock_conn.cursor = MagicMock(return_value=cursor_cm)

            # Call the transaction function which will raise the error
            return await func(mock_conn)

        mock_db.run_in_transaction = AsyncMock(side_effect=mock_transaction_error)

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

    @pytest.mark.asyncio
    async def test_turbo_router_fragment_field_extraction(self, turbo_registry) -> None:
        """Test that TurboRouter correctly extracts root field from fragment queries."""
        fragment_query = """
        fragment UserFields on User {
            id
            name
            email
        }

        query GetUsers($filter: String) {
            users(filter: $filter) {
                ...UserFields
            }
        }
        """

        sql_template = """
        SELECT jsonb_build_array(
            jsonb_build_object('id', '1', 'name', 'Alice', 'email', 'alice@example.com'),
            jsonb_build_object('id', '2', 'name', 'Bob', 'email', 'bob@example.com')
        ) as result
        """

        # Register the fragment query
        turbo_query = TurboQuery(
            graphql_query=fragment_query,
            sql_template=sql_template,
            param_mapping={"filter": "filter"},
            operation_name="GetUsers",
        )
        turbo_registry.register(turbo_query)

        # Mock database
        mock_db_result = [
            {
                "result": [
                    {"id": "1", "name": "Alice", "email": "alice@example.com"},
                    {"id": "2", "name": "Bob", "email": "bob@example.com"},
                ]
            }
        ]

        mock_db = AsyncMock()

        async def mock_transaction(func) -> None:
            mock_conn = AsyncMock()
            mock_cursor = AsyncMock()
            mock_cursor.execute = AsyncMock()
            mock_cursor.fetchall = AsyncMock(return_value=mock_db_result)

            cursor_cm = AsyncMock()
            cursor_cm.__aenter__ = AsyncMock(return_value=mock_cursor)
            cursor_cm.__aexit__ = AsyncMock(return_value=None)
            mock_conn.cursor = MagicMock(return_value=cursor_cm)

            return await func(mock_conn)

        mock_db.run_in_transaction = AsyncMock(side_effect=mock_transaction)

        context = {"db": mock_db}
        variables = {"filter": "active"}

        # Execute with TurboRouter
        turbo_router = TurboRouter(turbo_registry)
        result = await turbo_router.execute(
            query=fragment_query, variables=variables, context=context
        )

        # Should extract "users" as root field, not "id" from fragment
        assert result is not None
        assert "data" in result
        assert "users" in result["data"], f"Expected 'users' in result, got: {result}"
        assert isinstance(result["data"]["users"], list)
        assert len(result["data"]["users"]) == 2

    @pytest.mark.asyncio
    async def test_turbo_router_prevents_double_wrapping(self, turbo_registry) -> None:
        """Test that TurboRouter doesn't double-wrap pre-formatted GraphQL responses."""
        fragment_query = """
        fragment ProductFields on Product {
            id
            name
            price
        }

        query GetProducts {
            products {
                ...ProductFields
            }
        }
        """

        # SQL that returns a pre-wrapped GraphQL response (like FraiseQL Backend does)
        sql_template = """
        SELECT jsonb_build_object(
            'data', jsonb_build_object(
                'products', jsonb_build_array(
                    jsonb_build_object('id', '1', 'name', 'Product A', 'price', 100),
                    jsonb_build_object('id', '2', 'name', 'Product B', 'price', 200)
                )
            )
        ) as result
        """

        # Register the fragment query
        turbo_query = TurboQuery(
            graphql_query=fragment_query,
            sql_template=sql_template,
            param_mapping={},
            operation_name="GetProducts",
        )
        turbo_registry.register(turbo_query)

        # Mock database that returns pre-wrapped GraphQL response
        mock_db_result = [
            {
                "result": {
                    "data": {
                        "products": [
                            {"id": "1", "name": "Product A", "price": 100},
                            {"id": "2", "name": "Product B", "price": 200},
                        ]
                    }
                }
            }
        ]

        mock_db = AsyncMock()

        async def mock_transaction(func) -> None:
            mock_conn = AsyncMock()
            mock_cursor = AsyncMock()
            mock_cursor.execute = AsyncMock()
            mock_cursor.fetchall = AsyncMock(return_value=mock_db_result)

            cursor_cm = AsyncMock()
            cursor_cm.__aenter__ = AsyncMock(return_value=mock_cursor)
            cursor_cm.__aexit__ = AsyncMock(return_value=None)
            mock_conn.cursor = MagicMock(return_value=cursor_cm)

            return await func(mock_conn)

        mock_db.run_in_transaction = AsyncMock(side_effect=mock_transaction)

        context = {"db": mock_db}
        variables = {}

        # Execute with TurboRouter
        turbo_router = TurboRouter(turbo_registry)
        result = await turbo_router.execute(
            query=fragment_query, variables=variables, context=context
        )

        # Should NOT double-wrap: {"data": {"products": {"data": {"products": [...]}}}}
        # Should return: {"data": {"products": [...]}}
        assert result is not None
        assert "data" in result
        assert "products" in result["data"]

        # Check that it's NOT double-wrapped
        products = result["data"]["products"]
        assert isinstance(products, list), f"Expected list, got: {type(products)}"
        assert len(products) == 2

        # Ensure we don't have nested data structure
        assert "data" not in products[0], "Found double-wrapping - products contain 'data' field"

    @pytest.mark.asyncio
    async def test_turbo_query_with_context_params(self, turbo_registry) -> None:
        """Test turbo query with context parameters for multi-tenant support."""
        # Multi-tenant query that requires tenant_id from context
        query = """
        query GetAllocations($period: String!) {
            allocations(period: $period) {
                id
                name
                amount
            }
        }
        """

        # SQL template that expects both variable (period) and context params (tenant_id)
        sql_template = """
        SELECT turbo.fn_get_allocations(%(period)s, %(tenant_id)s)::json as result
        """

        # Create TurboQuery with context_params (like mutations support)
        turbo_query = TurboQuery(
            graphql_query=query,
            sql_template=sql_template,
            param_mapping={"period": "period"},
            operation_name="GetAllocations",
            context_params={"tenant_id": "tenant_id"},  # Map context.tenant_id -> SQL param
        )
        turbo_registry.register(turbo_query)

        # Mock database
        mock_db_result = [
            {
                "result": [
                    {"id": "1", "name": "Allocation A", "amount": 1000},
                    {"id": "2", "name": "Allocation B", "amount": 2000},
                ]
            }
        ]

        mock_db = AsyncMock()
        executed_sql_params = None

        async def mock_transaction(func) -> None:
            mock_conn = AsyncMock()
            mock_cursor = AsyncMock()

            # Capture the SQL parameters that were passed
            async def capture_execute(sql, params=None) -> None:
                nonlocal executed_sql_params
                # Only capture params from the actual query, not SET LOCAL commands
                if params is not None:
                    executed_sql_params = params

            mock_cursor.execute = AsyncMock(side_effect=capture_execute)
            mock_cursor.fetchall = AsyncMock(return_value=mock_db_result)
            mock_cursor.row_factory = None  # TurboRouter sets this

            cursor_cm = AsyncMock()
            cursor_cm.__aenter__ = AsyncMock(return_value=mock_cursor)
            cursor_cm.__aexit__ = AsyncMock(return_value=None)
            mock_conn.cursor = MagicMock(return_value=cursor_cm)

            return await func(mock_conn)

        mock_db.run_in_transaction = AsyncMock(side_effect=mock_transaction)

        # Context with tenant_id (like JWT authentication provides)
        context = {"db": mock_db, "tenant_id": "tenant-123"}
        variables = {"period": "CURRENT"}

        turbo_router = TurboRouter(turbo_registry)
        result = await turbo_router.execute(query, variables, context)

        # Verify result structure
        assert result is not None
        assert "data" in result
        assert "allocations" in result["data"]

        # CRITICAL: Verify that SQL received BOTH period (from variables) AND tenant_id (from context)
        assert executed_sql_params is not None, "SQL parameters were not captured"
        assert "period" in executed_sql_params, "period from variables missing"
        assert executed_sql_params["period"] == "CURRENT"
        assert "tenant_id" in executed_sql_params, "tenant_id from context missing"
        assert executed_sql_params["tenant_id"] == "tenant-123"

    @pytest.mark.asyncio
    async def test_turbo_query_missing_required_context_param(self, turbo_registry) -> None:
        """Test that turbo query raises error when required context param is missing."""
        query = """
        query GetAllocations($period: String!) {
            allocations(period: $period) {
                id
                name
            }
        }
        """

        sql_template = """
        SELECT turbo.fn_get_allocations(%(period)s, %(tenant_id)s)::json as result
        """

        turbo_query = TurboQuery(
            graphql_query=query,
            sql_template=sql_template,
            param_mapping={"period": "period"},
            operation_name="GetAllocations",
            context_params={"tenant_id": "tenant_id"},  # Required but not provided
        )
        turbo_registry.register(turbo_query)

        mock_db = AsyncMock()
        # Context WITHOUT tenant_id
        context = {"db": mock_db}
        variables = {"period": "CURRENT"}

        turbo_router = TurboRouter(turbo_registry)

        # Should raise ValueError for missing required context parameter
        with pytest.raises(ValueError, match="Required context parameter 'tenant_id'"):
            await turbo_router.execute(query, variables, context)
