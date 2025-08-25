import pytest

"""Tests for mutation error-as-data functionality."""

import fraiseql
from fraiseql.mutations.decorators import failure, success
from fraiseql.mutations.error_config import STRICT_STATUS_CONFIG
from fraiseql.mutations.mutation_decorator import mutation
from fraiseql.mutations.parser import parse_mutation_result
from fraiseql.types.errors import Error
from fraiseql.types.fraise_input import fraise_input


@pytest.mark.unit
@fraise_input
class MutationTestInput:
    name: str
    validate: bool = True


@fraiseql.type
class MutationTestEntity:
    id: str
    name: str


@success
class MutationTestSuccess:
    message: str
    entity: MutationTestEntity
    status: str = "success"


@failure
class MutationTestError:
    message: str
    code: str
    status: str
    errors: list[Error] | None = None


class TestMutationErrorAsData:
    """Test mutations that return errors as data."""

    def test_noop_status_returned_as_data(self) -> None:
        """Test that noop: statuses are returned as data with strict status config."""
        # Simulate PostgreSQL result with noop status
        result = {
            "id": None,
            "updated_fields": [],
            "status": "noop:invalid_input",
            "message": "Input validation failed",
            "object_data": None,
            "extra_metadata": {"code": "noop:invalid_input", "reason": "missing_required_field"},
        }

        # Parse with strict status config - should return error type as data
        parsed = parse_mutation_result(
            result, MutationTestSuccess, MutationTestError, STRICT_STATUS_CONFIG
        )

        # Should be MutationTestError instance (not raised as GraphQL error)
        assert isinstance(parsed, MutationTestError)
        assert parsed.status == "noop:invalid_input"
        assert parsed.message == "Input validation failed"
        assert parsed.code == "noop:invalid_input"

    def test_failed_status_triggers_error(self) -> None:
        """Test that failed: statuses still trigger errors with strict status config."""
        # Simulate PostgreSQL result with failed status
        result = {
            "id": None,
            "updated_fields": [],
            "status": "failed:database_error",
            "message": "Database connection failed",
            "object_data": None,
            "extra_metadata": {"code": "failed:database_error", "error": "connection timeout"},
        }

        # Parse with strict status config - should return error type
        parsed = parse_mutation_result(
            result, MutationTestSuccess, MutationTestError, STRICT_STATUS_CONFIG
        )

        # Should be MutationTestError instance
        assert isinstance(parsed, MutationTestError)
        assert parsed.status == "failed:database_error"
        assert parsed.message == "Database connection failed"

    def test_success_status_returns_success_type(self) -> None:
        """Test that success statuses return success type."""
        # Simulate successful PostgreSQL result
        result = {
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "updated_fields": ["name"],
            "status": "new",  # strict status success status
            "message": "Entity created successfully",
            "object_data": {"id": "123e4567-e89b-12d3-a456-426614174000", "name": "Test Entity"},
            "extra_metadata": None,
        }

        # Parse with strict status config
        parsed = parse_mutation_result(
            result, MutationTestSuccess, MutationTestError, STRICT_STATUS_CONFIG
        )

        # Should be MutationTestSuccess instance
        assert isinstance(parsed, MutationTestSuccess)
        assert parsed.status == "new"
        assert parsed.message == "Entity created successfully"
        assert parsed.entity.id == "123e4567-e89b-12d3-a456-426614174000"
        assert parsed.entity.name == "Test Entity"

    def test_mutation_with_fraiseql_config(self) -> None:
        """Test mutation decorator with strict status error config."""

        @mutation(function="test_mutation", schema="test", error_config=STRICT_STATUS_CONFIG)
        class TestMutation:
            input: MutationTestInput
            success: MutationTestSuccess
            failure: MutationTestError

        definition = TestMutation.__fraiseql_mutation__
        assert definition.error_config == STRICT_STATUS_CONFIG
        assert definition.function_name == "test_mutation"
        assert definition.schema == "test"

    def test_blocked_status_returned_as_data(self) -> None:
        """Test that blocked: statuses are returned as data."""
        result = {
            "id": None,
            "updated_fields": [],
            "status": "blocked:children",
            "message": "Cannot delete - has child records",
            "object_data": None,
            "extra_metadata": {"code": "blocked:children", "child_count": 5},
        }

        parsed = parse_mutation_result(
            result, MutationTestSuccess, MutationTestError, STRICT_STATUS_CONFIG
        )

        assert isinstance(parsed, MutationTestError)
        assert parsed.status == "blocked:children"
        assert parsed.message == "Cannot delete - has child records"
