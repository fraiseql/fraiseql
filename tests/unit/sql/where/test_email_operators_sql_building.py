"""Tests for Email operators SQL building functions.

These tests verify that Email operators generate correct PostgreSQL SQL
with proper validation for email address operations.
"""

import pytest
from psycopg.sql import SQL

# Import Email operator functions
from fraiseql.sql.where.operators.email import (
    build_email_eq_sql,
    build_email_neq_sql,
    build_email_in_sql,
    build_email_notin_sql,
)


class TestEmailBasicOperators:
    """Test basic Email operators (eq, neq, in, notin)."""

    def test_build_email_equality_sql(self):
        """Test Email equality operator with proper text handling."""
        path_sql = SQL("data->>'user_email'")
        value = "user@example.com"

        result = build_email_eq_sql(path_sql, value)
        expected = "data->>'user_email' = 'user@example.com'"

        assert result.as_string(None) == expected

    def test_build_email_inequality_sql(self):
        """Test Email inequality operator with proper text handling."""
        path_sql = SQL("data->>'user_email'")
        value = "old@example.com"

        result = build_email_neq_sql(path_sql, value)
        expected = "data->>'user_email' != 'old@example.com'"

        assert result.as_string(None) == expected

    def test_build_email_in_list_sql(self):
        """Test Email IN list with multiple email values."""
        path_sql = SQL("data->>'user_email'")
        value = ["admin@example.com", "user@example.com", "support@example.com"]

        result = build_email_in_sql(path_sql, value)
        expected = "data->>'user_email' IN ('admin@example.com', 'user@example.com', 'support@example.com')"

        assert result.as_string(None) == expected

    def test_build_email_not_in_list_sql(self):
        """Test Email NOT IN list with multiple email values."""
        path_sql = SQL("data->>'user_email'")
        value = ["test@example.com", "temp@example.com"]

        result = build_email_notin_sql(path_sql, value)
        expected = "data->>'user_email' NOT IN ('test@example.com', 'temp@example.com')"

        assert result.as_string(None) == expected

    def test_build_email_single_item_in_list(self):
        """Test Email IN list with single value."""
        path_sql = SQL("data->>'user_email'")
        value = ["single@example.com"]

        result = build_email_in_sql(path_sql, value)
        expected = "data->>'user_email' IN ('single@example.com')"

        assert result.as_string(None) == expected

    def test_build_email_different_formats(self):
        """Test Email operators with different email formats."""
        path_sql = SQL("data->>'email'")

        # Test simple email
        result_simple = build_email_eq_sql(path_sql, "user@domain.com")
        expected_simple = "data->>'email' = 'user@domain.com'"
        assert result_simple.as_string(None) == expected_simple

        # Test email with subdomain
        result_subdomain = build_email_eq_sql(path_sql, "admin@mail.company.com")
        expected_subdomain = "data->>'email' = 'admin@mail.company.com'"
        assert result_subdomain.as_string(None) == expected_subdomain

        # Test email with numbers and special chars
        result_complex = build_email_eq_sql(path_sql, "user.123+tag@sub.example-site.org")
        expected_complex = "data->>'email' = 'user.123+tag@sub.example-site.org'"
        assert result_complex.as_string(None) == expected_complex

    def test_build_email_empty_list_handling(self):
        """Test Email operators handle empty lists gracefully."""
        path_sql = SQL("data->>'email'")
        value = []

        result_in = build_email_in_sql(path_sql, value)
        expected_in = "data->>'email' IN ()"
        assert result_in.as_string(None) == expected_in

        result_notin = build_email_notin_sql(path_sql, value)
        expected_notin = "data->>'email' NOT IN ()"
        assert result_notin.as_string(None) == expected_notin

    def test_build_email_case_handling(self):
        """Test Email operators with different case formats."""
        path_sql = SQL("data->>'email'")

        # Email addresses are typically case-insensitive for domain, case-sensitive for local part
        result_upper = build_email_eq_sql(path_sql, "User@EXAMPLE.COM")
        expected_upper = "data->>'email' = 'User@EXAMPLE.COM'"
        assert result_upper.as_string(None) == expected_upper

        result_mixed = build_email_eq_sql(path_sql, "Mixed.Case@Example.Com")
        expected_mixed = "data->>'email' = 'Mixed.Case@Example.Com'"
        assert result_mixed.as_string(None) == expected_mixed


class TestEmailValidation:
    """Test Email operator validation and error handling."""

    def test_email_in_requires_list(self):
        """Test that Email 'in' operator requires a list."""
        path_sql = SQL("data->>'email'")

        with pytest.raises(TypeError, match="'in' operator requires a list"):
            build_email_in_sql(path_sql, "user@example.com")

    def test_email_notin_requires_list(self):
        """Test that Email 'notin' operator requires a list."""
        path_sql = SQL("data->>'email'")

        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            build_email_notin_sql(path_sql, "user@example.com")

    def test_email_formats_supported(self):
        """Test that various valid email formats are supported."""
        path_sql = SQL("data->>'email'")

        # Test valid email formats
        valid_emails = [
            "user@example.com",  # Standard email
            "first.last@company.org",  # With dots
            "user+tag@example.co.uk",  # With plus and multiple TLD
            "admin@sub.domain.example.net",  # Subdomain
            "test123@domain-name.info",  # With numbers and hyphens
            "support@xn--e1afmkfd.xn--p1ai",  # Internationalized domain
        ]

        for email in valid_emails:
            result = build_email_eq_sql(path_sql, email)
            expected = f"data->>'email' = '{email}'"
            assert result.as_string(None) == expected

    def test_email_special_characters(self):
        """Test Email with special characters and edge cases."""
        path_sql = SQL("data->>'email'")

        # Test emails with various allowed special characters
        special_emails = [
            "user_name@example.com",  # Underscore
            "user-name@example.com",  # Hyphen
            "user.name+tag@example.com",  # Dot and plus
            "123456@example.com",  # All numbers
            "a@b.co",  # Minimal valid email
        ]

        for email in special_emails:
            result = build_email_eq_sql(path_sql, email)
            expected = f"data->>'email' = '{email}'"
            assert result.as_string(None) == expected

    def test_email_long_addresses(self):
        """Test Email with longer addresses and domains."""
        path_sql = SQL("data->>'email'")

        # Test longer email addresses
        long_local = "very.long.email.address.with.many.dots"
        long_domain = "very.long.subdomain.example.corporation.com"
        long_email = f"{long_local}@{long_domain}"

        result_long = build_email_eq_sql(path_sql, long_email)
        expected_long = f"data->>'email' = '{long_email}'"
        assert result_long.as_string(None) == expected_long
