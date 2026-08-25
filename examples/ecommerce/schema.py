#!/usr/bin/env python3
"""E-commerce FraiseQL schema definition.

A catalogue, its customers and their orders: the five types the top-level
examples README advertises (Category, Product, Customer, Order, OrderItem),
with nested objects and a nested list.

The views this names are created by sql/setup.sql (singular Trinity naming:
tb_product/v_product and so on, each exposing a native `id` column and a JSONB
`data` column with snake_case keys, which FraiseQL projects to camelCase on the
GraphQL surface).

Nested objects live inside `data` — `v_product.data->'category'` and
`v_order.data->'items'` are built by the view, so a query that selects
`order { items { product { name } } }` is still one SQL statement.

Run: python3 schema.py
Output: schema.json (consumed by `fraiseql compile`)

Requires the SDK: `pip install fraiseql` (or, inside this repository,
`pip install -e sdks/official/fraiseql-python`).
"""

from enum import Enum
from pathlib import Path

import fraiseql
from fraiseql import ID, DateTime, Decimal


@fraiseql.enum
class OrderStatus(Enum):
    """Where an order is in its lifecycle."""

    PENDING = "pending"
    PAID = "paid"
    SHIPPED = "shipped"
    DELIVERED = "delivered"
    CANCELLED = "cancelled"


@fraiseql.type(sql_source="v_category")
class Category:
    """A catalogue section."""

    id: ID
    name: str
    slug: str
    description: str | None
    product_count: int
    created_at: DateTime


@fraiseql.type(embedded=True)
class ProductSummary:
    """A product as it appears inside an order line: enough to identify it."""

    id: ID
    sku: str
    name: str


@fraiseql.type(embedded=True)
class CategorySummary:
    """A category as it appears on a product."""

    id: ID
    name: str
    slug: str
    description: str | None


@fraiseql.type(sql_source="v_product")
class Product:
    """Something that can be bought, with the category it belongs to."""

    id: ID
    sku: str
    name: str
    description: str
    price: Decimal
    stock: int
    in_stock: bool
    is_active: bool
    created_at: DateTime
    category: CategorySummary


@fraiseql.type(embedded=True)
class CustomerSummary:
    """A customer as it appears on an order."""

    id: ID
    email: str
    first_name: str
    last_name: str
    full_name: str
    country: str


@fraiseql.type(sql_source="v_customer")
class Customer:
    """Someone who buys, with their order history rolled up."""

    id: ID
    email: str
    first_name: str
    last_name: str
    full_name: str
    country: str
    order_count: int
    lifetime_value: Decimal
    created_at: DateTime


@fraiseql.type(sql_source="v_order_item")
class OrderItem:
    """One line of an order."""

    id: ID
    quantity: int
    unit_price: Decimal
    total_price: Decimal
    product: ProductSummary


@fraiseql.type(sql_source="v_order")
class Order:
    """A placed order, with its customer and its lines."""

    id: ID
    order_number: str
    status: OrderStatus
    currency: str
    placed_at: DateTime
    item_count: int
    total: Decimal
    customer: CustomerSummary
    items: list[OrderItem]


# `limit`, `offset`, `where` and `order_by` are auto-params the compiler adds to
# every list query. Declaring them here would shadow the ones that paginate.
@fraiseql.query(sql_source="v_category")
def categories() -> list[Category]:
    """Every catalogue section."""
    ...


@fraiseql.query(sql_source="v_category")
def category(id: ID) -> Category | None:
    """One section by ID."""
    ...


@fraiseql.query(sql_source="v_product")
def products() -> list[Product]:
    """The catalogue."""
    ...


@fraiseql.query(sql_source="v_product")
def product(id: ID) -> Product | None:
    """One product by ID."""
    ...


@fraiseql.query(sql_source="v_customer")
def customers() -> list[Customer]:
    """Every customer, with order count and lifetime value."""
    ...


@fraiseql.query(sql_source="v_customer")
def customer(id: ID) -> Customer | None:
    """One customer by ID."""
    ...


@fraiseql.query(sql_source="v_order")
def orders() -> list[Order]:
    """Every order, newest first when ordered by placedAt."""
    ...


@fraiseql.query(sql_source="v_order")
def order(id: ID) -> Order | None:
    """One order by ID, with its customer and lines."""
    ...


if __name__ == "__main__":
    output_path = Path(__file__).parent / "schema.json"
    fraiseql.export_schema(str(output_path))
    print(f"Schema exported to: {output_path}")
