"""Test context parameters in mutations."""

import pytest

from fraiseql.mutations.mutation_decorator import MutationDefinition, mutation

pytestmark = pytest.mark.integration


# Test types for context parameter mutations
class CreateLocationInput:
    def __init__(self, name: str, address: str) -> None:
        self.name = name
        self.address = address

    def to_dict(self) -> None:
        return {"name": self.name, "address": self.address}


class CreateLocationSuccess:
    def __init__(self, location_id: str) -> None:
        self.location_id = location_id


class CreateLocationError:
    def __init__(self, message: str, code: str) -> None:
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

    def test_mutation_definition_with_context_params(self) -> None:
        """Test MutationDefinition stores context parameters correctly."""
        context_params = {"tenant_id": "input_pk_organization", "user": "input_created_by"}

        definition = MutationDefinition(CreateLocation, "create_location", "app", context_params)

        assert definition.context_params == context_params
        assert definition.function_name == "create_location"
        assert definition.schema == "app"

    def test_mutation_definition_without_context_params(self) -> None:
        """Test MutationDefinition works without context parameters."""
        definition = MutationDefinition(CreateLocation, "create_location", "app")

        assert definition.context_params == {}

    def test_decorator_with_context_params(self) -> None:
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
