"""Tests for Trinity Identifiers pattern."""

from uuid import UUID, uuid4

from fraiseql.patterns import (
    TrinityMixin,
    get_identifier_from_slug,
    get_pk_column_name,
    trinity_field,
)

# ============================================================================
# Test Fixtures
# ============================================================================


class User(TrinityMixin):
    """Test user type with Trinity identifiers."""

    def __init__(self, id: UUID, identifier: str | None, username: str, email: str) -> None:
        self.id = id
        self.identifier = identifier
        self.username = username
        self.email = email


class Post(TrinityMixin):
    """Test post type with Trinity identifiers."""

    __trinity_entity_name__ = "post"  # Explicit override

    def __init__(self, id: UUID, identifier: str | None, title: str, content: str) -> None:
        self.id = id
        self.identifier = identifier
        self.title = title
        self.content = content


# ============================================================================
# Trinity Mixin Tests
# ============================================================================


def test_trinity_mixin_auto_entity_name() -> None:
    """Test that entity name is auto-detected from class name."""
    user = User(id=uuid4(), identifier="johndoe", username="johndoe", email="john@example.com")

    assert user.__trinity_entity_name__ == "user"
    assert user._pk_name == "pk_user"


def test_trinity_mixin_explicit_entity_name() -> None:
    """Test that explicit entity name override works."""
    post = Post(id=uuid4(), identifier="my-post-slug", title="Test Post", content="Content")

    assert post.__trinity_entity_name__ == "post"
    assert post._pk_name == "pk_post"


def test_trinity_get_internal_pk() -> None:
    """Test getting internal pk value."""
    user = User(id=uuid4(), identifier="johndoe", username="johndoe", email="john@example.com")

    # Initially None
    assert user.get_internal_pk() is None

    # Set and get
    user.set_internal_pk(123)
    assert user.get_internal_pk() == 123


def test_trinity_set_internal_pk() -> None:
    """Test setting internal pk value."""
    user = User(id=uuid4(), identifier="johndoe", username="johndoe", email="john@example.com")

    user.set_internal_pk(456)
    assert user.pk_user == 456


# ============================================================================
# Helper Function Tests
# ============================================================================


def test_get_pk_column_name() -> None:
    """Test pk_* column name generation."""
    assert get_pk_column_name(User) == "pk_user"
    assert get_pk_column_name(Post) == "pk_post"


def test_get_identifier_from_slug_with_at_symbol() -> None:
    """Test slug parsing with @ prefix."""
    assert get_identifier_from_slug("@johndoe") == "johndoe"
    assert get_identifier_from_slug("@jane_doe") == "jane_doe"


def test_get_identifier_from_slug_with_path() -> None:
    """Test slug parsing from URL path."""
    assert get_identifier_from_slug("/users/@johndoe") == "johndoe"
    assert get_identifier_from_slug("/posts/my-post-slug") == "my-post-slug"


def test_get_identifier_from_slug_plain() -> None:
    """Test slug parsing without special characters."""
    assert get_identifier_from_slug("johndoe") == "johndoe"
    assert get_identifier_from_slug("my-post-slug") == "my-post-slug"


def test_get_identifier_from_slug_case_normalization() -> None:
    """Test that slugs are lowercased."""
    assert get_identifier_from_slug("@JohnDoe") == "johndoe"
    assert get_identifier_from_slug("MyPostSlug") == "mypostslug"


def test_get_identifier_from_slug_whitespace() -> None:
    """Test that whitespace is stripped."""
    assert get_identifier_from_slug("  johndoe  ") == "johndoe"
    assert get_identifier_from_slug(" @johndoe ") == "johndoe"


# ============================================================================
# Integration Tests
# ============================================================================


def test_trinity_type_has_required_fields() -> None:
    """Test that Trinity types have required fields."""
    user_id = uuid4()
    user = User(id=user_id, identifier="johndoe", username="johndoe", email="john@example.com")

    # Public fields
    assert user.id == user_id
    assert user.identifier == "johndoe"
    assert user.username == "johndoe"
    assert user.email == "john@example.com"


def test_trinity_identifier_optional() -> None:
    """Test that identifier can be None."""
    user = User(
        id=uuid4(),
        identifier=None,  # Some users might not have a slug
        username="johndoe",
        email="john@example.com",
    )

    assert user.identifier is None


def test_trinity_multiple_instances() -> None:
    """Test that Trinity works with multiple instances."""
    user1 = User(id=uuid4(), identifier="user1", username="user1", email="user1@example.com")
    user1.set_internal_pk(1)

    user2 = User(id=uuid4(), identifier="user2", username="user2", email="user2@example.com")
    user2.set_internal_pk(2)

    # Each instance has independent pk values
    assert user1.get_internal_pk() == 1
    assert user2.get_internal_pk() == 2


# ============================================================================
# Edge Cases
# ============================================================================


def test_trinity_with_complex_path() -> None:
    """Test slug parsing with complex nested paths."""
    assert get_identifier_from_slug("/api/v1/users/@johndoe") == "johndoe"
    assert get_identifier_from_slug("/blog/posts/2024/@my-post") == "my-post"


def test_trinity_entity_name_case_preservation() -> None:
    """Test that entity names respect class case."""

    class UserAccount(TrinityMixin):
        def __init__(self, id: UUID, name: str) -> None:
            self.id = id
            self.name = name

    account = UserAccount(id=uuid4(), name="Test")
    # Should be lowercased
    assert account.__trinity_entity_name__ == "useraccount"
    assert account._pk_name == "pk_useraccount"


def test_trinity_field_returns_metadata() -> None:
    """Test that trinity_field returns proper metadata."""
    metadata = trinity_field(description="User UUID", required=True)
    assert metadata["description"] == "User UUID"
    assert metadata["required"] is True
