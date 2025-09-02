"""GraphQL field type extraction utilities.

This module provides utilities to extract field type information from GraphQL
resolver context, enabling proper field type propagation to SQL operator strategies.
This is particularly important for specialized types like network addresses (IP, MAC)
that require specific SQL casting for proper operator behavior.
"""

from typing import Any, get_type_hints

from fraiseql.types import IpAddress, MacAddress

# Import specialized types with fallback handling
try:
    from fraiseql.types.scalars.ltree import LTreeField
except ImportError:
    LTreeField = None

try:
    from fraiseql.types.scalars.daterange import DateRangeField
except ImportError:
    DateRangeField = None


def extract_field_type_from_graphql_info(info: Any, field_name: str) -> type | None:
    """Extract field type information from GraphQL resolver context.

    This function bridges the gap between GraphQL schema definitions
    (where field types like IpAddress are declared) and SQL operator
    strategy selection (which needs this type info for proper casting).

    Args:
        info: GraphQL resolve info context
        field_name: Name of the field to extract type for

    Returns:
        Field type class (e.g., IpAddress) or None if not found
    """
    # Strategy 1: Extract from GraphQL return type annotations
    field_type = _extract_from_return_type_annotations(info, field_name)
    if field_type:
        return field_type

    # Strategy 2: Extract from parent type being resolved
    field_type = _extract_from_parent_type(info, field_name)
    if field_type:
        return field_type

    # Strategy 3: Use field name heuristics for common types
    field_type = _extract_from_field_name_heuristics(field_name)
    if field_type:
        return field_type

    return None


def _extract_from_return_type_annotations(info: Any, field_name: str) -> type | None:
    """Extract field type from GraphQL resolver return type annotations."""
    try:
        # Try to get the resolver function from GraphQL info
        if hasattr(info, "field_definition") and hasattr(info.field_definition, "resolver"):
            resolver = info.field_definition.resolver
            if resolver:
                # Get type hints from resolver function
                type_hints = get_type_hints(resolver)
                return_type = type_hints.get("return", None)

                if return_type:
                    # If return type is a dataclass/FraiseQL type, get field type
                    return _extract_field_type_from_dataclass(return_type, field_name)

    except (AttributeError, TypeError):
        pass

    return None


def _extract_from_parent_type(info: Any, field_name: str) -> type | None:
    """Extract field type from the parent type being resolved."""
    try:
        # Check if we can get the parent type from info
        if hasattr(info, "parent_type"):
            # This would need to be enhanced with actual GraphQL type mapping
            # For now, we rely on field name heuristics
            pass

    except (AttributeError, TypeError):
        pass

    return None


def _extract_field_type_from_dataclass(dataclass_type: type, field_name: str) -> type | None:
    """Extract field type from a FraiseQL dataclass type."""
    try:
        # Convert GraphQL field names to Python field names if needed
        python_field_name = _convert_graphql_to_python_field_name(field_name)

        # Get type hints from the dataclass
        type_hints = get_type_hints(dataclass_type)

        # Try exact match first
        field_type = type_hints.get(python_field_name)
        if field_type:
            return field_type

        # Try camelCase to snake_case conversion
        snake_case_name = _camel_to_snake(field_name)
        field_type = type_hints.get(snake_case_name)
        if field_type:
            return field_type

    except (AttributeError, TypeError):
        pass

    return None


def _extract_from_field_name_heuristics(field_name: str) -> type | None:
    """Use field name patterns to infer types for common fields."""
    field_lower = field_name.lower()

    # IP address patterns - handle both snake_case and camelCase
    ip_patterns = [
        "ip_address",
        "ipaddress",
        "server_ip",
        "gateway_ip",
        "host_ip",
        "serverip",
        "gatewayip",
        "hostip",  # camelCase variations
    ]
    if any(pattern in field_lower for pattern in ip_patterns):
        return IpAddress

    # Additional IP patterns that should be whole words or at start/end
    if (
        field_lower in ["ip", "host"]
        or field_lower.endswith(("_ip", "ip"))
        or field_lower.startswith(("ip_", "ip"))
    ):
        return IpAddress

    # MAC address patterns
    mac_patterns = ["mac_address", "macaddress", "mac", "hardware_address"]
    if any(pattern in field_lower for pattern in mac_patterns):
        return MacAddress

    # LTree patterns (hierarchical paths)
    ltree_patterns = ["path", "tree_path", "hierarchy"]
    if any(pattern in field_lower for pattern in ltree_patterns) and LTreeField:
        return LTreeField

    # DateRange patterns
    daterange_patterns = ["date_range", "daterange", "period", "time_range"]
    if any(pattern in field_lower for pattern in daterange_patterns) and DateRangeField:
        return DateRangeField

    return None


def _convert_graphql_to_python_field_name(graphql_name: str) -> str:
    """Convert GraphQL field name to Python field name."""
    # GraphQL typically uses camelCase, Python uses snake_case
    return _camel_to_snake(graphql_name)


def _camel_to_snake(name: str) -> str:
    """Convert camelCase to snake_case."""
    import re

    # Insert underscore before uppercase letters (except at start)
    s1 = re.sub("(.)([A-Z][a-z]+)", r"\1_\2", name)
    # Insert underscore before uppercase letters preceded by lowercase
    return re.sub("([a-z0-9])([A-Z])", r"\1_\2", s1).lower()


def enhance_type_hints_with_graphql_context(
    type_hints: dict[str, type] | None, graphql_info: Any, field_names: list[str]
) -> dict[str, type]:
    """Enhance type hints dictionary with GraphQL context field type extraction.

    This function augments the existing type_hints with field types extracted
    from GraphQL context, enabling proper field type propagation to SQL operators.

    Args:
        type_hints: Existing type hints dictionary (may be None)
        graphql_info: GraphQL resolve info context
        field_names: List of field names to extract types for

    Returns:
        Enhanced type hints dictionary with GraphQL-extracted field types
    """
    enhanced_hints = type_hints.copy() if type_hints else {}

    # Extract field types from GraphQL context, overriding generic types with specific ones
    for field_name in field_names:
        # Always try to extract GraphQL field type
        field_type = extract_field_type_from_graphql_info(graphql_info, field_name)
        if field_type:
            existing_type = enhanced_hints.get(field_name)
            # Override if field doesn't exist or is a generic type (str, int, etc.)
            if existing_type is None or existing_type in (str, int, float, bool):
                enhanced_hints[field_name] = field_type

    return enhanced_hints
