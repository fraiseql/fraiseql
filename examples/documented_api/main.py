"""Auto-Documentation Example for FraiseQL.

This example demonstrates how FraiseQL automatically generates comprehensive
GraphQL documentation from Python docstrings and type hints.

Features demonstrated:
- Type-level documentation via class docstrings
- Field-level documentation via attribute docstrings
- Automatic filter operator documentation
- Enum documentation
- Complex type documentation

The generated documentation appears in:
- GraphQL Playground
- Apollo Studio
- GraphiQL
- Any GraphQL introspection tool
"""

from dataclasses import dataclass
from datetime import datetime
from decimal import Decimal
from enum import Enum

from fraiseql import FraiseQL

# Initialize FraiseQL
app = FraiseQL(database_url="postgresql://localhost/ecommerce")


class ProductCategory(str, Enum):
    """Product category classification.

    Categories help organize products for browsing and filtering.
    Each product must belong to exactly one category.
    """

    ELECTRONICS = "electronics"
    """Electronic devices and accessories"""

    CLOTHING = "clothing"
    """Apparel and fashion items"""

    BOOKS = "books"
    """Physical and digital books"""

    HOME = "home"
    """Home and garden items"""

    SPORTS = "sports"
    """Sports equipment and outdoor gear"""


@app.type
@dataclass
class Product:
    """A product in the e-commerce catalog.

    Products can be physical goods, digital downloads, or services.
    Each product has pricing, inventory tracking, and categorization.
    All products support multiple images and detailed specifications.
    """

    id: int
    """Unique product identifier (auto-generated)"""

    name: str
    """Product display name.

    Maximum 200 characters. Used in search results and product listings.
    Should be descriptive and include key features.
    """

    description: str
    """Full product description in markdown format.

    Supports markdown formatting for rich text display.
    Include key features, specifications, and usage instructions.
    """

    price: Decimal
    """Price in USD.

    Supports up to 2 decimal places (e.g., 19.99).
    Does not include taxes or shipping costs.
    """

    category: ProductCategory
    """Product category for organization and filtering"""

    in_stock: bool
    """Whether the product is currently available for purchase.

    True: Available for immediate purchase
    False: Out of stock, can be wishlisted
    """

    stock_quantity: int
    """Current inventory count.

    Updated in real-time as orders are placed.
    When this reaches 0, in_stock automatically becomes False.
    """

    average_rating: float | None
    """Average customer rating (1.0 to 5.0 stars).

    Calculated from all customer reviews.
    Null if no reviews exist yet.
    """

    review_count: int
    """Total number of customer reviews"""

    created_at: datetime
    """When this product was added to the catalog (UTC)"""

    updated_at: datetime
    """Last modification timestamp (UTC)"""


@app.type
@dataclass
class Review:
    """Customer product review.

    Reviews help customers make informed purchasing decisions.
    All reviews are verified purchases and moderated for content.
    """

    id: int
    """Unique review identifier"""

    product_id: int
    """ID of the product being reviewed"""

    customer_name: str
    """Name of the reviewer (may be anonymized)"""

    rating: int
    """Star rating (1-5).

    1 = Very Poor
    2 = Poor
    3 = Average
    4 = Good
    5 = Excellent
    """

    title: str
    """Review headline (max 100 characters)"""

    content: str
    """Detailed review text.

    Should describe the customer's experience with the product.
    Helpful reviews include specific details about quality, features, and use cases.
    """

    verified_purchase: bool
    """Whether this review is from a verified purchase.

    Verified purchase reviews are weighted more heavily in ratings.
    """

    helpful_count: int
    """Number of users who marked this review as helpful"""

    created_at: datetime
    """When the review was submitted (UTC)"""


@app.type
@dataclass
class Customer:
    """Registered customer account.

    Customers can browse products, add items to cart, place orders,
    and write product reviews. Each customer has a unique email address.
    """

    id: int
    """Unique customer identifier"""

    email: str
    """Customer email address (used for login).

    Must be unique across all customers.
    Used for order confirmations and notifications.
    """

    name: str
    """Customer's full name"""

    membership_tier: str
    """Membership level (basic, premium, vip).

    - basic: Standard features
    - premium: Free shipping, early access to sales
    - vip: All premium features + dedicated support
    """

    total_orders: int
    """Lifetime number of completed orders"""

    total_spent: Decimal
    """Lifetime spending in USD"""

    account_created: datetime
    """Account creation date (UTC)"""


# =============================================================================
# GraphQL Queries
# =============================================================================

@app.query
async def products(
    info,
    category: ProductCategory | None = None,
    in_stock_only: bool = False,
    min_price: Decimal | None = None,
    max_price: Decimal | None = None,
    min_rating: float | None = None,
    limit: int = 20,
    offset: int = 0
) -> list[Product]:
    """Query products with flexible filtering.

    Supports filtering by category, availability, price range, and ratings.
    Results are paginated and sorted by relevance.

    Args:
        category: Filter by product category (optional)
        in_stock_only: If True, only return available products
        min_price: Minimum price filter (inclusive)
        max_price: Maximum price filter (inclusive)
        min_rating: Minimum average rating (1.0 to 5.0)
        limit: Maximum number of results (default: 20, max: 100)
        offset: Number of results to skip for pagination

    Returns:
        List of products matching the filters

    Example:
        ```graphql
        {
          products(
            category: ELECTRONICS,
            in_stock_only: true,
            min_price: 10.00,
            max_price: 100.00,
            min_rating: 4.0,
            limit: 10
          ) {
            id
            name
            price
            average_rating
          }
        }
        ```
    """
    db = info.context["db"]
    filters = {}

    if category:
        filters["category"] = category.value
    if in_stock_only:
        filters["in_stock"] = True
    if min_price is not None:
        filters["price__gte"] = min_price
    if max_price is not None:
        filters["price__lte"] = max_price
    if min_rating is not None:
        filters["average_rating__gte"] = min_rating

    return await db.find("v_products", limit=limit, offset=offset, **filters)


@app.query
async def product(info, id: int) -> Product | None:
    """Get a single product by ID.

    Returns detailed product information including all fields.
    Returns null if the product doesn't exist.

    Args:
        id: Product ID

    Returns:
        Product details or null if not found
    """
    db = info.context["db"]
    return await db.find_one("v_products", id=id)


@app.query
async def reviews(
    info,
    product_id: int,
    verified_only: bool = False,
    min_rating: int | None = None,
    limit: int = 10,
    offset: int = 0
) -> list[Review]:
    """Get reviews for a specific product.

    Args:
        product_id: Product to get reviews for
        verified_only: If True, only return verified purchase reviews
        min_rating: Minimum star rating (1-5)
        limit: Maximum number of reviews
        offset: Pagination offset

    Returns:
        List of reviews sorted by helpfulness and recency
    """
    db = info.context["db"]
    filters = {"product_id": product_id}

    if verified_only:
        filters["verified_purchase"] = True
    if min_rating is not None:
        filters["rating__gte"] = min_rating

    return await db.find("v_reviews", limit=limit, offset=offset, **filters)


# =============================================================================
# Database Schema (for reference)
# =============================================================================
"""
-- Products table
CREATE TABLE tb_products (
    id SERIAL PRIMARY KEY,
    data JSONB NOT NULL,
    category VARCHAR(50) NOT NULL,
    in_stock BOOLEAN NOT NULL DEFAULT true,
    price DECIMAL(10,2) NOT NULL,
    average_rating DECIMAL(3,2),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Products view (optimized for GraphQL queries)
CREATE VIEW v_products AS
SELECT
    id,
    data->>'name' as name,
    data->>'description' as description,
    price,
    category,
    in_stock,
    (data->>'stock_quantity')::int as stock_quantity,
    average_rating,
    (data->>'review_count')::int as review_count,
    created_at,
    updated_at
FROM tb_products;

-- Reviews table
CREATE TABLE tb_reviews (
    id SERIAL PRIMARY KEY,
    product_id INT NOT NULL REFERENCES tb_products(id),
    data JSONB NOT NULL,
    rating INT NOT NULL CHECK (rating >= 1 AND rating <= 5),
    verified_purchase BOOLEAN NOT NULL DEFAULT false,
    helpful_count INT NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Reviews view
CREATE VIEW v_reviews AS
SELECT
    id,
    product_id,
    data->>'customer_name' as customer_name,
    rating,
    data->>'title' as title,
    data->>'content' as content,
    verified_purchase,
    helpful_count,
    created_at
FROM tb_reviews;
"""

# =============================================================================
# Running the Example
# =============================================================================
if __name__ == "__main__":
    import uvicorn

    from fraiseql.fastapi import create_app

    # Create FastAPI app with FraiseQL
    fastapi_app = create_app(app, database_url="postgresql://localhost/ecommerce")

    print("Starting FraiseQL Auto-Documentation Example...")
    print("Open http://localhost:8000/graphql to see:")
    print("  - Full type documentation")
    print("  - Field descriptions")
    print("  - Argument documentation")
    print("  - Enum documentation")
    print("  - Example queries in docstrings")
    print()
    print("Try introspection queries to see the documentation:")
    print("  - Click 'Docs' in GraphQL Playground")
    print("  - Or use __type queries for programmatic access")

    uvicorn.run(fastapi_app, host="0.0.0.0", port=8000)
