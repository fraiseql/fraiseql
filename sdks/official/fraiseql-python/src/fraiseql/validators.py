"""Validation engine for custom GraphQL scalars."""

from typing import Any

from fraiseql.registry import SchemaRegistry
from fraiseql.scalars import CustomScalar


class ScalarValidationError(Exception):
    """Raised when custom scalar validation fails."""

    def __init__(self, scalar_name: str, context: str, message: str) -> None:
        self.scalar_name = scalar_name
        self.context = context
        super().__init__(
            f"Scalar {scalar_name!r} validation failed in {context}: {message}"
        )


def validate_custom_scalar(
    scalar_class: type[CustomScalar],
    value: Any,
    context: str = "parse_value",
) -> Any:
    """Execute validation for a custom scalar.

    Args:
        scalar_class: The CustomScalar subclass to validate with
        value: The value to validate
        context: One of "serialize", "parse_value", or "parse_literal"

    Returns:
        The validated/converted value

    Raises:
        ScalarValidationError: If validation fails
        ValueError: If context is unknown

    Examples:
        >>> from fraiseql.validators import validate_custom_scalar
        >>> from myapp.scalars import Email
        >>>
        >>> # Parse a variable value from GraphQL
        >>> email_value = validate_custom_scalar(Email, "user@example.com")
        >>> # Returns "user@example.com"
        >>>
        >>> # Validation fails
        >>> try:
        ...     email_value = validate_custom_scalar(Email, "invalid-email")
        ... except ScalarValidationError as e:
        ...     print(f"Validation error: {e}")
    """
    scalar_name = scalar_class.name

    try:
        scalar = scalar_class()

        if context == "serialize":
            return scalar.serialize(value)
        if context == "parse_value":
            return scalar.parse_value(value)
        if context == "parse_literal":
            return scalar.parse_literal(value)

        raise ValueError(f"Unknown validation context: {context!r}")

    except ScalarValidationError:
        raise
    except ValueError as e:
        raise ScalarValidationError(scalar_name, context, str(e)) from e
    except Exception as e:
        raise ScalarValidationError(
            scalar_name, context, f"{type(e).__name__}: {str(e)}"
        ) from e


def get_all_custom_scalars() -> dict[str, type[CustomScalar]]:
    """Get all registered custom scalars.

    Returns:
        Dictionary mapping scalar names to CustomScalar classes

    Examples:
        >>> from fraiseql.validators import get_all_custom_scalars
        >>> scalars = get_all_custom_scalars()
        >>> # {'Email': <class Email>, 'Phone': <class Phone>, ...}
    """
    return SchemaRegistry.get_custom_scalars()
