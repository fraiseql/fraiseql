"""Subscription type builder for GraphQL schema."""

from __future__ import annotations

import logging
from types import MappingProxyType
from typing import TYPE_CHECKING, Any, cast, get_type_hints

from graphql import (
    GraphQLArgument,
    GraphQLField,
    GraphQLObjectType,
    GraphQLOutputType,
)

from fraiseql.config.schema_config import SchemaConfig
from fraiseql.core.graphql_type import convert_type_to_graphql_input, convert_type_to_graphql_output
from fraiseql.utils.naming import snake_to_camel

if TYPE_CHECKING:
    from fraiseql.gql.builders.registry import SchemaRegistry

logger = logging.getLogger(__name__)


class SubscriptionTypeBuilder:
    """Builds the Subscription type from registered subscription resolvers."""

    def __init__(self, registry: SchemaRegistry) -> None:
        """Initialize with a schema registry.

        Args:
            registry: The schema registry containing registered subscriptions.
        """
        self.registry = registry

    def build(self) -> GraphQLObjectType:
        """Build the root Subscription GraphQLObjectType from registered subscriptions.

        Returns:
            The Subscription GraphQLObjectType.
        """
        fields = {}

        for name, fn in self.registry.subscriptions.items():
            hints = get_type_hints(fn)

            if "return" not in hints:
                msg = f"Subscription '{name}' is missing a return type annotation."
                raise TypeError(msg)

            # Extract yield type from AsyncGenerator
            return_type = hints["return"]
            yield_type = return_type.__args__[0] if hasattr(return_type, "__args__") else Any

            # Use convert_type_to_graphql_output for the yield type
            gql_return_type = convert_type_to_graphql_output(yield_type)
            gql_args: dict[str, GraphQLArgument] = {}
            # Track mapping from GraphQL arg names to Python param names
            arg_name_mapping: dict[str, str] = {}

            # Detect arguments (excluding 'info' and 'root')
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

                gql_args[graphql_arg_name] = GraphQLArgument(gql_input_type)
                # Store mapping from GraphQL name to Python name
                arg_name_mapping[graphql_arg_name] = param_name

            # Create a wrapper that adapts the GraphQL subscription signature
            wrapped_resolver = self._make_subscription(fn, arg_name_mapping)

            # Convert field name to camelCase if configured
            config = SchemaConfig.get_instance()
            graphql_field_name = snake_to_camel(name) if config.camel_case_fields else name

            fields[graphql_field_name] = GraphQLField(
                type_=cast("GraphQLOutputType", gql_return_type),
                args=gql_args,
                subscribe=wrapped_resolver,
                resolve=lambda value, info, **kwargs: value,  # Pass through the yielded value
            )

        return GraphQLObjectType(name="Subscription", fields=MappingProxyType(fields))

    def _make_subscription(self, fn, arg_name_mapping: dict[str, str] | None = None):
        """Create a GraphQL subscription from an async generator function.

        Args:
            fn: The async generator function to wrap as a GraphQL subscription.
            arg_name_mapping: Mapping from GraphQL argument names to Python parameter names.

        Returns:
            A GraphQL-compatible subscription function.
        """

        async def subscribe(root, info, **kwargs):
            # Map GraphQL argument names to Python parameter names
            if arg_name_mapping:
                mapped_kwargs = {}
                for gql_name, value in kwargs.items():
                    python_name = arg_name_mapping.get(gql_name, gql_name)
                    mapped_kwargs[python_name] = value
                kwargs = mapped_kwargs

            # Call the original function without the root argument
            async for value in fn(info, **kwargs):
                yield value

        return subscribe
