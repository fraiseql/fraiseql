"""Test vector operators for PostgreSQL pgvector (TDD Red Cycle).

These tests focus on SQL generation for pgvector's three native distance operators:
- <=> : cosine distance
- <-> : L2/Euclidean distance
- <#> : negative inner product
"""

import pytest
from psycopg.sql import SQL, Composed

from fraiseql.sql.where.core.field_detection import FieldType
from fraiseql.sql.where.operators import get_operator_function
from fraiseql.sql.where.operators.vectors import (
    build_cosine_distance_sql,
    build_l2_distance_sql,
    build_inner_product_sql,
    build_l1_distance_sql,
    build_hamming_distance_sql,
    build_jaccard_distance_sql,
)


class TestVectorOperators:
    """Test vector distance operator SQL generation."""

    def test_cosine_distance_sql(self) -> None:
        """Should generate cosine distance SQL using <=> operator."""
        # Red cycle - this will fail initially
        path_sql = SQL("embedding")
        value = [0.1, 0.2, 0.3]

        result = build_cosine_distance_sql(path_sql, value)

        # Should generate: (embedding)::vector <=> '[0.1,0.2,0.3]'::vector
        assert isinstance(result, Composed)
        sql_str = str(result)
        assert "<=>" in sql_str
        assert "'[0.1,0.2,0.3]'" in sql_str
        assert "::vector" in sql_str

    def test_l2_distance_sql(self) -> None:
        """Should generate L2 distance SQL using <-> operator."""
        # Red cycle - this will fail initially
        path_sql = SQL("text_embedding")
        value = [1.0, 2.0, 3.0, 4.0]

        result = build_l2_distance_sql(path_sql, value)

        # Should generate: (text_embedding)::vector <-> '[1.0,2.0,3.0,4.0]'::vector
        assert isinstance(result, Composed)
        sql_str = str(result)
        assert "<->" in sql_str
        assert "'[1.0,2.0,3.0,4.0]'" in sql_str
        assert "::vector" in sql_str

    def test_inner_product_sql(self) -> None:
        """Should generate inner product SQL using <#> operator."""
        # Red cycle - this will fail initially
        path_sql = SQL("image_embedding")
        value = [0.5, -0.1, 0.8]

        result = build_inner_product_sql(path_sql, value)

        # Should generate: (image_embedding)::vector <#> '[0.5,-0.1,0.8]'::vector
        assert isinstance(result, Composed)
        sql_str = str(result)
        assert "<#>" in sql_str
        assert "'[0.5,-0.1,0.8]'" in sql_str
        assert "::vector" in sql_str

    def test_l1_distance_sql(self) -> None:
        """Should generate L1/Manhattan distance SQL using <+> operator."""
        # Red cycle - this will fail initially
        path_sql = SQL("sparse_embedding")
        value = [0.1, -0.2, 0.3]

        result = build_l1_distance_sql(path_sql, value)

        # Should generate: (sparse_embedding)::vector <+> '[0.1,-0.2,0.3]'::vector
        assert isinstance(result, Composed)
        sql_str = str(result)
        assert "<+>" in sql_str
        assert "'[0.1,-0.2,0.3]'" in sql_str
        assert "::vector" in sql_str

    def test_hamming_distance_sql(self) -> None:
        """Should generate Hamming distance SQL using <~> operator for bit vectors."""
        # Red cycle - this will fail initially
        path_sql = SQL("fingerprint")
        value = "101010"  # 6-bit binary string

        result = build_hamming_distance_sql(path_sql, value)

        # Should generate: (fingerprint)::bit <~> '101010'::bit
        assert isinstance(result, Composed)
        sql_str = str(result)
        assert "<~>" in sql_str
        assert "'101010'" in sql_str
        assert "::bit" in sql_str

    def test_jaccard_distance_sql(self) -> None:
        """Should generate Jaccard distance SQL using <%> operator for bit vectors."""
        # Red cycle - this will fail initially
        path_sql = SQL("features")
        value = "111000"  # 6-bit binary string

        result = build_jaccard_distance_sql(path_sql, value)

        # Should generate: (features)::bit <%> '111000'::bit
        assert isinstance(result, Composed)
        sql_str = str(result)
        assert "<%>" in sql_str
        assert "'111000'" in sql_str
        assert "::bit" in sql_str

    def test_vector_casting_format(self) -> None:
        """Should properly format vector values as PostgreSQL array literals."""
        # Red cycle - this will fail initially
        path_sql = SQL("embedding")
        value = [0.123456, -0.789, 1.0]

        result = build_cosine_distance_sql(path_sql, value)

        # Should format as '[0.123456,-0.789,1.0]'::vector
        sql_str = str(result)
        assert "'[0.123456,-0.789,1.0]'" in sql_str

    def test_vector_null_handling(self) -> None:
        """Should handle NULL vectors appropriately."""
        # Red cycle - this will fail initially
        path_sql = SQL("embedding")
        value = [0.0, 0.0]

        result = build_cosine_distance_sql(path_sql, value)

        # NULL handling will be tested in integration, but basic structure should work
        assert isinstance(result, Composed)

    def test_vector_operators_registered(self) -> None:
        """Should have vector operators registered in OPERATOR_MAP."""
        # Test that get_operator_function returns correct builders for vector operators
        cosine_func = get_operator_function(FieldType.VECTOR, "cosine_distance")
        assert cosine_func == build_cosine_distance_sql

        l2_func = get_operator_function(FieldType.VECTOR, "l2_distance")
        assert l2_func == build_l2_distance_sql

        l1_func = get_operator_function(FieldType.VECTOR, "l1_distance")
        assert l1_func == build_l1_distance_sql

        inner_func = get_operator_function(FieldType.VECTOR, "inner_product")
        assert inner_func == build_inner_product_sql

        hamming_func = get_operator_function(FieldType.VECTOR, "hamming_distance")
        assert hamming_func == build_hamming_distance_sql

        jaccard_func = get_operator_function(FieldType.VECTOR, "jaccard_distance")
        assert jaccard_func == build_jaccard_distance_sql

    def test_get_operator_function_vector(self) -> None:
        """Should return correct builder functions for vector operators."""
        # Test that the functions work correctly when called through get_operator_function
        path_sql = SQL("embedding")
        value = [0.1, 0.2, 0.3]

        cosine_func = get_operator_function(FieldType.VECTOR, "cosine_distance")
        result = cosine_func(path_sql, value)
        assert isinstance(result, Composed)
        assert "<=>" in str(result)

    def test_vector_operator_function_signatures(self) -> None:
        """Should have correct function signatures for vector operators."""
        # Test that the functions can be called with expected parameters
        path_sql = SQL("test_column")
        test_vector = [1.0, 2.0, 3.0]

        # All three functions should work without errors
        cosine_result = build_cosine_distance_sql(path_sql, test_vector)
        l2_result = build_l2_distance_sql(path_sql, test_vector)
        inner_result = build_inner_product_sql(path_sql, test_vector)

        assert all(isinstance(r, Composed) for r in [cosine_result, l2_result, inner_result])
