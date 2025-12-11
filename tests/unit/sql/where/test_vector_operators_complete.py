"""Comprehensive tests for vector operator SQL building."""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.where.operators.vectors import (
    build_cosine_distance_sql,
    build_custom_distance_sql,
    build_half_vector_avg_aggregation,
    build_half_vector_sum_aggregation,
    build_hamming_distance_sql,
    build_inner_product_sql,
    build_jaccard_distance_sql,
    build_l1_distance_sql,
    build_l2_distance_sql,
    build_quantization_reconstruct_sql,
    build_quantized_distance_sql,
    build_sparse_cosine_distance_sql,
    build_sparse_inner_product_sql,
    build_sparse_l2_distance_sql,
    build_sparse_vector_avg_aggregation,
    build_sparse_vector_sum_aggregation,
    build_vector_avg_aggregation,
    build_vector_norm_sql,
    build_vector_sum_aggregation,
)


class TestDenseVectorDistanceOperators:
    """Test dense vector distance calculation operators."""

    def test_cosine_distance(self):
        """Test cosine distance operator."""
        path_sql = SQL("embedding")
        vector = [0.1, 0.2, 0.3, 0.4, 0.5]
        result = build_cosine_distance_sql(path_sql, vector)
        sql_str = result.as_string(None)
        assert "<=>" in sql_str
        assert "::vector" in sql_str
        assert "[0.1,0.2,0.3,0.4,0.5]" in sql_str
        assert "embedding" in sql_str

    def test_l2_distance(self):
        """Test L2/Euclidean distance operator."""
        path_sql = SQL("vector_field")
        vector = [1.0, 2.0, 3.0]
        result = build_l2_distance_sql(path_sql, vector)
        sql_str = result.as_string(None)
        assert "<->" in sql_str
        assert "::vector" in sql_str
        assert "[1.0,2.0,3.0]" in sql_str

    def test_inner_product(self):
        """Test inner product operator."""
        path_sql = SQL("embeddings")
        vector = [0.5, -0.2, 0.8]
        result = build_inner_product_sql(path_sql, vector)
        sql_str = result.as_string(None)
        assert "<#>" in sql_str
        assert "::vector" in sql_str
        assert "[0.5,-0.2,0.8]" in sql_str

    def test_l1_distance(self):
        """Test L1/Manhattan distance operator."""
        path_sql = SQL("vectors")
        vector = [1.5, 2.5, -1.0]
        result = build_l1_distance_sql(path_sql, vector)
        sql_str = result.as_string(None)
        assert "<+>" in sql_str
        assert "::vector" in sql_str
        assert "[1.5,2.5,-1.0]" in sql_str


class TestBinaryVectorDistanceOperators:
    """Test binary vector distance operators."""

    def test_hamming_distance(self):
        """Test Hamming distance for bit vectors."""
        path_sql = SQL("bit_vector")
        bit_string = "101010"
        result = build_hamming_distance_sql(path_sql, bit_string)
        sql_str = result.as_string(None)
        assert "<~>" in sql_str
        assert "::bit" in sql_str
        assert "101010" in sql_str

    def test_jaccard_distance(self):
        """Test Jaccard distance for bit vectors."""
        path_sql = SQL("bit_set")
        bit_string = "111000"
        result = build_jaccard_distance_sql(path_sql, bit_string)
        sql_str = result.as_string(None)
        assert "<%>" in sql_str
        assert "::bit" in sql_str
        assert "111000" in sql_str


class TestSparseVectorDistanceOperators:
    """Test sparse vector distance operators."""

    def test_sparse_cosine_distance(self):
        """Test sparse vector cosine distance."""
        path_sql = SQL("sparse_embedding")
        sparse_vector = {"indices": [0, 2, 4], "values": [0.1, 0.3, 0.5]}
        result = build_sparse_cosine_distance_sql(path_sql, sparse_vector)
        sql_str = result.as_string(None)
        assert "<=>" in sql_str
        assert "::sparsevec" in sql_str
        assert "0:0.1,2:0.3,4:0.5" in sql_str

    def test_sparse_l2_distance(self):
        """Test sparse vector L2 distance."""
        path_sql = SQL("sparse_vec")
        sparse_vector = {"indices": [1, 3, 5], "values": [0.2, 0.4, 0.6]}
        result = build_sparse_l2_distance_sql(path_sql, sparse_vector)
        sql_str = result.as_string(None)
        assert "<->" in sql_str
        assert "::sparsevec" in sql_str
        assert "1:0.2,3:0.4,5:0.6" in sql_str

    def test_sparse_inner_product(self):
        """Test sparse vector inner product."""
        path_sql = SQL("sparse_vectors")
        sparse_vector = {"indices": [0, 1, 2], "values": [1.0, 2.0, 3.0]}
        result = build_sparse_inner_product_sql(path_sql, sparse_vector)
        sql_str = result.as_string(None)
        assert "<#>" in sql_str
        assert "::sparsevec" in sql_str
        assert "0:1.0,1:2.0,2:3.0" in sql_str

    def test_sparse_empty_vector(self):
        """Test sparse vector with empty indices/values."""
        path_sql = SQL("sparse_field")
        sparse_vector = {"indices": [], "values": []}
        result = build_sparse_cosine_distance_sql(path_sql, sparse_vector)
        sql_str = result.as_string(None)
        assert "<=>" in sql_str
        assert "::sparsevec" in sql_str
        # Should handle empty case gracefully


class TestVectorAggregationOperators:
    """Test vector aggregation functions."""

    def test_vector_sum_aggregation(self):
        """Test vector SUM aggregation."""
        path_sql = SQL("embeddings")
        result = build_vector_sum_aggregation(path_sql)
        sql_str = result.as_string(None)
        assert "SUM(" in sql_str
        assert ")::vector" in sql_str
        assert "embeddings" in sql_str

    def test_vector_avg_aggregation(self):
        """Test vector AVG aggregation."""
        path_sql = SQL("vectors")
        result = build_vector_avg_aggregation(path_sql)
        sql_str = result.as_string(None)
        assert "AVG(" in sql_str
        assert ")::vector" in sql_str

    def test_sparse_vector_sum_aggregation(self):
        """Test sparse vector SUM aggregation."""
        path_sql = SQL("sparse_embeddings")
        result = build_sparse_vector_sum_aggregation(path_sql)
        sql_str = result.as_string(None)
        assert "SUM(" in sql_str
        assert ")::sparsevec" in sql_str

    def test_sparse_vector_avg_aggregation(self):
        """Test sparse vector AVG aggregation."""
        path_sql = SQL("sparse_vec")
        result = build_sparse_vector_avg_aggregation(path_sql)
        sql_str = result.as_string(None)
        assert "AVG(" in sql_str
        assert ")::sparsevec" in sql_str

    def test_half_vector_sum_aggregation(self):
        """Test half-vector SUM aggregation."""
        path_sql = SQL("half_vectors")
        result = build_half_vector_sum_aggregation(path_sql)
        sql_str = result.as_string(None)
        assert "SUM(" in sql_str
        assert ")::halfvec" in sql_str

    def test_half_vector_avg_aggregation(self):
        """Test half-vector AVG aggregation."""
        path_sql = SQL("half_vec")
        result = build_half_vector_avg_aggregation(path_sql)
        sql_str = result.as_string(None)
        assert "AVG(" in sql_str
        assert ")::halfvec" in sql_str


class TestCustomDistanceOperators:
    """Test custom distance function operators."""

    def test_custom_distance_with_function_only(self):
        """Test custom distance with just function name."""
        path_sql = SQL("vectors")
        custom_config = {"function": "my_custom_distance"}
        result = build_custom_distance_sql(path_sql, custom_config)
        sql_str = result.as_string(None)
        assert "my_custom_distance(" in sql_str
        assert "vectors" in sql_str

    def test_custom_distance_with_parameters(self):
        """Test custom distance with parameters."""
        path_sql = SQL("embeddings")
        custom_config = {"function": "advanced_distance", "parameters": ["param1", "param2", 42]}
        result = build_custom_distance_sql(path_sql, custom_config)
        sql_str = result.as_string(None)
        assert "advanced_distance(" in sql_str
        assert "embeddings" in sql_str
        assert "param1" in sql_str
        assert "param2" in sql_str
        assert "42" in sql_str

    def test_custom_distance_invalid_config(self):
        """Test custom distance with invalid configuration."""
        path_sql = SQL("vectors")
        with pytest.raises(ValueError, match="'function' key"):
            build_custom_distance_sql(path_sql, {})

    def test_custom_distance_non_dict_config(self):
        """Test custom distance with non-dict configuration."""
        path_sql = SQL("vectors")
        with pytest.raises(ValueError, match="'function' key"):
            build_custom_distance_sql(path_sql, "not_a_dict")  # type: ignore[arg-type]


class TestVectorUtilityOperators:
    """Test vector utility functions."""

    def test_vector_norm(self):
        """Test vector norm calculation."""
        path_sql = SQL("embeddings")
        result = build_vector_norm_sql(path_sql, None)
        sql_str = result.as_string(None)
        assert "vector_norm(" in sql_str
        assert "embeddings" in sql_str
        assert "'l2'" in sql_str


class TestQuantizedVectorOperators:
    """Test quantized vector operations."""

    def test_quantized_distance_cosine(self):
        """Test quantized vector cosine distance."""
        path_sql = SQL("quantized_vec")
        config = {"target_vector": [0.1, 0.2, 0.3], "distance_type": "cosine"}
        result = build_quantized_distance_sql(path_sql, config)
        sql_str = result.as_string(None)
        assert "quantized_cosine_distance(" in sql_str
        assert "[0.1,0.2,0.3]" in sql_str
        assert "::vector" in sql_str

    def test_quantized_distance_l2(self):
        """Test quantized vector L2 distance."""
        path_sql = SQL("quantized_embeddings")
        config = {"target_vector": [1.0, 2.0, 3.0], "distance_type": "l2"}
        result = build_quantized_distance_sql(path_sql, config)
        sql_str = result.as_string(None)
        assert "quantized_l2_distance(" in sql_str
        assert "[1.0,2.0,3.0]" in sql_str

    def test_quantized_distance_missing_target(self):
        """Test quantized distance with missing target vector."""
        path_sql = SQL("quantized_vec")
        with pytest.raises(ValueError, match="'target_vector' key"):
            build_quantized_distance_sql(path_sql, {})

    def test_quantized_distance_unsupported_target(self):
        """Test quantized distance with unsupported target type."""
        path_sql = SQL("quantized_vec")
        config = {"target_vector": "not_a_list"}
        with pytest.raises(ValueError, match="currently only supports dense target vectors"):
            build_quantized_distance_sql(path_sql, config)

    def test_quantization_reconstruct(self):
        """Test quantization reconstruction."""
        path_sql = SQL("quantized_data")
        result = build_quantization_reconstruct_sql(path_sql, None)
        sql_str = result.as_string(None)
        assert "reconstruct_quantized_vector(" in sql_str
        assert "quantized_data" in sql_str


class TestVectorEdgeCases:
    """Test vector operator edge cases."""

    def test_large_vector(self):
        """Test with a large vector."""
        path_sql = SQL("large_embedding")
        # Create a vector with 1000 dimensions
        vector = [0.01 * i for i in range(1000)]
        result = build_cosine_distance_sql(path_sql, vector)
        sql_str = result.as_string(None)
        assert "<=>" in sql_str
        assert "::vector" in sql_str
        # Should contain the vector literal (truncated in display but present)

    def test_vector_with_special_floats(self):
        """Test vector with special float values."""
        path_sql = SQL("special_vec")
        vector = [float("inf"), float("-inf"), float("nan")]
        result = build_l2_distance_sql(path_sql, vector)
        sql_str = result.as_string(None)
        assert "<->" in sql_str
        # Special float values should be handled

    def test_sparse_vector_single_element(self):
        """Test sparse vector with single element."""
        path_sql = SQL("sparse_single")
        sparse_vector = {"indices": [42], "values": [0.5]}
        result = build_sparse_cosine_distance_sql(path_sql, sparse_vector)
        sql_str = result.as_string(None)
        assert "42:0.5" in sql_str
        assert "/43" in sql_str  # dimension = max_index + 1 = 43
