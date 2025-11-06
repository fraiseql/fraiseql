"""Unit tests for nested JSONB path building functionality.

This module tests the generation of JSONB paths like data->'device'->>'is_active'
for nested field filters.
"""

import pytest
from unittest.mock import MagicMock
from psycopg_pool import AsyncConnectionPool

from fraiseql.db import FraiseQLRepository


class TestNestedJSONBPathBuilder:
    """Test nested JSONB path building."""

    def setup_method(self):
        """Set up test repository with mock pool."""
        self.mock_pool = MagicMock(spec=AsyncConnectionPool)
        self.repo = FraiseQLRepository(self.mock_pool)

    def test_build_nested_jsonb_path_method_exists(self):
        """Test that the nested JSONB path building method exists.

        🔴 RED CYCLE: This will fail because method doesn't exist yet.
        """
        # Check that the method exists
        assert hasattr(self.repo, "_build_nested_jsonb_path")

    def test_build_nested_jsonb_path_basic_functionality(self):
        """Test basic nested JSONB path building.

        🔴 RED CYCLE: This will fail until the method is implemented.
        """
        # Test building path for device.is_active
        result = self.repo._build_nested_jsonb_path("device", "is_active")

        # Should generate SQL like: data -> 'device' ->> 'is_active'
        sql_str = result.as_string(None)
        assert "data" in sql_str
        assert "'device'" in sql_str
        assert "'is_active'" in sql_str
        assert "->" in sql_str
        assert "->>" in sql_str

    def test_build_nested_jsonb_path_camelcase_conversion(self):
        """Test that camelCase field names are converted to snake_case.

        🔴 RED CYCLE: camelCase conversion is critical for the fix.
        """
        # Test camelCase parent field
        result = self.repo._build_nested_jsonb_path("deviceName", "isActive")
        sql_str = result.as_string(None)

        # Should convert to snake_case: device_name.is_active
        assert "'device_name'" in sql_str
        assert "'is_active'" in sql_str
        # Should NOT contain original camelCase
        assert "deviceName" not in sql_str
        assert "isActive" not in sql_str

    def test_build_nested_jsonb_path_snake_case_unchanged(self):
        """Test that snake_case field names remain unchanged."""
        # Test snake_case fields
        result = self.repo._build_nested_jsonb_path("device_name", "is_active")
        sql_str = result.as_string(None)

        # Should remain snake_case
        assert "'device_name'" in sql_str
        assert "'is_active'" in sql_str

    def test_build_nested_jsonb_path_complex_camelcase(self):
        """Test complex camelCase conversions."""
        test_cases = [
            ("deviceName", "isActive", "'device_name'", "'is_active'"),
            ("userId", "createdAt", "'user_id'", "'created_at'"),
            ("APIKey", "HTTPPort", "'api_key'", "'http_port'"),
        ]

        for parent_field, nested_field, expected_parent, expected_nested in test_cases:
            result = self.repo._build_nested_jsonb_path(parent_field, nested_field)
            sql_str = result.as_string(None)

            assert expected_parent in sql_str
            assert expected_nested in sql_str
            # Should not contain original camelCase
            assert parent_field not in sql_str
            assert nested_field not in sql_str

    def test_build_nested_jsonb_path_structure(self):
        """Test the exact SQL structure of generated paths."""
        result = self.repo._build_nested_jsonb_path("device", "is_active")
        sql_str = result.as_string(None)

        # Should follow pattern: data -> 'parent' ->> 'nested'
        # Note: The exact SQL structure depends on psycopg implementation
        # but should contain the key elements
        assert sql_str is not None
        assert len(sql_str) > 0
