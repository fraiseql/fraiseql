"""Mutation type builder for GraphQL schema."""

from __future__ import annotations

import logging
from types import MappingProxyType
from typing import TYPE_CHECKING, cast, get_type_hints

from graphql import (
    GraphQLArgument,
    GraphQLField,
    GraphQLNonNull,
    GraphQLObjectType,
    GraphQLOutputType,
)

from fraiseql.config.schema_config import SchemaConfig
from fraiseql.core.graphql_type import convert_type_to_graphql_input, convert_type_to_graphql_output
from fraiseql.mutations.decorators import resolve_union_annotation
from fraiseql.types.coercion import wrap_resolver_with_input_coercion
from fraiseql.utils.naming import snake_to_camel

if TYPE_CHECKING:
    from fraiseql.gql.builders.registry import SchemaRegistry

logger = logging.getLogger(__name__)


class MutationTypeBuilder:
    """Builds the Mutation type from registered mutation resolvers."""

    def __init__(self, registry: SchemaRegistry) -> None:
        """Initialize with a schema registry.

        Args:
            registry: The schema registry containing registered mutations.
        """
        self.registry = registry

    def build(self) -> GraphQLObjectType:
        """Build the root Mutation GraphQLObjectType from registered resolvers.

        Returns:
            The Mutation GraphQLObjectType.
        """
        fields = {}

        for name, fn in self.registry.mutations.items():
            hints = get_type_hints(fn)

            if "return" not in hints:
                msg = f"Mutation resolver '{name}' is missing a return type annotation."
                raise TypeError(msg)

            # Normalize return annotation (e.g., Annotated[Union[A, B], ...])
            resolved = resolve_union_annotation(hints["return"])
            fn.__annotations__["return"] = resolved  # override with resolved union

            # Use convert_type_to_graphql_output for the return type
            gql_return_type = convert_type_to_graphql_output(cast("type", resolved))
            gql_args: dict[str, GraphQLArgument] = {}
            # Track mapping from GraphQL arg names to Python param names
            arg_name_mapping: dict[str, str] = {}

            # Detect argument (usually just one input arg + info)
            for param_name, param_type in hints.items():
                if param_name in {"info", "root", "return"}:
                    continue
                # Use convert_type_to_graphql_input for input arguments
                gql_input_type = convert_type_to_graphql_input(param_type)
                # Convert argument name to camelCase if configured
                config = SchemaConfig.get_instance()
                graphql_arg_name = (
                    snake_to_camel(param_name) if config.camel_case_fields else param_name
                )

                # Special handling for Python reserved words that have trailing underscore
                if param_name.endswith("_") and graphql_arg_name == param_name:
                    # Remove trailing underscore for GraphQL (e.g., id_ -> id, class_ -> class)
                    graphql_arg_name = param_name.rstrip("_")

                gql_args[graphql_arg_name] = GraphQLArgument(GraphQLNonNull(gql_input_type))
                # Store mapping from GraphQL name to Python name
                arg_name_mapping[graphql_arg_name] = param_name

            resolver = self._wrap_mutation_resolver(fn, arg_name_mapping)

            # Convert field name to camelCase if configured
            config = SchemaConfig.get_instance()
            graphql_field_name = snake_to_camel(name) if config.camel_case_fields else name

            fields[graphql_field_name] = GraphQLField(
                type_=cast("GraphQLOutputType", gql_return_type),
                args=gql_args,
                resolve=resolver,
            )

        return GraphQLObjectType(name="Mutation", fields=MappingProxyType(fields))

    def _wrap_mutation_resolver(self, fn, arg_name_mapping: dict[str, str] | None = None):
        """Wrap a mutation function with argument mapping and input coercion.

        Args:
            fn: The mutation function to wrap.
            arg_name_mapping: Mapping from GraphQL argument names to Python parameter names.

        Returns:
            A wrapped resolver function.
        """
        # First wrap with input coercion
        coerced_fn = wrap_resolver_with_input_coercion(fn)

        # Then wrap with argument mapping
        import asyncio

        if asyncio.iscoroutinefunction(coerced_fn):

            async def async_resolver(root, info, **kwargs):
                # Map GraphQL argument names to Python parameter names
                if arg_name_mapping:
                    mapped_kwargs = {}
                    for gql_name, value in kwargs.items():
                        python_name = arg_name_mapping.get(gql_name, gql_name)
                        mapped_kwargs[python_name] = value
                    kwargs = mapped_kwargs

                return await coerced_fn(root, info, **kwargs)

            return async_resolver

        def sync_resolver(root, info, **kwargs):
            # Map GraphQL argument names to Python parameter names
            if arg_name_mapping:
                mapped_kwargs = {}
                for gql_name, value in kwargs.items():
                    python_name = arg_name_mapping.get(gql_name, gql_name)
                    mapped_kwargs[python_name] = value
                kwargs = mapped_kwargs

            return coerced_fn(root, info, **kwargs)

        return sync_resolver
