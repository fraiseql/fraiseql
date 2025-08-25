from datetime import date

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.date import (
    DateScalar,
    parse_date_literal,
    parse_date_value,
    serialize_date,
)


@pytest.mark.unit
class TestDateScalar:
    """Test suite for Date scalar type."""

    def test_serialize_date_object(self) -> None:
        """Test serializing date object to ISO format string."""
        test_date = date(2023, 12, 25)
        result = serialize_date(test_date)
        assert result == "2023-12-25"

    def test_serialize_date_string(self) -> None:
        """Test serializing valid date string returns the same string."""
        date_str = "2023-12-25"
        result = serialize_date(date_str)
        assert result == "2023-12-25"

    def test_serialize_invalid_date_string(self) -> None:
        """Test serializing invalid date string raises error."""
        with pytest.raises(GraphQLError, match="Date cannot represent invalid ISO date string"):
            serialize_date("2023-13-45")  # Invalid month and day

    def test_serialize_non_date_type(self) -> None:
        """Test serializing non-date type raises error."""
        with pytest.raises(GraphQLError, match="Date cannot represent non-date value"):
            serialize_date(12345)

    def test_parse_date_value_from_string(self) -> None:
        """Test parsing date from ISO format string."""
        date_str = "2023-12-25"
        result = parse_date_value(date_str)
        assert isinstance(result, date)
        assert result == date(2023, 12, 25)

    def test_parse_date_value_from_date_object(self) -> None:
        """Test parsing date from date object raises error."""
        test_date = date(2023, 12, 25)
        with pytest.raises(GraphQLError, match="Date cannot represent non-string value"):
            parse_date_value(test_date)

    def test_parse_date_value_invalid_format(self) -> None:
        """Test parsing invalid date format raises error."""
        with pytest.raises(GraphQLError, match="Invalid ISO 8601 Date"):
            parse_date_value("25/12/2023")  # Wrong format

    def test_parse_date_value_invalid_date(self) -> None:
        """Test parsing invalid date raises error."""
        with pytest.raises(GraphQLError, match="Invalid ISO 8601 Date"):
            parse_date_value("2023-02-30")  # February 30th doesn't exist

    def test_parse_date_value_non_string(self) -> None:
        """Test parsing non-string value raises error."""
        with pytest.raises(GraphQLError, match="Date cannot represent non-string value"):
            parse_date_value(20231225)

    def test_parse_date_value_none(self) -> None:
        """Test parsing None value."""
        assert parse_date_value(None) is None

    def test_parse_date_literal_valid(self) -> None:
        """Test parsing date literal from AST."""
        date_str = "2023-12-25"
        ast = StringValueNode(value=date_str)
        result = parse_date_literal(ast)
        assert isinstance(result, date)
        assert result == date(2023, 12, 25)

    def test_parse_date_literal_invalid_format(self) -> None:
        """Test parsing invalid date literal raises error."""
        ast = StringValueNode(value="12/25/2023")
        with pytest.raises(GraphQLError, match="Invalid ISO 8601 Date"):
            parse_date_literal(ast)

    def test_parse_date_literal_non_string_node(self) -> None:
        """Test parsing non-string AST node raises error."""
        ast = IntValueNode(value="20231225")
        with pytest.raises(GraphQLError, match="Date cannot represent non-string literal"):
            parse_date_literal(ast)

    def test_date_scalar_integration(self) -> None:
        """Test DateScalar scalar integration."""
        test_date = date(2023, 12, 25)
        date_str = "2023-12-25"

        # Test serialize - accepts both date objects and ISO strings
        assert DateScalar.serialize(test_date) == date_str
        assert DateScalar.serialize(date_str) == date_str

        # Test parse_value
        parsed = DateScalar.parse_value(date_str)
        assert isinstance(parsed, date)
        assert parsed == test_date

        # Test parse_literal
        ast = StringValueNode(value=date_str)
        parsed_literal = DateScalar.parse_literal(ast)
        assert isinstance(parsed_literal, date)
        assert parsed_literal == test_date

    def test_leap_year_dates(self) -> None:
        """Test handling of leap year dates."""
        # Valid leap year date
        leap_date = "2024-02-29"
        result = parse_date_value(leap_date)
        assert result == date(2024, 2, 29)

        # Invalid non-leap year date
        with pytest.raises(GraphQLError, match="Invalid ISO 8601 Date"):
            parse_date_value("2023-02-29")

    def test_edge_case_dates(self) -> None:
        """Test edge case dates."""
        # First day of year
        result = parse_date_value("2023-01-01")
        assert result == date(2023, 1, 1)

        # Last day of year
        result = parse_date_value("2023-12-31")
        assert result == date(2023, 12, 31)

        # Minimum date (year 1)
        result = parse_date_value("0001-01-01")
        assert result == date(1, 1, 1)

    def test_date_with_time_component(self) -> None:
        """Test that date strings with time components are rejected."""
        with pytest.raises(GraphQLError, match="Invalid ISO 8601 Date"):
            parse_date_value("2023-12-25T10:30:00")

        with pytest.raises(GraphQLError, match="Invalid ISO 8601 Date"):
            parse_date_value("2023-12-25 10:30:00")

    def test_serialize_jsonb_date_string(self) -> None:
        """Test serializing date strings from PostgreSQL JSONB columns.

        When PostgreSQL stores dates in JSONB columns, they are automatically
        converted to ISO strings. This test ensures FraiseQL can handle these
        pre-serialized dates from database views.
        """
        # Simulate date string from JSONB column
        jsonb_date = "2025-01-09"
        result = serialize_date(jsonb_date)
        assert result == "2025-01-09"

        # Invalid date string should still raise error
        with pytest.raises(GraphQLError, match="Date cannot represent invalid ISO date string"):
            serialize_date("not-a-date")

        # Malformed date should raise error
        with pytest.raises(GraphQLError, match="Date cannot represent invalid ISO date string"):
            serialize_date("2025-13-01")  # Invalid month
