"""Integration test for nested object filtering in GraphQL where inputs."""

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


class TestNestedObjectFilterIntegration:
    """Test nested object filtering works end-to-end."""

    def test_nested_filter_conversion_to_sql(self):
        """Test that nested filters are properly converted to SQL where conditions."""
        # Create where input types
        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # Create a nested filter
        test_machine_id = uuid.uuid4()
        where_input = AllocationWhereInput(
            machine=MachineWhereInput(
                id=UUIDFilter(eq=test_machine_id),
                is_current=BooleanFilter(eq=True),
                name=StringFilter(contains="Server"),
            ),
            status=StringFilter(eq="active"),
        )

        # Convert to SQL where type
        sql_where = where_input._to_sql_where()

        # Verify the conversion worked
        assert hasattr(sql_where, "machine")
        assert hasattr(sql_where, "status")

        # The machine field should contain the nested where conditions
        assert sql_where.machine is not None
        assert sql_where.status == {"eq": "active"}

        # Generate SQL and validate its correctness
        sql = sql_where.to_sql()
        assert sql is not None

        # To properly check the generated SQL, we need to examine the SQL components
        # Check that the nested path is correctly constructed as SQL("data -> 'machine'")
        sql_str = str(sql)

        # The SQL object should contain the nested path for machine fields
        # Looking for SQL("data -> 'machine'") in the representation
        assert "SQL(\"data -> 'machine'\")" in sql_str, (
            f"Expected nested JSONB path for machine fields, but got: {sql_str}"
        )

        # Root level status filter should just use 'data'
        # Count occurrences - should have both nested and root level paths
        assert sql_str.count("SQL(\"data -> 'machine'\")") == 3, (
            f"Expected 3 nested machine paths (for id, name, is_current), but got: {sql_str}"
        )

        assert "SQL('data')" in sql_str, (
            f"Expected root-level data access for status field, but got: {sql_str}"
        )

    def test_nested_filter_with_none_values(self):
        """Test that None values in nested filters are handled correctly."""
        create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # Test with None machine filter
        where_input = AllocationWhereInput(
            id=UUIDFilter(eq=uuid.uuid4()), machine=None, status=StringFilter(eq="pending")
        )

        sql_where = where_input._to_sql_where()
        assert sql_where.machine == {}  # No filter on machine
        assert sql_where.status == {"eq": "pending"}

    def test_deeply_nested_filtering(self):
        """Test multiple levels of nested filtering."""

        @fraiseql.type
        class Location:
            id: uuid.UUID
            city: str
            country: str

        @fraiseql.type
        class MachineWithLocation:
            id: uuid.UUID
            name: str
            location: Location | None

        @fraiseql.type
        class AllocationDeep:
            id: uuid.UUID
            machine: MachineWithLocation | None

        # Create where inputs
        LocationWhereInput = create_graphql_where_input(Location)
        MachineWithLocationWhereInput = create_graphql_where_input(MachineWithLocation)
        AllocationDeepWhereInput = create_graphql_where_input(AllocationDeep)

        # Create deeply nested filter
        where_input = AllocationDeepWhereInput(
            machine=MachineWithLocationWhereInput(
                name=StringFilter(startswith="VM"),
                location=LocationWhereInput(
                    city=StringFilter(eq="Seattle"), country=StringFilter(eq="USA")
                ),
            )
        )

        # Convert and verify
        sql_where = where_input._to_sql_where()
        assert hasattr(sql_where, "machine")
        assert sql_where.machine is not None

        # Generate SQL and verify deep nesting paths
        sql = sql_where.to_sql()
        assert sql is not None
        sql_str = str(sql)

        # Check that deeply nested paths are correctly generated
        # Machine name should be at: data -> 'machine' ->> 'name'
        assert "SQL(\"data -> 'machine'\")" in sql_str, (
            f"Expected nested path for machine.name, but got: {sql_str}"
        )

        # Location city should be at: data -> 'machine' -> 'location' ->> 'city'
        assert "SQL(\"data -> 'machine' -> 'location'\")" in sql_str, (
            f"Expected deeply nested path for machine.location.city, but got: {sql_str}"
        )

    def test_mixed_scalar_and_nested_filters(self):
        """Test mixing scalar and nested object filters."""
        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # Mix scalar and nested filters
        test_id = uuid.uuid4()
        where_input = AllocationWhereInput(
            id=UUIDFilter(eq=test_id),
            status=StringFilter(in_=["active", "pending"]),
            machine=MachineWhereInput(
                is_current=BooleanFilter(eq=True), name=StringFilter(neq="deprecated")
            ),
        )

        sql_where = where_input._to_sql_where()

        # Verify all filters are present
        assert sql_where.id == {"eq": test_id}
        assert sql_where.status == {"in": ["active", "pending"]}
        assert sql_where.machine is not None

    def test_empty_nested_filter(self):
        """Test that empty nested filters are handled correctly."""
        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # Create filter with empty nested filter
        where_input = AllocationWhereInput(
            status=StringFilter(eq="active"),
            machine=MachineWhereInput(),  # Empty filter
        )

        sql_where = where_input._to_sql_where()

        # Empty nested filter should create a nested where object with empty fields
        assert sql_where.status == {"eq": "active"}
        assert sql_where.machine is not None
        # The nested where object should have empty operator dicts
        assert hasattr(sql_where.machine, "id")
        assert sql_where.machine.id == {}
        assert sql_where.machine.name == {}
        assert sql_where.machine.is_current == {}

    def test_nested_camelcase_to_snake_case_conversion_in_sql(self):
        """Test that camelCase field names are converted to snake_case in nested JSONB paths.

        This is a regression test for Issue #111:
        When filtering on nested object fields, FraiseQL must convert camelCase GraphQL
        field names (e.g., 'isActive', 'isCurrent') to snake_case database field names
        (e.g., 'is_active', 'is_current') in JSONB path queries.

        Expected SQL: data->'machine'->>'is_current' = 'true'
        NOT: data->'machine'->>'isCurrent' = 'true'
        """
        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # Create nested filter using camelCase field (as it would come from GraphQL)
        where_input = AllocationWhereInput(
            machine=MachineWhereInput(
                is_current=BooleanFilter(eq=True),  # Python uses snake_case
            )
        )

        # Convert to SQL
        sql_where = where_input._to_sql_where()
        sql = sql_where.to_sql()

        # Get the actual SQL string by rendering it
        from psycopg import sql as psycopg_sql

        # Use as_string with a mock connection to render the SQL
        try:
            sql_str = sql.as_string(None)
        except (AttributeError, TypeError):
            # Fallback to string representation if as_string doesn't work
            sql_str = str(sql)

        # CRITICAL: Verify that the JSONB path uses snake_case 'is_current'
        # NOT camelCase 'isCurrent'
        assert "'is_current'" in sql_str, (
            f"BUG: Nested JSONB path is not using snake_case field name 'is_current'. "
            f"Generated SQL: {sql_str}. "
            f"This means the filter will query data->'machine'->>'isCurrent' which "
            f"returns NULL instead of data->'machine'->>'is_current' which returns the value."
        )

        # Ensure we're NOT using camelCase
        assert "'isCurrent'" not in sql_str, (
            f"BUG: Nested JSONB path is using camelCase 'isCurrent' instead of snake_case. "
            f"Generated SQL: {sql_str}"
        )

    def test_nested_multiple_camelcase_fields_conversion(self):
        """Test that multiple camelCase fields in nested objects are all converted to snake_case.

        Regression test for Issue #111 with multiple nested fields.
        """

        # Create a device type with multiple camelCase fields
        @fraiseql.type
        class Device:
            id: uuid.UUID
            device_name: str
            is_active: bool = False
            last_seen: datetime

        @fraiseql.type
        class Assignment:
            id: uuid.UUID
            device: Device | None
            status: str

        DeviceWhereInput = create_graphql_where_input(Device)
        AssignmentWhereInput = create_graphql_where_input(Assignment)

        # Filter on multiple nested fields with underscores
        where_input = AssignmentWhereInput(
            device=DeviceWhereInput(
                is_active=BooleanFilter(eq=True),
                device_name=StringFilter(contains="router"),
            )
        )

        sql_where = where_input._to_sql_where()
        sql = sql_where.to_sql()

        try:
            sql_str = sql.as_string(None)
        except (AttributeError, TypeError):
            sql_str = str(sql)

        # Verify all fields use snake_case in JSONB paths
        assert "'is_active'" in sql_str, (
            f"Field 'is_active' not using snake_case in nested path. SQL: {sql_str}"
        )
        assert "'device_name'" in sql_str, (
            f"Field 'device_name' not using snake_case in nested path. SQL: {sql_str}"
        )

        # Ensure we're not using camelCase versions
        assert "'isActive'" not in sql_str
        assert "'deviceName'" not in sql_str


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
class TestNestedObjectFilterDatabase:
    """End-to-end database integration tests for nested object filtering.

    These tests validate that nested object filters with camelCase fields
    correctly query the database using snake_case JSONB paths.
    """

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

            test_id_active = uuid.uuid4()
            test_id_inactive = uuid.uuid4()
            test_id_no_device = uuid.uuid4()

            # Insert records one by one with proper JSON handling
            await conn.execute(
                "INSERT INTO test_assignments (id, data) VALUES (%s::uuid, %s::jsonb)",
                (
                    str(test_id_active),
                    psycopg.types.json.Json(
                        {
                            "id": str(test_id_active),
                            "status": "active",
                            "device": {
                                "id": str(uuid.uuid4()),
                                "name": "router-01",
                                "is_active": True,  # CRITICAL: Using snake_case in JSONB
                            },
                        }
                    ),
                ),
            )

            await conn.execute(
                "INSERT INTO test_assignments (id, data) VALUES (%s::uuid, %s::jsonb)",
                (
                    str(test_id_inactive),
                    psycopg.types.json.Json(
                        {
                            "id": str(test_id_inactive),
                            "status": "active",
                            "device": {
                                "id": str(uuid.uuid4()),
                                "name": "router-02",
                                "is_active": False,  # CRITICAL: Using snake_case in JSONB
                            },
                        }
                    ),
                ),
            )

            await conn.execute(
                "INSERT INTO test_assignments (id, data) VALUES (%s::uuid, %s::jsonb)",
                (
                    str(test_id_no_device),
                    psycopg.types.json.Json(
                        {"id": str(test_id_no_device), "status": "pending", "device": None}
                    ),
                ),
            )

        yield {
            "active_id": test_id_active,
            "inactive_id": test_id_inactive,
            "no_device_id": test_id_no_device,
        }

        # Cleanup
        async with db_pool.connection() as conn:
            await conn.execute("DROP VIEW IF EXISTS test_assignment_view CASCADE")
            await conn.execute("DROP TABLE IF EXISTS test_assignments CASCADE")

    @pytest.mark.asyncio
    async def test_nested_boolean_filter_returns_correct_results(self, db_pool, setup_test_data):
        """Test that filtering on nested boolean field returns correct database results.

        This is the CRITICAL end-to-end test for Issue #111.
        If the field name conversion is broken, this test will fail because the query
        will look for data->'device'->>'isActive' (which returns NULL) instead of
        data->'device'->>'is_active' (which returns the actual value).
        """

        # Define types for the test
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
            "test_assignment_view",
            Assignment,
            table_columns={"id", "assignment_id", "status", "device", "data"},
        )

        repo = FraiseQLRepository(db_pool, context={"mode": "test"})

        # Use dict-based where clause (as comes from GraphQL)
        where_dict = {
            "device": {
                "is_active": {"eq": False},  # Filter for inactive devices
            }
        }

        raw_results = await repo.find("test_assignment_view", where=where_dict)
        results = _parse_rust_response(raw_results)

        # Should return 1 result (the inactive device)
        assert len(results) == 1
        assert str(results[0]["id"]) == str(setup_test_data["inactive_id"])
        assert results[0]["device"]["isActive"] is False

    @pytest.mark.asyncio
    async def test_nested_filter_combined_with_top_level_filter(self, db_pool, setup_test_data):
        """Test combining nested object filter with top-level field filter.

        Regression test for the specific pattern mentioned in Issue #111.
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

        register_type_for_view("test_assignment_view", Assignment)

        repo = FraiseQLRepository(db_pool, context={"mode": "test"})

        # Combined filter: status = 'active' AND device.is_active = true
        where_dict = {
            "status": {"eq": "active"},
            "device": {"is_active": {"eq": True}},
        }

        raw_results = await repo.find("test_assignment_view", where=where_dict)
        results = _parse_rust_response(raw_results)

        # Should return 1 result (active assignment with active device)
        assert len(results) == 1
        assert str(results[0]["id"]) == str(setup_test_data["active_id"])
        assert results[0]["status"] == "active"
        assert results[0]["device"]["isActive"] is True

    @pytest.mark.asyncio
    async def test_nested_filter_with_camelcase_input(self, db_pool, setup_test_data):
        """Test that camelCase field names in dict-based where clauses are converted to snake_case.

        This tests the scenario where GraphQL arguments come through with camelCase field names
        (as they appear in GraphQL queries) and verifies they're correctly converted to snake_case
        for JSONB path queries.

        GraphQL query: assignments(where: {device: {isActive: {eq: true}}})
        Dict representation: {"device": {"isActive": {"eq": True}}}
        Expected SQL: data->'device'->>'is_active' = 'true'
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
            "test_assignment_view",
            Assignment,
            table_columns={"id", "assignment_id", "status", "device", "data"},
        )

        repo = FraiseQLRepository(db_pool, context={"mode": "test"})

        # IMPORTANT: Using camelCase in the dict (as might come from GraphQL)
        where_dict_camelcase = {
            "device": {
                "isActive": {"eq": True},  # camelCase!
            }
        }

        raw_results = await repo.find("test_assignment_view", where=where_dict_camelcase)
        results = _parse_rust_response(raw_results)

        # Should correctly convert isActive → is_active and filter properly
        assert len(results) == 1, (
            f"Expected 1 result with camelCase input, got {len(results)}. "
            f"This indicates camelCase field names are not being converted to snake_case "
            f"for JSONB paths. Results: {results}"
        )
        assert str(results[0]["id"]) == str(setup_test_data["active_id"])
        assert results[0]["device"]["isActive"] is True

    @pytest.mark.asyncio
    async def test_multiple_nested_fields_in_dict_where_clause(self, db_pool, setup_test_data):
        """Test that dict-based where clauses with multiple nested fields work correctly.

        This tests the scenario: {device: {is_active: {eq: true}, device_name: {contains: "router"}}}
        Both conditions should be applied with AND.
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
            "test_assignment_view",
            Assignment,
            table_columns={"id", "assignment_id", "status", "device", "data"},
        )

        repo = FraiseQLRepository(db_pool, context={"mode": "test"})

        # Test multiple nested fields in dict where clause
        # First test: should match (is_active=True AND name contains "router")
        where_dict_match = {
            "device": {
                "is_active": {"eq": True},
                "name": {"contains": "router"},  # This should match "router-01"
            }
        }

        raw_results = await repo.find("test_assignment_view", where=where_dict_match)
        results = _parse_rust_response(raw_results)

        # Should return 1 result (the active device with name containing "router")
        assert len(results) == 1, (
            f"Expected 1 result with multiple nested field filters (match case), got {len(results)}. "
            f"Results: {results}"
        )
        assert str(results[0]["id"]) == str(setup_test_data["active_id"])

        # Second test: should NOT match (is_active=True AND name contains "switch")
        # This tests that BOTH conditions are applied with AND
        where_dict_no_match = {
            "device": {
                "is_active": {"eq": True},
                "name": {"contains": "switch"},  # This should NOT match "router-01"
            }
        }

        raw_results = await repo.find("test_assignment_view", where=where_dict_no_match)
        results = _parse_rust_response(raw_results)

        # Should return 0 results (no device has is_active=True AND name containing "switch")
        assert len(results) == 0, (
            f"Expected 0 results with multiple nested field filters (no match case), got {len(results)}. "
            f"This indicates the implementation only applies the first condition, not both with AND. "
            f"Results: {results}"
        )

    @pytest.mark.asyncio
    async def test_multiple_top_level_nested_objects_in_dict_where_clause(
        self, db_pool, setup_test_data
    ):
        """Test that dict-based where clauses with multiple top-level nested objects work correctly.

        This tests the scenario: {device: {is_active: {eq: true}}, machine: {name: {contains: "server"}}}
        But since our test data doesn't have machines, we'll test with device and a top-level filter.
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
            "test_assignment_view",
            Assignment,
            table_columns={"id", "assignment_id", "status", "device", "data"},
        )

        repo = FraiseQLRepository(db_pool, context={"mode": "test"})

        # Test multiple top-level filters: nested object + scalar
        where_dict = {
            "device": {
                "is_active": {"eq": True},
            },
            "status": {"eq": "active"},
        }

        raw_results = await repo.find("test_assignment_view", where=where_dict)
        results = _parse_rust_response(raw_results)

        # Should return 1 result (active assignment with active device)
        assert len(results) == 1, (
            f"Expected 1 result with multiple top-level filters, got {len(results)}. "
            f"Results: {results}"
        )
        assert str(results[0]["id"]) == str(setup_test_data["active_id"])
        assert results[0]["status"] == "active"
        assert results[0]["device"]["isActive"] is True

    @pytest.mark.asyncio
    async def test_multiple_nested_fields_with_camelcase_in_dict_where_clause(
        self, db_pool, setup_test_data
    ):
        """Test that dict-based where clauses with multiple nested camelCase fields work correctly.

        This tests the scenario: {device: {isActive: {eq: true}, name: {contains: "router"}}}
        Both conditions should be applied with AND, and camelCase should be converted to snake_case.
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
            "test_assignment_view",
            Assignment,
            table_columns={"id", "assignment_id", "status", "device", "data"},
        )

        repo = FraiseQLRepository(db_pool, context={"mode": "test"})

        # Test multiple nested fields with camelCase in dict where clause
        where_dict = {
            "device": {
                "isActive": {"eq": True},  # camelCase should be converted to is_active
                "name": {"contains": "router"},  # already snake_case
            }
        }

        raw_results = await repo.find("test_assignment_view", where=where_dict)
        results = _parse_rust_response(raw_results)

        # Should return 1 result (the active device with name containing "router")
        assert len(results) == 1, (
            f"Expected 1 result with multiple nested camelCase field filters, got {len(results)}. "
            f"This indicates the implementation doesn't properly handle multiple nested fields with camelCase. "
            f"Results: {results}"
        )
        assert str(results[0]["id"]) == str(setup_test_data["active_id"])
        assert results[0]["device"]["isActive"] is True
        assert "router" in results[0]["device"]["name"]
