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

from fraiseql.config.schema_config import SchemaConfig
from fraiseql.core.graphql_type import (
    convert_type_to_graphql_input,
    convert_type_to_graphql_output,
)
from fraiseql.gql.enum_serializer import wrap_resolver_with_enum_serialization
from fraiseql.mutations.decorators import resolve_union_annotation
from fraiseql.types.coercion import wrap_resolver_with_input_coercion
from fraiseql.utils.naming import snake_to_camel

if TYPE_CHECKING:
    from collections.abc import Callable


class SchemaRegistry:
    """Singleton registry for GraphQL query types and mutation resolvers."""

    _instance = None

    def clear(self) -> None:
        """Clear all registries and caches.

        This method clears:
        - SchemaRegistry's internal types and mutations
        - SchemaConfig settings
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
        self._queries.clear()
        self._subscriptions.clear()
        self._enums.clear()
        self._interfaces.clear()
        logger.debug("Registry after clearing: %s", list(self._types.keys()))

        # Clear mutation decorator registries
        from fraiseql.mutations.decorators import clear_mutation_registries

        clear_mutation_registries()

        # Reset SchemaConfig to defaults
        SchemaConfig.reset()

        # Clear GraphQL type cache since field names might change
        from fraiseql.core.graphql_type import _graphql_type_cache

        _graphql_type_cache.clear()

    def __init__(self) -> None:
        """Initialize empty registries for types, mutations, enums, and interfaces."""
        self._types: dict[type, type] = {}
        self._mutations: dict[str, Callable[..., Any]] = {}
        self._queries: dict[str, Callable[..., Any]] = {}
        self._subscriptions: dict[str, Callable[..., Any]] = {}
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
            logger.debug("Enum '%s' is already registered in the schema.", enum_cls.__name__)
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
            logger.debug("Registering interface '%s' to the schema.", interface_cls.__name__)

        self._interfaces[interface_cls] = interface_cls

    def deregister(self, typename: str) -> None:
        """Deregister a type by its name to avoid name conflicts in subsequent tests."""
        # If the type is in the registry, remove it
        types_to_remove = [key for key, value in self._types.items() if value.__name__ == typename]
        for key in types_to_remove:
            del self._types[key]
            logger.debug("Deregistered type '%s' from the schema.", typename)

    def register_mutation(self, mutation_or_fn: type | Callable[..., Any]) -> None:
        """Register a mutation class or resolver function as a GraphQL mutation.

        Args:
            mutation_or_fn: The mutation class or resolver function to register.
        """
        if hasattr(mutation_or_fn, "__fraiseql_mutation__"):
            # Check if it's a simple function-based mutation
            if (
                hasattr(mutation_or_fn, "__fraiseql_resolver__")
                and mutation_or_fn.__fraiseql_resolver__ is mutation_or_fn
            ):
                # It's a function-based mutation decorated with @mutation
                self._mutations[mutation_or_fn.__name__] = mutation_or_fn
            else:
                # It's a @mutation decorated class
                # Register the resolver function
                resolver_fn = mutation_or_fn.__fraiseql_resolver__
                self._mutations[resolver_fn.__name__] = resolver_fn
                # Also register the success and error types
                definition = mutation_or_fn.__fraiseql_mutation__
                if hasattr(definition, "success_type") and definition.success_type:
                    self.register_type(definition.success_type)
                if hasattr(definition, "error_type") and definition.error_type:
                    self.register_type(definition.error_type)
        else:
            # Legacy: direct resolver function
            self._mutations[mutation_or_fn.__name__] = mutation_or_fn

    def register_query(self, query_fn: Callable[..., Any]) -> None:
        """Register a query function as a GraphQL field."""
        self._queries[query_fn.__name__] = query_fn

    def register_subscription(self, subscription_fn: Callable[..., Any]) -> None:
        """Register a subscription function as a GraphQL field."""
        self._subscriptions[subscription_fn.__name__] = subscription_fn

    def build_query_type(self) -> GraphQLObjectType:
        """Build the root Query GraphQLObjectType from registered types and query functions."""
        fields: dict[str, GraphQLField] = {}

        # First, handle query functions if any are registered
        for name, fn in self._queries.items():
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
            def create_gql_resolver(fn):
                import asyncio

                if asyncio.iscoroutinefunction(fn):

                    async def async_resolver(root, info, **kwargs):
                        # Call the original function without the root argument
                        return await fn(info, **kwargs)

                    return async_resolver

                def sync_resolver(root, info, **kwargs):
                    # Call the original function without the root argument
                    return fn(info, **kwargs)

                return sync_resolver

            wrapped_resolver = create_gql_resolver(fn)
            wrapped_resolver = wrap_resolver_with_enum_serialization(wrapped_resolver)

            # Convert field name to camelCase if configured
            config = SchemaConfig.get_instance()
            graphql_field_name = snake_to_camel(name) if config.camel_case_fields else name

            fields[graphql_field_name] = GraphQLField(
                type_=cast("GraphQLOutputType", gql_return_type),
                args=gql_args,
                resolve=wrapped_resolver,
            )

        # Then, check for legacy QueryRoot type pattern
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

            # First check for @field decorated methods
            for attr_name in dir(typ):
                attr = getattr(typ, attr_name)
                if callable(attr) and hasattr(attr, "__fraiseql_field__"):
                    # This is a @field decorated method
                    import inspect

                    sig = inspect.signature(attr)
                    return_type = sig.return_annotation
                    if return_type == inspect.Signature.empty:
                        logger.warning("Field method %s missing return type annotation", attr_name)
                        continue

                    logger.debug("Found @field decorated method: %s", attr_name)
                    gql_type = convert_type_to_graphql_output(return_type)

                    # Get the bound method from the instance
                    bound_method = getattr(query_instance, attr_name)

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
                    field_count += 1

            # Then check regular fields
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
                field_count += 1

            if field_count == 0:
                logger.warning("No fields were added from QueryRoot: %s", typ.__name__)

        if not fields:
            msg = "Type Query must define one or more fields."
            raise TypeError(msg)

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

    def build_subscription_type(self: SchemaRegistry) -> GraphQLObjectType:
        """Build the root Subscription GraphQLObjectType from registered subscriptions."""
        fields = {}

        for name, fn in self._subscriptions.items():
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

            # Create a wrapper that adapts the GraphQL subscription signature
            def make_subscription(fn):
                async def subscribe(root, info, **kwargs):
                    # Call the original function without the root argument
                    async for value in fn(info, **kwargs):
                        yield value

                return subscribe

            wrapped_resolver = make_subscription(fn)

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

    def build_schema(self) -> GraphQLSchema:
        """Build the complete GraphQL schema from registered types and mutations."""
        # Check if there are any mutations registered
        mutation_type = None
        if self._mutations:
            mutation_type = self.build_mutation_type()

        # Check if there are any subscriptions registered
        subscription_type = None
        if self._subscriptions:
            subscription_type = self.build_subscription_type()

        # Collect all types that should be included in the schema
        all_types = []
        for typ in self._types.values():
            # Skip QueryRoot - it's special and its fields are added to Query type
            if typ.__name__ == "QueryRoot":
                continue

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
            subscription=subscription_type,
            types=all_types if all_types else None,
        )

    def build_schema_string(self) -> str:
        """Build the GraphQL schema and return it as a string."""
        from graphql import print_schema

        schema = self.build_schema()
        return print_schema(schema)


def build_fraiseql_schema(
    *,
    query_types: list[type | Callable[..., Any]] | None = None,
    mutation_resolvers: list[type | Callable[..., Any]] | None = None,
    subscription_resolvers: list[Callable[..., Any]] | None = None,
    camel_case_fields: bool = True,
) -> GraphQLSchema:
    """Compose a full GraphQL schema from query types, mutation resolvers, and subscriptions.

    Args:
        query_types: Optional list of Python types or query functions to register.
        mutation_resolvers: Optional list of mutation classes or resolver functions.
        subscription_resolvers: Optional list of subscription functions to register.
        camel_case_fields: Whether to convert snake_case field names to camelCase in GraphQL schema.

    Returns:
        A GraphQLSchema combining the registered query, mutation, and subscription types.
    """
    if mutation_resolvers is None:
        mutation_resolvers = []
    if query_types is None:
        query_types = []
    if subscription_resolvers is None:
        subscription_resolvers = []

    # Set the camelCase configuration
    SchemaConfig.set_config(camel_case_fields=camel_case_fields)

    # Clear GraphQL type cache since field names might change
    from fraiseql.core.graphql_type import _graphql_type_cache

    _graphql_type_cache.clear()

    registry = SchemaRegistry.get_instance()

    for typ in query_types:
        if callable(typ) and not isinstance(typ, type):
            # It's a query function
            registry.register_query(typ)
        else:
            # It's a type
            registry.register_type(typ)

    for fn in mutation_resolvers:
        registry.register_mutation(fn)

    for fn in subscription_resolvers:
        registry.register_subscription(fn)

    # Only add mutation type if there are mutations
    mutation_type = None
    # Check both passed-in mutations and auto-registered ones
    if mutation_resolvers or registry._mutations:
        mutation_type = registry.build_mutation_type()

    # Only add subscription type if there are subscriptions
    subscription_type = None
    if subscription_resolvers:
        subscription_type = registry.build_subscription_type()

    # Collect all types that should be included in the schema
    # This includes types that implement interfaces
    all_types = []
    for typ in registry._types.values():
        # Skip QueryRoot - it's special and its fields are added to Query type
        if typ.__name__ == "QueryRoot":
            continue

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
        subscription=subscription_type,
        types=all_types if all_types else None,
    )
