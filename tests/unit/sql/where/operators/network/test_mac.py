"""Comprehensive tests for MAC address operator SQL building.

Consolidated from test_mac_address_operators_sql_building.py and MAC parts of test_email_hostname_mac_complete.py.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.where.operators.mac_address import (
    build_mac_eq_sql,
    build_mac_in_sql,
    build_mac_neq_sql,
    build_mac_notin_sql,
)


class TestMacAddressBasicOperators:
    """Test basic MAC address operators (eq, neq, in, notin)."""

    def test_mac_eq(self):
        """Test MAC address equality operator."""
        path_sql = SQL("data->>'mac_address'")
        result = build_mac_eq_sql(path_sql, "00:11:22:33:44:55")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'mac_address')::macaddr = '00:11:22:33:44:55'::macaddr"

    def test_mac_neq(self):
        """Test MAC address inequality operator."""
        path_sql = SQL("data->>'mac_address'")
        result = build_mac_neq_sql(path_sql, "ff:ff:ff:ff:ff:ff")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'mac_address')::macaddr != 'ff:ff:ff:ff:ff:ff'::macaddr"

    def test_mac_in(self):
        """Test MAC address IN operator."""
        path_sql = SQL("data->>'device_mac'")
        result = build_mac_in_sql(
            path_sql, ["00:11:22:33:44:55", "aa:bb:cc:dd:ee:ff", "12:34:56:78:9a:bc"]
        )
        sql_str = result.as_string(None)
        expected = "(data->>'device_mac')::macaddr IN ('00:11:22:33:44:55'::macaddr, 'aa:bb:cc:dd:ee:ff'::macaddr, '12:34:56:78:9a:bc'::macaddr)"
        assert expected == sql_str

    def test_mac_notin(self):
        """Test MAC address NOT IN operator."""
        path_sql = SQL("data->>'mac_address'")
        result = build_mac_notin_sql(path_sql, ["00:00:00:00:00:00", "ff:ff:ff:ff:ff:ff"])
        sql_str = result.as_string(None)
        expected = "(data->>'mac_address')::macaddr NOT IN ('00:00:00:00:00:00'::macaddr, 'ff:ff:ff:ff:ff:ff'::macaddr)"
        assert expected == sql_str

    def test_build_mac_equality_sql(self) -> None:
        """Test MAC address equality SQL generation."""
        path_sql = SQL("data->>'mac_address'")
        mac_value = "00:11:22:33:44:55"

        result = build_mac_eq_sql(path_sql, mac_value)

        expected_sql = "(data->>'mac_address')::macaddr = '00:11:22:33:44:55'::macaddr"
        assert result.as_string(None) == expected_sql

    def test_build_mac_inequality_sql(self) -> None:
        """Test MAC address inequality SQL generation."""
        path_sql = SQL("data->>'mac_address'")
        mac_value = "aa:bb:cc:dd:ee:ff"

        result = build_mac_neq_sql(path_sql, mac_value)

        expected_sql = "(data->>'mac_address')::macaddr != 'aa:bb:cc:dd:ee:ff'::macaddr"
        assert result.as_string(None) == expected_sql

    def test_build_mac_in_list_sql(self) -> None:
        """Test MAC address IN list SQL generation."""
        path_sql = SQL("data->>'device_mac'")
        mac_list = ["00:11:22:33:44:55", "aa:bb:cc:dd:ee:ff", "ff:ee:dd:cc:bb:aa"]

        result = build_mac_in_sql(path_sql, mac_list)

        expected_sql = "(data->>'device_mac')::macaddr IN ('00:11:22:33:44:55'::macaddr, 'aa:bb:cc:dd:ee:ff'::macaddr, 'ff:ee:dd:cc:bb:aa'::macaddr)"
        assert result.as_string(None) == expected_sql

    def test_build_mac_not_in_list_sql(self) -> None:
        """Test MAC address NOT IN list SQL generation."""
        path_sql = SQL("data->>'device_mac'")
        mac_list = ["00:11:22:33:44:55", "aa:bb:cc:dd:ee:ff"]

        result = build_mac_notin_sql(path_sql, mac_list)

        expected_sql = "(data->>'device_mac')::macaddr NOT IN ('00:11:22:33:44:55'::macaddr, 'aa:bb:cc:dd:ee:ff'::macaddr)"
        assert result.as_string(None) == expected_sql

    def test_build_mac_single_item_in_list(self) -> None:
        """Test MAC address IN with single item."""
        path_sql = SQL("data->>'mac_address'")
        mac_list = ["00:11:22:33:44:55"]

        result = build_mac_in_sql(path_sql, mac_list)

        expected_sql = "(data->>'mac_address')::macaddr IN ('00:11:22:33:44:55'::macaddr)"
        assert result.as_string(None) == expected_sql

    def test_build_mac_different_separators(self) -> None:
        """Test MAC address with different separators (- vs :)."""
        path_sql = SQL("data->>'mac_address'")

        # Test with dash separators
        mac_dash = "00-11-22-33-44-55"
        result_dash = build_mac_eq_sql(path_sql, mac_dash)
        expected_dash = "(data->>'mac_address')::macaddr = '00-11-22-33-44-55'::macaddr"
        assert result_dash.as_string(None) == expected_dash

        # Test with colon separators
        mac_colon = "00:11:22:33:44:55"
        result_colon = build_mac_eq_sql(path_sql, mac_colon)
        expected_colon = "(data->>'mac_address')::macaddr = '00:11:22:33:44:55'::macaddr"
        assert result_colon.as_string(None) == expected_colon

    def test_build_mac_mixed_case(self) -> None:
        """Test MAC address with mixed case (PostgreSQL normalizes)."""
        path_sql = SQL("data->>'mac_address'")
        mac_mixed = "AaBb:CcDd:EeFf"

        result = build_mac_eq_sql(path_sql, mac_mixed)

        expected_sql = "(data->>'mac_address')::macaddr = 'AaBb:CcDd:EeFf'::macaddr"
        assert result.as_string(None) == expected_sql

    def test_build_mac_empty_list_handling(self) -> None:
        """Test MAC address operators with empty lists."""
        path_sql = SQL("data->>'mac_address'")
        empty_list = []

        # Empty IN list should generate valid SQL
        result_in = build_mac_in_sql(path_sql, empty_list)
        expected_in = "(data->>'mac_address')::macaddr IN ()"
        assert result_in.as_string(None) == expected_in

        # Empty NOT IN list should generate valid SQL
        result_notin = build_mac_notin_sql(path_sql, empty_list)
        expected_notin = "(data->>'mac_address')::macaddr NOT IN ()"
        assert result_notin.as_string(None) == expected_notin


class TestMacAddressSpecialCases:
    """Test MAC address operators with special cases."""

    def test_mac_uppercase(self):
        """Test MAC address with uppercase letters."""
        path_sql = SQL("data->>'mac_address'")
        result = build_mac_eq_sql(path_sql, "AA:BB:CC:DD:EE:FF")
        sql_str = result.as_string(None)
        assert "AA:BB:CC:DD:EE:FF" in sql_str

    def test_mac_mixed_case(self):
        """Test MAC address with mixed case."""
        path_sql = SQL("data->>'mac_address'")
        result = build_mac_eq_sql(path_sql, "Aa:Bb:Cc:Dd:Ee:Ff")
        sql_str = result.as_string(None)
        assert "Aa:Bb:Cc:Dd:Ee:Ff" in sql_str

    def test_mac_broadcast(self):
        """Test broadcast MAC address."""
        path_sql = SQL("data->>'mac_address'")
        result = build_mac_eq_sql(path_sql, "ff:ff:ff:ff:ff:ff")
        sql_str = result.as_string(None)
        assert "ff:ff:ff:ff:ff:ff" in sql_str

    def test_mac_zero(self):
        """Test zero MAC address."""
        path_sql = SQL("data->>'mac_address'")
        result = build_mac_eq_sql(path_sql, "00:00:00:00:00:00")
        sql_str = result.as_string(None)
        assert "00:00:00:00:00:00" in sql_str


class TestMACAddressValidation:
    """Test MAC address validation and error handling."""

    def test_mac_in_requires_list(self) -> None:
        """Test that MAC IN operator requires a list."""
        path_sql = SQL("data->>'mac_address'")

        with pytest.raises(TypeError, match="'in' operator requires a list"):
            build_mac_in_sql(path_sql, "not-a-list")  # type: ignore[arg-type]

    def test_mac_notin_requires_list(self) -> None:
        """Test that MAC NOT IN operator requires a list."""
        path_sql = SQL("data->>'mac_address'")

        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            build_mac_notin_sql(path_sql, "not-a-list")  # type: ignore[arg-type]
