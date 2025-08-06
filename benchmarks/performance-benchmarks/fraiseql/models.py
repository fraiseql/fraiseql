"""FraiseQL models for benchmark testing using updated patterns."""

from __future__ import annotations

from datetime import datetime
from uuid import UUID

from fraiseql import fraise_type


@fraise_type
class Category:
    """Product category"""

    id: UUID
    name: str
    slug: str
    description: str | None
    parent_id: UUID | None


@fraise_type
class User:
    """User with aggregated statistics"""

    id: UUID
    email: str
    username: str
    full_name: str
    created_at: datetime
    is_active: bool
    # Aggregated fields from view
    order_count: int
    total_spent: float
    review_count: int
    average_rating: float | None


@fraise_type
class ProductReviewUser:
    """Simplified user for product reviews"""

    id: UUID
    username: str
    full_name: str


@fraise_type
class Review:
    """Product review"""

    id: UUID
    rating: int
    title: str | None
    comment: str | None
    created_at: datetime
    user: ProductReviewUser


@fraise_type
class ProductReview:
    """Product review returned by mutations"""

    id: UUID
    user_id: UUID
    product_id: UUID
    rating: int
    title: str | None
    comment: str | None
    created_at: datetime


@fraise_type
class Product:
    """Product with aggregated data"""

    id: UUID
    name: str
    slug: str
    description: str | None
    price: float
    stock_quantity: int
    tags: list[str]
    created_at: datetime
    updated_at: datetime
    # Aggregated fields from view
    review_count: int
    average_rating: float | None
    categories: list[Category]
    reviews: list[Review]


@fraise_type
class OrderItem:
    """Item in an order"""

    id: UUID
    product_id: UUID
    product_name: str
    quantity: int
    unit_price: float
    total_price: float


@fraise_type
class Order:
    """Order with items"""

    id: UUID
    user_id: UUID
    status: str
    total_amount: float
    created_at: datetime
    updated_at: datetime
    items: list[OrderItem]


# Additional types for specialized views
@fraise_type
class PopularProduct:
    """Popular product from materialized view"""

    id: UUID
    name: str
    slug: str
    price: float
    review_count: int
    average_rating: float
    total_revenue: float


@fraise_type
class ProductsByCategory:
    """Products grouped by category"""

    category: str
    products: list[Product]


@fraise_type
class UserStats:
    """User statistics"""

    user_id: UUID
    username: str
    order_count: int
    total_spent: float
    review_count: int
    average_rating: float
