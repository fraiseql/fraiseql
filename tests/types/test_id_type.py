"""Tests for ID type."""

import uuid

import pytest
from graphql import GraphQLError

from fraiseql.types import ID
from fraiseql.types.scalars import IDScalar


def test_id_importable():
    """Test that ID is importable from fraiseql.types."""
    assert ID is not None


def test_id_scalar_exists():
    """Test that IDScalar exists."""
    assert IDScalar is not None
    assert IDScalar.name == "ID"


def test_id_scalar_serialize():
    """Test ID serialization."""
    test_uuid = uuid.uuid4()

    # Serialize UUID
    assert IDScalar.serialize(test_uuid) == str(test_uuid)

    # Serialize string
    assert IDScalar.serialize(str(test_uuid)) == str(test_uuid)


def test_id_scalar_parse():
    """Test ID parsing.

    NOTE: FraiseQL uses GraphQL's built-in ID type which returns strings,
    not UUID objects. This matches the GraphQL spec where ID is an opaque string.
    """
    test_uuid_str = "550e8400-e29b-41d4-a716-446655440000"

    parsed = IDScalar.parse_value(test_uuid_str)
    assert isinstance(parsed, str)  # Built-in ID returns string, not UUID
    assert parsed == test_uuid_str


def test_id_scalar_parse_invalid():
    """Test ID parsing with invalid value.

    NOTE: GraphQL's built-in ID type is permissive and accepts various inputs
    (strings, ints, etc.) as per the GraphQL spec. It doesn't validate UUID format.
    """
    # Built-in ID accepts any string (no UUID validation)
    result = IDScalar.parse_value("not-a-uuid")
    assert isinstance(result, str)
    assert result == "not-a-uuid"

    # Built-in ID also accepts integers
    result_int = IDScalar.parse_value(123)
    assert isinstance(result_int, str)
    assert result_int == "123"


def test_id_scalar_serialize_invalid():
    """Test ID serialization with invalid value.

    NOTE: GraphQL's built-in ID type accepts integers and converts them to strings.
    It only raises errors for truly invalid types like dicts.
    """
    # Built-in ID accepts integers
    result = IDScalar.serialize(123)
    assert result == "123"

    # Built-in ID should error on dict
    with pytest.raises((GraphQLError, TypeError)):
        IDScalar.serialize({"not": "a uuid"})
