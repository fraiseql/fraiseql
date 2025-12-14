import pytest

"""Tests for generic type support in FraiseQL."""

from typing import Generic, TypeVar

import fraiseql
from fraiseql.core.graphql_type import convert_type_to_graphql_output
from fraiseql.types.generic import (
    Connection,
    Edge,
    PageInfo,
    PaginatedResponse,
    create_concrete_type,
    get_or_create_concrete_type,
    is_fraise_generic,
    is_generic_type,
)


@pytest.mark.unit
class TestGenericTypes:
    """Test generic type functionality."""

    def test_pagination_types_are_defined(self, clear_registry) -> None:
        """Test that the basic pagination types are defined correctly."""
        # PageInfo should be a regular FraiseQL type
        assert hasattr(PageInfo, "__fraiseql_definition__")
        assert PageInfo.__fraiseql_definition__.kind == "type"

        # Edge and Connection should also be FraiseQL types
        assert hasattr(Edge, "__fraiseql_definition__")
        assert hasattr(Connection, "__fraiseql_definition__")

        # PaginatedResponse should be an alias for Connection
        assert PaginatedResponse is Connection

    def test_is_generic_type_detection(self, clear_registry) -> None:
        """Test detection of generic types."""
        # Basic types should not be generic
        assert not is_generic_type(str)
        assert not is_generic_type(int)
        assert not is_generic_type(PageInfo)

        # Generic types should be detected
        assert is_generic_type(Connection[str])
        assert is_generic_type(Edge[int])
        assert is_generic_type(list[str])

    def test_is_fraise_generic_detection(self, clear_registry) -> None:
        """Test detection of FraiseQL generic types."""

        # Create a test type
        @fraiseql.type
        class User:
            id: str = fraiseql.fraise_field(description="User ID")
            name: str = fraiseql.fraise_field(description="User name")

        # FraiseQL generic types should be detected
        assert is_fraise_generic(Connection[User])
        assert is_fraise_generic(Edge[User])

        # Regular generic types should not be detected as FraiseQL generics
        assert not is_fraise_generic(list[User])
        assert not is_fraise_generic(dict[str, User])

    def test_concrete_type_creation(self, clear_registry) -> None:
        """Test creation of concrete types from generics."""

        @fraiseql.type
        class Post:
            id: str = fraiseql.fraise_field(description="Post ID")
            title: str = fraiseql.fraise_field(description="Post title")

        # Create concrete type
        ConcreteConnection = create_concrete_type(Connection, Post)

        # Should have a unique name
        assert ConcreteConnection.__name__ == "ConnectionPost"

        # Should be a valid FraiseQL type
        assert hasattr(ConcreteConnection, "__fraiseql_definition__")
        assert ConcreteConnection.__fraiseql_definition__.kind == "type"

        # Should have the right fields with concrete types
        fields = getattr(ConcreteConnection, "__gql_fields__", {})
        assert "edges" in fields
        assert "page_info" in fields
        assert "total_count" in fields

    def test_concrete_type_caching(self, clear_registry) -> None:
        """Test that concrete types are cached for efficiency."""

        @fraiseql.type
        class User:
            id: str = fraiseql.fraise_field(description="User ID")

        # First call should create the type
        concrete1 = get_or_create_concrete_type(Connection, User)

        # Second call should return the same instance
        concrete2 = get_or_create_concrete_type(Connection, User)

        assert concrete1 is concrete2

    def test_graphql_conversion_with_generics(self, clear_registry) -> None:
        """Test that generic types convert to GraphQL correctly."""

        @fraiseql.type
        class Product:
            id: str = fraiseql.fraise_field(description="Product ID")
            name: str = fraiseql.fraise_field(description="Product name")
            price: float = fraiseql.fraise_field(description="Product price")

        # Convert Connection[Product] to GraphQL
        graphql_type = convert_type_to_graphql_output(Connection[Product])

        # Should be a GraphQL object type
        from graphql import GraphQLObjectType

        assert isinstance(graphql_type, GraphQLObjectType)

        # Should have the right name
        assert graphql_type.name == "ProductConnection"

        # Should have the expected fields
        fields = graphql_type.fields
        assert "edges" in fields
        assert "pageInfo" in fields  # FraiseQL uses camelCase field names now
        assert "totalCount" in fields

    def test_nested_generic_conversion(self, clear_registry) -> None:
        """Test that nested generics like list[Edge[T]] work correctly."""

        @fraiseql.type
        class Comment:
            id: str = fraiseql.fraise_field(description="Comment ID")
            text: str = fraiseql.fraise_field(description="Comment text")

        # Test Edge[Comment]
        edge_graphql = convert_type_to_graphql_output(Edge[Comment])

        from graphql import GraphQLObjectType

        assert isinstance(edge_graphql, GraphQLObjectType)
        assert edge_graphql.name == "EdgeComment"

        # Should have node and cursor fields
        fields = edge_graphql.fields
        assert "node" in fields
        assert "cursor" in fields

    def test_multiple_generic_args_error(self, clear_registry) -> None:
        """Test that generic types with multiple args raise appropriate errors."""
        T = TypeVar("T")
        U = TypeVar("U")

        @fraiseql.type
        class PairGeneric(Generic[T, U]):
            first: T = fraiseql.fraise_field(description="First item")
            second: U = fraiseql.fraise_field(description="Second item")

        @fraiseql.type
        class SampleType:
            id: str = fraiseql.fraise_field(description="ID")

        # Currently our system only supports single type parameters
        # This should either work or give a clear error
        try:
            concrete = create_concrete_type(PairGeneric, SampleType)
            # If it works, it should create a valid type
            assert hasattr(concrete, "__fraiseql_definition__")
        except (TypeError, ValueError, IndexError):
            # If it fails, that's expected for now
            pass

    def test_paginated_response_alias(self, clear_registry) -> None:
        """Test that PaginatedResponse works as an alias for Connection."""

        @fraiseql.type
        class Item:
            id: str = fraiseql.fraise_field(description="Item ID")

        # Both should produce the same result
        connection_type = convert_type_to_graphql_output(Connection[Item])
        paginated_type = convert_type_to_graphql_output(PaginatedResponse[Item])

        # Should have the same name (since PaginatedResponse is Connection)
        assert connection_type.name == paginated_type.name
