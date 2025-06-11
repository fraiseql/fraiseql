# Eliminating N+1 Queries with Database Views

One of FraiseQL's most powerful features is its ability to completely eliminate N+1 query problems through the strategic use of PostgreSQL views that pre-aggregate related data into JSONB structures.

## Understanding the N+1 Problem

The N+1 problem occurs when fetching a list of entities and their related data:

```graphql
# Traditional GraphQL might execute:
# 1 query for posts
# N queries for each post's author
# N queries for each post's comments
# N*M queries for each comment's author

query BlogFeed {
  posts {
    id
    title
    author {
      name
      avatarUrl
    }
    comments {
      content
      author {
        name
      }
    }
  }
}
```

With 10 posts, each having 5 comments, this could result in:
- 1 query for posts
- 10 queries for post authors
- 10 queries for comments
- 50 queries for comment authors
= **71 total queries!**

## The FraiseQL Solution: Composed Views

FraiseQL solves this by using PostgreSQL views that pre-aggregate all related data into JSONB structures. Here's how it works:

### 1. Basic Views for Single Entities

Start with simple views that convert normalized tables to JSONB:

```sql
-- Basic user view
CREATE VIEW v_users AS
SELECT
    id,
    jsonb_build_object(
        '__typename', 'User',
        'id', id,
        'email', email,
        'name', name,
        'bio', bio,
        'avatarUrl', avatar_url
    ) AS data
FROM tb_users;

-- Basic post view
CREATE VIEW v_posts AS
SELECT
    id,
    jsonb_build_object(
        '__typename', 'Post',
        'id', id,
        'title', title,
        'content', content,
        'authorId', author_id,
        'createdAt', created_at
    ) AS data
FROM tb_posts;
```

### 2. Composed Views with Embedded Relations

Create views that include related data using subqueries:

```sql
-- Posts with embedded author data
CREATE VIEW v_posts_with_author AS
SELECT
    p.id,
    jsonb_build_object(
        '__typename', 'Post',
        'id', p.id,
        'title', p.title,
        'content', p.content,
        -- Author data embedded directly
        'author', (
            SELECT jsonb_build_object(
                '__typename', 'User',
                'id', u.id,
                'name', u.name,
                'avatarUrl', u.avatar_url
            )
            FROM tb_users u
            WHERE u.id = p.author_id
        )
    ) AS data
FROM tb_posts p;
```

### 3. Deeply Nested Compositions

For complex queries, compose views with multiple levels of nesting:

```sql
-- Posts with author, comments, and comment authors
CREATE VIEW v_posts_full AS
SELECT
    p.id,
    jsonb_build_object(
        '__typename', 'Post',
        'id', p.id,
        'title', p.title,
        'content', p.content,
        -- Embedded author
        'author', (
            SELECT jsonb_build_object(
                '__typename', 'User',
                'id', u.id,
                'name', u.name,
                'avatarUrl', u.avatar_url
            )
            FROM tb_users u
            WHERE u.id = p.author_id
        ),
        -- Embedded comments with their authors
        'comments', COALESCE(
            (SELECT jsonb_agg(
                jsonb_build_object(
                    '__typename', 'Comment',
                    'id', c.id,
                    'content', c.content,
                    'createdAt', c.created_at,
                    -- Comment author embedded
                    'author', (
                        SELECT jsonb_build_object(
                            '__typename', 'User',
                            'id', cu.id,
                            'name', cu.name,
                            'avatarUrl', cu.avatar_url
                        )
                        FROM tb_users cu
                        WHERE cu.id = c.author_id
                    )
                )
                ORDER BY c.created_at
            )
            FROM tb_comments c
            WHERE c.post_id = p.id),
            '[]'::jsonb
        )
    ) AS data
FROM tb_posts p;
```

### 4. Using jsonb_agg for Collections

When embedding collections, use `jsonb_agg` to aggregate multiple rows:

```sql
-- User with all their posts
CREATE VIEW v_users_with_posts AS
SELECT
    u.id,
    jsonb_build_object(
        '__typename', 'User',
        'id', u.id,
        'name', u.name,
        'email', u.email,
        -- All user's posts as an array
        'posts', COALESCE(
            (SELECT jsonb_agg(
                jsonb_build_object(
                    '__typename', 'Post',
                    'id', p.id,
                    'title', p.title,
                    'excerpt', p.excerpt,
                    'publishedAt', p.published_at,
                    'viewCount', p.view_count
                )
                ORDER BY p.created_at DESC
            )
            FROM tb_posts p
            WHERE p.author_id = u.id),
            '[]'::jsonb
        ),
        -- Aggregated statistics
        'stats', jsonb_build_object(
            'postCount', (
                SELECT COUNT(*)
                FROM tb_posts
                WHERE author_id = u.id
            ),
            'totalViews', (
                SELECT COALESCE(SUM(view_count), 0)
                FROM tb_posts
                WHERE author_id = u.id
            )
        )
    ) AS data
FROM tb_users u;
```

## Key Patterns and Best Practices

### 1. Use COALESCE for Empty Collections

Always wrap `jsonb_agg` with `COALESCE` to handle empty results:

```sql
'comments', COALESCE(
    (SELECT jsonb_agg(...) FROM tb_comments WHERE ...),
    '[]'::jsonb  -- Return empty array instead of NULL
)
```

### 2. Filter Within Aggregations

Apply filters inside the subquery for better performance:

```sql
-- Only include published posts
'posts', COALESCE(
    (SELECT jsonb_agg(...)
     FROM tb_posts p
     WHERE p.author_id = u.id
     AND p.is_published = true  -- Filter here
     ORDER BY p.published_at DESC),
    '[]'::jsonb
)
```

### 3. Limit Nested Collections

For performance, limit the number of items in nested collections:

```sql
-- Only include latest 5 comments
'recentComments', (
    SELECT jsonb_agg(...)
    FROM (
        SELECT * FROM tb_comments
        WHERE post_id = p.id
        ORDER BY created_at DESC
        LIMIT 5  -- Limit here
    ) recent_comments
)
```

### 4. Create Specialized Views

Create different views for different use cases:

```sql
-- Lightweight view for lists
CREATE VIEW v_posts_summary AS
SELECT id, jsonb_build_object(
    'id', id,
    'title', title,
    'excerpt', excerpt,
    'authorName', (SELECT name FROM tb_users WHERE id = p.author_id)
) AS data
FROM tb_posts p;

-- Detailed view for single post pages
CREATE VIEW v_posts_detail AS
-- Include all relations and computed fields
```

## Performance Optimization

### 1. Strategic Indexes

Create indexes to support view queries:

```sql
-- Index for subquery lookups
CREATE INDEX idx_posts_author_id ON tb_posts(author_id);
CREATE INDEX idx_comments_post_id ON tb_comments(post_id);

-- Composite indexes for common filters
CREATE INDEX idx_posts_author_published
    ON tb_posts(author_id, is_published, created_at DESC);
```

### 2. Materialized Views

For expensive aggregations, use materialized views:

```sql
CREATE MATERIALIZED VIEW v_posts_with_stats AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'author', (...),
        'commentCount', (SELECT COUNT(*) FROM tb_comments WHERE post_id = p.id),
        'uniqueCommenters', (SELECT COUNT(DISTINCT author_id) FROM tb_comments WHERE post_id = p.id),
        'engagementScore', (
            p.view_count +
            (SELECT COUNT(*) FROM tb_comments WHERE post_id = p.id) * 10
        )
    ) AS data
FROM tb_posts p;

-- Refresh periodically
REFRESH MATERIALIZED VIEW CONCURRENTLY v_posts_with_stats;
```

### 3. Partial Materialization

Mix regular views with materialized aggregates:

```sql
-- Materialized view for stats only
CREATE MATERIALIZED VIEW v_post_stats AS
SELECT
    post_id,
    COUNT(*) as comment_count,
    COUNT(DISTINCT author_id) as unique_commenters
FROM tb_comments
GROUP BY post_id;

-- Regular view uses the materialized stats
CREATE VIEW v_posts_optimized AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'stats', jsonb_build_object(
            'commentCount', COALESCE(ps.comment_count, 0),
            'uniqueCommenters', COALESCE(ps.unique_commenters, 0)
        )
    ) AS data
FROM tb_posts p
LEFT JOIN v_post_stats ps ON ps.post_id = p.id;
```

## Integration with FraiseQL

### 1. Repository Pattern

FraiseQL's repository queries these views directly:

```python
class BlogRepository(CQRSRepository):
    async def get_posts_with_authors(self):
        """Get all posts with embedded author data."""
        return await self.select_from_json_view(
            "v_posts_with_author",
            order_by="createdAt_desc"
        )

    async def get_post_detail(self, post_id: UUID):
        """Get full post with all relations."""
        return await self.get_by_id("v_posts_full", post_id)
```

### 2. GraphQL Types

Define types that match your view structure:

```python
@fraise_type
class User:
    id: UUID
    name: str
    avatar_url: Optional[str]

@fraise_type
class Comment:
    id: UUID
    content: str
    created_at: datetime
    author: User  # Embedded from view

@fraise_type
class Post:
    id: UUID
    title: str
    content: str
    author: User  # Embedded from view
    comments: List[Comment]  # Embedded from view
```

### 3. Query Resolution

Queries resolve in a single database call:

```python
async def get_blog_feed(info) -> List[Post]:
    """Get blog feed with all related data."""
    db = info.context["db"]

    # Single query returns everything!
    posts_data = await db.select_from_json_view(
        "v_posts_full",
        where={"isPublished": True},
        order_by="publishedAt_desc",
        limit=20
    )

    return [Post.from_dict(data) for data in posts_data]
```

## Real-World Example: Blog Feed

Here's a complete example of eliminating N+1 for a blog feed:

```sql
-- Optimized blog feed view
CREATE VIEW v_blog_feed AS
SELECT
    p.id,
    jsonb_build_object(
        '__typename', 'BlogPost',
        'id', p.id,
        'title', p.title,
        'slug', p.slug,
        'excerpt', COALESCE(p.excerpt, LEFT(p.content, 200) || '...'),
        'publishedAt', p.published_at,

        -- Author summary
        'author', jsonb_build_object(
            'id', u.id,
            'name', u.name,
            'avatarUrl', u.avatar_url
        ),

        -- Pre-computed stats
        'stats', jsonb_build_object(
            'viewCount', p.view_count,
            'commentCount', (
                SELECT COUNT(*)
                FROM tb_comments
                WHERE post_id = p.id
            ),
            'readTime', CEIL(LENGTH(p.content) / 1000.0)
        ),

        -- Latest 3 comments preview
        'commentPreview', COALESCE(
            (SELECT jsonb_agg(
                jsonb_build_object(
                    'id', c.id,
                    'excerpt', LEFT(c.content, 100),
                    'authorName', cu.name,
                    'authorAvatar', cu.avatar_url,
                    'createdAt', c.created_at
                )
            )
            FROM (
                SELECT c.*, cu.name, cu.avatar_url
                FROM tb_comments c
                JOIN tb_users cu ON cu.id = c.author_id
                WHERE c.post_id = p.id
                ORDER BY c.created_at DESC
                LIMIT 3
            ) c),
            '[]'::jsonb
        )
    ) AS data
FROM tb_posts p
JOIN tb_users u ON u.id = p.author_id
WHERE p.is_published = true
ORDER BY p.published_at DESC;
```

This single view provides everything needed for a rich blog feed in **one query** instead of potentially hundreds!

## Benefits

1. **Performance**: One query instead of N+1
2. **Simplicity**: No complex ORM queries or data loaders
3. **Consistency**: Data aggregation happens at the database level
4. **Flexibility**: Different views for different use cases
5. **Caching**: Views can be materialized for better performance
6. **Type Safety**: JSONB structure matches GraphQL types exactly

## Conclusion

By leveraging PostgreSQL's powerful view system and JSONB capabilities, FraiseQL completely eliminates the N+1 query problem. This approach provides optimal performance while maintaining clean separation between your normalized write model and denormalized read model, perfectly implementing the CQRS pattern at the database level.
