# Building a Blog API with FraiseQL

This tutorial walks through building a complete blog API using FraiseQL's CQRS architecture. We'll create a production-ready API with posts, comments, and user management.

## Overview

We'll build:
- User management with profiles
- Blog posts with tagging and publishing
- Threaded comments system
- Optimized views to eliminate N+1 queries
- Type-safe GraphQL API with modern Python

## Prerequisites

- PostgreSQL 14+
- Python 3.10+
- Basic understanding of GraphQL
- Familiarity with CQRS concepts (see [Architecture](../core-concepts/architecture.md))

## Project Structure

```
blog_api/
├── db/
│   ├── migrations/
│   │   ├── 001_initial_schema.sql    # Tables
│   │   ├── 002_functions.sql         # Mutations
│   │   └── 003_views.sql             # Query views
│   └── views/
│       └── composed_views.sql        # Optimized views
├── models.py                          # GraphQL types
├── queries.py                         # Query resolvers
├── mutations.py                       # Mutation resolvers
├── dataloaders.py                     # N+1 prevention
├── db.py                             # Repository pattern
└── app.py                            # FastAPI application
```

## Step 1: Database Schema

FraiseQL follows CQRS, separating writes (tables) from reads (views).

### Tables (Write Side)

```sql
-- Users table
CREATE TABLE tb_users (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    bio TEXT,
    avatar_url VARCHAR(500),
    is_active BOOLEAN DEFAULT true,
    roles TEXT[] DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Posts table
CREATE TABLE tb_posts (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    author_id UUID NOT NULL REFERENCES tb_users(id),
    title VARCHAR(500) NOT NULL,
    slug VARCHAR(500) UNIQUE NOT NULL,
    content TEXT NOT NULL,
    excerpt TEXT,
    tags TEXT[] DEFAULT '{}',
    is_published BOOLEAN DEFAULT false,
    published_at TIMESTAMPTZ,
    view_count INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Comments table (with threading support)
CREATE TABLE tb_comments (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    post_id UUID NOT NULL REFERENCES tb_posts(id) ON DELETE CASCADE,
    author_id UUID NOT NULL REFERENCES tb_users(id),
    parent_id UUID REFERENCES tb_comments(id),
    content TEXT NOT NULL,
    is_edited BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes for performance
CREATE INDEX idx_posts_author ON tb_posts(author_id);
CREATE INDEX idx_posts_published ON tb_posts(is_published, published_at DESC);
CREATE INDEX idx_comments_post ON tb_comments(post_id);
CREATE INDEX idx_comments_parent ON tb_comments(parent_id);
```

### Views (Read Side)

FraiseQL requires views with JSONB `data` columns containing camelCase fields:

```sql
-- Basic posts view
CREATE OR REPLACE VIEW v_post AS
SELECT
    p.id,
    jsonb_build_object(
        '__typename', 'Post',
        'id', p.id,
        'authorId', p.author_id,
        'title', p.title,
        'slug', p.slug,
        'content', p.content,
        'excerpt', p.excerpt,
        'tags', p.tags,
        'isPublished', p.is_published,
        'publishedAt', p.published_at,
        'viewCount', p.view_count,
        'createdAt', p.created_at,
        'updatedAt', p.updated_at
    ) AS data
FROM tb_posts p;
```

## Step 2: Composed Views (N+1 Prevention)

The key to FraiseQL's performance is composed views that pre-aggregate related data:

```sql
-- Posts with author and comments - eliminates N+1 queries
CREATE OR REPLACE VIEW v_post_full AS
SELECT
    p.id,
    jsonb_build_object(
        '__typename', 'Post',
        'id', p.id,
        'title', p.title,
        'slug', p.slug,
        'content', p.content,
        'tags', p.tags,
        'publishedAt', p.published_at,
        'viewCount', p.view_count,
        -- Author embedded directly
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
        -- Comments with their authors
        'comments', COALESCE(
            (SELECT jsonb_agg(
                jsonb_build_object(
                    '__typename', 'Comment',
                    'id', c.id,
                    'content', c.content,
                    'createdAt', c.created_at,
                    'author', (
                        SELECT jsonb_build_object(
                            '__typename', 'User',
                            'id', cu.id,
                            'name', cu.name
                        )
                        FROM tb_users cu
                        WHERE cu.id = c.author_id
                    ),
                    -- Nested replies
                    'replies', COALESCE(
                        (SELECT jsonb_agg(
                            jsonb_build_object(
                                '__typename', 'Comment',
                                'id', r.id,
                                'content', r.content,
                                'author', (
                                    SELECT jsonb_build_object(
                                        'name', ru.name
                                    )
                                    FROM tb_users ru
                                    WHERE ru.id = r.author_id
                                )
                            )
                        )
                        FROM tb_comments r
                        WHERE r.parent_id = c.id),
                        '[]'::jsonb
                    )
                )
            )
            FROM tb_comments c
            WHERE c.post_id = p.id AND c.parent_id IS NULL),
            '[]'::jsonb
        )
    ) AS data
FROM tb_posts p;
```

This single view fetches posts with authors, comments, comment authors, and replies in **one query**!

## Step 3: GraphQL Types

Define types using modern Python 3.10+ syntax:

```python
from datetime import datetime
from uuid import UUID
import fraiseql
from fraiseql import fraise_field

@fraiseql.type
class User:
    """User type for blog application."""
    id: UUID
    email: str = fraise_field(description="Email address")
    name: str = fraise_field(description="Display name")
    bio: str | None = fraise_field(description="User biography")
    avatar_url: str | None = fraise_field(description="Profile picture URL")
    created_at: datetime
    updated_at: datetime
    is_active: bool = fraise_field(default=True)
    roles: list[str] = fraise_field(default_factory=list)

@fraiseql.type
class Post:
    """Blog post type."""
    id: UUID
    title: str = fraise_field(description="Post title")
    slug: str = fraise_field(description="URL-friendly identifier")
    content: str = fraise_field(description="Post content in Markdown")
    excerpt: str | None = fraise_field(description="Short description")
    author_id: UUID
    published_at: datetime | None = None
    created_at: datetime
    updated_at: datetime
    tags: list[str] = fraise_field(default_factory=list)
    is_published: bool = fraise_field(default=False)
    view_count: int = fraise_field(default=0)

@fraiseql.type
class Comment:
    """Comment on a blog post."""
    id: UUID
    post_id: UUID
    author_id: UUID
    content: str = fraise_field(description="Comment text")
    created_at: datetime
    updated_at: datetime
    is_approved: bool = fraise_field(default=True)
    parent_comment_id: UUID | None = None  # For threading
```

## Step 4: Query Implementation

Queries use the repository pattern to fetch from views:

```python
from typing import Optional
from uuid import UUID
import fraiseql
from fraiseql.auth import requires_auth

@fraiseql.query
async def get_post(info, id: UUID) -> Post | None:
    """Get a post by ID."""
    db: BlogRepository = info.context["db"]

    post_data = await db.get_post_by_id(id)
    if not post_data:
        return None

    # Increment view count asynchronously
    await db.increment_view_count(id)

    return Post.from_dict(post_data)

@fraiseql.query
async def get_posts(
    info,
    filters: PostFilters | None = None,
    order_by: PostOrderBy | None = None,
    limit: int = 20,
    offset: int = 0,
) -> list[Post]:
    """Get posts with filtering and pagination."""
    db: BlogRepository = info.context["db"]

    # Convert filters to WHERE clause
    filter_dict = {}
    if filters:
        if filters.is_published is not None:
            filter_dict["is_published"] = filters.is_published
        if filters.author_id:
            filter_dict["author_id"] = filters.author_id
        if filters.tags_contain:
            filter_dict["tags"] = filters.tags_contain

    # Get posts from view
    posts_data = await db.get_posts(
        filters=filter_dict,
        order_by=order_by.field if order_by else "created_at DESC",
        limit=limit,
        offset=offset
    )

    return [Post.from_dict(data) for data in posts_data]

@fraiseql.query
@requires_auth
async def me(info) -> User | None:
    """Get the current authenticated user."""
    db: BlogRepository = info.context["db"]
    user_context = info.context["user"]
    user_data = await db.get_user_by_id(UUID(user_context.user_id))
    return User.from_dict(user_data) if user_data else None
```

## Step 5: Mutations via PostgreSQL Functions

FraiseQL mutations use PostgreSQL functions (prefixed with `fn_`):

```sql
-- Create post function
CREATE OR REPLACE FUNCTION fn_create_post(input_data JSON)
RETURNS JSON AS $$
DECLARE
    new_post_id UUID;
    generated_slug VARCHAR(500);
BEGIN
    -- Validate required fields
    IF input_data->>'author_id' IS NULL
    OR input_data->>'title' IS NULL
    OR input_data->>'content' IS NULL THEN
        RETURN json_build_object(
            'success', false,
            'error', 'Required fields missing'
        );
    END IF;

    -- Generate unique slug
    generated_slug := LOWER(
        REGEXP_REPLACE(input_data->>'title', '[^a-zA-Z0-9]+', '-', 'g')
    );

    -- Ensure uniqueness
    WHILE EXISTS (SELECT 1 FROM tb_posts WHERE slug = generated_slug) LOOP
        generated_slug := generated_slug || '-' ||
                         EXTRACT(EPOCH FROM NOW())::INTEGER;
    END LOOP;

    -- Insert post
    INSERT INTO tb_posts (
        author_id, title, slug, content, excerpt, tags,
        is_published, published_at
    )
    VALUES (
        (input_data->>'author_id')::UUID,
        input_data->>'title',
        generated_slug,
        input_data->>'content',
        input_data->>'excerpt',
        COALESCE(
            ARRAY(SELECT json_array_elements_text(input_data->'tags')),
            ARRAY[]::TEXT[]
        ),
        COALESCE((input_data->>'is_published')::BOOLEAN, false),
        CASE
            WHEN COALESCE((input_data->>'is_published')::BOOLEAN, false)
            THEN NOW()
            ELSE NULL
        END
    )
    RETURNING id INTO new_post_id;

    RETURN json_build_object(
        'success', true,
        'post_id', new_post_id,
        'slug', generated_slug
    );

EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;
```

Python mutation handler:

```python
@fraiseql.mutation
async def create_post(
    info,
    input: CreatePostInput
) -> CreatePostSuccess | CreatePostError:
    """Create a new blog post."""
    db: BlogRepository = info.context["db"]
    user = info.context.get("user")

    if not user:
        return CreatePostError(
            message="Authentication required",
            code="UNAUTHENTICATED"
        )

    try:
        result = await db.create_post({
            "author_id": user.user_id,
            "title": input.title,
            "content": input.content,
            "excerpt": input.excerpt,
            "tags": input.tags or [],
            "is_published": input.is_published
        })

        if result["success"]:
            post_data = await db.get_post_by_id(result["post_id"])
            return CreatePostSuccess(
                post=Post.from_dict(post_data),
                message="Post created successfully"
            )
        else:
            return CreatePostError(
                message=result["error"],
                code="CREATE_FAILED"
            )
    except Exception as e:
        return CreatePostError(
            message=str(e),
            code="INTERNAL_ERROR"
        )
```

## Step 6: FastAPI Application

Wire everything together:

```python
import os
from fraiseql.fastapi import create_fraiseql_app
from psycopg_pool import AsyncConnectionPool

# Import to register decorators
import queries
from models import Comment, Post, User
from mutations import (
    create_comment,
    create_post,
    create_user,
    delete_post,
    update_post,
)
from db import BlogRepository

# Create the FraiseQL app
app = create_fraiseql_app(
    database_url=os.getenv("DATABASE_URL", "postgresql://localhost/blog_db"),
    types=[User, Post, Comment],
    mutations=[
        create_user,
        create_post,
        update_post,
        create_comment,
        delete_post,
    ],
    title="Blog API",
    version="1.0.0",
    description="A blog API built with FraiseQL",
    production=os.getenv("ENV") == "production",
)

# Create connection pool
pool = AsyncConnectionPool(
    os.getenv("DATABASE_URL", "postgresql://localhost/blog_db"),
    min_size=5,
    max_size=20,
)

# Dependency injection for repository
async def get_blog_db():
    """Get blog repository for the request."""
    async with pool.connection() as conn:
        yield BlogRepository(conn)

app.dependency_overrides["db"] = get_blog_db

if __name__ == "__main__":
    import uvicorn
    uvicorn.run("app:app", host="0.0.0.0", port=8000, reload=True)
```

## Step 7: Testing the API

### GraphQL Queries

Get posts with authors and comments (no N+1!):

```graphql
query GetPosts {
  getPosts(limit: 10, filters: { isPublished: true }) {
    id
    title
    slug
    excerpt
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
```

### GraphQL Mutations

Create a post:

```graphql
mutation CreatePost($input: CreatePostInput!) {
  createPost(input: $input) {
    __typename
    ... on CreatePostSuccess {
      post {
        id
        title
        slug
      }
      message
    }
    ... on CreatePostError {
      message
      code
    }
  }
}
```

## Performance Optimization

### 1. Materialized Views for Hot Paths

```sql
-- Popular posts with engagement metrics
CREATE MATERIALIZED VIEW v_popular_post AS
SELECT
    p.id,
    jsonb_build_object(
        '__typename', 'PopularPost',
        'id', p.id,
        'title', p.title,
        'author', jsonb_build_object(
            'name', u.name
        ),
        'metrics', jsonb_build_object(
            'viewCount', p.view_count,
            'commentCount', COUNT(DISTINCT c.id),
            'engagementScore', (
                p.view_count +
                (COUNT(DISTINCT c.id) * 10)
            )
        )
    ) AS data
FROM tb_posts p
JOIN tb_users u ON u.id = p.author_id
LEFT JOIN tb_comments c ON c.post_id = p.id
WHERE p.is_published = true
GROUP BY p.id, p.title, p.view_count, u.name
HAVING p.view_count > 100;

-- Refresh periodically
CREATE OR REPLACE FUNCTION refresh_blog_statistics()
RETURNS void AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY v_popular_post;
END;
$$ LANGUAGE plpgsql;
```

### 2. DataLoader for Remaining N+1 Cases

```python
from fraiseql import dataloader_field

@fraiseql.type
class Post:
    # ... other fields ...

    @dataloader_field
    async def related_posts(self, info) -> list["Post"]:
        """Get related posts by tags."""
        loader = info.context["related_posts_loader"]
        return await loader.load(self.id)
```

### 3. Query Analysis

Enable query analysis in development:

```python
app = create_fraiseql_app(
    # ...
    analyze_queries=True,  # Logs slow queries
    query_depth_limit=5,    # Prevent deep nesting
    query_complexity_limit=1000,  # Limit complexity
)
```

## Best Practices

1. **View Composition**: Create specialized views for common query patterns
2. **Filter Columns**: Add filter columns to views for WHERE clauses
3. **Batch Operations**: Use DataLoaders for any remaining N+1 patterns
4. **Caching**: Use materialized views for expensive aggregations
5. **Monitoring**: Track slow queries and optimize views accordingly

## Testing

```python
import pytest
from httpx import AsyncClient

@pytest.mark.asyncio
async def test_create_and_get_post():
    async with AsyncClient(app=app, base_url="http://test") as client:
        # Create post
        mutation = """
            mutation CreatePost($input: CreatePostInput!) {
                createPost(input: $input) {
                    ... on CreatePostSuccess {
                        post { id, slug }
                    }
                }
            }
        """

        response = await client.post(
            "/graphql",
            json={
                "query": mutation,
                "variables": {
                    "input": {
                        "title": "Test Post",
                        "content": "Content here",
                        "isPublished": true
                    }
                }
            }
        )

        assert response.status_code == 200
        data = response.json()
        post_id = data["data"]["createPost"]["post"]["id"]

        # Get post
        query = """
            query GetPost($id: UUID!) {
                getPost(id: $id) {
                    title
                    content
                }
            }
        """

        response = await client.post(
            "/graphql",
            json={
                "query": query,
                "variables": {"id": post_id}
            }
        )

        assert response.status_code == 200
        data = response.json()
        assert data["data"]["getPost"]["title"] == "Test Post"
```

## Deployment

### Production Configuration

```python
# Production settings
app = create_fraiseql_app(
    database_url=os.getenv("DATABASE_URL"),
    production=True,  # Disables playground, enables security
    cors_origins=["https://yourdomain.com"],
    max_query_depth=7,
    query_complexity_limit=5000,
    rate_limit="100/minute",
)
```

### Database Migrations

Use a migration tool like Alembic or migrate manually:

```bash
# Apply migrations
psql $DATABASE_URL -f db/migrations/001_initial_schema.sql
psql $DATABASE_URL -f db/migrations/002_functions.sql
psql $DATABASE_URL -f db/migrations/003_views.sql
psql $DATABASE_URL -f db/views/composed_views.sql
```

## Summary

This blog API demonstrates FraiseQL's power:

- **CQRS Architecture**: Clean separation of reads and writes
- **Performance**: Composed views eliminate N+1 queries
- **Type Safety**: Full type checking from database to GraphQL
- **Production Ready**: Authentication, error handling, and monitoring
- **PostgreSQL Native**: Leverages database features for performance

The complete example is available in `/home/lionel/code/fraiseql/examples/blog_api/`.

## Next Steps

- Add full-text search using PostgreSQL's `tsvector`
- Implement real-time subscriptions for comments
- Add image uploads with S3 integration
- Implement content moderation workflow
- Add analytics and metrics collection

See the [Mutations Guide](../mutations/index.md) for more complex mutation patterns.
