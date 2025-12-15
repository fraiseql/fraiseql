"""Integration tests for operator strategy selection and SQL generation.

This module tests the operator strategy system end-to-end, ensuring that:
- Correct strategies are selected for each operator and field type
- SQL generation produces correct output for special types
- JSONB type casting works correctly for Network, LTree, DateRange, and MacAddress types

Test Coverage:
- Network types: 4 core tests (eq, isPrivate, isPublic, inSubnet)
- LTree types: 3 core tests (eq, ancestor_of, matches_lquery)
- DateRange types: 3 core tests (eq, contains_date, overlaps)
- MacAddress types: 2 core tests (eq, in)

These are integration tests that verify the operator registry and strategy system work together.
"""

import logging
from dataclasses import dataclass

import pytest
from tests.helpers.sql_rendering import render_sql_for_testing

from fraiseql.sql.operators import get_default_registry as get_operator_registry
from fraiseql.types import DateRange, IpAddress, LTree, MacAddress

logger = logging.getLogger(__name__)

pytestmark = pytest.mark.integration


# Test Data Models
@dataclass
class NetworkDevice:
    """Test model with Network (IpAddress) fields in JSONB storage."""

    id: str
    name: str
    ip_address: IpAddress
    secondary_ip: IpAddress | None = None


@dataclass
class HierarchicalPath:
    """Test model with LTree fields in JSONB storage."""

    id: str
    name: str
    path: LTree
    category: str | None = None


@dataclass
class TimePeriod:
    """Test model with DateRange fields in JSONB storage."""

    id: str
    name: str
    period: DateRange
    status: str | None = None


@dataclass
class NetworkInterface:
    """Test model with MacAddress fields in JSONB storage."""

    id: str
    name: str
    mac: MacAddress
    port: int | None = None


@pytest.mark.core
class TestTier1NetworkTypes:
    """Core Network type filtering tests - must pass for basic functionality."""

    def test_network_eq_operator_jsonb_flat(self) -> None:
        """RED: Test Network eq operator with JSONB flat storage fails initially.

        This test reproduces the core issue where IP address equality filtering
        returns empty results due to improper JSONB->text->inet casting.
        """
        # Test the core issue directly using the operator strategies
        registry = get_operator_registry()

        # Test the exact failing case: IP address equality without field_type
        # (this simulates the production failure scenario)
        strategy_no_field_type = registry.get_strategy("eq", field_type=None)

        from psycopg.sql import SQL

        jsonb_path_sql = SQL("(data ->> 'ip_address')")

        # This is the core fix - should work now even without field_type
        result = strategy_no_field_type.build_sql("eq", "8.8.8.8", jsonb_path_sql, field_type=None)

        sql_str = render_sql_for_testing(result)
        logger.debug(f"Network eq SQL (no field_type): {sql_str}")

        # The critical fix: should now detect IP and apply proper casting
        assert "::inet" in sql_str, f"Must cast JSONB field to inet for IP operations: {sql_str}"
        assert "8.8.8.8" in sql_str, "Should include IP address in filter"

        # Also test with field_type for backward compatibility
        strategy_with_field_type = registry.get_strategy("eq", field_type=IpAddress)
        result_with_type = strategy_with_field_type.build_sql(
            "eq", "8.8.8.8", jsonb_path_sql, field_type=IpAddress
        )

        sql_with_type = render_sql_for_testing(result_with_type)
        logger.debug(f"Network eq SQL (with field_type): {sql_with_type}")

        # Both should now work and produce similar results
        assert "::inet" in sql_with_type, "Should work with field_type too"

    def test_network_isprivate_operator_jsonb_flat(self) -> None:
        """RED: Test Network isPrivate operator with JSONB flat storage.

        Tests private IP detection on JSONB-stored IP addresses.
        This commonly fails due to missing ::inet casting.
        """
        # Get the operator registry to test strategy selection
        registry = get_operator_registry()

        # Test that NetworkOperatorStrategy is selected for isPrivate
        strategy = registry.get_strategy("isPrivate", IpAddress)
        assert strategy is not None

        # Test SQL generation for private IP detection
        from psycopg.sql import SQL

        path_sql = SQL("(data ->> 'ip_address')")
        result = strategy.build_sql("isPrivate", True, path_sql, field_type=IpAddress)

        sql_str = render_sql_for_testing(result)

        # Must include proper inet casting
        assert "::inet" in sql_str, "Private IP detection requires inet casting"

        # Should use CIDR range checks for private IPs (no inet_public() in PostgreSQL)
        assert "10.0.0.0/8" in sql_str, f"isPrivate should check RFC 1918 ranges, got: {sql_str}"
        assert "192.168.0.0/16" in sql_str, (
            f"isPrivate should check RFC 1918 ranges, got: {sql_str}"
        )

    def test_network_ispublic_operator_jsonb_flat(self) -> None:
        """RED: Test Network isPublic operator with JSONB flat storage.

        Tests public IP detection as inverse of private IP logic.
        """
        registry = get_operator_registry()
        strategy = registry.get_strategy("isPublic", IpAddress)

        from psycopg.sql import SQL

        path_sql = SQL("(data ->> 'ip_address')")
        result = strategy.build_sql("isPublic", True, path_sql, field_type=IpAddress)

        sql_str = render_sql_for_testing(result)

        # Must include proper inet casting
        assert "::inet" in sql_str, "Public IP detection requires inet casting"

        # Should use CIDR range checks with NOT for public IPs (no inet_public() in PostgreSQL)
        assert "NOT" in sql_str, f"isPublic should use NOT (private ranges), got: {sql_str}"
        assert "10.0.0.0/8" in sql_str, f"isPublic should check RFC 1918 ranges, got: {sql_str}"
        assert "192.168.0.0/16" in sql_str, f"isPublic should check RFC 1918 ranges, got: {sql_str}"

    def test_network_insubnet_operator_jsonb_flat(self) -> None:
        """RED: Test Network inSubnet operator with JSONB flat storage.

        Tests subnet matching using PostgreSQL inet subnet operators.
        Critical for network-aware applications.
        """
        registry = get_operator_registry()
        strategy = registry.get_strategy("inSubnet", IpAddress)

        from psycopg.sql import SQL

        path_sql = SQL("(data ->> 'ip_address')")
        result = strategy.build_sql("inSubnet", "192.168.0.0/16", path_sql, field_type=IpAddress)

        sql_str = render_sql_for_testing(result)

        # Must use PostgreSQL subnet matching operator
        assert "<<=" in sql_str, "Subnet matching requires PostgreSQL <<= operator"
        assert "::inet" in sql_str, "Subnet matching requires inet casting"
        assert "192.168.0.0/16" in sql_str, "Should include subnet parameter"


@pytest.mark.core
class TestTier1LTreeTypes:
    """Core LTree hierarchical path filtering tests."""

    def test_ltree_eq_operator_jsonb_flat(self) -> None:
        """RED: Test LTree eq operator with JSONB flat storage fails initially.

        Tests basic LTree path equality with proper ltree casting.
        """
        registry = get_operator_registry()
        strategy = registry.get_strategy("eq", LTree)

        from psycopg.sql import SQL

        path_sql = SQL("(data ->> 'path')")
        result = strategy.build_sql("eq", "top.middle.bottom", path_sql, field_type=LTree)

        sql_str = render_sql_for_testing(result)

        # Must include proper ltree casting for JSONB fields
        assert "::ltree" in sql_str, "LTree equality requires ltree casting"
        assert "top.middle.bottom" in sql_str, "Should include path value"

    def test_ltree_ancestor_of_operator_jsonb_flat(self) -> None:
        """RED: Test LTree ancestor_of operator with JSONB flat storage.

        Tests hierarchical ancestor relationship using PostgreSQL ltree operators.
        """
        registry = get_operator_registry()
        strategy = registry.get_strategy("ancestor_of", LTree)

        from psycopg.sql import SQL

        path_sql = SQL("(data ->> 'path')")
        result = strategy.build_sql("ancestor_of", "top.middle.bottom", path_sql, field_type=LTree)

        sql_str = render_sql_for_testing(result)

        # Must use PostgreSQL ltree ancestor operator
        assert "@>" in sql_str, "Ancestor relationship requires PostgreSQL @> operator"
        assert "::ltree" in sql_str, "Ancestor operation requires ltree casting"

    def test_ltree_matches_lquery_operator_jsonb_flat(self) -> None:
        """RED: Test LTree matches_lquery operator with JSONB flat storage.

        Tests pattern matching using ltree lquery patterns.
        """
        registry = get_operator_registry()
        strategy = registry.get_strategy("matches_lquery", LTree)

        from psycopg.sql import SQL

        path_sql = SQL("(data ->> 'path')")
        result = strategy.build_sql("matches_lquery", "top.*", path_sql, field_type=LTree)

        sql_str = render_sql_for_testing(result)

        # Must use PostgreSQL ltree pattern matching
        assert "~" in sql_str, "Pattern matching requires PostgreSQL ~ operator"
        assert "::ltree" in sql_str, "Pattern matching requires ltree casting"
        assert "::lquery" in sql_str, "Pattern matching requires lquery casting"


@pytest.mark.core
class TestTier1DateRangeTypes:
    """Core DateRange temporal filtering tests."""

    def test_daterange_eq_operator_jsonb_flat(self) -> None:
        """RED: Test DateRange eq operator with JSONB flat storage fails initially.

        Tests basic DateRange equality with proper daterange casting.
        """
        registry = get_operator_registry()
        strategy = registry.get_strategy("eq", DateRange)

        from psycopg.sql import SQL

        path_sql = SQL("(data ->> 'period')")
        result = strategy.build_sql("eq", "[2024-01-01,2024-12-31)", path_sql, field_type=DateRange)

        sql_str = render_sql_for_testing(result)

        # Must include proper daterange casting for JSONB fields
        assert "::daterange" in sql_str, "DateRange equality requires daterange casting"
        assert "2024-01-01" in sql_str, "Should include start date"
        assert "2024-12-31" in sql_str, "Should include end date"

    def test_daterange_contains_date_operator_jsonb_flat(self) -> None:
        """RED: Test DateRange contains_date operator with JSONB flat storage.

        Tests whether a range contains a specific date using PostgreSQL range operators.
        """
        registry = get_operator_registry()
        strategy = registry.get_strategy("contains_date", DateRange)

        from psycopg.sql import SQL

        path_sql = SQL("(data ->> 'period')")
        result = strategy.build_sql("contains_date", "2024-06-15", path_sql, field_type=DateRange)

        sql_str = render_sql_for_testing(result)

        # Must use PostgreSQL range contains operator
        assert "@>" in sql_str, "Range contains requires PostgreSQL @> operator"
        assert "::daterange" in sql_str, "Contains operation requires daterange casting"
        assert "::date" in sql_str, "Date parameter requires date casting"

    def test_daterange_overlaps_operator_jsonb_flat(self) -> None:
        """RED: Test DateRange overlaps operator with JSONB flat storage.

        Tests range overlap detection using PostgreSQL range operators.
        """
        registry = get_operator_registry()
        strategy = registry.get_strategy("overlaps", DateRange)

        from psycopg.sql import SQL

        path_sql = SQL("(data ->> 'period')")
        result = strategy.build_sql(
            "overlaps", "[2024-06-01,2024-06-30)", path_sql, field_type=DateRange
        )

        sql_str = render_sql_for_testing(result)

        # Must use PostgreSQL range overlap operator
        assert "&&" in sql_str, "Range overlap requires PostgreSQL && operator"
        assert "::daterange" in sql_str, "Overlap operation requires daterange casting"


@pytest.mark.core
class TestTier1MacAddressTypes:
    """Core MAC address filtering tests."""

    def test_mac_eq_operator_jsonb_flat(self) -> None:
        """RED: Test MAC address eq operator with JSONB flat storage fails initially.

        Tests basic MAC address equality with proper macaddr casting.
        """
        registry = get_operator_registry()
        strategy = registry.get_strategy("eq", MacAddress)

        from psycopg.sql import SQL

        path_sql = SQL("(data ->> 'mac')")
        result = strategy.build_sql("eq", "00:11:22:33:44:55", path_sql, field_type=MacAddress)

        sql_str = render_sql_for_testing(result)

        # Must include proper macaddr casting for JSONB fields
        assert "::macaddr" in sql_str, "MAC equality requires macaddr casting"
        assert "00:11:22:33:44:55" in sql_str, "Should include MAC address"

    def test_mac_in_operator_jsonb_flat(self) -> None:
        """RED: Test MAC address in operator with JSONB flat storage.

        Tests MAC address list filtering with proper macaddr casting.
        """
        registry = get_operator_registry()
        strategy = registry.get_strategy("in", MacAddress)

        from psycopg.sql import SQL

        path_sql = SQL("(data ->> 'mac')")
        mac_list = ["00:11:22:33:44:55", "AA:BB:CC:DD:EE:FF"]
        result = strategy.build_sql("in", mac_list, path_sql, field_type=MacAddress)

        sql_str = render_sql_for_testing(result)

        # Must use proper IN clause with macaddr casting
        assert " IN " in sql_str, "MAC in operator requires IN clause"
        assert "::macaddr" in sql_str, "MAC in operation requires macaddr casting"
        assert "00:11:22:33:44:55" in sql_str, "Should include first MAC address"
        assert "AA:BB:CC:DD:EE:FF" in sql_str, "Should include second MAC address"


@pytest.mark.core
class TestTier1FieldTypePassthrough:
    """Test that field_type information propagates correctly through the system."""

    def test_field_type_propagation_network(self) -> None:
        """Test that IpAddress field_type propagates to operator strategies."""
        # This tests the critical field_type propagation issue
        from psycopg.sql import SQL

        from fraiseql.sql.where_generator import build_operator_composed

        # Test with explicit field_type (should work)
        path_sql = SQL("(data ->> 'ip_address')")
        result = build_operator_composed(path_sql, "isPrivate", True, IpAddress)

        sql_str = render_sql_for_testing(result)
        assert "::inet" in sql_str, "Field type should enable proper inet casting"

        # Test without field_type (may fail - this reveals the issue)
        try:
            _result_no_type = build_operator_composed(path_sql, "isPrivate", True, None)
            # If this doesn't fail, the implementation is already robust
            # The issue is when field_type=None prevents proper strategy selection
        except Exception as e:
            # Expected behavior if field_type is required for network operators
            # The error should mention field type detection issue or IP address requirement
            error_msg = str(e).lower()
            assert "field type" in error_msg or "ip address" in error_msg

    def test_field_type_propagation_ltree(self) -> None:
        """Test that LTree field_type propagates to operator strategies."""
        from psycopg.sql import SQL

        from fraiseql.sql.where_generator import build_operator_composed

        path_sql = SQL("(data ->> 'path')")
        result = build_operator_composed(path_sql, "ancestor_of", "top.middle", LTree)

        sql_str = render_sql_for_testing(result)
        assert "::ltree" in sql_str, "Field type should enable proper ltree casting"

    def test_field_type_propagation_daterange(self) -> None:
        """Test that DateRange field_type propagates to operator strategies."""
        from psycopg.sql import SQL

        from fraiseql.sql.where_generator import build_operator_composed

        path_sql = SQL("(data ->> 'period')")
        result = build_operator_composed(path_sql, "contains_date", "2024-06-15", DateRange)

        sql_str = render_sql_for_testing(result)
        assert "::daterange" in sql_str, "Field type should enable proper daterange casting"

    def test_field_type_propagation_macaddress(self) -> None:
        """Test that MacAddress field_type propagates to operator strategies."""
        from psycopg.sql import SQL

        from fraiseql.sql.where_generator import build_operator_composed

        path_sql = SQL("(data ->> 'mac')")
        result = build_operator_composed(path_sql, "eq", "00:11:22:33:44:55", MacAddress)

        sql_str = render_sql_for_testing(result)
        assert "::macaddr" in sql_str, "Field type should enable proper macaddr casting"


@pytest.mark.core
class TestTier1StrategySelection:
    """Test that the correct operator strategies are selected for each special type."""

    def test_network_strategy_selection(self) -> None:
        """Test that network operators select NetworkOperatorStrategy."""
        registry = get_operator_registry()

        # Network-specific operators should always select NetworkOperatorStrategy
        network_ops = ["inSubnet", "inRange", "isPrivate", "isPublic", "isIPv4", "isIPv6"]

        for op in network_ops:
            strategy = registry.get_strategy(op, IpAddress)
            assert strategy.__class__.__name__ == "NetworkOperatorStrategy", (
                f"Operator {op} should use NetworkOperatorStrategy"
            )

    def test_ltree_strategy_selection(self) -> None:
        """Test that LTree operators select LTreeOperatorStrategy."""
        registry = get_operator_registry()

        # LTree-specific operators should select LTreeOperatorStrategy
        ltree_ops = ["ancestor_of", "descendant_of", "matches_lquery", "matches_ltxtquery"]

        for op in ltree_ops:
            strategy = registry.get_strategy(op, LTree)
            assert strategy.__class__.__name__ == "LTreeOperatorStrategy", (
                f"Operator {op} should use LTreeOperatorStrategy"
            )

    def test_daterange_strategy_selection(self) -> None:
        """Test that DateRange operators select DateRangeOperatorStrategy."""
        registry = get_operator_registry()

        # DateRange-specific operators should select DateRangeOperatorStrategy
        daterange_ops = ["contains_date", "overlaps", "adjacent", "strictly_left", "strictly_right"]

        for op in daterange_ops:
            strategy = registry.get_strategy(op, DateRange)
            assert strategy.__class__.__name__ == "DateRangeOperatorStrategy", (
                f"Operator {op} should use DateRangeOperatorStrategy"
            )

    def test_macaddress_strategy_selection(self) -> None:
        """Test that MAC address operators with field_type select MacAddressOperatorStrategy."""
        registry = get_operator_registry()

        # For MAC addresses, basic operators should select MacAddressOperatorStrategy
        # when field_type is provided
        mac_ops = ["eq", "neq", "in", "notin"]

        for op in mac_ops:
            strategy = registry.get_strategy(op, MacAddress)
            # This should select MacAddressOperatorStrategy for proper macaddr casting
            assert strategy.__class__.__name__ == "MacAddressOperatorStrategy", (
                f"Operator {op} with MacAddress field_type should use MacAddressOperatorStrategy"
            )


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)
    # Quick smoke test
    logger.info("Running Tier 1 Core Special Types Tests...")

    # Test basic imports work
    assert IpAddress is not None
    assert LTree is not None
    assert DateRange is not None
    assert MacAddress is not None

    logger.info("✓ All imports successful")
    logger.info("✓ Tier 1 core test structure created")
    logger.info("\nRun with: pytest tests/core/test_special_types_tier1_core.py -m core -v")
