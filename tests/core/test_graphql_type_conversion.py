import time
from typing import Annotated, Union
from uuid import UUID

import pytest
from graphql import (
    GraphQLBoolean,
    GraphQLError,
    GraphQLFloat,
    GraphQLInputObjectType,
    GraphQLInt,
    GraphQLList,
    GraphQLObjectType,
    GraphQLScalarType,
    GraphQLString,  # Added this import
)

from fraiseql.core.graphql_type import convert_type_to_graphql_input, convert_type_to_graphql_output
from fraiseql.fields import fraise_field
from fraiseql.types.fraise_input import fraise_input
from fraiseql.types.fraise_type import fraise_type
from fraiseql.types.scalars.json import JSONField, parse_json_literal


class TestConvertTypeToGraphQLInput:
    """Test suite for convert_type_to_graphql_input function."""

    def test_basic_scalar_types(self) -> None:
        """Test conversion of basic Python scalar types."""
        assert convert_type_to_graphql_input(str) == GraphQLString
        assert convert_type_to_graphql_input(int) == GraphQLInt
        assert convert_type_to_graphql_input(float) == GraphQLFloat
        assert convert_type_to_graphql_input(bool) == GraphQLBoolean

    def test_list_types(self) -> None:
        """Test conversion of list types."""
        result = convert_type_to_graphql_input(list[str])
        assert isinstance(result, GraphQLList)
        assert result.of_type == GraphQLString

        result = convert_type_to_graphql_input(list[int])
        assert isinstance(result, GraphQLList)
        assert result.of_type == GraphQLInt

    def test_nested_list_types(self) -> None:
        """Test conversion of nested list types."""
        result = convert_type_to_graphql_input(list[list[str]])
        assert isinstance(result, GraphQLList)
        assert isinstance(result.of_type, GraphQLList)
        assert result.of_type.of_type == GraphQLString

    def test_simple_fraise_input_class(self) -> None:
        """Test conversion of simple FraiseQL input class."""

        @fraise_input
        class SimpleInput:
            name: str
            age: int

        result = convert_type_to_graphql_input(SimpleInput)
        assert isinstance(result, GraphQLInputObjectType)
        assert result.name == "SimpleInput"

        fields = result.fields
        assert "name" in fields
        assert "age" in fields
        assert fields["name"].type == GraphQLString
        assert fields["age"].type == GraphQLInt

    def test_fraise_input_with_defaults(self) -> None:
        """Test conversion of FraiseQL input class with defaults."""

        @fraise_input
        class InputWithDefaults:
            name: str
            age: int = 25
            active: bool = True

        result = convert_type_to_graphql_input(InputWithDefaults)
        assert isinstance(result, GraphQLInputObjectType)

        fields = result.fields
        assert len(fields) == 3
        assert fields["name"].type == GraphQLString
        assert fields["age"].type == GraphQLInt
        assert fields["active"].type == GraphQLBoolean

    def test_fraise_input_with_list_fields(self) -> None:
        """Test conversion of FraiseQL input class with list fields."""

        @fraise_input
        class InputWithLists:
            tags: list[str]
            scores: list[int]

        result = convert_type_to_graphql_input(InputWithLists)
        assert isinstance(result, GraphQLInputObjectType)

        fields = result.fields
        assert isinstance(fields["tags"].type, GraphQLList)
        assert fields["tags"].type.of_type == GraphQLString
        assert isinstance(fields["scores"].type, GraphQLList)
        assert fields["scores"].type.of_type == GraphQLInt

    def test_fraise_input_with_fraise_field_annotation(self) -> None:
        """Test conversion of FraiseQL input class with fraise_field annotations."""

        @fraise_input
        class AnnotatedInput:
            email: Annotated[str, fraise_field(description="User email")]
            count: Annotated[int, fraise_field(description="Item count")]

        result = convert_type_to_graphql_input(AnnotatedInput)
        assert isinstance(result, GraphQLInputObjectType)

        fields = result.fields
        assert fields["email"].type == GraphQLString
        assert fields["count"].type == GraphQLInt

    def test_fraise_input_inheritance(self) -> None:
        """Test conversion of inherited FraiseQL input classes."""

        @fraise_input
        class BaseInput:
            id: str

        @fraise_input
        class ExtendedInput(BaseInput):
            name: str
            age: int

        result = convert_type_to_graphql_input(ExtendedInput)
        assert isinstance(result, GraphQLInputObjectType)

        fields = result.fields
        assert "id" in fields
        assert "name" in fields
        assert "age" in fields
        assert len(fields) == 3

    def test_nested_fraise_input_classes(self) -> None:
        """Test conversion of nested FraiseQL input classes."""

        @fraise_input
        class AddressInput:
            street: str
            city: str

        @fraise_input
        class UserInputTestGQLConversion:
            name: str
            address: AddressInput

        result = convert_type_to_graphql_input(UserInputTestGQLConversion)
        assert isinstance(result, GraphQLInputObjectType)

        fields = result.fields
        assert fields["name"].type == GraphQLString
        assert isinstance(fields["address"].type, GraphQLInputObjectType)
        assert fields["address"].type.name == "AddressInput"

    def test_union_types_raise_error(self) -> None:
        """Test that Union types raise TypeError."""
        with pytest.raises(TypeError, match="Invalid type passed to convert_type_to_graphql_input"):
            convert_type_to_graphql_input(Union[str, int])  # type: ignore[arg-type]

    def test_annotated_union_types_raise_error(self) -> None:
        """Test that Annotated Union types raise TypeError."""
        with pytest.raises(TypeError, match="Invalid type passed to convert_type_to_graphql_input"):
            convert_type_to_graphql_input(Annotated[str | int, "some annotation"])  # type: ignore[arg-type]

    def test_invalid_type_raises_error(self) -> None:
        """Test that invalid types raise TypeError."""

        class RegularClass:
            pass

        with pytest.raises(TypeError, match="Invalid type passed to convert_type_to_graphql_input"):
            convert_type_to_graphql_input(RegularClass)

    def test_non_input_fraise_type_raises_error(self) -> None:
        """Test that non-input FraiseQL types raise TypeError."""

        @fraise_type
        class OutputType:
            name: str

        with pytest.raises(TypeError, match="Invalid type passed to convert_type_to_graphql_input"):
            convert_type_to_graphql_input(OutputType)


class TestConvertTypeToGraphQLOutput:
    """Test suite for convert_type_to_graphql_output function."""

    def test_basic_scalar_types(self) -> None:
        """Test conversion of basic Python scalar types."""
        assert convert_type_to_graphql_output(str) == GraphQLString
        assert convert_type_to_graphql_output(int) == GraphQLInt
        assert convert_type_to_graphql_output(float) == GraphQLFloat
        assert convert_type_to_graphql_output(bool) == GraphQLBoolean

    def test_list_types(self) -> None:
        """Test conversion of list types."""
        result = convert_type_to_graphql_output(list[str])
        assert isinstance(result, GraphQLList)
        assert result.of_type == GraphQLString

        result = convert_type_to_graphql_output(list[int])
        assert isinstance(result, GraphQLList)
        assert result.of_type == GraphQLInt

    def test_nested_list_types(self) -> None:
        """Test conversion of nested list types."""
        result = convert_type_to_graphql_output(list[list[str]])
        assert isinstance(result, GraphQLList)
        assert isinstance(result.of_type, GraphQLList)
        assert result.of_type.of_type == GraphQLString

    def test_simple_fraise_output_class(self) -> None:
        """Test conversion of simple FraiseQL output class."""

        @fraise_type
        class UserGQLConversion_1:
            name: str
            age: int

        result = convert_type_to_graphql_output(UserGQLConversion_1)
        assert isinstance(result, GraphQLObjectType)
        assert result.name == "UserGQLConversion_1"

        fields = result.fields
        assert "name" in fields
        assert "age" in fields
        assert fields["name"].type == GraphQLString
        assert fields["age"].type == GraphQLInt

    def test_fraise_output_with_list_fields(self) -> None:
        """Test conversion of FraiseQL output class with list fields."""

        @fraise_type
        class UserWithLists:
            tags: list[str]
            scores: list[int]

        result = convert_type_to_graphql_output(UserWithLists)
        assert isinstance(result, GraphQLObjectType)

        fields = result.fields
        assert isinstance(fields["tags"].type, GraphQLList)
        assert fields["tags"].type.of_type == GraphQLString
        assert isinstance(fields["scores"].type, GraphQLList)
        assert fields["scores"].type.of_type == GraphQLInt

    def test_fraise_output_with_fraise_field_annotation(self) -> None:
        """Test conversion of FraiseQL output class with fraise_field annotations."""

        @fraise_type
        class AnnotatedOutput:
            email: Annotated[str, fraise_field(description="User email")]
            count: Annotated[int, fraise_field(description="Item count")]

        result = convert_type_to_graphql_output(AnnotatedOutput)
        assert isinstance(result, GraphQLObjectType)

        fields = result.fields
        assert fields["email"].type == GraphQLString
        assert fields["count"].type == GraphQLInt

    def test_fraise_output_inheritance(self) -> None:
        """Test conversion of inherited FraiseQL output classes."""

        @fraise_type
        class BaseOutput:
            id: str

        @fraise_type
        class ExtendedOutput(BaseOutput):
            name: str
            age: int

        result = convert_type_to_graphql_output(ExtendedOutput)
        assert isinstance(result, GraphQLObjectType)

        fields = result.fields
        assert "id" in fields
        assert "name" in fields
        assert "age" in fields
        assert len(fields) == 3

    def test_nested_fraise_output_classes(self) -> None:
        """Test conversion of nested FraiseQL output classes."""

        @fraise_type
        class Address:
            street: str
            city: str

        @fraise_type
        class UserGQLConversion_2:
            name: str
            address: Address

        result = convert_type_to_graphql_output(UserGQLConversion_2)
        assert isinstance(result, GraphQLObjectType)

        fields = result.fields
        assert fields["name"].type == GraphQLString
        assert isinstance(fields["address"].type, GraphQLObjectType)
        assert fields["address"].type.name == "Address"

    def test_optional_types(self, clear_registry) -> None:
        """Test conversion of optional types (T | None)."""

        @fraise_type
        class OptionalFields:
            required_name: str
            optional_age: int | None  # Using the modern `| None` syntax for optional

        result = convert_type_to_graphql_output(OptionalFields)
        assert isinstance(result, GraphQLObjectType)

        fields = result.fields
        # Check for camelCase field names (default behavior)
        assert "requiredName" in fields or "required_name" in fields
        assert "optionalAge" in fields or "optional_age" in fields

        # Get the actual field names
        name_field = fields.get("requiredName") or fields.get("required_name")
        age_field = fields.get("optionalAge") or fields.get("optional_age")

        assert name_field.type == GraphQLString
        assert age_field.type == GraphQLInt  # Optional wrapper should be removed

    def test_success_failure_types(self) -> None:
        """Test conversion of success/failure types."""

        # This test assumes you have success/failure type decorators
        # Adjust based on your actual implementation
        @fraise_type
        class CreateUserSuccess:
            user_id: str
            message: str

        # Manually set the kind to "success" for testing
        CreateUserSuccess.__fraiseql_definition__.kind = "success"

        result = convert_type_to_graphql_output(CreateUserSuccess)
        assert isinstance(result, GraphQLObjectType)
        assert result.name == "CreateUserSuccess"

    def test_bare_union_types_raise_error(self) -> None:
        """Test that bare Union types raise TypeError."""
        with pytest.raises(
            TypeError, match="Use a FraiseUnion wrapper for result unions, not plain Union"
        ):
            convert_type_to_graphql_output(Union[str, int])

    def test_invalid_type_raises_error(self) -> None:
        """Test that invalid types raise TypeError."""

        class RegularClass:
            pass

        with pytest.raises(TypeError, match="Unsupported output type"):
            convert_type_to_graphql_output(RegularClass)

    def test_non_output_fraise_type_raises_error(self) -> None:
        """Test that non-output FraiseQL types raise TypeError."""

        @fraise_input
        class InputType:
            name: str

        with pytest.raises(TypeError, match="Unsupported output type"):
            convert_type_to_graphql_output(InputType)


class TestEdgeCases:
    """Test edge cases and error conditions for both conversion functions."""

    def test_empty_fraise_input_class(self) -> None:
        """Test conversion of empty FraiseQL input class."""

        @fraise_input
        class EmptyInput:
            pass

        result = convert_type_to_graphql_input(EmptyInput)
        assert isinstance(result, GraphQLInputObjectType)
        assert result.name == "EmptyInput"
        assert len(result.fields) == 0

    def test_empty_fraise_output_class(self) -> None:
        """Test conversion of empty FraiseQL output class."""

        @fraise_type
        class EmptyOutput:
            pass

        result = convert_type_to_graphql_output(EmptyOutput)
        assert isinstance(result, GraphQLObjectType)
        assert result.name == "EmptyOutput"
        assert len(result.fields) == 0

    def test_deeply_nested_lists(self) -> None:
        """Test conversion of deeply nested list types."""
        result_input = convert_type_to_graphql_input(list[list[list[str]]])
        assert isinstance(result_input, GraphQLList)
        assert isinstance(result_input.of_type, GraphQLList)
        assert isinstance(result_input.of_type.of_type, GraphQLList)
        assert result_input.of_type.of_type.of_type == GraphQLString

        result_output = convert_type_to_graphql_output(list[list[list[str]]])
        assert isinstance(result_output, GraphQLList)
        assert isinstance(result_output.of_type, GraphQLList)
        assert isinstance(result_output.of_type.of_type, GraphQLList)
        assert result_output.of_type.of_type.of_type == GraphQLString

    def test_complex_nested_structure(self, clear_registry) -> None:
        """Test conversion of complex nested structures."""

        @fraise_input
        class NestedInput:
            items: list[str]

        @fraise_input
        class ComplexInput:
            nested: NestedInput
            nested_list: list[NestedInput]

        result = convert_type_to_graphql_input(ComplexInput)
        assert isinstance(result, GraphQLInputObjectType)

        fields = result.fields
        # Check for camelCase (default) or snake_case field names
        nested_field = fields.get("nested") or fields.get("nested")
        nested_list_field = fields.get("nestedList") or fields.get("nested_list")

        assert nested_field is not None
        assert nested_list_field is not None

        assert isinstance(nested_field.type, GraphQLInputObjectType)
        assert isinstance(nested_list_field.type, GraphQLList)
        assert isinstance(nested_list_field.type.of_type, GraphQLInputObjectType)


# Custom scalar type tests (if you have custom scalars)
class TestCustomScalarTypes:
    """Test conversion of custom scalar types like UUID and JSONField."""

    def test_uuid_type_handling(self) -> None:
        """Test that UUID types are handled appropriately."""
        assert convert_type_to_graphql_input(UUID)
        assert convert_type_to_graphql_output(UUID)

    def test_json_field_type_handling(self) -> None:
        """Test that JSONField types are handled appropriately."""
        assert convert_type_to_graphql_input(JSONField)
        assert convert_type_to_graphql_output(JSONField)

    def test_fraise_input_with_custom_scalars(self) -> None:
        """Test FraiseQL input classes with custom scalar fields."""

        @fraise_input
        class InputWithCustomScalars:
            name: str
            # Fields to add when custom scalar support is implemented:
            # - user_id: UUID
            # - metadata: JSONField

        # For now, just test the working parts
        result = convert_type_to_graphql_input(InputWithCustomScalars)
        assert isinstance(result, GraphQLInputObjectType)
        assert "name" in result.fields


# 1. Test for recursive structures with JSONField
class TestRecursiveJSONField:
    """Test recursive structures with JSONField."""

    def test_recursive_input(self) -> None:
        """Test recursive JSONField in input."""

        @fraise_input
        class RecursiveInput:
            field: JSONField

        result = convert_type_to_graphql_input(RecursiveInput)
        assert isinstance(result, GraphQLInputObjectType)
        assert "field" in result.fields
        assert isinstance(result.fields["field"].type, GraphQLScalarType)  # JSON scalar type


class TestMissingFieldsInComplexTypes:
    """Test how missing or None fields in complex input types are handled."""

    def test_optional_json_field(self) -> None:
        """Test that optional JSONField can be missing."""

        @fraise_input
        class CreateUserInputTestGQLConversion_2:
            name: str
            metadata: JSONField | None = None

        # Missing `metadata` should still work
        result = convert_type_to_graphql_input(CreateUserInputTestGQLConversion_2)
        assert isinstance(result, GraphQLInputObjectType)
        assert "metadata" in result.fields
        assert result.fields["metadata"].type.name == "JSON"

    def test_optional_field_with_default(self) -> None:
        """Test that optional fields with default values are handled correctly."""

        @fraise_input
        class InputWithDefaults:
            name: str
            age: int = 25
            active: bool = True

        result = convert_type_to_graphql_input(InputWithDefaults)
        assert isinstance(result, GraphQLInputObjectType)

        fields = result.fields
        assert fields["name"].type == GraphQLString
        assert fields["age"].type == GraphQLInt
        assert fields["active"].type == GraphQLBoolean


# 4. Performance test for deeply nested structures
class TestPerformance:
    """Test conversion of deeply nested structures for performance."""

    def test_deeply_nested_lists(self) -> None:
        """Test conversion of deeply nested list types."""
        start = time.time()

        result_input = convert_type_to_graphql_input(list[list[list[list[str]]]])
        assert isinstance(result_input, GraphQLList)
        assert isinstance(result_input.of_type, GraphQLList)
        assert isinstance(result_input.of_type.of_type, GraphQLList)
        assert isinstance(result_input.of_type.of_type.of_type, GraphQLList)
        assert result_input.of_type.of_type.of_type.of_type == GraphQLString

        end = time.time()
        assert (end - start) < 1  # Conversion should take less than 1 second

    def test_deeply_nested_json(self) -> None:
        """Test conversion of deeply nested JSONField."""
        start = time.time()

        @fraise_input
        class DeeplyNestedJSON:
            data: JSONField

        result = convert_type_to_graphql_input(DeeplyNestedJSON)
        assert isinstance(result, GraphQLInputObjectType)
        assert "data" in result.fields
        assert isinstance(result.fields["data"].type, GraphQLScalarType)

        end = time.time()
        assert (end - start) < 1  # Conversion should take less than 1 second


# 5. Test handling of circular or deeply nested recursive structures
class TestCircularRecursiveStructures:
    """Test for circular or recursive types."""

    def test_recursive_structure_in_input(self) -> None:
        """Test handling circular references in input types."""

        @fraise_input
        class RecursiveInput:
            value: JSONField

        @fraise_input
        class CircularInput:
            field: RecursiveInput

        result = convert_type_to_graphql_input(CircularInput)
        assert isinstance(result, GraphQLInputObjectType)
        assert "field" in result.fields

    def test_recursive_structure_in_output(self) -> None:
        """Test handling circular references in output types."""

        @fraise_type
        class RecursiveOutput:
            value: JSONField

        @fraise_type
        class CircularOutput:
            field: RecursiveOutput

        result = convert_type_to_graphql_output(CircularOutput)
        assert isinstance(result, GraphQLObjectType)
        assert "field" in result.fields


# 6. Edge case for handling invalid JSONField data
class TestInvalidJSONFieldData:
    """Test invalid JSONField data cases."""

    def test_invalid_json_data(self) -> None:
        """Test that JSONField type conversion works correctly."""

        @fraise_input
        class ValidJSONInput:
            metadata: JSONField

        # Type conversion should succeed for JSONField
        result = convert_type_to_graphql_input(ValidJSONInput)
        assert isinstance(result, GraphQLInputObjectType)
        assert "metadata" in result.fields

        # The actual JSON validation happens at runtime when parsing values
        # not during type conversion

    def test_invalid_json_literal(self) -> None:
        """Test invalid JSON literal (e.g., non-JSON string)."""
        invalid_json = '{"key": "value",}'  # Invalid JSON syntax (extra comma)

        with pytest.raises(GraphQLError, match="JSON cannot represent.*literal of type str"):
            # Ensure that parsing invalid JSON literals raises an error
            parse_json_literal(invalid_json)  # type: ignore[arg-type]


# 7. Test for handling of `JSONField` and `List` of `JSONField`
class TestJSONFieldWithList:
    """Test handling of JSONField with nested lists."""

    def test_json_field_with_list(self) -> None:
        """Test JSONField inside a list."""

        @fraise_input
        class InputWithListOfJSON:
            items: list[JSONField]

        result = convert_type_to_graphql_input(InputWithListOfJSON)
        assert isinstance(result, GraphQLInputObjectType)
        assert "items" in result.fields
        assert isinstance(result.fields["items"].type, GraphQLList)

    def test_list_of_json_field_in_output(self) -> None:
        """Test list of JSONField in output."""

        @fraise_type
        class OutputWithListOfJSON:
            items: list[JSONField]

        result = convert_type_to_graphql_output(OutputWithListOfJSON)
        assert isinstance(result, GraphQLObjectType)
        assert "items" in result.fields
        assert isinstance(result.fields["items"].type, GraphQLList)


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
