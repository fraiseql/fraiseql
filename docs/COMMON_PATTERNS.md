# Common Patterns in FraiseQL

This guide covers real-world patterns and best practices for building production applications with FraiseQL.

## Table of Contents

1. [Multi-Tenant Applications](#multi-tenant-applications)
2. [Authentication & Authorization](#authentication--authorization)
3. [Pagination](#pagination)
4. [Filtering & Search](#filtering--search)
5. [Nested Objects & Relations](#nested-objects--relations)
6. [Batch Operations](#batch-operations)
7. [Error Handling](#error-handling)
8. [Caching](#caching)
9. [File Uploads](#file-uploads)
10. [Real-time Subscriptions](#real-time-subscriptions)

## Multi-Tenant Applications

### Basic Multi-Tenant Setup

```python
# context.py
async def get_context(request: Request) -> dict[str, Any]:
    """Extract tenant from request header."""
    pool = request.app.state.db_pool
    tenant_id = request.headers.get("x-tenant-id")
    
    if not tenant_id:
        raise HTTPException(400, "Missing tenant ID")
    
    # Pass tenant to repository context
    repo = FraiseQLRepository(pool, context={
        "tenant_id": tenant_id,
        "mode": "development"
    })
    
    return {
        "db": repo,
        "tenant_id": tenant_id,
        "request": request
    }

# Create app with custom context
app = create_fraiseql_app(
    database_url=DATABASE_URL,
    types=[User, Project],
    context_getter=get_context
)
```

### Database Views with Tenant Isolation

```sql
-- Always include tenant_id in views
CREATE VIEW tenant_users AS
SELECT 
    id,
    tenant_id,           -- Required for filtering
    email,
    status,
    jsonb_build_object(
        'id', id,
        'email', email,
        'name', name,
        'role', role,
        'created_at', created_at
    ) as data
FROM users;

-- Row-level security
ALTER TABLE users ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON users
    FOR ALL
    USING (tenant_id = current_setting('app.tenant_id')::uuid);
```

### Tenant-Aware Queries

```python
@fraiseql.query
async def users(info, role: str | None = None) -> list[User]:
    """Get users for current tenant."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]
    
    filters = {"tenant_id": tenant_id}
    if role:
        filters["role"] = role
    
    return await db.find("tenant_users", **filters)

@fraiseql.query
async def user(info, id: UUID) -> User | None:
    """Get single user ensuring tenant isolation."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]
    
    # Always include tenant_id in single queries too!
    return await db.find_one("tenant_users", 
        id=id, 
        tenant_id=tenant_id
    )
```

## Authentication & Authorization

### JWT Authentication Setup

```python
# auth.py
from fraiseql.auth import Auth0Config, requires_auth, requires_role

# Configure Auth0
auth_config = Auth0Config(
    domain="your-domain.auth0.com",
    api_identifier="https://api.example.com",
    algorithms=["RS256"]
)

# Create app with auth
app = create_fraiseql_app(
    database_url=DATABASE_URL,
    types=[User, Post],
    auth=auth_config
)
```

### Protected Queries

```python
@fraiseql.query
async def public_posts(info) -> list[Post]:
    """Anyone can see public posts."""
    db = info.context["db"]
    return await db.find("post_view", is_public=True)

@fraiseql.query
@requires_auth
async def my_posts(info) -> list[Post]:
    """Only authenticated users can see their posts."""
    db = info.context["db"]
    user = info.context["user"]  # Guaranteed to exist
    
    return await db.find("post_view", author_id=user.user_id)

@fraiseql.query
@requires_role("admin")
async def all_posts(info) -> list[Post]:
    """Only admins can see all posts."""
    db = info.context["db"]
    return await db.find("post_view")

@fraiseql.query
@requires_permission("posts:moderate")
async def flagged_posts(info) -> list[Post]:
    """Only moderators can see flagged posts."""
    db = info.context["db"]
    return await db.find("post_view", is_flagged=True)
```

### Custom Permission Checks

```python
@fraiseql.query
async def project(info, id: UUID) -> Project | None:
    """Get project with custom permission check."""
    db = info.context["db"]
    user = info.context.get("user")
    
    project = await db.find_one("project_view", id=id)
    if not project:
        return None
    
    # Custom permission logic
    if project.is_private and (not user or user.user_id != project.owner_id):
        raise GraphQLError("Permission denied")
    
    return project
```

## Pagination

### Cursor-Based Pagination

```python
@fraise_type
class PageInfo:
    has_next_page: bool
    has_previous_page: bool
    start_cursor: str | None = None
    end_cursor: str | None = None

@fraise_type
class PostEdge:
    node: Post
    cursor: str

@fraise_type
class PostConnection:
    edges: list[PostEdge]
    page_info: PageInfo
    total_count: int

@fraiseql.query
async def posts_paginated(
    info,
    first: int = 20,
    after: str | None = None,
    order_by: str = "created_at"
) -> PostConnection:
    """Paginated posts with cursor."""
    db = info.context["db"]
    
    # Decode cursor to get offset
    offset = 0
    if after:
        import base64
        offset = int(base64.b64decode(after).decode())
    
    # Fetch one extra to check for next page
    posts = await db.find("post_view", 
        limit=first + 1,
        offset=offset,
        order_by=order_by
    )
    
    has_next = len(posts) > first
    posts = posts[:first]  # Remove extra
    
    # Build edges with cursors
    edges = []
    for i, post in enumerate(posts):
        cursor = base64.b64encode(
            str(offset + i).encode()
        ).decode()
        edges.append(PostEdge(node=post, cursor=cursor))
    
    return PostConnection(
        edges=edges,
        page_info=PageInfo(
            has_next_page=has_next,
            has_previous_page=offset > 0,
            start_cursor=edges[0].cursor if edges else None,
            end_cursor=edges[-1].cursor if edges else None
        ),
        total_count=await db.count("post_view")  # Implement count
    )
```

### Offset-Based Pagination

```python
@fraise_type
class PaginatedPosts:
    items: list[Post]
    total: int
    page: int
    per_page: int
    pages: int

@fraiseql.query
async def posts_offset(
    info,
    page: int = 1,
    per_page: int = 20
) -> PaginatedPosts:
    """Simple offset pagination."""
    db = info.context["db"]
    
    offset = (page - 1) * per_page
    
    posts = await db.find("post_view",
        limit=per_page,
        offset=offset
    )
    
    total = await db.count("post_view")
    pages = (total + per_page - 1) // per_page
    
    return PaginatedPosts(
        items=posts,
        total=total,
        page=page,
        per_page=per_page,
        pages=pages
    )
```

## Filtering & Search

### Complex Filtering

```python
@fraise_input
class PostFilter:
    author_id: UUID | None = None
    status: str | None = None
    tags: list[str] | None = None
    created_after: datetime | None = None
    created_before: datetime | None = None
    search: str | None = None

@fraiseql.query
async def filtered_posts(
    info,
    filter: PostFilter | None = None,
    order_by: str = "created_at",
    order_desc: bool = True
) -> list[Post]:
    """Advanced post filtering."""
    db = info.context["db"]
    
    # Build WHERE conditions
    conditions = []
    params = {}
    
    if filter:
        if filter.author_id:
            conditions.append("author_id = %(author_id)s")
            params["author_id"] = filter.author_id
            
        if filter.status:
            conditions.append("status = %(status)s")
            params["status"] = filter.status
            
        if filter.tags:
            conditions.append("tags && %(tags)s")  # Array overlap
            params["tags"] = filter.tags
            
        if filter.created_after:
            conditions.append("created_at >= %(created_after)s")
            params["created_after"] = filter.created_after
            
        if filter.created_before:
            conditions.append("created_at <= %(created_before)s")
            params["created_before"] = filter.created_before
            
        if filter.search:
            conditions.append(
                "to_tsvector('english', title || ' ' || content) @@ "
                "plainto_tsquery('english', %(search)s)"
            )
            params["search"] = filter.search
    
    # Build query
    query = "SELECT * FROM post_view"
    if conditions:
        query += " WHERE " + " AND ".join(conditions)
    
    query += f" ORDER BY {order_by} {'DESC' if order_desc else 'ASC'}"
    
    # Execute with custom SQL
    from fraiseql.db import DatabaseQuery
    from psycopg.sql import SQL
    
    results = await db.run(DatabaseQuery(
        statement=SQL(query),
        params=params,
        fetch_result=True
    ))
    
    # In dev mode, instantiate manually
    if db.mode == "development":
        return [Post(**row["data"]) for row in results]
    return results
```

### Full-Text Search

```sql
-- Add search vector to view
CREATE VIEW post_search AS
SELECT 
    id,
    author_id,
    status,
    to_tsvector('english', title || ' ' || content || ' ' || 
        coalesce(array_to_string(tags, ' '), '')) as search_vector,
    ts_rank(
        to_tsvector('english', title || ' ' || content),
        plainto_tsquery('english', %(query)s)
    ) as rank,
    jsonb_build_object(
        'id', id,
        'title', title,
        'content', content,
        'author_id', author_id,
        'tags', tags,
        'created_at', created_at
    ) as data
FROM posts;

-- Index for performance
CREATE INDEX idx_posts_search ON posts 
USING gin(to_tsvector('english', title || ' ' || content));
```

```python
@fraiseql.query
async def search_posts(
    info,
    query: str,
    limit: int = 20
) -> list[Post]:
    """Full-text search posts."""
    db = info.context["db"]
    
    # Custom query for search
    from psycopg.sql import SQL
    
    sql = SQL("""
        SELECT * FROM post_search
        WHERE search_vector @@ plainto_tsquery('english', %(query)s)
        ORDER BY rank DESC
        LIMIT %(limit)s
    """)
    
    results = await db.run(DatabaseQuery(
        statement=sql,
        params={"query": query, "limit": limit},
        fetch_result=True
    ))
    
    return [Post(**row["data"]) for row in results]
```

## Nested Objects & Relations

### One-to-Many Relations

```sql
CREATE VIEW author_with_posts AS
SELECT 
    a.id,
    a.tenant_id,
    jsonb_build_object(
        'id', a.id,
        'name', a.name,
        'email', a.email,
        'posts', COALESCE(
            jsonb_agg(
                jsonb_build_object(
                    'id', p.id,
                    'title', p.title,
                    'published_at', p.published_at
                ) ORDER BY p.published_at DESC
            ) FILTER (WHERE p.id IS NOT NULL),
            '[]'::jsonb
        )
    ) as data
FROM authors a
LEFT JOIN posts p ON p.author_id = a.id AND p.status = 'published'
GROUP BY a.id;
```

```python
@fraise_type
class AuthorWithPosts:
    id: UUID
    name: str
    email: str
    posts: list[PostSummary]

@fraise_type
class PostSummary:
    id: UUID
    title: str
    published_at: datetime

@fraiseql.query
async def author_with_posts(info, id: UUID) -> AuthorWithPosts | None:
    db = info.context["db"]
    return await db.find_one("author_with_posts", id=id)
```

### Many-to-Many Relations

```sql
CREATE VIEW post_with_tags AS
SELECT 
    p.id,
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'content', p.content,
        'tags', COALESCE(
            jsonb_agg(
                jsonb_build_object(
                    'id', t.id,
                    'name', t.name,
                    'slug', t.slug
                )
            ) FILTER (WHERE t.id IS NOT NULL),
            '[]'::jsonb
        )
    ) as data
FROM posts p
LEFT JOIN post_tags pt ON pt.post_id = p.id
LEFT JOIN tags t ON t.id = pt.tag_id
GROUP BY p.id;
```

## Batch Operations

### Batch Create

```python
@fraise_input
class CreatePostInput:
    title: str
    content: str
    tags: list[str] = []

@fraiseql.mutation
async def create_posts_batch(
    info,
    inputs: list[CreatePostInput]
) -> list[Post]:
    """Create multiple posts in one mutation."""
    db = info.context["db"]
    user = info.context["user"]
    
    created_posts = []
    
    # Use transaction for atomicity
    async with db._pool.connection() as conn:
        async with conn.transaction():
            for input_data in inputs:
                # Insert post
                result = await conn.fetchone("""
                    INSERT INTO posts (title, content, author_id, tags)
                    VALUES (%(title)s, %(content)s, %(author_id)s, %(tags)s)
                    RETURNING id
                """, {
                    "title": input_data.title,
                    "content": input_data.content,
                    "author_id": user.user_id,
                    "tags": input_data.tags
                })
                
                # Fetch complete post
                post = await db.find_one("post_view", id=result["id"])
                created_posts.append(post)
    
    return created_posts
```

### Batch Update

```python
@fraise_input
class BatchUpdateInput:
    ids: list[UUID]
    status: str | None = None
    tags: list[str] | None = None

@fraiseql.mutation
async def update_posts_batch(
    info,
    input: BatchUpdateInput
) -> list[Post]:
    """Update multiple posts at once."""
    db = info.context["db"]
    
    # Build update fields
    updates = []
    params = {"ids": input.ids}
    
    if input.status is not None:
        updates.append("status = %(status)s")
        params["status"] = input.status
        
    if input.tags is not None:
        updates.append("tags = %(tags)s")
        params["tags"] = input.tags
    
    if not updates:
        raise GraphQLError("No fields to update")
    
    # Execute batch update
    async with db._pool.connection() as conn:
        await conn.execute(f"""
            UPDATE posts 
            SET {', '.join(updates)}, updated_at = CURRENT_TIMESTAMP
            WHERE id = ANY(%(ids)s)
        """, params)
    
    # Return updated posts
    return await db.find("post_view", id=input.ids)
```

## Error Handling

### Structured Error Responses

```python
@fraise_type
class Error:
    message: str
    code: str
    field: str | None = None

@fraise_type
class PostResult:
    post: Post | None = None
    errors: list[Error] = []

@fraiseql.mutation
async def create_post_safe(
    info,
    input: CreatePostInput
) -> PostResult:
    """Create post with structured error handling."""
    db = info.context["db"]
    errors = []
    
    # Validation
    if len(input.title) < 3:
        errors.append(Error(
            message="Title too short",
            code="TITLE_TOO_SHORT",
            field="title"
        ))
    
    if len(input.content) < 10:
        errors.append(Error(
            message="Content too short",
            code="CONTENT_TOO_SHORT",
            field="content"
        ))
    
    if errors:
        return PostResult(errors=errors)
    
    try:
        # Create post
        post = await create_post_internal(db, input)
        return PostResult(post=post)
        
    except UniqueViolationError:
        return PostResult(errors=[Error(
            message="A post with this title already exists",
            code="DUPLICATE_TITLE",
            field="title"
        )])
    except Exception as e:
        logger.error(f"Failed to create post: {e}")
        return PostResult(errors=[Error(
            message="Failed to create post",
            code="INTERNAL_ERROR"
        )])
```

### Global Error Handler

```python
from fastapi import Request
from fastapi.responses import JSONResponse

@app.exception_handler(GraphQLError)
async def graphql_error_handler(request: Request, exc: GraphQLError):
    return JSONResponse(
        status_code=200,  # GraphQL returns 200 even for errors
        content={
            "errors": [{
                "message": str(exc),
                "extensions": getattr(exc, "extensions", {})
            }]
        }
    )

@app.exception_handler(Exception)
async def general_error_handler(request: Request, exc: Exception):
    logger.error(f"Unhandled error: {exc}", exc_info=True)
    return JSONResponse(
        status_code=200,
        content={
            "errors": [{
                "message": "Internal server error",
                "extensions": {"code": "INTERNAL_ERROR"}
            }]
        }
    )
```

## Caching

### Redis Caching

```python
import json
import redis.asyncio as redis
from datetime import timedelta

# Setup Redis
redis_client = redis.from_url("redis://localhost")

@fraiseql.query
async def cached_user(info, id: UUID) -> User | None:
    """Get user with Redis caching."""
    cache_key = f"user:{id}"
    
    # Try cache first
    cached = await redis_client.get(cache_key)
    if cached:
        data = json.loads(cached)
        return User(**data)
    
    # Fetch from database
    db = info.context["db"]
    user = await db.find_one("user_view", id=id)
    
    if user:
        # Cache for 1 hour
        await redis_client.setex(
            cache_key,
            timedelta(hours=1),
            json.dumps(user.dict() if hasattr(user, 'dict') else user)
        )
    
    return user

@fraiseql.mutation
async def update_user(info, id: UUID, input: UpdateUserInput) -> User:
    """Update user and invalidate cache."""
    db = info.context["db"]
    
    # Update in database
    user = await update_user_internal(db, id, input)
    
    # Invalidate cache
    await redis_client.delete(f"user:{id}")
    
    return user
```

### Query Result Caching

```python
from functools import wraps
import hashlib

def cache_query(ttl: int = 300):
    """Decorator to cache query results."""
    def decorator(func):
        @wraps(func)
        async def wrapper(info, **kwargs):
            # Generate cache key from function name and args
            key_data = f"{func.__name__}:{json.dumps(kwargs, sort_keys=True)}"
            cache_key = hashlib.md5(key_data.encode()).hexdigest()
            
            # Try cache
            cached = await redis_client.get(cache_key)
            if cached:
                return json.loads(cached)
            
            # Execute query
            result = await func(info, **kwargs)
            
            # Cache result
            await redis_client.setex(
                cache_key,
                ttl,
                json.dumps(result, default=str)
            )
            
            return result
        return wrapper
    return decorator

@fraiseql.query
@cache_query(ttl=600)  # Cache for 10 minutes
async def expensive_report(info, month: int, year: int) -> dict:
    """Generate expensive report with caching."""
    # Complex calculation...
    pass
```

## File Uploads

### Handling File Uploads

```python
from fastapi import UploadFile, File

# Add file upload endpoint to your FastAPI app
@app.post("/upload")
async def upload_file(
    file: UploadFile = File(...),
    user: UserContext = Depends(get_current_user)
):
    """Handle file upload outside GraphQL."""
    # Save file
    file_id = uuid4()
    file_path = f"uploads/{user.user_id}/{file_id}_{file.filename}"
    
    async with aiofiles.open(file_path, 'wb') as f:
        content = await file.read()
        await f.write(content)
    
    # Store metadata in database
    async with db_pool.connection() as conn:
        await conn.execute("""
            INSERT INTO files (id, user_id, filename, path, size, content_type)
            VALUES (%(id)s, %(user_id)s, %(filename)s, %(path)s, %(size)s, %(content_type)s)
        """, {
            "id": file_id,
            "user_id": user.user_id,
            "filename": file.filename,
            "path": file_path,
            "size": len(content),
            "content_type": file.content_type
        })
    
    return {"file_id": str(file_id)}

# Reference uploaded files in GraphQL
@fraise_type
class FileReference:
    id: UUID
    filename: str
    size: int
    content_type: str
    url: str

@fraiseql.mutation
async def create_post_with_image(
    info,
    title: str,
    content: str,
    image_id: UUID | None = None
) -> Post:
    """Create post with uploaded image."""
    db = info.context["db"]
    
    # Verify file exists if provided
    if image_id:
        file = await db.find_one("file_view", id=image_id)
        if not file:
            raise GraphQLError("Invalid file ID")
    
    # Create post with image reference
    # ...
```

## Real-time Subscriptions

### WebSocket Subscriptions

```python
from fraiseql import subscription
import asyncio

# In-memory pubsub (use Redis in production)
subscribers: dict[str, list[asyncio.Queue]] = {}

@subscription
async def post_created(info, author_id: UUID | None = None):
    """Subscribe to new posts."""
    queue = asyncio.Queue()
    channel = f"posts:{author_id}" if author_id else "posts:all"
    
    # Register subscriber
    if channel not in subscribers:
        subscribers[channel] = []
    subscribers[channel].append(queue)
    
    try:
        while True:
            post = await queue.get()
            yield post
    finally:
        # Cleanup on disconnect
        subscribers[channel].remove(queue)

@fraiseql.mutation
async def create_post_with_notification(
    info,
    input: CreatePostInput
) -> Post:
    """Create post and notify subscribers."""
    db = info.context["db"]
    user = info.context["user"]
    
    # Create post
    post = await create_post_internal(db, input, user.user_id)
    
    # Notify subscribers
    channels = [f"posts:all", f"posts:{user.user_id}"]
    for channel in channels:
        if channel in subscribers:
            for queue in subscribers[channel]:
                await queue.put(post)
    
    return post
```

## Summary

These patterns cover the most common real-world scenarios in FraiseQL applications:

1. **Multi-tenancy**: Header-based tenant isolation
2. **Authentication**: JWT with role/permission checks
3. **Pagination**: Both cursor and offset-based
4. **Filtering**: Complex queries with full-text search
5. **Relations**: One-to-many and many-to-many
6. **Batch Operations**: Efficient bulk updates
7. **Error Handling**: Structured error responses
8. **Caching**: Redis integration
9. **File Uploads**: Hybrid REST/GraphQL approach
10. **Subscriptions**: Real-time updates

Remember to always:
- Use the JSONB data column pattern
- Include filtering columns in views
- Handle errors gracefully
- Test with both dev and production modes
- Consider performance implications