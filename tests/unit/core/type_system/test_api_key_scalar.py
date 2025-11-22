"""Tests for ApiKey scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.api_key import (
    ApiKeyField,
    parse_api_key_literal,
    parse_api_key_value,
    serialize_api_key,
)


@pytest.mark.unit
class TestApiKeySerialization:
    """Test API key serialization."""

    def test_serialize_valid_api_keys(self) -> None:
        """Test serializing valid API keys."""
        assert (
            serialize_api_key("test_key_4eC39HqLyjWDarjtT1zdp7dc")
            == "test_key_4eC39HqLyjWDarjtT1zdp7dc"
        )
        assert serialize_api_key("api_key_12345678901234567890") == "api_key_12345678901234567890"
        assert (
            serialize_api_key("ABCDEFGHIJKLMNOPQRSTUVWXYZ123456")
            == "ABCDEFGHIJKLMNOPQRSTUVWXYZ123456"
        )
        assert serialize_api_key("a" * 16) == "a" * 16
        assert serialize_api_key("a" * 128) == "a" * 128

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_api_key(None) is None

    def test_serialize_invalid_api_keys(self) -> None:
        """Test serializing invalid API keys raises error."""
        # Too short
        with pytest.raises(GraphQLError, match="Invalid API key"):
            serialize_api_key("short")

        with pytest.raises(GraphQLError, match="Invalid API key"):
            serialize_api_key("a" * 15)

        # Too long
        with pytest.raises(GraphQLError, match="Invalid API key"):
            serialize_api_key("a" * 129)

        # Contains spaces
        with pytest.raises(GraphQLError, match="Invalid API key"):
            serialize_api_key("sk_test key")

        # Contains special characters
        with pytest.raises(GraphQLError, match="Invalid API key"):
            serialize_api_key("sk_test@key")

        with pytest.raises(GraphQLError, match="Invalid API key"):
            serialize_api_key("sk_test.key")

        # Empty
        with pytest.raises(GraphQLError, match="Invalid API key"):
            serialize_api_key("")


class TestApiKeyParsing:
    """Test API key parsing from variables."""

    def test_parse_valid_api_keys(self) -> None:
        """Test parsing valid API keys."""
        assert (
            parse_api_key_value("test_key_4eC39HqLyjWDarjtT1zdp7dc")
            == "test_key_4eC39HqLyjWDarjtT1zdp7dc"
        )
        assert parse_api_key_value("api_key_12345678901234567890") == "api_key_12345678901234567890"
        assert parse_api_key_value("a" * 16) == "a" * 16
        assert parse_api_key_value("a" * 128) == "a" * 128

    def test_parse_invalid_api_keys(self) -> None:
        """Test parsing invalid API keys raises error."""
        with pytest.raises(GraphQLError, match="Invalid API key"):
            parse_api_key_value("short")

        with pytest.raises(GraphQLError, match="Invalid API key"):
            parse_api_key_value("a" * 15)

        with pytest.raises(GraphQLError, match="Invalid API key"):
            parse_api_key_value("a" * 129)

        with pytest.raises(GraphQLError, match="Invalid API key"):
            parse_api_key_value("sk_test key")

        with pytest.raises(GraphQLError, match="Invalid API key"):
            parse_api_key_value("")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="API key must be a string"):
            parse_api_key_value(123)

        with pytest.raises(GraphQLError, match="API key must be a string"):
            parse_api_key_value(None)

        with pytest.raises(GraphQLError, match="API key must be a string"):
            parse_api_key_value(["sk_test_key"])


class TestApiKeyField:
    """Test ApiKeyField class."""

    def test_create_valid_api_key_field(self) -> None:
        """Test creating ApiKeyField with valid values."""
        api_key = ApiKeyField("test_key_4eC39HqLyjWDarjtT1zdp7dc")
        assert api_key == "test_key_4eC39HqLyjWDarjtT1zdp7dc"
        assert isinstance(api_key, str)

        # Minimum length
        api_key = ApiKeyField("a" * 16)
        assert api_key == "a" * 16

        # Maximum length
        api_key = ApiKeyField("a" * 128)
        assert api_key == "a" * 128

    def test_create_invalid_api_key_field(self) -> None:
        """Test creating ApiKeyField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid API key"):
            ApiKeyField("short")

        with pytest.raises(ValueError, match="Invalid API key"):
            ApiKeyField("a" * 15)

        with pytest.raises(ValueError, match="Invalid API key"):
            ApiKeyField("a" * 129)

        with pytest.raises(ValueError, match="Invalid API key"):
            ApiKeyField("sk_test key")


class TestApiKeyLiteralParsing:
    """Test parsing API key from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid API key literals."""
        assert (
            parse_api_key_literal(StringValueNode(value="test_key_4eC39HqLyjWDarjtT1zdp7dc"))
            == "test_key_4eC39HqLyjWDarjtT1zdp7dc"
        )
        assert parse_api_key_literal(StringValueNode(value="a" * 16)) == "a" * 16
        assert parse_api_key_literal(StringValueNode(value="a" * 128)) == "a" * 128

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid API key format literals."""
        with pytest.raises(GraphQLError, match="Invalid API key"):
            parse_api_key_literal(StringValueNode(value="short"))

        with pytest.raises(GraphQLError, match="Invalid API key"):
            parse_api_key_literal(StringValueNode(value="a" * 15))

        with pytest.raises(GraphQLError, match="Invalid API key"):
            parse_api_key_literal(StringValueNode(value="sk_test key"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="API key must be a string"):
            parse_api_key_literal(IntValueNode(value="123"))
