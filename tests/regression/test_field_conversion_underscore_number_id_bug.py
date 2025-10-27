"""Test for field conversion bug with underscore+number+_id pattern.

This test reproduces the bug where fields like dns_1_id are incorrectly
transformed to dns_1 when passed to PostgreSQL functions.
"""

import uuid
from typing import Any

import pytest

import fraiseql
from fraiseql.mutations import mutation
from fraiseql.types.definitions import UNSET


@fraiseql.input
class CreateNetworkConfigurationInput:
    """Input for creating a network configuration with numbered DNS fields."""

    dns_1_id: uuid.UUID | None = UNSET  # Should remain as dns_1_id
    dns_2_id: uuid.UUID | None = UNSET  # Should remain as dns_2_id
    backup_1_id: uuid.UUID | None = UNSET  # Should remain as backup_1_id
    gateway_id: uuid.UUID | None = UNSET  # Should remain as gateway_id (works correctly)
    name: str


@fraiseql.success
class CreateNetworkConfigurationSuccess:
    """Success response for network configuration creation."""

    network_configuration: dict[str, Any]
    message: str = "Network configuration created successfully"


@fraiseql.failure
class CreateNetworkConfigurationError:
    """Error response for network configuration creation."""

    message: str
    error_code: str


@mutation(function="create_network_configuration", schema="app")
class CreateNetworkConfiguration:
    """Create a new network configuration."""

    input: CreateNetworkConfigurationInput
    success: CreateNetworkConfigurationSuccess
    failure: CreateNetworkConfigurationError


# Mock database function execution to capture the actual parameters being passed
class MockDatabase:
    """Mock database to capture function calls."""

    def __init__(self):
        self.last_function_call = None
        self.last_input_data = None

    async def execute_function(
        self, function_name: str, input_data: dict[str, Any]
    ) -> dict[str, Any]:
        """Mock function execution that captures parameters."""
        self.last_function_call = function_name
        self.last_input_data = input_data

        # Debug: Print the actual input data to see what's being passed
        print(f"DEBUG - Function called: {function_name}")
        print(f"DEBUG - Input data keys: {list(input_data.keys())}")
        print(f"DEBUG - Full input data: {input_data}")

        # Return a success response matching the expected structure
        return {
            "status": "success",
            "object_data": {
                "id": str(uuid.uuid4()),
                "name": input_data.get("name", "Test Config"),
                "dns_1_id": input_data.get("dns_1_id"),
                "dns_2_id": input_data.get("dns_2_id"),
                "backup_1_id": input_data.get("backup_1_id"),
                "gateway_id": input_data.get("gateway_id"),
            },
            "message": "Network configuration created successfully",
            "extra_metadata": {"entity": "network_configuration"},
        }


class MockInfo:
    """Mock GraphQL info object."""

    def __init__(self, db: MockDatabase):
        self.context = {"db": db}


@pytest.mark.asyncio
async def test_dns_1_id_field_not_transformed():
    """Test that dns_1_id is not incorrectly transformed to dns_1.

    This is the RED test - it should fail initially due to the bug.
    """
    # Arrange
    mock_db = MockDatabase()
    mock_info = MockInfo(mock_db)

    dns_1_uuid = uuid.uuid4()
    dns_2_uuid = uuid.uuid4()
    backup_1_uuid = uuid.uuid4()
    gateway_uuid = uuid.uuid4()

    input_data = CreateNetworkConfigurationInput(
        dns_1_id=dns_1_uuid,
        dns_2_id=dns_2_uuid,
        backup_1_id=backup_1_uuid,
        gateway_id=gateway_uuid,
        name="Test Network Config",
    )

    # Get the resolver from the mutation
    resolver = CreateNetworkConfiguration.__fraiseql_resolver__

    # Act
    result = await resolver(mock_info, input_data)

    # Assert - The function should receive the correct field names
    assert mock_db.last_function_call == "app.create_network_configuration"

    # Check that the input data contains the correct field names (not transformed)
    input_dict = mock_db.last_input_data

    # These assertions will FAIL initially due to the bug
    assert "dns_1_id" in input_dict, "dns_1_id should be preserved, not transformed to dns_1"
    assert "dns_2_id" in input_dict, "dns_2_id should be preserved, not transformed to dns_2"
    assert "backup_1_id" in input_dict, (
        "backup_1_id should be preserved, not transformed to backup_1"
    )
    assert "gateway_id" in input_dict, "gateway_id should be preserved (currently works)"

    # These should NOT exist (they would indicate the bug is present)
    assert "dns_1" not in input_dict, "dns_1 should NOT exist - indicates incorrect transformation"
    assert "dns_2" not in input_dict, "dns_2 should NOT exist - indicates incorrect transformation"
    assert "backup_1" not in input_dict, (
        "backup_1 should NOT exist - indicates incorrect transformation"
    )

    # Verify the values are correct
    assert str(input_dict["dns_1_id"]) == str(dns_1_uuid)
    assert str(input_dict["dns_2_id"]) == str(dns_2_uuid)
    assert str(input_dict["backup_1_id"]) == str(backup_1_uuid)
    assert str(input_dict["gateway_id"]) == str(gateway_uuid)


@pytest.mark.asyncio
async def test_various_underscore_number_id_patterns():
    """Test various patterns of underscore+number+_id fields."""
    mock_db = MockDatabase()
    mock_info = MockInfo(mock_db)

    # Create a more comprehensive input type for this test
    @fraiseql.input
    class TestInput:
        server_1_id: uuid.UUID | None = UNSET
        server_2_id: uuid.UUID | None = UNSET
        host_3_id: uuid.UUID | None = UNSET
        backup_10_id: uuid.UUID | None = UNSET  # Test double digits
        primary_id: uuid.UUID | None = UNSET  # No number, should work
        name: str

    @fraiseql.success
    class TestSuccess:
        result: dict[str, Any]

    @fraiseql.failure
    class TestError:
        message: str

    @mutation(function="test_function", schema="app")
    class TestMutation:
        input: TestInput
        success: TestSuccess
        failure: TestError

    # Create test UUIDs
    server_1_uuid = uuid.uuid4()
    server_2_uuid = uuid.uuid4()
    host_3_uuid = uuid.uuid4()
    backup_10_uuid = uuid.uuid4()
    primary_uuid = uuid.uuid4()

    input_data = TestInput(
        server_1_id=server_1_uuid,
        server_2_id=server_2_uuid,
        host_3_id=host_3_uuid,
        backup_10_id=backup_10_uuid,
        primary_id=primary_uuid,
        name="Test",
    )

    # Execute
    resolver = TestMutation.__fraiseql_resolver__
    await resolver(mock_info, input_data)

    # Verify all field names are preserved
    input_dict = mock_db.last_input_data

    expected_fields = [
        "server_1_id",
        "server_2_id",
        "host_3_id",
        "backup_10_id",
        "primary_id",
        "name",
    ]

    for field in expected_fields:
        assert field in input_dict, f"Field {field} should be preserved in input data"

    # Verify transformed versions don't exist
    bad_fields = ["server_1", "server_2", "host_3", "backup_10"]

    for field in bad_fields:
        assert field not in input_dict, (
            f"Field {field} should NOT exist - indicates incorrect transformation"
        )


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
