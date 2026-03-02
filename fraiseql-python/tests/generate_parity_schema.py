#!/usr/bin/env python3
"""Generate parity schema for cross-SDK comparison.

Usage:
    uv run python tests/generate_parity_schema.py
"""

import json
import sys

sys.path.insert(0, "src")

from fraiseql.registry import SchemaRegistry  # noqa: E402
import fraiseql  # noqa: E402
from fraiseql.scalars import ID  # noqa: E402

# Reset registry to avoid contamination from previous imports
SchemaRegistry.clear()


# --- Types ---


@fraiseql.type
class User:
    id: ID
    email: str
    name: str


@fraiseql.type
class Order:
    id: ID
    total: float


@fraiseql.error
class UserNotFound:
    message: str
    code: str


# --- Queries ---


@fraiseql.query(sql_source="v_user", auto_params=True)
def users() -> list[User]:
    """List all users."""


@fraiseql.query(
    sql_source="v_order",
    inject={"tenant_id": "jwt:tenant_id"},
    cache_ttl_seconds=300,
    requires_role="admin",
)
def tenantOrders() -> list[Order]:
    """List tenant orders (admin only)."""


# --- Mutations ---


@fraiseql.mutation(sql_source="fn_create_user", operation="insert")
def createUser(email: str, name: str) -> User:
    """Create a new user."""


@fraiseql.mutation(
    sql_source="fn_place_order",
    operation="insert",
    inject={"user_id": "jwt:sub"},
    invalidates_views=["v_order_summary"],
    invalidates_fact_tables=["tf_sales"],
)
def placeOrder() -> Order:
    """Place a new order."""


# Output schema as JSON
schema = SchemaRegistry.get_schema()
output = {
    "types": schema["types"],
    "queries": schema["queries"],
    "mutations": schema["mutations"],
}
print(json.dumps(output, indent=2))
