"""Auto-discovery orchestration for AutoFraiseQL.

This module provides the main AutoDiscovery class that orchestrates
the complete discovery pipeline from PostgreSQL metadata to GraphQL schema.
"""

import logging
from typing import Any, Callable, Dict, List, Type

import psycopg_pool

from .input_generator import InputGenerator
from .metadata_parser import MetadataParser
from .mutation_generator import MutationGenerator
from .postgres_introspector import FunctionMetadata, PostgresIntrospector, ViewMetadata
from .query_generator import QueryGenerator
from .type_generator import TypeGenerator
from .type_mapper import TypeMapper

logger = logging.getLogger(__name__)


class AutoDiscovery:
    """Orchestrate auto-discovery from PostgreSQL metadata to GraphQL schema."""

    def __init__(self, connection_pool: psycopg_pool.AsyncConnectionPool):
        """Initialize AutoDiscovery with database connection pool."""
        self.connection_pool = connection_pool

        # Initialize components
        self.introspector = PostgresIntrospector(connection_pool)
        self.metadata_parser = MetadataParser()
        self.type_mapper = TypeMapper()
        self.type_generator = TypeGenerator(self.type_mapper)
        self.input_generator = InputGenerator(self.type_mapper)
        self.query_generator = QueryGenerator()
        self.mutation_generator = MutationGenerator(self.input_generator)

        # Registry for generated types
        self.type_registry: Dict[str, Type] = {}

    async def discover_all(
        self,
        view_pattern: str = "v_%",
        function_pattern: str = "fn_%",
        schemas: List[str] | None = None,
    ) -> Dict[str, List[Any]]:
        """Full discovery pipeline.

        Args:
            view_pattern: Pattern for view discovery (default: "v_%")
            function_pattern: Pattern for function discovery (default: "fn_%")
            schemas: List of schemas to search (default: ["public"])

        Returns:
            Dictionary with discovered components:
            {
                'types': [User, Post, ...],
                'queries': [user, users, ...],
                'mutations': [createUser, ...],
            }
        """
        if schemas is None:
            schemas = ["public"]

        logger.info(f"Starting auto-discovery in schemas: {schemas}")

        # 1. Discover database objects
        views = await self.introspector.discover_views(pattern=view_pattern, schemas=schemas)
        functions = await self.introspector.discover_functions(
            pattern=function_pattern, schemas=schemas
        )

        logger.info(f"Discovered {len(views)} views and {len(functions)} functions")

        # 2. Parse metadata and generate types
        types = []
        for view in views:
            type_class = await self._generate_type_from_view(view)
            if type_class:
                types.append(type_class)

        # 3. Generate queries for types
        queries = []
        for type_class in types:
            type_queries = self._generate_queries_for_type(type_class)
            queries.extend(type_queries)

        # 4. Generate mutations from functions
        mutations = []
        for function in functions:
            mutation = await self._generate_mutation_from_function(function)
            if mutation:
                mutations.append(mutation)

        logger.info(
            f"Generated {len(types)} types, {len(queries)} queries, {len(mutations)} mutations"
        )

        return {
            "types": types,
            "queries": queries,
            "mutations": mutations,
        }

    async def _generate_type_from_view(self, view_metadata: ViewMetadata) -> Type | None:
        """Generate a type class from view metadata."""
        # Parse @fraiseql:type annotation
        annotation = self.metadata_parser.parse_type_annotation(view_metadata.comment)
        if not annotation:
            return None

        # Generate type class
        try:
            type_class = await self.type_generator.generate_type_class(
                view_metadata, annotation, self.connection_pool
            )

            # Register in type registry
            self.type_registry[type_class.__name__] = type_class

            logger.debug(f"Generated type: {type_class.__name__}")
            return type_class

        except Exception as e:
            logger.warning(f"Failed to generate type from view {view_metadata.view_name}: {e}")
            return None

    def _generate_queries_for_type(self, type_class: Type) -> List[Callable]:
        """Generate standard queries for a type."""
        try:
            # Get view metadata for the type (assuming it's stored in the type)
            # This is a simplified implementation - in practice we'd need to track view metadata
            view_name = getattr(type_class, "__sql_source__", type_class.__name__.lower())
            schema_name = "public"  # Default assumption

            # Create mock annotation for query generation
            from .metadata_parser import TypeAnnotation

            annotation = TypeAnnotation()

            queries = self.query_generator.generate_queries_for_type(
                type_class, view_name, schema_name, annotation
            )

            logger.debug(f"Generated {len(queries)} queries for type: {type_class.__name__}")
            return queries

        except Exception as e:
            logger.warning(f"Failed to generate queries for type {type_class.__name__}: {e}")
            return []

    async def _generate_mutation_from_function(
        self, function_metadata: FunctionMetadata
    ) -> Callable | None:
        """Generate a mutation from function metadata (SpecQL function).

        This method READS function metadata and delegates to MutationGenerator.
        It does NOT create or modify the database.
        """
        # Parse @fraiseql:mutation annotation (SpecQL adds this)
        annotation = self.metadata_parser.parse_mutation_annotation(function_metadata.comment)
        if not annotation:
            return None

        # Generate mutation (READS composite type from DB)
        try:
            mutation = await self.mutation_generator.generate_mutation_for_function(
                function_metadata,
                annotation,
                self.type_registry,
                self.introspector,  # ADD THIS: Pass introspector for composite type discovery
            )

            if mutation:
                logger.debug(f"Generated mutation: {mutation.__name__}")
            return mutation

        except Exception as e:
            logger.warning(
                f"Failed to generate mutation from function {function_metadata.function_name}: {e}"
            )
            return None
