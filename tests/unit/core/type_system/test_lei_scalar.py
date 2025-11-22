"""Tests for LEI scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.lei import (
    LEIField,
    parse_lei_literal,
    parse_lei_value,
    serialize_lei,
)


@pytest.mark.unit
class TestLEISerialization:
    """Test LEI serialization."""

    def test_serialize_valid_leis(self) -> None:
        """Test serializing valid LEIs."""
        assert serialize_lei("549300E9PC51EN656011") == "549300E9PC51EN656011"

    def test_serialize_case_insensitive(self) -> None:
        """Test LEI serialization is case-insensitive."""
        assert serialize_lei("549300e9pc51en656011") == "549300E9PC51EN656011"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_lei(None) is None

    def test_serialize_invalid_lei(self) -> None:
        """Test serializing invalid LEIs raises error."""
        # Wrong length
        with pytest.raises(GraphQLError, match="Invalid LEI"):
            serialize_lei("549300E9PC51EN65601")


class TestLEIParsing:
    """Test LEI parsing from variables."""

    def test_parse_valid_lei(self) -> None:
        """Test parsing valid LEIs."""
        assert parse_lei_value("549300E9PC51EN656011") == "549300E9PC51EN656011"

    def test_parse_invalid_lei(self) -> None:
        """Test parsing invalid LEIs raises error."""
        with pytest.raises(GraphQLError, match="Invalid LEI"):
            parse_lei_value("549300E9PC51EN65601")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="LEI must be a string"):
            parse_lei_value(123)


class TestLEIField:
    """Test LEIField class."""

    def test_create_valid_lei_field(self) -> None:
        """Test creating LEIField with valid values."""
        lei = LEIField("549300E9PC51EN656011")
        assert lei == "549300E9PC51EN656011"

    def test_create_invalid_lei_field(self) -> None:
        """Test creating LEIField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid LEI"):
            LEIField("549300E9PC51EN65601")


class TestLEILiteralParsing:
    """Test parsing LEI from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid LEI literals."""
        assert (
            parse_lei_literal(StringValueNode(value="549300E9PC51EN656011"))
            == "549300E9PC51EN656011"
        )

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid LEI format literals."""
        with pytest.raises(GraphQLError, match="Invalid LEI"):
            parse_lei_literal(StringValueNode(value="549300E9PC51EN65601"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="LEI must be a string"):
            parse_lei_literal(IntValueNode(value="123"))
