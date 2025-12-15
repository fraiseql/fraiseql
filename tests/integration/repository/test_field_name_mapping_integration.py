"""Integration tests for field name mapping in repository WHERE clauses.

These tests verify that the field name conversion works end-to-end with
the complete FraiseQL stack, including SQL generation and type detection.

NOTE: These tests are currently SKIPPED because they test the
`_convert_dict_where_to_sql()` method which was planned but never implemented.
The repository has a `_convert_field_name_to_database()` method, but it's not
called by any WHERE clause building logic. This functionality may be implemented
in the future.
"""

from unittest.mock import MagicMock

import pytest
from psycopg_pool import AsyncConnectionPool

from fraiseql.db import FraiseQLRepository

pytestmark = [
    pytest.mark.integration,
]


class TestFieldNameMappingIntegration:
    """Integration tests for WHERE clause field name conversion."""

    def setup_method(self) -> None:
        """Set up test repository with mock pool."""
        self.mock_pool = MagicMock(spec=AsyncConnectionPool)
        self.repo = FraiseQLRepository(self.mock_pool)

    def test_sql_generation_integration(self) -> None:
        """Test that SQL generation works correctly with field name conversion.

        This focuses on the SQL generation layer without complex async mocking.
        """
        # Test camelCase field names in WHERE clause
        where_clause = {
            "ipAddress": {"eq": "192.168.1.1"},  # camelCase
            "deviceName": {"contains": "router"},  # camelCase
        }

        # Normalize WHERE clause (this applies field name conversion)
        where_obj = self.repo._normalize_where(where_clause, "test_view", None)
        # Generate SQL from normalized WhereClause
        result, params = where_obj.to_sql()
        assert result is not None

        sql_str = result.as_string(None)

        # Should contain snake_case field names in the SQL
        assert "ip_address" in sql_str
        assert "device_name" in sql_str

        # Should NOT contain camelCase names in SQL
        assert "ipAddress" not in sql_str
        assert "deviceName" not in sql_str

        # Should contain the values
        assert "192.168.1.1" in str(params)
        assert "router" in str(params)

    def test_backward_compatibility_integration(self) -> None:
        """Test that existing snake_case field names continue to work."""
        where_clause = {
            "ip_address": {"eq": "10.0.0.1"},  # snake_case (existing usage)
            "status": {"eq": "active"},  # snake_case (existing usage)
        }

        where_obj = self.repo._normalize_where(where_clause, "test_view", None)
        result, params = where_obj.to_sql()
        assert result is not None

        sql_str = result.as_string(None)

        # Should work unchanged - snake_case names should remain
        assert "ip_address" in sql_str
        assert "status" in sql_str
        assert "10.0.0.1" in str(params)
        assert "active" in str(params)

    def test_mixed_case_sql_generation(self) -> None:
        """Test mixed camelCase and snake_case fields in same query."""
        where_clause = {
            "ipAddress": {"eq": "192.168.1.1"},  # camelCase (should be converted)
            "status": {"eq": "active"},  # snake_case (should remain)
            "deviceName": {"contains": "switch"},  # camelCase (should be converted)
            "created_at": {"gte": "2025-01-01"},  # snake_case (should remain)
        }

        where_obj = self.repo._normalize_where(where_clause, "test_view", None)
        result, _params = where_obj.to_sql()
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

    def test_complex_where_clause_field_conversion(self) -> None:
        """Test complex WHERE clauses with multiple operators per field."""
        where_clause = {
            "ipAddress": {"eq": "192.168.1.1", "neq": "127.0.0.1"},
            "devicePort": {"gte": 1024, "lt": 65536},
            "macAddress": {"eq": "aa:bb:cc:dd:ee:ff"},
        }

        # Convert using the repository method
        where_obj = self.repo._normalize_where(where_clause, "test_view", None)
        result, params = where_obj.to_sql()
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
        assert "192.168.1.1" in str(params)
        assert "127.0.0.1" in str(params)
        assert "1024" in str(params)
        assert "65536" in str(params)
        assert "aa:bb:cc:dd:ee:ff" in str(params)

    def test_field_conversion_with_type_detection(self) -> None:
        """Test that field conversion works correctly with FraiseQL's type detection.

        This verifies that IP addresses, MAC addresses, and other special types
        are still detected correctly after field name conversion.
        """
        # Test IP address type detection with camelCase field name
        where_clause = {"ipAddress": {"eq": "192.168.1.1"}}
        where_obj = self.repo._normalize_where(where_clause, "test_view", None)
        result, params = where_obj.to_sql()

        assert result is not None
        sql_str = result.as_string(None)

        # Should contain snake_case field name
        assert "ip_address" in sql_str
        # Should contain the IP value
        assert "192.168.1.1" in str(params)

        # Test MAC address type detection with camelCase field name
        where_clause = {"macAddress": {"eq": "aa:bb:cc:dd:ee:ff"}}
        where_obj = self.repo._normalize_where(where_clause, "test_view", None)
        result, params = where_obj.to_sql()

        assert result is not None
        sql_str = result.as_string(None)

        # Should contain snake_case field name
        assert "mac_address" in sql_str
        # Should contain the MAC value
        assert "aa:bb:cc:dd:ee:ff" in str(params)

    def test_deep_nested_field_conversion(self) -> None:
        """Test that deeply nested field names are converted correctly."""
        # Test 3 levels of nesting with camelCase field names
        where_clause = {
            "machine": {
                "network": {
                    "ipAddress": {"eq": "192.168.1.1"},
                    "macAddress": {"eq": "aa:bb:cc:dd:ee:ff"},
                }
            }
        }

        where_obj = self.repo._normalize_where(where_clause, "test_view", None)
        result, params = where_obj.to_sql()
        assert result is not None

        sql_str = result.as_string(None)

        # Should contain properly nested JSONB paths with converted field names
        assert "data" in sql_str  # JSONB column
        assert "machine" in sql_str
        assert "network" in sql_str
        assert "ip_address" in sql_str  # Converted from ipAddress
        assert "mac_address" in sql_str  # Converted from macAddress

        # Should NOT contain original camelCase names
        assert "ipAddress" not in sql_str
        assert "macAddress" not in sql_str

        # Should contain the values
        assert "192.168.1.1" in str(params)
        assert "aa:bb:cc:dd:ee:ff" in str(params)

    def test_performance_validation(self) -> None:
        """Validate that field name conversion works correctly at scale."""
        # Create a moderately sized WHERE clause to test functionality at scale
        where_clause = {f"field{i}Name": {"eq": f"value{i}"} for i in range(5)}

        # Test a reasonable number of conversions to validate functionality
        result = None
        for _ in range(10):  # Reduced iterations for CI stability
            where_obj = self.repo._normalize_where(where_clause, "test_view", None)
            result, _params = where_obj.to_sql()
            assert result is not None

        # Verify field name conversion works correctly
        assert result is not None
        sql_str = result.as_string(None)
        assert "field0_name" in sql_str  # Converted from field0Name
        assert "field0Name" not in sql_str  # Original shouldn't appear
        assert "field4_name" in sql_str  # Last field also converted
        assert "field4Name" not in sql_str  # Original shouldn't appear
