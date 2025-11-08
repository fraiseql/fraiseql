"""FraiseQL auto-discovery introspection engine.

This module provides automatic discovery of GraphQL schemas from PostgreSQL metadata.
It introspects database views, functions, and comments to generate types, queries, and mutations.
"""

from .auto_discovery import AutoDiscovery
from .input_generator import InputGenerator
from .metadata_parser import MetadataParser
from .mutation_generator import MutationGenerator
from .postgres_introspector import (
    CompositeAttribute,
    CompositeTypeMetadata,
    PostgresIntrospector,
)
from .query_generator import QueryGenerator
from .type_generator import TypeGenerator
from .type_mapper import TypeMapper

__all__ = [
    "AutoDiscovery",
    "CompositeAttribute",
    "CompositeTypeMetadata",
    "InputGenerator",
    "MetadataParser",
    "MutationGenerator",
    "PostgresIntrospector",
    "QueryGenerator",
    "TypeGenerator",
    "TypeMapper",
]
