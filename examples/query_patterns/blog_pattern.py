"""Example matching the blog post pattern - simple and clean.

This shows how to create a FraiseQL app using only the @query decorator,
which provides the cleanest and most intuitive API.
"""

from datetime import UTC, datetime
from uuid import UUID

import fraiseql
from fraiseql.fastapi import create_fraiseql_app


# Define your domain types
@fraiseql.type
class Author:
    id: UUID
    name: str
    email: str
    bio: str | None = None


@fraiseql.type
class Post:
    id: UUID
    title: str
    content: str
    author: Author
    published_at: datetime | None
    tags: list[str]


# Define queries using @fraiseql.query decorator
# These are automatically registered!
@fraiseql.query
async def get_post(info, id: UUID) -> Post | None:
    """Get a blog post by ID."""
    # In production, fetch from database using info.context["db"]
    if str(id) == "123e4567-e89b-12d3-a456-426614174000":
        return Post(
            id=id,
            title="Introduction to FraiseQL",
            content="FraiseQL makes GraphQL development simple and intuitive...",
            author=Author(
                id=UUID("223e4567-e89b-12d3-a456-426614174001"),
                name="Jane Developer",
                email="jane@example.com",
                bio="GraphQL enthusiast and FraiseQL contributor",
            ),
            published_at=datetime.now(tz=UTC),
            tags=["graphql", "python", "tutorial"],
        )
    return None


@fraiseql.query
async def list_posts(
    info,
    limit: int = 10,
    offset: int = 0,
    tag: str | None = None,
) -> list[Post]:
    """List blog posts with pagination and optional tag filter."""
    # Sample data - in production, query your database
    posts = []

    for i in range(offset, offset + limit):
        posts.append(
            Post(
                id=UUID(f"{i:032x}-0000-0000-0000-000000000000"),
                title=f"Blog Post {i + 1}",
                content=f"This is the content of blog post {i + 1}...",
                author=Author(
                    id=UUID("223e4567-e89b-12d3-a456-426614174001"),
                    name="Jane Developer",
                    email="jane@example.com",
                ),
                published_at=datetime.now(tz=UTC) if i % 2 == 0 else None,
                tags=["blog", "tutorial"] if i % 2 == 0 else ["draft"],
            ),
        )

    # Filter by tag if provided
    if tag:
        posts = [p for p in posts if tag in p.tags]

    return posts


@fraiseql.query
async def search_posts(info, query: str) -> list[Post]:
    """Search posts by title or content."""
    # In production, use full-text search
    # For demo, return some results if "fraiseql" is in query
    if "fraiseql" in query.lower():
        return [
            Post(
                id=UUID("123e4567-e89b-12d3-a456-426614174000"),
                title="Introduction to FraiseQL",
                content="FraiseQL makes GraphQL development simple...",
                author=Author(
                    id=UUID("223e4567-e89b-12d3-a456-426614174001"),
                    name="Jane Developer",
                    email="jane@example.com",
                ),
                published_at=datetime.now(tz=UTC),
                tags=["graphql", "python", "tutorial"],
            ),
        ]
    return []


@fraiseql.query
async def get_author(info, id: UUID) -> Author | None:
    """Get an author by ID."""
    if str(id) == "223e4567-e89b-12d3-a456-426614174001":
        return Author(
            id=id,
            name="Jane Developer",
            email="jane@example.com",
            bio="GraphQL enthusiast and FraiseQL contributor",
        )
    return None


# Create the app - Notice how clean this is!
# No need to list queries - they're auto-registered via @query decorator
app = create_fraiseql_app(
    database_url="postgresql://localhost/blog",
    types=[Author, Post],  # Just list your types
    # That's it! All @query decorated functions are automatically included
)


# Alternative: If you want to add context
async def get_context(request):
    """Build GraphQL context from request."""
    return {
        "db": None,  # Your database connection
        "user": None,  # Authenticated user
        "request": request,
    }


# App with context
app_with_context = create_fraiseql_app(
    database_url="postgresql://localhost/blog",
    types=[Author, Post],
    context_getter=get_context,  # Custom context
)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)  # noqa: S104
