"""Tests for IBAN scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.iban import (
    IBANField,
    parse_iban_literal,
    parse_iban_value,
    serialize_iban,
)


@pytest.mark.unit
class TestIBANSerialization:
    """Test IBAN serialization."""

    def test_serialize_valid_ibans(self) -> None:
        """Test serializing valid ISO 13616 IBANs."""
        # Valid IBANs with correct check digits
        assert serialize_iban("GB82WEST12345698765432") == "GB82WEST12345698765432"
        assert serialize_iban("DE89370400440532013000") == "DE89370400440532013000"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_iban(None) is None

    def test_serialize_invalid_iban(self) -> None:
        """Test serializing invalid IBANs raises error."""
        # Wrong format
        with pytest.raises(GraphQLError, match="Invalid IBAN"):
            serialize_iban("INVALID123")

        # Wrong length
        with pytest.raises(GraphQLError, match="Invalid IBAN"):
            serialize_iban("GB82WEST1234569876543")  # Too short

        with pytest.raises(GraphQLError, match="Invalid IBAN"):
            serialize_iban("GB82WEST123456987654321")  # Too long

        # Invalid country code
        with pytest.raises(GraphQLError, match="Invalid IBAN"):
            serialize_iban("ZZ82WEST12345698765432")

        # Wrong check digits
        with pytest.raises(GraphQLError, match="Invalid IBAN"):
            serialize_iban("GB29WEST12345698765432")  # Should be 82

        # Invalid characters
        with pytest.raises(GraphQLError, match="Invalid IBAN"):
            serialize_iban("GB82WEST1234569876543!")

        # Empty
        with pytest.raises(GraphQLError, match="Invalid IBAN"):
            serialize_iban("")


class TestIBANParsing:
    """Test IBAN parsing from variables."""

    def test_parse_valid_iban(self) -> None:
        """Test parsing valid IBANs."""
        assert parse_iban_value("GB82WEST12345698765432") == "GB82WEST12345698765432"
        assert parse_iban_value("DE89370400440532013000") == "DE89370400440532013000"

    def test_parse_invalid_iban(self) -> None:
        """Test parsing invalid IBANs raises error."""
        with pytest.raises(GraphQLError, match="Invalid IBAN"):
            parse_iban_value("INVALID123")

        with pytest.raises(GraphQLError, match="Invalid IBAN"):
            parse_iban_value("GB82WEST1234569876543")  # Wrong length

        with pytest.raises(GraphQLError, match="Invalid IBAN"):
            parse_iban_value("ZZ82WEST12345698765432")  # Invalid country

        with pytest.raises(GraphQLError, match="Invalid IBAN"):
            parse_iban_value("GB29WEST12345698765432")  # Wrong check digits

        with pytest.raises(GraphQLError, match="Invalid IBAN"):
            parse_iban_value("")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="IBAN must be a string"):
            parse_iban_value(123)

        with pytest.raises(GraphQLError, match="IBAN must be a string"):
            parse_iban_value(None)

        with pytest.raises(GraphQLError, match="IBAN must be a string"):
            parse_iban_value(["GB82WEST12345698765432"])


class TestIBANField:
    """Test IBANField class."""

    def test_create_valid_iban_field(self) -> None:
        """Test creating IBANField with valid values."""
        iban = IBANField("GB82WEST12345698765432")
        assert iban == "GB82WEST12345698765432"
        assert isinstance(iban, str)

        iban = IBANField("DE89370400440532013000")
        assert iban == "DE89370400440532013000"

    def test_create_invalid_iban_field(self) -> None:
        """Test creating IBANField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid IBAN"):
            IBANField("INVALID123")

        with pytest.raises(ValueError, match="Invalid IBAN"):
            IBANField("GB29WEST12345698765432")  # Wrong check digits


class TestIBANLiteralParsing:
    """Test parsing IBAN from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid IBAN literals."""
        assert (
            parse_iban_literal(StringValueNode(value="GB82WEST12345698765432"))
            == "GB82WEST12345698765432"
        )
        assert (
            parse_iban_literal(StringValueNode(value="DE89370400440532013000"))
            == "DE89370400440532013000"
        )

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid IBAN format literals."""
        with pytest.raises(GraphQLError, match="Invalid IBAN"):
            parse_iban_literal(StringValueNode(value="INVALID123"))

        with pytest.raises(GraphQLError, match="Invalid IBAN"):
            parse_iban_literal(StringValueNode(value="GB29WEST12345698765432"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="IBAN must be a string"):
            parse_iban_literal(IntValueNode(value="123"))
