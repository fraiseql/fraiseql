"""Tests for CurrencyCode scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.currency_code import (
    CurrencyCodeField,
    parse_currency_code_literal,
    parse_currency_code_value,
    serialize_currency_code,
)


@pytest.mark.unit
class TestCurrencyCodeSerialization:
    """Test currency code serialization."""

    def test_serialize_valid_currency_codes(self) -> None:
        """Test serializing valid ISO 4217 currency codes."""
        assert serialize_currency_code("USD") == "USD"
        assert serialize_currency_code("EUR") == "EUR"
        assert serialize_currency_code("GBP") == "GBP"
        assert serialize_currency_code("JPY") == "JPY"
        assert serialize_currency_code("CHF") == "CHF"
        assert serialize_currency_code("CAD") == "CAD"
        assert serialize_currency_code("AUD") == "AUD"

    def test_serialize_case_insensitive(self) -> None:
        """Test currency code serialization is case-insensitive (normalized to uppercase)."""
        assert serialize_currency_code("usd") == "USD"
        assert serialize_currency_code("Eur") == "EUR"
        assert serialize_currency_code("gbp") == "GBP"
        assert serialize_currency_code("JpY") == "JPY"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_currency_code(None) is None

    def test_serialize_invalid_currency_code(self) -> None:
        """Test serializing invalid currency codes raises error."""
        # Too long
        with pytest.raises(GraphQLError, match="Invalid currency code"):
            serialize_currency_code("USDD")

        with pytest.raises(GraphQLError, match="Invalid currency code"):
            serialize_currency_code("EURO")

        # Too short
        with pytest.raises(GraphQLError, match="Invalid currency code"):
            serialize_currency_code("US")

        with pytest.raises(GraphQLError, match="Invalid currency code"):
            serialize_currency_code("U")

        # Contains numbers
        with pytest.raises(GraphQLError, match="Invalid currency code"):
            serialize_currency_code("US1")

        # Contains special characters
        with pytest.raises(GraphQLError, match="Invalid currency code"):
            serialize_currency_code("US$")

        # Empty
        with pytest.raises(GraphQLError, match="Invalid currency code"):
            serialize_currency_code("")


class TestCurrencyCodeParsing:
    """Test currency code parsing from variables."""

    def test_parse_valid_currency_code(self) -> None:
        """Test parsing valid currency codes."""
        assert parse_currency_code_value("USD") == "USD"
        assert parse_currency_code_value("EUR") == "EUR"
        assert parse_currency_code_value("usd") == "USD"
        assert parse_currency_code_value("Eur") == "EUR"

    def test_parse_invalid_currency_code(self) -> None:
        """Test parsing invalid currency codes raises error."""
        with pytest.raises(GraphQLError, match="Invalid currency code"):
            parse_currency_code_value("USDD")

        with pytest.raises(GraphQLError, match="Invalid currency code"):
            parse_currency_code_value("US")

        with pytest.raises(GraphQLError, match="Invalid currency code"):
            parse_currency_code_value("US1")

        with pytest.raises(GraphQLError, match="Invalid currency code"):
            parse_currency_code_value("")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Currency code must be a string"):
            parse_currency_code_value(123)

        with pytest.raises(GraphQLError, match="Currency code must be a string"):
            parse_currency_code_value(None)

        with pytest.raises(GraphQLError, match="Currency code must be a string"):
            parse_currency_code_value(["USD"])


class TestCurrencyCodeField:
    """Test CurrencyCodeField class."""

    def test_create_valid_currency_code_field(self) -> None:
        """Test creating CurrencyCodeField with valid values."""
        currency = CurrencyCodeField("USD")
        assert currency == "USD"
        assert isinstance(currency, str)

        # Case normalization
        currency = CurrencyCodeField("eur")
        assert currency == "EUR"

    def test_create_invalid_currency_code_field(self) -> None:
        """Test creating CurrencyCodeField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid currency code"):
            CurrencyCodeField("USDD")

        with pytest.raises(ValueError, match="Invalid currency code"):
            CurrencyCodeField("US")

        with pytest.raises(ValueError, match="Invalid currency code"):
            CurrencyCodeField("US1")


class TestCurrencyCodeLiteralParsing:
    """Test parsing currency code from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid currency code literals."""
        assert parse_currency_code_literal(StringValueNode(value="USD")) == "USD"
        assert parse_currency_code_literal(StringValueNode(value="eur")) == "EUR"
        assert parse_currency_code_literal(StringValueNode(value="Gbp")) == "GBP"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid currency code format literals."""
        with pytest.raises(GraphQLError, match="Invalid currency code"):
            parse_currency_code_literal(StringValueNode(value="USDD"))

        with pytest.raises(GraphQLError, match="Invalid currency code"):
            parse_currency_code_literal(StringValueNode(value="US1"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Currency code must be a string"):
            parse_currency_code_literal(IntValueNode(value="123"))
