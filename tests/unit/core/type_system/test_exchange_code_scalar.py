"""Tests for ExchangeCode scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.exchange_code import (
    ExchangeCodeField,
    parse_exchange_code_literal,
    parse_exchange_code_value,
    serialize_exchange_code,
)


@pytest.mark.unit
class TestExchangeCodeSerialization:
    """Test ExchangeCode serialization."""

    def test_serialize_valid_exchange_codes(self) -> None:
        """Test serializing valid exchange codes."""
        assert serialize_exchange_code("NYSE") == "NYSE"
        assert serialize_exchange_code("NASDAQ") == "NASDAQ"
        assert serialize_exchange_code("LSE") == "LSE"
        assert serialize_exchange_code("TSE") == "TSE"
        assert serialize_exchange_code("HKEX") == "HKEX"

    def test_serialize_case_insensitive(self) -> None:
        """Test ExchangeCode serialization is case-insensitive."""
        assert serialize_exchange_code("nyse") == "NYSE"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_exchange_code(None) is None

    def test_serialize_invalid_exchange_code(self) -> None:
        """Test serializing invalid exchange codes raises error."""
        # Too long
        with pytest.raises(GraphQLError, match="Invalid exchange code"):
            serialize_exchange_code("NYSETOOLONG")


class TestExchangeCodeParsing:
    """Test ExchangeCode parsing from variables."""

    def test_parse_valid_exchange_code(self) -> None:
        """Test parsing valid exchange codes."""
        assert parse_exchange_code_value("NYSE") == "NYSE"

    def test_parse_invalid_exchange_code(self) -> None:
        """Test parsing invalid exchange codes raises error."""
        with pytest.raises(GraphQLError, match="Invalid exchange code"):
            parse_exchange_code_value("NYSETOOLONG")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Exchange code must be a string"):
            parse_exchange_code_value(123)


class TestExchangeCodeField:
    """Test ExchangeCodeField class."""

    def test_create_valid_exchange_code_field(self) -> None:
        """Test creating ExchangeCodeField with valid values."""
        code = ExchangeCodeField("NYSE")
        assert code == "NYSE"

    def test_create_invalid_exchange_code_field(self) -> None:
        """Test creating ExchangeCodeField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid exchange code"):
            ExchangeCodeField("NYSETOOLONG")


class TestExchangeCodeLiteralParsing:
    """Test parsing ExchangeCode from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid ExchangeCode literals."""
        assert parse_exchange_code_literal(StringValueNode(value="NYSE")) == "NYSE"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid ExchangeCode format literals."""
        with pytest.raises(GraphQLError, match="Invalid exchange code"):
            parse_exchange_code_literal(StringValueNode(value="NYSETOOLONG"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Exchange code must be a string"):
            parse_exchange_code_literal(IntValueNode(value="123"))
