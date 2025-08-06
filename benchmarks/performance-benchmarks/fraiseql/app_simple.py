"""Simplified FraiseQL benchmark application for testing"""

import os
from typing import Optional

from fraiseql import fraise_field, fraise_type
from fraiseql.fastapi import create_fraiseql_app


# Simple types without imports
@fraise_type
class User:
    id: str = fraise_field(purpose="identifier")
    email: str = fraise_field(purpose="email")
    username: str = fraise_field(purpose="username")
    fullName: str = fraise_field(purpose="display name")
    isActive: bool = fraise_field(purpose="status")


@fraise_type
class Product:
    id: str = fraise_field(purpose="identifier")
    name: str = fraise_field(purpose="product name")
    price: float = fraise_field(purpose="price")
    stockQuantity: int = fraise_field(purpose="stock")


@fraise_type
class Order:
    id: str = fraise_field(purpose="identifier")
    orderNumber: str = fraise_field(purpose="order number")
    status: str = fraise_field(purpose="order status")
    totalAmount: float = fraise_field(purpose="total")


# Query root
@fraise_type
class Query:
    users: list[User] = fraise_field(purpose="list all users")
    user: Optional[User] = fraise_field(purpose="get user by ID")
    products: list[Product] = fraise_field(purpose="list all products")
    orders: list[Order] = fraise_field(purpose="list all orders")


# Database URL without pydantic
DATABASE_URL = os.getenv(
    "DATABASE_URL",
    "postgresql://benchmark:benchmark@postgres:5432/benchmark_db?options=-csearch_path=benchmark",
)

# Create app
app = create_fraiseql_app(
    database_url=DATABASE_URL,
    types=[Query, User, Product, Order],
    title="FraiseQL Benchmark",
    version="1.0.0",
)

if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)  # noqa: S104
