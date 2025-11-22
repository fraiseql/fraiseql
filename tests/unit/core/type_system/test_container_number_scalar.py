"""Tests for ContainerNumber scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.container_number import (
    ContainerNumberField,
    parse_container_number_literal,
    parse_container_number_value,
    serialize_container_number,
)


@pytest.mark.unit
class TestContainerNumberSerialization:
    """Test container number serialization."""

    def test_serialize_valid_container_numbers(self) -> None:
        """Test serializing valid ISO 6346 container numbers."""
        # Valid container numbers with correct check digits
        assert serialize_container_number("CSQU3054383") == "CSQU3054383"
        assert serialize_container_number("MSKU1234565") == "MSKU1234565"
        assert serialize_container_number("TCLU1234568") == "TCLU1234568"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_container_number(None) is None

    def test_serialize_invalid_container_number(self) -> None:
        """Test serializing invalid container numbers raises error."""
        # Wrong format
        with pytest.raises(GraphQLError, match="Invalid container number"):
            serialize_container_number("INVALID123")

        # Wrong length
        with pytest.raises(GraphQLError, match="Invalid container number"):
            serialize_container_number("CSQU305438")  # Too short

        with pytest.raises(GraphQLError, match="Invalid container number"):
            serialize_container_number("CSQU30543830")  # Too long

        # Invalid owner code (not A-Z)
        with pytest.raises(GraphQLError, match="Invalid container number"):
            serialize_container_number("1SQU3054383")

        # Invalid equipment category (not U/J/Z)
        with pytest.raises(GraphQLError, match="Invalid container number"):
            serialize_container_number("CSQA3054383")

        # Invalid check digit
        with pytest.raises(GraphQLError, match="Invalid container number"):
            serialize_container_number("CSQU3054380")  # Should be 3


class TestContainerNumberParsing:
    """Test container number parsing from variables."""

    def test_parse_valid_container_number(self) -> None:
        """Test parsing valid container numbers."""
        assert parse_container_number_value("CSQU3054383") == "CSQU3054383"
        assert parse_container_number_value("MSKU1234565") == "MSKU1234565"
        assert parse_container_number_value("TCLU1234568") == "TCLU1234568"

    def test_parse_invalid_container_number(self) -> None:
        """Test parsing invalid container numbers raises error."""
        with pytest.raises(GraphQLError, match="Invalid container number"):
            parse_container_number_value("INVALID123")

        with pytest.raises(GraphQLError, match="Invalid container number"):
            parse_container_number_value("CSQU3054380")  # Wrong check digit

        with pytest.raises(GraphQLError, match="Invalid container number"):
            parse_container_number_value("CSQA3054383")  # Wrong equipment category

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Container number must be a string"):
            parse_container_number_value(123)

        with pytest.raises(GraphQLError, match="Container number must be a string"):
            parse_container_number_value(None)

        with pytest.raises(GraphQLError, match="Container number must be a string"):
            parse_container_number_value(["CSQU3054383"])


class TestContainerNumberField:
    """Test ContainerNumberField class."""

    def test_create_valid_container_number_field(self) -> None:
        """Test creating ContainerNumberField with valid values."""
        container = ContainerNumberField("CSQU3054383")
        assert container == "CSQU3054383"
        assert isinstance(container, str)

        container = ContainerNumberField("MSKU1234565")
        assert container == "MSKU1234565"

    def test_create_invalid_container_number_field(self) -> None:
        """Test creating ContainerNumberField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid container number"):
            ContainerNumberField("INVALID123")

        with pytest.raises(ValueError, match="Invalid container number"):
            ContainerNumberField("CSQU3054380")  # Wrong check digit


class TestContainerNumberLiteralParsing:
    """Test parsing container number from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid container number literals."""
        assert parse_container_number_literal(StringValueNode(value="CSQU3054383")) == "CSQU3054383"
        assert parse_container_number_literal(StringValueNode(value="MSKU1234565")) == "MSKU1234565"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid container number format literals."""
        with pytest.raises(GraphQLError, match="Invalid container number"):
            parse_container_number_literal(StringValueNode(value="INVALID123"))

        with pytest.raises(GraphQLError, match="Invalid container number"):
            parse_container_number_literal(StringValueNode(value="CSQU3054380"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Container number must be a string"):
            parse_container_number_literal(IntValueNode(value="123"))
