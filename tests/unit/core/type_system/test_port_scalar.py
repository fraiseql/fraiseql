"""Tests for Port scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.port import (
    PortField,
    parse_port_literal,
    parse_port_value,
    serialize_port,
)


@pytest.mark.unit
class TestPortSerialization:
    """Test port serialization."""

    def test_serialize_valid_port(self) -> None:
        """Test serializing valid port numbers."""
        assert serialize_port(80) == 80
        assert serialize_port(443) == 443
        assert serialize_port(8080) == 8080
        assert serialize_port(1) == 1
        assert serialize_port(65535) == 65535

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_port(None) is None

    def test_serialize_invalid_port_range(self) -> None:
        """Test serializing out-of-range ports raises error."""
        with pytest.raises(GraphQLError, match="Port must be between 1 and 65535"):
            serialize_port(0)

        with pytest.raises(GraphQLError, match="Port must be between 1 and 65535"):
            serialize_port(-1)

        with pytest.raises(GraphQLError, match="Port must be between 1 and 65535"):
            serialize_port(65536)

        with pytest.raises(GraphQLError, match="Port must be between 1 and 65535"):
            serialize_port(100000)

    def test_serialize_string_conversion(self) -> None:
        """Test serializing string representation of integers."""
        assert serialize_port("80") == 80
        assert serialize_port("443") == 443


class TestPortParsing:
    """Test port parsing from variables."""

    def test_parse_valid_port(self) -> None:
        """Test parsing valid port numbers."""
        assert parse_port_value(80) == 80
        assert parse_port_value(443) == 443
        assert parse_port_value(8080) == 8080
        assert parse_port_value(1) == 1
        assert parse_port_value(65535) == 65535

    def test_parse_invalid_port_range(self) -> None:
        """Test parsing out-of-range ports raises error."""
        with pytest.raises(GraphQLError, match="Port must be between 1 and 65535"):
            parse_port_value(0)

        with pytest.raises(GraphQLError, match="Port must be between 1 and 65535"):
            parse_port_value(-1)

        with pytest.raises(GraphQLError, match="Port must be between 1 and 65535"):
            parse_port_value(65536)

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-integer types raises error."""
        with pytest.raises(GraphQLError, match="Port must be an integer"):
            parse_port_value("80")

        with pytest.raises(GraphQLError, match="Port must be an integer"):
            parse_port_value(80.5)

        with pytest.raises(GraphQLError, match="Port must be an integer"):
            parse_port_value(None)


class TestPortField:
    """Test PortField class."""

    def test_create_valid_port_field(self) -> None:
        """Test creating PortField with valid values."""
        port = PortField(80)
        assert port == 80
        assert isinstance(port, int)

        port = PortField(65535)
        assert port == 65535

    def test_create_invalid_port_field(self) -> None:
        """Test creating PortField with invalid values raises error."""
        with pytest.raises(ValueError, match="Port must be between 1 and 65535"):
            PortField(0)

        with pytest.raises(ValueError, match="Port must be between 1 and 65535"):
            PortField(-1)

        with pytest.raises(ValueError, match="Port must be between 1 and 65535"):
            PortField(65536)


class TestPortLiteralParsing:
    """Test parsing port from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid port literals."""
        assert parse_port_literal(IntValueNode(value="80")) == 80
        assert parse_port_literal(IntValueNode(value="443")) == 443
        assert parse_port_literal(IntValueNode(value="1")) == 1
        assert parse_port_literal(IntValueNode(value="65535")) == 65535

    def test_parse_invalid_literal_range(self) -> None:
        """Test parsing out-of-range port literals."""
        with pytest.raises(GraphQLError, match="Port must be between 1 and 65535"):
            parse_port_literal(IntValueNode(value="0"))

        with pytest.raises(GraphQLError, match="Port must be between 1 and 65535"):
            parse_port_literal(IntValueNode(value="65536"))

    def test_parse_non_int_literal(self) -> None:
        """Test parsing non-integer literals."""
        with pytest.raises(GraphQLError, match="Port must be an integer"):
            parse_port_literal(StringValueNode(value="80"))
