"""SQL building utilities for where clauses.

This module provides the main entry point for building SQL WHERE clauses
from GraphQL filter inputs.
"""

from typing import Any

from psycopg.sql import SQL, Composed, Literal

from fraiseql.sql.operator_strategies import get_operator_registry


def is_operator_dict(d: dict) -> bool:
    """Check if dict contains operators vs nested objects."""
    operators = {
        "eq",
        "neq",
        "gt",
        "gte",
        "lt",
        "lte",
        "ilike",
        "like",
        "in",
        "notin",
        "contains",
        "startswith",
        "endswith",
        "is_null",
        "is_not_null",
        # Coordinate operators
        "distance_within",
        # Network operators
        "inSubnet",
        "inRange",
        "isPrivate",
        "isPublic",
        "isIPv4",
        "isIPv6",
        "isLoopback",
        "isLinkLocal",
        "isMulticast",
        "isDocumentation",
        "isCarrierGrade",
        # LTree operators
        "ancestor_of",
        "descendant_of",
        "matches_lquery",
        "matches_ltxtquery",
        "nlevel",
        "nlevel_eq",
        "nlevel_gt",
        "nlevel_gte",
        "nlevel_lt",
        "nlevel_lte",
        "subpath",
        "index",
        "index_eq",
        "index_gte",
        "concat",
        "lca",
        "matches_any_lquery",
        "in_array",
        "array_contains",
        # DateRange operators
        "contains_date",
        "overlaps",
        "adjacent",
        "strictly_left",
        "strictly_right",
        "not_left",
        "not_right",
    }
    return any(k in operators for k in d)


def build_jsonb_path(fields: list[str]) -> Composed:
    """Build JSONB navigation path for nested objects.

    Args:
        fields: List of field names from root to leaf

    Returns:
        SQL composed object with JSONB path

    Examples:
        ["status"] → data ->> 'status'
        ["machine", "name"] → data -> 'machine' ->> 'name'
        ["location", "address", "city"] → data -> 'location' -> 'address' ->> 'city'
    """
    if not fields:
        raise ValueError("Fields list cannot be empty")

    if len(fields) == 1:
        # Single field: data ->> 'field'
        return Composed([SQL("data ->> "), Literal(fields[0])])

    # Multiple fields: data -> 'field1' -> 'field2' ->> 'field3'
    parts = [SQL("data")]

    for i, field in enumerate(fields):
        if i == len(fields) - 1:
            # Last field: ->> (text extraction)
            parts.append(SQL(" ->> "))
            parts.append(Literal(field))
        else:
            # Intermediate fields: -> (JSONB navigation)
            parts.append(SQL(" -> "))
            parts.append(Literal(field))

    return Composed(parts)


def build_where_clause_recursive(where_dict: dict, path: list[str] | None = None) -> list[Composed]:
    """Recursively build WHERE clause with nested object support.

    Args:
        where_dict: WHERE clause dictionary
        path: Current field path in JSONB tree

    Returns:
        List of SQL conditions
    """
    if path is None:
        path = []

    conditions = []

    for field, value in where_dict.items():
        # Convert field name from camelCase to snake_case for JSONB path
        db_field_name = _camel_to_snake(field)

        if isinstance(value, dict) and not is_operator_dict(value):
            # Nested object - recurse deeper
            nested_path = [*path, db_field_name]
            nested_conditions = build_where_clause_recursive(value, nested_path)
            conditions.extend(nested_conditions)
        else:
            # Leaf node with operators
            full_path = [*path, db_field_name]
            jsonb_path = build_jsonb_path(full_path)

            # Handle operators on this field
            if isinstance(value, dict):
                for operator, op_value in value.items():
                    if op_value is None:
                        continue  # Skip None values

                    # Build operator condition using operator registry
                    registry = get_operator_registry()
                    condition = registry.build_sql(
                        path_sql=jsonb_path,
                        op=operator,
                        val=op_value,
                        field_type=None,  # Will auto-detect from value
                    )
                    conditions.append(condition)

    return conditions


def build_where_clause(where_dict: dict) -> Composed:
    """Build WHERE clause with nested object support.

    Args:
        where_dict: WHERE clause dictionary

    Returns:
        Composed SQL WHERE clause

    Examples:
        # Flat filter
        where = {"status": {"eq": "active"}}
        → data->>'status' = %(param_0)s

        # Nested filter
        where = {"machine": {"name": {"eq": "Machine 1"}}}
        → data->'machine'->>'name' = %(param_0)s
    """
    if not where_dict:
        return Composed([SQL("TRUE")])

    conditions = build_where_clause_recursive(where_dict)

    if not conditions:
        return Composed([SQL("TRUE")])

    if len(conditions) == 1:
        return conditions[0]

    # Combine multiple conditions with AND
    parts = [conditions[0]]
    for condition in conditions[1:]:
        parts.extend([SQL(" AND "), condition])

    return Composed(parts)


def build_where_clause_graphql(graphql_where: dict[str, Any]) -> Composed | None:
    """Build a SQL WHERE clause from GraphQL where input.

    Args:
        graphql_where: Dictionary representing GraphQL where input

    Returns:
        Composed SQL WHERE clause or None if no conditions
    """
    if not graphql_where:
        return None

    # Use recursive builder for nested object support
    # The recursive function will handle camelCase to snake_case conversion for all field names
    conditions = build_where_clause_recursive(graphql_where)

    if not conditions:
        return None

    if len(conditions) == 1:
        return conditions[0]

    # Combine multiple conditions with AND
    parts = [SQL("("), conditions[0]]
    for condition in conditions[1:]:
        parts.extend([SQL(" AND "), condition])
    parts.append(SQL(")"))

    return Composed(parts)


# Alias for backward compatibility
build_where_clause_from_graphql = build_where_clause_graphql


def _camel_to_snake(name: str) -> str:
    """Convert camelCase to snake_case."""
    import re

    # Insert underscore before uppercase letters that follow lowercase letters
    s1 = re.sub("(.)([A-Z][a-z]+)", r"\1_\2", name)
    # Insert underscore before uppercase letters that follow lowercase letters or digits
    return re.sub("([a-z0-9])([A-Z])", r"\1_\2", s1).lower()
