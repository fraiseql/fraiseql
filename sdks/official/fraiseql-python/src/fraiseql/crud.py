"""CRUD operation generation for FraiseQL types.

When ``crud=True`` (or a list of operation names) is passed to ``@fraiseql.type()``,
this module auto-registers the standard queries and mutations for the type.

Generated operations follow FraiseQL conventions:
  - Read:   query <snake>(id) + query <snake>s() with auto_params
  - Create: mutation create_<snake>() with all fields as arguments
  - Update: mutation update_<snake>() with PK required, other fields nullable
  - Delete: mutation delete_<snake>() with PK only
"""

from __future__ import annotations

import re
from typing import Any

from fraiseql.registry import SchemaRegistry

_ALL_OPS = frozenset({"read", "create", "update", "delete"})

_CONSONANT_Y_RE = re.compile(r"[^aeiou]y$")


def _pluralize(name: str) -> str:
    """Apply basic English pluralization to a snake_case name.

    Rules (ordered):
      1. Already ends in 's' (but not 'ss') -> no change
      2. Ends in 'ss', 'sh', 'ch', 'x', 'z' -> append 'es'
      3. Ends in consonant + 'y' -> replace 'y' with 'ies'
      4. Default -> append 's'
    """
    if name.endswith("s") and not name.endswith("ss"):
        return name
    for suffix in ("ss", "sh", "ch", "x", "z"):
        if name.endswith(suffix):
            return name + "es"
    if _CONSONANT_Y_RE.search(name):
        return name[:-1] + "ies"
    return name + "s"


def _parse_crud_ops(crud: bool | list[str]) -> set[str]:
    """Normalise the crud parameter into a set of operation names."""
    if crud is True:
        return set(_ALL_OPS)
    if isinstance(crud, list):
        unknown = set(crud) - _ALL_OPS
        if unknown:
            msg = f"Unknown CRUD operations: {sorted(unknown)}. Valid: {sorted(_ALL_OPS)}"
            raise ValueError(msg)
        return set(crud)
    return set()


def generate_crud_operations(  # noqa: PLR0913 — all parameters are meaningful
    type_name: str,
    fields: dict[str, dict[str, Any]],
    crud: bool | list[str],
    sql_source: str,
    cascade: bool = False,
    plural_name: str | None = None,
) -> None:
    """Generate and register CRUD queries/mutations for a type.

    Args:
        type_name: The GraphQL type name (e.g. "Product").
        fields: Field dict from ``extract_field_info()``.
        crud: ``True`` for all ops, or a list like ``["read", "create"]``.
        sql_source: The SQL view name (e.g. "v_product").
        cascade: When ``True``, generated mutations include ``cascade: true``.
        plural_name: Override the auto-pluralized name for the list query.
            When ``None``, the name is derived by pluralizing the snake_case type name.

    Raises:
        ValueError: If no fields are defined or crud contains unknown operations.
    """
    ops = _parse_crud_ops(crud)
    if not ops:
        return

    if not fields:
        msg = f"Type {type_name!r} has no fields; cannot generate CRUD operations"
        raise ValueError(msg)

    snake = _pascal_to_snake(type_name)
    field_list = list(fields.items())
    pk_name, pk_info = field_list[0]

    if "read" in ops:
        _generate_read_ops(type_name, snake, sql_source, pk_name, pk_info, plural_name)

    if "create" in ops:
        _generate_create_op(type_name, snake, field_list, cascade)

    if "update" in ops:
        _generate_update_op(type_name, snake, pk_name, pk_info, field_list, cascade)

    if "delete" in ops:
        _generate_delete_op(type_name, snake, pk_name, pk_info, cascade)


_CAMEL_RE = re.compile(r"(?<!^)(?=[A-Z])")


def _pascal_to_snake(name: str) -> str:
    return _CAMEL_RE.sub("_", name).lower()


def _generate_read_ops(  # noqa: PLR0913 — all parameters are meaningful
    type_name: str,
    snake: str,
    view: str,
    pk_name: str,
    pk_info: dict[str, Any],
    plural_name: str | None = None,
) -> None:
    # Get-by-ID query
    SchemaRegistry.register_query(
        name=snake,
        return_type=type_name,
        returns_list=False,
        nullable=True,
        arguments=[{"name": pk_name, "type": pk_info["type"], "nullable": False}],
        description=f"Get {type_name} by ID.",
        sql_source=view,
    )

    # List query with auto_params
    list_name = plural_name if plural_name is not None else _pluralize(snake)
    SchemaRegistry.register_query(
        name=list_name,
        return_type=type_name,
        returns_list=True,
        nullable=False,
        arguments=[],
        description=f"List {type_name} records.",
        sql_source=view,
        auto_params={"where": True, "order_by": True, "limit": True, "offset": True},
    )


def _generate_create_op(
    type_name: str,
    snake: str,
    field_list: list[tuple[str, dict[str, Any]]],
    cascade: bool,
) -> None:
    args = [
        {"name": name, "type": info["type"], "nullable": info["nullable"]}
        for name, info in field_list
    ]
    kwargs: dict[str, Any] = {
        "name": f"create_{snake}",
        "return_type": type_name,
        "returns_list": False,
        "nullable": False,
        "arguments": args,
        "description": f"Create a new {type_name}.",
        "sql_source": f"fn_create_{snake}",
        "operation": "INSERT",
    }
    if cascade:
        kwargs["cascade"] = True
    SchemaRegistry.register_mutation(**kwargs)


def _generate_update_op(  # noqa: PLR0913 — all parameters are meaningful
    type_name: str,
    snake: str,
    pk_name: str,
    pk_info: dict[str, Any],
    field_list: list[tuple[str, dict[str, Any]]],
    cascade: bool,
) -> None:
    args = [{"name": pk_name, "type": pk_info["type"], "nullable": False}]
    for name, info in field_list[1:]:
        args.append({"name": name, "type": info["type"], "nullable": True})
    kwargs: dict[str, Any] = {
        "name": f"update_{snake}",
        "return_type": type_name,
        "returns_list": False,
        "nullable": True,
        "arguments": args,
        "description": f"Update an existing {type_name}.",
        "sql_source": f"fn_update_{snake}",
        "operation": "UPDATE",
    }
    if cascade:
        kwargs["cascade"] = True
    SchemaRegistry.register_mutation(**kwargs)


def _generate_delete_op(
    type_name: str,
    snake: str,
    pk_name: str,
    pk_info: dict[str, Any],
    cascade: bool,
) -> None:
    kwargs: dict[str, Any] = {
        "name": f"delete_{snake}",
        "return_type": type_name,
        "returns_list": False,
        "nullable": False,
        "arguments": [
            {"name": pk_name, "type": pk_info["type"], "nullable": False},
        ],
        "description": f"Delete a {type_name}.",
        "sql_source": f"fn_delete_{snake}",
        "operation": "DELETE",
    }
    if cascade:
        kwargs["cascade"] = True
    SchemaRegistry.register_mutation(**kwargs)
