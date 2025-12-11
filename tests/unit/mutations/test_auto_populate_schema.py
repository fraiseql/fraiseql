"""Test that auto-populated fields appear in GraphQL schema."""

import pytest
from fraiseql.mutations.decorators import success, failure
from fraiseql.types import fraise_type


@fraise_type
class Machine:
    id: str
    name: str


def test_success_decorator_adds_fields_to_gql_fields():
    """Auto-populated fields should be in __gql_fields__ for schema generation."""

    @success
    class CreateMachineSuccess:
        machine: Machine

    gql_fields = getattr(CreateMachineSuccess, "__gql_fields__", {})

    # All auto-populated fields must be present
    assert "machine" in gql_fields, "Original field should be present"
    assert "status" in gql_fields, "Auto-injected status missing"
    assert "message" in gql_fields, "Auto-injected message missing"
    assert "errors" in gql_fields, "Auto-injected errors missing"
    assert "updated_fields" in gql_fields, "Auto-injected updatedFields missing"
    assert "id" in gql_fields, "Auto-injected id missing (entity detected)"

    # Verify field types
    assert gql_fields["status"].field_type == str
    assert gql_fields["message"].field_type == str | None


def test_failure_decorator_adds_fields():
    """Failure types should also get auto-populated fields."""

    @failure
    class CreateMachineError:
        error_code: str

    gql_fields = getattr(CreateMachineError, "__gql_fields__", {})

    assert "status" in gql_fields
    assert "message" in gql_fields
    assert "errors" in gql_fields
    assert "updated_fields" in gql_fields
    # Has entity field (error_code), so id should be added
    assert "id" in gql_fields


def test_no_entity_field_no_id():
    """ID should not be added when no entity field present."""

    @success
    class DeleteSuccess:
        """Deletion confirmation without entity."""

        pass

    gql_fields = getattr(DeleteSuccess, "__gql_fields__", {})

    # Standard fields should be present
    assert "status" in gql_fields
    assert "message" in gql_fields
    assert "errors" in gql_fields
    assert "updated_fields" in gql_fields

    # But NOT id (no entity field detected)
    assert "id" not in gql_fields


def test_user_defined_fields_not_overridden():
    """User's explicit field definitions should be preserved."""

    @success
    class CreateMachineSuccess:
        machine: Machine
        status: str = "custom_success"

    gql_fields = getattr(CreateMachineSuccess, "__gql_fields__", {})

    # User-defined status should be preserved
    assert "status" in gql_fields
    # But auto-injected fields should still be added
    assert "message" in gql_fields
    assert "errors" in gql_fields
