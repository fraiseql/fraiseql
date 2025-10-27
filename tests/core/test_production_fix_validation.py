"""Test validation for the production fix of JSONB network filtering.

This test validates that the fix in ComparisonOperatorStrategy._apply_type_cast
correctly handles IP addresses even when field_type information is not available.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.operator_strategies import get_operator_registry


@pytest.mark.core
class TestProductionFixValidation:
    """Validate that the production issue is fixed."""

    def test_ip_equality_without_field_type_now_works(self):
        """GREEN: Test that IP equality now works without field_type (the production fix)."""
        registry = get_operator_registry()

        # This is the exact scenario that was failing in production
        strategy = registry.get_strategy("eq", field_type=None)  # No field_type provided

        jsonb_path_sql = SQL("(data ->> 'ip_address')")
        result = strategy.build_sql(jsonb_path_sql, "eq", "8.8.8.8", field_type=None)

        sql_str = str(result)
        print(f"IP equality without field_type: {sql_str}")

        # CRITICAL: The fix should now detect "8.8.8.8" as an IP and apply ::inet casting
        assert "::inet" in sql_str, f"Production fix failed - no inet casting: {sql_str}"
        # Note: Fixed behavior uses direct ::inet casting for equality, not host()
        assert "8.8.8.8" in sql_str, f"Should include IP address value: {sql_str}"

        # The result should now be: (data ->> 'ip_address')::inet = '8.8.8.8'
        # Instead of the broken: (data ->> 'ip_address') = '8.8.8.8'

    def test_various_ip_formats_detected_correctly(self):
        """Test that various IP address formats are detected correctly."""
        registry = get_operator_registry()
        strategy = registry.get_strategy("eq", field_type=None)

        test_ips = [
            "192.168.1.1",  # Private IPv4
            "10.0.0.1",  # Private IPv4
            "172.16.0.1",  # Private IPv4
            "8.8.8.8",  # Public IPv4
            "127.0.0.1",  # Localhost
            "0.0.0.0",  # All zeros
            "255.255.255.255",  # Broadcast
            "2001:db8::1",  # IPv6
            "::1",  # IPv6 localhost
            "fe80::1",  # IPv6 link-local
        ]

        jsonb_path_sql = SQL("(data ->> 'ip_address')")

        for ip in test_ips:
            result = strategy.build_sql(jsonb_path_sql, "eq", ip, field_type=None)
            sql_str = str(result)

            print(f"IP {ip}: {sql_str}")

            # All should be detected as IPs and get inet casting
            assert "::inet" in sql_str, f"IP {ip} not detected - missing inet casting"
            assert ip in sql_str, f"IP {ip} value missing from SQL"

    def test_non_ip_strings_not_affected(self):
        """Test that non-IP strings are not affected by the fix."""
        registry = get_operator_registry()
        strategy = registry.get_strategy("eq", field_type=None)

        non_ip_strings = [
            "hello world",
            "Primary DNS Google",
            "server-01",
            "192.168.1",  # Incomplete IP
            "256.1.1.1",  # Invalid IP (octet too large)
            "not.an.ip.address",
            "",
            "192.168.1.1.1",  # Too many octets
        ]

        jsonb_path_sql = SQL("(data ->> 'identifier')")

        for text in non_ip_strings:
            result = strategy.build_sql(jsonb_path_sql, "eq", text, field_type=None)
            sql_str = str(result)

            print(f"Non-IP '{text}': {sql_str}")

            # These should NOT get inet casting
            assert "::inet" not in sql_str, f"Non-IP '{text}' incorrectly detected as IP"
            assert text in sql_str, f"Non-IP '{text}' value missing from SQL"

            # Should be plain text comparison
            expected_pattern = f"(data ->> 'identifier') = '{text}'"
            # Basic check - should be simple comparison
            assert " = " in sql_str, f"Should have simple equality for non-IP: {sql_str}"

    def test_ip_in_operator_without_field_type(self):
        """Test that 'in' operator with IP addresses works without field_type."""
        registry = get_operator_registry()
        strategy = registry.get_strategy("in", field_type=None)

        ip_list = ["192.168.1.1", "10.0.0.1", "8.8.8.8"]
        jsonb_path_sql = SQL("(data ->> 'ip_address')")

        result = strategy.build_sql(jsonb_path_sql, "in", ip_list, field_type=None)
        sql_str = str(result)

        print(f"IP 'in' operator: {sql_str}")

        # Should detect that this is a list of IPs and apply casting
        assert "::inet" in sql_str, "IN operator with IPs should get inet casting"
        assert " IN " in sql_str, "Should use IN operator"

        # All IPs should be present
        for ip in ip_list:
            assert ip in sql_str, f"IP {ip} missing from IN clause"

    def test_mixed_list_handling(self):
        """Test handling of mixed lists (some IPs, some not)."""
        registry = get_operator_registry()
        strategy = registry.get_strategy("in", field_type=None)

        # This is a bit of an edge case - should we apply IP casting if ANY value looks like an IP?
        mixed_list = ["192.168.1.1", "not-an-ip", "10.0.0.1"]
        jsonb_path_sql = SQL("(data ->> 'mixed_field')")

        result = strategy.build_sql(jsonb_path_sql, "in", mixed_list, field_type=None)
        sql_str = str(result)

        print(f"Mixed list: {sql_str}")

        # This test documents the behavior - if ANY item looks like an IP,
        # we apply IP casting to be safe
        # This might cause issues with the non-IP values, but it's better than
        # missing IP comparisons

    def test_backward_compatibility_with_field_type(self):
        """Test that the fix doesn't break existing behavior when field_type is provided."""
        from fraiseql.types import IpAddress

        registry = get_operator_registry()
        strategy = registry.get_strategy("eq", field_type=IpAddress)

        jsonb_path_sql = SQL("(data ->> 'ip_address')")
        result = strategy.build_sql(jsonb_path_sql, "eq", "8.8.8.8", field_type=IpAddress)

        sql_str = str(result)
        print(f"With field_type (backward compatibility): {sql_str}")

        # Should work with the new NetworkOperatorStrategy implementation
        assert "::inet" in sql_str, "Backward compatibility broken - missing inet casting"
        # NetworkOperatorStrategy now uses direct ::inet casting instead of host()
        assert " = " in sql_str, "Should generate equality comparison"
        assert "'8.8.8.8'" in sql_str, "Should contain the IP address literal"

    def test_production_scenario_exact_reproduction(self):
        """Test the exact production scenario that was failing."""
        # This reproduces the exact failing case from the deep dive document

        registry = get_operator_registry()

        # Production scenario 1: IP equality fails
        eq_strategy = registry.get_strategy("eq", field_type=None)
        jsonb_path = SQL("(data ->> 'ip_address')")

        # This was returning empty results in production
        result = eq_strategy.build_sql(jsonb_path, "eq", "8.8.8.8", field_type=None)
        sql_str = str(result)

        print(f"Production scenario - IP equality: {sql_str}")

        # After the fix, this should work
        assert "::inet" in sql_str, "Production IP equality fix failed"

        # The fix converts this from broken text comparison:
        # (data ->> 'ip_address') = '8.8.8.8'
        # To working IP comparison:
        # (data ->> 'ip_address')::inet = '8.8.8.8'

        # Verify we got the working version with proper INET casting
        assert "::inet" in sql_str, "Should use proper IP comparison with INET casting"


@pytest.mark.core
class TestIPDetectionLogic:
    """Test the IP address detection logic specifically."""

    def test_looks_like_ip_address_ipv4(self):
        """Test IPv4 detection logic."""
        from fraiseql.sql.operator_strategies import ComparisonOperatorStrategy

        strategy = ComparisonOperatorStrategy()

        valid_ipv4 = [
            "192.168.1.1",
            "10.0.0.1",
            "172.16.0.1",
            "8.8.8.8",
            "127.0.0.1",
            "0.0.0.0",
            "255.255.255.255",
        ]

        for ip in valid_ipv4:
            assert strategy._looks_like_ip_address_value(ip, "eq"), f"Should detect {ip} as IPv4"

    def test_looks_like_ip_address_ipv6(self):
        """Test IPv6 detection logic."""
        from fraiseql.sql.operator_strategies import ComparisonOperatorStrategy

        strategy = ComparisonOperatorStrategy()

        valid_ipv6 = ["2001:db8::1", "::1", "fe80::1", "2001:0db8:85a3:0000:0000:8a2e:0370:7334"]

        for ip in valid_ipv6:
            assert strategy._looks_like_ip_address_value(ip, "eq"), f"Should detect {ip} as IPv6"

    def test_looks_like_ip_address_negative(self):
        """Test that non-IPs are not detected as IPs."""
        from fraiseql.sql.operator_strategies import ComparisonOperatorStrategy

        strategy = ComparisonOperatorStrategy()

        not_ips = [
            "hello world",
            "server.example.com",
            "192.168.1",  # Incomplete
            "256.1.1.1",  # Invalid octet
            "not.an.ip.address",
            "",
            "192.168.1.1.1",  # Too many octets
            "primary-dns",
            "12345",
            True,
            None,
            [],
        ]

        for not_ip in not_ips:
            assert not strategy._looks_like_ip_address_value(not_ip, "eq"), (
                f"Should NOT detect {not_ip} as IP"
            )


if __name__ == "__main__":
    print("Testing production fix validation...")

    test_instance = TestProductionFixValidation()

    print("\n1. Testing IP equality without field_type...")
    test_instance.test_ip_equality_without_field_type_now_works()

    print("\n2. Testing various IP formats...")
    test_instance.test_various_ip_formats_detected_correctly()

    print("\n3. Testing non-IP strings...")
    test_instance.test_non_ip_strings_not_affected()

    print(
        "\nRun full tests with: pytest tests/core/test_production_fix_validation.py -m core -v -s"
    )
