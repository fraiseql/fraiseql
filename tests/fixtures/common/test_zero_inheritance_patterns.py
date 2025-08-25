"""Test that FraiseQL now works with zero inheritance for FraiseQL patterns."""

import uuid
from typing import Any

import pytest

import fraiseql
from fraiseql.mutations.error_config import DEFAULT_ERROR_CONFIG
from fraiseql.mutations.parser import parse_mutation_result


@pytest.mark.integration
class TestZeroInheritancePatterns:
    """Test that FraiseQL patterns work without any inheritance at all."""

    def test_completely_clean_mutation_definitions(self):
        """Test the cleanest possible mutation definitions."""

        # This is now the complete FraiseQL pattern - no inheritance needed!
        @fraiseql.input
        class CreateContractInput:
            name: str
            customer_contract_id: str

        @fraiseql.success  # No (MutationResultBase) inheritance!
        class CreateContractSuccess:
            contract: dict[str, Any] | None = None

        @fraiseql.failure  # No (MutationResultBase) inheritance!
        class CreateContractError:
            conflict_contract: dict[str, Any] | None = None

        # All the standard fields are automatically present
        success_fields = set(CreateContractSuccess.__fraiseql_definition__.fields.keys())
        error_fields = set(CreateContractError.__fraiseql_definition__.fields.keys())

        # Standard mutation fields are auto-injected
        assert {"status", "message", "errors"}.issubset(success_fields)
        assert {"status", "message", "errors"}.issubset(error_fields)

        # Custom fields are preserved
        assert "contract" in success_fields
        assert "conflict_contract" in error_fields

    def test_zero_config_fraiseql_workflow(self):
        """Test complete FraiseQL workflow with zero custom configuration."""

        @fraiseql.input
        class UpdateUserInput:
            email: str
            name: str

        @fraiseql.success
        class UpdateUserSuccess:
            user: dict | None = None

        @fraiseql.failure
        class UpdateUserError:
            conflict_user: dict | None = None
            validation_errors: list[str] | None = None

        # Test all FraiseQL status patterns work out-of-the-box
        test_cases = [
            # Success patterns
            {
                "id": str(uuid.uuid4()),
                "updated_fields": ["email", "name"],
                "status": "success",
                "message": "User updated successfully",
                "object_data": {"user": {"id": "test-user", "email": "test@example.com"}},
                "extra_metadata": {},
                "expected_type": UpdateUserSuccess,
            },
            {
                "id": str(uuid.uuid4()),
                "updated_fields": [],
                "status": "created",  # FraiseQL pattern
                "message": "User created",
                "object_data": {"user": {"id": "new-user", "email": "new@example.com"}},
                "extra_metadata": {},
                "expected_type": UpdateUserSuccess,
            },
            # Error-as-data patterns (handled by DEFAULT_ERROR_CONFIG)
            {
                "id": str(uuid.uuid4()),
                "updated_fields": [],
                "status": "noop:already_exists",  # FraiseQL pattern
                "message": "User already exists",
                "object_data": {
                    "conflicting_user": {"id": "existing", "email": "test@example.com"}
                },
                "extra_metadata": {"conflict_id": "existing-user-id"},
                "expected_type": UpdateUserError,
            },
            {
                "id": str(uuid.uuid4()),
                "updated_fields": [],
                "status": "blocked:children",  # FraiseQL pattern
                "message": "User has dependent records",
                "object_data": {},
                "extra_metadata": {"dependent_count": 5},
                "expected_type": UpdateUserError,
            },
            {
                "id": str(uuid.uuid4()),
                "updated_fields": [],
                "status": "duplicate:email",  # FraiseQL pattern
                "message": "Email already in use",
                "object_data": {},
                "extra_metadata": {"duplicate_field": "email"},
                "expected_type": UpdateUserError,
            },
        ]

        for test_case in test_cases:
            expected_type = test_case.pop("expected_type")

            # Parse with zero configuration - just uses enhanced defaults
            parsed = parse_mutation_result(
                test_case,
                UpdateUserSuccess,
                UpdateUserError,
                DEFAULT_ERROR_CONFIG,  # Enhanced for FraiseQL
            )

            # Should return the expected type
            assert isinstance(parsed, expected_type), (
                f"Status '{test_case['status']}' should return {expected_type.__name__}"
            )

            # Standard fields should be populated
            assert parsed.status == test_case["status"]
            assert parsed.message == test_case["message"]

            # Error cases should have auto-populated errors array
            if expected_type == UpdateUserError:
                assert parsed.errors is not None
                assert len(parsed.errors) >= 1
                assert parsed.errors[0].message == test_case["message"]

    def test_explicit_fields_still_override_defaults(self):
        """Test that explicit field definitions override auto-injection."""

        @fraiseql.success
        class CustomFieldsSuccess:
            status: str = "custom_default"  # Override auto-injected field
            message: str = "Custom message"  # Override auto-injected field
            errors: list[fraiseql.Error] | None = []  # Override auto-injected field
            entity: dict | None = None  # Custom field

        # Should still have all fields
        fields = set(CustomFieldsSuccess.__fraiseql_definition__.fields.keys())
        assert {"status", "message", "errors", "entity"}.issubset(fields)

        # Should be able to instantiate with custom defaults
        instance = CustomFieldsSuccess(entity={"id": "test"})
        assert instance.status == "custom_default"
        assert instance.message == "Custom message"
        assert instance.errors == []
        assert instance.entity == {"id": "test"}

    def test_migration_path_from_old_to_new_pattern(self):
        """Show how easy it is to migrate from MutationResultBase to zero inheritance."""

        # Old pattern (still works but verbose)
        @fraiseql.success
        class OldPatternSuccess(fraiseql.MutationResultBase):
            user: dict | None = None

        # New pattern (much cleaner)
        @fraiseql.success
        class NewPatternSuccess:  # No inheritance needed!
            user: dict | None = None

        # Both should have identical fields
        old_fields = set(OldPatternSuccess.__fraiseql_definition__.fields.keys())
        new_fields = set(NewPatternSuccess.__fraiseql_definition__.fields.keys())

        assert old_fields == new_fields
        assert {"status", "message", "errors", "user"}.issubset(old_fields)
        assert {"status", "message", "errors", "user"}.issubset(new_fields)

    def test_works_with_complex_fraiseql_mutations(self):
        """Test that complex FraiseQL mutation patterns work seamlessly."""

        @fraiseql.input
        class CreateOrderInput:
            customer_id: str
            items: list[dict]
            shipping_address: dict

        @fraiseql.success
        class CreateOrderSuccess:
            order: dict | None = None
            invoice: dict | None = None
            tracking_number: str | None = None

        @fraiseql.failure
        class CreateOrderError:
            validation_errors: list[dict] | None = None
            inventory_conflicts: list[dict] | None = None
            payment_error: dict | None = None

        # Complex success case
        success_result = {
            "id": str(uuid.uuid4()),
            "updated_fields": ["order_id", "invoice_id", "tracking_number"],
            "status": "created",
            "message": "Order created successfully",
            "object_data": {
                "order": {"id": "ord_123", "total": 99.99},
                "invoice": {"id": "inv_123", "amount": 99.99},
                "tracking_number": "TRK_789",
            },
            "extra_metadata": {"processing_time_ms": 234, "payment_method": "credit_card"},
        }

        parsed_success = parse_mutation_result(
            success_result, CreateOrderSuccess, CreateOrderError, DEFAULT_ERROR_CONFIG
        )

        assert isinstance(parsed_success, CreateOrderSuccess)
        assert parsed_success.status == "created"
        assert parsed_success.message == "Order created successfully"
        assert parsed_success.errors is None  # Success case
        assert parsed_success.order == {"id": "ord_123", "total": 99.99}
        assert parsed_success.tracking_number == "TRK_789"

        # Complex error case
        error_result = {
            "id": str(uuid.uuid4()),
            "updated_fields": [],
            "status": "noop:inventory_conflict",
            "message": "Insufficient inventory for requested items",
            "object_data": {
                "validation_errors": [{"field": "quantity", "message": "Exceeds available stock"}],
                "inventory_conflicts": [{"item_id": "item_123", "requested": 5, "available": 2}],
            },
            "extra_metadata": {"conflict_resolution_url": "/api/inventory/conflicts/123"},
        }

        parsed_error = parse_mutation_result(
            error_result, CreateOrderSuccess, CreateOrderError, DEFAULT_ERROR_CONFIG
        )

        assert isinstance(parsed_error, CreateOrderError)
        assert parsed_error.status == "noop:inventory_conflict"
        assert parsed_error.message == "Insufficient inventory for requested items"
        assert parsed_error.errors is not None  # Auto-populated
        assert len(parsed_error.errors) == 1
        assert parsed_error.errors[0].identifier == "inventory_conflict"
        assert parsed_error.validation_errors == [
            {"field": "quantity", "message": "Exceeds available stock"}
        ]
        assert parsed_error.inventory_conflicts == [
            {"item_id": "item_123", "requested": 5, "available": 2}
        ]
