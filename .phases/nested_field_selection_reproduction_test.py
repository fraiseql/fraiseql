"""Reproduction test for nested JSONB field selection bug.

This test demonstrates that field selection works for top-level queries but
does NOT work for nested JSONB objects embedded in parent data.

Issue: When a query requests specific fields from a nested object
(e.g., networkConfiguration { id ipAddress }), FraiseQL returns ALL fields
instead of just the requested ones.

Expected: Only requested fields in response
Actual: All fields from JSONB data in response
"""

import uuid
from typing import Any

import pytest
from graphql import GraphQLResolveInfo

import fraiseql
from fraiseql.db import FraiseQLRepository


# Test types for nested field selection
@fraiseql.type
class NetworkConfig:
    """Nested JSONB object type."""

    id: uuid.UUID
    ip_address: str | None = None
    subnet_mask: str | None = None
    gateway: str | None = None
    dns_server: str | None = None
    # Many more fields that should NOT be returned if not requested


@fraiseql.type(jsonb_column="data")
class Device:
    """Parent type with nested JSONB object."""

    id: uuid.UUID
    name: str
    network_config: NetworkConfig | None = None


pytestmark = pytest.mark.asyncio


async def test_top_level_field_selection_works(mock_db_pool):
    """Verify that field selection works for top-level queries (baseline)."""
    # This test should PASS - top-level field selection works
    db = FraiseQLRepository(mock_db_pool)

    # Create mock info with field selection for { id name }
    mock_info = create_mock_info_with_selection(["id", "name"])

    # Simulate query execution
    # NOTE: This is a simplified test - in reality we'd need full GraphQL execution
    # But we can verify that field_paths are correctly extracted from info

    from fraiseql.core.ast_parser import extract_field_paths_from_info
    from fraiseql.utils.casing import to_snake_case

    field_paths = extract_field_paths_from_info(mock_info, transform_path=to_snake_case)

    # Verify field paths extracted correctly
    assert field_paths is not None
    assert len(field_paths) == 2
    assert any(fp.path == ["id"] for fp in field_paths)
    assert any(fp.path == ["name"] for fp in field_paths)


async def test_nested_field_selection_broken(mock_db_pool):
    """Demonstrate that nested field selection does NOT work (BUG).

    When querying:
        devices {
            id
            name
            networkConfig { id ipAddress }
        }

    Expected behavior:
        - Top-level: Only id, name returned ✅
        - Nested: Only id, ipAddress returned ❌ BROKEN

    Actual behavior:
        - Top-level: Only id, name returned ✅
        - Nested: ALL fields returned (ip_address, subnet_mask, gateway, dns_server, etc.) ❌
    """
    # This test should FAIL - demonstrating the bug

    # Create mock nested data (simulating what comes from database)
    network_config_data = {
        "id": str(uuid.uuid4()),
        "ip_address": "192.168.1.100",
        "subnet_mask": "255.255.255.0",
        "gateway": "192.168.1.1",
        "dns_server": "8.8.8.8",
        # These fields should NOT be in response if not requested
    }

    device_data = {
        "id": str(uuid.uuid4()),
        "name": "Test Device",
        "network_config": network_config_data,
    }

    # Create parent object
    parent = Device(**device_data)

    # Create mock info for nested field selection: { id ipAddress }
    mock_info = create_mock_info_with_nested_selection(
        parent_fields=["id", "name", "networkConfig"],
        nested_field="networkConfig",
        nested_fields=["id", "ipAddress"],
    )

    # Simulate the nested field resolver
    from fraiseql.core.nested_field_resolver import create_smart_nested_field_resolver

    resolver = create_smart_nested_field_resolver("network_config", NetworkConfig | None)

    # Execute resolver
    result = await resolver(parent, mock_info)

    # BUG: Result contains ALL fields from JSONB, not just selected ones
    assert result is not None
    assert hasattr(result, "id")  # ✅ Requested field
    assert hasattr(result, "ip_address")  # ✅ Requested field

    # These assertions will FAIL because the resolver returns ALL fields
    # instead of applying field selection
    assert not hasattr(result, "subnet_mask"), "BUG: subnet_mask should not be in response"
    assert not hasattr(result, "gateway"), "BUG: gateway should not be in response"
    assert not hasattr(result, "dns_server"), "BUG: dns_server should not be in response"


# Helper functions for creating mock GraphQL info


def create_mock_info_with_selection(fields: list[str]) -> Any:
    """Create a mock GraphQLResolveInfo with field selection."""
    from unittest.mock import MagicMock

    from graphql import FieldNode, SelectionSetNode

    mock_info = MagicMock(spec=GraphQLResolveInfo)
    mock_info.field_nodes = [MagicMock(spec=FieldNode)]
    mock_info.field_nodes[0].selection_set = MagicMock(spec=SelectionSetNode)
    mock_info.fragments = {}

    # Create mock selections for requested fields
    mock_selections = []
    for field_name in fields:
        field_node = MagicMock(spec=FieldNode)
        field_node.name.value = field_name
        field_node.alias = None
        field_node.selection_set = None
        mock_selections.append(field_node)

    mock_info.field_nodes[0].selection_set.selections = mock_selections

    return mock_info


def create_mock_info_with_nested_selection(
    parent_fields: list[str],
    nested_field: str,
    nested_fields: list[str],
) -> Any:
    """Create a mock GraphQLResolveInfo with nested field selection."""
    from unittest.mock import MagicMock

    from graphql import FieldNode, SelectionSetNode

    mock_info = MagicMock(spec=GraphQLResolveInfo)
    mock_info.field_nodes = [MagicMock(spec=FieldNode)]
    mock_info.field_nodes[0].selection_set = MagicMock(spec=SelectionSetNode)
    mock_info.fragments = {}

    # Create parent-level selections
    mock_selections = []
    for field_name in parent_fields:
        field_node = MagicMock(spec=FieldNode)
        field_node.name.value = field_name
        field_node.alias = None

        # Add nested selection set for the nested field
        if field_name == nested_field:
            nested_selection_set = MagicMock(spec=SelectionSetNode)
            nested_selections = []

            for nested_field_name in nested_fields:
                nested_field_node = MagicMock(spec=FieldNode)
                nested_field_node.name.value = nested_field_name
                nested_field_node.alias = None
                nested_field_node.selection_set = None
                nested_selections.append(nested_field_node)

            nested_selection_set.selections = nested_selections
            field_node.selection_set = nested_selection_set
        else:
            field_node.selection_set = None

        mock_selections.append(field_node)

    mock_info.field_nodes[0].selection_set.selections = mock_selections

    return mock_info


@pytest.fixture
def mock_db_pool():
    """Create a mock database pool for testing."""
    from unittest.mock import AsyncMock, MagicMock

    mock_pool = MagicMock()
    mock_pool.connection = AsyncMock()
    return mock_pool
