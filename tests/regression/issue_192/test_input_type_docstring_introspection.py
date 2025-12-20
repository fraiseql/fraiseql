"""Regression test for Issue #192: Input type docstrings not exposed in GraphQL schema introspection.

GitHub Issue: https://github.com/fraiseql/fraiseql/issues/192

Problem:
    Docstrings on classes decorated with `@fraiseql.input` are not included in the
    GraphQL schema introspection, while docstrings on `@fraiseql.type`, `@fraiseql.success`,
    and `@fraiseql.error` decorated classes work correctly.

Expected Behavior:
    When a Python class decorated with `@fraiseql.input` has a docstring, that docstring
    should appear in the GraphQL schema's introspection query results.

Actual Behavior:
    Input type descriptions return `null` in introspection queries, even when comprehensive
    docstrings are present in the Python code.
"""

from typing import Any
from uuid import UUID

import pytest
from graphql import GraphQLSchema, graphql_sync

import fraiseql
from fraiseql import fraise_input, fraise_type
from fraiseql.decorators import query
from fraiseql.gql.builders.registry import SchemaRegistry
from fraiseql.mutations import mutation


@pytest.fixture
def registry(clear_registry):
    """Get a clean schema registry for each test."""
    return SchemaRegistry.get_instance()


def execute_introspection(schema: GraphQLSchema, query_string: str) -> dict[str, Any]:
    """Execute a GraphQL introspection query and return the data.

    Args:
        schema: The GraphQL schema to query
        query_string: The introspection query

    Returns:
        The query result data

    Raises:
        AssertionError: If the query has errors
    """
    result = graphql_sync(schema, query_string)
    assert not result.errors, f"Introspection query failed: {result.errors}"
    assert result.data is not None
    return result.data


@pytest.mark.regression
class TestInputTypeDocstringIntrospection:
    """Test that input type docstrings are exposed in GraphQL schema introspection."""

    def test_input_type_docstring_in_introspection(self, registry) -> None:
        """Test that @fraiseql.input docstrings appear in GraphQL introspection.

        This is the main regression test for issue #192.
        """
        # Define input type with comprehensive docstring
        @fraise_input
        class CreateMachineInput:
            """Input for creating a new printing machine.

            Creates a new machine record with associated model, contract, and order information.
            Supports both referencing existing orders and creating orders inline.
            """

            model_id: UUID
            machine_serial_number: str
            contract_id: UUID

        # Define a simple output type for comparison
        @fraise_type(sql_source="tv_machine")
        class Machine:
            """Printing device tracked in the system.

            Machines represent individual pieces of printing equipment deployed to
            customer locations.
            """

            id: UUID
            machine_serial_number: str

        # Define a simple query (required for schema)
        @query
        async def machines(info) -> list[Machine]:
            """List all machines."""
            return []

        # Define mutation that uses the input type
        @mutation
        async def create_machine(info, input: CreateMachineInput) -> Machine:
            """Create a new machine."""
            return None  # type: ignore

        # Build schema
        schema = registry.build_schema()

        # Execute introspection query
        data = execute_introspection(
            schema,
            """
            {
              inputType: __type(name: "CreateMachineInput") {
                name
                description
              }
              objectType: __type(name: "Machine") {
                name
                description
              }
            }
            """,
        )

        # Check object type description (this should already work)
        object_type = data["objectType"]
        assert object_type["name"] == "Machine"
        assert object_type["description"] is not None
        assert "Printing device tracked in the system" in object_type["description"]

        # Check input type description (THIS IS THE BUG FIX)
        input_type = data["inputType"]
        assert input_type["name"] == "CreateMachineInput"
        assert input_type["description"] is not None, (
            "Input type description should not be null. "
            "The docstring from @fraiseql.input should be exposed in GraphQL introspection."
        )
        assert "Input for creating a new printing machine" in input_type["description"]

    def test_all_decorator_types_preserve_docstrings(self, registry) -> None:
        """Test that all FraiseQL decorators preserve and expose docstrings."""
        # Define types with each decorator
        @fraise_type
        class User:
            """A user in the system."""

            id: UUID
            name: str

        @fraise_input
        class CreateUserInput:
            """Input for creating a new user."""

            name: str
            email: str

        @fraiseql.success
        class CreateUserSuccess:
            """Successful user creation result."""

            user: User

        @fraiseql.error
        class CreateUserError:
            """Error during user creation."""

            message: str

        # Define a simple query (required for schema)
        @query
        async def users(info) -> list[User]:
            """List all users."""
            return []

        @mutation
        async def create_user(info, input: CreateUserInput) -> User | CreateUserSuccess | CreateUserError:  # type: ignore
            """Create a new user."""
            return None  # type: ignore

        # Build schema and execute introspection
        schema = registry.build_schema()
        data = execute_introspection(
            schema,
            """
            {
              userType: __type(name: "User") {
                name
                description
              }
              inputType: __type(name: "CreateUserInput") {
                name
                description
              }
              successType: __type(name: "CreateUserSuccess") {
                name
                description
              }
              errorType: __type(name: "CreateUserError") {
                name
                description
              }
            }
            """,
        )

        # All types should have descriptions
        assert data["userType"]["description"] == "A user in the system."
        assert data["successType"]["description"] == "Successful user creation result."
        assert data["errorType"]["description"] == "Error during user creation."

        # Input type should also have description
        assert data["inputType"]["description"] is not None, (
            "@fraiseql.input should preserve docstrings like other decorators"
        )
        assert data["inputType"]["description"] == "Input for creating a new user."

    def test_input_type_without_docstring(self, registry) -> None:
        """Test that input types without docstrings have null description."""
        # Define input type WITHOUT docstring
        @fraise_input
        class SimpleInput:
            name: str
            value: int

        @fraise_type
        class SimpleOutput:
            result: str

        # Define a simple query (required for schema)
        @query
        async def simple_query(info) -> str:
            """Simple query."""
            return "test"

        @mutation
        async def simple_mutation(info, input: SimpleInput) -> SimpleOutput:
            return None  # type: ignore

        # Build schema and execute introspection
        schema = registry.build_schema()
        data = execute_introspection(
            schema,
            """
            {
              inputType: __type(name: "SimpleInput") {
                name
                description
              }
            }
            """,
        )

        # Input type without docstring should have null description
        assert data["inputType"]["description"] is None

    def test_input_type_field_descriptions(self, registry) -> None:
        """Test that input type field descriptions work correctly.

        While the class-level docstring may be missing, field descriptions
        should still work via fraise_field annotations.
        """
        from fraiseql import fraise_field

        @fraise_input
        class DetailedInput:
            """Input with detailed field descriptions."""

            name: str = fraise_field(description="The name of the entity")
            value: int = fraise_field(description="The numeric value")

        @fraise_type
        class SimpleOutput:
            result: str

        # Define a simple query (required for schema)
        @query
        async def simple_query(info) -> str:
            """Simple query."""
            return "test"

        @mutation
        async def detailed_mutation(info, input: DetailedInput) -> SimpleOutput:
            return None  # type: ignore

        # Build schema and execute introspection
        schema = registry.build_schema()
        data = execute_introspection(
            schema,
            """
            {
              inputType: __type(name: "DetailedInput") {
                name
                description
                inputFields {
                  name
                  description
                }
              }
            }
            """,
        )

        # Check field descriptions
        input_fields = {field["name"]: field for field in data["inputType"]["inputFields"]}
        assert input_fields["name"]["description"] == "The name of the entity"
        assert input_fields["value"]["description"] == "The numeric value"

        # Check class-level description
        assert data["inputType"]["description"] is not None, (
            "Input type class-level docstring should be exposed"
        )
