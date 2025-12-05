"""Regression test for Issue #124: WhereInput nested filters on hybrid tables.

This test reproduces the bug where nested filtering with GraphQL WhereInput objects
like {machine: {id: {eq: $machineId}}} fails with "Unsupported operator: id" error
on hybrid tables that have both SQL columns (machine_id) and JSONB data.

The bug exists because the WhereInput code path (._to_sql_where()) bypasses the
hybrid table detection logic that exists in the dict-based filtering path.

Issue: https://github.com/fraiseql/fraiseql/issues/124
"""

import uuid
from datetime import datetime

import pytest
import pytest_asyncio
from tests.fixtures.database.database_conftest import *  # noqa: F403
from tests.unit.utils.test_response_utils import extract_graphql_data

import fraiseql
from fraiseql.db import FraiseQLRepository, register_type_for_view
from fraiseql.sql import UUIDFilter, create_graphql_where_input

pytestmark = pytest.mark.database


# Define test types
@fraiseql.type
class Machine:
    """Machine type with minimal fields for testing."""

    id: uuid.UUID
    name: str


@fraiseql.type(sql_source="tv_allocation")
class Allocation:
    """Allocation type representing a hybrid table.

    Hybrid table structure:
    - SQL columns: id, machine_id, status, tenant_id, data
    - JSONB column 'data' contains: machine object with nested id

    This matches the real-world printoptim_backend structure.
    """

    id: uuid.UUID
    machine: Machine | None  # Nested object from JSONB
    status: str | None = None
    tenant_id: uuid.UUID | None = None
    created_at: datetime | None = None


class TestWhereInputNestedFilterHybridTables:
    """Test that WhereInput nested filtering works on hybrid tables."""

    @pytest_asyncio.fixture
    async def setup_test_data(self, class_db_pool, test_schema, clear_registry_class) -> dict:
        """Create hybrid allocation table and test data."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
            # Create hybrid table with both SQL columns and JSONB data
            await conn.execute(
                """
                CREATE TABLE IF NOT EXISTS tv_allocation (
                    id UUID PRIMARY KEY,
                    machine_id UUID,  -- SQL FK column
                    status TEXT,
                    tenant_id UUID DEFAULT '11111111-1111-1111-1111-111111111111'::uuid,
                    created_at TIMESTAMP DEFAULT NOW(),
                    data JSONB  -- JSONB column with nested objects
                )
                """
            )

            # Clear existing data
            await conn.execute("DELETE FROM tv_allocation")

            # Create test machines
            machine1_id = uuid.UUID("01513100-0000-0000-0000-000000000066")
            machine2_id = uuid.UUID("02513100-0000-0000-0000-000000000077")
            machine3_id = uuid.UUID("03513100-0000-0000-0000-000000000088")

            # Insert test allocations
            # Machine 1: 0 allocations (for testing zero results)
            # Machine 2: 2 allocations (for testing filtering works)
            # Machine 3: 1 allocation (additional data)

            import json

            allocations = [
                # 2 allocations for machine2
                {
                    "id": uuid.uuid4(),
                    "machine_id": machine2_id,
                    "status": "active",
                    "data": {
                        "id": str(uuid.uuid4()),
                        "machine": {"id": str(machine2_id), "name": "Printer 2"},
                        "status": "active",
                    },
                },
                {
                    "id": uuid.uuid4(),
                    "machine_id": machine2_id,
                    "status": "active",
                    "data": {
                        "id": str(uuid.uuid4()),
                        "machine": {"id": str(machine2_id), "name": "Printer 2"},
                        "status": "active",
                    },
                },
                # 1 allocation for machine3
                {
                    "id": uuid.uuid4(),
                    "machine_id": machine3_id,
                    "status": "pending",
                    "data": {
                        "id": str(uuid.uuid4()),
                        "machine": {"id": str(machine3_id), "name": "Printer 3"},
                        "status": "pending",
                    },
                },
                # 1 allocation with no machine (NULL)
                {
                    "id": uuid.uuid4(),
                    "machine_id": None,
                    "status": "pending",
                    "data": {
                        "id": str(uuid.uuid4()),
                        "machine": None,
                        "status": "pending",
                    },
                },
            ]

            async with conn.cursor() as cursor:
                for alloc in allocations:
                    await cursor.execute(
                        """
                        INSERT INTO tv_allocation (id, machine_id, status, data)
                        VALUES (%s, %s, %s, %s::jsonb)
                        """,
                        (
                            alloc["id"],
                            alloc["machine_id"],
                            alloc["status"],
                            json.dumps(alloc["data"]),
                        ),
                    )
            await conn.commit()

            # Verify data setup
            async with conn.cursor() as cursor:
                await cursor.execute(
                    "SELECT COUNT(*) FROM tv_allocation WHERE machine_id = %s",
                    (machine1_id,),
                )
                machine1_count = (await cursor.fetchone())[0]

                await cursor.execute(
                    "SELECT COUNT(*) FROM tv_allocation WHERE machine_id = %s",
                    (machine2_id,),
                )
                machine2_count = (await cursor.fetchone())[0]

                await cursor.execute(
                    "SELECT COUNT(*) FROM tv_allocation WHERE machine_id = %s",
                    (machine3_id,),
                )
                machine3_count = (await cursor.fetchone())[0]

                await cursor.execute("SELECT COUNT(*) FROM tv_allocation")
                total_count = (await cursor.fetchone())[0]

                return {
                    "machine1_id": machine1_id,
                    "machine2_id": machine2_id,
                    "machine3_id": machine3_id,
                    "machine1_count": machine1_count,  # 0
                    "machine2_count": machine2_count,  # 2
                    "machine3_count": machine3_count,  # 1
                    "total_count": total_count,  # 4
                }

    @pytest.mark.asyncio
    async def test_whereinput_nested_filter_returns_zero_results(
        self, class_db_pool, setup_test_data
    ) -> None:
        """Test WhereInput nested filter for machine with 0 allocations.

        This is the exact scenario from issue #124 where the filter should
        return 0 results but instead returns all allocations due to the
        "Unsupported operator: id" error.
        """
        test_data = setup_test_data

        # Register hybrid table with explicit column information
        register_type_for_view(
            "tv_allocation",
            Allocation,
            table_columns={"id", "machine_id", "status", "tenant_id", "created_at", "data"},
            has_jsonb_data=True,
        )

        repo = FraiseQLRepository(class_db_pool, context={"tenant_id": "test"})

        # Create WhereInput filter - this is what GraphQL generates
        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # Filter for machine1 which has 0 allocations
        where = AllocationWhereInput(
            machine=MachineWhereInput(id=UUIDFilter(eq=test_data["machine1_id"]))
        )

        # Execute query
        result = await repo.find("tv_allocation", where=where)
        results = extract_graphql_data(result, "tv_allocation")

        # EXPECTED: 0 allocations
        # BUG (before fix): Returns all allocations because filter is ignored
        assert len(results) == test_data["machine1_count"], (
            f"Expected {test_data['machine1_count']} allocations for machine1, "
            f"but got {len(results)}. "
            f"WhereInput nested filter is not working correctly on hybrid tables."
        )

    @pytest.mark.asyncio
    async def test_whereinput_nested_filter_returns_correct_results(
        self, class_db_pool, setup_test_data
    ) -> None:
        """Test WhereInput nested filter for machine with 2 allocations.

        Ensures the filter correctly returns only matching allocations.
        """
        test_data = setup_test_data

        register_type_for_view(
            "tv_allocation",
            Allocation,
            table_columns={"id", "machine_id", "status", "tenant_id", "created_at", "data"},
            has_jsonb_data=True,
        )

        repo = FraiseQLRepository(class_db_pool, context={"tenant_id": "test"})

        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # Filter for machine2 which has 2 allocations
        where = AllocationWhereInput(
            machine=MachineWhereInput(id=UUIDFilter(eq=test_data["machine2_id"]))
        )

        result = await repo.find("tv_allocation", where=where)
        results = extract_graphql_data(result, "tv_allocation")

        # Should return exactly 2 allocations
        assert len(results) == test_data["machine2_count"], (
            f"Expected {test_data['machine2_count']} allocations for machine2, "
            f"but got {len(results)}"
        )

        # Verify all returned allocations are for the correct machine
        for alloc in results:
            machine = alloc.get("machine")
            assert machine is not None, "Machine should not be None"
            assert str(machine.get("id")).lower() == str(test_data["machine2_id"]).lower(), (
                f"Allocation {alloc['id']} has wrong machine: {machine.get('id')}"
            )

    @pytest.mark.asyncio
    async def test_whereinput_vs_dict_filter_equivalence(
        self, class_db_pool, setup_test_data
    ) -> None:
        """Test that WhereInput and dict filters produce identical results.

        Dict-based filtering works correctly. This test ensures WhereInput
        produces the same results.
        """
        test_data = setup_test_data

        register_type_for_view(
            "tv_allocation",
            Allocation,
            table_columns={"id", "machine_id", "status", "tenant_id", "created_at", "data"},
            has_jsonb_data=True,
        )

        repo = FraiseQLRepository(class_db_pool, context={"tenant_id": "test"})

        # Query 1: Using WhereInput (GraphQL way)
        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        where_input = AllocationWhereInput(
            machine=MachineWhereInput(id=UUIDFilter(eq=test_data["machine2_id"]))
        )

        result_whereinput = await repo.find("tv_allocation", where=where_input)
        results_whereinput = extract_graphql_data(result_whereinput, "tv_allocation")

        # Query 2: Using dict (direct way - known to work)
        where_dict = {"machine": {"id": {"eq": test_data["machine2_id"]}}}

        result_dict = await repo.find("tv_allocation", where=where_dict)
        results_dict = extract_graphql_data(result_dict, "tv_allocation")

        # Both should return identical results
        assert len(results_whereinput) == len(results_dict), (
            f"WhereInput returned {len(results_whereinput)} results, "
            f"but dict returned {len(results_dict)} results. "
            f"They should be identical!"
        )

        assert len(results_whereinput) == test_data["machine2_count"], (
            f"Both methods should return {test_data['machine2_count']} results"
        )

    @pytest.mark.asyncio
    async def test_whereinput_uses_sql_column_not_jsonb(
        self, class_db_pool, setup_test_data
    ) -> None:
        """Test that WhereInput uses the SQL column (machine_id) not JSONB path.

        This is a performance test - SQL column access should be used for
        efficiency, not JSONB path traversal.
        """
        test_data = setup_test_data

        register_type_for_view(
            "tv_allocation",
            Allocation,
            table_columns={"id", "machine_id", "status", "tenant_id", "created_at", "data"},
            has_jsonb_data=True,
        )

        repo = FraiseQLRepository(class_db_pool, context={"tenant_id": "test"})

        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        where = AllocationWhereInput(
            machine=MachineWhereInput(id=UUIDFilter(eq=test_data["machine2_id"]))
        )

        # Execute and verify results
        result = await repo.find("tv_allocation", where=where)
        results = extract_graphql_data(result, "tv_allocation")

        # If this fails, the SQL column is not being used correctly
        assert len(results) == test_data["machine2_count"], (
            "Filter should use SQL column machine_id, not JSONB path"
        )

        # Additional verification: ensure no "Unsupported operator" warnings
        # This would be checked in test output/logs
