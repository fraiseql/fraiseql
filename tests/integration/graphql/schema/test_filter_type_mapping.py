"""Test filter type mapping for GraphQL schema generation (TDD Red Cycle).

These tests focus on the _get_filter_type_for_field function and how it maps
Python types and field names to appropriate GraphQL filter types.
"""

from typing import List

import pytest

from fraiseql.sql.graphql_where_generator import (
    ArrayFilter,
    StringFilter,
    VectorFilter,
    _get_filter_type_for_field,
)

pytestmark = pytest.mark.integration


class TestFilterTypeMapping:
    """Test filter type mapping functionality."""

    def test_embedding_field_maps_to_vector_filter(self) -> None:
        """Should detect embedding field by name and map to VectorFilter."""
        # Red cycle - this will fail initially
        field_type = List[float]
        field_name = "embedding"

        result = _get_filter_type_for_field(field_type, field_name=field_name)
        assert result == VectorFilter

    def test_text_embedding_maps_to_vector_filter(self) -> None:
        """Should detect text_embedding field by name and map to VectorFilter."""
        # Red cycle - this will fail initially
        field_type = List[float]
        field_name = "text_embedding"

        result = _get_filter_type_for_field(field_type, field_name=field_name)
        assert result == VectorFilter

    def test_regular_list_float_maps_to_array_filter(self) -> None:
        """Should map regular list[float] fields to ArrayFilter."""
        # Red cycle - this will fail initially
        field_type = List[float]
        field_name = "scores"  # Not a vector pattern

        result = _get_filter_type_for_field(field_type, field_name=field_name)
        assert result == ArrayFilter

    def test_vector_pattern_precedence(self) -> None:
        """Should give vector patterns precedence over regular array detection."""
        # Red cycle - this will fail initially
        field_type = List[float]

        # Vector pattern should get VectorFilter
        vector_result = _get_filter_type_for_field(field_type, field_name="embedding")
        assert vector_result == VectorFilter

        # Non-vector pattern should get ArrayFilter
        array_result = _get_filter_type_for_field(field_type, field_name="scores")
        assert array_result == ArrayFilter

    def test_vector_field_without_list_type(self) -> None:
        """Should not map to VectorFilter if field name matches but type is not list."""
        # Red cycle - this will fail initially
        field_type = str  # Not a list type
        field_name = "embedding"

        result = _get_filter_type_for_field(field_type, field_name=field_name)
        assert result == StringFilter  # Should fall back to StringFilter
