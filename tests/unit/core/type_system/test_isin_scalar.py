"""Tests for ISIN scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.isin import (
    ISINField,
    parse_isin_literal,
    parse_isin_value,
    serialize_isin,
)


@pytest.mark.unit
class TestISINSerialization:
    """Test ISIN serialization."""

    def test_serialize_valid_isins(self) -> None:
        """Test serializing valid ISINs."""
        assert serialize_isin("US0378331005") == "US0378331005"
        assert serialize_isin("GB0002374006") == "GB0002374006"

    def test_serialize_case_insensitive(self) -> None:
        """Test ISIN serialization is case-insensitive."""
        assert serialize_isin("us0378331005") == "US0378331005"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_isin(None) is None

    def test_serialize_invalid_isin(self) -> None:
        """Test serializing invalid ISINs raises error."""
        # Wrong length
        with pytest.raises(GraphQLError, match="Invalid ISIN"):
            serialize_isin("US037833100")

        # Invalid check digit
        with pytest.raises(GraphQLError, match="Invalid ISIN check digit"):
            serialize_isin("US0378331000")


class TestISINParsing:
    """Test ISIN parsing from variables."""

    def test_parse_valid_isin(self) -> None:
        """Test parsing valid ISINs."""
        assert parse_isin_value("US0378331005") == "US0378331005"

    def test_parse_invalid_isin(self) -> None:
        """Test parsing invalid ISINs raises error."""
        with pytest.raises(GraphQLError, match="Invalid ISIN"):
            parse_isin_value("US037833100")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="ISIN must be a string"):
            parse_isin_value(123)


class TestISINField:
    """Test ISINField class."""

    def test_create_valid_isin_field(self) -> None:
        """Test creating ISINField with valid values."""
        isin = ISINField("US0378331005")
        assert isin == "US0378331005"

    def test_create_invalid_isin_field(self) -> None:
        """Test creating ISINField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid ISIN"):
            ISINField("US037833100")


class TestISINLiteralParsing:
    """Test parsing ISIN from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid ISIN literals."""
        assert parse_isin_literal(StringValueNode(value="US0378331005")) == "US0378331005"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid ISIN format literals."""
        with pytest.raises(GraphQLError, match="Invalid ISIN"):
            parse_isin_literal(StringValueNode(value="US037833100"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="ISIN must be a string"):
            parse_isin_literal(IntValueNode(value="123"))
