"""End-to-end tests for hostname filtering functionality.

These tests verify that hostname operators work correctly in the full context
of the WHERE clause building system, from field detection through SQL generation.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.where.core.field_detection import FieldType
from fraiseql.sql.where.operators import get_operator_function

pytestmark = pytest.mark.database


class TestHostnameEndToEndIntegration:
    """Test Hostname operators in full integration context."""

    def test_hostname_field_type_operators(self) -> None:
        """Test that all expected Hostname operators are available."""
        expected_operators = {"eq", "neq", "in", "notin"}

        for op in expected_operators:
            func = get_operator_function(FieldType.HOSTNAME, op)
            assert callable(func), f"Hostname operator '{op}' should return a callable function"

    def test_hostname_operators_integration(self) -> None:
        """Test Hostname operators generate correct SQL in full context."""
        path_sql = SQL("data->>'server_hostname'")

        # Test equality
        eq_func = get_operator_function(FieldType.HOSTNAME, "eq")
        result = eq_func(path_sql, "api.example.com")
        expected = "data->>'server_hostname' = 'api.example.com'"
        assert result.as_string(None) == expected

        # Test IN list
        in_func = get_operator_function(FieldType.HOSTNAME, "in")
        result = in_func(path_sql, ["web.example.com", "api.example.com"])
        expected = "data->>'server_hostname' IN ('web.example.com', 'api.example.com')"
        assert result.as_string(None) == expected

    def test_hostname_dns_formats_integration(self) -> None:
        """Test Hostname operators with various DNS formats."""
        path_sql = SQL("data->>'hostname'")

        # Test FQDN
        eq_func = get_operator_function(FieldType.HOSTNAME, "eq")
        result = eq_func(path_sql, "server.company.example.com")
        expected = "data->>'hostname' = 'server.company.example.com'"
        assert result.as_string(None) == expected


class TestEmailEndToEndIntegration:
    """Test Email operators in full integration context."""

    def test_email_field_type_operators(self) -> None:
        """Test that all expected Email operators are available."""
        expected_operators = {"eq", "neq", "in", "notin"}

        for op in expected_operators:
            func = get_operator_function(FieldType.EMAIL, op)
            assert callable(func), f"Email operator '{op}' should return a callable function"

    def test_email_operators_integration(self) -> None:
        """Test Email operators generate correct SQL in full context."""
        path_sql = SQL("data->>'user_email'")

        # Test equality
        eq_func = get_operator_function(FieldType.EMAIL, "eq")
        result = eq_func(path_sql, "user@example.com")
        expected = "data->>'user_email' = 'user@example.com'"
        assert result.as_string(None) == expected

        # Test NOT IN list
        notin_func = get_operator_function(FieldType.EMAIL, "notin")
        result = notin_func(path_sql, ["test@example.com", "temp@example.com"])
        expected = "data->>'user_email' NOT IN ('test@example.com', 'temp@example.com')"
        assert result.as_string(None) == expected

    def test_email_complex_addresses_integration(self) -> None:
        """Test Email operators with complex email addresses."""
        path_sql = SQL("data->>'email'")

        # Test complex email
        eq_func = get_operator_function(FieldType.EMAIL, "eq")
        result = eq_func(path_sql, "user.name+tag@subdomain.example.co.uk")
        expected = "data->>'email' = 'user.name+tag@subdomain.example.co.uk'"
        assert result.as_string(None) == expected


class TestPortEndToEndIntegration:
    """Test Port operators in full integration context."""

    def test_port_field_type_operators(self) -> None:
        """Test that all expected Port operators are available."""
        expected_operators = {"eq", "neq", "in", "notin", "gt", "gte", "lt", "lte"}

        for op in expected_operators:
            func = get_operator_function(FieldType.PORT, op)
            assert callable(func), f"Port operator '{op}' should return a callable function"

    def test_port_basic_operators_integration(self) -> None:
        """Test Port basic operators generate correct SQL in full context."""
        path_sql = SQL("data->>'server_port'")

        # Test equality
        eq_func = get_operator_function(FieldType.PORT, "eq")
        result = eq_func(path_sql, 8080)
        expected = "(data->>'server_port')::integer = 8080"
        assert result.as_string(None) == expected

        # Test IN list
        in_func = get_operator_function(FieldType.PORT, "in")
        result = in_func(path_sql, [80, 443, 8080])
        expected = "(data->>'server_port')::integer IN (80, 443, 8080)"
        assert result.as_string(None) == expected

    def test_port_comparison_operators_integration(self) -> None:
        """Test Port comparison operators in full context."""
        path_sql = SQL("data->>'service_port'")

        # Test greater than
        gt_func = get_operator_function(FieldType.PORT, "gt")
        result = gt_func(path_sql, 1024)
        expected = "(data->>'service_port')::integer > 1024"
        assert result.as_string(None) == expected

        # Test less than or equal
        lte_func = get_operator_function(FieldType.PORT, "lte")
        result = lte_func(path_sql, 49151)
        expected = "(data->>'service_port')::integer <= 49151"
        assert result.as_string(None) == expected

    def test_port_range_queries_integration(self) -> None:
        """Test Port operators for range queries."""
        path_sql = SQL("data->>'port'")

        # Test well-known ports range (< 1024)
        lt_func = get_operator_function(FieldType.PORT, "lt")
        result = lt_func(path_sql, 1024)
        expected = "(data->>'port')::integer < 1024"
        assert result.as_string(None) == expected

        # Test registered ports range (>= 1024)
        gte_func = get_operator_function(FieldType.PORT, "gte")
        result = gte_func(path_sql, 1024)
        expected = "(data->>'port')::integer >= 1024"
        assert result.as_string(None) == expected


class TestHostnameIntegratedScenarios:
    """Test hostname operators in realistic integrated scenarios."""

    def test_server_configuration_filtering(self) -> None:
        """Test integrated server configuration filtering."""
        # Hostname filter
        hostname_func = get_operator_function(FieldType.HOSTNAME, "in")
        hostname_result = hostname_func(
            SQL("data->>'hostname'"), ["api.company.com", "web.company.com"]
        )
        hostname_expected = "data->>'hostname' IN ('api.company.com', 'web.company.com')"
        assert hostname_result.as_string(None) == hostname_expected

        # Port range filter
        port_func = get_operator_function(FieldType.PORT, "gte")
        port_result = port_func(SQL("data->>'port'"), 8000)
        port_expected = "(data->>'port')::integer >= 8000"
        assert port_result.as_string(None) == port_expected

    def test_user_contact_filtering(self) -> None:
        """Test integrated user contact information filtering."""
        # Email domain filtering
        email_func = get_operator_function(FieldType.EMAIL, "neq")
        email_result = email_func(SQL("data->>'email'"), "test@example.com")
        email_expected = "data->>'email' != 'test@example.com'"
        assert email_result.as_string(None) == email_expected

    def test_network_service_filtering(self) -> None:
        """Test integrated network service filtering."""
        # Service ports
        port_func = get_operator_function(FieldType.PORT, "in")
        common_service_ports = [22, 80, 443, 3306, 5432]
        port_result = port_func(SQL("data->>'service_port'"), common_service_ports)
        port_expected = "(data->>'service_port')::integer IN (22, 80, 443, 3306, 5432)"
        assert port_result.as_string(None) == port_expected

    def test_operator_error_handling_integration(self) -> None:
        """Test operator error handling in integration context."""
        # Test Hostname IN requires list
        hostname_func = get_operator_function(FieldType.HOSTNAME, "in")
        with pytest.raises(TypeError, match="'in' operator requires a list"):
            hostname_func(SQL("data->>'hostname'"), "single.hostname.com")

        # Test Email NOTIN requires list
        email_func = get_operator_function(FieldType.EMAIL, "notin")
        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            email_func(SQL("data->>'email'"), "single@email.com")

        # Test Port IN requires list
        port_func = get_operator_function(FieldType.PORT, "in")
        with pytest.raises(TypeError, match="'in' operator requires a list"):
            port_func(SQL("data->>'port'"), 8080)

    def test_operator_coverage_integration(self) -> None:
        """Test that all operators are properly integrated."""
        # Hostname operators
        hostname_ops = {"eq", "neq", "in", "notin"}
        for op in hostname_ops:
            func = get_operator_function(FieldType.HOSTNAME, op)
            assert callable(func), f"Hostname {op} not integrated"

        # Email operators
        email_ops = {"eq", "neq", "in", "notin"}
        for op in email_ops:
            func = get_operator_function(FieldType.EMAIL, op)
            assert callable(func), f"Email {op} not integrated"

        # Port operators (including comparisons)
        port_ops = {"eq", "neq", "in", "notin", "gt", "gte", "lt", "lte"}
        for op in port_ops:
            func = get_operator_function(FieldType.PORT, op)
            assert callable(func), f"Port {op} not integrated"

    def test_casting_consistency(self) -> None:
        """Test that operators use consistent casting patterns."""
        path_sql = SQL("data->>'field'")

        # Hostname and Email should not cast (text-based)
        hostname_func = get_operator_function(FieldType.HOSTNAME, "eq")
        hostname_result = hostname_func(path_sql, "example.com")
        assert "::text" not in hostname_result.as_string(None)
        assert "::varchar" not in hostname_result.as_string(None)

        email_func = get_operator_function(FieldType.EMAIL, "eq")
        email_result = email_func(path_sql, "user@example.com")
        assert "::text" not in email_result.as_string(None)
        assert "::varchar" not in email_result.as_string(None)

        # Port should cast to integer
        port_func = get_operator_function(FieldType.PORT, "eq")
        port_result = port_func(path_sql, 8080)
        assert "::integer" in port_result.as_string(None)
