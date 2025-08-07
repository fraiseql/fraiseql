---
← [Advanced Topics](index.md) | [Home](../index.md) | [Next: Event Sourcing](event-sourcing.md) →
---

# CQRS Implementation

> **In this section:** Implement Command Query Responsibility Segregation patterns with FraiseQL
> **Prerequisites:** Understanding of [FraiseQL architecture](../core-concepts/architecture.md)
> **Time to complete:** 20 minutes

FraiseQL naturally implements the CQRS (Command Query Responsibility Segregation) pattern by separating read models (views) from write models (tables and functions).

## CQRS Fundamentals

### Command Side (Writes)
Commands handle all data modifications through PostgreSQL functions:

```sql
-- Command: Create user
CREATE OR REPLACE FUNCTION fn_create_user(
    p_name TEXT,
    p_email TEXT
) RETURNS UUID AS $$
DECLARE
    user_id UUID;
BEGIN
    -- Business logic and validation
    IF EXISTS (SELECT 1 FROM tb_user WHERE email = p_email) THEN
        RAISE EXCEPTION 'Email already exists';
    END IF;

    -- Insert with business rules
    INSERT INTO tb_user (name, email, created_at)
    VALUES (p_name, p_email, NOW())
    RETURNING id INTO user_id;

    -- Audit logging
    INSERT INTO tb_audit_log (action, table_name, record_id)
    VALUES ('CREATE_USER', 'tb_user', user_id);

    RETURN user_id;
END;
$$ LANGUAGE plpgsql;
```

### Query Side (Reads)
Queries use optimized views with denormalized data:

```sql
-- Query: User with post counts
CREATE VIEW v_user_with_stats AS
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id,
        'name', u.name,
        'email', u.email,
        'post_count', COALESCE(p.post_count, 0),
        'created_at', u.created_at
    ) AS data
FROM tb_user u
LEFT JOIN (
    SELECT
        author_id,
        COUNT(*) as post_count
    FROM tb_post
    GROUP BY author_id
) p ON u.id = p.author_id;
```

## Python Implementation

### Commands (Mutations)
```python
@fraiseql.mutation
async def create_user(info, name: str, email: str) -> User:
    """Command: Create a new user"""
    repo = info.context["repo"]

    try:
        # Execute command function
        user_id = await repo.call_function(
            "fn_create_user",
            p_name=name,
            p_email=email
        )

        # Return updated read model
        result = await repo.find_one("v_user_with_stats", where={"id": user_id})
        return User(**result)

    except Exception as e:
        if "already exists" in str(e):
            raise GraphQLError("Email already in use", code="DUPLICATE_EMAIL")
        raise GraphQLError("Failed to create user", code="CREATE_FAILED")
```

### Queries (Read Models)
```python
@fraiseql.query
async def users(info, limit: int = 10) -> list[User]:
    """Query: Get users from optimized read model"""
    repo = info.context["repo"]
    return await repo.find("v_user_with_stats", limit=limit)

@fraiseql.query
async def user(info, id: ID) -> User | None:
    """Query: Get single user with all related data"""
    repo = info.context["repo"]
    result = await repo.find_one("v_user_with_stats", where={"id": id})
    return User(**result) if result else None
```

## Advanced CQRS Patterns

### Event-Driven Commands
```sql
-- Command with event publishing
CREATE OR REPLACE FUNCTION fn_publish_post(
    p_post_id UUID,
    p_user_id UUID
) RETURNS BOOLEAN AS $$
BEGIN
    -- Update post status
    UPDATE tb_post
    SET status = 'published', published_at = NOW()
    WHERE id = p_post_id AND author_id = p_user_id;

    -- Publish domain event
    INSERT INTO tb_domain_events (
        event_type,
        aggregate_id,
        event_data,
        created_at
    ) VALUES (
        'POST_PUBLISHED',
        p_post_id,
        jsonb_build_object(
            'post_id', p_post_id,
            'author_id', p_user_id,
            'published_at', NOW()
        ),
        NOW()
    );

    RETURN TRUE;
END;
$$ LANGUAGE plpgsql;
```

### Materialized Read Models
```sql
-- Materialized view for performance
CREATE MATERIALIZED VIEW mv_user_dashboard AS
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id,
        'name', u.name,
        'stats', jsonb_build_object(
            'total_posts', COALESCE(posts.count, 0),
            'total_views', COALESCE(views.total, 0),
            'last_post_date', posts.last_date
        )
    ) AS data
FROM tb_user u
LEFT JOIN (
    SELECT
        author_id,
        COUNT(*) as count,
        MAX(created_at) as last_date
    FROM tb_post
    WHERE status = 'published'
    GROUP BY author_id
) posts ON u.id = posts.author_id
LEFT JOIN (
    SELECT
        author_id,
        SUM(view_count) as total
    FROM tb_post_stats
    GROUP BY author_id
) views ON u.id = views.author_id;

-- Refresh strategy
CREATE OR REPLACE FUNCTION refresh_user_dashboard()
RETURNS TRIGGER AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_user_dashboard;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;
```

### Command Validation
```python
@fraiseql.input
class CreatePostInput:
    title: str
    content: str
    tags: list[str] = []

    def validate(self):
        """Command-side validation"""
        if len(self.title) < 3:
            raise ValueError("Title must be at least 3 characters")
        if len(self.content) < 50:
            raise ValueError("Content must be at least 50 characters")
        if len(self.tags) > 10:
            raise ValueError("Maximum 10 tags allowed")

@fraiseql.mutation
async def create_post(info, input: CreatePostInput) -> Post:
    """Command with validation"""
    # Validate input
    input.validate()

    # Check business rules
    user = info.context.get("user")
    if not user:
        raise GraphQLError("Authentication required", code="AUTH_REQUIRED")

    repo = info.context["repo"]

    # Execute command
    post_id = await repo.call_function(
        "fn_create_post",
        p_title=input.title,
        p_content=input.content,
        p_tags=input.tags,
        p_author_id=user.id
    )

    # Return from read model
    result = await repo.find_one("v_post_detail", where={"id": post_id})
    return Post(**result)
```

## CQRS Benefits with FraiseQL

### Performance
- **Optimized reads**: Views can be heavily optimized for query patterns
- **Optimized writes**: Functions handle complex business logic efficiently
- **Caching**: Read models can be cached independently

### Scalability
- **Read replicas**: Query side can use read-only database replicas
- **Materialized views**: Pre-computed aggregations for expensive queries
- **Independent scaling**: Scale read and write operations separately

### Maintainability
- **Clear separation**: Commands and queries have distinct responsibilities
- **Business logic**: Encapsulated in PostgreSQL functions
- **Domain modeling**: Tables represent write model, views represent read model

## Best Practices

### Command Design
```python
# ✅ Good: Single responsibility
@fraiseql.mutation
async def approve_post(info, post_id: ID) -> Post:
    """Single command: approve a post"""
    pass

# ❌ Bad: Multiple responsibilities
@fraiseql.mutation
async def approve_and_publish_post(info, post_id: ID) -> Post:
    """Two commands mixed together"""
    pass
```

### Query Optimization
```sql
-- ✅ Good: Optimized view with indexes
CREATE VIEW v_popular_posts AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'view_count', COALESCE(s.view_count, 0)
    ) AS data
FROM tb_post p
LEFT JOIN tb_post_stats s ON p.id = s.post_id
WHERE p.status = 'published';

CREATE INDEX idx_popular_posts_views ON tb_post_stats(view_count DESC);
```

### Error Handling
```python
@fraiseql.mutation
async def delete_post(info, post_id: ID) -> bool:
    """Command with proper error handling"""
    repo = info.context["repo"]
    user = info.context.get("user")

    try:
        result = await repo.call_function(
            "fn_delete_post",
            p_post_id=post_id,
            p_user_id=user.id
        )
        return result

    except Exception as e:
        if "not found" in str(e):
            raise GraphQLError("Post not found", code="NOT_FOUND")
        elif "permission" in str(e):
            raise GraphQLError("Not authorized", code="FORBIDDEN")
        else:
            raise GraphQLError("Delete failed", code="DELETE_FAILED")
```

## See Also

### Related Concepts
- [**Architecture Overview**](../core-concepts/architecture.md) - FraiseQL's CQRS foundation
- [**Event Sourcing**](event-sourcing.md) - Event-driven CQRS patterns
- [**Database Views**](../core-concepts/database-views.md) - Read model optimization

### Implementation Guides
- [**PostgreSQL Functions**](../mutations/postgresql-function-based.md) - Command implementation
- [**Performance Tuning**](performance.md) - Optimizing CQRS performance
- [**Multi-tenancy**](multi-tenancy.md) - CQRS in multi-tenant systems

### Advanced Topics
- [**Domain-Driven Design**](database-api-patterns.md) - DDD with CQRS
- [**Bounded Contexts**](bounded-contexts.md) - Context boundaries
- [**Testing**](../testing/index.md) - Testing CQRS implementations
