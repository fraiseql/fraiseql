"""Test VectorFilter GraphQL schema generation (TDD Red Cycle).

These tests focus on VectorFilter type generation and field mapping
for PostgreSQL pgvector support.
"""

import typing

import pytest

from fraiseql.sql.graphql_where_generator import VectorFilter

pytestmark = pytest.mark.integration


class TestVectorFilterSchema:
    """Test VectorFilter GraphQL schema generation."""

    def test_vector_filter_in_schema(self) -> None:
        """Should have VectorFilter type available for import."""
        # Red cycle - this will fail initially if VectorFilter doesn't exist
        assert VectorFilter is not None

    def test_vector_filter_fields(self) -> None:
        """Should have cosine_distance, l2_distance, inner_product fields."""
        # Red cycle - this will fail initially
        # Check that VectorFilter has the expected fields
        assert hasattr(VectorFilter, "cosine_distance")
        assert hasattr(VectorFilter, "l2_distance")
        assert hasattr(VectorFilter, "inner_product")
        assert hasattr(VectorFilter, "isnull")

    def test_vector_filter_field_types(self) -> None:
        """Should have proper GraphQL field types for vector operations."""
        from fraiseql.sql.graphql_where_generator import VectorDistanceInput

        hints = typing.get_type_hints(VectorFilter)

        # Vector distance fields now use VectorDistanceInput which supports
        # both dense vectors (list[float]) and sparse vectors (Dict)
        expected_vector_type = VectorDistanceInput | None
        assert hints.get("cosine_distance") == expected_vector_type
        assert hints.get("l2_distance") == expected_vector_type
        assert hints.get("l1_distance") == expected_vector_type
        assert hints.get("inner_product") == expected_vector_type
        assert hints.get("isnull") == (bool | None)

    def test_vector_filter_docstring(self) -> None:
        """Should have comprehensive docstring explaining pgvector operators."""
        # Red cycle - this will fail initially
        assert VectorFilter.__doc__ is not None
        docstring = VectorFilter.__doc__

        # Check for key documentation elements
        assert "pgvector" in docstring.lower()
        assert "cosine_distance" in docstring
        assert "l2_distance" in docstring
        assert "inner_product" in docstring
        assert "distance" in docstring.lower()
