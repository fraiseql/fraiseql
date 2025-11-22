"""Tests for SEDOL scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.sedol import (
    SEDOLField,
    parse_sedol_literal,
    parse_sedol_value,
    serialize_sedol,
)


@pytest.mark.unit
class TestSEDOLSerialization:
    """Test SEDOL serialization."""

    def test_serialize_valid_sedols(self) -> None:
        """Test serializing valid SEDOLs."""
        assert serialize_sedol("0263494") == "0263494"

    def test_serialize_case_insensitive(self) -> None:
        """Test SEDOL serialization is case-insensitive."""
        assert serialize_sedol("0263494") == "0263494"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_sedol(None) is None

    def test_serialize_invalid_sedol(self) -> None:
        """Test serializing invalid SEDOLs raises error."""
        # Wrong length
        with pytest.raises(GraphQLError, match="Invalid SEDOL"):
            serialize_sedol("026349")


class TestSEDOLParsing:
    """Test SEDOL parsing from variables."""

    def test_parse_valid_sedol(self) -> None:
        """Test parsing valid SEDOLs."""
        assert parse_sedol_value("0263494") == "0263494"

    def test_parse_invalid_sedol(self) -> None:
        """Test parsing invalid SEDOLs raises error."""
        with pytest.raises(GraphQLError, match="Invalid SEDOL"):
            parse_sedol_value("026349")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="SEDOL must be a string"):
            parse_sedol_value(123)


class TestSEDOLField:
    """Test SEDOLField class."""

    def test_create_valid_sedol_field(self) -> None:
        """Test creating SEDOLField with valid values."""
        sedol = SEDOLField("0263494")
        assert sedol == "0263494"

    def test_create_invalid_sedol_field(self) -> None:
        """Test creating SEDOLField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid SEDOL"):
            SEDOLField("026349")


class TestSEDOLLiteralParsing:
    """Test parsing SEDOL from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid SEDOL literals."""
        assert parse_sedol_literal(StringValueNode(value="0263494")) == "0263494"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid SEDOL format literals."""
        with pytest.raises(GraphQLError, match="Invalid SEDOL"):
            parse_sedol_literal(StringValueNode(value="026349"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="SEDOL must be a string"):
            parse_sedol_literal(IntValueNode(value="123"))
