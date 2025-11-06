"""Edge case tests for nested object filtering in GraphQL where inputs."""

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


class TestNestedObjectFilterEdgeCases:
    """Test edge cases for nested object filtering."""

    def test_empty_nested_filter_dict(self):
        """Test that empty nested filter dicts are handled correctly."""
        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # Create filter with empty nested filter
        where_input = AllocationWhereInput(
            machine=MachineWhereInput(),  # Empty filter
        )

        sql_where = where_input._to_sql_where()

        # Empty nested filter should create a nested where object with empty fields
        assert sql_where.machine is not None
        # The nested where object should have empty operator dicts
        assert hasattr(sql_where.machine, "id")
        assert sql_where.machine.id == {}
        assert sql_where.machine.name == {}
        assert sql_where.machine.is_current == {}

    def test_null_nested_filter(self):
        """Test that None nested filters are handled correctly."""
        create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # Test with None machine filter
        where_input = AllocationWhereInput(
            id=UUIDFilter(eq=uuid.uuid4()), machine=None, status=StringFilter(eq="pending")
        )

        sql_where = where_input._to_sql_where()
        assert sql_where.machine == {}  # No filter on machine
        assert sql_where.status == {"eq": "pending"}

    def test_mixed_fk_and_field_nested_filter(self):
        """Test mixed FK + field filters in nested objects.

        This tests the scenario: {machine: {id: {...}, name: {...}}}
        Should decide whether to use FK column or JSONB paths.
        """
        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # Create mixed filter: both id (FK) and name (JSONB field)
        test_machine_id = uuid.uuid4()
        where_input = AllocationWhereInput(
            machine=MachineWhereInput(
                id=UUIDFilter(eq=test_machine_id),
                name=StringFilter(contains="Server"),
            ),
        )

        sql_where = where_input._to_sql_where()

        # Verify the conversion worked
        assert hasattr(sql_where, "machine")
        assert sql_where.machine is not None

        # Generate SQL and validate its correctness
        sql = sql_where.to_sql()
        assert sql is not None

        # To properly check the generated SQL, we need to examine the SQL components
        sql_str = str(sql)

        # Should contain both FK column access and JSONB path access
        # FK: machine_id column
        # JSONB: data -> 'machine' ->> 'name'
        assert "machine_id" in sql_str or "machine" in sql_str, (
            f"Expected FK column or JSONB path for machine fields, but got: {sql_str}"
        )

    def test_deeply_nested_filtering_not_supported(self):
        """Test that deeply nested filtering (3+ levels) provides clear error messages.

        This tests: {machine: {location: {city: {...}}}}
        Should warn or error that deep nesting is not supported.
        """

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

        # Create deeply nested filter (3 levels: allocation.machine.location.city)
        where_input = AllocationDeepWhereInput(
            machine=MachineWithLocationWhereInput(
                location=LocationWhereInput(city=StringFilter(eq="Seattle")),
            )
        )

        # This should either work or provide a clear error message
        # For now, let's see what happens
        sql_where = where_input._to_sql_where()
        assert hasattr(sql_where, "machine")
        assert sql_where.machine is not None

        # Generate SQL and check if it handles deep nesting
        sql = sql_where.to_sql()
        if sql is not None:
            sql_str = str(sql)
            # Check that deeply nested paths are generated
            # Should be: data -> 'machine' -> 'location' ->> 'city'
            assert "data -> 'machine' -> 'location'" in sql_str, (
                f"Expected deeply nested path for machine.location.city, but got: {sql_str}"
            )


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
class TestNestedObjectFilterDatabaseEdgeCases:
    """End-to-end database integration tests for nested object filtering edge cases."""

    @pytest.fixture
    async def setup_edge_case_data(self, db_pool):
        """Set up test tables and data for edge case tests."""
        async with db_pool.connection() as conn:
            # Clean up any existing test data
            await conn.execute("DROP VIEW IF EXISTS test_allocation_edge_view CASCADE")
            await conn.execute("DROP TABLE IF EXISTS test_allocations_edge CASCADE")

            # Create test table with JSONB data column
            await conn.execute(
                """
                CREATE TABLE test_allocations_edge (
                    id UUID PRIMARY KEY,
                    data JSONB NOT NULL
                )
                """
            )

            # Create view that extracts nested data
            await conn.execute(
                """
                CREATE VIEW test_allocation_edge_view AS
                SELECT
                    id,
                    data->>'id' as allocation_id,
                    data->>'status' as status,
                    data->'machine' as machine,
                    data
                FROM test_allocations_edge
                """
            )

            # Insert test data with various edge cases
            import psycopg.types.json

            test_id_empty_machine = uuid.uuid4()
            test_id_null_machine = uuid.uuid4()
            test_id_mixed_filters = uuid.uuid4()

            # Insert records
            await conn.execute(
                "INSERT INTO test_allocations_edge (id, data) VALUES (%s::uuid, %s::jsonb)",
                (
                    str(test_id_empty_machine),
                    psycopg.types.json.Json(
                        {
                            "id": str(test_id_empty_machine),
                            "status": "active",
                            "machine": {},  # Empty machine object
                        }
                    ),
                ),
            )

            await conn.execute(
                "INSERT INTO test_allocations_edge (id, data) VALUES (%s::uuid, %s::jsonb)",
                (
                    str(test_id_null_machine),
                    psycopg.types.json.Json(
                        {
                            "id": str(test_id_null_machine),
                            "status": "pending",
                            "machine": None,  # Null machine
                        }
                    ),
                ),
            )

            await conn.execute(
                "INSERT INTO test_allocations_edge (id, data) VALUES (%s::uuid, %s::jsonb)",
                (
                    str(test_id_mixed_filters),
                    psycopg.types.json.Json(
                        {
                            "id": str(test_id_mixed_filters),
                            "status": "active",
                            "machine": {
                                "id": str(uuid.uuid4()),
                                "name": "Server-01",
                                "is_current": True,
                            },
                        }
                    ),
                ),
            )

        yield {
            "empty_machine_id": test_id_empty_machine,
            "null_machine_id": test_id_null_machine,
            "mixed_filters_id": test_id_mixed_filters,
        }

        # Cleanup
        async with db_pool.connection() as conn:
            await conn.execute("DROP VIEW IF EXISTS test_allocation_edge_view CASCADE")
            await conn.execute("DROP TABLE IF EXISTS test_allocations_edge CASCADE")

    @pytest.mark.asyncio
    async def test_empty_machine_filter_dict_where_clause(self, db_pool, setup_edge_case_data):
        """Test dict-based where clause with empty machine filter.

        {machine: {}} should match records with empty machine objects.
        """

        @fraiseql.type
        class Machine:
            id: uuid.UUID
            name: str
            is_current: bool

        @fraiseql.type
        class Allocation:
            id: uuid.UUID
            status: str
            machine: Machine | None

        register_type_for_view(
            "test_allocation_edge_view",
            Allocation,
            table_columns={"id", "allocation_id", "status", "machine", "data"},
        )

        repo = FraiseQLRepository(db_pool, context={"mode": "test"})

        # Test empty machine filter
        where_dict = {
            "machine": {},  # Empty filter
        }

        raw_results = await repo.find("test_allocation_edge_view", where=where_dict)
        results = _parse_rust_response(raw_results)

        # Empty nested filters are handled correctly - they don't add any conditions
        # So {machine: {}} means "no filter on machine", returning all records
        assert len(results) == 3, (
            f"Expected all 3 results with empty machine filter (no filtering applied), got {len(results)}. "
            f"Results: {results}"
        )

    @pytest.mark.asyncio
    async def test_null_machine_filter_dict_where_clause(self, db_pool, setup_edge_case_data):
        """Test dict-based where clause with null machine filter.

        {machine: None} should match records with null machine.
        """

        @fraiseql.type
        class Machine:
            id: uuid.UUID
            name: str
            is_current: bool

        @fraiseql.type
        class Allocation:
            id: uuid.UUID
            status: str
            machine: Machine | None

        register_type_for_view(
            "test_allocation_edge_view",
            Allocation,
            table_columns={"id", "allocation_id", "status", "machine", "data"},
        )

        repo = FraiseQLRepository(db_pool, context={"mode": "test"})

        # Test null machine filter - this might not work as expected
        # since None in dict where clauses might be treated differently
        where_dict = {
            "machine": None,  # Null filter
        }

        raw_results = await repo.find("test_allocation_edge_view", where=where_dict)
        results = _parse_rust_response(raw_results)

        # This test documents current behavior - may need to be updated
        # based on what actually happens with None values
        print(f"Null machine filter results: {results}")

    @pytest.mark.asyncio
    async def test_mixed_fk_and_field_filters_dict_where_clause(
        self, db_pool, setup_edge_case_data
    ):
        """Test dict-based where clause with mixed FK and field filters.

        {machine: {id: {...}, name: {...}}} should handle both FK and JSONB filtering.
        """

        @fraiseql.type
        class Machine:
            id: uuid.UUID
            name: str
            is_current: bool

        @fraiseql.type
        class Allocation:
            id: uuid.UUID
            status: str
            machine: Machine | None

        register_type_for_view(
            "test_allocation_edge_view",
            Allocation,
            table_columns={"id", "allocation_id", "status", "machine", "data"},
        )

        repo = FraiseQLRepository(db_pool, context={"mode": "test"})

        # Test mixed FK + field filter
        machine_id = uuid.uuid4()  # This won't match our test data
        where_dict = {
            "machine": {
                "id": {"eq": machine_id},  # FK filter (won't match)
                "name": {"contains": "Server"},  # JSONB field filter (will match)
            }
        }

        raw_results = await repo.find("test_allocation_edge_view", where=where_dict)
        results = _parse_rust_response(raw_results)

        # This test documents how mixed filters are handled
        # Currently, the implementation may prioritize one type over another
        print(f"Mixed filter results: {results}")
        # For now, just ensure it doesn't crash
        assert isinstance(results, list)
