"""Tests for CUSIP scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.cusip import (
    CUSIPField,
    parse_cusip_literal,
    parse_cusip_value,
    serialize_cusip,
)


@pytest.mark.unit
class TestCUSIPSerialization:
    """Test CUSIP serialization."""

    def test_serialize_valid_cusips(self) -> None:
        """Test serializing valid CUSIPs."""
        assert serialize_cusip("037833100") == "037833100"

    def test_serialize_case_insensitive(self) -> None:
        """Test CUSIP serialization is case-insensitive."""
        assert serialize_cusip("037833100") == "037833100"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_cusip(None) is None

    def test_serialize_invalid_cusip(self) -> None:
        """Test serializing invalid CUSIPs raises error."""
        # Wrong length
        with pytest.raises(GraphQLError, match="Invalid CUSIP"):
            serialize_cusip("03783310")


class TestCUSIPParsing:
    """Test CUSIP parsing from variables."""

    def test_parse_valid_cusip(self) -> None:
        """Test parsing valid CUSIPs."""
        assert parse_cusip_value("037833100") == "037833100"

    def test_parse_invalid_cusip(self) -> None:
        """Test parsing invalid CUSIPs raises error."""
        with pytest.raises(GraphQLError, match="Invalid CUSIP"):
            parse_cusip_value("03783310")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="CUSIP must be a string"):
            parse_cusip_value(123)


class TestCUSIPField:
    """Test CUSIPField class."""

    def test_create_valid_cusip_field(self) -> None:
        """Test creating CUSIPField with valid values."""
        cusip = CUSIPField("037833100")
        assert cusip == "037833100"

    def test_create_invalid_cusip_field(self) -> None:
        """Test creating CUSIPField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid CUSIP"):
            CUSIPField("03783310")


class TestCUSIPLiteralParsing:
    """Test parsing CUSIP from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid CUSIP literals."""
        assert parse_cusip_literal(StringValueNode(value="037833100")) == "037833100"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid CUSIP format literals."""
        with pytest.raises(GraphQLError, match="Invalid CUSIP"):
            parse_cusip_literal(StringValueNode(value="03783310"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="CUSIP must be a string"):
            parse_cusip_literal(IntValueNode(value="123"))
