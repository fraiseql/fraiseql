"""FraiseQL auto-discovery introspection engine.

This module provides automatic discovery of GraphQL schemas from PostgreSQL metadata.
It introspects database views, functions, and comments to generate types, queries, and mutations.
"""

from .metadata_parser import MetadataParser
from .postgres_introspector import PostgresIntrospector
from .query_generator import QueryGenerator
from .type_generator import TypeGenerator
from .type_mapper import TypeMapper

__all__ = [
    "MetadataParser",
    "PostgresIntrospector",
    "QueryGenerator",
    "TypeGenerator",
    "TypeMapper",
]
