"""Integration tests for FraiseQL framework patterns with built-in defaults."""

from typing import Any

import pytest

import fraiseql
from fraiseql.mutations.error_config import DEFAULT_ERROR_CONFIG
from fraiseql.mutations.parser import parse_mutation_result


@pytest.mark.integration
class TestFraiseQLStyleMutations:
    """Test that FraiseQL patterns work seamlessly with FraiseQL defaults."""

    def test_simple_mutation_with_defaults_only(self):
        """Test creating a mutation using only FraiseQL built-in types."""
        from fraiseql import MutationResultBase

        # This demonstrates typical FraiseQL mutation structure with framework defaults
        @fraiseql.input
        class CreateContractInput:
            name: str
            customer_contract_id: str

        @fraiseql.success
        class CreateContractSuccess(MutationResultBase):
            contract: dict[str, Any] | None = None

        @fraiseql.failure
        class CreateContractError(MutationResultBase):
            conflict_contract: dict[str, Any] | None = None

        # Should work exactly like FraiseQL's custom types
        assert hasattr(CreateContractSuccess, "__fraiseql_definition__")
        assert hasattr(CreateContractError, "__fraiseql_definition__")

        # Both should inherit all the fields from MutationResultBase
        success_fields = set(CreateContractSuccess.__fraiseql_definition__.fields.keys())
        error_fields = set(CreateContractError.__fraiseql_definition__.fields.keys())

        # Should have FraiseQL's standard fields
        expected_base_fields = {"status", "message", "errors"}
        assert expected_base_fields.issubset(success_fields)
        assert expected_base_fields.issubset(error_fields)

        # Should have their specific fields too
        assert "contract" in success_fields
        assert "conflict_contract" in error_fields

    def test_mutation_decorator_with_improved_defaults(self):
        """Test that @fraiseql.mutation uses the improved DEFAULT_ERROR_CONFIG."""
        from fraiseql import MutationResultBase

        @fraiseql.input
        class TestInput:
            name: str

        @fraiseql.success
        class TestSuccess(MutationResultBase):
            entity: dict | None = None

        @fraiseql.failure
        class TestError(MutationResultBase):
            conflict_entity: dict | None = None

        # Test that DEFAULT_ERROR_CONFIG handles FraiseQL patterns
        assert "noop:" in DEFAULT_ERROR_CONFIG.error_as_data_prefixes
        assert "blocked:" in DEFAULT_ERROR_CONFIG.error_as_data_prefixes
        assert "duplicate:" in DEFAULT_ERROR_CONFIG.error_as_data_prefixes
        assert "created" in DEFAULT_ERROR_CONFIG.success_keywords
        assert "cancelled" in DEFAULT_ERROR_CONFIG.success_keywords

    def test_error_auto_population_with_default_types(self):
        """Test that error auto-population works with FraiseQL's default types."""
        from fraiseql import Error, MutationResultBase

        @fraiseql.type
        class TestSuccess(MutationResultBase):
            entity: dict | None = None

        @fraiseql.type
        class TestError(MutationResultBase):
            conflict_entity: dict | None = None

        # Simulate FraiseQL's typical noop case
        result = {
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "updated_fields": [],
            "status": "noop:already_exists",
            "message": "Contract already exists",
            "object_data": {
                "conflicting_entity": {"id": "existing-id", "name": "Existing Contract"}
            },
            "extra_metadata": {"conflict_id": "existing-id"},
        }

        parsed = parse_mutation_result(result, TestSuccess, TestError, DEFAULT_ERROR_CONFIG)

        # Should be TestError (noop: is treated as error data, not success)
        assert isinstance(parsed, TestError)
        assert parsed.status == "noop:already_exists"
        assert parsed.message == "Contract already exists"

        # Errors should be auto-populated with FraiseQL's default Error type
        assert parsed.errors is not None
        assert len(parsed.errors) == 1

        error = parsed.errors[0]
        # Should be using FraiseQL's default Error type
        assert isinstance(error, Error)
        assert error.message == "Contract already exists"
        assert error.code == 409  # noop:already_exists maps to 409 (conflict)
        assert error.identifier == "already_exists"
        assert error.details == {"conflictId": "existing-id"}

    def test_no_custom_base_class_needed(self):
        """Test that FraiseQL works with standard base classes."""
        from fraiseql import MutationResultBase

        # This should work without any custom base classes
        @fraiseql.input
        class CreateUserInput:
            email: str
            name: str

        @fraiseql.success
        class CreateUserSuccess(MutationResultBase):
            user: dict | None = None

        @fraiseql.failure
        class CreateUserError(MutationResultBase):
            conflict_user: dict | None = None

        # Can be used directly with @fraiseql.mutation using framework defaults
        @fraiseql.mutation(
            function="create_user",
            schema="app",
            context_params={"tenant_id": "input_pk_organization", "user": "input_created_by"},
            # Uses DEFAULT_ERROR_CONFIG automatically - no need to specify error_config
        )
        class CreateUser:
            input: CreateUserInput
            success: CreateUserSuccess
            failure: CreateUserError

        # Should work with FraiseQL's standard mutation patterns
        assert hasattr(CreateUser, "__fraiseql_mutation__")

    def test_all_error_patterns_supported(self):
        """Test that all error patterns work with framework defaults."""
        from fraiseql import Error, MutationResultBase

        @fraiseql.type
        class TestSuccess(MutationResultBase):
            pass

        @fraiseql.type
        class TestError(MutationResultBase):
            pass

        # Test all framework status patterns
        test_cases = [
            # Success patterns
            ("success", "Created successfully", TestSuccess),
            ("created", "Entity created", TestSuccess),
            ("updated", "Entity updated", TestSuccess),
            ("cancelled", "Operation cancelled", TestSuccess),
            # Error-as-data patterns (should populate errors array)
            ("noop:already_exists", "Already exists", TestError),
            ("blocked:children", "Has dependent entities", TestError),
            ("duplicate:name", "Name already used", TestError),
            # GraphQL error patterns (would cause GraphQL errors in real usage)
            ("failed:validation", "Validation failed", TestError),
        ]

        for status, message, expected_type in test_cases:
            result = {
                "id": "123e4567-e89b-12d3-a456-426614174000",
                "updated_fields": [],
                "status": status,
                "message": message,
                "object_data": {},
                "extra_metadata": {},
            }

            parsed = parse_mutation_result(result, TestSuccess, TestError, DEFAULT_ERROR_CONFIG)
            assert isinstance(parsed, expected_type), (
                f"Status '{status}' should return {expected_type.__name__}"
            )

            if expected_type == TestError:
                # Error cases should have auto-populated errors
                assert parsed.errors is not None
                assert len(parsed.errors) >= 1
                assert isinstance(parsed.errors[0], Error)

    def test_default_config_comprehensive(self):
        """Test that DEFAULT_ERROR_CONFIG is comprehensive for framework patterns."""
        # The enhanced DEFAULT_ERROR_CONFIG should handle all framework needs
        assert "noop:" in DEFAULT_ERROR_CONFIG.error_as_data_prefixes
        assert "blocked:" in DEFAULT_ERROR_CONFIG.error_as_data_prefixes
        assert "duplicate:" in DEFAULT_ERROR_CONFIG.error_as_data_prefixes
        assert "created" in DEFAULT_ERROR_CONFIG.success_keywords
        assert "cancelled" in DEFAULT_ERROR_CONFIG.success_keywords

        # Should properly classify failed: as GraphQL errors (not data)

        from fraiseql import MutationResultBase

        @fraiseql.type
        class TestSuccess(MutationResultBase):
            pass

        @fraiseql.type
        class TestError(MutationResultBase):
            pass

        # With improved DEFAULT_ERROR_CONFIG, "failed:" triggers GraphQL errors
        assert DEFAULT_ERROR_CONFIG.is_error_status("failed:validation") is True
        assert DEFAULT_ERROR_CONFIG.is_error_status("noop:already_exists") is False
