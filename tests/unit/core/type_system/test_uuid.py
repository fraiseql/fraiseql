import uuid

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.uuid import (
    UUIDScalar,
    parse_uuid_literal,
    parse_uuid_value,
    serialize_uuid,
)


@pytest.mark.unit
class TestUUIDScalar:
    """Test suite for UUID scalar type."""

    def test_serialize_uuid_object(self) -> None:
        """Test serializing UUID object to string."""
        test_uuid = uuid.UUID("12345678-1234-5678-1234-567812345678")
        result = serialize_uuid(test_uuid)
        assert result == "12345678-1234-5678-1234-567812345678"

    def test_serialize_uuid_string(self) -> None:
        """Test serializing UUID string."""
        uuid_str = "12345678-1234-5678-1234-567812345678"
        result = serialize_uuid(uuid_str)
        assert result == uuid_str

    def test_serialize_invalid_uuid_string(self) -> None:
        """Test serializing invalid UUID string raises error."""
        with pytest.raises(GraphQLError, match="UUID cannot represent non-UUID value"):
            serialize_uuid("not-a-uuid")

    def test_serialize_non_uuid_type(self) -> None:
        """Test serializing non-UUID type raises error."""
        with pytest.raises(GraphQLError, match="UUID cannot represent non-UUID value"):
            serialize_uuid(123)

    def test_parse_uuid_value_from_string(self) -> None:
        """Test parsing UUID from string value."""
        uuid_str = "12345678-1234-5678-1234-567812345678"
        result = parse_uuid_value(uuid_str)
        assert isinstance(result, uuid.UUID)
        assert str(result) == uuid_str

    def test_parse_uuid_value_from_uuid_object(self) -> None:
        """Test parsing UUID from UUID object."""
        test_uuid = uuid.UUID("12345678-1234-5678-1234-567812345678")
        result = parse_uuid_value(str(test_uuid))
        assert result == test_uuid

    def test_parse_uuid_value_invalid_string(self) -> None:
        """Test parsing invalid UUID string raises error."""
        with pytest.raises(GraphQLError, match="Invalid UUID string provided"):
            parse_uuid_value("invalid-uuid")

    def test_parse_uuid_value_non_string(self) -> None:
        """Test parsing non-string value raises error."""
        with pytest.raises(GraphQLError, match="UUID cannot represent non-string value"):
            parse_uuid_value(12345)

    def test_parse_uuid_value_none(self) -> None:
        """Test parsing None value."""
        with pytest.raises(GraphQLError, match="UUID cannot represent non-string value"):
            parse_uuid_value(None)

    def test_parse_uuid_literal_valid(self) -> None:
        """Test parsing UUID literal from AST."""
        uuid_str = "12345678-1234-5678-1234-567812345678"
        ast = StringValueNode(value=uuid_str)
        result = parse_uuid_literal(ast)
        assert isinstance(result, uuid.UUID)
        assert str(result) == uuid_str

    def test_parse_uuid_literal_invalid_format(self) -> None:
        """Test parsing invalid UUID literal raises error."""
        ast = StringValueNode(value="not-a-valid-uuid")
        with pytest.raises(GraphQLError, match="Invalid UUID string provided"):
            parse_uuid_literal(ast)

    def test_parse_uuid_literal_non_string_node(self) -> None:
        """Test parsing non-string AST node raises error."""
        ast = IntValueNode(value="123")
        with pytest.raises(GraphQLError, match="UUID cannot represent non-string literal"):
            parse_uuid_literal(ast)

    def test_uuid_scalar_integration(self) -> None:
        """Test UUIDScalar scalar integration."""
        test_uuid = uuid.UUID("12345678-1234-5678-1234-567812345678")
        uuid_str = "12345678-1234-5678-1234-567812345678"

        # Test serialize
        assert UUIDScalar.serialize(test_uuid) == uuid_str
        assert UUIDScalar.serialize(uuid_str) == uuid_str

        # Test parse_value
        parsed = UUIDScalar.parse_value(uuid_str)
        assert isinstance(parsed, uuid.UUID)
        assert str(parsed) == uuid_str

        # Test parse_literal
        ast = StringValueNode(value=uuid_str)
        parsed_literal = UUIDScalar.parse_literal(ast)
        assert isinstance(parsed_literal, uuid.UUID)
        assert str(parsed_literal) == uuid_str

    def test_different_uuid_formats(self) -> None:
        """Test UUID with different valid formats."""
        # Test with uppercase
        upper_uuid = "12345678-1234-5678-1234-567812345678".upper()
        result = parse_uuid_value(upper_uuid)
        assert isinstance(result, uuid.UUID)

        # Test with lowercase (standard)
        lower_uuid = "12345678-1234-5678-1234-567812345678"
        result = parse_uuid_value(lower_uuid)
        assert isinstance(result, uuid.UUID)

    def test_nil_uuid(self) -> None:
        """Test handling of nil UUID."""
        nil_uuid = "00000000-0000-0000-0000-000000000000"
        result = parse_uuid_value(nil_uuid)
        assert isinstance(result, uuid.UUID)
        assert result == uuid.UUID(nil_uuid)
        assert serialize_uuid(result) == nil_uuid
