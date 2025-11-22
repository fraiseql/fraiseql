"""Tests for URL scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.url import (
    URLField,
    parse_url_literal,
    parse_url_value,
    serialize_url,
)


@pytest.mark.unit
class TestURLSerialization:
    """Test URL serialization."""

    def test_serialize_valid_urls(self) -> None:
        """Test serializing valid HTTP/HTTPS URLs."""
        assert serialize_url("https://example.com") == "https://example.com"
        assert serialize_url("http://api.example.com/v1/users") == "http://api.example.com/v1/users"
        assert serialize_url("https://example.com:8080/path") == "https://example.com:8080/path"
        assert (
            serialize_url("https://subdomain.example.com/path/to/resource")
            == "https://subdomain.example.com/path/to/resource"
        )

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_url(None) is None

    def test_serialize_invalid_url(self) -> None:
        """Test serializing invalid URLs raises error."""
        # Missing protocol
        with pytest.raises(GraphQLError, match="Invalid URL"):
            serialize_url("example.com")

        # Invalid protocol
        with pytest.raises(GraphQLError, match="Invalid URL"):
            serialize_url("ftp://example.com")

        # Invalid protocol
        with pytest.raises(GraphQLError, match="Invalid URL"):
            serialize_url("file:///etc/passwd")

        # Invalid characters
        with pytest.raises(GraphQLError, match="Invalid URL"):
            serialize_url("https://example.com/<script>")

        # Empty
        with pytest.raises(GraphQLError, match="Invalid URL"):
            serialize_url("")


class TestURLParsing:
    """Test URL parsing from variables."""

    def test_parse_valid_url(self) -> None:
        """Test parsing valid URLs."""
        assert parse_url_value("https://example.com") == "https://example.com"
        assert (
            parse_url_value("http://api.example.com/v1/users") == "http://api.example.com/v1/users"
        )
        assert parse_url_value("https://example.com:8080/path") == "https://example.com:8080/path"

    def test_parse_invalid_url(self) -> None:
        """Test parsing invalid URLs raises error."""
        with pytest.raises(GraphQLError, match="Invalid URL"):
            parse_url_value("example.com")

        with pytest.raises(GraphQLError, match="Invalid URL"):
            parse_url_value("ftp://example.com")

        with pytest.raises(GraphQLError, match="Invalid URL"):
            parse_url_value("")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="URL must be a string"):
            parse_url_value(123)

        with pytest.raises(GraphQLError, match="URL must be a string"):
            parse_url_value(None)

        with pytest.raises(GraphQLError, match="URL must be a string"):
            parse_url_value(["https://example.com"])


class TestURLField:
    """Test URLField class."""

    def test_create_valid_url_field(self) -> None:
        """Test creating URLField with valid values."""
        url = URLField("https://example.com")
        assert url == "https://example.com"
        assert isinstance(url, str)

        url = URLField("http://api.example.com/v1/users")
        assert url == "http://api.example.com/v1/users"

    def test_create_invalid_url_field(self) -> None:
        """Test creating URLField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid URL"):
            URLField("example.com")

        with pytest.raises(ValueError, match="Invalid URL"):
            URLField("ftp://example.com")

        with pytest.raises(ValueError, match="Invalid URL"):
            URLField("")


class TestURLLiteralParsing:
    """Test parsing URL from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid URL literals."""
        assert (
            parse_url_literal(StringValueNode(value="https://example.com")) == "https://example.com"
        )
        assert (
            parse_url_literal(StringValueNode(value="http://api.example.com/v1/users"))
            == "http://api.example.com/v1/users"
        )

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid URL format literals."""
        with pytest.raises(GraphQLError, match="Invalid URL"):
            parse_url_literal(StringValueNode(value="example.com"))

        with pytest.raises(GraphQLError, match="Invalid URL"):
            parse_url_literal(StringValueNode(value="ftp://example.com"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="URL must be a string"):
            parse_url_literal(IntValueNode(value="123"))
