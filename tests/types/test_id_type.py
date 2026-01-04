"""Tests for ID type.

ID is a NewType based on str, following Strawberry's convention.
It maps to a custom IDScalar that:
- Is named "ID" for Apollo/Relay cache compatibility
- Enforces UUID format (FraiseQL is opinionated)
"""

import uuid

import pytest
from graphql import GraphQLError

from fraiseql.types import ID
from fraiseql.types.scalars import IDScalar
from fraiseql.types.scalars.graphql_utils import convert_scalar_to_graphql
from fraiseql.types.scalars.id_scalar import ID as IdScalarID


def test_id_importable():
    """Test that ID is importable from fraiseql.types."""
    assert ID is not None


def test_id_is_newtype():
    """Test that ID is a NewType based on str."""
    assert hasattr(ID, "__supertype__")
    assert ID.__supertype__ is str


def test_id_callable_returns_str():
    """Test that ID(value) returns the value unchanged (NewType behavior)."""
    test_value = "550e8400-e29b-41d4-a716-446655440000"
    result = ID(test_value)
    assert result == test_value
    assert isinstance(result, str)


def test_id_maps_to_id_scalar():
    """Test that ID maps to custom IDScalar (named 'ID', enforces UUID)."""
    graphql_type = convert_scalar_to_graphql(ID)
    assert graphql_type is IDScalar
    assert graphql_type.name == "ID"  # For cache compatibility


def test_id_scalar_enforces_uuid():
    """Test that IDScalar enforces UUID format."""
    valid_uuid = "550e8400-e29b-41d4-a716-446655440000"

    # Valid UUIDs work
    assert IDScalar.serialize(valid_uuid) == valid_uuid
    assert IDScalar.serialize(uuid.UUID(valid_uuid)) == valid_uuid

    # Invalid formats are rejected
    with pytest.raises(GraphQLError, match="must be a valid UUID"):
        IDScalar.serialize("not-a-uuid")

    with pytest.raises(GraphQLError, match="must be a valid UUID"):
        IDScalar.serialize(123)


def test_id_scalar_parse_returns_uuid():
    """Test that parsing ID returns a UUID object."""
    uuid_str = "550e8400-e29b-41d4-a716-446655440000"
    parsed = IDScalar.parse_value(uuid_str)

    assert isinstance(parsed, uuid.UUID)
    assert str(parsed) == uuid_str


def test_id_scalar_parse_rejects_invalid():
    """Test that parsing invalid ID raises error."""
    with pytest.raises(GraphQLError, match="must be a valid UUID"):
        IDScalar.parse_value("invalid-id")


def test_id_same_from_types_and_id_scalar_module():
    """Test that ID from types and id_scalar module is the same."""
    assert ID is IdScalarID
