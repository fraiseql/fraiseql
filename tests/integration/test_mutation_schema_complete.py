"""Test that auto-populated fields appear in GraphQL schema."""

import pytest
from graphql import graphql_sync

from fraiseql import fraise_input, fraise_type, mutation, query, success
from fraiseql.gql.builders.registry import SchemaRegistry


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


@pytest.fixture
def clean_registry() -> None:
    """Clean the schema registry before and after each test."""
    from fraiseql.mutations.decorators import clear_mutation_registries

    registry = SchemaRegistry.get_instance()
    registry.clear()
    clear_mutation_registries()

    yield

    # Clear after test
    registry.clear()
    clear_mutation_registries()


@pytest.mark.asyncio
async def test_schema_includes_auto_populated_fields(clear_registry):
    """Test that auto-populated fields are included in the GraphQL schema."""
    # Re-register types that were cleared by the fixture
    from fraiseql import fraise_input, fraise_type, mutation, query, success

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

    @query
    async def test_health_check(info) -> str:
        return "OK"

    registry = SchemaRegistry.get_instance()
    schema = registry.build_schema()

    introspection_query = """
        query {
            __type(name: "CreateMachineSuccess") {
                fields {
                    name
                    type {
                        kind
                        name
                        ofType {
                            kind
                            name
                        }
                    }
                }
            }
        }
    """

    result = graphql_sync(schema, introspection_query)

    print(f"GraphQL Errors: {result.errors}")
    print(f"Result data: {result.data}")
    # Let's also check what types are actually in the schema
    all_types = [t.name for t in schema.type_map.values() if hasattr(t, "name") and t.name]
    print(f"Available types: {sorted(all_types)}")

    assert result.errors is None, f"Introspection errors: {result.errors}"

    field_names = {f["name"] for f in result.data["__type"]["fields"]}

    # All expected fields must be present
    assert "machine" in field_names, "Original field missing"
    assert "status" in field_names, "status field missing from schema"
    assert "message" in field_names, "message field missing from schema"
    # Note: 'errors' field removed in v1.8.1 - Success types no longer include errors
    assert "updatedFields" in field_names, "updatedFields missing (should be camelCase)"
    assert "id" in field_names, "id field missing from schema"

    print(f"✅ Schema fields: {sorted(field_names)}")


@pytest.mark.asyncio
async def test_field_types_correct(clear_registry):
    """Auto-populated fields should have correct GraphQL types."""
    # Re-register types that were cleared by the fixture
    from fraiseql import fraise_input, fraise_type, mutation, query, success

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

    @query
    async def test_health_check(info) -> str:
        return "OK"

    registry = SchemaRegistry.get_instance()
    schema = registry.build_schema()

    introspection_query = """
        query {
            __type(name: "CreateMachineSuccess") {
                fields {
                    name
                    type {
                        kind
                        name
                        ofType {
                            kind
                            name
                        }
                    }
                }
            }
        }
    """

    result = graphql_sync(schema, introspection_query)
    fields_by_name = {f["name"]: f["type"] for f in result.data["__type"]["fields"]}

    # status: String (SCALAR)
    assert fields_by_name["status"]["kind"] == "SCALAR"
    assert fields_by_name["status"]["name"] == "String"

    # message: String (nullable SCALAR)
    assert fields_by_name["message"]["kind"] == "SCALAR"
    assert fields_by_name["message"]["name"] == "String"

    # id: String (nullable SCALAR)
    assert fields_by_name["id"]["kind"] == "SCALAR"
    assert fields_by_name["id"]["name"] == "String"

    # Note: 'errors' field removed in v1.8.1 - Success types no longer include errors

    # updatedFields: [String] (LIST of SCALAR)
    assert fields_by_name["updatedFields"]["kind"] == "LIST"
    assert fields_by_name["updatedFields"]["ofType"]["kind"] == "SCALAR"
    assert fields_by_name["updatedFields"]["ofType"]["name"] == "String"

    print("✅ Field types correct")
