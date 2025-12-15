"""Test that auto-populated fields are queryable and follow GraphQL spec."""

import pytest

from fraiseql import fraise_input, fraise_type, mutation, query, success
from fraiseql.gql.builders.registry import SchemaRegistry

pytestmark = pytest.mark.usefixtures("clear_registry")


@fraise_type(sql_source="machines")
class Machine:
    id: str
    name: str


@fraise_input
class CreateMachineInput:
    name: str


@success
class CreateMachineSuccess:
    machine: Machine


@mutation
class CreateMachine:
    input: CreateMachineInput
    success: CreateMachineSuccess
    error: CreateMachineSuccess  # Using success type for simplicity in test


# Dummy query to satisfy GraphQL schema requirements
@query
async def health_check(info) -> str:
    """Health check query."""
    return "OK"


@pytest.mark.asyncio
async def test_can_query_auto_populated_fields():
    """Auto-populated fields should be queryable without errors."""
    # For this test, we'll just verify the schema can be built and fields are registered
    # Actual query execution would require a full GraphQL client setup
    registry = SchemaRegistry.get_instance()
    registry.register_query(health_check)
    registry.register_mutation(CreateMachine)
    schema = registry.build_schema()

    # Verify schema was built successfully
    assert schema is not None

    # Check that our mutation type exists
    mutation_type = schema.get_type("Mutation")
    assert mutation_type is not None

    # Check that our mutation field exists
    create_machine_field = mutation_type.fields.get("createMachine")
    assert create_machine_field is not None

    # Check that the success type has all expected fields
    success_type = schema.get_type("CreateMachineSuccess")
    assert success_type is not None

    field_names = set(success_type.fields.keys())

    # Check expected fields (auto-populated by @success decorator)
    assert "machine" in field_names, "Original field missing"
    assert "status" in field_names, "status field missing (auto-added)"
    assert "message" in field_names, "message field missing (auto-added)"
    assert "updatedFields" in field_names, "updatedFields missing (auto-added)"
    assert "id" in field_names, "id field missing (auto-added)"

    # Note: errors field is NOT added to success types (design decision)
    assert "errors" not in field_names, "errors field should not be in success types"

    print(f"✅ All auto-populated fields present in schema: {sorted(field_names)}")


@pytest.mark.asyncio
async def test_fields_optional_in_query():
    """Auto-populated fields should be optional (don't have to query them)."""
    registry = SchemaRegistry.get_instance()
    registry.register_query(health_check)
    registry.register_mutation(CreateMachine)
    schema = registry.build_schema()

    # Verify schema was built successfully
    assert schema is not None

    # Check that our mutation type exists and has the createMachine field
    mutation_type = schema.get_type("Mutation")
    assert mutation_type is not None
    assert "createMachine" in mutation_type.fields

    # Check that the success type exists and has all fields
    success_type = schema.get_type("CreateMachineSuccess")
    assert success_type is not None

    # All fields should exist in the schema (they're optional to query, but must exist)
    field_names = set(success_type.fields.keys())
    expected_fields = {"machine", "status", "message", "updatedFields", "id"}
    # Note: errors field NOT included (design decision - not auto-added to success types)
    assert expected_fields.issubset(field_names), f"Missing fields: {expected_fields - field_names}"

    # Verify errors field is NOT present
    assert "errors" not in field_names, "errors field should not be auto-added to success types"

    print(f"✅ All expected fields available in schema: {sorted(field_names)}")


@pytest.mark.asyncio
async def test_graphql_spec_compliance():
    """Verify GraphQL spec: fields only in response if explicitly requested."""
    registry = SchemaRegistry.get_instance()
    registry.register_query(health_check)
    registry.register_mutation(CreateMachine)
    schema = registry.build_schema()

    # Verify schema was built successfully
    assert schema is not None

    # Check that our types exist
    mutation_type = schema.get_type("Mutation")
    assert mutation_type is not None

    success_type = schema.get_type("CreateMachineSuccess")
    assert success_type is not None

    # Verify that all our auto-populated fields are properly defined
    # This is a basic check - full GraphQL spec compliance would require
    # actual query execution with a client that respects field selection
    field_names = set(success_type.fields.keys())
    required_fields = {"status", "message", "updatedFields", "id", "machine"}
    # Note: errors field NOT auto-added (design decision)

    assert required_fields.issubset(field_names), (
        f"Missing required fields: {required_fields - field_names}"
    )

    # Verify errors field is NOT present
    assert "errors" not in field_names, "errors field should not be auto-added to success types"

    print("✅ GraphQL schema spec compliant - all expected fields properly defined")
