"""Tests for developer experience improvements."""

from dataclasses import dataclass
from typing import Optional

import pytest

from fraiseql.debug import debug_partial_instance
from fraiseql.errors.exceptions import (
    DatabaseQueryError,
    PartialInstantiationError,
    QueryValidationError,
    WhereClauseError,
)
from fraiseql.gql.schema_builder import SchemaRegistry
from fraiseql.partial_instantiation import create_partial_instance
from fraiseql.validation import (
    _get_type_fields,
    validate_query_complexity,
    validate_where_input,
)

pytestmark = [pytest.mark.integration, pytest.mark.database]


@pytest.fixture(autouse=True)
def clear_registry() -> None:
    """Clear registry before each test to avoid type conflicts."""
    registry = SchemaRegistry.get_instance()
    registry.clear()

    # Also clear the GraphQL type cache
    from fraiseql.core.graphql_type import _graphql_type_cache

    _graphql_type_cache.clear()

    yield

    registry.clear()
    _graphql_type_cache.clear()


# Define User at module level but not decorated
@dataclass
class User:
    """Test user type."""

    id: int
    name: str
    email: str
    age: Optional[int] = None
    active: bool = True


class TestEnhancedExceptions:
    """Test the enhanced exception classes."""

    def test_partial_instantiation_error(self) -> None:
        """Test PartialInstantiationError with context and hints."""
        error = PartialInstantiationError(
            type_name="User",
            field_name="email",
            reason="Invalid email format",
            available_fields={"id", "name"},
            requested_fields={"id", "name", "email", "age"},
        )

        error_str = str(error)
        assert "PARTIAL_INSTANTIATION_ERROR" in error_str
        assert "Failed to instantiate partial User due to field 'email'" in error_str
        assert "Missing fields in data: age, email" in error_str
        assert "Available fields: id, name" in error_str
        assert "Check that your database view" in error_str

    def test_where_clause_error(self) -> None:
        """Test WhereClauseError with operator hints."""
        error = WhereClauseError(
            """Invalid operator '_between' for field 'age'""",
            where_input={"age": {"_between": [18, 65]}},
            field_name="age",
            operator="_between",
            supported_operators=["_eq", "_neq", "_gt", "_gte", "_lt", "_lte"],
        )

        error_str = str(error)
        assert "WHERE_CLAUSE_ERROR" in error_str
        assert "Supported operators: _eq, _neq, _gt, _gte, _lt, _lte" in error_str

    def test_query_validation_error_with_typo_hint(self) -> None:
        """Test QueryValidationError with typo suggestions."""
        error = QueryValidationError(
            """Invalid fields requested""",
            type_name="User",
            invalid_fields=["emial", "naem"],
            valid_fields=["id", "name", "email", "age"],
        )

        error_str = str(error)
        assert "QUERY_VALIDATION_ERROR" in error_str
        assert "Valid fields for User: age, email, id, name" in error_str
        # Note: The typo detection would need the fields to match case-insensitively

    def test_database_query_error_with_hints(self) -> None:
        """Test DatabaseQueryError with helpful hints."""
        error = DatabaseQueryError(
            """relation 'users_view' does not exist""",
            sql="SELECT * FROM users_view",
            view_name="users_view",
            cause=Exception("relation 'users_view' does not exist"),
        )

        error_str = str(error)
        assert "DATABASE_QUERY_ERROR" in error_str
        assert "Create the view 'users_view'" in error_str
        assert "Run 'fraiseql generate views'" in error_str


class TestPartialInstantiationWithErrors:
    """Test partial instantiation with better error handling."""

    def test_create_partial_instance_success(self) -> None:
        """Test successful partial instantiation."""
        data = {"id": 1, "name": "John"}
        user = create_partial_instance(User, data)

        assert user.id == 1
        assert user.name == "John"
        assert user.email is None  # Missing field set to None
        assert hasattr(user, "__fraiseql_partial__")
        assert user.__fraiseql_partial__ is True
        assert user.__fraiseql_fields__ == {"id", "name"}

    def test_create_partial_instance_with_invalid_type(self) -> None:
        """Test partial instantiation with type mismatch."""
        # This depends on how strict the type checking is
        # Some type mismatches might be caught, others might not
        data = {"id": 1, "name": "John", "age": 25}
        user = create_partial_instance(User, data)
        assert user.age == 25

    def test_debug_partial_instance(self) -> None:
        """Test debug output for partial instances."""
        data = {"id": 1, "name": "Alice"}
        user = create_partial_instance(User, data)

        debug_output = debug_partial_instance(user)

        assert "User (PARTIAL)" in debug_output
        assert "Requested fields: {id, name}" in debug_output
        assert "- id: 1" in debug_output
        assert '- name: "Alice"' in debug_output
        assert "Missing fields:" in debug_output
        assert "- email (not requested)" in debug_output


class TestValidationUtilities:
    """Test validation utilities."""

    def test_validate_where_input_valid(self) -> None:
        """Test validation of valid where input."""
        where_input = {
            "name": {"_eq": "John"},
            "age": {"_gte": 18},
            "_and": [{"active": {"_eq": True}}, {"email": {"_like": "%@example.com"}}],
        }

        errors = validate_where_input(where_input, User)
        assert errors == []

    def test_validate_where_input_unknown_field(self) -> None:
        """Test validation with unknown field."""
        where_input = {"unknown_field": {"_eq": "value"}}

        errors = validate_where_input(where_input, User)
        assert len(errors) == 1
        assert "Unknown field 'unknown_field'" in errors[0]
        assert "Available fields:" in errors[0]

    def test_validate_where_input_invalid_operator(self) -> None:
        """Test validation with invalid operator."""
        where_input = {"name": {"_invalid": "value"}}

        errors = validate_where_input(where_input, User)
        assert len(errors) == 1
        assert "Unknown operator '_invalid'" in errors[0]

    def test_validate_where_input_type_mismatch(self) -> None:
        """Test validation with operator type mismatch."""
        # String operator on non-string field
        where_input = {"age": {"_like": "%25%"}}

        errors = validate_where_input(where_input, User)
        assert len(errors) == 1
        assert "String operator '_like'" in errors[0]
        assert "can only be used with string fields" in errors[0]

    def test_validate_where_input_strict_mode(self) -> None:
        """Test validation in strict mode raises exceptions."""
        where_input = {"invalid_field": {"_eq": "value"}}

        with pytest.raises(WhereClauseError) as exc_info:
            validate_where_input(where_input, User, strict=True)

        assert "Unknown field 'invalid_field'" in str(exc_info.value)

    def test_validate_where_input_case_sensitivity_hint(self) -> None:
        """Test validation provides case sensitivity hints."""
        where_input = {"EMAIL": {"_eq": "test@example.com"}}

        errors = validate_where_input(where_input, User)
        assert len(errors) == 1
        assert "Did you mean 'email' instead of 'EMAIL'?" in errors[0]

    def test_get_type_fields(self) -> None:
        """Test field extraction from types."""
        fields = _get_type_fields(User)
        assert fields == {"id", "name", "email", "age", "active"}


class TestQueryComplexity:
    """Test query complexity validation."""

    def test_validate_query_complexity_simple(self) -> None:
        """Test complexity calculation for simple queries."""
        # This would need a mock GraphQLResolveInfo object
        # For now, we just test the function exists and returns expected types
        from unittest.mock import Mock

        mock_info = Mock()
        mock_info.field_nodes = []

        complexity, errors = validate_query_complexity(mock_info, max_complexity=100)
        assert isinstance(complexity, int)
        assert isinstance(errors, list)
        assert complexity == 0  # No fields selected
        assert errors == []
