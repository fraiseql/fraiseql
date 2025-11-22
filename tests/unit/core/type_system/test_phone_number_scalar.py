"""Tests for PhoneNumber scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.phone_number import (
    PhoneNumberField,
    parse_phone_number_literal,
    parse_phone_number_value,
    serialize_phone_number,
)


@pytest.mark.unit
class TestPhoneNumberSerialization:
    """Test phone number serialization."""

    def test_serialize_valid_phone_numbers(self) -> None:
        """Test serializing valid E.164 phone numbers."""
        assert serialize_phone_number("+1234567890") == "+1234567890"
        assert serialize_phone_number("+447911123456") == "+447911123456"
        assert serialize_phone_number("+15551234567") == "+15551234567"
        assert serialize_phone_number("+33123456789") == "+33123456789"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_phone_number(None) is None

    def test_serialize_invalid_phone_number(self) -> None:
        """Test serializing invalid phone numbers raises error."""
        # Missing +
        with pytest.raises(GraphQLError, match="Invalid phone number"):
            serialize_phone_number("1234567890")

        # Too short (minimum 7 digits after +)
        with pytest.raises(GraphQLError, match="Invalid phone number"):
            serialize_phone_number("+123456")

        # Too long (maximum 15 digits total)
        with pytest.raises(GraphQLError, match="Invalid phone number"):
            serialize_phone_number("+1234567890123456")

        # Invalid country code (must start with 1-9)
        with pytest.raises(GraphQLError, match="Invalid phone number"):
            serialize_phone_number("+0234567890")

        # Contains letters
        with pytest.raises(GraphQLError, match="Invalid phone number"):
            serialize_phone_number("+123456789A")

        # Contains spaces
        with pytest.raises(GraphQLError, match="Invalid phone number"):
            serialize_phone_number("+1 234 567 890")

        # Empty
        with pytest.raises(GraphQLError, match="Invalid phone number"):
            serialize_phone_number("")


class TestPhoneNumberParsing:
    """Test phone number parsing from variables."""

    def test_parse_valid_phone_number(self) -> None:
        """Test parsing valid phone numbers."""
        assert parse_phone_number_value("+1234567890") == "+1234567890"
        assert parse_phone_number_value("+447911123456") == "+447911123456"
        assert parse_phone_number_value("+15551234567") == "+15551234567"

    def test_parse_invalid_phone_number(self) -> None:
        """Test parsing invalid phone numbers raises error."""
        with pytest.raises(GraphQLError, match="Invalid phone number"):
            parse_phone_number_value("1234567890")

        with pytest.raises(GraphQLError, match="Invalid phone number"):
            parse_phone_number_value("+123456")

        with pytest.raises(GraphQLError, match="Invalid phone number"):
            parse_phone_number_value("+1234567890123456")

        with pytest.raises(GraphQLError, match="Invalid phone number"):
            parse_phone_number_value("+0234567890")

        with pytest.raises(GraphQLError, match="Invalid phone number"):
            parse_phone_number_value("")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Phone number must be a string"):
            parse_phone_number_value(1234567890)

        with pytest.raises(GraphQLError, match="Phone number must be a string"):
            parse_phone_number_value(None)

        with pytest.raises(GraphQLError, match="Phone number must be a string"):
            parse_phone_number_value(["+1234567890"])


class TestPhoneNumberField:
    """Test PhoneNumberField class."""

    def test_create_valid_phone_number_field(self) -> None:
        """Test creating PhoneNumberField with valid values."""
        phone = PhoneNumberField("+1234567890")
        assert phone == "+1234567890"
        assert isinstance(phone, str)

        phone = PhoneNumberField("+447911123456")
        assert phone == "+447911123456"

    def test_create_invalid_phone_number_field(self) -> None:
        """Test creating PhoneNumberField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid phone number"):
            PhoneNumberField("1234567890")

        with pytest.raises(ValueError, match="Invalid phone number"):
            PhoneNumberField("+123456")

        with pytest.raises(ValueError, match="Invalid phone number"):
            PhoneNumberField("+1234567890123456")


class TestPhoneNumberLiteralParsing:
    """Test parsing phone number from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid phone number literals."""
        assert parse_phone_number_literal(StringValueNode(value="+1234567890")) == "+1234567890"
        assert parse_phone_number_literal(StringValueNode(value="+447911123456")) == "+447911123456"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid phone number format literals."""
        with pytest.raises(GraphQLError, match="Invalid phone number"):
            parse_phone_number_literal(StringValueNode(value="1234567890"))

        with pytest.raises(GraphQLError, match="Invalid phone number"):
            parse_phone_number_literal(StringValueNode(value="+123456"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Phone number must be a string"):
            parse_phone_number_literal(IntValueNode(value="123"))
