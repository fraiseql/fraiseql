from datetime import UTC, datetime

import pytest
from graphql import GraphQLError
from graphql.language import StringValueNode

from fraiseql.types.scalars.datetime import (
    parse_datetime_literal,
    parse_datetime_value,
    serialize_datetime,
)


def test_serialize_datetime() -> None:
    # Test serializing datetime to ISO 8601 string
    dt = datetime(2023, 1, 1, 12, 0, 0, tzinfo=UTC)
    assert serialize_datetime(dt) == "2023-01-01T12:00:00Z"

    dt = datetime(2023, 1, 1, 12, 0, 0, tzinfo=UTC)
    assert serialize_datetime(dt) == "2023-01-01T12:00:00Z"


def test_serialize_datetime_string() -> None:
    # Test serializing valid ISO datetime strings (from JSONB)
    assert serialize_datetime("2023-01-01T12:00:00Z") == "2023-01-01T12:00:00Z"
    assert serialize_datetime("2023-01-01T12:00:00+00:00") == "2023-01-01T12:00:00+00:00"
    assert serialize_datetime("2023-01-01T12:00:00-05:00") == "2023-01-01T12:00:00-05:00"


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
        2023,
        1,
        1,
        12,
        0,
        0,
        tzinfo=UTC,
    )
    assert parse_datetime_value("2023-01-01T12:00:00+00:00") == datetime(
        2023,
        1,
        1,
        12,
        0,
        0,
        tzinfo=UTC,
    )
    assert parse_datetime_value("2023-01-01T12:00:00+02:00") == datetime(
        2023,
        1,
        1,
        10,
        0,
        0,
        tzinfo=UTC,
    )  # UTC equivalent
    assert parse_datetime_value("2023-01-01T12:00:00-05:00") == datetime(
        2023,
        1,
        1,
        17,
        0,
        0,
        tzinfo=UTC,
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
    converted to ISO strings. This test ensures FraiseQL can handle these
    pre-serialized datetimes from database views.
    """
    # Common formats from PostgreSQL JSONB
    jsonb_datetime_z = "2025-01-09T14:30:00Z"
    result = serialize_datetime(jsonb_datetime_z)
    assert result == "2025-01-09T14:30:00Z"

    jsonb_datetime_offset = "2025-01-09T14:30:00+02:00"
    result = serialize_datetime(jsonb_datetime_offset)
    assert result == "2025-01-09T14:30:00+02:00"

    # PostgreSQL sometimes uses +00:00 instead of Z
    jsonb_datetime_utc = "2025-01-09T14:30:00+00:00"
    result = serialize_datetime(jsonb_datetime_utc)
    assert result == "2025-01-09T14:30:00+00:00"

    # Invalid datetime string should raise error
    with pytest.raises(GraphQLError, match="DateTime cannot represent invalid ISO datetime string"):
        serialize_datetime("not-a-datetime")

    # Timezone-naive datetime should raise error (FraiseQL requires timezone)
    with pytest.raises(GraphQLError, match="DateTime cannot represent invalid ISO datetime string"):
        serialize_datetime("2025-01-09T14:30:00")
