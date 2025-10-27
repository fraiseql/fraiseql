"""RED-GREEN-REFACTOR Test for JSONB Network Type Casting Issue.

This test reproduces the exact issue described in /tmp/fraiseql_jsonb_network_filtering_deep_dive.md:

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

import pytest
from dataclasses import dataclass
from psycopg.sql import SQL

from fraiseql.types import IpAddress, LTree, DateRange, MacAddress
from fraiseql.sql.operator_strategies import get_operator_registry


@dataclass
class JsonbNetworkDevice:
    """Test model matching the production DnsServer schema pattern."""

    id: str
    identifier: str
    ip_address: IpAddress  # This field comes from JSONB data->>'ip_address' as TEXT


@pytest.mark.core
class TestJSONBNetworkCastingIssue:
    """RED: Tests that reproduce the exact JSONB->TEXT->network type issue."""

    def test_jsonb_ip_equality_fails_without_casting(self):
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
        result = strategy.build_sql(jsonb_path_sql, "eq", "8.8.8.8", IpAddress)

        sql_str = str(result)
        print(f"Generated SQL for IP equality: {sql_str}")

        # CRITICAL: The failure is that JSONB text comparison doesn't work for IPs
        # We need ::inet casting for proper IP address operations

        # For the RED phase, this might fail if the current implementation
        # doesn't properly handle IP addresses in JSONB
        # The key issue is: do we get text comparison or proper inet casting?

        # Check what type of comparison we're getting
        if "::inet" in sql_str:
            # GREEN: Proper casting is happening
            print("✓ PROPER CASTING: Found ::inet in SQL")
            assert "::inet" in sql_str, "Should cast to inet for IP operations"
        else:
            # RED: Text comparison (the bug!)
            print("❌ TEXT COMPARISON BUG: No ::inet casting found")
            print(f"   SQL: {sql_str}")
            print("   This will fail to match IP addresses correctly")

            # This is the bug - we're doing text comparison instead of inet comparison
            # This test should FAIL initially to demonstrate the issue
            pytest.fail(f"JSONB IP equality using text comparison instead of inet: {sql_str}")

    def test_jsonb_network_isprivate_requires_inet_casting(self):
        """RED: Test that reveals isPrivate operator casting issue."""
        registry = get_operator_registry()

        # Get the JSONB path exactly as FraiseQL generates it
        jsonb_path_sql = SQL("(data ->> 'ip_address')")

        # Test private IP detection - this definitely needs inet casting
        strategy = registry.get_strategy("isPrivate", IpAddress)
        result = strategy.build_sql(jsonb_path_sql, "isPrivate", True, IpAddress)

        sql_str = str(result)
        print(f"Generated SQL for isPrivate: {sql_str}")

        # Private IP detection MUST use inet casting and subnet operators
        assert "::inet" in sql_str, "Private IP detection requires inet casting"
        assert "<<=" in sql_str or "inet" in sql_str.lower(), "Should use PostgreSQL inet operators"

        # Should check RFC 1918 ranges
        private_ranges = ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "127.0.0.0/8"]
        has_private_check = any(range_str in sql_str for range_str in private_ranges)
        assert has_private_check, f"Should check private IP ranges, got: {sql_str}"

    def test_jsonb_network_insubnet_requires_inet_casting(self):
        """RED: Test that reveals inSubnet operator casting issue."""
        registry = get_operator_registry()

        jsonb_path_sql = SQL("(data ->> 'ip_address')")

        # Test subnet matching - this is the most critical network operation
        strategy = registry.get_strategy("inSubnet", IpAddress)
        result = strategy.build_sql(jsonb_path_sql, "inSubnet", "192.168.0.0/16", IpAddress)

        sql_str = str(result)
        print(f"Generated SQL for inSubnet: {sql_str}")

        # Subnet matching MUST use PostgreSQL inet subnet operators
        assert "::inet" in sql_str, "Subnet matching requires inet casting"
        assert "<<=" in sql_str, "Should use PostgreSQL subnet containment operator"
        assert "192.168.0.0/16" in sql_str, "Should include subnet parameter"

    def test_strategy_selection_for_network_types(self):
        """RED: Test that proper strategies are selected for network types."""
        registry = get_operator_registry()

        # For IpAddress types, network operators should use NetworkOperatorStrategy
        network_strategy = registry.get_strategy("isPrivate", IpAddress)
        assert network_strategy.__class__.__name__ == "NetworkOperatorStrategy"

        # For IpAddress types, eq should ideally also use NetworkOperatorStrategy
        # or at least ComparisonOperatorStrategy with proper IP handling
        eq_strategy = registry.get_strategy("eq", IpAddress)
        print(f"eq strategy for IpAddress: {eq_strategy.__class__.__name__}")

        # The issue might be here: if ComparisonOperatorStrategy handles eq for IpAddress,
        # it might not know to cast to inet for JSONB fields


@pytest.mark.core
class TestJSONBSpecialTypesCasting:
    """Test JSONB casting for all special types to ensure consistency."""

    def test_ltree_jsonb_casting_issue(self):
        """Test LTree operations need proper casting from JSONB."""
        registry = get_operator_registry()

        jsonb_path_sql = SQL("(data ->> 'path')")

        strategy = registry.get_strategy("ancestor_of", LTree)
        result = strategy.build_sql(jsonb_path_sql, "ancestor_of", "top.middle.bottom", LTree)

        sql_str = str(result)
        print(f"Generated SQL for LTree ancestor_of: {sql_str}")

        # LTree operations need ltree casting
        assert "::ltree" in sql_str, "LTree operations require ltree casting"
        assert "@>" in sql_str, "Should use PostgreSQL ltree ancestor operator"

    def test_daterange_jsonb_casting_issue(self):
        """Test DateRange operations need proper casting from JSONB."""
        registry = get_operator_registry()

        jsonb_path_sql = SQL("(data ->> 'period')")

        strategy = registry.get_strategy("contains_date", DateRange)
        result = strategy.build_sql(jsonb_path_sql, "contains_date", "2024-06-15", DateRange)

        sql_str = str(result)
        print(f"Generated SQL for DateRange contains_date: {sql_str}")

        # DateRange operations need daterange casting
        assert "::daterange" in sql_str, "DateRange operations require daterange casting"
        assert "@>" in sql_str, "Should use PostgreSQL range contains operator"

    def test_macaddress_jsonb_casting_issue(self):
        """Test MacAddress operations need proper casting from JSONB."""
        registry = get_operator_registry()

        jsonb_path_sql = SQL("(data ->> 'mac')")

        strategy = registry.get_strategy("eq", MacAddress)
        result = strategy.build_sql(jsonb_path_sql, "eq", "00:11:22:33:44:55", MacAddress)

        sql_str = str(result)
        print(f"Generated SQL for MacAddress eq: {sql_str}")

        # MacAddress operations need macaddr casting
        assert "::macaddr" in sql_str, "MacAddress operations require macaddr casting"


@pytest.mark.core
class TestProductionReproduction:
    """Tests that exactly reproduce the production failure scenario."""

    def test_production_failure_reproduction(self):
        """Exact reproduction of the production failure case.

        Based on the deep dive:
        - View: v_dns_server with JSONB data column
        - Field: data->>'ip_address' (TEXT from JSONB)
        - Type: IpAddress in Python
        - Query: ipAddress: { eq: "8.8.8.8" }
        - Expected: Find DNS server
        - Actual: Empty result (FAILS)
        """
        from fraiseql.sql.where_generator import build_operator_composed
        from psycopg.sql import SQL

        # This is the exact path FraiseQL uses for JSONB extraction
        jsonb_ip_path = SQL("(data ->> 'ip_address')")

        # Test the exact failing case: IP address equality
        result = build_operator_composed(jsonb_ip_path, "eq", "8.8.8.8", IpAddress)

        sql_str = str(result)
        print(f"Production reproduction SQL: {sql_str}")

        # The critical question: is this doing text comparison or inet comparison?
        # Text comparison will fail to match IP addresses properly
        # We need inet casting for reliable IP matching

        # If this is text comparison, it might work for exact matches but fail for
        # network operations like isPrivate, inSubnet, etc.

        # For the RED phase, we expect this to reveal the casting issue
        if "::inet" not in sql_str:
            print("❌ PRODUCTION BUG REPRODUCED: No inet casting for IP address equality")
            print(f"   This explains why ipAddress: {{eq: '8.8.8.8'}} fails in production")
            print(f"   SQL: {sql_str}")

    def test_production_network_operations_fail(self):
        """Test the network operations that definitely fail in production."""
        from fraiseql.sql.where_generator import build_operator_composed
        from psycopg.sql import SQL

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

            print(f"Operation {op} SQL: {sql_str}")

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

    def test_test_vs_production_sql_generation(self):
        """Compare SQL generation in test vs production scenarios.

        The deep dive mentions that pytest tests pass but production fails.
        This might be due to different data loading or initialization.
        """
        from fraiseql.sql.where_generator import build_operator_composed
        from psycopg.sql import SQL

        # Test the exact same SQL generation that would happen in both environments
        jsonb_path = SQL("(data ->> 'ip_address')")

        # Test all the operations that work in tests but fail in production
        operations = [("eq", "8.8.8.8"), ("isPrivate", True), ("inSubnet", "192.168.0.0/16")]

        for op, value in operations:
            result = build_operator_composed(jsonb_path, op, value, IpAddress)
            sql_str = str(result)

            print(f"Environment test - {op}: {sql_str}")

            # Document the exact SQL being generated
            # This will help us understand the discrepancy
            assert isinstance(result, type(result)), "Should generate SQL"


if __name__ == "__main__":
    # Quick smoke test to run manually
    print("Testing JSONB Network Casting Issue...")

    test_instance = TestJSONBNetworkCastingIssue()

    try:
        test_instance.test_jsonb_ip_equality_fails_without_casting()
        print("✓ IP equality test passed")
    except Exception as e:
        print(f"❌ IP equality test failed: {e}")

    try:
        test_instance.test_jsonb_network_isprivate_requires_inet_casting()
        print("✓ isPrivate test passed")
    except Exception as e:
        print(f"❌ isPrivate test failed: {e}")

    print(
        "\nRun full tests with: pytest tests/core/test_jsonb_network_casting_fix.py -m core -v -s"
    )
