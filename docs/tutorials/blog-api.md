# Blog API Tutorial

Complete blog application demonstrating FraiseQL's CQRS architecture, N+1 prevention, and production patterns.

## Overview

Build a blog API with:
- Users, posts, and threaded comments
- JSONB composition (single-query nested data)
- Mutation functions with explicit side effects
- Production-ready patterns

**Time**: 30-45 minutes
**Prerequisites**: Completed [quickstart](../quickstart.md), basic PostgreSQL knowledge

## Database Schema

### Tables (Write Side)

```sql
-- Users
CREATE TABLE tb_user (
    id SERIAL PRIMARY KEY,
    pk_user UUID DEFAULT gen_random_uuid() UNIQUE,
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    bio TEXT,
    avatar_url VARCHAR(500),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Posts
CREATE TABLE tb_post (
    id SERIAL PRIMARY KEY,
    pk_post UUID DEFAULT gen_random_uuid() UNIQUE,
    fk_author INTEGER REFERENCES tb_user(id),
    title VARCHAR(500) NOT NULL,
    slug VARCHAR(500) UNIQUE NOT NULL,
    content TEXT NOT NULL,
    excerpt TEXT,
    tags TEXT[] DEFAULT '{}',
    is_published BOOLEAN DEFAULT false,
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Comments (with threading)
CREATE TABLE tb_comment (
    id SERIAL PRIMARY KEY,
    pk_comment UUID DEFAULT gen_random_uuid() UNIQUE,
    fk_post INTEGER REFERENCES tb_post(id) ON DELETE CASCADE,
    fk_author INTEGER REFERENCES tb_user(id),
    fk_parent INTEGER REFERENCES tb_comment(id),
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes for performance
CREATE INDEX idx_post_author ON tb_post(fk_author);
CREATE INDEX idx_post_published ON tb_post(is_published, published_at DESC);
CREATE INDEX idx_comment_post ON tb_comment(fk_post, created_at);
CREATE INDEX idx_comment_parent ON tb_comment(fk_parent);
```

### Views (Read Side)

**N+1 Prevention Pattern**: Compose nested data in views.

```sql
-- Basic user view
CREATE VIEW v_user AS
SELECT
    id,
    jsonb_build_object(
        '__typename', 'User',
        'id', pk_user,
        'email', email,
        'name', name,
        'bio', bio,
        'avatarUrl', avatar_url,
        'createdAt', created_at
    ) AS data
FROM tb_user;

-- Post with embedded author
CREATE VIEW v_post AS
SELECT
    p.id,
    p.fk_author,
    p.is_published,
    p.created_at,
    jsonb_build_object(
        '__typename', 'Post',
        'id', p.pk_post,
        'title', p.title,
        'slug', p.slug,
        'content', p.content,
        'excerpt', p.excerpt,
        'tags', p.tags,
        'isPublished', p.is_published,
        'publishedAt', p.published_at,
        'createdAt', p.created_at,
        'author', (SELECT data FROM v_user WHERE id = p.fk_author)
    ) AS data
FROM tb_post p;

-- Comment with author, post, and replies (prevents N+1!)
CREATE VIEW v_comment AS
SELECT
    c.id,
    c.fk_post,
    c.created_at,
    jsonb_build_object(
        '__typename', 'Comment',
        'id', c.pk_comment,
        'content', c.content,
        'createdAt', c.created_at,
        'author', (SELECT data FROM v_user WHERE id = c.fk_author),
        'post', (
            SELECT jsonb_build_object(
                '__typename', 'Post',
                'id', p.pk_post,
                'title', p.title
            )
            FROM tb_post p WHERE p.id = c.fk_post
        ),
        'replies', COALESCE(
            (SELECT jsonb_agg(
                jsonb_build_object(
                    '__typename', 'Comment',
                    'id', r.pk_comment,
                    'content', r.content,
                    'createdAt', r.created_at,
                    'author', (SELECT data FROM v_user WHERE id = r.fk_author)
                ) ORDER BY r.created_at
            )
            FROM tb_comment r
            WHERE r.fk_parent = c.id),
            '[]'::jsonb
        )
    ) AS data
FROM tb_comment c;

-- Full post view with comments
CREATE VIEW v_post_full AS
SELECT
    p.id,
    p.is_published,
    p.created_at,
    jsonb_build_object(
        '__typename', 'Post',
        'id', p.pk_post,
        'title', p.title,
        'slug', p.slug,
        'content', p.content,
        'excerpt', p.excerpt,
        'tags', p.tags,
        'isPublished', p.is_published,
        'publishedAt', p.published_at,
        'createdAt', p.created_at,
        'author', (SELECT data FROM v_user WHERE id = p.fk_author),
        'comments', COALESCE(
            (SELECT jsonb_agg(data ORDER BY created_at)
             FROM v_comment
             WHERE fk_post = p.id AND fk_parent IS NULL),
            '[]'::jsonb
        )
    ) AS data
FROM tb_post p;
```

**Performance**: Fetching post + author + comments + replies = **1 query** (not N+1).

## GraphQL Types

```python
from datetime import datetime
from uuid import UUID
from fraiseql import type
from typing import List

@type(sql_source="v_user")
class User:
    id: UUID
    email: str
    name: str
    bio: str | None
    avatar_url: str | None
    created_at: datetime

@type(sql_source="v_comment")
class Comment:
    id: UUID
    content: str
    created_at: datetime
    author: User
    post: "Post"
    replies: List["Comment"]

@type(sql_source="v_post")
class Post:
    id: UUID
    title: str
    slug: str
    content: str
    excerpt: str | None
    tags: List[str]
    is_published: bool
    published_at: datetime | None
    created_at: datetime
    author: User
    comments: List[Comment]
```

## Queries

```python
from uuid import UUID
from fraiseql import query
from typing import List, Optional

@query
def get_post(id: UUID) -> Optional[Post]:
    """Get single post with all nested data."""
    pass  # Implementation handled by framework

@query
def get_posts(
    is_published: Optional[bool] = None,
    limit: int = 20,
    offset: int = 0
) -> List[Post]:
    """List posts with filtering and pagination."""
    pass  # Implementation handled by framework
```

## Mutations

**Pattern**: PostgreSQL functions handle business logic.

```sql
-- Create post function
CREATE OR REPLACE FUNCTION fn_create_post(
    p_author_id UUID,
    p_title TEXT,
    p_content TEXT,
    p_excerpt TEXT DEFAULT NULL,
    p_tags TEXT[] DEFAULT '{}',
    p_is_published BOOLEAN DEFAULT false
)
RETURNS UUID AS $$
DECLARE
    v_post_id INTEGER;
    v_post_pk UUID;
    v_author_id INTEGER;
    v_slug TEXT;
BEGIN
    -- Get author internal ID
    SELECT id INTO v_author_id
    FROM tb_user WHERE pk_user = p_author_id;

    IF v_author_id IS NULL THEN
        RAISE EXCEPTION 'Author not found: %', p_author_id;
    END IF;

    -- Generate slug
    v_slug := lower(regexp_replace(p_title, '[^a-zA-Z0-9]+', '-', 'g'));
    v_slug := trim(both '-' from v_slug);
    v_slug := v_slug || '-' || substr(md5(random()::text), 1, 8);

    -- Insert post
    INSERT INTO tb_post (
        fk_author, title, slug, content, excerpt, tags,
        is_published, published_at
    )
    VALUES (
        v_author_id, p_title, v_slug, p_content, p_excerpt, p_tags,
        p_is_published,
        CASE WHEN p_is_published THEN NOW() ELSE NULL END
    )
    RETURNING id, pk_post INTO v_post_id, v_post_pk;

    RETURN v_post_pk;
END;
$$ LANGUAGE plpgsql;

-- Create comment function
CREATE OR REPLACE FUNCTION fn_create_comment(
    p_author_id UUID,
    p_post_id UUID,
    p_content TEXT,
    p_parent_id UUID DEFAULT NULL
)
RETURNS UUID AS $$
DECLARE
    v_comment_pk UUID;
    v_author_id INTEGER;
    v_post_id INTEGER;
    v_parent_id INTEGER;
BEGIN
    -- Get internal IDs
    SELECT id INTO v_author_id FROM tb_user WHERE pk_user = p_author_id;
    SELECT id INTO v_post_id FROM tb_post WHERE pk_post = p_post_id;
    SELECT id INTO v_parent_id FROM tb_comment WHERE pk_comment = p_parent_id;

    IF v_author_id IS NULL OR v_post_id IS NULL THEN
        RAISE EXCEPTION 'Author or post not found';
    END IF;

    -- Insert comment
    INSERT INTO tb_comment (fk_author, fk_post, fk_parent, content)
    VALUES (v_author_id, v_post_id, v_parent_id, p_content)
    RETURNING pk_comment INTO v_comment_pk;

    RETURN v_comment_pk;
END;
$$ LANGUAGE plpgsql;
```

**Python Mutation Handlers**:

```python
from fraiseql import mutation, input
from typing import List, Optional

@input
class CreatePostInput:
    title: str
    content: str
    excerpt: Optional[str] = None
    tags: Optional[List[str]] = None
    is_published: bool = False

@input
class CreateCommentInput:
    post_id: UUID
    content: str
    parent_id: Optional[UUID] = None

@mutation
def create_post(input: CreatePostInput) -> Post:
    """Create new blog post."""
    pass  # Implementation handled by framework

@mutation
def create_comment(input: CreateCommentInput) -> Comment:
    """Add comment to post."""
    pass  # Implementation handled by framework
```

## Application Setup

```python
import os
from fraiseql import FraiseQL
from psycopg_pool import AsyncConnectionPool

# Initialize app
app = FraiseQL(
    database_url=os.getenv("DATABASE_URL", "postgresql://localhost/blog"),
    types=[User, Post, Comment],
    enable_playground=True
)

# Connection pool
pool = AsyncConnectionPool(
    conninfo=app.config.database_url,
    min_size=5,
    max_size=20
)

# Context setup
@app.context
async def get_context(request):
    async with pool.connection() as conn:
        repo = PsycopgRepository(pool=pool)
        return {
            "repo": repo,
            "tenant_id": request.headers.get("X-Tenant-ID"),
            "user_id": request.headers.get("X-User-ID"),  # From auth middleware
        }

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
```

## Testing

### GraphQL Queries

```graphql
# Get post with nested data (1 query!)
query GetPost($id: UUID!) {
  getPost(id: $id) {
    id
    title
    content
    author {
      id
      name
      avatarUrl
    }
    comments {
      id
      content
      author {
        name
      }
      replies {
        id
        content
        author {
          name
        }
      }
    }
  }
}

# List published posts
query GetPosts {
  getPosts(isPublished: true, limit: 10) {
    id
    title
    excerpt
    publishedAt
    author {
      name
    }
  }
}
```

### GraphQL Mutations

```graphql
mutation CreatePost($input: CreatePostInput!) {
  createPost(input: $input) {
    id
    title
    slug
    author {
      name
    }
  }
}

mutation AddComment($input: CreateCommentInput!) {
  createComment(input: $input) {
    id
    content
    createdAt
    author {
      name
    }
  }
}
```

## Performance Patterns

### 1. Materialized Views for Analytics

```sql
CREATE MATERIALIZED VIEW mv_popular_posts AS
SELECT
    p.pk_post,
    p.title,
    COUNT(DISTINCT c.id) as comment_count,
    array_agg(DISTINCT u.name) as commenters
FROM tb_post p
LEFT JOIN tb_comment c ON c.fk_post = p.id
LEFT JOIN tb_user u ON u.id = c.fk_author
WHERE p.is_published = true
GROUP BY p.pk_post, p.title
HAVING COUNT(DISTINCT c.id) > 5;

-- Refresh periodically
REFRESH MATERIALIZED VIEW CONCURRENTLY mv_popular_posts;
```

### 2. Partial Indexes for Common Queries

```sql
-- Index only published posts
CREATE INDEX idx_post_published_recent
ON tb_post (created_at DESC)
WHERE is_published = true;

-- Index only top-level comments
CREATE INDEX idx_comment_toplevel
ON tb_comment (fk_post, created_at)
WHERE fk_parent IS NULL;
```

## Production Checklist

- [ ] Add authentication middleware
- [ ] Implement rate limiting
- [ ] Set up query complexity limits
- [ ] Enable APQ caching
- [ ] Configure connection pooling
- [ ] Add monitoring (Prometheus/Sentry)
- [ ] Set up database backups
- [ ] Create migration strategy
- [ ] Write integration tests
- [ ] Deploy with Docker

## Key Patterns Demonstrated

1. **N+1 Prevention**: JSONB composition in views
2. **CQRS**: Separate read views from write tables
3. **Type Safety**: Full type checking end-to-end
4. **Performance**: Single-query nested data fetching
5. **Business Logic**: PostgreSQL functions for mutations

## Next Steps

- [Database Patterns](../advanced/database-patterns.md) - tv_ pattern and production patterns
- [Performance](../performance/index.md) - Rust transformation, APQ, TurboRouter
- [Multi-Tenancy](../advanced/multi-tenancy.md) - Tenant isolation patterns

## See Also

- [Quickstart](../quickstart.md) - 5-minute intro
- [Database API](../core/database-api.md) - Repository methods
- [Production Deployment](./production-deployment.md) - Deploy to production
