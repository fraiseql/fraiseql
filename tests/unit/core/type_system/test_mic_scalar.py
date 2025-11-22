"""Tests for MIC scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.mic import (
    MICField,
    parse_mic_literal,
    parse_mic_value,
    serialize_mic,
)


@pytest.mark.unit
class TestMICSerialization:
    """Test MIC serialization."""

    def test_serialize_valid_mics(self) -> None:
        """Test serializing valid MICs."""
        assert serialize_mic("XNYS") == "XNYS"
        assert serialize_mic("XNAS") == "XNAS"
        assert serialize_mic("XLON") == "XLON"

    def test_serialize_case_insensitive(self) -> None:
        """Test MIC serialization is case-insensitive."""
        assert serialize_mic("xnys") == "XNYS"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_mic(None) is None

    def test_serialize_invalid_mic(self) -> None:
        """Test serializing invalid MICs raises error."""
        # Wrong length
        with pytest.raises(GraphQLError, match="Invalid MIC"):
            serialize_mic("XNYS1")


class TestMICParsing:
    """Test MIC parsing from variables."""

    def test_parse_valid_mic(self) -> None:
        """Test parsing valid MICs."""
        assert parse_mic_value("XNYS") == "XNYS"

    def test_parse_invalid_mic(self) -> None:
        """Test parsing invalid MICs raises error."""
        with pytest.raises(GraphQLError, match="Invalid MIC"):
            parse_mic_value("XNYS1")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="MIC must be a string"):
            parse_mic_value(123)


class TestMICField:
    """Test MICField class."""

    def test_create_valid_mic_field(self) -> None:
        """Test creating MICField with valid values."""
        mic = MICField("XNYS")
        assert mic == "XNYS"

    def test_create_invalid_mic_field(self) -> None:
        """Test creating MICField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid MIC"):
            MICField("XNYS1")


class TestMICLiteralParsing:
    """Test parsing MIC from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid MIC literals."""
        assert parse_mic_literal(StringValueNode(value="XNYS")) == "XNYS"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid MIC format literals."""
        with pytest.raises(GraphQLError, match="Invalid MIC"):
            parse_mic_literal(StringValueNode(value="XNYS1"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="MIC must be a string"):
            parse_mic_literal(IntValueNode(value="123"))
