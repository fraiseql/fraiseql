"""Tests for DomainName scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.domain_name import (
    DomainNameField,
    parse_domain_name_literal,
    parse_domain_name_value,
    serialize_domain_name,
)


@pytest.mark.unit
class TestDomainNameSerialization:
    """Test domain name serialization."""

    def test_serialize_valid_domain_names(self) -> None:
        """Test serializing valid RFC-compliant domain names."""
        assert serialize_domain_name("example.com") == "example.com"
        assert serialize_domain_name("subdomain.example.co.uk") == "subdomain.example.co.uk"
        assert serialize_domain_name("api.github.com") == "api.github.com"
        assert serialize_domain_name("test-domain.org") == "test-domain.org"
        assert serialize_domain_name("EXAMPLE.COM") == "example.com"

    def test_serialize_case_insensitive(self) -> None:
        """Test domain name serialization normalizes to lowercase."""
        assert serialize_domain_name("EXAMPLE.COM") == "example.com"
        assert serialize_domain_name("SubDomain.Example.Co.Uk") == "subdomain.example.co.uk"
        assert serialize_domain_name("API.GITHUB.COM") == "api.github.com"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_domain_name(None) is None

    def test_serialize_invalid_domain_names(self) -> None:
        """Test serializing invalid domain names raises error."""
        # Missing TLD
        with pytest.raises(GraphQLError, match="Invalid domain name"):
            serialize_domain_name("example")

        # Invalid characters
        with pytest.raises(GraphQLError, match="Invalid domain name"):
            serialize_domain_name("exam ple.com")

        with pytest.raises(GraphQLError, match="Invalid domain name"):
            serialize_domain_name("example!.com")

        # Leading/trailing hyphens
        with pytest.raises(GraphQLError, match="Invalid domain name"):
            serialize_domain_name("-example.com")

        with pytest.raises(GraphQLError, match="Invalid domain name"):
            serialize_domain_name("example-.com")

        # Consecutive hyphens
        with pytest.raises(GraphQLError, match="Invalid domain name"):
            serialize_domain_name("exam--ple.com")

        # Empty labels
        with pytest.raises(GraphQLError, match="Invalid domain name"):
            serialize_domain_name("example..com")

        # Invalid TLD (numbers only)
        with pytest.raises(GraphQLError, match="Invalid domain name"):
            serialize_domain_name("example.123")

        # Too long (over 253 chars)
        long_domain = "a" * 250 + ".com"
        with pytest.raises(GraphQLError, match="Invalid domain name"):
            serialize_domain_name(long_domain)


class TestDomainNameParsing:
    """Test domain name parsing from variables."""

    def test_parse_valid_domain_names(self) -> None:
        """Test parsing valid domain names."""
        assert parse_domain_name_value("example.com") == "example.com"
        assert parse_domain_name_value("subdomain.example.co.uk") == "subdomain.example.co.uk"
        assert parse_domain_name_value("EXAMPLE.COM") == "example.com"
        assert parse_domain_name_value("test-domain.org") == "test-domain.org"

    def test_parse_invalid_domain_names(self) -> None:
        """Test parsing invalid domain names raises error."""
        with pytest.raises(GraphQLError, match="Invalid domain name"):
            parse_domain_name_value("example")

        with pytest.raises(GraphQLError, match="Invalid domain name"):
            parse_domain_name_value("exam ple.com")

        with pytest.raises(GraphQLError, match="Invalid domain name"):
            parse_domain_name_value("-example.com")

        with pytest.raises(GraphQLError, match="Invalid domain name"):
            parse_domain_name_value("example..com")

        with pytest.raises(GraphQLError, match="Invalid domain name"):
            parse_domain_name_value("example.123")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Domain name must be a string"):
            parse_domain_name_value(123)

        with pytest.raises(GraphQLError, match="Domain name must be a string"):
            parse_domain_name_value(None)

        with pytest.raises(GraphQLError, match="Domain name must be a string"):
            parse_domain_name_value(["example.com"])


class TestDomainNameField:
    """Test DomainNameField class."""

    def test_create_valid_domain_name_field(self) -> None:
        """Test creating DomainNameField with valid values."""
        domain = DomainNameField("example.com")
        assert domain == "example.com"
        assert isinstance(domain, str)

        # Case normalization
        domain = DomainNameField("EXAMPLE.COM")
        assert domain == "example.com"

    def test_create_invalid_domain_name_field(self) -> None:
        """Test creating DomainNameField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid domain name"):
            DomainNameField("example")

        with pytest.raises(ValueError, match="Invalid domain name"):
            DomainNameField("exam ple.com")

        with pytest.raises(ValueError, match="Invalid domain name"):
            DomainNameField("-example.com")

        with pytest.raises(ValueError, match="Invalid domain name"):
            DomainNameField("example.123")


class TestDomainNameLiteralParsing:
    """Test parsing domain name from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid domain name literals."""
        assert parse_domain_name_literal(StringValueNode(value="example.com")) == "example.com"
        assert (
            parse_domain_name_literal(StringValueNode(value="subdomain.example.co.uk"))
            == "subdomain.example.co.uk"
        )
        assert parse_domain_name_literal(StringValueNode(value="EXAMPLE.COM")) == "example.com"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid domain name format literals."""
        with pytest.raises(GraphQLError, match="Invalid domain name"):
            parse_domain_name_literal(StringValueNode(value="example"))

        with pytest.raises(GraphQLError, match="Invalid domain name"):
            parse_domain_name_literal(StringValueNode(value="exam ple.com"))

        with pytest.raises(GraphQLError, match="Invalid domain name"):
            parse_domain_name_literal(StringValueNode(value="example.123"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Domain name must be a string"):
            parse_domain_name_literal(IntValueNode(value="123"))
