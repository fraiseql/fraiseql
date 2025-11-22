"""Tests for LocaleCode scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.locale_code import (
    LocaleCodeField,
    parse_locale_code_literal,
    parse_locale_code_value,
    serialize_locale_code,
)


@pytest.mark.unit
class TestLocaleCodeSerialization:
    """Test locale code serialization."""

    def test_serialize_valid_locale_codes(self) -> None:
        """Test serializing valid BCP 47 locale codes."""
        assert serialize_locale_code("en-US") == "en-US"
        assert serialize_locale_code("fr-FR") == "fr-FR"
        assert serialize_locale_code("de-DE") == "de-DE"
        assert serialize_locale_code("es-ES") == "es-ES"
        assert serialize_locale_code("ja-JP") == "ja-JP"
        assert serialize_locale_code("zh-CN") == "zh-CN"
        assert serialize_locale_code("pt-BR") == "pt-BR"
        assert serialize_locale_code("en-GB") == "en-GB"

    def test_serialize_language_only(self) -> None:
        """Test serializing language-only codes."""
        assert serialize_locale_code("en") == "en"
        assert serialize_locale_code("fr") == "fr"
        assert serialize_locale_code("de") == "de"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_locale_code(None) is None

    def test_serialize_invalid_locale_code(self) -> None:
        """Test serializing invalid locale codes raises error."""
        # Wrong case (must be lowercase-UPPERCASE)
        with pytest.raises(GraphQLError, match="Invalid locale code"):
            serialize_locale_code("EN-us")

        with pytest.raises(GraphQLError, match="Invalid locale code"):
            serialize_locale_code("EN-US")

        with pytest.raises(GraphQLError, match="Invalid locale code"):
            serialize_locale_code("en-us")

        # Underscore instead of hyphen
        with pytest.raises(GraphQLError, match="Invalid locale code"):
            serialize_locale_code("en_US")

        # Region too long
        with pytest.raises(GraphQLError, match="Invalid locale code"):
            serialize_locale_code("en-USA")

        # Language too long
        with pytest.raises(GraphQLError, match="Invalid locale code"):
            serialize_locale_code("eng-US")

        # Invalid characters
        with pytest.raises(GraphQLError, match="Invalid locale code"):
            serialize_locale_code("en-U1")

        # Empty
        with pytest.raises(GraphQLError, match="Invalid locale code"):
            serialize_locale_code("")


class TestLocaleCodeParsing:
    """Test locale code parsing from variables."""

    def test_parse_valid_locale_code(self) -> None:
        """Test parsing valid locale codes."""
        assert parse_locale_code_value("en-US") == "en-US"
        assert parse_locale_code_value("fr-FR") == "fr-FR"
        assert parse_locale_code_value("en") == "en"

    def test_parse_invalid_locale_code(self) -> None:
        """Test parsing invalid locale codes raises error."""
        with pytest.raises(GraphQLError, match="Invalid locale code"):
            parse_locale_code_value("EN-us")

        with pytest.raises(GraphQLError, match="Invalid locale code"):
            parse_locale_code_value("en_US")

        with pytest.raises(GraphQLError, match="Invalid locale code"):
            parse_locale_code_value("en-USA")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Locale code must be a string"):
            parse_locale_code_value(123)

        with pytest.raises(GraphQLError, match="Locale code must be a string"):
            parse_locale_code_value(None)

        with pytest.raises(GraphQLError, match="Locale code must be a string"):
            parse_locale_code_value(["en-US"])


class TestLocaleCodeField:
    """Test LocaleCodeField class."""

    def test_create_valid_locale_code_field(self) -> None:
        """Test creating LocaleCodeField with valid values."""
        locale = LocaleCodeField("en-US")
        assert locale == "en-US"
        assert isinstance(locale, str)

        locale = LocaleCodeField("fr")
        assert locale == "fr"

    def test_create_invalid_locale_code_field(self) -> None:
        """Test creating LocaleCodeField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid locale code"):
            LocaleCodeField("EN-us")

        with pytest.raises(ValueError, match="Invalid locale code"):
            LocaleCodeField("en_US")

        with pytest.raises(ValueError, match="Invalid locale code"):
            LocaleCodeField("en-USA")


class TestLocaleCodeLiteralParsing:
    """Test parsing locale code from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid locale code literals."""
        assert parse_locale_code_literal(StringValueNode(value="en-US")) == "en-US"
        assert parse_locale_code_literal(StringValueNode(value="fr-FR")) == "fr-FR"
        assert parse_locale_code_literal(StringValueNode(value="en")) == "en"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid locale code format literals."""
        with pytest.raises(GraphQLError, match="Invalid locale code"):
            parse_locale_code_literal(StringValueNode(value="EN-us"))

        with pytest.raises(GraphQLError, match="Invalid locale code"):
            parse_locale_code_literal(StringValueNode(value="en_US"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Locale code must be a string"):
            parse_locale_code_literal(IntValueNode(value="123"))
