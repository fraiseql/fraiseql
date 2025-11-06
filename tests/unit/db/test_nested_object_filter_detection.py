"""Unit tests for nested object filter detection logic.

This module tests the detection of nested object filters in dict-based WHERE clauses.
The current implementation only recognizes nested objects when they contain an 'id' field,
but the fix will extend this to recognize nested filters on any field.
"""

import pytest
from unittest.mock import MagicMock
from psycopg_pool import AsyncConnectionPool

from fraiseql.db import FraiseQLRepository


class TestNestedObjectFilterDetection:
    """Test nested object filter detection logic."""

    def setup_method(self):
        """Set up test repository with mock pool."""
        self.mock_pool = MagicMock(spec=AsyncConnectionPool)
        self.repo = FraiseQLRepository(self.mock_pool)

    def test_current_behavior_nested_id_filter_works(self):
        """Test that nested filters on 'id' field currently work (FK scenario).

        This documents the existing behavior that works.
        """
        # This should work - nested filter on 'id' field
        where_dict = {"device": {"id": {"eq": "some-device-id"}}}

        # Mock table columns to simulate FK column existence
        table_columns = {"device_id", "status", "created_at"}

        result = self.repo._convert_dict_where_to_sql(where_dict, table_columns=table_columns)

        assert result is not None
        sql_str = result.as_string(None)

        # Should use FK column directly
        assert "device_id" in sql_str
        assert "some-device-id" in sql_str

    def test_nested_non_id_filter_now_works(self):
        """Test that nested filters on non-'id' fields now work correctly.

        🟢 GREEN CYCLE: After the fix, nested filters on any field should work.
        This test verifies that {"device": {"is_active": {"eq": True}}} generates
        the correct JSONB path: data -> 'device' ->> 'is_active' = 'true'
        """
        # This now WORKS - nested filter on 'is_active' field
        where_dict = {"device": {"is_active": {"eq": True}}}

        # Mock table columns (includes 'data' column for JSONB)
        table_columns = {"data", "status", "created_at"}

        result = self.repo._convert_dict_where_to_sql(where_dict, table_columns=table_columns)

        # Should now return a valid SQL condition
        assert result is not None
        sql_str = result.as_string(None)

        # Should contain the JSONB path
        assert "data" in sql_str
        assert "'device'" in sql_str
        assert "'is_active'" in sql_str
        assert "->" in sql_str
        assert "->>" in sql_str
        assert "true" in sql_str

    def test_nested_name_filter_now_works(self):
        """Test that nested filters on 'name' field now work correctly.

        🟢 GREEN CYCLE: After the fix, nested string filters should work.
        """
        # This now WORKS - nested filter on 'name' field
        where_dict = {"device": {"name": {"contains": "router"}}}

        table_columns = {"data", "status", "created_at"}

        result = self.repo._convert_dict_where_to_sql(where_dict, table_columns=table_columns)

        # Should now return a valid SQL condition
        assert result is not None
        sql_str = result.as_string(None)

        # Should contain the JSONB path and LIKE operator
        assert "data" in sql_str
        assert "'device'" in sql_str
        assert "'name'" in sql_str
        assert "LIKE" in sql_str
        assert "%router%" in sql_str

    def test_camelcase_nested_filter_now_works(self):
        """Test that camelCase nested filters now work correctly.

        🟢 GREEN CYCLE: camelCase field names should be converted to snake_case.
        """
        # This now WORKS - camelCase nested filter
        where_dict = {"device": {"isActive": {"eq": True}}}

        table_columns = {"data", "status", "created_at"}

        result = self.repo._convert_dict_where_to_sql(where_dict, table_columns=table_columns)

        # Should now return a valid SQL condition
        assert result is not None
        sql_str = result.as_string(None)

        # Should contain snake_case field names (converted from camelCase)
        assert "data" in sql_str
        assert "'device'" in sql_str
        assert "'is_active'" in sql_str  # isActive -> is_active
        assert "->" in sql_str
        assert "->>" in sql_str
        assert "true" in sql_str

        # Should NOT contain the original camelCase
        assert "isActive" not in sql_str

    def test_detection_helper_fk_scenario(self):
        """Test that the detection helper correctly identifies FK scenarios."""
        # FK scenario: {"id": {"eq": value}} with FK column present
        field_filter = {"id": {"eq": "device-123"}}
        table_columns = {"device_id", "status", "created_at"}

        is_nested, use_fk = self.repo._is_nested_object_filter(
            "device", field_filter, table_columns
        )

        assert is_nested is True
        assert use_fk is True

    def test_detection_helper_jsonb_scenario(self):
        """Test that the detection helper correctly identifies JSONB scenarios."""
        # JSONB scenario: {"is_active": {"eq": True}} with data column present
        field_filter = {"is_active": {"eq": True}}
        table_columns = {"data", "status", "created_at"}

        is_nested, use_fk = self.repo._is_nested_object_filter(
            "device", field_filter, table_columns
        )

        assert is_nested is True
        assert use_fk is False

    def test_detection_helper_no_nested_object(self):
        """Test that the detection helper correctly identifies non-nested filters."""
        # Not a nested object: regular operator filter
        field_filter = {"eq": "value"}
        table_columns = {"data", "status", "created_at"}

        is_nested, use_fk = self.repo._is_nested_object_filter(
            "status", field_filter, table_columns
        )

        assert is_nested is False
        assert use_fk is False

    def test_detection_helper_no_table_columns(self):
        """Test detection when no table columns are provided."""
        # Without table columns, should not detect JSONB scenario
        field_filter = {"is_active": {"eq": True}}
        table_columns = None

        is_nested, use_fk = self.repo._is_nested_object_filter(
            "device", field_filter, table_columns
        )

        assert is_nested is False
        assert use_fk is False

    def test_detection_helper_no_data_column(self):
        """Test detection when table has no 'data' column (not JSONB table)."""
        # Without 'data' column, should not detect JSONB scenario
        field_filter = {"is_active": {"eq": True}}
        table_columns = {"device_id", "status", "created_at"}  # No 'data' column

        is_nested, use_fk = self.repo._is_nested_object_filter(
            "device", field_filter, table_columns
        )

        assert is_nested is False
        assert use_fk is False
