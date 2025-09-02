#!/usr/bin/env python3
"""Test MAC address operators SQL building.

This module tests the clean MAC address operator functions that generate
proper PostgreSQL SQL with ::macaddr casting.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.where.operators import mac_address


class TestMACAddressSQLBuilding:
    """Test MAC address SQL building functions with proper PostgreSQL macaddr casting."""

    def test_build_mac_equality_sql(self):
        """Test MAC address equality SQL generation."""
        path_sql = SQL("data->>'mac_address'")
        mac_value = "00:11:22:33:44:55"

        result = mac_address.build_mac_eq_sql(path_sql, mac_value)

        expected_sql = "(data->>'mac_address')::macaddr = '00:11:22:33:44:55'::macaddr"
        assert result.as_string(None) == expected_sql

    def test_build_mac_inequality_sql(self):
        """Test MAC address inequality SQL generation."""
        path_sql = SQL("data->>'mac_address'")
        mac_value = "aa:bb:cc:dd:ee:ff"

        result = mac_address.build_mac_neq_sql(path_sql, mac_value)

        expected_sql = "(data->>'mac_address')::macaddr != 'aa:bb:cc:dd:ee:ff'::macaddr"
        assert result.as_string(None) == expected_sql

    def test_build_mac_in_list_sql(self):
        """Test MAC address IN list SQL generation."""
        path_sql = SQL("data->>'device_mac'")
        mac_list = ["00:11:22:33:44:55", "aa:bb:cc:dd:ee:ff", "ff:ee:dd:cc:bb:aa"]

        result = mac_address.build_mac_in_sql(path_sql, mac_list)

        expected_sql = "(data->>'device_mac')::macaddr IN ('00:11:22:33:44:55'::macaddr, 'aa:bb:cc:dd:ee:ff'::macaddr, 'ff:ee:dd:cc:bb:aa'::macaddr)"
        assert result.as_string(None) == expected_sql

    def test_build_mac_not_in_list_sql(self):
        """Test MAC address NOT IN list SQL generation."""
        path_sql = SQL("data->>'device_mac'")
        mac_list = ["00:11:22:33:44:55", "aa:bb:cc:dd:ee:ff"]

        result = mac_address.build_mac_notin_sql(path_sql, mac_list)

        expected_sql = "(data->>'device_mac')::macaddr NOT IN ('00:11:22:33:44:55'::macaddr, 'aa:bb:cc:dd:ee:ff'::macaddr)"
        assert result.as_string(None) == expected_sql

    def test_build_mac_single_item_in_list(self):
        """Test MAC address IN with single item."""
        path_sql = SQL("data->>'mac_address'")
        mac_list = ["00:11:22:33:44:55"]

        result = mac_address.build_mac_in_sql(path_sql, mac_list)

        expected_sql = "(data->>'mac_address')::macaddr IN ('00:11:22:33:44:55'::macaddr)"
        assert result.as_string(None) == expected_sql

    def test_build_mac_different_separators(self):
        """Test MAC address with different separators (- vs :)."""
        path_sql = SQL("data->>'mac_address'")

        # Test with dash separators
        mac_dash = "00-11-22-33-44-55"
        result_dash = mac_address.build_mac_eq_sql(path_sql, mac_dash)
        expected_dash = "(data->>'mac_address')::macaddr = '00-11-22-33-44-55'::macaddr"
        assert result_dash.as_string(None) == expected_dash

        # Test with colon separators
        mac_colon = "00:11:22:33:44:55"
        result_colon = mac_address.build_mac_eq_sql(path_sql, mac_colon)
        expected_colon = "(data->>'mac_address')::macaddr = '00:11:22:33:44:55'::macaddr"
        assert result_colon.as_string(None) == expected_colon

    def test_build_mac_mixed_case(self):
        """Test MAC address with mixed case (PostgreSQL normalizes)."""
        path_sql = SQL("data->>'mac_address'")
        mac_mixed = "AaBb:CcDd:EeFf"

        result = mac_address.build_mac_eq_sql(path_sql, mac_mixed)

        expected_sql = "(data->>'mac_address')::macaddr = 'AaBb:CcDd:EeFf'::macaddr"
        assert result.as_string(None) == expected_sql

    def test_build_mac_empty_list_handling(self):
        """Test MAC address operators with empty lists."""
        path_sql = SQL("data->>'mac_address'")
        empty_list = []

        # Empty IN list should generate valid SQL
        result_in = mac_address.build_mac_in_sql(path_sql, empty_list)
        expected_in = "(data->>'mac_address')::macaddr IN ()"
        assert result_in.as_string(None) == expected_in

        # Empty NOT IN list should generate valid SQL
        result_notin = mac_address.build_mac_notin_sql(path_sql, empty_list)
        expected_notin = "(data->>'mac_address')::macaddr NOT IN ()"
        assert result_notin.as_string(None) == expected_notin


class TestMACAddressValidation:
    """Test MAC address validation and error handling."""

    def test_mac_in_requires_list(self):
        """Test that MAC IN operator requires a list."""
        path_sql = SQL("data->>'mac_address'")

        with pytest.raises(TypeError, match="'in' operator requires a list"):
            mac_address.build_mac_in_sql(path_sql, "not-a-list")

    def test_mac_notin_requires_list(self):
        """Test that MAC NOT IN operator requires a list."""
        path_sql = SQL("data->>'mac_address'")

        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            mac_address.build_mac_notin_sql(path_sql, "not-a-list")
