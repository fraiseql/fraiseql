"""Test for dict WHERE filter bug with mixed nested and direct filters.

This test reproduces Issue #117: When using dict-based WHERE filters (not GraphQL
where types) with a mix of nested object filters (e.g., {machine: {id: {eq: value}}})
and direct field filters (e.g., {is_current: {eq: true}}), the second filter is
incorrectly skipped due to variable scoping bug in _convert_dict_where_to_sql().

Root cause: is_nested_object flag is declared outside the field iteration loop,
causing it to carry state between iterations.
"""

import json
from datetime import UTC, datetime
from typing import Optional
from uuid import UUID, uuid4

import pytest

pytestmark = pytest.mark.database

from tests.fixtures.database.database_conftest import *  # noqa: F403

import fraiseql
from fraiseql.core.rust_pipeline import RustResponseBytes
from fraiseql.db import FraiseQLRepository, register_type_for_view


# Test types
@fraiseql.type
class Machine:
    id: UUID
    name: str


@fraiseql.type
class RouterConfig:
    id: UUID
    machine_id: UUID
    config_name: str
    is_current: bool
    created_at: datetime
    machine: Optional[Machine] = None


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
            # The Rust pipeline returns a single object when there's 1 result
            # but tests expect a list
            if isinstance(data, dict):
                return [data]
            return data
        return response_json
    return result


class TestDictWhereMixedFiltersBug:
    """Test suite to reproduce and fix the dict WHERE mixed filters bug."""

    @pytest.fixture
    async def setup_test_tables(self, db_pool):
        """Create test tables for machines and router configs."""
        # Register types for views
        register_type_for_view("test_machine_view", Machine)
        register_type_for_view("test_router_config_view", RouterConfig)

        async with db_pool.connection() as conn:
            # Create tables
            await conn.execute(
                """
                CREATE TABLE IF NOT EXISTS test_machines (
                    id UUID PRIMARY KEY,
                    name TEXT NOT NULL
                )
            """
            )

            await conn.execute(
                """
                CREATE TABLE IF NOT EXISTS test_router_configs (
                    id UUID PRIMARY KEY,
                    machine_id UUID NOT NULL REFERENCES test_machines(id),
                    config_name TEXT NOT NULL,
                    is_current BOOLEAN NOT NULL,
                    created_at TIMESTAMP WITH TIME ZONE NOT NULL
                )
            """
            )

            # Create views with JSONB data column
            await conn.execute(
                """
                CREATE OR REPLACE VIEW test_machine_view AS
                SELECT
                    id, name,
                    jsonb_build_object(
                        'id', id,
                        'name', name
                    ) as data
                FROM test_machines
            """
            )

            await conn.execute(
                """
                CREATE OR REPLACE VIEW test_router_config_view AS
                SELECT
                    rc.id,
                    rc.machine_id,
                    rc.config_name,
                    rc.is_current,
                    rc.created_at,
                    jsonb_build_object(
                        'id', rc.id,
                        'machine_id', rc.machine_id,
                        'config_name', rc.config_name,
                        'is_current', rc.is_current,
                        'created_at', rc.created_at,
                        'machine', jsonb_build_object(
                            'id', m.id,
                            'name', m.name
                        )
                    ) as data
                FROM test_router_configs rc
                LEFT JOIN test_machines m ON rc.machine_id = m.id
            """
            )

            # Insert test data
            machine_1_id = uuid4()
            machine_2_id = uuid4()

            await conn.execute(
                """
                INSERT INTO test_machines (id, name)
                VALUES
                    (%s, 'router-01'),
                    (%s, 'router-02')
            """,
                (machine_1_id, machine_2_id),
            )

            # Insert router configs for machine_1
            # - 2 configs for machine_1, only 1 is current
            # - 2 configs for machine_2, only 1 is current
            await conn.execute(
                """
                INSERT INTO test_router_configs (id, machine_id, config_name, is_current, created_at)
                VALUES
                    (%s, %s, 'config-v1', false, '2024-01-01 10:00:00+00'),
                    (%s, %s, 'config-v2', true, '2024-01-02 10:00:00+00'),
                    (%s, %s, 'config-v1', false, '2024-01-01 10:00:00+00'),
                    (%s, %s, 'config-v2', true, '2024-01-02 10:00:00+00')
            """,
                (
                    uuid4(),
                    machine_1_id,
                    uuid4(),
                    machine_1_id,
                    uuid4(),
                    machine_2_id,
                    uuid4(),
                    machine_2_id,
                ),
            )

        yield {
            "machine_1_id": machine_1_id,
            "machine_2_id": machine_2_id,
        }

        # Cleanup
        async with db_pool.connection() as conn:
            await conn.execute("DROP VIEW IF EXISTS test_router_config_view")
            await conn.execute("DROP VIEW IF EXISTS test_machine_view")
            await conn.execute("DROP TABLE IF EXISTS test_router_configs")
            await conn.execute("DROP TABLE IF EXISTS test_machines")

    @pytest.mark.asyncio
    async def test_dict_where_with_nested_filter_only(self, db_pool, setup_test_tables):
        """Test dict WHERE with only nested object filter works correctly."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})
        machine_1_id = setup_test_tables["machine_1_id"]

        # Use dict-based WHERE filter with nested object
        where_dict = {"machine": {"id": {"eq": machine_1_id}}}

        raw_results = await repo.find("test_router_config_view", where=where_dict)
        results = _parse_rust_response(raw_results)

        # Should get both configs for machine_1
        assert len(results) == 2
        assert all(str(r["machineId"]) == str(machine_1_id) for r in results)

    @pytest.mark.asyncio
    async def test_dict_where_with_direct_filter_only(self, db_pool, setup_test_tables):
        """Test dict WHERE with only direct field filter works correctly."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Use dict-based WHERE filter with direct field
        where_dict = {"is_current": {"eq": True}}

        raw_results = await repo.find("test_router_config_view", where=where_dict)
        results = _parse_rust_response(raw_results)

        # Should get 2 current configs (1 from each machine)
        assert len(results) == 2
        assert all(r["isCurrent"] is True for r in results)

    @pytest.mark.asyncio
    async def test_dict_where_with_mixed_nested_and_direct_filters_BUG(
        self, db_pool, setup_test_tables
    ):
        """
        REPRODUCES BUG: Test dict WHERE with both nested object AND direct field filters.

        This test will FAIL due to the is_nested_object variable scoping bug.
        When fixed, it should pass by correctly applying both filters.

        The bug: is_nested_object is declared outside the loop in _convert_dict_where_to_sql(),
        causing it to carry state from the first iteration (nested filter) to the second
        iteration (direct filter), incorrectly treating the second filter as a nested object.
        """
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})
        machine_1_id = setup_test_tables["machine_1_id"]

        # Use dict-based WHERE filter with BOTH nested object AND direct field
        # This is the real-world use case: "get current config for this machine"
        where_dict = {
            "machine": {"id": {"eq": machine_1_id}},  # Nested object filter
            "is_current": {"eq": True},  # Direct field filter
        }

        raw_results = await repo.find("test_router_config_view", where=where_dict)
        results = _parse_rust_response(raw_results)

        # EXPECTED: Should get 1 config (the current config for machine_1)
        # ACTUAL (with bug): Gets 2 configs (only applies machine filter, ignores is_current)
        assert len(results) == 1, (
            f"Expected 1 result (current config for machine_1), got {len(results)}. "
            "This indicates the is_current filter was ignored due to the bug."
        )
        assert str(results[0]["machineId"]) == str(machine_1_id)
        assert results[0]["isCurrent"] is True
        assert results[0]["configName"] == "config-v2"

    @pytest.mark.asyncio
    async def test_dict_where_with_multiple_direct_filters_after_nested(
        self, db_pool, setup_test_tables
    ):
        """
        Test dict WHERE with nested filter followed by multiple direct filters.

        This is an edge case that further demonstrates the bug: when multiple
        direct filters follow a nested filter, all of them may be affected.
        """
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})
        machine_1_id = setup_test_tables["machine_1_id"]

        # Use dict-based WHERE filter with nested + multiple direct filters
        where_dict = {
            "machine": {"id": {"eq": machine_1_id}},  # Nested object filter
            "is_current": {"eq": True},  # Direct field filter 1
            "config_name": {"eq": "config-v2"},  # Direct field filter 2
        }

        raw_results = await repo.find("test_router_config_view", where=where_dict)
        results = _parse_rust_response(raw_results)

        # Should get exactly 1 config matching all criteria
        assert len(results) == 1, (
            f"Expected 1 result, got {len(results)}. "
            "Direct filters after nested filter were ignored."
        )
        assert str(results[0]["machineId"]) == str(machine_1_id)
        assert results[0]["isCurrent"] is True
        assert results[0]["configName"] == "config-v2"

    @pytest.mark.asyncio
    async def test_dict_where_with_direct_filter_before_nested(self, db_pool, setup_test_tables):
        """
        Test dict WHERE with direct filter BEFORE nested filter.

        This tests if the order matters. Due to dict iteration order (Python 3.7+),
        this should be predictable, but the bug might not manifest if direct comes first.
        """
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})
        machine_1_id = setup_test_tables["machine_1_id"]

        # Put direct filter BEFORE nested filter in dict
        # Note: In Python 3.7+, dicts maintain insertion order
        where_dict = {
            "is_current": {"eq": True},  # Direct field filter (first)
            "machine": {"id": {"eq": machine_1_id}},  # Nested object filter (second)
        }

        raw_results = await repo.find("test_router_config_view", where=where_dict)
        results = _parse_rust_response(raw_results)

        # Should get exactly 1 config
        # This might pass even with the bug, depending on iteration order
        assert len(results) == 1
        assert str(results[0]["machineId"]) == str(machine_1_id)
        assert results[0]["isCurrent"] is True
