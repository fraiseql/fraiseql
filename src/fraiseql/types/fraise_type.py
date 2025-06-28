"""Decorator to transform a class into a type suitable for use with the FraiseQL library."""

import logging
from collections.abc import Callable
from dataclasses import Field, field
from typing import Any, TypeVar, dataclass_transform, overload

logger = logging.getLogger(__name__)

from fraiseql.fields import fraise_field
from fraiseql.sql.where_generator import safe_create_where_type
from fraiseql.types.constructor import define_fraiseql_type

T = TypeVar("T", bound=type[Any])


@dataclass_transform(field_specifiers=(fraise_field, field, Field))
@overload
def fraise_type(
    _cls: None = None,
    *,
    sql_source: str | None = None,
    implements: list[type] | None = None,
) -> Callable[[T], T]: ...


@overload
def fraise_type(_cls: T) -> T: ...


def fraise_type(
    _cls: T | None = None,
    *,
    sql_source: str | None = None,
    implements: list[type] | None = None,
) -> T | Callable[[T], T]:
    """Decorator to define a FraiseQL GraphQL output type.

    This decorator transforms a Python dataclass into a GraphQL type that can be
    used in your schema. It supports automatic SQL query generation when a sql_source
    is provided.

    Args:
        sql_source: Optional table or view name to bind this type to for automatic
            SQL query generation. When provided, the type becomes queryable and
            filterable through GraphQL.
        implements: Optional list of GraphQL interface types that this type implements.

    Returns:
        The decorated class enhanced with FraiseQL capabilities.

    Examples:
        Basic type without SQL binding:
        ```python
        @fraise_type
        @dataclass
        class User:
            id: int
            name: str
            email: str
        ```

        Type with SQL source for automatic queries:
        ```python
        @fraise_type(sql_source="users")
        @dataclass
        class User:
            id: int
            name: str
            email: str
        ```

        Type implementing interfaces:
        ```python
        @fraise_type(sql_source="users", implements=[Node, Timestamped])
        @dataclass
        class User:
            id: int
            name: str
            created_at: datetime
        ```
    """

    def wrapper(cls: T) -> T:
        from fraiseql.utils.fields import patch_missing_field_types

        logger.debug("Decorating class %s at %s", cls.__name__, id(cls))

        # Patch types *before* definition is frozen
        patch_missing_field_types(cls)

        # Infer kind: treat no SQL source as a pure type
        inferred_kind = "type" if sql_source is None else "output"
        cls = define_fraiseql_type(cls, kind=inferred_kind)

        if sql_source:
            cls.__gql_table__ = sql_source
            cls.__fraiseql_definition__.sql_source = sql_source
            cls.__gql_where_type__ = safe_create_where_type(cls)

        # Store interfaces this type implements
        if implements:
            cls.__fraiseql_interfaces__ = implements
            # Register type with schema builder if it implements interfaces
            from fraiseql.gql.schema_builder import SchemaRegistry

            SchemaRegistry.get_instance().register_type(cls)

        return cls

    return wrapper if _cls is None else wrapper(_cls)
