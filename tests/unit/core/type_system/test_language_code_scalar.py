"""Tests for LanguageCode scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.language_code import (
    LanguageCodeField,
    parse_language_code_literal,
    parse_language_code_value,
    serialize_language_code,
)


@pytest.mark.unit
class TestLanguageCodeSerialization:
    """Test language code serialization."""

    def test_serialize_valid_language_codes(self) -> None:
        """Test serializing valid ISO 639-1 language codes."""
        assert serialize_language_code("en") == "en"
        assert serialize_language_code("fr") == "fr"
        assert serialize_language_code("de") == "de"
        assert serialize_language_code("es") == "es"
        assert serialize_language_code("ja") == "ja"
        assert serialize_language_code("zh") == "zh"
        assert serialize_language_code("ar") == "ar"
        assert serialize_language_code("ru") == "ru"
        assert serialize_language_code("pt") == "pt"
        assert serialize_language_code("it") == "it"

    def test_serialize_case_insensitive(self) -> None:
        """Test language code serialization is case-insensitive (normalized to lowercase)."""
        assert serialize_language_code("EN") == "en"
        assert serialize_language_code("Fr") == "fr"
        assert serialize_language_code("DE") == "de"
        assert serialize_language_code("eS") == "es"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_language_code(None) is None

    def test_serialize_invalid_language_code(self) -> None:
        """Test serializing invalid language codes raises error."""
        # Too long
        with pytest.raises(GraphQLError, match="Invalid language code"):
            serialize_language_code("eng")

        with pytest.raises(GraphQLError, match="Invalid language code"):
            serialize_language_code("english")

        # Too short
        with pytest.raises(GraphQLError, match="Invalid language code"):
            serialize_language_code("e")

        # Contains numbers
        with pytest.raises(GraphQLError, match="Invalid language code"):
            serialize_language_code("e1")

        # Contains special characters
        with pytest.raises(GraphQLError, match="Invalid language code"):
            serialize_language_code("en-US")  # Use LocaleCode instead

        with pytest.raises(GraphQLError, match="Invalid language code"):
            serialize_language_code("en_US")

        with pytest.raises(GraphQLError, match="Invalid language code"):
            serialize_language_code("en-")

        # Empty
        with pytest.raises(GraphQLError, match="Invalid language code"):
            serialize_language_code("")


class TestLanguageCodeParsing:
    """Test language code parsing from variables."""

    def test_parse_valid_language_code(self) -> None:
        """Test parsing valid language codes."""
        assert parse_language_code_value("en") == "en"
        assert parse_language_code_value("FR") == "fr"
        assert parse_language_code_value("De") == "de"

    def test_parse_invalid_language_code(self) -> None:
        """Test parsing invalid language codes raises error."""
        with pytest.raises(GraphQLError, match="Invalid language code"):
            parse_language_code_value("eng")

        with pytest.raises(GraphQLError, match="Invalid language code"):
            parse_language_code_value("e")

        with pytest.raises(GraphQLError, match="Invalid language code"):
            parse_language_code_value("en-US")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Language code must be a string"):
            parse_language_code_value(123)

        with pytest.raises(GraphQLError, match="Language code must be a string"):
            parse_language_code_value(None)

        with pytest.raises(GraphQLError, match="Language code must be a string"):
            parse_language_code_value(["en"])


class TestLanguageCodeField:
    """Test LanguageCodeField class."""

    def test_create_valid_language_code_field(self) -> None:
        """Test creating LanguageCodeField with valid values."""
        lang = LanguageCodeField("en")
        assert lang == "en"
        assert isinstance(lang, str)

        # Case normalization
        lang = LanguageCodeField("FR")
        assert lang == "fr"

    def test_create_invalid_language_code_field(self) -> None:
        """Test creating LanguageCodeField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid language code"):
            LanguageCodeField("eng")

        with pytest.raises(ValueError, match="Invalid language code"):
            LanguageCodeField("e")

        with pytest.raises(ValueError, match="Invalid language code"):
            LanguageCodeField("en-US")


class TestLanguageCodeLiteralParsing:
    """Test parsing language code from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid language code literals."""
        assert parse_language_code_literal(StringValueNode(value="en")) == "en"
        assert parse_language_code_literal(StringValueNode(value="FR")) == "fr"
        assert parse_language_code_literal(StringValueNode(value="De")) == "de"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid language code format literals."""
        with pytest.raises(GraphQLError, match="Invalid language code"):
            parse_language_code_literal(StringValueNode(value="eng"))

        with pytest.raises(GraphQLError, match="Invalid language code"):
            parse_language_code_literal(StringValueNode(value="en-US"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Language code must be a string"):
            parse_language_code_literal(IntValueNode(value="123"))
