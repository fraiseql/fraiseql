"""Tests for FlightNumber scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.flight_number import (
    FlightNumberField,
    parse_flight_number_literal,
    parse_flight_number_value,
    serialize_flight_number,
)


@pytest.mark.unit
class TestFlightNumberSerialization:
    """Test flight number serialization."""

    def test_serialize_valid_flight_numbers(self) -> None:
        """Test serializing valid IATA flight numbers."""
        assert serialize_flight_number("AA100") == "AA100"
        assert serialize_flight_number("BA2276") == "BA2276"
        assert serialize_flight_number("LH400") == "LH400"
        assert serialize_flight_number("UA1") == "UA1"
        assert serialize_flight_number("DL1234A") == "DL1234A"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_flight_number(None) is None

    def test_serialize_invalid_flight_number(self) -> None:
        """Test serializing invalid flight numbers raises error."""
        # Wrong airline code
        with pytest.raises(GraphQLError, match="Invalid flight number"):
            serialize_flight_number("A100")  # Too short airline code

        with pytest.raises(GraphQLError, match="Invalid flight number"):
            serialize_flight_number("AAA100")  # Too long airline code

        # Wrong flight number
        with pytest.raises(GraphQLError, match="Invalid flight number"):
            serialize_flight_number("AA")  # No flight number

        with pytest.raises(GraphQLError, match="Invalid flight number"):
            serialize_flight_number("AA12345")  # Too many digits

        # Invalid characters
        with pytest.raises(GraphQLError, match="Invalid flight number"):
            serialize_flight_number("AA@100")  # Invalid character

        # Empty
        with pytest.raises(GraphQLError, match="Invalid flight number"):
            serialize_flight_number("")


class TestFlightNumberParsing:
    """Test flight number parsing from variables."""

    def test_parse_valid_flight_number(self) -> None:
        """Test parsing valid flight numbers."""
        assert parse_flight_number_value("AA100") == "AA100"
        assert parse_flight_number_value("BA2276") == "BA2276"
        assert parse_flight_number_value("LH400") == "LH400"
        assert parse_flight_number_value("UA1") == "UA1"
        assert parse_flight_number_value("DL1234A") == "DL1234A"

    def test_parse_invalid_flight_number(self) -> None:
        """Test parsing invalid flight numbers raises error."""
        with pytest.raises(GraphQLError, match="Invalid flight number"):
            parse_flight_number_value("A100")

        with pytest.raises(GraphQLError, match="Invalid flight number"):
            parse_flight_number_value("AAA100")

        with pytest.raises(GraphQLError, match="Invalid flight number"):
            parse_flight_number_value("AA")

        with pytest.raises(GraphQLError, match="Invalid flight number"):
            parse_flight_number_value("AA12345")

        with pytest.raises(GraphQLError, match="Invalid flight number"):
            parse_flight_number_value("AA@100")

        with pytest.raises(GraphQLError, match="Invalid flight number"):
            parse_flight_number_value("")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Flight number must be a string"):
            parse_flight_number_value(123)

        with pytest.raises(GraphQLError, match="Flight number must be a string"):
            parse_flight_number_value(None)

        with pytest.raises(GraphQLError, match="Flight number must be a string"):
            parse_flight_number_value(["AA100"])


class TestFlightNumberField:
    """Test FlightNumberField class."""

    def test_create_valid_flight_number_field(self) -> None:
        """Test creating FlightNumberField with valid values."""
        flight = FlightNumberField("AA100")
        assert flight == "AA100"
        assert isinstance(flight, str)

        flight = FlightNumberField("BA2276")
        assert flight == "BA2276"

        flight = FlightNumberField("DL1234A")
        assert flight == "DL1234A"

    def test_create_invalid_flight_number_field(self) -> None:
        """Test creating FlightNumberField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid flight number"):
            FlightNumberField("A100")

        with pytest.raises(ValueError, match="Invalid flight number"):
            FlightNumberField("AAA100")

        with pytest.raises(ValueError, match="Invalid flight number"):
            FlightNumberField("AA")

        with pytest.raises(ValueError, match="Invalid flight number"):
            FlightNumberField("AA12345")


class TestFlightNumberLiteralParsing:
    """Test parsing flight number from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid flight number literals."""
        assert parse_flight_number_literal(StringValueNode(value="AA100")) == "AA100"
        assert parse_flight_number_literal(StringValueNode(value="BA2276")) == "BA2276"
        assert parse_flight_number_literal(StringValueNode(value="DL1234A")) == "DL1234A"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid flight number format literals."""
        with pytest.raises(GraphQLError, match="Invalid flight number"):
            parse_flight_number_literal(StringValueNode(value="A100"))

        with pytest.raises(GraphQLError, match="Invalid flight number"):
            parse_flight_number_literal(StringValueNode(value="AAA100"))

        with pytest.raises(GraphQLError, match="Invalid flight number"):
            parse_flight_number_literal(StringValueNode(value="AA"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Flight number must be a string"):
            parse_flight_number_literal(IntValueNode(value="123"))
