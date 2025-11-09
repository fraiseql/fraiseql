"""Tests for ExchangeRate scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.exchange_rate import (
    ExchangeRateField,
    parse_exchange_rate_literal,
    parse_exchange_rate_value,
    serialize_exchange_rate,
)


@pytest.mark.unit
class TestExchangeRateSerialization:
    """Test exchange rate serialization."""

    def test_serialize_valid_exchange_rates(self) -> None:
        """Test serializing valid exchange rates."""
        assert serialize_exchange_rate("1.23456789") == "1.23456789"
        assert serialize_exchange_rate("1234.5") == "1234.5"
        assert serialize_exchange_rate("0.00001234") == "0.00001234"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_exchange_rate(None) is None

    def test_serialize_invalid_exchange_rate(self) -> None:
        """Test serializing invalid exchange rates raises error."""
        # Negative value
        with pytest.raises(GraphQLError, match="Invalid exchange rate"):
            serialize_exchange_rate("-1.23")

        # Too many decimal places
        with pytest.raises(GraphQLError, match="Invalid exchange rate"):
            serialize_exchange_rate("1.234567890")


class TestExchangeRateParsing:
    """Test exchange rate parsing from variables."""

    def test_parse_valid_exchange_rate(self) -> None:
        """Test parsing valid exchange rates."""
        assert parse_exchange_rate_value("1.23456789") == "1.23456789"

    def test_parse_invalid_exchange_rate(self) -> None:
        """Test parsing invalid exchange rates raises error."""
        with pytest.raises(GraphQLError, match="Invalid exchange rate"):
            parse_exchange_rate_value("-1.23")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-numeric types raises error."""
        with pytest.raises(GraphQLError, match="Exchange rate must be a number or string"):
            parse_exchange_rate_value(["1.23"])


class TestExchangeRateField:
    """Test ExchangeRateField class."""

    def test_create_valid_exchange_rate_field(self) -> None:
        """Test creating ExchangeRateField with valid values."""
        rate = ExchangeRateField("1.23456789")
        assert rate == "1.23456789"

    def test_create_invalid_exchange_rate_field(self) -> None:
        """Test creating ExchangeRateField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid exchange rate"):
            ExchangeRateField("-1.23")


class TestExchangeRateLiteralParsing:
    """Test parsing exchange rate from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid exchange rate literals."""
        assert parse_exchange_rate_literal(StringValueNode(value="1.23456789")) == "1.23456789"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid exchange rate format literals."""
        with pytest.raises(GraphQLError, match="Invalid exchange rate"):
            parse_exchange_rate_literal(StringValueNode(value="-1.23"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Exchange rate must be a string"):
            parse_exchange_rate_literal(IntValueNode(value="123"))
