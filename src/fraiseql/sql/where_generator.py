"""Dynamic WHERE clause generator with SQL translation for dataclasses.

This module provides utilities to dynamically generate filter types for
dataclasses that translate into SQL WHERE clauses. It uses parameterized
SQL with placeholders to prevent SQL injection. The module supports a set
of filter operators mapped to SQL expressions and dynamic creation of
filter dataclasses with a `to_sql()` method.

This enables flexible and type-safe construction of SQL WHERE conditions
from Python data structures, useful in GraphQL-to-SQL translation layers
and similar query builders.
"""

from collections.abc import Callable
from dataclasses import dataclass, field
from functools import cache
from typing import (
    Any,
    Protocol,
    TypeVar,
    Union,
    cast,
    get_args,
    get_origin,
    get_type_hints,
    runtime_checkable,
)

from psycopg.sql import SQL, Composed, Literal

# Define a type variable for the generic value to coerce
TValue = TypeVar("TValue", bound=object)


@runtime_checkable
class DynamicType(Protocol):
    """Protocol for dynamic filter types convertible to SQL WHERE clause strings."""

    def to_sql(self) -> Composed | None:
        """Return a properly parameterized SQL snippet representing this filter.

        Returns:
            A psycopg Composed object with parameterized SQL, or None if no condition.
        """


def build_operator_composed(path_sql: SQL, op: str, val: object) -> Composed:
    """Build parameterized SQL for a specific operator using psycopg Composed.

    Args:
        path_sql: The SQL object representing the JSONB path expression.
        op: The operator name.
        val: The value to compare against.

    Returns:
        A psycopg Composed object with properly parameterized SQL.
    """
    # Special handling for isnull operator - don't convert the boolean
    if op == "isnull":
        # val is a boolean indicating whether we want IS NULL or IS NOT NULL
        if val:
            return Composed([path_sql, SQL(" IS NULL")])
        return Composed([path_sql, SQL(" IS NOT NULL")])

    # For JSONB text extraction (->>) we need to convert booleans to strings
    # since data->>'field' returns text, not boolean
    if isinstance(val, bool):
        val = str(val).lower()  # Convert True to 'true', False to 'false'

    if op == "eq":
        return Composed([path_sql, SQL(" = "), Literal(val)])
    if op == "neq":
        return Composed([path_sql, SQL(" != "), Literal(val)])
    if op == "gt":
        return Composed([path_sql, SQL(" > "), Literal(val)])
    if op == "gte":
        return Composed([path_sql, SQL(" >= "), Literal(val)])
    if op == "lt":
        return Composed([path_sql, SQL(" < "), Literal(val)])
    if op == "lte":
        return Composed([path_sql, SQL(" <= "), Literal(val)])
    if op == "contains":
        return Composed([path_sql, SQL(" @> "), Literal(val)])
    if op == "overlaps":
        return Composed([path_sql, SQL(" && "), Literal(val)])
    if op == "matches":
        return Composed([path_sql, SQL(" ~ "), Literal(val)])
    if op == "startswith":
        if isinstance(val, str):
            # Use LIKE for better performance and safety
            return Composed([path_sql, SQL(" LIKE "), Literal(val + "%")])
        return Composed([path_sql, SQL(" ~ "), Literal(str(val)), SQL(".*")])
    if op == "in":
        if isinstance(val, list):
            # Convert booleans to strings for JSONB text comparison
            converted_vals = [str(v).lower() if isinstance(v, bool) else v for v in val]
            literals = [Literal(v) for v in converted_vals]
            parts = [path_sql, SQL(" IN (")]
            for i, lit in enumerate(literals):
                if i > 0:
                    parts.append(SQL(", "))
                parts.append(lit)
            parts.append(SQL(")"))
            return Composed(parts)
        msg = f"'in' operator requires a list, got {type(val)}"
        raise ValueError(msg)
    if op == "notin":
        if isinstance(val, list):
            # Convert booleans to strings for JSONB text comparison
            converted_vals = [str(v).lower() if isinstance(v, bool) else v for v in val]
            literals = [Literal(v) for v in converted_vals]
            parts = [path_sql, SQL(" NOT IN (")]
            for i, lit in enumerate(literals):
                if i > 0:
                    parts.append(SQL(", "))
                parts.append(lit)
            parts.append(SQL(")"))
            return Composed(parts)
        msg = f"'notin' operator requires a list, got {type(val)}"
        raise ValueError(msg)
    if op == "depth_eq":
        return Composed([SQL("nlevel("), path_sql, SQL(") = "), Literal(val)])
    if op == "depth_gt":
        return Composed([SQL("nlevel("), path_sql, SQL(") > "), Literal(val)])
    if op == "depth_lt":
        return Composed([SQL("nlevel("), path_sql, SQL(") < "), Literal(val)])
    if op == "isdescendant":
        return Composed([path_sql, SQL(" <@ "), Literal(val)])
    if op == "strictly_contains":
        return Composed(
            [
                path_sql,
                SQL(" @> "),
                Literal(val),
                SQL(" AND "),
                path_sql,
                SQL(" != "),
                Literal(val),
            ],
        )
    msg = f"Unsupported operator: {op}"
    raise ValueError(msg)


def _make_filter_field_composed(
    name: str,
    valdict: dict[str, object],
    json_path: str,
) -> Composed | None:
    """Generate a parameterized SQL expression for a single field filter.

    Args:
        name: Field name to filter.
        valdict: Dict mapping operator strings to filter values.
        json_path: SQL JSON path expression string accessing the field.

    Returns:
        Composed SQL WHERE clause for the field, or None if no valid operators found.
    """
    conditions = []

    for op, val in valdict.items():
        if val is None:
            continue
        try:
            # Build the JSONB path expression
            path_sql = Composed([SQL("("), SQL(json_path), SQL(" ->> "), Literal(name), SQL(")")])
            condition = build_operator_composed(path_sql, op, val)
            conditions.append(condition)
        except ValueError:
            # Skip unsupported operators
            continue

    if not conditions:
        return None
    if len(conditions) == 1:
        return conditions[0]
    # Combine multiple conditions with AND
    result_parts = []
    for i, cond in enumerate(conditions):
        if i > 0:
            result_parts.append(SQL(" AND "))
        result_parts.append(cond)
    return Composed(result_parts)


def _build_where_to_sql(fields: list[str]) -> Callable[[object], Composed | None]:
    """Build a `to_sql` method for a dynamic filter dataclass.

    Args:
        fields: List of filter field names.

    Returns:
        A function suitable as a `to_sql(self)` method returning Composed SQL.
    """

    def to_sql(self: object) -> Composed | None:
        conditions: list[Composed] = []
        for name in fields:
            val = getattr(self, name, None)
            if val is not None and hasattr(val, "to_sql"):
                # Assume val is another DynamicType
                sql = val.to_sql()
                if sql:
                    conditions.append(sql)
            elif isinstance(val, dict):
                cond = _make_filter_field_composed(name, cast("dict[str, object]", val), "data")
                if cond:
                    conditions.append(cond)

        if not conditions:
            return None

        # Combine conditions with AND
        result_parts: list[SQL | Composed] = []
        for i, cond in enumerate(conditions):
            if i > 0:
                result_parts.append(SQL(" AND "))
            result_parts.append(cond)

        return Composed(result_parts)

    return to_sql


def unwrap_type(typ: type[Any]) -> type[Any]:
    """Unwrap Optional[T] to T, or return type as is.

    Args:
        typ: A type annotation to unwrap.

    Returns:
        The inner type if `typ` is Optional[T], else `typ` unchanged.
    """
    if get_origin(typ) is Union:
        args = [arg for arg in get_args(typ) if arg is not type(None)]
        if len(args) == 1:
            return args[0]
    return typ


@cache
def safe_create_where_type(cls: type[object]) -> type[DynamicType] | object:
    """Create a dataclass-based WHERE filter type dynamically for a given class.

    Args:
        cls: The base dataclass to generate filter type for.

    Returns:
        A new dataclass type implementing DynamicType with dict[str, base_type] | None
        fields and a to_sql method for SQL generation with parameterized queries.
    """
    type_hints = get_type_hints(cls)
    annotations: dict[str, object] = {}
    attrs: dict[str, object] = {}

    for name, typ in type_hints.items():
        unwrap_type(typ)
        annotations[name] = dict[str, Any] | None  # Use Any for the dict value type
        attrs[name] = field(default_factory=dict)

    field_names = list(type_hints.keys())
    attrs["__annotations__"] = annotations
    attrs["to_sql"] = _build_where_to_sql(field_names)

    where_name = f"{cls.__name__}Where"
    return dataclass(type(where_name, (), attrs))
