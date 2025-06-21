"""Simple working FraiseQL benchmark app."""

import os

from fraiseql import create_fraiseql_app, fraise_field, fraise_type
from fraiseql.fastapi import FraiseQLConfig


@fraise_type
class User:
    """User type."""

    id: int
    username: str
    email: str
    fullName: str


@fraise_type
class Product:
    """Product type."""

    id: int
    name: str
    price: float
    stockQuantity: int
    categoryId: int


@fraise_type
class Query:
    """Root query type."""

    # Health check - required for container
    health: str = fraise_field(default="healthy", description="Health check")

    # User queries
    users: list[User] = fraise_field(default_factory=list, description="List users")

    # Product queries
    products: list[Product] = fraise_field(default_factory=list, description="List products")


# Configure FraiseQL
config = FraiseQLConfig(
    database_url=os.environ.get("DATABASE_URL"),
    auto_camel_case=True,
)

# Create app
app = create_fraiseql_app(
    config=config,
    types=[User, Product, Query],
    title="FraiseQL Benchmark API",
)


# Add health endpoint for container health checks
@app.get("/health")
async def health_check():
    return {"status": "healthy"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)  # noqa: S104
