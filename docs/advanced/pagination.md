# Pagination

FraiseQL provides powerful pagination capabilities following the GraphQL Relay specification, with support for both cursor-based and offset-based pagination patterns.

## Overview

Pagination is essential for handling large datasets efficiently. FraiseQL offers two pagination approaches:

1. **Cursor-based pagination** - Recommended for most use cases
2. **Offset-based pagination** - Simple but less efficient for large datasets

## Cursor-Based Pagination

FraiseQL implements the [Relay Cursor Connections Specification](https://relay.dev/graphql/connections.htm) for cursor-based pagination, providing stable, efficient pagination even with changing data.

### Core Types

```python
from fraiseql import Connection, Edge, PageInfo, fraise_type, fraise_field

@fraise_type
class Post:
    id: str = fraise_field(description="Post ID")
    title: str = fraise_field(description="Post title")
    content: str = fraise_field(description="Post content")
    created_at: str = fraise_field(description="Creation timestamp")
```

### Using Pagination in Resolvers

```python
from fraiseql import create_connection
from fraiseql.cqrs import CQRSRepository

@fraiseql.field
async def posts(
    self,
    info: fraiseql.Info,
    first: int | None = None,
    after: str | None = None,
    last: int | None = None,
    before: str | None = None,
) -> Connection[Post]:
    """Get paginated posts."""
    repo = CQRSRepository(info.context["db"])

    # Execute paginated query
    result = await repo.paginate(
        "v_posts",
        first=first,
        after=after,
        last=last,
        before=before,
        order_by="created_at",
        order_direction="DESC",
        include_total=True,  # Optional: include total count
    )

    # Convert to typed Connection
    return create_connection(result, Post)
```

### GraphQL Query Example

```graphql
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
```

### Filtering with Pagination

```python
@fraiseql.field
async def posts_by_author(
    self,
    info: fraiseql.Info,
    author_id: str,
    first: int = 20,
    after: str | None = None,
    published_only: bool = True,
) -> Connection[Post]:
    """Get paginated posts by author."""
    repo = CQRSRepository(info.context["db"])

    # Build filters
    filters = {"author_id": author_id}
    if published_only:
        filters["is_published"] = True

    result = await repo.paginate(
        "v_posts",
        first=first,
        after=after,
        filters=filters,
        order_by="created_at",
        order_direction="DESC",
    )

    return create_connection(result, Post)
```

## Offset-Based Pagination

For simpler use cases or compatibility with existing APIs, FraiseQL also supports traditional offset-based pagination:

```python
@fraiseql.field
async def posts(
    self,
    info: fraiseql.Info,
    limit: int = 20,
    offset: int = 0,
) -> list[Post]:
    """Get posts with offset pagination."""
    # Enforce maximum limit
    limit = min(limit, 100)

    repo = CQRSRepository(info.context["db"])
    posts_data = await repo.query(
        "v_posts",
        limit=limit,
        offset=offset,
        order_by=[("created_at", "DESC")],
    )
    return [Post.from_dict(data) for data in posts_data]
```

## Database Optimization

### Indexes for Cursor Pagination

Create indexes on fields used for ordering:

```sql
-- Index for efficient cursor queries
CREATE INDEX idx_posts_created_at ON posts(created_at);

-- Composite index for filtered queries
CREATE INDEX idx_posts_author_created
ON posts(author_id, created_at);
```

### View Definition

Ensure your database views are optimized for pagination:

```sql
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
```

## Advanced Patterns

### Bi-directional Navigation

Support both forward and backward pagination:

```python
@fraiseql.field
async def posts(
    self,
    info: fraiseql.Info,
    first: int | None = None,
    after: str | None = None,
    last: int | None = None,
    before: str | None = None,
) -> Connection[Post]:
    """Bi-directional pagination support."""
    repo = CQRSRepository(info.context["db"])

    # Validate parameters
    if first is not None and last is not None:
        raise ValueError("Cannot use both 'first' and 'last'")

    result = await repo.paginate(
        "v_posts",
        first=first,
        after=after,
        last=last,
        before=before,
        order_by="created_at",
        order_direction="DESC",
    )

    return create_connection(result, Post)
```

### Custom Ordering

Paginate using different fields:

```python
@fraiseql.field
async def posts_by_popularity(
    self,
    info: fraiseql.Info,
    first: int = 20,
    after: str | None = None,
) -> Connection[Post]:
    """Order posts by view count."""
    repo = CQRSRepository(info.context["db"])

    result = await repo.paginate(
        "v_posts",
        first=first,
        after=after,
        order_by="view_count",
        order_direction="DESC",
    )

    return create_connection(result, Post)
```

### Pagination with Aggregations

Combine pagination with counts and aggregations:

```python
@fraise_type
class PostsResult:
    connection: Connection[Post] = fraise_field(
        description="Paginated posts"
    )
    total_by_status: dict[str, int] = fraise_field(
        description="Count by status"
    )

@fraiseql.field
async def posts_with_stats(
    self,
    info: fraiseql.Info,
    first: int = 20,
    after: str | None = None,
) -> PostsResult:
    """Get posts with statistics."""
    repo = CQRSRepository(info.context["db"])

    # Get paginated results
    pagination_result = await repo.paginate(
        "v_posts",
        first=first,
        after=after,
        order_by="created_at",
        order_direction="DESC",
    )

    # Get aggregated stats
    stats = await repo.query(
        "v_post_stats_by_status"
    )

    return PostsResult(
        connection=create_connection(pagination_result, Post),
        total_by_status={
            stat["status"]: stat["count"]
            for stat in stats
        }
    )
```

## Performance Considerations

### 1. Cursor Efficiency

Cursor-based pagination is more efficient than offset-based for large datasets:

```python
# ✅ Efficient: Uses index seek
WHERE created_at < '2024-01-01'
ORDER BY created_at DESC
LIMIT 20

# ❌ Inefficient: Scans and skips rows
OFFSET 10000
LIMIT 20
```

### 2. Limit Maximum Page Size

Always enforce reasonable limits:

```python
@fraiseql.field
async def posts(
    self,
    info: fraiseql.Info,
    first: int = 20,
    after: str | None = None,
) -> Connection[Post]:
    # Enforce maximum
    first = min(first or 20, 100)

    # ... pagination logic
```

### 3. Optional Total Count

The `totalCount` field can be expensive. Make it optional:

```python
# Only include total when requested
result = await repo.paginate(
    "v_posts",
    first=first,
    after=after,
    include_total=info.field_nodes[0].selection_set.selections
        and any(
            sel.name.value == "totalCount"
            for sel in info.field_nodes[0].selection_set.selections
        )
)
```

### 4. Connection Pooling

Use connection pooling for concurrent pagination requests:

```python
from psycopg_pool import AsyncConnectionPool

# Create pool
pool = AsyncConnectionPool(
    "postgresql://localhost/mydb",
    min_size=10,
    max_size=50,
)

# Use in context
app = await setup_app(
    database_url="postgresql://localhost/mydb",
    connection_pool=pool,
)
```

## Best Practices

1. **Use cursor-based pagination** for user-facing APIs
2. **Index ordering fields** to ensure efficient queries
3. **Limit page sizes** to prevent resource exhaustion
4. **Make totalCount optional** for better performance
5. **Use stable ordering** (include primary key as tie-breaker)
6. **Cache cursors** client-side for navigation
7. **Validate pagination parameters** to prevent abuse

## Common Patterns

### Search with Pagination

```python
@fraiseql.field
async def search_posts(
    self,
    info: fraiseql.Info,
    query: str,
    first: int = 20,
    after: str | None = None,
) -> Connection[Post]:
    """Full-text search with pagination."""
    repo = CQRSRepository(info.context["db"])

    result = await repo.paginate(
        "v_posts",
        first=first,
        after=after,
        filters={"search_vector": f"@@{query}"},
        order_by="created_at",
        order_direction="DESC",
    )

    return create_connection(result, Post)
```

### Nested Pagination

```python
@fraise_type
class Author:
    id: str = fraise_field()
    name: str = fraise_field()

    @fraiseql.field
    async def posts(
        self,
        info: fraiseql.Info,
        first: int = 10,
        after: str | None = None,
    ) -> Connection[Post]:
        """Author's posts with pagination."""
        repo = CQRSRepository(info.context["db"])

        result = await repo.paginate(
            "v_posts",
            first=first,
            after=after,
            filters={"author_id": self.id},
            order_by="created_at",
            order_direction="DESC",
        )

        return create_connection(result, Post)
```

## Troubleshooting

### Issue: Cursors become invalid

**Solution**: Ensure stable ordering by including a unique field:

```python
# Include ID as tie-breaker
order_by=["created_at", "id"]
```

### Issue: Performance degrades with high offsets

**Solution**: Switch to cursor-based pagination:

```python
# Replace offset-based
posts = await repo.query("v_posts", offset=10000, limit=20)

# With cursor-based
result = await repo.paginate("v_posts", first=20, after=cursor)
```

### Issue: Total count is slow

**Solution**: Use approximate counts or make it optional:

```python
# Use pg_stat_user_tables for approximate count
SELECT reltuples::BIGINT AS estimate
FROM pg_stat_user_tables
WHERE tablename = 'posts';
```

## See Also

- [GraphQL Relay Specification](https://relay.dev/graphql/connections.htm)
- [Performance Guide](./performance.md)
- [Database Views](../core-concepts/database-views.md)
- [Pagination Demo Example](https://github.com/fraiseql/fraiseql/blob/main/examples/pagination_demo.py)
