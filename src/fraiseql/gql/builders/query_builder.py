"""Query type builder for GraphQL schema."""

from __future__ import annotations

import asyncio
import logging
from types import MappingProxyType
from typing import TYPE_CHECKING, Any, cast, get_type_hints

from graphql import (
    GraphQLArgument,
    GraphQLField,
    GraphQLObjectType,
    GraphQLOutputType,
    GraphQLResolveInfo,
)

from fraiseql.config.schema_config import SchemaConfig
from fraiseql.core.graphql_type import convert_type_to_graphql_input, convert_type_to_graphql_output
from fraiseql.gql.enum_serializer import wrap_resolver_with_enum_serialization
from fraiseql.utils.naming import snake_to_camel

if TYPE_CHECKING:
    from fraiseql.gql.builders.registry import SchemaRegistry

logger = logging.getLogger(__name__)


class QueryTypeBuilder:
    """Builds the Query type from registered query functions and types."""

    def __init__(self, registry: SchemaRegistry) -> None:
        """Initialize with a schema registry.

        Args:
            registry: The schema registry containing registered types and queries.
        """
        self.registry = registry

    def build(self) -> GraphQLObjectType:
        """Build the root Query GraphQLObjectType from registered types and query functions.

        Returns:
            The Query GraphQLObjectType.

        Raises:
            TypeError: If no fields are defined for the Query type.
        """
        fields: dict[str, GraphQLField] = {}

        # First, handle query functions if any are registered
        self._add_query_functions(fields)

        # Then, check for legacy QueryRoot type pattern
        self._add_query_root_fields(fields)

        if not fields:
            msg = "Type Query must define one or more fields."
            raise TypeError(msg)

        return GraphQLObjectType(name="Query", fields=MappingProxyType(fields))

    def _add_query_functions(self, fields: dict[str, GraphQLField]) -> None:
        """Add registered query functions to the fields dictionary.

        Args:
            fields: The fields dictionary to populate.
        """
        for name, fn in self.registry.queries.items():
            hints = get_type_hints(fn)

            if "return" not in hints:
                msg = f"Query function '{name}' is missing a return type annotation."
                raise TypeError(msg)

            # Use convert_type_to_graphql_output for the return type
            gql_return_type = convert_type_to_graphql_output(hints["return"])
            logger.debug(
                "Query %s: return type %s converted to %s",
                name,
                hints["return"],
                gql_return_type,
            )
            gql_args: dict[str, GraphQLArgument] = {}

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
                gql_args[graphql_arg_name] = GraphQLArgument(gql_input_type)

            # Create a wrapper that adapts the GraphQL resolver signature
            wrapped_resolver = self._create_gql_resolver(fn)
            wrapped_resolver = wrap_resolver_with_enum_serialization(wrapped_resolver)

            # Convert field name to camelCase if configured
            config = SchemaConfig.get_instance()
            graphql_field_name = snake_to_camel(name) if config.camel_case_fields else name

            fields[graphql_field_name] = GraphQLField(
                type_=cast("GraphQLOutputType", gql_return_type),
                args=gql_args,
                resolve=wrapped_resolver,
            )

    def _create_gql_resolver(self, fn):
        """Create a GraphQL resolver from a function.

        Args:
            fn: The function to wrap as a GraphQL resolver.

        Returns:
            A GraphQL-compatible resolver function.
        """
        if asyncio.iscoroutinefunction(fn):

            async def async_resolver(root, info, **kwargs):
                # Call the original function without the root argument
                return await fn(info, **kwargs)

            return async_resolver

        def sync_resolver(root, info, **kwargs):
            # Call the original function without the root argument
            return fn(info, **kwargs)

        return sync_resolver

    def _add_query_root_fields(self, fields: dict[str, GraphQLField]) -> None:
        """Add fields from QueryRoot type if it exists.

        Args:
            fields: The fields dictionary to populate.
        """
        for typ in self.registry.types:
            definition = getattr(typ, "__fraiseql_definition__", None)
            if definition is None:
                continue

            kind = getattr(definition, "kind", None)
            if kind != "type":
                continue

            if typ.__name__ != "QueryRoot":
                continue

            query_instance = typ()
            field_count = 0

            # First check for @field decorated methods
            self._add_field_decorated_methods(typ, query_instance, fields)

            # Then check regular fields
            self._add_regular_fields(definition, query_instance, fields)

            if field_count == 0:
                logger.warning("No fields were added from QueryRoot: %s", typ.__name__)

    def _add_field_decorated_methods(
        self,
        typ: type,
        instance: Any,
        fields: dict[str, GraphQLField],
    ) -> None:
        """Add @field decorated methods to the fields dictionary.

        Args:
            typ: The type class.
            instance: An instance of the type.
            fields: The fields dictionary to populate.
        """
        import inspect

        for attr_name in dir(typ):
            attr = getattr(typ, attr_name)
            if callable(attr) and hasattr(attr, "__fraiseql_field__"):
                # This is a @field decorated method
                sig = inspect.signature(attr)
                return_type = sig.return_annotation
                if return_type == inspect.Signature.empty:
                    logger.warning("Field method %s missing return type annotation", attr_name)
                    continue

                logger.debug("Found @field decorated method: %s", attr_name)
                gql_type = convert_type_to_graphql_output(return_type)

                # Get the bound method from the instance
                bound_method = getattr(instance, attr_name)

                # The bound method should already have the wrapped resolver from the decorator
                wrapped_resolver = wrap_resolver_with_enum_serialization(bound_method)

                # Convert field name to camelCase if configured
                config = SchemaConfig.get_instance()
                graphql_field_name = (
                    snake_to_camel(attr_name) if config.camel_case_fields else attr_name
                )

                fields[graphql_field_name] = GraphQLField(
                    type_=cast("GraphQLOutputType", gql_type),
                    resolve=wrapped_resolver,
                    description=getattr(attr, "__fraiseql_field_description__", None),
                )

    def _add_regular_fields(
        self,
        definition: Any,
        instance: Any,
        fields: dict[str, GraphQLField],
    ) -> None:
        """Add regular fields from type definition to the fields dictionary.

        Args:
            definition: The type definition containing field information.
            instance: An instance of the type.
            fields: The fields dictionary to populate.
        """
        for field_name, field_def in definition.fields.items():
            logger.debug("Field '%s' definition: %s", field_name, field_def)
            if field_def.purpose not in {"output", "both"}:
                logger.debug(
                    "Skipping field '%s' because its purpose is not 'output' or 'both'.",
                    field_name,
                )
                continue

            logger.debug("Adding field '%s' to the QueryRoot fields", field_name)

            gql_type = convert_type_to_graphql_output(field_def.field_type)
            resolver = getattr(instance, f"resolve_{field_name}", None)

            # Wrap resolver if it exists
            if resolver is not None:
                resolver = wrap_resolver_with_enum_serialization(resolver)

            if resolver is None:
                logger.warning(
                    "No resolver found for '%s', falling back to attribute lookup",
                    field_name,
                )

                def make_resolver(instance: Any, field: str) -> Any:
                    def _resolver(_: Any, __: GraphQLResolveInfo) -> Any:
                        return getattr(instance, field, None)

                    return _resolver

                resolver = make_resolver(instance, field_name)

            # Wrap resolver to handle enum serialization
            wrapped_resolver = wrap_resolver_with_enum_serialization(resolver)

            # Convert field name to camelCase if configured
            config = SchemaConfig.get_instance()
            graphql_field_name = (
                snake_to_camel(field_name) if config.camel_case_fields else field_name
            )

            fields[graphql_field_name] = GraphQLField(
                type_=cast("GraphQLOutputType", gql_type),
                resolve=wrapped_resolver,
                description=field_def.description,
            )
