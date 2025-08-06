# tests/types/scalars/test_ltree.py

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.ltree import parse_ltree_literal, parse_ltree_value, serialize_ltree

# --- Serialization Tests ---


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        ("foo", "foo"),
        ("foo.bar.baz", "foo.bar.baz"),
        ("user.123.profile.settings", "user.123.profile.settings"),
    ],
)
def test_serialize_ltree_valid(value, expected) -> None:
    assert serialize_ltree(value) == expected


@pytest.mark.parametrize("value", [123, None, ["foo"], {"foo": "bar"}])
def test_serialize_ltree_invalid(value) -> None:
    with pytest.raises(GraphQLError):
        serialize_ltree(value)


# --- parse_value Tests ---


@pytest.mark.parametrize(("value", "expected"), [("foo", "foo"), ("foo.bar", "foo.bar")])
def test_parse_ltree_value_valid(value, expected) -> None:
    assert parse_ltree_value(value) == expected


@pytest.mark.parametrize("value", [123, None, True])
def test_parse_ltree_value_invalid(value) -> None:
    with pytest.raises(GraphQLError):
        parse_ltree_value(value)


# --- parse_literal Tests ---


def test_parse_ltree_literal_valid() -> None:
    ast = StringValueNode(value="foo.bar.baz")
    assert parse_ltree_literal(ast, None) == "foo.bar.baz"


def test_parse_ltree_literal_invalid_node_type() -> None:
    ast = IntValueNode(value="123")
    with pytest.raises(GraphQLError):
        parse_ltree_literal(ast, None)


def test_parse_ltree_literal_invalid_value() -> None:
    class FakeNode:
        value = None

    ast = FakeNode()
    with pytest.raises(GraphQLError):
        parse_ltree_literal(ast, None)  # type: ignore[arg-type]
