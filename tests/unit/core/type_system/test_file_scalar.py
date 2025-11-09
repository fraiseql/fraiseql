"""Tests for File scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.file import (
    FileField,
    parse_file_literal,
    parse_file_value,
    serialize_file,
)


@pytest.mark.unit
class TestFileSerialization:
    """Test file serialization."""

    def test_serialize_valid_files(self) -> None:
        """Test serializing valid file URLs/paths."""
        # URLs
        assert (
            serialize_file("https://example.com/document.pdf") == "https://example.com/document.pdf"
        )
        assert (
            serialize_file("http://cdn.example.com/file.zip") == "http://cdn.example.com/file.zip"
        )

        # File paths
        assert serialize_file("/uploads/document.pdf") == "/uploads/document.pdf"
        assert serialize_file("files/archive.zip") == "files/archive.zip"
        assert serialize_file("./documents/report.docx") == "./documents/report.docx"
        assert serialize_file("../files/data.csv") == "../files/data.csv"

        # Files without extensions
        assert serialize_file("README") == "README"
        assert serialize_file("/etc/passwd") == "/etc/passwd"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_file(None) is None

    def test_serialize_invalid_file(self) -> None:
        """Test serializing invalid files raises error."""
        # Control characters
        with pytest.raises(GraphQLError, match="Invalid file"):
            serialize_file("file\x00name.txt")

        with pytest.raises(GraphQLError, match="Invalid file"):
            serialize_file("file\x7fname.txt")

        # Empty
        with pytest.raises(GraphQLError, match="Invalid file"):
            serialize_file("")


class TestFileParsing:
    """Test file parsing from variables."""

    def test_parse_valid_file(self) -> None:
        """Test parsing valid files."""
        assert (
            parse_file_value("https://example.com/document.pdf")
            == "https://example.com/document.pdf"
        )
        assert parse_file_value("/uploads/document.pdf") == "/uploads/document.pdf"
        assert parse_file_value("README") == "README"

    def test_parse_invalid_file(self) -> None:
        """Test parsing invalid files raises error."""
        with pytest.raises(GraphQLError, match="Invalid file"):
            parse_file_value("file\x00name.txt")

        with pytest.raises(GraphQLError, match="Invalid file"):
            parse_file_value("")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="File must be a string"):
            parse_file_value(123)

        with pytest.raises(GraphQLError, match="File must be a string"):
            parse_file_value(None)

        with pytest.raises(GraphQLError, match="File must be a string"):
            parse_file_value(["document.pdf"])


class TestFileField:
    """Test FileField class."""

    def test_create_valid_file_field(self) -> None:
        """Test creating FileField with valid values."""
        file = FileField("https://example.com/document.pdf")
        assert file == "https://example.com/document.pdf"
        assert isinstance(file, str)

        file = FileField("/uploads/document.pdf")
        assert file == "/uploads/document.pdf"

        file = FileField("README")
        assert file == "README"

    def test_create_invalid_file_field(self) -> None:
        """Test creating FileField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid file"):
            FileField("file\x00name.txt")

        with pytest.raises(ValueError, match="Invalid file"):
            FileField("")


class TestFileLiteralParsing:
    """Test parsing file from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid file literals."""
        assert (
            parse_file_literal(StringValueNode(value="https://example.com/document.pdf"))
            == "https://example.com/document.pdf"
        )
        assert (
            parse_file_literal(StringValueNode(value="/uploads/document.pdf"))
            == "/uploads/document.pdf"
        )
        assert parse_file_literal(StringValueNode(value="README")) == "README"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid file format literals."""
        with pytest.raises(GraphQLError, match="Invalid file"):
            parse_file_literal(StringValueNode(value="file\x00name.txt"))

        with pytest.raises(GraphQLError, match="Invalid file"):
            parse_file_literal(StringValueNode(value=""))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="File must be a string"):
            parse_file_literal(IntValueNode(value="123"))
