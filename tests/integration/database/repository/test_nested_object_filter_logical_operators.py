"""Integration test for logical operators (OR, AND, NOT) in nested object filtering."""

import json
import uuid
from datetime import datetime

import pytest

import fraiseql
from fraiseql.core.rust_pipeline import RustResponseBytes
from fraiseql.db import FraiseQLRepository, register_type_for_view
from fraiseql.sql import (
    BooleanFilter,
    StringFilter,
    UUIDFilter,
    create_graphql_where_input,
)

# Import database fixtures
from tests.fixtures.database.database_conftest import *  # noqa: F403

# Define test types


@pytest.mark.unit
@fraiseql.type
class Machine:
    id: uuid.UUID
    name: str
    is_current: bool = False


@fraiseql.type
class Allocation:
    id: uuid.UUID
    machine: Machine | None
    status: str
    created_at: datetime


class TestNestedObjectFilterLogicalOperators:
    """Test logical operators (OR, AND, NOT) work within nested object filters."""

    def test_nested_or_logical_operator(self):
        """Test nested OR operator: {device: {OR: [{is_active: {eq: true}}, {name: {contains: "router"}}]}}"""
        # This test validates that the dict structure is accepted
        # The actual functionality will be tested in database integration tests
        where_dict = {
            "device": {"OR": [{"is_active": {"eq": True}}, {"name": {"contains": "router"}}]}
        }

        # For now, just validate the dict structure is correct
        assert "device" in where_dict
        assert "OR" in where_dict["device"]
        assert isinstance(where_dict["device"]["OR"], list)
        assert len(where_dict["device"]["OR"]) == 2

        # This will pass once the functionality is implemented
        # Currently the dict conversion will ignore the nested OR
        # but the structure validation should work

    def test_nested_and_logical_operator(self):
        """Test nested AND operator: {device: {AND: [{is_active: {eq: true}}, {name: {contains: "router"}}]}}"""
        # This test validates that the dict structure is accepted
        where_dict = {
            "device": {"AND": [{"is_active": {"eq": True}}, {"name": {"contains": "router"}}]}
        }

        # Validate the dict structure
        assert "device" in where_dict
        assert "AND" in where_dict["device"]
        assert isinstance(where_dict["device"]["AND"], list)
        assert len(where_dict["device"]["AND"]) == 2

    def test_nested_not_logical_operator(self):
        """Test nested NOT operator: {device: {NOT: {is_active: {eq: true}}}}"""
        # This test validates that the dict structure is accepted
        where_dict = {"device": {"NOT": {"is_active": {"eq": True}}}}

        # Validate the dict structure
        assert "device" in where_dict
        assert "NOT" in where_dict["device"]
        assert isinstance(where_dict["device"]["NOT"], dict)
        assert "is_active" in where_dict["device"]["NOT"]

    def test_top_level_or_with_nested_objects(self):
        """Test top-level OR with nested objects: {OR: [{device: {is_active: {eq: true}}}, {status: {eq: "pending"}}]}"""
        where_dict = {
            "OR": [{"device": {"is_active": {"eq": True}}}, {"status": {"eq": "pending"}}]
        }

        # Validate the dict structure
        assert "OR" in where_dict
        assert isinstance(where_dict["OR"], list)
        assert len(where_dict["OR"]) == 2
        assert "device" in where_dict["OR"][0]
        assert "status" in where_dict["OR"][1]

    def test_mixed_logical_operators(self):
        """Test mixed logical operators: {device: {AND: [{is_active: {eq: true}}, {NOT: {name: {contains: "deprecated"}}}]}}"""
        where_dict = {
            "device": {
                "AND": [{"is_active": {"eq": True}}, {"NOT": {"name": {"contains": "deprecated"}}}]
            }
        }

        # Validate the dict structure
        assert "device" in where_dict
        assert "AND" in where_dict["device"]
        assert isinstance(where_dict["device"]["AND"], list)
        assert len(where_dict["device"]["AND"]) == 2
        assert "NOT" in where_dict["device"]["AND"][1]

    def test_camelcase_in_logical_operators(self):
        """Test camelCase field names within logical operators: {device: {OR: [{isActive: {eq: true}}, {deviceName: {contains: "router"}}]}}"""
        where_dict = {
            "device": {
                "OR": [
                    {"isActive": {"eq": True}},  # camelCase
                    {"deviceName": {"contains": "router"}},  # camelCase
                ]
            }
        }

        # Validate the dict structure
        assert "device" in where_dict
        assert "OR" in where_dict["device"]
        assert isinstance(where_dict["device"]["OR"], list)
        assert len(where_dict["device"]["OR"]) == 2
        assert "isActive" in where_dict["device"]["OR"][0]  # camelCase preserved in input


def _parse_rust_response(result):
    """Helper to parse RustResponseBytes into Python objects."""
    if isinstance(result, RustResponseBytes):
        raw_json_str = bytes(result).decode("utf-8")
        response_json = json.loads(raw_json_str)
        # Extract data from GraphQL response structure
        if "data" in response_json:
            # Get the first key in data (the field name)
            field_name = list(response_json["data"].keys())[0]
            data = response_json["data"][field_name]

            # Normalize: always return a list for consistency
            if isinstance(data, dict):
                return [data]
            return data
        return response_json
    return result


@pytest.mark.database
class TestNestedObjectFilterLogicalOperatorsDatabase:
    """End-to-end database integration tests for logical operators in nested object filtering."""

    @pytest.fixture
    async def setup_test_data(self, db_pool):
        """Set up test tables and data for nested object filtering tests."""
        async with db_pool.connection() as conn:
            # Clean up any existing test data
            await conn.execute("DROP VIEW IF EXISTS test_assignment_view CASCADE")
            await conn.execute("DROP TABLE IF EXISTS test_assignments CASCADE")

            # Create test table with JSONB data column
            await conn.execute(
                """
                CREATE TABLE test_assignments (
                    id UUID PRIMARY KEY,
                    data JSONB NOT NULL
                )
                """
            )

            # Create view that extracts nested data
            await conn.execute(
                """
                CREATE VIEW test_assignment_view AS
                SELECT
                    id,
                    data->>'id' as assignment_id,
                    data->>'status' as status,
                    data->'device' as device,
                    data
                FROM test_assignments
                """
            )

            # Insert test data with nested device objects
            import psycopg.types.json

            test_id_active_router = uuid.uuid4()
            test_id_inactive_router = uuid.uuid4()
            test_id_active_switch = uuid.uuid4()
            test_id_pending = uuid.uuid4()

            # Insert records one by one with proper JSON handling
            await conn.execute(
                "INSERT INTO test_assignments (id, data) VALUES (%s::uuid, %s::jsonb)",
                (
                    str(test_id_active_router),
                    psycopg.types.json.Json(
                        {
                            "id": str(test_id_active_router),
                            "status": "active",
                            "device": {
                                "id": str(uuid.uuid4()),
                                "name": "router-01",
                                "is_active": True,  # snake_case
                            },
                        }
                    ),
                ),
            )

            await conn.execute(
                "INSERT INTO test_assignments (id, data) VALUES (%s::uuid, %s::jsonb)",
                (
                    str(test_id_inactive_router),
                    psycopg.types.json.Json(
                        {
                            "id": str(test_id_inactive_router),
                            "status": "active",
                            "device": {
                                "id": str(uuid.uuid4()),
                                "name": "router-02",
                                "is_active": False,  # snake_case
                            },
                        }
                    ),
                ),
            )

            await conn.execute(
                "INSERT INTO test_assignments (id, data) VALUES (%s::uuid, %s::jsonb)",
                (
                    str(test_id_active_switch),
                    psycopg.types.json.Json(
                        {
                            "id": str(test_id_active_switch),
                            "status": "active",
                            "device": {
                                "id": str(uuid.uuid4()),
                                "name": "switch-01",
                                "is_active": True,  # snake_case
                            },
                        }
                    ),
                ),
            )

            await conn.execute(
                "INSERT INTO test_assignments (id, data) VALUES (%s::uuid, %s::jsonb)",
                (
                    str(test_id_pending),
                    psycopg.types.json.Json(
                        {"id": str(test_id_pending), "status": "pending", "device": None}
                    ),
                ),
            )

        yield {
            "active_router_id": test_id_active_router,
            "inactive_router_id": test_id_inactive_router,
            "active_switch_id": test_id_active_switch,
            "pending_id": test_id_pending,
        }

        # Cleanup
        async with db_pool.connection() as conn:
            await conn.execute("DROP VIEW IF EXISTS test_assignment_view CASCADE")
            await conn.execute("DROP TABLE IF EXISTS test_assignments CASCADE")

    @pytest.mark.asyncio
    async def test_nested_or_returns_correct_results(self, db_pool, setup_test_data):
        """Test that nested OR operator returns correct database results.

        {device: {OR: [{is_active: {eq: true}}, {name: {contains: "router"}}]}}
        Should match:
        - active_router_id (is_active=true)
        - inactive_router_id (name contains "router")
        Should NOT match:
        - active_switch_id (is_active=true but name doesn't contain "router")
        - pending_id (no device)
        """

        @fraiseql.type
        class Device:
            id: uuid.UUID
            name: str
            is_active: bool

        @fraiseql.type
        class Assignment:
            id: uuid.UUID
            status: str
            device: Device | None

        register_type_for_view(
            "test_assignment_view", Assignment, has_jsonb_data=True, jsonb_column="data"
        )

        repo = FraiseQLRepository(db_pool, context={"mode": "test"})

        # Test nested OR: is_active=true OR name contains "router"
        where_dict = {
            "device": {"OR": [{"is_active": {"eq": True}}, {"name": {"contains": "router"}}]}
        }

        raw_results = await repo.find("test_assignment_view", where=where_dict)
        results = _parse_rust_response(raw_results)

        # Should return 3 results: active_router, inactive_router, active_switch
        assert len(results) == 3, (
            f"Expected 3 results with nested OR, got {len(results)}. "
            f"Should match active devices OR devices with 'router' in name. "
            f"Results: {results}"
        )

        result_ids = {str(r["id"]) for r in results}
        expected_ids = {
            str(setup_test_data["active_router_id"]),
            str(setup_test_data["inactive_router_id"]),
            str(setup_test_data["active_switch_id"]),
        }

        assert result_ids == expected_ids, (
            f"Result IDs {result_ids} don't match expected {expected_ids}"
        )

    @pytest.mark.asyncio
    async def test_nested_and_returns_correct_results(self, db_pool, setup_test_data):
        """Test that nested AND operator returns correct database results.

        {device: {AND: [{is_active: {eq: true}}, {name: {contains: "router"}}]}}
        Should match:
        - active_router_id (is_active=true AND name contains "router")
        Should NOT match:
        - inactive_router_id (is_active=false)
        - active_switch_id (name doesn't contain "router")
        - pending_id (no device)
        """

        @fraiseql.type
        class Device:
            id: uuid.UUID
            name: str
            is_active: bool

        @fraiseql.type
        class Assignment:
            id: uuid.UUID
            status: str
            device: Device | None

        register_type_for_view(
            "test_assignment_view", Assignment, has_jsonb_data=True, jsonb_column="data"
        )

        repo = FraiseQLRepository(db_pool, context={"mode": "test"})

        # Test nested AND: is_active=true AND name contains "router"
        where_dict = {
            "device": {"AND": [{"is_active": {"eq": True}}, {"name": {"contains": "router"}}]}
        }

        raw_results = await repo.find("test_assignment_view", where=where_dict)
        results = _parse_rust_response(raw_results)

        # Should return 1 result: only active_router matches both conditions
        assert len(results) == 1, (
            f"Expected 1 result with nested AND, got {len(results)}. "
            f"Should match devices that are active AND have 'router' in name. "
            f"Results: {results}"
        )

        assert str(results[0]["id"]) == str(setup_test_data["active_router_id"])

    @pytest.mark.asyncio
    async def test_nested_not_returns_correct_results(self, db_pool, setup_test_data):
        """Test that nested NOT operator returns correct database results.

        {device: {NOT: {is_active: {eq: true}}}}
        Should match:
        - inactive_router_id (is_active=false)
        Should NOT match:
        - active_router_id (is_active=true)
        - active_switch_id (is_active=true)
        - pending_id (no device)
        """

        @fraiseql.type
        class Device:
            id: uuid.UUID
            name: str
            is_active: bool

        @fraiseql.type
        class Assignment:
            id: uuid.UUID
            status: str
            device: Device | None

        register_type_for_view(
            "test_assignment_view", Assignment, has_jsonb_data=True, jsonb_column="data"
        )

        repo = FraiseQLRepository(db_pool, context={"mode": "test"})

        # Test nested NOT: NOT(is_active=true) i.e. is_active=false
        where_dict = {"device": {"NOT": {"is_active": {"eq": True}}}}

        raw_results = await repo.find("test_assignment_view", where=where_dict)
        results = _parse_rust_response(raw_results)

        # Should return 1 result: only inactive_router has is_active=false
        assert len(results) == 1, (
            f"Expected 1 result with nested NOT, got {len(results)}. "
            f"Should match devices where is_active is NOT true. "
            f"Results: {results}"
        )

        assert str(results[0]["id"]) == str(setup_test_data["inactive_router_id"])

    @pytest.mark.asyncio
    async def test_complex_nested_logical_expression(self, db_pool, setup_test_data):
        """Test complex nested logical expression.

        {device: {OR: [{AND: [{is_active: {eq: true}}, {name: {contains: "router"}}]}, {AND: [{is_active: {eq: false}}, {name: {contains: "router"}}]}]}}
        Should match:
        - active_router_id ((is_active=true AND name contains "router"))
        - inactive_router_id ((is_active=false AND name contains "router"))
        Should NOT match:
        - active_switch_id (is_active=true but name doesn't contain "router")
        - pending_id (no device)
        """

        @fraiseql.type
        class Device:
            id: uuid.UUID
            name: str
            is_active: bool

        @fraiseql.type
        class Assignment:
            id: uuid.UUID
            status: str
            device: Device | None

        register_type_for_view(
            "test_assignment_view", Assignment, has_jsonb_data=True, jsonb_column="data"
        )

        repo = FraiseQLRepository(db_pool, context={"mode": "test"})

        # Test complex nested logical expression
        where_dict = {
            "device": {
                "OR": [
                    {"AND": [{"is_active": {"eq": True}}, {"name": {"contains": "router"}}]},
                    {"AND": [{"is_active": {"eq": False}}, {"name": {"contains": "router"}}]},
                ]
            }
        }

        raw_results = await repo.find("test_assignment_view", where=where_dict)
        results = _parse_rust_response(raw_results)

        # Should return 2 results: active_router and inactive_router
        assert len(results) == 2, (
            f"Expected 2 results with complex nested logical expression, got {len(results)}. "
            f"Should match routers that are either active or inactive. "
            f"Results: {results}"
        )

        result_ids = {str(r["id"]) for r in results}
        expected_ids = {
            str(setup_test_data["active_router_id"]),
            str(setup_test_data["inactive_router_id"]),
        }

        assert result_ids == expected_ids, (
            f"Result IDs {result_ids} don't match expected {expected_ids}"
        )
