"""Date range validation utilities for FraiseQL input types.

This module provides utilities for validating date ranges in input types where
start_date and end_date are separate fields. This complements the DateRange scalar
which handles PostgreSQL date range strings.
"""

from datetime import date
from typing import Any, Protocol, runtime_checkable

from fraiseql.types.definitions import UNSET


@runtime_checkable
class DateRangeValidatable(Protocol):
    """Protocol for objects that have start_date and end_date fields."""

    start_date: date | None
    end_date: date | None


def validate_date_range(obj: DateRangeValidatable) -> tuple[bool, str | None]:
    """Validate that end_date is not before start_date.

    Args:
        obj: Object with start_date and end_date attributes

    Returns:
        Tuple of (is_valid, error_message)
        - is_valid: True if valid, False otherwise
        - error_message: Human-readable error message if invalid, None if valid
    """
    # Handle UNSET values - skip validation if either date is UNSET
    if obj.start_date is UNSET or obj.end_date is UNSET:
        return True, None

    # Skip validation if either date is None
    if obj.start_date is None or obj.end_date is None:
        return True, None

    # Validate that end_date is not before start_date
    if obj.end_date < obj.start_date:
        return False, f"End date ({obj.end_date}) cannot be before start date ({obj.start_date})"

    return True, None


def get_date_range_validation_errors(obj: DateRangeValidatable) -> list[dict[str, Any]]:
    """Get validation errors in FraiseQL error format.

    Args:
        obj: Object with start_date and end_date attributes

    Returns:
        List of error dictionaries in FraiseQL format, empty if valid
    """
    is_valid, error_msg = validate_date_range(obj)
    if not is_valid:
        return [
            {
                "message": error_msg,
                "code": 422,
                "identifier": "validation_error",
                "fields": ["start_date", "end_date"],
            }
        ]
    return []


class DateRangeValidationMixin:
    """Mixin that provides date range validation methods for FraiseQL input types.

    Usage:
        @fraiseql.input
        class MyInput(DateRangeValidationMixin):
            start_date: date
            end_date: date | None = UNSET
    """

    def validate_dates(self) -> tuple[bool, str | None]:
        """Validate the date range in this input object.

        Returns:
            Tuple of (is_valid, error_message)
        """
        return validate_date_range(self)

    def get_validation_errors(self) -> list[dict[str, Any]]:
        """Get validation errors in FraiseQL format.

        Returns:
            List of error dictionaries, empty if valid
        """
        return get_date_range_validation_errors(self)


def date_range_validator(cls: type) -> type:
    """Decorator that adds date range validation to a FraiseQL input class.

    This decorator adds validate_dates() and get_validation_errors() methods
    to the decorated class.

    Usage:
        @date_range_validator
        @fraiseql.input
        class MyInput:
            start_date: date
            end_date: date | None = UNSET

    Args:
        cls: The class to decorate

    Returns:
        The decorated class with validation methods added
    """
    # Store original __init__ if it exists
    original_init = getattr(cls, "__init__", None)  # noqa: F841

    # Add validation methods
    def validate_dates(self: Any) -> tuple[bool, str | None]:
        """Validate the date range in this input object."""
        return validate_date_range(self)

    def get_validation_errors(self: Any) -> list[dict[str, Any]]:
        """Get validation errors in FraiseQL format."""
        return get_date_range_validation_errors(self)

    # Set methods on the class
    cls.validate_dates = validate_dates
    cls.get_validation_errors = get_validation_errors

    return cls


__all__ = [
    "DateRangeValidatable",
    "DateRangeValidationMixin",
    "date_range_validator",
    "get_date_range_validation_errors",
    "validate_date_range",
]
