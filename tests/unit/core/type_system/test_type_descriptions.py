"""Tests for automatic docstring extraction in FraiseQL type descriptions."""

import pytest
from graphql import GraphQLObjectType, build_schema, print_schema

import fraiseql
from fraiseql.core.graphql_type import convert_type_to_graphql_output


class TestTypeDescriptions:
    """Test that @fraise_type classes automatically use docstrings for GraphQL descriptions."""

    def test_fraise_type_uses_docstring_as_description(self):
        """Test that @fraise_type decorated classes use their docstring as GraphQL type description."""

        @fraiseql.type(sql_source="test_table")
        class TestUser:
            """A user in the system with authentication and profile data."""

            id: int
            name: str
            email: str

        # Convert to GraphQL type
        gql_type = convert_type_to_graphql_output(TestUser)

        # Should be GraphQLObjectType
        assert isinstance(gql_type, GraphQLObjectType)

        # Should use docstring as description
        assert gql_type.description == "A user in the system with authentication and profile data."

    def test_fraise_type_without_docstring_has_no_description(self):
        """Test that @fraise_type classes without docstrings have no description."""

        @fraiseql.type(sql_source="test_table")
        class TestProduct:
            id: int
            name: str
            price: float

        # Convert to GraphQL type
        gql_type = convert_type_to_graphql_output(TestProduct)

        # Should be GraphQLObjectType
        assert isinstance(gql_type, GraphQLObjectType)

        # Should have no description (None)
        assert gql_type.description is None

    def test_fraise_type_multiline_docstring_is_cleaned(self):
        """Test that multiline docstrings are properly cleaned and formatted."""

        @fraiseql.type(sql_source="test_table")
        class TestOrder:
            """
            An order in the e-commerce system.

            Contains line items, customer information,
            and payment details.
            """

            id: int
            customer_id: int
            total: float

        # Convert to GraphQL type
        gql_type = convert_type_to_graphql_output(TestOrder)

        # Should be GraphQLObjectType
        assert isinstance(gql_type, GraphQLObjectType)

        # Should clean the docstring and use first meaningful line
        # For now, expect the full cleaned docstring
        expected_description = "An order in the e-commerce system.\n\nContains line items, customer information,\nand payment details."
        assert gql_type.description == expected_description

    def test_fraise_type_description_in_built_schema(self):
        """Test that type descriptions appear in the full GraphQL schema."""

        @fraiseql.type(sql_source="posts")
        class Post:
            """A blog post with content and metadata."""

            id: int
            title: str
            content: str

        # Build a simple schema
        from fraiseql.gql.schema_builder import build_fraiseql_schema

        @fraiseql.query
        async def test_query(info) -> str:
            """Test query."""
            return "test"

        schema = build_fraiseql_schema(
            query_types=[Post],
            mutation_resolvers=[],
        )

        # Check that the Post type has the correct description
        post_type = schema.type_map.get("Post")
        assert post_type is not None
        assert isinstance(post_type, GraphQLObjectType)
        assert post_type.description == "A blog post with content and metadata."

    def test_fraise_type_description_preserved_with_existing_functionality(self):
        """Test that adding docstring descriptions doesn't break existing field descriptions."""

        @fraiseql.type(sql_source="users")
        class DetailedUser:
            """A comprehensive user model with rich metadata."""

            id: int
            name: str = fraiseql.fraise_field(description="Full name of the user")
            email: str = fraiseql.fraise_field(description="Primary email address")

        # Convert to GraphQL type
        gql_type = convert_type_to_graphql_output(DetailedUser)

        # Should be GraphQLObjectType
        assert isinstance(gql_type, GraphQLObjectType)

        # Type should have docstring as description
        assert gql_type.description == "A comprehensive user model with rich metadata."

        # Fields should still have their explicit descriptions
        name_field = gql_type.fields["name"]
        email_field = gql_type.fields["email"]

        assert name_field.description == "Full name of the user"
        assert email_field.description == "Primary email address"
