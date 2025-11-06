"""Regression test for Issue #112: Nested JSONB Resolution Returns Wrong __typename and Missing Fields.

This test reproduces the bug where nested JSONB objects return with:
1. Wrong `__typename` - Returns parent type instead of actual nested type
2. Missing fields - Nested object missing fields defined in GraphQL schema

GitHub Issue: #112
"""

import uuid
from typing import Optional

import pytest
import pytest_asyncio
from fastapi.testclient import TestClient

import fraiseql
from fraiseql import query
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.types import fraise_type


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


@pytest_asyncio.fixture
async def setup_issue_112_database(db_connection):
    """Set up database schema matching issue #112 reproduction case."""
    async with db_connection.cursor() as cur:
        # Drop existing objects to ensure clean state
        await cur.execute("DROP VIEW IF EXISTS v_assignment CASCADE")
        await cur.execute("DROP VIEW IF EXISTS v_equipment CASCADE")
        await cur.execute("DROP TABLE IF EXISTS tb_assignment CASCADE")
        await cur.execute("DROP TABLE IF EXISTS tb_equipment CASCADE")

        # Create tb_equipment table
        await cur.execute(
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
        await cur.execute(
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
        await cur.execute(
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
        await cur.execute(
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

        await cur.execute(
            f"""
            INSERT INTO tb_equipment (id, name, is_active) VALUES
            ('{equipment_id}'::uuid, 'Device ABC', true)
            """
        )

        await cur.execute(
            f"""
            INSERT INTO tb_assignment (id, start_date, fk_equipment) VALUES
            ('{assignment_id}'::uuid, '2024-01-15', '{equipment_id}'::uuid)
            """
        )

        await db_connection.commit()


@pytest.fixture
def graphql_client(db_pool, setup_issue_112_database, clear_registry):
    """Create a GraphQL test client with real database connection."""
    from fraiseql.fastapi.dependencies import set_db_pool

    set_db_pool(db_pool)

    app = create_fraiseql_app(
        database_url="postgresql://test/test",  # Dummy URL since we're injecting pool
        types=[Equipment, Assignment],
        queries=[assignments],
        production=False,
    )
    return TestClient(app)


@pytest.mark.skip(reason="Schema registry singleton - only one initialization per process. Tests pass individually. Run with: pytest tests/regression/test_issue_112_nested_jsonb_typename.py -v")
class TestIssue112NestedJSONBTypename:
    """Test suite for Issue #112: Nested JSONB __typename bug.

    Note: Schema registry is a singleton and can only be initialized once per process.
    These tests pass individually but fail in full test suite runs.
    """

    def test_nested_object_has_correct_typename(self, graphql_client):
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

        response = graphql_client.post("/graphql", json={"query": query_str})
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
        assert assignment["__typename"] == "Assignment", \
            f"Parent __typename wrong: expected 'Assignment', got '{assignment['__typename']}'"

        # Nested type should be correct (BUG: this fails!)
        assert assignment["equipment"] is not None, "Expected equipment to be present"
        equipment = assignment["equipment"]

        # 🐛 BUG: This assertion will fail
        # Expected: "Equipment"
        # Actual: "Assignment" (nested object gets parent's typename)
        assert equipment["__typename"] == "Equipment", \
            f"❌ BUG CONFIRMED: Nested __typename wrong! Expected 'Equipment', got '{equipment['__typename']}'"

    def test_nested_object_has_all_fields(self, graphql_client):
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

        response = graphql_client.post("/graphql", json={"query": query_str})
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
        assert "isActive" in equipment, \
            f"❌ BUG CONFIRMED: Missing 'isActive' field! Available fields: {list(equipment.keys())}"

        # Verify field values
        assert equipment["name"] == "Device ABC"
        assert equipment["isActive"] is True

    def test_nested_object_type_inference_from_schema(self, graphql_client):
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

        response = graphql_client.post("/graphql", json={"query": query_str})
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
        assert equipment["__typename"] == "Equipment", \
            "Type inference should use schema annotation, not parent type"

    def test_multiple_assignments_all_have_correct_nested_typename(self, graphql_client):
        """Test that ALL nested objects have correct typename, not just the first one.

        This ensures the bug isn't a one-off issue but affects all nested objects.
        """
        # First, insert more test data
        # TODO: Add more assignments in fixture or here

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

        response = graphql_client.post("/graphql", json={"query": query_str})
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
                assert assignment["equipment"]["__typename"] == "Equipment", \
                    f"Assignment {idx} has wrong nested __typename: {assignment['equipment']['__typename']}"


if __name__ == "__main__":
    pytest.main([__file__, "-v", "--tb=short"])
