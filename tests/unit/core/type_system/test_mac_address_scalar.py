"""Tests for MacAddress scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.mac_address import (
    MacAddressField,
    normalize_mac_address,
    parse_mac_address_literal,
    parse_mac_address_value,
    serialize_mac_address,
)


@pytest.mark.unit
class TestMacAddressNormalization:
    """Test MAC address normalization."""

    def test_normalize_colon_separated(self) -> None:
        """Test normalizing colon-separated format."""
        assert normalize_mac_address("00:11:22:33:44:55") == "00:11:22:33:44:55"
        assert normalize_mac_address("aa:bb:cc:dd:ee:ff") == "AA:BB:CC:DD:EE:FF"

    def test_normalize_hyphen_separated(self) -> None:
        """Test normalizing hyphen-separated format."""
        assert normalize_mac_address("00-11-22-33-44-55") == "00:11:22:33:44:55"
        assert normalize_mac_address("AA-BB-CC-DD-EE-FF") == "AA:BB:CC:DD:EE:FF"

    def test_normalize_dot_separated(self) -> None:
        """Test normalizing dot-separated (Cisco): format."""
        assert normalize_mac_address("0011.2233.4455") == "00:11:22:33:44:55"
        assert normalize_mac_address("aabb.ccdd.eeff") == "AA:BB:CC:DD:EE:FF"

    def test_normalize_no_separators(self) -> None:
        """Test normalizing format with no separators."""
        assert normalize_mac_address("001122334455") == "00:11:22:33:44:55"
        assert normalize_mac_address("AABBCCDDEEFF") == "AA:BB:CC:DD:EE:FF"

    def test_normalize_invalid_format(self) -> None:
        """Test normalizing invalid formats raises error."""
        with pytest.raises(ValueError, match="Invalid MAC address format"):
            normalize_mac_address("00:11:22:33:44")  # Too short

        with pytest.raises(ValueError, match="Invalid MAC address format"):
            normalize_mac_address("00:11:22:33:44:55:66")  # Too long

        with pytest.raises(ValueError, match="Invalid MAC address format"):
            normalize_mac_address("GG:11:22:33:44:55")  # Invalid hex


class TestMacAddressSerialization:
    """Test MAC address serialization."""

    def test_serialize_valid_formats(self) -> None:
        """Test serializing valid MAC address formats."""
        # All should normalize to colon-separated uppercase
        assert serialize_mac_address("00:11:22:33:44:55") == "00:11:22:33:44:55"
        assert serialize_mac_address("00-11-22-33-44-55") == "00:11:22:33:44:55"
        assert serialize_mac_address("0011.2233.4455") == "00:11:22:33:44:55"
        assert serialize_mac_address("001122334455") == "00:11:22:33:44:55"

    def test_serialize_case_insensitive(self) -> None:
        """Test MAC address serialization normalizes case."""
        assert serialize_mac_address("aa:bb:cc:dd:ee:ff") == "AA:BB:CC:DD:EE:FF"
        assert serialize_mac_address("Aa:Bb:Cc:Dd:Ee:Ff") == "AA:BB:CC:DD:EE:FF"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_mac_address(None) is None

    def test_serialize_invalid_format(self) -> None:
        """Test serializing invalid formats raises error."""
        with pytest.raises(GraphQLError, match="Invalid MAC address"):
            serialize_mac_address("invalid")

        with pytest.raises(GraphQLError, match="Invalid MAC address"):
            serialize_mac_address("00:11:22:33:44")  # Too short

        with pytest.raises(GraphQLError, match="Invalid MAC address"):
            serialize_mac_address("00:11:22:33:44:55:66")  # Too long

        with pytest.raises(GraphQLError, match="Invalid MAC address"):
            serialize_mac_address("00:11:22:33:44:GG")  # Invalid hex


class TestMacAddressParsing:
    """Test MAC address parsing from variables."""

    def test_parse_valid_formats(self) -> None:
        """Test parsing valid MAC address formats."""
        assert parse_mac_address_value("00:11:22:33:44:55") == "00:11:22:33:44:55"
        assert parse_mac_address_value("00-11-22-33-44-55") == "00:11:22:33:44:55"
        assert parse_mac_address_value("0011.2233.4455") == "00:11:22:33:44:55"
        assert parse_mac_address_value("001122334455") == "00:11:22:33:44:55"

    def test_parse_invalid_format(self) -> None:
        """Test parsing invalid formats raises error."""
        with pytest.raises(GraphQLError, match="Invalid MAC address"):
            parse_mac_address_value("invalid")

        with pytest.raises(GraphQLError, match="Invalid MAC address"):
            parse_mac_address_value("00:11:22:33:44:55:66")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="MAC address must be a string"):
            parse_mac_address_value(123)

        with pytest.raises(GraphQLError, match="MAC address must be a string"):
            parse_mac_address_value(None)

        with pytest.raises(GraphQLError, match="MAC address must be a string"):
            parse_mac_address_value(["00:11:22:33:44:55"])


class TestMacAddressField:
    """Test MacAddressField class."""

    def test_create_valid_mac_field(self) -> None:
        """Test creating MacAddressField with valid values."""
        # Colon format
        mac = MacAddressField("00:11:22:33:44:55")
        assert mac == "00:11:22:33:44:55"
        assert isinstance(mac, str)

        # Hyphen format - normalized to colon
        mac = MacAddressField("00-11-22-33-44-55")
        assert mac == "00:11:22:33:44:55"

        # Cisco format - normalized to colon
        mac = MacAddressField("0011.2233.4455")
        assert mac == "00:11:22:33:44:55"

        # Bare format - normalized to colon
        mac = MacAddressField("001122334455")
        assert mac == "00:11:22:33:44:55"

    def test_create_case_normalization(self) -> None:
        """Test MacAddressField normalizes to uppercase."""
        mac = MacAddressField("aa:bb:cc:dd:ee:ff")
        assert mac == "AA:BB:CC:DD:EE:FF"

    def test_create_invalid_mac_field(self) -> None:
        """Test creating MacAddressField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid MAC address"):
            MacAddressField("invalid")

        with pytest.raises(ValueError, match="Invalid MAC address"):
            MacAddressField("00:11:22:33:44")  # Too short

        with pytest.raises(ValueError, match="Invalid MAC address"):
            MacAddressField("00:11:22:33:44:55:66")  # Too long

        with pytest.raises(ValueError, match="Invalid MAC address"):
            MacAddressField("ZZ:11:22:33:44:55")  # Invalid hex


class TestMacAddressLiteralParsing:
    """Test parsing MAC address from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid MAC address literals."""
        assert (
            parse_mac_address_literal(StringValueNode(value="00:11:22:33:44:55"))
            == "00:11:22:33:44:55"
        )
        assert (
            parse_mac_address_literal(StringValueNode(value="00-11-22-33-44-55"))
            == "00:11:22:33:44:55"
        )
        assert (
            parse_mac_address_literal(StringValueNode(value="0011.2233.4455"))
            == "00:11:22:33:44:55"
        )

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid MAC address format literals."""
        with pytest.raises(GraphQLError, match="Invalid MAC address"):
            parse_mac_address_literal(StringValueNode(value="invalid"))

        with pytest.raises(GraphQLError, match="Invalid MAC address"):
            parse_mac_address_literal(StringValueNode(value="00:11:22:33:44"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="MAC address must be a string"):
            parse_mac_address_literal(IntValueNode(value="123"))
