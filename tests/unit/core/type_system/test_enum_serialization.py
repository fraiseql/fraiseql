import pytest

"""Test that enum values are properly serialized in GraphQL responses."""

import json
from enum import Enum

from graphql import GraphQLField, GraphQLObjectType, GraphQLSchema, graphql_sync

from fraiseql.types.enum import fraise_enum


@pytest.mark.unit
def test_enum_serialization() -> None:
    """Test that enum values can be JSON serialized when returned from GraphQL."""

    # Define a test enum
    @fraise_enum
    class TestSourceType(Enum):
        PRODUCT = "Product"
        ITEM = "Item"
        MACHINE = "Machine"

    # Get the GraphQL enum type
    graphql_enum = TestSourceType.__graphql_type__

    # Create a simple schema with a field that returns the enum
    schema = GraphQLSchema(
        query=GraphQLObjectType(
            "Query",
            fields={
                "sourceType": GraphQLField(
                    graphql_enum,
                    resolve=lambda obj, info: "Product",  # Return the primitive value
                )
            },
        )
    )

    # Execute a query
    result = graphql_sync(schema, "{ sourceType }")

    # The result should be JSON serializable
    assert result.errors is None
    assert result.data is not None

    # This should not raise an error
    json_str = json.dumps(result.data)
    assert json_str == '{"sourceType": "PRODUCT"}'  # GraphQL enum name is used in response


def test_enum_with_mutation_return() -> None:
    """Test that enums work correctly when returned from mutations."""
    from graphql import (
        GraphQLArgument,
        GraphQLInputField,
        GraphQLInputObjectType,
        GraphQLNonNull,
        GraphQLString,
    )

    @fraise_enum
    class StatusType(Enum):
        PENDING = "pending"
        COMPLETED = "completed"
        FAILED = "failed"

    # Create input and output types
    input_type = GraphQLInputObjectType(
        "TestInput",
        fields={
            "status": GraphQLInputField(StatusType.__graphql_type__),
            "name": GraphQLInputField(GraphQLNonNull(GraphQLString)),
        },
    )

    output_type = GraphQLObjectType(
        "TestOutput",
        fields={
            "status": GraphQLField(StatusType.__graphql_type__),
            "message": GraphQLField(GraphQLString),
        },
    )

    # Create mutation
    schema = GraphQLSchema(
        query=GraphQLObjectType("Query", fields={"dummy": GraphQLField(GraphQLString)}),
        mutation=GraphQLObjectType(
            "Mutation",
            fields={
                "testMutation": GraphQLField(
                    output_type,
                    args={"input": GraphQLArgument(input_type)},
                    resolve=lambda obj, info, input: {
                        "status": input["status"],  # This should be the primitive value
                        "message": "Success",
                    },
                )
            },
        ),
    )

    # Execute mutation
    query = """
        mutation {
            testMutation(input: {status: PENDING, name: "test"}) {
                status
                message
            }
        }
    """

    result = graphql_sync(schema, query)

    # Should not have errors
    assert result.errors is None
    assert result.data is not None

    # Should be JSON serializable
    json_str = json.dumps(result.data)
    assert '"status": "PENDING"' in json_str
    assert '"message": "Success"' in json_str


def test_enum_value_storage() -> None:
    """Test that GraphQLEnumValue stores primitive values, not enum members."""

    @fraise_enum
    class TestEnum(Enum):
        FIRST = 1
        SECOND = "two"
        THIRD = 3.14

    graphql_enum = TestEnum.__graphql_type__

    # Check that the values stored are primitives, not enum members
    assert graphql_enum.values["FIRST"].value == 1
    assert graphql_enum.values["SECOND"].value == "two"
    assert graphql_enum.values["THIRD"].value == 3.14

    # Ensure they're not enum members
    assert not isinstance(graphql_enum.values["FIRST"].value, Enum)
    assert not isinstance(graphql_enum.values["SECOND"].value, Enum)
    assert not isinstance(graphql_enum.values["THIRD"].value, Enum)
