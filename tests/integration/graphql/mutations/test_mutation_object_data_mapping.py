"""Test mutation object_data mapping in production mode.

This test reproduces the issue reported in FraiseQL where mutation
results return null for the object field despite successful creation.
"""

from uuid import UUID

import pytest
from graphql import execute, parse

import fraiseql
from fraiseql import failure, mutation, success
from fraiseql import input as input_type
from fraiseql.db import FraiseQLRepository
from fraiseql.gql.schema_builder import build_fraiseql_schema


# Define the GraphQL types
@fraiseql.type
class Location:
    id: UUID
    name: str
    identifier: str
    active: bool = True


@input_type
class CreateLocationInput:
    name: str
    identifier: str


@success
class CreateLocationSuccess:
    status: str = "success"
    message: str = ""
    location: Location | None = None


@failure
class CreateLocationError:
    status: str = "error"
    message: str = ""


@mutation(function="create_location", schema="app")
class CreateLocation:
    input: CreateLocationInput
    success: CreateLocationSuccess
    failure: CreateLocationError


@pytest.mark.database
class TestMutationObjectDataMapping:
    """Test mutation object_data mapping in production mode."""

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
            CREATE OR REPLACE FUNCTION app.create_location(
                p_input JSONB
            ) RETURNS app.mutation_result AS $$
            DECLARE
                v_id UUID;
                v_result app.mutation_result;
                v_name TEXT;
                v_identifier TEXT;
            BEGIN
                -- Extract fields from input
                v_name := p_input->>'name';
                v_identifier := p_input->>'identifier';

                -- Generate a new ID
                v_id := gen_random_uuid();

                -- Build the result
                v_result.id := v_id;
                v_result.updated_fields := ARRAY['created'];
                v_result.status := 'success';
                v_result.message := 'Location successfully created.';
                v_result.object_data := jsonb_build_object(
                    'id', v_id,
                    'name', v_name,
                    'identifier', v_identifier,
                    'active', true
                );
                v_result.extra_metadata := jsonb_build_object(
                    'entity', 'location',
                    'trigger', 'api_create'
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
            query_types=[QueryRoot], mutation_resolvers=[CreateLocation], camel_case_fields=True
        )

    @pytest.fixture
    def mock_pool_production(self, setup_database):
        """Create a mock pool for production mode."""

        class MockPool:
            def connection(self):
                class ConnContext:
                    async def __aenter__(self):
                        return setup_database

                    async def __aexit__(self, *args):
                        pass

                return ConnContext()

        return MockPool()

    @pytest.fixture
    def mock_pool_development(self, setup_database):
        """Create a mock pool for development mode."""

        class MockPool:
            def connection(self):
                class ConnContext:
                    async def __aenter__(self):
                        return setup_database

                    async def __aexit__(self, *args):
                        pass

                return ConnContext()

        return MockPool()

    async def test_mutation_object_data_mapping_production(
        self, graphql_schema, mock_pool_production, setup_database
    ):
        """Test that object_data is properly mapped in production mode."""
        # Create repository with production mode context
        repo = FraiseQLRepository(mock_pool_production, context={"mode": "production"})

        # Execute the mutation
        query = """
            mutation CreateLocation($input: CreateLocationInput!) {
                createLocation(input: $input) {
                    __typename
                    ... on CreateLocationSuccess {
                        status
                        message
                        location {
                            id
                            name
                            identifier
                            active
                        }
                    }
                    ... on CreateLocationError {
                        message
                    }
                }
            }
        """
        variables = {"input": {"name": "Test Warehouse", "identifier": "WH-001"}}

        result = await execute(
            graphql_schema, parse(query), variable_values=variables, context_value={"db": repo}
        )

        # Verify the result
        assert result.errors is None
        assert result.data is not None

        mutation_result = result.data["createLocation"]
        assert mutation_result["__typename"] == "CreateLocationSuccess"
        assert mutation_result["status"] == "success"
        assert mutation_result["message"] == "Location successfully created."

        # This is the critical test - location should NOT be null
        assert mutation_result["location"] is not None
        assert mutation_result["location"]["name"] == "Test Warehouse"
        assert mutation_result["location"]["identifier"] == "WH-001"
        assert mutation_result["location"]["active"] is True
        assert isinstance(mutation_result["location"]["id"], str)

    async def test_mutation_object_data_mapping_development(
        self, graphql_schema, mock_pool_development, setup_database
    ):
        """Test that object_data is properly mapped in development mode (control test)."""
        # Create repository with development mode context
        repo = FraiseQLRepository(mock_pool_development, context={"mode": "development"})

        # Execute the same mutation in development mode
        query = """
            mutation CreateLocation($input: CreateLocationInput!) {
                createLocation(input: $input) {
                    __typename
                    ... on CreateLocationSuccess {
                        status
                        message
                        location {
                            id
                            name
                            identifier
                            active
                        }
                    }
                    ... on CreateLocationError {
                        message
                    }
                }
            }
        """
        variables = {"input": {"name": "Dev Warehouse", "identifier": "WH-002"}}

        result = await execute(
            graphql_schema, parse(query), variable_values=variables, context_value={"db": repo}
        )

        # Verify the result
        assert result.errors is None
        assert result.data is not None

        mutation_result = result.data["createLocation"]
        assert mutation_result["__typename"] == "CreateLocationSuccess"
        assert mutation_result["status"] == "success"
        assert mutation_result["message"] == "Location successfully created."

        # In development mode, this should also work
        assert mutation_result["location"] is not None
        assert mutation_result["location"]["name"] == "Dev Warehouse"
        assert mutation_result["location"]["identifier"] == "WH-002"
        assert mutation_result["location"]["active"] is True

    async def test_mutation_with_entity_hint_in_metadata(
        self, graphql_schema, mock_pool_production, setup_database
    ):
        """Test that entity hint in extra_metadata helps with mapping."""
        # Create repository with production mode context
        repo = FraiseQLRepository(mock_pool_production, context={"mode": "production"})

        # The function already includes entity: 'location' in metadata
        # This should help the parser find the correct field to map object_data to

        query = """
            mutation {
                createLocation(input: {name: "Metadata Test", identifier: "MT-001"}) {
                    __typename
                    ... on CreateLocationSuccess {
                        location {
                            name
                        }
                    }
                }
            }
        """
        result = await execute(graphql_schema, parse(query), context_value={"db": repo})

        assert result.errors is None
        assert result.data["createLocation"]["location"] is not None
        assert result.data["createLocation"]["location"]["name"] == "Metadata Test"
