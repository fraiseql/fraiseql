"""Schema composer for building complete GraphQL schemas."""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING

from graphql import GraphQLObjectType, GraphQLSchema

from fraiseql.core.graphql_type import convert_type_to_graphql_output
from fraiseql.gql.builders.mutation_builder import MutationTypeBuilder
from fraiseql.gql.builders.query_builder import QueryTypeBuilder
from fraiseql.gql.builders.subscription_builder import SubscriptionTypeBuilder

if TYPE_CHECKING:
    from fraiseql.gql.builders.registry import SchemaRegistry

logger = logging.getLogger(__name__)


class SchemaComposer:
    """Composes a complete GraphQL schema from registered types and resolvers."""

    def __init__(self, registry: SchemaRegistry) -> None:
        """Initialize with a schema registry.

        Args:
            registry: The schema registry containing all registered components.
        """
        self.registry = registry
        self.query_builder = QueryTypeBuilder(registry)
        self.mutation_builder = MutationTypeBuilder(registry)
        self.subscription_builder = SubscriptionTypeBuilder(registry)

    def compose(self) -> GraphQLSchema:
        """Build the complete GraphQL schema from registered types and mutations.

        Returns:
            A complete GraphQLSchema.
        """
        # Build query type (always required)
        query_type = self.query_builder.build()

        # Check if there are any mutations registered
        mutation_type = None
        if self.registry.mutations:
            mutation_type = self.mutation_builder.build()

        # Check if there are any subscriptions registered
        subscription_type = None
        if self.registry.subscriptions:
            subscription_type = self.subscription_builder.build()

        # Collect all types that should be included in the schema
        all_types = self._collect_all_types()

        return GraphQLSchema(
            query=query_type,
            mutation=mutation_type,
            subscription=subscription_type,
            types=all_types if all_types else None,
        )

    def _collect_all_types(self) -> list[GraphQLObjectType]:
        """Collect all types that should be included in the schema.

        Returns:
            List of GraphQLObjectType instances.
        """
        all_types = []
        for typ in self.registry.types.values():
            # Skip QueryRoot - it's special and its fields are added to Query type
            if typ.__name__ == "QueryRoot":
                continue

            definition = getattr(typ, "__fraiseql_definition__", None)
            if definition and definition.kind in ("type", "output"):
                # Convert to GraphQL type to ensure it's in the schema
                gql_type = convert_type_to_graphql_output(typ)
                if isinstance(gql_type, GraphQLObjectType):
                    all_types.append(gql_type)

        return all_types
