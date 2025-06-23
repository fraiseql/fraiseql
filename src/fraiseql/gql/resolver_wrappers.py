"""Utility to wrap Python async resolver functions as GraphQLField instances.

Converts function signatures to GraphQL argument definitions and return types,
and provides a resolver that calls the original function with `info` and
keyword arguments.
"""

from collections.abc import Awaitable, Callable
from dataclasses import is_dataclass
from inspect import isclass, signature
from typing import Any, cast

from graphql import (
    GraphQLArgument,
    GraphQLField,
    GraphQLNonNull,
    GraphQLOutputType,
    GraphQLResolveInfo,
)

from fraiseql.core.graphql_type import (
    convert_type_to_graphql_input,
    convert_type_to_graphql_output,
)


def wrap_resolver(fn: Callable[..., Awaitable[object]]) -> GraphQLField:
    """Wrap an async resolver function into a GraphQLField with typed arguments and input coercion."""  # noqa: E501
    sig = signature(fn)
    args: dict[str, GraphQLArgument] = {}

    # Build GraphQL argument definitions
    for name, param in sig.parameters.items():
        if name == "info":
            continue
        gql_input_type = convert_type_to_graphql_input(param.annotation)
        args[name] = GraphQLArgument(GraphQLNonNull(cast("Any", gql_input_type)))

    gql_output_type = convert_type_to_graphql_output(sig.return_annotation)
    gql_output_type_cast = cast("GraphQLOutputType", gql_output_type)

    async def resolver(root: object, info: GraphQLResolveInfo, **kwargs: object) -> object:
        _ = root
        coerced_kwargs: dict[str, object] = {}

        for name, value in kwargs.items():
            param = sig.parameters.get(name)
            expected_type = param.annotation if param else None

            if (
                isinstance(value, dict)
                and expected_type is not None
                and isclass(expected_type)
                and (
                    is_dataclass(expected_type) or hasattr(expected_type, "__fraiseql_definition__")
                )
            ):
                coerced_kwargs[name] = expected_type(**value)
            else:
                coerced_kwargs[name] = value

        return await fn(info=info, **coerced_kwargs)

    return GraphQLField(
        type_=gql_output_type_cast,
        args=args,
        resolve=resolver,
    )
