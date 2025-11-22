"""PostgreSQL JSONB operators for FraiseQL WHERE filtering."""

from typing import Any, Optional

from psycopg.sql import SQL, Composed, Literal
from pydantic import BaseModel


def build_has_key_sql(field_sql: SQL | Composed, value: str) -> Composed:
    """Build SQL for JSONB key existence using ? operator.

    Args:
        field_sql: The SQL for the JSONB field
        value: The key name to check for existence

    Returns:
        SQL fragment for the key existence check
    """
    return Composed([field_sql, SQL(" ? "), Literal(value)])


def build_has_any_keys_sql(field_sql: SQL | Composed, value: list[str]) -> Composed:
    """Build SQL for JSONB any key existence using ?| operator.

    Args:
        field_sql: The SQL for the JSONB field
        value: List of key names (match if ANY exist)

    Returns:
        SQL fragment for the any keys existence check
    """
    # Convert Python list to PostgreSQL array literal
    array_literal = "{" + ",".join(value) + "}"
    return Composed([field_sql, SQL(" ?| "), Literal(array_literal)])


def build_has_all_keys_sql(field_sql: SQL | Composed, value: list[str]) -> Composed:
    """Build SQL for JSONB all keys existence using ?& operator.

    Args:
        field_sql: The SQL for the JSONB field
        value: List of key names (match if ALL exist)

    Returns:
        SQL fragment for the all keys existence check
    """
    # Convert Python list to PostgreSQL array literal
    array_literal = "{" + ",".join(value) + "}"
    return Composed([field_sql, SQL(" ?& "), Literal(array_literal)])


def build_contains_sql(field_sql: SQL | Composed, value: Any) -> Composed:
    """Build SQL for JSONB containment using @> operator.

    Args:
        field_sql: The SQL for the JSONB field
        value: JSONB value to check if contained

    Returns:
        SQL fragment for the containment check
    """
    import json

    json_str = json.dumps(value)
    return Composed([field_sql, SQL(" @> "), Literal(json_str), SQL("::jsonb")])


def build_contained_by_sql(field_sql: SQL | Composed, value: Any) -> Composed:
    """Build SQL for JSONB contained by using <@ operator.

    Args:
        field_sql: The SQL for the JSONB field
        value: JSONB value to check if it contains the field

    Returns:
        SQL fragment for the contained by check
    """
    import json

    json_str = json.dumps(value)
    return Composed([field_sql, SQL(" <@ "), Literal(json_str), SQL("::jsonb")])


def build_path_exists_sql(field_sql: SQL | Composed, value: str) -> Composed:
    """Build SQL for JSONPath existence using @? operator.

    Args:
        field_sql: The SQL for the JSONB field
        value: JSONPath expression

    Returns:
        SQL fragment for the path existence check
    """
    return Composed([field_sql, SQL(" @? "), Literal(value)])


def build_path_match_sql(field_sql: SQL | Composed, value: str) -> Composed:
    """Build SQL for JSONPath match using @@ operator.

    Args:
        field_sql: The SQL for the JSONB field
        value: JSONPath predicate expression

    Returns:
        SQL fragment for the path match check
    """
    return Composed([field_sql, SQL(" @@ "), Literal(value)])


def build_get_path_sql(field_sql: SQL | Composed, value: list[str]) -> Composed:
    """Build SQL for JSONB path access using #> operator.

    Args:
        field_sql: The SQL for the JSONB field
        value: Array of path elements

    Returns:
        SQL fragment for the path access
    """
    # Convert Python list to PostgreSQL array literal
    array_literal = "{" + ",".join(value) + "}"
    return Composed([field_sql, SQL(" #> "), Literal(array_literal)])


def build_get_path_text_sql(field_sql: SQL | Composed, value: list[str]) -> Composed:
    """Build SQL for JSONB path access as text using #>> operator.

    Args:
        field_sql: The SQL for the JSONB field
        value: Array of path elements

    Returns:
        SQL fragment for the path access as text
    """
    # Convert Python list to PostgreSQL array literal
    array_literal = "{" + ",".join(value) + "}"
    return Composed([field_sql, SQL(" #>> "), Literal(array_literal)])


def build_strictly_contains_sql(field_sql: SQL | Composed, value: Any) -> Composed:
    """Build SQL for JSONB strictly contains (contains but not equal).

    This checks if the JSONB field contains the value but is not equal to it.
    Equivalent to: field @> value AND field != value

    Args:
        field_sql: The SQL expression for the JSONB field
        value: Dict or list to check containment (not equality)

    Returns:
        Composed SQL: field @> 'value'::jsonb AND field != 'value'::jsonb
    """
    import json

    json_str = json.dumps(value)
    return Composed(
        [
            field_sql,
            SQL(" @> "),
            Literal(json_str),
            SQL("::jsonb"),
            SQL(" AND "),
            field_sql,
            SQL(" != "),
            Literal(json_str),
            SQL("::jsonb"),
        ]
    )


class JSONBFilter(BaseModel):
    """JSONB filter operators for PostgreSQL JSONB columns.

    Supports PostgreSQL's JSONB capabilities including:
    - Key existence checks with ?, ?|, ?&
    - Containment operations with @>, <@
    - JSONPath queries with @?, @@
    - Deep path access with #>, #>>
    """

    # Basic comparison operators
    eq: Optional[Any] = None
    """Exact equality comparison (accepts dict or list)."""

    neq: Optional[Any] = None
    """Not equal comparison (accepts dict or list)."""

    isnull: Optional[bool] = None
    """Check if field is null."""

    # Key existence operators
    has_key: Optional[str] = None
    """Check if JSONB contains a specific key (? operator)."""

    has_any_keys: Optional[list[str]] = None
    """Check if JSONB contains any of the specified keys (?| operator)."""

    has_all_keys: Optional[list[str]] = None
    """Check if JSONB contains all of the specified keys (?& operator)."""

    # Containment operators
    contains: Optional[Any] = None
    """Check if JSONB contains the specified value (@> operator, accepts dict or list)."""

    contained_by: Optional[Any] = None
    """Check if JSONB is contained by the specified value (<@ operator, accepts dict or list)."""

    # JSONPath operators
    path_exists: Optional[str] = None
    """Check if JSONPath expression exists (@? operator)."""

    path_match: Optional[str] = None
    """Check if JSONPath predicate matches (@@ operator)."""

    # Deep path access operators
    get_path: Optional[list[str]] = None
    """Get value at path (#> operator)."""

    get_path_text: Optional[list[str]] = None
    """Get value at path as text (#>> operator)."""
