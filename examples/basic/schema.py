#!/usr/bin/env python3
"""
Basic FraiseQL Schema Definition

This example demonstrates how to define a simple blog schema
using the FraiseQL Python SDK.

Run: python3 schema.py
Output: schema.json
"""

import sys
from pathlib import Path

# Add fraiseql-python to path if running from examples directory
sys.path.insert(0, str(Path(__file__).parent.parent.parent / "fraiseql-python"))

import fraiseql
from datetime import datetime


# Define types using decorators
@fraiseql.type
class User:
    """A user in the system."""
    id: int
    name: str
    email: str
    created_at: datetime


@fraiseql.type
class Post:
    """A blog post."""
    id: int
    title: str
    content: str
    author_id: int
    author: User  # Relationship to User
    created_at: datetime


# Define queries
@fraiseql.query
def users(limit: int = 100) -> list[User]:
    """Get all users."""
    return fraiseql.config(sql_source="v_users")


@fraiseql.query
def user(id: int) -> User | None:
    """Get a user by ID."""
    return fraiseql.config(sql_source="v_users")


@fraiseql.query
def posts(limit: int = 100, author_id: int | None = None) -> list[Post]:
    """Get all posts, optionally filtered by author."""
    return fraiseql.config(sql_source="v_posts")


@fraiseql.query
def post(id: int) -> Post | None:
    """Get a post by ID."""
    return fraiseql.config(sql_source="v_posts")


# Export schema
if __name__ == "__main__":
    output_path = Path(__file__).parent / "schema.json"
    fraiseql.export_schema(str(output_path))
    print(f"Schema exported to: {output_path}")
