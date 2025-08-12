"""End-to-end tests for default schema configuration."""

import pytest
from fraiseql import mutation, fraise_type, fraise_input, query
from fraiseql.fastapi import FraiseQLConfig, create_fraiseql_app
from fraiseql.gql.builders.registry import SchemaRegistry
from httpx import AsyncClient
from unittest.mock import AsyncMock, patch


@pytest.fixture
def clean_registry():
    """Clean the schema registry before and after each test."""
    registry = SchemaRegistry.get_instance()
    registry.clear()
    yield
    registry.clear()


@fraise_input
class E2EInput:
    """Test input type."""
    name: str
    value: int


@fraise_type
class E2ESuccess:
    """Test success type."""
    message: str
    result: str


@fraise_type
class E2EError:
    """Test error type."""
    code: str
    message: str


# Dummy query to satisfy GraphQL schema requirements
@query
async def health_check(info) -> str:
    """Health check query."""
    return "OK"


@pytest.mark.asyncio
class TestDefaultSchemaE2E:
    """End-to-end tests for default schema configuration."""

    async def test_app_with_custom_default_mutation_schema(self, clean_registry):
        """Test that creating an app with custom default schema works."""
        # Create mutations without specifying schema
        @mutation(function="test_mutation")
        class TestMutation:
            input: E2EInput
            success: E2ESuccess
            failure: E2EError

        # Create app with custom default schema
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            default_mutation_schema="custom_app",
            default_query_schema="custom_queries"
        )

        app = create_fraiseql_app(
            config=config,
            mutations=[TestMutation],
            queries=[health_check],
            types=[E2ESuccess, E2EError]
        )

        # Verify the mutation uses the custom default schema
        assert TestMutation.__fraiseql_mutation__.schema == "custom_app"

        # Test with actual GraphQL query
        async with AsyncClient(app=app, base_url="http://test") as client:
            query = """
                mutation TestMutation($input: E2EInput!) {
                    testMutation(input: $input) {
                        __typename
                        ... on E2ESuccess {
                            message
                            result
                        }
                        ... on E2EError {
                            code
                            message
                        }
                    }
                }
            """

            # Mock the database execution
            with patch('fraiseql.fastapi.dependencies.get_db_pool') as mock_pool:
                mock_conn = AsyncMock()
                mock_pool.return_value = AsyncMock()
                mock_pool.return_value.connection.return_value.__aenter__.return_value = mock_conn

                # Mock the function call to verify the schema is used correctly
                mock_execute = AsyncMock(return_value=[{
                    "status": "success",
                    "message": "Test passed",
                    "object_data": {"message": "Test passed", "result": "Success"}
                }])

                with patch('fraiseql.cqrs.repository.CQRSRepository.execute_function', mock_execute):
                    response = await client.post(
                        "/graphql",
                        json={
                            "query": query,
                            "variables": {
                                "input": {
                                    "name": "test",
                                    "value": 42
                                }
                            }
                        }
                    )

                    # Verify the response
                    assert response.status_code == 200
                    data = response.json()

                    # Check that the function was called with the correct schema
                    mock_execute.assert_called()
                    call_args = mock_execute.call_args[0]
                    assert call_args[0] == "custom_app.test_mutation"

    async def test_multiple_apps_with_different_defaults(self, clean_registry):
        """Test that multiple apps can have different default schemas."""
        # Create first app with one default
        config1 = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            default_mutation_schema="app1"
        )

        @mutation(function="mutation1")
        class Mutation1:
            input: E2EInput
            success: E2ESuccess
            failure: E2EError

        app1 = create_fraiseql_app(
            config=config1,
            mutations=[Mutation1],
            queries=[health_check],
            types=[E2ESuccess, E2EError]
        )

        # Verify first mutation uses app1 schema
        assert Mutation1.__fraiseql_mutation__.schema == "app1"

        # Clean registry for second app
        registry = SchemaRegistry.get_instance()
        registry.clear()

        # Create second app with different default
        config2 = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            default_mutation_schema="app2"
        )

        @mutation(function="mutation2")
        class Mutation2:
            input: E2EInput
            success: E2ESuccess
            failure: E2EError

        app2 = create_fraiseql_app(
            config=config2,
            mutations=[Mutation2],
            queries=[health_check],
            types=[E2ESuccess, E2EError]
        )

        # Verify second mutation uses app2 schema
        assert Mutation2.__fraiseql_mutation__.schema == "app2"

    async def test_override_still_works_with_defaults(self, clean_registry):
        """Test that explicit schema override still works when defaults are set."""
        # Create app with default schema
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            default_mutation_schema="default_schema"
        )

        # Mutation with explicit schema override
        @mutation(function="override_mutation", schema="explicit_schema")
        class OverrideMutation:
            input: E2EInput
            success: E2ESuccess
            failure: E2EError

        # Mutation using default
        @mutation(function="default_mutation")
        class DefaultMutation:
            input: E2EInput
            success: E2ESuccess
            failure: E2EError

        app = create_fraiseql_app(
            config=config,
            mutations=[OverrideMutation, DefaultMutation],
            queries=[health_check],
            types=[E2ESuccess, E2EError]
        )

        # Verify schemas
        assert OverrideMutation.__fraiseql_mutation__.schema == "explicit_schema"
        assert DefaultMutation.__fraiseql_mutation__.schema == "default_schema"
