"""Vector scalar type for PostgreSQL pgvector.

Minimal validation following FraiseQL philosophy:
- Verify value is list of numbers
- Let PostgreSQL handle dimension validation
- No conversion or transformation
"""

from typing import Any

from graphql import GraphQLError, GraphQLScalarType

from fraiseql.types.definitions import ScalarMarker


def serialize_vector(value: Any) -> list[float]:
    """Serialize vector to GraphQL output (no transformation).

    Args:
        value: The vector value to serialize

    Returns:
        The vector as a list of floats

    Raises:
        GraphQLError: If the value is not a valid vector
    """
    if not isinstance(value, list):
        msg = f"Vector must be a list, got {type(value).__name__}"
        raise GraphQLError(msg)

    if not all(isinstance(x, (int, float)) for x in value):
        msg = "All vector values must be numbers"
        raise GraphQLError(msg)

    # Return as floats for consistency, but don't modify original
    return [float(x) for x in value]


def parse_vector_value(value: Any) -> list[float]:
    """Parse GraphQL input to vector (basic validation only).

    Args:
        value: Input value from GraphQL

    Returns:
        Validated vector as list of floats

    Raises:
        GraphQLError: If value is not a list or contains non-numeric elements
    """
    if not isinstance(value, list):
        msg = f"Vector must be a list of floats, got {type(value).__name__}"
        raise GraphQLError(msg)

    if not all(isinstance(x, (int, float)) for x in value):
        msg = "All vector values must be numbers"
        raise GraphQLError(msg)

    # NO dimension validation - let PostgreSQL handle it
    # Coerce integers to floats for consistency
    return [float(x) for x in value]


# GraphQL scalar definition
VectorScalar = GraphQLScalarType(
    name="Vector",
    description=(
        "PostgreSQL vector type for pgvector extension. "
        "Represents vector embeddings as lists of floats. "
        "Distance operators available: cosine_distance, l2_distance, inner_product."
    ),
    serialize=serialize_vector,
    parse_value=parse_vector_value,
    parse_literal=None,  # Vectors are typically passed as variables, not literals
)


# Python marker for use in dataclasses
class VectorField(list[float], ScalarMarker):
    """Python marker for the GraphQL Vector scalar.

    Use this type in your FraiseQL model fields to indicate vector embeddings:

    ```python
    @type(sql_source="documents")
    class Document:
        id: UUID
        embedding: VectorField  # Will be detected as vector field
    ```
    """

    __slots__ = ()

    def __repr__(self) -> str:
        """String representation used in type annotations and debug output."""
        return "Vector"
