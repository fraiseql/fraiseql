#!/usr/bin/env python3
"""Orders subgraph — owns Order, and extends the User that the users subgraph owns.

Two entities, two different roles:

* `Order` is owned here: this subgraph holds the rows and resolves the whole type.
* `User` is **extended** here: `extends=True` renders `extend type User @key(...)`,
  and the borrowed key field is marked `field(external=True)` because another
  subgraph owns it. This subgraph contributes exactly one field, `orders`, and the
  router stitches it onto the User the users subgraph returned.

There is no users table in this database. The only thing this subgraph ever learns
about a user is the `id` the router passes in an `_entities` representation.

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
    user_id: ID
    status: str
    total: Decimal
    created_at: DateTime


@fraiseql.type(sql_source="v_user", key_fields=["id"], extends=True)
class User:
    """The User this subgraph borrows, to hang its orders on."""

    id: Annotated[ID, fraiseql.field(external=True)]
    orders: list[Order]


@fraiseql.query(sql_source="v_order")
def orders() -> list[Order]:
    """Get all orders."""
    ...


@fraiseql.query(sql_source="v_order")
def order(id: ID) -> Order | None:
    """Get an order by ID."""
    ...


@fraiseql.query(sql_source="v_order")
def user_orders(user_id: ID) -> list[Order]:
    """Get every order belonging to one user."""
    ...


if __name__ == "__main__":
    output_path = Path(__file__).parent / "schema.json"
    fraiseql.export_schema(str(output_path), federation=Federation(service_name="orders"))
    print(f"Schema exported to: {output_path}")
