# Debugging Guide

Practical debugging strategies for FraiseQL applications.

## Debug Mode

### Enable Debug Logging

```python
# Environment variable
export FRAISEQL_LOG_LEVEL=DEBUG

# Or in code
import logging
logging.basicConfig(level=logging.DEBUG)
```

### What Debug Mode Shows

- SQL queries being executed
- GraphQL resolver execution
- Error stack traces
- Query execution timing

## Common Debugging Tasks

### View Not Found

```sql
-- Check if view exists
\dv

-- Show view definition
\d+ v_user

-- Create missing view
CREATE VIEW v_user AS
SELECT id, email, name FROM users;
```

### Check PostgreSQL Function

```sql
-- List functions
\df fn_create_*

-- Show function definition
\sf fn_create_user
```

### Test Database Connection

```bash
# Test connection
psql $DATABASE_URL -c "SELECT 1"

# Check FraiseQL can connect
python -c "from fraiseql import Repository; Repository('$DATABASE_URL')"
```

## Query Debugging

### Log SQL Queries

```python
import logging

# Enable SQL logging
logging.getLogger("fraiseql.sql").setLevel(logging.DEBUG)

@query
async def get_user(info, id: str):
    # SQL will be logged
    return await info.context["repo"].find_one("v_user", where={"id": id})
```

### Check Query Performance

```sql
-- Analyze query performance
EXPLAIN ANALYZE
SELECT * FROM v_user WHERE id = '123';

-- Check for missing indexes
SELECT * FROM pg_stats
WHERE tablename = 'users' AND attname = 'email';
```

## Error Debugging

### Get Full Error Details

```python
from graphql import GraphQLError

try:
    result = await repo.find_one("v_user", where={"id": id})
except Exception as e:
    # Log full error
    logger.error(f"Query failed: {e}", exc_info=True)

    # Return user-friendly error
    raise GraphQLError(
        message="User not found",
        extensions={"code": "NOT_FOUND", "debug": str(e)}
    )
```

### Test Mutations

```python
# Test PostgreSQL function directly
async def test_mutation():
    repo = Repository(DATABASE_URL)
    result = await repo.call_function(
        "fn_create_user",
        p_email="test@example.com",
        p_name="Test User"
    )
    print(f"Result: {result}")
```

## GraphQL Playground

### Enable Playground

```python
from fraiseql.fastapi import GraphQLRouter

router = GraphQLRouter(
    schema=schema,
    playground=True  # Enable playground in development
)
```

### Test Queries

```graphql
# Test in playground at /graphql
query GetUser {
  user(id: "123") {
    id
    name
    email
  }
}

# With variables
query GetUser($id: ID!) {
  user(id: $id) {
    id
    name
  }
}

# Variables:
{
  "id": "123"
}
```

## N+1 Query Prevention

### Use Composed Views

FraiseQL prevents N+1 queries by using PostgreSQL views that compose the data:

```sql
-- Bad: Separate queries for users and posts
CREATE VIEW v_user AS
SELECT id, name, email FROM users;

CREATE VIEW v_post AS
SELECT id, user_id, title FROM posts;

-- Good: Composed view with all data
CREATE VIEW v_user_with_posts AS
SELECT
    u.id,
    u.name,
    u.email,
    COALESCE(
        json_agg(
            json_build_object(
                'id', p.id,
                'title', p.title,
                'content', p.content
            ) ORDER BY p.created_at DESC
        ) FILTER (WHERE p.id IS NOT NULL),
        '[]'::json
    ) AS posts
FROM users u
LEFT JOIN posts p ON p.user_id = u.id
GROUP BY u.id;
```

### Use LATERAL Joins for Complex Relationships

```sql
-- Efficient view for users with their latest posts
CREATE VIEW v_user_recent_posts AS
SELECT
    u.id,
    u.name,
    recent_posts.posts
FROM users u
LEFT JOIN LATERAL (
    SELECT json_agg(
        json_build_object(
            'id', p.id,
            'title', p.title,
            'created_at', p.created_at
        )
    ) AS posts
    FROM posts p
    WHERE p.user_id = u.id
    ORDER BY p.created_at DESC
    LIMIT 5
) recent_posts ON true;
```

### Type Definition for Composed Views

```python
from fraiseql import fraise_type
from typing import List, Optional

@fraise_type
class Post:
    id: str
    title: str
    content: str

@fraise_type
class User:
    id: str
    name: str
    email: str
    posts: List[Post] = []  # Populated from view's JSON column
```

## Development Tools

### Auto-reload on Changes

```bash
# Use watchdog for auto-reload
pip install watchdog
watchmedo auto-restart --patterns="*.py" --recursive -- python app.py
```

### Schema Validation

```python
# Validate schema on startup
from fraiseql import create_schema

try:
    schema = create_schema()
    print("Schema valid")
except Exception as e:
    print(f"Schema error: {e}")
```

## Testing

### Mock Repository

```python
# Create mock for testing
class MockRepo:
    async def find_one(self, view, where):
        if where.get("id") == "123":
            return {
                "id": "123",
                "name": "Test",
                "posts": [
                    {"id": "1", "title": "Post 1"},
                    {"id": "2", "title": "Post 2"}
                ]
            }
        return None

# Test with mock
info = Mock(context={"repo": MockRepo()})
result = await get_user(info, id="123")
assert result["name"] == "Test"
assert len(result["posts"]) == 2
```

### Test Error Handling

```python
import pytest
from graphql import GraphQLError

@pytest.mark.asyncio
async def test_not_found_error():
    with pytest.raises(GraphQLError) as exc:
        await get_user(info, id="nonexistent")

    assert exc.value.extensions["code"] == "NOT_FOUND"
```

## Production Debugging

### Safe Error Messages

```python
import os

def get_error_message(error):
    if os.environ.get("FRAISEQL_ENV") == "production":
        # Don't leak details in production
        return "An error occurred"
    else:
        # Full details in development
        return str(error)
```

### Health Check Endpoint

```python
@app.get("/health")
async def health_check():
    try:
        # Check database
        await repo.execute("SELECT 1")
        return {"status": "healthy", "database": "connected"}
    except Exception as e:
        return {"status": "unhealthy", "error": str(e)}
```

## Performance Debugging

### Analyze View Performance

```sql
-- Check view execution plan
EXPLAIN (ANALYZE, BUFFERS)
SELECT * FROM v_user_with_posts WHERE id = '123';

-- Identify slow parts
-- Look for:
-- - Seq Scan (should be Index Scan for WHERE conditions)
-- - High cost numbers
-- - Large row counts in loops
```

### Optimize JSON Aggregation

```sql
-- If JSON aggregation is slow, consider materialized view
CREATE MATERIALIZED VIEW mv_user_with_stats AS
SELECT
    u.id,
    u.name,
    COUNT(p.id) as post_count,
    json_agg(p.* ORDER BY p.created_at DESC) as posts
FROM users u
LEFT JOIN posts p ON p.user_id = u.id
GROUP BY u.id;

-- Create index for fast lookups
CREATE UNIQUE INDEX ON mv_user_with_stats(id);

-- Refresh periodically
REFRESH MATERIALIZED VIEW CONCURRENTLY mv_user_with_stats;
```

## Quick Debugging Checklist

1. **Connection Issues**

   - Is PostgreSQL running?
   - Is DATABASE_URL correct?
   - Can you connect with psql?

2. **Schema Issues**

   - Does the view exist?
   - Are all columns present?
   - Do types match?

3. **Query Issues**

   - Is the WHERE clause valid?
   - Are you using supported operators?
   - Check the SQL being generated

4. **Mutation Issues**

   - Does the function exist?
   - Are parameters correct?
   - Test function directly in psql

5. **Performance Issues**

   - Are you using composed views to avoid N+1?
   - Do you have proper indexes?
   - Consider materialized views for complex aggregations

## Next Steps

- Review [error types](./error-types.md) for specific errors
- See [troubleshooting guide](./troubleshooting.md) for common issues
- Learn [error handling patterns](./handling-patterns.md)
