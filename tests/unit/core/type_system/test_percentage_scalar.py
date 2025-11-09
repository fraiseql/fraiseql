"""Tests for Percentage scalar type validation."""

from decimal import Decimal

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.percentage import (
    PercentageField,
    parse_percentage_literal,
    parse_percentage_value,
    serialize_percentage,
)


@pytest.mark.unit
class TestPercentageSerialization:
    """Test percentage serialization."""

    def test_serialize_valid_percentage_values(self) -> None:
        """Test serializing valid percentage values."""
        assert serialize_percentage("25.5") == "25.5"
        assert serialize_percentage("100") == "100"
        assert serialize_percentage("0.01") == "0.01"

    def test_serialize_numeric_inputs(self) -> None:
        """Test serializing numeric inputs."""
        assert serialize_percentage(25.5) == "25.5"
        assert serialize_percentage(Decimal("25.5")) == "25.5"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_percentage(None) is None

    def test_serialize_invalid_percentage(self) -> None:
        """Test serializing invalid percentage values raises error."""
        # Over 100
        with pytest.raises(GraphQLError, match="Invalid percentage"):
            serialize_percentage("150.5")

        # Too many decimal places
        with pytest.raises(GraphQLError, match="Invalid percentage"):
            serialize_percentage("25.555")

        # Invalid format
        with pytest.raises(GraphQLError, match="Invalid percentage"):
            serialize_percentage("abc")


class TestPercentageParsing:
    """Test percentage parsing from variables."""

    def test_parse_valid_percentage(self) -> None:
        """Test parsing valid percentage values."""
        assert parse_percentage_value("25.5") == "25.5"
        assert parse_percentage_value("100.00") == "100.00"

    def test_parse_invalid_percentage(self) -> None:
        """Test parsing invalid percentage values raises error."""
        with pytest.raises(GraphQLError, match="Invalid percentage"):
            parse_percentage_value("150.5")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-numeric types raises error."""
        with pytest.raises(GraphQLError, match="Percentage must be a number or string"):
            parse_percentage_value(["25.5"])


class TestPercentageField:
    """Test PercentageField class."""

    def test_create_valid_percentage_field(self) -> None:
        """Test creating PercentageField with valid values."""
        percentage = PercentageField("25.5")
        assert percentage == "25.5"

    def test_create_invalid_percentage_field(self) -> None:
        """Test creating PercentageField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid percentage"):
            PercentageField("150.5")


class TestPercentageLiteralParsing:
    """Test parsing percentage from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid percentage literals."""
        assert parse_percentage_literal(StringValueNode(value="25.5")) == "25.5"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid percentage format literals."""
        with pytest.raises(GraphQLError, match="Invalid percentage"):
            parse_percentage_literal(StringValueNode(value="150.5"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Percentage must be a string"):
            parse_percentage_literal(IntValueNode(value="25"))
