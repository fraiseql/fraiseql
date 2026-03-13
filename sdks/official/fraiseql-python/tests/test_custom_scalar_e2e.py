"""End-to-end test for custom scalar support."""

import json
import re
import tempfile
from pathlib import Path

import pytest

import fraiseql
from fraiseql import CustomScalar, scalar
from fraiseql.registry import SchemaRegistry
from fraiseql.validators import ScalarValidationError, validate_custom_scalar


class Email(CustomScalar):
    """Email address scalar with basic validation."""

    name = "Email"

    EMAIL_REGEX = re.compile(r"^[^@]+@[^@]+\.[^@]+$")

    def serialize(self, value: str) -> str:
        """Convert to string for response."""
        return str(value)

    def parse_value(self, value: str) -> str:
        """Validate email format."""
        value_str = str(value).strip()
        if not self.EMAIL_REGEX.match(value_str):
            raise ValueError(f"Invalid email format: {value_str}")
        return value_str

    def parse_literal(self, ast) -> str:
        """Parse literal from GraphQL query."""
        if hasattr(ast, "value"):
            return self.parse_value(ast.value)
        raise ValueError(f"Email literal must be string, got {type(ast)}")


class Phone(CustomScalar):
    """Phone number scalar in E.164 format."""

    name = "Phone"

    def serialize(self, value: str) -> str:
        """Convert to string for response."""
        return str(value)

    def parse_value(self, value: str) -> str:
        """Validate E.164 format."""
        value_str = str(value).strip()
        if not value_str.startswith("+"):
            raise ValueError("Phone must start with +")
        digits = value_str[1:]
        if not digits.isdigit():
            raise ValueError("Phone must contain only digits after +")
        if len(digits) < 10 or len(digits) > 14:
            raise ValueError(f"Phone must be 10-14 digits, got {len(digits)}")
        return value_str

    def parse_literal(self, ast) -> str:
        """Parse literal from GraphQL query."""
        if hasattr(ast, "value"):
            return self.parse_value(ast.value)
        raise ValueError(f"Phone literal must be string, got {type(ast)}")


@pytest.fixture(autouse=True)
def clear_registry():
    """Clear registry before and after each test."""
    SchemaRegistry.clear()
    yield
    SchemaRegistry.clear()


class TestCustomScalarRegistration:
    """Test @scalar decorator registration."""

    def test_scalar_registers_globally(self):
        """Scalar decorator registers the scalar globally."""
        # Before registration, no scalars
        assert len(SchemaRegistry.get_custom_scalars()) == 0

        # Register Email
        scalar(Email)

        # After registration, Email is in registry
        assert "Email" in SchemaRegistry.get_custom_scalars()
        assert SchemaRegistry.get_custom_scalars()["Email"] is Email

    def test_scalar_returns_class_unmodified(self):
        """Decorator returns class unmodified."""
        original = Email
        decorated = scalar(Email)

        assert decorated is original
        assert decorated.__name__ == "Email"
        assert decorated.name == "Email"

    def test_scalar_validates_class_is_custom_scalar(self):
        """Decorator validates class inherits from CustomScalar."""

        class NotAScalar:
            name = "NotAScalar"

        with pytest.raises(TypeError, match="CustomScalar subclasses"):
            scalar(NotAScalar)

    def test_scalar_validates_name_attribute(self):
        """Decorator validates class has a name attribute."""

        class NoNameScalar(CustomScalar):
            def serialize(self, value):
                return value

            def parse_value(self, value):
                return value

            def parse_literal(self, ast):
                return ast

        with pytest.raises(ValueError, match="must have a 'name'"):
            scalar(NoNameScalar)

    def test_scalar_validates_name_is_string(self):
        """Decorator validates name is a string."""

        class BadNameScalar(CustomScalar):
            name = 123

            def serialize(self, value):
                return value

            def parse_value(self, value):
                return value

            def parse_literal(self, ast):
                return ast

        with pytest.raises(ValueError, match="must be a non-empty string"):
            scalar(BadNameScalar)

    def test_scalar_prevents_duplicate_names(self):
        """Decorator prevents duplicate scalar names."""
        scalar(Email)

        class AnotherEmail(CustomScalar):
            name = "Email"

            def serialize(self, value):
                return value

            def parse_value(self, value):
                return value

            def parse_literal(self, ast):
                return ast

        with pytest.raises(ValueError, match="already registered"):
            scalar(AnotherEmail)


class TestCustomScalarValidation:
    """Test scalar validation engine."""

    def test_validate_parse_value_success(self):
        """validate_custom_scalar with parse_value context."""
        email = validate_custom_scalar(Email, "user@example.com", context="parse_value")
        assert email == "user@example.com"

    def test_validate_parse_value_failure(self):
        """validate_custom_scalar raises error on invalid value."""
        with pytest.raises(ScalarValidationError) as exc_info:
            validate_custom_scalar(Email, "invalid-email", context="parse_value")

        assert "Email" in str(exc_info.value)
        assert "parse_value" in str(exc_info.value)

    def test_validate_serialize(self):
        """validate_custom_scalar with serialize context."""
        serialized = validate_custom_scalar(Email, "user@example.com", context="serialize")
        assert serialized == "user@example.com"

    def test_validate_parse_literal(self):
        """validate_custom_scalar with parse_literal context."""

        class FakeAST:
            value = "user@example.com"

        result = validate_custom_scalar(Email, FakeAST(), context="parse_literal")
        assert result == "user@example.com"

    def test_validate_invalid_context(self):
        """validate_custom_scalar raises error on invalid context."""
        with pytest.raises(ScalarValidationError):
            validate_custom_scalar(Email, "user@example.com", context="invalid")

    def test_validate_multiple_scalars(self):
        """validate_custom_scalar works with multiple scalar types."""
        # Email validation
        email = validate_custom_scalar(Email, "test@test.com")
        assert email == "test@test.com"

        # Phone validation
        phone = validate_custom_scalar(Phone, "+12025551234")
        assert phone == "+12025551234"

        # Both fail appropriately
        with pytest.raises(ScalarValidationError):
            validate_custom_scalar(Email, "notanemail")

        with pytest.raises(ScalarValidationError):
            validate_custom_scalar(Phone, "invalid")


class TestCustomScalarInTypes:
    """Test using custom scalars in @type definitions."""

    def test_custom_scalar_in_type_annotation(self):
        """Custom scalar can be used in type annotation."""
        scalar(Email)
        scalar(Phone)

        @fraiseql.type
        class User:
            """A user in the system."""

            id: int
            email: Email
            phone: Phone | None
            name: str

        # Type is registered
        assert "User" in SchemaRegistry._types
        user_def = SchemaRegistry._types["User"]

        # Fields include email and phone
        field_names = [f["name"] for f in user_def["fields"]]
        assert "email" in field_names
        assert "phone" in field_names

        # Email field has correct type
        email_field = next(f for f in user_def["fields"] if f["name"] == "email")
        assert email_field["type"] == "Email"

        # Phone field is nullable
        phone_field = next(f for f in user_def["fields"] if f["name"] == "phone")
        assert phone_field["type"] == "Phone"
        assert phone_field["nullable"] is True


class TestSchemaExport:
    """Test schema export with custom scalars."""

    def test_schema_includes_custom_scalars(self):
        """Exported schema includes custom scalar definitions."""
        scalar(Email)
        scalar(Phone)

        @fraiseql.type
        class User:
            id: int
            email: Email
            phone: Phone | None

        schema = SchemaRegistry.get_schema()

        # Schema has customScalars section
        assert "customScalars" in schema
        assert "Email" in schema["customScalars"]
        assert "Phone" in schema["customScalars"]

    def test_custom_scalar_schema_structure(self):
        """Custom scalar schema has correct structure."""
        scalar(Email)

        schema = SchemaRegistry.get_schema()
        email_def = schema["customScalars"]["Email"]

        assert email_def["name"] == "Email"
        assert email_def["description"] is not None
        assert email_def["validate"] is True

    def test_schema_export_to_file(self):
        """Schema can be exported to JSON file."""
        scalar(Email)
        scalar(Phone)

        @fraiseql.type
        class User:
            id: int
            email: Email
            phone: Phone | None

        @fraiseql.query
        def users(limit: int = 10) -> list[User]:
            """Get all users."""
            pass

        # Export to temp file
        with tempfile.TemporaryDirectory() as tmpdir:
            schema_path = Path(tmpdir) / "schema.json"
            fraiseql.export_schema(str(schema_path))

            # File exists
            assert schema_path.exists()

            # File contains valid JSON
            with open(schema_path) as f:
                data = json.load(f)

            # Has custom scalars
            assert "customScalars" in data
            assert "Email" in data["customScalars"]
            assert "Phone" in data["customScalars"]

            # Has types
            assert "types" in data
            type_names = [t["name"] for t in data["types"]]
            assert "User" in type_names

            # Has queries
            assert "queries" in data
            assert len(data["queries"]) > 0

    def test_schema_export_without_custom_scalars(self):
        """Can export schema without custom scalars."""

        @fraiseql.type
        class User:
            id: int
            name: str

        with tempfile.TemporaryDirectory() as tmpdir:
            schema_path = Path(tmpdir) / "schema.json"
            fraiseql.export_schema(str(schema_path), include_custom_scalars=False)

            with open(schema_path) as f:
                data = json.load(f)

            # No customScalars section
            assert "customScalars" not in data


class TestGetAllCustomScalars:
    """Test get_all_custom_scalars utility function."""

    def test_get_all_returns_registered_scalars(self):
        """get_all_custom_scalars returns all registered scalars."""
        from fraiseql.validators import get_all_custom_scalars

        scalar(Email)
        scalar(Phone)

        scalars = get_all_custom_scalars()
        assert "Email" in scalars
        assert "Phone" in scalars
        assert scalars["Email"] is Email
        assert scalars["Phone"] is Phone

    def test_get_all_empty_when_none_registered(self):
        """get_all_custom_scalars returns empty dict when nothing registered."""
        from fraiseql.validators import get_all_custom_scalars

        scalars = get_all_custom_scalars()
        assert scalars == {}


class TestErrorMessages:
    """Test error message clarity."""

    def test_validation_error_includes_scalar_name(self):
        """Validation errors include scalar name."""
        with pytest.raises(ScalarValidationError) as exc_info:
            validate_custom_scalar(Email, "notanemail")

        assert "Email" in str(exc_info.value)

    def test_validation_error_includes_context(self):
        """Validation errors include context."""
        with pytest.raises(ScalarValidationError) as exc_info:
            validate_custom_scalar(Email, "notanemail", context="parse_value")

        assert "parse_value" in str(exc_info.value)

    def test_validation_error_includes_message(self):
        """Validation errors include original error message."""
        with pytest.raises(ScalarValidationError) as exc_info:
            validate_custom_scalar(Email, "notanemail")

        assert "Invalid email format" in str(exc_info.value)
