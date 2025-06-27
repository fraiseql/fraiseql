# Database Patterns in FraiseQL

## The JSONB Data Column Pattern

FraiseQL uses a unique pattern where all database views must return object data in a JSONB `data` column. This guide explains why and how to implement this pattern correctly.

## Table of Contents

1. [Why JSONB?](#why-jsonb)
2. [The Pattern](#the-pattern)
3. [Creating Views](#creating-views)
4. [Common Patterns](#common-patterns)
5. [Migration Guide](#migration-guide)
6. [Troubleshooting](#troubleshooting)

---

## Why JSONB?

### Traditional Approach Problems

```sql
-- ❌ Traditional view with separate columns
CREATE VIEW user_view AS
SELECT id, name, email, created_at
FROM users;
```

Problems:
- Type mapping complexity
- Nullable field handling
- Nested object challenges
- Inconsistent serialization

### FraiseQL's Solution

```sql
-- ✅ FraiseQL pattern with JSONB data column
CREATE VIEW user_view AS
SELECT
    id,              -- For filtering
    email,           -- For lookups
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at
    ) as data        -- All object data
FROM users;
```

Benefits:
- **Consistent API** - All views work the same way
- **Type Safety** - JSONB matches Python types perfectly
- **Nested Data** - Easy to include related objects
- **Null Handling** - JSONB handles nulls gracefully
- **Performance** - Single column to fetch

---

## The Pattern

### Basic Structure

Every view in FraiseQL must follow this pattern:

```sql
CREATE VIEW view_name AS
SELECT
    -- Filter columns (for WHERE clauses)
    id,
    tenant_id,
    status,
    
    -- Data column (for object instantiation)
    jsonb_build_object(
        'field1', value1,
        'field2', value2,
        -- ... all fields for the GraphQL type
    ) as data
FROM table_name;
```

### Key Rules

1. **Always include a `data` column** - This is required
2. **`data` must be JSONB** - Not JSON or text
3. **Include filter columns** - Columns used in WHERE clauses
4. **Match GraphQL type fields** - All type fields must be in the JSONB

---

## Creating Views

### Simple Object View

```python
# Python type
@fraiseql.type
class User:
    id: UUID
    email: str
    name: str
    is_active: bool
    created_at: datetime
```

```sql
-- Corresponding view
CREATE VIEW user_view AS
SELECT
    id,              -- For filtering by ID
    email,           -- For email lookups
    is_active,       -- For status filtering
    created_at,      -- For date filtering
    jsonb_build_object(
        'id', id,
        'email', email,
        'name', name,
        'is_active', is_active,
        'created_at', created_at
    ) as data
FROM users;
```

### View with Computed Fields

```sql
CREATE VIEW user_profile_view AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'email', email,
        'name', name,
        'full_name', first_name || ' ' || last_name,  -- Computed
        'post_count', (
            SELECT COUNT(*) FROM posts WHERE author_id = u.id
        ),
        'last_login_days_ago', 
            EXTRACT(DAY FROM NOW() - last_login_at)::int
    ) as data
FROM users u;
```

### View with Nested Objects

```python
# Python types
@fraiseql.type
class Author:
    id: UUID
    name: str
    email: str

@fraiseql.type
class Post:
    id: UUID
    title: str
    content: str
    author: Author  # Nested object
```

```sql
-- View with nested author
CREATE VIEW post_with_author AS
SELECT
    p.id,
    p.author_id,     -- For filtering
    p.created_at,    -- For sorting
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'content', p.content,
        'author', (
            SELECT jsonb_build_object(
                'id', u.id,
                'name', u.name,
                'email', u.email
            )
            FROM users u
            WHERE u.id = p.author_id
        )
    ) as data
FROM posts p;
```

### View with Arrays

```python
@fraiseql.type
class BlogPost:
    id: UUID
    title: str
    tags: list[str]
    comments: list[Comment]
```

```sql
CREATE VIEW blog_post_view AS
SELECT
    p.id,
    p.published_at,
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'tags', COALESCE(p.tags, '[]'::jsonb),  -- Array field
        'comments', COALESCE(
            (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', c.id,
                        'content', c.content,
                        'author_name', c.author_name,
                        'created_at', c.created_at
                    )
                    ORDER BY c.created_at DESC
                )
                FROM comments c
                WHERE c.post_id = p.id
            ),
            '[]'::jsonb  -- Empty array if no comments
        )
    ) as data
FROM posts p;
```

---

## Common Patterns

### Multi-Tenant Views

```sql
CREATE VIEW tenant_resource_view AS
SELECT
    id,
    tenant_id,       -- ALWAYS include for filtering
    owner_id,
    status,
    jsonb_build_object(
        'id', id,
        'name', name,
        'description', description,
        'owner_id', owner_id,
        'status', status,
        'created_at', created_at
    ) as data
FROM resources;

-- Usage in query
-- await db.find("tenant_resource_view", tenant_id=tenant_id)
```

### Pagination-Ready Views

```sql
CREATE VIEW paginated_posts AS
SELECT
    id,
    author_id,
    published_at,    -- For ordering
    jsonb_build_object(
        'id', id,
        'title', title,
        'excerpt', LEFT(content, 200),
        'author_name', (
            SELECT name FROM users WHERE id = p.author_id
        ),
        'published_at', published_at,
        'comment_count', (
            SELECT COUNT(*) FROM comments WHERE post_id = p.id
        )
    ) as data
FROM posts p
WHERE published_at IS NOT NULL;  -- Only published posts
```

### Search-Optimized Views

```sql
CREATE VIEW searchable_posts AS
SELECT
    id,
    author_id,
    to_tsvector('english', title || ' ' || content) as search_vector,
    jsonb_build_object(
        'id', id,
        'title', title,
        'content', content,
        'author_id', author_id,
        'search_rank', 0.0  -- Will be updated in query
    ) as data
FROM posts;

-- Create index for performance
CREATE INDEX idx_searchable_posts_search 
ON posts USING gin(to_tsvector('english', title || ' ' || content));
```

### Soft Delete Pattern

```sql
CREATE VIEW active_users AS
SELECT
    id,
    email,
    jsonb_build_object(
        'id', id,
        'email', email,
        'name', name,
        'created_at', created_at
    ) as data
FROM users
WHERE deleted_at IS NULL;  -- Soft delete filter in view
```

### Audit Trail Pattern

```sql
CREATE VIEW user_with_audit AS
SELECT
    u.id,
    u.email,
    jsonb_build_object(
        'id', u.id,
        'email', u.email,
        'name', u.name,
        'created_at', u.created_at,
        'created_by', (
            SELECT data FROM user_view WHERE id = u.created_by_id
        ),
        'updated_at', u.updated_at,
        'updated_by', (
            SELECT data FROM user_view WHERE id = u.updated_by_id
        ),
        'version', u.version
    ) as data
FROM users u;
```

---

## Migration Guide

### From Column-Based Views

Before (traditional columns):
```sql
-- Old view
CREATE VIEW old_user_view AS
SELECT id, name, email, is_active
FROM users;
```

After (JSONB pattern):
```sql
-- New FraiseQL view
CREATE VIEW user_view AS
SELECT
    id,           -- Keep for filtering
    email,        -- Keep for lookups
    is_active,    -- Keep for filtering
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'is_active', is_active
    ) as data
FROM users;
```

### From JSON (not JSONB)

Before:
```sql
CREATE VIEW user_json AS
SELECT
    id,
    row_to_json(users.*) as data  -- Returns JSON, not JSONB
FROM users;
```

After:
```sql
CREATE VIEW user_view AS
SELECT
    id,
    jsonb_build_object(     -- Use jsonb_build_object
        'id', id,
        'name', name,
        'email', email
        -- Explicitly list fields
    ) as data
FROM users;
```

### Adding Filter Columns

If queries are slow, add more filter columns:

```sql
-- Before: Only ID for filtering
CREATE VIEW post_view AS
SELECT
    id,
    jsonb_build_object(...) as data
FROM posts;

-- After: Add common filter columns
CREATE VIEW post_view AS
SELECT
    id,
    author_id,      -- Added for author filtering
    status,         -- Added for status filtering
    published_at,   -- Added for date filtering
    jsonb_build_object(...) as data
FROM posts;
```

---

## Troubleshooting

### Error: "View must have a 'data' column"

```sql
-- ❌ Wrong - missing data column
CREATE VIEW bad_view AS
SELECT id, name, email FROM users;

-- ✅ Correct
CREATE VIEW good_view AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email
    ) as data
FROM users;
```

### Error: "Cannot instantiate type from view"

Check that:
1. The view has a `data` column
2. The `data` column is JSONB (not JSON or TEXT)
3. All required fields are in the JSONB object

### Performance Issues

1. **Add indexes on filter columns**:
```sql
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_tenant_active 
    ON users(tenant_id, is_active) 
    WHERE deleted_at IS NULL;
```

2. **Use materialized views for complex aggregations**:
```sql
CREATE MATERIALIZED VIEW user_stats AS
SELECT
    id,
    jsonb_build_object(
        'user_id', id,
        'post_count', COUNT(DISTINCT p.id),
        'comment_count', COUNT(DISTINCT c.id),
        'total_likes', SUM(p.like_count)
    ) as data
FROM users u
LEFT JOIN posts p ON p.author_id = u.id
LEFT JOIN comments c ON c.author_id = u.id
GROUP BY u.id;

-- Refresh periodically
REFRESH MATERIALIZED VIEW user_stats;
```

3. **Avoid N+1 queries with proper joins**:
```sql
-- Instead of subqueries for each post
CREATE VIEW optimized_post_list AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'author', u.user_data,  -- Pre-computed
        'comment_count', COALESCE(c.count, 0)
    ) as data
FROM posts p
LEFT JOIN (
    SELECT id, jsonb_build_object(
        'id', id, 'name', name, 'email', email
    ) as user_data
    FROM users
) u ON u.id = p.author_id
LEFT JOIN (
    SELECT post_id, COUNT(*) as count
    FROM comments
    GROUP BY post_id
) c ON c.post_id = p.id;
```

## Best Practices

1. **Always include primary key in filter columns**
2. **Add commonly used WHERE clause columns**
3. **Use COALESCE for nullable arrays** to return empty arrays
4. **Keep JSONB flat when possible** for better performance
5. **Create indexes on filter columns**
6. **Use views for read, functions for write**
7. **Document your views** with comments

```sql
COMMENT ON VIEW user_view IS 'Active users with profile data. Excludes soft-deleted records.';
COMMENT ON COLUMN user_view.data IS 'JSONB containing all User type fields';
```

## Summary

The JSONB data column pattern is fundamental to FraiseQL. It provides:
- Consistent data access
- Type safety
- Excellent null handling
- Support for nested data
- Clear separation between filtering and data

Always remember: **filter columns for WHERE, data column for objects**.