import pytest
from graphql import GraphQLError
from graphql.language import StringValueNode

from fraiseql.types.scalars.email_address import (
    EmailAddressScalar,
    parse_email_address,
    parse_email_literal,
)


@pytest.mark.parametrize(
    "valid_email", ["user@example.com", "a@b.co", "first.last+tag@sub.domain.io"]
)
def test_parse_valid_email_address(valid_email: str) -> None:
    result = parse_email_address(valid_email)
    assert result == valid_email


@pytest.mark.parametrize(
    "invalid_email",
    ["", "no-at-symbol.com", "missing@domain", "@no-local-part.com", "a@b", 123, None, {}],
)
def test_parse_invalid_email_address_raises(invalid_email) -> None:
    with pytest.raises(GraphQLError):
        parse_email_address(invalid_email)  # type: ignore[arg-type]


def test_parse_literal_valid() -> None:
    node = StringValueNode(value="user@example.com")
    result = parse_email_literal(node)
    assert result == "user@example.com"


@pytest.mark.parametrize("ast_node", [StringValueNode(value="not-an-email")])
def test_parse_literal_invalid_value(ast_node: StringValueNode) -> None:
    with pytest.raises(GraphQLError):
        parse_email_literal(ast_node)


def test_parse_literal_invalid_node_type() -> None:
    from graphql.language import IntValueNode

    node = IntValueNode(value="123")
    with pytest.raises(GraphQLError):
        parse_email_literal(node)  # type: ignore[arg-type]


def test_serialize_returns_same_string() -> None:
    assert EmailAddressScalar.serialize("test@example.com") == "test@example.com"
