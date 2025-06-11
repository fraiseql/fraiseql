"""GraphQL interface type decorator for FraiseQL.

Interfaces are abstract types that define a set of fields that multiple
object types can implement. This is useful for polymorphic queries.
"""

from collections.abc import Callable
from typing import TypeVar, dataclass_transform, overload

from fraiseql.fields import fraise_field
from fraiseql.types.constructor import define_fraiseql_type
from fraiseql.utils.fields import patch_missing_field_types

T = TypeVar("T", bound=type)


@dataclass_transform(field_specifiers=(fraise_field,))
@overload
def fraise_interface(_cls: None = None) -> Callable[[T], T]: ...
@overload
def fraise_interface(_cls: T) -> T: ...


def fraise_interface(_cls: T | None = None) -> T | Callable[[T], T]:
    """Decorator to mark a class as a GraphQL interface type.

    Interfaces define a contract that implementing types must follow.
    All fields defined in the interface must be present in implementing types.

    Example:
        @fraise_interface
        class Node:
            id: str

        @fraise_type(implements=[Node])
        class User:
            id: str
            name: str
            email: str

        @fraise_type(implements=[Node])
        class Post:
            id: str
            title: str
            content: str
    """

    def wrap(cls: T) -> T:
        from fraiseql.gql.schema_builder import SchemaRegistry

        patch_missing_field_types(cls)
        # Use "interface" as the kind
        cls = define_fraiseql_type(cls, kind="interface")  # type: ignore[assignment,arg-type]
        SchemaRegistry.get_instance().register_interface(cls)
        return cls

    return wrap if _cls is None else wrap(_cls)
