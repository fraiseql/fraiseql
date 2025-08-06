"""Test for ID parameter naming issue."""

from uuid import UUID

import pytest

import fraiseql
from fraiseql import query
from fraiseql.gql.builders import SchemaComposer, SchemaRegistry


@fraiseql.type
class Allocation:
    id: UUID
    identifier: str
    machine_id: UUID | None


# Test query with id parameter
@query
async def allocation(info, id: UUID) -> Allocation | None:
    """Get allocation by ID."""
    # Simulate database lookup
    if str(id) == "12345678-1234-5678-1234-567812345678":
        return Allocation(id=id, identifier="TEST-001", machine_id=None)
    return None


# Test query with id_ parameter (workaround)
@query
async def allocation_workaround(info, id_: UUID) -> Allocation | None:
    """Get allocation by ID using id_ parameter."""
    # Simulate database lookup
    if str(id_) == "12345678-1234-5678-1234-567812345678":
        return Allocation(id=id_, identifier="TEST-001", machine_id=None)
    return None


class TestIdParameterIssue:
    def setup_method(self):
        """Clear registry before each test."""
        registry = SchemaRegistry.get_instance()
        registry.clear()

        # Re-register the queries and types defined in this module
        registry.register_type(Allocation)
        registry.register_query(allocation)
        registry.register_query(allocation_workaround)

    def test_query_with_id_parameter(self):
        """Test that queries can use 'id' as a parameter name."""
        # Create schema
        registry = SchemaRegistry.get_instance()
        composer = SchemaComposer(registry)
        schema = composer.compose()

        # Check that the schema has the expected field
        query_type = schema.type_map.get("Query")
        assert query_type is not None

        # Check allocation field exists
        allocation_field = query_type.fields.get("allocation")
        assert allocation_field is not None

        # Check that it has an 'id' argument
        assert "id" in allocation_field.args
        id_arg = allocation_field.args["id"]
        assert id_arg is not None

    def test_query_with_id_underscore_parameter(self):
        """Test that queries with id_ parameter work."""
        # Create schema
        registry = SchemaRegistry.get_instance()
        composer = SchemaComposer(registry)
        schema = composer.compose()

        # Check that the schema has the expected field
        query_type = schema.type_map.get("Query")
        assert query_type is not None

        # Check allocation_workaround field exists
        allocation_field = query_type.fields.get("allocationWorkaround")
        assert allocation_field is not None

        # Check that it has an 'id_' argument (or 'id' in GraphQL)
        # This is the key question - what does GraphQL see?
        print(f"Arguments for allocationWorkaround: {list(allocation_field.args.keys())}")

    @pytest.mark.asyncio
    async def test_resolver_execution_with_id(self):
        """Test that the resolver can be called with id parameter."""
        from graphql import graphql

        registry = SchemaRegistry.get_instance()
        composer = SchemaComposer(registry)
        schema = composer.compose()

        # GraphQL query using 'id' parameter
        query_str = """
            query GetAllocation($id: ID!) {
                allocation(id: $id) {
                    id
                    identifier
                }
            }
        """
        # Execute query
        result = await graphql(
            schema,
            query_str,
            variable_values={"id": "12345678-1234-5678-1234-567812345678"},
            context_value={"db": None},  # Mock context
        )

        # This is where we expect it to fail with "unexpected keyword argument 'id'"
        if result.errors:
            print(f"Errors: {result.errors}")
            # Check if it's the expected error
            error_msg = str(result.errors[0])
            print(f"Error message: {error_msg}")
            # The issue might be happening!
            if "unexpected keyword argument" in error_msg:
                print("FOUND THE ISSUE: GraphQL is passing 'id' but Python expects something else")
                # This is actually the bug we're looking for
                assert "id" in error_msg
        else:
            # If it works, check the result
            assert result.data is not None
            assert result.data["allocation"]["identifier"] == "TEST-001"
