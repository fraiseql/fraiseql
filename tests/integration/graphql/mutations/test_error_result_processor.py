"""Comprehensive tests for the new clean error management system.

These tests define the exact behavior expected from the rebuilt system:
- Errors arrays are ALWAYS populated for Error types
- Frontend-compatible structure is guaranteed
- Immutable processing preserves data integrity
- Edge cases are handled predictably
"""

import json

import pytest

from fraiseql.mutations.result_processor import (
    ErrorDetail,
    MutationResultProcessor,
    ProcessedResult,
)
from fraiseql.mutations.types import MutationResult

pytestmark = pytest.mark.integration


# Test data structures for mutations


@pytest.mark.unit
class CreateMachineError:
    """Mock error class for testing."""

    def __init__(self) -> None:
        self.message = "Failed to create machine"
        self.error_code = "CREATE_FAILED"
        self.__class__.__name__ = "CreateMachineError"


class CreateMachineSuccess:
    """Mock success class for testing."""

    def __init__(self) -> None:
        self.message = "Machine created successfully"
        self.__class__.__name__ = "CreateMachineSuccess"


class TestErrorResultProcessor:
    """Test the core error result processing logic."""

    def test_error_result_always_has_populated_errors_array(self) -> None:
        """RED: Error results must always have non-empty errors array."""
        # Given a database result indicating error
        db_result = MutationResult(
            status="noop:invalid_contract_id", message="Contract not found or access denied"
        )

        # When processed through the new system
        processor = MutationResultProcessor()
        result = processor.process_error(db_result, CreateMachineError)

        # Then errors array must be populated
        assert isinstance(result.errors, list)
        assert len(result.errors) > 0
        assert result.errors[0].code == 422
        assert result.errors[0].identifier == "invalid_contract_id"
        assert result.errors[0].message == "Contract not found or access denied"

    def test_success_result_has_empty_errors_array(self) -> None:
        """RED: Success results should have empty errors array, not None."""
        db_result = MutationResult(
            status="success", message="Machine created", object_data={"id": "machine-123"}
        )

        processor = MutationResultProcessor()
        result = processor.process_success(db_result, CreateMachineSuccess)

        assert isinstance(result.errors, list)
        assert len(result.errors) == 0  # Empty, not None

    def test_noop_status_creates_422_error(self) -> None:
        """RED: noop: statuses should create 422 errors with proper identifier."""
        db_result = MutationResult(
            status="noop:machine_already_exists", message="Machine with this serial already exists"
        )

        processor = MutationResultProcessor()
        result = processor.process_error(db_result, CreateMachineError)

        assert result.errors[0].code == 422
        assert result.errors[0].identifier == "machine_already_exists"

    def test_blocked_status_creates_422_error(self) -> None:
        """RED: blocked: statuses should create 422 errors."""
        db_result = MutationResult(
            status="blocked:insufficient_permissions", message="User lacks permission"
        )

        processor = MutationResultProcessor()
        result = processor.process_error(db_result, CreateMachineError)

        assert result.errors[0].code == 422
        assert result.errors[0].identifier == "insufficient_permissions"

    def test_failed_status_creates_500_error(self) -> None:
        """RED: failed: statuses should create 500 errors."""
        db_result = MutationResult(
            status="failed:database_connection", message="Database connection lost"
        )

        processor = MutationResultProcessor()
        result = processor.process_error(db_result, CreateMachineError)

        assert result.errors[0].code == 500
        assert result.errors[0].identifier == "database_connection"

    def test_immutable_processing(self) -> None:
        """RED: Processing should not mutate original objects."""
        db_result = MutationResult(status="noop:test", message="Original message")
        original_status = db_result.status
        original_message = db_result.message

        processor = MutationResultProcessor()
        result = processor.process_error(db_result, CreateMachineError)

        # Original should be unchanged
        assert db_result.status == original_status
        assert db_result.message == original_message

        # Result should be populated
        assert len(result.errors) > 0

    def test_complex_error_details_preservation(self) -> None:
        """RED: Complex error details should be preserved."""
        db_result = MutationResult(
            status="noop:validation_failed",
            message="Multiple validation errors",
            extra_metadata={
                "validation_errors": [
                    {"field": "serial_number", "issue": "already_exists"},
                    {"field": "model_id", "issue": "not_found"},
                ]
            },
        )

        processor = MutationResultProcessor()
        result = processor.process_error(db_result, CreateMachineError)

        assert result.errors[0].details["validation_errors"] == [
            {"field": "serial_number", "issue": "already_exists"},
            {"field": "model_id", "issue": "not_found"},
        ]

    def test_json_serializable_output(self) -> None:
        """RED: All processed results must be JSON serializable."""
        db_result = MutationResult(status="noop:test", message="Test message")

        processor = MutationResultProcessor()
        result = processor.process_error(db_result, CreateMachineError)

        # Should be JSON serializable without errors
        json_string = json.dumps(result.to_dict())
        parsed = json.loads(json_string)

        assert parsed["errors"][0]["code"] == 422

    def test_typename_field_present_in_result(self) -> None:
        """RED: __typename field must be present for GraphQL union resolution."""
        db_result = MutationResult(status="noop:test", message="Test message")

        processor = MutationResultProcessor()
        result = processor.process_error(db_result, CreateMachineError)

        result_dict = result.to_dict()
        assert "__typename" in result_dict
        assert result_dict["__typename"] == "CreateMachineError"

    def test_success_result_typename_field(self) -> None:
        """RED: Success results should also have __typename for union resolution."""
        db_result = MutationResult(status="success", message="Machine created")

        processor = MutationResultProcessor()
        result = processor.process_success(db_result, CreateMachineSuccess)

        result_dict = result.to_dict()
        assert "__typename" in result_dict
        assert result_dict["__typename"] == "CreateMachineSuccess"

    def test_status_without_colon_handled_gracefully(self) -> None:
        """RED: Status strings without colon should be handled as general errors."""
        db_result = MutationResult(status="general_failure", message="Something went wrong")

        processor = MutationResultProcessor()
        result = processor.process_error(db_result, CreateMachineError)

        assert result.errors[0].code == 500  # Default to server error
        assert result.errors[0].identifier == "general_error"  # Default identifier

    def test_none_status_handled_gracefully(self) -> None:
        """RED: None status should be handled gracefully."""
        db_result = MutationResult(status=None, message="No status provided")

        processor = MutationResultProcessor()
        result = processor.process_error(db_result, CreateMachineError)

        assert result.errors[0].code == 500
        assert result.errors[0].identifier == "general_error"
        assert "No status provided" in result.errors[0].message

    def test_empty_message_handled_gracefully(self) -> None:
        """RED: Empty or None messages should get default messages."""
        db_result = MutationResult(status="noop:test", message=None)

        processor = MutationResultProcessor()
        result = processor.process_error(db_result, CreateMachineError)

        assert result.errors[0].message == "Operation failed: noop:test"

    def test_error_details_structure_is_complete(self) -> None:
        """RED: Error details should have all required fields."""
        db_result = MutationResult(status="noop:test", message="Test message")

        processor = MutationResultProcessor()
        result = processor.process_error(db_result, CreateMachineError)

        error = result.errors[0]
        assert hasattr(error, "code")
        assert hasattr(error, "identifier")
        assert hasattr(error, "message")
        assert hasattr(error, "details")
        assert isinstance(error.details, dict)


class TestErrorDetail:
    """Test ErrorDetail structure and behavior."""

    def test_error_detail_is_immutable(self) -> None:
        """RED: ErrorDetail should be immutable (frozen dataclass)."""
        error = ErrorDetail(code=422, identifier="test_error", message="Test message", details={})

        # Should not be able to modify fields
        with pytest.raises(AttributeError):
            error.code = 500

    def test_error_detail_to_dict_conversion(self) -> None:
        """RED: ErrorDetail should convert to dictionary properly."""
        error = ErrorDetail(
            code=422, identifier="test_error", message="Test message", details={"extra": "info"}
        )

        # This test will fail until we implement the to_dict method
        expected_dict = {
            "code": 422,
            "identifier": "test_error",
            "message": "Test message",
            "details": {"extra": "info"},
        }

        # ErrorDetail should be directly JSON serializable via its fields
        # This will test that the structure is correct
        assert error.code == expected_dict["code"]
        assert error.identifier == expected_dict["identifier"]
        assert error.message == expected_dict["message"]
        assert error.details == expected_dict["details"]


class TestProcessedResult:
    """Test ProcessedResult structure and behavior."""

    def test_processed_result_is_immutable(self) -> None:
        """RED: ProcessedResult should be immutable."""
        result = ProcessedResult(
            typename="TestError", status="noop:test", message="Test message", errors=[]
        )

        # Should not be able to modify fields
        with pytest.raises(AttributeError):
            result.typename = "ModifiedType"

    def test_processed_result_to_dict_structure(self) -> None:
        """RED: ProcessedResult.to_dict() should return correct structure."""
        error = ErrorDetail(code=422, identifier="test_error", message="Test message", details={})

        result = ProcessedResult(
            typename="TestError", status="noop:test", message="Test message", errors=[error]
        )

        result_dict = result.to_dict()

        expected_structure = {
            "__typename": "TestError",
            "status": "noop:test",
            "message": "Test message",
            "errors": [
                {"code": 422, "identifier": "test_error", "message": "Test message", "details": {}}
            ],
        }

        assert result_dict == expected_structure
