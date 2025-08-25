from datetime import UTC, datetime

import pytest
from graphql import GraphQLError
from graphql.language import StringValueNode

from fraiseql.types.scalars.datetime import (
    parse_datetime_literal,
    parse_datetime_value,
    serialize_datetime,
)


@pytest.mark.unit
def test_serialize_datetime() -> None:
    # Test serializing datetime to ISO 8601 string
    dt = datetime(2023, 1, 1, 12, 0, 0, tzinfo=UTC)
    assert serialize_datetime(dt) == "2023-01-01T12:00:00Z"

    dt = datetime(2023, 1, 1, 12, 0, 0, tzinfo=UTC)
    assert serialize_datetime(dt) == "2023-01-01T12:00:00Z"


def test_serialize_datetime_string() -> None:
    # Test serializing ISO datetime strings - all should be converted to UTC with Z
    assert serialize_datetime("2023-01-01T12:00:00Z") == "2023-01-01T12:00:00Z"
    assert serialize_datetime("2023-01-01T12:00:00+00:00") == "2023-01-01T12:00:00Z"
    assert serialize_datetime("2023-01-01T12:00:00-05:00") == "2023-01-01T17:00:00Z"  # UTC


def test_serialize_invalid_datetime_string() -> None:
    # Test serializing invalid datetime strings
    with pytest.raises(GraphQLError, match="DateTime cannot represent invalid ISO datetime string"):
        serialize_datetime("not a datetime")

    # Timezone-naive strings should fail
    with pytest.raises(GraphQLError, match="DateTime cannot represent invalid ISO datetime string"):
        serialize_datetime("2023-01-01T12:00:00")


def test_serialize_non_datetime() -> None:
    # Test serializing non-datetime, non-string value
    with pytest.raises(GraphQLError, match="DateTime cannot represent non-datetime value"):
        serialize_datetime(12345)  # type: ignore[arg-type]


def test_parse_datetime_value() -> None:
    # Test parsing valid ISO 8601 strings with various timezone notations
    assert parse_datetime_value("2023-01-01T12:00:00Z") == datetime(
        2023, 1, 1, 12, 0, 0, tzinfo=UTC
    )
    assert parse_datetime_value("2023-01-01T12:00:00+00:00") == datetime(
        2023, 1, 1, 12, 0, 0, tzinfo=UTC
    )
    assert parse_datetime_value("2023-01-01T12:00:00+02:00") == datetime(
        2023, 1, 1, 10, 0, 0, tzinfo=UTC
    )  # UTC equivalent
    assert parse_datetime_value("2023-01-01T12:00:00-05:00") == datetime(
        2023, 1, 1, 17, 0, 0, tzinfo=UTC
    )  # UTC equivalent


def test_parse_invalid_datetime_value() -> None:
    # Test parsing invalid ISO 8601 strings
    with pytest.raises(GraphQLError):
        parse_datetime_value("2023-01-01T12:00:00")
    with pytest.raises(GraphQLError):
        parse_datetime_value("not a datetime")


def test_parse_none_datetime_value() -> None:
    # Test parsing None value
    assert parse_datetime_value(None) is None


def test_parse_datetime_literal() -> None:
    # Test parsing a DateTime literal from GraphQL AST
    ast = StringValueNode(value="2023-01-01T12:00:00Z")
    assert parse_datetime_literal(ast) == datetime(2023, 1, 1, 12, 0, 0, tzinfo=UTC)


def test_parse_invalid_datetime_literal() -> None:
    # Test parsing invalid DateTime literal from GraphQL AST
    ast = StringValueNode(value="not a datetime")
    with pytest.raises(GraphQLError):
        parse_datetime_literal(ast)


def test_serialize_jsonb_datetime_string() -> None:
    """Test serializing datetime strings from PostgreSQL JSONB columns.

    When PostgreSQL stores timestamps in JSONB columns, they are automatically
    converted to ISO strings. This test ensures FraiseQL normalizes these
    to UTC with Z suffix for JavaScript compatibility.
    """
    # Common formats from PostgreSQL JSONB - all normalized to UTC with Z
    jsonb_datetime_z = "2025-01-09T14:30:00Z"
    result = serialize_datetime(jsonb_datetime_z)
    assert result == "2025-01-09T14:30:00Z"

    jsonb_datetime_offset = "2025-01-09T14:30:00+02:00"
    result = serialize_datetime(jsonb_datetime_offset)
    assert result == "2025-01-09T12:30:00Z"  # Converted to UTC

    # PostgreSQL sometimes uses +00:00 instead of Z - normalize to Z
    jsonb_datetime_utc = "2025-01-09T14:30:00+00:00"
    result = serialize_datetime(jsonb_datetime_utc)
    assert result == "2025-01-09T14:30:00Z"

    # Invalid datetime string should raise error
    with pytest.raises(GraphQLError, match="DateTime cannot represent invalid ISO datetime string"):
        serialize_datetime("not-a-datetime")

    # Timezone-naive datetime should raise error (FraiseQL requires timezone)
    with pytest.raises(GraphQLError, match="DateTime cannot represent invalid ISO datetime string"):
        serialize_datetime("2025-01-09T14:30:00")


def test_datetime_utc_normalization() -> None:
    """Test that all datetime values are normalized to UTC with Z suffix."""
    from zoneinfo import ZoneInfo

    # Test various timezone inputs all normalize to UTC with Z
    test_cases = [
        # Input datetime, expected UTC output
        ("2025-05-01T14:00:00+02:00", "2025-05-01T12:00:00Z"),  # CEST to UTC
        ("2025-05-01T12:00:00+00:00", "2025-05-01T12:00:00Z"),  # UTC with offset to Z
        ("2025-05-01T12:00:00Z", "2025-05-01T12:00:00Z"),  # Already Z format
        ("2025-05-01T07:00:00-05:00", "2025-05-01T12:00:00Z"),  # EST to UTC
        ("2025-05-01T21:00:00+09:00", "2025-05-01T12:00:00Z"),  # JST to UTC
    ]

    for input_str, expected in test_cases:
        assert serialize_datetime(input_str) == expected

    # Test with datetime objects
    dt_utc = datetime(2025, 5, 1, 12, 0, 0, tzinfo=UTC)
    assert serialize_datetime(dt_utc) == "2025-05-01T12:00:00Z"

    # Test with other timezones (requires zoneinfo or pytz)
    try:
        dt_paris = datetime(2025, 5, 1, 14, 0, 0, tzinfo=ZoneInfo("Europe/Paris"))
        assert serialize_datetime(dt_paris) == "2025-05-01T12:00:00Z"

        dt_ny = datetime(2025, 5, 1, 8, 0, 0, tzinfo=ZoneInfo("America/New_York"))
        assert serialize_datetime(dt_ny) == "2025-05-01T12:00:00Z"
    except ImportError:
        # Skip timezone tests if zoneinfo not available
        pass

    # Test naive datetime (assumes UTC)
    dt_naive = datetime(2025, 5, 1, 12, 0, 0, tzinfo=None)
    assert serialize_datetime(dt_naive) == "2025-05-01T12:00:00Z"
