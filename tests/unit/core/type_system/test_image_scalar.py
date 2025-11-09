"""Tests for Image scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.image import (
    ImageField,
    parse_image_literal,
    parse_image_value,
    serialize_image,
)


@pytest.mark.unit
class TestImageSerialization:
    """Test image serialization."""

    def test_serialize_valid_images(self) -> None:
        """Test serializing valid image URLs/paths."""
        # URLs
        assert serialize_image("https://example.com/image.jpg") == "https://example.com/image.jpg"
        assert (
            serialize_image("http://cdn.example.com/photo.png")
            == "http://cdn.example.com/photo.png"
        )

        # File paths
        assert serialize_image("/uploads/avatar.gif") == "/uploads/avatar.gif"
        assert serialize_image("images/photo.webp") == "images/photo.webp"
        assert serialize_image("./assets/picture.svg") == "./assets/picture.svg"

        # Different extensions
        assert serialize_image("image.JPEG") == "image.JPEG"
        assert serialize_image("photo.PNG") == "photo.PNG"

    def test_serialize_none(self) -> None:
        """Test serializing None returns None."""
        assert serialize_image(None) is None

    def test_serialize_invalid_image(self) -> None:
        """Test serializing invalid images raises error."""
        # Wrong extension
        with pytest.raises(GraphQLError, match="Invalid image"):
            serialize_image("document.pdf")

        with pytest.raises(GraphQLError, match="Invalid image"):
            serialize_image("https://example.com/file.txt")

        # No extension
        with pytest.raises(GraphQLError, match="Invalid image"):
            serialize_image("image")

        # Empty
        with pytest.raises(GraphQLError, match="Invalid image"):
            serialize_image("")


class TestImageParsing:
    """Test image parsing from variables."""

    def test_parse_valid_image(self) -> None:
        """Test parsing valid images."""
        assert parse_image_value("https://example.com/image.jpg") == "https://example.com/image.jpg"
        assert parse_image_value("/uploads/avatar.png") == "/uploads/avatar.png"
        assert parse_image_value("photo.gif") == "photo.gif"

    def test_parse_invalid_image(self) -> None:
        """Test parsing invalid images raises error."""
        with pytest.raises(GraphQLError, match="Invalid image"):
            parse_image_value("document.pdf")

        with pytest.raises(GraphQLError, match="Invalid image"):
            parse_image_value("image")

        with pytest.raises(GraphQLError, match="Invalid image"):
            parse_image_value("")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Image must be a string"):
            parse_image_value(123)

        with pytest.raises(GraphQLError, match="Image must be a string"):
            parse_image_value(None)

        with pytest.raises(GraphQLError, match="Image must be a string"):
            parse_image_value(["image.jpg"])


class TestImageField:
    """Test ImageField class."""

    def test_create_valid_image_field(self) -> None:
        """Test creating ImageField with valid values."""
        image = ImageField("https://example.com/image.jpg")
        assert image == "https://example.com/image.jpg"
        assert isinstance(image, str)

        image = ImageField("/uploads/avatar.png")
        assert image == "/uploads/avatar.png"

    def test_create_invalid_image_field(self) -> None:
        """Test creating ImageField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid image"):
            ImageField("document.pdf")

        with pytest.raises(ValueError, match="Invalid image"):
            ImageField("image")

        with pytest.raises(ValueError, match="Invalid image"):
            ImageField("")


class TestImageLiteralParsing:
    """Test parsing image from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid image literals."""
        assert (
            parse_image_literal(StringValueNode(value="https://example.com/image.jpg"))
            == "https://example.com/image.jpg"
        )
        assert (
            parse_image_literal(StringValueNode(value="/uploads/avatar.png"))
            == "/uploads/avatar.png"
        )

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid image format literals."""
        with pytest.raises(GraphQLError, match="Invalid image"):
            parse_image_literal(StringValueNode(value="document.pdf"))

        with pytest.raises(GraphQLError, match="Invalid image"):
            parse_image_literal(StringValueNode(value="image"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Image must be a string"):
            parse_image_literal(IntValueNode(value="123"))
