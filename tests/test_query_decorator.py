"""Test the @query and @field decorators."""

from typing import Optional
from uuid import UUID

import pytest
from graphql import graphql

import fraiseql
from fraiseql.gql.schema_builder import SchemaRegistry, build_fraiseql_schema


# Define types
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str


@fraiseql.type
class Post:
    id: UUID
    title: str
    content: str


# Use @query decorator
@fraiseql.query
async def get_user(info, id: UUID) -> Optional[User]:
    """Get a user by ID."""
    if str(id) == "123e4567-e89b-12d3-a456-426614174000":
        return User(
            id=id,
            name="John Doe",
            email="john@example.com"
        )
    return None


@fraiseql.query
async def get_all_users(info) -> list[User]:
    """Get all users."""
    return [
        User(
            id=UUID("123e4567-e89b-12d3-a456-426614174000"),
            name="John Doe",
            email="john@example.com"
        ),
        User(
            id=UUID("223e4567-e89b-12d3-a456-426614174001"),
            name="Jane Smith",
            email="jane@example.com"
        )
    ]


# Use @field decorator with QueryRoot
@fraiseql.type
class QueryRoot:
    """Root query type with field decorators."""
    
    @fraiseql.field(description="API version")
    def version(self, root, info) -> str:
        """Get API version."""
        return "2.0.0"
    
    @fraiseql.field
    async def post_count(self, root, info) -> int:
        """Get total number of posts."""
        # In real app, would query database
        return 42


@pytest.fixture(autouse=True)
def clear_registry():
    """Clear the registry before each test."""
    registry = SchemaRegistry.get_instance()
    registry.clear()
    
    # Re-register the queries after clearing
    # This is needed because decorators run at import time
    registry.register_query(get_user)
    registry.register_query(get_all_users)
    
    yield
    registry.clear()


def test_query_decorator_registration():
    """Test that @query decorator registers functions."""
    # Functions should already be registered via decorator
    registry = SchemaRegistry.get_instance()
    
    # Build schema without passing queries
    schema = build_fraiseql_schema(
        query_types=[QueryRoot]  # Only need to pass types
    )
    
    # Check that decorated queries are in the schema
    query_fields = schema.query_type.fields
    assert "get_user" in query_fields
    assert "get_all_users" in query_fields
    assert "version" in query_fields  # From QueryRoot
    assert "post_count" in query_fields  # From QueryRoot


@pytest.mark.asyncio
async def test_query_decorator_execution():
    """Test that decorated queries can be executed."""
    schema = build_fraiseql_schema(
        query_types=[QueryRoot]
    )
    
    # Test get_user query
    query = """
        query GetUser($id: ID!) {
            get_user(id: $id) {
                id
                name
                email
            }
        }
    """
    
    result = await graphql(
        schema,
        query,
        variable_values={"id": "123e4567-e89b-12d3-a456-426614174000"},
        context_value={}
    )
    
    assert result.errors is None
    assert result.data == {
        "get_user": {
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "name": "John Doe",
            "email": "john@example.com"
        }
    }


@pytest.mark.asyncio
async def test_field_decorator_execution():
    """Test that @field decorated methods work."""
    schema = build_fraiseql_schema(
        query_types=[QueryRoot]
    )
    
    query = """
        query {
            version
            post_count
        }
    """
    
    result = await graphql(schema, query, context_value={})
    
    assert result.errors is None
    assert result.data == {
        "version": "2.0.0",
        "post_count": 42
    }


def test_query_decorator_with_empty_parentheses():
    """Test that @query() with parentheses works."""
    @fraiseql.query()
    async def get_posts(info) -> list[Post]:
        return [
            Post(
                id=UUID("323e4567-e89b-12d3-a456-426614174002"),
                title="Hello World",
                content="Test content"
            )
        ]
    
    schema = build_fraiseql_schema()
    
    query_fields = schema.query_type.fields
    assert "get_posts" in query_fields


def test_mixed_decorators_and_explicit_queries():
    """Test mixing @query decorator with explicit query list."""
    # Define a non-decorated query
    async def get_post(info, id: UUID) -> Optional[Post]:
        if str(id) == "323e4567-e89b-12d3-a456-426614174002":
            return Post(
                id=id,
                title="Test Post",
                content="Test content"
            )
        return None
    
    # Build schema with both decorated and explicit queries
    schema = build_fraiseql_schema(
        query_types=[QueryRoot, get_post]  # Mix types and functions
    )
    
    query_fields = schema.query_type.fields
    # Should have all queries
    assert "get_user" in query_fields  # From @query decorator
    assert "get_all_users" in query_fields  # From @query decorator
    assert "get_post" in query_fields  # From explicit list
    assert "version" in query_fields  # From QueryRoot @field
    assert "post_count" in query_fields  # From QueryRoot @field


def test_no_queries_error():
    """Test that schema building fails without any queries."""
    # Clear all registered queries first
    registry = SchemaRegistry.get_instance()
    registry._queries.clear()
    
    with pytest.raises(TypeError, match="Type Query must define one or more fields"):
        build_fraiseql_schema()