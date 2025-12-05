"""Tests for user-friendly error messages."""

import pytest

from fraiseql.errors.user_friendly import (
    FraiseQLError,
    InvalidFieldTypeError,
    MissingDatabaseViewError,
    MissingTypeHintError,
    MutationNotFoundError,
    SQLGenerationError,
)

pytestmark = pytest.mark.integration


@pytest.mark.unit
class TestFraiseQLError:
    """Test base error class."""

    def test_basic_error_message(self) -> None:
        """Test basic error with message only."""
        error = FraiseQLError("Something went wrong")
        assert str(error) == "Something went wrong"
        assert error.code == "FRAISEQL_ERROR"
        assert error.suggestion is None
        assert error.doc_link is None

    def test_error_with_all_fields(self) -> None:
        """Test error with all optional fields."""
        error = FraiseQLError(
            message="Invalid configuration",
            code="CONFIG_ERROR",
            suggestion="Check your database URL format",
            doc_link="https://docs.fraiseql.com/errors/config",
        )

        expected = (
            """Invalid configuration\n"""
            """Suggestion: Check your database URL format\n"""
            """See: https://docs.fraiseql.com/errors/config"""
        )
        assert str(error) == expected
        assert error.code == "CONFIG_ERROR"

    def test_error_with_context(self) -> None:
        """Test error with context information."""
        error = FraiseQLError(
            message="Query failed", context={"query_name": "getUsers", "duration": 1.23}
        )
        assert error.context == {"query_name": "getUsers", "duration": 1.23}


class TestMissingTypeHintError:
    """Test missing type hint errors."""

    def test_missing_type_hint_for_field(self) -> None:
        """Test error for missing field type hint."""
        error = MissingTypeHintError(class_name="User", field_name="email")

        expected = (
            """Field 'email' in class 'User' is missing a type hint.\n"""
            """Suggestion: Add a type annotation like: email: str\n"""
            """See: https://docs.fraiseql.com/errors/missing-type-hint"""
        )
        assert str(error) == expected
        assert error.code == "MISSING_TYPE_HINT"

    def test_missing_type_hint_with_example_type(self) -> None:
        """Test error with suggested type."""
        error = MissingTypeHintError(
            class_name="Post", field_name="created_at", suggested_type="datetime"
        )

        assert "created_at: datetime" in str(error)


class TestMissingDatabaseViewError:
    """Test missing database view errors."""

    def test_missing_view_error(self) -> None:
        """Test error for missing database view."""
        error = MissingDatabaseViewError(type_name="User", expected_view="v_users")

        expected = (
            """Database view 'v_users' for type 'User' not found.\n"""
            """Suggestion: Create a view named 'v_users' that returns a 'data' JSONB column:\n"""
            """\n"""
            """CREATE VIEW v_users AS\n"""
            """SELECT jsonb_build_object(\n"""
            """    'id', id,\n"""
            """    'email', email,\n"""
            """    'name', name\n"""
            """) as data\n"""
            """FROM users;\n"""
            """\n"""
            """See: https://docs.fraiseql.com/errors/missing-view"""
        )
        assert str(error) == expected

    def test_missing_view_with_custom_name(self) -> None:
        """Test error with custom view name."""
        error = MissingDatabaseViewError(
            type_name="Product", expected_view="product_catalog", custom_view_name=True
        )

        assert "product_catalog" in str(error)
        assert "v_products" not in str(error)


class TestInvalidFieldTypeError:
    """Test invalid field type errors."""

    def test_unsupported_type_error(self) -> None:
        """Test error for unsupported Python type."""
        error = InvalidFieldTypeError(
            class_name="User",
            field_name="metadata",
            field_type="set",
            supported_types=["dict", "list", "str", "int", "float", "bool"],
        )

        expected = (
            """Field 'metadata' in class 'User' has unsupported type 'set'.\n"""
            """Suggestion: Use one of the supported types: dict, list, str, int, float, bool\n"""
            """See: https://docs.fraiseql.com/errors/invalid-field-type"""
        )
        assert str(error) == expected

    def test_invalid_type_with_conversion_hint(self) -> None:
        """Test error with type conversion suggestion."""
        error = InvalidFieldTypeError(
            class_name="Config",
            field_name="options",
            field_type="set",
            conversion_hint="Convert to list: options: list[str]",
        )

        assert "Convert to list: options: list[str]" in str(error)


class TestSQLGenerationError:
    """Test SQL generation errors."""

    def test_sql_generation_error(self) -> None:
        """Test error during SQL generation."""
        error = SQLGenerationError(
            operation="WHERE clause generation",
            reason="Unsupported operator 'regex'",
            query_info={"field": "email", "operator": "regex", "value": ".*@example.com"},
        )

        expected = (
            """Failed to generate SQL for WHERE clause generation: Unsupported operator 'regex'\n"""
            """Suggestion: Check the GraphQL query syntax and supported operators\n"""
            """See: https://docs.fraiseql.com/errors/sql-generation"""
        )
        assert str(error) == expected
        assert error.context["query_info"]["operator"] == "regex"

    def test_sql_generation_with_suggestion(self) -> None:
        """Test SQL generation error with custom suggestion."""
        error = SQLGenerationError(
            operation="JOIN generation",
            reason="Circular reference detected",
            custom_suggestion="Remove the circular reference between User and Post types",
        )

        assert "Remove the circular reference" in str(error)


class TestMutationNotFoundError:
    """Test mutation not found errors."""

    def test_mutation_not_found(self) -> None:
        """Test error for missing mutation function."""
        error = MutationNotFoundError(
            mutation_name="createUser", function_name="graphql.create_user"
        )

        expected = (
            """PostgreSQL function 'graphql.create_user' for mutation 'createUser' not found.\n"""
            """Suggestion: Create the function in your database:\n"""
            """\n"""
            """CREATE FUNCTION graphql.create_user(input jsonb)\n"""
            """RETURNS jsonb AS $$\n"""
            """BEGIN\n"""
            """    -- Your mutation logic here\n"""
            """    RETURN jsonb_build_object(\n"""
            """        'type', 'success',\n"""
            """        'message', 'Created successfully'\n"""
            """    );\n"""
            """END;\n"""
            """$$ LANGUAGE plpgsql;\n"""
            """\n"""
            """See: https://docs.fraiseql.com/errors/missing-mutation"""
        )
        assert str(error) == expected

    def test_mutation_with_available_functions(self) -> None:
        """Test mutation error showing available functions."""
        error = MutationNotFoundError(
            mutation_name="updateUser",
            function_name="graphql.update_user",
            available_functions=["graphql.create_user", "graphql.delete_user"],
        )

        result = str(error)
        assert "Available mutations: graphql.create_user, graphql.delete_user" in result


class TestErrorChaining:
    """Test error chaining and causality."""

    def test_error_with_cause(self) -> None:
        """Test error with underlying cause."""
        msg = "Database connection failed"
        with pytest.raises(ValueError) as exc_info:
            raise ValueError(msg)

        e = exc_info.value
        error = FraiseQLError(message="Failed to execute query", cause=e)
        assert error.__cause__ == e
        assert "Database connection failed" in str(e)

    def test_error_context_preservation(self) -> None:
        """Test that context is preserved through error chain."""
        original = SQLGenerationError(
            operation="WHERE clause",
            reason="Invalid syntax",
            query_info={"field": "id", "value": "abc"},
        )

        wrapped = FraiseQLError(
            message="Query execution failed", cause=original, context={"request_id": "123"}
        )

        assert wrapped.__cause__ == original
        assert wrapped.context["request_id"] == "123"
        assert original.context["query_info"]["field"] == "id"
