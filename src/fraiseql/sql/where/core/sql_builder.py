"""SQL building utilities for where clauses.

This module provides the main entry point for building SQL WHERE clauses
from GraphQL filter inputs.
"""

from typing import Any

from psycopg.sql import SQL, Composed

from fraiseql.sql.where.operators import get_operator_function

from .field_detection import detect_field_type


def build_where_clause(graphql_where: dict[str, Any]) -> Composed | None:
    """Build a SQL WHERE clause from GraphQL where input.

    Args:
        graphql_where: Dictionary representing GraphQL where input

    Returns:
        Composed SQL WHERE clause or None if no conditions
    """
    if not graphql_where:
        return None

    conditions = []

    for field_name, field_filter in graphql_where.items():
        if not isinstance(field_filter, dict):
            continue

        for operator, value in field_filter.items():
            if value is None:
                continue

            # Detect field type
            field_type = detect_field_type(field_name, value, None)

            # Get operator function
            operator_func = get_operator_function(field_type, operator)

            # Convert GraphQL field name (camelCase) to database field name (snake_case)
            db_field_name = _camel_to_snake(field_name)

            # Build JSONB path
            path_sql = SQL(f"(data ->> '{db_field_name}')")

            # Build condition
            condition = operator_func(path_sql, value)
            conditions.append(condition)

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


def _camel_to_snake(name: str) -> str:
    """Convert camelCase to snake_case."""
    import re

    # Insert underscore before uppercase letters that follow lowercase letters
    s1 = re.sub("(.)([A-Z][a-z]+)", r"\1_\2", name)
    # Insert underscore before uppercase letters that follow lowercase letters or digits
    return re.sub("([a-z0-9])([A-Z])", r"\1_\2", s1).lower()
