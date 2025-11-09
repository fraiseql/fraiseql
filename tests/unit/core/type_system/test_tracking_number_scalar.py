"""Tests for TrackingNumber scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.tracking_number import (
    TrackingNumberField,
    parse_tracking_number_literal,
    parse_tracking_number_value,
    serialize_tracking_number,
)


@pytest.mark.unit
class TestTrackingNumberSerialization:
    """Test tracking number serialization."""

    def test_serialize_valid_tracking_numbers(self) -> None:
        """Test serializing valid tracking numbers."""
        assert serialize_tracking_number("1Z999AA10123456784") == "1Z999AA10123456784"
        assert serialize_tracking_number("123456789012") == "123456789012"
        assert serialize_tracking_number("ABC123DEF456GHI789") == "ABC123DEF456GHI789"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_tracking_number(None) is None

    def test_serialize_invalid_tracking_number(self) -> None:
        """Test serializing invalid tracking numbers raises error."""
        # Too short
        with pytest.raises(GraphQLError, match="Invalid tracking number"):
            serialize_tracking_number("1234567")

        # Too long
        with pytest.raises(GraphQLError, match="Invalid tracking number"):
            serialize_tracking_number("A" * 31)

        # Contains special characters
        with pytest.raises(GraphQLError, match="Invalid tracking number"):
            serialize_tracking_number("1Z999AA1012345678!")

        # Empty
        with pytest.raises(GraphQLError, match="Invalid tracking number"):
            serialize_tracking_number("")


class TestTrackingNumberParsing:
    """Test tracking number parsing from variables."""

    def test_parse_valid_tracking_number(self) -> None:
        """Test parsing valid tracking numbers."""
        assert parse_tracking_number_value("1Z999AA10123456784") == "1Z999AA10123456784"
        assert parse_tracking_number_value("123456789012") == "123456789012"
        assert parse_tracking_number_value("ABC123DEF456GHI789") == "ABC123DEF456GHI789"

    def test_parse_invalid_tracking_number(self) -> None:
        """Test parsing invalid tracking numbers raises error."""
        with pytest.raises(GraphQLError, match="Invalid tracking number"):
            parse_tracking_number_value("1234567")

        with pytest.raises(GraphQLError, match="Invalid tracking number"):
            parse_tracking_number_value("A" * 31)

        with pytest.raises(GraphQLError, match="Invalid tracking number"):
            parse_tracking_number_value("1Z999AA1012345678!")

        with pytest.raises(GraphQLError, match="Invalid tracking number"):
            parse_tracking_number_value("")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Tracking number must be a string"):
            parse_tracking_number_value(123)

        with pytest.raises(GraphQLError, match="Tracking number must be a string"):
            parse_tracking_number_value(None)

        with pytest.raises(GraphQLError, match="Tracking number must be a string"):
            parse_tracking_number_value(["1Z999AA10123456784"])


class TestTrackingNumberField:
    """Test TrackingNumberField class."""

    def test_create_valid_tracking_number_field(self) -> None:
        """Test creating TrackingNumberField with valid values."""
        tracking = TrackingNumberField("1Z999AA10123456784")
        assert tracking == "1Z999AA10123456784"
        assert isinstance(tracking, str)

        tracking = TrackingNumberField("123456789012")
        assert tracking == "123456789012"

    def test_create_invalid_tracking_number_field(self) -> None:
        """Test creating TrackingNumberField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid tracking number"):
            TrackingNumberField("1234567")

        with pytest.raises(ValueError, match="Invalid tracking number"):
            TrackingNumberField("A" * 31)

        with pytest.raises(ValueError, match="Invalid tracking number"):
            TrackingNumberField("1Z999AA1012345678!")


class TestTrackingNumberLiteralParsing:
    """Test parsing tracking number from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid tracking number literals."""
        assert (
            parse_tracking_number_literal(StringValueNode(value="1Z999AA10123456784"))
            == "1Z999AA10123456784"
        )
        assert (
            parse_tracking_number_literal(StringValueNode(value="123456789012")) == "123456789012"
        )

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid tracking number format literals."""
        with pytest.raises(GraphQLError, match="Invalid tracking number"):
            parse_tracking_number_literal(StringValueNode(value="1234567"))

        with pytest.raises(GraphQLError, match="Invalid tracking number"):
            parse_tracking_number_literal(StringValueNode(value="A" * 31))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Tracking number must be a string"):
            parse_tracking_number_literal(IntValueNode(value="123"))
