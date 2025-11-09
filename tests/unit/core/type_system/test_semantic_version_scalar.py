"""Tests for SemanticVersion scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.semantic_version import (
    SemanticVersionField,
    parse_semantic_version_literal,
    parse_semantic_version_value,
    serialize_semantic_version,
)


@pytest.mark.unit
class TestSemanticVersionSerialization:
    """Test semantic version serialization."""

    def test_serialize_valid_semantic_versions(self) -> None:
        """Test serializing valid semantic versions."""
        assert serialize_semantic_version("1.0.0") == "1.0.0"
        assert serialize_semantic_version("2.3.4") == "2.3.4"
        assert serialize_semantic_version("2.3.4-alpha.1") == "2.3.4-alpha.1"
        assert (
            serialize_semantic_version("3.0.0-beta+20130313144700") == "3.0.0-beta+20130313144700"
        )
        assert serialize_semantic_version("0.1.0") == "0.1.0"
        assert serialize_semantic_version("10.20.30-rc.1+build.2") == "10.20.30-rc.1+build.2"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_semantic_version(None) is None

    def test_serialize_invalid_semantic_versions(self) -> None:
        """Test serializing invalid semantic versions raises error."""
        # Missing parts
        with pytest.raises(GraphQLError, match="Invalid semantic version"):
            serialize_semantic_version("1.0")

        with pytest.raises(GraphQLError, match="Invalid semantic version"):
            serialize_semantic_version("1")

        # Leading zeros
        with pytest.raises(GraphQLError, match="Invalid semantic version"):
            serialize_semantic_version("01.0.0")

        with pytest.raises(GraphQLError, match="Invalid semantic version"):
            serialize_semantic_version("1.02.0")

        # Invalid characters
        with pytest.raises(GraphQLError, match="Invalid semantic version"):
            serialize_semantic_version("1.0.0-special!")

        # Empty prerelease identifiers
        with pytest.raises(GraphQLError, match="Invalid semantic version"):
            serialize_semantic_version("1.0.0-")

        with pytest.raises(GraphQLError, match="Invalid semantic version"):
            serialize_semantic_version("1.0.0-alpha.")

        # Empty build metadata
        with pytest.raises(GraphQLError, match="Invalid semantic version"):
            serialize_semantic_version("1.0.0+")

        with pytest.raises(GraphQLError, match="Invalid semantic version"):
            serialize_semantic_version("1.0.0-alpha+")

        # Invalid prerelease with uppercase
        with pytest.raises(GraphQLError, match="Invalid semantic version"):
            serialize_semantic_version("1.0.0-ALPHA")


class TestSemanticVersionParsing:
    """Test semantic version parsing from variables."""

    def test_parse_valid_semantic_versions(self) -> None:
        """Test parsing valid semantic versions."""
        assert parse_semantic_version_value("1.0.0") == "1.0.0"
        assert parse_semantic_version_value("2.3.4-alpha.1") == "2.3.4-alpha.1"
        assert (
            parse_semantic_version_value("3.0.0-beta+20130313144700") == "3.0.0-beta+20130313144700"
        )

    def test_parse_invalid_semantic_versions(self) -> None:
        """Test parsing invalid semantic versions raises error."""
        with pytest.raises(GraphQLError, match="Invalid semantic version"):
            parse_semantic_version_value("1.0")

        with pytest.raises(GraphQLError, match="Invalid semantic version"):
            parse_semantic_version_value("01.0.0")

        with pytest.raises(GraphQLError, match="Invalid semantic version"):
            parse_semantic_version_value("1.0.0-special!")

        with pytest.raises(GraphQLError, match="Invalid semantic version"):
            parse_semantic_version_value("")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Semantic version must be a string"):
            parse_semantic_version_value(123)

        with pytest.raises(GraphQLError, match="Semantic version must be a string"):
            parse_semantic_version_value(None)

        with pytest.raises(GraphQLError, match="Semantic version must be a string"):
            parse_semantic_version_value(["1.0.0"])


class TestSemanticVersionField:
    """Test SemanticVersionField class."""

    def test_create_valid_semantic_version_field(self) -> None:
        """Test creating SemanticVersionField with valid values."""
        version = SemanticVersionField("1.0.0")
        assert version == "1.0.0"
        assert isinstance(version, str)

        # With prerelease
        version = SemanticVersionField("2.3.4-alpha.1")
        assert version == "2.3.4-alpha.1"

        # With build metadata
        version = SemanticVersionField("3.0.0-beta+20130313144700")
        assert version == "3.0.0-beta+20130313144700"

    def test_create_invalid_semantic_version_field(self) -> None:
        """Test creating SemanticVersionField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid semantic version"):
            SemanticVersionField("1.0")

        with pytest.raises(ValueError, match="Invalid semantic version"):
            SemanticVersionField("01.0.0")

        with pytest.raises(ValueError, match="Invalid semantic version"):
            SemanticVersionField("1.0.0-special!")


class TestSemanticVersionLiteralParsing:
    """Test parsing semantic version from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid semantic version literals."""
        assert parse_semantic_version_literal(StringValueNode(value="1.0.0")) == "1.0.0"
        assert (
            parse_semantic_version_literal(StringValueNode(value="2.3.4-alpha.1"))
            == "2.3.4-alpha.1"
        )
        assert (
            parse_semantic_version_literal(StringValueNode(value="3.0.0-beta+20130313144700"))
            == "3.0.0-beta+20130313144700"
        )

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid semantic version format literals."""
        with pytest.raises(GraphQLError, match="Invalid semantic version"):
            parse_semantic_version_literal(StringValueNode(value="1.0"))

        with pytest.raises(GraphQLError, match="Invalid semantic version"):
            parse_semantic_version_literal(StringValueNode(value="01.0.0"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Semantic version must be a string"):
            parse_semantic_version_literal(IntValueNode(value="123"))
