"""Test Rust mutation binding exists and works."""

import pytest


def test_rust_binding_exists():
    """Test that build_mutation_response is exposed to Python."""
    assert hasattr(_fraiseql_rs, "build_mutation_response")


def test_rust_binding_simple_format():
    """Test simple format (just entity JSONB) transformation."""
    import json


    # Simple format: just entity data, no status wrapper
    mutation_json = json.dumps({"id": "123", "first_name": "John", "email": "john@example.com"})

    response_bytes = _fraiseql_rs.build_mutation_response(
        mutation_json,
        "createUser",  # GraphQL field name
        "CreateUserSuccess",  # Success type name
        "CreateUserError",  # Error type name
        "user",  # entity_field_name
        "User",  # entity_type for __typename
        None,  # cascade_selections
    )

    response = json.loads(response_bytes)
    assert response["data"]["createUser"]["__typename"] == "CreateUserSuccess"
    assert response["data"]["createUser"]["user"]["__typename"] == "User"
    assert response["data"]["createUser"]["user"]["firstName"] == "John"


def test_rust_binding_v2_success():
    """Test v2 format (full mutation_result) transformation."""
    import json


    mutation_json = json.dumps(
        {
            "status": "new",
            "message": "User created",
            "entity_id": "123",
            "entity_type": "User",
            "entity": {"id": "123", "first_name": "John"},
            "updated_fields": None,
            "cascade": None,
            "metadata": None,
        }
    )

    response_bytes = _fraiseql_rs.build_mutation_response(
        mutation_json,
        "createUser",
        "CreateUserSuccess",
        "CreateUserError",
        "user",  # entity_field_name
        None,  # entity_type (comes from JSON in v2)
        None,  # cascade_selections
    )

    response = json.loads(response_bytes)
    assert response["data"]["createUser"]["__typename"] == "CreateUserSuccess"
    assert response["data"]["createUser"]["user"]["firstName"] == "John"


def test_rust_binding_error():
    """Test error mutation transformation."""
    import json


    mutation_json = json.dumps(
        {
            "status": "failed:validation",
            "message": "Invalid email",
            "entity_id": None,
            "entity_type": None,
            "entity": None,
            "updated_fields": None,
            "cascade": None,
            "metadata": None,
        }
    )

    response_bytes = _fraiseql_rs.build_mutation_response(
        mutation_json,
        "createUser",
        "CreateUserSuccess",
        "CreateUserError",
        "user",  # entity_field_name
        None,  # entity_type
        None,  # cascade_selections
    )

    response = json.loads(response_bytes)
    assert response["data"]["createUser"]["__typename"] == "CreateUserError"
    assert response["data"]["createUser"]["code"] == 422


def test_rust_binding_invalid_json():
    """Test error handling for invalid JSON."""
    with pytest.raises(ValueError, match="Invalid JSON"):
        _fraiseql_rs.build_mutation_response(
            "not valid json",
            "createUser",
            "CreateUserSuccess",
            "CreateUserError",
            "user",
            None,
        )
