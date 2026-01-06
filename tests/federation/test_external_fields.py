"""Tests for external field management in type extensions."""

from fraiseql.federation.decorators import extend_entity, external
from fraiseql.federation.external_fields import (
    ExternalFieldInfo,
    ExternalFieldManager,
    extract_external_fields,
)


class TestExternalFieldInfo:
    """Tests for ExternalFieldInfo class."""

    def test_create_external_field_info(self) -> None:
        """Test creating external field info."""
        info = ExternalFieldInfo("id", str, is_required=True)
        assert info.field_name == "id"
        assert info.type_annotation is str
        assert info.is_required is True

    def test_external_field_info_optional(self) -> None:
        """Test external field info with optional field."""
        info = ExternalFieldInfo("description", str, is_required=False)
        assert info.field_name == "description"
        assert info.is_required is False

    def test_external_field_info_repr(self) -> None:
        """Test repr of external field info."""
        info = ExternalFieldInfo("id", str, is_required=True)
        repr_str = repr(info)
        assert "id" in repr_str
        assert "required=True" in repr_str


class TestExternalFieldManager:
    """Tests for ExternalFieldManager class."""

    def test_manager_initialization(self) -> None:
        """Test creating a manager."""
        manager = ExternalFieldManager()
        assert manager.get_external_fields() == []
        assert manager.get_new_fields() == []

    def test_mark_external(self) -> None:
        """Test marking fields as external."""
        manager = ExternalFieldManager()
        manager.mark_external("id", str, is_required=True)
        manager.mark_external("name", str, is_required=True)

        assert manager.get_external_fields() == ["id", "name"]
        assert manager.is_external("id")
        assert manager.is_external("name")

    def test_mark_new(self) -> None:
        """Test marking fields as new (local)."""
        manager = ExternalFieldManager()
        manager.mark_new("reviews")
        manager.mark_new("rating")

        assert set(manager.get_new_fields()) == {"rating", "reviews"}
        assert manager.is_new("reviews")
        assert manager.is_new("rating")

    def test_mixed_fields(self) -> None:
        """Test manager with both external and new fields."""
        manager = ExternalFieldManager()
        manager.mark_external("id", str)
        manager.mark_external("name", str)
        manager.mark_new("reviews")
        manager.mark_new("average_rating")

        assert manager.get_external_fields() == ["id", "name"]
        assert set(manager.get_new_fields()) == {"average_rating", "reviews"}
        assert manager.is_external("id")
        assert not manager.is_external("reviews")
        assert manager.is_new("reviews")
        assert not manager.is_new("id")

    def test_validate_all_fields_complete(self) -> None:
        """Test validation when all fields are categorized."""
        manager = ExternalFieldManager()
        manager.mark_external("id", str)
        manager.mark_external("name", str)
        manager.mark_new("reviews")

        all_fields = {"id", "name", "reviews"}
        uncategorized = manager.validate_all_fields(all_fields)
        assert uncategorized == []

    def test_validate_all_fields_incomplete(self) -> None:
        """Test validation with uncategorized fields."""
        manager = ExternalFieldManager()
        manager.mark_external("id", str)
        manager.mark_new("reviews")

        all_fields = {"id", "name", "reviews", "description"}
        uncategorized = manager.validate_all_fields(all_fields)
        assert set(uncategorized) == {"description", "name"}

    def test_manager_repr(self) -> None:
        """Test repr of manager."""
        manager = ExternalFieldManager()
        manager.mark_external("id", str)
        manager.mark_new("reviews")

        repr_str = repr(manager)
        assert "external=1" in repr_str
        assert "new=1" in repr_str


class TestExtractExternalFields:
    """Tests for extract_external_fields helper function."""

    def test_extract_no_external(self) -> None:
        """Test extracting from class with no external fields."""

        class User:
            id: str
            name: str
            email: str

        external_map, others = extract_external_fields(User)
        assert external_map == {}
        assert others == {"id", "name", "email"}

    def test_extract_with_external(self) -> None:
        """Test extracting from class with external fields."""

        @extend_entity(key="id")
        class Product:
            id: str = external()
            name: str = external()
            reviews: list

        external_map, others = extract_external_fields(Product)
        assert set(external_map.keys()) == {"id", "name"}
        assert "reviews" in others

    def test_extract_mixed_fields(self) -> None:
        """Test extracting with mixed external and new fields."""

        @extend_entity(key="id")
        class Post:
            id: str = external()
            title: str = external()
            content: str = external()
            comments: list
            likes_count: int

        external_map, others = extract_external_fields(Post)
        assert set(external_map.keys()) == {"id", "title", "content"}
        assert set(others) == {"comments", "likes_count"}

    def test_extract_all_external(self) -> None:
        """Test extracting when all fields are external."""

        @extend_entity(key="id")
        class User:
            id: str = external()
            name: str = external()
            email: str = external()

        external_map, others = extract_external_fields(User)
        assert set(external_map.keys()) == {"id", "name", "email"}
        assert others == set()

    def test_extract_all_new(self) -> None:
        """Test extracting when all fields are new."""

        class Review:
            text: str
            rating: int
            author: str

        external_map, others = extract_external_fields(Review)
        assert external_map == {}
        assert others == {"text", "rating", "author"}


class TestExtendEntityIntegration:
    """Integration tests for @extend_entity with external fields."""

    def test_extend_entity_marks_external(self) -> None:
        """Test that @extend_entity properly marks external fields."""

        @extend_entity(key="id")
        class Product:
            id: str = external()
            name: str = external()
            reviews: list

        external_map, _others = extract_external_fields(Product)
        assert "id" in external_map
        assert "name" in external_map
        assert "reviews" not in external_map

    def test_extend_entity_with_no_external(self) -> None:
        """Test @extend_entity with no explicit external fields."""

        @extend_entity(key="id")
        class Comment:
            id: str  # Key field must be present
            text: str
            rating: int

        external_map, others = extract_external_fields(Comment)
        # All fields are new since none explicitly marked as external
        assert external_map == {}
        assert set(others) == {"id", "text", "rating"}

    def test_extend_entity_registry(self) -> None:
        """Test that extended entities are registered."""
        from fraiseql.federation.decorators import get_entity_metadata

        @extend_entity(key="id")
        class Review:
            id: str = external()
            text: str = external()
            rating: int

        metadata = get_entity_metadata("Review")
        assert metadata is not None
        assert metadata.is_extension is True
        assert "id" in metadata.external_fields
        assert "text" in metadata.external_fields

    def test_composite_key_extension(self) -> None:
        """Test extending entity with composite key."""

        @extend_entity(key=["org_id", "user_id"])
        class OrgUser:
            org_id: str = external()
            user_id: str = external()
            role: str = external()
            permissions: list

        external_map, others = extract_external_fields(OrgUser)
        assert set(external_map.keys()) == {"org_id", "user_id", "role"}
        assert "permissions" in others


class TestExternalFieldValidation:
    """Tests for validation of external field usage."""

    def test_external_on_valid_type(self) -> None:
        """Test external() works on properly annotated fields."""

        @extend_entity(key="id")
        class Product:
            id: str = external()
            price: float = external()

        external_map, _others = extract_external_fields(Product)
        assert set(external_map.keys()) == {"id", "price"}

    def test_external_fields_in_metadata(self) -> None:
        """Test that external fields are tracked in metadata."""
        from fraiseql.federation.decorators import get_entity_metadata

        @extend_entity(key="id")
        class Article:
            id: str = external()
            title: str = external()
            content: str
            author: str = external()

        metadata = get_entity_metadata("Article")
        assert "id" in metadata.external_fields
        assert "title" in metadata.external_fields
        assert "author" in metadata.external_fields
        assert "content" not in metadata.external_fields


class TestExternalFieldUseCases:
    """Real-world use cases for external fields."""

    def test_product_review_extension(self) -> None:
        """Test extending Product type with reviews."""

        @extend_entity(key="id")
        class Product:
            # External fields from products subgraph
            id: str = external()
            name: str = external()
            price: float = external()

            # New fields in reviews subgraph
            reviews: list
            average_rating: float

        external_map, new = extract_external_fields(Product)
        assert set(external_map.keys()) == {"id", "name", "price"}
        assert set(new) == {"reviews", "average_rating"}

    def test_user_posts_extension(self) -> None:
        """Test extending User type with posts."""

        @extend_entity(key="id")
        class User:
            # External fields from users subgraph
            id: str = external()
            username: str = external()
            email: str = external()

            # New fields in posts subgraph
            posts: list
            post_count: int

        external_map, new = extract_external_fields(User)
        assert set(external_map.keys()) == {"id", "username", "email"}
        assert set(new) == {"posts", "post_count"}

    def test_multi_subgraph_extension(self) -> None:
        """Test entity extended by multiple subgraphs."""

        # First subgraph extends with analytics
        @extend_entity(key="id")
        class Article:
            id: str = external()
            title: str = external()
            content: str = external()

            # Analytics fields
            views: int
            shares: int

        external_map, new = extract_external_fields(Article)
        assert set(external_map.keys()) == {"id", "title", "content"}
        assert set(new) == {"views", "shares"}
