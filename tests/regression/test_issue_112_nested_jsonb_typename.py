"""Regression test for Issue #112.

Tests nested JSONB resolution returning wrong __typename and missing fields.

This test reproduces the bug where nested JSONB objects return with:
1. Wrong `__typename` - Returns parent type instead of actual nested type
2. Missing fields - Nested object missing fields defined in GraphQL schema

GitHub Issue: #112
"""

import uuid
from typing import TYPE_CHECKING, AsyncGenerator, Optional

import pytest
import pytest_asyncio

from fraiseql import query
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.types import fraise_type

if TYPE_CHECKING:
    from fastapi import FastAPI

pytestmark = pytest.mark.integration


# Note: Registry cleanup is handled by the clear_registry fixture
# which is explicitly included in graphql_app fixture dependencies


# Define GraphQL types matching issue #112
@fraise_type
class Equipment:
    """Equipment tracked in the system."""

    id: uuid.UUID
    name: str
    is_active: bool


@fraise_type
class Assignment:
    """Assignment of equipment to a location."""

    id: uuid.UUID
    start_date: str
    equipment: Optional[Equipment] = None  # Nested JSONB object


# GraphQL query resolver - uses automatic JSONB deserialization
@query
async def assignments(info, limit: int = 10) -> list[Assignment]:
    """Get list of assignments with nested equipment data.

    This resolver uses FraiseQL's automatic repository resolution
    which may trigger the __typename bug for nested JSONB objects.
    """
    repo = info.context["db"]

    # Register types for automatic resolution
    from fraiseql.db import register_type_for_view

    register_type_for_view("v_assignment", Assignment, has_jsonb_data=True, jsonb_column="data")
    register_type_for_view("v_equipment", Equipment, has_jsonb_data=True, jsonb_column="data")

    # Use repository.find() which returns RustResponseBytes
    # This lets FraiseQL automatically handle type resolution
    # where the bug might occur
    return await repo.find("v_assignment", limit=limit)


class TestIssue112NestedJSONBTypename:
    """Test suite for Issue #112: Nested JSONB __typename bug.

    This test class follows the FraiseQL test architecture:
    - Class-scoped database setup (setup_issue_112_database)
    - Schema isolation via test_<classname>_<uuid>
    - Proper search_path usage (no hardcoded schema names)
    - SchemaAwarePool wrapper for app connections

    The tests validate that nested JSONB objects get the correct __typename and all fields.
    """

    @pytest_asyncio.fixture(scope="class")
    async def setup_issue_112_database(self, class_db_pool, test_schema) -> AsyncGenerator[None]:
        """Set up database schema matching issue #112 reproduction case.

        This is a class-scoped fixture that creates tables/views once for all tests in the class.
        Uses class_db_pool directly with proper search_path setting for schema isolation.
        """
        async with class_db_pool.connection() as conn:
            # Set search_path to use test schema (critical for isolation)
            await conn.execute(f"SET search_path TO {test_schema}, public")

            # Drop existing objects to ensure clean state
            await conn.execute("DROP VIEW IF EXISTS v_assignment CASCADE")
            await conn.execute("DROP VIEW IF EXISTS v_equipment CASCADE")
            await conn.execute("DROP TABLE IF EXISTS tb_assignment CASCADE")
            await conn.execute("DROP TABLE IF EXISTS tb_equipment CASCADE")

            # Create tb_equipment table
            await conn.execute(
                """
                CREATE TABLE tb_equipment (
                    id UUID PRIMARY KEY,
                    name TEXT NOT NULL,
                    is_active BOOLEAN NOT NULL DEFAULT true,
                    created_at TIMESTAMPTZ DEFAULT NOW()
                )
                """
            )

            # Create tb_assignment table
            await conn.execute(
                """
                CREATE TABLE tb_assignment (
                    id UUID PRIMARY KEY,
                    start_date DATE NOT NULL,
                    fk_equipment UUID REFERENCES tb_equipment(id),
                    created_at TIMESTAMPTZ DEFAULT NOW()
                )
                """
            )

            # Create v_equipment view (for standalone equipment queries)
            await conn.execute(
                """
                CREATE VIEW v_equipment AS
                SELECT
                    jsonb_build_object(
                        'id', tb_equipment.id::text,
                        'name', tb_equipment.name,
                        'is_active', tb_equipment.is_active
                    ) as data
                FROM tb_equipment
                """
            )

            # Create v_assignment view with NESTED equipment JSONB
            await conn.execute(
                """
                CREATE VIEW v_assignment AS
                SELECT
                    jsonb_build_object(
                        'id', tb_assignment.id::text,
                        'start_date', tb_assignment.start_date::text,
                        'equipment', (
                            SELECT jsonb_build_object(
                                'id', tb_equipment.id::text,
                                'name', tb_equipment.name,
                                'is_active', tb_equipment.is_active
                            )
                            FROM tb_equipment
                            WHERE tb_equipment.id = tb_assignment.fk_equipment
                        )
                    ) as data
                FROM tb_assignment
                """
            )

            # Insert test data
            equipment_id = "12345678-abcd-ef12-3456-7890abcdef12"
            assignment_id = "a1b2c3d4-e5f6-7890-abcd-ef1234567890"

            await conn.execute(
                f"""
                INSERT INTO tb_equipment (id, name, is_active) VALUES
                ('{equipment_id}'::uuid, 'Device ABC', true)
                """
            )

            await conn.execute(
                f"""
                INSERT INTO tb_assignment (id, start_date, fk_equipment) VALUES
                ('{assignment_id}'::uuid, '2024-01-15', '{equipment_id}'::uuid)
                """
            )

            # Commit changes so they're visible to all tests in the class
            await conn.commit()

        # Yield control back to pytest - all setup is complete before test methods run
        yield

    @pytest_asyncio.fixture(scope="class")
    def graphql_app(
        self,
        class_db_pool,
        test_schema,
        setup_issue_112_database,
        clear_registry_class,
    ) -> "FastAPI":
        """Create a GraphQL app with real database connection.

        This is a class-scoped fixture that depends on setup_issue_112_database.
        The setup fixture's yield ensures all database work completes before this runs.
        """
        from contextlib import asynccontextmanager

        from fraiseql.fastapi.dependencies import set_db_pool

        # CRITICAL: Wrap pool to auto-set search_path for schema isolation
        class SchemaAwarePool:
            """Pool wrapper that ensures all connections use test_schema."""

            def __init__(self, pool, schema):
                self._pool = pool
                self._schema = schema

            @asynccontextmanager
            async def connection(self):
                """Get connection and automatically set search_path."""
                async with self._pool.connection() as conn:
                    await conn.execute(f"SET search_path TO {self._schema}, public")
                    yield conn

            def __getattr__(self, name):
                """Proxy all other attributes to underlying pool."""
                return getattr(self._pool, name)

        wrapped_pool = SchemaAwarePool(class_db_pool, test_schema)
        set_db_pool(wrapped_pool)

        app = create_fraiseql_app(
            database_url="postgresql://test/test",  # Dummy URL since we're injecting pool
            types=[Equipment, Assignment],
            queries=[assignments],
            production=False,
        )
        return app

    async def _execute_query(self, graphql_app, query_str: str):
        """Helper method to execute GraphQL queries with async client."""
        from asgi_lifespan import LifespanManager
        from httpx import ASGITransport, AsyncClient

        async with LifespanManager(graphql_app) as manager:
            transport = ASGITransport(app=manager.app)
            async with AsyncClient(transport=transport, base_url="http://test") as client:
                response = await client.post("/graphql", json={"query": query_str})
        return response

    @pytest.mark.asyncio
    async def test_nested_object_has_correct_typename(self, graphql_app) -> None:
        """Test that nested JSONB objects have correct __typename.

        BUG REPRODUCTION:
        - Parent Assignment should have __typename = "Assignment" ✅
        - Nested Equipment should have __typename = "Equipment" ❌ (returns "Assignment")

        This test is EXPECTED TO FAIL until the bug is fixed.
        """
        query_str = """
        query GetAssignments {
            assignments {
                id
                __typename
                startDate
                equipment {
                    id
                    name
                    isActive
                    __typename
                }
            }
        }
        """

        response = await self._execute_query(graphql_app, query_str)
        assert response.status_code == 200

        result = response.json()
        assert "data" in result, f"Expected 'data' key in response: {result}"
        assert "assignments" in result["data"], f"Expected 'assignments' field: {result['data']}"

        # Handle both list and single object responses
        # (RustResponseBytes may return different structures)
        assignments_data = result["data"]["assignments"]
        if isinstance(assignments_data, list):
            assert len(assignments_data) > 0, "Expected at least one assignment"
            assignment = assignments_data[0]
        else:
            # Single object returned instead of list
            assignment = assignments_data

        # Parent type should be correct (this works)
        assert assignment["__typename"] == "Assignment", (
            f"Parent __typename wrong: expected 'Assignment', got '{assignment['__typename']}'"
        )

        # Nested type should be correct (BUG: this fails!)
        assert assignment["equipment"] is not None, "Expected equipment to be present"
        equipment = assignment["equipment"]

        # 🐛 BUG: This assertion will fail
        # Expected: "Equipment"
        # Actual: "Assignment" (nested object gets parent's typename)
        assert equipment["__typename"] == "Equipment", (
            "❌ BUG CONFIRMED: Nested __typename wrong! "
            f"Expected 'Equipment', got '{equipment['__typename']}'"
        )

    @pytest.mark.asyncio
    async def test_nested_object_has_all_fields(self, graphql_app) -> None:
        """Test that nested JSONB objects have all fields resolved.

        BUG REPRODUCTION:
        - Nested Equipment should have: id, name, isActive
        - BUG: isActive field may be missing

        This test is EXPECTED TO FAIL until the bug is fixed.
        """
        query_str = """
        query GetAssignments {
            assignments {
                id
                startDate
                equipment {
                    id
                    name
                    isActive
                }
            }
        }
        """

        response = await self._execute_query(graphql_app, query_str)
        assert response.status_code == 200

        result = response.json()
        assert "data" in result

        # Handle both list and single object responses
        assignments_data = result["data"]["assignments"]
        if isinstance(assignments_data, list):
            assert len(assignments_data) > 0, "Expected at least one assignment"
            assignment = assignments_data[0]
        else:
            assignment = assignments_data

        equipment = assignment["equipment"]
        assert equipment is not None

        # All fields should be present
        assert "id" in equipment, "Missing 'id' field in nested equipment"
        assert "name" in equipment, "Missing 'name' field in nested equipment"

        # 🐛 BUG: This assertion may fail if isActive is missing
        assert "isActive" in equipment, (
            "❌ BUG CONFIRMED: Missing 'isActive' field! "
            f"Available fields: {list(equipment.keys())}"
        )

        # Verify field values
        assert equipment["name"] == "Device ABC"
        assert equipment["isActive"] is True

    @pytest.mark.asyncio
    async def test_nested_object_type_inference_from_schema(self, graphql_app) -> None:
        """Test that type inference works correctly for nested objects.

        The GraphQL schema defines:
        - Assignment.equipment: Equipment | None

        FraiseQL should infer the nested object type from the schema annotation,
        not from the parent object type.
        """
        query_str = """
        query GetAssignments {
            assignments {
                id
                equipment {
                    __typename
                }
            }
        }
        """

        response = await self._execute_query(graphql_app, query_str)
        assert response.status_code == 200

        result = response.json()

        # Handle both list and single object responses
        assignments_data = result["data"]["assignments"]
        if isinstance(assignments_data, list):
            assignment = assignments_data[0]
        else:
            assignment = assignments_data

        equipment = assignment["equipment"]

        # Type should be inferred from schema annotation (Assignment.equipment: Equipment)
        assert equipment["__typename"] == "Equipment", (
            "Type inference should use schema annotation, not parent type"
        )

    @pytest.mark.asyncio
    async def test_multiple_assignments_all_have_correct_nested_typename(self, graphql_app) -> None:
        """Test that ALL nested objects have correct typename, not just the first one.

        This ensures the bug isn't a one-off issue but affects all nested objects.
        """
        query_str = """
        query GetAssignments {
            assignments {
                id
                equipment {
                    __typename
                    name
                }
            }
        }
        """

        response = await self._execute_query(graphql_app, query_str)
        assert response.status_code == 200

        result = response.json()

        # Handle both list and single object responses
        assignments_data = result["data"]["assignments"]
        if isinstance(assignments_data, list):
            assignments = assignments_data
        else:
            assignments = [assignments_data]

        # Check that EVERY assignment's equipment has correct typename
        for idx, assignment in enumerate(assignments):
            if assignment["equipment"]:
                typename = assignment["equipment"]["__typename"]
                assert typename == "Equipment", (
                    f"Assignment {idx} has wrong nested __typename: {typename}"
                )


if __name__ == "__main__":
    pytest.main([__file__, "-v", "--tb=short"])
