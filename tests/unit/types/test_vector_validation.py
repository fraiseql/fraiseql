"""Tests for vector value validation and handling.

Tests vector scalar type validation following FraiseQL philosophy:
- Minimal validation (trust PostgreSQL for dimensions)
- Basic type checking only
- No transformation of values
"""

import pytest

from fraiseql.types.scalars.vector import parse_vector_value, serialize_vector


class TestVectorValidation:
    """Test vector value validation and serialization."""

    def test_vector_accepts_list_of_floats(self) -> None:
        """Valid vectors (list of floats) should pass validation."""
        valid_vector = [0.1, 0.2, 0.3, 0.4]
        result = parse_vector_value(valid_vector)
        assert result == valid_vector

    def test_vector_accepts_list_of_ints(self) -> None:
        """Integers should be coerced to floats."""
        int_vector = [1, 2, 3, 4]
        result = parse_vector_value(int_vector)
        assert result == [1.0, 2.0, 3.0, 4.0]
        assert all(isinstance(x, float) for x in result)

    def test_vector_rejects_non_list(self) -> None:
        """Non-list values should be rejected."""
        with pytest.raises(Exception, match="Vector must be a list of floats"):  # GraphQLError
            parse_vector_value("not a list")

        with pytest.raises(Exception, match="Vector must be a list of floats"):
            parse_vector_value({"key": "value"})

        with pytest.raises(Exception, match="Vector must be a list of floats"):
            parse_vector_value(42)

    def test_vector_rejects_non_numeric(self) -> None:
        """Lists containing non-numeric values should be rejected."""
        with pytest.raises(Exception, match="All vector values must be numbers"):
            parse_vector_value([0.1, "string", 0.3])

        with pytest.raises(Exception, match="All vector values must be numbers"):
            parse_vector_value([0.1, None, 0.3])

        with pytest.raises(Exception, match="All vector values must be numbers"):
            parse_vector_value([0.1, [1, 2], 0.3])

    def test_vector_no_dimension_validation(self) -> None:
        """Any dimension should be accepted (validation delegated to PostgreSQL)."""
        # Empty vector
        result = parse_vector_value([])
        assert result == []

        # Small dimension
        result = parse_vector_value([0.1, 0.2])
        assert result == [0.1, 0.2]

        # Large dimension (typical for embeddings)
        large_vector = [0.1] * 1536  # OpenAI ada-002 dimension
        result = parse_vector_value(large_vector)
        assert result == large_vector
        assert len(result) == 1536

    def test_vector_serialize_no_transformation(self) -> None:
        """Serialization should return values unchanged."""
        original = [0.1, 0.2, 0.3]
        result = serialize_vector(original)
        assert result == original  # Should be same values
        assert all(isinstance(x, float) for x in result)

    def test_vector_mixed_int_float_coercion(self) -> None:
        """Mixed int/float lists should be properly coerced to all floats."""
        mixed_vector = [1, 2.5, 3, 4.0]
        result = parse_vector_value(mixed_vector)
        assert result == [1.0, 2.5, 3.0, 4.0]
        assert all(isinstance(x, float) for x in result)
