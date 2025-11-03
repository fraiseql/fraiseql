"""Test for GitHub Issue #110: Rust execution engine fails with mutation return objects.

This test reproduces the exact issue where mutations that return complex objects
fail with 'missing a required argument: 'entity'' in Rust execution mode but work
correctly in Python execution mode.

Issue: https://github.com/fraiseql/fraiseql/issues/110
"""

from uuid import UUID, uuid4

import pytest
from graphql import execute, parse

import fraiseql
from fraiseql import failure, mutation, success
from fraiseql import input as input_type
from fraiseql.db import FraiseQLRepository
from fraiseql.gql.schema_builder import build_fraiseql_schema


# Define the GraphQL types matching the issue
@fraiseql.type
class Entity:
    """Entity type matching the issue's schema."""

    id: UUID
    name: str
    description: str | None = None
    active: bool = True


@input_type
class CreateEntityInput:
    """Input for creating an entity."""

    name: str
    description: str | None = None


@success
class CreateEntitySuccess:
    """Success type with entity field that fails in Rust mode."""

    status: str = "success"
    message: str = "Entity created successfully"
    entity: Entity  # ← This field fails to resolve in Rust mode


@failure
class CreateEntityError:
    """Failure type for create entity operation."""

    status: str = "error"
    message: str = ""


@mutation(function="create_entity", schema="app")
class CreateEntity:
    """Mutation that returns a complex object."""

    input: CreateEntityInput
    success: CreateEntitySuccess
    failure: CreateEntityError


@pytest.mark.database
class TestIssue110RustMutationObjectReturn:
    """Test suite for GitHub issue #110."""

    @pytest.fixture
    async def setup_database(self, db_connection_committed):
        """Set up test database schema and function."""
        conn = db_connection_committed

        # Create app schema
        await conn.execute("CREATE SCHEMA IF NOT EXISTS app")

        # Drop the type if it exists
        await conn.execute("DROP TYPE IF EXISTS app.mutation_result CASCADE")

        # Create the mutation_result type
        await conn.execute(
            """
            CREATE TYPE app.mutation_result AS (
                id UUID,
                updated_fields TEXT[],
                status TEXT,
                message TEXT,
                object_data JSONB,
                extra_metadata JSONB
            )
        """
        )

        # Create the function that returns mutation_result
        await conn.execute(
            """
            CREATE OR REPLACE FUNCTION app.create_entity(
                p_input JSONB
            ) RETURNS app.mutation_result AS $$
            DECLARE
                v_id UUID;
                v_result app.mutation_result;
                v_name TEXT;
                v_description TEXT;
            BEGIN
                -- Extract fields from input
                v_name := p_input->>'name';
                v_description := p_input->>'description';

                -- Generate a new ID
                v_id := gen_random_uuid();

                -- Build the result - exactly as described in issue #110
                v_result.id := v_id;
                v_result.updated_fields := ARRAY['name', 'description'];
                v_result.status := 'success';
                v_result.message := 'Entity created successfully';
                v_result.object_data := jsonb_build_object(
                    'id', v_id,
                    'name', v_name,
                    'description', v_description,
                    'active', true
                );
                v_result.extra_metadata := jsonb_build_object(
                    'entity', 'entity'
                );

                RETURN v_result;
            END;
            $$ LANGUAGE plpgsql;
        """
        )

        await conn.commit()
        return conn

    @pytest.fixture
    def graphql_schema(self, clear_registry):
        """Create GraphQL schema with the mutation."""

        # GraphQL requires a Query type with at least one field
        @fraiseql.type
        class QueryRoot:
            dummy: str = fraiseql.fraise_field(default="dummy")

        return build_fraiseql_schema(
            query_types=[QueryRoot],
            mutation_resolvers=[CreateEntity],
            camel_case_fields=True,
        )

    @pytest.fixture
    def mock_pool(self, setup_database):
        """Create a mock pool for testing."""

        class MockPool:
            def connection(self):
                class ConnContext:
                    async def __aenter__(self):
                        return setup_database

                    async def __aexit__(self, *args):
                        pass

                return ConnContext()

        return MockPool()

    async def test_mutation_python_mode_works(self, graphql_schema, mock_pool):
        """Test that mutation works in Python mode (control test)."""
        # Create repository with Python mode context
        repo = FraiseQLRepository(mock_pool, context={"mode": "normal"})

        query = """
            mutation CreateEntity($input: CreateEntityInput!) {
                createEntity(input: $input) {
                    __typename
                    ... on CreateEntitySuccess {
                        status
                        message
                        entity {
                            id
                            name
                            description
                            active
                        }
                    }
                    ... on CreateEntityError {
                        status
                        message
                    }
                }
            }
        """
        variables = {"input": {"name": "Test Entity", "description": "Test Description"}}

        result = await execute(
            graphql_schema, parse(query), variable_values=variables, context_value={"db": repo}
        )

        # Verify the result - this should PASS in Python mode
        assert result.errors is None, f"Unexpected errors: {result.errors}"
        assert result.data is not None

        mutation_result = result.data["createEntity"]
        assert mutation_result["__typename"] == "CreateEntitySuccess"
        assert mutation_result["status"] == "success"
        assert mutation_result["message"] == "Entity created successfully"

        # This is the critical test - entity should NOT be null
        assert mutation_result["entity"] is not None, "Entity field is null in Python mode!"
        assert mutation_result["entity"]["name"] == "Test Entity"
        assert mutation_result["entity"]["description"] == "Test Description"
        assert mutation_result["entity"]["active"] is True
        assert isinstance(mutation_result["entity"]["id"], str)

    async def test_mutation_rust_mode_works(self, graphql_schema, mock_pool):
        """Test that mutation works in Rust mode after fix.

        This test previously failed with 'missing a required argument: entity'.
        After the fix to _extract_field_value, this should now pass.
        """
        # Create repository with Rust mode context
        repo = FraiseQLRepository(mock_pool, context={"mode": "unified_rust"})

        query = """
            mutation CreateEntity($input: CreateEntityInput!) {
                createEntity(input: $input) {
                    __typename
                    ... on CreateEntitySuccess {
                        status
                        message
                        entity {
                            id
                            name
                            description
                            active
                        }
                    }
                    ... on CreateEntityError {
                        status
                        message
                    }
                }
            }
        """
        variables = {"input": {"name": "Test Entity", "description": "Test Description"}}

        result = await execute(
            graphql_schema, parse(query), variable_values=variables, context_value={"db": repo}
        )

        # After fix, this should pass (result.errors should be None)
        assert result.errors is None, f"Mutation failed in Rust mode: {result.errors}"
        assert result.data is not None

        mutation_result = result.data["createEntity"]
        assert mutation_result["__typename"] == "CreateEntitySuccess"
        assert mutation_result["status"] == "success"
        assert mutation_result["message"] == "Entity created successfully"

        # This is the critical test - entity should NOT be null
        assert mutation_result["entity"] is not None, "Entity field is null in Rust mode!"
        assert mutation_result["entity"]["name"] == "Test Entity"
        assert mutation_result["entity"]["description"] == "Test Description"
        assert mutation_result["entity"]["active"] is True
        assert isinstance(mutation_result["entity"]["id"], str)

    async def test_mutation_with_context_params_rust_mode(self, clear_registry):
        """Test mutation with context parameters in Rust mode.

        This tests the exact pattern from issue #110 with context_params.
        """
        # Define types for this test
        @input_type
        class CreateEntityContextInput:
            name: str
            description: str | None = None

        @success
        class CreateEntityContextSuccess:
            status: str = "success"
            message: str = "Entity created successfully"
            entity: Entity

        @failure
        class CreateEntityContextError:
            status: str = "error"
            message: str = ""

        @mutation(
            function="create_entity_with_context",
            schema="app",
            context_params={
                "tenant_id": "input_tenant_id",
                "user_id": "input_created_by",
            },
        )
        class CreateEntityWithContext:
            input: CreateEntityContextInput
            success: CreateEntityContextSuccess
            failure: CreateEntityContextError

        # Build schema
        @fraiseql.type
        class QueryRoot:
            dummy: str = fraiseql.fraise_field(default="dummy")

        schema = build_fraiseql_schema(
            query_types=[QueryRoot],
            mutation_resolvers=[CreateEntityWithContext],
            camel_case_fields=True,
        )

        # This test will be implemented after we confirm the basic case works
        pytest.skip("Context params test - implement after basic case is fixed")
