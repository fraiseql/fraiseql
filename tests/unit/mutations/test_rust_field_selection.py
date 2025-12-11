"""Test Rust field selection filtering directly."""
import json
import pytest
from fraiseql import _get_fraiseql_rs


@pytest.fixture
def fraiseql_rs():
    """Get Rust module."""
    return _get_fraiseql_rs()


def test_rust_filters_success_fields_correctly(fraiseql_rs):
    """Verify Rust only returns requested fields in success response."""

    # Fake mutation result from database
    fake_result = {
        "status": "success",
        "message": "Machine created successfully",
        "entity_id": "123e4567-e89b-12d3-a456-426614174000",
        "entity_type": "Machine",
        "entity": {
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "name": "Test Machine",
            "contractId": "contract-1"
        },
        "updated_fields": ["name", "contractId"],
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
    }

    # Only request 'status' and 'machine' fields (NOT message, errors, updatedFields, id)
    selected_fields = ["status", "machine"]

    response_json = fraiseql_rs.build_mutation_response(
        json.dumps(fake_result),  # mutation_json
        "createMachine",          # field_name
        "CreateMachineSuccess",   # success_type
        "CreateMachineError",     # error_type
        "machine",                # entity_field_name (Option)
        "Machine",                # entity_type (Option)
        None,                     # cascade_selections (Option)
        True,                     # auto_camel_case (bool)
        selected_fields,          # success_type_fields (Option)
    )

    response = json.loads(response_json)
    data = response["data"]["createMachine"]

    # Should have __typename (always)
    assert "__typename" in data
    assert data["__typename"] == "CreateMachineSuccess"

    # Should have requested fields
    assert "status" in data
    assert data["status"] == "success"
    assert "machine" in data
    assert data["machine"]["id"] == "123e4567-e89b-12d3-a456-426614174000"

    # Should NOT have unrequested fields
    assert "message" not in data, f"message should not be in response (not requested), got keys: {list(data.keys())}"
    assert "errors" not in data, "errors should not be present"
    assert "updatedFields" not in data, "updatedFields should not be present"
    assert "id" not in data, "id should not be present"

    print(f"✅ Rust filtering works: only {list(data.keys())} present")


def test_rust_returns_all_fields_when_all_requested(fraiseql_rs):
    """Verify all fields returned when all are requested."""

    fake_result = {
        "status": "success",
        "message": "Created",
        "entity_id": "123",
        "entity_type": "Machine",
        "entity": {"id": "123", "name": "Test"},
        "updated_fields": ["name"],
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
    }

    # Request ALL fields
    selected_fields = ["status", "message", "errors", "updatedFields", "id", "machine"]

    response_json = fraiseql_rs.build_mutation_response(
        json.dumps(fake_result),  # mutation_json
        "createMachine",          # field_name
        "CreateMachineSuccess",   # success_type
        "CreateMachineError",     # error_type
        "machine",                # entity_field_name (Option)
        "Machine",                # entity_type (Option)
        None,                     # cascade_selections (Option)
        True,                     # auto_camel_case (bool)
        selected_fields,          # success_type_fields (Option)
    )

    response = json.loads(response_json)
    data = response["data"]["createMachine"]

    # All requested fields should be present
    assert "status" in data
    assert "message" in data
    assert "errors" in data
    assert "updatedFields" in data
    assert "id" in data
    assert "machine" in data

    print(f"✅ All fields present when requested: {sorted(data.keys())}")


def test_rust_backward_compat_none_selection(fraiseql_rs):
    """Verify None selection returns all fields (backward compatibility)."""

    fake_result = {
        "status": "success",
        "message": "Created",
        "entity_id": "123",
        "entity_type": "Machine",
        "entity": {"id": "123", "name": "Test"},
        "updated_fields": ["name"],
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
    }

    # No field selection (None) - should return ALL fields
    response_json = fraiseql_rs.build_mutation_response(
        json.dumps(fake_result),  # mutation_json
        "createMachine",          # field_name
        "CreateMachineSuccess",   # success_type
        "CreateMachineError",     # error_type
        "machine",                # entity_field_name (Option)
        "Machine",                # entity_type (Option)
        None,                     # cascade_selections (Option)
        True,                     # auto_camel_case (bool)
        None,                     # success_type_fields (None = no filtering)
    )

    response = json.loads(response_json)
    data = response["data"]["createMachine"]

    # All fields should be present (no filtering)
    assert "status" in data
    assert "message" in data
    assert "errors" in data
    assert "updatedFields" in data
    assert "id" in data
    assert "machine" in data

    print("✅ Backward compat: None selection returns all fields")


def test_rust_error_response_field_filtering(fraiseql_rs):
    """Verify error responses also respect field selection."""

    fake_error = {
        "status": "failed",
        "message": "Validation error",
        "entity_id": None,
        "entity_type": None,
        "entity": None,
        "updated_fields": None,
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
        "errors": [
            {"code": "VALIDATION_ERROR", "message": "Invalid input"}
        ],
    }

    # Only request 'errors' field
    selected_fields = ["errors"]

    response_json = fraiseql_rs.build_mutation_response(
        json.dumps(fake_error),   # mutation_json
        "createMachine",          # field_name
        "CreateMachineSuccess",   # success_type
        "CreateMachineError",     # error_type
        "machine",                # entity_field_name (Option)
        "Machine",                # entity_type (Option)
        None,                     # cascade_selections (Option)
        True,                     # auto_camel_case (bool)
        selected_fields,          # success_type_fields (Option)
    )

    response = json.loads(response_json)
    data = response["data"]["createMachine"]

    # Should have __typename
    assert "__typename" in data
    assert data["__typename"] == "CreateMachineError"

    # Should have requested errors field
    assert "errors" in data
    assert len(data["errors"]) == 1

    # Should NOT have unrequested fields
    assert "code" not in data, "code should not be in response (not a top-level field)"
    assert "message" not in data, f"message should not be present (not requested), got keys: {list(data.keys())}"
    assert "status" not in data, "status should not be present"

    print(f"✅ Error response filtering works: {list(data.keys())}")
