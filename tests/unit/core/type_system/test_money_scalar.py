"""Tests for Money scalar type validation."""

from decimal import Decimal

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.money import (
    MoneyField,
    parse_money_literal,
    parse_money_value,
    serialize_money,
)


@pytest.mark.unit
class TestMoneySerialization:
    """Test money serialization."""

    def test_serialize_valid_money_values(self) -> None:
        """Test serializing valid money values."""
        assert serialize_money("123.45") == "123.45"
        assert serialize_money("-999.9999") == "-999.9999"
        assert serialize_money("100") == "100"
        assert serialize_money("0.01") == "0.01"

    def test_serialize_numeric_inputs(self) -> None:
        """Test serializing numeric inputs."""
        assert serialize_money(123.45) == "123.45"
        assert serialize_money(Decimal("123.45")) == "123.45"
        assert serialize_money(-100) == "-100"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_money(None) is None

    def test_serialize_invalid_money(self) -> None:
        """Test serializing invalid money values raises error."""
        # Too many decimal places
        with pytest.raises(GraphQLError, match="Invalid money value"):
            serialize_money("123.45678")

        # Too many digits before decimal
        with pytest.raises(GraphQLError, match="Invalid money value"):
            serialize_money("123456789012345678901234567890")

        # Invalid format
        with pytest.raises(GraphQLError, match="Invalid money value"):
            serialize_money("abc")


class TestMoneyParsing:
    """Test money parsing from variables."""

    def test_parse_valid_money(self) -> None:
        """Test parsing valid money values."""
        assert parse_money_value("123.45") == "123.45"
        assert parse_money_value("-999.9999") == "-999.9999"
        assert parse_money_value("100") == "100"

    def test_parse_numeric_inputs(self) -> None:
        """Test parsing numeric inputs."""
        assert parse_money_value(123.45) == "123.45"
        assert parse_money_value(Decimal("123.45")) == "123.45"

    def test_parse_invalid_money(self) -> None:
        """Test parsing invalid money values raises error."""
        with pytest.raises(GraphQLError, match="Invalid money value"):
            parse_money_value("123.45678")

        with pytest.raises(GraphQLError, match="Invalid money value"):
            parse_money_value("abc")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-numeric types raises error."""
        with pytest.raises(GraphQLError, match="Money must be a number or string"):
            parse_money_value(["123.45"])


class TestMoneyField:
    """Test MoneyField class."""

    def test_create_valid_money_field(self) -> None:
        """Test creating MoneyField with valid values."""
        money = MoneyField("123.45")
        assert money == "123.45"
        assert isinstance(money, str)

        # Numeric input
        money = MoneyField(123.45)
        assert money == "123.45"

    def test_create_invalid_money_field(self) -> None:
        """Test creating MoneyField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid money value"):
            MoneyField("123.45678")

        with pytest.raises(ValueError, match="Invalid money value"):
            MoneyField("abc")


class TestMoneyLiteralParsing:
    """Test parsing money from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid money literals."""
        assert parse_money_literal(StringValueNode(value="123.45")) == "123.45"
        assert parse_money_literal(StringValueNode(value="-999.9999")) == "-999.9999"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid money format literals."""
        with pytest.raises(GraphQLError, match="Invalid money value"):
            parse_money_literal(StringValueNode(value="123.45678"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Money must be a string"):
            parse_money_literal(IntValueNode(value="123"))
