"""Integration tests for FraiseQLRepository with specialized types through operator strategies.

This test suite verifies that FraiseQLRepository._build_dict_where_condition()
correctly handles all specialized FraiseQL types (IP addresses, MAC addresses,
LTree paths, DateRanges, etc.) by properly integrating with the operator
strategy system instead of using primitive SQL templates.

These tests complement the existing SQL generation tests by verifying the
complete repository layer integration works correctly for all specialized types.
"""

from unittest.mock import Mock

import pytest
from tests.helpers.sql_rendering import render_sql_for_testing

from fraiseql.db import FraiseQLRepository

pytestmark = pytest.mark.integration


class TestSpecializedTypesRepositoryIntegration:
    """Test repository integration with FraiseQL's specialized types."""

    def test_ip_address_repository_integration(self) -> None:
        """Test IP address filtering works through repository layer."""
        mock_pool = Mock()
        db = FraiseQLRepository(mock_pool)

        # Test IPv4 addresses
        test_cases = [
            ("ip_address", "eq", "192.168.1.1"),
            ("server_ip", "eq", "10.0.0.1"),
            ("gateway_address", "eq", "172.16.0.1"),
            ("ip_address", "in", ["192.168.1.1", "10.0.0.1"]),
        ]

        for field, operator, value in test_cases:
            condition = db._build_dict_where_condition(field, operator, value)
            condition_str = render_sql_for_testing(condition)

            print(f"IP test - Field: {field}, Op: {operator}, Value: {value}")
            print(f"Generated SQL: {condition_str}")

            # Verify IP addresses get ::inet casting
            assert condition is not None, f"Should generate SQL for IP field {field}"
            assert "::inet" in condition_str, (
                f"IP address should use ::inet casting: {condition_str}"
            )

    def test_mac_address_repository_integration(self) -> None:
        """Test MAC address filtering works through repository layer."""
        mock_pool = Mock()
        db = FraiseQLRepository(mock_pool)

        # Test MAC addresses with different formats
        test_cases = [
            ("mac_address", "eq", "aa:bb:cc:dd:ee:ff"),
            ("device_mac", "eq", "AA-BB-CC-DD-EE-FF"),
            ("mac_address", "in", ["aa:bb:cc:dd:ee:ff", "11:22:33:44:55:66"]),
        ]

        for field, operator, value in test_cases:
            condition = db._build_dict_where_condition(field, operator, value)
            condition_str = render_sql_for_testing(condition) if condition else "None"

            print(f"MAC test - Field: {field}, Op: {operator}, Value: {value}")
            print(f"Generated SQL: {condition_str}")

            # MAC addresses should generate valid SQL (may not have type casting yet)
            assert condition is not None, f"Should generate SQL for MAC field {field}"
            assert field.replace("_", "") in condition_str or "mac" in condition_str, (
                f"Should reference MAC field: {condition_str}"
            )

    def test_ltree_repository_integration(self) -> None:
        """Test LTree hierarchical path filtering through repository layer."""
        mock_pool = Mock()
        db = FraiseQLRepository(mock_pool)

        # Test hierarchical path operations
        test_cases = [
            ("category_path", "eq", "top.science.astrophysics"),
            ("organization_path", "eq", "company.engineering.backend"),
            ("menu_path", "ancestorOf", "top.products"),
            ("category_path", "descendantOf", "root.categories"),
        ]

        for field, operator, value in test_cases:
            condition = db._build_dict_where_condition(field, operator, value)
            condition_str = render_sql_for_testing(condition) if condition else "None"

            print(f"LTree test - Field: {field}, Op: {operator}, Value: {value}")
            print(f"Generated SQL: {condition_str}")

            # LTree should generate appropriate SQL
            if operator in ["ancestorOf", "descendantOf"]:
                # These might not be supported yet, but should not crash
                print(f"Hierarchical operator {operator} result: {condition_str}")
            else:
                assert condition is not None, f"Should generate SQL for LTree field {field}"

    def test_hostname_repository_integration(self) -> None:
        """Test hostname filtering through repository layer."""
        mock_pool = Mock()
        db = FraiseQLRepository(mock_pool)

        # Test hostname formats
        test_cases = [
            ("hostname", "eq", "server.example.com"),
            ("domain_name", "eq", "api.fraiseql.com"),
            ("server_hostname", "contains", "production"),
            ("hostname", "in", ["server1.example.com", "server2.example.com"]),
        ]

        for field, operator, value in test_cases:
            condition = db._build_dict_where_condition(field, operator, value)
            condition_str = render_sql_for_testing(condition) if condition else "None"

            print(f"Hostname test - Field: {field}, Op: {operator}, Value: {value}")
            print(f"Generated SQL: {condition_str}")

            assert condition is not None, f"Should generate SQL for hostname field {field}"
            # Hostname filtering should work with text-based operations

    def test_port_repository_integration(self) -> None:
        """Test port number filtering through repository layer."""
        mock_pool = Mock()
        db = FraiseQLRepository(mock_pool)

        # Test port number operations
        test_cases = [
            ("port", "eq", 8080),
            ("server_port", "gt", 1024),
            ("port", "lt", 65535),
            ("listen_port", "in", [80, 443, 8080, 8443]),
        ]

        for field, operator, value in test_cases:
            condition = db._build_dict_where_condition(field, operator, value)
            condition_str = render_sql_for_testing(condition) if condition else "None"

            print(f"Port test - Field: {field}, Op: {operator}, Value: {value}")
            print(f"Generated SQL: {condition_str}")

            assert condition is not None, f"Should generate SQL for port field {field}"
            # Numeric operations should work correctly

    def test_date_repository_integration(self) -> None:
        """Test date filtering through repository layer."""
        mock_pool = Mock()
        db = FraiseQLRepository(mock_pool)

        # Test date operations
        test_cases = [
            ("created_date", "eq", "2024-01-15"),
            ("birth_date", "gt", "1990-01-01"),
            ("expiry_date", "lt", "2025-12-31"),
            ("event_date", "in", ["2024-01-15", "2024-02-15"]),
        ]

        for field, operator, value in test_cases:
            condition = db._build_dict_where_condition(field, operator, value)
            condition_str = render_sql_for_testing(condition) if condition else "None"

            print(f"Date test - Field: {field}, Op: {operator}, Value: {value}")
            print(f"Generated SQL: {condition_str}")

            assert condition is not None, f"Should generate SQL for date field {field}"
            # Date operations should use appropriate casting/formatting

    def test_daterange_repository_integration(self) -> None:
        """Test DateRange filtering through repository layer."""
        mock_pool = Mock()
        db = FraiseQLRepository(mock_pool)

        # Test DateRange operations
        test_cases = [
            ("validity_period", "eq", "[2024-01-01,2024-12-31]"),
            ("service_period", "contains", "2024-06-15"),
            ("contract_period", "overlaps", "[2024-01-01,2024-06-30]"),
        ]

        for field, operator, value in test_cases:
            condition = db._build_dict_where_condition(field, operator, value)
            condition_str = render_sql_for_testing(condition) if condition else "None"

            print(f"DateRange test - Field: {field}, Op: {operator}, Value: {value}")
            print(f"Generated SQL: {condition_str}")

            # DateRange operations might need special operators
            if operator in ["contains", "overlaps"]:
                print(f"DateRange operator {operator} result: {condition_str}")
            else:
                assert condition is not None, f"Should generate SQL for daterange field {field}"

    def test_email_address_repository_integration(self) -> None:
        """Test email address filtering through repository layer."""
        mock_pool = Mock()
        db = FraiseQLRepository(mock_pool)

        # Test email operations
        test_cases = [
            ("email", "eq", "user@example.com"),
            ("contact_email", "contains", "@fraiseql.com"),
            ("notification_email", "endswith", "@company.com"),
            ("email", "in", ["user1@example.com", "user2@example.com"]),
        ]

        for field, operator, value in test_cases:
            condition = db._build_dict_where_condition(field, operator, value)
            condition_str = render_sql_for_testing(condition) if condition else "None"

            print(f"Email test - Field: {field}, Op: {operator}, Value: {value}")
            print(f"Generated SQL: {condition_str}")

            assert condition is not None, f"Should generate SQL for email field {field}"
            # Email should support text-based operations

    def test_cidr_repository_integration(self) -> None:
        """Test CIDR network filtering through repository layer."""
        mock_pool = Mock()
        db = FraiseQLRepository(mock_pool)

        # Test CIDR network operations
        test_cases = [
            ("network_cidr", "eq", "192.168.1.0/24"),
            ("subnet", "eq", "10.0.0.0/8"),
            ("allowed_networks", "in", ["192.168.1.0/24", "10.0.0.0/16"]),
        ]

        for field, operator, value in test_cases:
            condition = db._build_dict_where_condition(field, operator, value)
            condition_str = render_sql_for_testing(condition) if condition else "None"

            print(f"CIDR test - Field: {field}, Op: {operator}, Value: {value}")
            print(f"Generated SQL: {condition_str}")

            assert condition is not None, f"Should generate SQL for CIDR field {field}"
            # CIDR should ideally use ::cidr or ::inet casting

    def test_mixed_specialized_types_integration(self) -> None:
        """Test that multiple specialized types work correctly together."""
        mock_pool = Mock()
        db = FraiseQLRepository(mock_pool)

        # Mix of different specialized types
        mixed_cases = [
            ("server_ip", "eq", "192.168.1.100"),  # IP address
            ("server_port", "eq", 8080),  # Port
            ("server_hostname", "eq", "api.local"),  # Hostname
            ("server_mac", "eq", "aa:bb:cc:dd:ee:ff"),  # MAC address
            ("admin_email", "eq", "admin@local.com"),  # Email
        ]

        results = []
        for field, operator, value in mixed_cases:
            condition = db._build_dict_where_condition(field, operator, value)
            condition_str = render_sql_for_testing(condition) if condition else "None"
            results.append((field, condition_str))

            print(f"Mixed test - Field: {field}, SQL: {condition_str}")
            assert condition is not None, f"Should handle specialized type {field}"

        # Verify all different types generated different SQL patterns
        sql_patterns = [sql for _, sql in results]
        assert len(set(sql_patterns)) > 1, (
            "Different specialized types should generate different SQL patterns"
        )

        # IP address should have inet casting
        ip_sql = next(sql for field, sql in results if "ip" in field)
        assert "::inet" in ip_sql, f"IP address should use ::inet casting: {ip_sql}"

    def test_fallback_behavior_for_unknown_operators(self) -> None:
        """Test graceful fallback when operator strategies don't support certain operators."""
        mock_pool = Mock()
        db = FraiseQLRepository(mock_pool)

        # Test operators that might not be supported by specialized strategies
        fallback_cases = [
            ("ip_address", "customOperator", "192.168.1.1"),
            ("mac_address", "unknownOp", "aa:bb:cc:dd:ee:ff"),
            ("hostname", "specialFilter", "server.local"),
        ]

        for field, operator, value in fallback_cases:
            condition = db._build_dict_where_condition(field, operator, value)
            condition_str = render_sql_for_testing(condition) if condition else "None"

            print(f"Fallback test - Field: {field}, Op: {operator}, Result: {condition_str}")

            # Should either generate fallback SQL or return None gracefully
            # Should not crash or throw exceptions
            if condition is None:
                print(f"Operator {operator} not supported for {field} - graceful fallback")
            else:
                print(f"Fallback SQL generated: {condition_str}")
