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
                gql_args[graphql_arg_name] = GraphQLArgument(GraphQLNonNull(gql_input_type))

            resolver = wrap_resolver_with_input_coercion(fn)

            # Convert field name to camelCase if configured
            config = SchemaConfig.get_instance()
            graphql_field_name = snake_to_camel(name) if config.camel_case_fields else name

            fields[graphql_field_name] = GraphQLField(
                type_=cast("GraphQLOutputType", gql_return_type),
                args=gql_args,
                resolve=resolver,
            )

        return GraphQLObjectType(name="Mutation", fields=MappingProxyType(fields))
