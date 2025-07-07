# ADR-002: GraphQL Type System Design

## Status
Accepted

## Context
FraiseQL needs to generate GraphQL schemas from Python dataclasses while maintaining type safety. All data resolution should happen at the PostgreSQL level through views and projection tables, avoiding custom Python resolvers.

## Decision
We will use a decorator-based approach with:
- `@fraise_type` for output types bound to SQL views/tables
- `@fraise_input` for input types
- `@query` and `@mutation` for operations
- All relationships and computed fields handled by PostgreSQL views
- No custom Python field resolvers - everything comes from SQL

## Consequences

### Positive
- **Performance**: All computation happens in PostgreSQL
- **Consistency**: Single source of truth in the database
- **Composability**: Views can be composed and reused
- **SQL optimization**: Database can optimize complex queries
- **Simplicity**: No N+1 problems, no custom resolver logic

### Negative
- **SQL complexity**: Complex relationships require complex views
- **Less flexibility**: Can't easily add computed fields in Python
- **Database coupling**: Application logic moves to database

### Mitigation
- Create reusable view patterns and templates
- Use PostgreSQL functions for complex computations
- Document view composition patterns
- Leverage PostgreSQL's powerful JSON functions

## Implementation

### Basic Types
```python
from dataclasses import dataclass
from datetime import datetime
from fraiseql import fraise_type, fraise_input

@fraise_type(sql_source="v_users")
@dataclass
class User:
    id: str
    email: str
    name: str
    created_at: datetime
    # All fields come from the view
    post_count: int
    recent_posts: list[dict]

@fraise_input
@dataclass
class CreateUserInput:
    email: str
    name: str
```

### SQL Views Handle Everything
```sql
-- User view with computed fields and relationships
CREATE VIEW v_users AS
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id,
        'email', u.email,
        'name', u.name,
        'createdAt', u.created_at,
        'postCount', COALESCE(stats.post_count, 0),
        'recentPosts', COALESCE(recent.posts, '[]'::jsonb)
    ) as data
FROM tb_users u
LEFT JOIN LATERAL (
    SELECT COUNT(*) as post_count
    FROM tb_posts
    WHERE user_id = u.id
) stats ON true
LEFT JOIN LATERAL (
    SELECT jsonb_agg(
        jsonb_build_object(
            'id', p.id,
            'title', p.title,
            'createdAt', p.created_at
        ) ORDER BY p.created_at DESC
    ) as posts
    FROM tb_posts p
    WHERE p.user_id = u.id
    AND p.created_at > NOW() - INTERVAL '30 days'
    LIMIT 5
) recent ON true;

-- Post view with author embedded
CREATE VIEW v_posts AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'content', p.content,
        'createdAt', p.created_at,
        'author', jsonb_build_object(
            'id', u.id,
            'name', u.name,
            'email', u.email
        ),
        'commentCount', COALESCE(c.count, 0)
    ) as data
FROM tb_posts p
JOIN tb_users u ON p.user_id = u.id
LEFT JOIN LATERAL (
    SELECT COUNT(*) as count
    FROM tb_comments
    WHERE post_id = p.id
) c ON true;
```

### Types Match View Structure
```python
@fraise_type(sql_source="v_posts")
@dataclass
class Post:
    id: str
    title: str
    content: str
    created_at: datetime
    author: User  # Comes from the view's nested JSON
    comment_count: int  # Computed in the view
```

### Queries and Mutations
```python
from fraiseql import query, mutation

@query
async def users(
    info: Info,
    where: UserWhere | None = None,
    order_by: list[UserOrderBy] | None = None,
    limit: int = 100,
    offset: int = 0
) -> list[User]:
    """Query users with filtering and pagination."""
    # Simple pass-through to repository
    return await info.context["repo"].list(
        User,
        where=where,
        order_by=order_by,
        limit=limit,
        offset=offset
    )

@mutation
async def create_user(
    info: Info,
    input: CreateUserInput
) -> User:
    """Create a new user via SQL function."""
    # SQL function handles all updates to projections
    return await info.context["repo"].create("user", input)
```

### Projection Tables for Complex Aggregations
```sql
-- Projection table updated by mutation functions
CREATE TABLE tv_user_dashboard (
    user_id UUID PRIMARY KEY,
    data JSONB NOT NULL,
    last_updated TIMESTAMP NOT NULL
);

-- Mutation function updates projections
CREATE FUNCTION fn_create_post(input JSONB) RETURNS JSONB AS $$
DECLARE
    post_id UUID;
    user_id UUID;
BEGIN
    -- Insert post
    INSERT INTO tb_posts (user_id, title, content)
    VALUES (
        (input->>'userId')::UUID,
        input->>'title',
        input->>'content'
    )
    RETURNING id, user_id INTO post_id, user_id;

    -- Update user dashboard projection
    INSERT INTO tv_user_dashboard (user_id, data, last_updated)
    SELECT
        user_id,
        jsonb_build_object(
            'totalPosts', COUNT(*),
            'latestPostId', post_id,
            'monthlyPostCount', COUNT(*) FILTER (
                WHERE created_at > NOW() - INTERVAL '30 days'
            )
        ),
        NOW()
    FROM tb_posts
    WHERE user_id = user_id
    GROUP BY user_id
    ON CONFLICT (user_id) DO UPDATE
    SET data = EXCLUDED.data,
        last_updated = NOW();

    -- Return the new post
    RETURN (SELECT data FROM v_posts WHERE id = post_id);
END;
$$ LANGUAGE plpgsql;
```
