#!/usr/bin/env python3
"""Users subgraph — multi-tenant, composite-key federation.

`key_fields=["organization_id", "user_id"]` renders
`@key(fields: "organizationId userId")`. A user's identity is the PAIR, so no subgraph
can resolve one without naming the tenant, and a cross-tenant reference is not
expressible in the graph.

Run: python3 schema.py
Output: schema.json (consumed by `fraiseql-cli compile`)
"""

from pathlib import Path

import fraiseql
from fraiseql import ID, DateTime, Federation


@fraiseql.type(sql_source="v_organization", key_fields=["id"])
class Organization:
    """A tenant. Owned by this subgraph."""

    id: ID
    name: str
    created_at: DateTime


@fraiseql.type(sql_source="v_user", key_fields=["organization_id", "user_id"])
class User:
    """A user within one organization. Owned by this subgraph."""

    organization_id: ID
    user_id: ID
    name: str
    email: str
    role: str
    created_at: DateTime


@fraiseql.query(sql_source="v_organization")
def organizations() -> list[Organization]:
    """Get all organizations."""
    ...


@fraiseql.query(sql_source="v_organization")
def organization(id: ID) -> Organization | None:
    """Get an organization by ID."""
    ...


@fraiseql.query(sql_source="v_user")
def users(organization_id: ID) -> list[User]:
    """Get every user in one organization."""
    ...


@fraiseql.query(sql_source="v_user")
def user(organization_id: ID, user_id: ID) -> User | None:
    """Get one user by the composite key."""
    ...


if __name__ == "__main__":
    output_path = Path(__file__).parent / "schema.json"
    fraiseql.export_schema(str(output_path), federation=Federation(service_name="users"))
    print(f"Schema exported to: {output_path}")
