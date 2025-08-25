import pytest

"""Tests for security validators."""

from fraiseql.security.validators import InputValidator


@pytest.mark.security
class TestInputValidator:
    """Test suite for InputValidator."""

    def test_validate_field_value_with_sql_injection_patterns(self) -> None:
        """Test detection of SQL injection patterns."""
        # Test various SQL injection attempts
        injection_attempts = [
            ("'; DROP TABLE users; --", True),  # Classic SQL injection
            ("' OR '1'='1", False),  # Simple quote doesn't match our patterns
            ("UNION SELECT * FROM passwords", True),  # Union-based injection
            ("admin'--", True),  # Comment-based injection
            ("admin' OR data->>'role' = 'admin' --", True),  # More complete injection
            ("normal value", False),  # Normal string should pass
        ]

        for value, should_warn in injection_attempts:
            result = InputValidator.validate_field_value("username", value)
            assert result.is_valid  # Should still be valid (warnings only)
            if should_warn:
                assert len(result.warnings) > 0, f"Expected warning for: {value}"
            else:
                assert len(result.warnings) == 0, f"Unexpected warning for: {value}"

    def test_validate_field_value_with_xss_patterns(self) -> None:
        """Test detection of XSS patterns."""
        xss_attempts = [
            "<script>alert('xss')</script>",
            "javascript:alert(1)",
            "<img onerror=alert(1)>",
            "<iframe src='evil.com'>",
        ]

        for value in xss_attempts:
            result = InputValidator.validate_field_value("description", value, allow_html=False)
            assert not result.is_valid
            assert len(result.errors) > 0

        # Should pass with allow_html=True
        for value in xss_attempts:
            result = InputValidator.validate_field_value("description", value, allow_html=True)
            assert result.is_valid

    def test_validate_field_value_with_null_bytes(self) -> None:
        """Test handling of null bytes."""
        value = "user\x00name"
        result = InputValidator.validate_field_value("username", value)

        assert not result.is_valid
        assert "Null byte detected" in result.errors[0]
        assert result.sanitized_value == "username"  # Null byte removed

    def test_validate_field_value_length_limits(self) -> None:
        """Test field length validation."""
        # Test name field (max 255)
        long_name = "a" * 300
        result = InputValidator.validate_field_value("name", long_name)
        assert not result.is_valid
        assert "too long" in result.errors[0].lower()

        # Test within limit
        normal_name = "a" * 200
        result = InputValidator.validate_field_value("name", normal_name)
        assert result.is_valid

    def test_validate_numeric_values(self) -> None:
        """Test numeric value validation."""
        # Test infinity
        result = InputValidator.validate_field_value("age", float("inf"))
        assert not result.is_valid
        assert "Infinite value" in result.errors[0]

        # Test NaN
        result = InputValidator.validate_field_value("age", float("nan"))
        assert not result.is_valid
        assert "NaN value" in result.errors[0]

        # Normal numbers should pass
        result = InputValidator.validate_field_value("age", 25)
        assert result.is_valid

        result = InputValidator.validate_field_value("price", 99.99)
        assert result.is_valid

    def test_validate_where_clause(self) -> None:
        """Test WHERE clause validation."""
        # Valid WHERE clause
        where = {
            "name": {"eq": "John"},
            "age": {"gt": 18, "lt": 65},
            "role": {"in": ["admin", "user"]},
        }
        result = InputValidator.validate_where_clause(where)
        assert result.is_valid

        # Invalid operator
        where_invalid = {"name": {"invalid_op": "value"}}
        result = InputValidator.validate_where_clause(where_invalid)
        assert not result.is_valid
        assert "Invalid operator" in result.errors[0]

        # Invalid value type for operator
        where_invalid_type = {"role": {"in": "not_a_list"}}
        result = InputValidator.validate_where_clause(where_invalid_type)
        assert not result.is_valid
        assert "requires a list" in result.errors[0]

    def test_validate_email(self) -> None:
        """Test email validation."""
        # Valid emails
        valid_emails = ["user@example.com", "test.user@domain.co.uk", "admin+tag@company.org"]

        for email in valid_emails:
            result = InputValidator._validate_email(email)
            assert result.is_valid, f"Email should be valid: {email}"
            assert result.sanitized_value == email.lower().strip()

        # Invalid emails
        invalid_emails = ["not-an-email", "@example.com", "user@", "user@.com", "user@domain"]

        for email in invalid_emails:
            result = InputValidator._validate_email(email)
            assert not result.is_valid, f"Email should be invalid: {email}"

    def test_validate_mutation_input(self) -> None:
        """Test mutation input validation."""

        # Define a simple input type for testing
        class CreateUserInput:
            __annotations__ = {"name": str, "email": str, "age": int}

        # Valid input
        input_data = {"name": "John Doe", "email": "john@example.com", "age": 30}
        result = InputValidator.validate_mutation_input(input_data, CreateUserInput)
        assert result.is_valid

        # Invalid email
        input_data_invalid = {"name": "John Doe", "email": "not-an-email", "age": 30}
        result = InputValidator.validate_mutation_input(input_data_invalid, CreateUserInput)
        assert not result.is_valid
        assert any("email" in error.lower() for error in result.errors)

    def test_path_traversal_detection(self) -> None:
        """Test path traversal detection."""
        path_traversal_attempts = [
            "../../../etc/passwd",
            "..\\..\\windows\\system32",
            "/etc/passwd",
            "c:\\windows\\system32",
        ]

        for path in path_traversal_attempts:
            result = InputValidator.validate_field_value("filepath", path, field_type="path")
            assert not result.is_valid
            assert any(
                "path traversal" in error.lower() or "suspicious" in error.lower()
                for error in result.errors
            )

    def test_list_validation(self) -> None:
        """Test validation of list values."""
        # List with SQL injection attempts
        malicious_list = ["normal", "'; DROP TABLE users; --", "admin' OR '1'='1"]

        result = InputValidator.validate_field_value("tags", malicious_list)
        assert result.is_valid  # Warnings only for SQL patterns
        assert len(result.warnings) >= 2  # At least 2 malicious items

    def test_dict_validation(self) -> None:
        """Test validation of dictionary values."""
        # Dict with various issues
        test_dict = {
            "name": "John",
            "script": "<script>alert(1)</script>",
            "sql": "'; DROP TABLE users; --",
        }

        result = InputValidator.validate_field_value("data", test_dict, allow_html=False)
        assert not result.is_valid  # XSS in script field
        assert len(result.errors) > 0
        assert len(result.warnings) > 0  # SQL injection warning
