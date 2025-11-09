"""Tests for HashSHA256 scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.hash_sha256 import (
    HashSHA256Field,
    parse_hash_sha256_literal,
    parse_hash_sha256_value,
    serialize_hash_sha256,
)


@pytest.mark.unit
class TestHashSHA256Serialization:
    """Test SHA256 hash serialization."""

    def test_serialize_valid_sha256_hashes(self) -> None:
        """Test serializing valid SHA256 hashes."""
        # Empty string hash
        assert (
            serialize_hash_sha256(
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            )
            == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        )
        # Random hash
        assert (
            serialize_hash_sha256(
                "a665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3"
            )
            == "a665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3"
        )
        # Uppercase
        assert (
            serialize_hash_sha256(
                "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"
            )
            == "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"
        )
        # Mixed case
        assert (
            serialize_hash_sha256(
                "A665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3"
            )
            == "A665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3"
        )

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_hash_sha256(None) is None

    def test_serialize_invalid_sha256_hashes(self) -> None:
        """Test serializing invalid SHA256 hashes raises error."""
        # Too short
        with pytest.raises(GraphQLError, match="Invalid SHA256 hash"):
            serialize_hash_sha256("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85")

        # Too long
        with pytest.raises(GraphQLError, match="Invalid SHA256 hash"):
            serialize_hash_sha256(
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b8555"
            )

        # Contains invalid characters
        with pytest.raises(GraphQLError, match="Invalid SHA256 hash"):
            serialize_hash_sha256(
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85g"
            )

        with pytest.raises(GraphQLError, match="Invalid SHA256 hash"):
            serialize_hash_sha256(
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85!"
            )

        # Empty
        with pytest.raises(GraphQLError, match="Invalid SHA256 hash"):
            serialize_hash_sha256("")


class TestHashSHA256Parsing:
    """Test SHA256 hash parsing from variables."""

    def test_parse_valid_sha256_hashes(self) -> None:
        """Test parsing valid SHA256 hashes."""
        assert (
            parse_hash_sha256_value(
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            )
            == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        )
        assert (
            parse_hash_sha256_value(
                "A665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3"
            )
            == "A665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3"
        )

    def test_parse_invalid_sha256_hashes(self) -> None:
        """Test parsing invalid SHA256 hashes raises error."""
        with pytest.raises(GraphQLError, match="Invalid SHA256 hash"):
            parse_hash_sha256_value(
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85"
            )

        with pytest.raises(GraphQLError, match="Invalid SHA256 hash"):
            parse_hash_sha256_value(
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b8555"
            )

        with pytest.raises(GraphQLError, match="Invalid SHA256 hash"):
            parse_hash_sha256_value(
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85g"
            )

        with pytest.raises(GraphQLError, match="Invalid SHA256 hash"):
            parse_hash_sha256_value("")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="SHA256 hash must be a string"):
            parse_hash_sha256_value(123)

        with pytest.raises(GraphQLError, match="SHA256 hash must be a string"):
            parse_hash_sha256_value(None)

        with pytest.raises(GraphQLError, match="SHA256 hash must be a string"):
            parse_hash_sha256_value(
                ["e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"]
            )


class TestHashSHA256Field:
    """Test HashSHA256Field class."""

    def test_create_valid_sha256_hash_field(self) -> None:
        """Test creating HashSHA256Field with valid values."""
        hash_field = HashSHA256Field(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        )
        assert hash_field == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        assert isinstance(hash_field, str)

        # Uppercase
        hash_field = HashSHA256Field(
            "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"
        )
        assert hash_field == "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"

    def test_create_invalid_sha256_hash_field(self) -> None:
        """Test creating HashSHA256Field with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid SHA256 hash"):
            HashSHA256Field("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85")

        with pytest.raises(ValueError, match="Invalid SHA256 hash"):
            HashSHA256Field("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b8555")

        with pytest.raises(ValueError, match="Invalid SHA256 hash"):
            HashSHA256Field("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85g")


class TestHashSHA256LiteralParsing:
    """Test parsing SHA256 hash from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid SHA256 hash literals."""
        assert (
            parse_hash_sha256_literal(
                StringValueNode(
                    value="e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                )
            )
            == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        )
        assert (
            parse_hash_sha256_literal(
                StringValueNode(
                    value="A665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3"
                )
            )
            == "A665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3"
        )

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid SHA256 hash format literals."""
        with pytest.raises(GraphQLError, match="Invalid SHA256 hash"):
            parse_hash_sha256_literal(
                StringValueNode(
                    value="e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85"
                )
            )

        with pytest.raises(GraphQLError, match="Invalid SHA256 hash"):
            parse_hash_sha256_literal(
                StringValueNode(
                    value="e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b8555"
                )
            )

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="SHA256 hash must be a string"):
            parse_hash_sha256_literal(IntValueNode(value="123"))
