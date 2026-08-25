#!/usr/bin/env python3
"""Users subgraph — owns the User entity.

`key_fields=["id"]` makes User a federation entity: the router resolves it here by
its key, and any other subgraph may extend it (see ../orders-service/schema.py).

Run: python3 schema.py
Output: schema.json (consumed by `fraiseql-cli compile`)
"""

from pathlib import Path

import fraiseql
from fraiseql import ID, DateTime, Federation


@fraiseql.type(sql_source="v_user", key_fields=["id"])
class User:
    """A user of the platform. Owned by this subgraph."""

    id: ID
    name: str
    email: str
    created_at: DateTime


@fraiseql.query(sql_source="v_user")
def users() -> list[User]:
    """Get all users."""
    ...


@fraiseql.query(sql_source="v_user")
def user(id: ID) -> User | None:
    """Get a user by ID."""
    ...


if __name__ == "__main__":
    output_path = Path(__file__).parent / "schema.json"
    fraiseql.export_schema(str(output_path), federation=Federation(service_name="users"))
    print(f"Schema exported to: {output_path}")
