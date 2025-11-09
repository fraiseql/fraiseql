import pytest

"""Tests for date range validation utilities."""

from datetime import date

from fraiseql import UNSET
from fraiseql.types import (
    DateRangeValidatable,
    DateRangeValidationMixin,
    date_range_validator,
    fraise_input,
    get_date_range_validation_errors,
    validate_date_range,
)


@pytest.mark.unit
class TestValidateDateRange:
    """Test the validate_date_range function."""

    def test_valid_date_range(self) -> None:
        """Test validation with valid date range."""

        @fraise_input
        class TestInput:
            start_date: date
            end_date: date

        obj = TestInput(start_date=date(2025, 1, 1), end_date=date(2025, 12, 31))

        is_valid, error_msg = validate_date_range(obj)
        assert is_valid is True
        assert error_msg is None

    def test_invalid_date_range(self) -> None:
        """Test validation with end date before start date."""

        @fraise_input
        class TestInput:
            start_date: date
            end_date: date

        obj = TestInput(start_date=date(2025, 12, 31), end_date=date(2025, 1, 1))

        is_valid, error_msg = validate_date_range(obj)
        assert is_valid is False
        assert error_msg == "End date (2025-01-01) cannot be before start date (2025-12-31)"

    def test_same_dates(self) -> None:
        """Test validation with same start and end dates."""

        @fraise_input
        class TestInput:
            start_date: date
            end_date: date

        obj = TestInput(start_date=date(2025, 6, 15), end_date=date(2025, 6, 15))

        is_valid, error_msg = validate_date_range(obj)
        assert is_valid is True
        assert error_msg is None

    def test_unset_values(self) -> None:
        """Test validation with UNSET values."""

        @fraise_input
        class TestInput:
            start_date: date = UNSET
            end_date: date = UNSET

        # Both UNSET
        obj = TestInput()
        is_valid, error_msg = validate_date_range(obj)
        assert is_valid is True
        assert error_msg is None

        # Start date UNSET
        obj = TestInput(end_date=date(2025, 1, 1))
        is_valid, error_msg = validate_date_range(obj)
        assert is_valid is True
        assert error_msg is None

        # End date UNSET
        obj = TestInput(start_date=date(2025, 1, 1))
        is_valid, error_msg = validate_date_range(obj)
        assert is_valid is True
        assert error_msg is None

    def test_none_values(self) -> None:
        """Test validation with None values."""

        @fraise_input
        class TestInput:
            start_date: date | None
            end_date: date | None

        # Both None
        obj = TestInput(start_date=None, end_date=None)
        is_valid, error_msg = validate_date_range(obj)
        assert is_valid is True
        assert error_msg is None

        # Start date None
        obj = TestInput(start_date=None, end_date=date(2025, 1, 1))
        is_valid, error_msg = validate_date_range(obj)
        assert is_valid is True
        assert error_msg is None

        # End date None
        obj = TestInput(start_date=date(2025, 1, 1), end_date=None)
        is_valid, error_msg = validate_date_range(obj)
        assert is_valid is True
        assert error_msg is None


class TestGetDateRangeValidationErrors:
    """Test the get_date_range_validation_errors function."""

    def test_valid_range_no_errors(self) -> None:
        """Test that valid range returns empty error list."""

        @fraise_input
        class TestInput:
            start_date: date
            end_date: date

        obj = TestInput(start_date=date(2025, 1, 1), end_date=date(2025, 12, 31))

        errors = get_date_range_validation_errors(obj)
        assert errors == []

    def test_invalid_range_returns_errors(self) -> None:
        """Test that invalid range returns proper error format."""

        @fraise_input
        class TestInput:
            start_date: date
            end_date: date

        obj = TestInput(start_date=date(2025, 12, 31), end_date=date(2025, 1, 1))

        errors = get_date_range_validation_errors(obj)
        assert len(errors) == 1
        assert errors[0] == {
            "message": "End date (2025-01-01) cannot be before start date (2025-12-31)",
            "code": 422,
            "identifier": "validation_error",
            "fields": ["start_date", "end_date"],
        }


class TestDateRangeValidationMixin:
    """Test the DateRangeValidationMixin class."""

    def test_mixin_validate_dates(self) -> None:
        """Test mixin's validate_dates method."""

        @fraise_input
        class TestInput(DateRangeValidationMixin):
            start_date: date
            end_date: date

        # Valid range
        obj = TestInput(start_date=date(2025, 1, 1), end_date=date(2025, 12, 31))
        is_valid, error_msg = obj.validate_dates()
        assert is_valid is True
        assert error_msg is None

        # Invalid range
        obj = TestInput(start_date=date(2025, 12, 31), end_date=date(2025, 1, 1))
        is_valid, error_msg = obj.validate_dates()
        assert is_valid is False
        assert "cannot be before" in error_msg

    def test_mixin_get_validation_errors(self) -> None:
        """Test mixin's get_validation_errors method."""

        @fraise_input
        class TestInput(DateRangeValidationMixin):
            start_date: date
            end_date: date

        # Valid range
        obj = TestInput(start_date=date(2025, 1, 1), end_date=date(2025, 12, 31))
        errors = obj.get_validation_errors()
        assert errors == []

        # Invalid range
        obj = TestInput(start_date=date(2025, 12, 31), end_date=date(2025, 1, 1))
        errors = obj.get_validation_errors()
        assert len(errors) == 1
        assert errors[0]["code"] == 422

    def test_mixin_with_optional_fields(self) -> None:
        """Test mixin with optional date fields."""

        @fraise_input
        class TestInput(DateRangeValidationMixin):
            start_date: date | None = UNSET
            end_date: date | None = UNSET
            name: str

        # All UNSET
        obj = TestInput(name="Test")
        is_valid, error_msg = obj.validate_dates()
        assert is_valid is True
        assert error_msg is None

        # Mixed values
        obj = TestInput(name="Test", start_date=date(2025, 1, 1), end_date=None)
        is_valid, error_msg = obj.validate_dates()
        assert is_valid is True
        assert error_msg is None


class TestDateRangeValidatorDecorator:
    """Test the date_range_validator decorator."""

    def test_decorator_adds_methods(self) -> None:
        """Test that decorator adds validation methods."""

        @date_range_validator
        @fraise_input
        class TestInput:
            start_date: date
            end_date: date

        # Check methods exist
        assert hasattr(TestInput, "validate_dates")
        assert hasattr(TestInput, "get_validation_errors")

        # Test valid range
        obj = TestInput(start_date=date(2025, 1, 1), end_date=date(2025, 12, 31))
        is_valid, error_msg = obj.validate_dates()
        assert is_valid is True
        assert error_msg is None

        errors = obj.get_validation_errors()
        assert errors == []

    def test_decorator_with_invalid_range(self) -> None:
        """Test decorator with invalid date range."""

        @date_range_validator
        @fraise_input
        class TestInput:
            start_date: date
            end_date: date

        obj = TestInput(start_date=date(2025, 12, 31), end_date=date(2025, 1, 1))

        is_valid, error_msg = obj.validate_dates()
        assert is_valid is False
        assert "cannot be before" in error_msg

        errors = obj.get_validation_errors()
        assert len(errors) == 1
        assert errors[0]["identifier"] == "validation_error"

    def test_decorator_with_existing_methods(self) -> None:
        """Test that decorator doesn't break existing class functionality."""

        @date_range_validator
        @fraise_input
        class TestInput:
            start_date: date
            end_date: date
            description: str

            def custom_method(self) -> str:
                return f"{self.description}: {self.start_date} to {self.end_date}"

        obj = TestInput(
            start_date=date(2025, 1, 1), end_date=date(2025, 12, 31), description="Test Period"
        )

        # Original method still works
        assert obj.custom_method() == "Test Period: 2025-01-01 to 2025-12-31"

        # Validation methods added
        is_valid, _ = obj.validate_dates()
        assert is_valid is True


class TestDateRangeValidatableProtocol:
    """Test the DateRangeValidatable protocol."""

    def test_protocol_with_compliant_class(self) -> None:
        """Test that classes with start_date and end_date match protocol."""

        @fraise_input
        class CompliantInput:
            start_date: date
            end_date: date
            other_field: str

        obj = CompliantInput(
            start_date=date(2025, 1, 1), end_date=date(2025, 12, 31), other_field="test"
        )

        # Should be instance of protocol
        assert isinstance(obj, DateRangeValidatable)

        # Can use with validation functions
        is_valid, _ = validate_date_range(obj)
        assert is_valid is True

    def test_protocol_with_optional_dates(self) -> None:
        """Test protocol with optional date fields."""

        @fraise_input
        class OptionalDatesInput:
            start_date: date | None
            end_date: date | None

        obj = OptionalDatesInput(start_date=None, end_date=None)

        assert isinstance(obj, DateRangeValidatable)
        is_valid, _ = validate_date_range(obj)
        assert is_valid is True


class TestIntegrationScenarios:
    """Test real-world integration scenarios."""

    def test_mutation_integration_pattern(self) -> None:
        """Test the pattern used in mutation resolvers."""

        @fraise_input
        class CreateContractInput(DateRangeValidationMixin):
            customer_id: str
            start_date: date
            end_date: date | None = UNSET
            value: float

        # Simulate mutation resolver logic
        def process_input(input_data: CreateContractInput) -> dict[str, any]:
            # Validate dates first
            is_valid, error_msg = input_data.validate_dates()
            if not is_valid:
                return {
                    "status": "failed:validation",
                    "message": f"Invalid date range: {error_msg}",
                    "errors": input_data.get_validation_errors(),
                }

            # Process valid input
            return {
                "status": "success",
                "message": "Contract created",
                "data": {
                    "customer_id": input_data.customer_id,
                    "start_date": str(input_data.start_date),
                    "end_date": (
                        str(input_data.end_date)
                        if input_data.end_date and input_data.end_date is not UNSET
                        else None
                    ),
                },
            }

        # Test valid input
        valid_input = CreateContractInput(
            customer_id="CUST123",
            start_date=date(2025, 1, 1),
            end_date=date(2025, 12, 31),
            value=1000.0,
        )
        result = process_input(valid_input)
        assert result["status"] == "success"

        # Test invalid input
        invalid_input = CreateContractInput(
            customer_id="CUST123",
            start_date=date(2025, 12, 31),
            end_date=date(2025, 1, 1),
            value=1000.0,
        )
        result = process_input(invalid_input)
        assert result["status"] == "failed:validation"
        assert "Invalid date range" in result["message"]
        assert len(result["errors"]) == 1

    def test_update_mutation_pattern(self) -> None:
        """Test pattern for update mutations with partial data."""

        @date_range_validator
        @fraise_input
        class UpdateContractInput:
            id: str
            start_date: date | None = UNSET
            end_date: date | None = UNSET
            value: float | None = UNSET

        # Only updating end date - should skip validation
        input1 = UpdateContractInput(id="CONTRACT123", end_date=date(2025, 12, 31))
        is_valid, _ = input1.validate_dates()
        assert is_valid is True

        # Updating both dates with valid range
        input2 = UpdateContractInput(
            id="CONTRACT123", start_date=date(2025, 1, 1), end_date=date(2025, 12, 31)
        )
        is_valid, _ = input2.validate_dates()
        assert is_valid is True

        # Updating both dates with invalid range
        input3 = UpdateContractInput(
            id="CONTRACT123", start_date=date(2025, 12, 31), end_date=date(2025, 1, 1)
        )
        is_valid, error_msg = input3.validate_dates()
        assert is_valid is False
        assert error_msg is not None
