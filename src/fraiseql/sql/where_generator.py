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

from .operator_strategies import get_operator_registry

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


def build_operator_composed(
    path_sql: SQL,
    op: str,
    val: object,
    field_type: type | None = None,
) -> Composed:
    """Build parameterized SQL for a specific operator using psycopg Composed.

    This function delegates to the operator registry which uses a strategy
    pattern to handle different operators in a clean, maintainable way.

    Args:
        path_sql: The SQL object representing the JSONB path expression.
        op: The operator name.
        val: The value to compare against.
        field_type: Optional type hint for proper casting.

    Returns:
        A psycopg Composed object with properly parameterized SQL.
    """
    registry = get_operator_registry()
    return registry.build_sql(path_sql, op, val, field_type)


def _make_filter_field_composed(
    name: str,
    valdict: dict[str, object],
    json_path: str,
    field_type: type | None = None,
) -> Composed | None:
    """Generate a parameterized SQL expression for a single field filter.

    Args:
        name: Field name to filter.
        valdict: Dict mapping operator strings to filter values.
        json_path: SQL JSON path expression string accessing the field.
        field_type: Optional type hint for the field.

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
            condition = build_operator_composed(path_sql, op, val, field_type)
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


def _build_where_to_sql(
    fields: list[str],
    type_hints: dict[str, type] | None = None,
) -> Callable[[object], Composed | None]:
    """Build a `to_sql` method for a dynamic filter dataclass.

    Args:
        fields: List of filter field names.
        type_hints: Optional mapping of field names to their types.

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
                field_type = type_hints.get(name) if type_hints else None
                cond = _make_filter_field_composed(
                    name,
                    cast("dict[str, object]", val),
                    "data",
                    field_type,
                )
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
    attrs["to_sql"] = _build_where_to_sql(field_names, type_hints)

    where_name = f"{cls.__name__}Where"
    return dataclass(type(where_name, (), attrs))
