"""Custom GraphQL scalar type for DateRange for FraiseQL."""

import re
from datetime import datetime
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import (
    StringValueNode,
    ValueNode,
)

from fraiseql.types.definitions import ScalarMarker


def is_valid_date(date_str: str) -> bool:
    """Check if a date string is valid and in the correct format."""
    try:
        _ = datetime.strptime(date_str, "%Y-%m-%d")  # noqa: DTZ007
    except ValueError:
        return False
    else:
        return True


def serialize_date_range(value: Any) -> str:
    """Serialize a date range string.

    Args:
        value: The date range value to serialize.

    Returns:
        The serialized date range string.

    Raises:
        GraphQLError: If the value is not a string.
    """
    if isinstance(value, str):
        # Add validation logic here if needed
        return value
    msg = (
        f"DateRange cannot represent non-string value: {value!r}. "
        f"Expected a valid date range string."
    )
    raise GraphQLError(msg)


def parse_date_range_value(value: Any) -> str | None:
    """Parse a date range string.

    Args:
        value: The date range value to parse.

    Returns:
        The parsed date range string.

    Raises:
        GraphQLError: If the value is not a string or None, or if the date range
            format is invalid.
    """
    if value is None:
        return None

    if not isinstance(value, str):
        msg = (
            f"DateRange cannot represent non-string value: {value!r}. "
            f"Expected a valid date range string."
        )
        raise GraphQLError(msg)

    # Regular expression to match the date range pattern
    pattern = r"^[\[\(](\d{4}-\d{2}-\d{2}),\s*(\d{4}-\d{2}-\d{2})[\]\)]$"
    match = re.match(pattern, value)

    if not match:
        msg = (
            f"Invalid date range format: {value}. "
            f"Expected format like '[YYYY-MM-DD, YYYY-MM-DD]' or '(YYYY-MM-DD, YYYY-MM-DD)'."
        )
        raise GraphQLError(msg)

    start_date_str, end_date_str = match.groups()

    if not is_valid_date(start_date_str) or not is_valid_date(end_date_str):
        msg = f"Invalid date in range: {value}. Dates should be in the format YYYY-MM-DD."
        raise GraphQLError(msg)

    return value


def parse_date_range_literal(
    ast: ValueNode,
    variables: dict[str, object] | None = None,
) -> str | None:
    """Parse a literal date range string.

    Args:
        ast: The AST node to parse.
        variables: Optional variables for parsing.

    Returns:
        The parsed date range string.

    Raises:
        GraphQLError: If the AST node is not a StringValueNode or if the date range
            format is invalid.
    """
    _ = variables
    if isinstance(ast, StringValueNode):
        date_range_str = ast.value
        # Regular expression to match the date range pattern
        pattern = r"^\[?\(?(\d{4}-\d{2}-\d{2}),\s*(\d{4}-\d{2}-\d{2})\)?\]?$"
        match = re.match(pattern, date_range_str)

        if not match:
            msg = (
                f"Invalid date range format: {date_range_str}. "
                f"Expected format like '[YYYY-MM-DD, YYYY-MM-DD]' or '(YYYY-MM-DD, YYYY-MM-DD)'."
            )
            raise GraphQLError(msg)

        start_date_str, end_date_str = match.groups()

        if not is_valid_date(start_date_str) or not is_valid_date(end_date_str):
            msg = (
                f"Invalid date in range: {date_range_str}. "
                f"Dates should be in the format YYYY-MM-DD."
            )
            raise GraphQLError(msg)

        return date_range_str

    msg = (
        f"DateRange cannot represent non-string literal: {getattr(ast, 'value', None)!r}. "
        f"Expected a valid date range string."
    )
    raise GraphQLError(msg)


DateRangeScalar = GraphQLScalarType(
    name="DateRange",
    description="""Scalar for date range values as strings.
    Examples of date range formats:
    - Inclusive range: '[YYYY-MM-DD, YYYY-MM-DD]' includes both start and end dates.
    - Exclusive range: '(YYYY-MM-DD, YYYY-MM-DD)' excludes both start and end dates.
    - Left-inclusive range: '[YYYY-MM-DD, YYYY-MM-DD)' includes the start date but
      excludes the end date.
    - Right-inclusive range: '(YYYY-MM-DD, YYYY-MM-DD]' excludes the start date but
      includes the end date.
    """,
    serialize=serialize_date_range,
    parse_value=parse_date_range_value,
    parse_literal=parse_date_range_literal,
)


class DateRangeField(str, ScalarMarker):
    """Python-side marker for the DateRange scalar."""

    __slots__ = ()

    def __repr__(self) -> str:
        """Missing docstring."""
        return "DateRange"
