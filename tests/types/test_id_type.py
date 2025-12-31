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
    """Test ID parsing."""
    test_uuid_str = "550e8400-e29b-41d4-a716-446655440000"

    parsed = IDScalar.parse_value(test_uuid_str)
    assert isinstance(parsed, uuid.UUID)
    assert str(parsed) == test_uuid_str


def test_id_scalar_parse_invalid():
    """Test ID parsing with invalid value."""
    with pytest.raises(GraphQLError):
        IDScalar.parse_value("not-a-uuid")

    with pytest.raises(GraphQLError):
        IDScalar.parse_value(123)


def test_id_scalar_serialize_invalid():
    """Test ID serialization with invalid value."""
    with pytest.raises(GraphQLError):
        IDScalar.serialize(123)

    with pytest.raises(GraphQLError):
        IDScalar.serialize({"not": "a uuid"})
