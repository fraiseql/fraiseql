"""Test multiple entity fields pattern in Success/Error types.

This pattern is used in PrintOptim for:
- Conflict scenarios (showing both new and existing entity)
- Update operations (showing before and after states)
- Related entities (showing cascaded changes)
"""

from unittest.mock import AsyncMock, MagicMock

import pytest

pytestmark = pytest.mark.integration


@pytest.mark.asyncio
async def test_success_with_multiple_entities():
    """Test Success response with multiple entity fields at root level.

    Use case: Update operation showing before and after states.

    PostgreSQL returns wrapper with multiple entities:
    entity: {
        "machine": {...},
        "previous_location": {...},
        "new_location": {...}
    }
    """
    from fraiseql.core.rust_pipeline import RustResponseBytes
    from fraiseql.mutations.rust_executor import execute_mutation_rust

    # Mock connection - mutation_response with wrapper containing multiple entities
    mock_cursor = AsyncMock()
    mock_cursor.fetchone.return_value = (
        {
            "status": "updated",
            "message": "Machine location updated",
            "entity_id": "123",
            "entity_type": "Machine",
            "entity": {
                "machine": {"id": "123", "name": "Printer-01"},
                "previous_location": {"id": "old-loc-456", "name": "Warehouse A"},
                "new_location": {"id": "new-loc-789", "name": "Warehouse B"},
            },
            "updated_fields": ["location"],
            "cascade": None,
            "metadata": None,
        },
    )

    # Mock async context manager for cursor
    mock_cursor_ctx = AsyncMock()
    mock_cursor_ctx.__aenter__.return_value = mock_cursor
    mock_cursor_ctx.__aexit__.return_value = None

    mock_conn = MagicMock()
    mock_conn.cursor.return_value = mock_cursor_ctx

    result = await execute_mutation_rust(
        conn=mock_conn,
        function_name="app.update_machine",
        input_data={"id": "123", "location": "new-loc-789"},
        field_name="updateMachine",
        success_type="UpdateMachineSuccess",
        error_type="UpdateMachineError",
        entity_field_name="machine",  # Primary entity field
        entity_type="Machine",
    )

    assert isinstance(result, RustResponseBytes)

    # Verify the response structure
    import json

    response = json.loads(bytes(result))
    success = response["data"]["updateMachine"]

    # Auto-injected fields at root
    assert success["__typename"] == "UpdateMachineSuccess"
    assert success["status"] == "updated"
    assert success["message"] == "Machine location updated"

    # Primary entity (extracted from wrapper)
    assert "machine" in success
    assert success["machine"]["id"] == "123"
    assert success["machine"]["name"] == "Printer-01"

    # Additional entities (should be copied from wrapper)
    # This is the key test - do previousLocation and newLocation get copied?
    print("\n=== Testing Multiple Entities Pattern ===")
    print(f"Response keys: {list(success.keys())}")
    print(f"Has previousLocation: {'previousLocation' in success}")
    print(f"Has newLocation: {'newLocation' in success}")

    if "previousLocation" in success:
        assert success["previousLocation"]["id"] == "old-loc-456"
        assert success["previousLocation"]["name"] == "Warehouse A"
        print("✅ previousLocation copied from wrapper")
    else:
        pytest.fail(
            "❌ previousLocation NOT copied from wrapper - multiple entities pattern not supported"
        )

    if "newLocation" in success:
        assert success["newLocation"]["id"] == "new-loc-789"
        assert success["newLocation"]["name"] == "Warehouse B"
        print("✅ newLocation copied from wrapper")
    else:
        pytest.fail(
            "❌ newLocation NOT copied from wrapper - multiple entities pattern not supported"
        )


@pytest.mark.asyncio
async def test_error_with_conflict_entity():
    """Test Error response with conflict entity.

    Use case: Create operation failed due to existing entity.

    PostgreSQL returns wrapper with conflict entity:
    entity: {
        "conflict_machine": {...}
    }
    """
    from fraiseql.core.rust_pipeline import RustResponseBytes
    from fraiseql.mutations.rust_executor import execute_mutation_rust

    # Mock connection - mutation_response with conflict entity
    mock_cursor = AsyncMock()
    mock_cursor.fetchone.return_value = (
        {
            "status": "conflict:duplicate",  # Use conflict: prefix for 409 code
            "message": "Machine with this serial number already exists",
            "entity_id": None,
            "entity_type": "Machine",
            "entity": {
                "conflict_machine": {
                    "id": "existing-123",
                    "name": "Existing Printer",
                    "serial_number": "ABC123",
                }
            },
            "updated_fields": None,
            "cascade": None,
            "metadata": None,
        },
    )

    # Mock async context manager for cursor
    mock_cursor_ctx = AsyncMock()
    mock_cursor_ctx.__aenter__.return_value = mock_cursor
    mock_cursor_ctx.__aexit__.return_value = None

    mock_conn = MagicMock()
    mock_conn.cursor.return_value = mock_cursor_ctx

    result = await execute_mutation_rust(
        conn=mock_conn,
        function_name="app.create_machine",
        input_data={"name": "Test", "serial_number": "ABC123"},
        field_name="createMachine",
        success_type="CreateMachineSuccess",
        error_type="CreateMachineError",
        entity_field_name="machine",  # Would be used for success
        entity_type="Machine",
    )

    assert isinstance(result, RustResponseBytes)

    # Verify the response structure
    import json

    response = json.loads(bytes(result))
    error = response["data"]["createMachine"]

    # Auto-injected fields at root
    assert error["__typename"] == "CreateMachineError"
    assert error["status"] == "conflict:duplicate"
    assert error["message"] == "Machine with this serial number already exists"
    assert error["code"] == 409  # Conflict

    # Conflict entity should be accessible
    print("\n=== Testing Conflict Entity Pattern ===")
    print(f"Response keys: {list(error.keys())}")
    print(f"Has conflictMachine: {'conflictMachine' in error}")

    # TDD: Expecting this to work after implementation
    assert "conflictMachine" in error, "conflictMachine should be copied from wrapper"
    assert error["conflictMachine"]["id"] == "existing-123"
    assert error["conflictMachine"]["name"] == "Existing Printer"
    # Field names are camelCased due to auto_camel_case transformation
    assert error["conflictMachine"]["serialNumber"] == "ABC123"
    print("✅ conflictMachine copied from wrapper")
