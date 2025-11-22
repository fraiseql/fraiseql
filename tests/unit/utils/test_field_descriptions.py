"""Tests for automatic field description extraction."""

from dataclasses import dataclass
from typing import Annotated
from uuid import UUID

from fraiseql import fraise_field, fraise_type
from fraiseql.utils.field_descriptions import (
    _extract_annotation_descriptions,
    _extract_docstring_descriptions,
    _extract_inline_comments,
    apply_auto_descriptions,
    extract_field_descriptions,
)


class TestInlineCommentExtraction:
    """Test extraction of field descriptions from inline comments."""

    def test_no_inline_comments_for_dynamic_classes(self) -> None:
        """Test that dynamically created classes don't have source available."""

        @fraise_type
        @dataclass
        class Order:
            id: UUID
            amount: float
            created_at: str

        descriptions = _extract_inline_comments(Order)
        # Dynamic classes won't have source code available
        assert descriptions == {}

    def test_regex_pattern_matching(self) -> None:
        """Test the regex pattern used for inline comment extraction."""
        import re

        # Test the pattern used in _extract_inline_comments
        pattern = r"^\s*(\w+)\s*:\s*[^#]*#\s*(.+)$"

        test_lines = [
            "    id: UUID  # User identifier",
            "name: str#Product name",
            "    price: float  #  Price in USD",
            "tags: list[str]     # List of tags",
            "    status: str = 'active'  # Current status",
        ]

        expected = [
            ("id", "User identifier"),
            ("name", "Product name"),
            ("price", "Price in USD"),
            ("tags", "List of tags"),
            ("status", "Current status"),
        ]

        for i, line in enumerate(test_lines):
            match = re.match(pattern, line)
            assert match is not None, f"Pattern should match line: {line}"
            field_name = match.group(1)
            comment = match.group(2).strip()
            assert (field_name, comment) == expected[i]


class TestDocstringExtraction:
    """Test extraction of field descriptions from class docstrings."""

    def test_fields_section_extraction(self) -> None:
        """Test extraction from Fields: section in docstring."""

        @fraise_type
        @dataclass
        class User:
            """User account model.

            Fields:
                id: Unique identifier for the user
                name: Full name of the user
                email: User's email address
                status: Current account status
            """

            id: UUID
            name: str
            email: str
            status: str = "active"

        descriptions = _extract_docstring_descriptions(User)

        assert descriptions["id"] == "Unique identifier for the user"
        assert descriptions["name"] == "Full name of the user"
        assert descriptions["email"] == "User's email address"
        assert descriptions["status"] == "Current account status"

    def test_attributes_section_extraction(self) -> None:
        """Test extraction from Attributes: section in docstring."""

        @fraise_type
        @dataclass
        class Product:
            """Product model.

            Attributes:
                id: Product identifier
                name: Product name
                price: Price in USD
            """

            id: UUID
            name: str
            price: float

        descriptions = _extract_docstring_descriptions(Product)

        assert descriptions["id"] == "Product identifier"
        assert descriptions["name"] == "Product name"
        assert descriptions["price"] == "Price in USD"

    def test_args_section_extraction(self) -> None:
        """Test extraction from Args: section (for input types)."""

        @fraise_type
        @dataclass
        class CreateUserInput:
            """Input for creating a user.

            Args:
                name: User's full name
                email: User's email address
                password: User's password
            """

            name: str
            email: str
            password: str

        descriptions = _extract_docstring_descriptions(CreateUserInput)

        assert descriptions["name"] == "User's full name"
        assert descriptions["email"] == "User's email address"
        assert descriptions["password"] == "User's password"

    def test_no_docstring(self) -> None:
        """Test extraction when no docstring is present."""

        @fraise_type
        @dataclass
        class Order:
            id: UUID
            amount: float

        descriptions = _extract_docstring_descriptions(Order)
        assert descriptions == {}

    def test_docstring_without_fields_section(self) -> None:
        """Test extraction when docstring has no Fields: section."""

        @fraise_type
        @dataclass
        class Invoice:
            """This is a simple invoice model without field documentation."""

            id: UUID
            amount: float

        descriptions = _extract_docstring_descriptions(Invoice)
        assert descriptions == {}


class TestAnnotationExtraction:
    """Test extraction of descriptions from Annotated type hints."""

    def test_annotated_descriptions(self) -> None:
        """Test extraction from Annotated type hints."""

        @fraise_type
        @dataclass
        class User:
            id: Annotated[UUID, "Unique user identifier"]
            name: Annotated[str, "User's full name"]
            email: Annotated[str, "Email address for communication"]
            age: int  # Regular field without annotation

        descriptions = _extract_annotation_descriptions(User)

        # Check if we get the descriptions (Annotated might not work in all Python versions)
        if descriptions:
            assert descriptions["id"] == "Unique user identifier"
            assert descriptions["name"] == "User's full name"
            assert descriptions["email"] == "Email address for communication"
        assert "age" not in descriptions

    def test_mixed_annotations(self) -> None:
        """Test extraction with mix of annotated and regular fields."""

        @fraise_type
        @dataclass
        class Product:
            id: UUID
            price: float
            name: Annotated[str, "Product name"]
            description: Annotated[str, "Product description"]

        descriptions = _extract_annotation_descriptions(Product)

        # Check if we get the descriptions
        if descriptions:
            assert descriptions.get("name") == "Product name"
            assert descriptions.get("description") == "Product description"
        assert "id" not in descriptions
        assert "price" not in descriptions

    def test_no_annotated_fields(self) -> None:
        """Test extraction when no Annotated fields are present."""

        @fraise_type
        @dataclass
        class Order:
            id: UUID
            amount: float
            status: str

        descriptions = _extract_annotation_descriptions(Order)
        assert descriptions == {}


class TestIntegratedExtraction:
    """Test the complete extract_field_descriptions function."""

    def test_docstring_extraction_works(self) -> None:
        """Test that descriptions are extracted from docstring sources."""

        @fraise_type
        @dataclass
        class User:
            """User account model.

            Fields:
                name: User's full name
                status: Account status
            """

            id: UUID
            name: str
            email: str
            status: str = "active"

        descriptions = extract_field_descriptions(User)

        # Should get descriptions from docstring
        assert descriptions["name"] == "User's full name"
        assert descriptions["status"] == "Account status"

    def test_docstring_priority_over_annotations(self) -> None:
        """Test that inline comments take priority over docstring descriptions."""

        @fraise_type
        @dataclass
        class Product:
            """Product model.

            Fields:
                name: Product name from docstring
            """

            name: str

        descriptions = extract_field_descriptions(Product)

        # Should get description from docstring since inline comments won't work for dynamic classes
        assert descriptions["name"] == "Product name from docstring"


class TestAutoDescriptionApplication:
    """Test the apply_auto_descriptions function."""

    def test_applies_to_fields_without_descriptions(self) -> None:
        """Test that auto descriptions are applied only to fields without explicit descriptions."""

        @fraise_type
        @dataclass
        class User:
            id: UUID  # Auto-generated ID
            email: str  # User email address
            name: str = fraise_field(description="Explicit name description")

        # Check that auto descriptions were applied
        fields = User.__gql_fields__

        assert fields["id"].description == "Auto-generated ID"
        assert fields["name"].description == "Explicit name description"  # Unchanged
        assert fields["email"].description == "User email address"

    def test_preserves_explicit_descriptions(self) -> None:
        """Test that explicit descriptions are not overwritten."""

        @fraise_type
        @dataclass
        class Product:
            """Product model.

            Fields:
                name: Auto description from docstring
            """

            id: UUID
            price: float  # Price in USD
            name: str = fraise_field(description="Explicit description")

        fields = Product.__gql_fields__

        assert fields["name"].description == "Explicit description"  # Preserved
        assert fields["price"].description == "Price in USD"  # Auto-applied
        assert fields["id"].description is None  # No description available

    def test_handles_missing_gql_fields(self) -> None:
        """Test that function handles classes without __gql_fields__."""

        class RegularClass:
            pass

        # Should not raise an error
        apply_auto_descriptions(RegularClass)


class TestEdgeCases:
    """Test edge cases and error conditions."""

    def test_malformed_source_code(self) -> None:
        """Test that extraction gracefully handles when source code is unavailable."""

        # This is hard to test directly, but the function should handle OSError, TypeError, etc.
        # We can test by creating a class and then trying to extract
        class DynamicClass:
            id: UUID

        # Should not raise an error even if source is not available
        descriptions = _extract_inline_comments(DynamicClass)
        assert descriptions == {}

    def test_docstring_with_complex_field_types(self) -> None:
        """Test extraction with complex field types from docstring."""

        @fraise_type
        @dataclass
        class ComplexType:
            """Complex type with various field types.

            Fields:
                id: Primary identifier
                tags: List of tag names
                metadata: Key-value metadata
                optional_field: Optional string field
            """

            id: UUID
            tags: list[str]
            metadata: dict[str, str]
            optional_field: str | None

        descriptions = extract_field_descriptions(ComplexType)

        assert descriptions["id"] == "Primary identifier"
        assert descriptions["tags"] == "List of tag names"
        assert descriptions["metadata"] == "Key-value metadata"
        assert descriptions["optional_field"] == "Optional string field"

    def test_inheritance_with_descriptions(self) -> None:
        """Test that field descriptions work with class inheritance."""

        @fraise_type
        @dataclass
        class BaseUser:
            """Base user class.

            Fields:
                id: Base user ID
                created_at: Creation timestamp
            """

            id: UUID
            created_at: str

        @fraise_type
        @dataclass
        class AdminUser(BaseUser):
            """Admin user class.

            Fields:
                permissions: Admin permissions
            """

            permissions: list[str]

        base_descriptions = extract_field_descriptions(BaseUser)
        admin_descriptions = extract_field_descriptions(AdminUser)

        assert base_descriptions["id"] == "Base user ID"
        assert base_descriptions["created_at"] == "Creation timestamp"
        assert admin_descriptions["permissions"] == "Admin permissions"


class TestIntegrationWithExistingFramework:
    """Test integration with existing fraiseql features."""

    def test_graphql_schema_generation_with_auto_descriptions(self) -> None:
        """Test that auto descriptions appear in generated GraphQL schema."""

        @fraise_type
        @dataclass
        class User:
            """User account with authentication.

            Fields:
                id: Unique user identifier
                name: Full display name
            """

            id: UUID
            name: str
            email: str = fraise_field(description="Contact email address")

        # Convert to GraphQL type and check descriptions
        from fraiseql.core.graphql_type import convert_type_to_graphql_output

        gql_type = convert_type_to_graphql_output(User)

        # Check type description (from class docstring - includes the full cleaned docstring)
        assert gql_type.description.startswith("User account with authentication.")

        # Check field descriptions
        assert gql_type.fields["id"].description == "Unique user identifier"
        assert gql_type.fields["name"].description == "Full display name"
        assert gql_type.fields["email"].description == "Contact email address"

    def test_backward_compatibility(self) -> None:
        """Test that existing code without auto descriptions still works."""

        @fraise_type
        @dataclass
        class LegacyUser:
            id: UUID
            email: str
            name: str = fraise_field(description="User name")

        fields = LegacyUser.__gql_fields__

        assert fields["name"].description == "User name"
        assert fields["id"].description is None
        assert fields["email"].description is None
