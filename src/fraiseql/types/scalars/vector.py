"""Vector scalar types for PostgreSQL pgvector.

Supports multiple vector types:
- Vector: Standard 32-bit float vectors
- HalfVector: 16-bit half-precision vectors (50% memory savings)
- SparseVector: Sparse vectors for high-dimensional embeddings

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


# GraphQL scalar definitions
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

HalfVectorScalar = GraphQLScalarType(
    name="HalfVector",
    description=(
        "PostgreSQL halfvec type for pgvector extension. "
        "Half-precision 16-bit float vectors with 50% memory savings. "
        "Same distance operators as Vector type but reduced precision."
    ),
    serialize=serialize_vector,  # Same serialization as regular vectors
    parse_value=parse_vector_value,  # Same parsing as regular vectors
    parse_literal=None,  # Vectors are typically passed as variables, not literals
)


def serialize_sparse_vector(value: Any) -> dict[str, Any]:
    """Serialize sparse vector to GraphQL output.

    Sparse vectors are represented as dictionaries with 'indices' and 'values' keys.

    Args:
        value: The sparse vector value to serialize

    Returns:
        The sparse vector as a dict with indices and values

    Raises:
        GraphQLError: If the value is not a valid sparse vector
    """
    if not isinstance(value, dict):
        msg = f"SparseVector must be a dict, got {type(value).__name__}"
        raise GraphQLError(msg)

    if "indices" not in value or "values" not in value:
        msg = "SparseVector must have 'indices' and 'values' keys"
        raise GraphQLError(msg)

    indices = value["indices"]
    values = value["values"]

    if not isinstance(indices, list) or not isinstance(values, list):
        msg = "SparseVector indices and values must be lists"
        raise GraphQLError(msg)

    if not all(isinstance(x, int) for x in indices):
        msg = "SparseVector indices must be integers"
        raise GraphQLError(msg)

    if not all(isinstance(x, (int, float)) for x in values):
        msg = "SparseVector values must be numbers"
        raise GraphQLError(msg)

    if len(indices) != len(values):
        msg = "SparseVector indices and values must have same length"
        raise GraphQLError(msg)

    # Return normalized format
    return {"indices": [int(x) for x in indices], "values": [float(x) for x in values]}


def parse_sparse_vector_value(value: Any) -> dict[str, Any]:
    """Parse GraphQL input to sparse vector.

    Accepts either:
    - Dict with 'indices' and 'values' keys
    - String in pgvector sparse vector format

    Args:
        value: Input value from GraphQL

    Returns:
        Validated sparse vector as dict with indices and values

    Raises:
        GraphQLError: If value is not a valid sparse vector
    """
    if isinstance(value, str):
        # Parse pgvector sparse vector string format like "{1:0.1,3:0.2,5:0.3}/6"
        # This is a simplified parser - full implementation would be more complex
        msg = "String format for sparse vectors not yet implemented"
        raise GraphQLError(msg)

    if not isinstance(value, dict):
        msg = f"SparseVector must be a dict, got {type(value).__name__}"
        raise GraphQLError(msg)

    if "indices" not in value or "values" not in value:
        msg = "SparseVector must have 'indices' and 'values' keys"
        raise GraphQLError(msg)

    indices = value["indices"]
    values = value["values"]

    if not isinstance(indices, list) or not isinstance(values, list):
        msg = "SparseVector indices and values must be lists"
        raise GraphQLError(msg)

    if not all(isinstance(x, int) for x in indices):
        msg = "SparseVector indices must be integers"
        raise GraphQLError(msg)

    if not all(isinstance(x, (int, float)) for x in values):
        msg = "SparseVector values must be numbers"
        raise GraphQLError(msg)

    if len(indices) != len(values):
        msg = "SparseVector indices and values must have same length"
        raise GraphQLError(msg)

    # Validate indices are non-negative
    if any(x < 0 for x in indices):
        msg = "SparseVector indices must be non-negative"
        raise GraphQLError(msg)

    return {"indices": [int(x) for x in indices], "values": [float(x) for x in values]}


SparseVectorScalar = GraphQLScalarType(
    name="SparseVector",
    description=(
        "PostgreSQL sparsevec type for pgvector extension. "
        "Efficient storage for high-dimensional vectors with mostly zero values. "
        "Represented as dict with 'indices' and 'values' arrays."
    ),
    serialize=serialize_sparse_vector,
    parse_value=parse_sparse_vector_value,
    parse_literal=None,  # Sparse vectors are typically passed as variables, not literals
)


def serialize_quantized_vector(value: Any) -> dict[str, Any]:
    """Serialize quantized vector to GraphQL output.

    Quantized vectors are represented as dictionaries with quantization info.

    Args:
        value: The quantized vector value to serialize

    Returns:
        The quantized vector as a dict with quantization metadata

    Raises:
        GraphQLError: If the value is not a valid quantized vector
    """
    if not isinstance(value, dict):
        msg = f"QuantizedVector must be a dict, got {type(value).__name__}"
        raise GraphQLError(msg)

    # Expected format: {"codebook_id": int, "code": int, "scale": float, "offset": list[float]}
    required_keys = ["codebook_id", "code"]
    for key in required_keys:
        if key not in value:
            msg = f"QuantizedVector must have '{key}' key"
            raise GraphQLError(msg)

    return {
        "codebook_id": int(value["codebook_id"]),
        "code": int(value["code"]),
        "scale": float(value.get("scale", 1.0)),
        "offset": value.get("offset", []),
    }


def parse_quantized_vector_value(value: Any) -> dict[str, Any]:
    """Parse GraphQL input to quantized vector.

    Args:
        value: Input value from GraphQL

    Returns:
        Validated quantized vector as dict

    Raises:
        GraphQLError: If value is not a valid quantized vector
    """
    if not isinstance(value, dict):
        msg = f"QuantizedVector must be a dict, got {type(value).__name__}"
        raise GraphQLError(msg)

    required_keys = ["codebook_id", "code"]
    for key in required_keys:
        if key not in value:
            msg = f"QuantizedVector must have '{key}' key"
            raise GraphQLError(msg)

    return {
        "codebook_id": int(value["codebook_id"]),
        "code": int(value["code"]),
        "scale": float(value.get("scale", 1.0)),
        "offset": value.get("offset", []),
    }


QuantizedVectorScalar = GraphQLScalarType(
    name="QuantizedVector",
    description=(
        "Quantized vector type for reduced memory usage. "
        "Vectors are represented by their closest codebook entry index. "
        "Format: {codebook_id: int, code: int, scale: float, offset: list[float]}"
    ),
    serialize=serialize_quantized_vector,
    parse_value=parse_quantized_vector_value,
    parse_literal=None,  # Quantized vectors are typically passed as variables, not literals
)


# Python markers for use in dataclasses
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


class HalfVectorField(list[float], ScalarMarker):
    """Python marker for the GraphQL HalfVector scalar.

    Use this type in your FraiseQL model fields to indicate half-precision vector embeddings:

    ```python
    @type(sql_source="documents")
    class Document:
        id: UUID
        embedding: HalfVectorField  # Will be detected as halfvec field (50% memory savings)
    ```
    """

    __slots__ = ()

    def __repr__(self) -> str:
        """String representation used in type annotations and debug output."""
        return "HalfVector"


class SparseVectorField(dict, ScalarMarker):
    """Python marker for the GraphQL SparseVector scalar.

    Use this type in your FraiseQL model fields to indicate sparse vector embeddings:

    ```python
    @type(sql_source="documents")
    class Document:
        id: UUID
        embedding: SparseVectorField  # Will be detected as sparsevec field
    ```
    """

    __slots__ = ()

    def __repr__(self) -> str:
        """String representation used in type annotations and debug output."""
        return "SparseVector"


class QuantizedVectorField(dict, ScalarMarker):
    """Python marker for the GraphQL QuantizedVector scalar.

    Use this type in your FraiseQL model fields to indicate quantized vector embeddings:

    ```python
    @type(sql_source="documents")
    class Document:
        id: UUID
        embedding: QuantizedVectorField  # Will be detected as quantized vector field
    ```
    """

    __slots__ = ()

    def __repr__(self) -> str:
        """String representation used in type annotations and debug output."""
        return "QuantizedVector"
