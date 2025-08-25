import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.json import (
    JSONScalar,
    parse_json_literal,
    parse_json_value,
    serialize_json,
)


@pytest.mark.unit
class TestJSONScalar:
    """Test suite for JSON scalar type."""

    def test_serialize_dict(self) -> None:
        """Test serializing dictionary values."""
        data = {"key": "value", "number": 42}
        result = serialize_json(data)
        assert result == data

    def test_serialize_list(self) -> None:
        """Test serializing list values."""
        data = [1, 2, 3, "four"]
        result = serialize_json(data)
        assert result == data

    def test_serialize_primitive_types(self) -> None:
        """Test serializing primitive types."""
        assert serialize_json("string") == "string"
        assert serialize_json(123) == 123
        assert serialize_json(123.45) == 123.45
        assert serialize_json(True) is True
        assert serialize_json(None) is None

    def test_serialize_nested_structures(self) -> None:
        """Test serializing nested data structures."""
        data = {
            "user": {
                "name": "John",
                "tags": ["admin", "user"],
                "metadata": {"created": "2023-01-01", "active": True},
            }
        }
        result = serialize_json(data)
        assert result == data

    def test_parse_json_value_from_dict(self) -> None:
        """Test parsing JSON value from dictionary."""
        data = {"key": "value"}
        result = parse_json_value(data)
        assert result == data

    def test_parse_json_value_from_string(self) -> None:
        """Test parsing JSON value from JSON string - returns as-is."""
        json_string = '{"key": "value", "number": 42}'
        result = parse_json_value(json_string)
        assert result == json_string  # parse_json_value doesn't parse strings

    def test_parse_json_value_invalid_string(self) -> None:
        """Test parsing invalid JSON string - returns as-is."""
        invalid_json = "{invalid json}"
        # parse_json_value accepts any string as-is
        result = parse_json_value(invalid_json)
        assert result == invalid_json

    def test_parse_json_value_none(self) -> None:
        """Test parsing None value."""
        assert parse_json_value(None) is None

    def test_parse_json_literal_string(self) -> None:
        """Test parsing JSON literal from string AST node."""
        ast = StringValueNode(value='{"key": "value"}')
        result = parse_json_literal(ast)
        assert result == {"key": "value"}

    def test_parse_json_literal_invalid_json(self) -> None:
        """Test parsing invalid JSON literal raises error."""
        ast = StringValueNode(value="{invalid}")
        # Invalid JSON strings should raise an error
        with pytest.raises(GraphQLError, match="JSON cannot represent non-JSON string"):
            parse_json_literal(ast)

    def test_parse_json_literal_non_string_node(self) -> None:
        """Test parsing non-string AST nodes."""
        # JSON scalar accepts various literal types

        # IntValueNode should return integer
        ast_int = IntValueNode(value="123")
        assert parse_json_literal(ast_int) == 123

        # For truly unsupported types, test with a different node type
        from graphql.language import VariableNode

        ast_var = VariableNode(name={"value": "myVar"})
        with pytest.raises(
            GraphQLError, match="JSON cannot represent literal of type VariableNode"
        ):
            parse_json_literal(ast_var)

    def test_parse_json_literal_complex_json(self) -> None:
        """Test parsing complex JSON literal."""
        json_str = '{"users": [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}], "count": 2}'
        ast = StringValueNode(value=json_str)
        result = parse_json_literal(ast)

        assert result["count"] == 2
        assert len(result["users"]) == 2
        assert result["users"][0]["name"] == "Alice"

    def test_json_scalar_integration(self) -> None:
        """Test JSONScalar integration."""
        # Test serialize
        data = {"test": True}
        assert JSONScalar.serialize(data) == data

        # Test parse_value - it accepts Python objects, not JSON strings
        data_dict = {"test": True}
        assert JSONScalar.parse_value(data_dict) == data_dict

        # parse_value also accepts strings as-is
        json_str = '{"test": true}'
        assert JSONScalar.parse_value(json_str) == json_str

        # Test parse_literal - this parses JSON from string literals
        ast = StringValueNode(value=json_str)
        assert JSONScalar.parse_literal(ast) == {"test": True}
