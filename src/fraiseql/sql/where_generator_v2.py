"""Enhanced WHERE clause generator with integrated security validation.

This module provides an enhanced version of safe_create_where_type that
includes input validation before SQL generation.
"""

from typing import Any

from psycopg.sql import Composed

from fraiseql.security.validators import InputValidator, ValidationResult
from fraiseql.sql.where_generator import DynamicType, safe_create_where_type


def safe_create_where_type_with_validation(cls: type[object]) -> type[DynamicType]:
    """Create a WHERE input type with integrated validation.

    This enhanced version adds input validation before SQL generation,
    providing an additional layer of security against injection attempts.

    Args:
        cls: The FraiseQL type class to create WHERE conditions for

    Returns:
        A new type with WHERE operators and validation
    """
    # First create the base WHERE type
    base_type = safe_create_where_type(cls)

    # Get original to_sql method
    original_to_sql = base_type.to_sql

    # Create validated version
    def validated_to_sql(self) -> Composed | None:
        """Generate SQL WHERE clause with validation."""
        # Collect all conditions for validation
        conditions = {}

        # Extract conditions from instance attributes
        for attr_name in dir(self):
            if not attr_name.startswith("_"):
                attr_value = getattr(self, attr_name, None)
                if attr_value is not None:
                    conditions[attr_name] = attr_value

        # Validate all conditions
        validation_result = InputValidator.validate_where_clause(conditions)

        if not validation_result.is_valid:
            msg = f"Input validation failed: {'; '.join(validation_result.errors)}"
            raise ValueError(msg)

        # Log warnings if any (but don't block execution)
        if validation_result.warnings:
            import logging

            logger = logging.getLogger(__name__)
            logger.warning(
                "Suspicious patterns detected in WHERE clause: %s",
                validation_result.warnings,
            )

        # If validation passes, generate SQL using original method
        return original_to_sql(self)

    # Replace to_sql method with validated version
    base_type.to_sql = validated_to_sql

    # Add validation method for introspection
    def validate(self) -> ValidationResult:
        """Validate this WHERE clause without generating SQL."""
        conditions = {}
        for attr_name in dir(self):
            if not attr_name.startswith("_"):
                attr_value = getattr(self, attr_name, None)
                if attr_value is not None:
                    conditions[attr_name] = attr_value

        return InputValidator.validate_where_clause(conditions)

    base_type.validate = validate

    # Update class name and module
    base_type.__name__ = f"{cls.__name__}WhereValidated"
    base_type.__qualname__ = f"{cls.__name__}WhereValidated"

    return base_type  # type: ignore[return-value]


def create_secure_where_builder(cls: type[object]) -> "WhereBuilder":
    """Create a builder for constructing secure WHERE clauses.

    Provides a fluent interface for building WHERE conditions with
    automatic validation.

    Args:
        cls: The FraiseQL type class

    Returns:
        A WhereBuilder instance
    """
    return WhereBuilder(cls)


class WhereBuilder:
    """Fluent builder for secure WHERE clauses."""

    def __init__(self, cls: type[object]) -> None:
        self.cls = cls
        self.conditions = {}
        self._errors = []
        self._warnings = []

    def eq(self, field: str, value: Any) -> "WhereBuilder":
        """Add equality condition."""
        self._add_condition(field, "eq", value)
        return self

    def ne(self, field: str, value: Any) -> "WhereBuilder":
        """Add not-equal condition."""
        self._add_condition(field, "ne", value)
        return self

    def gt(self, field: str, value: Any) -> "WhereBuilder":
        """Add greater-than condition."""
        self._add_condition(field, "gt", value)
        return self

    def gte(self, field: str, value: Any) -> "WhereBuilder":
        """Add greater-than-or-equal condition."""
        self._add_condition(field, "gte", value)
        return self

    def lt(self, field: str, value: Any) -> "WhereBuilder":
        """Add less-than condition."""
        self._add_condition(field, "lt", value)
        return self

    def lte(self, field: str, value: Any) -> "WhereBuilder":
        """Add less-than-or-equal condition."""
        self._add_condition(field, "lte", value)
        return self

    def in_(self, field: str, values: list[Any]) -> "WhereBuilder":
        """Add IN condition."""
        self._add_condition(field, "in", values)
        return self

    def nin(self, field: str, values: list[Any]) -> "WhereBuilder":
        """Add NOT IN condition."""
        self._add_condition(field, "nin", values)
        return self

    def contains(self, field: str, value: str) -> "WhereBuilder":
        """Add CONTAINS condition."""
        self._add_condition(field, "contains", value)
        return self

    def starts_with(self, field: str, value: str) -> "WhereBuilder":
        """Add STARTS WITH condition."""
        self._add_condition(field, "starts_with", value)
        return self

    def ends_with(self, field: str, value: str) -> "WhereBuilder":
        """Add ENDS WITH condition."""
        self._add_condition(field, "ends_with", value)
        return self

    def is_null(self, field: str, value: bool = True) -> "WhereBuilder":
        """Add IS NULL condition."""
        self._add_condition(field, "is_null", value)
        return self

    def _add_condition(self, field: str, operator: str, value: Any) -> None:
        """Add a condition with validation."""
        # Validate immediately
        result = InputValidator.validate_field_value(field, value)

        if result.errors:
            self._errors.extend(result.errors)
        if result.warnings:
            self._warnings.extend(result.warnings)

        # Store condition
        if field not in self.conditions:
            self.conditions[field] = {}
        self.conditions[field][operator] = result.sanitized_value

    def build(self) -> Composed:
        """Build the WHERE clause SQL.

        Returns:
            Composed SQL for the WHERE clause

        Raises:
            ValueError: If validation errors exist
        """
        if self._errors:
            msg = f"Validation errors: {'; '.join(self._errors)}"
            raise ValueError(msg)

        if self._warnings:
            import logging

            logger = logging.getLogger(__name__)
            logger.warning("WHERE clause warnings: %s", self._warnings)

        # Create WHERE type instance
        where_type = safe_create_where_type_with_validation(self.cls)
        where_instance = where_type(**self.conditions)

        result = where_instance.to_sql()
        if result is None:
            msg = "No conditions to build"
            raise ValueError(msg)
        return result

    def validate(self) -> ValidationResult:
        """Validate current conditions without building SQL."""
        return InputValidator.validate_where_clause(self.conditions)


# Convenience function for migration
def migrate_to_validated_where(cls: type[object]) -> type[DynamicType]:
    """Migration helper to switch to validated WHERE types.

    This function helps migrate existing code to use validated WHERE types
    with minimal changes.

    Args:
        cls: The FraiseQL type class

    Returns:
        Validated WHERE type class
    """
    import warnings

    warnings.warn(
        "Consider using safe_create_where_type_with_validation directly for better error handling",
        DeprecationWarning,
        stacklevel=2,
    )
    return safe_create_where_type_with_validation(cls)
