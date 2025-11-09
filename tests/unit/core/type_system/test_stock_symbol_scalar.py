"""Tests for StockSymbol scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.stock_symbol import (
    StockSymbolField,
    parse_stock_symbol_literal,
    parse_stock_symbol_value,
    serialize_stock_symbol,
)


@pytest.mark.unit
class TestStockSymbolSerialization:
    """Test stock symbol serialization."""

    def test_serialize_valid_stock_symbols(self) -> None:
        """Test serializing valid stock symbols."""
        assert serialize_stock_symbol("AAPL") == "AAPL"
        assert serialize_stock_symbol("MSFT") == "MSFT"
        assert serialize_stock_symbol("BRK.A") == "BRK.A"
        assert serialize_stock_symbol("BRK.B") == "BRK.B"

    def test_serialize_case_insensitive(self) -> None:
        """Test stock symbol serialization is case-insensitive."""
        assert serialize_stock_symbol("aapl") == "AAPL"
        assert serialize_stock_symbol("msft") == "MSFT"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_stock_symbol(None) is None

    def test_serialize_invalid_stock_symbol(self) -> None:
        """Test serializing invalid stock symbols raises error."""
        # Too long
        with pytest.raises(GraphQLError, match="Invalid stock symbol"):
            serialize_stock_symbol("AAPLTOOMANY")

        # Invalid class suffix (only single letter allowed)
        with pytest.raises(GraphQLError, match="Invalid stock symbol"):
            serialize_stock_symbol("AAPL.AA")


class TestStockSymbolParsing:
    """Test stock symbol parsing from variables."""

    def test_parse_valid_stock_symbol(self) -> None:
        """Test parsing valid stock symbols."""
        assert parse_stock_symbol_value("AAPL") == "AAPL"
        assert parse_stock_symbol_value("aapl") == "AAPL"

    def test_parse_invalid_stock_symbol(self) -> None:
        """Test parsing invalid stock symbols raises error."""
        with pytest.raises(GraphQLError, match="Invalid stock symbol"):
            parse_stock_symbol_value("AAPLTOOMANY")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Stock symbol must be a string"):
            parse_stock_symbol_value(123)


class TestStockSymbolField:
    """Test StockSymbolField class."""

    def test_create_valid_stock_symbol_field(self) -> None:
        """Test creating StockSymbolField with valid values."""
        symbol = StockSymbolField("AAPL")
        assert symbol == "AAPL"

    def test_create_invalid_stock_symbol_field(self) -> None:
        """Test creating StockSymbolField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid stock symbol"):
            StockSymbolField("AAPLTOOMANY")


class TestStockSymbolLiteralParsing:
    """Test parsing stock symbol from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid stock symbol literals."""
        assert parse_stock_symbol_literal(StringValueNode(value="AAPL")) == "AAPL"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid stock symbol format literals."""
        with pytest.raises(GraphQLError, match="Invalid stock symbol"):
            parse_stock_symbol_literal(StringValueNode(value="AAPLTOOMANY"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Stock symbol must be a string"):
            parse_stock_symbol_literal(IntValueNode(value="123"))
