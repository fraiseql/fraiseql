"""GraphQL testing fixtures for FraiseQL.

This module provides comprehensive GraphQL testing utilities including:
- Schema builders and registries for test schemas
- GraphQL client fixtures for executing queries and mutations
- Response validation and assertion helpers
- Query and mutation builders
- Schema introspection utilities
- Error handling and validation

These fixtures enable thorough testing of GraphQL functionality
across unit, integration, and end-to-end test scenarios.
"""

from typing import Any, Dict, List, Optional
from unittest.mock import Mock

import pytest
from fastapi.testclient import TestClient
from graphql import get_introspection_query

import fraiseql

try:
    from fraiseql.config.schema_config import SchemaConfig
except ImportError:
    SchemaConfig = None

try:
    from fraiseql.gql.schema_builder import SchemaRegistry
except ImportError:
    SchemaRegistry = None

try:
    from fraiseql.fastapi.app import create_fraiseql_app
except ImportError:
    create_fraiseql_app = None


class GraphQLTestClient:
    """Enhanced GraphQL test client with assertion helpers."""

    def __init__(self, client: TestClient, endpoint: str = "/graphql"):
        """Initialize GraphQL test client.

        Args:
            client: FastAPI test client
            endpoint: GraphQL endpoint path
        """
        self.client = client
        self.endpoint = endpoint

    def execute(
        self,
        query: str,
        variables: Optional[Dict[str, Any]] = None,
        headers: Optional[Dict[str, str]] = None,
        operation_name: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Execute GraphQL query or mutation.

        Args:
            query: GraphQL query/mutation string
            variables: Query variables
            headers: HTTP headers
            operation_name: Operation name for multi-operation documents

        Returns:
            Dict: GraphQL response
        """
        payload = {"query": query}

        if variables:
            payload["variables"] = variables
        if operation_name:
            payload["operationName"] = operation_name

        response = self.client.post(self.endpoint, json=payload, headers=headers or {})

        return response.json()

    def execute_sync(
        self,
        query: str,
        variables: Optional[Dict[str, Any]] = None,
        headers: Optional[Dict[str, str]] = None,
    ) -> Dict[str, Any]:
        """Execute GraphQL query synchronously (alias for execute)."""
        return self.execute(query, variables, headers)

    async def execute_async(
        self,
        query: str,
        variables: Optional[Dict[str, Any]] = None,
        headers: Optional[Dict[str, str]] = None,
    ) -> Dict[str, Any]:
        """Execute GraphQL query asynchronously."""
        # For TestClient, all operations are sync
        return self.execute(query, variables, headers)

    def introspect(self) -> Dict[str, Any]:
        """Get schema introspection."""
        query = get_introspection_query()
        return self.execute(query)

    def assert_no_errors(self, response: Dict[str, Any]) -> None:
        """Assert response has no GraphQL errors.

        Args:
            response: GraphQL response

        Raises:
            AssertionError: If response contains errors
        """
        assert "errors" not in response, f"GraphQL errors: {response.get('errors')}"

    def assert_has_errors(self, response: Dict[str, Any]) -> None:
        """Assert response has GraphQL errors.

        Args:
            response: GraphQL response

        Raises:
            AssertionError: If response has no errors
        """
        assert "errors" in response, "Expected GraphQL errors but found none"

    def assert_error_message(self, response: Dict[str, Any], expected_message: str) -> None:
        """Assert response contains specific error message.

        Args:
            response: GraphQL response
            expected_message: Expected error message

        Raises:
            AssertionError: If error message not found
        """
        self.assert_has_errors(response)
        error_messages = [error.get("message", "") for error in response["errors"]]
        assert any(expected_message in msg for msg in error_messages), (
            f"Error message '{expected_message}' not found in: {error_messages}"
        )

    def assert_data_equals(self, response: Dict[str, Any], expected_data: Dict[str, Any]) -> None:
        """Assert response data matches expected data.

        Args:
            response: GraphQL response
            expected_data: Expected data structure

        Raises:
            AssertionError: If data doesn't match
        """
        self.assert_no_errors(response)
        assert response.get("data") == expected_data

    def assert_field_exists(self, response: Dict[str, Any], field_path: str) -> None:
        """Assert field exists in response data.

        Args:
            response: GraphQL response
            field_path: Dot-notation path to field (e.g., "user.profile.email")

        Raises:
            AssertionError: If field doesn't exist
        """
        self.assert_no_errors(response)

        data = response.get("data", {})
        fields = field_path.split(".")

        current = data
        for field in fields:
            assert isinstance(current, dict), f"Path {field_path}: Expected dict at {field}"
            assert field in current, f"Path {field_path}: Field '{field}' not found"
            current = current[field]


@pytest.fixture
def clear_schema_registry():
    """Clear FraiseQL schema registry before and after tests."""
    if SchemaRegistry is None:
        yield
        return

    # Clear before test
    SchemaRegistry.get_instance().clear()

    # Clear GraphQL type cache
    try:
        from fraiseql.core.graphql_type import _graphql_type_cache

        _graphql_type_cache.clear()
    except (ImportError, AttributeError):
        pass

    # Clear database type registry
    try:
        from fraiseql.db import _type_registry

        _type_registry.clear()
    except (ImportError, AttributeError):
        pass

    yield

    # Clear after test
    try:
        SchemaRegistry.get_instance().clear()
        _graphql_type_cache.clear()
        _type_registry.clear()
    except (NameError, AttributeError):
        pass


@pytest.fixture
def schema_config_factory():
    """Factory for creating schema configurations."""

    def create_config(
        camel_case_fields: bool = True,
        introspection_enabled: bool = True,
        playground_enabled: bool = True,
        **kwargs,
    ):
        """Create schema configuration.

        Args:
            camel_case_fields: Enable camelCase field transformation
            introspection_enabled: Enable schema introspection
            playground_enabled: Enable GraphQL playground
            **kwargs: Additional configuration options

        Returns:
            SchemaConfig: Configuration instance or mock if unavailable
        """
        if SchemaConfig is None:
            # Return mock config if SchemaConfig is not available
            mock_config = Mock()
            mock_config.camel_case_fields = camel_case_fields
            mock_config.introspection_enabled = introspection_enabled
            mock_config.playground_enabled = playground_enabled
            for key, value in kwargs.items():
                setattr(mock_config, key, value)
            return mock_config

        config = SchemaConfig.get_instance()
        config.camel_case_fields = camel_case_fields
        config.introspection_enabled = introspection_enabled
        config.playground_enabled = playground_enabled

        for key, value in kwargs.items():
            if hasattr(config, key):
                setattr(config, key, value)

        return config

    return create_config


@pytest.fixture
def fraiseql_app_factory(clear_schema_registry):
    """Factory for creating FraiseQL applications."""

    def create_app(
        types: Optional[List[Any]] = None,
        queries: Optional[List[Any]] = None,
        mutations: Optional[List[Any]] = None,
        database_url: Optional[str] = None,
        production: bool = False,
        **kwargs,
    ):
        """Create FraiseQL FastAPI application.

        Args:
            types: GraphQL types to register
            queries: Query resolvers
            mutations: Mutation resolvers
            database_url: Database connection URL
            production: Production mode flag
            **kwargs: Additional app configuration

        Returns:
            FastAPI: Configured application or mock if unavailable
        """
        if create_fraiseql_app is None:
            # Return mock app if create_fraiseql_app is not available
            from fastapi import FastAPI

            app = FastAPI()
            return app

        return create_fraiseql_app(
            types=types or [],
            queries=queries or [],
            mutations=mutations or [],
            database_url=database_url,
            production=production,
            **kwargs,
        )

    return create_app


@pytest.fixture
async def graphql_client_factory(fraiseql_app_factory):
    """Factory for creating GraphQL test clients."""

    async def create_client(
        types: Optional[List[Any]] = None,
        queries: Optional[List[Any]] = None,
        mutations: Optional[List[Any]] = None,
        database_url: Optional[str] = None,
        **app_kwargs,
    ) -> GraphQLTestClient:
        """Create GraphQL test client.

        Args:
            types: GraphQL types
            queries: Query resolvers
            mutations: Mutation resolvers
            database_url: Database URL
            **app_kwargs: Additional app configuration

        Returns:
            GraphQLTestClient: Test client instance
        """
        app = fraiseql_app_factory(
            types=types,
            queries=queries,
            mutations=mutations,
            database_url=database_url,
            **app_kwargs,
        )

        # Manually initialize the database pool for testing
        if database_url:
            from fraiseql.fastapi.app import create_db_pool
            from fraiseql.fastapi.dependencies import set_db_pool

            pool = await create_db_pool(database_url, min_size=1, max_size=5)
            set_db_pool(pool)

        client = TestClient(app)
        return GraphQLTestClient(client)

    return create_client


@pytest.fixture
async def simple_graphql_client(graphql_client_factory, postgres_url, blog_schema_setup):
    """Simple GraphQL client for basic testing. Enhanced for E2E with real database operations."""
    # Import blog models for E2E testing with real database operations
    try:
        from tests_new.e2e.blog_demo.models import (
            Comment,
            CommentOrderByInput,
            CommentWhereInput,
            CreateCommentInput,
            CreatePostInput,
            CreateTagInput,
            CreateUserInput,
            Post,
            PostOrderByInput,
            PostWhereInput,
            Tag,
            UpdateUserInput,
            User,
            UserOrderByInput,
            UserWhereInput,
        )

        # Import real database-backed resolvers
        from tests_new.fixtures.simple_database_resolvers import (
            comments,
            create_comment,
            create_post,
            create_tag,
            create_user,
            post,
            posts,
            publish_post,
            tags,
            update_comment,
            update_post,
            user,
            users,
        )

        # Use blog models with real database queries
        all_query_types = [User, Post, Comment, Tag, users, posts, user, post, comments, tags]
        mutations = [
            create_user,
            create_post,
            create_comment,
            create_tag,
            update_post,
            publish_post,
            update_comment,
        ]

    except ImportError:
        # Fallback to simple types for other tests
        @fraiseql.type
        class TestUser:
            id: str
            name: str
            email: str

        @fraiseql.query
        async def test_user(id: str) -> TestUser:
            return TestUser(id=id, name="Test User", email="test@example.com")

        all_query_types = [TestUser, test_user]
        mutations = []

    return await graphql_client_factory(
        queries=all_query_types, mutations=mutations, database_url=postgres_url
    )


@pytest.fixture
async def blog_graphql_client(graphql_client_factory, postgres_url):
    """GraphQL client configured for blog demo E2E tests."""
    # Import blog models
    from tests_new.e2e.blog_demo.models import (
        Comment,
        Post,
        Tag,
        User,
        create_comment,
        create_post,
        create_user,
        update_user,
    )

    # Add basic queries for E2E testing
    @fraiseql.query
    async def users(info, limit: int = 10) -> list[User]:
        db = info.context["db"]
        users_data = await db.find("users", limit=limit, order_by="created_at DESC")
        return [User(**user) for user in users_data]

    @fraiseql.query
    async def posts(info, limit: int = 10) -> list[Post]:
        db = info.context["db"]
        posts_data = await db.find("posts", limit=limit, order_by="created_at DESC")
        return [Post(**post) for post in posts_data]

    @fraiseql.query
    async def user(info, id: str) -> User | None:
        db = info.context["db"]
        user_data = await db.find_one("users", where={"id": id})
        return User(**user_data) if user_data else None

    # Combine types and queries since create_fraiseql_app uses queries OR types, not both
    all_query_types = [User, Post, Comment, Tag, users, posts, user]

    return await graphql_client_factory(
        queries=all_query_types,  # Pass everything as queries so types get registered
        mutations=[create_user, update_user, create_post, create_comment],
        database_url=postgres_url,
    )


# Fixture aliases for E2E tests
@pytest.fixture
async def e2e_graphql_client(blog_graphql_client):
    """Alias for blog GraphQL client for E2E tests."""
    return blog_graphql_client


# Query and mutation builder utilities
class QueryBuilder:
    """Helper for building GraphQL queries."""

    def __init__(self):
        self.reset()

    def reset(self):
        """Reset builder state."""
        self._query = ""
        self._variables = {}
        self._fragments = []
        return self

    def query(self, name: str, args: Optional[Dict[str, Any]] = None):
        """Start a query operation."""
        args_str = self._format_args(args) if args else ""
        self._query = f"query {name}{args_str} {{"
        return self

    def mutation(self, name: str, args: Optional[Dict[str, Any]] = None):
        """Start a mutation operation."""
        args_str = self._format_args(args) if args else ""
        self._query = f"mutation {name}{args_str} {{"
        return self

    def field(self, name: str, args: Optional[Dict[str, str]] = None, alias: Optional[str] = None):
        """Add a field to the query."""
        alias_str = f"{alias}: " if alias else ""
        args_str = self._format_inline_args(args) if args else ""
        self._query += f"\n  {alias_str}{name}{args_str}"
        return self

    def nested(self, fields: List[str]):
        """Add nested fields."""
        self._query += " {\n" + "\n".join(f"    {field}" for field in fields) + "\n  }"
        return self

    def fragment(self, name: str, type_name: str, fields: List[str]):
        """Add a fragment."""
        fragment = f"fragment {name} on {type_name} {{\n"
        fragment += "\n".join(f"  {field}" for field in fields)
        fragment += "\n}"
        self._fragments.append(fragment)
        return self

    def build(self) -> str:
        """Build the final query string."""
        query = self._query + "\n}"
        if self._fragments:
            query = "\n\n".join(self._fragments) + "\n\n" + query
        return query

    def _format_args(self, args: Dict[str, Any]) -> str:
        """Format query arguments."""
        if not args:
            return ""
        arg_list = [f"${k}: {v}" for k, v in args.items()]
        return f"({', '.join(arg_list)})"

    def _format_inline_args(self, args: Dict[str, str]) -> str:
        """Format inline field arguments."""
        if not args:
            return ""
        arg_list = [f"{k}: {v}" for k, v in args.items()]
        return f"({', '.join(arg_list)})"


@pytest.fixture
def query_builder():
    """GraphQL query builder."""
    return QueryBuilder()


# Schema validation utilities
def validate_schema_introspection(client: GraphQLTestClient) -> Dict[str, Any]:
    """Validate schema introspection works correctly.

    Args:
        client: GraphQL test client

    Returns:
        Dict: Introspection result

    Raises:
        AssertionError: If introspection fails
    """
    result = client.introspect()
    client.assert_no_errors(result)

    # Validate basic schema structure
    assert "data" in result
    assert "__schema" in result["data"]
    assert "types" in result["data"]["__schema"]

    return result


def find_type_in_schema(introspection: Dict[str, Any], type_name: str) -> Optional[Dict[str, Any]]:
    """Find a type in schema introspection result.

    Args:
        introspection: Schema introspection result
        type_name: Name of type to find

    Returns:
        Optional[Dict]: Type definition or None
    """
    types = introspection["data"]["__schema"]["types"]
    return next((t for t in types if t["name"] == type_name), None)


# Test data helpers
@pytest.fixture
def sample_users():
    """Sample user data for testing."""
    return [
        {"id": "1", "name": "John Doe", "email": "john@example.com"},
        {"id": "2", "name": "Jane Smith", "email": "jane@example.com"},
        {"id": "3", "name": "Bob Johnson", "email": "bob@example.com"},
    ]


@pytest.fixture
def sample_posts():
    """Sample blog post data for testing."""
    return [
        {
            "id": "1",
            "title": "Introduction to FraiseQL",
            "content": "FraiseQL is a powerful GraphQL framework...",
            "author_id": "1",
            "published": True,
        },
        {
            "id": "2",
            "title": "Advanced GraphQL Patterns",
            "content": "Learn advanced patterns for GraphQL APIs...",
            "author_id": "2",
            "published": True,
        },
        {
            "id": "3",
            "title": "Database Optimization",
            "content": "Tips for optimizing database queries...",
            "author_id": "1",
            "published": False,
        },
    ]
