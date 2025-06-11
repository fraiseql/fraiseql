# Database Views

Database views are the cornerstone of FraiseQL's architecture. Each GraphQL type is backed by a PostgreSQL view that returns JSON data, enabling efficient query resolution without N+1 problems.

## The View Pattern

Every entity in your GraphQL schema should have a corresponding database view:

```sql
-- Table structure
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    data JSONB NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Corresponding view
CREATE VIEW users_view AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', data->>'name',
        'email', data->>'email',
        'createdAt', created_at
    ) as data
FROM users;
```

## View Naming Convention

FraiseQL expects views to follow a naming pattern:
- Entity type: `User` → View name: `v_users`
- Entity type: `BlogPost` → View name: `v_blog_posts`

The pattern is: `v_` prefix, lowercase, plural, underscore-separated.

**Note**: Some examples may use `users_view` pattern, but `v_users` is preferred for brevity.

## Supported View Types

FraiseQL works with any PostgreSQL relation that returns a JSONB `data` column:

### 1. Standard Views
The most common approach - virtual views that execute at query time:
```sql
CREATE VIEW v_users AS
SELECT id, jsonb_build_object(...) as data FROM users;
```

### 2. Materialized Views
For expensive computations or aggregations:
```sql
CREATE MATERIALIZED VIEW v_user_stats AS
SELECT user_id as id, jsonb_build_object(
    'user_id', user_id,
    'post_count', COUNT(posts.id),
    'comment_count', COUNT(comments.id),
    'last_active', MAX(GREATEST(posts.created_at, comments.created_at))
) as data
FROM users
LEFT JOIN posts ON posts.author_id = users.id
LEFT JOIN comments ON comments.author_id = users.id
GROUP BY user_id;

-- Create indexes for concurrent refresh
CREATE UNIQUE INDEX idx_v_user_stats_id ON v_user_stats(id);

-- Refresh strategies
-- 1. Manual refresh
REFRESH MATERIALIZED VIEW CONCURRENTLY v_user_stats;

-- 2. Scheduled refresh (using pg_cron)
SELECT cron.schedule('refresh-user-stats', '0 * * * *', 'REFRESH MATERIALIZED VIEW CONCURRENTLY v_user_stats;');

-- 3. Trigger-based refresh (on data changes)
CREATE OR REPLACE FUNCTION refresh_user_stats_trigger()
RETURNS TRIGGER AS $$
BEGIN
    -- Refresh asynchronously to avoid blocking the transaction
    PERFORM pg_notify('refresh_user_stats', '');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
```

### 3. Projection Tables
For ultimate performance, use regular tables that store pre-computed JSON:
```sql
-- Create a projection table
CREATE TABLE v_users_projection (
    id INTEGER PRIMARY KEY,
    data JSONB NOT NULL,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Populate with triggers or batch updates
CREATE OR REPLACE FUNCTION update_user_projection()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO v_users_projection (id, data)
    VALUES (
        NEW.id,
        jsonb_build_object(
            'id', NEW.id,
            'email', NEW.email,
            'first_name', NEW.first_name,
            'last_name', NEW.last_name,
            'is_active', NEW.is_active
        )
    )
    ON CONFLICT (id) DO UPDATE
    SET data = EXCLUDED.data,
        updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER user_projection_trigger
AFTER INSERT OR UPDATE ON users
FOR EACH ROW EXECUTE FUNCTION update_user_projection();
```

As long as your view/table has an `id` column and a `data` JSONB column, FraiseQL can query it.

## Basic View Structure

### Simple Entity View

```sql
CREATE VIEW v_products AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', data->>'name',
        'price', (data->>'price')::numeric,
        'in_stock', (data->>'in_stock')::boolean,
        'description', data->>'description'
    ) as data
FROM products;
```

### View with Computed Fields

```sql
CREATE VIEW v_orders AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'order_number', data->>'order_number',
        'items', data->'items',
        'subtotal', (data->>'subtotal')::numeric,
        'tax', (data->>'tax')::numeric,
        'total', (
            (data->>'subtotal')::numeric +
            (data->>'tax')::numeric
        ),
        'status', data->>'status',
        'created_at', created_at
    ) as data
FROM orders;
```

## Composing Views

The real power comes from composing views to handle relationships:

### One-to-One Relationships

```sql
CREATE VIEW v_users AS
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id,
        'name', u.data->>'name',
        'email', u.data->>'email',
        'profile', p.data
    ) as data
FROM users u
LEFT JOIN v_profiles p ON p.id = u.profile_id;
```

### One-to-Many Relationships

```sql
CREATE VIEW v_authors AS
SELECT
    a.id,
    jsonb_build_object(
        'id', a.id,
        'name', a.data->>'name',
        'bio', a.data->>'bio',
        'posts', COALESCE(
            (SELECT jsonb_agg(p.data ORDER BY p.id DESC)
             FROM v_posts p
             WHERE p.data->>'author_id' = a.id::text),
            '[]'::jsonb
        )
    ) as data
FROM authors a;
```

### Many-to-Many Relationships

```sql
-- Tags view
CREATE VIEW v_tags AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', data->>'name',
        'slug', data->>'slug'
    ) as data
FROM tags;

-- Posts with tags
CREATE VIEW v_posts AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'title', p.data->>'title',
        'content', p.data->>'content',
        'tags', COALESCE(
            (SELECT jsonb_agg(t.data ORDER BY t.data->>'name')
             FROM post_tags pt
             JOIN v_tags t ON t.id = pt.tag_id
             WHERE pt.post_id = p.id),
            '[]'::jsonb
        )
    ) as data
FROM posts p;
```

## Performance Optimization

### Indexes for Views

Create appropriate indexes to support your views:

```sql
-- GIN index for JSONB queries
CREATE INDEX idx_users_data ON users USING GIN (data);

-- B-tree index for foreign keys
CREATE INDEX idx_posts_author_id ON posts ((data->>'authorId'));

-- Expression index for common queries
CREATE INDEX idx_users_email ON users ((data->>'email'));
```

### Materialized Views

For expensive computations, use materialized views:

```sql
CREATE MATERIALIZED VIEW stats_view AS
SELECT
    jsonb_build_object(
        'totalUsers', (SELECT COUNT(*) FROM users),
        'totalPosts', (SELECT COUNT(*) FROM posts),
        'activeUsers', (
            SELECT COUNT(DISTINCT data->>'authorId')
            FROM posts
            WHERE created_at > NOW() - INTERVAL '30 days'
        )
    ) as data;

-- Refresh periodically
REFRESH MATERIALIZED VIEW CONCURRENTLY stats_view;
```

### Partial Indexes

Optimize for common filter conditions:

```sql
-- Index for published posts only
CREATE INDEX idx_posts_published
ON posts ((data->>'published'))
WHERE (data->>'published')::boolean = true;
```

## Advanced Patterns

### Hierarchical Data

Using PostgreSQL's `WITH RECURSIVE`:

```sql
CREATE VIEW categories_view AS
WITH RECURSIVE cat_tree AS (
    -- Base case: root categories
    SELECT
        id,
        data,
        id as root_id,
        ARRAY[id] as path
    FROM categories
    WHERE (data->>'parentId') IS NULL

    UNION ALL

    -- Recursive case
    SELECT
        c.id,
        c.data,
        ct.root_id,
        ct.path || c.id
    FROM categories c
    JOIN cat_tree ct ON c.data->>'parentId' = ct.id::text
)
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', data->>'name',
        'path', path,
        'level', array_length(path, 1),
        'children', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', c.id,
                    'name', c.data->>'name'
                )
            )
            FROM categories c
            WHERE c.data->>'parentId' = cat_tree.id::text
        )
    ) as data
FROM cat_tree;
```

### Aggregations

Pre-compute aggregations in views:

```sql
CREATE VIEW user_stats_view AS
SELECT
    u.id,
    jsonb_build_object(
        'userId', u.id,
        'postCount', COUNT(DISTINCT p.id),
        'commentCount', COUNT(DISTINCT c.id),
        'lastPostDate', MAX(p.created_at),
        'totalLikes', COALESCE(SUM((p.data->>'likes')::int), 0)
    ) as data
FROM users u
LEFT JOIN posts p ON p.data->>'authorId' = u.id::text
LEFT JOIN comments c ON c.data->>'authorId' = u.id::text
GROUP BY u.id;
```

### Conditional Fields

Include fields based on conditions:

```sql
CREATE VIEW posts_view AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'title', p.data->>'title',
        'preview', CASE
            WHEN (p.data->>'published')::boolean = false
            THEN NULL
            ELSE LEFT(p.data->>'content', 200)
        END,
        'content', CASE
            WHEN (p.data->>'published')::boolean = true
            THEN p.data->>'content'
            ELSE NULL
        END
    ) as data
FROM posts p;
```

## Best Practices

1. **Keep Views Simple**: Complex logic in views can be hard to debug
2. **Use COALESCE**: Always provide defaults for aggregations
3. **Index Strategically**: Add indexes based on actual query patterns
4. **Monitor Performance**: Use `EXPLAIN ANALYZE` on view queries
5. **Version Control**: Track view definitions in migrations
6. **Document Views**: Add comments explaining complex logic

```sql
COMMENT ON VIEW users_view IS 'Main view for User type with profile data';
```

## Testing Views

Always test your views directly:

```sql
-- Test basic selection
SELECT * FROM users_view WHERE id = 1;

-- Test JSON field extraction
SELECT data->>'name' as name FROM users_view;

-- Test relationships
SELECT
    data->>'name' as author,
    jsonb_array_length(data->'posts') as post_count
FROM authors_view;
```

## Next Steps

- Learn about [Query Translation](./query-translation.md)
- Explore [Performance Optimization](../advanced/performance.md)
- Read about [Testing Strategies](../advanced/testing.md)
