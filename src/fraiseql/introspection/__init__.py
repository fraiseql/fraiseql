"""FraiseQL auto-discovery introspection engine.

This module provides automatic discovery of GraphQL schemas from PostgreSQL metadata.
It introspects database views, functions, and comments to generate types, queries, and mutations.
"""

from .metadata_parser import MetadataParser
from .postgres_introspector import PostgresIntrospector

__all__ = ["MetadataParser", "PostgresIntrospector"]
