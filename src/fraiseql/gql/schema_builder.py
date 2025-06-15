"""GraphQL schema builder managing type and mutation registrations.

Provides a singleton registry to collect query types and mutation resolvers,
and builds the corresponding GraphQLObjectType instances for the schema.

Typical usage involves registering decorated Python types and resolver
functions, then composing a complete GraphQLSchema for your API.
"""

from __future__ import annotations

import logging
from types import MappingProxyType
from typing import TYPE_CHECKING, Any, cast, get_type_hints

logger = logging.getLogger(__name__)

from graphql import (
    GraphQLArgument,
    GraphQLEnumType,
    GraphQLField,
    GraphQLNonNull,
    GraphQLObjectType,
    GraphQLOutputType,
    GraphQLResolveInfo,
    GraphQLSchema,
)

from fraiseql.core.graphql_type import (
    convert_type_to_graphql_input,
    convert_type_to_graphql_output,
)
from fraiseql.gql.enum_serializer import wrap_resolver_with_enum_serialization
from fraiseql.mutations.decorators import resolve_union_annotation
from fraiseql.types.coercion import wrap_resolver_with_input_coercion

if TYPE_CHECKING:
    from collections.abc import Callable


class SchemaRegistry:
    """Singleton registry for GraphQL query types and mutation resolvers."""

    _instance = None

    def clear(self) -> None:
        """Clear all registries and caches.

        This method clears:
        - SchemaRegistry's internal types and mutations
        - Registered enums
        - Mutation decorator registries (_success_registry, _failure_registry, _union_registry)
        - Any other internal caches to ensure clean state between tests

        This is particularly useful in test environments to prevent type name
        conflicts and ensure each test starts with a fresh registry state.
        """
        logger.debug("Clearing the registry...")
        # Clear SchemaRegistry's own registries
        self._types.clear()
        self._mutations.clear()
        self._enums.clear()
        self._interfaces.clear()
        logger.debug("Registry after clearing: %s", list(self._types.keys()))

        # Clear mutation decorator registries
        from fraiseql.mutations.decorators import clear_mutation_registries

        clear_mutation_registries()

    def __init__(self) -> None:
        """Initialize empty registries for types, mutations, enums, and interfaces."""
        self._types: dict[type, type] = {}
        self._mutations: dict[str, Callable[..., Any]] = {}
        self._enums: dict[type, GraphQLEnumType] = {}
        self._interfaces: dict[type, type] = {}

    @classmethod
    def get_instance(cls) -> SchemaRegistry:
        """Get or create the singleton registry instance."""
        if cls._instance is None:
            cls._instance = cls()
        return cls._instance

    def register_type(self, typ: type) -> None:
        """Register a Python type as a GraphQL query type.

        Args:
            typ: The decorated Python type to register.
        """
        # Debugging: Check if the type is already in the registry
        if typ in self._types:
            logger.debug("Type '%s' is already registered in the schema.", typ.__name__)
        else:
            logger.debug("Registering type '%s' to the schema.", typ.__name__)

        # Register the type if it's not already present
        self._types[typ] = typ
        logger.debug("Current registry: %s", list(self._types.keys()))

    def register_enum(self, enum_cls: type, graphql_enum: GraphQLEnumType) -> None:
        """Register a Python Enum class as a GraphQL enum type.

        Args:
            enum_cls: The Python Enum class decorated with @fraise_enum.
            graphql_enum: The corresponding GraphQL enum type.
        """
        if enum_cls in self._enums:
            logger.debug(
                "Enum '%s' is already registered in the schema.", enum_cls.__name__
            )
        else:
            logger.debug("Registering enum '%s' to the schema.", enum_cls.__name__)

        self._enums[enum_cls] = graphql_enum

    def register_interface(self, interface_cls: type) -> None:
        """Register a Python class as a GraphQL interface type.

        Args:
            interface_cls: The Python class decorated with @fraise_interface.
        """
        if interface_cls in self._interfaces:
            logger.debug(
                "Interface '%s' is already registered in the schema.",
                interface_cls.__name__,
            )
        else:
            logger.debug(
                "Registering interface '%s' to the schema.", interface_cls.__name__
            )

        self._interfaces[interface_cls] = interface_cls

    def deregister(self, typename: str) -> None:
        """Deregister a type by its name to avoid name conflicts in subsequent tests."""
        # If the type is in the registry, remove it
        types_to_remove = [
            key for key, value in self._types.items() if value.__name__ == typename
        ]
        for key in types_to_remove:
            del self._types[key]
            logger.debug("Deregistered type '%s' from the schema.", typename)

    def register_mutation(self, mutation_or_fn: type | Callable[..., Any]) -> None:
        """Register a mutation class or resolver function as a GraphQL mutation.

        Args:
            mutation_or_fn: The mutation class or resolver function to register.
        """
        if hasattr(mutation_or_fn, "__fraiseql_mutation__"):
            # It's a @mutation decorated class
            # Register the resolver function
            resolver_fn = mutation_or_fn.__fraiseql_resolver__
            self._mutations[resolver_fn.__name__] = resolver_fn
            # Also register the success and error types
            definition = mutation_or_fn.__fraiseql_mutation__
            if definition.success_type:
                self.register_type(definition.success_type)
            if definition.error_type:
                self.register_type(definition.error_type)
        else:
            # Legacy: direct resolver function
            self._mutations[mutation_or_fn.__name__] = mutation_or_fn

    def build_query_type(self) -> GraphQLObjectType:
        """Build the root Query GraphQLObjectType from registered types."""
        fields: dict[str, GraphQLField] = {}

        for typ in self._types:
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
                resolver = getattr(query_instance, f"resolve_{field_name}", None)

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

                    resolver = make_resolver(query_instance, field_name)

                # Wrap resolver to handle enum serialization
                wrapped_resolver = wrap_resolver_with_enum_serialization(resolver)

                fields[field_name] = GraphQLField(
                    type_=cast(GraphQLOutputType, gql_type),
                    resolve=wrapped_resolver,
                    description=field_def.description,
                )
                field_count += 1

            if field_count == 0:
                logger.warning("No fields were added from QueryRoot: %s", typ.__name__)

        if not fields:
            raise TypeError("Type Query must define one or more fields.")

        return GraphQLObjectType(name="Query", fields=MappingProxyType(fields))

    def build_mutation_type(self: SchemaRegistry) -> GraphQLObjectType:
        """Build the root Mutation GraphQLObjectType from registered resolvers."""
        fields = {}

        for name, fn in self._mutations.items():
            hints = get_type_hints(fn)

            if "return" not in hints:
                msg = f"Mutation resolver '{name}' is missing a return type annotation."
                raise TypeError(msg)

            # Normalize return annotation (e.g., Annotated[Union[A, B], ...])
            resolved = resolve_union_annotation(hints["return"])
            fn.__annotations__["return"] = resolved  # override with resolved union

            # Use convert_type_to_graphql_output for the return type
            gql_return_type = convert_type_to_graphql_output(cast(type, resolved))
            gql_args: dict[str, GraphQLArgument] = {}

            # Detect argument (usually just one input arg + info)
            for param_name, param_type in hints.items():
                if param_name in {"info", "root", "return"}:
                    continue
                # Use convert_type_to_graphql_input for input arguments
                gql_input_type = convert_type_to_graphql_input(param_type)
                gql_args[param_name] = GraphQLArgument(GraphQLNonNull(gql_input_type))

            resolver = wrap_resolver_with_input_coercion(fn)

            fields[name] = GraphQLField(
                type_=cast(GraphQLOutputType, gql_return_type),
                args=gql_args,
                resolve=resolver,
            )

        return GraphQLObjectType(name="Mutation", fields=MappingProxyType(fields))

    def build_schema(self) -> GraphQLSchema:
        """Build the complete GraphQL schema from registered types and mutations."""
        # Check if there are any mutations registered
        mutation_type = None
        if self._mutations:
            mutation_type = self.build_mutation_type()

        # Collect all types that should be included in the schema
        all_types = []
        for typ in self._types.values():
            definition = getattr(typ, "__fraiseql_definition__", None)
            if definition and definition.kind in ("type", "output"):
                # Convert to GraphQL type to ensure it's in the schema
                from fraiseql.core.graphql_type import convert_type_to_graphql_output

                gql_type = convert_type_to_graphql_output(typ)
                if isinstance(gql_type, GraphQLObjectType):
                    all_types.append(gql_type)

        return GraphQLSchema(
            query=self.build_query_type(),
            mutation=mutation_type,
            types=all_types if all_types else None,
        )

    def build_schema_string(self) -> str:
        """Build the GraphQL schema and return it as a string."""
        from graphql import print_schema

        schema = self.build_schema()
        return print_schema(schema)


def build_fraiseql_schema(
    *,
    query_types: list[type] | None = None,
    mutation_resolvers: list[type | Callable[..., Any]] | None = None,
) -> GraphQLSchema:
    """Compose a full GraphQL schema from query types and mutation resolvers.

    Args:
        query_types: Optional list of Python types to register as query types.
        mutation_resolvers: Optional list of mutation classes or resolver functions.

    Returns:
        A GraphQLSchema combining the registered query and mutation types.
    """
    if mutation_resolvers is None:
        mutation_resolvers = []
    if query_types is None:
        query_types = []

    registry = SchemaRegistry.get_instance()

    for typ in query_types:
        registry.register_type(typ)

    for fn in mutation_resolvers:
        registry.register_mutation(fn)

    # Only add mutation type if there are mutations
    mutation_type = None
    if mutation_resolvers:
        mutation_type = registry.build_mutation_type()

    # Collect all types that should be included in the schema
    # This includes types that implement interfaces
    all_types = []
    for typ in registry._types.values():
        definition = getattr(typ, "__fraiseql_definition__", None)
        if definition and definition.kind in ("type", "output"):
            # Convert to GraphQL type to ensure it's in the schema
            from fraiseql.core.graphql_type import convert_type_to_graphql_output

            gql_type = convert_type_to_graphql_output(typ)
            if isinstance(gql_type, GraphQLObjectType):
                all_types.append(gql_type)

    return GraphQLSchema(
        query=registry.build_query_type(),
        mutation=mutation_type,
        types=all_types if all_types else None,
    )
