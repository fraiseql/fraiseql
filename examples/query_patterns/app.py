"""Example showing different query registration patterns in FraiseQL.

This example demonstrates all three ways to register queries:
1. @fraiseql.query decorator (recommended)
2. QueryRoot class with @fraiseql.field
3. Explicit query function registration
"""

from uuid import UUID

import fraiseql
from fraiseql.fastapi import create_fraiseql_app


# Define your types
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str
    role: str


@fraiseql.type
class Post:
    id: UUID
    title: str
    content: str
    author_id: UUID
    published: bool


# Pattern 1: Using @fraiseql.query decorator (RECOMMENDED)
# These queries are automatically registered when the module is imported
@fraiseql.query
async def get_user(info, id: UUID) -> User:
    """Get a user by ID."""
    # In a real app, you'd fetch from database
    return User(id=id, name="John Doe", email="john@example.com", role="admin")


@fraiseql.query
async def list_users(info, role: str | None = None) -> list[User]:
    """List all users, optionally filtered by role."""
    # In a real app, you'd query the database
    users = [
        User(
            id=UUID("123e4567-e89b-12d3-a456-426614174000"),
            name="John Doe",
            email="john@example.com",
            role="admin",
        ),
        User(
            id=UUID("223e4567-e89b-12d3-a456-426614174001"),
            name="Jane Smith",
            email="jane@example.com",
            role="user",
        ),
    ]

    if role:
        users = [u for u in users if u.role == role]

    return users


# Pattern 2: Using QueryRoot class with @fraiseql.field
@fraiseql.type
class QueryRoot:
    """Root query type for field-based queries."""

    @fraiseql.field(description="Get the current API version")
    def api_version(self, root, info) -> str:
        """Returns the API version."""
        return "1.0.0"

    @fraiseql.field
    async def stats(self, root, info) -> dict[str, int]:
        """Get application statistics."""
        # Note: dict[str, int] works as a return type
        return {"total_users": 156, "total_posts": 1024, "active_sessions": 42}


# Pattern 3: Explicit function (not decorated)
# This function must be explicitly passed to create_fraiseql_app
async def search_posts(info, query: str, published_only: bool = True) -> list[Post]:
    """Search posts by title or content."""
    # In a real app, you'd search the database
    all_posts = [
        Post(
            id=UUID("323e4567-e89b-12d3-a456-426614174002"),
            title="Getting Started with FraiseQL",
            content="FraiseQL is a modern GraphQL framework...",
            author_id=UUID("123e4567-e89b-12d3-a456-426614174000"),
            published=True,
        ),
        Post(
            id=UUID("423e4567-e89b-12d3-a456-426614174003"),
            title="Draft: Advanced Patterns",
            content="This post explores advanced patterns...",
            author_id=UUID("123e4567-e89b-12d3-a456-426614174000"),
            published=False,
        ),
    ]

    # Filter by search query
    posts = [
        p
        for p in all_posts
        if query.lower() in p.title.lower() or query.lower() in p.content.lower()
    ]

    # Filter by published status
    if published_only:
        posts = [p for p in posts if p.published]

    return posts


# Create the FastAPI app
# Note: We can mix all three patterns!
app = create_fraiseql_app(
    database_url="postgresql://localhost/example",
    # Types to include in the schema
    types=[User, Post, QueryRoot],  # QueryRoot is included here
    # Explicitly registered queries (Pattern 3)
    queries=[search_posts],  # Only non-decorated functions need to be listed
    # Note: @query decorated functions (get_user, list_users) are automatically included!
    # You don't need to list them in the queries parameter
)


# You can also create an app with ONLY auto-registered queries:
minimal_app = create_fraiseql_app(
    database_url="postgresql://localhost/example",
    types=[User, Post],  # Just the types
    # No queries parameter needed - uses @query decorated functions
)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)  # noqa: S104
