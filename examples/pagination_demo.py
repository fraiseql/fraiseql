"""Demo of native PostgreSQL cursor-based pagination in FraiseQL.

This example shows how to use the built-in pagination system with
Connection[T], Edge[T], and PageInfo types.
"""

import asyncio
from typing import Optional

from fraiseql import Connection, create_connection, fraise_field
from fraiseql.cqrs import CQRSRepository
from fraiseql.types import fraise_type


@fraise_type
class Author:
    """Author model."""

    id: str = fraise_field(description="Author ID")
    name: str = fraise_field(description="Author name")
    email: str = fraise_field(description="Author email")


@fraise_type
class Post:
    """Post model with pagination support."""

    id: str = fraise_field(description="Post ID")
    title: str = fraise_field(description="Post title")
    content: str = fraise_field(description="Post content")
    author_id: str = fraise_field(description="Author ID")
    created_at: str = fraise_field(description="Creation timestamp")
    is_published: bool = fraise_field(description="Published status")

    @classmethod
    def from_dict(cls, data: dict) -> "Post":
        """Create Post from dictionary."""
        return cls(
            id=data["id"],
            title=data["title"],
            content=data["content"],
            author_id=data["author_id"],
            created_at=data["created_at"],
            is_published=data.get("is_published", True),
        )


# Example query class showing how pagination would work with resolvers
# (Note: @fraiseql.field decorator will be implemented next)
async def get_paginated_posts(
    repo: CQRSRepository,
    first: Optional[int] = 20,
    after: Optional[str] = None,
    author_id: Optional[str] = None,
    published_only: bool = True,
) -> Connection[Post]:
    """Get paginated posts with optional filtering."""
    # Build filters
    filters = {}
    if author_id:
        filters["author_id"] = author_id
    if published_only:
        filters["is_published"] = True

    # Execute paginated query
    result = await repo.paginate(
        "v_posts",
        first=first,
        after=after,
        filters=filters,
        order_by="created_at",
        order_direction="DESC",
        include_total=True,
    )

    # Convert to typed Connection
    return create_connection(result, Post)


def demo_pagination_usage():
    """Demonstrate pagination usage patterns."""

    print("=== FraiseQL Native PostgreSQL Pagination Demo ===\n")

    print("1. Basic Forward Pagination:")
    print("""
    posts = await repo.paginate(
        "v_posts",
        first=20,              # Get first 20 items
        after=cursor,          # Start after this cursor
        order_by="created_at", # Order by creation date
        order_direction="DESC" # Newest first
    )
    """)

    print("2. Backward Pagination:")
    print("""
    posts = await repo.paginate(
        "v_posts",
        last=10,               # Get last 10 items
        before=cursor,         # End before this cursor
        order_by="created_at",
        order_direction="DESC"
    )
    """)

    print("3. Filtered Pagination:")
    print("""
    posts = await repo.paginate(
        "v_posts",
        first=20,
        filters={
            "author_id": "user123",
            "is_published": True,
            "tags": ["python", "graphql"]  # Array containment
        }
    )
    """)

    print("4. Converting to Typed Connection:")
    print("""
    # Get pagination result
    result = await repo.paginate("v_posts", first=20)

    # Convert to typed Connection[Post]
    connection = create_connection(result, Post)

    # Access typed data
    for edge in connection.edges:
        post = edge.node  # Fully typed Post object
        cursor = edge.cursor
        print(f"{post.title} (cursor: {cursor})")

    # Access page info
    if connection.page_info.has_next_page:
        next_cursor = connection.page_info.end_cursor
    """)

    print("\n5. GraphQL Query Example:")
    print("""
    query GetPosts($first: Int!, $after: String) {
        posts(first: $first, after: $after) {
            edges {
                node {
                    id
                    title
                    content
                    createdAt
                }
                cursor
            }
            pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
            }
            totalCount
        }
    }
    """)

    print("\n6. Performance Benefits:")
    print("   ✅ Efficient cursor-based queries (no OFFSET)")
    print("   ✅ Stable pagination (no shifting data)")
    print("   ✅ Optional total count (only when needed)")
    print("   ✅ Bi-directional navigation")
    print("   ✅ Works with any unique field for ordering")

    print("\n7. Cursor Encoding:")
    print("   - Cursors are base64-encoded for opacity")
    print("   - Contains the value of the order_by field")
    print("   - Stateless - no server-side storage needed")

    print("\n8. Database View Requirements:")
    print("""
    CREATE VIEW v_posts AS
    SELECT
        p.id,
        jsonb_build_object(
            'id', p.id,
            'title', p.title,
            'content', p.content,
            'author_id', p.author_id,
            'created_at', p.created_at,
            'is_published', p.is_published
        ) AS data
    FROM posts p;

    -- Index for efficient cursor queries
    CREATE INDEX idx_posts_created_at ON posts(created_at);
    """)


async def demo_real_pagination():
    """Demonstrate real pagination with mock data."""

    # Create mock cursor with context manager support
    class MockCursor:
        async def __aenter__(self):
            return self

        async def __aexit__(self, *args):
            pass

        async def execute(self, query, params):
            pass

        async def fetchall(self):
            # Return mock posts
            return [
                (
                    "1",
                    {
                        "id": "1",
                        "title": "First Post",
                        "content": "Content 1",
                        "author_id": "author1",
                        "created_at": "2024-01-15T10:00:00Z",
                        "is_published": True,
                    },
                ),
                (
                    "2",
                    {
                        "id": "2",
                        "title": "Second Post",
                        "content": "Content 2",
                        "author_id": "author1",
                        "created_at": "2024-01-14T10:00:00Z",
                        "is_published": True,
                    },
                ),
            ]

        async def fetchone(self):
            return (2,)  # Total count

    # Create mock connection that returns the cursor
    class MockConnection:
        def cursor(self):
            # Return a context manager that yields MockCursor
            return MockCursor()

    # Create repository with mock connection
    repo = CQRSRepository(MockConnection())

    # Execute paginated query
    result = await repo.paginate(
        "v_posts",
        first=2,
        order_by="created_at",
        order_direction="DESC",
    )

    # Convert to typed connection using helper function
    connection = create_connection(result, Post)

    print("\n=== Real Pagination Example ===")
    print(f"Total posts: {connection.total_count}")
    print(f"Has next page: {connection.page_info.has_next_page}")
    print(f"Has previous page: {connection.page_info.has_previous_page}")
    print()

    print("Posts:")
    for edge in connection.edges:
        post = edge.node
        print(f"- {post.title} (ID: {post.id}, Created: {post.created_at})")
        print(f"  Cursor: {edge.cursor}")

    print()
    print(f"Start cursor: {connection.page_info.start_cursor}")
    print(f"End cursor: {connection.page_info.end_cursor}")


if __name__ == "__main__":
    demo_pagination_usage()

    # Run async demo
    print("\n" + "=" * 50)
    asyncio.run(demo_real_pagination())
