"""Tests for mutation result parser."""

import pytest

from fraiseql.mutations.decorators import failure, success
from fraiseql.mutations.parser import (
    _find_main_field,
    _instantiate_type,
    _is_error_status,
    parse_mutation_result,
)
from fraiseql.types.fraise_type import fraise_type


@fraise_type
class User:
    id: str
    name: str
    email: str


@fraise_type
class Order:
    id: str
    order_number: str
    total: float


@success
class CreateUserSuccess:
    message: str
    user: User


@failure
class CreateUserError:
    message: str
    conflict_user: User | None = None
    suggested_email: str | None = None


@success
class BulkUpdateSuccess:
    message: str
    affected_orders: list[Order]
    skipped_orders: list[Order] | None = None
    processing_time_ms: float | None = None


@failure
class BulkUpdateError:
    message: str
    failed_orders: list[Order] | None = None


@pytest.mark.usefixtures("clear_registry")
class TestStatusDetection:
    """Test error status detection."""

    def test_success_status(self) -> None:
        """Test success status detection."""
        assert not _is_error_status("success")
        assert not _is_error_status("completed")
        assert not _is_error_status("ok")

    def test_error_status(self) -> None:
        """Test error status detection."""
        assert _is_error_status("error")
        assert _is_error_status("failed")
        assert _is_error_status("not_found")
        assert _is_error_status("forbidden")
        assert _is_error_status("validation_error")
        assert _is_error_status("conflict")

    def test_case_insensitive(self) -> None:
        """Test case insensitive status detection."""
        assert _is_error_status("ERROR")
        assert _is_error_status("FAILED")
        assert _is_error_status("Not_Found")


@pytest.mark.usefixtures("clear_registry")
class TestSuccessfulResults:
    """Test parsing successful mutation results."""

    def test_simple_success_with_object_data(self) -> None:
        """Test parsing simple success result."""
        result = {
            "status": "success",
            "message": "User created successfully",
            "object_data": {"id": "123", "name": "John Doe", "email": "john@example.com"},
        }

        parsed = parse_mutation_result(result, CreateUserSuccess, CreateUserError)

        assert isinstance(parsed, CreateUserSuccess)
        assert parsed.message == "User created successfully"
        assert isinstance(parsed.user, User)
        assert parsed.user.id == "123"
        assert parsed.user.name == "John Doe"
        assert parsed.user.email == "john@example.com"

    def test_success_with_metadata_fields(self) -> None:
        """Test success with additional fields from metadata."""
        result = {
            "status": "success",
            "message": "Bulk update completed",
            "object_data": [
                {"id": "1", "order_number": "ORD-001", "total": 100.0},
                {"id": "2", "order_number": "ORD-002", "total": 200.0},
            ],
            "extra_metadata": {
                "entity": "affected_orders",
                "processing_time_ms": 150.5,
                "skipped_orders": [{"id": "3", "order_number": "ORD-003", "total": 50.0}],
            },
        }

        parsed = parse_mutation_result(result, BulkUpdateSuccess, BulkUpdateError)

        assert isinstance(parsed, BulkUpdateSuccess)
        assert parsed.message == "Bulk update completed"
        assert len(parsed.affected_orders) == 2
        assert all(isinstance(order, Order) for order in parsed.affected_orders)
        assert parsed.processing_time_ms == 150.5
        assert len(parsed.skipped_orders) == 1
        assert isinstance(parsed.skipped_orders[0], Order)


@pytest.mark.usefixtures("clear_registry")
class TestErrorResults:
    """Test parsing error mutation results."""

    def test_simple_error(self) -> None:
        """Test parsing simple error result."""
        result = {"status": "validation_error", "message": "Invalid input data"}

        parsed = parse_mutation_result(result, CreateUserSuccess, CreateUserError)

        assert isinstance(parsed, CreateUserError)
        assert parsed.message == "Invalid input data"
        assert parsed.conflict_user is None
        assert parsed.suggested_email is None

    def test_error_with_metadata(self) -> None:
        """Test error with additional data from metadata."""
        result = {
            "status": "email_exists",
            "message": "Email already registered",
            "extra_metadata": {
                "conflict_user": {
                    "id": "456",
                    "name": "Existing User",
                    "email": "existing@example.com",
                },
                "suggested_email": "john.doe.2@example.com",
            },
        }

        parsed = parse_mutation_result(result, CreateUserSuccess, CreateUserError)

        assert isinstance(parsed, CreateUserError)
        assert parsed.message == "Email already registered"
        assert isinstance(parsed.conflict_user, User)
        assert parsed.conflict_user.id == "456"
        assert parsed.suggested_email == "john.doe.2@example.com"


@pytest.mark.usefixtures("clear_registry")
class TestTypeInstantiation:
    """Test type instantiation logic."""

    def test_instantiate_primitive_types(self) -> None:
        """Test instantiating primitive types."""
        assert _instantiate_type(str, "hello") == "hello"
        assert _instantiate_type(int, 42) == 42
        assert _instantiate_type(float, 3.14) == 3.14
        assert _instantiate_type(bool, True) is True

    def test_instantiate_none(self) -> None:
        """Test instantiating None values."""
        assert _instantiate_type(str, None) is None
        assert _instantiate_type(User, None) is None

    def test_instantiate_list_of_objects(self) -> None:
        """Test instantiating list of complex objects."""
        data = [
            {"id": "1", "order_number": "ORD-001", "total": 100.0},
            {"id": "2", "order_number": "ORD-002", "total": 200.0},
        ]

        result = _instantiate_type(list[Order], data)

        assert isinstance(result, list)
        assert len(result) == 2
        assert all(isinstance(order, Order) for order in result)
        assert result[0].id == "1"
        assert result[1].order_number == "ORD-002"

    def test_instantiate_fraise_type(self) -> None:
        """Test instantiating FraiseQL types."""
        data = {"id": "123", "name": "John", "email": "john@example.com"}

        result = _instantiate_type(User, data)

        assert isinstance(result, User)
        assert result.id == "123"
        assert result.name == "John"
        assert result.email == "john@example.com"

    def test_instantiate_optional_type(self) -> None:
        """Test instantiating optional (union): types."""
        # Test with None
        result = _instantiate_type(User | None, None)
        assert result is None

        # Test with actual data
        data = {"id": "123", "name": "John", "email": "john@example.com"}
        result = _instantiate_type(User | None, data)
        assert isinstance(result, User)
        assert result.id == "123"


@pytest.mark.usefixtures("clear_registry")
class TestMainFieldDetection:
    """Test main field detection logic."""

    def test_find_main_field_with_entity_hint(self) -> None:
        """Test finding main field with entity hint."""
        annotations = {"message": str, "user": User, "extra_data": dict}
        metadata = {"entity": "user"}

        result = _find_main_field(annotations, metadata)
        assert result == "user"

    def test_find_main_field_with_entity_suffix(self) -> None:
        """Test finding main field with entity suffix matching."""
        annotations = {"message": str, "affected_orders": list[Order], "count": int}
        metadata = {"entity": "affected_order"}  # Singular form,

        result = _find_main_field(annotations, metadata)
        assert result == "affected_orders"  # Should match with 's' suffix

    def test_find_main_field_first_non_message(self) -> None:
        """Test finding first non-message field when no entity hint."""
        annotations = {"message": str, "user": User, "timestamp": str}

        result = _find_main_field(annotations, None)
        assert result == "user"  # First non-message field

    def test_find_main_field_no_fields(self) -> None:
        """Test finding main field when only message exists."""
        annotations = {"message": str}

        result = _find_main_field(annotations, None)
        assert result is None


@pytest.mark.usefixtures("clear_registry")
class TestComplexScenarios:
    """Test complex parsing scenarios."""

    def test_empty_metadata(self) -> None:
        """Test handling empty metadata."""
        result = {
            "status": "success",
            "message": "Done",
            "object_data": {"id": "123", "name": "Test", "email": "test@example.com"},
            "extra_metadata": None,
        }

        parsed = parse_mutation_result(result, CreateUserSuccess, CreateUserError)
        assert isinstance(parsed, CreateUserSuccess)
        assert parsed.user.id == "123"

    def test_empty_object_data(self) -> None:
        """Test handling empty object data."""
        result = {
            "status": "success",
            "message": "Task completed",
            "object_data": None,
            "extra_metadata": {"user": {"id": "123", "name": "Test", "email": "test@example.com"}},
        }

        parsed = parse_mutation_result(result, CreateUserSuccess, CreateUserError)
        assert isinstance(parsed, CreateUserSuccess)
        assert parsed.user.id == "123"
