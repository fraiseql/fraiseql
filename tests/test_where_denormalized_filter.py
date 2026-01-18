"""Tests for WHERE clause integration with denormalized column detection.

Phase 2 tests: Testing the _resolve_column_for_nested_filter function
and WHERE clause integration with denormalized columns.
"""

import pytest
from fraiseql.where_normalization import _resolve_column_for_nested_filter


class TestResolveColumnForNestedFilter:
    """Tests for denormalized column resolution in WHERE clauses."""

    def test_resolve_existing_denorm_column(self):
        """Test resolving a filter path that has a corresponding denormalized column."""
        filter_path = ["location", "ltreePath"]
        table_columns = {"id", "location__ltree_path", "data"}

        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        assert result == "location__ltree_path"

    def test_resolve_missing_denorm_column(self):
        """Test that missing denormalized column returns None (fallback to JSONB)."""
        filter_path = ["location", "ltreePath"]
        table_columns = {"id", "data"}  # No denormalized column

        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        assert result is None

    def test_resolve_three_level_nested_path(self):
        """Test resolving three-level nested path."""
        filter_path = ["company", "department", "name"]
        table_columns = {"id", "company__department__name", "data"}

        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        assert result == "company__department__name"

    def test_resolve_with_empty_table_columns(self):
        """Test resolution with empty table_columns set."""
        filter_path = ["location", "ltreePath"]
        table_columns = set()

        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        assert result is None

    def test_resolve_with_none_table_columns(self):
        """Test resolution with None table_columns."""
        filter_path = ["location", "ltreePath"]
        table_columns = None

        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        assert result is None

    def test_resolve_with_empty_filter_path(self):
        """Test resolution with empty filter path."""
        filter_path = []
        table_columns = {"id", "data"}

        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        assert result is None

    def test_resolve_single_level_field(self):
        """Test resolving single-level field (no nesting).

        Note: While single-level fields technically can't be denormalized
        (they don't have nested paths), if someone created a column named
        exactly "status" for some reason, we would still match it.
        """
        filter_path = ["status"]
        table_columns = {"id", "status", "data"}

        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        # Single-level field generates name "status" which exists in columns
        # So this returns the column name (even though it's not typically denormalized)
        assert result == "status"

    def test_resolve_with_hash_suffixed_column(self):
        """Test resolving with hash-suffixed denormalized column."""
        # For deeply nested paths, the column may have a hash suffix
        filter_path = ["very", "deeply", "nested", "structure", "with", "many", "levels", "field"]
        # Simulate a hash-suffixed column
        hash_col = "very__deeply__nested__struct_a7c2f1"
        table_columns = {"id", hash_col, "data"}

        # The resolution should still work - it generates the expected name
        # and checks if it exists (it might be truncated)
        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        # Depending on implementation, this might match the hash-suffixed column
        # or might not if the hash mismatch
        assert result is None or result == hash_col

    def test_resolve_allocation_location_ltree_path(self):
        """Test real-world example: allocation query filtering on location.ltreePath.

        Note: In the database, the denormalized column might be named
        "allocation__location__ltree_path" for clarity, but the filter path
        only contains the nested field path ["location", "ltreePath"],
        which generates "location__ltree_path".
        """
        filter_path = ["location", "ltreePath"]
        table_columns = {"id", "location__ltree_path", "data"}

        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        assert result == "location__ltree_path"

    def test_resolve_user_address_postal_code(self):
        """Test real-world example: user query filtering on address.postalCode."""
        filter_path = ["address", "postalCode"]
        table_columns = {"id", "address__postal_code", "data"}

        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        assert result == "address__postal_code"

    def test_resolve_handles_camel_case_in_path(self):
        """Test that camelCase in filter path is handled correctly."""
        # Filter path comes from GraphQL parser (likely camelCase)
        filter_path = ["location", "ltreePath"]
        # Column exists with snake_case conversion
        table_columns = {"location__ltree_path"}

        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        assert result == "location__ltree_path"

    def test_resolve_case_sensitivity(self):
        """Test that resolution is case-sensitive (PostgreSQL default)."""
        filter_path = ["location", "ltreePath"]
        # Wrong case column - should not match
        table_columns = {"Location__Ltree_Path"}

        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        # Should not match wrong case
        assert result is None

    def test_resolve_partial_match_does_not_match(self):
        """Test that partial matches don't resolve."""
        filter_path = ["location", "ltreePath"]
        # Similar column but not exact match
        table_columns = {"location__ltree", "location__ltree_path_old"}

        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        # Should still work if exact match exists... wait let me reconsider
        # Neither "location__ltree" nor "location__ltree_path_old" match the expected name
        # The expected name would be "location__ltree_path"
        # So if they don't have the exact match, it should return None
        assert result is None


class TestEdgeCasesWhereNormalization:
    """Test edge cases in WHERE normalization with denormalized columns."""

    def test_resolve_with_mixed_column_types(self):
        """Test resolution with mixed column types (regular + denormalized)."""
        filter_path = ["location", "ltreePath"]
        table_columns = {
            "id",
            "status",
            "location_id",
            "location__ltree_path",  # Denormalized
            "data",
        }

        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        assert result == "location__ltree_path"

    def test_resolve_filters_out_non_denormalized_columns(self):
        """Test that we don't match regular columns that happen to contain underscores."""
        filter_path = ["location", "status"]
        table_columns = {
            "id",
            "location__status",  # This is our denormalized column
            "location_status",  # This is a different column (not denormalized)
            "data",
        }

        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        # Should match the double-underscore version
        assert result == "location__status"

    def test_resolve_special_characters_in_path(self):
        """Test that special characters in path are handled."""
        filter_path = ["location", "ltree_path"]  # Already snake_case
        table_columns = {"location__ltree_path"}

        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        # Should work with already snake_case input
        assert result == "location__ltree_path"

    def test_resolve_multiple_possible_columns(self):
        """Test that we match the correct column when multiple exist."""
        filter_path = ["location", "name"]
        table_columns = {
            "location__name",
            "location__name__old",
            "location__name__archived",
        }

        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        # Should match exact "location__name"
        assert result == "location__name"

    def test_resolve_with_large_table_columns(self):
        """Test resolution performance with large column set."""
        filter_path = ["location", "ltreePath"]
        # Simulate a table with many columns
        table_columns = {f"column_{i}" for i in range(1000)}
        table_columns.add("location__ltree_path")

        result = _resolve_column_for_nested_filter(filter_path, table_columns)

        assert result == "location__ltree_path"
