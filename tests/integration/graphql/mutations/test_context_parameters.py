"""Test context parameters in mutations."""

from unittest.mock import AsyncMock, MagicMock
from uuid import UUID

import pytest

from fraiseql.mutations.mutation_decorator import MutationDefinition, mutation


# Test types for context parameter mutations
class CreateLocationInput:
    def __init__(self, name: str, address: str):
        self.name = name
        self.address = address

    def to_dict(self):
        return {"name": self.name, "address": self.address}


class CreateLocationSuccess:
    def __init__(self, location_id: UUID):
        self.location_id = location_id


class CreateLocationError:
    def __init__(self, message: str, code: str):
        self.message = message
        self.code = code


@mutation(
    function="create_location",
    schema="app",
    context_params={"tenant_id": "input_pk_organization", "user": "input_created_by"},
)
class CreateLocation:
    input: CreateLocationInput
    success: CreateLocationSuccess
    error: CreateLocationError


class TestContextParameters:
    """Test context parameter functionality."""

    def test_mutation_definition_with_context_params(self):
        """Test MutationDefinition stores context parameters correctly."""
        context_params = {"tenant_id": "input_pk_organization", "user": "input_created_by"}

        definition = MutationDefinition(CreateLocation, "create_location", "app", context_params)

        assert definition.context_params == context_params
        assert definition.function_name == "create_location"
        assert definition.schema == "app"

    def test_mutation_definition_without_context_params(self):
        """Test MutationDefinition works without context parameters."""
        definition = MutationDefinition(CreateLocation, "create_location", "app")

        assert definition.context_params == {}

    @pytest.mark.asyncio
    async def test_resolver_with_context_params(self):
        """Test resolver extracts context parameters and calls database correctly."""
        # Create mock database - return simple dict to avoid parser issues
        mock_db = AsyncMock()
        mock_db.execute_function_with_context.return_value = {}

        # Create mock user context
        class MockUserContext:
            def __init__(self, user_id: str):
                self.user_id = user_id

        mock_user = MockUserContext("user-123")
        tenant_id = "tenant-456"

        # Create mock GraphQL info
        mock_info = MagicMock()
        mock_info.context = {"db": mock_db, "tenant_id": tenant_id, "user": mock_user}

        # Create input
        input_data = CreateLocationInput("Test Location", "123 Test St")

        # Create mutation definition and resolver
        definition = MutationDefinition(
            CreateLocation,
            "create_location",
            "app",
            {"tenant_id": "input_pk_organization", "user": "input_created_by"},
        )
        resolver = definition.create_resolver()

        # Call resolver - expect it to fail due to empty result, but that's fine for this test
        try:
            await resolver(mock_info, input_data)
        except:
            pass  # We don't care about the parsing error, just the DB call

        # Verify database was called with context parameters
        mock_db.execute_function_with_context.assert_called_once()
        call_args = mock_db.execute_function_with_context.call_args

        # Check function name
        assert call_args[0][0] == "app.create_location"

        # Check context arguments (tenant_id, user_id)
        context_args = call_args[0][1]
        assert context_args == [tenant_id, mock_user.user_id]

        # Check input data
        input_dict = call_args[0][2]
        assert input_dict["name"] == "Test Location"
        assert input_dict["address"] == "123 Test St"

    @pytest.mark.asyncio
    async def test_resolver_without_context_params(self):
        """Test resolver falls back to original execute_function without context."""
        # Create mock database - return simple dict to avoid parser issues
        mock_db = AsyncMock()
        mock_db.execute_function.return_value = {}

        # Create mock GraphQL info
        mock_info = MagicMock()
        mock_info.context = {"db": mock_db}

        # Create input
        input_data = CreateLocationInput("Test Location", "123 Test St")

        # Create mutation definition without context params
        definition = MutationDefinition(CreateLocation, "create_location", "app")
        resolver = definition.create_resolver()

        # Call resolver - expect it to fail due to empty result, but that's fine for this test
        try:
            await resolver(mock_info, input_data)
        except:
            pass  # We don't care about the parsing error, just the DB call

        # Verify original execute_function was called
        mock_db.execute_function.assert_called_once()
        mock_db.execute_function_with_context.assert_not_called()

    @pytest.mark.asyncio
    async def test_resolver_missing_context_parameter(self):
        """Test resolver raises error when required context parameter is missing."""
        # Create mock database
        mock_db = AsyncMock()

        # Create mock GraphQL info missing tenant_id
        mock_info = MagicMock()
        mock_info.context = {"db": mock_db}  # Missing tenant_id and user

        # Create input
        input_data = CreateLocationInput("Test Location", "123 Test St")

        # Create mutation definition with context params
        definition = MutationDefinition(
            CreateLocation, "create_location", "app", {"tenant_id": "input_pk_organization"}
        )
        resolver = definition.create_resolver()

        # Call resolver and expect error
        with pytest.raises(RuntimeError, match="Required context parameter 'tenant_id' not found"):
            await resolver(mock_info, input_data)

    def test_decorator_with_context_params(self):
        """Test @mutation decorator accepts context_params parameter."""
        # This test verifies the decorator was applied correctly
        assert hasattr(CreateLocation, "__fraiseql_mutation__")

        definition = CreateLocation.__fraiseql_mutation__
        assert isinstance(definition, MutationDefinition)
        assert definition.context_params == {
            "tenant_id": "input_pk_organization",
            "user": "input_created_by",
        }
        assert definition.function_name == "create_location"
        assert definition.schema == "app"
