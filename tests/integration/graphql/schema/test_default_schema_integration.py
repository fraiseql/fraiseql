"""Integration tests for default schema configuration."""

from unittest.mock import AsyncMock, Mock

import pytest

from fraiseql import fraise_input, fraise_type, mutation
from fraiseql.fastapi import FraiseQLConfig
from fraiseql.gql.builders.registry import SchemaRegistry
from fraiseql.mutations.mutation_decorator import MutationDefinition


@pytest.fixture
def clean_registry():
    """Clean the schema registry before and after each test."""
    registry = SchemaRegistry.get_instance()
    registry.clear()
    yield
    registry.clear()


@fraise_input
class TestInput:
    """Test input type."""

    name: str
    value: int


@fraise_type
class TestSuccess:
    """Test success type."""

    message: str
    id: str


@fraise_type
class TestError:
    """Test error type."""

    code: str
    message: str


class TestDefaultSchemaIntegration:
    """Integration tests for default schema configuration."""

    def test_mutation_with_custom_default_schema(self, clean_registry):
        """Test that mutations use custom default schema from config."""
        # Create config with custom default schema
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            default_mutation_schema="app",
            default_query_schema="queries",
        )

        # Set config in registry
        registry = SchemaRegistry.get_instance()
        registry.config = config

        # Create a mutation without specifying schema
        @mutation(function="create_test")
        class CreateTest:
            input: TestInput
            success: TestSuccess
            failure: TestError

        # Verify the mutation uses the default schema
        definition = CreateTest.__fraiseql_mutation__
        assert isinstance(definition, MutationDefinition)
        assert definition.schema == "app"
        assert definition.function_name == "create_test"

    def test_multiple_mutations_with_different_schemas(self, clean_registry):
        """Test multiple mutations with different schema configurations."""
        # Set up config with default schema
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test", default_mutation_schema="app"
        )
        registry = SchemaRegistry.get_instance()
        registry.config = config

        # Mutation using default schema
        @mutation(function="create_default")
        class CreateDefault:
            input: TestInput
            success: TestSuccess
            failure: TestError

        # Mutation with explicit schema override
        @mutation(function="create_custom", schema="custom")
        class CreateCustom:
            input: TestInput
            success: TestSuccess
            failure: TestError

        # Mutation with explicit public schema
        @mutation(function="create_public", schema="public")
        class CreatePublic:
            input: TestInput
            success: TestSuccess
            failure: TestError

        # Verify each mutation uses the correct schema
        assert CreateDefault.__fraiseql_mutation__.schema == "app"
        assert CreateCustom.__fraiseql_mutation__.schema == "custom"
        assert CreatePublic.__fraiseql_mutation__.schema == "public"

    @pytest.mark.asyncio
    async def test_resolver_uses_correct_schema_in_function_call(self, clean_registry):
        """Test that the resolver uses the correct schema when calling database functions."""
        # Set up config with custom default schema
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test", default_mutation_schema="app"
        )
        registry = SchemaRegistry.get_instance()
        registry.config = config

        # Create mutation
        @mutation(function="process_data")
        class ProcessData:
            input: TestInput
            success: TestSuccess
            failure: TestError

        # Get the resolver
        resolver = ProcessData.__fraiseql_resolver__

        # Mock the database and context
        mock_db = AsyncMock()
        mock_db.execute_function.return_value = {
            "status": "success",
            "message": "Data processed",
            "object_data": {"id": "123", "message": "Success"},
        }

        info = Mock()
        info.context = {"db": mock_db}

        # Mock input
        input_obj = Mock()
        input_obj.name = "Test"
        input_obj.value = 42
        input_obj.__dict__ = {"name": "Test", "value": 42}

        # Call resolver
        result = await resolver(info, input_obj)

        # Verify the correct schema was used in the function call
        mock_db.execute_function.assert_called_once_with(
            "app.process_data", {"name": "Test", "value": 42}
        )

        # Verify result
        assert isinstance(result, TestSuccess)
        assert result.message == "Data processed"

    def test_schema_resolution_without_config(self, clean_registry):
        """Test schema resolution when no config is set."""
        # Ensure no config is set
        registry = SchemaRegistry.get_instance()
        registry.config = None

        # Create mutation without schema parameter
        @mutation(function="test_default")
        class TestDefault:
            input: TestInput
            success: TestSuccess
            failure: TestError

        # Should fall back to "public"
        assert TestDefault.__fraiseql_mutation__.schema == "public"

    def test_changing_config_affects_new_mutations(self, clean_registry):
        """Test that changing config affects newly created mutations."""
        registry = SchemaRegistry.get_instance()

        # Create first mutation with no config
        registry.config = None

        @mutation(function="first_mutation")
        class FirstMutation:
            input: TestInput
            success: TestSuccess
            failure: TestError

        assert FirstMutation.__fraiseql_mutation__.schema == "public"

        # Set config with custom default
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test", default_mutation_schema="custom_schema"
        )
        registry.config = config

        # Create second mutation
        @mutation(function="second_mutation")
        class SecondMutation:
            input: TestInput
            success: TestSuccess
            failure: TestError

        assert SecondMutation.__fraiseql_mutation__.schema == "custom_schema"

        # First mutation should still have its original schema
        assert FirstMutation.__fraiseql_mutation__.schema == "public"
