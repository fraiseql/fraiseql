"""Schema introspection and code generation.

Generates GraphQL input types, mutations, and queries from PostgreSQL schema.
Runtime introspection is implemented in Rust fraiseql-server.
"""

from .auto_discovery import SchemaDiscovery
from .type_generator import TypeGenerator
from .mutation_generator import MutationGenerator
from .query_generator import QueryGenerator

__all__ = ["SchemaDiscovery", "TypeGenerator", "MutationGenerator", "QueryGenerator"]
