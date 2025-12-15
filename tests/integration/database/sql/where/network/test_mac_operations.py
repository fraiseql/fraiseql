"""Integration tests for MAC address filtering operations.

Tests the SQL generation and database execution of MAC address filters
to ensure proper PostgreSQL macaddr type handling.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.operators import get_default_registry as get_operator_registry
from fraiseql.types import MacAddress

pytestmark = pytest.mark.database


@pytest.mark.integration
class TestMacAddressFilterOperations:
    """Test MAC address filtering with proper PostgreSQL macaddr type handling."""

    def test_mac_address_eq_with_different_formats(self) -> None:
        """Test MAC address equality with different input formats."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'mac_address'")

        # Test various MAC address formats that should all match
        test_cases = [
            "00:11:22:33:44:55",  # Colon (canonical)
            "00-11-22-33-44-55",  # Hyphen (Windows)
            "0011.2233.4455",  # Dot (Cisco)
            "001122334455",  # Bare
        ]

        for mac_format in test_cases:
            sql = registry.build_sql("eq", mac_format, path_sql, field_type=MacAddress)

            # Should cast both sides to macaddr for proper comparison
            sql_str = sql.as_string(None)
            assert "::macaddr" in sql_str, f"Missing macaddr cast for format {mac_format}"
            assert mac_format in sql_str

    def test_mac_address_case_insensitive_comparison(self) -> None:
        """Test that MAC address comparison is case insensitive."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'mac_address'")

        # Test case variations
        test_cases = [
            "AA:BB:CC:DD:EE:FF",  # Uppercase
            "aa:bb:cc:dd:ee:ff",  # Lowercase
            "Aa:Bb:Cc:Dd:Ee:Ff",  # Mixed case
        ]

        for mac_case in test_cases:
            sql = registry.build_sql("eq", mac_case, path_sql, field_type=MacAddress)

            sql_str = sql.as_string(None)
            # Should use macaddr casting for case-insensitive comparison
            assert "::macaddr" in sql_str, f"Missing macaddr cast for case {mac_case}"

    def test_mac_address_in_list_with_mixed_formats(self) -> None:
        """Test MAC address IN operation with mixed formats."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'mac_address'")

        # Mixed formats in a single IN clause
        mixed_macs = [
            "00:11:22:33:44:55",  # Colon
            "66-77-88-99-AA-BB",  # Hyphen
            "ccdd.eeff.0011",  # Dot
            "223344556677",  # Bare
        ]

        sql = registry.build_sql("in", mixed_macs, path_sql, field_type=MacAddress)

        sql_str = sql.as_string(None)
        # Should cast the field to macaddr
        assert "::macaddr" in sql_str
        # Should include all MAC addresses
        for mac in mixed_macs:
            assert mac in sql_str

    def test_mac_address_neq_operation(self) -> None:
        """Test MAC address not-equal operation."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'mac_address'")

        sql = registry.build_sql("neq", "00:11:22:33:44:55", path_sql, field_type=MacAddress)

        sql_str = sql.as_string(None)
        assert "::macaddr" in sql_str
        assert "!=" in sql_str
        assert "00:11:22:33:44:55" in sql_str

    def test_mac_address_nin_operation(self) -> None:
        """Test MAC address NOT IN operation."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'mac_address'")

        excluded_macs = ["00:11:22:33:44:55", "66-77-88-99-AA-BB"]

        sql = registry.build_sql("notin", excluded_macs, path_sql, field_type=MacAddress)

        sql_str = sql.as_string(None)
        assert "::macaddr" in sql_str
        assert "NOT IN" in sql_str
        for mac in excluded_macs:
            assert mac in sql_str

    def test_mac_address_isnull_operation(self) -> None:
        """Test MAC address NULL check operations."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'mac_address'")

        # Test IS NULL
        sql_null = registry.build_sql("isnull", True, path_sql, field_type=MacAddress)
        assert "IS NULL" in str(sql_null)

        # Test IS NOT NULL
        sql_not_null = registry.build_sql("isnull", False, path_sql, field_type=MacAddress)
        assert "IS NOT NULL" in str(sql_not_null)

    def test_mac_address_filter_excludes_pattern_operators(self) -> None:
        """Test that MacAddressFilter doesn't include problematic pattern operators."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'mac_address'")

        # These operators should not be available for MAC addresses
        problematic_ops = ["contains", "startswith", "endswith"]

        for op in problematic_ops:
            with pytest.raises(
                ValueError, match=f"Pattern operator '{op}' is not supported for MAC address fields"
            ):
                registry.build_sql(op, "00:11", path_sql, field_type=MacAddress)

    def test_mac_address_vs_string_field_behavior(self) -> None:
        """Test that MAC address fields get different treatment than string fields."""
        registry = get_operator_registry()
        path_sql = SQL("data->>'some_field'")

        # For MAC address fields, should use macaddr casting
        mac_sql = registry.build_sql("eq", "00:11:22:33:44:55", path_sql, field_type=MacAddress)
        mac_sql_str = mac_sql.as_string(None)
        assert "::macaddr" in mac_sql_str

        # For regular string fields, should NOT use macaddr casting
        string_sql = registry.build_sql("eq", "00:11:22:33:44:55", path_sql, field_type=str)
        string_sql_str = string_sql.as_string(None)
        assert "::macaddr" not in string_sql_str

    def test_mac_address_normalization_in_sql_generation(self) -> None:
        """Test that SQL properly handles MAC address format normalization.

        This test should pass once MacAddressOperatorStrategy is implemented.
        It verifies that different MAC formats are properly normalized at the SQL level.
        """
        registry = get_operator_registry()
        path_sql = SQL("data->>'mac_address'")

        # These different formats should generate equivalent SQL
        # when compared to the same canonical MAC address
        formats = [
            "00:11:22:33:44:55",  # Canonical
            "00-11-22-33-44-55",  # Hyphen
            "0011.2233.4455",  # Cisco dot
        ]

        for fmt in formats:
            sql = registry.build_sql("eq", fmt, path_sql, field_type=MacAddress)

            sql_str = sql.as_string(None)

            # All should use proper macaddr casting that enables normalization
            assert "::macaddr" in sql_str
            # The format should be preserved in the literal but casting handles normalization
            assert fmt in sql_str
