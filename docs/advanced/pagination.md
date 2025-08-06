# Pagination in FraiseQL

Pagination is crucial for building performant APIs. FraiseQL provides three pagination strategies, each optimized for different use cases. Let's explore how to implement them effectively.

## Quick Start

The simplest pagination uses `LIMIT` and `OFFSET` in your view:

```sql
-- Basic pagination view
CREATE VIEW v_post_paginated AS
SELECT jsonb_build_object(
    'posts', (
        SELECT jsonb_agg(
            jsonb_build_object(
                'id', p.id,
                'title', p.title,
                'content', p.content,
                'created_at', p.created_at
            )
        )
        FROM (
            SELECT * FROM posts
            ORDER BY created_at DESC
            LIMIT COALESCE($1, 10)  -- Default to 10 items
            OFFSET COALESCE($2, 0)   -- Default to first page
        ) p
    ),
    'total', (SELECT COUNT(*) FROM posts)
) as data;
```

```python
# Using in Python
from fraiseql import FraiseQL

app = FraiseQL(database_url="postgresql://...")

# Query with pagination
result = await app.repository.query(
    "v_post_paginated",
    params=[20, 40]  # LIMIT 20 OFFSET 40 (page 3)
)
```

## Three Pagination Types Explained

### 1. Offset/Limit Pagination (Simple but Limited)

**When to use**: Small datasets, simple UIs, prototyping

**Pros**: 
- Dead simple to implement
- Works with any ordering
- Easy to jump to specific pages

**Cons**:
- Performance degrades with large offsets
- Can miss or duplicate items if data changes
- Not suitable for real-time data

```sql
-- Offset pagination with metadata
CREATE VIEW v_products_offset AS
SELECT jsonb_build_object(
    'items', (
        SELECT jsonb_agg(data)
        FROM (
            SELECT jsonb_build_object(
                'id', p.id,
                'name', p.name,
                'price', p.price,
                'category', (SELECT data FROM v_category WHERE id = p.category_id)
            ) as data
            FROM products p
            WHERE ($3::text IS NULL OR p.category = $3)
            ORDER BY p.name ASC
            LIMIT $1
            OFFSET $2
        ) filtered
    ),
    'pagination', jsonb_build_object(
        'page', ($2 / $1) + 1,
        'pageSize', $1,
        'total', (
            SELECT COUNT(*) 
            FROM products 
            WHERE ($3::text IS NULL OR category = $3)
        ),
        'totalPages', CEILING(
            (SELECT COUNT(*)::float FROM products WHERE ($3::text IS NULL OR category = $3)) / $1
        )
    )
) as data;
```

```python
# Python implementation
from dataclasses import dataclass

@dataclass
class OffsetPagination:
    page: int = 1
    page_size: int = 20
    
    @property
    def offset(self) -> int:
        return (self.page - 1) * self.page_size
    
    @property
    def limit(self) -> int:
        return self.page_size

async def get_products(pagination: OffsetPagination, category: str | None = None):
    return await repo.query(
        "v_products_offset",
        params=[pagination.limit, pagination.offset, category]
    )
```

### 2. Cursor-Based Pagination (Recommended)

**When to use**: Large datasets, infinite scroll, real-time feeds, GraphQL APIs

**Pros**:
- Consistent performance regardless of position
- No missed/duplicate items
- Perfect for GraphQL Relay spec
- Supports bi-directional navigation

**Cons**:
- Can't jump to arbitrary pages
- Slightly more complex implementation
- Requires unique, orderable field

```sql
-- Cursor-based pagination view (Relay-compatible)
CREATE VIEW v_post_cursor AS
WITH paginated AS (
    SELECT 
        p.id,
        p.title,
        p.content,
        p.created_at,
        p.author_id,
        -- Generate cursor from timestamp + id for uniqueness
        encode(
            (p.created_at::text || ':' || p.id::text)::bytea, 
            'base64'
        ) as cursor
    FROM posts p
    WHERE 
        -- After cursor filter
        ($1::text IS NULL OR 
         (p.created_at, p.id) > (
            split_part(decode($1, 'base64')::text, ':', 1)::timestamp,
            split_part(decode($1, 'base64')::text, ':', 2)::uuid
         ))
        -- Before cursor filter  
        AND ($2::text IS NULL OR
         (p.created_at, p.id) < (
            split_part(decode($2, 'base64')::text, ':', 1)::timestamp,
            split_part(decode($2, 'base64')::text, ':', 2)::uuid
         ))
    ORDER BY p.created_at DESC, p.id DESC
    LIMIT $3
)
SELECT jsonb_build_object(
    'edges', (
        SELECT jsonb_agg(
            jsonb_build_object(
                'cursor', cursor,
                'node', jsonb_build_object(
                    'id', id,
                    'title', title,
                    'content', content,
                    'createdAt', created_at,
                    'author', (SELECT data FROM v_user WHERE id = author_id)
                )
            )
        )
        FROM paginated
    ),
    'pageInfo', jsonb_build_object(
        'hasNextPage', (
            SELECT EXISTS(
                SELECT 1 FROM posts p2
                WHERE (p2.created_at, p2.id) < (
                    SELECT (created_at, id) 
                    FROM paginated 
                    ORDER BY created_at DESC, id DESC 
                    LIMIT 1
                )
            )
        ),
        'hasPreviousPage', (
            SELECT EXISTS(
                SELECT 1 FROM posts p2
                WHERE (p2.created_at, p2.id) > (
                    SELECT (created_at, id) 
                    FROM paginated 
                    ORDER BY created_at ASC, id ASC 
                    LIMIT 1
                )
            )
        ),
        'startCursor', (SELECT cursor FROM paginated ORDER BY created_at DESC, id DESC LIMIT 1),
        'endCursor', (SELECT cursor FROM paginated ORDER BY created_at ASC, id ASC LIMIT 1)
    ),
    'totalCount', (SELECT COUNT(*) FROM posts)
) as data;
```

```python
# Using FraiseQL's built-in cursor pagination
from fraiseql.cqrs.pagination import CursorPaginator, PaginationParams

async def get_posts_relay(
    first: int | None = None,
    after: str | None = None,
    last: int | None = None,
    before: str | None = None
):
    paginator = CursorPaginator(connection)
    params = PaginationParams(
        first=first,
        after=after,
        last=last,
        before=before,
        order_by="created_at",
        order_direction="DESC"
    )
    
    return await paginator.paginate(
        view_name="v_post",
        params=params,
        include_total=True
    )

# GraphQL schema using cursor pagination
type PostConnection {
    edges: [PostEdge!]!
    pageInfo: PageInfo!
    totalCount: Int
}

type PostEdge {
    cursor: String!
    node: Post!
}

type PageInfo {
    hasNextPage: Boolean!
    hasPreviousPage: Boolean!
    startCursor: String
    endCursor: String
}
```

### 3. Keyset Pagination (For Large Datasets)

**When to use**: Very large tables (millions of rows), consistent read performance, data exports

**Pros**:
- Best performance for large datasets
- Consistent query time
- Works well with indexes
- No offset performance penalty

**Cons**:
- Requires stable sort order
- Complex for multi-column sorting
- No backward navigation without reversing

```sql
-- Keyset pagination using timestamp + id
CREATE VIEW v_events_keyset AS
SELECT jsonb_build_object(
    'events', (
        SELECT jsonb_agg(
            jsonb_build_object(
                'id', e.id,
                'type', e.event_type,
                'payload', e.payload,
                'timestamp', e.created_at,
                -- Include keys for next page
                'nextKey', jsonb_build_object(
                    'timestamp', e.created_at,
                    'id', e.id
                )
            )
        )
        FROM (
            SELECT * FROM events
            WHERE 
                -- Keyset condition: events after the given timestamp/id
                (created_at, id) > (
                    COALESCE($1::timestamp, '1970-01-01'::timestamp),
                    COALESCE($2::uuid, '00000000-0000-0000-0000-000000000000'::uuid)
                )
            ORDER BY created_at ASC, id ASC
            LIMIT $3
        ) e
    ),
    'hasMore', (
        SELECT EXISTS(
            SELECT 1 FROM events
            WHERE (created_at, id) > (
                SELECT (created_at, id) 
                FROM events
                WHERE (created_at, id) > (
                    COALESCE($1::timestamp, '1970-01-01'::timestamp),
                    COALESCE($2::uuid, '00000000-0000-0000-0000-000000000000'::uuid)
                )
                ORDER BY created_at ASC, id ASC
                LIMIT $3
                OFFSET $3 - 1
            )
        )
    )
) as data;
```

```python
# Keyset pagination implementation
from datetime import datetime
from uuid import UUID

class KeysetPaginator:
    def __init__(self, page_size: int = 100):
        self.page_size = page_size
    
    async def get_page(
        self,
        after_timestamp: datetime | None = None,
        after_id: UUID | None = None
    ) -> dict:
        result = await repo.query(
            "v_events_keyset",
            params=[after_timestamp, after_id, self.page_size]
        )
        
        # Extract last item's keys for next page
        events = result['data']['events']
        if events and result['data']['hasMore']:
            last_event = events[-1]
            next_key = last_event['nextKey']
            result['data']['nextPageParams'] = {
                'after_timestamp': next_key['timestamp'],
                'after_id': next_key['id']
            }
        
        return result

# Usage for streaming large datasets
paginator = KeysetPaginator(page_size=1000)
page = await paginator.get_page()

while page['data']['hasMore']:
    # Process current page
    process_events(page['data']['events'])
    
    # Get next page using keyset
    next_params = page['data']['nextPageParams']
    page = await paginator.get_page(
        after_timestamp=next_params['after_timestamp'],
        after_id=next_params['after_id']
    )
```

## Performance Tips and Pitfalls

### 1. Always Use Indexes

```sql
-- Essential indexes for pagination
CREATE INDEX idx_posts_created_at_id ON posts(created_at DESC, id DESC);
CREATE INDEX idx_events_timestamp_id ON events(created_at ASC, id ASC);

-- For filtered pagination
CREATE INDEX idx_products_category_name ON products(category, name);
```

### 2. Avoid COUNT(*) for Large Tables

```sql
-- Instead of exact counts for large tables
CREATE VIEW v_post_fast AS
SELECT jsonb_build_object(
    'posts', (SELECT jsonb_agg(data) FROM ...),
    -- Use estimate for large tables
    'totalEstimate', (
        SELECT reltuples::bigint 
        FROM pg_class 
        WHERE relname = 'posts'
    )
) as data;

-- Or use a cached count
CREATE MATERIALIZED VIEW mv_post_count AS
SELECT COUNT(*) as total FROM posts;

-- Refresh periodically
REFRESH MATERIALIZED VIEW CONCURRENTLY mv_post_count;
```

### 3. Optimize for Common Patterns

```sql
-- Pre-built view for "load more" pattern
CREATE VIEW v_feed_infinite AS
WITH RECURSIVE pages AS (
    -- First page
    SELECT 
        1 as page_num,
        ARRAY(
            SELECT id FROM posts 
            ORDER BY created_at DESC 
            LIMIT 20
        ) as ids
    
    UNION ALL
    
    -- Subsequent pages
    SELECT 
        p.page_num + 1,
        ARRAY(
            SELECT id FROM posts
            WHERE created_at < (
                SELECT created_at FROM posts WHERE id = ANY(p.ids)
                ORDER BY created_at ASC LIMIT 1
            )
            ORDER BY created_at DESC
            LIMIT 20
        )
    FROM pages p
    WHERE p.page_num < $1  -- Max pages to pre-fetch
)
SELECT jsonb_build_object(
    'pages', (
        SELECT jsonb_agg(
            jsonb_build_object(
                'page', page_num,
                'posts', (
                    SELECT jsonb_agg(data)
                    FROM v_post
                    WHERE id = ANY(pages.ids)
                )
            )
        )
        FROM pages
    )
) as data;
```

## Integration with GraphQL Standards

### Relay Connection Specification

FraiseQL's cursor pagination is designed to work seamlessly with Relay:

```python
from strawberry import type, field
from fraiseql import FraiseQL

@type
class Query:
    @field
    async def posts(
        self,
        first: int | None = None,
        after: str | None = None,
        last: int | None = None,
        before: str | None = None
    ) -> PostConnection:
        # FraiseQL handles Relay spec automatically
        result = await app.repository.query(
            "v_post_cursor",
            params=[after, before, first or 20]
        )
        
        return PostConnection.from_dict(result['data'])
```

### Custom Pagination Wrappers

```python
# Create reusable pagination utilities
from typing import Generic, TypeVar

T = TypeVar('T')

class PaginatedResponse(Generic[T]):
    def __init__(self, items: list[T], has_more: bool, cursor: str | None = None):
        self.items = items
        self.has_more = has_more
        self.cursor = cursor
    
    def to_graphql(self) -> dict:
        return {
            'nodes': self.items,
            'pageInfo': {
                'hasNextPage': self.has_more,
                'endCursor': self.cursor
            }
        }

# Use with any view
async def paginated_query(
    view_name: str,
    limit: int = 20,
    cursor: str | None = None
) -> PaginatedResponse:
    result = await repo.query(
        view_name,
        params=[cursor, limit + 1]  # Fetch one extra to check hasMore
    )
    
    items = result['data']['items']
    has_more = len(items) > limit
    
    if has_more:
        items = items[:limit]
    
    last_cursor = items[-1].get('cursor') if items else None
    
    return PaginatedResponse(items, has_more, last_cursor)
```

## Choosing the Right Approach

| Use Case | Recommended | Why |
|----------|-------------|-----|
| **Admin dashboard tables** | Offset/Limit | Users expect page numbers |
| **Social media feeds** | Cursor-based | Infinite scroll, real-time updates |
| **Data exports** | Keyset | Best performance for large datasets |
| **Search results** | Offset/Limit or Cursor | Depends on UI requirements |
| **API with GraphQL** | Cursor-based | Follows GraphQL best practices |
| **Mobile app lists** | Cursor-based | Handles connection interruptions |
| **Report generation** | Keyset | Consistent performance |

## Common Pitfalls and Solutions

### Pitfall 1: Using OFFSET with Large Values

```sql
-- ❌ Bad: OFFSET 10000 scans 10000 rows
SELECT * FROM posts ORDER BY created_at OFFSET 10000 LIMIT 20;

-- ✅ Good: Keyset pagination jumps directly
SELECT * FROM posts 
WHERE (created_at, id) > ('2024-01-01', 'uuid-here')
ORDER BY created_at, id
LIMIT 20;
```

### Pitfall 2: Not Handling Empty Results

```sql
-- ✅ Handle empty results gracefully
CREATE VIEW v_safe_pagination AS
SELECT jsonb_build_object(
    'items', COALESCE(
        (SELECT jsonb_agg(data) FROM ...),
        '[]'::jsonb  -- Return empty array, not null
    ),
    'hasMore', COALESCE(
        (SELECT EXISTS(...)),
        false
    )
) as data;
```

### Pitfall 3: Forgetting Unique Ordering

```sql
-- ❌ Bad: Non-deterministic ordering
ORDER BY created_at  -- Multiple records can have same timestamp

-- ✅ Good: Deterministic with tie-breaker
ORDER BY created_at, id  -- ID ensures unique ordering
```

## Testing Your Pagination

```python
import pytest
from fraiseql import FraiseQL

@pytest.mark.asyncio
async def test_pagination_consistency():
    """Ensure no items are missed or duplicated."""
    all_items = set()
    cursor = None
    
    while True:
        page = await app.repository.query(
            "v_post_cursor",
            params=[cursor, None, 10, None]  # first=10
        )
        
        edges = page['data']['edges']
        if not edges:
            break
            
        for edge in edges:
            # Check for duplicates
            assert edge['node']['id'] not in all_items
            all_items.add(edge['node']['id'])
        
        if not page['data']['pageInfo']['hasNextPage']:
            break
            
        cursor = page['data']['pageInfo']['endCursor']
    
    # Verify we got all items
    total = await app.repository.query("v_post_count")
    assert len(all_items) == total['data']['count']
```

## Summary

FraiseQL provides flexible pagination strategies that leverage PostgreSQL's power:

1. **Start simple** with offset/limit for prototypes
2. **Use cursor-based** for production APIs (especially GraphQL)
3. **Switch to keyset** for large-scale data operations
4. **Always index** your ordering columns
5. **Compose views** to reuse pagination logic

The beauty of FraiseQL's approach is that pagination logic lives in your SQL views, making it testable, optimizable, and reusable across different API endpoints. Your application code stays clean while PostgreSQL handles the heavy lifting.

Ready to implement authentication? Check out our [Authentication Guide](/advanced/authentication) or explore [Performance Optimization](/advanced/performance) to make your paginated queries even faster.