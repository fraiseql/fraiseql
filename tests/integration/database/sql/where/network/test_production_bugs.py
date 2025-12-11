"""Test for production CQRS IP filtering bug reproduction.

This test reproduces the exact issue reported in the PrintOptim Backend where
IP filtering fails due to INET -> JSONB string conversion in CQRS patterns.
"""

import pytest
from psycopg.sql import SQL

import fraiseql
from fraiseql.sql.operators import get_default_registry as get_operator_registry
from fraiseql.types import IpAddress

pytestmark = pytest.mark.database


@fraiseql.type(sql_source="v_dns_server_cqrs_test")
class DnsServerCQRS:
    """DNS server using CQRS pattern that reproduces the bug.

    This mimics the PrintOptim pattern where:
    - Command side: tenant.tb_dns_server with ip_address INET
    - Query side: v_dns_server with JSONB data column containing string IP
    """

    id: str
    identifier: str
    ip_address: IpAddress  # This gets converted to string in JSONB
    n_total_allocations: int | None = None


class TestProductionCQRSIPFilteringBug:
    """Test the specific production CQRS IP filtering bug."""

    def test_ip_address_detection_with_common_ips(self) -> None:
        """Test IP detection with common IP addresses from production."""
        registry = get_operator_registry()

        # Production IP addresses from PrintOptim that were failing
        production_ips = [
            "21.43.63.2",  # delete_netconfig_2
            "120.0.0.1",  # primary-dns-server
            "8.8.8.8",  # Primary DNS Google
            "1.1.1.1",  # Cloudflare DNS
            "192.168.1.1",  # Common router IP
            "10.0.0.1",  # Private network
        ]

        field_path = SQL("data->>'ip_address'")

        for ip in production_ips:
            # Test eq operator SQL generation
            eq_sql = registry.build_sql("eq", ip, field_path, field_type=IpAddress)
            eq_str = eq_sql.as_string(None)  # type: ignore

            # Should properly cast to inet even without field_type
            assert "::inet" in eq_str, f"IP {ip} should be cast to inet: {eq_str}"
            assert ip in eq_str, f"IP {ip} should be in SQL: {eq_str}"

            # Test in operator with list
            in_sql = registry.build_sql("in", [ip], field_path, field_type=IpAddress)
            in_str = in_sql.as_string(None)  # type: ignore

            assert "::inet" in in_str, f"IP list [{ip}] should be cast to inet: {in_str}"
            assert ip in in_str, f"IP {ip} should be in list SQL: {in_str}"

    def test_ip_detection_with_edge_cases(self) -> None:
        """Test IP detection with edge cases that might occur in production."""
        registry = get_operator_registry()
        field_path = SQL("data->>'ip_address'")

        edge_cases = [
            "127.0.0.1",  # Localhost
            "0.0.0.0",  # Any address
            "255.255.255.255",  # Broadcast
            "169.254.1.1",  # Link-local
            "::1",  # IPv6 localhost
            "2001:db8::1",  # IPv6 example
        ]

        for ip in edge_cases:
            eq_sql = registry.build_sql("eq", ip, field_path, field_type=IpAddress)
            eq_str = eq_sql.as_string(None)  # type: ignore

            # Should detect and cast IPv4 and IPv6 addresses
            if ":" in ip:  # IPv6
                assert "::inet" in eq_str, f"IPv6 {ip} should be cast to inet: {eq_str}"
            else:  # IPv4
                assert "::inet" in eq_str, f"IPv4 {ip} should be cast to inet: {eq_str}"

    def test_non_ip_values_not_cast_to_inet(self) -> None:
        """Test that non-IP values are not incorrectly cast to inet."""
        registry = get_operator_registry()
        field_path = SQL("data->>'some_field'")

        non_ip_values = [
            "not.an.ip.address",
            "192.168.1.300",  # Invalid IP (> 255)
            "192.168.1",  # Incomplete IP
            "example.com",  # Domain name
            "test_string",  # Regular string
            "12345",  # Number as string
        ]

        for value in non_ip_values:
            eq_sql = registry.build_sql("eq", value, field_path, field_type=None)
            eq_str = eq_sql.as_string(None)  # type: ignore

            # Should NOT be cast to inet
            assert "::inet" not in eq_str, (
                f"Non-IP value '{value}' should not be cast to inet: {eq_str}"
            )

    def test_mixed_list_filtering(self) -> None:
        """Test filtering with mixed IP and non-IP values in lists."""
        registry = get_operator_registry()
        field_path = SQL("data->>'ip_address'")

        # List with valid IPs - should be detected and cast
        ip_list = ["192.168.1.1", "10.0.0.1", "8.8.8.8"]
        in_sql = registry.build_sql("in", ip_list, field_path, field_type=IpAddress)
        in_str = in_sql.as_string(None)  # type: ignore

        assert "::inet" in in_str, f"IP list should be cast to inet: {in_str}"
        for ip in ip_list:
            assert ip in in_str, f"IP {ip} should be in list SQL: {in_str}"

    def test_comparison_vs_network_operator_strategies(self) -> None:
        """Test that comparison and network strategies handle IPs consistently."""
        registry = get_operator_registry()
        field_path = SQL("data->>'ip_address'")

        test_ip = "192.168.1.100"

        # Both should properly handle IP addresses
        eq_sql = registry.build_sql("eq", test_ip, field_path, field_type=IpAddress)
        subnet_sql = registry.build_sql(
            "inSubnet", "192.168.1.0/24", field_path, field_type=IpAddress
        )

        eq_str = eq_sql.as_string(None)  # type: ignore
        subnet_str = subnet_sql.as_string(None)  # type: ignore

        # Both should cast to inet
        assert "::inet" in eq_str, f"Eq strategy should cast to inet: {eq_str}"
        assert "::inet" in subnet_str, f"Network strategy should cast to inet: {subnet_str}"

        # Different operators for different purposes
        assert "=" in eq_str or "host(" in eq_str, f"Eq should use equality: {eq_str}"
        assert "<<=" in subnet_str, f"Subnet should use containment: {subnet_str}"

    def test_production_jsonb_pattern_simulation(self) -> None:
        """Simulate the exact production pattern that was failing."""
        registry = get_operator_registry()

        # Simulate the JSONB path that would be generated in production
        # This mimics: SELECT data FROM v_dns_server WHERE data->>'ip_address' = ?
        field_path = SQL("data->>'ip_address'")

        # Production scenario: Filter for specific DNS server IP
        target_ip = "21.43.63.2"  # The IP from delete_netconfig_2

        # This is the exact pattern that was failing in PrintOptim
        where_sql = registry.build_sql("eq", target_ip, field_path, field_type=IpAddress)
        where_str = where_sql.as_string(None)  # type: ignore

        print(f"Generated SQL for production pattern: {where_str}")

        # This should generate SQL that works with PostgreSQL INET comparison
        # Expected: (data->>'ip_address')::inet = '21.43.63.2'::inet
        # OR: host((data->>'ip_address')::inet) = '21.43.63.2'

        assert target_ip in where_str, "Target IP should be in the WHERE clause"
        assert "::inet" in where_str, "Should cast JSONB text to INET for proper comparison"

        # The key insight: PostgreSQL needs both sides to be the same type
        # Either both strings: data->>'ip_address' = '21.43.63.2'  (FAILS for INET data)
        # Or both inet: (data->>'ip_address')::inet = '21.43.63.2'::inet  (WORKS)

    def test_list_filtering_production_scenario(self) -> None:
        """Test list filtering with production IPs."""
        registry = get_operator_registry()
        field_path = SQL("data->>'ip_address'")

        # Production scenario: Filter for multiple DNS server IPs
        target_ips = ["120.0.0.1", "8.8.8.8"]  # primary-dns-server and Primary DNS Google

        in_sql = registry.build_sql("in", target_ips, field_path, field_type=IpAddress)
        in_str = in_sql.as_string(None)  # type: ignore

        print(f"Generated SQL for production list filtering: {in_str}")

        # Should properly cast for list comparison
        assert "::inet" in in_str, "Should cast to INET for list comparison"
        for ip in target_ips:
            assert ip in in_str, f"IP {ip} should be in the IN clause"

        # Expected: (data->>'ip_address')::inet IN ('120.0.0.1'::inet, '8.8.8.8'::inet)


if __name__ == "__main__":
    test = TestProductionCQRSIPFilteringBug()
    test.test_ip_address_detection_with_common_ips()
    test.test_ip_detection_with_edge_cases()
    test.test_non_ip_values_not_cast_to_inet()
    test.test_mixed_list_filtering()
    test.test_comparison_vs_network_operator_strategies()
    test.test_production_jsonb_pattern_simulation()
    test.test_list_filtering_production_scenario()
    print("All production CQRS IP filtering tests passed!")
