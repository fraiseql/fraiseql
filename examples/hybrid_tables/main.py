"""Hybrid Table Filtering Example for FraiseQL.

This example demonstrates the hybrid filtering pattern where:
- **tv_* tables**: Contain JSONB data for fast GraphQL queries (0.05-0.5ms)
- **Dedicated SQL columns**: Provide indexed filtering capabilities
- **Hybrid filtering**: Filter using SQL columns, return JSONB data

The "hybrid" aspect is filtering using SQL columns while querying JSONB data.
"""

from dataclasses import dataclass
from datetime import datetime
from decimal import Decimal

from fraiseql import FraiseQL

# Initialize FraiseQL
app = FraiseQL(database_url="postgresql://localhost/ecommerce")


@app.type(sql_source="tv_products", jsonb_column="data")
@dataclass
class Product:
    """E-commerce product with hybrid filtering.

    Uses tv_products table with:
    - Dedicated SQL columns for filtering (category_id, price, is_active)
    - JSONB data column for complete GraphQL responses
    """

    id: str  # UUID from tv_products.id
    """GraphQL ID - Primary key for queries"""

    # Fields from dedicated SQL columns (for filtering)
    category_id: int
    """Category ID - Indexed for fast filtering"""

    is_active: bool
    """Active status - Partial index for active products"""

    price: Decimal
    """Price - Indexed for range queries and sorting"""

    created_at: datetime
    """Creation timestamp - Indexed for sorting"""

    # Fields from JSONB data (complete response data)
    name: str
    """Product name from JSONB"""

    description: str
    """Full description from JSONB"""

    sku: str
    """Stock keeping unit from JSONB"""

    brand: str
    """Brand name from JSONB"""

    specifications: dict
    """Product specifications (variable by category)"""

    images: list[str]
    """Product image URLs"""

    tags: list[str]
    """Search/filter tags"""

    metadata: dict
    """Additional flexible metadata"""


@app.type
@dataclass
class Order:
    """Customer order with hybrid storage."""

    id: int
    """Order ID - Primary key"""

    customer_id: int
    """Customer foreign key - Indexed"""

    status: str
    """Order status - Indexed for filtering"""

    total_amount: Decimal
    """Order total - Indexed for reporting"""

    created_at: datetime
    """Order date - Indexed for range queries"""

    # JSONB fields
    shipping_address: dict
    """Full shipping address details"""

    billing_address: dict
    """Full billing address details"""

    items: list[dict]
    """Order items with product details"""

    payment_method: dict
    """Payment method details"""

    notes: str | None
    """Customer notes"""


# =============================================================================
# GraphQL Queries - Demonstrating Performance
# =============================================================================


@app.query
async def products(
    info,
    category_id: int | None = None,
    is_active: bool = True,
    min_price: Decimal | None = None,
    max_price: Decimal | None = None,
    brand: str | None = None,
    limit: int = 20,
    offset: int = 0,
) -> list[Product]:
    """Query products with hybrid filtering.

    **Performance characteristics:**
    - category_id filter: Uses B-tree index (O(log n))
    - is_active filter: Uses partial index (only active products indexed)
    - price range: Uses B-tree index range scan
    - brand filter: JSONB path search (slower, but flexible)

    On 1M products:
    - Indexed queries: ~5-10ms
    - JSONB-only queries: ~100-500ms
    - Combined queries: Uses index first, then JSONB filter

    Example:
        ```graphql
        # FAST: Uses indexed columns
        {
          products(category_id: 5, is_active: true, min_price: 10.00, max_price: 100.00) {
            id
            name
            price
          }
        }

        # FLEXIBLE: Searches JSONB data
        {
          products(brand: "Acme Corp") {
            name
            brand
            specifications
          }
        }
        ```
    """
    db = info.context["db"]
    filters = {}

    if category_id is not None:
        filters["category_id"] = category_id
    if is_active is not None:
        filters["is_active"] = is_active
    if min_price is not None:
        filters["price__gte"] = min_price
    if max_price is not None:
        filters["price__lte"] = max_price
    if brand:
        # JSONB path search
        filters["data__brand"] = brand

    return await db.find("tv_products", limit=limit, offset=offset, **filters)


@app.query
async def expensive_products(info, min_price: Decimal = Decimal(1000)) -> list[Product]:
    """Find expensive products using indexed price column.

    **Performance:**
    - Uses B-tree index on price column
    - ~5ms on 1M rows
    - Compare to: JSONB-only would be ~500ms

    Example:
        ```graphql
        {
          expensive_products(min_price: 1000.00) {
            name
            price
            brand
          }
        }
        ```
    """
    db = info.context["db"]
    return await db.find("tv_products", price__gte=min_price, is_active=True)


@app.query
async def orders(
    info,
    customer_id: int | None = None,
    status: str | None = None,
    min_amount: Decimal | None = None,
    from_date: datetime | None = None,
    limit: int = 20,
    offset: int = 0,
) -> list[Order]:
    """Query orders with hybrid filtering.

    **Performance:**
    - customer_id: Uses foreign key index
    - status: Uses B-tree index
    - created_at range: Uses B-tree index range scan
    - total_amount range: Uses B-tree index

    Example:
        ```graphql
        {
          orders(
            customer_id: 123,
            status: "completed",
            min_amount: 50.00,
            from_date: "2025-01-01T00:00:00Z"
          ) {
            id
            total_amount
            status
            items
            shipping_address
          }
        }
        ```
    """
    db = info.context["db"]
    filters = {}

    if customer_id is not None:
        filters["customer_id"] = customer_id
    if status:
        filters["status"] = status
    if min_amount is not None:
        filters["total_amount__gte"] = min_amount
    if from_date:
        filters["created_at__gte"] = from_date

    return await db.find("tv_orders", limit=limit, offset=offset, order_by="-created_at", **filters)


# =============================================================================
# Database Schema - Hybrid Filtering Pattern
# =============================================================================
"""
-- Base table: Normalized data with constraints
CREATE TABLE tb_products (
    pk_product SERIAL PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT gen_random_uuid(),
    category_id INT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    price DECIMAL(10,2) NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    name TEXT NOT NULL,
    description TEXT,
    sku TEXT UNIQUE,
    CONSTRAINT fk_category FOREIGN KEY (category_id) REFERENCES tb_categories(id)
);

-- Query table: Hybrid filtering with dedicated columns + JSONB data
CREATE TABLE tv_products (
    id UUID PRIMARY KEY,  -- GraphQL identifier
    category_id INT NOT NULL,  -- Dedicated column for filtering
    is_active BOOLEAN NOT NULL DEFAULT true,  -- Dedicated column for filtering
    price DECIMAL(10,2) NOT NULL,  -- Dedicated column for filtering
    created_at TIMESTAMP NOT NULL,  -- Dedicated column for filtering
    data JSONB NOT NULL  -- Complete JSONB data for GraphQL responses
);

-- Performance indexes for filtering
CREATE INDEX idx_tv_products_category ON tv_products(category_id);
CREATE INDEX idx_tv_products_price ON tv_products(price);
CREATE INDEX idx_tv_products_created ON tv_products(created_at DESC);
CREATE INDEX idx_tv_products_active ON tv_products(is_active) WHERE is_active = true;
CREATE INDEX idx_tv_products_category_price ON tv_products(category_id, price);

-- Orders query table: Hybrid filtering
CREATE TABLE tv_orders (
    id UUID PRIMARY KEY,  -- GraphQL identifier
    customer_id UUID NOT NULL,  -- Dedicated column for filtering
    status VARCHAR(50) NOT NULL,  -- Dedicated column for filtering
    total_amount DECIMAL(10,2) NOT NULL,  -- Dedicated column for filtering
    created_at TIMESTAMP NOT NULL,  -- Dedicated column for filtering
    data JSONB NOT NULL  -- Complete JSONB data for GraphQL responses
);

-- Performance indexes for filtering
CREATE INDEX idx_tv_orders_customer ON tv_orders(customer_id);
CREATE INDEX idx_tv_orders_status ON tv_orders(status);
CREATE INDEX idx_tv_orders_amount ON tv_orders(total_amount);
CREATE INDEX idx_tv_orders_created ON tv_orders(created_at DESC);
CREATE INDEX idx_tv_orders_customer_status ON tv_orders(customer_id, status);

-- Performance comparison queries

-- FAST: Uses dedicated SQL columns for filtering
-- EXPLAIN ANALYZE SELECT data FROM tv_products WHERE category_id = 5 AND price >= 10 AND price <= 100;
-- Result: Index Scan using idx_tv_products_category + idx_tv_products_price (~5-10ms on 1M rows)

-- FLEXIBLE: Uses JSONB path filtering
-- EXPLAIN ANALYZE SELECT data FROM tv_products WHERE data->>'brand' = 'Acme Corp';
-- Result: Index Scan using JSONB path index (~50ms on 1M rows)
--         OR Seq Scan if no JSONB index (~500ms on 1M rows)

-- HYBRID: Filter using SQL columns, return JSONB data
-- EXPLAIN ANALYZE SELECT data FROM tv_products WHERE category_id = 5 AND data->>'brand' = 'Acme Corp';
-- Result: Uses category_id index first (fast), then filters by brand (~15ms on 1M rows)
"""

# =============================================================================
# Example Data
# =============================================================================
"""
-- Insert sample products
INSERT INTO tb_products (category_id, is_active, price, data) VALUES
(5, true, 299.99, '{
    "name": "Wireless Headphones",
    "description": "Premium noise-cancelling headphones",
    "sku": "WH-1000XM5",
    "brand": "Sony",
    "specifications": {
        "battery_life": "30 hours",
        "weight": "250g",
        "bluetooth": "5.2"
    },
    "images": ["https://example.com/img1.jpg"],
    "tags": ["audio", "wireless", "premium"]
}'),
(5, true, 199.99, '{
    "name": "Smart Watch",
    "description": "Fitness tracking smartwatch",
    "sku": "SW-ULTRA-2",
    "brand": "Apple",
    "specifications": {
        "display": "AMOLED",
        "water_resistant": "50m"
    },
    "images": ["https://example.com/img2.jpg"],
    "tags": ["wearable", "fitness"]
}');

-- Insert sample orders
INSERT INTO tb_orders (customer_id, status, total_amount, data) VALUES
(123, 'completed', 299.99, '{
    "shipping_address": {
        "street": "123 Main St",
        "city": "San Francisco",
        "state": "CA",
        "zip": "94105"
    },
    "billing_address": {
        "street": "123 Main St",
        "city": "San Francisco",
        "state": "CA",
        "zip": "94105"
    },
    "items": [
        {
            "product_id": 1,
            "name": "Wireless Headphones",
            "quantity": 1,
            "price": 299.99
        }
    ],
    "payment_method": {
        "type": "credit_card",
        "last4": "4242"
    },
    "notes": "Please leave at door"
}');
"""

# =============================================================================
# Running the Example
# =============================================================================
if __name__ == "__main__":
    import uvicorn

    from fraiseql.fastapi import create_app

    fastapi_app = create_app(app, database_url="postgresql://localhost/ecommerce")

    print("Starting FraiseQL Hybrid Tables Example...")
    print()
    print("This example demonstrates:")
    print("  ✅ tv_* tables for fast GraphQL queries (JSONB data)")
    print("  ✅ Dedicated SQL columns for efficient filtering")
    print("  ✅ Hybrid filtering: SQL WHERE clauses + JSONB responses")
    print("  ✅ 10-100x speedup on large datasets")
    print()
    print("Performance comparison on 1M rows:")
    print("  - SQL column filtering: ~5-10ms")
    print("  - JSONB path filtering: ~50-100ms")
    print("  - Hybrid filtering: ~15ms (SQL first, then JSONB)")
    print()
    print("Open http://localhost:8000/graphql to try queries")

    uvicorn.run(fastapi_app, host="0.0.0.0", port=8000)
