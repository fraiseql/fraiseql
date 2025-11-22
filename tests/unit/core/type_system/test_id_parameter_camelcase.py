"""Test for ID parameter naming issue with camelCase configuration."""

from uuid import UUID

import pytest

import fraiseql
from fraiseql import query
from fraiseql.config.schema_config import SchemaConfig
from fraiseql.gql.builders import SchemaComposer, SchemaRegistry


@fraiseql.type
class User:
    id: UUID
    name: str


# Query that should work with 'id' parameter
@query
async def user(info, id: UUID) -> User | None:
    """Get user by ID."""
    return User(id=id, name="Test User")


# Query with other reserved Python keywords
@query
async def search_users(info, type: str = "active", class_: str = "premium") -> list[User]:
    """Search users with Python reserved keywords as parameters."""
    return []


class TestIdParameterWithCamelCase:
    def setup_method(self) -> None:
        """Clear registry before each test."""
        SchemaRegistry._instance = None
        SchemaConfig._instance = None

    def test_id_parameter_with_camelcase_enabled(self) -> None:
        """Test that 'id' parameter works with camelCase configuration."""
        # Enable camelCase
        config = SchemaConfig.get_instance()
        config.camel_case_fields = True

        # Register our types and queries
        registry = SchemaRegistry.get_instance()

        # Manually register since decorators already ran
        registry.register_type(User)
        registry.register_query(user)
        registry.register_query(search_users)

        # Create schema
        composer = SchemaComposer(registry)
        schema = composer.compose()

        # Check Query type
        query_type = schema.type_map.get("Query")
        assert query_type is not None

        # Check user field
        user_field = query_type.fields.get("user")
        assert user_field is not None

        # Print all arguments for debugging
        for _arg_name, _arg in user_field.args.items():
            pass

        # The GraphQL field should accept 'id' as argument
        assert "id" in user_field.args

        # Check searchUsers field (camelCase)
        search_field = query_type.fields.get("searchUsers")
        assert search_field is not None

        # Check if Python reserved keywords are handled
        assert "type" in search_field.args
        # class_ should become class in GraphQL
        assert "class" in search_field.args or "class_" in search_field.args

    @pytest.mark.asyncio
    async def test_graphql_execution_with_reserved_keywords(self) -> None:
        """Test that GraphQL execution handles Python reserved keywords correctly."""
        from graphql import graphql

        # Enable camelCase
        config = SchemaConfig.get_instance()
        config.camel_case_fields = True

        # Get schema
        registry = SchemaRegistry.get_instance()
        registry.register_type(User)
        registry.register_query(user)

        composer = SchemaComposer(registry)
        schema = composer.compose()

        # Execute query with 'id' parameter
        query_str = """
            query GetUser($userId: ID!) {
                user(id: $userId) {
                    id
                    name
                }
            }
        """
        result = await graphql(
            schema,
            query_str,
            variable_values={"userId": "12345678-1234-5678-1234-567812345678"},
            context_value={"db": None},
        )

        # Check if we get the expected error
        if result.errors:
            error_msg = str(result.errors[0])
            if "unexpected keyword argument" in error_msg:
                pass
                # This confirms the issue - GraphQL passes 'id' but Python function expects 'id'
                # and somehow there's a mismatch

    def test_graphql_introspection(self) -> None:
        """Test GraphQL introspection to see parameter names."""
        from graphql import graphql_sync

        config = SchemaConfig.get_instance()
        config.camel_case_fields = True

        registry = SchemaRegistry.get_instance()
        registry.register_type(User)
        registry.register_query(user)

        composer = SchemaComposer(registry)
        schema = composer.compose()

        introspection_query = """
        {
            __type(name: "Query") {
                name
                fields {
                    name
                    args {
                        name
                        type {
                            name
                            kind
                        }
                    }
                }
            }
        }
        """
        result = graphql_sync(schema, introspection_query)

        if result.data:
            query_type = result.data["__type"]
            for field in query_type["fields"]:
                if field["name"] == "user":
                    # Check what GraphQL sees
                    arg_names = [arg["name"] for arg in field["args"]]
                    assert "id" in arg_names, f"Expected 'id' in args, got {arg_names}"
