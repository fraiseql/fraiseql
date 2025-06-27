"""Partial object instantiation for GraphQL types.

This module provides utilities for creating partial instances of FraiseQL types
when only a subset of fields is requested in a GraphQL query.
"""

import dataclasses
from typing import Any, get_type_hints

from .errors.exceptions import PartialInstantiationError


def create_partial_instance(type_class: type, data: dict[str, Any]) -> Any:
    """Create a partial instance of a type with only the provided fields.

    This function creates an instance even when required fields are missing,
    by using a special partial instantiation mode.

    Args:
        type_class: The FraiseQL type class to instantiate
        data: Dictionary of field values (may be incomplete)

    Returns:
        Instance of type_class with only the provided fields set

    Raises:
        PartialInstantiationError: If instantiation fails completely
    """
    try:
        # Check if this is a dataclass
        if dataclasses.is_dataclass(type_class):
            return _create_partial_dataclass(type_class, data)

        # For regular classes, try to instantiate with available data
        try:
            return type_class(**data)
        except TypeError:
            # If instantiation fails due to missing required fields,
            # create a minimal instance
            return _create_minimal_instance(type_class, data)
    except Exception as e:
        # Get all available and expected fields for better error message
        available_fields = set(data.keys()) if data else set()

        # Try to get expected fields
        expected_fields = set()
        if dataclasses.is_dataclass(type_class):
            expected_fields = {f.name for f in dataclasses.fields(type_class)}
        else:
            try:
                hints = get_type_hints(type_class)
                expected_fields = set(hints.keys())
            except Exception:
                pass

        raise PartialInstantiationError(
            type_name=type_class.__name__,
            reason=str(e),
            available_fields=available_fields,
            requested_fields=expected_fields,
            cause=e,
        ) from e


def _create_partial_dataclass(type_class: type, data: dict[str, Any]) -> Any:
    """Create a partial dataclass instance."""
    # Get all fields from the dataclass
    fields = dataclasses.fields(type_class)

    # Create a new dict with defaults for missing required fields
    partial_data = {}
    failed_field = None

    try:
        for field in fields:
            field_name = field.name

            if field_name in data:
                # Use provided value
                partial_data[field_name] = data[field_name]
            elif field.default is not dataclasses.MISSING:
                # Use default value
                partial_data[field_name] = field.default
            elif field.default_factory is not dataclasses.MISSING:
                # Use default factory
                partial_data[field_name] = field.default_factory()
            else:
                # For required fields without defaults, use None
                # This is the key difference - we allow None for missing required fields
                partial_data[field_name] = None

        # Create instance with partial data
        try:
            instance = type_class(**partial_data)
        except Exception:
            # If instantiation still fails (e.g., due to __post_init__ validation),
            # create an instance without calling __init__
            instance = object.__new__(type_class)

            # Set all fields directly
            for field_name, value in partial_data.items():
                try:
                    setattr(instance, field_name, value)
                except Exception as attr_error:
                    failed_field = field_name
                    raise PartialInstantiationError(
                        type_name=type_class.__name__,
                        field_name=field_name,
                        reason=f"Failed to set attribute: {attr_error!s}",
                        available_fields=set(data.keys()),
                        cause=attr_error,
                    ) from attr_error

        # Mark this as a partial instance
        instance.__fraiseql_partial__ = True
        instance.__fraiseql_fields__ = set(data.keys())

    except Exception as e:
        if isinstance(e, PartialInstantiationError):
            raise

        raise PartialInstantiationError(
            type_name=type_class.__name__,
            field_name=failed_field,
            reason=f"Failed to create partial dataclass: {e!s}",
            available_fields=set(data.keys()),
            requested_fields={f.name for f in fields},
            cause=e,
        ) from e
    else:
        return instance


def _create_minimal_instance(type_class: type, data: dict[str, Any]) -> Any:
    """Create a minimal instance for non-dataclass types."""
    try:
        # Get type hints to understand the expected fields
        type_hints = get_type_hints(type_class)
    except Exception:
        # If we can't get type hints, use the data keys
        type_hints = dict.fromkeys(data.keys(), Any)

    # Build kwargs with None for missing required fields
    kwargs = {}
    for field_name in type_hints:
        kwargs[field_name] = data.get(field_name)

    try:
        instance = type_class(**kwargs)
    except Exception:
        # If we still can't instantiate, create an empty instance
        # and set attributes directly
        try:
            instance = object.__new__(type_class)
            for key, value in data.items():
                setattr(instance, key, value)
        except Exception as attr_error:
            raise PartialInstantiationError(
                type_name=type_class.__name__,
                field_name=key if "key" in locals() else None,
                reason=f"Failed to create minimal instance: {attr_error!s}",
                available_fields=set(data.keys()),
                requested_fields=set(type_hints.keys()),
                cause=attr_error,
            ) from attr_error

    # Mark as partial
    instance.__fraiseql_partial__ = True
    instance.__fraiseql_fields__ = set(data.keys())

    return instance


def is_partial_instance(obj: Any) -> bool:
    """Check if an object is a partial instance."""
    return getattr(obj, "__fraiseql_partial__", False)


def get_available_fields(obj: Any) -> set[str]:
    """Get the set of fields that were actually provided for a partial instance."""
    return getattr(obj, "__fraiseql_fields__", set())
