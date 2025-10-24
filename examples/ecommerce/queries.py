"""E-commerce queries for FraiseQL example."""

from uuid import UUID

import fraiseql
from fraiseql import Info

from .models import (
    Address,
    Cart,
    CartItem,
    Order,
    OrderItem,
    OrderStatus,
    Product,
    ProductFilterInput,
    Review,
    User,
)


@fraiseql.type
class ProductConnection:
    """Paginated product results."""

    items: list[Product]
    total_count: int
    has_next_page: bool
    has_previous_page: bool


@fraiseql.type
class OrderConnection:
    """Paginated order results."""

    items: list[Order]
    total_count: int
    has_next_page: bool
    has_previous_page: bool


@fraiseql.type
class ReviewConnection:
    """Paginated review results."""

    items: list[Review]
    total_count: int
    average_rating: float


@fraiseql.type
class CartWithItems:
    """Cart with its items."""

    cart: Cart
    items: list[CartItem]
    recommended_products: list[Product]


@fraiseql.type
class OrderWithDetails:
    """Order with full details."""

    order: Order
    items: list[OrderItem]
    shipping_address: Address
    billing_address: Address
    user: User


@fraiseql.type
class ProductWithReviews:
    """Product with reviews."""

    product: Product
    reviews: ReviewConnection
    average_rating: float
    review_count: int
    related_products: list[Product]


@fraiseql.type
class DashboardStats:
    """User dashboard statistics."""

    total_orders: int
    total_spent: fraiseql.Decimal
    average_order_value: fraiseql.Decimal
    wishlist_count: int
    review_count: int
    points_balance: int


@fraiseql.type
class Query:
    """Root query type for e-commerce."""

    # User queries
    @fraiseql.field
    async def me(self, info: Info) -> User | None:
        """Get current authenticated user."""
        # Requires authentication
        return None  # Placeholder

    @fraiseql.field
    async def user(self, info: Info, id: UUID) -> User | None:
        """Get user by ID (admin only)."""
        return None  # Placeholder

    # Product queries
    @fraiseql.field
    async def product(self, info: Info, id: UUID) -> Product | None:
        """Get product by ID."""
        return None  # Placeholder

    @fraiseql.field
    async def product_by_sku(self, info: Info, sku: str) -> Product | None:
        """Get product by SKU."""
        return None  # Placeholder

    @fraiseql.field
    async def products(
        self,
        info: Info,
        filters: ProductFilterInput | None = None,
        sort_by: str = "created_at",
        sort_order: str = "desc",
        limit: int = 20,
        offset: int = 0,
    ) -> ProductConnection:
        """Search and filter products."""
        return ProductConnection(
            items=[],
            total_count=0,
            has_next_page=False,
            has_previous_page=False,
        )

    @fraiseql.field
    async def featured_products(self, info: Info, limit: int = 8) -> list[Product]:
        """Get featured products."""
        return []

    @fraiseql.field
    async def best_sellers(self, info: Info, limit: int = 10) -> list[Product]:
        """Get best selling products."""
        return []

    @fraiseql.field
    async def new_arrivals(self, info: Info, limit: int = 10) -> list[Product]:
        """Get newest products."""
        return []

    @fraiseql.field
    async def product_with_reviews(
        self,
        info: Info,
        id: UUID,
        review_limit: int = 10,
        review_offset: int = 0,
    ) -> ProductWithReviews | None:
        """Get product with reviews and related products."""
        return None  # Placeholder

    # Cart queries
    @fraiseql.field
    async def my_cart(self, info: Info) -> CartWithItems | None:
        """Get current user's cart with items."""
        return None  # Placeholder

    @fraiseql.field
    async def cart(self, info: Info, id: UUID) -> Cart | None:
        """Get cart by ID."""
        return None  # Placeholder

    # Order queries
    @fraiseql.field
    async def my_orders(
        self,
        info: Info,
        status: OrderStatus | None = None,
        limit: int = 20,
        offset: int = 0,
    ) -> OrderConnection:
        """Get current user's orders."""
        return OrderConnection(
            items=[],
            total_count=0,
            has_next_page=False,
            has_previous_page=False,
        )

    @fraiseql.field
    async def order(self, info: Info, id: UUID) -> OrderWithDetails | None:
        """Get order by ID with full details."""
        return None  # Placeholder

    @fraiseql.field
    async def order_by_number(self, info: Info, order_number: str) -> OrderWithDetails | None:
        """Get order by order number."""
        return None  # Placeholder

    # Address queries
    @fraiseql.field
    async def my_addresses(self, info: Info) -> list[Address]:
        """Get current user's addresses."""
        return []

    @fraiseql.field
    async def address(self, info: Info, id: UUID) -> Address | None:
        """Get address by ID."""
        return None  # Placeholder

    # Review queries
    @fraiseql.field
    async def product_reviews(
        self,
        info: Info,
        product_id: UUID,
        limit: int = 20,
        offset: int = 0,
        sort_by: str = "created_at",
    ) -> ReviewConnection:
        """Get reviews for a product."""
        return ReviewConnection(
            items=[],
            total_count=0,
            average_rating=0.0,
        )

    @fraiseql.field
    async def my_reviews(self, info: Info) -> list[Review]:
        """Get current user's reviews."""
        return []

    # Dashboard/Stats queries
    @fraiseql.field
    async def my_dashboard(self, info: Info) -> DashboardStats:
        """Get user dashboard statistics."""
        return DashboardStats(
            total_orders=0,
            total_spent=fraiseql.Decimal("0"),
            average_order_value=fraiseql.Decimal("0"),
            wishlist_count=0,
            review_count=0,
            points_balance=0,
        )

    @fraiseql.field
    async def my_wishlist(self, info: Info) -> list[Product]:
        """Get current user's wishlist."""
        return []

    # Search
    @fraiseql.field
    async def search_products(
        self,
        info: Info,
        query: str,
        limit: int = 20,
    ) -> list[Product]:
        """Full-text search products."""
        return []

    @fraiseql.field
    async def search_suggestions(
        self,
        info: Info,
        query: str,
        limit: int = 5,
    ) -> list[str]:
        """Get search suggestions."""
        return []


# Add some example query fragments for documentation

PRODUCT_FRAGMENT = """
fragment ProductDetails on Product {
    id
    sku
    name
    description
    category
    price
    compareAtPrice
    inventoryCount
    isActive
    images
    tags
}
"""

ORDER_FRAGMENT = """
fragment OrderDetails on Order {
    id
    orderNumber
    status
    paymentStatus
    subtotal
    taxAmount
    shippingAmount
    discountAmount
    total
    placedAt
    shippedAt
    deliveredAt
}
"""

EXAMPLE_QUERIES = """
# Get product with reviews
query GetProduct($id: UUID!) {
    productWithReviews(id: $id) {
        product {
            ...ProductDetails
        }
        averageRating
        reviewCount
        reviews(limit: 5) {
            items {
                id
                rating
                title
                comment
                user {
                    name
                }
                createdAt
            }
        }
        relatedProducts {
            ...ProductDetails
        }
    }
}

# Search products
query SearchProducts($filters: ProductFilterInput!, $limit: Int, $offset: Int) {
    products(filters: $filters, limit: $limit, offset: $offset) {
        items {
            ...ProductDetails
        }
        totalCount
        hasNextPage
    }
}

# Get user's orders
query MyOrders($status: OrderStatus) {
    myOrders(status: $status) {
        items {
            ...OrderDetails
            items {
                product {
                    name
                    images
                }
                quantity
                price
                total
            }
        }
        totalCount
    }
}
"""
