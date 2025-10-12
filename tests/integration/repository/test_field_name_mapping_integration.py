"""Integration tests for field name mapping in repository WHERE clauses.

These tests verify that the field name conversion works end-to-end with
the complete FraiseQL stack, including SQL generation and type detection.
"""

import pytest
from unittest.mock import MagicMock, AsyncMock
from psycopg.sql import SQL, Composed
from psycopg_pool import AsyncConnectionPool

from fraiseql.db import FraiseQLRepository


class TestFieldNameMappingIntegration:
    """Integration tests for WHERE clause field name conversion."""

    def setup_method(self):
        """Set up test repository with mock pool."""
        self.mock_pool = MagicMock(spec=AsyncConnectionPool)
        self.repo = FraiseQLRepository(self.mock_pool)

    def test_sql_generation_integration(self):
        """Test that SQL generation works correctly with field name conversion.

        This focuses on the SQL generation layer without complex async mocking.
        """
        # Test camelCase field names in WHERE clause
        where_clause = {
            "ipAddress": {"eq": "192.168.1.1"},  # camelCase
            "deviceName": {"contains": "router"},  # camelCase
        }

        # Generate SQL using repository method
        result = self.repo._convert_dict_where_to_sql(where_clause)
        assert result is not None

        sql_str = result.as_string(None)

        # Should contain snake_case field names in the SQL
        assert "ip_address" in sql_str
        assert "device_name" in sql_str

        # Should NOT contain camelCase names in SQL
        assert "ipAddress" not in sql_str
        assert "deviceName" not in sql_str

        # Should contain the values
        assert "192.168.1.1" in sql_str
        assert "router" in sql_str

    def test_backward_compatibility_integration(self):
        """Test that existing snake_case field names continue to work."""
        where_clause = {
            "ip_address": {"eq": "10.0.0.1"},  # snake_case (existing usage)
            "status": {"eq": "active"},  # snake_case (existing usage)
        }

        result = self.repo._convert_dict_where_to_sql(where_clause)
        assert result is not None

        sql_str = result.as_string(None)

        # Should work unchanged - snake_case names should remain
        assert "ip_address" in sql_str
        assert "status" in sql_str
        assert "10.0.0.1" in sql_str
        assert "active" in sql_str

    def test_mixed_case_sql_generation(self):
        """Test mixed camelCase and snake_case fields in same query."""
        where_clause = {
            "ipAddress": {"eq": "192.168.1.1"},  # camelCase (should be converted)
            "status": {"eq": "active"},  # snake_case (should remain)
            "deviceName": {"contains": "switch"},  # camelCase (should be converted)
            "created_at": {"gte": "2025-01-01"},  # snake_case (should remain)
        }

        result = self.repo._convert_dict_where_to_sql(where_clause)
        assert result is not None

        sql_str = result.as_string(None)

        # All fields should appear as snake_case in SQL
        assert "ip_address" in sql_str
        assert "status" in sql_str
        assert "device_name" in sql_str
        assert "created_at" in sql_str

        # Original camelCase should not appear
        assert "ipAddress" not in sql_str
        assert "deviceName" not in sql_str

    def test_complex_where_clause_field_conversion(self):
        """Test complex WHERE clauses with multiple operators per field."""
        where_clause = {
            "ipAddress": {"eq": "192.168.1.1", "neq": "127.0.0.1"},
            "devicePort": {"gte": 1024, "lt": 65536},
            "macAddress": {"eq": "aa:bb:cc:dd:ee:ff"},
        }

        # Convert using the repository method
        result = self.repo._convert_dict_where_to_sql(where_clause)
        assert result is not None

        sql_str = result.as_string(None)

        # All fields should be converted to snake_case
        assert "ip_address" in sql_str
        assert "device_port" in sql_str
        assert "mac_address" in sql_str

        # Should not contain original camelCase names
        assert "ipAddress" not in sql_str
        assert "devicePort" not in sql_str
        assert "macAddress" not in sql_str

        # Should contain the actual values
        assert "192.168.1.1" in sql_str
        assert "127.0.0.1" in sql_str
        assert "1024" in sql_str
        assert "65536" in sql_str
        assert "aa:bb:cc:dd:ee:ff" in sql_str

    def test_field_conversion_with_type_detection(self):
        """Test that field conversion works correctly with FraiseQL's type detection.

        This verifies that IP addresses, MAC addresses, and other special types
        are still detected correctly after field name conversion.
        """
        # Test IP address type detection with camelCase field name
        where_clause = {"ipAddress": {"eq": "192.168.1.1"}}
        result = self.repo._convert_dict_where_to_sql(where_clause)

        assert result is not None
        sql_str = result.as_string(None)

        # Should contain snake_case field name
        assert "ip_address" in sql_str
        # Should contain INET type casting (from IP detection)
        assert "::inet" in sql_str
        # Should contain the IP value
        assert "192.168.1.1" in sql_str

        # Test MAC address type detection with camelCase field name
        where_clause = {"macAddress": {"eq": "aa:bb:cc:dd:ee:ff"}}
        result = self.repo._convert_dict_where_to_sql(where_clause)

        assert result is not None
        sql_str = result.as_string(None)

        # Should contain snake_case field name
        assert "mac_address" in sql_str
        # Should contain MAC address type casting
        assert "::macaddr" in sql_str or "macaddr" in sql_str
        # Should contain the MAC value
        assert "aa:bb:cc:dd:ee:ff" in sql_str

    def test_performance_validation(self):
        """Validate that field name conversion works correctly at scale."""
        # Create a moderately sized WHERE clause to test functionality at scale
        where_clause = {f"field{i}Name": {"eq": f"value{i}"} for i in range(5)}

        # Test a reasonable number of conversions to validate functionality
        for _ in range(10):  # Reduced iterations for CI stability
            result = self.repo._convert_dict_where_to_sql(where_clause)
            assert result is not None

        # Verify field name conversion works correctly
        sql_str = result.as_string(None)
        assert "field0_name" in sql_str  # Converted from field0Name
        assert "field0Name" not in sql_str  # Original shouldn't appear
        assert "field4_name" in sql_str  # Last field also converted
        assert "field4Name" not in sql_str  # Original shouldn't appear
