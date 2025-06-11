from datetime import UTC, datetime

import pytest
from graphql import GraphQLError
from graphql.language import StringValueNode

from fraiseql.types.scalars.datetime import (
    parse_datetime_literal,
    parse_datetime_value,
    serialize_datetime,
)


def test_serialize_datetime():
    # Test serializing datetime to ISO 8601 string
    dt = datetime(2023, 1, 1, 12, 0, 0, tzinfo=UTC)
    assert serialize_datetime(dt) == "2023-01-01T12:00:00Z"

    dt = datetime(2023, 1, 1, 12, 0, 0, tzinfo=UTC)
    assert serialize_datetime(dt) == "2023-01-01T12:00:00Z"


def test_serialize_non_datetime():
    # Test serializing non-datetime value
    with pytest.raises(GraphQLError):
        serialize_datetime("not a datetime")  # type: ignore


def test_parse_datetime_value():
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


def test_parse_invalid_datetime_value():
    # Test parsing invalid ISO 8601 strings
    with pytest.raises(GraphQLError):
        parse_datetime_value("2023-01-01T12:00:00")
    with pytest.raises(GraphQLError):
        parse_datetime_value("not a datetime")


def test_parse_none_datetime_value():
    # Test parsing None value
    assert parse_datetime_value(None) is None


def test_parse_datetime_literal():
    # Test parsing a DateTime literal from GraphQL AST
    ast = StringValueNode(value="2023-01-01T12:00:00Z")
    assert parse_datetime_literal(ast) == datetime(2023, 1, 1, 12, 0, 0, tzinfo=UTC)


def test_parse_invalid_datetime_literal():
    # Test parsing invalid DateTime literal from GraphQL AST
    ast = StringValueNode(value="not a datetime")
    with pytest.raises(GraphQLError):
        parse_datetime_literal(ast)
