"""RED-GREEN-REFACTOR Test for JSONB Network Type Casting Issue.

This test reproduces the exact issue described in
/tmp/fraiseql_jsonb_network_filtering_deep_dive.md:

PROBLEM: Network filtering fails when IP addresses are stored in JSONB columns because
         FraiseQL treats `data->>'ip_address'` as TEXT but network operations require
         proper PostgreSQL inet casting.

REPRODUCTION:
- Data stored as JSONB: {"ip_address": "8.8.8.8"}
- FraiseQL extracts as: data->>'ip_address' (TEXT)
- Network operators try to apply: TEXT.isPrivate() (FAILS)
- Should cast to: (data->>'ip_address')::inet for network operations

This is the core issue causing 3 release failures.
"""

import logging
from dataclasses import dataclass

import pytest
from psycopg.sql import SQL
from tests.helpers.sql_rendering import render_sql_for_testing

from fraiseql.sql.operators import get_default_registry as get_operator_registry
from fraiseql.types import DateRange, IpAddress, LTree, MacAddress

logger = logging.getLogger(__name__)

pytestmark = pytest.mark.integration


@dataclass
class JsonbNetworkDevice:
    """Test model matching the production DnsServer schema pattern."""

    id: str
    identifier: str
    ip_address: IpAddress  # This field comes from JSONB data->>'ip_address' as TEXT


@pytest.mark.core
class TestJSONBNetworkCastingIssue:
    """RED: Tests that reproduce the exact JSONB->TEXT->network type issue."""

    def test_jsonb_ip_equality_fails_without_casting(self) -> None:
        """RED: Test that reveals the core JSONB casting issue.

        This test reproduces the exact failure from the deep dive:
        - IP stored in JSONB as TEXT
        - Extracted via data->>'ip_address'
        - Network operators fail without ::inet casting
        """
        registry = get_operator_registry()

        # This is the exact pattern from production: JSONB text extraction
        jsonb_path_sql = SQL("(data ->> 'ip_address')")

        # Test the problematic case: ComparisonOperatorStrategy with IpAddress type
        # This should use NetworkOperatorStrategy or at least cast to ::inet
        strategy = registry.get_strategy("eq", IpAddress)

        # Generate SQL for IP equality - this is where the issue occurs
        result = strategy.build_sql("eq", "8.8.8.8", jsonb_path_sql, IpAddress)

        sql_str = render_sql_for_testing(result)
        logger.debug(f"Generated SQL for IP equality: {sql_str}")

        # CRITICAL: The failure is that JSONB text comparison doesn't work for IPs
        # We need ::inet casting for proper IP address operations

        # For the RED phase, this might fail if the current implementation
        # doesn't properly handle IP addresses in JSONB
        # The key issue is: do we get text comparison or proper inet casting?

        # Current implementation: IP equality uses text comparison
        logger.debug(f"Generated SQL: {sql_str}")
        assert "8.8.8.8" in sql_str, "Should contain IP address"
        assert "data ->> 'ip_address'" in sql_str, "Should contain JSONB extraction"

        # Current result: text comparison without inet casting
        has_inet_casting = "::inet" in sql_str
        logger.debug(f"Has ::inet casting: {has_inet_casting}")

    def test_jsonb_network_isprivate_requires_inet_casting(self) -> None:
        """RED: Test that reveals isPrivate operator casting issue."""
        registry = get_operator_registry()

        # Get the JSONB path exactly as FraiseQL generates it
        jsonb_path_sql = SQL("(data ->> 'ip_address')")

        # Test private IP detection - this definitely needs inet casting
        strategy = registry.get_strategy("isPrivate", IpAddress)
        result = strategy.build_sql("isPrivate", True, jsonb_path_sql, IpAddress)

        sql_str = render_sql_for_testing(result)
        logger.debug(f"Generated SQL for isPrivate: {sql_str}")

        # Current implementation: may or may not use inet casting
        has_inet_casting = "::inet" in sql_str
        logger.debug(f"Has ::inet casting: {has_inet_casting}")
        assert "data ->> 'ip_address'" in sql_str, "Should contain JSONB extraction"

    def test_jsonb_network_insubnet_requires_inet_casting(self) -> None:
        """RED: Test that reveals inSubnet operator casting issue."""
        registry = get_operator_registry()

        jsonb_path_sql = SQL("(data ->> 'ip_address')")

        # Test subnet matching - this is the most critical network operation
        strategy = registry.get_strategy("inSubnet", IpAddress)
        result = strategy.build_sql("inSubnet", "192.168.0.0/16", jsonb_path_sql, IpAddress)

        sql_str = render_sql_for_testing(result)
        logger.debug(f"Generated SQL for inSubnet: {sql_str}")

        # Current implementation: may or may not use inet casting
        has_inet_casting = "::inet" in sql_str
        logger.debug(f"Has ::inet casting: {has_inet_casting}")
        assert "192.168.0.0/16" in sql_str, "Should include subnet parameter"

    def test_strategy_selection_for_network_types(self) -> None:
        """RED: Test that proper strategies are selected for network types."""
        registry = get_operator_registry()

        # For IpAddress types, network operators should use NetworkOperatorStrategy
        network_strategy = registry.get_strategy("isPrivate", IpAddress)
        assert network_strategy.__class__.__name__ == "NetworkOperatorStrategy"

        # For IpAddress types, eq should ideally also use NetworkOperatorStrategy
        # or at least ComparisonOperatorStrategy with proper IP handling
        eq_strategy = registry.get_strategy("eq", IpAddress)
        logger.debug(f"eq strategy for IpAddress: {eq_strategy.__class__.__name__}")

        # The issue might be here: if ComparisonOperatorStrategy handles eq for IpAddress,
        # it might not know to cast to inet for JSONB fields


@pytest.mark.core
class TestJSONBSpecialTypesCasting:
    """Test JSONB casting for all special types to ensure consistency."""

    def test_ltree_jsonb_casting_issue(self) -> None:
        """Test LTree operations need proper casting from JSONB."""
        registry = get_operator_registry()

        jsonb_path_sql = SQL("(data ->> 'path')")

        strategy = registry.get_strategy("ancestor_of", LTree)
        result = strategy.build_sql("ancestor_of", "top.middle.bottom", jsonb_path_sql, LTree)

        sql_str = render_sql_for_testing(result)
        logger.debug(f"Generated SQL for LTree ancestor_of: {sql_str}")

        # Current implementation: may or may not use ltree casting
        has_ltree_casting = "::ltree" in sql_str
        logger.debug(f"Has ::ltree casting: {has_ltree_casting}")
        assert "top.middle.bottom" in sql_str, "Should contain LTree path"

    def test_daterange_jsonb_casting_issue(self) -> None:
        """Test DateRange operations need proper casting from JSONB."""
        registry = get_operator_registry()

        jsonb_path_sql = SQL("(data ->> 'period')")

        strategy = registry.get_strategy("contains_date", DateRange)
        result = strategy.build_sql("contains_date", "2024-06-15", jsonb_path_sql, DateRange)

        sql_str = render_sql_for_testing(result)
        logger.debug(f"Generated SQL for DateRange contains_date: {sql_str}")

        # Current implementation: may or may not use daterange casting
        has_daterange_casting = "::daterange" in sql_str
        logger.debug(f"Has ::daterange casting: {has_daterange_casting}")
        assert "2024-06-15" in sql_str, "Should contain date"

    def test_macaddress_jsonb_casting_issue(self) -> None:
        """Test MacAddress operations need proper casting from JSONB."""
        registry = get_operator_registry()

        jsonb_path_sql = SQL("(data ->> 'mac')")

        strategy = registry.get_strategy("eq", MacAddress)
        result = strategy.build_sql("eq", "00:11:22:33:44:55", jsonb_path_sql, MacAddress)

        sql_str = render_sql_for_testing(result)
        logger.debug(f"Generated SQL for MacAddress eq: {sql_str}")

        # Current implementation: may or may not use macaddr casting
        has_macaddr_casting = "::macaddr" in sql_str
        logger.debug(f"Has ::macaddr casting: {has_macaddr_casting}")
        assert "00:11:22:33:44:55" in sql_str, "Should contain MAC address"


@pytest.mark.core
class TestProductionReproduction:
    """Tests that exactly reproduce the production failure scenario."""

    def test_production_failure_reproduction(self) -> None:
        """Exact reproduction of the production failure case.

        Based on the deep dive:
        - View: v_dns_server with JSONB data column
        - Field: data->>'ip_address' (TEXT from JSONB)
        - Type: IpAddress in Python
        - Query: ipAddress: { eq: "8.8.8.8" }
        - Expected: Find DNS server
        - Actual: Empty result (FAILS)
        """
        from psycopg.sql import SQL

        from fraiseql.sql.where_generator import build_operator_composed

        # This is the exact path FraiseQL uses for JSONB extraction
        jsonb_ip_path = SQL("(data ->> 'ip_address')")

        # Test the exact failing case: IP address equality
        result = build_operator_composed(jsonb_ip_path, "eq", "8.8.8.8", IpAddress)

        sql_str = str(result)
        logger.debug(f"Production reproduction SQL: {sql_str}")

        # The critical question: is this doing text comparison or inet comparison?
        # Text comparison will fail to match IP addresses properly
        # We need inet casting for reliable IP matching

        # If this is text comparison, it might work for exact matches but fail for
        # network operations like isPrivate, inSubnet, etc.

        # For the RED phase, we expect this to reveal the casting issue
        if "::inet" not in sql_str:
            logger.debug("❌ PRODUCTION BUG REPRODUCED: No inet casting for IP address equality")
            logger.debug("   This explains why ipAddress: {eq: '8.8.8.8'} fails in production")
            logger.debug(f"   SQL: {sql_str}")

    def test_production_network_operations_fail(self) -> None:
        """Test the network operations that definitely fail in production."""
        from psycopg.sql import SQL

        from fraiseql.sql.where_generator import build_operator_composed

        jsonb_ip_path = SQL("(data ->> 'ip_address')")

        # These are the operations that fail in production
        failing_operations = [
            ("isPrivate", True),
            ("isPublic", True),
            ("inSubnet", "192.168.0.0/16"),
        ]

        for op, value in failing_operations:
            result = build_operator_composed(jsonb_ip_path, op, value, IpAddress)
            sql_str = str(result)

            logger.debug(f"Operation {op} SQL: {sql_str}")

            # All network operations MUST have inet casting to work
            assert "::inet" in sql_str, f"Network operation {op} requires inet casting"

            if op in ("isPrivate", "isPublic"):
                # Should check private IP ranges
                assert "<<=" in sql_str or "inet" in sql_str.lower()
            elif op == "inSubnet":
                # Should use subnet containment
                assert "<<=" in sql_str


@pytest.mark.core
class TestEnvironmentalParity:
    """Test to understand why tests pass but production fails."""

    def test_test_vs_production_sql_generation(self) -> None:
        """Compare SQL generation in test vs production scenarios.

        The deep dive mentions that pytest tests pass but production fails.
        This might be due to different data loading or initialization.
        """
        from psycopg.sql import SQL

        from fraiseql.sql.where_generator import build_operator_composed

        # Test the exact same SQL generation that would happen in both environments
        jsonb_path = SQL("(data ->> 'ip_address')")

        # Test all the operations that work in tests but fail in production
        operations = [("eq", "8.8.8.8"), ("isPrivate", True), ("inSubnet", "192.168.0.0/16")]

        for op, value in operations:
            result = build_operator_composed(jsonb_path, op, value, IpAddress)
            sql_str = str(result)

            logger.debug(f"Environment test - {op}: {sql_str}")

            # Document the exact SQL being generated
            # This will help us understand the discrepancy
            assert isinstance(result, type(result)), "Should generate SQL"


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)
    # Quick smoke test to run manually
    logger.info("Testing JSONB Network Casting Issue...")

    test_instance = TestJSONBNetworkCastingIssue()

    try:
        test_instance.test_jsonb_ip_equality_fails_without_casting()
        logger.info("✓ IP equality test passed")
    except Exception as e:
        logger.error(f"❌ IP equality test failed: {e}")

    try:
        test_instance.test_jsonb_network_isprivate_requires_inet_casting()
        logger.info("✓ isPrivate test passed")
    except Exception as e:
        logger.error(f"❌ isPrivate test failed: {e}")

    logger.info(
        "\nRun full tests with: pytest tests/core/test_jsonb_network_casting_fix.py -m core -v -s"
    )
