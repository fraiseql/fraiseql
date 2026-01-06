"""Tests for ID type.

ID is a NewType based on str, following Strawberry's convention.
It maps to GraphQL's built-in ID scalar (GraphQLID).

Note: UUID validation for ID fields is handled at the input validation layer,
controlled by SchemaConfig.id_policy. The scalar itself accepts any string
to avoid "Redefinition of reserved type 'ID'" errors from graphql-core.
"""

import uuid

from graphql import GraphQLID

from fraiseql.types import ID
from fraiseql.types.scalars import IDScalar
from fraiseql.types.scalars.graphql_utils import convert_scalar_to_graphql
from fraiseql.types.scalars.id_scalar import ID as IdScalarID  # noqa: N811


def test_id_importable() -> None:
    """Test that ID is importable from fraiseql.types."""
    assert ID is not None


def test_id_is_newtype() -> None:
    """Test that ID is a NewType based on str."""
    assert hasattr(ID, "__supertype__")
    assert ID.__supertype__ is str


def test_id_callable_returns_str() -> None:
    """Test that ID(value) returns the value unchanged (NewType behavior)."""
    test_value = "550e8400-e29b-41d4-a716-446655440000"
    result = ID(test_value)
    assert result == test_value
    assert isinstance(result, str)


def test_id_maps_to_graphql_id() -> None:
    """Test that ID maps to GraphQL's built-in ID scalar."""
    graphql_type = convert_scalar_to_graphql(ID)
    assert graphql_type is GraphQLID
    assert graphql_type is IDScalar  # IDScalar is now an alias for GraphQLID
    assert graphql_type.name == "ID"


def test_id_scalar_is_graphql_id() -> None:
    """Test that IDScalar is GraphQL's built-in ID scalar.

    We use GraphQL's built-in ID to avoid "Redefinition of reserved type 'ID'"
    errors from graphql-core. UUID validation is done at input validation layer.
    """
    assert IDScalar is GraphQLID


def test_id_scalar_accepts_any_string() -> None:
    """Test that IDScalar (GraphQLID) accepts any string.

    Note: UUID validation happens at input validation layer via id_policy,
    not at the scalar level. The scalar accepts any string per GraphQL spec.
    """
    # Valid UUID works
    valid_uuid = "550e8400-e29b-41d4-a716-446655440000"
    assert IDScalar.serialize(valid_uuid) == valid_uuid

    # Non-UUID strings also work (GraphQL ID accepts any string)
    assert IDScalar.serialize("user-123") == "user-123"
    assert IDScalar.serialize("abc") == "abc"

    # Numbers are coerced to strings
    assert IDScalar.serialize(123) == "123"

    # UUIDs are converted to strings
    assert IDScalar.serialize(uuid.UUID(valid_uuid)) == valid_uuid


def test_id_scalar_parse_returns_string() -> None:
    """Test that parsing ID returns a string (not UUID object).

    GraphQL's built-in ID scalar returns strings. UUID parsing, if needed,
    happens at the application layer based on id_policy configuration.
    """
    uuid_str = "550e8400-e29b-41d4-a716-446655440000"
    parsed = IDScalar.parse_value(uuid_str)

    assert isinstance(parsed, str)
    assert parsed == uuid_str


def test_id_same_from_types_and_id_scalar_module() -> None:
    """Test that ID from types and id_scalar module is the same."""
    assert ID is IdScalarID
