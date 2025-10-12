"""Unit tests for automatic field name conversion in WHERE clauses.

This module tests the new field name mapping functionality that converts
GraphQL camelCase field names to database snake_case field names automatically
in WHERE clause processing.
"""

import pytest
from unittest.mock import MagicMock, AsyncMock
from psycopg.sql import SQL, Composed
from psycopg_pool import AsyncConnectionPool

from fraiseql.db import FraiseQLRepository


class TestFieldNameMapping:
    """Test automatic field name conversion in WHERE clauses."""

    def setup_method(self):
        """Set up test repository with mock pool."""
        self.mock_pool = MagicMock(spec=AsyncConnectionPool)
        self.repo = FraiseQLRepository(self.mock_pool)

    def test_camel_case_where_field_names_work_automatically(self):
        """GraphQL camelCase field names should work in WHERE clauses without manual conversion.

        🔴 RED CYCLE: This test will fail initially - that's expected!
        """
        # Test camelCase field names in WHERE clauses
        where_clause = {"ipAddress": {"eq": "192.168.1.1"}}

        # This should convert ipAddress -> ip_address internally
        result = self.repo._convert_dict_where_to_sql(where_clause)

        assert result is not None
        sql_str = result.as_string(None)

        # Should generate SQL with snake_case database field names
        assert "ip_address" in sql_str
        # Should NOT contain the original camelCase name
        assert "ipAddress" not in sql_str
        # Should contain the IP value
        assert "192.168.1.1" in sql_str

    def test_multiple_camel_case_fields_converted(self):
        """Multiple camelCase fields should all be converted automatically.

        🔴 RED CYCLE: This will fail initially.
        """
        where_clause = {
            "ipAddress": {"eq": "192.168.1.1"},
            "macAddress": {"eq": "aa:bb:cc:dd:ee:ff"},
            "deviceName": {"contains": "router"},
        }

        result = self.repo._convert_dict_where_to_sql(where_clause)
        assert result is not None

        sql_str = result.as_string(None)

        # All fields should be converted to snake_case
        assert "ip_address" in sql_str
        assert "mac_address" in sql_str
        assert "device_name" in sql_str

        # Should not contain camelCase names
        assert "ipAddress" not in sql_str
        assert "macAddress" not in sql_str
        assert "deviceName" not in sql_str

    def test_snake_case_fields_work_unchanged(self):
        """Existing snake_case field names should continue to work (backward compatibility)."""
        where_clause = {"ip_address": {"eq": "192.168.1.1"}}

        result = self.repo._convert_dict_where_to_sql(where_clause)
        assert result is not None

        sql_str = result.as_string(None)
        assert "ip_address" in sql_str
        assert "192.168.1.1" in sql_str

    def test_mixed_case_fields_both_work(self):
        """Mixed camelCase and snake_case fields should both work in same query."""
        where_clause = {
            "ipAddress": {"eq": "192.168.1.1"},  # camelCase
            "status": {"eq": "active"},  # snake_case
            "deviceName": {"contains": "router"},  # camelCase
        }

        result = self.repo._convert_dict_where_to_sql(where_clause)
        assert result is not None

        sql_str = result.as_string(None)

        # Converted camelCase fields
        assert "ip_address" in sql_str
        assert "device_name" in sql_str

        # Unchanged snake_case field
        assert "status" in sql_str

        # Original camelCase should not appear
        assert "ipAddress" not in sql_str
        assert "deviceName" not in sql_str

    def test_field_name_conversion_method_exists(self):
        """The field name conversion method should exist and work correctly.

        🔴 RED CYCLE: This will fail because method doesn't exist yet.
        """
        # Test the conversion method directly
        assert hasattr(self.repo, "_convert_field_name_to_database")

        # Test various conversions
        assert self.repo._convert_field_name_to_database("ipAddress") == "ip_address"
        assert self.repo._convert_field_name_to_database("macAddress") == "mac_address"
        assert self.repo._convert_field_name_to_database("deviceName") == "device_name"
        assert self.repo._convert_field_name_to_database("createdAt") == "created_at"

        # Snake case should be unchanged
        assert self.repo._convert_field_name_to_database("ip_address") == "ip_address"
        assert self.repo._convert_field_name_to_database("status") == "status"

    def test_empty_where_clause_handling(self):
        """Empty WHERE clauses should be handled gracefully."""
        result = self.repo._convert_dict_where_to_sql({})
        assert result is None

    def test_none_field_values_ignored(self):
        """None values in WHERE clauses should be ignored."""
        where_clause = {
            "ipAddress": {"eq": "192.168.1.1"},  # Valid
            "status": None,  # Should be ignored
            "deviceName": {"contains": None},  # Should be ignored
        }

        result = self.repo._convert_dict_where_to_sql(where_clause)
        assert result is not None

        sql_str = result.as_string(None)

        # Should contain the valid field (converted)
        assert "ip_address" in sql_str

        # Should not contain ignored fields
        assert "status" not in sql_str
        assert "device_name" not in sql_str

    def test_complex_camel_case_conversions(self):
        """Test more complex camelCase to snake_case conversions."""
        test_cases = [
            ("ipAddress", "ip_address"),
            ("macAddress", "mac_address"),
            ("deviceName", "device_name"),
            ("createdAt", "created_at"),
            ("updatedAt", "updated_at"),
            ("userId", "user_id"),
            ("organizationId", "organization_id"),
            ("APIKey", "api_key"),  # Multiple capitals
            ("HTTPPort", "http_port"),  # Multiple capitals
            ("XMLData", "xml_data"),  # Multiple capitals
        ]

        for camel_case, expected_snake_case in test_cases:
            where_clause = {camel_case: {"eq": "test_value"}}
            result = self.repo._convert_dict_where_to_sql(where_clause)

            assert result is not None
            sql_str = result.as_string(None)

            # Should contain the expected snake_case field name
            assert expected_snake_case in sql_str, (
                f"Failed to convert {camel_case} to {expected_snake_case}"
            )

            # Should not contain the original camelCase name
            assert camel_case not in sql_str, (
                f"Original camelCase {camel_case} should not appear in SQL"
            )

    def test_edge_case_field_names(self):
        """Test edge cases like empty strings and unusual field names."""
        # Test empty field name handling
        assert self.repo._convert_field_name_to_database("") == ""

        # Test None handling (should be graceful)
        assert self.repo._convert_field_name_to_database(None) == ""

        # Test single character names
        assert self.repo._convert_field_name_to_database("a") == "a"
        assert self.repo._convert_field_name_to_database("A") == "a"

        # Test numbers in field names
        assert self.repo._convert_field_name_to_database("id1") == "id1"
        assert self.repo._convert_field_name_to_database("apiV2Key") == "api_v2_key"

    def test_method_is_idempotent(self):
        """Calling the conversion method multiple times should produce the same result."""
        test_cases = ["ipAddress", "ip_address", "deviceName", "status"]

        for field_name in test_cases:
            # First conversion
            first_result = self.repo._convert_field_name_to_database(field_name)

            # Second conversion on the result
            second_result = self.repo._convert_field_name_to_database(first_result)

            # Should be the same
            assert first_result == second_result, f"Method not idempotent for {field_name}"
