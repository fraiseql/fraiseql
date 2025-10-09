---
← [Query Translation](./query-translation.md) | [Core Concepts Index](./index.md) | [Advanced Topics →](../advanced/index.md)
---

# Database Views

> **In this section:** Master the art of designing efficient database views for GraphQL APIs
> **Prerequisites:** SQL knowledge and understanding of database design patterns
> **Time to complete:** 25 minutes

Database views are the cornerstone of FraiseQL's query system. They define how your data is exposed through the GraphQL API, implementing the read side of the CQRS pattern.

## View Design Principles

### 1. JSONB-First Design
Every view returns a JSONB `data` column containing the complete object structure:

```sql
CREATE OR REPLACE VIEW v_user AS
SELECT
    id,                    -- For filtering and joins
    email,                 -- For filtering
    is_active,            -- For filtering
    jsonb_build_object(
        '__typename', 'User',  -- GraphQL type identifier
        'id', id,
        'email', email,
        'name', name,
        'bio', bio,
        'is_active', is_active,  -- snake_case in database
        'created_at', created_at -- snake_case in database
    ) AS data
FROM tb_users;
```

**Note**: Fields are stored in snake_case in the database. FraiseQL automatically converts to camelCase when serving GraphQL responses.

## Performance and JSONB Optimization

### Why Separate Filter Columns?

One of FraiseQL's most common questions: **"Why do I need both `id` as a column AND inside the JSONB `data`?"**

The answer: **PostgreSQL query performance**.

#### The Performance Problem with JSONB-Only Views

```sql
-- ❌ ANTI-PATTERN: Everything in JSONB (slow filtering)
CREATE OR REPLACE VIEW v_user_bad AS
SELECT
    jsonb_build_object(
        'id', id,
        'email', email,
        'name', name,
        'is_active', is_active,
        'created_at', created_at
    ) AS data
FROM tb_users;
```

**When you query with filters:**
```sql
-- This query must scan JSONB for every row
SELECT * FROM v_user_bad
WHERE data->>'is_active' = 'true';  -- String comparison!
```

**Problems:**
1. **No indexes work** - PostgreSQL can't use regular B-tree indexes on JSONB extraction
2. **Type casting overhead** - `data->>'is_active'` extracts as text, requiring cast to boolean
3. **Full table scan** - Every row must be examined
4. **Slow on large tables** - 100ms+ for 10,000+ rows

#### The High-Performance Pattern

```sql
-- ✅ BEST PRACTICE: Filter columns + JSONB data
CREATE OR REPLACE VIEW v_user AS
SELECT
    id,                    -- Separate column for WHERE id = ?
    email,                 -- Separate column for WHERE email = ?
    is_active,            -- Separate column for WHERE is_active = true
    created_at,           -- Separate column for ORDER BY created_at
    jsonb_build_object(
        'id', id,          -- Also in JSONB for GraphQL response
        'email', email,
        'name', name,
        'is_active', is_active,
        'created_at', created_at
    ) AS data
FROM tb_users;
```

**When you query with filters:**
```sql
-- Uses native column with index
SELECT * FROM v_user
WHERE is_active = true;  -- Boolean comparison, uses index!
```

**Benefits:**
1. **Indexes work** - PostgreSQL uses B-tree indexes on native columns
2. **Native types** - No type casting overhead
3. **Index-only scans** - Can satisfy queries from index alone
4. **100x faster** - 1ms vs 100ms on 10,000+ rows

### Performance Benchmarks

Real-world performance comparison on a table with 100,000 users:

| View Design | Query Type | Without Index | With Index | Improvement |
|-------------|-----------|---------------|------------|-------------|
| **JSONB-only** | `WHERE data->>'is_active' = 'true'` | 145ms | 142ms | Minimal (GIN index) |
| **Separate columns** | `WHERE is_active = true` | 85ms | **0.8ms** | **180x faster** |
| **JSONB-only** | `WHERE data->>'email' = 'john@example.com'` | 152ms | 89ms | 1.7x |
| **Separate columns** | `WHERE email = 'john@example.com'` | 82ms | **0.2ms** | **410x faster** |

```sql
-- Test yourself:
EXPLAIN (ANALYZE, BUFFERS)
SELECT * FROM v_user WHERE is_active = true;

-- Example output:
-- Index Scan using idx_users_is_active  (cost=0.29..8.31 rows=1 width=64) (actual time=0.015..0.016 rows=1 loops=1)
--   Index Cond: (is_active = true)
-- Planning Time: 0.089 ms
-- Execution Time: 0.031 ms
```

### When JSONB Optimization Applies

FraiseQL's "JSON Passthrough" optimization provides **sub-millisecond responses** when:

#### ✅ Optimization Applies

1. **Query uses APQ (Automatic Persisted Queries)**
   ```graphql
   # Sent as SHA-256 hash instead of full query
   ```

2. **View includes separate filter columns**
   ```sql
   SELECT id, is_active, data FROM v_user
   WHERE is_active = true  -- Uses index
   ```

3. **Query is cached in TurboRouter**
   ```python
   # Precompiled SQL template ready to execute
   ```

4. **Result set is reasonable size** (< 1000 rows by default)
   ```python
   @query
   async def users(info, limit: int = 100) -> list[User]:
       # Passthrough works: small result set
   ```

**Result:** 0.5-2ms response time

#### ❌ Optimization Doesn't Apply

1. **First-time query (not in APQ cache)**
   ```graphql
   # Full query parsing required
   ```

2. **Complex filtering on JSONB fields**
   ```sql
   WHERE data->>'custom_field' = 'value'  -- Can't use passthrough
   ```

3. **Aggregations or computations**
   ```sql
   SELECT COUNT(*), AVG(data->>'age'::int) FROM v_user  -- Computed
   ```

4. **Result set too large** (> 1000 rows)
   ```python
   @query
   async def all_users(info) -> list[User]:
       # Too large for passthrough optimization
   ```

**Result:** 25-100ms response time (still fast, just not sub-millisecond)

### Optimizing Your Views for Maximum Performance

#### Pattern 1: Basic Entity (Fast Lookups)

```sql
-- Optimized for: WHERE id = ?, WHERE email = ?
CREATE OR REPLACE VIEW v_user AS
SELECT
    id,           -- Primary key lookups
    email,        -- Unique constraint lookups
    is_active,   -- Boolean filters
    jsonb_build_object(
        'id', id,
        'email', email,
        'name', name,
        'bio', bio,
        'is_active', is_active
    ) AS data
FROM tb_users;

-- Essential indexes
CREATE INDEX idx_users_email ON tb_users(email);
CREATE INDEX idx_users_is_active ON tb_users(is_active) WHERE is_active = true;
```

**Performance:** 0.2-0.5ms for single record lookup

#### Pattern 2: Filtered Lists (Fast Pagination)

```sql
-- Optimized for: WHERE author_id = ? ORDER BY published_at LIMIT ?
CREATE OR REPLACE VIEW v_post AS
SELECT
    id,
    author_id,       -- Foreign key filter (most common)
    is_published,    -- Status filter
    published_at,    -- Sorting column
    view_count,      -- For range queries (WHERE view_count > ?)
    jsonb_build_object(
        'id', id,
        'title', title,
        'excerpt', excerpt,
        'author_id', author_id,
        'is_published', is_published,
        'published_at', published_at,
        'view_count', view_count
    ) AS data
FROM tb_posts;

-- Composite indexes for common queries
CREATE INDEX idx_posts_author_published ON tb_posts(author_id, published_at DESC)
    WHERE is_published = true;
```

**Performance:** 0.8-2ms for paginated lists (20-100 items)

#### Pattern 3: Complex Aggregations (Use Materialized Views)

```sql
-- For expensive computations, pre-calculate
CREATE MATERIALIZED VIEW mv_user_statistics AS
SELECT
    user_id,
    jsonb_build_object(
        'user_id', user_id,
        'post_count', COUNT(DISTINCT p.id),
        'comment_count', COUNT(DISTINCT c.id),
        'total_views', SUM(p.view_count),
        'engagement_score', (
            COUNT(DISTINCT p.id) * 10 +
            COUNT(DISTINCT c.id) * 2 +
            SUM(p.view_count) * 0.1
        )
    ) AS data
FROM tb_users u
LEFT JOIN tb_posts p ON p.author_id = u.id
LEFT JOIN tb_comments c ON c.author_id = u.id
GROUP BY u.id;

-- Create index on materialized view
CREATE UNIQUE INDEX idx_mv_user_statistics_user_id
    ON mv_user_statistics(user_id);

-- Refresh strategy (every 15 minutes)
REFRESH MATERIALIZED VIEW CONCURRENTLY mv_user_statistics;
```

**Performance:** 0.5-1ms (after refresh), vs 50-200ms if computed on-the-fly

### Index Strategy for FraiseQL Views

#### Essential Indexes

1. **Primary Key** (automatically indexed)
   ```sql
   -- Already has index via PRIMARY KEY constraint
   ```

2. **Foreign Keys** (index manually)
   ```sql
   CREATE INDEX idx_posts_author_id ON tb_posts(author_id);
   CREATE INDEX idx_comments_post_id ON tb_comments(post_id);
   ```

3. **Boolean Filters** (partial index)
   ```sql
   -- Only index TRUE values if that's the common query
   CREATE INDEX idx_users_is_active ON tb_users(is_active)
       WHERE is_active = true;
   ```

4. **Timestamp Sorting** (descending order common)
   ```sql
   CREATE INDEX idx_posts_published_at ON tb_posts(published_at DESC);
   ```

5. **Composite Indexes** (for multi-column queries)
   ```sql
   -- For: WHERE author_id = ? AND is_published = ? ORDER BY published_at
   CREATE INDEX idx_posts_author_published ON tb_posts(
       author_id,
       is_published,
       published_at DESC
   );
   ```

#### JSONB Indexes (When Needed)

Only add JSONB indexes when you MUST filter on JSONB fields:

```sql
-- GIN index for containment queries
CREATE INDEX idx_posts_data_gin ON tb_posts USING gin(data);

-- Use for queries like:
SELECT * FROM tb_posts
WHERE data @> '{"tags": ["python"]}'::jsonb;

-- GIN index for path queries
CREATE INDEX idx_posts_data_path_gin ON tb_posts
USING gin(data jsonb_path_ops);
```

**Cost:** GIN indexes are 3-5x larger than B-tree indexes and slower to update.

**Rule:** Only use JSONB indexes when filtering on dynamic/schema-less fields. For known fields, use separate columns.

### Measuring Your View Performance

#### 1. Query Plan Analysis

```sql
EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT * FROM v_user
WHERE is_active = true
ORDER BY created_at DESC
LIMIT 20;

-- Look for:
-- ✅ "Index Scan" or "Index Only Scan" (good)
-- ❌ "Seq Scan" (bad - full table scan)
-- ✅ Execution Time < 5ms (good)
-- ❌ Execution Time > 50ms (needs optimization)
```

#### 2. Monitor Query Performance in Production

```python
from fraiseql import query
import time

@query
async def users(info, is_active: bool = True) -> list[User]:
    start = time.time()
    repo = info.context["repo"]
    result = await repo.find("v_user", where={"is_active": is_active})
    duration = time.time() - start

    if duration > 0.050:  # > 50ms
        print(f"SLOW QUERY: v_user filter took {duration*1000:.1f}ms")

    return result
```

#### 3. Check Index Usage

```sql
-- See which indexes are actually used
SELECT
    schemaname,
    tablename,
    indexname,
    idx_scan,
    idx_tup_read,
    idx_tup_fetch
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
ORDER BY idx_scan DESC;

-- Unused indexes (consider dropping)
SELECT
    schemaname,
    tablename,
    indexname
FROM pg_stat_user_indexes
WHERE idx_scan = 0
    AND schemaname = 'public';
```

### Common Performance Pitfalls

#### Pitfall 1: No Filter Columns

```sql
-- ❌ BAD: Forces JSONB extraction on every query
CREATE VIEW v_post AS
SELECT jsonb_build_object(...) AS data
FROM tb_posts;

-- Every filter is slow:
WHERE data->>'author_id' = '123'  -- Slow JSONB extraction
```

#### Pitfall 2: Missing Indexes

```sql
-- ✅ Created view with filter columns
CREATE VIEW v_post AS
SELECT id, author_id, data FROM tb_posts;

-- ❌ But forgot the index!
-- Query: WHERE author_id = '123'
-- Result: Full table scan (slow)

-- ✅ FIX: Add the index
CREATE INDEX idx_posts_author_id ON tb_posts(author_id);
```

#### Pitfall 3: Over-Aggregation

```sql
-- ❌ BAD: Aggregating too much data
CREATE VIEW v_user_with_everything AS
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id,
        'posts', (SELECT jsonb_agg(data) FROM v_post WHERE author_id = u.id),  -- Could be 1000s
        'comments', (SELECT jsonb_agg(data) FROM v_comment WHERE author_id = u.id),  -- Could be 1000s
        'likes', (SELECT jsonb_agg(data) FROM v_like WHERE user_id = u.id)  -- Could be 1000s
    ) AS data
FROM tb_users u;

-- ✅ BETTER: Limit aggregations
'recent_posts', (
    SELECT jsonb_agg(data ORDER BY published_at DESC)
    FROM (SELECT data, published_at FROM v_post WHERE author_id = u.id LIMIT 10) p
)
```

#### Pitfall 4: N+1 in Views

```sql
-- ❌ BAD: Subquery per row
CREATE VIEW v_post AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'author', (SELECT data FROM v_user WHERE id = p.author_id)  -- Subquery per row!
    ) AS data
FROM tb_posts p;

-- ✅ BETTER: Use JOIN
CREATE VIEW v_post AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'author', u.data  -- Joined once
    ) AS data
FROM tb_posts p
LEFT JOIN v_user u ON u.id = p.author_id;
```

### Summary: The FraiseQL View Performance Formula

```
Fast Query = Separate Filter Columns + Proper Indexes + Limited Aggregation + JSON Passthrough
```

**Recipe for Sub-Millisecond Queries:**

1. ✅ Include frequently filtered columns separately (id, foreign keys, booleans, timestamps)
2. ✅ Keep the full object in JSONB `data` for GraphQL response
3. ✅ Add B-tree indexes on filter columns
4. ✅ Limit aggregations (use LIMIT in subqueries)
5. ✅ Use JOINs instead of subqueries where possible
6. ✅ Use materialized views for expensive computations
7. ✅ Enable APQ (Automatic Persisted Queries) in production

**Result:** 0.5-5ms query performance for 99% of API calls.

### 2. Filter Columns
Include columns outside the JSONB for efficient filtering:

```sql
-- Good: Separate filter columns for WHERE clauses
CREATE OR REPLACE VIEW v_post AS
SELECT
    id,
    author_id,        -- For filtering by author
    is_published,     -- For filtering published posts
    published_at,     -- For date range queries
    data
FROM (
    SELECT
        id,
        author_id,
        is_published,
        published_at,
        jsonb_build_object(...) AS data
    FROM tb_posts
) AS posts_data;
```

### 3. Composed Views
Composed views reuse existing views to build complex structures, eliminating N+1 queries:

```sql
-- First, define base views
CREATE OR REPLACE VIEW v_author AS
SELECT
    id,
    jsonb_build_object(
        '__typename', 'Author',
        'id', id,
        'email', email,
        'name', name,
        'bio', bio
    ) AS data
FROM tb_users;

CREATE OR REPLACE VIEW v_comment AS
SELECT
    id,
    post_id,
    user_id,
    created_at,  -- For sorting
    jsonb_build_object(
        '__typename', 'Comment',
        'id', id,
        'content', content,
        'created_at', created_at
    ) AS data
FROM tb_comments;

-- Then compose them together
CREATE OR REPLACE VIEW v_post_with_author AS
SELECT
    p.id,
    p.author_id,
    p.is_published,
    jsonb_build_object(
        '__typename', 'Post',
        'id', p.id,
        'title', p.title,
        'content', p.content,
        -- Reuse v_author view
        'author', a.data,
        -- Aggregate comments using v_comment
        'comments', (
            SELECT jsonb_agg(c.data ORDER BY c.created_at DESC)
            FROM v_comment c
            WHERE c.post_id = p.id
        )
    ) AS data
FROM tb_posts p
LEFT JOIN v_author a ON a.id = p.author_id;
```

## Naming Conventions

FraiseQL uses strict naming conventions for database objects:

### View Prefixes

| Prefix | Type | Description | Performance |
|--------|------|-------------|-------------|
| `v_` | Regular View | Computed on-demand | Fast for simple queries |
| `tv_` | Table View | Projection tables with foreign keys to source entities | Fastest for cached aggregations |
| `mv_` | Materialized View | PostgreSQL materialized view | Fast with periodic refresh |

### Examples

```sql
-- Regular view (computed on each query)
CREATE OR REPLACE VIEW v_active_user AS
SELECT
    id,
    email,
    jsonb_build_object(
        '__typename', 'User',
        'id', pk_user,
        'email', email,
        'name', name
    ) AS data
FROM tb_users
WHERE is_active = true;

-- Table view with proper Sacred Trinity + Foreign Key pattern
CREATE TABLE tv_user_statistics (
    id INTEGER GENERATED BY DEFAULT AS IDENTITY,
    pk_user_statistics UUID DEFAULT gen_random_uuid() NOT NULL,
    fk_user INTEGER NOT NULL,
    data JSONB NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ DEFAULT NOW(),

    CONSTRAINT pk_tv_user_statistics PRIMARY KEY (id),
    CONSTRAINT uq_tv_user_statistics_pk UNIQUE (pk_user_statistics),
    CONSTRAINT fk_tv_user_statistics_user FOREIGN KEY (fk_user) REFERENCES tb_users(id),
    CONSTRAINT uq_tv_user_statistics_user UNIQUE (fk_user)
);

-- Sync function called explicitly from mutations
CREATE OR REPLACE FUNCTION sync_user_statistics(p_user_id INTEGER)
RETURNS void AS $$
BEGIN
    INSERT INTO tv_user_statistics (fk_user, data, version, updated_at)
    SELECT
        u.id AS fk_user,
        jsonb_build_object(
            '__typename', 'UserStatistics',
            'user_id', u.pk_user,
            'post_count', COALESCE(p.post_count, 0),
            'comment_count', COALESCE(c.comment_count, 0),
            'total_views', COALESCE(p.total_views, 0),
            'engagement_score', (
                COALESCE(p.post_count, 0) * 10 +
                COALESCE(c.comment_count, 0) * 2
            )
        ) AS data,
        COALESCE((SELECT version + 1 FROM tv_user_statistics WHERE fk_user = u.id), 1),
        NOW()
    FROM tb_users u
    LEFT JOIN (
        SELECT fk_author, COUNT(*) as post_count, SUM(view_count) as total_views
        FROM tb_posts
        WHERE fk_author = p_user_id
        GROUP BY fk_author
    ) p ON p.fk_author = u.id
    LEFT JOIN (
        SELECT fk_author, COUNT(*) as comment_count
        FROM tb_comments
        WHERE fk_author = p_user_id
        GROUP BY fk_author
    ) c ON c.fk_author = u.id
    WHERE u.id = p_user_id
    ON CONFLICT (fk_user) DO UPDATE SET
        data = EXCLUDED.data,
        version = EXCLUDED.version,
        updated_at = EXCLUDED.updated_at;
END;
$$ LANGUAGE plpgsql;

-- Mutation function with explicit sync call
CREATE OR REPLACE FUNCTION fn_create_post(
    p_title text,
    p_content text,
    p_author_id integer
) RETURNS jsonb AS $$
DECLARE
    v_post_id INTEGER;
BEGIN
    -- Create post
    INSERT INTO tb_posts (title, content, fk_author)
    VALUES (p_title, p_content, p_author_id)
    RETURNING id INTO v_post_id;

    -- Explicitly sync user statistics
    PERFORM sync_user_statistics(p_author_id);

    -- Return post data
    RETURN (SELECT data FROM v_post WHERE id = v_post_id);
END;
$$ LANGUAGE plpgsql;
```

## JSONB Aggregation Patterns

### Basic Aggregation
```sql
-- Aggregate array of objects
SELECT jsonb_agg(
    jsonb_build_object(
        'id', id,
        'name', name,
        'created_at', created_at
    )
    ORDER BY created_at DESC
) AS users
FROM tb_users;
```

### Reusing Views in Aggregation
```sql
-- Compose using existing views for consistency
CREATE OR REPLACE VIEW v_user_with_post AS
SELECT
    u.id,
    jsonb_build_object(
        '__typename', 'User',
        'id', u.id,
        'name', u.name,
        'posts', COALESCE(
            (SELECT jsonb_agg(p.data ORDER BY p.published_at DESC)
             FROM v_post p
             WHERE p.author_id = u.id),
            '[]'::jsonb
        )
    ) AS data
FROM tb_users u;
```

### Conditional Aggregation
```sql
-- Include different fields based on conditions
CREATE OR REPLACE VIEW v_user_profile AS
SELECT
    id,
    jsonb_build_object(
        '__typename', 'UserProfile',
        'id', id,
        'name', name,
        'email', CASE
            WHEN is_email_public THEN email
            ELSE null
        END,
        'stats', jsonb_build_object(
            'post_count', (
                SELECT COUNT(*)
                FROM tb_posts
                WHERE author_id = u.id AND is_published = true
            ),
            'follower_count', (
                SELECT COUNT(*)
                FROM tb_user_follows
                WHERE followed_id = u.id
            )
        )
    ) AS data
FROM tb_users u;
```

## Performance Optimization Strategies

### 1. Use Indexes on Filter Columns
```sql
-- Index columns used in WHERE clauses
CREATE INDEX idx_posts_author_id ON tb_posts(author_id);
CREATE INDEX idx_posts_published ON tb_posts(is_published, published_at DESC);

-- GIN index for JSONB data
CREATE INDEX idx_posts_data_gin ON tb_posts USING gin(data);
```

### 2. Optimize Composed Views with Lateral Joins
```sql
-- Efficient composition using LATERAL
CREATE OR REPLACE VIEW v_user_with_recent_post AS
SELECT
    u.id,
    jsonb_build_object(
        '__typename', 'User',
        'id', u.id,
        'name', u.name,
        'recent_posts', COALESCE(p.posts, '[]'::jsonb)
    ) AS data
FROM tb_users u
LEFT JOIN LATERAL (
    SELECT jsonb_agg(
        v.data
        ORDER BY v.published_at DESC
    ) AS posts
    FROM v_post v
    WHERE v.author_id = u.id
        AND v.is_published = true
    LIMIT 5
) p ON true;
```

### 3. Materialized Views for Expensive Computations
```sql
-- Materialized view for analytics
CREATE MATERIALIZED VIEW mv_post_analytics AS
SELECT
    p.id,
    jsonb_build_object(
        '__typename', 'PostAnalytics',
        'id', p.id,
        'title', p.title,
        'view_count', p.view_count,
        'comment_count', COUNT(DISTINCT c.id),
        'unique_commenters', COUNT(DISTINCT c.user_id),
        'avg_rating', AVG(r.rating),
        'engagement', (
            p.view_count +
            COUNT(DISTINCT c.id) * 10 +
            COUNT(DISTINCT l.user_id) * 5
        )
    ) AS data
FROM tb_posts p
LEFT JOIN tb_comments c ON c.post_id = p.id
LEFT JOIN tb_post_likes l ON l.post_id = p.id
LEFT JOIN tb_post_ratings r ON r.post_id = p.id
GROUP BY p.id, p.title, p.view_count;

-- Refresh strategy
CREATE OR REPLACE FUNCTION refresh_post_analytics()
RETURNS void AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_post_analytics;
END;
$$ LANGUAGE plpgsql;

-- Schedule refresh (using pg_cron or similar)
SELECT cron.schedule('refresh-analytics', '*/15 * * * *',
    'SELECT refresh_post_analytics()');
```

## Complex View Examples

### Hierarchical Data (Comments with Replies)
```sql
CREATE OR REPLACE VIEW v_comment_tree AS
WITH RECURSIVE comment_tree AS (
    -- Base case: top-level comments
    SELECT
        id,
        post_id,
        parent_id,
        content,
        user_id,
        created_at,
        0 as depth,
        ARRAY[id] as path
    FROM tb_comments
    WHERE parent_id IS NULL

    UNION ALL

    -- Recursive case: replies
    SELECT
        c.id,
        c.post_id,
        c.parent_id,
        c.content,
        c.user_id,
        c.created_at,
        ct.depth + 1,
        ct.path || c.id
    FROM tb_comments c
    JOIN comment_tree ct ON c.parent_id = ct.id
    WHERE ct.depth < 5  -- Limit depth
)
SELECT
    post_id,
    jsonb_agg(
        jsonb_build_object(
            '__typename', 'Comment',
            'id', id,
            'content', content,
            'depth', depth,
            'path', path,
            'created_at', created_at,
            'author', (
                SELECT data
                FROM v_user
                WHERE id = comment_tree.user_id
            )
        )
        ORDER BY path
    ) AS comments
FROM comment_tree
GROUP BY post_id;
```

### Time-Series Data
```sql
CREATE OR REPLACE VIEW v_user_activity_timeline AS
SELECT
    user_id,
    jsonb_build_object(
        '__typename', 'ActivityTimeline',
        'user_id', user_id,
        'daily', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'date', date,
                    'post_count', post_count,
                    'comment_count', comment_count,
                    'like_count', like_count
                )
                ORDER BY date DESC
            )
            FROM (
                SELECT
                    DATE(created_at) as date,
                    COUNT(DISTINCT CASE WHEN type = 'post' THEN id END) as post_count,
                    COUNT(DISTINCT CASE WHEN type = 'comment' THEN id END) as comment_count,
                    COUNT(DISTINCT CASE WHEN type = 'like' THEN id END) as like_count
                FROM tb_user_activities
                WHERE user_id = u.id
                    AND created_at >= CURRENT_DATE - INTERVAL '30 days'
                GROUP BY DATE(created_at)
            ) daily_stats
        )
    ) AS data
FROM tb_users u;
```

### Full-Text Search View
```sql
CREATE OR REPLACE VIEW v_post_search AS
SELECT
    id,
    published_at,
    -- Create search vector
    to_tsvector('english', title || ' ' || content) as search_vector,
    jsonb_build_object(
        '__typename', 'SearchResult',
        'id', id,
        'title', title,
        'excerpt', LEFT(content, 200),
        'highlights', null,  -- Populated by search function
        'score', null        -- Populated by search function
    ) AS data
FROM tb_posts
WHERE is_published = true;

-- Index for full-text search
CREATE INDEX idx_posts_search ON tb_posts
USING gin(to_tsvector('english', title || ' ' || content));
```

## Best Practices

### 1. Always Include `__typename`
This helps with GraphQL type resolution and client-side caching:
```sql
jsonb_build_object(
    '__typename', 'User',  -- Always first
    'id', id,
    -- other fields...
)
```

### 2. Use COALESCE for Nullable Aggregations
```sql
'posts', COALESCE(
    (SELECT jsonb_agg(...) FROM tb_posts WHERE ...),
    '[]'::jsonb  -- Empty array instead of null
)
```

### 3. Consistent Field Naming

- Use snake_case throughout the database
- FraiseQL handles conversion to camelCase for GraphQL
```sql
jsonb_build_object(
    'created_at', created_at,  -- snake_case in DB
    'is_active', is_active     -- converts to camelCase in API
)
```

### 4. Limit Aggregation Depth
Prevent performance issues with deep nesting:
```sql
-- Limit related data
'recent_posts', (
    SELECT jsonb_agg(data)
    FROM (
        SELECT data FROM v_posts
        WHERE author_id = u.id
        ORDER BY published_at DESC  -- Use actual column for ordering
        LIMIT 10  -- Always limit aggregations
    ) p
)
```

### 5. Document View Purpose
```sql
-- View: v_dashboard_stats
-- Purpose: Provides aggregated statistics for user dashboard
-- Performance: Uses mv_user_statistics for fast aggregation
-- Refresh: Every 15 minutes via pg_cron
CREATE OR REPLACE VIEW v_dashboard_stats AS ...
```

## View Composition Patterns

### Layer Your Views
Build complex views by composing simpler ones:

```sql
-- Level 1: Base entity views
CREATE OR REPLACE VIEW v_user AS ...
CREATE OR REPLACE VIEW v_post AS ...

-- Level 2: Views with single relationships
CREATE OR REPLACE VIEW v_post_with_author AS
SELECT
    p.id,
    p.author_id,
    p.published_at,  -- Keep for sorting
    jsonb_build_object(
        '__typename', 'Post',
        'id', p.id,
        'title', p.title,
        'published_at', p.published_at,
        'author', a.data  -- Reuse v_users data
    ) AS data
FROM tb_posts p
LEFT JOIN v_user a ON a.id = p.author_id;

-- Level 3: Views with multiple relationships
CREATE OR REPLACE VIEW v_post_full AS
SELECT
    p.id,
    jsonb_build_object(
        '__typename', 'Post',
        'id', p.id,
        'title', p.title,
        'author', a.data,
        'comments', (
            SELECT jsonb_agg(c.data ORDER BY c.created_at DESC)
            FROM v_comment c
            WHERE c.post_id = p.id
        ),
        'tags', (
            SELECT jsonb_agg(t.data)
            FROM v_tag t
            JOIN tb_post_tags pt ON pt.tag_id = t.id
            WHERE pt.post_id = p.id
        )
    ) AS data
FROM tb_posts p
LEFT JOIN v_user a ON a.id = p.author_id;
```

## Testing Views

### Verify Structure
```sql
-- Test view returns expected structure
SELECT
    data->>'__typename' as type,
    jsonb_typeof(data->'posts') as posts_type,
    jsonb_array_length(data->'posts') as post_count
FROM v_user_with_post
WHERE id = 'test-user-id';
```

### Performance Testing
```sql
-- Analyze query performance
EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
SELECT * FROM v_post_with_author
WHERE is_published = true
ORDER BY published_at DESC
LIMIT 20;
```

### Data Integrity
```sql
-- Ensure all relationships resolve
SELECT COUNT(*) AS orphaned_posts
FROM v_post_with_author
WHERE data->'author' IS NULL;
```

## Case Conversion in FraiseQL

FraiseQL automatically handles case conversion between PostgreSQL (snake_case) and GraphQL (camelCase):

### Database Layer (snake_case)
```sql
-- Views store data in snake_case
CREATE OR REPLACE VIEW v_user AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'created_at', created_at,
        'is_active', is_active,
        'email_verified', email_verified
    ) AS data
FROM tb_users;
```

### GraphQL Response (camelCase)
```graphql
# Automatically converted by FraiseQL
{
  user {
    id
    createdAt      # created_at → createdAt
    isActive       # is_active → isActive
    emailVerified  # email_verified → emailVerified
  }
}
```

### Python Models
```python
@fraiseql.type
class User:
    id: str
    created_at: datetime  # Snake case in Python
    is_active: bool
    email_verified: bool

# GraphQL schema automatically uses camelCase
```

## Migration Strategies

### From ORM to Views
```sql
-- Step 1: Create view alongside ORM
CREATE OR REPLACE VIEW v_user AS
SELECT ... FROM tb_users;

-- Step 2: Test view performance
-- Step 3: Migrate queries gradually
-- Step 4: Remove ORM queries
```

### View Versioning
```sql
-- Maintain backward compatibility
CREATE OR REPLACE VIEW v_user_v2 AS ...

-- Deprecation notice in old view
COMMENT ON VIEW v_user IS
    'DEPRECATED: Use v_user_v2. Will be removed in version 2.0';
```

## Next Steps

- Explore the [Type System](./type-system.md) for defining GraphQL types
- Learn about [Query Translation](./query-translation.md) to understand how views are queried
- See practical examples in the [Blog API Tutorial](../tutorials/blog-api.md)
