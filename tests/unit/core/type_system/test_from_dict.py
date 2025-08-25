import pytest

"""Tests for automatic from_dict functionality in FraiseQL types."""

from uuid import UUID, uuid4

import fraiseql
from fraiseql import fraise_field

# Custom scalars are GraphQLScalarType objects, not Python types
# So we'll skip the custom scalar test for now


@pytest.mark.unit
def test_basic_from_dict() -> None:
    """Test basic from_dict functionality with simple types."""

    @fraiseql.type
    class User:
        id: UUID = fraise_field(description="User's ID")
        name: str = fraise_field(description="User's name")
        age: int = fraise_field(description="User's age")
        is_active: bool = fraise_field(description="Whether user is active")

    # Test with camelCase input
    user_data = {"id": str(uuid4()), "name": "John Doe", "age": 30, "isActive": True}

    user = User.from_dict(user_data)
    # UUID field now gets properly converted to UUID object
    assert isinstance(user.id, UUID)
    assert str(user.id) == user_data["id"]
    assert user.name == "John Doe"
    assert user.age == 30
    assert user.is_active is True


def test_from_dict_with_nested_types() -> None:
    """Test from_dict with nested types."""

    @fraiseql.type
    class Address:
        street: str = fraise_field(description="Street address")
        city: str = fraise_field(description="City")
        zip_code: str = fraise_field(description="ZIP code")

    @fraiseql.type
    class User:
        id: UUID = fraise_field(description="User's ID")
        name: str = fraise_field(description="User's name")
        primary_address: Address | None = fraise_field(description="Primary address")

    # Test with nested camelCase data
    user_data = {
        "id": str(uuid4()),
        "name": "Jane Smith",
        "primaryAddress": {"street": "123 Main St", "city": "New York", "zipCode": "10001"},
    }

    user = User.from_dict(user_data)
    assert user.name == "Jane Smith"
    # Now nested objects are properly instantiated
    assert isinstance(user.primary_address, Address)
    assert user.primary_address.street == "123 Main St"
    assert user.primary_address.city == "New York"
    assert user.primary_address.zip_code == "10001"


def test_from_dict_with_optional_fields() -> None:
    """Test from_dict with optional/missing fields."""

    @fraiseql.type
    class Profile:
        id: UUID = fraise_field(description="Profile ID")
        bio: str | None = fraise_field(description="User bio", default=None)
        website: str | None = fraise_field(description="Website URL", default=None)
        follower_count: int = fraise_field(description="Number of followers", default=0)

    # Test with missing optional fields
    profile_data = {"id": str(uuid4()), "followerCount": 100}

    profile = Profile.from_dict(profile_data)
    assert profile.bio is None
    assert profile.website is None
    assert profile.follower_count == 100


def test_from_dict_with_lists() -> None:
    """Test from_dict with list fields."""

    @fraiseql.type
    class Post:
        id: UUID = fraise_field(description="Post ID")
        title: str = fraise_field(description="Post title")
        tags: list[str] = fraise_field(description="Post tags", default_factory=list)
        view_counts: list[int] = fraise_field(description="Daily view counts")

    post_data = {
        "id": str(uuid4()),
        "title": "My First Post",
        "tags": ["python", "fraiseql", "graphql"],
        "viewCounts": [10, 20, 30, 40],
    }

    post = Post.from_dict(post_data)
    assert post.title == "My First Post"
    assert post.tags == ["python", "fraiseql", "graphql"]
    assert post.view_counts == [10, 20, 30, 40]


def test_from_dict_ignores_typename() -> None:
    """Test that from_dict ignores __typename field from GraphQL."""

    @fraiseql.type
    class Product:
        id: UUID = fraise_field(description="Product ID")
        name: str = fraise_field(description="Product name")
        price: float = fraise_field(description="Product price")

    product_data = {"__typename": "Product", "id": str(uuid4()), "name": "Widget", "price": 19.99}

    product = Product.from_dict(product_data)
    assert product.name == "Widget"
    assert product.price == 19.99
    # Ensure __typename is not set as an attribute
    assert not hasattr(product, "__typename")


def test_from_dict_camel_case_conversion() -> None:
    """Test various camelCase to snake_case conversions."""

    @fraiseql.type
    class ComplexModel:
        user_id: UUID = fraise_field(description="User ID")
        first_name: str = fraise_field(description="First name")
        last_updated_at: str = fraise_field(description="Last update timestamp")
        is_premium_member: bool = fraise_field(description="Premium status")
        total_order_count: int = fraise_field(description="Total orders")

    data = {
        "userId": str(uuid4()),
        "firstName": "Alice",
        "lastUpdatedAt": "2024-01-01T00:00:00Z",
        "isPremiumMember": True,
        "totalOrderCount": 42,
    }

    model = ComplexModel.from_dict(data)
    assert model.first_name == "Alice"
    assert model.last_updated_at == "2024-01-01T00:00:00Z"
    assert model.is_premium_member is True
    assert model.total_order_count == 42


def test_from_dict_with_uuid() -> None:
    """Test from_dict with UUID fields."""

    @fraiseql.type
    class Account:
        id: UUID = fraise_field(description="Account ID")
        name: str = fraise_field(description="Account name")
        balance: float = fraise_field(description="Account balance")

    account_id = uuid4()
    account_data = {"id": str(account_id), "name": "Savings Account", "balance": 1000.50}

    account = Account.from_dict(account_data)
    # UUID field now gets properly converted to UUID object
    assert isinstance(account.id, UUID)
    assert str(account.id) == account_data["id"]
    assert account.name == "Savings Account"
    assert account.balance == 1000.50


def test_from_dict_only_on_output_types(clear_registry) -> None:
    """Test that from_dict is only added to output types, not input types."""
    from fraiseql import fraise_input

    @fraise_input
    class CreateUserInput:
        name: str = fraise_field(description="User's name")
        email: str = fraise_field(description="User's email")

    @fraiseql.type
    class User:
        id: UUID = fraise_field(description="User ID")
        name: str = fraise_field(description="User's name")
        email: str = fraise_field(description="User's email")

    # Output types should have from_dict
    assert hasattr(User, "from_dict")

    # Input types should NOT have from_dict
    assert not hasattr(CreateUserInput, "from_dict")

    # Test that User.from_dict works
    user_data = {"id": str(uuid4()), "name": "Test User", "email": "test@example.com"}
    user = User.from_dict(user_data)
    assert user.name == "Test User"
