"""FraideQL decorators for mutation result classes and input types."""

import types
from collections.abc import Callable
from typing import (
    Annotated,
    Any,
    TypeVar,
    Union,
    cast,
    dataclass_transform,
    get_args,
    get_origin,
    overload,
)

from fraiseql.fields import fraise_field
from fraiseql.mutations.registry import register_result
from fraiseql.utils.fields import patch_missing_field_types

T = TypeVar("T", bound=type[Any])

_success_registry: dict[str, type] = {}
_failure_registry: dict[str, type] = {}
_union_registry: dict[str, object] = {}


def clear_mutation_registries() -> None:
    """Clear all mutation decorator registries."""
    _success_registry.clear()
    _failure_registry.clear()
    _union_registry.clear()


class FraiseUnion:
    """Metadata wrapper for union result types."""

    def __init__(self, name: str) -> None:
        """Missing docstring."""
        self.name = name


def resolve_union_annotation(annotation: object) -> object:
    """Resolve `Success | Error` into an Annotated union result."""
    origin = get_origin(annotation)
    if origin not in (Union, types.UnionType):
        return annotation

    args = get_args(annotation)
    success = next((a for a in args if getattr(a, "__name__", "").endswith("Success")), None)
    error = next((a for a in args if getattr(a, "__name__", "").endswith("Error")), None)

    if not success or not error:
        return annotation

    base_name = success.__name__.removesuffix("Success")
    union_name = f"{base_name}Result"

    if union_name not in _union_registry:
        _union_registry[union_name] = Annotated[success | error, FraiseUnion(union_name)]

    return _union_registry[union_name]


# ------------------------
# Decorators
# ------------------------


@dataclass_transform(field_specifiers=(fraise_field,))
@overload
def success(_cls: None = None) -> Callable[[T], T]: ...
@overload
def success(_cls: T) -> T: ...


def success(_cls: T | None = None) -> T | Callable[[T], T]:
    """Decorator to define a FraiseQL mutation success type."""

    def wrap(cls: T) -> T:
        from fraiseql.gql.schema_builder import SchemaRegistry
        from fraiseql.types.constructor import define_fraiseql_type

        patch_missing_field_types(cls)
        cls = define_fraiseql_type(cls, kind="output")  # type: ignore[assignment]

        SchemaRegistry.get_instance().register_type(cls)

        _success_registry[cls.__name__] = cls
        _maybe_register_union(cls.__name__)
        return cls

    return wrap if _cls is None else wrap(_cls)


@dataclass_transform(field_specifiers=(fraise_field,))
@overload
def failure(_cls: None = None) -> Callable[[T], T]: ...
@overload
def failure(_cls: T) -> T: ...


def failure(_cls: T | None = None) -> T | Callable[[T], T]:
    """Decorator to define a FraiseQL mutation error type."""

    def wrap(cls: T) -> T:
        from fraiseql.gql.schema_builder import SchemaRegistry
        from fraiseql.types.constructor import define_fraiseql_type

        patch_missing_field_types(cls)
        cls = define_fraiseql_type(cls, kind="output")  # type: ignore[assignment]

        SchemaRegistry.get_instance().register_type(cls)

        _failure_registry[cls.__name__] = cls
        _maybe_register_union(cls.__name__)
        return cls

    return wrap if _cls is None else wrap(_cls)


# ------------------------
# Result Union Utilities
# ------------------------


def _maybe_register_union(_: str) -> None:
    for success_name, success_cls in _success_registry.items():
        error_name = f"{success_name.removesuffix('Success')}Error"
        if error_name in _failure_registry:
            failure_cls = _failure_registry[error_name]
            union_name = f"{success_name}Result"
            if union_name not in _union_registry:
                register_result(success_cls, failure_cls)
                _union_registry[union_name] = Annotated[
                    success_cls | failure_cls,
                    FraiseUnion(union_name),
                ]

    for failure_name, failure_cls in _failure_registry.items():
        success_name = f"{failure_name.removesuffix('Error')}Success"
        if success_name in _success_registry:
            success_cls = _success_registry[success_name]
            union_name = f"{success_name}Result"
            if union_name not in _union_registry:
                register_result(success_cls, failure_cls)
                _union_registry[union_name] = Annotated[
                    success_cls | failure_cls,
                    FraiseUnion(union_name),
                ]


def result(success_cls: type, error_cls: type) -> type:
    """Manually register a success+error result union type."""
    base_name = success_cls.__name__.removesuffix("Success")
    union_name = f"{base_name}Result"

    if union_name in _union_registry:
        return cast("type", _union_registry[union_name])

    register_result(success_cls, error_cls)
    union = Annotated[success_cls | error_cls, FraiseUnion(union_name)]
    _union_registry[union_name] = union
    return cast("type", union)
