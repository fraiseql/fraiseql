"""Test automatic mutation field injection without inheritance."""

import pytest

import fraiseql
from fraiseql import Error
from fraiseql.mutations.error_config import DEFAULT_ERROR_CONFIG
from fraiseql.mutations.parser import parse_mutation_result


@pytest.mark.unit
class TestAutoMutationFields:
    """Test that @fraiseql.success and @fraiseql.failure automatically inject standard fields."""

    def test_success_auto_injects_standard_fields(self):
        """Test that @success automatically adds status, message, errors fields."""

        @fraiseql.success
        class CreateUserSuccess:
            """Success response without explicit inheritance."""

            user: dict | None = None

        # Should have auto-injected standard fields
        assert hasattr(CreateUserSuccess, "__fraiseql_definition__")
        definition = CreateUserSuccess.__fraiseql_definition__
        field_names = set(definition.fields.keys())

        # Should have both custom and auto-injected fields
        assert "user" in field_names  # Custom field
        assert "status" in field_names  # Auto-injected
        assert "message" in field_names  # Auto-injected
        assert "errors" in field_names  # Auto-injected

    def test_failure_auto_injects_standard_fields(self):
        """Test that @failure automatically adds status, message, errors fields."""

        @fraiseql.failure
        class CreateUserError:
            """Error response without explicit inheritance."""

            conflict_user: dict | None = None

        # Should have auto-injected standard fields
        assert hasattr(CreateUserError, "__fraiseql_definition__")
        definition = CreateUserError.__fraiseql_definition__
        field_names = set(definition.fields.keys())

        # Should have both custom and auto-injected fields
        assert "conflict_user" in field_names  # Custom field
        assert "status" in field_names  # Auto-injected
        assert "message" in field_names  # Auto-injected
        assert "errors" in field_names  # Auto-injected

    def test_parser_works_with_auto_injected_fields(self):
        """Test that mutation parser works with auto-injected fields."""

        @fraiseql.success
        class TestSuccess:
            entity: dict | None = None

        @fraiseql.failure
        class TestError:
            conflict_entity: dict | None = None

        # Test successful case
        success_result = {
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "updated_fields": [],
            "status": "success",
            "message": "Entity created successfully",
            "object_data": {"entity": {"id": "test-id", "name": "Test Entity"}},
            "extra_metadata": {},
        }

        parsed_success = parse_mutation_result(
            success_result, TestSuccess, TestError, DEFAULT_ERROR_CONFIG
        )

        # Should be success type with auto-populated fields
        assert isinstance(parsed_success, TestSuccess)
        assert parsed_success.status == "success"
        assert parsed_success.message == "Entity created successfully"
        assert parsed_success.errors is None  # Success shouldn't have errors
        assert parsed_success.entity is not None

    def test_error_case_with_auto_injected_fields(self):
        """Test error case with auto-injected fields and error auto-population."""

        @fraiseql.success
        class TestSuccess:
            entity: dict | None = None

        @fraiseql.failure
        class TestError:
            conflict_entity: dict | None = None

        # Test error case (noop: is error-as-data)
        error_result = {
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "updated_fields": [],
            "status": "noop:already_exists",
            "message": "Entity already exists",
            "object_data": {"conflicting_entity": {"id": "existing-id", "name": "Existing Entity"}},
            "extra_metadata": {"conflict_id": "existing-id"},
        }

        parsed_error = parse_mutation_result(
            error_result, TestSuccess, TestError, DEFAULT_ERROR_CONFIG
        )

        # Should be error type with auto-populated fields
        assert isinstance(parsed_error, TestError)
        assert parsed_error.status == "noop:already_exists"
        assert parsed_error.message == "Entity already exists"

        # Errors should be auto-populated with FraiseQL's default Error type
        assert parsed_error.errors is not None
        assert len(parsed_error.errors) == 1
        assert isinstance(parsed_error.errors[0], Error)
        assert parsed_error.errors[0].message == "Entity already exists"
        assert parsed_error.errors[0].identifier == "already_exists"

    def test_explicit_fields_override_auto_injection(self):
        """Test that explicitly defined fields override auto-injection."""

        @fraiseql.success
        class CustomSuccess:
            status: str = "custom_success"  # Override default
            message: str = "Custom message"  # Override default
            errors: list[Error] | None = None  # Override default
            entity: dict | None = None

        # Should still have all fields but with custom types/defaults
        definition = CustomSuccess.__fraiseql_definition__
        field_names = set(definition.fields.keys())

        assert "status" in field_names
        assert "message" in field_names
        assert "errors" in field_names
        assert "entity" in field_names

    def test_no_inheritance_needed_for_fraiseql_patterns(self):
        """Test that FraiseQL patterns work without any inheritance."""

        # This should work exactly like FraiseQL's current patterns
        @fraiseql.input
        class CreateContractInput:
            name: str
            customer_contract_id: str

        @fraiseql.success  # No (MutationResultBase) needed!
        class CreateContractSuccess:
            contract: dict | None = None

        @fraiseql.failure  # No (MutationResultBase) needed!
        class CreateContractError:
            conflict_contract: dict | None = None

        # Should work exactly like if they inherited from MutationResultBase
        success_fields = set(CreateContractSuccess.__fraiseql_definition__.fields.keys())
        error_fields = set(CreateContractError.__fraiseql_definition__.fields.keys())

        # Should have FraiseQL's standard fields auto-injected
        expected_base_fields = {"status", "message", "errors"}
        assert expected_base_fields.issubset(success_fields)
        assert expected_base_fields.issubset(error_fields)

        # Should have their specific fields too
        assert "contract" in success_fields
        assert "conflict_contract" in error_fields
