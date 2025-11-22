"""Tests for Hostname scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.hostname import (
    HostnameField,
    parse_hostname_literal,
    parse_hostname_value,
    serialize_hostname,
)


@pytest.mark.unit
class TestHostnameSerialization:
    """Test hostname serialization."""

    def test_serialize_valid_hostname(self) -> None:
        """Test serializing valid hostnames."""
        assert serialize_hostname("example.com") == "example.com"
        assert serialize_hostname("sub.example.com") == "sub.example.com"
        assert serialize_hostname("my-server") == "my-server"
        assert serialize_hostname("server123") == "server123"
        assert serialize_hostname("a") == "a"  # Single character is valid
        assert serialize_hostname("123.456.789.local") == "123.456.789.local"

    def test_serialize_case_insensitive(self) -> None:
        """Test hostname serialization is case-insensitive (normalized to lowercase)."""
        assert serialize_hostname("EXAMPLE.COM") == "example.com"
        assert serialize_hostname("My-Server.Local") == "my-server.local"
        assert serialize_hostname("TEST123.Domain") == "test123.domain"

    def test_serialize_max_length(self) -> None:
        """Test serializing hostname at maximum length."""
        # 253 characters total (4 labels with dots)
        long_hostname = "a" * 63 + "." + "b" * 63 + "." + "c" * 63 + "." + "d" * 61
        assert len(long_hostname) == 253
        assert serialize_hostname(long_hostname) == long_hostname

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_hostname(None) is None

    def test_serialize_invalid_hostname(self) -> None:
        """Test serializing invalid hostnames raises error."""
        # Starting with hyphen
        with pytest.raises(GraphQLError, match="Invalid hostname"):
            serialize_hostname("-example.com")

        # Ending with hyphen
        with pytest.raises(GraphQLError, match="Invalid hostname"):
            serialize_hostname("example-.com")

        # Consecutive hyphens
        with pytest.raises(GraphQLError, match="Invalid hostname"):
            serialize_hostname("ex--ample.com")

        # Invalid characters
        with pytest.raises(GraphQLError, match="Invalid hostname"):
            serialize_hostname("example_com")  # Underscore not allowed

        with pytest.raises(GraphQLError, match="Invalid hostname"):
            serialize_hostname("example.com/path")  # Slash not allowed

        with pytest.raises(GraphQLError, match="Invalid hostname"):
            serialize_hostname("example@com")  # @ not allowed

        # Too long (over 253 characters)
        with pytest.raises(GraphQLError, match="Invalid hostname"):
            serialize_hostname("a" * 254)

        # Label too long (over 63 characters)
        with pytest.raises(GraphQLError, match="Invalid hostname"):
            serialize_hostname("a" * 64 + ".com")

        # Empty
        with pytest.raises(GraphQLError, match="Invalid hostname"):
            serialize_hostname("")

        # Just a dot
        with pytest.raises(GraphQLError, match="Invalid hostname"):
            serialize_hostname(".")

        # Ending with dot
        with pytest.raises(GraphQLError, match="Invalid hostname"):
            serialize_hostname("example.com.")


class TestHostnameParsing:
    """Test hostname parsing from variables."""

    def test_parse_valid_hostname(self) -> None:
        """Test parsing valid hostnames."""
        assert parse_hostname_value("example.com") == "example.com"
        assert parse_hostname_value("MY-SERVER") == "my-server"
        assert parse_hostname_value("TEST.DOMAIN.COM") == "test.domain.com"

    def test_parse_invalid_hostname(self) -> None:
        """Test parsing invalid hostnames raises error."""
        with pytest.raises(GraphQLError, match="Invalid hostname"):
            parse_hostname_value("-invalid.com")

        with pytest.raises(GraphQLError, match="Invalid hostname"):
            parse_hostname_value("invalid-.com")

        with pytest.raises(GraphQLError, match="Invalid hostname"):
            parse_hostname_value("in--valid.com")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Hostname must be a string"):
            parse_hostname_value(123)

        with pytest.raises(GraphQLError, match="Hostname must be a string"):
            parse_hostname_value(None)

        with pytest.raises(GraphQLError, match="Hostname must be a string"):
            parse_hostname_value(["example.com"])


class TestHostnameField:
    """Test HostnameField class."""

    def test_create_valid_hostname_field(self) -> None:
        """Test creating HostnameField with valid values."""
        hostname = HostnameField("example.com")
        assert hostname == "example.com"
        assert isinstance(hostname, str)

        # Case normalization
        hostname = HostnameField("EXAMPLE.COM")
        assert hostname == "example.com"

    def test_create_invalid_hostname_field(self) -> None:
        """Test creating HostnameField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid hostname"):
            HostnameField("-invalid.com")

        with pytest.raises(ValueError, match="Invalid hostname"):
            HostnameField("invalid..com")

        with pytest.raises(ValueError, match="Invalid hostname"):
            HostnameField("a" * 64 + ".com")


class TestHostnameLiteralParsing:
    """Test parsing hostname from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid hostname literals."""
        assert parse_hostname_literal(StringValueNode(value="example.com")) == "example.com"
        assert parse_hostname_literal(StringValueNode(value="SUB.EXAMPLE.COM")) == "sub.example.com"
        assert parse_hostname_literal(StringValueNode(value="my-server")) == "my-server"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid hostname format literals."""
        with pytest.raises(GraphQLError, match="Invalid hostname"):
            parse_hostname_literal(StringValueNode(value="-invalid.com"))

        with pytest.raises(GraphQLError, match="Invalid hostname"):
            parse_hostname_literal(StringValueNode(value="invalid..com"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Hostname must be a string"):
            parse_hostname_literal(IntValueNode(value="123"))
