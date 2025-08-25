"""Test default Error type and MutationResultBase for plug-and-play usage."""

import pytest

import fraiseql


@pytest.mark.unit
class TestDefaultErrorType:
    """Test that FraiseQL provides a default Error type out of the box."""

    def test_error_type_is_available_from_main_import(self):
        """Test that Error type is importable from main fraiseql module."""
        # This should work without custom Error type definition
        from fraiseql import Error

        # Should be a proper @fraise_type
        assert hasattr(Error, "__fraiseql_definition__")

        # Should have standard fields
        definition = Error.__fraiseql_definition__
        field_names = set(definition.fields.keys())

        expected_fields = {"message", "code", "identifier", "details"}
        assert expected_fields.issubset(field_names)

    def test_error_type_can_be_instantiated(self):
        """Test that default Error type works like FraiseQL's custom one."""
        from fraiseql import Error

        error = Error(
            message="Test error", code=400, identifier="test_error", details={"field": "value"}
        )

        assert error.message == "Test error"
        assert error.code == 400
        assert error.identifier == "test_error"
        assert error.details == {"field": "value"}

    def test_error_type_details_optional(self):
        """Test that details field is optional."""
        from fraiseql import Error

        error = Error(message="Simple error", code=500, identifier="simple")

        assert error.message == "Simple error"
        assert error.code == 500
        assert error.identifier == "simple"
        # details should default to None
        assert error.details is None


class TestDefaultMutationResultBase:
    """Test that FraiseQL provides a default MutationResultBase."""

    def test_mutation_result_base_available(self):
        """Test that MutationResultBase is available from main import."""
        from fraiseql import MutationResultBase

        assert hasattr(MutationResultBase, "__fraiseql_definition__")

        # Should have standard fields from FraiseQL pattern
        definition = MutationResultBase.__fraiseql_definition__
        field_names = set(definition.fields.keys())

        expected_fields = {"status", "message", "errors"}
        assert expected_fields.issubset(field_names)

    def test_mutation_result_base_can_be_used(self):
        """Test that MutationResultBase works for common patterns."""
        from fraiseql import MutationResultBase

        @fraiseql.type
        class TestSuccess(MutationResultBase):
            entity: dict | None = None

        @fraiseql.type
        class TestError(MutationResultBase):
            conflict_entity: dict | None = None

        # Should work without issues
        assert hasattr(TestSuccess, "__fraiseql_definition__")
        assert hasattr(TestError, "__fraiseql_definition__")

    def test_mutation_result_base_inheritance_works(self):
        """Test that inheriting from MutationResultBase provides expected fields."""
        from fraiseql import MutationResultBase

        @fraiseql.type
        class CreateUserSuccess(MutationResultBase):
            user: dict | None = None

        success = CreateUserSuccess(
            status="success",
            message="User created",
            errors=None,
            user={"id": "123", "name": "Test User"},
        )

        assert success.status == "success"
        assert success.message == "User created"
        assert success.errors is None
        assert success.user == {"id": "123", "name": "Test User"}


class TestImprovedDefaultErrorConfig:
    """Test that DEFAULT_ERROR_CONFIG is more FraiseQL-friendly."""

    def test_default_error_config_includes_fraiseql_patterns(self):
        """Test that default config handles noop: and blocked: as data."""
        from fraiseql.mutations.error_config import DEFAULT_ERROR_CONFIG

        # Should treat noop: and blocked: as error_as_data (not GraphQL errors)
        assert "noop:" in DEFAULT_ERROR_CONFIG.error_as_data_prefixes
        assert "blocked:" in DEFAULT_ERROR_CONFIG.error_as_data_prefixes

        # Should have reasonable success keywords
        expected_success = {"success", "completed", "ok", "done", "updated", "created", "deleted"}
        assert expected_success.issubset(DEFAULT_ERROR_CONFIG.success_keywords)

    def test_default_config_works_with_error_auto_population(self):
        """Test that default config properly populates error arrays."""
        from fraiseql import Error
        from fraiseql.mutations.error_config import DEFAULT_ERROR_CONFIG
        from fraiseql.mutations.parser import parse_mutation_result

        @fraiseql.type
        class TestSuccess:
            message: str
            entity: dict | None = None

        @fraiseql.type
        class TestError:
            message: str
            errors: list[Error] | None = None

        # Test noop case - should be treated as data but populate errors array
        result = {
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "updated_fields": [],
            "status": "noop:already_exists",
            "message": "Entity already exists",
            "object_data": {},
            "extra_metadata": {"conflict_id": "existing-id"},
        }

        parsed = parse_mutation_result(result, TestSuccess, TestError, DEFAULT_ERROR_CONFIG)

        # Should be TestError (because noop: is error_as_data, not success)
        assert isinstance(parsed, TestError)
        assert parsed.errors is not None
        assert len(parsed.errors) == 1
        assert parsed.errors[0].message == "Entity already exists"
        assert parsed.errors[0].identifier == "already_exists"


class TestPlugAndPlayIntegration:
    """Test that FraiseQL patterns work with minimal configuration."""

    def test_simple_mutation_with_defaults(self):
        """Test creating a mutation using only FraiseQL defaults."""
        from fraiseql import MutationResultBase

        @fraiseql.input
        class CreateEntityInput:
            name: str

        @fraiseql.success
        class CreateEntitySuccess(MutationResultBase):
            entity: dict | None = None

        @fraiseql.failure
        class CreateEntityError(MutationResultBase):
            conflict_entity: dict | None = None

        # Should work without custom Error type or MutationResultBase definition
        assert hasattr(CreateEntitySuccess, "__fraiseql_definition__")
        assert hasattr(CreateEntityError, "__fraiseql_definition__")

        # Both should have errors field from MutationResultBase
        success_fields = set(CreateEntitySuccess.__fraiseql_definition__.fields.keys())
        error_fields = set(CreateEntityError.__fraiseql_definition__.fields.keys())

        assert "errors" in success_fields
        assert "errors" in error_fields
        assert "status" in success_fields
        assert "message" in success_fields

    def test_mutation_decorator_uses_better_defaults(self):
        """Test that @mutation decorator uses improved defaults."""
        from fraiseql.mutations.error_config import DEFAULT_ERROR_CONFIG

        # The improved defaults should handle FraiseQL patterns
        assert "noop:" in DEFAULT_ERROR_CONFIG.error_as_data_prefixes
        assert "blocked:" in DEFAULT_ERROR_CONFIG.error_as_data_prefixes
        assert "duplicate:" in DEFAULT_ERROR_CONFIG.error_as_data_prefixes

        # Should have comprehensive success keywords
        assert "created" in DEFAULT_ERROR_CONFIG.success_keywords
        assert "updated" in DEFAULT_ERROR_CONFIG.success_keywords
        assert "deleted" in DEFAULT_ERROR_CONFIG.success_keywords
