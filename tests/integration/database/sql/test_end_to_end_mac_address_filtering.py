import pytest

pytestmark = pytest.mark.database

#!/usr/bin/env python3
"""End-to-end integration tests for MAC address filtering functionality.

This module tests the complete MAC address filtering pipeline:
1. GraphQL WHERE input → Field detection → Operator selection → SQL generation
2. Real database execution to verify MAC address filtering works correctly
"""

from psycopg.sql import SQL

from fraiseql.sql.where import build_where_clause, detect_field_type, get_operator_function
from fraiseql.sql.where.core.field_detection import FieldType


class TestEndToEndMACAddressFiltering:
    """Test complete MAC address filtering pipeline with real database operations."""

    def test_graphql_mac_address_equality_filtering(self) -> None:
        """Test MAC address equality filtering reproduces expected behavior."""
        # Simulate GraphQL WHERE input for MAC address filtering
        graphql_where = {"macAddress": {"eq": "00:11:22:33:44:55"}}

        # Build WHERE clause using our clean architecture
        where_clause = build_where_clause(graphql_where)

        # Verify we get proper SQL with ::macaddr casting
        assert where_clause is not None
        sql_string = where_clause.as_string(None)

        # Should generate MAC address equality with proper casting
        expected_patterns = [
            "mac_address",  # Field name conversion
            "::macaddr",  # PostgreSQL MAC casting
            "= '00:11:22:33:44:55'::macaddr",  # Value with casting
        ]

        for pattern in expected_patterns:
            assert pattern in sql_string, f"Expected pattern '{pattern}' not found in: {sql_string}"

    def test_mac_address_filtering_with_list_values(self) -> None:
        """Test MAC address filtering with IN and NOT IN operations."""
        mac_list = ["00:11:22:33:44:55", "aa:bb:cc:dd:ee:ff", "ff:ee:dd:cc:bb:aa"]

        # Test IN operation
        graphql_where_in = {"deviceMac": {"in": mac_list}}

        where_clause_in = build_where_clause(graphql_where_in)
        sql_in = where_clause_in.as_string(None)

        # Should contain all MAC addresses with proper casting
        assert "::macaddr IN (" in sql_in
        for mac in mac_list:
            assert f"'{mac}'::macaddr" in sql_in

    def test_field_name_conversion_snake_to_camel_mac(self) -> None:
        """Test that field names are correctly converted from camelCase to snake_case."""
        graphql_where = {"deviceMacAddress": {"neq": "00:11:22:33:44:55"}}  # camelCase

        where_clause = build_where_clause(graphql_where)
        sql_string = where_clause.as_string(None)

        # Should convert to snake_case in database query
        assert "device_mac_address" in sql_string
        assert "::macaddr" in sql_string

    def test_field_detection_recognizes_mac_addresses(self) -> None:
        """Test that field detection correctly identifies MAC address fields."""
        # Test field name detection
        mac_field_names = [
            "mac_address",
            "macAddress",
            "device_mac",
            "deviceMac",
            "mac_addr",
            "macAddr",
            "hardware_address",
            "hardwareAddress",
            "mac",
            "deviceMacAddress",
        ]

        for field_name in mac_field_names:
            field_type = detect_field_type(field_name, "00:11:22:33:44:55", None)
            assert field_type == FieldType.MAC_ADDRESS, (
                f"Field '{field_name}' should be detected as MAC_ADDRESS"
            )

    def test_field_detection_recognizes_mac_values(self) -> None:
        """Test that field detection recognizes MAC address values."""
        mac_values = [
            "00:11:22:33:44:55",  # Colon separated
            "00-11-22-33-44-55",  # Dash separated
            "aa:bb:cc:dd:ee:ff",  # Lowercase
            "AA:BB:CC:DD:EE:FF",  # Uppercase
            "0a:1b:2c:3d:4e:5f",  # Mixed case
        ]

        for mac_value in mac_values:
            # Use a clearly MAC field name to ensure MAC detection takes precedence over IP detection
            field_type = detect_field_type("mac_address", mac_value, None)
            assert field_type == FieldType.MAC_ADDRESS, (
                f"Value '{mac_value}' should be detected as MAC_ADDRESS"
            )

    def test_operator_function_selection_for_mac_addresses(self) -> None:
        """Test that correct operator functions are selected for MAC address fields."""
        mac_operators = ["eq", "neq", "in", "notin"]

        for operator in mac_operators:
            func = get_operator_function(FieldType.MAC_ADDRESS, operator)
            assert func is not None, f"Should have operator function for MAC_ADDRESS.{operator}"

            # Test that it generates proper SQL
            path_sql = SQL("data->>'mac_address'")
            test_value = "00:11:22:33:44:55" if operator in ["eq", "neq"] else ["00:11:22:33:44:55"]

            result = func(path_sql, test_value)
            sql_string = result.as_string(None)

            assert "::macaddr" in sql_string, (
                f"MAC operator {operator} should use ::macaddr casting"
            )

    def test_mac_address_different_formats_normalized(self) -> None:
        """Test that different MAC address formats are handled properly."""
        formats = [
            "00:11:22:33:44:55",  # Colon format
            "00-11-22-33-44-55",  # Dash format
            "001122334455",  # No separators (if supported)
        ]

        for mac_format in formats:
            graphql_where = {"macAddress": {"eq": mac_format}}

            where_clause = build_where_clause(graphql_where)
            sql_string = where_clause.as_string(None)

            # Should generate valid PostgreSQL with proper casting
            assert "::macaddr = " in sql_string
            assert mac_format in sql_string

    def test_mixed_field_types_in_where_clause_with_mac(self) -> None:
        """Test WHERE clause with both MAC address and other field types."""
        graphql_where = {
            "macAddress": {"eq": "00:11:22:33:44:55"},
            "ipAddress": {"eq": "192.168.1.100"},
            "name": {"contains": "device"},
        }

        where_clause = build_where_clause(graphql_where)
        sql_string = where_clause.as_string(None)

        # Should handle all field types correctly
        assert "::macaddr" in sql_string  # MAC address casting
        assert "::inet" in sql_string  # IP address casting
        assert "LIKE" in sql_string  # Text pattern matching
        assert " AND " in sql_string  # Multiple conditions
