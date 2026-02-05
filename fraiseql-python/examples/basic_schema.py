"""Example FraiseQL schema definition.

This example demonstrates:
- Type definitions with @fraiseql.type
- Query definitions with @fraiseql.query
- Mutation definitions with @fraiseql.mutation
- Schema export to JSON

Usage:
    python examples/basic_schema.py
    # Creates schema.json that can be compiled with: fraiseql-cli compile schema.json
"""

import fraiseql


@fraiseql.type
class User:
    """User type representing a user in the system."""

    id: int
    name: str
    email: str
    created_at: str
    is_active: bool


@fraiseql.type
class Post:
    """Post type representing a blog post."""

    id: int
    title: str
    content: str
    author_id: int
    published: bool
    created_at: str


@fraiseql.query(
    sql_source="v_user",
    auto_params={"limit": True, "offset": True, "where": True, "order_by": True},
)
def users(limit: int = 10, offset: int = 0, is_active: bool | None = None) -> list[User]:
    """Get list of users with pagination.

    Args:
        limit: Maximum number of users to return
        offset: Number of users to skip
        is_active: Filter by active status (optional)

    Returns:
        List of User objects
    """
    pass


@fraiseql.query(sql_source="v_user")
def user(id: int) -> User | None:
    """Get a single user by ID.

    Args:
        id: User ID

    Returns:
        User object or None if not found
    """
    pass


@fraiseql.query(
    sql_source="v_post",
    auto_params={"limit": True, "offset": True, "where": True, "order_by": True},
)
def posts(author_id: int | None = None, published: bool = True) -> list[Post]:
    """Get list of posts with filtering.

    Args:
        author_id: Filter by author ID (optional)
        published: Filter by published status

    Returns:
        List of Post objects
    """
    pass


@fraiseql.mutation(sql_source="fn_create_user", operation="CREATE")
def create_user(name: str, email: str) -> User:
    """Create a new user.

    Args:
        name: User's full name
        email: User's email address

    Returns:
        Created User object
    """
    pass


@fraiseql.mutation(sql_source="fn_update_user", operation="UPDATE")
def update_user(id: int, name: str | None = None, email: str | None = None) -> User:
    """Update an existing user.

    Args:
        id: User ID to update
        name: New name (optional)
        email: New email (optional)

    Returns:
        Updated User object
    """
    pass


@fraiseql.mutation(sql_source="fn_delete_user", operation="DELETE")
def delete_user(id: int) -> User:
    """Delete a user.

    Args:
        id: User ID to delete

    Returns:
        Deleted User object
    """
    pass


@fraiseql.mutation(sql_source="fn_create_post", operation="CREATE")
def create_post(title: str, content: str, author_id: int) -> Post:
    """Create a new blog post.

    Args:
        title: Post title
        content: Post content
        author_id: ID of the author

    Returns:
        Created Post object
    """
    pass


if __name__ == "__main__":
    # Export schema to JSON
    fraiseql.export_schema("schema.json")

    print("\n✅ Schema exported successfully!")
    print("   Next steps:")
    print("   1. Compile schema: fraiseql-cli compile schema.json")
    print("   2. Start server: fraiseql-server --schema schema.compiled.json")
