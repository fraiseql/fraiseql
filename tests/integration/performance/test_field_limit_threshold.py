"""Tests for JSONB field limit threshold functionality."""

import pytest
from psycopg.sql import SQL

from fraiseql.core.ast_parser import FieldPath
from fraiseql.sql.sql_generator import build_sql_query

pytestmark = pytest.mark.integration


class TestFieldLimitThreshold:
    """Test field limit threshold behavior in SQL generation."""

    def test_normal_query_below_threshold(self) -> None:
        """Test that queries with few fields use jsonb_build_object."""
        # Create 5 field paths (below any reasonable threshold)
        field_paths = [FieldPath(alias=f"field{i}", path=[f"field{i}"]) for i in range(5)]

        query = build_sql_query(
            table="users", field_paths=field_paths, json_output=True, field_limit_threshold=20
        )

        sql_str = query.as_string(None)

        # Should use jsonb_build_object
        assert "jsonb_build_object(" in sql_str
        assert "AS result" in sql_str
        # Should include all fields
        for i in range(5):
            assert f"'field{i}'" in sql_str

    def test_query_exceeds_threshold_returns_full_data(self) -> None:
        """Test that queries exceeding threshold return full data column."""
        # Create 25 field paths (exceeds threshold of 20)
        field_paths = [FieldPath(alias=f"field{i}", path=[f"field{i}"]) for i in range(25)]

        query = build_sql_query(
            table="users", field_paths=field_paths, json_output=True, field_limit_threshold=20
        )

        sql_str = query.as_string(None)

        # Should NOT use jsonb_build_object
        assert "jsonb_build_object(" not in sql_str
        # Should select data column directly
        assert "SELECT data AS result" in sql_str
        assert 'FROM "users"' in sql_str

    def test_query_at_exact_threshold(self) -> None:
        """Test behavior when field count equals threshold."""
        # Create exactly 20 field paths
        field_paths = [FieldPath(alias=f"field{i}", path=[f"field{i}"]) for i in range(20)]

        query = build_sql_query(
            table="products", field_paths=field_paths, json_output=True, field_limit_threshold=20
        )

        sql_str = query.as_string(None)

        # At threshold, should still use jsonb_build_object
        assert "jsonb_build_object(" in sql_str

    def test_threshold_with_where_clause(self) -> None:
        """Test that WHERE clause works with full data selection."""
        field_paths = [FieldPath(alias=f"field{i}", path=[f"field{i}"]) for i in range(30)]

        where_clause = SQL("data->>'status' = 'active'")

        query = build_sql_query(
            table="users",
            field_paths=field_paths,
            where_clause=where_clause,
            json_output=True,
            field_limit_threshold=20,
        )

        sql_str = query.as_string(None)

        # Should select full data
        assert "SELECT data AS result" in sql_str
        # Should include WHERE clause
        assert "WHERE data->>'status' = 'active'" in sql_str

    def test_raw_json_output_with_threshold(self) -> None:
        """Test raw JSON output when exceeding threshold."""
        field_paths = [FieldPath(alias=f"field{i}", path=[f"field{i}"]) for i in range(25)]

        query = build_sql_query(
            table="users",
            field_paths=field_paths,
            json_output=True,
            raw_json_output=True,
            field_limit_threshold=20,
        )

        sql_str = query.as_string(None)

        # Should cast to text for raw output
        assert "SELECT data::text AS result" in sql_str

    def test_no_json_output_with_threshold(self) -> None:
        """Test non-JSON output when exceeding threshold."""
        field_paths = [FieldPath(alias=f"field{i}", path=[f"field{i}"]) for i in range(25)]

        query = build_sql_query(
            table="users", field_paths=field_paths, json_output=False, field_limit_threshold=20
        )

        sql_str = query.as_string(None)

        # Should select data without aliasing
        assert "SELECT data FROM" in sql_str
        assert "AS result" not in sql_str

    def test_no_threshold_specified(self) -> None:
        """Test behavior when no threshold is specified."""
        # Create many fields
        field_paths = [FieldPath(alias=f"field{i}", path=[f"field{i}"]) for i in range(100)]

        query = build_sql_query(
            table="users",
            field_paths=field_paths,
            json_output=True,
            field_limit_threshold=None,  # No threshold
        )

        sql_str = query.as_string(None)

        # Should use jsonb_build_object regardless of field count
        assert "jsonb_build_object(" in sql_str

    def test_zero_threshold(self) -> None:
        """Test behavior with zero threshold (always use full data)."""
        field_paths = [
            FieldPath(alias="name", path=["name"]),
            FieldPath(alias="email", path=["email"]),
        ]

        query = build_sql_query(
            table="users", field_paths=field_paths, json_output=True, field_limit_threshold=0
        )

        sql_str = query.as_string(None)

        # With 0 threshold, any fields should trigger full data
        assert "SELECT data AS result" in sql_str

    def test_nested_fields_count_correctly(self) -> None:
        """Test that nested fields are counted correctly."""
        field_paths = [
            FieldPath(alias=f"nested{i}", path=["profile", "details", f"field{i}"])
            for i in range(25)
        ]

        query = build_sql_query(
            table="users", field_paths=field_paths, json_output=True, field_limit_threshold=20
        )

        sql_str = query.as_string(None)

        # Should still exceed threshold with nested fields
        assert "SELECT data AS result" in sql_str

    def test_typename_not_counted_in_threshold(self) -> None:
        """Test that __typename doesn't count toward field limit."""
        # Create exactly 20 fields
        field_paths = [FieldPath(alias=f"field{i}", path=[f"field{i}"]) for i in range(20)]

        # With typename, we'd have 21 total fields in jsonb_build_object
        query = build_sql_query(
            table="users",
            field_paths=field_paths,
            json_output=True,
            typename="User",
            field_limit_threshold=20,
        )

        sql_str = query.as_string(None)

        # Should still use jsonb_build_object (typename doesn't count)
        assert "jsonb_build_object(" in sql_str
        assert "'__typename'" in sql_str
        assert "'User'" in sql_str


class TestFieldLimitThresholdEdgeCases:
    """Test edge cases for field limit threshold."""

    def test_empty_field_paths(self) -> None:
        """Test behavior with no fields requested."""
        query = build_sql_query(
            table="users", field_paths=[], json_output=True, field_limit_threshold=20
        )

        sql_str = query.as_string(None)

        # Should still use jsonb_build_object for empty
        assert "jsonb_build_object(" in sql_str

    def test_single_field_various_thresholds(self) -> None:
        """Test single field with various threshold values."""
        field_paths = [FieldPath(alias="name", path=["name"])]

        # Threshold of 1 (should use jsonb_build_object)
        query1 = build_sql_query(
            table="users", field_paths=field_paths, json_output=True, field_limit_threshold=1
        )
        assert "jsonb_build_object(" in query1.as_string(None)

        # Threshold of 0 (should use full data)
        query2 = build_sql_query(
            table="users", field_paths=field_paths, json_output=True, field_limit_threshold=0
        )
        assert "SELECT data AS result" in query2.as_string(None)

    def test_very_large_field_count(self) -> None:
        """Test with field count that would exceed PostgreSQL limits."""
        # Create 60 fields (would need 120 parameters in jsonb_build_object)
        field_paths = [FieldPath(alias=f"field{i}", path=[f"field{i}"]) for i in range(60)]

        query = build_sql_query(
            table="users", field_paths=field_paths, json_output=True, field_limit_threshold=50
        )

        sql_str = query.as_string(None)

        # Should use full data selection
        assert "SELECT data AS result" in sql_str

    @pytest.mark.parametrize(
        ("threshold", "expected_mode"),
        [
            (10, "full_data"),  # 25 fields > 10 threshold
            (30, "jsonb_build"),  # 25 fields < 30 threshold
            (25, "jsonb_build"),  # 25 fields = 25 threshold (not exceeded)
            (24, "full_data"),  # 25 fields > 24 threshold
        ],
    )
    def test_various_thresholds(self, threshold, expected_mode) -> None:
        """Test various threshold values with 25 fields."""
        field_paths = [FieldPath(alias=f"field{i}", path=[f"field{i}"]) for i in range(25)]

        query = build_sql_query(
            table="users",
            field_paths=field_paths,
            json_output=True,
            field_limit_threshold=threshold,
        )

        sql_str = query.as_string(None)

        if expected_mode == "full_data":
            assert "SELECT data AS result" in sql_str
            assert "jsonb_build_object(" not in sql_str
        else:
            assert "jsonb_build_object(" in sql_str
            assert "SELECT data AS result" not in sql_str
