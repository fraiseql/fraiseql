"""Decorator to define FraiseQL input types with flexible field ordering.

This decorator supports GraphQL schema generation and type introspection for input types,
using `fraise_field` to mark fields and store metadata.

Unlike traditional `@dataclass`, this avoids default-before-non-default limitations
by generating its own `__init__`, making it compatible with Strawberry-style field layouts.
"""

from collections.abc import Callable
from typing import TypeVar, dataclass_transform, overload

from fraiseql.fields import fraise_field
from fraiseql.types.constructor import define_fraiseql_type
from fraiseql.utils.fields import patch_missing_field_types

T = TypeVar("T", bound=type)


@dataclass_transform(field_specifiers=(fraise_field,))
@overload
def fraise_input(_cls: None = None) -> Callable[[T], T]: ...
@overload
def fraise_input(_cls: T) -> T: ...


def fraise_input(_cls: T | None = None) -> T | Callable[[T], T]:
    """Decorator for FraiseQL input types using keyword-only init and safe field ordering."""

    def wrap(cls: T) -> T:
        from fraiseql.gql.schema_builder import SchemaRegistry

        patch_missing_field_types(cls)
        cls = define_fraiseql_type(cls, kind="input")  # type: ignore[assignment]
        SchemaRegistry.get_instance().register_type(cls)
        return cls

    return wrap if _cls is None else wrap(_cls)
