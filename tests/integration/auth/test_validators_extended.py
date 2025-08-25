import pytest

"""Extended tests for security validators to improve coverage."""

from fraiseql.security.validators import InputValidator, ValidationResult


@pytest.mark.security
class TestValidationResult:
    """Test ValidationResult dataclass."""

    def test_validation_result_init(self):
        """Test ValidationResult initialization."""
        # With all fields
        result = ValidationResult(
            is_valid=True, errors=["error1"], sanitized_value="clean", warnings=["warning1"]
        )

        assert result.is_valid is True
        assert result.errors == ["error1"]
        assert result.sanitized_value == "clean"
        assert result.warnings == ["warning1"]

    def test_validation_result_post_init(self):
        """Test ValidationResult post-init warnings initialization."""
        # Without warnings
        result = ValidationResult(is_valid=False, errors=[], sanitized_value=None)

        assert result.warnings == []


class TestInputValidatorExtended:
    """Extended tests for InputValidator class."""

    def test_validate_none_value(self):
        """Test validation of None values."""
        result = InputValidator.validate_field_value("field", None)

        assert result.is_valid is True
        assert result.errors == []
        assert result.sanitized_value is None
        assert result.warnings == []

    def test_validate_length_limits(self):
        """Test field length validation."""
        # Test predefined fields
        long_name = "a" * 300  # Exceeds 255 limit,
        result = InputValidator.validate_field_value("name", long_name)

        assert result.is_valid is False
        assert any("too long" in error for error in result.errors)

        # Test email field
        long_email = "a" * 300 + "@example.com"
        result = InputValidator.validate_field_value("email", long_email)

        assert result.is_valid is False
        assert any("too long" in error for error in result.errors)

        # Test description field (higher limit)
        long_desc = "a" * 6000  # Exceeds 5000 limit,
        result = InputValidator.validate_field_value("description", long_desc)

        assert result.is_valid is False
        assert any("too long" in error for error in result.errors)

        # Test URL field
        long_url = "https://example.com/" + "a" * 2100  # Exceeds 2000 limit,
        result = InputValidator.validate_field_value("url", long_url)

        assert result.is_valid is False
        assert any("too long" in error for error in result.errors)

    def test_validate_default_field_length(self):
        """Test default field length validation."""
        # Unknown field should use default limit
        long_value = "a" * 11000  # Exceeds 10000 default,
        result = InputValidator.validate_field_value("unknown_field", long_value)

        assert result.is_valid is False
        assert any("too long" in error for error in result.errors)

    def test_null_byte_sanitization(self):
        """Test null byte detection and sanitization."""
        value = "hello\x00world"
        result = InputValidator.validate_field_value("field", value)

        assert result.is_valid is False
        assert any("Null byte" in error for error in result.errors)
        assert result.sanitized_value == "helloworld"

    def test_sql_injection_patterns_detailed(self):
        """Test detailed SQL injection pattern detection."""
        patterns = [
            ("DELETE FROM users", True),
            ("INSERT INTO passwords VALUES", True),
            ("UPDATE users SET admin=1", True),
            ("exec sp_help", True),
            ("xp_cmdshell 'dir'", True),
            ("; SELECT * FROM users", True),
            ("/* comment */ UNION SELECT", True),
            ("normal exec function", True),  # Still triggers due to 'exec'
            ("This is a normal comment", False),
        ]

        for value, should_warn in patterns:
            result = InputValidator.validate_field_value("input", value)
            if should_warn:
                assert len(result.warnings) > 0, f"Expected warning for: {value}"
            else:
                assert len(result.warnings) == 0, f"No warning expected for: {value}"

    def test_path_traversal_detection(self):
        """Test path traversal pattern detection."""
        # Test with path-like field names
        path_fields = ["path", "file_path", "upload_path", "filename"]

        for field in path_fields:
            # Unix path traversal
            result = InputValidator.validate_field_value(field, "../../../etc/passwd")
            assert not result.is_valid
            assert any("Path traversal" in error for error in result.errors)

            # Windows path traversal
            result = InputValidator.validate_field_value(field, "..\\..\\windows\\system32")
            assert not result.is_valid
            assert any("Path traversal" in error for error in result.errors)

            # Direct system file access
            result = InputValidator.validate_field_value(field, "/etc/passwd")
            assert not result.is_valid
            assert any("Suspicious system file" in error for error in result.errors)

            # Windows system path
            result = InputValidator.validate_field_value(field, "C:\\Windows\\System32")
            assert not result.is_valid
            assert any("Suspicious system path" in error for error in result.errors)

    def test_path_validation_non_path_fields(self):
        """Test that path validation doesn't apply to non-path fields."""
        result = InputValidator.validate_field_value("description", "../description")
        assert result.is_valid is True
        assert len(result.errors) == 0

    def test_list_validation(self):
        """Test validation of list values."""
        # List with valid values
        result = InputValidator.validate_field_value("tags", ["tag1", "tag2", "tag3"])
        assert result.is_valid is True

        # List with SQL injection attempts
        result = InputValidator.validate_field_value("tags", ["normal", "'; DROP TABLE --"])
        assert result.is_valid is True  # Warnings only
        assert len(result.warnings) > 0

        # List with XSS attempts
        result = InputValidator.validate_field_value(
            "comments", ["normal comment", "<script>alert(1)</script>"], allow_html=False
        )
        assert result.is_valid is False
        assert any("[1]" in error for error in result.errors)

    def test_validate_batch_operations(self):
        """Test batch validation of multiple fields manually."""
        fields = {
            "name": "John Doe",
            "email": "john@example.com",
            "bio": "<p>Hello world</p>",
            "username": "admin'--",
        }

        # Validate each field manually
        results = {}
        for field_name, value in fields.items():
            results[field_name] = InputValidator.validate_field_value(
                field_name, value, allow_html=False
            )

        assert "name" in results
        assert results["name"].is_valid is True

        assert "email" in results
        assert results["email"].is_valid is True

        assert "bio" in results
        assert results["bio"].is_valid is False  # HTML not allowed

        assert "username" in results
        assert len(results["username"].warnings) > 0  # SQL injection warning

    def test_dict_validation(self):
        """Test validation of dictionary values."""
        # Dict with mixed values
        data = {"user": {"name": "Test User", "role": "admin'; DROP TABLE users; --"}}

        result = InputValidator.validate_field_value("data", data)
        assert result.is_valid is True  # Dict itself is valid
        assert len(result.warnings) > 0  # Nested field has warnings

    def test_non_string_values(self):
        """Test validation of non-string values."""
        # Numbers
        result = InputValidator.validate_field_value("age", 25)
        assert result.is_valid is True

        # Boolean
        result = InputValidator.validate_field_value("active", True)
        assert result.is_valid is True

        # Float
        result = InputValidator.validate_field_value("price", 19.99)
        assert result.is_valid is True

    def test_type_specific_validation(self):
        """Test field type specific validation."""
        # Email type
        result = InputValidator.validate_field_value(
            "contact", "test@example.com<script>", field_type="email"
        )
        assert not result.is_valid

        # URL type
        result = InputValidator.validate_field_value(
            "website", "javascript:alert(1)", field_type="url"
        )
        assert not result.is_valid

    def test_nested_list_validation(self):
        """Test deeply nested list validation."""
        nested_data = [
            ["safe", "value"],
            ["another", "'; DROP TABLE --"],
            [["deeply", "nested", "<script>test</script>"]],
        ]

        result = InputValidator.validate_field_value("nested", nested_data)
        assert len(result.warnings) > 0  # SQL warning
        assert len(result.errors) > 0  # XSS error


class TestValidationFunctions:
    """Test standalone validation functions."""

    def test_email_validation(self):
        """Test email-specific validation."""
        # Use the private method for now
        # Valid emails
        valid_emails = ["user@example.com", "test.user+tag@domain.co.uk", "admin@localhost"]

        for email in valid_emails:
            result = InputValidator._validate_email(email)
            assert result.is_valid is True

        # Invalid emails
        invalid_emails = [
            "not-an-email",
            "@example.com",
            "user@",
            "user space@example.com",
            "user@example.com<script>",
            "",
            123,  # Non-string
        ]

        for email in invalid_emails:
            result = InputValidator._validate_email(email)
            assert result.is_valid is False

    def test_mutation_input_validation(self):
        """Test mutation input validation."""
        # Valid input
        valid_input = {"name": "Test User", "email": "test@example.com", "age": 25}

        result = InputValidator.validate_mutation_input(valid_input)
        assert result.is_valid is True

        # Input with issues
        problematic_input = {
            "name": "admin'; DROP TABLE users; --",
            "email": "invalid-email",
            "description": "<script>alert(1)</script>",
        }

        result = InputValidator.validate_mutation_input(problematic_input)
        assert result.is_valid is False
        assert len(result.errors) > 0
        assert len(result.warnings) > 0
