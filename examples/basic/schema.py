#!/usr/bin/env python3
"""Basic FraiseQL schema definition.

A simple blog schema authored with the FraiseQL Python SDK.
The views it names are created by sql/setup.sql (singular Trinity naming:
tb_user/v_user, tb_post/v_post, each exposing a JSONB `data` column).

Run: python3 schema.py
Output: schema.json (consumed by `fraiseql-cli compile`)

Requires the SDK: `pip install fraiseql` (or, inside this repository,
`pip install -e sdks/official/fraiseql-python`).
"""

from pathlib import Path

import fraiseql
from fraiseql import ID, DateTime


@fraiseql.type(sql_source="v_user")
class User:
    """A user in the system."""

    id: ID
    name: str
    email: str
    created_at: DateTime


@fraiseql.type(sql_source="v_post")
class Post:
    """A blog post, denormalized with its author's identity."""

    id: ID
    title: str
    content: str
    author_id: ID
    author_name: str
    author_email: str
    created_at: DateTime


@fraiseql.query(sql_source="v_user")
def users() -> list[User]:
    """Get all users."""
    ...


@fraiseql.query(sql_source="v_user")
def user(id: ID) -> User | None:
    """Get a user by ID."""
    ...


@fraiseql.query(sql_source="v_post")
def posts() -> list[Post]:
    """Get all posts."""
    ...


@fraiseql.query(sql_source="v_post")
def post(id: ID) -> Post | None:
    """Get a post by ID."""
    ...


if __name__ == "__main__":
    output_path = Path(__file__).parent / "schema.json"
    fraiseql.export_schema(str(output_path))
    print(f"Schema exported to: {output_path}")
