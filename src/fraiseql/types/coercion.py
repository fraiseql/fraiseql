"""Module for coercing input data into FraiseQL objects based on type hints."""

import inspect
from collections.abc import Callable
from typing import (
    Any,
    Literal,
    Protocol,
    TypeVar,
    Union,
    cast,
    get_args,
    get_origin,
    runtime_checkable,
)

from fraiseql.config.schema_config import SchemaConfig
from fraiseql.fields import FRAISE_MISSING
from fraiseql.utils.fraiseql_builder import collect_fraise_fields
from fraiseql.utils.naming import snake_to_camel

R = TypeVar("R")


class FraiseQLDefinition(Protocol):
    """Missing docstring."""

    kind: Literal["input", "type", "success", "failure"]


@runtime_checkable
class HasFraiseDefinition(Protocol):
    """Missing docstring."""

    __fraiseql_definition__: FraiseQLDefinition


def _coerce_field_value(raw_value: object, field_type: object) -> object:
    """Coerces a single field's raw value based on its type."""
    if raw_value is None:
        return None

    origin = get_origin(field_type)
    args = get_args(field_type)

    # Case 1: direct FraiseQL object
    if isinstance(field_type, HasFraiseDefinition) and field_type.__fraiseql_definition__.kind in {
        "input",
        "type",
        "success",
        "failure",
    }:
        return coerce_input(cast("type", field_type), cast("dict[str, object]", raw_value))

    # Case 2: Union containing a FraiseQL object
    if origin is Union and args:
        for arg in args:
            if isinstance(arg, HasFraiseDefinition) and arg.__fraiseql_definition__.kind in {
                "input",
                "type",
                "success",
                "failure",
            }:
                return coerce_input(cast("type", arg), cast("dict[str, object]", raw_value))

    # Case 3: List of FraiseQL objects
    if origin is list and args and hasattr(args[0], "__fraiseql_definition__"):
        return [
            coerce_input(cast("type", args[0]), cast("dict[str, object]", item))
            for item in cast("list[object]", raw_value)
        ]

    return raw_value


def coerce_input(cls: type, raw: dict[str, object]) -> object:
    """Coerce a dict into a FraiseQL object instance."""
    fields, type_hints = collect_fraise_fields(cls)
    coerced_data: dict[str, object] = {}

    # Get schema config to check if camelCase is enabled
    config = SchemaConfig.get_instance()

    # Create a mapping of potential GraphQL field names to Python field names
    field_mapping = {}
    if config.camel_case_fields:
        for python_name in fields:
            graphql_name = snake_to_camel(python_name)
            field_mapping[graphql_name] = python_name

    for name, field in fields.items():
        # Check if the field exists in raw data (either as snake_case or camelCase)
        raw_key = None
        if name in raw:
            raw_key = name
        elif config.camel_case_fields:
            # Check if the camelCase version exists
            camel_name = snake_to_camel(name)
            if camel_name in raw:
                raw_key = camel_name

        if raw_key is not None:
            coerced_data[name] = _coerce_field_value(raw[raw_key], type_hints.get(name, object))
        elif field.default is not FRAISE_MISSING:
            coerced_data[name] = field.default
        elif field.default_factory is not None:
            coerced_data[name] = field.default_factory()
        else:
            msg = f"Missing required field '{name}' for {cls.__name__}"
            raise ValueError(msg)

    instance = object.__new__(cls)
    for key, value in coerced_data.items():
        setattr(instance, key, value)
    return instance


def coerce_input_arguments(
    fn: Callable[..., object],
    raw_args: dict[str, object],
) -> dict[str, object]:
    """Coerce raw GraphQL resolver args into FraiseQL-typed input objects."""
    signature = inspect.signature(fn)
    coerced: dict[str, object] = {}

    # Get schema config to check if camelCase is enabled
    config = SchemaConfig.get_instance()

    for name, param in signature.parameters.items():
        if name in {"info", "root"}:
            continue

        # Check if the argument exists in raw_args (either as snake_case or camelCase)
        raw_key = None
        if name in raw_args:
            raw_key = name
        elif config.camel_case_fields:
            # Check if the camelCase version exists
            camel_name = snake_to_camel(name)
            if camel_name in raw_args:
                raw_key = camel_name

        if raw_key is None:
            # Don't add the key at all for omitted fields
            # This allows the coerce_input function to use the field's default value
            continue

        raw_value = raw_args[raw_key]

        if raw_value is None:
            coerced[name] = None
            continue

        annotation = param.annotation
        if annotation is inspect.Parameter.empty:
            coerced[name] = raw_value
            continue

        if hasattr(annotation, "__fraiseql_definition__"):
            coerced[name] = coerce_input(annotation, raw_value)  # type: ignore[arg-type]
        else:
            coerced[name] = raw_value

    return coerced


def wrap_resolver_with_input_coercion(
    fn: Callable[..., Any],
) -> Callable[..., Any]:
    """Wrap a GraphQL resolver to coerce input arguments into FraiseQL objects."""
    import asyncio

    if asyncio.iscoroutinefunction(fn):

        async def async_wrapper(root: object, info: object, **kwargs: object) -> Any:
            _ = root
            coerced_args = coerce_input_arguments(fn, kwargs)
            return await fn(info, **coerced_args)

        return async_wrapper

    def sync_wrapper(root: object, info: object, **kwargs: object) -> Any:
        _ = root
        coerced_args = coerce_input_arguments(fn, kwargs)
        return fn(info, **coerced_args)

    return sync_wrapper
