"""Logical operators (AND, OR, NOT) - Foundation for issue #33.

This module provides the foundation for implementing logical operators
in GraphQL where clauses, enabling complex nested filtering conditions.
"""

from psycopg.sql import SQL, Composed


def build_and_sql(conditions: list[Composed]) -> Composed:
    """Combine conditions with AND operator.

    Args:
        conditions: List of SQL conditions to combine

    Returns:
        Combined SQL with AND operators

    Examples:
        build_and_sql([condition1, condition2])
        # -> (condition1 AND condition2)
    """
    if not conditions:
        return SQL("TRUE")

    if len(conditions) == 1:
        return conditions[0]

    parts = [SQL("("), conditions[0]]
    for condition in conditions[1:]:
        parts.extend([SQL(" AND "), condition])
    parts.append(SQL(")"))

    return Composed(parts)


def build_or_sql(conditions: list[Composed]) -> Composed:
    """Combine conditions with OR operator.

    Args:
        conditions: List of SQL conditions to combine

    Returns:
        Combined SQL with OR operators

    Examples:
        build_or_sql([condition1, condition2])
        # -> (condition1 OR condition2)
    """
    if not conditions:
        return SQL("FALSE")

    if len(conditions) == 1:
        return conditions[0]

    parts = [SQL("("), conditions[0]]
    for condition in conditions[1:]:
        parts.extend([SQL(" OR "), condition])
    parts.append(SQL(")"))

    return Composed(parts)


def build_not_sql(condition: Composed) -> Composed:
    """Negate a condition with NOT operator.

    Args:
        condition: SQL condition to negate

    Returns:
        Negated SQL condition

    Examples:
        build_not_sql(condition)
        # -> NOT (condition)
    """
    return Composed([SQL("NOT ("), condition, SQL(")")])


# Future GraphQL schema structure for issue #33:
#
# input WhereInput {
#   field1: StringFilter
#   field2: IntFilter
#   # ... other fields
#
#   # Logical operators
#   AND: [WhereInput!]
#   OR: [WhereInput!]
#   NOT: WhereInput
# }
#
# Example usage:
# {
#   OR: [
#     { status: { eq: "draft" } },
#     { status: { eq: "published" } }
#   ]
# }
#
# {
#   AND: [
#     { author: { eq: "john" } },
#     { OR: [
#       { status: { eq: "featured" } },
#       { views: { gt: 1000 } }
#     ]}
#   ]
# }
