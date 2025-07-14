"""Test mutation object_data mapping in production mode.

This test reproduces the issue reported in PrintOptim where mutation
results return null for the object field despite successful creation.
"""

from uuid import UUID

import pytest

from fraiseql import FraiseQL
from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.types.common_outputs import MutationResultBase
from fraiseql.types.types import failure, fraise_type, input_type, mutation, success


# Define the GraphQL types
@fraise_type
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
class CreateLocationSuccess(MutationResultBase):
    location: Location | None = None


@failure
class CreateLocationError(MutationResultBase):
    pass


@mutation(
    function="create_location",
    schema="app",
)
class CreateLocation:
    input: CreateLocationInput
    success: CreateLocationSuccess
    failure: CreateLocationError


@pytest.mark.database
class TestMutationObjectDataMapping:
    """Test mutation object_data mapping in production mode."""

    @pytest.fixture
    def setup_database(self, db_connection_committed):
        """Set up test database schema and function."""
        db = db_connection_committed

        # Create the mutation_result type
        db.execute("""
            CREATE TYPE app.mutation_result AS (
                id UUID,
                updated_fields TEXT[],
                status TEXT,
                message TEXT,
                object_data JSONB,
                extra_metadata JSONB
            )
        """)

        # Create the function that returns mutation_result
        db.execute("""
            CREATE OR REPLACE FUNCTION app.create_location(
                p_name TEXT,
                p_identifier TEXT
            ) RETURNS app.mutation_result AS $$
            DECLARE
                v_id UUID;
                v_result app.mutation_result;
            BEGIN
                -- Generate a new ID
                v_id := gen_random_uuid();
                
                -- Build the result
                v_result.id := v_id;
                v_result.updated_fields := ARRAY['created'];
                v_result.status := 'success';
                v_result.message := 'Location successfully created.';
                v_result.object_data := jsonb_build_object(
                    'id', v_id,
                    'name', p_name,
                    'identifier', p_identifier,
                    'active', true
                );
                v_result.extra_metadata := jsonb_build_object(
                    'entity', 'location',
                    'trigger', 'api_create'
                );
                
                RETURN v_result;
            END;
            $$ LANGUAGE plpgsql;
        """)

        db.commit()
        return db

    @pytest.fixture
    def fraiseql_production(self, setup_database):
        """Create FraiseQL instance in production mode."""
        config = FraiseQLConfig(
            environment="production",
            database_url=setup_database.url,
            enable_introspection=True,
        )

        fraiseql = FraiseQL(config)
        fraiseql.mutation(CreateLocation)

        return fraiseql

    @pytest.fixture
    def fraiseql_development(self, setup_database):
        """Create FraiseQL instance in development mode."""
        config = FraiseQLConfig(
            environment="development",
            database_url=setup_database.url,
            enable_introspection=True,
        )

        fraiseql = FraiseQL(config)
        fraiseql.mutation(CreateLocation)

        return fraiseql

    async def test_mutation_object_data_mapping_production(
        self, fraiseql_production, setup_database
    ):
        """Test that object_data is properly mapped in production mode."""
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

        result = await fraiseql_production.execute_query(
            query, variables=variables, context={"db": setup_database}
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
        self, fraiseql_development, setup_database
    ):
        """Test that object_data is properly mapped in development mode (control test)."""
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

        result = await fraiseql_development.execute_query(
            query, variables=variables, context={"db": setup_database}
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

    async def test_mutation_with_entity_hint_in_metadata(self, fraiseql_production, setup_database):
        """Test that entity hint in extra_metadata helps with mapping."""
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

        result = await fraiseql_production.execute_query(query, context={"db": setup_database})

        assert result.errors is None
        assert result.data["createLocation"]["location"] is not None
        assert result.data["createLocation"]["location"]["name"] == "Metadata Test"
