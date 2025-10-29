"""Automatic description generation for GraphQL where clause filter types.

This module provides utilities to automatically generate comprehensive field descriptions
for all filter types used in GraphQL where clauses, making Apollo Studio more helpful.
"""

from fraiseql.fields import FraiseQLField

# Standard operator descriptions for different field types
OPERATOR_DESCRIPTIONS = {
    # Equality operations
    "eq": "Exact match - field equals the specified value",
    "neq": "Not equal - field does not equal the specified value",
    # Comparison operations (numeric, date, datetime)
    "gt": "Greater than - field value is greater than the specified value",
    "gte": "Greater than or equal - field value is greater than or equal to the specified value",
    "lt": "Less than - field value is less than the specified value",
    "lte": "Less than or equal - field value is less than or equal to the specified value",
    # String operations
    "contains": "Substring search - field contains the specified text (case-sensitive)",
    "startswith": "Prefix match - field starts with the specified text",
    "endswith": "Suffix match - field ends with the specified text",
    # Array operations
    "in_": "In list - field value is one of the values in the provided list",
    "nin": "Not in list - field value is not in any of the provided list values",
    # Null operations
    "isnull": "Null check - true to find null values, false to find non-null values",
    # Network-specific operations
    "inSubnet": "Subnet membership - IP address is within the specified CIDR subnet",
    "inRange": "Range membership - IP address is within the specified range (from/to)",
    "isPrivate": "Private network - IP address is in RFC 1918 private ranges",
    "isPublic": "Public network - IP address is not in private ranges",
    "isIPv4": "IPv4 address - IP address is IPv4 format",
    "isIPv6": "IPv6 address - IP address is IPv6 format",
    "isLoopback": "Loopback address - IP is loopback (127.0.0.1 or ::1)",
    "isMulticast": "Multicast address - IP is multicast (224.0.0.0/4 or ff00::/8)",
    "isBroadcast": "Broadcast address - IP is broadcast (255.255.255.255)",
    "isLinkLocal": "Link-local address - IP is link-local (169.254.0.0/16 or fe80::/10)",
    "isDocumentation": "Documentation address - IP is in RFC 3849/5737 documentation ranges",
    "isReserved": "Reserved address - IP is reserved/unspecified (0.0.0.0 or ::)",
    "isCarrierGrade": "Carrier-Grade NAT - IP is in CGN range (100.64.0.0/10)",
    "isSiteLocal": "Site-local IPv6 - IP is site-local (fec0::/10, deprecated)",
    "isUniqueLocal": "Unique local IPv6 - IP is unique local (fc00::/7)",
    "isGlobalUnicast": "Global unicast - IP is global unicast address",
    # Range operations
    "from_": "Range start - starting value for range filtering",
    "to": "Range end - ending value for range filtering",
    # Logical operators (future enhancement)
    "AND": "Logical AND - all conditions in the list must be true",
    "OR": "Logical OR - at least one condition in the list must be true",
    "NOT": "Logical NOT - negates the given condition",
    # LTREE hierarchical path operations
    "ancestor_of": "Hierarchical ancestor - path is an ancestor of the specified path",
    "descendant_of": "Hierarchical descendant - path is a descendant of the specified path",
    "matches_lquery": "Pattern match - path matches the lquery pattern (wildcards supported)",
    "matches_ltxtquery": "Text search - path matches the ltxtquery text pattern (AND/OR/NOT)",
    "matches_any_lquery": "Match any pattern - path matches any of the provided lquery patterns",
    "in_array": "Path in array - path is contained in the specified array of paths",
    "array_contains": "Array contains path - the specified array contains the target path",
    "nlevel": "Path depth - returns the number of labels in the path",
    "nlevel_eq": "Exact depth - path has exactly N levels (e.g., nlevel_eq: 3 for 3-level paths)",
    "nlevel_gt": "Depth greater than - path has more than N levels",
    "nlevel_gte": "Depth greater than or equal - path has N or more levels",
    "nlevel_lt": "Depth less than - path has fewer than N levels",
    "nlevel_lte": "Depth less than or equal - path has N or fewer levels",
    "subpath": "Extract subpath - extract a portion of the path (offset, length)",
    "index": "Find sublabel position - returns position of sublabel in path (-1 if not found)",
    "index_eq": "Exact position - sublabel appears at the specified position in path",
    "index_gte": "Minimum position - sublabel appears at or after the specified position",
    "concat": "Concatenate paths - join two hierarchical paths together",
    "lca": "Lowest common ancestor - find the most specific common ancestor of multiple paths",
}


# Filter type descriptions by class name
FILTER_TYPE_DESCRIPTIONS = {
    "StringFilter": {
        "description": "String field filtering operations for text search and matching.",
        "note": "All string operations are case-sensitive.",
    },
    "IntFilter": {
        "description": "Integer field filtering operations for numeric comparisons.",
        "note": "Supports exact matches, ranges, and list membership.",
    },
    "FloatFilter": {
        "description": "Floating-point field filtering operations for numeric comparisons.",
        "note": "Supports exact matches, ranges, and list membership.",
    },
    "DecimalFilter": {
        "description": "Decimal field filtering operations for precise numeric comparisons.",
        "note": "Use for currency and other precision-critical numeric values.",
    },
    "BooleanFilter": {
        "description": "Boolean field filtering operations for true/false values.",
        "note": "Limited to equality and null checks.",
    },
    "UUIDFilter": {
        "description": "UUID field filtering operations for unique identifier matching.",
        "note": "Supports exact matches and list membership only.",
    },
    "DateFilter": {
        "description": "Date field filtering operations for date-only comparisons.",
        "note": "Use YYYY-MM-DD format for date values.",
    },
    "DateTimeFilter": {
        "description": "DateTime field filtering operations for timestamp comparisons.",
        "note": "Use ISO 8601 format for datetime values (e.g., 2023-12-25T10:30:00Z).",
    },
    "NetworkAddressFilter": {
        "description": "Network address filtering with IP-specific operations for CIDR/inet types.",
        "note": "Includes advanced network classification beyond basic string matching.",
    },
    "MacAddressFilter": {
        "description": "MAC address filtering with exact matching for hardware addresses.",
        "note": "String pattern matching excluded due to PostgreSQL normalization.",
    },
    "LTreeFilter": {
        "description": "Hierarchical path filtering operations for PostgreSQL ltree data type.",
        "note": "Supports hierarchical relationships, pattern matching, and path analysis.",
    },
    "IPRange": {
        "description": "IP address range specification for network filtering operations.",
        "note": "Define from/to range for IP address filtering.",
    },
    # Logical operator containers (for any WhereInput type)
    "WhereInput": {
        "description": "Advanced filtering with logical operators and field-specific filters.",
        "note": "Combine field filters with AND, OR, NOT for complex queries.",
    },
}


def generate_filter_docstring(filter_class_name: str, fields: dict[str, FraiseQLField]) -> str:
    """Generate a comprehensive docstring for a filter class.

    Args:
        filter_class_name: Name of the filter class (e.g., "StringFilter")
        fields: Dictionary of field names to FraiseQLField objects

    Returns:
        Formatted docstring with description and field documentation
    """
    filter_info = FILTER_TYPE_DESCRIPTIONS.get(filter_class_name, {})
    base_description = filter_info.get("description", f"{filter_class_name} operations.")
    note = filter_info.get("note", "")

    # Start building the docstring
    docstring_parts = [base_description]

    if note:
        docstring_parts.append(f"\n{note}")

    # Add fields section
    docstring_parts.append("\nFields:")

    for field_name, field in fields.items():
        # Get the GraphQL name (might be different from Python name)
        graphql_name = field.graphql_name or field_name
        display_name = graphql_name if graphql_name != field_name else field_name

        description = OPERATOR_DESCRIPTIONS.get(field_name, f"{field_name} operation")
        docstring_parts.append(f"    {display_name}: {description}")

    return "\n".join(docstring_parts)


def apply_filter_descriptions(cls: type) -> None:
    """Apply automatic descriptions to filter type fields.

    This function enhances filter classes (StringFilter, IntFilter, etc.) and
    WhereInput classes with comprehensive field descriptions that will appear in Apollo Studio.

    Args:
        cls: The filter class to enhance with descriptions
    """
    if not hasattr(cls, "__gql_fields__"):
        return

    class_name = cls.__name__

    # Apply to filter classes, where input classes, and special types
    if not (class_name.endswith(("Filter", "WhereInput")) or class_name in ["IPRange"]):
        return

    # Generate and set the class docstring if it's basic
    if not cls.__doc__ or cls.__doc__.strip().endswith("operations."):
        cls.__doc__ = generate_filter_docstring(class_name, cls.__gql_fields__)

    # Apply field descriptions
    for field_name, field in cls.__gql_fields__.items():
        if isinstance(field, FraiseQLField) and not field.description:
            description = OPERATOR_DESCRIPTIONS.get(field_name)
            if description:
                field.description = description
            else:
                # Fallback for unknown operators in filter classes
                field.description = f"{field_name} operation"


# List of all known filter class names for batch processing
FILTER_CLASS_NAMES = [
    "StringFilter",
    "IntFilter",
    "FloatFilter",
    "DecimalFilter",
    "BooleanFilter",
    "UUIDFilter",
    "DateFilter",
    "DateTimeFilter",
    "NetworkAddressFilter",
    "MacAddressFilter",
    "IPRange",
]


def enhance_all_filter_types() -> None:
    """Enhance all existing filter types with automatic descriptions.

    This function can be called to retroactively enhance filter types that
    were already defined before this description system was implemented.
    """
    from fraiseql.sql import graphql_where_generator

    # Get all filter classes from the where generator module
    for class_name in FILTER_CLASS_NAMES:
        if hasattr(graphql_where_generator, class_name):
            filter_class = getattr(graphql_where_generator, class_name)
            apply_filter_descriptions(filter_class)
