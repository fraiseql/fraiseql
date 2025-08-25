import pytest

"""Tests for field-level authorization in FraiseQL."""

import fraiseql
from fraiseql import query
from fraiseql.decorators import field
from fraiseql.gql.schema_builder import build_fraiseql_schema
from fraiseql.security.field_auth import FieldAuthorizationError, authorize_field


@pytest.mark.security
class TestFieldAuthorization:
    """Test field-level authorization functionality."""

    def test_field_auth_basic_error_handling(self):
        """Test that FieldAuthorizationError can be raised and handled."""
        # Test that the error can be instantiated
        error = FieldAuthorizationError("Test error message")
        assert str(error) == "Test error message"

        # Test with default message
        error2 = FieldAuthorizationError()
        assert "Not authorized" in str(error2)

    def test_field_auth_integration_with_graphql(self):
        """Test field authorization in actual GraphQL execution."""
        from graphql import graphql_sync

        @fraiseql.type
        class SecureData:
            public_info: str

            @authorize_field(lambda info: info.context.get("authenticated", False))
            @field
            def private_info(self) -> str:
                return "secret data"

        @query
        def secure_data(info) -> SecureData:
            return SecureData(public_info="public data")

        schema = build_fraiseql_schema(query_types=[secure_data])

        # Test authenticated access
        query_str = """
        query {
            secureData {
                publicInfo
                privateInfo
            }
        }
        """
        result = graphql_sync(schema, query_str, context_value={"authenticated": True})

        assert result.errors is None
        assert result.data == {
            "secureData": {"publicInfo": "public data", "privateInfo": "secret data"}
        }

        # Test unauthenticated access
        result = graphql_sync(schema, query_str, context_value={"authenticated": False})

        assert result.errors is not None
        assert len(result.errors) == 1
        assert "Not authorized to access field" in str(result.errors[0])
        # Public field should still be accessible
        assert result.data == {"secureData": {"publicInfo": "public data", "privateInfo": None}}
