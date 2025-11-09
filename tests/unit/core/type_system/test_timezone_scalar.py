"""Tests for Timezone scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.timezone import (
    TimezoneField,
    parse_timezone_literal,
    parse_timezone_value,
    serialize_timezone,
)


@pytest.mark.unit
class TestTimezoneSerialization:
    """Test timezone serialization."""

    def test_serialize_valid_timezones(self) -> None:
        """Test serializing valid IANA timezone identifiers."""
        assert serialize_timezone("America/New_York") == "America/New_York"
        assert serialize_timezone("Europe/Paris") == "Europe/Paris"
        assert serialize_timezone("Asia/Tokyo") == "Asia/Tokyo"
        assert serialize_timezone("Pacific/Auckland") == "Pacific/Auckland"
        assert serialize_timezone("America/Los_Angeles") == "America/Los_Angeles"
        assert serialize_timezone("Europe/London") == "Europe/London"
        assert serialize_timezone("Australia/Sydney") == "Australia/Sydney"

    def test_serialize_three_part_timezones(self) -> None:
        """Test serializing timezones with three parts (Region/City/Locality)."""
        assert (
            serialize_timezone("America/Argentina/Buenos_Aires") == "America/Argentina/Buenos_Aires"
        )
        assert serialize_timezone("America/Indiana/Indianapolis") == "America/Indiana/Indianapolis"
        assert serialize_timezone("America/Kentucky/Louisville") == "America/Kentucky/Louisville"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_timezone(None) is None

    def test_serialize_invalid_timezone(self) -> None:
        """Test serializing invalid timezones raises error."""
        # Abbreviations not supported
        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("EST")

        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("PST")

        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("UTC")

        # Offsets not supported
        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("UTC+5")

        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("GMT-8")

        # Wrong capitalization
        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("america/new_york")

        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("AMERICA/NEW_YORK")

        # Missing slash
        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("NewYork")

        # Too many parts
        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("America/USA/New_York/Manhattan")

        # Empty
        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("")


class TestTimezoneParsing:
    """Test timezone parsing from variables."""

    def test_parse_valid_timezone(self) -> None:
        """Test parsing valid timezones."""
        assert parse_timezone_value("America/New_York") == "America/New_York"
        assert parse_timezone_value("Europe/Paris") == "Europe/Paris"
        assert (
            parse_timezone_value("America/Argentina/Buenos_Aires")
            == "America/Argentina/Buenos_Aires"
        )

    def test_parse_invalid_timezone(self) -> None:
        """Test parsing invalid timezones raises error."""
        with pytest.raises(GraphQLError, match="Invalid timezone"):
            parse_timezone_value("EST")

        with pytest.raises(GraphQLError, match="Invalid timezone"):
            parse_timezone_value("UTC+5")

        with pytest.raises(GraphQLError, match="Invalid timezone"):
            parse_timezone_value("america/new_york")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Timezone must be a string"):
            parse_timezone_value(123)

        with pytest.raises(GraphQLError, match="Timezone must be a string"):
            parse_timezone_value(None)

        with pytest.raises(GraphQLError, match="Timezone must be a string"):
            parse_timezone_value(["America/New_York"])


class TestTimezoneField:
    """Test TimezoneField class."""

    def test_create_valid_timezone_field(self) -> None:
        """Test creating TimezoneField with valid values."""
        tz = TimezoneField("America/New_York")
        assert tz == "America/New_York"
        assert isinstance(tz, str)

        tz = TimezoneField("Europe/Paris")
        assert tz == "Europe/Paris"

        tz = TimezoneField("America/Argentina/Buenos_Aires")
        assert tz == "America/Argentina/Buenos_Aires"

    def test_create_invalid_timezone_field(self) -> None:
        """Test creating TimezoneField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid timezone"):
            TimezoneField("EST")

        with pytest.raises(ValueError, match="Invalid timezone"):
            TimezoneField("UTC+5")

        with pytest.raises(ValueError, match="Invalid timezone"):
            TimezoneField("america/new_york")


class TestTimezoneLiteralParsing:
    """Test parsing timezone from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid timezone literals."""
        assert (
            parse_timezone_literal(StringValueNode(value="America/New_York")) == "America/New_York"
        )
        assert parse_timezone_literal(StringValueNode(value="Europe/Paris")) == "Europe/Paris"
        assert (
            parse_timezone_literal(StringValueNode(value="America/Argentina/Buenos_Aires"))
            == "America/Argentina/Buenos_Aires"
        )

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid timezone format literals."""
        with pytest.raises(GraphQLError, match="Invalid timezone"):
            parse_timezone_literal(StringValueNode(value="EST"))

        with pytest.raises(GraphQLError, match="Invalid timezone"):
            parse_timezone_literal(StringValueNode(value="america/new_york"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Timezone must be a string"):
            parse_timezone_literal(IntValueNode(value="123"))
