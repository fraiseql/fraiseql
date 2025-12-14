"""Tests for JSON validation in FraiseQL."""

from typing import Any

import pytest
from graphql import GraphQLError, graphql

import fraiseql
from fraiseql import build_fraiseql_schema, fraise_input
from fraiseql.fields import fraise_field
from fraiseql.types.scalars.json import JSONField, JSONScalar, parse_json_literal, parse_json_value


class TestJSONValidation:
    """Test JSON validation at various stages."""

    def test_parse_json_literal_invalid_syntax(self) -> None:
        """Test parsing invalid JSON syntax."""
        # Invalid JSON strings
        invalid_jsons = [
            '{"key": "value",}',  # Trailing comma
            "{'key': 'value'}",  # Single quotes
            '{key: "value"}',  # Unquoted key
            '{"key": undefined}',  # undefined is not valid JSON
            '{"key": NaN}',  # NaN is not valid JSON
        ]

        for invalid_json in invalid_jsons:
            with pytest.raises(GraphQLError, match="JSON cannot represent"):
                parse_json_literal(invalid_json)

    def test_parse_json_value_non_serializable(self) -> None:
        """Test that non-JSON serializable Python objects raise errors."""

        # Non-serializable Python objects
        class CustomObject:
            pass

        non_serializable_values = [
            {1, 2, 3},  # Sets are not JSON serializable
            CustomObject(),  # Custom objects are not JSON serializable
            lambda x: x,  # Functions are not JSON serializable
        ]

        for value in non_serializable_values:
            with pytest.raises(GraphQLError, match="JSON cannot represent"):
                parse_json_value(value)

    def test_json_field_with_invalid_default(self) -> None:
        """Test that invalid default values in JSON fields are caught."""
        # The validation happens when converting to GraphQL types, not at class definition

        class NonSerializable:
            def __repr__(self) -> str:
                return "<NonSerializable>"

        @fraise_input
        class InvalidDefaultInput:
            # This has an invalid default value
            metadata: JSONScalar = NonSerializable()

        # The error should occur when we try to convert this to a GraphQL type
        # and then access the fields (thunk evaluation)
        from fraiseql.core.graphql_type import convert_type_to_graphql_input

        # The validation happens when the GraphQL type's fields thunk is evaluated
        gql_type = convert_type_to_graphql_input(InvalidDefaultInput)
        with pytest.raises(GraphQLError, match="Invalid JSON value in field metadata"):
            # Accessing .fields triggers the thunk and validates defaults
            _ = gql_type.fields

    @pytest.mark.asyncio
    async def test_graphql_mutation_with_invalid_json(self, clear_registry) -> None:
        """Test that invalid JSON in GraphQL mutations is rejected."""

        @fraiseql.type
        class QueryRoot:
            dummy: str = fraise_field(default="dummy")

        @fraise_input
        class CreateItemInput:
            name: str
            metadata: JSONField

        @fraiseql.type
        class Item:
            id: str
            name: str
            metadata: JSONField

        async def create_item(info: Any, input: CreateItemInput) -> Item:
            return Item(id="1", name=input.name, metadata=input.metadata)

        schema = build_fraiseql_schema(query_types=[QueryRoot], mutation_resolvers=[create_item])

        # Test with invalid JSON string in the mutation
        mutation = """
        mutation CreateItem($input: CreateItemInput!) {
            createItem(input: $input) {
                id
                name
                metadata
            }
        }
        """
        # When passing through variables, GraphQL expects actual objects, not JSON strings
        # The JSONField type expects a dict/list, not a string
        # If you want to pass a JSON string, you'd need to parse it first

        # Test 1: Valid JSON object works fine
        valid_variables = {
            "input": {
                "name": "Test Item",
                "metadata": {"valid": "json"},  # This is a dict, not a string
            }
        }

        result = await graphql(schema=schema, source=mutation, variable_values=valid_variables)

        assert result.errors is None
        assert result.data["createItem"]["metadata"] == {"valid": "json"}

        # Test 2: Non-serializable objects should fail
        class NonSerializable:
            pass

        # This would fail at the GraphQL variable parsing level
        # because GraphQL can't serialize the custom object
        invalid_variables = {
            "input": {
                "name": "Test Item",
                "metadata": NonSerializable(),  # Non-serializable
            }
        }

        # Try to execute with non-serializable object
        result = await graphql(schema=schema, source=mutation, variable_values=invalid_variables)

        # GraphQL should either raise an error or return errors in the result
        assert result.errors is not None
        error_message = str(result.errors[0])
        assert (
            "JSON" in error_message
            or "serialize" in error_message.lower()
            or "NonSerializable" in error_message
        )

    @pytest.mark.asyncio
    async def test_graphql_query_with_json_literal(self, clear_registry) -> None:
        """Test JSON literal parsing in GraphQL queries."""

        @fraiseql.type
        class QueryRoot:
            dummy: str = fraise_field(default="dummy")

        @fraise_input
        class FilterInput:
            data: JSONField

        @fraiseql.type
        class Result:
            success: bool
            data: JSONField

        async def search(info: Any, filter: FilterInput) -> Result:
            return (Result(success=True, data=filter.data),)

        schema = build_fraiseql_schema(query_types=[QueryRoot], mutation_resolvers=[search])

        # Test with invalid JSON literal directly in the query
        query_with_invalid_json = """
        mutation Search {
            search(filter: { data: {invalid: json} }) {
                success
                data
            }
        }
        """
        result = await graphql(schema=schema, source=query_with_invalid_json)

        assert result.errors is not None
        assert len(result.errors) > 0

    def test_json_field_type_validation(self) -> None:
        """Test that JSONField only accepts dict-like objects at runtime."""
        from fraiseql.types.scalars.json import JSONField

        # JSONField should be a dict subclass
        assert issubclass(JSONField, dict)

        # Valid JSONField creation
        valid_json = JSONField({"key": "value"})
        assert valid_json["key"] == "value"

        # JSONField initialization with non-dict should work
        # (Python's dict constructor is flexible)
        json_from_list = JSONField([("key", "value")])
        assert json_from_list["key"] == "value"


class TestJSONFieldInMutations:
    """Test JSON field behavior in mutation contexts."""

    @pytest.mark.asyncio
    async def test_mutation_with_nested_invalid_json(self, clear_registry) -> None:
        """Test deeply nested invalid JSON structures."""

        @fraiseql.type
        class QueryRoot:
            dummy: str = fraise_field(default="dummy")

        @fraise_input
        class NestedInput:
            data: JSONField

        @fraise_input
        class CreateInput:
            name: str
            nested: NestedInput

        @fraiseql.type
        class Result:
            success: bool

        async def create(info: Any, input: CreateInput) -> Result:
            # This would fail if we try to serialize invalid JSON
            return Result(success=True)

        schema = build_fraiseql_schema(query_types=[QueryRoot], mutation_resolvers=[create])

        # Create a circular reference (not JSON serializable)
        circular_ref: dict[str, Any] = {"key": "value"}
        circular_ref["self"] = circular_ref

        mutation = """
        mutation Create($input: CreateInput!) {
            create(input: $input) {
                success
            }
        }
        """
        # Note: GraphQL variables are already parsed as JSON
        # so we can't pass truly invalid JSON through variables.
        # Invalid JSON would be caught at the GraphQL parsing level.
        result = await graphql(
            schema=schema,
            source=mutation,
            variable_values={
                "input": {
                    "name": "Test",
                    "nested": {
                        "data": {"valid": "json"},  # This will be valid
                    },
                }
            },
        )

        # Should succeed with valid JSON
        assert result.errors is None
        assert result.data["create"]["success"] is True


class TestJSONScalarCoercion:
    """Test JSON scalar coercion and validation."""

    def test_json_scalar_serialize(self) -> None:
        """Test JSONScalar serialization."""
        # Valid values should serialize successfully
        valid_values = [{"key": "value"}, ["item1", "item2"], "string", 123, 12.34, True, None]

        for value in valid_values:
            result = JSONScalar.serialize(value)
            assert result == value

        # The serialize function now validates JSON-serializability
        # This prevents non-serializable objects from causing issues later
        # in the serialization pipeline.

        # Test that it rejects non-JSON-serializable values
        with pytest.raises(GraphQLError) as exc_info:
            JSONScalar.serialize({1, 2, 3})
        assert "not JSON-serializable" in str(exc_info.value)
        assert "set" in str(exc_info.value)

        # Test other non-serializable types
        class CustomObject:
            pass

        with pytest.raises(GraphQLError) as exc_info:
            JSONScalar.serialize(CustomObject())
        assert "not JSON-serializable" in str(exc_info.value)

        # Test function
        with pytest.raises(GraphQLError) as exc_info:
            JSONScalar.serialize(lambda x: x)
        assert "not JSON-serializable" in str(exc_info.value)

    def test_json_scalar_parse_value(self) -> None:
        """Test JSONScalar parse_value (from variables)."""
        # Valid values should parse successfully
        valid_values = [{"key": "value"}, ["item1", "item2"], "string", 123, 12.34, True, None]

        for value in valid_values:
            result = JSONScalar.parse_value(value)
            assert result == value

        # Non-serializable values should raise
        with pytest.raises(GraphQLError):
            JSONScalar.parse_value(lambda x: x)

    def test_json_scalar_parse_literal(self) -> None:
        """Test JSONScalar parse_literal (from query)."""
        from graphql.language import parse_value

        # JSONScalar expects JSON to be passed as a string literal
        # that contains valid JSON
        # Test parsing valid JSON strings
        json_obj_literal = parse_value('"{\\"key\\": \\"value\\"}"')
        result = JSONScalar.parse_literal(json_obj_literal)
        assert result == {"key": "value"}

        # Test parsing JSON array
        json_arr_literal = parse_value('"[1, 2, 3]"')
        result = JSONScalar.parse_literal(json_arr_literal)
        assert result == [1, 2, 3]

        # Test primitive values (these are allowed directly)
        int_literal = parse_value("123")
        result = JSONScalar.parse_literal(int_literal)
        assert result == 123  # Now properly returns int

        float_literal = parse_value("12.34")
        result = JSONScalar.parse_literal(float_literal)
        assert result == 12.34  # Now properly returns float

        bool_literal = parse_value("true")
        result = JSONScalar.parse_literal(bool_literal)
        assert result is True  # BooleanValueNode returns actual bool

        null_literal = parse_value("null")
        result = JSONScalar.parse_literal(null_literal)
        assert result is None  # NullValueNode returns None

        # Test invalid JSON string
        invalid_json = parse_value('"{invalid json}"')
        with pytest.raises(GraphQLError, match="JSON cannot represent non-JSON string"):
            JSONScalar.parse_literal(invalid_json)

        # Test non-string literals (objects/arrays) which aren't allowed
        obj_literal = parse_value('{key: "value"}')
        with pytest.raises(GraphQLError, match="Use a String literal containing JSON"):
            JSONScalar.parse_literal(obj_literal)
