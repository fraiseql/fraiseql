"""Tests for PostalCode scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.postal_code import (
    PostalCodeField,
    parse_postal_code_literal,
    parse_postal_code_value,
    serialize_postal_code,
)


@pytest.mark.unit
class TestPostalCodeSerialization:
    """Test postal code serialization."""

    def test_serialize_valid_postal_codes(self) -> None:
        """Test serializing valid postal codes."""
        assert serialize_postal_code("90210") == "90210"
        assert serialize_postal_code("SW1A 1AA") == "SW1A 1AA"
        assert serialize_postal_code("75001") == "75001"
        assert serialize_postal_code("100-0001") == "100-0001"
        assert serialize_postal_code("12345-6789") == "12345-6789"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_postal_code(None) is None

    def test_serialize_invalid_postal_code(self) -> None:
        """Test serializing invalid postal codes raises error."""
        # Too short (less than 3 characters)
        with pytest.raises(GraphQLError, match="Invalid postal code"):
            serialize_postal_code("12")

        # Too long (more than 10 characters)
        with pytest.raises(GraphQLError, match="Invalid postal code"):
            serialize_postal_code("12345678901")

        # Empty string
        with pytest.raises(GraphQLError, match="Invalid postal code"):
            serialize_postal_code("")

        # Special characters not allowed
        with pytest.raises(GraphQLError, match="Invalid postal code"):
            serialize_postal_code("123@45")


class TestPostalCodeParsing:
    """Test postal code parsing from variables."""

    def test_parse_valid_postal_code(self) -> None:
        """Test parsing valid postal codes."""
        assert parse_postal_code_value("90210") == "90210"
        assert parse_postal_code_value("SW1A 1AA") == "SW1A 1AA"
        assert parse_postal_code_value("75001") == "75001"

    def test_parse_invalid_postal_code(self) -> None:
        """Test parsing invalid postal codes raises error."""
        # Too short
        with pytest.raises(GraphQLError, match="Invalid postal code"):
            parse_postal_code_value("12")

        # Too long
        with pytest.raises(GraphQLError, match="Invalid postal code"):
            parse_postal_code_value("12345678901")

        # Empty string
        with pytest.raises(GraphQLError, match="Invalid postal code"):
            parse_postal_code_value("")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Postal code must be a string"):
            parse_postal_code_value(12345)

        with pytest.raises(GraphQLError, match="Postal code must be a string"):
            parse_postal_code_value(None)

        with pytest.raises(GraphQLError, match="Postal code must be a string"):
            parse_postal_code_value(["90210"])


class TestPostalCodeField:
    """Test PostalCodeField class."""

    def test_create_valid_postal_code_field(self) -> None:
        """Test creating PostalCodeField with valid values."""
        postal_code = PostalCodeField("90210")
        assert postal_code == "90210"
        assert isinstance(postal_code, str)

        postal_code = PostalCodeField("SW1A 1AA")
        assert postal_code == "SW1A 1AA"

    def test_create_invalid_postal_code_field(self) -> None:
        """Test creating PostalCodeField with invalid values raises error."""
        # Too short
        with pytest.raises(ValueError, match="Invalid postal code"):
            PostalCodeField("12")

        # Too long
        with pytest.raises(ValueError, match="Invalid postal code"):
            PostalCodeField("12345678901")

        # Empty string
        with pytest.raises(ValueError, match="Invalid postal code"):
            PostalCodeField("")


class TestPostalCodeLiteralParsing:
    """Test parsing postal code from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid postal code literals."""
        assert parse_postal_code_literal(StringValueNode(value="90210")) == "90210"
        assert parse_postal_code_literal(StringValueNode(value="SW1A 1AA")) == "SW1A 1AA"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid postal code format literals."""
        # Too short
        with pytest.raises(GraphQLError, match="Invalid postal code"):
            parse_postal_code_literal(StringValueNode(value="12"))

        # Too long
        with pytest.raises(GraphQLError, match="Invalid postal code"):
            parse_postal_code_literal(StringValueNode(value="12345678901"))

        # Empty string
        with pytest.raises(GraphQLError, match="Invalid postal code"):
            parse_postal_code_literal(StringValueNode(value=""))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Postal code must be a string"):
            parse_postal_code_literal(IntValueNode(value="123"))
