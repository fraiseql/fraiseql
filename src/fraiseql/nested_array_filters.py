"""Registry system for nested array filtering with comprehensive logical operators in FraiseQL.

This module provides a clean, registration-based approach for enabling where filtering
on nested array fields with complete AND/OR/NOT logical operator support, without verbose
field definitions.

Features:
- Complete logical operator support (AND/OR/NOT with unlimited nesting)
- All standard field operators (equals, contains, gte, isnull, etc.)
- Clean registration-based API (no verbose field definitions)
- Convention over configuration with automatic detection
- Full type safety with generated WhereInput types

Instead of:
    print_servers: list[PrintServer] = fraise_field(
        default_factory=list,
        supports_where_filtering=True,
        nested_where_type=PrintServer
    )

You can simply:
    print_servers: list[PrintServer] = fraise_field(default_factory=list)

    # Then register the filter separately
    register_nested_array_filter(NetworkConfiguration, 'print_servers', PrintServer)

Or even better, use automatic detection:
    @auto_nested_array_filters  # Enables all list[SomeType] fields automatically
    @fraise_type
    class NetworkConfiguration:
        print_servers: list[PrintServer] = fraise_field(default_factory=list)

This enables sophisticated GraphQL queries like:
    printServers(where: {
      AND: [
        { operatingSystem: { in: ["Linux", "Windows"] } }
        { OR: [
            { nTotalAllocations: { gte: 100 } }
            { hostname: { contains: "critical" } }
          ]
        }
        { NOT: { ipAddress: { isnull: true } } }
      ]
    })
"""

import logging
from typing import Any, Callable, Type, get_args, get_origin

logger = logging.getLogger(__name__)

# Global registry for nested array filters
_nested_array_filters: dict[str, dict[str, Type]] = {}


def register_nested_array_filter(parent_type: Type, field_name: str, element_type: Type) -> None:
    """Register a nested array field for where filtering.

    Args:
        parent_type: The parent type containing the array field
        field_name: The name of the array field
        element_type: The type of elements in the array

    Example:
        register_nested_array_filter(NetworkConfiguration, 'print_servers', PrintServer)
    """
    parent_key = f"{parent_type.__module__}.{parent_type.__name__}"

    if parent_key not in _nested_array_filters:
        _nested_array_filters[parent_key] = {}

    _nested_array_filters[parent_key][field_name] = element_type
    logger.debug(
        f"Registered nested array filter: {parent_key}.{field_name} -> {element_type.__name__}"
    )


def enable_nested_array_filtering(parent_type: Type) -> None:
    """Automatically enable where filtering for all list[SomeType] fields in a type.

    This scans the type annotations and automatically registers filters for any
    fields that are list[SomeType] where SomeType has FraiseQL metadata.

    Args:
        parent_type: The parent type to scan and enable filtering for

    Example:
        enable_nested_array_filtering(NetworkConfiguration)
        # Automatically enables filtering for all list[T] fields where T is a FraiseQL type
    """
    from typing import get_type_hints

    try:
        type_hints = get_type_hints(parent_type)
    except Exception as e:
        logger.warning(f"Could not get type hints for {parent_type}: {e}")
        return

    for field_name, field_type in type_hints.items():
        element_type = _extract_list_element_type(field_type)
        if element_type and _is_fraiseql_type(element_type):
            register_nested_array_filter(parent_type, field_name, element_type)
            logger.info(f"Auto-registered nested array filter: {parent_type.__name__}.{field_name}")


def get_nested_array_filter(parent_type: Type, field_name: str) -> Type | None:
    """Get the registered element type for a nested array field.

    Args:
        parent_type: The parent type
        field_name: The field name

    Returns:
        The element type if registered, None otherwise
    """
    parent_key = f"{parent_type.__module__}.{parent_type.__name__}"
    return _nested_array_filters.get(parent_key, {}).get(field_name)


def is_nested_array_filterable(parent_type: Type, field_name: str) -> bool:
    """Check if a field is registered for nested array filtering.

    Args:
        parent_type: The parent type
        field_name: The field name

    Returns:
        True if the field is registered for filtering
    """
    return get_nested_array_filter(parent_type, field_name) is not None


def list_registered_filters() -> dict[str, dict[str, str]]:
    """List all registered nested array filters.

    Returns:
        Dictionary mapping parent types to their registered array fields
    """
    result = {}
    for parent_key, fields in _nested_array_filters.items():
        result[parent_key] = {
            field_name: element_type.__name__ for field_name, element_type in fields.items()
        }
    return result


def clear_registry() -> None:
    """Clear the nested array filter registry. Mainly for testing."""
    global _nested_array_filters
    _nested_array_filters = {}


def _extract_list_element_type(field_type: Any) -> Type | None:
    """Extract the element type from list[T] annotations."""
    origin = get_origin(field_type)

    # Handle list[T]
    if origin is list:
        args = get_args(field_type)
        if args:
            return args[0]

    # Handle Optional[list[T]] -> Union[list[T], None]
    if origin is type(None) or (hasattr(origin, "__name__") and origin.__name__ == "UnionType"):
        args = get_args(field_type)
        for arg in args:
            if arg is not type(None):
                return _extract_list_element_type(arg)

    return None


def _is_fraiseql_type(type_class: Type) -> bool:
    """Check if a type is a FraiseQL type (has __fraiseql_definition__)."""
    return hasattr(type_class, "__fraiseql_definition__")


# Convenience decorators for cleaner registration


def nested_array_filterable(*field_names: str) -> Callable:
    """Decorator to mark specific fields as filterable.

    Usage:
        @nested_array_filterable('print_servers', 'dns_servers')
        @fraise_type
        class NetworkConfiguration:
            print_servers: list[PrintServer] = fraise_field(default_factory=list)
            dns_servers: list[DnsServer] = fraise_field(default_factory=list)
    """

    def decorator(cls: Type) -> Type:
        from typing import get_type_hints

        try:
            type_hints = get_type_hints(cls)
            for field_name in field_names:
                if field_name in type_hints:
                    element_type = _extract_list_element_type(type_hints[field_name])
                    if element_type:
                        register_nested_array_filter(cls, field_name, element_type)
        except Exception as e:
            logger.warning(f"Could not process @nested_array_filterable for {cls}: {e}")

        return cls

    return decorator


def auto_nested_array_filters(cls: Type) -> Type:
    """Decorator to automatically enable filtering for all list[T] fields.

    Usage:
        @auto_nested_array_filters
        @fraise_type
        class NetworkConfiguration:
            print_servers: list[PrintServer] = fraise_field(default_factory=list)
            dns_servers: list[DnsServer] = fraise_field(default_factory=list)
            # Both fields automatically get where filtering enabled
    """
    enable_nested_array_filtering(cls)
    return cls
