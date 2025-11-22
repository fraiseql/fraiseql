"""Tests for PortCode scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.port_code import (
    PortCodeField,
    parse_port_code_literal,
    parse_port_code_value,
    serialize_port_code,
)


@pytest.mark.unit
class TestPortCodeSerialization:
    """Test port code serialization."""

    def test_serialize_valid_port_codes(self) -> None:
        """Test serializing valid UN/LOCODE port codes."""
        assert serialize_port_code("USNYC") == "USNYC"
        assert serialize_port_code("CNSHA") == "CNSHA"
        assert serialize_port_code("NLRTM") == "NLRTM"
        assert serialize_port_code("GBLON") == "GBLON"
        assert serialize_port_code("usnyc") == "USNYC"  # Case normalization

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_port_code(None) is None

    def test_serialize_invalid_port_codes(self) -> None:
        """Test serializing invalid port codes raises error."""
        # Too short
        with pytest.raises(GraphQLError, match="Invalid port code"):
            serialize_port_code("USNY")

        # Too long
        with pytest.raises(GraphQLError, match="Invalid port code"):
            serialize_port_code("USNYCC")

        # Case is normalized, this should work
        assert serialize_port_code("usNYC") == "USNYC"

        # Invalid country code (numbers)
        with pytest.raises(GraphQLError, match="Invalid port code"):
            serialize_port_code("1SNYC")

        # Invalid location code (special chars)
        with pytest.raises(GraphQLError, match="Invalid port code"):
            serialize_port_code("USNY!")

        # Empty
        with pytest.raises(GraphQLError, match="Invalid port code"):
            serialize_port_code("")


class TestPortCodeParsing:
    """Test port code parsing from variables."""

    def test_parse_valid_port_codes(self) -> None:
        """Test parsing valid port codes."""
        assert parse_port_code_value("USNYC") == "USNYC"
        assert parse_port_code_value("CNSHA") == "CNSHA"
        assert parse_port_code_value("usnyc") == "USNYC"  # Case normalization

    def test_parse_invalid_port_codes(self) -> None:
        """Test parsing invalid port codes raises error."""
        with pytest.raises(GraphQLError, match="Invalid port code"):
            parse_port_code_value("USNY")

        with pytest.raises(GraphQLError, match="Invalid port code"):
            parse_port_code_value("USNYCC")

        # Case is normalized, this should work
        assert parse_port_code_value("usNYC") == "USNYC"

        with pytest.raises(GraphQLError, match="Invalid port code"):
            parse_port_code_value("USNY!")

        with pytest.raises(GraphQLError, match="Invalid port code"):
            parse_port_code_value("")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Port code must be a string"):
            parse_port_code_value(123)

        with pytest.raises(GraphQLError, match="Port code must be a string"):
            parse_port_code_value(None)

        with pytest.raises(GraphQLError, match="Port code must be a string"):
            parse_port_code_value(["USNYC"])


class TestPortCodeField:
    """Test PortCodeField class."""

    def test_create_valid_port_code_field(self) -> None:
        """Test creating PortCodeField with valid values."""
        port_code = PortCodeField("USNYC")
        assert port_code == "USNYC"
        assert isinstance(port_code, str)

        # Case normalization
        port_code = PortCodeField("usnyc")
        assert port_code == "USNYC"

    def test_create_invalid_port_code_field(self) -> None:
        """Test creating PortCodeField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid port code"):
            PortCodeField("USNY")

        with pytest.raises(ValueError, match="Invalid port code"):
            PortCodeField("USNYCC")

        # Case is normalized, this should work
        port_code = PortCodeField("usNYC")
        assert port_code == "USNYC"

        with pytest.raises(ValueError, match="Invalid port code"):
            PortCodeField("USNY!")


class TestPortCodeLiteralParsing:
    """Test parsing port code from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid port code literals."""
        assert parse_port_code_literal(StringValueNode(value="USNYC")) == "USNYC"
        assert parse_port_code_literal(StringValueNode(value="CNSHA")) == "CNSHA"
        assert parse_port_code_literal(StringValueNode(value="usnyc")) == "USNYC"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid port code format literals."""
        with pytest.raises(GraphQLError, match="Invalid port code"):
            parse_port_code_literal(StringValueNode(value="USNY"))

        with pytest.raises(GraphQLError, match="Invalid port code"):
            parse_port_code_literal(StringValueNode(value="USNYCC"))

        # Case is normalized, this should work
        assert parse_port_code_literal(StringValueNode(value="usNYC")) == "USNYC"

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Port code must be a string"):
            parse_port_code_literal(IntValueNode(value="123"))
