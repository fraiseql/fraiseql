#!/usr/bin/env python3
"""Orders subgraph — owns Order, extends the composite-keyed User.

The extension borrows BOTH halves of the key: `organization_id` and `user_id` are each
`field(external=True)`, because the users subgraph owns them. An order therefore cannot
be attached to a user without naming the tenant.

Run: python3 schema.py
Output: schema.json (consumed by `fraiseql-cli compile`)
"""

from pathlib import Path
from typing import Annotated

import fraiseql
from fraiseql import ID, DateTime, Decimal, Federation


@fraiseql.type(sql_source="v_order", key_fields=["id"])
class Order:
    """An order. Owned by this subgraph."""

    id: ID
    organization_id: ID
    user_id: ID
    status: str
    total: Decimal
    created_at: DateTime


@fraiseql.type(
    sql_source="v_user",
    key_fields=["organization_id", "user_id"],
    extends=True,
)
class User:
    """The User this subgraph borrows, to hang its orders on."""

    organization_id: Annotated[ID, fraiseql.field(external=True)]
    user_id: Annotated[ID, fraiseql.field(external=True)]
    orders: list[Order]


@fraiseql.query(sql_source="v_order")
def orders(organization_id: ID) -> list[Order]:
    """Get every order in one organization."""
    ...


@fraiseql.query(sql_source="v_order")
def order(id: ID) -> Order | None:
    """Get an order by ID."""
    ...


@fraiseql.query(sql_source="v_order")
def user_orders(organization_id: ID, user_id: ID) -> list[Order]:
    """Get every order belonging to one user, within their organization."""
    ...


if __name__ == "__main__":
    output_path = Path(__file__).parent / "schema.json"
    fraiseql.export_schema(str(output_path), federation=Federation(service_name="orders"))
    print(f"Schema exported to: {output_path}")
