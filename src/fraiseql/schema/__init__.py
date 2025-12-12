"""FraiseQL schema generation and validation.

Union type generation for mutations.
"""

from .mutation_schema_generator import MutationSchema, generate_mutation_schema
from .validator import SchemaValidator

__all__ = [
    "MutationSchema",
    "SchemaValidator",
    "generate_mutation_schema",
]
