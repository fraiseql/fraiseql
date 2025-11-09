"""Tests for VIN scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.vin import (
    VINField,
    parse_vin_literal,
    parse_vin_value,
    serialize_vin,
)


@pytest.mark.unit
class TestVINSerialization:
    """Test VIN serialization."""

    def test_serialize_valid_vins(self) -> None:
        """Test serializing valid ISO 3779/3780 VINs."""
        # Valid VINs with correct check digits
        assert serialize_vin("1HGBH41JXMN109186") == "1HGBH41JXMN109186"
        assert serialize_vin("JH4KA8268MC000000") == "JH4KA8268MC000000"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_vin(None) is None

    def test_serialize_invalid_vin(self) -> None:
        """Test serializing invalid VINs raises error."""
        # Wrong length
        with pytest.raises(GraphQLError, match="Invalid VIN"):
            serialize_vin("1HGBH41JXMN10918")  # Too short

        with pytest.raises(GraphQLError, match="Invalid VIN"):
            serialize_vin("1HGBH41JXMN1091867")  # Too long

        # Contains forbidden characters
        with pytest.raises(GraphQLError, match="Invalid VIN"):
            serialize_vin("1HGBH41JXMN10918I")  # Contains I

        with pytest.raises(GraphQLError, match="Invalid VIN"):
            serialize_vin("1HGBH41JXMN10918O")  # Contains O

        # Wrong check digit
        with pytest.raises(GraphQLError, match="Invalid VIN"):
            serialize_vin("1HGBH41JXMN109180")  # Should be 6


class TestVINParsing:
    """Test VIN parsing from variables."""

    def test_parse_valid_vin(self) -> None:
        """Test parsing valid VINs."""
        assert parse_vin_value("1HGBH41JXMN109186") == "1HGBH41JXMN109186"
        assert parse_vin_value("JH4KA8268MC000000") == "JH4KA8268MC000000"

    def test_parse_invalid_vin(self) -> None:
        """Test parsing invalid VINs raises error."""
        with pytest.raises(GraphQLError, match="Invalid VIN"):
            parse_vin_value("1HGBH41JXMN10918")  # Wrong length

        with pytest.raises(GraphQLError, match="Invalid VIN"):
            parse_vin_value("1HGBH41JXMN10918I")  # Forbidden character

        with pytest.raises(GraphQLError, match="Invalid VIN"):
            parse_vin_value("1HGBH41JXMN109180")  # Wrong check digit

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="VIN must be a string"):
            parse_vin_value(123)

        with pytest.raises(GraphQLError, match="VIN must be a string"):
            parse_vin_value(None)

        with pytest.raises(GraphQLError, match="VIN must be a string"):
            parse_vin_value(["1HGBH41JXMN109186"])


class TestVINField:
    """Test VINField class."""

    def test_create_valid_vin_field(self) -> None:
        """Test creating VINField with valid values."""
        vin = VINField("1HGBH41JXMN109186")
        assert vin == "1HGBH41JXMN109186"
        assert isinstance(vin, str)

        vin = VINField("JH4KA8268MC000000")
        assert vin == "JH4KA8268MC000000"

    def test_create_invalid_vin_field(self) -> None:
        """Test creating VINField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid VIN"):
            VINField("1HGBH41JXMN10918")  # Wrong length

        with pytest.raises(ValueError, match="Invalid VIN"):
            VINField("1HGBH41JXMN10918I")  # Forbidden character

        with pytest.raises(ValueError, match="Invalid VIN"):
            VINField("1HGBH41JXMN109180")  # Wrong check digit


class TestVINLiteralParsing:
    """Test parsing VIN from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid VIN literals."""
        assert parse_vin_literal(StringValueNode(value="1HGBH41JXMN109186")) == "1HGBH41JXMN109186"
        assert parse_vin_literal(StringValueNode(value="JH4KA8268MC000000")) == "JH4KA8268MC000000"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid VIN format literals."""
        with pytest.raises(GraphQLError, match="Invalid VIN"):
            parse_vin_literal(StringValueNode(value="1HGBH41JXMN10918"))

        with pytest.raises(GraphQLError, match="Invalid VIN"):
            parse_vin_literal(StringValueNode(value="1HGBH41JXMN10918I"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="VIN must be a string"):
            parse_vin_literal(IntValueNode(value="123"))
