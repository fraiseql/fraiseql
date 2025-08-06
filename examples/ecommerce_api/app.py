"""E-commerce API Application

Demonstrates FraiseQL's capabilities with a complete e-commerce system
"""

import os
from contextlib import asynccontextmanager

import asyncpg
from fastapi import FastAPI

from fraiseql import FraiseQL

from .models import EcommerceQuery
from .mutations import (
    add_customer_address,
    add_to_cart,
    add_to_wishlist,
    apply_coupon_to_cart,
    cancel_order,
    clear_cart,
    create_order,
    mark_review_helpful,
    process_order_payment,
    register_customer,
    submit_review,
    update_cart_item,
    update_customer_profile,
    update_order_status,
)

# Database configuration
DATABASE_URL = os.getenv(
    "DATABASE_URL",
    "postgresql://user:password@localhost:5432/ecommerce",
)


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application lifespan manager"""
    # Create connection pool
    app.state.db_pool = await asyncpg.create_pool(
        DATABASE_URL,
        min_size=10,
        max_size=20,
        command_timeout=60,
    )

    yield

    # Close connection pool
    await app.state.db_pool.close()


# Create FastAPI app
app = FastAPI(
    title="E-commerce API",
    description="""
    A comprehensive e-commerce API built with FraiseQL.

    Features:
    - Product catalog with categories and variants
    - Shopping cart management
    - Order processing with inventory tracking
    - Customer accounts and addresses
    - Product reviews and ratings
    - Wishlist functionality
    - Coupon system
    - Real-time inventory management

    This example demonstrates:
    - Complex relational data modeling
    - CQRS architecture with views and functions
    - Performance optimization with materialized views
    - Business logic in PostgreSQL functions
    - Type-safe GraphQL API
    """,
    version="1.0.0",
    lifespan=lifespan,
)


# Create FraiseQL instance
fraiseql = FraiseQL(
    db_url=DATABASE_URL,
    query_type=EcommerceQuery,
    mutations=[
        # Cart mutations
        add_to_cart,
        update_cart_item,
        clear_cart,
        apply_coupon_to_cart,
        # Order mutations
        create_order,
        update_order_status,
        process_order_payment,
        cancel_order,
        # Customer mutations
        register_customer,
        update_customer_profile,
        add_customer_address,
        # Wishlist mutations
        add_to_wishlist,
        # Review mutations
        submit_review,
        mark_review_helpful,
    ],
)


# Add GraphQL endpoint
fraiseql.attach_to_app(app, path="/graphql")


# Add REST endpoints for specific operations
@app.get("/")
async def root():
    """API root endpoint"""
    return {
        "message": "E-commerce API",
        "graphql": "/graphql",
        "playground": "/graphql",
        "docs": "/docs",
        "health": "/health",
    }


@app.get("/health")
async def health_check():
    """Health check endpoint"""
    try:
        # Check database connection
        async with app.state.db_pool.acquire() as conn:
            await conn.fetchval("SELECT 1")

        return {"status": "healthy", "database": "connected"}
    except Exception as e:
        return {"status": "unhealthy", "database": "disconnected", "error": str(e)}


@app.get("/api/products/search")
async def search_products(
    q: str,
    category: str | None = None,
    min_price: float | None = None,
    max_price: float | None = None,
    in_stock: bool | None = None,
    limit: int = 20,
    offset: int = 0,
):
    """REST endpoint for product search

    Demonstrates integration with FraiseQL's query system
    """
    query = """
    query SearchProducts(
        $q: String!, $category: String, $minPrice: Float, $maxPrice: Float,
        $inStock: Boolean, $limit: Int, $offset: Int
    ) {
        productSearch(
            where: {
                _and: [
                    {name: {_ilike: $q}},
                    {categorySlug: {_eq: $category}},
                    {minPrice: {_gte: $minPrice}},
                    {maxPrice: {_lte: $maxPrice}},
                    {inStock: {_eq: $inStock}}
                ]
            },
            limit: $limit,
            offset: $offset,
            orderBy: {averageRating: DESC, reviewCount: DESC}
        ) {
            id
            name
            slug
            shortDescription
            minPrice
            maxPrice
            inStock
            primaryImageUrl
            categoryName
            reviewCount
            averageRating
        }
    }
    """

    variables = {
        "q": f"%{q}%",
        "category": category,
        "minPrice": min_price,
        "maxPrice": max_price,
        "inStock": in_stock,
        "limit": limit,
        "offset": offset,
    }

    # Execute through FraiseQL
    result = await fraiseql.execute(query, variables)
    return result.get("data", {}).get("productSearch", [])


@app.get("/api/categories/tree")
async def get_category_tree():
    """REST endpoint for category tree"""
    query = """
    query GetCategoryTree {
        categoryTree(
            where: {parentId: {_isNull: true}, isActive: {_eq: true}},
            orderBy: {name: ASC}
        ) {
            id
            name
            slug
            productCount
            subcategories
        }
    }
    """

    result = await fraiseql.execute(query)
    return result.get("data", {}).get("categoryTree", [])


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)  # noqa: S104
