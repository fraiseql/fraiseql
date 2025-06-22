"""Consolidated WHERE clause generator with integrated validation.

This module combines the functionality of where_generator.py and where_generator_v2.py
into a single, configurable implementation with better error handling.
"""

import json
from collections.abc import Callable
from dataclasses import dataclass
from dataclasses import field as dataclass_field
from datetime import date, datetime
from decimal import Decimal
from enum import Enum
from typing import Any, Optional, Union, get_args, get_origin, get_type_hints

from psycopg.sql import SQL, Composed, Literal, Placeholder

from fraiseql.errors.user_friendly import SQLGenerationError
from fraiseql.security.validators import InputValidator


class ValidationMode(Enum):
    """Validation mode for WHERE clause generation."""

    STRICT = "strict"  # Reject suspicious input
    LENIENT = "lenient"  # Sanitize suspicious input
    DISABLED = "disabled"  # No validation (use with caution)


@dataclass
class WhereGeneratorConfig:
    """Configuration for WHERE clause generation."""

    validation_mode: ValidationMode = ValidationMode.STRICT
    max_string_length: int = 1000
    max_list_length: int = 100
    max_numeric_value: float = 1e15
    field_mapping: dict[str, str] = dataclass_field(default_factory=dict)
    excluded_fields: set[str] = dataclass_field(default_factory=set)
    custom_operators: dict[str, Callable] = dataclass_field(default_factory=dict)
    enable_nested_access: bool = False
    table_prefix: str = "data"


class WhereClauseMixin:
    """Mixin that provides WHERE clause generation functionality."""

    _config: WhereGeneratorConfig
    _conditions: dict[str, Any]

    def __init__(self, **kwargs) -> None:
        """Initialize with field values."""
        self._config = getattr(self.__class__, "_config", WhereGeneratorConfig())
        self._conditions = {}

        # Process kwargs into conditions
        for key, value in kwargs.items():
            if value is not None:
                self._conditions[key] = value

    def _validate_value(self, field_name: str, value: Any) -> Any:
        """Validate a field value based on configuration."""
        if self._config.validation_mode == ValidationMode.DISABLED:
            return value

        # String validation
        if isinstance(value, str):
            if len(value) > self._config.max_string_length:
                raise SQLGenerationError(
                    operation="WHERE clause generation",
                    reason=(
                        f"Field '{field_name}' exceeds maximum length "
                        f"({self._config.max_string_length})"
                    ),
                    custom_suggestion=f"Reduce the length of the {field_name} filter",
                )

            if self._config.validation_mode == ValidationMode.STRICT:
                # Check for SQL injection patterns
                validation_result = InputValidator.validate_field_value(field_name, value)
                if not validation_result.is_valid or validation_result.warnings:
                    raise SQLGenerationError(
                        operation="WHERE clause generation",
                        reason="SQL injection pattern detected",
                        query_info={"field": field_name, "value": value},
                        custom_suggestion=(
                            "Remove SQL keywords and special characters from the filter"
                        ),
                    )

        # Numeric validation
        elif isinstance(value, int | float | Decimal):
            import math

            if isinstance(value, float) and (math.isinf(value) or math.isnan(value)):
                raise SQLGenerationError(
                    operation="WHERE clause generation",
                    reason=f"Field '{field_name}' contains invalid numeric value",
                    custom_suggestion="Use a valid finite number",
                )

            if abs(float(value)) > self._config.max_numeric_value:
                raise SQLGenerationError(
                    operation="WHERE clause generation",
                    reason=f"Field '{field_name}' exceeds maximum numeric value",
                    custom_suggestion=(f"Use a value less than {self._config.max_numeric_value}"),
                )

        # List validation
        elif isinstance(value, list):
            if len(value) > self._config.max_list_length:
                raise SQLGenerationError(
                    operation="WHERE clause generation",
                    reason=(
                        f"Field '{field_name}' list exceeds maximum length "
                        f"({self._config.max_list_length})"
                    ),
                    custom_suggestion="Reduce the number of items in the filter list",
                )

            # Validate each item in the list
            return [
                self._validate_value(f"{field_name}[{i}]", item) for i, item in enumerate(value)
            ]

        return value

    def _get_field_path(self, field_name: str) -> SQL | Composed:
        """Get the SQL path for a field."""
        # Apply field mapping
        mapped_name = self._config.field_mapping.get(field_name, field_name)

        # Handle nested access
        if self._config.enable_nested_access and "__" in mapped_name:
            parts = mapped_name.split("__")
            # Build nested JSON access: data->'part1'->'part2'->>'final'
            path = SQL(self._config.table_prefix)
            for i, part in enumerate(parts):
                if i == len(parts) - 1:
                    # Last part uses ->>
                    return SQL("{}->>{}").format(path, Literal(part))
                # Intermediate parts use ->
                path = SQL("{}->{}").format(path, Literal(part))
            return path

        # Check if field_name contains __ (for nested field that's not in mapping)
        if self._config.enable_nested_access and "__" in field_name:
            parts = field_name.split("__")
            path = SQL(self._config.table_prefix)
            for i, part in enumerate(parts):
                if i == len(parts) - 1:
                    return SQL("{}->>{}").format(path, Literal(part))
                path = SQL("{}->{}").format(path, Literal(part))
            return path

        return SQL("{}->>{}").format(SQL(self._config.table_prefix), Literal(mapped_name))

    def _get_typed_field_path(self, field_name: str, field_type: type) -> SQL | Composed:
        """Get the SQL path with appropriate type casting."""
        base_path = self._get_field_path(field_name)

        # Determine casting based on type
        if field_type is int or field_type == Optional[int]:
            return SQL("({})::int").format(base_path)
        if field_type is float or field_type == Optional[float]:
            return SQL("({})::float").format(base_path)
        if field_type is Decimal or field_type == Optional[Decimal]:
            return SQL("({})::numeric").format(base_path)
        if field_type is bool or field_type == Optional[bool]:
            return SQL("({})::boolean").format(base_path)
        if field_type is datetime or field_type == Optional[datetime]:
            return SQL("({})::timestamp").format(base_path)
        if field_type is date or field_type == Optional[date]:
            return SQL("({})::date").format(base_path)
        return base_path

    def _build_condition(
        self,
        field_name: str,
        operator: str,
        value: Any,
        field_type: type,
    ) -> Composed:
        """Build a single WHERE condition."""
        # Validate the value
        validated_value = self._validate_value(field_name, value)

        # Get the field path with casting
        if (
            operator in ("contains", "starts_with", "ends_with")
            or field_type is str
            or field_type == Optional[str]
        ):
            field_path = self._get_field_path(field_name)
        else:
            field_path = self._get_typed_field_path(field_name, field_type)

        # Handle special operators
        if operator == "is_null":
            if validated_value:
                return SQL("{} IS NULL").format(field_path)
            return SQL("{} IS NOT NULL").format(field_path)

        if operator in ("contains", "starts_with", "ends_with"):
            return SQL("{} ILIKE {}").format(field_path, Placeholder())

        if operator == "in":
            return SQL("{} = ANY({})").format(field_path, Placeholder())

        if operator == "not_in":
            return SQL("NOT ({} = ANY({}))").format(field_path, Placeholder())

        if operator == "array_contains":
            # For JSONB array contains
            json_path = SQL("{}->{}").format(
                SQL(self._config.table_prefix),
                Literal(field_name),
            )
            return SQL("{} @> {}").format(json_path, Placeholder())

        # Standard operators
        operator_map = {
            "eq": "=",
            "not": "!=",
            "lt": "<",
            "lte": "<=",
            "gt": ">",
            "gte": ">=",
        }

        sql_op = operator_map.get(operator, "=")
        return SQL("{} {} {}").format(field_path, SQL(sql_op), Placeholder())

    def _get_params(self) -> list[Any]:
        """Get parameter values in the order they appear in the SQL."""
        params = []

        for key, value in sorted(self._conditions.items()):
            if key.startswith("_"):
                continue  # Skip special keys like _or, _and

            # Parse field name and operator
            parts = key.split("_")
            field_name = parts[0]
            operator = "_".join(parts[1:]) if len(parts) > 1 else "eq"

            # Skip if field is excluded
            if field_name in self._config.excluded_fields:
                continue

            # Skip null checks
            if operator == "is_null":
                continue

            # Get field type from original class for proper handling
            original_type_hints = get_type_hints(self.__class__._original_class)
            field_type = original_type_hints.get(field_name, str)

            # Check if contains should be array_contains based on field type
            actual_operator = operator
            if operator == "contains":
                origin = get_origin(field_type)
                if origin is list or (
                    origin in (Union, type(int | None))
                    and any(
                        get_origin(arg) is list
                        for arg in get_args(field_type)
                        if arg is not type(None)
                    )
                ):
                    actual_operator = "array_contains"

            # Transform value for special operators
            if actual_operator == "contains":
                params.append(f"%{value}%")
            elif operator == "starts_with":
                params.append(f"{value}%")
            elif operator == "ends_with":
                params.append(f"%{value}")
            elif actual_operator == "array_contains":
                # Convert to JSON array
                params.append(json.dumps([value]))
            else:
                params.append(value)

        return params

    def to_sql(self) -> Composed | None:
        """Generate the WHERE clause SQL."""
        if not self._conditions:
            return None

        conditions = []

        # Handle special keys first
        if "_or" in self._conditions:
            or_conditions = []
            for or_dict in self._conditions["_or"]:
                sub_where = self.__class__(**or_dict)
                sub_sql = sub_where.to_sql()
                if sub_sql:
                    or_conditions.append(sub_sql)

            if or_conditions:
                conditions.append(SQL("({})").format(SQL(" OR ").join(or_conditions)))

        if "_and" in self._conditions:
            and_conditions = []
            for and_dict in self._conditions["_and"]:
                sub_where = self.__class__(**and_dict)
                sub_sql = sub_where.to_sql()
                if sub_sql:
                    and_conditions.append(sub_sql)

            if and_conditions:
                conditions.append(SQL("({})").format(SQL(" AND ").join(and_conditions)))

        # Get type hints from the ORIGINAL class, not the WHERE class
        original_type_hints = get_type_hints(self.__class__._original_class)

        # Process regular fields
        for key, value in sorted(self._conditions.items()):
            if key.startswith("_"):
                continue  # Skip special keys

            # Check for nested field access first
            if self._config.enable_nested_access and "__" in key:
                # For nested access, check if the base field exists
                base_field = key.split("__")[0]
                if base_field in original_type_hints:
                    field_name = key  # Use the full key as field name
                    operator = "eq"
                else:
                    # Not a valid nested field, continue with normal parsing
                    field_name = None
                    operator = None
            else:
                field_name = None
                operator = None

            # If not a nested field, parse normally
            if field_name is None:
                # Parse field name and operator
                parts = key.split("_")

                # Find the actual field name (handling operators like _not_in)
                field_name = None
                operator = "eq"

                # Try to match field names from longest to shortest
                for i in range(len(parts), 0, -1):
                    potential_field = "_".join(parts[:i])
                    if potential_field in original_type_hints:
                        field_name = potential_field
                        operator = "_".join(parts[i:]) if i < len(parts) else "eq"
                        break

                if not field_name:
                    # Default to using first part as field name
                    field_name = parts[0]
                    operator = "_".join(parts[1:]) if len(parts) > 1 else "eq"

            # Skip if field is excluded
            if field_name in self._config.excluded_fields:
                continue

            # Get field type from original class
            if "__" in field_name and self._config.enable_nested_access:
                # For nested fields, get the base field type
                base_field = field_name.split("__")[0]
                field_type = original_type_hints.get(base_field, str)
            else:
                field_type = original_type_hints.get(field_name, str)

            # Handle array contains specially for list fields
            if operator == "contains":
                origin = get_origin(field_type)
                # Handle both list[str] and Optional[list[str]]
                if origin is list:
                    operator = "array_contains"
                elif origin in (Union, type(int | None)):  # Handle Union and UnionType
                    # Check if it's Optional[list[...]]
                    args = get_args(field_type)
                    for arg in args:
                        if arg is not type(None) and get_origin(arg) is list:
                            operator = "array_contains"
                            break
                elif field_type is list:  # Handle bare list type
                    operator = "array_contains"

            # Build condition
            condition = self._build_condition(field_name, operator, value, field_type)
            conditions.append(condition)

        if not conditions:
            return None

        return SQL(" AND ").join(conditions)


def create_where_type(cls: type, config: WhereGeneratorConfig | None = None) -> type:
    """Create a WHERE input type for the given class.

    Args:
        cls: The dataclass to create WHERE filters for
        config: Optional configuration for WHERE generation

    Returns:
        A new class with WHERE filtering capabilities
    """
    if config is None:
        config = WhereGeneratorConfig()

    # Get type hints from the original class
    type_hints = get_type_hints(cls)

    # Build attributes for the WHERE class
    attributes = {
        "_config": config,
        "_original_class": cls,
        "__module__": cls.__module__,
    }

    # Add field attributes with operators
    for field_name, field_type in type_hints.items():
        if field_name in config.excluded_fields:
            continue

        # Basic equality
        attributes[field_name] = None

        # Type-specific operators
        origin = get_origin(field_type)
        base_type = field_type
        if origin is Union:
            # Handle Optional types
            args = get_args(field_type)
            if len(args) == 2 and type(None) in args:
                base_type = args[0] if args[1] is type(None) else args[1]

        # Add operators based on type
        if base_type in {int, float, Decimal, datetime, date}:
            attributes[f"{field_name}_lt"] = None
            attributes[f"{field_name}_lte"] = None
            attributes[f"{field_name}_gt"] = None
            attributes[f"{field_name}_gte"] = None

        if base_type is str:
            attributes[f"{field_name}_contains"] = None
            attributes[f"{field_name}_starts_with"] = None
            attributes[f"{field_name}_ends_with"] = None

        # For list fields, add contains operator for array containment
        if origin is list:
            attributes[f"{field_name}_contains"] = None

        # Common operators for all types
        attributes[f"{field_name}_not"] = None
        attributes[f"{field_name}_in"] = None
        attributes[f"{field_name}_not_in"] = None
        attributes[f"{field_name}_is_null"] = None

    # Add special operators
    attributes["_or"] = None
    attributes["_and"] = None

    # Add a custom __setattr__ to handle dynamic attributes
    def custom_setattr(self, name: str, value: Any) -> None:
        self.__dict__[name] = value

    attributes["__setattr__"] = custom_setattr

    # Create the WHERE class
    return type(
        f"{cls.__name__}Where",
        (WhereClauseMixin,),
        attributes,
    )
