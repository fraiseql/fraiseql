"""Tests for LicensePlate scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.license_plate import (
    LicensePlateField,
    parse_license_plate_literal,
    parse_license_plate_value,
    serialize_license_plate,
)


@pytest.mark.unit
class TestLicensePlateSerialization:
    """Test license plate serialization."""

    def test_serialize_valid_license_plates(self) -> None:
        """Test serializing valid license plates."""
        assert serialize_license_plate("ABC123") == "ABC123"
        assert serialize_license_plate("NY 1234 AB") == "NY 1234 AB"
        assert serialize_license_plate("ABC-1234") == "ABC-1234"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_license_plate(None) is None

    def test_serialize_invalid_license_plate(self) -> None:
        """Test serializing invalid license plates raises error."""
        # Too short
        with pytest.raises(GraphQLError, match="Invalid license plate"):
            serialize_license_plate("A")

        # Too long
        with pytest.raises(GraphQLError, match="Invalid license plate"):
            serialize_license_plate("A" * 13)

        # Contains invalid characters
        with pytest.raises(GraphQLError, match="Invalid license plate"):
            serialize_license_plate("ABC@123")

        # Empty
        with pytest.raises(GraphQLError, match="Invalid license plate"):
            serialize_license_plate("")


class TestLicensePlateParsing:
    """Test license plate parsing from variables."""

    def test_parse_valid_license_plate(self) -> None:
        """Test parsing valid license plates."""
        assert parse_license_plate_value("ABC123") == "ABC123"
        assert parse_license_plate_value("NY 1234 AB") == "NY 1234 AB"
        assert parse_license_plate_value("ABC-1234") == "ABC-1234"

    def test_parse_invalid_license_plate(self) -> None:
        """Test parsing invalid license plates raises error."""
        with pytest.raises(GraphQLError, match="Invalid license plate"):
            parse_license_plate_value("A")

        with pytest.raises(GraphQLError, match="Invalid license plate"):
            parse_license_plate_value("A" * 13)

        with pytest.raises(GraphQLError, match="Invalid license plate"):
            parse_license_plate_value("ABC@123")

        with pytest.raises(GraphQLError, match="Invalid license plate"):
            parse_license_plate_value("")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="License plate must be a string"):
            parse_license_plate_value(123)

        with pytest.raises(GraphQLError, match="License plate must be a string"):
            parse_license_plate_value(None)

        with pytest.raises(GraphQLError, match="License plate must be a string"):
            parse_license_plate_value(["ABC123"])


class TestLicensePlateField:
    """Test LicensePlateField class."""

    def test_create_valid_license_plate_field(self) -> None:
        """Test creating LicensePlateField with valid values."""
        plate = LicensePlateField("ABC123")
        assert plate == "ABC123"
        assert isinstance(plate, str)

        plate = LicensePlateField("NY 1234 AB")
        assert plate == "NY 1234 AB"

    def test_create_invalid_license_plate_field(self) -> None:
        """Test creating LicensePlateField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid license plate"):
            LicensePlateField("A")

        with pytest.raises(ValueError, match="Invalid license plate"):
            LicensePlateField("A" * 13)

        with pytest.raises(ValueError, match="Invalid license plate"):
            LicensePlateField("ABC@123")


class TestLicensePlateLiteralParsing:
    """Test parsing license plate from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid license plate literals."""
        assert parse_license_plate_literal(StringValueNode(value="ABC123")) == "ABC123"
        assert parse_license_plate_literal(StringValueNode(value="NY 1234 AB")) == "NY 1234 AB"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid license plate format literals."""
        with pytest.raises(GraphQLError, match="Invalid license plate"):
            parse_license_plate_literal(StringValueNode(value="A"))

        with pytest.raises(GraphQLError, match="Invalid license plate"):
            parse_license_plate_literal(StringValueNode(value="A" * 13))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="License plate must be a string"):
            parse_license_plate_literal(IntValueNode(value="123"))
