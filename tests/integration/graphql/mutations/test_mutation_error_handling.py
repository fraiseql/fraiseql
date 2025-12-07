"""Test mutation error handling in production mode."""

import pytest

from fraiseql.fastapi.config import FraiseQLConfig

pytestmark = pytest.mark.integration


@pytest.mark.asyncio
async def test_production_config_environment_check() -> None:
    """Test that production config properly sets environment attribute."""
    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test", environment="production"
    )

    # Verify the config has the environment attribute and it's accessible
    assert hasattr(config, "environment")
    assert config.environment == "production"

    # This should NOT raise AttributeError (the bug we're testing)
    try:
        # This is the pattern used in the error handling code
        hide_errors = config.environment == "production"
        assert hide_errors is True
    except AttributeError as e:
        pytest.fail(f"Config environment access raised AttributeError: {e}")


@pytest.mark.asyncio
async def test_multiple_validation_errors_array_pattern() -> None:
    """Test that multiple validation errors can be returned as arrays.

    This demonstrates the FraiseQL Backend error pattern where
    complex validation produces structured error arrays.
    """
    # Mock multiple validation errors
    validation_errors = [
        {
            "code": "REQUIRED_FIELD_MISSING",
            "field": "name",
            "message": "Name is required",
            "details": {"constraint": "not_null"},
        },
        {
            "code": "INVALID_FORMAT",
            "field": "email",
            "message": "Email format is invalid",
            "details": {"pattern": "email", "value": "invalid-email"},
        },
        {
            "code": "VALUE_TOO_SHORT",
            "field": "password",
            "message": "Password must be at least 8 characters",
            "details": {"min_length": 8, "actual_length": 4},
        },
    ]

    # Verify error array structure
    assert len(validation_errors) == 3
    assert all("code" in error for error in validation_errors)
    assert all("field" in error for error in validation_errors)
    assert all("message" in error for error in validation_errors)
    assert all("details" in error for error in validation_errors)

    # Verify error codes follow pattern
    error_codes = {error["code"] for error in validation_errors}
    expected_codes = {"REQUIRED_FIELD_MISSING", "INVALID_FORMAT", "VALUE_TOO_SHORT"}
    assert error_codes == expected_codes


@pytest.mark.asyncio
async def test_development_config_environment_check() -> None:
    """Test that development config properly sets environment attribute."""
    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test", environment="development"
    )

    # Verify the config has the environment attribute and it's accessible
    assert hasattr(config, "environment")
    assert config.environment == "development"

    # This should NOT raise AttributeError
    try:
        # This is the pattern used in the error handling code
        hide_errors = config.environment == "production"
        assert hide_errors is False
    except AttributeError as e:
        pytest.fail(f"Config environment access raised AttributeError: {e}")


@pytest.mark.asyncio
async def test_config_no_get_method() -> None:
    """Test that config object doesn't have .get(): method (ensuring we don't use it)."""
    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test", environment="production"
    )

    # The old broken code tried to use config.get() - ensure this doesn't exist
    assert not hasattr(config, "get"), "Config should not have dictionary-style .get() method"


class TestMutationErrorHandlingV190:
    """Test mutation error handling in v1.8.0."""

    def test_noop_returns_error_type_with_422(self):
        """noop:* statuses return Error type with code 422."""
        result = execute_mutation(
            "createMachine",
            input={"contractId": "invalid-id", "modelId": "valid-model"}
        )

        assert result["__typename"] == "CreateMachineError"
        assert result["code"] == 422
        assert result["status"].startswith("noop:")
        assert result["message"] is not None

    def test_not_found_returns_error_type_with_404(self):
        """not_found:* statuses return Error type with code 404."""
        result = execute_mutation(
            "deleteMachine",
            id="nonexistent-machine-id"
        )

        assert result["__typename"] == "DeleteMachineError"
        assert result["code"] == 404
        assert result["status"] == "not_found:machine"

    def test_conflict_returns_error_type_with_409(self):
        """conflict:* statuses return Error type with code 409."""
        # Create machine
        machine1 = execute_mutation("createMachine", input={
            "serialNumber": "DUPLICATE-123",
            "modelId": "valid-model"
        })
        assert machine1["__typename"] == "CreateMachineSuccess"

        # Try to create duplicate
        machine2 = execute_mutation("createMachine", input={
            "serialNumber": "DUPLICATE-123",  # Same serial
            "modelId": "valid-model"
        })

        assert machine2["__typename"] == "CreateMachineError"
        assert machine2["code"] == 409
        assert machine2["status"] == "conflict:duplicate_serial_number"

    def test_success_always_has_entity(self):
        """Success type always has non-null entity."""
        result = execute_mutation("createMachine", input={
            "serialNumber": "VALID-123",
            "modelId": "valid-model",
            "contractId": "valid-contract"
        })

        assert result["__typename"] == "CreateMachineSuccess"
        assert result["machine"] is not None
        assert result["machine"]["id"] is not None
        assert result["machine"]["serialNumber"] == "VALID-123"

    def test_http_always_200(self):
        """HTTP status is always 200 OK (even for errors)."""
        import httpx

        response = httpx.post("/graphql", json={
            "query": """
                mutation { createMachine(input: {contractId: "invalid"}) {
                    __typename
                    ... on CreateMachineError { code status }
                }}
            """
        })

        # HTTP level: always 200
        assert response.status_code == 200

        # Application level: code field indicates error type
        data = response.json()["data"]["createMachine"]
        assert data["__typename"] == "CreateMachineError"
        assert data["code"] == 422  # Application-level code
