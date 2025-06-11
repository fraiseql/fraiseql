"""Tests for FoundryAnalyzer."""

from typing import Optional

import pytest

from fraiseql import fraise_field, fraise_input
from fraiseql.extensions.testfoundry.analyzer import (
    FoundryAnalyzer,
)
from fraiseql.types.scalars.uuid import UUIDField


@fraise_input
class UserInput:
    """Test user input type."""

    email: str = fraise_field(description="User email address")
    name: str = fraise_field(description="User full name")
    bio: Optional[str] = fraise_field(description="User biography", default=None)
    avatar_url: Optional[str] = fraise_field(description="Avatar URL", default=None)
    is_active: bool = fraise_field(description="Active status", default=True)
    age: Optional[int] = fraise_field(description="User age", default=None)
    score: Optional[float] = fraise_field(description="User score", default=None)


@fraise_input
class PostInput:
    """Test post input type."""

    author_id: UUIDField = fraise_field(description="Post author ID")
    title: str = fraise_field(description="Post title")
    content: str = fraise_field(description="Post content")
    tags: Optional[list[str]] = fraise_field(description="Post tags", default=None)
    is_published: bool = fraise_field(description="Published status", default=False)


@fraise_input
class CommentInput:
    """Test comment input type."""

    post_id: UUIDField = fraise_field(description="Parent post ID")
    user_id: UUIDField = fraise_field(description="Comment author ID")
    content: str = fraise_field(description="Comment content")
    parent_comment_id: Optional[UUIDField] = fraise_field(
        description="Parent comment for replies", default=None
    )


class TestFoundryAnalyzer:
    """Test the FoundryAnalyzer class."""

    @pytest.fixture
    def analyzer(self):
        """Create an analyzer instance."""
        # SchemaBuilder is not actually used in the analyzer methods we're testing
        return FoundryAnalyzer(None)

    def test_analyze_simple_input_type(self, analyzer):
        """Test analyzing a simple input type."""
        mappings = analyzer.analyze_input_type(UserInput)

        # Should have mappings for all fields
        assert len(mappings) == 7  # email, name, bio, avatar_url, is_active, age, score

        # Check email field
        email_mapping = next(m for m in mappings if m.field_name == "email")
        assert email_mapping.input_type == "user"  # Input suffix is removed
        assert email_mapping.generator_type == "random"
        assert email_mapping.random_function == "testfoundry_random_email"
        assert email_mapping.required is True
        assert email_mapping.fk_mapping_key is None

        # Check optional field
        bio_mapping = next(m for m in mappings if m.field_name == "bio")
        assert bio_mapping.required is False
        assert (
            bio_mapping.random_function is None
        )  # No special function for generic text

        # Check URL field
        url_mapping = next(m for m in mappings if m.field_name == "avatar_url")
        assert url_mapping.random_function == "testfoundry_random_url"

        # Check boolean field
        active_mapping = next(m for m in mappings if m.field_name == "is_active")
        assert active_mapping.random_function == "testfoundry_random_boolean"

        # Check numeric fields
        age_mapping = next(m for m in mappings if m.field_name == "age")
        assert age_mapping.random_function == "testfoundry_random_integer"

        score_mapping = next(m for m in mappings if m.field_name == "score")
        assert score_mapping.random_function == "testfoundry_random_float"

    def test_analyze_input_with_fk(self, analyzer):
        """Test analyzing input type with foreign keys."""
        mappings = analyzer.analyze_input_type(PostInput)

        # Check FK field
        author_mapping = next(m for m in mappings if m.field_name == "author_id")
        assert author_mapping.generator_type == "resolve_fk"
        assert author_mapping.fk_mapping_key == "author_id"
        assert author_mapping.random_function is None  # FKs don't use random functions

        # Check array field
        tags_mapping = next(m for m in mappings if m.field_name == "tags")
        assert tags_mapping.random_function == "testfoundry_random_array"

    def test_analyze_complex_fk_patterns(self, analyzer):
        """Test analyzing various FK naming patterns."""
        mappings = analyzer.analyze_input_type(CommentInput)

        # Standard _id pattern
        post_mapping = next(m for m in mappings if m.field_name == "post_id")
        assert post_mapping.generator_type == "resolve_fk"
        assert post_mapping.fk_mapping_key == "post_id"

        user_mapping = next(m for m in mappings if m.field_name == "user_id")
        assert user_mapping.generator_type == "resolve_fk"
        assert user_mapping.fk_mapping_key == "user_id"

        # Optional FK
        parent_mapping = next(
            m for m in mappings if m.field_name == "parent_comment_id"
        )
        assert parent_mapping.generator_type == "resolve_fk"
        assert parent_mapping.fk_mapping_key == "parent_comment_id"
        assert parent_mapping.required is False

    def test_snake_case_conversion(self, analyzer):
        """Test PascalCase to snake_case conversion."""
        assert analyzer._to_snake_case("UserInput") == "user"
        assert analyzer._to_snake_case("PostCommentInput") == "post_comment"
        assert analyzer._to_snake_case("APIKeyInput") == "api_key"
        assert analyzer._to_snake_case("HTTPRequestInput") == "http_request"
        assert analyzer._to_snake_case("user_input") == "user"  # Already snake_case
        assert analyzer._to_snake_case("UserProfileInput") == "user_profile"

    def test_analyze_entity_relationships(self, analyzer):
        """Test generating FK mappings for entities."""
        fk_mappings = analyzer.analyze_entity_relationships("user", "tb_users")

        assert len(fk_mappings) == 1
        mapping = fk_mappings[0]

        assert mapping.input_type == "user_id"
        assert mapping.from_expression == "tb_users"
        assert mapping.select_field == "id"
        assert mapping.random_pk_field == "id"
        assert mapping.random_value_field == "email"  # Default for users
        assert mapping.random_select_where == "deleted_at IS NULL"

    def test_display_field_guessing(self, analyzer):
        """Test guessing display fields for different entities."""
        assert analyzer._guess_display_field("user") == "email"
        assert analyzer._guess_display_field("post") == "title"
        assert analyzer._guess_display_field("article") == "title"
        assert analyzer._guess_display_field("comment") == "content"
        assert analyzer._guess_display_field("category") == "name"  # Default
        assert analyzer._guess_display_field("product") == "name"  # Default

    def test_generate_sql_statements(self, analyzer):
        """Test SQL generation for metadata."""
        # Analyze UserInput
        analyzer.field_mappings = analyzer.analyze_input_type(UserInput)
        analyzer.fk_mappings = analyzer.analyze_entity_relationships("user", "tb_users")

        sql = analyzer.generate_sql_statements()

        # Should have field mappings
        assert "INSERT INTO testfoundry.testfoundry_tb_input_field_mapping" in sql
        assert (
            "('user', 'email', 'random', NULL, 'testfoundry_random_email', True, NULL, False, NULL)"
            in sql
        )
        assert "('user', 'name', 'random', NULL, NULL, True, NULL, False, NULL)" in sql

        # Should have FK mappings
        assert "INSERT INTO testfoundry.testfoundry_tb_fk_mapping" in sql
        assert (
            "('user_id', 'tb_users', 'id', 'id', 'email', 'deleted_at IS NULL')" in sql
        )

    def test_special_field_name_detection(self, analyzer):
        """Test detection of special field names for random functions."""

        @fraise_input
        class ContactInput:
            phone: str
            mobile_phone: Optional[str]
            work_phone: Optional[str]
            latitude: float
            longitude: float
            lat: Optional[float]
            lon: Optional[float]
            website_url: str
            profile_link: Optional[str]

        mappings = analyzer.analyze_input_type(ContactInput)

        # Phone fields
        phone_fields = [m for m in mappings if "phone" in m.field_name]
        for field in phone_fields:
            assert field.random_function == "testfoundry_random_phone"

        # Latitude fields
        lat_fields = [m for m in mappings if m.field_name in ("latitude", "lat")]
        for field in lat_fields:
            assert field.random_function == "testfoundry_random_latitude"

        # Longitude fields
        lon_fields = [m for m in mappings if m.field_name in ("longitude", "lon")]
        for field in lon_fields:
            assert field.random_function == "testfoundry_random_longitude"

        # URL fields
        url_fields = [
            m for m in mappings if "url" in m.field_name or "link" in m.field_name
        ]
        for field in url_fields:
            assert field.random_function == "testfoundry_random_url"

    def test_uuid_field_detection(self, analyzer):
        """Test UUID field type detection."""

        @fraise_input
        class EntityInput:
            id: UUIDField
            external_id: Optional[UUIDField]

        mappings = analyzer.analyze_input_type(EntityInput)

        id_mapping = next(m for m in mappings if m.field_name == "id")
        assert id_mapping.random_function == "gen_random_uuid"

        external_mapping = next(m for m in mappings if m.field_name == "external_id")
        assert external_mapping.random_function == "gen_random_uuid"
        assert external_mapping.required is False

    def test_empty_mappings_sql(self, analyzer):
        """Test SQL generation with empty mappings."""
        analyzer.field_mappings = []
        analyzer.fk_mappings = []

        sql = analyzer.generate_sql_statements()

        # Should return empty string
        assert sql == ""

    def test_non_fraise_input_error(self, analyzer):
        """Test error handling for non-FraiseQL input types."""

        class RegularClass:
            name: str

        with pytest.raises(ValueError, match="is not a FraiseQL input type"):
            analyzer.analyze_input_type(RegularClass)
