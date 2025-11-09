"""Tests for CIDR scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.cidr import (
    CIDRField,
    parse_cidr_literal,
    parse_cidr_value,
    serialize_cidr,
)


@pytest.mark.unit
class TestCIDRSerialization:
    """Test CIDR serialization."""

    def test_serialize_valid_ipv4_cidr(self) -> None:
        """Test serializing valid IPv4 CIDR notations."""
        assert serialize_cidr("192.168.1.0/24") == "192.168.1.0/24"
        assert serialize_cidr("10.0.0.0/8") == "10.0.0.0/8"
        assert serialize_cidr("172.16.0.0/12") == "172.16.0.0/12"
        assert serialize_cidr("0.0.0.0/0") == "0.0.0.0/0"
        assert serialize_cidr("192.168.1.1/32") == "192.168.1.1/32"

    def test_serialize_valid_ipv6_cidr(self) -> None:
        """Test serializing valid IPv6 CIDR notations."""
        assert serialize_cidr("2001:db8::/32") == "2001:db8::/32"
        assert serialize_cidr("::/0") == "::/0"
        assert serialize_cidr("fe80::/10") == "fe80::/10"

    def test_serialize_with_host_bits(self) -> None:
        """Test serializing CIDR with host bits set (allowed with strict=False)."""
        assert serialize_cidr("192.168.1.1/24") == "192.168.1.1/24"
        assert serialize_cidr("10.0.0.5/8") == "10.0.0.5/8"

    def test_serialize_ip_without_prefix(self) -> None:
        """Test serializing IP without prefix (automatically gets /32 for IPv4, /128 for IPv6)."""
        assert serialize_cidr("192.168.1.0") == "192.168.1.0"
        assert serialize_cidr("2001:db8::1") == "2001:db8::1"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_cidr(None) is None

    def test_serialize_invalid_cidr(self) -> None:
        """Test serializing invalid CIDR raises error."""
        with pytest.raises(GraphQLError, match="Invalid CIDR notation"):
            serialize_cidr("192.168.1.0/33")  # Invalid prefix for IPv4

        with pytest.raises(GraphQLError, match="Invalid CIDR notation"):
            serialize_cidr("999.999.999.999/24")  # Invalid IP

        with pytest.raises(GraphQLError, match="Invalid CIDR notation"):
            serialize_cidr("192.168.1.0/abc")  # Invalid prefix

        with pytest.raises(GraphQLError, match="Invalid CIDR notation"):
            serialize_cidr("invalid")  # Invalid format

    def test_serialize_invalid_type(self) -> None:
        """Test serializing non-string types raises error."""
        with pytest.raises(GraphQLError, match="CIDR must be a string"):
            serialize_cidr(192168)

        with pytest.raises(GraphQLError, match="CIDR must be a string"):
            serialize_cidr(["192.168.1.0/24"])


class TestCIDRParsing:
    """Test CIDR parsing from variables."""

    def test_parse_valid_cidr(self) -> None:
        """Test parsing valid CIDR notations."""
        assert parse_cidr_value("192.168.1.0/24") == "192.168.1.0/24"
        assert parse_cidr_value("10.0.0.0/8") == "10.0.0.0/8"
        assert parse_cidr_value("2001:db8::/32") == "2001:db8::/32"

    def test_parse_invalid_cidr(self) -> None:
        """Test parsing invalid CIDR raises error."""
        with pytest.raises(GraphQLError, match="Invalid CIDR notation"):
            parse_cidr_value("192.168.1.0/33")  # Invalid prefix for IPv4

        with pytest.raises(GraphQLError, match="Invalid CIDR notation"):
            parse_cidr_value("192.168.1.256/24")  # Invalid IP octet

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="CIDR must be a string"):
            parse_cidr_value(123)

        with pytest.raises(GraphQLError, match="CIDR must be a string"):
            parse_cidr_value(None)


class TestCIDRField:
    """Test CIDRField class."""

    def test_create_valid_cidr_field(self) -> None:
        """Test creating CIDRField with valid values."""
        cidr = CIDRField("192.168.1.0/24")
        assert cidr == "192.168.1.0/24"
        assert isinstance(cidr, str)

        cidr = CIDRField("2001:db8::/32")
        assert cidr == "2001:db8::/32"

    def test_create_invalid_cidr_field(self) -> None:
        """Test creating CIDRField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid CIDR notation"):
            CIDRField("192.168.1.0/33")  # Invalid prefix for IPv4

        with pytest.raises(ValueError, match="Invalid CIDR notation"):
            CIDRField("invalid")  # Invalid format

        with pytest.raises(ValueError, match="Invalid CIDR notation"):
            CIDRField("999.999.999.999/24")  # Invalid IP


class TestCIDRLiteralParsing:
    """Test parsing CIDR from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid CIDR literals."""
        assert parse_cidr_literal(StringValueNode(value="192.168.1.0/24")) == "192.168.1.0/24"
        assert parse_cidr_literal(StringValueNode(value="10.0.0.0/8")) == "10.0.0.0/8"
        assert parse_cidr_literal(StringValueNode(value="2001:db8::/32")) == "2001:db8::/32"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid CIDR format literals."""
        with pytest.raises(GraphQLError, match="Invalid CIDR notation"):
            parse_cidr_literal(StringValueNode(value="192.168.1.0/33"))  # Invalid prefix

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="CIDR must be a string"):
            parse_cidr_literal(IntValueNode(value="192"))
