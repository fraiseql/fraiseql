"""Test field detection for vector/embedding fields (TDD Red Cycle).

These tests focus on detecting vector fields based on field name patterns
for PostgreSQL pgvector support.
"""

import pytest

from fraiseql.sql.where.core.field_detection import FieldType, detect_field_type


class TestVectorFieldDetection:
    """Test vector/embedding field detection functionality."""

    def test_detect_vector_from_field_name_embedding_suffix(self) -> None:
        """Should detect vector fields from embedding field names."""
        # Red cycle - this will fail initially
        result = detect_field_type("embedding", [0.1, 0.2, 0.3], None)
        assert result == FieldType.VECTOR

    def test_detect_vector_from_field_name_text_embedding(self) -> None:
        """Should detect vector fields from text_embedding field names."""
        # Red cycle - this will fail initially
        result = detect_field_type("text_embedding", [0.1, 0.2, 0.3], None)
        assert result == FieldType.VECTOR

    def test_detect_vector_from_field_name_vector_suffix(self) -> None:
        """Should detect vector fields from _vector field names."""
        # Red cycle - this will fail initially
        result = detect_field_type("_vector", [0.1, 0.2, 0.3], None)
        assert result == FieldType.VECTOR

    def test_detect_vector_from_field_name_embedding_vector(self) -> None:
        """Should detect vector fields from embedding_vector field names."""
        # Red cycle - this will fail initially
        result = detect_field_type("embedding_vector", [0.1, 0.2, 0.3], None)
        assert result == FieldType.VECTOR

    def test_vector_vs_array_disambiguation(self) -> None:
        """Should detect vector fields vs regular array fields by name pattern."""
        # Red cycle - this will fail initially
        # Vector field (should be VECTOR)
        vector_result = detect_field_type("embedding", [0.1, 0.2, 0.3], None)
        assert vector_result == FieldType.VECTOR

        # Regular array field (should be ARRAY)
        array_result = detect_field_type("tags", ["tag1", "tag2"], None)
        assert array_result == FieldType.ARRAY

        # Another regular array field (should be ARRAY)
        scores_result = detect_field_type("scores", [1.0, 2.0, 3.0], None)
        assert scores_result == FieldType.ARRAY

    def test_vector_field_type_enum_exists(self) -> None:
        """Should have VECTOR field type in FieldType enum."""
        # Red cycle - this will fail initially
        # This test verifies the enum exists and can be accessed
        assert hasattr(FieldType, "VECTOR")
        assert FieldType.VECTOR.value == "vector"
